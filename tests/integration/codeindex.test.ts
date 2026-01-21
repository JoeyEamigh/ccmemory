import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { closeDatabase, createDatabase, setDatabase, type Database } from '../../src/db/database.js';
import { createEmbeddingService } from '../../src/services/embedding/index.js';
import type { EmbeddingService } from '../../src/services/embedding/types.js';
import { createCodeIndexService } from '../../src/services/codeindex/index.js';
import { getOrCreateProject } from '../../src/services/project.js';
import { releaseLock, listActiveWatchers } from '../../src/services/codeindex/coordination.js';

describe('Code Index Integration', () => {
  const testDir = `/tmp/ccmemory-codeindex-integration-${Date.now()}`;
  const projectDir = join(testDir, 'test-project');
  let db: Database;
  let embeddingService: EmbeddingService;

  beforeAll(async () => {
    await mkdir(testDir, { recursive: true });
    await mkdir(projectDir, { recursive: true });

    process.env['CCMEMORY_DATA_DIR'] = testDir;
    process.env['CCMEMORY_CONFIG_DIR'] = testDir;
    process.env['CCMEMORY_CACHE_DIR'] = testDir;

    db = await createDatabase(join(testDir, 'test.db'));
    setDatabase(db);

    embeddingService = await createEmbeddingService();

    await writeFile(
      join(projectDir, 'main.ts'),
      `
import { helper } from './utils';

export function main(): void {
  console.log('Hello, world!');
  helper();
}

export class Application {
  private name: string;

  constructor(name: string) {
    this.name = name;
  }

  run(): void {
    main();
  }
}
`,
    );

    await writeFile(
      join(projectDir, 'utils.ts'),
      `
export function helper(): void {
  console.log('Helper called');
}

export function formatDate(date: Date): string {
  return date.toISOString();
}

export const PI = 3.14159;
`,
    );

    await writeFile(
      join(projectDir, 'script.py'),
      `
def process_data(data):
    """Process input data and return results."""
    return [x * 2 for x in data]

class DataProcessor:
    def __init__(self, name):
        self.name = name

    def run(self):
        return self.name
`,
    );

    await writeFile(join(projectDir, '.gitignore'), 'dist/\n*.log\nnode_modules/');
    await mkdir(join(projectDir, 'dist'), { recursive: true });
    await writeFile(join(projectDir, 'dist/bundle.js'), 'bundled code');
    await mkdir(join(projectDir, 'node_modules/lib'), { recursive: true });
    await writeFile(join(projectDir, 'node_modules/lib/index.js'), 'module.exports = {};');
  });

  afterAll(async () => {
    const watchers = await listActiveWatchers();
    for (const w of watchers) {
      if (w.projectPath.startsWith(testDir)) {
        await releaseLock(w.projectPath);
      }
    }

    closeDatabase();
    await rm(testDir, { recursive: true, force: true });
    delete process.env['CCMEMORY_DATA_DIR'];
    delete process.env['CCMEMORY_CONFIG_DIR'];
    delete process.env['CCMEMORY_CACHE_DIR'];
  });

  describe('CodeIndexService.index', () => {
    test('indexes project code files and creates embeddings', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      const progress = await codeIndex.index(projectDir, project.id);

      expect(progress.phase).toBe('complete');
      expect(progress.totalFiles).toBeGreaterThan(0);
      expect(progress.indexedFiles).toBe(progress.totalFiles);
      expect(progress.errors.length).toBe(0);
    });

    test('respects gitignore patterns - excludes dist and node_modules', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);

      const results = await db.execute(
        `SELECT source_path FROM documents WHERE project_id = ? AND is_code = 1`,
        [project.id],
      );

      const paths = results.rows.map(r => String(r['source_path']));
      expect(paths.some(p => p.includes('dist/'))).toBe(false);
      expect(paths.some(p => p.includes('node_modules/'))).toBe(false);
    });

    test('dry run reports file count without indexing', async () => {
      const dryRunProject = join(testDir, 'dry-run-project');
      await mkdir(dryRunProject, { recursive: true });
      await writeFile(join(dryRunProject, 'test.ts'), 'const x = 1;');

      const project = await getOrCreateProject(dryRunProject);
      const codeIndex = createCodeIndexService(embeddingService);

      const progress = await codeIndex.index(dryRunProject, project.id, { dryRun: true });

      expect(progress.totalFiles).toBe(1);
      expect(progress.indexedFiles).toBe(0);

      const state = await codeIndex.getState(project.id);
      expect(state).toBeNull();
    });

    test('skips unchanged files on re-index', async () => {
      const reindexProject = join(testDir, 'reindex-project');
      await mkdir(reindexProject, { recursive: true });
      await writeFile(join(reindexProject, 'file.ts'), 'const x = 1;');

      const project = await getOrCreateProject(reindexProject);
      const codeIndex = createCodeIndexService(embeddingService);

      const first = await codeIndex.index(reindexProject, project.id);
      expect(first.indexedFiles).toBe(1);

      const second = await codeIndex.index(reindexProject, project.id);
      expect(second.indexedFiles).toBe(0);
    });

    test('re-indexes changed files with force option', async () => {
      const forceProject = join(testDir, 'force-project');
      await mkdir(forceProject, { recursive: true });
      await writeFile(join(forceProject, 'file.ts'), 'const x = 1;');

      const project = await getOrCreateProject(forceProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(forceProject, project.id);

      await writeFile(join(forceProject, 'file.ts'), 'const x = 2; // modified');
      const forceResult = await codeIndex.index(forceProject, project.id, { force: true });

      expect(forceResult.indexedFiles).toBe(1);
    });

    test('force option includes unchanged files in scan but skips identical content', async () => {
      const forceProject2 = join(testDir, 'force-project-2');
      await mkdir(forceProject2, { recursive: true });
      await writeFile(join(forceProject2, 'file.ts'), 'const x = 1;');

      const project = await getOrCreateProject(forceProject2);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(forceProject2, project.id);

      const forceResult = await codeIndex.index(forceProject2, project.id, { force: true });

      expect(forceResult.totalFiles).toBe(1);
      expect(forceResult.indexedFiles).toBe(0);
    });

    test('indexes multiple files in parallel correctly', async () => {
      const parallelProject = join(testDir, 'parallel-project');
      await mkdir(parallelProject, { recursive: true });

      for (let i = 0; i < 10; i++) {
        await writeFile(
          join(parallelProject, `file${i}.ts`),
          `export function fn${i}() { return ${i}; }`,
        );
      }

      const project = await getOrCreateProject(parallelProject);
      const codeIndex = createCodeIndexService(embeddingService);

      const progress = await codeIndex.index(parallelProject, project.id);

      expect(progress.totalFiles).toBe(10);
      expect(progress.indexedFiles).toBe(10);
      expect(progress.errors.length).toBe(0);

      const results = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(results.rows[0]?.['count'])).toBe(10);
    });
  });

  describe('CodeIndexService.search', () => {
    test('returns relevant code snippets for semantic query', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);

      const results = await codeIndex.search({
        query: 'main function entry point application',
        projectId: project.id,
        limit: 5,
      });

      expect(results.length).toBeGreaterThan(0);
      expect(results[0]?.score).toBeGreaterThan(0);
    });

    test('filters results by language', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);

      const tsResults = await codeIndex.search({
        query: 'function',
        projectId: project.id,
        language: 'ts',
        limit: 10,
      });

      for (const result of tsResults) {
        expect(result.language).toBe('ts');
      }

      const pyResults = await codeIndex.search({
        query: 'function',
        projectId: project.id,
        language: 'py',
        limit: 10,
      });

      for (const result of pyResults) {
        expect(result.language).toBe('py');
      }
    });

    test('returns empty array for unindexed project', async () => {
      const project = await getOrCreateProject('/nonexistent/project');
      const codeIndex = createCodeIndexService(embeddingService);

      const results = await codeIndex.search({
        query: 'anything',
        projectId: project.id,
      });

      expect(results).toEqual([]);
    });

    test('results include file path and line numbers', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);

      const results = await codeIndex.search({
        query: 'class application',
        projectId: project.id,
        limit: 5,
      });

      expect(results.length).toBeGreaterThan(0);
      const result = results[0];
      expect(result?.path).toBeDefined();
      expect(result?.startLine).toBeGreaterThan(0);
      expect(result?.endLine).toBeGreaterThanOrEqual(result?.startLine ?? 0);
    });

    test('results include extracted symbols', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);

      const results = await codeIndex.search({
        query: 'application class',
        projectId: project.id,
        limit: 10,
      });

      const hasSymbols = results.some(r => r.symbols.length > 0);
      expect(hasSymbols).toBe(true);
    });
  });

  describe('CodeIndexService.cleanupDeletedFiles', () => {
    test('removes index entries for deleted files', async () => {
      const cleanupProject = join(testDir, 'cleanup-project');
      await mkdir(cleanupProject, { recursive: true });
      await writeFile(join(cleanupProject, 'keep.ts'), 'const keep = 1;');
      await writeFile(join(cleanupProject, 'delete.ts'), 'const remove = 2;');

      const project = await getOrCreateProject(cleanupProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(cleanupProject, project.id);

      const beforeCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(beforeCount.rows[0]?.['count'])).toBe(2);

      await rm(join(cleanupProject, 'delete.ts'));

      const deleted = await codeIndex.cleanupDeletedFiles(project.id);
      expect(deleted).toBe(1);

      const afterCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(afterCount.rows[0]?.['count'])).toBe(1);
    });
  });

  describe('CodeIndexService.deleteFile', () => {
    test('removes specific file from index by path', async () => {
      const deleteProject = join(testDir, 'delete-file-project');
      await mkdir(deleteProject, { recursive: true });
      await writeFile(join(deleteProject, 'keep.ts'), 'const keep = 1;');
      await writeFile(join(deleteProject, 'target.ts'), 'const target = 2;');

      const project = await getOrCreateProject(deleteProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(deleteProject, project.id);

      const beforeCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(beforeCount.rows[0]?.['count'])).toBe(2);

      const deleted = await codeIndex.deleteFile(project.id, join(deleteProject, 'target.ts'));
      expect(deleted).toBe(true);

      const afterCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(afterCount.rows[0]?.['count'])).toBe(1);

      const remaining = await db.execute(
        'SELECT path FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(String(remaining.rows[0]?.['path'])).toContain('keep.ts');
    });

    test('returns false for non-existent file', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      const deleted = await codeIndex.deleteFile(project.id, '/nonexistent/file.ts');
      expect(deleted).toBe(false);
    });

    test('also removes document chunks and vectors', async () => {
      const chunksProject = join(testDir, 'chunks-project');
      await mkdir(chunksProject, { recursive: true });
      await writeFile(join(chunksProject, 'file.ts'), 'export function foo() { return 1; }');

      const project = await getOrCreateProject(chunksProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(chunksProject, project.id);

      const chunksBeforeResult = await db.execute(
        `SELECT COUNT(*) as count FROM document_chunks dc
         JOIN documents d ON dc.document_id = d.id
         WHERE d.project_id = ?`,
        [project.id],
      );
      expect(Number(chunksBeforeResult.rows[0]?.['count'])).toBeGreaterThan(0);

      await codeIndex.deleteFile(project.id, join(chunksProject, 'file.ts'));

      const chunksAfterResult = await db.execute(
        `SELECT COUNT(*) as count FROM document_chunks dc
         JOIN documents d ON dc.document_id = d.id
         WHERE d.project_id = ?`,
        [project.id],
      );
      expect(Number(chunksAfterResult.rows[0]?.['count'])).toBe(0);
    });
  });

  describe('CodeIndexService.processFileChanges', () => {
    test('handles delete events by removing file from index', async () => {
      const processProject = join(testDir, 'process-changes-project');
      await mkdir(processProject, { recursive: true });
      await writeFile(join(processProject, 'keep.ts'), 'const keep = 1;');
      await writeFile(join(processProject, 'toDelete.ts'), 'const toDelete = 2;');

      const project = await getOrCreateProject(processProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(processProject, project.id);

      const beforeCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(beforeCount.rows[0]?.['count'])).toBe(2);

      await codeIndex.processFileChanges(processProject, project.id, [
        { type: 'delete', path: join(processProject, 'toDelete.ts'), timestamp: Date.now() },
      ]);

      const afterCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(afterCount.rows[0]?.['count'])).toBe(1);

      const remaining = await db.execute(
        'SELECT path FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(String(remaining.rows[0]?.['path'])).toContain('keep.ts');
    });

    test('handles add events by indexing new file', async () => {
      const addProject = join(testDir, 'add-changes-project');
      await mkdir(addProject, { recursive: true });
      await writeFile(join(addProject, 'existing.ts'), 'const existing = 1;');

      const project = await getOrCreateProject(addProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(addProject, project.id);

      const beforeCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(beforeCount.rows[0]?.['count'])).toBe(1);

      await writeFile(join(addProject, 'newFile.ts'), 'const newFile = 2;');

      await codeIndex.processFileChanges(addProject, project.id, [
        { type: 'add', path: join(addProject, 'newFile.ts'), timestamp: Date.now() },
      ]);

      const afterCount = await db.execute(
        'SELECT COUNT(*) as count FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      expect(Number(afterCount.rows[0]?.['count'])).toBe(2);
    });

    test('handles change events by re-indexing modified file', async () => {
      const changeProject = join(testDir, 'change-project');
      await mkdir(changeProject, { recursive: true });
      await writeFile(join(changeProject, 'file.ts'), 'const original = 1;');

      const project = await getOrCreateProject(changeProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(changeProject, project.id);

      const beforeResult = await db.execute(
        'SELECT checksum FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      const originalChecksum = String(beforeResult.rows[0]?.['checksum']);

      await writeFile(join(changeProject, 'file.ts'), 'const modified = 2;');

      await codeIndex.processFileChanges(changeProject, project.id, [
        { type: 'change', path: join(changeProject, 'file.ts'), timestamp: Date.now() },
      ]);

      const afterResult = await db.execute(
        'SELECT checksum FROM indexed_files WHERE project_id = ?',
        [project.id],
      );
      const newChecksum = String(afterResult.rows[0]?.['checksum']);

      expect(newChecksum).not.toBe(originalChecksum);
    });
  });

  describe('CodeIndexService.exportIndex', () => {
    test('exports indexed project data', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);

      const exported = await codeIndex.exportIndex(projectDir, project.id);

      expect(exported).not.toBeNull();
      expect(exported?.version).toBe(1);
      expect(exported?.files.length).toBeGreaterThan(0);
      expect(exported?.state.projectId).toBe(project.id);

      const tsFile = exported?.files.find(f => f.relativePath.endsWith('.ts'));
      expect(tsFile).toBeDefined();
      expect(tsFile?.chunks.length).toBeGreaterThan(0);
      expect(tsFile?.chunks[0]?.vector.length).toBeGreaterThan(0);
    });

    test('returns null for unindexed project', async () => {
      const project = await getOrCreateProject('/nonexistent/project');
      const codeIndex = createCodeIndexService(embeddingService);

      const exported = await codeIndex.exportIndex('/nonexistent/project', project.id);

      expect(exported).toBeNull();
    });
  });

  describe('CodeIndexService.importIndex', () => {
    test('imports exported index data', async () => {
      const exportProject = join(testDir, 'export-project');
      await mkdir(exportProject, { recursive: true });
      await writeFile(join(exportProject, 'source.ts'), 'export const value = 42;');

      const project1 = await getOrCreateProject(exportProject);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(exportProject, project1.id);
      const exported = await codeIndex.exportIndex(exportProject, project1.id);
      expect(exported).not.toBeNull();

      const importProject = join(testDir, 'import-project');
      await mkdir(importProject, { recursive: true });

      const project2 = await getOrCreateProject(importProject);
      const result = await codeIndex.importIndex(importProject, project2.id, exported!);

      expect(result.imported).toBe(1);
      expect(result.skipped).toBe(0);

      const state = await codeIndex.getState(project2.id);
      expect(state).not.toBeNull();
      expect(state?.indexedFiles).toBe(1);
    });

    test('skips files with matching checksum', async () => {
      const project = await getOrCreateProject(projectDir);
      const codeIndex = createCodeIndexService(embeddingService);

      await codeIndex.index(projectDir, project.id);
      const exported = await codeIndex.exportIndex(projectDir, project.id);
      expect(exported).not.toBeNull();

      const result = await codeIndex.importIndex(projectDir, project.id, exported!);

      expect(result.skipped).toBe(exported!.files.length);
      expect(result.imported).toBe(0);
    });
  });

  describe('CodeIndexService.getState', () => {
    test('returns state after indexing', async () => {
      const stateProject = join(testDir, 'state-project');
      await mkdir(stateProject, { recursive: true });
      await writeFile(join(stateProject, 'test.ts'), 'const x = 1;');

      const project = await getOrCreateProject(stateProject);
      const codeIndex = createCodeIndexService(embeddingService);

      const progress = await codeIndex.index(stateProject, project.id);

      const state = await codeIndex.getState(project.id);

      expect(state).not.toBeNull();
      expect(state?.projectId).toBe(project.id);
      expect(state?.lastIndexedAt).toBeGreaterThan(0);
      expect(state?.indexedFiles).toBe(progress.indexedFiles);
    });

    test('returns null for unindexed project', async () => {
      const codeIndex = createCodeIndexService(embeddingService);
      const state = await codeIndex.getState('nonexistent-project-id');
      expect(state).toBeNull();
    });
  });
});

