//! IPC server for the actor-based daemon architecture.
//!
//! The server accepts connections on a Unix socket and routes requests
//! to `ProjectActor` instances via the `ProjectRouter`. It supports
//! response streaming for long-running operations.
//!
//! # Design Principles
//!
//! - **No two-phase initialization**: All dependencies are passed to `Server::new()`
//! - **No `set_*` methods**: Configuration is immutable after construction
//! - **Actor-based routing**: Requests go through `ProjectRouter` → `ProjectActor`
//! - **Streaming support**: Response channel supports multiple messages per request
//!
//! # Example
//!
//! ```ignore
//! let config = ServerConfig {
//!     socket_path: PathBuf::from("/tmp/ccengram.sock"),
//!     router: Arc::new(project_router),
//!     activity: Arc::new(activity_tracker),
//!     sessions: Arc::new(session_tracker),
//!     hooks_config: HooksConfig::default(),
//! };
//!
//! let server = Server::new(config);
//! server.run(cancel_token).await?;
//! ```

use std::{
  path::PathBuf,
  sync::{Arc, atomic::AtomicU64},
};

use futures::{SinkExt, StreamExt};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::{
  codec::{Framed, LinesCodec},
  sync::CancellationToken,
};
use tracing::{debug, error, info, trace, warn};

use crate::{
  actor::{
    ProjectRouter,
    lifecycle::{
      activity::KeepAlive,
      session::{SessionId, SessionTracker},
    },
    message::{ProjectActorPayload, ProjectActorResponse},
  },
  ipc::{IpcError, Request, RequestData, Response},
};

// ============================================================================
// Server Configuration
// ============================================================================

/// Configuration for the IPC server.
///
/// Contains all dependencies the server needs, eliminating the need
/// for two-phase initialization with `set_*` methods. All fields are
/// immutable after construction.
pub struct ServerConfig {
  /// Path to the Unix socket for IPC
  pub socket_path: PathBuf,

  /// Project router for dispatching requests to ProjectActors
  pub router: Arc<ProjectRouter>,

  /// Activity tracker for idle detection
  pub activity: Arc<KeepAlive>,

  /// Session tracker for lifecycle management
  pub sessions: Arc<SessionTracker>,
}

// ============================================================================
// Server
// ============================================================================

/// IPC server that accepts connections and routes requests to ProjectActors.
///
/// The server listens on a Unix socket and spawns a task for each connection.
/// Requests are routed to `ProjectActor` instances via the `ProjectRouter`,
/// which spawns actors on demand.
///
/// # Lifecycle
///
/// 1. `Server::new()` creates the server with all dependencies
/// 2. `Server::run()` binds the socket and accepts connections
/// 3. Each connection spawns a `handle_connection` task
/// 4. On cancellation, cleanup and exit
///
/// # Threading Model
///
/// - Server accepts connections on main task
/// - Each connection runs in its own spawned task
/// - All tasks share the `ProjectRouter` via `Arc`
pub struct Server {
  config: ServerConfig,
  /// Total requests handled across all connections (for metrics)
  request_count: AtomicU64,
}

impl Server {
  /// Create a new server with the given configuration.
  ///
  /// All dependencies must be provided upfront - there are no `set_*` methods.
  pub fn new(config: ServerConfig) -> Self {
    Self {
      config,
      request_count: AtomicU64::new(0),
    }
  }

