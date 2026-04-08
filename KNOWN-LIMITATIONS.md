# Known Limitations

`gcc-formed` is currently in the `v1alpha` maturity line, and the current artifact line is `0.1.x`. The current alpha baseline is intentionally narrow.

## Primary Contract

- Linux-first runtime assumptions only.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the only primary enhanced-render target.
- The terminal renderer is the primary user-facing surface.

## Compatibility Paths

- GCC 13/14 are compatibility-only paths. They may preserve conservative passthrough output or shadow-only capture instead of the enhanced render path.
- `x86_64-unknown-linux-gnu` is a compatibility smoke and exception path, not the primary shipped artifact.
- Older GCC versions are outside the first-release support scope and should be expected to fall back to passthrough behavior.

## Raw Fallback

Raw fallback is part of the shipped contract. It means the wrapper preserved compiler output because it could not produce a clearly better, trustworthy render.

You should expect raw fallback when:

- the backend is outside the primary GCC 15 render path
- the invocation forces an incompatible diagnostics sink
- structured capture is unavailable or incomplete
- the renderer cannot produce a higher-confidence document than the preserved raw diagnostics

## What Is Not Guaranteed Yet

- Enhanced render quality outside the GCC 15 primary path.
- Non-Linux production artifacts.
- Elimination of passthrough, shadow mode, or raw fallback.
- Public beta artifacts (`0.2.0-beta.N`) or release-candidate / stable artifacts (`1.0.0-rc.N`, `1.0.0`).
- Stable general-availability support promises beyond the documented `v1alpha` / `0.1.x` scope.

## Bug Reports

When reporting a bug, include the support tier and a trace bundle when possible. The shortest path is:

```bash
gcc-formed --formed-trace=always ...
```

Attach the resulting `trace.json`, normalized IR, and preserved `stderr.raw` from the trace directory.
