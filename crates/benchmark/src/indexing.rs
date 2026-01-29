//! Indexing performance benchmarking.
//!
//! Measures initial indexing performance for repositories, including
//! scan time, chunking throughput, embedding generation, and resource usage.
//! Also includes incremental indexing and large file handling benchmarks.

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use ccengram::ipc::{
  Client,
  code::{CodeIndexParams, CodeIndexResult, CodeSearchParams, CodeStatsParams},
  project::ProjectCleanParams,
  watch::{WatchStartParams, WatchStatusParams, WatchStopParams},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{
  Result,
  fixtures::FixtureGenerator,
  metrics::{
    IncrementalBenchResult, IncrementalReport, IncrementalSummary, IndexingMetrics, LargeFileBenchResult,
    ResourceMonitor,
  },
  repos::{TargetRepo, prepare_repo},
};

/// Result of a single indexing benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingBenchResult {
  /// Repository name
  pub repo: String,
  /// Iteration number (0-indexed)
  pub iteration: usize,
  /// Whether this was a cold start (no cache)
  pub cold_start: bool,
  /// Detailed indexing metrics
  pub metrics: IndexingMetrics,
  /// Files scanned
  pub files_scanned: usize,
  /// Files indexed
  pub files_indexed: usize,
  /// Bytes processed
  pub bytes_processed: u64,
}

/// Aggregate statistics across multiple indexing runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingSummary {
  /// Repository name
  pub repo: String,
  /// Number of iterations
  pub iterations: usize,
  /// Average wall time in milliseconds
  pub avg_wall_time_ms: u64,
  /// Median wall time in milliseconds
  pub p50_wall_time_ms: u64,
  /// 95th percentile wall time in milliseconds
  pub p95_wall_time_ms: u64,
  /// Average chunks per second
  pub avg_chunks_per_sec: f64,
  /// Average embeddings per second
  pub avg_embeddings_per_sec: f64,
  /// Peak memory usage in bytes
  pub peak_memory_bytes: u64,
  /// Average files per second
  pub avg_files_per_sec: f64,
}

/// Full indexing benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingReport {
  /// Timestamp of the benchmark run
  pub timestamp: String,
  /// CCEngram version
  pub version: String,
  /// Individual run results
  pub results: Vec<IndexingBenchResult>,
  /// Per-repository summaries
  pub summaries: Vec<IndexingSummary>,
}

impl IndexingReport {
  /// Create a new report from results.
  pub fn from_results(results: Vec<IndexingBenchResult>) -> Self {
    let summaries = Self::compute_summaries(&results);

    Self {
      timestamp: chrono::Utc::now().to_rfc3339(),
      version: env!("CARGO_PKG_VERSION").to_string(),
      results,
      summaries,
    }
  }

  fn compute_summaries(results: &[IndexingBenchResult]) -> Vec<IndexingSummary> {
    use std::collections::HashMap;

    // Group by repo
    let mut by_repo: HashMap<String, Vec<&IndexingBenchResult>> = HashMap::new();
    for result in results {
      by_repo.entry(result.repo.clone()).or_default().push(result);
    }

    by_repo
      .into_iter()
      .map(|(repo, runs)| {
        let mut wall_times: Vec<u64> = runs.iter().map(|r| r.metrics.wall_time_ms).collect();
        wall_times.sort_unstable();

        let iterations = runs.len();
        let avg_wall_time_ms = wall_times.iter().sum::<u64>() / iterations as u64;
        let p50_wall_time_ms = wall_times.get(iterations / 2).copied().unwrap_or(0);
        let p95_wall_time_ms = wall_times
          .get((iterations as f64 * 0.95) as usize)
          .copied()
          .unwrap_or(*wall_times.last().unwrap_or(&0));

        let avg_chunks_per_sec = runs.iter().map(|r| r.metrics.chunks_per_sec).sum::<f64>() / iterations as f64;
        let avg_embeddings_per_sec = runs.iter().map(|r| r.metrics.embeddings_per_sec).sum::<f64>() / iterations as f64;
        let peak_memory_bytes = runs.iter().map(|r| r.metrics.peak_memory_bytes).max().unwrap_or(0);

        let avg_files_per_sec = runs
          .iter()
          .map(|r| {
            if r.metrics.wall_time_ms > 0 {
              r.files_indexed as f64 / (r.metrics.wall_time_ms as f64 / 1000.0)
            } else {
              0.0
            }
          })
          .sum::<f64>()
          / iterations as f64;

        IndexingSummary {
          repo,
          iterations,
          avg_wall_time_ms,
          p50_wall_time_ms,
          p95_wall_time_ms,
          avg_chunks_per_sec,
          avg_embeddings_per_sec,
          peak_memory_bytes,
          avg_files_per_sec,
        }
      })
      .collect()
  }

