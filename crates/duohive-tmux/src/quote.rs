//! Quoting of arguments for a tmux control-mode command line.

/// A tmux argument that can't be quoted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuoteError {
    /// A newline would end the control-mode command.
    #[error("a tmux command argument can't contain a newline")]
    Newline,
    /// A CR ends or corrupts a control-mode line; NUL truncates tmux's C string.
    #[error("a tmux command argument can't contain the control character {0:?}")]
    ControlChar(char),
}

/// Single-quotes `arg` so tmux's parser reads it literally (`'` becomes `'\''`).
///
/// Quoting does not stop tmux's option parsing of a leading `-`: callers put `--`
/// before positional text. Nor does it stop format expansion of `#{` in formatted
/// arguments: callers double `#` to `##` for `-n`, `-c` and `display-message`.
///
/// # Errors
/// Returns [`QuoteError::Newline`] when `arg` contains a newline.
pub fn tmux_quote(arg: &str) -> Result<String, QuoteError> {
    if arg.contains('\n') {
        return Err(QuoteError::Newline);
    }
    if let Some(c) = arg.chars().find(|c| matches!(c, '\r' | '\0')) {
        return Err(QuoteError::ControlChar(c));
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
    fn tmux_quote_rejects_cr_and_nul() {
        assert_eq!(tmux_quote("a\rb"), Err(QuoteError::ControlChar('\r')));
        assert_eq!(tmux_quote("a\0b"), Err(QuoteError::ControlChar('\0')));
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
