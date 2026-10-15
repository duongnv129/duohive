//! Integration tests: the runtime against a real tmux server on a throwaway socket.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use duohive_core::launch::{Key, LaunchFile, WindowId};
use duohive_core::ports::{LiveWindow, Runtime, RuntimeEvent};
use duohive_tmux::runtime::{TmuxError, TmuxRuntime, tmux_version};
use support::{TestServer, agent, read_when_ready};

const SECS: fn(u64) -> Duration = Duration::from_secs;

fn is_window_gone(ev: &RuntimeEvent, w: &WindowId) -> bool {
    matches!(ev, RuntimeEvent::WindowGone { window, .. } if window == w)
}

fn is_pane_died(ev: &RuntimeEvent, w: &WindowId) -> bool {
    matches!(ev, RuntimeEvent::PaneDied { window, .. } if window == w)
}

#[tokio::test]
async fn start_reports_started_and_tags_the_window() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let w = ev.started(&a).await;
    assert!(w.0.starts_with('@'), "{w:?}");
    let tag = srv.tmux_out(&["show-options", "-w", "-v", "-t", &w.0, "@duohive_agent"]);
    assert_eq!(tag.trim(), "a");
}

#[tokio::test]
async fn launch_script_passes_args_verbatim() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let out = srv.path("out");
    let mut cmd = srv.sh("printf '%s\\n' \"$@\" > \"$OUT\"");
    cmd.args
        .extend(["x", "it's", "$(touch pwned)", "`id`", "-rf", "a\nb", "é ✓"].map(String::from));
    cmd.env = vec![("OUT".into(), out.display().to_string())];
    let a = agent("a");
    rt.start(&a, cmd);
    ev.started(&a).await;
    let got = read_when_ready(&out, SECS(5)).await;
    assert_eq!(got, "it's\n$(touch pwned)\n`id`\n-rf\na\nb\né ✓\n");
    assert!(!srv.path("pwned").exists());
    assert!(!srv.run_root.join("a/pwned").exists());
}

#[tokio::test]
async fn env_carries_token_and_path() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let out = srv.path("out");
    let mut cmd = srv.sh(r#"printf "%s|%s" "$DUOHIVE_TOKEN" "$PATH" > "$OUT""#);
    cmd.env = vec![
        ("DUOHIVE_TOKEN".into(), "tok-1".into()),
        ("OUT".into(), out.display().to_string()),
    ];
    let a = agent("a");
    rt.start(&a, cmd);
    ev.started(&a).await;
    let got = read_when_ready(&out, SECS(5)).await;
    assert_eq!(got, format!("tok-1|{}", std::env::var("PATH").unwrap()));
}

#[tokio::test]
async fn start_writes_files_with_owner_only_modes() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    let run_dir = srv.run_root.join("a");
    let mut cmd = srv.sh("cat");
    cmd.files = vec![LaunchFile {
        path: run_dir.join("hooks.json"),
        contents: "{}".into(),
    }];
    rt.start(&a, cmd);
    ev.started(&a).await;
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&run_dir), 0o700);
    assert_eq!(mode(&run_dir.join("hooks.json")), 0o600);
    assert_eq!(mode(&run_dir.join("launch.sh")), 0o700);
    assert_eq!(
        std::fs::read_to_string(run_dir.join("hooks.json")).unwrap(),
        "{}"
    );
}

#[tokio::test]
async fn start_refuses_files_outside_the_run_dir() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    for path in [srv.path("evil"), srv.run_root.join("a/../b/evil")] {
        let mut cmd = srv.sh("cat");
        cmd.files = vec![LaunchFile {
            path: path.clone(),
            contents: "x".into(),
        }];
        rt.start(&a, cmd);
        let got = ev.next(SECS(5)).await;
        assert!(
            matches!(got, Some(RuntimeEvent::StartFailed { ref agent, .. }) if *agent == a),
            "{got:?}"
        );
        assert!(!path.exists());
    }
    assert_eq!(srv.window_ids().len(), 1, "only the keeper window");
}

