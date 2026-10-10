# Terminal Mode (FinalShell-style) — Complete Implementation Plan

Status: implemented; follow-up correctness fixes and verification completed
Scope: complete (all phases P0–P5)
Owner: to be executed by an OpenCode agent, one phase per "继续"

This document is self-contained. It contains the requirements, the locked
architecture decisions, the evidence for each decision (file:line), the
file-level change list, the code-convention constraints, and the verification
commands. A fresh agent must be able to implement the feature from this file
alone, without redoing the research.

---

## 1. Requirements

| # | Requirement | Semantics |
|---|---|---|
| R1 | Left panel: SSH connection list | Always present. Reads saved `ssh_connections` plus `~/.ssh/config` hosts. |
| R2 | Center: terminal, multiple hosts concurrently | A terminal on machine A and one on machine B coexist in one window; both keep running in the background. |
| R3 | Bottom panel closed by default | Entering terminal mode closes the bottom dock. |
| R4 | Right panel: file tree follows the active terminal | "Whichever window is selected, show its tree." |
| R5 | Default to remote `$HOME` | Terminal cwd and file-tree root both land in the remote user's home. |
| R6 | Cross-host file copy | A -> local relay -> B, invisible to the user (no manual download/upload). |

### Locked decisions

- **File preview shape (R4):** the preview editor lives in the **lower half of
  the right panel** (FinalShell style). The right panel is split: top = file
  tree, bottom = preview pane whose project is the active session's project.
- **File tree following the terminal's current directory:** **out of scope for
  v1.** ZZZ does not track remote cwd (`terminal.rs:2366` returns `None` for
  remote terminals), and competitors struggle with this too. The tree root is
  the session's remote `$HOME`; navigation within the tree is manual.

---

## 2. Architecture

**Decision: one window hosts N "session projects", one per SSH connection.**

Each session owns its own `RemoteClient` + `Project` + `ProjectPanel`. The
window's `Workspace` keeps a primary project for non-terminal use, while
terminal-mode sessions are additional `Project::remote(...)` entities that the
session store owns.

```
Left panel SshSessionPanel      Center: workspace center PaneGroup      Right panel SessionFilePanel
  connection list + status        [machine A terminal] [machine B term]   top:    ProjectPanel (session $HOME)
  weak handle to SessionStore     each TerminalView -> session project    bottom: Pane(session project) preview
        |                                     |                                   ^
        +-------- SessionStore (N x SshSession) -----------+                        |
                    active_session / VfsManager (R6)       |  observe active ------+
                                                           v
        subscribe to center pane events (ActivateItem/Focus) -> resolve active TerminalView -> project -> session
```

### Why this works — evidence

Three facts, each verified in code, make "N foreign-project sessions in one
window" viable:

1. **`TerminalView` / `Pane` are not bound to `workspace.project()`.**
   - `TerminalView` stores its own `project: WeakEntity<Project>`
     (`terminal_view.rs:128`), used only in `handle_drop` (`:1606`) and
     `clone_on_split` (`:1774`).
   - `Terminal` holds no project/remote handle, only `is_remote_terminal: bool`
     (`terminal.rs:920-959`, field at `:940`).
   - `Pane`/`Workspace::add_item` never assert `item.project ==
     workspace.project()` (`workspace.rs:4546-4585`, `pane.rs:1245-1363`).
   - `Pane::new(workspace, project, ...)` accepts any project (`pane.rs:569`).
2. **`ProjectPanel` touches `workspace.project()` exactly once** — the
   constructor at `project_panel.rs:643` (`let project =
   workspace.project().clone();`). Everywhere else it uses `self.project`
   (`project_panel.rs:143`). Parameterizing it is a one-line change plus
   callers.
3. **The cross-host copy engine already exists** —
   `transfer_cross_provider` (`vfs/src/transfer.rs:406`) streams source
   `read_at` -> local buffer -> target `write_at`, with staging, SHA-256
   verification and atomic rename. `VfsManager` (`manager.rs:8`),
   `VfsSnapshot::new(mount_id, provider, budgets)` (`snapshot.rs:503`) and
   `MountId::new(u64)` (`vfs.rs:397`) are all public.

### Rejected alternative

`MultiWorkspace` already holds N workspaces (each = 1 project = 1 remote
connection) in one window (`multi_workspace.rs:282-295`) and retains them
running, but renders only the active one (`:2147`). It cannot show two hosts'
terminals side by side and cannot drive a "follow the active terminal" right
panel. Kept only as conceptual reference.

