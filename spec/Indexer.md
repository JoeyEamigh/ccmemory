# Semantic Code Search for CCMemory

## Overview

Add project-level file indexing and semantic code search to CCMemory. The agent gets a `code_search` tool to find relevant code by meaning, not just keywords.

## Implementation Status

**Status: COMPLETE** (v0.2.3)

All planned components have been implemented and tested:
- All 7 service files created in `src/services/codeindex/`
- Database migration v7 added
- 2 MCP tools added (`code_search`, `code_index`)
- 5 CLI commands added (`watch`, `code-index`, `code-search`, `code-index-export`, `code-index-import`)
- Build passes, all 641 tests pass
- All P2 features implemented: auto-reindex on .gitignore changes, parallel file processing, nested .gitignore support, auto-start watcher on session start, export/import for sharing
- Bug fix: Watcher now correctly detects file deletions and emits `delete` events
- Optimization: Targeted file deletion uses event path directly instead of scanning all indexed files
- Added integration tests for `processFileChanges` covering delete, add, and change events

## Architecture Decision: Standalone File Watcher Daemon

**Rationale**: Multiple Claude sessions may run against the same project simultaneously. A standalone daemon avoids conflicts and provides centralized indexing.

**Design:**
- `ccmemory watch [project-dir]` - Standalone daemon watching a project directory
- Single instance per project (lock file coordination, like existing WebUI server)
- Uses Bun's native file watching (recursive directory watch)
- Respects `.gitignore` via parsed patterns
- Indexes incrementally on file changes (debounced)
- Graceful shutdown via `ccmemory watch --stop`

## Components

### 1. Service: `src/services/codeindex/`

```
src/services/codeindex/
  types.ts        # Type definitions (CodeLanguage, ChunkType, SearchResult, etc.)
  gitignore.ts    # .gitignore parsing with glob-to-regex conversion
  scanner.ts      # Directory scanning with gitignore filtering
  chunker.ts      # Code-aware chunking with boundary detection
  coordination.ts # Lock file + PID tracking for single instance per project
  watcher.ts      # File watcher daemon with 500ms debouncing
  index.ts        # CodeIndexService - main orchestrator
```

### 2. Database Migration (v7)

```sql
-- Extend documents table
ALTER TABLE documents ADD COLUMN language TEXT;
ALTER TABLE documents ADD COLUMN line_count INTEGER;
ALTER TABLE documents ADD COLUMN is_code INTEGER DEFAULT 0;

-- Extend document_chunks table
ALTER TABLE document_chunks ADD COLUMN start_line INTEGER;
ALTER TABLE document_chunks ADD COLUMN end_line INTEGER;
ALTER TABLE document_chunks ADD COLUMN chunk_type TEXT;  -- 'function' | 'class' | 'imports' | 'block'
ALTER TABLE document_chunks ADD COLUMN symbols_json TEXT; -- function/class names

-- Index state tracking
CREATE TABLE IF NOT EXISTS code_index_state (
  project_id TEXT PRIMARY KEY,
  last_indexed_at INTEGER,
  indexed_files INTEGER DEFAULT 0,
  gitignore_hash TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

-- File-level tracking for incremental updates
CREATE TABLE IF NOT EXISTS indexed_files (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  path TEXT NOT NULL,
  checksum TEXT NOT NULL,
  mtime INTEGER NOT NULL,
  indexed_at INTEGER NOT NULL,
  UNIQUE(project_id, path)
);

-- Indexes
CREATE INDEX idx_indexed_files_project ON indexed_files(project_id);
CREATE INDEX idx_indexed_files_path ON indexed_files(project_id, path);
CREATE INDEX idx_documents_code ON documents(project_id, is_code) WHERE is_code = 1;
CREATE INDEX idx_documents_language ON documents(language) WHERE language IS NOT NULL;
```

### 3. MCP Tools

**`code_search`** - Semantic code search
```typescript
{
  name: 'code_search',
  description: 'Search indexed code by semantic similarity. Returns snippets with file paths and line numbers.',
  inputSchema: {
    properties: {
      query: { type: 'string', description: 'Search query' },
      language: { type: 'string', description: 'Filter by language (ts, js, py, etc.)' },
      limit: { type: 'number', description: 'Max results (default: 10)' },
    },
    required: ['query'],
  },
}
```