  /// Generate markdown report.
  pub fn to_markdown(&self) -> String {
    let mut out = String::new();

    out.push_str("# Indexing Performance Report\n\n");
    out.push_str(&format!("**Timestamp:** {}\n", self.timestamp));
    out.push_str(&format!("**Version:** {}\n\n", self.version));

    out.push_str("## Summary\n\n");
    out.push_str("| Repository | Iterations | Avg Time | p50 | p95 | Chunks/sec | Files/sec | Peak Memory |\n");
    out.push_str("|------------|------------|----------|-----|-----|------------|-----------|-------------|\n");

    for summary in &self.summaries {
      out.push_str(&format!(
        "| {} | {} | {:.1}s | {:.1}s | {:.1}s | {:.0} | {:.0} | {:.1} MB |\n",
        summary.repo,
        summary.iterations,
        summary.avg_wall_time_ms as f64 / 1000.0,
        summary.p50_wall_time_ms as f64 / 1000.0,
        summary.p95_wall_time_ms as f64 / 1000.0,
        summary.avg_chunks_per_sec,
        summary.avg_files_per_sec,
        summary.peak_memory_bytes as f64 / (1024.0 * 1024.0),
      ));
    }

    out.push_str("\n## Detailed Results\n\n");

    for result in &self.results {
      out.push_str(&format!("### {} (iteration {})\n\n", result.repo, result.iteration));
      out.push_str(&format!("- **Cold start:** {}\n", result.cold_start));
      out.push_str(&format!(
        "- **Wall time:** {:.2}s\n",
        result.metrics.wall_time_ms as f64 / 1000.0
      ));
      out.push_str(&format!("- **Files scanned:** {}\n", result.files_scanned));
      out.push_str(&format!("- **Files indexed:** {}\n", result.files_indexed));
      out.push_str(&format!("- **Chunks created:** {}\n", result.metrics.chunks_processed));
      out.push_str(&format!("- **Chunks/sec:** {:.0}\n", result.metrics.chunks_per_sec));
      out.push_str(&format!(
        "- **Embeddings/sec:** {:.0}\n",
        result.metrics.embeddings_per_sec
      ));
      out.push_str(&format!(
        "- **Peak memory:** {:.1} MB\n",
        result.metrics.peak_memory_bytes as f64 / (1024.0 * 1024.0)
      ));
      out.push_str(&format!(
        "- **Bytes processed:** {:.1} MB\n\n",
        result.bytes_processed as f64 / (1024.0 * 1024.0)
      ));
    }

    out
  }

  /// Save report to files (JSON and Markdown).
  pub async fn save(&self, output_dir: &PathBuf) -> Result<()> {
    tokio::fs::create_dir_all(output_dir).await?;

    // Save JSON
    let json_path = output_dir.join("indexing.json");
    let json = serde_json::to_string_pretty(self)?;
    tokio::fs::write(&json_path, json).await?;
    info!("Saved JSON report: {}", json_path.display());

    // Save Markdown
    let md_path = output_dir.join("indexing.md");
    tokio::fs::write(&md_path, self.to_markdown()).await?;
    info!("Saved Markdown report: {}", md_path.display());

    Ok(())
  }
}

/// Indexing benchmark runner.
pub struct IndexingBenchmark {
  client: Client,
  cache_dir: Option<PathBuf>,
}

