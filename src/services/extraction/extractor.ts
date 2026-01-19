import { getDatabase } from '../../db/database.js';
import { log } from '../../utils/log.js';
import { MEMORY_TYPE_TO_SECTOR, type MemoryType } from '../memory/types.js';
import { makeAnthropicCall, parseJsonFromResponse } from './inference.js';
import type {
  ExtractedMemory,
  ExtractionConfig,
  ExtractionResponse,
  ExtractionTrigger,
  SegmentAccumulator,
  SignalClassification,
} from './types.js';
import { DEFAULT_EXTRACTION_CONFIG } from './types.js';

const EXTRACTION_SYSTEM = `You are a memory extraction agent. Your job is to extract USEFUL insights from a Claude Code work segment that would help in FUTURE sessions.`;

const TODO_COMPLETION_SYSTEM = `You are a memory extraction agent focused on capturing brief task completion records. Keep extractions minimal and action-focused.`;

function buildExtractionPrompt(
  accumulator: SegmentAccumulator,
  signal?: SignalClassification,
): string {
  const lines: string[] = [];

  lines.push('## Work Segment');
  lines.push('');

  if (accumulator.userPrompts.length > 0) {
    lines.push('**User Prompts:**');
    for (const prompt of accumulator.userPrompts) {
      lines.push(`- ${prompt.content.slice(0, 500)}${prompt.content.length > 500 ? '...' : ''}`);
    }
    lines.push('');
  }

  if (accumulator.filesRead.length > 0) {
    lines.push(`**Files Read:** ${accumulator.filesRead.slice(0, 20).join(', ')}${accumulator.filesRead.length > 20 ? '...' : ''}`);
  }

  if (accumulator.filesModified.length > 0) {
    lines.push(`**Files Modified:** ${accumulator.filesModified.slice(0, 20).join(', ')}${accumulator.filesModified.length > 20 ? '...' : ''}`);
  }

  if (accumulator.commandsRun.length > 0) {
    lines.push('**Commands Run:**');
    for (const cmd of accumulator.commandsRun.slice(0, 10)) {
      const status = cmd.hasError ? '(error)' : '(ok)';
      lines.push(`- ${cmd.command.slice(0, 100)} ${status}`);
    }
  }

  if (accumulator.errorsEncountered.length > 0) {
    lines.push('**Errors Encountered:**');
    for (const err of accumulator.errorsEncountered.slice(0, 5)) {
      lines.push(`- [${err.source}] ${err.message.slice(0, 200)}`);
    }
  }

  if (accumulator.completedTasks.length > 0) {
    lines.push('**Tasks Completed:**');
    for (const task of accumulator.completedTasks.slice(0, 20)) {
      lines.push(`- ${task.content.slice(0, 200)}`);
    }
    lines.push('');
  }

  if (accumulator.lastAssistantMessage) {
    lines.push('');
    lines.push('**Claude\'s Response:**');
    lines.push(accumulator.lastAssistantMessage.slice(0, 2000));
  }

  lines.push('');

  if (signal?.extractable && signal.summary) {
    lines.push('**User Signal Detected:**');
    lines.push(`Category: ${signal.category}`);
    lines.push(`Summary: ${signal.summary}`);
    lines.push('');
    lines.push('This should likely become a memory. Extract it.');
    lines.push('');
  }

  lines.push(`## Instructions

Extract memories that would help in FUTURE sessions. Be generous - it's better to capture something that might be useful than to lose it.

## Memory Types

### Discrete Knowledge (extract 0-10 per session):

- \`preference\`: User preference, style choice, or correction (HIGHEST PRIORITY)
  - "User prefers spaces over tabs"
  - "User dislikes excessive comments, prefers self-documenting code"
  - "User corrected: use Bun not Node for this project"

- \`codebase\`: How the codebase works - architecture, structure, conventions, file locations
  - "Authentication middleware is in src/middleware/auth.ts and uses JWT"
  - "The project uses a monorepo with packages/ for shared code"
  - Can be LONG - include all relevant details for complex systems

- \`decision\`: Architectural decision with rationale (the WHY matters most)
  - "Chose SQLite over Postgres because the app runs locally and simplicity > scale"
  - "Using SSR instead of SPA for SEO requirements"

- \`gotcha\`: Pitfall, gotcha, or thing that caused confusion/errors
  - "Must run prebuild before build or types won't be generated"
  - "The API returns 200 even on validation errors - check response.success"

- \`pattern\`: Reusable pattern, convention, or workflow
  - "Error handling pattern: wrap in try/catch, log with context, return typed Result"
  - "Testing convention: colocate tests in __test__ folders"

### Turn Summary (always include ONE if meaningful work was done):

- \`turn_summary\`: Comprehensive narrative of what happened this turn
  - Include: what was accomplished, key decisions made, problems encountered
  - Purpose: provides context when resuming work later
  - Should be 2-5 paragraphs summarizing the turn's work
  - Has regular decay - good for longer-term context

### Task Completion (for each meaningful completed task):

- \`task_completion\`: Brief record of a completed task
  - Should be 1-2 sentences capturing WHAT was done
  - Decays faster - recent context for near-term recall
  - Extract if a task was completed this turn
  - Keep it brief and action-focused

## What to ALWAYS Extract:
- User corrections or stated preferences (even if implicit)
- Discoveries about codebase that weren't obvious
- Decisions made and WHY (rationale is gold)
- Errors/confusion that took time to resolve
- Patterns you'll want to follow again
- Turn summary if any real work was done
- Task completions for each meaningful completed task

## What to SKIP:
- Routine reads/writes without insight
- Standard library usage everyone knows
- Temporary debugging that's now resolved
- Single-use information

## Output Format

Return JSON array:

\`\`\`json
[
  {
    "type": "preference|codebase|decision|gotcha|pattern|turn_summary|task_completion",
    "summary": "Brief searchable title (1-2 sentences)",
    "content": "Full explanation - BE DETAILED for complex topics. Multiple paragraphs OK.",
    "context": "How this was discovered",
    "concepts": ["keyword1", "keyword2"],
    "confidence": 0.0-1.0,
    "relatedFiles": ["path/to/file.ts"]
  }
]
\`\`\`

**Guidelines:**
- \`content\` should be SELF-CONTAINED and useful without other context
- For \`codebase\` and \`turn_summary\`: err on the side of MORE detail
- \`confidence\`: user stated = 1.0, inferred = 0.6-0.8, guessed = 0.3-0.5
- \`concepts\`: ALWAYS include 2-5 keywords. Include technology names (TypeScript, React, SQLite), domains (authentication, memory, search), and project-specific terms (ccmemory, extraction, embedding). These are used for search.
- Include a \`turn_summary\` if ANY meaningful work was done
- For complex architecture, write multiple paragraphs - future you will thank you`);

  return lines.join('\n');
}

