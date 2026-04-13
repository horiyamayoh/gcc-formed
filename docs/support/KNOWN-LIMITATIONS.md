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
- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.
- `VersionBand` and `ProcessingPath` are observability metadata; they do not justify weaker value claims inside `GCC 9-15`.
- `SupportLevel` appears in self-check and public JSON as the machine labels `in_scope` and `passthrough_only`; those labels describe applicability, not a public hierarchy inside the shared `GCC 9-15` contract.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.
- The currently recommended build-system insertion pattern is direct `CC` / `CXX` replacement, optionally with one wrapper-owned backend launcher via `FORMED_BACKEND_LAUNCHER`, `--formed-backend-launcher`, or `[backend].launcher`.
- The checked-in interop lab is the source of truth for Make / CMake topology guidance. When the lab does not prove a chain, prefer raw `gcc` / `g++` or `--formed-mode=passthrough` rather than adding another launcher layer in front of the wrapper.

## Known Constraints

- In-scope bands share one public contract, but capture mechanisms, same-run raw-preservation details, and default processing paths still differ by capability.
- `ProcessingPath` may vary by invocation, diagnostics sink, or explicit mode request.
- The C semantics / systems / toolchain backfills, the C++ core / overload / template pack, and the modern C++ applicability pack are still a mix of representative evidence and explicit applicability inventory. When a family has no checked-in GCC13-14 or GCC9-12 representative replay cell for a given path yet, the repo records that gap in `meta.yaml` under `older_band_applicability`; that inventory is not stop-ship matrix coverage.
- `openmp`, `analyzer`, historical path/toolchain residue, several C++ core families, and some modern C++ families still do not claim every older-band emitted representative cell. `overload` and `template` already use older-band `single_sink_structured` representative proof, and the modern C++ pack now keeps `gcc13_14` representative anchors plus `gcc15` companion snapshots, but missing GCC9-12 cells remain explicit inventory rather than weaker guarantees. When these families emit on in-scope bands, they still follow the shared headline / first-action / disclosure contract rather than a lower-tier older-band variant.
- `three_way_comparison`, `concepts_constraints`, `coroutine`, `module_import`, and `ranges_views` are version-sensitive within `GCC9-12`; `module_import` additionally lacks front-end availability on GCC9-GCC10. Their `older_band_applicability` notes intentionally record that scope instead of inferring band-wide support from the newer-band anchors.
- `x86_64-unknown-linux-gnu` remains a compatibility smoke and exception path, not the primary shipped artifact.
- `GCC16+`, older compilers outside `GCC 9-15`, and unknown gcc-like variants may still resolve conservatively to passthrough behavior.
- Current runtime and self-check output already use the current vocabulary. The remaining limit is capability-dependent capture behavior, not a public hierarchy between in-scope bands. Use `--formed-self-check` and [docs/support/OPERATOR-INTEROP.md](OPERATOR-INTEROP.md) for the current operator next step.
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

- Claims that `GCC16+` or unknown gcc-like compilers are already inside the `GCC 9-15` contract.
- Perfect parity across every diagnostic family and every capability path.
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

Attach the resulting `trace.json`, normalized IR, and preserved `stderr.raw` from the trace directory. If you are working from older artifacts that still show legacy internal classification fields, attach them verbatim as evidence rather than translating them by hand. If you used a user-specified trace path, mention it explicitly and note whether the bundle was redaction-reviewed before sharing.
