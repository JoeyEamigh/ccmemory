//! Entity and relationship integration tests for the CCEngram daemon
//!
//! Tests: memory relationships, relationship type parsing, entity operations,
//! memory-entity links, router entity tools, router relationship tools.

mod common;

use daemon::Request;
use tempfile::TempDir;

/// Test memory relationships database operations
#[tokio::test]
async fn test_memory_relationships() {
  use db::ProjectDb;
  use engram_core::{MemoryId, RelationshipType};
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = engram_core::ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id, db_path, 768).await.unwrap();

  // Create memory IDs
  let from_id = MemoryId::new();
  let to_id1 = MemoryId::new();
  let to_id2 = MemoryId::new();

  // Create relationships
  let rel1 = db
    .create_relationship(&from_id, &to_id1, RelationshipType::BuildsOn, 0.9, "test")
    .await
    .unwrap();
  assert_eq!(rel1.relationship_type, RelationshipType::BuildsOn);
  assert_eq!(rel1.confidence, 0.9);

  let rel2 = db
    .create_relationship(&from_id, &to_id2, RelationshipType::Contradicts, 0.7, "test")
    .await
    .unwrap();
  assert_eq!(rel2.relationship_type, RelationshipType::Contradicts);

  // Get relationships from
  let rels_from = db.get_relationships_from(&from_id).await.unwrap();
  assert_eq!(rels_from.len(), 2);

  // Get relationships of specific type
  let contradicts = db
    .get_relationships_of_type(&from_id, RelationshipType::Contradicts)
    .await
    .unwrap();
  assert_eq!(contradicts.len(), 1);
  assert_eq!(contradicts[0].to_memory_id, to_id2);

  // Count relationships
  let count = db.count_relationships(&from_id).await.unwrap();
  assert_eq!(count, 2);

  // Delete relationships for a memory
  db.delete_relationships_for_memory(&from_id).await.unwrap();
  let count_after = db.count_relationships(&from_id).await.unwrap();
  assert_eq!(count_after, 0);
}

/// Test RelationshipType parsing
#[test]
fn test_relationship_type_from_str() {
  use engram_core::RelationshipType;

  // Test all types
  assert_eq!(
    "supersedes".parse::<RelationshipType>().unwrap(),
    RelationshipType::Supersedes
  );
  assert_eq!(
    "contradicts".parse::<RelationshipType>().unwrap(),
    RelationshipType::Contradicts
  );
  assert_eq!(
    "related_to".parse::<RelationshipType>().unwrap(),
    RelationshipType::RelatedTo
  );
  assert_eq!(
    "builds_on".parse::<RelationshipType>().unwrap(),
    RelationshipType::BuildsOn
  );
  assert_eq!(
    "confirms".parse::<RelationshipType>().unwrap(),
    RelationshipType::Confirms
  );
  assert_eq!(
    "applies_to".parse::<RelationshipType>().unwrap(),
    RelationshipType::AppliesTo
  );
  assert_eq!(
    "depends_on".parse::<RelationshipType>().unwrap(),
    RelationshipType::DependsOn
  );
  assert_eq!(
    "alternative_to".parse::<RelationshipType>().unwrap(),
    RelationshipType::AlternativeTo
  );

  // Test case insensitivity
  assert_eq!(
    "SUPERSEDES".parse::<RelationshipType>().unwrap(),
    RelationshipType::Supersedes
  );

  // Test alternate forms
  assert_eq!(
    "relatedto".parse::<RelationshipType>().unwrap(),
    RelationshipType::RelatedTo
  );

  // Test invalid
  assert!("invalid".parse::<RelationshipType>().is_err());
}

