import { getDatabase } from "../../db/database.js";
import { log } from "../../utils/log.js";
import type { EmbeddingService } from "../embedding/types.js";

export type VectorResult = {
  memoryId: string;
  distance: number;
  similarity: number;
};

function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length) return 0;

  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    const aVal = a[i] ?? 0;
    const bVal = b[i] ?? 0;
    dotProduct += aVal * bVal;
    normA += aVal * aVal;
    normB += bVal * bVal;
  }

  const magnitude = Math.sqrt(normA) * Math.sqrt(normB);
  if (magnitude === 0) return 0;

  return dotProduct / magnitude;
}

function parseVector(blob: unknown): number[] {
  if (blob instanceof Uint8Array || blob instanceof ArrayBuffer) {
    const buffer = blob instanceof ArrayBuffer ? blob : blob.buffer;
    return Array.from(new Float32Array(buffer));
  }

  if (typeof blob === "string") {
    try {
      return JSON.parse(blob) as number[];
    } catch {
      return [];
    }
  }

  if (Array.isArray(blob)) {
    return blob as number[];
  }

  return [];
}

export async function searchVector(
  query: string,
  embeddingService: EmbeddingService,
  projectId?: string,
  limit = 20
): Promise<VectorResult[]> {
  const db = await getDatabase();
  const start = Date.now();

  log.debug("search", "Vector search starting", {
    queryLength: query.length,
    projectId,
    limit,
  });

  const queryEmbedding = await embeddingService.embed(query);
  const modelId = embeddingService.getActiveModelId();

  log.debug("search", "Query embedded", {
    model: modelId,
    ms: Date.now() - start,
  });

  let sql = `
    SELECT
      mv.memory_id,
      mv.vector
    FROM memory_vectors mv
    JOIN memories m ON mv.memory_id = m.id
    WHERE mv.model_id = ?
      AND m.is_deleted = 0
  `;
  const args: (string | number)[] = [modelId];

  if (projectId) {
    sql += " AND m.project_id = ?";
    args.push(projectId);
  }

  const result = await db.execute(sql, args);

  const scored: VectorResult[] = [];

  for (const row of result.rows) {
    const memoryId = String(row["memory_id"]);
    const vectorData = row["vector"];
    const vector = parseVector(vectorData);

    if (vector.length !== queryEmbedding.dimensions) {
      continue;
    }

    const similarity = cosineSimilarity(queryEmbedding.vector, vector);
    const distance = 1 - similarity;

    scored.push({
      memoryId,
      distance,
      similarity,
    });
  }

  scored.sort((a, b) => b.similarity - a.similarity);

  const topResults = scored.slice(0, limit);

  log.info("search", "Vector search complete", {
    candidates: result.rows.length,
    results: topResults.length,
    ms: Date.now() - start,
  });

  return topResults;
}
