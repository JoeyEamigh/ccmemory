//! Metrics collection for benchmarking.
//!
//! Two categories:
//! - Performance: latency, throughput, resource usage
//! - Accuracy: discovery rate, noise ratio, navigation quality

mod accuracy;
pub mod performance;

pub use accuracy::{
  AccuracyMetrics, BloatDiagnosis, ConvergenceDiagnosis, DiscoveryPattern, ExplorationDiagnostics, OverExpandedStep,
  RecallCategoryBreakdown, RecallDiagnosis,
};
pub use performance::{
  BatchChangeResult, FileOperationsResult, GitignoreResult, IncrementalBenchResult, IncrementalReport,
  IncrementalSummary, IndexingMetrics, LargeFileBenchResult, LatencyTracker, OperationResult, PerformanceMetrics,
  ResourceMonitor, SingleChangeResult, StepMetrics, WatcherLifecycleResult, WatcherReport, WatcherSummary,
};

/// All metrics targets from the plan.
pub struct MetricTargets {
  /// File recall target (≥70%)
  pub file_recall: f64,
  /// Symbol recall target (≥70%)
  pub symbol_recall: f64,
  /// Max steps to core (≤3)
  pub max_steps_to_core: usize,
  /// MRR target (≥0.5)
  pub mrr: f64,
  /// Max noise ratio (≤25%)
  pub noise_ratio: f64,
  /// Hint utility target (≥60%)
  pub hint_utility: f64,
  /// Convergence rate target (≥70%)
  pub convergence_rate: f64,
  /// Navigation efficiency target (≥50%)
  pub navigation_efficiency: f64,
  /// Max context bloat (≤30%)
  pub context_bloat: f64,
  /// Max dead end ratio (≤20%)
  pub dead_end_ratio: f64,
  /// File diversity target (≥60%)
  pub file_diversity: f64,
}

impl Default for MetricTargets {
  fn default() -> Self {
    Self {
      file_recall: 0.70,
      symbol_recall: 0.70,
      max_steps_to_core: 3,
      mrr: 0.50,
      noise_ratio: 0.25,
      hint_utility: 0.60,
      convergence_rate: 0.70,
      navigation_efficiency: 0.50,
      context_bloat: 0.30,
      dead_end_ratio: 0.20,
      file_diversity: 0.60,
    }
  }
}
