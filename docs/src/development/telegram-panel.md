---
title: Telegram Panel
description: How the opt-in Telegram panel is structured, synchronizes, and renders.
---

# Telegram Panel

ZZZ has an opt-in Telegram panel that docks left, bottom, or right, next to the
Agent and Git panels. It is split across two crates: a GUI-independent engine
and a GPUI panel, mirroring the `git` plus `git_ui` split.

Nothing connects at application start. The engine connects only after you open
the panel or press Sign in, and it sends no telemetry. See
[Privacy and Network Boundary](./privacy-boundary.md).

## Crate layout {#crates}

| Crate         | Responsibility                  | Depends on                                     |
| ------------- | ------------------------------- | ---------------------------------------------- |
| `telegram`    | Engine, session, settings model | `grammers`, `tokio`, `serde`, `anyhow`, crypto |
| `telegram_ui` | Panel, actions, settings, i18n  | `gpui`, `ui`, `workspace`, `settings`, `i18n`  |

`telegram_ui` registers the engine global and the panel actions in
`crates/telegram_ui/src/telegram_ui.rs`, which the app calls from
`crates/zzz/src/main.rs`. The panel is added in `initialize_panels` in
`crates/zzz/src/zzz.rs`. The engine is an app-global entity; the panel is a
per-workspace entity that reads it.

The UI treatments follow the Agent panel (`crates/agent_ui`) and reuse the
components and theme tokens in `crates/ui`. Telegram Desktop (`tdesktop`),
`64gram`, `kotatogram`, and `telegram-tt` are interaction and layout
references only. No code is copied from them and none is a dependency.

## Engine and synchronization {#engine}

The engine runs on a dedicated multi-thread Tokio runtime on its own OS thread.
It owns one grammers client and a long-lived update stream. The service surface
is small:

- `send(command)` on an `mpsc` channel to the engine thread.
- `subscribe() -> watch::Receiver<Arc<ViewModel>>` for immutable snapshots.

The panel spawns one foreground task that awaits `watch.changed()` and
rerenders. All networking, file, and media work happens on the engine thread;
the panel only sends commands and renders snapshots.

### Lazy start {#startup}

`EngineHandle::spawn` does not connect. The first `Command::Start`, sent when
the panel becomes active or you press Sign in, connects and restores a saved
session. If no session exists, the engine reports the sign-in state.

### Cache and incremental history {#cache}

History is stored oldest-first. The engine keeps a per-chat `CachedHistory`
keyed by chat id, so selecting a chat renders the cached messages immediately,
then refreshes the tail in the background. Scrolling to the top of the
conversation prepends the next older page automatically; the header's
load-older button is a fallback.
Two monotonic counters guard concurrency:

- `history_epoch` is bumped whenever the displayed chat changes. A page load
  that finishes after the user switched chats is dropped instead of corrupting
  the new history.
- `connection_epoch` is bumped on each connection attempt. A stream that
  disconnects after a newer connection has started is ignored.

`fetch_history_page` normalizes each fetched page to oldest-first, because
grammers' `iter_messages` yields newest-first. `merge_tail` merges a refreshed
newest page into the history, keeping older loaded messages and optimistic
(id `0`) sends, preferring the page for duplicate ids, and sorting the result
oldest-first with pending sends last. `load_gap` backfills messages between two
known ids when an incoming update jumps ahead, so a busy chat does not leave
holes.

### Debounce and reconnect {#reconnect}

New messages update the chat preview and unread count immediately. If a
message arrives for a chat that is not in the loaded list, the engine schedules
a single chat-list reload after 500 ms instead of refetching on every message.
A dropped connection clears the client and session, then schedules a
reconnection with exponential backoff, capped at 30 seconds. A first connection
failure schedules a retry too, even before sign-in has completed.

### Read state {#read}

The panel reports whether the selected chat is focused and scrolled to the
bottom with `Command::SetViewing`. A chat is marked read only while all three
hold: the chat is selected, the panel has window focus, and the history follows
the tail. Opening a chat therefore does not mark it read while you are reading
older messages. `Command::MarkUnread` sets the unread flag explicitly, and
messages that arrive while the panel is unfocused keep their badge.

Read updates from other devices arrive as raw `updateReadHistoryInbox`,
`updateReadChannelInbox`, `updateReadHistoryOutbox`, and
`updateReadChannelOutbox` updates. The engine maps their peer to the chat id and
syncs `unread_count` and `read_outbox_message_id`, ignoring peers that are not
in the loaded list. The conversation header shows the outbox read position for
the account's own messages.

### Conversations {#conversations}

The panel supports the common message actions without leaving the keyboard:

- **Replies.** The context menu's Reply action shows a banner above the
  composer. Sending attaches the reply to the first message of a multi-message
  send. Reply previews jump to the referenced message when it is loaded.
