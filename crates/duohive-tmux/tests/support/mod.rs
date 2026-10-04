//! A throwaway tmux server per test (`duohive-test-<random hex>`), killed on drop.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")]

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use duohive_core::engine::Input;
use duohive_core::launch::{LaunchCommand, WindowId};
use duohive_core::names::AgentName;
use duohive_core::ports::RuntimeEvent;
use duohive_tmux::runtime::{TmuxConfig, TmuxRuntime};
use tokio::sync::mpsc;

/// A tmux server on its own socket, with a temp dir for runs and outputs.
pub struct TestServer {
    pub socket: String,
    pub dir: tempfile::TempDir,
    pub run_root: PathBuf,
    pub tmux_bin: PathBuf,
}

fn random_hex() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())
}

impl TestServer {
    pub fn new() -> Self {
        Self::with_run_root("run")
    }

    /// `rel` is the run root relative to the temp dir.
    pub fn with_run_root(rel: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let run_root = dir.path().join(rel);
        Self {
            socket: format!("duohive-test-{}", random_hex()),
            run_root,
            dir,
            tmux_bin: PathBuf::from("tmux"),
        }
    }

    pub fn config(&self) -> TmuxConfig {
        TmuxConfig {
            tmux_bin: self.tmux_bin.clone(),
            socket: self.socket.clone(),
            session: "duohive".into(),
            buffer_name: format!("duohive-{}", std::process::id()),
            run_root: self.run_root.clone(),
        }
    }

    /// Runs `tmux -L <socket> <args>`.
    pub fn tmux(&self, args: &[&str]) -> Output {
        Command::new(&self.tmux_bin)
            .args(["-f", "/dev/null", "-L"])
            .arg(&self.socket)
            .args(args)
            .env_remove("TMUX")
            .output()
            .unwrap()
    }

    /// The stdout of a tmux command.
    pub fn tmux_out(&self, args: &[&str]) -> String {
        String::from_utf8(self.tmux(args).stdout).unwrap()
    }

    /// Window ids in the session.
    pub fn window_ids(&self) -> Vec<String> {
        self.tmux_out(&["list-windows", "-t", "=duohive", "-F", "#{window_id}"])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub async fn connect(&self) -> (TmuxRuntime, Events) {
        let (tx, rx) = mpsc::channel(1024);
        let rt = TmuxRuntime::connect(self.config(), tx).await.unwrap();
        (
            rt,
            Events {
                rx,
                seen: Vec::new(),
            },
        )
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A `/bin/sh -c <script>` launch in the temp dir.
    pub fn sh(&self, script: &str) -> LaunchCommand {
        sh_in(self.dir.path(), script)
    }
}

pub fn sh_in(cwd: &Path, script: &str) -> LaunchCommand {
    LaunchCommand {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        env: Vec::new(),
        cwd: cwd.to_path_buf(),
        files: Vec::new(),
    }
}

impl TestServer {
    /// `$TMUX_TMPDIR` (else `/tmp`) `/tmux-<uid>/<socket>`, where tmux puts the socket.
    fn socket_path(&self) -> PathBuf {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::metadata(self.dir.path()).unwrap().uid();
        let tmp =
            std::env::var_os("TMUX_TMPDIR").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        tmp.join(format!("tmux-{uid}")).join(&self.socket)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.tmux(&["kill-server"]);
        // tmux 3.2a leaves its socket file behind, and a test may have killed the server.
        let _ = std::fs::remove_file(self.socket_path());
    }
}

pub fn agent(s: &str) -> AgentName {
    AgentName::parse(s).unwrap()
}

/// The runtime's events, with a log of every one seen.
pub struct Events {
    pub rx: mpsc::Receiver<Input>,
    pub seen: Vec<RuntimeEvent>,
}

impl Events {
    /// The next runtime event within `within`.
    pub async fn next(&mut self, within: Duration) -> Option<RuntimeEvent> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            match tokio::time::timeout(left, self.rx.recv()).await {
                Ok(Some(Input::Runtime(ev))) => {
                    self.seen.push(ev.clone());
                    return Some(ev);
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return None,
            }
        }
    }

    /// Waits for the first event matching `pred`.
    pub async fn wait_for(
        &mut self,
        within: Duration,
        pred: impl Fn(&RuntimeEvent) -> bool,
    ) -> Option<RuntimeEvent> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            let ev = self.next(left).await?;
            if pred(&ev) {
                return Some(ev);
            }
        }
    }

    /// Collects every event for `during`.
    pub async fn collect(&mut self, during: Duration) -> Vec<RuntimeEvent> {
        let deadline = Instant::now() + during;
        let mut out = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.next(left).await {
                Some(ev) => out.push(ev),
                None => break,
            }
        }
        out
    }

    /// Waits for `Started` (or fails on `StartFailed`) for `who`.
    pub async fn started(&mut self, who: &AgentName) -> WindowId {
        let ev = self
            .wait_for(Duration::from_secs(5), |e| {
                matches!(e, RuntimeEvent::Started { agent, .. } | RuntimeEvent::StartFailed { agent, .. } if agent == who)
            })
            .await
            .expect("no Started");
        match ev {
            RuntimeEvent::Started { window, .. } => window,
            other => panic!("expected Started, got {other:?}"),
        }
    }
}

/// Polls until `path` exists with content, up to `within`.
pub async fn read_when_ready(path: &Path, within: Duration) -> String {
    let deadline = Instant::now() + within;
    loop {
        if let Ok(s) = std::fs::read_to_string(path)
            && !s.is_empty()
        {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "{} never written",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
