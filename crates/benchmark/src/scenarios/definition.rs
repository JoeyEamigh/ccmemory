//! Scenario definition types (TOML schema).

use crate::repos::TargetRepo;
use crate::{BenchmarkError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Difficulty level for a scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
  Easy,
  #[default]
  Medium,
  Hard,
}

/// Intent type for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskIntent {
  #[default]
  ArchitecturalDiscovery,
  SymbolLookup,
  FlowTracing,
  BugInvestigation,
  FeatureExploration,
}

/// Scenario metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMetadata {
  /// Unique scenario ID
  pub id: String,
  /// Human-readable name
  pub name: String,
  /// Target repository
  pub repo: TargetRepo,
  /// Difficulty level
  #[serde(default)]
  pub difficulty: Difficulty,
  /// Optional description
  #[serde(default)]
  pub description: Option<String>,
}

/// Task definition within a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
  /// The exploration prompt/question
  pub prompt: String,
  /// Intent of the task
  #[serde(default)]
  pub intent: TaskIntent,
}

/// Expected results for a scenario.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Expected {
  /// Files that must be found (glob patterns allowed)
  #[serde(default)]
  pub must_find_files: Vec<String>,
  /// Symbols that must be found
  #[serde(default)]
  pub must_find_symbols: Vec<String>,
  /// Patterns that indicate noise results
  #[serde(default)]
  pub noise_patterns: Vec<String>,
  /// Optional specific file:line locations
  #[serde(default)]
  pub must_find_locations: Vec<String>,
}

/// A single step in a multi-step scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
  /// Query to execute (supports template syntax for adaptive exploration)
  ///
  /// Template syntax:
  /// - `{{previous.symbol}}` - First symbol from previous step
  /// - `{{previous.symbols[N]}}` - Nth symbol from previous step
  /// - `{{previous.file}}` - First file from previous step
  /// - `{{previous.files[N]}}` - Nth file from previous step
  /// - `{{previous.caller}}` - First caller symbol from previous context
  /// - `{{previous.callee}}` - First callee symbol from previous context
  pub query: String,
  /// Expected number of useful results
  #[serde(default)]
  pub expected_results: Option<usize>,
  /// Maximum acceptable noise ratio
  #[serde(default)]
  pub max_noise_ratio: Option<f64>,
  /// Whether this step depends on previous step results
  #[serde(default)]
  pub depends_on_previous: bool,
  /// Scope to search (code, memory, docs, all)
  #[serde(default)]
  pub scope: Option<String>,
  /// Optional: IDs to fetch context for (simulating follow-up)
  /// Also supports templates: `{{previous.id}}`, `{{previous.ids[N]}}`
  #[serde(default)]
  pub context_ids: Vec<String>,
}

impl Step {
  /// Check if this step's query contains templates.
  pub fn has_templates(&self) -> bool {
    self.query.contains("{{") || self.context_ids.iter().any(|id| id.contains("{{"))
  }
}

/// Results from a previous step, used for template resolution.
#[derive(Debug, Clone, Default)]
pub struct PreviousStepResults {
  /// Result IDs
  pub ids: Vec<String>,
  /// Files found
  pub files: Vec<String>,
  /// Symbols found
  pub symbols: Vec<String>,
  /// Caller symbols (from expanded context)
  pub callers: Vec<String>,
  /// Callee symbols (from expanded context)
  pub callees: Vec<String>,
}

impl PreviousStepResults {
  /// Resolve template placeholders in a string.
  pub fn resolve_template(&self, template: &str) -> String {
    use regex::Regex;

    let mut result = template.to_string();

    // Simple replacements
    result = result.replace("{{previous.symbol}}", self.symbols.first().unwrap_or(&String::new()));
    result = result.replace("{{previous.file}}", self.files.first().unwrap_or(&String::new()));
    result = result.replace("{{previous.id}}", self.ids.first().unwrap_or(&String::new()));
    result = result.replace("{{previous.caller}}", self.callers.first().unwrap_or(&String::new()));
    result = result.replace("{{previous.callee}}", self.callees.first().unwrap_or(&String::new()));

    // Indexed replacements (e.g., {{previous.symbols[1]}})
    if let Ok(re) = Regex::new(r"\{\{previous\.symbols\[(\d+)\]\}\}") {
      result = re
        .replace_all(&result, |caps: &regex::Captures| -> String {
          let idx: usize = caps[1].parse().unwrap_or(0);
          self.symbols.get(idx).cloned().unwrap_or_default()
        })
        .to_string();
    }

    if let Ok(re) = Regex::new(r"\{\{previous\.files\[(\d+)\]\}\}") {
      result = re
        .replace_all(&result, |caps: &regex::Captures| -> String {
          let idx: usize = caps[1].parse().unwrap_or(0);
          self.files.get(idx).cloned().unwrap_or_default()
        })
        .to_string();
    }

    if let Ok(re) = Regex::new(r"\{\{previous\.ids\[(\d+)\]\}\}") {
      result = re
        .replace_all(&result, |caps: &regex::Captures| -> String {
          let idx: usize = caps[1].parse().unwrap_or(0);
          self.ids.get(idx).cloned().unwrap_or_default()
        })
        .to_string();
    }

    if let Ok(re) = Regex::new(r"\{\{previous\.callers\[(\d+)\]\}\}") {
      result = re
        .replace_all(&result, |caps: &regex::Captures| -> String {
          let idx: usize = caps[1].parse().unwrap_or(0);
          self.callers.get(idx).cloned().unwrap_or_default()
        })
        .to_string();
    }

    if let Ok(re) = Regex::new(r"\{\{previous\.callees\[(\d+)\]\}\}") {
      result = re
        .replace_all(&result, |caps: &regex::Captures| -> String {
          let idx: usize = caps[1].parse().unwrap_or(0);
          self.callees.get(idx).cloned().unwrap_or_default()
        })
        .to_string();
    }

    result
  }

