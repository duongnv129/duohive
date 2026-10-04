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
    /// `duohive bench-agent [--no-hooks] [--task <text>]` (hidden): the fake CLI.
    BenchAgent {
        /// Fire hooks (unless `--no-hooks`).
        hooks: bool,
        /// The first turn's task, verbatim.
        task: Option<String>,
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
        [cmd, rest @ ..] if cmd == "bench-agent" => parse_bench_agent(rest),
        [other, ..] => Err(format!("unknown command \"{other}\"")),
    }
}

const BENCH_USAGE: &str = "usage: duohive bench-agent [--no-hooks] [--task <text>]";

/// `[--no-hooks] [--task <text>]` in any order; the task is taken verbatim,
/// even when it starts with `-`.
fn parse_bench_agent(args: &[String]) -> Result<Cli, String> {
    let mut hooks = true;
    let mut task = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--no-hooks" if hooks => hooks = false,
            "--task" if task.is_none() => {
                task = Some(rest.next().ok_or(BENCH_USAGE)?.clone());
            }
            _ => return Err(BENCH_USAGE.to_owned()),
        }
    }
    Ok(Cli::BenchAgent { hooks, task })
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
    fn args_parse_bench_agent() {
        assert_eq!(
            parse_args(&s(["bench-agent", "--no-hooks"])),
            Ok(Cli::BenchAgent {
                hooks: false,
                task: None
            })
        );
        assert_eq!(
            parse_args(&s(["bench-agent", "--task", "-rf a b"])),
            Ok(Cli::BenchAgent {
                hooks: true,
                task: Some("-rf a b".into())
            })
        );
        assert_eq!(
            parse_args(&s(["bench-agent"])),
            Ok(Cli::BenchAgent {
                hooks: true,
                task: None
            })
        );
        assert!(parse_args(&s(["bench-agent", "--task"])).is_err());
        assert!(parse_args(&s(["bench-agent", "--frob"])).is_err());
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
