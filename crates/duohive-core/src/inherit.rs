//! Inheritance of settings and stacking of prompts down the org tree.

use std::path::{Path, PathBuf};

use crate::cli_kind::CliKind;
use crate::names::{AgentName, NodeName};
use crate::org::{Fields, NodeKey, Org, RoleKey};
use crate::validate::FieldError;

/// Duo's working directory (D5): the user's home.
const DUO_DIRECTORY: &str = "~";
/// Label for values that come from the org defaults.
const ORG_LABEL: &str = "Org";

/// An agent with every setting resolved, ready to launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAgent {
    /// The agent's name.
    pub name: AgentName,
    /// Whether this is Duo.
    pub is_duo: bool,
    /// Which CLI to run.
    pub cli: CliKind,
    /// Extra CLI flags, split like a shell.
    pub flags: Vec<String>,
    /// Working directory as written (may start with `~`).
    pub directory: Option<String>,
    /// Model, if any level sets one.
    pub model: Option<String>,
    /// Effort, if any level sets one.
    pub effort: Option<String>,
    /// Whether to work in a git worktree.
    pub worktree: bool,
    /// The stacked prompt.
    pub prompt: String,
}

/// One level of the prompt stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptLevel {
    /// The org.
    Org,
    /// A team.
    Team(NodeName),
    /// A role.
    Role(NodeName),
    /// An agent.
    Agent(AgentName),
}

impl PromptLevel {
    fn heading(&self) -> String {
        match self {
            PromptLevel::Org => "# Org".to_string(),
            PromptLevel::Team(n) => format!("# Team: {n}"),
            PromptLevel::Role(n) => format!("# Role: {n}"),
            PromptLevel::Agent(n) => format!("# Agent: {n}"),
        }
    }

    fn label(&self) -> String {
        match self {
            PromptLevel::Org => ORG_LABEL.to_string(),
            PromptLevel::Team(n) | PromptLevel::Role(n) => n.to_string(),
            PromptLevel::Agent(n) => n.to_string(),
        }
    }
}

/// A setting and the level it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InheritedValue<T> {
    /// The value.
    pub value: T,
    /// `"Org"`, a team name or a role name.
    pub from: String,
}

/// What a child of some node inherits, with sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inherited {
    /// CLI.
    pub cli: InheritedValue<CliKind>,
    /// Directory.
    pub directory: Option<InheritedValue<String>>,
    /// Model.
    pub model: Option<InheritedValue<String>>,
    /// Effort.
    pub effort: Option<InheritedValue<String>>,
    /// Flags, as typed.
    pub flags: Option<InheritedValue<String>>,
    /// Worktree.
    pub worktree: InheritedValue<bool>,
    /// Prompt per level, org first; `None` where the level has no prompt.
    pub prompts: Vec<(String, Option<String>)>,
}

/// The levels above (and including) a node, outermost first, org excluded.
fn levels<'a>(org: &'a Org, key: &NodeKey) -> Vec<(PromptLevel, &'a Fields)> {
    let team = |name: &NodeName| org.teams.iter().find(|t| &t.name == name);
    let role_in =
        |roles: &'a [crate::org::Role], name: &NodeName| roles.iter().find(|r| &r.name == name);
    let mut out = Vec::with_capacity(3);
    match key {
        NodeKey::Duo => {}
        NodeKey::Team(t) => {
            if let Some(t) = team(t) {
                out.push((PromptLevel::Team(t.name.clone()), &t.defaults));
            }
        }
        NodeKey::Role(RoleKey { team: tn, role }) => {
            let roles = match tn.as_ref().and_then(team) {
                Some(t) => {
                    out.push((PromptLevel::Team(t.name.clone()), &t.defaults));
                    &t.roles[..]
                }
                None => &org.roles[..],
            };
            if let Some(r) = role_in(roles, role) {
                out.push((PromptLevel::Role(r.name.clone()), &r.defaults));
            }
        }
        NodeKey::Agent(a) => {
            if let Some(loc) = org.find_agent(a) {
                if let Some(t) = loc.team {
                    out.push((PromptLevel::Team(t.name.clone()), &t.defaults));
                }
                out.push((PromptLevel::Role(loc.role.name.clone()), &loc.role.defaults));
                out.push((
                    PromptLevel::Agent(loc.agent.name.clone()),
                    &loc.agent.fields,
                ));
            }
        }
    }
    out
}

