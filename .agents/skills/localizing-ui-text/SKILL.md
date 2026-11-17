---
name: localizing-ui-text
description:
  "Adds, changes, or removes user-facing text in ZZZ through the locale catalogs
  and i18n::tr. Use when a UI string is hardcoded, when adding a label, menu
  entry, dialog, or status message, when editing assets/locales/en.json or
  assets/locales/zh-CN.json, or when a locale gate fails. Do not use for docs,
  logs, shell commands, or protocol identifiers."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - i18n
    - locales
    - ui
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. It carries the key shape, the
placeholder rules, and what each locale gate rejects. A key that exists in one
catalog only, or a fallback that drifted from `en.json`, fails a gate and ships
nothing.

# Localizing UI Text

You add or change user-facing text so that both catalogs and every call site agree.
The two catalogs are the authority for what is displayed; the in-code fallback is
a copy of the English value that the gate compares byte for byte.

## Purpose

Land a user-facing string once, in both `assets/locales/en.json` and
`assets/locales/zh-CN.json` plus its `i18n::tr` call site, with matching keys,
matching placeholders, and a fallback equal to the English value.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write catalog values in the language of their catalog. Write code, commit
  messages, and key names in English.
- When a technical term could be ambiguous, add a short Chinese gloss followed by
  the English term in parentheses.

## When to Use

- A UI surface shows an English literal that should be translated.
- The user adds a label, menu item, dialog, tooltip, or status message.
- The user edits `assets/locales/en.json` or `assets/locales/zh-CN.json`.
- `cargo xtask check locale-*` fails and the message names a key or a call site.

Do not use this skill to translate documentation (`docs/src/`), to localize log
lines, telemetry names, shell commands, HTTP headers, or internal identifiers, or
to add a new language. Brand names and product names stay as written.

## Operating Rules

- Project hard rules first: `.rules`, `AGENTS.md`, and the locale gates outrank
  any translation preference.
- Add every key to both catalogs in the same change. `check locale-keys` requires
  identical key sets, so a one-sided key is a failing tree, not a partial step.
- The in-code `fallback` equals the `en.json` value exactly. `check
  locale-fallbacks` parses real `tr` call sites with `syn`, so a near-match fails.
- Placeholders are Rust format syntax, and both catalogs must carry the same
  multiset: `{}`, `{0}`, `{name}`, `{:?}`. Reordering is allowed; adding, dropping,
  or renaming one is not. `{{` and `}}` are literal braces.
- Keys are flat, dot-separated, lowercase identifiers with at least two segments:
  `area.surface.element`, for example `agent_ui.message_editor.cut`. No spaces,
  uppercase, or `a..b`. See `REFERENCE.md` for the exact rule.
- Reuse an existing key when the text is identical in both languages. Do not
  duplicate the same string under a second key.
- Use `i18n::tr(cx, key, fallback)`. Crates commonly alias the crate with
  `use i18n as app_i18n;` and call `app_i18n::tr(...)`; match the crate you are in.
- Do not localize brand names, shell commands, HTTP headers, file-format names, or
  internal and log identifiers.
- Removing a key is the mirror image: delete the call site first, then remove the
  key from both catalogs, and confirm nothing else references it.
- Do not add a runtime fallback path, a per-locale setting, or a new catalog
  format to work around a gate failure. Fix the data.

## Prerequisites

- A checked-out repository with `assets/locales/en.json` and
  `assets/locales/zh-CN.json`.
- The pinned Rust toolchain for `cargo xtask check`.
- `rg` to find existing keys and call sites.

## Procedure

1. Read `AGENTS.md`, `.rules`, and this skill's `REFERENCE.md`. Run
   `cargo xtask check locale-catalog` first so a pre-existing failure is known
   before you edit.
2. Decide the key. Find the nearest existing namespace for the surface with
   `rg -n '"<area>\.' assets/locales/en.json`, and follow its naming shape. Reuse
   an existing key when the text already matches.
3. Write the `en.json` value. Use `docs/.rules` for voice and terminology. If the
   string interpolates data, place the Rust format placeholder where the value
   belongs.
4. Write the `zh-CN.json` value with the same placeholders in any order. Translate
   meaning, not word for word, and keep the UI tone consistent with neighboring
   keys.
5. Change the call site to `i18n::tr(cx, "<key>", "<en value>")`, copying the
   English catalog value as the fallback. Keep the surrounding construction
   (`Label`, `SharedString`, menu item) as it was.
6. Run the four gates for the touched area, then the fast full pass:
   `cargo xtask check locale-catalog`, `locale-keys`, `locale-fallbacks`,
   `locale-placeholders`, then `cargo xtask check`.
7. Build the touched crate (`cargo check -p <crate>`) so a moved or renamed
   constant does not only fail at runtime.
8. Report the key, both values, the call site, and the gate results.

## Examples

```text
This dialog has a hardcoded "Cancel" button; route it through the catalogs.
```

```text
check locale-placeholders fails on the new export dialog key.
```

## Troubleshooting

- `check locale-keys` names a key that exists on one side: add it to the other
  catalog. Do not delete the existing side to make the gate pass.
- `check locale-fallbacks` reports a mismatch: copy the `en.json` value into the
  fallback argument exactly, including punctuation.
- `check locale-placeholders` reports a mismatch: compare the placeholder multisets
  in `REFERENCE.md`, then restore the missing placeholder rather than rewording
  around it.
- `check locale-catalog` rejects a key: it has one segment, an uppercase letter, a
  space, or an empty segment, or the value is blank.
- The string never appears at runtime: confirm the `App` you pass to `tr` is the
  same context, and that the key is inside the `entries` object rather than at the
  document root.

## Limitations

- The gates check key parity, structure, placeholders, and call sites. They cannot
  judge whether a translation reads well, so the Chinese copy still needs a human
  read.
- Only `en` and `zh-CN` exist. Adding a language is a separate change that touches
  the settings enum and the catalog loader.
- This skill does not translate documentation prose; `docs/.rules` governs that.

## Final Response

Summarize in Simplified Chinese:

- the key added, changed, or removed, with its English and Chinese values
- the call sites changed, each as `path/to/file.rs:line`
- gate results as PASS/FAIL for the four locale checks and `cargo check -p <crate>`
- any translation choice the user may want to reword