  /// Resolve template in a step, creating a new step with resolved query.
  pub fn resolve_step(&self, step: &Step) -> Step {
    let mut resolved = step.clone();
    resolved.query = self.resolve_template(&step.query);
    resolved.context_ids = step.context_ids.iter().map(|id| self.resolve_template(id)).collect();
    resolved
  }
}

/// Comprehension question for LLM-as-judge evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComprehensionQuestion {
  /// The question to ask
  pub question: String,
  /// Key concepts or facts that should be mentioned in the answer
  #[serde(default)]
  pub expected_concepts: Vec<String>,
  /// Incorrect concepts that should NOT appear (indicates misunderstanding)
  #[serde(default)]
  pub wrong_concepts: Vec<String>,
  /// Weight of this question (1.0 = standard, higher = more important)
  #[serde(default = "default_weight")]
  pub weight: f64,
}

fn default_weight() -> f64 {
  1.0
}

/// LLM judge configuration for comprehension testing.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmJudgeConfig {
  /// Comprehension questions to evaluate understanding
  #[serde(default)]
  pub comprehension_questions: Vec<ComprehensionQuestion>,
  /// Minimum comprehension score required (0.0-1.0)
  #[serde(default)]
  pub min_comprehension_score: Option<f64>,
}

/// Success criteria for a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
  /// Minimum file discovery score (0.0-1.0)
  #[serde(default = "default_min_discovery_score")]
  pub min_discovery_score: f64,
  /// Maximum acceptable noise ratio (0.0-1.0)
  #[serde(default = "default_max_noise_ratio")]
  pub max_noise_ratio: f64,
  /// Maximum steps to find first core result
  #[serde(default = "default_max_steps_to_core")]
  pub max_steps_to_core: usize,
  /// Minimum MRR (mean reciprocal rank)
  #[serde(default)]
  pub min_mrr: Option<f64>,
  /// Minimum hint utility score
  #[serde(default)]
  pub min_hint_utility: Option<f64>,

  // === Exploration-specific criteria ===
  /// Minimum convergence rate (how quickly discoveries plateau, target >= 0.7)
  #[serde(default)]
  pub min_convergence_rate: Option<f64>,
  /// Maximum context bloat (% of empty context calls, target <= 0.3)
  #[serde(default)]
  pub max_context_bloat: Option<f64>,
  /// Minimum navigation efficiency (optimal_hops/actual_hops, target >= 0.5)
  #[serde(default)]
  pub min_navigation_efficiency: Option<f64>,
  /// Minimum suggestion quality (% of useful suggestions, target >= 0.5)
  #[serde(default)]
  pub min_suggestion_quality: Option<f64>,
  /// Maximum dead end ratio (% of steps with no discoveries, target <= 0.2)
  #[serde(default)]
  pub max_dead_end_ratio: Option<f64>,
}

fn default_min_discovery_score() -> f64 {
  0.7
}

fn default_max_noise_ratio() -> f64 {
  0.25
}

fn default_max_steps_to_core() -> usize {
  3
}

impl Default for SuccessCriteria {
  fn default() -> Self {
    Self {
      min_discovery_score: default_min_discovery_score(),
      max_noise_ratio: default_max_noise_ratio(),
      max_steps_to_core: default_max_steps_to_core(),
      min_mrr: None,
      min_hint_utility: None,
      min_convergence_rate: None,
      max_context_bloat: None,
      min_navigation_efficiency: None,
      min_suggestion_quality: None,
      max_dead_end_ratio: None,
    }
  }
}

/// Complete scenario definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
  /// Scenario metadata
  #[serde(rename = "scenario")]
  pub metadata: ScenarioMetadata,
  /// Task definition
  pub task: Task,
  /// Expected results
  #[serde(default)]
  pub expected: Expected,
  /// Multi-step exploration steps
  #[serde(default)]
  pub steps: Vec<Step>,
  /// Success criteria
  #[serde(default, rename = "success")]
  pub success_criteria: SuccessCriteria,
  /// LLM judge configuration for comprehension testing
  #[serde(default)]
  pub llm_judge: LlmJudgeConfig,
}

