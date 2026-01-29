# Actor-Based Daemon Architecture

The daemon uses an **actor model with message passing** for concurrency. Each logical component is a long-running task with its own event loop, communicating via `mpsc` channels. State is owned, not shared.

## Why Actors?

The previous shared-state model (`Arc<Mutex<...>>` everywhere) led to:
- God objects doing too much
- Two-phase initialization (create → mutate via setters)
- Blocking async (`spawn_blocking` wrapping `block_on`)
- Triple indirection (`Arc<Mutex<Option<Arc<T>>>>`)

The actor model eliminates these by giving each component clear ownership and explicit message boundaries.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Daemon (Supervisor)                      │
│  - Owns startup/shutdown lifecycle                               │
│  - Spawns and supervises all actors                              │
│  - Holds CancellationToken for graceful shutdown                 │
└─────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Core Actor System                           │
├─────────────────────────────────┬───────────────────────────────┤
│         Server Actor            │       Scheduler Actor         │
│         (IPC Listener)          │  (Decay/Cleanup/IdleShutdown) │
└────────────────┬────────────────┴───────────────────────────────┘
                 │
                 ▼
┌─────────────────┐         ┌─────────────────────────────────────┐
│Connection Task  │────────▶│         ProjectRouter               │
└─────────────────┘         │  - Routes requests to ProjectActors │
                            │  - Spawns ProjectActors on demand   │
                            └─────────────────┬───────────────────┘
                                              │
                                              ▼
                            ┌─────────────────────────────────────┐
                            │         ProjectActor                 │
                            │  - Owns ProjectDb                    │
                            │  - Owns IndexerActor handle          │
                            │  - Owns WatcherTask handle           │
                            └─────────────────┬───────────────────┘
                                              │
                              ┌───────────────┴───────────────┐
                              ▼                               ▼
                    ┌─────────────────┐             ┌─────────────────┐
                    │  IndexerActor   │◀────────────│   WatcherTask   │
                    │  - Batch embed  │   IndexJob  │  - File events  │
                    │  - Update DB    │             │  - Debouncing   │
                    └─────────────────┘             └─────────────────┘
```

## Core Components

### Daemon
The supervisor that owns the entire lifecycle. Creates a master `CancellationToken` that propagates to all children for graceful shutdown.

### ProjectRouter
Routes requests to per-project actors, spawning them on demand. Uses `DashMap` for lock-free concurrent access. Each project gets its own actor with isolated state.

### ProjectActor
The per-project actor that owns:
- **ProjectDb** (via Arc for sharing with Indexer)
- **IndexerHandle** for sending index jobs
- **WatcherTask** lifecycle (start/stop)

Receives messages via channel, dispatches to handlers. Handlers can send multiple responses for streaming.

### IndexerActor
Handles all file indexing operations:
- Single file indexing (from watcher or manual)
- Batch indexing (startup scan, reindex)
- Delete/rename operations

Processes jobs from a queue, sends progress updates via optional channels.

### WatcherTask
Watches filesystem for changes using `notify`. Debounces rapid changes before sending `IndexJob` messages to the indexer. Bridges sync notify callbacks to async via `mpsc::blocking_send`.

### Scheduler
Single actor for all periodic background tasks:
- Memory decay
- Session cleanup
- Log rotation
- Idle shutdown (background mode only)

## Message Passing

All requests include an `mpsc::Sender` for responses, enabling streaming:

- **Request** → Actor receives message with reply channel
- **Progress** → Actor sends intermediate updates (not final)
- **Done/Error** → Actor sends final response, receiver breaks loop

Response channels are `mpsc` (not oneshot) to support multiple messages per request.

## Key Design Principles

1. **No two-phase init** — Everything created with full state, no `set_*` methods
2. **Owned state** — No `Arc<Mutex<...>>` inside actors, only at boundaries
3. **Child tokens** — Each spawned actor gets a child `CancellationToken`
4. **Pure async** — No `spawn_blocking` or `block_on` in actor code
5. **Idempotent routing** — `get_or_create` handles race conditions gracefully
