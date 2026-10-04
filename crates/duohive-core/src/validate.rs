//! Validation of the org tree and per-agent problems.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use crate::inherit::ResolvedAgent;
use crate::names::{AgentName, NodeName};
use crate::org::{Fields, Org, Role};

/// Longest agent name (D4).
const MAX_NAME_LEN: usize = 40;
/// Longest slug once names would overflow (D11).
const SLUG_CUT: usize = 16;

/// A problem with one field, addressed by its path in the org.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    /// Field path, e.g. `team[Payments].role[Backend].agent[billing-api].flags`.
    pub path: String,
    /// What is wrong, in a sentence.
    pub message: String,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for FieldError {}

/// What a probe found at a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirStatus {
    /// A directory inside a git repository.
    GitRepo,
    /// A directory that exists.
    Found,
    /// Nothing there.
    Missing,
}

/// Looks at directories; implemented outside the core, which does no I/O.
pub trait DirProbe: Send + Sync {
    /// The status of `dir`.
    fn probe(&self, dir: &Path) -> DirStatus;
}

fn err(path: String, message: String) -> FieldError {
    FieldError { path, message }
}

fn check_flags(path: &str, fields: &Fields, out: &mut Vec<FieldError>) {
    let Some(flags) = fields.flags.as_deref() else {
        return;
    };
    if let Err(e) = shell_words::split(flags) {
        out.push(err(format!("{path}.flags"), format!("flags: {e}")));
    }
}

fn check_agent_name(
    path: &str,
    name: &AgentName,
    seen: &mut HashSet<AgentName>,
    out: &mut Vec<FieldError>,
) {
    if !seen.insert(name.clone()) {
        out.push(err(
            format!("{path}.name"),
            format!("an agent named {name} already exists"),
        ));
    }
}

fn check_roles(
    prefix: &str,
    parent: &str,
    roles: &[Role],
    seen: &mut HashSet<AgentName>,
    out: &mut Vec<FieldError>,
) {
    let mut names = HashSet::with_capacity(roles.len());
    for role in roles {
        let path = format!("{prefix}role[{}]", role.name);
        if !names.insert(&role.name) {
            out.push(err(
                format!("{path}.name"),
                format!("a role named {} already exists in {parent}", role.name),
            ));
        }
        check_flags(&path, &role.defaults, out);
        for agent in &role.agents {
            let apath = format!("{path}.agent[{}]", agent.name);
            check_agent_name(&apath, &agent.name, seen, out);
            check_flags(&apath, &agent.fields, out);
        }
    }
}

/// Blocking problems: the org is not used while any remain.
pub fn validate(org: &Org) -> Vec<FieldError> {
    let mut out = Vec::new();
    let mut seen = HashSet::from([org.duo.name.clone()]);
    let mut teams = HashSet::with_capacity(org.teams.len());
    for team in &org.teams {
        let path = format!("team[{}]", team.name);
        if !teams.insert(&team.name) {
            out.push(err(
                format!("{path}.name"),
                format!("a team named {} already exists", team.name),
            ));
        }
        check_flags(&path, &team.defaults, &mut out);
        let prefix = format!("{path}.");
        check_roles(
            &prefix,
            team.name.as_str(),
            &team.roles,
            &mut seen,
            &mut out,
        );
    }
    check_roles(
        "",
        &org.duo.name.to_string(),
        &org.roles,
        &mut seen,
        &mut out,
    );
    out
}

/// A non-blocking problem with one agent, for its row.
///
/// `dir` is the caller's probe of the expanded directory; `None` means not
/// probed yet (D26), when only a missing directory setting can be reported.
/// The message quotes the directory as written, so `home` is not needed.
pub fn agent_problem(r: &ResolvedAgent, _home: &Path, dir: Option<DirStatus>) -> Option<String> {
    let Some(written) = r.directory.as_deref() else {
        return Some("no directory set · e to fix".to_string());
    };
    match dir {
        Some(DirStatus::Missing) => Some(format!("✕ {written} not found · e to fix")),
        _ => None,
    }
}

