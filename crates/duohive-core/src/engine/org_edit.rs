//! Org edits, reload and settings (PRD Lifecycle, UI §4.4; D10, D11, D26, D29).
//!
//! An edit is applied to a copy of the org, validated, saved and only then made
//! current. Edits are refused while `org.toml` is broken, so the user's hand
//! edit is never overwritten (Review Focus 2).

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use super::{AgentRt, Engine, reject};
use crate::command::{OrgEdit, RoleDraft};
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::names::{AgentName, NodeName};
use crate::org::{AgentDef, Fields, NodeKey, Org, Role, RoleKey, Team};
use crate::ports::{OrgFileError, StoreError};
use crate::settings::Settings;
use crate::status::AgentState;
use crate::validate::{next_agent_names, validate};

/// Why every edit is refused while `org.toml` doesn't parse.
const ORG_FILE_BROKEN: &str = "org.toml has an error · fix it first (o)";
/// Why a rename of a running (or errored) agent is refused.
const RENAME_WHILE_STOPPED: &str = "rename only while stopped with no worktrees";

impl Engine {
    /// `EditOrg`: check, apply to a copy, validate, save, then replace the org.
    pub(super) fn edit_org(&mut self, edit: OrgEdit) -> Vec<Event> {
        if self.org_error.is_some() {
            return reject(ORG_FILE_BROKEN);
        }
        if let Err(reason) = self.check_edit(&edit) {
            return reject(reason);
        }
        let mut org = self.org.clone();
        if let Err(reason) = apply_edit(&mut org, &edit) {
            return reject(reason);
        }
        if let Some(e) = validate(&org).into_iter().next() {
            return reject(e.message);
        }
        if let Err(e) = self.ports.store.save_org(&org) {
            return reject(format!("couldn't save org.toml: {e}"));
        }
        let old = std::mem::replace(&mut self.org, org);
        let mut out = Vec::new();
        self.follow_edit(&old, &edit, &mut out);
        out.insert(0, self.org_changed(&old));
        out
    }

    /// `ReloadOrg`: re-read `org.toml`; a broken file keeps the current org, and
    /// an accepted one becomes the last good org (D10).
    pub(super) fn reload_org(&mut self) -> Vec<Event> {
        let org = match self.ports.store.load_org() {
            Err(e) => return self.org_file_broken(e),
            Ok(None) => return self.org_file_fine(),
            Ok(Some(org)) => org,
        };
        if let Some(e) = validate(&org).into_iter().next() {
            return self.org_file_broken(OrgFileError {
                line: None,
                message: e.to_string(),
            });
        }
        if let Some(e) = self.reload_drops_live_agent(&org) {
            return self.org_file_broken(e);
        }
        let old = std::mem::replace(&mut self.org, org);
        self.sync_agents();
        let mut out = vec![self.org_changed(&old)];
        let fine = self.org_file_fine();
        // Not accepted while a running agent it doesn't name is held (ruling B).
        if !self.holds_rows() {
            self.mark_last_good(&mut out);
        }
        out.extend(fine);
        out
    }

    /// `UpdateSettings`: saved, unless `settings.toml` is broken (D29), then applied.
    pub(super) fn update_settings(&mut self, settings: Settings) -> Vec<Event> {
        if self.settings_error.is_none()
            && let Err(e) = self.ports.store.save_settings(&settings)
        {
            return reject(format!("couldn't save settings.toml: {e}"));
        }
        self.settings = settings;
        Vec::new()
    }

    fn org_file_broken(&mut self, e: OrgFileError) -> Vec<Event> {
        self.disown_banner();
        self.org_error = Some(e.clone());
        vec![Event::OrgFileError(Some(e))]
    }

    /// Clears the banner, unless a held agent still needs it (ruling B); held
    /// agents the org names now are adopted. With no file yet, the current org stays.
    fn org_file_fine(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        let banner = self.adopt_held(&mut out);
        self.org_error.clone_from(&banner);
        out.push(Event::OrgFileError(banner));
        out
    }

