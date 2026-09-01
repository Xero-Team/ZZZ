# ZZZ Extensions

This directory contains extensions for ZZZ that are largely maintained in
this repository. They currently live here for ease of maintenance.

If you are looking for the public extension registry used by the
user-initiated gallery, see [`zed-industries/extensions`](https://github.com/zed-industries/extensions).
The gallery may contact `https://api.zed.dev` when you request an
install.

## Structure

Currently, ZZZ includes support for a number of languages without
requiring installing an extension. Those languages can be found under
[`crates/languages/src`](../crates/languages/src).

Support for all other languages is done via extensions. This directory
([`extensions/`](./)) contains some of the officially maintained
extensions. These extensions use the same
[zed_extension_api](https://docs.rs/zed_extension_api/latest/zed_extension_api/)
available to other extensions for providing
[language servers](../docs/src/extensions/languages.md#language-servers),
[tree-sitter grammars](../docs/src/extensions/languages.md#grammar) and
[tree-sitter queries](../docs/src/extensions/languages.md#tree-sitter-queries).

You can find other maintained extensions in the
[zed-extensions organization](https://github.com/zed-extensions).

## Dev Extensions

See [Developing an Extension Locally](../docs/src/extensions/developing-extensions.md#developing-an-extension-locally)
for how to work with one of these extensions.
