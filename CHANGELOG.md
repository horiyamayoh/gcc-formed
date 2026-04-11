# Changelog

All notable user-visible changes to `gcc-formed` are recorded here.

The current maturity label is `v1beta`, and the current artifact semver line is `0.2.0-beta.N`. This changelog does not imply `1.0.0-rc.N` or `1.0.0 stable` support.

## [Unreleased]

### Changed

- Refactored `diag_cli_front` into `args`, `config`, `mode`, `backend`, `execute`, `render`, and `self_check` modules so `src/main.rs` is dispatch-only while preserving the existing CLI contract, trace output, and self-check behavior.
- Added `cargo xtask rc-gate` plus a manual `rc-gate` GitHub Actions workflow so curated replay, rollout matrix, benchmark smoke, deterministic replay, and RC sign-off evidence are aggregated into machine-readable `rc-gate-report.json` / `rc-gate-summary.md` artifacts instead of a checklist-only release-candidate process.
- Changed `cargo xtask bench-smoke` from a target-only stub into a measured benchmark smoke that reports success-path overhead, simple failure p95, and template-heavy failure p95.
- Extended `replay-report.json` fixture summaries with lead confidence, rendered first-action line, raw/rendered line counts, and compression ratios so RC metrics can be derived from corpus replay instead of ad hoc scripts.
- Added `metrics-report.json` and `eval/rc/metrics-manual-eval.json` so rc-gate now retains automated fallback/compression/performance/family-coverage metrics plus the manual raw-GCC comparison packet for TRC / TFAH / first-fix / high-confidence mislead.
- Added `cargo xtask fuzz-smoke`, checked-in `fuzz/` adversarial seeds, and automatic `fuzz-smoke-report.json` / `fuzz-evidence.json` generation so nightly and rc-gate can enforce `fuzz crash 0` without a manual `fuzz-status.json` handoff.
- Added `cargo xtask human-eval-kit` and automatic `rc-gate/human-eval/` bundle generation so representative expert review sheets, task-study sheets, counterbalance order, and manual evidence templates are reproducible from the curated corpus instead of assembled ad hoc per RC.
- Hardened compatibility-path UX so GCC 13/14 and out-of-scope compilers now print exact support-tier / selected-mode / fallback-reason banners, and `--formed-self-check` / `rc-gate` pin those notices in the rollout matrix to catch wording drift.
- Added `cargo xtask stable-release`, `release-stable.yml`, `docs/releases/STABLE-RELEASE.md`, and stable evidence artifacts so a future `1.0.0` cut can seed a prior release-repo bundle, promote one signed candidate through `canary` / `beta` / `stable` without rebuilding, and retain an auditable rollback drill showing a single `current` symlink switch.
- Added `SUPPORT.md`, maintainer runbooks for incident triage / trace bundle collection / rollback, and bug-template links so support routing no longer depends on chat history or maintainer tribal knowledge.
- Added `docs/policies/GOVERNANCE.md`, strengthened `ADR-0020`, and expanded the PR template so stable-prep changes must declare `breaking` / `non-breaking` / `experimental` classification and keep post-`1.0.0` backlog items out of the current shipped contract unless they go through explicit ADR review.
- Extended `cargo xtask check` to run the Python `ci/test_*.py` suite as well, so CI helper scripts and governance/support contract docs are checked through the same local and CI gate instead of relying on separate ad hoc commands.
- Hardened `cargo xtask check` so the standard local and PR gate now fails on `cargo clippy --workspace --all-targets -- -D warnings` regressions instead of treating clippy-cleanliness as an out-of-band manual check.
- Rekeyed release provenance generation around explicit `release_scope` metadata so CI and release workflows now record `maturity_label` and nightly `version_band` instead of emitting legacy `support_tier` fields in `release-provenance.json`.
- Renamed CI gate status, summary, and static-plan metadata from legacy `support_tier` selectors to explicit `gate_scope` plus `version_band`, and switched nightly workflow plumbing from `MATRIX_SUPPORT_TIER` to `MATRIX_VERSION_BAND`.
- Removed remaining current-authority `SupportTier` phrasing from governance and ADR index docs, leaving any surviving `Support Tier` / `compatibility tier` wording explicitly marked as historical ADR-title context only.

## [0.2.0-beta.1] - 2026-04-09

### Added