#[tokio::test]
async fn start_refuses_a_run_root_with_a_control_character() {
    let srv = TestServer::with_run_root("bad\rdir/run");
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let got = ev.next(SECS(5)).await;
    assert!(
        matches!(got, Some(RuntimeEvent::StartFailed { .. })),
        "{got:?}"
    );
    assert_eq!(srv.window_ids().len(), 1, "nothing sent to tmux");
}

#[tokio::test]
async fn activity_reported_for_window_without_output() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("while :; do echo x; sleep 0.1; done"));
    ev.started(&a).await;
    let got = ev.collect(SECS(4)).await;
    let n = got
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::Activity { agent, .. } if agent == &a))
        .count();
    assert!(n >= 2, "{got:?}");
    // The control client is silenced: no %output lines are sent to it.
    let flags = srv.tmux_out(&["list-clients", "-F", "#{client_flags}"]);
    assert!(flags.contains("no-output"), "{flags}");
}

#[tokio::test]
async fn idle_windows_send_nothing() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    ev.started(&a).await;
    ev.wait_for(SECS(3), |e| matches!(e, RuntimeEvent::Activity { .. }))
        .await
        .expect("a first Activity");
    let got = ev.collect(SECS(3)).await;
    assert!(
        !got.iter()
            .any(|e| matches!(e, RuntimeEvent::Activity { .. })),
        "{got:?}"
    );
}

#[tokio::test]
async fn start_works_with_a_space_in_the_data_dir() {
    let srv = TestServer::with_run_root("My Data/run");
    let (mut rt, mut ev) = srv.connect().await;
    let out = srv.path("out");
    let mut cmd = srv.sh("echo ran > \"$OUT\"");
    cmd.env = vec![("OUT".into(), out.display().to_string())];
    let a = agent("a");
    rt.start(&a, cmd);
    ev.started(&a).await;
    assert_eq!(read_when_ready(&out, SECS(5)).await, "ran\n");
    assert!(srv.path("My Data/run/a/launch.sh").is_file());
}

#[tokio::test]
async fn pane_died_reports_exit_status() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("exit 3"));
    let w = ev.started(&a).await;
    let got = ev.wait_for(SECS(4), |e| is_pane_died(e, &w)).await;
    assert_eq!(
        got,
        Some(RuntimeEvent::PaneDied {
            agent: a,
            window: w,
            status: Some(3)
        })
    );
}

#[tokio::test]
async fn kill_targets_only_the_given_window() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let w1 = ev.started(&a).await;
    rt.kill(&a, &w1);
    ev.wait_for(SECS(3), |e| is_window_gone(e, &w1))
        .await
        .expect("WindowGone @1");
    rt.start(&a, srv.sh("cat"));
    let w2 = ev.started(&a).await;
    rt.kill(&a, &w1);
    ev.collect(SECS(2)).await;
    assert!(srv.window_ids().contains(&w2.0));
    let gone: Vec<_> = ev
        .seen
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::WindowGone { .. }))
        .collect();
    assert_eq!(
        gone,
        vec![&RuntimeEvent::WindowGone {
            agent: a,
            window: w1
        }]
    );
}

#[tokio::test]
async fn stop_sends_quit_keys() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh(r#"read l; [ "$l" = q ] && exit 0; exit 1"#));
    let w = ev.started(&a).await;
    rt.stop(&a, vec![Key::Text("q".into()), Key::Enter], SECS(5));
    let got = ev.wait_for(SECS(3), |e| is_pane_died(e, &w)).await;
    assert_eq!(
        got,
        Some(RuntimeEvent::PaneDied {
            agent: a,
            window: w,
            status: Some(0)
        })
    );
}

#[tokio::test]
async fn stop_sends_text_with_a_leading_dash_literally() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(
        &a,
        srv.sh(r#"read l; [ "$l" = '-x #{q}' ] && exit 0; exit 1"#),
    );
    let w = ev.started(&a).await;
    rt.stop(&a, vec![Key::Text("-x #{q}".into()), Key::Enter], SECS(5));
    let got = ev.wait_for(SECS(3), |e| is_pane_died(e, &w)).await;
    assert!(
        matches!(
            got,
            Some(RuntimeEvent::PaneDied {
                status: Some(0),
                ..
            })
        ),
        "{got:?}"
    );
}

