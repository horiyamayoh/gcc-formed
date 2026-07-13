#!/usr/bin/env python3
"""Bind qualification, payload, product gates, and orchestration to typed commits."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


SCHEMA_VERSION = 1
POLICY_VERSION = "release-commit-chain-v1"


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def git(repository: Path, *args: str, text: bool = True):
    return subprocess.run(
        ["git", "-C", str(repository), *args],
        check=True,
        capture_output=True,
        text=text,
    ).stdout


def resolve_commit(repository: Path, revision: str) -> str:
    return git(repository, "rev-parse", "--verify", f"{revision}^{{commit}}").strip()


def blob_sha256(repository: Path, commit: str, path: str) -> str | None:
    try:
        payload = git(repository, "show", f"{commit}:{path}", text=False)
    except subprocess.CalledProcessError:
        return None
    return sha256_bytes(payload)


def tree_mode(repository: Path, commit: str, path: str) -> str | None:
    output = git(repository, "ls-tree", commit, "--", path).strip()
    if not output:
        return None
    return output.split(maxsplit=1)[0]


def changed_files(repository: Path, before: str, after: str) -> list[dict]:
    raw = git(
        repository,
        "diff",
        "--name-status",
        "--no-renames",
        "-z",
        before,
        after,
        text=False,
    )
    fields = raw.split(b"\0")
    if fields and not fields[-1]:
        fields.pop()
    if len(fields) % 2:
        raise ValueError("unexpected git diff --name-status output")
    changes = []
    for index in range(0, len(fields), 2):
        status = fields[index].decode("utf-8")
        path = fields[index + 1].decode("utf-8")
        changes.append(
            {
                "status": status,
                "path": path,
                "old_sha256": blob_sha256(repository, before, path),
                "new_sha256": blob_sha256(repository, after, path),
            }
        )
    return changes


def renamed_or_copied_paths(repository: Path, before: str, after: str) -> list[str]:
    output = git(repository, "diff", "--name-status", "--find-renames", before, after)
    return [
        line
        for line in output.splitlines()
        if line.startswith("R") or line.startswith("C")
    ]


def is_allowed_documentation_path(path: str) -> bool:
    candidate = Path(path)
    if candidate.is_absolute() or ".." in candidate.parts or candidate.suffix != ".md":
        return False
    if len(candidate.parts) == 1:
        return path in {"README.md", "CHANGELOG.md"}
    return candidate.parts[0] in {"docs", "adr-initial-set"}


def write_report(path: Path, report: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


def legacy_report(path: Path) -> dict:
    provenance = json.loads(path.read_text(encoding="utf-8"))
    release = provenance.get("release", {})
    return {
        "schema_version": SCHEMA_VERSION,
        "relation_policy_version": POLICY_VERSION,
        "relation_verdict": "unverified",
        "qualification_candidate_sha": release.get("agent_output_quality", {}).get(
            "candidate_sha"
        ),
        "payload_source_sha": release.get("rc_payload_verification", {}).get(
            "candidate_commit"
        ),
        "gate_source_sha": None,
        "workflow_definition_sha": provenance.get("github", {}).get("sha"),
        "diff_manifest_sha256": None,
        "failures": [
            "legacy evidence predates the typed commit-chain contract; no relation is inferred"
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository-root", type=Path, default=Path("."))
    parser.add_argument("--qualification-report", type=Path)
    parser.add_argument("--payload-source-sha")
    parser.add_argument("--gate-source-sha")
    parser.add_argument("--workflow-definition-sha")
    parser.add_argument("--allowed-diff-manifest", type=Path)
    parser.add_argument("--legacy-provenance", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.legacy_provenance:
        if any(
            [
                args.qualification_report,
                args.payload_source_sha,
                args.gate_source_sha,
                args.workflow_definition_sha,
                args.allowed_diff_manifest,
            ]
        ):
            raise SystemExit("--legacy-provenance cannot be combined with current-chain inputs")
        write_report(args.output, legacy_report(args.legacy_provenance))
        return 0

    required = {
        "--qualification-report": args.qualification_report,
        "--payload-source-sha": args.payload_source_sha,
        "--gate-source-sha": args.gate_source_sha,
        "--workflow-definition-sha": args.workflow_definition_sha,
    }
    missing = [name for name, value in required.items() if not value]
    if missing:
        raise SystemExit(f"missing required current-chain inputs: {', '.join(missing)}")

    failures: list[str] = []
    qualification = json.loads(args.qualification_report.read_text(encoding="utf-8"))
    qualification_sha = resolve_commit(args.repository_root, qualification["candidate_sha"])
    payload_sha = resolve_commit(args.repository_root, args.payload_source_sha)
    gate_sha = resolve_commit(args.repository_root, args.gate_source_sha)
    workflow_sha = resolve_commit(args.repository_root, args.workflow_definition_sha)
    changes = changed_files(args.repository_root, qualification_sha, payload_sha)
    manifest_sha = None
    manifest = None

    if gate_sha != payload_sha:
        failures.append(
            "gate_source_sha must equal payload_source_sha so product gates run on promoted source"
        )

    if qualification_sha != payload_sha:
        if args.allowed_diff_manifest is None:
            failures.append(
                "qualification_candidate_sha differs from payload_source_sha without an allowed-diff manifest"
            )
        else:
            manifest_bytes = args.allowed_diff_manifest.read_bytes()
            manifest_sha = sha256_bytes(manifest_bytes)
            manifest = json.loads(manifest_bytes)
            expected_header = {
                "schema_version": SCHEMA_VERSION,
                "policy_version": POLICY_VERSION,
                "qualification_candidate_sha": qualification_sha,
                "payload_source_sha": payload_sha,
                "commit_range": f"{qualification_sha}..{payload_sha}",
            }
            for key, expected in expected_header.items():
                if manifest.get(key) != expected:
                    failures.append(
                        f"allowed-diff manifest {key}={manifest.get(key)!r}, expected {expected!r}"
                    )
            if manifest.get("allowed_changes") != changes:
                failures.append(
                    "allowed-diff manifest file/hash evidence does not exactly match the commit range"
                )
            for rename in renamed_or_copied_paths(
                args.repository_root, qualification_sha, payload_sha
            ):
                failures.append(f"rename/copy changes are not allowed: {rename}")
            for change in changes:
                if change["status"] not in {"A", "M", "D"}:
                    failures.append(
                        f"unsupported change status {change['status']!r}: {change['path']}"
                    )
                if not is_allowed_documentation_path(change["path"]):
                    failures.append(
                        f"qualification-affecting path is not allowed: {change['path']}"
                    )
                for commit in (qualification_sha, payload_sha):
                    mode = tree_mode(args.repository_root, commit, change["path"])
                    if mode in {"120000", "160000"}:
                        failures.append(
                            f"symlink/submodule changes are not allowed: {change['path']}"
                        )

    report = {
        "schema_version": SCHEMA_VERSION,
        "relation_policy_version": POLICY_VERSION,
        "relation_verdict": "pass" if not failures else "fail",
        "qualification_candidate_sha": qualification_sha,
        "payload_source_sha": payload_sha,
        "gate_source_sha": gate_sha,
        "workflow_definition_sha": workflow_sha,
        "commit_range": f"{qualification_sha}..{payload_sha}",
        "diff_manifest_sha256": manifest_sha,
        "allowed_diff_manifest": manifest,
        "changed_files": changes,
        "failures": failures,
    }
    write_report(args.output, report)
    if failures:
        raise SystemExit("; ".join(failures))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
