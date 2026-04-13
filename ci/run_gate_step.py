#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from gate_catalog import (  # noqa: E402
    EXECUTION_CATALOG,
    build_execution_env,
    canonical_workflow_name,
    plan_path_for_workflow,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resolve a shared CI/local gate step command and execute it via gate_step.py."
    )
    parser.add_argument("--workflow", required=True, help="Workflow alias or canonical name.")
    parser.add_argument("--step-id", required=True, help="Step identifier from the checked-in plan.")
    parser.add_argument("--report-root", required=True, help="Report root for gate artifacts.")
    parser.add_argument(
        "--matrix-gcc-version",
        default=None,
        help="Optional nightly matrix GCC selector such as gcc:13.",
    )
    parser.add_argument(
        "--matrix-version-band",
        default=None,
        help="Optional nightly matrix version band such as gcc13_14.",
    )
    parser.add_argument(
        "--release-blocker",
        default="true",
        choices=["true", "false"],
        help="Whether reference-path-only nightly steps apply.",
    )
    parser.add_argument(
        "--local",
        action="store_true",
        help="Use local-isolated work directories instead of GitHub workflow defaults.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = SCRIPT_DIR.parent
    workflow = canonical_workflow_name(args.workflow)
    try:
        step = EXECUTION_CATALOG[workflow][args.step_id]
    except KeyError as error:
        raise SystemExit(f"unknown step `{args.step_id}` for workflow `{workflow}`") from error

    report_root = Path(args.report_root).resolve()
    env = os.environ.copy()
    env.update(
        build_execution_env(
            repo_root,
            report_root,
            workflow,
            local_mode=args.local,
            matrix_gcc_version=args.matrix_gcc_version,
            matrix_version_band=args.matrix_version_band,
            release_blocker=args.release_blocker,
        )
    )

    command = [
        "python3",
        str(repo_root / "ci" / "gate_step.py"),
        "--plan",
        str(plan_path_for_workflow(repo_root, workflow)),
        "--step-id",
        args.step_id,
        "--report-root",
        str(report_root),
        "--command",
        step.command,
    ]
    if args.matrix_gcc_version is not None:
        command.extend(["--matrix-gcc-version", args.matrix_gcc_version])
    if args.matrix_version_band is not None:
        command.extend(["--matrix-version-band", args.matrix_version_band])
    if args.release_blocker is not None:
        command.extend(["--release-blocker", args.release_blocker])

    completed = subprocess.run(command, cwd=repo_root, check=False, env=env)
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
