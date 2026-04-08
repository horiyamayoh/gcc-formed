# Contributing

## Project Baseline

`gcc-formed` is a spec-first repository with the current `v1alpha` maturity label and the `0.1.x` artifact line. User-visible behavior, config, IR semantics, and release contracts should be treated as deliberate interfaces, not incidental implementation details. See [VERSIONING.md](VERSIONING.md) for the fixed maturity / semver / channel vocabulary.

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
- Keep corpus expectations, snapshots, and docs aligned. If a promoted fixture changes, update the canonical expectation and rerun the replay and snapshot gates.
- Update `CHANGELOG.md` for user-visible changes. Keep `RELEASE-NOTES.md` aligned with the shipped baseline scope and the current maturity / artifact wording from `VERSIONING.md`.

## Submission Notes

- Keep pull requests narrow and decision-complete.
- Document intentional tradeoffs and limitations in the PR description.
- When CI fails, inspect the uploaded `REPORT_ROOT/gate/gate-summary.json` and `gate-summary.md` artifacts before diving into raw GitHub step logs; they are the primary failure-triage entrypoint for instrumented `run:` steps.
- For snapshot failures or updates, inspect `snapshot-report.json` and per-fixture `comparisons.json`; they separate `normalization_only` drift from semantic mismatches.
- `cargo xtask package` expects a clean git worktree for production artifacts; do not cut release artifacts from a dirty tree.
