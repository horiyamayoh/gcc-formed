---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Zero-config backend resolution, primary CLI modes, and build-system evidence.
do_not_use_for: RepairUnit grouping semantics or historical CLI design.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Zero-config backend resolution, primary CLI modes, and build-system evidence.
> Do not use for: RepairUnit grouping semantics or historical CLI design.

# Zero-config compiler replacement

`gcc-formed` resolves `gcc`; `g++-formed` resolves `g++`. PATH resolution canonicalizes candidates and rejects direct, symlink, hardlink, launcher, and launcher-cycle recursion. `FORMED_BACKEND_GCC` and `FORMED_BACKEND_LAUNCHER` remain optional advanced/recovery overrides, never quickstart requirements.

The normal vocabulary is:

| Current operation | Compatibility spelling |
|---|---|
| default RepairUnit view | `--formed-profile=default`, presentation presets |
| `--formed-raw` | `--formed-profile=raw_fallback`, `--formed-mode=passthrough` |
| `--formed-explain` | verbose/debug profile and cascade inspection flags |
| `--formed-self-check` | unchanged |

Compatibility spellings remain accepted for the beta window. They are advanced controls and do not appear in the wrapper's first-screen help.

## Verification evidence

`cargo xtask build-system-smoke --make --cmake` runs in clean temporary projects with both backend override variables removed. Make uses only `CC=<absolute gcc-formed>`, builds in parallel, creates an object, dependency file, and linked executable. CMake uses only `CMAKE_CXX_COMPILER=<absolute g++-formed alias>`, completes compiler identification, parallel build, object creation, and link.

The backend/capture/CLI test suites cover driver alias selection, PATH lookup, recursion, unknown argv ordering, `--`, response files, exit/signal propagation, stdout/stderr, depfiles, object/link side effects, and self-check metadata. `bench-smoke` retains success/failure overhead budgets. Per-invocation attribution for parallel builds is audited by the real-project corpus.
