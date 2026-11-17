# Reference: ZZZ Security Audit

Normative material for `auditing-security`. Load this before reconnaissance.
Keep code, JSON, and commands in English; keep report narrative in Simplified
Chinese.

## Contents

- Trust boundary model
- Verdicts and severity
- Coverage ledger
- Attack surfaces of ZZZ
- `findings.json` schema
- Report shape
- Anti-patterns
- Sources

## Trust boundary model

A candidate becomes a finding only when all six fields can be filled. Missing one
means the claim is not yet a security failure, whatever the code looks like.

| Field | Means | Example |
|---|---|---|
| `principal` | The lower-trust actor and its starting capability | A cloned project that has not been trusted |
| `input` | The accepted value, action, or state transition | A `SKILL.md` under `.agents/skills/` |
| `control` | The mechanism that should reject, bind, isolate, limit, or revoke | The worktree trust check before project skills load |
| `boundary` | The trust boundary that is crossed | Project content reaching the agent's system prompt |
| `affected` | The principal or resource that suffers | The user's tool permissions and file access |
| `result` | What is observed, or what an owner could observe | Untrusted instructions appear in the prompt |

Rules that follow from the model:

- A path, process name, display name, or sender field is not authentication.
  Only OS peer credentials, code identity, capability handles, or protected
  channel state are.
- Self-impact within the same principal and authority is not a boundary violation.
- A control that exists earlier in the chain still counts if it holds. The absence
  of a second layer is hardening, not a vulnerability.
- When the decisive control is outside the repository — a packaging entitlement, an
  OS sandbox profile, a CI secret, a provider setting — the record is
  `needs_validation` with that exact missing fact.

## Verdicts and severity

| Verdict | Has severity | Requires |
|---|---|---|
| `confirmed` | yes, P0/P1/P2 | six boundary fields, evidence, fix, independent verifier |
| `needs_validation` | no | five hypothesis fields (`principal`, `input`, `control`, `boundary`, `affected`) and the exact `validation_blocker` |
| `rejected` | no | `rejection_reason` naming the control that holds or the wrong step |

Severity uses the existing model, not a parallel scale:

| Level | Anchor in this repository |
|---|---|
| **P0** | Demonstrated cross-boundary effect with real consequences: code execution from project content, an extension, or a link payload; credential or session disclosure; bypass of the tool-permission or worktree-trust check; reachable memory unsafety; unauthorized file or network action. Blocks merge. |
| **P1** | A real boundary weakening with limits: an uncommon precondition, a narrow resource set, or a consequence confined to one surface. Fix in this change. |
| **P2** | Hardening: a missing defense whose absence no reachable path exploits today. Label it hardening; it is not a vulnerability. |

The P0/P1 discriminator: does the demonstrated result fully defeat an explicit
control for an action with real consequences, or only weaken it? If the concrete
damage cannot be stated, the level is lower than it feels.

## Coverage ledger

One unit is one surface crossed with one boundary and one attack class. The id is
`<surface>.<boundary>.<attack_class>` in lowercase dot-separated segments, for
example `agent.skills.trust`.

```json
{
  "run_id": "run-1",
  "units": [
    {
      "coverage_id": "agent.skills.trust",
      "surface": "agent skill loading",
      "boundary": "worktree trust",
      "attack_class": "untrusted project content",
      "state": "planned",
      "owner": null,
      "reason": null
    }
  ]
}
```

| State | Meaning | Extra field |
|---|---|---|
| `planned` | seeded, not assigned | none |
| `in_progress` | assigned to a hunter | `owner` |
| `covered` | hunted, nothing survived | `owner` |
| `candidate` | produced a candidate under validation | `owner` |
| `deferred` | in scope but not reached | `reason` |
| `blocked` | needs a fact the run cannot obtain | `reason` |
| `out_of_scope` | deliberately not audited | `reason` |

Assignment order: lowest-trust entry surfaces first, then boundaries guarding code
execution, credentials, and agent authority, then gaps from an earlier run, then
speculative classes. Ties break lexicographically so runs stay reproducible.

