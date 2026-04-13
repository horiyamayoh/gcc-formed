#!/usr/bin/env python3
"""Check repo-specific synchronization rules for changed files."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Allow imports when run from arbitrary working directories.
THIS_FILE = Path(__file__).resolve()
CODEX_DIR = THIS_FILE.parent
if str(CODEX_DIR) not in sys.path:
    sys.path.insert(0, str(CODEX_DIR))

from quality_gate_common import changed_paths, find_repo_root, load_json, matches_any, write_json


def evaluate_rule(rule: dict, paths: list[str]) -> dict:
    """Evaluate one synchronization rule."""
    when_any = rule.get("when_any_changed", [])
    if when_any and not any(matches_any(path, when_any) for path in paths):
        return {
            "name": rule.get("name", "unnamed"),
            "severity": rule.get("severity", "error"),
            "triggered": False,
            "passed": True,
            "message": rule.get("message", ""),
            "details": [],
        }

    details: list[str] = []
    passed = True

    require_all = rule.get("require_changed_all", [])
    for pattern in require_all:
        if not any(matches_any(path, [pattern]) for path in paths):
            passed = False
            details.append(f"missing required companion change matching: {pattern}")

    require_any = rule.get("require_changed_any", [])
    if require_any and not any(matches_any(path, require_any) for path in paths):
        passed = False
        details.append(
            "expected at least one companion change matching one of: "
            + ", ".join(require_any)
        )

    forbid_any = rule.get("forbid_changed_any", [])
    forbidden_hits = [path for path in paths if matches_any(path, forbid_any)]
    if forbidden_hits:
        passed = False
        details.append("forbidden companion changes present: " + ", ".join(sorted(forbidden_hits)))

    return {
        "name": rule.get("name", "unnamed"),
        "severity": rule.get("severity", "error"),
        "triggered": True,
        "passed": passed,
        "message": rule.get("message", ""),
        "details": details,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json-out", type=Path, default=None)
    args = parser.parse_args()

    repo_root = find_repo_root()
    rules_path = repo_root / ".codex" / "gate-rules.json"
    rules = load_json(rules_path, default={})
    paths = changed_paths(repo_root)
    results = [evaluate_rule(rule, paths) for rule in rules.get("sync_rules", [])]

    error_failures = [item for item in results if item["triggered"] and not item["passed"] and item["severity"] == "error"]
    warn_failures = [item for item in results if item["triggered"] and not item["passed"] and item["severity"] != "error"]

    payload = {
        "repo_root": str(repo_root),
        "changed_files": paths,
        "results": results,
        "error_failures": error_failures,
        "warn_failures": warn_failures,
        "passed": not error_failures,
    }

    if args.json_out:
        write_json(args.json_out, payload)

    if error_failures:
        for failure in error_failures:
            print(f"[sync:error] {failure['name']}: {failure['message']}", file=sys.stderr)
            for detail in failure["details"]:
                print(f"  - {detail}", file=sys.stderr)
        return 1

    for failure in warn_failures:
        print(f"[sync:warn] {failure['name']}: {failure['message']}", file=sys.stderr)
        for detail in failure["details"]:
            print(f"  - {detail}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
