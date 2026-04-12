# Interop Lab

This directory contains the checked-in fixtures for issue #128 and child issue #137.

The lab is intentionally narrow. It proves:

- direct `CC` / `CXX` insertion for Make
- direct `CMAKE_C_COMPILER` / `CMAKE_CXX_COMPILER` insertion for CMake
- one wrapper-owned backend launcher behind the wrapper
- parallel builds
- depfile generation
- response-file pass-through without wrapper expansion
- stdout-sensitive compiler probes like `-E` and `-print-*`

The runner executes `make -j2` and `cmake --build ... --parallel 2` against the checked-in fixtures.

Run the lab with:

```bash
python3 ci/interop_lab.py --lab-root eval/interop --report-dir target/interop-lab
```

The runner writes `interop-lab-report.json` into the report directory and keeps per-case backend logs under `workspace/`.

The lab proves direct wrapper insertion and one wrapper-owned backend launcher. It does not recommend ccache / distcc / sccache stacks or other launcher chains ahead of the wrapper.