/// Lowercase, spaces to `-`, other characters dropped, hyphen runs collapsed
/// and ends trimmed (D11).
fn slug(name: &NodeName) -> String {
    let mut out = String::with_capacity(name.as_str().len());
    for c in name.as_str().chars() {
        let c = match c {
            c if c.is_whitespace() || c == '-' => '-',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            _ => continue,
        };
        if c != '-' || !out.ends_with('-') {
            out.push(c);
        }
    }
    out.trim_matches('-').to_string()
}

fn cut(slug: &str) -> String {
    let short: String = slug.chars().take(SLUG_CUT).collect();
    short.trim_end_matches('-').to_string()
}

fn candidate(base: &str, k: u32) -> String {
    if base.is_empty() {
        k.to_string()
    } else {
        format!("{base}-{k}")
    }
}

/// Names for `count` new agents in a role (D11), skipping names in use.
pub fn next_agent_names(
    org: &Org,
    team: Option<&NodeName>,
    role: &NodeName,
    count: u8,
) -> Vec<AgentName> {
    let slugs: Vec<String> = team
        .into_iter()
        .chain([role])
        .map(slug)
        .filter(|s| !s.is_empty())
        .collect();
    let full = slugs.join("-");
    let short = slugs
        .iter()
        .map(|s| cut(s))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let taken: HashSet<&AgentName> = org
        .agents()
        .into_iter()
        .map(|a| &a.agent.name)
        .chain([&org.duo.name])
        .collect();
    (1u32..)
        .filter_map(|k| {
            let name = candidate(&full, k);
            let name = if name.len() > MAX_NAME_LEN {
                candidate(&short, k)
            } else {
                name
            };
            AgentName::parse(&name).ok()
        })
        .filter(|n| !taken.contains(n))
        .take(usize::from(count))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inherit::{expand_home, resolve};
    use crate::names::{AgentName, NodeName};
    use crate::org::{AgentDef, Fields, Org, Role, Team};
    use crate::testkit::{FakeDirs, example_org, n};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;

    fn nn(s: &str) -> NodeName {
        NodeName::parse(s).unwrap()
    }

    fn fe(path: &str, message: &str) -> FieldError {
        FieldError {
            path: path.into(),
            message: message.into(),
        }
    }

    fn role(name: &str, agents: &[&str]) -> Role {
        Role {
            name: nn(name),
            defaults: Fields::default(),
            agents: agents
                .iter()
                .map(|a| AgentDef {
                    name: n(a),
                    fields: Fields::default(),
                })
                .collect(),
        }
    }

    fn team(name: &str, roles: Vec<Role>) -> Team {
        Team {
            name: nn(name),
            defaults: Fields::default(),
            roles,
        }
    }

    #[test]
    fn duplicate_agent_names_rejected() {
        let mut org = example_org();
        org.teams[0].roles[1].agents[0].name = n("auth-service");
        assert_eq!(
            validate(&org),
            [fe(
                "team[Payments].role[QA].agent[auth-service].name",
                "an agent named auth-service already exists"
            )]
        );
    }

    #[test]
    fn agent_may_not_take_duos_name() {
        let mut org = example_org();
        org.roles[0].agents[0].name = AgentName::duo();
        assert_eq!(
            validate(&org),
            [fe(
                "role[Generalist].agent[Duo].name",
                "an agent named Duo already exists"
            )]
        );
    }

    #[test]
    fn duplicate_team_and_sibling_role_names_rejected() {
        let mut org = example_org();
        org.teams.push(team("Payments", vec![]));
        assert_eq!(
            validate(&org),
            [fe(
                "team[Payments].name",
                "a team named Payments already exists"
            )]
        );
        let mut org = example_org();
        org.teams[0].roles.push(role("QA", &[]));
        assert_eq!(
            validate(&org),
            [fe(
                "team[Payments].role[QA].name",
                "a role named QA already exists in Payments"
            )]
        );
        let mut org = example_org();
        org.roles.push(role("Generalist", &[]));
        assert_eq!(
            validate(&org),
            [fe(
                "role[Generalist].name",
                "a role named Generalist already exists in Duo"
            )]
        );
    }

    #[test]
    fn same_role_name_in_different_teams_is_fine() {
        let mut org = Org::blank(crate::cli_kind::CliKind::Claude);
        org.teams = vec![
            team("Payments", vec![role("Backend", &["pay-be"])]),
            team("Growth", vec![role("Backend", &["growth-be"])]),
        ];
        assert!(validate(&org).is_empty());
    }

    #[test]
    fn example_org_is_valid() {
        assert!(validate(&example_org()).is_empty());
    }

    #[test]
    fn bad_flags_are_reported_at_the_level_that_owns_them() {
        let mut org = example_org();
        org.teams[0].defaults.flags = Some("'".into());
        org.teams[0].roles[0].defaults.flags = Some("'".into());
        org.teams[0].roles[0].agents[1].fields.flags = Some("'".into());
        org.roles[0].defaults.flags = Some("'".into());
        let paths: Vec<_> = validate(&org).into_iter().map(|e| e.path).collect();
        assert_eq!(
            paths,
            [
                "team[Payments].flags",
                "team[Payments].role[Backend].flags",
                "team[Payments].role[Backend].agent[billing-api].flags",
                "role[Generalist].flags",
            ]
        );
    }

    #[test]
    fn field_error_displays_path_and_message() {
        let e = fe(
            "team[Payments].name",
            "a team named Payments already exists",
        );
        assert_eq!(
            e.to_string(),
            "team[Payments].name: a team named Payments already exists"
        );
        let boxed: Box<dyn std::error::Error> = Box::new(e);
        assert!(boxed.to_string().contains("Payments"));
    }

    const HOME: &str = "/h";

    fn probe_all(org: &Org, dirs: &FakeDirs) -> Vec<(String, Option<String>)> {
        org.agents()
            .iter()
            .map(|a| {
                let r = resolve(org, &a.agent.name).unwrap();
                let status = r
                    .directory
                    .as_deref()
                    .map(|d| dirs.probe(&expand_home(d, Path::new(HOME))));
                (
                    r.name.to_string(),
                    agent_problem(&r, Path::new(HOME), status),
                )
            })
            .collect()
    }

    #[test]
    fn missing_directory_invalidates_only_that_agent() {
        let mut org = example_org();
        org.defaults.cli = crate::cli_kind::CliKind::Claude;
        org.teams[0].defaults.directory = Some("~/code/pay".into());
        org.roles[0].defaults.directory = Some("/srv/squad".into());
        org.teams[0].roles[0].agents[1].fields.directory = Some("~/code/billing".into());
        let dirs = FakeDirs::new(HashMap::from([(
            PathBuf::from("/h/code/billing"),
            DirStatus::Missing,
        )]));
        let got = probe_all(&org, &dirs);
        assert_eq!(dirs.1.load(Ordering::SeqCst), 6);
        for (name, problem) in got {
            if name == "billing-api" {
                assert_eq!(
                    problem.as_deref(),
                    Some("✕ ~/code/billing not found · e to fix")
                );
            } else {
                assert_eq!(problem, None, "{name}");
            }
        }
    }

    #[test]
    fn git_repo_and_found_directories_are_fine() {
        let mut org = example_org();
        org.defaults.cli = crate::cli_kind::CliKind::Claude;
        org.teams[0].roles[0].agents[1].fields.directory = Some("/x".into());
        let r = resolve(&org, &n("billing-api")).unwrap();
        for s in [DirStatus::GitRepo, DirStatus::Found] {
            assert_eq!(agent_problem(&r, Path::new(HOME), Some(s)), None);
        }
    }

    #[test]
    fn no_directory_reported() {
        let org = example_org();
        let r = resolve(&org, &n("billing-api")).unwrap();
        assert_eq!(r.directory, None);
        assert_eq!(
            agent_problem(&r, Path::new(HOME), None),
            Some("no directory set · e to fix".into())
        );
        assert_eq!(
            agent_problem(&r, Path::new(HOME), Some(DirStatus::Found)),
            Some("no directory set · e to fix".into())
        );
        let duo = resolve(&org, &AgentName::duo()).unwrap();
        assert_eq!(agent_problem(&duo, Path::new(HOME), None), None);
    }

    #[test]
    fn pending_directory_reports_no_problem() {
        let mut org = example_org();
        org.teams[0].roles[0].agents[1].fields.directory = Some("~/code/billing".into());
        let r = resolve(&org, &n("billing-api")).unwrap();
        assert_eq!(agent_problem(&r, Path::new(HOME), None), None);
    }

    fn frontend_org() -> Org {
        let mut org = Org::blank(crate::cli_kind::CliKind::Claude);
        org.teams = vec![
            team("Payments", vec![role("Frontend", &["payments-frontend-1"])]),
            team("Growth", vec![role("Frontend", &[])]),
        ];
        org.roles = vec![role("Frontend", &[])];
        org
    }

    #[test]
    fn next_agent_names_skip_existing() {
        let org = frontend_org();
        let names = |t: Option<&str>, c: u8| -> Vec<String> {
            let t = t.map(nn);
            next_agent_names(&org, t.as_ref(), &nn("Frontend"), c)
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        assert_eq!(
            names(Some("Payments"), 2),
            ["payments-frontend-2", "payments-frontend-3"]
        );
        assert_eq!(names(Some("Growth"), 1), ["growth-frontend-1"]);
        assert_eq!(names(None, 1), ["frontend-1"]);
        assert!(names(None, 0).is_empty());
    }

    #[test]
    fn next_agent_names_fit_forty_characters() {
        let org = Org::blank(crate::cli_kind::CliKind::Claude);
        let got = next_agent_names(
            &org,
            Some(&nn("Customer Success Operations")),
            &nn("Platform Reliability Engineering"),
            1,
        );
        assert_eq!(
            got.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["customer-success-platform-reliabi-1"]
        );
    }

    fn first_name(team: &str, role: &str) -> String {
        let org = Org::blank(crate::cli_kind::CliKind::Claude);
        next_agent_names(&org, Some(&nn(team)), &nn(role), 1)[0].to_string()
    }

    #[test]
    fn forty_character_name_stays_uncut() {
        let got = first_name("abcdefghijklmnopqrs", "abcdefghijklmnopqr");
        assert_eq!(got.len(), 40);
        assert_eq!(got, "abcdefghijklmnopqrs-abcdefghijklmnopqr-1");
    }

    #[test]
    fn forty_one_character_name_is_cut() {
        let got = first_name("abcdefghijklmnopqrs", "abcdefghijklmnopqrs");
        assert_eq!(got, "abcdefghijklmnop-abcdefghijklmnop-1");
    }

    #[test]
    fn count_is_always_met() {
        let org = Org::blank(crate::cli_kind::CliKind::Claude);
        let got = next_agent_names(&org, Some(&nn("a-b")), &nn("c"), 255);
        assert_eq!(got.len(), 255);
    }

    #[test]
    fn slugs_strip_and_collapse_hyphens() {
        let org = Org::blank(crate::cli_kind::CliKind::Claude);
        let got = next_agent_names(&org, Some(&nn("+Growth  Ops")), &nn("Front - End"), 1);
        assert_eq!(
            got.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["growth-ops-front-end-1"]
        );
    }

    #[test]
    fn empty_slug_is_left_out_with_its_separator() {
        let org = Org::blank(crate::cli_kind::CliKind::Claude);
        let got = next_agent_names(&org, Some(&nn("+++")), &nn("QA"), 1);
        assert_eq!(got[0].as_str(), "qa-1");
    }

    #[test]
    fn next_agent_names_skip_duo() {
        let mut org = Org::blank(crate::cli_kind::CliKind::Claude);
        org.duo.name = n("duo-1");
        let got = next_agent_names(&org, None, &nn("Duo"), 1);
        assert_eq!(got[0].as_str(), "duo-2");
    }
}