**`code_index`** - Trigger indexing
```typescript
{
  name: 'code_index',
  description: 'Index/re-index project code files. Respects .gitignore.',
  inputSchema: {
    properties: {
      force: { type: 'boolean', description: 'Re-index all files' },
      dry_run: { type: 'boolean', description: 'Scan only, no indexing' },
    },
  },
}
```

### 4. CLI Commands

```bash
# File watcher daemon
ccmemory watch [project-dir]        # Start watcher for project
ccmemory watch --stop [project-dir] # Stop watcher for project
ccmemory watch --status             # Show active watchers

# One-shot indexing
ccmemory code-index [dir]           # Index code files
ccmemory code-index --force         # Re-index all files
ccmemory code-index --dry-run       # Scan only, no indexing

# Code search
ccmemory code-search <query>        # Search indexed code
ccmemory code-search -l ts <query>  # Filter by language
ccmemory code-search -n 20 <query>  # Limit results
ccmemory code-search --json <query> # JSON output

# Export/Import
ccmemory code-index-export [dir] [-o file.json]  # Export index to JSON
ccmemory code-index-import <file.json> [dir]     # Import index from JSON
```

### 5. Watcher Daemon Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ccmemory watch                           │
├─────────────────────────────────────────────────────────────┤
│  1. Acquire project lock (prevents duplicate watchers)      │
│  2. Load/parse .gitignore patterns                          │
│  3. Initial full scan (index any unindexed/changed files)   │
│  4. Start Bun file watcher (recursive)                      │
│  5. On change: debounce → filter gitignore → index file     │
│  6. Graceful shutdown on SIGINT/SIGTERM                     │
└─────────────────────────────────────────────────────────────┘
```

**Debouncing**: 500ms delay after last change before indexing (handles rapid saves/builds).

**Lock file**: `~/.local/share/ccmemory/watchers/<project-hash>.lock` - contains PID and status, allows `--stop` to signal shutdown.

### 6. Drift Detection & Warnings

**Problem**: Files may change while watcher isn't running. Also, users may forget to start the watcher.

**Drift handling:**
- On watcher start, perform full mtime comparison against `indexed_files` table
- Re-index any files where `file.mtime > indexed_files.mtime`
- Delete index entries for files that no longer exist
- This happens automatically during step 3 (initial full scan)

**Warning when index is empty/stale:**
- If `code_index_state` doesn't exist for project → "Code not indexed"
- If `last_indexed_at` > 24 hours ago → "Index may be stale"
- If indexed file count is 0 → "No files indexed"

**Surfacing warnings to Claude Code:**

MCP tool response with instruction to warn user:
```
IMPORTANT: Tell the user that project code has not been indexed yet.
They should run `ccmemory watch .` in the project directory to enable
semantic code search, or you can run it for them via Bash.

