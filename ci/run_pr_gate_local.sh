#!/usr/bin/env bash
set -euo pipefail

# Repo-local heavy gate intended to mirror the current PR workflow closely enough
# that Codex cannot plausibly claim "done" after a shallow implementation pass.
# It is intentionally expensive.

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

REPORT_ROOT="${REPORT_ROOT:-$repo_root/.codex/evidence/pr-gate}"
TARGET_DIR="${TARGET_DIR:-$repo_root/.codex/evidence/pr-target}"
RUNNER_TEMP="${RUNNER_TEMP:-$repo_root/.codex/evidence/tmp}"
TARGET_TRIPLE="${TARGET_TRIPLE:-x86_64-unknown-linux-musl}"
GCC_IMAGE="${GCC_IMAGE:-gcc:15}"
CI_IMAGE_TAG="${CI_IMAGE_TAG:-gcc-formed-ci:pr}"
SIGNING_KEY_HEX="${SIGNING_KEY_HEX:-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f}"
TRUSTED_SIGNING_PUBLIC_KEY_SHA256="${TRUSTED_SIGNING_PUBLIC_KEY_SHA256:-56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

section() {
  printf '\n==> %s\n' "$1"
}

mkdir -p \
  "$REPORT_ROOT/gate" \
  "$REPORT_ROOT/replay" \
  "$REPORT_ROOT/snapshot" \
  "$REPORT_ROOT/self-check" \
  "$REPORT_ROOT/release" \
  "$TARGET_DIR" \
  "$RUNNER_TEMP"

require_cmd git
require_cmd python3
require_cmd cargo
require_cmd docker

if command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed | grep -qx "$TARGET_TRIPLE"; then
    section "Install Rust target"
    rustup target add "$TARGET_TRIPLE"
  fi
fi

if ! command -v cargo-deny >/dev/null 2>&1; then
  section "Install cargo-deny"
  cargo install cargo-deny --locked
fi

package_version="$(
python3 - <<'PY'
import tomllib
with open("Cargo.toml", "rb") as handle:
    cargo = tomllib.load(handle)
print(cargo["workspace"]["package"]["version"])
PY
)"
control_dir="dist/gcc-formed-v${package_version}-linux-x86_64-musl"

section "Capture host build environment"
python3 ci/gate_capture_environment.py \
  --output "$REPORT_ROOT/gate/build-environment-host.json" \
  --mode host \
  --toolchain-file rust-toolchain.toml

section "Build GCC 15 reference-path CI image"
docker build \
  --build-arg "GCC_IMAGE=$GCC_IMAGE" \
  -t "$CI_IMAGE_TAG" \
  -f ci/images/gcc-matrix/Dockerfile \
  .

section "Capture reference CI environment"
python3 ci/gate_capture_environment.py \
  --output "$REPORT_ROOT/gate/build-environment-reference-image.json" \
  --mode ci-image \
  --toolchain-file rust-toolchain.toml \
  --dockerfile ci/images/gcc-matrix/Dockerfile \
  --docker-base-image "$GCC_IMAGE" \
  --docker-image-tag "$CI_IMAGE_TAG"

section "cargo xtask check"
cargo xtask check

section "Representative acceptance replay"
cargo xtask replay \
  --root corpus \
  --subset representative \
  --report-dir "$REPORT_ROOT/replay"

section "Replay stop-ship contract"
python3 ci/gate_replay_contract.py \
  --replay-report "$REPORT_ROOT/replay/replay-report.json" \
  --output "$REPORT_ROOT/gate/replay-stop-ship.json"

section "Build wrapper binary in reference-path image"
docker run --rm \
  -v "$PWD:/workspace" \
  -v "$TARGET_DIR:/tmp/gcc-formed-target" \
  -w /workspace \
  "$CI_IMAGE_TAG" \
  bash -lc "export CARGO_TARGET_DIR=/tmp/gcc-formed-target && cargo build --bin gcc-formed"

section "Wrapper self-check in reference-path image"
docker run --rm \
  -v "$PWD:/workspace" \
  -v "$TARGET_DIR:/tmp/gcc-formed-target" \
  -v "$REPORT_ROOT:/reports" \
  -w /workspace \
  "$CI_IMAGE_TAG" \
  bash -lc "export CARGO_TARGET_DIR=/tmp/gcc-formed-target && /tmp/gcc-formed-target/debug/gcc-formed --formed-self-check > /reports/self-check/report.json"

section "Representative reference-path snapshot check"
cargo xtask snapshot \
  --root corpus \
  --subset representative \
  --check \
  --docker-image gcc:15 \
  --version-band gcc15_plus \
  --report-dir "$REPORT_ROOT/snapshot"

vendor_dir="$RUNNER_TEMP/vendor"
rm -rf "$vendor_dir"

section "Vendor dependency tree"
cargo xtask vendor --output-dir "$vendor_dir" > "$REPORT_ROOT/release/vendor.json"

section "Hermetic release build smoke"
cargo xtask hermetic-release-check \
  --vendor-dir "$vendor_dir" \
  --bin gcc-formed \
  --target-triple "$TARGET_TRIPLE" \
  > "$REPORT_ROOT/release/hermetic-release.json"

release_signing_key="$RUNNER_TEMP/release-signing.key"
printf '%s\n' "$SIGNING_KEY_HEX" > "$release_signing_key"

