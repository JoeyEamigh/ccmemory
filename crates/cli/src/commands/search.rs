//! Search commands for memories, code, and documents

use anyhow::{Context, Result};
use ccengram::ipc::{code::CodeSearchParams, docs::DocsSearchParams, memory::MemorySearchParams};
use tracing::error;

/// Format an ID for display
///
/// When `long` is false, shows only the first 8 characters with "..." suffix.
/// When `long` is true or ID is short, shows the full ID.
fn format_id(id: &str, long: bool) -> String {
  if long || id.len() <= 12 {
    id.to_string()
  } else {
    format!("{}...", &id[..8])
  }
}

/// Search memories
#[allow(clippy::too_many_arguments)]
pub async fn cmd_search(
  query: &str,
  limit: usize,
  project: Option<&str>,
  sector: Option<&str>,
  memory_type: Option<&str>,
  min_salience: Option<f32>,
  include_superseded: bool,
  scope: Option<&str>,
  json_output: bool,
  long_ids: bool,
) -> Result<()> {
  let cwd = project
    .map(std::path::PathBuf::from)
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| std::path::PathBuf::from("."));

  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = MemorySearchParams {
    query: query.to_string(),
    sector: sector.map(|s| s.to_string()),
    memory_type: memory_type.map(|t| t.to_string()),
    min_salience,
    scope_path: scope.map(|s| s.to_string()),
    limit: Some(limit),
    include_superseded,
    ..Default::default()
  };

  match client.call(params).await {
    Ok(result) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
      }

      let memories = &result.items;
      if memories.is_empty() {
        // Show suggestion if search quality indicates low confidence
        if let Some(quality) = &result.search_quality
          && let Some(suggestion) = &quality.suggested_action
        {
          println!("No memories found for: {}. {}", query, suggestion);
          return Ok(());
        }
        println!("No memories found for: {}", query);
      } else {
        println!("Found {} memories:\n", memories.len());

        // Show search quality warning if low confidence
        if let Some(quality) = &result.search_quality
          && quality.low_confidence
          && let Some(suggestion) = &quality.suggested_action
        {
          println!("Note: {}\n", suggestion);
        }

        for (i, memory) in memories.iter().enumerate() {
          println!("{}. [{}] {}", i + 1, memory.sector, format_id(&memory.id, long_ids));
          // Print first 200 chars
          let content = &memory.content;
          let preview = if content.len() > 200 {
            format!("{}...", &content[..200])
          } else {
            content.to_string()
          };
          println!("   {}", preview.replace('\n', "\n   "));
          if let Some(sim) = memory.similarity {
            println!("   Similarity: {:.2}", sim);
          }
          println!();
        }

        // Help message about prefix matching
        if !long_ids {
          println!("Tip: Use --long to show full IDs. Prefixes (8+ chars) work in commands.");
        }
      }
    }
    Err(e) => {
      error!("Search error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Search code
#[allow(clippy::too_many_arguments)]
pub async fn cmd_search_code(
  query: &str,
  limit: usize,
  project: Option<&str>,
  language: Option<&str>,
  chunk_type: Option<&str>,
  path: Option<&str>,
  symbol: Option<&str>,
  json_output: bool,
) -> Result<()> {
  let cwd = project
    .map(std::path::PathBuf::from)
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| std::path::PathBuf::from("."));

  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  // Build file_pattern from optional filters
  let file_pattern = match (language, chunk_type, path, symbol) {
    (Some(lang), _, _, _) => Some(format!("*.{}", lang)),
    (_, _, Some(p), _) => Some(p.to_string()),
    _ => None,
  };

  let symbol_type = chunk_type.map(|ct| ct.to_string());

  // TODO: Note: The daemon may need to handle "symbol" separately
  let _ = symbol;

  let params = CodeSearchParams {
    query: query.to_string(),
    limit: Some(limit),
    file_pattern,
    symbol_type,
    language: None,
    visibility: vec![],
    chunk_type: vec![],
    min_caller_count: None,
  };

  match client.call(params).await {
    Ok(result) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
      }

      let chunks = &result.chunks;

      if chunks.is_empty() {
        println!("No code found for: {}", query);
      } else {
        println!("Found {} code chunks:\n", chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
          println!(
            "{}. {}:{}-{} [{}]",
            i + 1,
            chunk.file_path,
            chunk.start_line,
            chunk.end_line,
            chunk.language.as_deref().unwrap_or("?")
          );

          if !chunk.symbols.is_empty() {
            println!("   Symbols: {}", chunk.symbols.join(", "));
          }

          if let Some(sim) = chunk.similarity {
            println!("   Similarity: {:.2}", sim);
          }
          println!();
        }
      }
    }
    Err(e) => {
      error!("Code search error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Search documents
pub async fn cmd_search_docs(
  query: &str,
  limit: usize,
  project: Option<&str>,
  json_output: bool,
  long_ids: bool,
) -> Result<()> {
  let cwd = project
    .map(std::path::PathBuf::from)
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| std::path::PathBuf::from("."));

  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = DocsSearchParams {
    query: query.to_string(),
    limit: Some(limit),
  };

  match client.call(params).await {
    Ok(chunks) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&chunks)?);
        return Ok(());
      }

      if chunks.is_empty() {
        println!("No documents found for: {}", query);
      } else {
        println!("Found {} document chunks:\n", chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
          println!(
            "{}. {} [{}]",
            i + 1,
            chunk.title,
            format_id(&chunk.document_id, long_ids)
          );

          let preview = if chunk.content.len() > 200 {
            format!("{}...", &chunk.content[..200])
          } else {
            chunk.content.clone()
          };
          println!("   {}", preview.replace('\n', "\n   "));

          if let Some(sim) = chunk.similarity {
            println!("   Similarity: {:.2}", sim);
          }
          println!();
        }

        // Help message about prefix matching
        if !long_ids {
          println!("Tip: Use --long to show full IDs. Prefixes (8+ chars) work in commands.");
        }
      }
    }
    Err(e) => {
      error!("Document search error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}