/// Test entity extraction subsystem
#[tokio::test]
async fn test_entity_operations() {
  use db::ProjectDb;
  use engram_core::{Entity, EntityType};
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = engram_core::ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id, db_path, 768).await.unwrap();

  // Create an entity
  let entity = Entity {
    id: uuid::Uuid::new_v4(),
    name: "John Doe".to_string(),
    entity_type: EntityType::Person,
    summary: Some("A test user".to_string()),
    aliases: vec!["JD".to_string(), "Johnnie".to_string()],
    first_seen_at: chrono::Utc::now(),
    last_seen_at: chrono::Utc::now(),
    mention_count: 1,
  };

  // Add entity
  db.add_entity(&entity).await.unwrap();

  // Get entity by ID
  let retrieved = db.get_entity(&entity.id).await.unwrap();
  assert!(retrieved.is_some(), "Should retrieve entity by ID");
  let retrieved = retrieved.unwrap();
  assert_eq!(retrieved.name, "John Doe");
  assert_eq!(retrieved.entity_type, EntityType::Person);

  // Find entity by name
  let found = db.find_entity_by_name("John Doe").await.unwrap();
  assert!(found.is_some(), "Should find entity by name");
  assert_eq!(found.unwrap().id, entity.id);

  // Update entity
  let mut updated = retrieved.clone();
  updated.mention_count = 5;
  updated.summary = Some("An updated test user".to_string());
  db.update_entity(&updated).await.unwrap();

  let after_update = db.get_entity(&entity.id).await.unwrap().unwrap();
  assert_eq!(after_update.mention_count, 5);
  assert_eq!(after_update.summary, Some("An updated test user".to_string()));

  // Add another entity of different type
  let entity2 = Entity {
    id: uuid::Uuid::new_v4(),
    name: "Rust".to_string(),
    entity_type: EntityType::Technology,
    summary: Some("A programming language".to_string()),
    aliases: vec!["rust-lang".to_string()],
    first_seen_at: chrono::Utc::now(),
    last_seen_at: chrono::Utc::now(),
    mention_count: 1,
  };
  db.add_entity(&entity2).await.unwrap();

  // List entities by type
  let people = db.list_entities_by_type(EntityType::Person).await.unwrap();
  assert_eq!(people.len(), 1);
  assert_eq!(people[0].name, "John Doe");

  let techs = db.list_entities_by_type(EntityType::Technology).await.unwrap();
  assert_eq!(techs.len(), 1);
  assert_eq!(techs[0].name, "Rust");

  // Get top entities
  let top = db.get_top_entities(10).await.unwrap();
  assert_eq!(top.len(), 2);
  // After update, John Doe has 5 mentions so should be first
  assert_eq!(top[0].name, "John Doe");

  // Record additional mention
  db.record_entity_mention(&entity2.id).await.unwrap();
  let after_mention = db.get_entity(&entity2.id).await.unwrap().unwrap();
  assert_eq!(after_mention.mention_count, 2);

  // Find or create (existing)
  let found_or_created = db.find_or_create_entity("John Doe", EntityType::Person).await.unwrap();
  assert_eq!(found_or_created.id, entity.id, "Should find existing entity");

  // Find or create (new)
  let new_entity = db.find_or_create_entity("React", EntityType::Technology).await.unwrap();
  assert_ne!(new_entity.id, entity.id);
  assert_eq!(new_entity.name, "React");

  // Delete entity
  db.delete_entity(&entity.id).await.unwrap();
  let after_delete = db.get_entity(&entity.id).await.unwrap();
  assert!(after_delete.is_none(), "Entity should be deleted");
}

