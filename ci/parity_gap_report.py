#!/usr/bin/env python3
import argparse
import json
from collections import Counter
from pathlib import Path

import yaml


def build_report(root: Path) -> dict:
    gaps = []
    for path in sorted(root.glob("*/*/*/meta.yaml")):
        meta = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
        fixture_id = meta.get("corpus_id") or str(path.parent.relative_to(root))
        family = next((tag for tag in meta.get("tags", []) if ":" not in tag and tag not in {"phase1", "representative", "beta-bar"}), "unknown")
        for band, paths in (meta.get("older_band_applicability") or {}).items():
            if band == "shared_contract_when_emitted" or not isinstance(paths, dict):
                continue
            for processing_path, cell in paths.items():
                if cell.get("status") == "missing_representative_evidence":
                    gaps.append({"kind": "family", "fixture_id": fixture_id, "family": family, "version_band": band, "processing_path": processing_path, "surface": None, "severity": "critical" if cell.get("parity_critical") else "informational", "note": cell.get("note")})
        matrix = meta.get("matrix_applicability") or {}
        declared = set(matrix.get("surfaces") or [])
        for surface in matrix.get("required_surfaces") or []:
            if surface not in declared:
                gaps.append({"kind": "surface", "fixture_id": fixture_id, "family": family, "version_band": matrix.get("version_band"), "processing_path": matrix.get("processing_path"), "surface": surface, "severity": "critical", "note": matrix.get("note")})
        if "debug" not in declared and matrix:
            gaps.append({"kind": "surface", "fixture_id": fixture_id, "family": family, "version_band": matrix.get("version_band"), "processing_path": matrix.get("processing_path"), "surface": "debug", "severity": "informational", "note": matrix.get("note")})
    counts = Counter((gap["severity"], gap["kind"]) for gap in gaps)
    critical = sum(1 for gap in gaps if gap["severity"] == "critical")
    return {"schema_version": 1, "status": "fail" if critical else "pass", "critical_gap_count": critical, "gap_counts": {f"{severity}_{kind}": count for (severity, kind), count in sorted(counts.items())}, "gaps": gaps}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default="corpus")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    report = build_report(Path(args.root))
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return 1 if report["status"] == "fail" else 0


if __name__ == "__main__":
    raise SystemExit(main())
