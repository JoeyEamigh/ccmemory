//! Hook event types and parsing.

use serde::{Deserialize, Serialize};

use crate::service::util::ServiceError;

/// Hook event types from Claude Code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
  SessionStart,
  SessionEnd,
  UserPromptSubmit,
  PostToolUse,
  PreCompact,
  Stop,
  SubagentStop,
  Notification,
}

impl std::fmt::Display for HookEvent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::SessionStart => write!(f, "SessionStart"),
      Self::SessionEnd => write!(f, "SessionEnd"),
      Self::UserPromptSubmit => write!(f, "UserPromptSubmit"),
      Self::PostToolUse => write!(f, "PostToolUse"),
      Self::PreCompact => write!(f, "PreCompact"),
      Self::Stop => write!(f, "Stop"),
      Self::SubagentStop => write!(f, "SubagentStop"),
      Self::Notification => write!(f, "Notification"),
    }
  }
}

impl std::str::FromStr for HookEvent {
  type Err = ServiceError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "SessionStart" => Ok(Self::SessionStart),
      "SessionEnd" => Ok(Self::SessionEnd),
      "UserPromptSubmit" => Ok(Self::UserPromptSubmit),
      "PostToolUse" => Ok(Self::PostToolUse),
      "PreCompact" => Ok(Self::PreCompact),
      "Stop" => Ok(Self::Stop),
      "SubagentStop" => Ok(Self::SubagentStop),
      "Notification" => Ok(Self::Notification),
      _ => Err(ServiceError::validation(format!("Unknown hook event: {}", s))),
    }
  }
}
