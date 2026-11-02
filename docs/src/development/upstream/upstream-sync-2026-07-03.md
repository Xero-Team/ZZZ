---
title: Upstream Sync 2026-07-03
description: Sync report for upstream Zed main changes reviewed on 2026-07-03.
---

# Upstream Sync 2026-07-03

This report records the upstream sync reviewed on 2026-07-03 in the
`sync-upstream-20260703` branch.

## Scope

- Upstream branch: `upstream/main`
- Upstream head reviewed: `bb48a42983f2a4bb9ac9d31c63abe02497088f67`
- Upstream head commit time:
  `2026-07-02T15:24:34-04:00`
- Candidate range boundary:
  `2026-06-20T16:00:00Z`
- Local base policy:
  keep fork-only philosophy from the root `README.md`

## Hard Filters

Allowed:

- editor, git, LSP, UI, platform compatibility, performance, stability
- ACP, MCP, sandbox-adjacent settings when they stay ACP-only
- provider-agnostic AI support
- OpenAI-compatible, Anthropic-compatible, local providers, Ollama,
  llama.cpp, manual silent configuration

Rejected:

- telemetry, analytics, tracking, remote telemetry
- sign-in, sign-up, trial, billing, subscription, upgrade, plan chips
- account routing, usernames, young-account logic, cloud-only UX
- shared-thread cloud/social features
- Guild/community automation, CI/release/marketing plumbing
- Zed Business or cloud-promotion docs
- upstream native or proprietary agent runtime paths

## Result

The sync reached the reviewed upstream head
`bb48a42983f2a4bb9ac9d31c63abe02497088f67`.

No candidate commit in the requested range remained unreviewed at the
end of the sync.

## A/B/C Summary

### A: Cherry-picked or absorbed with equivalent local adaptation

These upstream changes landed directly or as conflict-resolved
equivalents on the sync branch:

```text
ca2d7fd9e5 076fd14c88 e25e52be87 9a992ed33c 13dd39b4e4 356e396517 514b14ed49
c0945a8201 2df089ebe1 50b4a1c17e 91ff38aee2 39bba3c7ec 438070b1cf d3756f3025
d753a31db5 bab93f3191 42a2eff274 daf4656c87 f5c19126fb d655580d9b 5a5242ffeb
c8f68c38e2 f18e91efc6 8036a3c74b e0878c4989 9ac117693b a1d0015e70 64b8491fc6
25ea339552 6e085b2fdc 99c3d17da6 eae0b583c0 dae3e574e4 ec7c11c65c 35eaeb94a7
eb87750323 90587dd639 7582bf7434 33473c1cd3 480796c168 76e07d5c9a d7b9b28deb
03125da544 b083358680 d0802abdec d1e8c0b50f 9375695626 da2b62c917 e5513539ec
4a1cb2b1e2 442a3476bc 2ec0e6c2d7 969c6c719c 7eb4cb2bfa 78b6bf2fbe 17090674b3
e1ec575d3f 27a9e2057b 31fc9d5f47 ea87b05794 2882636c06 bb48a42983 52b61cf424
21d66eb9d5 b206841b4b 7b128f9263 8186af99a3 995e56d263
```

### B: Mixed or locally divergent commits, manually ported

Only the philosophy-safe parts of these commits were kept:

```text
7187d65774 15c31d4147 a2fee92e30 485aeabff3 45015f89d7 3eb9bf2d21 cf93437d6a
70fd3c5774 2df74932bc bfe0d7c8f6 5fa184f8b6 56562c28b0 c35650a884 e7afe9fcf9
4eb039b451 db30c67ed2 a8ffae4c00 7d545c0bae 1fd93cbd34 f964172a69 02aabb9cef
7e0f63412c
```

Kept portions included:

- OpenAI-compatible thinking support
- OpenAI/Anthropic-compatible provider configuration UI
- PTY startup race fix
- reqwest stale-connection handling
- ACP boolean config defaults
- MCP timeout settings page integration
- thread-search dismissal behavior
- ACP embedded resources and message-id handling
- git draft reconnect restore
- remote HEAD checkout support
- Ollama partial-model-fetch tolerance
- markdown preview context menu improvements
- clickable linked images in markdown
- shared-thread removal work aligned with fork philosophy
- project panel filename escaping and expand-all controls
- safe terminal path escaping

