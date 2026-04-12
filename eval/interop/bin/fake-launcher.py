#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path


def log_dir() -> Path:
    value = os.environ.get("INTEROP_FAKE_LAUNCHER_LOG_DIR")
    if not value:
        raise SystemExit("INTEROP_FAKE_LAUNCHER_LOG_DIR is required")
    path = Path(value)
    path.mkdir(parents=True, exist_ok=True)
    return path


def write_log(payload: dict) -> None:
    path = log_dir() / f"{os.getpid()}-{time.time_ns()}.json"
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    if len(sys.argv) < 2:
        raise SystemExit("fake-launcher.py requires a compiler path as argv[1]")
    compiler = sys.argv[1]
    compiler_args = sys.argv[2:]
    write_log(
        {
            "argv0": sys.argv[0],
            "cwd": os.getcwd(),
            "raw_argv": sys.argv[1:],
            "compiler_path": compiler,
            "compiler_args": compiler_args,
        }
    )
    os.execv(compiler, [compiler, *compiler_args])


if __name__ == "__main__":
    raise SystemExit(main())
