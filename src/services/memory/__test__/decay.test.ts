import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import {
  createDatabase,
  closeDatabase,
  setDatabase,
  type Database,
} from "../../../db/database.js";
import {
  calculateDecay,
  calculateSalienceBoost,
  applyDecay,
  getMemoriesForDecay,
  startDecayProcess,
  getDecayRateForSector,
  estimateTimeToDecay,
} from "../decay.js";
import type { Memory } from "../types.js";
import { SECTOR_DECAY_RATES } from "../types.js";

function createMockMemory(overrides: Partial<Memory> = {}): Memory {
  return {
    id: "test-id",
    projectId: "proj1",
    content: "Test content",
    sector: "semantic",
    tier: "project",
    importance: 0.5,
    categories: [],
    salience: 1.0,
    accessCount: 0,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    lastAccessed: Date.now(),
    isDeleted: false,
    tags: [],
    concepts: [],
    files: [],
    ...overrides,
  };
}

describe("calculateDecay", () => {
  test("returns same salience for recent memory", () => {
    const memory = createMockMemory({
      lastAccessed: Date.now(),
      salience: 1.0,
    });

    const decayed = calculateDecay(memory);

    expect(decayed).toBeCloseTo(1.0, 1);
  });

  test("emotional memories decay slowest", () => {
    const now = Date.now();
    const sevenDaysAgo = now - 7 * 24 * 60 * 60 * 1000;

    const emotional = createMockMemory({
      sector: "emotional",
      salience: 1.0,
      lastAccessed: sevenDaysAgo,
    });

    const episodic = createMockMemory({
      sector: "episodic",
      salience: 1.0,
      lastAccessed: sevenDaysAgo,
    });

    expect(calculateDecay(emotional)).toBeGreaterThan(calculateDecay(episodic));
  });

  test("higher importance slows decay", () => {
    const now = Date.now();
    const thirtyDaysAgo = now - 30 * 24 * 60 * 60 * 1000;

    const highImportance = createMockMemory({
      importance: 0.9,
      salience: 1.0,
      lastAccessed: thirtyDaysAgo,
    });

    const lowImportance = createMockMemory({
      importance: 0.1,
      salience: 1.0,
      lastAccessed: thirtyDaysAgo,
    });

    expect(calculateDecay(highImportance)).toBeGreaterThan(
      calculateDecay(lowImportance)
    );
  });

  test("access count provides protection", () => {
    const now = Date.now();
    const thirtyDaysAgo = now - 30 * 24 * 60 * 60 * 1000;

    const highAccess = createMockMemory({
      accessCount: 50,
      salience: 0.5,
      lastAccessed: thirtyDaysAgo,
    });

    const lowAccess = createMockMemory({
      accessCount: 1,
      salience: 0.5,
      lastAccessed: thirtyDaysAgo,
    });

    expect(calculateDecay(highAccess)).toBeGreaterThan(calculateDecay(lowAccess));
  });

  test("salience has minimum floor", () => {
    const ancient = createMockMemory({
      salience: 1.0,
      lastAccessed: Date.now() - 365 * 24 * 60 * 60 * 1000,
      accessCount: 0,
      importance: 0.1,
    });

    expect(calculateDecay(ancient)).toBeGreaterThanOrEqual(0.05);
  });

  test("different sectors have different decay rates", () => {
    const now = Date.now();
    const fourteenDaysAgo = now - 14 * 24 * 60 * 60 * 1000;

    const sectors: Array<Memory["sector"]> = [
      "emotional",
      "semantic",
      "reflective",
      "procedural",
      "episodic",
    ];

    const decayed = sectors.map((sector) =>
      calculateDecay(
        createMockMemory({
          sector,
          salience: 1.0,
          lastAccessed: fourteenDaysAgo,
        })
      )
    );

    expect(decayed[0]).toBeGreaterThan(decayed[4] ?? 0);
  });
});

describe("calculateSalienceBoost", () => {
  test("boosts low salience more", () => {
    const lowBoost = calculateSalienceBoost(0.3, 0.5);
    const highBoost = calculateSalienceBoost(0.8, 0.5);

    expect(lowBoost - 0.3).toBeGreaterThan(highBoost - 0.8);
  });

  test("never exceeds 1.0", () => {
    const boosted = calculateSalienceBoost(0.9, 0.5);
    expect(boosted).toBeLessThanOrEqual(1.0);
  });

  test("returns exactly 1.0 when already at max", () => {
    const boosted = calculateSalienceBoost(1.0, 0.5);
    expect(boosted).toBe(1.0);
  });
});

