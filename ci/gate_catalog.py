#!/usr/bin/env python3

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path

TRUSTED_SIGNING_PUBLIC_KEY_SHA256 = (
    "56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c"
)
TEST_SIGNING_PRIVATE_KEY_HEX = (
    "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
)

WORKFLOW_ALIASES = {
    "pr": "pr-gate",
    "pr-gate": "pr-gate",
    "nightly": "nightly-gate",
    "nightly-gate": "nightly-gate",
    "rc": "rc-gate",
    "rc-gate": "rc-gate",
}

PLAN_PATHS = {
    "pr-gate": Path("ci/plans/pr-gate.json"),
    "nightly-gate": Path("ci/plans/nightly-gate.json"),
    "rc-gate": Path("ci/plans/rc-gate.json"),
}

NIGHTLY_LANES = {
    "gcc12": {
        "gcc_image": "gcc:12",
        "gcc_label": "gcc12",
        "version_band": "gcc9_12",
        "release_blocker": "false",
    },
    "gcc13": {
        "gcc_image": "gcc:13",
        "gcc_label": "gcc13",
        "version_band": "gcc13_14",
        "release_blocker": "false",
    },
    "gcc14": {
        "gcc_image": "gcc:14",
        "gcc_label": "gcc14",
        "version_band": "gcc13_14",
        "release_blocker": "false",
    },
    "gcc15": {
        "gcc_image": "gcc:15",
        "gcc_label": "gcc15",
        "version_band": "gcc15",
        "release_blocker": "true",
    },
}


@dataclass(frozen=True)
class StepExecution:
    command: str
    run_condition: str = "on_success"
    requires_step_id: str | None = None


def canonical_workflow_name(workflow: str) -> str:
    try:
        return WORKFLOW_ALIASES[workflow]
    except KeyError as error:
        supported = ", ".join(sorted(WORKFLOW_ALIASES))
        raise KeyError(f"unknown workflow `{workflow}` (supported: {supported})") from error


def plan_path_for_workflow(repo_root: Path, workflow: str) -> Path:
    return repo_root / PLAN_PATHS[canonical_workflow_name(workflow)]


def nightly_lane_names(selected_lane: str) -> list[str]:
    if selected_lane == "all":
        return list(NIGHTLY_LANES)
    if selected_lane not in NIGHTLY_LANES:
        supported = ", ".join(["all", *NIGHTLY_LANES])
        raise KeyError(f"unknown nightly lane `{selected_lane}` (supported: {supported})")
    return [selected_lane]


def load_package_version(repo_root: Path) -> str:
    with (repo_root / "Cargo.toml").open("rb") as handle:
        cargo = tomllib.load(handle)
    return cargo["workspace"]["package"]["version"]


