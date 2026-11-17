# Documentation Automation Agent Guidelines

This file governs automated documentation updates triggered by code changes. All automation phases must comply with these rules.

## Documentation System

This documentation uses **mdBook** (https://rust-lang.github.io/mdBook/).

### Key Files

- **`docs/src/SUMMARY.md`**: Table of contents following mdBook format (https://rust-lang.github.io/mdBook/format/summary.html)
- **`docs/book.toml`**: mdBook configuration
- **`docs/.prettierrc`**: Prettier config (80 char line width)

### SUMMARY.md Format

The `SUMMARY.md` file defines the book structure. Format rules:

- Chapter titles are links: `[Title](./path/to/file.md)`
- Nesting via indentation (2 spaces per level)
- Separators: `---` for horizontal rules between sections
- Draft chapters: `[Title]()` (empty parens, not yet written)

Example:

```markdown
# Section Title

- [Chapter](./chapter.md)
  - [Nested Chapter](./nested.md)

---

# Another Section
```

### Custom Preprocessor

The docs use a custom preprocessor (`docs_preprocessor`) that expands special commands:

| Syntax                        | Purpose                               | Example                         |
| ----------------------------- | ------------------------------------- | ------------------------------- |
| `{#kb action::ActionName}`    | Keybinding for action                 | `{#kb agent::ToggleFocus}`      |
| `{#action agent::ActionName}` | Action reference (renders as command) | `{#action agent::OpenSettings}` |

**Rules:**

- Always use preprocessor syntax for keybindings instead of hardcoding
- Action names use `snake_case` in the namespace, `PascalCase` for the action
- Common namespaces: `agent::`, `editor::`, `assistant::`, `vim::`

### Formatting Requirements

All documentation must pass **Prettier** formatting. `cargo xtask check docs`
runs the same check in CI, so a page that fails it blocks the gate:

```sh
script/prettier          # check
script/prettier --write  # fix
```

Prettier config: 80 character line width (`docs/.prettierrc`)

### Section Anchors

Use `{#anchor-id}` syntax for linkable section headers:

```markdown
## Getting Started {#getting-started}

### Custom Models {#anthropic-custom-models}
```

Anchor IDs should be:

- Lowercase with hyphens
- Unique within the page
- Descriptive (can include parent context like `anthropic-custom-models`)

### Code Block Annotations

Use annotations after the language identifier to indicate file context:

```markdown
\`\`\`json [settings]
{
"agent": { ... }
}
\`\`\`

\`\`\`json [keymap]
[
{ "bindings": { ... } }
]
\`\`\`
```

Valid annotations: `[settings]` (for settings.json), `[keymap]` (for keymap.json)

### Blockquote Formatting

Use bold labels for callouts:

```markdown
> **Note:** Important information the user should know.

> **Tip:** Helpful advice that saves time or improves workflow.

> **Warn:** Caution about potential issues or gotchas.
```

### Image References

Do not hotlink `https://zed.dev/img/...` or `https://images.zed.dev/...`.
Use an in-repo asset, or omit the image.

### Cross-Linking

- Relative links for same-directory: `[Agent Panel](./agent-panel.md)`
- With anchors: `[Custom Models](./llm-providers.md#anthropic-custom-models)`
- Parent directory: `[Telemetry](../telemetry.md)`

## Voice and Tone

`docs/.rules` is the authority for voice, formatting, and terminology in
user-facing documentation. It already lists the principles, the phrases to
avoid, worked examples of good and bad copy, and the terminology table, so
they are not repeated here. Read it before writing or rewriting a page.

## Scope

### In-Scope Documentation

- All Markdown files in `docs/src/`
- `docs/src/SUMMARY.md` (mdBook table of contents)
- Language-specific docs in `docs/src/languages/`
- Feature docs (AI, extensions, configuration, etc.)

### Out-of-Scope (Do Not Modify)

- `CHANGELOG.md`, `CONTRIBUTING.md`, `README.md` at repo root
- Inline code comments and rustdoc
- `CLAUDE.md`, `GEMINI.md`, or other AI instruction files
- Build configuration (`book.toml`, theme files, `docs_preprocessor`)
- Any file outside `docs/src/`

## Page Structure Patterns

### Standard Page Layout

Most documentation pages follow this structure:

1. **Title** (H1) - Single sentence or phrase
2. **Overview/Introduction** - 1-3 paragraphs explaining what this is
3. **Getting Started** `{#getting-started}` - Prerequisites and first steps
4. **Main Content** - Feature details, organized by topic
5. **Advanced/Configuration** - Power user options
6. **See Also** (optional) - Related documentation links

### Settings Documentation Pattern

When documenting settings:

1. Show the Settings Editor (UI) approach first
2. Then show JSON as "Or add this to your settings.json:"
3. Always show complete, valid JSON with surrounding structure:

```json [settings]
{
  "agent": {
    "default_model": {
      "provider": "anthropic",
      "model": "claude-sonnet-4"
    }
  }
}
```

### Provider/Feature Documentation Pattern

For each provider or distinct feature:

1. H3 heading with anchor: `### Provider Name {#provider-name}`
2. Brief description (1-2 sentences)
3. Setup steps (numbered list)
4. Configuration example (JSON code block)
5. Custom models section if applicable: `#### Custom Models {#provider-custom-models}`

## Style Rules

Inherit every convention from `docs/.rules`: voice, formatting, terminology,
and the worked examples. Do not restate those rules in this file or in a page;
link to `docs/.rules` instead, so a change lands in one place.

## ZZZ-Specific Conventions

### Recognized Rules Files

When documenting rules/instructions for AI, note that ZZZ recognizes these files (in priority order):

- `.rules`
- `.cursorrules`
- `.windsurfrules`
- `.clinerules`
- `.github/copilot-instructions.md`
- `AGENT.md`
- `AGENTS.md`
- `CLAUDE.md`
- `GEMINI.md`

### Settings File Locations

- macOS: `~/.config/zzz/settings.json`
- Linux: `~/.config/zzz/settings.json`
- Windows: `%AppData%\ZZZ\settings.json`

### Keymap File Locations

- macOS: `~/.config/zzz/keymap.json`
- Linux: `~/.config/zzz/keymap.json`
- Windows: `%AppData%\ZZZ\keymap.json`

## Safety Constraints

### Must Not

- Delete existing documentation files
- Remove sections documenting existing functionality
- Change URLs or anchor links without verifying references
- Modify `SUMMARY.md` structure without corresponding content
- Add speculative documentation for unreleased features
- Include internal implementation details not relevant to users

### Must

- Preserve existing structure when updating content
- Maintain backward compatibility of documented settings/commands
- Flag uncertainty explicitly rather than guessing
- Link to related documentation when adding new sections

## Change Classification

### Requires Documentation Update

- New user-facing features or commands
- Changed keybindings or default behaviors
- Modified settings schema or options
- Deprecated or removed functionality
- API changes affecting extensions

### Does Not Require Documentation Update

- Internal refactoring without behavioral changes
- Performance optimizations (unless user-visible)
- Bug fixes that restore documented behavior
- Test changes
- CI/CD changes

## Output Format

### Phase 4 Documentation Plan

When generating a documentation plan, use this structure:

```markdown
## Documentation Impact Assessment

### Summary

Brief description of code changes analyzed.

### Documentation Updates Required: [Yes/No]

### Planned Changes

#### 1. [File Path]

- **Section**: [Section name or "New section"]
- **Change Type**: [Update/Add/Deprecate]
- **Reason**: Why this change is needed
- **Description**: What will be added/modified

#### 2. [File Path]

...

### Uncertainty Flags

- [ ] [Description of any assumptions or areas needing confirmation]

### No Changes Needed

- [List files reviewed but not requiring updates, with brief reason]
```

### Phase 6 Summary Format

```markdown
## Documentation Update Summary

### Changes Made

| File           | Change            | Related Code      |
| -------------- | ----------------- | ----------------- |
| path/to/doc.md | Brief description | link to PR/commit |

### Rationale

Brief explanation of why these updates were made.

### Review Notes

Any items reviewers should pay special attention to.
```

## Behavioral Guidelines

### Conservative by Default

- When uncertain whether to document something, flag it for human review
- Prefer smaller, focused updates over broad rewrites
- Do not "improve" documentation unrelated to the triggering code change

### Traceability

- Every documentation change should trace to a specific code change
- Include references to relevant commits, PRs, or issues in summaries

### Incremental Updates

- Update existing sections rather than creating parallel documentation
- Maintain consistency with surrounding content
- Follow the established patterns in each documentation area
