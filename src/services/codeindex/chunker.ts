import type { ChunkType, CodeChunk, CodeLanguage } from './types.js';

const CHARS_PER_TOKEN = 4;
const TARGET_LINES = 50;
const MAX_LINES = 100;
const MIN_LINES = 5;

type BoundaryPattern = {
  regex: RegExp;
  type: ChunkType;
  extractSymbol?: (match: RegExpMatchArray) => string | null;
};

const JS_TS_PATTERNS: BoundaryPattern[] = [
  {
    regex: /^(?:export\s+)?(?:async\s+)?function\s+(\w+)/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:export\s+)?(?:default\s+)?class\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:export\s+)?const\s+(\w+)\s*=\s*(?:async\s+)?\(/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:export\s+)?const\s+(\w+)\s*=\s*(?:async\s+)?function/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:export\s+)?(?:type|interface)\s+(\w+)/,
    type: 'block',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^import\s+/,
    type: 'imports',
  },
];

const PYTHON_PATTERNS: BoundaryPattern[] = [
  {
    regex: /^(?:async\s+)?def\s+(\w+)/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^class\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:from\s+\S+\s+)?import\s+/,
    type: 'imports',
  },
];

const GO_PATTERNS: BoundaryPattern[] = [
  {
    regex: /^func\s+(?:\([^)]+\)\s+)?(\w+)/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^type\s+(\w+)\s+(?:struct|interface)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^import\s+/,
    type: 'imports',
  },
];

const RUST_PATTERNS: BoundaryPattern[] = [
  {
    regex: /^(?:pub\s+)?(?:async\s+)?fn\s+(\w+)/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:pub\s+)?struct\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:pub\s+)?enum\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:pub\s+)?trait\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:pub\s+)?impl(?:<[^>]+>)?\s+(?:(\w+)|for\s+(\w+))/,
    type: 'class',
    extractSymbol: m => m[1] ?? m[2] ?? null,
  },
  {
    regex: /^use\s+/,
    type: 'imports',
  },
];

const JAVA_PATTERNS: BoundaryPattern[] = [
  {
    regex: /^(?:public|private|protected)?\s*(?:static\s+)?(?:\w+\s+)?(\w+)\s*\([^)]*\)\s*(?:throws\s+\w+\s*)?{/,
    type: 'function',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:public|private|protected)?\s*(?:abstract\s+)?class\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^(?:public|private|protected)?\s*interface\s+(\w+)/,
    type: 'class',
    extractSymbol: m => m[1] ?? null,
  },
  {
    regex: /^import\s+/,
    type: 'imports',
  },
];

const LANGUAGE_PATTERNS: Partial<Record<CodeLanguage, BoundaryPattern[]>> = {
  ts: JS_TS_PATTERNS,
  tsx: JS_TS_PATTERNS,
  js: JS_TS_PATTERNS,
  jsx: JS_TS_PATTERNS,
  py: PYTHON_PATTERNS,
  go: GO_PATTERNS,
  rs: RUST_PATTERNS,
  java: JAVA_PATTERNS,
  kt: JAVA_PATTERNS,
  scala: JAVA_PATTERNS,
};

type BoundaryInfo = {
  lineIndex: number;
  type: ChunkType;
  symbol: string | null;
};

function detectBoundaries(lines: string[], language: CodeLanguage): BoundaryInfo[] {
  const patterns = LANGUAGE_PATTERNS[language];
  if (!patterns) return [];

  const boundaries: BoundaryInfo[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]?.trim() ?? '';
    if (!line) continue;

    for (const pattern of patterns) {
      const match = line.match(pattern.regex);
      if (match) {
        boundaries.push({
          lineIndex: i,
          type: pattern.type,
          symbol: pattern.extractSymbol?.(match) ?? null,
        });
        break;
      }
    }
  }

  return boundaries;
}

function findBestBreakPoint(lines: string[], start: number, targetEnd: number, maxEnd: number): number {
  const safeMaxEnd = Math.min(maxEnd, lines.length);
  const safeTargetEnd = Math.min(targetEnd, lines.length);

  for (let i = safeTargetEnd; i < safeMaxEnd; i++) {
    const line = lines[i]?.trim() ?? '';
    if (line === '' || line === '}' || line === '};' || line === 'end') {
      return i + 1;
    }
  }

  for (let i = safeTargetEnd - 1; i > start + MIN_LINES; i--) {
    const line = lines[i]?.trim() ?? '';
    if (line === '' || line === '}' || line === '};' || line === 'end') {
      return i + 1;
    }
  }

  return safeTargetEnd;
}

