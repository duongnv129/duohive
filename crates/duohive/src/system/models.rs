//! Model and effort lists (TD §5.2): read from each found CLI after the first frame, from the
//! cache when its version matches, and reported to the core as `Input::ModelsListed`.

use std::sync::Arc;

use duohive_core::adapter::{Adapters, CliPresence, ModelList};
use duohive_core::cli_kind::CliKind;
use duohive_core::engine::Input;
use duohive_store::{Paths, model_cache};
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;

/// One CLI's list: the cache when it holds this version, else the CLI's own, which is then cached.
fn list_one(
    adapters: &Adapters,
    paths: &Paths,
    cli: CliKind,
    version: &str,
) -> Result<ModelList, String> {
    if let Some(list) = model_cache::read(paths, cli, version) {
        return Ok(list);
    }
    let adapter = adapters
        .get(cli)
        .ok_or_else(|| format!("no adapter for {}", cli.binary()))?;
    let list = adapter
        .list_models()
        .map_err(|e| format!("couldn't list {}'s models: {e}", cli.binary()))?;
    if let Err(e) = model_cache::write(paths, cli, version, &list) {
        tracing::warn!(error = %e, "model cache not written");
    }
    Ok(list)
}

/// Lists the models of every CLI, one result per CLI, whatever the org holds.
pub(super) async fn run_model_listing(
    inventory: Vec<(CliKind, CliPresence)>,
    adapters: Arc<Adapters>,
    paths: Paths,
    inputs: mpsc::Sender<Input>,
) {
    for (cli, presence) in inventory {
        // A CLI that isn't installed answers too, so its fields stop saying `loading`.
        let CliPresence::Found { version } = presence else {
            let result = Err(format!("{} not found", cli.binary()));
            if inputs
                .send(Input::ModelsListed { cli, result })
                .await
                .is_err()
            {
                return;
            }
            continue;
        };
        let (adapters, paths) = (Arc::clone(&adapters), paths.clone());
        let Ok(result) = spawn_blocking(move || list_one(&adapters, &paths, cli, &version)).await
        else {
            return;
        };
        if inputs
            .send(Input::ModelsListed { cli, result })
            .await
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::adapter::ModelListError;
    use duohive_core::testkit::StubAdapter;

    use super::*;

    fn paths(dir: &std::path::Path) -> Paths {
        Paths {
            config_dir: dir.join("c"),
            data_dir: dir.join("d"),
            tmux_socket: "t".to_owned(),
        }
    }

    fn list() -> ModelList {
        ModelList {
            models: vec!["opus".to_owned()],
            ..ModelList::default()
        }
    }

    fn adapters(stub: &StubAdapter) -> Arc<Adapters> {
        Arc::new(Adapters::new(vec![Box::new(stub.clone())]))
    }

    #[test]
    fn list_models_uses_cache_then_refreshes() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let stub = StubAdapter::new(CliKind::Claude).with_models(Ok(list()));
        let a = adapters(&stub);
        assert_eq!(list_one(&a, &p, CliKind::Claude, "2.1.0"), Ok(list()));
        assert_eq!(list_one(&a, &p, CliKind::Claude, "2.1.0"), Ok(list()));
        assert_eq!(*stub.list_calls.lock().unwrap(), 1, "second read is cached");
        assert_eq!(list_one(&a, &p, CliKind::Claude, "2.2.0"), Ok(list()));
        assert_eq!(
            *stub.list_calls.lock().unwrap(),
            2,
            "a new version re-reads"
        );
        assert_eq!(
            model_cache::read(&p, CliKind::Claude, "2.2.0"),
            Some(list())
        );
    }

    #[test]
    fn list_models_failure_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let stub = StubAdapter::new(CliKind::Claude)
            .with_models(Err(ModelListError::Parse("bad".to_owned())));
        let result = list_one(&adapters(&stub), &paths(dir.path()), CliKind::Claude, "1");
        assert_eq!(
            result,
            Err("couldn't list claude's models: unexpected output: bad".to_owned())
        );
    }

    #[test]
    fn list_models_timeout_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let stub = StubAdapter::new(CliKind::Opencode).with_models(Err(ModelListError::Timeout));
        let result = list_one(&adapters(&stub), &paths(dir.path()), CliKind::Opencode, "1");
        assert_eq!(
            result,
            Err("couldn't list opencode's models: timed out".to_owned())
        );
    }

    #[tokio::test]
    async fn listing_sends_one_input_per_cli() {
        let dir = tempfile::tempdir().unwrap();
        let claude = StubAdapter::new(CliKind::Claude).with_models(Ok(list()));
        let (tx, mut rx) = mpsc::channel(8);
        run_model_listing(
            vec![
                (
                    CliKind::Claude,
                    CliPresence::Found {
                        version: "1".into(),
                    },
                ),
                (CliKind::Opencode, CliPresence::Missing),
            ],
            adapters(&claude),
            paths(dir.path()),
            tx,
        )
        .await;
        let first = rx.recv().await.unwrap();
        assert!(matches!(
            first,
            Input::ModelsListed {
                cli: CliKind::Claude,
                result: Ok(_)
            }
        ));
        let second = rx.recv().await.unwrap();
        assert!(matches!(
            second,
            Input::ModelsListed {
                cli: CliKind::Opencode,
                result: Err(_)
            }
        ));
        assert!(rx.recv().await.is_none());
    }
}
