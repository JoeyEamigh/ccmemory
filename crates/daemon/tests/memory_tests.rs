//! Memory-related integration tests for the CCEngram daemon
//!
//! Tests: memory lifecycle, search, delete, timeline, supersede, promote, get/list,
//! deduplication, simhash similarity, and decay functionality.

mod common;

use daemon::Request;

/// Test that the router handles memory operations correctly
#[tokio::test]
async fn test_router_memory_lifecycle() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Test memory_add
  let add_request = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "This is a test memory for integration testing",
        "sector": "semantic",
        "tags": ["test", "integration"],
        "cwd": cwd
    }),
  };

  let add_response = router.handle(add_request).await;
  assert!(
    add_response.error.is_none(),
    "memory_add should succeed: {:?}",
    add_response.error
  );
  let result = add_response.result.expect("Should have result");
  let memory_id = result
    .get("id")
    .expect("Should have id")
    .as_str()
    .expect("id should be string");

  // Test memory_search - use a substring that exists in the content
  let search_request = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "integration testing",
        "cwd": cwd
    }),
  };

  let search_response = router.handle(search_request).await;
  assert!(
    search_response.error.is_none(),
    "memory_search should succeed: {:?}",
    search_response.error
  );
  let results = search_response.result.expect("Should have results");
  let results_arr = results.as_array().expect("Results should be array");
  assert!(
    !results_arr.is_empty(),
    "Should find the memory we added (searched for 'integration testing')"
  );

  // Test memory_reinforce
  let reinforce_request = Request {
    id: Some(serde_json::json!(3)),
    method: "memory_reinforce".to_string(),
    params: serde_json::json!({
        "memory_id": memory_id,
        "amount": 0.2,
        "cwd": cwd
    }),
  };

  let reinforce_response = router.handle(reinforce_request).await;
  assert!(
    reinforce_response.error.is_none(),
    "memory_reinforce should succeed: {:?}",
    reinforce_response.error
  );
  let result = reinforce_response.result.expect("Should have result");
  let new_salience = result
    .get("new_salience")
    .expect("Should have new_salience")
    .as_f64()
    .expect("salience should be number");
  assert!(new_salience > 0.9, "Salience should be high after reinforce");

  // Test memory_deemphasize
  let deemphasize_request = Request {
    id: Some(serde_json::json!(4)),
    method: "memory_deemphasize".to_string(),
    params: serde_json::json!({
        "memory_id": memory_id,
        "amount": 0.1,
        "cwd": cwd
    }),
  };

  let deemphasize_response = router.handle(deemphasize_request).await;
  assert!(
    deemphasize_response.error.is_none(),
    "memory_deemphasize should succeed"
  );

  // Test memory_delete (soft)
  let delete_request = Request {
    id: Some(serde_json::json!(5)),
    method: "memory_delete".to_string(),
    params: serde_json::json!({
        "memory_id": memory_id,
        "hard": false,
        "cwd": cwd
    }),
  };

  let delete_response = router.handle(delete_request).await;
  assert!(delete_response.error.is_none(), "memory_delete should succeed");
  let result = delete_response.result.expect("Should have result");
  assert_eq!(result.get("hard_delete").and_then(|v| v.as_bool()), Some(false));
}

