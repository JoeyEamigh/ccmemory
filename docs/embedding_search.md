# Embedding-First Search Design

CCEngram uses qwen3-embedding (8B parameters) to produce 4096-dimensional embeddings for all indexed content. This document explains the design philosophy behind our search system and why we rely on vector similarity rather than traditional text matching.

## The Core Principle

**Trust the embedding model.**

High-dimensional embeddings naturally encode semantic relationships. The model understands that "auth" relates to "authentication", "JWT", and "OAuth" without being told. It knows "mutex" connects to "lock", "concurrent", and "sync". These relationships emerge from training on vast amounts of code and documentation.

When we layer hardcoded synonym dictionaries or SQL LIKE queries on top of vector search, we're second-guessing the model with inferior heuristics. The embedding model has seen more code than any human could curate into a synonym map.

## Why Vector Search Over Text Matching

### Semantic Recall

A memory stating "The authentication system verifies JSON Web Tokens before granting access" won't match a LIKE query for the symbol `validate_token`. But in embedding space, these concepts are close neighbors. Vector search finds what users mean, not just what they typed.

### Domain Adaptation

Hardcoded synonyms miss domain-specific relationships. A synonym map might know "auth" relates to "login", but it won't know that in your codebase, "LTV" means "lifetime value" or that "the coordinator" refers to a specific service. The embedding model learns these relationships from context.

### Performance

LIKE queries with wildcards (`%pattern%`) require full table scans. Vector search uses indexes. For a codebase with thousands of chunks and hundreds of memories, this matters.

### Maintenance

A 300-line synonym map requires constant human curation. It's always incomplete and sometimes wrong. Embeddings handle this automatically.

## Design Decisions

### Related Content Discovery

When finding memories related to a code chunk, we use the chunk's embedding to search the memory table directly. This replaces the previous approach of iterating through symbols and running LIKE queries for each one.

The chunk's `embedding_text` already captures its semantic content—the function signature, docstring, and context. One vector search against this embedding finds all semantically related memories regardless of whether they mention specific symbol names.

### Query Processing

User queries go directly to the embedding model without synonym expansion. Intent detection still cleans queries ("how does X work" → "X"), but we don't inflate "auth" into ten related terms before embedding.

The embedding model handles this naturally. Embedding "auth" produces a vector already close to authentication-related concepts. Pre-expanding the query just adds noise and can dilute the original intent.

### Metadata Filtering

LanceDB supports pre-filtering during vector search. We filter by:

- **visibility**: pub, private, pub(crate)
- **chunk_type**: Function, Class, Module, Block
- **language**: rust, typescript, python, etc.
- **caller_count**: Centrality metric for importance

Filtering before vector search is more efficient than filtering after. It also produces more relevant results—if someone searches for public APIs, they shouldn't see private helpers even if those are semantically similar.

### Cross-Domain Search

Code chunks and memories exist in the same embedding space. A memory about database migrations lives near code that performs migrations. This enables bidirectional discovery:

- Exploring a code chunk shows related memories (decisions, context, history)
- Exploring a memory shows related code (implementations, examples)

This emerges naturally from using the same embedding model for both domains.

### Confidence Signals

Vector distance indicates confidence. A distance of 0.1 means high confidence; 0.6 means the match might be noise. We use this for:

- **Adaptive limiting**: When top results are very confident, return fewer results rather than padding with weak matches
- **Quality warnings**: When nothing is close (best distance > 0.5), flag that results may not be relevant
- **Result metadata**: Each result includes its confidence score so consumers can make informed decisions

## What We Don't Do

### Hybrid Search

Some systems combine BM25 (keyword matching) with vector search. We don't. Our embeddings are good enough that adding keyword matching adds complexity without improving results. If a user needs exact string matching, they can use grep.

### Query Expansion

We removed the 300-line synonym dictionary. The embedding model handles semantic relationships better than any manually curated list. Keeping both would mean maintaining two systems that do the same thing, with the worse one occasionally overriding the better one.

### Re-ranking with LLMs

Some systems use an LLM to re-rank vector search results. This adds latency and cost for marginal improvement. Our ranking combines vector similarity with structural signals (visibility, caller count) which is fast and effective.

## Key Files

| Path | Purpose |
|------|---------|
| `service/code/search.rs` | Code search with ranking |
| `service/memory/search.rs` | Memory search |
| `service/explore/context.rs` | Cross-domain context retrieval |
| `service/util/filter.rs` | Filter building for vector search |
| `context/files/code/chunker.rs` | Creates enriched `embedding_text` |
| `db/code/codes.rs` | Code chunk DB operations |
| `db/memory/memories.rs` | Memory DB operations |
