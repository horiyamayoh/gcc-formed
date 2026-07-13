#!/usr/bin/env python3
"""V2 sealed qualification controller with a source-disjoint holdout epoch."""

from dataclasses import replace
import importlib.util
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent
BASE_PATH = ROOT.parent / "output-quality-single-agent-v1" / "harness.py"
spec = importlib.util.spec_from_file_location("output_quality_v1_base", BASE_PATH)
assert spec and spec.loader
base = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = base
spec.loader.exec_module(base)

base.ROOT = ROOT
base.PROTOCOL = ROOT / "protocol.json"
base.ANALYSIS_PLAN = ROOT / "analysis-plan.json"
base.AGENT_MANIFEST = ROOT / "model-agent-tool-manifest.json"
base.ATTESTATION = ROOT / "no-subagent-attestation.json"
base.TRIAL_PROMPT = ROOT / "trial-prompt.txt"
base.RESULT_SCHEMA = ROOT / "trial-result.schema.json"
base.STATIC_FILES = (
    base.PROTOCOL,
    base.ANALYSIS_PLAN,
    base.AGENT_MANIFEST,
    base.ATTESTATION,
    base.TRIAL_PROMPT,
    base.RESULT_SCHEMA,
)

_write_json_v1 = base.write_json


def write_json_v2(path: Path, value):
    if path.name == "corpus-manifest.json" and isinstance(value, dict):
        value = dict(value)
        value["families"] = [
            {**family, "semantic_family_id": f"F{index + 121:03d}"}
            for index, family in enumerate(value.get("families", []))
        ]
    _write_json_v1(path, value)


base.write_json = write_json_v2

_task_for_v1 = base.task_for


def task_for_v2(index: int, variant: int, attempt: int):
    task = _task_for_v1(index, variant, attempt)
    old_suffix = f"{index + 1}_{variant + 1}_{attempt}"
    new_suffix = f"{index + 121}_{variant + 1}_{attempt}"

    def rewrite(value: str) -> str:
        return value.replace(old_suffix, new_suffix)

    return replace(
        task,
        family_id=f"F{index + 121:03d}",
        files={rewrite(path): rewrite(content) for path, content in task.files.items()},
        build_command=[rewrite(item) for item in task.build_command],
        allowed_files=[rewrite(path) for path in task.allowed_files],
        repair_token=rewrite(task.repair_token),
    )


base.task_for = task_for_v2

_diagnostic_for_v1 = base.diagnostic_for


def diagnostic_for_v2(*args, **kwargs):
    completed = _diagnostic_for_v1(*args, **kwargs)
    # Compiler diagnostics are on stderr. Driver command echoes on stdout can
    # contain a candidate preset identifier and are not exposed to the agent.
    return subprocess.CompletedProcess(
        completed.args,
        completed.returncode,
        stdout=b"",
        stderr=completed.stderr,
    )


base.diagnostic_for = diagnostic_for_v2
base.BUILD_SH = base.BUILD_SH.replace(
    "git diff --binary -- . ':(exclude).trial' ':(exclude)build' > \".trial/patch-${attempt}.diff\"",
    """git diff --binary -- . ':(exclude).trial' ':(exclude)build' > \".trial/patch-${attempt}.diff\"
while IFS= read -r path; do
  git diff --no-index --binary -- /dev/null \"$path\" >> \".trial/patch-${attempt}.diff\" || [[ $? -eq 1 ]]
done < <(git ls-files --others --exclude-standard -- . ':(exclude).trial' ':(exclude)build')""",
).replace(
    "git status --short --", "git status --short --untracked-files=all --"
)

_score_trial_v1 = base.score_trial


def _status_paths(path: Path) -> list[str]:
    if not path.exists():
        return []
    return sorted(
        {line[3:].strip() for line in path.read_text(errors="replace").splitlines() if len(line) > 3}
    )


def score_trial_v2(packet_root, record, process_returncode, elapsed_ms):
    score = _score_trial_v1(packet_root, record, process_returncode, elapsed_ms)
    trial_root = packet_root / "trials" / record["trial_id"]
    work = trial_root / "work"
    private = base.read_json(trial_root / "controller.json")
    status = subprocess.run(
        [
            "git",
            "status",
            "--short",
            "--untracked-files=all",
            "--",
            ".",
            ":(exclude).trial",
            ":(exclude)build",
        ],
        cwd=work,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    ).stdout.decode(errors="replace")
    changed = sorted({line[3:].strip() for line in status.splitlines() if len(line) > 3})
    token = private["repair_token"]

    def allowed(path: str) -> bool:
        return path in private["allowed_files"] or path.endswith(token)

    final_patch = trial_root / "final.patch"
    with final_patch.open("ab") as handle:
        for relative in changed:
            if (work / relative).is_file() and relative not in private["allowed_files"]:
                completed = subprocess.run(
                    ["git", "diff", "--no-index", "--binary", "--", "/dev/null", relative],
                    cwd=work,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                )
                if completed.returncode not in (0, 1):
                    raise RuntimeError(f"could not capture untracked patch for {relative}")
                handle.write(completed.stdout)
    final_diff = final_patch.read_text(errors="replace")
    repaired = token in final_diff or any(path.endswith(token) for path in changed)
    wrong = any(not allowed(path) for path in changed) or not repaired
    first_changed = _status_paths(work / ".trial" / "status-1.txt")
    first_patch = (work / ".trial" / "patch-1.diff")
    first_text = first_patch.read_text(errors="replace") if first_patch.exists() else ""
    first_repaired = token in first_text or any(path.endswith(token) for path in first_changed)
    first_wrong = any(not allowed(path) for path in first_changed) or not first_repaired
    score["changed_files"] = changed
    score["wrong_file_or_anchor"] = wrong
    score["first_patch_success"] = score["first_success_attempt"] == 1 and not first_wrong
    score["success_within_three_loops"] = (
        score["first_success_attempt"] is not None and not wrong
    )
    final_source = []
    for relative in sorted(set(private["allowed_files"] + changed)):
        path = work / relative
        if path.is_file():
            final_source.append(
                {"path": relative, "sha256": base.sha256_file(path), "bytes": path.stat().st_size}
            )
    score["final_source"] = final_source
    base.write_json(trial_root / "score.json", score)
    return score


base.score_trial = score_trial_v2


if __name__ == "__main__":
    raise SystemExit(base.main())
