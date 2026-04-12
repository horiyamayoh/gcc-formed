//! Main CLI entry point that wraps GCC with diagnostic enrichment and rendering.

mod args;
mod backend;
mod config;
mod error;
mod execute;
mod mode;
mod public_json;
mod render;
mod self_check;
mod trace_bundle;
use std::process::ExitCode;

fn main() -> ExitCode {
    execute::entrypoint()
}