/// Test memory-entity linkage
#[tokio::test]
async fn test_memory_entity_links() {
  use db::ProjectDb;
  use engram_core::{Entity, EntityRole, EntityType, MemoryEntityLink};
  use std::path::Path;

  let data_dir = TempDir::new().expect("Failed to create temp dir");
  let db_path = data_dir.path().join("test.lancedb");
  let project_id = engram_core::ProjectId::from_path(Path::new("/test"));

  let db = ProjectDb::open_at_path(project_id, db_path, 768).await.unwrap();

  // Create entity
  let entity = Entity {
    id: uuid::Uuid::new_v4(),
    name: "Test Entity".to_string(),
    entity_type: EntityType::Concept,
    summary: None,
    aliases: vec![],
    first_seen_at: chrono::Utc::now(),
    last_seen_at: chrono::Utc::now(),
    mention_count: 1,
  };
  db.add_entity(&entity).await.unwrap();

  // Create memory IDs (as strings)
  let memory_id1 = uuid::Uuid::new_v4().to_string();
  let memory_id2 = uuid::Uuid::new_v4().to_string();

  // Create and link entity to memories
  let link1 = MemoryEntityLink::new(memory_id1.clone(), entity.id, EntityRole::Subject, 0.9);
  db.link_entity_to_memory(&link1).await.unwrap();
  assert_eq!(link1.role, EntityRole::Subject);
  assert_eq!(link1.confidence, 0.9);

  let link2 = MemoryEntityLink::new(memory_id2.clone(), entity.id, EntityRole::Reference, 0.7);
  db.link_entity_to_memory(&link2).await.unwrap();
  assert_eq!(link2.role, EntityRole::Reference);

  // Get entity links for a memory
  let mem1_links = db.get_memory_entity_links(&memory_id1).await.unwrap();
  assert_eq!(mem1_links.len(), 1);
  assert_eq!(mem1_links[0].entity_id, entity.id);

  // Get memory links for an entity
  let entity_links = db.get_entity_memory_links(&entity.id).await.unwrap();
  assert_eq!(entity_links.len(), 2);

  // Check if link exists
  let exists = db.entity_memory_link_exists(&memory_id1, &entity.id).await.unwrap();
  assert!(exists, "Link should exist");

  let not_exists = db.entity_memory_link_exists("fake-id", &entity.id).await.unwrap();
  assert!(!not_exists, "Link should not exist for fake memory");

  // Count entity memories
  let count = db.count_entity_memories(&entity.id).await.unwrap();
  assert_eq!(count, 2);

  // Delete all links for the memory
  db.delete_memory_entity_links(&memory_id1).await.unwrap();
  let after_delete = db.get_memory_entity_links(&memory_id1).await.unwrap();
  assert!(after_delete.is_empty(), "Link should be deleted");

  // Count again
  let count_after = db.count_entity_memories(&entity.id).await.unwrap();
  assert_eq!(count_after, 1);
}

