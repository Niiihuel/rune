# Avisos de las fuentes añadidas por limn-egui

Estos avisos y los cuatro archivos de licencia de este directorio acompañan
las fuentes embebidas por `limn-egui::install_fonts`. Deben conservarse al
distribuir RUNE con esas fuentes. Los archivos TTF/OTF siguen en la dependencia
limn; no se duplican en este repositorio.

## Ubuntu Regular, Medium y Bold 0.83

Aviso incluido en la tabla `name` de los tres archivos TTF:

> Copyright 2011 Canonical Ltd.  Licensed under the Ubuntu Font Licence 1.0

Licencia completa: [Ubuntu-UFL.txt](Ubuntu-UFL.txt).

Fuente de distribución:
[Google Fonts, commit a50de97857f6626d6628c91e6a34c12ee8a16ede](https://github.com/google/fonts/tree/a50de97857f6626d6628c91e6a34c12ee8a16ede/ufl/ubuntu).
Los archivos se usan sin modificar.

| Archivo | SHA-256 |
| --- | --- |
| Ubuntu-Regular.ttf | `3128df86a31805618436d0ae5651ba4285d0c9de0a39057d025f64ee33bceb64` |
| Ubuntu-Medium.ttf | `3cd952b8b52581e48a8fa95b31c8829c2baa41b635042a69587d57f98092961e` |
| Ubuntu-Bold.ttf | `679b5c1e09cab3156bb8ef529735f9382bf31ca7ac737382ab959297f8d82ad4` |
| Ubuntu-UFL.txt | `2f0015108d68627bd788d313f529c21ff4da2c2c42a5e1f3883acc83480f9002` |

## Noto Sans Math 3.000

> Copyright 2022 The Noto Project Authors (https://github.com/notofonts/math)

Licencia completa, SIL Open Font License 1.1:
[NotoSansMath-OFL.txt](NotoSansMath-OFL.txt).

Proyecto: [notofonts/math](https://github.com/notofonts/math).
Fuente de distribución:
[Google Fonts, commit 823468bd7825152bce2b8fd2cf740432ad2fce8d](https://github.com/google/fonts/tree/823468bd7825152bce2b8fd2cf740432ad2fce8d/ofl/notosansmath).

El TTF se usa sin modificar, ocupa 1.015.396 bytes y su tabla `name` indica
`Version 3.000; ttfautohint (v1.8.4.7-5d5b)`. El texto de licencia está
completo; la copia de limn sólo recorta un espacio al final de una línea.

| Archivo | SHA-256 |
| --- | --- |
| NotoSansMath-Regular.ttf | `3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d` |
| NotoSansMath-OFL.txt | `5b18c06210e575a638ec293b5c824a4e1084a7ec5a3d67098b15dae51d2df24b` |

## Alcance de estos avisos

Este inventario cubre las fuentes que limn añade explícitamente. Las
fuentes predeterminadas de `epaint_default_fonts` y todas las dependencias
Rust tienen sus avisos en [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md),
que cada artefacto lleva. No cubre las fuentes del sistema que pueda usar el
auxiliar de login ni las bibliotecas nativas que pone la distribución.
Ver [SECURITY.md](../../docs/SECURITY.md#procedencia-de-lo-que-rune-trae).

## Noto Sans Symbols 2 2.008

> Copyright 2022 The Noto Project Authors (https://github.com/notofonts/symbols)

Licencia completa, SIL Open Font License 1.1:
[NotoSansSymbols2-OFL.txt](NotoSansSymbols2-OFL.txt).

Fuente: [release oficial NotoSansSymbols2-v2.008](https://github.com/notofonts/symbols/releases/tag/NotoSansSymbols2-v2.008),
archivo `NotoSansSymbols2-v2.008.zip`, miembro
`NotoSansSymbols2/unhinted/otf/NotoSansSymbols2-Regular.otf`.
Se usa sin modificar ni recortar. Ocupa 404.576 bytes y su tabla `name`
indica `Version 2.008`. La licencia conserva el texto completo de la release;
se recorta un espacio al final de una línea, como en limn.

| Archivo | SHA-256 |
| --- | --- |
| Release ZIP | `346c930bbe8eb946701a05c54e9c11a2094dee1d93c387bf1771c0a3e335688f` |
| NotoSansSymbols2-Regular.otf | `9b0f186fea27f311018800fb3a01d44d59331dbcccf518d2dadcae894eb017c0` |
| NotoSansSymbols2-OFL.txt | `e87c2ed7ff174c637d55fa381939ebb96f43f0415ad94605a37589228f4cbf4f` |

## Noto Emoji 3.002

> Copyright 2013 Google LLC

Licencia completa, SIL Open Font License 1.1:
[NotoEmoji-OFL.txt](NotoEmoji-OFL.txt).

Proyecto: [googlefonts/noto-emoji](https://github.com/googlefonts/noto-emoji).
Fuente de distribución:
[Google Fonts, commit 8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5](https://github.com/google/fonts/tree/8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5/ofl/notoemoji).

El archivo original `NotoEmoji[wght].ttf` se distribuye como `NotoEmoji.ttf`
con los mismos bytes. Es una fuente variable monocroma, ocupa 1.982.596
bytes y su tabla `name` indica `Version 3.002`. No se recortó ni convirtió;
no se descarga en ejecución. La licencia conserva su texto completo, con
un espacio final de línea recortado como en limn.

| Archivo | SHA-256 |
| --- | --- |
| NotoEmoji.ttf | `de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551` |
| NotoEmoji-OFL.txt | `79d348cb5161481f603dcda564fb129a9b2c11d7012dcad820af045fed75c020` |
