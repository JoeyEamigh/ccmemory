import { parseArgs } from 'util';
import { resolve } from 'node:path';
import { log } from '../../utils/log.js';
import { createEmbeddingService } from '../../services/embedding/index.js';
import { getOrCreateProject } from '../../services/project.js';
import { createCodeIndexService } from '../../services/codeindex/index.js';
import { listActiveWatchers, readLockFile, stopWatcher } from '../../services/codeindex/coordination.js';
import { startWatcher } from '../../services/codeindex/watcher.js';

export async function watchCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      stop: { type: 'boolean' },
      status: { type: 'boolean' },
    },
    allowPositionals: true,
  });

  if (values.status) {
    await showWatcherStatus();
    return;
  }

  const projectPath = resolve(positionals[0] ?? process.cwd());

  if (values.stop) {
    await stopWatcherCommand(projectPath);
    return;
  }

  await startWatcherCommand(projectPath);
}

async function showWatcherStatus(): Promise<void> {
  const watchers = await listActiveWatchers();

  if (watchers.length === 0) {
    console.log('No active watchers.');
    return;
  }

  console.log('Active watchers:\n');
  for (const watcher of watchers) {
    const startedAt = new Date(watcher.startedAt).toISOString();
    const lastActivity = new Date(watcher.lastActivity).toISOString();
    console.log(`  Project: ${watcher.projectPath}`);
    console.log(`  PID: ${watcher.pid}`);
    console.log(`  Started: ${startedAt}`);
    console.log(`  Last activity: ${lastActivity}`);
    console.log(`  Indexed files: ${watcher.indexedFiles}`);
    console.log('');
  }
}

async function stopWatcherCommand(projectPath: string): Promise<void> {
  console.log(`Stopping watcher for: ${projectPath}`);

  const stopped = await stopWatcher(projectPath);
  if (stopped) {
    console.log('Watcher stopped successfully.');
  } else {
    const status = await readLockFile(projectPath);
    if (status) {
      console.log('Watcher process not running. Cleaned up stale lock file.');
    } else {
      console.log('No watcher running for this project.');
    }
  }
}

async function startWatcherCommand(projectPath: string): Promise<void> {
  console.log(`Starting watcher for: ${projectPath}`);
  log.info('cli', 'Starting watcher', { projectPath });

  const project = await getOrCreateProject(projectPath);
  const embeddingService = await createEmbeddingService();
  const codeIndex = createCodeIndexService(embeddingService);

  let indexedCount = 0;

  const watcher = await startWatcher({
    projectPath,
    onFileChange: async events => {
      await codeIndex.processFileChanges(projectPath, project.id, events);
      indexedCount += events.length;
      console.log(`Processed ${events.length} file change(s). Total indexed: ${indexedCount}`);
    },
    onInitialScan: async () => {
      console.log('Performing initial scan...');
      const progress = await codeIndex.index(projectPath, project.id, {
        onProgress: p => {
          if (p.phase === 'scanning') {
            process.stdout.write(`\rScanning: ${p.scannedFiles} files found`);
          } else if (p.phase === 'indexing' && p.currentFile) {
            process.stdout.write(`\rIndexing: ${p.indexedFiles}/${p.totalFiles} - ${p.currentFile.slice(0, 50)}`);
          }
        },
      });

      console.log('\n');
      console.log(`Initial scan complete:`);
      console.log(`  Scanned: ${progress.scannedFiles} files`);
      console.log(`  Indexed: ${progress.indexedFiles} files`);

      if (progress.errors.length > 0) {
        console.log(`  Errors: ${progress.errors.length}`);
        for (const error of progress.errors.slice(0, 5)) {
          console.log(`    - ${error}`);
        }
        if (progress.errors.length > 5) {
          console.log(`    ... and ${progress.errors.length - 5} more errors`);
        }
      }

      indexedCount = progress.indexedFiles;
      console.log('\nWatching for file changes. Press Ctrl+C to stop.\n');
    },
    onGitignoreChange: async () => {
      console.log('\n.gitignore changed, re-scanning with updated patterns...');
      const progress = await codeIndex.index(projectPath, project.id, {
        force: true,
        onProgress: p => {
          if (p.phase === 'scanning') {
            process.stdout.write(`\rScanning: ${p.scannedFiles} files found`);
          } else if (p.phase === 'indexing' && p.currentFile) {
            process.stdout.write(`\rIndexing: ${p.indexedFiles}/${p.totalFiles} - ${p.currentFile.slice(0, 50)}`);
          }
        },
      });

      console.log('\n');
      console.log(`Re-scan complete:`);
      console.log(`  Scanned: ${progress.scannedFiles} files`);
      console.log(`  Indexed: ${progress.indexedFiles} files`);
      indexedCount = progress.indexedFiles;
      console.log('\nWatching for file changes. Press Ctrl+C to stop.\n');
    },
    onError: error => {
      console.error(`Watcher error: ${error.message}`);
    },
  });

  if (!watcher) {
    console.error('Failed to start watcher. It may already be running for this project.');
    console.log('Use `ccmemory watch --status` to see active watchers.');
    console.log('Use `ccmemory watch --stop <path>` to stop an existing watcher.');
    process.exit(1);
  }

  await new Promise(() => {});
}
