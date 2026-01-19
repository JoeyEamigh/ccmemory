import { getDatabase } from '../../db/database.js';
import { log } from '../../utils/log.js';
import type {
  ACCUMULATOR_LIMITS,
  CommandSummary,
  CompletedTask,
  ErrorSummary,
  SearchSummary,
  SegmentAccumulator,
  UserPrompt,
} from './types.js';

const LIMITS = {
  maxFilesTracked: 100,
  maxCommandsTracked: 50,
  maxErrorsTracked: 20,
  maxSearchesTracked: 50,
  maxTasksTracked: 50,
};

function parseJsonArray<T>(value: unknown): T[] {
  if (typeof value === 'string') {
    try {
      const parsed = JSON.parse(value) as unknown;
      if (Array.isArray(parsed)) {
        return parsed as T[];
      }
    } catch {
      return [];
    }
  }
  return [];
}

export async function getAccumulator(sessionId: string): Promise<SegmentAccumulator | null> {
  const db = await getDatabase();

  const result = await db.execute('SELECT * FROM segment_accumulators WHERE session_id = ?', [sessionId]);

  if (result.rows.length === 0) return null;
  const row = result.rows[0];
  if (!row) return null;

  return {
    sessionId: String(row['session_id']),
    projectId: String(row['project_id']),
    segmentId: String(row['segment_id']),
    segmentStart: Number(row['segment_start']),
    userPrompts: parseJsonArray<UserPrompt>(row['user_prompts_json']),
    filesRead: parseJsonArray<string>(row['files_read_json']),
    filesModified: parseJsonArray<string>(row['files_modified_json']),
    commandsRun: parseJsonArray<CommandSummary>(row['commands_run_json']),
    errorsEncountered: parseJsonArray<ErrorSummary>(row['errors_encountered_json']),
    searchesPerformed: parseJsonArray<SearchSummary>(row['searches_performed_json']),
    completedTasks: parseJsonArray<CompletedTask>(row['completed_tasks_json']),
    lastAssistantMessage: row['last_assistant_message'] ? String(row['last_assistant_message']) : undefined,
    toolCallCount: Number(row['tool_call_count']),
  };
}

export async function getOrCreateAccumulator(sessionId: string, projectId: string): Promise<SegmentAccumulator> {
  const existing = await getAccumulator(sessionId);
  if (existing) return existing;

  return await startNewSegment(sessionId, projectId);
}

