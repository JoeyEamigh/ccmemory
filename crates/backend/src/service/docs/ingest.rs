//! Document ingestion service using the streaming pipeline.
//!
//! Documents are indexed through the same unified pipeline as code,
//! using the `Indexer` which handles both code and document files.

use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::{Duration, Instant},
};

use ignore::WalkBuilder;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use uuid::Uuid;

use crate::{
  actor::{indexer::PipelineConfig, message::IndexProgress, pipeline::run_pipeline},
  context::files::Indexer,
  db::ProjectDb,
  embedding::EmbeddingProvider,
  ipc::types::docs::DocsIngestResult,
  service::util::ServiceError,
};

// ============================================================================
// Progress Types
// ============================================================================

/// Progress update for document ingestion.
#[derive(Debug, Clone)]
pub struct IngestProgress {
  /// Number of files processed so far
  pub processed: usize,
  /// Total number of files to process
  pub total: usize,
  /// Current file being processed (if any)
  pub current_file: Option<String>,
  /// Number of chunks created (populated in final progress update)
  pub chunks_created: usize,
  /// Number of documents ingested
  pub docs_ingested: usize,
}

impl IngestProgress {
  /// Create a new progress update
  pub fn new(processed: usize, total: usize) -> Self {
    Self {
      processed,
      total,
      current_file: None,
      chunks_created: 0,
      docs_ingested: 0,
    }
  }

  /// Set the current file being processed
  pub fn with_current_file(mut self, file: impl Into<String>) -> Self {
    self.current_file = Some(file.into());
    self
  }

  /// Set the number of chunks created
  pub fn with_chunks_created(mut self, count: usize) -> Self {
    self.chunks_created = count;
    self
  }

  /// Set the number of documents ingested
  pub fn with_docs_ingested(mut self, count: usize) -> Self {
    self.docs_ingested = count;
    self
  }

  /// Calculate completion percentage
  pub fn percent(&self) -> u8 {
    if self.total == 0 {
      100
    } else {
      ((self.processed as f64 / self.total as f64) * 100.0).min(100.0) as u8
    }
  }

  #[cfg(test)]
  /// Check if processing is complete
  pub fn is_complete(&self) -> bool {
    self.processed >= self.total
  }
}

// ============================================================================
// Scan Types
// ============================================================================

/// Result of scanning for document files.
#[derive(Debug, Clone)]
pub struct ScanResult {
  /// Files found that should be ingested
  pub files: Vec<PathBuf>,
  /// Total bytes across all files
  pub total_bytes: u64,
  /// Time taken to scan
  pub duration: Duration,
}

/// Parameters for scanning.
#[derive(Debug, Clone)]
pub struct ScanParams {
  /// Maximum file size to include
  pub max_file_size: u64,
  /// File extensions to include
  pub extensions: Vec<String>,
}

impl Default for ScanParams {
  fn default() -> Self {
    Self {
      max_file_size: 5 * 1024 * 1024, // 5MB default
      extensions: vec![
        "md".to_string(),
        "markdown".to_string(),
        "txt".to_string(),
        "text".to_string(),
        "rst".to_string(),
        "adoc".to_string(),
        "asciidoc".to_string(),
        "org".to_string(),
      ],
    }
  }
}

// ============================================================================
// Ingest Types
// ============================================================================

/// Parameters for document ingestion.
#[derive(Debug, Clone)]
pub struct IngestParams {
  /// Directory to scan for documents (relative to root)
  pub directory: Option<String>,
  /// Single file to ingest (can be absolute or relative)
  pub file: Option<String>,
  /// Project ID for document chunks
  pub project_id: Uuid,
  /// Project root directory
  pub root: PathBuf,
}