impl IndexingBenchmark {
  /// Create a new indexing benchmark runner.
  pub fn new(client: Client, cache_dir: Option<PathBuf>) -> Self {
    Self { client, cache_dir }
  }

  /// Check if the daemon is running.
  pub async fn check_daemon(&self) -> bool {
    use ccengram::ipc::system::HealthCheckParams;

    match self.client.call(HealthCheckParams).await {
      Ok(result) => result.healthy,
      Err(_) => false,
    }
  }

  /// Run indexing benchmark for specified repositories.
  pub async fn run(&mut self, repos: &[TargetRepo], iterations: usize, cold_start: bool) -> Result<IndexingReport> {
    let mut results = Vec::new();

    for repo in repos {
      info!("Benchmarking indexing for: {}", repo);

      // Prepare repository (download if needed)
      let repo_path = prepare_repo(*repo, self.cache_dir.clone()).await?;
      info!("Repository path: {}", repo_path.display());

      for i in 0..iterations {
        info!("  Iteration {}/{}", i + 1, iterations);

        // If cold start requested and not first iteration, clear the index
        if cold_start && i > 0 {
          self.clear_index(&repo_path).await?;
        }

        // Run indexing and collect metrics
        let result = self
          .run_single_index(&repo.to_string(), &repo_path, i, cold_start && i == 0)
          .await?;
        results.push(result);
      }
    }

    Ok(IndexingReport::from_results(results))
  }

  /// Run a single indexing operation and collect metrics.
  async fn run_single_index(
    &mut self,
    repo_name: &str,
    repo_path: &Path,
    iteration: usize,
    cold_start: bool,
  ) -> Result<IndexingBenchResult> {
    let mut monitor = ResourceMonitor::new();
    monitor.snapshot();

    let start = Instant::now();

    // Send index request to daemon
    self.client.change_cwd(repo_path.to_path_buf());
    let result: CodeIndexResult = self
      .client
      .call(CodeIndexParams {
        force: cold_start,
        stream: false,
      })
      .await?;
    let elapsed = start.elapsed();

    monitor.snapshot();

    // Extract fields from typed result
    let files_scanned = result.files_scanned;
    let files_indexed = result.files_indexed;
    let chunks_created = result.chunks_created;
    let bytes_processed = result.bytes_processed;

    let wall_time_ms = elapsed.as_millis() as u64;
    let chunks_per_sec = if wall_time_ms > 0 {
      chunks_created as f64 / (wall_time_ms as f64 / 1000.0)
    } else {
      0.0
    };

    // Estimate embeddings (typically 1 embedding per chunk)
    let embeddings_generated = chunks_created;
    let embeddings_per_sec = chunks_per_sec;

    let metrics = IndexingMetrics {
      wall_time_ms,
      peak_memory_bytes: monitor.peak_memory(),
      avg_cpu_percent: monitor.avg_cpu(),
      chunks_processed: chunks_created,
      chunks_per_sec,
      embeddings_generated,
      embeddings_per_sec,
    };

    debug!(
      "  Indexed {} files, {} chunks in {:.2}s ({:.0} chunks/sec)",
      files_indexed,
      chunks_created,
      wall_time_ms as f64 / 1000.0,
      chunks_per_sec
    );

    Ok(IndexingBenchResult {
      repo: repo_name.to_string(),
      iteration,
      cold_start,
      metrics,
      files_scanned,
      files_indexed,
      bytes_processed,
    })
  }

  /// Clear index for a project (for cold start testing).
  /// Note: clear_code_index is not yet in the ipc Method enum, using ProjectClean as workaround.
  async fn clear_index(&mut self, repo_path: &Path) -> Result<()> {
    self.client.change_cwd(repo_path.to_path_buf());
    let _ = self
      .client
      .call(ProjectCleanParams {
        project: Some(repo_path.to_string_lossy().to_string()),
      })
      .await?;

    Ok(())
  }
}

// ============================================================================
// Incremental Indexing Benchmark
// ============================================================================

