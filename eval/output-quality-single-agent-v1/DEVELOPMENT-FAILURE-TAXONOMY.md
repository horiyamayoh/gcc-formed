---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Development-only baseline findings that led to the final compact hybrid candidate.
do_not_use_for: Qualification claims or combining development data with sealed trials.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Development-only baseline findings that led to the final compact hybrid candidate.
> Do not use for: Qualification claims or combining development data with sealed trials.

# Development failure taxonomy

This report is development evidence only. Its generated sources and diagnostics
are excluded from the sealed qualification denominator.

## Baseline findings

| Failure class | Native GCC | `subject_blocks_v2` | Initial hybrid | Corrective decision |
| --- | --- | --- | --- | --- |
| simple source salience | source/caret is immediate | raw message, evidence, and excerpt repeat the same anchor | native-style gutter was added but duplicates remained | retain one concrete headline and one source/caret excerpt |
| syntax recovery | compact primary parse error, but follow-on warnings remain | each residual card repeats raw source evidence | disclosure was repeated once per card | keep one block per visible RepairUnit and one session disclosure |
| overload/template flood | long candidate/template lists dominate | semantic grouping helps, but multiline raw titles remain large | raw multiline titles were followed by a second excerpt | keep the first concrete title line and move the rest to `--formed-explain` / `--formed-raw` |
| linker diagnostics | native is very short but lacks source caret | symbol/from slots improve anchoring | per-card disclosure exceeded the native byte budget | retain symbol/from evidence and emit disclosure once |
| partial/residual honesty | raw text exposes uncertainty implicitly | explicit partial notice is honest but precedes the action | same notice delayed the first action | put the action first and retain the partial/raw notice at session end |
| agent output schema | tool-level schema works when the turn completes | same | same | invalid/absent final JSON remains a trial failure; prose never rescues it |
| agent transport capacity | not applicable | not applicable | attempt 2 started 237 turns after the pinned transport quota was exhausted | retain all failures, fail the attempt, and stop on the first non-retryable capacity error in future runs |

## Development metric result

On 120 source-distinct family blocks / 360 generated diagnostics outside the
sealed partition, the compact candidate averaged `519.85` diagnostic bytes,
versus `629.16` for native GCC and `984.50` for `subject_blocks_v2`. The paired
family-bootstrap candidate ratios were:

- native GCC: estimate `0.8263`, 97.5% interval `[0.7524, 0.9174]`;
- `subject_blocks_v2`: estimate `0.5280`, 97.5% interval `[0.4994, 0.5619]`.

The strengthened deterministic display checks passed 120/120 concrete
headlines, primary anchors, first-action budgets, and one-step disclosures;
90/90 cases where native exposed source/caret retained both; all 40 simple
families met the native first-screen action budget; and all 40 noisy families
placed the first action at least 20% earlier when native had pre-action text.

These measurements were used only to choose and freeze the next candidate.
They are not qualification evidence and make no human behavioral claim.
