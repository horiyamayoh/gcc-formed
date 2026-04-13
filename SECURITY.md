# Security Policy

## Release Status

`gcc-formed` is currently in the `v1beta` maturity line, and the current artifact semver line is `0.2.0-beta.N`. It is available for narrow public beta use inside the documented support boundary, but `1.0.0-rc.N` and `1.0.0` have not shipped.

## Current Support Boundary

Security support statements should be read inside the same support boundary documented in [docs/support/SUPPORT-BOUNDARY.md](docs/support/SUPPORT-BOUNDARY.md).

- Linux first.
- `x86_64-unknown-linux-musl` is the primary production artifact.
- The terminal renderer is the primary user-facing surface.
- `GCC15`, `GCC13-14`, and `GCC9-12` share one in-scope public contract.
- `VersionBand` and `ProcessingPath` remain observability metadata; they do not encode unequal user value inside `GCC 9-15`.
- `GCC16+`, `<=8`, and unknown gcc-like compilers are `PassthroughOnly` until separately evidenced.
- Internal capture mechanisms and raw-preservation details may differ by capability and invocation.
- Raw fallback remains part of the shipped contract when the wrapper cannot produce a clearly better, trustworthy result.

This file defines the reporting path and response expectations. It does not widen the product support posture beyond the canonical support boundary.

## Supported Versions

| Artifact line | Security support status |
| --- | --- |
| `0.2.0-beta.N` | Current `v1beta` public-beta line; best-effort coordinated fixes for the current shipped artifacts within the documented support boundary |
| `0.1.x` | Superseded `v1alpha` baseline; upgrade to the `0.2.0-beta.N` line for ongoing fixes |
| `main` | Development branch; fixes may land here first without backport guarantees |
| `< 0.1.0` | Not supported |

## Reporting a Vulnerability

- Do not open a public issue for embargoed vulnerabilities.
- Prefer the repository host's private vulnerability reporting or security advisory flow when it is enabled for this repository.
- If no private reporting flow is available, contact the maintainers through the same private channel used to obtain release artifacts before any public disclosure.
- Include the affected version or commit, target platform, GCC version, reproduction steps, observed impact, and whether the issue requires a specially crafted compiler invocation or source input.
- For non-security breakage, use [SUPPORT.md](SUPPORT.md) and the linked runbooks instead of the security path.

## Response Expectations

- Acknowledgement target: within 5 business days on a best-effort basis.
- Triage target: severity classification and reproduction status within 10 business days when a working reproduction is provided.
- Fix target: no SLA is promised for the current `v1beta` / `0.2.0-beta.N` baseline; coordinated fixes are handled best-effort and may ship only in the next beta or release-candidate artifact.

## Scope

- In scope: code execution, arbitrary file access, privilege boundary bypass, release artifact tampering, signature verification bypass, and integrity failures in install or update flows.
- Out of scope: unsupported toolchains, end-of-life versions, issues that require local source modification by an already trusted user, and UX-only defects without a security impact.
