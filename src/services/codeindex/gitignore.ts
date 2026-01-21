import { createHash } from 'crypto';
import { dirname, join, relative, sep } from 'path';
import { DEFAULT_IGNORE_PATTERNS } from './types.js';

type GitignorePattern = {
  pattern: string;
  negated: boolean;
  directoryOnly: boolean;
  regex: RegExp;
  basePath: string;
};

export type GitignoreFilter = {
  isIgnored: (filePath: string, isDirectory: boolean) => boolean;
  hash: string;
};

function escapeRegexSpecialChars(str: string): string {
  return str.replace(/[.+^${}()|[\]\\]/g, '\\$&');
}

function globToRegex(pattern: string): RegExp {
  let regexStr = '';
  let i = 0;

  while (i < pattern.length) {
    const char = pattern[i];

    if (char === '*') {
      if (pattern[i + 1] === '*') {
        if (pattern[i + 2] === '/') {
          regexStr += '(?:.*/)?';
          i += 3;
          continue;
        }
        regexStr += '.*';
        i += 2;
        continue;
      }
      regexStr += '[^/]*';
      i++;
      continue;
    }

    if (char === '?') {
      regexStr += '[^/]';
      i++;
      continue;
    }

    if (char === '[') {
      const closeBracket = pattern.indexOf(']', i + 1);
      if (closeBracket !== -1) {
        const bracketContent = pattern.slice(i, closeBracket + 1);
        regexStr += bracketContent;
        i = closeBracket + 1;
        continue;
      }
    }

    regexStr += escapeRegexSpecialChars(char ?? '');
    i++;
  }

  return new RegExp(`^${regexStr}$`);
}

function parsePattern(line: string, basePath = ''): GitignorePattern | null {
  let pattern = line.trim();

  if (!pattern || pattern.startsWith('#')) {
    return null;
  }

  const negated = pattern.startsWith('!');
  if (negated) {
    pattern = pattern.slice(1);
  }

  pattern = pattern.replace(/\\(.)/g, '$1');

  const directoryOnly = pattern.endsWith('/');
  if (directoryOnly) {
    pattern = pattern.slice(0, -1);
  }

  if (!pattern.includes('/') && !pattern.startsWith('**/')) {
    pattern = '**/' + pattern;
  }

  if (pattern.startsWith('/')) {
    pattern = pattern.slice(1);
  }

  const regex = globToRegex(pattern);

  return { pattern, negated, directoryOnly, regex, basePath };
}

export function parseGitignore(content: string, basePath = ''): GitignorePattern[] {
  const lines = content.split('\n');
  const patterns: GitignorePattern[] = [];

  for (const line of lines) {
    const parsed = parsePattern(line, basePath);
    if (parsed) {
      patterns.push(parsed);
    }
  }

  return patterns;
}

export async function loadGitignorePatterns(projectRoot: string): Promise<GitignoreFilter> {
  const patterns: GitignorePattern[] = [];
  const contentParts: string[] = [];

  for (const defaultPattern of DEFAULT_IGNORE_PATTERNS) {
    const parsed = parsePattern(defaultPattern);
    if (parsed) {
      patterns.push(parsed);
      contentParts.push(defaultPattern);
    }
  }

  const gitignorePath = join(projectRoot, '.gitignore');
  try {
    const file = Bun.file(gitignorePath);
    if (await file.exists()) {
      const content = await file.text();
      contentParts.push(content);
      const parsed = parseGitignore(content);
      patterns.push(...parsed);
    }
  } catch {
    // Ignore errors reading .gitignore
  }

  const hash = createHash('sha256').update(contentParts.join('\n')).digest('hex').slice(0, 16);

  return {
    isIgnored: (filePath: string, isDirectory: boolean): boolean => {
      const relativePath = filePath.startsWith(projectRoot)
        ? relative(projectRoot, filePath)
        : filePath;

      const normalizedPath = relativePath.split(sep).join('/');

      let ignored = false;

      for (const { negated, directoryOnly, regex, basePath } of patterns) {
        if (directoryOnly && !isDirectory) {
          continue;
        }

        let pathToTest = normalizedPath;
        if (basePath) {
          const normalizedBase = basePath.split(sep).join('/');
          if (!normalizedPath.startsWith(normalizedBase + '/') && normalizedPath !== normalizedBase) {
            continue;
          }
          pathToTest = normalizedPath.slice(normalizedBase.length + 1);
        }

        if (regex.test(pathToTest)) {
          ignored = !negated;
        }
      }

      return ignored;
    },
    hash,
  };
}

export type NestedGitignoreFilter = GitignoreFilter & {
  addNestedGitignore: (dirPath: string, content: string) => void;
};

export async function loadGitignorePatternsWithNesting(projectRoot: string): Promise<NestedGitignoreFilter> {
  const patterns: GitignorePattern[] = [];
  const contentParts: string[] = [];

  for (const defaultPattern of DEFAULT_IGNORE_PATTERNS) {
    const parsed = parsePattern(defaultPattern, '');
    if (parsed) {
      patterns.push(parsed);
      contentParts.push(defaultPattern);
    }
  }

  const gitignorePath = join(projectRoot, '.gitignore');
  try {
    const file = Bun.file(gitignorePath);
    if (await file.exists()) {
      const content = await file.text();
      contentParts.push(content);
      const parsed = parseGitignore(content, '');
      patterns.push(...parsed);
    }
  } catch {
    // Ignore errors reading .gitignore
  }

  const baseHash = createHash('sha256').update(contentParts.join('\n')).digest('hex').slice(0, 16);

  return {
    isIgnored: (filePath: string, isDirectory: boolean): boolean => {
      const relativePath = filePath.startsWith(projectRoot)
        ? relative(projectRoot, filePath)
        : filePath;

      const normalizedPath = relativePath.split(sep).join('/');

      let ignored = false;

      for (const { negated, directoryOnly, regex, basePath } of patterns) {
        if (directoryOnly && !isDirectory) {
          continue;
        }

        let pathToTest = normalizedPath;
        if (basePath) {
          const normalizedBase = basePath.split(sep).join('/');
          if (!normalizedPath.startsWith(normalizedBase + '/') && normalizedPath !== normalizedBase) {
            continue;
          }
          pathToTest = normalizedPath.slice(normalizedBase.length + 1);
        }

        if (regex.test(pathToTest)) {
          ignored = !negated;
        }
      }

      return ignored;
    },
    hash: baseHash,
    addNestedGitignore: (dirPath: string, content: string): void => {
      const relativeDir = relative(projectRoot, dirPath);
      const parsed = parseGitignore(content, relativeDir);
      patterns.push(...parsed);
    },
  };
}

export function shouldIgnoreFile(fileName: string): boolean {
  const lowerName = fileName.toLowerCase();

  if (lowerName.startsWith('.')) {
    const allowedDotFiles = ['.gitignore', '.env.example', '.env.local'];
    if (!allowedDotFiles.includes(lowerName)) {
      return true;
    }
  }

  const ignoredExtensions = [
    '.exe', '.dll', '.so', '.dylib', '.bin',
    '.png', '.jpg', '.jpeg', '.gif', '.bmp', '.ico', '.svg', '.webp',
    '.mp3', '.mp4', '.avi', '.mov', '.mkv', '.wav',
    '.zip', '.tar', '.gz', '.rar', '.7z',
    '.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx',
    '.ttf', '.otf', '.woff', '.woff2', '.eot',
    '.db', '.sqlite', '.sqlite3',
  ];

  for (const ext of ignoredExtensions) {
    if (lowerName.endsWith(ext)) {
      return true;
    }
  }

  return false;
}
