---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Final audited outcome of RepairUnit epic #201.
do_not_use_for: Claiming human non-inferiority or default promotion.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Final audited outcome of RepairUnit epic #201.
> Do not use for: Claiming human non-inferiority or default promotion.

# RepairUnit epic outcome

## Disposition

Epic #201 is concluded with **architectural delivery complete and release
cutover rejected**.  Work packages #197--#216 completed normally.  #217 and
#218 completed as documented no-go decisions after their release gate could not
be established.  This is a mixed outcome, not a claim that every original
checkbox became true.

## Completion-criteria audit

| Original criterion | Authoritative evidence | Status |
|---|---|---|
| RepairUnit meaning, observability, confidence, suppression proof | ADR-0037 and current IR/render specs | satisfied |
| lossless separation and round-trip of raw facts / derived units | #205, #206; IR validation and public-export tests | satisfied |
| family is not required for grouping, visibility, support | #207, #208; inference tests | satisfied |
| counterfactual repair/recompile oracle | #203 and `cargo xtask repair-oracle` | satisfied |
| curated one/two/three/flood/adjacent corpus | #204 and repair-unit exact-count corpus | satisfied |
| false merge/split/fact loss gates are zero | #214 quality report and CI gates | satisfied |
| uncertain/unclassified evidence stays visible | ADR-0037, #207, renderer tests | satisfied |
| one visible RepairUnit can render as one block with native source evidence | #212 semantic renderer assertions | satisfied as capability |
| one-operation raw/explain and traceability | #213 and disclosure tests | satisfied |
| normal operation does not require internal family/cascade vocabulary | #216 and zero-config evidence | satisfied |
| real-project and blinded evaluation prove native non-degradation | #215 passed; #217 human evidence absent and agent studies inconclusive | **not satisfied** |
| promote only after all gates, with rollback | gate prevented promotion; ADR-0038 retains current default | safety invariant satisfied; promotion not performed |

## Delivered value retained

- immutable compiler facts plus additive evidence graph and RepairUnit IR;
- deterministic evidence-constrained syntax, type, template, macro, and linker
  inference;
- exact-count causal oracle, false-merge/split/fact-loss gates, and
  real-project differential fixtures;
- one-unit rendering capability, honest unknown visibility, raw/explain/public
  export traceability, and drop-in wrapper operation across GCC 9--15 paths;
- explicit compatibility, raw, passthrough, and presentation rollback paths.

These are maintained product assets.  The absence of default promotion does not
authorize regression to family-table completeness or score-first grouping as a
correctness model.

## Undelivered claim

The project has not shown that intended human users reach a first correct edit
at a non-inferior speed with a RepairUnit default.  Zero human participants were
recruited.  A sole implementer evaluation was rejected as biased release
evidence, and two blinded agent datasets had confidence intervals crossing the
frozen time margin.  The no-config beta presentation therefore remains
`subject_blocks_v2`.

## Historical reconstruction

- #202--#216 and #197--#200 contain per-package implementation evidence.
- `eval/repair-units-v1/` contains the frozen human protocol, empty human
  result, both agent designs, all started raw records, condition keys, analyzers,
  exclusions, and reports.
- ADR-0038 records the delegated decision, cognitive rationale, alternatives,
  asymmetry analysis, and future reconsideration threshold.
- `docs/releases/REPAIR-UNIT-PROMOTION-DECISION.md` records the rejected #218
  cutover and exact shipped behavior.

A future effort must open a new epic or cutover issue.  It may cite the
completed architecture but must not rewrite #201, #217, or #218 as a successful
human-evaluated rollout.
