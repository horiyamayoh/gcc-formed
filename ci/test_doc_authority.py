import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent

METADATA_DOCS = [
    REPO_ROOT / "AGENTS.md",
    REPO_ROOT / "README.md",
    *sorted((REPO_ROOT / "docs").rglob("*.md")),
    *sorted((REPO_ROOT / "adr-initial-set").rglob("*.md")),
]

REQUIRED_METADATA_FIELDS = {
    "doc_role",
    "lifecycle_status",
    "audience",
    "use_for",
    "do_not_use_for",
    "supersedes",
    "superseded_by",
}

ALLOWED_DOC_ROLES = {"current-authority", "reference-only", "history-only"}
ALLOWED_LIFECYCLE_STATUS = {
    "accepted-baseline",
    "draft",
    "superseded",
    "legacy",
    "archived",
}
ALLOWED_AUDIENCE = {"human", "agent", "both"}

PRIMARY_SECTIONS = [
    (Path("AGENTS.md"), "## Current Authority Order"),
    (Path("README.md"), "## この repo の読み方"),
    (Path("docs/README.md"), "## Current Authority"),
]

BANNED_CURRENT_AUTHORITY_PHRASES = [
    "compatibility-only",
    "GCC 15-first support policy",
    "GCC 15 blocker portion of `nightly-gate`",
    "production render",
    "production quality path",
    "support tier",
    "Tier A",
    "Tier B",
    "Tier C",
    "Representative GCC 15 snapshot check",
]

ALLOWED_BANNED_PHRASE_DOCS = {
    Path("AGENTS.md"),
}

MARKDOWN_LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
ARCHITECTURE_DOC = Path("docs/architecture/gcc-formed-vnext-change-design.md")
ARCHITECTURE_STALE_RUNTIME_PHRASES = [
    "`diag_backend_probe` は `support_tier_for_major` で `15+ -> A`, `13/14 -> B`, それ以外 -> `C` を返し",
    "`diag_adapter_gcc::ingest` は `sarif_path: Option<&Path>` と `stderr_text: &str` を受け",
    "| バージョン判定 | `SupportTier` で 15+/13-14/その他 を直結 |",
    "| ingest | `ingest(sarif_path, stderr_text, ...)` |",
]


def extract_front_matter(text: str) -> dict[str, object]:
    if not text.startswith("---\n"):
        raise AssertionError("missing YAML front matter")

    end = text.find("\n---\n", 4)
    if end == -1:
        raise AssertionError("unterminated YAML front matter")

    block = text[4:end]
    data: dict[str, object] = {}
    lines = block.splitlines()
    index = 0

    while index < len(lines):
        line = lines[index]
        if not line.strip():
            index += 1
            continue
        key, value = line.split(":", 1)
        value = value.lstrip()
        if value == "[]":
            data[key] = []
            index += 1
            continue
        if value:
            data[key] = value
            index += 1
            continue
        index += 1
        items: list[str] = []
        while index < len(lines) and lines[index].startswith("  - "):
            items.append(lines[index][4:])
            index += 1
        data[key] = items

    return data


def read_metadata(relative_path: Path) -> dict[str, object]:
    text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
    return extract_front_matter(text)


def current_authority_docs() -> list[Path]:
    docs: list[Path] = []
    for path in METADATA_DOCS:
        relative = path.relative_to(REPO_ROOT)
        metadata = read_metadata(relative)
        if metadata["doc_role"] == "current-authority":
            docs.append(relative)
    return docs


def extract_section(text: str, heading: str) -> str:
    lines = text.splitlines()
    capture = False
    section: list[str] = []
    heading_level = len(heading) - len(heading.lstrip("#"))

    for line in lines:
        if line.strip() == heading:
            capture = True
            continue
        if capture and line.startswith("#"):
            level = len(line) - len(line.lstrip("#"))
            if level <= heading_level:
                break
        if capture:
            section.append(line)

    if not section:
        raise AssertionError(f"section not found or empty: {heading}")

    return "\n".join(section)


