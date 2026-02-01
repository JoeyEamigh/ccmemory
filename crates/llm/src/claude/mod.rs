//! LLM inference for CCEngram via Claude CLI
//!
//! This module provides LLM inference capabilities by invoking the `claude` CLI
//! in print mode with JSON output. It disables all hooks and plugins to avoid
//! recursive calls when invoked from within CCEngram hooks.

use std::{
  process::Stdio,
  time::{Duration, Instant},
};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::{io::AsyncReadExt, process::Command, time::timeout};
use tracing::{debug, error, trace, warn};

use crate::{InferenceRequest, InferenceResponse, LlmError, LlmProvider, Result};

/// Claude CLI provider for LLM inference
///
/// This provider invokes the `claude` CLI tool in print mode with JSON output.
/// It disables all hooks and plugins to avoid recursive calls when invoked
/// from within CCEngram hooks.
#[derive(Debug, Clone)]
pub struct ClaudeProvider {
  /// Cached path to the claude executable
  claude_path: String,
}

impl ClaudeProvider {
  /// Create a new Claude provider
  ///
  /// Attempts to find the claude executable in PATH.
  /// Use `is_available()` to check if the provider can be used.
  pub fn new() -> Self {
    Self {
      claude_path: find_claude().unwrap_or_default(),
    }
  }
}

impl Default for ClaudeProvider {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
  fn name(&self) -> &str {
    "claude-cli"
  }

  fn is_available(&self) -> bool {
    !self.claude_path.is_empty()
  }

  async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
    if self.claude_path.is_empty() {
      return Err(LlmError::ClaudeNotFound);
    }
    infer_internal(&self.claude_path, request).await
  }
}

