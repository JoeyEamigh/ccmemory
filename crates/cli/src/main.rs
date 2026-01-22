//! CCEngram CLI - Intelligent memory and code search for Claude Code

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;
mod logging;
mod mcp;

use commands::{
  cmd_agent, cmd_archive, cmd_config, cmd_daemon, cmd_delete, cmd_export, cmd_health, cmd_hook, cmd_index,
  cmd_migrate, cmd_search, cmd_search_code, cmd_search_docs, cmd_show, cmd_stats, cmd_tui, cmd_update, cmd_watch,
};
use logging::{init_cli_logging, init_daemon_logging};
use mcp::cmd_mcp;

#[derive(Parser)]
#[command(name = "ccengram")]
#[command(about = "Intelligent memory and code search for Claude Code")]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

/// Subcommands for `ccengram index`
#[derive(Subcommand)]
pub enum IndexCommand {
  /// Index code files in the project
  Code {
    /// Force re-index all files
    #[arg(long)]
    force: bool,
    /// Show index statistics
    #[arg(long)]
    stats: bool,
    /// Export index to file
    #[arg(long, value_name = "FILE")]
    export: Option<String>,
    /// Load index from file
    #[arg(long, value_name = "FILE")]
    load: Option<String>,
  },
  /// Index documents from a directory
  Docs {
    /// Directory to index (default: configured docs.directory)
    #[arg(short, long)]
    directory: Option<String>,
    /// Force re-index all documents
    #[arg(long)]
    force: bool,
    /// Show document index statistics
    #[arg(long)]
    stats: bool,
  },
  /// Index a single file (auto-detects code vs document)
  File {
    /// File path to index
    path: String,
    /// Document title (optional, for documents only)
    #[arg(short, long)]
    title: Option<String>,
    /// Force re-index even if unchanged
    #[arg(long)]
    force: bool,
  },
}

#[derive(Subcommand)]
enum Commands {
  /// Start the daemon
  Daemon {
    #[arg(long)]
    foreground: bool,
  },
  /// MCP server (for Claude Code integration)
  Mcp,
  /// Handle hook event
  Hook { name: String },
  /// Search memories
  Search {
    query: String,
    #[arg(short, long, default_value = "10")]
    limit: usize,
    /// Project path (default: current directory)
    #[arg(short, long)]
    project: Option<String>,
    /// Filter by sector (episodic, semantic, procedural, emotional, reflective)
    #[arg(long)]
    sector: Option<String>,
    /// Filter by memory type (preference, codebase, decision, gotcha, pattern)
    #[arg(long, name = "type")]
    memory_type: Option<String>,
    /// Minimum salience threshold (0.0-1.0)
    #[arg(long)]
    min_salience: Option<f32>,
    /// Include superseded memories
    #[arg(long)]
    include_superseded: bool,
    /// Filter by scope path prefix
    #[arg(long)]
    scope: Option<String>,
    /// Use semantic (vector) search only (default, this flag is for compatibility)
    #[arg(long)]
    semantic: bool,
    /// Use keyword search only (not supported - emits warning)
    #[arg(long)]
    keywords: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
  },
  /// Search code (semantic search over indexed code)
  SearchCode {
    query: String,
    #[arg(short, long, default_value = "10")]
    limit: usize,
    /// Project path (default: current directory)
    #[arg(short, long)]
    project: Option<String>,
    /// Filter by programming language (rust, python, typescript, etc.)
    #[arg(long)]
    language: Option<String>,
    /// Filter by chunk type (function, class, module, block, import)
    #[arg(long, name = "type")]
    chunk_type: Option<String>,
    /// Filter by file path prefix
    #[arg(long)]
    path: Option<String>,
    /// Filter by symbol name
    #[arg(long)]
    symbol: Option<String>,
    /// Use semantic (vector) search only (default, this flag is for compatibility)
    #[arg(long)]
    semantic: bool,
    /// Use keyword search only (not supported - emits warning)
    #[arg(long)]
    keywords: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
  },
  /// Search documents (semantic search over ingested docs)
  SearchDocs {
    query: String,
    #[arg(short, long, default_value = "10")]
    limit: usize,
    /// Project path (default: current directory)
    #[arg(short, long)]
    project: Option<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
  },
  /// Show detailed memory by ID
  Show {
    /// Memory ID to show
    memory_id: String,
    /// Include related memories
    #[arg(long)]
    related: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
  },
  /// Delete a memory
  Delete {
    /// Memory ID to delete
    memory_id: String,
    /// Permanently delete (hard delete)
    #[arg(long)]
    hard: bool,
  },
  /// Export memories to file
  Export {
    /// Output file path
    #[arg(short, long)]
    output: Option<String>,
    /// Output format (json or csv)
    #[arg(short, long, default_value = "json")]
    format: String,
  },
  /// Manage code and document index
  Index {
    #[command(subcommand)]
    command: Option<IndexCommand>,
  },
  /// Watch for file changes and update index
  Watch {
    /// Stop any running watcher
    #[arg(long)]
    stop: bool,
    /// Check watcher status
    #[arg(long)]
    status: bool,
  },
  /// Archive old low-salience memories
  Archive {
    /// Archive memories older than this date (YYYY-MM-DD)
    #[arg(long)]
    before: Option<String>,
    /// Minimum salience threshold (archive below this)
    #[arg(long, default_value = "0.1")]
    threshold: f32,
    /// Preview what would be archived without making changes
    #[arg(long)]
    dry_run: bool,
  },
  /// Show statistics
  Stats,
  /// Health check
  Health,
  /// Show or set configuration
  Config {
    /// Configuration key to get/set
    key: Option<String>,
    /// Value to set (if provided with key)
    value: Option<String>,
    /// Reset all configuration to defaults
    #[arg(long)]
    reset: bool,
    /// List all configuration keys
    #[arg(long)]
    list: bool,
    /// Initialize project config file (.claude/ccengram.toml)
    #[arg(long)]
    init: bool,
    /// Tool preset for init: minimal, standard (default), or full
    #[arg(long, default_value = "standard")]
    preset: String,
    /// Show current effective config
    #[arg(long)]
    show: bool,
  },
  /// Check for updates or update to latest version
  Update {
    /// Only check for updates without installing
    #[arg(long)]
    check: bool,
    /// Specific version to update to
    #[arg(long)]
    version: Option<String>,
  },
  /// Migrate embeddings to new dimensions/model
  Migrate {
    /// Preview what would be migrated without making changes
    #[arg(long)]
    dry_run: bool,
    /// Force re-embed even if dimensions match
    #[arg(long)]
    force: bool,
  },
  /// Generate a MemExplore subagent for Claude Code
  Agent {
    /// Output path (default: .claude/agents/MemExplore.md)
    #[arg(long)]
    output: Option<String>,
    /// Overwrite existing file
    #[arg(long)]
    force: bool,
  },
  /// Launch interactive TUI
  Tui {
    /// Project path (default: current directory)
    #[arg(short, long)]
    project: Option<PathBuf>,
  },
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();

