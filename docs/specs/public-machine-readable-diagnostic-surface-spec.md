---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Current implementation contract for the public machine-readable diagnostic surface.
do_not_use_for: Historical export drafts or ad hoc JSON examples.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Current implementation contract for the public machine-readable diagnostic surface.
> Do not use for: Historical export drafts or ad hoc JSON examples.

# Public Machine-Readable Diagnostic Surface

This document defines the public JSON export contract for deterministic consumers of `gcc-formed`.

The internal `Diagnostic IR` remains the product core. The public JSON export is a versioned projection of that internal model for CI, agents, wrappers, and other machine consumers. It is the public automation contract. It is not a promise that the internal IR, trace bundle, or terminal text are stable public interfaces.

## 1. Scope

This spec covers:

- the public JSON export emitted by `--formed-public-json=<sink>`
- safe stdout emission rules
- file emission rules
- deterministic serialization and compatibility expectations
- snapshot and CI handling of `public.export.json`

This spec does not cover:

- the internal `Diagnostic IR` schema
- trace-bundle layout
- terminal renderer text
- terminal presentation presets, display-family labels, or template IDs
- GitHub Release body or repo metadata

## 2. Emission Contract

`gcc-formed` exposes the public machine-readable surface via:

- `--formed-public-json=/path/to/file.json`
- `--formed-public-json=-`
- `--formed-public-json=stdout`

### 2.1 File sink

File emission is the default automation path.

- The wrapper must create parent directories when needed.
- The JSON payload must be canonical JSON with deterministic key ordering.
- File emission may be used for `render`, `shadow`, and runs that resolve to explicit `unavailable` exports.

### 2.2 Stdout sink

Stdout emission is allowed only when stdout is not needed for the compiler's own contract.

The wrapper must reject stdout export for invocations that are unsafe for a pure JSON stdout channel, including:

- passthrough mode
- compiler introspection-like runs
- commands that may legitimately write compiler output to stdout, such as `-E`, `-M`, `-MM`, `-o -`, and `-o-`

When stdout emission is allowed, the JSON export must be the only stdout payload for that invocation.

## 3. Top-Level Shape

The public export is a single JSON object with the following top-level fields.

| Field | Required | Meaning |
|---|---:|---|
| `schema_version` | yes | Public export schema version. Current value: `1.0.0-alpha.1`. |
| `kind` | yes | Export kind discriminator. Current value: `gcc_formed_public_diagnostic_export`. |
| `status` | yes | `available` or `unavailable`. |
| `producer` | yes | Wrapper identity that emitted the export. |
| `invocation` | yes | Stable invocation summary for the observed run. |
| `execution` | yes | Resolved band/path/support/fallback context. |
| `result` | only when `status = available` | Public diagnostic payload. |
| `unavailable_reason` | only when `status = unavailable` | Reason code for why an `available` payload was not produced. |

The export must not expose raw internal IR nodes wholesale. It must remain a projection with explicit public field names and semantics.

## 4. Nested Fields

### 4.1 `producer`

`producer` must contain:

- `name`
- `version`

### 4.2 `invocation`

`invocation` must contain:

- `exit_status`

It may additionally contain:

- `invocation_id`
- `invoked_as`
- `primary_tool`
- `language_mode`
- `wrapper_mode`

If present, `primary_tool` may contain:

- `name`
- `version`
- `component`
- `vendor`

### 4.3 `execution`

`execution` must contain:

- `version_band`
- `processing_path`
- `support_level`

It may additionally contain:

- `source_authority`
- `fallback_grade`
- `fallback_reason`
- `document_completeness`

Current `version_band` labels are the current-authority machine labels such as `gcc15_plus`, `gcc13_14`, `gcc9_12`, and `unknown`.

Current `processing_path` labels are current-authority machine labels such as:

- `dual_sink_structured`
- `single_sink_structured`
- `native_text_capture`
- `passthrough`

Current `support_level` labels are current-authority machine labels such as:

- `preview`
- `experimental`
- `passthrough_only`

