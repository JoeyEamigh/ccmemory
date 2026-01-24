//! CCEngram Benchmark CLI
//!
//! Run benchmarks against the explore/context tools using real codebases.

use anyhow::Result;
use benchmark::{
  indexing::IndexingBenchmark,
  reports::{ComparisonReport, generate_reports},
  repos::{RepoCache, RepoRegistry, TargetRepo, default_cache_dir, prepare_repo},
  scenarios::{Scenario, ScenarioRunner, filter_scenarios, load_scenarios_from_dir, run_scenarios_parallel},
};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tracing::{Level, info, warn};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "ccengram-bench")]
#[command(about = "Benchmark harness for CCEngram explore/context tools")]
#[command(version)]
struct Cli {
  /// Enable verbose logging
  #[arg(short, long, global = true)]
  verbose: bool,

  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Run benchmark scenarios
  Run {
    /// Output directory for results
    #[arg(short, long, default_value = "./benchmark-results")]
    output: PathBuf,

    /// Filter scenarios by pattern (supports glob wildcards)
    #[arg(short, long)]
    scenarios: Option<String>,

    /// Enable LLM-as-judge evaluation (expensive)
    #[arg(long)]
    llm_judge: bool,

    /// Scenarios directory
    #[arg(long)]
    scenarios_dir: Option<PathBuf>,

    /// Run scenarios in parallel
    #[arg(long)]
    parallel: bool,

    /// Name for this benchmark run
    #[arg(long)]
    name: Option<String>,
  },

  /// Compare two benchmark results for regressions
  Compare {
    /// Baseline results file (JSON)
    baseline: PathBuf,

    /// Current results file (JSON)
    current: PathBuf,

    /// Regression threshold percentage
    #[arg(short, long, default_value = "10")]
    threshold: f64,

    /// Output comparison report
    #[arg(short, long)]
    output: Option<PathBuf>,
  },

  /// Download repositories
  Index {
    /// Repositories to download (comma-separated: zed,vscode or 'all')
    #[arg(short, long, default_value = "all")]
    repos: String,

    /// Force re-download
    #[arg(long)]
    force: bool,

    /// Cache directory
    #[arg(long)]
    cache_dir: Option<PathBuf>,
  },

  /// Index repositories via daemon (with streaming progress)
  IndexCode {
    /// Repositories to index (comma-separated: zed,vscode or 'all')
    #[arg(short, long, default_value = "all")]
    repos: String,

    /// Force re-index even if already indexed
    #[arg(long)]
    force: bool,

    /// Cache directory for repositories
    #[arg(long)]
    cache_dir: Option<PathBuf>,
  },

  /// List available scenarios
  List {
    /// Scenarios directory
    #[arg(long)]
    scenarios_dir: Option<PathBuf>,

    /// Show detailed information
    #[arg(short, long)]
    detailed: bool,
  },

  /// Clean cached data
  Clean {
    /// Clean all cached data
    #[arg(long)]
    all: bool,

    /// Specific repository to clean
    #[arg(long)]
    repo: Option<String>,

    /// Cache directory
    #[arg(long)]
    cache_dir: Option<PathBuf>,
  },

  /// Benchmark indexing performance
  IndexPerf {
    /// Repositories to benchmark (comma-separated: zed,vscode or 'all')
    #[arg(short, long, default_value = "all")]
    repos: String,

    /// Number of iterations per repository
    #[arg(short, long, default_value = "3")]
    iterations: usize,

    /// Output directory for results
    #[arg(short, long, default_value = "./benchmark-results")]
    output: PathBuf,

    /// Force cold start (clear index before each iteration)
    #[arg(long)]
    cold: bool,

    /// Cache directory for repositories
    #[arg(long)]
    cache_dir: Option<PathBuf>,
  },
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();

  // Setup logging
  let level = if cli.verbose { Level::DEBUG } else { Level::INFO };
  let subscriber = FmtSubscriber::builder()
    .with_max_level(level)
    .with_target(false)
    .finish();
  tracing::subscriber::set_global_default(subscriber)?;

