---
title: Upstream Sync 2026-10-28 ACP Elicitation Entries
description: Re-absorption of the elicitation entry wiring that the 2026-09-22 port dropped.
---

# Upstream Sync 2026-10-28 ACP Elicitation Entries

## Scope

- Target: `main` at `149bd4c6d6`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Upstream head queried for this review: `2e794a1ea7`
- Query time: `2026-10-28T18:10:00+01:00`

This is not a forward batch. It re-opens the ACP elicitation commits that
are already inside the reviewed baseline, because the port from the
[2026-09-22 run](./upstream-sync-2026-09-22-acp-elicitation.md) left the
entry-slot wiring behind and the shipped build crashed at runtime:

```text
thread 'main' panicked at crates/agent_ui/src/entry_view_state.rs:262:25:
index out of bounds: the len is 3 but the index is 4
   3: <agent_ui::entry_view_state::EntryViewState>::sync_entry
   4: ... Entity<EntryViewState>, ..., ConversationView>::handle_thread_event::{closure#1}
```

## Root cause

`EntryViewState::entries` is positional with `AcpThread::entries`.
`sync_entry(index)`, `EntryUpdated(index)`, `EntriesRemoved(range)` and
the per-entry focus handles spliced into `ListState` all address slots by
index, and `set_entry` writes `self.entries[index]`.

The 2026-09-22 port (`3f297b3c05`) inlined the elicitation store, cards
and handlers, but left `AgentThreadEntry::Elicitation(_) => {}` in
`sync_entry` and never added `Entry::Elicitation`. An elicitation then
occupied an index in the thread but no slot in the view state, so the
following `AssistantMessage` arrived at `index == len + 1` and `set_entry`
indexed past the end. The published regression test
`test_hidden_elicitation_preserves_entry_index` from `fa66442a01` was
dropped along with the hunk, so nothing caught it.

The same port also dropped the live wiring from `conversation_view.rs`:
no `sync_elicitation_state_for_entry` call on `NewEntry`/`EntryUpdated`
(a pending form elicitation therefore rendered a card with no fields,
since `ElicitationCard::render_form` returns `Empty` without form state),
and no notification on `ElicitationRequested`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                              |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------- |
| fa66442a01 | B     | d5aa8b2838   | `Entry::Elicitation` slot, focus handle, and the entry-index regression test.                            |
| fbceb2823b | B     | d5aa8b2838   | Flag-free test parity plus live `sync_elicitation_state_for_entry` and the pending-request notification. |
| c05b439174 | A     | 3f297b3c05   | Re-checked: option descriptions only, no entry-index surface.                                            |
| 33e2058302 | A     | 3f297b3c05   | Re-checked: warnings and validation only, no entry-index surface.                                        |
| 68ec865bfd | A     | 3f297b3c05   | Re-checked: keyboard navigation inside the card only.                                                    |
| 8f92822cbf | A     | 3f297b3c05   | Re-checked: the `unicode_confusables` scanner only.                                                      |
| 4605a0f8dc | A     | --           | Panicking downcasts in `editor_for_diff`/`terminal` are already `and_then(..).ok()` locally.             |

Later elicitation work on `entry_view_state.rs` (`73be6ac6b1`,
`1c9dd3e70f`, and the ACP v2 sharing/refactor series through
`d93bf74085`) stays out: it is written against `schema::v2`, which ZZZ has
no usage of, and those SHAs are already in the rejection ledger.

## Applied work

### d5aa8b2838 — `sync: keep elicitation entries in the entry view state from fa66442a01`

`sync_entry` now materializes `Entry::Elicitation { focus_handle }` for
`AgentThreadEntry::Elicitation`, exactly as upstream does, and every
`Entry` match (`focus_handle`, `message_editor`,
`scroll_handle_for_assistant_message_chunk`, `has_content`,
`agent_ui_font_size_changed`, `Focusable`) accounts for it. The thread
view already rendered elicitation bodies from `ElicitationStore`; the new
slot restores the invariant that the view state has one entry per thread
entry, which also re-enables the focus handle the elicitation card tracks.

`ConversationView::handle_thread_event` calls
`sync_elicitation_state_for_entry` on `NewEntry` and `EntryUpdated`, so a
live elicitation request gets its `ElicitationFormState` before the card
renders, and `ElicitationRequested` raises the new localized
`agent_ui.conversation.waiting_for_input` notification.

The regression test reproduces the crash on the pre-fix tree
(`the len is 0 but the index is 1` at `entry_view_state.rs:258`) and
passes after the fix, asserting that an elicitation and the assistant
message after it land in slots 0 and 1.

## Verification

| Check                                    | Result  |
| ---------------------------------------- | ------- |
| `cargo test -p agent_ui --lib` (304)     | PASS    |
| `cargo clippy -p agent_ui --all-targets` | PASS    |
| `cargo check -p zzz`                     | PASS    |
| `cargo fmt --all`                        | PASS    |
| `./script/check-upstream-ledger`         | PASS    |
| Runtime repro on the shipped build       | NOT RUN |

## Remaining

- Elicitation form validation messages are still English, as noted in the
  2026-09-22 report; the synchronous validators have no `cx`.
- `EntryViewState::remove` still drains the incoming range unclamped. It is
  correct once the slot invariant holds; if another entry variant is added
  upstream, it must get a slot here or the same panic class returns.