describe('Code Index CLI Commands', () => {
  const cliTestDir = `/tmp/ccmemory-codeindex-cli-${Date.now()}`;
  const cliProjectDir = join(cliTestDir, 'cli-project');

  beforeAll(async () => {
    await mkdir(cliTestDir, { recursive: true });
    await mkdir(cliProjectDir, { recursive: true });

    await writeFile(
      join(cliProjectDir, 'index.ts'),
      `
export function main() {
  console.log('Hello');
}
`,
    );

    await writeFile(
      join(cliProjectDir, 'util.ts'),
      `
export function helper() {
  return 42;
}
`,
    );
  });

  afterAll(async () => {
    await rm(cliTestDir, { recursive: true, force: true });
  });

  test('code-index command indexes project and reports progress', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-index ${cliProjectDir}`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    expect(result).toContain('Indexing code in:');
    expect(result).toContain('Indexing complete');
    expect(result).toContain('Files scanned:');
    expect(result).toContain('Files indexed:');
  });

  test('code-index --dry-run shows files without indexing', async () => {
    const dryRunDir = join(cliTestDir, 'dryrun-project');
    await mkdir(dryRunDir, { recursive: true });
    await writeFile(join(dryRunDir, 'file.ts'), 'const x = 1;');

    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-index --dry-run ${dryRunDir}`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    expect(result).toContain('Dry run complete');
    expect(result).toContain('Files found:');
    expect(result).toContain('Run without --dry-run to index');
  });

  test('code-search returns results after indexing', async () => {
    const searchResult = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-search -p ${cliProjectDir} "main function"`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    expect(searchResult).toContain('index.ts');
    expect(searchResult).toContain('result');
  });

  test('code-search --json outputs JSON format', async () => {
    const jsonResult = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-search --json -p ${cliProjectDir} "helper"`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    const parsed = JSON.parse(jsonResult);
    expect(parsed.results).toBeInstanceOf(Array);
    expect(parsed.query).toBe('helper');
    expect(parsed.projectPath).toBe(cliProjectDir);
  });

  test('code-search -l filters by language', async () => {
    await writeFile(join(cliProjectDir, 'script.py'), 'def foo(): pass');
    await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-index -f ${cliProjectDir}`.env({
      ...process.env,
      CCMEMORY_DATA_DIR: cliTestDir,
      CCMEMORY_CONFIG_DIR: cliTestDir,
      CCMEMORY_CACHE_DIR: cliTestDir,
    });

    const tsResult = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-search --json -l ts -p ${cliProjectDir} "function"`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    const parsed = JSON.parse(tsResult);
    for (const result of parsed.results) {
      expect(result.language).toBe('ts');
    }
  });

  test('code-search errors if project not indexed', async () => {
    const unindexedDir = join(cliTestDir, 'unindexed');
    await mkdir(unindexedDir, { recursive: true });
    await writeFile(join(unindexedDir, 'file.ts'), 'const x = 1;');

    const proc = Bun.spawn(
      ['bun', '/home/joey/Documents/ccmemory/src/main.ts', 'code-search', '-p', unindexedDir, 'test'],
      {
        env: {
          ...process.env,
          CCMEMORY_DATA_DIR: cliTestDir,
          CCMEMORY_CONFIG_DIR: cliTestDir,
          CCMEMORY_CACHE_DIR: cliTestDir,
        },
        stderr: 'pipe',
      },
    );
    await proc.exited;
    expect(proc.exitCode).toBe(1);
  });

  test('auto-start watcher only when index exists', async () => {
    const autoStartDir = join(cliTestDir, 'autostart-project');
    await mkdir(autoStartDir, { recursive: true });
    await writeFile(join(autoStartDir, 'main.ts'), 'export const main = 1;');

    const result1 = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts watch --status`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    const hasAutoStartWatcher = result1.includes(autoStartDir);
    expect(hasAutoStartWatcher).toBe(false);

    await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-index ${autoStartDir}`.env({
      ...process.env,
      CCMEMORY_DATA_DIR: cliTestDir,
      CCMEMORY_CONFIG_DIR: cliTestDir,
      CCMEMORY_CACHE_DIR: cliTestDir,
    });

    const indexResult = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts code-search -p ${autoStartDir} "main"`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();
    expect(indexResult).toContain('main.ts');
  });

  test('watch --status shows active watchers', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/main.ts watch --status`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: cliTestDir,
        CCMEMORY_CONFIG_DIR: cliTestDir,
        CCMEMORY_CACHE_DIR: cliTestDir,
      })
      .text();

    expect(result.includes('Active watchers:') || result.includes('No active watchers')).toBe(true);
  });
});
