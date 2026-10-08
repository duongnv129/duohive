//! Desktop alerts for macOS (`osascript`) and Linux (`notify-send`) (P2-D18).
//!
//! One child process per alert, killed after a timeout. Alert text only ever
//! travels as argv elements, never through a shell and never inside script text.
//! A click is reported where the notifier can (`notify-send --action`), and
//! never raises the terminal window.
//!
//! Children are run from plain threads rather than tokio's process support: the
//! core's runtime has no I/O driver, and a thread per (rare) alert keeps the
//! engine's task free.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use duohive_core::alert::Alert;
use duohive_core::engine::Input;
use duohive_core::ports::Notifier;
use tokio::sync::mpsc;

/// How long a child may live in production.
pub const ALERT_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a waiting thread checks whether its child has exited.
const POLL: Duration = Duration::from_millis(20);
/// What `notify-send` prints when the user clicks the action.
const OPEN_ACTION: &str = "open";

/// The operating systems with a desktop notifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// macOS, through `osascript`.
    MacOs,
    /// Linux, through `notify-send`.
    Linux,
}

impl Platform {
    /// The running platform (`std::env::consts::OS`), when it is supported.
    pub fn current() -> Option<Self> {
        Self::from_os(std::env::consts::OS)
    }

    fn from_os(os: &str) -> Option<Self> {
        match os {
            "macos" => Some(Self::MacOs),
            "linux" => Some(Self::Linux),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Backend {
    OsaScript(PathBuf),
    NotifySend { bin: PathBuf, action: bool },
}

/// Shows alerts through the system's own notifier.
#[derive(Debug)]
pub struct DesktopNotifier {
    backend: Option<Backend>,
    path: Option<OsString>,
    tx: mpsc::Sender<Input>,
    timeout: Duration,
}

impl DesktopNotifier {
    /// Looks for the platform's notifier on `path_env["PATH"]`. Children run
    /// with that same `PATH`. Blocking: it may run `notify-send --help`.
    pub fn detect(
        platform: Platform,
        path_env: &HashMap<String, String>,
        tx: mpsc::Sender<Input>,
        timeout: Duration,
    ) -> Self {
        let path = path_env.get("PATH").map(OsString::from);
        let backend = path.as_deref().and_then(|p| match platform {
            Platform::MacOs => find_on_path(p, "osascript").map(Backend::OsaScript),
            Platform::Linux => find_on_path(p, "notify-send").map(|bin| {
                let action = supports_action(&bin, p);
                Backend::NotifySend { bin, action }
            }),
        });
        Self {
            backend,
            path,
            tx,
            timeout,
        }
    }
}

impl Notifier for DesktopNotifier {
    fn available(&self) -> bool {
        self.backend.is_some()
    }

    fn notify(&mut self, alert: &Alert) {
        let Some(backend) = &self.backend else {
            return;
        };
        let (bin, args, reports_clicks) = match backend {
            Backend::OsaScript(bin) => (bin, osascript_args(alert), false),
            Backend::NotifySend { bin, action } => (bin, notify_send_args(alert, *action), *action),
        };
        let mut command = Command::new(bin);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(path) = &self.path {
            command.env("PATH", path);
        }
        let child = match command.spawn() {
            Ok(child) => child,
            Err(e) => return tracing::warn!(error = %e, "cannot show a desktop alert"),
        };
        let click = reports_clicks.then(|| (self.tx.clone(), alert.agent.clone()));
        let timeout = self.timeout;
        let spawned = thread::Builder::new()
            .name("duohive-alert".to_owned())
            .spawn(move || supervise(child, timeout, click));
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot watch a desktop alert");
        }
    }
}

/// The constant script: the title and body arrive through `argv`.
const OSASCRIPT: [&str; 3] = [
    "on run argv",
    "display notification (item 2 of argv) with title (item 1 of argv)",
    "end run",
];

fn osascript_args(alert: &Alert) -> Vec<String> {
    let mut args = Vec::with_capacity(OSASCRIPT.len() * 2 + 3);
    for line in OSASCRIPT {
        args.push("-e".to_owned());
        args.push(line.to_owned());
    }
    // `--` so a title or body starting with `-` is never an option.
    args.extend(["--".to_owned(), alert.title.clone(), alert.body.clone()]);
    args
}

fn notify_send_args(alert: &Alert, action: bool) -> Vec<String> {
    let mut args = vec!["--app-name=DuoHive".to_owned()];
    if action {
        args.push(format!("--action={OPEN_ACTION}=Open"));
    }
    args.extend(["--".to_owned(), alert.title.clone(), alert.body.clone()]);
    args
}

/// Waits for `child`, reporting a printed `open` as a click, and kills it at `timeout`.
fn supervise(
    mut child: Child,
    timeout: Duration,
    click: Option<(mpsc::Sender<Input>, duohive_core::names::AgentName)>,
) {
    if let (Some(stdout), Some((tx, agent))) = (child.stdout.take(), click) {
        // Detached: a grandchild holding the pipe must not hold this thread.
        let reader = thread::Builder::new()
            .name("duohive-alert-out".to_owned())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if line.trim() == OPEN_ACTION {
                        let _ = tx.blocking_send(Input::AlertClicked {
                            agent: agent.clone(),
                        });
                    }
                }
            });
        if let Err(e) = reader {
            tracing::warn!(error = %e, "cannot read a desktop alert's answer");
        }
    }
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = %e, "cannot wait for a desktop alert");
                break;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The first executable named `name` in the `PATH` value `path`.
