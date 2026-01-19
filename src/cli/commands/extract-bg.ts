import { parseArgs } from 'util';
import { closeDatabase } from '../../db/database.js';
import { createEmbeddingServiceOptional } from '../../services/embedding/index.js';
import {
  clearAccumulator,
  createExtractionService,
  getAccumulator,
  setLastAssistantMessage,
} from '../../services/extraction/index.js';
import type { ExtractionTrigger } from '../../services/extraction/types.js';
import { getOrCreateSession } from '../../services/memory/sessions.js';
import { getOrCreateProject } from '../../services/project.js';
import { log } from '../../utils/log.js';

export async function extractBgCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      trigger: { type: 'string', short: 't', default: 'stop' },
      cwd: { type: 'string' },
      transcript: { type: 'string' },
    },
    allowPositionals: true,
  });

  const sessionId = positionals[0];
  if (!sessionId) {
    log.error('extract-bg', 'Session ID required');
    process.exit(1);
  }

  const cwd = values.cwd || process.cwd();
  const trigger = (values.trigger || 'stop') as ExtractionTrigger;

  log.info('extract-bg', 'Background extraction starting', { sessionId, trigger });

  try {
    const project = await getOrCreateProject(cwd);
    await getOrCreateSession(sessionId, project.id);

    const accumulator = await getAccumulator(sessionId);

    if (!accumulator || accumulator.toolCallCount === 0) {
      log.info('extract-bg', 'No work to extract', { sessionId });
      closeDatabase();
      process.exit(0);
    }

    if (values.transcript) {
      try {
        const content = await Bun.file(values.transcript).text();
        const lines = content.split('\n').filter(l => l.trim());

        for (let i = lines.length - 1; i >= 0; i--) {
          const line = lines[i];
          if (!line) continue;

          try {
            const entry = JSON.parse(line) as {
              type?: string;
              message?: {
                role?: string;
                content?: string | Array<{ type: string; text?: string }>;
              };
            };

            if (entry.type === 'assistant' && entry.message?.content) {
              const content = entry.message.content;
              const text = typeof content === 'string'
                ? content
                : Array.isArray(content)
                  ? content
                      .filter(c => c.type === 'text' && c.text)
                      .map(c => c.text)
                      .join('\n')
                  : '';

              if (text) {
                await setLastAssistantMessage(sessionId, text.slice(0, 10000));
                log.debug('extract-bg', 'Captured assistant message', {
                  length: text.length
                });
                break;
              }
            }
          } catch {
            continue;
          }
        }
      } catch (err) {
        log.debug('extract-bg', 'Could not read transcript', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }

    const embeddingService = await createEmbeddingServiceOptional();
    const extractionService = createExtractionService(embeddingService);

    await extractionService.extractSegment(accumulator, trigger);
    await clearAccumulator(sessionId);

    log.info('extract-bg', 'Background extraction complete', { sessionId });
  } catch (err) {
    log.error('extract-bg', 'Extraction failed', {
      sessionId,
      error: err instanceof Error ? err.message : String(err),
    });
  }

  closeDatabase();
  process.exit(0);
}