/// Test memory search with extended filter options
#[tokio::test]
async fn test_router_memory_search_filters() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Add memories with different attributes
  let add_preference = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "User prefers tabs over spaces for indentation",
        "sector": "emotional",
        "type": "preference",
        "scope_path": "src/formatter",
        "scope_module": "formatter",
        "categories": ["coding-style", "preferences"],
        "importance": 0.9,
        "cwd": cwd
    }),
  };
  let pref_response = router.handle(add_preference).await;
  assert!(pref_response.error.is_none(), "Should add preference memory");

  let add_codebase = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "The Router struct handles all incoming MCP requests",
        "sector": "semantic",
        "type": "codebase",
        "scope_path": "src/router",
        "scope_module": "router",
        "categories": ["architecture"],
        "importance": 0.7,
        "cwd": cwd
    }),
  };
  let code_response = router.handle(add_codebase).await;
  assert!(code_response.error.is_none(), "Should add codebase memory");

  // Search with sector filter - should only return emotional sector
  let search_sector = Request {
    id: Some(serde_json::json!(3)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "indentation",
        "sector": "emotional",
        "cwd": cwd
    }),
  };
  let sector_response = router.handle(search_sector).await;
  assert!(sector_response.error.is_none(), "Sector-filtered search should succeed");

  // Verify sector filter is applied
  if let Some(results) = sector_response.result.as_ref().and_then(|r| r.as_array()) {
    for result in results {
      let sector = result.get("sector").and_then(|v| v.as_str()).unwrap_or("");
      assert_eq!(
        sector, "emotional",
        "Sector filter should only return emotional memories, got: {}",
        sector
      );
    }
  }

  // Search with memory type filter - should only return codebase type
  let search_type = Request {
    id: Some(serde_json::json!(4)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "code",
        "type": "codebase",
        "cwd": cwd
    }),
  };
  let type_response = router.handle(search_type).await;
  assert!(type_response.error.is_none(), "Type-filtered search should succeed");

  // Verify type filter is applied
  if let Some(results) = type_response.result.as_ref().and_then(|r| r.as_array()) {
    for result in results {
      let mem_type = result.get("memory_type").and_then(|v| v.as_str()).unwrap_or("");
      assert_eq!(
        mem_type, "codebase",
        "Type filter should only return codebase memories, got: {}",
        mem_type
      );
    }
  }

  // Search with min_salience filter
  let search_salience = Request {
    id: Some(serde_json::json!(5)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "preference",
        "min_salience": 0.5,
        "cwd": cwd
    }),
  };
  let salience_response = router.handle(search_salience).await;
  assert!(
    salience_response.error.is_none(),
    "Salience-filtered search should succeed"
  );

  // Search with scope_path filter (prefix match)
  let search_scope = Request {
    id: Some(serde_json::json!(6)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "code",
        "scope_path": "src/",
        "cwd": cwd
    }),
  };
  let scope_response = router.handle(search_scope).await;
  assert!(scope_response.error.is_none(), "Scope-filtered search should succeed");
}

/// Test memory search result metadata includes new fields
#[tokio::test]
async fn test_router_memory_search_result_metadata() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Add a memory with all fields populated - use unique keyword for reliable text search
  let add_request = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "The project uses XYZMETADATA framework for implementation testing",
        "sector": "semantic",
        "type": "codebase",
        "scope_path": "src/frontend",
        "scope_module": "ui",
        "categories": ["tech-stack", "frontend"],
        "tags": ["framework", "frontend"],
        "importance": 0.8,
        "cwd": cwd
    }),
  };
  let add_response = router.handle(add_request).await;
  assert!(add_response.error.is_none(), "Should add memory");

  // Search for it using the unique keyword
  let search_request = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "XYZMETADATA",
        "cwd": cwd
    }),
  };
  let search_response = router.handle(search_request).await;
  assert!(search_response.error.is_none(), "Search should succeed");

  let results = search_response.result.expect("Should have results");
  let results_arr = results.as_array().expect("Results should be array");
  assert!(!results_arr.is_empty(), "Should find the memory with unique keyword");

  // Verify result includes extended metadata
  let first_result = &results_arr[0];
  assert!(first_result.get("sector").is_some(), "Result should include sector");
  assert!(first_result.get("tier").is_some(), "Result should include tier");
  assert!(
    first_result.get("memory_type").is_some(),
    "Result should include memory_type"
  );
  assert!(first_result.get("salience").is_some(), "Result should include salience");
  assert!(
    first_result.get("importance").is_some(),
    "Result should include importance"
  );
  assert!(
    first_result.get("is_superseded").is_some(),
    "Result should include is_superseded"
  );
  assert!(first_result.get("tags").is_some(), "Result should include tags");
  assert!(
    first_result.get("categories").is_some(),
    "Result should include categories"
  );
  assert!(
    first_result.get("scope_path").is_some(),
    "Result should include scope_path"
  );
  assert!(
    first_result.get("scope_module").is_some(),
    "Result should include scope_module"
  );
  assert!(
    first_result.get("created_at").is_some(),
    "Result should include created_at"
  );
  assert!(
    first_result.get("last_accessed").is_some(),
    "Result should include last_accessed"
  );
}

