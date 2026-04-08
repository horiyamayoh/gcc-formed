#!/usr/bin/env python3

import argparse
import json
import os
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from string import Template

SCHEMA_VERSION = 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Aggregate CI gate step statuses into JSON and Markdown summaries."
    )
    parser.add_argument("--plan", required=True, help="Path to the workflow step plan JSON.")
    parser.add_argument(
        "--report-root", required=True, help="Workflow report root that owns gate artifacts."
    )
    parser.add_argument(
        "--matrix-gcc-version",
        default=None,
        help="Optional nightly matrix GCC selector such as gcc:13.",
    )
    parser.add_argument(
        "--matrix-support-tier",
        default=None,
        help="Optional nightly matrix support tier such as gcc13_compatibility.",
    )
    parser.add_argument(
        "--release-blocker",
        default="true",
        choices=["true", "false"],
        help="Whether release-blocker-only steps apply to this job.",
    )
    return parser.parse_args()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def load_plan(plan_path: Path) -> dict:
    with plan_path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def ensure_gate_dirs(report_root: Path) -> tuple[Path, Path]:
    gate_root = report_root / "gate"
    status_dir = gate_root / "status"
    gate_root.mkdir(parents=True, exist_ok=True)
    status_dir.mkdir(parents=True, exist_ok=True)
    return gate_root, status_dir


def substitute(value, mapping):
    if value is None:
        return None
    if isinstance(value, str):
        return Template(value).safe_substitute(mapping)
    if isinstance(value, list):
        return [substitute(item, mapping) for item in value]
    if isinstance(value, dict):
        return {key: substitute(item, mapping) for key, item in value.items()}
    return value


def build_mapping(args: argparse.Namespace) -> dict:
    return {
        "REPORT_ROOT": args.report_root,
        "MATRIX_GCC_VERSION": args.matrix_gcc_version or "",
        "MATRIX_SUPPORT_TIER": args.matrix_support_tier or "",
        "RELEASE_BLOCKER": args.release_blocker,
    }


