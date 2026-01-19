import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { closeDatabase, createDatabase, setDatabase, type Database } from '../../../db/database.js';
import { createMemoryStore, type MemoryStore } from '../store.js';

describe('MemoryStore', () => {
  let db: Database;
  let store: MemoryStore;

  beforeEach(async () => {
    db = await createDatabase(':memory:');
    setDatabase(db);
    store = createMemoryStore();

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
    test('creates memory with auto-classification', async () => {
      const memory = await store.create({ content: 'The API endpoint is /api/users' }, 'proj1');

      expect(memory.sector).toBe('semantic');
      expect(memory.simhash).toBeDefined();
      expect(memory.salience).toBe(1.0);
      expect(memory.tier).toBe('project');
    });

    test('respects explicit sector', async () => {
      const memory = await store.create({ content: 'Some content', sector: 'episodic' }, 'proj1');

      expect(memory.sector).toBe('episodic');
    });

    test('extracts concepts from content', async () => {
      const memory = await store.create({ content: 'Use `getUserById` function from src/users/api.ts' }, 'proj1');

      expect(memory.concepts.length).toBeGreaterThan(0);
      expect(memory.concepts).toContain('getUserById');
    });

    test('sets validFrom when provided', async () => {
      const validFrom = Date.now() - 1000;
      const memory = await store.create({ content: 'Test', validFrom }, 'proj1');

      expect(memory.validFrom).toBe(validFrom);
    });

    test('stores tags and files', async () => {
      const memory = await store.create(
        {
          content: 'Test',
          tags: ['important', 'todo'],
          files: ['src/index.ts'],
        },
        'proj1',
      );

      expect(memory.tags).toEqual(['important', 'todo']);
      expect(memory.files).toEqual(['src/index.ts']);
    });
  });

  describe('deduplication', () => {
    test('reinforces existing memory for duplicate content', async () => {
      const mem1 = await store.create(
        { content: 'The auth module is in src/auth/index.ts file path location' },
        'proj1',
      );

      const initialSalience = mem1.salience;

      const mem2 = await store.create(
        { content: 'The auth module is in src/auth/index.ts file path location' },
        'proj1',
      );

      expect(mem2.id).toBe(mem1.id);
      expect(mem2.salience).toBeGreaterThanOrEqual(initialSalience);
    });
  });

  describe('get', () => {
    test('returns memory by id', async () => {
      const created = await store.create({ content: 'Test content' }, 'proj1');

      const retrieved = await store.get(created.id);

      expect(retrieved).not.toBeNull();
      expect(retrieved?.content).toBe('Test content');
    });

    test('returns null for non-existent id', async () => {
      const retrieved = await store.get('non-existent-id');
      expect(retrieved).toBeNull();
    });
  });

  describe('update', () => {
    test('updates content and recomputes hash', async () => {
      const memory = await store.create({ content: 'Original' }, 'proj1');
      const originalHash = memory.contentHash;

      const updated = await store.update(memory.id, {
        content: 'Updated content',
      });

      expect(updated.content).toBe('Updated content');
      expect(updated.contentHash).not.toBe(originalHash);
    });

    test('updates sector', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');

      const updated = await store.update(memory.id, { sector: 'reflective' });

      expect(updated.sector).toBe('reflective');
    });

    test('updates importance', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');

      const updated = await store.update(memory.id, { importance: 0.9 });

      expect(updated.importance).toBe(0.9);
    });

    test('updates tags', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');

      const updated = await store.update(memory.id, {
        tags: ['new', 'tags'],
      });

      expect(updated.tags).toEqual(['new', 'tags']);
    });
  });

  describe('delete', () => {
    test('soft delete marks as deleted', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');
      await store.delete(memory.id);

      const deleted = await store.get(memory.id);
      expect(deleted?.isDeleted).toBe(true);
      expect(deleted?.deletedAt).toBeDefined();
    });

    test('hard delete removes permanently', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');
      await store.delete(memory.id, true);

      const deleted = await store.get(memory.id);
      expect(deleted).toBeNull();
    });
  });

  describe('restore', () => {
    test('restores soft-deleted memory', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');
      await store.delete(memory.id);
      await store.restore(memory.id);

      const restored = await store.get(memory.id);
      expect(restored?.isDeleted).toBe(false);
      expect(restored?.deletedAt).toBeUndefined();
    });
  });

  describe('list', () => {
    test('returns memories for project', async () => {
      await store.create({ content: 'The authentication flow uses JWT tokens for validation' }, 'proj1');
      await store.create({ content: 'Database migrations are stored in the migrations folder' }, 'proj1');

      const list = await store.list({ projectId: 'proj1', limit: 10 });

      expect(list).toHaveLength(2);
    });

    test('filters by sector', async () => {
      await store.create({ content: 'Test', sector: 'semantic' }, 'proj1');
      await store.create({ content: 'Test', sector: 'episodic' }, 'proj1');

      const list = await store.list({ sector: 'semantic', limit: 10 });

      expect(list).toHaveLength(1);
      expect(list[0]?.sector).toBe('semantic');
    });

    test('filters by tier', async () => {
      await store.create({ content: 'Test', tier: 'session' }, 'proj1');
      await store.create({ content: 'Test', tier: 'project' }, 'proj1');

      const list = await store.list({ tier: 'session', limit: 10 });

      expect(list).toHaveLength(1);
      expect(list[0]?.tier).toBe('session');
    });

    test('filters by minimum salience', async () => {
      const mem1 = await store.create({ content: 'The authentication flow uses JWT tokens for validation' }, 'proj1');
      await store.create({ content: 'Database migrations are stored in the migrations folder' }, 'proj1');
      await store.deemphasize(mem1.id, 0.9);

      const list = await store.list({ minSalience: 0.5, limit: 10 });

      expect(list).toHaveLength(1);
    });

    test('excludes deleted by default', async () => {
      await store.create({ content: 'Visible' }, 'proj1');
      const toDelete = await store.create({ content: 'Hidden' }, 'proj1');
      await store.delete(toDelete.id);

      const list = await store.list({ limit: 10 });
      expect(list).toHaveLength(1);
    });

    test('includes deleted when requested', async () => {
      await store.create({ content: 'Visible' }, 'proj1');
      const toDelete = await store.create({ content: 'Hidden' }, 'proj1');
      await store.delete(toDelete.id);

      const list = await store.list({ limit: 10, includeDeleted: true });
      expect(list).toHaveLength(2);
    });

    test('orders by salience', async () => {
      const mem1 = await store.create({ content: 'Low' }, 'proj1');
      await store.create({ content: 'High' }, 'proj1');
      await store.deemphasize(mem1.id, 0.5);

      const list = await store.list({
        orderBy: 'salience',
        order: 'desc',
        limit: 10,
      });

      expect(list[0]?.salience).toBeGreaterThan(list[1]?.salience ?? 0);
    });

    test('supports pagination', async () => {
      await store.create({ content: 'The authentication flow uses JWT tokens for validation' }, 'proj1');
      await store.create({ content: 'Database migrations are stored in the migrations folder' }, 'proj1');
      await store.create({ content: 'The WebSocket server handles real-time events' }, 'proj1');

      const page1 = await store.list({ limit: 2, offset: 0 });
      const page2 = await store.list({ limit: 2, offset: 2 });

      expect(page1).toHaveLength(2);
      expect(page2).toHaveLength(1);
    });
  });

  describe('touch', () => {
    test('updates last_accessed and access_count', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');
      const originalAccessed = memory.lastAccessed;
      const originalCount = memory.accessCount;

      await new Promise(resolve => setTimeout(resolve, 10));
      await store.touch(memory.id);

      const touched = await store.get(memory.id);
      expect(touched?.lastAccessed).toBeGreaterThan(originalAccessed);
      expect(touched?.accessCount).toBe(originalCount + 1);
    });
  });

  describe('reinforce', () => {
    test('increases salience with diminishing returns', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');
      expect(memory.salience).toBe(1.0);

      const after = await store.reinforce(memory.id, 0.5);
      expect(after.salience).toBe(1.0);

      const mem2 = await store.create({ content: 'Another test' }, 'proj1');
      await store.deemphasize(mem2.id, 0.5);
      const lowSalience = await store.get(mem2.id);
      expect(lowSalience?.salience).toBe(0.5);

      const boosted = await store.reinforce(mem2.id, 0.5);
      expect(boosted.salience).toBe(0.75);
    });

    test('updates access_count', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');

      await store.reinforce(memory.id);

      const reinforced = await store.get(memory.id);
      expect(reinforced?.accessCount).toBe(1);
    });
  });

  describe('deemphasize', () => {
    test('reduces salience with floor', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');

      await store.deemphasize(memory.id, 2.0);

      const after = await store.get(memory.id);
      expect(after?.salience).toBe(0.05);
    });

    test('normal reduction', async () => {
      const memory = await store.create({ content: 'Test' }, 'proj1');

      await store.deemphasize(memory.id, 0.3);

      const after = await store.get(memory.id);
      expect(after?.salience).toBe(0.7);
    });
  });

  describe('session linking', () => {
    test('linkToSession creates record', async () => {
      const now = Date.now();
      await db.execute(`INSERT INTO sessions (id, project_id, started_at) VALUES (?, ?, ?)`, ['sess1', 'proj1', now]);

      const memory = await store.create({ content: 'Test' }, 'proj1', 'sess1');

      const result = await db.execute('SELECT * FROM session_memories WHERE memory_id = ?', [memory.id]);

      expect(result.rows).toHaveLength(1);
      expect(result.rows[0]?.['usage_type']).toBe('created');
    });

    test('getBySession returns linked memories', async () => {
      const now = Date.now();
      await db.execute(`INSERT INTO sessions (id, project_id, started_at) VALUES (?, ?, ?)`, ['sess1', 'proj1', now]);

      await store.create({ content: 'The authentication flow uses JWT tokens for validation' }, 'proj1', 'sess1');
      await store.create({ content: 'Database migrations are stored in the migrations folder' }, 'proj1', 'sess1');
      await store.create({ content: 'The WebSocket server handles real-time events' }, 'proj1');

      const sessionMemories = await store.getBySession('sess1');

      expect(sessionMemories).toHaveLength(2);
    });
  });
});

