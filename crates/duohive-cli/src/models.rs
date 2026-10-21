//! Reading the model and effort lists out of each CLI's own output (TD §5.2).

use std::collections::BTreeMap;

use duohive_core::adapter::{ModelList, ModelListError};

/// The text of option `flag` in `claude --help`: its line and the indented lines that wrap it.
fn option_text(help: &str, flag: &str) -> Option<String> {
    let mut lines = help
        .lines()
        .skip_while(|l| !l.trim_start().starts_with(flag));
    let first = lines.next()?;
    let mut text = first.to_owned();
    for line in lines {
        // The next option starts with a dash at the usual two-space indent.
        if line.trim_start().starts_with('-') && line.len() - line.trim_start().len() <= 2 {
            break;
        }
        text.push(' ');
        text.push_str(line.trim());
    }
    Some(text)
}

/// Aliases and effort levels from `claude --help`: the quoted aliases in `--model`'s text and
/// the parenthesised choices of `--effort`.
pub fn parse_claude_help(text: &str) -> Result<ModelList, ModelListError> {
    let model = option_text(text, "--model")
        .ok_or_else(|| ModelListError::Parse("no --model option".to_owned()))?;
    let models: Vec<String> = model
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    let effort = option_text(text, "--effort")
        .ok_or_else(|| ModelListError::Parse("no --effort option".to_owned()))?;
    let efforts: Vec<String> = effort
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(choices, _)| choices.split(',').map(|c| c.trim().to_owned()).collect())
        .unwrap_or_default();
    if models.is_empty() || efforts.is_empty() {
        return Err(ModelListError::Parse(
            "no aliases or effort levels in --help".to_owned(),
        ));
    }
    Ok(ModelList {
        models,
        efforts,
        variants: BTreeMap::new(),
    })
}

/// `provider/model` ids, one per line, from `opencode models`. OpenCode 2.1.1 has no
/// `--verbose`, so it gives no variants: its efforts are typed (TD §5.2).
pub fn parse_opencode_models(text: &str) -> Result<ModelList, ModelListError> {
    let models: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    if models.is_empty() || models.iter().any(|m| !m.contains('/') || m.contains(' ')) {
        return Err(ModelListError::Parse(
            "expected provider/model ids, one per line".to_owned(),
        ));
    }
    Ok(ModelList {
        models,
        efforts: Vec::new(),
        variants: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &str = include_str!("../tests/fixtures/models/claude-help.txt");
    const OPENCODE: &str = include_str!("../tests/fixtures/models/opencode-models.txt");

    #[test]
    fn claude_help_gives_aliases_and_efforts() {
        let list = parse_claude_help(CLAUDE).unwrap();
        for alias in ["opus", "sonnet"] {
            assert!(list.models.iter().any(|m| m == alias), "{alias}");
        }
        assert_eq!(list.efforts, ["low", "medium", "high", "xhigh", "max"]);
    }

    #[test]
    fn claude_help_parse_failure_is_an_error() {
        assert!(matches!(
            parse_claude_help("Usage: claude\n  --print  Print\n"),
            Err(ModelListError::Parse(_))
        ));
    }

    #[test]
    fn opencode_models_lists_ids_with_no_variants() {
        let list = parse_opencode_models(OPENCODE).unwrap();
        assert!(list.models.len() > 100);
        assert!(list.models.iter().all(|m| m.contains('/')));
        assert!(list.efforts.is_empty() && list.variants.is_empty());
    }

    #[test]
    fn empty_output_is_an_error() {
        assert!(parse_opencode_models("").is_err());
        assert!(parse_opencode_models("\n  \n").is_err());
        assert!(parse_opencode_models("ERROR no server\n").is_err());
    }
}
