import json
import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
README = REPO_ROOT / "README.md"
EXAMPLE_MANIFEST = REPO_ROOT / "ci" / "readme_examples.json"
MARKDOWN_LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
PROCESSING_PATHS = {
    "dual_sink_structured",
    "single_sink_structured",
    "native_text_capture",
    "passthrough",
}
LIVE_DOCS = [
    "README.md",
    "CHANGELOG.md",
    "corpus/README.md",
    "docs/support/OPERATOR-INTEROP.md",
    "docs/releases/RELEASE-CHECKLIST.md",
    "docs/releases/RC-RELEASE.md",
]
STALE_PRESENT_TENSE = [
    "Current Beta-Bar Target",
    "現在の RC support level",
    "RC support posture",
    "Versioned beta policy for this issue scope",
    "Current RC Support Boundary",
    "The current source candidate is `1.0.0-rc.1`",
    "publication still requires the signed RC release gates",
]


def repository_relative_links(text: str) -> list[str]:
    links: list[str] = []
    for target in MARKDOWN_LINK_RE.findall(text):
        target = target.split("#", 1)[0]
        if not target or "://" in target or target.startswith(("#", "/", "mailto:")):
            continue
        links.append(target)
    return links


class StableDocumentationContractTest(unittest.TestCase):
    def test_all_readme_repository_relative_links_resolve(self) -> None:
        for target in repository_relative_links(README.read_text(encoding="utf-8")):
            with self.subTest(target=target):
                self.assertTrue((REPO_ROOT / target).exists())

    def test_readme_snapshot_links_use_band_and_processing_path(self) -> None:
        links = repository_relative_links(README.read_text(encoding="utf-8"))
        snapshot_links = [target for target in links if "/snapshots/" in target]
        self.assertGreaterEqual(len(snapshot_links), 4)
        for target in snapshot_links:
            with self.subTest(target=target):
                suffix = target.split("/snapshots/", 1)[1].split("/")
                self.assertGreaterEqual(len(suffix), 3)
                self.assertIn(suffix[1], PROCESSING_PATHS)

    def test_named_readme_examples_equal_canonical_snapshots(self) -> None:
        readme = README.read_text(encoding="utf-8")
        manifest = json.loads(EXAMPLE_MANIFEST.read_text(encoding="utf-8"))
        self.assertEqual(manifest["schema_version"], 1)
        for example in manifest["examples"]:
            marker = re.escape(f"<!-- canonical-example: {example['id']} -->")
            match = re.search(marker + r"\n```text\n(?P<body>.*?)\n```", readme, re.DOTALL)
            with self.subTest(example=example["id"]):
                self.assertIsNotNone(match)
                path = REPO_ROOT / example["path"]
                self.assertTrue(path.exists())
                self.assertEqual(match.group("body"), path.read_text(encoding="utf-8").rstrip("\n"))
                self.assertIn(f"]({example['path']})", readme)

    def test_live_docs_do_not_reintroduce_pre_stable_present_tense(self) -> None:
        for relative in LIVE_DOCS:
            text = (REPO_ROOT / relative).read_text(encoding="utf-8")
            for phrase in STALE_PRESENT_TENSE:
                with self.subTest(path=relative, phrase=phrase):
                    self.assertNotIn(phrase, text)

    def test_historical_beta_release_record_remains_explicit(self) -> None:
        release_notes = (REPO_ROOT / "docs/releases/RELEASE-NOTES.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("## 0.2.0-beta.1", release_notes)
        self.assertIn("### Historical `v1beta` Support Boundary", release_notes)
        self.assertIn("At beta publication time", release_notes)


if __name__ == "__main__":
    unittest.main()
