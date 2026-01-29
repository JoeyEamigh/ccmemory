//! Daemon command

use anyhow::{Context, Result, bail};
use ccengram::{
  config::EmbeddingProvider,
  ipc::{Client, system::ShutdownParams},
};
use tracing::{error, info};

/// Start the daemon
///
/// # Arguments
/// * `stop` - Stop the running daemon
/// * `foreground` - Run in foreground mode (disables auto-shutdown, logs to console)
/// * `background` - Run in background mode (enables auto-shutdown, for auto-start)
/// * `embedding_provider` - Override embedding provider (ollama or openrouter)
/// * `openrouter_api_key` - Override OpenRouter API key
pub async fn cmd_daemon(
  stop: bool,
  foreground: bool,
  background: bool,
  embedding_provider: Option<String>,
  openrouter_api_key: Option<String>,
) -> Result<()> {
  let socket_path = ccengram::dirs::default_socket_path();

  // Handle --stop flag
  if stop {
    if !ccengram::dirs::is_daemon_running() {
      println!("Daemon is not running");
      return Ok(());
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let client = Client::connect(cwd).await.context("Failed to connect to daemon")?;

    match client.call(ShutdownParams).await {
      Ok(_) => {
        println!("Daemon stopped");
      }
      Err(e) => {
        error!("Shutdown error: {}", e);
        std::process::exit(1);
      }
    }

    return Ok(());
  }

  // --foreground and --background are mutually exclusive
  if foreground && background {
    bail!("Cannot specify both --foreground and --background");
  }

  // If --background, we're being spawned by auto-start
  // Check if already running and exit silently
  if ccengram::dirs::is_daemon_running() {
    if background {
      // Another instance is handling it, exit silently
      return Ok(());
    }
    bail!("Daemon is already running at {:?}", socket_path);
  }

  // Create config based on mode
  let mut config = ccengram::RuntimeConfig::load().await;
  config.foreground = foreground;

  // Apply embedding provider overrides
  if let Some(provider) = embedding_provider {
    match provider.to_lowercase().as_str() {
      "ollama" => {
        config.config.embedding.provider = EmbeddingProvider::Ollama;
        if config.config.embedding.model == "qwen/qwen3-embedding-8b" {
          config.config.embedding.model = "qwen3-embedding".to_string();
          config.config.embedding.dimensions = 4096;
        }
        info!("Using Ollama embedding provider (override)");
      }
      "openrouter" => {
        config.config.embedding.provider = EmbeddingProvider::OpenRouter;
        if config.config.embedding.model == "qwen3-embedding" {
          config.config.embedding.model = "qwen/qwen3-embedding-8b".to_string();
          config.config.embedding.dimensions = 4096;
        }
        info!("Using OpenRouter embedding provider (override)");
      }
      other => bail!("Unknown embedding provider: {}. Use 'ollama' or 'openrouter'", other),
    }
  }

  if let Some(key) = openrouter_api_key {
    config.config.embedding.openrouter_api_key = Some(key);
  }

  info!("Starting CCEngram daemon");
  let pid = ccengram::Daemon::spawn(config).await?;
  info!("Daemon started with PID {}", pid);

  Ok(())
}
