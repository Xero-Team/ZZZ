#!/usr/bin/env python3
"""Generate deterministic static Markdown surveys for repository checkouts."""

from __future__ import annotations

import argparse
import collections
import json
import re
import tomllib
from pathlib import Path

from _common import git, is_git_repository, read_tsv, resolve_under_base, truncate

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


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    parser.add_argument("--max-file-bytes", type=int, default=1_000_000)
    return parser.parse_args()


def extract_dependencies(repo: Path, files: list[str]) -> list[str]:
    dependencies: set[str] = set()
    for relative in files:
        path = repo / relative
        try:
            if path.name == "package.json":
                data = json.loads(path.read_text(encoding="utf-8"))
                for key in ("dependencies", "devDependencies", "peerDependencies"):
                    dependencies.update((data.get(key) or {}).keys())
            elif path.name in {"pyproject.toml", "Cargo.toml"}:
                data = tomllib.loads(path.read_text(encoding="utf-8"))
                if path.name == "Cargo.toml":
                    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
                        dependencies.update((data.get(key) or {}).keys())
                else:
                    project = data.get("project") or {}
                    for item in project.get("dependencies") or []:
                        match = re.match(r"[A-Za-z0-9_.-]+", str(item))
                        if match:
                            dependencies.add(match.group(0))
            elif path.name == "go.mod":
                for line in path.read_text(encoding="utf-8").splitlines():
                    match = re.match(r"\s*([\w./-]+)\s+v\d", line)
                    if match:
                        dependencies.add(match.group(1))
        except (OSError, UnicodeError, json.JSONDecodeError, tomllib.TOMLDecodeError):
            continue
    return sorted(dependencies)


def survey(repo: Path, display_name: str, max_file_bytes: int) -> str:
    files = [line for line in git(repo, "ls-files").splitlines() if line]
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
            if keyword in text and len(keyword_files[keyword]) < 12:
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
    lines.extend(f"- `{name}`: {count} files" for name, count in top_level.most_common(30))
    for title, values in (
        ("Manifests", manifests),
        ("Candidate entrypoints", entrypoints),
        ("Tests, benchmarks, and evals", tests),
        ("Architecture and policy documents", docs),
        ("Declared dependencies", dependencies),
    ):
        lines.extend(["", f"## {title}", ""])
        if values:
            lines.extend(f"- `{value}`" for value in values[:80])
            if len(values) > 80:
                lines.append(f"- … {len(values) - 80} more")
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
        try:
            text = survey(repo, row.get("requested_name") or slug, arguments.max_file_bytes)
            destination = output_dir / f"{slug}.md"
            destination.write_text(text, encoding="utf-8")
            print(f"written\t{destination}")
            written += 1
        except Exception as error:
            print(f"error\t{row.get('id', '')}\t{truncate(str(error))}")
            failures += 1
    print(f"surveys={written} failures={failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
