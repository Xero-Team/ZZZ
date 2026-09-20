---
title: Remote Development in ZZZ - SSH Workflows
description: Use remote development in ZZZ to edit code over SSH with local UI performance, remote terminals, language servers, and tasks.
---

# Remote Development

Remote Development lets you edit code on a remote server while running ZZZ locally. The UI stays responsive because it runs on your machine, while language servers, tasks, and terminals run on the server.

For day-to-day workflows, pair remote development with [Tasks](./tasks.md),
[Terminal](./terminal.md), and [Debugger](./debugger.md).

## Overview

Remote development requires two computers, your local machine that runs the ZZZ UI and the remote server which runs a ZZZ headless server. The two communicate over SSH, so you will need to be able to SSH from your local machine into the remote server to use this feature.

On your local machine, ZZZ runs its UI, talks to language models, uses Tree-sitter to parse and syntax-highlight code, and store unsaved changes and recent projects. The source code, language servers, tasks, and the terminal all run on the remote server. [AI features](./ai/overview.md) work in remote sessions, including the Agent Panel and Inline Assistant.

> **Note:** Remote development talks to the remote host over SSH. ZZZ does not route that traffic through a hosted collaboration service.

## Setup

1. Build ZZZ from source. See [Installation](./installation.md).
1. Use {#kb projects::OpenRemote} to open the "Remote Projects" dialog.
1. Click "Connect New Server" and enter the command you use to SSH into the server. See [Supported SSH options](#supported-ssh-options) for options you can pass.
1. Your local machine will attempt to connect to the remote server using the `ssh` binary on your path. Assuming the connection is successful, ZZZ starts the embedded `remote_server` on the remote host.
1. Once the ZZZ server is running, you will be prompted to choose a path to open on the remote server.
   > **Note:** ZZZ does not currently handle opening very large directories (for example, `/` or `~` that may have >100,000 files) very well. We are working on improving this, but suggest in the meantime opening only specific projects, or subfolders of very large mono-repos.

For simple cases where you don't need any SSH arguments, you can run `zzz://ssh/[<user>@]<host>[:<port>]/<path>` to open a remote folder/file directly. The CLI also accepts the scp style `zzz ssh://[<user>@]<host>[:<port>]/<path>` or `zzz ssh://[<user>@]<host>[:<port>]/<path>`. If you'd like to hotlink into an SSH project, use a link of the format: `zzz://ssh/[<user>@]<host>[:<port>]/<path>`.

## Supported platforms

The remote machine must be able to run ZZZ's server. The following platforms should work, though note that we have not exhaustively tested every Linux distribution:

- macOS Catalina or later (Intel or Apple Silicon)
- Linux (x86_64 or arm64, we do not yet support 32-bit platforms)
- Windows is not yet supported as a remote server, but Windows can be used as a local machine to connect to remote servers.

## Configuration

The list of remote servers is stored in your settings file {#kb zzz::OpenSettings}. You can edit this list using the Remote Projects dialog {#kb projects::OpenRemote}, which provides some robustness - for example it checks that the connection can be established before writing it to the settings file.

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "projects": [{ "paths": ["~/code/zzz/zzz"] }]
    }
  ]
}
```

ZZZ shells out to the `ssh` on your path, and so it will inherit any configuration you have in `~/.ssh/config` for the given host. That said, if you need to override anything you can configure the following additional options on each connection:

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "projects": [{ "paths": ["~/code/zzz/zzz"] }],
      // any argument to pass to the ssh master process
      "args": ["-i", "~/.ssh/work_id_file"],
      "port": 22, // defaults to 22
      // defaults to your username on your local machine
      "username": "me"
    }
  ]
}
```

There is one additional ZZZ-specific option per connection, `nickname`:

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "projects": [{ "paths": ["~/code/zzz/zzz"] }],
      // Shown in the ZZZ UI to help distinguish multiple hosts.
      "nickname": "lil-linux"
    }
  ]
}
```

If you use the command line to open a connection to a host by doing `zzz ssh://192.168.1.10/~/.vimrc`, then extra options are read from your settings file by finding the first connection that matches the host/username/port of the URL on the command line.

