#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LAB_ROOT = REPO_ROOT / "eval" / "interop"


def ensure_wrapper_binary() -> Path:
    binary = REPO_ROOT / "target" / "debug" / "gcc-formed"
    if not binary.exists():
        subprocess.run(["cargo", "build", "--bin", "gcc-formed"], cwd=REPO_ROOT, check=True)
    return binary


def make_executable(path: Path) -> None:
    path.chmod(0o755)


def copy_fixture_tree(lab_root: Path, workspace: Path) -> Path:
    project_root = workspace / "project"
    shutil.copytree(lab_root / "project", project_root)
    backend_script = workspace / "fake-gcc.py"
    launcher_script = workspace / "fake-launcher.py"
    shutil.copy2(lab_root / "bin" / "fake-gcc.py", backend_script)
    shutil.copy2(lab_root / "bin" / "fake-launcher.py", launcher_script)
    make_executable(backend_script)
    make_executable(launcher_script)
    return project_root


def create_wrapper_symlinks(wrapper_binary: Path, workspace: Path) -> tuple[Path, Path]:
    bin_dir = workspace / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    gcc_wrapper = bin_dir / "gcc-formed"
    gxx_wrapper = bin_dir / "g++-formed"
    for link in (gcc_wrapper, gxx_wrapper):
        if link.exists() or link.is_symlink():
            link.unlink()
        try:
            link.symlink_to(wrapper_binary)
        except OSError:
            shutil.copy2(wrapper_binary, link)
            make_executable(link)
    return gcc_wrapper, gxx_wrapper


