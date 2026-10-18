//! duohive-store crate: file and database adapters behind the `Store` port.

pub mod closed;
pub mod db;
mod messages;
pub mod org_file;
pub mod paths;
pub mod settings_file;
mod sqlite_store;

pub use org_file::{load_org, mark_last_good, save_org};
pub use paths::{Paths, PathsError};
pub use settings_file::{load_settings, save_settings};
pub use sqlite_store::SqliteStore;
