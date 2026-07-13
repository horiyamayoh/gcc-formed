import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "ci" / "verify_release_commit_chain.py"


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["python3", str(SCRIPT), *args],
        text=True,
        capture_output=True,
        check=False,
    )


class ReleaseCommitChainTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "config", "user.name", "Test"], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.email", "test@example.com"],
            check=True,
        )
        (self.root / "src").mkdir()
        (self.root / "src/main.rs").write_text("fn main() {}\n", encoding="utf-8")
        (self.root / "README.md").write_text("initial\n", encoding="utf-8")
        self.commit("candidate")
        self.candidate = self.rev()
        self.qualification = self.root / "qualification.json"
        self.qualification.write_text(
            json.dumps({"candidate_sha": self.candidate}), encoding="utf-8"
        )
        (self.root / ".git/info/exclude").write_text(
            "qualification.json\nreport.json\nallowed.json\nlegacy.json\nlegacy-report.json\n",
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def commit(self, message: str) -> None:
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "-qm", message], check=True)

    def rev(self) -> str:
        return subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True
        ).strip()

    def invoke(self, payload: str, gate: str, *extra: str) -> tuple[subprocess.CompletedProcess, dict]:
        output = self.root / "report.json"
        completed = run(
            "--repository-root",
            str(self.root),
            "--qualification-report",
            str(self.qualification),
            "--payload-source-sha",
            payload,
            "--gate-source-sha",
            gate,
            "--workflow-definition-sha",
            payload,
            "--output",
            str(output),
            *extra,
        )
        return completed, json.loads(output.read_text(encoding="utf-8"))

    def test_exact_commit_chain_passes_with_typed_roles(self) -> None:
        completed, report = self.invoke(self.candidate, self.candidate)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["relation_verdict"], "pass")
        self.assertEqual(report["qualification_candidate_sha"], self.candidate)
        self.assertEqual(report["payload_source_sha"], self.candidate)
        self.assertEqual(report["gate_source_sha"], self.candidate)
        self.assertEqual(report["relation_policy_version"], "release-commit-chain-v1")

    def test_runtime_or_gate_source_mismatch_is_rejected(self) -> None:
        (self.root / "src/main.rs").write_text("fn main() { println!(\"changed\"); }\n")
        self.commit("runtime change")
        payload = self.rev()
        completed, report = self.invoke(payload, self.candidate)
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(report["relation_verdict"], "fail")
        self.assertTrue(any("gate_source_sha" in item for item in report["failures"]))
        self.assertTrue(any("without an allowed-diff" in item for item in report["failures"]))

    def test_exact_docs_manifest_records_commit_range_and_hash(self) -> None:
        (self.root / "README.md").write_text("documented\n", encoding="utf-8")
        self.commit("docs only")
        payload = self.rev()
        old_hash = hashlib.sha256(b"initial\n").hexdigest()
        new_hash = hashlib.sha256(b"documented\n").hexdigest()
        manifest = self.root / "allowed.json"
        manifest.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "policy_version": "release-commit-chain-v1",
                    "qualification_candidate_sha": self.candidate,
                    "payload_source_sha": payload,
                    "commit_range": f"{self.candidate}..{payload}",
                    "allowed_changes": [
                        {
                            "status": "M",
                            "path": "README.md",
                            "old_sha256": old_hash,
                            "new_sha256": new_hash,
                        }
                    ],
                },
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        completed, report = self.invoke(
            payload, payload, "--allowed-diff-manifest", str(manifest)
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(report["relation_verdict"], "pass")
        self.assertEqual(report["diff_manifest_sha256"], hashlib.sha256(manifest.read_bytes()).hexdigest())

    def test_runtime_change_cannot_be_whitelisted_by_exact_manifest(self) -> None:
        (self.root / "src/main.rs").write_text("fn changed() {}\n", encoding="utf-8")
        self.commit("runtime")
        payload = self.rev()
        manifest = self.root / "allowed.json"
        manifest.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "policy_version": "release-commit-chain-v1",
                    "qualification_candidate_sha": self.candidate,
                    "payload_source_sha": payload,
                    "commit_range": f"{self.candidate}..{payload}",
                    "allowed_changes": [
                        {
                            "status": "M",
                            "path": "src/main.rs",
                            "old_sha256": hashlib.sha256(b"fn main() {}\n").hexdigest(),
                            "new_sha256": hashlib.sha256(b"fn changed() {}\n").hexdigest(),
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        completed, report = self.invoke(
            payload, payload, "--allowed-diff-manifest", str(manifest)
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertTrue(any("qualification-affecting path" in item for item in report["failures"]))

    def test_fixture_oracle_gate_and_packaging_changes_are_rejected(self) -> None:
        changes = {
            "corpus/case/meta.yaml": b"fixture: changed\n",
            "eval/oracle.json": b"{}\n",
            "ci/gate.py": b"raise SystemExit(0)\n",
            "xtask/src/commands/package.rs": b"// changed\n",
            ".github/workflows/release.yml": b"name: changed\n",
        }
        for relative, payload_bytes in changes.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload_bytes)
        self.commit("qualification-affecting paths")
        payload = self.rev()
        allowed_changes = [
            {
                "status": "A",
                "path": path,
                "old_sha256": None,
                "new_sha256": hashlib.sha256(contents).hexdigest(),
            }
            for path, contents in sorted(changes.items())
        ]
        manifest = self.root / "allowed.json"
        manifest.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "policy_version": "release-commit-chain-v1",
                    "qualification_candidate_sha": self.candidate,
                    "payload_source_sha": payload,
                    "commit_range": f"{self.candidate}..{payload}",
                    "allowed_changes": allowed_changes,
                }
            ),
            encoding="utf-8",
        )
        completed, report = self.invoke(
            payload, payload, "--allowed-diff-manifest", str(manifest)
        )
        self.assertNotEqual(completed.returncode, 0)
        rejected = "\n".join(report["failures"])
        for path in changes:
            with self.subTest(path=path):
                self.assertIn(path, rejected)

    def test_legacy_stable_evidence_is_read_only_unverified(self) -> None:
        legacy = self.root / "legacy.json"
        legacy.write_text(
            json.dumps(
                {
                    "github": {"sha": "b" * 40},
                    "release": {
                        "agent_output_quality": {"candidate_sha": "a" * 40},
                        "rc_payload_verification": {"candidate_commit": "c" * 40},
                    },
                }
            ),
            encoding="utf-8",
        )
        output = self.root / "legacy-report.json"
        completed = run(
            "--legacy-provenance", str(legacy), "--output", str(output)
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["relation_verdict"], "unverified")
        self.assertIsNone(report["gate_source_sha"])


if __name__ == "__main__":
    unittest.main()
