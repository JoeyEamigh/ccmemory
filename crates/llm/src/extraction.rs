//! High-level extraction functions using LLM inference
//!
//! This module provides provider-agnostic functions for:
//! - Signal classification (detecting extractable user inputs)
//! - Memory extraction (extracting memories from conversation context)
//! - Superseding detection (finding memories that should be marked superseded)

use serde::de::DeserializeOwned;
use tracing::{debug, info, trace, warn};

use crate::{
  ExtractionContext, ExtractionResult, InferenceRequest, LlmProvider, Result, SignalCategory, SignalClassification,
  SupersedingResult,
  prompts::{
    EXTRACTION_SCHEMA, EXTRACTION_SYSTEM_PROMPT, SIGNAL_CLASSIFICATION_SCHEMA, SUPERSEDING_SCHEMA,
    build_extraction_prompt, build_signal_classification_prompt, build_superseding_prompt,
  },
};

/// Parse JSON from an LLM response text
///
/// Handles responses that may be wrapped in markdown code blocks:
/// - ```json ... ```
/// - ``` ... ```
/// - Raw JSON
fn parse_json<T: DeserializeOwned>(text: &str) -> std::result::Result<T, serde_json::Error> {
  // Try to extract JSON from markdown code blocks
  let (json_str, extracted_from_block) = if let Some(captures) = extract_code_block(text) {
    (captures, true)
  } else {
    (text.trim(), false)
  };

  trace!(
    input_len = text.len(),
    json_len = json_str.len(),
    extracted_from_code_block = extracted_from_block,
    "Parsing JSON from LLM response"
  );

  match serde_json::from_str(json_str) {
    Ok(result) => {
      debug!(
        json_len = json_str.len(),
        extracted_from_code_block = extracted_from_block,
        "Successfully parsed JSON from LLM response"
      );
      Ok(result)
    }
    Err(e) => {
      warn!(
          err = %e,
          json_len = json_str.len(),
          json_preview = %json_str.chars().take(200).collect::<String>(),
          "Failed to parse JSON from LLM response"
      );
      Err(e)
    }
  }
}

fn extract_code_block(text: &str) -> Option<&str> {
  let text = text.trim();

  if !text.starts_with("```") {
    return None;
  }

  // Find the end of the first line (after ```)
  let first_newline = text.find('\n')?;
  let after_fence = &text[first_newline + 1..];

  // Find closing fence
  let end = after_fence.rfind("```")?;
  Some(after_fence[..end].trim())
}

/// Classify a user message to determine if it contains extractable signals
pub async fn classify_signal(provider: &dyn LlmProvider, user_message: &str) -> Result<SignalClassification> {
  debug!(
      provider = provider.name(),
      message_len = user_message.len(),
      message_preview = %user_message.chars().take(100).collect::<String>(),
      "Starting signal classification"
  );

  let prompt = build_signal_classification_prompt(user_message);

  let request = InferenceRequest {
    prompt,
    model: "haiku".to_string(),
    timeout_secs: 30,
    json_schema: SIGNAL_CLASSIFICATION_SCHEMA.to_string(),
    ..Default::default()
  };

  let response = provider.infer(request).await?;
  let classification: SignalClassification = parse_json(&response.text)?;

  debug!(
      category = ?classification.category,
      is_extractable = classification.is_extractable,
      is_high_priority = classification.category.is_high_priority(),
      summary = ?classification.summary,
      "Signal classification complete"
  );

  if classification.category.is_high_priority() {
    debug!(
        category = ?classification.category,
        "Detected high-priority signal requiring immediate extraction"
    );
  }

  Ok(classification)
}

