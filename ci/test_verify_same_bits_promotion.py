#!/usr/bin/env python3

import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "ci" / "verify_same_bits_promotion.py"


class SameBitsPromotionTest(unittest.TestCase):
    def test_verifies_commit_version_and_all_signed_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            control = root / "control"
            control.mkdir()
            (control / "payload.tar.gz").write_bytes(b"immutable rc payload")
            (control / "manifest.json").write_text(
                json.dumps({"product_version": "1.0.0-rc.1"}), encoding="utf-8"
            )
            digest = hashlib.sha256((control / "payload.tar.gz").read_bytes()).hexdigest()
            (control / "SHA256SUMS").write_text(
                f"{digest}  payload.tar.gz\n", encoding="utf-8"
            )
            (control / "SHA256SUMS.sig").write_text("{}\n", encoding="utf-8")
            provenance = root / "release-provenance.json"
            provenance.write_text(
                json.dumps(
                    {
                        "workflow": "public-beta-release",
                        "github": {"sha": "a" * 40},
                        "release_scope": {"package_version": "1.0.0-rc.1"},
                        "release": {
                            key: {"status": "pass"}
                            for key in (
                                "package",
                                "install",
                                "install_release",
                                "replay_stop_ship",
                                "agent_output_quality",
                                "agent_output_quality_integrity",
                                "no_subagent_attestation",
                                "model_agent_tool_manifest",
                            )
                        },
                    }
                ),
                encoding="utf-8",
            )
            output = root / "report.json"
            completed = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--rc-provenance",
                    str(provenance),
                    "--control-dir",
                    str(control),
                    "--expected-commit",
                    "a" * 40,
                    "--expected-payload-version",
                    "1.0.0-rc.1",
                    "--output",
                    str(output),
                ],
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(json.loads(output.read_text())["status"], "pass")

    def test_rejects_different_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            control = root / "control"
            control.mkdir()
            (control / "manifest.json").write_text(
                json.dumps({"product_version": "1.0.0-rc.1"}), encoding="utf-8"
            )
            (control / "SHA256SUMS").write_text("", encoding="utf-8")
            (control / "SHA256SUMS.sig").write_text("{}\n", encoding="utf-8")
            provenance = root / "release-provenance.json"
            provenance.write_text(
                json.dumps(
                    {
                        "workflow": "public-beta-release",
                        "github": {"sha": "b" * 40},
                        "release_scope": {"package_version": "1.0.0-rc.1"},
                        "release": {
                            key: {"status": "pass"}
                            for key in (
                                "package",
                                "install",
                                "install_release",
                                "replay_stop_ship",
                                "agent_output_quality",
                                "agent_output_quality_integrity",
                                "no_subagent_attestation",
                                "model_agent_tool_manifest",
                            )
                        },
                    }
                ),
                encoding="utf-8",
            )
            completed = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--rc-provenance",
                    str(provenance),
                    "--control-dir",
                    str(control),
                    "--expected-commit",
                    "a" * 40,
                    "--expected-payload-version",
                    "1.0.0-rc.1",
                    "--output",
                    str(root / "report.json"),
                ],
                text=True,
                capture_output=True,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertEqual(json.loads((root / "report.json").read_text())["status"], "fail")


if __name__ == "__main__":
    unittest.main()
