import { parseArgs } from 'util';
import { getDatabase } from '../../db/database.js';
import { getOrCreateProject } from '../../services/project.js';
import { log } from '../../utils/log.js';

export async function statsCommand(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: { project: { type: 'string', short: 'p' } },
  });

  log.debug('cli', 'Stats command', { project: values.project });

  const db = await getDatabase();

  let projectFilter = '';
  const projectArgs: string[] = [];

  if (values.project) {
    const project = await getOrCreateProject(values.project);
    projectFilter = 'WHERE project_id = ?';
    projectArgs.push(project.id);
  }

  const bySector = await db.execute(
    `SELECT sector, COUNT(*) as count
     FROM memories
     ${projectFilter ? projectFilter + ' AND is_deleted = 0' : 'WHERE is_deleted = 0'}
     GROUP BY sector`,
    projectArgs,
  );

  const byTier = await db.execute(
    `SELECT tier, COUNT(*) as count
     FROM memories
     ${projectFilter ? projectFilter + ' AND is_deleted = 0' : 'WHERE is_deleted = 0'}
     GROUP BY tier`,
    projectArgs,
  );

  const totals = await db.execute(`
    SELECT
      (SELECT COUNT(*) FROM memories WHERE is_deleted = 0) as memories,
      (SELECT COUNT(*) FROM documents) as documents,
      (SELECT COUNT(*) FROM document_chunks) as chunks,
      (SELECT COUNT(*) FROM projects) as projects,
      (SELECT COUNT(*) FROM sessions) as sessions
  `);

  const salience = await db.execute(`
    SELECT
      COUNT(CASE WHEN salience >= 0.8 THEN 1 END) as high,
      COUNT(CASE WHEN salience >= 0.5 AND salience < 0.8 THEN 1 END) as medium,
      COUNT(CASE WHEN salience >= 0.2 AND salience < 0.5 THEN 1 END) as low,
      COUNT(CASE WHEN salience < 0.2 THEN 1 END) as very_low
    FROM memories
    WHERE is_deleted = 0
  `);

  const totalsRow = totals.rows[0];
  const salienceRow = salience.rows[0];

  console.log('\nCCMemory Statistics\n');

  console.log('Totals:');
  console.log(`  Memories: ${totalsRow?.['memories'] ?? 0}`);
  console.log(`  Documents: ${totalsRow?.['documents'] ?? 0}`);
  console.log(`  Document Chunks: ${totalsRow?.['chunks'] ?? 0}`);
  console.log(`  Projects: ${totalsRow?.['projects'] ?? 0}`);
  console.log(`  Sessions: ${totalsRow?.['sessions'] ?? 0}`);

  console.log('\nMemories by Sector:');
  for (const row of bySector.rows) {
    console.log(`  ${row['sector']}: ${row['count']}`);
  }

  console.log('\nMemories by Tier:');
  for (const row of byTier.rows) {
    console.log(`  ${row['tier']}: ${row['count']}`);
  }

  console.log('\nSalience Distribution:');
  console.log(`  High (≥0.8): ${salienceRow?.['high'] ?? 0}`);
  console.log(`  Medium (0.5-0.8): ${salienceRow?.['medium'] ?? 0}`);
  console.log(`  Low (0.2-0.5): ${salienceRow?.['low'] ?? 0}`);
  console.log(`  Very Low (<0.2): ${salienceRow?.['very_low'] ?? 0}`);

  log.info('cli', 'Stats displayed', { project: values.project });
}
