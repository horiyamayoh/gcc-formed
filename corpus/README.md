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
4. Classify the fixture by semantic family, support tier, processing path, and fallback contract.
5. Add `invoke.yaml`, `expectations.yaml`, `meta.yaml`, and snapshot artifacts under `snapshots/gcc15/`.
6. Record provenance in `meta.yaml` as hand-authored or minimized-from-shadow.
7. Run `cargo xtask replay --root corpus` and `cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15`.
8. Update `CHANGELOG.md` and any user-visible docs when the new fixture expands or clarifies the supported scenario set.

## Fixture Notes

- Promoted fixtures should carry semantic expectations strict enough to catch family, fallback, and provenance regressions without overfitting line noise.
- Use render expectation fields like `first_action_max_line`, `partial_notice_required`, `raw_diagnostics_hint_required`, `raw_sub_block_required`, `low_confidence_notice_required`, and `compaction_required_substrings` to make native-parity coverage explicit.
- Keep `required_substrings` / `forbidden_substrings` for general wording locks; reserve the dedicated native-parity fields for stop-ship dimensions that CI should classify directly.
- For Band B/Band C coverage, prefer explicit meta tags such as `band:gcc13_14` or `band:gcc9_12`, `processing_path:native_text_capture` or `processing_path:single_sink_structured`, and `fallback_contract:bounded_render` or `fallback_contract:honest_fallback`.
- For Band C `SingleSinkStructured`, `diagnostics.json` is the authoritative replay ingress. Keep `diagnostics.sarif` only when another tool still expects it, not as the path-defining artifact.
- The current harness layout still stores fixture goldens under `snapshots/gcc15/`; the band, processing path, and fallback contract are declared in fixture metadata, not in the snapshot directory name.
- If a fixture is not ready for representative gating yet, keep it out of the representative subset instead of weakening the representative expectations.
- When a snapshot changes, explain whether the change is semantic or normalization-only in the review.
