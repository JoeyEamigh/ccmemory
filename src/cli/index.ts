#!/usr/bin/env bun

import { searchCommand } from "./commands/search.js";
import { showCommand } from "./commands/show.js";
import { deleteCommand, archiveCommand } from "./commands/delete.js";
import { importCommand, exportCommand } from "./commands/import.js";
import { configCommand } from "./commands/config.js";
import { healthCommand } from "./commands/health.js";
import { statsCommand } from "./commands/stats.js";
import { serveCommand } from "./commands/serve.js";
import { log } from "../utils/log.js";

const commands: Record<string, (args: string[]) => Promise<void>> = {
  search: searchCommand,
  show: showCommand,
  delete: deleteCommand,
  archive: archiveCommand,
  import: importCommand,
  export: exportCommand,
  config: configCommand,
  health: healthCommand,
  stats: statsCommand,
  serve: serveCommand,
};

async function main(): Promise<void> {
  const [command, ...args] = process.argv.slice(2);

  if (!command || command === "help" || command === "--help") {
    printHelp();
    return;
  }

  if (command === "--version" || command === "-v") {
    console.log("ccmemory 0.1.0");
    return;
  }

  const handler = commands[command];
  if (!handler) {
    log.warn("cli", "Unknown command", { command });
    console.error(`Unknown command: ${command}`);
    console.error(`Run 'ccmemory help' for usage.`);
    process.exit(1);
  }

  log.debug("cli", "Executing command", { command, args: args.length });
  await handler(args);
}

function printHelp(): void {
  console.log(`
CCMemory - Claude Code Memory System

Usage: ccmemory <command> [options]

Commands:
  search <query>           Search memories
    -p, --project <path>   Filter by project
    -s, --sector <sector>  Filter by sector
    -l, --limit <n>        Max results (default: 10)
    --semantic             Semantic search only
    --keywords             Keyword search only
    --json                 JSON output

  show <id>               Show memory details
    -r, --related          Show related memories

  delete <id>             Delete a memory
    --force                Skip confirmation

  archive                 Archive old low-salience memories
    --before <date>        Archive before date
    --dry-run              Preview without changes

  import <file>           Import document
    -p, --project <path>   Associate with project
    -t, --title <title>    Document title

  export                  Export memories
    -p, --project <path>   Filter by project
    -f, --format <fmt>     Format: json, csv (default: json)
    -o, --output <file>    Output file

  config [key] [value]    View/set configuration
    Examples:
      ccmemory config
      ccmemory config embedding.provider
      ccmemory config embedding.provider ollama

  health                  Check system health
    -v, --verbose          Detailed output

  stats                   Show statistics
    -p, --project <path>   Filter by project

  serve                   Start WebUI server
    -p, --port <port>      Port (default: 37778)
    --open                 Open in browser
`);
}

main().catch((err: Error) => {
  log.error("cli", "Command failed", { error: err.message });
  console.error(err);
  process.exit(1);
});
