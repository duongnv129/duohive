#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wild_err_arm,
    reason = "test-only helpers"
)]

use std::path::PathBuf;

use duohive_core::cli_kind::CliKind;
use duohive_core::names::NodeName;
use duohive_core::org::{AgentDef, Fields, Org, Role, Team};
use duohive_core::ports::OrgFileError;
use duohive_core::testkit::{example_org, n};
use duohive_store::{Paths, load_org, mark_last_good, save_org};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
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
        }],
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
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(example_org()));
}

#[test]
fn every_field_round_trips() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &full_org()).unwrap();
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(full_org()));
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
    assert_eq!(load_org(&p.org_toml()).unwrap(), Some(full_org()));
    assert_eq!(load_org(&p.org_backup()).unwrap(), Some(example_org()));
    assert!(!p.config_dir.join("org.toml.tmp").exists());
}

#[test]
fn save_refreshes_last_good() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    assert_eq!(load_org(&p.org_last_good()).unwrap(), Some(example_org()));
    save_org(&p, &full_org()).unwrap();
    assert_eq!(load_org(&p.org_last_good()).unwrap(), Some(full_org()));
}

#[test]
fn loading_never_touches_last_good() {
    let (_tmp, p) = temp_paths();
    save_org(&p, &example_org()).unwrap();
    std::fs::write(p.org_toml(), "this is not toml [").unwrap();
    assert!(load_org(&p.org_toml()).is_err());
    assert_eq!(load_org(&p.org_last_good()).unwrap(), Some(example_org()));
}

#[test]
fn mark_last_good_writes_only_last_good() {
    let (_tmp, p) = temp_paths();
    mark_last_good(&p, &full_org()).unwrap();
    assert_eq!(load_org(&p.org_last_good()).unwrap(), Some(full_org()));
    assert!(!p.org_toml().exists());
}
