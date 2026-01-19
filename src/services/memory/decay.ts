import { getDatabase } from '../../db/database.js';
import { log } from '../../utils/log.js';
import type { Memory, MemorySector } from './types.js';
import { SECTOR_DECAY_RATES } from './types.js';
import { rowToMemory } from './utils.js';

export type DecayConfig = {
  enabled: boolean;
  interval: number;
  batchSize: number;
};

const DEFAULT_DECAY_CONFIG: DecayConfig = {
  enabled: true,
  interval: 60 * 60 * 1000,
  batchSize: 100,
};

export function calculateDecay(memory: Memory): number {
  const daysSinceAccess = (Date.now() - memory.lastAccessed) / (1000 * 60 * 60 * 24);
  const decayRate = SECTOR_DECAY_RATES[memory.sector];

  const effectiveDecayRate = decayRate / (memory.importance + 0.1);
  const decayed = memory.salience * Math.exp(-effectiveDecayRate * daysSinceAccess);

  const accessProtection = Math.min(0.1, Math.log1p(memory.accessCount) * 0.02);

  const finalSalience = Math.max(0.05, Math.min(1.0, decayed + accessProtection));

  return finalSalience;
}

export function calculateSalienceBoost(currentSalience: number, amount: number): number {
  const boosted = currentSalience + amount * (1.0 - currentSalience);
  return Math.min(1.0, boosted);
}

export async function applyDecay(memories: Memory[]): Promise<void> {
  if (memories.length === 0) return;

  const db = await getDatabase();
  const now = Date.now();

  log.debug('decay', 'Applying decay', { count: memories.length });

  const statements = memories.map(memory => {
    const newSalience = calculateDecay(memory);
    return {
      sql: `UPDATE memories SET salience = ?, updated_at = ? WHERE id = ?`,
      args: [newSalience, now, memory.id],
    };
  });

  await db.batch(statements);

  log.info('decay', 'Decay applied', { count: memories.length });
}

export async function getMemoriesForDecay(batchSize: number): Promise<Memory[]> {
  const db = await getDatabase();

  const result = await db.execute(
    `SELECT * FROM memories
     WHERE salience > 0.05 AND is_deleted = 0
     ORDER BY updated_at ASC
     LIMIT ?`,
    [batchSize],
  );

  return result.rows.map(rowToMemory);
}

export function startDecayProcess(config: Partial<DecayConfig> = {}): () => void {
  const finalConfig: DecayConfig = { ...DEFAULT_DECAY_CONFIG, ...config };

  if (!finalConfig.enabled) {
    log.info('decay', 'Decay process disabled');
    return () => {};
  }

  log.info('decay', 'Starting decay process', {
    interval: finalConfig.interval,
    batchSize: finalConfig.batchSize,
  });

  let stopped = false;

  const runDecay = async (): Promise<void> => {
    if (stopped) return;

    try {
      const memories = await getMemoriesForDecay(finalConfig.batchSize);
      if (memories.length > 0) {
        await applyDecay(memories);
      }
    } catch (error) {
      log.error('decay', 'Decay process error', {
        error: error instanceof Error ? error.message : String(error),
      });
    }
  };

  runDecay();

  const interval = setInterval(runDecay, finalConfig.interval);

  return () => {
    stopped = true;
    clearInterval(interval);
    log.info('decay', 'Decay process stopped');
  };
}

export function getDecayRateForSector(sector: MemorySector): number {
  return SECTOR_DECAY_RATES[sector];
}

export function estimateTimeToDecay(memory: Memory, targetSalience: number): number {
  if (memory.salience <= targetSalience) {
    return 0;
  }

  const decayRate = SECTOR_DECAY_RATES[memory.sector];
  const effectiveDecayRate = decayRate / (memory.importance + 0.1);

  const accessProtection = Math.min(0.1, Math.log1p(memory.accessCount) * 0.02);
  const adjustedTarget = targetSalience - accessProtection;

  if (adjustedTarget <= 0) {
    return Infinity;
  }

  const daysToDecay = Math.log(memory.salience / adjustedTarget) / effectiveDecayRate;

  return daysToDecay * 24 * 60 * 60 * 1000;
}
