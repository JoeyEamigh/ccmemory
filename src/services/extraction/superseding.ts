import { log } from '../../utils/log.js';
import type { EmbeddingService } from '../embedding/types.js';
import { supersede } from '../memory/relationships.js';
import { createMemoryStore } from '../memory/store.js';
import type { Memory } from '../memory/types.js';
import { makeAnthropicCall, parseJsonFromResponse } from './inference.js';
import type { ExtractedMemory, SupersedingCheckResult, SupersedingConfig } from './types.js';
import { DEFAULT_SUPERSEDING_CONFIG } from './types.js';

const SUPERSEDING_SYSTEM = `You are checking if a new memory supersedes (replaces/updates) an existing memory in a knowledge base.`;

function buildSupersedingPrompt(existing: Memory, newMemory: ExtractedMemory): string {
  return `**Existing Memory:**
Type: ${existing.memoryType ?? existing.sector}
Content: ${existing.content}
Created: ${new Date(existing.createdAt).toISOString()}

**New Memory:**
Type: ${newMemory.type}
Content: ${newMemory.content}

**Question:** Does the new memory supersede (replace, update, or contradict) the existing memory?

Respond with JSON:
\`\`\`json
{
  "supersedes": true|false,
  "reason": "Brief explanation"
}
\`\`\`

**Guidelines:**
- Supersedes if: new memory contradicts, updates, or refines the existing one
- Does NOT supersede if: memories are about different topics, or complementary
- When in doubt, return false (keep both memories)`;
}

async function findSimilarMemories(
  newMemory: ExtractedMemory,
  projectId: string,
  embeddingService: EmbeddingService | null,
  config: SupersedingConfig,
): Promise<Memory[]> {
  const store = createMemoryStore();

  const memories = await store.list({
    projectId,
    memoryType: newMemory.type,
    minSalience: 0.3,
    limit: 20,
    orderBy: 'created_at',
    order: 'desc',
  });

  if (!embeddingService) {
    return memories.filter(m => {
      const contentWords = newMemory.content.toLowerCase().split(/\s+/);
      const existingWords = m.content.toLowerCase().split(/\s+/);
      const commonWords = contentWords.filter(w => existingWords.includes(w) && w.length > 3);
      return commonWords.length >= 3;
    });
  }

  const newEmbedding = await embeddingService.embed(newMemory.content);
  const candidates: Array<{ memory: Memory; similarity: number }> = [];

  for (const memory of memories) {
    const existingEmbedding = await embeddingService.embed(memory.content);

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;
    for (let i = 0; i < newEmbedding.vector.length; i++) {
      const a = newEmbedding.vector[i] ?? 0;
      const b = existingEmbedding.vector[i] ?? 0;
      dotProduct += a * b;
      normA += a * a;
      normB += b * b;
    }
    const similarity = dotProduct / (Math.sqrt(normA) * Math.sqrt(normB) || 1);

    if (similarity >= config.similarityThreshold) {
      candidates.push({ memory, similarity });
    }
  }

  candidates.sort((a, b) => b.similarity - a.similarity);

  return candidates.slice(0, 5).map(c => c.memory);
}

async function checkSuperseding(
  existing: Memory,
  newMemory: ExtractedMemory,
  config: SupersedingConfig,
): Promise<SupersedingCheckResult> {
  const response = await makeAnthropicCall({
    model: config.model,
    max_tokens: 200,
    temperature: 0.0,
    system: SUPERSEDING_SYSTEM,
    messages: [
      {
        role: 'user',
        content: buildSupersedingPrompt(existing, newMemory),
      },
    ],
  });

  if (!response) {
    return { supersedes: false, reason: 'No API response' };
  }

  const result = parseJsonFromResponse<SupersedingCheckResult>(response);

  if (!result) {
    return { supersedes: false, reason: 'Failed to parse response' };
  }

  return {
    supersedes: Boolean(result.supersedes),
    reason: typeof result.reason === 'string' ? result.reason : 'Unknown',
  };
}

export async function detectAndHandleSuperseding(
  newMemory: ExtractedMemory,
  newMemoryId: string,
  projectId: string,
  embeddingService: EmbeddingService | null,
  config: SupersedingConfig = DEFAULT_SUPERSEDING_CONFIG,
): Promise<string[]> {
  if (newMemory.confidence < config.confidenceThreshold) {
    log.debug('superseding', 'Skipping superseding check - low confidence', {
      confidence: newMemory.confidence,
      threshold: config.confidenceThreshold,
    });
    return [];
  }

  const start = Date.now();
  const supersededIds: string[] = [];

  log.debug('superseding', 'Checking for superseding', {
    memoryType: newMemory.type,
    content: newMemory.content.slice(0, 100),
  });

  const candidates = await findSimilarMemories(newMemory, projectId, embeddingService, config);

  if (candidates.length === 0) {
    log.debug('superseding', 'No similar memories found');
    return [];
  }

  log.debug('superseding', 'Found candidates for superseding check', {
    count: candidates.length,
  });

  for (let i = 0; i < candidates.length; i++) {
    const existing = candidates[i];
    if (!existing) continue;

    if (i > 0) {
      await new Promise(resolve => setTimeout(resolve, 500));
    }

    const result = await checkSuperseding(existing, newMemory, config);

    if (result.supersedes) {
      log.info('superseding', 'Memory supersedes existing', {
        newId: newMemoryId,
        oldId: existing.id,
        reason: result.reason,
      });

      await supersede(existing.id, newMemoryId);
      supersededIds.push(existing.id);
    }
  }

  log.info('superseding', 'Superseding check complete', {
    candidates: candidates.length,
    superseded: supersededIds.length,
    ms: Date.now() - start,
  });

  return supersededIds;
}
