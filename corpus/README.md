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
4. Classify the fixture by semantic family, support tier, and expected wrapper mode.
5. Add `invoke.yaml`, `expectations.yaml`, `meta.yaml`, and snapshot artifacts under `snapshots/gcc15/`.
6. Record provenance in `meta.yaml` as hand-authored or minimized-from-shadow.
7. Run `cargo xtask replay --root corpus` and `cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15`.
8. Update `CHANGELOG.md` and any user-visible docs when the new fixture expands or clarifies the supported scenario set.

## Fixture Notes

- Promoted fixtures should carry semantic expectations strict enough to catch family, fallback, and provenance regressions without overfitting line noise.
- If a fixture is not ready for representative gating yet, keep it out of the representative subset instead of weakening the representative expectations.
- When a snapshot changes, explain whether the change is semantic or normalization-only in the review.