Additionally it's worth noting that while you can pass a password on the command line `zzz ssh://user:password@host/~`, we do not support writing a password to your settings file. If you're connecting repeatedly to the same host, you should configure key-based authentication.

## Remote Development on Windows (SSH)

ZZZ on Windows supports SSH remoting and will prompt for credentials when needed.

If you encounter authentication issues, confirm that your SSH key agent is running (e.g., ssh-agent or your Git client's agent) and that ssh.exe is on PATH.

### Troubleshooting SSH on Windows

When prompted for credentials, use the graphical askpass dialog. If it doesn't appear, check for credential manager conflicts and that GUI prompts aren't blocked by your terminal.

## WSL Support

ZZZ supports opening folders inside of WSL natively on Windows.

### Opening a local folder in WSL

To open a local folder inside a WSL container, use the {#action projects::OpenFolderInWsl} action and select the folder you want to open. You will be presented with a list of available WSL distributions to open the folder in.

### Opening a folder already in WSL

To open a folder that's already located inside of a WSL container, use the {#action projects::OpenWsl} action and select the WSL distribution. The distribution will be added to the `Remote Projects` window where you will be able to open the folder.

## Port forwarding

If you'd like to be able to connect to ports on your remote server from your local machine, you can configure port forwarding in your settings file. This is particularly useful for developing websites so you can load the site in your browser while working.

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "port_forwards": [{ "local_port": 8080, "remote_port": 80 }]
    }
  ]
}
```

This will cause requests from your local machine to `localhost:8080` to be forwarded to the remote machine's port 80. Under the hood this uses the `-L` argument to ssh.

By default these ports are bound to localhost, so other computers in the same network as your development machine cannot access them. You can set the local_host to bind to a different interface, for example, 0.0.0.0 will bind to all local interfaces.

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "port_forwards": [
        {
          "local_port": 8080,
          "remote_port": 80,
          "local_host": "0.0.0.0"
        }
      ]
    }
  ]
}
```

