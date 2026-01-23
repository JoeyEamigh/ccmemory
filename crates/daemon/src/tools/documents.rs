//! Document ingestion and search tool methods

use super::ToolHandler;
use crate::router::{Request, Response};
use engram_core::{ChunkParams, DocumentChunk, DocumentId, DocumentSource, chunk_text};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tracing::{debug, warn};

impl ToolHandler {
  pub async fn docs_search(&self, request: Request) -> Response {
    #[derive(Deserialize)]
    struct Args {
      query: String,
      #[serde(default)]
      cwd: Option<String>,
      #[serde(default)]
      limit: Option<usize>,
    }

    let args: Args = match serde_json::from_value(request.params.clone()) {
      Ok(a) => a,
      Err(e) => return Response::error(request.id, -32602, &format!("Invalid params: {}", e)),
    };

    let project_path = args
      .cwd
      .map(PathBuf::from)
      .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let (_, db) = match self.registry.get_or_create(&project_path).await {
      Ok(p) => p,
      Err(e) => return Response::error(request.id, -32000, &format!("Project error: {}", e)),
    };

    let limit = args.limit.unwrap_or(5);

    // Try vector search if embedding provider is available
    if let Some(query_vec) = self.get_embedding(&args.query).await {
      debug!("Using vector search for docs query: {}", args.query);
      match db.search_documents(&query_vec, limit, None).await {
        Ok(results) => {
          let results: Vec<_> = results
            .into_iter()
            .map(|(chunk, distance)| {
              let similarity = 1.0 - distance.min(1.0);
              serde_json::json!({
                  "id": chunk.id.to_string(),
                  "document_id": chunk.document_id.to_string(),
                  "title": chunk.title,
                  "source": chunk.source,
                  "content": chunk.content,
                  "chunk_index": chunk.chunk_index,
                  "total_chunks": chunk.total_chunks,
                  "similarity": similarity,
              })
            })
            .collect();

          return Response::success(request.id, serde_json::json!(results));
        }
        Err(e) => {
          warn!("Vector docs search failed, falling back to text: {}", e);
        }
      }
    }

    // Fallback: text-based search
    debug!("Using text search for docs query: {}", args.query);
    match db.list_document_chunks(None, Some(limit * 10)).await {
      Ok(chunks) => {
        let query_lower = args.query.to_lowercase();
        let results: Vec<_> = chunks
          .into_iter()
          .filter(|c| c.content.to_lowercase().contains(&query_lower) || c.title.to_lowercase().contains(&query_lower))
          .take(limit)
          .map(|chunk| {
            serde_json::json!({
                "id": chunk.id.to_string(),
                "document_id": chunk.document_id.to_string(),
                "title": chunk.title,
                "source": chunk.source,
                "content": chunk.content,
                "chunk_index": chunk.chunk_index,
                "total_chunks": chunk.total_chunks,
            })
          })
          .collect();

        Response::success(request.id, serde_json::json!(results))
      }
      Err(e) => Response::error(request.id, -32000, &format!("Docs search error: {}", e)),
    }
  }

