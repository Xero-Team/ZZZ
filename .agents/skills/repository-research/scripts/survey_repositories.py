#!/usr/bin/env python3
"""Generate deterministic static Markdown surveys for repository checkouts."""

from __future__ import annotations

import argparse
import collections
import json
import re
import tomllib
from pathlib import Path

from _common import git, is_git_repository, read_tsv, resolve_under_base, tracked_files, truncate

KEYWORDS = [
    "agent", "benchmark", "browser", "cache", "checkpoint", "citation", "embed",
    "evaluation", "graph", "index", "mcp", "oauth", "plugin", "provider", "queue",
    "rank", "retriev", "sandbox", "search", "symbol", "telemetry", "tree_sitter", "vector",
]
ENTRYPOINT_NAMES = {
    "main.py", "__main__.py", "cli.py", "server.py", "app.py", "main.rs", "lib.rs",
    "main.go", "main.ts", "index.ts", "index.js", "extension.ts", "action.yml",
}
TEST_MARKERS = ("test", "tests", "spec", "specs", "benchmark", "benchmarks", "eval", "evals")
DOC_NAMES = ("readme", "architecture", "design", "security", "contributing", "agents")
TEXT_SUFFIXES = {".c", ".cc", ".cpp", ".go", ".java", ".js", ".jsx", ".json", ".kt", ".md", ".py", ".rb", ".rs", ".toml", ".ts", ".tsx", ".yaml", ".yml"}
DEFAULT_MAX_FILE_BYTES = 1_000_000
MAX_KEYWORD_FILES = 12
MAX_SUBSYSTEMS = 30
MAX_LISTED_ENTRIES = 80


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    parser.add_argument("--max-file-bytes", type=int, default=DEFAULT_MAX_FILE_BYTES)
    parser.add_argument(
        "--skip-existing",
        action="store_true",
        help="keep existing survey files, so manual analysis beside them is not overwritten",
    )
    return parser.parse_args()


def package_json_dependencies(path: Path) -> set[str]:
    data = json.loads(path.read_text(encoding="utf-8"))
    return {
        name
        for key in ("dependencies", "devDependencies", "peerDependencies")
        for name in (data.get(key) or {})
    }


def cargo_toml_dependencies(path: Path) -> set[str]:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return {
        name
        for key in ("dependencies", "dev-dependencies", "build-dependencies")
        for name in (data.get(key) or {})
    }


def pyproject_dependencies(path: Path) -> set[str]:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    project = data.get("project") or {}
    dependencies: set[str] = set()
    for item in project.get("dependencies") or []:
        match = re.match(r"[A-Za-z0-9_.-]+", str(item))
        if match:
            dependencies.add(match.group(0))
    return dependencies


def go_mod_dependencies(path: Path) -> set[str]:
    dependencies: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        match = re.match(r"\s*([\w./-]+)\s+v\d", line)
        if match:
            dependencies.add(match.group(1))
    return dependencies


MANIFEST_PARSERS = {
    "package.json": package_json_dependencies,
    "pyproject.toml": pyproject_dependencies,
    "Cargo.toml": cargo_toml_dependencies,
    "go.mod": go_mod_dependencies,
}


def extract_dependencies(repo: Path, files: list[str]) -> list[str]:
    dependencies: set[str] = set()
    for relative in files:
        parser = MANIFEST_PARSERS.get(Path(relative).name)
        if parser is None:
            continue
        try:
            dependencies.update(parser(repo / relative))
        except (OSError, UnicodeError, json.JSONDecodeError, tomllib.TOMLDecodeError):
            continue
    return sorted(dependencies)