  match cli.command {
    Commands::Run {
      output,
      scenarios,
      llm_judge,
      scenarios_dir,
      parallel,
      name,
    } => run_benchmarks(output, scenarios, llm_judge, scenarios_dir, parallel, name).await,
    Commands::Compare {
      baseline,
      current,
      threshold,
      output,
    } => compare_results(baseline, current, threshold, output),
    Commands::Index {
      repos,
      force,
      cache_dir,
    } => index_repos(repos, force, cache_dir).await,
    Commands::IndexCode {
      repos,
      force,
      cache_dir,
    } => index_code_streaming(repos, force, cache_dir).await,
    Commands::List {
      scenarios_dir,
      detailed,
    } => list_scenarios(scenarios_dir, detailed),
    Commands::Clean { all, repo, cache_dir } => clean_cache(all, repo, cache_dir),
    Commands::IndexPerf {
      repos,
      iterations,
      output,
      cold,
      cache_dir,
    } => run_indexing_benchmark(repos, iterations, output, cold, cache_dir).await,
  }
}

async fn run_benchmarks(
  output: PathBuf,
  scenario_filter: Option<String>,
  llm_judge: bool,
  scenarios_dir: Option<PathBuf>,
  parallel: bool,
  run_name: Option<String>,
) -> Result<()> {
  use std::collections::HashMap;

  // Load scenarios
  let scenarios_dir = scenarios_dir.unwrap_or_else(|| PathBuf::from("crates/benchmark/scenarios"));
  info!("Loading scenarios from: {}", scenarios_dir.display());

  let all_scenarios = load_scenarios_from_dir(&scenarios_dir)?;
  if all_scenarios.is_empty() {
    warn!("No scenarios found in {}", scenarios_dir.display());
    return Ok(());
  }

  // Filter scenarios
  let scenarios: Vec<Scenario> = if let Some(pattern) = &scenario_filter {
    filter_scenarios(&all_scenarios, pattern).into_iter().cloned().collect()
  } else {
    all_scenarios.clone()
  };

  if scenarios.is_empty() {
    warn!(
      "No scenarios match filter: {}",
      scenario_filter.as_deref().unwrap_or("*")
    );
    return Ok(());
  }

  info!("Running {} scenarios", scenarios.len());

  let socket_path = ScenarioRunner::default_socket_path();
  let annotations_dir = scenarios_dir.parent().map(|p| p.join("annotations"));

  // Group scenarios by repo
  let mut scenarios_by_repo: HashMap<TargetRepo, Vec<&Scenario>> = HashMap::new();
  for scenario in &scenarios {
    scenarios_by_repo
      .entry(scenario.metadata.repo)
      .or_default()
      .push(scenario);
  }

  // Prepare repos - verify they're downloaded and indexed
  let mut repo_paths: HashMap<TargetRepo, PathBuf> = HashMap::new();
  for repo in scenarios_by_repo.keys() {
    // Ensure repo is downloaded
    let repo_path = match prepare_repo(*repo, None).await {
      Ok(path) => path,
      Err(e) => {
        anyhow::bail!(
          "Repository {} not available. Run:\n  cargo run -p benchmark -- index --repos {}\nError: {}",
          repo,
          repo,
          e
        );
      }
    };

    // Check if repo is indexed (quick stats check)
    if let Err(e) = check_repo_indexed(&socket_path, &repo_path).await {
      anyhow::bail!(
        "Repository {} not indexed. Run:\n  cargo run -p benchmark -- index-code --repos {}\nError: {}",
        repo,
        repo,
        e
      );
    }

    repo_paths.insert(*repo, repo_path);
  }

  // Run scenarios grouped by repo
  let mut results = Vec::new();

  // Progress bar for sequential execution
  let pb = if !parallel {
    let pb = ProgressBar::new(scenarios.len() as u64);
    pb.set_style(
      ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("#>-"),
    );
    Some(pb)
  } else {
    None
  };

  for (repo, repo_scenarios) in &scenarios_by_repo {
    let repo_path = repo_paths.get(repo).unwrap();
    let project_path = repo_path.to_string_lossy().to_string();

    // Create runner for this repo
    let runner = ScenarioRunner::new(&socket_path, &project_path, annotations_dir.clone());
    let runner = if llm_judge { runner.with_llm_judge()? } else { runner };

    // Check daemon
    if !runner.check_daemon().await {
      anyhow::bail!(
        "CCEngram daemon is not running. Start it with: ccengram daemon\n\
               Socket: {}",
        socket_path
      );
    }

    if parallel {
      info!("Running {} scenarios for {} in parallel", repo_scenarios.len(), repo);
      // Clone scenarios for parallel execution
      let scenarios_owned: Vec<Scenario> = repo_scenarios.iter().map(|s| (*s).clone()).collect();
      let repo_results = run_scenarios_parallel(&runner, &scenarios_owned).await;
      results.extend(repo_results);
    } else {
      for scenario in repo_scenarios {
        if let Some(ref pb) = pb {
          pb.set_message(scenario.metadata.id.clone());
        }

        match runner.run(scenario).await {
          Ok(result) => {
            let status = if result.passed { "✓" } else { "✗" };
            info!("{} {} ({}ms)", status, scenario.metadata.id, result.total_duration_ms);
            results.push(result);
          }
          Err(e) => {
            warn!("Failed to run {}: {}", scenario.metadata.id, e);
          }
        }

        if let Some(ref pb) = pb {
          pb.inc(1);
        }
      }
    }
  }

  if let Some(pb) = pb {
    pb.finish_with_message("done");
  }

  // Generate reports
  info!("Generating reports in: {}", output.display());
  generate_reports(&results, &output, run_name.as_deref())?;

  // Print summary
  let passed = results.iter().filter(|r| r.passed).count();
  let failed = results.len() - passed;
  println!("\n{} passed, {} failed", passed, failed);

  if failed > 0 {
    std::process::exit(1);
  }

  Ok(())
}