// Internal types for parsing Claude CLI JSON output

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ClaudeMessage {
  User {},
  System {},
  Assistant(AssistantMessage),
  Result(ResultMessage),
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
  message: MessageContent,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
  content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentBlock {
  Text {
    text: String,
  },
  #[serde(other)]
  Other,
}

#[derive(Debug, Deserialize)]
struct ResultMessage {
  #[serde(default)]
  is_error: bool,
  #[serde(default)]
  duration_ms: u64,
  #[serde(default)]
  total_cost_usd: f64,
  usage: Option<Usage>,
  result: Option<String>,
  /// Structured output when --json-schema is used
  structured_output: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Usage {
  input_tokens: u32,
  output_tokens: u32,
}

/// Find the claude executable in PATH
fn find_claude() -> Result<String> {
  let which_cmd = if cfg!(windows) { "where" } else { "which" };

  let output = std::process::Command::new(which_cmd)
    .arg("claude")
    .output()
    .map_err(|e| {
      debug!(err = %e, "Failed to execute 'which claude'");
      LlmError::ClaudeNotFound
    })?;

  if !output.status.success() {
    debug!("Claude executable not found in PATH");
    return Err(LlmError::ClaudeNotFound);
  }

  let path = String::from_utf8_lossy(&output.stdout)
    .lines()
    .next()
    .map(|s| s.trim().to_string())
    .ok_or(LlmError::ClaudeNotFound)?;

  if path.is_empty() {
    debug!("Claude path is empty");
    return Err(LlmError::ClaudeNotFound);
  }

  trace!(claude_path = %path, "Found claude executable");
  Ok(path)
}

/// Internal inference implementation
///
/// This function:
/// 1. Spawns the `claude` CLI with `--print` mode
/// 2. Disables all hooks/plugins to avoid recursion
/// 3. Parses the JSON output
/// 4. Returns the text response and usage stats
async fn infer_internal(claude_path: &str, request: InferenceRequest) -> Result<InferenceResponse> {
  let start = Instant::now();

  let full_prompt = if let Some(system) = &request.system_prompt {
    format!("{}\n\n{}", system, request.prompt)
  } else {
    request.prompt.clone()
  };

  debug!(
    model = %request.model.as_str(),
    prompt_len = full_prompt.len(),
    timeout_secs = request.timeout_secs,
    has_system_prompt = request.system_prompt.is_some(),
    "Starting inference request"
  );

  let mut cmd = Command::new(claude_path);
  cmd
    .arg("-p") // Print mode
    .arg("--model")
    .arg(&request.model)
    .arg("--output-format")
    .arg("json")
    .arg("--no-session-persistence")
    // Disable all hooks and plugins to avoid recursion
    .arg("--settings")
    .arg(r#"{"hooks":{}}"#)
    .arg("--setting-sources")
    .arg("")
    // Disable all tools - we only want text generation
    .arg("--tools")
    .arg("")
    .arg("--json-schema")
    .arg(&request.json_schema);

  cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

  trace!(
    claude_path = %claude_path,
    model = %request.model.as_str(),
    "Spawning Claude CLI process"
  );

  let mut child = match cmd.spawn() {
    Ok(child) => child,
    Err(e) => {
      error!(err = %e, "Failed to spawn Claude CLI process");
      return Err(e.into());
    }
  };

  // Write prompt to stdin
  if let Some(mut stdin) = child.stdin.take() {
    use tokio::io::AsyncWriteExt;
    stdin.write_all(full_prompt.as_bytes()).await?;
    drop(stdin); // Close stdin to signal end of input
  }

  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| std::io::Error::other("stdout not piped"))?;
  let mut reader = tokio::io::BufReader::new(stdout);

  // Read with timeout
  let read_future = async {
    let mut output = String::new();
    reader.read_to_string(&mut output).await?;
    Ok::<_, std::io::Error>(output)
  };

  let output = match timeout(Duration::from_secs(request.timeout_secs), read_future).await {
    Ok(Ok(output)) => output,
    Ok(Err(e)) => {
      error!(err = %e, "Failed to read Claude CLI output");
      return Err(e.into());
    }
    Err(_) => {
      warn!(
        timeout_secs = request.timeout_secs,
        elapsed_ms = start.elapsed().as_millis() as u64,
        model = %request.model.as_str(),
        "Claude CLI timed out"
      );
      return Err(LlmError::Timeout(request.timeout_secs));
    }
  };

  // Wait for process to complete
  let status = child.wait().await?;
  if !status.success() {
    let exit_code = status.code().unwrap_or(-1);
    error!(
      exit_code = exit_code,
      model = %request.model.as_str(),
      "Claude CLI process failed"
    );
    return Err(LlmError::ProcessFailed(exit_code));
  }

  trace!(
    output_len = output.len(),
    elapsed_ms = start.elapsed().as_millis() as u64,
    "Claude CLI process completed"
  );

  // Parse JSON array output
  // The output is a JSON array: [{system}, {assistant}, {result}]
  let messages: Vec<ClaudeMessage> = match serde_json::from_str::<Vec<ClaudeMessage>>(&output) {
    Ok(msgs) => {
      trace!(message_count = msgs.len(), "Parsed Claude CLI JSON response");
      msgs
    }
    Err(e) => {
      warn!(
        err = %e,
        output_len = output.len(),
        output_preview = %output.chars().take(200).collect::<String>(),
        "Failed to parse Claude CLI JSON response"
      );
      return Err(e.into());
    }
  };

  let mut response_text = String::new();
  let mut structured_output: Option<serde_json::Value> = None;
  let mut input_tokens = 0u32;
  let mut output_tokens = 0u32;
  let mut cost_usd = None;
  let mut duration_ms = 0u64;

  for msg in messages {
    match msg {
      ClaudeMessage::User {} => {
        // User input echo, nothing to extract
      }
      ClaudeMessage::System {} => {
        // Session init, nothing to extract
      }
      ClaudeMessage::Assistant(assistant) => {
        for block in assistant.message.content {
          if let ContentBlock::Text { text } = block {
            response_text.push_str(&text);
          }
        }
      }
      ClaudeMessage::Result(result) => {
        if result.is_error {
          let error_msg = result.result.unwrap_or_else(|| "Unknown error".to_string());
          error!(
            error_msg = %error_msg,
            model = %request.model.as_str(),
            "Claude returned an error"
          );
          return Err(LlmError::ClaudeError(error_msg));
        }

        duration_ms = result.duration_ms;
        cost_usd = Some(result.total_cost_usd);

        // Capture structured_output when --json-schema was used
        structured_output = result.structured_output;

        if let Some(usage) = result.usage {
          input_tokens = usage.input_tokens;
          output_tokens = usage.output_tokens;
        }
      }
    }
  }

  // When using --json-schema, the structured output is in structured_output field
  // Otherwise, use the assistant text response
  let final_response = if let Some(structured) = structured_output {
    trace!(
      structured_output_type = %structured.as_object().map(|_| "object").unwrap_or("other"),
      "Using structured_output from result"
    );
    // Serialize back to string for the caller to parse
    serde_json::to_string(&structured).map_err(|e| {
      error!(err = %e, "Failed to serialize structured_output");
      LlmError::ParseError(e)
    })?
  } else {
    response_text
  };

  if final_response.is_empty() {
    warn!(
      model = %request.model.as_str(),
      elapsed_ms = start.elapsed().as_millis() as u64,
      "Claude returned no response text"
    );
    return Err(LlmError::NoResponse);
  }

  debug!(
    response_len = final_response.len(),
    input_tokens,
    output_tokens,
    duration_ms,
    cost_usd = ?cost_usd,
    elapsed_ms = start.elapsed().as_millis() as u64,
    model = %request.model.as_str(),
    "Inference completed successfully"
  );

  Ok(InferenceResponse {
    text: final_response,
    input_tokens,
    output_tokens,
    cost_usd,
    duration_ms,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  // Integration test for ClaudeProvider - requires `claude` CLI to be available
  #[tokio::test]
  #[ignore = "requires claude CLI"]
  async fn test_claude_provider_infer() {
    let provider = ClaudeProvider::new();
    assert!(provider.is_available());
    assert_eq!(provider.name(), "claude-cli");

    let request = InferenceRequest {
      prompt: "Say 'hello' and nothing else".to_string(),
      model: "haiku".to_string(),
      timeout_secs: 30,
      json_schema: "".to_string(),
      ..Default::default()
    };

    let response = provider.infer(request).await.unwrap();
    assert!(response.text.to_lowercase().contains("hello"));
    assert!(response.output_tokens > 0);
  }
}
