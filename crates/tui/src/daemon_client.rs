use anyhow::{Context, Result};
use daemon::{Client, Response};
use ipc::{
  CodeListParams, CodeSearchParams, CodeStatsParams, DocsSearchParams, EntityGetParams, EntityListParams,
  EntityTopParams, HealthCheckParams, MemoryDeemphasizeParams, MemoryGetParams, MemoryListParams,
  MemoryReinforceParams, MemorySearchParams, Method, MetricsParams, PingParams, ProjectStatsParams,
  RelationshipListParams, ShutdownParams, StatusParams, WatchStatusParams,
};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use tracing::debug;

/// Wrapper around daemon client for TUI-specific operations
pub struct DaemonClient {
  client: Client,
  project_path: PathBuf,
}

impl DaemonClient {
  pub async fn connect(project_path: PathBuf) -> Result<Self> {
    let client = Client::connect().await.context("Failed to connect to daemon")?;

    Ok(Self { client, project_path })
  }

  /// Get the cwd parameter for requests
  fn cwd(&self) -> String {
    self.project_path.to_string_lossy().to_string()
  }

  /// Make a typed request to the daemon
  async fn call_typed<P: Serialize>(&mut self, method: Method, params: P) -> Result<Response> {
    let method_str = serde_json::to_value(method)
      .ok()
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| format!("{:?}", method).to_lowercase());

    let params_value = serde_json::to_value(params).context("Failed to serialize params")?;