def build_execution_env(
    repo_root: Path,
    report_root: Path,
    workflow: str,
    *,
    local_mode: bool,
    matrix_gcc_version: str | None = None,
    matrix_version_band: str | None = None,
    release_blocker: str = "true",
) -> dict[str, str]:
    package_version = os.environ.get("PACKAGE_VERSION") or load_package_version(repo_root)
    if local_mode:
        work_root = Path(os.environ.get("WORK_ROOT", report_root / "work"))
        target_dir = Path(os.environ.get("TARGET_DIR", work_root / "target"))
        dist_dir = Path(os.environ.get("DIST_DIR", work_root / "dist"))
        vendor_dir = Path(os.environ.get("VENDOR_DIR", work_root / "vendor"))
        release_repo_dir = Path(os.environ.get("RELEASE_REPO_DIR", work_root / "release-repo"))
        signing_key_path = Path(os.environ.get("SIGNING_KEY_PATH", work_root / "release-signing.key"))
    else:
        runner_temp = Path(os.environ.get("RUNNER_TEMP", report_root.parent))
        work_root = Path(os.environ.get("WORK_ROOT", runner_temp))
        target_dir = Path(os.environ.get("TARGET_DIR", runner_temp / "gcc-formed-target"))
        dist_dir = Path(os.environ.get("DIST_DIR", "dist"))
        vendor_dir = Path(os.environ.get("VENDOR_DIR", "vendor"))
        release_repo_dir = Path(os.environ.get("RELEASE_REPO_DIR", runner_temp / "gcc-formed-release-repo"))
        signing_key_path = Path(os.environ.get("SIGNING_KEY_PATH", runner_temp / "release-signing.key"))

    control_dir = Path(
        os.environ.get(
            "CONTROL_DIR",
            dist_dir / f"gcc-formed-v{package_version}-linux-x86_64-musl",
        )
    )
    image_tag = "gcc-formed-ci:nightly" if canonical_workflow_name(workflow) == "nightly-gate" else "gcc-formed-ci:pr"

    return {
        "PACKAGE_VERSION": package_version,
        "REPORT_ROOT": str(report_root),
        "WORK_ROOT": str(work_root),
        "TARGET_DIR": str(target_dir),
        "DIST_DIR": str(dist_dir),
        "VENDOR_DIR": str(vendor_dir),
        "CONTROL_DIR": str(control_dir),
        "RELEASE_REPO_DIR": str(release_repo_dir),
        "SIGNING_KEY_PATH": str(signing_key_path),
        "REPO_ROOT": str(repo_root),
        "CI_IMAGE_TAG": image_tag,
        "MATRIX_GCC_VERSION": matrix_gcc_version or "",
        "MATRIX_VERSION_BAND": matrix_version_band or "",
        "RELEASE_BLOCKER": release_blocker,
    }


def policy_skips_step(
    step: dict,
    *,
    matrix_version_band: str | None = None,
    release_blocker: str = "true",
) -> bool:
    policy = step.get("policy", "always")
    if policy == "release_blocker_only":
        return release_blocker == "false"
    if policy in {"release_lane_only", "reference_path_only"}:
        return release_blocker == "false"
    return False


def common_prepare_directories_command() -> str:
    return (
        'mkdir -p "$REPORT_ROOT/replay" "$REPORT_ROOT/snapshot" "$REPORT_ROOT/self-check" '
        '"$REPORT_ROOT/release" "$REPORT_ROOT/gate" "$TARGET_DIR" "$DIST_DIR" "$VENDOR_DIR" "$WORK_ROOT"'
    )


def merge_build_environment_command(source_path: str, section_name: str) -> str:
    return "\n".join(
        [
            "python3 - <<'PY'",
            "import json",
            "import os",
            "from pathlib import Path",
            "",
            f'source = Path(os.path.expandvars("{source_path}"))',
            'target = Path(os.environ["REPORT_ROOT"]) / "gate" / "build-environment.json"',
            'payload = json.loads(source.read_text(encoding="utf-8"))',
            "if target.exists():",
            '    merged = json.loads(target.read_text(encoding="utf-8"))',
            "else:",
            "    merged = {}",
            'merged["schema_version"] = payload.get("schema_version", 1)',
            'merged["updated_at"] = payload.get("updated_at")',
            'merged.setdefault("host", None)',
            'merged.setdefault("ci_image", None)',
            f'merged["{section_name}"] = payload.get("{section_name}")',
            'target.write_text(json.dumps(merged, indent=2) + "\\n", encoding="utf-8")',
            "PY",
        ]
    )


def capture_host_environment_command(
    output_path: str = '$REPORT_ROOT/gate/build-environment.json',
    *,
    merge_into_summary: bool = False,
) -> str:
    command = (
        f'python3 ci/gate_capture_environment.py --output "{output_path}" '
        '--mode host --toolchain-file rust-toolchain.toml'
    )
    if not merge_into_summary:
        return command
    return "\n".join([command, merge_build_environment_command(output_path, "host")])


