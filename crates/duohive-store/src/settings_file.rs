//! `settings.toml`: personal settings, read and written whole.

use std::path::Path;

use duohive_core::ports::{OrgFileError, StoreError};
use duohive_core::settings::Settings;

use crate::org_file::{parse_error, read_text, write_atomic};

/// Loads the settings; a missing file gives the defaults.
pub fn load_settings(path: &Path) -> Result<Settings, OrgFileError> {
    let Some(text) = read_text(path)? else {
        return Ok(Settings::default());
    };
    toml::from_str(&text).map_err(|e| parse_error(&text, &e))
}

/// Saves the settings atomically.
pub fn save_settings(path: &Path, s: &Settings) -> Result<(), StoreError> {
    let text = toml::to_string(s).map_err(|e| StoreError(format!("cannot write settings: {e}")))?;
    write_atomic(path, text.as_bytes(), None)
}
