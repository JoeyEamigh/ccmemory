import { existsSync, watch, type FSWatcher } from 'node:fs';
import { basename, extname, join, relative } from 'node:path';
import { log } from '../../utils/log.js';
import { acquireLock, releaseLock, updateLockActivity } from './coordination.js';
import { loadGitignorePatterns, shouldIgnoreFile, type GitignoreFilter } from './gitignore.js';
import { LANGUAGE_EXTENSIONS, type WatcherEvent } from './types.js';

const DEBOUNCE_MS = 500;
const GITIGNORE_DEBOUNCE_MS = 1000;

type WatcherOptions = {
  projectPath: string;
  onFileChange: (events: WatcherEvent[]) => Promise<void>;
  onInitialScan?: () => Promise<void>;
  onGitignoreChange?: () => Promise<void>;
  onError?: (error: Error) => void;
};

type WatcherInstance = {
  stop: () => Promise<void>;
  projectPath: string;
};

export async function startWatcher(options: WatcherOptions): Promise<WatcherInstance | null> {
  const { projectPath, onFileChange, onInitialScan, onGitignoreChange, onError } = options;

  const lockAcquired = await acquireLock(projectPath);
  if (!lockAcquired) {
    log.warn('watcher', 'Failed to acquire lock, watcher may already be running', { projectPath });
    return null;
  }

  log.info('watcher', 'Starting file watcher', { projectPath });

  let gitignore: GitignoreFilter;
  try {
    gitignore = await loadGitignorePatterns(projectPath);
  } catch (err) {
    log.error('watcher', 'Failed to load gitignore patterns', { error: (err as Error).message });
    await releaseLock(projectPath);
    return null;
  }

  const pendingEvents = new Map<string, WatcherEvent>();
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let gitignoreDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let isProcessing = false;
  let isStopping = false;
  let currentGitignoreHash = gitignore.hash;

  const watchers: FSWatcher[] = [];

  async function handleGitignoreChange(): Promise<void> {
    if (isStopping) return;

    try {
      const newGitignore = await loadGitignorePatterns(projectPath);
      if (newGitignore.hash !== currentGitignoreHash) {
        log.info('watcher', '.gitignore changed, reloading patterns', {
          oldHash: currentGitignoreHash,
          newHash: newGitignore.hash,
        });
        currentGitignoreHash = newGitignore.hash;
        gitignore = newGitignore;

        if (onGitignoreChange) {
          await onGitignoreChange();
        }
      }
    } catch (err) {
      log.error('watcher', 'Failed to reload gitignore patterns', {
        error: (err as Error).message,
      });
    }
  }

  function scheduleGitignoreProcessing(): void {
    if (gitignoreDebounceTimer) {
      clearTimeout(gitignoreDebounceTimer);
    }
    gitignoreDebounceTimer = setTimeout(() => {
      gitignoreDebounceTimer = null;
      handleGitignoreChange();
    }, GITIGNORE_DEBOUNCE_MS);
  }

  async function processPendingEvents(): Promise<void> {
    if (isProcessing || pendingEvents.size === 0 || isStopping) return;

    isProcessing = true;
    const events = Array.from(pendingEvents.values());
    pendingEvents.clear();

    try {
      await onFileChange(events);
      await updateLockActivity(projectPath);
    } catch (err) {
      log.error('watcher', 'Error processing file changes', { error: (err as Error).message });
      onError?.(err as Error);
    } finally {
      isProcessing = false;

      if (pendingEvents.size > 0) {
        scheduleProcessing();
      }
    }
  }

  function scheduleProcessing(): void {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      processPendingEvents();
    }, DEBOUNCE_MS);
  }

  function handleFileEvent(eventType: string, filename: string | null, basePath: string): void {
    if (isStopping || !filename) return;

    const fullPath = join(basePath, filename);
    const relativePath = relative(projectPath, fullPath);

    const fileBasename = basename(filename);
    if (fileBasename === '.gitignore') {
      log.debug('watcher', 'Detected .gitignore change, scheduling reload');
      scheduleGitignoreProcessing();
      return;
    }

    const segments = relativePath.split('/');
    for (const segment of segments) {
      if (segment.startsWith('.') && segment !== '.gitignore') {
        return;
      }
    }

    if (gitignore.isIgnored(fullPath, false)) {
      return;
    }

    if (shouldIgnoreFile(filename)) {
      return;
    }

    const ext = extname(filename).toLowerCase();
    if (!LANGUAGE_EXTENSIONS[ext]) {
      return;
    }

    let eventKind: 'add' | 'change' | 'delete';
    if (eventType === 'rename') {
      eventKind = existsSync(fullPath) ? 'add' : 'delete';
    } else {
      eventKind = 'change';
    }

    const event: WatcherEvent = {
      type: eventKind,
      path: fullPath,
      timestamp: Date.now(),
    };

    pendingEvents.set(fullPath, event);
    scheduleProcessing();

    log.debug('watcher', 'File event', {
      type: event.type,
      path: relativePath,
    });
  }

  try {
    const watcher = watch(
      projectPath,
      { recursive: true },
      (eventType, filename) => {
        handleFileEvent(eventType, filename, projectPath);
      },
    );

    watcher.on('error', err => {
      log.error('watcher', 'Watcher error', { error: err.message });
      onError?.(err);
    });

    watchers.push(watcher);

    if (onInitialScan) {
      await onInitialScan();
    }

    log.info('watcher', 'File watcher started', { projectPath });
  } catch (err) {
    log.error('watcher', 'Failed to start watcher', { error: (err as Error).message });
    await releaseLock(projectPath);
    return null;
  }

  const instance: WatcherInstance = {
    projectPath,
    stop: async () => {
      isStopping = true;

      if (debounceTimer) {
        clearTimeout(debounceTimer);
        debounceTimer = null;
      }

      if (gitignoreDebounceTimer) {
        clearTimeout(gitignoreDebounceTimer);
        gitignoreDebounceTimer = null;
      }

      for (const watcher of watchers) {
        try {
          watcher.close();
        } catch (err) {
          log.warn('watcher', 'Error closing watcher', { error: (err as Error).message });
        }
      }

      await releaseLock(projectPath);
      log.info('watcher', 'File watcher stopped', { projectPath });
    },
  };

  process.on('SIGINT', async () => {
    log.info('watcher', 'Received SIGINT, stopping watcher');
    await instance.stop();
    process.exit(0);
  });

  process.on('SIGTERM', async () => {
    log.info('watcher', 'Received SIGTERM, stopping watcher');
    await instance.stop();
    process.exit(0);
  });

  return instance;
}
