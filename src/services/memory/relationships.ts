import { getDatabase } from "../../db/database.js";
import { log } from "../../utils/log.js";
import type { Memory } from "./types.js";
import { rowToMemory } from "./utils.js";

export type RelationshipType =
  | "SUPERSEDES"
  | "CONTRADICTS"
  | "RELATED_TO"
  | "BUILDS_ON"
  | "CONFIRMS"
  | "APPLIES_TO"
  | "DEPENDS_ON"
  | "ALTERNATIVE_TO";

export type ExtractedBy = "user" | "llm" | "system";

export type MemoryRelationship = {
  id: string;
  sourceMemoryId: string;
  targetMemoryId: string;
  relationshipType: RelationshipType;
  createdAt: number;
  validFrom: number;
  validUntil?: number;
  confidence: number;
  extractedBy: ExtractedBy;
};

export const RELATIONSHIP_TYPES: RelationshipType[] = [
  "SUPERSEDES",
  "CONTRADICTS",
  "RELATED_TO",
  "BUILDS_ON",
  "CONFIRMS",
  "APPLIES_TO",
  "DEPENDS_ON",
  "ALTERNATIVE_TO",
];

export function isValidRelationshipType(type: string): type is RelationshipType {
  return RELATIONSHIP_TYPES.includes(type as RelationshipType);
}

export async function createRelationship(
  sourceId: string,
  targetId: string,
  type: RelationshipType,
  extractedBy: ExtractedBy = "system",
  confidence = 1.0
): Promise<MemoryRelationship> {
  const db = await getDatabase();
  const id = crypto.randomUUID();
  const now = Date.now();

  log.debug("memory", "Creating relationship", {
    sourceId,
    targetId,
    type,
    extractedBy,
  });

  await db.execute(
    `INSERT INTO memory_relationships (
      id, source_memory_id, target_memory_id, relationship_type,
      created_at, valid_from, confidence, extracted_by
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    [id, sourceId, targetId, type, now, now, confidence, extractedBy]
  );

  log.info("memory", "Relationship created", { id, type });

  return {
    id,
    sourceMemoryId: sourceId,
    targetMemoryId: targetId,
    relationshipType: type,
    createdAt: now,
    validFrom: now,
    confidence,
    extractedBy,
  };
}

export async function getRelationships(
  memoryId: string
): Promise<MemoryRelationship[]> {
  const db = await getDatabase();

  const result = await db.execute(
    `SELECT * FROM memory_relationships
     WHERE (source_memory_id = ? OR target_memory_id = ?)
       AND valid_until IS NULL
     ORDER BY created_at DESC`,
    [memoryId, memoryId]
  );

  return result.rows.map((row) => ({
    id: String(row["id"]),
    sourceMemoryId: String(row["source_memory_id"]),
    targetMemoryId: String(row["target_memory_id"]),
    relationshipType: String(row["relationship_type"]) as RelationshipType,
    createdAt: Number(row["created_at"]),
    validFrom: Number(row["valid_from"]),
    validUntil: row["valid_until"] ? Number(row["valid_until"]) : undefined,
    confidence: Number(row["confidence"]),
    extractedBy: String(row["extracted_by"]) as ExtractedBy,
  }));
}

export async function getRelatedMemories(
  memoryId: string,
  relationshipType?: RelationshipType
): Promise<Memory[]> {
  const db = await getDatabase();

  let sql = `
    SELECT m.* FROM memories m
    JOIN memory_relationships r ON (
      (r.source_memory_id = ? AND r.target_memory_id = m.id) OR
      (r.target_memory_id = ? AND r.source_memory_id = m.id)
    )
    WHERE r.valid_until IS NULL
      AND m.is_deleted = 0
  `;
  const args: (string | number)[] = [memoryId, memoryId];

  if (relationshipType) {
    sql += " AND r.relationship_type = ?";
    args.push(relationshipType);
  }

  sql += " ORDER BY r.created_at DESC";

  const result = await db.execute(sql, args);
  return result.rows.map(rowToMemory);
}

export async function supersede(
  oldMemoryId: string,
  newMemoryId: string
): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  log.info("memory", "Superseding memory", { oldMemoryId, newMemoryId });

  await db.execute(
    `UPDATE memories SET valid_until = ?, updated_at = ?
     WHERE id = ? AND valid_until IS NULL`,
    [now, now, oldMemoryId]
  );

  await createRelationship(newMemoryId, oldMemoryId, "SUPERSEDES", "system");
}

export async function getSupersedingMemory(
  memoryId: string
): Promise<Memory | null> {
  const db = await getDatabase();

  const result = await db.execute(
    `SELECT m.* FROM memories m
     JOIN memory_relationships r ON r.source_memory_id = m.id
     WHERE r.target_memory_id = ?
       AND r.relationship_type = 'SUPERSEDES'
       AND r.valid_until IS NULL
       AND m.is_deleted = 0
     ORDER BY r.created_at DESC
     LIMIT 1`,
    [memoryId]
  );

  if (result.rows.length === 0) return null;
  const row = result.rows[0];
  if (!row) return null;
  return rowToMemory(row);
}

export async function invalidateRelationship(relationshipId: string): Promise<void> {
  const db = await getDatabase();
  const now = Date.now();

  await db.execute(
    `UPDATE memory_relationships SET valid_until = ? WHERE id = ?`,
    [now, relationshipId]
  );
}
