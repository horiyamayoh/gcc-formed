#!/usr/bin/env python3
"""Freeze blinded agent-evaluator packets from reviewed repair-oracle fixtures."""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ROOT = Path(__file__).resolve().parent
WRAPPER = REPO / "target/debug/gcc-formed"

FIXTURES = [
    "corpus/repair-unit-exact-count/single/case-01",
    "corpus/repair-unit-exact-count/single/case-09",
    "corpus/repair-unit-exact-count/single/case-10",
    "corpus/repair-unit-exact-count/single/case-11",
    "corpus/repair-unit-exact-count/single/case-12",
    "corpus/repair-unit-exact-count/double/case-01",
    "corpus/repair-unit-exact-count/double/case-03",
    "corpus/repair-unit-exact-count/double/case-10",
    "corpus/repair-unit-exact-count/triple/case-01",
    "corpus/repair-unit-exact-count/triple/case-03",
    "corpus/real-project/direct-multi-tu-c/case-04",
    "corpus/real-project/cmake-duplicate-symbol-cpp/case-04",
]


def normalize(text: str, temp: Path) -> str:
    text = text.replace(str(temp), "<work>")
    text = re.sub(r"/tmp/(?:cc|tmp)[A-Za-z0-9_.-]+", "<temp>", text)
    text = re.sub(r"0x[0-9a-fA-F]+", "<addr>", text)
    return text


def run(argv: list[str], cwd: Path, env: dict[str, str] | None = None) -> dict:
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, check=False)
    return {
        "exit_code": result.returncode,
        "stdout": normalize(result.stdout, cwd),
        "stderr": normalize(result.stderr, cwd),
    }


def sha(payload: object) -> str:
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def main() -> None:
    if not WRAPPER.is_file():
        raise SystemExit(f"build wrapper first: {WRAPPER}")
    packets_dir = ROOT / "agent-packets"
    sessions_dir = ROOT / "agent-sessions"
    shutil.rmtree(packets_dir, ignore_errors=True)
    shutil.rmtree(sessions_dir, ignore_errors=True)
    packets_dir.mkdir()
    sessions_dir.mkdir()
    manifest = {"schema_version": 1, "condition_key_sha256": None, "packets": []}
    answer_key = {"schema_version": 1, "packets": []}
    public_packets = []
    condition_key = {"A": "native_gcc", "B": "subject_blocks_v2", "C": "repair_units_v1"}
    manifest["condition_key_sha256"] = sha(condition_key)
    for index, fixture_name in enumerate(FIXTURES, 1):
        source = REPO / fixture_name
        spec = tomllib.loads((source / "repair-oracle.toml").read_text(encoding="utf-8"))
        packet_id = f"P{index:02d}"
        with tempfile.TemporaryDirectory() as raw_temp:
            temp = Path(raw_temp) / "fixture"
            shutil.copytree(source, temp, ignore=shutil.ignore_patterns("causal-map.json"))
            compiler = shutil.which(spec["compiler"])
            if compiler is None:
                raise SystemExit(f"missing compiler {spec['compiler']}")
            native = run([compiler, *spec["args"]], temp)
            environment = os.environ.copy()
            environment["FORMED_BACKEND_GCC"] = compiler
            compatibility = run(
                [str(WRAPPER), "--formed-presentation=subject_blocks_v2", *spec["args"]],
                temp,
                environment,
            )
            candidate = run([str(WRAPPER), *spec["args"]], temp, environment)
        sources = {
            str(path.relative_to(source)): path.read_text(encoding="utf-8")
            for path in sorted((source / "src").glob("**/*"))
            if path.is_file()
        }
        repairs = [
            (source / defect["patch"]).read_text(encoding="utf-8")
            for defect in spec["defects"]
        ]
        base = {
            "schema_version": 1,
            "packet_id": packet_id,
            "fixture_id": spec["fixture_id"],
            "language": spec.get("language", "unknown"),
            "shape": spec.get("diagnostic_shape", "unknown"),
            "defect_count": len(spec["defects"]),
            "source_files": sources,
            "instruction": "Record the first minimal source edit you would make. Do not request condition identity. Return the edit before any further analysis.",
        }
        conditions = {"A": native, "B": compatibility, "C": candidate}
        packet_record = {
            **base,
            "expected_repairs": repairs,
            "condition_output_hashes": {key: sha(value) for key, value in conditions.items()},
        }
        packet_record["packet_sha256"] = sha(packet_record)
        manifest["packets"].append({
            "packet_id": packet_id,
            "fixture_id": spec["fixture_id"],
            "packet_sha256": packet_record["packet_sha256"],
            "condition_output_hashes": packet_record["condition_output_hashes"],
        })
        answer_key["packets"].append(packet_record)
        public_packets.append((base, conditions, packet_record["packet_sha256"]))
    (ROOT / "agent-packet-freeze.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    Path("/tmp/repair-unit-agent-answer-key.json").write_text(
        json.dumps(answer_key, indent=2) + "\n", encoding="utf-8"
    )
    for session_index in range(11):
        trials = []
        for packet_index, (base, conditions, packet_sha) in enumerate(public_packets):
            condition = "ABC"[(packet_index + session_index) % 3]
            trial = {**base, "condition": condition, "diagnostic": conditions[condition], "packet_sha256": packet_sha}
            trial["trial_sha256"] = sha(trial)
            trials.append(trial)
        session = {
            "schema_version": 1,
            "session_id": f"S{session_index + 1:02d}",
            "evaluator_type": "isolated_agent",
            "blinded": True,
            "trials": trials,
        }
        (sessions_dir / f"S{session_index + 1:02d}.json").write_text(
            json.dumps(session, indent=2) + "\n", encoding="utf-8"
        )
    Path("/tmp/repair-unit-agent-condition-key.json").write_text(
        json.dumps({"schema_version": 1, **condition_key}, indent=2) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
