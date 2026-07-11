---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Preregistered independent-agent alternative when human recruitment is infeasible.
do_not_use_for: Claiming that agent evaluators are human participants or generalizing to human usability.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Preregistered independent-agent alternative when human recruitment is infeasible.
> Do not use for: Claiming that agent evaluators are human participants or generalizing to human usability.

# Independent agent-evaluator addendum

Human recruitment was attempted and the repository owner reported that no collaborators could be found. Before any alternative trial is run, the owner explicitly requested independent investigations/experiments that are materially different, candid, and not constructed to force success. This addendum freezes that alternative design.

## Scope and labeling

The study uses at least 8 fresh, isolated agent sessions and at least 96 trials. These are **agent evaluators, not human developers**. The result may establish diagnostic-information non-inferiority for independent code-reasoning agents and may unblock this beta cutover only under the owner's explicit alternative-design approval. It does not establish human preference or human task-time performance; that limitation remains in release notes.

## Independence and blinding

- Each evaluator starts with no conversation history (`fork_turns=none`).
- Evaluators receive neutral conditions A/B/C and are not told which is native, compatibility, or candidate.
- Each evaluator receives 12 different causal tasks and four trials per condition, with Latin-square rotation.
- The coordinator fixes task packets, expected repairs, condition mapping, and hashes before spawning evaluators.
- Evaluators may inspect only the supplied source and diagnostic packet. They record the first proposed edit before any compiler check.
- Evaluators time each trial with monotonic wall-clock timestamps around packet inspection and answer production.

## Anti-bias rules

- Every started trial is retained, including wrong edits, abandonment, parser failures, and unfavorable results.
- No task, condition, participant, or metric may be removed after results are visible except corrupt transport proven by a missing packet hash.
- Conditions use the same source defect and compiler invocation; only the diagnostic presentation differs.
- Tasks cover simple/noisy, one/two/three defects, C/C++, syntax/type/overload/template/macro/linker/unknown/fallback, and real-project multi-TU contexts.
- The candidate is not allowed a task-specific prompt, extra source, or extra compiler fact unavailable to native GCC.

## Frozen outcomes and verdict

Primary correctness metrics and non-inferiority margins remain those in `analysis-plan.json`. Time is reported as agent wall-clock latency and analyzed separately from human time. A high-confidence misleading edit is stop-ship. The result is `pass` only if correctness, first-fix success, multi-defect target selection, and agent-time non-inferiority confidence intervals all pass. Any missing session, fewer than 96 trials, condition imbalance, packet hash mismatch, or interval crossing a margin yields `inconclusive` and blocks promotion.

The final report must disclose evaluator type, model/session isolation, all failures, scenario-level raw/explain requests, and the absence of human generalizability.
