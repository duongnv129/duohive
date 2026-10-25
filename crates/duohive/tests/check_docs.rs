//! `scripts/check-docs.sh` against throwaway repositories: a good one passes, and each kind of
//! drift (a broken link, a missing changelog entry, a release target the README does not list)
//! fails with a message that names it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::Path;
use std::process::{Command, Output};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/check-docs.sh");

const WORKFLOW: &str = "jobs:\n  build:\n    strategy:\n      matrix:\n        include:\n          - target: aarch64-apple-darwin\n            os: macos-14\n          - target: x86_64-unknown-linux-gnu\n            os: ubuntu-22.04\n";

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository that passes: version 1.2.3, its changelog entry, a README that links an existing
/// file and names both release targets.
fn good_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(
        r,
        "Cargo.toml",
        "[workspace.package]\nversion = \"1.2.3\"\n\n[workspace.dependencies]\n",
    );
    write(
        r,
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n## [1.2.3] - 2026-01-01\n",
    );
    write(
        r,
        "README.md",
        "# Demo\n\nSee [the guide](docs/guide.md#start), [the site](https://example.com) and [top](#top).\n\n`aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`.\n",
    );
    write(r, "docs/guide.md", "# Guide\n");
    write(r, ".github/workflows/release-binaries.yml", WORKFLOW);
    dir
}

fn check(root: &Path) -> Output {
    Command::new("sh")
        .arg(SCRIPT)
        .env("CHECK_DOCS_ROOT", root)
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn check_docs_passes_a_consistent_repo() {
    let repo = good_repo();
    let out = check(repo.path());
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok (version 1.2.3)"));
}

#[test]
fn check_docs_names_a_broken_link() {
    let repo = good_repo();
    write(
        repo.path(),
        "README.md",
        "[gone](docs/missing.md)\n`aarch64-apple-darwin` `x86_64-unknown-linux-gnu`\n",
    );
    let out = check(repo.path());
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("broken link (docs/missing.md)"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn check_docs_ignores_urls_and_anchors() {
    let repo = good_repo();
    write(
        repo.path(),
        "README.md",
        "[a](https://example.com/x) [b](mailto:a@b.c) [c](#section) `aarch64-apple-darwin` `x86_64-unknown-linux-gnu`\n",
    );
    let out = check(repo.path());
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn check_docs_wants_a_changelog_entry_for_the_version() {
    let repo = good_repo();
    write(
        repo.path(),
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n## [1.2.2] - 2026-01-01\n",
    );
    let out = check(repo.path());
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("no '## [1.2.3]' entry"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn check_docs_wants_every_release_target_in_the_readme() {
    let repo = good_repo();
    write(
        repo.path(),
        "README.md",
        "# Demo\n\nOnly `aarch64-apple-darwin` here.\n",
    );
    let out = check(repo.path());
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("does not list the release target x86_64-unknown-linux-gnu"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn check_docs_passes_on_this_repository() {
    // The real docs: the same check CI runs, so a broken link fails a local `cargo test` too.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new("sh")
        .arg(SCRIPT)
        .env("CHECK_DOCS_ROOT", root.canonicalize().unwrap())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}
