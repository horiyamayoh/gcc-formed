# Versioning Policy

This document fixes the naming contract for `gcc-formed` so maturity labels, artifact versions, and release channels are not conflated.

## Current Baseline

| Axis | Current value | Meaning |
| --- | --- | --- |
| Maturity label | `v1beta` | Current product maturity and support posture |
| Artifact semver line | `0.2.0-beta.N` | Current public-beta artifact series |
| Current artifact in the line | `0.2.0-beta.1` | First public-beta artifact |
| General-availability stable release | Not available | `1.0.0` has not shipped |

## Fixed Progression

| Product stage | Maturity label | Artifact semver policy |
| --- | --- | --- |
| Alpha baseline | `v1alpha` | `0.1.x` |
| Public beta | `v1beta` | `0.2.0-beta.N` |
| Release candidate | `v1.0.0-rc` | `1.0.0-rc.N` |
| Stable release | `v1.0.0 stable` | `1.0.0` |

## Release Channels Are Separate

Repository channels such as `canary`, `beta`, and `stable` are distribution pointers. They are not maturity labels and they do not override artifact semver.

Examples:

- `0.2.0-beta.1` may be published to a `beta` channel inside a release repository while the product maturity remains `v1beta`.
- `0.2.0-beta.3` is still a beta artifact even if it is the current `stable` channel target for an internal rollout.

## Wording Rules

- Use maturity labels when describing support posture or lifecycle state.
- Use artifact semver when describing a concrete release, package, rollback target, or changelog entry.
- Prefer wording such as "artifact `0.2.0-beta.1` in the `v1beta` maturity line".
- Do not use `v0.2.0-beta.1` as a maturity label.
- Archive names, tags, and install paths may embed a `v` prefix for readability, such as `gcc-formed-v0.2.0-beta.1-linux-x86_64-musl.tar.gz`; that prefix does not change the underlying semver.

## Current Reader Guidance

- `README.md` should state both the current maturity label and current artifact line.
- `CHANGELOG.md` and `RELEASE-NOTES.md` should use artifact semver headings and call out the current maturity line in prose.
- `SECURITY.md`, `KNOWN-LIMITATIONS.md`, `RELEASE-CHECKLIST.md`, and `PUBLIC-BETA-RELEASE.md` should describe support and guarantees using the same vocabulary.
- `GOVERNANCE.md` should define which contract changes are `breaking`, `non-breaking`, or `experimental`, and which backlog remains post-`1.0.0` only.

## Authority

This summary is governed by [ADR-0021](adr-initial-set/adr-0021-release-maturity-labels-and-artifact-semver-policy.md) and the broader change-control policy in [GOVERNANCE.md](GOVERNANCE.md).
