# ZZZ

> Zedless, Zeroed, Zen. — Zed, without the noise.

ZZZ is a community fork of [Zed](https://github.com/zed-industries/zed),
a high-performance code editor originally built by the creators of
[Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

This fork exists because some things should not be configurable — they should simply be absent.

---

## Philosophy

> _无 Zed 之 Zed，是为真 Zed。_
> In English: Zed without Zed is true Zed.
>
> Strip away the telemetry, the upsells, the proprietary coupling —
> what remains is the editor.

---

## How ZZZ differs from upstream Zed

|                       | Upstream Zed                    | ZZZ                                    |
| --------------------- | ------------------------------- | -------------------------------------- |
| Telemetry             | Opt-out                         | Removed in source code                 |
| AI service promotion  | upsells                         | None                                   |
| Agent protocol        | Zed Agent (proprietary) and ACP | ACP Only                               |
| Commercial API        | promoted                        | Available, manually configured, silent |
| Contributor agreement | CLA required                    | No CLA — you keep your copyright       |

---

## How ZZZ differs from other Zed forks

[Gram](https://codeberg.org/GramEditor/gram) removes AI entirely — a valid and principled choice.
[Zedless](https://github.com/zedless-editor/zedless) takes a similar privacy-first approach.

ZZZ takes a different position: **AI features can stay, but they default to your own infrastructure.**
The editor ships pointed at a local endpoint. No account, no cloud, nowhere asking you to sign up.
If you want a commercial provider, you can add it yourself — quietly. _Actually, you can even use Zed AI if you want._

---

## Installation

ZZZ does not provide pre-built binaries yet. Build from source:

```sh
cargo run
```

See upstream build guides for system dependencies:
[macOS](./docs/src/development/macos.md) ·
[Linux](./docs/src/development/linux.md) ·
[Windows](./docs/src/development/windows.md)

To use an AI provider (OpenAI, Anthropic, Zed AI, etc.), add the API key manually in settings.

---

## Contributing

No CLA. No copyright assignment.

Contributions are accepted under the
[Developer Certificate of Origin (DCO)](https://developercertificate.org/).
Sign your commits with `git commit -s` and you're done.

See [CONTRIBUTING.md](./CONTRIBUTING.md) for the full workflow.

---

## Upstream Sync

ZZZ tracks the upstream Zed `main` branch.

If a patch conflicts with upstream, opening an issue or PR is welcome.

---

## Acknowledgements

[Gram](https://codeberg.org/GramEditor/gram) proved that a Zed fork built
on genuine principles — not just preferences — is worth doing.
This project would not exist without that precedent.

---

## Sponsoring

If you’d like to support the project, please give it a star. : )

---

## Licensing

ZZZ inherits Zed's license structure.
See [LICENSE-GPL](./LICENSE-GPL) and [LICENSE-APACHE](./LICENSE-APACHE).

The original Zed README is preserved at [README.ORIGINAL.md](./README.ORIGINAL.md).
