//! Admin and meta command integration tests for the CCEngram daemon
//!
//! Tests: validation errors, meta commands, health check, project stats, retry config,
//! scheduler configuration, accumulator extraction triggers, database migrations,
//! session memory links, session stats extended.

mod common;

use daemon::Request;
use tempfile::TempDir;

/// Test validation errors
#[tokio::test]
async fn test_router_validation_errors() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Content too short
  let request = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "hi",
        "cwd": cwd
    }),
  };

  let response = router.handle(request).await;
  assert!(response.error.is_some(), "Should reject short content");
  let err = response.error.unwrap();
  assert!(
    err.message.contains("too short"),
    "Error should mention content too short"
  );

  // Missing required field for docs_ingest
  let request = Request {
    id: Some(serde_json::json!(2)),
    method: "docs_ingest".to_string(),
    params: serde_json::json!({
        "cwd": cwd
    }),
  };

  let response = router.handle(request).await;
  assert!(response.error.is_some(), "Should reject missing content/path/url");
}

/// Test ping/status commands
#[tokio::test]
async fn test_router_meta_commands() {
  let (_data_dir, _project_dir, router) = common::create_test_router();

  // Test ping
  let ping_request = Request {
    id: Some(serde_json::json!(1)),
    method: "ping".to_string(),
    params: serde_json::json!({}),
  };

  let ping_response = router.handle(ping_request).await;
  assert!(ping_response.error.is_none());
  assert_eq!(ping_response.result.unwrap(), serde_json::json!("pong"));

  // Test status
  let status_request = Request {
    id: Some(serde_json::json!(2)),
    method: "status".to_string(),
    params: serde_json::json!({}),
  };

  let status_response = router.handle(status_request).await;
  assert!(status_response.error.is_none());
  let result = status_response.result.expect("Should have result");
  assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("running"));
}

/// Test health check endpoint
#[tokio::test]
async fn test_router_health_check() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  let health_request = Request {
    id: Some(serde_json::json!(1)),
    method: "health_check".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let health_response = router.handle(health_request).await;
  assert!(health_response.error.is_none(), "Health check should succeed");

  let health = health_response.result.unwrap();

  // Verify database section exists and reports status
  let database = health.get("database").expect("Should have database section");
  assert!(database.get("status").is_some(), "Should have database status");

  // Verify ollama section exists
  let ollama = health.get("ollama").expect("Should have ollama section");
  assert!(ollama.get("available").is_some(), "Should report ollama availability");
  assert!(
    ollama.get("configured_model").is_some(),
    "Should report configured model"
  );

  // Verify embedding section exists
  let embedding = health.get("embedding").expect("Should have embedding section");
  assert!(
    embedding.get("configured").is_some(),
    "Should report embedding configuration"
  );
}

/// Test project statistics
#[tokio::test]
async fn test_router_project_stats() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Add some memories with diverse content to avoid deduplication
  let memory_contents = [
    "The authentication system uses JWT tokens with RS256 algorithm for secure session management",
    "Database connections are pooled using pg-pool with a maximum of 10 concurrent connections",
    "The frontend uses React 18 with concurrent rendering features for better performance",
    "API rate limiting is implemented using Redis with a sliding window algorithm",
    "Logging is handled by Winston with structured JSON output for ELK stack integration",
  ];

  for (i, content) in memory_contents.iter().enumerate() {
    let add = Request {
      id: Some(serde_json::json!(i)),
      method: "memory_add".to_string(),
      params: serde_json::json!({
          "content": content,
          "cwd": cwd
      }),
    };
    router.handle(add).await;
  }

  // Get project stats
  let stats_request = Request {
    id: Some(serde_json::json!(100)),
    method: "project_stats".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let stats_response = router.handle(stats_request).await;
  assert!(stats_response.error.is_none(), "Stats request should succeed");

  let stats = stats_response.result.unwrap();

  // Verify memory stats
  let memories = stats.get("memories").expect("Should have memories section");
  let total = memories.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
  assert!(total >= 1, "Should have at least 1 memory, got {}", total);

  // Verify by_sector exists and has values
  let by_sector = memories
    .get("by_sector")
    .and_then(|v| v.as_object())
    .expect("Should have by_sector");
  assert!(by_sector.contains_key("semantic") || by_sector.contains_key("episodic"));

  // Verify by_tier exists
  let by_tier = memories
    .get("by_tier")
    .and_then(|v| v.as_object())
    .expect("Should have by_tier");
  assert!(by_tier.contains_key("session") || by_tier.contains_key("project"));

  // Verify salience distribution exists
  let by_salience = memories.get("by_salience").expect("Should have by_salience");
  assert!(by_salience.get("high").is_some());
  assert!(by_salience.get("medium").is_some());
  assert!(by_salience.get("low").is_some());
  assert!(by_salience.get("very_low").is_some());

  // Verify code stats section exists
  let code = stats.get("code").expect("Should have code section");
  assert!(code.get("total_chunks").is_some());
  assert!(code.get("total_files").is_some());
  assert!(code.get("by_language").is_some());

  // Verify documents stats section exists
  let documents = stats.get("documents").expect("Should have documents section");
  assert!(documents.get("total").is_some());
  assert!(documents.get("total_chunks").is_some());

  // Verify entities stats section exists
  let entities = stats.get("entities").expect("Should have entities section");
  assert!(entities.get("total").is_some());
  assert!(entities.get("by_type").is_some());
}

