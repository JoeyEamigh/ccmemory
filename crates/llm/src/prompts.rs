//! LLM prompts for extraction, classification, and analysis
//!
//! Uses JSON schemas for structured output validation.

use tracing::trace;

/// JSON schema for signal classification response
pub const SIGNAL_CLASSIFICATION_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "category": {
      "type": "string",
      "enum": ["correction", "preference", "context", "task", "question", "feedback", "other"]
    },
    "is_extractable": { "type": "boolean" },
    "summary": { "type": ["string", "null"] }
  },
  "required": ["category", "is_extractable"]
}"#;

/// JSON schema for memory extraction response
pub const EXTRACTION_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "memories": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "content": { "type": "string" },
          "summary": { "type": ["string", "null"] },
          "memory_type": {
            "type": "string",
            "enum": ["preference", "codebase", "decision", "gotcha", "pattern", "turn_summary", "task_completion"]
          },
          "tags": { "type": "array", "items": { "type": "string" } },
          "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
        },
        "required": ["content", "memory_type", "confidence"]
      }
    }
  },
  "required": ["memories"]
}"#;

/// JSON schema for superseding detection response
pub const SUPERSEDING_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "supersedes": { "type": "boolean" },
    "superseded_memory_id": { "type": ["string", "null"] },
    "reason": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "required": ["supersedes", "confidence"]
}"#;

/// Prompt for classifying user input signals
pub const SIGNAL_CLASSIFICATION_PROMPT: &str = r#"Classify this user message:
- correction: User correcting previous behavior
- preference: User expressing a preference
- context: User providing information
- task: User requesting work
- question: User asking something
- feedback: User giving feedback
- other: None of the above

Set is_extractable=true if the message contains memorable information.

Message:
"#;

/// Prompt for extracting memories from conversation context
pub const MEMORY_EXTRACTION_PROMPT: &str = r#"Extract valuable long-term memories from this conversation segment.

Memory types:
- preference: User's stated preference
- codebase: Knowledge about code structure/behavior
- decision: Design or implementation decision with rationale
- gotcha: Pitfall or warning to remember
- pattern: Recurring pattern or best practice
- turn_summary: Summary of what was accomplished
- task_completion: Record of completed task

Only extract memories with confidence >= 0.6. Return empty array if nothing worth extracting.

Conversation:
"#;

/// Prompt for detecting if new memory supersedes existing ones
pub const SUPERSEDING_DETECTION_PROMPT: &str = r#"Does the new memory supersede any existing memory?

Supersedes when: updates/replaces same info, contradicts with newer truth, refines same concept.
Does NOT supersede when: different topic, adds without contradicting, related but distinct.

New memory:
{new_memory}

Existing memories:
{existing_memories}
"#;

/// System prompt for extraction context
pub const EXTRACTION_SYSTEM_PROMPT: &str = r#"You are CCEngram's memory extraction system. Extract valuable information from Claude Code conversations that would be useful in future sessions.

Guidelines:
1. Focus on lasting value, not ephemeral details
2. Prefer explicit statements over inferences
3. Include enough context to be standalone
4. Use clear, concise language
5. Assign appropriate confidence scores"#;

#[allow(dead_code)]
/// Prompt for analyzing code changes and extracting patterns (may use at some point - lot of inference though)
pub const CODE_ANALYSIS_PROMPT: &str = r#"You are analyzing code changes to extract patterns and learnings.

Look for:
- Architectural patterns being established
- Coding conventions being followed
- Bug patterns that were fixed
- Performance considerations
- Testing patterns

Respond with JSON only:
{
  "patterns": [
    {
      "content": "<pattern description>",
      "pattern_type": "<architectural|convention|bug_fix|performance|testing>",
      "files_involved": ["<file paths>"],
      "confidence": <0.0-1.0>
    }
  ]
}

Code changes:
"#;

/// Build a signal classification prompt for a specific user message
pub fn build_signal_classification_prompt(user_message: &str) -> String {
  let prompt = format!("{}{}", SIGNAL_CLASSIFICATION_PROMPT, user_message);
  trace!(
    template_len = SIGNAL_CLASSIFICATION_PROMPT.len(),
    message_len = user_message.len(),
    total_len = prompt.len(),
    "Built signal classification prompt"
  );
  prompt
}