def capture_ci_environment_command(
    docker_base_image: str,
    *,
    output_path: str = '$REPORT_ROOT/gate/build-environment.json',
    docker_image_tag: str = '"$CI_IMAGE_TAG"',
    merge_into_summary: bool = False,
) -> str:
    command = (
        f'python3 ci/gate_capture_environment.py --output "{output_path}" '
        '--mode ci-image --toolchain-file rust-toolchain.toml --dockerfile ci/images/gcc-matrix/Dockerfile '
        f'--docker-base-image {docker_base_image} --docker-image-tag {docker_image_tag}'
    )
    if not merge_into_summary:
        return command
    return "\n".join([command, merge_build_environment_command(output_path, "ci_image")])


def build_ci_image_command(docker_base_image: str) -> str:
    return (
        f'docker build --build-arg GCC_IMAGE={docker_base_image} '
        '-t "$CI_IMAGE_TAG" -f ci/images/gcc-matrix/Dockerfile .'
    )


def build_ci_image_with_tag_command(docker_base_image: str, docker_image_tag: str) -> str:
    return (
        f'docker build --build-arg GCC_IMAGE={docker_base_image} '
        f'-t {docker_image_tag} -f ci/images/gcc-matrix/Dockerfile .'
    )


def build_wrapper_binary_command() -> str:
    return "\n".join(
        [
            "docker run --rm \\",
            '  -v "$PWD:/workspace" \\',
            '  -v "$TARGET_DIR:/tmp/gcc-formed-target" \\',
            "  -w /workspace \\",
            '  "$CI_IMAGE_TAG" \\',
            '  bash -lc "export CARGO_TARGET_DIR=/tmp/gcc-formed-target && cargo build --bin gcc-formed"',
        ]
    )


def build_wrapper_binary_in_image_command(docker_image_tag: str, target_subdir: str) -> str:
    return "\n".join(
        [
            "docker run --rm \\",
            '  -v "$PWD:/workspace" \\',
            '  -v "$TARGET_DIR:/tmp/gcc-formed-target" \\',
            "  -w /workspace \\",
            f"  {docker_image_tag} \\",
            '  bash -lc "export CARGO_TARGET_DIR=/tmp/gcc-formed-target/'
            + target_subdir
            + ' && cargo build --bin gcc-formed"',
        ]
    )


def wrapper_self_check_command() -> str:
    return "\n".join(
        [
            "docker run --rm \\",
            '  -v "$PWD:/workspace" \\',
            '  -v "$TARGET_DIR:/tmp/gcc-formed-target" \\',
            '  -v "$REPORT_ROOT:/reports" \\',
            "  -w /workspace \\",
            '  "$CI_IMAGE_TAG" \\',
            '  bash -lc "export CARGO_TARGET_DIR=/tmp/gcc-formed-target && '
            '/tmp/gcc-formed-target/debug/gcc-formed --formed-self-check > /reports/self-check/report.json"',
        ]
    )


def vendor_command() -> str:
    return 'cargo xtask vendor --output-dir "$VENDOR_DIR" > "$REPORT_ROOT/release/vendor.json"'


def hermetic_release_command() -> str:
    return (
        'cargo xtask hermetic-release-check --vendor-dir "$VENDOR_DIR" --bin gcc-formed '
        '--target-triple x86_64-unknown-linux-musl > "$REPORT_ROOT/release/hermetic-release.json"'
    )


def release_packaging_command() -> str:
    return "\n".join(
        [
            f'printf "%s\\n" "{TEST_SIGNING_PRIVATE_KEY_HEX}" > "$SIGNING_KEY_PATH"',
            "cargo xtask package \\",
            '  --binary target/hermetic-release/x86_64-unknown-linux-musl/release/gcc-formed \\',
            '  --target-triple x86_64-unknown-linux-musl \\',
            '  --out-dir "$DIST_DIR" \\',
            '  --release-channel beta \\',
            '  --maturity-label v1beta \\',
            '  --signing-private-key "$SIGNING_KEY_PATH" \\',
            '  > "$REPORT_ROOT/release/package.json"',
        ]
    )


