#!/usr/bin/env python3

import argparse
import json
import os
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from string import Template

SCHEMA_VERSION = 3

LEGACY_SUPPORT_TIER_MAP = {
    "repository_gate": ("repository", None),
    "release_candidate_gate": ("release_candidate", None),
    "gcc15_primary": ("reference_path", "gcc15_plus"),
    "gcc13_compatibility": ("matrix", "gcc13_14"),
    "gcc14_compatibility": ("matrix", "gcc13_14"),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a CI gate step and emit machine-readable status artifacts."
    )
    parser.add_argument("--plan", required=True, help="Path to the workflow step plan JSON.")
    parser.add_argument("--step-id", required=True, help="Step identifier from the plan.")
    parser.add_argument(
        "--report-root", required=True, help="Workflow report root that owns gate artifacts."
    )
    parser.add_argument(
        "--command", required=True, help="Shell command to execute for this step."
    )
    parser.add_argument(
        "--shell",
        default="/bin/bash",
        help="Shell used to execute the command via -lc.",
    )
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
        "--matrix-support-tier",
        dest="legacy_matrix_support_tier",
        default=None,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--release-blocker",
        default=None,
        choices=["true", "false"],
        help="Optional nightly release blocker marker recorded in matrix metadata.",
    )
    return parser.parse_args()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def load_plan(plan_path: Path) -> dict:
    with plan_path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def find_step(plan: dict, step_id: str) -> dict:
    for step in plan.get("steps", []):
        if step.get("id") == step_id:
            return step
    available = ", ".join(step.get("id", "<unknown>") for step in plan.get("steps", []))
    raise KeyError(f"unknown step_id `{step_id}` (available: {available})")


def substitute(value, mapping):
    if value is None:
        return None
    if isinstance(value, str):
        return Template(value).safe_substitute(mapping)
    if isinstance(value, list):
        return [substitute(item, mapping) for item in value]
    if isinstance(value, dict):
        return {key: substitute(item, mapping) for key, item in value.items()}
    return value


def legacy_gate_metadata(legacy_support_tier: str | None) -> tuple[str | None, str | None]:
    if legacy_support_tier is None:
        return None, None
    return LEGACY_SUPPORT_TIER_MAP.get(legacy_support_tier, (legacy_support_tier, None))


def resolve_matrix_version_band(args: argparse.Namespace) -> str | None:
    if args.matrix_version_band is not None:
        return args.matrix_version_band
    _, version_band = legacy_gate_metadata(args.legacy_matrix_support_tier)
    return version_band


def resolve_step_metadata(step: dict, mapping: dict) -> tuple[str | None, str | None]:
    gate_scope = substitute(step.get("gate_scope"), mapping)
    version_band = substitute(step.get("version_band"), mapping)
    legacy_support_tier = substitute(step.get("support_tier"), mapping)
    legacy_gate_scope, legacy_version_band = legacy_gate_metadata(legacy_support_tier)
    if gate_scope is None:
        gate_scope = legacy_gate_scope
    if version_band is None:
        version_band = legacy_version_band
    return gate_scope, version_band


def ensure_gate_dirs(report_root: Path) -> tuple[Path, Path, Path]:
    gate_root = report_root / "gate"
    logs_dir = gate_root / "logs"
    status_dir = gate_root / "status"
    logs_dir.mkdir(parents=True, exist_ok=True)
    status_dir.mkdir(parents=True, exist_ok=True)
    return gate_root, logs_dir, status_dir


def status_file_name(order: int, step_id: str) -> str:
    return f"{order:02d}-{step_id}.json"


def log_file_name(order: int, step_id: str, stream: str) -> str:
    return f"{order:02d}-{step_id}.{stream}.log"


def stream_pipe(pipe, console, log_handle) -> None:
    try:
        for chunk in iter(pipe.readline, ""):
            if not chunk:
                break
            console.write(chunk)
            console.flush()
            log_handle.write(chunk)
            log_handle.flush()
    finally:
        pipe.close()


def build_mapping(args: argparse.Namespace) -> dict:
    return {
        "REPORT_ROOT": args.report_root,
        "WORK_ROOT": os.environ.get("WORK_ROOT", ""),
        "TARGET_DIR": os.environ.get("TARGET_DIR", ""),
        "DIST_DIR": os.environ.get("DIST_DIR", ""),
        "VENDOR_DIR": os.environ.get("VENDOR_DIR", ""),
        "CONTROL_DIR": os.environ.get("CONTROL_DIR", ""),
        "RELEASE_REPO_DIR": os.environ.get("RELEASE_REPO_DIR", ""),
        "SIGNING_KEY_PATH": os.environ.get("SIGNING_KEY_PATH", ""),
        "PACKAGE_VERSION": os.environ.get("PACKAGE_VERSION", ""),
        "MATRIX_GCC_VERSION": args.matrix_gcc_version or "",
        "MATRIX_SUPPORT_TIER": args.legacy_matrix_support_tier or "",
        "MATRIX_VERSION_BAND": resolve_matrix_version_band(args) or "",
        "RELEASE_BLOCKER": args.release_blocker or "",
    }


