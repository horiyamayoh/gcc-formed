#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]


def load(name: str, relative: str):
    path = REPO / relative
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


V1 = load("output_quality_harness_v1_for_v2_tests", "eval/output-quality-single-agent-v1/harness.py")
V2 = load("output_quality_harness_v2", "eval/output-quality-single-agent-v2/harness.py")


class OutputQualityHarnessV2Tests(unittest.TestCase):
    def test_static_contract_and_source_disjoint_family_epoch(self) -> None:
        self.assertEqual(V2.base.validate_static()["status"], "pass")
        v2_tasks = [V2.task_for_v2(index, 0, 1) for index in range(120)]
        self.assertEqual(v2_tasks[0].family_id, "F121")
        self.assertEqual(v2_tasks[-1].family_id, "F240")
        self.assertEqual(len({task.family_id for task in v2_tasks}), 120)
        for index, task in enumerate(v2_tasks):
            self.assertNotEqual(
                V1.canonical_hash(V1.task_for(index, 0, 1).files),
                V2.base.canonical_hash(task.files),
            )

    def test_corpus_manifest_family_ids_follow_v2_epoch(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "corpus-manifest.json"
            V2.write_json_v2(
                path,
                {
                    "families": [
                        {"semantic_family_id": f"F{index + 1:03d}"}
                        for index in range(120)
                    ]
                },
            )
            families = json.loads(path.read_text())["families"]
            self.assertEqual(families[0]["semantic_family_id"], "F121")
            self.assertEqual(families[-1]["semantic_family_id"], "F240")

    def test_candidate_identity_is_not_exposed_through_driver_stdout(self) -> None:
        original = V2._diagnostic_for_v1
        try:
            V2._diagnostic_for_v1 = lambda *args, **kwargs: subprocess.CompletedProcess(
                ["compiler"], 1, stdout=b"--formed-presentation=repair_units_hybrid_v2\n", stderr=b"error\n"
            )
            completed = V2.diagnostic_for_v2()
        finally:
            V2._diagnostic_for_v1 = original
        self.assertEqual(completed.stdout, b"")
        self.assertEqual(completed.stderr, b"error\n")

    def test_build_recorder_captures_new_header_status_and_patch(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Test"], cwd=root, check=True)
            (root / "seed.c").write_text("int main(void) { return 0; }\n")
            subprocess.run(["git", "add", "seed.c"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-q", "-m", "seed"], cwd=root, check=True)
            include = root / "include"
            include.mkdir()
            (include / "sealed_token.h").write_text("#define SEALED_TOKEN 1\n")
            script = root / "build.sh"
            script.write_text(V2.base.BUILD_SH.replace("__BUILD_COMMAND__", "true"))
            script.chmod(0o755)
            subprocess.run([str(script)], cwd=root, check=True)
            status = (root / ".trial" / "status-1.txt").read_text()
            patch = (root / ".trial" / "patch-1.diff").read_text()
            self.assertIn("include/sealed_token.h", status)
            self.assertIn("include/sealed_token.h", patch)
            self.assertIn("SEALED_TOKEN", patch)


if __name__ == "__main__":
    unittest.main()
