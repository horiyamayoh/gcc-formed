---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current versioning, governance, and change-control rules.
do_not_use_for: Historical rollout policy or superseded support language.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current versioning, governance, and change-control rules.
> Do not use for: Historical rollout policy or superseded support language.

# Change Governance

This document operationalizes [ADR-0020](../../adr-initial-set/adr-0020-stability-promises.md) for the current stable-prep governance freeze. Use it to classify changes, decide whether an ADR update is required, and keep the pre-`1.0.0` backlog separate from post-`1.0.0` expansion work.

## Current Freeze

Until `1.0.0` ships, treat the current `v1beta` / `0.2.0-beta.N` contract as frozen by default.

- A change that touches a stable contract surface must be explicitly classified as `breaking`, `non-breaking`, or `experimental`.
- Contract changes must update the governing docs, tests, and release-facing wording in the same change.
- Post-`1.0.0` backlog items must not be silently pulled into the current support boundary.

## Stable Contract Surfaces

| Surface | Source of truth | Breaking if... | Non-breaking if... |
| --- | --- | --- | --- |
| CLI surface | `README.md`, `rendering-ux-contract-spec.md`, wrapper help, `ADR-0001`, `ADR-0019` | existing flags, subcommands, exit behavior, or introspection outputs are removed, renamed, or semantically changed | new opt-in flags or additive outputs leave existing behavior unchanged |
| Config / environment contract | `gcc-formed-vnext-change-design.md`, `CONTRIBUTING.md`, `ADR-0020` | precedence, defaults, env keys, or config meaning changes for existing users | additive config is optional and preserves current precedence and defaults |
| IR schema semantics | `diagnostic-ir-v1alpha-spec.md`, `ADR-0012` | existing fields, enums, provenance, or semantic meaning change | additive fields can be ignored by existing consumers without changing meaning |
| Renderer wording / confidence / fallback notices | `rendering-ux-contract-spec.md`, `KNOWN-LIMITATIONS.md`, `ADR-0005`, `ADR-0019` | existing family headings, confidence labels, compatibility notices, or fallback reason semantics change | wording additions preserve the existing message contract and honest fallback semantics |
| Release / install / rollback / signing contract | `packaging-runtime-operations-spec.md`, `PUBLIC-BETA-RELEASE.md`, `STABLE-RELEASE.md`, `ADR-0024`, `ADR-0025` | artifact naming, manifest meaning, install layout, channel resolution, checksum/signature verification, or rollback semantics change | additional evidence or optional tooling preserves the same bits and install contract |
| Support boundary / runbook routing | `SUPPORT-BOUNDARY.md`, `SUPPORT.md`, `docs/runbooks/`, `SECURITY.md` | supported scope, recovery promises, or public routing changes | clarifications preserve the exact support boundary and existing routing promises |

## Change Classification

### Breaking

A change is `breaking` when it alters the meaning of an existing contract surface.

Examples:

- removing or renaming an existing CLI flag, subcommand, env var, config key, IR field, or manifest field
- changing default mode selection, precedence, fallback semantics, support-tier wording, or release/install behavior for existing users
- changing renderer headings, confidence wording, rollout notices, or trace/report semantics in a way that invalidates the current contract

Required actions:

- add or supersede an ADR
- explain migration or rollout impact in the PR
- update affected user-facing docs and release notes in the same change
- record the user-visible impact in `CHANGELOG.md`

Post-`1.0.0`, a breaking change requires the next major version or a clearly versioned replacement lane. It must not ship as a silent patch/minor drift.

### Non-Breaking

A change is `non-breaking` when it is additive or behavior-preserving for existing contract surfaces.

Examples:

- internal refactors with no contract-surface change
- new optional report artifacts or extra diagnostics context that preserves existing semantics
- additive IR fields that existing consumers may ignore safely
- additional docs, tests, or observability that do not broaden support promises

Required actions:

- say why the change is non-breaking in the PR
- update docs when the behavior is user-visible
- keep corpus, snapshot, and release evidence aligned if the change affects those paths

### Experimental

A change is `experimental` only when it is explicitly opt-in and outside the shipped support boundary.

Rules:

- it must be labeled `experimental` in docs and user-facing help
- it must be disabled by default
- it must not silently replace the current stable-prep path
- it must be excluded from `SUPPORT-BOUNDARY.md` and release promises until promoted through normal ADR review

An experimental feature may change or be removed before `1.0.0`. After `1.0.0`, it still must remain opt-in until it has a reviewed promotion plan.

## Pre-1.0 Must-Have Backlog

These items are still in scope before `1.0.0`:

- keep `pr-gate`, the GCC 15 blocker slice of `nightly-gate`, and release smoke evidence green
- preserve the current support boundary, honest compatibility notices, and fail-open fallback behavior
- fix regressions in GCC 15 primary render quality, trace integrity, signing, install, rollback, and release evidence
- keep corpus quality gates, human-eval packet, fuzz packet, metrics packet, and stable-release evidence aligned with the shipped contract
- land any contract change only with the required ADR/docs/changelog/test updates

## Post-1.0 Backlog

These remain explicitly out of scope for the current frozen contract and should not be smuggled into `v1beta` or the first stable release:

- non-Linux production artifacts
- GCC 13/14 enhanced-render quality guarantees
- elimination of passthrough, shadow mode, or raw fallback
- package-manager-native distribution as the primary release path
- self-updater flows
- container-primary distribution
- Clang support
- editor integration, daemon mode, TUI surfaces, or auto-fix apply flows

## Reviewer Checklist

Use this checklist on every contract-adjacent change:

- classify the change as `breaking`, `non-breaking`, or `experimental`
- mark every affected contract surface in the PR template
- require an ADR add/supersede for breaking semantic changes
- verify README / release notes / checklist / support docs stay aligned with the changed contract
- reject changes that silently pull a post-`1.0.0` backlog item into the current support boundary