/// Configuration for incremental indexing benchmarks.
#[derive(Debug, Clone)]
pub struct IncrementalBenchConfig {
  /// Number of files to modify per iteration
  pub files_per_iteration: usize,
  /// Number of iterations to run
  pub iterations: usize,
  /// Threshold for passing (ms per file)
  pub threshold_ms_per_file: u64,
}

impl Default for IncrementalBenchConfig {
  fn default() -> Self {
    Self {
      files_per_iteration: 10,
      iterations: 3,
      threshold_ms_per_file: 200, // Target: < 200ms per changed file
    }
  }
}

/// Incremental indexing benchmark runner.
pub struct IncrementalBenchmark {
  client: Client,
  cache_dir: Option<PathBuf>,
  config: IncrementalBenchConfig,
}

impl IncrementalBenchmark {
  /// Create a new incremental benchmark runner.
  pub fn new(client: Client, cache_dir: Option<PathBuf>) -> Self {
    Self {
      client,
      cache_dir,
      config: IncrementalBenchConfig::default(),
    }
  }

  /// Set benchmark configuration.
  pub fn with_config(mut self, config: IncrementalBenchConfig) -> Self {
    self.config = config;
    self
  }

  /// Run incremental indexing benchmark for specified repositories.
  pub async fn run(&mut self, repos: &[TargetRepo]) -> Result<IncrementalReport> {
    let mut results = Vec::new();
    let mut large_file_results = Vec::new();

    for repo in repos {
      info!("Running incremental benchmark for: {}", repo);

      let repo_path = prepare_repo(*repo, self.cache_dir.clone()).await?;
      self.client.change_cwd(repo_path.clone());

      // Ensure index exists first
      self.ensure_indexed(&repo_path).await?;

      // Run incremental tests
      for iteration in 0..self.config.iterations {
        info!("  Iteration {}/{}", iteration + 1, self.config.iterations);

        let result = self.run_incremental_iteration(&repo.to_string(), &repo_path).await?;
        results.push(result);
      }

      // Run large file tests
      let large_results = self.run_large_file_tests(&repo_path).await?;
      large_file_results.extend(large_results);
    }

    let summary = Self::compute_summary(&results, &large_file_results, self.config.threshold_ms_per_file);

    Ok(IncrementalReport {
      timestamp: chrono::Utc::now().to_rfc3339(),
      version: env!("CARGO_PKG_VERSION").to_string(),
      results,
      large_file_results,
      summary,
    })
  }

  /// Ensure the repository is indexed before incremental tests.
  async fn ensure_indexed(&self, _repo_path: &Path) -> Result<()> {
    let stats = self.client.call(CodeStatsParams).await?;

    if stats.total_chunks == 0 {
      info!("Repository not indexed, running initial index...");
      let _ = self
        .client
        .call(CodeIndexParams {
          force: false,
          stream: false,
        })
        .await?;
    }

    Ok(())
  }

