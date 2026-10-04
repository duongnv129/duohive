//! Quoting of arguments for a tmux control-mode command line.

/// A tmux argument that can't be quoted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuoteError {
    /// A newline would end the control-mode command.
    #[error("a tmux command argument can't contain a newline")]
    Newline,
}

/// Single-quotes `arg` so tmux's parser reads it literally (`'` becomes `'\''`).
///
/// # Errors
/// Returns [`QuoteError::Newline`] when `arg` contains a newline.
pub fn tmux_quote(arg: &str) -> Result<String, QuoteError> {
    if arg.contains('\n') {
        return Err(QuoteError::Newline);
    }
    Ok(format!("'{}'", arg.replace('\'', r"'\''")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_quote_escapes_single_quotes() {
        assert_eq!(tmux_quote("it's").unwrap(), r"'it'\''s'");
    }

    #[test]
    fn tmux_quote_rejects_newlines() {
        assert_eq!(tmux_quote("a\nb"), Err(QuoteError::Newline));
    }

    #[test]
    fn hostile_text_is_kept_literal() {
        assert_eq!(
            tmux_quote("$(echo hi) `x` \"q\" ; #{pane} \\n").unwrap(),
            "'$(echo hi) `x` \"q\" ; #{pane} \\n'"
        );
        assert_eq!(tmux_quote("-x héllo 日本").unwrap(), "'-x héllo 日本'");
        assert_eq!(tmux_quote("").unwrap(), "''");
    }
}
