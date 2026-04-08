# Changelog

All notable user-visible changes to `gcc-formed` are recorded here.

The project is currently `v1alpha`. This changelog does not imply general-availability stable support.

## [Unreleased]

- No unreleased entries.

## [0.1.0] - 2026-04-07

### Added

- Established the `v1alpha` GCC-first workspace baseline for wrapper, capture, adapter, IR, render, trace, and corpus replay.
- Added release packaging via `cargo xtask package`, including primary/debug/source archives, `manifest.json`, `build-info.txt`, and `SHA256SUMS`.
- Added `cargo xtask install`, `rollback`, and `uninstall` for staged install verification, checksum validation, and `current` symlink switching.
- Added `cargo xtask vendor` and `cargo xtask hermetic-release-check` for vendored, locked, offline release verification.
- Added `cargo xtask release-publish`, `release-promote`, `release-resolve`, and `install-release` for immutable release repositories and exact-version installs.
- Added optional Ed25519 signing for `SHA256SUMS.sig`, trusted public key SHA-256 pin validation, and pseudo-root system-wide install smoke coverage.

### Scope

- The shipped release contract remains intentionally narrow: Linux first, GCC 15 primary support, and `x86_64-unknown-linux-musl` as the canonical production artifact.

### Known Limits

- `x86_64-unknown-linux-gnu` remains a compatibility and exception path rather than the primary shipped artifact.
- Public stable release status is not claimed; the repository baseline remains `v1alpha`.
