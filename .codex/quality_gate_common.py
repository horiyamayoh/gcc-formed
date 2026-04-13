#!/usr/bin/env python3
"""Shared helpers for repo-local Codex quality gates."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from fnmatch import fnmatch
from pathlib import Path
from typing import Any, Iterable

DEFAULT_IGNORE_PATTERNS = [
    ".codex/evidence/**",
    ".codex/__pycache__/**",
    "**/__pycache__/**",
    "**/*.pyc",
]


def matches_any(path: str, patterns: Iterable[str]) -> bool:
    """Return True if a path matches any glob-like pattern."""
    normalized = path.replace("\\", "/")
    for pattern in patterns:
        candidate = pattern.replace("\\", "/")
        if fnmatch(normalized, candidate):
            return True
        if candidate.endswith("/**"):
            prefix = candidate[:-3]
            if normalized.startswith(prefix):
                return True
        if candidate.endswith("/*"):
            prefix = candidate[:-1]
            if normalized.startswith(prefix):
                return True
    return False


def find_repo_root(start: str | Path | None = None) -> Path:
    """Return the git repo root, or the current directory if git is unavailable."""
    cwd = Path(start or os.getcwd()).resolve()
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=str(cwd),
            check=True,
            capture_output=True,
            text=True,
        )
        return Path(result.stdout.strip()).resolve()
    except Exception:
        return cwd


def git(repo_root: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    """Run a git command inside the repository root."""
    return subprocess.run(
        ["git", *args],
        cwd=str(repo_root),
        check=check,
        capture_output=True,
        text=True,
    )


def relpath(repo_root: Path, path: Path) -> str:
    """Return a POSIX-style path relative to the repository root."""
    return path.resolve().relative_to(repo_root.resolve()).as_posix()


def list_changed_entries(repo_root: Path) -> list[dict[str, str]]:
    """Return changed git entries from porcelain output, including untracked files."""
    result = git(repo_root, "status", "--porcelain=v1", "--untracked-files=all", check=False)
    entries: list[dict[str, str]] = []
    for raw_line in result.stdout.splitlines():
        line = raw_line.rstrip("\n")
        if not line:
            continue
        if line.startswith("?? "):
            path = line[3:]
            entries.append({"code": "??", "path": path})
            continue

        code = line[:2]
        body = line[3:]
        if " -> " in body:
            old_path, new_path = body.split(" -> ", 1)
            entries.append({"code": code, "path": new_path, "old_path": old_path})
        else:
            entries.append({"code": code, "path": body})
    return entries


def relevant_entries(
    repo_root: Path,
    ignore_patterns: Iterable[str] | None = None,
) -> list[dict[str, str]]:
    """Filter changed entries to user-authored repo changes only."""
    ignored = list(ignore_patterns or DEFAULT_IGNORE_PATTERNS)
    output: list[dict[str, str]] = []
    for entry in list_changed_entries(repo_root):
        path = entry["path"]
        if matches_any(path, ignored):
            continue
        output.append(entry)
    return output


def changed_paths(
    repo_root: Path,
    ignore_patterns: Iterable[str] | None = None,
) -> list[str]:
    """Return unique changed paths, ignoring gate-generated artifacts by default."""
    seen: set[str] = set()
    paths: list[str] = []
    for entry in relevant_entries(repo_root, ignore_patterns=ignore_patterns):
        path = entry["path"]
        if path not in seen:
            seen.add(path)
            paths.append(path)
    return sorted(paths)


def file_sha256(path: Path) -> str:
    """Compute a file's SHA-256 digest."""
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def snapshot_changed_files(
    repo_root: Path,
    ignore_patterns: Iterable[str] | None = None,
) -> dict[str, dict[str, str]]:
    """Create a stable snapshot of current changed files and their content hashes."""
    snapshot: dict[str, dict[str, str]] = {}
    for entry in relevant_entries(repo_root, ignore_patterns=ignore_patterns):
        rel = entry["path"]
        abs_path = repo_root / rel
        code = entry["code"]
        if not abs_path.exists():
            snapshot[rel] = {"status": code, "kind": "deleted"}
            continue
        if abs_path.is_dir():
            snapshot[rel] = {"status": code, "kind": "directory"}
            continue
        snapshot[rel] = {
            "status": code,
            "kind": "file",
            "sha256": file_sha256(abs_path),
            "size": str(abs_path.stat().st_size),
        }
    return dict(sorted(snapshot.items(), key=lambda item: item[0]))


def snapshot_hash(snapshot: dict[str, Any]) -> str:
    """Hash a JSON snapshot deterministically."""
    payload = json.dumps(snapshot, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def load_json(path: Path, default: Any | None = None) -> Any:
    """Load JSON from a file or return default when it does not exist."""
    if not path.exists():
        return default
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: Any) -> None:
    """Write JSON with stable formatting."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2, sort_keys=True)
        handle.write("\n")


def command_exists(name: str) -> bool:
    """Return True if a command is available on PATH."""
    for p in os.environ.get("PATH", "").split(os.pathsep):
        try:
            if (Path(p) / name).exists():
                return True
        except (PermissionError, OSError):
            continue
    return False


def read_text(path: Path) -> str:
    """Read text safely."""
    return path.read_text(encoding="utf-8") if path.exists() else ""


def normalize_text(value: str) -> str:
    """Normalize text for loose heading checks."""
    return "\n".join(line.rstrip() for line in value.replace("\r\n", "\n").splitlines()).strip()


def require_headings(text: str, headings: Iterable[str]) -> list[str]:
    """Return missing headings for a text blob."""
    norm = normalize_text(text).lower()
    missing: list[str] = []
    for heading in headings:
        if heading.lower() not in norm:
            missing.append(heading)
    return missing


def utc_now_iso() -> str:
    """Return a UTC ISO-8601 timestamp."""
    from datetime import datetime, timezone

    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def dump_json_stdout(payload: Any) -> None:
    """Print a JSON payload to stdout."""
    json.dump(payload, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")
