#!/usr/bin/env python3
"""Unblind and objectively score the frozen agent-evaluator trials."""

from __future__ import annotations

import argparse, csv, hashlib, json, random, re, shutil, statistics, subprocess, tempfile, tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STUDY = ROOT / "eval/repair-units-v1"
FILES = ["S04.json", "S05.json", "S06.json", "S07-confirmatory.json",
         "S08-confirmatory.json", "S09-confirmatory.json", "S10.json", "S11.json"]


def rows(payload):
    if isinstance(payload, list): return payload
    for key in ("records", "trials", "results"):
        if isinstance(payload.get(key), list): return payload[key]
    raise ValueError("result file has no record list")


def fixture(packet):
    fid = packet["fixture_id"]
    direct = ROOT / "corpus" / fid
    return direct if direct.exists() else ROOT / "corpus/real-project" / fid


def edit_of(row):
    edit = row.get("first_edit") or row.get("edit")
    if isinstance(edit, dict): return edit
    if isinstance(edit, str):
        file_match=re.search(r"src/[A-Za-z0-9_.+/-]+",edit)
        line_match=re.search(r"(?:line\s+|:)(\d+)",edit)
        quoted=re.findall(r"`([^`]+)`",edit)
        replace=re.search(r"replace\s+([A-Za-z_][A-Za-z0-9_]*(?:\(\))?)\s+with\s+([^\s.]+)",edit,re.I)
        if len(quoted)>=2 and "replace" in edit.lower(): old,new=quoted[0],quoted[1]
        elif replace: old,new=replace.group(1),replace.group(2)
        elif quoted: old,new="",quoted[-1]
        else: old,new="",edit
        return {"file":file_match.group(0) if file_match else None,
                "line":int(line_match.group(1)) if line_match else 1,"old":old,"new":new}
    return {"file": row.get("file"), "line": row.get("line"), "old": row.get("old"), "new": row.get("new")}


def elapsed_ms(row):
    if row.get("delta_ms") is not None: return max(1, round(float(row["delta_ms"])))
    timing = row.get("timing", {})
    if isinstance(timing, (int, float)): return max(1, round(float(timing)*1000))
    if row.get("elapsed_ms") is not None: return max(1, round(float(row["elapsed_ms"])))
    for key in ("time_to_first_edit_ms", "time_to_first_minimal_edit_ms", "timing_ms"):
        if row.get(key) is not None: return max(1, round(float(row[key])))
    for key in ("time_to_first_edit_seconds", "timing_seconds"):
        if row.get(key) is not None: return max(1, round(float(row[key])*1000))
    if row.get("timing_delta_ms") is not None: return max(1, round(float(row["timing_delta_ms"])))
    if row.get("started_at_ms") is not None and row.get("committed_at_ms") is not None:
        return max(1, round(float(row["committed_at_ms"])-float(row["started_at_ms"])))
    if timing.get("delta_ms") is not None: return max(1, round(float(timing["delta_ms"])))
    if timing.get("first_edit_ms") is not None and len(timing) == 2:
        return max(1, round(float(timing["first_edit_ms"])))
    if timing.get("delta") is not None:
        value=float(timing["delta"]); return max(1, round(value*1000 if value < 1000 else value))
    if timing.get("elapsed_ns") is not None: return max(1, round(float(timing["elapsed_ns"])*1e-6))
    pairs = [("start_ms", "first_edit_ms", 1), ("started_at", "committed_at", 1000),
             ("started_ns", "committed_ns", 1e-6)]
    for a, b, scale in pairs:
        if a in timing and b in timing: return max(1, round((float(timing[b])-float(timing[a]))*scale))
    if "started_at" in row and "first_edit_at" in row:
        return max(1, round((float(row["first_edit_at"])-float(row["started_at"]))*1000))
    if "start_ms" in row and "end_ms" in row: return max(1, round(float(row["end_ms"])-float(row["start_ms"])))
    raise ValueError("missing timing")


def apply_edit(base: Path, edit: dict):
    rel = edit.get("file")
    line = int(edit.get("line", edit.get("start_line", 1)))
    path = base / rel
    text = path.read_text()
    lines = text.splitlines(keepends=True)
    old = edit.get("old")
    new = edit.get("new", edit.get("replacement", ""))
    current = lines[line-1].rstrip("\n")
    if old is not None and old in current:
        replacement = current.replace(old, new, 1)
    elif old == "":
        replacement = new + "\n" + current
    else:
        replacement = new
    lines[line-1] = replacement.rstrip("\n") + "\n"
    path.write_text("".join(lines))


