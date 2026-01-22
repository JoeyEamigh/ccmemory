//! CLI command implementations

mod admin;
mod agent;
mod daemon;
mod hook;
mod index;
mod memory;
mod migrate;
mod search;
mod update;
mod watch;

pub use admin::{cmd_archive, cmd_config_init, cmd_config_reset, cmd_config_show, cmd_health, cmd_stats};
pub use agent::{cmd_agent, cmd_tui};
pub use daemon::cmd_daemon;
pub use hook::cmd_hook;
pub use index::cmd_index;
pub use memory::{cmd_delete, cmd_export, cmd_show};
pub use migrate::cmd_migrate;
pub use search::{cmd_search, cmd_search_code, cmd_search_docs};
pub use update::cmd_update;
pub use watch::cmd_watch;
