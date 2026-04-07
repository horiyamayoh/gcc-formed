# Release Notes

## v0.1.0

- Establishes the `v1alpha` GCC-first workspace baseline for wrapper, capture, adapter, IR, render, trace, and corpus replay.
- Adds release packaging support through `cargo xtask package`, generating primary/debug/source archives plus `manifest.json`, `build-info.txt`, and `SHA256SUMS`.
- Keeps release scope intentionally narrow: GCC 15 primary support, Linux-first runtime assumptions, and fail-open fallback behavior remain the shipped contract.

## Known Limits

- Detached signature generation is not implemented yet.
- The canonical production target remains `x86_64-unknown-linux-musl`; non-musl packaging is for smoke validation and compatibility paths only.
