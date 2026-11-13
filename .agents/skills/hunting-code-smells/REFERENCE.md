# Reference: ZZZ Code Smell Catalog

Normative material for `hunting-code-smells`. Load this before reviewing.
Keep the report in Simplified Chinese; keep code, commands, and this catalog in
English.

## Contents

- How to use this catalog
- Severity model
- 1. ZZZ hard rules (P0)
- 2. Memory safety and UB contract (P0/P1)
- 3. Rust idiom smells (P1)
- 4. Clean Code structure smells (P1/P2)
- 5. GPUI and architecture smells (P1/P2)
- 6. i18n and docs smells (P1/P2)
- 7. Quality optimization leads (P2)
- 8. Mechanical checks
- 9. Review report template
- 10. Reviewer anti-patterns
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

Rule IDs in sections 3-4 keep their Clean Code letters (`G5`, `N1`, `F1`, …)
and rust-skills prefixes (`own-`, `anti-`, `perf-`, …) so they can be traced to
the sources. Section 2 uses `MEM-` IDs anchored to a named clause in the
Reference or the FLS, so each finding cites the language contract and not an
opinion.

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
| Z-PROC | Use `smol::process::Command`, not `std::process::Command` | `cargo xtask clippy` (disallowed-methods) or `rg -n 'std::process::Command'` | `smol::process::Command` |
| Z-TIMER | GPUI tests use executor timers | `rg -n 'smol::Timer::after' <path>` | `cx.background_executor().timer(dur).await` |
| Z-SERDE-IO | Parse from slice, not reader | `rg -n 'serde_json(_lenient)?::from_reader' <path>` | read into `Vec`/`String`, then `from_slice` |
| Z-I18N-MISS | User-facing text is localized | `rg -n 'Label::new\("|\.child\("[A-Z]|SharedString::from\("[A-Z]' <path>` (leads) | `i18n::tr(cx, "key", "fallback")` |
| Z-I18N-KEYS | Key sets match in both catalogs | see section 8 locale parity | add the missing key to `en.json` and `zh-CN.json` |
| Z-I18N-FB | In-code `fallback` equals `en.json` value | `rg -n 'tr\(cx, "[^"]+", "[^"]*"' <path>` | make fallback identical to the `en.json` entry |
| Z-PHIL | No rejected commercial/network surface returns | `cargo xtask check philosophy` (also run as the `cargo xtask clippy` preflight); `script/check-philosophy` | delete the surface; never gate it behind a setting |
| Z-COMMENT | Comments explain why only | `rg -n '^\s*//' <path>` | delete restating/organizational comments |
| Z-NAMES | Full-word variable names | naming pass | rename abbreviations |
| Z-FILES | Prefer existing files; no speculative new files | `git diff --stat` | fold into the existing logical component |

### Philosophy gate detail

`cargo xtask check philosophy` asserts that removed surfaces stay removed: product
defaults, source tree, locale catalogs, legal, and docs. If a change
reintroduces telemetry, accounts, billing, trials, hosted AI/collab,
auto-update, or a default remote endpoint, it is P0. The fix is deletion, not
configuration. Hand the removal to `enforcing-philosophy-absence` if it is
larger than the reviewed change.

## 2. Memory safety and UB contract (P0/P1)

Source of truth: the Rust Reference UB list, the FLS legality rules, and the
UCG/MiniRust aliasing models. Section 1 rules are project-local; these are the
language-level contract, so they outrank every idiom and structural preference
below.

Read this section when the change touches `unsafe`, FFI, raw pointers,
`MaybeUninit`, `transmute`, `repr`, manually managed allocation, or a
hand-written interior-mutability type. When none of those are in scope, say so
in one line and move on — do not invent memory-safety findings.

### 2.1 Where trust is localized

