//! Log viewing commands

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Get the log directory path
fn log_dir() -> PathBuf {
  // Check XDG_DATA_HOME first
  if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
    return PathBuf::from(xdg_data).join("ccengram");
  }

  // Fall back to platform default
  dirs::data_local_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("ccengram")
}

/// View daemon logs
pub fn cmd_logs(follow: bool, lines: usize, date: Option<&str>, level: Option<&str>, open: bool) -> Result<()> {
  let log_directory = log_dir();

  // Handle --open flag: open log directory in file manager
  if open {
    #[cfg(target_os = "macos")]
    {
      Command::new("open").arg(&log_directory).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
      Command::new("xdg-open").arg(&log_directory).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
      Command::new("explorer").arg(&log_directory).spawn()?;
    }

    println!("Opening: {}", log_directory.display());
    return Ok(());
  }

  // Determine which log file to read
  let log_file = if let Some(d) = date {
    log_directory.join(format!("ccengram.log.{}", d))
  } else {
    log_directory.join("ccengram.log")
  };

  // Check if log file exists
  if !log_file.exists() {
    if date.is_some() {
      anyhow::bail!("Log file not found: {}", log_file.display());
    } else {
      println!("No log file found at: {}", log_file.display());
      println!("Daemon may not have run yet, or logs are in a different location.");
      println!();
      println!("Log directory: {}", log_directory.display());
      println!();
      println!("Available log files:");

      // List available log files
      if let Ok(entries) = std::fs::read_dir(&log_directory) {
        let mut found_logs = false;
        for entry in entries.flatten() {
          let name = entry.file_name();
          let name_str = name.to_string_lossy();
          if name_str.starts_with("ccengram") && name_str.contains("log") {
            println!("  {}", name_str);
            found_logs = true;
          }
        }
        if !found_logs {
          println!("  (none)");
        }
      }

      return Ok(());
    }
  }

  println!("Log file: {}", log_file.display());
  println!();

  if follow {
    // Use tail -f for following logs
    let mut cmd = Command::new("tail")
      .arg("-f")
      .arg("-n")
      .arg(lines.to_string())
      .arg(&log_file)
      .stdout(Stdio::piped())
      .spawn()
      .context("Failed to start tail command")?;

    if let Some(level_filter) = level {
      let level_upper = level_filter.to_uppercase();
      println!("Following logs (level: {})... (Ctrl+C to stop)", level_upper);
      println!();

      // Filter output by level
      if let Some(stdout) = cmd.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
          if let Ok(line) = line
            && line.to_uppercase().contains(&level_upper)
          {
            println!("{}", line);
          }
        }
      }
    } else {
      println!("Following logs... (Ctrl+C to stop)");
      println!();

      // Stream output directly
      if let Some(stdout) = cmd.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
          println!("{}", line);
        }
      }
    }

    cmd.wait()?;
  } else {
    // Read and optionally filter
    let content = std::fs::read_to_string(&log_file).context("Failed to read log file")?;
    let all_lines: Vec<&str> = content.lines().collect();

    let filtered: Vec<&str> = if let Some(level_filter) = level {
      let level_upper = level_filter.to_uppercase();
      all_lines
        .iter()
        .filter(|line| line.to_uppercase().contains(&level_upper))
        .copied()
        .collect()
    } else {
      all_lines
    };

    // Show last N lines
    let start = filtered.len().saturating_sub(lines);
    for line in &filtered[start..] {
      println!("{}", line);
    }

    println!();
    if let Some(level_filter) = level {
      println!(
        "Showing {} of {} {} entries",
        filtered.len() - start,
        filtered.len(),
        level_filter.to_uppercase()
      );
    } else {
      println!("Showing {} of {} lines", filtered.len() - start, filtered.len());
    }
  }

  Ok(())
}

/// List available log files
pub fn cmd_logs_list() -> Result<()> {
  let log_directory = log_dir();

  println!("Log Directory: {}", log_directory.display());
  println!();

  if !log_directory.exists() {
    println!("Log directory does not exist. Daemon may not have run yet.");
    return Ok(());
  }

  let mut log_files: Vec<_> = std::fs::read_dir(&log_directory)?
    .filter_map(|e| e.ok())
    .filter(|e| {
      let name = e.file_name();
      let name_str = name.to_string_lossy();
      name_str.starts_with("ccengram") && name_str.contains("log")
    })
    .collect();

  if log_files.is_empty() {
    println!("No log files found.");
    return Ok(());
  }

  // Sort by modification time (newest first)
  log_files.sort_by(|a, b| {
    let time_a = a.metadata().and_then(|m| m.modified()).ok();
    let time_b = b.metadata().and_then(|m| m.modified()).ok();
    time_b.cmp(&time_a)
  });

  println!("Log Files:");
  for entry in log_files {
    let name = entry.file_name();
    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
    let size_str = format_size(size);
    println!("  {:40} {}", name.to_string_lossy(), size_str);
  }

  Ok(())
}

fn format_size(bytes: u64) -> String {
  if bytes < 1024 {
    format!("{} B", bytes)
  } else if bytes < 1024 * 1024 {
    format!("{:.1} KB", bytes as f64 / 1024.0)
  } else {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
  }
}
