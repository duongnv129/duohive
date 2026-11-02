//! The session catalog (v1.1, TD §5.6): lists a CLI's past sessions off the engine's task and
//! reports each answer to the core as `Input::SessionsListed`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use duohive_core::adapter::Adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::engine::Input;
use duohive_core::names::AgentName;
use duohive_core::session_catalog::{CatalogError, SessionCatalog, SessionInfo};
use duohive_core::worktree::TaskId;
use tokio::sync::mpsc;

/// The directories to read, each with the worktree task it belongs to.
type Dirs = Vec<(PathBuf, Option<TaskId>)>;

/// Blocking read of one CLI's sessions.
type Reader = Arc<
    dyn Fn(CliKind, &[(PathBuf, Option<TaskId>)]) -> Result<Vec<SessionInfo>, CatalogError>
        + Send
        + Sync,
>;

/// How long one listing may take before it is answered as unreadable. OpenCode's reader
/// runs once per distinct directory, up to 10 s each, so 60 s covers a handful of them; the
/// Claude reader (file reads) has no limit of its own, and a hung mount must not leave `r`
/// without an answer, since the engine keeps one pending listing per agent.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// The real [`SessionCatalog`]: never blocks the caller; always answers each request once.
pub struct SessionCatalogRunner {
    reader: Reader,
    inputs: mpsc::Sender<Input>,
    handle: tokio::runtime::Handle,
    timeout: Duration,
}

impl SessionCatalogRunner {
    /// Reads through the CLIs' adapters, from `home`, on the runtime the caller is in.
    pub fn new(adapters: Arc<Adapters>, home: PathBuf, inputs: mpsc::Sender<Input>) -> Self {
        Self::with_reader(
            adapter_reader(adapters, home),
            inputs,
            tokio::runtime::Handle::current(),
        )
    }