Every formal system in `Sources` concentrates its trust in one place: lambda-Rust
in the lifetime logic, RustHornBelt in the prophecy/unsafe interface, Verus in
`external_body` and `assume`, Creusot in `#[trusted]`. The review lesson is not
the prover; it is that a soundness argument is only as strong as its trusted
base. ZZZ has no mechanized spec, so its trust base is:

1. new or widened `unsafe` blocks, `unsafe impl`, and `unsafe fn` signatures;
2. the FFI boundary (`crates/gpui_macos`, `crates/gpui_windows`, `crates/sqlez`,
   `crates/database_core`, `crates/media`, `crates/video`, `crates/util`);
3. hand-written buffers and interior-mutability types anywhere;
4. `vendor/` code, only when the change edits it.

Review in that order. A P0 memory-safety finding outside those four areas is
rare enough that you should double-check it before reporting.

### 2.2 Rule table

| ID | Contract | Detect | Fix |
|----|----------|--------|-----|
| MEM-ALIAS | A live `&T` referent is not mutated (except inside `UnsafeCell`); a live `&mut T` referent is not read or written through any other pointer and no other reference points to it; `Box<T>` counts as `&'static mut T`. Violating this is UB. Reference `behavior-considered-undefined.md` `r[undefined.alias]`; FLS `ownership-and-deconstruction.rst` aliasing rules | `rg -n 'as \*mut\|as \*const\|: \*mut \|: \*const '`; two pointers derived from one place; a stored `&mut` reused after a second borrow | one owner at a time; `UnsafeCell` for interior mutation; shorten each raw pointer's live range |
| MEM-PROV | Pointer arithmetic stays inside the same live allocation; a `usize` round trip does not restore provenance. Reference `r[undefined.place-projection]`, `memory-model.md` (bytes carry optional provenance) | `rg -n 'as usize\b\|wrapping_offset\|\.offset\('` | keep `*const T`/`*mut T` and use in-bounds `add`/`offset`; use `expose_provenance` / `with_exposed_provenance` only when an integer is genuinely required |
| MEM-VALID | Producing an invalid value is UB: `bool` is `0`/`1`, `char` is a non-surrogate scalar, an enum discriminant is valid, a `fn` pointer is non-null, scalars and pointers are initialized. Reference `r[undefined.validity]` | `rg -n 'mem::zeroed\|mem::uninitialized\|transmute'` | `MaybeUninit` plus explicit initialization; never `zeroed()` for a type whose all-zero pattern is invalid |
| MEM-INIT | Reading uninitialized memory is UB. `assume_init` only after full initialization; `Vec::set_len` only after every element is written; no `ptr::read` of an uninitialized place. FLS `ownership-and-deconstruction.rst` ("all memory starts as uninitialized"; a variable shall be initialized before it is accessed) | `rg -n 'assume_init\|set_len\(\|ptr::read'` | `write` then `assume_init_read` in that order, or a safe container |
| MEM-ALIGN | A place based on a misaligned pointer is UB when loaded or stored, and alignment comes from the dereferenced pointer type, not the field type. Reference `r[undefined.misaligned]` | `rg -n 'read_unaligned\|write_unaligned\|repr\(packed\)'` | `read_unaligned`/`write_unaligned`, or drop `repr(packed)` |
| MEM-CELL | `UnsafeCell<T>` is the only legal way to mutate through `&`; multiple `&mut UnsafeCell<T>` aliases are still UB. Reference `interior-mutability.md`; FLS (referent of an immutable reference is mutable only when the type has interior mutability) | `rg -n 'repr\(transparent\)'` near a `*mut` field; `&`→`*mut` casts | use `UnsafeCell`, or a GPUI entity / atomic |
| MEM-LAYOUT | `repr(Rust)` layout is unspecified (field order, padding), so `transmute` between `repr(Rust)` types and an FFI struct without `repr(C)` are unsound by contract. Reference `r[layout.repr.rust.unspecified]`, `r[layout.repr.c]` | `rg -n 'transmute\|repr\(C\)'` | `repr(C)` at the boundary, or a field-by-field conversion |
| MEM-UNSOUND | A safe API must not let safe client code reach any of the rows above; `unsafe` code that a safe caller can misuse is unsound. Reference `r[undefined.soundness]` | an `unsafe fn` that does not need `unsafe` at the call site; an invariant that exists only in a doc comment | mark it `unsafe`, narrow to `pub(crate)`, or add the runtime check |
| MEM-DOC | Every `unsafe` block and `unsafe fn` names the invariant it relies on, plus why the safe equivalent is not viable. `.rules` mandates this for new and changed code, so a bare `unsafe` there is P0; a pre-existing site is P1: report it, do not mass-edit | `rg -n 'unsafe \{\|unsafe fn'` cross-checked against `rg -n '// SAFETY:'` | add the `// SAFETY:` comment stating the invariant, not the code's behavior |