/// Check if a repo is indexed by querying code stats.
async fn check_repo_indexed(socket_path: &str, repo_path: &std::path::Path) -> Result<()> {
  use ipc::{CodeStatsParams, CodeStatsResult, Method, Request, Response};
  use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
  use tokio::net::UnixStream;

  let mut stream = UnixStream::connect(socket_path).await?;

  let request = Request {
    id: Some(1),
    method: Method::CodeStats,
    params: CodeStatsParams {
      cwd: Some(repo_path.to_string_lossy().to_string()),
    },
  };

  let request_str = serde_json::to_string(&request)?;
  stream.write_all(request_str.as_bytes()).await?;
  stream.write_all(b"\n").await?;
  stream.flush().await?;

  let mut reader = BufReader::new(stream);
  let mut response_str = String::new();
  reader.read_line(&mut response_str).await?;

  let response: Response<CodeStatsResult> = serde_json::from_str(&response_str)?;

  if let Some(error) = &response.error {
    anyhow::bail!("Stats error: {}", error.message);
  }

  // Check if there are chunks
  if let Some(result) = &response.result {
    let chunks = result.total_chunks;
    if chunks == 0 {
      anyhow::bail!("No code indexed (0 chunks)");
    }
    info!(
      "  {} has {} chunks indexed",
      repo_path.file_name().unwrap_or_default().to_string_lossy(),
      chunks
    );
  }

  Ok(())
}

