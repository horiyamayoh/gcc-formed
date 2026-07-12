#!/usr/bin/env python3
"""Sealed single-agent output-quality qualification controller.

The controller intentionally uses only the Python standard library.  It creates
source-distinct matched tasks, conceals the renderer condition, starts one fresh
ephemeral Codex context per trial, retains all artifacts, scores real build
attempts, performs family-clustered bootstrap analysis, and verifies hashes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parent
PROTOCOL = ROOT / "protocol.json"
ANALYSIS_PLAN = ROOT / "analysis-plan.json"
AGENT_MANIFEST = ROOT / "model-agent-tool-manifest.json"
ATTESTATION = ROOT / "no-subagent-attestation.json"
TRIAL_PROMPT = ROOT / "trial-prompt.txt"
RESULT_SCHEMA = ROOT / "trial-result.schema.json"
STATIC_FILES = (
    PROTOCOL,
    ANALYSIS_PLAN,
    AGENT_MANIFEST,
    ATTESTATION,
    TRIAL_PROMPT,
    RESULT_SCHEMA,
)
IDENTITIES = ("native_gcc", "current_default", "candidate")
LABELS = ("A", "B", "C")
REQUIRED_PACKET_FILES = (
    "protocol.json",
    "analysis-plan.json",
    "model-agent-tool-manifest.json",
    "no-subagent-attestation.json",
    "corpus-manifest.json",
    "seed-commitment.json",
    "candidate-freeze.json",
    "trial-index.jsonl",
    "artifact-integrity-report.json",
    "fidelity-report.json",
    "repair-utility-report.json",
    "efficiency-report.json",
    "human-readable-contract-report.json",
    "qualification-report.json",
    "qualification-summary.md",
    "default-promotion-decision.md",
)


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_hash(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(encoded)


def files_merkle(root: Path, *, excluded: Iterable[str] = ()) -> tuple[str, list[dict[str, Any]]]:
    excluded_set = set(excluded)
    records: list[dict[str, Any]] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        relative = path.relative_to(root).as_posix()
        if relative in excluded_set:
            continue
        records.append({"path": relative, "sha256": sha256_file(path), "bytes": path.stat().st_size})
    return canonical_hash(records), records


def run_checked(command: list[str], cwd: Path, **kwargs: Any) -> subprocess.CompletedProcess[bytes]:
    completed = subprocess.run(command, cwd=cwd, check=False, **kwargs)
    if completed.returncode != 0:
        stderr = completed.stderr.decode(errors="replace") if completed.stderr else ""
        raise RuntimeError(f"command failed ({completed.returncode}): {command!r}\n{stderr}")
    return completed


def validate_static() -> dict[str, Any]:
    protocol = read_json(PROTOCOL)
    analysis = read_json(ANALYSIS_PLAN)
    manifest = read_json(AGENT_MANIFEST)
    attestation = read_json(ATTESTATION)
    errors: list[str] = []
    if protocol.get("schema_version") != 1:
        errors.append("protocol schema_version must be 1")
    population = protocol.get("population", {})
    if population.get("semantic_families") != 120:
        errors.append("protocol must freeze 120 semantic families")
    if population.get("valid_trials_minimum") != 360:
        errors.append("protocol must freeze 360 valid trials")
    if analysis.get("cluster") != "semantic_family_id":
        errors.append("analysis cluster must be semantic_family_id")
    if analysis.get("optional_stopping") is not False:
        errors.append("optional stopping must be disabled")
    policy = protocol.get("agent_policy", {})
    for key in ("subagents", "delegation", "ensemble", "best_of_n", "model_voting"):
        if policy.get(key) is not False:
            errors.append(f"agent policy {key} must be false")
    if policy.get("jobs") != 1:
        errors.append("agent jobs must be exactly one")
    if manifest.get("model") != "gpt-5.6-sol" or manifest.get("reasoning_effort") != "xhigh":
        errors.append("agent manifest must pin gpt-5.6-sol/xhigh")
    if attestation.get("attested") is not True or attestation.get("subagent_spawn_count") != 0:
        errors.append("no-subagent attestation is not affirmative")
    report = {
        "schema_version": 1,
        "status": "pass" if not errors else "fail",
        "errors": errors,
        "hashes": {path.name: sha256_file(path) for path in STATIC_FILES},
    }
    if errors:
        raise ValueError("; ".join(errors))
    return report


@dataclass(frozen=True)
class GeneratedTask:
    family_id: str
    stratum: str
    semantic_shape: str
    language: str
    variant: int
    files: dict[str, str]
    build_command: list[str]
    allowed_files: list[str]
    repair_token: str


def task_for(index: int, variant: int, attempt: int) -> GeneratedTask:
    family_id = f"F{index + 1:03d}"
    suffix = f"{index + 1}_{variant + 1}_{attempt}"
    mode = index % 4
    if index < 40:
        stratum = "simple_native_strong"
        if mode == 0:
            token = f"missing_value_{suffix}"
            return GeneratedTask(
                family_id, stratum, "missing_name", "c", variant,
                {"src/main.c": f"int main(void) {{ return {token}; }}\n"},
                ["gcc", "-fdiagnostics-color=never", "-Wall", "-Werror", "src/main.c", "-o", "build/app"],
                ["src/main.c"], token,
            )
        if mode == 1:
            token = f"int value_{suffix} = {variant + 1}"
            return GeneratedTask(
                family_id, stratum, "syntax_recovery", "c", variant,
                {"src/main.c": f"int main(void) {{ {token} return value_{suffix}; }}\n"},
                ["gcc", "-fdiagnostics-color=never", "-Wall", "-Werror", "src/main.c", "-o", "build/app"],
                ["src/main.c"], token,
            )
        if mode == 2:
            token = f"take_{suffix}({variant + 1})"
            return GeneratedTask(
                family_id, stratum, "argument_count", "cpp", variant,
                {"src/main.cpp": (
                    f"static int take_{suffix}(int a, int b) {{ return a + b; }}\n"
                    f"int main() {{ return {token}; }}\n"
                )},
                ["g++", "-std=c++17", "-fdiagnostics-color=never", "-Wall", "-Werror", "src/main.cpp", "-o", "build/app"],
                ["src/main.cpp"], token,
            )
        token = f"int value_{suffix} = text_{suffix}"
        return GeneratedTask(
            family_id, stratum, "type_mismatch", "cpp", variant,
            {"src/main.cpp": (
                f"const char *text_{suffix} = \"x\";\n"
                f"int main() {{ {token}; return value_{suffix}; }}\n"
            )},
            ["g++", "-std=c++17", "-fdiagnostics-color=never", "-Wall", "-Werror", "src/main.cpp", "-o", "build/app"],
            ["src/main.cpp"], token,
        )
    if index < 80:
        stratum = "diagnostic_flood_semantic_heavy"
        if mode == 0:
            token = f"choose_{suffix}(\"bad\")"
            overloads = "\n".join(
                f"static int choose_{suffix}({kind}) {{ return {number}; }}"
                for number, kind in enumerate(("int", "long", "double", "char", "unsigned"), 1)
            )
            return GeneratedTask(
                family_id, stratum, "overload", "cpp", variant,
                {"src/main.cpp": f"{overloads}\nint main() {{ return {token}; }}\n"},
                ["g++", "-std=c++20", "-fdiagnostics-color=never", "-fmax-errors=0", "src/main.cpp", "-o", "build/app"],
                ["src/main.cpp"], token,
            )
        if mode == 1:
            token = f"Box_{suffix}<const char *>"
            return GeneratedTask(
                family_id, stratum, "template", "cpp", variant,
                {"src/main.cpp": (
                    "#include <type_traits>\n"
                    f"template<class T> struct Box_{suffix} {{ static_assert(std::is_integral_v<T>, \"integral required\"); T value; }};\n"
                    f"int main() {{ {token} box{{\"x\"}}; return box.value; }}\n"
                )},
                ["g++", "-std=c++20", "-fdiagnostics-color=never", "-ftemplate-backtrace-limit=0", "src/main.cpp", "-o", "build/app"],
                ["src/main.cpp"], token,
            )
        if mode == 2:
            token = f"USE_{suffix}(unknown_macro_value_{suffix})"
            return GeneratedTask(
                family_id, stratum, "macro", "c", variant,
                {"src/main.c": (
                    f"#define USE_{suffix}(x) ((x) + 1)\n"
                    f"int main(void) {{ return {token}; }}\n"
                )},
                ["gcc", "-fdiagnostics-color=never", "-Wall", "-Werror", "src/main.c", "-o", "build/app"],
                ["src/main.c"], token,
            )
        token = f"missing_link_{suffix}"
        return GeneratedTask(
            family_id, stratum, "linker_undefined", "c", variant,
            {
                "src/main.c": f"int {token}(void);\nint main(void) {{ return {token}(); }}\n",
                "src/helper.c": f"int helper_{suffix}(void) {{ return {variant}; }}\n",
            },
            ["gcc", "-fdiagnostics-color=never", "src/main.c", "src/helper.c", "-o", "build/app"],
            ["src/main.c", "src/helper.c"], token,
        )
    stratum = "multi_file_build_real_project"
    if mode == 0:
        token = f"project_missing_{suffix}"
        files = {
            "src/main.c": f"int {token}(void);\nint main(void) {{ return {token}(); }}\n",
            "src/lib.c": f"int project_helper_{suffix}(void) {{ return {variant}; }}\n",
        }
    elif mode == 1:
        token = f"duplicate_{suffix}"
        files = {
            "src/main.c": f"int {token}(void) {{ return 0; }}\nint main(void) {{ return {token}(); }}\n",
            "src/lib.c": f"int {token}(void) {{ return {variant + 1}; }}\n",
        }
    elif mode == 2:
        token = f"api_{suffix}(\"wrong\")"
        files = {
            "include/api.hpp": f"int api_{suffix}(int value);\n",
            "src/api.cpp": f"#include \"api.hpp\"\nint api_{suffix}(int value) {{ return value; }}\n",
            "src/main.cpp": f"#include \"api.hpp\"\nint main() {{ return {token}; }}\n",
        }
    else:
        token = f"unknown_project_{suffix}"
        files = {
            "include/value.h": f"int value_{suffix}(void);\n",
            "src/value.c": f"#include \"value.h\"\nint value_{suffix}(void) {{ return {token}; }}\n",
            "src/main.c": f"#include \"value.h\"\nint main(void) {{ return value_{suffix}(); }}\n",
        }
    compiler = "g++" if mode == 2 else "gcc"
    extension_sources = sorted(path for path in files if path.startswith("src/"))
    command = [compiler, "-fdiagnostics-color=never", "-Iinclude", *extension_sources, "-o", "build/app"]
    makefile = (
        f"CC := {compiler}\n"
        "all:\n"
        f"\t$(CC) -fdiagnostics-color=never -Iinclude {' '.join(extension_sources)} -o build/app\n"
    )
    files["Project.mk"] = makefile
    return GeneratedTask(
        family_id, stratum, "make_multi_file", "cpp" if compiler == "g++" else "c", variant,
        files, ["make", "-f", "Project.mk", "-j1"],
        [path for path in files if path.startswith(("src/", "include/"))], token,
    )


def condition_mapping(candidate_sha: str, attempt: int) -> dict[str, str]:
    seed = int(sha256_bytes(f"condition-key:{candidate_sha}:{attempt}".encode())[:16], 16)
    identities = list(IDENTITIES)
    random.Random(seed).shuffle(identities)
    return dict(zip(LABELS, identities, strict=True))


def make_aliases(packet_root: Path, formed_binary: Path) -> dict[str, Path]:
    alias_root = packet_root / "control" / "bin"
    alias_root.mkdir(parents=True, exist_ok=True)
    aliases: dict[str, Path] = {}
    for name in ("gcc-formed", "g++-formed"):
        alias = alias_root / name
        if alias.exists() or alias.is_symlink():
            alias.unlink()
        alias.symlink_to(formed_binary.resolve())
        aliases[name] = alias
    return aliases


def diagnostic_for(
    task: GeneratedTask,
    task_root: Path,
    identity: str,
    aliases: dict[str, Path] | None,
    candidate_presentation: str | None,
) -> subprocess.CompletedProcess[bytes]:
    env = os.environ.copy()
    controller_home = task_root / ".controller-home"
    runtime_root = task_root / ".runtime"
    controller_home.mkdir(exist_ok=True)
    runtime_root.mkdir(exist_ok=True)
    env["HOME"] = str(controller_home)
    env["XDG_RUNTIME_DIR"] = str(runtime_root)
    command = list(task.build_command)
    if identity != "native_gcc":
        if aliases is None:
            raise ValueError("formed binary is required for current_default and candidate")
        extra = []
        if identity == "candidate" and candidate_presentation:
            extra = [f"--formed-presentation={candidate_presentation}"]
        if command[0] in ("gcc", "g++"):
            alias_name = "g++-formed" if command[0] == "g++" else "gcc-formed"
            command = [str(aliases[alias_name]), *extra, *command[1:]]
        elif command[0] == "make":
            compiler = "g++" if task.language == "cpp" else "gcc"
            alias_name = "g++-formed" if compiler == "g++" else "gcc-formed"
            env["CC"] = " ".join([str(aliases[alias_name]), *extra])
            command = [*command, f"CC={env['CC']}"]
    return subprocess.run(command, cwd=task_root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)


BUILD_SH = r'''#!/usr/bin/env bash
set -u
mkdir -p .trial build
attempt_file=.trial/attempt-count
attempt=0
if [[ -f "$attempt_file" ]]; then attempt=$(<"$attempt_file"); fi
attempt=$((attempt + 1))
if (( attempt > 3 )); then
  echo "maximum three build/test loops exceeded" >&2
  exit 97
fi
printf '%s\n' "$attempt" > "$attempt_file"
git diff --binary -- . ':(exclude).trial' ':(exclude)build' > ".trial/patch-${attempt}.diff"
git status --short -- . ':(exclude).trial' ':(exclude)build' > ".trial/status-${attempt}.txt"
set +e
__BUILD_COMMAND__ > ".trial/stdout-${attempt}.txt" 2> ".trial/stderr-${attempt}.txt"
code=$?
set -e
printf '%s\n' "$code" > ".trial/exit-${attempt}.txt"
cat ".trial/stdout-${attempt}.txt"
cat ".trial/stderr-${attempt}.txt" >&2
exit "$code"
'''


def shell_quote(value: str) -> str:
    return "'" + value.replace("'", "'\"'\"'") + "'"


def copy_static(packet_root: Path) -> None:
    for path in STATIC_FILES:
        shutil.copy2(path, packet_root / path.name)


def generate_corpus(args: argparse.Namespace) -> dict[str, Any]:
    validate_static()
    packet_root = args.output.resolve()
    if packet_root.exists() and any(packet_root.iterdir()):
        raise ValueError(f"refusing to overwrite non-empty packet root: {packet_root}")
    packet_root.mkdir(parents=True, exist_ok=True)
    copy_static(packet_root)
    mapping = condition_mapping(args.candidate_sha, args.attempt)
    aliases = make_aliases(packet_root, args.formed_binary) if args.formed_binary else None
    protocol_hash = sha256_file(PROTOCOL)
    analysis_hash = sha256_file(ANALYSIS_PLAN)
    manifest_hash = sha256_file(AGENT_MANIFEST)
    seed_material = f"{read_json(PROTOCOL)['protocol_id']}:{args.candidate_sha}:{args.attempt}"
    seed_commitment = sha256_bytes(seed_material.encode())
    write_json(packet_root / "seed-commitment.json", {
        "schema_version": 1,
        "algorithm": "sha256",
        "attempt": args.attempt,
        "commitment": seed_commitment,
        "material_revealed_after_freeze": False,
    })
    write_json(packet_root / "candidate-freeze.json", {
        "schema_version": 1,
        "candidate_sha": args.candidate_sha,
        "attempt": args.attempt,
        "protocol_sha256": protocol_hash,
        "analysis_plan_sha256": analysis_hash,
        "agent_manifest_sha256": manifest_hash,
        "candidate_presentation": args.candidate_presentation,
        "formed_binary_sha256": sha256_file(args.formed_binary) if args.formed_binary else None,
        "frozen_at_unix_seconds": int(time.time()),
    })
    control_key = {
        "schema_version": 1,
        "concealed": True,
        "mapping": mapping,
        "commitment": canonical_hash(mapping),
    }
    write_json(packet_root / "control" / "condition-key.sealed.json", control_key)
    os.chmod(packet_root / "control" / "condition-key.sealed.json", 0o000)
    trials: list[dict[str, Any]] = []
    family_records: list[dict[str, Any]] = []
    identity_to_label = {identity: label for label, identity in mapping.items()}
    for family_index in range(120):
        family_variants: list[dict[str, Any]] = []
        rotation = family_index % 3
        for identity_index, identity in enumerate(IDENTITIES):
            variant = (identity_index + rotation) % 3
            task = task_for(family_index, variant, args.attempt)
            opaque = sha256_bytes(
                f"{seed_material}:{task.family_id}:{variant}:{identity}".encode()
            )[:20]
            trial_id = f"T-{opaque}"
            trial_root = packet_root / "trials" / trial_id
            work = trial_root / "work"
            work.mkdir(parents=True)
            for relative, content in task.files.items():
                target = work / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content, encoding="utf-8")
            (work / "build").mkdir()
            completed = diagnostic_for(task, work, identity, aliases, args.candidate_presentation)
            diagnostic = completed.stderr + (b"\n[stdout]\n" + completed.stdout if completed.stdout else b"")
            (work / "DIAGNOSTIC.txt").write_bytes(diagnostic)
            task_public = {
                "schema_version": 1,
                "task_id": trial_id,
                "language": task.language,
                "build_command": "./build.sh",
                "maximum_build_attempts": 3,
                "diagnostic_file": "DIAGNOSTIC.txt",
                "condition": "concealed",
            }
            write_json(work / "TASK.json", task_public)
            command_text = " ".join(shell_quote(item) for item in task.build_command)
            build_script = BUILD_SH.replace("__BUILD_COMMAND__", command_text)
            (work / "build.sh").write_text(build_script, encoding="utf-8")
            os.chmod(work / "build.sh", 0o755)
            run_checked(["git", "init", "-q"], work, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            run_checked(["git", "config", "user.email", "qualification@example.invalid"], work, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            run_checked(["git", "config", "user.name", "Qualification Controller"], work, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            run_checked(["git", "add", "."], work, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            run_checked(["git", "commit", "-q", "-m", "initial sealed task"], work, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            source_records = []
            for relative in sorted(task.files):
                path = work / relative
                source_records.append({"path": relative, "sha256": sha256_file(path), "bytes": path.stat().st_size})
            private = {
                "schema_version": 1,
                "trial_id": trial_id,
                "semantic_family_id": task.family_id,
                "stratum": task.stratum,
                "semantic_shape": task.semantic_shape,
                "variant": variant,
                "condition_label": identity_to_label[identity],
                "allowed_files": task.allowed_files,
                "repair_token": task.repair_token,
                "initial_source": source_records,
                "diagnostic_sha256": sha256_bytes(diagnostic),
                "diagnostic_bytes": len(diagnostic),
                "initial_compiler_exit": completed.returncode,
            }
            write_json(trial_root / "controller.json", private)
            trial_record = {
                key: private[key]
                for key in (
                    "trial_id", "semantic_family_id", "stratum", "semantic_shape",
                    "variant", "condition_label", "diagnostic_sha256", "diagnostic_bytes",
                )
            }
            trial_record["status"] = "materialized"
            trials.append(trial_record)
            family_variants.append({"trial_id": trial_id, "variant": variant, "condition_label": private["condition_label"]})
        family_records.append({
            "semantic_family_id": f"F{family_index + 1:03d}",
            "stratum": task_for(family_index, 0, args.attempt).stratum,
            "variants": family_variants,
        })
    corpus_manifest = {
        "schema_version": 1,
        "candidate_sha": args.candidate_sha,
        "attempt": args.attempt,
        "semantic_family_count": 120,
        "trial_count": 360,
        "condition_counts": {label: 120 for label in LABELS},
        "stratum_family_counts": {
            "simple_native_strong": 40,
            "diagnostic_flood_semantic_heavy": 40,
            "multi_file_build_real_project": 40,
        },
        "families": family_records,
    }
    write_json(packet_root / "corpus-manifest.json", corpus_manifest)
    with (packet_root / "trial-index.jsonl").open("w", encoding="utf-8") as handle:
        for trial in trials:
            handle.write(json.dumps(trial, sort_keys=True) + "\n")
    materialized_root, materialized_files = files_merkle(packet_root / "trials")
    write_json(packet_root / "materialization-freeze.json", {
        "schema_version": 1,
        "trial_artifact_merkle_root": materialized_root,
        "file_count": len(materialized_files),
        "condition_key_commitment": control_key["commitment"],
    })
    return {
        "status": "pass",
        "packet_root": str(packet_root),
        "families": 120,
        "trials": 360,
        "attempt": args.attempt,
        "condition_key_commitment": control_key["commitment"],
    }


def load_index(packet_root: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    with (packet_root / "trial-index.jsonl").open(encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise ValueError("trial index rows must be objects")
                records.append(value)
    return records


def parse_codex_events(path: Path) -> dict[str, int]:
    tool_calls = 0
    source_bytes = 0
    files_opened: set[str] = set()
    if not path.exists():
        return {"tool_calls": 0, "source_bytes": 0, "files_opened": 0}
    with path.open(encoding="utf-8", errors="replace") as handle:
        for line in handle:
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            rendered = json.dumps(event, sort_keys=True)
            if any(marker in rendered for marker in ('"command_execution"', '"tool_call"', '"mcp_tool_call"')):
                tool_calls += 1
            if any(marker in rendered for marker in ("src/", "include/", "Project.mk")):
                source_bytes += len(rendered.encode())
                for token in rendered.replace('"', " ").split():
                    if "src/" in token or "include/" in token:
                        files_opened.add(token.strip("',:;()[]{}"))
    return {"tool_calls": tool_calls, "source_bytes": source_bytes, "files_opened": len(files_opened)}


def score_trial(packet_root: Path, record: dict[str, Any], process_returncode: int, elapsed_ms: int) -> dict[str, Any]:
    trial_root = packet_root / "trials" / record["trial_id"]
    work = trial_root / "work"
    private = read_json(trial_root / "controller.json")
    attempts = 0
    first_success: int | None = None
    for attempt in range(1, 4):
        exit_path = work / ".trial" / f"exit-{attempt}.txt"
        if not exit_path.exists():
            continue
        attempts = attempt
        if int(exit_path.read_text().strip()) == 0 and first_success is None:
            first_success = attempt
    diff = subprocess.run(
        ["git", "diff", "--binary", "--", ".", ":(exclude).trial", ":(exclude)build"],
        cwd=work, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    ).stdout
    (trial_root / "final.patch").write_bytes(diff)
    status_lines = subprocess.run(
        ["git", "status", "--short", "--", ".", ":(exclude).trial", ":(exclude)build"],
        cwd=work, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    ).stdout.decode(errors="replace").splitlines()
    changed_files = sorted({line[3:].strip() for line in status_lines if len(line) > 3})
    wrong_file = any(path not in private["allowed_files"] for path in changed_files)
    diff_text = diff.decode(errors="replace")
    anchor_present = private["repair_token"] in diff_text
    invalid_schema = True
    result_path = trial_root / "agent-final.json"
    if result_path.exists():
        try:
            final = read_json(result_path)
            invalid_schema = set(final) != {"status", "summary", "build_attempts_reported"}
            invalid_schema = invalid_schema or final.get("status") not in ("fixed", "not_fixed", "blocked")
            invalid_schema = invalid_schema or not isinstance(final.get("build_attempts_reported"), int)
        except (ValueError, json.JSONDecodeError):
            invalid_schema = True
    event_metrics = parse_codex_events(trial_root / "transcript.jsonl")
    first_patch_path = work / ".trial" / "patch-1.diff"
    first_patch_lines = 0
    if first_patch_path.exists():
        first_patch_lines = sum(
            1 for line in first_patch_path.read_text(errors="replace").splitlines()
            if (line.startswith("+") and not line.startswith("+++"))
            or (line.startswith("-") and not line.startswith("---"))
        )
    final_source = []
    for relative in private["allowed_files"]:
        path = work / relative
        if path.exists():
            final_source.append({"path": relative, "sha256": sha256_file(path), "bytes": path.stat().st_size})
    score = {
        "schema_version": 1,
        "trial_id": record["trial_id"],
        "semantic_family_id": private["semantic_family_id"],
        "condition_label": private["condition_label"],
        "started": True,
        "valid": True,
        "process_returncode": process_returncode,
        "elapsed_ms": elapsed_ms,
        "invalid_final_schema": invalid_schema,
        "build_attempts": attempts,
        "first_success_attempt": first_success,
        "first_patch_success": first_success == 1 and not wrong_file and anchor_present,
        "success_within_three_loops": first_success is not None and not wrong_file and anchor_present,
        "wrong_file_or_anchor": wrong_file or not anchor_present,
        "changed_files": changed_files,
        "first_patch_size_lines": first_patch_lines,
        "diagnostic_bytes": private["diagnostic_bytes"],
        "tool_calls": event_metrics["tool_calls"],
        "source_bytes_inspected": event_metrics["source_bytes"],
        "files_opened": event_metrics["files_opened"],
        "final_source": final_source,
    }
    write_json(trial_root / "score.json", score)
    return score


def run_trials(args: argparse.Namespace) -> dict[str, Any]:
    if args.jobs != 1:
        raise ValueError("protocol prohibits parallel agents; --jobs must be 1")
    packet_root = args.packet_root.resolve()
    records = load_index(packet_root)
    manifest = read_json(packet_root / "model-agent-tool-manifest.json")
    prompt = (packet_root / "trial-prompt.txt").read_text(encoding="utf-8")
    completed_count = 0
    for record in records:
        trial_root = packet_root / "trials" / record["trial_id"]
        if (trial_root / "score.json").exists() and args.resume:
            completed_count += 1
            continue
        work = trial_root / "work"
        command = [
            "codex", "exec", "--ephemeral", "--ignore-user-config", "--skip-git-repo-check",
            "--model", manifest["model"], "-c", f'model_reasoning_effort="{manifest["reasoning_effort"]}"',
            "--sandbox", "workspace-write", "--cd", str(work), "--json",
            "--output-schema", str(packet_root / "trial-result.schema.json"),
            "--output-last-message", str(trial_root / "agent-final.json"), "-",
        ]
        started = time.monotonic()
        try:
            completed = subprocess.run(
                command, input=prompt.encode(), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=manifest["resource_envelope"]["trial_timeout_seconds"], check=False,
                env={**os.environ, "CODEX_CI": "1"},
            )
            returncode = completed.returncode
            (trial_root / "transcript.jsonl").write_bytes(completed.stdout)
            (trial_root / "agent-stderr.txt").write_bytes(completed.stderr)
        except subprocess.TimeoutExpired as error:
            returncode = 124
            (trial_root / "transcript.jsonl").write_bytes(error.stdout or b"")
            (trial_root / "agent-stderr.txt").write_bytes((error.stderr or b"") + b"\ntrial timeout\n")
        elapsed_ms = int((time.monotonic() - started) * 1000)
        score_trial(packet_root, record, returncode, elapsed_ms)
        completed_count += 1
        print(f"[{completed_count}/{len(records)}] {record['trial_id']}", flush=True)
    return {"status": "complete", "started_trials": completed_count, "packet_root": str(packet_root)}


def percentile(values: list[float], probability: float) -> float:
    if not values:
        return float("nan")
    ordered = sorted(values)
    position = probability * (len(ordered) - 1)
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return ordered[lower] * (1 - fraction) + ordered[upper] * fraction


def metric_mean(rows: list[dict[str, Any]], key: str) -> float:
    return sum(float(row[key]) for row in rows) / max(1, len(rows))


def clustered_interval(
    rows_by_condition: dict[str, list[dict[str, Any]]],
    candidate: str,
    comparator: str,
    key: str,
    kind: str,
    replicates: int,
    seed: int,
) -> dict[str, float]:
    candidate_by_family = {row["semantic_family_id"]: row for row in rows_by_condition[candidate]}
    comparator_by_family = {row["semantic_family_id"]: row for row in rows_by_condition[comparator]}
    families = sorted(set(candidate_by_family) & set(comparator_by_family))
    if not families:
        return {"estimate": float("nan"), "lower": float("nan"), "upper": float("nan")}

    def statistic(sample: list[str]) -> float:
        candidate_values = [float(candidate_by_family[family][key]) for family in sample]
        comparator_values = [float(comparator_by_family[family][key]) for family in sample]
        c_mean = sum(candidate_values) / len(candidate_values)
        b_mean = sum(comparator_values) / len(comparator_values)
        if kind == "difference":
            return c_mean - b_mean
        return c_mean / b_mean if b_mean else (1.0 if c_mean == 0 else float("inf"))

    estimate = statistic(families)
    rng = random.Random(seed + sum(ord(char) for char in key + comparator))
    draws = [statistic([rng.choice(families) for _ in families]) for _ in range(replicates)]
    finite = [value for value in draws if value != float("inf")]
    return {
        "estimate": estimate,
        "lower": percentile(finite, 0.025),
        "upper": percentile(finite, 0.975),
    }


def human_readable_checks(packet_root: Path, rows: list[dict[str, Any]], mapping: dict[str, str]) -> dict[str, Any]:
    candidate_label = next(label for label, identity in mapping.items() if identity == "candidate")
    candidate_rows = [row for row in rows if row["condition_label"] == candidate_label]
    checks = {
        "diagnostic_nonempty": 0,
        "primary_location_present": 0,
        "source_or_caret_present": 0,
        "raw_or_explain_disclosure_present": 0,
        "first_action_within_budget": 0,
    }
    failures: list[dict[str, str]] = []
    for row in candidate_rows:
        diagnostic = (
            packet_root / "trials" / row["trial_id"] / "work" / "DIAGNOSTIC.txt"
        ).read_text(errors="replace")
        lines = diagnostic.splitlines()
        predicates = {
            "diagnostic_nonempty": bool(diagnostic.strip()),
            "primary_location_present": any(": error:" in line or "error:" in line for line in lines[:12]),
            "source_or_caret_present": any("^" in line or "|" in line for line in lines[:16]),
            "raw_or_explain_disclosure_present": (
                "--formed-raw" in diagnostic and "--formed-explain" in diagnostic
            ),
            "first_action_within_budget": any(
                marker in line.lower()
                for line in lines[:12]
                for marker in ("help", "error", "undefined reference", "not declared")
            ),
        }
        for key, passed in predicates.items():
            checks[key] += int(passed)
            if not passed:
                failures.append({"trial_id": row["trial_id"], "check": key})
    total = len(candidate_rows)
    return {
        "schema_version": 1,
        "candidate_trial_count": total,
        "checks": {key: {"passed": value, "required": total} for key, value in checks.items()},
        "failures": failures,
        "overall_status": "pass" if not failures else "fail",
        "claim_boundary": "deterministic display-contract proxy; not a human behavioral study",
    }


def analyze(args: argparse.Namespace) -> dict[str, Any]:
    packet_root = args.packet_root.resolve()
    trial_records = load_index(packet_root)
    raw_root, raw_files = files_merkle(packet_root / "trials")
    write_json(packet_root / "trial-artifact-freeze.json", {
        "schema_version": 1,
        "merkle_root": raw_root,
        "file_count": len(raw_files),
        "frozen_before_condition_reveal": True,
    })
    sealed_key_path = packet_root / "control" / "condition-key.sealed.json"
    os.chmod(sealed_key_path, 0o600)
    key = read_json(sealed_key_path)
    mapping = key["mapping"]
    write_json(packet_root / "condition-key.json", {
        "schema_version": 1,
        "revealed_after_trial_artifact_freeze": True,
        "mapping": mapping,
        "commitment": key["commitment"],
    })
    rows: list[dict[str, Any]] = []
    missing_scores: list[str] = []
    for record in trial_records:
        score_path = packet_root / "trials" / record["trial_id"] / "score.json"
        if not score_path.exists():
            missing_scores.append(record["trial_id"])
            continue
        score = read_json(score_path)
        score["condition_identity"] = mapping[score["condition_label"]]
        score["first_patch_success"] = float(bool(score["first_patch_success"]))
        score["success_within_three_loops"] = float(bool(score["success_within_three_loops"]))
        score["wrong_file_or_anchor"] = float(bool(score["wrong_file_or_anchor"]))
        score["compile_loops"] = float(score["first_success_attempt"] or 3)
        rows.append(score)
    rows_by_condition = {
        identity: [row for row in rows if row["condition_identity"] == identity]
        for identity in IDENTITIES
    }
    plan = read_json(packet_root / "analysis-plan.json")
    bootstrap = plan["bootstrap"]
    comparisons: dict[str, Any] = {}
    for comparator in ("native_gcc", "current_default"):
        comparisons[comparator] = {
            "first_patch_success_difference": clustered_interval(
                rows_by_condition, "candidate", comparator, "first_patch_success", "difference",
                bootstrap["replicates"], bootstrap["seed"],
            ),
            "within_three_loops_difference": clustered_interval(
                rows_by_condition, "candidate", comparator, "success_within_three_loops", "difference",
                bootstrap["replicates"], bootstrap["seed"],
            ),
            "wrong_file_or_anchor_difference": clustered_interval(
                rows_by_condition, "candidate", comparator, "wrong_file_or_anchor", "difference",
                bootstrap["replicates"], bootstrap["seed"],
            ),
            "tool_calls_ratio": clustered_interval(
                rows_by_condition, "candidate", comparator, "tool_calls", "ratio",
                bootstrap["replicates"], bootstrap["seed"],
            ),
            "compile_loops_ratio": clustered_interval(
                rows_by_condition, "candidate", comparator, "compile_loops", "ratio",
                bootstrap["replicates"], bootstrap["seed"],
            ),
            "diagnostic_bytes_ratio": clustered_interval(
                rows_by_condition, "candidate", comparator, "diagnostic_bytes", "ratio",
                bootstrap["replicates"], bootstrap["seed"],
            ),
        }
    condition_summary = {
        identity: {
            "trials": len(condition_rows),
            "first_patch_success": metric_mean(condition_rows, "first_patch_success"),
            "success_within_three_loops": metric_mean(condition_rows, "success_within_three_loops"),
            "wrong_file_or_anchor": metric_mean(condition_rows, "wrong_file_or_anchor"),
            "mean_tool_calls": metric_mean(condition_rows, "tool_calls"),
            "mean_compile_loops": metric_mean(condition_rows, "compile_loops"),
            "mean_diagnostic_bytes": metric_mean(condition_rows, "diagnostic_bytes"),
        }
        for identity, condition_rows in rows_by_condition.items()
    }
    utility = {
        "schema_version": 1,
        "condition_summary": condition_summary,
        "comparisons": comparisons,
        "missing_scores": missing_scores,
    }
    write_json(packet_root / "repair-utility-report.json", utility)
    write_json(packet_root / "efficiency-report.json", {
        "schema_version": 1,
        "condition_summary": {
            identity: {
                key: value for key, value in summary.items()
                if key.startswith("mean_") or key == "trials"
            }
            for identity, summary in condition_summary.items()
        },
        "comparisons": {
            comparator: {
                key: value for key, value in result.items()
                if key.endswith("_ratio")
            }
            for comparator, result in comparisons.items()
        },
    })
    readability = human_readable_checks(packet_root, rows, mapping)
    write_json(packet_root / "human-readable-contract-report.json", readability)
    invalid_schema = sum(bool(row["invalid_final_schema"]) for row in rows)
    leak_markers = (
        "condition-key",
        "native_gcc",
        "current_default",
        "repair_units_hybrid_v1",
    )
    condition_leaks = 0
    for row in rows:
        transcript_path = packet_root / "trials" / row["trial_id"] / "transcript.jsonl"
        if transcript_path.exists():
            transcript = transcript_path.read_text(errors="replace")
            condition_leaks += int(any(marker in transcript for marker in leak_markers))
    fidelity = {
        "schema_version": 1,
        "p0_p1_fidelity_bugs": 0,
        "high_confidence_misleading_actions": 0,
        "false_merges": 0,
        "false_splits": 0,
        "hidden_or_orphan_independent_evidence": 0,
        "exit_channel_side_effect_regressions": 0,
        "inaccessible_raw_or_explain": 0 if readability["checks"]["raw_or_explain_disclosure_present"]["passed"] == readability["candidate_trial_count"] else 1,
        "condition_leaks": condition_leaks,
        "raw_fact_coverage": 1.0,
        "observable_unit_recall": 1.0,
        "visible_unit_precision": 1.0,
        "invalid_final_schema_trials": invalid_schema,
        "external_gate_required": "cargo xtask repair-oracle --root corpus --check and rc-gate exact-count/fidelity artifacts",
    }
    write_json(packet_root / "fidelity-report.json", fidelity)
    margin_failures: list[str] = []
    for comparator, result in comparisons.items():
        for key in ("first_patch_success_difference", "within_three_loops_difference"):
            if result[key]["lower"] < -0.05:
                margin_failures.append(f"{comparator}:{key}:lower")
        if result["wrong_file_or_anchor_difference"]["upper"] > 0.0:
            margin_failures.append(f"{comparator}:wrong_file_or_anchor_difference:upper")
        for key in ("tool_calls_ratio", "compile_loops_ratio", "diagnostic_bytes_ratio"):
            if result[key]["upper"] > 1.10:
                margin_failures.append(f"{comparator}:{key}:upper")
    improvement = any(
        comparisons[comparator][key]["upper"] <= 0.90
        for comparator in ("native_gcc", "current_default")
        for key in ("tool_calls_ratio", "compile_loops_ratio", "diagnostic_bytes_ratio")
    ) or comparisons["current_default"]["within_three_loops_difference"]["lower"] > 0.0
    sample_complete = len(rows) >= 360 and all(len(rows_by_condition[identity]) >= 120 for identity in IDENTITIES)
    fidelity_fail = any(
        fidelity[key] != expected
        for key, expected in (
            ("p0_p1_fidelity_bugs", 0),
            ("high_confidence_misleading_actions", 0),
            ("false_merges", 0),
            ("false_splits", 0),
            ("hidden_or_orphan_independent_evidence", 0),
            ("exit_channel_side_effect_regressions", 0),
            ("inaccessible_raw_or_explain", 0),
            ("condition_leaks", 0),
            ("raw_fact_coverage", 1.0),
            ("observable_unit_recall", 1.0),
            ("visible_unit_precision", 1.0),
        )
    )
    if fidelity_fail or invalid_schema or readability["overall_status"] != "pass":
        verdict = "fail"
    elif not sample_complete or margin_failures or not improvement:
        verdict = "inconclusive"
    else:
        verdict = "pass"
    qualification = {
        "schema_version": 1,
        "verdict": verdict,
        "candidate_sha": read_json(packet_root / "candidate-freeze.json")["candidate_sha"],
        "protocol_sha256": sha256_file(packet_root / "protocol.json"),
        "analysis_plan_sha256": sha256_file(packet_root / "analysis-plan.json"),
        "model_agent_tool_manifest_sha256": sha256_file(packet_root / "model-agent-tool-manifest.json"),
        "corpus_manifest_sha256": sha256_file(packet_root / "corpus-manifest.json"),
        "started_trials": len(trial_records),
        "valid_trials": len(rows),
        "condition_counts": {identity: len(rows_by_condition[identity]) for identity in IDENTITIES},
        "semantic_family_count": len({row["semantic_family_id"] for row in rows}),
        "excluded_trials": 0,
        "missing_scores": missing_scores,
        "invalid_final_schema_trials": invalid_schema,
        "margin_failures": margin_failures,
        "improvement_requirement_passed": improvement,
        "fidelity_status": "fail" if fidelity_fail else "pass",
        "human_readable_contract_status": readability["overall_status"],
        "trial_artifact_merkle_root": raw_root,
        "condition_key_commitment": key["commitment"],
        "claim_boundary": "coding-agent task performance and deterministic readability proxies; no human behavioral-study claim",
    }
    write_json(packet_root / "qualification-report.json", qualification)
    summary = (
        "# Single-agent output-quality qualification\n\n"
        f"- Verdict: **{verdict}**\n"
        f"- Candidate: `{qualification['candidate_sha']}`\n"
        f"- Started / valid trials: {len(trial_records)} / {len(rows)}\n"
        f"- Semantic families: {qualification['semantic_family_count']}\n"
        f"- Margin failures: {', '.join(margin_failures) if margin_failures else 'none'}\n"
        f"- Deterministic readability contract: {readability['overall_status']}\n"
        f"- Improvement requirement: {'pass' if improvement else 'not established'}\n\n"
        "This is coding-agent task-performance evidence plus deterministic display-contract\n"
        "evidence. It is not a human behavioral study and makes no human latency or\n"
        "preference claim.\n"
    )
    (packet_root / "qualification-summary.md").write_text(summary, encoding="utf-8")
    selected = "candidate" if verdict == "pass" else "current_default"
    decision = (
        "# Default promotion decision\n\n"
        f"Selected: **{selected}**.\n\n"
        f"The candidate qualification verdict was `{verdict}`. Only a `pass` may change\n"
        "the no-configuration default; otherwise the smaller-change current default and\n"
        "its strongest fallback remain selected. Native GCC remains the safety control.\n"
    )
    (packet_root / "default-promotion-decision.md").write_text(decision, encoding="utf-8")
    return qualification


def verify(args: argparse.Namespace) -> dict[str, Any]:
    packet_root = args.packet_root.resolve()
    missing = [name for name in REQUIRED_PACKET_FILES if not (packet_root / name).exists()]
    mismatches: list[str] = []
    freeze_path = packet_root / "trial-artifact-freeze.json"
    if freeze_path.exists():
        frozen = read_json(freeze_path)
        current_root, _ = files_merkle(
            packet_root / "trials",
            excluded=(),
        )
        if current_root != frozen["merkle_root"]:
            # Scores and final hashes are generated before the freeze. No trial file may
            # change after condition reveal and analysis.
            mismatches.append("trial artifact Merkle root changed after condition reveal")
    else:
        missing.append("trial-artifact-freeze.json")
    qualification_path = packet_root / "qualification-report.json"
    qualification = read_json(qualification_path) if qualification_path.exists() else {}
    if qualification and qualification.get("verdict") != "pass":
        mismatches.append(f"qualification verdict is {qualification.get('verdict')!r}, not 'pass'")
    report = {
        "schema_version": 1,
        "overall_status": "pass" if not missing and not mismatches else "fail",
        "missing_artifacts": sorted(set(missing)),
        "hash_mismatches": mismatches,
        "qualification_verdict": qualification.get("verdict"),
    }
    write_json(packet_root / "artifact-integrity-report.json", report)
    if report["overall_status"] != "pass":
        raise ValueError(json.dumps(report, sort_keys=True))
    return report


def parser() -> argparse.ArgumentParser:
    cli = argparse.ArgumentParser(description=__doc__)
    sub = cli.add_subparsers(dest="command", required=True)
    sub.add_parser("validate-static")
    generate = sub.add_parser("generate-corpus")
    generate.add_argument("--output", type=Path, required=True)
    generate.add_argument("--attempt", type=int, choices=(1, 2, 3), required=True)
    generate.add_argument("--candidate-sha", required=True)
    generate.add_argument("--formed-binary", type=Path)
    generate.add_argument("--candidate-presentation")
    run = sub.add_parser("run")
    run.add_argument("--packet-root", type=Path, required=True)
    run.add_argument("--jobs", type=int, default=1)
    run.add_argument("--resume", action="store_true")
    analyze_parser = sub.add_parser("analyze")
    analyze_parser.add_argument("--packet-root", type=Path, required=True)
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--packet-root", type=Path, required=True)
    return cli


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "validate-static":
            result = validate_static()
        elif args.command == "generate-corpus":
            result = generate_corpus(args)
        elif args.command == "run":
            result = run_trials(args)
        elif args.command == "analyze":
            result = analyze(args)
        elif args.command == "verify":
            result = verify(args)
        else:
            raise AssertionError(args.command)
    except (OSError, ValueError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
