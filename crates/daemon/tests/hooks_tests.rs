//! Hook and watcher integration tests for the CCEngram daemon
//!
//! Tests: watcher tools, hook handler for various events.

mod common;

use daemon::Request;

/// Test watcher tools via router
#[tokio::test]
async fn test_router_watcher_tools() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Test watch_start
  let start_request = Request {
    id: Some(serde_json::json!(1)),
    method: "watch_start".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let start_response = router.handle(start_request).await;
  assert!(
    start_response.error.is_none(),
    "watch_start should succeed: {:?}",
    start_response.error
  );
  let result = start_response.result.expect("Should have result");
  assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("started"));

  // Test watch_status
  let status_request = Request {
    id: Some(serde_json::json!(2)),
    method: "watch_status".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let status_response = router.handle(status_request).await;
  assert!(
    status_response.error.is_none(),
    "watch_status should succeed: {:?}",
    status_response.error
  );
  let result = status_response.result.expect("Should have result");
  assert_eq!(result.get("running").and_then(|v| v.as_bool()), Some(true));

  // Test watch_stop
  let stop_request = Request {
    id: Some(serde_json::json!(3)),
    method: "watch_stop".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let stop_response = router.handle(stop_request).await;
  assert!(
    stop_response.error.is_none(),
    "watch_stop should succeed: {:?}",
    stop_response.error
  );
  let result = stop_response.result.expect("Should have result");
  assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("stopped"));

  // Verify status shows stopped
  let status_request2 = Request {
    id: Some(serde_json::json!(4)),
    method: "watch_status".to_string(),
    params: serde_json::json!({ "cwd": cwd }),
  };
  let status_response2 = router.handle(status_request2).await;
  let result = status_response2.result.expect("Should have result");
  assert_eq!(result.get("running").and_then(|v| v.as_bool()), Some(false));
}

/// Test hook handler via router
/// Valid hooks: SessionStart, SessionEnd, UserPromptSubmit, PostToolUse, PreCompact, Stop, SubagentStop, Notification
#[tokio::test]
async fn test_router_hook_handler() {
  let (_data_dir, project_dir, router) = common::create_test_router();
  let cwd = project_dir.path().to_string_lossy().to_string();

  // Test SessionStart hook (uses "event" field and nested "params")
  let session_start = Request {
    id: Some(serde_json::json!(1)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "SessionStart",
        "params": {
            "session_id": "test-session-123",
            "cwd": cwd
        }
    }),
  };
  let start_response = router.handle(session_start).await;
  assert!(
    start_response.error.is_none(),
    "SessionStart hook should succeed: {:?}",
    start_response.error
  );
  let start_result = start_response.result.unwrap();
  assert!(start_result.get("status").is_some(), "Should have status");

  // Test PostToolUse hook (for tool observation capture)
  let post_tool = Request {
    id: Some(serde_json::json!(2)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "PostToolUse",
        "params": {
            "session_id": "test-session-123",
            "tool_name": "Read",
            "tool_input": {"file_path": "/some/file.rs"},
            "tool_output": "fn main() { println!(\"Hello\"); }",
            "cwd": cwd
        }
    }),
  };
  let post_response = router.handle(post_tool).await;
  assert!(
    post_response.error.is_none(),
    "PostToolUse hook should succeed: {:?}",
    post_response.error
  );

  // Test UserPromptSubmit hook
  let user_prompt = Request {
    id: Some(serde_json::json!(3)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "UserPromptSubmit",
        "params": {
            "session_id": "test-session-123",
            "content": "Help me debug the authentication module",
            "cwd": cwd
        }
    }),
  };
  let prompt_response = router.handle(user_prompt).await;
  assert!(
    prompt_response.error.is_none(),
    "UserPromptSubmit hook should succeed: {:?}",
    prompt_response.error
  );

  // Test Stop hook (should trigger extraction)
  let stop_hook = Request {
    id: Some(serde_json::json!(4)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "Stop",
        "params": {
            "session_id": "test-session-123",
            "stop_reason": "end_turn",
            "cwd": cwd
        }
    }),
  };
  let stop_response = router.handle(stop_hook).await;
  assert!(
    stop_response.error.is_none(),
    "Stop hook should succeed: {:?}",
    stop_response.error
  );

  // Test PreCompact hook
  let pre_compact = Request {
    id: Some(serde_json::json!(5)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "PreCompact",
        "params": {
            "session_id": "test-session-123",
            "cwd": cwd
        }
    }),
  };
  let compact_response = router.handle(pre_compact).await;
  assert!(
    compact_response.error.is_none(),
    "PreCompact hook should succeed: {:?}",
    compact_response.error
  );

  // Test SessionEnd hook (should trigger promotion)
  let session_end = Request {
    id: Some(serde_json::json!(6)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "SessionEnd",
        "params": {
            "session_id": "test-session-123",
            "cwd": cwd
        }
    }),
  };
  let end_response = router.handle(session_end).await;
  assert!(
    end_response.error.is_none(),
    "SessionEnd hook should succeed: {:?}",
    end_response.error
  );

  // Test Notification hook
  let notification = Request {
    id: Some(serde_json::json!(7)),
    method: "hook".to_string(),
    params: serde_json::json!({
        "event": "Notification",
        "params": {
            "session_id": "test-session-123",
            "message": "Build completed successfully",
            "cwd": cwd
        }
    }),
  };
  let notif_response = router.handle(notification).await;
  assert!(
    notif_response.error.is_none(),
    "Notification hook should succeed: {:?}",
    notif_response.error
  );

  // Clean up - stop all watchers
  router.registry().stop_all_watchers().await;
}