## Attack surfaces of ZZZ

Read the pointed files before trusting this list. A surface that does not exist in
the pinned source ref is not in scope.

### 1. Project trust and agent context

Untrusted actors: a cloned project, a shared `zzz://skill` link, a downloaded
config.

- Project skills load from `<worktree>/.agents/skills/`, and only from a trusted
  worktree (`docs/src/ai/skills.md`). Check the trust gate, not just the path.
- `AGENTS.md`, `.rules`, `CLAUDE.md`, and sibling instruction files enter the
  model's context. Check whether an untrusted project can inject instructions
  before the user reviews them.
- `zzz://skill` carries whole `SKILL.md` content in the link payload
  (`crates/zzz/src/zzz/open_listener.rs`). Check parsing, size limits, and whether
  installation asks for confirmation.

### 2. Extensions

Untrusted actor: an extension author, or a project that can suggest one.

- WASI modules plus a capability model: `crates/extension_host/src/capability_granter.rs`
  and `extension_settings.rs` track `granted_capabilities`, including
  `ExtensionCapability::ProcessExec`.
- Check that each grant is per operation, that arguments are normalized before the
  comparison, and that a granted capability cannot be reused for a different
  command.
- `crates/extension_api` exposes the host surface an extension calls.

### 3. Agent tools and model-driven actions

Untrusted actor: model output influenced by a file, a web page, an MCP tool
description, or a terminal response.

- Tool permission flow decides whether an edit, terminal command, or skill load
  runs. Check that a permission decision binds to the specific action rather than
  the tool name.
- `crates/acp_tools` and `crates/context_server` (MCP client, OAuth) carry the
  agent's external reach. Check token handling, loopback callbacks, and whether a
  tool description can redirect an action.
- Prompt injection is a real boundary here: remote text that reaches the model can
  select a tool. The result must be a concrete action the user did not authorize.

### 4. Local IPC and URL handling

- Single-instance and open requests arrive through the open listener; a local
  process or a crafted URL can reach it. Check argument parsing, path handling, and
  whether a remote caller can select a privileged action.
- `zzz://` handlers cover settings, git clone, extension install, and skill
  install. Check scheme, host, and payload validation per handler.

### 5. Remote development and credentials

- SSH remoting (`docs/src/remote-development.md`) and the embedded remote server.
  Check host-key and credential handling, and whether a remote peer can drive local
  file or tool actions beyond its grant.
- Credentials live in the OS keychain through `crates/credentials_provider`. Check
  that a failure path does not fall back to a file or an environment variable.

### 6. Telegram panel

- `crates/telegram` keeps an encrypted session on disk; `api_id` and `api_hash` are
  application credentials, not user secrets.
- Incoming message text is remote content routed into a UI panel
  (`crates/telegram_ui`). Check rendering, link handling, and whether message text
  can reach an agent tool.

### 7. Parsing surfaces

- Settings, keymap, and locale JSON: `crates/settings`, `assets/keymaps`,
  `assets/locales`.
- Tree-sitter grammars and vendored sources under `vendor/` and
  `crates/grammars/`.
- Preview pipelines: markdown/HTML, image, PDF, and CSV preview.
- For each, the question is whether a malformed input causes an out-of-bounds
  access, an unbounded allocation, or a privileged action, not merely an error.

### 8. Memory safety

- `unsafe` blocks concentrate in platform code (`crates/gpui/src/platform`) and FFI.
  Check the stated invariant, the lifetime of borrowed data, and whether the unsafe
  scope is wider than needed.
- Reachable unsafety from untrusted input is P0. An undocumented invariant that no
  untrusted path reaches is P1 at most.

### 9. Supply chain

- `Cargo.lock`, `vendor/`, grammar sources, extension packages, and the pinned
  toolchain. Check that a build cannot fetch at run time and that a vendored change
  is reviewable.

## `findings.json` schema

