---
title: Building ZZZ for Linux
description: "Guide to building ZZZ for Linux development."
---

# Building ZZZ for Linux

## Repository

Clone the [ZZZ repository](https://github.com/Xero-Team/ZZZ).

## Dependencies

- Install [rustup](https://www.rust-lang.org/tools/install)

- Install the necessary system libraries:

  ```sh
  script/linux
  ```

  If you prefer to install the system libraries manually, you can find the list of required packages in the `script/linux` file.

## Building from source

Once the dependencies are installed, you can build ZZZ using [Cargo](https://doc.rust-lang.org/cargo/).

For a debug build of the editor:

```sh
cargo run
```

And to run the tests:

```sh
cargo test --workspace
```

In release mode, the primary user interface is the `cli` crate. You can run it in development with:

```sh
cargo run -p cli
```

## Installing a development build

You can install a local build on your machine with:

```sh
./script/install-linux
```

This builds `~/.local/bin/zzz` and the `cli` in release mode, installs the binary at `~/.local/bin/zzz`, and installs `.desktop` files to `~/.local/share`.

## Cross-compiling Windows executables

To build `zzz.exe` on Linux without an MSI, see
[Cross-compiling from Linux](./windows.md#cross-compiling-from-linux).

## Wayland & X11

ZZZ supports both X11 and Wayland. By default, we pick whichever we can find at runtime. If you're on Wayland and want to run in X11 mode, use the environment variable `WAYLAND_DISPLAY=''`.

## Notes for packaging ZZZ

This section is for distribution maintainers packaging ZZZ.

### Technical requirements

ZZZ has two main binaries:

- You will need to build `crates/cli` and make its binary available in `$PATH` with the name `zzz`.
- You will need to build `~/.local/lib/zzz/zzz-editor` and put it at `~/.local/lib/zzz/zzz-editor`. For example, if you are going to put the CLI at `~/.local/lib/zzz/zzz-editor`, put `zzz` at `~/.local/lib/zzz/zzz-editor`. As some Linux distributions (notably Arch) discourage the use of `libexec`, you can also put this binary at `~/.local/lib/zzz/zzz-editor` instead.
- If you are going to provide a `.desktop` file you can find a template in `crates/zzz/resources/zzz.desktop.in`, and use `envsubst` to populate it with the values required. This file should also be renamed to `$APP_ID.desktop` so that the file [follows the FreeDesktop standards](`crates/zzz/resources/zzz.desktop.in`). You should also make this desktop file executable (`chmod 755`).
- You will need to ensure that the necessary libraries are installed. You can get the current list by inspecting the built binary; see [`script/bundle-linux`](../../script/bundle-linux).
- For an example of a complete build script, see [`script/bundle-linux`](../../script/bundle-linux).
- You can disable ZZZ's auto updates and provide instructions for users who try to update ZZZ manually by building (or running) ZZZ with the environment variable `ZZZ_UPDATE_EXPLANATION`. For example: ZZZ_UPDATE_EXPLANATION.
- Make sure to update the contents of the `crates/zzz/RELEASE_CHANNEL` file to `stable` or `dev`, with no newline. Packaged builds that should use the system credentials manager should use `stable`.

### Other things to note

ZZZ moves quickly, and distribution maintainers often have different constraints and priorities. The points below describe current trade-offs:

- ZZZ is a fast-moving project. We typically publish 2-3 builds per week to address reported issues and ship larger changes.
- There are a couple of other `zed-cli` binaries that may be present on Linux systems ([1](`zed-cli`), [2](`zed-cli`)). If you want to rename our CLI binary because of these issues, we suggest `zedit`, `zeditor`, or `zed-cli`.
- ZZZ automatically installs versions of common developer tools, similar to rustup/rbenv/pyenv.
- Users can install extensions locally and from the public Zed marketplace at `https://api.zed.dev`. Extensions may install additional tools such as language servers.
- A fresh ZZZ install does not create an account, send telemetry, or contact hosted collaboration. Language-server, debug adapter, Prettier, Node, and extension downloads may contact third-party hosts.
- Because of the points above, ZZZ currently does not work well with sandboxes.

## Flatpak

> ZZZ's current Flatpak integration exits the sandbox on startup. Workflows that rely on Flatpak's sandboxing may not work as expected.

To build & install the Flatpak package locally follow the steps below:

1. Install Flatpak for your distribution as outlined [here](https://flathub.org/setup).
2. Run the `script/flatpak/deps` script to install the required dependencies.
3. Run `script/flatpak/bundle-flatpak`.
4. Now the package has been installed and has a bundle available at `target/release/{app-id}.flatpak`.

## Memory profiling

[`heaptrack`](https://github.com/KDE/heaptrack) is quite useful for diagnosing memory leaks. To install it:

```sh
$ sudo apt install heaptrack heaptrack-gui
$ cargo install cargo-heaptrack
```

Then, to build and run ZZZ with the profiler attached:

```sh
$ cargo heaptrack -b zzz
```

When this ZZZ instance is exited, terminal output will include a command to run `heaptrack_interpret` to convert the `*.raw.zst` profile to a `*.zst` file which can be passed to `heaptrack_gui` for viewing.

## Perf recording

How to get a flamegraph with resolved symbols from a running ZZZ instance.
Use this when ZZZ is using a lot of CPU. It is not useful for hangs.

### During the incident

- Find the PID (process ID) using:
  `ps -eo size,pid,comm | grep zzz | sort | head -n 1 | cut -d ' ' -f 2`
  Or find the PID of `zzz-editor` with the highest RAM usage in something
  like htop/btop/top.

- Install perf:
  On Ubuntu (derivatives) run `sudo apt install linux-tools`.

- Perf record:
  Run `sudo perf record -p <pid you just found>`, wait a few seconds to gather data, then press Ctrl+C. You should now have a `perf.data` file.

- Make the output file user owned:
  run `sudo chown $USER:$USER perf.data`

- Get build info:
  Run ZZZ again and type `zzz: about` in the command pallet to get the exact commit.

The `perf.data` file can be sent to ZZZ together with the exact commit.

### Later

This can be done by ZZZ staff.

- Build ZZZ with symbols:
  Check out the commit found previously and modify `Cargo.toml`.
  Apply the following diff, then make a release build.

```diff
[profile.release]
-debug = "limited"
+debug = "full"
```

- Add the symbols to the perf database:
  `perf buildid-cache -v -a <path to release zzz binary>`

- Resolve the symbols from the db:
  `perf inject -i perf.data -o perf_with_symbols.data`

- Install flamegraph:
  `cargo install cargo-flamegraph`

- Render the flamegraph:
  `flamegraph --perfdata perf_with_symbols.data`

## Troubleshooting

### Cargo errors claiming that a dependency is using unstable features

Try `cargo clean` and `cargo build`.