def release_install_command(system_layout: bool = False) -> str:
    install_root = (
        '$WORK_ROOT/system-root/opt/cc-formed/x86_64-unknown-linux-musl'
        if system_layout
        else '$WORK_ROOT/install-root/x86_64-unknown-linux-musl'
    )
    bin_dir = '$WORK_ROOT/system-root/usr/local/bin' if system_layout else '$WORK_ROOT/install-bin'
    install_json = "system-install.json" if system_layout else "install.json"
    version_txt = "system-version.txt" if system_layout else "installed-version.txt"
    uninstall_json = "system-uninstall.json" if system_layout else "uninstall.json"
    return "\n".join(
        [
            f'trusted_signing_public_key_sha256="{TRUSTED_SIGNING_PUBLIC_KEY_SHA256}"',
            'signing_key_id="$(python3 -c '
            '\'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["key_id"])\' '
            '"$CONTROL_DIR/SHA256SUMS.sig")"',
            f'cargo xtask install --control-dir "$CONTROL_DIR" --install-root "{install_root}" '
            f'--bin-dir "{bin_dir}" --expected-signing-key-id "$signing_key_id" '
            '--expected-signing-public-key-sha256 "$trusted_signing_public_key_sha256" '
            f'> "$REPORT_ROOT/release/{install_json}"',
            f'"{bin_dir}/gcc-formed" --formed-version > "$REPORT_ROOT/release/{version_txt}"',
            f'cargo xtask uninstall --install-root "{install_root}" --bin-dir "{bin_dir}" '
            f'--mode purge-install > "$REPORT_ROOT/release/{uninstall_json}"',
        ]
    )


def release_repository_command() -> str:
    return "\n".join(
        [
            f'trusted_signing_public_key_sha256="{TRUSTED_SIGNING_PUBLIC_KEY_SHA256}"',
            'resolved_json="$REPORT_ROOT/release/release-resolve-beta.json"',
            'install_root="$WORK_ROOT/release-install/x86_64-unknown-linux-musl"',
            'bin_dir="$WORK_ROOT/release-install-bin"',
            'cargo xtask release-publish --control-dir "$CONTROL_DIR" --repository-root "$RELEASE_REPO_DIR" '
            '> "$REPORT_ROOT/release/release-publish.json"',
            'cargo xtask release-promote --repository-root "$RELEASE_REPO_DIR" '
            '--target-triple x86_64-unknown-linux-musl --version "$PACKAGE_VERSION" --channel canary '
            '> "$REPORT_ROOT/release/release-promote-canary.json"',
            'cargo xtask release-promote --repository-root "$RELEASE_REPO_DIR" '
            '--target-triple x86_64-unknown-linux-musl --version "$PACKAGE_VERSION" --channel beta '
            '> "$REPORT_ROOT/release/release-promote-beta.json"',
            'cargo xtask release-resolve --repository-root "$RELEASE_REPO_DIR" '
            '--target-triple x86_64-unknown-linux-musl --channel beta > "$resolved_json"',
            'primary_sha="$(python3 -c '
            '\'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["primary_archive_sha256"])\' '
            '"$resolved_json")"',
            'signing_key_id="$(python3 -c '
            '\'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["signing_key_id"])\' '
            '"$resolved_json")"',
            'cargo xtask install-release --repository-root "$RELEASE_REPO_DIR" '
            '--target-triple x86_64-unknown-linux-musl --version "$PACKAGE_VERSION" '
            '--expected-primary-sha256 "$primary_sha" --expected-signing-key-id "$signing_key_id" '
            '--expected-signing-public-key-sha256 "$trusted_signing_public_key_sha256" '
            '--install-root "$install_root" --bin-dir "$bin_dir" '
            '> "$REPORT_ROOT/release/install-release.json"',
            '"$bin_dir/gcc-formed" --formed-version > "$REPORT_ROOT/release/install-release-version.txt"',
            'cargo xtask uninstall --install-root "$install_root" --bin-dir "$bin_dir" --mode purge-install '
            '> "$REPORT_ROOT/release/install-release-uninstall.json"',
        ]
    )


