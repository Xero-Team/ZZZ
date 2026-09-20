# Vendored tree-sitter TOML grammar

Local fork of `tree-sitter-toml-ng`.

## Why vendored

Patched for TOML 1.1 support gaps needed by ZZZ:

- offset datetime with space delimiter, e.g. `1979-05-27 07:32Z`
- local datetime without seconds, e.g. `1979-05-27T07:32`
- local time without seconds, e.g. `07:32`
- inline table trailing comma, e.g. `{ x = 1, y = 2, }`

## Main patch points

- `grammar.js`
  - `rfc3339_time`
  - `inline_table`

## Regenerate parser artifacts

From this directory run:

```sh
npx tree-sitter-cli generate
```

This regenerates:

- `src/parser.c`
- `src/grammar.json`
- `src/node-types.json`

## Related workspace wiring

- workspace dependency: `/Cargo.toml`
- ZZZ TOML queries/config: `/crates/grammars/src/toml/`
