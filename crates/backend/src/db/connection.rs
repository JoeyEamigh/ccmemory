use std::{path::PathBuf, sync::Arc};

use lancedb::{Connection, connect};
use thiserror::Error;
use tracing::{debug, error, info};

use crate::{
  config::Config,
  db::schema::{
    code_chunks_schema, document_metadata_schema, documents_schema, indexed_files_schema, memories_schema,
    memory_relationships_schema, session_memories_schema, sessions_schema,
  },
  domain::project::ProjectId,
};

#[derive(Error, Debug)]
pub enum DbError {
  #[error("LanceDB error: {0}")]
  Lance(#[from] lancedb::Error),
  #[error("Arrow error: {0}")]
  Arrow(#[from] arrow::error::ArrowError),
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
  #[error("Not found: {0}")]
  NotFound(String),
  #[error("Serialization error: {0}")]
  Serialization(#[from] serde_json::Error),
  #[error("Invalid input: {0}")]
  InvalidInput(String),
  #[error("Database query error: {0}")]
  Query(String),
  #[error("Ambiguous prefix '{prefix}' matches {count} items. Use more characters.")]
  AmbiguousPrefix { prefix: String, count: usize },
}

pub type Result<T> = std::result::Result<T, DbError>;

/// Database connection for a specific project
pub struct ProjectDb {
  pub project_id: ProjectId,
  pub connection: Connection,
  pub vector_dim: usize,
}

impl ProjectDb {
  /// Open or create a project database
  pub async fn open(project_id: ProjectId, base_path: &std::path::Path, config: Arc<Config>) -> Result<Self> {
    let db_path = project_id.data_dir(base_path).join("lancedb");
    Self::open_at_path(project_id, db_path, config).await
  }

  /// Open database at a specific path
  pub async fn open_at_path(project_id: ProjectId, db_path: PathBuf, config: Arc<Config>) -> Result<Self> {
    // Ensure directory exists
    if let Some(parent) = db_path.parent() {
      tokio::fs::create_dir_all(parent).await?;
    }

    info!(path = %db_path.display(), project_id = %project_id.as_str(), vector_dim = config.embedding.dimensions, "Opening database connection");
    let connection = match connect(db_path.to_string_lossy().as_ref()).execute().await {
      Ok(conn) => {
        debug!(path = %db_path.display(), "Database connection established");
        conn
      }
      Err(e) => {
        error!(path = %db_path.display(), err = %e, "Failed to connect to database");
        return Err(e.into());
      }
    };

    let db = Self {
      vector_dim: config.embedding.dimensions,
      project_id,
      connection,
    };

    // Ensure tables exist
    debug!("Initializing database schema");
    db.ensure_tables().await?;

    Ok(db)
  }

  /// Ensure all required tables exist
  async fn ensure_tables(&self) -> Result<()> {
    let table_names = self.connection.table_names().execute().await?;
    debug!(existing_tables = table_names.len(), "Checking required tables");

    if !table_names.contains(&"memories".to_string()) {
      debug!("Creating memories table");
      self
        .connection
        .create_empty_table("memories", memories_schema(self.vector_dim))
        .execute()
        .await?;
    }

    if !table_names.contains(&"code_chunks".to_string()) {
      debug!("Creating code_chunks table");
      self
        .connection
        .create_empty_table("code_chunks", code_chunks_schema(self.vector_dim))
        .execute()
        .await?;
    }

    if !table_names.contains(&"sessions".to_string()) {
      debug!("Creating sessions table");
      self
        .connection
        .create_empty_table("sessions", sessions_schema())
        .execute()
        .await?;
    }

    if !table_names.contains(&"documents".to_string()) {
      debug!("Creating documents table");
      self
        .connection
        .create_empty_table("documents", documents_schema(self.vector_dim))
        .execute()
        .await?;
    }

    if !table_names.contains(&"session_memories".to_string()) {
      debug!("Creating session_memories table");
      self
        .connection
        .create_empty_table("session_memories", session_memories_schema())
        .execute()
        .await?;
    }

    if !table_names.contains(&"memory_relationships".to_string()) {
      debug!("Creating memory_relationships table");
      self
        .connection
        .create_empty_table("memory_relationships", memory_relationships_schema())
        .execute()
        .await?;
    }

    if !table_names.contains(&"document_metadata".to_string()) {
      debug!("Creating document_metadata table");
      self
        .connection
        .create_empty_table("document_metadata", document_metadata_schema())
        .execute()
        .await?;
    }

    if !table_names.contains(&"indexed_files".to_string()) {
      debug!("Creating indexed_files table");
      self
        .connection
        .create_empty_table("indexed_files", indexed_files_schema())
        .execute()
        .await?;
    }

    Ok(())
  }

  /// Get the memories table
  pub async fn memories_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("memories").execute().await?)
  }

  /// Get the code_chunks table
  pub async fn code_chunks_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("code_chunks").execute().await?)
  }

  /// Get the sessions table
  pub async fn sessions_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("sessions").execute().await?)
  }

  /// Get the documents table
  pub async fn documents_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("documents").execute().await?)
  }

  /// Get the session_memories table
  pub async fn session_memories_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("session_memories").execute().await?)
  }

  /// Get the memory_relationships table
  pub async fn memory_relationships_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("memory_relationships").execute().await?)
  }

  /// Get the document_metadata table
  pub async fn document_metadata_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("document_metadata").execute().await?)
  }

  /// Get the indexed_files table
  pub async fn indexed_files_table(&self) -> Result<lancedb::Table> {
    Ok(self.connection.open_table("indexed_files").execute().await?)
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use tempfile::TempDir;

  use super::*;

  #[tokio::test]
  async fn test_open_database() {
    let temp_dir = TempDir::new().unwrap();
    let project_id = ProjectId::from_path(Path::new("/test/project")).await;

    let db = ProjectDb::open_at_path(
      project_id.clone(),
      temp_dir.path().join("test.lancedb"),
      Arc::new(Config::default()),
    )
    .await
    .unwrap();

    assert_eq!(db.project_id.as_str(), project_id.as_str());
  }

  #[tokio::test]
  async fn test_tables_created() {
    let temp_dir = TempDir::new().unwrap();
    let project_id = ProjectId::from_path(Path::new("/test/project")).await;

    let db = ProjectDb::open_at_path(
      project_id,
      temp_dir.path().join("test.lancedb"),
      Arc::new(Config::default()),
    )
    .await
    .unwrap();

    let tables = db.connection.table_names().execute().await.unwrap();
    assert!(tables.contains(&"memories".to_string()), "memories table should exist");
    assert!(
      tables.contains(&"code_chunks".to_string()),
      "code_chunks table should exist"
    );
    assert!(tables.contains(&"sessions".to_string()), "sessions table should exist");
    assert!(
      tables.contains(&"documents".to_string()),
      "documents table should exist"
    );
  }
}
