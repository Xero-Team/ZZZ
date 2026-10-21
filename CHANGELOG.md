# Changelog

All notable changes to ZZZ are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and ZZZ follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
ZZZ has not cut a versioned release yet, so the changes below are unreleased.

## How entries are written

- Add a line under `Added`, `Changed`, `Removed`, or `Fixed`.
- Write it so a ZZZ user can understand it. Skip internal jargon.
- Credit the contributor with `by @handle` when it came from a pull request.
- When a change is absorbed from upstream Zed, cite the upstream pull
  request as `(zed#NNNNN)`. Most ZZZ commits are absorbed from Zed, so
  these references are the norm.
- `Removed` is not a footnote. It records the surfaces ZZZ deliberately
  does not carry, and it is part of the project's evidence that they stay
  gone.

## [Unreleased]

### Added

- Add `SECURITY.md`. Security reports are public and go in the issue
  tracker; there is no private channel.
- Add [The Mission](./docs/src/mission.md), describing why the fork exists.
- Add [Migrating from Zed](./docs/src/migrate/zed.md).
- Add [Keeping ZZZ Lean](./docs/src/development/bloat.md) and
  [GPUI: Ownership and Data Flow](./docs/src/development/ownership-and-data-flow.md).
- Add GitHub pull request and issue templates.
- Add a changelog.
- Add code outline support for diff files (gram#524).
- Add a `project_panel.auto_open.should_focus` setting so a pasted file
  can open without taking focus from the project panel (gram#487).
- Give each git author a stable color derived from their email, so
  commit views can distinguish authors without fetching an avatar.
- Suggest how to build and upload a remote server when none is found on
  the remote host (gram#485).
- Add built-in Typst support, including syntax highlighting, outlines,
  and the tinymist language server (gram#517).
- Show local and remote branch names on entries in the git history list.
- Add a server monitor to terminal mode that samples each connected SSH
  session and shows CPU, memory, swap, disk, load, uptime and network
  throughput in the session file panel.
- Add local crash reporting. Set `ZZZ_GENERATE_MINIDUMPS=1` to have ZZZ write
  a minidump and metadata to its logs directory after a panic. Nothing is
  uploaded; see [Debugging Crashes](./docs/src/development/debugging-crashes.md).
- Add an `extension` project-panel sort order that groups files by their
  last suffix (zed#65776).

### Changed

- Require AI-assisted review for every contribution. Original features
  additionally require human review; upstream absorption needs only the AI
  review.
- Distribute ZZZ under AGPL-3.0-or-later. Upstream Zed license texts are
  retained for the parts they cover.
- Require an extension to honor the user's language server and debug
  adapter binary permissions before it can look up or download a binary
  (gram#507).
- Improve fuzzy search in the agent model and configuration-option
  pickers. Punctuation, provider, group, description, and value-id
  queries match, and results are no longer capped at 100 (zed#64147).

### Removed

- N/A

### Fixed

- Fix a crash while laying out a grid (for example a Markdown or HTML table)
  whose track template or placement described no tracks at all.
- Highlight fenced and inline languages in notebook cells immediately and
  after language grammars load (zed#65893).
- Save agent configuration defaults only after the agent accepts them,
  using the value the agent returns (zed#65849).
- Name both the attempted and existing borrow sites when a GPUI app
  double-borrow panics (zed#65863).
- Rank fuzzy file-search hits closer to the end of the path above earlier
  duplicate matches (zed#59224).
- Remove deleted files from the file finder's history and selection when
  the worktree changes (zed#65660).
- Keep language server notifications and requests in order, so formatting
  requests see pending document changes (zed#65856).
