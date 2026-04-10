import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent

GOVERNANCE_HEADINGS = [
    "# Change Governance",
    "## Current Freeze",
    "## Stable Contract Surfaces",
    "## Change Classification",
    "### Breaking",
    "### Non-Breaking",
    "### Experimental",
    "## Pre-1.0 Must-Have Backlog",
    "## Post-1.0 Backlog",
    "## Reviewer Checklist",
]

POST_STABLE_BACKLOG_ITEMS = [
    "non-Linux production artifacts",
    "GCC 13/14 enhanced-render quality guarantees",
    "elimination of passthrough, shadow mode, or raw fallback",
    "package-manager-native distribution as the primary release path",
    "self-updater flows",
    "container-primary distribution",
    "Clang support",
    "editor integration, daemon mode, TUI surfaces, or auto-fix apply flows",
]


class GovernanceDocsTest(unittest.TestCase):
    def test_governance_doc_has_change_classes_and_backlog_split(self) -> None:
        text = (REPO_ROOT / "docs/policies/GOVERNANCE.md").read_text(encoding="utf-8")
        for heading in GOVERNANCE_HEADINGS:
            with self.subTest(heading=heading):
                self.assertIn(heading, text)
        for item in POST_STABLE_BACKLOG_ITEMS:
            with self.subTest(item=item):
                self.assertIn(item, text)

    def test_adr_0020_references_governance_and_change_classes(self) -> None:
        text = (REPO_ROOT / "adr-initial-set" / "adr-0020-stability-promises.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("../docs/policies/GOVERNANCE.md", text)
        self.assertIn("`breaking` / `non-breaking` / `experimental`", text)
        self.assertIn("post-`1.0.0`", text)

    def test_reader_docs_link_governance(self) -> None:
        expected_references = {
            "README.md": "docs/policies/GOVERNANCE.md",
            "docs/policies/VERSIONING.md": "(GOVERNANCE.md)",
            "CONTRIBUTING.md": "docs/policies/GOVERNANCE.md",
            "docs/releases/RELEASE-CHECKLIST.md": "../policies/GOVERNANCE.md",
        }
        for relative_path, expected in expected_references.items():
            with self.subTest(path=relative_path):
                text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
                self.assertIn(expected, text)


if __name__ == "__main__":
    unittest.main()
