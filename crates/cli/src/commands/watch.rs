//! Watch command for file change monitoring

use anyhow::{Context, Result};
use daemon::{Client, Request, default_socket_path, is_running};
use tracing::error;

/// Watch for file changes
pub async fn cmd_watch(stop: bool, status: bool) -> Result<()> {
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

  if stop {
    let request = Request {
      id: Some(serde_json::json!(1)),
      method: "watch_stop".to_string(),
      params: serde_json::json!({ "cwd": cwd }),
    };

    let response = client.request(request).await.context("Failed to stop watcher")?;

    if let Some(err) = response.error {
      error!("Stop error: {}", err.message);
      std::process::exit(1);
    }

    println!("File watcher stopped");
    return Ok(());
  }

  if status {
    let request = Request {
      id: Some(serde_json::json!(1)),
      method: "watch_status".to_string(),
      params: serde_json::json!({ "cwd": cwd }),
    };

    let response = client.request(request).await.context("Failed to get watcher status")?;

    if let Some(err) = response.error {
      error!("Status error: {}", err.message);
      std::process::exit(1);
    }

    if let Some(result) = response.result {
      let is_running = result.get("running").and_then(|v| v.as_bool()).unwrap_or(false);
      println!("Watcher Status: {}", if is_running { "RUNNING" } else { "STOPPED" });

      if is_running {
        if let Some(paths) = result.get("watched_paths").and_then(|v| v.as_u64()) {
          println!("Watched Paths: {}", paths);
        }
        if let Some(changes) = result.get("pending_changes").and_then(|v| v.as_u64()) {
          println!("Pending Changes: {}", changes);
        }
      }
    }
    return Ok(());
  }

  // Start watching
  let request = Request {
    id: Some(serde_json::json!(1)),
    method: "watch_start".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };

  let response = client.request(request).await.context("Failed to start watcher")?;

  if let Some(err) = response.error {
    error!("Watch error: {}", err.message);
    std::process::exit(1);
  }

  println!("File watcher started for {}", cwd);
  println!("Press Ctrl+C to stop watching");

  // Keep the CLI alive until interrupted
  tokio::signal::ctrl_c().await?;

  // Send stop command on exit
  let stop_request = Request {
    id: Some(serde_json::json!(1)),
    method: "watch_stop".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let _ = client.request(stop_request).await;

  println!("\nWatcher stopped");
  Ok(())
}
