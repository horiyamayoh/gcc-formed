# Release Checklist

This checklist defines the minimum bar for shipping artifacts from the current `v1beta` / `0.2.0-beta.N` baseline and for deciding whether the project is ready to advance to `v1.0.0-rc` / `1.0.0-rc.N`.

## Versioning Contract

- Current maturity label: `v1beta`
- Current artifact semver line: `0.2.0-beta.N`
- Current public beta artifact: `0.2.0-beta.1`
- Planned release-candidate line: `1.0.0-rc.N`
- Planned stable line: `1.0.0`
- Release repository channels such as `canary`, `beta`, and `stable` are distribution pointers, not maturity labels.
- Canonical support-boundary wording lives in `SUPPORT-BOUNDARY.md`.

## Release Blockers

- `pr-gate` is green on `main`.
- The GCC 15 blocker portion of `nightly-gate` is green.
- Representative acceptance replay is green and the report artifacts are attached.
- Representative GCC 15 snapshot check is green and the report artifacts are attached.
- Signed package generation, install, rollback/uninstall, and install-release smoke all pass.
- The public GitHub Release exists and includes the minimum beta asset set.
- Release artifacts include `release-provenance.json`.
- Advancing to `1.0.0-rc.N` additionally requires a fresh `cargo xtask rc-gate --report-dir ...` run with no blockers and attached `rc-gate-report.json` / `rc-gate-summary.md`.
- Advancing from a stable candidate to `1.0.0` additionally requires a fresh `cargo xtask stable-release --report-dir ...` run or the `release-stable.yml` workflow, with attached `stable-release-report.json`, `stable-release-summary.md`, `promotion-evidence.json`, and `rollback-drill.json`.
- The RC metrics packet (`metrics-report.json` + `metrics-manual-eval.json`) is attached and current.
- The RC fuzz packet (`fuzz-smoke-report.json` + `fuzz-evidence.json`) is attached and current.
- The RC human-eval bundle (`human-eval/README.md`, `expert-review-sheet.csv`, `task-study-sheet.csv`, template JSONs, and selected fixture artifacts) is attached and current.

## Current Beta Support Boundary

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary enhanced-render path.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 are compatibility-only paths and may use conservative passthrough or shadow behavior instead of the primary enhanced-render path.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

## Explicit Non-Goals

- Do not label `0.2.0-beta.N` artifacts as `1.0.0-rc.N` or `1.0.0 stable`.
- Do not describe GCC 13/14 as primary enhanced-render coverage.
- Do not expand primary support to non-Linux artifacts.
- Do not claim that raw fallback has been eliminated.

## Release Notes Gate

- README states the current beta-baseline scope in one screen.
- README links to `PUBLIC-BETA-RELEASE.md` for install / rollback / exact-pin instructions.
- `SUPPORT-BOUNDARY.md` exists and matches the wording reused in README, release notes, limitations, security, and contributing docs.
- README links to `VERSIONING.md` and distinguishes maturity labels from artifact semver.
- `RELEASE-NOTES.md` calls out compatibility paths and raw fallback semantics.
- `KNOWN-LIMITATIONS.md` is linked from README and release notes.
- `STABLE-RELEASE.md` exists and matches the workflow/xtask stable cut contract.
- `SUPPORT.md` and the runbooks under `docs/runbooks/` exist, and the public bug template links to them.
- The GitHub Release body links `SUPPORT-BOUNDARY.md`, `KNOWN-LIMITATIONS.md`, and `PUBLIC-BETA-RELEASE.md`.

## Artifact Retention

- Replay report includes normalized IR, preserved raw stderr, and rendered output.
- Snapshot report includes expected/actual artifacts for the representative fixtures.
- Release smoke retains `manifest.json`, package/install JSON output, and resolve/install-release JSON output.
- Release smoke retains `release-provenance.json` alongside signing material and build metadata.
- Stable release smoke retains `stable-release-report.json`, `stable-release-summary.md`, `promotion-evidence.json`, and `rollback-drill.json`, and the rollback drill shows one `current` symlink switch.
- RC gate retains `replay-report.json`, `bench-smoke-report.json`, `deterministic-replay-report.json`, `rollout-matrix-report.json`, `human-eval/`, `fuzz-smoke-report.json`, `fuzz-evidence.json`, `metrics-report.json`, and the normalized manual evidence JSON files.
- The public GitHub Release ships `primary`, `debug`, and `source` archives, the full control bundle, the immutable release-repo bundle, `manifest.json`, `build-info.txt`, `SHA256SUMS`, `SHA256SUMS.sig`, and `release-provenance.json`.
- Signing key rotation / revoke / emergency re-sign follows `SIGNING-KEY-OPERATIONS.md`.