/// Test entity tools via router
#[tokio::test]
async fn test_router_entity_tools() {
  use engram_core::{Entity, EntityType};

  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // First, create an entity directly in the database
  let project_path = project_dir.path();
  let project_id = engram_core::ProjectId::from_path(project_path);
  let data_dir = _data_dir
    .path()
    .join("projects")
    .join(project_id.as_str())
    .join("lancedb");
  std::fs::create_dir_all(&data_dir).unwrap();
  let db = db::ProjectDb::open_at_path(project_id, data_dir, 768).await.unwrap();

  let entity = Entity {
    id: uuid::Uuid::new_v4(),
    name: "Rust".to_string(),
    entity_type: EntityType::Technology,
    summary: Some("A systems programming language".to_string()),
    aliases: vec!["rust-lang".to_string()],
    first_seen_at: chrono::Utc::now(),
    last_seen_at: chrono::Utc::now(),
    mention_count: 5,
  };
  db.add_entity(&entity).await.unwrap();

  // Test entity_list
  let list_request = Request {
    id: Some(serde_json::json!(1)),
    method: "entity_list".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let list_response = router.handle(list_request).await;
  assert!(
    list_response.error.is_none(),
    "entity_list should succeed: {:?}",
    list_response.error
  );

  // Test entity_get by name
  let get_request = Request {
    id: Some(serde_json::json!(2)),
    method: "entity_get".to_string(),
    params: serde_json::json!({ "cwd": cwd, "name": "Rust" }),
  };
  let get_response = router.handle(get_request).await;
  assert!(
    get_response.error.is_none(),
    "entity_get should succeed: {:?}",
    get_response.error
  );
  let result = get_response.result.expect("Should have result");
  assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Rust"));

  // Test entity_top
  let top_request = Request {
    id: Some(serde_json::json!(3)),
    method: "entity_top".to_string(),
    params: serde_json::json!({ "cwd": cwd, "limit": 5 }),
  };
  let top_response = router.handle(top_request).await;
  assert!(
    top_response.error.is_none(),
    "entity_top should succeed: {:?}",
    top_response.error
  );
}

/// Test router relationship management tools
#[tokio::test]
async fn test_router_relationship_tools() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Create two memories to relate
  let add_first = Request {
    id: Some(serde_json::json!(1)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "The authentication system uses JWT tokens for session management",
        "cwd": cwd
    }),
  };
  let first_response = router.handle(add_first).await;
  assert!(first_response.error.is_none(), "First memory creation should succeed");
  let first_id = first_response
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  let add_second = Request {
    id: Some(serde_json::json!(2)),
    method: "memory_add".to_string(),
    params: serde_json::json!({
        "content": "JWT tokens are stored in httpOnly cookies for security",
        "cwd": cwd
    }),
  };
  let second_response = router.handle(add_second).await;
  assert!(second_response.error.is_none(), "Second memory creation should succeed");
  let second_id = second_response
    .result
    .unwrap()
    .get("id")
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();

  // Create a relationship between them
  let add_rel = Request {
    id: Some(serde_json::json!(3)),
    method: "relationship_add".to_string(),
    params: serde_json::json!({
        "from_memory_id": first_id,
        "to_memory_id": second_id,
        "relationship_type": "builds_on",
        "confidence": 0.85,
        "cwd": cwd
    }),
  };
  let rel_response = router.handle(add_rel).await;
  assert!(rel_response.error.is_none(), "Relationship creation should succeed");
  let rel_result = rel_response.result.unwrap();
  let rel_id = rel_result
    .get("id")
    .and_then(|v| v.as_str())
    .expect("Should return relationship ID");
  assert_eq!(
    rel_result.get("relationship_type").and_then(|v| v.as_str()),
    Some("builds_on")
  );
  let confidence = rel_result.get("confidence").and_then(|v| v.as_f64()).unwrap();
  assert!(
    (confidence - 0.85).abs() < 0.001,
    "Confidence should be approximately 0.85"
  );

  // List relationships for the first memory
  let list_rel = Request {
    id: Some(serde_json::json!(4)),
    method: "relationship_list".to_string(),
    params: serde_json::json!({
        "memory_id": first_id,
        "cwd": cwd
    }),
  };
  let list_response = router.handle(list_rel).await;
  assert!(list_response.error.is_none(), "Relationship listing should succeed");
  let list_result = list_response.result.unwrap();
  let relationships = list_result.as_array().unwrap();
  assert_eq!(relationships.len(), 1);
  assert_eq!(
    relationships[0].get("to_memory_id").and_then(|v| v.as_str()),
    Some(second_id.as_str())
  );

  // Get related memories with full context
  let get_related = Request {
    id: Some(serde_json::json!(5)),
    method: "relationship_related".to_string(),
    params: serde_json::json!({
        "memory_id": first_id,
        "cwd": cwd
    }),
  };
  let related_response = router.handle(get_related).await;
  assert!(
    related_response.error.is_none(),
    "Getting related memories should succeed"
  );
  let related_result = related_response.result.unwrap();
  let related_arr = related_result.as_array().unwrap();
  assert_eq!(related_arr.len(), 1);
  // Should have both relationship info and memory content
  assert!(related_arr[0].get("memory").is_some(), "Should include memory content");
  assert!(
    related_arr[0].get("relationship").is_some(),
    "Should include relationship info"
  );

  // Test filtering by relationship type
  let list_filtered = Request {
    id: Some(serde_json::json!(6)),
    method: "relationship_list".to_string(),
    params: serde_json::json!({
        "memory_id": first_id,
        "relationship_type": "contradicts",
        "cwd": cwd
    }),
  };
  let filtered_response = router.handle(list_filtered).await;
  assert!(filtered_response.error.is_none(), "Filtered listing should succeed");
  let filtered_result = filtered_response.result.unwrap();
  let filtered_rels = filtered_result.as_array().unwrap();
  assert_eq!(filtered_rels.len(), 0, "Should find no contradicts relationships");

  // Delete the relationship
  let delete_rel = Request {
    id: Some(serde_json::json!(7)),
    method: "relationship_delete".to_string(),
    params: serde_json::json!({
        "relationship_id": rel_id,
        "cwd": cwd
    }),
  };
  let delete_response = router.handle(delete_rel).await;
  assert!(delete_response.error.is_none(), "Relationship deletion should succeed");
  let delete_result = delete_response.result.unwrap();
  assert_eq!(delete_result.get("deleted").and_then(|v| v.as_bool()), Some(true));

  // Verify relationship was deleted
  let list_after_delete = Request {
    id: Some(serde_json::json!(8)),
    method: "relationship_list".to_string(),
    params: serde_json::json!({
        "memory_id": first_id,
        "cwd": cwd
    }),
  };
  let after_delete_response = router.handle(list_after_delete).await;
  let after_delete_result = after_delete_response.result.unwrap();
  let after_delete_rels = after_delete_result.as_array().unwrap();
  assert_eq!(after_delete_rels.len(), 0, "Relationship should be deleted");
}
