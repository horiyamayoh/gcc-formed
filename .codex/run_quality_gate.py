#!/usr/bin/env python3
"""Repo-local completion gate for Codex tasks.

This script is intentionally stricter than a typical "did the tests pass?" check.
It also verifies:
- required evidence files exist
- synchronization rules across contract-sensitive docs/surfaces
- the diff snapshot recorded at validation time
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

THIS_FILE = Path(__file__).resolve()
CODEX_DIR = THIS_FILE.parent
if str(CODEX_DIR) not in sys.path:
    sys.path.insert(0, str(CODEX_DIR))

from quality_gate_common import (
    changed_paths,
    command_exists,
    find_repo_root,
    load_json,
    matches_any,
    require_headings,
    snapshot_changed_files,
    snapshot_hash,
    utc_now_iso,
    write_json,
)

STATUS_PASSED = "passed"
STATUS_FAILED = "failed"
STATUS_BLOCKED = "blocked"
STATUS_SKIPPED = "skipped"


def docs_only(paths: list[str], rules: dict[str, Any]) -> bool:
    """Return True if the change set is docs-only."""
    if not paths:
        return False
    docs_patterns = rules.get("docs_only_globs", [])
    never_docs_patterns = rules.get("never_docs_only_globs", [])
    if any(matches_any(path, never_docs_patterns) for path in paths):
        return False
    return all(matches_any(path, docs_patterns) for path in paths)


def classify_paths(paths: list[str]) -> dict[str, bool]:
    """Classify changed paths into coarse, repo-relevant buckets."""
    workflow_touched = any(matches_any(path, [".github/workflows/**"]) for path in paths)
    release_touched = any(
        matches_any(
            path,
            [
                "xtask/**",
                "docs/releases/**",
                ".github/workflows/release*.yml",
                "SUPPORT.md",
                "docs/runbooks/**",
            ],
        )
        for path in paths
    )
    dependency_touched = any(
        matches_any(
            path,
            ["Cargo.toml", "Cargo.lock", "deny.toml", "vendor/**", "rust-toolchain.toml"],
        )
        for path in paths
    )
    public_export_touched = any(
        matches_any(
            path,
            ["diag_public_export/**", "docs/specs/public-machine-readable-diagnostic-surface-spec.md"],
        )
        for path in paths
    )
    support_boundary_touched = any(
        matches_any(path, ["docs/support/SUPPORT-BOUNDARY.md", "README.md", "docs/support/PUBLIC-SURFACE.md"])
        for path in paths
    )
    return {
        "workflow_touched": workflow_touched,
        "release_touched": release_touched,
        "dependency_touched": dependency_touched,
        "public_export_touched": public_export_touched,
        "support_boundary_touched": support_boundary_touched,
    }


def validate_required_evidence(repo_root: Path, rules: dict[str, Any]) -> list[dict[str, Any]]:
    """Validate required evidence files and headings."""
    results: list[dict[str, Any]] = []
    for item in rules.get("evidence", {}).get("required_files", []):
        rel = item["path"]
        path = repo_root / rel
        exists = path.exists()
        text = path.read_text(encoding="utf-8") if exists else ""
        missing_headings = require_headings(text, item.get("headings", [])) if exists else item.get("headings", [])
        results.append(
            {
                "path": rel,
                "exists": exists,
                "non_empty": bool(text.strip()) if exists else False,
                "missing_headings": missing_headings,
                "passed": exists and bool(text.strip()) and not missing_headings,
            }
        )
    return results


def command_result(
    *,
    name: str,
    command: str,
    repo_root: Path,
    logs_dir: Path,
    env: dict[str, str] | None = None,
) -> dict[str, Any]:
    """Run one shell command, capturing stdout/stderr to a log file."""
    slug = "".join(ch.lower() if ch.isalnum() else "-" for ch in name).strip("-")
    log_path = logs_dir / f"{slug}.log"
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)

    started = time.perf_counter()
    try:
        result = subprocess.run(
            command,
            cwd=str(repo_root),
            env=merged_env,
            shell=True,
            check=False,
            capture_output=True,
            text=True,
        )
        elapsed = time.perf_counter() - started
        log_path.write_text(
            f"$ {command}\n\n[stdout]\n{result.stdout}\n\n[stderr]\n{result.stderr}\n",
            encoding="utf-8",
        )
        status = STATUS_PASSED if result.returncode == 0 else STATUS_FAILED
        return {
            "name": name,
            "command": command,
            "status": status,
            "exit_code": result.returncode,
            "duration_seconds": round(elapsed, 3),
            "log_path": log_path.relative_to(repo_root).as_posix(),
        }
    except FileNotFoundError as exc:
        elapsed = time.perf_counter() - started
        log_path.write_text(str(exc), encoding="utf-8")
        return {
            "name": name,
            "command": command,
            "status": STATUS_BLOCKED,
            "exit_code": None,
            "duration_seconds": round(elapsed, 3),
            "log_path": log_path.relative_to(repo_root).as_posix(),
            "error": str(exc),
        }


def actionlint_needed(classification: dict[str, bool]) -> bool:
    """Return True when workflow lint should run."""
    return bool(classification["workflow_touched"])


def build_command_plan(
    *,
    repo_root: Path,
    rules: dict[str, Any],
    paths: list[str],
    docs_change: bool,
) -> tuple[list[dict[str, str]], list[str]]:
    """Return the command plan and any blocking prerequisites."""
    commands: list[dict[str, str]] = []
    blocking_reasons: list[str] = []
    evidence_dir = repo_root / ".codex" / "evidence"
    parity_script = repo_root / "ci" / "run_pr_gate_local.sh"
    sync_json = evidence_dir / "sync-check.json"

    commands.append(
        {
            "name": "git diff check",
            "command": "git diff --check",
        }
    )
    commands.append(
        {
            "name": "sync requirements",
            "command": f"python3 .codex/check_sync_requirements.py --json-out {sync_json.as_posix()}",
        }
    )

    classification = classify_paths(paths)

    if docs_change:
        commands.append(
            {
                "name": "python contract tests",
                "command": "python3 -B -m unittest discover -s ci -p 'test_*.py'",
            }
        )
    elif parity_script.exists():
        commands.append(
            {
                "name": "ci parity gate",
                "command": "bash ci/run_pr_gate_local.sh",
            }
        )
    else:
        # Fallback to the current repo-local gate defined in CONTRIBUTING.md.
        tmp_vendor = evidence_dir / "tmp-vendor"
        replay_dir = evidence_dir / "replay"
        snapshot_dir = evidence_dir / "snapshot"
        commands.extend(
            [
                {
                    "name": "cargo xtask check",
                    "command": "cargo xtask check",
                },
                {
                    "name": "representative replay",
                    "command": f"cargo xtask replay --root corpus --subset representative --report-dir {replay_dir.as_posix()}",
                },
                {
                    "name": "representative snapshot check",
                    "command": f"cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15 --version-band gcc15_plus --report-dir {snapshot_dir.as_posix()}",
                },
                {
                    "name": "vendor dependencies",
                    "command": f"rm -rf {tmp_vendor.as_posix()} && cargo xtask vendor --output-dir {tmp_vendor.as_posix()}",
                },
                {
                    "name": "cargo deny check",
                    "command": "cargo deny check",
                },
                {
                    "name": "hermetic release smoke",
                    "command": f"cargo xtask hermetic-release-check --vendor-dir {tmp_vendor.as_posix()} --bin gcc-formed --target-triple x86_64-unknown-linux-musl",
                },
            ]
        )

    if actionlint_needed(classification):
        workflow_lints = rules.get("workflow_lint_commands", [])
        if not workflow_lints:
            blocking_reasons.append("workflow files changed but no workflow lint command is configured")
        else:
            workflow_command = workflow_lints[0]
            if not command_exists(workflow_command):
                blocking_reasons.append(
                    "workflow files changed but `actionlint` is not installed; install it or add it to the Codex image"
                )
            else:
                commands.append(
                    {
                        "name": "workflow lint",
                        "command": workflow_command,
                    }
                )

    return commands, blocking_reasons


def summarize_status(command_results: list[dict[str, Any]], evidence_results: list[dict[str, Any]], blocking: list[str]) -> tuple[bool, str]:
    """Determine overall pass/fail/block status."""
    if blocking:
        return False, STATUS_BLOCKED

    if not all(item["passed"] for item in evidence_results):
        return False, STATUS_FAILED

    for item in command_results:
        if item["status"] == STATUS_BLOCKED:
            return False, STATUS_BLOCKED
        if item["status"] != STATUS_PASSED:
            return False, STATUS_FAILED

    return True, STATUS_PASSED


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json-out", type=Path, default=None)
    args = parser.parse_args()

    repo_root = find_repo_root()
    os.chdir(repo_root)

    evidence_dir = repo_root / ".codex" / "evidence"
    logs_dir = evidence_dir / "logs"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    logs_dir.mkdir(parents=True, exist_ok=True)

    rules_path = repo_root / ".codex" / "gate-rules.json"
    rules = load_json(rules_path, default={})
    paths = changed_paths(repo_root)
    snapshot = snapshot_changed_files(repo_root)

    evidence_results = validate_required_evidence(repo_root, rules)
    blocking_reasons: list[str] = []

    if not paths:
        blocking_reasons.append("no changed files detected; nothing was validated")

    docs_change = docs_only(paths, rules) if paths else False
    plan, plan_blocking = build_command_plan(
        repo_root=repo_root,
        rules=rules,
        paths=paths,
        docs_change=docs_change,
    )
    blocking_reasons.extend(plan_blocking)

    command_results: list[dict[str, Any]] = []
    if not blocking_reasons:
        for item in plan:
            result = command_result(
                name=item["name"],
                command=item["command"],
                repo_root=repo_root,
                logs_dir=logs_dir,
            )
            command_results.append(result)
            if result["status"] in {STATUS_FAILED, STATUS_BLOCKED}:
                # Preserve fail-fast semantics so the agent must fix the first real failure.
                break

    passed, status = summarize_status(command_results, evidence_results, blocking_reasons)

    payload = {
        "generated_at_utc": utc_now_iso(),
        "repo_root": str(repo_root),
        "passed": passed,
        "status": status,
        "changed_files": paths,
        "docs_only": docs_change,
        "classification": classify_paths(paths),
        "required_evidence": evidence_results,
        "commands": command_results,
        "blocking_reasons": blocking_reasons,
        "diff_snapshot": snapshot,
        "diff_snapshot_hash": snapshot_hash(snapshot),
    }

    json_out = args.json_out or (evidence_dir / "quality-gate.json")
    write_json(json_out, payload)

    if passed:
        print(f"[quality-gate] PASS -> {json_out.relative_to(repo_root).as_posix()}")
        return 0

    print(f"[quality-gate] {status.upper()} -> {json_out.relative_to(repo_root).as_posix()}", file=sys.stderr)
    for reason in blocking_reasons:
        print(f"  - {reason}", file=sys.stderr)
    for item in evidence_results:
        if not item["passed"]:
            print(f"  - evidence failed: {item['path']}", file=sys.stderr)
            if item["missing_headings"]:
                print(f"    missing headings: {', '.join(item['missing_headings'])}", file=sys.stderr)
    for item in command_results:
        if item["status"] != STATUS_PASSED:
            print(
                f"  - command {item['name']} -> {item['status']} (see {item['log_path']})",
                file=sys.stderr,
            )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
