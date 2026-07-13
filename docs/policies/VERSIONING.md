---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current versioning, governance, and change-control rules.
do_not_use_for: Historical rollout policy or superseded support language.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current versioning, governance, and change-control rules.
> Do not use for: Historical rollout policy or superseded support language.

# Versioning Policy

This document fixes the naming contract for `gcc-formed` so maturity labels, artifact versions, and release channels are not conflated.

## Current Baseline

| Axis | Current value | Meaning |
| --- | --- | --- |
| Maturity label | `v1.0.0 stable` | Current stable maturity and support posture |
| Stable release identity | `1.0.0` | Published general-availability release |
| Immutable payload semver | `1.0.0-rc.1` | Signed qualified RC payload promoted without rebuilding or rewriting |
| General-availability stable release | [v1.0.0](https://github.com/horiyamayoh/gcc-formed/releases/tag/v1.0.0) | Published 2026-07-13 |

## Fixed Progression

| Product stage | Maturity label | Artifact semver policy |
| --- | --- | --- |
| Alpha baseline | `v1alpha` | `0.1.x` |
| Public beta | `v1beta` | `0.2.0-beta.N` |
| Release candidate | `v1.0.0-rc` | `1.0.0-rc.N` |
| Stable release | `v1.0.0 stable` | release identity `1.0.0`; immutable promoted payload retains qualified `1.0.0-rc.N` semver |

## Release Channels Are Separate

Repository channels such as `canary`, `beta`, and `stable` are distribution pointers. They are not maturity labels and they do not override artifact semver.

For the first stable cut, `v1.0.0` is a release identity over the exact signed
RC payload. The payload keeps its `1.0.0-rc.N` embedded version, archive names,
manifest, checksums, and signature. Rewriting any of those to say `1.0.0`
would contradict the required same-bits promotion. Stable provenance therefore
records `stable_release_version=1.0.0` separately from `package_version`.

Examples:

- `0.2.0-beta.1` may be published to a `beta` channel inside a release repository while the product maturity remains `v1beta`.
- `0.2.0-beta.3` is still a beta artifact even if it is the current `stable` channel target for an internal rollout.

## Wording Rules

- Use maturity labels when describing support posture or lifecycle state.
- Use artifact semver when describing a concrete release, package, rollback target, or changelog entry.
- Prefer wording such as "artifact `0.2.0-beta.1` in the `v1beta` maturity line".
- Do not use `v0.2.0-beta.1` as a maturity label.
- Archive names, tags, and install paths may embed a `v` prefix for readability, such as `gcc-formed-v0.2.0-beta.1-linux-x86_64-musl.tar.gz`; that prefix does not change the underlying semver.
- `1.0.0-rc.N` or `1.0.0` output-quality wording must identify single-agent task-performance evidence and deterministic readability proxies. It must not say or imply that a human behavioral study passed.
- `1.0.0-rc.N` must not be published until its candidate SHA has a passing sealed qualification packet and strict RC gate.

## Current Reader Guidance

- `README.md` should state both the current maturity label and current artifact line.
- `CHANGELOG.md` and `RELEASE-NOTES.md` should use artifact semver headings and call out the current maturity line in prose.
- `SECURITY.md`, `KNOWN-LIMITATIONS.md`, `RELEASE-CHECKLIST.md`, and `PUBLIC-BETA-RELEASE.md` should describe support and guarantees using the same vocabulary.
- [GOVERNANCE.md](GOVERNANCE.md) should define which contract changes are `breaking`, `non-breaking`, or `experimental`, and which backlog remains post-`1.0.0` only.

## Authority

This summary is governed by [ADR-0021](../../adr-initial-set/adr-0021-release-maturity-labels-and-artifact-semver-policy.md) and the broader change-control policy in [GOVERNANCE.md](GOVERNANCE.md).