    fn with_reader(
        reader: Reader,
        inputs: mpsc::Sender<Input>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            reader,
            inputs,
            handle,
            timeout: READ_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn adapter_reader(adapters: Arc<Adapters>, home: PathBuf) -> Reader {
    Arc::new(move |cli, dirs| {
        let adapter = adapters
            .get(cli)
            .ok_or_else(|| CatalogError::Unreadable(format!("no adapter for {}", cli.binary())))?;
        adapter.list_sessions(Path::new(&home), dirs)
    })
}

impl SessionCatalog for SessionCatalogRunner {
    fn request(&mut self, agent: &AgentName, cli: CliKind, dirs: Dirs) {
        let (reader, inputs, agent, timeout) = (
            Arc::clone(&self.reader),
            self.inputs.clone(),
            agent.clone(),
            self.timeout,
        );
        self.handle.spawn(async move {
            let read = tokio::task::spawn_blocking(move || reader(cli, &dirs));
            // On timeout the blocking thread cannot be cancelled: its handle is dropped, so a
            // late result is discarded and the request is never answered twice.
            let result = match tokio::time::timeout(timeout, read).await {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "session listing failed");
                    Err(CatalogError::Unreadable(
                        "reading the sessions failed".into(),
                    ))
                }
                Err(_) => {
                    tracing::warn!(?timeout, "session listing timed out");
                    Err(CatalogError::Unreadable(
                        "reading the sessions timed out".into(),
                    ))
                }
            };
            // A closed channel means shutdown.
            let _ = inputs.send(Input::SessionsListed { agent, result }).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    use duohive_core::testkit::n;

    use super::*;

    fn runner(reader: Reader) -> (SessionCatalogRunner, mpsc::Receiver<Input>) {
        let (tx, rx) = mpsc::channel(8);
        (
            SessionCatalogRunner::with_reader(reader, tx, tokio::runtime::Handle::current()),
            rx,
        )
    }

    fn info(id: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            title: "t".into(),
            updated_ms: 1,
            branch: None,
            size_bytes: 0,
            dir: PathBuf::from("/w"),
            worktree: None,
            in_use_by: None,
        }
    }

    async fn next(rx: &mut mpsc::Receiver<Input>) -> Input {
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("an input in time")
            .expect("channel open")
    }

    #[tokio::test]
    async fn request_answers_with_an_input() {
        let (mut r, mut rx) = runner(Arc::new(|_, _| Ok(vec![info("s1")])));
        r.request(&n("ana"), CliKind::Claude, vec![]);
        let Input::SessionsListed { agent, result } = next(&mut rx).await else {
            panic!("expected SessionsListed");
        };
        assert_eq!(agent, n("ana"));
        assert_eq!(result, Ok(vec![info("s1")]));
    }

    #[tokio::test]
    async fn reader_panic_becomes_unreadable_not_a_crash() {
        let (mut r, mut rx) = runner(Arc::new(|_, _| panic!("boom")));
        r.request(&n("ana"), CliKind::Claude, vec![]);
        let Input::SessionsListed { result, .. } = next(&mut rx).await else {
            panic!("expected SessionsListed");
        };
        assert_eq!(
            result,
            Err(CatalogError::Unreadable(
                "reading the sessions failed".into()
            ))
        );
    }

    #[tokio::test]
    async fn request_does_not_block_the_caller() {
        let (release_tx, release_rx) = std_mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let (mut r, mut rx) = runner(Arc::new(move |_, _| {
            let _ = release_rx.lock().map(|g| g.recv());
            Ok(vec![])
        }));
        r.request(&n("ana"), CliKind::Claude, vec![]);
        assert!(rx.try_recv().is_err());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err(), "the reader is still blocked");
        release_tx.send(()).expect("release");
        assert!(matches!(next(&mut rx).await, Input::SessionsListed { .. }));
    }

    #[tokio::test]
    async fn missing_adapter_answers_unreadable() {
        let (tx, mut rx) = mpsc::channel(8);
        let reader = adapter_reader(Arc::new(Adapters::new(vec![])), PathBuf::from("/h"));
        let mut r =
            SessionCatalogRunner::with_reader(reader, tx, tokio::runtime::Handle::current());
        r.request(&n("ana"), CliKind::Opencode, vec![]);
        let Input::SessionsListed { result, .. } = next(&mut rx).await else {
            panic!("expected SessionsListed");
        };
        assert_eq!(
            result,
            Err(CatalogError::Unreadable(format!(
                "no adapter for {}",
                CliKind::Opencode.binary()
            )))
        );
    }

    #[tokio::test]
    async fn exactly_one_input_per_request() {
        let (mut r, mut rx) = runner(Arc::new(|_, _| Ok(vec![])));
        r.request(&n("ana"), CliKind::Claude, vec![]);
        r.request(&n("bo"), CliKind::Claude, vec![]);
        let _ = next(&mut rx).await;
        let _ = next(&mut rx).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
    }

    fn runner_with_timeout(
        reader: Reader,
        timeout: Duration,
    ) -> (SessionCatalogRunner, mpsc::Receiver<Input>) {
        let (r, rx) = runner(reader);
        (r.with_timeout(timeout), rx)
    }

    fn timed_out() -> Result<Vec<SessionInfo>, CatalogError> {
        Err(CatalogError::Unreadable(
            "reading the sessions timed out".into(),
        ))
    }

    #[tokio::test]
    async fn a_reader_that_never_returns_is_answered_unreadable_after_the_timeout() {
        let (release_tx, release_rx) = std_mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let (mut r, mut rx) = runner_with_timeout(
            Arc::new(move |_, _| {
                let _ = release_rx.lock().map(|g| g.recv());
                Ok(vec![])
            }),
            Duration::from_millis(150),
        );
        r.request(&n("ana"), CliKind::Claude, vec![]);
        let Input::SessionsListed { agent, result } = next(&mut rx).await else {
            panic!("expected SessionsListed");
        };
        assert_eq!(agent, n("ana"));
        assert_eq!(result, timed_out());
        drop(release_tx);
    }

    #[tokio::test]
    async fn a_late_result_after_the_timeout_sends_no_second_input() {
        let (r_tx, r_rx) = std_mpsc::channel::<()>();
        let r_rx = Mutex::new(r_rx);
        let (mut r, mut rx) = runner_with_timeout(
            Arc::new(move |_, _| {
                let _ = r_rx.lock().map(|g| g.recv());
                Ok(vec![info("late")])
            }),
            Duration::from_millis(150),
        );
        r.request(&n("ana"), CliKind::Claude, vec![]);
        let Input::SessionsListed { result, .. } = next(&mut rx).await else {
            panic!("expected SessionsListed");
        };
        assert_eq!(result, timed_out());
        r_tx.send(()).expect("release the reader");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(rx.try_recv().is_err(), "no second input for the request");
    }

    #[tokio::test]
    async fn a_reader_within_the_timeout_is_unaffected() {
        let (mut r, mut rx) = runner_with_timeout(
            Arc::new(|_, _| {
                std::thread::sleep(Duration::from_millis(50));
                Ok(vec![info("s1")])
            }),
            Duration::from_secs(5),
        );
        r.request(&n("ana"), CliKind::Claude, vec![]);
        let Input::SessionsListed { result, .. } = next(&mut rx).await else {
            panic!("expected SessionsListed");
        };
        assert_eq!(result, Ok(vec![info("s1")]));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn closed_input_channel_is_ignored() {
        let (mut r, rx) = runner(Arc::new(|_, _| Ok(vec![])));
        drop(rx);
        r.request(&n("ana"), CliKind::Claude, vec![]);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
