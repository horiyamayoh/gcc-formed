import json
import subprocess
import sys
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "ci"))

import public_surface  # noqa: E402


class PublicSurfaceContractTest(unittest.TestCase):
    def test_repo_metadata_payload_matches_checked_in_contract(self) -> None:
        payload = public_surface.repo_metadata_payload()
        self.assertEqual(
            payload["description"],
            "GCC diagnostic UX wrapper for GCC 9-15 that keeps terminal output shorter, root-cause-first, and fail-open.",
        )
        self.assertEqual(
            payload["homepage"],
            "https://github.com/horiyamayoh/gcc-formed/blob/main/docs/README.md",
        )
        self.assertEqual(
            payload["topics"],
            ["gcc", "compiler-diagnostics", "c", "cpp", "cli", "developer-tools"],
        )
        self.assertEqual(payload["readme_tagline"], payload["description"])

    def test_readme_top_copy_matches_public_surface_contract(self) -> None:
        readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn(public_surface.repo_metadata_payload()["readme_tagline"], readme)

    def test_support_boundary_lines_feed_release_body_generation(self) -> None:
        body = public_surface.render_release_body(
            kind="beta",
            version="0.2.0-beta.1",
            repository="horiyamayoh/gcc-formed",
            commit="main",
            signing_key_id="test-key",
            signing_public_key_sha256="test-sha256",
            rollback_baseline_version=None,
        )
        for line in public_surface.support_boundary_lines():
            with self.subTest(line=line):
                self.assertIn(line, body)
        self.assertIn("## Current Docs", body)
        self.assertIn("docs/support/SUPPORT-BOUNDARY.md", body)
        self.assertIn("docs/support/KNOWN-LIMITATIONS.md", body)
        self.assertIn("docs/releases/PUBLIC-BETA-RELEASE.md", body)
        self.assertIn("signing key id: `test-key`", body)
        self.assertIn("trusted signing public key sha256: `test-sha256`", body)

    def test_stable_release_body_renders_current_doc_links_and_evidence(self) -> None:
        body = public_surface.render_release_body(
            kind="stable",
            version="1.0.0",
            repository="horiyamayoh/gcc-formed",
            commit="deadbeef",
            signing_key_id="stable-key",
            signing_public_key_sha256="stable-sha256",
            rollback_baseline_version="0.2.0-beta.1",
        )
        self.assertIn("rollback baseline version: `0.2.0-beta.1`", body)
        self.assertIn("signing key id: `stable-key`", body)
        self.assertIn("trusted signing public key sha256: `stable-sha256`", body)
        self.assertIn("docs/releases/STABLE-RELEASE.md", body)
        self.assertIn("docs/releases/RELEASE-CHECKLIST.md", body)
        for line in public_surface.support_boundary_lines():
            with self.subTest(line=line):
                self.assertIn(line, body)

    def test_cli_repo_metadata_and_release_body_commands_work(self) -> None:
        metadata = subprocess.run(
            ["python3", "ci/public_surface.py", "repo-metadata"],
            cwd=REPO_ROOT,
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        payload = json.loads(metadata.stdout)
        self.assertEqual(payload["description"], public_surface.repo_metadata_payload()["description"])

        beta_body = subprocess.run(
            [
                "python3",
                "ci/public_surface.py",
                "render-release-body",
                "--kind",
                "beta",
                "--version",
                "0.2.0-beta.1",
                "--repository",
                "horiyamayoh/gcc-formed",
                "--commit",
                "main",
                "--signing-key-id",
                "cli-key",
                "--signing-public-key-sha256",
                "cli-sha256",
            ],
            cwd=REPO_ROOT,
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        self.assertIn("cli-key", beta_body.stdout)
        self.assertIn("docs/support/SUPPORT-BOUNDARY.md", beta_body.stdout)

    def test_public_surface_doc_includes_manual_sync_commands(self) -> None:
        text = (REPO_ROOT / "docs" / "support" / "PUBLIC-SURFACE.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("sync-github-repo-metadata", text)
        self.assertIn("render-release-body --kind beta", text)
        self.assertIn("repo-metadata", text)


if __name__ == "__main__":
    unittest.main()
