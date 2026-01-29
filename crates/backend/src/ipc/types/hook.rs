//! Hook IPC types - results from hook event handlers
use serde::{Deserialize, Serialize};

use crate::{
  impl_ipc_request,
  ipc::{RequestData, ResponseData},
};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookParams {
  pub hook_name: String,
  pub session_id: Option<String>,
  pub cwd: Option<String>,
  #[serde(flatten)]
  pub data: serde_json::Value, // Hook-specific data varies widely
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
  #[serde(flatten)]
  pub data: serde_json::Value, // Hook results vary
}

/// Result from SessionStart hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartHookResult {
  pub status: String,
  pub project_id: String,
  pub project_name: String,
  pub project_path: String,
  pub watcher_started: bool,
}

/// Result from SessionEnd hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndHookResult {
  pub status: String,
  pub memories_created: Vec<String>,
  pub memories_promoted: usize,
}

/// Result from UserPromptSubmit hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptHookResult {
  pub status: String,
  pub memories_created: Vec<String>,
}

/// Result from PostToolUse hook
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostToolUseHookResult {
  pub status: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub observation_memory_id: Option<String>,
}

/// Result from PreCompact hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCompactHookResult {
  pub status: String,
  pub background_extraction: bool,
  pub memories_created: Vec<String>,
}

/// Result from Stop hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopHookResult {
  pub status: String,
  pub background_extraction: bool,
  pub memories_created: Vec<String>,
}

/// Simple status-only hook result (SubagentStop, Notification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleHookResult {
  pub status: String,
}

impl_ipc_request!(
  HookParams => HookResult,
  ResponseData::Hook(v) => v,
  v => RequestData::Hook(v),
  v => ResponseData::Hook(v)
);
