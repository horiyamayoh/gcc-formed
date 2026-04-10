use crate::SnapshotSubset;
use crate::commands::corpus::run_replay;
use crate::util::process::run;
use std::path::Path;

pub(crate) fn run_check() -> Result<(), Box<dyn std::error::Error>> {
    run("cargo", &["fmt", "--check"])?;
    run("cargo", &["test", "--workspace"])?;
    run_replay(
        Path::new("corpus"),
        None,
        None,
        SnapshotSubset::Representative,
        None,
    )?;
    run(
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
    )?;
    Ok(())
}
