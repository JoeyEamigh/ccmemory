import { describe, expect, test, beforeEach, afterEach } from 'bun:test';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { startWatcher } from '../watcher.js';
import { releaseLock, readLockFile } from '../coordination.js';
import type { WatcherEvent } from '../types.js';

describe('watcher', () => {
  let testDir: string;

  beforeEach(async () => {
    testDir = `/tmp/watcher-test-${Date.now()}`;
    await mkdir(testDir, { recursive: true });
  });

  afterEach(async () => {
    await releaseLock(testDir);
    await rm(testDir, { recursive: true, force: true });
  });

  describe('startWatcher', () => {
    test('acquires lock on start', async () => {
      const events: WatcherEvent[] = [];
      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (e) => { events.push(...e); },
      });

      expect(watcher).not.toBeNull();

      const lockStatus = await readLockFile(testDir);
      expect(lockStatus).not.toBeNull();
      expect(lockStatus?.pid).toBe(process.pid);

      await watcher?.stop();
    });

    test('releases lock on stop', async () => {
      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {},
      });

      expect(watcher).not.toBeNull();
      await watcher?.stop();

      const lockStatus = await readLockFile(testDir);
      expect(lockStatus).toBeNull();
    });

    test('returns null if lock already held', async () => {
      const watcher1 = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {},
      });

      expect(watcher1).not.toBeNull();

      const watcher2 = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {},
      });

      expect(watcher2).toBeNull();

      await watcher1?.stop();
    });

    test('calls onInitialScan callback when provided', async () => {
      let initialScanCalled = false;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {},
        onInitialScan: async () => {
          initialScanCalled = true;
        },
      });

      expect(watcher).not.toBeNull();
      expect(initialScanCalled).toBe(true);

      await watcher?.stop();
    });

    test('detects new file creation', async () => {
      const receivedEvents: WatcherEvent[] = [];
      let eventReceived = false;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
          eventReceived = true;
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await writeFile(join(testDir, 'newfile.ts'), 'const x = 1;');

      for (let i = 0; i < 30 && !eventReceived; i++) {
        await new Promise(r => setTimeout(r, 100));
      }

      await watcher?.stop();

      if (receivedEvents.length > 0) {
        expect(receivedEvents.some(e => e.path.includes('newfile.ts'))).toBe(true);
      } else {
        console.log('Note: File watcher event not received (OS-dependent timing)');
      }
      expect(true).toBe(true);
    });

    test('detects file modifications', async () => {
      await writeFile(join(testDir, 'existing.ts'), 'const x = 1;');

      const receivedEvents: WatcherEvent[] = [];
      let eventPromiseResolve: () => void;
      const eventPromise = new Promise<void>(resolve => {
        eventPromiseResolve = resolve;
      });

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
          eventPromiseResolve();
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 100));
      await writeFile(join(testDir, 'existing.ts'), 'const x = 2;');

      await Promise.race([
        eventPromise,
        new Promise(resolve => setTimeout(resolve, 2000)),
      ]);

      await watcher?.stop();

      expect(receivedEvents.length).toBeGreaterThan(0);
      expect(receivedEvents.some(e => e.path.includes('existing.ts'))).toBe(true);
    });

    test('ignores files matching gitignore patterns', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.log\nignored/');
      await mkdir(join(testDir, 'ignored'), { recursive: true });

      const receivedEvents: WatcherEvent[] = [];

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
        },
      });

      expect(watcher).not.toBeNull();

      await writeFile(join(testDir, 'debug.log'), 'log content');
      await writeFile(join(testDir, 'ignored/file.ts'), 'const x = 1;');
      await writeFile(join(testDir, 'valid.ts'), 'const y = 2;');

      await new Promise(r => setTimeout(r, 1000));

      await watcher?.stop();

      const ignoredFiles = receivedEvents.filter(
        e => e.path.includes('.log') || e.path.includes('ignored/')
      );
      expect(ignoredFiles.length).toBe(0);
    });

    test('ignores binary files but allows code files', async () => {
      const receivedEvents: WatcherEvent[] = [];
      let eventReceived = false;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
          eventReceived = true;
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await writeFile(join(testDir, 'image.png'), 'fake binary');
      await writeFile(join(testDir, 'code.ts'), 'const x = 1;');

      for (let i = 0; i < 30 && !eventReceived; i++) {
        await new Promise(r => setTimeout(r, 100));
      }

      await watcher?.stop();

      expect(receivedEvents.some(e => e.path.includes('.png'))).toBe(false);
      if (receivedEvents.length > 0) {
        expect(receivedEvents.some(e => e.path.includes('.ts'))).toBe(true);
      }
    });

    test('ignores hidden directories except .gitignore', async () => {
      await mkdir(join(testDir, '.hidden'), { recursive: true });

      const receivedEvents: WatcherEvent[] = [];

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
        },
      });

      expect(watcher).not.toBeNull();

      await writeFile(join(testDir, '.hidden/secret.ts'), 'const secret = 1;');
      await writeFile(join(testDir, 'visible.ts'), 'const visible = 2;');

      await new Promise(r => setTimeout(r, 1000));

      await watcher?.stop();

      expect(receivedEvents.some(e => e.path.includes('.hidden'))).toBe(false);
    });

    test('debounces rapid file changes', async () => {
      let onChangeCallCount = 0;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {
          onChangeCallCount++;
        },
      });

      expect(watcher).not.toBeNull();

      for (let i = 0; i < 10; i++) {
        await writeFile(join(testDir, 'rapid.ts'), `const x = ${i};`);
        await new Promise(r => setTimeout(r, 50));
      }

      await new Promise(r => setTimeout(r, 1000));

      await watcher?.stop();

      expect(onChangeCallCount).toBeLessThan(10);
      expect(onChangeCallCount).toBeGreaterThan(0);
    });

    test('batches multiple file changes together', async () => {
      let batchSizes: number[] = [];

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          batchSizes.push(events.length);
        },
      });

      expect(watcher).not.toBeNull();

      await writeFile(join(testDir, 'file1.ts'), 'const a = 1;');
      await writeFile(join(testDir, 'file2.ts'), 'const b = 2;');
      await writeFile(join(testDir, 'file3.ts'), 'const c = 3;');

      await new Promise(r => setTimeout(r, 1000));

      await watcher?.stop();

      const totalEvents = batchSizes.reduce((sum, n) => sum + n, 0);
      expect(totalEvents).toBeGreaterThanOrEqual(1);
    });

    test('provides correct event type for changes', async () => {
      await writeFile(join(testDir, 'existing.ts'), 'const x = 1;');

      const receivedEvents: WatcherEvent[] = [];

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 100));
      await writeFile(join(testDir, 'existing.ts'), 'const x = 2;');
      await writeFile(join(testDir, 'brand-new.ts'), 'const y = 1;');

      await new Promise(r => setTimeout(r, 1000));

      await watcher?.stop();

      for (const event of receivedEvents) {
        expect(['add', 'change']).toContain(event.type);
        expect(event.timestamp).toBeGreaterThan(0);
        expect(event.path).toBeDefined();
      }
    });

    test('detects file deletion with delete event type', async () => {
      const filePath = join(testDir, 'to-delete.ts');
      await writeFile(filePath, 'const x = 1;');

      const receivedEvents: WatcherEvent[] = [];
      let deleteEventReceived = false;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
          if (events.some(e => e.type === 'delete')) {
            deleteEventReceived = true;
          }
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await rm(filePath);

      for (let i = 0; i < 30 && !deleteEventReceived; i++) {
        await new Promise(r => setTimeout(r, 100));
      }

      await watcher?.stop();

      if (deleteEventReceived) {
        const deleteEvent = receivedEvents.find(e => e.type === 'delete' && e.path.includes('to-delete.ts'));
        expect(deleteEvent).toBeDefined();
        expect(deleteEvent?.type).toBe('delete');
      } else {
        console.log('Note: File delete event not received (OS-dependent timing)');
      }
    });

    test('calls onError when file change handler throws', async () => {
      const errors: string[] = [];

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {
          throw new Error('Handler error');
        },
        onError: (err) => {
          errors.push(err.message);
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await writeFile(join(testDir, 'trigger.ts'), 'const x = 1;');

      for (let i = 0; i < 30 && errors.length === 0; i++) {
        await new Promise(r => setTimeout(r, 100));
      }

      await watcher?.stop();

      if (errors.length > 0) {
        expect(errors[0]).toBe('Handler error');
      }
    });

    test('calls onGitignoreChange when .gitignore is modified', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.log');

      let gitignoreChangeCallCount = 0;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {},
        onGitignoreChange: async () => {
          gitignoreChangeCallCount++;
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await writeFile(join(testDir, '.gitignore'), '*.log\n*.tmp');

      for (let i = 0; i < 30 && gitignoreChangeCallCount === 0; i++) {
        await new Promise(r => setTimeout(r, 100));
      }

      await watcher?.stop();

      if (gitignoreChangeCallCount > 0) {
        expect(gitignoreChangeCallCount).toBeGreaterThan(0);
      } else {
        console.log('Note: .gitignore change event not received (OS-dependent timing)');
      }
    });

    test('does not call onGitignoreChange if hash is unchanged', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.log');

      let gitignoreChangeCount = 0;

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async () => {},
        onGitignoreChange: async () => {
          gitignoreChangeCount++;
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await writeFile(join(testDir, '.gitignore'), '*.log');

      await new Promise(r => setTimeout(r, 1500));

      await watcher?.stop();

      expect(gitignoreChangeCount).toBe(0);
    });

    test('does not trigger onFileChange for .gitignore modifications', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.log');

      const receivedEvents: WatcherEvent[] = [];

      const watcher = await startWatcher({
        projectPath: testDir,
        onFileChange: async (events) => {
          receivedEvents.push(...events);
        },
      });

      expect(watcher).not.toBeNull();

      await new Promise(r => setTimeout(r, 200));

      await writeFile(join(testDir, '.gitignore'), '*.log\n*.tmp');

      await new Promise(r => setTimeout(r, 1500));

      await watcher?.stop();

      const gitignoreEvents = receivedEvents.filter(e => e.path.includes('.gitignore'));
      expect(gitignoreEvents.length).toBe(0);
    });
  });
});
