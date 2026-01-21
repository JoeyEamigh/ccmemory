import { createHash } from 'crypto';
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { getPaths } from '../../utils/paths.js';
import { log } from '../../utils/log.js';
import type { WatcherStatus } from './types.js';

function getProjectHash(projectPath: string): string {
  return createHash('sha256').update(projectPath).digest('hex').slice(0, 16);
}

function getWatchersDir(): string {
  const { data } = getPaths();
  return join(data, 'watchers');
}

function getLockFilePath(projectPath: string): string {
  const hash = getProjectHash(projectPath);
  return join(getWatchersDir(), `${hash}.lock`);
}

export async function ensureWatchersDir(): Promise<void> {
  const dir = getWatchersDir();
  await mkdir(dir, { recursive: true });
}

export async function acquireLock(projectPath: string): Promise<boolean> {
  await ensureWatchersDir();

  const lockPath = getLockFilePath(projectPath);

  const existingStatus = await readLockFile(projectPath);
  if (existingStatus) {
    const isRunning = await isProcessRunning(existingStatus.pid);
    if (isRunning) {
      log.warn('coordination', 'Watcher already running', {
        projectPath,
        pid: existingStatus.pid,
      });
      return false;
    }

    log.info('coordination', 'Stale lock file found, removing', { projectPath });
    await releaseLock(projectPath);
  }

  const status: WatcherStatus = {
    projectId: getProjectHash(projectPath),
    projectPath,
    pid: process.pid,
    startedAt: Date.now(),
    lastActivity: Date.now(),
    indexedFiles: 0,
  };

  await writeFile(lockPath, JSON.stringify(status, null, 2));
  log.info('coordination', 'Lock acquired', { projectPath, pid: process.pid });
  return true;
}

export async function releaseLock(projectPath: string): Promise<void> {
  const lockPath = getLockFilePath(projectPath);

  try {
    await unlink(lockPath);
    log.info('coordination', 'Lock released', { projectPath });
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code !== 'ENOENT') {
      log.warn('coordination', 'Failed to release lock', {
        projectPath,
        error: (err as Error).message,
      });
    }
  }
}

export async function readLockFile(projectPath: string): Promise<WatcherStatus | null> {
  const lockPath = getLockFilePath(projectPath);

  try {
    const content = await readFile(lockPath, 'utf-8');
    return JSON.parse(content) as WatcherStatus;
  } catch {
    return null;
  }
}

export async function updateLockActivity(projectPath: string, indexedFiles?: number): Promise<void> {
  const status = await readLockFile(projectPath);
  if (!status) return;

  status.lastActivity = Date.now();
  if (indexedFiles !== undefined) {
    status.indexedFiles = indexedFiles;
  }

  const lockPath = getLockFilePath(projectPath);
  await writeFile(lockPath, JSON.stringify(status, null, 2));
}

export async function isProcessRunning(pid: number): Promise<boolean> {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export async function stopWatcher(projectPath: string): Promise<boolean> {
  const status = await readLockFile(projectPath);
  if (!status) {
    log.info('coordination', 'No watcher running', { projectPath });
    return false;
  }

  const isRunning = await isProcessRunning(status.pid);
  if (!isRunning) {
    log.info('coordination', 'Watcher process not running, cleaning up lock', { projectPath });
    await releaseLock(projectPath);
    return false;
  }

  try {
    process.kill(status.pid, 'SIGTERM');
    log.info('coordination', 'Sent SIGTERM to watcher', { projectPath, pid: status.pid });

    let attempts = 0;
    while (attempts < 10) {
      await new Promise(r => setTimeout(r, 500));
      if (!(await isProcessRunning(status.pid))) {
        break;
      }
      attempts++;
    }

    if (await isProcessRunning(status.pid)) {
      process.kill(status.pid, 'SIGKILL');
      log.warn('coordination', 'Sent SIGKILL to watcher', { projectPath, pid: status.pid });
    }

    await releaseLock(projectPath);
    return true;
  } catch (err) {
    log.error('coordination', 'Failed to stop watcher', {
      projectPath,
      error: (err as Error).message,
    });
    return false;
  }
}

export async function listActiveWatchers(): Promise<WatcherStatus[]> {
  await ensureWatchersDir();
  const watchersDir = getWatchersDir();

  let entries: string[];
  try {
    const fs = await import('node:fs/promises');
    entries = await fs.readdir(watchersDir);
  } catch {
    entries = [];
  }

  const statuses: WatcherStatus[] = [];

  for (const entry of entries) {
    if (!entry.endsWith('.lock')) continue;

    const lockPath = join(watchersDir, entry);
    try {
      const content = await readFile(lockPath, 'utf-8');
      const status = JSON.parse(content) as WatcherStatus;

      const isRunning = await isProcessRunning(status.pid);
      if (isRunning) {
        statuses.push(status);
      } else {
        await unlink(lockPath);
      }
    } catch {
      continue;
    }
  }

  return statuses;
}

export { getProjectHash };
