use clap::ValueEnum;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub(crate) enum CiWorkflow {
    Pr,
    Nightly,
    Rc,
}

impl CiWorkflow {
    pub(crate) fn as_cli_value(self) -> &'static str {
        match self {
            Self::Pr => "pr",
            Self::Nightly => "nightly",
            Self::Rc => "rc",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub(crate) enum CiMatrixLane {
    All,
    Gcc12,
    Gcc13,
    Gcc14,
    Gcc15,
}

impl CiMatrixLane {
    pub(crate) fn as_cli_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Gcc12 => "gcc12",
            Self::Gcc13 => "gcc13",
            Self::Gcc14 => "gcc14",
            Self::Gcc15 => "gcc15",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CiGateCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate should live under the repo root")
        .to_path_buf()
}

pub(crate) fn build_ci_gate_command(
    workflow: CiWorkflow,
    report_dir: Option<&PathBuf>,
    matrix_lane: Option<CiMatrixLane>,
) -> Result<CiGateCommand, Box<dyn std::error::Error>> {
    if workflow != CiWorkflow::Nightly && matrix_lane.is_some() {
        return Err("--matrix-lane is only supported for `nightly`".into());
    }

    let repo_root = repo_root();
    let mut args = vec![
        repo_root.join("ci/run_local_gate.py").display().to_string(),
        "--workflow".to_string(),
        workflow.as_cli_value().to_string(),
    ];

    if let Some(report_dir) = report_dir {
        args.push("--report-dir".to_string());
        args.push(report_dir.display().to_string());
    }
    if let Some(matrix_lane) = matrix_lane {
        args.push("--matrix-lane".to_string());
        args.push(matrix_lane.as_cli_value().to_string());
    }

    Ok(CiGateCommand {
        program: "python3".to_string(),
        args,
        cwd: repo_root,
    })
}

pub(crate) fn run_ci_gate(
    workflow: CiWorkflow,
    report_dir: Option<PathBuf>,
    matrix_lane: Option<CiMatrixLane>,
) -> Result<(), Box<dyn std::error::Error>> {
    let command = build_ci_gate_command(workflow, report_dir.as_ref(), matrix_lane)?;
    let status = Command::new(&command.program)
        .current_dir(&command.cwd)
        .args(&command.args)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{} {} failed", command.program, command.args.join(" ")).into())
    }
}
