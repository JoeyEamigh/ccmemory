//! LLM provider trait for inference
//!
//! This module defines the `LlmProvider` trait that different LLM backends
//! can implement to provide inference capabilities.

use async_trait::async_trait;
use dyn_clone::DynClone;

use crate::{InferenceRequest, InferenceResponse, LlmError};

/// Result type for LLM operations
pub type Result<T> = std::result::Result<T, LlmError>;

/// Trait for LLM inference providers
///
/// Implement this trait to add support for different LLM backends.
///
/// # Example
///
/// ```ignore
/// use llm::{LlmProvider, InferenceRequest, InferenceResponse, Result};
///
/// struct MyProvider;
///
/// #[async_trait::async_trait]
/// impl LlmProvider for MyProvider {
///     fn name(&self) -> &str {
///         "my-provider"
///     }
///
///     fn is_available(&self) -> bool {
///         true
///     }
///
///     async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
///         // Implement inference logic
///         todo!()
///     }
/// }
/// ```
#[async_trait]
pub trait LlmProvider: Send + Sync + DynClone {
  /// The name of this provider (for logging/identification)
  fn name(&self) -> &str;

  /// Check if this provider is available/configured
  ///
  /// Returns `true` if the provider can be used for inference.
  /// This might check for API keys, CLI availability, etc.
  fn is_available(&self) -> bool;

  /// Perform inference with the given request
  ///
  /// # Arguments
  ///
  /// * `request` - The inference request containing the prompt and configuration
  ///
  /// # Returns
  ///
  /// The inference response containing the generated text and usage statistics
  async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse>;
}

dyn_clone::clone_trait_object!(LlmProvider);
