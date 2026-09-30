# Reference: ZZZ Code Smell Catalog

Normative material for `hunting-code-smells`. Load this before reviewing.
Keep the report in Simplified Chinese; keep code, commands, and this catalog in
English.

## Contents

- How to use this catalog
- Severity model
- 1. ZZZ hard rules (P0)
- 2. Rust idiom smells (P1)
- 3. Clean Code structure smells (P1/P2)
- 4. GPUI and architecture smells (P1/P2)
- 5. i18n and docs smells (P1/P2)
- 6. Quality optimization leads (P2)
- 7. Mechanical checks
- 8. Review report template
- 9. Reviewer anti-patterns
- Sources

## How to use this catalog

- Work top-down by severity. Do not spend time on P2 polish while a P0 exists.
- Every finding needs a `file:line`, the rule ID or a one-line smell name, and
  the smallest safe fix. No `file:line`, no finding.
- Detection commands are leads, not proof. Read the surrounding code before
  reporting.
- When a rule overlaps an existing skill, defer: upstream absorption to
  `absorbing-upstream`, philosophy removal to `enforcing-philosophy-absence`,
  skill authoring to `creating-skills`.

## Severity model

| Severity | Meaning | Examples |
|----------|---------|----------|
| P0 | Correctness, safety, philosophy, or an explicit project rule is violated. Blocks merge. | `unwrap()`, `let _ =` on fallible, dropped task, philosophy surface, `mod.rs`, i18n key mismatch |
| P1 | Idiom or structural regression with clear, contained fix. | unnecessary clone, stringly typed, function does two things, feature envy |
| P2 | Polish or opportunity, safe to defer. | naming precision, magic number, comment cleanup, unused dep |

Rule IDs in sections 2-3 keep their Clean Code letters (`G5`, `N1`, `F1`, …)
and rust-skills prefixes (`own-`, `anti-`, `perf-`, …) so they can be traced to
the sources.

## 1. ZZZ hard rules (P0)

These come from `.rules`, `AGENTS.md`, `clippy.toml`, and
`[workspace.lints]`. They override every idiom preference.

| ID | Rule | Detect | Fix |
|----|------|--------|-----|
| Z-PANIC | No `unwrap()` / `expect()` outside tests and true invariants | `rg -n '\.unwrap\(\)|\.expect\(' <path> --glob '*.rs'` | `?`, `ok_or`/`context`, or `expect("invariant: …")` only when the invariant is real |
| Z-DISCARD | No `let _ =` on fallible operations | `rg -n 'let _ = ' <path> --glob '*.rs'` | `?`, `.log_err()`, `.detach_and_log_err(cx)`, or explicit `match` |
| Z-MODRS | No `mod.rs` | `git ls-files '**/mod.rs'` | move to `src/<module>.rs` and update the parent `mod` declaration |
| Z-TASK | Dropped `cx.spawn` / `cx.background_spawn` is cancelled | `rg -n 'cx\.(spawn|background_spawn)' <path>` | await, `detach()`, or store the `Task` in a field |
| Z-CX | In `Entity::update` use the inner `cx`; never re-enter the same entity | read each `update` closure | use the closure's `cx`; restructure to avoid nested self-update |
| Z-PROC | Use `smol::process::Command`, not `std::process::Command` | `./script/clippy` (disallowed-methods) or `rg -n 'std::process::Command'` | `smol::process::Command` |
| Z-TIMER | GPUI tests use executor timers | `rg -n 'smol::Timer::after' <path>` | `cx.background_executor().timer(dur).await` |
| Z-SERDE-IO | Parse from slice, not reader | `rg -n 'serde_json(_lenient)?::from_reader' <path>` | read into `Vec`/`String`, then `from_slice` |
| Z-I18N-MISS | User-facing text is localized | `rg -n 'Label::new\("|\.child\("[A-Z]|SharedString::from\("[A-Z]' <path>` (leads) | `i18n::tr(cx, "key", "fallback")` |
| Z-I18N-KEYS | Key sets match in both catalogs | see section 7 locale parity | add the missing key to `en.json` and `zh-CN.json` |
| Z-I18N-FB | In-code `fallback` equals `en.json` value | `rg -n 'tr\(cx, "[^"]+", "[^"]*"' <path>` | make fallback identical to the `en.json` entry |
| Z-PHIL | No rejected commercial/network surface returns | `./script/clippy` runs `script/check-philosophy`; `./script/check-philosophy` | delete the surface; never gate it behind a setting |
| Z-COMMENT | Comments explain why only | `rg -n '^\s*//' <path>` | delete restating/organizational comments |
| Z-NAMES | Full-word variable names | naming pass | rename abbreviations |
| Z-FILES | Prefer existing files; no speculative new files | `git diff --stat` | fold into the existing logical component |

