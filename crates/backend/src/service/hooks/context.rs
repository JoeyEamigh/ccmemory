//! Session context accumulation for hook processing.
//!
//! This module manages the accumulated context from a session segment,
//! tracking tool uses, files, commands, and other information needed
//! for memory extraction.

use llm::{ExtractionContext, ToolUse};
use tracing::debug;

/// Accumulated context from a session segment.
///
/// Tracks all relevant information from a conversation segment
/// needed for memory extraction at segment boundaries (Stop, PreCompact, etc.)
#[derive(Debug, Default, Clone)]
pub struct SegmentContext {
  /// All tool uses in this segment (typed)
  pub tool_uses: Vec<ToolUse>,
  /// The user's original prompt that started this segment
  pub user_prompt: Option<String>,
  /// Additional prompts sent while agent was working
  pub additional_prompts: Vec<String>,
  /// Files that were read (paths) - derived from tool_uses for quick access
  pub files_read: Vec<String>,
  /// Files that were modified (paths) - derived from tool_uses for quick access
  pub files_modified: Vec<String>,
  /// Commands run with exit codes - derived from tool_uses for quick access
  pub commands_run: Vec<(String, i32)>,
  /// Errors encountered
  pub errors_encountered: Vec<String>,
  /// Search patterns executed - derived from tool_uses for quick access
  pub searches_performed: Vec<String>,
  /// Tasks completed (from TodoWrite)
  pub completed_tasks: Vec<String>,
  /// Last assistant message (if captured)
  pub last_assistant_message: Option<String>,
  /// Number of active subagents (skip extraction when > 0)
  pub subagent_depth: usize,
}

impl SegmentContext {
  /// Total tool call count in this segment
  pub fn tool_call_count(&self) -> usize {
    self.tool_uses.len()
  }

  /// Check if this segment has meaningful work to extract.
  ///
  /// Returns true if there are:
  /// - No active subagents, AND
  /// - At least 3 tool calls, OR
  /// - File modifications, OR
  /// - Completed tasks, OR
  /// - Errors encountered
  pub fn has_meaningful_work(&self) -> bool {
    // Skip if subagent is active
    if self.subagent_depth > 0 {
      debug!(depth = self.subagent_depth, "Skipping extraction - subagent active");
      return false;
    }

    let has_work = self.tool_call_count() >= 3
      || !self.files_modified.is_empty()
      || !self.completed_tasks.is_empty()
      || !self.errors_encountered.is_empty();

    if !has_work {
      debug!(
        tool_calls = self.tool_call_count(),
        files_modified = self.files_modified.len(),
        completed_tasks = self.completed_tasks.len(),
        errors = self.errors_encountered.len(),
        "Segment has no meaningful work to extract"
      );
    }

    has_work
  }

  /// Convert to LLM extraction context
  ///
  /// Filters out Read tool uses as they don't provide useful context for memory extraction.
  pub fn to_extraction_context(&self) -> ExtractionContext {
    // Filter out Read tools - their results aren't useful for memory summaries
    let filtered_tool_uses: Vec<_> = self.tool_uses.iter().filter(|t| !t.is_file_read()).cloned().collect();

    ExtractionContext {
      user_prompt: self.user_prompt.clone(),
      files_read: Vec::new(), // Don't include files_read in extraction context
      files_modified: self.files_modified.clone(),
      commands_run: self.commands_run.clone(),
      errors_encountered: self.errors_encountered.clone(),
      searches_performed: self.searches_performed.clone(),
      completed_tasks: self.completed_tasks.clone(),
      last_assistant_message: self.last_assistant_message.clone(),
      tool_call_count: filtered_tool_uses.len(),
      tool_uses: filtered_tool_uses,
    }
  }

  /// Reset the context for a new segment
  pub fn reset(&mut self) {
    self.tool_uses.clear();
    self.user_prompt = None;
    self.additional_prompts.clear();
    self.files_read.clear();
    self.files_modified.clear();
    self.commands_run.clear();
    self.errors_encountered.clear();
    self.searches_performed.clear();
    self.completed_tasks.clear();
    self.last_assistant_message = None;
    self.subagent_depth = 0;
  }