  /// Run a single incremental indexing iteration.
  async fn run_incremental_iteration(&self, repo_name: &str, repo_path: &Path) -> Result<IncrementalBenchResult> {
    let mut fixtures = FixtureGenerator::new(repo_path).await?;
    let mut monitor = ResourceMonitor::new();

    // Get initial chunk count
    let initial_stats = self.client.call(CodeStatsParams).await?;
    let initial_chunks = initial_stats.total_chunks;

    // Create test files with unique markers
    let mut created_markers = Vec::new();
    for i in 0..self.config.files_per_iteration {
      let (_, marker) = fixtures.create_rust_file(&format!("incremental_{}", i)).await?;
      created_markers.push(marker);
    }

    monitor.snapshot();
    let start = Instant::now();

    // Start watcher and trigger reindex
    let _ = self.client.call(WatchStartParams).await;

    // Wait for watcher to process changes
    self.wait_for_watcher_ready().await?;

    // Trigger explicit reindex to ensure changes are processed
    let _ = self
      .client
      .call(CodeIndexParams {
        force: false,
        stream: false,
      })
      .await?;

    let elapsed = start.elapsed();
    monitor.snapshot();

    // Verify which files were indexed by searching for markers
    let mut true_positives = 0;
    let mut false_negatives = 0;

    for marker in &created_markers {
      let search_result = self
        .client
        .call(CodeSearchParams {
          query: marker.clone(),
          limit: Some(1),
          ..Default::default()
        })
        .await?;

      if search_result.chunks.is_empty() {
        false_negatives += 1;
      } else {
        true_positives += 1;
      }
    }

    // Check for false positives by comparing chunk counts
    let final_stats = self.client.call(CodeStatsParams).await?;
    let chunks_added = final_stats.total_chunks.saturating_sub(initial_chunks);

    // Rough heuristic: if we added significantly more chunks than files, might have false positives
    // This is imprecise but gives a signal
    let expected_chunks = self.config.files_per_iteration * 2; // Rough estimate
    let false_positives = if chunks_added > expected_chunks * 2 {
      chunks_added - expected_chunks
    } else {
      0
    };

    // Stop watcher
    let _ = self.client.call(WatchStopParams).await;

    // Cleanup fixtures
    fixtures.cleanup().await?;

    let total_time_ms = elapsed.as_millis() as u64;
    let time_per_file_ms = if self.config.files_per_iteration > 0 {
      total_time_ms as f64 / self.config.files_per_iteration as f64
    } else {
      0.0
    };

    Ok(IncrementalBenchResult {
      repo: repo_name.to_string(),
      files_modified: self.config.files_per_iteration,
      time_per_file_ms,
      true_positives,
      false_positives,
      false_negatives,
      total_time_ms,
      peak_memory_bytes: monitor.peak_memory(),
    })
  }

