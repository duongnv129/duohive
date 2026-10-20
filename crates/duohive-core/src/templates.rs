//! First-run templates (UI §4.3): the tree each one builds and the org a first
//! run writes. Pure: directories are checked by the UI's probe, never here.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::cli_kind::CliKind;
use crate::names::{AgentName, NodeName};
use crate::org::{AgentDef, DuoConfig, Fields, Org, Role, Team};
use crate::validate::FieldError;

/// The squad's default size (UI §4.3).
pub const SQUAD_DEFAULT: u32 = 3;
/// The smallest squad (UI §4.3).
pub const SQUAD_MIN: u32 = 1;
/// The largest squad (UI §4.3).
pub const SQUAD_MAX: u32 = 100;
/// The Product Team's default name.
pub const PRODUCT_NAME: &str = "Product";

/// Starter prompts for the template roles (P4-Q1): Generalist, Backend,
/// Frontend and QA.
pub const STARTER_PROMPTS: [(&str, &str); 4] = [
    (
        "Generalist",
        "You are a careful engineer. Keep changes small, and run the tests before you say you are done.",
    ),
    (
        "Backend",
        "You own the backend. Keep the API and data model consistent, and run the tests before you say you are done.",
    ),
    (
        "Frontend",
        "You own the frontend. Match the design system, keep the UI accessible, and run the tests before you say you are done.",
    ),
    (
        "QA",
        "You own quality. Test the work you are given against its task, and report what fails with a reproduction.",
    ),
];

/// What a first run (or a new org) is built from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateInput {
    /// One Generalist role under Duo with `size` agents.
    AgentSquad(SquadInput),
    /// One team with Backend, Frontend and QA.
    ProductTeam(ProductInput),
    /// Several teams, each with Backend and QA.
    EngineeringOrg(EngineeringInput),
    /// You and Duo only.
    Blank,
}

/// Agent Squad's answers (UI §4.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SquadInput {
    /// How many agents (1–100).
    pub size: u32,
    /// Their shared working directory.
    pub directory: String,
    /// Their CLI.
    pub cli: CliKind,
    /// Whether new work gets its own worktree.
    pub worktrees: bool,
    /// The role's prompt; `None` keeps the Generalist starter prompt.
    pub prompt: Option<String>,
}

/// Product Team's answers (UI §4.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductInput {
    /// The team's name.
    pub team_name: String,
    /// The roles' CLI.
    pub cli: CliKind,
    /// The backend, frontend and QA directories, in that order.
    pub directories: [String; 3],
    /// Whether new work gets its own worktree.
    pub worktrees: bool,
}

/// Engineering Org's answers (UI §4.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineeringInput {
    /// The teams' CLI.
    pub cli: CliKind,
    /// Whether new work gets its own worktree.
    pub worktrees: bool,
    /// Each team's name and repo, in the order typed; empty rows are ignored.
    pub teams: Vec<(String, String)>,
}

/// The teams and root roles a template creates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrgTree {
    /// Teams, in order.
    pub teams: Vec<Team>,
    /// Roles directly under Duo, in order.
    pub roles: Vec<Role>,
}

/// Why a template can't be built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    /// A squad size outside 1–100.
    #[error("a squad is 1–100 agents")]
    SizeOutOfRange,
    /// Two teams with one name.
    #[error("{0} already has a team")]
    DuplicateTeam(String),
    /// Engineering Org with no row filled in.
    #[error("add at least one team")]
    NoTeams,
    /// A field the form must fix first.
    #[error("{}", .0.first().map_or_else(|| "invalid template".to_owned(), ToString::to_string))]
    Invalid(Vec<FieldError>),
}

impl TemplateError {
    /// The first field error, for a form to show.
    pub fn first_field(&self) -> Option<&FieldError> {
        match self {
            TemplateError::Invalid(errors) => errors.first(),
            _ => None,
        }
    }
}

fn field(path: &str, message: impl Into<String>) -> TemplateError {
    TemplateError::Invalid(vec![FieldError {
        path: path.to_owned(),
        message: message.into(),
    }])
}

