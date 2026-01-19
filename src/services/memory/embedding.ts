import { getDatabase } from '../../db/database.js';
import { log } from '../../utils/log.js';
import type { EmbeddingService } from '../embedding/types.js';

export async function embedMemory(
  memoryId: string,
  content: string,
  embeddingService: EmbeddingService,
): Promise<void> {
  const db = await getDatabase();
  const start = Date.now();

  try {
    const result = await embeddingService.embed(content);
    const modelId = embeddingService.getActiveModelId();
    const vectorBuffer = new Float32Array(result.vector).buffer;
    const now = Date.now();

    await db.execute(
      `INSERT INTO memory_vectors (memory_id, model_id, vector, dim, created_at)
       VALUES (?, ?, ?, ?, ?)
       ON CONFLICT (memory_id) DO UPDATE SET
         model_id = excluded.model_id,
         vector = excluded.vector,
         dim = excluded.dim,
         created_at = excluded.created_at`,
      [memoryId, modelId, new Uint8Array(vectorBuffer), result.dimensions, now],
    );

    log.debug('embedding', 'Memory embedded', {
      memoryId,
      model: modelId,
      dimensions: result.dimensions,
      ms: Date.now() - start,
    });
  } catch (err) {
    log.warn('embedding', 'Failed to embed memory', {
      memoryId,
      error: err instanceof Error ? err.message : String(err),
    });
  }
}

export async function embedMemoriesBatch(
  memories: Array<{ id: string; content: string }>,
  embeddingService: EmbeddingService,
): Promise<void> {
  if (memories.length === 0) return;

  const db = await getDatabase();
  const start = Date.now();

  try {
    const contents = memories.map(m => m.content);
    const results = await embeddingService.embedBatch(contents);
    const modelId = embeddingService.getActiveModelId();
    const now = Date.now();

    for (let i = 0; i < memories.length; i++) {
      const memory = memories[i];
      const result = results[i];
      if (!memory || !result) continue;

      const vectorBuffer = new Float32Array(result.vector).buffer;

      await db.execute(
        `INSERT INTO memory_vectors (memory_id, model_id, vector, dim, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (memory_id) DO UPDATE SET
           model_id = excluded.model_id,
           vector = excluded.vector,
           dim = excluded.dim,
           created_at = excluded.created_at`,
        [memory.id, modelId, new Uint8Array(vectorBuffer), result.dimensions, now],
      );
    }

    log.info('embedding', 'Batch embedded memories', {
      count: memories.length,
      model: modelId,
      ms: Date.now() - start,
    });
  } catch (err) {
    log.warn('embedding', 'Failed to batch embed memories', {
      count: memories.length,
      error: err instanceof Error ? err.message : String(err),
    });
  }
}
