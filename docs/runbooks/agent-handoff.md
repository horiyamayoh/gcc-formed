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

# Agent Handoff

Use this runbook whenever a coding session may pause, the active agent context may be lost, or work needs to transfer between maintainers and agents.

## Start Of Session

Read these three things in order:

1. the parent epic for the active milestone
2. the most recent handoff comment on that epic
3. the `Tonight Queue` or equivalent ready-work view for the active milestone

If GitHub Project fields are unavailable in the current environment, use the parent epic plus sub-issues as the system of record and keep the same field names in the issue body.

## End Of Session

Add one handoff comment to the parent epic. Keep the format fixed:

```text
current state:
blockers:
in-flight PRs:
next 3 ready work packages:
docs or contracts touched:
```

Do not end a session with state that exists only in chat or local notes.

## Work Package Contract

Every implementation issue should be a sub-issue of an epic and must include:

- `Goal`
- `Why now`
- `Allowed files`
- `Forbidden surfaces`
- `Acceptance criteria`
- `Commands`
- `Stop conditions`
- `Reviewer evidence`

If any of those fields are missing, the issue is not ready for nightly or handoff-safe execution.

## PR Pause Contract

Every PR should retain enough context to resume without the original chat:

- `Goal`
- `Parent Issue / Work Package`
- `Acceptance evidence`
- `Stop condition not hit`
- `Next recommended action if paused`

If work stops midstream, update the PR body or parent issue comment before leaving.