/// A directory as it is stored: trimmed, and required.
fn directory(path: &str, raw: &str) -> Result<String, TemplateError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(field(path, "choose a directory"));
    }
    Ok(trimmed.to_owned())
}

/// A team or role name as stored: trimmed, and required.
fn node_name(path: &str, raw: &str) -> Result<NodeName, TemplateError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(field(path, "a name is required"));
    }
    NodeName::parse(trimmed).map_err(|e| field(path, e.to_string()))
}

/// `base-N`, skipping every number already in `taken`.
fn free_numbered(base: &str, taken: &mut BTreeSet<AgentName>) -> AgentName {
    for n in 1..=u32::MAX {
        let candidate = format!("{base}-{n}");
        if let Ok(name) = AgentName::parse(&candidate)
            && taken.insert(name.clone())
        {
            return name;
        }
    }
    AgentName::parse(&format!("{base}-1")).unwrap_or_else(|_| AgentName::duo())
}

/// `base`, or `base-2`, `base-3`… when `taken` already holds it.
fn free_plain(base: &str, taken: &mut BTreeSet<AgentName>) -> AgentName {
    for n in 1..=u32::MAX {
        let candidate = if n == 1 {
            base.to_owned()
        } else {
            format!("{base}-{n}")
        };
        if let Ok(name) = AgentName::parse(&candidate)
            && taken.insert(name.clone())
        {
            return name;
        }
    }
    AgentName::parse(base).unwrap_or_else(|_| AgentName::duo())
}

/// The starter prompt for `role`.
fn starter(role: &str) -> Option<String> {
    STARTER_PROMPTS
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, text)| (*text).to_owned())
}

/// A role's defaults for a template.
fn role_fields(cli: CliKind, directory: &str, worktrees: bool, prompt: Option<String>) -> Fields {
    Fields {
        cli: Some(cli),
        flags: None,
        directory: Some(directory.to_owned()),
        model: None,
        effort: None,
        worktree: Some(worktrees),
        prompt,
    }
}

/// A team's defaults for a template.
fn team_fields(cli: CliKind, worktrees: bool) -> Fields {
    Fields {
        cli: Some(cli),
        flags: None,
        directory: None,
        model: None,
        effort: None,
        worktree: Some(worktrees),
        prompt: None,
    }
}

/// One role with a single agent.
fn one_agent(
    role_name: &str,
    cli: CliKind,
    directory: &str,
    worktrees: bool,
    prompt: &str,
    base: &str,
    taken: &mut BTreeSet<AgentName>,
) -> Result<Role, TemplateError> {
    Ok(Role {
        name: node_name("role", role_name)?,
        defaults: role_fields(cli, directory, worktrees, starter(prompt)),
        agents: vec![AgentDef {
            name: free_plain(base, taken),
            fields: Fields::default(),
        }],
    })
}

/// A lower-case, identifier-safe form of a team name, for its agents' names.
fn slug(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "team".to_owned()
    } else {
        slug
    }
}

impl OrgTree {
    /// Builds `input`'s tree; `kept_names` still have worktree rows, so a
    /// generated name skips them.
    pub fn build(
        input: &TemplateInput,
        kept_names: &BTreeSet<AgentName>,
    ) -> Result<OrgTree, TemplateError> {
        let mut taken = kept_names.clone();
        match input {
            TemplateInput::AgentSquad(s) => build_squad(s, &mut taken),
            TemplateInput::ProductTeam(p) => build_product(p, &mut taken),
            TemplateInput::EngineeringOrg(e) => build_engineering(e, &mut taken),
            TemplateInput::Blank => Ok(OrgTree {
                teams: Vec::new(),
                roles: Vec::new(),
            }),
        }
    }
}

