#!/usr/bin/env python3
"""Continue the Codex turn if it tries to claim DONE without fresh validation evidence."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

THIS_FILE = Path(__file__).resolve()
CODEX_DIR = THIS_FILE.parents[1]
if str(CODEX_DIR) not in sys.path:
    sys.path.insert(0, str(CODEX_DIR))

from quality_gate_common import find_repo_root, load_json, snapshot_changed_files, snapshot_hash

DONE_RE = re.compile(r"(^|\n)\s*STATUS:\s*DONE\b", re.IGNORECASE)


def load_hook_input() -> dict:
    try:
        raw = sys.stdin.read().strip()
        return json.loads(raw) if raw else {}
    except Exception:
        return {}


def assess(repo_root: Path) -> tuple[bool, str]:
    quality_gate = load_json(repo_root / ".codex" / "evidence" / "quality-gate.json", default=None)
    if not quality_gate:
        return False, "Run `python3 .codex/run_quality_gate.py` before claiming completion."
    if not quality_gate.get("passed"):
        return False, "The quality gate is failing; fix the failures and rerun the gate."
    current_snapshot = snapshot_changed_files(repo_root)
    current_hash = snapshot_hash(current_snapshot)
    if current_hash != quality_gate.get("diff_snapshot_hash"):
        return False, (
            "The quality gate is stale because files changed after validation. "
            "Regenerate `.codex/evidence/self-review.md` if needed and rerun the gate."
        )

    required = [
        repo_root / ".codex" / "evidence" / "work-plan.md",
        repo_root / ".codex" / "evidence" / "self-review.md",
    ]
    missing = [path.relative_to(repo_root).as_posix() for path in required if not path.exists() or not path.read_text(encoding="utf-8").strip()]
    if missing:
        return False, "Missing required evidence file(s): " + ", ".join(missing)

    return True, ""


def main() -> int:
    payload = load_hook_input()
    last_message = str(payload.get("last_assistant_message") or "")
    stop_hook_active = bool(payload.get("stop_hook_active"))
    if stop_hook_active:
        return 0

    if not DONE_RE.search(last_message):
        return 0

    repo_root = find_repo_root(payload.get("cwd"))
    ok, reason = assess(repo_root)
    if ok:
        return 0

    print(json.dumps({"decision": "block", "reason": reason}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