/// Index repositories with streaming progress display.
async fn index_code_streaming(repos: String, force: bool, cache_dir: Option<PathBuf>) -> Result<()> {
  use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
  use tokio::net::UnixStream;

  let targets: Vec<TargetRepo> = if repos == "all" {
    TargetRepo::all().to_vec()
  } else {
    repos
      .split(',')
      .filter_map(|s| TargetRepo::from_name(s.trim()))
      .collect()
  };

  if targets.is_empty() {
    anyhow::bail!("No valid repositories specified. Use: zed, vscode, or 'all'");
  }

  let socket_path = ScenarioRunner::default_socket_path();

  // Check daemon is running
  {
    if UnixStream::connect(&socket_path).await.is_err() {
      anyhow::bail!(
        "CCEngram daemon is not running. Start it with: ccengram daemon\nSocket: {}",
        socket_path
      );
    }
  }

  for repo in targets {
    // Ensure repo is downloaded first
    let repo_path = match prepare_repo(repo, cache_dir.clone()).await {
      Ok(path) => path,
      Err(e) => {
        warn!("Repository {} not downloaded: {}", repo, e);
        info!("Run: cargo run -p benchmark -- index --repos {}", repo);
        continue;
      }
    };

    info!("Indexing {} at {}", repo, repo_path.display());

    // Connect to daemon
    let mut stream = UnixStream::connect(&socket_path).await?;

    // Send streaming index request
    use ipc::{CodeIndexParams, CodeIndexResult, Method, Request, Response};

    let request: Request<CodeIndexParams> = Request {
      id: Some(1),
      method: Method::CodeIndex,
      params: CodeIndexParams {
        cwd: Some(repo_path.to_string_lossy().to_string()),
        force,
        stream: true,
      },
    };

    let request_str = serde_json::to_string(&request)?;
    stream.write_all(request_str.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    // Read streaming responses
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // Progress bar
    let pb = ProgressBar::new(100);
    pb.set_style(
      ProgressStyle::default_bar()
        .template("{spinner:.green} {msg:40} [{bar:30.cyan/blue}] {pos}%")
        .unwrap()
        .progress_chars("█▓░"),
    );

    loop {
      line.clear();
      match reader.read_line(&mut line).await {
        Ok(0) => break, // EOF
        Ok(_) => {
          // For streaming, we parse as generic Response since we get both progress and final result
          let response: Response<CodeIndexResult> = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(_) => continue,
          };

          // Check for progress update
          if let Some(progress) = &response.progress {
            let phase = progress.phase.as_str();
            let message = progress.message.as_deref().unwrap_or("");

            match phase {
              "scanning" => {
                let scanned = progress.processed_files.unwrap_or(0) as u64;
                pb.set_message(format!("Scanning: {} files", scanned));
                pb.set_position(0);
              }
              "indexing" => {
                let processed = progress.processed_files.unwrap_or(0) as u64;
                let total = progress.total_files.unwrap_or(1) as u64;
                let percent = if total > 0 { (processed * 100) / total } else { 0 };
                pb.set_position(percent);
                pb.set_message(message.to_string());
              }
              "complete" => {
                pb.set_position(100);
                pb.finish_with_message(message.to_string());
              }
              _ => {}
            }
          }

          // Check for final result
          if let Some(result) = &response.result {
            let files = result.files_processed;
            let chunks = result.chunks_created;
            println!("  Indexed {} files, {} chunks", files, chunks);
            break;
          }

          // Check for error
          if let Some(error) = &response.error {
            pb.finish_with_message("Error");
            anyhow::bail!("Index error: {}", error.message);
          }
        }
        Err(e) => {
          pb.finish_with_message("Error");
          anyhow::bail!("Read error: {}", e);
        }
      }
    }
  }

  Ok(())
}

fn compare_results(baseline: PathBuf, current: PathBuf, threshold: f64, output: Option<PathBuf>) -> Result<()> {
  info!(
    "Comparing {} vs {} (threshold: {:.0}%)",
    baseline.display(),
    current.display(),
    threshold
  );

  let comparison = ComparisonReport::from_files(&baseline, &current, threshold)?;

  // Print markdown summary
  println!("{}", comparison.to_markdown());

  // Save if output specified
  if let Some(output) = output {
    comparison.save(&output)?;
    info!("Comparison saved to: {}", output.display());
  }

  if !comparison.summary.passes {
    std::process::exit(1);
  }

  Ok(())
}

