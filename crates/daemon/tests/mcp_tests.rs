//! MCP protocol compliance tests for explore and context tools.
//!
//! These tests verify that the tools conform to the MCP (Model Context Protocol)
//! specification for tool definitions and responses.

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

// ============================================================================
// Tool Availability Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_explore_tool_is_available() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Test that the explore tool is callable and returns proper structure
    let request = make_request(
        "explore",
        json!({
            "query": "test",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    // Should succeed (not method not found)
    assert!(response.error.is_none() || response.error.as_ref().unwrap().code != -32601,
        "explore tool should be available (not method not found)");
}

#[tokio::test]
async fn test_mcp_context_tool_is_available() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory to have a valid ID
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Test memory for context tool availability check",
            "type": "observation",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let memory_id = add_response.result.unwrap()["id"].as_str().unwrap().to_string();

    // Test that the context tool is callable
    let request = make_request(
        "context",
        json!({
            "id": memory_id,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    // Should succeed (not method not found)
    assert!(response.error.is_none() || response.error.as_ref().unwrap().code != -32601,
        "context tool should be available (not method not found)");
}

// ============================================================================
// Request/Response Format Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_explore_request_with_all_params() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("test.rs");
    fs::write(&test_file, "pub fn test_fn() {}").unwrap();

    // Index
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Full explore request with all parameters
    let request = make_request(
        "explore",
        json!({
            "query": "test",
            "scope": "code",
            "expand_top": 2,
            "limit": 5,
            "format": "json",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_none(), "Request with all params should succeed");
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_mcp_explore_response_structure() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create a test file
    let test_file = project_dir.path().join("example.rs");
    fs::write(&test_file, "pub fn example() { todo!() }").unwrap();

    // Index
    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    let request = make_request(
        "explore",
        json!({
            "query": "example",
            "scope": "code",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();

    // Verify MCP-compliant response structure
    assert!(result["results"].is_array(), "results should be an array");
    assert!(result["counts"].is_object(), "counts should be an object");
    assert!(result["suggestions"].is_array(), "suggestions should be an array");

    // Check result item structure
    let results = result["results"].as_array().unwrap();
    if !results.is_empty() {
        let item = &results[0];
        assert!(item["id"].is_string(), "result should have id");
        assert!(item["type"].is_string(), "result should have type");
        assert!(item["preview"].is_string(), "result should have preview");
        assert!(item["score"].is_number(), "result should have score");
        assert!(item["hints"].is_object(), "result should have hints");
    }
}

#[tokio::test]
async fn test_mcp_context_request_single_id() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory to get an ID
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Test memory for MCP compliance",
            "type": "observation",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let memory_id = add_response.result.unwrap()["id"].as_str().unwrap().to_string();

    // Context request with single id
    let request = make_request(
        "context",
        json!({
            "id": memory_id,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_mcp_context_request_multiple_ids() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add memories to get IDs
    let mut ids = Vec::new();
    for i in 1..=2 {
        let add_request = make_request(
            "memory_add",
            json!({
                "content": format!("Test memory {} for MCP compliance", i),
                "type": "observation",
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );
        let add_response = router.handle(add_request).await;
        ids.push(add_response.result.unwrap()["id"].as_str().unwrap().to_string());
    }

    // Context request with multiple ids
    let request = make_request(
        "context",
        json!({
            "ids": ids,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_mcp_context_response_structure_code() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Create and index code
    let test_file = project_dir.path().join("handler.rs");
    fs::write(&test_file, "pub fn handle() { todo!() }").unwrap();

    let index_request = make_request(
        "code_index",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    router.handle(index_request).await;

    // Get ID via explore
    let explore_request = make_request(
        "explore",
        json!({
            "query": "handle",
            "scope": "code",
            "expand_top": 0,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let explore_response = router.handle(explore_request).await;
    let results = explore_response.result.unwrap()["results"].as_array().unwrap().clone();

    if !results.is_empty() {
        let chunk_id = results[0]["id"].as_str().unwrap();

        let request = make_request(
            "context",
            json!({
                "id": chunk_id,
                "cwd": project_dir.path().to_str().unwrap()
            }),
        );

        let response = router.handle(request).await;
        assert!(response.result.is_some());

        let result = response.result.unwrap();

        // Verify code context structure
        assert_eq!(result["type"], "code");
        assert!(result["items"].is_array());

        let items = result["items"].as_array().unwrap();
        if !items.is_empty() {
            let item = &items[0];
            assert!(item["id"].is_string());
            assert!(item["file"].is_string());
            assert!(item["content"].is_string());
            assert!(item["language"].is_string());
            assert!(item["lines"].is_array());
            assert!(item["symbols"].is_array());
            assert!(item["imports"].is_array());
            assert!(item["callers"].is_array());
            assert!(item["callees"].is_array());
            assert!(item["siblings"].is_array());
            assert!(item["memories"].is_array());
        }
    }
}

#[tokio::test]
async fn test_mcp_context_response_structure_memory() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Test memory for structure check",
            "type": "decision",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let memory_id = add_response.result.unwrap()["id"].as_str().unwrap().to_string();

    let request = make_request(
        "context",
        json!({
            "id": memory_id,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.result.is_some());

    let result = response.result.unwrap();

    // Verify memory context structure
    assert_eq!(result["type"], "memory");
    assert!(result["items"].is_array());

    let items = result["items"].as_array().unwrap();
    assert!(!items.is_empty());

    let item = &items[0];
    assert!(item["id"].is_string());
    assert!(item["content"].is_string());
    assert!(item["sector"].is_string());
    assert!(item["type"].is_string());
    assert!(item["salience"].is_number());
    assert!(item["created_at"].is_string());
    assert!(item["timeline"].is_object());
    assert!(item["timeline"]["before"].is_array());
    assert!(item["timeline"]["after"].is_array());
    assert!(item["related"].is_array());
}

// ============================================================================
// Error Response Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_error_response_structure() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Invalid request (empty query)
    let request = make_request(
        "explore",
        json!({
            "query": "",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());

    let error = response.error.unwrap();
    // MCP error should have code and message
    assert!(error.code != 0, "Error should have non-zero code");
    assert!(!error.message.is_empty(), "Error should have message");
}

#[tokio::test]
async fn test_mcp_error_invalid_params() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Invalid scope parameter
    let request = make_request(
        "explore",
        json!({
            "query": "test",
            "scope": "invalid_scope",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());

    let error = response.error.unwrap();
    // Should be invalid params error (-32602)
    assert_eq!(error.code, -32602, "Invalid params should return -32602");
}

#[tokio::test]
async fn test_mcp_error_method_not_found() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Non-existent method
    let request = make_request(
        "nonexistent_method",
        json!({
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());

    let error = response.error.unwrap();
    // Should be method not found error (-32601)
    assert_eq!(error.code, -32601, "Unknown method should return -32601");
}

// ============================================================================
// JSON-RPC Compliance Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_response_has_id() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = Request {
        id: Some(json!(42)),
        method: "explore".to_string(),
        params: json!({
            "query": "test",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    };

    let response = router.handle(request).await;
    // Response ID should match request ID
    assert_eq!(response.id, Some(json!(42)));
}

#[tokio::test]
async fn test_mcp_response_id_string() {
    let (_data_dir, project_dir, router) = create_test_router();

    let request = Request {
        id: Some(json!("request-123")),
        method: "explore".to_string(),
        params: json!({
            "query": "test",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    };

    let response = router.handle(request).await;
    // Response ID should match request ID even when string
    assert_eq!(response.id, Some(json!("request-123")));
}

#[tokio::test]
async fn test_mcp_batch_context_respects_limit() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Try to request more IDs than allowed (default max is 5)
    let fake_ids: Vec<String> = (0..10).map(|i| format!("fake_id_{:06}", i)).collect();

    let request = make_request(
        "context",
        json!({
            "ids": fake_ids,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some());
    assert!(response.error.unwrap().message.contains("Too many"));
}

// ============================================================================
// Tool Parameter Validation Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_explore_query_required() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Missing query parameter
    let request = make_request(
        "explore",
        json!({
            "scope": "code",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some(), "Missing query should error");
}

#[tokio::test]
async fn test_mcp_context_id_required() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Missing both id and ids
    let request = make_request(
        "context",
        json!({
            "depth": 5,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_some(), "Missing id/ids should error");
}

#[tokio::test]
async fn test_mcp_explore_optional_params_defaults() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Only required param (query)
    let request = make_request(
        "explore",
        json!({
            "query": "test",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    // Should succeed with defaults
    assert!(response.error.is_none());
    assert!(response.result.is_some());

    let result = response.result.unwrap();
    // Defaults should be applied (scope=all returns counts for all types)
    assert!(result["counts"].is_object());
}

#[tokio::test]
async fn test_mcp_context_optional_depth() {
    let (_data_dir, project_dir, router) = create_test_router();

    // Add a memory
    let add_request = make_request(
        "memory_add",
        json!({
            "content": "Test for optional depth",
            "type": "observation",
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );
    let add_response = router.handle(add_request).await;
    let memory_id = add_response.result.unwrap()["id"].as_str().unwrap().to_string();

    // Request without depth (should use default)
    let request = make_request(
        "context",
        json!({
            "id": memory_id,
            "cwd": project_dir.path().to_str().unwrap()
        }),
    );

    let response = router.handle(request).await;
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}
