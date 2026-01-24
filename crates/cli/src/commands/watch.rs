//! Watch command for file change monitoring

use anyhow::{Context, Result};
use daemon::{Request, connect_or_start};
use tracing::error;

/// Watch for file changes
///
/// # Arguments
/// * `stop` - Stop any running watcher
/// * `status` - Check watcher status
/// * `no_startup_scan` - Skip startup scan (don't reconcile with filesystem)
/// * `startup_scan_mode` - Startup scan mode: deleted_only, deleted_and_new, full
/// * `startup_scan_sync` - Wait for startup scan to complete before watching
pub async fn cmd_watch(
  stop: bool,
  status: bool,
  no_startup_scan: bool,
  startup_scan_mode: Option<String>,
  startup_scan_sync: bool,
) -> Result<()> {
  let mut client = connect_or_start().await.context("Failed to connect to daemon")?;

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
      let is_scanning = result.get("scanning").and_then(|v| v.as_bool()).unwrap_or(false);
      println!("Watcher Status: {}", if is_running { "RUNNING" } else { "STOPPED" });

      if is_running {
        if is_scanning {
          println!("Startup Scan: IN PROGRESS");
          if let Some(progress) = result.get("scan_progress")
            && let (Some(processed), Some(total)) = (
              progress.get(0).and_then(|v| v.as_u64()),
              progress.get(1).and_then(|v| v.as_u64()),
            )
          {
            println!("  Progress: {}/{}", processed, total);
          }
        }
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

  // Build params for watch_start
  let mut params = serde_json::json!({ "cwd": cwd });

  // Add startup scan options if provided
  if no_startup_scan {
    params["startup_scan"] = serde_json::json!(false);
  }
  if let Some(ref mode) = startup_scan_mode {
    params["startup_scan_mode"] = serde_json::json!(mode);
  }
  if startup_scan_sync {
    params["startup_scan_blocking"] = serde_json::json!(true);
  }

  // Start watching
  let request = Request {
    id: Some(serde_json::json!(1)),
    method: "watch_start".to_string(),
    params,
  };

  let response = client.request(request).await.context("Failed to start watcher")?;

  if let Some(err) = response.error {
    error!("Watch error: {}", err.message);
    std::process::exit(1);
  }

  println!("File watcher started for {}", cwd);
  if !no_startup_scan {
    if startup_scan_sync {
      println!("Startup scan completed (blocking mode)");
    } else {
      println!("Startup scan running in background");
    }
  }
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
