---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current maintainer replay procedure for stored trace bundles.
do_not_use_for: Historical replay experiments or live re-capture workflows.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current maintainer replay procedure for stored trace bundles.
> Do not use for: Historical replay experiments or live re-capture workflows.

# Trace Bundle Replay

Use this runbook when a maintainer needs to inspect an already captured trace bundle without re-running the original command.

## 1. Preconditions

Replay assumes the bundle was captured locally with `--formed-trace-bundle[=<path>]` and that the stored contents are the source of truth.

Do not use replay to:

- fetch fresh stderr from the host
- infer missing artifacts without saying so
- recreate a capture that was never stored
- bypass redaction or privacy review

If the bundle came from the default state-root trace directory, note that in the report. If it came from a user-specified path, preserve that path verbatim.

## 2. Replay Command

```bash
cargo xtask replay-trace-bundle --bundle <path>
```

Use the path to the stored bundle directory or archive. The replay command must operate on bundle contents only.

## 3. Report The Result

When you share the replay result, include:

- bundle source path
- whether the bundle was captured under the state-root default or a user-specified path
- which artifacts were present in the stored bundle
- whether redaction removed paths, usernames, source excerpts, or command lines
- any explicit degradation disclosure emitted by replay

If the bundle is incomplete or redacted enough to reduce fidelity, say that plainly instead of presenting the replay as full-fidelity reproduction.

## 4. Practical Rule

Replay is read-only support analysis. If the stored bundle is not enough, say so and ask for a fresh capture or additional evidence. Do not silently fill gaps by re-running the live command.