No indexed code to search.
```

## Chunking Strategy: Line-Based with Boundary Detection

**Why not full AST?** Requires language-specific parsers (tree-sitter), adds native dependencies. Line-based with regex heuristics provides ~80% benefit with ~20% complexity.

**Algorithm:**
1. Target ~50 lines per chunk, max 100 lines, min 5 lines
2. Detect boundaries using regex patterns per language:
   - **JS/TS**: function, class, const arrow functions, type/interface, imports
   - **Python**: def, async def, class, import/from
   - **Go**: func, type struct/interface, import
   - **Rust**: fn, struct, enum, trait, impl, use
   - **Java/Kotlin/Scala**: methods, class, interface, import
3. Prefer breaking at boundary patterns or blank lines
4. Extract symbol names (function/class) via regex for filtering
5. Store line numbers for accurate source location

**Supported Languages:**
`ts`, `tsx`, `js`, `jsx`, `py`, `rs`, `go`, `java`, `c`, `cpp`, `h`, `hpp`, `cs`, `rb`, `php`, `swift`, `kt`, `scala`, `sh`, `bash`, `zsh`, `sql`, `json`, `yaml`, `yml`, `toml`, `md`, `css`, `scss`, `html`, `vue`, `svelte`

## Gitignore Handling

- Parse `.gitignore` at project root
- Convert glob patterns to regex
- Default ignore patterns (always applied):
  - `node_modules`, `.git`, `dist`, `build`, `out`, `.next`, `.nuxt`
  - `coverage`, `__pycache__`, `.pytest_cache`, `venv`, `.venv`
  - `target`, `vendor`, `.idea`, `.vscode`
  - `*.min.js`, `*.min.css`, `*.bundle.js`, `*.map`
  - Lock files: `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`, etc.
- Binary/media files filtered by extension

## Efficiency for Large Codebases

- **Incremental indexing**: mtime comparison first (fast), checksum only if mtime changed
- **Batch embedding**: Process 50 chunks at a time via existing `embedBatch()`
- **File size limit**: Skip files > 1MB
- **Empty file skip**: Skip zero-byte files

## Lessons Learned

### What Worked Well

1. **Reusing existing infrastructure**: The `documents` and `document_chunks` tables already had vector storage. Adding columns was simpler than new tables.

2. **Regex-based boundary detection**: Provides good chunk quality without AST parsing complexity. Patterns for 6 language families cover most use cases.

3. **Lock file coordination**: Simple PID-based locking prevents duplicate watchers effectively.

4. **Debouncing**: 500ms delay handles rapid saves well without noticeable lag.

### Challenges Encountered

1. **Bun's `readdir` types**: The `withFileTypes` option returns `Dirent<string>` but TypeScript expected `Dirent<NonSharedBuffer>`. Fixed by using plain `readdir()` + separate `stat()` calls.

2. **Embedding latency**: Full project indexing is slow due to embedding API calls. The dry-run option helps users understand scope before committing.

3. **Progress display**: Console output with `\r` for progress updates works in terminals but may behave oddly in non-TTY contexts.

4. **Bun.file().exists() for directories**: Returns `false` for directories. Fixed `listActiveWatchers()` to use `readdir` with try/catch instead. Discovered via unit tests.

5. **Test design**: Tests should encode desired outcomes, not just current behavior. Tests helped uncover multiple bugs:
   - Directory existence check bug in `listActiveWatchers()`
   - Chunker boundary off-by-one bug where `findBestBreakPoint()` accessed beyond array bounds

6. **Chunker boundary detection**: The `findBestBreakPoint()` function was accessing `lines[targetEnd]` which could be beyond array bounds, causing it to treat `undefined` as an empty line and return `targetEnd + 1` instead of `targetEnd`. Fixed by adding bounds checking.

7. **File deletion detection**: Node.js `fs.watch` emits `rename` events for both file creation AND deletion. Initial implementation only produced `add` events for `rename`, missing deletions entirely. Fixed by checking `existsSync(fullPath)` after a rename event to determine if file was created (`add`) or deleted (`delete`).

8. **Targeted file deletion**: Initial `processFileChanges` called `cleanupDeletedFiles()` on every delete event, which scanned ALL indexed files to check existence. Optimized by adding `deleteFile(projectId, filePath)` method that directly removes a specific file by path, used by both delete events and the full cleanup scan.

### Future Improvements

1. **AST integration**: Could add optional tree-sitter for languages where it matters

### Auto-Reindex on .gitignore Changes (Implemented)

The watcher now automatically detects changes to `.gitignore` and triggers a full re-scan when the gitignore content hash changes. This ensures the index stays consistent with the project's ignore rules.

**Implementation details:**
- `.gitignore` changes are detected via the file watcher
- Debounced with 1000ms delay (longer than code file changes) to handle rapid edits
- Compares gitignore hash to detect actual content changes (not just file touches)
- When hash changes, reloads patterns and calls `onGitignoreChange` callback
- CLI watch command performs force re-index when gitignore changes

**Test coverage:**
- Test: `calls onGitignoreChange when .gitignore is modified`
- Test: `does not call onGitignoreChange if hash is unchanged`
- Test: `does not trigger onFileChange for .gitignore modifications`

### Parallel File Processing (Implemented)

Files are now processed in parallel batches for faster indexing. The implementation uses controlled concurrency to balance speed with resource usage.

**Implementation details:**
- Files are processed in parallel batches of 5 files at a time (`PARALLEL_FILES = 5`)
- Uses `Promise.allSettled` to handle individual file failures gracefully
- Each file's embedding batch is still processed together for optimal API usage
- Errors from individual files don't stop the overall indexing process

**Test coverage:**
- Test: `indexes multiple files in parallel correctly` - verifies 10 files are indexed correctly

### Nested .gitignore Support (Implemented)

The scanner now loads `.gitignore` files from subdirectories as it traverses the project, applying patterns relative to their containing directory.

**Implementation details:**
- New type `NestedGitignoreFilter` extends `GitignoreFilter` with `addNestedGitignore()` method
- `loadGitignorePatternsWithNesting()` creates a filter that accumulates patterns as directories are scanned
- Each pattern tracks its `basePath` to ensure it only applies to its subtree
- Patterns in nested `.gitignore` files don't affect sibling directories

**Test coverage:**
- Test: `respects nested gitignore files` - verifies patterns only apply in their directory
- Test: `nested gitignore patterns only apply to their directory subtree` - verifies sibling dirs unaffected
- Test: `deeply nested gitignore files are respected` - verifies patterns work at any depth

### Export/Import Index for Sharing (Implemented)

Code indexes can now be exported to JSON files and imported into other projects, enabling sharing of pre-indexed codebases.

**CLI Commands:**
```bash
ccmemory code-index-export [project-dir] [-o output.json]  # Export index
ccmemory code-index-import <file.json> [project-dir]       # Import index
```

**Export format (version 1):**
- `version`: Format version number
- `exportedAt`: Timestamp of export
- `projectPath`: Original project path
- `state`: Index state (lastIndexedAt, indexedFiles, gitignoreHash)
- `files[]`: Array of indexed files with:
  - `relativePath`, `language`, `lineCount`, `checksum`
  - `chunks[]`: Array of chunks with content, line numbers, type, symbols, and vectors

**Implementation details:**
- New types: `CodeIndexExport`, `IndexedFileExport`, `ChunkExport`
- Service methods: `exportIndex()`, `importIndex()`
- Import skips files with matching checksum (already indexed)
- Vectors are stored as arrays of numbers for JSON compatibility

**Test coverage:**
- Test: `exports indexed project data` - verifies export contains files with chunks and vectors
- Test: `returns null for unindexed project` - verifies graceful handling
- Test: `imports exported index data` - verifies round-trip export/import
- Test: `skips files with matching checksum` - verifies deduplication on import

### Auto-Start Watcher on Session Start (Implemented)

The code index watcher is automatically started when a Claude Code session begins, if:
1. An index already exists for the project
2. No watcher is currently running for that project

**Implementation details:**
- Hook: `SessionStart` triggers `maybeAutoStartWatcher()`
- Checks for existing watcher via lock file
- Checks for existing index via `getState()`
- Spawns `ccmemory watch <path>` as a detached background process
- Process continues running after hook exits

**Behavior:**
- First session: No auto-start (must run `ccmemory code-index` or `ccmemory watch` first)
- Subsequent sessions: Auto-starts watcher if previous index exists
- Duplicate protection: Won't start if watcher already running

**Test coverage:**
- Test: `auto-start watcher only when index exists` - verifies watcher doesn't start without index

---

## Testing

### Unit Tests

Location: `src/services/codeindex/__test__/`

117 unit tests across 5 test files:

| File | Coverage |
|------|----------|
| `gitignore.test.ts` | Pattern parsing, negation, directory-only patterns, binary/hidden file filtering |
| `chunker.test.ts` | Small file handling, function/class boundary detection, symbol extraction, line limits, multi-language support (TS, Python, Go, Rust) |
| `scanner.test.ts` | Recursive scanning, gitignore filtering, large file skipping, language detection, progress callbacks |
| `coordination.test.ts` | Lock acquisition/release, stale lock cleanup, duplicate prevention, watcher stop/list |
| `watcher.test.ts` | Lock lifecycle, file change detection, gitignore filtering, binary file filtering, debouncing, batching |

### Integration Tests

Location: `tests/integration/codeindex.test.ts`

22 integration tests covering:
- CLI `code-index`: indexing, dry-run, gitignore respect
- CLI `code-search`: finding code, JSON output, language filtering
- CLI `watch`: start/stop lifecycle, status reporting, duplicate prevention
- MCP tools: `code_search` and `code_index` functionality

### MCP Tool Tests

Location: `src/mcp/__test__/server.test.ts`

10 tests covering `code_search` and `code_index` MCP tools.

---

## Acceptance Criteria

### Must Have (P0)

- [x] `ccmemory code-index` scans project and creates searchable index
- [x] `ccmemory code-index --dry-run` reports file count without indexing
- [x] `ccmemory code-search <query>` returns relevant code snippets with file:line
- [x] `code_search` MCP tool available to Claude Code agent
- [x] `code_index` MCP tool available to Claude Code agent
- [x] Respects `.gitignore` patterns
- [x] Skips `node_modules`, `.git`, `dist`, and other common directories
- [x] Incremental indexing (only re-index changed files)
- [x] Database migration adds required tables/columns
- [x] All existing tests continue to pass
- [x] Build completes without TypeScript errors

### Should Have (P1)

- [x] `ccmemory watch` starts background watcher daemon
- [x] `ccmemory watch --stop` gracefully stops watcher
- [x] `ccmemory watch --status` shows active watchers
- [x] Lock file prevents duplicate watchers per project
- [x] Watcher debounces rapid file changes (500ms)
- [x] Code chunks detect function/class boundaries
- [x] Symbol names extracted for filtering
- [x] MCP tool warns when index is empty/stale
- [x] CLI shows progress during indexing
- [x] `--force` option re-indexes all files
- [x] `--language` filter in search

### Nice to Have (P2)

- [x] Watcher auto-re-indexes on `.gitignore` changes
- [x] Parallel file processing for faster indexing
- [x] Nested `.gitignore` support
- [x] Auto-start watcher on session start
- [x] Index statistics in `ccmemory stats`
- [x] Export/import index for sharing

### Quality Gates

| Metric | Target | Actual |
|--------|--------|--------|
| Build passes | Yes | Yes |
| Test pass rate | 100% | 100% (641/641) |
| TypeScript errors | 0 | 0 |
| Dry-run works | Yes | Yes |
| Help text updated | Yes | Yes |
| Unit tests added | Yes | Yes (118 tests in codeindex) |
| Integration tests added | Yes | Yes (33 tests) |
| MCP tool tests added | Yes | Yes (10 tests) |
| Stats command shows code index | Yes | Yes |

---

## Files Created

```
src/services/codeindex/
  types.ts        # Type definitions
  gitignore.ts    # .gitignore parser
  scanner.ts      # Directory scanner
  chunker.ts      # Code chunker with boundary detection
  coordination.ts # Lock file management
  watcher.ts      # File watcher daemon
  index.ts        # Main CodeIndexService

