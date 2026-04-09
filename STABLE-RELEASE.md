# Stable Release Runbook

This document defines the automation contract for a future stable cut such as `1.0.0`. It does not claim that a stable artifact is already published today; the current public line remains `v1beta` / `0.2.0-beta.N`.

## Preconditions

- `Cargo.toml` workspace version already matches the intended stable artifact semver, for example `1.0.0`.
- The stable candidate passes a strict `cargo xtask rc-gate` run with no blockers.
- `RELEASE_SIGNING_PRIVATE_KEY_HEX` is configured for the GitHub workflow or a local Ed25519 signing key is available for `cargo xtask package`.
- A previously published GitHub Release exists for the rollback baseline version, and its `.release-repo.tar.gz` bundle is available.
- The rollback baseline version differs from the stable candidate version.

## Stable Cut Contract

Stable release automation must prove all of the following in one run:

1. Build the canonical `x86_64-unknown-linux-musl` payload exactly once.
2. Sign the candidate control directory exactly once.
3. Seed the immutable release repository from a previously published `.release-repo.tar.gz` bundle.
4. Publish the candidate into that repository and promote the same published bits through `canary`, `beta`, and `stable` without rebuilding.
5. Install the rollback baseline version, install the candidate by exact version/checksum/signature pin, then roll back with one `current` symlink switch.
6. Publish the same signed control-dir contents, final release-repo bundle, and stable-cut evidence to GitHub Releases.

The canonical automation entrypoint is `cargo xtask stable-release`.

## Local Dry Run

```bash
baseline_version="0.2.0-beta.1"
candidate_version="1.0.0"
artifact_dir="$(mktemp -d)"
repo_root="$artifact_dir/release-repo"
install_root="$artifact_dir/install/x86_64-unknown-linux-musl"
bin_dir="$artifact_dir/bin"
signing_private_key="$PWD/release-signing.key"

gh release download "v${baseline_version}" \
  --dir "$artifact_dir" \
  --pattern "gcc-formed-v${baseline_version}-linux-x86_64-musl.release-repo.tar.gz"

tar -xzf "$artifact_dir/gcc-formed-v${baseline_version}-linux-x86_64-musl.release-repo.tar.gz" -C "$artifact_dir"
mv \
  "$artifact_dir/gcc-formed-v${baseline_version}-linux-x86_64-musl.release-repo" \
  "$repo_root"

cargo xtask vendor --output-dir vendor
cargo xtask hermetic-release-check --vendor-dir vendor --bin gcc-formed --target-triple x86_64-unknown-linux-musl
cargo xtask package \
  --binary "target/hermetic-release/x86_64-unknown-linux-musl/release/gcc-formed" \
  --target-triple x86_64-unknown-linux-musl \
  --release-channel stable \
  --signing-private-key "$signing_private_key"

cargo xtask stable-release \
  --control-dir "dist/gcc-formed-v${candidate_version}-linux-x86_64-musl" \
  --repository-root "$repo_root" \
  --target-triple x86_64-unknown-linux-musl \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --report-dir target/stable-release \
  --rollback-baseline-version "$baseline_version"
```

If the seeded repository already has a trustworthy `stable` channel pointer, `--rollback-baseline-version` may be omitted locally. The GitHub workflow keeps it explicit so the rollback target is auditable from the workflow inputs.

## Evidence Files

Every stable cut must retain these files:

- `stable-release-report.json`
- `stable-release-summary.md`
- `promotion-evidence.json`
- `rollback-drill.json`
- `release-provenance.json`

These evidence files must agree on the candidate version, target triple, signing metadata, promoted channel pointers, and rollback baseline version.
