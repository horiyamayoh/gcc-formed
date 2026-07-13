#!/usr/bin/env python3
"""Verify that a stable cut is promoting the immutable signed RC payload."""

import argparse
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rc-provenance", required=True, type=Path)
    parser.add_argument("--control-dir", required=True, type=Path)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--expected-payload-version", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    provenance = json.loads(args.rc_provenance.read_text(encoding="utf-8"))
    manifest_path = args.control_dir / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    failures: list[str] = []

    if provenance.get("workflow") != "public-beta-release":
        failures.append("source provenance was not produced by the prerelease workflow")
    rc_evidence = provenance.get("release", {})
    for required in (
        "package",
        "install",
        "install_release",
        "replay_stop_ship",
        "agent_output_quality",
        "agent_output_quality_integrity",
        "no_subagent_attestation",
        "model_agent_tool_manifest",
    ):
        if not rc_evidence.get(required):
            failures.append(f"RC provenance missing required field evidence: release.{required}")

    qualification_candidate_sha = rc_evidence.get("agent_output_quality", {}).get(
        "candidate_sha"
    )
    if not qualification_candidate_sha:
        failures.append("RC provenance missing qualification candidate SHA")

    provenance_commit = provenance.get("source_identity", {}).get(
        "payload_source_sha"
    ) or provenance.get("github", {}).get("sha")
    if provenance_commit != args.expected_commit:
        failures.append(
            f"RC provenance commit {provenance_commit!r} != checkout {args.expected_commit!r}"
        )
    provenance_version = provenance.get("release_scope", {}).get("package_version")
    if provenance_version != args.expected_payload_version:
        failures.append(
            f"RC provenance payload version {provenance_version!r} != expected "
            f"{args.expected_payload_version!r}"
        )
    manifest_version = manifest.get("product_version")
    if manifest_version != args.expected_payload_version:
        failures.append(
            f"manifest payload version {manifest_version!r} != expected "
            f"{args.expected_payload_version!r}"
        )

    verified_files: dict[str, str] = {}
    shasums_path = args.control_dir / "SHA256SUMS"
    for line in shasums_path.read_text(encoding="utf-8").splitlines():
        expected, relative_name = line.split(maxsplit=1)
        relative_name = relative_name.lstrip("* ")
        artifact = args.control_dir / relative_name
        actual = sha256(artifact)
        verified_files[relative_name] = actual
        if actual != expected:
            failures.append(f"SHA256 mismatch for {relative_name}: {actual} != {expected}")

    report = {
        "schema_version": 1,
        "status": "pass" if not failures else "fail",
        "stable_operation": "metadata-only-promotion-of-signed-rc-payload",
        "rc_provenance_path": str(args.rc_provenance),
        "rc_provenance_sha256": sha256(args.rc_provenance),
        "candidate_commit": args.expected_commit,
        "qualification_candidate_sha": qualification_candidate_sha,
        "payload_source_sha": args.expected_commit,
        "payload_version": args.expected_payload_version,
        "required_rc_field_evidence": [
            "package",
            "install",
            "install_release",
            "replay_stop_ship",
            "agent_output_quality",
            "agent_output_quality_integrity",
            "no_subagent_attestation",
            "model_agent_tool_manifest",
        ],
        "manifest_sha256": sha256(manifest_path),
        "shasums_sha256": sha256(shasums_path),
        "signature_sha256": sha256(args.control_dir / "SHA256SUMS.sig"),
        "verified_files": verified_files,
        "failures": failures,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if failures:
        raise SystemExit("; ".join(failures))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
