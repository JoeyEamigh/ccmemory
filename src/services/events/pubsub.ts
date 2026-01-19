import { join } from 'node:path';
import { log } from '../../utils/log.js';
import { getPaths } from '../../utils/paths.js';

export type MemoryEvent = {
  type: 'memory:created' | 'memory:updated' | 'memory:deleted' | 'memory:reinforced';
  memoryId: string;
  projectId: string;
  timestamp: number;
};

function getEventsDir(): string {
  const xdgRuntime = process.env['XDG_RUNTIME_DIR'];
  if (xdgRuntime) {
    return join(xdgRuntime, 'ccmemory', 'events');
  }
  const paths = getPaths();
  return join(paths.cache, 'runtime', 'events');
}

const EVENTS_DIR = getEventsDir();
const MAX_EVENT_AGE_MS = 30000;

export async function publishEvent(event: MemoryEvent): Promise<void> {
  try {
    await Bun.$`mkdir -p ${EVENTS_DIR}`.quiet();

    const filename = `${event.timestamp}-${event.memoryId.slice(0, 8)}.json`;
    const filepath = join(EVENTS_DIR, filename);

    await Bun.write(filepath, JSON.stringify(event));

    log.debug('pubsub', 'Event published', {
      type: event.type,
      memoryId: event.memoryId,
    });
  } catch (err) {
    log.debug('pubsub', 'Failed to publish event', {
      error: err instanceof Error ? err.message : String(err),
    });
  }
}

export async function consumeEvents(): Promise<MemoryEvent[]> {
  try {
    const eventsDir = Bun.file(EVENTS_DIR);
    if (!(await eventsDir.exists())) {
      return [];
    }

    const now = Date.now();
    const events: MemoryEvent[] = [];
    const toDelete: string[] = [];

    const glob = new Bun.Glob('*.json');
    for await (const filename of glob.scan(EVENTS_DIR)) {
      const filepath = join(EVENTS_DIR, filename);
      try {
        const content = await Bun.file(filepath).text();
        const event = JSON.parse(content) as MemoryEvent;

        if (now - event.timestamp < MAX_EVENT_AGE_MS) {
          events.push(event);
        }
        toDelete.push(filepath);
      } catch {
        toDelete.push(filepath);
      }
    }

    for (const filepath of toDelete) {
      try {
        await Bun.$`rm -f ${filepath}`.quiet();
      } catch {
        // Ignore cleanup errors
      }
    }

    if (events.length > 0) {
      log.debug('pubsub', 'Events consumed', { count: events.length });
    }

    return events.sort((a, b) => a.timestamp - b.timestamp);
  } catch (err) {
    log.debug('pubsub', 'Failed to consume events', {
      error: err instanceof Error ? err.message : String(err),
    });
    return [];
  }
}

export async function cleanupOldEvents(): Promise<void> {
  try {
    const eventsDir = Bun.file(EVENTS_DIR);
    if (!(await eventsDir.exists())) {
      return;
    }

    const now = Date.now();
    const glob = new Bun.Glob('*.json');

    for await (const filename of glob.scan(EVENTS_DIR)) {
      const filepath = join(EVENTS_DIR, filename);
      try {
        const content = await Bun.file(filepath).text();
        const event = JSON.parse(content) as MemoryEvent;

        if (now - event.timestamp > MAX_EVENT_AGE_MS) {
          await Bun.$`rm -f ${filepath}`.quiet();
        }
      } catch {
        await Bun.$`rm -f ${filepath}`.quiet();
      }
    }
  } catch {
    // Ignore cleanup errors
  }
}
