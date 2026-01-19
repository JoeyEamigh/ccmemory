import { getOrCreateSession } from '../services/memory/sessions.js';
import { createMemoryStore } from '../services/memory/store.js';
import { getOrCreateProject } from '../services/project.js';
import { log } from '../utils/log.js';
import { registerClient } from '../webui/coordination.js';

type HookInput = {
  session_id: string;
  cwd: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
  tool_response: unknown;
};

const TIMEOUT_MS = 10000;

function parseInput(text: string): HookInput | null {
  try {
    const parsed = JSON.parse(text) as unknown;
    if (typeof parsed !== 'object' || parsed === null) return null;
    const obj = parsed as Record<string, unknown>;
    if (typeof obj['session_id'] !== 'string') return null;
    if (typeof obj['cwd'] !== 'string') return null;
    if (typeof obj['tool_name'] !== 'string') return null;
    return obj as unknown as HookInput;
  } catch {
    return null;
  }
}

export async function captureHook(): Promise<void> {
  const timeoutId = setTimeout(() => {
    log.warn('capture', 'Capture hook timed out');
    process.exit(0);
  }, TIMEOUT_MS);

  const inputText = await Bun.stdin.text();
  const input = parseInput(inputText);

  if (!input) {
    log.warn('capture', 'Invalid hook input, skipping');
    clearTimeout(timeoutId);
    process.exit(0);
  }

  const { session_id, cwd, tool_name, tool_input, tool_response } = input;
  log.debug('capture', 'Processing tool observation', { session_id, tool_name });

  // Ensure server is running and register this session
  await registerSessionClient(session_id);

  const resultStr = JSON.stringify(tool_response);
  if (resultStr.length > 10000) {
    log.debug('capture', 'Skipping large tool result', {
      tool_name,
      bytes: resultStr.length,
    });
    clearTimeout(timeoutId);
    process.exit(0);
  }

  const project = await getOrCreateProject(cwd);
  await getOrCreateSession(session_id, project.id);

  const content = formatToolObservation(tool_name, tool_input, tool_response);
  const files = extractFilePaths(tool_input, tool_response);

  const store = createMemoryStore();

  const memory = await store.create(
    {
      content,
      sector: 'episodic',
      tier: 'session',
      files,
    },
    project.id,
    session_id,
  );

  log.debug('capture', 'Captured tool observation', {
    session_id,
    tool_name,
    memoryId: memory.id,
  });

  await notifyMemoryCreated(memory.id, project.id, session_id);

  clearTimeout(timeoutId);
  process.exit(0);
}

function formatToolObservation(toolName: string, input: Record<string, unknown>, result: unknown): string {
  const lines: string[] = [`Tool: ${toolName}`];

  switch (toolName) {
    case 'Read':
      lines.push(`Read file: ${String(input['file_path'] ?? '')}`);
      break;
    case 'Write':
      lines.push(`Wrote file: ${String(input['file_path'] ?? '')}`);
      break;
    case 'Edit':
      lines.push(`Edited file: ${String(input['file_path'] ?? '')}`);
      break;
    case 'Bash': {
      const command = String(input['command'] ?? '').slice(0, 200);
      lines.push(`Command: ${command}`);
      if (typeof result === 'string' && result.length < 500) {
        lines.push(`Output: ${result}`);
      }
      break;
    }
    case 'Grep':
    case 'Glob':
      lines.push(`Pattern: ${String(input['pattern'] ?? '')}`);
      break;
    default:
      lines.push(`Input: ${JSON.stringify(input).slice(0, 300)}`);
  }

  return lines.join('\n');
}

function extractFilePaths(input: Record<string, unknown>, result: unknown): string[] {
  const paths: string[] = [];

  if (typeof input['file_path'] === 'string') {
    paths.push(input['file_path']);
  }
  if (typeof input['path'] === 'string') {
    paths.push(input['path']);
  }

  if (Array.isArray(result)) {
    const filePaths = result.filter((r): r is string => typeof r === 'string' && (r.includes('/') || r.includes('\\')));
    paths.push(...filePaths.slice(0, 10));
  }

  return [...new Set(paths)];
}

async function registerSessionClient(sessionId: string): Promise<void> {
  try {
    await registerClient(sessionId);
    log.debug('capture', 'Session registered', { sessionId });
  } catch (err) {
    log.debug('capture', 'Session registration skipped', {
      error: err instanceof Error ? err.message : String(err),
    });
  }
}

async function notifyMemoryCreated(_memoryId: string, _projectId: string, _sessionId: string): Promise<void> {
  // WebUI notifications are optional - silently skip if server not running
  // This function is kept for potential future use with WebSocket notifications
}
