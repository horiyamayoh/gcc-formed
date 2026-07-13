---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current public support wording and support boundaries.
do_not_use_for: Historical support claims or superseded rollout posture.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current public support wording and support boundaries.
> Do not use for: Historical support claims or superseded rollout posture.

# Support Boundary

This document is the canonical wording for the current `v1.0.0-rc` / `1.0.0-rc.N` support posture.
Keep `README.md`, release notes, support docs, contribution docs, and GitHub templates aligned with this wording.

---

## 1. Canonical vocabulary

### RepairUnit and observable defect

`RepairUnit` is the product identity for one visible diagnostic item: evidence from the current invocation resolved by the same minimal edit, or evidence kept separate because independence cannot be disproved. An `observable defect` is an independently actionable defect distinguishable from evidence GCC emitted in that invocation. The product does not claim defects the compiler did not observe. Unknown family remains in scope and visible; family completeness is not a correctness metric.

### VersionBand

Compiler band used to reason about product scope.

- `GCC16+`
- `GCC15`
- `GCC13-14`
- `GCC9-12`
- `Unknown`

### ProcessingPath

Resolved execution path used by the wrapper.

- `DualSinkStructured`
- `SingleSinkStructured`
- `NativeTextCapture`
- `Passthrough`

### SupportLevel

Public quality claim for the current artifact line.

- `InScope`
- `PassthroughOnly`

### RawPreservationLevel

How much native / raw compiler output is preserved in the same run.

- `NativeAndStructuredSameRun`
- `StructuredOnlySameRun`
- `RawOnly`

---

## 2. Current `v1.0.0-rc` / `1.0.0-rc.N` support boundary

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.
- `VersionBand` and `ProcessingPath` remain observability metadata; they do not encode unequal user value inside `GCC 9-15`.
- `GCC16+`, `<=8`, and unknown gcc-like compilers are `PassthroughOnly` until separately evidenced.
- Internal capture mechanisms and raw-preservation details may differ by capability and invocation.
- `repair_units_hybrid_v2` is the qualified RC default terminal preset. `subject_blocks_v2` is the immediate rollback preset; `subject_blocks_v1` and `legacy_v1` remain compatibility presets. Raw and explain remain one-operation disclosure paths.
- `cascade.max_expanded_independent_roots` remains a deprecated compatibility knob; visible-root behavior belongs to presentation/session policy, not cascade semantics.
- Representative corpus may carry review-only `subject_blocks_v2/render.presentation.json` or `subject_blocks_v1/render.presentation.json` artifacts, but those artifacts are internal and not part of the public machine-readable surface.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.
- The wrapper must not knowingly ship a default TTY experience that is less legible than native GCC without either correcting the output or conservatively falling back / explicitly disclosing the limitation.
- Release qualification uses one preregistered, sealed, single-agent actual patch/build study plus deterministic human-readable contract checks. It does not claim that a human behavioral study passed or that human edit latency or preference improved.

---

## 3. Current RC matrix

| VersionBand | Typical ProcessingPath | RawPreservationLevel | SupportLevel | Current expectation |
|---|---|---|---|---|
| `GCC15` | `DualSinkStructured` | `NativeAndStructuredSameRun` | `InScope` | Shared in-scope contract; this row documents one representative capability shape, not a stronger public value claim |
| `GCC13-14` | `NativeTextCapture`, `SingleSinkStructured` | path-dependent; do not assume same-run native+structured | `InScope` | Shared in-scope contract; this row documents representative capability shapes, not a weaker product tier |
| `GCC9-12` | `SingleSinkStructured` (JSON), `NativeTextCapture` | path-dependent; do not assume same-run native+structured | `InScope` | Shared in-scope contract; this row documents representative capability shapes, not a weaker product tier |
| `GCC16+` | `Passthrough` | `RawOnly` | `PassthroughOnly` | Outside the current `GCC 9-15` contract until separately evidenced |
| `Unknown` | `Passthrough` | `RawOnly` | `PassthroughOnly` | Do not break the build or hide facts |

### Interpretive notes

- “shared in-scope public contract” means: present in specs, tests, issue taxonomy, quality gates, roadmap, and corpus tagging with one public value claim across `GCC 9-15`.
- Representative corpus / replay gates must track `GCC15/DualSinkStructured`, `GCC13-14/NativeTextCapture`, `GCC13-14/SingleSinkStructured`, `GCC9-12/NativeTextCapture`, and `GCC9-12/SingleSinkStructured` separately as capability coverage, not as unequal product tiers.
- Runtime disclosure and machine-readable surfaces must derive `default_processing_path` and `allowed_processing_paths` from resolved capability facts. `VersionBand` constrains scope, but it must not silently stand in for the full path set of an observed backend.
- Capture mechanisms and same-run raw-preservation guarantees may differ by capability profile even when the public contract is shared.
- If a run resolves to `Passthrough`, that is still a valid shipped behavior when it is the most trustworthy choice.

---

## 4. Release-gate language

A beta or release-candidate build must be held if any of the following are true on representative fixtures:

1. default TTY output loses useful color, pointing, or severity signaling relative to native GCC without compensating user benefit
2. default TTY output becomes substantially longer than native GCC without improving first-fix behavior
3. template / overload / stdlib noise is not compressed enough to justify wrapping
4. the wrapper hides provenance, confidence, or compiler-owned facts
5. fallback behavior becomes opaque or misleading
6. the pinned single-agent qualification is missing, inconclusive, hash-inconsistent, or outside its preregistered safety / utility / efficiency margins

---

## 5. Explicitly outside the current boundary

- Non-Linux production artifacts
- Claims that `GCC16+` or unknown gcc-like compilers are already in the same public contract as `GCC 9-15`
- Claims that every GCC diagnostic family is already improved
- Elimination of passthrough or raw fallback
- Stable / GA promises beyond what this document explicitly states

---

## 6. Required wording alignment

The following files must stay aligned with this document:

- `README.md`
- `docs/support/PUBLIC-SURFACE.md`
- release notes
- generated GitHub Release body
- bug report template
- pull request template
- support runbooks
- contribution docs
- GitHub repo landing description / website / topics / README top copy
- any user-facing “current support” wording

If wording changes here, update those surfaces in the same change.
