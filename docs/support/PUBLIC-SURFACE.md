---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Canonical repo landing metadata, release-body generation inputs, and manual GitHub About sync.
do_not_use_for: Product-support claims that override SUPPORT-BOUNDARY.md or ad hoc marketing copy.
supersedes: []
superseded_by: []
repo_description: GCC diagnostic UX wrapper for GCC 9-15 that keeps terminal output shorter, root-cause-first, and fail-open.
repo_homepage_url: https://github.com/horiyamayoh/gcc-formed/blob/main/docs/README.md
repo_topics:
  - gcc
  - compiler-diagnostics
  - c
  - cpp
  - cli
  - developer-tools
readme_tagline: GCC diagnostic UX wrapper for GCC 9-15 that keeps terminal output shorter, root-cause-first, and fail-open.
beta_release_intro: This GitHub prerelease ships artifact `{version}` in the `v1beta` maturity line.
beta_release_gate_scope:
  - `GCC15+ / DualSinkStructured` remains the blocking reference-path snapshot smoke in this workflow.
  - `gcc13_14` and `gcc9_12` product-path blockers are classified by `replay-stop-ship.json`.
  - `replay-stop-ship.json` preserves missing required `VersionBand × ProcessingPath × Surface` cells and path-aware quality regressions from representative replay.
beta_install_path_lines:
  - Direct install / rollback path: see `docs/releases/PUBLIC-BETA-RELEASE.md` and use the `.control.tar.gz` bundle.
  - Exact-pin release-repository path: use the `.release-repo.tar.gz` bundle together with `cargo xtask install-release`.
beta_release_doc_paths:
  - docs/releases/PUBLIC-BETA-RELEASE.md
  - docs/support/SUPPORT-BOUNDARY.md
  - docs/support/KNOWN-LIMITATIONS.md
beta_included_assets:
  - primary archive
  - debug archive
  - source archive
  - control-dir bundle
  - immutable release-repo bundle
  - `manifest.json`
  - `build-info.txt`
  - `SHA256SUMS`
  - `SHA256SUMS.sig`
  - `replay-stop-ship.json`
  - `release-provenance.json`
beta_known_limits:
  - `{version}` remains a public-beta artifact, not a release candidate or stable release.
  - Current beta artifacts do not claim identical guarantees across all `VersionBand` values.
  - Raw fallback remains part of the shipped contract.
stable_release_intro: This GitHub Release publishes stable artifact `{version}` from a single signed build and promotes the same published bits through `canary`, `beta`, and `stable` without rebuilding.
stable_evidence_lines:
  - rollback baseline version: `{rollback_baseline_version}`
  - signing key id: `{signing_key_id}`
  - trusted signing public key sha256: `{signing_public_key_sha256}`
  - stable release report: `stable-release-report.json`
  - promotion evidence: `promotion-evidence.json`
  - rollback drill: `rollback-drill.json`
  - provenance bundle: `release-provenance.json`
  - rollout matrix report: `rollout-matrix-report.json`
  - path-aware replay stop-ship report: `replay-stop-ship.json`
  - rc gate report: `rc-gate-report.json`
stable_release_gate_scope:
  - `GCC15+` remains the primary fidelity reference path for shipped release quality.
  - Stable promotion is blocked by strict `rc-gate`, which checks rollout drift, representative replay quality, deterministic replay, fuzz, and manual UX sign-off.
  - `rollout-matrix-report.json` records the expected current `VersionBand` / `ProcessingPath` cases.
  - `replay-stop-ship.json` records missing required `VersionBand × ProcessingPath × Surface` cells and path-aware quality regressions from representative replay.
stable_release_doc_paths:
  - docs/releases/STABLE-RELEASE.md
  - docs/support/SUPPORT-BOUNDARY.md
  - docs/support/KNOWN-LIMITATIONS.md
  - docs/releases/RELEASE-CHECKLIST.md
stable_included_assets:
  - primary archive
  - debug archive
  - source archive
  - control-dir bundle
  - immutable release-repo bundle
  - `manifest.json`
  - `build-info.txt`
  - `SHA256SUMS`
  - `SHA256SUMS.sig`
  - `stable-release-report.json`
  - `stable-release-summary.md`
  - `promotion-evidence.json`
  - `rollback-drill.json`
  - `rc-gate-report.json`
  - `rollout-matrix-report.json`
  - `replay-stop-ship.json`
  - `release-provenance.json`
stable_known_limits:
  - Current release notes must not flatten the path-dependent guarantees across `VersionBand` values.
  - Raw fallback remains part of the shipped contract when it is the most trustworthy choice.
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Canonical repo landing metadata, release-body generation inputs, and manual GitHub About sync.
> Do not use for: Product-support claims that override `SUPPORT-BOUNDARY.md` or ad hoc marketing copy.

# Public Surface Contract

This document version-controls the shortest public statements that appear before a reader opens the full architecture and support docs.

It owns these surfaces:

- GitHub repository `About` description
- GitHub repository `Website` URL
- GitHub repository topics
- README top copy
- generated GitHub Release body links and non-support prose

`docs/support/SUPPORT-BOUNDARY.md` remains the canonical source for current support wording.  
This document does not redefine support posture. It defines the metadata and short-form prose that must stay synchronized with that support posture.

## Canonical Surface Set

- GitHub repo description must equal the `repo_description` front-matter field.
- README top copy must include the `readme_tagline` front-matter field verbatim.
- GitHub Release body generation must read this document plus `SUPPORT-BOUNDARY.md`; workflows must not hand-maintain release-body heredocs.
- GitHub repo `Website` and topics must be sourced from this document, not from manual memory.

## Manual Sync Contract

GitHub repository settings are still a maintainer-triggered operation. The checked-in source of truth is this document; the sync action is the `ci/public_surface.py` helper.

Preview the current contract with:

```bash
python3 ci/public_surface.py repo-metadata
python3 ci/public_surface.py render-release-body --kind beta --version 0.2.0-beta.1
python3 ci/public_surface.py sync-github-repo-metadata --repo horiyamayoh/gcc-formed --dry-run
```

Apply the GitHub `About` sync with:

```bash
python3 ci/public_surface.py sync-github-repo-metadata --repo horiyamayoh/gcc-formed
```

If the `repo_description`, `repo_homepage_url`, or `repo_topics` fields change, update the GitHub repository settings in the same change or immediately after merge, and record the sync in the issue/closeout note.