def build_child_env(args: argparse.Namespace) -> dict:
    env = os.environ.copy()
    env["REPORT_ROOT"] = args.report_root
    if args.matrix_gcc_version is not None:
        env["MATRIX_GCC_VERSION"] = args.matrix_gcc_version
    if args.legacy_matrix_support_tier is not None:
        env["MATRIX_SUPPORT_TIER"] = args.legacy_matrix_support_tier
    matrix_version_band = resolve_matrix_version_band(args)
    if matrix_version_band is not None:
        env["MATRIX_VERSION_BAND"] = matrix_version_band
    if args.release_blocker is not None:
        env["RELEASE_BLOCKER"] = args.release_blocker
    return env


def build_status_payload(
    args: argparse.Namespace,
    plan: dict,
    step: dict,
    mapping: dict,
    status: str,
    exit_code: int,
    started_at: str,
    finished_at: str,
    duration_ms: int,
    stdout_path: Path,
    stderr_path: Path,
) -> dict:
    gate_scope, version_band = resolve_step_metadata(step, mapping)
    return {
        "schema_version": SCHEMA_VERSION,
        "workflow": plan["workflow"],
        "job": plan["job"],
        "step": {
            "id": step["id"],
            "name": step["name"],
            "order": step["order"],
            "policy": step.get("policy", "always"),
            "failure_classification": step.get("failure_classification", "product"),
        },
        "status": status,
        "command": args.command,
        "exit_code": exit_code,
        "fixture": substitute(step.get("fixture"), mapping),
        "gcc_version": substitute(step.get("gcc_version"), mapping),
        "gate_scope": gate_scope,
        "version_band": version_band,
        "artifact_paths": substitute(step.get("artifact_paths", []), mapping),
        "log_paths": {
            "stdout": str(stdout_path),
            "stderr": str(stderr_path),
        },
        "started_at": started_at,
        "finished_at": finished_at,
        "duration_ms": duration_ms,
        "matrix": {
            "gcc_version": args.matrix_gcc_version,
            "version_band": resolve_matrix_version_band(args),
            "release_blocker": args.release_blocker,
        },
    }


def write_status(status_path: Path, payload: dict) -> None:
    status_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    plan_path = Path(args.plan)
    report_root = Path(args.report_root)
    plan = load_plan(plan_path)
    step = find_step(plan, args.step_id)
    mapping = build_mapping(args)
    _, logs_dir, status_dir = ensure_gate_dirs(report_root)

    step_order = int(step["order"])
    stdout_path = logs_dir / log_file_name(step_order, step["id"], "stdout")
    stderr_path = logs_dir / log_file_name(step_order, step["id"], "stderr")
    status_path = status_dir / status_file_name(step_order, step["id"])

    started_at = utc_now()
    started_monotonic = time.monotonic()
    exit_code = -1
    status = "failure"

    child_env = build_child_env(args)

    with stdout_path.open("w", encoding="utf-8") as stdout_handle, stderr_path.open(
        "w", encoding="utf-8"
    ) as stderr_handle:
        try:
            process = subprocess.Popen(
                [args.shell, "-lc", args.command],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                encoding="utf-8",
                errors="replace",
                env=child_env,
            )
        except OSError as error:
            message = f"failed to spawn `{args.shell} -lc`: {error}\n"
            sys.stderr.write(message)
            sys.stderr.flush()
            stderr_handle.write(message)
            stderr_handle.flush()
            exit_code = -1
        else:
            stdout_thread = threading.Thread(
                target=stream_pipe, args=(process.stdout, sys.stdout, stdout_handle)
            )
            stderr_thread = threading.Thread(
                target=stream_pipe, args=(process.stderr, sys.stderr, stderr_handle)
            )
            stdout_thread.start()
            stderr_thread.start()
            exit_code = process.wait()
            stdout_thread.join()
            stderr_thread.join()

        status = "success" if exit_code == 0 else "failure"

    finished_at = utc_now()
    duration_ms = int((time.monotonic() - started_monotonic) * 1000)
    payload = build_status_payload(
        args,
        plan,
        step,
        mapping,
        status,
        exit_code,
        started_at,
        finished_at,
        duration_ms,
        stdout_path,
        stderr_path,
    )
    write_status(status_path, payload)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
