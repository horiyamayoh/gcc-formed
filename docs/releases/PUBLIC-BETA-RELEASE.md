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

# Public Beta Release

This document defines the user-facing public-beta artifact story for the current `v1beta` / `0.2.0-beta.N` baseline.

The current public artifact is `0.2.0-beta.1`. Replace that version with the latest published `0.2.0-beta.N` tag when a newer beta artifact exists.

## Minimum GitHub Release Asset Set

Each public beta GitHub Release must publish these assets for the primary `x86_64-unknown-linux-musl` target:

- `gcc-formed-v0.2.0-beta.1-linux-x86_64-musl.tar.gz`
- `gcc-formed-v0.2.0-beta.1-linux-x86_64-musl.debug.tar.gz`
- `gcc-formed-v0.2.0-beta.1-source.tar.gz`
- `gcc-formed-v0.2.0-beta.1-linux-x86_64-musl.control.tar.gz`
- `gcc-formed-v0.2.0-beta.1-linux-x86_64-musl.release-repo.tar.gz`
- `manifest.json`
- `build-info.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`
- `release-provenance.json`

The GitHub Release body must also link the current [SUPPORT-BOUNDARY.md](../support/SUPPORT-BOUNDARY.md), [KNOWN-LIMITATIONS.md](../support/KNOWN-LIMITATIONS.md), and this document, and must call out the signing key id plus trusted signing public key SHA-256 for the shipped `SHA256SUMS.sig`.

The GitHub Release body is generated from [PUBLIC-SURFACE.md](../support/PUBLIC-SURFACE.md) plus the canonical support wording in [SUPPORT-BOUNDARY.md](../support/SUPPORT-BOUNDARY.md) via `python3 ci/public_surface.py render-release-body --kind beta ...`. Do not hand-maintain workflow heredocs for release-note text.

## Promote Story

Public beta artifacts follow one build and one sign step:

1. Build the canonical `x86_64-unknown-linux-musl` payload with `cargo xtask hermetic-release-check`.
2. Produce one signed control directory with `cargo xtask package`.
3. Publish that control directory into an immutable release repository with `cargo xtask release-publish`.
4. Promote the exact same published bits from `canary` to `beta` with `cargo xtask release-promote`.
5. Upload the same signed control-dir contents and the promoted release-repo bundle to GitHub Releases without rebuilding.

`canary`, `beta`, and `stable` remain release-repository channels, not maturity labels.

The automated GitHub Release workflow expects the repository secret `RELEASE_SIGNING_PRIVATE_KEY_HEX` to contain the Ed25519 private key hex used for `SHA256SUMS.sig`.

## Install from GitHub Release

The most direct end-user path is the control-dir bundle from GitHub Releases.

```bash
version="v0.2.0-beta.1"
artifact_dir="$(mktemp -d)"
install_root="$HOME/.local/opt/cc-formed/x86_64-unknown-linux-musl"
bin_dir="$HOME/.local/bin"

gh release download "$version" \
  --dir "$artifact_dir" \
  --pattern "gcc-formed-${version}-linux-x86_64-musl.control.tar.gz"

tar -xzf "$artifact_dir/gcc-formed-${version}-linux-x86_64-musl.control.tar.gz" -C "$artifact_dir"
control_dir="$artifact_dir/gcc-formed-${version}-linux-x86_64-musl"

cargo xtask install \
  --control-dir "$control_dir" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir"

"$bin_dir/gcc-formed" --formed-version
```

If you want detached-signature verification during install, read `signing_key_id` and `signing_public_key_sha256` from the GitHub Release notes, then pass:

```bash
--expected-signing-key-id "<release-notes key id>" \
--expected-signing-public-key-sha256 "<release-notes trusted public key sha256>"
```

## Rollback and Uninstall

Rollback remains a `current` symlink switch. If multiple versions are installed under the same install root, you can move back to a prior version immediately.

```bash
cargo xtask rollback \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --version 0.2.0-beta.1
```

To remove the managed install payload and launchers:

```bash
cargo xtask uninstall \
  --install-root "$install_root" \
  --bin-dir "$bin_dir" \
  --mode purge-install
```

## Exact Version Pin and `install-release`

The immutable release-repo bundle is the operator / CI path for exact-version installs.

```bash
version="v0.2.0-beta.1"
artifact_dir="$(mktemp -d)"
install_root="$HOME/.local/opt/cc-formed/x86_64-unknown-linux-musl"
bin_dir="$HOME/.local/bin"
trusted_signing_key_id="<release-notes key id>"
trusted_signing_public_key_sha256="<release-notes trusted public key sha256>"

gh release download "$version" \
  --dir "$artifact_dir" \
  --pattern "gcc-formed-${version}-linux-x86_64-musl.release-repo.tar.gz"

tar -xzf "$artifact_dir/gcc-formed-${version}-linux-x86_64-musl.release-repo.tar.gz" -C "$artifact_dir"
repo_root="$artifact_dir/gcc-formed-${version}-linux-x86_64-musl.release-repo"

resolved_json="$artifact_dir/release-resolve.json"
cargo xtask release-resolve \
  --repository-root "$repo_root" \
  --target-triple x86_64-unknown-linux-musl \
  --channel beta \
  > "$resolved_json"

primary_sha="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1], "r", encoding="utf-8"))["primary_archive_sha256"])' "$resolved_json")"

cargo xtask install-release \
  --repository-root "$repo_root" \
  --target-triple x86_64-unknown-linux-musl \
  --version 0.2.0-beta.1 \
  --expected-primary-sha256 "$primary_sha" \
  --expected-signing-key-id "$trusted_signing_key_id" \
  --expected-signing-public-key-sha256 "$trusted_signing_public_key_sha256" \
  --install-root "$install_root" \
  --bin-dir "$bin_dir"
```

For production CI and fleet automation, trust the signing public key SHA-256 from the release notes or another out-of-band trust channel. Do not derive the trust pin only from the downloaded artifact itself.
