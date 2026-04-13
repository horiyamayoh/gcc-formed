---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: agent
use_for: Current agent entrypoint and authority order.
do_not_use_for: Historical provenance or superseded policy.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current agent entrypoint and authority order.
> Do not use for: Historical provenance or superseded policy.

# Agent Entry Point

Use this file as the default starting point for AI coding agents working in this repository.

## Current Authority Order

1. `README.md`
2. `docs/support/SUPPORT-BOUNDARY.md`
3. `docs/process/EXECUTION-MODEL.md`
4. `adr-initial-set/README.md`
5. current specs under `docs/specs/`

Only documents marked as `doc_role: current-authority` may drive implementation decisions.

## Public Machine Surface

For machine-readable diagnostic consumption, prefer `--formed-public-json=<path>` and the public export contract in `docs/specs/public-machine-readable-diagnostic-surface-spec.md`.

Do not treat terminal text, trace bundles, or internal IR snapshots as the public automation contract unless the task explicitly asks for those internal artifacts.

## Ignore By Default

- `docs/history/`
- historical bundles under `docs/archive/`
- superseded ADRs under `adr-initial-set/superseded/`
- historical or legacy planning material

Read those only when you need provenance or migration context.

## Conflict Rule

- Newer `current-authority` beats older `reference-only` or `history-only`.
- `reference-only` may explain context, but it cannot override current contract docs.
- `history-only` exists for provenance and must not be treated as current product truth.

## Current Vocabulary

- Historical: `SupportTier`
- Current: `VersionBand`, `CapabilityProfile`, `ProcessingPath`, `SupportLevel`

If runtime or old docs still mention `SupportTier`, translate it into the current vocabulary before making decisions.

## Explicit Warning

If a document says `GCC15-first`, `compatibility-only`, or otherwise treats GCC 15 as the only real product path, and that document is not marked `current-authority`, do not use it as the current architecture.

The repo was historically GCC15-first. The current vNext direction is multi-band across `GCC15+`, `GCC13-14`, and `GCC9-12`.