---

## 3. New crate: `crates/terminal_mode`

Module files (no `mod.rs`; declare modules in the lib root). Per `.rules:15`,
name the library root explicitly in `Cargo.toml` with `[lib] path = "..."`.

```
crates/terminal_mode/
  Cargo.toml                 # [lib] path = "src/terminal_mode.rs"
  src/
    terminal_mode.rs         # crate root: pub mod ...; pub fn init(cx: &mut App)
    session.rs               # SshSession, SessionStore
    session_panel.rs         # left panel SshSessionPanel (impl Panel)
    file_panel.rs            # right panel SessionFilePanel (impl Panel)
    transfer.rs              # cross-host copy via VfsManager relay
    settings.rs              # TerminalModeSettings
    tests/
      session_tests.rs
      active_tracking_tests.rs
      transfer_tests.rs
```

Dependencies (workspace deps only; add to root `Cargo.toml` `[workspace.dependencies]`
if missing, then reference with `workspace = true`): `gpui`, `workspace`,
`project`, `settings`, `terminal`, `terminal_view`, `remote`,
`remote_connection`, `recent_projects`, `fs`, `vfs`, `rpc`, `worktree`, `ui`,
`util`, `i18n`, `anyhow`, `serde`, `futures`, `collections`, `theme`.

Wire the crate into `crates/zzz/Cargo.toml` and call its `init` from
`crates/zzz/src/zzz.rs` (see 4.7).

---

## 4. Module-by-module implementation

### 4.1 `session.rs` — session authority

```rust
pub struct SshSession {
    pub id: SessionId,                                  // small newtype over usize
    pub connection_options: RemoteConnectionOptions,
    pub client: Entity<RemoteClient>,
    pub project: Entity<Project>,
    pub home_worktree: Option<Entity<Worktree>>,        // rooted at remote $HOME
    pub file_panel_view: Option<Entity<ProjectPanel>>,  // this session's tree+preview panel body
    pub status: SessionStatus,                          // Connecting | Connected | Reconnecting | Failed
}

pub struct SessionStore {
    workspace: WeakEntity<Workspace>,
    sessions: Vec<SshSession>,
    active_session: Option<SessionId>,
    transfer_manager: VfsManager,                       // R6: self-built, see 4.6
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Event> for SessionStore {}
pub enum Event { SessionActivated(SessionId), SessionsChanged }
```

Key methods:

- `SessionStore::connect(store, connection_options, window, cx) -> Task<Result<SessionId>>`
  1. `remote::connect(options, delegate, cx).await` (`remote_client.rs:417`) —
     reuses `ConnectionPool` for identical options.
  2. `RemoteClient::new(ConnectionIdentifier::Setup(next_id), remote, cancel_rx, delegate, cx).await`
     (`remote_client.rs:446`). Reuse `remote_connection::RemoteClientDelegate`
     (`remote_connection.rs:538`, askpass UI at `:558`) so password prompts
     work.
  3. `Project::remote(client, app_state.client, app_state.node_runtime,
     app_state.user_store, app_state.languages, app_state.fs, true, cx)`
     (`project.rs:1457`). This is the "build project only, no workspace" step
     factored out of `open_remote_project_with_new_connection`
     (`workspace.rs:9302-9357`).
  4. Resolve remote home and mount a worktree:
     run `printf %s "$HOME"` over the authenticated connection (expose the
     private `SshSocket::ssh_command`/`run_command` at
     `transport/ssh.rs:1273-1327`, or add a small RPC), then
     `project.find_or_create_worktree(home_path, true, cx)`
     (`project.rs:4190`).
  5. Build the session's `ProjectPanel` body (see 4.4) and store it.
- `SessionStore::open_terminal(session, window, cx)`:
  `session.project.update(cx, |p, cx| p.create_terminal_shell(None, cx))`
  (`terminals.rs:307`; `cwd = None` makes a remote terminal land in remote
  `$HOME` — R5), then `TerminalView::new(terminal, workspace.weak_handle(),
  workspace.database_id(), session.project.downgrade(), window, cx)`
  (`terminal_view.rs:232`) and `workspace.add_item_to_active_pane(...)`
  (`workspace.rs:4546`). This mirrors `add_center_terminal`
  (`terminal_panel.rs:752`) but takes the session project explicitly.
