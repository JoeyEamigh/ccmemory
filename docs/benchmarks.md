# CCEngram Benchmark Harness

Benchmarking for testing the **exploration** capabilities of CCEngram's `explore` and `context` tools against large real-world codebases.

## Philosophy: Exploration vs Search

| Search | Exploration |
|--------|-------------|
| "Find the `LanguageServer` struct" | "How does code intelligence work?" |
| Agent knows what to look for | Agent has no prior knowledge |
| Needle in a haystack | Building a mental map |
| Measure: Did we find the exact thing? | Measure: Can the agent now do useful work? |

**Key insight:** Agents are dropped into unfamiliar codebases. They don't know the terminology (`Action`, `GPUI`, `ExtensionHost`). They need to discover architecture through natural questions.

## Quick Start

```bash
# 1. Start the daemon (required - leave running in background)
ccengram daemon &

# 2. Build the benchmark tool
cargo build -p benchmark

# 3. Download test repositories
cargo run -p benchmark -- download --repos zed

# 4. Index the repositories (code and docs, with progress)
cargo run -p benchmark -- index --repos zed

# 5. Run benchmarks
cargo run -p benchmark -- run --scenarios "zed*" --output ./results

# 6. Compare runs for regressions
cargo run -p benchmark -- compare baseline.json current.json --threshold 10
```

### Performance Benchmarks

For indexing and watcher performance testing:

```bash
# Incremental indexing performance
cargo run -p benchmark -- incremental-perf --repos zed

# File watcher performance
cargo run -p benchmark -- watcher-perf --repo zed

# Large file handling
cargo run -p benchmark -- large-file-perf --repo zed
```

The flow is: **download → index → run**. Each step is explicit:
- `download` downloads repos to cache
- `index` indexes code and docs via daemon (with streaming progress)
- `run` checks repos are indexed, then executes scenarios

### Using OpenRouter for Embeddings

To use OpenRouter instead of Ollama for embeddings:

```bash
# 1. Set API key and start daemon
export OPENROUTER_API_KEY="sk-or-..."
ccengram daemon &

# 2. Download and index with OpenRouter
cargo run -p benchmark -- download --repos zed
cargo run -p benchmark -- index --repos zed --embedding-provider openrouter

# Or provide the key directly
cargo run -p benchmark -- index --repos zed \
  --embedding-provider openrouter \
  --openrouter-api-key "sk-or-..."
```

## How Benchmarks Work

### Execution Flow

1. **Load scenarios** from TOML files
2. **Group by repository** (scenarios specify `repo = "zed"` or `repo = "vscode"`)
3. **Verify each repo** is downloaded and indexed (fails with helpful message if not)
4. **Execute steps** sequentially:
   - Run `explore` query against daemon with `cwd` = repo path
   - Optionally fetch `context` for top N results
   - Resolve templates (`{{previous.symbol}}`) from previous results
5. **Collect metrics** at each step (latency, recall, noise)
6. **Evaluate success criteria** against thresholds
7. **Generate reports** (JSON for CI, Markdown for humans)

### What Gets Measured

The benchmark measures whether exploration:
- **Finds important files/symbols** (recall)
- **Avoids test/mock code** (noise ratio)
- **Converges quickly** (discoveries per step)
- **Doesn't waste context budget** (bloat)
- **Enables understanding** (LLM comprehension, optional)

## CLI Reference

### `run` - Execute Benchmarks

```bash
cargo run -p benchmark -- run [OPTIONS]

Options:
  -o, --output <DIR>         Output directory [default: ./benchmark-results]
  -s, --scenarios <PATTERN>  Filter by glob pattern (e.g., "zed*", "*blind*")
      --parallel             Run scenarios concurrently
      --llm-judge            Enable LLM comprehension evaluation
      --scenarios-dir <DIR>  Custom scenarios directory
```

### `list` - Show Available Scenarios

```bash
cargo run -p benchmark -- list [OPTIONS]

Options:
  -d, --detailed    Show full scenario details including steps
```

### `compare` - Regression Detection

```bash
cargo run -p benchmark -- compare <BASELINE> <CURRENT> [OPTIONS]

Arguments:
  <BASELINE>  Previous results JSON file
  <CURRENT>   Current results JSON file

Options:
  -t, --threshold <PCT>  Regression threshold [default: 10]
```

