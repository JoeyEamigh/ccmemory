# CCMemory

Self-contained memory plugin for Claude Code. Provides persistent, searchable memory across sessions with automatic vectorization via Ollama.

## Features

- **5-Sector Memory Model**: Episodic, semantic, procedural, emotional, and reflective memory types
- **3-Tier System**: Session → Project → Global memory promotion
- **Hybrid Search**: Combined FTS5 full-text and vector similarity search
- **Automatic Capture**: PostToolUse hooks capture tool observations
- **Session Summaries**: Stop hooks generate reflective summaries via Claude SDK
- **Salience Decay**: Time-based memory decay with reinforcement on access
- **Deduplication**: SimHash-based duplicate detection with automatic boosting
- **Document Ingestion**: Chunk and vectorize files/URLs for RAG
- **WebUI**: Real-time browser interface with WebSocket updates
- **Multi-Agent Support**: Track memories across concurrent Claude Code instances

## Requirements

- [Bun](https://bun.sh/) v1.0+
- [Ollama](https://ollama.ai/) with `qwen3-embedding` model (or OpenRouter API key)

## Installation

```bash
# Clone the repository
git clone https://github.com/your-username/ccmemory.git
cd ccmemory

# Install dependencies
bun install

# Pull the embedding model
ollama pull qwen3-embedding

# Build everything
bun run build:all
```

## Quick Start

### As a Claude Code Plugin

1. Copy the plugin to your Claude Code plugins directory:

```bash
cp -r plugin ~/.claude/plugins/ccmemory
```

2. The plugin auto-registers hooks and MCP server. Restart Claude Code to activate.

### CLI Usage

```bash
# Search memories
ccmemory search "authentication flow"

# Search with filters
ccmemory search "API design" --sector semantic --tier project

# Show memory details
ccmemory show <memory-id>

# View statistics
ccmemory stats

# Start WebUI
ccmemory serve --open
```

### MCP Server (Standalone)

```bash
# Start the MCP server
bun run src/mcp/server.ts
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Claude Code Plugin                       │
├─────────────┬─────────────┬─────────────┬──────────────────┤
│ PostToolUse │    Stop     │ SessionEnd  │   MCP Server     │
│   Capture   │  Summarize  │   Cleanup   │     Tools        │
└──────┬──────┴──────┬──────┴──────┬──────┴────────┬─────────┘
       │             │             │               │
       ▼             ▼             ▼               ▼
┌─────────────────────────────────────────────────────────────┐
│                      Memory Service                          │
├─────────────┬─────────────┬─────────────┬──────────────────┤
│   Sectors   │    Decay    │   Dedup     │  Relationships   │
│  (5 types)  │  (salience) │  (simhash)  │   (4 types)      │
└──────┬──────┴──────┬──────┴──────┬──────┴────────┬─────────┘
       │             │             │               │
       ▼             ▼             ▼               ▼
┌─────────────────────────────────────────────────────────────┐
│                      Search Service                          │
├─────────────────────────────┬───────────────────────────────┤
│         FTS5 Search         │       Vector Search           │
│    (full-text + snippets)   │   (cosine similarity)         │
└──────────────┬──────────────┴───────────────┬───────────────┘
               │                              │
               ▼                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Embedding Service                        │
├─────────────────────────────┬───────────────────────────────┤
│     Ollama Provider         │    OpenRouter Provider        │
│   (qwen3-embedding)         │   (API fallback)              │
└──────────────┬──────────────┴───────────────┬───────────────┘
               │                              │
               ▼                              ▼
┌─────────────────────────────────────────────────────────────┐
│                       libSQL Database                        │
│              (WAL mode, F32_BLOB vectors)                   │
└─────────────────────────────────────────────────────────────┘
```

## Memory Sectors

| Sector | Description | Decay Rate |
|--------|-------------|------------|
| **Episodic** | What happened (tool calls, observations) | Fast (0.1/day) |
| **Semantic** | Facts and knowledge | Slow (0.02/day) |
| **Procedural** | How to do things (commands, workflows) | Very slow (0.01/day) |
| **Emotional** | Preferences and reactions | Medium (0.05/day) |
| **Reflective** | Session summaries and insights | Very slow (0.01/day) |

## Memory Tiers

| Tier | Scope | Promotion |
|------|-------|-----------|
| **Session** | Current session only | Auto-promoted on session end |
| **Project** | Current project | Manual or high salience |
| **Global** | All projects | High-value cross-project knowledge |

## MCP Tools

| Tool | Description |
|------|-------------|
| `memory_search` | Hybrid search with sector/tier filtering |
| `memory_timeline` | Chronological view with session grouping |
| `memory_add` | Create new memory with auto-classification |
| `memory_reinforce` | Increase salience (mark as useful) |
| `memory_deemphasize` | Decrease salience (mark as less relevant) |
| `memory_delete` | Soft delete a memory |
| `memory_supersede` | Replace old memory with new version |
| `docs_search` | Search ingested documents |
| `docs_ingest` | Ingest file, URL, or raw content |

## Configuration

Environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `CCMEMORY_DATA_DIR` | Database location | `~/.local/share/ccmemory` |
| `CCMEMORY_CONFIG_DIR` | Config files | `~/.config/ccmemory` |
| `CCMEMORY_CACHE_DIR` | Cache files | `~/.cache/ccmemory` |
| `OPENROUTER_API_KEY` | OpenRouter API key | (none) |
| `LOG_LEVEL` | Log verbosity | `info` |

## Development

```bash
# Type check
bun run typecheck

# Run all tests
bun run test

# Run specific test file
bun test src/services/memory/__test__/store.test.ts

# Build CLI
bun run build:cli

# Build MCP server
bun run build:mcp

# Check Ollama status
bun run ollama:check

# View database stats
bun run db:counts
```

## Project Structure

```
src/
├── cli/              # CLI commands
├── db/               # libSQL connection, schema, migrations
├── mcp/              # MCP server (stdio transport)
├── services/
│   ├── documents/    # Chunking, ingestion
│   ├── embedding/    # Ollama/OpenRouter providers
│   ├── memory/       # Store, sectors, decay, dedup
│   └── search/       # FTS5, vector, hybrid ranking
├── utils/            # Paths, logging
└── webui/            # Bun server, React SSR, WebSocket

scripts/              # Hook scripts (capture, summarize, cleanup)
plugin/               # Claude Code plugin config
spec/                 # Design specifications
tests/                # Integration tests
```

## Hooks

The plugin uses three hooks:

1. **PostToolUse** (`capture.ts`): Captures tool observations as episodic memories
2. **Stop** (`summarize.ts`): Generates session summary via Claude SDK agent
3. **SessionEnd** (`cleanup.ts`): Promotes session memories, ends session

## WebUI

Start the WebUI server:

```bash
ccmemory serve --port 37778 --open
```

Features:
- Real-time memory updates via WebSocket
- Search with sector/tier filters
- Timeline view with session grouping
- Memory reinforcement/de-emphasis
- Multi-agent activity tracking

## License

MIT