### Philosophy gate detail

`script/check-philosophy` asserts that removed surfaces stay removed: product
defaults, source tree, locale catalogs, legal, and docs. If a change
reintroduces telemetry, accounts, billing, trials, hosted AI/collab,
auto-update, or a default remote endpoint, it is P0. The fix is deletion, not
configuration. Hand the removal to `enforcing-philosophy-absence` if it is
larger than the reviewed change.

## 2. Rust idiom smells (P1)

Derived from the rust-skills rule set. Apply only where the project rule does
not already decide.

### Ownership and borrowing (`own-`, `anti-`)

- `own-borrow-over-clone`: `.clone()` used to dodge the borrow checker, clone
  in a loop, or clone just to read. Detect:
  `rg -n '\.clone\(\)' <path>` then check liveness. Fix: borrow, or restructure.
- `own-slice-over-vec` / `anti-string-for-str`: `&String`, `&Vec<T>` in
  signatures. Fix: `&str`, `&[T]`, or generic `impl AsRef<str>`.
- `anti-vec-for-slice`: pass `Vec<T>` where `&[T]` works.
- Excessive `Arc`/`Mutex`/`Rc<RefCell<_>>` where ownership or a GPUI entity
  would do (`anti-over-abstraction`).

### Error handling (`err-`, `anti-`)

- `anti-unwrap-abuse`, `anti-panic-expected`: panic on recoverable input.
- `anti-empty-catch`: errors swallowed with `if let Err(_)`, `.ok()`,
  `let _ =`, or an empty `match` arm.
- Error type is a `String` or `anyhow::Error` in a public library API where a
  typed error (`thiserror`) belongs. Apps may use `anyhow`.
- `.map(|_| ())` or `.unwrap_or_default()` hiding a failure that the caller
  needs.

### Type safety (`type-`, `anti-`)

- `anti-stringly-typed` / `type-no-stringly`: strings or `&str` standing in for
  an enum, command, or identifier.
- Boolean parameters selecting behavior (`F3`, `G15`, `type-enum-states`). Fix:
  split the function or take an enum describing the intent.
- Primitive obsession / sentinel values (`-1`, `usize::MAX`) instead of
  `Option`, `NonZero`, or a newtype (`type-newtype-ids`,
  `type-newtype-validated`).
- `Deref` used to fake inheritance instead of `type-deref-coercion` guidance.

### API and abstraction (`api-`, `anti-`)

- `F1` too many arguments (4+). Fix: parameter struct or builder.
- `F2` output arguments. Fix: return values, or a method on the mutated type.
- `anti-over-abstraction`: trait/generic/introduction added for a single
  caller, or a trait with one implementor that is not a test seam.
- `anti-type-erasure`: `Box<dyn Trait>` where `impl Trait` or a generic is
  enough; `trait-dyn-vs-generic`.

### Async, concurrency, performance (`async-`, `conc-`, `perf-`, `mem-`)

- `anti-lock-across-await`: a `Mutex`/`RwLock` guard held across `.await`.
- Blocking work (`std::fs`, `std::process`) inside an async body.
- `anti-collect-intermediate`: `.collect::<Vec<_>>()` immediately iterated.
- `anti-format-hot-path`: `format!`/`to_string()` in a hot loop.
- `anti-index-over-iter`: `for i in 0..v.len() { v[i] }` instead of iteration.
- `perf-*`: default hasher in a hot map, repeated allocation, missing
  `with_capacity` on a known-size `Vec`.

### Unsafe (`unsafe-`)