### 4.4 `result`

When present, `result` must contain:

- `summary`
- `diagnostics`

`summary` contains count-level rollups such as:

- `diagnostic_count`
- `error_count`
- `warning_count`
- `note_count`

It may additionally contain cascade-derived counts such as:

- `independent_root_count`
- `dependent_follow_on_count`
- `duplicate_count`
- `uncertain_count`

Each entry in `diagnostics` is a recursive public diagnostic node. A diagnostic may contain:

- `severity`
- `phase`
- `semantic_role`
- `family`
- `headline`
- `message`
- `first_action`
- `confidence`
- `primary_location`
- `provenance_capture_refs`
- `suggestions`
- `related_diagnostics`

`family` remains the machine-facing diagnostic family. It is not a promise that terminal rendering will expose the same label, because display-family mapping is presentation policy rather than public export schema.

If present, `primary_location` contains:

- `path`
- `line`
- `column`
- `role`
- optional `ownership`

If present, each `suggestions[]` entry contains:

- `label`
- `applicability`
- `edits`

Each `edits[]` entry contains:

- `path`
- `start_line`
- `start_column`
- `end_line`
- `end_column`
- `replacement`

### 4.5 `unavailable_reason`

Current reason codes are:

- `introspection_like`
- `passthrough_mode`

Additional reason codes may be added additively in the future.

## 5. Determinism Rules

The public export must be deterministic for the same input, compiler band, processing path, and wrapper version.

- JSON object keys must be emitted in canonical sorted order.
- Array order must remain semantically stable.
- Volatile values such as wrapper version, invocation id, and tool version may be normalized in snapshot comparison, but the field layout itself must remain stable.
- Consumers must not need terminal text scraping to recover the same meaning.
- Presentation customization, template selection, and location-host decisions must not be required to interpret the export.

The canonical snapshot artifact for this surface is `public.export.json`.

## 6. Compatibility Rules

- Additive optional fields are allowed.
- Removing, renaming, or semantically redefining an existing public field requires a schema-version change and same-change doc updates.
- The public JSON surface must not silently inherit internal-only field names just because they exist in the internal IR.
- The public JSON surface must remain presentation-independent even when terminal text becomes subject-first, preset-driven, or otherwise configurable.
- Consumers should ignore unknown additive fields and anchor on the documented required fields first.

Report bundles may emit `public.export.schema-shape-fingerprint.txt` as a schema-shape compatibility sentinel. That sidecar is for review and gate logic; the golden snapshot remains `public.export.json`.

## 7. Snapshot and CI Rules

Promoted fixture snapshots should include `public.export.json` alongside IR, view, and rendered artifacts.

CI and automation consumers should validate the export as JSON before trusting it downstream.

```bash
python3 -m json.tool diagnostic-export.json >/dev/null
```

```bash
jq -e '
  .schema_version == "1.0.0-alpha.1"
  and .kind == "gcc_formed_public_diagnostic_export"
  and .execution.version_band != null
  and .execution.processing_path != null
  and .execution.support_level != null
' diagnostic-export.json
```

## 8. Usage Guidance

Preferred machine-consumption path:

```bash
gcc-formed --formed-public-json=artifacts/diagnostic.json -c src/main.c
```

Safe stdout path:

```bash
gcc-formed --formed-public-json=- -c src/main.c | jq '.execution.version_band'
```

Automation should prefer this public export over:

- scraping terminal stderr
- scraping any subject-first / preset-customized terminal block format
- parsing internal IR snapshots as if they were public
- mining trace bundles when a public export is sufficient

## 9. Alignment

This contract must stay aligned with:

- `README.md`
- `AGENTS.md`
- `SUPPORT.md`
- `ci/README.md`
- `docs/specs/diagnostic-ir-v1alpha-spec.md`
- `docs/specs/rendering-ux-contract-spec.md`
- `docs/specs/quality-corpus-test-gate-spec.md`

If this contract changes, update those surfaces in the same change set.
