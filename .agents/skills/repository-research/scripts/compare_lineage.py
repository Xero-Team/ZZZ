#!/usr/bin/env python3
"""Compare shared paths across checkouts to quantify common ancestry."""

from __future__ import annotations

import argparse
import itertools
from pathlib import Path

from _common import git, is_git_repository, read_tsv, resolve_under_base, write_tsv

FIELDS = [
    "repository_a",
    "repository_b",
    "head_a",
    "head_b",
    "path_filter",
    "common_paths",
    "byte_identical",
    "changed_common_paths",
    "only_a",
    "only_b",
]


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    parser.add_argument(
        "--paths",
        default="",
        help="comma-separated path prefixes to compare, for example crates/gpui,crates/editor",
    )
    return parser.parse_args()


def list_blobs(repo: Path, prefixes: list[str]) -> dict[str, str]:
    blobs: dict[str, str] = {}
    for entry in git(repo, "ls-tree", "-r", "-z", "--full-tree", "HEAD").split("\0"):
        if not entry:
            continue
        meta, _, path = entry.partition("\t")
        fields = meta.split()
        if len(fields) < 3 or fields[1] != "blob":
            continue
        if prefixes and not any(path == prefix or path.startswith(prefix + "/") for prefix in prefixes):
            continue
        blobs[path] = fields[2]
    return blobs


def main() -> int:
    arguments = parse_arguments()
    base_dir = arguments.base_dir.resolve()
    prefixes = [item.strip() for item in arguments.paths.split(",") if item.strip()]

    checkouts: list[tuple[str, Path, str]] = []
    failures = 0
    for row in read_tsv(arguments.sources):
        if row.get("status") == "unavailable":
            continue
        identifier = row.get("id") or row.get("requested_name") or ""
        try:
            repo = resolve_under_base(row.get("clone_path", ""), base_dir)
        except ValueError as error:
            print(f"error\t{identifier}\t{error}")
            failures += 1
            continue
        if not is_git_repository(repo):
            print(f"missing\t{identifier}\t{repo}")
            failures += 1
            continue
        checkouts.append((identifier, repo, git(repo, "rev-parse", "HEAD")))

    blobs = {identifier: list_blobs(repo, prefixes) for identifier, repo, _ in checkouts}
    rows: list[dict[str, object]] = []
    for (id_a, _, head_a), (id_b, _, head_b) in itertools.combinations(checkouts, 2):
        blob_a = blobs[id_a]
        blob_b = blobs[id_b]
        common = blob_a.keys() & blob_b.keys()
        identical = sum(1 for path in common if blob_a[path] == blob_b[path])
        rows.append(
            {
                "repository_a": id_a,
                "repository_b": id_b,
                "head_a": head_a,
                "head_b": head_b,
                "path_filter": ",".join(prefixes) or "whole-tree",
                "common_paths": len(common),
                "byte_identical": identical,
                "changed_common_paths": len(common) - identical,
                "only_a": len(blob_a) - len(common),
                "only_b": len(blob_b) - len(common),
            }
        )

    write_tsv(arguments.output, rows, FIELDS)
    print(f"pairs={len(rows)} failures={failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
