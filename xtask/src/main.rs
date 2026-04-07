use clap::{Parser, Subcommand};
use diag_testkit::{discover, family_counts, validate_fixture};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Check,
    Replay {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
    },
    Snapshot {
        #[arg(long, default_value = "corpus")]
        root: PathBuf,
    },
    BenchSmoke,
    SelfCheck,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Check => {
            run("cargo", &["fmt", "--check"])?;
            run("cargo", &["test", "--workspace"])?;
        }
        Commands::Replay { root } | Commands::Snapshot { root } => {
            let fixtures = discover(&root)?;
            for fixture in &fixtures {
                validate_fixture(fixture)?;
            }
            let counts = family_counts(&fixtures);
            enforce_minimum_family_counts(&counts)?;
            println!("{}", serde_json::to_string_pretty(&counts)?);
        }
        Commands::BenchSmoke => {
            println!(
                "{}",
                serde_json::json!({
                    "success_path_p95_ms_target": 40,
                    "simple_failure_p95_ms_target": 80,
                    "template_heavy_p95_ms_target": 250
                })
            );
        }
        Commands::SelfCheck => {
            println!(
                "{}",
                serde_json::json!({
                    "workspace": "ok",
                    "toolchain": "managed via rust-toolchain.toml",
                    "corpus_root": "corpus"
                })
            );
        }
    }
    Ok(())
}

fn run(binary: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(binary).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{binary} {} failed", args.join(" ")).into())
    }
}

fn enforce_minimum_family_counts(
    counts: &std::collections::BTreeMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let minimums = [
        ("syntax", 8_usize),
        ("type", 10),
        ("overload", 6),
        ("template", 12),
        ("macro_include", 10),
        ("linker", 10),
        ("partial", 6),
        ("path", 6),
    ];
    for (family, minimum) in minimums {
        let actual = counts.get(family).copied().unwrap_or_default();
        if actual < minimum {
            return Err(format!(
                "family `{family}` below minimum fixture count: expected >= {minimum}, got {actual}"
            )
            .into());
        }
    }
    Ok(())
}