/// Build a memory extraction prompt for a conversation segment
pub fn build_extraction_prompt(context: &ExtractionContext) -> String {
  let mut prompt = String::new();
  prompt.push_str(MEMORY_EXTRACTION_PROMPT);

  if let Some(user_prompt) = &context.user_prompt {
    prompt.push_str("\nUser prompt: ");
    prompt.push_str(user_prompt);
  }

  // Include detailed tool uses if available (preferred over legacy fields)
  if !context.tool_uses.is_empty() {
    prompt.push_str("\n\nTool sequence:");
    for tool_use in &context.tool_uses {
      prompt.push_str(&format!("\n  - {}", tool_use.format_for_prompt()));
    }
  } else {
    // Fallback to legacy fields if no detailed tool uses
    if !context.files_read.is_empty() {
      prompt.push_str("\nFiles read: ");
      prompt.push_str(&context.files_read.join(", "));
    }

    if !context.files_modified.is_empty() {
      prompt.push_str("\nFiles modified: ");
      prompt.push_str(&context.files_modified.join(", "));
    }

    if !context.commands_run.is_empty() {
      prompt.push_str("\nCommands run: ");
      for (cmd, exit_code) in &context.commands_run {
        prompt.push_str(&format!("\n  - {} (exit: {})", cmd, exit_code));
      }
    }
  }

  if !context.errors_encountered.is_empty() {
    prompt.push_str("\nErrors encountered: ");
    for err in &context.errors_encountered {
      prompt.push_str(&format!("\n  - {}", err));
    }
  }

  if !context.completed_tasks.is_empty() {
    prompt.push_str("\nCompleted tasks: ");
    for task in &context.completed_tasks {
      prompt.push_str(&format!("\n  - {}", task));
    }
  }

  if let Some(assistant_msg) = &context.last_assistant_message {
    prompt.push_str("\nLast assistant response (truncated): ");
    let truncated: String = assistant_msg.chars().take(1000).collect();
    prompt.push_str(&truncated);
  }

  trace!(
    template_len = MEMORY_EXTRACTION_PROMPT.len(),
    total_len = prompt.len(),
    has_user_prompt = context.user_prompt.is_some(),
    tool_uses_count = context.tool_uses.len(),
    files_read_count = context.files_read.len(),
    files_modified_count = context.files_modified.len(),
    commands_run_count = context.commands_run.len(),
    errors_count = context.errors_encountered.len(),
    tasks_count = context.completed_tasks.len(),
    has_assistant_message = context.last_assistant_message.is_some(),
    "Built memory extraction prompt"
  );

  prompt
}

/// Build a superseding detection prompt
pub fn build_superseding_prompt(new_memory: &str, existing_memories: &[(String, String)]) -> String {
  let mut existing_json = String::from("[\n");
  for (i, (id, content)) in existing_memories.iter().enumerate() {
    if i > 0 {
      existing_json.push_str(",\n");
    }
    existing_json.push_str(&format!(
      r#"  {{"id": "{}", "content": "{}"}}"#,
      id,
      content.replace('"', "\\\"").replace('\n', "\\n")
    ));
  }
  existing_json.push_str("\n]");

  let prompt = SUPERSEDING_DETECTION_PROMPT
    .replace("{new_memory}", new_memory)
    .replace("{existing_memories}", &existing_json);

  trace!(
    template_len = SUPERSEDING_DETECTION_PROMPT.len(),
    new_memory_len = new_memory.len(),
    existing_memories_count = existing_memories.len(),
    existing_json_len = existing_json.len(),
    total_len = prompt.len(),
    "Built superseding detection prompt"
  );

  prompt
}

/// Typed tool use data for extraction context
#[derive(Debug, Clone)]
pub enum ToolUse {
  /// File read operation
  Read { file_path: String },
  /// File edit operation
  Edit {
    file_path: String,
    /// Brief description of what was changed (first ~100 chars of old_string)
    change_preview: Option<String>,
  },
  /// File write operation
  Write { file_path: String },
  /// Notebook edit operation
  NotebookEdit { notebook_path: String },
  /// Bash command execution
  Bash { command: String, exit_code: i32 },
  /// Glob file search
  Glob { pattern: String },
  /// Grep content search
  Grep { pattern: String },
  /// Task/subagent spawn
  Task { description: Option<String> },
  /// Todo list management
  TodoWrite {
    completed_tasks: Vec<String>,
    pending_tasks: Vec<String>,
  },
  /// Web fetch
  WebFetch { url: String },
  /// Web search
  WebSearch { query: String },
  /// Other tool (catch-all)
  Other { tool_name: String },
}