async fn index_repos(repos: String, force: bool, cache_dir: Option<PathBuf>) -> Result<()> {
  let cache_dir = cache_dir.unwrap_or_else(default_cache_dir);
  let cache = RepoCache::new(cache_dir.clone());

  let targets: Vec<TargetRepo> = if repos == "all" {
    TargetRepo::all().to_vec()
  } else {
    repos
      .split(',')
      .filter_map(|s| TargetRepo::from_name(s.trim()))
      .collect()
  };

  if targets.is_empty() {
    anyhow::bail!("No valid repositories specified. Use: zed, vscode, or 'all'");
  }

  for repo in targets {
    let config = RepoRegistry::get(repo);
    info!("Preparing {} ({})", repo, config.release_tag);

    if force {
      info!("Removing existing cache for {}", repo);
      cache.remove(repo)?;
    }

    match prepare_repo(repo, Some(cache_dir.clone())).await {
      Ok(path) => {
        info!("Repository ready at: {}", path.display());
      }
      Err(e) => {
        warn!("Failed to prepare {}: {}", repo, e);
      }
    }
  }

  Ok(())
}

fn list_scenarios(scenarios_dir: Option<PathBuf>, detailed: bool) -> Result<()> {
  let scenarios_dir = scenarios_dir.unwrap_or_else(|| PathBuf::from("crates/benchmark/scenarios"));

  let scenarios = load_scenarios_from_dir(&scenarios_dir)?;

  if scenarios.is_empty() {
    println!("No scenarios found in {}", scenarios_dir.display());
    return Ok(());
  }

  println!("Available scenarios ({}):\n", scenarios.len());

  for scenario in &scenarios {
    if detailed {
      println!("  {} - {}", scenario.metadata.id, scenario.metadata.name);
      println!("    Repo: {}", scenario.metadata.repo);
      println!("    Difficulty: {:?}", scenario.metadata.difficulty);
      println!("    Steps: {}", scenario.steps.len());
      println!("    Expected files: {}", scenario.expected.must_find_files.len());
      println!("    Expected symbols: {}", scenario.expected.must_find_symbols.len());
      println!();
    } else {
      println!(
        "  {:30} {:10} {:10} ({} steps)",
        scenario.metadata.id,
        format!("{}", scenario.metadata.repo),
        format!("{:?}", scenario.metadata.difficulty).to_lowercase(),
        scenario.steps.len()
      );
    }
  }

  Ok(())
}

async fn run_indexing_benchmark(
  repos: String,
  iterations: usize,
  output: PathBuf,
  cold: bool,
  cache_dir: Option<PathBuf>,
) -> Result<()> {
  let targets: Vec<TargetRepo> = if repos == "all" {
    TargetRepo::all().to_vec()
  } else {
    repos
      .split(',')
      .filter_map(|s| TargetRepo::from_name(s.trim()))
      .collect()
  };

  if targets.is_empty() {
    anyhow::bail!("No valid repositories specified. Use: zed, vscode, or 'all'");
  }

  info!(
    "Running indexing benchmark: {} repos, {} iterations, cold={}",
    targets.len(),
    iterations,
    cold
  );

  // Create benchmark runner
  let socket_path = IndexingBenchmark::default_socket_path();
  let benchmark = IndexingBenchmark::new(&socket_path, cache_dir);

  // Check daemon
  if !benchmark.check_daemon().await {
    anyhow::bail!(
      "CCEngram daemon is not running. Start it with: ccengram daemon\n\
             Socket: {}",
      socket_path
    );
  }

  // Run benchmark
  let report = benchmark.run(&targets, iterations, cold).await?;

  // Save reports
  report.save(&output)?;

  // Print summary
  println!("\n{}", report.to_markdown());

  Ok(())
}

fn clean_cache(all: bool, repo: Option<String>, cache_dir: Option<PathBuf>) -> Result<()> {
  let cache_dir = cache_dir.unwrap_or_else(default_cache_dir);
  let cache = RepoCache::new(cache_dir);

  if all {
    info!("Cleaning all cached data");
    cache.clean_all()?;
    println!("Cache cleaned");
  } else if let Some(repo_name) = repo {
    if let Some(target) = TargetRepo::from_name(&repo_name) {
      info!("Cleaning cache for {}", target);
      cache.remove(target)?;
      println!("Cleaned {}", target);
    } else {
      anyhow::bail!("Unknown repository: {}", repo_name);
    }
  } else {
    println!("Specify --all or --repo <name>");
  }

  Ok(())
}