def survey(repo: Path, display_name: str, max_file_bytes: int) -> str:
    files = tracked_files(repo)
    if files and not any((repo / relative).exists() for relative in files[:50]):
        raise RuntimeError("working tree is empty; run the survey on a checked-out clone")
    top_level = collections.Counter(Path(item).parts[0] for item in files)
    manifests = [item for item in files if Path(item).name in {"package.json", "pyproject.toml", "Cargo.toml", "go.mod", "pom.xml", "action.yml"}]
    entrypoints = [item for item in files if Path(item).name.lower() in ENTRYPOINT_NAMES]
    tests = [item for item in files if any(part.lower() in TEST_MARKERS or part.lower().startswith("test_") for part in Path(item).parts)]
    docs = [item for item in files if Path(item).suffix.lower() in {".md", ".mdx", ".rst"} and any(name in Path(item).name.lower() for name in DOC_NAMES)]
    keyword_files: dict[str, list[str]] = {keyword: [] for keyword in KEYWORDS}

    for relative in files:
        path = repo / relative
        if path.suffix.lower() not in TEXT_SUFFIXES:
            continue
        try:
            if path.stat().st_size > max_file_bytes:
                continue
            text = path.read_text(encoding="utf-8", errors="ignore").lower()
        except OSError:
            continue
        for keyword in KEYWORDS:
            if keyword in text and len(keyword_files[keyword]) < MAX_KEYWORD_FILES:
                keyword_files[keyword].append(relative)

    dependencies = extract_dependencies(repo, manifests)
    head = git(repo, "rev-parse", "HEAD")
    lines = [
        f"# Static survey: {display_name}",
        "",
        f"- Checkout: `{repo}`",
        f"- HEAD: `{head}`",
        f"- Tracked files: {len(files)}",
        "",
        "## Top-level subsystems",
        "",
    ]
    lines.extend(f"- `{name}`: {count} files" for name, count in top_level.most_common(MAX_SUBSYSTEMS))
    for title, values in (
        ("Manifests", manifests),
        ("Candidate entrypoints", entrypoints),
        ("Tests, benchmarks, and evals", tests),
        ("Architecture and policy documents", docs),
        ("Declared dependencies", dependencies),
    ):
        lines.extend(["", f"## {title}", ""])
        if values:
            lines.extend(f"- `{value}`" for value in values[:MAX_LISTED_ENTRIES])
            if len(values) > MAX_LISTED_ENTRIES:
                lines.append(f"- … {len(values) - MAX_LISTED_ENTRIES} more")
        else:
            lines.append("- None detected by the static survey.")
    lines.extend(["", "## Architecture keyword hints", ""])
    for keyword in KEYWORDS:
        values = keyword_files[keyword]
        if values:
            lines.append(f"- **{keyword}**: " + ", ".join(f"`{value}`" for value in values))
    lines.extend([
        "",
        "## Interpretation limit",
        "",
        "This survey identifies candidate files. It does not establish behavior; read and cite the implementation and tests before making claims.",
        "",
    ])
    return "\n".join(lines)


def main() -> int:
    arguments = parse_arguments()
    base_dir = arguments.base_dir.resolve()
    output_dir = arguments.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    failures = 0
    written = 0
    skipped = 0
    for row in read_tsv(arguments.sources):
        if row.get("status") == "unavailable":
            continue
        try:
            repo = resolve_under_base(row.get("clone_path", ""), base_dir)
        except ValueError as error:
            print(f"error\t{row.get('id', '')}\t{error}")
            failures += 1
            continue
        if not is_git_repository(repo):
            print(f"missing\t{row.get('id', '')}\t{repo}")
            failures += 1
            continue
        slug = repo.name
        destination = output_dir / f"{slug}.md"
        if arguments.skip_existing and destination.exists():
            print(f"kept\t{destination}")
            skipped += 1
            continue
        try:
            text = survey(repo, row.get("requested_name") or slug, arguments.max_file_bytes)
            destination.write_text(text, encoding="utf-8")
            print(f"written\t{destination}")
            written += 1
        except Exception as error:
            print(f"error\t{row.get('id', '')}\t{truncate(str(error))}")
            failures += 1
    print(f"surveys={written} skipped={skipped} failures={failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
