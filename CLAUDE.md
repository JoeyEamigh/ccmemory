# CCEngram Build Agent

You are building **CCEngram** - intelligent memory and code search for Claude Code.

## Architecture Summary

**Single daemon + thin clients** via Unix socket IPC:

- `ccengram daemon` - Long-running process (embeddings, file watcher, all logic)
- `ccengram mcp` - Thin stdio proxy for Claude Code MCP
- `ccengram hook <name>` - Thin hook client, sends event to daemon

**Storage: LanceDB** (per-project isolation)

- Each project gets its own database at `~/.local/share/ccengram/projects/<hash>/`
- Tables: `memories`, `code_chunks`, `documents`, `sessions`, `entities`

**Search: Pure vector** - Semantic similarity via embeddings

## Crate Structure

```
crates/
├── core/       # Domain types (Memory, Sector, CodeChunk, Config)
├── db/         # LanceDB wrapper, per-project connections
├── embedding/  # Ollama embedding provider
├── index/      # File scanner, tree-sitter parser, chunker
├── extract/    # Dedup (SimHash), decay, sector classification
├── llm/        # LLM integration for summarization
├── daemon/     # Unix socket server, request router, tools/hooks
├── tui/        # Terminal user interface (ratatui)
└── cli/        # Binary: main.rs, MCP proxy, hook client, commands
```

## Type Safety Rules

- **NO `any`**. NO `unwrap()` in library code. Use `?` and proper error types.
- Use `thiserror` for error enums
- Validate all inputs at boundaries

## Testing

- Unit tests: Colocated in `src/` as `#[cfg(test)]` modules
- Integration tests: `tests/integration/`
- Run with `cargo test`

## Sample Commands

```bash
cargo build                               # Build all
cargo test                                # Run tests
cargo clippy --all-targets --all-features # Lint all
cargo fmt --all                           # Format all
```

## MCP Tools

### Unified Exploration Tools (Minimal Preset - Recommended)

The minimal preset provides 2 powerful tools optimized for codebase exploration:

#### `explore` - Universal semantic search

Search code, memories, and docs with one tool. Returns ranked results with navigation hints.

```json
{
  "query": "authentication flow",  // Natural language or symbol name
  "scope": "all",                  // "code" | "memory" | "docs" | "all"
  "expand_top": 3,                 // Include full context for top N results
  "limit": 10                      // Max results per scope
}
```

**Response includes:**
- Ranked results with preview, file:line, and type
- Navigation hints (caller count, callee count, related memory count)
- Full context for top `expand_top` results (callers, callees, siblings, memories)
- Suggested related queries for further exploration

#### `context` - Comprehensive drill-down

Get full context for any explore result. Auto-detects type (code/memory/doc).

```json
{
  "id": "abc123",      // Single ID from explore results
  "ids": ["a", "b"],   // OR array of IDs (max 5) for batch context
  "depth": 5           // Items per section (callers, callees, etc.)
}
```

**Returns for code:** content, callers, callees, siblings, related memories
**Returns for memory:** content, timeline, related memories
**Returns for docs:** content, before/after chunks

### Typical Exploration Flow

```
# 1. Start broad - find entry points
explore("authentication", expand_top=3)

# 2. Drill into specific result if needed
context("chunk_abc123")

# 3. Follow suggestions or callers/callees
explore("session management", scope="code")
```

## Tool Presets

| Preset | Tools | Description |
|--------|-------|-------------|
| **minimal** | `explore`, `context` | Streamlined exploration (2 tools) - **Recommended** |
| **standard** | Above + memory management, code maintenance, diagnostics | Daily driver (11 tools) |
| **full** | All 40 tools including legacy search tools | Everything |

Initialize project config:

```bash
ccengram config init --preset minimal
```

## CLI Commands

```bash
# Core
ccengram daemon                     # Start daemon (required)
ccengram watch                      # Start file watcher
ccengram tui                        # Launch TUI
ccengram mcp                        # MCP server (for plugin)
ccengram hook <name>                # Handle hook event

# Search (memories, code, docs)
ccengram search memories "query"    # Search memories
ccengram search code "query"        # Search code
ccengram search docs "query"        # Search documents

# Context retrieval
ccengram context <chunk_id>         # Get context (auto-detects code vs doc)
ccengram context <id> --before 30   # More lines/chunks before
ccengram context <id> --after 30    # More lines/chunks after
ccengram context <id> --json        # JSON output

# Memory management
ccengram memory show <id>           # Show memory details
ccengram memory delete <id>         # Soft-delete a memory
ccengram memory restore <id>        # Restore a soft-deleted memory
ccengram memory deleted             # List soft-deleted memories
ccengram memory export              # Export memories to file
ccengram memory archive             # Archive old low-salience memories

# Index management
ccengram index                      # Index code (default)
ccengram index code                 # Index code files
ccengram index code --force         # Re-index all code
ccengram index code --stats         # Show code index stats
ccengram index docs                 # Index documents from configured directory
ccengram index docs --directory ./  # Index docs from specific directory
ccengram index file <path>          # Index a single file (auto-detects type)

# Configuration
ccengram config help                # Show presets and config locations
ccengram config show                # Show effective config
ccengram config init                # Create project config (standard preset)
ccengram config init --preset minimal
ccengram config reset               # Reset user config to defaults

# Project management
ccengram projects list              # List all indexed projects
ccengram projects show <path>       # Show project details
ccengram projects clean <path>      # Remove a project's data
ccengram projects clean-all         # Remove all project data

# Logs
ccengram logs                       # Show last 50 lines of logs
ccengram logs -f                    # Follow logs in real-time
ccengram logs -n 100                # Show last 100 lines
ccengram logs --level error         # Filter by log level
ccengram logs --open                # Open log directory

# Utilities
ccengram stats                      # Show statistics
ccengram health                     # Health check
ccengram migrate                    # Migrate embeddings
ccengram update                     # Check for updates
ccengram agent                      # Generate MemExplore agent

# Shell completions
ccengram completions bash           # Generate bash completions
ccengram completions zsh            # Generate zsh completions
ccengram completions fish           # Generate fish completions
ccengram completions powershell     # Generate PowerShell completions
```