describe('extractConcepts', () => {
  test('extracts backtick code references', async () => {
    const db = await createDatabase(':memory:');
    setDatabase(db);
    const store = createMemoryStore();

    const now = Date.now();
    await db.execute(`INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`, [
      'proj1',
      '/test',
      'Test',
      now,
      now,
    ]);

    const memory = await store.create({ content: 'Call `handleAuth` and `processRequest`' }, 'proj1');

    expect(memory.concepts).toContain('handleAuth');
    expect(memory.concepts).toContain('processRequest');

    closeDatabase();
  });

  test('extracts file paths', async () => {
    const db = await createDatabase(':memory:');
    setDatabase(db);
    const store = createMemoryStore();

    const now = Date.now();
    await db.execute(`INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`, [
      'proj1',
      '/test',
      'Test',
      now,
      now,
    ]);

    const memory = await store.create({ content: 'Located in /src/auth/handler.ts' }, 'proj1');

    expect(memory.concepts).toContain('src/auth/handler.ts');

    closeDatabase();
  });

  test('extracts camelCase identifiers', async () => {
    const db = await createDatabase(':memory:');
    setDatabase(db);
    const store = createMemoryStore();

    const now = Date.now();
    await db.execute(`INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`, [
      'proj1',
      '/test',
      'Test',
      now,
      now,
    ]);

    const memory = await store.create({ content: 'The UserService class' }, 'proj1');

    expect(memory.concepts).toContain('UserService');

    closeDatabase();
  });
});
