//! Enforces the dependency rule: every crate depends inward on `duohive-core` only.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wild_err_arm,
    reason = "test helpers read fixed manifests"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn workspace_duohive_deps() -> BTreeMap<String, BTreeSet<String>> {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(&crates_dir).unwrap() {
        let manifest = entry.unwrap().path().join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let doc: toml::Table = fs::read_to_string(&manifest).unwrap().parse().unwrap();
        let name = doc["package"]["name"].as_str().unwrap().to_string();
        let deps = doc
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .map(|t| {
                t.keys()
                    .filter(|k| k.starts_with("duohive"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        out.insert(name, deps);
    }
    out
}

#[test]
fn crates_depend_inward_only() {
    let deps = workspace_duohive_deps();
    assert_eq!(deps["duohive-core"], set(&[]));
    for c in [
        "duohive-tmux",
        "duohive-cli",
        "duohive-store",
        "duohive-tui",
        "duohive-mcp",
    ] {
        assert_eq!(deps[c], set(&["duohive-core"]), "{c}");
    }
    assert_eq!(
        deps["duohive"],
        set(&[
            "duohive-core",
            "duohive-tmux",
            "duohive-cli",
            "duohive-store",
            "duohive-tui",
            "duohive-mcp"
        ])
    );
    assert_eq!(deps.len(), 7);
}
