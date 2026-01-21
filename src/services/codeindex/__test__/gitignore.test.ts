import { describe, expect, test, beforeEach, afterEach } from 'bun:test';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parseGitignore, loadGitignorePatterns, shouldIgnoreFile } from '../gitignore.js';

describe('gitignore', () => {
  describe('parseGitignore', () => {
    test('parses simple patterns', () => {
      const patterns = parseGitignore('node_modules\n*.log');
      expect(patterns).toHaveLength(2);
    });

    test('handles negation patterns', () => {
      const patterns = parseGitignore('*.log\n!important.log');
      expect(patterns).toHaveLength(2);
      expect(patterns[1]?.negated).toBe(true);
    });

    test('handles directory-only patterns', () => {
      const patterns = parseGitignore('dist/');
      expect(patterns).toHaveLength(1);
      expect(patterns[0]?.directoryOnly).toBe(true);
    });

    test('ignores comments and blank lines', () => {
      const patterns = parseGitignore('# comment\n\nnode_modules');
      expect(patterns).toHaveLength(1);
    });

    test('handles glob patterns with double asterisk', () => {
      const patterns = parseGitignore('**/test/**/*.ts');
      expect(patterns).toHaveLength(1);
    });

    test('handles character classes', () => {
      const patterns = parseGitignore('*.log[0-9]');
      expect(patterns).toHaveLength(1);
    });

    test('handles question mark wildcard', () => {
      const patterns = parseGitignore('file?.txt');
      expect(patterns).toHaveLength(1);
    });

    test('handles escaped characters', () => {
      const patterns = parseGitignore('file\\#name.txt');
      expect(patterns).toHaveLength(1);
    });

    test('handles patterns starting with slash', () => {
      const patterns = parseGitignore('/root-file.txt');
      expect(patterns).toHaveLength(1);
    });

    test('returns empty array for empty input', () => {
      const patterns = parseGitignore('');
      expect(patterns).toHaveLength(0);
    });

    test('returns empty array for only comments', () => {
      const patterns = parseGitignore('# comment 1\n# comment 2');
      expect(patterns).toHaveLength(0);
    });
  });

  describe('GitignoreFilter.isIgnored', () => {
    let testDir: string;

    beforeEach(async () => {
      testDir = `/tmp/gitignore-test-${Date.now()}`;
      await mkdir(testDir, { recursive: true });
    });

    afterEach(async () => {
      await rm(testDir, { recursive: true, force: true });
    });

    test('matches default ignore patterns for directories', async () => {
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored(join(testDir, 'node_modules'), true)).toBe(true);
      expect(filter.isIgnored(join(testDir, '.git'), true)).toBe(true);
      expect(filter.isIgnored(join(testDir, 'dist'), true)).toBe(true);
    });

    test('matches gitignore patterns from file', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.test.ts\nbuild/');
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored(join(testDir, 'app.test.ts'), false)).toBe(true);
      expect(filter.isIgnored(join(testDir, 'build'), true)).toBe(true);
    });

    test('respects negation', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.log\n!important.log');
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored(join(testDir, 'debug.log'), false)).toBe(true);
      expect(filter.isIgnored(join(testDir, 'important.log'), false)).toBe(false);
    });

    test('matches directory-only patterns correctly', async () => {
      await writeFile(join(testDir, '.gitignore'), 'mydir/');
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored(join(testDir, 'mydir'), true)).toBe(true);
      expect(filter.isIgnored(join(testDir, 'mydir'), false)).toBe(false);
    });

    test('does not ignore allowed files', async () => {
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored(join(testDir, 'src/main.ts'), false)).toBe(false);
      expect(filter.isIgnored(join(testDir, 'package.json'), false)).toBe(false);
    });

    test('generates consistent hash', async () => {
      const filter1 = await loadGitignorePatterns(testDir);
      const filter2 = await loadGitignorePatterns(testDir);
      expect(filter1.hash).toBe(filter2.hash);
    });

    test('hash changes when gitignore changes', async () => {
      const filter1 = await loadGitignorePatterns(testDir);
      await writeFile(join(testDir, '.gitignore'), 'newpattern/');
      const filter2 = await loadGitignorePatterns(testDir);
      expect(filter1.hash).not.toBe(filter2.hash);
    });

    test('handles missing gitignore file gracefully', async () => {
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.hash).toBeDefined();
      expect(filter.isIgnored(join(testDir, 'src/app.ts'), false)).toBe(false);
    });

    test('matches nested directory paths', async () => {
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored(join(testDir, 'src/components/node_modules'), true)).toBe(true);
    });

    test('handles relative paths for directories', async () => {
      const filter = await loadGitignorePatterns(testDir);
      expect(filter.isIgnored('node_modules', true)).toBe(true);
    });
  });

  describe('shouldIgnoreFile', () => {
    test('ignores binary files', () => {
      expect(shouldIgnoreFile('image.png')).toBe(true);
      expect(shouldIgnoreFile('app.exe')).toBe(true);
      expect(shouldIgnoreFile('photo.jpg')).toBe(true);
      expect(shouldIgnoreFile('music.mp3')).toBe(true);
      expect(shouldIgnoreFile('archive.zip')).toBe(true);
      expect(shouldIgnoreFile('document.pdf')).toBe(true);
    });

    test('ignores hidden files except allowlist', () => {
      expect(shouldIgnoreFile('.hidden')).toBe(true);
      expect(shouldIgnoreFile('.DS_Store')).toBe(true);
      expect(shouldIgnoreFile('.settings')).toBe(true);
    });

    test('allows specific dotfiles', () => {
      expect(shouldIgnoreFile('.gitignore')).toBe(false);
      expect(shouldIgnoreFile('.env.example')).toBe(false);
      expect(shouldIgnoreFile('.env.local')).toBe(false);
    });

    test('does not ignore code files', () => {
      expect(shouldIgnoreFile('main.ts')).toBe(false);
      expect(shouldIgnoreFile('app.js')).toBe(false);
      expect(shouldIgnoreFile('script.py')).toBe(false);
      expect(shouldIgnoreFile('lib.rs')).toBe(false);
    });

    test('handles case insensitivity', () => {
      expect(shouldIgnoreFile('IMAGE.PNG')).toBe(true);
      expect(shouldIgnoreFile('App.EXE')).toBe(true);
    });

    test('ignores font files', () => {
      expect(shouldIgnoreFile('font.ttf')).toBe(true);
      expect(shouldIgnoreFile('font.woff2')).toBe(true);
    });

    test('ignores database files', () => {
      expect(shouldIgnoreFile('data.db')).toBe(true);
      expect(shouldIgnoreFile('app.sqlite')).toBe(true);
      expect(shouldIgnoreFile('app.sqlite3')).toBe(true);
    });
  });
});
