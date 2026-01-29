//! Watch IPC types - requests and responses
use serde::{Deserialize, Serialize};

use crate::{
  impl_ipc_request,
  ipc::{RequestData, ResponseData},
};

// ============================================================================
// Request types
// ============================================================================

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "data")]
pub enum WatchRequest {
  Start(WatchStartParams),
  Stop(WatchStopParams),
  Status(WatchStatusParams),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchStartParams;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchStopParams;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchStatusParams;

// ============================================================================
// Response types
// ============================================================================

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "data")]
pub enum WatchResponse {
  Status(WatchStatusResult),
  Start(WatchStartResult),
  Stop(WatchStopResult),
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStatusResult {
  pub running: bool,
  pub root: Option<String>,
  pub pending_changes: usize,
  pub project_id: String,
  pub scanning: bool,
  pub scan_progress: Option<[usize; 2]>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStartResult {
  pub status: String,
  pub path: String,
  pub project_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStopResult {
  pub status: String,
  pub path: String,
  pub project_id: String,
}

impl_ipc_request!(
  WatchStartParams => WatchStartResult,
  ResponseData::Watch(WatchResponse::Start(v)) => v,
  v => RequestData::Watch(WatchRequest::Start(v)),
  v => ResponseData::Watch(WatchResponse::Start(v))
);
impl_ipc_request!(
  WatchStopParams => WatchStopResult,
  ResponseData::Watch(WatchResponse::Stop(v)) => v,
  v => RequestData::Watch(WatchRequest::Stop(v)),
  v => ResponseData::Watch(WatchResponse::Stop(v))
);
impl_ipc_request!(
  WatchStatusParams => WatchStatusResult,
  ResponseData::Watch(WatchResponse::Status(v)) => v,
  v => RequestData::Watch(WatchRequest::Status(v)),
  v => ResponseData::Watch(WatchResponse::Status(v))
);
