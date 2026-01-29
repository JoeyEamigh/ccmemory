//! Project-level services.
//!
//! Provides operations for project management including:
//! - Project statistics
//! - Project cleanup

use std::path::Path;

use uuid::Uuid;

use crate::{
  db::ProjectDb,
  domain::project::ProjectId,
  ipc::project::{ProjectCleanResult, ProjectInfoResult, ProjectStatsResult},
  service::util::ServiceError,
};

/// Get project information.
///
/// # Arguments
/// * `db` - Project database
/// * `project_id` - Project ID
/// * `root` - Project root path
///
/// # Returns
/// * `Ok(ProjectInfoResult)` - Project information
/// * `Err(ServiceError)` - If query fails
pub async fn info(db: &ProjectDb, project_id: &ProjectId, root: &Path) -> Result<ProjectInfoResult, ServiceError> {
  let memory_count = db.list_memories(None, Some(1)).await.map(|m| m.len()).unwrap_or(0);
  let code_chunk_count = db.list_code_chunks(None, Some(1)).await.map(|c| c.len()).unwrap_or(0);

  Ok(ProjectInfoResult {
    id: project_id.to_string(),
    path: root.to_string_lossy().to_string(),
    name: root
      .file_name()
      .map(|n| n.to_string_lossy().to_string())
      .unwrap_or_else(|| "unknown".to_string()),
    memory_count,
    code_chunk_count,
    document_count: 0,
    session_count: 0,
    db_path: String::new(), // Caller can fill this in if needed
  })
}

/// Get project statistics.
///
/// # Arguments
/// * `db` - Project database
/// * `project_id` - Project ID
/// * `project_uuid` - Project UUID for session counting
/// * `root` - Project root path
///
/// # Returns
/// * `Ok(ProjectStatsResult)` - Project statistics
/// * `Err(ServiceError)` - If query fails
pub async fn stats(
  db: &ProjectDb,
  project_id: &ProjectId,
  project_uuid: &Uuid,
  root: &Path,
) -> Result<ProjectStatsResult, ServiceError> {
  let memories = db.list_memories(None, None).await.map(|m| m.len()).unwrap_or(0);
  let code_chunks = db.list_code_chunks(None, None).await.map(|c| c.len()).unwrap_or(0);
  let documents = db.list_document_chunks(None, None).await.map(|d| d.len()).unwrap_or(0);
  let sessions = db.count_sessions(project_uuid).await.unwrap_or(0);

  Ok(ProjectStatsResult {
    project_id: project_id.to_string(),
    path: root.to_string_lossy().to_string(),
    memories,
    code_chunks,
    documents,
    sessions,
  })
}

/// Clean all data from a project.
///
/// Deletes all memories, code chunks, and documents.
///
/// # Arguments
/// * `db` - Project database
/// * `root` - Project root path
///
/// # Returns
/// * `Ok(ProjectCleanResult)` - Cleanup results with counts
/// * `Err(ServiceError)` - If cleanup fails
pub async fn clean(db: &ProjectDb, root: &Path) -> Result<ProjectCleanResult, ServiceError> {
  // Get counts and delete all data
  let memories = db.list_memories(None, None).await.unwrap_or_default();
  let memories_deleted = memories.len();
  for memory in &memories {
    let _ = db.delete_memory(&memory.id).await;
  }

  let code_chunks = db.list_code_chunks(None, None).await.unwrap_or_default();
  let code_chunks_deleted = code_chunks.len();
  for chunk in &code_chunks {
    let _ = db.delete_code_chunk(&chunk.id).await;
  }

  let documents = db.list_document_chunks(None, None).await.unwrap_or_default();
  let documents_deleted = documents.len();
  for doc in &documents {
    let _ = db.delete_document_chunk(&doc.id).await;
  }

  Ok(ProjectCleanResult {
    path: root.to_string_lossy().to_string(),
    memories_deleted,
    code_chunks_deleted,
    documents_deleted,
  })
}