fn find_on_path(path: &std::ffi::OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Whether `notify-send --help` lists `--action`.
fn supports_action(bin: &Path, path: &std::ffi::OsStr) -> bool {
    Command::new(bin)
        .arg("--help")
        .env("PATH", path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("--action"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use duohive_core::alert::AlertKind;
    use duohive_core::testkit::n;
    use tempfile::TempDir;

    use super::*;

    const SYSTEM_PATH: &str = "/usr/bin:/bin";
    const HOSTILE: &str = "\"); do shell script \"rm";

    fn alert(body: &str) -> Alert {
        Alert {
            agent: n("auth"),
            kind: AlertKind::NeedsYou,
            title: "auth needs you".to_owned(),
            body: body.to_owned(),
        }
    }

    fn path_env(dir: &Path) -> HashMap<String, String> {
        HashMap::from([(
            "PATH".to_owned(),
            format!("{}:{SYSTEM_PATH}", dir.display()),
        )])
    }

    fn script(dir: &Path, name: &str, body: &str) {
        let file = dir.join(name);
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    async fn eventually(what: &str, mut ready: impl FnMut() -> bool) {
        for _ in 0..200 {
            if ready() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("timed out waiting for {what}");
    }

    #[test]
    fn detect_none_when_neither_on_path() {
        let empty = TempDir::new().unwrap();
        let env = HashMap::from([("PATH".to_owned(), empty.path().display().to_string())]);
        for platform in [Platform::MacOs, Platform::Linux] {
            let (tx, _rx) = mpsc::channel(1);
            let n = DesktopNotifier::detect(platform, &env, tx, ALERT_TIMEOUT);
            assert!(!n.available(), "{platform:?}");
        }
        let (tx, _rx) = mpsc::channel(1);
        let no_path = DesktopNotifier::detect(Platform::Linux, &HashMap::new(), tx, ALERT_TIMEOUT);
        assert!(!no_path.available());
    }

    #[test]
    fn detects_the_platforms_own_notifier() {
        let dir = TempDir::new().unwrap();
        script(dir.path(), "osascript", "exit 0");
        let (tx, _rx) = mpsc::channel(1);
        let env = path_env(dir.path());
        assert!(
            DesktopNotifier::detect(Platform::MacOs, &env, tx.clone(), ALERT_TIMEOUT).available()
        );
        assert!(!DesktopNotifier::detect(Platform::Linux, &env, tx, ALERT_TIMEOUT).available());
        assert_eq!(Platform::from_os("macos"), Some(Platform::MacOs));
        assert_eq!(Platform::from_os("windows"), None);
    }

    #[test]
    fn osascript_args_are_argv_not_script_text() {
        let args = osascript_args(&alert(HOSTILE));
        let (script, argv) = args.split_at(args.len() - 3);
        assert_eq!(argv, ["--", "auth needs you", HOSTILE]);
        assert!(script.iter().all(|a| !a.contains("rm")));
        assert!(script.iter().all(|a| !a.contains("auth")));
    }

    #[tokio::test]
    async fn osascript_receives_the_hostile_body_as_one_argument() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("args");
        script(
            dir.path(),
            "osascript",
            &format!("printf '%s\\n' \"$@\" > '{}'", out.display()),
        );
        let (tx, _rx) = mpsc::channel(1);
        let mut notifier =
            DesktopNotifier::detect(Platform::MacOs, &path_env(dir.path()), tx, ALERT_TIMEOUT);
        notifier.notify(&alert(HOSTILE));
        eventually("osascript's arguments", || out.exists()).await;
        let lines: Vec<String> = loop {
            let text = fs::read_to_string(&out).unwrap();
            if text.ends_with(&format!("{HOSTILE}\n")) {
                break text.lines().map(str::to_owned).collect();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(lines[lines.len() - 1], HOSTILE);
        assert_eq!(lines.iter().filter(|l| l.contains("rm")).count(), 1);
    }

    #[test]
    fn notify_send_uses_action_when_supported() {
        let with = TempDir::new().unwrap();
        script(
            with.path(),
            "notify-send",
            "echo '  -A, --action=[NAME=]Text  add action'",
        );
        let without = TempDir::new().unwrap();
        script(without.path(), "notify-send", "echo 'usage: notify-send'");
        let (tx, _rx) = mpsc::channel(1);
        let a = DesktopNotifier::detect(
            Platform::Linux,
            &path_env(with.path()),
            tx.clone(),
            ALERT_TIMEOUT,
        );
        let b = DesktopNotifier::detect(
            Platform::Linux,
            &path_env(without.path()),
            tx,
            ALERT_TIMEOUT,
        );
        assert!(matches!(
            a.backend,
            Some(Backend::NotifySend { action: true, .. })
        ));
        assert!(matches!(
            b.backend,
            Some(Backend::NotifySend { action: false, .. })
        ));
        let args = notify_send_args(&alert("b"), true);
        assert!(args.contains(&"--action=open=Open".to_owned()));
        assert_eq!(args[args.len() - 3..], ["--", "auth needs you", "b"]);
        assert!(
            !notify_send_args(&alert("b"), false)
                .iter()
                .any(|a| a.starts_with("--action"))
        );
    }

    #[tokio::test]
    async fn click_sends_alert_clicked() {
        let dir = TempDir::new().unwrap();
        script(
            dir.path(),
            "notify-send",
            "case \"$1\" in --help) echo '--action';; *) echo open;; esac",
        );
        let (tx, mut rx) = mpsc::channel(4);
        let mut notifier =
            DesktopNotifier::detect(Platform::Linux, &path_env(dir.path()), tx, ALERT_TIMEOUT);
        assert!(notifier.available());
        notifier.notify(&alert("done"));
        let input = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap();
        assert!(matches!(input, Some(Input::AlertClicked { agent }) if agent == n("auth")));
    }

    #[tokio::test]
    async fn child_killed_after_timeout() {
        let dir = TempDir::new().unwrap();
        let pid_file = dir.path().join("pid");
        script(
            dir.path(),
            "osascript",
            &format!("echo $$ > '{}'\nexec sleep 30", pid_file.display()),
        );
        let (tx, _rx) = mpsc::channel(1);
        let mut notifier = DesktopNotifier::detect(
            Platform::MacOs,
            &path_env(dir.path()),
            tx,
            Duration::from_millis(1000),
        );
        notifier.notify(&alert("x"));
        eventually("the pid", || {
            fs::read_to_string(&pid_file).is_ok_and(|s| s.trim().parse::<u32>().is_ok())
        })
        .await;
        let pid = fs::read_to_string(&pid_file).unwrap().trim().to_owned();
        let alive = || {
            Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        eventually("the child to die", || !alive()).await;
    }
}
