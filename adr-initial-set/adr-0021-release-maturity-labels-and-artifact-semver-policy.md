---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Accepted design decisions that constrain implementation.
do_not_use_for: Historical superseded policy or workflow detail outside the decision.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Accepted design decisions that constrain implementation.
> Do not use for: Historical superseded policy or workflow detail outside the decision.

# ADR-0021: Release maturity labels and artifact semver policy

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

現状の top-level docs では、成熟度ラベルとしての `v1alpha`、artifact semver としての `0.1.0`、repository channel としての `stable` / `beta` が近い位置に出てくる。その結果、「`0.1.0` は stable なのか」「`stable` channel にあるなら `v1.0.0 stable` なのか」「`v0.1.0` は成熟度名なのか」が読み手に伝わりにくい。

spec-first repository として beta / rc / stable へ進む前に、support posture を示す語彙と、具体的な artifact version を示す語彙を分離して固定する必要がある。

## Decision

- 製品の成熟度は maturity label で表し、現在値は `v1alpha` とする
- shipped artifact の version は semver で表し、現在の artifact line は `0.1.x` とする
- release repository の `canary` / `beta` / `stable` は distribution channel であり、maturity label でも semver でもない
- 将来の naming は次に固定する

| Product stage | Maturity label | Artifact semver policy |
| --- | --- | --- |
| Alpha baseline | `v1alpha` | `0.1.x` |
| Public beta | `v1beta` | `0.2.0-beta.N` |
| Release candidate | `v1.0.0-rc` | `1.0.0-rc.N` |
| Stable release | `v1.0.0 stable` | `1.0.0` |

- user-facing docs では、support posture を説明するときは maturity label を、具体 artifact や rollback target を説明するときは semver を使う
- `v0.1.0` のような表現は archive 名や tag 名では許容するが、それを maturity label として扱ってはならない
- README、CHANGELOG、RELEASE-NOTES、RELEASE-CHECKLIST、KNOWN-LIMITATIONS、SECURITY、CONTRIBUTING はこの語彙を共有する

## Consequences

- third party reader が「いま stable かどうか」を誤読しにくくなる
- `0.1.x` artifact を `stable` channel に promote しても、それが `v1.0.0 stable` を意味しないことを説明できる
- beta / rc / stable に進むとき、artifact naming を後付けで決め直さずに済む

## Out of Scope

- Git tag naming の強制
- package manager ごとの metadata field 詳細
- future multi-product branding (`cc-formed` など) の version policy

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0008`, `ADR-0020`

## Source Specs

- `../README.md`
- `../docs/releases/RELEASE-NOTES.md`
- `../docs/releases/RELEASE-CHECKLIST.md`
- `../docs/support/KNOWN-LIMITATIONS.md`
- `../SECURITY.md`