### 2.3 Aliasing models are not normative

Stacked Borrows and Tree Borrows (`unsafe-code-guidelines/wip/stacked-borrows.md`,
`wip/tree-borrows.md`) say so in their own headers: not normative, not endorsed
by the UCG WG, describing what Miri implements, with MiniRust as the
authoritative text. So never cite either model as "the rule". Cite the
Reference's `r[undefined.alias]` outline, and treat Miri as the executable
oracle:

- Miri needs a nightly toolchain; `rust-toolchain.toml` pins `1.100.0`, so this
  is `cargo +nightly miri test -p <crate>`, a manual lead and not a gate.
- Miri is not wired into CI here. Do not report "Miri is clean" unless you ran
  it, and say which crate and toolchain.

### 2.4 What not to do

- Do not turn a soundness finding into a lint suppression or a widened
  `unsafe` scope; both hide the invariant instead of stating it.
- Do not propose a `debug_assert!` as the fix for a UB path. Debug assertions
  vanish in release; state the precondition or enforce it.
- Do not rewrite an FFI shim you cannot build or test on its target platform.
  Report `file:line` and the clause it breaks.

## 3. Rust idiom smells (P1)

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

The contract for `unsafe` lives in section 2; these three rust-skills IDs are
the style half of it.

- Missing `// SAFETY:` comment (`unsafe-safety-comment`, and `MEM-DOC`).
- Over-broad `unsafe` scope (`unsafe-minimize-scope`).
- `unsafe` without a Miri-visible test (`unsafe-miri-ci`). Report, do not
  rewrite.

## 4. Clean Code structure smells (P1/P2)

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

## 5. GPUI and architecture smells (P1/P2)

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

## 6. i18n and docs smells (P1/P2)

- Hardcoded user-facing English in UI construction (`Label`, `.child("…")`,
  `SharedString::from` on visible copy).
- Key added to only one of `en.json` / `zh-CN.json`.
- Key sets diverged between the two catalogs.
- In-code fallback drifted from the `en.json` value (invisible at runtime,
  so it rots).
- Localizing brand names, shell commands, HTTP headers, or internal/log
  identifiers (do not localize these).
- Docs changes: `docs/AGENTS.md` is authoritative. Run `script/prettier` (the
  same script `cargo xtask check docs` runs) and do not hotlink `zed.dev`
  images.

## 7. Quality optimization leads (P2)

From `docs/src/development/bloat.md`. Leads, not rules.

- Unused dependencies: `cargo shear` and `cargo machete`; trust a hit only
  when both agree or you confirm with `cargo tree`.
- Binary size / duplication: `cargo bloat --release -p zzz --bin zzz`;
  `CARGO_PROFILE_RELEASE_LTO=fat cargo llvm-lines --release --sort copies -p zzz --bin zzz`.
- Generic amplification: a generic helper with many monomorphized copies; test
  whether it can be non-generic or take a trait object.
- Async cost: an `async fn` that never awaits can be plain code.
- Keep dependency removals and refactors in separate changes.

## 8. Mechanical checks

Run from the repository root.