### `download` - Download Repositories

```bash
cargo run -p benchmark -- download [OPTIONS]

Options:
  -r, --repos <LIST>   Repositories: zed, vscode, or 'all' [default: all]
      --force          Force re-download
```

Downloads repository tarballs from GitHub to `~/.cache/ccengram-bench/repos/`.

### `index` - Index Repositories

```bash
cargo run -p benchmark -- index [OPTIONS]

Options:
  -r, --repos <LIST>           Repositories: zed, vscode, or 'all' [default: all]
      --force                  Force re-index even if already indexed
      --embedding-provider     Embedding provider: ollama or openrouter [default: ollama]
      --openrouter-api-key     OpenRouter API key (or set OPENROUTER_API_KEY env var)
```

Indexes code and docs via the daemon with streaming progress display. Creates CCEngram databases at `~/.local/share/ccengram/projects/`.

### `index-perf` - Indexing Performance

```bash
cargo run -p benchmark -- index-perf [OPTIONS]

Options:
  -r, --repos <LIST>      Repos to benchmark [default: all]
  -i, --iterations <N>    Iterations per repo [default: 3]
      --cold              Clear index between iterations
```

### `incremental-perf` - Incremental Indexing Performance

Measures how quickly the system detects and reindexes modified files.

```bash
cargo run -p benchmark -- incremental-perf [OPTIONS]

Options:
  -r, --repos <LIST>        Repos to benchmark [default: all]
  -f, --files-per-iter <N>  Files to modify per iteration [default: 10]
  -i, --iterations <N>      Iterations per repo [default: 3]
  -o, --output <DIR>        Output directory [default: ./benchmark-results]
      --cache-dir <DIR>     Cache directory for repositories
```

**What it measures:**
- Time per changed file (target: < 200ms)
- Detection accuracy (true positives, false positives, false negatives)
- Large file handling (1MB, 5MB, 10MB, 50MB files)

**Output:** `incremental.json` and `incremental.md`

### `watcher-perf` - File Watcher Performance

Comprehensive benchmarks for the file watcher subsystem.

```bash
cargo run -p benchmark -- watcher-perf [OPTIONS]

Options:
  -r, --repo <NAME>         Repository to test [default: zed]
  -i, --iterations <N>      Iterations per test [default: 5]
  -o, --output <DIR>        Output directory [default: ./benchmark-results]
      --cache-dir <DIR>     Cache directory for repositories
      --test <TYPE>         Run specific test only (see below)
```

**Test types** (use with `--test`):
| Type | Description |
|------|-------------|
| `lifecycle` | Watcher startup/shutdown latency, resource leak detection |
| `single` | End-to-end latency from file save to searchable |
| `batch` | Debounce accuracy (50 rapid file changes) |
| `operations` | Create/modify/delete/rename handling |
| `gitignore` | Respect rate for ignored vs tracked files |

**Examples:**
```bash
# Run all watcher tests
cargo run -p benchmark -- watcher-perf --repo zed --iterations 5

# Run only end-to-end latency test
cargo run -p benchmark -- watcher-perf --repo zed --test single

# Run only gitignore respect test
cargo run -p benchmark -- watcher-perf --repo zed --test gitignore
```

**Output:** `watcher.json` and `watcher.md`

### `large-file-perf` - Large File Handling

Tests indexing behavior with files of various sizes.

```bash
cargo run -p benchmark -- large-file-perf [OPTIONS]

Options:
  -o, --output <DIR>        Output directory [default: ./benchmark-results]
      --sizes-mb <LIST>     File sizes in MB [default: 1,5,10,50]
  -r, --repo <NAME>         Repository for testing [default: zed]
      --cache-dir <DIR>     Cache directory for repositories
```

**Output:** `large_file.json`

## Creating Scenarios

Scenarios are TOML files in `crates/benchmark/scenarios/`. Run `list --detailed` to see existing ones.

### Scenario Structure

