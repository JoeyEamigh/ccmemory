# Agent Preferences for Explore Output

This document captures the reasoning behind explore tool output design choices, optimized for LLM agent consumption.

## Problem Statement

When an LLM agent uses the `explore` tool, it receives search results and expanded context. The output must help the agent:
1. Quickly evaluate relevance without reading full code
2. Navigate to related code efficiently
3. Avoid context window bloat from large chunks

## Design Decisions

### 1. Signatures in Caller/Callee Info

**Problem:** When viewing callers/callees, agents see only a truncated preview and must make another `context` call to understand what the function does.

**Solution:** Include the function signature in `CallInfo`.

**Rationale:**
- Signatures contain type information (parameters, return type) that immediately signals relevance
- A signature like `fn search(db: &ProjectDb, query: &str, limit: usize) -> Vec<Result>` tells the agent far more than a preview like `let results = db.query(...)`
- Reduces round-trips to the `context` tool

### 2. Adaptive Content Expansion

**Problem:** Large code chunks (100+ lines) bloat context without proportionally increasing usefulness. A 200-line function body is rarely needed in full during exploration.

**Solution:** Truncate chunks >80 lines to signature + first 20 lines + truncation indicator.

**Rationale:**
- First 20 lines usually contain the function signature, docstring, and initial logic
- The truncation message points agents to the `context` tool for full content
- Keeps expanded context focused on structural/navigational information
- 80-line threshold balances completeness vs. verbosity (most semantic units are <80 lines)

**Constants:**
- `EXPANSION_LINE_THRESHOLD = 80` - chunks larger than this get truncated
- `EXPANSION_PREVIEW_LINES = 20` - lines to show when truncated

### 3. Exposing Depth Parameter

**Problem:** Context depth was hardcoded to 5, preventing agents from requesting more/fewer related items.

**Solution:** Expose `depth` parameter in the `explore` tool IPC.

**Rationale:**
- Different queries benefit from different depths
- Broad exploration: lower depth (2-3) to avoid noise
- Deep investigation: higher depth (7-10) to find all connections
- Default remains 5 for backward compatibility

### 4. UUIDv4 for Code Chunks (vs UUIDv7)

**Problem:** UUIDv7 uses timestamp prefixes, so chunks indexed close in time share ID prefixes. When an agent uses the `context` tool with an 8-char prefix, collisions can occur.

**Solution:** Use UUIDv4 (random) instead of UUIDv7 for code chunks.

**Rationale:**
- UUIDv4's random distribution means 8-char prefixes are effectively unique (~1 in 4 billion collision chance)
- UUIDv7's time-ordering benefit isn't valuable for code chunks:
  - Chunks are recreated on re-index anyway
  - No queries rely on chunk creation order
- Memories still use UUIDv7 where timeline ordering is semantically meaningful

**Tradeoff:** Lost time-ordering in IDs, but this wasn't being used for code chunks.

## Output Format Philosophy

The explore tool output is designed for **progressive disclosure**:

1. **Result list:** ID, type, file, lines, score, semantic metadata (definition_kind, signature, docstring)
2. **Hints:** Counts of callers/callees/siblings/memories - tells agent what's available without fetching it
3. **Expanded context (top N):** Full content (or adaptive preview), callers, callees, siblings, related memories
4. **Context tool:** On-demand full detail for any ID

This layering lets agents decide how deep to go based on initial relevance signals, minimizing wasted context.
