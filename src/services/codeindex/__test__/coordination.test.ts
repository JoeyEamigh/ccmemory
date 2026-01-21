import { describe, expect, test, beforeEach, afterEach } from 'bun:test';
import { mkdir, rm, writeFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import {
  acquireLock,
  releaseLock,
  readLockFile,
  isProcessRunning,
  stopWatcher,
  listActiveWatchers,
  getProjectHash,
  ensureWatchersDir,
  updateLockActivity,
} from '../coordination.js';
import { getPaths } from '../../../utils/paths.js';

describe('coordination', () => {
  let testProjectPath: string;
  let watchersDir: string;

  beforeEach(async () => {
    testProjectPath = `/tmp/coord-test-${Date.now()}`;
    await mkdir(testProjectPath, { recursive: true });

    const { data } = getPaths();
    watchersDir = join(data, 'watchers');
    await mkdir(watchersDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(testProjectPath, { recursive: true, force: true });
    await releaseLock(testProjectPath);
  });

  describe('getProjectHash', () => {
    test('generates consistent hash for same path', () => {
      const hash1 = getProjectHash('/some/project/path');
      const hash2 = getProjectHash('/some/project/path');
      expect(hash1).toBe(hash2);
    });

    test('generates different hash for different paths', () => {
      const hash1 = getProjectHash('/project/one');
      const hash2 = getProjectHash('/project/two');
      expect(hash1).not.toBe(hash2);
    });

    test('returns 16-character hash', () => {
      const hash = getProjectHash('/any/path');
      expect(hash.length).toBe(16);
    });
  });

  describe('acquireLock', () => {
    test('creates lock file with PID', async () => {
      const acquired = await acquireLock(testProjectPath);
      expect(acquired).toBe(true);

      const status = await readLockFile(testProjectPath);
      expect(status?.pid).toBe(process.pid);
      expect(status?.projectPath).toBe(testProjectPath);
    });

    test('records start time', async () => {
      const before = Date.now();
      await acquireLock(testProjectPath);
      const after = Date.now();

      const status = await readLockFile(testProjectPath);
      expect(status?.startedAt).toBeGreaterThanOrEqual(before);
      expect(status?.startedAt).toBeLessThanOrEqual(after);
    });

    test('initializes indexed files to 0', async () => {
      await acquireLock(testProjectPath);
      const status = await readLockFile(testProjectPath);
      expect(status?.indexedFiles).toBe(0);
    });

    test('fails if lock already held by running process', async () => {
      const first = await acquireLock(testProjectPath);
      expect(first).toBe(true);

      const second = await acquireLock(testProjectPath);
      expect(second).toBe(false);
    });

    test('cleans up stale locks from dead processes', async () => {
      const hash = getProjectHash(testProjectPath);
      const lockPath = join(watchersDir, `${hash}.lock`);

      const staleStatus = {
        projectId: hash,
        projectPath: testProjectPath,
        pid: 99999999,
        startedAt: Date.now() - 3600000,
        lastActivity: Date.now() - 3600000,
        indexedFiles: 100,
      };
      await writeFile(lockPath, JSON.stringify(staleStatus));

      const acquired = await acquireLock(testProjectPath);
      expect(acquired).toBe(true);

      const status = await readLockFile(testProjectPath);
      expect(status?.pid).toBe(process.pid);
    });
  });

  describe('releaseLock', () => {
    test('removes lock file', async () => {
      await acquireLock(testProjectPath);
      const beforeRelease = await readLockFile(testProjectPath);
      expect(beforeRelease).not.toBeNull();

      await releaseLock(testProjectPath);
      const afterRelease = await readLockFile(testProjectPath);
      expect(afterRelease).toBeNull();
    });

    test('handles non-existent lock file gracefully', async () => {
      await releaseLock('/nonexistent/project/path');
    });
  });

  describe('readLockFile', () => {
    test('returns status for existing lock', async () => {
      await acquireLock(testProjectPath);
      const status = await readLockFile(testProjectPath);

      expect(status).not.toBeNull();
      expect(status?.projectPath).toBe(testProjectPath);
      expect(status?.pid).toBe(process.pid);
    });

    test('returns null for non-existent lock', async () => {
      const status = await readLockFile('/nonexistent/project');
      expect(status).toBeNull();
    });
  });

  describe('updateLockActivity', () => {
    test('updates last activity time', async () => {
      await acquireLock(testProjectPath);
      const initial = await readLockFile(testProjectPath);

      await new Promise(r => setTimeout(r, 10));
      await updateLockActivity(testProjectPath);

      const updated = await readLockFile(testProjectPath);
      expect(updated?.lastActivity).toBeGreaterThan(initial?.lastActivity ?? 0);
    });

    test('updates indexed files count', async () => {
      await acquireLock(testProjectPath);
      await updateLockActivity(testProjectPath, 50);

      const status = await readLockFile(testProjectPath);
      expect(status?.indexedFiles).toBe(50);
    });

    test('does nothing for non-existent lock', async () => {
      await updateLockActivity('/nonexistent/project', 100);
    });
  });

  describe('isProcessRunning', () => {
    test('returns true for current process', async () => {
      const running = await isProcessRunning(process.pid);
      expect(running).toBe(true);
    });

    test('returns false for non-existent PID', async () => {
      const running = await isProcessRunning(99999999);
      expect(running).toBe(false);
    });
  });

  describe('stopWatcher', () => {
    test('returns false if no watcher running', async () => {
      const result = await stopWatcher('/tmp/nonexistent-project');
      expect(result).toBe(false);
    });

    test('cleans up lock if process not running', async () => {
      const hash = getProjectHash(testProjectPath);
      const lockPath = join(watchersDir, `${hash}.lock`);

      const staleStatus = {
        projectId: hash,
        projectPath: testProjectPath,
        pid: 99999999,
        startedAt: Date.now(),
        lastActivity: Date.now(),
        indexedFiles: 0,
      };
      await writeFile(lockPath, JSON.stringify(staleStatus));

      const result = await stopWatcher(testProjectPath);
      expect(result).toBe(false);

      const status = await readLockFile(testProjectPath);
      expect(status).toBeNull();
    });
  });

  describe('listActiveWatchers', () => {
    test('returns empty array when no watchers', async () => {
      const existingLocks = await listActiveWatchers();
      for (const lock of existingLocks) {
        await releaseLock(lock.projectPath);
      }

      const watchers = await listActiveWatchers();
      expect(watchers).toEqual([]);
    });

    test('includes current process lock in active watchers', async () => {
      const acquired = await acquireLock(testProjectPath);
      expect(acquired).toBe(true);

      const watchers = await listActiveWatchers();
      const found = watchers.find(w => w.projectPath === testProjectPath);
      expect(found).toBeDefined();
      expect(found?.pid).toBe(process.pid);
    });

    test('filters out stale lock files', async () => {
      const hash = getProjectHash('/tmp/stale-project');
      const lockPath = join(watchersDir, `${hash}.lock`);

      const staleStatus = {
        projectId: hash,
        projectPath: '/tmp/stale-project',
        pid: 99999999,
        startedAt: Date.now(),
        lastActivity: Date.now(),
        indexedFiles: 0,
      };
      await writeFile(lockPath, JSON.stringify(staleStatus));

      const watchers = await listActiveWatchers();
      expect(watchers.some(w => w.projectPath === '/tmp/stale-project')).toBe(false);
    });
  });

  describe('ensureWatchersDir', () => {
    test('creates watchers directory if not exists', async () => {
      await ensureWatchersDir();
      const { data } = getPaths();
      const dirPath = join(data, 'watchers');

      try {
        const dirStat = await stat(dirPath);
        expect(dirStat.isDirectory()).toBe(true);
      } catch {
        throw new Error('Watchers directory should exist');
      }
    });
  });
});
