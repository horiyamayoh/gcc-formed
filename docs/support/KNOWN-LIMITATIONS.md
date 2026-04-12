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

# Known Limitations

`gcc-formed` is currently in the `v1beta` maturity line, and the current artifact line is `0.2.0-beta.N`. The current public-beta baseline is intentionally narrow.

The exact public wording is fixed in [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md). This file summarizes the current limits and known gaps around that contract.

## Current Beta Posture

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15+` is the primary fidelity reference path.
- `GCC13-14` and `GCC9-12` are in-scope product bands, but with narrower guarantees and path-dependent capture constraints.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.
- The currently recommended build-system insertion pattern is direct `CC` / `CXX` replacement, optionally with one wrapper-owned backend launcher via `FORMED_BACKEND_LAUNCHER`, `--formed-backend-launcher`, or `[backend].launcher`.
- The checked-in interop lab is the source of truth for Make / CMake topology guidance. When the lab does not prove a chain, prefer raw `gcc` / `g++` or `--formed-mode=passthrough` rather than adding another launcher layer in front of the wrapper.

## Known Constraints

- Not every `VersionBand` currently has the same fidelity or the same raw-preservation guarantees.
- `ProcessingPath` may vary by invocation, diagnostics sink, or explicit mode request.
- `x86_64-unknown-linux-gnu` remains a compatibility smoke and exception path, not the primary shipped artifact.
- Older or unknown compiler variants may still resolve conservatively to passthrough behavior.
- Current runtime and self-check output already use the current vocabulary. The remaining limit is band-dependent fidelity and raw-preservation behavior, not legacy tier wording. Use `--formed-self-check` and [docs/support/OPERATOR-INTEROP.md](OPERATOR-INTEROP.md) for the current operator next step.
- Default TTY non-regression is a release gate, but the full path-aware enforcement work is still in flight. Regressions in color, first-screen length, noise compression, or disclosure honesty should be reported with traces.
- The checked-in interop lab covers `make -j`, `cmake --build`, one wrapper-owned backend launcher, depfile generation, response-file pass-through, and stdout-sensitive compiler probes under `eval/interop/`, but that coverage is intentionally narrow and does not prove launcher stacks in front of the wrapper or multi-launcher chains.

## Raw Fallback

Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

You should expect raw fallback when:

- the selected path is still the most conservative trustworthy option for the observed compiler band
- the invocation forces an incompatible diagnostics sink
- structured capture is unavailable or incomplete
- the renderer cannot produce a higher-confidence document than the preserved raw diagnostics

## What Is Not Guaranteed Yet

- Identical guarantees across all `VersionBand` values.
- Non-Linux production artifacts.
- Elimination of passthrough, shadow-mode-like conservative behavior, or raw fallback.
- Release-candidate or stable artifacts (`1.0.0-rc.N`, `1.0.0`).
- Stable general-availability support promises beyond the documented `v1beta` / `0.2.0-beta.N` scope.
- Backlog items reserved for post-`1.0.0` expansion; see [GOVERNANCE.md](../policies/GOVERNANCE.md).

## Bug Reports

When reporting a bug, include the selected `VersionBand`, `ProcessingPath` if known, and a trace bundle when possible. Prefer the opt-in bundle surface so the bundle stays local by default:

```bash
gcc-formed --formed-trace-bundle ...
gcc-formed --formed-trace-bundle=/secure/local/path ...
```

Attach the resulting `trace.json`, normalized IR, and preserved `stderr.raw` from the trace directory. If runtime output still shows legacy internal classification fields, attach them verbatim as evidence rather than translating them by hand. If you used a user-specified trace path, mention it explicitly and note whether the bundle was redaction-reviewed before sharing.
