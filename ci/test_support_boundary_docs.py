import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent

CANONICAL_LINES = [
    "- Linux first.",
    "- `x86_64-unknown-linux-musl` is the primary production artifact.",
    "- GCC 15 is the primary enhanced-render path.",
    "- The terminal renderer is the primary user-facing surface.",
    (
        "- GCC 13/14 are compatibility-only paths and may use conservative passthrough "
        "or shadow behavior instead of the primary enhanced-render path."
    ),
    (
        "- Raw fallback remains part of the shipped contract when the wrapper cannot "
        "produce a clearly better, trustworthy render."
    ),
]

DOCS_WITH_CANONICAL_COPY = [
    "SUPPORT-BOUNDARY.md",
    "README.md",
    "RELEASE-NOTES.md",
    "RELEASE-CHECKLIST.md",
    "KNOWN-LIMITATIONS.md",
    "SECURITY.md",
    "CONTRIBUTING.md",
]

PR_TEMPLATE_HEADINGS = [
    "## Goal",
    "## Why Now",
    "## Milestone / Work Package",
    "## Change Classification",
    "## Read Docs",
    "## Contract Surfaces",
    "## Files Touched",
    "## Constraints",
    "## Out Of Scope",
    "## Acceptance Criteria",
    "## Commands Run",
    "## Docs Updated",
    "## Snapshot / Corpus / Docs Update Rationale",
    "## Support Tier Impact",
    "## Trace / Fallback Impact",
]


class SupportBoundaryDocsTest(unittest.TestCase):
    def test_canonical_support_boundary_is_copied_into_key_docs(self) -> None:
        for relative_path in DOCS_WITH_CANONICAL_COPY:
            with self.subTest(path=relative_path):
                text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
                for line in CANONICAL_LINES:
                    self.assertIn(line, text)

    def test_pr_template_matches_required_sections(self) -> None:
        text = (REPO_ROOT / ".github" / "pull_request_template.md").read_text(
            encoding="utf-8"
        )
        for heading in PR_TEMPLATE_HEADINGS:
            with self.subTest(heading=heading):
                self.assertIn(heading, text)
        self.assertIn("GCC 15 primary enhanced-render path", text)
        self.assertIn("GCC 13/14 compatibility-only path", text)
        self.assertIn("SUPPORT-BOUNDARY.md", text)
        self.assertIn("GOVERNANCE.md", text)
        self.assertIn("adr-0020-stability-promises.md", text)

    def test_bug_template_uses_canonical_support_tier_labels(self) -> None:
        text = (REPO_ROOT / ".github" / "ISSUE_TEMPLATE" / "bug_report.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("GCC 15 primary enhanced-render path", text)
        self.assertIn("GCC 13/14 compatibility-only path", text)
        self.assertIn("SUPPORT-BOUNDARY.md", text)


if __name__ == "__main__":
    unittest.main()
