/// Get the default socket path
pub fn default_socket_path() -> std::path::PathBuf {
  // Try XDG_RUNTIME_DIR first, fallback to /tmp
  if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
    std::path::PathBuf::from(runtime_dir).join("ccengram.sock")
  } else {
    let uid = unsafe { libc::getuid() };
    std::path::PathBuf::from(format!("/tmp/{}.sock", uid))
  }
}

/// Check if the daemon is running at the default socket path.
pub fn is_daemon_running() -> bool {
  let socket_path = default_socket_path();
  std::os::unix::net::UnixStream::connect(socket_path).is_ok()
}

/// Get the default base path for CCEngram data
///
/// Respects the following environment variables (in order of precedence):
/// 1. DATA_DIR - explicit data directory override
/// 2. XDG_DATA_HOME - standard XDG data home directory
/// 3. dirs::data_local_dir() - platform default
pub fn default_data_dir() -> std::path::PathBuf {
  // Check explicit override first
  if let Ok(dir) = std::env::var("DATA_DIR") {
    return std::path::PathBuf::from(dir);
  }

  // Check XDG_DATA_HOME
  if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
    return std::path::PathBuf::from(xdg_data).join("ccengram");
  }

  // Fall back to platform default
  dirs::data_local_dir()
    .unwrap_or_else(|| std::path::PathBuf::from("."))
    .join("ccengram")
}

/// Get the default config directory
///
/// Respects the following environment variables (in order of precedence):
/// 1. CONFIG_DIR - explicit config directory override
/// 2. XDG_CONFIG_HOME - standard XDG config home directory
/// 3. dirs::config_dir() - platform default
pub fn default_config_dir() -> std::path::PathBuf {
  // Check explicit override first
  if let Ok(dir) = std::env::var("CONFIG_DIR") {
    return std::path::PathBuf::from(dir);
  }

  // Check XDG_CONFIG_HOME
  if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
    return std::path::PathBuf::from(xdg_config).join("ccengram");
  }

  // Fall back to platform default
  dirs::config_dir()
    .unwrap_or_else(|| std::path::PathBuf::from("."))
    .join("ccengram")
}

/// Get the default cache directory
///
/// Respects the following environment variables (in order of precedence):
/// 1. XDG_CACHE_HOME - standard XDG cache home directory
/// 2. dirs::cache_dir() - platform default
pub fn default_cache_dir() -> std::path::PathBuf {
  // Check XDG_CACHE_HOME
  if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
    return std::path::PathBuf::from(xdg_cache).join("ccengram");
  }

  // Fall back to platform default
  dirs::cache_dir()
    .unwrap_or_else(|| std::path::PathBuf::from("."))
    .join("ccengram")
}
