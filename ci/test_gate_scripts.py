import json
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
GATE_SUMMARY = REPO_ROOT / "ci" / "gate_summary.py"
GATE_REPLAY_CONTRACT = REPO_ROOT / "ci" / "gate_replay_contract.py"


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
        gate_scope: str | None = "repository",
        version_band: str | None = None,
        legacy_support_tier: str | None = None,
        artifact_paths: list[str] | None = None,
    ) -> dict:
        payload = {
            "schema_version": 3,
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
            "gate_scope": gate_scope,
            "version_band": version_band,
            "artifact_paths": artifact_paths or [],
            "log_paths": {"stdout": None, "stderr": None},
            "started_at": "2026-04-09T00:00:00Z",
            "finished_at": "2026-04-09T00:00:01Z",
            "duration_ms": 1000,
            "matrix": {
                "gcc_version": None,
                "version_band": None,
                "release_blocker": "true",
            },
        }
        if legacy_support_tier is not None:
            payload["support_tier"] = legacy_support_tier
        return payload

    def run_gate_summary(
        self,
        plan_path: Path,
        report_root: Path,
        *,
        matrix_gcc_version: str | None = None,
        matrix_version_band: str | None = None,
        release_blocker: str = "true",
    ) -> subprocess.CompletedProcess:
        command = [
            "python3",
            str(GATE_SUMMARY),
            "--plan",
            str(plan_path),
            "--report-root",
            str(report_root),
            "--release-blocker",
            release_blocker,
        ]
        if matrix_gcc_version is not None:
            command.extend(["--matrix-gcc-version", matrix_gcc_version])
        if matrix_version_band is not None:
            command.extend(["--matrix-version-band", matrix_version_band])
        return subprocess.run(
            command,
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
            self.assertEqual(summary["steps"][1]["gate_scope"], "repository")
            self.assertIsNone(summary["steps"][1]["version_band"])

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

    def test_gate_summary_normalizes_legacy_support_tier_fields(self) -> None:
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
                            "id": "legacy-step",
                            "order": 1,
                            "name": "Legacy step",
                            "policy": "always",
                            "failure_classification": "product",
                            "support_tier": "gcc15_primary",
                        }
                    ],
                },
            )
            write_json(
                status_dir / "01-legacy-step.json",
                self.make_status_payload(
                    step_id="legacy-step",
                    order=1,
                    name="Legacy step",
                    failure_classification="product",
                    status="success",
                    exit_code=0,
                    gate_scope=None,
                    version_band=None,
                    legacy_support_tier="gcc15_primary",
                ),
            )

            completed = self.run_gate_summary(plan_path, report_root)
            self.assertEqual(completed.returncode, 0, completed.stderr)

            summary = json.loads(
                (report_root / "gate" / "gate-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(summary["steps"][0]["gate_scope"], "reference_path")
            self.assertEqual(summary["steps"][0]["version_band"], "gcc15_plus")

    def test_gate_summary_surfaces_machine_readable_path_aware_blockers(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            report_root = root / "reports"
            plan_path = root / "plan.json"
            status_dir = report_root / "gate" / "status"
            stop_ship_path = report_root / "gate" / "replay-stop-ship.json"
            write_json(
                plan_path,
                {
                    "schema_version": 1,
                    "workflow": "test-gate",
                    "job": "test-job",
                    "steps": [
                        {
                            "id": "path-aware-replay-stop-ship",
                            "order": 1,
                            "name": "Replay stop-ship contract",
                            "policy": "always",
                            "failure_classification": "product",
                        }
                    ],
                },
            )
            write_json(
                status_dir / "01-path-aware-replay-stop-ship.json",
                self.make_status_payload(
                    step_id="path-aware-replay-stop-ship",
                    order=1,
                    name="Replay stop-ship contract",
                    failure_classification="product",
                    status="failure",
                    exit_code=1,
                    artifact_paths=[str(stop_ship_path)],
                ),
            )
            write_json(
                stop_ship_path,
                {
                    "schema_version": 1,
                    "status": "fail",
                    "replay_report_path": str(report_root / "replay" / "replay-report.json"),
                    "blocker_counts": {
                        "total": 2,
                        "by_category": {"matrix_hole": 1, "quality_regression": 1},
                        "by_concern": {
                            "coverage.band_path_surface": 1,
                            "semantic.family": 1,
                        },
                    },
                    "blockers": [
                        {
                            "category": "matrix_hole",
                            "concern": "coverage.band_path_surface",
                            "layer": "coverage.band_path_surface",
                            "summary": "missing required coverage cell `gcc13_14/native_text_capture/ci`",
                            "fixture_id": None,
                            "support_band": "gcc13_14",
                            "processing_path": "native_text_capture",
                            "surface": "ci",
                            "matrix_cell": "gcc13_14/native_text_capture/ci",
                        },
                        {
                            "category": "quality_regression",
                            "concern": "semantic.family",
                            "layer": "semantic.family",
                            "summary": "expected `syntax`, got `linker`",
                            "fixture_id": "c/syntax/case-09",
                            "support_band": "gcc9_12",
                            "processing_path": "native_text_capture",
                            "surface": None,
                        },
                    ],
                },
            )

            completed = self.run_gate_summary(plan_path, report_root)
            self.assertEqual(completed.returncode, 0, completed.stderr)

            summary = json.loads(
                (report_root / "gate" / "gate-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(summary["machine_readable_blocker_counts"]["total"], 2)
            self.assertEqual(
                summary["machine_readable_blocker_counts"]["by_category"]["matrix_hole"], 1
            )
            self.assertEqual(
                summary["machine_readable_blockers"][0]["processing_path"],
                "native_text_capture",
            )
            self.assertEqual(summary["machine_readable_blockers"][0]["surface"], "ci")

    def test_gate_summary_skips_reference_path_only_steps_outside_reference_band(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            report_root = root / "reports"
            plan_path = root / "plan.json"
            write_json(
                plan_path,
                {
                    "schema_version": 1,
                    "workflow": "test-gate",
                    "job": "test-job",
                    "steps": [
                        {
                            "id": "release-packaging-smoke",
                            "order": 1,
                            "name": "Release packaging smoke",
                            "policy": "reference_path_only",
                            "failure_classification": "product",
                            "gate_scope": "repository",
                            "command": "echo package",
                        }
                    ],
                },
            )

            completed = self.run_gate_summary(
                plan_path,
                report_root,
                matrix_gcc_version="gcc:13",
                matrix_version_band="gcc13_14",
                release_blocker="false",
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            summary = json.loads(
                (report_root / "gate" / "gate-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(summary["steps"][0]["status"], "skipped_by_policy")
            self.assertEqual(summary["steps"][0]["step"]["policy"], "reference_path_only")
            self.assertEqual(summary["steps"][0]["matrix"]["version_band"], "gcc13_14")


class ReplayContractTest(unittest.TestCase):
    def run_replay_contract(
        self, replay_report: Path, output_path: Path
    ) -> subprocess.CompletedProcess:
        return subprocess.run(
            [
                "python3",
                str(GATE_REPLAY_CONTRACT),
                "--replay-report",
                str(replay_report),
                "--output",
                str(output_path),
            ],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )

    def test_replay_contract_classifies_matrix_and_surface_blockers(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            replay_report = root / "replay-report.json"
            output_path = root / "replay-stop-ship.json"
            write_json(
                replay_report,
                {
                    "coverage": {
                        "missing_required_band_path_surfaces": [
                            "gcc13_14/native_text_capture/ci"
                        ],
                        "missing_required_band_paths": [],
                    },
                    "fixtures": [
                        {
                            "fixture_id": "c/syntax/case-09",
                            "support_band": "gcc9_12",
                            "processing_path": "native_text_capture",
                        }
                    ],
                    "failures": [
                        {
                            "fixture_id": "c/syntax/case-09",
                            "layer": "semantic.family",
                            "summary": "expected `syntax`, got `linker`",
                        },
                        {
                            "fixture_id": "c/syntax/case-09",
                            "layer": "render.ci.line_budget",
                            "summary": "rendered 20 lines, budget is 14",
                        },
                    ],
                    "native_parity": {
                        "failing_fixtures": [
                            {
                                "fixture_id": "c/syntax/case-09",
                                "dimension": "line_budget",
                                "layer": "render.ci.line_budget",
                                "summary": "rendered 20 lines, budget is 14",
                            }
                        ]
                    },
                },
            )

            completed = self.run_replay_contract(replay_report, output_path)
            self.assertEqual(completed.returncode, 1, completed.stderr)

            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["status"], "fail")
            self.assertEqual(payload["blocker_counts"]["by_category"]["matrix_hole"], 1)
            self.assertEqual(payload["blocker_counts"]["by_category"]["native_parity"], 1)
            self.assertEqual(
                payload["blocker_counts"]["by_category"]["quality_regression"], 1
            )
            concerns = {blocker["concern"] for blocker in payload["blockers"]}
            self.assertIn("coverage.band_path_surface", concerns)
            self.assertIn("line_budget", concerns)
            self.assertIn("semantic.family", concerns)


class CheckedInPlanTest(unittest.TestCase):
    def load_plan(self, relative_path: str) -> dict:
        return json.loads((REPO_ROOT / relative_path).read_text(encoding="utf-8"))

    def test_checked_in_plans_include_path_aware_replay_stop_ship_step(self) -> None:
        cases = [
            ("ci/plans/pr-gate.json", "representative-acceptance-replay"),
            ("ci/plans/nightly-gate.json", "representative-acceptance-replay"),
            ("ci/plans/rc-gate.json", "cargo-xtask-rc-gate"),
        ]
        for relative_path, prerequisite_id in cases:
            with self.subTest(plan=relative_path):
                plan = self.load_plan(relative_path)
                steps_by_id = {step["id"]: step for step in plan["steps"]}
                self.assertIn("path-aware-replay-stop-ship", steps_by_id)
                stop_ship = steps_by_id["path-aware-replay-stop-ship"]
                self.assertIn("$REPORT_ROOT/gate/replay-stop-ship.json", stop_ship["artifact_paths"])
                self.assertGreater(stop_ship["order"], steps_by_id[prerequisite_id]["order"])

    def test_pr_gate_plan_uses_reference_path_metadata_for_gcc15_plus_slice(self) -> None:
        plan = self.load_plan("ci/plans/pr-gate.json")
        steps_by_id = {step["id"]: step for step in plan["steps"]}
        for step_id in [
            "build-reference-ci-image",
            "capture-reference-ci-environment",
            "representative-acceptance-replay",
            "path-aware-replay-stop-ship",
            "build-wrapper-binary-reference-image",
            "wrapper-self-check-reference-image",
            "representative-reference-snapshot-check",
        ]:
            with self.subTest(step_id=step_id):
                step = steps_by_id[step_id]
                self.assertEqual(step["gate_scope"], "reference_path")
                self.assertEqual(step["version_band"], "gcc15_plus")
                self.assertNotIn("support_tier", step)
        for step_id in [
            "build-reference-ci-image",
            "capture-reference-ci-environment",
            "build-wrapper-binary-reference-image",
            "wrapper-self-check-reference-image",
            "representative-reference-snapshot-check",
        ]:
            with self.subTest(step_id=step_id):
                self.assertIn("reference-path", steps_by_id[step_id]["name"])
        self.assertNotIn("build-gcc15-ci-image", steps_by_id)
        self.assertNotIn("capture-gcc15-ci-environment", steps_by_id)
        self.assertNotIn("build-wrapper-binary-gcc15-image", steps_by_id)
        self.assertNotIn("wrapper-self-check-gcc15-image", steps_by_id)
        self.assertNotIn("representative-gcc15-snapshot-check", steps_by_id)

    def test_checked_in_plans_use_gate_scope_and_drop_legacy_support_tier(self) -> None:
        for relative_path in [
            "ci/plans/pr-gate.json",
            "ci/plans/nightly-gate.json",
            "ci/plans/rc-gate.json",
        ]:
            with self.subTest(plan=relative_path):
                plan = self.load_plan(relative_path)
                self.assertEqual(plan["schema_version"], 3)
                for step in plan["steps"]:
                    self.assertIn("gate_scope", step)
                    self.assertNotIn("support_tier", step)

    def test_nightly_plan_marks_matrix_blocker_steps_with_matrix_metadata(self) -> None:
        plan = self.load_plan("ci/plans/nightly-gate.json")
        steps_by_id = {step["id"]: step for step in plan["steps"]}
        for step_id in [
            "representative-acceptance-replay",
            "path-aware-replay-stop-ship",
            "wrapper-self-check-matrix-image",
            "representative-matrix-snapshot-check",
        ]:
            with self.subTest(step_id=step_id):
                step = steps_by_id[step_id]
                self.assertEqual(step["policy"], "always")
                self.assertEqual(step["gcc_version"], "${MATRIX_GCC_VERSION}")
                self.assertEqual(step["gate_scope"], "matrix")
                self.assertEqual(step["version_band"], "${MATRIX_VERSION_BAND}")

        snapshot_step = steps_by_id["representative-matrix-snapshot-check"]
        self.assertIn(
            '--version-band "$MATRIX_VERSION_BAND"',
            snapshot_step["command"],
        )

        for step_id in [
            "cargo-xtask-fuzz-smoke",
            "vendor-dependency-tree",
            "hermetic-release-build-smoke",
            "release-packaging-smoke",
            "release-install-smoke",
            "rollback-symlink-smoke",
            "system-wide-layout-smoke",
            "release-repository-promote-and-pin-smoke",
            "dependency-and-license-gate",
        ]:
            with self.subTest(step_id=step_id):
                self.assertEqual(steps_by_id[step_id]["policy"], "reference_path_only")


class CheckedInWorkflowTest(unittest.TestCase):
    def extract_gate_step_ids(self, workflow_relative_path: str) -> list[str]:
        workflow = (REPO_ROOT / workflow_relative_path).read_text(encoding="utf-8")
        return re.findall(r"--step-id ([a-z0-9-]+)", workflow)

    def extract_step_names(self, workflow_relative_path: str) -> list[str]:
        workflow = (REPO_ROOT / workflow_relative_path).read_text(encoding="utf-8")
        return re.findall(r"- name: (.+)", workflow)

    def test_pr_workflow_step_ids_match_checked_in_plan_order(self) -> None:
        workflow_step_ids = self.extract_gate_step_ids(".github/workflows/pr.yml")
        plan = json.loads((REPO_ROOT / "ci/plans/pr-gate.json").read_text(encoding="utf-8"))
        plan_step_ids = [step["id"] for step in plan["steps"]]
        self.assertEqual(workflow_step_ids, plan_step_ids)

    def test_pr_workflow_uses_reference_path_naming_instead_of_gcc15_labels(self) -> None:
        workflow = (REPO_ROOT / ".github" / "workflows" / "pr.yml").read_text(encoding="utf-8")
        self.assertIn("Build GCC 15 reference-path CI image", workflow)
        self.assertIn("Capture GCC 15 reference-path CI environment", workflow)
        self.assertIn("Build wrapper binary in reference-path image", workflow)
        self.assertIn("Wrapper self-check in reference-path image", workflow)
        self.assertIn("Representative reference-path snapshot check", workflow)
        self.assertIn("--step-id build-reference-ci-image", workflow)
        self.assertIn("--step-id capture-reference-ci-environment", workflow)
        self.assertIn("--step-id build-wrapper-binary-reference-image", workflow)
        self.assertIn("--step-id wrapper-self-check-reference-image", workflow)
        self.assertIn("--step-id representative-reference-snapshot-check", workflow)
        self.assertNotIn("Build GCC 15 CI image", workflow)
        self.assertNotIn("Capture GCC 15 CI environment", workflow)
        self.assertNotIn("Representative GCC 15 snapshot check", workflow)
        self.assertNotIn("--step-id build-gcc15-ci-image", workflow)
        self.assertNotIn("--step-id capture-gcc15-ci-environment", workflow)
        self.assertNotIn("--step-id representative-gcc15-snapshot-check", workflow)

    def test_nightly_workflow_uses_matrix_version_band_metadata(self) -> None:
        workflow = (
            REPO_ROOT / ".github" / "workflows" / "nightly.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("MATRIX_VERSION_BAND", workflow)
        self.assertIn("--matrix-version-band", workflow)
        self.assertNotIn("MATRIX_SUPPORT_TIER", workflow)
        self.assertNotIn("--matrix-support-tier", workflow)

    def test_nightly_workflow_includes_gcc9_12_matrix_lane(self) -> None:
        workflow = (
            REPO_ROOT / ".github" / "workflows" / "nightly.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("gcc_image: gcc:12", workflow)
        self.assertIn("gcc_label: gcc12", workflow)
        self.assertIn("version_band: gcc9_12", workflow)
        self.assertIn("MATRIX_VERSION_BAND: ${{ matrix.version_band }}", workflow)
        self.assertIn("name: nightly-${{ matrix.gcc_label }}-artifacts", workflow)

    def test_nightly_workflow_uses_matrix_snapshot_step_without_gcc15_only_markers(self) -> None:
        workflow = (
            REPO_ROOT / ".github" / "workflows" / "nightly.yml"
        ).read_text(encoding="utf-8")
        self.assertNotIn("continue-on-error: ${{ matrix.release_blocker == false }}", workflow)
        self.assertIn("Representative matrix snapshot check", workflow)
        self.assertIn("--step-id representative-matrix-snapshot-check", workflow)
        self.assertIn('--docker-image "$MATRIX_GCC_VERSION"', workflow)
        self.assertNotIn("Representative GCC 15 snapshot check", workflow)
        self.assertNotIn("--step-id representative-gcc15-snapshot-check", workflow)
        snapshot_block = re.search(
            r"- name: Representative matrix snapshot check\n(?P<body>(?:\s{8}.+\n)+)",
            workflow,
        )
        self.assertIsNotNone(snapshot_block)
        self.assertIn('--version-band "$MATRIX_VERSION_BAND"', snapshot_block.group(0))
        self.assertNotIn("if: matrix.release_blocker", snapshot_block.group(0))

    def test_release_beta_workflow_uses_reference_path_snapshot_and_replay_stop_ship(self) -> None:
        workflow = (
            REPO_ROOT / ".github" / "workflows" / "release-beta.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("Representative reference-path snapshot check", workflow)
        self.assertIn("Path-aware replay stop-ship contract", workflow)
        self.assertIn("ci/public_surface.py render-release-body", workflow)
        self.assertIn("ci/gate_replay_contract.py", workflow)
        self.assertIn('--replay-report "$REPORT_ROOT/replay/replay-report.json"', workflow)
        self.assertIn('--output "$REPORT_ROOT/release/replay-stop-ship.json"', workflow)
        self.assertIn("--maturity-label", workflow)
        self.assertNotIn("--support-tier", workflow)
        self.assertIn("replay-stop-ship.json", workflow)
        self.assertNotIn('cat > "$RELEASE_NOTES_PATH" <<EOF', workflow)
        self.assertNotIn("Representative GCC 15 snapshot check", workflow)

    def test_release_beta_workflow_orders_release_provenance_after_assets(self) -> None:
        step_names = self.extract_step_names(".github/workflows/release-beta.yml")
        self.assertLess(step_names.index("Representative acceptance replay"), step_names.index("Path-aware replay stop-ship contract"))
        self.assertLess(step_names.index("Path-aware replay stop-ship contract"), step_names.index("Representative reference-path snapshot check"))
        self.assertLess(step_names.index("Assemble GitHub Release bundles"), step_names.index("Write release provenance"))
        self.assertLess(step_names.index("Write release provenance"), step_names.index("Write release notes"))
        self.assertLess(step_names.index("Write release notes"), step_names.index("Publish GitHub prerelease"))

    def test_release_stable_workflow_surfaces_release_gate_evidence_in_provenance(self) -> None:
        workflow = (
            REPO_ROOT / ".github" / "workflows" / "release-stable.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("Path-aware replay stop-ship contract", workflow)
        self.assertIn("ci/public_surface.py render-release-body", workflow)
        self.assertIn('--replay-report "$REPORT_ROOT/rc-gate/replay-report.json"', workflow)
        self.assertIn('--output "$REPORT_ROOT/rc-gate/replay-stop-ship.json"', workflow)
        self.assertIn("--maturity-label", workflow)
        self.assertNotIn("--support-tier", workflow)
        self.assertIn("rollout-matrix-report.json", workflow)
        self.assertIn("replay-stop-ship.json", workflow)
        self.assertIn("release-provenance.json", workflow)
        self.assertNotIn('cat > "$RELEASE_NOTES_PATH" <<EOF', workflow)
        self.assertIn("stable-release", workflow)

    def test_release_stable_workflow_orders_release_provenance_after_rc_gate_evidence(self) -> None:
        step_names = self.extract_step_names(".github/workflows/release-stable.yml")
        self.assertLess(step_names.index("Strict RC gate"), step_names.index("Path-aware replay stop-ship contract"))
        self.assertLess(step_names.index("Path-aware replay stop-ship contract"), step_names.index("Assemble GitHub Release bundles"))
        self.assertLess(step_names.index("Assemble GitHub Release bundles"), step_names.index("Write release provenance"))
        self.assertLess(step_names.index("Write release provenance"), step_names.index("Write release notes"))
        self.assertLess(step_names.index("Write release notes"), step_names.index("Publish GitHub release"))


if __name__ == "__main__":
    unittest.main()
