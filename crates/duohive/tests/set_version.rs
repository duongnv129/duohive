//! `scripts/set-version.sh` against a copy of the workspace manifest.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::Path;
use std::process::{Command, Output};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/set-version.sh");
const MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");

fn run(root: &Path, version: &str) -> Output {
    Command::new("sh")
        .arg(SCRIPT)
        .arg(version)
        .env("SET_VERSION_ROOT", root)
        .output()
        .unwrap()
}

fn copy_manifest() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(MANIFEST, dir.path().join("Cargo.toml")).unwrap();
    dir
}

#[test]
fn set_version_changes_every_pin() {
    let dir = copy_manifest();
    let out = run(dir.path(), "1.0.0-rc.1");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    // The package version and the six path pins all moved together.
    assert_eq!(text.matches("1.0.0-rc.1").count(), 7, "{text}");
    assert!(text.contains("version = \"1.0.0-rc.1\"\nedition"));
    assert!(!text.contains("\"0.1.0\""), "an old pin is left: {text}");
    // Other dependencies are untouched.
    assert!(text.contains("tokio = \"1.53\""));
    assert!(text.contains("serde_json = \"1\""));
}

#[test]
fn set_version_refuses_a_non_semver_argument() {
    for bad in ["1.0", "latest", "1.0.0; rm -rf /", "v1.0.0"] {
        let dir = copy_manifest();
        let before = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        let out = run(dir.path(), bad);
        assert_eq!(out.status.code(), Some(2), "{bad}: {out:?}");
        let after = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert_eq!(before, after, "{bad} changed the manifest");
    }
}
