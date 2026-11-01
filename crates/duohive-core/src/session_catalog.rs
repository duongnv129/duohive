//! The session catalog: a CLI's past sessions, listed so one can be resumed (v1.1).

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli_kind::CliKind;
use crate::names::AgentName;
use crate::worktree::TaskId;

/// One past session of a CLI, as shown in the resume picker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    /// The CLI's own session id.
    pub id: String,
    /// The session's title or first prompt.
    pub title: String,
    /// When it was last updated, in milliseconds since the Unix epoch.
    pub updated_ms: u64,
    /// The git branch it ran on, when the CLI records one (OpenCode does not).
    pub branch: Option<String>,
    /// Its size on disk in bytes; 0 means unknown (OpenCode does not report it).
    pub size_bytes: u64,
    /// The directory it ran in.
    pub dir: PathBuf,
    /// The worktree task that directory belongs to, when it is one.
    pub worktree: Option<TaskId>,
    /// The agent currently running this session, when one is.
    pub in_use_by: Option<AgentName>,
}

/// Why a CLI's sessions couldn't be listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogError {
    /// The session store could not be read; the text is the reason.
    Unreadable(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::Unreadable(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Lists a CLI's past sessions. Fire-and-forget; the answer returns as an
/// `Input::SessionsListed`.
pub trait SessionCatalog: Send {
    /// Asks for the sessions of `cli` that ran in any of `dirs` (each with the
    /// worktree task it belongs to, if any), on behalf of `agent`.
    fn request(&mut self, agent: &AgentName, cli: CliKind, dirs: Vec<(PathBuf, Option<TaskId>)>);
}

/// A [`SessionCatalog`] that answers nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSessionCatalog;

impl SessionCatalog for NoSessionCatalog {
    fn request(
        &mut self,
        _agent: &AgentName,
        _cli: CliKind,
        _dirs: Vec<(PathBuf, Option<TaskId>)>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::n;
    use crate::worktree::TaskId;

    fn info() -> SessionInfo {
        SessionInfo {
            id: "abc-123".into(),
            title: "Fix the \"flaky\" test ✓".into(),
            updated_ms: 1_700_000_000_000,
            branch: None,
            size_bytes: 0,
            dir: "/work/repo".into(),
            worktree: Some(TaskId("t-1".into())),
            in_use_by: Some(n("a")),
        }
    }

    #[test]
    fn session_info_serde_round_trip() {
        let with_branch = SessionInfo {
            branch: Some("main".into()),
            size_bytes: 4096,
            ..info()
        };
        for s in [info(), with_branch] {
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(serde_json::from_str::<SessionInfo>(&json).unwrap(), s);
        }
    }

    #[test]
    fn catalog_error_display_is_the_reason() {
        let e = CatalogError::Unreadable("permission denied".into());
        assert_eq!(e.to_string(), "permission denied");
    }

    #[test]
    fn fake_catalog_records_requests() {
        let mut fake = crate::testkit::FakeSessionCatalog::default();
        let calls = std::sync::Arc::clone(&fake.requests);
        fake.request(
            &n("a"),
            crate::cli_kind::CliKind::Claude,
            vec![("/r".into(), None)],
        );
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
}
