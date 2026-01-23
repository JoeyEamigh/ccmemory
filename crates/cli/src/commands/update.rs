//! Update command for self-updating the CLI

use anyhow::{Context, Result};
use serde::Deserialize;

const GITHUB_REPO: &str = "joey-goodjob/ccengram";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct Release {
  tag_name: String,
  name: String,
  html_url: String,
  assets: Vec<Asset>,
  prerelease: bool,
  draft: bool,
}

#[derive(Deserialize)]
struct Asset {
  name: String,
  browser_download_url: String,
}

/// Check for updates or update to latest version
pub async fn cmd_update(check_only: bool, target_version: Option<String>) -> Result<()> {
  println!("CCEngram v{}", CURRENT_VERSION);
  println!();

  // Fetch latest release info from GitHub API
  let client = reqwest::Client::builder().user_agent("ccengram-updater").build()?;

  let releases_url = format!("https://api.github.com/repos/{}/releases", GITHUB_REPO);

  let response = client
    .get(&releases_url)
    .send()
    .await
    .context("Failed to fetch releases from GitHub")?;

  if !response.status().is_success() {
    anyhow::bail!("Failed to fetch releases: HTTP {}", response.status());
  }

  let releases: Vec<Release> = response.json().await?;

  // Filter out prereleases and drafts
  let stable_releases: Vec<_> = releases.iter().filter(|r| !r.prerelease && !r.draft).collect();

  if stable_releases.is_empty() {
    println!("No releases found");
    return Ok(());
  }

  // Find target version or use latest
  let target = if let Some(ref ver) = target_version {
    stable_releases
      .iter()
      .find(|r| r.tag_name.trim_start_matches('v') == ver.trim_start_matches('v'))
      .copied()
      .ok_or_else(|| anyhow::anyhow!("Version {} not found", ver))?
  } else {
    stable_releases[0]
  };

  let target_ver = target.tag_name.trim_start_matches('v');

  // Compare versions
  let current_parts: Vec<u32> = CURRENT_VERSION.split('.').filter_map(|p| p.parse().ok()).collect();
  let target_parts: Vec<u32> = target_ver.split('.').filter_map(|p| p.parse().ok()).collect();

  let needs_update = target_parts
    .iter()
    .zip(current_parts.iter().chain(std::iter::repeat(&0)))
    .any(|(t, c)| t > c)
    || target_parts.len() > current_parts.len();

  if !needs_update {
    println!("You are running the latest version (v{})", CURRENT_VERSION);
    return Ok(());
  }

  println!("New version available: v{} -> v{}", CURRENT_VERSION, target_ver);
  println!("  Release: {}", target.name);
  println!("  URL: {}", target.html_url);
  println!();

  if check_only {
    println!("Run 'ccengram update' to install the update");
    return Ok(());
  }

  // Determine platform-specific asset name
  let os = std::env::consts::OS;
  let arch = std::env::consts::ARCH;

  let platform = match (os, arch) {
    ("linux", "x86_64") => "linux-x86_64",
    ("linux", "aarch64") => "linux-aarch64",
    ("macos", "x86_64") => "darwin-x86_64",
    ("macos", "aarch64") => "darwin-aarch64",
    ("windows", "x86_64") => "windows-x86_64.exe",
    _ => {
      println!("Unsupported platform: {} {}", os, arch);
      println!("Please download manually from: {}", target.html_url);
      return Ok(());
    }
  };

  let asset_name = format!("ccengram-{}", platform);
  let asset = target
    .assets
    .iter()
    .find(|a| a.name.starts_with(&asset_name))
    .ok_or_else(|| {
      anyhow::anyhow!(
        "No binary found for platform {}. Available: {:?}",
        platform,
        target.assets.iter().map(|a| &a.name).collect::<Vec<_>>()
      )
    })?;

  println!("Downloading: {}", asset.name);

  // Download the binary
  let download_response = client.get(&asset.browser_download_url).send().await?;

  if !download_response.status().is_success() {
    anyhow::bail!("Failed to download: HTTP {}", download_response.status());
  }

  let bytes = download_response.bytes().await?;

  // Get current executable path
  let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
  let backup_path = current_exe.with_extension("bak");

  // Backup current binary
  println!("Backing up current binary...");
  std::fs::rename(&current_exe, &backup_path).context("Failed to backup current binary")?;

  // Write new binary
  println!("Installing new version...");
  std::fs::write(&current_exe, &bytes).context("Failed to write new binary")?;

  // Set executable permissions on Unix
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&current_exe, perms)?;
  }

  println!();
  println!("Successfully updated to v{}", target_ver);
  println!();
  println!("Backup saved to: {:?}", backup_path);

  Ok(())
}
