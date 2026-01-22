//! Document ingestion and search integration tests for the CCEngram daemon
//!
//! Tests: document lifecycle, ingest from file, metadata and updates, full content storage.

mod common;

use daemon::Request;
use tempfile::TempDir;

/// Test document ingestion and search
#[tokio::test]
async fn test_router_docs_lifecycle() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Test docs_ingest with content
  let ingest_request = Request {
    id: Some(serde_json::json!(1)),
    method: "docs_ingest".to_string(),
    params: serde_json::json!({
        "content": "This is a test document about Rust programming and memory management. It covers topics like ownership, borrowing, and lifetimes.",
        "title": "Rust Memory Guide",
        "cwd": cwd
    }),
  };

  let ingest_response = router.handle(ingest_request).await;
  assert!(
    ingest_response.error.is_none(),
    "docs_ingest should succeed: {:?}",
    ingest_response.error
  );
  let result = ingest_response.result.expect("Should have result");
  let doc_id = result.get("document_id").expect("Should have document_id");
  assert!(doc_id.is_string(), "document_id should be string");
  let chunks = result.get("chunks_created").and_then(|v| v.as_u64()).unwrap_or(0);
  assert!(chunks >= 1, "Should create at least one chunk");

  // Test docs_search
  let search_request = Request {
    id: Some(serde_json::json!(2)),
    method: "docs_search".to_string(),
    params: serde_json::json!({
        "query": "Rust ownership borrowing",
        "cwd": cwd
    }),
  };

  let search_response = router.handle(search_request).await;
  assert!(search_response.error.is_none(), "docs_search should succeed");
  // Text search fallback should find results since vector search won't have embeddings
}

/// Test document ingestion from file path
#[tokio::test]
async fn test_router_docs_ingest_from_file() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Create a markdown document
  let doc_content = r#"# Getting Started Guide

This guide explains how to set up the project.

## Installation

Run the following command:
```
npm install
```

## Configuration

Create a `.env` file with your settings.
"#;

  let doc_path = project_dir.path().join("guide.md");
  std::fs::write(&doc_path, doc_content).expect("Failed to write guide.md");

  // Ingest from file
  let ingest_request = Request {
    id: Some(serde_json::json!(1)),
    method: "docs_ingest".to_string(),
    params: serde_json::json!({
        "path": "guide.md",
        "title": "Getting Started",
        "cwd": cwd
    }),
  };

  let response = router.handle(ingest_request).await;
  assert!(
    response.error.is_none(),
    "docs_ingest from file should succeed: {:?}",
    response.error
  );

  let result = response.result.expect("Should have result");
  assert!(result.get("document_id").is_some(), "Should have document_id");
  assert!(result.get("chunks_created").and_then(|v| v.as_u64()).unwrap_or(0) >= 1);

  // Search for the document
  let search_request = Request {
    id: Some(serde_json::json!(2)),
    method: "docs_search".to_string(),
    params: serde_json::json!({
        "query": "npm install configuration",
        "cwd": cwd
    }),
  };

  let search_response = router.handle(search_request).await;
  assert!(search_response.error.is_none(), "docs_search should succeed");
}

/// Test document metadata and update detection
#[tokio::test]
async fn test_document_metadata_and_updates() {
  use db::{ProjectDb, compute_content_hash};
  use engram_core::{Document, DocumentSource};
  use std::io::Write;
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create data temp dir");
  let project_dir = TempDir::new().expect("Failed to create project temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = engram_core::ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id, db_path, 768).await.unwrap();

  let doc_project_id = uuid::Uuid::new_v4();

  // Create a test file
  let file_path = project_dir.path().join("test_doc.md");
  let initial_content = "# Initial Content\n\nThis is the initial document content.";
  std::fs::write(&file_path, initial_content).unwrap();

  // Create document metadata
  let content_hash = compute_content_hash(initial_content);
  let doc = Document::new(
    doc_project_id,
    "Test Document".to_string(),
    file_path.to_string_lossy().to_string(),
    DocumentSource::File,
    content_hash.clone(),
    initial_content.len(),
    1,
  );

  // Store metadata
  db.upsert_document_metadata(&doc).await.unwrap();

  // Verify it can be retrieved by ID
  let retrieved = db.get_document_metadata(&doc.id).await.unwrap();
  assert!(retrieved.is_some());
  assert_eq!(retrieved.unwrap().content_hash, content_hash);

  // Verify it can be retrieved by source path
  let by_source = db.get_document_by_source(&file_path.to_string_lossy()).await.unwrap();
  assert!(by_source.is_some());
  assert_eq!(by_source.unwrap().id, doc.id);

  // Check for updates - should be unchanged
  let updates = db.check_document_updates(&doc_project_id.to_string()).await.unwrap();
  assert!(updates.modified.is_empty(), "Should not detect modifications yet");
  assert!(updates.missing.is_empty(), "File should exist");

  // Modify the file
  let mut file = std::fs::File::create(&file_path).unwrap();
  writeln!(file, "# Modified Content\n\nThis content has been changed.").unwrap();
  drop(file);

  // Check for updates again - should detect modification
  let updates_after = db.check_document_updates(&doc_project_id.to_string()).await.unwrap();
  assert_eq!(updates_after.modified.len(), 1, "Should detect one modified document");
  assert_eq!(updates_after.modified[0], doc.id);

  // Delete the file
  std::fs::remove_file(&file_path).unwrap();

  // Check for updates - should detect missing
  let updates_missing = db.check_document_updates(&doc_project_id.to_string()).await.unwrap();
  assert_eq!(updates_missing.missing.len(), 1, "Should detect one missing document");
  assert_eq!(updates_missing.missing[0], doc.id);
}

/// Test document full content storage
#[tokio::test]
async fn test_document_full_content_storage() {
  use db::ProjectDb;
  use engram_core::{Document, DocumentSource};
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = engram_core::ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id, db_path, 768).await.unwrap();

  let project_uuid = uuid::Uuid::new_v4();
  let full_content = "This is the full document content.\n\nWith multiple paragraphs.\n\nAnd more text.";

  // Create document with full content
  let doc = Document::with_content(
    project_uuid,
    "Full Content Doc".to_string(),
    "test://doc".to_string(),
    DocumentSource::Content,
    full_content.to_string(),
    3,
  );

  // Verify hash was computed
  assert!(!doc.content_hash.is_empty());
  assert_eq!(doc.char_count, full_content.len());
  assert!(doc.full_content.is_some());

  // Store and retrieve
  db.upsert_document_metadata(&doc).await.unwrap();

  let retrieved = db.get_document_metadata(&doc.id).await.unwrap().unwrap();
  assert_eq!(retrieved.full_content, Some(full_content.to_string()));
  assert_eq!(retrieved.char_count, full_content.len());
}
