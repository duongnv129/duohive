//! DuoHive binary internals: argument parsing, start-up and logging, the `hook`
//! subcommand, the hook listener, the fake agent, the headless system and the bench.

pub mod args;
pub mod bench;
pub mod fake_agent;
pub mod hook_cmd;
pub mod hook_listener;
pub mod logging;
pub mod real_ports;
pub mod startup;
pub mod system;

/// The longest `run.sock` line (without its newline), shared by `duohive hook`
/// (which shrinks a payload to fit) and the listener (which drops longer lines).
pub const MAX_HOOK_LINE: usize = 1024 * 1024;
