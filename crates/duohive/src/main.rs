//! DuoHive binary entry point.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::panic::{self, AssertUnwindSafe};
use std::process::ExitCode;

use duohive::args::{Cli, parse_args};
use duohive::hook_cmd::run_hook;

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
        // The app arrives in Task 29.
        Ok(Cli::App | Cli::Hook { .. }) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "duohive: {e}");
            ExitCode::from(2)
        }
    }
}

/// Runs the hook with a silent panic hook (helper threads included) and
/// turns any panic into a clean exit.
fn hook(args: &[String]) -> ExitCode {
    panic::set_hook(Box::new(|_| {}));
    panic::catch_unwind(AssertUnwindSafe(|| {
        let env = utf8_vars(std::env::vars_os());
        let stdin = std::io::stdin();
        let is_terminal = stdin.is_terminal();
        run_hook(args, stdin, is_terminal, &env)
    }))
    .unwrap_or(ExitCode::SUCCESS)
}

fn utf8_vars(vars: impl Iterator<Item = (OsString, OsString)>) -> HashMap<String, String> {
    vars.filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}
