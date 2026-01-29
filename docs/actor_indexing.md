# Streaming Pipeline Architecture

The file indexing system uses a multi-stage streaming pipeline with backpressure. This design replaces a naive `join_all` approach that created memory pressure and sync points.

## Design Goals

The pipeline addresses several architectural problems:

- **Backpressure propagation**: When downstream stages are saturated, upstream stages naturally block
- **Bounded memory**: Fixed buffer sizes prevent unbounded memory growth during large indexing jobs
- **Unified gitignore handling**: Single source of truth for file filtering
- **Code/document unification**: Common `FileIndexer` trait for both code and document indexing
- **Low-latency incremental updates**: Watcher can bypass the scanner and inject directly into the reader stage

## Pipeline Stages

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Scanner   │───▶│   Reader    │───▶│   Parser    │───▶│  Embedder   │───▶│   Writer    │
│  (files)    │    │  (content)  │    │  (chunks)   │    │  (batches)  │    │ (DB flush)  │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
     256               128               256                64                 flush
    paths            files             chunks           embedded            on size/time
```

Each stage has a bounded channel. When a channel fills, the sending stage blocks until space is available.

### Scanner

Emits file paths from a provided list. For bulk indexing, this is the full file list from a directory scan. Sends `PipelineFile::Done` when exhausted.

### Reader (Worker Pool)

Multiple workers (8-16) read file contents in parallel. I/O-bound, so more workers than CPU cores is beneficial. Workers share a receiver via `Arc<Mutex<Receiver>>` for work-stealing. Outputs `PipelineContent` messages.

### Parser (Worker Pool)

CPU-bound stage with workers matching core count. Each worker owns a `Chunker` instance. Performs:

- AST-aware chunking via tree-sitter
- Incremental parsing when old content is available
- Embedding reuse lookup (queries DB for existing embeddings by content hash)
- Outputs `PipelineChunks` with indices of chunks needing new embeddings

### Embedder (Concurrent Batches)

The embedder fires embedding requests without blocking. Multiple batches can be in-flight simultaneously.

Batches fire when:

- Size threshold reached (64 texts)
- Time threshold reached (10-50ms depending on mode)
- Pipeline ending (final flush)

This design maximizes throughput because the rate limiter becomes the bottleneck, not serialization. The parser continues feeding while embeddings are in progress.

Failed batches use zero vectors as fallback and log a warning.

### Writer (Accumulator)

Single task that accumulates `ProcessedFile` results until a threshold:

- Size threshold: 500 chunks (bulk) or 50 chunks (incremental)
- Time threshold: 1s (bulk) or 100ms (incremental)

On flush, performs batch delete of old chunks followed by batch insert of new chunks. Single DB transaction per flush rather than per-file.

## Pipeline Messages

Messages flow between stages with explicit `Done` variants for clean shutdown:

| Type               | Contents                                                              | Purpose           |
| ------------------ | --------------------------------------------------------------------- | ----------------- |
| `PipelineFile`     | path, relative path, optional old content                             | Scanner → Reader  |
| `PipelineContent`  | relative path, content, optional old content                          | Reader → Parser   |
| `PipelineChunks`   | relative path, chunks, existing embeddings, indices needing embedding | Parser → Embedder |
| `PipelineEmbedded` | batch of `ProcessedFile`                                              | Embedder → Writer |

The `old_content` field enables incremental parsing - tree-sitter can reuse parse trees for unchanged portions.

## Configuration Modes

`PipelineConfig` provides presets for different workloads:

### Bulk Mode (>100 files)

Large buffers and long timeouts optimized for throughput:

- Scanner buffer: 256
- Reader buffer: 128
- Embedding batch timeout: 50ms
- DB flush: 500 chunks or 1s

### Incremental Mode (≤100 files)

Small buffers and short timeouts optimized for latency:

- Scanner buffer: 16
- Reader buffer: 8
- Embedding batch timeout: 10ms
- DB flush: 50 chunks or 100ms

`PipelineConfig::auto(file_count)` selects the appropriate mode automatically.

## FileIndexer Trait

Unified interface for code and document indexing:

```rust
pub trait FileIndexer: Send + Sync {
    type Chunk: Clone + Send;
    type Metadata: Clone + Send;

