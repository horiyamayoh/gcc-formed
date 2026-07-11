# Prospective agent-evaluator replication v2

This replication was frozen after the first confirmatory study returned
`inconclusive` solely because the simple-task time confidence interval was too
wide.  It must not replace, exclude, or modify any v1 observation.

- 24 fresh isolated sessions (S12--S35), 12 trials each, 288 trials total.
- All 12 fixtures are disjoint from v1 and represent different source surface
  forms.  They cover C/C++, one/two/three defects, repeated diagnostics,
  multiple translation/build contexts, warning-as-error, generated input,
  linker failures, and system-header frontier noise.
- Assignment remains blinded A/B/C, four trials per condition per session, with
  Latin-square rotation.
- Every started trial is retained.  Packet/condition hashes are frozen before
  collection.  The condition mapping is unchanged but remains unopened while
  v2 data are collected.
- Primary analysis is performed on v2 alone with the exact v1 metrics, margins,
  seed policy, and unpaired trial bootstrap.  A combined v1+v2 analysis is
  secondary and may not rescue a failing or inconclusive v2 result.
- Passing requires the v2 97.5% upper confidence bound for the candidate/native
  median simple-task time ratio to be at most 1.10; correctness and multi-target
  lower bounds must remain within 5 percentage points; high-confidence
  misleading edits must be zero.
- These remain agent evaluators, not humans.  No human-usability generalization
  is permitted.

The repository owner previously approved independent agent experiments without
additional migration approval.  This replication increases power without
changing an analysis rule after observing condition results.
