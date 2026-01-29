//! Migrate command for embedding migration

use anyhow::{Context, Result};
use ccengram::ipc::system::{MigrateEmbeddingParams, ProjectStatsParams};
use tracing::error;

/// Migrate embeddings to new dimensions/model
pub async fn cmd_migrate(dry_run: bool, force: bool) -> Result<()> {
  use ccengram::config::Config;

  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd.clone())
    .await
    .context("Failed to connect to daemon")?;

  // Load config to show what we're migrating to
  let config = Config::load_for_project(&cwd).await;

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
  let stats = client.call(ProjectStatsParams).await.context("Failed to get stats")?;

  let memory_count = stats.memories;
  let code_count = stats.code_chunks;
  let doc_count = stats.documents;

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

  // TODO: Note: force flag is handled at CLI level, daemon always performs migration
  let _ = force; // Acknowledge unused for now until ipc adds force field

  match client.call(MigrateEmbeddingParams).await {
    Ok(result) => {
      println!("Migration complete:");
      println!("  Migrated: {}", result.migrated);
      println!("  Message:  {}", result.message);
    }
    Err(e) => {
      error!("Migration error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}
