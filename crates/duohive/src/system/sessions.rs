//! The session catalog (v1.1, TD §5.6): lists a CLI's past sessions off the engine's task and
//! reports each answer to the core as `Input::SessionsListed`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// The real [`SessionCatalog`]: never blocks the caller; always answers each request once.
pub struct SessionCatalogRunner {
    reader: Reader,
    inputs: mpsc::Sender<Input>,
    handle: tokio::runtime::Handle,
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
        }
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
        let (reader, inputs, agent) =
            (Arc::clone(&self.reader), self.inputs.clone(), agent.clone());
        self.handle.spawn(async move {
            let result = match tokio::task::spawn_blocking(move || reader(cli, &dirs)).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::warn!(error = %e, "session listing failed");
                    Err(CatalogError::Unreadable(
                        "reading the sessions failed".into(),
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

    #[tokio::test]
    async fn closed_input_channel_is_ignored() {
        let (mut r, rx) = runner(Arc::new(|_, _| Ok(vec![])));
        drop(rx);
        r.request(&n("ana"), CliKind::Claude, vec![]);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
