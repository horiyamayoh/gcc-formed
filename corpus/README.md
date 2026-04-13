# Corpus Workflow

`gcc-formed` treats the corpus as a product contract, not a loose collection of examples.

## Current Beta-Bar Target

- Keep the hand-authored corpus between 80 and 120 fixtures.
- Preserve the composition quota from `quality-corpus-test-gate-spec.md`.
- Prefer adding narrow repros over broad "kitchen sink" failures.

## Promotion Flow

When a harvested trace deserves to become a committed fixture, use this order:

1. Sanitize paths, usernames, and any source snippets that should not leave the trace bundle.
2. Deduplicate against the existing corpus by scenario and failure shape.
3. Minimize the repro until only the causal files and flags remain.
4. Classify the fixture by semantic family, version band, processing path, support level, and fallback contract.
5. Add `invoke.yaml`, `expectations.yaml`, `meta.yaml`, and snapshot artifacts under `snapshots/<version_band>/<processing_path>/`.
6. Record provenance in `meta.yaml` as hand-authored or minimized-from-shadow.
7. Run `cargo xtask replay --root corpus` and `cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15`.
8. Update `CHANGELOG.md` and any user-visible docs when the new fixture expands or clarifies the supported scenario set.

## Fixture Notes

- Promoted fixtures should carry semantic expectations strict enough to catch family, fallback, and provenance regressions without overfitting line noise.
- Use render expectation fields like `first_action_max_line`, `partial_notice_required`, `raw_diagnostics_hint_required`, `raw_sub_block_required`, `low_confidence_notice_required`, and `compaction_required_substrings` to make native-parity coverage explicit.
- Keep `required_substrings` / `forbidden_substrings` for general wording locks; reserve the dedicated native-parity fields for stop-ship dimensions that CI should classify directly.
- Representative fixtures that participate in the replay stop-ship matrix MUST declare explicit `band:*`, `processing_path:*`, and `surface:*` tags in `meta.yaml`. Use `band:gcc15` / `processing_path:dual_sink_structured` for GCC15 fixtures and the corresponding older-band tags for GCC13-14 / GCC9-12 fixtures.
- Representative fixtures MUST also carry a `matrix_applicability` block in `meta.yaml` with the declared `version_band`, `processing_path`, and checked-in replay `surfaces`. If a fixture omits a stop-ship surface such as `debug`, `matrix_applicability.note` MUST explain why that cell is intentionally not claimed yet.
- For Band B/Band C representative matrix coverage, keep `fallback_contract:bounded_render` or `fallback_contract:honest_fallback` explicit when the fixture is proving compatibility-path behavior.
- The C-first representative older-band pack must keep `compile`, `link`, `include+macro`, `preprocessor`, and `honest_fallback` visible in the promoted representative set. One fixture may cover more than one category, but the human-eval and RC packet must still expose all five categories explicitly.
- Treat `VersionBand x ProcessingPath x Surface` as the stop-ship replay vocabulary for representative fixtures. The replay report keeps the older band/path aggregates, but checked-in metadata should declare matrix cells with `surface:*` tags and `matrix_applicability` so missing coverage is explicit.
- Use the `cascade:` block in `expectations.yaml` for document-level episode/root count assertions, and reserve per-surface `expected_summary_only_group_count` / `expected_hidden_group_count` / `expected_suppressed_group_count` for output-shape gates.
- Use `anti_collision` plus scenario tags like `anti_collision:same_file_dual_syntax`, `anti_collision:syntax_flood_plus_type`, and `anti_collision:template_frontier_independent` for fixtures whose job is to prove independent-root recall and false-hidden-suppression safety.
- The representative replay report treats anti-collision fixtures as a separate stop-ship slice. Keep that slice populated across `gcc15_plus/dual_sink_structured`, `gcc13_14/native_text_capture`, `gcc13_14/single_sink_structured`, `gcc9_12/native_text_capture`, and `gcc9_12/single_sink_structured`.
- For Band C `SingleSinkStructured`, `diagnostics.json` is the authoritative replay ingress. Keep `diagnostics.sarif` only when another tool still expects it, not as the path-defining artifact.
- The primary fixture layout stores goldens under `snapshots/<version_band>/<processing_path>/`. If a fixture keeps extra presentation-specific artifacts such as `subject_blocks_v1`, nest them under that path root, for example `snapshots/gcc15/dual_sink_structured/subject_blocks_v1/`.
- Treat root-level `snapshots/gcc15/*` goldens as historical compatibility residue only; new or normalized representative fixtures should not rely on the implicit GCC15 root layout.
- If a fixture is not ready for representative gating yet, keep it out of the representative subset instead of weakening the representative expectations.
- When a snapshot changes, explain whether the change is semantic or normalization-only in the review.
