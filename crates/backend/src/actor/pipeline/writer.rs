//! Writer stage - accumulates processed files and batch writes to DB.

use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::{Duration, Instant},
};

use chrono::Utc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

use super::{
  PipelineError,
  embedder::{EmbeddedChunks, ProcessedFile},
};
use crate::{
  actor::indexer::PipelineConfig,
  context::files::{Chunk, Indexer},
  db::{IndexedFile, ProjectDb},
  domain::document::Document,
};

/// Configuration for the writer stage
#[derive(Debug, Clone)]
pub struct WriterConfig {
  pub flush_count: usize,
  pub flush_timeout: Duration,
  pub project_root: Option<PathBuf>,
  pub project_id: Option<String>,
}

impl WriterConfig {
  pub fn from_pipeline_config(config: &PipelineConfig) -> Self {
    Self {
      flush_count: config.db_flush_count,
      flush_timeout: config.db_flush_timeout,
      project_root: None,
      project_id: None,
    }
  }

  pub fn with_project(mut self, root: PathBuf, project_id: String) -> Self {
    self.project_root = Some(root);
    self.project_id = Some(project_id);
    self
  }
}

struct WriteAccumulator {
  pending_files: Vec<ProcessedFile>,
  chunk_count: usize,
  last_activity: Instant,
}

impl WriteAccumulator {
  fn new() -> Self {
    Self {
      pending_files: Vec::new(),
      chunk_count: 0,
      last_activity: Instant::now(),
    }
  }

  fn add(&mut self, file: ProcessedFile) {
    let chunk_count = file.chunk_count();
    self.chunk_count += chunk_count;
    self.pending_files.push(file);
    self.last_activity = Instant::now();
  }

  fn should_flush_count(&self, threshold: usize) -> bool {
    self.chunk_count >= threshold
  }

  fn should_flush_time(&self, timeout: Duration) -> bool {
    !self.pending_files.is_empty() && self.last_activity.elapsed() >= timeout
  }

  fn take(&mut self) -> Vec<ProcessedFile> {
    self.chunk_count = 0;
    self.last_activity = Instant::now();
    std::mem::take(&mut self.pending_files)
  }

  fn is_empty(&self) -> bool {
    self.pending_files.is_empty()
  }
}

/// Stats returned by the writer stage
#[derive(Debug, Default)]
pub struct WriterStats {
  pub chunks_written: usize,
}

/// Writer stage - uses Indexer::store_chunks for DB writes.
pub async fn writer_stage(
  indexer: Indexer,
  mut rx: mpsc::Receiver<EmbeddedChunks>,
  db: Arc<ProjectDb>,
  config: WriterConfig,
  cancel: CancellationToken,
) -> WriterStats {
  debug!(
    flush_count = config.flush_count,
    flush_timeout_ms = config.flush_timeout.as_millis(),
    "Writer stage starting"
  );

  let mut accumulator = WriteAccumulator::new();
  let mut interval = tokio::time::interval(config.flush_timeout);
  let mut total_chunks_written = 0usize;

  let project_root = config.project_root.as_ref();
  let project_id = config.project_id.as_deref();

  loop {
    tokio::select! {
      biased;

      _ = cancel.cancelled() => {
        debug!("Writer stage cancelled");
        if !accumulator.is_empty() {
          let files = accumulator.take();
          match flush_to_db(&indexer, &db, files, project_root, project_id).await {
            Ok((_, c)) => total_chunks_written += c,
            Err(e) => error!(error = %e, "Failed to flush on cancellation"),
          }
        }
        break;
      }

      msg = rx.recv() => {
        match msg {
          Some(EmbeddedChunks::Batch { files }) => {
              for file in files {
                accumulator.add(file);
              }

              if accumulator.should_flush_count(config.flush_count) {
                let files = accumulator.take();
                match flush_to_db(&indexer, &db, files, project_root, project_id).await {
                  Ok((_, c)) => {
                    total_chunks_written += c;
                    trace!(chunks = c, total = total_chunks_written, "Flushed batch to DB");
                  }
                  Err(e) => error!(error = %e, "Failed to flush to DB"),
                }
              }
          }
          Some(EmbeddedChunks::Done) | None => {
            if !accumulator.is_empty() {
              let files = accumulator.take();
              match flush_to_db(&indexer, &db, files, project_root, project_id).await {
                Ok((_, c)) => total_chunks_written += c,
                Err(e) => error!(error = %e, "Failed to flush final batch to DB"),
              }
            }

            debug!(total_chunks_written, "Writer stage complete");
            return WriterStats {
              chunks_written: total_chunks_written,
            };
          }
        }
      }

      _ = interval.tick() => {
        if accumulator.should_flush_time(config.flush_timeout) {
          let files = accumulator.take();
          match flush_to_db(&indexer, &db, files, project_root, project_id).await {
            Ok((_, c)) => {
              total_chunks_written += c;
              trace!(chunks = c, "Timeout flush to DB");
            }
            Err(e) => error!(error = %e, "Failed to flush on timeout"),
          }
        }
      }
    }
  }

  WriterStats {
    chunks_written: total_chunks_written,
  }
}

