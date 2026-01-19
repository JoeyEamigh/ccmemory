import type { MemoryType } from '../memory/types.js';

export type SignalCategory = 'correction' | 'preference' | 'context' | 'task' | 'question' | 'feedback';

export type SignalClassification = {
  category: SignalCategory;
  extractable: boolean;
  summary: string | null;
};

export type UserPrompt = {
  content: string;
  timestamp: number;
  signal?: SignalClassification;
};

export type CommandSummary = {
  command: string;
  exitCode?: number;
  hasError: boolean;
};

export type ErrorSummary = {
  source: string;
  message: string;
};

export type SearchSummary = {
  tool: 'Grep' | 'Glob';
  pattern: string;
  resultCount: number;
};

export type CompletedTask = {
  content: string;
  timestamp: number;
};

export type SegmentAccumulator = {
  sessionId: string;
  projectId: string;
  segmentId: string;
  segmentStart: number;
  userPrompts: UserPrompt[];
  filesRead: string[];
  filesModified: string[];
  commandsRun: CommandSummary[];
  errorsEncountered: ErrorSummary[];
  searchesPerformed: SearchSummary[];
  completedTasks: CompletedTask[];
  lastAssistantMessage?: string;
  toolCallCount: number;
};

export type ExtractionTrigger = 'user_prompt' | 'pre_compact' | 'stop' | 'todo_completion';

export type ExtractedMemory = {
  type: MemoryType;
  summary: string;
  content: string;
  context: string;
  concepts: string[];
  confidence: number;
  relatedFiles: string[];
};

export type ExtractionResponse = ExtractedMemory[];

export type SupersedingCheckResult = {
  supersedes: boolean;
  reason: string;
};

export type ExtractionConfig = {
  model: string;
  maxTokens: number;
  minToolCallsToExtract: number;
};

export type SignalDetectionConfig = {
  model: string;
  maxTokens: number;
};

export type SupersedingConfig = {
  model: string;
  similarityThreshold: number;
  confidenceThreshold: number;
};

export type ExtractionSegmentRecord = {
  id: string;
  sessionId: string;
  projectId: string;
  trigger: ExtractionTrigger;
  userPrompts: UserPrompt[];
  filesRead: string[];
  filesModified: string[];
  toolCallCount: number;
  memoriesExtracted: number;
  extractionTokens?: number;
  segmentStart: number;
  segmentEnd: number;
  extractionDurationMs?: number;
  createdAt: number;
};

export const DEFAULT_EXTRACTION_CONFIG: ExtractionConfig = {
  model: 'sonnet',
  maxTokens: 32768,
  minToolCallsToExtract: 3,
};

export const DEFAULT_SIGNAL_DETECTION_CONFIG: SignalDetectionConfig = {
  model: 'haiku',
  maxTokens: 150,
};

export const DEFAULT_SUPERSEDING_CONFIG: SupersedingConfig = {
  model: 'haiku',
  similarityThreshold: 0.7,
  confidenceThreshold: 0.7,
};

export const ACCUMULATOR_LIMITS = {
  maxFilesTracked: 100,
  maxCommandsTracked: 50,
  maxErrorsTracked: 20,
  maxSearchesTracked: 50,
  maxTasksTracked: 50,
};
