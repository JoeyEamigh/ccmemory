import { log } from '../../utils/log.js';
import type { EmbeddingService } from '../embedding/types.js';
import { embedMemory } from '../memory/embedding.js';
import { MEMORY_TYPE_TO_SECTOR } from '../memory/types.js';
import { createMemoryStore } from '../memory/store.js';
import {
  clearAccumulator,
  getAccumulator,
  getOrCreateAccumulator,
  saveAccumulator,
  startNewSegment,
} from './accumulator.js';
import { extractMemories, saveExtractionSegment } from './extractor.js';
import { classifyUserSignal, isHighPrioritySignal } from './signal-detection.js';
import { detectAndHandleSuperseding } from './superseding.js';
import type {
  ExtractedMemory,
  ExtractionConfig,
  ExtractionTrigger,
  SegmentAccumulator,
  SignalClassification,
  SignalDetectionConfig,
  SupersedingConfig,
  UserPrompt,
} from './types.js';
import {
  DEFAULT_EXTRACTION_CONFIG,
  DEFAULT_SIGNAL_DETECTION_CONFIG,
  DEFAULT_SUPERSEDING_CONFIG,
} from './types.js';

export type ExtractionServiceConfig = {
  extraction: ExtractionConfig;
  signalDetection: SignalDetectionConfig;
  superseding: SupersedingConfig;
};

const DEFAULT_CONFIG: ExtractionServiceConfig = {
  extraction: DEFAULT_EXTRACTION_CONFIG,
  signalDetection: DEFAULT_SIGNAL_DETECTION_CONFIG,
  superseding: DEFAULT_SUPERSEDING_CONFIG,
};

export type ExtractionService = {
  classifySignal(prompt: string): Promise<SignalClassification | null>;
  extractSegment(
    accumulator: SegmentAccumulator,
    trigger: ExtractionTrigger,
    signal?: SignalClassification,
  ): Promise<ExtractedMemory[]>;
  startSegment(sessionId: string, projectId: string, initialPrompt?: UserPrompt): Promise<SegmentAccumulator>;
  getAccumulator(sessionId: string): Promise<SegmentAccumulator | null>;
  clearAccumulator(sessionId: string): Promise<void>;
};

export function createExtractionService(
  embeddingService: EmbeddingService | null,
  config: ExtractionServiceConfig = DEFAULT_CONFIG,
): ExtractionService {
  const store = createMemoryStore();

  return {
    async classifySignal(prompt: string): Promise<SignalClassification | null> {
      return classifyUserSignal(prompt, config.signalDetection);
    },

    async extractSegment(
      accumulator: SegmentAccumulator,
      trigger: ExtractionTrigger,
      signal?: SignalClassification,
    ): Promise<ExtractedMemory[]> {
      const start = Date.now();

      const memories = await extractMemories(accumulator, signal, config.extraction, trigger);

      if (memories.length === 0) {
        await saveExtractionSegment(accumulator, trigger, 0, Date.now() - start);
        return [];
      }

      const createdMemories: ExtractedMemory[] = [];

      for (const memory of memories) {
        const sector = MEMORY_TYPE_TO_SECTOR[memory.type];

        const created = await store.create(
          {
            content: memory.content,
            summary: memory.summary,
            concepts: memory.concepts,
            sector,
            tier: 'project',
            memoryType: memory.type,
            context: memory.context,
            confidence: memory.confidence,
            files: memory.relatedFiles,
            segmentId: accumulator.segmentId,
          },
          accumulator.projectId,
          accumulator.sessionId,
        );

        log.info('extraction', 'Memory created from extraction', {
          id: created.id,
          type: memory.type,
          summary: memory.summary?.slice(0, 100),
          concepts: memory.concepts?.length ?? 0,
          confidence: memory.confidence,
        });

        if (embeddingService) {
          await embedMemory(created.id, memory.content, embeddingService);
        }

        if (memory.confidence >= config.superseding.confidenceThreshold) {
          await detectAndHandleSuperseding(
            memory,
            created.id,
            accumulator.projectId,
            embeddingService,
            config.superseding,
          );
        }

        createdMemories.push(memory);
      }

      await saveExtractionSegment(accumulator, trigger, memories.length, Date.now() - start);

      return createdMemories;
    },

    async startSegment(
      sessionId: string,
      projectId: string,
      initialPrompt?: UserPrompt,
    ): Promise<SegmentAccumulator> {
      return startNewSegment(sessionId, projectId, initialPrompt);
    },

    async getAccumulator(sessionId: string): Promise<SegmentAccumulator | null> {
      return getAccumulator(sessionId);
    },

    async clearAccumulator(sessionId: string): Promise<void> {
      return clearAccumulator(sessionId);
    },
  };
}

export {
  addCommand,
  addCompletedTask,
  addError,
  addFileModified,
  addFileRead,
  addSearch,
  addUserPrompt,
  clearAccumulator,
  getAccumulator,
  getOrCreateAccumulator,
  incrementToolCallCount,
  setLastAssistantMessage,
} from './accumulator.js';

export { classifyUserSignal, isHighPrioritySignal } from './signal-detection.js';

export type {
  CommandSummary,
  CompletedTask,
  ErrorSummary,
  ExtractedMemory,
  ExtractionConfig,
  ExtractionResponse,
  ExtractionTrigger,
  SearchSummary,
  SegmentAccumulator,
  SignalCategory,
  SignalClassification,
  SignalDetectionConfig,
  SupersedingCheckResult,
  SupersedingConfig,
  UserPrompt,
} from './types.js';
