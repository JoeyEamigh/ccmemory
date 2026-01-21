import { describe, expect, test, beforeEach, afterEach } from 'bun:test';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { scanDirectory, getFileMtime, fileExists } from '../scanner.js';

describe('scanner', () => {
  let testDir: string;

  beforeEach(async () => {
    testDir = `/tmp/scanner-test-${Date.now()}`;
    await mkdir(testDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(testDir, { recursive: true, force: true });
  });

  describe('scanDirectory', () => {
    test('finds code files', async () => {
      await writeFile(join(testDir, 'index.ts'), 'export const x = 1;');
      await writeFile(join(testDir, 'utils.js'), 'module.exports = {};');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(2);
    });

    test('scans recursively', async () => {
      await mkdir(join(testDir, 'src/components'), { recursive: true });
      await writeFile(join(testDir, 'src/main.ts'), 'const x = 1;');
      await writeFile(join(testDir, 'src/components/Button.tsx'), 'export const Button = () => null;');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(2);
      expect(result.files.some(f => f.path.includes('components'))).toBe(true);
    });

    test('respects gitignore patterns', async () => {
      await mkdir(join(testDir, 'node_modules/lib'), { recursive: true });
      await writeFile(join(testDir, 'node_modules/lib/index.js'), 'module.exports = {};');
      await writeFile(join(testDir, 'index.ts'), 'import lib from "lib";');

      const result = await scanDirectory(testDir);
      expect(result.files.some(f => f.path.includes('node_modules'))).toBe(false);
      expect(result.files.length).toBe(1);
    });

    test('skips large files', async () => {
      const largeContent = 'x'.repeat(2 * 1024 * 1024);
      await writeFile(join(testDir, 'large.ts'), largeContent);
      await writeFile(join(testDir, 'small.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files.every(f => f.size <= 1024 * 1024)).toBe(true);
      expect(result.files.length).toBe(1);
    });

    test('skips empty files', async () => {
      await writeFile(join(testDir, 'empty.ts'), '');
      await writeFile(join(testDir, 'nonempty.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(1);
      expect(result.files[0]?.relativePath).toBe('nonempty.ts');
    });

    test('detects correct language from extension', async () => {
      await writeFile(join(testDir, 'app.ts'), 'const x = 1;');
      await writeFile(join(testDir, 'app.py'), 'x = 1');
      await writeFile(join(testDir, 'app.rs'), 'let x = 1;');
      await writeFile(join(testDir, 'app.go'), 'var x = 1');

      const result = await scanDirectory(testDir);

      const tsFile = result.files.find(f => f.path.endsWith('.ts'));
      const pyFile = result.files.find(f => f.path.endsWith('.py'));
      const rsFile = result.files.find(f => f.path.endsWith('.rs'));
      const goFile = result.files.find(f => f.path.endsWith('.go'));

      expect(tsFile?.language).toBe('ts');
      expect(pyFile?.language).toBe('py');
      expect(rsFile?.language).toBe('rs');
      expect(goFile?.language).toBe('go');
    });

    test('reports progress via callback', async () => {
      for (let i = 0; i < 150; i++) {
        await writeFile(join(testDir, `file${i}.ts`), `const x${i} = ${i};`);
      }

      const progressCalls: number[] = [];
      await scanDirectory(testDir, n => progressCalls.push(n));

      expect(progressCalls.length).toBeGreaterThan(0);
      expect(progressCalls.every(n => n % 100 === 0)).toBe(true);
    });

    test('tracks total size', async () => {
      const content = 'const x = 1;';
      await writeFile(join(testDir, 'file1.ts'), content);
      await writeFile(join(testDir, 'file2.ts'), content);

      const result = await scanDirectory(testDir);
      expect(result.totalSize).toBe(content.length * 2);
    });

    test('tracks skipped count', async () => {
      await mkdir(join(testDir, 'node_modules'), { recursive: true });
      await writeFile(join(testDir, 'node_modules/lib.js'), 'x');
      await writeFile(join(testDir, 'image.png'), 'binary');
      await writeFile(join(testDir, 'app.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.skippedCount).toBeGreaterThan(0);
    });

    test('provides relative paths', async () => {
      await mkdir(join(testDir, 'src'), { recursive: true });
      await writeFile(join(testDir, 'src/main.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files[0]?.relativePath).toBe('src/main.ts');
    });

    test('records mtime', async () => {
      await writeFile(join(testDir, 'app.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files[0]?.mtime).toBeGreaterThan(0);
    });

    test('handles deeply nested directories', async () => {
      const deepPath = join(testDir, 'a/b/c/d/e/f');
      await mkdir(deepPath, { recursive: true });
      await writeFile(join(deepPath, 'deep.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(1);
      expect(result.files[0]?.relativePath).toBe('a/b/c/d/e/f/deep.ts');
    });

    test('skips unknown file types', async () => {
      await writeFile(join(testDir, 'data.xyz'), 'unknown format');
      await writeFile(join(testDir, 'app.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(1);
      expect(result.files[0]?.language).toBe('ts');
    });

    test('handles default ignore directories', async () => {
      await mkdir(join(testDir, 'dist'), { recursive: true });
      await mkdir(join(testDir, 'build'), { recursive: true });
      await mkdir(join(testDir, '.git'), { recursive: true });
      await writeFile(join(testDir, 'dist/bundle.js'), 'bundled');
      await writeFile(join(testDir, 'build/output.js'), 'output');
      await writeFile(join(testDir, '.git/config'), 'gitconfig');
      await writeFile(join(testDir, 'src.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(1);
      expect(result.files[0]?.relativePath).toBe('src.ts');
    });

    test('handles gitignore with custom patterns', async () => {
      await writeFile(join(testDir, '.gitignore'), '*.test.ts\n/temp/');
      await mkdir(join(testDir, 'temp'), { recursive: true });
      await writeFile(join(testDir, 'temp/file.ts'), 'const x = 1;');
      await writeFile(join(testDir, 'app.test.ts'), 'test()');
      await writeFile(join(testDir, 'app.ts'), 'const x = 1;');

      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(1);
      expect(result.files[0]?.relativePath).toBe('app.ts');
    });

    test('handles empty directory', async () => {
      const result = await scanDirectory(testDir);
      expect(result.files.length).toBe(0);
      expect(result.totalSize).toBe(0);
    });

    test('respects nested gitignore files', async () => {
      await mkdir(join(testDir, 'src/components'), { recursive: true });
      await writeFile(join(testDir, 'src/components/.gitignore'), '*.generated.ts');
      await writeFile(join(testDir, 'src/components/Button.ts'), 'export const Button = {};');
      await writeFile(join(testDir, 'src/components/Button.generated.ts'), 'export const ButtonGenerated = {};');
      await writeFile(join(testDir, 'src/main.ts'), 'import { Button } from "./components/Button";');
      await writeFile(join(testDir, 'src/main.generated.ts'), 'const generated = true;');

      const result = await scanDirectory(testDir);

      expect(result.files.some(f => f.relativePath === 'src/main.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'src/main.generated.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'src/components/Button.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'src/components/Button.generated.ts')).toBe(false);
    });

    test('nested gitignore patterns only apply to their directory subtree', async () => {
      await mkdir(join(testDir, 'lib'), { recursive: true });
      await mkdir(join(testDir, 'app'), { recursive: true });
      await writeFile(join(testDir, 'lib/.gitignore'), '*.test.ts');
      await writeFile(join(testDir, 'lib/utils.ts'), 'export const utils = {};');
      await writeFile(join(testDir, 'lib/utils.test.ts'), 'test("utils", () => {});');
      await writeFile(join(testDir, 'app/main.ts'), 'const main = true;');
      await writeFile(join(testDir, 'app/main.test.ts'), 'test("main", () => {});');

      const result = await scanDirectory(testDir);

      expect(result.files.some(f => f.relativePath === 'lib/utils.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'lib/utils.test.ts')).toBe(false);
      expect(result.files.some(f => f.relativePath === 'app/main.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'app/main.test.ts')).toBe(true);
    });

    test('deeply nested gitignore files are respected', async () => {
      await mkdir(join(testDir, 'a/b/c'), { recursive: true });
      await writeFile(join(testDir, 'a/b/c/.gitignore'), 'secret.*');
      await writeFile(join(testDir, 'a/b/c/public.ts'), 'export const pub = 1;');
      await writeFile(join(testDir, 'a/b/c/secret.ts'), 'export const secret = "hidden";');
      await writeFile(join(testDir, 'a/b/other.ts'), 'export const other = 2;');
      await writeFile(join(testDir, 'a/secret.ts'), 'export const rootSecret = "visible";');

      const result = await scanDirectory(testDir);

      expect(result.files.some(f => f.relativePath === 'a/b/c/public.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'a/b/c/secret.ts')).toBe(false);
      expect(result.files.some(f => f.relativePath === 'a/b/other.ts')).toBe(true);
      expect(result.files.some(f => f.relativePath === 'a/secret.ts')).toBe(true);
    });
  });

  describe('getFileMtime', () => {
    test('returns mtime for existing file', async () => {
      const filePath = join(testDir, 'test.ts');
      await writeFile(filePath, 'const x = 1;');

      const mtime = await getFileMtime(filePath);
      expect(mtime).toBeGreaterThan(0);
    });

    test('returns null for non-existent file', async () => {
      const mtime = await getFileMtime(join(testDir, 'nonexistent.ts'));
      expect(mtime).toBeNull();
    });
  });

  describe('fileExists', () => {
    test('returns true for existing file', async () => {
      const filePath = join(testDir, 'test.ts');
      await writeFile(filePath, 'const x = 1;');

      const exists = await fileExists(filePath);
      expect(exists).toBe(true);
    });

    test('returns false for non-existent file', async () => {
      const exists = await fileExists(join(testDir, 'nonexistent.ts'));
      expect(exists).toBe(false);
    });

    test('returns true for existing directory', async () => {
      const dirPath = join(testDir, 'subdir');
      await mkdir(dirPath);

      const exists = await fileExists(dirPath);
      expect(exists).toBe(true);
    });
  });
});
