//! Finding an installed CLI: `<binary> --version` with a time limit (Sn.1).

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use duohive_core::adapter::CliPresence;
use duohive_core::cli_kind::CliKind;

/// How long `--version` may take before DuoHive stops waiting.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Runs `<binary> --version` (2 s limit) and reads the version.
///
/// A binary that cannot be started is `Missing`. One that starts but times
/// out or prints something the Sn.1 rule does not recognise is still `Found`,
/// with an empty version (no untested-version notice), so an installed CLI is
/// never reported as missing.
pub fn probe_version(kind: CliKind) -> CliPresence {
    probe(kind, kind.binary().as_ref(), PROBE_TIMEOUT)
}

/// The version in `<binary> --version` output, by the Sn.1 rule:
/// Claude Code `^(\d+)\.(\d+)\.(\d+) \(Claude Code\)$` (S1.1),
/// OpenCode `^(\d+)\.(\d+)\.(\d+)$` (S2.1).
pub fn parse_cli_version(kind: CliKind, out: &str) -> Option<String> {
    let line = out.trim_end_matches(['\n', '\r']);
    let version = match kind {
        CliKind::Claude => line.strip_suffix(" (Claude Code)")?,
        CliKind::Opencode => line,
    };
    let parts: Vec<&str> = version.split('.').collect();
    let numeric = |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    (parts.len() == 3 && parts.iter().all(numeric)).then(|| version.to_owned())
}

pub(crate) fn probe(kind: CliKind, program: &OsStr, timeout: Duration) -> CliPresence {
    let Ok(mut child) = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return CliPresence::Missing;
    };
    // Read on a helper thread so that a child (or a grandchild holding its
    // stdout) that never finishes cannot hold DuoHive past the timeout.
    let (tx, rx) = mpsc::channel();
    if let Some(mut stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut out = String::new();
            // Unreadable output leaves `out` empty, which parses to no version.
            let _ = stdout.read_to_string(&mut out);
            let _ = tx.send(out);
        });
    }
    let out = rx.recv_timeout(timeout).unwrap_or_default();
    if !matches!(child.try_wait(), Ok(Some(_))) {
        // Best effort: the child may exit between the check and the kill.
        let _ = child.kill();
    }
    let _ = child.wait();
    CliPresence::Found {
        version: parse_cli_version(kind, &out).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use duohive_core::adapter::CliPresence;
    use duohive_core::cli_kind::CliKind;

    use super::*;

    #[test]
    fn version_output_parses() {
        assert_eq!(
            parse_cli_version(CliKind::Claude, "2.1.288 (Claude Code)\n").as_deref(),
            Some("2.1.288")
        );
        for garbage in [
            "",
            "garbage",
            "2.1 (Claude Code)",
            "2.1.x (Claude Code)",
            "2.1.288",
            "v2.1.288 (Claude Code)",
            "2.1.288 (Claude Code) extra",
        ] {
            assert_eq!(
                parse_cli_version(CliKind::Claude, garbage),
                None,
                "{garbage}"
            );
        }
    }

    fn script(dir: &Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("fake-cli");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn probe_reads_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "echo '2.1.288 (Claude Code)'");
        assert_eq!(
            probe(CliKind::Claude, exe.as_os_str(), PROBE_TIMEOUT),
            CliPresence::Found {
                version: "2.1.288".into()
            }
        );
    }

    #[test]
    fn probe_reports_missing_binary() {
        assert_eq!(
            probe(
                CliKind::Claude,
                "/nonexistent/duohive-no-such-cli".as_ref(),
                PROBE_TIMEOUT
            ),
            CliPresence::Missing
        );
    }

    #[test]
    fn probe_found_with_unknown_version_when_output_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "echo something else; exit 3");
        assert_eq!(
            probe(CliKind::Claude, exe.as_os_str(), PROBE_TIMEOUT),
            CliPresence::Found {
                version: String::new()
            }
        );
    }

    #[test]
    fn probe_gives_up_at_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "exec sleep 10");
        let started = Instant::now();
        let got = probe(CliKind::Claude, exe.as_os_str(), Duration::from_millis(200));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            got,
            CliPresence::Found {
                version: String::new()
            }
        );
    }

    #[test]
    fn probe_timeout_is_two_seconds() {
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(2));
    }
}
