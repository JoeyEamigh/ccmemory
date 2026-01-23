//! Integration tests for explore and context tools.
//!
//! These tests verify the unified exploration tools work end-to-end.
//!
//! Note: These tests expect Ollama to be running locally with the qwen3-embedding model.
//! Run: ollama pull qwen3-embedding

mod common;

use common::create_test_router;
use daemon::Request;
use serde_json::json;
use std::fs;

/// Helper to create a request
fn make_request(method: &str, params: serde_json::Value) -> Request {
    Request {
        id: Some(json!(1)),
        method: method.to_string(),
        params,
    }
}

#[tokio::test]
async fn test_explore_empty_query() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = make_request(
        "explore",
        json!({
            "query": "",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("empty"));
}

#[tokio::test]
async fn test_explore_invalid_scope() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = make_request(
        "explore",
        json!({
            "query": "test",
            "scope": "invalid",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("Invalid scope"));
}

#[tokio::test]
async fn test_explore_no_results() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Search for something that doesn't exist
    let request = make_request(
        "explore",
        json!({
            "query": "nonexistent_xyz_123",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let results = result["results"].as_array().unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_explore_code_scope() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("test.rs");
    fs::write(&test_file, "pub fn authenticate(user: &str) { todo!() }").unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Now explore for "authenticate" with code scope
    let request = make_request(
        "explore",
        json!({
            "query": "authenticate",
            "scope": "code",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    assert!(result["counts"]["code"].is_number());
    assert!(result["suggestions"].is_array());
}

#[tokio::test]
async fn test_explore_with_expand_top() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create test files
    let test_file = project_dir.path().join("auth.rs");
    fs::write(
        &test_file,
        r#"
pub fn authenticate(credentials: &Credentials) -> Result<Session, AuthError> {
    validate_credentials(credentials)?;
    create_session(credentials.user_id)
}

pub fn validate_credentials(creds: &Credentials) -> Result<(), AuthError> {
    if creds.password.len() < 8 {
        return Err(AuthError::WeakPassword);
    }
    Ok(())
}
"#,
    )
    .unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore with expand_top=2
    let request = make_request(
        "explore",
        json!({
            "query": "authentication",
            "scope": "code",
            "expand_top": 2,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let results = result["results"].as_array().unwrap();

    // Check that top results have context expanded
    for (i, r) in results.iter().enumerate() {
        if i < 2 {
            // Top 2 should have context (if they exist)
            if r["context"].is_object() {
                assert!(r["context"]["content"].is_string());
            }
        }
    }
}

#[tokio::test]
async fn test_context_missing_id() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = make_request(
        "context",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("id"));
}

#[tokio::test]
async fn test_context_id_too_short() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = make_request(
        "context",
        json!({
            "id": "abc",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("6 characters"));
}

#[tokio::test]
async fn test_context_not_found() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = make_request(
        "context",
        json!({
            "id": "nonexistent_id_12345",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("not found"));
}

#[tokio::test]
async fn test_context_batch_too_many() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = make_request(
        "context",
        json!({
            "ids": ["a12345", "b12345", "c12345", "d12345", "e12345", "f12345"],
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("Too many"));
}

#[tokio::test]
async fn test_explore_then_context_workflow() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("service.rs");
    fs::write(
        &test_file,
        r#"
pub struct UserService {
    db: Database,
}

impl UserService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn find_user(&self, id: u64) -> Option<User> {
        self.db.query_user(id)
    }

    pub fn create_user(&self, name: &str) -> User {
        self.db.insert_user(name)
    }
}
"#,
    )
    .unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Step 1: Explore
    let explore_request = make_request(
        "explore",
        json!({
            "query": "UserService",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let explore_response = router.handle(explore_request).await;
    assert!(explore_response.result.is_some());

    let explore_result = explore_response.result.unwrap();
    let results = explore_result["results"].as_array().unwrap();

    if !results.is_empty() {
        // Step 2: Get context for first result
        let chunk_id = results[0]["id"].as_str().unwrap();

        let context_request = make_request(
            "context",
            json!({
                "id": chunk_id,
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );

        let context_response = router.handle(context_request).await;
        assert!(context_response.result.is_some());

        let context_result = context_response.result.unwrap();
        assert!(context_result["type"].is_string());
        assert!(context_result["items"].is_array());
    }
}

#[tokio::test]
async fn test_explore_suggestions_generated() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create test files with authentication-related code
    let test_file = project_dir.path().join("auth.rs");
    fs::write(
        &test_file,
        r#"
pub fn login(username: &str, password: &str) -> Session {
    authenticate(username, password);
    create_session()
}

pub fn authenticate(username: &str, password: &str) -> bool {
    validate_credentials(username, password)
}
"#,
    )
    .unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore for "auth"
    let request = make_request(
        "explore",
        json!({
            "query": "auth",
            "scope": "code",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let suggestions = result["suggestions"].as_array().unwrap();

    // Should have some suggestions
    assert!(!suggestions.is_empty() || result["results"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_explore_memory_scope() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory first
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "User authentication should use bcrypt with cost factor 12",
            "type": "decision",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(add_request).await;

    // Now explore memories
    let request = make_request(
        "explore",
        json!({
            "query": "authentication bcrypt",
            "scope": "memory",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    assert!(result["counts"]["memory"].is_number());
}

#[tokio::test]
async fn test_explore_docs_scope() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a markdown doc
    let doc_file = project_dir.path().join("README.md");
    fs::write(
        &doc_file,
        r#"# Authentication Guide

This document describes the authentication flow.

## Login Process

Users authenticate using their email and password.
The system validates credentials against the database.
"#,
    )
    .unwrap();

    // Index the document
    let index_request = make_request(
        "docs_index",
        json!({
            "directory": project_dir.path().to_str().unwrap(),
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Now explore docs
    let request = make_request(
        "explore",
        json!({
            "query": "authentication login",
            "scope": "docs",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    assert!(result["counts"]["docs"].is_number());
}

#[tokio::test]
async fn test_explore_all_scopes() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("auth.rs");
    fs::write(&test_file, "pub fn authenticate(user: &str) { todo!() }").unwrap();

    // Add a memory
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Authentication uses JWT tokens",
            "type": "decision",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(add_request).await;

    // Index the code
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore with scope="all"
    let request = make_request(
        "explore",
        json!({
            "query": "authenticate",
            "scope": "all",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    // Should have counts for code and memory (docs may be empty)
    assert!(result["counts"].is_object());
}

#[tokio::test]
async fn test_explore_expand_top_zero() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("service.rs");
    fs::write(&test_file, "pub fn process_data(data: &str) { todo!() }").unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore with expand_top=0 (no context expansion)
    let request = make_request(
        "explore",
        json!({
            "query": "process",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let results = result["results"].as_array().unwrap();

    // All results should have no context
    for r in results {
        assert!(r["context"].is_null(), "Result should not have context when expand_top=0");
    }
}

#[tokio::test]
async fn test_explore_hints_computed() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create test files with function calls
    let test_file = project_dir.path().join("caller.rs");
    fs::write(
        &test_file,
        r#"
pub fn caller_function() {
    helper_function();
}

pub fn helper_function() {
    // Helper implementation
}
"#,
    )
    .unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore
    let request = make_request(
        "explore",
        json!({
            "query": "helper",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let results = result["results"].as_array().unwrap();

    // Check that hints are present on code results
    for r in results {
        if r["type"].as_str() == Some("code") {
            let hints = &r["hints"];
            // Hints should be present (may be null but the field should exist)
            assert!(hints.is_object(), "Code results should have hints object");
        }
    }
}

#[tokio::test]
async fn test_explore_ranking() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create files with varying relevance
    let file1 = project_dir.path().join("auth_service.rs");
    fs::write(
        &file1,
        r#"
pub struct AuthenticationService {
    // Primary authentication service
}

impl AuthenticationService {
    pub fn authenticate(&self, user: &str, pass: &str) -> bool {
        true
    }
}
"#,
    )
    .unwrap();

    let file2 = project_dir.path().join("utils.rs");
    fs::write(
        &file2,
        r#"
pub fn format_string(s: &str) -> String {
    s.to_string()
}
"#,
    )
    .unwrap();

    // Index files
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore for "authentication"
    let request = make_request(
        "explore",
        json!({
            "query": "authentication service",
            "scope": "code",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let results = result["results"].as_array().unwrap();

    // Results should be sorted by score (descending)
    let mut prev_score = f64::MAX;
    for r in results {
        let score = r["score"].as_f64().unwrap_or(0.0);
        assert!(
            score <= prev_score,
            "Results should be sorted by score descending"
        );
        prev_score = score;
    }
}

#[tokio::test]
async fn test_context_single_code() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("handler.rs");
    fs::write(
        &test_file,
        r#"
pub fn handle_request(req: Request) -> Response {
    validate(req);
    process(req)
}
"#,
    )
    .unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // First explore to get an ID
    let explore_request = make_request(
        "explore",
        json!({
            "query": "handle_request",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let explore_response = router.handle(explore_request).await;
    let explore_result = explore_response.result.unwrap();
    let results = explore_result["results"].as_array().unwrap();

    if !results.is_empty() {
        let chunk_id = results[0]["id"].as_str().unwrap();

        // Get context for single code chunk
        let context_request = make_request(
            "context",
            json!({
                "id": chunk_id,
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );

        let context_response = router.handle(context_request).await;
        assert!(context_response.result.is_some());

        let context_result = context_response.result.unwrap();
        assert_eq!(context_result["type"].as_str(), Some("code"));
        assert!(context_result["items"].is_array());

        let items = context_result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);

        // Check code context fields
        let item = &items[0];
        assert!(item["id"].is_string());
        assert!(item["file"].is_string());
        assert!(item["content"].is_string());
        assert!(item["language"].is_string());
        assert!(item["lines"].is_array());
        assert!(item["symbols"].is_array());
        assert!(item["callers"].is_array());
        assert!(item["callees"].is_array());
        assert!(item["siblings"].is_array());
        assert!(item["memories"].is_array());
    }
}

#[tokio::test]
async fn test_context_single_memory() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Database connections should be pooled with max 10 connections",
            "type": "decision",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    assert!(add_response.result.is_some());

    let add_result = add_response.result.unwrap();
    let memory_id = add_result["id"].as_str().unwrap();

    // Get context for single memory
    let context_request = make_request(
        "context",
        json!({
            "id": memory_id,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let context_response = router.handle(context_request).await;
    assert!(context_response.result.is_some());

    let context_result = context_response.result.unwrap();
    assert_eq!(context_result["type"].as_str(), Some("memory"));
    assert!(context_result["items"].is_array());

    let items = context_result["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);

    // Check memory context fields
    let item = &items[0];
    assert!(item["id"].is_string());
    assert!(item["content"].is_string());
    assert!(item["sector"].is_string());
    assert!(item["type"].is_string());
    assert!(item["salience"].is_number());
    assert!(item["created_at"].is_string());
    assert!(item["timeline"].is_object());
    assert!(item["related"].is_array());
}

#[tokio::test]
async fn test_context_single_doc() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a markdown doc
    let doc_file = project_dir.path().join("guide.md");
    fs::write(
        &doc_file,
        r#"# API Guide

## Getting Started

This is the introduction to the API.

## Authentication

Use bearer tokens for authentication.
"#,
    )
    .unwrap();

    // Index the document
    let index_request = make_request(
        "docs_index",
        json!({
            "directory": project_dir.path().to_str().unwrap(),
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // First explore to get an ID
    let explore_request = make_request(
        "explore",
        json!({
            "query": "API authentication",
            "scope": "docs",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let explore_response = router.handle(explore_request).await;
    let explore_result = explore_response.result.unwrap();
    let results = explore_result["results"].as_array().unwrap();

    if !results.is_empty() {
        let doc_id = results[0]["id"].as_str().unwrap();

        // Get context for single doc
        let context_request = make_request(
            "context",
            json!({
                "id": doc_id,
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );

        let context_response = router.handle(context_request).await;
        assert!(context_response.result.is_some());

        let context_result = context_response.result.unwrap();
        assert_eq!(context_result["type"].as_str(), Some("doc"));
        assert!(context_result["items"].is_array());

        let items = context_result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);

        // Check doc context fields
        let item = &items[0];
        assert!(item["id"].is_string());
        assert!(item["title"].is_string());
        assert!(item["content"].is_string());
        assert!(item["source"].is_string());
        assert!(item["chunk_index"].is_number());
        assert!(item["total_chunks"].is_number());
        assert!(item["before"].is_array());
        assert!(item["after"].is_array());
    }
}

#[tokio::test]
async fn test_context_batch() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add multiple memories
    let mut memory_ids = Vec::new();
    for i in 1..=3 {
        let add_request = make_request(
            "memory_add",
            json!({
                "content": format!("Memory content number {}", i),
                "type": "observation",
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );
        let add_response = router.handle(add_request).await;
        let add_result = add_response.result.unwrap();
        memory_ids.push(add_result["id"].as_str().unwrap().to_string());
    }

    // Get context for multiple IDs
    let context_request = make_request(
        "context",
        json!({
            "ids": memory_ids,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let context_response = router.handle(context_request).await;
    assert!(context_response.result.is_some());

    let context_result = context_response.result.unwrap();
    assert_eq!(context_result["type"].as_str(), Some("memory"));

    let items = context_result["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
}

#[tokio::test]
async fn test_context_prefix_match() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a code file (prefix matching works for code chunks)
    let test_file = project_dir.path().join("prefix_test.rs");
    fs::write(&test_file, "pub fn prefix_test_function() { todo!() }").unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Get the full ID via explore
    let explore_request = make_request(
        "explore",
        json!({
            "query": "prefix_test_function",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let explore_response = router.handle(explore_request).await;
    let results = explore_response.result.unwrap()["results"].as_array().unwrap().clone();

    if !results.is_empty() {
        let full_id = results[0]["id"].as_str().unwrap();

        // Use 8-character prefix (should work for code chunks)
        let prefix = &full_id[..8.min(full_id.len())];

        let context_request = make_request(
            "context",
            json!({
                "id": prefix,
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );

        let context_response = router.handle(context_request).await;
        assert!(context_response.result.is_some(), "8-char prefix should resolve for code chunks");
    }
}

#[tokio::test]
async fn test_context_full_memory_id() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory (memory IDs require full UUID, not prefix)
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Caching should use Redis for session storage",
            "type": "decision",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let add_result = add_response.result.unwrap();
    let full_id = add_result["id"].as_str().unwrap();

    // Use full ID for memory
    let context_request = make_request(
        "context",
        json!({
            "id": full_id,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let context_response = router.handle(context_request).await;
    assert!(context_response.result.is_some(), "Full memory ID should resolve");
}

#[tokio::test]
async fn test_context_mixed_types() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a code file
    let test_file = project_dir.path().join("service.rs");
    fs::write(&test_file, "pub fn serve() { todo!() }").unwrap();

    // Index the code
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Add a memory
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Service layer should be stateless",
            "type": "decision",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let memory_id = add_response.result.unwrap()["id"].as_str().unwrap().to_string();

    // Get code chunk ID via explore
    let explore_request = make_request(
        "explore",
        json!({
            "query": "serve",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let explore_response = router.handle(explore_request).await;
    let results = explore_response.result.unwrap()["results"].as_array().unwrap().clone();

    if !results.is_empty() {
        let code_id = results[0]["id"].as_str().unwrap().to_string();

        // Request context for both code and memory IDs
        let context_request = make_request(
            "context",
            json!({
                "ids": [code_id, memory_id],
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );

        let context_response = router.handle(context_request).await;
        assert!(context_response.result.is_some());

        let context_result = context_response.result.unwrap();
        // Should return mixed type when different item types
        assert_eq!(context_result["type"].as_str(), Some("mixed"));
        assert!(context_result["code"].is_array());
        assert!(context_result["memories"].is_array());
    }
}

#[tokio::test]
async fn test_explore_text_format() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("example.rs");
    fs::write(&test_file, "pub fn example_function() { todo!() }").unwrap();

    // Index the file
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore with text format
    let request = make_request(
        "explore",
        json!({
            "query": "example",
            "scope": "code",
            "format": "text",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    // Text format returns a string
    assert!(result.is_string());
    let text = result.as_str().unwrap();
    assert!(text.contains("Found") || text.contains("result"));
}

#[tokio::test]
async fn test_context_text_format() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Format test memory content",
            "type": "observation",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let memory_id = add_response.result.unwrap()["id"].as_str().unwrap().to_string();

    // Get context with text format
    let context_request = make_request(
        "context",
        json!({
            "id": memory_id,
            "format": "text",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let context_response = router.handle(context_request).await;
    assert!(context_response.result.is_some());

    let result = context_response.result.unwrap();
    // Text format returns a string
    assert!(result.is_string());
    let text = result.as_str().unwrap();
    assert!(text.contains("Memory context") || text.contains("memory"));
}

#[tokio::test]
async fn test_explore_limit_parameter() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create multiple test files
    for i in 1..=5 {
        let test_file = project_dir.path().join(format!("file{}.rs", i));
        fs::write(&test_file, format!("pub fn function{}() {{ todo!() }}", i)).unwrap();
    }

    // Index all files
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Explore with limit=2
    let request = make_request(
        "explore",
        json!({
            "query": "function",
            "scope": "code",
            "limit": 2,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    let results = result["results"].as_array().unwrap();

    // Should respect limit
    assert!(results.len() <= 2, "Results should be limited to 2");
}
