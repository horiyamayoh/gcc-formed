mod args;
mod backend;
mod config;
mod execute;
mod mode;
mod render;
mod self_check;
use std::process::ExitCode;

fn main() -> ExitCode {
    execute::entrypoint()
}
