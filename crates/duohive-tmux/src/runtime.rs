//! `TmuxRuntime`: duohive-core's `Runtime` port over tmux control mode.

use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;

use duohive_core::engine::Input;
use duohive_core::launch::{Key, LaunchCommand, WindowId, sh_quote};
use duohive_core::names::AgentName;
use duohive_core::ports::{Runtime, RuntimeEvent};
use tokio::sync::{Mutex, mpsc};
use tokio::task::{AbortHandle, JoinHandle};

use crate::activity::{self, Table};
use crate::connection::{self, CmdError, Conn, Notice};
use crate::quote::tmux_quote;
use crate::server;
pub use crate::server::{TmuxConfig, TmuxError, tmux_version};

/// The activity poll period (S0.5).
const POLL_EVERY: Duration = Duration::from_secs(1);

/// The wait between two keys sent in a row (see `key_steps`).
const KEY_PAUSE: Duration = Duration::from_millis(150);

/// One request from the engine, served in order.
enum Req {
    Start(AgentName, LaunchCommand),
    Stop(AgentName, Vec<Key>, Duration),
    Kill(WindowId),
    Paste(AgentName, String, Vec<Key>),
    Capture(AgentName),
    Inventory,
}

/// What the runtime's tasks share. Every event is sent with `table` locked, so
/// events about a window keep their order (Started before PaneDied, Inventory
/// before anything about a restored window).
struct Shared {
    cfg: TmuxConfig,
    conn: Conn,
    events: mpsc::Sender<Input>,
    table: Mutex<Table>,
    /// Polls run one at a time, so replies are diffed in the order they were asked for.
    poll_gate: Mutex<()>,
}

impl Shared {
    async fn emit(&self, ev: RuntimeEvent) {
        if self.events.send(Input::Runtime(ev)).await.is_err() {
            tracing::debug!("engine gone; runtime event dropped");
        }
    }

    async fn emit_all(&self, evs: Vec<RuntimeEvent>) {
        for ev in evs {
            self.emit(ev).await;
        }
    }
}

/// The tmux runtime. Its methods queue requests for its tasks; results come back as events.
pub struct TmuxRuntime {
    requests: mpsc::UnboundedSender<Req>,
    tasks: Vec<AbortHandle>,
}

impl TmuxRuntime {
    /// Checks the tmux version, sets up the server and session, attaches the control
    /// client (silenced, D25) and starts the activity poll.
    ///
    /// # Errors
    /// tmux missing or older than 3.2 (an unknown version is logged and accepted),
    /// or the server set-up or attach failing.
    pub async fn connect(cfg: TmuxConfig, events: mpsc::Sender<Input>) -> Result<Self, TmuxError> {
        server::check_version(&cfg).await?;
        server::ensure_server(&cfg).await?;
        let (conn, notices, conn_task) = connection::spawn(cfg.clone()).await?;
        let shared = Arc::new(Shared {
            cfg,
            conn,
            events,
            table: Mutex::new(Table::default()),
            poll_gate: Mutex::new(()),
        });
        let (requests, rx) = mpsc::unbounded_channel();
        let events = shared.events.clone();
        let tasks = vec![
            watched("control client", conn_task, events.clone()),
            watched(
                "activity poll",
                tokio::spawn(poll_loop(shared.clone())),
                events.clone(),
            ),
            watched(
                "notice loop",
                tokio::spawn(notice_loop(shared.clone(), notices)),
                events.clone(),
            ),
            watched(
                "request loop",
                tokio::spawn(request_loop(shared, rx)),
                events,
            ),
        ];
        Ok(Self { requests, tasks })
    }

    fn send(&self, req: Req) {
        if self.requests.send(req).is_err() {
            tracing::warn!("tmux runtime stopped; request dropped");
        }
    }
}

impl Drop for TmuxRuntime {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl Runtime for TmuxRuntime {
    fn start(&mut self, agent: &AgentName, cmd: LaunchCommand) {
        self.send(Req::Start(agent.clone(), cmd));
    }

