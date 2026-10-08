//! Runs one `git` child: argv only, never a shell, with a fixed environment and
//! a timeout.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{ChildStderr, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// How often a waiting worker checks whether its child has exited.
const POLL: Duration = Duration::from_millis(10);

/// Variables that would point git somewhere other than `-C <dir>`.
const REDIRECTING_VARS: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

/// What a finished git child left.
#[derive(Debug)]
pub(super) struct Output {
    /// Its exit code; `None` when a signal ended it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// Its stdout when it succeeded, else its stderr as the error.
    pub fn into_result(self) -> Result<String, String> {
        if self.ok() {
            Ok(self.stdout)
        } else {
            Err(self.reason())
        }
    }

    /// The first line of stderr, without git's `fatal: `/`error: ` prefix.
    pub fn reason(&self) -> String {
        let line = self.stderr.lines().find(|l| !l.trim().is_empty());
        let line = line.unwrap_or("git failed").trim();
        line.strip_prefix("fatal: ")
            .or_else(|| line.strip_prefix("error: "))
            .unwrap_or(line)
            .to_owned()
    }
}

/// Spawns git children for one [`super::GitCli`].
#[derive(Debug)]
pub(super) struct Runner {
    pub env: Vec<(OsString, OsString)>,
    pub timeout: Duration,
}

impl Runner {
    /// `git -C <dir> <args>`. `Err` when git could not be run or timed out.
    pub fn git<S: AsRef<OsStr>>(&self, dir: &Path, args: &[S]) -> Result<Output, String> {
        self.git_env(dir, args, &[])
    }

    /// [`Runner::git`] with `extra` variables for this one child.
    pub fn git_env<S: AsRef<OsStr>>(
        &self,
        dir: &Path,
        args: &[S],
        extra: &[(&str, &str)],
    ) -> Result<Output, String> {
        let name = args
            .iter()
            .map(AsRef::as_ref)
            .find(|a| !a.to_string_lossy().starts_with('-') && !a.to_string_lossy().contains('='))
            .map_or_else(String::new, |a| a.to_string_lossy().into_owned());
        let mut command = Command::new("git");
        command.arg("-C").arg(dir).args(args);
        for var in REDIRECTING_VARS {
            command.env_remove(var);
        }
        command
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .envs(extra.iter().copied())
            .env("GIT_TERMINAL_PROMPT", "0")
            // A check never takes the index lock (`status` would refresh it).
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own process group: a credential prompt on the terminal stops
            // it (and the timeout kills it) instead of reading the TUI's keys.
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot run git: {e}"))?;
        let pipes =
            drain(child.stdout.take()).and_then(|out| Ok((out, drain(child.stderr.take())?)));
        let (stdout, stderr) = match pipes {
            Ok(pipes) => pipes,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("git {name} timed out"));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("cannot wait for git {name}: {e}"));
                }
            }
        };
        // A grandchild still holding a pipe must not hold the worker past the deadline.
        let collect = |rx: mpsc::Receiver<Vec<u8>>| {
            let left = deadline.saturating_duration_since(Instant::now());
            rx.recv_timeout(left)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .map_err(|_| format!("git {name} timed out"))
        };
        Ok(Output {
            code: status.code(),
            stdout: collect(stdout)?,
            stderr: collect(stderr)?,
        })
    }

    /// `git -C <dir> <args>`, its stdout when it succeeded.
    pub fn text<S: AsRef<OsStr>>(&self, dir: &Path, args: &[S]) -> Result<String, String> {
        self.git(dir, args)?.into_result()
    }
}

/// A pipe of a child.
trait Pipe: Read + Send + 'static {}
impl Pipe for ChildStdout {}
impl Pipe for ChildStderr {}

/// Reads `pipe` to its end on its own thread.
fn drain(pipe: Option<impl Pipe>) -> Result<mpsc::Receiver<Vec<u8>>, String> {
    let (tx, rx) = mpsc::channel();
    let Some(mut pipe) = pipe else {
        let _ = tx.send(Vec::new());
        return Ok(rx);
    };
    thread::Builder::new()
        .name("duohive-git-pipe".to_owned())
        .spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        })
        .map_err(|e| format!("cannot read git's output: {e}"))?;
    Ok(rx)
}
