# CCMemory Installation Guide

## Prerequisites

### Bun Runtime

CCMemory requires Bun v1.0 or later.

```bash
# Install Bun (Linux/macOS)
curl -fsSL https://bun.sh/install | bash

# Verify installation
bun --version
```

### Ollama (Recommended)

Ollama provides local embeddings without API costs.

```bash
# Install Ollama (Linux)
curl -fsSL https://ollama.ai/install.sh | sh

# Install Ollama (macOS)
brew install ollama

# Start Ollama service
ollama serve

# Pull the embedding model
ollama pull qwen3-embedding

# Verify model is available
ollama list
```

### OpenRouter (Alternative)

If you prefer cloud-based embeddings or don't have GPU resources:

1. Get an API key from [OpenRouter](https://openrouter.ai/)
2. Set the environment variable:

```bash
export OPENROUTER_API_KEY="your-api-key-here"
```

CCMemory will automatically fall back to OpenRouter if Ollama is unavailable.

## Installation Methods

### Method 1: As a Claude Code Plugin (Recommended)

This is the primary use case - CCMemory runs automatically with Claude Code.

```bash
# Clone the repository
git clone https://github.com/your-username/ccmemory.git
cd ccmemory

# Install dependencies
bun install

# Build everything
bun run build:all

# Install the plugin
mkdir -p ~/.claude/plugins
cp -r plugin ~/.claude/plugins/ccmemory

# Restart Claude Code to activate
```

The plugin will:
- Capture tool observations automatically (PostToolUse hook)
- Generate session summaries when conversations end (Stop hook)
- Promote session memories on session end (SessionEnd hook)
- Provide MCP tools for memory search and management

### Method 2: CLI Only

Use CCMemory as a standalone command-line tool.

```bash
# Clone and build
git clone https://github.com/your-username/ccmemory.git
cd ccmemory
bun install
bun run build:cli

# Add to PATH (optional)
sudo ln -s $(pwd)/dist/ccmemory /usr/local/bin/ccmemory

# Or add to your shell profile
echo 'export PATH="$PATH:$(pwd)/dist"' >> ~/.bashrc
source ~/.bashrc
```

### Method 3: MCP Server Only

Run the MCP server for integration with other tools.

```bash
# Clone and build
git clone https://github.com/your-username/ccmemory.git
cd ccmemory
bun install
bun run build:mcp

# Run the MCP server
./dist/mcp-server
```

### Method 4: Development Mode

For contributors or local development:

```bash
# Clone and install
git clone https://github.com/your-username/ccmemory.git
cd ccmemory
bun install

# Run type checking
bun run typecheck

# Run tests
bun run test

# Run in development mode (no build required)
bun run src/cli/index.ts search "test query"
bun run src/mcp/server.ts
```

## Configuration

### Data Directories

CCMemory follows XDG Base Directory Specification:

| Platform | Data | Config | Cache |
|----------|------|--------|-------|
| Linux | `~/.local/share/ccmemory` | `~/.config/ccmemory` | `~/.cache/ccmemory` |
| macOS | `~/Library/Application Support/ccmemory` | `~/Library/Preferences/ccmemory` | `~/Library/Caches/ccmemory` |
| Windows | `%LOCALAPPDATA%\ccmemory` | `%APPDATA%\ccmemory` | `%LOCALAPPDATA%\ccmemory\cache` |

Override with environment variables:

```bash
export CCMEMORY_DATA_DIR="/custom/path/data"
export CCMEMORY_CONFIG_DIR="/custom/path/config"
export CCMEMORY_CACHE_DIR="/custom/path/cache"
```

### Logging

Control log verbosity:

```bash
# Options: debug, info, warn, error
export LOG_LEVEL="debug"
```

Logs are written to `$CCMEMORY_DATA_DIR/logs/ccmemory.log`.

### Embedding Configuration

Create `$CCMEMORY_CONFIG_DIR/config.json`:

```json
{
  "embedding": {
    "provider": "ollama",
    "model": "qwen3-embedding",
    "ollamaUrl": "http://localhost:11434"
  }
}
```

For OpenRouter:

```json
{
  "embedding": {
    "provider": "openrouter",
    "model": "openai/text-embedding-3-small"
  }
}
```

## Verification

### Check Ollama Connection

```bash
bun run ollama:check
```

Expected output:
```
Ollama connection: OK
Model qwen3-embedding: available
Embedding dimensions: 1024
```

### Check Database

```bash
bun run db:counts
```

Expected output:
```
Memories: 0
Sessions: 0
Projects: 0
Documents: 0
Embeddings: 0
```

### Run Health Check

```bash
ccmemory health
```

Expected output:
```
Database: OK
Embedding: OK (ollama/qwen3-embedding)
FTS5: OK
Vector search: OK
```

### Run Tests

```bash
bun run test
```

All 360+ tests should pass.

## Troubleshooting

### Ollama Connection Failed

```
Error: Failed to connect to Ollama at http://localhost:11434
```

**Solutions:**
1. Ensure Ollama is running: `ollama serve`
2. Check if the port is blocked by firewall
3. Verify the URL in config matches Ollama's actual address

### Model Not Found

```
Error: Model qwen3-embedding not found
```

**Solutions:**
1. Pull the model: `ollama pull qwen3-embedding`
2. Check available models: `ollama list`
3. Use a different model and update config

### Permission Denied

```
Error: EACCES: permission denied, mkdir '/home/user/.local/share/ccmemory'
```

**Solutions:**
1. Create directory manually: `mkdir -p ~/.local/share/ccmemory`
2. Fix permissions: `chmod 755 ~/.local/share/ccmemory`

### Database Locked

```
Error: database is locked
```

**Solutions:**
1. Only one write process should access the database at a time
2. If a process crashed, delete the lock file: `rm $CCMEMORY_DATA_DIR/*.lock`
3. Check for zombie processes: `ps aux | grep ccmemory`

### Tests Failing

```
Error: 50 tests failed
```

**Solutions:**
1. Ensure Ollama is running with the correct model
2. Clean test artifacts: `rm -rf /tmp/ccmemory-*`
3. Check for port conflicts on 37778 (WebUI tests)

## Updating

```bash
cd ccmemory
git pull origin main
bun install
bun run build:all

# If using as plugin, update the installed copy
cp -r plugin ~/.claude/plugins/ccmemory
```

## Uninstalling

```bash
# Remove plugin
rm -rf ~/.claude/plugins/ccmemory

# Remove data (optional - this deletes all memories!)
rm -rf ~/.local/share/ccmemory
rm -rf ~/.config/ccmemory
rm -rf ~/.cache/ccmemory

# Remove CLI from PATH
rm /usr/local/bin/ccmemory
```
