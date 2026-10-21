//! `<data_dir>/model-cache.json`: each CLI's model list, keyed by CLI and version (TD §5.2, §7).
//! Safe to delete; a damaged file reads as empty.

use std::collections::BTreeMap;
use std::path::PathBuf;

use duohive_core::adapter::ModelList;
use duohive_core::cli_kind::CliKind;
use duohive_core::ports::StoreError;
use serde::{Deserialize, Serialize};

use crate::org_file::write_atomic;
use crate::paths::Paths;

const FILE: &str = "model-cache.json";

#[derive(Default, Serialize, Deserialize)]
struct Cache(BTreeMap<String, Entry>);

#[derive(Serialize, Deserialize)]
struct Entry {
    version: String,
    list: ModelList,
}

fn path(paths: &Paths) -> PathBuf {
    paths.data_dir.join(FILE)
}

fn load(paths: &Paths) -> Cache {
    std::fs::read_to_string(path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The cached list for `cli` at exactly `version`, or `None`.
pub fn read(paths: &Paths, cli: CliKind, version: &str) -> Option<ModelList> {
    load(paths)
        .0
        .remove(cli.binary())
        .filter(|e| e.version == version)
        .map(|e| e.list)
}

/// Stores `list` for `cli` at `version`, keeping the other CLI's entry.
pub fn write(
    paths: &Paths,
    cli: CliKind,
    version: &str,
    list: &ModelList,
) -> Result<(), StoreError> {
    let mut cache = load(paths);
    cache.0.insert(
        cli.binary().to_owned(),
        Entry {
            version: version.to_owned(),
            list: list.clone(),
        },
    );
    let text = serde_json::to_string_pretty(&cache)
        .map_err(|e| StoreError(format!("cannot write the model cache: {e}")))?;
    std::fs::create_dir_all(&paths.data_dir)
        .map_err(|e| StoreError(format!("cannot write the model cache: {e}")))?;
    write_atomic(&path(paths), text.as_bytes(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &std::path::Path) -> Paths {
        Paths {
            config_dir: dir.join("config"),
            data_dir: dir.join("data"),
            tmux_socket: "t".to_owned(),
        }
    }

    fn list(model: &str) -> ModelList {
        ModelList {
            models: vec![model.to_owned()],
            ..ModelList::default()
        }
    }

    #[test]
    fn cache_hit_per_cli_version() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        write(&p, CliKind::Claude, "2.1.0", &list("opus")).unwrap();
        write(&p, CliKind::Opencode, "2.1.1", &list("a/b")).unwrap();
        assert_eq!(read(&p, CliKind::Claude, "2.1.0"), Some(list("opus")));
        assert_eq!(read(&p, CliKind::Opencode, "2.1.1"), Some(list("a/b")));
    }

    #[test]
    fn cache_miss_on_new_version() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        write(&p, CliKind::Claude, "2.1.0", &list("opus")).unwrap();
        assert_eq!(read(&p, CliKind::Claude, "2.2.0"), None);
        assert_eq!(read(&p, CliKind::Opencode, "2.1.0"), None);
    }

    #[test]
    fn damaged_cache_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        std::fs::create_dir_all(&p.data_dir).unwrap();
        std::fs::write(path(&p), "{not json").unwrap();
        assert_eq!(read(&p, CliKind::Claude, "1"), None);
        write(&p, CliKind::Claude, "1", &list("x")).unwrap();
        assert_eq!(read(&p, CliKind::Claude, "1"), Some(list("x")));
    }
}
