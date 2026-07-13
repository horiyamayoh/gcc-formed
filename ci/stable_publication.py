#!/usr/bin/env python3
"""Build and verify an append-only stable GitHub Release publication plan."""

import argparse
import hashlib
import json
from pathlib import Path


SCHEMA_VERSION = 1
POLICY_VERSION = "append-only-stable-publication-v1"


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def provenance_inputs(provenance: dict) -> dict:
    scope = provenance.get("release_scope", {})
    integrity = provenance.get("release_integrity", {})
    return {
        "stable_release_version": scope.get("stable_release_version"),
        "payload_version": scope.get("package_version"),
        "promoted_from_tag": scope.get("promoted_from_tag"),
        "rollback_baseline_version": scope.get("rollback_baseline_version"),
        "signing_key_id": scope.get("signing_key_id"),
        "signing_public_key_sha256": scope.get("signing_public_key_sha256"),
        "qualification_candidate_sha": integrity.get("qualification_candidate_sha"),
        "payload_source_sha": integrity.get("payload_source_sha"),
        "gate_source_sha": integrity.get("gate_source_sha"),
        "workflow_definition_sha": integrity.get("workflow_definition_sha"),
        "relation_policy_version": integrity.get("relation_policy_version"),
        "relation_verdict": integrity.get("relation_verdict"),
        "diff_manifest_sha256": integrity.get("diff_manifest_sha256"),
    }


def validate_publication_inputs(inputs: dict) -> None:
    required = [
        "stable_release_version",
        "payload_version",
        "promoted_from_tag",
        "rollback_baseline_version",
        "signing_key_id",
        "signing_public_key_sha256",
        "qualification_candidate_sha",
        "payload_source_sha",
        "gate_source_sha",
        "workflow_definition_sha",
        "relation_policy_version",
        "relation_verdict",
    ]
    missing = [field for field in required if not inputs.get(field)]
    if missing:
        raise ValueError(f"release provenance missing publication inputs: {', '.join(missing)}")
    if inputs["relation_verdict"] != "pass":
        raise ValueError("release commit-chain relation_verdict must be pass")


def build_expected(args: argparse.Namespace) -> dict:
    provenance = read_json(args.release_provenance)
    inputs = provenance_inputs(provenance)
    validate_publication_inputs(inputs)
    if args.release_tag != f"v{inputs['stable_release_version']}":
        raise ValueError("release tag does not match stable release identity")
    if args.tag_target_sha != inputs["payload_source_sha"]:
        raise ValueError("tag target does not match provenance payload_source_sha")

    assets = []
    names = set()
    provenance_resolved = args.release_provenance.resolve()
    for asset in args.asset:
        resolved = asset.resolve()
        if not asset.is_file():
            raise ValueError(f"expected asset does not exist: {asset}")
        if asset.name in names:
            raise ValueError(f"duplicate expected asset name: {asset.name}")
        names.add(asset.name)
        assets.append(
            {
                "name": asset.name,
                "size": asset.stat().st_size,
                "sha256": sha256_file(asset),
                "source_path": str(asset),
            }
        )
    if provenance_resolved not in {asset.resolve() for asset in args.asset}:
        raise ValueError("release-provenance.json must be in the expected asset set")

    notes = args.notes_file.read_bytes()
    return {
        "schema_version": SCHEMA_VERSION,
        "policy_version": POLICY_VERSION,
        "release_tag": args.release_tag,
        "release_name": args.release_name,
        "tag_target_sha": args.tag_target_sha,
        "body_sha256": sha256_bytes(notes),
        "is_draft": False,
        "is_prerelease": False,
        "publication_inputs": inputs,
        "release_provenance_sha256": sha256_file(args.release_provenance),
        "assets": sorted(assets, key=lambda item: item["name"]),
    }


def normalized_inventory(existing: dict, existing_assets_dir: Path | None) -> dict:
    assets = []
    for asset in existing.get("assets", []):
        digest = asset.get("digest")
        if isinstance(digest, str) and digest.startswith("sha256:"):
            digest = digest.removeprefix("sha256:")
        else:
            downloaded = existing_assets_dir / asset["name"] if existing_assets_dir else None
            digest = sha256_file(downloaded) if downloaded and downloaded.is_file() else None
        assets.append(
            {
                "name": asset.get("name"),
                "size": asset.get("size"),
                "sha256": digest,
                "state": asset.get("state"),
            }
        )
    return {
        "release_tag": existing.get("tagName"),
        "release_name": existing.get("name"),
        "resolved_tag_target_sha": existing.get("resolvedTagTargetSha"),
        "body_sha256": sha256_bytes(existing.get("body", "").encode("utf-8")),
        "is_draft": existing.get("isDraft"),
        "is_prerelease": existing.get("isPrerelease"),
        "assets": sorted(assets, key=lambda item: item["name"] or ""),
    }


