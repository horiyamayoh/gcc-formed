import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
GATE_SUMMARY = REPO_ROOT / "ci" / "gate_summary.py"


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


class GateSummaryTest(unittest.TestCase):
    def make_status_payload(
        self,
        *,
        step_id: str,
        order: int,
        name: str,
        failure_classification: str,
        status: str,
        exit_code: int | None,
    ) -> dict:
        return {
            "schema_version": 2,
            "workflow": "test-gate",
            "job": "test-job",
            "step": {
                "id": step_id,
                "name": name,
                "order": order,
                "policy": "always",
                "failure_classification": failure_classification,
            },
            "status": status,
            "command": f"echo {step_id}",
            "exit_code": exit_code,
            "fixture": None,
            "gcc_version": "host",
            "support_tier": "repository_gate",
            "artifact_paths": [],
            "log_paths": {"stdout": None, "stderr": None},
            "started_at": "2026-04-09T00:00:00Z",
            "finished_at": "2026-04-09T00:00:01Z",
            "duration_ms": 1000,
            "matrix": {
                "gcc_version": None,
                "support_tier": None,
                "release_blocker": "true",
            },
        }

    def run_gate_summary(self, plan_path: Path, report_root: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["python3", str(GATE_SUMMARY), "--plan", str(plan_path), "--report-root", str(report_root)],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )

    def test_gate_summary_embeds_build_environment_and_product_classification(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            report_root = root / "reports"
            plan_path = root / "plan.json"
            status_dir = report_root / "gate" / "status"
            write_json(
                plan_path,
                {
                    "schema_version": 1,
                    "workflow": "test-gate",
                    "job": "test-job",
                    "steps": [
                        {
                            "id": "capture-host-build-environment",
                            "order": 1,
                            "name": "Capture host build environment",
                            "policy": "always",
                            "failure_classification": "infrastructure",
                        },
                        {
                            "id": "cargo-xtask-check",
                            "order": 2,
                            "name": "cargo xtask check",
                            "policy": "always",
                            "failure_classification": "product",
                        },
                    ],
                },
            )
            write_json(
                status_dir / "01-capture-host-build-environment.json",
                self.make_status_payload(
                    step_id="capture-host-build-environment",
                    order=1,
                    name="Capture host build environment",
                    failure_classification="infrastructure",
                    status="success",
                    exit_code=0,
                ),
            )
            write_json(
                status_dir / "02-cargo-xtask-check.json",
                self.make_status_payload(
                    step_id="cargo-xtask-check",
                    order=2,
                    name="cargo xtask check",
                    failure_classification="product",
                    status="failure",
                    exit_code=101,
                ),
            )
            write_json(
                report_root / "gate" / "build-environment.json",
                {
                    "schema_version": 1,
                    "updated_at": "2026-04-09T00:00:00Z",
                    "host": {
                        "rustc": {"release": "1.94.1"},
                        "cargo": {"release": "1.94.1"},
                        "docker": {"version": "Docker version 28.0.0"},
                    },
                    "ci_image": None,
                },
            )

            completed = self.run_gate_summary(plan_path, report_root)
            self.assertEqual(completed.returncode, 0, completed.stderr)

            summary = json.loads(
                (report_root / "gate" / "gate-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(summary["overall_status"], "failure")
            self.assertEqual(summary["overall_failure_classification"], "product")
            self.assertEqual(summary["failure_classification_counts"]["product"], 1)
            self.assertEqual(summary["failure_classification_counts"]["instrumentation"], 0)
            self.assertEqual(summary["build_environment"]["host"]["rustc"]["release"], "1.94.1")

    def test_gate_summary_flags_missing_build_environment_as_instrumentation(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            report_root = root / "reports"
            plan_path = root / "plan.json"
            status_dir = report_root / "gate" / "status"
            write_json(
                plan_path,
                {
                    "schema_version": 1,
                    "workflow": "test-gate",
                    "job": "test-job",
                    "steps": [
                        {
                            "id": "capture-host-build-environment",
                            "order": 1,
                            "name": "Capture host build environment",
                            "policy": "always",
                            "failure_classification": "infrastructure",
                        }
                    ],
                },
            )
            write_json(
                status_dir / "01-capture-host-build-environment.json",
                self.make_status_payload(
                    step_id="capture-host-build-environment",
                    order=1,
                    name="Capture host build environment",
                    failure_classification="infrastructure",
                    status="success",
                    exit_code=0,
                ),
            )

            completed = self.run_gate_summary(plan_path, report_root)
            self.assertEqual(completed.returncode, 1)

            summary = json.loads(
                (report_root / "gate" / "gate-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(summary["overall_status"], "failure")
            self.assertEqual(summary["overall_failure_classification"], "instrumentation")
            self.assertEqual(summary["failure_classification_counts"]["instrumentation"], 1)
            self.assertTrue(
                any("build environment artifact missing" in anomaly for anomaly in summary["anomalies"])
            )


if __name__ == "__main__":
    unittest.main()
