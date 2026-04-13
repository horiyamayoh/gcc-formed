# Codex Cloud Execution Contract

> **Scope**: This file applies only to **Codex Cloud** (autonomous, non-interactive) tasks.
> Local Codex sessions and other agents (Claude, etc.) are **not** bound by this file.
>
> Read `AGENTS.md` first — it is the shared contract for all agents.
> This file adds the stricter execution discipline required when running autonomously
> with no human in the loop.

## When this contract applies

This contract applies when the task is executed by Codex Cloud (OpenAI's autonomous
cloud agent). If you are running Codex locally in interactive mode, or using a
different agent (Claude Code, Cursor, etc.), follow `AGENTS.md` only.

## Non-negotiables

- Quality is preferred over speed.
- A task is not complete when code exists; it is complete only when the required
  evidence exists and the repo-local gate is green.

## Work package contract

Before editing, derive and follow these fields from the issue / PR / parent epic /
current-authority docs:

- Goal
- Why now
- Allowed files
- Forbidden surfaces
- Acceptance criteria
- Commands
- Stop conditions
- Reviewer evidence

If one of these is missing, recover it from the issue graph or current-authority docs
before editing. Do not silently widen scope.

## Scope rules (Codex Cloud additions)

In addition to the scope rules in `AGENTS.md`:

- Treat the GitHub issue as source of truth; the prompt is a derived transport layer.
- Keep work item state in GitHub, not chat.
- If work pauses, leave enough state for handoff.

## Mandatory execution phases

Do not skip these phases.

### Phase 0 — recover scope

Before editing, identify:

- the exact goal
- non-goals
- allowed files
- forbidden surfaces
- change classification
- required docs / ADR / changelog sync
- validation commands that will prove success

### Phase 1 — write a work plan

Before editing implementation files, create:

- `.codex/evidence/work-plan.md`

Use `.codex/templates/work-plan-template.md` if present.

The work plan must include:

- goal and non-goals
- allowed files and forbidden surfaces
- current-authority docs consulted
- risk list
- validation plan
- sync obligations
- stop conditions

### Phase 2 — implement the smallest complete diff

- Prefer contract-preserving edits over broad rewrites.
- Preserve fail-open behavior unless the task explicitly changes it.
- If the task grows beyond one issue / one PR / one primary purpose, stop and report
  `STATUS: BLOCKED`.

### Phase 3 — self-review before claiming progress

Before any completion report, create:

- `.codex/evidence/self-review.md`

Use `.codex/templates/self-review-template.md` if present.

The self-review must include:

- diff audit
- contract audit
- edge cases checked
- commands run
- required docs / ADR / changelog decisions
- remaining risks

### Phase 4 — validate

Run exactly this command from the repository root:

```bash
python3 .codex/run_quality_gate.py
```

This is the completion gate. It writes `.codex/evidence/quality-gate.json`.

### Phase 5 — only then report status

A task is complete only when:

1. `python3 .codex/run_quality_gate.py` exits with code `0`
2. `.codex/evidence/quality-gate.json` exists
3. that JSON says `"passed": true`
4. the diff snapshot inside that JSON still matches the current workspace
5. the work plan and self-review files exist and are non-empty

If any of those are false, do **not** report success.

## Non-negotiable gate behavior

- Never report `STATUS: DONE` before the completion gate passes.
- Never say the task is done because one unit test passed.
- Never publish, push, tag, create a PR, or create a release before the gate passes.
- If the gate fails, fix the issue and rerun the gate.
- If the gate cannot run because of a missing prerequisite, do **not** claim success.
  Report `STATUS: BLOCKED` with the exact missing prerequisite and the exact command
  that could not be validated.

## Gate escalation rule

For docs-only changes, the quality gate may run the docs / contract subset.

For any non-doc change, prefer full local parity with CI through:

```bash
bash ci/run_pr_gate_local.sh
```

`python3 .codex/run_quality_gate.py` will automatically use that parity gate when
the script exists. If it does not exist, the gate falls back to the current repo-local
required checks from `CONTRIBUTING.md`.

## Publish guard

Do not run any of the following before the quality gate passes and the diff snapshot
remains unchanged:

- `git push`
- `gh pr create`
- `gh pr merge`
- `git tag`
- `gh release create`
- `cargo publish`
- `cargo xtask release-publish`
- `cargo xtask release-promote`
- `cargo xtask stable-release`

## Required final report format

When you stop, start the final message with exactly one of these headers:

- `STATUS: DONE`
- `STATUS: BLOCKED`
- `STATUS: NEEDS-INPUT`

If the status is `DONE`, include all of the following:

- change classification
- files touched
- docs / ADR / changelog updates
- commands run
- pass/fail result for each command
- path to `.codex/evidence/quality-gate.json`
- remaining non-blocking risks, if any

Do not hide unfinished work behind TODO comments, "follow-up later", or unverified
claims.

## Handoff format

If work pauses, leave state in the parent issue or PR body using this format:

```text
current state:
blockers:
in-flight PRs:
next 3 ready work packages:
docs or contracts touched:
```