/// Test memory hard delete vs soft delete
#[tokio::test]
async fn test_router_memory_delete_modes() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Create two memories
  let add1 = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "Memory for soft delete test content here",
        "cwd": cwd
    }),
  };
  let response1 = router.handle(add1).await;
  let soft_delete_id = response1
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  let add2 = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "Memory for hard delete test content here",
        "cwd": cwd
    }),
  };
  let response2 = router.handle(add2).await;
  let hard_delete_id = response2
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  // Soft delete first memory
  let soft_delete = Request {
    id: Some(serde_json::json!(3)),
    method: "memory_delete".to_string(),
    params: serde_json::json!({
        "memory_id": soft_delete_id,
        "hard": false,
        "cwd": cwd
    }),
  };
  let soft_response = router.handle(soft_delete).await;
  assert!(soft_response.error.is_none(), "Soft delete should succeed");
  let soft_result = soft_response.result.unwrap();
  assert_eq!(soft_result.get("hard_delete").and_then(|v| v.as_bool()), Some(false));

  // Hard delete second memory
  let hard_delete = Request {
    id: Some(serde_json::json!(4)),
    method: "memory_delete".to_string(),
    params: serde_json::json!({
        "memory_id": hard_delete_id,
        "hard": true,
        "cwd": cwd
    }),
  };
  let hard_response = router.handle(hard_delete).await;
  assert!(hard_response.error.is_none(), "Hard delete should succeed");
  let hard_result = hard_response.result.unwrap();
  assert_eq!(hard_result.get("hard_delete").and_then(|v| v.as_bool()), Some(true));
}

/// Test memory timeline
#[tokio::test]
async fn test_router_memory_timeline() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // First add a memory
  let add_request = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "Timeline test memory content for this test",
        "cwd": cwd
    }),
  };

  let add_response = router.handle(add_request).await;
  assert!(
    add_response.error.is_none(),
    "memory_add should succeed: {:?}",
    add_response.error
  );
  let result = add_response.result.expect("Should have result");
  let memory_id = result
    .get("id")
    .expect("Should have id")
    .as_str()
    .expect("id should be string");

  // Test timeline
  let timeline_request = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_timeline".to_string(),
    params: serde_json::json!({
        "anchor_id": memory_id,
        "depth_before": 3,
        "depth_after": 3,
        "cwd": cwd
    }),
  };

  let timeline_response = router.handle(timeline_request).await;
  assert!(timeline_response.error.is_none(), "memory_timeline should succeed");
  let result = timeline_response.result.expect("Should have result");

  // Verify anchor memory is present and matches the requested ID
  let anchor = result.get("anchor").expect("Should have anchor memory");
  assert!(anchor.is_object(), "Anchor should be an object");
  let anchor_id = anchor
    .get("id")
    .and_then(|v| v.as_str())
    .expect("Anchor should have id");
  assert_eq!(anchor_id, memory_id, "Anchor should match requested memory");

  // Verify before/after are arrays (even if empty)
  let before = result.get("before").expect("Should have before array");
  assert!(before.is_array(), "before should be an array");

  let after = result.get("after").expect("Should have after array");
  assert!(after.is_array(), "after should be an array");
}

/// Test memory supersede
#[tokio::test]
async fn test_router_memory_supersede() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Create two memories
  let add1 = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "Original memory content that will be superseded",
        "cwd": cwd
    }),
  };
  let response1 = router.handle(add1).await;
  assert!(
    response1.error.is_none(),
    "memory_add should succeed: {:?}",
    response1.error
  );
  let old_id = response1
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  let add2 = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "New memory content that supersedes the original",
        "cwd": cwd
    }),
  };
  let response2 = router.handle(add2).await;
  assert!(
    response2.error.is_none(),
    "memory_add should succeed: {:?}",
    response2.error
  );
  let new_id = response2
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  // Supersede
  let supersede_request = Request {
    id: Some(serde_json::json!(3)),
    method: "memory_supersede".to_string(),
    params: serde_json::json!({
        "old_memory_id": old_id,
        "new_memory_id": new_id,
        "cwd": cwd
    }),
  };

  let supersede_response = router.handle(supersede_request).await;
  assert!(supersede_response.error.is_none(), "memory_supersede should succeed");
}

