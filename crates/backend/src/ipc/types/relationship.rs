//! Relationship IPC types - requests, responses, and conversions
use serde::{Deserialize, Serialize};

use super::memory::MemorySummary;

// ============================================================================
// Request types
// ============================================================================

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "data")]
pub enum RelationshipRequest {
  Add(RelationshipAddParams),
  List(RelationshipListParams),
  Delete(RelationshipDeleteParams),
  Related(RelationshipRelatedParams),
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipListParams {
  pub memory_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipAddParams {
  pub from_memory_id: String,
  pub to_memory_id: String,
  pub relationship_type: String,
  pub confidence: Option<f32>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipDeleteParams {
  pub relationship_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipRelatedParams {
  pub memory_id: String,
  pub limit: Option<usize>,
}

// ============================================================================
// Response types
// ============================================================================

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "data")]
pub enum RelationshipResponse {
  Add(RelationshipResult),
  List(Vec<RelationshipListItem>),
  Delete(DeletedResult),
  Related(Vec<RelatedMemoryItem>),
}

/// Relationship result (from add)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipResult {
  pub id: String,
  pub from_memory_id: String,
  pub to_memory_id: String,
  pub relationship_type: String,
  pub confidence: f32,
}

/// Relationship list item (with timestamps)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipListItem {
  pub id: String,
  pub from_memory_id: String,
  pub to_memory_id: String,
  pub relationship_type: String,
  pub confidence: f32,
  pub created_at: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub valid_until: Option<String>,
}

/// Delete result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedResult {
  pub deleted: bool,
}

/// Relationship info (for related memories)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipInfo {
  #[serde(rename = "type")]
  pub relationship_type: String,
  pub confidence: f32,
  pub direction: String,
}

/// Related memory with relationship info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedMemoryItem {
  pub memory: MemorySummary,
  pub relationship: RelationshipInfo,
}

// ============================================================================
// IpcRequest implementations
// ============================================================================

use crate::{
  impl_ipc_request,
  ipc::{RequestData, ResponseData},
};

impl_ipc_request!(
  RelationshipAddParams => RelationshipResult,
  ResponseData::Relationship(RelationshipResponse::Add(v)) => v,
  v => RequestData::Relationship(RelationshipRequest::Add(v)),
  v => ResponseData::Relationship(RelationshipResponse::Add(v))
);
impl_ipc_request!(
  RelationshipListParams => Vec<RelationshipListItem>,
  ResponseData::Relationship(RelationshipResponse::List(v)) => v,
  v => RequestData::Relationship(RelationshipRequest::List(v)),
  v => ResponseData::Relationship(RelationshipResponse::List(v))
);
impl_ipc_request!(
  RelationshipDeleteParams => DeletedResult,
  ResponseData::Relationship(RelationshipResponse::Delete(v)) => v,
  v => RequestData::Relationship(RelationshipRequest::Delete(v)),
  v => ResponseData::Relationship(RelationshipResponse::Delete(v))
);
impl_ipc_request!(
  RelationshipRelatedParams => Vec<RelatedMemoryItem>,
  ResponseData::Relationship(RelationshipResponse::Related(v)) => v,
  v => RequestData::Relationship(RelationshipRequest::Related(v)),
  v => ResponseData::Relationship(RelationshipResponse::Related(v))
);
