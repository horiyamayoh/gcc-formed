# Fuzz Smoke Suite

`cargo xtask fuzz-smoke --root fuzz` runs the deterministic robustness seed suite.

This directory is not a replacement for a full coverage-guided fuzzer. It is the checked-in
regression suite for:

- malformed SARIF ingest
- residual stderr classification
- invalid / partial IR validation
- renderer stress cases
- trace serialization
- capture-runtime path sanitization

Each case lives under `fuzz/cases/<case-id>/case.json`. Asset files referenced by a case are kept
next to that `case.json` file.
