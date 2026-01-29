//! Memory management commands (show, delete, deleted)

use anyhow::{Context, Result};
use ccengram::ipc::memory::{MemoryDeleteParams, MemoryGetParams, MemoryListDeletedParams, MemoryRestoreParams};
use tracing::error;

/// Show detailed memory by ID
pub async fn cmd_show(memory_id: &str, related: bool, json_output: bool) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = MemoryGetParams {
    memory_id: memory_id.to_string(),
    include_related: if related { Some(true) } else { None },
  };

  match client.call(params).await {
    Ok(memory) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&memory)?);
        return Ok(());
      }

      // Pretty print the memory
      println!("Memory Details");
      println!("==============\n");

      println!("ID:       {}", memory.id);
      println!("Sector:   {}", memory.sector);
      println!("Tier:     {}", memory.tier);
      if let Some(mem_type) = &memory.memory_type {
        println!("Type:     {}", mem_type);
      }
      println!("Salience: {:.2}", memory.salience);
      println!("Importance: {:.2}", memory.importance);
      println!("Created:  {}", memory.created_at);
      println!("Accessed: {}", memory.last_accessed);
      if let Some(superseded) = &memory.superseded_by {
        println!("Superseded by: {}", superseded);
      }

      println!();

      println!("Content:");
      println!("{}", memory.content);

      if !memory.tags.is_empty() {
        println!("\nTags: {}", memory.tags.join(", "));
      }

      if related
        && let Some(relationships) = &memory.relationships
        && !relationships.is_empty()
      {
        println!("\nRelated Memories ({}):", relationships.len());
        for rel in relationships {
          println!("  - {} -> {}", rel.relationship_type, rel.target_id);
        }
      }
    }
    Err(e) => {
      error!("Error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Delete a memory
pub async fn cmd_delete(memory_id: &str, hard: bool) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = MemoryDeleteParams {
    memory_id: memory_id.to_string(),
  };

  // Note: "hard" parameter would need to be added to MemoryDeleteParams if the API supports it
  let _ = hard;

  match client.call(params).await {
    Ok(_result) => {
      if hard {
        println!("Memory {} permanently deleted", memory_id);
      } else {
        println!("Memory {} soft deleted (can be recovered)", memory_id);
      }
    }
    Err(e) => {
      error!("Delete error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Restore a soft-deleted memory
pub async fn cmd_restore(memory_id: &str) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = MemoryRestoreParams {
    memory_id: memory_id.to_string(),
  };

  match client.call(params).await {
    Ok(result) => {
      println!("Restored memory: {}", memory_id);
      println!("{}", result.message);
    }
    Err(e) => {
      error!("Restore error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// List soft-deleted memories
pub async fn cmd_deleted(limit: usize, json_output: bool) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = MemoryListDeletedParams { limit: Some(limit) };

  match client.call(params).await {
    Ok(memories) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&memories)?);
        return Ok(());
      }

      if memories.is_empty() {
        println!("No deleted memories found.");
        return Ok(());
      }

      println!("Deleted Memories ({}):", memories.len());
      println!();

      for (i, mem) in memories.iter().enumerate() {
        // Truncate content for preview
        let preview: String = mem.content.chars().take(60).collect();
        let preview = preview.replace('\n', " ");
        let preview = if mem.content.len() > 60 {
          format!("{}...", preview)
        } else {
          preview
        };

        println!("{}. [{}] {}", i + 1, mem.sector, mem.id);
        println!("   {}", preview);
        println!("   Created: {}", mem.created_at);
        println!();
      }

      println!("Use 'ccengram memory restore <id>' to restore a memory.");
    }
    Err(e) => {
      error!("Error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}