/// Result of ingesting documents.
#[derive(Debug, Clone)]
pub struct IngestResult {
  /// Status message
  pub status: String,
  /// Number of files scanned
  pub files_scanned: usize,
  /// Number of files successfully ingested
  pub files_ingested: usize,
  /// Number of chunks created
  pub chunks_created: usize,
  /// Number of files that failed
  pub failed_files: usize,
  /// Time spent scanning
  pub scan_duration: Duration,
  /// Time spent ingesting
  pub ingest_duration: Duration,
  /// Total time
  pub total_duration: Duration,
  /// Files processed per second
  pub files_per_second: f64,
  /// Bytes processed
  pub bytes_processed: u64,
  /// Total bytes
  pub total_bytes: u64,
  /// Individual file results
  pub results: Vec<DocsIngestResult>,
}

// ============================================================================
// Scanning
// ============================================================================

/// Scan a directory for document files.
///
/// # Arguments
/// * `root` - Root directory to scan
/// * `params` - Scan parameters
///
/// # Returns
/// * `ScanResult` - Files found, total bytes, and scan duration
pub fn scan_directory(root: &Path, params: &ScanParams) -> ScanResult {
  let start = Instant::now();
  let mut files: Vec<PathBuf> = Vec::new();
  let mut total_bytes: u64 = 0;

  let walker = WalkBuilder::new(root)
    .hidden(true) // Respect hidden files (.gitignore default)
    .git_ignore(true) // Respect .gitignore
    .git_global(true) // Respect global gitignore
    .git_exclude(true) // Respect .git/info/exclude
    .max_filesize(Some(params.max_file_size))
    .build();

  for entry in walker.flatten() {
    let path = entry.path();

    // Skip directories
    if path.is_dir() {
      continue;
    }

    // Only include files with supported extensions
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
      let ext_lower = ext.to_lowercase();
      if params.extensions.iter().any(|e| e.to_lowercase() == ext_lower) {
        // Track file size
        if let Ok(metadata) = std::fs::metadata(path) {
          total_bytes += metadata.len();
        }
        files.push(path.to_path_buf());
      }
    }
  }

  ScanResult {
    files,
    total_bytes,
    duration: start.elapsed(),
  }
}

/// Check if a file is a valid document file.
fn is_document_file(path: &Path, params: &ScanParams) -> bool {
  if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
    let ext_lower = ext.to_lowercase();
    params.extensions.iter().any(|e| e.to_lowercase() == ext_lower)
  } else {
    false
  }
}

// ============================================================================
// Ingest Context
// ============================================================================

/// Context for document ingestion with Arc-wrapped resources.
///
/// This is separate from `DocsContext` because the streaming pipeline
/// requires Arc-wrapped resources for sharing across async tasks.
pub struct IngestContext {
  /// Project database connection
  pub db: Arc<ProjectDb>,
  /// Embedding provider
  pub embedding: Arc<dyn EmbeddingProvider>,
}

impl IngestContext {
  /// Create a new ingest context
  pub fn new(db: Arc<ProjectDb>, embedding: Arc<dyn EmbeddingProvider>) -> Self {
    Self { db, embedding }
  }
}

// ============================================================================
// Ingestion
// ============================================================================