    /// The runtime checks an edit needs: deletes and renames only while stopped.
    fn check_edit(&self, edit: &OrgEdit) -> Result<(), String> {
        let not_stopped = |a: &AgentName| self.state_of(a) != AgentState::Stopped;
        match edit {
            OrgEdit::Delete(NodeKey::Duo) => Err("Duo can't be deleted".to_owned()),
            OrgEdit::Delete(NodeKey::Agent(a)) if self.org.is_duo(a) => {
                Err("Duo can't be deleted".to_owned())
            }
            OrgEdit::Delete(key) => match agents_under(&self.org, key)
                .into_iter()
                .find(|a| not_stopped(a))
            {
                Some(a) => Err(format!("stop {a} first")),
                None => Ok(()),
            },
            OrgEdit::EditAgent { key, draft } if draft.name != *key && not_stopped(key) => {
                Err(RENAME_WHILE_STOPPED.to_owned())
            }
            OrgEdit::EditOrgDefaults { duo, .. }
                if duo.name != self.org.duo.name && not_stopped(&self.org.duo.name) =>
            {
                Err(RENAME_WHILE_STOPPED.to_owned())
            }
            _ => Ok(()),
        }
    }

    /// What a saved edit does to the runtime: renames move state, deletes drop
    /// session rows, and a new CLI for a running Duo restarts it.
    fn follow_edit(&mut self, old: &Org, edit: &OrgEdit, out: &mut Vec<Event>) {
        match edit {
            OrgEdit::EditAgent { key, draft } if draft.name != *key => {
                self.move_agent(key, &draft.name, out);
            }
            OrgEdit::EditOrgDefaults { duo, .. } => {
                if duo.name != old.duo.name {
                    self.move_agent(&old.duo.name, &duo.name, out);
                }
                let name = duo.name.clone();
                // A stop the user already asked for stays a plain stop.
                let stopping = self.agents.get(&name).is_some_and(|rt| rt.stopping);
                if duo.cli != old.duo.cli && self.state_of(&name).is_running() && !stopping {
                    out.extend(self.stop_agent(&name));
                    self.rt_mut(&name).restart_after_stop = true;
                }
            }
            OrgEdit::Delete(key) => {
                for agent in agents_under(old, key) {
                    self.delete_row(&agent, out);
                }
            }
            _ => {}
        }
        self.sync_agents();
    }

    /// Moves a stopped agent's runtime state and session row to its new name:
    /// the new row is saved before the old one is deleted, and on a store
    /// failure nothing moves.
    fn move_agent(&mut self, from: &AgentName, to: &AgentName, out: &mut Vec<Event>) {
        let row = match self.ports.store.load_sessions() {
            Ok(rows) => rows.into_iter().find(|r| &r.agent == from),
            Err(e) => return state_not_saved(&e, out),
        };
        if let Some(mut row) = row {
            row.agent = to.clone();
            if let Err(e) = self.ports.store.save_session(&row) {
                return state_not_saved(&e, out);
            }
            self.delete_row(from, out);
        }
        if let Some(rt) = self.agents.remove(from) {
            self.agents.insert(to.clone(), rt);
        }
    }

    /// Refuses a reload that drops an agent that isn't stopped (renamed or removed
    /// by hand, Duo included): its live session must not be orphaned (D9).
    fn reload_drops_live_agent(&self, org: &Org) -> Option<OrgFileError> {
        let keeps = |a: &AgentName| org.is_duo(a) || org.find_agent(a).is_some();
        self.agent_names()
            .into_iter()
            .filter(|a| !keeps(a))
            .find_map(|a| drop_refusal(&a, self.state_of(&a)))
    }

    /// Gives every agent in the org a runtime entry and drops entries the org no
    /// longer names (only stopped agents ever leave the org).
    pub(super) fn sync_agents(&mut self) {
        let now = self.ports.clock.now();
        let names: BTreeSet<AgentName> = self.agent_names().into_iter().collect();
        self.agents.retain(|a, _| names.contains(a));
        for name in names {
            self.agents
                .entry(name)
                .or_insert_with(|| AgentRt::stopped(now));
        }
    }

