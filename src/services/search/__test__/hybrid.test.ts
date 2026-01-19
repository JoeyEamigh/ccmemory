import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import {
  createDatabase,
  closeDatabase,
  setDatabase,
  type Database,
} from "../../../db/database.js";
import { createMemoryStore, type MemoryStore } from "../../memory/store.js";
import { createRelationship, supersede } from "../../memory/relationships.js";
import { createSearchService, type SearchService } from "../hybrid.js";
import type { EmbeddingService, EmbeddingResult } from "../../embedding/types.js";

function createMockEmbeddingService(): EmbeddingService {
  const mockVector = Array(128).fill(0.1);

  return {
    getProvider: () => ({
      name: "mock",
      model: "test-model",
      dimensions: 128,
      embed: async () => mockVector,
      embedBatch: async () => [],
      isAvailable: async () => true,
    }),
    embed: async (): Promise<EmbeddingResult> => ({
      vector: mockVector,
      model: "test-model",
      dimensions: 128,
      cached: false,
    }),
    embedBatch: async (texts: string[]): Promise<EmbeddingResult[]> =>
      texts.map(() => ({
        vector: mockVector,
        model: "test-model",
        dimensions: 128,
        cached: false,
      })),
    getActiveModelId: () => "mock:test-model",
    switchProvider: async () => {},
  };
}

