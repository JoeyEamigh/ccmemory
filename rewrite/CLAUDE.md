# CCEngram Build Agent

You are building **CCEngram** - intelligent memory and code search for Claude Code.

## Architecture Summary

**Single daemon + thin clients** via Unix socket IPC:

- `ccengram daemon` - Long-running process (LanceDB, file watcher, all logic)
- `ccengram mcp-proxy` - Thin stdio proxy for Claude Code MCP
- `ccengram hook <name>` - Thin hook client, sends event to daemon

**Storage: LanceDB** (per-project isolation)

- Each project gets its own LanceDB at `~/.local/share/ccengram/projects/<hash>/lancedb/`
- Tables: `memories` (with vectors), `code_chunks` (with vectors), `sessions`, `events`

**Search: Pure vector** - No hybrid FTS blending (hurt quality in testing)

## Crate Structure

```
crates/
├── core/       # Domain types (Memory, Sector, CodeChunk, etc.)
├── db/         # LanceDB wrapper, per-project connections
├── embedding/  # Ollama/OpenRouter embedding providers
├── index/      # File scanner, tree-sitter parser, chunker
├── extract/    # Dedup (SimHash), decay, sector classification
├── daemon/     # Unix socket server, request router, tools/hooks
└── cli/        # Binary: main.rs, proxy, hook client, CLI commands
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
cargo build              # Build all
cargo nextest run        # Run tests
cargo clippy --all       # Lint all
cargo fmt --all          # Format all
cargo run -p cli -- daemon          # Run daemon
cargo run -p cli -- search "query"  # Search
```
