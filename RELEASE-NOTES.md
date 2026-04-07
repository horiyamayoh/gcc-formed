# Release Notes

## v0.1.0

- Establishes the `v1alpha` GCC-first workspace baseline for wrapper, capture, adapter, IR, render, trace, and corpus replay.
- Adds release packaging support through `cargo xtask package`, generating primary/debug/source archives plus `manifest.json`, `build-info.txt`, and `SHA256SUMS`.
- Adds `cargo xtask install`, `rollback`, and `uninstall` so packaged artifacts can be verified with checksum validation, staged self-check, and `current` symlink switching.
- Adds `cargo xtask vendor` and `cargo xtask hermetic-release-check` so vendored dependency preparation and `--locked --offline --release` verification are part of the release path.
- Adds `cargo xtask release-publish`, `release-promote`, `release-resolve`, and `install-release` so immutable version repositories, metadata-only canary/beta/stable promotion, and exact-version + checksum pin installs are part of the release path.
- Adds optional Ed25519 `SHA256SUMS.sig` generation plus signing key id pin verification, and covers pseudo-root system-wide install layout in the packaging smoke path.
- Keeps release scope intentionally narrow: GCC 15 primary support, Linux-first runtime assumptions, and fail-open fallback behavior remain the shipped contract.

## Known Limits

- Production signing key distribution and trust bootstrap remain an operational concern outside the packaged artifact itself.
- The canonical production target remains `x86_64-unknown-linux-musl`; non-musl packaging is for smoke validation and compatibility paths only.
