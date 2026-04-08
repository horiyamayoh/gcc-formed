use std::env;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!(
        "cargo:rustc-env=FORMED_BUILD_PROFILE={}",
        env::var("PROFILE").unwrap_or_else(|_| "dev".to_string())
    );
    println!(
        "cargo:rustc-env=FORMED_TARGET={}",
        env::var("TARGET").unwrap_or_else(|_| "unknown-target".to_string())
    );
    println!(
        "cargo:rustc-env=FORMED_GIT_COMMIT={}",
        command_output("git", &["rev-parse", "--short=12", "HEAD"])
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "cargo:rustc-env=FORMED_RUSTC_VERSION={}",
        command_output(
            &env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string()),
            &["--version"]
        )
        .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "cargo:rustc-env=FORMED_CARGO_VERSION={}",
        command_output(
            &env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()),
            &["--version"]
        )
        .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "cargo:rustc-env=FORMED_BUILD_TIMESTAMP={}",
        env::var("SOURCE_DATE_EPOCH").unwrap_or_else(|_| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
    );
    println!(
        "cargo:rustc-env=FORMED_RELEASE_CHANNEL={}",
        env::var("FORMED_RELEASE_CHANNEL").unwrap_or_else(|_| "dev".to_string())
    );
}

fn command_output(binary: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(binary).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
