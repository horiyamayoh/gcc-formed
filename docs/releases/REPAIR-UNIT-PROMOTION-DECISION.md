---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current disposition of the RepairUnit beta-default cutover.
do_not_use_for: Claiming the RepairUnit semantic core or preview was removed.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current disposition of the RepairUnit beta-default cutover.
> Do not use for: Claiming the RepairUnit semantic core or preview was removed.

# RepairUnit default-promotion decision

## Outcome

The #218 cutover is rejected for the current beta release.  No-config
`gcc-formed` and `g++-formed` continue to use the documented
`subject_blocks_v2` beta presentation.  There is no migration or rollback event
to perform because the proposed default change was not shipped.

RepairUnit inference, lossless evidence, public export, causal corpus, quality
gates, one-unit renderer capability, raw/explain disclosure, and real-project
evidence remain in the product and test suite.  Family completeness and fixed
family counts do not regain correctness status; family remains optional
compatibility/presentation metadata.

## Acceptance audit

| #218 requirement | Evidence | Decision status |
|---|---|---|
| RepairUnit ADR and prerequisite implementation packages | #202--#216 reports and commits | satisfied |
| exact-count / false merge / false split / fact loss | quality reports from #214 | satisfied |
| real-project P0/P1 regression gate | #215 report | satisfied |
| zero-config/drop-in behavior | #216 evidence | satisfied for current shipped default |
| human evaluation reports `pass` | 0 recruited humans; two agent studies are `inconclusive` | **not satisfied** |
| no stop condition is active | #217 is inconclusive | **not satisfied** |
| change no-config default to RepairUnit | forbidden while stop condition is active | **not performed** |
| cutover package/rollback transcript | no cutover artifact exists | not applicable to rejected release change |

The unchecked requirements are not waived or marked true.  Closing #218 records
a reviewed no-go outcome under its own stop conditions.

## Shipped behavior

- default terminal presentation: `subject_blocks_v2`
- previous presentation rollback: `--formed-presentation=subject_blocks_v1`
- legacy wording rollback: `--formed-presentation=legacy_v1`
- compiler-native disclosure: `--formed-raw`
- grouping/provenance disclosure: `--formed-explain`
- emergency behavior: existing fail-open raw/passthrough contract

No telemetry warning or deprecation is added because users are not being moved
to a new default.  Existing compatibility windows remain governed by ADR-0036.

## Reopening

See ADR-0038.  A future cutover is a new release decision and must not reopen
#218 merely to reinterpret its failed gate.  It needs new qualifying user
evidence, a fresh cutover issue, same-commit package/rollback evidence, and all
quality/CI gates green.
