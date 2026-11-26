#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wild_err_arm,
    reason = "test-only helpers"
)]

use std::path::PathBuf;

use duohive_core::cli_kind::CliKind;
use duohive_core::colour::TeamColour;
use duohive_core::names::NodeName;
use duohive_core::org::{AgentDef, Fields, Org, Role, Team};
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::ports::OrgFileError;
use duohive_core::testkit::{example_org, n};
use duohive_core::time::Millis;
use duohive_store::{Paths, backup_stamp, load_org, mark_last_good, replace_org, save_org};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// `org` as a save writes it and a load reads it back: every effective colour spelled out.
fn fixed(mut org: Org) -> Org {
    org.fix_colours();
    org
}

fn temp_paths() -> (tempfile::TempDir, Paths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
        config_dir: tmp.path().join("config"),
        data_dir: tmp.path().join("data"),
        tmux_socket: "duohive".into(),
    };
    (tmp, paths)
}

fn full_org() -> Org {
    let fields = Fields {
        cli: Some(CliKind::Opencode),
        flags: Some("--verbose".into()),
        directory: Some("~/code/auth".into()),
        model: Some("sonnet".into()),
        effort: Some("high".into()),
        worktree: Some(true),
        prompt: Some("API URLs use \"hyphens\".\nSecond line.".into()),
    };
    let mut org = Org::blank(CliKind::Claude);
    org.defaults.model = Some("opus".into());
    org.defaults.effort = Some("low".into());
    org.defaults.worktree = true;
    org.defaults.prompt = Some("Ask before you delete anything.".into());
    org.duo.cli = CliKind::Opencode;
    org.teams = vec![Team {
        name: NodeName::parse("Payments").unwrap(),
        defaults: fields.clone(),
        roles: vec![Role {
            name: NodeName::parse("Backend").unwrap(),
            defaults: fields.clone(),
            agents: vec![AgentDef {
                name: n("auth-service"),
                fields: fields.clone(),
            }],
            color: None,
        }],
        color: None,
    }];
    org.roles = vec![Role {
        name: NodeName::parse("Generalist").unwrap(),
        defaults: Fields::default(),
        agents: vec![AgentDef {
            name: n("squad-1"),
            fields: Fields {
                worktree: Some(false),
                ..Fields::default()
            },
        }],
        color: None,
    }];
    org
}

#[test]
fn parses_prd_example_org() {
    assert_eq!(
        load_org(&fixture("org-example.toml")).unwrap().unwrap(),
        example_org()
    );
}

#[test]
fn save_then_load_round_trips() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(fixed(example_org())));
}

#[test]
fn every_field_round_trips() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &full_org()).unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(fixed(full_org())));
}

#[test]
fn unknown_cli_reports_line() {
    assert_eq!(
        load_org(&fixture("org-bad-cli.toml")).unwrap_err(),
        OrgFileError {
            line: Some(42),
            message: "unknown CLI \"claud\"".into()
        }
    );
}

#[test]
fn agent_under_agent_is_rejected() {
    let err = load_org(&fixture("org-nested-agent.toml")).unwrap_err();
    assert_eq!(err.line, Some(17));
    assert!(err.message.contains("agent"), "{}", err.message);
}

#[test]
fn unknown_top_level_key_is_rejected() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    std::fs::write(
        p.org_toml(),
        "[org]\ncli = \"claude\"\n[duo]\nname = \"Duo\"\ncli = \"claude\"\n\n[[squad]]\nname = \"x\"\n",
    )
    .unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap_err().line, Some(7));
}

#[test]
fn bad_agent_name_reports_line() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    std::fs::write(
        p.org_toml(),
        "[org]\ncli = \"claude\"\n[duo]\nname = \"Duo\"\ncli = \"claude\"\n[[role]]\nname = \"R\"\n[[role.agent]]\nname = \"bad name\"\n",
    )
    .unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap_err().line, Some(9));
}

