# Contributing

## Project Baseline

`gcc-formed` is a spec-first repository with the current `v1alpha` maturity label and the `0.1.x` artifact line. User-visible behavior, config, IR semantics, and release contracts should be treated as deliberate interfaces, not incidental implementation details. See [VERSIONING.md](VERSIONING.md) for the fixed maturity / semver / channel vocabulary.

## Current Support Boundary

Keep support-boundary wording aligned with [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md).

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary enhanced-render path.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 are compatibility-only paths and may use conservative passthrough or shadow behavior instead of the primary enhanced-render path.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

## Local Prerequisites

- Rust `1.94.1`
- `x86_64-unknown-linux-musl` target via `rustup target add x86_64-unknown-linux-musl`
- Docker for the GCC 15 snapshot gate

## Required Checks Before Opening a Change

Run these from the repository root unless the change is documentation-only:

```bash
cargo xtask check
cargo xtask replay --root corpus
cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15
cargo deny check
cargo xtask hermetic-release-check --vendor-dir vendor --bin gcc-formed --target-triple x86_64-unknown-linux-musl
```

If you touch release packaging, install flows, or release metadata, also validate the relevant `cargo xtask package`, `install`, `release-publish`, `release-promote`, and `install-release` paths in a clean worktree.

## Change Policy

- Prefer behavior-preserving fixes over silent contract drift.
- If a change alters CLI surface, config or environment behavior, IR semantics, renderer wording, or release/install contract, add or supersede an ADR instead of quietly rewriting the baseline.
- If a change alters the support boundary, update `SUPPORT-BOUNDARY.md`, the copied wording in the user-facing docs, and the GitHub templates in the same change.
- Keep corpus expectations, snapshots, and docs aligned. If a promoted fixture changes, update the canonical expectation and rerun the replay and snapshot gates.
- Update `CHANGELOG.md` for user-visible changes. Keep `RELEASE-NOTES.md` aligned with the shipped baseline scope and the current maturity / artifact wording from `VERSIONING.md`.

## Corpus Workflow

- Keep the hand-authored corpus within the current beta-bar target described in [corpus/README.md](corpus/README.md): 80 to 120 fixtures while preserving the composition quota from `quality-corpus-test-gate-spec.md`.
- When a harvested trace graduates into the corpus, sanitize it first, minimize it to a bounded repro, then commit fixture metadata and GCC 15 snapshots in the same change.
- Prefer semantic expectations that catch family, fallback, provenance, and first-action regressions without overfitting transient line or quote drift.
- Use render expectation assertions such as `required_substrings` / `forbidden_substrings` when a promoted fixture needs to pin family-specific headings, omission notices, or the raw fallback escape hatch without snapshotting every line detail.

## Submission Notes

- Keep pull requests narrow and decision-complete.
- Document intentional tradeoffs and limitations in the PR description.
- When CI fails, inspect the uploaded `REPORT_ROOT/gate/gate-summary.json`, `gate-summary.md`, and `build-environment.json` artifacts before diving into raw GitHub step logs; they are the primary failure-triage entrypoint for instrumented `run:` steps and record the exact `rustc` / `cargo` / Docker / GCC environment used by the gate.
- For snapshot failures or updates, inspect `snapshot-report.json` and per-fixture `comparisons.json`; they separate `normalization_only` drift from semantic mismatches.
- `cargo xtask package` expects a clean git worktree for production artifacts; do not cut release artifacts from a dirty tree.
