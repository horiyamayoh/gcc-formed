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
- whether the observed surface is terminal renderer, shadow mode, passthrough/raw fallback, or packaging/install/release
- the trace bundle, if available

If the report is security-sensitive, stop here and switch to [SECURITY.md](../../SECURITY.md).

## 2. Route By Support Tier

Use the bug report’s support-tier field and confirm it against `--formed-self-check`.

### Tier A

- Conditions: GCC 15 primary enhanced-render path on the supported Linux primary artifact.
- Treat as product-path priority.
- Check whether the failure is:
  - incorrect renderer output
  - missing/incorrect first action
  - unexpected fallback
  - packaging/install/release regression

### Tier B

- Conditions: GCC 13/14 compatibility-only path.
- First verify whether the runtime banner already explains the conservative behavior.
- If the complaint is “not enhanced enough,” compare it against the documented compatibility contract before escalating.
- If the issue is install, trace, checksum, signature, or rollback related, treat it as a release-path defect regardless of tier.

### Tier C

- Conditions: older or unsupported compiler path.
- Verify whether the behavior matches the out-of-scope compatibility notice.
- Escalate only if the wrapper breaks fail-open guarantees, corrupts packaging/install state, or contradicts documented fallback behavior.

## 3. Route By Surface

### Terminal renderer / shadow / passthrough

1. Confirm the selected mode, support tier, and fallback reason from `trace.json` or `--formed-self-check`.
2. Compare the output against [SUPPORT-BOUNDARY.md](../../SUPPORT-BOUNDARY.md) and [KNOWN-LIMITATIONS.md](../../KNOWN-LIMITATIONS.md).
3. If needed, reproduce with `--formed-trace=always` and follow [trace-bundle-collection.md](trace-bundle-collection.md).

### Packaging / install / release

1. Confirm the install root, target triple, and access checks from `--formed-self-check`.
2. For end-user recovery, follow [rollback.md](rollback.md).
3. For stable-cut evidence or channel-promotion questions, also inspect [STABLE-RELEASE.md](../../STABLE-RELEASE.md).

## 4. Initial Severity Decision

- `release-blocker`: install/rollback/release integrity failures, signature/checksum bypass, or Tier A regressions that break the primary shipped path
- `high`: Tier A wrong diagnosis ranking, raw fallback without documented reason, or broken trace collection/redaction on the supported path
- `normal`: Tier B compatibility regressions, Tier C unexpected but fail-open-preserving behavior, or documentation gaps
- `low`: cosmetic wording issues that do not change routing, support boundary, or recovery procedures

## 5. Close The First Response

Before handing off or fixing:

- point the reporter to the relevant runbook
- state the confirmed support tier and surface
- record whether a trace bundle exists
- state whether the next step is reproduction, rollback/reinstall guidance, or release-path investigation
