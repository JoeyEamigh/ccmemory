//! Tool observation memory creation.
//!
//! This module creates episodic memories from significant tool uses,
//! forming the "tool trail" that tracks what Claude did during a session.

use std::collections::HashSet;

use tracing::debug;
use uuid::Uuid;

use crate::{
  context::memory::extract::dedup::compute_hashes,
  db::ProjectDb,
  domain::memory::{Memory, Sector},
  embedding::EmbeddingProvider,
  service::util::ServiceError,
};

/// Context for observation creation.
pub struct ObservationContext<'a> {
  /// Project database connection
  pub db: &'a ProjectDb,
  /// Optional embedding provider
  pub embedding: &'a dyn EmbeddingProvider,
  /// Project UUID
  pub project_id: Uuid,
}

impl<'a> ObservationContext<'a> {
  /// Create a new observation context
  pub fn new(db: &'a ProjectDb, embedding: &'a dyn EmbeddingProvider, project_id: Uuid) -> Self {
    Self {
      db,
      embedding,
      project_id,
    }
  }

  /// Get an embedding for the given text
  async fn get_embedding(&self, text: &str) -> Result<Vec<f32>, ServiceError> {
    // Document mode - we're embedding observation content for storage
    Ok(
      self
        .embedding
        .embed(text, crate::embedding::EmbeddingMode::Document)
        .await?,
    )
  }
}

/// Result of creating a tool observation
pub struct ObservationResult {
  /// ID of the created memory, if any
  pub memory_id: Option<String>,
}

/// Create an episodic memory from a tool observation.
///
/// This captures significant tool uses as immediate episodic memories
/// for the "tool trail" - a record of what Claude did during the session.
///
/// # Arguments
/// * `ctx` - Observation context with database and providers
/// * `tool_name` - Name of the tool used
/// * `tool_params` - Parameters passed to the tool
/// * `tool_result` - Optional result from the tool
/// * `seen_hashes` - Set of already-seen content hashes for deduplication
///
/// # Returns
/// * `Ok(ObservationResult)` - Result with optional memory ID
/// * `Err(ServiceError)` - If storage fails
pub async fn create_tool_observation(
  ctx: &ObservationContext<'_>,
  tool_name: &str,
  tool_params: &serde_json::Value,
  tool_result: Option<&serde_json::Value>,
  seen_hashes: &mut HashSet<String>,
) -> Result<ObservationResult, ServiceError> {
  // Format the observation based on tool type
  let observation = match tool_name {
    "Read" => {
      let Some(path) = tool_params.get("file_path").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      format!("Read file: {}", path)
    }
    "Edit" => {
      let Some(path) = tool_params.get("file_path").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      let old_str = tool_params.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
      let new_str = tool_params.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
      let old_preview = if old_str.len() > 50 { &old_str[..50] } else { old_str };
      let new_preview = if new_str.len() > 50 { &new_str[..50] } else { new_str };
      format!("Edited {}: '{}...' -> '{}...'", path, old_preview, new_preview)
    }
    "Write" => {
      let Some(path) = tool_params.get("file_path").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      format!("Created/wrote file: {}", path)
    }
    "Bash" => {
      let Some(cmd) = tool_params.get("command").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      let exit_code = tool_result
        .and_then(|r| r.get("exit_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
      let cmd_preview = if cmd.len() > 80 {
        format!("{}...", &cmd[..80])
      } else {
        cmd.to_string()
      };
      if exit_code == 0 {
        format!("Ran command: {}", cmd_preview)
      } else {
        format!("Command failed (exit {}): {}", exit_code, cmd_preview)
      }
    }
    "Grep" | "Glob" => {
      let Some(pattern) = tool_params.get("pattern").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      let path = tool_params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
      format!("Searched '{}' in {}", pattern, path)
    }
    "WebFetch" => {
      let Some(url) = tool_params.get("url").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      format!("Fetched URL: {}", url)
    }
    "WebSearch" => {
      let Some(query) = tool_params.get("query").and_then(|v| v.as_str()) else {
        return Ok(ObservationResult { memory_id: None });
      };
      format!("Web search: {}", query)
    }
    // Skip tools that don't produce meaningful observations
    "TodoWrite" | "AskUserQuestion" | "Task" | "Skill" | "EnterPlanMode" | "ExitPlanMode" => {
      return Ok(ObservationResult { memory_id: None });
    }
    _ => {
      // Generic observation for other tools
      format!("Used tool: {}", tool_name)
    }
  };

  // Skip if observation is too short
  if observation.len() < 15 {
    return Ok(ObservationResult { memory_id: None });
  }

  // Compute hashes for dedup
  let (content_hash, simhash) = compute_hashes(&observation);

  // Check for duplicates
  if seen_hashes.contains(&content_hash) {
    debug!(
      "Skipping duplicate tool observation: {}",
      &observation[..observation.len().min(50)]
    );
    return Ok(ObservationResult { memory_id: None });
  }

  // Create episodic memory (tool trail memories are always episodic)
  let mut memory = Memory::new(ctx.project_id, observation.clone(), Sector::Episodic);
  memory.memory_type = None; // Tool observations don't have a specific memory type
  memory.importance = 0.3; // Lower importance for tool observations
  memory.salience = 0.4; // Medium salience - they decay faster
  memory.content_hash = content_hash.clone();
  memory.simhash = simhash;

  // Extract file paths from tool params
  let files = extract_tool_file_paths(tool_params);
  if !files.is_empty() {
    memory.files = files;
  }

  // Get embedding
  let embedding = ctx.get_embedding(&memory.content).await?;

  // Store the memory
  ctx.db.add_memory(&memory, &embedding).await?;

  // Track hash
  seen_hashes.insert(content_hash);

  debug!(
    "Created tool observation memory: {} ({})",
    memory.id,
    &memory.content[..memory.content.len().min(50)]
  );
  Ok(ObservationResult {
    memory_id: Some(memory.id.to_string()),
  })
}

/// Extract file paths from tool parameters.
fn extract_tool_file_paths(params: &serde_json::Value) -> Vec<String> {
  let mut files = Vec::new();

  // Common file path keys
  for key in ["file_path", "notebook_path", "path", "source"] {
    if let Some(path) = params.get(key).and_then(|v| v.as_str())
      && !path.is_empty()
      && !files.contains(&path.to_string())
    {
      files.push(path.to_string());
    }
  }

  files
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_extract_tool_file_paths() {
    let params = serde_json::json!({
      "file_path": "/src/main.rs",
      "path": "/other/path.rs"
    });
    let files = extract_tool_file_paths(&params);
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"/src/main.rs".to_string()));
  }

  #[test]
  fn test_extract_tool_file_paths_empty() {
    let params = serde_json::json!({
      "query": "test"
    });
    let files = extract_tool_file_paths(&params);
    assert!(files.is_empty());
  }

  #[test]
  fn test_extract_tool_file_paths_dedup() {
    let params = serde_json::json!({
      "file_path": "/src/main.rs",
      "source": "/src/main.rs"
    });
    let files = extract_tool_file_paths(&params);
    assert_eq!(files.len(), 1);
  }
}
