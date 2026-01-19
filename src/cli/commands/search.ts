import { parseArgs } from "util";
import { createSearchService } from "../../services/search/hybrid.js";
import { createEmbeddingService } from "../../services/embedding/index.js";
import { getOrCreateProject } from "../../services/project.js";
import { log } from "../../utils/log.js";
import type { MemorySector } from "../../services/memory/types.js";

export async function searchCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      project: { type: "string", short: "p" },
      sector: { type: "string", short: "s" },
      limit: { type: "string", short: "l", default: "10" },
      semantic: { type: "boolean" },
      keywords: { type: "boolean" },
      json: { type: "boolean" },
    },
    allowPositionals: true,
  });

  const query = positionals.join(" ");
  if (!query) {
    console.error("Usage: ccmemory search <query> [-p project] [-s sector]");
    process.exit(1);
  }

  const embeddingService = await createEmbeddingService();
  const search = createSearchService(embeddingService);

  let projectId: string | undefined;
  if (values.project) {
    const project = await getOrCreateProject(values.project);
    projectId = project.id;
  }

  const mode = values.semantic
    ? "semantic"
    : values.keywords
      ? "keyword"
      : "hybrid";

  log.debug("cli", "Search command", {
    query: query.slice(0, 50),
    mode,
    projectId,
    limit: values.limit,
  });

  const results = await search.search({
    query,
    projectId,
    sector: values.sector as MemorySector | undefined,
    limit: parseInt(values.limit as string, 10),
    mode,
  });

  log.info("cli", "Search complete", {
    results: results.length,
    query: query.slice(0, 30),
  });

  if (values.json) {
    console.log(JSON.stringify(results, null, 2));
  } else {
    if (results.length === 0) {
      console.log("No memories found.");
      return;
    }

    for (const result of results) {
      const mem = result.memory;
      console.log(`\n${"─".repeat(60)}`);
      console.log(`ID: ${mem.id}`);
      console.log(
        `Sector: ${mem.sector} | Score: ${result.score.toFixed(3)} | Salience: ${mem.salience.toFixed(2)}`
      );
      console.log(`Created: ${new Date(mem.createdAt).toLocaleString()}`);
      console.log(`\n${mem.content}`);
    }
  }
}
