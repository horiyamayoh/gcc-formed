---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Signed 1.0 release-candidate publication contract.
do_not_use_for: Stable promotion or claims beyond the sealed single-agent evidence.
supersedes:
  - PUBLIC-BETA-RELEASE.md
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Signed 1.0 release-candidate publication contract.
> Do not use for: Stable promotion or claims beyond the sealed single-agent evidence.

# Release Candidate Runbook

The current source candidate is `1.0.0-rc.1`. It may be published only from
the exact qualified product commit after the evidence-only commit has added the
verified qualification summaries consumed by `cargo xtask rc-gate`.

## Preconditions

- `qualification-report.json` is `pass` for 120 families / 360 valid trials.
- `artifact-integrity-report.json`, fidelity, utility, efficiency, readability,
  and the required GCC/path matrix are all `pass`.
- The candidate SHA, binary SHA-256, protocol/analysis hashes, pinned
  model-agent-tool manifest, and no-subagent attestation agree.
- `cargo xtask check`, exact-count/fidelity, real-project, strict RC, package,
  install, and rollback gates are green.
- `RELEASE_SIGNING_PRIVATE_KEY_HEX` is configured in GitHub Actions.

## Publication

Dispatch `.github/workflows/release-beta.yml` (workflow name `prerelease`) with
`version=1.0.0-rc.1`, or push the signed release tag after all branch checks
pass. The workflow recognizes `-rc.N`, uses maturity label `v1.0.0-rc`, runs
strict RC and candidate-matrix gates, builds the musl payload hermetically,
signs it, verifies install/rollback/exact-pin behavior, and publishes a GitHub
prerelease.

The release additionally retains the full sealed qualification packet as a
GitHub Release asset. Raw per-trial artifacts are not rewritten or reduced to
the checked-in summaries.

## Claim boundary

Release text may report pinned coding-agent task performance and deterministic
readability-contract results. It must state that no human behavioral study was
performed and must not claim human-population latency, preference, usability,
or non-inferiority.

## Rollback

Use `subject_blocks_v2` for the immediate presentation rollback,
`--formed-raw` for byte-faithful compiler output, or the signed
`0.2.0-beta.1` package as the artifact rollback baseline. Stable promotion
uses [STABLE-RELEASE.md](STABLE-RELEASE.md) only after RC field evidence is
green. That workflow consumes this RC's immutable signed payload directly; it
does not rebuild, rename, or re-sign the payload for the stable release.
