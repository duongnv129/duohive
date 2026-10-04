//! DuoHive binary entry point.

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use duohive::args::{Cli, parse_args};
use duohive::hook_cmd::run_hook;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `hook` runs inside an agent's CLI: whatever its arguments, it exits 0 silently.
    if args.first().is_some_and(|a| a == "hook") {
        let stdin = std::io::stdin();
        let is_terminal = stdin.is_terminal();
        return run_hook(&args, stdin, is_terminal, &std::env::vars().collect());
    }
    match parse_args(&args) {
        // The app arrives in Task 29.
        Ok(Cli::App | Cli::Hook { .. }) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive: {e}");
            ExitCode::from(2)
        }
    }
}
