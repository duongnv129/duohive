//! `CreateOrg` (UI §4.3): the first-run wizard's answer becomes `org.toml`.
//! Creating an org starts no agent; they wait for `s` like any other org.

use std::collections::BTreeSet;

use super::{Engine, reject};
use crate::cli_kind::CliKind;
use crate::event::Event;
use crate::names::AgentName;
use crate::org::Org;
use crate::templates::{OrgTree, TemplateInput, first_run_org};
use crate::validate::validate;

#[cfg(test)]
mod tests;

impl Engine {
    /// Saves `template` as the org, with `[org] cli` and `[duo]` on `duo_cli`
    /// and your name in `settings.toml`; refused once an org exists.
    pub(super) fn create_org(
        &mut self,
        template: &TemplateInput,
        your_name: Option<String>,
        duo_cli: CliKind,
    ) -> Vec<Event> {
        if self.first_run.is_none() {
            return reject("an org already exists");
        }
        let kept: BTreeSet<AgentName> = match self.ports.store.load_worktrees() {
            Ok(rows) => rows.into_iter().map(|row| row.agent).collect(),
            Err(e) => return reject(format!("couldn't read the worktrees: {e}")),
        };
        let org = match build(template, duo_cli, &kept) {
            Ok(org) => org,
            Err(reason) => return reject(reason),
        };
        if let Some(error) = validate(&org).into_iter().next() {
            return reject(error.to_string());
        }
        if let Err(e) = self.ports.store.save_org(&org) {
            return reject(format!("couldn't write org.toml: {e}"));
        }
        if let Err(e) = self.ports.store.mark_last_good(&org) {
            return reject(format!("couldn't save org.toml: {e}"));
        }
        let mut settings = self.settings.clone();
        settings.your_name = your_name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty());
        if self.settings_error.is_none()
            && let Err(e) = self.ports.store.save_settings(&settings)
        {
            return reject(format!("couldn't save settings.toml: {e}"));
        }
        self.settings = settings;
        let old = std::mem::replace(&mut self.org, org);
        self.sync_agents();
        self.first_run = None;
        vec![self.org_changed(&old), Event::OrgCreated]
    }
}

/// The org `template` describes, or the reason it can't be built.
fn build(
    template: &TemplateInput,
    duo_cli: CliKind,
    kept: &BTreeSet<AgentName>,
) -> Result<Org, String> {
    OrgTree::build(template, kept)
        .map(|tree| first_run_org(duo_cli, tree))
        .map_err(|e| e.to_string())
}