def release_provenance_command(workflow: str) -> str:
    parts = [
        "python3 ci/release_provenance.py",
        f'  --workflow {workflow}',
        '  --report-root "$REPORT_ROOT"',
        '  --output "$REPORT_ROOT/release/release-provenance.json"',
        '  --package-version "$PACKAGE_VERSION"',
        "  --target-triple x86_64-unknown-linux-musl",
        "  --release-channel beta",
        "  --maturity-label v1beta",
    ]
    if workflow == "nightly-gate":
        parts.extend(
            [
                '  --matrix-gcc-image "$MATRIX_GCC_VERSION"',
                '  --matrix-version-band "$MATRIX_VERSION_BAND"',
                '  --release-blocker "$RELEASE_BLOCKER"',
            ]
        )
    return " \\\n".join(parts)


def dependency_gate_command() -> str:
    return "\n".join(
        [
            "cargo install cargo-deny --locked",
            'cargo deny check > "$REPORT_ROOT/release/cargo-deny.txt"',
        ]
    )


def wrapper_self_check_in_image_command(
    docker_image_tag: str,
    target_subdir: str,
    report_subpath: str,
) -> str:
    return "\n".join(
        [
            "docker run --rm \\",
            '  -v "$PWD:/workspace" \\',
            '  -v "$TARGET_DIR:/tmp/gcc-formed-target" \\',
            '  -v "$REPORT_ROOT:/reports" \\',
            "  -w /workspace \\",
            f"  {docker_image_tag} \\",
            '  bash -lc "export CARGO_TARGET_DIR=/tmp/gcc-formed-target/'
            + target_subdir
            + ' && /tmp/gcc-formed-target/'
            + target_subdir
            + '/debug/gcc-formed --formed-self-check > /reports/'
            + report_subpath
            + '"',
        ]
    )


def pr_prepare_directories_command() -> str:
    return (
        'mkdir -p "$REPORT_ROOT/replay" '
        '"$REPORT_ROOT/snapshot/gcc9_12" "$REPORT_ROOT/snapshot/gcc13_14" "$REPORT_ROOT/snapshot/gcc15" '
        '"$REPORT_ROOT/self-check/gcc9_12" "$REPORT_ROOT/self-check/gcc13_14" "$REPORT_ROOT/self-check/gcc15" '
        '"$REPORT_ROOT/release" "$REPORT_ROOT/gate" "$TARGET_DIR" "$DIST_DIR" "$VENDOR_DIR" "$WORK_ROOT"'
    )