- **Editing.** Editing one of the account's own text messages loads it into the
  composer, shows an editing banner, and sends `Command::EditMessage` on submit.
  Cancelling restores the text that was in the composer before editing started.
- **Selection.** Entering selection mode adds checkboxes to bubbles. The toolbar
  can forward or delete the selected messages in one request.
- **Reactions.** The context menu offers a small emoji palette and removes an
  existing reaction. Reactions render under the bubble and update from
  `updateMessageReactions`.
- **Forwarding.** Messages forward to any loaded chat. Forward Without Author
  uses the raw `messages.forwardMessages` request with `drop_author` set.
- **Deletion.** Delete for Me and Delete for Everyone map to the `revoke`
  parameter. Channels have no per-user deletion, so Delete for Me removes the
  message locally.
- **Pinning.** A pinned message bar under the conversation header scrolls to the
  pinned message. Pinning a chat moves it to the top of its folder.
- **Service messages.** Joins, leaves, title and photo changes, pinned-message
  notices, and calls render as centered one-line notices.

### Folders and the archive {#folders}

The chat list keeps the full dialog set and derives the visible list from the
selected `FolderSelection`. Custom folders come from `messages.getDialogFilters`;
their membership comes from each dialog's `folder_id`. Telegram exposes a single
`folder_id` per dialog, so a chat appears in the folder the server reports.
Archive is the reserved folder with id `1` and is fetched separately with
`messages.getDialogs`, because the high-level dialog iterator only walks the
default folder. Switching folders never refetches, and the unread badge spans
every folder.

### Typing and presence {#typing}

Incoming `updateUserTyping`, `updateChatUserTyping`, and
`updateChannelUserTyping` updates update a per-chat typing snapshot, which
expires after six seconds without a refresh. The chat row and conversation
header show "is typing" and then fall back to presence: online, recently online,
or last seen. Outgoing typing actions are sent at most once every four seconds
while the composer is non-empty. Sender names for typing updates come from
messages already loaded in the conversation.

## Content width {#width}

Content is centered when the panel is wider than the configured maximum,
following the Agent panel's constrained content column.

`render_centered` wraps the chat header, the composer, and every message in a
full-width row that centers a column constrained to `max_content_width`. The
`telegram_panel.limit_content_width` setting (default `true`) controls whether
the constraint applies at all, and `telegram_panel.max_content_width` (default
`760`) sets the width in pixels. When the limit is off, content fills the panel.
Messages no longer hard-code a fixed relative maximum width.

## Media {#media}

Attachments render as collapsed cards that mirror the Agent panel's tool-call
cards. The header shows an icon, the name, and metadata (duration, dimensions,
size, MIME type), with a `Disclosure` chevron. The body is built only when the
card is expanded.

- Photos and stickers show a thumbnail first. Expanding the card requests a
  small preview through `Command::DownloadThumbnail`: the engine prefers an
  embedded thumbnail, otherwise it requests the smallest server-side size, and
  caches the bytes under the Telegram thumbnail cache. Only the Download button
  fetches the original.
- Voice messages play through the existing `audio` crate once downloaded.
- Video, audio, document, photo, and sticker cards expose a Download button;
  once downloaded, the button becomes Open and hands the file to the system
  player. Clicking a video poster does the same as its button: it starts the
  download, then opens the file. Contacts, polls, geolocations, and link
  previews have no file content and do not offer a Download.
- Polls render their question, options, voter counts, and closed or
  multiple-choice state. Contacts show the name and phone number. Locations and
  venues show a title, address, coordinates, and live-location state.
  Animations and round video messages get their own media kinds.
- Albums are sent as one `messages.sendMedia` batch when the composer contains
  several consecutive attachments. Received albums keep their `grouped_id`, but
  each message still renders as its own card.
- Link previews render as a compact card that opens the URL.
- Media-only messages show a localized attachment name in the chat list
  preview instead of a blank line.

Downloads run on spawned engine tasks, so a large download does not block
sending a message or receiving incoming updates.

### Storage and cache {#storage}

Persistent state and regenerable media live in separate roots:

```
<paths::telegram_dir()>/               persistent, never cleared with the cache
  session/
    session.bin                        0600
    master.key                         0600

<paths::telegram_cache_dir()>/         cache, the OS may delete it
  media/<chat_id>/<message_id>.<ext>
  thumbnails/<chat_id>/<message_id>.<ext>
```

`paths::telegram_dir()` is `data_dir()/telegram`; `paths::telegram_cache_dir()`
is `temp_dir()/telegram`, which maps to the platform cache directory. Media is
sharded by chat id, its extension is derived from the document name or MIME
type (with a kind fallback for photos and stickers), and a missing cache
directory means nothing is cached rather than an error. A cache miss
re-downloads transparently.

