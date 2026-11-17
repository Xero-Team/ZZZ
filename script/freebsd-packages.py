#!/usr/bin/env python3
"""Populate a FreeBSD sysroot with the GUI packages ZZZ needs to cross-compile.

`base.txz` is a minimal system: it has no X11, ALSA, fontconfig, Wayland, or
FFmpeg development files. The GUI links against those libraries through `pkg-config`,
so the toolchain image installs their FreeBSD 14 package closure into the same
sysroot the linker wrapper already uses.

Usage:
    freebsd-packages.py --arch amd64 --dest /opt/freebsd/sysroot/amd64
    freebsd-packages.py --arch arm64 --dest /opt/freebsd/sysroot/arm64

The proxy variables (http_proxy/https_proxy) are inherited from the environment.
"""

import argparse
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import urllib.parse
import urllib.request

# Seeds cover every `pkg_config::Config::probe` in the FreeBSD GUI dependency
# graph: x11 (x11-rs), alsa (alsa-sys), fontconfig (yeslogic-fontconfig-sys),
# freetype2 (freetype-sys), libxkbcommon / wayland (wayland-sys, xkbcommon),
# and ffmpeg (ffmpeg-sys-the-third). `libX11` pulls in the rest of the X11
# closure; ffmpeg pulls in its codec/format/scaling runtime.
SEEDS = [
    "libX11",
    "alsa-lib",
    "fontconfig",
    "freetype2",
    "libxkbcommon",
    "wayland",
    "ffmpeg",
]

ABI = {
    "amd64": "FreeBSD:14:amd64",
    "arm64": "FreeBSD:14:aarch64",
}


def build_opener():
    """Honor both lower-case (host) and upper-case (podman build-arg) proxies.

    Podman injects `HTTP_PROXY`/`HTTPS_PROXY` build args as upper-case
    environment variables, while `urllib`'s automatic proxy detection is
    easiest to reason about when we build the handler explicitly.
    """
    proxies = {}
    for name, value in os.environ.items():
        lowered = name.lower()
        # Only http/https are valid urllib proxy schemes; `all_proxy` is a
        # curl/socks convention and would create an inert "all_open" handler.
        if lowered in ("http_proxy", "https_proxy") and value:
            proxies[lowered.split("_")[0]] = value
    if proxies:
        return urllib.request.build_opener(urllib.request.ProxyHandler(proxies))
    return urllib.request.build_opener()


def fetch(url, timeout=240):
    req = urllib.request.Request(url, headers={"User-Agent": "curl/8"})
    last_error = None
    for attempt in range(3):
        try:
            with build_opener().open(req, timeout=timeout) as resp:
                return resp.read()
        except Exception as error:  # noqa: BLE001 - transient proxy/network errors
            last_error = error
            if attempt < 2:
                print(f"  retry {attempt + 1}/3 for {url}: {error}", flush=True)
    raise RuntimeError(f"failed to fetch {url}: {last_error}")


def zstd_decompress(data):
    result = subprocess.run(["zstd", "-d", "-c"], input=data, capture_output=True)
    if result.returncode != 0:
        raise RuntimeError("zstd failed: " + result.stderr.decode("utf-8", "replace")[:300])
    return result.stdout


def load_index(base):
    raw = zstd_decompress(fetch(base + "/packagesite.pkg"))
    records = {}
    with tarfile.open(fileobj=io.BytesIO(raw)) as tf:
        for member in tf.getmembers():
            if not (member.name.endswith("packagesite.yaml") and member.isfile()):
                continue
            stream = tf.extractfile(member)
            for line in stream:
                line = line.strip()
                if not line or line.startswith(b"#"):
                    continue
                try:
                    record = json.loads(line)
                except ValueError:
                    continue
                records[record["name"]] = record
            break
    return records


def resolve_closure(records):
    needed, queue, seen = {}, list(SEEDS), set()
    while queue:
        name = queue.pop(0)
        if name in seen:
            continue
        seen.add(name)
        record = records.get(name)
        if record is None:
            print(f"  [warn] no package record for {name}", flush=True)
            continue
        needed[name] = record
        for dep in record.get("deps", {}):
            if dep not in seen:
                queue.append(dep)
    return needed


def extract_package(raw, dest):
    """Unpack one FreeBSD `.pkg` into the sysroot.

    `tarfile.extract`'s default `data_filter` rejects absolute symlinks, and
    several packages ship convenience links such as
    `usr/local/share/p11-kit/certs/anchors -> /etc/ssl/certs`. Inside a cross
    sysroot an absolute link target is meaningless (it would point at the
    container's own paths), so we skip absolute links but keep relative ones —
    the `libX11.so -> libX11.so.6` style links are required by the linker.
    """
    with tarfile.open(fileobj=io.BytesIO(raw)) as tf:
        members = [
            m
            for m in tf.getmembers()
            if m.name.lstrip("/")
            # `+MANIFEST`/`+COMPACT_MANIFEST` are per-package pkg(8) metadata.
            # Every package ships one, they overwrite each other, and a cross
            # sysroot never needs them.
            and not os.path.basename(m.name).startswith("+")
        ]
        # Regular files and directories first, so link targets exist.
        for member in members:
            name = member.name.lstrip("/")
            if name.startswith(".."):
                continue
            target = os.path.join(dest, name)
            if member.isdir():
                os.makedirs(target, exist_ok=True)
            elif member.isfile():
                os.makedirs(os.path.dirname(target), exist_ok=True)
                source = tf.extractfile(member)
                if source is None:
                    continue
                with open(target, "wb") as output:
                    shutil.copyfileobj(source, output)
                os.chmod(target, member.mode)
        for member in members:
            name = member.name.lstrip("/")
            if name.startswith(".."):
                continue
            target = os.path.join(dest, name)
            if member.issym():
                if os.path.isabs(member.linkname):
                    continue
                os.makedirs(os.path.dirname(target), exist_ok=True)
                if os.path.lexists(target):
                    os.remove(target)
                os.symlink(member.linkname, target)
            elif member.islnk():
                source = os.path.join(dest, member.linkname.lstrip("/"))
                if not os.path.exists(source):
                    continue
                os.makedirs(os.path.dirname(target), exist_ok=True)
                if os.path.lexists(target):
                    os.remove(target)
                os.link(source, target)


def install_packages(base, dest, needed):
    os.makedirs(dest, exist_ok=True)
    for name, record in sorted(needed.items()):
        repopath = record["repopath"]
        url = base + "/" + urllib.parse.quote(repopath, safe="/$,~")
        data = fetch(url, 180)
        raw = zstd_decompress(data)
        extract_package(raw, dest)
        print(f"  installed {name}", flush=True)
    return len(needed)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--arch", choices=sorted(ABI), required=True)
    parser.add_argument("--dest", required=True)
    args = parser.parse_args()

    abi = ABI[args.arch]
    base = f"https://pkg.freebsd.org/{abi}/latest"
    print(f"== FreeBSD GUI packages: {args.arch} ({abi}) -> {args.dest}", flush=True)
    records = load_index(base)
    print(f"  index: {len(records)} packages", flush=True)
    needed = resolve_closure(records)
    print(f"  closure: {len(needed)} packages", flush=True)
    count = install_packages(base, args.dest, needed)
    print(f"  done: {count} packages", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