describe("Hybrid Search", () => {
  let db: Database;
  let store: MemoryStore;
  let search: SearchService;

  beforeEach(async () => {
    db = await createDatabase(":memory:");
    setDatabase(db);
    store = createMemoryStore();
    search = createSearchService(createMockEmbeddingService());

    const now = Date.now();
    await db.execute(
      `INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
      ["proj1", "/test/path", "Test Project", now, now]
    );

    await db.execute(
      `INSERT INTO sessions (id, project_id, started_at) VALUES (?, ?, ?)`,
      ["sess1", "proj1", now]
    );

    await db.execute(
      `INSERT INTO embedding_models (id, name, provider, dimensions, is_active, created_at)
       VALUES (?, ?, ?, ?, ?, ?)`,
      ["mock:test-model", "test-model", "mock", 128, 1, now]
    );

    await store.create(
      {
        content: "Decided to use React with TypeScript for the frontend application",
        sector: "reflective",
      },
      "proj1",
      "sess1"
    );
    await store.create(
      {
        content: "To deploy run npm build then upload artifacts to S3 bucket",
        sector: "procedural",
      },
      "proj1",
      "sess1"
    );
    await store.create(
      {
        content: "The API routes are defined in src/routes/api.ts file",
        sector: "semantic",
      },
      "proj1",
      "sess1"
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  test("returns results from FTS", async () => {
    const results = await search.search({
      query: "React TypeScript",
      projectId: "proj1",
      mode: "keyword",
    });
    expect(results.length).toBeGreaterThan(0);
    expect(results[0]?.matchType).toBe("keyword");
  });

  test("filters by sector", async () => {
    const results = await search.search({
      query: "build deploy",
      projectId: "proj1",
      sector: "procedural",
    });
    expect(results.every((r) => r.memory.sector === "procedural")).toBe(true);
  });

  test("filters by tier", async () => {
    await store.create({ content: "Session tier content", tier: "session" }, "proj1");

    const results = await search.search({
      query: "content",
      projectId: "proj1",
      tier: "session",
    });

    expect(results.every((r) => r.memory.tier === "session")).toBe(true);
  });

  test("filters by minimum salience", async () => {
    const lowSalienceMem = await store.create(
      { content: "Low salience memory for testing minimum filter" },
      "proj1"
    );
    await store.deemphasize(lowSalienceMem.id, 0.9);

    const results = await search.search({
      query: "testing",
      projectId: "proj1",
      minSalience: 0.5,
    });

    expect(
      results.every((r) => r.memory.salience >= 0.5)
    ).toBe(true);
  });

  test("includes related memory count", async () => {
    const mem1 = await store.create(
      { content: "First memory for relationship test" },
      "proj1"
    );
    const mem2 = await store.create(
      { content: "Second memory for relationship test" },
      "proj1"
    );
    await createRelationship(mem1.id, mem2.id, "RELATED_TO", "user");

    const results = await search.search({
      query: "First memory relationship",
      projectId: "proj1",
    });

    const result = results.find((r) => r.memory.id === mem1.id);
    expect(result?.relatedMemoryCount).toBe(1);
  });

  test("excludes superseded memories by default", async () => {
    const oldMem = await store.create(
      { content: "Old API endpoint is at version one" },
      "proj1"
    );
    const newMem = await store.create(
      { content: "New API endpoint is at version two" },
      "proj1"
    );
    await supersede(oldMem.id, newMem.id);

    const results = await search.search({
      query: "API endpoint",
      projectId: "proj1",
    });

    expect(results.find((r) => r.memory.id === oldMem.id)).toBeUndefined();
  });

  test("includes superseded when requested", async () => {
    const oldMem = await store.create(
      { content: "Outdated information about configuration" },
      "proj1"
    );
    const newMem = await store.create(
      { content: "Updated information about configuration" },
      "proj1"
    );
    await supersede(oldMem.id, newMem.id);

    const results = await search.search({
      query: "information configuration",
      projectId: "proj1",
      includeSuperseded: true,
    });

    const oldResult = results.find((r) => r.memory.id === oldMem.id);
    expect(oldResult?.isSuperseded).toBe(true);
    expect(oldResult?.supersededBy?.id).toBe(newMem.id);
  });

  test("respects limit", async () => {
    await store.create({ content: "Extra memory one for limit test" }, "proj1");
    await store.create({ content: "Extra memory two for limit test" }, "proj1");

    const results = await search.search({
      query: "memory",
      projectId: "proj1",
      limit: 2,
    });

    expect(results.length).toBeLessThanOrEqual(2);
  });

  test("boosts salience on retrieval", async () => {
    const mem = await store.create(
      { content: "Memory to be boosted by search" },
      "proj1"
    );
    await store.deemphasize(mem.id, 0.5);
    const before = await store.get(mem.id);

    await search.search({
      query: "boosted",
      projectId: "proj1",
    });

    const after = await store.get(mem.id);
    expect(after?.salience).toBeGreaterThan(before?.salience ?? 0);
  });
});

describe("Timeline", () => {
  let db: Database;
  let store: MemoryStore;
  let search: SearchService;

  beforeEach(async () => {
    db = await createDatabase(":memory:");
    setDatabase(db);
    store = createMemoryStore();
    search = createSearchService(createMockEmbeddingService());

    const now = Date.now();
    await db.execute(
      `INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
      ["proj1", "/test/path", "Test Project", now, now]
    );
    await db.execute(
      `INSERT INTO sessions (id, project_id, started_at) VALUES (?, ?, ?)`,
      ["sess1", "proj1", now]
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  test("returns anchor memory", async () => {
    const anchor = await store.create(
      { content: "Anchor memory for timeline" },
      "proj1"
    );

    const timeline = await search.timeline(anchor.id);

    expect(timeline.anchor.id).toBe(anchor.id);
  });

  test("returns memories before anchor", async () => {
    const before1 = await store.create({ content: "Before one" }, "proj1");
    await new Promise((r) => setTimeout(r, 10));
    const before2 = await store.create({ content: "Before two" }, "proj1");
    await new Promise((r) => setTimeout(r, 10));
    const anchor = await store.create({ content: "Anchor memory" }, "proj1");

    const timeline = await search.timeline(anchor.id, 5, 0);

    expect(timeline.before.length).toBe(2);
    expect(timeline.before[0]?.id).toBe(before1.id);
    expect(timeline.before[1]?.id).toBe(before2.id);
  });

  test("returns memories after anchor", async () => {
    const anchor = await store.create({ content: "Anchor memory" }, "proj1");
    await new Promise((r) => setTimeout(r, 10));
    const after1 = await store.create({ content: "After one" }, "proj1");
    await new Promise((r) => setTimeout(r, 10));
    const after2 = await store.create({ content: "After two" }, "proj1");

    const timeline = await search.timeline(anchor.id, 0, 5);

    expect(timeline.after.length).toBe(2);
    expect(timeline.after[0]?.id).toBe(after1.id);
    expect(timeline.after[1]?.id).toBe(after2.id);
  });

  test("respects depth limits", async () => {
    const uniqueContents = [
      "The authentication system uses JWT tokens for validation",
      "Database migrations run automatically on deployment",
      "WebSocket connections handle real-time updates",
      "Redux store manages application state efficiently",
      "API rate limiting prevents abuse of endpoints",
      "Caching layer improves response time significantly",
      "Logging system captures all error messages",
      "Build pipeline runs tests before deployment",
      "Configuration loaded from environment variables",
      "Health checks verify service availability status",
    ];

    for (const content of uniqueContents) {
      await store.create({ content }, "proj1");
      await new Promise((r) => setTimeout(r, 5));
    }
    const anchor = await store.create({ content: "Anchor memory for depth test" }, "proj1");

    const timeline = await search.timeline(anchor.id, 3, 0);

    expect(timeline.before.length).toBe(3);
  });

  test("throws for non-existent anchor", async () => {
    expect(search.timeline("non-existent-id")).rejects.toThrow(
      "Anchor memory not found"
    );
  });
});

describe("Session Context", () => {
  let db: Database;
  let store: MemoryStore;
  let search: SearchService;

  beforeEach(async () => {
    db = await createDatabase(":memory:");
    setDatabase(db);
    store = createMemoryStore();
    search = createSearchService(createMockEmbeddingService());

    const now = Date.now();
    await db.execute(
      `INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
      ["proj1", "/test/path", "Test Project", now, now]
    );
    await db.execute(
      `INSERT INTO sessions (id, project_id, started_at, summary) VALUES (?, ?, ?, ?)`,
      ["sess1", "proj1", now, "Test session summary"]
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  test("returns session context for memory", async () => {
    const memory = await store.create(
      { content: "Memory with session context" },
      "proj1",
      "sess1"
    );

    const context = await search.getSessionContext(memory.id);

    expect(context).not.toBeNull();
    expect(context?.session.id).toBe("sess1");
    expect(context?.usageType).toBe("created");
  });

  test("returns null for memory without session", async () => {
    const memory = await store.create(
      { content: "Memory without session" },
      "proj1"
    );

    const context = await search.getSessionContext(memory.id);

    expect(context).toBeNull();
  });

  test("includes memories in session count", async () => {
    await store.create({ content: "First memory" }, "proj1", "sess1");
    const memory = await store.create({ content: "Second memory" }, "proj1", "sess1");

    const context = await search.getSessionContext(memory.id);

    expect(context?.memoriesInSession).toBeGreaterThanOrEqual(2);
  });

  test("includes session summary", async () => {
    const memory = await store.create(
      { content: "Memory for summary test" },
      "proj1",
      "sess1"
    );

    const context = await search.getSessionContext(memory.id);

    expect(context?.session.summary).toBe("Test session summary");
  });
});

describe("Ranking", () => {
  let db: Database;
  let store: MemoryStore;
  let search: SearchService;

  beforeEach(async () => {
    db = await createDatabase(":memory:");
    setDatabase(db);
    store = createMemoryStore();
    search = createSearchService(createMockEmbeddingService());

    const now = Date.now();
    await db.execute(
      `INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
      ["proj1", "/test/path", "Test Project", now, now]
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  test("higher salience memories rank higher", async () => {
    const lowSalience = await store.create(
      { content: "Low salience ranking test memory" },
      "proj1"
    );
    const highSalience = await store.create(
      { content: "High salience ranking test memory" },
      "proj1"
    );

    await store.deemphasize(lowSalience.id, 0.5);

    const results = await search.search({
      query: "ranking test memory",
      projectId: "proj1",
    });

    const highIndex = results.findIndex((r) => r.memory.id === highSalience.id);
    const lowIndex = results.findIndex((r) => r.memory.id === lowSalience.id);

    if (highIndex !== -1 && lowIndex !== -1) {
      expect(highIndex).toBeLessThan(lowIndex);
    }
  });
});

describe("Degraded Mode (no embedding service)", () => {
  let db: Database;
  let store: MemoryStore;
  let search: SearchService;

  beforeEach(async () => {
    db = await createDatabase(":memory:");
    setDatabase(db);
    store = createMemoryStore();
    search = createSearchService(null);

    const now = Date.now();
    await db.execute(
      `INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
      ["proj1", "/test/path", "Test Project", now, now]
    );
    await db.execute(
      `INSERT INTO sessions (id, project_id, started_at) VALUES (?, ?, ?)`,
      ["sess1", "proj1", now]
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  test("falls back to keyword search when no embedding service", async () => {
    await store.create(
      { content: "React TypeScript frontend application" },
      "proj1",
      "sess1"
    );

    const results = await search.search({
      query: "React TypeScript",
      projectId: "proj1",
    });

    expect(results.length).toBeGreaterThan(0);
    expect(results.every((r) => r.matchType === "keyword")).toBe(true);
  });

  test("handles hybrid mode request gracefully", async () => {
    await store.create(
      { content: "Testing graceful degradation with hybrid mode" },
      "proj1",
      "sess1"
    );

    const results = await search.search({
      query: "graceful degradation",
      projectId: "proj1",
      mode: "hybrid",
    });

    expect(results.length).toBeGreaterThan(0);
    expect(results.every((r) => r.matchType === "keyword")).toBe(true);
  });

  test("handles semantic mode request gracefully", async () => {
    await store.create(
      { content: "Semantic search fallback test" },
      "proj1",
      "sess1"
    );

    const results = await search.search({
      query: "semantic search",
      projectId: "proj1",
      mode: "semantic",
    });

    expect(results.every((r) => r.matchType === "keyword")).toBe(true);
  });

  test("keyword mode works normally", async () => {
    await store.create(
      { content: "Keyword only mode test memory" },
      "proj1",
      "sess1"
    );

    const results = await search.search({
      query: "Keyword mode test",
      projectId: "proj1",
      mode: "keyword",
    });

    expect(results.length).toBeGreaterThan(0);
    expect(results.every((r) => r.matchType === "keyword")).toBe(true);
  });

  test("timeline works without embedding service", async () => {
    const anchor = await store.create(
      { content: "Anchor for degraded timeline" },
      "proj1",
      "sess1"
    );

    const timeline = await search.timeline(anchor.id);

    expect(timeline.anchor.id).toBe(anchor.id);
  });

  test("getSessionContext works without embedding service", async () => {
    const memory = await store.create(
      { content: "Memory for session context test" },
      "proj1",
      "sess1"
    );

    const context = await search.getSessionContext(memory.id);

    expect(context).not.toBeNull();
    expect(context?.session.id).toBe("sess1");
  });
});