/// Ingest documents using the streaming pipeline.
///
/// Supports three modes:
/// 1. Single file (params.file is set) - Ingest one file
/// 2. Directory (params.directory is set) - Ingest all docs in a directory
/// 3. Default (neither set) - Ingest all docs from project root
///
/// This uses the same high-throughput pipeline as code indexing, with:
/// - Multi-stage processing with bounded channels
/// - Backpressure propagation
/// - Concurrent embedding batches
/// - Batched database writes via Indexer::store_chunks
///
/// # Arguments
/// * `ctx` - Ingest context with Arc-wrapped database and embedding provider
/// * `params` - Ingestion parameters
/// * `progress_tx` - Optional channel for progress updates
///
/// # Returns
/// * `Ok(IngestResult)` - Full ingestion result with stats
/// * `Err(ServiceError)` - If ingestion fails
pub async fn ingest(
  ctx: &IngestContext,
  params: IngestParams,
  progress_tx: Option<mpsc::Sender<IngestProgress>>,
) -> Result<IngestResult, ServiceError> {
  let start = Instant::now();
  let scan_params = ScanParams::default();

  // Handle single file ingest
  if let Some(ref file_path) = params.file {
    return ingest_single_file(ctx, &params, file_path, &scan_params, start).await;
  }

  // Handle directory/project ingest
  let scan_dir = params
    .directory
    .as_ref()
    .map(|d| params.root.join(d))
    .unwrap_or_else(|| params.root.clone());

  if !scan_dir.exists() {
    return Err(ServiceError::Validation(format!(
      "Directory does not exist: {}",
      scan_dir.display()
    )));
  }

  // Scan for files
  let scan_result = scan_directory(&scan_dir, &scan_params);
  let total_files = scan_result.files.len();

  debug!(
    files_found = total_files,
    scan_ms = scan_result.duration.as_millis() as u64,
    "Document scan complete"
  );

  if total_files == 0 {
    return Ok(IngestResult {
      status: "complete".to_string(),
      files_scanned: 0,
      files_ingested: 0,
      chunks_created: 0,
      failed_files: 0,
      scan_duration: scan_result.duration,
      ingest_duration: Duration::ZERO,
      total_duration: scan_result.duration,
      files_per_second: 0.0,
      bytes_processed: 0,
      total_bytes: 0,
      results: Vec::new(),
    });
  }

  // Create pipeline progress channel and adapter
  let (pipeline_progress_tx, mut pipeline_progress_rx) = mpsc::channel::<IndexProgress>(16);

  // Spawn progress adapter task if we have a progress channel
  let progress_handle = if let Some(ingest_tx) = progress_tx {
    Some(tokio::spawn(async move {
      while let Some(index_progress) = pipeline_progress_rx.recv().await {
        let mut ingest_progress = IngestProgress::new(index_progress.processed, index_progress.total)
          .with_chunks_created(index_progress.chunks_created)
          .with_docs_ingested(index_progress.processed);

        if let Some(ref file) = index_progress.current_file {
          ingest_progress = ingest_progress.with_current_file(file.clone());
        }

        let _ = ingest_tx.send(ingest_progress).await;
      }
    }))
  } else {
    drop(pipeline_progress_rx);
    None
  };

  // Configure pipeline for documents
  let config = PipelineConfig::from_index_config(
    &crate::domain::config::IndexConfig::default(),
    64,                // embedding batch size
    8192,              // context length
    total_files > 100, // bulk mode for large batches
  );

  // Run the pipeline with unified Indexer
  let pipeline_result = run_pipeline(
    Indexer::new(params.project_id),
    params.root.clone(),
    scan_result.files,
    ctx.db.clone(),
    ctx.embedding.clone(),
    config,
    Some(pipeline_progress_tx),
    CancellationToken::new(),
    None, // Documents don't track indexed_files metadata the same way
  )
  .await
  .map_err(|e| ServiceError::Internal(format!("Pipeline error: {}", e)))?;

  // Wait for progress adapter to finish
  if let Some(handle) = progress_handle {
    let _ = handle.await;
  }

  let ingest_duration = start.elapsed() - scan_result.duration;
  let total_duration = start.elapsed();

  let files_per_second = if total_duration.as_secs_f64() > 0.0 {
    total_files as f64 / total_duration.as_secs_f64()
  } else {
    0.0
  };

  Ok(IngestResult {
    status: "complete".to_string(),
    files_scanned: total_files,
    files_ingested: pipeline_result.files_processed,
    chunks_created: pipeline_result.chunks_indexed,
    failed_files: pipeline_result.errors.len(),
    scan_duration: scan_result.duration,
    ingest_duration,
    total_duration,
    files_per_second,
    bytes_processed: scan_result.total_bytes,
    total_bytes: scan_result.total_bytes,
    results: Vec::new(), // Pipeline doesn't track per-file results
  })
}

