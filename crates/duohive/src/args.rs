//! Command-line parsing (hand-rolled).

use crate::bench::BenchArgs;
use crate::fake_agent::BenchAgentArgs;

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
    /// `duohive bench-agent [--no-hooks] [--ignore-input] [--task <text>]` (hidden):
    /// the fake CLI.
    BenchAgent(BenchAgentArgs),
    /// `duohive bench [--agents N] [--idle-secs N] [--turns N] [--view] [--json]`.
    Bench(BenchArgs),
    /// `duohive --help` (or `-h`): print [`USAGE`].
    Help,
}

/// What `--help` prints (`bench-agent` is internal and left out).
pub const USAGE: &str = "\
usage: duohive                 run DuoHive (your agents keep running in tmux after you quit)
       duohive hook <agent> <event>
                               forward one CLI hook (agent CLIs call this; prints nothing)
       duohive bench [--agents 1-1000] [--idle-secs N] [--turns 1-1000] [--view] [--json]
                               measure CPU, memory and hook latency with fake agents
                               (--view holds one live pane stream open throughout)
       duohive --help          show this
";

/// Parses the arguments (without the program name); `[]` is the app.
pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    match args {
        [] => Ok(Cli::App),
        [flag] if flag == "--help" || flag == "-h" => Ok(Cli::Help),
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
    "usage: duohive bench [--agents 1-1000] [--idle-secs N] [--turns 1-1000] [--view] [--json]";

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
            "--view" => out.view = true,
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

const BENCH_USAGE: &str =
    "usage: duohive bench-agent [--no-hooks] [--ignore-input] [--task <text>]";

/// `[--no-hooks] [--ignore-input] [--task <text>]` in any order; the task is
/// taken verbatim, even when it starts with `-`.
fn parse_bench_agent(args: &[String]) -> Result<Cli, String> {
    let mut out = BenchAgentArgs {
        hooks: true,
        ..BenchAgentArgs::default()
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--no-hooks" if out.hooks => out.hooks = false,
            "--ignore-input" if !out.ignore_input => out.ignore_input = true,
            "--task" if out.task.is_none() => {
                out.task = Some(rest.next().ok_or(BENCH_USAGE)?.clone());
            }
            _ => return Err(BENCH_USAGE.to_owned()),
        }
    }
    Ok(Cli::BenchAgent(out))
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
        let agent = |hooks, ignore_input, task: Option<&str>| {
            Ok(Cli::BenchAgent(BenchAgentArgs {
                hooks,
                ignore_input,
                task: task.map(str::to_owned),
            }))
        };
        assert_eq!(
            parse_args(&s(["bench-agent", "--no-hooks"])),
            agent(false, false, None)
        );
        assert_eq!(
            parse_args(&s(["bench-agent", "--task", "-rf a b"])),
            agent(true, false, Some("-rf a b"))
        );
        assert_eq!(parse_args(&s(["bench-agent"])), agent(true, false, None));
        assert_eq!(
            parse_args(&s(["bench-agent", "--task", "t", "--ignore-input"])),
            agent(true, true, Some("t"))
        );
        assert!(parse_args(&s(["bench-agent", "--ignore-input", "--ignore-input"])).is_err());
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
                view: false,
                json: false
            }))
        );
        assert_eq!(
            parse_args(&s(["bench", "--view", "--agents", "10"])),
            Ok(Cli::Bench(BenchArgs {
                agents: 10,
                view: true,
                ..Default::default()
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
