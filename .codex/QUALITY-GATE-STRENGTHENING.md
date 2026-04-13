# Quality Gate Strengthening Plan

This package is designed to stop the common failure mode where a cloud coding agent writes a plausible patch, runs one shallow check, and declares success.

## What this solution changes

### 1) It turns "done" into an evidence-backed state
Codex must create:
- `.codex/evidence/work-plan.md`
- `.codex/evidence/self-review.md`
- `.codex/evidence/quality-gate.json`

That makes the agent do three separate passes:
1. plan
2. implement
3. review + validate

This is the simplest practical way to make "think harder" observable.

### 2) It makes the repo-local gate the definition of completion
The authoritative gate becomes:

```bash
python3 .codex/run_quality_gate.py
```

That script checks:
- required evidence files
- repo-specific sync rules
- docs-only vs non-doc classification
- full CI-parity gate when `ci/run_pr_gate_local.sh` exists
- stale-validation detection using a diff snapshot hash

### 3) It blocks premature push / PR / release actions
`.codex/hooks/pre_tool_use_guard.py` denies:
- `git push`
- `gh pr create`
- `gh pr merge`
- tags and release commands
- publish commands

unless the quality gate passed and the workspace has not changed since validation.

### 4) It blocks fake `STATUS: DONE`
`.codex/hooks/stop_quality_gate.py` automatically continues the turn if Codex tries to emit `STATUS: DONE` without fresh validation evidence.

### 5) It upgrades non-doc work from "local subset" to "CI-parity" when possible
If `ci/run_pr_gate_local.sh` exists, the local gate uses it for any non-doc change.
That script mirrors the current PR workflow much more closely than a small local subset.

This is the biggest lever for turning a 5-minute shallow patch into a deeper pass.

## Recommended rollout order

### Phase A — immediate
Install:
- `AGENTS.md`
- `.codex/`
- `ci/run_pr_gate_local.sh`

This alone sharply reduces premature completion.

### Phase B — make remote branch work visible
Add `.github/workflows/codex-branch-gate.example.yml` as a real workflow, renamed as needed.

Why: the current repo PR workflow runs on pull requests and pushes to `main`. A remote Codex branch can therefore look "done" without a branch CI run unless you add a branch workflow.

Recommended branch patterns:
- `codex/**`
- `agent/**`
- `wip/**`

### Phase C — protect merge quality
In GitHub rulesets / branch protection:
- require pull request before merge
- require status checks before merge
- require branch to be up to date before merge
- require at least one approving review
- require Code Owners review for critical surfaces if you have a team
- do not give the Codex actor a bypass path

## Suggested required checks

For `main`:
- `pr-gate`

For agent branches:
- `codex-branch-gate` (informational / early warning)

## Recommended human ownership boundaries

If you use CODEOWNERS, the best places to require human review are:
- `.github/workflows/**`
- `xtask/**`
- `docs/specs/**`
- `docs/support/**`
- `diag_public_export/**`
- release / install surfaces

## How this changes Codex behavior in practice

Without these files:
- the agent can patch
- maybe run one command
- maybe stop early
- maybe push a branch that has never seen CI

With these files:
- it must plan first
- it must self-review
- it must pass a repo-local gate
- it cannot push or create a PR before that
- it cannot keep `STATUS: DONE` if the gate is missing or stale

## Stronger follow-up improvements for this repo

These are not required for the first rollout, but they are the next best upgrades.

### Add a single repo-native xtask entrypoint
Create something like:

```bash
cargo xtask pr-gate-local
```

Then make:
- `ci/run_pr_gate_local.sh`
- local maintainers
- Codex
- future docs

all point to the same one-liner.

This removes drift between:
- PR workflow YAML
- local scripts
- agent instructions

### Add workflow lint for workflow edits
The local gate already supports requiring `actionlint` when workflow files change.
Install `actionlint` in the Codex image so workflow changes cannot pass with only YAML syntax luck.

### Add path-sensitive sync rules over time
The current `.codex/gate-rules.json` already enforces some high-confidence sync rules.
You can keep adding more as patterns stabilize.

Good candidates:
- support-boundary wording copies
- public-surface wording
- release-body generation inputs
- install / rollback docs vs xtask behavior
- corpus / snapshot promotion rules

## Failure mode this package intentionally accepts

No static gate can literally force "40 minutes of thinking".
What it can do is make shallow work expensive to finalize:
- missing plan blocks completion
- missing review blocks completion
- missing or stale validation blocks completion
- publish actions are blocked
- remote branch CI can be added for early red/green feedback

That is the right kind of structural pressure.
