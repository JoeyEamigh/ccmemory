//! Watch command for file change monitoring

use anyhow::{Context, Result};
use ccengram::ipc::watch::{WatchStartParams, WatchStatusParams, WatchStopParams};
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
  _no_startup_scan: bool,
  _startup_scan_mode: Option<String>,
  _startup_scan_sync: bool,
) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd.clone())
    .await
    .context("Failed to connect to daemon")?;

  if stop {
    match client.call(WatchStopParams).await {
      Ok(result) => {
        println!("File watcher stopped: {}", result.status);
      }
      Err(e) => {
        error!("Stop error: {}", e);
        std::process::exit(1);
      }
    }
    return Ok(());
  }

  if status {
    match client.call(WatchStatusParams).await {
      Ok(result) => {
        println!("Watcher Status: {}", if result.running { "RUNNING" } else { "STOPPED" });

        if result.running {
          if result.scanning {
            println!("Startup Scan: IN PROGRESS");
            if let Some(progress) = result.scan_progress {
              println!("  Progress: {}/{}", progress[0], progress[1]);
            }
          }
          println!("Pending Changes: {}", result.pending_changes);
          println!("Project ID: {}", result.project_id);
          if let Some(root) = &result.root {
            println!("Root: {}", root);
          }
        }
      }
      Err(e) => {
        error!("Status error: {}", e);
        std::process::exit(1);
      }
    }
    return Ok(());
  }

  // Start watching
  match client.call(WatchStartParams).await {
    Ok(result) => {
      println!("File watcher started: {}", result.status);
      println!("Path: {}", result.path);
      println!("Project ID: {}", result.project_id);
      println!("Press Ctrl+C to stop watching");
    }
    Err(e) => {
      error!("Watch error: {}", e);
      std::process::exit(1);
    }
  }

  // Keep the CLI alive until interrupted
  tokio::signal::ctrl_c().await?;

  // Send stop command on exit
  let _ = client.call(WatchStopParams).await;

  println!("\nWatcher stopped");
  Ok(())
}