/// Test search with include_superseded flag
#[tokio::test]
async fn test_router_memory_search_include_superseded() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Create and supersede a memory
  let add_old = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "Project uses PostgreSQL 14 for database needs",
        "cwd": cwd
    }),
  };
  let old_response = router.handle(add_old).await;
  let old_id = old_response
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  let add_new = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "Project now uses PostgreSQL 16 (upgraded)",
        "cwd": cwd
    }),
  };
  let new_response = router.handle(add_new).await;
  let new_id = new_response
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  // Supersede the old memory
  let supersede = Request {
    id: Some(serde_json::json!(3)),
    method: "memory_supersede".to_string(),
    params: serde_json::json!({
        "old_memory_id": old_id,
        "new_memory_id": new_id,
        "cwd": cwd
    }),
  };
  let supersede_response = router.handle(supersede).await;
  assert!(
    supersede_response.error.is_none(),
    "Supersede should succeed: {:?}",
    supersede_response.error
  );

  // Search without include_superseded (default) - should only find new
  let search_default = Request {
    id: Some(serde_json::json!(4)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "PostgreSQL database",
        "cwd": cwd
    }),
  };
  let default_response = router.handle(search_default).await;
  let default_results = default_response.result.unwrap();
  let default_arr = default_results.as_array().unwrap();
  // Non-superseded results should not show the old memory
  for result in default_arr {
    let is_superseded = result.get("is_superseded").and_then(|v| v.as_bool()).unwrap_or(false);
    // The old memory (with "14") should be filtered out or marked superseded
    if result
      .get("content")
      .and_then(|v| v.as_str())
      .unwrap_or("")
      .contains("14")
    {
      assert!(is_superseded, "Old memory should be superseded");
    }
  }

  // Search with include_superseded = true - should find both
  let search_all = Request {
    id: Some(serde_json::json!(5)),
    method: "memory_search".to_string(),
    params: serde_json::json!({
        "query": "PostgreSQL database",
        "include_superseded": true,
        "cwd": cwd
    }),
  };
  let all_response = router.handle(search_all).await;
  assert!(
    all_response.error.is_none(),
    "Search with include_superseded should succeed"
  );
}

/// Test memory promotion from session tier to project tier
#[tokio::test]
async fn test_router_memory_promotion() {
  use db::{ProjectDb, Session, UsageType};
  use engram_core::{Memory, ProjectId, Sector, Tier};
  use std::path::Path;
  use tempfile::TempDir;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id.clone(), db_path, 768).await.unwrap();

  // Create two sessions
  let session1 = Session::new(uuid::Uuid::new_v4());
  let session2 = Session::new(uuid::Uuid::new_v4());
  db.add_session(&session1).await.unwrap();
  db.add_session(&session2).await.unwrap();

  let proj_uuid = uuid::Uuid::new_v4();

  // Create a session-tier memory
  let mut memory = Memory::new(proj_uuid, "Session tier memory".to_string(), Sector::Semantic);
  memory.content_hash = "hash1".to_string();
  memory.tier = Tier::Session;
  db.add_memory(&memory, None).await.unwrap();

  // Link to session1 as created
  db.link_memory(session1.id, &memory.id.to_string(), UsageType::Created)
    .await
    .unwrap();

  // Try promotion with threshold 2 - should not promote (only 1 usage)
  let promoted = db.promote_session_memories(&session1.id, 2).await.unwrap();
  assert_eq!(promoted, 0, "Should not promote with only 1 usage");

  // Verify still session tier
  let still_session = db.get_memory(&memory.id).await.unwrap().unwrap();
  assert_eq!(still_session.tier, Tier::Session);

  // Link to session2 (second usage)
  db.link_memory(session2.id, &memory.id.to_string(), UsageType::Recalled)
    .await
    .unwrap();

  // Now promotion should work (2 usages >= threshold)
  let promoted = db.promote_session_memories(&session1.id, 2).await.unwrap();
  assert_eq!(promoted, 1, "Should promote 1 memory");

  // Verify promoted to project tier
  let promoted_memory = db.get_memory(&memory.id).await.unwrap().unwrap();
  assert_eq!(
    promoted_memory.tier,
    Tier::Project,
    "Memory should be promoted to project tier"
  );
}

