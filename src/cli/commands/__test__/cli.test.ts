import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { closeDatabase, getDatabase } from '../../../db/database.js';

let testDir: string;

beforeEach(async () => {
  const testId = Date.now() + Math.floor(Math.random() * 100000);
  testDir = `/tmp/ccmemory-cli-test-${testId}`;
  process.env.CCMEMORY_DATA_DIR = testDir;
  process.env.CCMEMORY_CONFIG_DIR = testDir;

  const db = await getDatabase();
  await db.execute(`
    INSERT INTO projects (id, name, path, created_at) VALUES
    ('proj1', 'Test Project', '/test/project-${testId}', ${Date.now()})
  `);

  const now = Date.now();
  await db.execute(`
    INSERT INTO memories (id, project_id, content, sector, tier, salience, created_at, updated_at, last_accessed, access_count, is_deleted)
    VALUES
    ('mem1', 'proj1', 'React is a JavaScript library for building user interfaces', 'semantic', 'project', 0.8, ${now}, ${now}, ${now}, 1, 0),
    ('mem2', 'proj1', 'To run tests, use bun test command', 'procedural', 'project', 0.6, ${now}, ${now}, ${now}, 1, 0),
    ('mem3', 'proj1', 'User asked about testing patterns', 'episodic', 'session', 0.3, ${now}, ${now}, ${now}, 1, 0)
  `);
});

afterEach(async () => {
  await closeDatabase();
  delete process.env.CCMEMORY_DATA_DIR;
  delete process.env.CCMEMORY_CONFIG_DIR;
  await Bun.$`rm -rf ${testDir}`.quiet();
});

describe('CLI Help', () => {
  test('prints help when no command given', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('CCMemory');
    expect(result).toContain('Usage:');
    expect(result).toContain('Commands:');
  });

  test('prints help with help command', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts help`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('search <query>');
  });

  test('prints version', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts --version`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('0.1.0');
  });
});

describe('CLI Config Command', () => {
  test('shows all config when no args', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts config`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('embedding');
  });

  test('gets specific key', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts config embedding.provider`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toMatch(/ollama|openrouter/);
  });

  test('sets value', async () => {
    await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts config capture.enabled false`.env({
      ...process.env,
      CCMEMORY_DATA_DIR: testDir,
      CCMEMORY_CONFIG_DIR: testDir,
    });
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts config capture.enabled`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('false');
  });
});

describe('CLI Stats Command', () => {
  test('shows statistics', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts stats`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('CCMemory Statistics');
    expect(result).toContain('Totals:');
    expect(result).toContain('Memories:');
  });

  test('shows code index stats when code is indexed', async () => {
    const db = await getDatabase();
    const now = Date.now();

    await db.execute(`
      INSERT INTO indexed_files (id, project_id, path, checksum, mtime, indexed_at)
      VALUES ('if1', 'proj1', '/test/file1.ts', 'abc123', ${now}, ${now})
    `);

    await db.execute(`
      INSERT INTO documents (id, project_id, is_code, language, source_path, source_type, full_content, line_count, created_at, updated_at)
      VALUES ('doc1', 'proj1', 1, 'ts', '/test/file1.ts', 'file', 'function foo() {}', 50, ${now}, ${now})
    `);

    await db.execute(`
      INSERT INTO document_chunks (id, document_id, content, chunk_index, start_line, end_line)
      VALUES ('chunk1', 'doc1', 'function foo() {}', 0, 1, 10)
    `);

    await db.execute(`
      INSERT INTO code_index_state (project_id, indexed_files, last_indexed_at)
      VALUES ('proj1', 1, ${now})
    `);

    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts stats`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();

    expect(result).toContain('Code Index:');
    expect(result).toContain('Indexed Files:');
    expect(result).toContain('Code Documents:');
    expect(result).toContain('By Language:');
    expect(result).toContain('ts:');
  });
});

describe('CLI Show Command', () => {
  test('shows memory details', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts show mem1`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('mem1');
    expect(result).toContain('React');
    expect(result).toContain('semantic');
  });

  test('outputs JSON when requested', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts show mem1 --json`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    const parsed = JSON.parse(result);
    expect(parsed.id).toBe('mem1');
    expect(parsed.sector).toBe('semantic');
  });

  test('exits with error for non-existent memory', async () => {
    const proc = Bun.spawn(['bun', '/home/joey/Documents/ccmemory/src/cli/index.ts', 'show', 'nonexistent'], {
      env: {
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      },
      stderr: 'pipe',
    });
    await proc.exited;
    expect(proc.exitCode).toBe(1);
  });
});

describe('CLI Delete Command', () => {
  test('deletes memory with force flag', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts delete mem1 --force`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('soft-deleted');

    const db = await getDatabase();
    const check = await db.execute('SELECT is_deleted FROM memories WHERE id = ?', ['mem1']);
    expect(check.rows[0]?.['is_deleted']).toBe(1);
  });
});

describe('CLI Export Command', () => {
  test('exports as JSON to stdout', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts export`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    const parsed = JSON.parse(result);
    expect(Array.isArray(parsed)).toBe(true);
    expect(parsed.length).toBe(3);
  });

  test('exports as CSV', async () => {
    const result = await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts export -f csv`
      .env({
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      })
      .text();
    expect(result).toContain('id,sector,tier,salience,content,created_at');
    expect(result).toContain('mem1');
  });

  test('exports to file', async () => {
    const outFile = `${testDir}/export.json`;
    await Bun.$`bun /home/joey/Documents/ccmemory/src/cli/index.ts export -o ${outFile}`.env({
      ...process.env,
      CCMEMORY_DATA_DIR: testDir,
      CCMEMORY_CONFIG_DIR: testDir,
    });
    const content = await Bun.file(outFile).text();
    const parsed = JSON.parse(content);
    expect(parsed.length).toBe(3);
  });
});

describe('CLI Unknown Command', () => {
  test('exits with error for unknown command', async () => {
    const proc = Bun.spawn(['bun', '/home/joey/Documents/ccmemory/src/cli/index.ts', 'foobar'], {
      env: {
        ...process.env,
        CCMEMORY_DATA_DIR: testDir,
        CCMEMORY_CONFIG_DIR: testDir,
      },
      stderr: 'pipe',
    });
    await proc.exited;
    expect(proc.exitCode).toBe(1);
  });
});
