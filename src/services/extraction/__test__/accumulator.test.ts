import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { closeDatabase, createDatabase, setDatabase, type Database } from '../../../db/database.js';
import {
  addCommand,
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
  startNewSegment,
} from '../accumulator.js';
import type { SignalClassification, UserPrompt } from '../types.js';

describe('SegmentAccumulator', () => {
  let db: Database;

  beforeEach(async () => {
    db = await createDatabase(':memory:');
    setDatabase(db);

    const now = Date.now();
    await db.execute(`INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`, [
      'proj1',
      '/test/path',
      'Test Project',
      now,
      now,
    ]);

    await db.execute(`INSERT INTO sessions (id, project_id, started_at) VALUES (?, ?, ?)`, [
      'sess1',
      'proj1',
      now,
    ]);
  });

  afterEach(() => {
    closeDatabase();
  });

  describe('startNewSegment', () => {
    test('creates a new accumulator with unique segment ID', async () => {
      const accumulator = await startNewSegment('sess1', 'proj1');

      expect(accumulator.sessionId).toBe('sess1');
      expect(accumulator.projectId).toBe('proj1');
      expect(accumulator.segmentId).toMatch(/^[0-9a-f-]{36}$/);
      expect(accumulator.toolCallCount).toBe(0);
    });

    test('creates accumulator with initial user prompt when provided', async () => {
      const prompt: UserPrompt = {
        content: 'Fix the login bug',
        timestamp: Date.now(),
        signal: { category: 'task', extractable: false, summary: null },
      };

      const accumulator = await startNewSegment('sess1', 'proj1', prompt);

      expect(accumulator.userPrompts).toHaveLength(1);
      expect(accumulator.userPrompts[0]?.content).toBe('Fix the login bug');
    });

    test('starts with empty tracking arrays', async () => {
      const accumulator = await startNewSegment('sess1', 'proj1');

      expect(accumulator.filesRead).toEqual([]);
      expect(accumulator.filesModified).toEqual([]);
      expect(accumulator.commandsRun).toEqual([]);
      expect(accumulator.errorsEncountered).toEqual([]);
      expect(accumulator.searchesPerformed).toEqual([]);
    });
  });

  describe('getOrCreateAccumulator', () => {
    test('returns existing accumulator if present', async () => {
      const first = await startNewSegment('sess1', 'proj1');
      const second = await getOrCreateAccumulator('sess1', 'proj1');

      expect(second.segmentId).toBe(first.segmentId);
    });

    test('creates new accumulator if none exists', async () => {
      const accumulator = await getOrCreateAccumulator('sess1', 'proj1');

      expect(accumulator).not.toBeNull();
      expect(accumulator.sessionId).toBe('sess1');
    });
  });

  describe('addFileRead', () => {
    test('tracks read file paths', async () => {
      await startNewSegment('sess1', 'proj1');
      await addFileRead('sess1', '/path/to/file.ts');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.filesRead).toContain('/path/to/file.ts');
    });

    test('increments tool call count', async () => {
      await startNewSegment('sess1', 'proj1');
      await addFileRead('sess1', '/path/to/file.ts');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.toolCallCount).toBe(1);
    });

    test('does not add duplicate file paths', async () => {
      await startNewSegment('sess1', 'proj1');
      await addFileRead('sess1', '/path/to/file.ts');
      await addFileRead('sess1', '/path/to/file.ts');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.filesRead).toHaveLength(1);
    });

    test('enforces maximum tracked files limit', async () => {
      await startNewSegment('sess1', 'proj1');

      for (let i = 0; i < 110; i++) {
        await addFileRead('sess1', `/path/file${i}.ts`);
      }

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.filesRead.length).toBeLessThanOrEqual(100);
    });
  });

  describe('addFileModified', () => {
    test('tracks modified file paths', async () => {
      await startNewSegment('sess1', 'proj1');
      await addFileModified('sess1', '/path/to/modified.ts');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.filesModified).toContain('/path/to/modified.ts');
    });

    test('does not add duplicate modified file paths', async () => {
      await startNewSegment('sess1', 'proj1');
      await addFileModified('sess1', '/path/to/file.ts');
      await addFileModified('sess1', '/path/to/file.ts');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.filesModified).toHaveLength(1);
    });
  });

  describe('addCommand', () => {
    test('tracks command executions with exit codes', async () => {
      await startNewSegment('sess1', 'proj1');
      await addCommand('sess1', {
        command: 'npm test',
        exitCode: 0,
        hasError: false,
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.commandsRun).toHaveLength(1);
      expect(accumulator?.commandsRun[0]?.command).toBe('npm test');
      expect(accumulator?.commandsRun[0]?.exitCode).toBe(0);
    });

    test('tracks failed commands', async () => {
      await startNewSegment('sess1', 'proj1');
      await addCommand('sess1', {
        command: 'npm build',
        exitCode: 1,
        hasError: true,
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.commandsRun[0]?.hasError).toBe(true);
    });

    test('enforces maximum commands limit', async () => {
      await startNewSegment('sess1', 'proj1');

      for (let i = 0; i < 60; i++) {
        await addCommand('sess1', { command: `cmd${i}`, hasError: false });
      }

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.commandsRun.length).toBeLessThanOrEqual(50);
    });
  });

  describe('addError', () => {
    test('tracks encountered errors', async () => {
      await startNewSegment('sess1', 'proj1');
      await addError('sess1', {
        source: 'TypeScript',
        message: 'Type error at line 42',
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.errorsEncountered).toHaveLength(1);
      expect(accumulator?.errorsEncountered[0]?.source).toBe('TypeScript');
    });

    test('enforces maximum errors limit', async () => {
      await startNewSegment('sess1', 'proj1');

      for (let i = 0; i < 30; i++) {
        await addError('sess1', { source: 'Test', message: `Error ${i}` });
      }

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.errorsEncountered.length).toBeLessThanOrEqual(20);
    });
  });

  describe('addSearch', () => {
    test('tracks grep searches', async () => {
      await startNewSegment('sess1', 'proj1');
      await addSearch('sess1', {
        tool: 'Grep',
        pattern: 'TODO',
        resultCount: 15,
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.searchesPerformed).toHaveLength(1);
      expect(accumulator?.searchesPerformed[0]?.tool).toBe('Grep');
    });

    test('tracks glob searches', async () => {
      await startNewSegment('sess1', 'proj1');
      await addSearch('sess1', {
        tool: 'Glob',
        pattern: '**/*.ts',
        resultCount: 42,
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.searchesPerformed[0]?.tool).toBe('Glob');
    });
  });

  describe('addUserPrompt', () => {
    test('appends user prompts to accumulator', async () => {
      await startNewSegment('sess1', 'proj1');

      const prompt: UserPrompt = {
        content: 'Help me fix this bug',
        timestamp: Date.now(),
        signal: { category: 'task', extractable: false, summary: null },
      };

      await addUserPrompt('sess1', prompt);

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.userPrompts).toHaveLength(1);
    });

    test('tracks signal classification on prompts', async () => {
      await startNewSegment('sess1', 'proj1');

      const signal: SignalClassification = {
        category: 'preference',
        extractable: true,
        summary: 'User prefers async/await over callbacks',
      };

      await addUserPrompt('sess1', {
        content: 'Always use async/await, not callbacks',
        timestamp: Date.now(),
        signal,
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.userPrompts[0]?.signal?.category).toBe('preference');
      expect(accumulator?.userPrompts[0]?.signal?.extractable).toBe(true);
    });
  });

  describe('setLastAssistantMessage', () => {
    test('stores the last assistant message', async () => {
      await startNewSegment('sess1', 'proj1');
      await setLastAssistantMessage('sess1', 'I have fixed the bug by updating the handler.');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.lastAssistantMessage).toBe('I have fixed the bug by updating the handler.');
    });

    test('truncates very long messages', async () => {
      await startNewSegment('sess1', 'proj1');
      const longMessage = 'x'.repeat(15000);
      await setLastAssistantMessage('sess1', longMessage);

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.lastAssistantMessage?.length).toBeLessThanOrEqual(10000);
    });
  });

  describe('incrementToolCallCount', () => {
    test('increments the tool call counter', async () => {
      await startNewSegment('sess1', 'proj1');
      await incrementToolCallCount('sess1');
      await incrementToolCallCount('sess1');
      await incrementToolCallCount('sess1');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.toolCallCount).toBe(3);
    });
  });

  describe('clearAccumulator', () => {
    test('removes the accumulator for a session', async () => {
      await startNewSegment('sess1', 'proj1');
      await clearAccumulator('sess1');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator).toBeNull();
    });
  });

  describe('getAccumulator', () => {
    test('returns null for non-existent session', async () => {
      const accumulator = await getAccumulator('nonexistent');
      expect(accumulator).toBeNull();
    });

    test('returns fully populated accumulator', async () => {
      await startNewSegment('sess1', 'proj1');
      await addFileRead('sess1', '/src/index.ts');
      await addFileModified('sess1', '/src/utils.ts');
      await addCommand('sess1', { command: 'npm test', hasError: false });
      await addError('sess1', { source: 'Lint', message: 'Missing semicolon' });
      await addSearch('sess1', { tool: 'Grep', pattern: 'TODO', resultCount: 5 });

      const accumulator = await getAccumulator('sess1');

      expect(accumulator?.filesRead).toContain('/src/index.ts');
      expect(accumulator?.filesModified).toContain('/src/utils.ts');
      expect(accumulator?.commandsRun).toHaveLength(1);
      expect(accumulator?.errorsEncountered).toHaveLength(1);
      expect(accumulator?.searchesPerformed).toHaveLength(1);
    });
  });
});
