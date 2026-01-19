import { execSync } from 'child_process';
import { query, type Options, type SDKAssistantMessage, type SDKResultMessage } from '@anthropic-ai/claude-agent-sdk';
import { log } from '../../utils/log.js';

function findClaudeExecutable(): string {
  try {
    const claudePath = execSync(
      process.platform === 'win32' ? 'where claude' : 'which claude',
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }
    ).trim().split('\n')[0]?.trim();

    if (claudePath) return claudePath;
  } catch {
    log.debug('inference', 'Claude executable auto-detection failed');
  }

  throw new Error('Claude executable not found. Please add "claude" to your system PATH.');
}

type AnthropicRequest = {
  model: string;
  max_tokens: number;
  temperature?: number;
  system?: string;
  messages: Array<{ role: string; content: string }>;
};

type AnthropicResponse = {
  content: Array<{ type: string; text?: string }>;
  usage?: {
    input_tokens: number;
    output_tokens: number;
  };
};

const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 1000;

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

export function parseJsonFromResponse<T>(response: AnthropicResponse): T | null {
  const text = response.content.find(c => c.type === 'text')?.text ?? '';

  const jsonMatch = text.match(/```(?:json)?\s*([\s\S]*?)```/);
  const jsonStr = jsonMatch ? jsonMatch[1]?.trim() : text.trim();

  try {
    return JSON.parse(jsonStr || "") as T;
  } catch {
    log.debug('inference', 'Failed to parse JSON from response', { preview: jsonStr?.slice(0, 200) });
    return null;
  }
}

async function makeSDKCall(request: AnthropicRequest): Promise<AnthropicResponse | null> {
  const start = Date.now();

  const fullPrompt = request.system
    ? `${request.system}\n\n${request.messages.map(m => m.content).join('\n\n')}`
    : request.messages.map(m => m.content).join('\n\n');

  log.info('inference', 'Starting Agent SDK call', {
    model: request.model,
    maxTokens: request.max_tokens,
    promptLength: fullPrompt.length,
    promptPreview: fullPrompt.slice(0, 200).replace(/\n/g, '\\n'),
  });

  const modelMap: Record<string, string> = {
    'haiku': 'claude-3-5-haiku-latest',
    'sonnet': 'claude-sonnet-4-20250514',
    'opus': 'claude-opus-4-20250514',
  };

  const modelId = modelMap[request.model] ?? 'claude-3-5-haiku-latest';
  let stderrOutput = '';

  try {
    const claudePath = findClaudeExecutable();
    log.debug('inference', 'Using claude executable', { claudePath });

    const options: Options = {
      model: modelId,
      persistSession: false,
      disallowedTools: [
        'Bash', 'Read', 'Write', 'Edit', 'Glob', 'Grep',
        'Task', 'WebFetch', 'WebSearch', 'NotebookEdit',
        'AskUserQuestion', 'TodoWrite'
      ],
      hooks: {},
      settingSources: [],
      pathToClaudeCodeExecutable: claudePath,
      extraArgs: {
        'print': null,
      },
      stderr: (data: string) => {
        stderrOutput += data;
      },
    };

    const abortController = new AbortController();
    const timeout = setTimeout(() => {
      log.warn('inference', 'SDK call timed out, aborting');
      abortController.abort();
    }, 60000);

    let responseText = '';
    let inputTokens = 0;
    let outputTokens = 0;

    const queryResult = query({
      prompt: fullPrompt,
      options: {
        ...options,
        abortController,
      },
    });

    for await (const message of queryResult) {
      if (message.type === 'assistant') {
        const assistantMsg = message as SDKAssistantMessage;
        for (const block of assistantMsg.message.content) {
          if (block.type === 'text' && 'text' in block) {
            responseText += block.text;
          }
        }
      } else if (message.type === 'result') {
        const resultMsg = message as SDKResultMessage;
        inputTokens = resultMsg.usage?.inputTokens ?? 0;
        outputTokens = resultMsg.usage?.outputTokens ?? 0;
      }
    }

    clearTimeout(timeout);

    log.info('inference', 'Agent SDK call completed', {
      responseLength: responseText.length,
      inputTokens,
      outputTokens,
      ms: Date.now() - start,
    });

    return {
      content: [{ type: 'text', text: responseText }],
      usage: {
        input_tokens: inputTokens,
        output_tokens: outputTokens,
      },
    };
  } catch (err) {
    log.error('inference', 'Agent SDK call failed', {
      error: err instanceof Error ? err.message : String(err),
      stderr: stderrOutput.slice(0, 500),
      ms: Date.now() - start,
    });
    return null;
  }
}

export async function makeAnthropicCall(request: AnthropicRequest): Promise<AnthropicResponse | null> {
  for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
    const result = await makeSDKCall(request);
    if (result) return result;

    if (attempt < MAX_RETRIES) {
      log.debug('inference', 'Retrying Agent SDK call', { attempt, maxRetries: MAX_RETRIES });
      await sleep(RETRY_DELAY_MS * attempt);
    }
  }

  log.warn('inference', 'All Agent SDK retry attempts failed');
  return null;
}