describe("Decay with Database", () => {
  let db: Database;

  beforeEach(async () => {
    db = await createDatabase(":memory:");
    setDatabase(db);

    const now = Date.now();
    await db.execute(
      `INSERT INTO projects (id, path, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
      ["proj1", "/test/path", "Test Project", now, now]
    );
  });

  afterEach(() => {
    closeDatabase();
  });

  async function insertMemory(
    id: string,
    salience: number,
    lastAccessed: number
  ): Promise<void> {
    const now = Date.now();
    await db.execute(
      `INSERT INTO memories (
        id, project_id, content, sector, tier, importance,
        salience, access_count, created_at, updated_at, last_accessed,
        is_deleted, tags_json, concepts_json, files_json, categories_json
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        id,
        "proj1",
        "Test content",
        "semantic",
        "project",
        0.5,
        salience,
        0,
        now,
        now,
        lastAccessed,
        0,
        "[]",
        "[]",
        "[]",
        "[]",
      ]
    );
  }

  test("applyDecay updates salience in database", async () => {
    const oneMonthAgo = Date.now() - 30 * 24 * 60 * 60 * 1000;
    await insertMemory("mem1", 1.0, oneMonthAgo);

    const memories = await getMemoriesForDecay(10);
    await applyDecay(memories);

    const result = await db.execute(
      "SELECT salience FROM memories WHERE id = ?",
      ["mem1"]
    );

    const newSalience = Number(result.rows[0]?.["salience"]);
    expect(newSalience).toBeLessThan(1.0);
    expect(newSalience).toBeGreaterThan(0.05);
  });

  test("getMemoriesForDecay excludes deleted memories", async () => {
    await insertMemory("mem1", 0.5, Date.now());
    await db.execute("UPDATE memories SET is_deleted = 1 WHERE id = ?", ["mem1"]);

    const memories = await getMemoriesForDecay(10);

    expect(memories).toHaveLength(0);
  });

  test("getMemoriesForDecay excludes memories at floor", async () => {
    await insertMemory("mem1", 0.05, Date.now());

    const memories = await getMemoriesForDecay(10);

    expect(memories).toHaveLength(0);
  });

  test("getMemoriesForDecay orders by updated_at", async () => {
    const now = Date.now();
    await insertMemory("mem1", 0.5, now);
    await db.execute("UPDATE memories SET updated_at = ? WHERE id = ?", [
      now - 1000,
      "mem1",
    ]);

    await insertMemory("mem2", 0.5, now);
    await db.execute("UPDATE memories SET updated_at = ? WHERE id = ?", [
      now - 2000,
      "mem2",
    ]);

    const memories = await getMemoriesForDecay(10);

    expect(memories[0]?.id).toBe("mem2");
    expect(memories[1]?.id).toBe("mem1");
  });
});

describe("startDecayProcess", () => {
  test("returns stop function when disabled", () => {
    const stop = startDecayProcess({ enabled: false });
    expect(typeof stop).toBe("function");
    stop();
  });

  test("returns stop function when enabled", () => {
    const stop = startDecayProcess({ enabled: true, interval: 1000000 });
    expect(typeof stop).toBe("function");
    stop();
  });
});

describe("getDecayRateForSector", () => {
  test("returns correct rate for each sector", () => {
    expect(getDecayRateForSector("emotional")).toBe(SECTOR_DECAY_RATES.emotional);
    expect(getDecayRateForSector("semantic")).toBe(SECTOR_DECAY_RATES.semantic);
    expect(getDecayRateForSector("reflective")).toBe(SECTOR_DECAY_RATES.reflective);
    expect(getDecayRateForSector("procedural")).toBe(SECTOR_DECAY_RATES.procedural);
    expect(getDecayRateForSector("episodic")).toBe(SECTOR_DECAY_RATES.episodic);
  });
});

describe("estimateTimeToDecay", () => {
  test("returns 0 when already below target", () => {
    const memory = createMockMemory({ salience: 0.3 });
    expect(estimateTimeToDecay(memory, 0.5)).toBe(0);
  });

  test("returns positive time for higher salience", () => {
    const memory = createMockMemory({
      salience: 1.0,
      accessCount: 0,
    });

    const time = estimateTimeToDecay(memory, 0.5);
    expect(time).toBeGreaterThan(0);
  });

  test("higher access count increases decay time", () => {
    const lowAccess = createMockMemory({
      salience: 1.0,
      accessCount: 1,
    });

    const highAccess = createMockMemory({
      salience: 1.0,
      accessCount: 100,
    });

    const timeLow = estimateTimeToDecay(lowAccess, 0.1);
    const timeHigh = estimateTimeToDecay(highAccess, 0.1);

    expect(timeHigh).toBeGreaterThan(timeLow);
  });
});