function extractSymbols(lines: string[], startLine: number, endLine: number, language: CodeLanguage): string[] {
  const symbols: string[] = [];
  const patterns = LANGUAGE_PATTERNS[language];
  if (!patterns) return symbols;

  for (let i = startLine; i < endLine && i < lines.length; i++) {
    const line = lines[i]?.trim() ?? '';
    if (!line) continue;

    for (const pattern of patterns) {
      if (pattern.extractSymbol) {
        const match = line.match(pattern.regex);
        if (match) {
          const symbol = pattern.extractSymbol(match);
          if (symbol) {
            symbols.push(symbol);
          }
        }
      }
    }
  }

  return [...new Set(symbols)];
}

function determineChunkType(lines: string[], startLine: number, endLine: number, language: CodeLanguage): ChunkType {
  const patterns = LANGUAGE_PATTERNS[language];
  if (!patterns) return 'block';

  const firstLines = lines.slice(startLine, Math.min(startLine + 5, endLine));

  let hasImports = false;
  let hasClass = false;
  let hasFunction = false;

  for (const line of firstLines) {
    const trimmed = line?.trim() ?? '';
    if (!trimmed) continue;

    for (const pattern of patterns) {
      if (pattern.regex.test(trimmed)) {
        if (pattern.type === 'imports') hasImports = true;
        else if (pattern.type === 'class') hasClass = true;
        else if (pattern.type === 'function') hasFunction = true;
      }
    }
  }

  if (hasClass) return 'class';
  if (hasFunction) return 'function';
  if (hasImports) return 'imports';
  return 'block';
}

export function chunkCode(content: string, language: CodeLanguage): CodeChunk[] {
  const lines = content.split('\n');
  const chunks: CodeChunk[] = [];

  if (lines.length <= MAX_LINES) {
    const chunkContent = content;
    return [
      {
        content: chunkContent,
        startLine: 1,
        endLine: lines.length,
        chunkType: determineChunkType(lines, 0, lines.length, language),
        symbols: extractSymbols(lines, 0, lines.length, language),
        tokensEstimate: Math.ceil(chunkContent.length / CHARS_PER_TOKEN),
      },
    ];
  }

  const boundaries = detectBoundaries(lines, language);

  let currentStart = 0;

  while (currentStart < lines.length) {
    let chunkEnd: number;

    const relevantBoundaries = boundaries.filter(b => b.lineIndex > currentStart && b.lineIndex <= currentStart + MAX_LINES);

    if (relevantBoundaries.length > 0) {
      const targetBoundary = relevantBoundaries.find(b => b.lineIndex >= currentStart + TARGET_LINES);

      if (targetBoundary) {
        chunkEnd = targetBoundary.lineIndex;
      } else {
        const lastBoundary = relevantBoundaries[relevantBoundaries.length - 1];
        if (lastBoundary && lastBoundary.lineIndex >= currentStart + MIN_LINES) {
          chunkEnd = lastBoundary.lineIndex;
        } else {
          chunkEnd = findBestBreakPoint(lines, currentStart, currentStart + TARGET_LINES, currentStart + MAX_LINES);
        }
      }
    } else {
      chunkEnd = findBestBreakPoint(lines, currentStart, currentStart + TARGET_LINES, currentStart + MAX_LINES);
    }

    if (chunkEnd <= currentStart) {
      chunkEnd = Math.min(currentStart + TARGET_LINES, lines.length);
    }

    const chunkLines = lines.slice(currentStart, chunkEnd);
    const chunkContent = chunkLines.join('\n');

    if (chunkContent.trim()) {
      chunks.push({
        content: chunkContent,
        startLine: currentStart + 1,
        endLine: chunkEnd,
        chunkType: determineChunkType(lines, currentStart, chunkEnd, language),
        symbols: extractSymbols(lines, currentStart, chunkEnd, language),
        tokensEstimate: Math.ceil(chunkContent.length / CHARS_PER_TOKEN),
      });
    }

    currentStart = chunkEnd;
  }

  return chunks;
}

export function estimateCodeTokens(content: string): number {
  return Math.ceil(content.length / CHARS_PER_TOKEN);
}