- `SessionStore::set_active_session(id, cx)`: set field, `cx.emit(Event::SessionActivated(id))`.
- `SessionStore::resolve_active_session(cx)`: from
  `workspace.focused_pane()` / `workspace.active_item(cx)`
  (`workspace.rs:5731`), `downcast::<TerminalView>()`, read
  `terminal_view.read(cx).project` (`terminal_view.rs:873`) and match to a
  session.
- `SessionStore::disconnect(session, cx)`: drop client/project, close its
  panels/terminals; async work via `cx.spawn` and `detach_and_log_err`.
- `SessionStore::serialize/deserialize`: KVP key `"TerminalMode"` in
  `WorkspaceDb` (pattern: `terminal_panel.rs:43,949`). Persist connection
  options + titles; restore reconnects (see 6.3).

### 4.2 `session_panel.rs` — left panel (R1)

```rust
pub struct SshSessionPanel {
    store: WeakEntity<SessionStore>,
    focus_handle: FocusHandle,
    // list selection / scroll state
}
impl Render for SshSessionPanel { ... }
impl Panel for SshSessionPanel {
    fn position(...) -> DockPosition { DockPosition::Left }
    fn panel_key() -> &'static str { "SshSessionPanel" }
    // + toggle_action, default_size, activation_priority, persistent_name,
    //   position_is_valid  (model on project_panel.rs:8084-8145)
}
```

- Data sources: `RemoteSettings::ssh_connections()`
  (`recent_projects/src/remote_connections.rs:40`, type `SshConnection` at
  `settings_content.rs:1315`, default at `assets/settings/default.json:2668`)
  and `parse_ssh_config_hosts` (`recent_projects/src/ssh_config.rs:17`).
  Watch `~/.ssh/config` for changes (pattern: `remote_servers.rs:3567`).
- Interaction: click an entry -> `SessionStore::connect(...)` (auth via the
  existing askpass modal); show status dot and active highlight; right-click
  menu: disconnect / reconnect; a "New Connection…" item opens the existing
  `RemoteConnectionModal` (`remote_connection.rs:23`).
- Empty and error states must use `i18n::tr` keys (see section 5).

### 4.3 Center terminals (R2) + cleanup of hardcoded `workspace.project()`

Add (in `terminal_mode`) a spawn helper that mirrors `add_center_terminal`
(`terminal_panel.rs:752`) but binds the session project:

```rust
pub fn spawn_session_terminal(
    workspace: &mut Workspace,
    session: &SshSession,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<Result<WeakEntity<Terminal>>>;
```

Multiple sessions become multiple center tabs automatically (each
`TerminalView` is one tab). Sessions can share a pane or use separate panes.

**Mandatory cleanup so foreign-project terminals do not resolve against the
wrong project:**

1. `default_working_directory` (`terminal_view.rs:2134-2162`) reads
   `workspace.project()`. Add a variant that accepts an explicit project, or
   have callers (`TerminalView::deploy` `:221`, `new_pane_with_active_terminal`
   `:477`) pass the session project.
2. Terminal path clicks (`terminal_path_like_target.rs:147-197`) resolve via
   `workspace.project()`; route through the owning `TerminalView.project`.
3. `TerminalPanel` spawn entry points that hardcode `workspace.project()`
   (`terminal_panel.rs:92,275,420,457,767,777,804,814,884,903,1200,1406`) keep
   their current behavior for non-terminal-mode use; terminal-mode spawning
   uses 4.3's explicit-project path instead.

### 4.4 Right panel (R4/R5)

`SessionFilePanel` renders a vertical split inside the dock (reuse the
"dock panel owns its own PaneGroup" pattern from `terminal_panel.rs:79,1399`):

1. **Top: `ProjectPanel` (tree)** for the active session's `$HOME` worktree.
2. **Bottom: preview `Pane`** created with `Pane::new(workspace,
   session.project, ...)` (`pane.rs:569`); files clicked in the tree open here
   (their buffers are created from `session.project`, so save/reveal act on the
   correct project; see risk 6.2).

Required refactors (strictly limited):

- Parameterize `ProjectPanel::new` to accept `project: Entity<Project>`
  (`project_panel.rs:643` is the only place `workspace.project()` is read);
  update `ProjectPanel::load` (`:1020`) and the `zzz.rs:718` caller to pass
  `workspace.project()` so default behavior is unchanged.
- Add a configurable "open target" to `ProjectPanel` (default: current center
  behavior). `SessionFilePanel` injects its preview `Pane` weak handle so the
  tree opens into the preview instead of the workspace center.

