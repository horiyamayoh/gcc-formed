# Support Boundary

This document is the canonical wording for the current `v1beta` / `0.2.0-beta.N` support boundary. Keep README, release notes, known limitations, security policy, contribution guidance, and GitHub templates aligned with the exact wording below.

## Current `v1beta` / `0.2.0-beta.N` Support Boundary

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary enhanced-render path.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 are compatibility-only paths and may use conservative passthrough or shadow behavior instead of the primary enhanced-render path.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

## Explicitly Outside the Current Boundary

- Non-Linux production artifacts.
- Enhanced-render guarantees outside the GCC 15 primary path.
- Elimination of passthrough, shadow mode, or raw fallback.
- Release-candidate or stable general-availability support claims beyond the documented `v1beta` / `0.2.0-beta.N` baseline.
