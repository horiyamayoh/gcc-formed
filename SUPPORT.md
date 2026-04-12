# Support

`gcc-formed` is still in the `v1beta` / `0.2.0-beta.N` line, so support remains intentionally narrow and release-boundary driven.

## Current Support Boundary

Keep wording aligned with [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md).

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15+`, `GCC13-14`, and `GCC9-12` are all in-scope product bands.
- `GCC15+` is the primary fidelity reference path.
- `GCC13-14` and `GCC9-12` are product paths with narrower guarantees and different capture constraints.
- `GCC13-14` remains a first-class beta path inside that narrower contract.
- `GCC9-12` is a product path with narrower guarantees and different capture constraints.
- `ProcessingPath` and `RawPreservationLevel` may differ by band and by invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.

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

- `GCC15+`: highest-priority reference path. Treat regressions here as product-path issues.
- `GCC13-14`: in-scope first-class beta path. Check whether the observed path was `NativeTextCapture` or `SingleSinkStructured`, and evaluate the complaint against the current support boundary before treating it as a stop-ship regression.
- `GCC9-12`: in-scope `Experimental` path with narrower expected wins. Fail-open behavior or honest passthrough may still be the correct result.
- `Unknown`: `PassthroughOnly` until proven otherwise. Prioritize build correctness, provenance, and recovery over enhancement.

Current runtime and trace output may still expose legacy internal tier-oriented fields until the M1 vocabulary migration lands. Attach those raw fields as evidence, but use `VersionBand`, `ProcessingPath`, and `SupportLevel` as the canonical public labels in new issues and PRs.

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
