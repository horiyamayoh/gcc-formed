import json
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "ci"))
import run_local_gate  # noqa: E402


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


class LocalGateRunnerTest(unittest.TestCase):
    def test_preflight_reports_unreachable_docker_for_pr_gate(self) -> None:
        with (
            mock.patch.object(run_local_gate, "command_exists", side_effect=lambda binary: True),
            mock.patch.object(
                run_local_gate,
                "docker_daemon_ready",
                return_value=(False, "daemon missing"),
            ),
            mock.patch.object(
                run_local_gate,
                "installed_rust_targets",
                return_value={"x86_64-unknown-linux-musl"},
            ),
        ):
            errors = run_local_gate.preflight_errors(REPO_ROOT, "pr-gate")
        self.assertEqual(errors, ["Docker daemon is not reachable: daemon missing"])

    def test_local_execution_env_defaults_stay_inside_report_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_root = Path(tmpdir) / "nightly" / "lanes" / "gcc15"
            env = run_local_gate.build_execution_env(
                REPO_ROOT,
                report_root,
                "nightly-gate",
                local_mode=True,
                matrix_gcc_version="gcc:15",
                matrix_version_band="gcc15",
                release_blocker="true",
            )
            self.assertEqual(env["REPORT_ROOT"], str(report_root))
            self.assertEqual(env["WORK_ROOT"], str(report_root / "work"))
            self.assertEqual(env["TARGET_DIR"], str(report_root / "work" / "target"))
            self.assertEqual(env["DIST_DIR"], str(report_root / "work" / "dist"))
            self.assertEqual(env["VENDOR_DIR"], str(report_root / "work" / "vendor"))
            self.assertEqual(
                env["RELEASE_REPO_DIR"], str(report_root / "work" / "release-repo")
            )

    def test_run_single_workflow_fail_fast_still_runs_always_steps(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            plan_path = root / "plan.json"
            report_root = root / "reports"
            write_json(
                plan_path,
                {
                    "schema_version": 3,
                    "workflow": "pr-gate",
                    "job": "pr-gate",
                    "steps": [
                        {"id": "step-ok", "order": 1, "policy": "always"},
                        {"id": "step-fail", "order": 2, "policy": "always"},
                        {"id": "step-skip", "order": 3, "policy": "always"},
                        {"id": "step-always", "order": 4, "policy": "always"},
                    ],
                },
            )
            catalog = {
                "pr-gate": {
                    "step-ok": types.SimpleNamespace(
                        run_condition="on_success", requires_step_id=None
                    ),
                    "step-fail": types.SimpleNamespace(
                        run_condition="on_success", requires_step_id=None
                    ),
                    "step-skip": types.SimpleNamespace(
                        run_condition="on_success", requires_step_id=None
                    ),
                    "step-always": types.SimpleNamespace(
                        run_condition="always", requires_step_id=None
                    ),
                }
            }
            invoked_step_ids: list[str] = []

            def fake_subprocess_run(command, cwd, check, env):
                if command[1].endswith("run_gate_step.py"):
                    step_id = command[command.index("--step-id") + 1]
                    invoked_step_ids.append(step_id)
                    return types.SimpleNamespace(returncode=1 if step_id == "step-fail" else 0)
                summary_path = report_root / "gate" / "gate-summary.json"
                write_json(
                    summary_path,
                    {
                        "overall_status": "failure",
                        "overall_failure_classification": "product",
                        "failure_classification_counts": {
                            "product": 1,
                            "infrastructure": 0,
                            "instrumentation": 0,
                        },
                    },
                )
                return types.SimpleNamespace(returncode=1)

            with (
                mock.patch.object(run_local_gate, "EXECUTION_CATALOG", catalog),
                mock.patch.object(run_local_gate, "plan_path_for_workflow", return_value=plan_path),
                mock.patch.object(run_local_gate.subprocess, "run", side_effect=fake_subprocess_run),
            ):
                exit_code, _summary = run_local_gate.run_single_workflow(
                    REPO_ROOT,
                    "pr-gate",
                    report_root,
                )

            self.assertEqual(exit_code, 1)
            self.assertEqual(invoked_step_ids, ["step-ok", "step-fail", "step-always"])

    def test_write_matrix_summary_aggregates_lane_failures(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            report_dir = Path(tmpdir)
            run_local_gate.write_matrix_summary(
                report_dir,
                [
                    {
                        "lane": "gcc12",
                        "gcc_image": "gcc:12",
                        "version_band": "gcc9_12",
                        "release_blocker": "false",
                        "overall_status": "success",
                        "overall_failure_classification": None,
                        "failure_classification_counts": {
                            "product": 0,
                            "infrastructure": 0,
                            "instrumentation": 0,
                        },
                        "report_root": str(report_dir / "lanes" / "gcc12"),
                        "summary_path": str(report_dir / "lanes" / "gcc12" / "gate/gate-summary.json"),
                    },
                    {
                        "lane": "gcc15",
                        "gcc_image": "gcc:15",
                        "version_band": "gcc15",
                        "release_blocker": "true",
                        "overall_status": "failure",
                        "overall_failure_classification": "product",
                        "failure_classification_counts": {
                            "product": 1,
                            "infrastructure": 0,
                            "instrumentation": 0,
                        },
                        "report_root": str(report_dir / "lanes" / "gcc15"),
                        "summary_path": str(report_dir / "lanes" / "gcc15" / "gate/gate-summary.json"),
                    },
                ],
            )
            payload = json.loads((report_dir / "matrix-summary.json").read_text(encoding="utf-8"))
            self.assertEqual(payload["overall_status"], "failure")
            self.assertEqual(payload["failed_lanes"], ["gcc15"])
            self.assertEqual(payload["failure_classification_counts"]["product"], 1)
            markdown = (report_dir / "matrix-summary.md").read_text(encoding="utf-8")
            self.assertIn("gcc12", markdown)
            self.assertIn("gcc15", markdown)


if __name__ == "__main__":
    unittest.main()