- Missing `// SAFETY:` comment (`unsafe-safety-comment`).
- Over-broad `unsafe` scope (`unsafe-minimize-scope`).
- `unsafe` without a Miri-visible test (`unsafe-miri-ci`). Report, do not
  rewrite.

## 3. Clean Code structure smells (P1/P2)

Adapted from `clean-code-skills` to Rust. Use the letter IDs in reports.

### Functions

- `F3` / `G15` flag and selector arguments.
- `G30` function does more than one thing. Detect by sectioning the body.
- `G34` statements at mixed abstraction levels in one function.
- `G10` variables declared far from first use; private fn far from caller.
- `G28` complex boolean inline. Fix: extract `fn should_…` or a named `let`.
- `G29` negative conditionals (`if !thing.should_not_…`).
- `G33` boundary expressions (`level + 1`) repeated. Fix: named local.

### Duplication, dead code, clutter

- `G5` duplication: copy-pasted blocks, repeated match/if-else ladders, similar
  algorithms. This is the highest-value structural smell.
- `G9` / `F4` dead code: unreachable arms, uncalled private fns. Rust's
  dead_code lint catches some; check `#[allow(dead_code)]` sites.
- `G12` clutter: unused variables, no-op `impl`, information-free comments.
- `C5` commented-out code. Delete it.
- `C3` / `C2` redundant or obsolete comments.
- `G25` magic numbers and magic strings. Fix: named constant.
- `G16` obscured intent: dense expressions without explanatory locals (`G19`).
- `G11` inconsistency: the same concept named or handled differently in
  nearby code.
- `G14` feature envy: a free fn reaching through another type's accessors.
- `G36` transitive navigation (Law of Demeter): `a.get_b().get_c().do_thing()`.
- `G13` / `G17` misplaced responsibility: code placed for convenience, not
  where a reader expects it.

### Names

- `N1` poor names; `N2` names at the wrong abstraction level; `N4` ambiguous
  names; `N5` name length vs. scope; `N6` encodings/Hungarian notation;
  `N7` names hiding side effects.
- Rust naming conventions (`C-CONV`): `as_` (free, borrowed), `to_` (may
  allocate), `into_` (consuming); `new`/`with_*` constructors; predicates use
  `is_`/`has_`; traits use verbs.

### Tests (T1-T9), Rust-flavored

- `T1`/`T5` missing boundary tests (empty, zero, max, off-by-one, error path).
- `T4`/`G4` skipped or ignored tests and lint suppressions used to hide a
  failure.
- `T9` slow tests. `#[ignore]` as a permanent state is a smell.
- Prefer `test-arrange-act-assert`, `test-descriptive-names`,
  `test-should-panic`, `test-cfg-test-module` from rust-skills.
- GPUI tests: use executor timers and `run_until_parked`.

## 4. GPUI and architecture smells (P1/P2)

From `docs/src/development/ownership-and-data-flow.md` and `.rules`.

- `Rc<RefCell<_>>` shared state instead of app-owned `Entity<T>`.
- Polling another entity's state inside `render` instead of `cx.observe` /
  `cx.subscribe`.
- `cx.notify()` missing after a state change that affects rendering, or
  fired unconditionally on every update.
- A `Subscription` from `observe`/`subscribe` dropped immediately, silently
  stopping the callback. Keep it in a `_subscriptions` field or `.detach()`.
- Mutual strong `Entity` handles between two entities. Fix: `WeakEntity`.
- Using the outer `cx` inside an `update` closure, or nesting a self-update.
- A long-lived handle to an entity that should not keep it alive.
- New `Entity` type created only to hold a temporary; prefer local state or a
  `RenderOnce` component.

## 5. i18n and docs smells (P1/P2)

- Hardcoded user-facing English in UI construction (`Label`, `.child("…")`,
  `SharedString::from` on visible copy).
- Key added to only one of `en.json` / `zh-CN.json`.
- Key sets diverged between the two catalogs.
- In-code fallback drifted from the `en.json` value (invisible at runtime,
  so it rots).
- Localizing brand names, shell commands, HTTP headers, or internal/log
  identifiers (do not localize these).
- Docs changes: `docs/AGENTS.md` is authoritative. Run Prettier
  (`cd docs && npx prettier --check src/`) and do not hotlink `zed.dev`
  images.

