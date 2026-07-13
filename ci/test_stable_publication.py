import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "ci" / "stable_publication.py"


class StablePublicationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.notes = self.root / "notes.md"
        self.notes.write_text("stable notes\n", encoding="utf-8")
        self.provenance = self.root / "release-provenance.json"
        self.provenance.write_text(
            json.dumps(
                {
                    "release_scope": {
                        "stable_release_version": "1.0.1",
                        "package_version": "1.0.1-rc.1",
                        "promoted_from_tag": "v1.0.1-rc.1",
                        "rollback_baseline_version": "1.0.0",
                        "signing_key_id": "key-1",
                        "signing_public_key_sha256": "a" * 64,
                    },
                    "release_integrity": {
                        "qualification_candidate_sha": "b" * 40,
                        "payload_source_sha": "b" * 40,
                        "gate_source_sha": "b" * 40,
                        "workflow_definition_sha": "c" * 40,
                        "relation_policy_version": "release-commit-chain-v1",
                        "relation_verdict": "pass",
                        "diff_manifest_sha256": None,
                    },
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        self.payload = self.root / "payload.tar.gz"
        self.payload.write_bytes(b"payload")
        self.expected = self.root / "expected.json"
        completed = self.run_script(
            "build",
            "--release-tag",
            "v1.0.1",
            "--release-name",
            "gcc-formed 1.0.1",
            "--tag-target-sha",
            "b" * 40,
            "--notes-file",
            str(self.notes),
            "--release-provenance",
            str(self.provenance),
            "--asset",
            str(self.payload),
            "--asset",
            str(self.provenance),
            "--output",
            str(self.expected),
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_script(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["python3", str(SCRIPT), *args],
            text=True,
            capture_output=True,
            check=False,
        )

    def inventory(self, include_payload: bool = True) -> Path:
        expected = json.loads(self.expected.read_text(encoding="utf-8"))
        assets = expected["assets"]
        if not include_payload:
            assets = [asset for asset in assets if asset["name"] != "payload.tar.gz"]
        inventory = {
            "tagName": expected["release_tag"],
            "name": expected["release_name"],
            "resolvedTagTargetSha": expected["tag_target_sha"],
            "body": self.notes.read_text(encoding="utf-8"),
            "isDraft": False,
            "isPrerelease": False,
            "assets": [
                {
                    "name": asset["name"],
                    "size": asset["size"],
                    "digest": f"sha256:{asset['sha256']}",
                    "state": "uploaded",
                }
                for asset in assets
            ],
        }
        path = self.root / "inventory.json"
        path.write_text(json.dumps(inventory), encoding="utf-8")
        return path

    def decide(
        self,
        inventory: Path | None,
        provenance: Path | None = None,
        existing_assets_dir: Path | None = None,
        existing_tag_target_sha: str | None = None,
    ):
        output = self.root / "decision.json"
        args = [
            "decide",
            "--expected-manifest",
            str(self.expected),
            "--output",
            str(output),
        ]
        if inventory:
            args.extend(["--existing-inventory", str(inventory)])
        if provenance:
            args.extend(["--existing-provenance", str(provenance)])
        if existing_assets_dir:
            args.extend(["--existing-assets-dir", str(existing_assets_dir)])
        if existing_tag_target_sha:
            args.extend(["--existing-tag-target-sha", existing_tag_target_sha])
        completed = self.run_script(*args)
        return completed, json.loads(output.read_text(encoding="utf-8"))

    def test_new_release_generates_create_plan(self) -> None:
        completed, report = self.decide(None)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["decision"], "create")
        self.assertCountEqual(report["missing_assets"], ["payload.tar.gz", "release-provenance.json"])

    def test_existing_tag_with_different_target_rejects_release_creation(self) -> None:
        completed, report = self.decide(None, existing_tag_target_sha="d" * 40)
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(report["decision"], "reject")
        self.assertTrue(any("tag_target_sha" in item for item in report["mismatches"]))

    def test_exact_rerun_is_noop_with_inventory_hash(self) -> None:
        completed, report = self.decide(self.inventory(), self.provenance)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["decision"], "noop")
        self.assertRegex(report["existing_inventory_sha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(report["mismatches"], [])

    def test_missing_non_authority_asset_is_the_only_recovery_write(self) -> None:
        completed, report = self.decide(self.inventory(include_payload=False), self.provenance)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["decision"], "upload_missing")
        self.assertEqual(report["missing_assets"], ["payload.tar.gz"])
        self.assertEqual(report["missing_asset_paths"], [str(self.payload)])

    def test_one_byte_asset_mismatch_rejects_before_write(self) -> None:
        inventory = json.loads(self.inventory().read_text(encoding="utf-8"))
        inventory["assets"][0]["digest"] = f"sha256:{hashlib.sha256(b'other').hexdigest()}"
        path = self.root / "mismatch.json"
        path.write_text(json.dumps(inventory), encoding="utf-8")
        completed, report = self.decide(path, self.provenance)
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(report["decision"], "reject")
        self.assertTrue(any("sha256=" in item for item in report["mismatches"]))

    def test_body_tag_and_extra_asset_mismatch_reject(self) -> None:
        inventory = json.loads(self.inventory().read_text(encoding="utf-8"))
        inventory["body"] = "silently edited\n"
        inventory["resolvedTagTargetSha"] = "d" * 40
        inventory["assets"].append(
            {"name": "unexpected.json", "size": 2, "digest": "sha256:" + "e" * 64, "state": "uploaded"}
        )
        path = self.root / "metadata-mismatch.json"
        path.write_text(json.dumps(inventory), encoding="utf-8")
        completed, report = self.decide(path, self.provenance)
        self.assertNotEqual(completed.returncode, 0)
        details = "\n".join(report["mismatches"])
        self.assertIn("body_sha256", details)
        self.assertIn("tag_target_sha", details)
        self.assertIn("unexpected existing assets", details)

    def test_rollback_signing_and_typed_chain_mismatch_reject(self) -> None:
        existing = json.loads(self.provenance.read_text(encoding="utf-8"))
        existing["release_scope"]["rollback_baseline_version"] = "0.2.0-beta.1"
        existing["release_scope"]["signing_key_id"] = "different-key"
        existing["release_integrity"]["workflow_definition_sha"] = "d" * 40
        path = self.root / "existing-provenance.json"
        path.write_text(json.dumps(existing), encoding="utf-8")
        completed, report = self.decide(self.inventory(), path)
        self.assertNotEqual(completed.returncode, 0)
        details = "\n".join(report["mismatches"])
        self.assertIn("rollback_baseline_version", details)
        self.assertIn("signing_key_id", details)
        self.assertIn("workflow_definition_sha", details)

    def test_missing_existing_provenance_fails_closed(self) -> None:
        completed, report = self.decide(self.inventory())
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(report["decision"], "reject")
        self.assertTrue(any("cannot be verified" in item for item in report["mismatches"]))

    def test_downloaded_asset_hash_is_used_when_api_digest_is_unavailable(self) -> None:
        inventory = json.loads(self.inventory().read_text(encoding="utf-8"))
        payload_asset = next(
            asset for asset in inventory["assets"] if asset["name"] == "payload.tar.gz"
        )
        payload_asset["digest"] = None
        path = self.root / "digest-unavailable.json"
        path.write_text(json.dumps(inventory), encoding="utf-8")
        completed, report = self.decide(path, self.provenance, self.root)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["decision"], "noop")


if __name__ == "__main__":
    unittest.main()