export async function startNewSegment(
  sessionId: string,
  projectId: string,
  initialPrompt?: UserPrompt,
): Promise<SegmentAccumulator> {
  const db = await getDatabase();
  const segmentId = crypto.randomUUID();
  const now = Date.now();

  const userPrompts = initialPrompt ? [initialPrompt] : [];

  await db.execute(
    `INSERT OR REPLACE INTO segment_accumulators (
      session_id, project_id, segment_id, segment_start,
      user_prompts_json, files_read_json, files_modified_json,
      commands_run_json, errors_encountered_json, searches_performed_json,
      completed_tasks_json, tool_call_count, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      sessionId,
      projectId,
      segmentId,
      now,
      JSON.stringify(userPrompts),
      '[]',
      '[]',
      '[]',
      '[]',
      '[]',
      '[]',
      0,
      now,
    ],
  );

  log.debug('accumulator', 'Started new segment', { sessionId, segmentId });

  return {
    sessionId,
    projectId,
    segmentId,
    segmentStart: now,
    userPrompts,
    filesRead: [],
    filesModified: [],
    commandsRun: [],
    errorsEncountered: [],
    searchesPerformed: [],
    completedTasks: [],
    toolCallCount: 0,
  };
}

export async function addUserPrompt(sessionId: string, prompt: UserPrompt): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) {
    log.warn('accumulator', 'No accumulator found for session', { sessionId });
    return;
  }

  const userPrompts = [...accumulator.userPrompts, prompt];

  await db.execute(
    `UPDATE segment_accumulators SET user_prompts_json = ?, updated_at = ? WHERE session_id = ?`,
    [JSON.stringify(userPrompts), now, sessionId],
  );

  log.debug('accumulator', 'Added user prompt', { sessionId, category: prompt.signal?.category });
}

export async function addFileRead(sessionId: string, filePath: string): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) return;

  if (accumulator.filesRead.length >= LIMITS.maxFilesTracked) return;
  if (accumulator.filesRead.includes(filePath)) return;

  const filesRead = [...accumulator.filesRead, filePath];

  await db.execute(
    `UPDATE segment_accumulators SET
      files_read_json = ?,
      tool_call_count = tool_call_count + 1,
      updated_at = ?
    WHERE session_id = ?`,
    [JSON.stringify(filesRead), now, sessionId],
  );
}

export async function addFileModified(sessionId: string, filePath: string): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) return;

  if (accumulator.filesModified.length >= LIMITS.maxFilesTracked) return;
  if (accumulator.filesModified.includes(filePath)) return;

  const filesModified = [...accumulator.filesModified, filePath];

  await db.execute(
    `UPDATE segment_accumulators SET
      files_modified_json = ?,
      tool_call_count = tool_call_count + 1,
      updated_at = ?
    WHERE session_id = ?`,
    [JSON.stringify(filesModified), now, sessionId],
  );
}

export async function addCommand(sessionId: string, command: CommandSummary): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) return;

  if (accumulator.commandsRun.length >= LIMITS.maxCommandsTracked) return;

  const commandsRun = [...accumulator.commandsRun, command];

  await db.execute(
    `UPDATE segment_accumulators SET
      commands_run_json = ?,
      tool_call_count = tool_call_count + 1,
      updated_at = ?
    WHERE session_id = ?`,
    [JSON.stringify(commandsRun), now, sessionId],
  );
}

export async function addError(sessionId: string, error: ErrorSummary): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) return;

  if (accumulator.errorsEncountered.length >= LIMITS.maxErrorsTracked) return;

  const errorsEncountered = [...accumulator.errorsEncountered, error];

  await db.execute(
    `UPDATE segment_accumulators SET errors_encountered_json = ?, updated_at = ? WHERE session_id = ?`,
    [JSON.stringify(errorsEncountered), now, sessionId],
  );
}

export async function addSearch(sessionId: string, search: SearchSummary): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) return;

  if (accumulator.searchesPerformed.length >= LIMITS.maxSearchesTracked) return;

  const searchesPerformed = [...accumulator.searchesPerformed, search];

  await db.execute(
    `UPDATE segment_accumulators SET
      searches_performed_json = ?,
      tool_call_count = tool_call_count + 1,
      updated_at = ?
    WHERE session_id = ?`,
    [JSON.stringify(searchesPerformed), now, sessionId],
  );
}

export async function addCompletedTask(sessionId: string, task: CompletedTask): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  const accumulator = await getAccumulator(sessionId);
  if (!accumulator) return;

  if (accumulator.completedTasks.length >= LIMITS.maxTasksTracked) return;

  const completedTasks = [...accumulator.completedTasks, task];

  await db.execute(
    `UPDATE segment_accumulators SET completed_tasks_json = ?, updated_at = ? WHERE session_id = ?`,
    [JSON.stringify(completedTasks), now, sessionId],
  );

  log.debug('accumulator', 'Added completed task', { sessionId, task: task.content.slice(0, 50) });
}

export async function incrementToolCallCount(sessionId: string): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  await db.execute(
    `UPDATE segment_accumulators SET tool_call_count = tool_call_count + 1, updated_at = ? WHERE session_id = ?`,
    [now, sessionId],
  );
}

export async function setLastAssistantMessage(sessionId: string, message: string): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  await db.execute(
    `UPDATE segment_accumulators SET last_assistant_message = ?, updated_at = ? WHERE session_id = ?`,
    [message.slice(0, 10000), now, sessionId],
  );
}

export async function clearAccumulator(sessionId: string): Promise<void> {
  const db = await getDatabase();

  await db.execute('DELETE FROM segment_accumulators WHERE session_id = ?', [sessionId]);

  log.debug('accumulator', 'Cleared accumulator', { sessionId });
}

export async function saveAccumulator(accumulator: SegmentAccumulator): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  await db.execute(
    `INSERT OR REPLACE INTO segment_accumulators (
      session_id, project_id, segment_id, segment_start,
      user_prompts_json, files_read_json, files_modified_json,
      commands_run_json, errors_encountered_json, searches_performed_json,
      completed_tasks_json, last_assistant_message, tool_call_count, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      accumulator.sessionId,
      accumulator.projectId,
      accumulator.segmentId,
      accumulator.segmentStart,
      JSON.stringify(accumulator.userPrompts),
      JSON.stringify(accumulator.filesRead),
      JSON.stringify(accumulator.filesModified),
      JSON.stringify(accumulator.commandsRun),
      JSON.stringify(accumulator.errorsEncountered),
      JSON.stringify(accumulator.searchesPerformed),
      JSON.stringify(accumulator.completedTasks),
      accumulator.lastAssistantMessage ?? null,
      accumulator.toolCallCount,
      now,
    ],
  );
}
