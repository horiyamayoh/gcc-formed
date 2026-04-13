---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Accepted design decisions that constrain implementation.
do_not_use_for: Historical superseded policy or workflow detail outside the decision.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Accepted design decisions that constrain implementation.
> Do not use for: Historical superseded policy or workflow detail outside the decision.

# ADR-0036: Presentation V2 becomes the beta default while preserving explicit rollback presets

- **Status**: Accepted
- **Date**: 2026-04-14

## Context

ADR-0034 fixed the Presentation V2 grammar, session model, and rollout order. The repo then landed the v2 preset loader, semantic-shape routing, layout externalization, family-fact hardening, visible-root invariants, and checked-in PV2 contract fixtures.

At that point the remaining drift was not the grammar itself but the shipped default selection. The beta contract still reported `subject_blocks_v1` as the no-config default even though the reviewed subject-first policy had already become the intended current path.

Changing the no-config preset is a breaking config / renderer contract change under `GOVERNANCE.md`, because existing users will see a different default render policy unless they pin a preset explicitly.

## Decision

- The no-config beta terminal default is `subject_blocks_v2`.
- `subject_blocks_v1` remains available as an explicit rollback preset for users who want the previous beta default during migration review.
- `legacy_v1` remains available as an explicit compatibility / rollback preset for the legacy wording and session behavior.
- If an explicit built-in preset is selected and an external `presentation_file` overlay cannot be loaded, fail-open must return to that built-in preset rather than silently switching to another preset.
- Unknown or unavailable preset IDs still fail open to the current built-in default.
- Public machine-readable export remains presentation-independent; default promotion does not change the public JSON contract.
- Internal review artifacts may keep preset-scoped snapshots such as `subject_blocks_v2/render.presentation.json` and `subject_blocks_v1/render.presentation.json`, but those artifacts are not public contract surfaces.

## Consequences

- README, support-boundary wording, rendering spec, release notes, and changelog must move in lockstep with the promoted default.
- Breaking-change rollout notes must tell users how to pin `subject_blocks_v1` or `legacy_v1` explicitly when they need rollback during migration.
- Tests and checked-in default render evidence must treat `subject_blocks_v2` as the no-config baseline while continuing to protect the explicit rollback presets.

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0019`, `ADR-0020`, `ADR-0030`, `ADR-0034`

## Source Specs

- `../README.md`
- `../docs/support/SUPPORT-BOUNDARY.md`
- `../docs/specs/rendering-ux-contract-spec.md`
- `../docs/specs/public-machine-readable-diagnostic-surface-spec.md`