### C: Rejected

These commits were rejected because they violated fork philosophy,
depended on rejected cloud/account/runtime paths, or were upstream-only
infra/docs:

```text
50e6411571 17c0ebb0f7 479bce0995 1b7318bf8b 784b14e207 5989a369d3 471fb15d7a
0b61ce2039 7b73d5ccc3 53e4d34a71 0a7c84bc37 c55693876e 8ba35e5eac 5c58d5c49a
550ddc9405 4aa8ad9742 eef824cce5 fa66442a01 037f32aef0 c49a29f461 5d06fffd98
3d1b26d683 3648fe6f19 10b0795183 1e7f1a11f9 968379f5a1 3b2acfe0ad 35c3d27282
b3d5ead59f 8372eb1b13 153f709a18 779ca5ef72 3c312b596e d132afe9fc
```

## Final Local Commits

These commits were added on top of the local branch during this sync and
record the actual absorbed work:

```text
f7e756a752 sync: port project panel expand-all controls from 02aabb9cef
c1a465ab57 project_panel: Wrap filenames in code spans in confirmation dialogs (#53068)
709e7b8a00 workspace: Use remote host's path style when validating trust scope (#60139)
a66d107602 git_panel: Focus back on commit editor when expanded (#59901)
d51c57856d agent_ui: Close search when hitting escape from message editor (#59705)
a09f649096 markdown: Make linked images clickable (#59525)
7f2125a2b4 Add range-based whitespace and newline removal to buffer formatting (#53942)
a18d0c70d6 sync: port OpenAI-compatible thinking support from 7187d65774
28893ec1b9 sync: port OpenAI-compatible provider form from 15c31d4147
0551513e69 sync: port terminal startup handshake from a2fee92e30
f5cc6bd18c sync: port reqwest keepalive tuning from 485aeabff3
7ecd48e5ff sync: port ACP boolean config defaults from 45015f89d7
ed24d963ff sync: move MCP timeout settings into MCP subpage
690ee13065 sync: manually port git commit draft restore from bfe0d7c8f6
5656f622fd sync: manually port embedded tool resources from 2df74932bc
c37d19587f sync: manually port ACP message-id chunk boundaries from 70fd3c5774
4bd11d11e5 sync: manually port remote op locking from e7afe9fcf9
4426f2fb24 sync: manually port ollama model fetch tolerance from 4eb039b451
fd31f3a61c sync: manually port project panel markdown preview entry from db30c67ed2
ce410882b1 sync: manually port markdown preview context menu polish from a8ffae4c00
```

The branch log contains the full ordered list of local sync commits.

## Validation

Passed checks and targeted tests included:

```text
cargo check -p project_panel
cargo test -p project_panel test_expand_all_for_entry --lib -- --nocapture
cargo check -p project_panel -p workspace -p git_ui -p settings_ui -p markdown_preview -p recent_projects
cargo check -p workspace
cargo test -p workspace validate_trust_scope_accepts_remote_posix_paths --lib -- --nocapture
cargo test -p workspace validate_trust_scope_rejects_remote_non_ancestor_or_relative_paths --lib -- --nocapture
cargo check -p markdown
cargo test -p markdown test_clicking_loaded_image_inside_link_opens_link_url --lib
cargo test -p markdown test_clicking_image_fallback_ --lib
cargo check -p terminal
cargo test -p terminal test_init_command_startup_marker -- --nocapture
cargo test -p terminal write_init_command_after_startup -- --nocapture
cargo check -p settings_content -p project -p agent_servers -p acp_thread -p agent_ui
cargo test -p settings_content agent_config_option_value --lib
cargo test -p settings_content test_set_tool_default_permission --lib
```

Notes:

- Some earlier Windows test invocations timed out while compiling test
  artifacts for the first time. Later reruns of the directly relevant
  tests completed successfully.
- Warnings remained in unrelated local code such as
  `crates/remote_connection/src/remote_connection.rs`, but no new sync
  error remained in the touched areas.

## Working Tree State

At the end of the sync, the branch was clean apart from local untracked
helper files:

```text
.codex-candidate-list.txt
.codex-candidates-with-marks.json
.codex-git-cherry.txt
.codex-upstream-candidates.json
```