/// Extract memories from a conversation segment
pub async fn extract_memories(provider: &dyn LlmProvider, context: &ExtractionContext) -> Result<ExtractionResult> {
  debug!(
    provider = provider.name(),
    tool_call_count = context.tool_call_count,
    files_read = context.files_read.len(),
    files_modified = context.files_modified.len(),
    commands_run = context.commands_run.len(),
    errors_encountered = context.errors_encountered.len(),
    completed_tasks = context.completed_tasks.len(),
    has_user_prompt = context.user_prompt.is_some(),
    has_assistant_message = context.last_assistant_message.is_some(),
    "Starting memory extraction"
  );

  // Check if we have enough content to extract
  if !context.has_meaningful_content() {
    debug!(
      tool_call_count = context.tool_call_count,
      files_modified = context.files_modified.len(),
      "Skipping extraction - insufficient content for meaningful memories"
    );
    return Ok(ExtractionResult { memories: Vec::new() });
  }

  let prompt = build_extraction_prompt(context);
  trace!(prompt_len = prompt.len(), "Built extraction prompt");

  let request = InferenceRequest {
    prompt,
    system_prompt: Some(EXTRACTION_SYSTEM_PROMPT.to_string()),
    model: "haiku".to_string(),
    timeout_secs: 60,
    json_schema: EXTRACTION_SCHEMA.to_string(),
  };

  debug!("Calling LLM for memory extraction");
  let response = provider.infer(request).await?;
  let result: ExtractionResult = parse_json(&response.text)?;

  if result.memories.is_empty() {
    debug!(
      input_tokens = response.input_tokens,
      output_tokens = response.output_tokens,
      "No memories extracted from context"
    );
  } else {
    // Log summary of extracted memory types
    let memory_types: Vec<&str> = result.memories.iter().map(|m| m.memory_type.as_str()).collect();
    let avg_confidence: f32 = result.memories.iter().map(|m| m.confidence).sum::<f32>() / result.memories.len() as f32;

    info!(
        memories_extracted = result.memories.len(),
        memory_types = ?memory_types,
        avg_confidence = format!("{:.2}", avg_confidence),
        input_tokens = response.input_tokens,
        output_tokens = response.output_tokens,
        "Memory extraction completed"
    );

    // Log individual memories at trace level
    for (i, memory) in result.memories.iter().enumerate() {
      trace!(
          index = i,
          memory_type = %memory.memory_type,
          confidence = memory.confidence,
          tags = ?memory.tags,
          content_len = memory.content.len(),
          "Extracted memory"
      );
    }
  }

  Ok(result)
}

/// Detect if a new memory supersedes any existing memories
///
/// Takes the new memory content and a list of candidate existing memories
/// (typically found via embedding similarity search).
pub async fn detect_superseding(
  provider: &dyn LlmProvider,
  new_memory: &str,
  existing_memories: &[(String, String)], // (id, content)
) -> Result<SupersedingResult> {
  debug!(
    provider = provider.name(),
    new_memory_len = new_memory.len(),
    candidate_count = existing_memories.len(),
    "Starting superseding detection"
  );

  if existing_memories.is_empty() {
    debug!("No existing memories to check for superseding");
    return Ok(SupersedingResult {
      supersedes: false,
      superseded_memory_id: None,
      reason: None,
      confidence: 1.0,
    });
  }

  // Log candidate IDs at trace level
  trace!(
      candidate_ids = ?existing_memories.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
      "Checking candidates for superseding"
  );

  let prompt = build_superseding_prompt(new_memory, existing_memories);
  trace!(prompt_len = prompt.len(), "Built superseding prompt");

  let request = InferenceRequest {
    prompt,
    model: "haiku".to_string(),
    timeout_secs: 30,
    json_schema: SUPERSEDING_SCHEMA.to_string(),
    ..Default::default()
  };

  debug!("Calling LLM for superseding detection");
  let response = provider.infer(request).await?;
  let result: SupersedingResult = parse_json(&response.text)?;

  if result.supersedes {
    info!(
        superseded_id = ?result.superseded_memory_id,
        reason = ?result.reason,
        confidence = result.confidence,
        candidates_checked = existing_memories.len(),
        "Detected memory supersession"
    );
  } else {
    debug!(
      candidates_checked = existing_memories.len(),
      confidence = result.confidence,
      "No superseding relationship detected"
    );
  }

  Ok(result)
}