    /// `OrgChanged` for the current org. Once directories are known (D26), each
    /// directory whose agent's setting changed since `old` is probed first.
    pub(super) fn org_changed(&mut self, old: &Org) -> Event {
        if self.dirs.is_some() {
            let before = self.directories(old);
            let changed: BTreeSet<PathBuf> = self
                .directories(&self.org)
                .into_iter()
                .filter(|(agent, dir)| before.get(agent) != Some(dir))
                .map(|(_, dir)| dir)
                .collect();
            let probed: Vec<_> = changed
                .into_iter()
                .map(|dir| {
                    let status = self.ports.dirs.probe(&dir);
                    (dir, status)
                })
                .collect();
            if let Some(cache) = self.dirs.as_mut() {
                cache.extend(probed);
            }
        }
        Event::OrgChanged {
            org: Box::new(self.org.clone()),
            problems: self.problems(),
        }
    }

    /// Each agent's resolved, home-expanded directory in `org`.
    fn directories(&self, org: &Org) -> HashMap<AgentName, PathBuf> {
        let names = std::iter::once(org.duo.name.clone())
            .chain(org.agents().into_iter().map(|a| a.agent.name.clone()));
        names
            .filter_map(|name| {
                let dir = resolve(org, &name).ok()?.directory?;
                Some((name, expand_home(&dir, &self.cfg.home)))
            })
            .collect()
    }
}

/// Why an org that drops `agent` in `state` can't be accepted; `None` when stopped.
pub(super) fn drop_refusal(agent: &AgentName, state: AgentState) -> Option<OrgFileError> {
    let how = match state {
        AgentState::Stopped => return None,
        AgentState::Errored => "errored",
        _ => "running",
    };
    Some(OrgFileError {
        line: None,
        message: format!("{agent} is {how} · stop it, then press o to reload"),
    })
}

/// Reports a session row that couldn't be written.
fn state_not_saved(e: &StoreError, out: &mut Vec<Event>) {
    out.push(Event::CommandRejected {
        reason: format!("couldn't save state: {e}"),
    });
}

/// Agents a delete of `key` removes, in tree order.
fn agents_under(org: &Org, key: &NodeKey) -> Vec<AgentName> {
    let names = |roles: &[Role]| -> Vec<AgentName> {
        roles
            .iter()
            .flat_map(|r| r.agents.iter().map(|a| a.name.clone()))
            .collect()
    };
    match key {
        NodeKey::Duo => Vec::new(),
        NodeKey::Team(t) => org
            .teams
            .iter()
            .find(|team| &team.name == t)
            .map_or_else(Vec::new, |team| names(&team.roles)),
        NodeKey::Role(k) => {
            find_role(org, k).map_or_else(Vec::new, |r| names(std::slice::from_ref(r)))
        }
        NodeKey::Agent(a) => org
            .find_agent(a)
            .map_or_else(Vec::new, |loc| vec![loc.agent.name.clone()]),
    }
}

/// Applies `edit` to `org`; fails only when its target doesn't exist. Pure: forms use it to
/// validate a draft against the org it would produce.
pub fn apply_edit(org: &mut Org, edit: &OrgEdit) -> Result<(), String> {
    match edit {
        OrgEdit::AddTeam(d) => org.teams.push(Team {
            name: d.name.clone(),
            defaults: d.defaults.clone(),
            roles: Vec::new(),
        }),
        OrgEdit::AddRole { team, role, agents } => {
            let new = new_role(org, team.as_ref(), role, *agents);
            roles_mut(org, team.as_ref())?.push(new);
        }
        OrgEdit::AddAgent { role, agent } => role_mut(org, role)?.agents.push(AgentDef {
            name: agent.name.clone(),
            fields: agent.fields.clone(),
        }),
        OrgEdit::EditTeam { key, draft } => {
            let team = team_mut(org, key)?;
            team.name = draft.name.clone();
            team.defaults = draft.defaults.clone();
        }
        OrgEdit::EditRole { key, draft } => {
            let role = role_mut(org, key)?;
            role.name = draft.name.clone();
            role.defaults = draft.defaults.clone();
        }
        OrgEdit::EditAgent { key, draft } => {
            let agent = agent_mut(org, key)?;
            agent.name = draft.name.clone();
            agent.fields = draft.fields.clone();
        }
        OrgEdit::EditOrgDefaults { defaults, duo } => {
            org.defaults = defaults.clone();
            org.duo = duo.clone();
        }
        OrgEdit::Delete(key) => delete(org, key)?,
    }
    Ok(())
}

