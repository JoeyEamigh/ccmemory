import { describe, expect, test } from 'bun:test';
import {
  isValidMemoryType,
  MEMORY_TYPE_TO_SECTOR,
  type MemoryType,
} from '../../memory/types.js';
import {
  ACCUMULATOR_LIMITS,
  DEFAULT_EXTRACTION_CONFIG,
  DEFAULT_SIGNAL_DETECTION_CONFIG,
  DEFAULT_SUPERSEDING_CONFIG,
} from '../types.js';

describe('MemoryType validation', () => {
  test('validates preference memory type', () => {
    expect(isValidMemoryType('preference')).toBe(true);
  });

  test('validates codebase memory type', () => {
    expect(isValidMemoryType('codebase')).toBe(true);
  });

  test('validates decision memory type', () => {
    expect(isValidMemoryType('decision')).toBe(true);
  });

  test('validates gotcha memory type', () => {
    expect(isValidMemoryType('gotcha')).toBe(true);
  });

  test('validates pattern memory type', () => {
    expect(isValidMemoryType('pattern')).toBe(true);
  });

  test('rejects invalid memory type', () => {
    expect(isValidMemoryType('invalid')).toBe(false);
  });

  test('rejects undefined', () => {
    expect(isValidMemoryType(undefined)).toBe(false);
  });

  test('rejects null', () => {
    expect(isValidMemoryType(null)).toBe(false);
  });

  test('rejects non-string values', () => {
    expect(isValidMemoryType(123)).toBe(false);
    expect(isValidMemoryType({})).toBe(false);
    expect(isValidMemoryType([])).toBe(false);
  });
});

describe('MEMORY_TYPE_TO_SECTOR mapping', () => {
  test('preference maps to emotional sector', () => {
    expect(MEMORY_TYPE_TO_SECTOR['preference' as MemoryType]).toBe('emotional');
  });

  test('codebase maps to semantic sector', () => {
    expect(MEMORY_TYPE_TO_SECTOR['codebase' as MemoryType]).toBe('semantic');
  });

  test('decision maps to reflective sector', () => {
    expect(MEMORY_TYPE_TO_SECTOR['decision' as MemoryType]).toBe('reflective');
  });

  test('gotcha maps to procedural sector', () => {
    expect(MEMORY_TYPE_TO_SECTOR['gotcha' as MemoryType]).toBe('procedural');
  });

  test('pattern maps to procedural sector', () => {
    expect(MEMORY_TYPE_TO_SECTOR['pattern' as MemoryType]).toBe('procedural');
  });

  test('all memory types have sector mappings', () => {
    const memoryTypes: MemoryType[] = ['preference', 'codebase', 'decision', 'gotcha', 'pattern'];
    for (const type of memoryTypes) {
      expect(MEMORY_TYPE_TO_SECTOR[type]).toBeDefined();
    }
  });
});

describe('Default configuration values', () => {
  describe('DEFAULT_EXTRACTION_CONFIG', () => {
    test('uses Claude Sonnet for extraction', () => {
      expect(DEFAULT_EXTRACTION_CONFIG.model).toContain('sonnet');
    });

    test('has reasonable max tokens', () => {
      expect(DEFAULT_EXTRACTION_CONFIG.maxTokens).toBeGreaterThan(500);
      expect(DEFAULT_EXTRACTION_CONFIG.maxTokens).toBeLessThanOrEqual(100_000);
    });

    test('requires minimum tool calls before extraction', () => {
      expect(DEFAULT_EXTRACTION_CONFIG.minToolCallsToExtract).toBeGreaterThan(0);
    });
  });

  describe('DEFAULT_SIGNAL_DETECTION_CONFIG', () => {
    test('uses Claude Haiku for fast classification', () => {
      expect(DEFAULT_SIGNAL_DETECTION_CONFIG.model).toContain('haiku');
    });

    test('has minimal max tokens for quick responses', () => {
      expect(DEFAULT_SIGNAL_DETECTION_CONFIG.maxTokens).toBeLessThanOrEqual(20_00);
    });
  });

  describe('DEFAULT_SUPERSEDING_CONFIG', () => {
    test('uses Claude Haiku for superseding checks', () => {
      expect(DEFAULT_SUPERSEDING_CONFIG.model).toContain('haiku');
    });

    test('has high similarity threshold for matching', () => {
      expect(DEFAULT_SUPERSEDING_CONFIG.similarityThreshold).toBeGreaterThanOrEqual(0.6);
    });

    test('has high confidence threshold for superseding', () => {
      expect(DEFAULT_SUPERSEDING_CONFIG.confidenceThreshold).toBeGreaterThanOrEqual(0.7);
    });
  });
});

describe('ACCUMULATOR_LIMITS', () => {
  test('limits files tracked to reasonable number', () => {
    expect(ACCUMULATOR_LIMITS.maxFilesTracked).toBe(100);
  });

  test('limits commands tracked', () => {
    expect(ACCUMULATOR_LIMITS.maxCommandsTracked).toBe(50);
  });

  test('limits errors tracked', () => {
    expect(ACCUMULATOR_LIMITS.maxErrorsTracked).toBe(20);
  });

  test('limits searches tracked', () => {
    expect(ACCUMULATOR_LIMITS.maxSearchesTracked).toBe(50);
  });
});