  /// Wait for the watcher to finish processing.
  async fn wait_for_watcher_ready(&self) -> Result<()> {
    let timeout = Duration::from_secs(30);
    let start = Instant::now();

    loop {
      if start.elapsed() > timeout {
        warn!("Timeout waiting for watcher to be ready");
        break;
      }

      let status = self.client.call(WatchStatusParams).await?;
      if !status.scanning && status.pending_changes == 0 {
        break;
      }

      tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
  }

  /// Run large file handling benchmarks.
  async fn run_large_file_tests(&self, repo_path: &Path) -> Result<Vec<LargeFileBenchResult>> {
    let sizes_mb = [1, 5, 10, 50];
    let mut results = Vec::new();

    for size_mb in sizes_mb {
      let size_bytes = size_mb as u64 * 1024 * 1024;
      info!("  Testing large file: {} MB", size_mb);

      let result = self.run_single_large_file_test(repo_path, size_bytes).await?;
      results.push(result);
    }

    Ok(results)
  }

  /// Run a single large file benchmark.
  async fn run_single_large_file_test(&self, repo_path: &Path, size_bytes: u64) -> Result<LargeFileBenchResult> {
    let mut fixtures = FixtureGenerator::new(repo_path).await?;
    let mut monitor = ResourceMonitor::new();

    // Get initial stats
    let initial_stats = self.client.call(CodeStatsParams).await?;
    let initial_chunks = initial_stats.total_chunks;

    // Create large file
    let (path, marker) = fixtures.create_large_file(size_bytes).await?;
    let actual_size = tokio::fs::metadata(&path).await?.len();

    monitor.snapshot();
    let start = Instant::now();

    // Trigger indexing
    let _ = self
      .client
      .call(CodeIndexParams {
        force: false,
        stream: false,
      })
      .await?;

    let elapsed = start.elapsed();
    monitor.snapshot();

    // Check if file was indexed
    let search_result = self
      .client
      .call(CodeSearchParams {
        query: marker.clone(),
        limit: Some(1),
        ..Default::default()
      })
      .await?;

    let final_stats = self.client.call(CodeStatsParams).await?;
    let chunks_added = final_stats.total_chunks.saturating_sub(initial_chunks);

    let indexed = !search_result.chunks.is_empty();
    let skip_reason = if !indexed {
      Some("File may be too large or skipped by indexer".to_string())
    } else {
      None
    };

    // Cleanup
    fixtures.cleanup().await?;

    Ok(LargeFileBenchResult {
      file_size_bytes: actual_size,
      indexed,
      chunks_created: if indexed { Some(chunks_added) } else { None },
      processing_time_ms: elapsed.as_millis() as u64,
      peak_memory_bytes: monitor.peak_memory(),
      skip_reason,
    })
  }

  /// Compute summary statistics.
  fn compute_summary(
    results: &[IncrementalBenchResult],
    large_file_results: &[LargeFileBenchResult],
    threshold_ms: u64,
  ) -> IncrementalSummary {
    if results.is_empty() {
      return IncrementalSummary::default();
    }

    let times: Vec<f64> = results.iter().map(|r| r.time_per_file_ms).collect();
    let avg_time = times.iter().sum::<f64>() / times.len() as f64;
    let max_time = times.iter().cloned().fold(0.0, f64::max);

    let total_tp: usize = results.iter().map(|r| r.true_positives).sum();
    let total_fn: usize = results.iter().map(|r| r.false_negatives).sum();
    let total_fp: usize = results.iter().map(|r| r.false_positives).sum();

    let detection_accuracy = if total_tp + total_fn > 0 {
      total_tp as f64 / (total_tp + total_fn) as f64
    } else {
      1.0
    };

    let false_positive_rate = if total_tp + total_fp > 0 {
      total_fp as f64 / (total_tp + total_fp) as f64
    } else {
      0.0
    };

    let max_indexed_file = large_file_results
      .iter()
      .filter(|r| r.indexed)
      .map(|r| r.file_size_bytes)
      .max()
      .unwrap_or(0);

    // Pass if: avg time < threshold and detection accuracy > 90%
    let passes = avg_time < threshold_ms as f64 && detection_accuracy >= 0.9;

    IncrementalSummary {
      avg_time_per_file_ms: avg_time,
      max_time_per_file_ms: max_time,
      detection_accuracy,
      false_positive_rate,
      max_indexed_file_bytes: max_indexed_file,
      passes,
    }
  }
}

impl IncrementalReport {
  /// Generate markdown report.
  pub fn to_markdown(&self) -> String {
    let mut out = String::new();

    out.push_str("# Incremental Indexing Performance Report\n\n");
    out.push_str(&format!("**Timestamp:** {}\n", self.timestamp));
    out.push_str(&format!("**Version:** {}\n\n", self.version));

    // Summary
    out.push_str("## Summary\n\n");
    let status = if self.summary.passes { "PASS" } else { "FAIL" };
    out.push_str(&format!("**Status:** {}\n\n", status));
    out.push_str(&format!(
      "- Average time per file: {:.1} ms\n",
      self.summary.avg_time_per_file_ms
    ));
    out.push_str(&format!(
      "- Max time per file: {:.1} ms\n",
      self.summary.max_time_per_file_ms
    ));
    out.push_str(&format!(
      "- Detection accuracy: {:.1}%\n",
      self.summary.detection_accuracy * 100.0
    ));
    out.push_str(&format!(
      "- False positive rate: {:.1}%\n",
      self.summary.false_positive_rate * 100.0
    ));
    out.push_str(&format!(
      "- Largest indexed file: {:.1} MB\n\n",
      self.summary.max_indexed_file_bytes as f64 / (1024.0 * 1024.0)
    ));

    // Incremental results
    out.push_str("## Incremental Results\n\n");
    out.push_str("| Repo | Files | Time/File | TP | FP | FN | Total Time |\n");
    out.push_str("|------|-------|-----------|----|----|----|-----------|\n");

    for result in &self.results {
      out.push_str(&format!(
        "| {} | {} | {:.1} ms | {} | {} | {} | {} ms |\n",
        result.repo,
        result.files_modified,
        result.time_per_file_ms,
        result.true_positives,
        result.false_positives,
        result.false_negatives,
        result.total_time_ms,
      ));
    }

    // Large file results
    out.push_str("\n## Large File Results\n\n");
    out.push_str("| Size | Indexed | Chunks | Time | Memory |\n");
    out.push_str("|------|---------|--------|------|--------|\n");

    for result in &self.large_file_results {
      let size_mb = result.file_size_bytes as f64 / (1024.0 * 1024.0);
      let chunks = result
        .chunks_created
        .map(|c| c.to_string())
        .unwrap_or_else(|| "-".to_string());
      let indexed = if result.indexed { "Yes" } else { "No" };
      let memory_mb = result.peak_memory_bytes as f64 / (1024.0 * 1024.0);

      out.push_str(&format!(
        "| {:.1} MB | {} | {} | {} ms | {:.1} MB |\n",
        size_mb, indexed, chunks, result.processing_time_ms, memory_mb,
      ));
    }

    out
  }

