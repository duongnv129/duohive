//! CLI detection and directory probes (D26, D30): the only place either runs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use duohive_core::adapter::{Adapters, CliPresence};
use duohive_core::cli_kind::CliKind;
use duohive_core::engine::Input;
use duohive_core::inherit::{expand_home, resolve};
use duohive_core::org::Org;
use duohive_core::validate::DirProbe;
use duohive_store::Paths;
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinError, spawn_blocking};

use crate::real_ports::FsDirProbe;

/// Every adapter's `detect()`, off the executor (each may run `--version`);
/// `None` when the runtime is shutting down.
pub(super) async fn detect_all(adapters: Arc<Adapters>) -> Option<Vec<(CliKind, CliPresence)>> {
    joined(
        spawn_blocking(move || adapters.detect_all()).await,
        "CLI detection",
    )
}

/// The blank org's CLI on first run: the first found, in `CliKind::ALL` order, else Claude (D28).
pub(super) fn first_found(list: &[(CliKind, CliPresence)]) -> CliKind {
    list.iter()
        .find(|(_, p)| matches!(p, CliPresence::Found { .. }))
        .map_or(CliKind::Claude, |(k, _)| *k)
}

/// Waits for the first frame (when the app gives one), then detects the CLIs
/// (unless the first run already did) and probes every resolved agent directory.
pub(super) async fn run_detection(d: DetectionInputs) {
    let DetectionInputs {
        first_frame,
        detect,
        adapters,
        paths,
        first_run_inventory,
        org,
        home,
        inputs,
    } = d;
    if let Some(first_frame) = first_frame {
        // A dropped sender means the app is going: detecting anyway is harmless,
        // and launches queued until the inventory (D30) never hang.
        let _ = first_frame.await;
    }
    // The inventory the model lists are read for: detected now, or the first run's own.
    let mut inventory = first_run_inventory.unwrap_or_default();
    if detect {
        let Some(list) = detect_all(Arc::clone(&adapters)).await else {
            return;
        };
        if inputs
            .send(Input::CliInventory(list.clone()))
            .await
            .is_err()
        {
            return;
        }
        inventory = list;
    }
    let dirs = agent_dirs(&org.read().unwrap_or_else(PoisonError::into_inner), &home);
    let probed = spawn_blocking(move || {
        dirs.into_iter()
            .map(|d| {
                let status = FsDirProbe.probe(&d);
                (d, status)
            })
            .collect()
    })
    .await;
    let Some(probed) = joined(probed, "directory probe") else {
        return;
    };
    if inputs.send(Input::DirInventory(probed)).await.is_err() {
        return;
    }
    super::models::run_model_listing(inventory, adapters, paths, inputs).await;
}

/// What `run_detection` needs: when to start, what to detect and list, and where.
pub(super) struct DetectionInputs {
    /// The app's first frame; detection waits for it.
    pub(super) first_frame: Option<oneshot::Receiver<()>>,
    /// Detect the CLIs (false on a first run, which already did).
    pub(super) detect: bool,
    /// The adapters to detect and list with.
    pub(super) adapters: Arc<Adapters>,
    /// Where the model cache lives.
    pub(super) paths: Paths,
    /// A first run's inventory, held by `start_system`.
    pub(super) first_run_inventory: Option<Vec<(CliKind, CliPresence)>>,
    /// The org whose directories are probed.
    pub(super) org: Arc<RwLock<Org>>,
    /// The home directory, for `~`.
    pub(super) home: PathBuf,
    /// The core's input queue.
    pub(super) inputs: mpsc::Sender<Input>,
}

/// Every resolved agent directory (Duo's too), `~` expanded, once each.
fn agent_dirs(org: &Org, home: &Path) -> BTreeSet<PathBuf> {
    std::iter::once(org.duo.name.clone())
        .chain(org.agents().into_iter().map(|a| a.agent.name.clone()))
        .filter_map(|name| resolve(org, &name).ok()?.directory)
        .map(|d| expand_home(&d, home))
        .collect()
}

/// A blocking job's result. Its panic resumes here, so the task's supervisor
/// sees it and shuts the system down; a cancellation (runtime shutdown) is `None`.
fn joined<T>(result: Result<T, JoinError>, what: &str) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => match e.try_into_panic() {
            Ok(payload) => std::panic::resume_unwind(payload),
            Err(e) => {
                tracing::warn!(error = %e, "{what} was cancelled");
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::testkit::{StubAdapter, example_org};

    use super::*;

    fn inputs(
        stub: &StubAdapter,
        first_frame: oneshot::Receiver<()>,
        first_run: bool,
        dir: &Path,
    ) -> (DetectionInputs, mpsc::Receiver<Input>) {
        let (tx, rx) = mpsc::channel(16);
        let found = vec![(
            CliKind::Claude,
            CliPresence::Found {
                version: "2.1.0".to_owned(),
            },
        )];
        let d = DetectionInputs {
            first_frame: Some(first_frame),
            detect: !first_run,
            adapters: Arc::new(Adapters::new(vec![Box::new(stub.clone())])),
            paths: Paths {
                config_dir: dir.join("c"),
                data_dir: dir.join("d"),
                tmux_socket: "t".to_owned(),
            },
            first_run_inventory: first_run.then_some(found),
            org: Arc::new(RwLock::new(example_org())),
            home: dir.to_path_buf(),
            inputs: tx,
        };
        (d, rx)
    }

    async fn listed_only_after_frame(first_run: bool) {
        let dir = tempfile::tempdir().unwrap();
        let stub = StubAdapter::new(CliKind::Claude).with_models(Ok(Default::default()));
        let (frame_tx, frame_rx) = oneshot::channel();
        let (d, mut rx) = inputs(&stub, frame_rx, first_run, dir.path());
        let task = tokio::spawn(run_detection(d));
        tokio::task::yield_now().await;
        assert_eq!(
            *stub.list_calls.lock().unwrap(),
            0,
            "no listing before the frame"
        );
        frame_tx.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(*stub.list_calls.lock().unwrap(), 1);
        let mut saw = false;
        while let Ok(i) = rx.try_recv() {
            saw |= matches!(i, Input::ModelsListed { .. });
        }
        assert!(saw);
    }

    #[tokio::test]
    async fn list_models_runs_after_the_first_frame_on_detection() {
        listed_only_after_frame(false).await;
    }

    #[tokio::test]
    async fn list_models_runs_after_the_first_frame_on_first_run() {
        listed_only_after_frame(true).await;
    }
}
