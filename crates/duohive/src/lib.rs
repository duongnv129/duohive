//! DuoHive binary internals: argument parsing, the `hook` subcommand and the hook listener.

pub mod args;
pub mod hook_cmd;
pub mod hook_listener;

/// The longest `run.sock` line (without its newline), shared by `duohive hook`
/// (which shrinks a payload to fit) and the listener (which drops longer lines).
pub const MAX_HOOK_LINE: usize = 1024 * 1024;
