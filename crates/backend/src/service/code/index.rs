//! Code indexing service.
//!
//! Provides file scanning and indexing orchestration for code files.

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use ignore::WalkBuilder;
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
  actor::{handle::IndexerHandle, message::IndexProgress},
  domain::code::Language,
};

/// Result of scanning a directory for code files.
#[derive(Debug, Clone)]
pub struct ScanResult {
  /// Files found that should be indexed
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
}

impl Default for ScanParams {
  fn default() -> Self {
    Self {
      max_file_size: 1024 * 1024, // 1MB default
    }
  }
}

/// Result of indexing files.
#[derive(Debug, Clone)]
pub struct IndexResult {
  /// Status message
  pub status: String,
  /// Number of files scanned
  pub files_scanned: usize,
  /// Number of files successfully indexed
  pub files_indexed: usize,
  /// Number of chunks created
  pub chunks_created: usize,
  /// Number of files that failed
  pub failed_files: usize,
  /// Whether indexing resumed from checkpoint
  pub resumed_from_checkpoint: bool,
  /// Time spent scanning
  pub scan_duration: Duration,
  /// Time spent indexing
  pub index_duration: Duration,
  /// Total time
  pub total_duration: Duration,
  /// Files processed per second
  pub files_per_second: f64,
  /// Bytes processed
  pub bytes_processed: u64,
  /// Total bytes
  pub total_bytes: u64,
}

/// Scan a directory for code files, respecting .gitignore.
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

    // Only index files with supported code extensions
    if let Some(ext) = path.extension().and_then(|e| e.to_str())
      && Language::from_extension(ext).is_some()
    {
      // Track file size
      if let Ok(metadata) = std::fs::metadata(path) {
        total_bytes += metadata.len();
      }
      files.push(path.to_path_buf());
    }
  }

  ScanResult {
    files,
    total_bytes,
    duration: start.elapsed(),
  }
}

/// Run the full indexing pipeline.
///
/// # Arguments
/// * `indexer` - Handle to the indexer actor
/// * `scan_result` - Result from scanning
/// * `progress_tx` - Optional channel for progress updates
///
/// # Returns
/// * `IndexResult` - Full indexing result with stats
pub async fn run_indexing(
  indexer: &IndexerHandle,
  scan_result: ScanResult,
  progress_tx: Option<mpsc::Sender<IndexProgress>>,
) -> IndexResult {
  let start = Instant::now();

  if scan_result.files.is_empty() {
    return IndexResult {
      status: "complete".to_string(),
      files_scanned: 0,
      files_indexed: 0,
      chunks_created: 0,
      failed_files: 0,
      resumed_from_checkpoint: false,
      scan_duration: scan_result.duration,
      index_duration: Duration::ZERO,
      total_duration: scan_result.duration,
      files_per_second: 0.0,
      bytes_processed: 0,
      total_bytes: 0,
    };
  }

  let files_scanned = scan_result.files.len();
  let total_bytes = scan_result.total_bytes;

  // Create internal progress channel to capture final result
  let (internal_tx, mut internal_rx) = mpsc::channel::<IndexProgress>(64);

  // Send batch index job to IndexerActor
  let index_result = indexer.index_batch(scan_result.files, Some(internal_tx)).await;

  if let Err(e) = index_result {
    warn!(error = %e, "Batch index job failed to start");
    let index_duration = start.elapsed();
    let total_duration = scan_result.duration + index_duration;
    return IndexResult {
      status: "failed".to_string(),
      files_scanned,
      files_indexed: 0,
      chunks_created: 0,
      failed_files: files_scanned,
      resumed_from_checkpoint: false,
      scan_duration: scan_result.duration,
      index_duration,
      total_duration,
      files_per_second: 0.0,
      bytes_processed: 0,
      total_bytes,
    };
  }

  // Wait for progress updates, forwarding to caller and capturing final result
  let mut chunks_created = 0;

  while let Some(progress) = internal_rx.recv().await {
    // Forward to caller if they want progress updates
    if let Some(ref tx) = progress_tx {
      let _ = tx.send(progress.clone()).await;
    }

    // Check if this is the final progress (processed == total with chunks_created > 0 means final)
    if progress.is_complete() && progress.chunks_created > 0 {
      chunks_created = progress.chunks_created;
      break;
    }
  }

  // Drain any remaining progress messages
  while let Ok(progress) = internal_rx.try_recv() {
    if let Some(ref tx) = progress_tx {
      let _ = tx.send(progress.clone()).await;
    }
    if progress.chunks_created > 0 {
      chunks_created = progress.chunks_created;
    }
  }

  let index_duration = start.elapsed();
  let total_duration = scan_result.duration + index_duration;

  let files_per_second = if total_duration.as_secs_f64() > 0.0 {
    files_scanned as f64 / total_duration.as_secs_f64()
  } else {
    0.0
  };

  IndexResult {
    status: "complete".to_string(),
    files_scanned,
    files_indexed: files_scanned,
    chunks_created,
    failed_files: 0,
    resumed_from_checkpoint: false,
    scan_duration: scan_result.duration,
    index_duration,
    total_duration,
    files_per_second,
    bytes_processed: total_bytes,
    total_bytes,
  }
}
