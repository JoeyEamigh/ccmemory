//! MCP (Model Context Protocol) server for Claude Code integration

use anyhow::{Context, Result};
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

// MCP protocol response structures
#[derive(Serialize)]
struct InitializeResult {
  #[serde(rename = "protocolVersion")]
  protocol_version: &'static str,
  capabilities: McpCapabilities,
  #[serde(rename = "serverInfo")]
  server_info: McpServerInfo,
}

#[derive(Serialize)]
struct McpCapabilities {
  tools: McpToolsCapability,
}

#[derive(Serialize)]
struct McpToolsCapability {}

#[derive(Serialize)]
struct McpServerInfo {
  name: &'static str,
  version: &'static str,
}

#[derive(Serialize)]
struct ToolsListResult {
  tools: serde_json::Value,
}

#[derive(Serialize)]
struct McpContent {
  #[serde(rename = "type")]
  content_type: &'static str,
  text: String,
}

#[derive(Serialize)]
struct McpToolResult {
  content: Vec<McpContent>,
  #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
  is_error: Option<bool>,
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
        serde_json::to_value(InitializeResult {
          protocol_version: "2024-11-05",
          capabilities: McpCapabilities {
            tools: McpToolsCapability {},
          },
          server_info: McpServerInfo {
            name: "ccengram",
            version: env!("CARGO_PKG_VERSION"),
          },
        })
        .unwrap_or_default(),
      ),
      "notifications/initialized" => {
        // No response needed for notification
        continue;
      }
      "tools/list" => mcp_success(
        mcp_request.id,
        serde_json::to_value(ToolsListResult {
          tools: crate::tools::get_tool_definitions_for_cwd().await,
        })
        .unwrap_or_default(),
      ),
      "tools/call" => {
        // Extract tool name and arguments
        let tool_name = mcp_request.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = mcp_request
          .params
          .get("arguments")
          .cloned()
          .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        // Add cwd to arguments for project context
        let mut args = arguments;
        if let Some(obj) = args.as_object_mut()
          && !obj.contains_key("cwd")
          && let Ok(cwd) = std::env::current_dir()
        {
          obj.insert(
            "cwd".to_string(),
            serde_json::Value::String(cwd.to_string_lossy().to_string()),
          );
        }

        // Dispatch tool call to daemon
        match dispatch_tool_call(tool_name, args).await {
          Ok(result) => {
            // Format the result for LLM consumption, falling back to JSON if no formatter
            let text = crate::format::format_tool_result(tool_name, &result)
              .unwrap_or_else(|| serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()));
            mcp_success(
              mcp_request.id,
              serde_json::to_value(McpToolResult {
                content: vec![McpContent {
                  content_type: "text",
                  text,
                }],
                is_error: None,
              })
              .unwrap_or_default(),
            )
          }
          Err(e) => mcp_success(
            mcp_request.id,
            serde_json::to_value(McpToolResult {
              content: vec![McpContent {
                content_type: "text",
                text: format!("Error: {}", e),
              }],
              is_error: Some(true),
            })
            .unwrap_or_default(),
          ),
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

/// Dispatch a tool call to the daemon using typed IPC
async fn dispatch_tool_call(tool_name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
  use ccengram::ipc::{
    code::*,
    docs::*,
    memory::*,
    project::*,
    relationship::*,
    search::{ContextParams, ExploreParams},
    system::*,
    watch::*,
  };

  let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let client = ccengram::Daemon::connect_or_start(cwd)
    .await
    .context("Failed to connect to daemon")?;

  // Macro to reduce boilerplate: deserialize args, call client, serialize result
  macro_rules! call {
    ($params:ty) => {{
      let params: $params =
        serde_json::from_value(args).context(concat!("Invalid params for ", stringify!($params)))?;
      let result = client.call(params).await?;
      serde_json::to_value(result).context("Failed to serialize response")
    }};
  }

  match tool_name {
    // Unified exploration tools
    "explore" => call!(ExploreParams),
    "context" => call!(ContextParams),

    // Memory tools
    "memory_search" => call!(MemorySearchParams),
    "memory_get" => call!(MemoryGetParams),
    "memory_list" => call!(MemoryListParams),
    "memory_add" => call!(MemoryAddParams),
    "memory_reinforce" => call!(MemoryReinforceParams),
    "memory_deemphasize" => call!(MemoryDeemphasizeParams),
    "memory_delete" => call!(MemoryDeleteParams),
    "memory_supersede" => call!(MemorySupersedeParams),
    "memory_timeline" => call!(MemoryTimelineParams),
    "memory_related" => call!(MemoryRelatedParams),

    // Code tools
    "code_search" => call!(CodeSearchParams),
    "code_context" => call!(CodeContextParams),
    "code_index" => call!(CodeIndexParams),
    "code_list" => call!(CodeListParams),
    "code_stats" => call!(CodeStatsParams),
    "code_memories" => call!(CodeMemoriesParams),
    "code_callers" => call!(CodeCallersParams),
    "code_callees" => call!(CodeCalleesParams),
    "code_related" => call!(CodeRelatedParams),
    "code_context_full" => call!(CodeContextFullParams),

    // Watch tools
    "watch_start" => call!(WatchStartParams),
    "watch_stop" => call!(WatchStopParams),
    "watch_status" => call!(WatchStatusParams),

    // Document tools
    "docs_search" => call!(DocsSearchParams),
    "doc_context" => call!(DocContextParams),
    "docs_ingest" => call!(DocsIngestParams),

    // Relationship tools
    "relationship_add" => call!(RelationshipAddParams),
    "relationship_list" => call!(RelationshipListParams),
    "relationship_delete" => call!(RelationshipDeleteParams),
    "relationship_related" => call!(RelationshipRelatedParams),

    // Project tools
    "project_list" => call!(ProjectListParams),
    "project_info" => call!(ProjectInfoParams),
    "project_clean" => call!(ProjectCleanParams),
    "project_clean_all" => call!(ProjectCleanAllParams),

    // System tools
    "project_stats" => call!(ProjectStatsParams),
    "health_check" => call!(HealthCheckParams),

    _ => anyhow::bail!("Unknown tool: {}", tool_name),
  }
}