#[tokio::test]
async fn stop_kills_after_grace() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("trap '' INT; sleep 100"));
    let w = ev.started(&a).await;
    let t0 = Instant::now();
    rt.stop(&a, vec![Key::CtrlC], SECS(1));
    ev.wait_for(SECS(4), |e| is_window_gone(e, &w))
        .await
        .expect("WindowGone after grace");
    assert!(
        t0.elapsed() >= Duration::from_millis(900),
        "{:?}",
        t0.elapsed()
    );
}

#[tokio::test]
async fn stop_with_no_quit_keys_kills_after_grace() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let w = ev.started(&a).await;
    rt.stop(&a, Vec::new(), SECS(1));
    ev.wait_for(SECS(4), |e| is_window_gone(e, &w))
        .await
        .expect("WindowGone after grace");
}

#[tokio::test]
async fn capture_returns_screen_text() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("printf 'hello\\nworld\\n'; cat"));
    ev.started(&a).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    rt.capture(&a);
    let got = ev
        .wait_for(SECS(3), |e| matches!(e, RuntimeEvent::Screen { .. }))
        .await;
    let Some(RuntimeEvent::Screen { agent: who, text }) = got else {
        panic!("{got:?}")
    };
    assert_eq!(who, a);
    // tmux 3.2a keeps trailing spaces with -J; the adapters trim them (Tasks 20–21).
    let lines: Vec<_> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect();
    assert!(lines.starts_with(&["hello", "world"]), "{text:?}");
}

#[tokio::test]
async fn paste_types_multiline_text() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat > out"));
    ev.started(&a).await;
    rt.paste(
        &a,
        Vec::new(),
        "line one\nline two".into(),
        vec![Key::Enter],
    );
    let out = srv.path("out");
    let deadline = Instant::now() + SECS(5);
    loop {
        let got = std::fs::read_to_string(&out).unwrap_or_default();
        if got == "line one\nline two\n" {
            break;
        }
        assert!(Instant::now() < deadline, "{got:?}; events {:?}", ev.seen);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The keys before a paste reach the pane first (OpenCode's `Escape`, TD §5.5).
#[tokio::test]
async fn paste_sends_before_keys_first() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat > out"));
    ev.started(&a).await;
    rt.paste(
        &a,
        vec![Key::Text("first ".into())],
        "then".into(),
        vec![Key::Enter],
    );
    let out = srv.path("out");
    let deadline = Instant::now() + SECS(5);
    loop {
        let got = std::fs::read_to_string(&out).unwrap_or_default();
        if got == "first then\n" {
            break;
        }
        assert!(Instant::now() < deadline, "{got:?}; events {:?}", ev.seen);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn paste_without_a_window_fails() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    rt.paste(&agent("nobody"), Vec::new(), "x".into(), vec![Key::Enter]);
    let got = ev.next(SECS(3)).await;
    assert!(
        matches!(got, Some(RuntimeEvent::PasteFailed { .. })),
        "{got:?}"
    );
}

#[tokio::test]
async fn inventory_lists_tagged_windows_with_dead_status() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let (a, b) = (agent("a"), agent("b"));
    rt.start(&a, srv.sh("cat"));
    let wa = ev.started(&a).await;
    rt.start(&b, srv.sh("exit 2"));
    let wb = ev.started(&b).await;
    ev.wait_for(SECS(4), |e| is_pane_died(e, &wb))
        .await
        .expect("b died");
    rt.inventory();
    let got = ev
        .wait_for(SECS(3), |e| matches!(e, RuntimeEvent::Inventory(_)))
        .await;
    let Some(RuntimeEvent::Inventory(mut list)) = got else {
        panic!("{got:?}")
    };
    list.sort_by(|x, y| x.agent.cmp(&y.agent));
    let summary: Vec<_> = list
        .iter()
        .map(|w| (w.agent.clone(), w.window.clone(), w.dead))
        .collect();
    assert_eq!(summary, vec![(a, wa, None), (b, wb, Some(Some(2)))]);
    assert!(list.iter().all(|w| w.activity_secs > 0));
}

#[tokio::test]
async fn connection_lost_when_server_killed() {
    let srv = TestServer::new();
    let (_rt, mut ev) = srv.connect().await;
    srv.tmux(&["kill-server"]);
    let got = ev
        .wait_for(SECS(2), |e| {
            matches!(e, RuntimeEvent::ConnectionLost { .. })
        })
        .await;
    assert!(got.is_some(), "{:?}", ev.seen);
}

#[tokio::test]
async fn control_client_loss_reattaches() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let (n, d) = (agent("n"), agent("d"));
    rt.start(&n, srv.sh("while :; do echo x; sleep 0.1; done"));
    ev.started(&n).await;
    rt.start(&d, srv.sh("sleep 0.5; exit 4"));
    let wd = ev.started(&d).await;
    srv.tmux(&["detach-client", "-s", "duohive"]);
    let got = ev.collect(SECS(3)).await;
    assert!(
        !got.iter()
            .any(|e| matches!(e, RuntimeEvent::ConnectionLost { .. })),
        "{got:?}"
    );
    assert!(
        got.iter()
            .any(|e| matches!(e, RuntimeEvent::Activity { agent, .. } if agent == &n)),
        "{got:?}"
    );
    assert!(
        got.contains(&RuntimeEvent::PaneDied {
            agent: d,
            window: wd,
            status: Some(4)
        }),
        "{got:?}"
    );
    let flags = srv.tmux_out(&["list-clients", "-F", "#{client_flags}"]);
    assert!(flags.contains("no-output"), "silenced again: {flags}");
}