    debug!("Calling daemon method: {}", method_str);
    self
      .client
      .call(&method_str, params_value)
      .await
      .map_err(|e| anyhow::anyhow!("{}", e))
  }

  /// Ping the daemon to check if it's responsive
  pub async fn ping(&mut self) -> Result<bool> {
    match self.call_typed(Method::Ping, PingParams).await {
      Ok(response) => Ok(response.error.is_none()),
      Err(_) => Ok(false),
    }
  }

  /// Get daemon status
  pub async fn status(&mut self) -> Result<Value> {
    let params = StatusParams { cwd: Some(self.cwd()) };
    let response = self.call_typed(Method::Status, params).await?;
    response.result.ok_or_else(|| {
      let msg = response
        .error
        .map(|e| e.message)
        .unwrap_or_else(|| "Unknown error".to_string());
      anyhow::anyhow!("{}", msg)
    })
  }

  /// Get project statistics
  pub async fn project_stats(&mut self) -> Result<Value> {
    let params = ProjectStatsParams { cwd: Some(self.cwd()) };
    let response = self.call_typed(Method::ProjectStats, params).await?;
    response.result.ok_or_else(|| {
      let msg = response
        .error
        .map(|e| e.message)
        .unwrap_or_else(|| "Unknown error".to_string());
      anyhow::anyhow!("{}", msg)
    })
  }

  /// Get health check
  pub async fn health_check(&mut self) -> Result<Value> {
    let params = HealthCheckParams;
    let response = self.call_typed(Method::HealthCheck, params).await?;
    response.result.ok_or_else(|| {
      let msg = response
        .error
        .map(|e| e.message)
        .unwrap_or_else(|| "Unknown error".to_string());
      anyhow::anyhow!("{}", msg)
    })
  }

  /// Search memories
  pub async fn memory_search(&mut self, query: &str, limit: usize) -> Result<Vec<Value>> {
    let params = MemorySearchParams {
      query: query.to_string(),
      cwd: Some(self.cwd()),
      limit: Some(limit),
      ..Default::default()
    };
    let response = self.call_typed(Method::MemorySearch, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// List memories
  pub async fn memory_list(&mut self, limit: usize, offset: usize) -> Result<Vec<Value>> {
    let params = MemoryListParams {
      cwd: Some(self.cwd()),
      limit: Some(limit),
      offset: Some(offset),
      ..Default::default()
    };
    let response = self.call_typed(Method::MemoryList, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Get a single memory by ID
  pub async fn memory_get(&mut self, memory_id: &str) -> Result<Value> {
    let params = MemoryGetParams {
      memory_id: memory_id.to_string(),
      cwd: Some(self.cwd()),
      include_related: Some(true),
    };
    let response = self.call_typed(Method::MemoryGet, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response.result.ok_or_else(|| anyhow::anyhow!("Memory not found"))
  }

  /// Reinforce a memory
  pub async fn memory_reinforce(&mut self, memory_id: &str) -> Result<()> {
    let params = MemoryReinforceParams {
      memory_id: memory_id.to_string(),
      cwd: Some(self.cwd()),
      amount: Some(0.2),
    };
    let response = self.call_typed(Method::MemoryReinforce, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    Ok(())
  }

  /// Deemphasize a memory
  pub async fn memory_deemphasize(&mut self, memory_id: &str) -> Result<()> {
    let params = MemoryDeemphasizeParams {
      memory_id: memory_id.to_string(),
      cwd: Some(self.cwd()),
      amount: Some(0.2),
    };
    let response = self.call_typed(Method::MemoryDeemphasize, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    Ok(())
  }

  /// Search code
  pub async fn code_search(&mut self, query: &str, limit: usize) -> Result<Vec<Value>> {
    let params = CodeSearchParams {
      query: query.to_string(),
      cwd: Some(self.cwd()),
      limit: Some(limit),
      ..Default::default()
    };
    let response = self.call_typed(Method::CodeSearch, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// List code chunks
  pub async fn code_list(&mut self) -> Result<Vec<Value>> {
    let params = CodeListParams {
      cwd: Some(self.cwd()),
      ..Default::default()
    };
    let response = self.call_typed(Method::CodeList, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Get code stats
  pub async fn code_stats(&mut self) -> Result<Value> {
    let params = CodeStatsParams { cwd: Some(self.cwd()) };
    let response = self.call_typed(Method::CodeStats, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Search documents
  pub async fn docs_search(&mut self, query: &str, limit: usize) -> Result<Vec<Value>> {
    let params = DocsSearchParams {
      query: query.to_string(),
      cwd: Some(self.cwd()),
      limit: Some(limit),
    };
    let response = self.call_typed(Method::DocsSearch, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// List entities
  pub async fn entity_list(&mut self, limit: usize) -> Result<Vec<Value>> {
    let params = EntityListParams {
      cwd: Some(self.cwd()),
      limit: Some(limit),
    };
    let response = self.call_typed(Method::EntityList, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Get top entities
  pub async fn entity_top(&mut self, limit: usize) -> Result<Vec<Value>> {
    let params = EntityTopParams {
      cwd: Some(self.cwd()),
      limit: Some(limit),
    };
    let response = self.call_typed(Method::EntityTop, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Get entity details
  pub async fn entity_get(&mut self, entity_id: &str) -> Result<Value> {
    let params = EntityGetParams {
      entity_id: entity_id.to_string(),
      cwd: Some(self.cwd()),
    };
    let response = self.call_typed(Method::EntityGet, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response.result.ok_or_else(|| anyhow::anyhow!("Entity not found"))
  }

  /// List relationships for a memory
  pub async fn relationship_list(&mut self, memory_id: &str) -> Result<Vec<Value>> {
    let params = RelationshipListParams {
      memory_id: memory_id.to_string(),
      cwd: Some(self.cwd()),
    };
    let response = self.call_typed(Method::RelationshipList, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Get memory timeline
  pub async fn memory_timeline(&mut self, limit: usize) -> Result<Vec<Value>> {
    // Note: The daemon's memory_timeline expects an anchor_id, but for TUI session listing
    // we use memory_list with a limit to get recent memories as a workaround.
    // This maintains backward compatibility with existing TUI behavior.
    let params = MemoryListParams {
      cwd: Some(self.cwd()),
      limit: Some(limit),
      ..Default::default()
    };
    let response = self.call_typed(Method::MemoryList, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    response
      .result
      .and_then(|v| v.as_array().cloned())
      .ok_or_else(|| anyhow::anyhow!("Invalid response format"))
  }

  /// Shutdown the daemon
  pub async fn shutdown(&mut self) -> Result<()> {
    debug!("Sending shutdown request to daemon");
    let params = ShutdownParams;
    let response = self.call_typed(Method::Shutdown, params).await?;

    if let Some(err) = response.error {
      return Err(anyhow::anyhow!("{}", err.message));
    }

    Ok(())
  }

  /// Get file watcher status
  pub async fn watch_status(&mut self) -> Result<Value> {
    let params = WatchStatusParams { cwd: Some(self.cwd()) };
    let response = self.call_typed(Method::WatchStatus, params).await?;
    response.result.ok_or_else(|| {
      let msg = response
        .error
        .map(|e| e.message)
        .unwrap_or_else(|| "Unknown error".to_string());
      anyhow::anyhow!("{}", msg)
    })
  }

  /// Get daemon metrics
  pub async fn metrics(&mut self) -> Result<Value> {
    let params = MetricsParams;
    let response = self.call_typed(Method::Metrics, params).await?;
    response.result.ok_or_else(|| {
      let msg = response
        .error
        .map(|e| e.message)
        .unwrap_or_else(|| "Unknown error".to_string());
      anyhow::anyhow!("{}", msg)
    })
  }
}
