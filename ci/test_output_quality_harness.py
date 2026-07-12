#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
HARNESS_PATH = REPO / "eval" / "output-quality-single-agent-v1" / "harness.py"
SPEC = importlib.util.spec_from_file_location("output_quality_harness", HARNESS_PATH)
assert SPEC is not None and SPEC.loader is not None
HARNESS = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = HARNESS
SPEC.loader.exec_module(HARNESS)


class OutputQualityHarnessTests(unittest.TestCase):
    def test_static_contract_is_frozen_and_single_agent(self) -> None:
        report = HARNESS.validate_static()
        self.assertEqual(report["status"], "pass")

    def test_generator_has_120_families_and_required_strata(self) -> None:
        tasks = [HARNESS.task_for(index, 0, 1) for index in range(120)]
        self.assertEqual(len({task.family_id for task in tasks}), 120)
        counts: dict[str, int] = {}
        for task in tasks:
            counts[task.stratum] = counts.get(task.stratum, 0) + 1
        self.assertEqual(
            counts,
            {
                "simple_native_strong": 40,
                "diagnostic_flood_semantic_heavy": 40,
                "multi_file_build_real_project": 40,
            },
        )

    def test_variants_are_source_distinct(self) -> None:
        for index in range(120):
            hashes = {
                HARNESS.canonical_hash(HARNESS.task_for(index, variant, 1).files)
                for variant in range(3)
            }
            self.assertEqual(len(hashes), 3, f"family {index + 1} variants collided")

    def test_representative_native_tasks_begin_failing(self) -> None:
        for index in range(0, 120, 10):
            task = HARNESS.task_for(index, index % 3, 1)
            with self.subTest(family=task.family_id), tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                for relative, content in task.files.items():
                    path = root / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(content, encoding="utf-8")
                (root / "build").mkdir()
                completed = subprocess.run(
                    task.build_command,
                    cwd=root,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                )
                self.assertNotEqual(completed.returncode, 0, task.family_id)
                self.assertTrue(completed.stderr or completed.stdout, task.family_id)


if __name__ == "__main__":
    unittest.main()
