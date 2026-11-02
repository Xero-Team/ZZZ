#!/usr/bin/env python3

"""Download the newest FFmpeg development package for a ZZZ target."""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import os
import platform
import shutil
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from datetime import datetime, timedelta, timezone
from pathlib import Path, PurePosixPath


GITHUB_REPOSITORY = "BtbN/FFmpeg-Builds"
RELEASES_API = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/releases"
DOWNLOAD_BASE = f"https://github.com/{GITHUB_REPOSITORY}/releases/download"

# `ffmpeg-the-third` links against the FFmpeg 9.0 ABI, so the newest build of
# that series is used. BtbN republishes the `latest` release on every build,
# which keeps the asset names stable while the contents move forward.
FFMPEG_SERIES = "9.0"

# Checksums come from the GitHub API, so a cached install is refreshed on this
# interval to pick up new builds. `--refresh` forces a check immediately.
REFRESH_INTERVAL_DAYS = 7

TARGETS = {
    "x86_64-unknown-linux-gnu": {"platform": "linux64", "extension": "tar.xz"},
    "aarch64-unknown-linux-gnu": {"platform": "linuxarm64", "extension": "tar.xz"},
    "x86_64-pc-windows-msvc": {"platform": "win64", "extension": "zip"},
    "aarch64-pc-windows-msvc": {"platform": "winarm64", "extension": "zip"},
}

MARKER_NAME = ".zzz-ffmpeg.json"
DOWNLOAD_ATTEMPTS = 3
RELEASE_PAGES = 40


def repository_root() -> Path:
    return Path(__file__).resolve().parent.parent


def asset_pattern(spec: dict[str, str]) -> str:
    return (
        f"ffmpeg-n*-{spec['platform']}-lgpl-shared-{FFMPEG_SERIES}.{spec['extension']}"
    )


