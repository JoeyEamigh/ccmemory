//! Project management commands (list, show, clean)

use std::io::Write;

use anyhow::{Context, Result};
use ccengram::ipc::project::{ProjectCleanAllParams, ProjectCleanParams, ProjectInfoParams, ProjectListParams};
use tracing::error;

/// List all indexed projects
pub async fn cmd_projects_list(json_output: bool) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  match client.call(ProjectListParams).await {
    Ok(projects) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&projects)?);
        return Ok(());
      }

      if projects.is_empty() {
        println!("No projects indexed.");
        return Ok(());
      }

      println!("Indexed Projects ({})", projects.len());
      println!("==================\n");

      for project in &projects {
        // Truncate ID for display
        let short_id = if project.id.len() > 8 {
          &project.id[..8]
        } else {
          &project.id
        };

        println!("{} [{}]", project.name, short_id);
        println!("  Path: {}", project.path);
        println!();
      }
    }
    Err(e) => {
      error!("Error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Show details for a specific project
pub async fn cmd_projects_show(project: &str, json_output: bool) -> Result<()> {
  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = ProjectInfoParams {
    project: Some(project.to_string()),
  };

  match client.call(params).await {
    Ok(info) => {
      if json_output {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
      }

      println!("Project Details");
      println!("===============\n");

      println!("ID:           {}", info.id);
      println!("Path:         {}", info.path);
      println!("Name:         {}", info.name);

      println!();
      println!("Statistics:");
      println!("  Memories:     {}", info.memory_count);
      println!("  Code Chunks:  {}", info.code_chunk_count);
      println!("  Documents:    {}", info.document_count);
      println!("  Sessions:     {}", info.session_count);

      println!();
      println!("Database Path: {}", info.db_path);
    }
    Err(e) => {
      error!("Error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Remove a project's data
pub async fn cmd_projects_clean(project: &str, force: bool) -> Result<()> {
  if !force {
    print!("Remove all data for project '{}'? [y/N] ", project);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
      println!("Cancelled.");
      return Ok(());
    }
  }

  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  let params = ProjectCleanParams {
    project: Some(project.to_string()),
  };

  match client.call(params).await {
    Ok(result) => {
      println!("Removed project: {}", result.path);
      println!("  Memories deleted: {}", result.memories_deleted);
      println!("  Code chunks deleted: {}", result.code_chunks_deleted);
      println!("  Documents deleted: {}", result.documents_deleted);
    }
    Err(e) => {
      error!("Error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Remove all project data
pub async fn cmd_projects_clean_all(force: bool) -> Result<()> {
  if !force {
    print!("Remove ALL project data? This cannot be undone! [y/N] ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
      println!("Cancelled.");
      return Ok(());
    }
  }

  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  match client.call(ProjectCleanAllParams).await {
    Ok(result) => {
      println!("Removed {} projects", result.projects_removed);
    }
    Err(e) => {
      error!("Error: {}", e);
      std::process::exit(1);
    }
  }

  Ok(())
}