def load_status_files(status_dir: Path) -> tuple[dict[str, dict], list[str]]:
    statuses: dict[str, dict] = {}
    unknown_files: list[str] = []
    for path in sorted(status_dir.glob("*.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        step = payload.get("step", {})
        step_id = step.get("id")
        if not step_id:
            unknown_files.append(str(path))
            continue
        statuses[step_id] = payload
    return statuses, unknown_files


def planned_status(step: dict, mapping: dict, status: str) -> dict:
    return {
        "schema_version": SCHEMA_VERSION,
        "workflow": substitute(step["workflow"], mapping),
        "job": substitute(step["job"], mapping),
        "step": {
            "id": step["id"],
            "name": step["name"],
            "order": step["order"],
            "policy": step.get("policy", "always"),
        },
        "status": status,
        "command": substitute(step.get("command"), mapping),
        "exit_code": None,
        "fixture": substitute(step.get("fixture"), mapping),
        "gcc_version": substitute(step.get("gcc_version"), mapping),
        "support_tier": substitute(step.get("support_tier"), mapping),
        "artifact_paths": substitute(step.get("artifact_paths", []), mapping),
        "log_paths": {"stdout": None, "stderr": None},
        "started_at": None,
        "finished_at": None,
        "duration_ms": None,
        "matrix": {
            "gcc_version": mapping.get("MATRIX_GCC_VERSION") or None,
            "support_tier": mapping.get("MATRIX_SUPPORT_TIER") or None,
            "release_blocker": mapping.get("RELEASE_BLOCKER") or None,
        },
    }


def build_markdown(summary: dict) -> str:
    lines = [
        "# Gate Summary",
        "",
        f"- Workflow: `{summary['workflow']}`",
        f"- Job: `{summary['job']}`",
        f"- Overall status: `{summary['overall_status']}`",
    ]
    first_failed = summary.get("first_failed_step")
    if first_failed is None:
        lines.append("- First failed step: none")
    else:
        lines.append(
            f"- First failed step: `{first_failed['order']:02d} {first_failed['name']}` (`{first_failed['id']}`)"
        )
    counts = summary["status_counts"]
    lines.extend(
        [
            (
                "- Status counts: "
                f"success={counts.get('success', 0)}, "
                f"failure={counts.get('failure', 0)}, "
                f"skipped_prior_failure={counts.get('skipped_prior_failure', 0)}, "
                f"skipped_by_policy={counts.get('skipped_by_policy', 0)}"
            ),
            "",
            "| Order | Step | Status | Exit | GCC | Tier | Fixture |",
            "| --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for step in summary["steps"]:
        step_meta = step["step"]
        fixture = step.get("fixture") or "-"
        exit_code = "-" if step.get("exit_code") is None else str(step["exit_code"])
        gcc_version = step.get("gcc_version") or "-"
        support_tier = step.get("support_tier") or "-"
        lines.append(
            "| "
            f"{step_meta['order']:02d} | "
            f"{step_meta['name']} | "
            f"`{step['status']}` | "
            f"{exit_code} | "
            f"`{gcc_version}` | "
            f"`{support_tier}` | "
            f"`{fixture}` |"
        )

    if summary.get("anomalies"):
        lines.extend(["", "## Anomalies", ""])
        for anomaly in summary["anomalies"]:
            lines.append(f"- {anomaly}")

    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()
    plan = load_plan(Path(args.plan))
    report_root = Path(args.report_root)
    gate_root, status_dir = ensure_gate_dirs(report_root)
    mapping = build_mapping(args)
    statuses_by_id, unknown_files = load_status_files(status_dir)

    plan_steps = []
    for step in plan.get("steps", []):
        step_copy = dict(step)
        step_copy["workflow"] = plan["workflow"]
        step_copy["job"] = plan["job"]
        plan_steps.append(step_copy)
    plan_steps.sort(key=lambda item: int(item["order"]))

    materialized_steps = []
    anomalies = []
    prior_failure_seen = False
    first_failed_step = None

    known_plan_ids = {step["id"] for step in plan_steps}
    for unknown in unknown_files:
        anomalies.append(f"status file missing step.id metadata: {unknown}")

    for step in plan_steps:
        policy = step.get("policy", "always")
        status = statuses_by_id.get(step["id"])
        if status is not None:
            materialized_steps.append(status)
            if status["status"] == "failure":
                prior_failure_seen = True
                if first_failed_step is None:
                    first_failed_step = {
                        "id": step["id"],
                        "name": step["name"],
                        "order": step["order"],
                    }
            continue

        if policy == "release_blocker_only" and args.release_blocker == "false":
            materialized_steps.append(planned_status(step, mapping, "skipped_by_policy"))
            continue

        if prior_failure_seen:
            materialized_steps.append(planned_status(step, mapping, "skipped_prior_failure"))
            continue

        anomalies.append(
            f"planned step `{step['id']}` has no status artifact before any prior failure"
        )
        materialized_steps.append(planned_status(step, mapping, "skipped_prior_failure"))
        if first_failed_step is None:
            first_failed_step = {
                "id": step["id"],
                "name": step["name"],
                "order": step["order"],
            }

    for step_id in sorted(statuses_by_id):
        if step_id not in known_plan_ids:
            anomalies.append(f"status artifact exists for unknown step `{step_id}`")

    status_counts = Counter(step["status"] for step in materialized_steps)
    overall_status = "failure" if status_counts.get("failure", 0) > 0 or anomalies else "success"

    summary = {
        "schema_version": SCHEMA_VERSION,
        "workflow": plan["workflow"],
        "job": plan["job"],
        "generated_at": utc_now(),
        "overall_status": overall_status,
        "first_failed_step": first_failed_step,
        "status_counts": {
            "success": status_counts.get("success", 0),
            "failure": status_counts.get("failure", 0),
            "skipped_prior_failure": status_counts.get("skipped_prior_failure", 0),
            "skipped_by_policy": status_counts.get("skipped_by_policy", 0),
        },
        "steps": materialized_steps,
        "anomalies": anomalies,
        "matrix": {
            "gcc_version": args.matrix_gcc_version,
            "support_tier": args.matrix_support_tier,
            "release_blocker": args.release_blocker,
        },
    }

    summary_json_path = gate_root / "gate-summary.json"
    summary_md_path = gate_root / "gate-summary.md"
    summary_json_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    markdown = build_markdown(summary)
    summary_md_path.write_text(markdown, encoding="utf-8")

    step_summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if step_summary_path:
        with open(step_summary_path, "a", encoding="utf-8") as handle:
            handle.write(markdown)

    return 1 if anomalies else 0


if __name__ == "__main__":
    raise SystemExit(main())
