use crate::util::process::run;

pub(crate) fn run_check() -> Result<(), Box<dyn std::error::Error>> {
    run("cargo", &["fmt", "--check"])?;
    run("cargo", &["test", "--workspace"])?;
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
