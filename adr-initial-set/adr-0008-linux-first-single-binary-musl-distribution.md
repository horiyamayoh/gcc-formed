# ADR-0008: Linux-first single-binary musl distribution

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

compiler wrapper は developer machine と CI の両方で同じ artifact を使えることが重要である。runtime 依存を減らし、install / rollback / support の責任境界を製品側へ寄せるには、Linux first の単一バイナリ配布を正本にする必要がある。

## Decision

- primary artifact は versioned archive に入った単一 Rust 実行バイナリとする
- Linux first の production baseline は `x86_64-unknown-linux-musl` とする
- install は versioned root + atomic symlink switch を採る
- package manager integration は same bits repackaging に限定し、primary artifact を正本とする

## Consequences

- rootless install、CI、rollback が単純になる
- release engineering は checksum、manifest、offline build 前提になる
- macOS / Windows / distro-native convenience は secondary channel 扱いになる

## Out of Scope

- self-update
- package-manager-native rebuild
- container-only distribution

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0007`, `ADR-0016`, `ADR-0017`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の推奨技術、配布方針、19
- `../packaging-runtime-operations-spec.md` の 3、4、7、8、11、12、14、19