```toml
[scenario]
id = "unique-id"
name = "Human-Readable Name"
repo = "zed"              # or "vscode"
difficulty = "medium"     # easy, medium, hard
description = "What this tests"

[task]
prompt = "High-level exploration goal"
intent = "architectural_discovery"  # See intent types below

[expected]
# Validation targets - queries should NOT use these terms
must_find_files = ["**/buffer.rs", "**/editor.rs"]
must_find_symbols = ["Buffer", "save", "write"]
noise_patterns = ["**/tests/**", "test_*", "Mock*"]

[[steps]]
query = "How does file saving work?"  # Natural language, no jargon
scope = "code"
expand_top = 4

[[steps]]
query = "What triggers a save operation?"
depends_on_previous = true
expand_top = 3

[[steps]]
query = "How does {{previous.symbol}} handle errors?"
depends_on_previous = true

[success]
min_discovery_score = 0.6
max_noise_ratio = 0.3
min_convergence_rate = 0.5
```

### Intent Types

| Intent | Use For |
|--------|---------|
| `architectural_discovery` | Understanding system structure |
| `feature_exploration` | "How does X work?" |
| `bug_investigation` | Tracing failure paths |
| `flow_tracing` | Following data through system |
| `task_completion` | Finding everything to complete a task |

### Query Patterns

**The key rule:** Write queries as a zero-knowledge agent would ask. Never use codebase-specific terms in queries.

```toml
# ❌ Bad - assumes knowledge
query = "What is the Action trait?"
query = "Show me LanguageServer"

# ✅ Good - describes observable behavior
query = "How do keyboard shortcuts trigger functionality?"
query = "How does code intelligence work?"
```

#### Pattern 1: Feature Exploration
```toml
[[steps]]
query = "How does file saving work?"

[[steps]]
query = "What triggers a save operation?"
depends_on_previous = true

[[steps]]
query = "How are unsaved changes tracked?"
depends_on_previous = true
```

#### Pattern 2: Bug Investigation
```toml
[[steps]]
query = "How does file saving work?"

[[steps]]
query = "What errors can occur during save?"
depends_on_previous = true

[[steps]]
query = "How is {{previous.symbol}} handled when it fails?"
depends_on_previous = true
```

#### Pattern 3: Data Flow
```toml
[[steps]]
query = "How are keyboard events received?"

[[steps]]
query = "How do events reach the text editor?"
depends_on_previous = true

[[steps]]
query = "How does {{previous.symbol}} insert text?"
depends_on_previous = true
```

#### Pattern 4: Blind Exploration
```toml
[[steps]]
query = "What is the main entry point?"

[[steps]]
query = "How is the UI organized?"
depends_on_previous = true

[[steps]]
query = "What does {{previous.symbol}} do?"
depends_on_previous = true
```

### Adaptive Templates

Use templates to follow discoveries from previous steps:

| Template | Resolves To |
|----------|-------------|
| `{{previous.symbol}}` | First symbol from previous step |
| `{{previous.symbols[N]}}` | Nth symbol |
| `{{previous.file}}` | First file path |
| `{{previous.id}}` | First result ID |
| `{{previous.caller}}` | First caller (from context) |
| `{{previous.callee}}` | First callee (from context) |

### Step Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `query` | string | required | The exploration query |
| `scope` | string | `"all"` | `code`, `memory`, `docs`, `all` |
| `expand_top` | int | `3` | Get full context for top N results |
| `depends_on_previous` | bool | `false` | Enable template resolution |
| `context_ids` | array | `[]` | Specific IDs to fetch context for |

### Success Criteria

| Criterion | Type | Target | Description |
|-----------|------|--------|-------------|
| `min_discovery_score` | float | 0.7 | File recall |
| `max_noise_ratio` | float | 0.25 | Max noise in results |
| `max_steps_to_core` | int | 3 | Steps to first useful result |
| `min_convergence_rate` | float | 0.7 | Discovery rate per step |
| `max_context_bloat` | float | 0.3 | Wasted context calls |
| `max_dead_end_ratio` | float | 0.2 | Steps with no discoveries |
| `min_file_diversity` | float | 0.6 | Unique files in top-5 |

### LLM Comprehension Testing

Test whether exploration enables understanding (requires `--llm-judge` flag):

```toml
[llm_judge]
min_comprehension_score = 0.5

[[llm_judge.comprehension_questions]]
question = "How are commands represented?"
expected_concepts = ["Action", "trait", "dispatch"]
wrong_concepts = ["Redux", "event listeners"]
weight = 1.0
```

### Task-Completion Scenarios

For scenarios measuring whether exploration enables a specific task:

