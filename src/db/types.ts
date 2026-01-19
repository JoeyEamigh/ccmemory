export type MemorySector = 'episodic' | 'semantic' | 'procedural' | 'emotional' | 'reflective';

export type MemoryTier = 'session' | 'project';

export type MemoryType = 'preference' | 'codebase' | 'decision' | 'gotcha' | 'pattern';

export type ExtractionTrigger = 'user_prompt' | 'pre_compact' | 'stop';

export type RelationshipType =
  | 'SUPERSEDES'
  | 'CONTRADICTS'
  | 'RELATED_TO'
  | 'BUILDS_ON'
  | 'CONFIRMS'
  | 'APPLIES_TO'
  | 'DEPENDS_ON'
  | 'ALTERNATIVE_TO';

export type UsageType = 'created' | 'recalled' | 'updated' | 'reinforced';

export type EntityType = 'person' | 'project' | 'technology' | 'concept' | 'file' | 'error';

export type ExtractedBy = 'user' | 'llm' | 'system';

export type DocumentSourceType = 'txt' | 'md' | 'url';
