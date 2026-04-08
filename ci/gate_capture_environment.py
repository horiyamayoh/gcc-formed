#!/usr/bin/env python3

import argparse
import json
import os
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path
import tomllib

SCHEMA_VERSION = 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Capture pinned build-environment metadata for CI gate artifacts."
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path to the build-environment JSON file under REPORT_ROOT/gate/.",
    )
    parser.add_argument(
        "--mode",
        required=True,
        choices=["host", "ci-image"],
        help="Which environment section to capture.",
    )
    parser.add_argument(
        "--toolchain-file",
        required=True,
        help="Path to rust-toolchain.toml used by the workflow and CI image.",
    )
    parser.add_argument(
        "--docker-image-tag",
        default=None,
        help="Built CI image tag such as gcc-formed-ci:pr or gcc-formed-ci:nightly.",
    )
    parser.add_argument(
        "--docker-base-image",
        default=None,
        help="Requested base GCC image reference such as gcc:15.",
    )
    parser.add_argument(
        "--dockerfile",
        default=None,
        help="Dockerfile used to build the CI image.",
    )
    return parser.parse_args()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def run_command(command: list[str]) -> str:
    completed = subprocess.run(
        command,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return completed.stdout.strip()


def load_toolchain_policy(toolchain_path: Path) -> dict:
    parsed = tomllib.loads(toolchain_path.read_text(encoding="utf-8"))
    toolchain = parsed.get("toolchain", {})
    return {
        "path": str(toolchain_path),
        "channel": toolchain.get("channel"),
        "profile": toolchain.get("profile"),
        "components": toolchain.get("components", []),
        "targets": toolchain.get("targets", []),
    }


def parse_version_block(output: str) -> dict:
    lines = [line.rstrip() for line in output.splitlines() if line.strip()]
    payload = {"raw": output}
    if not lines:
        return payload
    payload["version_line"] = lines[0]
    first_line_parts = lines[0].split()
    if len(first_line_parts) >= 2:
        payload["release"] = first_line_parts[1]
    for line in lines[1:]:
        if ": " not in line:
            continue
        key, value = line.split(": ", 1)
        payload[key.strip().replace("-", "_")] = value.strip()
    return payload


def load_payload(output_path: Path) -> dict:
    if output_path.exists():
        payload = json.loads(output_path.read_text(encoding="utf-8"))
    else:
        payload = {}
    payload["schema_version"] = SCHEMA_VERSION
    payload["updated_at"] = utc_now()
    payload.setdefault("host", None)
    payload.setdefault("ci_image", None)
    return payload


def capture_host_environment(toolchain_policy: dict) -> dict:
    return {
        "runner": {
            "os": os.environ.get("RUNNER_OS"),
            "arch": os.environ.get("RUNNER_ARCH"),
            "name": os.environ.get("RUNNER_NAME"),
            "temp": os.environ.get("RUNNER_TEMP"),
        },
        "toolchain_policy": toolchain_policy,
        "rustc": parse_version_block(run_command(["rustc", "--version", "--verbose"])),
        "cargo": parse_version_block(run_command(["cargo", "--version", "--verbose"])),
        "docker": {
            "version": run_command(["docker", "--version"]),
        },
    }


def probe_ci_image(image_tag: str) -> dict:
    inspect_payload = json.loads(run_command(["docker", "image", "inspect", image_tag]))
    image = inspect_payload[0]
    probe_script = r"""
python3 - <<'PY'
import json
import shutil
import subprocess
import sys


def capture(command):
    completed = subprocess.run(
        command,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return completed.stdout.strip()


payload = {
    "gcc": {
        "version_line": capture(["bash", "-lc", "gcc --version | head -n 1"]),
        "dumpfullversion": capture(["gcc", "-dumpfullversion", "-dumpversion"]),
        "dumpmachine": capture(["gcc", "-dumpmachine"]),
        "path": shutil.which("gcc"),
    },
    "rustc": capture(["rustc", "--version", "--verbose"]),
    "cargo": capture(["cargo", "--version", "--verbose"]),
}
json.dump(payload, sys.stdout)
PY
""".strip()
    probe_payload = json.loads(
        run_command(["docker", "run", "--rm", image_tag, "bash", "-lc", probe_script])
    )
    return {
        "image_id": image.get("Id"),
        "repo_tags": image.get("RepoTags") or [],
        "repo_digests": image.get("RepoDigests") or [],
        "architecture": image.get("Architecture"),
        "os": image.get("Os"),
        "gcc": probe_payload["gcc"],
        "rustc": parse_version_block(probe_payload["rustc"]),
        "cargo": parse_version_block(probe_payload["cargo"]),
    }


def capture_ci_image_environment(args: argparse.Namespace, toolchain_policy: dict) -> dict:
    if not args.docker_image_tag:
        raise ValueError("--docker-image-tag is required for --mode ci-image")
    return {
        "dockerfile": args.dockerfile,
        "requested_base_image": args.docker_base_image,
        "built_image_tag": args.docker_image_tag,
        "toolchain_policy": toolchain_policy,
        "image": probe_ci_image(args.docker_image_tag),
    }


def main() -> int:
    args = parse_args()
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    toolchain_policy = load_toolchain_policy(Path(args.toolchain_file))
    payload = load_payload(output_path)

    if args.mode == "host":
        payload["host"] = capture_host_environment(toolchain_policy)
    else:
        payload["ci_image"] = capture_ci_image_environment(args, toolchain_policy)

    output_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