  /// Save report to files (JSON and Markdown).
  pub async fn save(&self, output_dir: &PathBuf) -> Result<()> {
    tokio::fs::create_dir_all(output_dir).await?;

    // Save JSON
    let json_path = output_dir.join("incremental.json");
    let json = serde_json::to_string_pretty(self)?;
    tokio::fs::write(&json_path, json).await?;
    info!("Saved JSON report: {}", json_path.display());

    // Save Markdown
    let md_path = output_dir.join("incremental.md");
    tokio::fs::write(&md_path, self.to_markdown()).await?;
    info!("Saved Markdown report: {}", md_path.display());

    Ok(())
  }
}

/// Compare two indexing reports for regressions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingComparison {
  /// Baseline report timestamp
  pub baseline_timestamp: String,
  /// Current report timestamp
  pub current_timestamp: String,
  /// Per-repo comparisons
  pub comparisons: Vec<RepoComparison>,
  /// Overall pass/fail
  pub passes: bool,
}

/// Comparison for a single repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoComparison {
  /// Repository name
  pub repo: String,
  /// Wall time change percentage (positive = slower)
  pub wall_time_change_pct: f64,
  /// Chunks/sec change percentage (positive = faster)
  pub throughput_change_pct: f64,
  /// Memory change percentage (positive = more memory)
  pub memory_change_pct: f64,
  /// Whether this repo passes threshold
  pub passes: bool,
}

impl IndexingComparison {
  /// Compare two reports with a regression threshold.
  pub fn compare(baseline: &IndexingReport, current: &IndexingReport, threshold_pct: f64) -> Self {
    let mut comparisons = Vec::new();
    let mut all_pass = true;

    for current_summary in &current.summaries {
      if let Some(baseline_summary) = baseline.summaries.iter().find(|s| s.repo == current_summary.repo) {
        let wall_time_change_pct = if baseline_summary.avg_wall_time_ms > 0 {
          ((current_summary.avg_wall_time_ms as f64 - baseline_summary.avg_wall_time_ms as f64)
            / baseline_summary.avg_wall_time_ms as f64)
            * 100.0
        } else {
          0.0
        };

        let throughput_change_pct = if baseline_summary.avg_chunks_per_sec > 0.0 {
          ((current_summary.avg_chunks_per_sec - baseline_summary.avg_chunks_per_sec)
            / baseline_summary.avg_chunks_per_sec)
            * 100.0
        } else {
          0.0
        };

        let memory_change_pct = if baseline_summary.peak_memory_bytes > 0 {
          ((current_summary.peak_memory_bytes as f64 - baseline_summary.peak_memory_bytes as f64)
            / baseline_summary.peak_memory_bytes as f64)
            * 100.0
        } else {
          0.0
        };

        // Regression if wall time increased or throughput decreased beyond threshold
        let passes = wall_time_change_pct <= threshold_pct && throughput_change_pct >= -threshold_pct;

        if !passes {
          all_pass = false;
        }

        comparisons.push(RepoComparison {
          repo: current_summary.repo.clone(),
          wall_time_change_pct,
          throughput_change_pct,
          memory_change_pct,
          passes,
        });
      }
    }

    Self {
      baseline_timestamp: baseline.timestamp.clone(),
      current_timestamp: current.timestamp.clone(),
      comparisons,
      passes: all_pass,
    }
  }

