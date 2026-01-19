import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { closeDatabase, createDatabase, setDatabase, type Database } from '../../../db/database.js';
import { createMemoryStore } from '../../memory/store.js';
import {
  addCommand,
  addFileModified,
  addFileRead,
  createExtractionService,
  getAccumulator,
  getOrCreateAccumulator,
} from '../index.js';
import type { SignalClassification, UserPrompt } from '../types.js';

describe('ExtractionService', () => {
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

  describe('createExtractionService', () => {
    test('creates service without embedding service', () => {
      const service = createExtractionService(null);
      expect(service).toBeDefined();
      expect(service.startSegment).toBeInstanceOf(Function);
      expect(service.extractSegment).toBeInstanceOf(Function);
      expect(service.clearAccumulator).toBeInstanceOf(Function);
    });

    test('service.startSegment initializes accumulator', async () => {
      const service = createExtractionService(null);
      const prompt: UserPrompt = {
        content: 'Help me debug',
        timestamp: Date.now(),
      };

      await service.startSegment('sess1', 'proj1', prompt);

      const accumulator = await getAccumulator('sess1');
      expect(accumulator).not.toBeNull();
      expect(accumulator?.userPrompts).toHaveLength(1);
    });

    test('service.clearAccumulator removes accumulator', async () => {
      const service = createExtractionService(null);
      await service.startSegment('sess1', 'proj1', { content: 'Test', timestamp: Date.now() });

      await service.clearAccumulator('sess1');

      const accumulator = await getAccumulator('sess1');
      expect(accumulator).toBeNull();
    });
  });

  describe('Segment workflow', () => {
    test('accumulator tracks work through segment', async () => {
      await getOrCreateAccumulator('sess1', 'proj1');

      await addFileRead('sess1', '/src/index.ts');
      await addFileRead('sess1', '/src/utils.ts');
      await addFileModified('sess1', '/src/app.ts');
      await addCommand('sess1', { command: 'npm test', hasError: false });
      await addCommand('sess1', { command: 'npm build', exitCode: 1, hasError: true });

      const accumulator = await getAccumulator('sess1');

      expect(accumulator?.filesRead).toHaveLength(2);
      expect(accumulator?.filesModified).toHaveLength(1);
      expect(accumulator?.commandsRun).toHaveLength(2);
      expect(accumulator?.toolCallCount).toBe(5);
    });

    test('new segment replaces previous accumulator', async () => {
      const service = createExtractionService(null);

      await service.startSegment('sess1', 'proj1', { content: 'First task', timestamp: Date.now() });
      await addFileRead('sess1', '/first.ts');

      const firstAccum = await getAccumulator('sess1');
      const firstSegmentId = firstAccum?.segmentId;

      await service.startSegment('sess1', 'proj1', { content: 'Second task', timestamp: Date.now() });

      const secondAccum = await getAccumulator('sess1');
      expect(secondAccum?.segmentId).not.toBe(firstSegmentId);
      expect(secondAccum?.filesRead).toHaveLength(0);
    });
  });

  describe('Memory creation with types', () => {
    test('creates memory with memory type', async () => {
      const store = createMemoryStore();

      const memory = await store.create(
        {
          content: 'User prefers async/await over callbacks',
          memoryType: 'preference',
          context: 'User correction during code review',
          confidence: 1.0,
          tier: 'project',
        },
        'proj1',
      );

      expect(memory.memoryType).toBe('preference');
      expect(memory.context).toBe('User correction during code review');
      expect(memory.confidence).toBe(1.0);
      expect(memory.sector).toBe('emotional');
    });

    test('codebase memory type maps to semantic sector', async () => {
      const store = createMemoryStore();

      const memory = await store.create(
        {
          content: 'The API routes are defined in src/routes/api.ts',
          memoryType: 'codebase',
        },
        'proj1',
      );

      expect(memory.sector).toBe('semantic');
    });

    test('gotcha memory type maps to procedural sector', async () => {
      const store = createMemoryStore();

      const memory = await store.create(
        {
          content: 'Must clear cache after changing config',
          memoryType: 'gotcha',
        },
        'proj1',
      );

      expect(memory.sector).toBe('procedural');
    });

    test('decision memory type maps to reflective sector', async () => {
      const store = createMemoryStore();

      const memory = await store.create(
        {
          content: 'Chose PostgreSQL over MySQL for better JSON support',
          memoryType: 'decision',
        },
        'proj1',
      );

      expect(memory.sector).toBe('reflective');
    });

    test('pattern memory type maps to procedural sector', async () => {
      const store = createMemoryStore();

      const memory = await store.create(
        {
          content: 'All React components use functional style with hooks',
          memoryType: 'pattern',
        },
        'proj1',
      );

      expect(memory.sector).toBe('procedural');
    });
  });

  describe('Signal classification workflow', () => {
    test('signal stored in accumulator with prompt', async () => {
      const service = createExtractionService(null);

      const signal: SignalClassification = {
        category: 'preference',
        extractable: true,
        summary: 'User prefers TypeScript strict mode',
      };

      await service.startSegment('sess1', 'proj1', {
        content: 'Always use strict TypeScript',
        timestamp: Date.now(),
        signal,
      });

      const accumulator = await getAccumulator('sess1');
      expect(accumulator?.userPrompts[0]?.signal?.category).toBe('preference');
      expect(accumulator?.userPrompts[0]?.signal?.extractable).toBe(true);
    });
  });
});
