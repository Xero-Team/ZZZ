#!/usr/bin/env python3

"""Download the pinned FFmpeg development package for a ZZZ target."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


RELEASE_TAG = "autobuild-2026-10-01-13-06"
FFMPEG_REVISION = "9.0.2-22-g46d8f462ee"
DOWNLOAD_BASE = (
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/" + RELEASE_TAG
)

PACKAGES = {
    "x86_64-unknown-linux-gnu": {
        "archive": f"ffmpeg-n{FFMPEG_REVISION}-linux64-lgpl-shared-9.0.tar.xz",
        "sha256": "f5eb4ea32b4cc1f3d0e00e52e3fb7b35e0703bcb3d314c1f8ee551b74ecd208a",
    },
    "aarch64-unknown-linux-gnu": {
        "archive": f"ffmpeg-n{FFMPEG_REVISION}-linuxarm64-lgpl-shared-9.0.tar.xz",
        "sha256": "cdbb097c1bc6d71d87b5000ca6543cad10d5a52d10eadec719756cc55c83c45a",
    },
    "x86_64-pc-windows-msvc": {
        "archive": f"ffmpeg-n{FFMPEG_REVISION}-win64-lgpl-shared-9.0.zip",
        "sha256": "2a41605c6c28455c7029e5057f77cda18838203fa09d40bed6e6cac14200e1f5",
    },
    "aarch64-pc-windows-msvc": {
        "archive": f"ffmpeg-n{FFMPEG_REVISION}-winarm64-lgpl-shared-9.0.zip",
        "sha256": "51d0dfe11e2045290eb7eeaae5f4fb33aae88e0b61ceb4d9b785fc94094eb6aa",
    },
}

MARKER_NAME = ".zzz-ffmpeg.json"


def repository_root() -> Path:
    return Path(__file__).resolve().parent.parent


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


def download(url: str, destination: Path) -> None:
    partial = destination.with_suffix(destination.suffix + ".part")
    partial.unlink(missing_ok=True)
    print(f"Downloading {url}", file=sys.stderr)
    request = urllib.request.Request(url, headers={"User-Agent": "ZZZ-FFmpeg-Setup"})
    try:
        with urllib.request.urlopen(request) as response, partial.open("wb") as output:
            shutil.copyfileobj(response, output, length=1024 * 1024)
        partial.replace(destination)
    finally:
        partial.unlink(missing_ok=True)


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


def marker_matches(install_dir: Path, target: str, package: dict[str, str]) -> bool:
    marker = install_dir / MARKER_NAME
    try:
        data = json.loads(marker.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return False
    return (
        data.get("target") == target
        and data.get("archive") == package["archive"]
        and data.get("sha256") == package["sha256"]
        and (install_dir / "include" / "libavcodec" / "avcodec.h").is_file()
        and (install_dir / "lib").is_dir()
    )


def ensure(target: str, dependency_root: Path, force: bool) -> Path:
    package = PACKAGES.get(target)
    if package is None:
        supported = ", ".join(sorted(PACKAGES))
        raise RuntimeError(f"unsupported FFmpeg target {target}; supported: {supported}")

    dependency_root = dependency_root.resolve()
    install_dir = dependency_root / target
    if not force and marker_matches(install_dir, target, package):
        return install_dir

    cache_dir = dependency_root / "cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    archive = cache_dir / package["archive"]
    if not archive.is_file() or sha256(archive) != package["sha256"]:
        archive.unlink(missing_ok=True)
        download(f"{DOWNLOAD_BASE}/{package['archive']}", archive)
    actual_sha256 = sha256(archive)
    if actual_sha256 != package["sha256"]:
        archive.unlink(missing_ok=True)
        raise RuntimeError(
            f"FFmpeg checksum mismatch: expected {package['sha256']}, got {actual_sha256}"
        )

    dependency_root.mkdir(parents=True, exist_ok=True)
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
            "ffmpeg_revision": FFMPEG_REVISION,
            "release_tag": RELEASE_TAG,
            "archive": package["archive"],
            "sha256": package["sha256"],
            "source": "https://github.com/BtbN/FFmpeg-Builds",
        }
        (staging / MARKER_NAME).write_text(
            json.dumps(marker, indent=2) + "\n", encoding="utf-8"
        )
        if install_dir.exists():
            shutil.rmtree(install_dir)
        staging.replace(install_dir)

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
    parser.add_argument("--force", action="store_true", help="reinstall the package")
    args = parser.parse_args()

    try:
        target = args.target or default_target()
        install_dir = ensure(target, args.root, args.force)
    except (OSError, RuntimeError, urllib.error.URLError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    print(install_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
