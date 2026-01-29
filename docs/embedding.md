# Embedding System

This document explains the embedding system used in CCEngram, including the design decisions and configuration options.

## Overview

CCEngram uses **qwen3-embedding** (8B parameter model) producing **4096-dimensional embeddings** for both code and memory search. The embedding system is designed around two key principles:

1. **Trust the model**: Modern embedding models like qwen3-embedding understand semantic relationships between programming concepts. They know "mutex" relates to "lock", "auth" relates to "jwt", etc. without being explicitly told.

2. **Pure vector search**: Instead of relying on hardcoded synonym dictionaries or SQL LIKE queries, we use vector similarity search exclusively. This provides better recall, performance, and maintainability.

## EmbeddingMode

The embedding system distinguishes between two modes:

### Document Mode

Used when **indexing** content (code chunks, memories, documents). Text is embedded as-is without any instruction prefix.

```rust
// Indexing a code chunk
let embedding = provider.embed(chunk.content, EmbeddingMode::Document).await?;
```

### Query Mode

Used when **searching** for content. For instruction-aware models like qwen3-embedding, queries are formatted with an instruction prefix that tells the model the retrieval task.

```rust
// Searching for code
let embedding = provider.embed(query, EmbeddingMode::Query).await?;
```

The formatted query looks like:

```
Instruct: Given a code search query, retrieve relevant code snippets and documentation that match the query
Query:authentication jwt
```

This instruction format is specific to qwen3-embedding. The model uses the instruction to understand the retrieval task and produce better query embeddings.

## Configuration

The query instruction is configurable in `ccengram.toml`:

```toml
[embedding]
# Optional instruction for query mode (qwen3-embedding style)
# Set to empty string "" to disable instruction formatting
query_instruction = "Given a code search query, retrieve relevant code snippets and documentation that match the query"
```

### When to disable

If using an embedding model that doesn't support instruction-based retrieval (e.g., older sentence-transformers models), set `query_instruction = ""` to embed queries as raw text.

## Why Pure Semantic Search?

### The Problem with Hardcoded Expansion

Previously, code search used a 300+ line synonym dictionary:

```rust
("auth", &["authentication", "authorization", "login", "session", "token", "jwt", "oauth"...])
```

This approach has several problems:

1. **Limited coverage**: Misses domain-specific terms in the user's codebase. "login flow" won't expand to "OAuth callback handler" even if they're semantically related.

2. **No confidence weighting**: Treats all expansions equally. Expanding "auth" to 10 terms adds noise when only 2 are relevant.

3. **Maintenance burden**: Requires manual updates as terminology evolves.

4. **Performance**: Expanded queries can be longer, slower to embed.

### The Solution

The 4096-dimensional embedding space naturally encodes semantic relationships:

- The vector for "auth" is close to "authentication", "jwt", "oauth"
- The vector for "mutex" is close to "lock", "concurrent", "sync"
- The vector for "LTV" is close to "lifetime value" (domain knowledge)

By trusting the embedding model, we get:

- **Better recall**: Finds domain-specific relationships the synonym map missed
- **Weighted similarity**: Closer vectors = stronger relationship
- **Zero maintenance**: The model already knows these relationships
- **Domain adaptability**: Works with any codebase terminology

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    EmbeddingProvider trait                   │
├─────────────────────────────────────────────────────────────┤
│  embed(text, mode) -> Vec<f32>                              │
│  embed_batch(texts, mode) -> Vec<Vec<f32>>                  │
└─────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ OpenRouterProv. │ │  OllamaProvider │ │ RateLimitedProv │
│ (Cloud API)     │ │ (Local)         │ │ (Wrapper)       │
└─────────────────┘ └─────────────────┘ └─────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ ResilientProv.  │
                    │ (Retries/Split) │
                    └─────────────────┘
```

### Provider Layers

1. **Base providers** (OpenRouter, Ollama): Handle API communication and instruction formatting
2. **RateLimitedProvider**: Wraps a provider with rate limiting to avoid API throttling
3. **ResilientProvider**: Wraps a provider with retry logic, exponential backoff, and batch splitting on failure

## Search Flow

```
Query: "how does auth work"
         │
         ▼
┌─────────────────────────┐
│   Intent Detection      │
│   (logging only)        │
└─────────────────────────┘
         │
         ▼ "how does auth work" (unchanged)
┌─────────────────────────┐
│   format_for_embedding  │
│   (Query mode)          │
└─────────────────────────┘
         │
         ▼ "Instruct: Given a code search...\nQuery:how does auth work"
┌─────────────────────────┐
│   Embedding API         │
└─────────────────────────┘
         │
         ▼ [f32; 4096]
┌─────────────────────────┐
│   Vector Search         │
│   (LanceDB)             │
└─────────────────────────┘
         │
         ▼ Vec<(CodeChunk, distance)>
┌─────────────────────────┐
│   Ranking               │
│   - semantic_weight     │
│   - symbol_weight       │
│   - importance_weight   │
└─────────────────────────┘
         │
         ▼ Vec<CodeItem>
```

## Ranking Weights

Search results are ranked using a weighted combination of signals:

| Signal | Weight | Description |
|--------|--------|-------------|
| Semantic | 0.55 | Vector similarity (primary signal) |
| Symbol | 0.30 | Exact/partial matches on symbols, definition names |
| Importance | 0.15 | Visibility (pub > private) |

The semantic weight is higher because we now rely on the embedding model for concept matching rather than hardcoded expansion.

## References

- [qwen3-embedding documentation](https://huggingface.co/Alibaba-NLP/qwen3-embedding-8b)
- `crates/backend/src/embedding/mod.rs` - EmbeddingMode and provider trait
- `crates/backend/src/embedding/openrouter.rs` - OpenRouter implementation
- `crates/backend/src/embedding/ollama.rs` - Ollama implementation
- `crates/backend/src/service/code/search.rs` - Code search with ranking
