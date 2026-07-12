#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from argparse import Namespace
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
        shapes = {task.semantic_shape for task in tasks}
        for required in (
            "missing_header",
            "warning_as_error",
            "generated_header_frontier",
            "depfile_compile",
            "concept_constraint",
            "ranges_constraint",
            "include_frontier",
            "system_header_frontier",
            "residual_unknown",
            "response_file",
            "parallel_make_interleaving",
            "library_order_link",
            "independent_units_2",
            "independent_units_3",
            "independent_units_4",
            "independent_units_5",
            "independent_units_5_multi_tu",
        ):
            self.assertIn(required, shapes)

    def test_variants_are_source_distinct(self) -> None:
        for index in range(120):
            hashes = {
                HARNESS.canonical_hash(HARNESS.task_for(index, variant, 1).files)
                for variant in range(3)
            }
            self.assertEqual(len(hashes), 3, f"family {index + 1} variants collided")

    def test_representative_native_tasks_begin_failing(self) -> None:
        for index in [*range(10), *range(40, 50), *range(80, 90)]:
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

    def test_synthetic_full_packet_analyzes_and_verifies(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            for source in HARNESS.STATIC_FILES:
                shutil.copy2(source, root / source.name)
            analysis = json.loads((root / "analysis-plan.json").read_text())
            analysis["bootstrap"]["replicates"] = 100
            HARNESS.write_json(root / "analysis-plan.json", analysis)
            HARNESS.write_json(
                root / "candidate-freeze.json",
                {"candidate_sha": "a" * 64, "schema_version": 1},
            )
            HARNESS.write_json(root / "corpus-manifest.json", {"schema_version": 1})
            HARNESS.write_json(root / "seed-commitment.json", {"schema_version": 1})
            HARNESS.write_json(root / "materialization-freeze.json", {"schema_version": 1})
            mapping = {"A": "native_gcc", "B": "current_default", "C": "candidate"}
            HARNESS.write_json(
                root / "control" / "condition-key.sealed.json",
                {"schema_version": 1, "mapping": mapping, "commitment": HARNESS.canonical_hash(mapping)},
            )
            rows = []
            for family in range(120):
                for label in HARNESS.LABELS:
                    trial_id = f"T-{family:03d}-{label}"
                    trial = root / "trials" / trial_id
                    work = trial / "work"
                    work.mkdir(parents=True)
                    diagnostic_bytes = 50 if label == "C" else 100
                    (work / "DIAGNOSTIC.txt").write_text(
                        "src/main.c:1:1: error: bad\n1 | bad\n  | ^\n"
                        "details: --formed-explain | raw: --formed-raw\n"
                    )
                    (trial / "transcript.jsonl").write_text("")
                    (trial / "agent-stderr.txt").write_text("")
                    HARNESS.write_json(trial / "agent-command.json", {"schema_version": 1})
                    (trial / "final.patch").write_text("diff\n")
                    (work / "TASK.json").write_text("{}\n")
                    (work / "build.sh").write_text("#!/bin/sh\n")
                    attempt_root = work / ".trial"
                    attempt_root.mkdir()
                    for name in (
                        "patch-1.diff",
                        "status-1.txt",
                        "stdout-1.txt",
                        "stderr-1.txt",
                        "exit-1.txt",
                    ):
                        (attempt_root / name).write_text("0\n")
                    HARNESS.write_json(
                        trial / "controller.json",
                        {
                            "schema_version": 1,
                            "trial_id": trial_id,
                            "semantic_family_id": f"F{family + 1:03d}",
                            "condition_label": label,
                            "diagnostic_sha256": HARNESS.sha256_file(
                                work / "DIAGNOSTIC.txt"
                            ),
                            "initial_source": [],
                            "allowed_files": [],
                            "stratum": (
                                "simple_native_strong"
                                if family < 40
                                else "diagnostic_flood_semantic_heavy"
                                if family < 80
                                else "multi_file_build_real_project"
                            ),
                        },
                    )
                    HARNESS.write_json(
                        trial / "score.json",
                        {
                            "schema_version": 1,
                            "trial_id": trial_id,
                            "semantic_family_id": f"F{family + 1:03d}",
                            "condition_label": label,
                            "started": True,
                            "valid": True,
                            "process_returncode": 0,
                            "elapsed_ms": 1,
                            "invalid_final_schema": False,
                            "build_attempts": 1,
                            "first_success_attempt": 1,
                            "first_patch_success": True,
                            "success_within_three_loops": True,
                            "wrong_file_or_anchor": False,
                            "changed_files": ["src/main.c"],
                            "first_patch_size_lines": 2,
                            "diagnostic_bytes": diagnostic_bytes,
                            "tool_calls": 2,
                            "source_bytes_inspected": 20,
                            "files_opened": 1,
                            "final_source": [],
                        },
                    )
                    rows.append(
                        {
                            "trial_id": trial_id,
                            "semantic_family_id": f"F{family + 1:03d}",
                            "condition_label": label,
                            "status": "complete",
                        }
                    )
            (root / "trial-index.jsonl").write_text(
                "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows)
            )

            qualification = HARNESS.analyze(Namespace(packet_root=root))
            self.assertEqual(
                qualification["verdict"],
                "pass",
                qualification,
            )
            integrity = HARNESS.verify(Namespace(packet_root=root))

            self.assertEqual(integrity["overall_status"], "pass")


if __name__ == "__main__":
    unittest.main()
