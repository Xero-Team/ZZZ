# Reference: ZZZ Locale Catalogs

Normative material for `localizing-ui-text`. Read `Catalog layout` and `Key rule`
first, then the section the edit needs.
Keep code, keys, and commands in English; keep the report in Simplified Chinese.

## Contents

Read `Catalog layout` and `Key rule` first; every edit depends on them.

| If you are | Read |
|---|---|
| changing the call site that renders the string | `Call sites` |
| working with formatted or plural text | `Placeholders` |
| fixing a failing locale gate | `Gates` |
| removing a string | `Removing a key` |
| choosing the wording | `Translation notes` |

`Sources` is provenance only; read it when a rule's origin is questioned.

## Catalog layout

Two files, both tracked, both authoritative for display:

- `assets/locales/en.json`
- `assets/locales/zh-CN.json`

Shape is a single root object with one `entries` map:

```json
{
  "entries": {
    "menu.app": "ZZZ",
    "menu.about": "About ZZZ",
    "editor.git.count_one": "{} file"
  }
}
```

Each entry is a flat string. There is no nesting beyond `entries`, and no arrays.
Keys are unique across the whole map; a duplicate key is rejected at parse time
even though JSON parsers would silently keep the last one.

## Key rule

`tooling/xtask/src/tasks/check/locale_catalog.rs` defines the accepted shape:
at least two dot-separated segments, each non-empty and containing only ASCII
lowercase letters, digits, and underscores.

| Accepted | Rejected |
|---|---|
| `menu.settings.open` | `Menu.Settings` (uppercase) |
| `agent_ui.message_editor.cut` | `single` (one segment) |
| `editor.git.count_one` | `a..b` (empty segment) |
| `database.result_view.row_2` | `has space.here` |

Follow the namespace of the surface you are editing. The first segment is the
area (`agent_ui`, `editor`, `terminal`, `settings`), the middle segments narrow
to the view, and the last is the label. `git diff --stat assets/locales/` and
`rg -n '"<area>\.' assets/locales/en.json` show the neighboring keys.

Values must be non-blank strings. Whitespace-only values fail the gate.

## Call sites

`i18n::tr(cx: &App, key: &str, fallback: &str) -> String` returns the catalog value
for the active locale, or the fallback when the key is missing from both catalogs.
The fallback therefore matters only on a broken tree, which is why it must match
`en.json`: a drifted fallback is invisible until the day the catalog fails to
load.

Crates in this workspace commonly re-alias the crate rather than use the full
path:

```rust
use i18n as app_i18n;

let cut = app_i18n::tr(cx, "agent_ui.message_editor.cut", "Cut");
```

Both spellings are the same function. `check locale-fallbacks` matches any call
path whose final segment is `tr`.

Two consequences worth remembering:

- Pass a string literal key and literal fallback. A computed key, such as a key
  selected from a table, is skipped by the gate, so that escape hatch is
  unreviewed by design; prefer literals where the text is fixed.
- The context must be an `App`. Inside a render method use the `cx` you were
  given; inside an `Entity::update` closure use the inner `cx`.

## Placeholders

Values interpolate with Rust format syntax. `check locale-placeholders` compares the
multiset of placeholders between the two catalogs and syntax-checks each value on
its own.

| Form | Meaning | Rule |
|---|---|---|
| `{}` | next positional argument | same count on both sides |
| `{0}`, `{1}` | explicit positional | index may be reordered, count must match |
| `{name}` | named argument | name spelled identically on both sides |
| `{:?}` | debug formatting | specifier must survive translation |
| `{{`, `}}` | literal braces | not placeholders; keep both braces |

Reordering is allowed because the gate compares sorted multisets. Dropping a
placeholder shifts every later argument passed through `format!` or `replacen`,
which is why a mismatch blocks the change instead of warning.

A value that is malformed on both sides, such as an unclosed `{` or a stray `}`,
fails the syntax pass rather than slipping through parity.

## Gates

All four run under `cargo xtask check all` and individually:

| Command | Rejects | Source |
|---|---|---|
| `cargo xtask check locale-catalog` | duplicate keys, malformed keys, blank values, non-string values | `tooling/xtask/src/tasks/check/locale_catalog.rs` |
| `cargo xtask check locale-keys` | key sets that differ between the two catalogs | `script/check-locale-keys` |
| `cargo xtask check locale-fallbacks` | a `tr` call whose literal key is absent from `en.json`, or whose literal fallback differs | `tooling/xtask/src/tasks/check/locale_fallbacks.rs` |
| `cargo xtask check locale-placeholders` | placeholder count, name, or syntax mismatches | `tooling/xtask/src/tasks/check/locale_placeholders.rs` |

The quick parity check without the toolchain:

```sh
python3 - <<'PY'
import json
en = json.load(open("assets/locales/en.json"))["entries"]
zh = json.load(open("assets/locales/zh-CN.json"))["entries"]
print("only en:", sorted(set(en) - set(zh)))
print("only zh:", sorted(set(zh) - set(en)))
PY
```

## Removing a key

Removal touches three places, in this order:

1. Change or delete the call site.
2. `rg -n '"<key>"' crates/` to confirm nothing else references it, including
   tests and settings copy.
3. Delete the entry from both catalogs.

Do not delete the entry first and leave the call site computing a fallback: that
tree passes the locale gates and ships an untranslated string. `check
locale-fallbacks` only inspects literal keys, so a leftover computed reference is
invisible to it.

## Translation notes

- `docs/.rules` governs voice, terminology, and punctuation for user-facing copy;
  read it before writing a batch of Chinese values.
- Translate the intent of the control, not the English word. A menu action reads
  like an action in Chinese too.
- Keep length near the English value. These strings sit in menus, buttons, and
  status bars, and a much longer Chinese string wraps in places the layout did not
  budget for.
- Product and brand names, file formats, command names, HTTP headers, and
  identifiers stay untranslated, per `AGENTS.md`.
- Do not localize text that only developers see: log lines, panic messages, and
  internal error codes.

## Sources

- Repository: `AGENTS.md` (i18n section), `docs/.rules`,
  `docs/src/ai/skills.md`, `crates/i18n/src/i18n.rs`,
  `tooling/xtask/src/tasks/check/{locale_catalog,locale_fallbacks,locale_placeholders}.rs`,
  `script/check-locale-keys`.
- Adapted from `i18n-localization` in
  https://github.com/alirezarezvani/claude-skills (MIT). Its key/placeholder/plural
  checklist informed this reference; the commands, catalog shape, and key rules are
  ZZZ's own, so its Python scripts are not bundled here.
