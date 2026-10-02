#!/usr/bin/env python3
"""Initialize a repository-research workspace without overwriting files."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, default=Path(".tmp/skill_ref"))
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    workspace = arguments.workspace.resolve()
    assets = Path(__file__).resolve().parent.parent / "assets"
    workspace.mkdir(parents=True, exist_ok=True)
    (workspace / "surveys").mkdir(exist_ok=True)

    copies = {
        assets / "SOURCES.tsv": workspace / "SOURCES.tsv",
        assets / "EVIDENCE.tsv": workspace / "EVIDENCE.tsv",
        assets / "EXPERIMENTS.tsv": workspace / "EXPERIMENTS.tsv",
        assets / "report-template.md": workspace / "REPORT.md",
    }
    for source, destination in copies.items():
        if destination.exists():
            print(f"preserved\t{destination}")
            continue
        shutil.copyfile(source, destination)
        print(f"created\t{destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
