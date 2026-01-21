import { parseArgs } from 'util';
import { resolve } from 'node:path';
import { log } from '../../utils/log.js';
import { createEmbeddingService } from '../../services/embedding/index.js';
import { getOrCreateProject } from '../../services/project.js';
import { createCodeIndexService } from '../../services/codeindex/index.js';
import type { CodeIndexExport } from '../../services/codeindex/types.js';

export async function codeIndexCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      force: { type: 'boolean', short: 'f' },
      'dry-run': { type: 'boolean' },
    },
    allowPositionals: true,
  });

  const projectPath = resolve(positionals[0] ?? process.cwd());
  const force = values.force ?? false;
  const dryRun = values['dry-run'] ?? false;

  log.info('cli', 'Starting code indexing', { projectPath, force, dryRun });

  console.log(`Indexing code in: ${projectPath}`);
  if (force) console.log('  (force re-index all files)');
  if (dryRun) console.log('  (dry run - no changes will be made)');
  console.log('');

  const project = await getOrCreateProject(projectPath);
  const embeddingService = await createEmbeddingService();
  const codeIndex = createCodeIndexService(embeddingService);

  const progress = await codeIndex.index(projectPath, project.id, {
    force,
    dryRun,
    onProgress: p => {
      if (p.phase === 'scanning') {
        process.stdout.write(`\rScanning: ${p.scannedFiles} files found`);
      } else if (p.phase === 'indexing') {
        if (p.currentFile) {
          const truncatedFile = p.currentFile.length > 50 ? '...' + p.currentFile.slice(-47) : p.currentFile;
          process.stdout.write(`\rIndexing: ${p.indexedFiles}/${p.totalFiles} - ${truncatedFile}`);
        } else {
          process.stdout.write(`\rIndexing: ${p.indexedFiles}/${p.totalFiles}`);
        }
      }
    },
  });

  console.log('\n');

  if (dryRun) {
    console.log('Dry run complete:');
    console.log(`  Files found: ${progress.totalFiles}`);
    console.log('\nRun without --dry-run to index these files.');
    return;
  }

  console.log('Indexing complete:');
  console.log(`  Files scanned: ${progress.scannedFiles}`);
  console.log(`  Files indexed: ${progress.indexedFiles}`);

  if (progress.errors.length > 0) {
    console.log(`  Errors: ${progress.errors.length}`);
    console.log('');
    console.log('Errors:');
    for (const error of progress.errors.slice(0, 10)) {
      console.log(`  - ${error}`);
    }
    if (progress.errors.length > 10) {
      console.log(`  ... and ${progress.errors.length - 10} more errors`);
    }
  }

  const state = await codeIndex.getState(project.id);
  if (state) {
    const lastIndexed = new Date(state.lastIndexedAt).toISOString();
    console.log('');
    console.log(`Index state:`);
    console.log(`  Last indexed: ${lastIndexed}`);
    console.log(`  Total indexed files: ${state.indexedFiles}`);
  }
}

export async function codeIndexExportCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      output: { type: 'string', short: 'o' },
    },
    allowPositionals: true,
  });

  const projectPath = resolve(positionals[0] ?? process.cwd());
  const outputPath = values.output ?? `code-index-${Date.now()}.json`;

  log.info('cli', 'Exporting code index', { projectPath, outputPath });

  const project = await getOrCreateProject(projectPath);
  const embeddingService = await createEmbeddingService();
  const codeIndex = createCodeIndexService(embeddingService);

  const data = await codeIndex.exportIndex(projectPath, project.id);

  if (!data) {
    console.error('No index found for this project. Run `ccmemory code-index` first.');
    process.exit(1);
  }

  await Bun.write(outputPath, JSON.stringify(data, null, 2));

  console.log(`Exported code index:`);
  console.log(`  Files: ${data.files.length}`);
  console.log(`  Output: ${outputPath}`);
}

export async function codeIndexImportCommand(args: string[]): Promise<void> {
  const { positionals } = parseArgs({
    args,
    options: {
      project: { type: 'string', short: 'p' },
    },
    allowPositionals: true,
  });

  const inputPath = positionals[0];
  if (!inputPath) {
    console.error('Usage: ccmemory code-index-import <file> [-p project-path]');
    process.exit(1);
  }

  const projectPath = resolve(positionals[1] ?? process.cwd());

  log.info('cli', 'Importing code index', { inputPath, projectPath });

  let data: CodeIndexExport;
  try {
    const content = await Bun.file(inputPath).text();
    data = JSON.parse(content) as CodeIndexExport;
  } catch (err) {
    console.error(`Failed to read import file: ${(err as Error).message}`);
    process.exit(1);
  }

  if (!data.version || !data.files) {
    console.error('Invalid code index export format.');
    process.exit(1);
  }

  const project = await getOrCreateProject(projectPath);
  const embeddingService = await createEmbeddingService();
  const codeIndex = createCodeIndexService(embeddingService);

  console.log(`Importing code index from: ${inputPath}`);
  console.log(`  Original project: ${data.projectPath}`);
  console.log(`  Target project: ${projectPath}`);
  console.log(`  Files in export: ${data.files.length}`);
  console.log('');

  const result = await codeIndex.importIndex(projectPath, project.id, data);

  console.log('Import complete:');
  console.log(`  Imported: ${result.imported} files`);
  console.log(`  Skipped (already indexed): ${result.skipped} files`);
}
