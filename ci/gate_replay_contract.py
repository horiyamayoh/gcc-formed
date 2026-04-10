#!/usr/bin/env python3

import argparse
import json
from collections import Counter
from pathlib import Path

SCHEMA_VERSION = 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Classify replay-report stop-ship blockers into a machine-readable gate artifact."
    )
    parser.add_argument("--replay-report", required=True, help="Path to replay-report.json.")
    parser.add_argument(
        "--output",
        required=True,
        help="Path to the machine-readable replay stop-ship artifact.",
    )
    return parser.parse_args()


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def native_parity_concern_from_layer(layer: str) -> str | None:
    if layer.endswith(".ansi"):
        return "color_meaning"
    if layer.endswith(".line_budget"):
        return "line_budget"
    if layer.endswith(".first_action_visibility"):
        return "first_action_visibility"
    if (
        layer.endswith(".omission_notice")
        or layer.endswith(".partial_notice")
        or layer.endswith(".raw_disclosure")
        or layer.endswith(".raw_sub_block")
        or layer.endswith(".low_confidence_notice")
    ):
        return "disclosure_honesty"
    if layer.endswith(".compaction"):
        return "compaction"
    return None


def surface_from_layer(layer: str) -> str | None:
    parts = layer.split(".")
    if len(parts) >= 3 and parts[0] == "render":
        return parts[1]
    return None


def concern_from_layer(layer: str) -> str:
    native_parity = native_parity_concern_from_layer(layer)
    if native_parity is not None:
        return native_parity
    parts = layer.split(".")
    if len(parts) >= 3 and parts[0] == "render":
        return parts[2]
    return layer


def fixture_index(replay: dict) -> dict[str, dict]:
    fixtures = {}
    for fixture in replay.get("fixtures", []):
        fixture_id = fixture.get("fixture_id")
        if fixture_id:
            fixtures[fixture_id] = fixture
    return fixtures


def build_blocker(
    *,
    category: str,
    concern: str,
    layer: str,
    summary: str,
    fixture_id: str | None = None,
    support_band: str | None = None,
    processing_path: str | None = None,
    surface: str | None = None,
    matrix_cell: str | None = None,
) -> dict:
    blocker = {
        "category": category,
        "concern": concern,
        "layer": layer,
        "summary": summary,
        "fixture_id": fixture_id,
        "support_band": support_band,
        "processing_path": processing_path,
        "surface": surface,
    }
    if matrix_cell is not None:
        blocker["matrix_cell"] = matrix_cell
    return blocker


def build_matrix_hole_blockers(coverage: dict) -> list[dict]:
    blockers = []
    for cell in coverage.get("missing_required_band_path_surfaces", []):
        parts = cell.split("/", 2)
        support_band = parts[0] if len(parts) >= 1 else None
        processing_path = parts[1] if len(parts) >= 2 else None
        surface = parts[2] if len(parts) >= 3 else None
        blockers.append(
            build_blocker(
                category="matrix_hole",
                concern="coverage.band_path_surface",
                layer="coverage.band_path_surface",
                summary=f"missing required coverage cell `{cell}`",
                support_band=support_band,
                processing_path=processing_path,
                surface=surface,
                matrix_cell=cell,
            )
        )
    for cell in coverage.get("missing_required_band_paths", []):
        parts = cell.split("/", 1)
        support_band = parts[0] if len(parts) >= 1 else None
        processing_path = parts[1] if len(parts) >= 2 else None
        blockers.append(
            build_blocker(
                category="matrix_hole",
                concern="coverage.band_path",
                layer="coverage.band_path",
                summary=f"missing required coverage path `{cell}`",
                support_band=support_band,
                processing_path=processing_path,
                matrix_cell=cell,
            )
        )
    return blockers


def build_native_parity_blockers(replay: dict, fixtures: dict[str, dict]) -> list[dict]:
    blockers = []
    for failure in (replay.get("native_parity") or {}).get("failing_fixtures", []):
        fixture = fixtures.get(failure.get("fixture_id") or "", {})
        layer = failure.get("layer") or "native_parity"
        blockers.append(
            build_blocker(
                category="native_parity",
                concern=(failure.get("dimension") or concern_from_layer(layer)),
                layer=layer,
                summary=failure.get("summary") or "native parity stop-ship regression",
                fixture_id=failure.get("fixture_id"),
                support_band=fixture.get("support_band"),
                processing_path=fixture.get("processing_path"),
                surface=surface_from_layer(layer),
            )
        )
    return blockers


def build_quality_blockers(replay: dict, fixtures: dict[str, dict]) -> list[dict]:
    blockers = []
    for failure in replay.get("failures", []):
        layer = failure.get("layer") or "unknown"
        if layer in {"coverage.band_path_surface", "coverage.band_path"}:
            continue
        if native_parity_concern_from_layer(layer) is not None:
            continue
        fixture_id = failure.get("fixture_id")
        fixture = fixtures.get(fixture_id or "", {})
        blockers.append(
            build_blocker(
                category="quality_regression",
                concern=concern_from_layer(layer),
                layer=layer,
                summary=failure.get("summary") or "replay verification failure",
                fixture_id=fixture_id,
                support_band=fixture.get("support_band"),
                processing_path=fixture.get("processing_path"),
                surface=surface_from_layer(layer),
            )
        )
    return blockers


def missing_report_payload(report_path: Path) -> dict:
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "fail",
        "replay_report_path": str(report_path),
        "blocker_counts": {
            "total": 1,
            "by_category": {"instrumentation": 1},
            "by_concern": {"missing_replay_report": 1},
        },
        "blockers": [
            build_blocker(
                category="instrumentation",
                concern="missing_replay_report",
                layer="instrumentation.missing_replay_report",
                summary=f"missing replay report `{report_path}`",
            )
        ],
    }


def main() -> int:
    args = parse_args()
    replay_report_path = Path(args.replay_report)
    output_path = Path(args.output)

    if not replay_report_path.exists():
        payload = missing_report_payload(replay_report_path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        return 1

    replay = load_json(replay_report_path)
    fixtures = fixture_index(replay)
    blockers = []
    blockers.extend(build_matrix_hole_blockers(replay.get("coverage") or {}))
    blockers.extend(build_native_parity_blockers(replay, fixtures))
    blockers.extend(build_quality_blockers(replay, fixtures))

    by_category = Counter(blocker["category"] for blocker in blockers)
    by_concern = Counter(blocker["concern"] for blocker in blockers)
    payload = {
        "schema_version": SCHEMA_VERSION,
        "status": "pass" if not blockers else "fail",
        "replay_report_path": str(replay_report_path),
        "blocker_counts": {
            "total": len(blockers),
            "by_category": dict(sorted(by_category.items())),
            "by_concern": dict(sorted(by_concern.items())),
        },
        "blockers": blockers,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return 0 if not blockers else 1


if __name__ == "__main__":
    raise SystemExit(main())
