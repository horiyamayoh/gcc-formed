# Release Checklist

This checklist defines the minimum bar for shipping artifacts from the current `v1alpha` / `0.1.x` baseline and for deciding whether the project is ready to advance to `v1beta` / `0.2.0-beta.N`.

## Versioning Contract

- Current maturity label: `v1alpha`
- Current artifact semver line: `0.1.x`
- Planned public beta line: `0.2.0-beta.N`
- Planned release-candidate line: `1.0.0-rc.N`
- Planned stable line: `1.0.0`
- Release repository channels such as `canary`, `beta`, and `stable` are distribution pointers, not maturity labels.

## Release Blockers

- `pr-gate` is green on `main`.
- The GCC 15 blocker portion of `nightly-gate` is green.
- Representative acceptance replay is green and the report artifacts are attached.
- Representative GCC 15 snapshot check is green and the report artifacts are attached.
- Signed package generation, install, rollback/uninstall, and install-release smoke all pass.
- Release artifacts include `release-provenance.json`.

## Current Alpha Baseline Scope

- Linux first.
- `x86_64-unknown-linux-musl` only as the primary production artifact.
- GCC 15 only as the primary enhanced-render path.
- Terminal renderer only as the primary surface.

## Explicit Non-Goals

- Do not label `0.1.x` artifacts as `v1beta`, `1.0.0-rc.N`, or `1.0.0 stable`.
- Do not claim enhanced render guarantees for GCC 13/14.
- Do not expand primary support to non-Linux artifacts.
- Do not claim that raw fallback has been eliminated.

## Release Notes Gate

- README states the current alpha-baseline scope in one screen.
- README links to `VERSIONING.md` and distinguishes maturity labels from artifact semver.
- `RELEASE-NOTES.md` calls out compatibility paths and raw fallback semantics.
- `KNOWN-LIMITATIONS.md` is linked from README and release notes.

## Artifact Retention

- Replay report includes normalized IR, preserved raw stderr, and rendered output.
- Snapshot report includes expected/actual artifacts for the representative fixtures.
- Release smoke retains `manifest.json`, package/install JSON output, and resolve/install-release JSON output.
- Release smoke retains `release-provenance.json` alongside signing material and build metadata.
- Signing key rotation / revoke / emergency re-sign follows `SIGNING-KEY-OPERATIONS.md`.
