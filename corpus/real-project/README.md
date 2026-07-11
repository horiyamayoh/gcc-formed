# Real-project differential corpus

This directory is the reviewed, network-free realism layer for RepairUnit validation. The sources are purpose-built minimized extracts modeled on common Make, CMake, and direct-invocation topologies; no third-party source is copied. Every scenario declares CC0-1.0 redistribution, provenance, redaction, reviewer, repair owner, invocation-boundary policy, and inverse repair patches.

Run the deterministic gates with:

```bash
cargo xtask real-project-corpus verify
cargo xtask repair-oracle --root corpus/real-project --check
cargo xtask quality-report --root corpus/real-project --format json
```

## Harvest and promotion

1. Capture each compiler invocation separately, assigning an `invocation_id` before collecting parallel stderr. Never infer a RepairUnit across an unattributed byte stream.
2. Redact absolute paths, user identifiers, environment values, and source not approved for redistribution. Reject rather than partially sanitize uncertain material.
3. Reproduce without network access, minimize while retaining the build-system shape, and add one reviewed inverse patch per independent defect.
4. Record structural repair anchors and provenance. Raw-message equality is not defect identity; repeated output across TUs is one or several defects only as proven by the reviewed patches.
5. Run oracle and quality gates. Promote only when exact-count, fact coverage, visibility, license, and metadata audits pass.

Unknown and unresolved diagnostics remain in raw capture and public RepairUnit membership. They are coverage, not discardable noise.

## Triage artifacts

An accepted example is `direct-multi-tu-c/case-04`: two attributed TUs, two inverse patches, two visible units, and full raw membership. A rejected harvest is represented by `rejected-example.json`; it demonstrates the machine-readable reasons that prevent unreviewed or sensitive input from becoming a golden. False merge/split triage uses the fixture's `causal-map.json`, `scenario.json`, raw diagnostic fingerprints, repair anchors, and RepairUnit coverage record together.
