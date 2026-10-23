//! Finding an installed CLI: `<binary> --version` with a time limit (Sn.1).

use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use duohive_core::adapter::{CliPresence, ModelListError};
use duohive_core::cli_kind::CliKind;

/// How long a model listing may take (TD §5.2).
pub const LISTING_TIMEOUT: Duration = Duration::from_secs(10);

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
        // Its own process group, so a timeout can stop everything it started.
        .process_group(0)
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
    let out = rx.recv_timeout(timeout).unwrap_or_else(|_| {
        // Still running, or a leftover process holds stdout: stop the whole
        // group, which closes the pipe, and keep what was already printed.
        kill_group(child.id());
        rx.recv_timeout(REAP_GRACE).unwrap_or_default()
    });
    if !matches!(child.try_wait(), Ok(Some(_))) {
        // Best effort: the child may exit between the check and the kill.
        let _ = child.kill();
    }
    let _ = child.wait();
    CliPresence::Found {
        version: parse_cli_version(kind, &out).unwrap_or_default(),
    }
}

/// Runs `<program> <args>` and returns its stdout, giving up after `timeout`: the whole process
/// group is killed, so a hung listing never holds DuoHive. Stdout is read on a helper thread, so
/// a long output cannot fill the pipe and block the child. Used for the CLIs' read-only
/// listings; nothing here writes any CLI configuration.
pub(crate) fn run_listing(
    program: &OsStr,
    args: &[&str],
    timeout: Duration,
) -> Result<String, ModelListError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| ModelListError::Io(e.to_string()))?;
    let (tx, rx) = mpsc::channel();
    if let Some(mut stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut out = String::new();
            let read = stdout.read_to_string(&mut out).map(|_| out);
            let _ = tx.send(read);
        });
    }
    let result = match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(ModelListError::Io(e.to_string())),
        Err(_) => {
            kill_group(child.id());
            Err(ModelListError::Timeout)
        }
    };
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
    }
    let status = child.wait();
    match (result, status) {
        (Ok(out), Ok(st)) if !st.success() && out.trim().is_empty() => {
            Err(ModelListError::Io(format!("exited with {st}")))
        }
        (result, _) => result,
    }
}

/// How long to wait for the output once the process group is killed.
const REAP_GRACE: Duration = Duration::from_millis(500);

/// `kill -s KILL -- -<pgid>`: the probe's whole process group (its leader's pid).
/// Best effort; the group may already be gone.
fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .args(["-s", "KILL", "--", &format!("-{pgid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
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
    fn list_models_kills_a_hung_child() {
        let dir = tempfile::tempdir().unwrap();
        let program = script(dir.path(), "sleep 30");
        let start = Instant::now();
        let result = run_listing(program.as_os_str(), &[], Duration::from_millis(200));
        assert_eq!(result, Err(ModelListError::Timeout));
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn run_listing_returns_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let program = script(dir.path(), "printf 'a/b\\nc/d\\n'");
        let out = run_listing(program.as_os_str(), &[], LOADED_TIMEOUT).unwrap();
        assert_eq!(out, "a/b\nc/d\n");
    }

    #[test]
    fn run_listing_reports_a_missing_program() {
        let result = run_listing("/no/such/duohive-cli".as_ref(), &[], LOADED_TIMEOUT);
        assert!(matches!(result, Err(ModelListError::Io(_))));
    }

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

    /// Generous probe limit for tests that must read a real fake binary's
    /// output: a freshly written script can take seconds to start when the
    /// whole workspace's tests run in parallel. The production limit
    /// (`PROBE_TIMEOUT`, 2 s) is pinned by `probe_timeout_is_two_seconds`, and
    /// the timeout path is covered with a deliberately hanging binary.
    const LOADED_TIMEOUT: Duration = Duration::from_secs(30);

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
            probe(CliKind::Claude, exe.as_os_str(), LOADED_TIMEOUT),
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
            probe(CliKind::Claude, exe.as_os_str(), LOADED_TIMEOUT),
            CliPresence::Found {
                version: String::new()
            }
        );
    }

    /// On Linux, running a file that some process still has open for writing fails with ETXTBSY
    /// ("text file busy"): a CLI being updated as DuoHive starts, or, in these tests, a
    /// script just written while another test thread forks. The probe waits it out.
    #[test]
    fn probe_waits_out_a_binary_that_is_still_being_written() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "echo '2.1.288 (Claude Code)'");
        let writer = std::fs::OpenOptions::new().write(true).open(&exe).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(writer);
        });
        assert_eq!(
            probe(CliKind::Claude, exe.as_os_str(), LOADED_TIMEOUT),
            CliPresence::Found {
                version: "2.1.288".into()
            }
        );
        release.join().unwrap();
    }

    #[test]
    fn probe_gives_up_at_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "exec sleep 10");
        let started = Instant::now();
        let got = probe(CliKind::Claude, exe.as_os_str(), Duration::from_millis(200));
        // The binary sleeps 10 s; returning well before that proves the
        // timeout (not the binary) ended the probe. 8 s leaves slack for load.
        assert!(
            started.elapsed() < Duration::from_secs(8),
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
    fn probe_reads_the_version_when_a_grandchild_holds_stdout_and_kills_it() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("grandchild.pid");
        let exe = script(
            dir.path(),
            &format!(
                "echo '1.18.34'; sleep 60 & echo $! > '{}'",
                pid_file.display()
            ),
        );
        let started = Instant::now();
        // The grandchild keeps stdout open, so the probe always runs to its
        // timeout and then kills the group. A long-enough limit lets the
        // script print its version first even under load; the bound proves
        // the probe still returns at the timeout rather than at the 60 s sleep.
        let limit = Duration::from_secs(5);
        let got = probe(CliKind::Opencode, exe.as_os_str(), limit);
        assert!(
            started.elapsed() < limit + Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            got,
            CliPresence::Found {
                version: "1.18.34".into()
            }
        );
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let alive = || {
            std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        // The killed grandchild may stay a zombie until init reaps it.
        let deadline = Instant::now() + Duration::from_secs(2);
        while alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "grandchild {} still running", pid.trim());
    }

    #[test]
    fn probe_timeout_is_two_seconds() {
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(2));
    }
}
