# Release Notes

This document uses artifact semver for release headings. Artifact `0.1.0` belongs to the `v1alpha` maturity line; it is not a `v1beta`, `1.0.0-rc.N`, or `1.0.0 stable` release.

## 0.1.0

### Current `v1alpha` Scope

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary support target.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 remain compatibility-only paths and may fall back to conservative passthrough or shadow behavior.
- Quality improvements are only guaranteed on the GCC 15 render path.

### Highlights

- Establishes the `v1alpha` GCC-first workspace baseline for wrapper, capture, adapter, IR, render, trace, and corpus replay.
- Adds release packaging support through `cargo xtask package`, generating primary/debug/source archives plus `manifest.json`, `build-info.txt`, and `SHA256SUMS`.
- Adds `cargo xtask install`, `rollback`, and `uninstall` so packaged artifacts can be verified with checksum validation, staged self-check, and `current` symlink switching.
- Adds `cargo xtask vendor` and `cargo xtask hermetic-release-check` so vendored dependency preparation and `--locked --offline --release` verification are part of the release path.
- Adds `cargo xtask release-publish`, `release-promote`, `release-resolve`, and `install-release` so immutable version repositories, metadata-only canary/beta/stable promotion, and exact-version + checksum pin installs are part of the release path.
- Adds optional Ed25519 `SHA256SUMS.sig` generation plus signing key id and trusted signing public key sha256 pin verification, and covers pseudo-root system-wide install layout in the packaging smoke path.
- Stores a `release-provenance.json` bundle in CI artifacts so package/publish/promote/install evidence can be audited per run, and documents key rotation/revoke/emergency re-sign operations.
- Verifies the canonical `x86_64-unknown-linux-musl` artifact end to end: vendored hermetic release build, package generation, install, system-wide pseudo-root layout, immutable release publish/promote, and exact-pin install all run against the musl payload.
- Keeps release scope intentionally narrow: GCC 15 primary support, Linux-first runtime assumptions, and fail-open fallback behavior remain the shipped contract.

## Known Limits

- `0.1.x` remains the alpha-baseline artifact line. Public beta artifacts will start at `0.2.0-beta.N`.
- Release repository channels such as `canary`, `beta`, and `stable` are distribution pointers; they do not change the maturity label of the artifact they point to.
- Release verification now supports trusted signing public key sha256 pinning, so CI and installers can bind detached signatures to a stable trust anchor instead of relying on key id alone.
- `x86_64-unknown-linux-gnu` remains a compatibility and exception path; the shipped release story is now centered on `x86_64-unknown-linux-musl`.
- GCC 13/14 are not a primary enhanced-render target; they should be treated as conservative compatibility paths.
- Raw fallback remains part of the shipped contract when the renderer cannot make a trustworthy improvement over preserved compiler output.
- See [KNOWN-LIMITATIONS.md](KNOWN-LIMITATIONS.md) for the detailed support boundary and fallback semantics.