  /// Run the server until the cancellation token is triggered.
  ///
  /// This method:
  /// 1. Removes any stale socket file
  /// 2. Creates the socket parent directory if needed
  /// 3. Binds to the socket and accepts connections
  /// 4. Spawns a task for each connection
  /// 5. Cleans up on shutdown
  pub async fn run(&self, cancel: CancellationToken) -> Result<(), IpcError> {
    // Remove stale socket file
    if self.config.socket_path.exists() {
      tokio::fs::remove_file(&self.config.socket_path).await?;
    }

    // Create parent directory if needed
    if let Some(parent) = self.config.socket_path.parent() {
      tokio::fs::create_dir_all(parent).await?;
    }

    let listener = UnixListener::bind(&self.config.socket_path)?;
    info!("Server listening on {:?}", self.config.socket_path);

    loop {
      tokio::select! {
        biased;

        _ = cancel.cancelled() => {
            info!("Server shutting down (cancelled)");
            break;
        }

        result = listener.accept() => {
          match result {
            Ok((stream, _)) => {
              // Touch activity tracker on any connection
              self.config.activity.touch();

              let router = Arc::clone(&self.config.router);
              let activity = Arc::clone(&self.config.activity);
              let sessions = Arc::clone(&self.config.sessions);
              let request_count = &self.request_count;

              // Increment connection count (we track requests inside handle_connection)
              let _ = request_count;

              tokio::spawn(handle_connection(stream, router, activity, sessions));
            }
            Err(e) => {
              error!("Accept error: {}", e);
            }
          }
        }
      }
    }

    // Cleanup socket file
    if self.config.socket_path.exists() {
      tokio::fs::remove_file(&self.config.socket_path).await?;
    }

    Ok(())
  }
}

// ============================================================================
// Connection Handler
// ============================================================================

/// Handle a single client connection.
///
/// This function:
/// 1. Reads newline-delimited JSON requests from the client
/// 2. Routes each request to the appropriate ProjectActor via the router
/// 3. Streams responses back to the client until a final response
/// 4. Touches the activity tracker on each request
///
/// # Protocol
///
/// - Requests: JSON objects, one per line
/// - Responses: JSON objects, one per line (may be multiple for streaming)
/// - A response with `is_final()` == true ends the request
///
/// # Error Handling
///
/// - Parse errors return an error response but don't close the connection
/// - Actor errors return an error response but don't close the connection
/// - IO errors close the connection
async fn handle_connection(
  stream: UnixStream,
  router: Arc<ProjectRouter>,
  activity: Arc<KeepAlive>,
  sessions: Arc<SessionTracker>,
) -> Result<(), IpcError> {
  debug!("Client connected");
  let framed = Framed::new(stream, LinesCodec::new());
  let (mut sink, mut stream) = framed.split();
  let mut request_count = 0u64;

  while let Some(result) = stream.next().await {
    let line = match result {
      Ok(l) => l,
      Err(e) => {
        warn!(error = %e, "Error reading from client");
        break;
      }
    };

    // Touch activity tracker on every request
    activity.touch();
    request_count += 1;

    let trimmed = line.trim();
    if trimmed.is_empty() {
      continue;
    }

    // Parse request
    let request: Request = match serde_json::from_str(trimmed) {
      Ok(r) => r,
      Err(e) => {
        warn!("Invalid request JSON: {}", e);
        let response = Response::rpc_error("unknown", -32700, format!("Parse error: {}", e));
        let json = serde_json::to_string(&response)?;
        sink.send(json).await?;
        continue;
      }
    };

    let start = std::time::Instant::now();
    trace!(method = ?request.data, id = %request.id, cwd = %request.cwd, "Processing request");

    // Track sessions for lifecycle management
    if let RequestData::Hook(ref params) = request.data
      && let Some(ref session_id) = params.session_id
    {
      let sid = SessionId::from(session_id.as_str());
      match params.hook_name.as_str() {
        "SessionStart" => {
          sessions.register(sid).await;
        }
        "SessionEnd" => {
          sessions.unregister(&sid).await;
        }
        _ => {
          // Touch session on any other hook to keep it alive
          sessions.touch(&sid).await;
        }
      }
    }

    // Get or create project actor for this request's cwd
    let project_path = PathBuf::from(&request.cwd);
    let handle = match router.get_or_create(&project_path).await {
      Ok(h) => h,
      Err(e) => {
        let response = Response::rpc_error(&request.id, -32000, format!("Failed to get project: {}", e));
        let json = serde_json::to_string(&response)?;
        sink.send(json).await?;
        continue;
      }
    };

    // Convert IPC request to actor message payload
    let payload = ProjectActorPayload::Request(request.data);

    // Send request to project actor and get response channel
    let mut reply_rx = match handle.send(request.id.clone(), payload).await {
      Ok(rx) => rx,
      Err(e) => {
        let response = Response::rpc_error(&request.id, -32000, format!("Failed to send to actor: {}", e));
        let json = serde_json::to_string(&response)?;
        sink.send(json).await?;
        continue;
      }
    };

    // Stream responses until we get a final one
    while let Some(response) = reply_rx.recv().await {
      let ipc_response = convert_actor_response(&request.id, response.clone());
      let json = serde_json::to_string(&ipc_response)?;
      sink.send(json).await?;

      if response.is_final() {
        break;
      }
    }

    let elapsed = start.elapsed();
    debug!(
        id = %request.id,
        elapsed_ms = elapsed.as_millis() as u64,
        "Request completed"
    );
  }

  debug!(requests_handled = request_count, "Client disconnected");
  Ok(())
}

