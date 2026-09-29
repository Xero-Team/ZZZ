---
title: Telegram Panel Hardening Plan
description: Plan for completing and hardening the Telegram panel.
---

# Telegram Panel Hardening Plan

This is an internal work plan, not user documentation. It records the
defects found while reviewing the uncommitted Telegram panel
(`crates/telegram`, `crates/telegram_ui`) and the agreed design for
finishing it. Implement the workstreams in order. Keep the diff scoped:
do not bundle unrelated dependency changes.

The existing feature overview lives in
[Telegram Panel](./telegram-panel.md). The privacy contract lives in
[Privacy and Network Boundary](./privacy-boundary.md).

## Locked decisions {#decisions}

These are settled. Do not relitigate them while implementing.

1. **Media is a cache.** Downloaded media and thumbnails may be deleted
   by the OS at any time and must be transparently re-downloaded. They
   live in the platform cache directory, never in the data directory.
2. **Credentials are state.** The encrypted session and master key are
   persistent and must not be cleared when the cache is cleared.
3. **Paths live in `crates/paths`.** No path is constructed by joining
   strings inside `telegram_ui` or `telegram`. Add accessors and pass
   them in.
4. **The engine returns errors, the UI localizes them.** The engine must
   not build user-facing English strings. Surface structured error codes
   and let the panel render `i18n::tr` text.
5. **Reuse the design system.** Prefer `crates/ui`, `crates/util`,
   `crates/time_format`, and `crates/audio_viewer` helpers over new
   bespoke widgets, formatters, and tokens.
6. **Keep the two-crate split.** `telegram` stays GUI-independent;
   `telegram_ui` stays a GPUI panel. Do not add non-workspace
   dependencies.

## Storage layout {#storage}

Add two accessors to `crates/paths`:

```
paths::telegram_dir()        -> data_dir()/telegram
paths::telegram_cache_dir()  -> temp_dir()/telegram
```

Note: `paths::temp_dir()` is the platform cache directory despite its
name (it maps to `dirs::cache_dir()` on every OS). It is already used as
the cache location by `crates/fs` and `crates/keymap_editor`.

```
<telegram_dir>/                      persistent, 0700
  session/
    session.bin                      0600
    master.key                       0600

<telegram_cache_dir>/                cache, safe to delete, 0700
  media/<chat_id>/<message_id>.<ext>
  thumbnails/<chat_id>/<message_id>.<ext>
```

Rules:

- Create `<telegram_dir>/session` once at engine start. Create the cache
  subdirectories lazily before the first write, and treat a missing
  cache directory as "nothing cached" rather than an error.
- Derive the file extension from the MIME type (or the document file
  name) instead of defaulting to `.bin`. Photos and stickers have no
  `file_name` today, so they currently land as `.bin` and cannot be
  opened.
- Shard by `chat_id` so a single directory does not accumulate millions
  of entries, and so per-chat cleanup is possible.
- Add an eviction policy (size cap and/or age) and clear the media cache
  on logout. Record the retention in the feature docs.
- Keep `EngineConfig` injectable: pass `session_dir` and `cache_dir` as
  fields so tests can use temp directories.

### Why not under the database directory {#why-not-db}

Colocating with `paths::database_dir()` (`data_dir()/db`) was
considered and rejected:

- The database stores scoped SQLite files (`db/0-<channel>/db.sqlite`)
  and its recovery path moves or recreates those scope directories.
  Large, regenerable media must not be entangled with that.
- Media is chat content, but it is also a cache. XDG semantics put
  regenerable data in the cache directory and persistent state in the
  data directory.
- Deleting the media cache must never risk the session. Keeping them in
  separate roots makes that guarantee structural.

If a single Telegram root is later required, keep `session/` and the
cache in separate subdirectories and never let cache cleanup touch
`session/`.

## Workstreams {#workstreams}

### WS1: Unblock the engine command loop (P0) {#ws1}

`run_engine` awaits `handle_command` inline (`engine.rs:208-222`), so a
long download, a search, or `load_chats` blocks new commands and, worse,
blocks incoming-update events. A user downloading a file cannot send a
message or receive new ones until it finishes.

- Move downloads, search, and history/chat loading onto `tokio::spawn`
  tasks that report back through `EngineEvent`.
- Keep the select loop free of long awaits.
- Guard results with the existing epochs so stale tasks are dropped.

Acceptance: while a large media download runs, sending a message and
receiving an incoming message both work immediately.