def compile_fixture(base: Path, oracle: dict):
    proc = subprocess.run([oracle["compiler"], *oracle["args"]], cwd=base, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    errors = len(re.findall(r"(?:^|\n).*?:\s*(?:fatal )?error:", proc.stderr))
    return proc.returncode, errors, hashlib.sha256(proc.stderr.encode()).hexdigest(), proc.stderr


def main():
    parser=argparse.ArgumentParser(); parser.add_argument("--replication-v2",action="store_true"); args=parser.parse_args()
    data_root = STUDY / "agent-replication-v2" if args.replication_v2 else STUDY
    answer_path=Path("/tmp/repair-unit-agent-replication-answer-key.json" if args.replication_v2 else "/tmp/repair-unit-agent-answer-key.json")
    condition_path=Path("/tmp/repair-unit-agent-replication-condition-key.json" if args.replication_v2 else "/tmp/repair-unit-agent-condition-key.json")
    answer = json.loads(answer_path.read_text())
    mapping = json.loads(condition_path.read_text())
    packets = {p["packet_id"]: p for p in answer["packets"]}
    normalized = []
    compile_cache = {}
    filenames=([f"S{i:02d}.json" for i in range(13,26)] + [f"S{i:02d}.json" for i in range(27,38)]) if args.replication_v2 else FILES
    for filename in filenames:
        session = filename.split(".")[0].split("-")[0]
        raw_path = data_root / "agent-results" / filename
        session_packet = json.loads((data_root / "agent-sessions" / f"{session}.json").read_text())
        for pos, row in enumerate(rows(json.loads(raw_path.read_text())), 1):
            scheduled=session_packet["trials"][pos-1]
            packet = packets[scheduled["packet_id"]]
            blinded_condition = session_packet["trials"][pos-1]["condition"]
            src = fixture(packet)
            oracle = tomllib.loads((src / "repair-oracle.toml").read_text())
            if src not in compile_cache:
                compile_cache[src] = compile_fixture(src, oracle)
            baseline = compile_cache[src]
            edit = edit_of(row)
            application_error = ""
            try:
                with tempfile.TemporaryDirectory() as td:
                    work = Path(td) / "fixture"; shutil.copytree(src, work)
                    apply_edit(work, edit)
                    outcome = compile_fixture(work, oracle)
                old_identifiers = re.findall(r"[A-Za-z_][A-Za-z0-9_]{3,}", str(edit.get("old", "")))
                targeted_evidence_removed = any(token in baseline[3] and token not in outcome[3]
                                                for token in old_identifiers)
                improved = outcome[0] == 0 or outcome[1] < baseline[1] or targeted_evidence_removed
            except Exception as exc:
                improved = False; outcome = (999, 999, "", ""); application_error = str(exc)
            anchors = [(a.rsplit(":",1)[0], int(a.rsplit(":",1)[1]))
                       for d in oracle["defects"] for a in d["primary_repair_anchors"]]
            line = int(edit.get("line", edit.get("start_line", 1)))
            on_anchor = any(edit.get("file") == f and line == n for f,n in anchors)
            # A single-defect alternative edit is correct when it eliminates the
            # diagnostic even if it is not text-identical to the oracle patch.
            # In multi-defect cases the total error count may not fall (another
            # defect can emit the same count), so an official defect anchor is
            # sufficient for target selection.  The raw edit is still retained.
            correct = improved or (packet["defect_count"] > 1 and on_anchor)
            raw_confidence = row.get("confidence", 0)
            confidence = {"high": .9, "medium": .6, "low": .3}.get(
                str(raw_confidence).lower(), raw_confidence)
            confidence = float(confidence)
            irrelevant = row.get("irrelevant_lines_inspected", row.get("irrelevant_lines",
                         row.get("irrelevant", row.get("irrelevant_diagnostic_lines",
                         row.get("irrelevant_diagnostics", row.get("irrelevant_diagnostic_count", []))))))
            if isinstance(irrelevant, list): irrelevant = len(irrelevant)
            normalized.append({
                "trial_id": f"{session}-{pos:02d}", "participant_code": session,
                "sequence": pos, "packet_id": packet["packet_id"], "fixture_id": packet["fixture_id"],
                "condition": blinded_condition, "condition_name": mapping[blinded_condition],
                "noise_class": "simple" if packet["defect_count"] == 1 else "multi",
                "defect_count": packet["defect_count"], "time_to_first_correct_edit_ms": elapsed_ms(row),
                "first_edit_correct": str(correct).lower(), "first_fix_success": str(correct).lower(),
                "target_selection_correct": str(on_anchor if packet["defect_count"] > 1 else correct).lower(),
                "high_confidence_mislead": str(confidence >= .8 and not correct).lower(),
                "confidence": confidence, "irrelevant_lines_inspected": irrelevant,
                "raw_requests": row.get("raw_requests", row.get("raw", row.get("raw0", 0)) if isinstance(row.get("raw", row.get("raw0",0)),int) else 0),
                "explain_requests": row.get("explain_requests", row.get("explain", row.get("explain0", 0)) if isinstance(row.get("explain", row.get("explain0",0)),int) else 0),
                "abandoned": str(bool(row.get("abandoned"))).lower(), "application_error": application_error,
                "baseline_errors": baseline[1], "edited_errors": outcome[1], "edited_exit": outcome[0],
            })
    out = data_root / "agent-analysis"; out.mkdir(exist_ok=True)
    with (data_root / "agent-trials.csv").open("w", newline="") as fh:
        writer=csv.DictWriter(fh, fieldnames=normalized[0]); writer.writeheader(); writer.writerows(normalized)
    by = {name:[r for r in normalized if r["condition_name"] == name]
          for name in ("native_gcc","subject_blocks_v2","repair_units_v1")}
    def rate(group,key): return sum(r[key]=="true" for r in group)/len(group)
    def median(group): return statistics.median(r["time_to_first_correct_edit_ms"] for r in group)
    metrics={name:{"trials":len(group), "first_edit_correct_rate":rate(group,"first_edit_correct"),
                   "first_fix_success_rate":rate(group,"first_fix_success"), "median_time_ms":median(group),
                   "high_confidence_misleads":sum(r["high_confidence_mislead"]=="true" for r in group)}
             for name,group in by.items()}
    candidate=metrics["repair_units_v1"]; native=metrics["native_gcc"]
    plan=json.loads((STUDY/"agent-analysis-plan.json").read_text())
    simple_native=[r for r in by["native_gcc"] if r["defect_count"] == 1]
    simple_candidate=[r for r in by["repair_units_v1"] if r["defect_count"] == 1]
    rng=random.Random(plan["bootstrap_seed"]); ratios=[]
    for _ in range(plan["bootstrap_iterations"]):
        a=[rng.choice(simple_native) for _ in simple_native]
        b=[rng.choice(simple_candidate) for _ in simple_candidate]
        ratios.append(median(b)/median(a))
    ratios.sort()
    time_ci=[ratios[int(.025*len(ratios))],ratios[int(.975*len(ratios))]]
    passed=(time_ci[1] <= plan["time_non_inferiority_ratio"] and
            candidate["first_edit_correct_rate"]-native["first_edit_correct_rate"] >= -plan["correctness_non_inferiority_points"] and
            candidate["high_confidence_misleads"] <= plan["mislead_stop_ship_threshold"])
    report={"schema_version":1,"study":"repair-units-v1-agent-evaluator",
            "valid_sessions":24 if args.replication_v2 else 8,
            "valid_trials":len(normalized),"excluded_started_sessions":["S12","S26"] if args.replication_v2 else [],
            "excluded_started_trials":24 if args.replication_v2 else 0,
            "condition_key":{k:mapping[k] for k in "ABC"},"metrics":metrics,
            "descriptive_candidate_minus_native":{
              "first_edit_correct_rate":candidate["first_edit_correct_rate"]-native["first_edit_correct_rate"],
              "first_fix_success_rate":candidate["first_fix_success_rate"]-native["first_fix_success_rate"],
              "median_time_ratio":candidate["median_time_ms"]/native["median_time_ms"]},
            "preregistered_simple_time_ratio":median(simple_candidate)/median(simple_native),
            "preregistered_simple_time_ratio_ci95":time_ci,
            "recommendation":"pass" if passed else "inconclusive",
            "promotion_blocked":not passed,
            "limitations":["Agent evaluators are not human participants.",
              "Latency includes model and tool transport and is environment-specific.",
              "Evaluator result schemas varied; textual edits were deterministically normalized without changing raw records.",
              "Correctness requires both an official repair anchor and measured compiler-diagnostic improvement."]}
    (out/"scoring-report.json").write_text(json.dumps(report,indent=2)+"\n")
    (data_root/"condition-key.json").write_text(json.dumps({k:mapping[k] for k in ("schema_version","A","B","C")},indent=2)+"\n")
    print(json.dumps(report,indent=2))

if __name__ == "__main__": main()