    fn stop(&mut self, agent: &AgentName, quit: Vec<Key>, grace: Duration) {
        self.send(Req::Stop(agent.clone(), quit, grace));
    }

    fn kill(&mut self, _agent: &AgentName, window: &WindowId) {
        self.send(Req::Kill(window.clone()));
    }

    fn paste(&mut self, agent: &AgentName, text: String, submit: Vec<Key>) {
        self.send(Req::Paste(agent.clone(), text, submit));
    }

    fn capture(&mut self, agent: &AgentName) {
        self.send(Req::Capture(agent.clone()));
    }

    fn inventory(&mut self) {
        self.send(Req::Inventory);
    }
}

/// Watches a spawned runtime task: if it panics, the engine hears `ConnectionLost`
/// (its agents error) instead of DuoHive silently losing tmux. Aborting the returned
/// handle (on drop) stops the task and reports nothing.
fn watched(name: &'static str, task: JoinHandle<()>, events: mpsc::Sender<Input>) -> AbortHandle {
    let abort = task.abort_handle();
    tokio::spawn(async move {
        if let Err(e) = task.await
            && e.is_panic()
        {
            tracing::error!(task = name, "tmux runtime task panicked");
            let reason = format!("tmux {name} stopped");
            let _ = events
                .send(Input::Runtime(RuntimeEvent::ConnectionLost { reason }))
                .await;
        }
    });
    abort
}

/// Polls once a second until the connection is gone.
async fn poll_loop(shared: Arc<Shared>) {
    let mut every = tokio::time::interval(POLL_EVERY);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        every.tick().await;
        let closed = poll(&shared).await == Err(CmdError::Closed) && shared.conn.is_closed();
        if closed || shared.events.is_closed() {
            return;
        }
    }
}

/// One poll: `list-windows` (no lock held while tmux answers), then the diff and its
/// events under the table lock. Entries added after the mark wait for the next reply.
async fn poll(shared: &Shared) -> Result<(), CmdError> {
    let _one_at_a_time = shared.poll_gate.lock().await;
    let mark = shared.table.lock().await.mark();
    let reply = shared
        .conn
        .cmd(activity::poll_command(&shared.cfg.session))
        .await
        .inspect_err(|e| tracing::debug!(%e, "activity poll failed"))?;
    let rows: Vec<_> = reply
        .iter()
        .filter_map(|l| activity::parse_poll_line(l))
        .collect();
    let mut table = shared.table.lock().await;
    let evs = table.diff(&rows, mark);
    shared.emit_all(evs).await;
    drop(table);
    Ok(())
}

/// Window closes, reattaches (diffed at once) and the loss of the server.
async fn notice_loop(shared: Arc<Shared>, mut notices: mpsc::UnboundedReceiver<Notice>) {
    while let Some(notice) = notices.recv().await {
        match notice {
            Notice::WindowClose(window) => {
                let mut table = shared.table.lock().await;
                if let Some(ev) = table.gone(&window) {
                    shared.emit(ev).await;
                }
            }
            Notice::Reattached => {
                let _ = poll(&shared).await;
            }
            Notice::Lost(reason) => {
                let _guard = shared.table.lock().await;
                shared.emit(RuntimeEvent::ConnectionLost { reason }).await;
                return;
            }
        }
    }
}

/// Serves requests in order; a stop's grace runs on its own task.
async fn request_loop(shared: Arc<Shared>, mut requests: mpsc::UnboundedReceiver<Req>) {
    while let Some(req) = requests.recv().await {
        match req {
            Req::Start(agent, cmd) => start(&shared, agent, cmd).await,
            Req::Stop(agent, quit, grace) => stop(&shared, &agent, &quit, grace).await,
            Req::Kill(window) => kill(&shared, &window).await,
            Req::Paste(agent, text, submit) => paste(&shared, agent, &text, &submit).await,
            Req::Capture(agent) => capture(&shared, agent).await,
            Req::Inventory => inventory(&shared).await,
        }
    }
}

