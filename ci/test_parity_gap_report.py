import tempfile
import unittest
from pathlib import Path

from ci.parity_gap_report import build_report


class ParityGapReportTest(unittest.TestCase):
    def test_documentary_gaps_are_emitted_and_critical_cells_block(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory) / "cpp" / "sample" / "case-01"
            fixture.mkdir(parents=True)
            lines = [
                "corpus_id: cpp/sample/case-01",
                "tags: [sample]",
                "older_band_applicability:",
                "  shared_contract_when_emitted: true",
                "  gcc13_14:",
                "    native_text_capture:",
                "      status: missing_representative_evidence",
                "      parity_critical: true",
                "      note: required cell",
                "matrix_applicability:",
                "  version_band: gcc15",
                "  processing_path: dual_sink_structured",
                "  surfaces: [default, ci]",
                "  required_surfaces: [default, ci, debug]",
            ]
            (fixture / "meta.yaml").write_text("\n".join(lines) + "\n", encoding="utf-8")
            report = build_report(Path(directory))
            self.assertEqual(report["status"], "fail")
            self.assertEqual(report["critical_gap_count"], 2)
            self.assertEqual({gap["kind"] for gap in report["gaps"]}, {"family", "surface"})


if __name__ == "__main__":
    unittest.main()
