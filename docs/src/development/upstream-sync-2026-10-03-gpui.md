---
title: Upstream Sync 2026-10-03 GPUI Refactor
description: Targeted Zed upstream review for GPUI diagnostics and threaded test dispatch.
---

# Upstream Sync 2026-10-03 GPUI Refactor

## Scope

- Local branch: `refactor/gpui-architecture` from `main` at `152a5eb983a883c69a6cc4eae082312ba75aa9f9`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `decbf641b18f1982b3475c037e7c5c554471574f`
- Live upstream head queried: `a84689073d296dfd39987bc7dd478e43ef76d83a`
- Query method: direct `git ls-remote` and `git fetch --no-tags`; no named upstream remote
- Review purpose: GPUI frame diagnostics, benchmark dispatcher, and AccessKit boundary

The reviewed baseline is an ancestor of the live head. This targeted review does not
advance the general upstream baseline; it records the commits used to guide the GPUI
refactor and preserves ZZZ's local-first, no-account, ACP-only boundary.

## Decisions

| Upstream                                   | Class | Local commit | Disposition                                                                                                                                                                                                                                           |
| ------------------------------------------ | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `a9e7b7767207ccf40bcba04a4c5cad96d6916f13` | A     | --           | Already equivalent: ZZZ has feature-gated `InputLatencyTracker` and `Window::input_latency_snapshot`.                                                                                                                                                 |
| `9e236090b9a31338caf233d440f724922b58d7e1` | C     | --           | Frame-duration behavior is coupled to Zed telemetry and `input_latency_ui`; local diagnostics stays provider-free.                                                                                                                                    |
| `a21007b7a948e46be719150f5e9968bfcd1078`   | C     | --           | Broad profiler/Window rewrite is not isolatable against ZZZ's current APIs and feature graph.                                                                                                                                                         |
| `07a0bd1220bddb940028d610e40319a459dd49fb` | C     | --           | Product keybindings and debug overlay are outside the frame journal contract.                                                                                                                                                                         |
| `1861e58f984c76afc06032e753557994ffc8fe44` | C     | --           | Hang journal/watchdog and telemetry migration import rejected product surfaces.                                                                                                                                                                       |
| `55007f518bc1d49e6b3291c5eaa1aabf649b36fd` | C     | --           | Dirty-to-present reporting is tied to Zed's telemetry report.                                                                                                                                                                                         |
| `cd4fc8de4ca8548cca2567352b87bcaaec13328f` | C     | --           | Platform frame-request timestamp APIs require later platform seams and the unabsorbed hang journal.                                                                                                                                                   |
| `36b6d0951fdee409f0957294a69360ba2e8e980e` | C     | --           | Follow-up to the rejected/unabsorbed hang journal path.                                                                                                                                                                                               |
| `8886dcb0d4ea0e145e4512d415d3260602eca99`  | B     | `49b351afb1` | Ported worker pool, main-thread handoff, real-time timer queue, idle tracking, and explicit shutdown using ZZZ's existing `PlatformDispatcher` and priority queue. Omitted the upstream rename-only benchmark graph and incompatible profiler fields. |
| `cc053a4a6fa2fd0e8793201ed9099466af1be0b1` | C     | --           | AccessKit semantic writer path is absent from ZZZ, so the author-id builder is not an independent safe change.                                                                                                                                        |
| `0eda7703f6c88aa08a25c1d2105ff1ca46f775d4` | C     | --           | ZZZ's macOS backend has no AccessKit adapter or `SubclassingAdapter` ownership to release.                                                                                                                                                            |

Totals: one A, one B, and nine C.

## Applied work

`49b351afb1` is a local B port. It keeps deterministic `TestDispatcher` for ordinary
unit tests and exposes `ThreadedDispatcher` only through `test-support`. Worker and timer
threads use ZZZ's existing priority queue and scheduler runnable metadata. The dispatcher
tracks in-flight background work, waits for main-thread handoffs, cancels pending timers,
and signals timer shutdown when dropped. No upstream account, telemetry, collaboration,
native-agent, or AccessKit writer surface was imported.

The frame diagnostics implementation is local code guided by the safe observation invariant;
it is recorded under the A/B/C decisions above and does not claim a direct upstream sync.
Its commits are `8d410f4b55` (core), `d031aa92de` (deterministic runner),
`766b7cb85c` (cache replay runner), and `567ff2f4c3` (batched journal and allocation probe).

## Verification

| Check                                                                     | Result    | Notes                                                         |
| ------------------------------------------------------------------------- | --------- | ------------------------------------------------------------- |
| Live `git ls-remote`/`git fetch --no-tags`                                | `PASS`    | `FETCH_HEAD=a84689073d296dfd39987bc7dd478e43ef76d83a`         |
| Reviewed baseline ancestry                                                | `PASS`    | `decbf641...` is an ancestor of `FETCH_HEAD`                  |
| `cargo check --locked -p gpui --features test-support`                    | `PASS`    | Threaded dispatcher compiles                                  |
| `cargo test --locked -p gpui --features test-support threaded_dispatcher` | `PASS`    | handoff, timer/cancel, and 100 dispatcher teardown iterations |
| `./script/clippy -p gpui --features test-support`                         | `PASS`    | all-target release checks and philosophy gate                 |
| GPUI default/diagnostics test suites                                      | `PASS`    | 214 default tests; 216 diagnostics tests; no failures         |
| AccessKit native runtime QA                                               | `NOT RUN` | Current host is Linux and ZZZ has no native adapter path      |

## Hard stops and omissions

The AccessKit commits remain C because the required core writer and platform adapters are
absent in ZZZ. The frame timestamp/hang-journal series remains C until a later platform and
accessibility review can isolate a ZZZ caller without importing rejected telemetry machinery.
