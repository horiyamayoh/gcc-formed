#!/usr/bin/env python3
"""Generate the checked-in, license-safe real-project differential corpus."""

from __future__ import annotations

import difflib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent

SHAPES = [
    ("make-generated-c", "c", "make", "compile", "generated_config_header"),
    ("cmake-template-cpp", "cpp", "cmake", "compile", "template_heavy_cpp"),
    ("direct-multi-tu-c", "c", "direct", "compile", "repeated_across_translation_units"),
    ("make-parallel-c", "c", "make", "compile", "parallel_independent_files"),
    ("cmake-duplicate-symbol-cpp", "cpp", "cmake", "link", "duplicate_symbol"),
    ("direct-link-order-c", "c", "direct", "link", "missing_library_link_order"),
    ("make-werror-c", "c", "make", "compile", "warning_as_error"),
    ("cmake-frontier-cpp", "cpp", "cmake", "compile", "system_header_frontier_launcher_path"),
]

REQUIRED_FAMILIES = [
    "generated_config_header",
    "repeated_across_translation_units",
    "parallel_independent_files",
    "macro_heavy_c",
    "template_heavy_cpp",
    "system_header_frontier",
    "missing_library_duplicate_symbol_link_order",
    "warning_as_error",
    "non_utf8_path_terminal_edge",
    "ccache_style_launcher",
]


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def patch(old: str, new: str, name: str) -> str:
    return "".join(
        difflib.unified_diff(
            old.splitlines(keepends=True),
            new.splitlines(keepends=True),
            fromfile=f"a/{name}",
            tofile=f"b/{name}",
        )
    )


def sources(language: str, phase: str, index: int, multi: bool) -> tuple[dict[str, str], dict[str, str]]:
    ext = "cpp" if language == "cpp" else "c"
    symbol_a = f"missing_project_symbol_{index}_a"
    if phase == "link":
        broken = {f"src/main.{ext}": f"int {symbol_a}(void); int main(void) {{ return {symbol_a}(); }}\n"}
        fixed = {f"src/main.{ext}": f"int {symbol_a}(void) {{ return 0; }} int main(void) {{ return {symbol_a}(); }}\n"}
    else:
        broken = {f"src/main.{ext}": f"int main(void) {{ return {symbol_a}; }}\n"}
        fixed = {f"src/main.{ext}": "int main(void) { return 0; }\n"}
    if multi:
        symbol_b = f"missing_project_symbol_{index}_b"
        if phase == "link":
            broken[f"src/worker.{ext}"] = f"int {symbol_b}(void); int worker(void) {{ return {symbol_b}(); }}\n"
            fixed[f"src/worker.{ext}"] = f"int {symbol_b}(void) {{ return 0; }} int worker(void) {{ return {symbol_b}(); }}\n"
        else:
            broken[f"src/worker.{ext}"] = f"int worker(void) {{ return {symbol_b}; }}\n"
            fixed[f"src/worker.{ext}"] = "int worker(void) { return 0; }\n"
    return broken, fixed


def main() -> None:
    manifest = {"schema_version": 1, "projects": [], "scenario_count": 0}
    for shape_index, (shape, language, build_system, phase, family) in enumerate(SHAPES, 1):
        project = {
            "project_id": shape,
            "build_system": build_system,
            "language": language,
            "phase": phase,
            "scenario_family": family,
            "license": "CC0-1.0",
            "provenance": "purpose-built minimized extract modeled on common open-source build topology; no third-party source copied",
            "redaction": "synthetic relative paths and identifiers only",
            "reviewer": "horiyamayoh",
            "repair_owner": "compiler-ux",
            "network_required": False,
            "scenarios": [],
        }
        for case in range(1, 6):
            fixture_id = f"{shape}/case-{case:02d}"
            directory = ROOT / shape / f"case-{case:02d}"
            multi = shape in {"direct-multi-tu-c", "make-parallel-c"} or case in {4, 5}
            scenario_family = REQUIRED_FAMILIES[((shape_index - 1) * 5 + case - 1) % len(REQUIRED_FAMILIES)]
            broken, fixed = sources(language, phase, shape_index * 10 + case, multi)
            for name, content in broken.items():
                write(directory / name, content)
            args = ["-fdiagnostics-color=never", "-fmax-errors=0"]
            if phase == "compile":
                args.append("-c")
            args.extend(sorted(broken))
            if phase == "link":
                args.extend(["-o", "project-app"])
            compiler = "g++" if language == "cpp" else "gcc"
            defects = []
            for defect_index, name in enumerate(sorted(broken), 1):
                patch_name = f"repairs/defect-{defect_index}.patch"
                write(directory / patch_name, patch(broken[name], fixed[name], name))
                defects.append(
                    "\n".join(
                        [
                            "[[defects]]",
                            f'defect_id = "{shape}-case-{case:02d}-defect-{defect_index}"',
                            f'patch = "{patch_name}"',
                            f'primary_repair_anchors = ["{name}:1"]',
                        ]
                    )
                )
            oracle = "\n".join(
                [
                    "schema_version = 1",
                    f'fixture_id = "{fixture_id}"',
                    f'compiler = "{compiler}"',
                    f"args = {json.dumps(args)}",
                    f'language = "{language}"',
                    f'diagnostic_shape = "{family}"',
                    f'trap_kind = "{"false_merge" if multi else "false_split"}"',
                    f'reviewer = "{project["reviewer"]}"',
                    f'owner = "{project["repair_owner"]}"',
                    f'version_evidence = "{"gcc15_direct" if shape_index % 3 == 1 else "gcc13_14_or_gcc9_12_representative"}"',
                    "",
                    "\n\n".join(defects),
                    "",
                ]
            )
            write(directory / "repair-oracle.toml", oracle)
            scenario_meta = {
                "schema_version": 1,
                "fixture_id": fixture_id,
                "project_id": shape,
                "build_system": build_system,
                "language": language,
                "phase": phase,
                "scenario_family": scenario_family,
                "multi_invocation": multi,
                "invocation_boundary": "one compiler argv per captured invocation; parallel streams carry invocation_id",
                "license": project["license"],
                "provenance": project["provenance"],
                "redaction": project["redaction"],
                "reviewer": project["reviewer"],
                "repair_owner": project["repair_owner"],
                "network_required": False,
                "promotion_status": "reviewed",
                "unknown_unresolved_retention": "retain_with_raw_capture",
            }
            write(directory / "scenario.json", json.dumps(scenario_meta, indent=2) + "\n")
            invocations = {
                "schema_version": 1,
                "parallel_capture": multi,
                "attribution_precedes_repair_unit_inference": True,
                "invocations": [
                    {
                        "invocation_id": f"invocation-{n}",
                        "translation_unit": name,
                        "argv": [compiler, "-fdiagnostics-color=never", "-c", name],
                        "stderr_span_ref": f"raw/{n}.stderr",
                    }
                    for n, name in enumerate(sorted(broken), 1)
                ],
            }
            write(directory / "invocations.json", json.dumps(invocations, indent=2) + "\n")
            project["scenarios"].append(fixture_id)
            manifest["scenario_count"] += 1
        manifest["projects"].append(project)
    write(ROOT / "manifest.json", json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
