# Support

`gcc-formed` is still in the `v1beta` / `0.2.0-beta.N` line, so support remains intentionally narrow and release-boundary driven.

## Current Support Boundary

Keep wording aligned with [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md).

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary enhanced-render path.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 are compatibility-only paths and may use conservative passthrough or shadow behavior instead of the primary enhanced-render path.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

## First Routing

- Security-sensitive issues: use [SECURITY.md](SECURITY.md), not the public bug template.
- Public bug reports: use [bug_report.yml](.github/ISSUE_TEMPLATE/bug_report.yml).
- Packaging, install, rollback, or release issues: start with [docs/runbooks/rollback.md](docs/runbooks/rollback.md).
- Runtime triage and maintainer initial response: use [docs/runbooks/incident-triage.md](docs/runbooks/incident-triage.md).
- Trace capture and redaction: use [docs/runbooks/trace-bundle-collection.md](docs/runbooks/trace-bundle-collection.md).

## Support Tier Routing

- Tier A: GCC 15 primary enhanced-render path on the supported Linux target. Treat this as the highest-priority product path.
- Tier B: GCC 13/14 compatibility-only path. Expect conservative passthrough or shadow behavior, and verify the compatibility banner before escalating as a renderer regression.
- Tier C: older or unsupported compiler path. These reports are still useful, but confirm whether the observed behavior is already covered by the documented compatibility/out-of-scope wording before treating it as a blocker.

You can confirm the active tier and local path layout with:

```bash
gcc-formed --formed-self-check
```

## Maintainer Initial Packet

For non-security incidents, ask for this minimum packet before deep triage:

1. `gcc-formed --formed-version=verbose`
2. `gcc-formed --formed-self-check`
3. The exact failing command line
4. The support-tier classification chosen in the bug template
5. A trace bundle or an explicit note that no trace bundle was captured

The detailed collection steps live in [docs/runbooks/trace-bundle-collection.md](docs/runbooks/trace-bundle-collection.md).
