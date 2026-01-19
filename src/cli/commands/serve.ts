import { parseArgs } from "util";
import { log } from "../../utils/log.js";

export async function serveCommand(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: {
      port: { type: "string", short: "p", default: "37778" },
      open: { type: "boolean" },
    },
  });

  const port = parseInt(values.port as string, 10);

  log.info("cli", "Starting WebUI server", { port });

  console.log(`\nCCMemory WebUI`);
  console.log(`\nStarting server on port ${port}...`);
  console.log(`\n⚠️  WebUI not yet implemented (Phase 8)`);
  console.log(`\nServer would be available at: http://localhost:${port}`);

  if (values.open) {
    console.log(`Would open browser automatically.`);
  }
}