async fn start(shared: &Shared, agent: AgentName, cmd: LaunchCommand) {
    match open_window(shared, &agent, &cmd).await {
        Ok(window) => {
            let mut table = shared.table.lock().await;
            table.insert_started(&window, &agent);
            shared.emit(RuntimeEvent::Started { agent, window }).await;
        }
        Err(reason) => {
            tracing::warn!(agent = agent.as_str(), reason, "start failed");
            shared
                .emit(RuntimeEvent::StartFailed { agent, reason })
                .await;
        }
    }
}

/// Writes the run dir and opens the tagged window; any failure is the reason.
async fn open_window(
    shared: &Shared,
    agent: &AgentName,
    cmd: &LaunchCommand,
) -> Result<WindowId, String> {
    let run_dir = shared.cfg.run_root.join(agent.as_str());
    let script = render_script(cmd, std::env::var("PATH").ok().as_deref())?;
    let launch = run_dir.join("launch.sh");
    let launch_str = launch
        .to_str()
        .ok_or_else(|| format!("run dir is not UTF-8: {}", run_dir.display()))?;
    let shell = format!("sh {}", sh_quote(launch_str));
    // Quoting first: a run dir tmux can't carry fails before anything is written or sent.
    let line = format!(
        "new-window -d -t {} -n {} -P -F '#{{window_id}}' {}",
        q(&format!("{}:", server::exact(&shared.cfg)))?,
        q(&escape_format(agent.as_str()))?,
        q(&shell)?,
    );
    write_run_dir(&run_dir, cmd, &script)?;
    let reply = shared.conn.cmd(line).await.map_err(|e| e.to_string())?;
    let window = reply
        .first()
        .map(|l| l.trim())
        .filter(|w| w.starts_with('@'))
        .map(|w| WindowId(w.to_owned()))
        .ok_or_else(|| format!("unexpected new-window reply {reply:?}"))?;
    let tag = format!(
        "set-option -w -t {} @duohive_agent {}",
        q(&window.0)?,
        q(agent.as_str())?
    );
    if let Err(e) = shared.conn.cmd(tag).await {
        let _ = shared
            .conn
            .cmd(format!("kill-window -t {}", q(&window.0)?))
            .await;
        return Err(format!("tagging the window failed: {e}"));
    }
    Ok(window)
}

/// `tmux_quote` with the error as text.
fn q(arg: &str) -> Result<String, String> {
    tmux_quote(arg).map_err(|e| e.to_string())
}

/// Doubles `#` so a formatted argument (`-n`, `-c`, `display-message`) stays literal.
fn escape_format(s: &str) -> String {
    s.replace('#', "##")
}

/// The command for one key; `Text` is literal (`-l`) after `--`.
fn key_command(window: &WindowId, key: &Key) -> Result<String, String> {
    let target = q(&window.0)?;
    Ok(match key {
        Key::Enter => format!("send-keys -t {target} Enter"),
        Key::Escape => format!("send-keys -t {target} Escape"),
        Key::CtrlC => format!("send-keys -t {target} C-c"),
        Key::CtrlD => format!("send-keys -t {target} C-d"),
        Key::Text(text) => format!("send-keys -t {target} -l -- {}", q(text)?),
    })
}

/// One step of sending keys: a tmux command, or a wait before the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Cmd(String),
    Pause(Duration),
}

/// The steps for `keys`, empty text skipped, with `pause` between two keys. Sent back
/// to back, a TUI reads them as one burst (checked live with Claude Code 2.1.289 and
/// OpenCode 1.18.34, TD §5.5): OpenCode takes ESC and `/` as one Alt-key, so `Escape`
/// misses an open approval prompt and `Enter` answers it, and drops an `Enter` that
/// arrives with the `/exit` it follows. Every command is built first, so a key tmux
/// can't carry fails before anything is sent.
fn key_steps(window: &WindowId, keys: &[Key], pause: Duration) -> Result<Vec<Step>, String> {
    let keys: Vec<&Key> = keys
        .iter()
        .filter(|k| !matches!(k, Key::Text(t) if t.is_empty()))
        .collect();
    let mut steps = Vec::with_capacity(keys.len() + 1);
    for (i, key) in keys.iter().enumerate() {
        steps.push(Step::Cmd(key_command(window, key)?));
        if i + 1 < keys.len() {
            steps.push(Step::Pause(pause));
        }
    }
    Ok(steps)
}

