import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
RELEASE_PROVENANCE = REPO_ROOT / "ci" / "release_provenance.py"


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


class ReleaseProvenanceTest(unittest.TestCase):
    def run_release_provenance(self, *args: str, env: dict[str, str]) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["python3", str(RELEASE_PROVENANCE), *args],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=env,
        )

    def github_env(self, **extra: str) -> dict[str, str]:
        env = os.environ.copy()
        env.update(
            {
                "GITHUB_REPOSITORY": "horiyamayoh/gcc-formed",
                "GITHUB_SHA": "deadbeef",
                "GITHUB_REF": "refs/heads/main",
                "GITHUB_RUN_ID": "1234",
                "GITHUB_RUN_ATTEMPT": "2",
                "GITHUB_ACTOR": "codex",
            }
        )
        env.update(extra)
        return env

    def test_pr_gate_records_release_scope_without_legacy_support_tier(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_root = Path(tmpdir) / "reports"
            output_path = report_root / "release" / "release-provenance.json"
            write_json(report_root / "release" / "package.json", {"kind": "package"})
            write_json(report_root / "release" / "release-publish.json", {"kind": "publish"})

            completed = self.run_release_provenance(
                "--workflow",
                "pr-gate",
                "--report-root",
                str(report_root),
                "--output",
                str(output_path),
                "--package-version",
                "0.2.0-beta.1",
                "--target-triple",
                "x86_64-unknown-linux-musl",
                "--release-channel",
                "beta",
                "--maturity-label",
                "v1beta",
                env=self.github_env(),
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["workflow"], "pr-gate")
            self.assertEqual(payload["schema_version"], 1)
            self.assertEqual(payload["release_scope"]["maturity_label"], "v1beta")
            self.assertEqual(payload["release_scope"]["target_triple"], "x86_64-unknown-linux-musl")
            self.assertEqual(payload["release"]["package"]["kind"], "package")
            self.assertEqual(payload["release"]["release_publish"]["kind"], "publish")
            self.assertNotIn("support_tier", output_path.read_text(encoding="utf-8"))

    def test_nightly_gate_records_version_band_in_matrix_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_root = Path(tmpdir) / "reports"
            output_path = report_root / "release" / "release-provenance.json"
            write_json(report_root / "release" / "package.json", {"kind": "package"})
            write_json(report_root / "release" / "bench-smoke.json", {"kind": "bench"})

            completed = self.run_release_provenance(
                "--workflow",
                "nightly-gate",
                "--report-root",
                str(report_root),
                "--output",
                str(output_path),
                "--package-version",
                "0.2.0-beta.1",
                "--target-triple",
                "x86_64-unknown-linux-musl",
                "--release-channel",
                "beta",
                "--maturity-label",
                "v1beta",
                "--matrix-gcc-image",
                "gcc:14",
                "--matrix-version-band",
                "gcc13_14",
                "--release-blocker",
                "false",
                env=self.github_env(),
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(
                payload["matrix"],
                {
                    "gcc_image": "gcc:14",
                    "version_band": "gcc13_14",
                    "release_blocker": "false",
                },
            )
            self.assertEqual(payload["release_scope"]["maturity_label"], "v1beta")
            self.assertEqual(payload["release"]["bench_smoke"]["kind"], "bench")
            self.assertIsNone(payload["release"]["fuzz_smoke"])
            self.assertNotIn("support_tier", output_path.read_text(encoding="utf-8"))
            self.assertNotIn("gcc15_primary", output_path.read_text(encoding="utf-8"))

    def test_public_beta_release_records_release_scope_and_tag(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_root = Path(tmpdir) / "reports"
            output_path = report_root / "release" / "release-provenance.json"
            write_json(report_root / "release" / "package.json", {"kind": "package"})
            write_json(report_root / "release" / "install-release.json", {"kind": "install-release"})

            completed = self.run_release_provenance(
                "--workflow",
                "public-beta-release",
                "--report-root",
                str(report_root),
                "--output",
                str(output_path),
                "--package-version",
                "0.2.0-beta.1",
                "--target-triple",
                "x86_64-unknown-linux-musl",
                "--release-channel",
                "beta",
                "--maturity-label",
                "v1beta",
                "--signing-key-id",
                "sig-1",
                "--signing-public-key-sha256",
                "abcd1234",
                env=self.github_env(RELEASE_TAG="v0.2.0-beta.1"),
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["github"]["tag"], "v0.2.0-beta.1")
            self.assertEqual(payload["release_scope"]["release_channel"], "beta")
            self.assertEqual(payload["release_scope"]["maturity_label"], "v1beta")
            self.assertEqual(payload["release_scope"]["signing_key_id"], "sig-1")
            self.assertEqual(payload["release"]["install_release"]["kind"], "install-release")
            self.assertNotIn("support_tier", output_path.read_text(encoding="utf-8"))

    def test_stable_release_records_rollback_scope_and_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_root = Path(tmpdir) / "reports"
            output_path = report_root / "release" / "release-provenance.json"
            write_json(report_root / "release" / "stable-release-command.json", {"kind": "command"})
            write_json(
                report_root / "stable-release" / "promotion-evidence.json",
                {"kind": "promotion"},
            )
            write_json(report_root / "rc-gate" / "rc-gate-report.json", {"kind": "rc-gate"})

            completed = self.run_release_provenance(
                "--workflow",
                "stable-release",
                "--report-root",
                str(report_root),
                "--output",
                str(output_path),
                "--package-version",
                "1.0.0",
                "--target-triple",
                "x86_64-unknown-linux-musl",
                "--release-channel",
                "stable",
                "--maturity-label",
                "v1beta",
                "--rollback-baseline-version",
                "0.2.0-beta.1",
                "--signing-key-id",
                "sig-2",
                "--signing-public-key-sha256",
                "efgh5678",
                env=self.github_env(RELEASE_TAG="v1.0.0"),
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["workflow"], "stable-release")
            self.assertEqual(payload["release_scope"]["rollback_baseline_version"], "0.2.0-beta.1")
            self.assertEqual(payload["release_scope"]["release_channel"], "stable")
            self.assertEqual(payload["release"]["stable_release_command"]["kind"], "command")
            self.assertEqual(payload["release"]["promotion_evidence"]["kind"], "promotion")
            self.assertEqual(payload["release"]["rc_gate"]["kind"], "rc-gate")
            self.assertNotIn("support_tier", output_path.read_text(encoding="utf-8"))