## 6. Quality optimization leads (P2)

From `docs/src/development/bloat.md`. Leads, not rules.

- Unused dependencies: `cargo shear` and `cargo machete`; trust a hit only
  when both agree or you confirm with `cargo tree`.
- Binary size / duplication: `cargo bloat --release -p zzz --bin zzz`;
  `CARGO_PROFILE_RELEASE_LTO=fat cargo llvm-lines --release --sort copies -p zzz --bin zzz`.
- Generic amplification: a generic helper with many monomorphized copies; test
  whether it can be non-generic or take a trait object.
- Async cost: an `async fn` that never awaits can be plain code.
- Keep dependency removals and refactors in separate changes.

## 7. Mechanical checks

Run from the repository root.

```sh
# hard gate + workspace lints + philosophy (scoped form: -p <crate>)
./script/clippy
./script/clippy -p <crate>

cargo fmt --all -- --check

script/check-todos
script/check-keymaps
script/check-licenses
script/check-links
```

Locale parity (jq is a lead; use the Python fallback if jq is absent):

```sh
comm -3 \
  <(jq -r '.entries | keys[]' assets/locales/en.json   | sort) \
  <(jq -r '.entries | keys[]' assets/locales/zh-CN.json | sort)

python3 - <<'PY'
import json
en = json.load(open("assets/locales/en.json"))["entries"]
zh = json.load(open("assets/locales/zh-CN.json"))["entries"]
print("only en:", sorted(set(en) - set(zh)))
print("only zh:", sorted(set(zh) - set(en)))
PY
```

Fast P0 detectors:

```sh
rg -n '\.unwrap\(\)|\.expect\(' crates/<crate>/src --glob '*.rs'
rg -n 'let _ = ' crates/<crate>/src --glob '*.rs'
rg -n 'cx\.(spawn|background_spawn)' crates/<crate>/src --glob '*.rs'
rg -n 'smol::Timer::after|std::process::Command|from_reader' crates/<crate>/src --glob '*.rs'
git ls-files '**/mod.rs'
```

Tests for a behavior-affecting change:

```sh
cargo test -p <crate>
```

## 8. Review report template

Write the report in Simplified Chinese. Keep code identifiers in English.

```markdown
## 代码异味审查报告

**范围**: <diff / crate / 文件列表>
**检查命令**: <实际运行的命令与结果>

### P0 — 阻断合并

1. `crates/x/src/y.rs:123` — <异味名称 / 规则 ID>
   - 问题: <一句话说明>
   - 修复: <最小安全修复>

### P1 — 建议本次修复

1. ...

### P2 — 可延后 / 机会项

1. ...

### 已修复

- `file:line` — <改了什么>

### 未修复（仅报告）

- `file:line` — <为什么超出本次范围>

### 剩余最高风险

- <一个>
```

## 9. Reviewer anti-patterns

- Drive-by refactoring or reformatting outside the reviewed area.
- Reporting without `file:line`, or padding with speculative findings.
- "Fixing" a lint by adding `#[allow(...)]` or deleting the test (`G4`).
- Inventing an abstraction for a single caller (`anti-over-abstraction`).
- Doing a large redesign the user did not ask for; report and propose instead.
- Editing `.rules` inline instead of proposing additions.
- Reintroducing a philosophy surface behind a setting.
- Treating a Clean Code heuristic as a hard rule when the project already
  decided otherwise. Project rules win.

## Sources

- ZZZ: `.rules`, `AGENTS.md`, `docs/AGENTS.md`,
  `docs/src/development/bloat.md`,
  `docs/src/development/ownership-and-data-flow.md`, `docs/src/mission.md`,
  `clippy.toml`, `Cargo.toml` `[workspace.lints]`, `script/clippy`,
  `script/check-philosophy`.
- `clean-code-skills` (Robert C. Martin, *Clean Code*, ch. 17 smell
  taxonomy) — https://github.com/hatlesswizard/clean-code-skills.
- `rust-skills` (265 Rust rules) — https://github.com/leonardomso/rust-skills.
- `rust-course` (Rust 语言圣经, naming and idiomatic conventions) —
  https://github.com/sunface/rust-course.
