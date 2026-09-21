---
title: Keeping ZZZ Lean
description: "How to find unused dependencies, oversized functions, and generic code amplification in ZZZ."
---

# Keeping ZZZ Lean

ZZZ is a large Rust workspace. Binary size and compile time are both
product features: a smaller binary starts faster and is easier to audit, and
a smaller dependency graph is easier to trust. This page collects the tools
and techniques that help.

None of this is a rule. Treat these as leads, verify each result, and keep
changes reviewable.

## Finding unused dependencies

Two tools overlap here:

- [`cargo-shear`](https://github.com/Boshen/cargo-shear)
- [`cargo-machete`](https://github.com/bnjbvr/cargo-machete)

Run both. `cargo-shear` tends to find more and produce fewer false
positives, but the two disagree often enough to be worth cross-checking.

```sh
cargo shear
cargo machete
```

A dependency reported by only one tool is worth a manual look before you
remove it. A dependency reported by both is usually real.

## Finding large and duplicated code

- [`cargo-bloat`](https://github.com/RazrFalcon/cargo-bloat) ranks
  functions by the space they take in the binary.
- [`cargo-llvm-lines`](https://github.com/dtolnay/cargo-llvm-lines) shows
  how much code each function generates after monomorphization.

```sh
cargo bloat --release -p zzz --bin zzz
CARGO_PROFILE_RELEASE_LTO=fat cargo llvm-lines --release --sort copies -p zzz --bin zzz
```

Use full LTO for `llvm-lines` so that every monomorphization lands in the
same crate, and sort by copies so the most-duplicated functions appear
first. Functions with a large number of copies are usually generic
functions that do not need to be generic, or that can take a trait object
instead of a type parameter.

## Generic code amplification

Every generic function or struct is compiled once per set of type
arguments. A generic helper used across the workspace can quietly become
dozens of copies. When `cargo llvm-lines` points at one:

1. Check whether the generic parameter is actually needed. Moving a
   function's body into a non-generic inner function often collapses the
   copies.
2. Prefer a trait object when the call site does not need static dispatch.
3. Move the generic boundary outward, so fewer layers are monomorphized.

Settings and configuration types are a common source of this in ZZZ.

## Async cost

Async Rust rewrites function bodies into state machines, which amplifies
code size. Not every function needs to be async. When reviewing a large
async function, ask whether it awaits anything at all; if it does not, it
can usually be plain code, or run on a background thread instead.

## Before you remove something

- A dependency may be used only on one platform, or only through a
  feature flag. Check `cargo tree` before deleting it.
- A large function may be large for a reason. Prefer moving code over
  rewriting behavior.
- Keep the change focused. Dependency removals and refactors belong in
  separate pull requests.
