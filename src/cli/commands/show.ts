import { parseArgs } from 'util';
import { createEmbeddingService } from '../../services/embedding/index.js';
import { getRelatedMemories } from '../../services/memory/relationships.js';
import { createMemoryStore } from '../../services/memory/store.js';
import { createSearchService } from '../../services/search/hybrid.js';
import { log } from '../../utils/log.js';

export async function showCommand(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      related: { type: 'boolean', short: 'r' },
      json: { type: 'boolean' },
    },
    allowPositionals: true,
  });

  const id = positionals[0];
  if (!id) {
    console.error('Usage: ccmemory show <id> [--related]');
    process.exit(1);
  }

  log.debug('cli', 'Show command', { id, related: values.related });

  const store = createMemoryStore();
  const memory = await store.get(id);

  if (!memory) {
    log.warn('cli', 'Memory not found', { id });
    console.error(`Memory not found: ${id}`);
    process.exit(1);
  }

  log.debug('cli', 'Memory retrieved', { id, sector: memory.sector });

  if (values.json) {
    console.log(JSON.stringify(memory, null, 2));
  } else {
    console.log(`ID: ${memory.id}`);
    console.log(`Sector: ${memory.sector}`);
    console.log(`Tier: ${memory.tier}`);
    console.log(`Salience: ${memory.salience.toFixed(3)}`);
    console.log(`Access Count: ${memory.accessCount}`);
    console.log(`Created: ${new Date(memory.createdAt).toLocaleString()}`);
    console.log(`Last Accessed: ${new Date(memory.lastAccessed).toLocaleString()}`);
    console.log(`\nContent:\n${memory.content}`);

    if (memory.tags.length > 0) {
      console.log(`\nTags: ${memory.tags.join(', ')}`);
    }
    if (memory.concepts.length > 0) {
      console.log(`Concepts: ${memory.concepts.join(', ')}`);
    }
    if (memory.files.length > 0) {
      console.log(`Files: ${memory.files.join(', ')}`);
    }
  }

  if (values.related) {
    console.log(`\n${'─'.repeat(40)}\nRelated memories:\n`);

    const embeddingService = await createEmbeddingService();
    const search = createSearchService(embeddingService);
    const timeline = await search.timeline(id, 3, 3);

    const allMemories = [...timeline.before, timeline.anchor, ...timeline.after];
    for (const mem of allMemories) {
      const marker = mem.id === id ? '>>>' : '   ';
      console.log(`${marker} [${new Date(mem.createdAt).toISOString().slice(0, 16)}] ${mem.sector}`);
      console.log(`    ${mem.content.slice(0, 100)}...`);
    }

    const related = await getRelatedMemories(id);
    if (related.length > 0) {
      console.log(`\nDirectly related memories:`);
      for (const rel of related) {
        console.log(`  ${rel.id}: ${rel.content.slice(0, 50)}...`);
      }
    }
  }
}
