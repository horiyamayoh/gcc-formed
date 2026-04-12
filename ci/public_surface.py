#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
PUBLIC_SURFACE_DOC = REPO_ROOT / "docs" / "support" / "PUBLIC-SURFACE.md"
SUPPORT_BOUNDARY_DOC = REPO_ROOT / "docs" / "support" / "SUPPORT-BOUNDARY.md"
SUPPORT_BOUNDARY_HEADING = "## 2. Current `v1beta` / `0.2.0-beta.N` support boundary"


def extract_front_matter(text: str) -> dict[str, object]:
    if not text.startswith("---\n"):
        raise AssertionError("missing YAML front matter")

    end = text.find("\n---\n", 4)
    if end == -1:
        raise AssertionError("unterminated YAML front matter")

    block = text[4:end]
    data: dict[str, object] = {}
    lines = block.splitlines()
    index = 0

    while index < len(lines):
        line = lines[index]
        if not line.strip():
            index += 1
            continue
        key, value = line.split(":", 1)
        value = value.lstrip()
        if value == "[]":
            data[key] = []
            index += 1
            continue
        if value:
            data[key] = value
            index += 1
            continue
        index += 1
        items: list[str] = []
        while index < len(lines) and lines[index].startswith("  - "):
            items.append(lines[index][4:])
            index += 1
        data[key] = items

    return data


def extract_section(text: str, heading: str) -> str:
    lines = text.splitlines()
    capture = False
    section: list[str] = []
    heading_level = len(heading) - len(heading.lstrip("#"))

    for line in lines:
        if line.strip() == heading:
            capture = True
            continue
        if capture and line.startswith("#"):
            level = len(line) - len(line.lstrip("#"))
            if level <= heading_level:
                break
        if capture:
            section.append(line)

    if not section:
        raise AssertionError(f"section not found or empty: {heading}")

    return "\n".join(section)


def load_public_surface_contract() -> dict[str, object]:
    return extract_front_matter(PUBLIC_SURFACE_DOC.read_text(encoding="utf-8"))


def support_boundary_lines() -> list[str]:
    section = extract_section(
        SUPPORT_BOUNDARY_DOC.read_text(encoding="utf-8"), SUPPORT_BOUNDARY_HEADING
    )
    lines = [line.strip() for line in section.splitlines()]
    return [line for line in lines if line.startswith("- ")]


def format_template(value: str, **kwargs: str | None) -> str:
    rendered = value
    for key, raw in kwargs.items():
        rendered = rendered.replace(f"{{{key}}}", raw if raw is not None else "<not supplied>")
    return rendered


def required_list(metadata: dict[str, object], key: str) -> list[str]:
    value = metadata.get(key)
    if not isinstance(value, list):
        raise AssertionError(f"{key} must be a list")
    return [str(item) for item in value]


def required_string(metadata: dict[str, object], key: str) -> str:
    value = metadata.get(key)
    if not isinstance(value, str) or not value:
        raise AssertionError(f"{key} must be a non-empty string")
    return value


def doc_url(repository: str, commit: str, relative_path: str) -> str:
    return f"https://github.com/{repository}/blob/{commit}/{relative_path}"


def render_doc_links(repository: str, commit: str, doc_paths: list[str]) -> list[str]:
    return [
        f"- [`{relative_path}`]({doc_url(repository, commit, relative_path)})"
        for relative_path in doc_paths
    ]


def bulletize(lines: list[str]) -> list[str]:
    return [line if line.startswith("- ") else f"- {line}" for line in lines]


