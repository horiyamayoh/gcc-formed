---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: RepairUnit exact-count corpus coverage and review rules.
do_not_use_for: Runtime grouping behavior or family completeness claims.
supersedes: []
superseded_by: []
---

> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: RepairUnit exact-count corpus coverage and review rules.
> Do not use for: Runtime grouping behavior or family completeness claims.

# RepairUnit exact-count corpus coverage

The authoritative artifact is `corpus/repair-unit-exact-count/repair-unit-coverage.json`; snapshots are secondary.

| Dimension | Evidence |
|---|---:|
| single-defect fixtures | 12 |
| two-defect fixtures | 10 |
| three-defect fixtures | 6 |
| false-split traps with multiple raw evidence | 11 |
| false-merge traps with distinct repairs | 16 |
| languages | C, C++ |
| shapes | syntax recovery, semantic/type, overload, template, macro definition/use, adjacent same-family, linker two-symbol |
| version evidence | GCC13 direct on the recorded host; GCC12/GCC14/GCC15 representative behavior justified by the repository's live matrix policy and replay contract |

The exact-count denominator includes only defects marked observable in the current invocation. A later defect hidden by compiler recovery is recorded `observable = false` and excluded. Interaction/composite repairs must set `independently_applicable = false` and share an `interaction_group`; they are never forced into independent units.

Two repairs may be declared independent only when each applies to the failing baseline, removes its own stable causal signature without removing the other's signature, the fully repaired program succeeds, and forward/reverse application yields the same compiler outcome. Review records a stable defect ID, owner, reviewer, repair anchor, language, shape, trap kind, and version evidence.

The baseline report records raw top-level count, raw evidence count, oracle RepairUnit count, current formed visible block count, and an `exact`, `false_split`, `false_merge_or_hidden`, or `formed_unavailable` classification for every case. Current drift is evidence to fix later; expected oracle counts are never changed to match runtime output.
