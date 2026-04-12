from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent


class PublicExportContractTest(unittest.TestCase):
    def test_xtask_report_bundle_includes_schema_fingerprint_sidecar(self) -> None:
        source = (REPO_ROOT / "xtask" / "src" / "commands" / "corpus.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn("PUBLIC_EXPORT_SCHEMA_FINGERPRINT_ARTIFACT", source)
        self.assertIn(
            "public.export.schema-shape-fingerprint.txt",
            source,
        )
        self.assertIn(
            "public_export_schema_shape_fingerprint_for_export",
            source,
        )

    def test_public_export_helper_has_shape_and_drift_coverage(self) -> None:
        source = (REPO_ROOT / "diag_public_export" / "src" / "lib.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn("schema_shape_fingerprint_ignores_scalar_value_changes", source)
        self.assertIn("schema_shape_fingerprint_changes_when_structure_changes", source)
        self.assertIn("schema_shape_fingerprint(export: &PublicDiagnosticExport)", source)


if __name__ == "__main__":
    unittest.main()
