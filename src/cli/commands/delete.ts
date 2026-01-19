import { parseArgs } from 'util';
import { createMemoryStore } from '../../services/memory/store.js';
import { log } from '../../utils/log.js';

export async function deleteCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      force: { type: 'boolean', short: 'f' },
      hard: { type: 'boolean' },
    },
    allowPositionals: true,
  });

  const id = positionals[0];
  if (!id) {
    console.error('Usage: ccmemory delete <id> [--force] [--hard]');
    process.exit(1);
  }

  log.debug('cli', 'Delete command', {
    id,
    force: values.force,
    hard: values.hard,
  });

  const store = createMemoryStore();
  const memory = await store.get(id);

  if (!memory) {
    log.warn('cli', 'Memory not found', { id });
    console.error(`Memory not found: ${id}`);
    process.exit(1);
  }

  if (!values.force) {
    console.log(`Memory to delete:`);
    console.log(`  ID: ${memory.id}`);
    console.log(`  Sector: ${memory.sector}`);
    console.log(`  Content: ${memory.content.slice(0, 100)}...`);
    console.log();
    console.log(
      values.hard ? 'This will PERMANENTLY delete the memory.' : 'This will soft-delete the memory (can be restored).',
    );
    console.log('Run with --force to confirm.');
    return;
  }

  await store.delete(id, values.hard ?? false);

  log.info('cli', 'Memory deleted', { id, hard: values.hard });
  console.log(values.hard ? `Memory permanently deleted: ${id}` : `Memory soft-deleted: ${id}`);
}

export async function archiveCommand(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: {
      before: { type: 'string' },
      'dry-run': { type: 'boolean' },
      threshold: { type: 'string', default: '0.2' },
    },
  });

  log.debug('cli', 'Archive command', {
    before: values.before,
    dryRun: values['dry-run'],
    threshold: values.threshold,
  });

  const store = createMemoryStore();
  const threshold = parseFloat(values.threshold as string);

  const beforeDate = values.before ? new Date(values.before).getTime() : Date.now() - 30 * 24 * 60 * 60 * 1000;

  const memories = await store.list({
    orderBy: 'salience',
    order: 'asc',
    limit: 1000,
  });

  const toArchive = memories.filter(m => m.salience < threshold && m.createdAt < beforeDate);

  if (values['dry-run']) {
    console.log(`Would archive ${toArchive.length} memories:`);
    for (const mem of toArchive.slice(0, 10)) {
      console.log(
        `  ${mem.id} (salience: ${mem.salience.toFixed(2)}, created: ${new Date(mem.createdAt).toLocaleDateString()})`,
      );
    }
    if (toArchive.length > 10) {
      console.log(`  ... and ${toArchive.length - 10} more`);
    }
    return;
  }

  let archived = 0;
  for (const mem of toArchive) {
    await store.delete(mem.id, false);
    archived++;
  }

  log.info('cli', 'Memories archived', { count: archived });
  console.log(`Archived ${archived} memories`);
}
