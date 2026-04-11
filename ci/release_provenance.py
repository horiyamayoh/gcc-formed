#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path

SCHEMA_VERSION = 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Write machine-readable release provenance for CI and release workflows."
    )
    parser.add_argument(
        "--workflow",
        required=True,
        choices=["pr-gate", "nightly-gate", "public-beta-release", "stable-release"],
        help="Workflow identifier recorded in the provenance bundle.",
    )
    parser.add_argument(
        "--report-root",
        required=True,
        help="Workflow report root that contains release evidence directories.",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path to the release-provenance.json output file.",
    )
    parser.add_argument("--package-version", default=None)
    parser.add_argument("--target-triple", default=None)
    parser.add_argument("--release-channel", default=None)
    parser.add_argument("--maturity-label", default=None)
    parser.add_argument("--rollback-baseline-version", default=None)
    parser.add_argument("--signing-key-id", default=None)
    parser.add_argument("--signing-public-key-sha256", default=None)
    parser.add_argument("--matrix-gcc-image", default=None)
    parser.add_argument("--matrix-version-band", default=None)
    parser.add_argument(
        "--release-blocker",
        default=None,
        choices=["true", "false"],
    )
    return parser.parse_args()


def compact_dict(payload: dict) -> dict:
    return {key: value for key, value in payload.items() if value is not None}


def load_json(path: Path):
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def release_root(report_root: Path) -> Path:
    return report_root / "release"


def stable_root(report_root: Path) -> Path:
    return report_root / "stable-release"


def build_github_metadata() -> dict:
    return compact_dict(
        {
            "repository": os.environ.get("GITHUB_REPOSITORY"),
            "sha": os.environ.get("GITHUB_SHA"),
            "ref": os.environ.get("GITHUB_REF"),
            "run_id": os.environ.get("GITHUB_RUN_ID"),
            "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "actor": os.environ.get("GITHUB_ACTOR"),
            "tag": os.environ.get("RELEASE_TAG"),
        }
    )


def build_release_scope(args: argparse.Namespace) -> dict | None:
    scope = compact_dict(
        {
            "package_version": args.package_version,
            "target_triple": args.target_triple,
            "release_channel": args.release_channel,
            "maturity_label": args.maturity_label,
            "rollback_baseline_version": args.rollback_baseline_version,
            "signing_key_id": args.signing_key_id,
            "signing_public_key_sha256": args.signing_public_key_sha256,
        }
    )
    return scope or None


def build_matrix_metadata(args: argparse.Namespace) -> dict | None:
    matrix = compact_dict(
        {
            "gcc_image": args.matrix_gcc_image,
            "version_band": args.matrix_version_band,
            "release_blocker": args.release_blocker,
        }
    )
    return matrix or None


def build_release_evidence(args: argparse.Namespace, report_root: Path) -> dict:
    release = release_root(report_root)
    if args.workflow == "stable-release":
        stable = stable_root(report_root)
        return {
            "package": load_json(release / "package.json"),
            "stable_release_command": load_json(release / "stable-release-command.json"),
            "stable_release_report": load_json(stable / "stable-release-report.json"),
            "promotion_evidence": load_json(stable / "promotion-evidence.json"),
            "rollback_drill": load_json(stable / "rollback-drill.json"),
            "hermetic_release": load_json(release / "hermetic-release.json"),
            "vendor": load_json(release / "vendor.json"),
            "rc_gate": load_json(report_root / "rc-gate" / "rc-gate-report.json"),
            "metrics_report": load_json(report_root / "rc-gate" / "metrics-report.json"),
            "fuzz_report": load_json(report_root / "rc-gate" / "fuzz-smoke-report.json"),
            "human_eval": load_json(report_root / "rc-gate" / "human-eval" / "human-eval-report.json"),
        }

    evidence = {
        "package": load_json(release / "package.json"),
        "release_publish": load_json(release / "release-publish.json"),
        "release_promote_canary": load_json(release / "release-promote-canary.json"),
        "release_promote_beta": load_json(release / "release-promote-beta.json"),
        "release_resolve_beta": load_json(release / "release-resolve-beta.json"),
        "install": load_json(release / "install.json"),
        "install_release": load_json(release / "install-release.json"),
        "system_install": load_json(release / "system-install.json"),
        "hermetic_release": load_json(release / "hermetic-release.json"),
        "vendor": load_json(release / "vendor.json"),
    }
    if args.workflow == "nightly-gate":
        evidence["bench_smoke"] = load_json(release / "bench-smoke.json")
        evidence["fuzz_smoke"] = load_json(release / "fuzz-smoke-report.json")
    return evidence


def build_payload(args: argparse.Namespace) -> dict:
    report_root = Path(args.report_root)
    payload = {
        "schema_version": SCHEMA_VERSION,
        "workflow": args.workflow,
        "github": build_github_metadata(),
        "release": build_release_evidence(args, report_root),
    }
    release_scope = build_release_scope(args)
    if release_scope is not None:
        payload["release_scope"] = release_scope
    matrix = build_matrix_metadata(args)
    if matrix is not None:
        payload["matrix"] = matrix
    return payload


def main() -> int:
    args = parse_args()
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(build_payload(args), indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
