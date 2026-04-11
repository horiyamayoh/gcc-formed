use crate::SnapshotSubset;
use crate::commands::corpus::run_replay;
use crate::util::process::run;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CheckStep {
    Command(&'static str, &'static [&'static str]),
    RepresentativeReplay,
}

fn standard_check_steps() -> Vec<CheckStep> {
    vec![
        CheckStep::Command("cargo", &["fmt", "--check"]),
        CheckStep::Command(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        CheckStep::Command("cargo", &["test", "--workspace"]),
        CheckStep::RepresentativeReplay,
        CheckStep::Command(
            "python3",
            &[
                "-B",
                "-m",
                "unittest",
                "discover",
                "-s",
                "ci",
                "-p",
                "test_*.py",
            ],
        ),
    ]
}

pub(crate) fn run_check() -> Result<(), Box<dyn std::error::Error>> {
    for step in standard_check_steps() {
        match step {
            CheckStep::Command(binary, args) => run(binary, args)?,
            CheckStep::RepresentativeReplay => run_replay(
                Path::new("corpus"),
                None,
                None,
                SnapshotSubset::Representative,
                None,
            )?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CheckStep, standard_check_steps};

    #[test]
    fn standard_gate_runs_clippy_in_the_primary_check_path() {
        assert_eq!(
            standard_check_steps(),
            vec![
                CheckStep::Command("cargo", &["fmt", "--check"]),
                CheckStep::Command(
                    "cargo",
                    &[
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--",
                        "-D",
                        "warnings"
                    ],
                ),
                CheckStep::Command("cargo", &["test", "--workspace"]),
                CheckStep::RepresentativeReplay,
                CheckStep::Command(
                    "python3",
                    &[
                        "-B",
                        "-m",
                        "unittest",
                        "discover",
                        "-s",
                        "ci",
                        "-p",
                        "test_*.py",
                    ],
                ),
            ]
        );
    }
}
