//! Context retrieval commands for code and document chunks

use anyhow::{Context, Result};
use ccengram::ipc::{code::CodeContextParams, docs::DocContextParams};
use tracing::error;

/// Get context around a chunk (auto-detects code vs document)
pub async fn cmd_context(chunk_id: &str, before: Option<usize>, after: Option<usize>, json_output: bool) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  // Try code_context first
  let code_result = client
    .call(CodeContextParams {
      chunk_id: chunk_id.to_string(),
      before,
      after,
    })
    .await;

  // Check if code_context succeeded
  if let Ok(result) = code_result {
    if json_output {
      println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
      print_code_context(&result);
    }
    return Ok(());
  }

  // If code_context failed, try doc_context
  let doc_result = client
    .call(DocContextParams {
      doc_id: chunk_id.to_string(),
      before: Some(before.unwrap_or(1)),
      after: Some(after.unwrap_or(1)),
    })
    .await;

  match doc_result {
    Ok(result) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
      } else {
        print_doc_context(&result);
      }
      Ok(())
    }
    Err(doc_err) => {
      // Both failed - show error
      error!("Chunk not found in code or documents. Error: {}", doc_err);
      std::process::exit(1);
    }
  }
}

/// Print code context in a readable format
fn print_code_context(result: &ccengram::ipc::code::CodeContextResponse) {
  println!("File: {} ({})", result.file_path, result.language);
  println!("Total lines: {}", result.total_file_lines);
  if let Some(warning) = &result.warning {
    println!("Warning: {}", warning);
  }
  println!();

  // Print before section
  if !result.context.before.content.is_empty() {
    println!("--- Before (line {}) ---", result.context.before.start_line);
    println!("{}", result.context.before.content);
    println!();
  }

  // Print target section
  println!(
    ">>> Target (lines {}-{}) <<<",
    result.context.target.start_line, result.context.target.end_line
  );
  println!("{}", result.context.target.content);
  println!();

  // Print after section
  if !result.context.after.content.is_empty() {
    println!("--- After (line {}) ---", result.context.after.start_line);
    println!("{}", result.context.after.content);
  }
}

/// Print document context in a readable format
fn print_doc_context(result: &ccengram::ipc::docs::DocContextResult) {
  println!("Document: {}", result.title);
  println!("Source: {}", result.source);
  println!("Total chunks: {}", result.total_chunks);
  println!();

  // Print before chunks
  for chunk in &result.context.before {
    println!("--- Chunk {} ---", chunk.chunk_index);
    println!("{}", chunk.content);
    println!();
  }

  // Print target chunk
  println!(">>> Chunk {} (target) <<<", result.context.target.chunk_index);
  println!("{}", result.context.target.content);
  println!();

  // Print after chunks
  for chunk in &result.context.after {
    println!("--- Chunk {} ---", chunk.chunk_index);
    println!("{}", chunk.content);
    println!();
  }
}