def iter_internal_doc_links(relative_path: Path, text: str) -> list[Path]:
    links: list[Path] = []
    for match in MARKDOWN_LINK_RE.finditer(text):
        target = match.group(1).split("#", 1)[0]
        if not target or "://" in target or target.startswith("/"):
            continue
        if not target.endswith(".md") and not target.endswith(".tar.gz"):
            continue
        resolved = (REPO_ROOT / relative_path.parent / target).resolve()
        links.append(resolved.relative_to(REPO_ROOT))
    return links


class DocAuthorityTest(unittest.TestCase):
    def test_all_tracked_docs_have_required_metadata(self) -> None:
        for path in METADATA_DOCS:
            relative = path.relative_to(REPO_ROOT)
            metadata = read_metadata(relative)
            with self.subTest(path=relative.as_posix()):
                self.assertTrue(REQUIRED_METADATA_FIELDS.issubset(metadata.keys()))
                self.assertIn(metadata["doc_role"], ALLOWED_DOC_ROLES)
                self.assertIn(metadata["lifecycle_status"], ALLOWED_LIFECYCLE_STATUS)
                self.assertIn(metadata["audience"], ALLOWED_AUDIENCE)
                self.assertIsInstance(metadata["supersedes"], list)
                self.assertIsInstance(metadata["superseded_by"], list)
                text = path.read_text(encoding="utf-8")
                self.assertIn("> [!IMPORTANT]", text)
                self.assertIn("Authority:", text)
                self.assertIn("Use for:", text)
                self.assertIn("Do not use for:", text)

    def test_docs_planning_is_reference_only(self) -> None:
        for path in sorted((REPO_ROOT / "docs" / "planning").rglob("*.md")):
            metadata = read_metadata(path.relative_to(REPO_ROOT))
            with self.subTest(path=path.relative_to(REPO_ROOT).as_posix()):
                self.assertEqual(metadata["doc_role"], "reference-only")

    def test_history_and_superseded_adrs_are_not_current_authority(self) -> None:
        paths = [
            *sorted((REPO_ROOT / "docs" / "history").rglob("*.md")),
            *sorted((REPO_ROOT / "adr-initial-set" / "superseded").rglob("*.md")),
        ]
        for path in paths:
            metadata = read_metadata(path.relative_to(REPO_ROOT))
            with self.subTest(path=path.relative_to(REPO_ROOT).as_posix()):
                self.assertNotEqual(metadata["doc_role"], "current-authority")

    def test_primary_read_order_links_only_current_authority_docs(self) -> None:
        for relative_path, heading in PRIMARY_SECTIONS:
            text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
            section = extract_section(text, heading)
            for target in iter_internal_doc_links(relative_path, section):
                with self.subTest(source=relative_path.as_posix(), target=target.as_posix()):
                    self.assertFalse(target.as_posix().startswith("docs/history/"))
                    self.assertFalse(target.as_posix().startswith("adr-initial-set/superseded/"))
                    if target.suffix == ".md":
                        metadata = read_metadata(target)
                        self.assertEqual(metadata["doc_role"], "current-authority")

    def test_current_authority_docs_avoid_legacy_wording(self) -> None:
        for relative_path in current_authority_docs():
            if relative_path in ALLOWED_BANNED_PHRASE_DOCS:
                continue
            text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
            for phrase in BANNED_CURRENT_AUTHORITY_PHRASES:
                with self.subTest(path=relative_path.as_posix(), phrase=phrase):
                    self.assertNotIn(phrase, text)

    def test_current_authority_links_resolve(self) -> None:
        for relative_path in current_authority_docs():
            text = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
            for target in iter_internal_doc_links(relative_path, text):
                with self.subTest(source=relative_path.as_posix(), target=target.as_posix()):
                    self.assertTrue((REPO_ROOT / target).exists())

    def test_architecture_doc_avoids_stale_parity_runtime_wording(self) -> None:
        text = (REPO_ROOT / ARCHITECTURE_DOC).read_text(encoding="utf-8")
        for phrase in ARCHITECTURE_STALE_RUNTIME_PHRASES:
            with self.subTest(phrase=phrase):
                self.assertNotIn(phrase, text)


if __name__ == "__main__":
    unittest.main()
