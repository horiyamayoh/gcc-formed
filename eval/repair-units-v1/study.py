#!/usr/bin/env python3
"""Validate, anonymize, and analyze the preregistered RepairUnit study."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import random
import statistics
import sys
from collections import Counter, defaultdict
from pathlib import Path

CONDITIONS = {"native_gcc", "subject_blocks_v2", "repair_units_v1"}
BOOL = {"true": True, "false": False, "1": True, "0": False}


def load_rows(root: Path) -> list[dict[str, str]]:
    with (root / "trials.csv").open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def load_plan(root: Path) -> dict:
    return json.loads((root / "analysis-plan.json").read_text(encoding="utf-8"))


def condition_key(root: Path) -> dict[str, str] | None:
    path = root / "condition-key.json"
    if not path.exists():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    mapping = {key: payload.get(key) for key in ("A", "B", "C")}
    return mapping if set(mapping.values()) == CONDITIONS else None


def valid_rows(rows: list[dict[str, str]]) -> list[dict[str, str]]:
    return [row for row in rows if not row.get("exclusion_reason", "").strip()]


def validate(root: Path) -> dict:
    plan = load_plan(root)
    rows = load_rows(root)
    valid = valid_rows(rows)
    participants = {row["participant_code"] for row in valid if row["participant_code"]}
    errors: list[str] = []
    with (root / "trials.csv").open(encoding="utf-8") as handle:
        required = set(next(csv.reader(handle)))
    if len(required) != 20:
        errors.append("trial schema drift")
    if len(valid) < plan["minimum_valid_trials"]:
        errors.append(f"valid trials {len(valid)} < {plan['minimum_valid_trials']}")
    if len(participants) < plan["minimum_participants"]:
        errors.append(f"participants {len(participants)} < {plan['minimum_participants']}")
    duplicates = [key for key, count in Counter(row["trial_id"] for row in rows).items() if count > 1]
    if duplicates:
        errors.append(f"duplicate trial ids: {duplicates}")
    for index, row in enumerate(valid, 2):
        if row["experience_confirmed"].lower() not in {"true", "1"}:
            errors.append(f"row {index}: compiler experience not confirmed")
        if row["condition"] not in {"A", "B", "C"}:
            errors.append(f"row {index}: invalid blinded condition")
        for field in ["first_edit_correct", "first_fix_success", "target_selection_correct", "high_confidence_mislead", "abandoned"]:
            if row[field].lower() not in BOOL:
                errors.append(f"row {index}: invalid boolean {field}")
        for field in ["sequence", "defect_count", "time_to_first_correct_edit_ms", "irrelevant_lines_inspected", "raw_requests", "explain_requests"]:
            try:
                if int(row[field]) < 0:
                    raise ValueError
            except ValueError:
                errors.append(f"row {index}: invalid non-negative integer {field}")
    per_participant = defaultdict(set)
    for row in valid:
        per_participant[row["participant_code"]].add(row["condition"])
    incomplete = sorted(code for code, conditions in per_participant.items() if conditions != {"A", "B", "C"})
    if incomplete:
        errors.append(f"participants missing crossover conditions: {incomplete}")
    report = {
        "schema_version": 1,
        "study": "repair-units-v1",
        "trial_count": len(rows),
        "valid_trial_count": len(valid),
        "participant_count": len(participants),
        "counterbalanced": not incomplete and bool(valid),
        "condition_key_revealed": condition_key(root) is not None,
        "errors": errors,
        "verdict": "pass" if not errors else "inconclusive",
    }
    return report


def anonymize(root: Path, output: Path) -> dict:
    rows = load_rows(root)
    output.mkdir(parents=True, exist_ok=True)
    target = output / "anonymized-trials.csv"
    fieldnames = [name for name in rows[0].keys() if name not in {"participant_code", "exclusion_reason"}] if rows else []
    fieldnames.insert(1, "participant_hash") if fieldnames else None
    with target.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            exported = {key: value for key, value in row.items() if key not in {"participant_code", "exclusion_reason"}}
            exported["participant_hash"] = hashlib.sha256(
                f"repair-units-v1:{row['participant_code']}".encode()
            ).hexdigest()[:16]
            writer.writerow(exported)
    report = {"schema_version": 1, "rows": len(rows), "participant_identity_fields": 0, "output": target.name}
    (output / "export-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def rate(rows: list[dict[str, str]], field: str) -> float:
    return sum(BOOL[row[field].lower()] for row in rows) / len(rows) if rows else 0.0


def percentile(values: list[float], probability: float) -> float:
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(probability * len(ordered)))]


def bootstrap(rows_a: list[dict[str, str]], rows_b: list[dict[str, str]], metric, seed: int, iterations: int) -> list[float]:
    rng = random.Random(seed)
    values = []
    for _ in range(iterations):
        a = [rng.choice(rows_a) for _ in rows_a]
        b = [rng.choice(rows_b) for _ in rows_b]
        values.append(metric(a, b))
    return values


def analyze(root: Path, output: Path) -> dict:
    validation = validate(root)
    mapping = condition_key(root)
    if validation["errors"] or mapping is None:
        report = {"schema_version": 1, "study": "repair-units-v1", "recommendation": "inconclusive", "promotion_blocked": True, "reasons": [*validation["errors"], *( [] if mapping else ["condition key is not revealed/valid"])]}
        output.mkdir(parents=True, exist_ok=True)
        (output / "analysis-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        return report
    rows = valid_rows(load_rows(root))
    by_condition = {name: [row for row in rows if mapping[row["condition"]] == name] for name in CONDITIONS}
    native = by_condition["native_gcc"]
    candidate = by_condition["repair_units_v1"]
    simple_native = [row for row in native if row["noise_class"] == "simple" and int(row["defect_count"]) == 1]
    simple_candidate = [row for row in candidate if row["noise_class"] == "simple" and int(row["defect_count"]) == 1]
    multi_native = [row for row in native if int(row["defect_count"]) > 1]
    multi_candidate = [row for row in candidate if int(row["defect_count"]) > 1]
    plan = load_plan(root)
    iterations = plan["bootstrap_iterations"]
    seed = plan["bootstrap_seed"]
    median_time = lambda group: statistics.median(int(row["time_to_first_correct_edit_ms"]) for row in group)
    time_ratio = median_time(simple_candidate) / median_time(simple_native)
    time_boot = bootstrap(simple_native, simple_candidate, lambda a, b: median_time(b) / median_time(a), seed, iterations)
    correctness_delta = rate(candidate, "first_edit_correct") - rate(native, "first_edit_correct")
    correctness_boot = bootstrap(native, candidate, lambda a, b: rate(b, "first_edit_correct") - rate(a, "first_edit_correct"), seed + 1, iterations)
    success_delta = rate(candidate, "first_fix_success") - rate(native, "first_fix_success")
    success_boot = bootstrap(native, candidate, lambda a, b: rate(b, "first_fix_success") - rate(a, "first_fix_success"), seed + 2, iterations)
    target_delta = rate(multi_candidate, "target_selection_correct") - rate(multi_native, "target_selection_correct")
    target_boot = bootstrap(multi_native, multi_candidate, lambda a, b: rate(b, "target_selection_correct") - rate(a, "target_selection_correct"), seed + 3, iterations)
    misleads = sum(BOOL[row["high_confidence_mislead"].lower()] for row in candidate)
    passed = percentile(time_boot, .975) <= plan["time_non_inferiority_ratio"] and percentile(correctness_boot, .025) >= -plan["correctness_non_inferiority_points"] and percentile(success_boot, .025) >= -plan["correctness_non_inferiority_points"] and percentile(target_boot, .025) >= -plan["target_accuracy_non_inferiority_points"] and misleads == 0
    report = {
        "schema_version": 1,
        "study": "repair-units-v1",
        "recommendation": "pass" if passed else "inconclusive",
        "promotion_blocked": not passed,
        "participant_count": validation["participant_count"],
        "valid_trial_count": validation["valid_trial_count"],
        "metrics": {
            "simple_time_ratio": time_ratio,
            "simple_time_ratio_ci95": [percentile(time_boot, .025), percentile(time_boot, .975)],
            "first_edit_correctness_delta": correctness_delta,
            "first_edit_correctness_delta_ci95": [percentile(correctness_boot, .025), percentile(correctness_boot, .975)],
            "first_fix_success_delta": success_delta,
            "first_fix_success_delta_ci95": [percentile(success_boot, .025), percentile(success_boot, .975)],
            "multi_target_accuracy_delta": target_delta,
            "multi_target_accuracy_delta_ci95": [percentile(target_boot, .025), percentile(target_boot, .975)],
            "candidate_raw_request_count": sum(int(row["raw_requests"]) for row in candidate),
            "candidate_explain_request_count": sum(int(row["explain_requests"]) for row in candidate),
            "high_confidence_mislead_count": misleads,
        },
    }
    output.mkdir(parents=True, exist_ok=True)
    (output / "analysis-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["validate", "export", "analyze"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--anonymize", action="store_true")
    args = parser.parse_args()
    if args.command == "validate":
        report = validate(args.root)
    elif args.command == "export":
        if not args.anonymize or args.output is None:
            parser.error("export requires --anonymize and --output")
        report = anonymize(args.root, args.output)
    else:
        if args.output is None:
            parser.error("analyze requires --output")
        report = analyze(args.root, args.output)
    print(json.dumps(report, indent=2))
    return 0 if report.get("verdict", report.get("recommendation", "pass")) == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
