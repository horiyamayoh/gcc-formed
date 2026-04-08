# ADR-0024: Public beta release channel and GitHub Release policy

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

`cargo xtask package`, `install`, `rollback`, `release-publish`, `release-promote`, `release-resolve`, and `install-release` により、local/CI での packaging と exact-pin install の仕組みは揃っている。一方で、public artifact を third party user が取得する正本は未固定であり、GitHub Releases は空のままだった。

beta では「artifact を作れる」だけでは不十分である。GitHub Release 上で何を公開し、immutable release repository とどう結び付け、install / rollback / exact version pin / signature pin をどの文書で説明するかを fixed contract にする必要がある。

## Decision

- public beta artifact は GitHub Releases の prerelease として公開する
- 公開対象の current maturity label は `v1beta`、artifact line は `0.2.0-beta.N` とする
- GitHub Release に載せる最小 asset set は次に固定する

| Asset | Purpose |
| --- | --- |
| primary archive | canonical shipped bits |
| debug archive | symbol/debug companion |
| source archive | source escape hatch |
| control-dir bundle | direct `cargo xtask install` path |
| immutable release-repo bundle | exact-pin `install-release` path |
| `manifest.json` | build / payload metadata |
| `build-info.txt` | human-readable build metadata |
| `SHA256SUMS` | integrity verification |
| `SHA256SUMS.sig` | detached signature |
| `release-provenance.json` | auditable package/publish/promote/install evidence |

- public beta release workflow は canonical `x86_64-unknown-linux-musl` payload を 1 回 build し、1 回 sign し、その same bits を GitHub Release assets と immutable release-repo bundle の両方へ出す
- promote story は `canary` → `beta` を metadata-only operation で行い、artifact rebuild を伴ってはならない
- GitHub Release body には support boundary, known limits, install / rollback / exact-pin install 導線, signing key id, signing public key sha256 を含める
- tag は archive 名と対応する `v<artifact semver>` 形式を使う。最初の public beta tag は `v0.2.0-beta.1` とする

## Consequences

- `There aren’t any releases here` の状態を解消し、third party user が public artifact を取得できる
- GitHub Release と immutable release repository が別物ではなく、same bits を別の access path で見せるものだと説明できる
- install / rollback / exact version pin / signature pin を README と release notes から一貫して辿れる

## Out of Scope

- `deb` / `rpm` / Homebrew / asdf など secondary distribution channels
- `stable` channel の一般公開運用
- automatic update や self-updater
- package-manager-native install UX の標準化

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0008`, `ADR-0017`, `ADR-0021`

## Source Specs

- `../packaging-runtime-operations-spec.md`
- `../RELEASE-NOTES.md`
- `../RELEASE-CHECKLIST.md`
- `../PUBLIC-BETA-RELEASE.md`
- `../SIGNING-KEY-OPERATIONS.md`