```json
{
  "run_id": "run-1",
  "repo": "ZZZ",
  "source_ref": { "commit": "<sha>", "dirty": false },
  "run_status": "complete",
  "incomplete_reason": null,
  "findings": [
    {
      "id": "SEC-1",
      "status": "confirmed",
      "title": "Project skills load before the worktree is trusted",
      "severity": "P0",
      "coverage_id": "agent.skills.trust",
      "found_by": "hunter-a",
      "verified_by": "verifier-b",
      "boundary": {
        "principal": "an untrusted cloned project",
        "input": "a SKILL.md under .agents/skills/",
        "control": "worktree trust check before project skills load",
        "boundary": "project content reaching the agent system prompt",
        "affected": "the user's tool permissions",
        "result": "untrusted instructions reach the prompt"
      },
      "evidence": [
        { "path": "crates/agent/src/tools.rs", "line": 20, "quote": "skill" }
      ],
      "fix": {
        "invariant": "project skills load only from a trusted worktree",
        "change": "gate the catalog read on the trust state"
      },
      "verification": {
        "verdict": "confirmed",
        "notes": "re-read the loader and the trust state it consults"
      }
    }
  ]
}
```

Rules the validator enforces:

- `status` is one of `confirmed`, `needs_validation`, `rejected`.
- `confirmed` carries P0/P1/P2, all six boundary fields, at least one evidence
  entry whose `path` exists in the repository, a `fix` with `invariant` and
  `change`, a `verification.verdict`, and `verified_by` different from `found_by`.
- `needs_validation` carries no severity, five hypothesis fields, and a
  `validation_blocker`.
- `rejected` carries no severity and a `rejection_reason`.
- `run_status: "incomplete"` requires `incomplete_reason`.
- Ids are `SEC-<n>` and unique.

## Report shape

```markdown
## 安全审计报告

**范围**: <surfaces and paths>
**源版本**: <commit>, dirty: <yes|no>
**Profile**: quick | standard
**覆盖**: <units by state>, out_of_scope: <list>
**验证器**: check-ledger PASS/FAIL, check-findings PASS/FAIL

### P0

1. `path/file.rs:123` — <标题>
   - 边界: <principal → input → control → boundary → affected → result>
   - 证据: <quote>
   - 最小修复: <invariant + change>

### P1

...

### 待验证（needs_validation，无严重度）

1. <hypothesis> — 缺少的事实: <blocker>

### 已排除（rejected）

1. <claim> — <which control holds>

### 本轮未覆盖

- <unit> — <why>

### 剩余最高风险

- <one>
```

## Anti-patterns

- A checklist deviation presented as a vulnerability.
- A defense-in-depth gap described as exploitable when the earlier layer holds.
- Live or shared-environment probing where source evidence was enough.
- Guessing a packaging, OS, proxy, or provider behavior that the repository does
  not contain.
- Treating same-principal self-impact as a cross-boundary result.
- A parser error or crash reported as something stronger than what was observed.
- Prose-only findings that cannot be deduplicated, verified, or re-read later.
- A finding whose recommendation restores telemetry, accounts, billing, hosted AI,
  auto-update, or a default network call. The absence boundary outranks it.
- Severity attached to a hypothesis.
- A clean-coverage claim from one run, or a report written before verification.

## Sources

- Orchestration adapted from Cloudflare's `security-audit-skill`
  (https://github.com/cloudflare/security-audit-skill, MIT). Six phases collapsed
  to five, the OS-sandbox and artifact-promotion protocol replaced with
  source-only execution against this repository, its Node validators replaced by
  `scripts/security_audit.py`, and its severity scale replaced with the existing
  P0/P1/P2 model.
- Repository: `hunting-code-smells/REFERENCE.md` (severity model),
  `.rules`, `AGENTS.md`, `docs/src/ai/skills.md`, `docs/src/remote-development.md`,
  `script/check-philosophy`, `crates/extension_host/src/capability_granter.rs`,
  `crates/zzz/src/zzz/open_listener.rs`, `crates/telegram/src/credentials.rs`.