#[tokio::test]
async fn reconnect_finds_windows_started_by_a_previous_connection() {
    let srv = TestServer::new();
    let a = agent("a");
    let w = {
        let (mut rt, mut ev) = srv.connect().await;
        rt.start(&a, srv.sh("cat"));
        ev.started(&a).await
    };
    let (mut rt, mut ev) = srv.connect().await;
    rt.inventory();
    let got = ev
        .wait_for(SECS(3), |e| matches!(e, RuntimeEvent::Inventory(_)))
        .await;
    let Some(RuntimeEvent::Inventory(list)) = got else {
        panic!("{got:?}")
    };
    assert_eq!(list.len(), 1, "{list:?}");
    let LiveWindow {
        agent: who,
        window,
        dead,
        ..
    } = &list[0];
    assert_eq!((who, window, dead), (&a, &w, &None));
}

#[tokio::test]
async fn restored_windows_report_nothing_before_the_inventory() {
    let srv = TestServer::new();
    let (r1, r2) = (agent("r1"), agent("r2"));
    let (w1, w2) = {
        let (mut rt, mut ev) = srv.connect().await;
        rt.start(&r1, srv.sh("sleep 1; exit 7"));
        let w1 = ev.started(&r1).await;
        rt.start(&r2, srv.sh("cat"));
        (w1, ev.started(&r2).await)
    };
    let (mut rt, mut ev) = srv.connect().await;
    tokio::time::sleep(SECS(1)).await;
    srv.tmux(&["kill-window", "-t", &w2.0]);
    // r1 dies and r2 vanishes while the engine has not asked for the inventory yet.
    let before = ev.collect(Duration::from_millis(2500)).await;
    assert!(before.is_empty(), "{before:?}");
    rt.inventory();
    let got = ev.next(SECS(3)).await;
    let Some(RuntimeEvent::Inventory(list)) = got else {
        panic!("{got:?}")
    };
    let summary: Vec<_> = list.iter().map(|w| (w.window.clone(), w.dead)).collect();
    assert_eq!(summary, vec![(w1.clone(), Some(Some(7)))]);
    // Already reported dead by the inventory: no second report.
    let after = ev.collect(Duration::from_millis(2500)).await;
    assert!(
        !after
            .iter()
            .any(|e| is_pane_died(e, &w1) || is_window_gone(e, &w2)),
        "{after:?}"
    );
}

