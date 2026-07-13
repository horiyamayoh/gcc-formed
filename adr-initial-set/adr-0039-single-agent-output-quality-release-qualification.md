---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current output-quality qualification and release prerequisite.
do_not_use_for: Claiming human behavioral validation or rewriting prior no-go evidence.
supersedes:
  - adr-0038-repair-unit-default-promotion-deferred.md
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current output-quality qualification and release prerequisite.
> Do not use for: Claiming human behavioral validation or rewriting prior no-go evidence.

# ADR-0039: Single-agent output-quality qualification replaces mandatory human recruitment

- Status: **Accepted**
- Date: 2026-07-12
- Issue: #219
- Supersedes: ADR-0038 only for future release prerequisites

## Decision

Release qualification no longer depends on recruiting human participants or on
a mandatory `human-eval/` packet. A release candidate instead requires one
preregistered, sealed `agent-output-quality/` qualification performed by a
single pinned coding-agent identity and configuration. The qualification uses
real compiler diagnostics, actual source edits and build/test loops, blinded
conditions, deterministic scoring, family-clustered intervals, complete trial
retention, and artifact hash verification.

The same named coding agent owns protocol design, implementation, development
and validation work, candidate freeze, qualification execution, analysis, and
release evidence. Fresh context and a clean sandbox per trial are isolation
controls, not delegation. Subagents, planner/critic/judge agents, ensembles,
model voting, best-of-N selection, post-result exclusions, seed rerolls, and
overwriting failed trials are prohibited.

The required comparison has three concealed conditions: native GCC, the current
no-configuration default, and the frozen candidate. Candidate promotion is
allowed only when all fidelity stop-ship checks pass, utility and efficiency are
non-inferior to both controls under the preregistered margins, at least one
preregistered improvement criterion passes, and the human-readable contract
proxy is green across the required compiler/path matrix. If the candidate does
not pass, the current default remains in place.

## Claim boundary

The permitted public claim is:

> Under a pinned single coding agent, real compiler and patch/build loops,
> sealed holdout tasks, and deterministic scoring, the selected default did not
> regress safety, repair success, or preregistered efficiency relative to native
> GCC and the previous default. Its source/caret, first-action, information
> budget, progressive-disclosure, and fallback contracts passed deterministic
> checks.

The project MUST NOT claim that a human behavioral study passed, that human
edit latency or preference improved, or that AI trials are observations of a
human population. Future human feedback remains welcome research input, but it
is not a `1.0.0` release blocker.

## Required evidence and gate

The current authority root is `eval/output-quality-single-agent-v2/`. It must
contain the frozen protocol, analysis plan, model/agent/tool manifest,
no-subagent attestation, corpus and seed commitments, candidate freeze, trial
index, integrity report, fidelity report, repair-utility report, efficiency
report, human-readable-contract report, qualification report and summary, and
the default-promotion decision. Per-trial source, diagnostic, transcript,
tool-call, patch, build/test, and oracle artifacts may be retained in the RC
packet rather than Git history, but their hashes and the packet Merkle root must
be committed before condition reveal.

`cargo xtask rc-gate` treats a missing, incomplete, hash-inconsistent, failed,
or inconclusive `agent-output-quality/qualification-report.json` as a blocker.
The historical `eval/repair-units-v1/` material remains immutable and optional
for research; it is not imported into new qualification denominators.

## Relationship to ADR-0038

ADR-0038 was correct for its release and evidence. Its empty human collection,
inconclusive agent studies, no-promotion decision, and issue dispositions stay
historical truth. This ADR does not relabel those results. It supersedes only
ADR-0038's rule that future reconsideration requires recruited human
participants or external evaluators.

## Consequences

- Release completion no longer waits on collaborator recruitment.
- Evaluation cost moves into reproducible single-agent trials and artifact
  retention.
- Human readability is governed by explicit deterministic proxies and a narrow
  public claim, not by simulated human preference.
- Model, agent build, instruction, tool policy, or resource drift invalidates a
  qualification and requires a fresh attempt on a preregistered disjoint
  partition.
- A failed or inconclusive candidate still blocks promotion and release.
