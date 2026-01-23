//! MCP (Model Context Protocol) server for Claude Code integration

use anyhow::{Context, Result};
use daemon::{Request, connect_or_start};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[derive(Debug, Deserialize)]
struct McpRequest {
  #[serde(rename = "jsonrpc")]
  _jsonrpc: String,
  id: Option<serde_json::Value>,
  method: String,
  #[serde(default)]
  params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct McpResponse {
  jsonrpc: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  id: Option<serde_json::Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  result: Option<serde_json::Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
  code: i32,
  message: String,
}

fn mcp_success(id: Option<serde_json::Value>, result: serde_json::Value) -> McpResponse {
  McpResponse {
    jsonrpc: "2.0",
    id,
    result: Some(result),
    error: None,
  }
}

fn mcp_error(id: Option<serde_json::Value>, code: i32, message: &str) -> McpResponse {
  McpResponse {
    jsonrpc: "2.0",
    id,
    result: None,
    error: Some(McpError {
      code,
      message: message.to_string(),
    }),
  }
}

/// MCP stdio server - implements the Model Context Protocol for Claude Code
pub async fn cmd_mcp() -> Result<()> {
  // Tool definitions are loaded from cli::tools and filtered based on config

  // Use async IO for proper non-blocking behavior with MCP
  let stdin = tokio::io::stdin();
  let mut stdout = tokio::io::stdout();
  let reader = tokio::io::BufReader::new(stdin);
  let mut lines = reader.lines();

  // Process MCP requests
  while let Some(line) = lines.next_line().await.context("Failed to read line from stdin")? {
    if line.trim().is_empty() {
      continue;
    }

    let mcp_request: McpRequest = match serde_json::from_str(&line) {
      Ok(r) => r,
      Err(e) => {
        let response = mcp_error(None, -32700, &format!("Parse error: {}", e));
        let out = serde_json::to_string(&response)? + "\n";
        stdout.write_all(out.as_bytes()).await?;
        stdout.flush().await?;
        continue;
      }
    };

    let response = match mcp_request.method.as_str() {
      // MCP protocol methods
      "initialize" => mcp_success(
        mcp_request.id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "ccengram",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
      ),
      "notifications/initialized" => {
        // No response needed for notification
        continue;
      }
      "tools/list" => mcp_success(
        mcp_request.id,
        serde_json::json!({
            "tools": cli::get_tool_definitions_for_cwd()
        }),
      ),
      "tools/call" => {
        // Extract tool name and arguments
        let tool_name = mcp_request.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = mcp_request
          .params
          .get("arguments")
          .cloned()
          .unwrap_or(serde_json::json!({}));

        // Add cwd to arguments for project context
        let mut args = arguments;
        if let Some(obj) = args.as_object_mut()
          && !obj.contains_key("cwd")
          && let Ok(cwd) = std::env::current_dir()
        {
          obj.insert("cwd".to_string(), serde_json::json!(cwd.to_string_lossy()));
        }

        // Connect to daemon (auto-starts if not running)
        match connect_or_start().await {
          Ok(mut client) => {
            let request = Request {
              id: Some(serde_json::json!(1)),
              method: tool_name.to_string(),
              params: args,
            };

            match client.request(request).await {
              Ok(daemon_response) => {
                if let Some(err) = daemon_response.error {
                  // Return error as text content (MCP style)
                  mcp_success(
                    mcp_request.id,
                    serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {}", err.message)
                        }],
                        "isError": true
                    }),
                  )
                } else if let Some(result) = daemon_response.result {
                  // Format result as text content
                  let text = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                  mcp_success(
                    mcp_request.id,
                    serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": text
                        }]
                    }),
                  )
                } else {
                  mcp_success(
                    mcp_request.id,
                    serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": "Success"
                        }]
                    }),
                  )
                }
              }
              Err(e) => mcp_error(mcp_request.id, -32000, &format!("Daemon error: {}", e)),
            }
          }
          Err(e) => mcp_error(mcp_request.id, -32000, &format!("Failed to start daemon: {}", e)),
        }
      }
      // Unknown method
      _ => mcp_error(
        mcp_request.id,
        -32601,
        &format!("Method not found: {}", mcp_request.method),
      ),
    };

    let out = serde_json::to_string(&response)? + "\n";
    stdout.write_all(out.as_bytes()).await?;
    stdout.flush().await?;
  }

  Ok(())
}
