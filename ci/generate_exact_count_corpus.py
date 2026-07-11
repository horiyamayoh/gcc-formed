#!/usr/bin/env python3
"""Deterministically generate the reviewed #204 exact-count fixture pack."""
from __future__ import annotations

import difflib
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "corpus" / "repair-unit-exact-count"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def patch(before: str, after: str, source: str) -> str:
    return "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True), f"a/{source}", f"b/{source}"))


def make_fixture(kind: str, index: int, count: int) -> None:
    fixture = ROOT / kind / f"case-{index:02d}"
    cpp = index % 3 == 0
    source = "src/main.cpp" if cpp else "src/main.c"
    compiler = "g++" if cpp else "gcc"
    names = [f"missing_{kind}_{index}_{n}" for n in range(1, count + 1)]
    false_split = kind == "single" and index <= 8
    custom_after: list[str] | None = None
    if kind == "single" and index == 9:
        before = "int main(void) { int value = 0 return value; }\n"
        custom_after = ["int main(void) { int value = 0; return value; }\n"]
        shape, trap = "syntax_recovery", "false_split"
    elif kind == "single" and index == 10:
        cpp, source, compiler = True, "src/main.cpp", "g++"
        before = "void call(int, int);\nint main() { call(1); }\n"
        custom_after = ["void call(int, int);\nint main() { call(1, 2); }\n"]
        shape, trap = "overload_candidates", "false_split"
    elif kind == "single" and index == 11:
        cpp, source, compiler = True, "src/main.cpp", "g++"
        before = "template<class T> void take(T*);\nint main() { take(1); }\n"
        custom_after = ["template<class T> void take(T*);\nint main() { int value=1; take(&value); }\n"]
        shape, trap = "template_instantiation", "false_split"
    elif kind == "single" and index == 12:
        before = "#define BAD_VALUE missing_macro_value\nint main(void) { return BAD_VALUE; }\n"
        custom_after = ["#define BAD_VALUE 0\nint main(void) { return BAD_VALUE; }\n"]
        shape, trap = "macro_definition_use", "false_split"
    elif kind == "double" and index == 10:
        source, compiler = "src/main.c", "gcc"
        before = "int missing_link_a(void); int missing_link_b(void);\nint main(void) { return missing_link_a()+missing_link_b(); }\n"
        custom_after = [before + "int missing_link_a(void) { return 0; }\n", before + "int missing_link_b(void) { return 0; }\n"]
        shape, trap = "linker_two_symbols", "false_merge"
    elif false_split:
        before = "\n".join([f"int use_{n}(void) {{ return {names[0]}; }}" for n in range(1, 4)]) + "\n"
        shape, trap = "repeated_evidence", "false_split"
    else:
        before = "\n".join([f"int use_{n}(void) {{ return {name}; }}" for n, name in enumerate(names, 1)]) + "\n"
        shape, trap = "adjacent_same_family", "false_merge" if count > 1 else "single"
    specs = [
        "schema_version = 1",
        f'fixture_id = "repair-unit-exact-count/{kind}/case-{index:02d}"',
        f'compiler = "{compiler}"',
        f'args = ["-fdiagnostics-color=never", "-fmax-errors=0", "{source}", "-o", "main.o"]' if shape == "linker_two_symbols" else f'args = ["-fdiagnostics-color=never", "-fmax-errors=0", "-c", "{source}", "-o", "main.o"]',
        f'language = "{"cpp" if cpp else "c"}"',
        f'diagnostic_shape = "{shape}"',
        f'trap_kind = "{trap}"',
        f'oracle_repair_unit_count = {count}',
        'reviewer = "horiyamayoh"',
        'owner = "compiler-ux"',
        f'version_evidence = "{"gcc13_direct" if index % 4 == 1 else "gcc14_representative_justified" if index % 4 == 2 else "gcc15_representative_justified" if index % 4 == 3 else "gcc12_representative_justified"}"',
        "",
    ]
    for defect_index, name in enumerate(names, 1):
        after = custom_after[defect_index - 1] if custom_after else before.replace(name, "0")
        patch_name = f"repairs/defect-{defect_index}.patch"
        write(fixture / patch_name, patch(before, after, source))
        specs.extend([
            "[[defects]]",
            f'defect_id = "{kind}-{index:02d}-defect-{defect_index}"',
            f'patch = "{patch_name}"',
            f'primary_repair_anchors = ["{source}:1"]',
            "",
        ])
    write(fixture / source, before)
    if custom_after:
        passing = before
        if count == 1:
            passing = custom_after[0]
        else:
            for addition in [text.removeprefix(before) for text in custom_after]:
                passing += addition
    else:
        passing = before
        for name in names:
            passing = passing.replace(name, "0")
    write(fixture / "passing" / Path(source).name, passing)
    write(fixture / "repair-oracle.toml", "\n".join(specs))


def main() -> None:
    if ROOT.exists():
        shutil.rmtree(ROOT)
    for kind, total, count in [("single", 12, 1), ("double", 10, 2), ("triple", 6, 3)]:
        for index in range(1, total + 1):
            make_fixture(kind, index, count)


if __name__ == "__main__":
    main()