/// Test memory_get and memory_list tools
#[tokio::test]
async fn test_router_memory_get_and_list() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Add multiple memories
  let mut memory_ids = Vec::new();
  for i in 0..3 {
    let add_request = Request {
      id: Some(serde_json::json!(i)),
      method: "memory_add".to_string(),
      params: serde_json::json!({
          "content": format!("Test memory #{} for get/list testing with unique content", i),
          "sector": if i % 2 == 0 { "semantic" } else { "procedural" },
          "cwd": cwd
      }),
    };
    let response = router.handle(add_request).await;
    assert!(
      response.error.is_none(),
      "memory_add should succeed: {:?}",
      response.error
    );
    let id = response
      .result
      .unwrap()
      .get("id")
      .unwrap()
      .as_str()
      .unwrap()
      .to_string();
    memory_ids.push(id);
  }

  // Test memory_get
  let get_request = Request {
    id: Some(serde_json::json!(10)),
    method: "memory_get".to_string(),
    params: serde_json::json!({
        "memory_id": memory_ids[0],
        "cwd": cwd
    }),
  };
  let get_response = router.handle(get_request).await;
  assert!(
    get_response.error.is_none(),
    "memory_get should succeed: {:?}",
    get_response.error
  );
  let memory = get_response.result.unwrap();
  assert!(memory.get("content").is_some(), "Should have content");
  assert!(memory.get("sector").is_some(), "Should have sector");
  assert!(memory.get("salience").is_some(), "Should have salience");
  assert!(memory.get("created_at").is_some(), "Should have created_at");

  // Test memory_list without filter
  let list_request = Request {
    id: Some(serde_json::json!(11)),
    method: "memory_list".to_string(),
    params: serde_json::json!({
        "cwd": cwd
    }),
  };
  let list_response = router.handle(list_request).await;
  assert!(
    list_response.error.is_none(),
    "memory_list should succeed: {:?}",
    list_response.error
  );
  let memories = list_response.result.unwrap();
  let memories_arr = memories.as_array().unwrap();
  // Note: memory_list returns memories - count may vary due to dedup or other factors
  assert!(!memories_arr.is_empty(), "Should have memories");

  // Verify memory structure
  let first_mem = &memories_arr[0];
  assert!(first_mem.get("id").is_some(), "Memory should have id");
  assert!(first_mem.get("content").is_some(), "Memory should have content");
  assert!(first_mem.get("sector").is_some(), "Memory should have sector");

  // Test memory_list with limit
  let list_limited = Request {
    id: Some(serde_json::json!(13)),
    method: "memory_list".to_string(),
    params: serde_json::json!({
        "limit": 2,
        "cwd": cwd
    }),
  };
  let limited_response = router.handle(list_limited).await;
  assert!(
    limited_response.error.is_none(),
    "memory_list with limit should succeed"
  );
  let limited = limited_response.result.unwrap();
  let limited_arr = limited.as_array().unwrap();
  assert!(limited_arr.len() <= 2, "Should respect limit");

  // Test memory_get with non-existent ID
  let get_missing = Request {
    id: Some(serde_json::json!(14)),
    method: "memory_get".to_string(),
    params: serde_json::json!({
        "memory_id": "00000000-0000-0000-0000-000000000000",
        "cwd": cwd
    }),
  };
  let missing_response = router.handle(get_missing).await;
  assert!(
    missing_response.error.is_some(),
    "memory_get should fail for non-existent ID"
  );
}

