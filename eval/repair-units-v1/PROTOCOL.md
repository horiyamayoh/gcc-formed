---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Preregistered blinded RepairUnit first-correct-edit study and analysis plan.
do_not_use_for: Substituting automated corpus results for human trials.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Preregistered blinded RepairUnit first-correct-edit study and analysis plan.
> Do not use for: Substituting automated corpus results for human trials.

# RepairUnit blinded first-correct-edit protocol

This protocol is frozen before trial collection. Conditions A/B/C are assigned by the checked-in Latin-square allocation and are revealed only after the anonymized dataset is locked. They correspond to native GCC, the beta compatibility view, and the RepairUnit candidate in a separately held condition key.

## Population and power rule

Collect at least 80 valid trials from at least 8 participants who self-attest to real C/C++ compiler-diagnostic experience. Each participant completes counterbalanced simple/noisy, one/multiple-defect, C/C++, compile/link tasks. Exclude only tool failure, incomplete consent, or a preregistered interruption; retain unsuccessful and abandoned tasks.

## Frozen primary outcomes

- milliseconds to first correct source edit;
- first-edit correctness;
- first-fix compile improvement/success;
- multi-defect repair-target selection accuracy;
- irrelevant lines inspected;
- raw/explain request count.

The candidate passes only if simple one-defect median time is no worse than native by more than 10%, overall first-edit correctness and first-fix success are non-inferior by a 5 percentage-point margin, and multi-defect target accuracy is non-inferior by 5 points. Any high-confidence misleading action/location is stop-ship. Bootstrap 95% confidence intervals are computed with a checked-in deterministic seed. An interval crossing a non-inferiority margin yields `inconclusive`, which blocks promotion.

## Privacy and blinding

Store a random study-local participant code only. Do not collect name, email, repository path, source outside the distributed tasks, shell environment, or free-form sensitive telemetry. Export hashes participant codes with a study salt and removes free text. The evaluator records the source edit, compiler exit/outcome, selected repair target, and interface requests; preference is collected only after objective trials.

## Task coverage

The allocation covers missing semicolon; two/three syntax defects; argument count/type; overload flood; similar calls; template/concept depth; macro definition/invocation; one/multiple linker units; unknown structured evidence; and low-information fallback. Causal repairs come from the checked-in oracle corpus and may not be changed after collection begins.