def render_release_body(
    *,
    kind: str,
    version: str,
    repository: str,
    commit: str,
    signing_key_id: str | None,
    signing_public_key_sha256: str | None,
    rollback_baseline_version: str | None,
) -> str:
    metadata = load_public_surface_contract()
    support_lines = support_boundary_lines()

    sections: list[str] = [f"# gcc-formed {version}", ""]

    if kind == "beta":
        sections.append(format_template(required_string(metadata, "beta_release_intro"), version=version))
        sections.extend(
            [
                "",
                "## Support Boundary",
                "",
                *support_lines,
                "",
                "## Release Gate Scope",
                "",
                *bulletize(required_list(metadata, "beta_release_gate_scope")),
                "",
                "## Install Paths",
                "",
                *bulletize(required_list(metadata, "beta_install_path_lines")),
                "",
                "## Current Docs",
                "",
                *render_doc_links(
                    repository, commit, required_list(metadata, "beta_release_doc_paths")
                ),
                "",
                "## Signing",
                "",
                f"- signing key id: `{signing_key_id or '<not supplied>'}`",
                (
                    "- trusted signing public key sha256: "
                    f"`{signing_public_key_sha256 or '<not supplied>'}`"
                ),
                "",
                "## Included Assets",
                "",
                *bulletize(required_list(metadata, "beta_included_assets")),
                "",
                "## Known Limits",
                "",
                *bulletize([
                    format_template(line, version=version)
                    for line in required_list(metadata, "beta_known_limits")
                ]),
            ]
        )
    elif kind == "stable":
        sections.append(
            format_template(required_string(metadata, "stable_release_intro"), version=version)
        )
        sections.extend(
            [
                "",
                "## Stable Cut Evidence",
                "",
                *bulletize([
                    format_template(
                        line,
                        rollback_baseline_version=rollback_baseline_version,
                        signing_key_id=signing_key_id,
                        signing_public_key_sha256=signing_public_key_sha256,
                    )
                    for line in required_list(metadata, "stable_evidence_lines")
                ]),
                "",
                "## Support Boundary",
                "",
                *support_lines,
                "",
                "## Release Gate Scope",
                "",
                *bulletize(required_list(metadata, "stable_release_gate_scope")),
                "",
                "## Current Docs",
                "",
                *render_doc_links(
                    repository, commit, required_list(metadata, "stable_release_doc_paths")
                ),
                "",
                "## Included Assets",
                "",
                *bulletize(required_list(metadata, "stable_included_assets")),
                "",
                "## Known Limits",
                "",
                *bulletize(required_list(metadata, "stable_known_limits")),
            ]
        )
    else:
        raise AssertionError(f"unsupported release kind: {kind}")

    return "\n".join(sections) + "\n"


def repo_metadata_payload() -> dict[str, object]:
    metadata = load_public_surface_contract()
    return {
        "description": required_string(metadata, "repo_description"),
        "homepage": required_string(metadata, "repo_homepage_url"),
        "topics": required_list(metadata, "repo_topics"),
        "readme_tagline": required_string(metadata, "readme_tagline"),
    }


def sync_github_repo_metadata(repo: str, *, dry_run: bool) -> dict[str, object]:
    payload = repo_metadata_payload()
    if dry_run:
        return payload

    subprocess.run(
        [
            "gh",
            "api",
            f"repos/{repo}",
            "--method",
            "PATCH",
            "-f",
            f"description={payload['description']}",
            "-f",
            f"homepage={payload['homepage']}",
        ],
        cwd=REPO_ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    subprocess.run(
        [
            "gh",
            "api",
            f"repos/{repo}/topics",
            "--method",
            "PUT",
            "--input",
            "-",
        ],
        cwd=REPO_ROOT,
        input=json.dumps({"names": payload["topics"]}),
        check=True,
        text=True,
        capture_output=True,
    )
    return payload


def default_repository() -> str:
    return os.environ.get("GITHUB_REPOSITORY", "horiyamayoh/gcc-formed")


def default_commit() -> str:
    return os.environ.get("GITHUB_SHA", "main")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Render release-body and repo-landing metadata from current-authority docs."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    repo_metadata = subparsers.add_parser(
        "repo-metadata", help="Print canonical repo metadata as JSON."
    )
    repo_metadata.set_defaults(handler=handle_repo_metadata)

    render_release_body_parser = subparsers.add_parser(
        "render-release-body", help="Render the GitHub Release body to stdout."
    )
    render_release_body_parser.add_argument(
        "--kind", choices=("beta", "stable"), required=True
    )
    render_release_body_parser.add_argument("--version", required=True)
    render_release_body_parser.add_argument("--repository", default=default_repository())
    render_release_body_parser.add_argument("--commit", default=default_commit())
    render_release_body_parser.add_argument("--signing-key-id")
    render_release_body_parser.add_argument("--signing-public-key-sha256")
    render_release_body_parser.add_argument("--rollback-baseline-version")
    render_release_body_parser.set_defaults(handler=handle_render_release_body)

    sync_repo = subparsers.add_parser(
        "sync-github-repo-metadata",
        help="Sync repo description/homepage/topics to GitHub via gh api.",
    )
    sync_repo.add_argument("--repo", required=True)
    sync_repo.add_argument("--dry-run", action="store_true")
    sync_repo.set_defaults(handler=handle_sync_repo_metadata)

    return parser


def handle_repo_metadata(_args: argparse.Namespace) -> int:
    print(json.dumps(repo_metadata_payload(), indent=2))
    return 0


def handle_render_release_body(args: argparse.Namespace) -> int:
    print(
        render_release_body(
            kind=args.kind,
            version=args.version,
            repository=args.repository,
            commit=args.commit,
            signing_key_id=args.signing_key_id,
            signing_public_key_sha256=args.signing_public_key_sha256,
            rollback_baseline_version=args.rollback_baseline_version,
        ),
        end="",
    )
    return 0


def handle_sync_repo_metadata(args: argparse.Namespace) -> int:
    print(json.dumps(sync_github_repo_metadata(args.repo, dry_run=args.dry_run), indent=2))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.handler(args)


if __name__ == "__main__":
    sys.exit(main())