```toml
[task]
intent = "task_completion"

[task_requirements]
must_identify_modification_points = true
must_find_example = true
must_find_related_concerns = ["action definition", "keybinding"]

modification_point_indicators = ["register", "impl Action"]
example_indicators = ["SelectAll", "Copy"]

[task_requirements.concern_indicators]
"action definition" = ["Action", "impl_actions"]
"keybinding" = ["keymap", "KeyBinding"]
```

## Metrics Reference

### Performance

| Metric | Description |
|--------|-------------|
| Search Latency | p50/p95/p99 for explore queries |
| Context Latency | p50/p95/p99 for context fetches |
| Total Time | End-to-end scenario time |

### Accuracy

| Metric | Target | Description |
|--------|--------|-------------|
| File Recall | >= 70% | % of expected files found |
| Symbol Recall | >= 70% | % of expected symbols found |
| Noise Ratio | <= 25% | % of results matching noise patterns |
| MRR | >= 0.5 | Mean reciprocal rank |

### Exploration Quality

| Metric | Target | Description |
|--------|--------|-------------|
| Convergence Rate | >= 70% | Discoveries plateau early (good) |
| Context Bloat | <= 30% | Empty/useless context calls |
| Dead End Ratio | <= 20% | Steps with no discoveries |
| File Diversity | >= 60% | Unique files in top-5 |

### Incremental Indexing

| Metric | Target | Description |
|--------|--------|-------------|
| Time per file | < 200ms | Reindex latency for changed files |
| Detection accuracy | >= 90% | Changed files correctly detected |
| False positive rate | < 10% | Unchanged files incorrectly reprocessed |

### File Watcher

| Metric | Target | Description |
|--------|--------|-------------|
| E2E latency | < 200ms | Time from file save to searchable |
| p95 E2E latency | < 500ms | 95th percentile latency |
| Debounce accuracy | 100% | Rapid changes coalesced correctly |
| Gitignore respect | 100% | Ignored files not indexed |
| Resource leaks | 0 | No file descriptor leaks |

## Diagnostic Reports

When metrics fail, the JSON report includes diagnostics explaining why:

```json
{
  "diagnostics": {
    "convergence": {
      "empty_steps": [2, 5],
      "discovery_pattern": "early_plateau",
      "recommendation": "Steps 2, 5 found nothing. Try broader queries."
    },
    "recall": {
      "in_index_not_retrieved": ["action.rs"],
      "recommendation": "File indexed but not retrieved - improve query terms."
    },
    "bloat": {
      "over_expanded_steps": [{"step": 1, "requested": 5, "useful": 2}],
      "recommendation": "Use expand_top=2 for focused queries."
    }
  }
}
```

## Troubleshooting

### "Daemon not running"

```bash
ccengram daemon  # Start daemon first
cargo run -p benchmark -- run
```

### Low recall

1. Check `diagnostics.recall.in_index_not_retrieved` - files indexed but queries miss them
2. Improve query terms to be more natural/behavioral
3. Verify daemon has indexed the repository

### High noise

1. Add more specific noise patterns
2. Check if test files appear in top results

### Low convergence

1. Check which steps found nothing (`empty_steps`)
2. Are queries too specific (using jargon)?
3. Are queries too broad (returning everything)?

### High context bloat

1. Reduce `expand_top` for focused queries
2. Check for redundant context fetches

## Storage and Cleanup

Benchmarks create data in three locations:

| Location | Contents | Size | Clean Command |
|----------|----------|------|---------------|
| `~/.cache/ccengram-bench/repos/` | Downloaded repos | ~2GB | `benchmark clean --all` |
| `~/.local/share/ccengram/projects/` | CCEngram indexes | ~500MB | `ccengram projects clean-all` |
| `./benchmark-results/` | Reports | ~1MB | `rm -rf ./benchmark-results` |

### Output Files

| Command | JSON Output | Markdown Output |
|---------|-------------|-----------------|
| `run` | `results.json` | `report.md` |
| `index-perf` | `indexing.json` | `indexing.md` |
| `incremental-perf` | `incremental.json` | `incremental.md` |
| `watcher-perf` | `watcher.json` | `watcher.md` |
| `large-file-perf` | `large_file.json` | - |

**Full cleanup:**
```bash
cargo run -p benchmark -- clean --all      # Remove downloaded repos
ccengram projects clean-all                 # Remove CCEngram indexes
rm -rf ./benchmark-results                  # Remove reports
```
