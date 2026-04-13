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
- The `nightly-gate` blocker portion is green across the current multi-band matrix.
- The current required diagnostic matrix evidence lanes are `gcc:12`, `gcc:13`, and `gcc:15`.
- `gcc:14` remains additional nightly / periodic evidence inside `gcc13_14`.
- The release-only `gcc15` lane is limited to packaging / signing / install smoke and does not redefine diagnostic coverage.
- Representative acceptance replay is green and the report artifacts are attached.
- Representative matrix snapshot check is green and the report artifacts are attached.
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
- The terminal renderer is the primary user-facing surface.
- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.
- `VersionBand` and `ProcessingPath` remain observability metadata; they do not encode unequal user value inside `GCC 9-15`.
- `GCC16+`, `<=8`, and unknown gcc-like compilers are `PassthroughOnly` until separately evidenced.
- Internal capture mechanisms and raw-preservation details may differ by capability and invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.
- Use the exact canonical wording in [docs/support/SUPPORT-BOUNDARY.md](../support/SUPPORT-BOUNDARY.md) for release notes, release bodies, README summaries, and templates.

## Explicit Non-Goals

- Do not label `0.2.0-beta.N` artifacts as `1.0.0-rc.N` or `1.0.0 stable`.
- Do not claim that `GCC16+` or unknown gcc-like compilers are already inside the `GCC 9-15` contract.
- Do not claim that every diagnostic family or capability path already has identical evidence across the in-scope bands.
- Do not widen the support boundary beyond `SUPPORT-BOUNDARY.md`.
- Do not expand production claims to non-Linux artifacts.
- Do not claim that raw fallback has been eliminated.

## Governance Freeze

- [GOVERNANCE.md](../policies/GOVERNANCE.md) exists and stays aligned with `ADR-0020` and `.github/pull_request_template.md`.
- Contract-affecting changes since the last artifact are classified as `breaking`, `non-breaking`, or `experimental`.
- `breaking` changes carry an ADR update/supersede plus migration or rollout notes.
- `experimental` changes remain opt-in and outside `SUPPORT-BOUNDARY.md` and shipped release promises.
- No post-`1.0.0` backlog item is silently promoted into the current support boundary.

## Release Notes Gate

- README states the current beta-baseline scope in one screen.
- README links to `PUBLIC-BETA-RELEASE.md` for install / rollback / exact-pin instructions.
- `SUPPORT-BOUNDARY.md` exists and remains the canonical wording source for README summary text, release notes, limitations, security, and contributing docs.
- `PUBLIC-SURFACE.md` exists and remains the canonical source for repo landing metadata, README top copy, and generated GitHub Release body inputs.
- README links to [VERSIONING.md](../policies/VERSIONING.md) and distinguishes maturity labels from artifact semver.
- README links [GOVERNANCE.md](../policies/GOVERNANCE.md), and the governance freeze wording is consistent with `ADR-0020`, contributing guidance, and the PR template.
- `RELEASE-NOTES.md` calls out current `VersionBand` posture and raw fallback semantics.
- `KNOWN-LIMITATIONS.md` is linked from README and release notes.
- `STABLE-RELEASE.md` exists and matches the workflow/xtask stable cut contract.
- `SUPPORT.md` and the runbooks under `docs/runbooks/` exist, and the public bug template links to them.
- The GitHub Release body is generated from `ci/public_surface.py` and links `SUPPORT-BOUNDARY.md`, `KNOWN-LIMITATIONS.md`, and the current release runbook.

## Artifact Retention

- Replay report includes normalized IR, preserved raw stderr, and rendered output.
- Snapshot report includes expected/actual artifacts for the representative fixtures.
- Release smoke retains `manifest.json`, package/install JSON output, and resolve/install-release JSON output.
- Release smoke retains `release-provenance.json` alongside signing material and build metadata.
- Stable release smoke retains `stable-release-report.json`, `stable-release-summary.md`, `promotion-evidence.json`, and `rollback-drill.json`, and the rollback drill shows one `current` symlink switch.
- RC gate retains `replay-report.json`, `bench-smoke-report.json`, `deterministic-replay-report.json`, `rollout-matrix-report.json`, `human-eval/`, `fuzz-smoke-report.json`, `fuzz-evidence.json`, `metrics-report.json`, and the normalized manual evidence JSON files.
- The public GitHub Release ships `primary`, `debug`, and `source` archives, the full control bundle, the immutable release-repo bundle, `manifest.json`, `build-info.txt`, `SHA256SUMS`, `SHA256SUMS.sig`, and `release-provenance.json`.
- Signing key rotation / revoke / emergency re-sign follows `SIGNING-KEY-OPERATIONS.md`.