impl Scenario {
  /// Load a scenario from a TOML file.
  pub fn load(path: &Path) -> Result<Self> {
    let content = std::fs::read_to_string(path)?;
    let scenario: Scenario = toml::from_str(&content)?;
    scenario.validate()?;
    Ok(scenario)
  }

  /// Validate the scenario definition.
  pub fn validate(&self) -> Result<()> {
    if self.metadata.id.is_empty() {
      return Err(BenchmarkError::Scenario("Scenario ID cannot be empty".into()));
    }
    if self.metadata.name.is_empty() {
      return Err(BenchmarkError::Scenario("Scenario name cannot be empty".into()));
    }
    if self.task.prompt.is_empty() {
      return Err(BenchmarkError::Scenario("Task prompt cannot be empty".into()));
    }
    if self.steps.is_empty() {
      return Err(BenchmarkError::Scenario("Scenario must have at least one step".into()));
    }
    for (i, step) in self.steps.iter().enumerate() {
      if step.query.is_empty() {
        return Err(BenchmarkError::Scenario(format!(
          "Step {} query cannot be empty",
          i + 1
        )));
      }
    }
    Ok(())
  }

  /// Create a test scenario (for unit tests).
  #[cfg(test)]
  pub fn new_test(id: &str, name: &str) -> Self {
    Self {
      metadata: ScenarioMetadata {
        id: id.to_string(),
        name: name.to_string(),
        repo: TargetRepo::Zed,
        difficulty: Difficulty::Medium,
        description: None,
      },
      task: Task {
        prompt: "Test prompt".to_string(),
        intent: TaskIntent::ArchitecturalDiscovery,
      },
      expected: Expected::default(),
      steps: vec![Step {
        query: "test query".to_string(),
        expected_results: None,
        max_noise_ratio: None,
        depends_on_previous: false,
        scope: None,
        context_ids: vec![],
      }],
      success_criteria: SuccessCriteria::default(),
      llm_judge: LlmJudgeConfig::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE_TOML: &str = r#"
[scenario]
id = "zed-command-system"
name = "Understanding Zed Command Architecture"
repo = "zed"
difficulty = "hard"

[task]
prompt = "How does Zed handle editor commands?"
intent = "architectural_discovery"

[expected]
must_find_files = ["crates/zed/src/commands.rs", "crates/gpui/src/keymap.rs"]
must_find_symbols = ["Command", "execute", "Keymap"]
noise_patterns = ["**/tests/**", "test_*", "Mock*"]

[[steps]]
query = "How does Zed handle editor commands?"
expected_results = 5
max_noise_ratio = 0.3

[[steps]]
query = "What is the Command type and how is it dispatched?"
depends_on_previous = true

[success]
min_discovery_score = 0.7
max_noise_ratio = 0.25
max_steps_to_core = 3
"#;

  #[test]
  fn test_parse_scenario() {
    let scenario: Scenario = toml::from_str(SAMPLE_TOML).unwrap();

    assert_eq!(scenario.metadata.id, "zed-command-system");
    assert_eq!(scenario.metadata.repo, TargetRepo::Zed);
    assert_eq!(scenario.metadata.difficulty, Difficulty::Hard);
    assert_eq!(scenario.task.intent, TaskIntent::ArchitecturalDiscovery);
    assert_eq!(scenario.expected.must_find_files.len(), 2);
    assert_eq!(scenario.expected.must_find_symbols.len(), 3);
    assert_eq!(scenario.steps.len(), 2);
    assert!(scenario.steps[1].depends_on_previous);
  }

  #[test]
  fn test_validate_scenario() {
    let scenario: Scenario = toml::from_str(SAMPLE_TOML).unwrap();
    assert!(scenario.validate().is_ok());
  }

  #[test]
  fn test_validate_empty_id() {
    let mut scenario: Scenario = toml::from_str(SAMPLE_TOML).unwrap();
    scenario.metadata.id = "".to_string();
    assert!(scenario.validate().is_err());
  }

  #[test]
  fn test_default_success_criteria() {
    let criteria = SuccessCriteria::default();
    assert!((criteria.min_discovery_score - 0.7).abs() < f64::EPSILON);
    assert!((criteria.max_noise_ratio - 0.25).abs() < f64::EPSILON);
    assert_eq!(criteria.max_steps_to_core, 3);
  }

  #[test]
  fn test_difficulty_default() {
    assert_eq!(Difficulty::default(), Difficulty::Medium);
  }

  #[test]
  fn test_task_intent_default() {
    assert_eq!(TaskIntent::default(), TaskIntent::ArchitecturalDiscovery);
  }
}
