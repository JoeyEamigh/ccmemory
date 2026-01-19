import { getDatabase } from '../../db/database.js';
import type { Memory } from './types.js';
import { rowToMemory } from './utils.js';

function fnv1a64(str: string): bigint {
  let hash = 14695981039346656037n;
  const mask = (1n << 64n) - 1n;
  for (let i = 0; i < str.length; i++) {
    hash ^= BigInt(str.charCodeAt(i));
    hash = (hash * 1099511628211n) & mask;
  }
  return hash;
}

export function computeSimhash(text: string): string {
  const tokens = text
    .toLowerCase()
    .replace(/[^\w\s]/g, '')
    .split(/\s+/)
    .filter(t => t.length > 2);

  if (tokens.length === 0) {
    return '0000000000000000';
  }

  const vector: number[] = [];
  for (let i = 0; i < 64; i++) {
    vector.push(0);
  }

  for (const token of tokens) {
    const hash = fnv1a64(token);
    for (let i = 0; i < 64; i++) {
      const currentValue = vector[i] ?? 0;
      if ((hash >> BigInt(i)) & 1n) {
        vector[i] = currentValue + 1;
      } else {
        vector[i] = currentValue - 1;
      }
    }
  }

  let result = 0n;
  for (let i = 0; i < 64; i++) {
    const value = vector[i] ?? 0;
    if (value > 0) {
      result |= 1n << BigInt(i);
    }
  }

  return result.toString(16).padStart(16, '0');
}

export function hammingDistance(hash1: string, hash2: string): number {
  const h1 = BigInt('0x' + hash1);
  const h2 = BigInt('0x' + hash2);
  const xor = h1 ^ h2;

  let count = 0;
  let n = xor;
  while (n > 0n) {
    count += Number(n & 1n);
    n >>= 1n;
  }
  return count;
}

export function isDuplicate(hash1: string, hash2: string, threshold = 3): boolean {
  return hammingDistance(hash1, hash2) <= threshold;
}

export async function findSimilarMemory(simhash: string, projectId: string, threshold = 3): Promise<Memory | null> {
  const db = await getDatabase();

  const result = await db.execute(
    `SELECT * FROM memories
     WHERE project_id = ?
       AND is_deleted = 0
       AND simhash IS NOT NULL
     ORDER BY created_at DESC`,
    [projectId],
  );

  for (const row of result.rows) {
    const rowSimhash = row['simhash'];
    if (typeof rowSimhash === 'string' && isDuplicate(simhash, rowSimhash, threshold)) {
      return rowToMemory(row);
    }
  }

  return null;
}

export async function computeMD5(text: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(text);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}
