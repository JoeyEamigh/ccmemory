//! Administrative commands (stats, health, archive, config)

use anyhow::{Context, Result};
use daemon::{Client, Request, default_socket_path, is_running};
use tracing::error;

/// Show statistics
pub async fn cmd_stats() -> Result<()> {
  let socket_path = default_socket_path();

  if !is_running(&socket_path) {
    error!("Daemon is not running. Start it with: ccengram daemon");
    std::process::exit(1);
  }

  let mut client = Client::connect_to(&socket_path)
    .await
    .context("Failed to connect to daemon")?;

  // Get daemon status
  let status_request = Request {
    id: Some(serde_json::json!(1)),
    method: "status".to_string(),
    params: serde_json::json!({}),
  };

  let status_response = client.request(status_request).await.context("Failed to get status")?;

  println!("CCEngram Statistics");
  println!("===================\n");

  // Print daemon status
  if let Some(result) = status_response.result {
    if let Some(version) = result.get("version").and_then(|v| v.as_str()) {
      println!("Version: {}", version);
    }
    if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
      println!("Status: {}", status);
    }
    if let Some(projects) = result.get("projects").and_then(|v| v.as_u64()) {
      println!("Active projects: {}", projects);
    }
  }

  // Get project-specific stats for current directory
  let cwd = std::env::current_dir()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|_| ".".to_string());

  let stats_request = Request {
    id: Some(serde_json::json!(2)),
    method: "project_stats".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };

  let stats_response = client
    .request(stats_request)
    .await
    .context("Failed to get project stats")?;

  if let Some(err) = stats_response.error {
    error!("Stats error: {}", err.message);
    std::process::exit(1);
  }

  if let Some(stats) = stats_response.result {
    println!("\n--- Memory Statistics ---");

    if let Some(memories) = stats.get("memories") {
      let total = memories.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
      println!("Total memories: {}", total);

      // By sector
      if let Some(by_sector) = memories.get("by_sector").and_then(|v| v.as_object()) {
        println!("\nBy sector:");
        for (sector, count) in by_sector {
          let cnt = count.as_u64().unwrap_or(0);
          if cnt > 0 {
            println!("  {}: {}", sector, cnt);
          }
        }
      }

      // By tier
      if let Some(by_tier) = memories.get("by_tier").and_then(|v| v.as_object()) {
        println!("\nBy tier:");
        for (tier, count) in by_tier {
          let cnt = count.as_u64().unwrap_or(0);
          if cnt > 0 {
            println!("  {}: {}", tier, cnt);
          }
        }
      }

      // Salience distribution
      if let Some(by_salience) = memories.get("by_salience") {
        println!("\nSalience distribution:");
        let high = by_salience.get("high").and_then(|v| v.as_u64()).unwrap_or(0);
        let medium = by_salience.get("medium").and_then(|v| v.as_u64()).unwrap_or(0);
        let low = by_salience.get("low").and_then(|v| v.as_u64()).unwrap_or(0);
        let very_low = by_salience.get("very_low").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("  high (>=0.7): {}", high);
        println!("  medium (0.4-0.7): {}", medium);
        println!("  low (0.2-0.4): {}", low);
        println!("  very_low (<0.2): {}", very_low);
      }

      let superseded = memories.get("superseded_count").and_then(|v| v.as_u64()).unwrap_or(0);
      if superseded > 0 {
        println!("\nSuperseded memories: {}", superseded);
      }
    }

    println!("\n--- Code Index Statistics ---");

    if let Some(code) = stats.get("code") {
      let total_chunks = code.get("total_chunks").and_then(|v| v.as_u64()).unwrap_or(0);
      let total_files = code.get("total_files").and_then(|v| v.as_u64()).unwrap_or(0);
      println!("Total chunks: {}", total_chunks);
      println!("Total files: {}", total_files);

      // By language
      if let Some(by_language) = code.get("by_language").and_then(|v| v.as_object())
        && !by_language.is_empty()
      {
        println!("\nBy language:");
        let mut langs: Vec<_> = by_language.iter().collect();
        langs.sort_by(|a, b| b.1.as_u64().unwrap_or(0).cmp(&a.1.as_u64().unwrap_or(0)));
        for (lang, count) in langs.iter().take(10) {
          let cnt = count.as_u64().unwrap_or(0);
          if cnt > 0 {
            println!("  {}: {}", lang, cnt);
          }
        }
      }

      // Recent activity
      if let Some(recent) = code.get("recent_indexed").and_then(|v| v.as_array())
        && !recent.is_empty()
      {
        println!("\nRecent index activity:");
        for item in recent.iter().take(5) {
          let file_path = item.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
          let chunks = item.get("chunks").and_then(|v| v.as_u64()).unwrap_or(0);
          println!("  {} ({} chunks)", file_path, chunks);
        }
      }
    }

    println!("\n--- Document Statistics ---");

    if let Some(docs) = stats.get("documents") {
      let total = docs.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
      let total_chunks = docs.get("total_chunks").and_then(|v| v.as_u64()).unwrap_or(0);
      println!("Total documents: {}", total);
      println!("Total document chunks: {}", total_chunks);
    }

    println!("\n--- Entity Statistics ---");

    if let Some(entities) = stats.get("entities") {
      let total = entities.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
      println!("Total entities: {}", total);

      if let Some(by_type) = entities.get("by_type").and_then(|v| v.as_object())
        && !by_type.is_empty()
      {
        println!("\nBy type:");
        for (entity_type, count) in by_type {
          let cnt = count.as_u64().unwrap_or(0);
          if cnt > 0 {
            println!("  {}: {}", entity_type, cnt);
          }
        }
      }
    }
  }

  Ok(())
}

