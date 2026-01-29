# Handler System Architecture

This document describes CCEngram's handler architecture: a service-oriented design integrated with the actor system for clean separation of concerns.

## Overview

The handler system follows a three-layer architecture:

1. **Handlers** (thin adapters) - Parse requests, build responses
2. **Services** (business logic) - Testable domain operations
3. **Utilities** (shared patterns) - Common infrastructure

This design achieves proper separation of concerns, enables unit testing of business logic, and integrates cleanly with the actor model.

---

## Module Structure

```
crates/backend/src/
├── actor/
│   └── project.rs          # ProjectActor calls services
│
├── server.rs               # IPC server setup, routing to ProjectRouter which calls ProjectActor
│
└── service/                # Business logic layer
    ├── mod.rs              # Re-exports, SearchContext
    │
    ├── util/               # Shared utilities
    ├── memory/
    │   ├── mod.rs          # MemoryService, MemoryContext
    │   ├── search.rs       # Search with ranking
    │   ├── ranking.rs      # Ranking algorithms
    │   ├── dedup.rs        # Duplicate detection
    │   └── lifecycle.rs    # Reinforce, deemphasize, supersede
    ├── code/
    │   ├── mod.rs          # CodeService, CodeContext
    │   ├── search.rs       # Code search with symbol boosting
    │   ├── expansion.rs    # Query expansion
    │   ├── context.rs      # Callers, callees, siblings
    │   └── stats.rs        # Code statistics
    └── explore/
        ├── mod.rs          # ExploreService
        ├── unified.rs      # Cross-domain parallel search
        └── suggestions.rs  # Suggestion generation

```

---

## Core Abstractions

### Context Structs

Each service domain has a context struct that bundles dependencies:

```rust
pub struct MemoryContext<'a> {
    pub db: &'a ProjectDb,
    pub embedding: &'a dyn EmbeddingProvider,
    pub project_id: &'a str,
}

pub struct CodeContext<'a> {
    pub db: &'a ProjectDb,
    pub embedding: &'a dyn EmbeddingProvider,
}

pub struct ExploreContext<'a> {
    pub db: &'a ProjectDb,
    pub embedding: &'a dyn EmbeddingProvider,
}
```

### ServiceError

Unified error type for all service operations:

```rust
pub enum ServiceError {
    NotFound { item_type: &'static str, id: String },
    Ambiguous { prefix: String, count: usize },
    Validation(String),
    Database(DbError),
    Embedding(String),
    Project(String),
    Timeout,
}
```

ServiceError maps to JSON-RPC error codes:

- `Validation` → `-32602` (invalid params)
- All others → `-32000` (server error)

### Resolver

Generic ID/prefix resolution for all entity types:

```rust
// Resolve memory by UUID or prefix
let memory = Resolver::memory(&db, "abc123").await?;

// Resolve code chunk
let chunk = Resolver::code_chunk(&db, id_or_prefix).await?;

// Resolve any entity type (auto-detect)
let entity = Resolver::any(&db, id_or_prefix).await?;
```

### FilterBuilder

Safe SQL filter construction (prevents injection):

```rust
let filter = FilterBuilder::new()
    .exclude_inactive(include_superseded)
    .add_eq_opt("sector", sector.as_deref())
    .add_min_opt("salience", min_salience)
    .build();
```

---

## Service Layer Patterns

### Memory Service

The memory service handles all memory operations:

- **Search**: Vector search with text fallback, post-search ranking
- **Lifecycle**: Reinforce, deemphasize, supersede operations
- **Dedup**: Multi-level duplicate detection (hash → simhash → jaccard)

Key design decisions:

- Search does NOT auto-reinforce (side effects are explicit)
- Ranking is configurable via `RankingConfig`
- Dedup thresholds are configurable via `DedupConfig`

### Code Service

The code service handles code search and context:

- **Search**: Query expansion, symbol boosting, importance scoring
- **Context**: Callers, callees, siblings, related memories
- **Stats**: Language breakdown, index health scoring

Query expansion consolidates programming term synonyms into a single source of truth (used by both search and suggestions).

### Explore Service

The explore service provides unified cross-domain search:

- **Unified Search**: Parallel search across code, memory, docs via `tokio::join!`
- **Suggestions**: Query suggestions based on expansion maps and content analysis
- **Formatting**: Response formatting utilities

---

## Handler Integration

### ProjectActor as Handler Host

The `ProjectActor` owns project state and delegates to services:

```rust
impl ProjectActor {
    async fn handle_memory(&self, req: MemoryRequest) -> ProjectActorResponse {
        let ctx = self.memory_context();
        let result = match req {
            MemoryRequest::Search(p) => memory::search(&ctx, p).await,
            MemoryRequest::Add(p) => memory::add(&ctx, p).await,
            // ...
        };
        match result {
            Ok(data) => ProjectActorResponse::Done(data),
            Err(e) => ProjectActorResponse::error(e.code(), e.to_string()),
        }
    }
}
```

### Embedding Cache

The `ProjectActor` maintains a per-project embedding cache:

- `moka::future::Cache` with 1000 entry capacity
- 5-minute TTL for cached embeddings
- `get_embedding()` method with cache-first lookup

### Handler Responsibilities

Handlers are thin adapters that:

1. Parse request arguments (JSON → typed struct)
2. Extract project path from arguments
3. Create service context
4. Call service method
5. Build response from service result

Handlers do NOT contain business logic.

---

## Adding New Functionality

### Adding a Service Method

1. Add the method to the appropriate service module
2. Use the domain's context struct for dependencies
3. Return `Result<T, ServiceError>`
4. Add unit tests in the service module

### Adding a Handler Endpoint

1. Define the request/response types in `ipc/types/`
2. Add the handler method (parse → context → service → response)
3. Wire the handler in `ProjectActor::handle_*`

### Utility Guidelines

- Use `Resolver` for any ID/prefix lookups
- Use `FilterBuilder` for constructing SQL filters
- Use `ServiceError` for all error returns
- Implement `From<DbError>` for automatic conversion

---

## Design Principles

1. **Handlers are thin** - No business logic, just request/response transformation
2. **Services are testable** - Pure functions with injected dependencies
3. **Side effects are explicit** - No hidden writes during reads
4. **Single source of truth** - Query expansion, ranking, etc. in one place
5. **Actor owns state** - Embedding cache, DB connection per project
6. **Errors are typed** - ServiceError with meaningful codes and messages