async fn send_keys(shared: &Shared, window: &WindowId, keys: &[Key]) -> Result<(), String> {
    for step in key_steps(window, keys, KEY_PAUSE)? {
        match step {
            Step::Cmd(line) => {
                shared.conn.cmd(line).await.map_err(|e| e.to_string())?;
            }
            Step::Pause(d) => tokio::time::sleep(d).await,
        }
    }
    Ok(())
}

async fn current_window(shared: &Shared, agent: &AgentName) -> Option<WindowId> {
    shared.table.lock().await.current(agent)
}

async fn stop(shared: &Arc<Shared>, agent: &AgentName, quit: &[Key], grace: Duration) {
    let Some(window) = current_window(shared, agent).await else {
        tracing::info!(agent = agent.as_str(), "stop: no window");
        return;
    };
    if let Err(e) = send_keys(shared, &window, quit).await {
        tracing::warn!(agent = agent.as_str(), e, "stop: quit keys failed");
    }
    let shared = Arc::clone(shared);
    tokio::spawn(async move {
        tokio::time::sleep(grace).await;
        if shared.table.lock().await.contains(&window) {
            kill(&shared, &window).await;
        }
    });
}

async fn kill(shared: &Shared, window: &WindowId) {
    let Ok(target) = q(&window.0) else { return };
    if let Err(e) = shared.conn.cmd(format!("kill-window -t {target}")).await {
        tracing::debug!(window = window.0, %e, "kill-window failed");
    }
}

async fn paste(shared: &Shared, agent: AgentName, text: &str, submit: &[Key]) {
    if let Err(reason) = try_paste(shared, &agent, text, submit).await {
        shared
            .emit(RuntimeEvent::PasteFailed { agent, reason })
            .await;
    }
}

async fn try_paste(
    shared: &Shared,
    agent: &AgentName,
    text: &str,
    submit: &[Key],
) -> Result<(), String> {
    let window = current_window(shared, agent)
        .await
        .ok_or_else(|| "no window".to_owned())?;
    server::load_buffer(&shared.cfg, text).await?;
    let line = format!(
        "paste-buffer -d -p -b {} -t {}",
        q(&shared.cfg.buffer_name)?,
        q(&window.0)?
    );
    shared.conn.cmd(line).await.map_err(|e| e.to_string())?;
    send_keys(shared, &window, submit).await
}

async fn capture(shared: &Shared, agent: AgentName) {
    let Some(window) = current_window(shared, &agent).await else {
        tracing::info!(agent = agent.as_str(), "capture: no window");
        return;
    };
    let Ok(target) = q(&window.0) else { return };
    match shared
        .conn
        .cmd(format!("capture-pane -p -J -t {target}"))
        .await
    {
        Ok(lines) => {
            let text = lines.join("\n");
            shared.emit(RuntimeEvent::Screen { agent, text }).await;
        }
        Err(e) => tracing::warn!(agent = agent.as_str(), %e, "capture failed"),
    }
}

/// Lists tagged windows; they join the table already reported, so the poll only
/// reports what changes after this `Inventory`.
///
/// A reply lost to a reattach (`Closed` while the connection lives) is asked for once
/// more. Any other failure is only logged: the port has no failed-inventory event, and an
/// empty `Inventory` would read as "every window vanished". If the server is gone,
/// `ConnectionLost` follows instead.
async fn inventory(shared: &Shared) {
    let line = activity::inventory_command(&shared.cfg.session);
    let mut reply = shared.conn.cmd(line.clone()).await;
    if reply == Err(CmdError::Closed) && !shared.conn.is_closed() {
        reply = shared.conn.cmd(line).await;
    }
    let reply = match reply {
        Ok(reply) => reply,
        Err(e) => {
            tracing::warn!(%e, "inventory failed");
            return;
        }
    };
    let list: Vec<_> = reply
        .iter()
        .filter_map(|l| activity::parse_inventory_line(l))
        .collect();
    let mut table = shared.table.lock().await;
    for live in &list {
        table.insert_restored(live);
    }
    shared.emit(RuntimeEvent::Inventory(list)).await;
}

