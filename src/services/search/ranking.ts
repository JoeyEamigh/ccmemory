import type { Memory, MemorySector } from '../memory/types.js';

export type RankingWeights = {
  semantic: number;
  keyword: number;
  salience: number;
  recency: number;
  sectorBoost: Record<MemorySector, number>;
};

export const DEFAULT_WEIGHTS: RankingWeights = {
  semantic: 0.4,
  keyword: 0.25,
  salience: 0.2,
  recency: 0.15,
  sectorBoost: {
    reflective: 1.2,
    semantic: 1.1,
    procedural: 1.0,
    emotional: 0.9,
    episodic: 0.8,
  },
};

export function computeScore(
  memory: Memory,
  semanticSim: number,
  ftsRank: number,
  weights: RankingWeights = DEFAULT_WEIGHTS,
): number {
  const normalizedFTS = ftsRank ? Math.min(1, Math.abs(ftsRank) / 10) : 0;

  const daysSinceCreated = (Date.now() - memory.createdAt) / (1000 * 60 * 60 * 24);
  const recencyScore = Math.exp(-0.05 * daysSinceCreated);

  let score =
    weights.semantic * semanticSim +
    weights.keyword * normalizedFTS +
    weights.salience * memory.salience +
    weights.recency * recencyScore;

  score *= weights.sectorBoost[memory.sector] ?? 1.0;

  if (memory.validUntil) {
    score *= 0.5;
  }

  return Math.min(1, Math.max(0, score));
}

export function normalizeScores(results: Array<{ score: number }>): Array<{ score: number }> {
  if (results.length === 0) return results;

  const maxScore = Math.max(...results.map(r => r.score));
  if (maxScore === 0) return results;

  return results.map(r => ({
    ...r,
    score: r.score / maxScore,
  }));
}
