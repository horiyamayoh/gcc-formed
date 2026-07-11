---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: RepairUnit identity, observable-defect scope, suppression proof, and migration decisions.
do_not_use_for: Historical score-first or family-completeness behavior.
supersedes: []
superseded_by: []
---

> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: RepairUnit identity, observable-defect scope, suppression proof, and migration decisions.
> Do not use for: Historical score-first or family-completeness behavior.

# ADR-0037: RepairUnit is the product identity

- Status: **Accepted**
- Date: 2026-07-11
- Issue: #202

## Decision

A `RepairUnit` is the evidence from the current compiler invocation that is judged to be resolved by the same minimal edit, or evidence kept separate because independence cannot be disproved. It replaces diagnostic family, emission episode, and root score as the identity of one user-visible item.

The scope is observable evidence from the current invocation. The runtime does not infer defects GCC did not emit and never edits source or recompiles counterfactually. Counterfactual compilation belongs only to the corpus oracle.

Relations use `proven`, `strong`, `tentative`, and `unresolved` confidence. `tentative` and `unresolved` evidence remains visible by default. Suppression carries the proof obligation: family equality, message substring, file equality, proximity, order, or score alone cannot merge or hide evidence.

Family classification remains optional presentation/extraction metadata. Unknown family is neither unsupported nor suppressible. Family count, pattern count, and unknown rate are not correctness or completion metrics.

Default failure output renders one block per visible RepairUnit. Supporting evidence may be compacted inside that block, while every node remains traceable to immutable raw capture facts.

## Normative definitions

- **observable defect**: an independently actionable defect distinguishable from evidence emitted in the current invocation.
- **false merge**: evidence belonging to independently repairable oracle defects placed in one RepairUnit.
- **false split**: evidence resolved by one oracle repair divided into multiple visible RepairUnits.
- **fact loss**: a compiler-owned fact without a reversible path from a visible RepairUnit or raw disclosure.
- **suppression proof obligation**: the requirement that hiding a node is justified by structural evidence linking it to exactly one visible RepairUnit.

## Invariants

1. Raw facts are immutable; analysis is additive.
2. Every RepairUnit links to raw capture references.
3. Every hidden node is reachable from exactly one visible RepairUnit.
4. Uncertain and unclassified evidence is visible.
5. Actions require grounded evidence.
6. Visible block count equals visible RepairUnit count.
7. Native GCC source emphasis is a non-regression baseline.

## Type and data-flow migration

`CaptureBundle -> DiagnosticDocument(raw nodes/hierarchy/fix-its/ranges) -> DiagnosticEvidenceGraph -> RepairUnit inference -> RepairUnitView -> terminal/public export`.

`DiagnosticEpisode` and `GroupCascadeAnalysis` remain compatibility inputs and shadow-comparison outputs during migration. They cannot decide final identity, support, fallback, or visibility. Rollback keeps `subject_blocks_v2`, legacy score-first presets, and raw/passthrough independently selectable for at least one beta compatibility window.

## Worked examples

1. Missing semicolon: parser error, fix-it, and recovery notes share the fix-it/recovery region and form one unit.
2. Two independent errors: separate edit spans without structural linkage remain two units even when adjacent or in the same family.
3. Template candidate flood: the failing call is the unit anchor; candidate and instantiation frames remain evidence inside it, unless another call-site frontier proves a second unit.
4. Duplicate symbol: linker definitions keyed by the same symbol form one unit; `collect2` is a driver summary inside it, while another symbol remains separate.

## Conflict check

- ADR-0002 remains valid: the evidence graph and RepairUnit are additive IR layers.
- ADR-0006 remains valid: raw fallback and provenance are mandatory.
- ADR-0010 remains valid: inference is deterministic and offline.
- ADR-0019/0034/0036 remain rollback compatibility contracts, but episode/family presentation is not product identity.
- ADR-0030 remains valid: analysis, view, and theme stay separated.
- ADR-0031 is strengthened by the native-source-emphasis invariant.
- No accepted ADR promises family completeness; any non-authoritative score/family-completeness wording is superseded by this decision once accepted.

## Consequences and non-goals

The corpus moves toward repair patches and causal labels; inference favors separation under uncertainty; the renderer consumes RepairUnits. The project does not claim all source defects, run runtime repair/recompile, require an AI classifier, or translate every GCC message.

## Acceptance record

- Authoring and conflict check: complete
- Human reviewer: repository owner (`horiyamayoh`)
- Review decision/date: **Approved, 2026-07-11**

Approval authorizes #203–#218 to use this ADR as current authority without further migration pauses.