  /// Generate markdown comparison report.
  pub fn to_markdown(&self) -> String {
    let mut out = String::new();

    out.push_str("# Indexing Performance Comparison\n\n");
    out.push_str(&format!("**Baseline:** {}\n", self.baseline_timestamp));
    out.push_str(&format!("**Current:** {}\n\n", self.current_timestamp));

    let status = if self.passes { "PASS" } else { "FAIL" };
    out.push_str(&format!("**Status:** {}\n\n", status));

    out.push_str("| Repository | Wall Time | Throughput | Memory | Status |\n");
    out.push_str("|------------|-----------|------------|--------|--------|\n");

    for comp in &self.comparisons {
      let wall_icon = if comp.wall_time_change_pct > 10.0 {
        "🔴"
      } else if comp.wall_time_change_pct < -10.0 {
        "🟢"
      } else {
        "⚪"
      };
      let throughput_icon = if comp.throughput_change_pct < -10.0 {
        "🔴"
      } else if comp.throughput_change_pct > 10.0 {
        "🟢"
      } else {
        "⚪"
      };
      let status_icon = if comp.passes { "✅" } else { "❌" };

      out.push_str(&format!(
        "| {} | {} {:+.1}% | {} {:+.1}% | {:+.1}% | {} |\n",
        comp.repo,
        wall_icon,
        comp.wall_time_change_pct,
        throughput_icon,
        comp.throughput_change_pct,
        comp.memory_change_pct,
        status_icon,
      ));
    }

    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_indexing_summary_computation() {
    let results = vec![
      IndexingBenchResult {
        repo: "test".to_string(),
        iteration: 0,
        cold_start: true,
        metrics: IndexingMetrics {
          wall_time_ms: 1000,
          peak_memory_bytes: 100_000_000,
          avg_cpu_percent: 50.0,
          chunks_processed: 1000,
          chunks_per_sec: 1000.0,
          embeddings_generated: 1000,
          embeddings_per_sec: 1000.0,
        },
        files_scanned: 100,
        files_indexed: 100,
        bytes_processed: 1_000_000,
      },
      IndexingBenchResult {
        repo: "test".to_string(),
        iteration: 1,
        cold_start: false,
        metrics: IndexingMetrics {
          wall_time_ms: 800,
          peak_memory_bytes: 90_000_000,
          avg_cpu_percent: 45.0,
          chunks_processed: 1000,
          chunks_per_sec: 1250.0,
          embeddings_generated: 1000,
          embeddings_per_sec: 1250.0,
        },
        files_scanned: 100,
        files_indexed: 100,
        bytes_processed: 1_000_000,
      },
    ];

    let report = IndexingReport::from_results(results);
    assert_eq!(report.summaries.len(), 1);

    let summary = &report.summaries[0];
    assert_eq!(summary.repo, "test");
    assert_eq!(summary.iterations, 2);
    assert_eq!(summary.avg_wall_time_ms, 900);
  }

  #[test]
  fn test_comparison() {
    let baseline = IndexingReport {
      timestamp: "2024-01-01".to_string(),
      version: "0.1.0".to_string(),
      results: vec![],
      summaries: vec![IndexingSummary {
        repo: "test".to_string(),
        iterations: 1,
        avg_wall_time_ms: 1000,
        p50_wall_time_ms: 1000,
        p95_wall_time_ms: 1000,
        avg_chunks_per_sec: 1000.0,
        avg_embeddings_per_sec: 1000.0,
        peak_memory_bytes: 100_000_000,
        avg_files_per_sec: 100.0,
      }],
    };

    let current = IndexingReport {
      timestamp: "2024-01-02".to_string(),
      version: "0.1.0".to_string(),
      results: vec![],
      summaries: vec![IndexingSummary {
        repo: "test".to_string(),
        iterations: 1,
        avg_wall_time_ms: 1100, // 10% slower
        p50_wall_time_ms: 1100,
        p95_wall_time_ms: 1100,
        avg_chunks_per_sec: 900.0, // 10% slower
        avg_embeddings_per_sec: 900.0,
        peak_memory_bytes: 110_000_000,
        avg_files_per_sec: 90.0,
      }],
    };

    let comparison = IndexingComparison::compare(&baseline, &current, 10.0);
    assert_eq!(comparison.comparisons.len(), 1);
    assert!(comparison.passes); // Within 10% threshold
  }
}