These ports also default to the `localhost` interface on the remote host. If you need to change this, you can also set the remote host:

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "port_forwards": [
        {
          "local_port": 8080,
          "remote_port": 80,
          "remote_host": "docker-host"
        }
      ]
    }
  ]
}
```

## ZZZ settings

When opening a remote project there are three relevant settings locations:

- The local ZZZ settings (in `~/.config/ZZZ/settings.json` on macOS and Linux) on your local machine.
- The server ZZZ settings (in the same place) on the remote server.
- The project settings (in `.ZZZ/settings.json` or `.editorconfig` of your project)

Both the local ZZZ and the server ZZZ read the project settings, but they are not aware of the other's main settings file.

Which settings file you should use depends on the kind of setting you want to make:

- Project settings should be used for things that affect the project: indentation settings, which formatter / language server to use, etc.
- Server settings should be used for things that affect the server: paths to language servers, proxy settings, etc.
- Local settings should be used for things that affect the UI: font size, etc.

In addition any extensions you have installed locally will be propagated to the remote server. This means that language servers, etc. will run correctly.

## Proxy Configuration

The remote server will not use your local machine's proxy configuration because they may be under different network policies. If your remote server requires a proxy to access the internet, you must configure it on the remote server itself.

In most cases, your remote server will already have proxy environment variables configured. ZZZ will automatically use them when downloading language servers, communicating with LLM models, etc.

If needed, you can set these environment variables in the server's shell configuration (e.g., `~/.bashrc`):

```bash
export http_proxy="http://proxy.example.com:8080"
export https_proxy="http://proxy.example.com:8080"
export no_proxy="localhost,127.0.0.1"
```

Alternatively, you can configure the proxy in the remote machine's `~/.config/ZZZ/settings.json`:

```json
{
  "proxy": "http://proxy.example.com:8080"
}
```

See the [proxy documentation](./reference/all-settings.md#network-proxy) for supported proxy types and additional configuration options.

## Initializing the remote server

Once you provide the SSH options, ZZZ shells out to `ssh` on your local machine to create a ControlMaster connection with the options you provide.

Any prompts that SSH needs will be shown in the UI, so you can verify host keys, type key passwords, etc.

Once the master connection is established, ZZZ will check to see if the remote
server binary is present in `~/.zzz_server` on the remote, and that its version
matches the current version of ZZZ that you're using.

If it is not there or the version mismatches, a non-debug ZZZ build uploads an
embedded `remote_server` archive over SSH for the remote OS and architecture.
`script/bundle-mac` embeds the host macOS archive plus Linux x86_64 and
aarch64 musl archives (`zig` and `cargo-zigbuild` are required).
`script/bundle-linux` embeds the host Linux archive and, when Zig is
available, Linux aarch64. Other archives are embedded when that target can
be compiled on the build machine. Cross-compiling macOS `remote_server` from
Linux downloads a macOS SDK into a temporary directory when `zig` and
`cargo-zigbuild` are available.

If no matching archive is embedded, ZZZ errors unless the binary is already on
the remote, or a debug `cargo run` compiles `remote_server` from source
(`ZZZ_BUILD_REMOTE_SERVER`, default `nocompress`). Force embedding in a debug
or Dev build with `ZZZ_EMBED_REMOTE_SERVERS=1`.

Debug `cargo run` builds do not embed archives.

If you'd like to maintain the server binary yourself, build it with
`cargo build -p remote_server --release` and upload it to `~/.zzz_server` on
the server. The filename must match the ZZZ version you are using, for example
`~/.zzz_server/zzz-remote-server-dev-build` for Dev or
`~/.zzz_server/zzz-remote-server-stable-1.19.0` for Stable.

## Maintaining the SSH connection

Once the server is initialized. ZZZ will create new SSH connections (reusing the existing ControlMaster) to run the remote development server.

Each connection tries to run the development server in proxy mode. This mode will start the daemon if it is not running, and reconnect to it if it is. This way when your connection drops and is restarted, you can continue to work without interruption.

In the case that reconnecting fails, the daemon will not be re-used. That said, unsaved changes are by default persisted locally, so that you do not lose work. You can always reconnect to the project at a later date and ZZZ will restore unsaved changes.

If you are struggling with connection issues, you should be able to see more information in the ZZZ log `cmd-shift-p Open Log`. If you are seeing things that are unexpected, please open an issue in the project tracker.

## Supported SSH Options

Under the hood, ZZZ shells out to the `ssh` binary to connect to the remote server. We create one SSH control master per project, and then use that to multiplex SSH connections for the ZZZ protocol itself, any terminals you open and tasks you run. We read settings from your SSH config file, but if you want to specify additional options to the SSH control master you can configure ZZZ to set them.

When typing in the "Connect New Server" dialog, you can use bash-style quoting to pass options containing a space. Once you have created a server it will be added to the `"ssh_connections": []` array in your settings file. You can edit the settings file directly to make changes to SSH connections.

Supported options:

- `-p` / `-l` - these are equivalent to passing the port and the username in the host string.
- `-L` / `-R` for port forwarding
- `-i` - to use a specific key file
- `-o` - to set custom options
- `-J` / `-w` - to proxy the SSH connection
- `-F` for specifying an `ssh_config`
- And also... `-4`, `-6`, `-A`, `-B`, `-C`, `-D`, `-I`, `-K`, `-P`, `-X`, `-Y`, `-a`, `-b`, `-c`, `-i`, `-k`, `-l`, `-m`, `-o`, `-p`, `-w`, `-x`, `-y`

Note that we deliberately disallow some options (for example `-t` or `-T`) that ZZZ will set for you.

## Known Limitations

- You can't open files from the remote Terminal by typing the `zzz` command.

## See also

- [Running & Testing](./running-testing.md): Run tasks, terminal commands, and
  debugger sessions while you work remotely.
- [Git Worktrees](./git.md#git-worktrees): Create and switch between linked
  Git worktrees. ZZZ supports the worktree picker in remote projects when the
  remote connection is active.
- [Configuring ZZZ](./configuring-zzz.md): Manage shared and project settings,
  including `.ZZZ/settings.json`.
- [Agent Panel](./ai/agent-panel.md): Use AI workflows in remote projects.
