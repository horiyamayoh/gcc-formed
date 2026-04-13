#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from gate_catalog import (  # noqa: E402
    EXECUTION_CATALOG,
    NIGHTLY_LANES,
    build_execution_env,
    canonical_workflow_name,
    nightly_lane_names,
    plan_path_for_workflow,
    policy_skips_step,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run GitHub CI-equivalent gate workflows locally with isolated work directories."
    )
    parser.add_argument("--workflow", required=True, choices=["pr", "nightly", "rc"])
    parser.add_argument(
        "--matrix-lane",
        default="all",
        choices=["all", "gcc12", "gcc13", "gcc14", "gcc15"],
        help="Nightly matrix lane selector. Ignored for non-nightly workflows.",
    )
    parser.add_argument(
        "--report-dir",
        default=None,
        help="Top-level report directory. Defaults to target/local-gates/<workflow>.",
    )
    return parser.parse_args()


def command_exists(binary: str) -> bool:
    return shutil.which(binary) is not None


def installed_rust_targets(repo_root: Path) -> set[str]:
    completed = subprocess.run(
        ["rustup", "target", "list", "--installed"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or "failed to inspect installed rust targets")
    return {line.strip() for line in completed.stdout.splitlines() if line.strip()}


def docker_daemon_ready(repo_root: Path) -> tuple[bool, str]:
    completed = subprocess.run(
        ["docker", "info", "--format", "{{.ServerVersion}}"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if completed.returncode == 0:
        return True, completed.stdout.strip()
    return False, completed.stderr.strip() or completed.stdout.strip() or "docker info failed"


def preflight_errors(repo_root: Path, workflow: str) -> list[str]:
    errors: list[str] = []
    for binary in ["python3", "cargo"]:
        if not command_exists(binary):
            errors.append(f"required command `{binary}` was not found in PATH")

    if workflow in {"pr-gate", "nightly-gate"}:
        if not command_exists("docker"):
            errors.append("required command `docker` was not found in PATH")
        else:
            docker_ok, docker_message = docker_daemon_ready(repo_root)
            if not docker_ok:
                errors.append(f"Docker daemon is not reachable: {docker_message}")

        if not command_exists("rustup"):
            errors.append("required command `rustup` was not found in PATH")
        else:
            try:
                targets = installed_rust_targets(repo_root)
            except RuntimeError as error:
                errors.append(str(error))
            else:
                if "x86_64-unknown-linux-musl" not in targets:
                    errors.append(
                        "required Rust target `x86_64-unknown-linux-musl` is not installed"
                    )
    return errors


def should_execute_step(
    execution_condition: str,
    *,
    prior_failure: bool,
    executed_steps: set[str],
    required_step_id: str | None,
) -> bool:
    if execution_condition == "always":
        return True
    if execution_condition == "after_step_not_skipped":
        return required_step_id in executed_steps
    return not prior_failure


def run_gate_summary(
    repo_root: Path,
    workflow: str,
    report_root: Path,
    env: dict[str, str],
    *,
    matrix_gcc_version: str | None = None,
    matrix_version_band: str | None = None,
    release_blocker: str = "true",
) -> int:
    command = [
        "python3",
        str(repo_root / "ci" / "gate_summary.py"),
        "--plan",
        str(plan_path_for_workflow(repo_root, workflow)),
        "--report-root",
        str(report_root),
        "--release-blocker",
        release_blocker,
    ]
    if matrix_gcc_version is not None:
        command.extend(["--matrix-gcc-version", matrix_gcc_version])
    if matrix_version_band is not None:
        command.extend(["--matrix-version-band", matrix_version_band])
    completed = subprocess.run(command, cwd=repo_root, check=False, env=env)
    return completed.returncode


def write_matrix_summary(report_dir: Path, lane_summaries: list[dict]) -> None:
    overall_status = "success"
    failed_lanes: list[str] = []
    failure_classification_counts: dict[str, int] = {}
    for lane_summary in lane_summaries:
        if lane_summary["overall_status"] != "success":
            overall_status = "failure"
            failed_lanes.append(lane_summary["lane"])
        for key, value in lane_summary["failure_classification_counts"].items():
            failure_classification_counts[key] = failure_classification_counts.get(key, 0) + value

    payload = {
        "schema_version": 1,
        "workflow": "nightly-gate",
        "overall_status": overall_status,
        "failed_lanes": failed_lanes,
        "failure_classification_counts": failure_classification_counts,
        "lanes": lane_summaries,
    }
    summary_json = report_dir / "matrix-summary.json"
    summary_md = report_dir / "matrix-summary.md"
    summary_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Local Nightly Matrix Summary",
        "",
        f"- overall_status: `{overall_status}`",
        f"- failed_lanes: `{', '.join(failed_lanes) if failed_lanes else 'none'}`",
        "",
        "| lane | gcc_image | version_band | release_blocker | overall_status | failure_classification |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for lane_summary in lane_summaries:
        lines.append(
            "| {lane} | {gcc_image} | {version_band} | {release_blocker} | {overall_status} | {failure_classification} |".format(
                lane=lane_summary["lane"],
                gcc_image=lane_summary["gcc_image"],
                version_band=lane_summary["version_band"],
                release_blocker=lane_summary["release_blocker"],
                overall_status=lane_summary["overall_status"],
                failure_classification=lane_summary["overall_failure_classification"] or "none",
            )
        )
    summary_md.write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_single_workflow(
    repo_root: Path,
    workflow: str,
    report_root: Path,
    *,
    matrix_gcc_version: str | None = None,
    matrix_version_band: str | None = None,
    release_blocker: str = "true",
) -> tuple[int, dict]:
    plan = json.loads(plan_path_for_workflow(repo_root, workflow).read_text(encoding="utf-8"))
    plan_steps = sorted(plan["steps"], key=lambda step: int(step["order"]))

    env = os.environ.copy()
    env.update(
        build_execution_env(
            repo_root,
            report_root,
            workflow,
            local_mode=True,
            matrix_gcc_version=matrix_gcc_version,
            matrix_version_band=matrix_version_band,
            release_blocker=release_blocker,
        )
    )

    report_root.mkdir(parents=True, exist_ok=True)
    executed_steps: set[str] = set()
    prior_failure = False
    any_failure = False

    for step in plan_steps:
        if policy_skips_step(
            step,
            matrix_version_band=matrix_version_band,
            release_blocker=release_blocker,
        ):
            continue

        execution = EXECUTION_CATALOG[workflow][step["id"]]
        if not should_execute_step(
            execution.run_condition,
            prior_failure=prior_failure,
            executed_steps=executed_steps,
            required_step_id=execution.requires_step_id,
        ):
            continue

        command = [
            "python3",
            str(repo_root / "ci" / "run_gate_step.py"),
            "--workflow",
            workflow,
            "--step-id",
            step["id"],
            "--report-root",
            str(report_root),
            "--release-blocker",
            release_blocker,
            "--local",
        ]
        if matrix_gcc_version is not None:
            command.extend(["--matrix-gcc-version", matrix_gcc_version])
        if matrix_version_band is not None:
            command.extend(["--matrix-version-band", matrix_version_band])

        completed = subprocess.run(command, cwd=repo_root, check=False, env=env)
        executed_steps.add(step["id"])
        if completed.returncode != 0:
            any_failure = True
            prior_failure = True

    summary_returncode = run_gate_summary(
        repo_root,
        workflow,
        report_root,
        env,
        matrix_gcc_version=matrix_gcc_version,
        matrix_version_band=matrix_version_band,
        release_blocker=release_blocker,
    )
    summary_path = report_root / "gate" / "gate-summary.json"
    payload = json.loads(summary_path.read_text(encoding="utf-8"))
    return (1 if any_failure or summary_returncode != 0 else 0), payload


def main() -> int:
    args = parse_args()
    repo_root = SCRIPT_DIR.parent
    workflow = canonical_workflow_name(args.workflow)
    if workflow != "nightly-gate" and args.matrix_lane != "all":
        raise SystemExit("--matrix-lane is only supported for `nightly`")

    default_report_dir = repo_root / "target" / "local-gates" / args.workflow
    report_dir = Path(args.report_dir).resolve() if args.report_dir else default_report_dir

    errors = preflight_errors(repo_root, workflow)
    if errors:
        report_dir.mkdir(parents=True, exist_ok=True)
        message = "\n".join(f"- {error}" for error in errors)
        sys.stderr.write(f"local gate preflight failed for `{workflow}`:\n{message}\n")
        sys.stderr.flush()
        return 1

    if workflow != "nightly-gate":
        exit_code, _summary = run_single_workflow(repo_root, workflow, report_dir)
        return exit_code

    lane_summaries: list[dict] = []
    exit_code = 0
    for lane_name in nightly_lane_names(args.matrix_lane):
        lane = NIGHTLY_LANES[lane_name]
        lane_report_root = report_dir / "lanes" / lane_name
        lane_exit_code, summary = run_single_workflow(
            repo_root,
            workflow,
            lane_report_root,
            matrix_gcc_version=lane["gcc_image"],
            matrix_version_band=lane["version_band"],
            release_blocker=lane["release_blocker"],
        )
        lane_summaries.append(
            {
                "lane": lane_name,
                "gcc_image": lane["gcc_image"],
                "version_band": lane["version_band"],
                "release_blocker": lane["release_blocker"],
                "overall_status": summary["overall_status"],
                "overall_failure_classification": summary["overall_failure_classification"],
                "failure_classification_counts": summary["failure_classification_counts"],
                "report_root": str(lane_report_root),
                "summary_path": str(lane_report_root / "gate" / "gate-summary.json"),
            }
        )
        if lane_exit_code != 0:
            exit_code = 1

    write_matrix_summary(report_dir, lane_summaries)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
