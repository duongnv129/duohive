//! duohive-store crate: file and database adapters behind the `Store` port.

mod cards;
mod chat;
pub mod closed;
pub mod db;
mod messages;
pub mod model_cache;
pub mod org_file;
pub mod paths;
pub mod settings_file;
mod sqlite_store;

pub use org_file::{backup_stamp, load_org, mark_last_good, replace_org, save_org};
pub use paths::{Paths, PathsError};
pub use settings_file::{load_settings, save_settings};
pub use sqlite_store::SqliteStore;