impl ToolUse {
  /// Parse a tool use from raw hook event data.
  ///
  /// # Arguments
  /// * `tool_name` - The name of the tool
  /// * `params` - The tool input parameters (from hook event)
  /// * `result` - Optional tool result (from hook event, used for exit codes etc.)
  pub fn from_hook_event(tool_name: &str, params: &serde_json::Value, result: Option<&serde_json::Value>) -> Self {
    match tool_name {
      "Read" => {
        let file_path = params
          .get("file_path")
          .and_then(|v| v.as_str())
          .unwrap_or("")
          .to_string();
        ToolUse::Read { file_path }
      }
      "Edit" => {
        let file_path = params
          .get("file_path")
          .and_then(|v| v.as_str())
          .unwrap_or("")
          .to_string();
        let change_preview = params
          .get("old_string")
          .and_then(|v| v.as_str())
          .map(|s| s.chars().take(100).collect());
        ToolUse::Edit {
          file_path,
          change_preview,
        }
      }
      "Write" => {
        let file_path = params
          .get("file_path")
          .and_then(|v| v.as_str())
          .unwrap_or("")
          .to_string();
        ToolUse::Write { file_path }
      }
      "NotebookEdit" => {
        let notebook_path = params
          .get("notebook_path")
          .and_then(|v| v.as_str())
          .unwrap_or("")
          .to_string();
        ToolUse::NotebookEdit { notebook_path }
      }
      "Bash" => {
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        // exit_code comes from result, not params
        let exit_code = result
          .and_then(|r| r.get("exit_code"))
          .and_then(|v| v.as_i64())
          .unwrap_or(0) as i32;
        ToolUse::Bash { command, exit_code }
      }
      "Glob" => {
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_string();
        ToolUse::Glob { pattern }
      }
      "Grep" => {
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_string();
        ToolUse::Grep { pattern }
      }
      "Task" => {
        let description = params.get("description").and_then(|v| v.as_str()).map(String::from);
        ToolUse::Task { description }
      }
      "TodoWrite" => {
        let mut completed_tasks = Vec::new();
        let mut pending_tasks = Vec::new();
        if let Some(todos) = params.get("todos").and_then(|v| v.as_array()) {
          for todo in todos {
            if let Some(content) = todo.get("content").and_then(|v| v.as_str()) {
              if todo.get("status").and_then(|v| v.as_str()) == Some("completed") {
                completed_tasks.push(content.to_string());
              } else {
                pending_tasks.push(content.to_string());
              }
            }
          }
        }
        ToolUse::TodoWrite {
          completed_tasks,
          pending_tasks,
        }
      }
      "WebFetch" => {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        ToolUse::WebFetch { url }
      }
      "WebSearch" => {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
        ToolUse::WebSearch { query }
      }
      _ => ToolUse::Other {
        tool_name: tool_name.to_string(),
      },
    }
  }

  /// Get the tool name
  pub fn name(&self) -> &str {
    match self {
      ToolUse::Read { .. } => "Read",
      ToolUse::Edit { .. } => "Edit",
      ToolUse::Write { .. } => "Write",
      ToolUse::NotebookEdit { .. } => "NotebookEdit",
      ToolUse::Bash { .. } => "Bash",
      ToolUse::Glob { .. } => "Glob",
      ToolUse::Grep { .. } => "Grep",
      ToolUse::Task { .. } => "Task",
      ToolUse::TodoWrite { .. } => "TodoWrite",
      ToolUse::WebFetch { .. } => "WebFetch",
      ToolUse::WebSearch { .. } => "WebSearch",
      ToolUse::Other { tool_name } => tool_name,
    }
  }

  /// Get the file path if this tool operates on a file
  pub fn file_path(&self) -> Option<&str> {
    match self {
      ToolUse::Read { file_path } => Some(file_path),
      ToolUse::Edit { file_path, .. } => Some(file_path),
      ToolUse::Write { file_path } => Some(file_path),
      ToolUse::NotebookEdit { notebook_path } => Some(notebook_path),
      _ => None,
    }
  }

