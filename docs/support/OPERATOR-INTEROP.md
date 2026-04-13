---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Operator quickstart and current interop topology guidance for Make / CMake.
do_not_use_for: Historical rollout notes or unsupported launcher-chain claims.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Operator quickstart and current interop topology guidance for Make / CMake.
> Do not use for: Historical rollout notes or unsupported launcher-chain claims.

# Operator Interop

This document is the shortest current-authority guide for inserting `gcc-formed` into real GCC build systems.

The checked-in interop lab proves two insertion shapes:

- direct `CC` / `CXX` replacement
- direct `CC` / `CXX` replacement plus one wrapper-owned backend launcher

In both cases, `make` or `cmake` owns only the outer build orchestration. The wrapper remains the configured compiler entrypoint.

## Quickstart

### Make

```bash
export CC=gcc-formed
export CXX=g++-formed
export FORMED_BACKEND_GCC="$(command -v gcc)"
make -j
```

### Optional single backend launcher

If you need one cache / remote-exec-style launcher, put it behind the wrapper instead of in front of it.

```bash
export CC=gcc-formed
export CXX=g++-formed
export FORMED_BACKEND_GCC="$(command -v gcc)"
export FORMED_BACKEND_LAUNCHER="/absolute/path/to/ccache"
make -j
```

### CMake

```bash
cmake -S . -B build -G "Unix Makefiles" \
  -DCMAKE_C_COMPILER=gcc-formed \
  -DCMAKE_CXX_COMPILER=g++-formed
cmake --build build -j
```

The same launcher shape works with CMake because the build-system insertion still stays at `gcc-formed` / `g++-formed`:

```bash
export FORMED_BACKEND_GCC="$(command -v gcc)"
export FORMED_BACKEND_LAUNCHER="/absolute/path/to/ccache"
cmake -S . -B build -G "Unix Makefiles" \
  -DCMAKE_C_COMPILER=gcc-formed \
  -DCMAKE_CXX_COMPILER=g++-formed
cmake --build build -j
```

### Raw fallback

If a build is not yet proven, fall back to raw `gcc` / `g++` for that build.

For a direct wrapper invocation, `--formed-mode=passthrough` is the explicit bypass path.

## VersionBand Routing

- `GCC15`: keep direct `CC` / `CXX` replacement as the default insertion shape. `DualSinkStructured` is the default capability profile, but the public contract is the same one used across `GCC 9-15`.
- `GCC13-14`: the shared in-scope contract still applies. `NativeTextCapture` is the default capability profile, `SingleSinkStructured` remains explicit, and you should keep at most one wrapper-owned backend launcher behind the wrapper.
- `GCC9-12`: the shared in-scope contract still applies. Prefer `NativeTextCapture` for ordinary runs, use explicit JSON `SingleSinkStructured` when needed, and fall back to raw `gcc` / `g++` or `--formed-mode=passthrough` only when the topology or trust level is not proven.
- `Unknown`: use raw `gcc` / `g++` or `--formed-mode=passthrough` until a supported `VersionBand` is confirmed.

## Shared Operator Guidance

Self-check and runtime notices use the same operator-next-step wording below.

- `GCC15`: keep direct `CC` / `CXX` replacement, and keep at most one wrapper-owned backend launcher behind the wrapper.
- `GCC13-14`: for C-first Make / CMake builds, set `CC=gcc-formed` and `CXX=g++-formed`; keep at most one wrapper-owned backend launcher behind the wrapper, and fall back to raw `gcc` / `g++` or `--formed-mode=passthrough` only if the topology is not proven.
- `GCC9-12`: same topology guidance as `GCC13-14`, but prefer `NativeTextCapture` for ordinary runs and use explicit JSON `SingleSinkStructured` when you need machine-readable structured capture.
- `Unknown`: use raw `gcc` / `g++` or `--formed-mode=passthrough` until a supported `VersionBand` is confirmed.

The release doc keeps rollback and uninstall close to the install instructions:

- [docs/releases/PUBLIC-BETA-RELEASE.md](../releases/PUBLIC-BETA-RELEASE.md)

## Topology Policy

Versioned beta policy for this issue scope:

| Topology | Status | Guidance |
|---|---|---|
| Direct `CC` / `CXX` wrapper insertion into Make or CMake | Supported and lab-proven | Use first |
| Direct `CC` / `CXX` wrapper insertion plus one wrapper-owned backend launcher via `--formed-backend-launcher`, `FORMED_BACKEND_LAUNCHER`, or `[backend].launcher` | Supported and lab-proven | Supported for one concrete launcher executable behind the wrapper |
| Raw `gcc` / `g++` build without the wrapper | Supported fallback | Use for triage or rollback |
| Direct wrapper invocation with `--formed-mode=passthrough` | Supported escape hatch | Use when you need the wrapper path but not the renderer |
| ccache / distcc / sccache / other launcher stack in front of the wrapper | Unsupported | Do not recommend |
| `CC="ccache gcc-formed"` / `CMAKE_<LANG>_COMPILER_LAUNCHER=ccache` / similar build-system-managed launcher-before-wrapper topologies | Unsupported | Keep the wrapper as the compiler entrypoint instead |
| Wrapper behind another launcher or multi-launcher backend chain | Unsupported | Configure at most one launcher executable |

## Interop Lab Scope

The checked-in lab covers:

- direct Make and CMake insertion
- wrapper-owned single backend-launcher insertion
- parallel build execution
- repeated stress rounds at `make -j4` / `cmake --build --parallel 4`
- depfile generation
- response-file non-expansion
- stdout-sensitive compiler probes
- runtime / trace root cleanup after successful concurrent builds

The lab does not claim support for launcher stacks in front of the wrapper, multi-launcher chains, or shell-based re-parsing by the wrapper.