def default_target() -> str:
    system = platform.system()
    machine = platform.machine().lower()
    architecture = {
        "amd64": "x86_64",
        "x86_64": "x86_64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }.get(machine)
    if architecture is None:
        raise RuntimeError(f"unsupported host architecture: {machine}")
    if system == "Linux":
        return f"{architecture}-unknown-linux-gnu"
    if system == "Windows":
        return f"{architecture}-pc-windows-msvc"
    if system == "Darwin":
        raise RuntimeError(
            "macOS uses the AVFoundation video backend and does not require FFmpeg"
        )
    raise RuntimeError(f"unsupported host operating system: {system}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch_json(url: str, timeout: int = 60) -> object:
    headers = {
        "User-Agent": "ZZZ-FFmpeg-Setup",
        "Accept": "application/vnd.github+json",
    }
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def download(url: str, destination: Path) -> None:
    partial = destination.with_suffix(destination.suffix + ".part")
    print(f"Downloading {url}", file=sys.stderr)
    request = urllib.request.Request(url, headers={"User-Agent": "ZZZ-FFmpeg-Setup"})
    for attempt in range(1, DOWNLOAD_ATTEMPTS + 1):
        partial.unlink(missing_ok=True)
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                with partial.open("wb") as output:
                    shutil.copyfileobj(response, output, length=1024 * 1024)
            partial.replace(destination)
            return
        except urllib.error.HTTPError as error:
            partial.unlink(missing_ok=True)
            if error.code in (403, 404, 410) or attempt == DOWNLOAD_ATTEMPTS:
                raise
            print(
                f"retrying after HTTP {error.code} (attempt {attempt})", file=sys.stderr
            )
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            partial.unlink(missing_ok=True)
            if attempt == DOWNLOAD_ATTEMPTS:
                raise
            print(f"retrying after {error} (attempt {attempt})", file=sys.stderr)
        time.sleep(2 * attempt)
    raise RuntimeError(f"unable to download {url}")


def newest_asset(spec: dict[str, str]) -> dict[str, str]:
    """Resolve the newest release asset for the target's FFmpeg series."""
    pattern = asset_pattern(spec)
    releases = fetch_json(f"{RELEASES_API}?per_page={RELEASE_PAGES}")
    if not isinstance(releases, list):
        raise RuntimeError(f"unexpected response from {RELEASES_API}")

    candidates = []
    for release in releases:
        for asset in release.get("assets", []):
            name = asset.get("name", "")
            digest = asset.get("digest") or ""
            if not fnmatch.fnmatchcase(name, pattern) or not digest.startswith("sha256:"):
                continue
            candidates.append(
                {
                    "release_tag": release.get("tag_name", ""),
                    "archive": name,
                    "sha256": digest[len("sha256:") :],
                    "published_at": release.get("published_at")
                    or release.get("created_at")
                    or "",
                    "url": asset.get("browser_download_url", ""),
                }
            )
    if not candidates:
        raise RuntimeError(
            f"{GITHUB_REPOSITORY} publishes no signed asset matching {pattern}"
        )
    candidates.sort(key=lambda candidate: candidate["published_at"], reverse=True)
    return candidates[0]


def verify_checksum(archive: Path, expected: str, description: str) -> None:
    actual = sha256(archive)
    if actual != expected:
        archive.unlink(missing_ok=True)
        raise RuntimeError(
            f"FFmpeg checksum mismatch for {description}: "
            f"expected {expected}, got {actual}"
        )


def prune_cache(cache_dir: Path, pattern: str, keep: Path) -> None:
    """Drop older archives of the same series so the cache stays bounded."""
    for candidate in cache_dir.iterdir():
        if candidate == keep or not candidate.is_file():
            continue
        if fnmatch.fnmatchcase(candidate.name, pattern):
            candidate.unlink(missing_ok=True)


def cached_archive(asset: dict[str, str], pattern: str, cache_dir: Path) -> Path:
    """Return a verified local copy of `asset`, downloading it when needed."""
    cache_dir.mkdir(parents=True, exist_ok=True)
    archive = cache_dir / asset["archive"]
    if archive.is_file():
        if sha256(archive) == asset["sha256"]:
            return archive
        archive.unlink(missing_ok=True)
    download(asset["url"], archive)
    verify_checksum(archive, asset["sha256"], asset["archive"])
    prune_cache(cache_dir, pattern, archive)
    return archive


def safe_archive_path(name: str) -> bool:
    path = PurePosixPath(name.replace("\\", "/"))
    return not path.is_absolute() and ".." not in path.parts


def extract_archive(archive: Path, destination: Path) -> None:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            if any(not safe_archive_path(item.filename) for item in package.infolist()):
                raise RuntimeError(f"archive contains an unsafe path: {archive}")
            package.extractall(destination)
        return

    with tarfile.open(archive, "r:xz") as package:
        if any(not safe_archive_path(item.name) for item in package.getmembers()):
            raise RuntimeError(f"archive contains an unsafe path: {archive}")
        if hasattr(tarfile, "data_filter"):
            package.extractall(destination, filter="data")
        else:
            package.extractall(destination)


def package_root(extracted: Path) -> Path:
    candidates = [
        path
        for path in extracted.iterdir()
        if path.is_dir()
        and (path / "include" / "libavcodec" / "avcodec.h").is_file()
        and (path / "lib").is_dir()
    ]
    if len(candidates) != 1:
        raise RuntimeError("FFmpeg archive does not contain one development package")
    return candidates[0]


def read_marker(install_dir: Path) -> dict[str, object] | None:
    marker = install_dir / MARKER_NAME
    try:
        data = json.loads(marker.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None
    if not isinstance(data, dict):
        return None
    if not (install_dir / "include" / "libavcodec" / "avcodec.h").is_file():
        return None
    if not (install_dir / "lib").is_dir():
        return None
    return data


def marker_is_current(
    marker: dict[str, object], spec: dict[str, str], refresh_interval: timedelta
) -> bool:
    if marker.get("asset_pattern") != asset_pattern(spec):
        return False
    checked_at = marker.get("checked_at")
    if not isinstance(checked_at, str):
        return False
    try:
        checked = datetime.fromisoformat(checked_at)
    except ValueError:
        return False
    return datetime.now(timezone.utc) - checked < refresh_interval


def install(
    target: str,
    install_dir: Path,
    dependency_root: Path,
    archive: Path,
    spec: dict[str, str],
    asset: dict[str, str],
    checked_at: datetime,
) -> None:
    with tempfile.TemporaryDirectory(prefix="ffmpeg-extract-", dir=dependency_root) as tmp:
        extracted = Path(tmp)
        extract_archive(archive, extracted)
        source = package_root(extracted)
        staging = dependency_root / f".{target}.staging"
        if staging.exists():
            shutil.rmtree(staging)
        shutil.move(str(source), staging)
        marker = {
            "target": target,
            "asset_pattern": asset_pattern(spec),
            "ffmpeg_series": FFMPEG_SERIES,
            "archive": asset["archive"],
            "sha256": asset["sha256"],
            "release_tag": asset["release_tag"],
            "published_at": asset["published_at"],
            "source": asset["url"],
            "checked_at": checked_at.isoformat(),
        }
        (staging / MARKER_NAME).write_text(
            json.dumps(marker, indent=2) + "\n", encoding="utf-8"
        )
        if install_dir.exists():
            shutil.rmtree(install_dir)
        staging.replace(install_dir)


def ensure(target: str, dependency_root: Path, force: bool, refresh: bool) -> Path:
    spec = TARGETS.get(target)
    if spec is None:
        supported = ", ".join(sorted(TARGETS))
        raise RuntimeError(f"unsupported FFmpeg target {target}; supported: {supported}")

    dependency_root = dependency_root.resolve()
    dependency_root.mkdir(parents=True, exist_ok=True)
    install_dir = dependency_root / target
    cache_dir = dependency_root / "cache"

    refresh_days = os.environ.get("ZZZ_FFMPEG_REFRESH_DAYS")
    try:
        refresh_interval = timedelta(
            days=float(refresh_days) if refresh_days else REFRESH_INTERVAL_DAYS
        )
    except ValueError:
        raise RuntimeError(
            f"ZZZ_FFMPEG_REFRESH_DAYS must be a number of days, got {refresh_days!r}"
        ) from None
    marker = None if force else read_marker(install_dir)
    if marker is not None and not refresh and marker_is_current(
        marker, spec, refresh_interval
    ):
        return install_dir

    try:
        asset = newest_asset(spec)
    except (OSError, RuntimeError, urllib.error.URLError) as error:
        if marker is None:
            raise
        print(
            f"warning: keeping the installed FFmpeg package, "
            f"could not check for a newer build: {error}",
            file=sys.stderr,
        )
        return install_dir

    checked_at = datetime.now(timezone.utc)
    if marker is not None and marker.get("sha256") == asset["sha256"]:
        marker["checked_at"] = checked_at.isoformat()
        (install_dir / MARKER_NAME).write_text(
            json.dumps(marker, indent=2) + "\n", encoding="utf-8"
        )
        return install_dir

    print(
        f"Installing FFmpeg {FFMPEG_SERIES} from {asset['release_tag']}/"
        f"{asset['archive']} (published {asset['published_at'] or 'unknown'})",
        file=sys.stderr,
    )
    archive = cached_archive(asset, asset_pattern(spec), cache_dir)
    install(target, install_dir, dependency_root, archive, spec, asset, checked_at)
    return install_dir


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["ensure"])
    parser.add_argument("--target", help="Rust target triple")
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(os.environ.get("ZZZ_FFMPEG_ROOT", repository_root() / ".deps/ffmpeg")),
        help="dependency root (default: .deps/ffmpeg)",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="check for a newer build now instead of after the refresh interval",
    )
    parser.add_argument("--force", action="store_true", help="reinstall the package")
    args = parser.parse_args()

    try:
        target = args.target or default_target()
        install_dir = ensure(target, args.root, args.force, args.refresh)
    except (OSError, RuntimeError, urllib.error.URLError) as error:
        print(f"error: {error}", file=sys.stderr)
        print(
            "hint: set FFMPEG_DIR to an existing FFmpeg development package, or "
            "ZZZ_FFMPEG_ROOT to reuse a download from another checkout.",
            file=sys.stderr,
        )
        return 1

    print(install_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
