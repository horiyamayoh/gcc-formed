# Release Checklist

This checklist defines the minimum bar for a first public release candidate.

## Release Blockers

- `pr-gate` is green on `main`.
- The GCC 15 blocker portion of `nightly-gate` is green.
- Representative acceptance replay is green and the report artifacts are attached.
- Representative GCC 15 snapshot check is green and the report artifacts are attached.
- Signed package generation, install, rollback/uninstall, and install-release smoke all pass.

## First-Release Scope

- Linux first.
- `x86_64-unknown-linux-musl` only as the primary production artifact.
- GCC 15 only as the primary enhanced-render path.
- Terminal renderer only as the primary surface.

## Explicit Non-Goals

- Do not claim enhanced render guarantees for GCC 13/14.
- Do not expand primary support to non-Linux artifacts.
- Do not claim that raw fallback has been eliminated.

## Release Notes Gate

- README states the first-release scope in one screen.
- `RELEASE-NOTES.md` calls out compatibility paths and raw fallback semantics.
- `KNOWN-LIMITATIONS.md` is linked from README and release notes.

## Artifact Retention

- Replay report includes normalized IR, preserved raw stderr, and rendered output.
- Snapshot report includes expected/actual artifacts for the representative fixtures.
- Release smoke retains `manifest.json`, package/install JSON output, and resolve/install-release JSON output.
