---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: RepairUnit default-promotion status, evaluation decision, and reconsideration conditions.
do_not_use_for: Rejecting RepairUnit as semantic IR or weakening causal quality gates.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: RepairUnit default-promotion status, evaluation decision, and reconsideration conditions.
> Do not use for: Rejecting RepairUnit as semantic IR or weakening causal quality gates.

# ADR-0038: Keep RepairUnit out of the beta default without user evidence

- Status: **Accepted**
- Date: 2026-07-12
- Issues: #217, #218, #201

## Decision

`RepairUnit` remains the product's semantic diagnostic identity, lossless IR,
causal quality model, explicit preview, and machine-readable foundation.  It is
**not promoted to the no-configuration terminal default in this release**.
`subject_blocks_v2` remains the beta runtime default and the existing explicit
RepairUnit, raw, explain, passthrough, and rollback selections remain available.

This decision overrides only the immediate default-promotion sentence in
ADR-0037.  ADR-0037's identity, observability, inference, fidelity, visibility,
and one-unit rendering contracts remain accepted.  It does not restore family
count, message patterns, or score-first grouping as product-correctness claims.

## Evidence considered

1. The preregistered human study required at least 8 experienced C/C++
   participants and 80 valid trials.  The owner requested collaborators in both
   personal and company contexts; no participant agreed to take part.
2. The owner offered to act as the sole evaluator.  This is rejected as release
   evidence: an implementer knows the intended grouping, has seen the fixtures,
   and cannot remove expectation, learning, and carry-over effects from an N=1
   crossover.  More repeated trials would increase rows, not population validity.
3. Independent blinded agent study v1 retained 96 trials.  Correctness was
   descriptively non-regressed, but the preregistered simple-task time interval
   was 0.491--2.038 against a 1.10 upper non-inferiority margin.
4. A prospective disjoint-fixture replication retained every started session
   and analyzed 24 valid sessions / 288 trials.  The candidate/native simple
   median ratio was 1.083 with a 95% interval of 0.25--3.80.  It was again
   inconclusive.  Agent timing and response-schema variability prevent treating
   this as human usability evidence.
5. Automated causal corpus, fidelity, exact-count, fact-loss, real-project, and
   zero-configuration gates support the architecture but cannot measure human
   attention, search order, trust calibration, or time to a correct edit.

## Cognitive and release reasoning

Compiler-diagnostic use is an information-foraging task.  Performance depends
on salience, visual search, expertise-shaped scanning, working-memory load,
trust in compression, and familiarity with native GCC cues.  A system designer
or model can reason about those mechanisms and identify risks, but cannot infer
their population effect with a release-grade confidence bound from repository
structure alone.

The asymmetric cost favors non-promotion.  Keeping an explicit preview delays a
potential benefit while preserving evidence collection.  Promoting without the
required evidence changes every no-config interaction and risks slower or
misdirected edits.  The current default already has rollback and non-regression
evidence, so retaining it is the lower-risk reversible choice.

Model capability is not used as a proxy participant.  A stronger reasoning
model raises the expected quality of this governance decision and its audit,
but does not turn simulated timing into observations of human cognition.

## Issue disposition

- #217 completes with an **inconclusive / gate-not-satisfied** outcome, not a
  pass.  Its protocol, raw trials, exclusions, keys, scripts, and reports remain
  immutable evidence.
- #218 completes as **rejected for this release**.  No default switch is made.
- #201 completes as an architectural delivery with its final default-cutover
  phase explicitly rejected by this ADR.  Completed RepairUnit foundations are
  retained; the epic must not be described as proving human non-regression.

Closing these issues records a decision and stops an unsafe rollout.  It does
not make unchecked acceptance boxes true.

## Reconsideration rule

A future proposal must open a new issue and may supersede this decision only
with one of:

1. the frozen human protocol completed with at least 8 relevant participants
   and a `pass` recommendation; or
2. a new powered design reviewed before collection, with externally measured
   timing, independent evaluators representative of intended users, blinded
   conditions, retained failures, and confidence intervals inside all frozen
   non-inferiority margins.

One owner evaluation, preference polling, automated corpus success, model
self-timing, or a post-hoc analysis change is insufficient.  Any future cutover
must still rerun exact-count, fidelity, package, rollback, and CI gates on the
same commit proposed for promotion.

## Acceptance record

- Decision authority: delegated by repository owner to the acting engineering
  agent after recruitment failed in personal and company contexts.
- Decision: **do not promote; preserve the evidence and reversible preview**.
- Review date: **2026-07-12**.
