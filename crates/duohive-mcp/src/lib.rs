//! DuoHive's MCP side (TD §6): the endpoint behind `run.sock` and the
//! `duohive mcp` helper each agent runs.

pub mod calls;
pub mod endpoint;
pub mod protocol;
pub mod tools;

#[cfg(test)]
mod tests;