/// Ingest a single file using the pipeline.
async fn ingest_single_file(
  ctx: &IngestContext,
  params: &IngestParams,
  file_path: &str,
  scan_params: &ScanParams,
  start: Instant,
) -> Result<IngestResult, ServiceError> {
  // Resolve the file path
  let path = if Path::new(file_path).is_absolute() {
    PathBuf::from(file_path)
  } else {
    params.root.join(file_path)
  };

  if !path.exists() {
    return Err(ServiceError::Validation(format!(
      "File does not exist: {}",
      path.display()
    )));
  }

  if !path.is_file() {
    return Err(ServiceError::Validation(format!(
      "Path is not a file: {}",
      path.display()
    )));
  }

  // Check if it's a document file
  if !is_document_file(&path, scan_params) {
    return Err(ServiceError::Validation(format!(
      "File is not a recognized document type: {}",
      path.display()
    )));
  }

  // Get file size
  let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
  let scan_duration = start.elapsed();

  // Determine the root for relative path calculation
  // If the file is outside the project root, use its parent as the "root"
  let effective_root = if path.starts_with(&params.root) {
    params.root.clone()
  } else {
    path
      .parent()
      .map(|p| p.to_path_buf())
      .unwrap_or_else(|| params.root.clone())
  };

  // Configure pipeline for single file (smaller buffers)
  let config = PipelineConfig::from_index_config(
    &crate::domain::config::IndexConfig::default(),
    64,    // embedding batch size
    8192,  // context length
    false, // not bulk mode
  );

  // Run the pipeline with unified Indexer for this one file
  let pipeline_result = run_pipeline(
    Indexer::new(params.project_id),
    effective_root,
    vec![path],
    ctx.db.clone(),
    ctx.embedding.clone(),
    config,
    None, // No progress for single file
    CancellationToken::new(),
    None, // Documents don't track indexed_files metadata the same way
  )
  .await
  .map_err(|e| ServiceError::Internal(format!("Pipeline error: {}", e)))?;

  let ingest_duration = start.elapsed() - scan_duration;
  let total_duration = start.elapsed();

  if pipeline_result.files_processed == 0 {
    return Err(ServiceError::Validation(format!(
      "File was not recognized as a document: {}",
      file_path
    )));
  }

  Ok(IngestResult {
    status: "complete".to_string(),
    files_scanned: 1,
    files_ingested: pipeline_result.files_processed,
    chunks_created: pipeline_result.chunks_indexed,
    failed_files: pipeline_result.errors.len(),
    scan_duration,
    ingest_duration,
    total_duration,
    files_per_second: 1.0 / total_duration.as_secs_f64().max(0.001),
    bytes_processed: file_size,
    total_bytes: file_size,
    results: Vec::new(),
  })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_ingest_progress_percent() {
    let progress = IngestProgress::new(0, 100);
    assert_eq!(progress.percent(), 0);

    let progress = IngestProgress::new(50, 100);
    assert_eq!(progress.percent(), 50);

    let progress = IngestProgress::new(100, 100);
    assert_eq!(progress.percent(), 100);

    // Edge case: empty batch
    let progress = IngestProgress::new(0, 0);
    assert_eq!(progress.percent(), 100);
  }

  #[test]
  fn test_ingest_progress_is_complete() {
    let progress = IngestProgress::new(50, 100);
    assert!(!progress.is_complete());

    let progress = IngestProgress::new(100, 100);
    assert!(progress.is_complete());

    let progress = IngestProgress::new(101, 100);
    assert!(progress.is_complete());
  }

  #[test]
  fn test_is_document_file() {
    let params = ScanParams::default();

    assert!(is_document_file(Path::new("README.md"), &params));
    assert!(is_document_file(Path::new("docs/guide.txt"), &params));
    assert!(is_document_file(Path::new("CHANGELOG.MD"), &params)); // case insensitive

    assert!(!is_document_file(Path::new("main.rs"), &params));
    assert!(!is_document_file(Path::new("script.py"), &params));
    assert!(!is_document_file(Path::new("noext"), &params));
  }
}