/// `launch.sh`: DuoHive's PATH (Review Focus 4), the env, the cwd, then `exec` with
/// every argument single-quoted, so text reaches the program verbatim.
fn render_script(cmd: &LaunchCommand, path: Option<&str>) -> Result<String, String> {
    let mut s = String::from("#!/bin/sh\n");
    if let Some(path) = path {
        s.push_str(&format!("export PATH={}\n", sh_quote(path)));
    }
    for (k, v) in &cmd.env {
        if !is_env_name(k) {
            return Err(format!("invalid environment variable name {k:?}"));
        }
        s.push_str(&format!("export {k}={}\n", sh_quote(v)));
    }
    let cwd = cmd
        .cwd
        .to_str()
        .ok_or_else(|| format!("directory is not UTF-8: {}", cmd.cwd.display()))?;
    s.push_str(&format!("cd {} || exit 126\n", sh_quote(cwd)));
    s.push_str("exec");
    for word in std::iter::once(&cmd.program).chain(&cmd.args) {
        s.push(' ');
        s.push_str(&sh_quote(word));
    }
    s.push('\n');
    Ok(s)
}

fn is_env_name(k: &str) -> bool {
    let mut chars = k.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// True when `path` names something strictly inside `dir`, with no `..` or `.`.
fn is_inside(path: &Path, dir: &Path) -> bool {
    path.strip_prefix(dir).is_ok_and(|rest| {
        rest.components().next().is_some()
            && rest.components().all(|c| matches!(c, Component::Normal(_)))
    })
}

/// Creates `run_dir` (0700), writes `cmd.files` (0600) and `launch.sh` (0700).
fn write_run_dir(run_dir: &Path, cmd: &LaunchCommand, script: &str) -> Result<(), String> {
    if let Some(bad) = cmd.files.iter().find(|f| !is_inside(&f.path, run_dir)) {
        return Err(format!(
            "launch file {} is outside {}",
            bad.path.display(),
            run_dir.display()
        ));
    }
    let io = |what: &Path, e: std::io::Error| format!("{}: {e}", what.display());
    make_private_dir(run_dir).map_err(|e| io(run_dir, e))?;
    for file in &cmd.files {
        if let Some(parent) = file.path.parent() {
            make_private_dir(parent).map_err(|e| io(parent, e))?;
        }
        write_file(&file.path, &file.contents, 0o600).map_err(|e| io(&file.path, e))?;
    }
    let launch = run_dir.join("launch.sh");
    write_file(&launch, script, 0o700).map_err(|e| io(&launch, e))
}

fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

fn write_file(path: &Path, contents: &str, mode: u32) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        // A symlink planted at the path is refused, never followed.
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    f.set_permissions(std::fs::Permissions::from_mode(mode))?;
    f.write_all(contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_task_that_dies_reports_the_connection_lost() {
        let (tx, mut rx) = mpsc::channel(4);
        // `resume_unwind` panics without the panic hook: no noise in the output.
        let _task = watched(
            "activity poll",
            tokio::spawn(async { std::panic::resume_unwind(Box::new("boom")) }),
            tx,
        );
        match rx.recv().await {
            Some(Input::Runtime(RuntimeEvent::ConnectionLost { reason })) => {
                assert_eq!(reason, "tmux activity poll stopped");
            }
            other => panic!("expected ConnectionLost, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_aborted_task_reports_nothing() {
        let (tx, mut rx) = mpsc::channel(4);
        let task = watched("activity poll", tokio::spawn(std::future::pending()), tx);
        task.abort();
        assert!(rx.recv().await.is_none());
    }

    #[test]
    fn launch_script_quotes_every_word() {
        let cmd = LaunchCommand {
            program: "/bin/my prog".into(),
            args: vec!["it's".into(), "$(x)".into()],
            env: vec![("TOKEN".into(), "a b".into())],
            cwd: "/w d".into(),
            files: Vec::new(),
        };
        assert_eq!(
            render_script(&cmd, Some("/bin:/usr/bin")).unwrap(),
            "#!/bin/sh\nexport PATH='/bin:/usr/bin'\nexport TOKEN='a b'\ncd '/w d' || exit 126\nexec '/bin/my prog' 'it'\\''s' '$(x)'\n"
        );
    }

    #[test]
    fn launch_script_rejects_bad_env_names() {
        let mut cmd = LaunchCommand {
            program: "p".into(),
            args: Vec::new(),
            env: vec![("A;rm".into(), "x".into())],
            cwd: "/".into(),
            files: Vec::new(),
        };
        assert!(render_script(&cmd, None).is_err());
        cmd.env = vec![("_OK1".into(), "x".into())];
        assert!(render_script(&cmd, None).is_ok());
    }

    #[test]
    fn files_must_be_strictly_inside_the_run_dir() {
        let dir = Path::new("/d/run/a");
        assert!(is_inside(Path::new("/d/run/a/x.json"), dir));
        assert!(is_inside(Path::new("/d/run/a/sub/x"), dir));
        assert!(!is_inside(Path::new("/d/run/a"), dir));
        assert!(!is_inside(Path::new("/d/run/a/../b/x"), dir));
        assert!(!is_inside(Path::new("/d/run/ab/x"), dir));
        assert!(!is_inside(Path::new("/etc/x"), dir));
    }

    #[test]
    fn format_hashes_are_doubled() {
        assert_eq!(escape_format("a#{b}#"), "a##{b}##");
    }

    #[test]
    fn keys_map_to_send_keys() {
        let w = WindowId("@3".into());
        assert_eq!(
            key_command(&w, &Key::Enter).unwrap(),
            "send-keys -t '@3' Enter"
        );
        assert_eq!(
            key_command(&w, &Key::CtrlC).unwrap(),
            "send-keys -t '@3' C-c"
        );
        assert_eq!(
            key_command(&w, &Key::CtrlD).unwrap(),
            "send-keys -t '@3' C-d"
        );
        assert_eq!(
            key_command(&w, &Key::Escape).unwrap(),
            "send-keys -t '@3' Escape"
        );
        assert_eq!(
            key_command(&w, &Key::Text("-x it's".into())).unwrap(),
            r"send-keys -t '@3' -l -- '-x it'\''s'"
        );
        assert!(key_command(&w, &Key::Text("a\nb".into())).is_err());
    }

    fn plan(keys: &[Key]) -> Vec<Step> {
        key_steps(&WindowId("@3".into()), keys, Duration::from_millis(7)).unwrap()
    }

    #[test]
    fn every_key_that_more_keys_follow_gets_the_pause() {
        let pause = Step::Pause(Duration::from_millis(7));
        assert_eq!(
            plan(&[
                Key::Escape,
                Key::CtrlC,
                Key::Text("/exit".into()),
                Key::Enter
            ]),
            vec![
                Step::Cmd("send-keys -t '@3' Escape".into()),
                pause.clone(),
                Step::Cmd("send-keys -t '@3' C-c".into()),
                pause.clone(),
                Step::Cmd("send-keys -t '@3' -l -- '/exit'".into()),
                pause,
                Step::Cmd("send-keys -t '@3' Enter".into()),
            ]
        );
    }

    #[test]
    fn a_last_key_has_no_pause_and_empty_text_is_skipped() {
        assert_eq!(
            plan(&[Key::Enter]),
            vec![Step::Cmd("send-keys -t '@3' Enter".into())]
        );
        assert_eq!(
            plan(&[Key::Escape, Key::Text(String::new())]),
            vec![Step::Cmd("send-keys -t '@3' Escape".into())]
        );
        assert_eq!(plan(&[]), vec![]);
    }

    #[test]
    fn a_bad_key_fails_before_anything_is_sent() {
        assert!(
            key_steps(
                &WindowId("@3".into()),
                &[Key::Escape, Key::Text("a\nb".into())],
                Duration::ZERO
            )
            .is_err()
        );
    }
}
