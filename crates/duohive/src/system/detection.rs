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
pub(super) async fn run_detection(
    first_frame: Option<oneshot::Receiver<()>>,
    adapters: Option<Arc<Adapters>>,
    org: Arc<RwLock<Org>>,
    home: PathBuf,
    inputs: mpsc::Sender<Input>,
) {
    if let Some(first_frame) = first_frame {
        // A dropped sender means the app is going: detecting anyway is harmless,
        // and launches queued until the inventory (D30) never hang.
        let _ = first_frame.await;
    }
    if let Some(adapters) = adapters {
        let Some(list) = detect_all(adapters).await else {
            return;
        };
        if inputs.send(Input::CliInventory(list)).await.is_err() {
            return;
        }
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
    let _ = inputs.send(Input::DirInventory(probed)).await;
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