def inventory_sha256(inventory: dict) -> str:
    canonical = json.dumps(
        inventory, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return sha256_bytes(canonical)


def decide(args: argparse.Namespace) -> tuple[dict, int]:
    expected = read_json(args.expected_manifest)
    expected_hash = sha256_file(args.expected_manifest)
    if args.existing_inventory is None:
        mismatches = []
        if (
            args.existing_tag_target_sha
            and args.existing_tag_target_sha != expected["tag_target_sha"]
        ):
            mismatches.append(
                "tag_target_sha: existing="
                f"{args.existing_tag_target_sha!r} expected={expected['tag_target_sha']!r}"
            )
        return (
            {
                "schema_version": SCHEMA_VERSION,
                "policy_version": POLICY_VERSION,
                "decision": "reject" if mismatches else "create",
                "expected_manifest_sha256": expected_hash,
                "existing_inventory_sha256": None,
                "missing_assets": [asset["name"] for asset in expected["assets"]],
                "missing_asset_paths": [asset["source_path"] for asset in expected["assets"]],
                "mismatches": mismatches,
            },
            1 if mismatches else 0,
        )

    existing = read_json(args.existing_inventory)
    inventory = normalized_inventory(existing, args.existing_assets_dir)
    mismatches = []
    metadata_pairs = [
        ("release_tag", inventory["release_tag"], expected["release_tag"]),
        ("release_name", inventory["release_name"], expected["release_name"]),
        ("tag_target_sha", inventory["resolved_tag_target_sha"], expected["tag_target_sha"]),
        ("body_sha256", inventory["body_sha256"], expected["body_sha256"]),
        ("is_draft", inventory["is_draft"], expected["is_draft"]),
        ("is_prerelease", inventory["is_prerelease"], expected["is_prerelease"]),
    ]
    for field, actual, wanted in metadata_pairs:
        if actual != wanted:
            mismatches.append(f"{field}: existing={actual!r} expected={wanted!r}")

    if args.existing_provenance is None or not args.existing_provenance.is_file():
        mismatches.append(
            "existing release lacks release-provenance.json; publication inputs cannot be verified"
        )
    else:
        provenance_inventory = next(
            (
                asset
                for asset in inventory["assets"]
                if asset["name"] == "release-provenance.json"
            ),
            None,
        )
        downloaded_provenance_sha = sha256_file(args.existing_provenance)
        if (
            provenance_inventory is None
            or provenance_inventory["sha256"] != downloaded_provenance_sha
        ):
            mismatches.append(
                "downloaded release-provenance.json does not match the existing asset inventory"
            )
        existing_provenance = read_json(args.existing_provenance)
        actual_inputs = provenance_inputs(existing_provenance)
        if actual_inputs != expected["publication_inputs"]:
            for field in sorted(expected["publication_inputs"]):
                actual = actual_inputs.get(field)
                wanted = expected["publication_inputs"][field]
                if actual != wanted:
                    mismatches.append(
                        f"publication_inputs.{field}: existing={actual!r} expected={wanted!r}"
                    )

    expected_assets = {asset["name"]: asset for asset in expected["assets"]}
    existing_assets = {asset["name"]: asset for asset in inventory["assets"]}
    extra_assets = sorted(set(existing_assets) - set(expected_assets))
    if extra_assets:
        mismatches.append(f"unexpected existing assets: {', '.join(extra_assets)}")
    missing_assets = sorted(set(expected_assets) - set(existing_assets))
    for name in sorted(set(expected_assets) & set(existing_assets)):
        actual = existing_assets[name]
        wanted = expected_assets[name]
        if actual["state"] != "uploaded":
            mismatches.append(f"asset {name} state={actual['state']!r}, expected 'uploaded'")
        if actual["size"] != wanted["size"]:
            mismatches.append(
                f"asset {name} size={actual['size']!r}, expected {wanted['size']!r}"
            )
        if actual["sha256"] != wanted["sha256"]:
            mismatches.append(
                f"asset {name} sha256={actual['sha256']!r}, expected {wanted['sha256']!r}"
            )

    if mismatches:
        decision = "reject"
        status = 1
    elif missing_assets:
        decision = "upload_missing"
        status = 0
    else:
        decision = "noop"
        status = 0
    return (
        {
            "schema_version": SCHEMA_VERSION,
            "policy_version": POLICY_VERSION,
            "decision": decision,
            "expected_manifest_sha256": expected_hash,
            "existing_inventory_sha256": inventory_sha256(inventory),
            "existing_inventory": inventory,
            "missing_assets": missing_assets,
            "missing_asset_paths": [expected_assets[name]["source_path"] for name in missing_assets],
            "mismatches": mismatches,
        },
        status,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    build = subparsers.add_parser("build")
    build.add_argument("--release-tag", required=True)
    build.add_argument("--release-name", required=True)
    build.add_argument("--tag-target-sha", required=True)
    build.add_argument("--notes-file", required=True, type=Path)
    build.add_argument("--release-provenance", required=True, type=Path)
    build.add_argument("--asset", action="append", required=True, type=Path)
    build.add_argument("--output", required=True, type=Path)
    verify = subparsers.add_parser("decide")
    verify.add_argument("--expected-manifest", required=True, type=Path)
    verify.add_argument("--existing-inventory", type=Path)
    verify.add_argument("--existing-provenance", type=Path)
    verify.add_argument("--existing-assets-dir", type=Path)
    verify.add_argument("--existing-tag-target-sha")
    verify.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "build":
        write_json(args.output, build_expected(args))
        return 0
    report, status = decide(args)
    write_json(args.output, report)
    if status:
        raise SystemExit("; ".join(report["mismatches"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
