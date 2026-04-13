---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current release, packaging, and promotion contract.
do_not_use_for: Historical release posture or archived artifact context.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current release, packaging, and promotion contract.
> Do not use for: Historical release posture or archived artifact context.

# Release Notes

This document uses artifact semver for release headings. Artifact `0.2.0-beta.1` belongs to the `v1beta` maturity line; it is not a `1.0.0-rc.N` or `1.0.0 stable` release.

## 0.2.0-beta.1

### Current `v1beta` Support Boundary

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.
- `VersionBand` and `ProcessingPath` remain observability metadata; they do not encode unequal user value inside `GCC 9-15`.
- `GCC16+`, `<=8`, and unknown gcc-like compilers are `PassthroughOnly` until separately evidenced.
- Internal capture mechanisms and raw-preservation details may differ by capability and invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.

### Highlights

- Ships the first public-beta artifact line as `0.2.0-beta.1` in the `v1beta` maturity line.
- Publishes a public GitHub prerelease with the signed primary/debug/source archives, `manifest.json`, `build-info.txt`, `SHA256SUMS`, `SHA256SUMS.sig`, a full control-dir bundle, an immutable release-repo bundle, and `release-provenance.json`.
- Adds a dedicated public-beta release workflow and `ADR-0024` so GitHub Releases, immutable release repositories, promote metadata, and support-boundary wording are governed by one policy.
- Adds `cargo xtask stable-release`, `release-stable.yml`, `STABLE-RELEASE.md`, and `ADR-0025` so a future stable cut can seed a prior release-repo bundle, promote one signed candidate from `canary` to `beta` to `stable` without rebuilding, and retain rollback drill artifacts.
- Adds `SUPPORT.md` plus incident/trace/rollback runbooks so beta users and maintainers can route bug reports and recovery steps from docs instead of relying on ad hoc guidance.
- Adds `GOVERNANCE.md` and strengthens `ADR-0020` / PR review prompts so stable-prep changes must be classified as `breaking`, `non-breaking`, or `experimental`, and post-`1.0.0` backlog expansion cannot drift into the shipped support boundary by accident.
- Routes the Python `ci/test_*.py` contract suite through `cargo xtask check`, so CI helper scripts and governance/support docs now fail the same standard gate as the Rust workspace tests.
- Documents the beta user path for install, rollback, exact version pin, and `install-release` in `PUBLIC-BETA-RELEASE.md`.
- Verifies the canonical `x86_64-unknown-linux-musl` artifact end to end: vendored hermetic release build, signed package generation, install, rollback, system-wide pseudo-root layout, immutable release publish/promote, and exact-pin install all run against the musl payload.
- Preserves reason-coded fallback evidence in trace, replay, snapshot, and release provenance outputs, including sink conflicts, unsupported version bands, shadow-only paths, missing SARIF, malformed SARIF, and renderer-side conservative fallback.
- Keeps release scope intentionally narrow while preserving one shared in-scope diagnostic contract across `GCC15`, `GCC13-14`, and `GCC9-12`; `GCC16+` and unknown compilers remain passthrough-only, Linux-first runtime assumptions remain intact, and fail-open fallback behavior remains part of the shipped contract.

## Known Limits

- `0.2.0-beta.N` is still a public-beta line; release-candidate artifacts start at `1.0.0-rc.N`.
- Release repository channels such as `canary`, `beta`, and `stable` are distribution pointers; they do not change the maturity label of the artifact they point to.
- Release verification now supports trusted signing public key sha256 pinning, so CI and installers can bind detached signatures to a stable trust anchor instead of relying on key id alone.
- Future stable cuts are expected to retain `stable-release-report.json`, `stable-release-summary.md`, `promotion-evidence.json`, and `rollback-drill.json`; see [STABLE-RELEASE.md](STABLE-RELEASE.md) for the runbook.
- `x86_64-unknown-linux-gnu` remains a compatibility and exception path; the shipped release story is centered on the primary `x86_64-unknown-linux-musl` artifact.
- Current beta artifacts do not claim that `GCC16+` or unknown gcc-like compilers are already inside the `GCC 9-15` contract.
- Internal capture mechanisms and same-run raw-preservation details still vary by capability even when the public contract is shared.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.
- See [KNOWN-LIMITATIONS.md](../support/KNOWN-LIMITATIONS.md) for the detailed support boundary and fallback semantics, and [PUBLIC-BETA-RELEASE.md](PUBLIC-BETA-RELEASE.md) for the public artifact and install story.

## 0.1.0

Historical alpha-baseline artifact. `0.1.0` belongs to the `v1alpha` maturity line and predates the public-beta GitHub Release path.