EXECUTION_CATALOG = {
    "pr-gate": {
        "prepare-report-directories": StepExecution(pr_prepare_directories_command()),
        "capture-host-build-environment": StepExecution(
            capture_host_environment_command(
                '$REPORT_ROOT/gate/build-environment-host.json',
                merge_into_summary=True,
            )
        ),
        "build-gcc12-ci-image": StepExecution(
            build_ci_image_with_tag_command("gcc:12", "gcc-formed-ci:pr-gcc12")
        ),
        "capture-gcc12-ci-environment": StepExecution(
            capture_ci_environment_command(
                "gcc:12",
                output_path='$REPORT_ROOT/gate/build-environment-gcc12.json',
                docker_image_tag="gcc-formed-ci:pr-gcc12",
                merge_into_summary=True,
            )
        ),
        "build-gcc13-ci-image": StepExecution(
            build_ci_image_with_tag_command("gcc:13", "gcc-formed-ci:pr-gcc13")
        ),
        "capture-gcc13-ci-environment": StepExecution(
            capture_ci_environment_command(
                "gcc:13",
                output_path='$REPORT_ROOT/gate/build-environment-gcc13.json',
                docker_image_tag="gcc-formed-ci:pr-gcc13",
                merge_into_summary=True,
            )
        ),
        "build-gcc15-ci-image": StepExecution(
            build_ci_image_with_tag_command("gcc:15", "gcc-formed-ci:pr-gcc15")
        ),
        "capture-gcc15-ci-environment": StepExecution(
            capture_ci_environment_command(
                "gcc:15",
                output_path='$REPORT_ROOT/gate/build-environment-gcc15.json',
                docker_image_tag="gcc-formed-ci:pr-gcc15",
                merge_into_summary=True,
            )
        ),
        "cargo-xtask-check": StepExecution("cargo xtask check"),
        "representative-acceptance-replay": StepExecution(
            'cargo xtask replay --root corpus --subset representative --report-dir "$REPORT_ROOT/replay"'
        ),
        "path-aware-replay-stop-ship": StepExecution(
            'python3 ci/gate_replay_contract.py --replay-report "$REPORT_ROOT/replay/replay-report.json" '
            '--output "$REPORT_ROOT/gate/replay-stop-ship.json"',
            run_condition="after_step_not_skipped",
            requires_step_id="representative-acceptance-replay",
        ),
        "build-wrapper-binary-gcc12-image": StepExecution(
            build_wrapper_binary_in_image_command("gcc-formed-ci:pr-gcc12", "gcc12")
        ),
        "wrapper-self-check-gcc12-image": StepExecution(
            wrapper_self_check_in_image_command(
                "gcc-formed-ci:pr-gcc12",
                "gcc12",
                "self-check/gcc9_12/report.json",
            )
        ),
        "representative-gcc12-snapshot-check": StepExecution(
            'cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:12 '
            '--version-band gcc9_12 --report-dir "$REPORT_ROOT/snapshot/gcc9_12"'
        ),
        "build-wrapper-binary-gcc13-image": StepExecution(
            build_wrapper_binary_in_image_command("gcc-formed-ci:pr-gcc13", "gcc13")
        ),
        "wrapper-self-check-gcc13-image": StepExecution(
            wrapper_self_check_in_image_command(
                "gcc-formed-ci:pr-gcc13",
                "gcc13",
                "self-check/gcc13_14/report.json",
            )
        ),
        "representative-gcc13-snapshot-check": StepExecution(
            'cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:13 '
            '--version-band gcc13_14 --report-dir "$REPORT_ROOT/snapshot/gcc13_14"'
        ),
        "build-wrapper-binary-gcc15-image": StepExecution(
            build_wrapper_binary_in_image_command("gcc-formed-ci:pr-gcc15", "gcc15")
        ),
        "wrapper-self-check-gcc15-image": StepExecution(
            wrapper_self_check_in_image_command(
                "gcc-formed-ci:pr-gcc15",
                "gcc15",
                "self-check/gcc15/report.json",
            )
        ),
        "representative-gcc15-snapshot-check": StepExecution(
            'cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15 '
            '--version-band gcc15 --report-dir "$REPORT_ROOT/snapshot/gcc15"'
        ),
        "vendor-dependency-tree": StepExecution(vendor_command()),
        "hermetic-release-build-smoke": StepExecution(hermetic_release_command()),
        "release-packaging-smoke": StepExecution(release_packaging_command()),
        "release-install-smoke": StepExecution(release_install_command()),
        "rollback-symlink-smoke": StepExecution(
            'cargo test -p xtask rollback_switches_current_symlink_to_requested_version '
            '> "$REPORT_ROOT/release/rollback-smoke.txt"'
        ),
        "system-wide-layout-smoke": StepExecution(release_install_command(system_layout=True)),
        "release-repository-promote-and-pin-smoke": StepExecution(release_repository_command()),
        "release-provenance-manifest": StepExecution(
            release_provenance_command("pr-gate"),
            run_condition="always",
        ),
        "dependency-and-license-gate": StepExecution(dependency_gate_command()),
    },
    "nightly-gate": {
        "prepare-report-directories": StepExecution(common_prepare_directories_command()),
        "capture-host-build-environment": StepExecution(capture_host_environment_command()),
        "build-matrix-ci-image": StepExecution(build_ci_image_command('"$MATRIX_GCC_VERSION"')),
        "capture-matrix-ci-environment": StepExecution(
            capture_ci_environment_command('"$MATRIX_GCC_VERSION"')
        ),
        "cargo-test-workspace": StepExecution("cargo test --workspace"),
        "representative-acceptance-replay": StepExecution(
            'cargo xtask replay --root corpus --subset representative --report-dir "$REPORT_ROOT/replay"'
        ),
        "path-aware-replay-stop-ship": StepExecution(
            'python3 ci/gate_replay_contract.py --replay-report "$REPORT_ROOT/replay/replay-report.json" '
            '--output "$REPORT_ROOT/gate/replay-stop-ship.json"',
            run_condition="after_step_not_skipped",
            requires_step_id="representative-acceptance-replay",
        ),
        "cargo-xtask-bench-smoke": StepExecution(
            'cargo xtask bench-smoke > "$REPORT_ROOT/release/bench-smoke.json"'
        ),
        "cargo-xtask-fuzz-smoke": StepExecution(
            'cargo xtask fuzz-smoke --root fuzz --report-dir "$REPORT_ROOT/release"'
        ),
        "build-wrapper-binary-matrix-image": StepExecution(build_wrapper_binary_command()),
        "wrapper-self-check-matrix-image": StepExecution(wrapper_self_check_command()),
        "representative-matrix-snapshot-check": StepExecution(
            'cargo xtask snapshot --root corpus --subset representative --check '
            '--docker-image "$MATRIX_GCC_VERSION" --version-band "$MATRIX_VERSION_BAND" '
            '--report-dir "$REPORT_ROOT/snapshot"'
        ),
        "vendor-dependency-tree": StepExecution(vendor_command()),
        "hermetic-release-build-smoke": StepExecution(hermetic_release_command()),
        "release-packaging-smoke": StepExecution(release_packaging_command()),
        "release-install-smoke": StepExecution(release_install_command()),
        "rollback-symlink-smoke": StepExecution(
            'cargo test -p xtask rollback_switches_current_symlink_to_requested_version '
            '> "$REPORT_ROOT/release/rollback-smoke.txt"'
        ),
        "system-wide-layout-smoke": StepExecution(release_install_command(system_layout=True)),
        "release-repository-promote-and-pin-smoke": StepExecution(release_repository_command()),
        "release-provenance-manifest": StepExecution(
            release_provenance_command("nightly-gate"),
            run_condition="always",
        ),
        "dependency-and-license-gate": StepExecution(dependency_gate_command()),
    },
    "rc-gate": {
        "prepare-report-directories": StepExecution('mkdir -p "$REPORT_ROOT/rc-gate" "$REPORT_ROOT/gate"'),
        "capture-host-build-environment": StepExecution(capture_host_environment_command()),
        "cargo-xtask-rc-gate": StepExecution(
            'cargo xtask rc-gate --root corpus --report-dir "$REPORT_ROOT/rc-gate" '
            '--metrics-manual-report eval/rc/metrics-manual-eval.json '
            '--issue-budget-report eval/rc/issue-budget.json --fuzz-root fuzz '
            '--ux-signoff-report eval/rc/ux-signoff.json --allow-pending-manual-checks'
        ),
        "path-aware-replay-stop-ship": StepExecution(
            'python3 ci/gate_replay_contract.py --replay-report "$REPORT_ROOT/rc-gate/replay-report.json" '
            '--output "$REPORT_ROOT/gate/replay-stop-ship.json"',
            run_condition="after_step_not_skipped",
            requires_step_id="cargo-xtask-rc-gate",
        ),
    },
}
