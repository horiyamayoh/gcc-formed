# Security Policy

## Release Status

`gcc-formed` is currently in the `v1alpha` maturity line, and the current artifact semver line is `0.1.x`. It is not yet declared ready for broad public use beyond the current alpha baseline; `v1beta`, `1.0.0-rc.N`, and `1.0.0` have not shipped.

## Current Support Boundary

Security support statements should be read inside the same support boundary documented in [SUPPORT-BOUNDARY.md](SUPPORT-BOUNDARY.md).

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- GCC 15 is the primary enhanced-render path.
- The terminal renderer is the primary user-facing surface.
- GCC 13/14 are compatibility-only paths and may use conservative passthrough or shadow behavior instead of the primary enhanced-render path.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy render.

## Supported Versions

| Artifact line | Security support status |
| --- | --- |
| `0.1.x` | `v1alpha` baseline; best-effort coordinated fixes for the current shipped artifacts within the documented support boundary |
| `main` | Development branch; fixes may land here first without backport guarantees |
| `< 0.1.0` | Not supported |

## Reporting a Vulnerability

- Do not open a public issue for embargoed vulnerabilities.
- Prefer the repository host's private vulnerability reporting or security advisory flow when it is enabled for this repository.
- If no private reporting flow is available, contact the maintainers through the same private channel used to obtain release artifacts before any public disclosure.
- Include the affected version or commit, target platform, GCC version, reproduction steps, observed impact, and whether the issue requires a specially crafted compiler invocation or source input.

## Response Expectations

- Acknowledgement target: within 5 business days on a best-effort basis.
- Triage target: severity classification and reproduction status within 10 business days when a working reproduction is provided.
- Fix target: no SLA is promised for the current `v1alpha` / `0.1.x` baseline; coordinated fixes are handled best-effort and may ship only in the next baseline release.

## Scope

- In scope: code execution, arbitrary file access, privilege boundary bypass, release artifact tampering, signature verification bypass, and integrity failures in install or update flows.
- Out of scope: unsupported toolchains, end-of-life versions, issues that require local source modification by an already trusted user, and UX-only defects without a security impact.
