//! Command-line parsing (hand-rolled; Tasks 27–29 add the rest).

/// What the command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cli {
    /// The app itself (no arguments).
    App,
    /// `duohive hook <agent> <event>`: forward one CLI hook.
    Hook {
        /// The agent's name.
        agent: String,
        /// DuoHive's event name.
        event: String,
    },
}

/// Parses the arguments (without the program name); `[]` is the app.
pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    match args {
        [] => Ok(Cli::App),
        [cmd, agent, event] if cmd == "hook" => Ok(Cli::Hook {
            agent: agent.clone(),
            event: event.clone(),
        }),
        [cmd, ..] if cmd == "hook" => Err("usage: duohive hook <agent> <event>".to_owned()),
        [other, ..] => Err(format!("unknown command \"{other}\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s<const N: usize>(items: [&str; N]) -> Vec<String> {
        items.iter().map(|i| (*i).to_owned()).collect()
    }

    #[test]
    fn args_parse_hook() {
        assert_eq!(
            parse_args(&s(["hook", "a", "busy"])),
            Ok(Cli::Hook {
                agent: "a".into(),
                event: "busy".into()
            })
        );
        assert!(parse_args(&s(["frob"])).is_err());
    }

    #[test]
    fn no_args_is_the_app() {
        assert_eq!(parse_args(&[]), Ok(Cli::App));
    }

    #[test]
    fn hook_needs_exactly_agent_and_event() {
        assert!(parse_args(&s(["hook", "a"])).is_err());
        assert!(parse_args(&s(["hook", "a", "busy", "extra"])).is_err());
    }
}