### WS2: Media pipeline (P0) {#ws2}

- Create the cache directories; today nothing ever creates them, so
  every attachment download fails (`engine.rs:291-292`, `:1416`).
- Write media and thumbnails under `telegram_cache_dir` (see
  [Storage layout](#storage)).
- Derive extensions from MIME; fix photo/sticker `.bin` paths.
- On a cache miss, re-download. On a cache hit, reuse.
- Only show a Download affordance for kinds that are actually
  downloadable (`MediaSnapshot::is_downloadable`); contacts, polls, and
  geolocations must not offer a broken Download.
- Show media names in the chat list preview for media-only messages
  (`display_name`) instead of an empty line.

Acceptance: a photo downloads, opens in the system viewer, and after the
cache is deleted downloads again.

### WS3: Markdown rendering (P0) {#ws3}

`Markdown::new_text` parses links only (`markdown.rs:670-680`,
`parser.rs:712`), so all formatting renders literally. The cache is also
never updated when a message is edited (`telegram_panel.rs:334-360`).

- Construct with `Markdown::new(source, None, None, cx)`.
- Key the cache by `(chat_id, message_id)` and replace the entity when
  the source changes.
- Represent underline without raw HTML (`<u>` is rendered as literal
  text by the markdown renderer).
- When entities cross or cannot be represented, return `None` and fall
  back to plain text instead of emitting invalid CommonMark
  (`telegram/src/markdown.rs:114-145`).
- Escape backticks in code and pre entities.

Acceptance: bold, italic, code, pre, and links render; editing a message
updates the rendered body.

### WS4: Wire up search (P0) {#ws4}

Nothing sends `Command::Search`; the search editor has no subscription,
so the search UI is dead (`telegram_panel.rs:176`, `engine.rs:387`).

- Subscribe to the search editor, debounce, and send `Command::Search`.
- Render a searching indicator from `view_model.searching`.
- Clicking a hit opens the chat and scrolls to that message.
- Prefer the Agent panel's top search bar layout over the current
  misplaced sidebar (`thread_search_bar.rs`). Do not recreate the
  results `ListState` on every render.

Acceptance: typing filters results and clicking one jumps to the
message.

### WS5: Per-chat drafts (P0) {#ws5}

The engine tracks `view.draft` but the UI never reads it, and the
composer is not reset on chat switch, so drafts leak between chats
(`engine.rs:861`, `telegram_panel.rs:392-398`).

- On chat switch, load the stored draft into the composer.
- On send, clear both the composer and the stored draft.

Acceptance: switching chats shows the correct draft and never carries
text across chats.

### WS6: Errors and i18n (P1) {#ws6}

The engine builds user-facing English strings (`engine.rs:514,681,707`)
and the panel shows them verbatim (`telegram_panel.rs:1176-1211`).

- Emit structured errors (codes plus parameters) from the engine.
- Render them in the panel via `i18n::tr`; add every key to both
  `assets/locales/en.json` and `assets/locales/zh-CN.json`.
- Handle `FLOOD_WAIT` with a countdown using the existing
  `flood_wait_seconds` field.
- Localize the new-message toast action label (`telegram_panel.rs:290`),
  currently the only hardcoded UI string.
- Localize or move media `display_name` out of the engine.

Acceptance: no English engine string reaches the UI; fallbacks match
`en.json`.

### WS7: Design-system conformance (P1) {#ws7}

- Message bubbles: use surface tokens plus a border
  (`element_background`/`editor_background`), not `element_selected` /
  `element_hover`, and match the Agent panel's message card radius
  (`thread_view.rs:4828-4859`).
- Chat rows: use `ui::ListItem` (the forward picker already does) so
  hover/selected tokens, cursor, and rem spacing match the rest of the
  app.
- Replace fixed px with rem/DynamicSpacing so the panel scales with
  `ui_font_size`.
- Reuse `ui::CountBadge`, `ui::Avatar`/`AvatarStyle`, and `ui::Callout`
  instead of bespoke badge, avatar, and error banner widgets.
- Header: use `Tab::container_height`, `tab_bar_background`, and the
  standard border color like the Agent and Git panels.
- Use `Tooltip::for_action` so shortcuts appear in tooltips.
- Set the composer minimum to one line (currently four).
- Reuse `time_format` for timestamps, `util::format_file_size` for
  sizes, and the audio/video viewer duration formatter (which handles
  hours; the current one shows `60:00`).

Acceptance: visual parity with the Agent panel; no fixed-px layout.

### WS8: Notifications and mute (P1) {#ws8}

`ChatSnapshot::muted` is computed but ignored, so muted chats still
raise toasts and count toward the badge (`telegram_panel.rs:254-296`,
`engine.rs:841-848`).

- Skip toasts for muted chats.
- Decide and document whether muted chats count toward `unread_total`.

### WS9: Keymap conflict (P1) {#ws9}

`ctrl-shift-t` is bound twice in the `Workspace` context; the Telegram
binding silently overrides `pane::ReopenClosedItem` on Linux and Windows
(`default-linux.json:671,689`). Move the Telegram binding to a
non-conflicting chord. Consider extending `script/check-keymaps` to
detect duplicate bindings, since it cannot today.

### WS10: Settings and docs integration (P1) {#ws10}

- Add a `telegram_panel`/`telegram` section to the Settings Editor
  (`crates/settings_ui/src/page_data.rs`), matching the `git_panel`
  section.
- Document both settings in `docs/src/reference/all-settings.md`.
- Update `docs/src/development/telegram-panel.md` to reflect the final
  storage layout, the cache semantics, and the corrected limitations.

### WS11: Cleanup (P2) {#ws11}

- Remove dead state and commands: `status`, `credentials_present`,
  `searching`/`search_query` (unless WS4 uses them), `login_hint`,
  `flood_wait_seconds` (unless WS6 uses it), `is_group`/`is_channel`,
  `is_playable_video`, `site_name`, `grouped_id`/`reply_to_message_id`
  (unless implemented), `AuthState::SignedIn` fields, `expires_unix`,
  `Command::CloseChat`, `Command::MarkAllRead`, and the unbound
  `Toggle`/`SignIn`/`Logout`/`FocusComposer` actions.
- Prune `expanded_media` when history changes; it grows unbounded today.
- Give `Command` a redacting `Debug`; it currently derives `Debug` over
  `SubmitPassword`/`SubmitCode`.
- Reset the QR auth state when the QR flow errors instead of leaving a
  stale code on screen.
- Persist the session on disconnect and shutdown, not only on send and
  login.

### WS12: Tests and CI (P2) {#ws12}

- Engine unit tests: cache path construction, MIME extension, eviction,
  cache-miss refetch.
- GPUI tests for the panel: search wiring, draft switching, media card
  states.
- Run the verification commands below.

## Out of scope {#out-of-scope}

- Replies, albums, in-panel video playback, outgoing voice notes,
  multi-account, and per-message read receipts. Keep them listed as
  limitations.
- The rodio codec expansion (`symphonia-libopus` and the native
  `opusic-sys` build dependency) and the grammers DNS/root-certificate
  additions. These belong in a separate dependency review, not this fix.
  Do not expand them further here.

## Verification {#verification}

- `cargo check --workspace` and `cargo test --workspace`.
- `./script/clippy` (not `cargo clippy`).
- `script/check-keymaps`, `script/check-philosophy`,
  `script/check-licenses`.
- `cd docs && npx prettier --check src/`.

Real sign-in and delivery require a Telegram account. The automated
checks cover compilation, tests, lint, keymaps, licenses, and the
privacy boundary.

## Prompt for the implementation session {#prompt}

Use the following prompt in a fresh conversation:

```
Read docs/src/development/telegram-panel-hardening.md and
docs/src/development/telegram-panel.md, then fully implement the
hardening plan. Work through WS1-WS12 in priority order.

Constraints:
- Follow AGENTS.md: no unwrap on fallible ops, use i18n::tr with
  fallbacks identical to en.json, add keys to both en.json and
  zh-CN.json, do not add comments, run ./script/clippy (not
  cargo clippy), use GPUI timers (not smol::Timer::after) and
  smol::process (not std::process::Command).
- Honor the locked decisions in the plan: media is a system-cleanable
  cache under paths::temp_dir(); the session stays under
  paths::data_dir()/telegram/session; add paths accessors and stop
  hardcoding paths in telegram_ui.
- Reuse crates/ui, crates/util, and crates/time_format components.
- Keep the two-crate split and add no non-workspace dependencies.
- Treat the rodio/opusic and grammers DNS dependency changes as out of
  scope.
- For each workstream, implement it, add tests, and verify with the
  commands listed in the plan.
- Report a short summary per workstream with files changed and test
  results. Do not commit unless asked.
```