#[test]
fn missing_org_file_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(load_org(&tmp.path().join("org.toml")).unwrap(), None);
}

#[test]
fn save_keeps_previous_version_as_backup() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    save_org(&p, &full_org()).unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(fixed(full_org())));
    assert_eq!(
        load_org(&p.org_backup()).unwrap(),
        Some(fixed(example_org()))
    );
    assert!(!p.config_dir.join("org.toml.tmp").exists());
}

#[test]
fn save_refreshes_last_good() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    assert_eq!(
        load_org(&p.org_last_good()).unwrap(),
        Some(fixed(example_org()))
    );
    save_org(&p, &full_org()).unwrap();
    assert_eq!(
        load_org(&p.org_last_good()).unwrap(),
        Some(fixed(full_org()))
    );
}

#[test]
fn loading_never_touches_last_good() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    std::fs::write(p.org_toml(), "this is not toml [").unwrap();
    assert!(load_org(&p.org_toml()).is_err());
    assert_eq!(
        load_org(&p.org_last_good()).unwrap(),
        Some(fixed(example_org()))
    );
}

#[test]
fn mark_last_good_writes_only_last_good() {
    let (_tmp, p) = temp_paths();
    mark_last_good(&p, &full_org()).unwrap();
    assert_eq!(
        load_org(&p.org_last_good()).unwrap(),
        Some(fixed(full_org()))
    );
    assert!(!p.org_toml().exists());
}

#[test]
fn backup_name_is_timestamped_and_beside_org_toml() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    let now = Millis(1_000_000_000_000);
    replace_org(&p, &full_org(), now).unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(fixed(full_org())));
    let backup = p
        .config_dir
        .join(format!("org.toml.{}.bak", backup_stamp(now)));
    assert_eq!(backup.parent(), p.org_toml().parent());
    assert_eq!(load_org(&backup).unwrap(), Some(fixed(example_org())));
}

#[test]
fn backup_stamp_formats_utc() {
    assert_eq!(backup_stamp(Millis(0)), "19700101-000000");
    assert_eq!(backup_stamp(Millis(1_000_000_000_000)), "20010909-014640");
}

#[test]
fn timestamped_backup_kept() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    replace_org(&p, &full_org(), Millis(1_000_000_000_000)).unwrap();
    replace_org(&p, &example_org(), Millis(1_000_086_400_000)).unwrap();
    let first = p.config_dir.join("org.toml.20010909-014640.bak");
    let second = p.config_dir.join("org.toml.20010910-014640.bak");
    assert_eq!(load_org(&first).unwrap(), Some(fixed(example_org())));
    assert_eq!(load_org(&second).unwrap(), Some(fixed(full_org())));
}

#[test]
fn replace_org_with_no_file_saves_without_backup() {
    let (_tmp, p) = temp_paths();
    replace_org(&p, &full_org(), Millis(1_000_000_000_000)).unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(fixed(full_org())));
    assert!(!p.config_dir.join("org.toml.20010909-014640.bak").exists());
}

const TWO_TEAMS: &str = r#"[org]
cli = "claude"

[duo]
name = "Duo"
cli = "claude"

[[team]]
name = "Payments"
%A%
[[team]]
name = "Platform"
%B%
[[role]]
name = "Generalist"
%C%
"#;

fn two_teams(a: &str, b: &str, c: &str) -> String {
    TWO_TEAMS
        .replace("%A%", a)
        .replace("%B%", b)
        .replace("%C%", c)
}

fn team_colour(org: &Org, name: &str) -> Option<TeamColour> {
    org.colour_of(&NodeKey::Team(NodeName::parse(name).unwrap()))
}

#[test]
fn color_round_trips_on_a_team_and_a_root_role() {
    let (_tmp, p) = temp_paths();
    let mut org = example_org();
    org.teams[0].color = Some(TeamColour::Orchid);
    org.roles[0].color = Some(TeamColour::Slate);
    save_org(&p, &org).unwrap();
    let text = std::fs::read_to_string(p.org_toml()).unwrap();
    assert!(text.contains("color = \"orchid\"") && text.contains("color = \"slate\""));
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(org));
}