  pub async fn docs_ingest(&self, request: Request) -> Response {
    #[derive(Deserialize)]
    struct Args {
      #[serde(default)]
      path: Option<String>,
      #[serde(default)]
      url: Option<String>,
      #[serde(default)]
      content: Option<String>,
      #[serde(default)]
      title: Option<String>,
      #[serde(default)]
      cwd: Option<String>,
    }

    let args: Args = match serde_json::from_value(request.params.clone()) {
      Ok(a) => a,
      Err(e) => return Response::error(request.id, -32602, &format!("Invalid params: {}", e)),
    };

    // Must provide one of path, url, or content
    if args.path.is_none() && args.url.is_none() && args.content.is_none() {
      return Response::error(request.id, -32602, "Must provide path, url, or content");
    }

    let project_path = args
      .cwd
      .map(PathBuf::from)
      .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let (info, db) = match self.registry.get_or_create(&project_path).await {
      Ok(p) => p,
      Err(e) => return Response::error(request.id, -32000, &format!("Project error: {}", e)),
    };

    // Determine source type and get content
    let (content, source, source_type, title) = if let Some(path) = args.path {
      // Read from file
      let full_path = if path.starts_with('/') {
        PathBuf::from(&path)
      } else {
        project_path.join(&path)
      };

      match std::fs::read_to_string(&full_path) {
        Ok(content) => {
          let title = args.title.unwrap_or_else(|| {
            full_path
              .file_name()
              .map(|s| s.to_string_lossy().to_string())
              .unwrap_or_else(|| path.clone())
          });
          (content, path, DocumentSource::File, title)
        }
        Err(e) => return Response::error(request.id, -32000, &format!("Failed to read file: {}", e)),
      }
    } else if let Some(url) = args.url {
      // Fetch from URL
      match reqwest::get(&url).await {
        Ok(resp) => match resp.text().await {
          Ok(content) => {
            let title = args.title.unwrap_or_else(|| url.clone());
            (content, url, DocumentSource::Url, title)
          }
          Err(e) => return Response::error(request.id, -32000, &format!("Failed to read response: {}", e)),
        },
        Err(e) => return Response::error(request.id, -32000, &format!("Failed to fetch URL: {}", e)),
      }
    } else if let Some(content) = args.content {
      let title = args.title.unwrap_or_else(|| "Untitled Document".to_string());
      (content, "content".to_string(), DocumentSource::Content, title)
    } else {
      return Response::error(request.id, -32602, "Must provide path, url, or content");
    };

    // Validate content
    if content.is_empty() {
      return Response::error(request.id, -32602, "Document content is empty");
    }
    if content.len() > 1_000_000 {
      return Response::error(request.id, -32602, "Document too large (max 1MB)");
    }

    // Compute content hash for deduplication
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let content_hash = format!("{:x}", hasher.finalize());

    // Check if document already exists
    let filter = format!(
      "source = '{}' AND title = '{}'",
      source.replace('\'', "''"),
      title.replace('\'', "''")
    );
    match db.list_document_chunks(Some(&filter), Some(1)).await {
      Ok(existing) if !existing.is_empty() => {
        // Delete existing document first
        let existing_doc_id = existing[0].document_id;
        if let Err(e) = db.delete_document(&existing_doc_id).await {
          warn!("Failed to delete existing document: {}", e);
        }
      }
      _ => {}
    }

    // Chunk the content
    let params = ChunkParams::default();
    let text_chunks = chunk_text(&content, &params);
    let total_chunks = text_chunks.len();

    // Create document ID
    let document_id = DocumentId::new();
    let project_uuid = uuid::Uuid::parse_str(info.id.as_str()).unwrap_or_else(|_| uuid::Uuid::new_v4());

    // Create and store chunks
    let mut stored_chunks = 0;
    for (i, (chunk_content, char_offset)) in text_chunks.into_iter().enumerate() {
      let chunk = DocumentChunk::new(
        document_id,
        project_uuid,
        chunk_content.clone(),
        title.clone(),
        source.clone(),
        source_type,
        i,
        total_chunks,
        char_offset,
      );

      // Generate embedding
      let vector = match self.get_embedding(&chunk_content).await {
        Some(v) => v,
        None => vec![0.0f32; db.vector_dim],
      };

      if let Err(e) = db.add_document_chunk(&chunk, Some(&vector)).await {
        warn!("Failed to store chunk {}: {}", i, e);
        continue;
      }
      stored_chunks += 1;
    }

    Response::success(
      request.id,
      serde_json::json!({
          "document_id": document_id.to_string(),
          "title": title,
          "source": source,
          "source_type": source_type.as_str(),
          "content_hash": content_hash,
          "char_count": content.len(),
          "chunks_created": stored_chunks,
          "total_chunks": total_chunks,
      }),
    )
  }

