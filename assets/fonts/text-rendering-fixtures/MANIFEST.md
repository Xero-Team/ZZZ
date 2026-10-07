# Text rendering fixture manifest

These fonts are test-only subsets for `TEXT-007`. They are generated without
network access by `script/build-text-rendering-fixtures` and contain only the
glyphs needed to distinguish COLRv1, bitmap color, OpenType-SVG, and monochrome
raster paths.

Generation environment: HarfBuzz 14.5.1, 2026-10-07.

## COLRv1

- File: `noto-colrv1-grinning-face.ttf`
- Character: U+1F600 GRINNING FACE
- Tables under test: `COLR`, `CPAL`
- Source: Noto Color Emoji 2.057,
  `google-noto-color-emoji-fonts-2.057-1.fc46.noarch`
- Upstream: <https://github.com/googlefonts/noto-emoji>
- Source SHA-256:
  `b8e25ea68db82f9e4d0aee921f4420be2be39887bd5c893a2ad98710531f9d0c`
- Fixture SHA-256:
  `5283bd5533adb3c0b1da62df4e646791232a7c919d704d420122092a3cab4a55`
- License: SIL Open Font License 1.1, copied in `OFL-1.1.txt`

## Bitmap color

- File: `noto-color-emoji-bitmap-grinning-face.ttf`
- Character: U+1F600 GRINNING FACE
- Tables under test: `CBDT`, `CBLC`
- Source repository: <https://github.com/gpui-ce/gpui-ce>
- Source commit: `c6b17e616a35271183ab49f0da1890ee81953a99`
- Source path: `assets/fonts/noto-color-emoji/NotoColorEmoji.subset.ttf`
- Source SHA-256:
  `62ef43fae1c92fd7c820406219eb96f90b56e6f4ee4955b8c1f1ef4e90fd7e32`
- Fixture SHA-256:
  `b3242069a2b5e35da59ed528d1ebac83f14add2624448b07a8dff9069d2185c6`
- License: SIL Open Font License 1.1, copied in `OFL-1.1.txt`

## OpenType-SVG

- File: `twitter-color-emoji-svg-rocket.ttf`
- Character: U+1F680 ROCKET
- Table under test: `SVG `
- Source repository: <https://github.com/avaloniaui/avalonia>
- Source commit: `17350180c33b063f0e98abbfd19aa3cae63f5d56`
- Source path:
  `tests/Avalonia.RenderTests/Assets/TwitterColorEmoji-SVGinOT.ttf`
- Original project: <https://github.com/eosrei/twemoji-color-font>
- Source SHA-256:
  `178846e7886c51ac2afa6094317f285ec0366cd147fe114279ea0f51a7db8acc`
- Fixture SHA-256:
  `773eea8086a08eb971ce9c69ee3ef858c04a5a7d658a69e70a48c37c9f15f676`
- Attribution: Copyright 2019 Brad Erickson; Twitter Emoji artwork copyright
  2019 Twitter, Inc.
- License: Creative Commons Attribution 4.0 International, copied in
  `CC-BY-4.0.txt`

## Monochrome control

`TEXT-007` uses the existing `assets/fonts/openmoji/openmoji.ttf` fixture for
U+1F600. Its SHA-256 is
`b0c48711651c0502956692beb2e130ae2fc9b0df3b01febf758b3aeb41501dc4` and its
Creative Commons Attribution-ShareAlike 4.0 license is in
`assets/fonts/openmoji/LICENSE`.

## Reproduction

```sh
script/build-text-rendering-fixtures \
  /path/to/Noto-COLRv1.ttf \
  /path/to/NotoColorEmoji.subset.ttf \
  /path/to/TwitterColorEmoji-SVGinOT.ttf
```

The script uses `hb-subset` for all three fonts. HarfBuzz does not subset the
OpenType `SVG ` table, so the script extracts the single source SVG document,
attaches it to the renumbered subset glyph, and recomputes all sfnt checksums.
