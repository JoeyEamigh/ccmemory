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

  const codeIndexStats = await db.execute(`
    SELECT
      (SELECT COUNT(*) FROM indexed_files) as indexed_files,
      (SELECT COUNT(*) FROM documents WHERE is_code = 1) as code_documents,
      (SELECT COUNT(*) FROM document_chunks dc
       JOIN documents d ON dc.document_id = d.id
       WHERE d.is_code = 1) as code_chunks
  `);

  const codeIndexRow = codeIndexStats.rows[0];
  const indexedFilesCount = Number(codeIndexRow?.['indexed_files'] ?? 0);
  const codeDocsCount = Number(codeIndexRow?.['code_documents'] ?? 0);
  const codeChunksCount = Number(codeIndexRow?.['code_chunks'] ?? 0);

  if (indexedFilesCount > 0 || codeDocsCount > 0) {
    console.log('\nCode Index:');
    console.log(`  Indexed Files: ${indexedFilesCount}`);
    console.log(`  Code Documents: ${codeDocsCount}`);
    console.log(`  Code Chunks: ${codeChunksCount}`);

    const languageStats = await db.execute(`
      SELECT language, COUNT(*) as count
      FROM documents
      WHERE is_code = 1 AND language IS NOT NULL
      GROUP BY language
      ORDER BY count DESC
      LIMIT 10
    `);

    if (languageStats.rows.length > 0) {
      console.log('\n  By Language:');
      for (const row of languageStats.rows) {
        console.log(`    ${row['language']}: ${row['count']}`);
      }
    }

    const projectIndexStates = await db.execute(`
      SELECT
        p.name as project_name,
        p.path as project_path,
        cis.indexed_files,
        cis.last_indexed_at
      FROM code_index_state cis
      JOIN projects p ON cis.project_id = p.id
      ORDER BY cis.last_indexed_at DESC
      LIMIT 5
    `);

    if (projectIndexStates.rows.length > 0) {
      console.log('\n  Recent Index Activity:');
      for (const row of projectIndexStates.rows) {
        const lastIndexed = new Date(Number(row['last_indexed_at'])).toLocaleString();
        const projectName = row['project_name'] ?? row['project_path'];
        console.log(`    ${projectName}: ${row['indexed_files']} files (${lastIndexed})`);
      }
    }
  }

  log.info('cli', 'Stats displayed', { project: values.project });
}