/// Convert an actor response to an IPC response.
///
/// This handles the different response types:
/// - `Progress` → stream chunk with status info
/// - `Stream` → stream chunk with data
/// - `Done` → success response
/// - `Error` → error response
fn convert_actor_response(request_id: &str, response: ProjectActorResponse) -> Response {
  match response {
    ProjectActorResponse::Progress { message, percent } => Response::stream_progress(request_id, message, percent),
    ProjectActorResponse::Stream { data } => Response::stream_chunk(request_id, data),
    ProjectActorResponse::Done(data) => Response::success(request_id, data),
    ProjectActorResponse::Error { code, message } => Response::rpc_error(request_id, code, message),
  }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ipc::{ResponseData, system::SystemResponse};

  #[test]
  fn test_convert_actor_response_done() {
    let response = ProjectActorResponse::Done(ResponseData::System(SystemResponse::Ping("pong".to_string())));
    let ipc = convert_actor_response("test-1", response);

    assert_eq!(ipc.id, "test-1");
    match ipc.scenario {
      crate::ipc::ResponseScenario::Result { data } => {
        assert!(matches!(data, ResponseData::System(SystemResponse::Ping(_))));
      }
      _ => panic!("Expected Result scenario"),
    }
  }

  #[test]
  fn test_convert_actor_response_error() {
    let response = ProjectActorResponse::Error {
      code: -32000,
      message: "test error".to_string(),
    };
    let ipc = convert_actor_response("test-2", response);

    assert_eq!(ipc.id, "test-2");
    match ipc.scenario {
      crate::ipc::ResponseScenario::Error { error } => {
        assert!(matches!(error, IpcError::Rpc { code: -32000, .. }));
      }
      _ => panic!("Expected Error scenario"),
    }
  }

  #[test]
  fn test_convert_actor_response_stream() {
    let response = ProjectActorResponse::Stream {
      data: ResponseData::System(SystemResponse::Ping("streaming".to_string())),
    };
    let ipc = convert_actor_response("test-3", response);

    assert_eq!(ipc.id, "test-3");
    match ipc.scenario {
      crate::ipc::ResponseScenario::Stream { chunk, done, .. } => {
        assert!(chunk.is_some());
        assert!(!done);
      }
      _ => panic!("Expected Stream scenario"),
    }
  }

  #[test]
  fn test_convert_actor_response_progress() {
    let response = ProjectActorResponse::Progress {
      message: "Indexing files".to_string(),
      percent: Some(50),
    };
    let ipc = convert_actor_response("test-4", response);

    assert_eq!(ipc.id, "test-4");
    match ipc.scenario {
      crate::ipc::ResponseScenario::Stream { chunk, progress, done } => {
        assert!(chunk.is_none());
        assert!(!done);
        let p = progress.expect("Expected progress");
        assert_eq!(p.message, "Indexing files");
        assert_eq!(p.percent, Some(50));
      }
      _ => panic!("Expected Stream scenario"),
    }
  }
}
