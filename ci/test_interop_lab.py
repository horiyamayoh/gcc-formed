import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
LAB_SCRIPT = REPO_ROOT / "ci" / "interop_lab.py"
LAB_ROOT = REPO_ROOT / "eval" / "interop"


class InteropLabTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("make"), "make is required for the interop lab")
    @unittest.skipUnless(shutil.which("cmake"), "cmake is required for the interop lab")
    def test_interop_lab_runs_make_cmake_and_stdout_cases(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_dir = Path(tmpdir) / "interop-report"
            completed = subprocess.run(
                [
                    "python3",
                    str(LAB_SCRIPT),
                    "--lab-root",
                    str(LAB_ROOT),
                    "--report-dir",
                    str(report_dir),
                ],
                cwd=REPO_ROOT,
                check=False,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            report = json.loads((report_dir / "interop-lab-report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["schema_version"], 1)
            self.assertTrue(report["wrapper_binary"].endswith("gcc-formed"))

            case_names = {case["name"] for case in report["cases"]}
            self.assertEqual(
                case_names,
                {
                    "make-build",
                    "make-build-with-launcher",
                    "make-response-file",
                    "cmake-build",
                    "cmake-build-with-launcher",
                    "cmake-response-file",
                    "stdout-sensitive",
                },
            )

            stress_runs = report["stress_runs"]
            self.assertEqual(len(stress_runs), 12)
            self.assertEqual(
                {run["suite"] for run in stress_runs},
                {
                    "make-stress",
                    "make-launcher-stress",
                    "cmake-stress",
                    "cmake-launcher-stress",
                },
            )

            for suite in {
                "make-stress",
                "make-launcher-stress",
                "cmake-stress",
                "cmake-launcher-stress",
            }:
                runs = [run for run in stress_runs if run["suite"] == suite]
                self.assertEqual({run["round"] for run in runs}, {1, 2, 3})
                self.assertEqual(len({run["runtime_root"] for run in runs}), 3)
                self.assertEqual(len({run["trace_root"] for run in runs}), 3)
                for run in runs:
                    self.assertTrue(run["runtime_cleanup_ok"])
                    self.assertTrue(run["trace_cleanup_ok"])
                    self.assertEqual(run["runtime_entries_after"], [])
                    self.assertEqual(run["trace_entries_after"], [])
                    self.assertGreaterEqual(run["backend_invocations"], 2)
                    command_text = " ".join(run["command"])
                    if suite.startswith("make"):
                        self.assertIn("-j4", command_text)
                    else:
                        self.assertIn("--parallel 4", command_text)
                    if suite in {"make-launcher-stress", "cmake-launcher-stress"}:
                        self.assertGreaterEqual(run["launcher_invocations"], 2)
                        self.assertTrue(run["launcher_received_compiler_path"])
                    else:
                        self.assertEqual(run["launcher_invocations"], 0)
                        self.assertIsNone(run["launcher_received_compiler_path"])
                    self.assertTrue(run["depfiles_present"])

            make_build = next(case for case in report["cases"] if case["name"] == "make-build")
            self.assertTrue(make_build["depfiles_present"])
            self.assertGreaterEqual(make_build["backend_invocations"], 2)
            for artifact in make_build["artifacts"]:
                self.assertTrue(Path(artifact).exists(), artifact)

            make_launcher = next(
                case for case in report["cases"] if case["name"] == "make-build-with-launcher"
            )
            self.assertTrue(make_launcher["depfiles_present"])
            self.assertGreaterEqual(make_launcher["backend_invocations"], 2)
            self.assertGreaterEqual(make_launcher["launcher_invocations"], 2)
            self.assertTrue(make_launcher["launcher_received_compiler_path"])

            make_response = next(
                case for case in report["cases"] if case["name"] == "make-response-file"
            )
            self.assertTrue(make_response["response_file_argument_seen"])
            self.assertTrue(make_response["response_file_payload_seen"])
            self.assertTrue(make_response["depfile_present"])

            cmake_build = next(case for case in report["cases"] if case["name"] == "cmake-build")
            self.assertTrue(cmake_build["depfiles_present"])
            self.assertGreaterEqual(cmake_build["backend_invocations"], 2)
            for artifact in cmake_build["artifacts"]:
                self.assertTrue(Path(artifact).exists(), artifact)

            cmake_launcher = next(
                case for case in report["cases"] if case["name"] == "cmake-build-with-launcher"
            )
            self.assertTrue(cmake_launcher["depfiles_present"])
            self.assertGreaterEqual(cmake_launcher["backend_invocations"], 2)
            self.assertGreaterEqual(cmake_launcher["launcher_invocations"], 2)
            self.assertTrue(cmake_launcher["launcher_received_compiler_path"])

            cmake_response = next(
                case for case in report["cases"] if case["name"] == "cmake-response-file"
            )
            self.assertTrue(cmake_response["response_file_argument_seen"])
            self.assertTrue(cmake_response["response_file_payload_seen"])
            self.assertTrue(cmake_response["depfile_present"])

            stdout_sensitive = next(
                case for case in report["cases"] if case["name"] == "stdout-sensitive"
            )
            self.assertIn("preprocess.c", stdout_sensitive["preprocess_stdout"])
            self.assertIn("install: =/opt/fake-gcc", stdout_sensitive["search_dirs_stdout"])
            self.assertEqual(stdout_sensitive["prog_name_stdout"], "/opt/fake-gcc/libexec/cc1\n")

    def test_operator_docs_require_lab_proven_topology_and_raw_fallback(self) -> None:
        readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
        release_doc = (REPO_ROOT / "docs" / "releases" / "PUBLIC-BETA-RELEASE.md").read_text(
            encoding="utf-8"
        )
        limitations = (REPO_ROOT / "docs" / "support" / "KNOWN-LIMITATIONS.md").read_text(
            encoding="utf-8"
        )
        topology = (REPO_ROOT / "docs" / "support" / "OPERATOR-INTEROP.md").read_text(
            encoding="utf-8"
        )
        lab_readme = (REPO_ROOT / "eval" / "interop" / "README.md").read_text(encoding="utf-8")

        for snippet in [
            "Operator Quickstart for Make / CMake",
            "CC=gcc-formed",
            "CXX=g++-formed",
            "FORMED_BACKEND_LAUNCHER",
            "--formed-mode=passthrough",
            "docs/support/OPERATOR-INTEROP.md",
            "docs/releases/PUBLIC-BETA-RELEASE.md",
        ]:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, readme)

        for snippet in [
            "OPERATOR-INTEROP.md",
            "wrapper-owned backend launcher",
            "FORMED_BACKEND_LAUNCHER",
            "--formed-mode=passthrough",
        ]:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, release_doc)

        for snippet in [
            "single backend launcher",
            "direct `CC` / `CXX` replacement",
            "FORMED_BACKEND_LAUNCHER",
            "ccache / distcc / sccache",
            "Raw fallback",
        ]:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, topology)

        for snippet in [
            "depfile generation",
            "response-file pass-through",
            "stdout-sensitive compiler probes",
            "wrapper-owned backend launcher",
            "make -j2",
            "cmake --build ... --parallel 2",
        ]:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, lab_readme)

        for snippet in ["make -j", "cmake --build", "FORMED_BACKEND_LAUNCHER"]:
            with self.subTest(snippet=snippet):
                self.assertIn(snippet, limitations)


if __name__ == "__main__":
    unittest.main()
