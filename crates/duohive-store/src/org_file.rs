//! `org.toml`: the org as a hand-editable file.
//!
//! The file shape lives here, in adapter-side structs mapped to the core
//! types, so core stays format-free. Every struct denies unknown fields, so a
//! fourth level (an agent under an agent) fails to parse.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use duohive_core::cli_kind::CliKind;
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, DuoConfig, Fields, Org, OrgDefaults, Role, Team};
use duohive_core::ports::{OrgFileError, StoreError};
use duohive_core::time::Millis;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;

/// Reads a CLI name, reporting a bad one with `CliKind`'s own message.
fn de_cli<'de, D: Deserializer<'de>>(d: D) -> Result<CliKind, D::Error> {
    let s = String::deserialize(d)?;
    CliKind::from_str(&s).map_err(de::Error::custom)
}

fn de_opt_cli<'de, D: Deserializer<'de>>(d: D) -> Result<Option<CliKind>, D::Error> {
    let s = Option::<String>::deserialize(d)?;
    s.map(|s| CliKind::from_str(&s).map_err(de::Error::custom))
        .transpose()
}

/// Declares a file struct holding `name`, the optional default fields and the
/// given extra children (`#[serde(flatten)]` cannot be combined with
/// `deny_unknown_fields`, so the fields are spelled out once here).
macro_rules! node_file {
    ($(#[$meta:meta])* $name:ident($name_ty:ty) { $($(#[$fmeta:meta])* $child:ident: $ty:ty),* }) => {
        $(#[$meta])*
        #[derive(Debug, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct $name {
            name: $name_ty,
            #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_cli")]
            cli: Option<CliKind>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            flags: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            directory: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            model: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            effort: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            worktree: Option<bool>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            prompt: Option<String>,
            $($(#[$fmeta])* $child: $ty,)*
        }
    };
}

node_file!(AgentFile(AgentName) {});
node_file!(RoleFile(NodeName) {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    agent: Vec<AgentFile>
});
node_file!(TeamFile(NodeName) {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    role: Vec<RoleFile>
});

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgSection {
    #[serde(deserialize_with = "de_cli")]
    cli: CliKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    #[serde(default)]
    worktree: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DuoSection {
    name: AgentName,
    #[serde(deserialize_with = "de_cli")]
    cli: CliKind,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgFile {
    org: OrgSection,
    duo: DuoSection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    team: Vec<TeamFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    role: Vec<RoleFile>,
}

/// Splits the shared optional fields off a file node.
macro_rules! take_fields {
    ($f:expr) => {
        Fields {
            cli: $f.cli,
            flags: $f.flags,
            directory: $f.directory,
            model: $f.model,
            effort: $f.effort,
            worktree: $f.worktree,
            prompt: $f.prompt,
        }
    };
}

/// Builds a file node from a name and its optional fields.
macro_rules! file_node {
    ($ty:ident, $name:expr, $fields:expr, $($child:ident: $value:expr),*) => {{
        let f: &Fields = $fields;
        $ty {
            name: $name,
            cli: f.cli,
            flags: f.flags.clone(),
            directory: f.directory.clone(),
            model: f.model.clone(),
            effort: f.effort.clone(),
            worktree: f.worktree,
            prompt: f.prompt.clone(),
            $($child: $value,)*
        }
    }};
}

impl AgentFile {
    fn into_core(self) -> AgentDef {
        AgentDef {
            name: self.name.clone(),
            fields: take_fields!(self),
        }
    }
}

impl RoleFile {
    fn into_core(mut self) -> Role {
        let agents = std::mem::take(&mut self.agent);
        Role {
            name: self.name.clone(),
            agents: agents.into_iter().map(AgentFile::into_core).collect(),
            defaults: take_fields!(self),
        }
    }
}

impl TeamFile {
    fn into_core(mut self) -> Team {
        let roles = std::mem::take(&mut self.role);
        Team {
            name: self.name.clone(),
            roles: roles.into_iter().map(RoleFile::into_core).collect(),
            defaults: take_fields!(self),
        }
    }
}

fn agent_file(a: &AgentDef) -> AgentFile {
    file_node!(AgentFile, a.name.clone(), &a.fields,)
}

fn role_file(r: &Role) -> RoleFile {
    file_node!(RoleFile, r.name.clone(), &r.defaults, agent: r.agents.iter().map(agent_file).collect())
}

fn team_file(t: &Team) -> TeamFile {
    file_node!(TeamFile, t.name.clone(), &t.defaults, role: t.roles.iter().map(role_file).collect())
}

fn org_file(org: &Org) -> OrgFile {
    OrgFile {
        org: OrgSection {
            cli: org.defaults.cli,
            model: org.defaults.model.clone(),
            effort: org.defaults.effort.clone(),
            worktree: org.defaults.worktree,
            prompt: org.defaults.prompt.clone(),
        },
        duo: DuoSection {
            name: org.duo.name.clone(),
            cli: org.duo.cli,
        },
        team: org.teams.iter().map(team_file).collect(),
        role: org.roles.iter().map(role_file).collect(),
    }
}

impl OrgFile {
    fn into_core(self) -> Org {
        Org {
            defaults: OrgDefaults {
                cli: self.org.cli,
                model: self.org.model,
                effort: self.org.effort,
                worktree: self.org.worktree,
                prompt: self.org.prompt,
            },
            duo: DuoConfig {
                name: self.duo.name,
                cli: self.duo.cli,
            },
            teams: self.team.into_iter().map(TeamFile::into_core).collect(),
            roles: self.role.into_iter().map(RoleFile::into_core).collect(),
        }
    }
}

/// Reads a file; `None` when it does not exist.
pub(crate) fn read_text(path: &Path) -> Result<Option<String>, OrgFileError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(OrgFileError {
            line: None,
            message: format!("cannot read {}: {e}", path.display()),
        }),
    }
}

/// Maps a TOML error to its 1-based line and its own message.
pub(crate) fn parse_error(text: &str, e: &toml::de::Error) -> OrgFileError {
    let line = e.span().map(|span| {
        text.as_bytes()[..span.start.min(text.len())]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
            + 1
    });
    OrgFileError {
        line,
        message: e.message().to_owned(),
    }
}

/// Loads `org.toml`; `Ok(None)` when there is no file. Never touches the last-good copy.
pub fn load_org(path: &Path) -> Result<Option<Org>, OrgFileError> {
    let Some(text) = read_text(path)? else {
        return Ok(None);
    };
    let file: OrgFile = toml::from_str(&text).map_err(|e| parse_error(&text, &e))?;
    Ok(Some(file.into_core()))
}

fn io_err(what: &str, path: &Path, e: &std::io::Error) -> StoreError {
    StoreError(format!("cannot {what} {}: {e}", path.display()))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Writes `bytes` to `path` through `<path>.tmp`: write, fsync, optionally copy
/// the current file to `backup`, then rename over `path`.
pub(crate) fn write_atomic(
    path: &Path,
    bytes: &[u8],
    backup: Option<&Path>,
) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io_err("create", dir, &e))?;
    }
    let tmp = sibling(path, ".tmp");
    let result = write_tmp_then_rename(path, &tmp, bytes, backup);
    if result.is_err() {
        // Never leave a stale temp file behind; the target is untouched.
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_tmp_then_rename(
    path: &Path,
    tmp: &Path,
    bytes: &[u8],
    backup: Option<&Path>,
) -> Result<(), StoreError> {
    let mut file = fs::File::create(tmp).map_err(|e| io_err("create", tmp, &e))?;
    file.write_all(bytes)
        .map_err(|e| io_err("write", tmp, &e))?;
    file.sync_all().map_err(|e| io_err("sync", tmp, &e))?;
    drop(file);
    if let Some(backup) = backup {
        match fs::copy(path, backup) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(io_err("back up to", backup, &e)),
        }
    }
    fs::rename(tmp, path).map_err(|e| io_err("replace", path, &e))
}

fn render(org: &Org) -> Result<String, StoreError> {
    toml::to_string(&org_file(org)).map_err(|e| StoreError(format!("cannot write org: {e}")))
}

/// Saves the org atomically, keeps the previous file as `org.toml.bak` and
/// refreshes the last-good copy (the engine saves only validated orgs).
///
/// Partial failure: `org.toml` is replaced first. If that succeeds but the
/// last-good write fails, this still returns `Err` though `org.toml` is new on
/// disk; the message then starts `org.toml was saved, but` so callers can tell
/// it from a save that changed nothing. Any other `Err` leaves `org.toml` as it
/// was and no `.tmp` file behind; `org.toml.bak` too, except that when the
/// backup copy succeeded and the final rename then failed, `.bak` may already
/// hold the current file.
pub fn save_org(paths: &Paths, org: &Org) -> Result<(), StoreError> {
    let text = render(org)?;
    write_atomic(
        &paths.org_toml(),
        text.as_bytes(),
        Some(&paths.org_backup()),
    )?;
    write_atomic(&paths.org_last_good(), text.as_bytes(), None).map_err(|e| {
        StoreError(format!(
            "org.toml was saved, but the last-good copy was not refreshed: {}",
            e.0
        ))
    })
}

/// Replaces the org (UI §4.8): the current `org.toml`, when there is one, is
/// copied to `org.toml.<stamp>.bak` (P4-Q5) before [`save_org`] writes the new
/// one. A missing `org.toml` leaves no backup.
pub fn replace_org(paths: &Paths, org: &Org, now: Millis) -> Result<(), StoreError> {
    let path = paths.org_toml();
    match fs::read(&path) {
        Ok(old) => {
            let backup = sibling(&path, &format!(".{}.bak", backup_stamp(now)));
            write_atomic(&backup, &old, None)?;
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(io_err("read", &path, &e)),
    }
    save_org(paths, org)
}

/// `now` as `YYYYMMDD-HHMMSS` in UTC, for a replacement's backup name.
pub fn backup_stamp(now: Millis) -> String {
    let secs = i64::try_from(now.0 / 1000).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let time = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (time / 3600, (time / 60) % 60, time % 60);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// The civil date `days` after 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Records `org` as the last good org, without touching `org.toml`.
pub fn mark_last_good(paths: &Paths, org: &Org) -> Result<(), StoreError> {
    write_atomic(&paths.org_last_good(), render(org)?.as_bytes(), None)
}
