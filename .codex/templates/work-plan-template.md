# Work plan

## Goal
One primary purpose only.

## Non-goals
What this task will explicitly not change.

## Current-authority docs read
- `README.md`
- `docs/support/SUPPORT-BOUNDARY.md`
- `docs/process/EXECUTION-MODEL.md`
- relevant ADR(s)
- relevant spec(s)

## Allowed files
- exact files or directories

## Forbidden surfaces
- exact files or directories

## Risks
- contract drift
- renderer regression
- fallback honesty
- support-boundary drift
- release/install drift
- other task-specific risks

## Validation plan
- `python3 .codex/run_quality_gate.py`
- any extra path-specific checks

## Sync obligations
- docs
- ADR
- changelog
- corpus / snapshots
- templates / workflows

## Stop conditions
- missing prerequisite
- conflicting current-authority docs
- scope expansion outside Allowed files
- task grows beyond one issue / one PR / one primary purpose
