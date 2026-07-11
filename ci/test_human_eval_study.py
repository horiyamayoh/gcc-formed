from __future__ import annotations

import csv
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repair_unit_study", REPO / "eval" / "repair-units-v1" / "study.py"
)
study = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(study)


class HumanEvalStudyTest(unittest.TestCase):
    def make_study(self, populated: bool) -> Path:
        root = Path(tempfile.mkdtemp())
        for name in ["analysis-plan.json", "trials.csv"]:
            (root / name).write_bytes((REPO / "eval" / "repair-units-v1" / name).read_bytes())
        if not populated:
            return root
        (root / "condition-key.json").write_text(
            json.dumps({"A": "native_gcc", "B": "subject_blocks_v2", "C": "repair_units_v1"}),
            encoding="utf-8",
        )
        with (root / "trials.csv").open(encoding="utf-8") as source:
            fields = next(csv.reader(source))
        with (root / "trials.csv").open("a", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=fields)
            for participant in range(8):
                for sequence in range(12):
                    condition = "ABC"[sequence % 3]
                    candidate = condition == "C"
                    writer.writerow({
                        "trial_id": f"T{participant:02d}-{sequence:02d}",
                        "participant_code": f"P{participant:02d}",
                        "experience_confirmed": "true",
                        "sequence": sequence,
                        "condition": condition,
                        "fixture_id": f"fixture-{sequence}",
                        "language": "c" if sequence % 2 else "cpp",
                        "shape": "syntax",
                        "noise_class": "simple" if sequence < 6 else "noisy",
                        "defect_count": 1 if sequence < 6 else 2,
                        "time_to_first_correct_edit_ms": 800 if candidate else 1000,
                        "first_edit_correct": "true",
                        "first_fix_success": "true",
                        "target_selection_correct": "true",
                        "irrelevant_lines_inspected": 1,
                        "raw_requests": 0,
                        "explain_requests": 0,
                        "high_confidence_mislead": "false",
                        "abandoned": "false",
                        "exclusion_reason": "",
                    })
        return root

    def test_empty_study_is_inconclusive_and_blocks_promotion(self) -> None:
        root = self.make_study(False)
        report = study.analyze(root, root / "analysis")
        self.assertEqual(report["recommendation"], "inconclusive")
        self.assertTrue(report["promotion_blocked"])

    def test_powered_counterbalanced_noninferior_study_passes(self) -> None:
        root = self.make_study(True)
        validation = study.validate(root)
        self.assertEqual(validation["verdict"], "pass")
        report = study.analyze(root, root / "analysis")
        self.assertEqual(report["recommendation"], "pass")
        self.assertFalse(report["promotion_blocked"])


if __name__ == "__main__":
    unittest.main()