/// Health check
pub async fn cmd_health() -> Result<()> {
  let socket_path = default_socket_path();

  println!("CCEngram Health Check");
  println!("=====================\n");

  // Check if daemon is running
  if !is_running(&socket_path) {
    println!("Daemon:     NOT RUNNING");
    println!("Socket:     {:?}", socket_path);
    println!("\nStart the daemon with: ccengram daemon");
    std::process::exit(1);
  }

  let mut client = Client::connect_to(&socket_path)
    .await
    .context("Failed to connect to daemon")?;

  // Ping test
  let ping_request = Request {
    id: Some(serde_json::json!(1)),
    method: "ping".to_string(),
    params: serde_json::json!({}),
  };

  let ping_ok = match client.request(ping_request).await {
    Ok(response) => response.error.is_none(),
    Err(_) => false,
  };

  if ping_ok {
    println!("Daemon:     HEALTHY");
  } else {
    println!("Daemon:     UNHEALTHY (ping failed)");
    std::process::exit(1);
  }
  println!("Socket:     {:?}", socket_path);

  // Get comprehensive health status
  let cwd = std::env::current_dir()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|_| ".".to_string());

  let health_request = Request {
    id: Some(serde_json::json!(2)),
    method: "health_check".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };

  if let Ok(response) = client.request(health_request).await
    && let Some(health) = response.result
  {
    // Database status
    println!("\n--- Database ---");
    if let Some(db) = health.get("database") {
      let status = db.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
      if status == "healthy" {
        println!("Status:     HEALTHY");
      } else {
        let error = db.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
        println!("Status:     ERROR ({})", error);
      }
    }

    // Ollama status
    println!("\n--- Ollama ---");
    if let Some(ollama) = health.get("ollama") {
      let available = ollama.get("available").and_then(|v| v.as_bool()).unwrap_or(false);
      if available {
        println!("Status:     AVAILABLE");
        let models_count = ollama.get("models_count").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("Models:     {} available", models_count);
        let configured = ollama
          .get("configured_model")
          .and_then(|v| v.as_str())
          .unwrap_or("unknown");
        let model_available = ollama
          .get("configured_model_available")
          .and_then(|v| v.as_bool())
          .unwrap_or(false);
        println!(
          "Configured: {} ({})",
          configured,
          if model_available { "available" } else { "NOT FOUND" }
        );
      } else {
        println!("Status:     NOT AVAILABLE");
        println!("            Make sure Ollama is running: ollama serve");
      }
    }

    // Embedding status
    println!("\n--- Embedding Service ---");
    if let Some(embed) = health.get("embedding") {
      let configured = embed.get("configured").and_then(|v| v.as_bool()).unwrap_or(false);
      if configured {
        let provider = embed.get("provider").and_then(|v| v.as_str()).unwrap_or("unknown");
        let model = embed.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");
        let dimensions = embed.get("dimensions").and_then(|v| v.as_u64()).unwrap_or(0);
        let available = embed.get("available").and_then(|v| v.as_bool()).unwrap_or(false);

        println!("Provider:   {}", provider);
        println!("Model:      {}", model);
        println!("Dimensions: {}", dimensions);
        println!("Status:     {}", if available { "AVAILABLE" } else { "NOT AVAILABLE" });
      } else {
        println!("Status:     NOT CONFIGURED");
      }
    }
  }

  Ok(())
}