/// The nearest level that sets a value, with its label.
fn nearest<T>(
    levels: &[(PromptLevel, &Fields)],
    get: impl Fn(&Fields) -> Option<T>,
) -> Option<InheritedValue<T>> {
    levels.iter().rev().find_map(|(level, f)| {
        get(f).map(|value| InheritedValue {
            value,
            from: level.label(),
        })
    })
}

fn org_value<T>(value: Option<T>) -> Option<InheritedValue<T>> {
    value.map(|value| InheritedValue {
        value,
        from: ORG_LABEL.to_string(),
    })
}

/// Joins the non-blank levels, each under its heading (D3).
pub fn stack_prompt(levels: &[(PromptLevel, Option<&str>)]) -> String {
    levels
        .iter()
        .filter_map(|(level, text)| {
            let text = text.filter(|t| !t.trim().is_empty())?;
            Some(format!("{}\n{text}", level.heading()))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// What a child of `parent` inherits, with where each value comes from.
pub fn inherited_for(org: &Org, parent: &NodeKey) -> Inherited {
    let lv = levels(org, parent);
    let d = &org.defaults;
    let mut prompts = vec![(ORG_LABEL.to_string(), d.prompt.clone())];
    prompts.extend(lv.iter().map(|(l, f)| (l.label(), f.prompt.clone())));
    Inherited {
        cli: nearest(&lv, |f| f.cli).unwrap_or(InheritedValue {
            value: d.cli,
            from: ORG_LABEL.to_string(),
        }),
        directory: nearest(&lv, |f| f.directory.clone()),
        model: nearest(&lv, |f| f.model.clone()).or_else(|| org_value(d.model.clone())),
        effort: nearest(&lv, |f| f.effort.clone()).or_else(|| org_value(d.effort.clone())),
        flags: nearest(&lv, |f| f.flags.clone()),
        worktree: nearest(&lv, |f| f.worktree).unwrap_or(InheritedValue {
            value: d.worktree,
            from: ORG_LABEL.to_string(),
        }),
        prompts,
    }
}

/// Resolves every setting of `agent`, or names the field that is wrong.
pub fn resolve(org: &Org, agent: &AgentName) -> Result<ResolvedAgent, FieldError> {
    let d = &org.defaults;
    if org.is_duo(agent) {
        return Ok(ResolvedAgent {
            name: agent.clone(),
            is_duo: true,
            cli: org.duo.cli,
            flags: Vec::new(),
            directory: Some(DUO_DIRECTORY.to_string()),
            model: d.model.clone(),
            effort: d.effort.clone(),
            worktree: d.worktree,
            prompt: stack_prompt(&[(PromptLevel::Org, d.prompt.as_deref())]),
        });
    }
    let Some(loc) = org.find_agent(agent) else {
        return Err(FieldError {
            path: format!("agent[{agent}]"),
            message: "no such agent".to_string(),
        });
    };
    let lv = levels(org, &NodeKey::Agent(agent.clone()));
    let flags = match nearest(&lv, |f| f.flags.clone()) {
        Some(f) => shell_words::split(&f.value).map_err(|e| FieldError {
            path: format!("{}.flags", field_path(&loc)),
            message: format!("flags: {e}"),
        })?,
        None => Vec::new(),
    };
    let mut stack = vec![(PromptLevel::Org, d.prompt.as_deref())];
    stack.extend(lv.iter().map(|(l, f)| (l.clone(), f.prompt.as_deref())));
    Ok(ResolvedAgent {
        name: agent.clone(),
        is_duo: false,
        cli: nearest(&lv, |f| f.cli).map_or(d.cli, |v| v.value),
        flags,
        directory: nearest(&lv, |f| f.directory.clone()).map(|v| v.value),
        model: nearest(&lv, |f| f.model.clone()).map_or_else(|| d.model.clone(), |v| Some(v.value)),
        effort: nearest(&lv, |f| f.effort.clone())
            .map_or_else(|| d.effort.clone(), |v| Some(v.value)),
        worktree: nearest(&lv, |f| f.worktree).map_or(d.worktree, |v| v.value),
        prompt: stack_prompt(&stack),
    })
}

fn field_path(loc: &crate::org::AgentLoc<'_>) -> String {
    let team = loc
        .team
        .map(|t| format!("team[{}].", t.name))
        .unwrap_or_default();
    format!("{team}role[{}].agent[{}]", loc.role.name, loc.agent.name)
}

/// Expands a leading `~` or `~/` against `home`; anything else is unchanged.
pub fn expand_home(dir: &str, home: &Path) -> PathBuf {
    if dir == "~" {
        return home.to_path_buf();
    }
    match dir.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::{Fields, RoleKey};
    use crate::testkit::{example_org, n};
    use std::path::{Path, PathBuf};

    fn nn(s: &str) -> NodeName {
        NodeName::parse(s).unwrap()
    }

    fn set_prompts(org: &mut Org) {
        org.defaults.prompt = Some("Ask before you delete anything.".into());
        org.teams[0].roles[0].defaults.prompt = Some("API URLs use hyphens.".into());
        org.teams[0].roles[0].agents[1].fields.prompt = Some("Owns billing.".into());
    }

    fn billing_mut(org: &mut Org) -> &mut Fields {
        &mut org.teams[0].roles[0].agents[1].fields
    }

    #[test]
    fn prompts_stack_in_order_with_headings() {
        let mut org = example_org();
        set_prompts(&mut org);
        assert_eq!(
            resolve(&org, &n("billing-api")).unwrap().prompt,
            "# Org\nAsk before you delete anything.\n\n# Role: Backend\nAPI URLs use hyphens.\n\n# Agent: billing-api\nOwns billing."
        );
    }

    #[test]
    fn stack_prompt_includes_team_and_skips_blank_levels() {
        let out = stack_prompt(&[
            (PromptLevel::Org, Some("  \n")),
            (PromptLevel::Team(nn("Payments")), Some("Be careful.")),
            (PromptLevel::Role(nn("Backend")), None),
        ]);
        assert_eq!(out, "# Team: Payments\nBe careful.");
        assert_eq!(stack_prompt(&[]), "");
    }

    #[test]
    fn nearest_value_wins() {
        let mut org = example_org();
        org.defaults.model = Some("haiku".into());
        org.teams[0].defaults.model = Some("sonnet".into());
        org.teams[0].roles[0].defaults.model = Some("opus".into());
        assert_eq!(
            resolve(&org, &n("billing-api")).unwrap().model.as_deref(),
            Some("opus")
        );
        assert_eq!(
            resolve(&org, &n("pay-qa")).unwrap().model.as_deref(),
            Some("sonnet")
        );
        billing_mut(&mut org).model = Some("fable".into());
        assert_eq!(
            resolve(&org, &n("billing-api")).unwrap().model.as_deref(),
            Some("fable")
        );
        assert_eq!(
            resolve(&org, &n("squad-1")).unwrap().model.as_deref(),
            Some("haiku")
        );
    }

    #[test]
    fn org_defaults_are_the_last_resort() {
        let mut org = example_org();
        org.defaults.worktree = true;
        let r = resolve(&org, &n("billing-api")).unwrap();
        assert_eq!(r.cli, org.defaults.cli);
        assert!(r.worktree);
        assert_eq!(r.directory, None);
        assert!(r.flags.is_empty());
        assert!(!r.is_duo);
    }

    #[test]
    fn root_role_skips_team_level() {
        let mut org = example_org();
        org.defaults.prompt = Some("O".into());
        org.roles[0].defaults.prompt = Some("R".into());
        org.roles[0].agents[0].fields.prompt = Some("A".into());
        assert_eq!(
            resolve(&org, &n("squad-1")).unwrap().prompt,
            "# Org\nO\n\n# Role: Generalist\nR\n\n# Agent: squad-1\nA"
        );
    }

    #[test]
    fn duo_resolves_from_org_defaults_and_home() {
        let mut org = example_org();
        org.duo.cli = crate::cli_kind::CliKind::Opencode;
        org.defaults.prompt = Some("O".into());
        let r = resolve(&org, &n("Duo")).unwrap();
        assert!(r.is_duo);
        assert_eq!(r.cli, org.duo.cli);
        assert_eq!(r.directory.as_deref(), Some("~"));
        assert_eq!(r.prompt, "# Org\nO");
    }

    #[test]
    fn flags_split_like_a_shell() {
        let mut org = example_org();
        billing_mut(&mut org).flags = Some("--verbose --name 'a b'".into());
        assert_eq!(
            resolve(&org, &n("billing-api")).unwrap().flags,
            ["--verbose", "--name", "a b"]
        );
    }

    #[test]
    fn bad_flags_name_the_field() {
        let mut org = example_org();
        billing_mut(&mut org).flags = Some("--x 'unterminated".into());
        let err = resolve(&org, &n("billing-api")).unwrap_err();
        assert_eq!(
            err.path,
            "team[Payments].role[Backend].agent[billing-api].flags"
        );
        let mut org = example_org();
        org.roles[0].agents[0].fields.flags = Some("'".into());
        assert_eq!(
            resolve(&org, &n("squad-1")).unwrap_err().path,
            "role[Generalist].agent[squad-1].flags"
        );
    }

    #[test]
    fn unknown_agent_is_a_field_error() {
        let err = resolve(&example_org(), &n("nobody")).unwrap_err();
        assert_eq!(err.path, "agent[nobody]");
    }

    #[test]
    fn inherited_for_role_in_team_reports_sources() {
        let mut org = example_org();
        set_prompts(&mut org);
        org.teams[0].defaults.directory = Some("~/pay".into());
        let key = NodeKey::Role(RoleKey {
            team: Some(nn("Payments")),
            role: nn("Backend"),
        });
        let got = inherited_for(&org, &key);
        assert_eq!(got.cli.from, "Org");
        assert_eq!(got.cli.value, org.defaults.cli);
        assert_eq!(
            got.directory.as_ref().map(|d| d.from.as_str()),
            Some("Payments")
        );
        assert_eq!(got.model, None);
        assert_eq!(got.flags, None);
        assert_eq!(got.worktree.from, "Org");
        assert_eq!(
            got.prompts,
            [
                (
                    "Org".to_string(),
                    Some("Ask before you delete anything.".to_string())
                ),
                ("Payments".to_string(), None),
                (
                    "Backend".to_string(),
                    Some("API URLs use hyphens.".to_string())
                ),
            ]
        );
    }

    #[test]
    fn inherited_for_duo_and_team() {
        let mut org = example_org();
        org.defaults.model = Some("haiku".into());
        org.teams[0].defaults.model = Some("sonnet".into());
        let duo = inherited_for(&org, &NodeKey::Duo);
        assert_eq!(
            duo.model.map(|m| (m.value, m.from)),
            Some(("haiku".into(), "Org".into()))
        );
        assert_eq!(duo.prompts, [("Org".to_string(), None)]);
        let team = inherited_for(&org, &NodeKey::Team(nn("Payments")));
        assert_eq!(
            team.model.map(|m| (m.value, m.from)),
            Some(("sonnet".into(), "Payments".into()))
        );
    }

    #[test]
    fn expand_home_only_leading_tilde() {
        let h = Path::new("/h");
        assert_eq!(expand_home("~/code", h), PathBuf::from("/h/code"));
        assert_eq!(expand_home("~", h), PathBuf::from("/h"));
        assert_eq!(expand_home("/a/~b", h), PathBuf::from("/a/~b"));
        assert_eq!(expand_home("~bob/x", h), PathBuf::from("~bob/x"));
    }
}
