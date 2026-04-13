import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
SPEC_PATH = (
    REPO_ROOT / "docs" / "specs" / "public-machine-readable-diagnostic-surface-spec.md"
)
LIB_PATH = REPO_ROOT / "diag_public_export" / "src" / "lib.rs"


class PublicDiagnosticExportDocsTest(unittest.TestCase):
    def test_spec_tracks_current_code_constants(self) -> None:
        spec = SPEC_PATH.read_text(encoding="utf-8")
        lib = LIB_PATH.read_text(encoding="utf-8")

        schema_version = re.search(
            r'PUBLIC_EXPORT_SCHEMA_VERSION: &str = "([^"]+)"', lib
        ).group(1)
        kind = re.search(r'PUBLIC_EXPORT_KIND: &str = "([^"]+)"', lib).group(1)

        self.assertIn(schema_version, spec)
        self.assertIn(kind, spec)

    def test_spec_documents_actual_public_fields(self) -> None:
        spec = SPEC_PATH.read_text(encoding="utf-8")
        required_terms = [
            "`schema_version`",
            "`kind`",
            "`status`",
            "`producer`",
            "`invocation`",
            "`execution`",
            "`result`",
            "`unavailable_reason`",
            "`version_band`",
            "`processing_path`",
            "`support_level`",
            "`allowed_processing_paths`",
            "`public.export.json`",
            "`public.export.schema-shape-fingerprint.txt`",
            "`--formed-public-json=<sink>`",
        ]
        for term in required_terms:
            with self.subTest(term=term):
                self.assertIn(term, spec)

    def test_reader_docs_prefer_public_json_over_scraping(self) -> None:
        docs = {
            "README.md": (REPO_ROOT / "README.md").read_text(encoding="utf-8"),
            "AGENTS.md": (REPO_ROOT / "AGENTS.md").read_text(encoding="utf-8"),
            "SUPPORT.md": (REPO_ROOT / "SUPPORT.md").read_text(encoding="utf-8"),
            "ci/README.md": (REPO_ROOT / "ci" / "README.md").read_text(encoding="utf-8"),
        }

        self.assertIn("docs/specs/public-machine-readable-diagnostic-surface-spec.md", docs["README.md"])
        self.assertIn("--formed-public-json", docs["README.md"])

        self.assertIn("docs/specs/public-machine-readable-diagnostic-surface-spec.md", docs["AGENTS.md"])
        self.assertIn("--formed-public-json", docs["AGENTS.md"])

        self.assertIn("public-machine-readable-diagnostic-surface-spec.md", docs["SUPPORT.md"])
        self.assertIn("JSON export", docs["SUPPORT.md"])

        self.assertIn("public.export.json", docs["ci/README.md"])
        self.assertIn("public.export.schema-shape-fingerprint.txt", docs["ci/README.md"])
        self.assertIn(".execution.version_band", docs["ci/README.md"])


if __name__ == "__main__":
    unittest.main()
