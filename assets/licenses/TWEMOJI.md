# Twemoji artwork

Twemoji graphics by Twitter, Inc. and other contributors, maintained by the
[jdecked/twemoji](https://github.com/jdecked/twemoji) project, licensed under
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/).
The full license, as shipped in the release, is
[Twemoji-CC-BY-4.0.txt](Twemoji-CC-BY-4.0.txt). Twemoji's code (MIT) is not
included.

Source: release [v17.0.3](https://github.com/jdecked/twemoji/releases/tag/v17.0.3),
commit `b6b55fef1e8636b540a6d016a4729ca8cdf2e60b`, archive
`https://github.com/jdecked/twemoji/archive/b6b55fef1e8636b540a6d016a4729ca8cdf2e60b.tar.gz`,
SHA-256 `705d79de1460e5e775f362f0d0f01fbe3ef8d65bf4648c490e4649704584f747`,
retrieved 2026-10-16.

`../twemoji/pngs.bin` holds all 4,009 upstream `assets/72x72/*.png` files,
byte-for-byte unmodified, back to back. Changes: none to the pixels; they are
only packed together and indexed. `../twemoji/index.tsv` lists each sequence
(hexadecimal code points without U+FE0F, joined by `-`), its byte offset and
length in the pack, and `t` when the sequence is a single code point whose
default presentation is text, else `e`. That flag is derived from Unicode's
[emoji-data.txt 17.0.0](https://www.unicode.org/Public/17.0.0/ucd/emoji/emoji-data.txt)
(SHA-256 `2cb2bb9455cda83e8481541ecf5b6dfda66a3bb89efa3fa7c5297eccf607b72b`,
[Unicode License v3](https://www.unicode.org/license.txt)).

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `pngs.bin` | 4,248,486 | `aaf2f3482e3070e3137b6c13ffbb7492d8e86800618bd55658c494dae9644711` |
| `index.tsv` | 120,898 | `edc82cbc199dd5851a5df85dec7e352c9c1f382b25e87a12c674a087c2ab8f7f` |

Rebuild from the repository root with `python3 -I scripts/twemoji.py`
(standard library only; it verifies both downloads before reading them and
produces the same bytes). Pass `--archive` and `--emoji-data` to build offline.
