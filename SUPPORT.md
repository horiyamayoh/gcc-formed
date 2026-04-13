# Support

`gcc-formed` is still in the `v1beta` / `0.2.0-beta.N` line, so support remains intentionally narrow and release-boundary driven.

## Canonical Support Docs

Current support wording is owned by [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md).  
Known operating limits and path-dependent constraints are summarized in [docs/support/KNOWN-LIMITATIONS.md](docs/support/KNOWN-LIMITATIONS.md).

Keep the canonical wording below aligned with `SUPPORT-BOUNDARY.md`.

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.
- `VersionBand` and `ProcessingPath` remain observability metadata; they do not encode unequal user value inside `GCC 9-15`.
- `GCC16+`, `<=8`, and unknown gcc-like compilers are `PassthroughOnly` until separately evidenced.
- Internal capture mechanisms and raw-preservation details may differ by capability and invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.

This file is still the routing page. The detailed support procedure lives in the linked runbooks.

## First Routing

- Security-sensitive issues: use [SECURITY.md](SECURITY.md), not the public bug template.
- Public bug reports: use [bug_report.yml](.github/ISSUE_TEMPLATE/bug_report.yml).
- Packaging, install, rollback, or release issues: start with [docs/runbooks/rollback.md](docs/runbooks/rollback.md).
- Runtime triage and maintainer initial response: use [docs/runbooks/incident-triage.md](docs/runbooks/incident-triage.md).
- Trace capture and redaction: use [docs/runbooks/trace-bundle-collection.md](docs/runbooks/trace-bundle-collection.md).
- Maintainer replay of stored bundles: use [docs/runbooks/trace-bundle-replay.md](docs/runbooks/trace-bundle-replay.md).
- Session handoff and resumability: use [docs/runbooks/agent-handoff.md](docs/runbooks/agent-handoff.md).
- Machine-readable export / automation consumers: use [docs/specs/public-machine-readable-diagnostic-surface-spec.md](docs/specs/public-machine-readable-diagnostic-surface-spec.md) and prefer attaching the JSON export over screen-scraped excerpts.

## VersionBand / ProcessingPath Routing

- `GCC15`: in-scope. `DualSinkStructured` is the default capability profile, but the public contract is the same one used across `GCC 9-15`.
- `GCC13-14`: in-scope. Check whether the observed path was `NativeTextCapture` or explicit `SingleSinkStructured`, and evaluate the complaint against the shared support boundary rather than a lower-value band contract.
- `GCC9-12`: in-scope. Check whether the observed path was `NativeTextCapture` or explicit JSON `SingleSinkStructured`; honest passthrough remains valid only when it is the most trustworthy result, not because the band is treated as lower-value.
- `GCC16+` / `Unknown`: `PassthroughOnly` until proven otherwise. Prioritize build correctness, provenance, and recovery over enhancement.

Runtime and trace output use `VersionBand`, `ProcessingPath`, and `SupportLevel` as the canonical public labels. Use `--formed-self-check` for the current operator guidance, and keep [docs/support/OPERATOR-INTEROP.md](docs/support/OPERATOR-INTEROP.md) as the shared next-step reference for older GCC and C-first builds.

You can confirm the local wrapper layout and capture backend with:

```bash
gcc-formed --formed-self-check
```

## Maintainer Initial Packet

For non-security incidents, ask for this minimum packet before deep triage:

1. `gcc-formed --formed-version=verbose`
2. `gcc-formed --formed-self-check`
3. The exact failing command line
4. The `VersionBand`, `ProcessingPath`, and user surface chosen in the bug template
5. A trace bundle or an explicit note that no trace bundle was captured
6. If a trace bundle exists, note whether it came from the default state-root trace directory or a user-specified path, and whether redaction review was performed before sharing
7. If available, the public JSON export artifact produced by the run, without hand-formatting or screenshotting it

The detailed collection steps live in [docs/runbooks/trace-bundle-collection.md](docs/runbooks/trace-bundle-collection.md).