```sh
# hard gate + workspace lints + philosophy preflight (scoped form: -p <crate>)
cargo xtask clippy
cargo xtask clippy -p <crate>

cargo fmt --all -- --check

# compiler-free repository gates; `check all` runs the whole set
cargo xtask check

# or run the individual gates that matter for the touched area
cargo xtask check todos
cargo xtask check keys
cargo xtask check scroll-handlers
cargo xtask check licenses
cargo xtask check module-layout
cargo xtask check test-markers
cargo xtask check locale-catalog
cargo xtask check locale-keys
cargo xtask check locale-fallbacks
cargo xtask check locale-placeholders
cargo xtask check dco
cargo xtask check philosophy
cargo xtask check upstream-ledger
cargo xtask check workflows
cargo xtask check docs

# local-only, needs extra tools on PATH
cargo xtask check links
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

Unsafe surface inventory (section 2 leads):

```sh
# where the trust base is, by crate, heaviest first
rg -c 'unsafe ' crates --glob '*.rs' | sort -t: -k2 -rn

# the constructs that carry the contract
rg -n 'unsafe \{|unsafe fn|unsafe impl' crates/<crate>/src --glob '*.rs'
rg -n 'transmute|mem::zeroed|mem::uninitialized|assume_init|set_len\(|from_raw_parts' crates/<crate>/src --glob '*.rs'
rg -n 'as \*(mut|const)|\*mut |\*const ' crates/<crate>/src --glob '*.rs'

