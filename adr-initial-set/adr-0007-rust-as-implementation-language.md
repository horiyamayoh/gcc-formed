# ADR-0007: Rust as implementation language

- **Status**: Accepted
- **Date**: 2026-04-07

## Context

本プロジェクトは低レベル compiler internals ではなく、高品質な structured CLI / IR 製品である。型安全、単一バイナリ配布、再現可能 build、長期保守性を同時に満たす実装言語が必要である。

## Decision

- v1alpha の実装言語は Rust とする
- release build は Rust toolchain と Cargo lockfile を基準にする
- shipped runtime に Python / Node / Java を要求しない
- rich IR と validation を Rust の型表現で支える

## Consequences

- single-binary distribution と相性が良い
- validation、serialization、CLI 実装を一貫して扱える
- チームには Rust 習熟と dependency governance が必要になる

## Out of Scope

- end user primary path としての source build
- Python venv 前提の配布
- polyglot runtime 混在を前提にした実装

## Supersedes/Related

- **Supersedes**: None
- **Related**: `ADR-0008`, `ADR-0009`, `ADR-0017`

## Source Specs

- `../gcc-formed-architecture-proposal.md` の推奨技術、12.2、19
- `../packaging-runtime-operations-spec.md` の 3、4、12、19
