# Codex Work Package Template

Use this as the issue body or prompt scaffold for cloud Codex tasks.

```md
# Work Package

## Goal
One primary purpose only.

## Why now
Why this belongs in the current milestone / epic.

## Background
Current behavior, current-authority docs, and exact contract surface involved.

## Allowed files
- exact files or directories the agent may modify

## Forbidden surfaces
- files / directories / contracts the agent must not change

## Change classification
- `non-breaking` / `breaking` / `experimental`
- explain why

## Acceptance criteria
- [ ] exact behavior change is complete
- [ ] required docs / ADR / changelog updates are included
- [ ] no silent scope expansion
- [ ] `python3 .codex/run_quality_gate.py` passes
- [ ] `.codex/evidence/quality-gate.json` exists and says `"passed": true`

## Evidence files Codex must create
- [ ] `.codex/evidence/work-plan.md` before implementation edits
- [ ] `.codex/evidence/self-review.md` before completion report
- [ ] `.codex/evidence/quality-gate.json` after validation

## Required commands
```bash
python3 .codex/run_quality_gate.py
```

## Reviewer evidence
- expected changed files
- expected docs / ADR / changelog updates
- expected risk areas
- expected gate evidence path: `.codex/evidence/quality-gate.json`

## Stop conditions
- conflicting current-authority docs
- missing prerequisite that prevents required validation
- change would require expanding outside Allowed files
- task is larger than one issue / one PR / one primary purpose
```
