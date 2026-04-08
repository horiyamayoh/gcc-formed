# Changelog

All notable user-visible changes to `gcc-formed` are recorded here.

The current maturity label is `v1alpha`, and the current artifact semver line is `0.1.x`. This changelog does not imply `v1beta`, `v1.0.0-rc`, or `v1.0.0 stable` support.

## [Unreleased]

### Added

- Added `REPORT_ROOT/gate/` CI artifacts with per-step status JSON, stdout/stderr logs, and `gate-summary.{json,md}` so PR and nightly failures can be triaged without replaying the full GitHub log stream.
- Added representative acceptance and snapshot report output to `cargo xtask replay` / `cargo xtask snapshot` so CI can retain normalized IR, raw stderr, rendered output, and failure summaries as artifacts.
- Added first-release scope, known limitations, and release checklist documents to make the GCC 15 primary contract and GCC 13/14 compatibility path explicit.
- Added issue and pull request templates that require support-tier and trace-bundle context for release-impacting changes.
- Added three more promoted representative fixtures so the GCC 15 acceptance/snapshot gate now covers multiple syntax, type-overload, and linker cases instead of a single exemplar.
- Added `VERSIONING.md` and `ADR-0021` to separate maturity labels, artifact semver, and release channels in the public documentation.

### Changed

- Updated CI workflows to use pinned action SHAs, corrected the Rust toolchain action ref, added rollback smoke coverage, retained gate artifacts, and treat GCC 13/14 nightly runs as health indicators instead of release blockers.
- Updated the CLI to announce conservative compatibility mode when the selected backend is outside the primary GCC 15 render path.
- Tightened representative acceptance verification so promoted fixtures can require a user-owned lead location, and replay quality rates now use expectation-derived denominators instead of the full promoted set.
- Unified README, release notes, checklist, limitations, security policy, and contribution guidance around `v1alpha` as the current maturity line and `0.1.x` as the current artifact line.
- Moved snapshot normalization and comparison logic into `diag_testkit` so harness-side volatile-field handling is centralized, and snapshot reports now distinguish `exact`, `normalization_only`, `semantic`, and `missing_expected` drift kinds.

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
- `0.1.0` is an artifact in the `v1alpha` maturity line, not a `v1beta`, `1.0.0-rc.N`, or `1.0.0` stable release.