The media cache is bounded. Files older than 30 days are evicted first, then
the oldest files are removed until the cache is under 512 MiB. Logging out
clears the media cache but never the encrypted session. The cache semantics are
recorded here because the cache is disposable; the session is not.

## Composer and keymap contexts {#composer}

The composer is an `editor::Editor` in auto-height mode, the same control the
Git panel uses. Its initial height comes from
`telegram_panel.composer_min_lines` (default `4`), and it grows from there before
scrolling. The panel keymap binds send and newline with a descendant
context:

```json [keymap]
{
  "context": "TelegramComposer > Editor && mode == auto_height",
  "bindings": {
    "enter": "telegram_panel::Send",
    "shift-enter": "editor::Newline"
  }
}
```

The distinction between `&&` and `>` matters here:

- `&&` combines predicates that must all hold on the same element's key
  context. `TelegramPanel && TelegramComposer` never matches, because no single
  element carries both contexts: the panel has `TelegramPanel` and the editor
  has `Editor`.
- `>` requires the right-hand context to be an ancestor of the focused one. The
  editor is wrapped in a container with the `TelegramComposer` key context, so
  `TelegramComposer > Editor && mode == auto_height` matches the focused
  composer and takes priority over the editor's generic `Enter` binding.

The panel-level context binds `escape` to back, `ctrl-enter` to send,
`alt-up`/`alt-down` to chat navigation, and `ctrl-r` to refresh. Panel
keybindings avoid `cmd-`, `super-`, `win-`, and `fn-`, which
`cargo xtask check keys` rejects.

There is no composer hint label; the placeholder is `Message`, localized through
`telegram_panel.composer.placeholder`.

### Search {#search}

The header search opens a server-side message search after a short debounce.
Queries shorter than two characters only filter loaded chat titles. The filter
chooser selects all messages, text, media, links, or files, mapped to
`messages.Filter` variants. Results are grouped by chat and paginate with
`Command::SearchMore`; clicking a hit opens the chat and loads the page that
contains the message before scrolling to it. Clearing the query leaves search
mode without discarding the loaded history.

### Paste behavior {#composer-paste}

The composer intercepts `Paste` before the editor's default handler:

- A single selection copied from a ZZZ editor carries its file and line range as
  clipboard metadata. Pasting it stages a code reference chip that links to the
  source instead of inserting the raw text.
- An image on the clipboard is written under the Telegram cache and staged as an
  attachment. External file paths on the clipboard are staged as attachments
  too, but only when the project is local.
- Anything else pastes as plain text.

`Paste as Plain Text`, `Cut`, `Copy`, and `Insert Code Reference from
Selection` are available from the composer's context menu. Code blocks in
received and sending messages are syntax-highlighted using the project's
language registry.

## Proxy {#proxy}

Telegram supports SOCKS5 only. The engine reads a dedicated
`telegram.proxy` setting first, then falls back to ZZZ's global proxy when it
is a `socks5://` URL. HTTP and HTTPS proxies are not usable by grammers; the
engine logs a warning and connects directly. There is no HTTP-to-SOCKS bridge.

## Settings {#settings}

The panel registers `telegram_panel` (button, dock, default width and height,
flexible sizing, starts open, unread badge, content-width options, and composer
minimum lines) and `telegram` (SOCKS5 proxy). Panel size, position, and
visibility are otherwise managed by the workspace dock. The unread badge counts
unread messages across every folder and excludes muted chats.

User-facing strings are localized through `i18n::tr` under the
`telegram_panel.*` and `workspace.dock.panel.telegram` keys, present in both
`assets/locales/en.json` and `assets/locales/zh-CN.json`.

## Not implemented {#not-implemented}

- Received albums keep their `grouped_id` but are not visually merged into a
  grid; each message renders as its own media card.
- Chat avatars use initials. Profile photos are not downloaded.
- Bot inline keyboards, callback buttons, and inline bots are not rendered.
- Forum topics and message threads are not modeled.
- There is no in-panel video playback. Videos are cards that open in the system
  player.
- Outgoing voice notes are not implemented.
- Multi-account, calls, and per-message read receipts are out of scope.
- Received TGS sticker animation and message effects are not rendered.
- Offline chat history and drafts are not persisted across restarts. Only the
  encrypted session survives; message history is refetched after reconnecting.
- Muted chats do not raise notifications and do not count toward the unread
  badge. They still count toward the per-chat unread count.

## Verification {#verification}

- `cargo check --workspace` and `cargo test --workspace`.
- `cargo xtask clippy`.
- `cargo xtask check keys`, `cargo xtask check philosophy`, `cargo xtask check licenses`.
- `cd docs && npx prettier --check src/`.

Real sign-in and message delivery need a real Telegram account; the automated
checks cover compilation, unit tests, lint, keymaps, licenses, and the privacy
boundary.
