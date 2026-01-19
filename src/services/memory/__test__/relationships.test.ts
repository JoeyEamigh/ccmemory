import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import {
  createDatabase,
  closeDatabase,
  setDatabase,
  type Database,
} from "../../../db/database.js";
import {
  createRelationship,
  getRelationships,
  getRelatedMemories,
  supersede,
  getSupersedingMemory,
  invalidateRelationship,
  isValidRelationshipType,
  RELATIONSHIP_TYPES,
} from "../relationships.js";

describe("Memory Relationships", () => {
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

  async function createTestMemory(
    id: string,
    content: string,
    projectId = "proj1"
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
        projectId,
        content,
        "semantic",
        "project",
        0.5,
        1.0,
        0,
        now,
        now,
        now,
        0,
        "[]",
        "[]",
        "[]",
        "[]",
      ]
    );
  }

  test("createRelationship creates valid relationship", async () => {
    await createTestMemory("mem1", "Old fact");
    await createTestMemory("mem2", "New fact");

    const rel = await createRelationship("mem2", "mem1", "SUPERSEDES", "user");

    expect(rel.sourceMemoryId).toBe("mem2");
    expect(rel.targetMemoryId).toBe("mem1");
    expect(rel.relationshipType).toBe("SUPERSEDES");
    expect(rel.extractedBy).toBe("user");
    expect(rel.confidence).toBe(1.0);
  });

  test("createRelationship with custom confidence", async () => {
    await createTestMemory("mem1", "Fact 1");
    await createTestMemory("mem2", "Fact 2");

    const rel = await createRelationship(
      "mem1",
      "mem2",
      "RELATED_TO",
      "llm",
      0.85
    );

    expect(rel.confidence).toBe(0.85);
    expect(rel.extractedBy).toBe("llm");
  });

  test("getRelationships returns all relationships for memory", async () => {
    await createTestMemory("mem1", "Center");
    await createTestMemory("mem2", "Related 1");
    await createTestMemory("mem3", "Related 2");

    await createRelationship("mem1", "mem2", "RELATED_TO");
    await createRelationship("mem3", "mem1", "BUILDS_ON");

    const relationships = await getRelationships("mem1");

    expect(relationships).toHaveLength(2);
  });

  test("getRelationships excludes invalidated relationships", async () => {
    await createTestMemory("mem1", "Fact 1");
    await createTestMemory("mem2", "Fact 2");

    const rel = await createRelationship("mem1", "mem2", "RELATED_TO");
    await invalidateRelationship(rel.id);

    const relationships = await getRelationships("mem1");

    expect(relationships).toHaveLength(0);
  });

  test("getRelatedMemories returns related memory objects", async () => {
    await createTestMemory("mem1", "Main memory");
    await createTestMemory("mem2", "Related memory");

    await createRelationship("mem1", "mem2", "RELATED_TO");

    const related = await getRelatedMemories("mem1");

    expect(related).toHaveLength(1);
    expect(related[0]?.id).toBe("mem2");
    expect(related[0]?.content).toBe("Related memory");
  });

  test("getRelatedMemories filters by relationship type", async () => {
    await createTestMemory("mem1", "Main");
    await createTestMemory("mem2", "Related");
    await createTestMemory("mem3", "Builds on");

    await createRelationship("mem1", "mem2", "RELATED_TO");
    await createRelationship("mem3", "mem1", "BUILDS_ON");

    const related = await getRelatedMemories("mem1", "RELATED_TO");
    expect(related).toHaveLength(1);
    expect(related[0]?.id).toBe("mem2");

    const buildsOn = await getRelatedMemories("mem1", "BUILDS_ON");
    expect(buildsOn).toHaveLength(1);
    expect(buildsOn[0]?.id).toBe("mem3");
  });

  test("getRelatedMemories excludes deleted memories", async () => {
    await createTestMemory("mem1", "Main");
    await createTestMemory("mem2", "To be deleted");

    await createRelationship("mem1", "mem2", "RELATED_TO");

    await db.execute("UPDATE memories SET is_deleted = 1 WHERE id = ?", [
      "mem2",
    ]);

    const related = await getRelatedMemories("mem1");
    expect(related).toHaveLength(0);
  });

  test("supersede marks old memory with valid_until", async () => {
    await createTestMemory("old", "Old fact");
    await createTestMemory("new", "New fact");

    await supersede("old", "new");

    const result = await db.execute(
      "SELECT valid_until FROM memories WHERE id = ?",
      ["old"]
    );
    expect(result.rows[0]?.["valid_until"]).toBeDefined();
    expect(result.rows[0]?.["valid_until"]).not.toBeNull();
  });

  test("supersede creates SUPERSEDES relationship", async () => {
    await createTestMemory("old", "Old fact");
    await createTestMemory("new", "New fact");

    await supersede("old", "new");

    const relationships = await getRelationships("old");
    expect(relationships).toHaveLength(1);
    expect(relationships[0]?.relationshipType).toBe("SUPERSEDES");
    expect(relationships[0]?.sourceMemoryId).toBe("new");
    expect(relationships[0]?.targetMemoryId).toBe("old");
  });

  test("getSupersedingMemory returns newer version", async () => {
    await createTestMemory("old", "Old fact");
    await createTestMemory("new", "New fact");

    await supersede("old", "new");

    const superseding = await getSupersedingMemory("old");
    expect(superseding).not.toBeNull();
    expect(superseding?.id).toBe("new");
    expect(superseding?.content).toBe("New fact");
  });

  test("getSupersedingMemory returns null when not superseded", async () => {
    await createTestMemory("mem1", "Standalone fact");

    const superseding = await getSupersedingMemory("mem1");
    expect(superseding).toBeNull();
  });

  test("getSupersedingMemory excludes deleted superseding memories", async () => {
    await createTestMemory("old", "Old fact");
    await createTestMemory("new", "New fact");

    await supersede("old", "new");
    await db.execute("UPDATE memories SET is_deleted = 1 WHERE id = ?", ["new"]);

    const superseding = await getSupersedingMemory("old");
    expect(superseding).toBeNull();
  });

  test("invalidateRelationship sets valid_until", async () => {
    await createTestMemory("mem1", "Fact 1");
    await createTestMemory("mem2", "Fact 2");

    const rel = await createRelationship("mem1", "mem2", "RELATED_TO");
    await invalidateRelationship(rel.id);

    const result = await db.execute(
      "SELECT valid_until FROM memory_relationships WHERE id = ?",
      [rel.id]
    );
    expect(result.rows[0]?.["valid_until"]).toBeDefined();
  });
});

describe("Relationship Type Validation", () => {
  test("isValidRelationshipType accepts valid types", () => {
    for (const type of RELATIONSHIP_TYPES) {
      expect(isValidRelationshipType(type)).toBe(true);
    }
  });

  test("isValidRelationshipType rejects invalid types", () => {
    expect(isValidRelationshipType("INVALID")).toBe(false);
    expect(isValidRelationshipType("")).toBe(false);
    expect(isValidRelationshipType("supersedes")).toBe(false);
  });

  test("RELATIONSHIP_TYPES contains all expected types", () => {
    expect(RELATIONSHIP_TYPES).toContain("SUPERSEDES");
    expect(RELATIONSHIP_TYPES).toContain("CONTRADICTS");
    expect(RELATIONSHIP_TYPES).toContain("RELATED_TO");
    expect(RELATIONSHIP_TYPES).toContain("BUILDS_ON");
    expect(RELATIONSHIP_TYPES).toContain("CONFIRMS");
    expect(RELATIONSHIP_TYPES).toContain("APPLIES_TO");
    expect(RELATIONSHIP_TYPES).toContain("DEPENDS_ON");
    expect(RELATIONSHIP_TYPES).toContain("ALTERNATIVE_TO");
  });
});