Follow-active: `SessionFilePanel` subscribes to `SessionStore` and re-renders
when `Event::SessionActivated` arrives.

### 4.5 Active-terminal tracking (cross-cutting)

- `SessionStore` subscribes to all center panes'
  `pane::Event::{ActivateItem, Focus}` (`pane.rs:348-378`; subscription style
  as in `terminal_panel.rs:1270`) and to `workspace::Event::ActiveItemChanged`
  (`workspace.rs:1280`, emitted `:5846`; it covers center panes, which is where
  terminal-mode terminals live).
- On change: if the active item `downcast::<TerminalView>()` succeeds, resolve
  its project -> session, call `set_active_session`, emit
  `Event::SessionActivated`, and the right panel refreshes.
- Re-subscribe when panes are added/removed.

### 4.6 Cross-host copy (R6)

- `SessionStore` owns its own `VfsManager` (`Default`, `manager.rs:8`).
- Registration (lazy per session): obtain the session worktree's
  `remote_vfs_provider(cx)` (`worktree.rs:962`, a `RemoteProviderProxy`), wrap
  with `VfsSnapshot::new(MountId::new(assigned_id), provider, budgets)`
  (`snapshot.rs:503`), then `manager.register(snapshot)` (`manager.rs:24`).
  **Assign our own mount ids** to avoid collisions: each remote server starts
  its mount ids at 1 (`snapshot.rs:485`), so two hosts can both present
  `MountId(1)` and `register` would fail `AlreadyExists` (`manager.rs:27`).
- Copy: `manager.copy_path(src_vfs_path, dst_vfs_path, VfsTransferOptions)`
  (`transfer.rs:230`) -> `transfer_cross_provider` (`transfer.rs:406`). Use
  `move_path` (`:240`) for move. The engine does the local relay, 256 KiB
  chunks (`:17`), SHA-256 verify, staging + atomic rename.
- Path construction: build `VfsPath`s from each session's worktree-relative
  path plus our assigned mount id (`VfsPath::new` `vfs.rs:443`). Path encoding
  must match on both ends (`transfer.rs:418-431`); Linux<->Linux works,
  Linux<->Windows errors cleanly (`Unsupported`).
- UI: on a selected tree entry, a "Copy to session…" action opens a session
  picker, then runs the transfer with a progress toast
  (`workspace.show_toast`). From the user's view it is a single copy command.
- Limitation to document: because the worktree is rooted at `$HOME`, transfers
  are within `$HOME`. Whole-filesystem access would need an additional `/`
  worktree (extension, not v1).

### 4.7 Mode toggle, bottom dock, settings, wiring

- New action `ToggleTerminalMode` defined in the `terminal_mode` crate
  (`actions!`), registered in `terminal_mode::init`.
- Entering: `workspace.open_panel` for left/right, `workspace.close_panel::<TerminalPanel>`
  (`workspace.rs:4444`) to close the bottom dock (R3), persist the flag.
  Exiting restores dock state. Model persistence on `centered_layout`
  (`workspace.rs:1395`, toggle at `:7272-7283`, action at `:325`, registration
  at `:7033`).
- `terminal_mode::init(cx)`:
  - `cx.observe_new(|workspace, window, cx| { register ToggleTerminalMode;
    lazily create a `SessionStore` entity; store a weak handle where both
    panels can reach it; })`;
  - call from `crates/zzz/src/zzz.rs:5682` next to `terminal_view::init(cx)`.
- Panel registration: in `initialize_panels` (`zzz.rs:716-752`) add
  `SshSessionPanel::load(...)` and `SessionFilePanel::load(...)` mirroring
  `TerminalPanel::load` (`terminal_panel.rs:247`), both sharing the workspace's
  `SessionStore` weak handle.
- `TerminalModeSettings` (`settings.rs`): left/right default widths,
  `preview_enabled`, `auto_connect_last_session`, `file_tree_root`
  (`"$HOME"` default). Add to `assets/settings/default.json` with a schema
  entry modeled on `project_panel_settings`.

---

## 5. Code conventions (must follow)

From `AGENTS.md`, `.rules`, and `clippy.toml`:

- **i18n:** every user-facing string uses `i18n::tr(cx, key, fallback)`. Add
  each key to **both** `assets/locales/en.json` and
  `assets/locales/zh-CN.json`; the two catalogs must keep identical key sets.
  The in-code `fallback` must be byte-identical to the `en.json` value.