    fn scan_file(&self, path: &Path, root: &Path) -> Option<Self::Metadata>;
    fn chunk_file(&mut self, content: &str, metadata: &Self::Metadata, old_content: Option<&str>) -> Result<Vec<Self::Chunk>, FileIndexError>;
    fn prepare_embedding_text(&self, chunk: &Self::Chunk) -> String;
    fn cache_key(&self, chunk: &Self::Chunk) -> Option<String>;
    fn can_reuse_embedding(&self, chunk: &Self::Chunk, existing: &HashMap<String, Vec<f32>>) -> Vec<f32>;
}
```

### CodeIndexer

Wraps the `Chunker` for AST-aware code chunking. Features:

- Language detection from file extension
- Incremental parsing support
- Embedding reuse via content hash comparison

### DocumentIndexer

Sentence-aware chunking for markdown, text, rst, org, and similar formats. Configurable chunk size and overlap. Does not reuse embeddings since documents tend to change more holistically.

## Watcher Integration

The file watcher can operate in two modes:

### Indexer Mode (Traditional)

Sends `IndexJob` messages to the `IndexerActor`. The actor then runs a pipeline for the batch.

### Pipeline Mode (Low-Latency)

Injects `PipelineFile` directly into the reader stage, bypassing the scanner. Provides sub-200ms latency for single-file changes.

The watcher maintains:

- **Content cache**: LRU cache (1000 files, 512KB max) of old file content for incremental parsing
- **Gitignore filtering**: Single `GitignoreBuilder` combining `.gitignore` and `.ccengramignore`
- **Event coalescing**: Merges rapid successive events (Create+Modify → Create, Delete+Create → Modified)

For deletes and renames, the watcher sends `WriteOperation` messages directly to the writer stage.

## Embedding Provider Features

### Rate Limit Refunds

The rate limiter issues tokens that can be refunded when requests fail before reaching the provider's rate limiter:

| Error Type         | Refund? | Reason                           |
| ------------------ | ------- | -------------------------------- |
| Network error      | Yes     | Never reached provider           |
| Timeout            | Yes     | Probably didn't complete         |
| 5xx errors         | Yes     | Server error before rate limiter |
| 429 (rate limited) | No      | Provider counted it              |
| 4xx errors         | No      | Request was processed            |

### Oversized Chunk Protection

Text validation truncates chunks exceeding the embedding model's context limit (estimated at ~8000 tokens). Uses a conservative 4 chars/token estimate. Truncation is logged for debugging.

## Performance Characteristics

### Bulk Indexing (1000 files)

- Memory bounded by reader buffer (128 files max in RAM)
- ~15-20 embedding API batches instead of 1000 individual calls
- ~2 DB batch inserts instead of 1000
- No sync points - files stream continuously

### Incremental Updates (1-3 files)

- End-to-end latency <200ms for single file
- Only re-embeds changed chunks (content hash comparison)
- Small buffers minimize overhead

### Embedder Throughput

With 64 texts per batch and rate limiting at 50 requests per 10 seconds, theoretical maximum is ~3200 texts per 10 seconds. Concurrent batches in-flight achieve 2-3x throughput compared to sequential blocking.

## Key Files

| File                                         | Purpose                                 |
| -------------------------------------------- | --------------------------------------- |
| `crates/backend/src/actor/pipeline.rs`       | Pipeline stages and orchestration       |
| `crates/backend/src/actor/indexer.rs`        | `IndexerActor` and `PipelineConfig`     |
| `crates/backend/src/actor/message.rs`        | Pipeline message types                  |
| `crates/backend/src/actor/watcher.rs`        | File watcher with dual-mode sink        |
| `crates/backend/src/context/files/mod.rs`    | `FileIndexer` trait and implementations |
| `crates/backend/src/embedding/validation.rs` | Oversized chunk protection              |
| `crates/backend/src/embedding/rate_limit.rs` | Rate limiter with refund tokens         |
