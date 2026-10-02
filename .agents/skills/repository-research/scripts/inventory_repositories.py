#!/usr/bin/env python3
"""Create a reproducible inventory for repository checkouts in SOURCES.tsv."""

from __future__ import annotations

import argparse
import collections
from pathlib import Path

from _common import git, is_git_repository, read_tsv, redact_url, resolve_under_base, truncate, utc_now, write_tsv

EXTENSION_LANGUAGES = {
    ".c": "C", ".cc": "C++", ".cpp": "C++", ".cs": "C#", ".go": "Go",
    ".java": "Java", ".js": "JavaScript", ".jsx": "JavaScript", ".kt": "Kotlin",
    ".kts": "Kotlin", ".lua": "Lua", ".php": "PHP", ".py": "Python",
    ".rb": "Ruby", ".rs": "Rust", ".scala": "Scala", ".swift": "Swift",
    ".ts": "TypeScript", ".tsx": "TypeScript", ".zig": "Zig",
}
MANIFEST_NAMES = {
    "Cargo.toml", "go.mod", "package.json", "pnpm-workspace.yaml", "pyproject.toml",
    "requirements.txt", "Gemfile", "pom.xml", "build.gradle", "build.gradle.kts",
    "Package.swift", "deno.json", "deno.jsonc", "action.yml", "docker-compose.yml",
}
POLICY_PATTERNS = ("agents", "security", "privacy", "threat", "contributing", "code_of_conduct")
FIELDS = [
    "id", "requested_name", "clone_path", "source_status", "repository_state",
    "origin_url", "head_sha", "head_date", "head_subject", "branch", "is_shallow",
    "tracked_files", "tracked_bytes", "dominant_languages", "top_level_entries",
    "manifests", "license_files", "policy_files", "working_tree", "inventoried_at", "error",
]


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    return parser.parse_args()


def list_join(values: list[str], limit: int = 30) -> str:
    return ",".join(values[:limit])


def inventory(row: dict[str, str], base_dir: Path) -> dict[str, object]:
    output: dict[str, object] = {field: "" for field in FIELDS}
    output.update({
        "id": row.get("id", ""),
        "requested_name": row.get("requested_name", ""),
        "clone_path": row.get("clone_path", ""),
        "source_status": row.get("status", ""),
        "inventoried_at": utc_now(),
    })
    try:
        repo = resolve_under_base(row.get("clone_path", ""), base_dir)
    except ValueError as error:
        output["repository_state"] = "invalid"
        output["error"] = str(error)
        return output
    if not repo.exists():
        output["repository_state"] = "missing"
        return output
    if not is_git_repository(repo):
        output["repository_state"] = "not-git"
        return output

    try:
        files = [line for line in git(repo, "ls-files").splitlines() if line]
        language_counts: collections.Counter[str] = collections.Counter()
        top_level: collections.Counter[str] = collections.Counter()
        tracked_bytes = 0
        manifests: list[str] = []
        licenses: list[str] = []
        policies: list[str] = []
        for relative in files:
            path = Path(relative)
            top_level[path.parts[0]] += 1
            language = EXTENSION_LANGUAGES.get(path.suffix.lower())
            if language:
                language_counts[language] += 1
            absolute = repo / path
            try:
                tracked_bytes += absolute.stat().st_size
            except OSError:
                pass
            lower_name = path.name.lower()
            if path.name in MANIFEST_NAMES:
                manifests.append(relative)
            if lower_name.startswith(("license", "copying")):
                licenses.append(relative)
            if any(pattern in lower_name for pattern in POLICY_PATTERNS):
                policies.append(relative)

        output.update({
            "repository_state": "present",
            "origin_url": redact_url(git(repo, "remote", "get-url", "origin")) if git(repo, "remote") else "",
            "head_sha": git(repo, "rev-parse", "HEAD"),
            "head_date": git(repo, "show", "-s", "--format=%cI", "HEAD"),
            "head_subject": truncate(git(repo, "show", "-s", "--format=%s", "HEAD"), 240),
            "branch": git(repo, "branch", "--show-current") or "detached",
            "is_shallow": git(repo, "rev-parse", "--is-shallow-repository"),
            "tracked_files": len(files),
            "tracked_bytes": tracked_bytes,
            "dominant_languages": list_join([f"{name}:{count}" for name, count in language_counts.most_common(8)]),
            "top_level_entries": list_join([f"{name}:{count}" for name, count in top_level.most_common(30)]),
            "manifests": list_join(sorted(manifests)),
            "license_files": list_join(sorted(licenses)),
            "policy_files": list_join(sorted(policies)),
            "working_tree": "clean" if not git(repo, "status", "--porcelain") else "dirty",
        })
    except RuntimeError as error:
        output["repository_state"] = "error"
        output["error"] = truncate(str(error))
    return output


def main() -> int:
    arguments = parse_arguments()
    base_dir = arguments.base_dir.resolve()
    rows = [inventory(row, base_dir) for row in read_tsv(arguments.sources)]
    write_tsv(arguments.output, rows, FIELDS)
    failures = sum(row["repository_state"] not in {"present", "missing"} for row in rows)
    print(f"rows={len(rows)} failures={failures} output={arguments.output}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
