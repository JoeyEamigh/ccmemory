import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { closeDatabase, createDatabase, setDatabase, type Database } from '../../../db/database.js';
import { createMemoryStore, type MemoryStore } from '../../memory/store.js';
import { searchFTS } from '../fts.js';

describe('FTS5 Search', () => {
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

    await db.execute(`INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`, [
      'proj2',
      '/test/path2',
      'Test Project 2',
      now,
      now,
    ]);

    await store.create(
      {
        content: 'The authentication module handles user login and JWT tokens for secure access',
      },
      'proj1',
    );
    await store.create(
      {
        content: 'Database migrations are run with the migrate command to update schema',
      },
      'proj1',
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  test('finds memories by keyword', async () => {
    const results = await searchFTS('authentication', 'proj1');
    expect(results.length).toBe(1);
    expect(results[0]?.memoryId).toBeDefined();
  });

  test('supports prefix matching', async () => {
    const results = await searchFTS('auth', 'proj1');
    expect(results.length).toBe(1);
  });

  test('returns snippet with content', async () => {
    const results = await searchFTS('authentication', 'proj1');
    expect(results[0]?.snippet).toBeDefined();
    expect(results[0]?.snippet.length).toBeGreaterThan(0);
  });

  test('returns highlighted snippets', async () => {
    const results = await searchFTS('authentication', 'proj1');
    expect(results[0]?.snippet).toContain('<mark>');
    expect(results[0]?.snippet).toContain('</mark>');
  });

  test('filters by project', async () => {
    await store.create({ content: 'authentication in another project for different users' }, 'proj2');

    const results = await searchFTS('authentication', 'proj1');
    expect(results.length).toBe(1);
  });

  test('finds across projects when no filter', async () => {
    await store.create({ content: 'authentication in another project for different users' }, 'proj2');

    const results = await searchFTS('authentication');
    expect(results.length).toBe(2);
  });

  test('handles empty query', async () => {
    const results = await searchFTS('', 'proj1');
    expect(results).toEqual([]);
  });

  test('handles single character query', async () => {
    const results = await searchFTS('a', 'proj1');
    expect(results).toEqual([]);
  });

  test('finds multiple matching memories', async () => {
    await store.create({ content: 'User management with authentication tokens for API access' }, 'proj1');

    const results = await searchFTS('authentication', 'proj1');
    expect(results.length).toBe(2);
  });

  test('returns results ordered by relevance', async () => {
    const results = await searchFTS('migrate database', 'proj1');
    expect(results.length).toBeGreaterThan(0);
    expect(results[0]?.rank).toBeDefined();
  });

  test('excludes deleted memories', async () => {
    const mem = await store.create({ content: 'authentication system for deleted content' }, 'proj1');
    await store.delete(mem.id);

    const results = await searchFTS('deleted', 'proj1');
    expect(results.length).toBe(0);
  });

  test('finds by multiple keywords', async () => {
    const results = await searchFTS('JWT tokens', 'proj1');
    expect(results.length).toBe(1);
    expect(results[0]?.snippet).toContain('JWT');
  });

  test('handles special characters without crashing', async () => {
    // Special characters should be escaped/handled gracefully
    const results = await searchFTS("user's login", 'proj1');
    // Should find the memory with "user login" despite the apostrophe
    expect(results.length).toBe(1);
    expect(results[0]?.snippet).toContain('login');
  });

  test('handles query with only special characters', async () => {
    // Pure special characters should return empty, not throw
    const results = await searchFTS('!@#$%', 'proj1');
    expect(results).toEqual([]);
  });

  test('respects limit parameter', async () => {
    await store.create({ content: 'authentication system one for testing' }, 'proj1');
    await store.create({ content: 'authentication system two for testing' }, 'proj1');
    await store.create({ content: 'authentication system three for testing' }, 'proj1');

    const results = await searchFTS('authentication', 'proj1', 2);
    expect(results.length).toBe(2);
  });
});
