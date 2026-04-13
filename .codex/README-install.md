# gcc-formed Codex quality solution

This kit is for one specific problem:

> Cloud Codex writes a plausible patch too quickly, reports completion too early, and only later would CI or deeper review find the gaps.

The solution is to combine **instruction gating**, **evidence gating**, **local validation gating**, and **publish gating**.

## Included files

### Core repo policy
- `AGENTS.md`
- `Codex.md`
- `.codex/templates/work-package-template.md`
- `.codex/QUALITY-GATE-STRENGTHENING.md`

### Codex enforcement
- `.codex/config.toml`
- `.codex/hooks.json`
- `.codex/gate-rules.json`
- `.codex/quality_gate_common.py`
- `.codex/check_sync_requirements.py`
- `.codex/run_quality_gate.py`
- `.codex/hooks/pre_tool_use_guard.py`
- `.codex/hooks/stop_quality_gate.py`
- `.codex/templates/work-plan-template.md`
- `.codex/templates/self-review-template.md`

### CI-parity gate
- `ci/run_pr_gate_local.sh`

### Optional branch CI example
- `.github/workflows/codex-branch-gate.example.yml`

## Install steps

1. Replace the repository-root `AGENTS.md` with this kit's `AGENTS.md`.
2. Copy the entire `.codex/` directory into the repo root.
3. Copy `ci/run_pr_gate_local.sh` into the repo and make it executable:

```bash
chmod +x ci/run_pr_gate_local.sh
chmod +x .codex/run_quality_gate.py
chmod +x .codex/check_sync_requirements.py
chmod +x .codex/hooks/pre_tool_use_guard.py
chmod +x .codex/hooks/stop_quality_gate.py
```

4. Start using `.codex/templates/work-package-template.md` for cloud Codex tasks.
5. Optionally add the example branch workflow as a real workflow.
6. Configure GitHub branch protection / rulesets for `main`.

## Required developer habit

For any non-trivial Codex task, the execution order should be:

1. create `.codex/evidence/work-plan.md`
2. implement the change
3. create `.codex/evidence/self-review.md`
4. run `python3 .codex/run_quality_gate.py`
5. only then allow `STATUS: DONE`, push, or PR creation

## Why the heavy gate matters

If `ci/run_pr_gate_local.sh` exists, non-doc changes escalate to a heavy parity gate.
That is intentional.

A coding agent should not optimize for the fastest possible green light.
It should optimize for:
- surviving CI
- surviving code review
- not drifting from docs / ADR / support contracts
- not creating a "looks okay until morning" branch

## Recommended GitHub settings

For `main`, enable:
- require pull request before merging
- require status checks before merging
- require branches to be up to date before merging
- require at least one approving review
- require Code Owners review if you have a team
- no bypass for the automation identity

## Notes

- Hooks are an extra guardrail, not a substitute for clear task scoping.
- The strictest gain comes from combining:
  - issue-based work packages
  - strong `AGENTS.md`
  - repo-local gate scripts
  - stop / publish hooks
  - branch CI for Codex branches
  - merge protection on `main`
