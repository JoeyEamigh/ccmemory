export type MemorySector = "episodic" | "semantic" | "procedural" | "emotional" | "reflective";

export type MemoryTier = "session" | "project" | "global";

export type Memory = {
  id: string;
  projectId: string;
  content: string;
  summary?: string;
  contentHash?: string;

  sector: MemorySector;
  tier: MemoryTier;
  importance: number;
  categories: string[];

  simhash?: string;

  salience: number;
  accessCount: number;

  createdAt: number;
  updatedAt: number;
  lastAccessed: number;
  validFrom?: number;
  validUntil?: number;

  isDeleted: boolean;
  deletedAt?: number;

  embeddingModelId?: string;
  tags: string[];
  concepts: string[];
  files: string[];
};

export type MemoryInput = {
  content: string;
  sector?: MemorySector;
  tier?: MemoryTier;
  importance?: number;
  tags?: string[];
  files?: string[];
  validFrom?: number;
};

export type ListOptions = {
  projectId?: string;
  limit?: number;
  offset?: number;
  sector?: MemorySector;
  tier?: MemoryTier;
  minSalience?: number;
  includeDeleted?: boolean;
  orderBy?: "created_at" | "salience" | "last_accessed";
  order?: "asc" | "desc";
};

export type UsageType = "created" | "recalled" | "updated" | "reinforced";

export const SECTOR_PATTERNS: Record<MemorySector, RegExp[]> = {
  emotional: [
    /\b(frustrated|annoyed|happy|satisfied|confused|angry|upset)\b/i,
    /\b(love|hate|prefer|dislike)\b/i,
    /\b(pain point|struggle|enjoy|frustrating)\b/i,
    /\b(feel|feeling|feels)\b/i,
  ],
  reflective: [
    /\b(learned|realized|noticed|insight|pattern)\b/i,
    /\b(better to|should have|next time)\b/i,
    /\b(observation|conclusion|takeaway)\b/i,
    /\b(this codebase|this project|in general)\b/i,
  ],
  episodic: [
    /\b(asked|said|mentioned|discussed|talked about)\b/i,
    /\b(session|conversation|earlier|just now)\b/i,
    /\b(user (wanted|requested|asked for))\b/i,
  ],
  procedural: [
    /\b(how to|steps to|process for|workflow|procedure)\b/i,
    /\b(first|then|next|finally|step \d+)\b/i,
    /\b(run the|execute the|to build|to deploy|to test)\b/i,
    /\b(command:|script:|recipe)\b/i,
  ],
  semantic: [
    /\b(is located|are located|was located|were located)\b/i,
    /\b(located (at|in)|defined in|implemented in)\b/i,
    /\b(file|function|class|module|component|endpoint)\b/i,
    /\b(fact|information|knowledge)\b/i,
    /\b(has|have|contains|returns)\b/i,
  ],
};

export const SECTOR_DECAY_RATES: Record<MemorySector, number> = {
  emotional: 0.003,
  semantic: 0.005,
  reflective: 0.008,
  procedural: 0.01,
  episodic: 0.02,
};

export const ALL_SECTORS: MemorySector[] = [
  "episodic",
  "semantic",
  "procedural",
  "emotional",
  "reflective",
];

const SECTOR_PRIORITY: MemorySector[] = [
  "emotional",
  "reflective",
  "episodic",
  "procedural",
  "semantic",
];

export function classifyMemorySector(content: string): MemorySector {
  const scores: Record<MemorySector, number> = {
    episodic: 0,
    semantic: 0,
    procedural: 0,
    emotional: 0,
    reflective: 0,
  };

  for (const sector of ALL_SECTORS) {
    const patterns = SECTOR_PATTERNS[sector];
    for (const pattern of patterns) {
      const matches = content.match(new RegExp(pattern.source, "gi"));
      if (matches) {
        scores[sector] += matches.length;
      }
    }
  }

  let maxSector: MemorySector = "semantic";
  let maxScore = 0;

  for (const sector of SECTOR_PRIORITY) {
    if (scores[sector] > maxScore) {
      maxScore = scores[sector];
      maxSector = sector;
    }
  }

  return maxSector;
}

export function isValidSector(sector: string): sector is MemorySector {
  return ALL_SECTORS.includes(sector as MemorySector);
}

export function isValidTier(tier: string): tier is MemoryTier {
  return tier === "session" || tier === "project" || tier === "global";
}