  /// Get adjacent chunks from the same document
  pub async fn doc_context(&self, request: Request) -> Response {
    #[derive(Deserialize)]
    struct Args {
      chunk_id: String,
      #[serde(default)]
      cwd: Option<String>,
      #[serde(default)]
      chunks_before: Option<usize>,
      #[serde(default)]
      chunks_after: Option<usize>,
    }

    let args: Args = match serde_json::from_value(request.params.clone()) {
      Ok(a) => a,
      Err(e) => return Response::error(request.id, -32602, &format!("Invalid params: {}", e)),
    };

    let project_path = args
      .cwd
      .map(PathBuf::from)
      .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let (_, db) = match self.registry.get_or_create(&project_path).await {
      Ok(p) => p,
      Err(e) => return Response::error(request.id, -32000, &format!("Project error: {}", e)),
    };

    // Cap and default context chunks
    let chunks_before = args.chunks_before.unwrap_or(1).min(10);
    let chunks_after = args.chunks_after.unwrap_or(1).min(10);

    // Look up chunk by ID or prefix
    let target_chunk = match db.get_document_chunk_by_id_or_prefix(&args.chunk_id).await {
      Ok(Some(c)) => c,
      Ok(None) => {
        return Response::error(
          request.id,
          -32000,
          &format!("Document chunk not found: {}", args.chunk_id),
        );
      }
      Err(db::DbError::AmbiguousPrefix { prefix, count }) => {
        return Response::error(
          request.id,
          -32000,
          &format!(
            "Ambiguous prefix '{}' matches {} chunks. Use more characters.",
            prefix, count
          ),
        );
      }
      Err(db::DbError::InvalidInput(msg)) => {
        return Response::error(request.id, -32602, &msg);
      }
      Err(e) => {
        return Response::error(request.id, -32000, &format!("Database error: {}", e));
      }
    };

    // Get adjacent chunks
    let adjacent_chunks = match db
      .get_adjacent_document_chunks(
        &target_chunk.document_id,
        target_chunk.chunk_index,
        chunks_before,
        chunks_after,
      )
      .await
    {
      Ok(chunks) => chunks,
      Err(e) => {
        return Response::error(
          request.id,
          -32000,
          &format!("Failed to retrieve adjacent chunks: {}", e),
        );
      }
    };

    // Split into before, target, and after
    let mut before_chunks: Vec<serde_json::Value> = Vec::new();
    let mut after_chunks: Vec<serde_json::Value> = Vec::new();
    let mut target_json = serde_json::json!(null);

    for chunk in adjacent_chunks {
      let chunk_json = serde_json::json!({
        "chunk_index": chunk.chunk_index,
        "content": chunk.content,
      });

      if chunk.chunk_index < target_chunk.chunk_index {
        before_chunks.push(chunk_json);
      } else if chunk.chunk_index > target_chunk.chunk_index {
        after_chunks.push(chunk_json);
      } else {
        target_json = chunk_json;
      }
    }

    Response::success(
      request.id,
      serde_json::json!({
        "chunk_id": target_chunk.id.to_string(),
        "document_id": target_chunk.document_id.to_string(),
        "title": target_chunk.title,
        "source": target_chunk.source,
        "context": {
          "before": before_chunks,
          "target": target_json,
          "after": after_chunks,
        },
        "total_chunks": target_chunk.total_chunks,
      }),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::super::create_test_handler;
  use crate::router::Request;

  #[tokio::test]
  async fn test_docs_ingest_missing_source() {
    let (_dir, handler) = create_test_handler();

    // No path, url, or content provided
    let request = Request {
      id: Some(serde_json::json!(1)),
      method: "docs_ingest".to_string(),
      params: serde_json::json!({
          "title": "Test Doc"
      }),
    };

    let response = handler.docs_ingest(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("Must provide"));
  }

  #[tokio::test]
  async fn test_docs_ingest_empty_content() {
    let (_dir, handler) = create_test_handler();

    let request = Request {
      id: Some(serde_json::json!(1)),
      method: "docs_ingest".to_string(),
      params: serde_json::json!({
          "content": "",
          "title": "Empty Doc"
      }),
    };

    let response = handler.docs_ingest(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("empty"));
  }
}
