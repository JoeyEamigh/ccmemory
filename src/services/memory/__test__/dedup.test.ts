import { describe, expect, test } from 'bun:test';
import { computeMD5, computeSimhash, hammingDistance, isDuplicate } from '../dedup.js';

describe('Simhash Computation', () => {
  test('identical text produces identical hash', () => {
    const text = 'The quick brown fox jumps over the lazy dog';
    expect(computeSimhash(text)).toBe(computeSimhash(text));
  });

  test('similar text produces similar hash', () => {
    const text1 = 'The quick brown fox jumps over the lazy dog';
    const text2 = 'The quick brown fox leaps over the lazy dog';
    const distance = hammingDistance(computeSimhash(text1), computeSimhash(text2));
    expect(distance).toBeLessThan(10);
  });

  test('different text produces different hash', () => {
    const text1 = 'The quick brown fox';
    const text2 = 'A completely different sentence about programming';
    const distance = hammingDistance(computeSimhash(text1), computeSimhash(text2));
    expect(distance).toBeGreaterThan(15);
  });

  test('returns zero hash for empty content', () => {
    expect(computeSimhash('')).toBe('0000000000000000');
  });

  test('returns zero hash for only short words', () => {
    expect(computeSimhash('a an to')).toBe('0000000000000000');
  });

  test('hash is always 16 hex characters', () => {
    const texts = [
      'short',
      'This is a medium length text',
      'This is a much longer text that contains many words and should produce a valid hash',
    ];
    for (const text of texts) {
      const hash = computeSimhash(text);
      expect(hash).toHaveLength(16);
      expect(/^[0-9a-f]{16}$/.test(hash)).toBe(true);
    }
  });

  test('case insensitive', () => {
    const text1 = 'THE QUICK BROWN FOX';
    const text2 = 'the quick brown fox';
    expect(computeSimhash(text1)).toBe(computeSimhash(text2));
  });

  test('ignores punctuation', () => {
    const text1 = 'Hello, world! How are you?';
    const text2 = 'Hello world How are you';
    expect(computeSimhash(text1)).toBe(computeSimhash(text2));
  });
});

describe('Hamming Distance', () => {
  test('identical hashes have zero distance', () => {
    const hash = '0123456789abcdef';
    expect(hammingDistance(hash, hash)).toBe(0);
  });

  test('calculates correct distance for one bit difference', () => {
    const hash1 = '0000000000000000';
    const hash2 = '0000000000000001';
    expect(hammingDistance(hash1, hash2)).toBe(1);
  });

  test('calculates correct distance for multiple bits', () => {
    const hash1 = '0000000000000000';
    const hash2 = '0000000000000007';
    expect(hammingDistance(hash1, hash2)).toBe(3);
  });

  test('maximum distance is 64', () => {
    const hash1 = '0000000000000000';
    const hash2 = 'ffffffffffffffff';
    expect(hammingDistance(hash1, hash2)).toBe(64);
  });

  test('symmetric distance', () => {
    const hash1 = '1234567890abcdef';
    const hash2 = 'fedcba0987654321';
    expect(hammingDistance(hash1, hash2)).toBe(hammingDistance(hash2, hash1));
  });
});

describe('isDuplicate', () => {
  test('identifies duplicates with default threshold', () => {
    const hash1 = '0000000000000000';
    const hash2 = '0000000000000007';
    expect(isDuplicate(hash1, hash2)).toBe(true);
  });

  test('rejects non-duplicates with default threshold', () => {
    const hash1 = '0000000000000000';
    const hash2 = '000000000000000f';
    expect(isDuplicate(hash1, hash2)).toBe(false);
  });

  test('respects custom threshold', () => {
    const hash1 = '0000000000000000';
    const hash2 = '000000000000001f';
    expect(isDuplicate(hash1, hash2, 5)).toBe(true);
    expect(isDuplicate(hash1, hash2, 4)).toBe(false);
  });

  test('identical hashes are always duplicates', () => {
    const hash = '1234567890abcdef';
    expect(isDuplicate(hash, hash)).toBe(true);
    expect(isDuplicate(hash, hash, 0)).toBe(true);
  });
});

describe('computeMD5', () => {
  test('produces consistent hash', async () => {
    const text = 'Hello, world!';
    const hash1 = await computeMD5(text);
    const hash2 = await computeMD5(text);
    expect(hash1).toBe(hash2);
  });

  test('different text produces different hash', async () => {
    const hash1 = await computeMD5('Hello');
    const hash2 = await computeMD5('World');
    expect(hash1).not.toBe(hash2);
  });

  test('returns 64 character hex string (SHA-256)', async () => {
    const hash = await computeMD5('test');
    expect(hash).toHaveLength(64);
    expect(/^[0-9a-f]{64}$/.test(hash)).toBe(true);
  });

  test('handles empty string', async () => {
    const hash = await computeMD5('');
    expect(hash).toHaveLength(64);
  });

  test('handles unicode', async () => {
    const hash = await computeMD5('你好世界');
    expect(hash).toHaveLength(64);
  });
});

describe('Practical Simhash Scenarios', () => {
  test('memory content variations are detected as similar', () => {
    const original = 'The auth module is located in src/auth/index.ts file';
    const variation1 = 'The auth module is located at src/auth/index.ts file';

    const h0 = computeSimhash(original);
    const h1 = computeSimhash(variation1);

    expect(hammingDistance(h0, h1)).toBeLessThan(20);
  });

  test('completely different memories have high distance', () => {
    const memory1 = 'The user prefers dark mode for the IDE interface';
    const memory2 = 'Deploy to production using the CI pipeline system';

    const h1 = computeSimhash(memory1);
    const h2 = computeSimhash(memory2);

    expect(hammingDistance(h1, h2)).toBeGreaterThan(15);
  });

  test('exact duplicate has zero distance', () => {
    const memory = 'The database schema is defined in src/db/schema.ts';
    const h1 = computeSimhash(memory);
    const h2 = computeSimhash(memory);

    expect(hammingDistance(h1, h2)).toBe(0);
    expect(isDuplicate(h1, h2)).toBe(true);
  });
});
