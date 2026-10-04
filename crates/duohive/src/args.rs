//! Command-line parsing (hand-rolled; Tasks 27–29 add the rest).

use crate::bench::BenchArgs;

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
    /// `duohive bench [--agents N] [--idle-secs N] [--turns N] [--json]`.
    Bench(BenchArgs),
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
        [cmd, rest @ ..] if cmd == "bench" => parse_bench(rest).map(Cli::Bench),
        [other, ..] => Err(format!("unknown command \"{other}\"")),
    }
}

const BENCH_RUN_USAGE: &str =
    "usage: duohive bench [--agents 1-1000] [--idle-secs N] [--turns 1-1000] [--json]";

/// The bench's flags in any order; counts must be numbers, agents and turns at least 1.
fn parse_bench(args: &[String]) -> Result<BenchArgs, String> {
    let mut out = BenchArgs::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut number = || -> Result<u64, String> {
            rest.next()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| BENCH_RUN_USAGE.to_owned())
        };
        match arg.as_str() {
            "--json" => out.json = true,
            "--agents" => out.agents = count(number()?)?,
            "--turns" => out.turns = count(number()?)?,
            "--idle-secs" => out.idle_secs = number()?,
            _ => return Err(BENCH_RUN_USAGE.to_owned()),
        }
    }
    Ok(out)
}

/// The most agents or turns a bench run takes.
const BENCH_MAX: u64 = 1000;

/// An agent or turn count: 1 to [`BENCH_MAX`].
fn count(n: u64) -> Result<u32, String> {
    u32::try_from(n)
        .ok()
        .filter(|n| (1..=BENCH_MAX as u32).contains(n))
        .ok_or_else(|| BENCH_RUN_USAGE.to_owned())
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
    use crate::bench::BenchArgs;

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
    fn args_parse_bench() {
        assert_eq!(
            parse_args(&s(["bench", "--agents", "10", "--json"])),
            Ok(Cli::Bench(BenchArgs {
                agents: 10,
                json: true,
                ..Default::default()
            }))
        );
        assert_eq!(
            parse_args(&s(["bench"])),
            Ok(Cli::Bench(BenchArgs::default()))
        );
        assert_eq!(
            parse_args(&s([
                "bench",
                "--idle-secs",
                "2",
                "--turns",
                "1",
                "--agents",
                "2"
            ])),
            Ok(Cli::Bench(BenchArgs {
                agents: 2,
                idle_secs: 2,
                turns: 1,
                json: false
            }))
        );
        assert!(parse_args(&s(["bench", "--agents"])).is_err());
        assert!(parse_args(&s(["bench", "--agents", "x"])).is_err());
        assert!(parse_args(&s(["bench", "--agents", "0"])).is_err());
        for bad in ["5000000000", "1001", "-1"] {
            let err = parse_args(&s(["bench", "--agents", bad])).unwrap_err();
            assert!(err.starts_with("usage: duohive bench"), "{bad}: {err}");
        }
        let err = parse_args(&s(["bench", "--turns", "1001"])).unwrap_err();
        assert!(err.starts_with("usage: duohive bench"), "{err}");
        assert!(parse_args(&s(["bench", "--frob"])).is_err());
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