section "Release packaging smoke"
cargo xtask package \
  --binary "target/hermetic-release/${TARGET_TRIPLE}/release/gcc-formed" \
  --target-triple "$TARGET_TRIPLE" \
  --release-channel beta \
  --maturity-label v1beta \
  --signing-private-key "$release_signing_key" \
  > "$REPORT_ROOT/release/package.json"

signing_key_id="$(
python3 -c 'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["key_id"])' \
  "$control_dir/SHA256SUMS.sig"
)"

install_root="$RUNNER_TEMP/gcc-formed/$TARGET_TRIPLE"
bin_dir="$RUNNER_TEMP/gcc-formed/bin"
rm -rf "$install_root" "$bin_dir"

section "Release install smoke"
cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --expected-signing-key-id "$signing_key_id" \
  --expected-signing-public-key-sha256 "$TRUSTED_SIGNING_PUBLIC_KEY_SHA256" \
  > "$REPORT_ROOT/release/install.json"
"$bin_dir/gcc-formed" --formed-version > "$REPORT_ROOT/release/installed-version.txt"
cargo xtask uninstall \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --mode purge-install \
  > "$REPORT_ROOT/release/uninstall.json"

section "Rollback symlink smoke"
cargo test -p xtask rollback_switches_current_symlink_to_requested_version \
  > "$REPORT_ROOT/release/rollback-smoke.txt"

system_install_root="$RUNNER_TEMP/system-root/opt/cc-formed/$TARGET_TRIPLE"
system_bin_dir="$RUNNER_TEMP/system-root/usr/local/bin"
rm -rf "$system_install_root" "$system_bin_dir"

section "System-wide layout smoke"
cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$system_install_root" \
  --bin-dir "$system_bin_dir" \
  --expected-signing-key-id "$signing_key_id" \
  --expected-signing-public-key-sha256 "$TRUSTED_SIGNING_PUBLIC_KEY_SHA256" \
  > "$REPORT_ROOT/release/system-install.json"
"$system_bin_dir/gcc-formed" --formed-version > "$REPORT_ROOT/release/system-version.txt"
cargo xtask uninstall \
  --install-root "$system_install_root" \
  --bin-dir "$system_bin_dir" \
  --mode purge-install \
  > "$REPORT_ROOT/release/system-uninstall.json"

release_repo_root="$RUNNER_TEMP/gcc-formed-release-repo"
resolved_json="$REPORT_ROOT/release/release-resolve-beta.json"
install_release_root="$RUNNER_TEMP/gcc-formed-release/$TARGET_TRIPLE"
install_release_bin="$RUNNER_TEMP/gcc-formed-release-bin"
rm -rf "$release_repo_root" "$install_release_root" "$install_release_bin"

section "Release repository promote and pin smoke"
cargo xtask release-publish \
  --control-dir "$control_dir" \
  --repository-root "$release_repo_root" \
  > "$REPORT_ROOT/release/release-publish.json"
cargo xtask release-promote \
  --repository-root "$release_repo_root" \
  --target-triple "$TARGET_TRIPLE" \
  --version "$package_version" \
  --channel canary \
  > "$REPORT_ROOT/release/release-promote-canary.json"
cargo xtask release-promote \
  --repository-root "$release_repo_root" \
  --target-triple "$TARGET_TRIPLE" \
  --version "$package_version" \
  --channel beta \
  > "$REPORT_ROOT/release/release-promote-beta.json"
cargo xtask release-resolve \
  --repository-root "$release_repo_root" \
  --target-triple "$TARGET_TRIPLE" \
  --channel beta \
  > "$resolved_json"

primary_sha="$(
python3 -c 'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["primary_archive_sha256"])' \
  "$resolved_json"
)"
resolved_signing_key_id="$(
python3 -c 'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["signing_key_id"])' \
  "$resolved_json"
)"

cargo xtask install-release \
  --repository-root "$release_repo_root" \
  --target-triple "$TARGET_TRIPLE" \
  --version "$package_version" \
  --expected-primary-sha256 "$primary_sha" \
  --expected-signing-key-id "$resolved_signing_key_id" \
  --expected-signing-public-key-sha256 "$TRUSTED_SIGNING_PUBLIC_KEY_SHA256" \
  --install-root "$install_release_root" \
  --bin-dir "$install_release_bin" \
  > "$REPORT_ROOT/release/install-release.json"
"$install_release_bin/gcc-formed" --formed-version > "$REPORT_ROOT/release/install-release-version.txt"
cargo xtask uninstall \
  --install-root "$install_release_root" \
  --bin-dir "$install_release_bin" \
  --mode purge-install \
  > "$REPORT_ROOT/release/install-release-uninstall.json"

section "Release provenance manifest"
python3 ci/release_provenance.py \
  --workflow pr-gate-local \
  --report-root "$REPORT_ROOT" \
  --output "$REPORT_ROOT/release/release-provenance.json" \
  --package-version "$package_version" \
  --target-triple "$TARGET_TRIPLE" \
  --release-channel beta \
  --maturity-label v1beta

section "Dependency and license gate"
cargo deny check > "$REPORT_ROOT/release/cargo-deny.txt"

section "Done"
echo "Local PR parity gate passed."
echo "Artifacts: $REPORT_ROOT"
