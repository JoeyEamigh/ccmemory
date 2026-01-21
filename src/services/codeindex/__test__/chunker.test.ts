import { describe, expect, test } from 'bun:test';
import { chunkCode, estimateCodeTokens } from '../chunker.js';

describe('chunker', () => {
  describe('chunkCode', () => {
    test('returns single chunk for small files', () => {
      const code = 'const x = 1;\nconst y = 2;';
      const chunks = chunkCode(code, 'ts');
      expect(chunks).toHaveLength(1);
      expect(chunks[0]?.startLine).toBe(1);
      expect(chunks[0]?.endLine).toBe(2);
    });

    test('detects function boundaries in TypeScript', () => {
      const lines: string[] = [];
      for (let i = 0; i < 30; i++) {
        lines.push(`function fn${i}() {\n  return ${i};\n}`);
      }
      const code = lines.join('\n\n');
      const chunks = chunkCode(code, 'ts');
      expect(chunks.some(c => c.chunkType === 'function')).toBe(true);
    });

    test('extracts symbols from TypeScript chunks', () => {
      const code = `
export function myFunction() {}
export class MyClass {}
const arrowFn = () => {};
`;
      const chunks = chunkCode(code, 'ts');
      expect(chunks[0]?.symbols).toContain('myFunction');
      expect(chunks[0]?.symbols).toContain('MyClass');
    });

    test('handles Python syntax', () => {
      const code = `
def foo():
    pass

class Bar:
    pass

async def baz():
    pass
`;
      const chunks = chunkCode(code, 'py');
      expect(chunks[0]?.symbols).toContain('foo');
      expect(chunks[0]?.symbols).toContain('Bar');
      expect(chunks[0]?.symbols).toContain('baz');
    });

    test('respects max line limit', () => {
      const longCode = Array(200).fill('const x = 1;').join('\n');
      const chunks = chunkCode(longCode, 'ts');
      for (const chunk of chunks) {
        const lines = chunk.endLine - chunk.startLine + 1;
        expect(lines).toBeLessThanOrEqual(100);
      }
    });

    test('handles Go syntax', () => {
      const code = `
package main

func main() {
    fmt.Println("hello")
}

type Config struct {
    Name string
}

func (c *Config) String() string {
    return c.Name
}
`;
      const chunks = chunkCode(code, 'go');
      expect(chunks[0]?.symbols).toContain('main');
      expect(chunks[0]?.symbols).toContain('Config');
      expect(chunks[0]?.symbols).toContain('String');
    });

    test('handles Rust syntax', () => {
      const code = `
pub fn process(data: &str) -> Result<(), Error> {
    Ok(())
}

struct Config {
    name: String,
}

pub enum Status {
    Active,
    Inactive,
}

trait Handler {
    fn handle(&self);
}

impl Handler for Config {
    fn handle(&self) {}
}
`;
      const chunks = chunkCode(code, 'rs');
      expect(chunks[0]?.symbols).toContain('process');
      expect(chunks[0]?.symbols).toContain('Config');
      expect(chunks[0]?.symbols).toContain('Status');
      expect(chunks[0]?.symbols).toContain('Handler');
    });

    test('handles Java syntax', () => {
      const code = `
public class MyClass {
    public void myMethod() {
        System.out.println("Hello");
    }

    private String helper() {
        return "help";
    }
}

interface MyInterface {
    void doSomething();
}
`;
      const chunks = chunkCode(code, 'java');
      expect(chunks[0]?.symbols).toContain('MyClass');
      expect(chunks[0]?.symbols).toContain('MyInterface');
    });

    test('identifies imports chunk type', () => {
      const code = `
import { foo } from './foo';
import * as bar from 'bar';
import type { Baz } from './baz';

const x = 1;
`;
      const chunks = chunkCode(code, 'ts');
      expect(chunks[0]?.chunkType).toBe('imports');
    });

    test('handles files with no recognized patterns', () => {
      const code = '# Just a comment\nsome text\nmore text';
      const chunks = chunkCode(code, 'md');
      expect(chunks).toHaveLength(1);
      expect(chunks[0]?.chunkType).toBe('block');
      expect(chunks[0]?.symbols).toEqual([]);
    });

    test('handles empty content', () => {
      const chunks = chunkCode('', 'ts');
      expect(chunks).toHaveLength(1);
      expect(chunks[0]?.content).toBe('');
    });

    test('calculates token estimate', () => {
      const code = 'const x = 1;';
      const chunks = chunkCode(code, 'ts');
      expect(chunks[0]?.tokensEstimate).toBe(Math.ceil(code.length / 4));
    });

    test('breaks at function boundaries for long files', () => {
      const functions: string[] = [];
      for (let i = 0; i < 20; i++) {
        const body = Array(10).fill(`  const v${i} = ${i};`).join('\n');
        functions.push(`function process${i}() {\n${body}\n}`);
      }
      const code = functions.join('\n\n');
      const chunks = chunkCode(code, 'ts');

      expect(chunks.length).toBeGreaterThan(1);

      for (const chunk of chunks) {
        expect(chunk.startLine).toBeGreaterThan(0);
        expect(chunk.endLine).toBeGreaterThanOrEqual(chunk.startLine);
      }
    });

    test('handles class chunk type', () => {
      const code = `
class MyClass {
  constructor() {}
  method() {}
}
`.repeat(20);
      const chunks = chunkCode(code, 'ts');
      expect(chunks.some(c => c.chunkType === 'class')).toBe(true);
    });

    test('handles arrow functions as function type', () => {
      const code = `
export const handler = async (req, res) => {
  const data = await fetch('/api');
  return data;
};

export const process = (input) => {
  return input.map(x => x * 2);
};
`;
      const chunks = chunkCode(code, 'ts');
      expect(chunks[0]?.symbols).toContain('handler');
      expect(chunks[0]?.symbols).toContain('process');
    });

    test('handles type/interface declarations', () => {
      const code = `
export type Config = {
  name: string;
  value: number;
};

export interface IHandler {
  handle(): void;
}
`;
      const chunks = chunkCode(code, 'ts');
      expect(chunks[0]?.symbols).toContain('Config');
      expect(chunks[0]?.symbols).toContain('IHandler');
    });

    test('deduplicates symbols', () => {
      const code = `
function foo() {}
function foo() {} // duplicate
function foo() {}
`;
      const chunks = chunkCode(code, 'ts');
      const fooCount = chunks[0]?.symbols.filter(s => s === 'foo').length ?? 0;
      expect(fooCount).toBe(1);
    });

    test('line numbers are 1-indexed', () => {
      const code = 'line1\nline2\nline3';
      const chunks = chunkCode(code, 'ts');
      expect(chunks[0]?.startLine).toBe(1);
      expect(chunks[0]?.endLine).toBe(3);
    });

    test('prefers breaking at natural boundaries over arbitrary line counts', () => {
      const code = `
function firstFunction() {
  const a = 1;
  const b = 2;
  return a + b;
}

function secondFunction() {
  const c = 3;
  const d = 4;
  return c + d;
}
`.repeat(10);
      const chunks = chunkCode(code, 'ts');

      for (const chunk of chunks) {
        const content = chunk.content.trim();
        const endsWithCloseBrace = content.endsWith('}');
        const endsWithBlankLine = content.endsWith('\n') || content.endsWith('\n\n');
        expect(endsWithCloseBrace || endsWithBlankLine || chunk === chunks[chunks.length - 1]).toBe(true);
      }
    });

    test('chunks cover entire file without gaps or overlaps', () => {
      const lines: string[] = [];
      for (let i = 0; i < 150; i++) {
        lines.push(`const line${i} = ${i};`);
      }
      const code = lines.join('\n');
      const chunks = chunkCode(code, 'ts');
      const totalLines = code.split('\n').length;

      let currentLine = 1;
      for (const chunk of chunks) {
        expect(chunk.startLine).toBe(currentLine);
        expect(chunk.endLine).toBeGreaterThanOrEqual(chunk.startLine);
        currentLine = chunk.endLine + 1;
      }

      expect(currentLine - 1).toBe(totalLines);
    });

    test('provides useful context for semantic search', () => {
      const code = `
export function calculateTax(income: number, rate: number): number {
  const taxableAmount = income - 10000;
  if (taxableAmount <= 0) return 0;
  return taxableAmount * rate;
}
`;
      const chunks = chunkCode(code, 'ts');

      expect(chunks.length).toBe(1);
      expect(chunks[0]?.symbols).toContain('calculateTax');
      expect(chunks[0]?.content).toContain('calculateTax');
      expect(chunks[0]?.content).toContain('income');
      expect(chunks[0]?.content).toContain('taxableAmount');
    });

    test('captures complete function bodies even when large', () => {
      const bodyLines = Array(80).fill('  const x = 1;').join('\n');
      const code = `
function largeFunction() {
${bodyLines}
  return x;
}
`;
      const chunks = chunkCode(code, 'ts');

      const functionChunk = chunks.find(c => c.symbols.includes('largeFunction'));
      expect(functionChunk).toBeDefined();
      expect(functionChunk?.content).toContain('function largeFunction');
    });
  });

  describe('estimateCodeTokens', () => {
    test('estimates approximately 4 chars per token', () => {
      expect(estimateCodeTokens('word')).toBe(1);
    });

    test('handles longer text', () => {
      expect(estimateCodeTokens('a'.repeat(100))).toBe(25);
    });

    test('handles empty string', () => {
      expect(estimateCodeTokens('')).toBe(0);
    });

    test('rounds up partial tokens', () => {
      expect(estimateCodeTokens('abc')).toBe(1);
      expect(estimateCodeTokens('abcde')).toBe(2);
    });
  });
});