def run(command: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"command failed ({completed.returncode}): {command}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed


def read_json_logs(log_dir: Path) -> list[dict]:
    logs: list[dict] = []
    for path in sorted(log_dir.glob("*.json")):
        logs.append(json.loads(path.read_text(encoding="utf-8")))
    return logs


def case_env(
    base_env: dict[str, str],
    backend_script: Path,
    log_dir: Path,
    runtime_root: Path,
    trace_root: Path,
    gcc_wrapper: Path,
    gxx_wrapper: Path,
    launcher_script: Path | None = None,
    launcher_log_dir: Path | None = None,
) -> dict[str, str]:
    env = base_env.copy()
    env["FORMED_BACKEND_GCC"] = str(backend_script)
    env["INTEROP_FAKE_GCC_LOG_DIR"] = str(log_dir)
    env["FORMED_RUNTIME_DIR"] = str(runtime_root)
    env["FORMED_TRACE_DIR"] = str(trace_root)
    env["CC"] = str(gcc_wrapper)
    env["CXX"] = str(gxx_wrapper)
    env["PATH"] = os.pathsep.join([str(gcc_wrapper.parent), env.get("PATH", "")])
    if launcher_script is not None and launcher_log_dir is not None:
        env["FORMED_BACKEND_LAUNCHER"] = str(launcher_script)
        env["INTEROP_FAKE_LAUNCHER_LOG_DIR"] = str(launcher_log_dir)
    else:
        env.pop("FORMED_BACKEND_LAUNCHER", None)
        env.pop("INTEROP_FAKE_LAUNCHER_LOG_DIR", None)
    return env


def make_case_report(name: str, command: list[str], logs: list[dict], **extra: object) -> dict[str, object]:
    return {
        "name": name,
        "command": command,
        "backend_invocations": len(logs),
        **extra,
    }


def list_relative_entries(root: Path) -> list[str]:
    if not root.exists():
        return []
    entries: list[str] = []
    for path in sorted(root.rglob("*")):
        entries.append(str(path.relative_to(root)))
    return entries


def run_stress_round(
    *,
    suite: str,
    round_index: int,
    root_base: Path,
    pre_commands: list[list[str]] | None,
    command: list[str],
    cwd: Path,
    base_env: dict[str, str],
    backend_script: Path,
    gcc_wrapper: Path,
    gxx_wrapper: Path,
    launcher_script: Path | None = None,
    launcher_log_dir: Path | None = None,
    depfile_paths: list[Path] | None = None,
) -> dict[str, object]:
    round_root = root_base / "stress-runs" / suite / f"round-{round_index}"
    runtime_root = round_root / "runtime"
    trace_root = round_root / "trace"
    log_dir = round_root / "logs"
    log_dir.mkdir(parents=True, exist_ok=True)
    if launcher_log_dir is not None:
        launcher_log_dir.mkdir(parents=True, exist_ok=True)

    env = case_env(
        base_env,
        backend_script,
        log_dir,
        runtime_root,
        trace_root,
        gcc_wrapper,
        gxx_wrapper,
        launcher_script,
        launcher_log_dir,
    )
    for pre_command in pre_commands or []:
        run(pre_command, cwd=cwd, env=env)
    run(command, cwd=cwd, env=env)

    logs = read_json_logs(log_dir)
    launcher_logs = read_json_logs(launcher_log_dir) if launcher_log_dir is not None else []
    runtime_entries = list_relative_entries(runtime_root)
    trace_entries = list_relative_entries(trace_root)
    return {
        "suite": suite,
        "round": round_index,
        "command": command,
        "runtime_root": str(runtime_root),
        "trace_root": str(trace_root),
        "runtime_entries_after": runtime_entries,
        "trace_entries_after": trace_entries,
        "runtime_cleanup_ok": not runtime_entries,
        "trace_cleanup_ok": not trace_entries,
        "backend_invocations": len(logs),
        "launcher_invocations": len(launcher_logs) if launcher_logs else 0,
        "launcher_received_compiler_path": (
            all(log["compiler_path"] == str(backend_script) for log in launcher_logs)
            if launcher_logs
            else None
        ),
        "depfiles_present": (
            all(path.exists() for path in depfile_paths) if depfile_paths is not None else None
        ),
    }


def run_lab(lab_root: Path, report_dir: Path) -> dict[str, object]:
    wrapper_binary = ensure_wrapper_binary()
    report_dir.mkdir(parents=True, exist_ok=True)
    workspace = report_dir / "workspace"
    if workspace.exists():
        shutil.rmtree(workspace)
    workspace.mkdir(parents=True)

    project_root = copy_fixture_tree(lab_root, workspace)
    gcc_wrapper, gxx_wrapper = create_wrapper_symlinks(wrapper_binary, workspace)
    backend_script = workspace / "fake-gcc.py"
    launcher_script = workspace / "fake-launcher.py"

    base_env = os.environ.copy()
    base_env.pop("CC", None)
    base_env.pop("CXX", None)

    cases: list[dict[str, object]] = []

    make_build_dir = workspace / "make-build"
    make_build_log_dir = workspace / "logs" / "make-build"
    make_build_log_dir.mkdir(parents=True, exist_ok=True)
    make_build_runtime_root = workspace / "runtime" / "make-build"
    make_build_trace_root = workspace / "trace" / "make-build"
    make_build_env = case_env(
        base_env,
        backend_script,
        make_build_log_dir,
        make_build_runtime_root,
        make_build_trace_root,
        gcc_wrapper,
        gxx_wrapper,
    )
    make_build_command = ["make", "-j2", f"BUILD_DIR={make_build_dir}", "build"]
    run(make_build_command, cwd=project_root, env=make_build_env)
    make_build_logs = read_json_logs(make_build_log_dir)
    cases.append(
        make_case_report(
            "make-build",
            make_build_command,
            make_build_logs,
            artifacts=[
                str(make_build_dir / "interop_app"),
                str(make_build_dir / "main.d"),
                str(make_build_dir / "helper.d"),
            ],
            depfiles_present=all(
                (make_build_dir / name).exists() for name in ("main.d", "helper.d")
            ),
        )
    )

    make_launcher_dir = workspace / "make-launcher-build"
    make_launcher_backend_log_dir = workspace / "logs" / "make-launcher-build" / "backend"
    make_launcher_log_dir = workspace / "logs" / "make-launcher-build" / "launcher"
    make_launcher_backend_log_dir.mkdir(parents=True, exist_ok=True)
    make_launcher_log_dir.mkdir(parents=True, exist_ok=True)
    make_launcher_env = case_env(
        base_env,
        backend_script,
        make_launcher_backend_log_dir,
        workspace / "runtime" / "make-launcher-build",
        workspace / "trace" / "make-launcher-build",
        gcc_wrapper,
        gxx_wrapper,
        launcher_script,
        make_launcher_log_dir,
    )
    make_launcher_command = ["make", "-j2", f"BUILD_DIR={make_launcher_dir}", "build"]
    run(make_launcher_command, cwd=project_root, env=make_launcher_env)
    make_launcher_backend_logs = read_json_logs(make_launcher_backend_log_dir)
    make_launcher_logs = read_json_logs(make_launcher_log_dir)
    cases.append(
        make_case_report(
            "make-build-with-launcher",
            make_launcher_command,
            make_launcher_backend_logs,
            launcher_invocations=len(make_launcher_logs),
            launcher_received_compiler_path=all(
                log["compiler_path"] == str(backend_script) for log in make_launcher_logs
            ),
            depfiles_present=all(
                (make_launcher_dir / name).exists() for name in ("main.d", "helper.d")
            ),
        )
    )

    make_response_dir = workspace / "make-response"
    make_response_log_dir = workspace / "logs" / "make-response"
    make_response_log_dir.mkdir(parents=True, exist_ok=True)
    make_response_env = case_env(
        base_env,
        backend_script,
        make_response_log_dir,
        workspace / "runtime" / "make-response",
        workspace / "trace" / "make-response",
        gcc_wrapper,
        gxx_wrapper,
    )
    make_response_command = ["make", "-j2", f"BUILD_DIR={make_response_dir}", "response-file"]
    run(make_response_command, cwd=project_root, env=make_response_env)
    make_response_logs = read_json_logs(make_response_log_dir)
    cases.append(
        make_case_report(
            "make-response-file",
            make_response_command,
            make_response_logs,
            response_file_argument_seen=any(
                any(arg.startswith("@") for arg in log["raw_argv"]) for log in make_response_logs
            ),
            response_file_payload_seen=any(
                "-DRESP_FROM_FILE=1" in log["expanded_argv"] for log in make_response_logs
            ),
            depfile_present=(make_response_dir / "response.d").exists(),
        )
    )

    cmake_build_dir = workspace / "cmake-build"
    cmake_log_dir = workspace / "logs" / "cmake-build"
    cmake_log_dir.mkdir(parents=True, exist_ok=True)
    cmake_env = case_env(
        base_env,
        backend_script,
        cmake_log_dir,
        workspace / "runtime" / "cmake-build",
        workspace / "trace" / "cmake-build",
        gcc_wrapper,
        gxx_wrapper,
    )
    cmake_configure_command = [
        "cmake",
        "-S",
        str(project_root),
        "-B",
        str(cmake_build_dir),
        "-G",
        "Unix Makefiles",
        f"-DCMAKE_C_COMPILER={gcc_wrapper}",
        f"-DCMAKE_CXX_COMPILER={gxx_wrapper}",
    ]
    run(cmake_configure_command, cwd=workspace, env=cmake_env)
    cmake_build_command = ["cmake", "--build", str(cmake_build_dir), "--parallel", "2"]
    run(cmake_build_command, cwd=workspace, env=cmake_env)
    cmake_build_logs = read_json_logs(cmake_log_dir)
    cases.append(
        make_case_report(
            "cmake-build",
            cmake_build_command,
            cmake_build_logs,
            artifacts=[
                str(cmake_build_dir / "interop_app"),
                str(cmake_build_dir / "cmake-main.d"),
                str(cmake_build_dir / "cmake-helper.d"),
            ],
            depfiles_present=all(
                (cmake_build_dir / name).exists() for name in ("cmake-main.d", "cmake-helper.d")
            ),
        )
    )

    cmake_launcher_build_dir = workspace / "cmake-launcher-build"
    cmake_launcher_backend_log_dir = workspace / "logs" / "cmake-launcher-build" / "backend"
    cmake_launcher_log_dir = workspace / "logs" / "cmake-launcher-build" / "launcher"
    cmake_launcher_backend_log_dir.mkdir(parents=True, exist_ok=True)
    cmake_launcher_log_dir.mkdir(parents=True, exist_ok=True)
    cmake_launcher_env = case_env(
        base_env,
        backend_script,
        cmake_launcher_backend_log_dir,
        workspace / "runtime" / "cmake-launcher-build",
        workspace / "trace" / "cmake-launcher-build",
        gcc_wrapper,
        gxx_wrapper,
        launcher_script,
        cmake_launcher_log_dir,
    )
    cmake_launcher_configure_command = [
        "cmake",
        "-S",
        str(project_root),
        "-B",
        str(cmake_launcher_build_dir),
        "-G",
        "Unix Makefiles",
        f"-DCMAKE_C_COMPILER={gcc_wrapper}",
        f"-DCMAKE_CXX_COMPILER={gxx_wrapper}",
    ]
    run(cmake_launcher_configure_command, cwd=workspace, env=cmake_launcher_env)
    cmake_launcher_build_command = [
        "cmake",
        "--build",
        str(cmake_launcher_build_dir),
        "--parallel",
        "2",
    ]
    run(cmake_launcher_build_command, cwd=workspace, env=cmake_launcher_env)
    cmake_launcher_backend_logs = read_json_logs(cmake_launcher_backend_log_dir)
    cmake_launcher_logs = read_json_logs(cmake_launcher_log_dir)
    cases.append(
        make_case_report(
            "cmake-build-with-launcher",
            cmake_launcher_build_command,
            cmake_launcher_backend_logs,
            launcher_invocations=len(cmake_launcher_logs),
            launcher_received_compiler_path=all(
                log["compiler_path"] == str(backend_script) for log in cmake_launcher_logs
            ),
            depfiles_present=all(
                (cmake_launcher_build_dir / name).exists()
                for name in ("cmake-main.d", "cmake-helper.d")
            ),
        )
    )

    cmake_response_log_dir = workspace / "logs" / "cmake-response"
    cmake_response_log_dir.mkdir(parents=True, exist_ok=True)
    cmake_response_env = case_env(
        base_env,
        backend_script,
        cmake_response_log_dir,
        workspace / "runtime" / "cmake-response",
        workspace / "trace" / "cmake-response",
        gcc_wrapper,
        gxx_wrapper,
    )
    cmake_response_command = [
        "cmake",
        "--build",
        str(cmake_build_dir),
        "--target",
        "response_file",
        "--parallel",
        "2",
    ]
    run(cmake_response_command, cwd=workspace, env=cmake_response_env)
    cmake_response_logs = read_json_logs(cmake_response_log_dir)
    cases.append(
        make_case_report(
            "cmake-response-file",
            cmake_response_command,
            cmake_response_logs,
            response_file_argument_seen=any(
                any(arg.startswith("@") for arg in log["raw_argv"]) for log in cmake_response_logs
            ),
            response_file_payload_seen=any(
                "-DRESP_FROM_FILE=1" in log["expanded_argv"] for log in cmake_response_logs
            ),
            depfile_present=(cmake_build_dir / "response.d").exists(),
        )
    )

    stdout_log_dir = workspace / "logs" / "stdout-sensitive"
    stdout_log_dir.mkdir(parents=True, exist_ok=True)
    stdout_env = case_env(
        base_env,
        backend_script,
        stdout_log_dir,
        workspace / "runtime" / "stdout-sensitive",
        workspace / "trace" / "stdout-sensitive",
        gcc_wrapper,
        gxx_wrapper,
    )
    source = project_root / "src" / "preprocess.c"
    preprocess = run([str(gcc_wrapper), "-E", str(source)], cwd=workspace, env=stdout_env)
    print_search_dirs = run(
        [str(gcc_wrapper), "-print-search-dirs"], cwd=workspace, env=stdout_env
    )
    print_prog_name = run(
        [str(gcc_wrapper), "-print-prog-name=cc1"], cwd=workspace, env=stdout_env
    )
    stdout_logs = read_json_logs(stdout_log_dir)
    cases.append(
        make_case_report(
            "stdout-sensitive",
            [
                str(gcc_wrapper),
                "-E",
                str(source),
                "&&",
                str(gcc_wrapper),
                "-print-search-dirs",
                "&&",
                str(gcc_wrapper),
                "-print-prog-name=cc1",
            ],
            stdout_logs,
            preprocess_stdout=preprocess.stdout,
            search_dirs_stdout=print_search_dirs.stdout,
            prog_name_stdout=print_prog_name.stdout,
        )
    )

    stress_runs: list[dict[str, object]] = []

    for round_index in range(1, 4):
        make_build_dir = workspace / "stress-runs" / "make-stress" / f"round-{round_index}" / "build"
        make_launcher_dir = (
            workspace / "stress-runs" / "make-launcher-stress" / f"round-{round_index}" / "build"
        )
        stress_runs.append(
            run_stress_round(
                suite="make-stress",
                round_index=round_index,
                root_base=workspace,
                pre_commands=None,
                command=["make", "-j4", f"BUILD_DIR={make_build_dir}", "build"],
                cwd=project_root,
                base_env=base_env,
                backend_script=backend_script,
                gcc_wrapper=gcc_wrapper,
                gxx_wrapper=gxx_wrapper,
                depfile_paths=[make_build_dir / "main.d", make_build_dir / "helper.d"],
            )
        )
        stress_runs.append(
            run_stress_round(
                suite="make-launcher-stress",
                round_index=round_index,
                root_base=workspace,
                pre_commands=None,
                command=["make", "-j4", f"BUILD_DIR={make_launcher_dir}", "build"],
                cwd=project_root,
                base_env=base_env,
                backend_script=backend_script,
                gcc_wrapper=gcc_wrapper,
                gxx_wrapper=gxx_wrapper,
                launcher_script=launcher_script,
                launcher_log_dir=(
                    workspace
                    / "stress-runs"
                    / "make-launcher-stress"
                    / f"round-{round_index}"
                    / "launcher-logs"
                ),
                depfile_paths=[make_launcher_dir / "main.d", make_launcher_dir / "helper.d"],
            )
        )

        cmake_build_dir = workspace / "stress-runs" / "cmake-stress" / f"round-{round_index}" / "build"
        cmake_launcher_dir = (
            workspace / "stress-runs" / "cmake-launcher-stress" / f"round-{round_index}" / "build"
        )
        cmake_configure = [
            "cmake",
            "-S",
            str(project_root),
            "-B",
            str(cmake_build_dir),
            "-G",
            "Unix Makefiles",
            f"-DCMAKE_C_COMPILER={gcc_wrapper}",
            f"-DCMAKE_CXX_COMPILER={gxx_wrapper}",
        ]
        stress_runs.append(
            run_stress_round(
                suite="cmake-stress",
                round_index=round_index,
                root_base=workspace,
                pre_commands=[cmake_configure],
                command=["cmake", "--build", str(cmake_build_dir), "--parallel", "4"],
                cwd=workspace,
                base_env=base_env,
                backend_script=backend_script,
                gcc_wrapper=gcc_wrapper,
                gxx_wrapper=gxx_wrapper,
                depfile_paths=[cmake_build_dir / "cmake-main.d", cmake_build_dir / "cmake-helper.d"],
            )
        )
        cmake_launcher_configure = [
            "cmake",
            "-S",
            str(project_root),
            "-B",
            str(cmake_launcher_dir),
            "-G",
            "Unix Makefiles",
            f"-DCMAKE_C_COMPILER={gcc_wrapper}",
            f"-DCMAKE_CXX_COMPILER={gxx_wrapper}",
        ]
        stress_runs.append(
            run_stress_round(
                suite="cmake-launcher-stress",
                round_index=round_index,
                root_base=workspace,
                pre_commands=[cmake_launcher_configure],
                command=["cmake", "--build", str(cmake_launcher_dir), "--parallel", "4"],
                cwd=workspace,
                base_env=base_env,
                backend_script=backend_script,
                gcc_wrapper=gcc_wrapper,
                gxx_wrapper=gxx_wrapper,
                launcher_script=launcher_script,
                launcher_log_dir=(
                    workspace
                    / "stress-runs"
                    / "cmake-launcher-stress"
                    / f"round-{round_index}"
                    / "launcher-logs"
                ),
                depfile_paths=[
                    cmake_launcher_dir / "cmake-main.d",
                    cmake_launcher_dir / "cmake-helper.d",
                ],
            )
        )

    report = {
        "schema_version": 1,
        "lab_root": str(lab_root),
        "workspace_root": str(workspace),
        "wrapper_binary": str(wrapper_binary),
        "cases": cases,
        "stress_runs": stress_runs,
    }
    report_path = report_dir / "interop-lab-report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lab-root", type=Path, default=DEFAULT_LAB_ROOT)
    parser.add_argument("--report-dir", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    run_lab(args.lab_root, args.report_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