fn build_squad(s: &SquadInput, taken: &mut BTreeSet<AgentName>) -> Result<OrgTree, TemplateError> {
    let size = usize::try_from(s.size).map_err(|_| TemplateError::SizeOutOfRange)?;
    if !(SQUAD_MIN..=SQUAD_MAX).contains(&s.size) {
        return Err(TemplateError::SizeOutOfRange);
    }
    let directory = directory("directory", &s.directory)?;
    let prompt = s
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map_or_else(|| starter("Generalist"), |p| Some(p.to_owned()));
    let agents = (0..size)
        .map(|_| AgentDef {
            name: free_numbered("squad", taken),
            fields: Fields::default(),
        })
        .collect();
    Ok(OrgTree {
        teams: Vec::new(),
        roles: vec![Role {
            name: node_name("role", "Generalist")?,
            defaults: role_fields(s.cli, &directory, s.worktrees, prompt),
            agents,
        }],
    })
}

fn build_product(
    p: &ProductInput,
    taken: &mut BTreeSet<AgentName>,
) -> Result<OrgTree, TemplateError> {
    let name = node_name("team", &p.team_name)?;
    let dirs = [
        directory("backend directory", &p.directories[0])?,
        directory("frontend directory", &p.directories[1])?,
        directory("qa directory", &p.directories[2])?,
    ];
    let roles = vec![
        one_agent(
            "Backend",
            p.cli,
            &dirs[0],
            p.worktrees,
            "Backend",
            "backend",
            taken,
        )?,
        one_agent(
            "Frontend",
            p.cli,
            &dirs[1],
            p.worktrees,
            "Frontend",
            "frontend",
            taken,
        )?,
        one_agent("QA", p.cli, &dirs[2], p.worktrees, "QA", "qa", taken)?,
    ];
    Ok(OrgTree {
        teams: vec![Team {
            name,
            defaults: team_fields(p.cli, p.worktrees),
            roles,
        }],
        roles: Vec::new(),
    })
}

fn build_engineering(
    e: &EngineeringInput,
    taken: &mut BTreeSet<AgentName>,
) -> Result<OrgTree, TemplateError> {
    let mut teams: Vec<Team> = Vec::new();
    for (i, (name, repo)) in e.teams.iter().enumerate() {
        let (name, repo) = (name.trim(), repo.trim());
        if name.is_empty() && repo.is_empty() {
            continue;
        }
        if name.is_empty() {
            return Err(field(&format!("team[{i}].name"), "a team name is required"));
        }
        if repo.is_empty() {
            return Err(field(&format!("team[{i}].repo"), "choose a directory"));
        }
        let node = node_name("team", name)?;
        if teams.iter().any(|t| t.name == node) {
            return Err(TemplateError::DuplicateTeam(node.to_string()));
        }
        let slug = slug(name);
        let roles = vec![
            one_agent(
                "Backend",
                e.cli,
                repo,
                e.worktrees,
                "Backend",
                &format!("{slug}-backend"),
                taken,
            )?,
            one_agent(
                "QA",
                e.cli,
                repo,
                e.worktrees,
                "QA",
                &format!("{slug}-qa"),
                taken,
            )?,
        ];
        teams.push(Team {
            name: node,
            defaults: team_fields(e.cli, e.worktrees),
            roles,
        });
    }
    if teams.is_empty() {
        return Err(TemplateError::NoTeams);
    }
    Ok(OrgTree {
        teams,
        roles: Vec::new(),
    })
}

/// The org a first run writes: `[org]` holds only `cli`, and `[duo]` is Duo on
/// the same CLI.
pub fn first_run_org(duo_cli: CliKind, tree: OrgTree) -> Org {
    Org {
        defaults: Org::blank(duo_cli).defaults,
        duo: DuoConfig {
            name: AgentName::duo(),
            cli: duo_cli,
        },
        teams: tree.teams,
        roles: tree.roles,
    }
}