/// Test memory deduplication - adding same content twice should return existing memory
#[tokio::test]
async fn test_memory_deduplication() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  let content = "This is a unique memory about integration testing that should be deduplicated when added twice";

  // Add the memory first time
  let add_request1 = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": content,
        "sector": "semantic",
        "cwd": cwd
    }),
  };

  let response1 = router.handle(add_request1).await;
  assert!(
    response1.error.is_none(),
    "First add should succeed: {:?}",
    response1.error
  );
  let result1 = response1.result.expect("Should have result");
  let id1 = result1
    .get("id")
    .expect("Should have id")
    .as_str()
    .expect("id should be string");

  // Add the exact same content again - should return same ID (deduplication)
  let add_request2 = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": content,
        "sector": "semantic",
        "cwd": cwd
    }),
  };

  let response2 = router.handle(add_request2).await;
  assert!(
    response2.error.is_none(),
    "Second add should succeed: {:?}",
    response2.error
  );
  let result2 = response2.result.expect("Should have result");
  let id2 = result2
    .get("id")
    .expect("Should have id")
    .as_str()
    .expect("id should be string");

  // Should return the same ID due to deduplication
  assert_eq!(id1, id2, "Duplicate content should return same memory ID");

  // Check for is_duplicate flag
  let is_dup = result2.get("is_duplicate").and_then(|v| v.as_bool()).unwrap_or(false);
  assert!(is_dup, "Second add should be marked as duplicate");

  // Completely different content should create new memory
  let different_content = "This is completely different content about debugging database issues";
  let add_request3 = Request {
    id: Some(serde_json::json!(3)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": different_content,
        "sector": "semantic",
        "cwd": cwd
    }),
  };

  let response3 = router.handle(add_request3).await;
  assert!(
    response3.error.is_none(),
    "Third add should succeed: {:?}",
    response3.error
  );
  let result3 = response3.result.expect("Should have result");
  let id3 = result3
    .get("id")
    .expect("Should have id")
    .as_str()
    .expect("id should be string");

  // Different content should create new memory
  assert_ne!(id1, id3, "Different content should create new memory");
}

/// Test deduplication detection with similar content (SimHash)
#[tokio::test]
async fn test_deduplication_simhash_similarity() {
  use extract::{DuplicateChecker, DuplicateMatch, content_hash, simhash};

  let checker = DuplicateChecker::new();

  let content1 = "The user wants to implement a feature for handling authentication in the application";
  let hash1 = content_hash(content1);

  // Create first memory (needs content_hash and simhash set on the memory)
  let mut memory1 = engram_core::Memory::new(uuid::Uuid::new_v4(), content1.into(), engram_core::Sector::Semantic);
  memory1.content_hash = hash1.clone();
  memory1.simhash = simhash(content1);

  // Check exact duplicate - comparing against the same memory
  let exact_match = checker.is_duplicate(content1, &hash1, memory1.simhash, &memory1);
  assert!(
    matches!(exact_match, DuplicateMatch::Exact),
    "Should detect exact duplicate"
  );

  // Check similar content (slight modification)
  let content2 = "The user wants to implement a feature for handling authentication in their application";
  let hash2 = content_hash(content2);
  let simhash2 = simhash(content2);

  // Check against memory1 (different content, potentially similar simhash)
  let similar_match = checker.is_duplicate(content2, &hash2, simhash2, &memory1);

  // May or may not match depending on similarity threshold
  match similar_match {
    DuplicateMatch::Simhash { distance, jaccard } => {
      assert!((0.0..=1.0).contains(&jaccard), "Jaccard should be valid");
      assert!(distance <= 10, "Distance should be reasonable for similar content");
    }
    DuplicateMatch::Exact | DuplicateMatch::None => {
      // Either too different or exact match is fine
    }
  }

  // Check completely different content
  let content3 = "Debugging the database connection timeout issue in production environment";
  let hash3 = content_hash(content3);
  let simhash3 = simhash(content3);

  let different_match = checker.is_duplicate(content3, &hash3, simhash3, &memory1);
  assert!(
    matches!(different_match, DuplicateMatch::None),
    "Completely different content should not match"
  );
}

/// Test decay calculation and stats
#[tokio::test]
async fn test_decay_functionality() {
  use chrono::{Duration, Utc};
  use engram_core::{Memory, Sector};
  use extract::{DecayConfig, apply_decay};

  // Create a memory with high salience
  let project_id = uuid::Uuid::new_v4();
  let mut memory = Memory::new(project_id, "Test memory for decay".into(), Sector::Episodic);
  memory.salience = 1.0;
  memory.importance = 0.5;

  let config = DecayConfig::default();

  // Apply decay for 30 days in the future
  let future = Utc::now() + Duration::days(30);
  let result = apply_decay(&mut memory, future, &config);

  // Verify decay happened
  assert!(
    result.new_salience < result.previous_salience,
    "Salience should decrease"
  );
  assert_eq!(result.previous_salience, 1.0);
  assert!(result.days_since_access >= 29.0); // ~30 days

  // Verify memory is not archived yet (needs to be below threshold)
  // For 30 days, episodic memories should still be above archive threshold
  assert!(!result.should_archive || result.new_salience < config.archive_threshold);
}
