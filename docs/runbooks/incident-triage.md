---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current operational procedure and support response guidance.
do_not_use_for: Historical planning context or superseded delivery models.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current operational procedure and support response guidance.
> Do not use for: Historical planning context or superseded delivery models.

# Incident Triage Runbook

Use this runbook when a bug report or internal incident arrives and you need to decide the first action without rereading the whole repo.

## 1. Collect The Minimum Packet

Ask for, or reproduce, all of the following first:

```bash
gcc-formed --formed-version=verbose
gcc-formed --formed-self-check
```

Also collect:

- the exact failing compiler or wrapper command
- the reported `VersionBand`, `ProcessingPath`, and user surface
- the trace bundle, if available

If the report is security-sensitive, stop here and switch to [SECURITY.md](../../SECURITY.md).

## 2. Route By VersionBand

Use the bug report’s `VersionBand` field first. If current runtime output still only exposes legacy internal classification fields, preserve them as evidence but translate the incident into the public `VersionBand` / `ProcessingPath` vocabulary in the issue thread.

### `GCC15+`

- Treat as product-path priority.
- Check whether the failure is:
  - incorrect renderer output
  - missing/incorrect first action
  - unexpected fallback
  - packaging/install/release regression

### `GCC13-14`

- Treat as in-scope `Experimental`.
- Confirm whether the run behaved like `NativeTextCapture`, `SingleSinkStructured`, or a conservative fallback path.
- If the complaint is “not enhanced enough,” compare it against the documented support boundary and current beta matrix before escalating it to stop-ship severity.
- If the issue is install, trace, checksum, signature, or rollback related, treat it as a release-path defect regardless of band.

### `GCC9-12`

- Treat as in-scope `Experimental` with narrower expected wins.
- Confirm whether the wrapper preserved build correctness, provenance, and an honest escape hatch.
- Escalate when the wrapper breaks fail-open guarantees, corrupts packaging/install state, or hides compiler-owned facts.

### `Unknown`

- Treat as `PassthroughOnly` unless you have stronger evidence.
- Escalate only if the wrapper breaks build correctness, trace collection, install/release integrity, or documented fallback honesty.

## 3. Route By Surface

### TTY renderer / CI renderer / raw fallback

1. Confirm the selected mode, available backend classification, and fallback reason from `trace.json` or `--formed-self-check`.
2. Compare the output against [SUPPORT-BOUNDARY.md](../support/SUPPORT-BOUNDARY.md) and [KNOWN-LIMITATIONS.md](../support/KNOWN-LIMITATIONS.md).
3. If needed, reproduce with `--formed-trace=always` and follow [trace-bundle-collection.md](trace-bundle-collection.md).

### Probe / capture / analysis

1. Confirm compiler version, sink selection, and any preserved structured artifacts.
2. Check whether the observed behavior contradicts the declared `ProcessingPath` or merely reflects a narrower-quality path.
3. Preserve trace artifacts before attempting to normalize or reinterpret them.

### Packaging / install / release

1. Confirm the install root, target triple, and access checks from `--formed-self-check`.
2. For end-user recovery, follow [rollback.md](rollback.md).
3. For stable-cut evidence or channel-promotion questions, also inspect [STABLE-RELEASE.md](../releases/STABLE-RELEASE.md).

## 4. Initial Severity Decision

- `release-blocker`: install/rollback/release integrity failures, signature/checksum bypass, or `GCC15+` regressions that break the primary shipped path
- `high`: wrong diagnosis ranking on the reference path, raw fallback without documented reason, or broken trace collection/redaction on a supported path
- `normal`: `GCC13-14` / `GCC9-12` regressions that preserve fail-open behavior, or documentation gaps
- `low`: cosmetic wording issues that do not change routing, support boundary, or recovery procedures

## 5. Close The First Response

Before handing off or fixing:

- point the reporter to the relevant runbook
- state the confirmed `VersionBand`, `ProcessingPath` if known, and user surface
- record whether a trace bundle exists
- state whether the next step is reproduction, rollback/reinstall guidance, release-path investigation, or work-package creation
