//! DuoHive binary entry point.

use std::io::{IsTerminal, Write};
use std::panic::{self, AssertUnwindSafe};
use std::process::ExitCode;

use duohive::args::{Cli, USAGE, parse_args};
use duohive::bench::gate::{GateReport, git_dirty, git_head, run_child, run_gate};
use duohive::bench::{BenchArgs, BenchError, run_bench_detailed};
use duohive::fake_agent::run_bench_agent;
use duohive::hook_cmd::run_hook;
use duohive::mcp_cmd::run_mcp;
use duohive::startup::{EXIT_GRACE, block_on_then_exit, env_vars, run_app_main};

fn main() -> ExitCode {
    // `env::args()` and `env::vars()` panic on non-UTF-8: drop such entries instead.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .filter_map(|a| a.into_string().ok())
        .collect();
    // `hook` runs inside an agent's CLI: whatever happens, it exits 0 silently.
    if std::env::args_os().nth(1).is_some_and(|a| a == "hook") {
        return hook(&args);
    }
    match parse_args(&args) {
        Ok(Cli::App) => app(),
        // Handled above.
        Ok(Cli::Hook { .. }) => ExitCode::SUCCESS,
        Ok(Cli::Help) => {
            let _ = std::io::stdout().write_all(USAGE.as_bytes());
            ExitCode::SUCCESS
        }
        Ok(Cli::BenchAgent(args)) => run_bench_agent(&args, &env_vars()),
        Ok(Cli::Mcp) => run_mcp(&env_vars()),
        Ok(Cli::Version) => {
            let _ = writeln!(std::io::stdout(), "duohive {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Cli::Bench(args)) => bench(args),
        Ok(Cli::Gate { json }) => gate(json),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive: {e}");
            ExitCode::from(2)
        }
    }
}

/// The app, on a multi-thread runtime.
fn app() -> ExitCode {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => block_on_then_exit(rt, run_app_main(), EXIT_GRACE),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive: cannot start the runtime: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the bench and prints its report (plain, or JSON with `--json`).
fn bench(args: BenchArgs) -> ExitCode {
    let json = args.json;
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive bench: {e}");
            return ExitCode::FAILURE;
        }
    };
    match block_on_then_exit(rt, run_bench_detailed(args), EXIT_GRACE) {
        Ok(outcome) => {
            let text = if json {
                format!("{}\n", outcome.to_json())
            } else {
                outcome.to_string()
            };
            let _ = std::io::stdout().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            if json && let BenchError::LoadTooHigh { load } = &e {
                let out = serde_json::json!({ "discarded": "load", "load": load });
                let _ = writeln!(std::io::stdout(), "{out}");
            } else {
                let _ = writeln!(std::io::stderr(), "duohive bench: {e}");
            }
            ExitCode::from(bench_failure_code(&e))
        }
    }
}

/// Runs the release gate (`bench --gate`): exit 0 pass, 1 miss, 3 discarded
/// for load, 130 interrupted.
fn gate(json: bool) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive bench --gate: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut runner = |agents, view| run_child(&exe, agents, view);
    match run_gate(&mut runner) {
        Ok(tiers) => {
            let report = GateReport {
                tiers,
                version: env!("CARGO_PKG_VERSION").to_owned(),
                git_sha: git_head(),
                git_dirty: git_dirty(),
                binary_bytes: std::fs::metadata(&exe).ok().map(|m| m.len()),
            };
            let text = if json {
                format!("{}\n", report.to_json())
            } else {
                report.text()
            };
            let _ = std::io::stdout().write_all(text.as_bytes());
            ExitCode::from(report.exit_code())
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive bench --gate: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

/// 130 when interrupted; 3 for a run the gate discards (busy machine); else 1.
fn bench_failure_code(e: &BenchError) -> u8 {
    match e {
        BenchError::Interrupted => 130,
        BenchError::LoadTooHigh { .. } => 3,
        _ => 1,
    }
}

/// Runs the hook with a silent panic hook (helper threads included) and
/// turns any panic into a clean exit.
fn hook(args: &[String]) -> ExitCode {
    panic::set_hook(Box::new(|_| {}));
    panic::catch_unwind(AssertUnwindSafe(|| {
        let env = env_vars();
        let stdin = std::io::stdin();
        let is_terminal = stdin.is_terminal();
        run_hook(args, stdin, is_terminal, &env)
    }))
    .unwrap_or(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupted_bench_exits_130() {
        assert_eq!(bench_failure_code(&BenchError::Interrupted), 130);
        assert_eq!(bench_failure_code(&BenchError::Timeout("agents")), 1);
    }

    #[test]
    fn load_above_ceiling_exits_3_discarded() {
        let e = BenchError::LoadTooHigh { load: 7.25 };
        assert_eq!(bench_failure_code(&e), 3);
        assert_eq!(e.to_string(), "discarded: load 7.25");
    }
}