/// Test resilient provider retry configuration
#[tokio::test]
async fn test_retry_config_presets() {
  use embedding::RetryConfig;
  use std::time::Duration;

  // Test default config
  let default = RetryConfig::default();
  assert_eq!(default.max_retries, 3);
  assert_eq!(default.initial_backoff, Duration::from_secs(1));
  assert!(default.add_jitter);

  // Test local config (faster)
  let local = RetryConfig::for_local();
  assert_eq!(local.max_retries, 2);
  assert!(local.initial_backoff < Duration::from_secs(1));
  assert!(local.request_timeout < Duration::from_secs(60));

  // Test cloud config (more resilient)
  let cloud = RetryConfig::for_cloud();
  assert_eq!(cloud.max_retries, 5);
  assert!(cloud.max_backoff > Duration::from_secs(30));
  assert!(cloud.request_timeout > Duration::from_secs(60));

  // Test backoff calculation
  let backoff_config = RetryConfig {
    max_retries: 5,
    initial_backoff: Duration::from_secs(1),
    max_backoff: Duration::from_secs(60),
    backoff_multiplier: 2.0,
    add_jitter: false,
    request_timeout: Duration::from_secs(30),
  };

  assert_eq!(backoff_config.backoff_for_attempt(0), Duration::from_secs(1));
  assert_eq!(backoff_config.backoff_for_attempt(1), Duration::from_secs(2));
  assert_eq!(backoff_config.backoff_for_attempt(2), Duration::from_secs(4));
}

/// Test scheduler configuration
#[tokio::test]
async fn test_scheduler_configuration() {
  use daemon::SchedulerConfig;

  // Test default config
  let config = SchedulerConfig::default();
  assert_eq!(config.decay_interval_hours, 60);
  assert_eq!(config.session_cleanup_hours, 6);
  assert_eq!(config.max_session_age_hours, 6);
  assert_eq!(config.checkpoint_interval_secs, 30);

  // Test custom config
  let custom = SchedulerConfig {
    decay_interval_hours: 24,
    session_cleanup_hours: 1,
    max_session_age_hours: 2,
    checkpoint_interval_secs: 10,
    decay_batch_size: 1000,
  };
  assert_eq!(custom.decay_interval_hours, 24);
  assert_eq!(custom.session_cleanup_hours, 1);
  assert_eq!(custom.decay_batch_size, 1000);
}

/// Test accumulator extraction triggers
#[tokio::test]
async fn test_accumulator_extraction_triggers() {
  use db::SegmentAccumulator;
  use uuid::Uuid;

  let session_id = Uuid::new_v4();
  let project_id = Uuid::new_v4();

  let mut acc = SegmentAccumulator::new(session_id, project_id);

  // Initially no meaningful work
  assert!(!acc.has_meaningful_work());

  // Add user prompts (with full signature)
  acc.add_user_prompt("first prompt", None, true);
  acc.add_user_prompt("second prompt", Some("question".to_string()), true);

  // Still no meaningful work (just prompts)
  assert!(!acc.has_meaningful_work());

  // Test meaningful work detection via tool calls
  let mut acc2 = SegmentAccumulator::new(session_id, project_id);
  acc2.add_file_modified("src/main.rs");
  assert!(acc2.has_meaningful_work()); // Modified files count

  let mut acc3 = SegmentAccumulator::new(session_id, project_id);
  acc3.increment_tool_calls();
  acc3.increment_tool_calls();
  assert!(!acc3.has_meaningful_work()); // Only 2 tool calls

  acc3.increment_tool_calls();
  assert!(acc3.has_meaningful_work()); // Now 3

  // Test todo completion trigger
  let mut acc4 = SegmentAccumulator::new(session_id, project_id);
  assert!(!acc4.should_trigger_todo_extraction());

  acc4.add_completed_task("Task 1");
  acc4.add_completed_task("Task 2");
  acc4.add_completed_task("Task 3");
  assert!(!acc4.should_trigger_todo_extraction()); // Need 5+ tool calls too

  // Add tool calls
  for _ in 0..5 {
    acc4.increment_tool_calls();
  }
  assert!(acc4.should_trigger_todo_extraction()); // Now has 3 tasks and 5+ tool calls

  // Test reset
  acc4.reset();
  assert!(!acc4.has_meaningful_work());
  assert!(!acc4.should_trigger_todo_extraction());
}

