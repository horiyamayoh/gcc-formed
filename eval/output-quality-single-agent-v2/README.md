---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: agent
use_for: Source-disjoint v2 single-agent qualification after the immutable v1 failure.
do_not_use_for: Reinterpreting or pooling v1 results, or human behavioral claims.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Source-disjoint v2 qualification after the immutable v1 failure.
> Do not use for: Reinterpreting or pooling v1 results, or human behavioral claims.

# Output-quality single-agent qualification v2

V1 attempt 3 completed all 360 trials and failed. That packet remains
immutable. Before any v2 sealed task is materialized, this epoch fixes three
evaluation defects found in v1: compiler-driver stdout is excluded from the
agent diagnostic, untracked source files are recorded individually with patch
content, and a newly created header matching the sealed repair token is a valid
repair target.

V2 uses family IDs `F121` through `F240`. Their generated source tokens and
paths are disjoint from v1 `F001` through `F120`. Results are never pooled
across protocol versions. The protocol, analyzer, manifest, attestation,
prompt, schema, harness wrapper, candidate SHA, and binary hash must be frozen
before generation.

The controller deliberately reuses the frozen v1 generator and analyzer while
overriding only the preregistered v2 epoch and evidence corrections:

```bash
python3 eval/output-quality-single-agent-v2/harness.py validate-static
python3 eval/output-quality-single-agent-v2/harness.py generate-corpus \
  --output target/output-quality/qualification-v2 \
  --attempt 1 \
  --candidate-sha "$sha" \
  --formed-binary target/release/gcc-formed \
  --candidate-presentation repair_units_hybrid_v2
python3 eval/output-quality-single-agent-v2/harness.py run \
  --packet-root target/output-quality/qualification-v2 --jobs 1
python3 eval/output-quality-single-agent-v2/harness.py analyze \
  --packet-root target/output-quality/qualification-v2
python3 eval/output-quality-single-agent-v2/harness.py verify \
  --packet-root target/output-quality/qualification-v2
```

The same named `gpt-5.6-sol` / `xhigh` coding-agent identity performs every
trial with jobs fixed to one. Subagents, delegation, ensembles, judge agents,
best-of-N, and result-based threshold changes are prohibited. The evidence
measures coding-agent task performance and deterministic readability contract
proxies; it is not a human behavioral study.
