import { readdir, stat } from 'node:fs/promises';
import { extname, join, relative } from 'node:path';
import { log } from '../../utils/log.js';
import {
  type GitignoreFilter,
  type NestedGitignoreFilter,
  loadGitignorePatterns,
  loadGitignorePatternsWithNesting,
  shouldIgnoreFile,
} from './gitignore.js';
import { LANGUAGE_EXTENSIONS, type CodeLanguage, type ScanResult, type ScannedFile } from './types.js';

function getLanguageFromPath(filePath: string): CodeLanguage {
  const ext = extname(filePath).toLowerCase();
  return LANGUAGE_EXTENSIONS[ext] ?? 'unknown';
}

export async function scanDirectory(
  projectRoot: string,
  onProgress?: (scanned: number) => void,
): Promise<ScanResult> {
  const start = Date.now();
  log.info('scanner', 'Starting directory scan', { projectRoot });

  const gitignore = await loadGitignorePatternsWithNesting(projectRoot);
  const files: ScannedFile[] = [];
  let skippedCount = 0;
  let totalSize = 0;
  let scannedCount = 0;

  async function scanDir(dirPath: string): Promise<void> {
    let entries: string[];

    try {
      entries = await readdir(dirPath);
    } catch (err) {
      log.warn('scanner', 'Failed to read directory', { dirPath, error: (err as Error).message });
      return;
    }

    if (dirPath !== projectRoot) {
      const nestedGitignorePath = join(dirPath, '.gitignore');
      try {
        const file = Bun.file(nestedGitignorePath);
        if (await file.exists()) {
          const content = await file.text();
          gitignore.addNestedGitignore(dirPath, content);
          log.debug('scanner', 'Loaded nested .gitignore', { path: nestedGitignorePath });
        }
      } catch {
        // Ignore errors reading nested .gitignore
      }
    }

    for (const entryName of entries) {
      const fullPath = join(dirPath, entryName);

      let entryStat: Awaited<ReturnType<typeof stat>>;
      try {
        entryStat = await stat(fullPath);
      } catch {
        continue;
      }

      const isDir = entryStat.isDirectory();

      if (gitignore.isIgnored(fullPath, isDir)) {
        skippedCount++;
        continue;
      }

      if (isDir) {
        await scanDir(fullPath);
        continue;
      }

      if (shouldIgnoreFile(entryName)) {
        skippedCount++;
        continue;
      }

      const language = getLanguageFromPath(entryName);
      if (language === 'unknown') {
        skippedCount++;
        continue;
      }

      if (entryStat.size > 1024 * 1024) {
        log.debug('scanner', 'Skipping large file', { path: fullPath, size: entryStat.size });
        skippedCount++;
        continue;
      }

      if (entryStat.size === 0) {
        skippedCount++;
        continue;
      }

      files.push({
        path: fullPath,
        relativePath: relative(projectRoot, fullPath),
        size: entryStat.size,
        mtime: entryStat.mtimeMs,
        language,
      });

      totalSize += entryStat.size;
      scannedCount++;

      if (onProgress && scannedCount % 100 === 0) {
        onProgress(scannedCount);
      }
    }
  }

  await scanDir(projectRoot);

  log.info('scanner', 'Directory scan complete', {
    files: files.length,
    skipped: skippedCount,
    totalSize,
    ms: Date.now() - start,
  });

  return { files, skippedCount, totalSize };
}

export async function getFileMtime(filePath: string): Promise<number | null> {
  try {
    const fileStat = await stat(filePath);
    return fileStat.mtimeMs;
  } catch {
    return null;
  }
}

export async function fileExists(filePath: string): Promise<boolean> {
  try {
    await stat(filePath);
    return true;
  } catch {
    return false;
  }
}

export { loadGitignorePatterns, type GitignoreFilter };
