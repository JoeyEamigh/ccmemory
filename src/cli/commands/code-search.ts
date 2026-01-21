import { parseArgs } from 'util';
import { resolve } from 'node:path';
import { log } from '../../utils/log.js';
import { createEmbeddingService } from '../../services/embedding/index.js';
import { getOrCreateProject } from '../../services/project.js';
import { createCodeIndexService } from '../../services/codeindex/index.js';
import type { CodeLanguage } from '../../services/codeindex/types.js';

export async function codeSearchCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      language: { type: 'string', short: 'l' },
      limit: { type: 'string', short: 'n' },
      project: { type: 'string', short: 'p' },
      json: { type: 'boolean' },
    },
    allowPositionals: true,
  });

  const query = positionals.join(' ');
  if (!query) {
    console.error('Usage: ccmemory code-search <query> [options]');
    console.error('');
    console.error('Options:');
    console.error('  -l, --language <lang>   Filter by language (ts, js, py, etc.)');
    console.error('  -n, --limit <n>         Max results (default: 10)');
    console.error('  -p, --project <path>    Project path (default: current directory)');
    console.error('  --json                  Output as JSON');
    process.exit(1);
  }

  const projectPath = resolve(values.project ?? process.cwd());
  const language = values.language as CodeLanguage | undefined;
  const limit = values.limit ? parseInt(values.limit, 10) : 10;
  const jsonOutput = values.json ?? false;

  log.info('cli', 'Code search', { query: query.slice(0, 50), projectPath, language, limit });

  const project = await getOrCreateProject(projectPath);
  const embeddingService = await createEmbeddingService();
  const codeIndex = createCodeIndexService(embeddingService);

  const state = await codeIndex.getState(project.id);
  if (!state) {
    console.error('Error: Project code has not been indexed yet.');
    console.error('');
    console.error('Run one of the following to index your code:');
    console.error('  ccmemory code-index      # One-time indexing');
    console.error('  ccmemory watch .         # Start watcher for continuous indexing');
    process.exit(1);
  }

  const results = await codeIndex.search({
    query,
    projectId: project.id,
    language,
    limit,
  });

  if (results.length === 0) {
    if (jsonOutput) {
      console.log(JSON.stringify({ results: [], query, projectPath }, null, 2));
    } else {
      console.log('No results found.');
    }
    return;
  }

  if (jsonOutput) {
    console.log(JSON.stringify({ results, query, projectPath }, null, 2));
    return;
  }

  console.log(`Found ${results.length} result(s) for "${query}":\n`);

  for (let i = 0; i < results.length; i++) {
    const result = results[i];
    if (!result) continue;

    console.log(`[${i + 1}] ${result.path}:${result.startLine}-${result.endLine}`);
    console.log(`    Language: ${result.language} | Type: ${result.chunkType} | Score: ${result.score.toFixed(3)}`);

    if (result.symbols.length > 0) {
      console.log(`    Symbols: ${result.symbols.join(', ')}`);
    }

    const preview = result.content.split('\n').slice(0, 5).join('\n');
    const indentedPreview = preview
      .split('\n')
      .map(line => '    │ ' + line)
      .join('\n');

    console.log('');
    console.log(indentedPreview);

    if (result.content.split('\n').length > 5) {
      console.log('    │ ...');
    }

    console.log('');
  }

  const timeSinceIndex = Date.now() - state.lastIndexedAt;
  const hoursAgo = Math.floor(timeSinceIndex / (1000 * 60 * 60));
  if (hoursAgo > 24) {
    console.log(`Note: Index was last updated ${hoursAgo} hours ago. Run 'ccmemory code-index' to refresh.`);
  }
}
