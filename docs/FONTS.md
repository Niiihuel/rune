# Fuentes de la interfaz

La UI usa las fuentes embebidas por limn-egui, sin descargarlas en
ejecución. Ubuntu 0.83 aporta las caras Regular, Medium y Bold; Noto Sans
Math 3.000, Noto Sans Symbols 2 2.008 y Noto Emoji 3.002 son respaldos al
final de cada familia, así que nunca cambian lo que Ubuntu y las fuentes
predeterminadas de egui ya dibujan. Los emoji en color son Twemoji
([ARTWORK.md](ARTWORK.md#emoji-a-color-twemoji)); Noto Emoji queda para lo
que el paquete no dibuja y sigue dando el ancho de cada emoji.

Avisos, licencias completas, versiones, procedencia y SHA-256:
[assets/licenses/FONT-NOTICES.md](../assets/licenses/FONT-NOTICES.md) y
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), que cada artefacto
lleva (`rune --licenses`, [UPDATES.md](UPDATES.md)). Ubuntu tiene la Ubuntu
Font Licence 1.0; las Noto, SIL OFL-1.1. Se usan los archivos oficiales sin
modificar (Serein usa un subset propio de Symbols 2; no se copió).

## Qué cubre cada respaldo

- **Noto Sans Math**: el bloque alfanumérico matemático U+1D400–U+1D7FF
  (`𝖗𝖚𝖓𝖊`, `𝓡𝓤𝓝𝓔`), común en nombres de Discord.
- **Noto Sans Symbols 2**: geometría, flechas, estrellas y separadores
  decorativos (`◆●■▸⌜⟡✦❖➜⤷⣿`). Para el dibujo de cajas (`├─│╭╰║`) las
  familias proporcionales usan al final las monoespaciadas de egui.
- **Noto Emoji** (monocromo): todos los escalares del catálogo de emoji del
  selector tienen glifo en las cinco familias.

La consulta `has_glyph` de egui 0.36 da falsos negativos en la cara de
sustitución, por eso las pruebas de limn verifican las tablas de caracteres
de las fuentes configuradas y la resolución real de egui con muestras
sintéticas.

## Fuentes del sistema como respaldo

Hangul, kana, CJK y el relleno hangul «ㅤ» (U+3164) que mucha gente usa
como espacio en su nombre no están en ninguna fuente de RUNE (lo comprueba
`apps/desktop/src/fallback_fonts.rs`). Como un navegador, RUNE le pide a
fontconfig la fuente para lo que le falta: después de cada layout mira el
texto no ASCII que se pinta, y un carácter que ninguna cara dibuja (según
su `cmap`, leído con `skrifa`) se busca fuera del hilo de la interfaz con
`fc-match -s :charset=…` (`rune_platform::fonts::fallbacks`). La primera
cara OpenType que realmente lo dibuja se agrega **al final** de cada
familia. Límites: seis caras por sesión, 512 caracteres buscados, 32 MiB
por archivo; cada carácter se busca una sola vez. No corre en capturas ni
en el benchmark (dependería de cada máquina). Los caracteres de formato
(anchos cero, selectores, etiquetas) no se buscan.

## Límites

egui elige la fuente de un grafema por su primer carácter: si ya existe en
una cara anterior, un respaldo posterior no puede aportar la secuencia
completa. Por eso algunas banderas (`🇦🇷`), keycaps y secuencias ZWJ se
componen mal en el glifo monocromo (Twemoji las cubre en color cuando están
en su paquete). No hay cobertura Unicode completa ni composición tipográfica
general.