/// Test database migrations
#[tokio::test]
async fn test_database_migrations() {
  use db::{CURRENT_SCHEMA_VERSION, ProjectDb};
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = engram_core::ProjectId::from_path(Path::new("/test/migrations"));

  let db = ProjectDb::open_at_path(project_id, db_path, 768).await.unwrap();

  // New database should need migration
  let needs = db.needs_migration().await.unwrap();
  assert!(needs, "New database should need migration");

  // Run migrations
  let applied = db.run_migrations().await.unwrap();
  assert!(!applied.is_empty(), "Should apply at least one migration");

  // Check version matches target
  let version = db.get_current_version().await.unwrap();
  assert_eq!(version, CURRENT_SCHEMA_VERSION);

  // Should not need migration now
  let needs_after = db.needs_migration().await.unwrap();
  assert!(!needs_after, "Should not need migration after running");

  // Migration history should be populated
  let history = db.get_migration_history().await.unwrap();
  assert!(!history.is_empty(), "Should have migration history");
  assert_eq!(history[0].version, 1);
  assert_eq!(history[0].name, "initial_schema");

  // Running again should be idempotent
  let applied_again = db.run_migrations().await.unwrap();
  assert!(applied_again.is_empty(), "Second run should apply nothing");
}

/// Test session-memory linkage
#[tokio::test]
async fn test_session_memory_links() {
  use db::{ProjectDb, Session, UsageType};
  use engram_core::ProjectId;
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id.clone(), db_path, 768).await.unwrap();

  // Create and add a session
  let mut session = Session::new(uuid::Uuid::new_v4());
  session.user_prompt = Some("Test prompt".to_string());
  db.add_session(&session).await.unwrap();

  // Create memory IDs (as strings)
  let mem1 = uuid::Uuid::new_v4().to_string();
  let mem2 = uuid::Uuid::new_v4().to_string();

  // Link memories to session
  db.link_memory(session.id, &mem1, UsageType::Created).await.unwrap();
  db.link_memory(session.id, &mem2, UsageType::Recalled).await.unwrap();
  db.link_memory(session.id, &mem1, UsageType::Updated).await.unwrap(); // Same memory, different usage

  // Get session links
  let links = db.get_session_memory_links(&session.id).await.unwrap();
  assert_eq!(links.len(), 3, "Should have 3 links");

  // Get session stats
  let stats = db.get_session_stats(&session.id).await.unwrap();
  assert_eq!(stats.total_memories, 3);
  assert_eq!(stats.created, 1);
  assert_eq!(stats.recalled, 1);
  assert_eq!(stats.updated, 1);

  // Get memory usage count
  let mem1_count = db.get_memory_usage_count(&mem1).await.unwrap();
  assert_eq!(mem1_count, 2, "mem1 should have 2 usages");

  // Get memories for session
  let session_memories = db.get_memory_session_links(&mem1).await.unwrap();
  assert!(!session_memories.is_empty(), "Should find links from memory to session");
}

/// Test extended session statistics with sector breakdown
#[tokio::test]
async fn test_session_stats_extended() {
  use db::{ProjectDb, Session, UsageType};
  use engram_core::{Memory, ProjectId, Sector};
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id.clone(), db_path, 768).await.unwrap();

  // Create a session
  let session = Session::new(uuid::Uuid::new_v4());
  db.add_session(&session).await.unwrap();

  let proj_uuid = uuid::Uuid::new_v4();

  // Create memories with different sectors and saliences
  let mut m1 = Memory::new(proj_uuid, "Semantic memory content".to_string(), Sector::Semantic);
  m1.content_hash = "hash1".to_string();
  m1.salience = 0.8;

  let mut m2 = Memory::new(proj_uuid, "Emotional memory content".to_string(), Sector::Emotional);
  m2.content_hash = "hash2".to_string();
  m2.salience = 0.6;

  let mut m3 = Memory::new(proj_uuid, "Procedural memory content".to_string(), Sector::Procedural);
  m3.content_hash = "hash3".to_string();
  m3.salience = 0.4;

  // Add memories
  db.add_memory(&m1, None).await.unwrap();
  db.add_memory(&m2, None).await.unwrap();
  db.add_memory(&m3, None).await.unwrap();

  // Link to session
  db.link_memory(session.id, &m1.id.to_string(), UsageType::Created)
    .await
    .unwrap();
  db.link_memory(session.id, &m2.id.to_string(), UsageType::Created)
    .await
    .unwrap();
  db.link_memory(session.id, &m3.id.to_string(), UsageType::Recalled)
    .await
    .unwrap();

  // Get extended session stats
  let stats = db.get_session_stats(&session.id).await.unwrap();
  assert_eq!(stats.total_memories, 3);
  assert_eq!(stats.created, 2);
  assert_eq!(stats.recalled, 1);

  // Verify sector breakdown
  assert_eq!(*stats.by_sector.get("semantic").unwrap_or(&0), 1);
  assert_eq!(*stats.by_sector.get("emotional").unwrap_or(&0), 1);
  assert_eq!(*stats.by_sector.get("procedural").unwrap_or(&0), 1);

  // Verify average salience (0.8 + 0.6 + 0.4) / 3 = 0.6
  assert!(
    (stats.average_salience - 0.6).abs() < 0.01,
    "Average salience should be 0.6, got {}",
    stats.average_salience
  );
}