async fn flush_to_db(
  indexer: &Indexer,
  db: &ProjectDb,
  files: Vec<ProcessedFile>,
  project_root: Option<&PathBuf>,
  project_id: Option<&str>,
) -> Result<(usize, usize), PipelineError> {
  if files.is_empty() {
    return Ok((0, 0));
  }

  let mut total_files = 0;
  let mut total_chunks = 0;

  for file in files {
    let file_path = &file.relative;
    let chunk_count = file.chunks_with_vectors.len();

    // Delete existing chunks for this file
    if let Err(e) = indexer.delete_file_chunks(db, file_path).await {
      warn!(file = %file_path, error = %e, "Failed to delete existing chunks");
    }

    // Store new chunks
    if let Err(e) = indexer.store_chunks(db, file_path, &file.chunks_with_vectors).await {
      error!(file = %file_path, error = %e, "Failed to store chunks");
      continue;
    }

    total_files += 1;
    total_chunks += chunk_count;

    // Update indexed_files metadata for startup scan detection
    if let (Some(root), Some(pid)) = (project_root, project_id) {
      update_indexed_file_metadata(db, file_path, &file.chunks_with_vectors, root, pid).await;
    }

    // Create Document metadata for document files (those with char_count/content_hash)
    if let (Some(char_count), Some(content_hash), Some(_pid)) =
      (file.char_count, file.content_hash.as_ref(), project_id)
      && let Some((first_chunk, _)) = file.chunks_with_vectors.first()
      && let Chunk::Document(doc_chunk) = first_chunk
    {
      let doc = Document {
        id: doc_chunk.document_id,
        project_id: doc_chunk.project_id,
        title: doc_chunk.title.clone(),
        source: file.relative.clone(),
        source_type: doc_chunk.source_type,
        content_hash: content_hash.clone(),
        char_count,
        chunk_count,
        full_content: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
      };
      if let Err(e) = db.upsert_document_metadata(&doc).await {
        warn!(file = %file_path, error = %e, "Failed to upsert document metadata");
      } else {
        trace!(file = %file_path, doc_id = %doc.id, "Created document metadata");
      }
    }
  }

  Ok((total_files, total_chunks))
}

async fn update_indexed_file_metadata(
  db: &ProjectDb,
  file_path: &str,
  chunks_with_vectors: &[(Chunk, Vec<f32>)],
  project_root: &Path,
  project_id: &str,
) {
  let full_path = project_root.join(file_path);

  let metadata = match tokio::fs::metadata(&full_path).await {
    Ok(m) => m,
    Err(e) => {
      warn!(file_path = %file_path, error = %e, "Failed to get file metadata");
      return;
    }
  };

  let mtime = metadata
    .modified()
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0);

  let file_size = metadata.len();

  // Get content hash from first chunk
  let content_hash = chunks_with_vectors
    .first()
    .map(|(chunk, _)| chunk.file_hash())
    .filter(|h| !h.is_empty())
    .map(|h| h.to_string())
    .unwrap_or_else(|| "unknown".to_string());

  let indexed_file = IndexedFile {
    file_path: file_path.to_string(),
    project_id: project_id.to_string(),
    mtime,
    content_hash,
    file_size,
    last_indexed_at: Utc::now().timestamp_millis(),
  };

  if let Err(e) = db.save_indexed_files_batch(&[indexed_file]).await {
    warn!(error = %e, file_path = %file_path, "Failed to update indexed_files metadata");
  }
}