#[test]
fn color_on_a_nested_role_fails_with_its_line() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    let text = "[org]\ncli = \"claude\"\n\n[duo]\nname = \"Duo\"\ncli = \"claude\"\n\n[[team]]\nname = \"Payments\"\n\n[[team.role]]\nname = \"Backend\"\ncolor = \"teal\"\n";
    std::fs::write(p.org_toml(), text).unwrap();
    let err = load_org(&p.org_toml()).unwrap_err();
    assert_eq!(err.line, Some(13), "{err:?}");
    assert!(err.message.contains("color"), "{}", err.message);
}

#[test]
fn an_unknown_colour_gives_the_td_text_with_its_line() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    std::fs::write(p.org_toml(), two_teams("color = \"tael\"\n", "", "")).unwrap();
    let err = load_org(&p.org_toml()).unwrap_err();
    assert_eq!(err.line, Some(10), "{err:?}");
    assert_eq!(
        err.message,
        "unknown colour \"tael\" · teal, violet, slate or orchid"
    );
}

#[test]
fn an_existing_org_toml_without_color_still_loads() {
    let org = load_org(&fixture("org-example.toml")).unwrap().unwrap();
    assert!(org.teams.iter().all(|t| t.color.is_none()));
    for t in &org.teams {
        assert!(org.colour_of(&NodeKey::Team(t.name.clone())).is_some());
    }
}

#[test]
fn a_file_without_color_loads_with_derived_colours_and_is_not_rewritten() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    let text = two_teams("", "", "");
    std::fs::write(p.org_toml(), &text).unwrap();
    let org = load_org(&p.org_toml()).unwrap().unwrap();
    assert_eq!(team_colour(&org, "Payments"), Some(TeamColour::Teal));
    assert_eq!(team_colour(&org, "Platform"), Some(TeamColour::Violet));
    assert_eq!(std::fs::read_to_string(p.org_toml()).unwrap(), text);
}

#[test]
fn a_save_writes_every_effective_colour() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    std::fs::write(p.org_toml(), two_teams("", "", "")).unwrap();
    let org = load_org(&p.org_toml()).unwrap().unwrap();
    save_org(&p, &org).unwrap();
    let text = std::fs::read_to_string(p.org_toml()).unwrap();
    assert!(text.contains("color = \"teal\""), "{text}");
    assert!(text.contains("color = \"violet\""), "{text}");
    assert!(text.contains("color = \"slate\""), "{text}");
}

#[test]
fn two_teams_may_share_a_colour() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    let both = "color = \"slate\"\n";
    std::fs::write(p.org_toml(), two_teams(both, both, "")).unwrap();
    let org = load_org(&p.org_toml()).unwrap().unwrap();
    assert_eq!(team_colour(&org, "Payments"), Some(TeamColour::Slate));
    assert_eq!(team_colour(&org, "Platform"), Some(TeamColour::Slate));
}

#[test]
fn a_mixed_file_derives_missing_colours_by_position() {
    let (_tmp, p) = temp_paths();
    std::fs::create_dir_all(&p.config_dir).unwrap();
    std::fs::write(p.org_toml(), two_teams("color = \"violet\"\n", "", "")).unwrap();
    let org = load_org(&p.org_toml()).unwrap().unwrap();
    assert_eq!(team_colour(&org, "Payments"), Some(TeamColour::Violet));
    // Platform is the second sibling: ALL[1 % 4], violet too (TD §7).
    assert_eq!(team_colour(&org, "Platform"), Some(TeamColour::Violet));
    let key = NodeKey::Role(RoleKey {
        team: None,
        role: NodeName::parse("Generalist").unwrap(),
    });
    assert_eq!(org.colour_of(&key), Some(TeamColour::Slate));
}