- **Errors:** no `unwrap()`, no `let _ =` on fallible ops. Propagate with `?`,
  or use `.log_err()`/`detach_and_log_err` when intentionally ignoring. Surface
  connection/transfer errors to the UI (toast/notification), never swallow.
- **GPUI:** inside `Entity::update` closures use the inner `cx`; never re-enter
  an update of the same entity. `cx.spawn`/`cx.background_spawn` tasks must be
  awaited, detached, or stored — dropping cancels them.
- **Tests:** use GPUI executor timers (`cx.background_executor().timer(...)`),
  not `smol::Timer::after(...)`.
- **Processes:** no `std::process::Command`; use `smol::process::Command`.
- **Files/naming:** no `mod.rs`; full-word variable names; prefer extending
  existing files over creating many small files.
- **Build/lint:** run `./script/clippy` (not `cargo clippy`).
- **Commits (only if asked):** `git commit -s` (DCO).

---

## 6. Risks and mitigations

1. **No active-terminal event exists.** `handle_pane_event` only serializes on
   `ActivateItem`/`Focus` (`terminal_panel.rs:351,434`). Mitigation: pane-event
   subscription + new `SessionStore` event (4.5).
2. **Opening/saving files against the wrong project.** `Workspace::open_paths`,
   `Pane::save_item` (`pane.rs:2006`) and reveal use the pane's project. The
   session `ProjectPanel` must route opens to the session preview `Pane`
   (created with `session.project`), never `Workspace::open_paths`.
3. **Session restore across restarts.** `ConnectionIdentifier::Setup` is not
   stable. Persist connection options + titles in KVP and reconnect on load
   (`auto_connect_last_session`). Exact terminal buffer/scroll state is not
   guaranteed (competitors behave the same).
4. **Cross-host mount-id collisions and path encoding.** Self-built
   `VfsManager` + assigned ids (4.6); Linux<->Linux OK, mixed encodings error
   cleanly.
5. **File tree does not follow the terminal's cwd.** Out of scope in v1
   (decision above).
6. **`ProjectPanel` as a child view.** Verify focus, context menus and reveal
   after parameterization (it holds `WeakEntity<Workspace>`,
   `project_panel.rs:159`).

---

## 7. Execution phases

Do one phase per "继续". After each phase, run the verification commands,
report status, and stop.

| Phase | Deliverable |
|---|---|
| **P0 Skeleton** | Create `crates/terminal_mode`; add to workspace + `crates/zzz` deps; `terminal_mode::init` called at `zzz.rs:5682`; panel stubs registered in `initialize_panels`; `TerminalModeSettings` in `default.json` + schema; compiles. |
| **P1 Session layer** | Factor "build project only" out of `workspace.rs:9302`; implement `SessionStore::connect` (RemoteClient + Project + home worktree) reusing `RemoteClientDelegate`; expose remote `$HOME` probe; unit tests for connect bookkeeping. |
| **P2 Single-connection end to end** | Left panel list + connect; center session terminal (home cwd); parameterize `ProjectPanel::new`; right panel tree + preview Pane; `ToggleTerminalMode` + close bottom dock; active tracking; clean up `default_working_directory` and `terminal_path_like_target` bindings. |
| **P3 Multi-session concurrency** | N sessions / multiple tabs; right panel follows active session; disconnect/reconnect; TerminalPanel spawn paths left untouched for non-terminal use. |
| **P4 Cross-host copy** | Self-built `VfsManager` + snapshot registration with assigned ids; `copy_path` relay; "Copy to session…" action + progress toast. |
| **P5 Persistence and polish** | KVP session save/restore; empty/error states; full i18n (both catalogs, identical keys, fallback == en); `./script/clippy` clean; `cargo test -p terminal_mode` green. |

Estimated total: ~6 weeks (1 engineer).

---

## 8. Verification commands

```sh
cargo check -p terminal_mode
cargo check -p zzz
cargo test -p terminal_mode
./script/clippy
# docs only if docs change:
# cd docs && npx prettier --check src/
```

Acceptance (end of P5):

- Left panel lists saved and `~/.ssh/config` hosts; clicking connects.
- Two hosts' terminals run in one window simultaneously.
- Right panel shows the active terminal's session `$HOME` tree; switching the
  active terminal switches the tree; the lower half previews opened files.
- Entering terminal mode closes the bottom dock; exiting restores it.
- A file can be copied from host A to host B with one action (local relay).
- Sessions restore after restart (auto-connect per setting).
- `./script/clippy` and `cargo test -p terminal_mode` pass.