/// Builds `input`'s org for a first run.
pub fn first_run_for(
    duo_cli: CliKind,
    input: &TemplateInput,
    kept: &BTreeSet<AgentName>,
) -> Result<Org, TemplateError> {
    Ok(first_run_org(duo_cli, OrgTree::build(input, kept)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::validate;

    fn kept(names: &[&str]) -> BTreeSet<AgentName> {
        names
            .iter()
            .map(|n| AgentName::parse(n).expect("a name"))
            .collect()
    }

    fn squad(size: u32) -> TemplateInput {
        TemplateInput::AgentSquad(SquadInput {
            size,
            directory: " ~/code/app ".into(),
            cli: CliKind::Claude,
            worktrees: true,
            prompt: None,
        })
    }

    fn product() -> TemplateInput {
        TemplateInput::ProductTeam(ProductInput {
            team_name: "Product".into(),
            cli: CliKind::Claude,
            directories: [
                "~/code/api".into(),
                "~/code/web".into(),
                "~/code/api".into(),
            ],
            worktrees: false,
        })
    }

    fn engineering() -> TemplateInput {
        TemplateInput::EngineeringOrg(EngineeringInput {
            cli: CliKind::Opencode,
            worktrees: true,
            teams: vec![
                ("Payments".into(), "~/code/payments".into()),
                (String::new(), String::new()),
                ("Growth".into(), "~/code/growth".into()),
            ],
        })
    }

    fn agent_names(tree: &OrgTree) -> Vec<String> {
        let mut names: Vec<String> = tree
            .teams
            .iter()
            .flat_map(|t| t.roles.iter())
            .flat_map(|r| r.agents.iter())
            .map(|a| a.name.to_string())
            .collect();
        names.extend(
            tree.roles
                .iter()
                .flat_map(|r| r.agents.iter())
                .map(|a| a.name.to_string()),
        );
        names
    }

    #[test]
    fn squad_has_n_agents_under_root_role() {
        let tree = OrgTree::build(&squad(3), &BTreeSet::new()).expect("a tree");
        assert!(tree.teams.is_empty());
        assert_eq!(tree.roles.len(), 1);
        let role = &tree.roles[0];
        assert_eq!(role.name.as_str(), "Generalist");
        let names: Vec<String> = role.agents.iter().map(|a| a.name.to_string()).collect();
        assert_eq!(names, ["squad-1", "squad-2", "squad-3"]);
    }

    #[test]
    fn squad_worktrees_on() {
        let tree = OrgTree::build(&squad(3), &BTreeSet::new()).expect("a tree");
        assert_eq!(tree.roles[0].defaults.worktree, Some(true));
        let org = first_run_org(CliKind::Claude, tree);
        assert!(!org.defaults.worktree);
    }

    #[test]
    fn squad_directory_is_trimmed() {
        let tree = OrgTree::build(&squad(1), &BTreeSet::new()).expect("a tree");
        assert_eq!(
            tree.roles[0].defaults.directory.as_deref(),
            Some("~/code/app")
        );
    }

    #[test]
    fn product_team_three_roles_with_prompts() {
        let tree = OrgTree::build(&product(), &BTreeSet::new()).expect("a tree");
        assert!(tree.roles.is_empty());
        assert_eq!(tree.teams.len(), 1);
        let team = &tree.teams[0];
        assert_eq!(team.name.as_str(), "Product");
        let roles: Vec<&str> = team.roles.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(roles, ["Backend", "Frontend", "QA"]);
        for role in &team.roles {
            assert_eq!(role.agents.len(), 1);
            assert!(role.defaults.prompt.is_some(), "{} has a prompt", role.name);
        }
        assert_eq!(agent_names(&tree), ["backend", "frontend", "qa"]);
    }

    #[test]
    fn product_team_name_required_and_unique() {
        let input = ProductInput {
            team_name: "  ".into(),
            cli: CliKind::Claude,
            directories: ["a".into(), "b".into(), "c".into()],
            worktrees: false,
        };
        assert!(matches!(
            OrgTree::build(&TemplateInput::ProductTeam(input.clone()), &BTreeSet::new()),
            Err(TemplateError::Invalid(_))
        ));
        let dup = EngineeringInput {
            cli: CliKind::Claude,
            worktrees: false,
            teams: vec![
                ("Same".into(), "~/code/a".into()),
                ("Same".into(), "~/code/b".into()),
            ],
        };
        assert_eq!(
            OrgTree::build(&TemplateInput::EngineeringOrg(dup), &BTreeSet::new()),
            Err(TemplateError::DuplicateTeam("Same".into()))
        );
    }

    #[test]
    fn engineering_org_skips_empty_rows() {
        let tree = OrgTree::build(&engineering(), &BTreeSet::new()).expect("a tree");
        let names: Vec<&str> = tree.teams.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Payments", "Growth"]);
        for team in &tree.teams {
            let roles: Vec<&str> = team.roles.iter().map(|r| r.name.as_str()).collect();
            assert_eq!(roles, ["Backend", "QA"]);
            assert!(team.roles.iter().all(|r| r.agents.len() == 1));
        }
        assert_eq!(
            agent_names(&tree),
            [
                "payments-backend",
                "payments-qa",
                "growth-backend",
                "growth-qa"
            ]
        );
    }

    #[test]
    fn engineering_org_needs_one_team() {
        let empty = EngineeringInput {
            cli: CliKind::Claude,
            worktrees: false,
            teams: vec![(String::new(), String::new())],
        };
        assert_eq!(
            OrgTree::build(&TemplateInput::EngineeringOrg(empty), &BTreeSet::new()),
            Err(TemplateError::NoTeams)
        );
    }

    #[test]
    fn templates_validate() {
        for input in [squad(3), product(), engineering(), TemplateInput::Blank] {
            let org = first_run_for(CliKind::Claude, &input, &BTreeSet::new()).expect("an org");
            assert_eq!(validate(&org), Vec::new(), "{input:?}");
        }
    }

    #[test]
    fn template_agent_names_unique() {
        let mut dup = engineering();
        if let TemplateInput::EngineeringOrg(e) = &mut dup {
            e.teams.push(("Payments".into(), "~/code/other".into()));
        }
        assert!(OrgTree::build(&dup, &BTreeSet::new()).is_err());
    }

    #[test]
    fn templates_write_only_org_cli() {
        for input in [squad(3), product(), engineering(), TemplateInput::Blank] {
            let org = first_run_for(CliKind::Opencode, &input, &BTreeSet::new()).expect("an org");
            assert_eq!(org.defaults, Org::blank(CliKind::Opencode).defaults);
            assert_eq!(org.duo.cli, CliKind::Opencode);
        }
    }

    #[test]
    fn blank_writes_org_cli_and_duo() {
        let org = first_run_for(CliKind::Claude, &TemplateInput::Blank, &BTreeSet::new())
            .expect("an org");
        assert_eq!(org, Org::blank(CliKind::Claude));
    }

    #[test]
    fn squad_size_bounds() {
        assert_eq!(
            OrgTree::build(&squad(0), &BTreeSet::new()),
            Err(TemplateError::SizeOutOfRange)
        );
        assert_eq!(
            OrgTree::build(&squad(101), &BTreeSet::new()),
            Err(TemplateError::SizeOutOfRange)
        );
        assert_eq!(
            agent_names(&OrgTree::build(&squad(1), &BTreeSet::new()).expect("a tree")).len(),
            1
        );
        assert_eq!(
            agent_names(&OrgTree::build(&squad(100), &BTreeSet::new()).expect("a tree")).len(),
            100
        );
    }

    #[test]
    fn template_names_skip_kept_names() {
        let kept = kept(&["squad-1", "backend", "payments-backend"]);
        let tree = OrgTree::build(&squad(3), &kept).expect("a tree");
        let names = agent_names(&tree);
        assert_eq!(names[0], "squad-2");
        let tree = OrgTree::build(&product(), &kept).expect("a tree");
        assert!(agent_names(&tree).contains(&"backend-2".to_owned()));
        let tree = OrgTree::build(&engineering(), &kept).expect("a tree");
        assert!(
            agent_names(&tree).contains(&"payments-backend-2".to_owned()),
            "{:?}",
            agent_names(&tree)
        );
    }
}