  /// Check if this is a file modification (Edit, Write, NotebookEdit)
  pub fn is_file_modification(&self) -> bool {
    matches!(
      self,
      ToolUse::Edit { .. } | ToolUse::Write { .. } | ToolUse::NotebookEdit { .. }
    )
  }

  /// Check if this is a file read
  pub fn is_file_read(&self) -> bool {
    matches!(self, ToolUse::Read { .. })
  }

  /// Get command info if this is a Bash tool use
  pub fn command_info(&self) -> Option<(&str, i32)> {
    match self {
      ToolUse::Bash { command, exit_code } => Some((command, *exit_code)),
      _ => None,
    }
  }

  /// Get search pattern if this is a search tool
  pub fn search_pattern(&self) -> Option<&str> {
    match self {
      ToolUse::Glob { pattern } | ToolUse::Grep { pattern } => Some(pattern),
      _ => None,
    }
  }

  /// Get completed tasks if this is a TodoWrite
  pub fn completed_tasks(&self) -> Option<&[String]> {
    match self {
      ToolUse::TodoWrite { completed_tasks, .. } => Some(completed_tasks),
      _ => None,
    }
  }

  /// Format for LLM prompt inclusion
  pub fn format_for_prompt(&self) -> String {
    match self {
      ToolUse::Read { file_path } => format!("Read: {}", file_path),
      ToolUse::Edit {
        file_path,
        change_preview,
      } => {
        if let Some(preview) = change_preview {
          format!("Edit: {} (changed: {}...)", file_path, preview)
        } else {
          format!("Edit: {}", file_path)
        }
      }
      ToolUse::Write { file_path } => format!("Write: {}", file_path),
      ToolUse::NotebookEdit { notebook_path } => format!("NotebookEdit: {}", notebook_path),
      ToolUse::Bash { command, exit_code } => {
        if *exit_code == 0 {
          format!("Bash: {}", command)
        } else {
          format!("Bash: {} (exit: {})", command, exit_code)
        }
      }
      ToolUse::Glob { pattern } => format!("Glob: {}", pattern),
      ToolUse::Grep { pattern } => format!("Grep: {}", pattern),
      ToolUse::Task { description } => {
        if let Some(desc) = description {
          format!("Task: {}", desc)
        } else {
          "Task: (subagent)".to_string()
        }
      }
      ToolUse::TodoWrite {
        completed_tasks,
        pending_tasks,
      } => {
        format!(
          "TodoWrite: {} completed, {} pending",
          completed_tasks.len(),
          pending_tasks.len()
        )
      }
      ToolUse::WebFetch { url } => format!("WebFetch: {}", url),
      ToolUse::WebSearch { query } => format!("WebSearch: {}", query),
      ToolUse::Other { tool_name } => tool_name.clone(),
    }
  }
}

/// Context for memory extraction
#[derive(Debug, Default, Clone)]
pub struct ExtractionContext {
  /// The user's prompt that started this segment
  pub user_prompt: Option<String>,
  /// Files that were read during this segment
  pub files_read: Vec<String>,
  /// Files that were modified during this segment
  pub files_modified: Vec<String>,
  /// Commands that were run with exit codes
  pub commands_run: Vec<(String, i32)>,
  /// Errors that were encountered
  pub errors_encountered: Vec<String>,
  /// Searches that were performed
  pub searches_performed: Vec<String>,
  /// Tasks that were completed
  pub completed_tasks: Vec<String>,
  /// The last assistant message
  pub last_assistant_message: Option<String>,
  /// Total tool calls in this segment
  pub tool_call_count: usize,
  /// Detailed tool use records
  pub tool_uses: Vec<ToolUse>,
}

impl ExtractionContext {
  pub fn new() -> Self {
    Self::default()
  }

  /// Check if this segment has enough content to warrant extraction
  pub fn has_meaningful_content(&self) -> bool {
    // At least 3 tool calls OR explicit signals
    self.tool_call_count >= 3
      || !self.files_modified.is_empty()
      || !self.completed_tasks.is_empty()
      || !self.errors_encountered.is_empty()
  }

  /// Check if this segment has high-priority signals requiring immediate extraction
  pub fn has_high_priority_signals(&self) -> bool {
    // Corrections or preferences trigger immediate extraction
    // This will be determined by signal classification
    false // Placeholder - actual check happens via LLM classification
  }
}