- Added `REPORT_ROOT/gate/` CI artifacts with per-step status JSON, stdout/stderr logs, and `gate-summary.{json,md}` so PR and nightly failures can be triaged without replaying the full GitHub log stream.
- Added `REPORT_ROOT/gate/build-environment.json` so PR and nightly artifacts retain the host/container `rustc`, `cargo`, Docker, and GCC versions used by the gate.
- Added representative acceptance and snapshot report output to `cargo xtask replay` / `cargo xtask snapshot` so CI can retain normalized IR, raw stderr, rendered output, and failure summaries as artifacts.
- Added reason-coded fallback taxonomy to trace and replay/snapshot outputs so unsupported tiers, sink conflicts, SARIF loss/parse failures, and renderer fallback decisions can be counted instead of reported as ad hoc strings.
- Added 12 new GCC 15 fixtures across syntax, type mismatch, macro/include, linker, overload, and template families so the hand-authored corpus now reaches the 80-fixture beta bar.
- Added corpus governance metadata to the newly promoted GCC 15 fixtures and linked the corpus workflow guide from the top-level README.
- Added `docs/support/SUPPORT-BOUNDARY.md` as the canonical wording for the current support boundary and aligned the copied wording across user-facing docs and GitHub templates.
- Added first-release scope, known limitations, and release checklist documents to make the GCC 15 primary contract and GCC 13/14 compatibility path explicit.
- Added issue and pull request templates that require support-tier and trace-bundle context for release-impacting changes.
- Added three more promoted representative fixtures so the GCC 15 acceptance/snapshot gate now covers multiple syntax, type-overload, and linker cases instead of a single exemplar.
- Added `docs/policies/VERSIONING.md`, `ADR-0021`, and `ADR-0024` to separate maturity labels, artifact semver, release channels, and the GitHub public-beta release policy.
- Added `corpus/README.md` to document the harvested-trace to committed-fixture promotion flow.
- Added `docs/releases/PUBLIC-BETA-RELEASE.md` and a dedicated GitHub Release workflow so signed public-beta artifacts, control bundles, immutable release-repo bundles, and exact-pin install instructions are part of the shipped release path.

### Changed

- Promoted the repository baseline from `v1alpha` / `0.1.x` to `v1beta` / `0.2.0-beta.1`, while keeping the support boundary intentionally narrow around Linux, `x86_64-unknown-linux-musl`, GCC 15, the terminal renderer, and honest raw fallback.
- Hardened `diag_render` selection and budgeting so lead diagnostics now rank user-owned/confident/actionable groups first, family-specific cards use stable template/overload/macro/include/linker headings, CI linker output becomes explicitly `linker:`-prefixed when locations are weak, and enhanced renders always leave a `--formed-profile=raw_fallback` escape hatch.
- Refactored `diag_enrich` into separate family, headline, action-hint, and ownership modules, and added deterministic unit coverage for syntax, type/overload, template, macro/include, linker, passthrough, and unknown classification paths.
- Strengthened `diag_render` lead selection and honesty rules so user-owned high-confidence roots win by default, low-confidence cases fall back to raw compiler wording without synthesized first actions, and repeated context/note lines collapse deterministically across TTY and CI profiles.
- Wired `diag_render` profile budgets through selection, excerpts, family-specific supporting evidence, truncation, and warning suppression so default/concise/verbose/ci now share one deterministic source of truth instead of ad hoc per-file limits.
- Tightened the curated corpus gate so replay rejects fixture counts outside the beta-bar window instead of enforcing only the minimum size.
- Updated CI workflows to use pinned action/toolchain versions, corrected the Rust toolchain action ref, added rollback smoke coverage, retained gate artifacts, classify gate failures as `product` / `infrastructure` / `instrumentation`, and treat GCC 13/14 nightly runs as health indicators instead of release blockers.
- Updated the release smoke path to exercise canary-to-beta promote, beta-channel exact-pin install, and GitHub Release-ready artifact bundles instead of only local repository smoke.
- Fixed the public-beta GitHub Release workflow so runner-temp derived report and asset paths are initialized inside a runtime step, avoiding GitHub parser failures before jobs are created on tag push or manual dispatch.
- Updated the CLI to announce conservative compatibility mode when the selected backend is outside the primary GCC 15 render path.
- Tightened representative acceptance verification so promoted fixtures can require a user-owned lead location, and replay quality rates now use expectation-derived denominators instead of the full promoted set.
- Changed GCC SARIF ingest to fail open when authoritative SARIF is missing or malformed, preserving raw diagnostics while emitting `sarif_missing` / `sarif_parse_failed` trace reasons.
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
