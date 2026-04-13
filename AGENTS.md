---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: agent
use_for: Shared agent entrypoint and authority order for all coding agents.
do_not_use_for: Historical provenance or superseded policy.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> This file is the shared entry point for **all** coding agents (Claude, Codex, etc.).
> Agent-specific execution contracts live in dedicated files (`Codex.md`, `CLAUDE.md`, etc.).

# Agent Entry Point — Shared Contract

## Authority order

Use these sources in this order:

1. `README.md`
2. `docs/support/SUPPORT-BOUNDARY.md`
3. `docs/process/EXECUTION-MODEL.md`
4. `adr-initial-set/README.md`
5. current-authority specs under `docs/specs/`

Only documents marked `doc_role: current-authority` may drive implementation decisions.

## Repository truths

- This repository is **spec-first** and **contract-sensitive**.
- Use current vocabulary: `VersionBand`, `CapabilityProfile`, `ProcessingPath`, `SupportLevel`.
- The public machine-readable contract is `--formed-public-json=` and `docs/specs/public-machine-readable-diagnostic-surface-spec.md`.
- Do **not** treat terminal text, trace bundles, or internal IR snapshots as the public automation contract unless the task explicitly asks for them.
- Default TTY non-regression is stop-ship. If the wrapper becomes less legible than native GCC and does not clearly compensate, that is a failure, not polish debt.
- Raw fallback is a shipped contract and must remain honest.

## Public machine surface

For machine-readable diagnostic consumption, prefer `--formed-public-json=<path>` and the public export contract in `docs/specs/public-machine-readable-diagnostic-surface-spec.md`.

Do not treat terminal text, trace bundles, or internal IR snapshots as the public automation contract unless the task explicitly asks for those internal artifacts.

## Scope rules

- Keep `1 issue = 1 PR = 1 primary purpose`.
- Prefer the smallest complete diff.
- Do not perform incidental refactors, drive-by renames, or broad moves unless the task explicitly requires them.

## Change classification

Classify every contract-adjacent change using `docs/policies/GOVERNANCE.md`:

- `breaking`
- `non-breaking`
- `experimental`

Rules:

- If you change CLI surface, config/env behavior, renderer wording, support boundary, public export semantics, IR semantics, install/release/rollback/signing behavior, or other stable contract surfaces, update the governing docs in the same change.
- If the change is user-visible, update `CHANGELOG.md`.
- If the semantic change is breaking, add or supersede the relevant ADR in the same change.
- Experimental work must stay opt-in, disabled by default, and outside the shipped support boundary until promoted.

## Required synchronization

Keep these surfaces aligned when touched:

- Support boundary changes: `docs/support/SUPPORT-BOUNDARY.md`, copied user-facing wording, and relevant GitHub templates.
- Public machine-readable export changes: runtime behavior plus `docs/specs/public-machine-readable-diagnostic-surface-spec.md`.
- Release / install / rollback changes: docs, xtask behavior, workflows, and related ADRs together.
- Corpus / render expectation changes: fixtures, replay evidence, snapshot evidence, and docs together.
- Public landing copy changes: `README.md` plus `docs/support/PUBLIC-SURFACE.md`.

## Ignore by default

- `docs/history/`
- historical bundles under `docs/archive/`
- superseded ADRs under `adr-initial-set/superseded/`
- historical or legacy planning material

Read those only when you need provenance or migration context.

## Conflict rule

- Newer `current-authority` beats older `reference-only` or `history-only`.
- `reference-only` may explain context, but it cannot override current contract docs.
- `history-only` exists for provenance and must not be treated as current product truth.

## Current vocabulary

- Historical: `SupportTier`
- Current: `VersionBand`, `CapabilityProfile`, `ProcessingPath`, `SupportLevel`

If runtime or old docs still mention `SupportTier`, translate it into the current vocabulary before making decisions.

## Explicit warning

If a document says `GCC15-first`, `compatibility-only`, or otherwise treats GCC 15 as the only real product path, and that document is not marked `current-authority`, do not use it as the current architecture.

The repo was historically GCC15-first. The current vNext direction is multi-band across `GCC15+`, `GCC13-14`, and `GCC9-12`.
