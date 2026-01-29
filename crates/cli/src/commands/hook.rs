//! Hook command for handling hook events

use std::io::Read;

use anyhow::{Context, Result};
use ccengram::ipc::{Client, hook::HookParams};
use tracing::error;

/// Read hook input from stdin (JSON parameters from Claude Code)
fn read_hook_input() -> Result<serde_json::Value> {
  let mut input = String::new();
  std::io::stdin().read_to_string(&mut input)?;

  if input.trim().is_empty() {
    return Ok(serde_json::Value::Object(serde_json::Map::new()));
  }

  serde_json::from_str(&input).context("Invalid JSON in hook input")
}

/// Handle a hook event
pub async fn cmd_hook(name: &str) -> Result<()> {
  // Read input from stdin
  let input = read_hook_input().context("Failed to read hook input")?;

  // Try to connect to running daemon first
  if ccengram::dirs::is_daemon_running() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let cwd_str = cwd.to_string_lossy().to_string();
    let client = Client::connect(cwd).await.context("Failed to connect to daemon")?;

    let params = HookParams {
      hook_name: name.to_string(),
      session_id: None,
      cwd: Some(cwd_str),
      data: input,
    };

    match client.call(params).await {
      Ok(result) => {
        // Output the hook result
        println!("{}", serde_json::to_string(&result)?);
      }
      Err(e) => {
        error!("Hook error: {}", e);
      }
    }
  } else {
    // Daemon not running - can't process hook without daemon
    error!("Daemon is not running. Start with: ccengram daemon");
    std::process::exit(1);
  }

  Ok(())
}