  // Use file logging for daemon, console-only for other commands
  let _guard = match &cli.command {
    Commands::Daemon { .. } => init_daemon_logging(),
    _ => {
      init_cli_logging();
      None
    }
  };

  match cli.command {
    Commands::Daemon { foreground: _ } => cmd_daemon().await,
    Commands::Mcp => cmd_mcp().await,
    Commands::Hook { name } => cmd_hook(&name).await,
    Commands::Search {
      query,
      limit,
      project,
      sector,
      memory_type,
      min_salience,
      include_superseded,
      scope,
      semantic: _,
      keywords,
      json,
    } => {
      if keywords {
        eprintln!("Warning: --keywords mode is not supported in this version. Using semantic search.");
      }
      cmd_search(
        &query,
        limit,
        project.as_deref(),
        sector.as_deref(),
        memory_type.as_deref(),
        min_salience,
        include_superseded,
        scope.as_deref(),
        json,
      )
      .await
    }
    Commands::SearchCode {
      query,
      limit,
      project,
      language,
      chunk_type,
      path,
      symbol,
      semantic: _,
      keywords,
      json,
    } => {
      if keywords {
        eprintln!("Warning: --keywords mode is not supported in this version. Using semantic search.");
      }
      cmd_search_code(
        &query,
        limit,
        project.as_deref(),
        language.as_deref(),
        chunk_type.as_deref(),
        path.as_deref(),
        symbol.as_deref(),
        json,
      )
      .await
    }
    Commands::SearchDocs {
      query,
      limit,
      project,
      json,
    } => cmd_search_docs(&query, limit, project.as_deref(), json).await,
    Commands::Show {
      memory_id,
      related,
      json,
    } => cmd_show(&memory_id, related, json).await,
    Commands::Delete { memory_id, hard } => cmd_delete(&memory_id, hard).await,
    Commands::Export { output, format } => cmd_export(output.as_deref(), &format).await,
    Commands::Index { command } => cmd_index(command).await,
    Commands::Watch { stop, status } => cmd_watch(stop, status).await,
    Commands::Archive {
      before,
      threshold,
      dry_run,
    } => cmd_archive(before.as_deref(), threshold, dry_run).await,
    Commands::Stats => cmd_stats().await,
    Commands::Health => cmd_health().await,
    Commands::Config {
      key,
      value,
      reset,
      list,
      init,
      preset,
      show,
    } => cmd_config(key, value, reset, list, init, &preset, show).await,
    Commands::Update { check, version } => cmd_update(check, version).await,
    Commands::Migrate { dry_run, force } => cmd_migrate(dry_run, force).await,
    Commands::Agent { output, force } => cmd_agent(output.as_deref(), force).await,
    Commands::Tui { project } => cmd_tui(project).await,
  }
}