/// Archive old low-salience memories
pub async fn cmd_archive(before: Option<&str>, threshold: f32, dry_run: bool) -> Result<()> {
  let socket_path = default_socket_path();

  if !is_running(&socket_path) {
    error!("Daemon is not running. Start it with: ccengram daemon");
    std::process::exit(1);
  }

  let mut client = Client::connect_to(&socket_path)
    .await
    .context("Failed to connect to daemon")?;

  let cwd = std::env::current_dir()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|_| ".".to_string());

  // First, get all memories to find archival candidates
  let request = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_list".to_string(),
    params: serde_json::json!({
        "cwd": cwd,
    }),
  };

  let response = client.request(request).await.context("Failed to list memories")?;

  if let Some(err) = response.error {
    error!("Archive error: {}", err.message);
    std::process::exit(1);
  }

  let Some(memories) = response.result else {
    println!("No memories found");
    return Ok(());
  };

  let Some(arr) = memories.as_array() else {
    println!("No memories found");
    return Ok(());
  };

  // Parse the before date if provided
  let before_date: Option<chrono::NaiveDateTime> = before.and_then(|s| {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
      .ok()
      .and_then(|d| d.and_hms_opt(0, 0, 0))
  });

  let mut candidates: Vec<(String, f32, String)> = Vec::new();

  for mem in arr {
    let salience = mem.get("salience").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;

    // Skip if above threshold
    if salience >= threshold {
      continue;
    }

    // Check date if specified
    if let Some(cutoff) = before_date
      && let Some(created) = mem.get("created_at").and_then(|v| v.as_str())
      && let Ok(mem_date) = chrono::DateTime::parse_from_rfc3339(created)
    {
      let mem_naive: chrono::NaiveDateTime = mem_date.naive_utc();
      if mem_naive >= cutoff {
        continue;
      }
    }

    let id = mem.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let content = mem.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let summary = if content.len() > 60 {
      format!("{}...", &content[..60])
    } else {
      content
    };

    candidates.push((id, salience, summary));
  }

  if candidates.is_empty() {
    println!(
      "No memories match archival criteria (salience < {}{})",
      threshold,
      before.map(|d| format!(", before {}", d)).unwrap_or_default()
    );
    return Ok(());
  }

  println!("Found {} memories to archive:", candidates.len());
  println!();

  for (id, salience, summary) in &candidates {
    println!("  [{:.2}] {} - {}", salience, &id[..8], summary.replace('\n', " "));
  }
  println!();

  if dry_run {
    println!("Dry run - no changes made");
    return Ok(());
  }

  // Archive (soft delete) each memory
  let mut archived = 0;
  for (id, _, _) in candidates {
    let request = Request {
      id: Some(serde_json::json!(1)),
      method: "memory_delete".to_string(),
      params: serde_json::json!({
          "memory_id": id,
          "cwd": cwd,
          "hard": false,
      }),
    };

    match client.request(request).await {
      Ok(resp) if resp.error.is_none() => archived += 1,
      _ => error!("Failed to archive memory {}", id),
    }
  }

  println!("Archived {} memories (soft deleted)", archived);
  Ok(())
}

/// Show current effective configuration
pub async fn cmd_config_show() -> Result<()> {
  use engram_core::Config;

  let cwd = std::env::current_dir()?;
  let config = Config::load_for_project(&cwd);

  // Check which config file is being used
  let project_config = Config::project_config_path(&cwd);
  let user_config = Config::user_config_path();

  println!("Effective configuration for: {:?}", cwd);
  println!();

  if project_config.exists() {
    println!("Using project config: {:?}", project_config);
  } else if let Some(ref user_path) = user_config {
    if user_path.exists() {
      println!("Using user config: {:?}", user_path);
    } else {
      println!("Using default configuration (no config file found)");
    }
  } else {
    println!("Using default configuration");
  }
  println!();

  // Show config as TOML
  let toml_str = toml::to_string_pretty(&config)?;
  println!("{}", toml_str);

  Ok(())
}

/// Initialize project configuration file
pub async fn cmd_config_init(preset: &str) -> Result<()> {
  use engram_core::{Config, ToolPreset};

  let cwd = std::env::current_dir()?;
  let config_path = Config::project_config_path(&cwd);

  if config_path.exists() {
    error!("Config file already exists: {:?}", config_path);
    println!("Delete it first if you want to regenerate");
    std::process::exit(1);
  }

  // Parse preset
  let tool_preset = match preset.to_lowercase().as_str() {
    "minimal" => ToolPreset::Minimal,
    "standard" => ToolPreset::Standard,
    "full" => ToolPreset::Full,
    _ => {
      error!("Invalid preset: {}. Use minimal, standard, or full", preset);
      std::process::exit(1);
    }
  };

  // Create .claude directory if needed
  if let Some(parent) = config_path.parent() {
    std::fs::create_dir_all(parent)?;
  }

  // Generate and write config
  let template = Config::generate_template(tool_preset);
  std::fs::write(&config_path, &template)?;

  println!("Created project config: {:?}", config_path);
  println!();
  println!("Tool preset: {}", preset);
  println!("Edit the file to customize settings.");

  Ok(())
}

/// Reset user configuration to defaults
pub async fn cmd_config_reset() -> Result<()> {
  use engram_core::{Config, ToolPreset};

  if let Some(user_config_path) = Config::user_config_path() {
    if let Some(parent) = user_config_path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let template = Config::generate_template(ToolPreset::Standard);
    std::fs::write(&user_config_path, &template)?;
    println!("Reset user config to defaults: {:?}", user_config_path);
  } else {
    error!("Could not determine user config path");
    std::process::exit(1);
  }

  Ok(())
}
