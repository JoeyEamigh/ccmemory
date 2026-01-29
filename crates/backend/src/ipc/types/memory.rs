//! Memory IPC types - requests, responses, and conversions
use serde::{Deserialize, Serialize};

use crate::domain::memory::Memory;

// ============================================================================
// Request types
// ============================================================================

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "data")]
pub enum MemoryRequest {
  Search(MemorySearchParams),
  Get(MemoryGetParams),
  Add(MemoryAddParams),
  List(MemoryListParams),
  Reinforce(MemoryReinforceParams),
  Deemphasize(MemoryDeemphasizeParams),
  ListDeleted(MemoryListDeletedParams),
  Delete(MemoryDeleteParams),
  HardDelete(MemoryHardDeleteParams),
  Restore(MemoryRestoreParams),
  Supersede(MemorySupersedeParams),
  Timeline(MemoryTimelineParams),
  Related(MemoryRelatedParams),
  SetSalience(MemorySetSalienceParams),
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemorySearchParams {
  pub query: String,
  pub sector: Option<String>,
  pub tier: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
  pub memory_type: Option<String>,
  pub min_salience: Option<f32>,
  pub scope_path: Option<String>,
  pub scope_module: Option<String>,
  pub session_id: Option<String>,
  pub limit: Option<usize>,
  #[serde(default)]
  pub include_superseded: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAddParams {
  pub content: String,
  pub sector: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
  pub memory_type: Option<String>,
  pub context: Option<String>,
  pub tags: Option<Vec<String>>,
  pub categories: Option<Vec<String>>,
  pub scope_path: Option<String>,
  pub scope_module: Option<String>,
  pub importance: Option<f32>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGetParams {
  pub memory_id: String,
  pub include_related: Option<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListParams {
  pub sector: Option<String>,
  pub limit: Option<usize>,
  pub offset: Option<usize>,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReinforceParams {
  pub memory_id: String,
  pub amount: Option<f32>,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDeemphasizeParams {
  pub memory_id: String,
  pub amount: Option<f32>,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemorySupersedeParams {
  pub old_memory_id: String,
  pub new_content: String,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRelatedParams {
  pub memory_id: String,
  pub limit: Option<usize>,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryListDeletedParams {
  pub limit: Option<usize>,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDeleteParams {
  pub memory_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHardDeleteParams {
  pub memory_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemorySetSalienceParams {
  pub memory_id: String,
  pub salience: f32,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRestoreParams {
  pub memory_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTimelineParams {
  pub memory_id: String,
}

// ============================================================================
// Response types
// ============================================================================

/// Re-export SearchQuality from code types for use in memory search results.
pub use super::code::SearchQuality;

#[allow(clippy::large_enum_variant)]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "data")]
pub enum MemoryResponse {
  Search(MemorySearchResult),
  Get(MemoryFullDetail),
  Add(MemoryAddResult),
  Update(MemoryUpdateResult),
  Delete(MemoryDeleteResult),
  List(Vec<MemoryItem>),
  Timeline(MemoryTimelineResult),
  Related(MemoryRelatedResult),
  Supersede(MemorySupersedeResult),
  Restore(MemoryRestoreResult),
  ListDeleted(Vec<MemoryItem>),
}

/// Memory search result with items and quality metadata.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
  /// The search result items
  pub items: Vec<MemoryItem>,
  /// Search quality metadata. When `low_confidence` is true, consider
  /// refining the query for better results.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub search_quality: Option<SearchQuality>,
}

/// Memory item for search and list results
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
  pub id: String,
  pub content: String,
  pub sector: String,
  pub tier: String,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub summary: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory_type: Option<String>,

  // Scores - only in search results
  #[serde(skip_serializing_if = "Option::is_none")]
  pub similarity: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub rank_score: Option<f32>,

  pub salience: f32,
  pub importance: f32,

  pub is_superseded: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub superseded_by: Option<String>,

  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tags: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub categories: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope_path: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope_module: Option<String>,

  pub created_at: String,
  pub last_accessed: String,
}

/// Full memory detail response
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFullDetail {
  pub id: String,
  pub content: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub summary: Option<String>,
  pub sector: String,
  pub tier: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory_type: Option<String>,
  pub salience: f32,
  pub importance: f32,
  pub confidence: f32,
  pub access_count: u32,
  pub is_deleted: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub superseded_by: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tags: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub categories: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub concepts: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub files: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub context: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope_path: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub scope_module: Option<String>,
  pub created_at: String,
  pub updated_at: String,
  pub last_accessed: String,
  pub valid_from: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub valid_until: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub relationships: Option<Vec<MemoryRelationshipItem>>,
}

/// Memory relationship in get response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRelationshipItem {
  #[serde(rename = "type")]
  pub relationship_type: String,
  pub from_id: String,
  pub to_id: String,
  pub target_id: String,
  pub confidence: f32,
}

/// Memory summary (for related memories)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
  pub id: String,
  pub content: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub summary: Option<String>,
  pub sector: String,
  pub salience: f32,
}

/// Memory session info
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySessionInfo {
  pub id: String,
  pub started_at: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ended_at: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub summary: Option<String>,
}

/// Memory timeline item
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTimelineItem {
  pub id: String,
  pub content: String,
  pub sector: String,
  pub salience: f32,
  pub created_at: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub session_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub session: Option<MemorySessionInfo>,
}

/// Memory timeline response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTimelineResult {
  pub anchor: MemoryTimelineItem,
  pub before: Vec<MemoryTimelineItem>,
  pub after: Vec<MemoryTimelineItem>,
}

/// Related memory search item
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRelatedItem {
  pub id: String,
  pub content: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub summary: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory_type: Option<String>,
  pub sector: String,
  pub salience: f32,
  pub score: f32,
  pub relationship: String,
  pub created_at: String,
}

/// Memory related response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRelatedResult {
  pub memory_id: String,
  pub content: String,
  pub related: Vec<MemoryRelatedItem>,
  pub count: usize,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAddResult {
  pub id: String,
  pub message: String,
  #[serde(default)]
  pub is_duplicate: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUpdateResult {
  pub id: String,
  pub new_salience: f32,
  pub message: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDeleteResult {
  pub id: String,
  pub message: String,
  #[serde(default)]
  pub hard_delete: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRestoreResult {
  pub id: String,
  pub message: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySupersedeResult {
  pub old_id: String,
  pub new_id: String,
  pub message: String,
}

// ============================================================================
// Conversions from domain types
// ============================================================================

impl From<&Memory> for MemoryItem {
  fn from(m: &Memory) -> Self {
    Self::from_memory(m, None, None)
  }
}

impl MemoryItem {
  pub fn from_memory(m: &Memory, similarity: Option<f32>, rank_score: Option<f32>) -> Self {
    Self {
      id: m.id.to_string(),
      content: m.content.clone(),
      summary: m.summary.clone(),
      sector: m.sector.as_str().to_string(),
      tier: m.tier.as_str().to_string(),
      memory_type: m.memory_type.map(|t| t.as_str().to_string()),
      salience: m.salience,
      importance: m.importance,
      similarity,
      rank_score,
      is_superseded: m.is_superseded(),
      superseded_by: m.superseded_by.map(|id| id.to_string()),
      tags: m.tags.clone(),
      categories: m.categories.clone(),
      scope_path: m.scope_path.clone(),
      scope_module: m.scope_module.clone(),
      created_at: m.created_at.to_rfc3339(),
      last_accessed: m.last_accessed.to_rfc3339(),
    }
  }

  pub fn from_search(m: &Memory, similarity: f32, rank_score: f32) -> Self {
    Self::from_memory(m, Some(similarity), Some(rank_score))
  }

  pub fn from_list(m: &Memory) -> Self {
    Self::from_memory(m, None, None)
  }
}

impl From<&Memory> for MemoryFullDetail {
  fn from(m: &Memory) -> Self {
    Self {
      id: m.id.to_string(),
      content: m.content.clone(),
      summary: m.summary.clone(),
      sector: m.sector.as_str().to_string(),
      tier: m.tier.as_str().to_string(),
      memory_type: m.memory_type.map(|t| t.as_str().to_string()),
      salience: m.salience,
      importance: m.importance,
      confidence: m.confidence,
      access_count: m.access_count,
      is_deleted: m.is_deleted,
      superseded_by: m.superseded_by.map(|id| id.to_string()),
      tags: m.tags.clone(),
      categories: m.categories.clone(),
      concepts: m.concepts.clone(),
      files: m.files.clone(),
      context: m.context.clone(),
      scope_path: m.scope_path.clone(),
      scope_module: m.scope_module.clone(),
      created_at: m.created_at.to_rfc3339(),
      updated_at: m.updated_at.to_rfc3339(),
      last_accessed: m.last_accessed.to_rfc3339(),
      valid_from: m.valid_from.to_rfc3339(),
      valid_until: m.valid_until.map(|t| t.to_rfc3339()),
      relationships: None,
    }
  }
}

impl MemoryFullDetail {
  pub fn with_relationships(mut self, relationships: Vec<MemoryRelationshipItem>) -> Self {
    self.relationships = Some(relationships);
    self
  }
}

impl From<&Memory> for MemorySummary {
  fn from(m: &Memory) -> Self {
    Self {
      id: m.id.to_string(),
      content: m.content.clone(),
      summary: m.summary.clone(),
      sector: m.sector.as_str().to_string(),
      salience: m.salience,
    }
  }
}

impl From<&Memory> for MemoryTimelineItem {
  fn from(m: &Memory) -> Self {
    Self {
      id: m.id.to_string(),
      content: m.content.clone(),
      sector: m.sector.as_str().to_string(),
      salience: m.salience,
      created_at: m.created_at.to_rfc3339(),
      session_id: m.session_id.clone(),
      session: None,
    }
  }
}

impl MemoryTimelineItem {
  pub fn with_session(mut self, session: MemorySessionInfo) -> Self {
    self.session = Some(session);
    self
  }
}

// ============================================================================
// IpcRequest implementations for typed request/response handling
// ============================================================================

use crate::{
  impl_ipc_request,
  ipc::{RequestData, ResponseData},
};

impl_ipc_request!(
  MemorySearchParams => MemorySearchResult,
  ResponseData::Memory(MemoryResponse::Search(v)) => v,
  v => RequestData::Memory(MemoryRequest::Search(v)),
  v => ResponseData::Memory(MemoryResponse::Search(v))
);
impl_ipc_request!(
  MemoryGetParams => MemoryFullDetail,
  ResponseData::Memory(MemoryResponse::Get(v)) => v,
  v => RequestData::Memory(MemoryRequest::Get(v)),
  v => ResponseData::Memory(MemoryResponse::Get(v))
);
impl_ipc_request!(
  MemoryAddParams => MemoryAddResult,
  ResponseData::Memory(MemoryResponse::Add(v)) => v,
  v => RequestData::Memory(MemoryRequest::Add(v)),
  v => ResponseData::Memory(MemoryResponse::Add(v))
);
impl_ipc_request!(
  MemoryListParams => Vec<MemoryItem>,
  ResponseData::Memory(MemoryResponse::List(v)) => v,
  v => RequestData::Memory(MemoryRequest::List(v)),
  v => ResponseData::Memory(MemoryResponse::List(v))
);
impl_ipc_request!(
  MemoryReinforceParams => MemoryUpdateResult,
  ResponseData::Memory(MemoryResponse::Update(v)) => v,
  v => RequestData::Memory(MemoryRequest::Reinforce(v)),
  v => ResponseData::Memory(MemoryResponse::Update(v))
);
impl_ipc_request!(
  MemoryDeemphasizeParams => MemoryUpdateResult,
  ResponseData::Memory(MemoryResponse::Update(v)) => v,
  v => RequestData::Memory(MemoryRequest::Deemphasize(v))
);
impl_ipc_request!(
  MemoryListDeletedParams => Vec<MemoryItem>,
  ResponseData::Memory(MemoryResponse::ListDeleted(v)) => v,
  v => RequestData::Memory(MemoryRequest::ListDeleted(v))
);
impl_ipc_request!(
  MemoryDeleteParams => MemoryDeleteResult,
  ResponseData::Memory(MemoryResponse::Delete(v)) => v,
  v => RequestData::Memory(MemoryRequest::Delete(v)),
  v => ResponseData::Memory(MemoryResponse::Delete(v))
);
impl_ipc_request!(
  MemoryHardDeleteParams => MemoryDeleteResult,
  ResponseData::Memory(MemoryResponse::Delete(v)) => v,
  v => RequestData::Memory(MemoryRequest::HardDelete(v))
);
impl_ipc_request!(
  MemorySetSalienceParams => MemoryUpdateResult,
  ResponseData::Memory(MemoryResponse::Update(v)) => v,
  v => RequestData::Memory(MemoryRequest::SetSalience(v))
);
impl_ipc_request!(
  MemoryRestoreParams => MemoryRestoreResult,
  ResponseData::Memory(MemoryResponse::Restore(v)) => v,
  v => RequestData::Memory(MemoryRequest::Restore(v)),
  v => ResponseData::Memory(MemoryResponse::Restore(v))
);
impl_ipc_request!(
  MemorySupersedeParams => MemorySupersedeResult,
  ResponseData::Memory(MemoryResponse::Supersede(v)) => v,
  v => RequestData::Memory(MemoryRequest::Supersede(v)),
  v => ResponseData::Memory(MemoryResponse::Supersede(v))
);
impl_ipc_request!(
  MemoryTimelineParams => MemoryTimelineResult,
  ResponseData::Memory(MemoryResponse::Timeline(v)) => v,
  v => RequestData::Memory(MemoryRequest::Timeline(v)),
  v => ResponseData::Memory(MemoryResponse::Timeline(v))
);
impl_ipc_request!(
  MemoryRelatedParams => MemoryRelatedResult,
  ResponseData::Memory(MemoryResponse::Related(v)) => v,
  v => RequestData::Memory(MemoryRequest::Related(v)),
  v => ResponseData::Memory(MemoryResponse::Related(v))
);
