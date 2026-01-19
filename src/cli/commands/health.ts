import { parseArgs } from 'util';
import { getDatabase } from '../../db/database.js';
import { createEmbeddingService } from '../../services/embedding/index.js';
import { log } from '../../utils/log.js';

type CheckResult = {
  name: string;
  status: 'ok' | 'warn' | 'fail';
  message: string;
};

export async function healthCommand(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: { verbose: { type: 'boolean', short: 'v' } },
  });

  log.debug('cli', 'Health check starting');

  const checks: CheckResult[] = [];

  try {
    const db = await getDatabase();
    await db.execute('SELECT 1');
    checks.push({ name: 'Database', status: 'ok', message: 'Connected' });

    const journal = await db.execute('PRAGMA journal_mode');
    const journalMode = journal.rows[0]?.['journal_mode'];
    if (journalMode === 'wal') {
      checks.push({ name: 'WAL Mode', status: 'ok', message: 'Enabled' });
    } else {
      checks.push({
        name: 'WAL Mode',
        status: 'warn',
        message: `Mode: ${String(journalMode)}`,
      });
    }
  } catch (err) {
    checks.push({
      name: 'Database',
      status: 'fail',
      message: err instanceof Error ? err.message : String(err),
    });
  }

  try {
    const response = await fetch('http://localhost:11434/api/tags', {
      signal: AbortSignal.timeout(5000),
    });
    if (response.ok) {
      const data = (await response.json()) as { models?: unknown[] };
      const models = data.models ?? [];
      checks.push({
        name: 'Ollama',
        status: 'ok',
        message: `${models.length} models available`,
      });
    } else {
      checks.push({
        name: 'Ollama',
        status: 'fail',
        message: 'Not responding',
      });
    }
  } catch {
    checks.push({
      name: 'Ollama',
      status: 'warn',
      message: 'Not running (OpenRouter fallback available)',
    });
  }

  try {
    const embedding = await createEmbeddingService();
    const provider = embedding.getProvider();
    checks.push({
      name: 'Embedding',
      status: 'ok',
      message: `Provider: ${provider.name}, Model: ${provider.model}`,
    });
  } catch (err) {
    checks.push({
      name: 'Embedding',
      status: 'fail',
      message: err instanceof Error ? err.message : String(err),
    });
  }

  const icons = { ok: '✓', warn: '⚠', fail: '✗' };
  const colors = { ok: '\x1b[32m', warn: '\x1b[33m', fail: '\x1b[31m' };
  const reset = '\x1b[0m';

  console.log('\nCCMemory Health Check\n');

  for (const check of checks) {
    const icon = icons[check.status];
    const color = colors[check.status];
    console.log(`${color}${icon}${reset} ${check.name}: ${check.message}`);
  }

  const failed = checks.filter(c => c.status === 'fail').length;
  const warned = checks.filter(c => c.status === 'warn').length;

  log.info('cli', 'Health check complete', {
    passed: checks.length - failed - warned,
    warned,
    failed,
  });

  console.log();
  if (failed > 0) {
    console.log(`${colors.fail}${failed} check(s) failed${reset}`);
    process.exit(1);
  } else if (warned > 0) {
    console.log(`${colors.warn}${warned} warning(s)${reset}`);
  } else {
    console.log(`${colors.ok}All checks passed${reset}`);
  }
}