src/services/codeindex/__test__/
  gitignore.test.ts    # Gitignore parsing and filtering tests
  chunker.test.ts      # Code chunking tests
  scanner.test.ts      # Directory scanning tests
  coordination.test.ts # Lock file coordination tests
  watcher.test.ts      # File watcher functionality tests

src/cli/commands/
  watch.ts        # Watch daemon CLI
  code-index.ts   # One-shot indexing CLI
  code-search.ts  # Search CLI

tests/integration/
  codeindex.test.ts    # Integration tests for code indexing
```

## Files Modified

- `src/db/migrations.ts` - Added migration v7
- `src/mcp/server.ts` - Added `code_search` and `code_index` tools
- `src/main.ts` - Added CLI commands and help text
- `src/services/codeindex/coordination.ts` - Fixed `listActiveWatchers()` directory check bug
- `src/services/codeindex/chunker.ts` - Fixed `findBestBreakPoint()` off-by-one bug at array bounds
- `src/cli/commands/stats.ts` - Added code index statistics (indexed files, documents, chunks, languages)

## Usage Flow

1. User starts watcher: `ccmemory watch ~/myproject` (or agent can start it via CLI)
2. Watcher indexes all code files initially
3. Watcher runs in background, re-indexing on file changes
4. Agent uses `code_search` MCP tool to find relevant code
5. User stops watcher when done: `ccmemory watch --stop`

Alternative (one-shot):
1. User runs `ccmemory code-index` to index project
2. Agent uses `code_search` MCP tool
3. Re-run `ccmemory code-index` when code changes significantly
