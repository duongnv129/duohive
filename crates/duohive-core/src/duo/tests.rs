//! Tests for Duo's prompt: Task 3.5 (TD §6.4; P3-D13).

use super::DUO_PROMPT;
use crate::inherit::resolve;
use crate::testkit::{example_org, n};
use crate::tool::ToolName;

#[test]
fn duo_prompt_has_required_points() {
    for point in [
        "card",
        "pending",
        "[message from",
        "DuoHive boundary",
        "commit, push, merge, publish and delete",
    ] {
        assert!(DUO_PROMPT.contains(point), "missing {point:?}");
    }
    for tool in ToolName::ALL {
        assert!(
            DUO_PROMPT.contains(tool.as_str()),
            "missing {}",
            tool.as_str()
        );
    }
}

#[test]
fn duo_prompt_under_org_prompt() {
    let mut org = example_org();
    org.defaults.prompt = Some("O".into());
    let duo = resolve(&org, &n("Duo")).unwrap().prompt;
    assert_eq!(duo, format!("# Org\nO\n\n# Duo (DuoHive)\n{DUO_PROMPT}"));
    // With no org prompt, Duo's own heading comes first.
    org.defaults.prompt = None;
    let duo = resolve(&org, &n("Duo")).unwrap().prompt;
    assert_eq!(duo, format!("# Duo (DuoHive)\n{DUO_PROMPT}"));
    org.defaults.prompt = Some("O".into());
    for agent in org.agents() {
        let prompt = resolve(&org, &agent.agent.name).unwrap().prompt;
        assert!(!prompt.contains("# Duo (DuoHive)"), "{prompt}");
        assert!(!prompt.contains(DUO_PROMPT.trim()), "{prompt}");
    }
}
