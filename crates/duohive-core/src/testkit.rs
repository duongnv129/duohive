//! Test-only fixtures and fakes shared across crates.

use crate::cli_kind::CliKind;
use crate::names::{AgentName, NodeName};
use crate::org::{AgentDef, Fields, Org, Role, Team};
use crate::validate::{DirProbe, DirStatus};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Parses an agent name, panicking on invalid input.
pub fn n(s: &str) -> AgentName {
    AgentName::parse(s).unwrap()
}

fn agent(name: &str) -> AgentDef {
    AgentDef {
        name: n(name),
        fields: Fields::default(),
    }
}

fn role(name: &str, agents: &[&str]) -> Role {
    Role {
        name: NodeName::parse(name).unwrap(),
        defaults: Fields::default(),
        agents: agents.iter().map(|a| agent(a)).collect(),
    }
}

/// The PRD's example org: Payments (Backend, QA) plus a root Generalist role.
pub fn example_org() -> Org {
    let mut org = Org::blank(CliKind::Claude);
    org.teams = vec![Team {
        name: NodeName::parse("Payments").unwrap(),
        defaults: Fields::default(),
        roles: vec![
            role("Backend", &["auth-service", "billing-api"]),
            role("QA", &["pay-qa"]),
        ],
    }];
    org.roles = vec![role("Generalist", &["squad-1", "squad-2", "squad-3"])];
    org
}

/// A [`DirProbe`] with canned answers (a path not listed is `Found`) and a
/// count of probe calls.
pub struct FakeDirs(pub HashMap<PathBuf, DirStatus>, pub Arc<AtomicUsize>);

impl FakeDirs {
    /// Fake with these statuses and no calls yet.
    pub fn new(statuses: HashMap<PathBuf, DirStatus>) -> Self {
        Self(statuses, Arc::new(AtomicUsize::new(0)))
    }
}

impl DirProbe for FakeDirs {
    fn probe(&self, dir: &Path) -> DirStatus {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.get(dir).copied().unwrap_or(DirStatus::Found)
    }
}
