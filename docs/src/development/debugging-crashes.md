---
title: Debugging Crashes
description: "Generate and read local crash minidumps in ZZZ."
---

# Debugging Crashes

ZZZ does not upload crash reports. When you enable local crash reporting, ZZZ
runs a sidecar process that inspects the editor's memory after a panic and
writes a [minidump](https://chromium.googlesource.com/breakpad/breakpad/+/master/docs/getting_started_with_breakpad.md#the-minidump-file-format)
plus a JSON metadata file into the logs directory. Both files stay on your
machine.

## Enable local crash reports {#enable}

Crash reporting is off by default. Set the environment variable before you
start ZZZ:

```sh
ZZZ_GENERATE_MINIDUMPS=1 zzz
```

Stable builds install the sidecar automatically. Development builds only
install it when the variable is set, so set `ZZZ_GENERATE_MINIDUMPS=0` to keep
it off in a stable build.

The same variable is forwarded to a remote server when you connect over SSH,
WSL, or a dev container.

## Read a minidump {#read}

The minidump is compressed with zstd. Decompress it, then walk the stacks:

```sh
zstd -d ~/.local/share/zzz/<session-id>.dmp -o minidump.dmp
minidump-stackwalk minidump.dmp
```

The logs directory is `~/Library/Logs/ZZZ` on macOS, `~/.local/share/zzz/logs`
or `$XDG_DATA_HOME/zzz/logs` on Linux and FreeBSD, and
`%LOCALAPPDATA%\ZZZ\logs` on Windows.

Alongside the minidump you find a `<session-id>.json` file with the panic
message, span, and system specs. ZZZ strips string-slicing panic payloads so
that arbitrary text is not written into the report.

A minidump is hard to read without symbols. Download the source and an
unstripped binary, or a separate symbols file, for your ZZZ build before you
walk the stacks.

## Using a debugger {#debugger}

If you can reproduce the crash consistently, use a debugger to inspect program
state at the crash point. See
[Using Debuggers](./debuggers.md#debugging-panics-and-crashes).