/// A role with `count` new agents named by D11, with empty fields.
fn new_role(org: &Org, team: Option<&NodeName>, draft: &RoleDraft, count: u8) -> Role {
    Role {
        name: draft.name.clone(),
        defaults: draft.defaults.clone(),
        agents: next_agent_names(org, team, &draft.name, count)
            .into_iter()
            .map(|name| AgentDef {
                name,
                fields: Fields::default(),
            })
            .collect(),
    }
}

fn delete(org: &mut Org, key: &NodeKey) -> Result<(), String> {
    match key {
        // Refused before any edit is applied.
        NodeKey::Duo => return Err("Duo can't be deleted".to_owned()),
        NodeKey::Team(t) => {
            let at = org.teams.iter().position(|team| &team.name == t);
            org.teams.remove(at.ok_or_else(|| no_team(t))?);
        }
        NodeKey::Role(k) => {
            let roles = roles_mut(org, k.team.as_ref())?;
            let at = roles.iter().position(|r| r.name == k.role);
            roles.remove(at.ok_or_else(|| no_role(k))?);
        }
        NodeKey::Agent(a) => {
            let role = all_roles_mut(org)
                .find(|r| r.agents.iter().any(|x| &x.name == a))
                .ok_or_else(|| no_agent(a))?;
            role.agents.retain(|x| &x.name != a);
        }
    }
    Ok(())
}

fn find_role<'a>(org: &'a Org, key: &RoleKey) -> Option<&'a Role> {
    let roles = match &key.team {
        Some(t) => &org.teams.iter().find(|team| &team.name == t)?.roles,
        None => &org.roles,
    };
    roles.iter().find(|r| r.name == key.role)
}

fn team_mut<'a>(org: &'a mut Org, name: &NodeName) -> Result<&'a mut Team, String> {
    org.teams
        .iter_mut()
        .find(|t| &t.name == name)
        .ok_or_else(|| no_team(name))
}

fn roles_mut<'a>(org: &'a mut Org, team: Option<&NodeName>) -> Result<&'a mut Vec<Role>, String> {
    match team {
        Some(t) => Ok(&mut team_mut(org, t)?.roles),
        None => Ok(&mut org.roles),
    }
}

fn role_mut<'a>(org: &'a mut Org, key: &RoleKey) -> Result<&'a mut Role, String> {
    roles_mut(org, key.team.as_ref())?
        .iter_mut()
        .find(|r| r.name == key.role)
        .ok_or_else(|| no_role(key))
}

fn all_roles_mut(org: &mut Org) -> impl Iterator<Item = &mut Role> {
    org.teams
        .iter_mut()
        .flat_map(|t| t.roles.iter_mut())
        .chain(org.roles.iter_mut())
}

fn agent_mut<'a>(org: &'a mut Org, name: &AgentName) -> Result<&'a mut AgentDef, String> {
    all_roles_mut(org)
        .flat_map(|r| r.agents.iter_mut())
        .find(|a| &a.name == name)
        .ok_or_else(|| no_agent(name))
}

fn no_team(name: &NodeName) -> String {
    format!("no team named {name}")
}

fn no_role(key: &RoleKey) -> String {
    match &key.team {
        Some(team) => format!("no role named {team} › {}", key.role),
        None => format!("no role named {}", key.role),
    }
}

fn no_agent(name: &AgentName) -> String {
    format!("no agent named {name}")
}

#[cfg(test)]
mod tests;
