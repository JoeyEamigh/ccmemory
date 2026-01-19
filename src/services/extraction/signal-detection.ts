import { log } from '../../utils/log.js';
import { makeAnthropicCall, parseJsonFromResponse } from './inference.js';
import type { SignalClassification } from './types.js';
import { DEFAULT_SIGNAL_DETECTION_CONFIG } from './types.js';

const SIGNAL_CLASSIFICATION_SYSTEM = `You are a signal classifier for a Claude Code memory system. Your job is to classify user messages to understand if they contain information worth remembering.`;

function buildSignalClassificationPrompt(userPrompt: string): string {
  return `Classify this user message to Claude Code. Respond with JSON only.

Message: "${userPrompt.slice(0, 2000)}"

Categories:
- correction: User is correcting Claude's approach or output
- preference: User is stating a preference for how things should be done
- context: User is providing background information about the codebase/project
- task: User is giving a new task or continuing work
- question: User is asking a question
- feedback: User is giving positive/negative feedback without correction

\`\`\`json
{
  "category": "correction|preference|context|task|question|feedback",
  "extractable": true|false,
  "summary": "One sentence summary if extractable, null otherwise"
}
\`\`\`

Guidelines:
- "extractable" is true if this message contains information worth remembering
- corrections and preferences are almost always extractable
- context is often extractable
- tasks, questions, and feedback are rarely extractable
- summary should capture the preference/correction/context if present`;
}

type RawSignalResponse = {
  category: string;
  extractable: boolean;
  summary: string | null;
};

export async function classifyUserSignal(
  prompt: string,
  config = DEFAULT_SIGNAL_DETECTION_CONFIG,
): Promise<SignalClassification | null> {
  const start = Date.now();

  log.info('signal-detection', 'Starting signal classification', {
    promptLength: prompt.length,
    promptPreview: prompt.slice(0, 100).replace(/\n/g, '\\n'),
    model: config.model,
  });

  const classificationPrompt = buildSignalClassificationPrompt(prompt);

  const response = await makeAnthropicCall({
    model: config.model,
    max_tokens: config.maxTokens,
    system: SIGNAL_CLASSIFICATION_SYSTEM,
    messages: [
      {
        role: 'user',
        content: classificationPrompt,
      },
    ],
  });

  if (!response) {
    log.error('signal-detection', 'Signal classification failed - SDK returned null', {
      promptLength: prompt.length,
      model: config.model,
      ms: Date.now() - start,
    });
    return null;
  }

  const parsed = parseJsonFromResponse<RawSignalResponse>(response);

  if (!parsed) {
    log.warn('signal-detection', 'Failed to parse signal classification response');
    return null;
  }

  const validCategories = ['correction', 'preference', 'context', 'task', 'question', 'feedback'] as const;
  type ValidCategory = (typeof validCategories)[number];

  const category = validCategories.includes(parsed.category as ValidCategory)
    ? (parsed.category as ValidCategory)
    : 'task';

  const result: SignalClassification = {
    category,
    extractable: Boolean(parsed.extractable),
    summary: parsed.summary ?? null,
  };

  log.info('signal-detection', 'Signal classified', {
    category: result.category,
    extractable: result.extractable,
    hasSummary: !!result.summary,
    ms: Date.now() - start,
  });

  return result;
}

export function isHighPrioritySignal(signal: SignalClassification): boolean {
  return signal.category === 'correction' || signal.category === 'preference';
}

export function shouldExtractImmediately(signal: SignalClassification): boolean {
  return signal.extractable && isHighPrioritySignal(signal);
}