function buildTodoCompletionPrompt(accumulator: SegmentAccumulator): string {
  const lines: string[] = [];

  lines.push('## Completed Tasks');
  lines.push('');

  if (accumulator.completedTasks.length > 0) {
    for (const task of accumulator.completedTasks.slice(0, 10)) {
      lines.push(`- ${task.content}`);
    }
    lines.push('');
  }

  if (accumulator.filesModified.length > 0) {
    lines.push(`**Files Modified:** ${accumulator.filesModified.slice(0, 10).join(', ')}`);
    lines.push('');
  }

  if (accumulator.errorsEncountered.length > 0) {
    lines.push('**Issues Encountered:**');
    for (const err of accumulator.errorsEncountered.slice(0, 3)) {
      lines.push(`- [${err.source}] ${err.message.slice(0, 100)}`);
    }
    lines.push('');
  }

  lines.push(`## Instructions

Extract ONLY task_completion memories - brief records of what was accomplished. This is for intermediate progress tracking, NOT end-of-turn summaries.

Guidelines:
- Extract 0-3 task_completion memories max
- Each should be 1-2 sentences
- Focus on WHAT was done, not HOW
- Only extract if genuinely useful to remember
- Skip routine/obvious completions

Output JSON array:

\`\`\`json
[
  {
    "type": "task_completion",
    "summary": "Brief description of completed task",
    "content": "What was accomplished in 1-2 sentences",
    "context": "Brief note on what triggered this",
    "concepts": ["keyword1", "keyword2"],
    "confidence": 0.6-0.9,
    "relatedFiles": ["path/to/file.ts"]
  }
]
\`\`\`

Return empty array [] if nothing worth extracting.`);

  return lines.join('\n');
}

type RawExtractedMemory = {
  type: string;
  summary: string;
  content: string;
  context: string;
  concepts: string[];
  confidence: number;
  relatedFiles: string[];
};

