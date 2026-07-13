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

## Runtime identity contract

Runtime and support surfaces use two identities and must never collapse them:

| Field | Authority | Example | Meaning |
| --- | --- | --- | --- |
| `release_identity` | verified release-repository channel pointer plus the installer-verified signing key | `version=1.0.0`, `channel=stable` | Public distribution context from which this install was obtained |
| `payload_identity` | embedded build manifest plus verified primary-archive digest | `product_version=1.0.0-rc.1`, commit, SHA-256 | Immutable bits that were qualified, signed, installed, and may be rolled back to |

The installer writes `install-identity.json` only after checksum, manifest, and signature verification. Runtime accepts a release identity only when that record matches the embedded payload version and commit, has a valid archive digest, and names `verified_channel_pointer` as its attestation source. A missing, malformed, or mismatched record yields `release_identity = unknown/not-attested`; a direct archive extraction must never infer `stable` from filenames, Cargo prerelease text, an environment variable, or the current date. Payload identity remains reportable independently.

Current truth table:

| Context | Release identity | Payload identity |
| --- | --- | --- |
| development build | unknown / not attested | embedded Cargo version + build commit; archive hash unknown |
| direct RC or stable-asset archive extraction | unknown / not attested | embedded `1.0.0-rc.1` + build commit; archive hash unknown unless installed through a verifier |
| exact-version repository install | unknown / not attested | verified payload version + commit + archive hash |
| signed stable-channel install | attested `1.0.0 stable` | verified `1.0.0-rc.1` + build commit + archive hash |

`--formed-version` remains the concise payload-semver output for existing scripts. `--formed-version=verbose`, `--formed-self-check`, `--formed-dump-build-manifest`, trace version summaries, and public JSON producer metadata report the two identities additively. Plain `gcc-formed --version` remains GCC-compatible compiler introspection forwarded to the backend and is not a wrapper-version command.

For maintenance releases, the public release identity advances to the published final version (`1.0.1`, `1.0.2`, ...). `payload_identity.product_version` advances only when new payload bits are built and signed; a same-bits promotion retains its qualified payload semver and digest. A maintenance workflow must publish the relationship explicitly and must not rewrite an already published payload, tag, archive, manifest, checksum, or signature.

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