  // ========================================================================
  // Tool Tracking Methods
  // ========================================================================

  /// Record a user prompt. First prompt becomes user_prompt, subsequent ones
  /// are added to additional_prompts (user can send more prompts while agent works).
  pub fn record_user_prompt(&mut self, prompt: String) {
    if self.user_prompt.is_none() {
      self.user_prompt = Some(prompt);
    } else {
      self.additional_prompts.push(prompt);
    }
  }

  /// Record a tool use with typed data
  pub fn record_tool_use(&mut self, tool_use: ToolUse) {
    self.tool_uses.push(tool_use);
  }

  /// Record a file read
  pub fn record_file_read(&mut self, path: &str) {
    if !self.files_read.contains(&path.to_string()) {
      self.files_read.push(path.to_string());
    }
  }

  /// Record a file modification
  pub fn record_file_modified(&mut self, path: &str) {
    if !self.files_modified.contains(&path.to_string()) {
      self.files_modified.push(path.to_string());
    }
  }

  /// Record a command execution
  pub fn record_command(&mut self, command: String, exit_code: i32) {
    // Truncate long commands
    let cmd_display = if command.len() > 100 {
      format!("{}...", &command[..100])
    } else {
      command
    };

    // Record error if non-zero exit
    if exit_code != 0 {
      self.errors_encountered.push(format!(
        "Command '{}' failed with exit code {}",
        &cmd_display, exit_code
      ));
    }

    self.commands_run.push((cmd_display, exit_code));
  }

  /// Record a search pattern
  pub fn record_search(&mut self, pattern: &str) {
    if !self.searches_performed.contains(&pattern.to_string()) {
      self.searches_performed.push(pattern.to_string());
    }
  }

  /// Record a completed task
  pub fn record_completed_task(&mut self, task: &str) {
    if !self.completed_tasks.contains(&task.to_string()) {
      self.completed_tasks.push(task.to_string());
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_segment_context_meaningful_work() {
    // Empty context has no meaningful work
    let ctx = SegmentContext::default();
    assert!(!ctx.has_meaningful_work());

    // With file modifications
    let mut ctx = SegmentContext::default();
    ctx.record_file_modified("src/lib.rs");
    assert!(ctx.has_meaningful_work());

    // With 3+ tool calls
    let mut ctx = SegmentContext::default();
    ctx.record_tool_use(ToolUse::Read {
      file_path: "src/lib.rs".to_string(),
    });
    ctx.record_tool_use(ToolUse::Edit {
      file_path: "src/lib.rs".to_string(),
      change_preview: None,
    });
    ctx.record_tool_use(ToolUse::Write {
      file_path: "src/new.rs".to_string(),
    });
    assert!(ctx.has_meaningful_work());
  }

  #[test]
  fn test_segment_context_reset() {
    let mut ctx = SegmentContext {
      user_prompt: Some("Test".to_string()),
      ..Default::default()
    };
    ctx.record_file_modified("test.rs");
    ctx.record_tool_use(ToolUse::Read {
      file_path: "test.rs".to_string(),
    });

    ctx.reset();

    assert!(ctx.user_prompt.is_none());
    assert!(ctx.files_modified.is_empty());
    assert!(ctx.tool_uses.is_empty());
  }

  #[test]
  fn test_to_extraction_context() {
    let mut ctx = SegmentContext {
      user_prompt: Some("Test prompt".to_string()),
      ..Default::default()
    };
    ctx.record_file_modified("src/lib.rs");
    ctx.record_tool_use(ToolUse::Edit {
      file_path: "src/lib.rs".to_string(),
      change_preview: None,
    });

    let ext_ctx = ctx.to_extraction_context();
    assert_eq!(ext_ctx.user_prompt, Some("Test prompt".to_string()));
    assert_eq!(ext_ctx.files_modified, vec!["src/lib.rs".to_string()]);
    assert_eq!(ext_ctx.tool_call_count, 1);
    assert_eq!(ext_ctx.tool_uses.len(), 1);
  }
}
