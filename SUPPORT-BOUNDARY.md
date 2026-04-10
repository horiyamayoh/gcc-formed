# Support Boundary

This document is the canonical wording for the current `v1beta` / `0.2.0-beta.N` vNext support posture.  
Keep `README.md`, release notes, support docs, contribution docs, and GitHub templates aligned with this wording.

---

## 1. Canonical vocabulary

### VersionBand

Compiler band used to reason about product scope.

- `GCC15+`
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

- `Preview`
- `Experimental`
- `PassthroughOnly`

### RawPreservationLevel

How much native / raw compiler output is preserved in the same run.

- `NativeAndStructuredSameRun`
- `StructuredOnlySameRun`
- `RawOnly`

---

## 2. Current `v1beta` / `0.2.0-beta.N` support boundary

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15+`, `GCC13-14`, and `GCC9-12` are all in-scope product bands.
- `GCC15+` is the primary fidelity reference path.
- `GCC13-14` and `GCC9-12` are product paths with narrower guarantees and different capture constraints.
- `GCC13-14` is a first-class beta product band inside that narrower contract.
- They are part of the product surface, not merely incidental fallback behavior.
- `ProcessingPath` and `RawPreservationLevel` may differ by band and by invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.
- The wrapper must not knowingly ship a default TTY experience that is less legible than native GCC without either correcting the output or conservatively falling back / explicitly disclosing the limitation.

---

## 3. Current beta matrix

| VersionBand | Typical ProcessingPath | RawPreservationLevel | SupportLevel | Current expectation |
|---|---|---|---|---|
| `GCC15+` | `DualSinkStructured` | `NativeAndStructuredSameRun` | `Preview` | Highest-fidelity reference path |
| `GCC13-14` | `NativeTextCapture`, `SingleSinkStructured` | path-dependent; do not assume same-run native+structured | `Experimental` | First-class beta path with narrower fidelity than GCC15+ |
| `GCC9-12` | `SingleSinkStructured` (JSON), `NativeTextCapture` | path-dependent; do not assume same-run native+structured | `Experimental` | Wins on simple / type / linker / basic-template cases |
| `Unknown` | `Passthrough` | `RawOnly` | `PassthroughOnly` | Do not break the build or hide facts |

### Interpretive notes

- “first-class product band” means: present in specs, tests, issue taxonomy, quality gates, roadmap, and corpus tagging.
- It does **not** mean that all bands have identical fidelity or identical raw-preservation guarantees.
- If a run resolves to `Passthrough`, that is still a valid shipped behavior when it is the most trustworthy choice.

---

## 4. Release-gate language

A beta or release-candidate build must be held if any of the following are true on representative fixtures:

1. default TTY output loses useful color, pointing, or severity signaling relative to native GCC without compensating user benefit
2. default TTY output becomes substantially longer than native GCC without improving first-fix behavior
3. template / overload / stdlib noise is not compressed enough to justify wrapping
4. the wrapper hides provenance, confidence, or compiler-owned facts
5. fallback behavior becomes opaque or misleading

---

## 5. Explicitly outside the current boundary

- Non-Linux production artifacts
- Claims that all VersionBands have identical guarantees
- Claims that every GCC diagnostic family is already improved
- Elimination of passthrough or raw fallback
- Stable / GA promises beyond what this document explicitly states

---

## 6. Required wording alignment

The following files must stay aligned with this document:

- `README.md`
- release notes
- bug report template
- pull request template
- support runbooks
- contribution docs
- any user-facing “current support” wording

If wording changes here, update those surfaces in the same change.