#[tokio::test]
async fn tmux_version_reports_found_missing_and_unknown() {
    let srv = TestServer::new();
    let v = tmux_version(&srv.config()).unwrap().expect("tmux on PATH");
    assert!(v.supported(), "{v:?}");
    let mut cfg = srv.config();
    cfg.tmux_bin = srv.path("no-such-tmux");
    assert_eq!(tmux_version(&cfg).unwrap(), None);
    cfg.tmux_bin = fake_tmux(&srv, "tmux master");
    assert!(matches!(
        tmux_version(&cfg),
        Err(TmuxError::UnknownVersion { .. })
    ));
}

/// A wrapper that prints `version` for `-V` and runs the real tmux otherwise.
fn fake_tmux(srv: &TestServer, version: &str) -> std::path::PathBuf {
    let real = String::from_utf8(
        std::process::Command::new("sh")
            .args(["-c", "command -v tmux"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let path = srv.path(&format!("tmux-{}", version.replace(' ', "-")));
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \" $* \" in *\" -V \"*) echo '{version}'; exit 0;; esac\nexec '{}' \"$@\"\n",
            real.trim()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn connect_refuses_old_tmux_and_proceeds_on_unknown() {
    let mut srv = TestServer::new();
    srv.tmux_bin = fake_tmux(&srv, "tmux 3.1c");
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let err = TmuxRuntime::connect(srv.config(), tx).await.err();
    assert!(
        matches!(err, Some(TmuxError::Unsupported { ref found }) if found == "tmux 3.1c"),
        "{err:?}"
    );
    srv.tmux_bin = fake_tmux(&srv, "tmux master");
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    ev.started(&a).await;
}

#[tokio::test]
async fn started_windows_are_160_by_48() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let w = ev.started(&a).await;
    let size = srv.tmux_out(&[
        "display-message",
        "-p",
        "-t",
        &w.0,
        "#{window_width}x#{window_height}",
    ]);
    assert_eq!(size.trim(), "160x48");
}

#[tokio::test]
async fn server_ignores_user_config_and_runs_posix_sh() {
    let srv = TestServer::new();
    let (_rt, _ev) = srv.connect().await;
    let files = srv.tmux_out(&["display-message", "-p", "#{config_files}"]);
    assert_eq!(files.trim(), "/dev/null");
    let shell = srv.tmux_out(&["show-options", "-g", "-v", "default-shell"]);
    assert_eq!(shell.trim(), "/bin/sh");
}

#[tokio::test]
async fn start_refuses_a_symlinked_launch_script() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let target = srv.path("target");
    std::fs::write(&target, "orig").unwrap();
    let run_dir = srv.run_root.join("a");
    std::fs::create_dir_all(&run_dir).unwrap();
    std::os::unix::fs::symlink(&target, run_dir.join("launch.sh")).unwrap();
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let got = ev.next(SECS(5)).await;
    assert!(
        matches!(got, Some(RuntimeEvent::StartFailed { .. })),
        "{got:?}"
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "orig");
}

#[tokio::test]
async fn reattach_reports_a_window_killed_while_detached() {
    let srv = TestServer::new();
    let (mut rt, mut ev) = srv.connect().await;
    let a = agent("a");
    rt.start(&a, srv.sh("cat"));
    let w = ev.started(&a).await;
    // One tmux command list: the window goes after the control client has detached.
    srv.tmux(&[
        "detach-client",
        "-s",
        "duohive",
        ";",
        "kill-window",
        "-t",
        &w.0,
    ]);
    let got = ev.collect(SECS(3)).await;
    assert!(
        !got.iter()
            .any(|e| matches!(e, RuntimeEvent::ConnectionLost { .. })),
        "{got:?}"
    );
    let gone: Vec<_> = got.iter().filter(|e| is_window_gone(e, &w)).collect();
    assert_eq!(gone.len(), 1, "{got:?}");
    assert!(!srv.window_ids().contains(&w.0));
}
