//! Migrate command for embedding migration

use anyhow::{Context, Result};
use daemon::{Request, connect_or_start};
use tracing::error;

/// Migrate embeddings to new dimensions/model
pub async fn cmd_migrate(dry_run: bool, force: bool) -> Result<()> {
  use engram_core::Config;

  let mut client = connect_or_start().await.context("Failed to connect to daemon")?;

  let cwd = std::env::current_dir()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|_| ".".to_string());

  // Load config to show what we're migrating to
  let config = Config::load_for_project(&std::path::PathBuf::from(&cwd));

  println!("Embedding Migration");
  println!("===================\n");
  println!("Target configuration:");
  println!("  Provider:   {:?}", config.embedding.provider);
  println!("  Model:      {}", config.embedding.model);
  println!("  Dimensions: {}", config.embedding.dimensions);
  println!();

  if dry_run {
    println!("DRY RUN - no changes will be made\n");
  }

  // Get current stats
  let stats_request = Request {
    id: Some(serde_json::json!(1)),
    method: "project_stats".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };

  let stats_response = client.request(stats_request).await.context("Failed to get stats")?;

  let mut memory_count = 0u64;
  let mut code_count = 0u64;
  let mut doc_count = 0u64;

  if let Some(stats) = stats_response.result {
    if let Some(memories) = stats.get("memories") {
      memory_count = memories.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    }
    if let Some(code) = stats.get("code") {
      code_count = code.get("total_chunks").and_then(|v| v.as_u64()).unwrap_or(0);
    }
    if let Some(docs) = stats.get("documents") {
      doc_count = docs.get("total_chunks").and_then(|v| v.as_u64()).unwrap_or(0);
    }
  }

  println!("Items to migrate:");
  println!("  Memories:       {}", memory_count);
  println!("  Code chunks:    {}", code_count);
  println!("  Document chunks: {}", doc_count);
  println!();

  let total = memory_count + code_count + doc_count;
  if total == 0 {
    println!("Nothing to migrate.");
    return Ok(());
  }

  if dry_run {
    println!("Would migrate {} total items.", total);
    return Ok(());
  }

  // Call the migration tool
  println!("Starting migration...\n");

  let request = Request {
    id: Some(serde_json::json!(1)),
    method: "migrate_embedding".to_string(),
    params: serde_json::json!({
        "cwd": cwd,
        "force": force,
    }),
  };

  let response = client.request(request).await.context("Failed to migrate embeddings")?;

  if let Some(err) = response.error {
    error!("Migration error: {}", err.message);
    std::process::exit(1);
  }

  if let Some(result) = response.result {
    let migrated = result.get("migrated_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let skipped = result.get("skipped_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let errors = result.get("error_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let duration_ms = result.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);

    println!("Migration complete:");
    println!("  Migrated: {}", migrated);
    println!("  Skipped:  {}", skipped);
    println!("  Errors:   {}", errors);
    println!("  Duration: {:.1}s", duration_ms as f64 / 1000.0);
  }

  Ok(())
}