function validateMemory(raw: RawExtractedMemory): ExtractedMemory | null {
  const validTypes = ['preference', 'codebase', 'decision', 'gotcha', 'pattern', 'turn_summary', 'task_completion'];

  if (!validTypes.includes(raw.type)) {
    log.warn('extractor', 'Invalid memory type', { type: raw.type });
    return null;
  }

  if (!raw.content || typeof raw.content !== 'string' || raw.content.length < 5) {
    log.warn('extractor', 'Invalid memory content', { content: raw.content?.slice(0, 50) });
    return null;
  }

  const confidence = typeof raw.confidence === 'number' ? Math.max(0, Math.min(1, raw.confidence)) : 0.5;

  const relatedFiles = Array.isArray(raw.relatedFiles)
    ? raw.relatedFiles.filter((f): f is string => typeof f === 'string').slice(0, 10)
    : [];

  const concepts = Array.isArray(raw.concepts)
    ? raw.concepts.filter((c): c is string => typeof c === 'string' && c.length > 0).slice(0, 20)
    : [];

  const summary = typeof raw.summary === 'string' && raw.summary.length > 0
    ? raw.summary.slice(0, 500)
    : raw.content.slice(0, 200);

  return {
    type: raw.type as MemoryType,
    summary,
    content: raw.content.slice(0, 32000),
    context: typeof raw.context === 'string' ? raw.context.slice(0, 1000) : '',
    concepts,
    confidence,
    relatedFiles,
  };
}

export async function extractMemories(
  accumulator: SegmentAccumulator,
  signal?: SignalClassification,
  config: ExtractionConfig = DEFAULT_EXTRACTION_CONFIG,
  trigger?: ExtractionTrigger,
): Promise<ExtractionResponse> {
  const start = Date.now();
  const isTodoCompletion = trigger === 'todo_completion';

  if (!isTodoCompletion && accumulator.toolCallCount < config.minToolCallsToExtract && !signal?.extractable) {
    log.debug('extractor', 'Skipping extraction - too few tool calls and no extractable signal', {
      toolCallCount: accumulator.toolCallCount,
      minRequired: config.minToolCallsToExtract,
    });
    return [];
  }

  log.info('extractor', 'Extracting memories from segment', {
    segmentId: accumulator.segmentId,
    toolCallCount: accumulator.toolCallCount,
    filesRead: accumulator.filesRead.length,
    filesModified: accumulator.filesModified.length,
    hasSignal: !!signal?.extractable,
    trigger: trigger ?? 'default',
  });

  const prompt = isTodoCompletion
    ? buildTodoCompletionPrompt(accumulator)
    : buildExtractionPrompt(accumulator, signal);
  const systemPrompt = isTodoCompletion ? TODO_COMPLETION_SYSTEM : EXTRACTION_SYSTEM;
  const maxTokens = isTodoCompletion ? 2000 : config.maxTokens;
  const model = isTodoCompletion ? 'haiku' : config.model;

  const response = await makeAnthropicCall({
    model,
    max_tokens: maxTokens,
    system: systemPrompt,
    messages: [
      {
        role: 'user',
        content: prompt,
      },
    ],
  });

  if (!response) {
    log.warn('extractor', 'Extraction failed - no response');
    return [];
  }

  const rawMemories = parseJsonFromResponse<RawExtractedMemory[]>(response);

  if (!rawMemories || !Array.isArray(rawMemories)) {
    log.debug('extractor', 'No memories extracted (empty or invalid response)');
    return [];
  }

  const validMemories = rawMemories
    .map(validateMemory)
    .filter((m): m is ExtractedMemory => m !== null)
    .slice(0, 5);

  log.info('extractor', 'Extraction complete', {
    segmentId: accumulator.segmentId,
    memoriesExtracted: validMemories.length,
    ms: Date.now() - start,
  });

  return validMemories;
}

export async function saveExtractionSegment(
  accumulator: SegmentAccumulator,
  trigger: ExtractionTrigger,
  memoriesExtracted: number,
  extractionDurationMs: number,
  extractionTokens?: number,
): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  await db.execute(
    `INSERT INTO extraction_segments (
      id, session_id, project_id, trigger,
      user_prompts_json, files_read_json, files_modified_json,
      tool_call_count, memories_extracted, extraction_tokens,
      segment_start, segment_end, extraction_duration_ms, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      accumulator.segmentId,
      accumulator.sessionId,
      accumulator.projectId,
      trigger,
      JSON.stringify(accumulator.userPrompts),
      JSON.stringify(accumulator.filesRead),
      JSON.stringify(accumulator.filesModified),
      accumulator.toolCallCount,
      memoriesExtracted,
      extractionTokens ?? null,
      accumulator.segmentStart,
      now,
      extractionDurationMs,
      now,
    ],
  );

  log.debug('extractor', 'Saved extraction segment', {
    segmentId: accumulator.segmentId,
    trigger,
    memoriesExtracted,
  });
}

export function getSectorForMemoryType(type: MemoryType): string {
  return MEMORY_TYPE_TO_SECTOR[type];
}
