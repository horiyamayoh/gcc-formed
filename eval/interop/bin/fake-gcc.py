#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import shlex
import sys
import time
from pathlib import Path


VERSION_STRING = "gcc (Fake GCC) 15.2.0"
SEARCH_DIRS_OUTPUT = "install: =/opt/fake-gcc\nprograms: =/opt/fake-gcc/bin\nlibraries: =/opt/fake-gcc/lib\n"
PROG_NAME_OUTPUT = "/opt/fake-gcc/libexec/cc1"
FILE_NAME_PREFIX = "/opt/fake-gcc/lib/"


def log_dir() -> Path:
    value = os.environ.get("INTEROP_FAKE_GCC_LOG_DIR")
    if not value:
        raise SystemExit("INTEROP_FAKE_GCC_LOG_DIR is required")
    path = Path(value)
    path.mkdir(parents=True, exist_ok=True)
    return path


def write_log(payload: dict) -> None:
    path = log_dir() / f"{os.getpid()}-{time.time_ns()}.json"
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def expand_response_files(raw_args: list[str]) -> list[str]:
    expanded: list[str] = []
    for arg in raw_args:
        if arg.startswith("@") and len(arg) > 1:
            response_file = Path(arg[1:])
            payload = response_file.read_text(encoding="utf-8")
            expanded.extend(shlex.split(payload, comments=False, posix=True))
        else:
            expanded.append(arg)
    return expanded


def argument_value(args: list[str], flag: str) -> str | None:
    for index, arg in enumerate(args):
        if arg == flag and index + 1 < len(args):
            return args[index + 1]
    return None


def has_flag(args: list[str], flag: str) -> bool:
    return flag in args


def first_source(args: list[str]) -> str | None:
    suffixes = (".c", ".cc", ".cpp", ".cxx", ".C")
    for arg in args:
        if not arg.startswith("-") and arg.endswith(suffixes):
            return arg
    return None


def emit_stdout(text: str) -> None:
    sys.stdout.write(text)
    if not text.endswith("\n"):
        sys.stdout.write("\n")


def ensure_parent(path: str | None) -> None:
    if path is None:
        return
    Path(path).parent.mkdir(parents=True, exist_ok=True)


def write_depfile(depfile: str | None, target: str | None, source: str | None) -> None:
    if depfile is None:
        return
    ensure_parent(depfile)
    dep_target = target or depfile
    dep_source = source or "fake-input.c"
    Path(depfile).write_text(f"{dep_target}: {dep_source}\n", encoding="utf-8")


def touch_compilation_output(output: str | None, link_mode: bool) -> None:
    if output is None:
        output = "a.out"
    ensure_parent(output)
    path = Path(output)
    if link_mode:
        path.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
        path.chmod(0o755)
    else:
        path.write_text("fake object\n", encoding="utf-8")


def main() -> int:
    raw_args = sys.argv[1:]
    expanded_args = expand_response_files(raw_args)
    payload = {
        "argv0": sys.argv[0],
        "cwd": os.getcwd(),
        "raw_argv": raw_args,
        "expanded_argv": expanded_args,
        "has_response_file": any(arg.startswith("@") for arg in raw_args),
        "response_payload_seen": "-DRESP_FROM_FILE=1" in expanded_args,
    }
    write_log(payload)

    if any(arg.startswith("-print-search-dirs") for arg in expanded_args):
        emit_stdout(SEARCH_DIRS_OUTPUT)
        return 0

    if any(arg.startswith("-print-prog-name=") for arg in expanded_args):
        emit_stdout(PROG_NAME_OUTPUT)
        return 0

    if any(arg.startswith("-print-file-name=") for arg in expanded_args):
        requested = next(arg.split("=", 1)[1] for arg in expanded_args if arg.startswith("-print-file-name="))
        emit_stdout(FILE_NAME_PREFIX + requested)
        return 0

    if has_flag(expanded_args, "--version"):
        emit_stdout(VERSION_STRING)
        return 0

    if has_flag(expanded_args, "-dumpversion"):
        emit_stdout("15.2.0")
        return 0

    if has_flag(expanded_args, "-dumpfullversion"):
        emit_stdout("15.2.0")
        return 0

    if has_flag(expanded_args, "-dumpmachine"):
        emit_stdout("x86_64-unknown-linux-gnu")
        return 0

    if has_flag(expanded_args, "-E"):
        source = first_source(expanded_args) or "fake-input.c"
        text = f"# 1 \"{source}\"\nint fake_preprocess_marker = 1;\n"
        output = argument_value(expanded_args, "-o")
        if output is not None:
            ensure_parent(output)
            Path(output).write_text(text, encoding="utf-8")
        emit_stdout(text)
        return 0

    if any(arg.startswith("@") for arg in raw_args) and not payload["response_payload_seen"]:
        sys.stderr.write("response-file payload was not expanded\n")
        return 1

    output = argument_value(expanded_args, "-o")
    depfile = argument_value(expanded_args, "-MF")
    dep_target = argument_value(expanded_args, "-MT")
    source = first_source(expanded_args)
    compile_mode = has_flag(expanded_args, "-c")
    link_mode = not compile_mode

    touch_compilation_output(output, link_mode=link_mode)
    if depfile is not None or has_flag(expanded_args, "-MD") or has_flag(expanded_args, "-MMD"):
        write_depfile(depfile, dep_target or output, source)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
