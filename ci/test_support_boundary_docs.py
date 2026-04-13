import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent

CANONICAL_LINES = [
    "- Linux first.",
    "- `x86_64-unknown-linux-musl` is the primary production artifact.",
    "- The terminal renderer is the primary user-facing surface.",
    "- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.",
    (
        "- `VersionBand` and `ProcessingPath` remain observability metadata; "
        "they do not encode unequal user value inside `GCC 9-15`."
    ),
    "- `GCC16+`, `<=8`, and unknown gcc-like compilers are `PassthroughOnly` until separately evidenced.",
    "- Internal capture mechanisms and raw-preservation details may differ by capability and invocation.",
    (
        "- Raw fallback remains part of the shipped contract when the wrapper cannot "
        "produce a clearly better, trustworthy result."
    ),
]

DOCS_WITH_CANONICAL_COPY = [
    "docs/support/SUPPORT-BOUNDARY.md",
    "docs/releases/RELEASE-NOTES.md",
    "docs/releases/RELEASE-CHECKLIST.md",
    "SECURITY.md",
    "SUPPORT.md",
    "CONTRIBUTING.md",
]

RELEASE_CHECKLIST_REQUIRED_SNIPPETS = [
    "- `pr-gate` is green on `main`.",
    "- The `nightly-gate` blocker portion is green across the current multi-band matrix.",
    "- Representative acceptance replay is green and the report artifacts are attached.",
    "- Representative matrix snapshot check is green and the report artifacts are attached.",
    "- Release artifacts include `release-provenance.json`.",
]

README_REQUIRED_SNIPPETS = [
    "[AGENTS.md](AGENTS.md)",
    "[docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md)",
    "[docs/process/EXECUTION-MODEL.md](docs/process/EXECUTION-MODEL.md)",
    "[docs/releases/PUBLIC-BETA-RELEASE.md](docs/releases/PUBLIC-BETA-RELEASE.md)",
    "**GCC 15・GCC 13–14・GCC 9–12 は 1 つの in-scope public contract を共有する。**",
    "**VersionBand と ProcessingPath は observability metadata であり、GCC 9–15 の価値序列を表さない。**",
    "**raw fallback は shipped contract の一部である。**",
    "**default TTY は native GCC より読みにくくなってはならない。**",
]

PR_TEMPLATE_HEADINGS = [
    "## Goal",
    "## Why Now",
    "## Parent Issue / Work Package",
    "## Workstream / Band / Layer",
    "## Change Classification",
    "## Read Docs",
    "## Contract Surfaces",
    "## In Scope",
    "## Out Of Scope",
    "## Acceptance Criteria",
    "## Evidence",
    "### Commands Run",
    "## Path Impact",
    "## Non-Negotiables",
    "## Docs Updated",
    "## Human Review Requested",
    "## Risk / Rollback",
    "## Pause / Resume",
]


class SupportBoundaryDocsTest(unittest.TestCase):
    def test_canonical_support_boundary_is_copied_into_key_docs(self) -> None:
        for relative_path in DOCS_WITH_CANONICAL_COPY:
            with self.subTest(path=relative_path):
                text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
                for line in CANONICAL_LINES:
                    self.assertIn(line, text)

    def test_readme_keeps_summary_and_navigation_contract(self) -> None:
        text = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
        for snippet in README_REQUIRED_SNIPPETS:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, text)
        self.assertIn("| VersionBand |", text)
        self.assertIn("| GCC 15 |", text)
        self.assertIn("SupportLevel / ProcessingPath / RawPreservationLevel", text)
        self.assertIn(
            "GCC diagnostic UX wrapper for GCC 9-15 that keeps terminal output shorter, root-cause-first, and fail-open.",
            text,
        )

    def test_pr_template_matches_required_sections(self) -> None:
        text = (REPO_ROOT / ".github" / "pull_request_template.md").read_text(
            encoding="utf-8"
        )
        for heading in PR_TEMPLATE_HEADINGS:
            with self.subTest(heading=heading):
                self.assertIn(heading, text)
        self.assertIn("VersionBand", text)
        self.assertIn("ProcessingPath", text)
        self.assertIn("docs/support/SUPPORT-BOUNDARY.md", text)
        self.assertIn("docs/process/EXECUTION-MODEL.md", text)
        self.assertIn("Acceptance evidence:", text)
        self.assertIn("Stop condition not hit:", text)
        self.assertIn("Next recommended action if paused:", text)

    def test_release_checklist_uses_current_multi_band_blocker_wording(self) -> None:
        text = (REPO_ROOT / "docs" / "releases" / "RELEASE-CHECKLIST.md").read_text(
            encoding="utf-8"
        )
        for snippet in RELEASE_CHECKLIST_REQUIRED_SNIPPETS:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, text)

    def test_bug_template_uses_vnext_vocabulary(self) -> None:
        text = (REPO_ROOT / ".github" / "ISSUE_TEMPLATE" / "bug_report.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("Version band", text)
        self.assertIn("Processing path", text)
        self.assertIn("docs/support/SUPPORT-BOUNDARY.md", text)
        self.assertIn("docs/process/EXECUTION-MODEL.md", text)

    def test_issue_templates_for_epic_and_work_package_exist(self) -> None:
        epic = (REPO_ROOT / ".github" / "ISSUE_TEMPLATE" / "epic.yml").read_text(
            encoding="utf-8"
        )
        work_package = (
            REPO_ROOT / ".github" / "ISSUE_TEMPLATE" / "work_package.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("Why this matters to doctrine", epic)
        self.assertIn("Allowed files", work_package)
        self.assertIn("Reviewer evidence", work_package)


if __name__ == "__main__":
    unittest.main()
