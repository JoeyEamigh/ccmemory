use serde::{Deserialize, Serialize};

pub mod extraction;
mod prompts;
mod provider;

#[cfg(feature = "claude")]
mod claude;

// Re-export provider trait and types
// Re-export prompts and context types
pub use prompts::{ExtractionContext, ToolUse};
pub use provider::{LlmProvider, Result};

/// Semantic type for extracted memories
///
/// Used by both LLM extraction (with json-schema validation) and storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
  /// User's expressed preferences
  Preference,
  /// How code is organized/works
  Codebase,
  /// Architectural decisions with rationale
  Decision,
  /// Pitfalls to avoid
  Gotcha,
  /// Workflows/conventions to follow
  Pattern,
  /// Narrative of work completed
  TurnSummary,
  /// Record of completed task
  TaskCompletion,
}

impl MemoryType {
  pub fn as_str(&self) -> &'static str {
    match self {
      MemoryType::Preference => "preference",
      MemoryType::Codebase => "codebase",
      MemoryType::Decision => "decision",
      MemoryType::Gotcha => "gotcha",
      MemoryType::Pattern => "pattern",
      MemoryType::TurnSummary => "turn_summary",
      MemoryType::TaskCompletion => "task_completion",
    }
  }
}

impl std::fmt::Display for MemoryType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.as_str())
  }
}

impl std::str::FromStr for MemoryType {
  type Err = ();

  fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "preference" => Ok(MemoryType::Preference),
      "codebase" => Ok(MemoryType::Codebase),
      "decision" => Ok(MemoryType::Decision),
      "gotcha" => Ok(MemoryType::Gotcha),
      "pattern" => Ok(MemoryType::Pattern),
      "turn_summary" | "turnsummary" => Ok(MemoryType::TurnSummary),
      "task_completion" | "taskcompletion" => Ok(MemoryType::TaskCompletion),
      _ => Err(()),
    }
  }
}

/// Create the default LLM provider based on available features
///
/// Returns the first available provider in priority order:
/// 1. Claude CLI (if `claude` feature is enabled)
///
/// Returns an error if no provider is available.
pub fn create_provider() -> Result<Box<dyn LlmProvider>> {
  #[cfg(feature = "claude")]
  {
    let provider = claude::ClaudeProvider::new();
    if provider.is_available() {
      return Ok(Box::new(provider));
    }
    Err(LlmError::ClaudeNotFound)
  }

  #[cfg(not(feature = "claude"))]
  {
    Err(LlmError::NoProviderAvailable)
  }
}

/// Request for LLM inference
#[derive(Debug, Clone, Default)]
pub struct InferenceRequest {
  /// The prompt to send
  pub prompt: String,
  /// Optional system prompt
  pub system_prompt: Option<String>,
  /// Model to use (default: Haiku)
  pub model: String,
  /// Timeout in seconds (default: 60)
  pub timeout_secs: u64,
  /// Optional JSON schema for structured output
  pub json_schema: String,
}

impl InferenceRequest {
  pub fn new(prompt: impl Into<String>, json_schema: String) -> Self {
    Self {
      prompt: prompt.into(),
      system_prompt: None,
      model: Default::default(),
      timeout_secs: 60,
      json_schema,
    }
  }
}

/// Response from LLM inference
#[derive(Debug, Clone)]
pub struct InferenceResponse {
  /// The text response
  pub text: String,
  /// Input tokens used
  pub input_tokens: u32,
  /// Output tokens generated
  pub output_tokens: u32,
  /// Cost in USD (if available)
  pub cost_usd: Option<f64>,
  /// Duration in milliseconds
  pub duration_ms: u64,
}

/// Structured extraction result for memory extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
  pub memories: Vec<ExtractedMemory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMemory {
  pub content: String,
  #[serde(default)]
  pub summary: Option<String>,
  pub memory_type: MemoryType,
  #[serde(default)]
  pub tags: Vec<String>,
  pub confidence: f32,
}

/// Signal classification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalClassification {
  pub category: SignalCategory,
  pub is_extractable: bool,
  pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalCategory {
  /// User correcting previous behavior/output
  Correction,
  /// User expressing a preference
  Preference,
  /// User providing context/information
  Context,
  /// User requesting a task
  Task,
  /// User asking a question
  Question,
  /// User giving feedback
  Feedback,
  /// Unclassified
  Other,
}

impl SignalCategory {
  /// Whether this signal type should trigger immediate extraction
  pub fn is_high_priority(&self) -> bool {
    matches!(self, SignalCategory::Correction | SignalCategory::Preference)
  }
}

/// Superseding detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersedingResult {
  pub supersedes: bool,
  pub superseded_memory_id: Option<String>,
  pub reason: Option<String>,
  pub confidence: f32,
}

/// Errors that can occur during LLM inference
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
  #[error("Failed to spawn process: {0}")]
  SpawnFailed(#[from] std::io::Error),
  #[error("process timed out after {0} seconds")]
  Timeout(u64),
  #[error("process exited with non-zero status: {0}")]
  ProcessFailed(i32),
  #[error("Failed to parse JSON response: {0}")]
  ParseError(#[from] serde_json::Error),
  #[error("No assistant message in response")]
  NoResponse,
  #[error("No LLM provider available. Enable a provider feature (e.g., 'claude').")]
  NoProviderAvailable,
  #[cfg(feature = "claude")]
  #[error("Claude executable not found. Ensure 'claude' is in your PATH.")]
  ClaudeNotFound,
  #[cfg(feature = "claude")]
  #[error("Claude returned an error: {0}")]
  ClaudeError(String),
}
