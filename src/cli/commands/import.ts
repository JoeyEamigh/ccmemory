import { parseArgs } from "util";
import { getOrCreateProject } from "../../services/project.js";
import { createDocumentService } from "../../services/documents/ingest.js";
import { createEmbeddingService } from "../../services/embedding/index.js";
import { getDatabase } from "../../db/database.js";
import { log } from "../../utils/log.js";

export async function importCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      project: { type: "string", short: "p" },
      title: { type: "string", short: "t" },
    },
    allowPositionals: true,
  });

  const filePath = positionals[0];
  if (!filePath) {
    console.error("Usage: ccmemory import <file> [-p project] [--title title]");
    process.exit(1);
  }

  const cwd = values.project ?? process.cwd();
  const project = await getOrCreateProject(cwd);

  const embeddingService = await createEmbeddingService();
  const docs = createDocumentService(embeddingService);

  log.info("cli", "Importing document", { path: filePath, project: project.id });

  const doc = await docs.ingest({
    projectId: project.id,
    path: filePath,
    title: values.title,
  });

  const db = await getDatabase();
  const chunks = await db.execute(
    "SELECT COUNT(*) as count FROM document_chunks WHERE document_id = ?",
    [doc.id]
  );
  const chunkCount = Number(chunks.rows[0]?.["count"] ?? 0);

  log.info("cli", "Document imported", { id: doc.id, title: doc.title });
  console.log(`Imported: ${doc.title ?? doc.id}`);
  console.log(`Chunks: ${chunkCount}`);
}

export async function exportCommand(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: {
      project: { type: "string", short: "p" },
      format: { type: "string", short: "f", default: "json" },
      output: { type: "string", short: "o" },
    },
  });

  log.debug("cli", "Export command", {
    project: values.project,
    format: values.format,
  });

  const db = await getDatabase();

  let sql = "SELECT * FROM memories WHERE is_deleted = 0";
  const sqlArgs: string[] = [];

  if (values.project) {
    const project = await getOrCreateProject(values.project);
    sql += " AND project_id = ?";
    sqlArgs.push(project.id);
  }

  sql += " ORDER BY created_at DESC";

  const result = await db.execute(sql, sqlArgs);

  type MemoryRow = {
    id: string;
    sector: string;
    tier: string;
    salience: number;
    content: string;
    created_at: number;
  };

  const memories: MemoryRow[] = result.rows.map((row) => ({
    id: String(row["id"]),
    sector: String(row["sector"]),
    tier: String(row["tier"]),
    salience: Number(row["salience"]),
    content: String(row["content"]),
    created_at: Number(row["created_at"]),
  }));

  let output: string;
  if (values.format === "json") {
    output = JSON.stringify(memories, null, 2);
  } else if (values.format === "csv") {
    const headers = ["id", "sector", "tier", "salience", "content", "created_at"];
    const rows = memories.map((m) => [
      m.id,
      m.sector,
      m.tier,
      m.salience,
      `"${m.content.replace(/"/g, '""')}"`,
      m.created_at,
    ]);
    output = [headers.join(","), ...rows.map((r) => r.join(","))].join("\n");
  } else {
    console.error("Unsupported format. Use: json, csv");
    process.exit(1);
  }

  if (values.output) {
    await Bun.write(values.output, output);
    log.info("cli", "Memories exported to file", {
      count: memories.length,
      path: values.output,
    });
    console.log(`Exported ${memories.length} memories to ${values.output}`);
  } else {
    log.info("cli", "Memories exported to stdout", { count: memories.length });
    console.log(output);
  }
}