# MEM-DOC: unsafe sites without a stated invariant
rg -n '// SAFETY:' crates/<crate>/src --glob '*.rs' | wc -l
```

Tests for a behavior-affecting change:

```sh
cargo test -p <crate>
# optional, manual, needs nightly in addition to the pinned 1.100.0
cargo +nightly miri test -p <crate>
```

## 9. Review report template

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

## 10. Reviewer anti-patterns

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
  `clippy.toml`, `Cargo.toml` `[workspace.lints]`,
  `tooling/xtask/src/tasks/check.rs`, `script/check-philosophy`,
  `cargo xtask check philosophy` (the `script/clippy` shim forwards to
  `cargo xtask clippy`).
- `clean-code-skills` (Robert C. Martin, *Clean Code*, ch. 17 smell
  taxonomy) — https://github.com/hatlesswizard/clean-code-skills.
- `rust-skills` (265 Rust rules) — https://github.com/leonardomso/rust-skills.
- `rust-course` (Rust 语言圣经, naming and idiomatic conventions) —
  https://github.com/sunface/rust-course.

### Memory-safety sources (section 2)

The memory-safety rules are anchored to the clauses named in section 2.2, not
to a reviewer's memory. Normative first, then supporting work. Checked
2026-11-13 by shallow clone; cite the URLs, not a local checkout.

### Normative for Rust as it ships

- **The Rust Reference** — https://github.com/rust-lang/reference,
  rendered at https://doc.rust-lang.org/reference/.
  `src/behavior-considered-undefined.md` is the UB list and the soundness
  definition used by MEM-ALIAS/…/MEM-UNSOUND; `src/memory-model.md` covers
  bytes, initialization, and provenance; `src/interior-mutability.md` is the
  `UnsafeCell` contract; `src/type-layout.md` has `r[layout.repr.*]`. Each
  clause has a stable `r[...]` anchor — quote the anchor in the report.
- **FLS (Ferrocene Language Specification)** —
  https://github.com/rust-lang/fls, rendered at
  https://rust-lang.github.io/fls/. `src/ownership-and-deconstruction.rst`
  gives the legality rules (one owner per value, referent initialized, no
  mutation while a reference is active, no overlapping reference while a
  `&mut` is active, aliasing `&mut`s from an unsafe context is UB);
  `src/unsafety.rst` enumerates the unsafe operations (raw-pointer
  dereference, reading or writing an external or mutable `static`, `union`
  field access other than assignment, calling an `unsafe fn`, `asm!`);
  `src/undefined-behavior.rst` indexes the UB sections. Ferrocene's
  qualification material (IEC 61508 / ISO 26262, the pre-qualified `core`
  API list) lives at https://public-docs.ferrocene.dev/.

### Aliasing models (explicitly non-normative)

- **Unsafe Code Guidelines** — https://github.com/rust-lang/unsafe-code-guidelines.
  `wip/stacked-borrows.md` and `wip/tree-borrows.md` both say in their own
  headers that they are non-normative, describe what Miri implements, and
  point at MiniRust as authoritative; `wip/memory-interface.md` is a stub
  redirecting to MiniRust. Use these to explain a finding, never as the rule.
- **MiniRust** — https://github.com/minirust/minirust (referenced by the UCG
  docs). Authoritative literate specification of the memory model and of Tree
  Borrows; reach for it when a finding turns on provenance or per-byte
  permissions.

### Mechanized soundness of safe Rust

- **lambda-Rust / RustBelt** — https://gitlab.mpi-sws.org/iris/lambda-rust.
  `theories/lang` formalizes the core language with the theorem that programs
  with data races get stuck; `theories/lifetime` and `theories/typing` hold
  the lifetime logic and the semantic types. `Missing.md` lists what the model
  deliberately does not cover (ZST corner cases, structural `Cell` slicing),
  which is a useful reminder that "proved sound" has a boundary.
- **RustHornBelt** — https://github.com/dewert99/rust-horn-belt. Extends the
  same base to functional correctness for code containing `unsafe`;
  `theories/lang`, `theories/typing`, `theories/prophecy`, plus
  `walkthrough.md` as the guided tour. Relevant because the local check is
  "does the unsafe abstraction have a contract", which is this paper's subject.
- **Oxide: The Essence of Rust** — https://arxiv.org/abs/1903.00982
  (Weiss, Gierczak, Patterson, Ahmed; v4 2021). First syntactic
  progress/preservation proof for borrow checking, with lifetimes modelled as
  an approximation of the provenances of references. The review consequence:
  a lifetime annotation approximates provenance and is not by itself a
  licence to fabricate a reference.

### Verification toolchains and their trusted bases

- **Aeneas** — https://github.com/AeneasVerif/aeneas;
  `documentation/aeneas-overview.md` explains the ownership-driven
  translation (references become plain values plus backward continuations, no
  heap model) and the `Result` monad for panics and overflow. **Charon** —
  https://github.com/AeneasVerif/charon — extracts LLBC from MIR.
  **mechanized-llbc** — https://github.com/AeneasVerif/mechanized-llbc — is
  the Rocq formalization of LLBC after Ho, Fromherz, Protzenko, *Sound
  Borrow-Checking for Rust via Symbolic Semantics* (2024), with
  `limitations-LLBC-sharp` for edge cases the model excludes. Use these for
  the argument shape: if the crate's borrow structure cannot be expressed,
  the tool reports it rather than proving it.
- **Verus** — https://github.com/verus-lang/verus, guide at
  https://verus-lang.github.io/verus/guide/. `source/docs/guide/src/memory-safety.md`
  states the key asymmetry — Verus has no safe/unsafe distinction, so it
  verifies memory safety through specifications rather than through unsafe
  encapsulation; `tcb.md`, `tcb`/`external_body`/`assume` and
  `interacting-with-unverified-code.md` mark the remaining trust holes.
- **VerusBelt** — https://github.com/verus-lang/verus-belt — the Rocq
  development validating Verus's proof-oriented extensions to the type
  system (`src/`, `doc/`, `VERUS-IMPL.md`).
- **Creusot** — https://github.com/creusot-rs/creusot. `ARCHITECTURE.md` for
  the pipeline; `guide/src/limitations.md` for what its contracts do not
  cover (capacity-overflow panics on `Vec` growth, architecture-specific type
  sizes, the `vec!` reimplementation); `guide/src/trusted.md` for `#[trusted]`.
  The transferable rule: a trust marker is a review target, not a proof.