/// High-priority extraction for corrections and preferences
///
/// Triggered immediately when a high-priority signal is detected.
pub async fn extract_high_priority(
  provider: &dyn LlmProvider,
  user_message: &str,
  classification: &SignalClassification,
) -> Result<ExtractionResult> {
  debug!(
      provider = provider.name(),
      category = ?classification.category,
      is_extractable = classification.is_extractable,
      message_len = user_message.len(),
      "Starting high-priority extraction"
  );

  if !classification.is_extractable {
    debug!(
        category = ?classification.category,
        "Skipping high-priority extraction - signal not extractable"
    );
    return Ok(ExtractionResult { memories: Vec::new() });
  }

  // Build a minimal context just for this message
  let context = ExtractionContext {
    user_prompt: Some(user_message.to_string()),
    tool_call_count: 1, // Force meaningful content check to pass
    ..Default::default()
  };

  let signal_type = match classification.category {
    SignalCategory::Correction => "CORRECTION",
    SignalCategory::Preference => "PREFERENCE",
    _ => "SIGNAL",
  };

  debug!(signal_type = signal_type, "Building high-priority extraction prompt");

  let prompt = format!(
    "This is a high-priority {} signal. Extract the memory immediately.\n\n{}",
    signal_type,
    build_extraction_prompt(&context)
  );
  trace!(prompt_len = prompt.len(), "Built high-priority extraction prompt");

  let request = InferenceRequest {
    prompt,
    system_prompt: Some(EXTRACTION_SYSTEM_PROMPT.to_string()),
    model: "haiku".to_string(),
    timeout_secs: 30,
    json_schema: EXTRACTION_SCHEMA.to_string(),
  };

  debug!("Calling LLM for high-priority extraction");
  let response = provider.infer(request).await?;
  let result: ExtractionResult = parse_json(&response.text)?;

  if result.memories.is_empty() {
    warn!(
        category = ?classification.category,
        signal_type = signal_type,
        "High-priority extraction yielded no memories"
    );
  } else {
    let memory_types: Vec<&str> = result.memories.iter().map(|m| m.memory_type.as_str()).collect();
    info!(
        memories_extracted = result.memories.len(),
        memory_types = ?memory_types,
        category = ?classification.category,
        signal_type = signal_type,
        input_tokens = response.input_tokens,
        output_tokens = response.output_tokens,
        "High-priority extraction completed"
    );
  }

  Ok(result)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::create_provider;

  // These tests require an LLM provider to be available

  #[tokio::test]
  #[ignore = "requires LLM provider"]
  async fn test_classify_correction_signal() {
    let provider = create_provider().unwrap();
    let result = classify_signal(&*provider, "No, use spaces not tabs for indentation")
      .await
      .unwrap();

    assert!(result.category == SignalCategory::Correction || result.category == SignalCategory::Preference);
    assert!(result.is_extractable);
  }

  #[tokio::test]
  #[ignore = "requires LLM provider"]
  async fn test_classify_task_signal() {
    let provider = create_provider().unwrap();
    let result = classify_signal(&*provider, "Please implement the login feature")
      .await
      .unwrap();

    assert_eq!(result.category, SignalCategory::Task);
  }

  #[tokio::test]
  #[ignore = "requires LLM provider"]
  async fn test_extract_from_context() {
    let provider = create_provider().unwrap();
    let context = ExtractionContext {
      user_prompt: Some("I always prefer using Result over panicking".into()),
      files_modified: vec!["src/lib.rs".into()],
      tool_call_count: 5,
      ..Default::default()
    };

    let result = extract_memories(&*provider, &context).await.unwrap();

    // Should extract at least one memory about error handling preference
    assert!(!result.memories.is_empty());
  }

  #[tokio::test]
  #[ignore = "requires LLM provider"]
  async fn test_detect_superseding_yes() {
    let provider = create_provider().unwrap();
    let existing = vec![("mem1".to_string(), "The project uses tabs for indentation".to_string())];

    let result = detect_superseding(
      &*provider,
      "The project now uses spaces for indentation (2 spaces)",
      &existing,
    )
    .await
    .unwrap();

    assert!(result.supersedes);
    assert_eq!(result.superseded_memory_id, Some("mem1".to_string()));
  }

  #[tokio::test]
  #[ignore = "requires LLM provider"]
  async fn test_detect_superseding_no() {
    let provider = create_provider().unwrap();
    let existing = vec![("mem1".to_string(), "The project uses tabs for indentation".to_string())];

    let result = detect_superseding(&*provider, "The database uses PostgreSQL", &existing)
      .await
      .unwrap();

    assert!(!result.supersedes);
  }
}
