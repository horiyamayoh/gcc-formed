# ADR-0025: Stable release automation and rollback evidence

- **Status**: Accepted
- **Date**: 2026-04-09

## Context

public beta では GitHub prerelease と immutable release repository を same-bits policy で結び付ける contract を `ADR-0024` で固定した。一方で stable cut には、beta より厳しい証跡が必要になる。

stable では「同じ signed candidate を canary / beta / stable へ metadata-only で昇格した」「exact version / checksum / signature pin install が成立した」「rollback が 1 回の `current` symlink switch で成立した」を 1 run で説明できなければならない。単に `release-promote` が存在するだけでは、GitHub Release assets・release-repo bundle・rollback drill の間に audit trail が残らない。

## Decision

- stable cut の canonical automation は `cargo xtask stable-release` と `.github/workflows/release-stable.yml` に固定する
- stable workflow は prior GitHub Release の immutable `.release-repo.tar.gz` bundle を seed に使い、その上で candidate control dir を publish する
- candidate は 1 回 build し、1 回 sign し、その same bits を `canary` → `beta` → `stable` へ metadata-only で昇格させる
- stable workflow / command は `stable-release-report.json`, `stable-release-summary.md`, `promotion-evidence.json`, `rollback-drill.json`, `release-provenance.json` を必須 evidence とする
- rollback drill は previous published version を install した状態から candidate を exact version / checksum / signature pin で install し、その後 1 回の `current` symlink switch で previous version へ戻ることを証明する
- stable GitHub Release は prerelease ではなく通常 release とし、control-dir bundle, release-repo bundle, signing material, and stable-cut evidence files を同時に公開する

## Consequences

- GitHub Release と immutable release repository が stable line でも別々の story にならず、same bits / same checksums / same signing metadata として説明できる
- rollback の「できるはず」ではなく、artifact ごとの drill evidence を release asset と workflow artifact の両方に残せる
- reviewer と maintainer は stable cut ごとに no-rebuild proof と rollback proof を機械可読 JSON で確認できる

## Out of Scope

- support / incident triage の人手運用 runbook 全体
- post-stable governance freeze や breaking-change policy
- package-manager-native stable distribution channels

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0008`, `ADR-0017`, `ADR-0021`, `ADR-0024`

## Source Specs

- `../packaging-runtime-operations-spec.md`
- `../RELEASE-CHECKLIST.md`
- `../RELEASE-NOTES.md`
- `../STABLE-RELEASE.md`
