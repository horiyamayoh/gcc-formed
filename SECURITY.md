# Security Policy

## Release Status

`gcc-formed` is currently `v1alpha`. It is not yet declared a general-availability stable release for broad public use.

## Supported Versions

| Version | Security support status |
| --- | --- |
| `v0.1.x` | Best-effort coordinated fixes for the current baseline |
| `main` | Development branch; fixes may land here first without backport guarantees |
| `< v0.1.0` | Not supported |

## Reporting a Vulnerability

- Do not open a public issue for embargoed vulnerabilities.
- Prefer the repository host's private vulnerability reporting or security advisory flow when it is enabled for this repository.
- If no private reporting flow is available, contact the maintainers through the same private channel used to obtain release artifacts before any public disclosure.
- Include the affected version or commit, target platform, GCC version, reproduction steps, observed impact, and whether the issue requires a specially crafted compiler invocation or source input.

## Response Expectations

- Acknowledgement target: within 5 business days on a best-effort basis.
- Triage target: severity classification and reproduction status within 10 business days when a working reproduction is provided.
- Fix target: no SLA is promised for `v1alpha`; coordinated fixes are handled best-effort and may ship only in the next baseline release.

## Scope

- In scope: code execution, arbitrary file access, privilege boundary bypass, release artifact tampering, signature verification bypass, and integrity failures in install or update flows.
- Out of scope: unsupported toolchains, end-of-life versions, issues that require local source modification by an already trusted user, and UX-only defects without a security impact.
