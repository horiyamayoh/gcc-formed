---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: agent
use_for: Authoring and verifying counterfactual repair-oracle fixtures.
do_not_use_for: Runtime source modification or automatic repair.
supersedes: []
superseded_by: []
---

> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Authoring and verifying counterfactual repair-oracle fixtures.
> Do not use for: Runtime source modification or automatic repair.

# Counterfactual repair oracle

The oracle is test-only. It copies each fixture into a private temporary directory, applies reviewed unified-diff repairs there, invokes the declared compiler, and writes canonical `causal-map.json`. It never edits the fixture source or a user tree.

Run all fixtures:

```bash
cargo xtask repair-oracle --root corpus/repair-oracle --check
```

Run one fixture by stable ID:

```bash
cargo xtask repair-oracle --root corpus/repair-oracle --fixture repair-oracle/double --check
```

`repair-oracle.toml` declares stable `defect_id`, patch, independent applicability, optional `interaction_group`, observability, anchors, compiler, and arguments. Non-independent repairs must name an interaction group. Alignment fingerprints normalized diagnostic content rather than matching full GCC messages or locations. Baseline, each single repair, full repair, disappeared/appeared evidence, exit status, command, ambiguity, and patch-order stability are recorded.

Review requires an empty ambiguity list for observable independent defects, successful full repair, deterministic `--check`, and an unchanged fixture source tree. `CascadeExpectations` root count may be projected only as a compatibility seed; it is not causal proof.
