---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current release, packaging, and promotion contract.
do_not_use_for: Historical release posture or archived artifact context.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current release, packaging, and promotion contract.
> Do not use for: Historical release posture or archived artifact context.

# Stable Release Runbook

This document defines the automation contract used for the published [`1.0.0` stable cut](https://github.com/horiyamayoh/gcc-formed/releases/tag/v1.0.0). The stable identity promotes the signed `1.0.0-rc.1` payload without rebuilding or rewriting it.

## Preconditions

- `Cargo.toml` workspace version matches the signed RC payload semver, for example `1.0.0-rc.1`.
- The signed RC GitHub prerelease and its control/release-repository bundles exist.
- The stable candidate passes a strict `cargo xtask rc-gate` run with no blockers.
- `RELEASE_SIGNING_PRIVATE_KEY_HEX` is configured for the GitHub workflow or a local Ed25519 signing key is available for `cargo xtask package`.
- A previously published GitHub Release exists for the rollback baseline version, and its `.release-repo.tar.gz` bundle is available.
- The rollback baseline version differs from the stable candidate version.

## Stable Cut Contract

Stable release automation must prove all of the following in one run:

1. Download the canonical `x86_64-unknown-linux-musl` payload and release repository from the signed RC GitHub prerelease; do not rebuild or re-sign it.
2. Verify the RC provenance commit equals the RC release tag commit and the payload version, manifest, checksums, and detached signature agree. Release-orchestration-only maintenance may run from a later workflow checkout without changing the promoted source commit.
3. Seed the immutable release repository from the RC `.release-repo.tar.gz` bundle.
4. Re-publish the unchanged RC control directory into that repository and promote the same published bits through `canary`, `beta`, and `stable` without rebuilding.
5. Install the rollback baseline version, install the candidate by exact version/checksum/signature pin, then roll back with one `current` symlink switch.
6. Publish stable GitHub Release `v1.0.0` with the exact RC payload assets (which retain their honest `1.0.0-rc.N` payload filenames), final release-repo bundle, and stable-cut evidence.

`1.0.0` is the stable release and channel identity. The promoted immutable
payload retains its `1.0.0-rc.N` semver because changing an embedded version,
archive, manifest, checksum, or signature would no longer be a same-bits
promotion. Provenance records both identities explicitly.

The canonical automation entrypoint is `cargo xtask stable-release`.

The GitHub Release body for a stable cut is generated from [PUBLIC-SURFACE.md](../support/PUBLIC-SURFACE.md) plus the canonical support wording in [SUPPORT-BOUNDARY.md](../support/SUPPORT-BOUNDARY.md) via `python3 ci/public_surface.py render-release-body --kind stable ...`. Do not hand-edit workflow-local release-note prose.

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
