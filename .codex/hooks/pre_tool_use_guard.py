#!/usr/bin/env python3
"""Block push/PR/publish commands until the quality gate passes and remains fresh."""

from __future__ import annotations

import json
import sys
from pathlib import Path

THIS_FILE = Path(__file__).resolve()
CODEX_DIR = THIS_FILE.parents[1]
if str(CODEX_DIR) not in sys.path:
    sys.path.insert(0, str(CODEX_DIR))

from quality_gate_common import find_repo_root, load_json, snapshot_changed_files, snapshot_hash

BLOCK_PATTERNS = [
    "git push",
    "gh pr create",
    "gh pr merge",
    "git tag",
    "gh release create",
    "cargo publish",
    "cargo xtask release-publish",
    "cargo xtask release-promote",
    "cargo xtask stable-release",
]


def _matches_any_subcommand(command: str, patterns: list[str]) -> bool:
    """Check whether any sub-command in a shell pipeline/chain starts with a blocked pattern.

    Splits on ``|``, ``&&``, ``||``, and ``;`` so that ``echo "git push"``
    does NOT match while ``git push --force`` still does.
    """
    import re

    sub_commands = re.split(r"\s*(?:\|\||&&|[|;])\s*", command)
    for sub in sub_commands:
        # Strip leading environment variable assignments (e.g. GIT_DIR=x git push)
        tokens = sub.strip().split()
        while tokens and "=" in tokens[0]:
            tokens.pop(0)
        normalized = " ".join(tokens).lower()
        for pattern in patterns:
            if normalized == pattern or normalized.startswith(pattern + " "):
                return True
    return False


def load_hook_input() -> dict:
    try:
        raw = sys.stdin.read().strip()
        return json.loads(raw) if raw else {}
    except Exception:
        return {}


def gate_ok(repo_root: Path) -> tuple[bool, str]:
    quality_gate = load_json(repo_root / ".codex" / "evidence" / "quality-gate.json", default=None)
    if not quality_gate:
        return False, "quality gate has not been run yet"
    if not quality_gate.get("passed"):
        return False, "quality gate is not passing"
    current_snapshot = snapshot_changed_files(repo_root)
    current_hash = snapshot_hash(current_snapshot)
    if current_hash != quality_gate.get("diff_snapshot_hash"):
        return False, "quality gate is stale because the workspace changed after validation"
    return True, ""


def main() -> int:
    payload = load_hook_input()
    command = str(payload.get("tool_input", {}).get("command", "")).strip()
    if not command:
        return 0

    if not _matches_any_subcommand(command, BLOCK_PATTERNS):
        return 0

    repo_root = find_repo_root(payload.get("cwd"))
    ok, reason = gate_ok(repo_root)
    if ok:
        return 0

    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": (
                        "Publish guard blocked this command because "
                        + reason
                        + ". Run `python3 .codex/run_quality_gate.py`, fix failures, "
                        "and do not modify the workspace afterward."
                    ),
                }
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
