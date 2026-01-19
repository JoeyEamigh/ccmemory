import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { closeDatabase, createDatabase, setDatabase, type Database } from '../../../db/database.js';
import { createSessionService, type SessionService } from '../sessions.js';
import { createMemoryStore, type MemoryStore } from '../store.js';

describe('SessionService', () => {
  let db: Database;
  let sessionService: SessionService;
  let memoryStore: MemoryStore;

  beforeEach(async () => {
    db = await createDatabase(':memory:');
    setDatabase(db);
    sessionService = createSessionService();
    memoryStore = createMemoryStore();

    const now = Date.now();
    await db.execute(`INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`, [
      'proj1',
      '/test/path',
      'Test Project',
      now,
      now,
    ]);
  });

  afterEach(() => {
    closeDatabase();
  });

  describe('create', () => {
    test('creates session with minimal input', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      expect(session.id).toBeDefined();
      expect(session.projectId).toBe('proj1');
      expect(session.startedAt).toBeDefined();
      expect(session.endedAt).toBeUndefined();
    });

    test('creates session with user prompt', async () => {
      const session = await sessionService.create({
        projectId: 'proj1',
        userPrompt: 'Help me fix authentication',
      });

      expect(session.userPrompt).toBe('Help me fix authentication');
    });

    test('creates session with context', async () => {
      const session = await sessionService.create({
        projectId: 'proj1',
        context: { tool: 'Claude Code', version: '1.0' },
      });

      expect(session.context).toEqual({ tool: 'Claude Code', version: '1.0' });
    });
  });

  describe('get', () => {
    test('returns session by id', async () => {
      const created = await sessionService.create({ projectId: 'proj1' });

      const retrieved = await sessionService.get(created.id);

      expect(retrieved).not.toBeNull();
      expect(retrieved?.projectId).toBe('proj1');
    });

    test('returns null for non-existent id', async () => {
      const retrieved = await sessionService.get('non-existent');
      expect(retrieved).toBeNull();
    });
  });

  describe('end', () => {
    test('ends session without summary', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const ended = await sessionService.end(session.id);

      expect(ended.endedAt).toBeDefined();
      expect(ended.summary).toBeUndefined();
    });

    test('ends session with summary', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const ended = await sessionService.end(session.id, 'Implemented authentication feature');

      expect(ended.endedAt).toBeDefined();
      expect(ended.summary).toBe('Implemented authentication feature');
    });
  });

  describe('getStats', () => {
    test('returns empty stats for new session', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const stats = await sessionService.getStats(session.id);

      expect(stats.memoriesCreated).toBe(0);
      expect(stats.memoriesRecalled).toBe(0);
      expect(stats.memoriesUpdated).toBe(0);
      expect(stats.memoriesReinforced).toBe(0);
      expect(stats.totalMemories).toBe(0);
    });

    test('tracks created memories', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation' },
        'proj1',
        session.id,
      );
      await memoryStore.create(
        { content: 'Database migrations are stored in the migrations folder' },
        'proj1',
        session.id,
      );

      const stats = await sessionService.getStats(session.id);

      expect(stats.memoriesCreated).toBe(2);
      expect(stats.totalMemories).toBe(2);
    });

    test('tracks reinforced memories', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const memory = await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation' },
        'proj1',
        session.id,
      );

      await new Promise(resolve => setTimeout(resolve, 5));
      await memoryStore.linkToSession(memory.id, session.id, 'reinforced');

      const stats = await sessionService.getStats(session.id);

      expect(stats.memoriesCreated).toBe(1);
      expect(stats.memoriesReinforced).toBe(1);
      expect(stats.totalMemories).toBe(2);
    });
  });

  describe('getSessionMemories', () => {
    test('returns memories linked to session', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation' },
        'proj1',
        session.id,
      );
      await memoryStore.create(
        { content: 'Database migrations are stored in the migrations folder' },
        'proj1',
        session.id,
      );

      const memories = await sessionService.getSessionMemories(session.id);

      expect(memories).toHaveLength(2);
    });

    test('excludes memories from other sessions', async () => {
      const session1 = await sessionService.create({ projectId: 'proj1' });
      const session2 = await sessionService.create({ projectId: 'proj1' });

      await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation' },
        'proj1',
        session1.id,
      );
      await memoryStore.create(
        { content: 'Database migrations are stored in the migrations folder' },
        'proj1',
        session2.id,
      );

      const memories1 = await sessionService.getSessionMemories(session1.id);
      const memories2 = await sessionService.getSessionMemories(session2.id);

      expect(memories1).toHaveLength(1);
      expect(memories2).toHaveLength(1);
    });

    test('returns distinct memories when used multiple times', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const memory = await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation' },
        'proj1',
        session.id,
      );

      await new Promise(resolve => setTimeout(resolve, 5));
      await memoryStore.linkToSession(memory.id, session.id, 'recalled');
      await new Promise(resolve => setTimeout(resolve, 5));
      await memoryStore.linkToSession(memory.id, session.id, 'reinforced');

      const memories = await sessionService.getSessionMemories(session.id);

      expect(memories).toHaveLength(1);
    });
  });

  describe('promoteSessionMemories', () => {
    test('promotes memories used multiple times', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const memory = await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation', tier: 'session' },
        'proj1',
        session.id,
      );

      await new Promise(resolve => setTimeout(resolve, 5));
      await memoryStore.linkToSession(memory.id, session.id, 'recalled');

      const promotedCount = await sessionService.promoteSessionMemories(session.id, 2);

      expect(promotedCount).toBe(1);

      const updated = await memoryStore.get(memory.id);
      expect(updated?.tier).toBe('project');
    });

    test('does not promote memories below threshold', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const memory = await memoryStore.create(
        { content: 'The authentication flow uses JWT tokens for validation', tier: 'session' },
        'proj1',
        session.id,
      );

      const promotedCount = await sessionService.promoteSessionMemories(session.id, 2);

      expect(promotedCount).toBe(0);

      const updated = await memoryStore.get(memory.id);
      expect(updated?.tier).toBe('session');
    });

    test('returns zero when no memories to promote', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });

      const promotedCount = await sessionService.promoteSessionMemories(session.id);

      expect(promotedCount).toBe(0);
    });
  });

  describe('getActiveSession', () => {
    test('returns active session for project', async () => {
      await sessionService.create({ projectId: 'proj1' });

      const active = await sessionService.getActiveSession('proj1');

      expect(active).not.toBeNull();
      expect(active?.endedAt).toBeUndefined();
    });

    test('returns null when no active session', async () => {
      const session = await sessionService.create({ projectId: 'proj1' });
      await sessionService.end(session.id);

      const active = await sessionService.getActiveSession('proj1');

      expect(active).toBeNull();
    });

    test('returns most recent active session', async () => {
      await sessionService.create({ projectId: 'proj1' });
      await new Promise(resolve => setTimeout(resolve, 10));
      const session2 = await sessionService.create({ projectId: 'proj1' });

      const active = await sessionService.getActiveSession('proj1');

      expect(active?.id).toBe(session2.id);
    });
  });
});
