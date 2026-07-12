---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: agent
use_for: Single-agent output-quality qualification protocol, execution, and evidence layout.
do_not_use_for: Human behavioral claims or reinterpretation of historical studies.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Single-agent output-quality qualification protocol, execution, and evidence layout.
> Do not use for: Human behavioral claims or reinterpretation of historical studies.

# Output-quality single-agent qualification v1

This root implements ADR-0039 and issue #219. It evaluates native GCC, the
current no-configuration default, and one frozen candidate using a single
pinned coding agent in fresh isolated contexts. Every trial uses a real source
tree, an actual edit/build loop, a concealed condition, deterministic scoring,
and retained raw artifacts.

The checked-in `protocol.json` and `analysis-plan.json` are frozen before the
candidate and qualification results. `model-agent-tool-manifest.json` pins the
agent identity and resource policy. `no-subagent-attestation.json` is both a
machine-readable prohibition and the final run attestation template.

The controller is `harness.py`:

```bash
python3 eval/output-quality-single-agent-v1/harness.py validate-static
python3 eval/output-quality-single-agent-v1/harness.py generate-corpus \
  --output target/output-quality/qualification \
  --attempt "$attempt" \
  --candidate-sha "$sha" \
  --formed-binary target/debug/gcc-formed \
  --candidate-presentation repair_units_hybrid_v1
python3 eval/output-quality-single-agent-v1/harness.py run \
  --packet-root target/output-quality/qualification --jobs 1
python3 eval/output-quality-single-agent-v1/harness.py analyze \
  --packet-root target/output-quality/qualification
python3 eval/output-quality-single-agent-v1/harness.py verify \
  --packet-root target/output-quality/qualification
```

`--jobs` is fixed to `1`; parallel agent execution, subagents, judge agents,
ensembles, and best-of-N are rejected. Infrastructure tools such as compilers,
test runners, parsers, hashers, and the deterministic analyzer are not agents.

## Evidence packet

An RC packet contains:

- `protocol.json`, `analysis-plan.json`, `model-agent-tool-manifest.json`
- `no-subagent-attestation.json`, `corpus-manifest.json`
- `seed-commitment.json`, `candidate-freeze.json`, `condition-key.json`
- `trial-index.jsonl` and `trials/<trial-id>/...`
- `artifact-integrity-report.json`, `fidelity-report.json`
- `repair-utility-report.json`, `efficiency-report.json`
- `human-readable-contract-report.json`
- `qualification-report.json`, `qualification-summary.md`
- `default-promotion-decision.md`

The condition key is revealed only after all started-trial artifacts are frozen
and their Merkle root is recorded. Failed and interrupted trials remain in the
index. A failed or inconclusive report is never rewritten; a product change
requires a new candidate SHA and the next preregistered disjoint partition.
The controller stops after retaining a non-retryable transport-capacity failure
instead of starting the rest of a partition when they cannot execute. Before
materializing the final partition, operators must run an out-of-packet pinned
agent smoke and confirm enough transport capacity to complete all 360 trials.

## Claim boundary

This study measures coding-agent task performance. The human-readable report is
a deterministic contract proxy for source/caret, first action, information
budget, progressive disclosure, and honest fallback. Neither is a human
behavioral study, and neither supports a claim about human latency or preference.
