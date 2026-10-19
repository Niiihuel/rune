# Fuentes de la interfaz

La UI principal usa las fuentes embebidas por limn-egui, sin descargarlas
en ejecución. Ubuntu 0.83 aporta las caras Regular, Medium y Bold. Noto
Sans Math 3.000, Noto Sans Symbols 2 2.008 y Noto Emoji 3.002 amplían las fuentes de respaldo,
conservando la prioridad de Ubuntu y de las fuentes predeterminadas de egui.

Los avisos de copyright, licencias completas, versiones, procedencia fijada
y SHA-256 de estas seis fuentes están en
[assets/licenses/FONT-NOTICES.md](../assets/licenses/FONT-NOTICES.md).
Esos avisos, y los de las fuentes predeterminadas de egui, están también
en [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), que cada artefacto
lleva (`rune --licenses`, [UPDATES.md](UPDATES.md#avisos-de-terceros)).

## Cobertura comprobada

El [PR #38 de limn](https://github.com/Niiihuel/limn/pull/38), commit
`e9322fa8e8f4d33bff9b5a6629448dc8b5fc9a49`, añade el respaldo matemático.
Antes del cambio, la prueba de búsqueda real de glifos de egui fallaba para
`𝖗` (U+1D597). Después pasan las muestras sintéticas `𝖗𝖚𝖓𝖊` y `𝓡𝓤𝓝𝓔`,
los caracteres alfanuméricos asignados del bloque U+1D400–U+1D7FF y símbolos
alfabéticos matemáticos habituales, en las cinco familias de limn. Se
verificaron también capturas nativas de la galería en claro y oscuro.
Estas pruebas no usan ni acreditan la representación de un nombre de cuenta
real. El cambio no añade cobertura general de CJK, emoji en color ni un
nuevo sistema de composición de texto.

La licencia de Ubuntu es Ubuntu Font Licence 1.0; las de las fuentes Noto son
SIL OFL-1.1, separadas de las licencias del código Rust. Las fuentes
predeterminadas de egui (`epaint_default_fonts`: Hack, Ubuntu Light, Noto
Emoji y emoji-icon-font) y el resto de las dependencias tienen sus avisos en
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) desde el 2026-10-19.

## Símbolos decorativos

El [PR #40 de limn](https://github.com/Niiihuel/limn/pull/40), commit
`f9bc36b804a41050ca9c6d7193aec78ea607e7df`, añade Symbols 2 como fallback después de
Math en todas las familias. El OTF oficial, sin modificar, añade 404.576
bytes; no hay descarga durante el uso de RUNE. Además, las familias
proporcionales aprovechan al final las fuentes monospace que egui ya
incluye para símbolos de dibujo de cajas como `├─│╭╰║`, sin otro recurso.

La prueba usa exclusivamente una muestra sintética de geometría, flechas,
estrellas y separadores: `◆◇●○■□▪▫▸▹▾▿⌜⌟├─│╭╰═║⟡✦✧❖➜→←⇢➤⤷⎯⏤⠿⣿`.
Verifica las tablas de caracteres de las fuentes realmente configuradas,
además de la resolución de `✦` por egui, que falta en la cadena anterior.
La consulta `has_glyph` de egui 0.36 compara caras y devuelve falsos negativos
para caracteres válidos en la cara de sustitución; por eso no se usa como
única evidencia de cobertura. Las métricas de texto latino y monospace
siguen comprobándose con pruebas separadas.

Validación del PR: 46 pruebas de limn-egui, 31 de galería y un doctest
pasaron, junto con Clippy de ambos paquetes para todos los targets con
warnings como errores. Se inspeccionaron capturas nativas de las muestras
en claro y oscuro. Se integró en la rama local `rune/integration`; el PR
público permanece abierto. La actualización del pin y la validación del
workspace de RUNE corresponden a la integración de esta iteración.

La referencia estática fue `references/serein/assets/README.md` y
`crates/ui/src/fonts.rs`, commit `d1af883c9d7dda90d1261a2d15fc852a5445204d`:
Serein también añade Symbols 2, pero usa un subset propio. Aquí se emplea
la release oficial íntegra; no se copió código ni el subset de Serein.
No se leyeron nombres reales de cuenta ni se identificaron los caracteres
exactos de la captura del usuario. Tampoco se promete cobertura Unicode
completa, emoji en color ni composición tipográfica general.

## Emoji monocromos

Desde el 2026-10-16 los emoji del paquete Twemoji se pintan en color encima
de estos glifos ([ARTWORK.md](ARTWORK.md#emoji-a-color-twemoji)); Noto Emoji
queda como respaldo para lo que el paquete no dibuja y sigue dando el ancho
de cada emoji en la línea.

El [PR #41 de limn](https://github.com/Niiihuel/limn/pull/41), commit
`4e03ab0b9a6b716b71aa347a2c9d32ee41384bf0`, añade Noto Emoji 3.002 al
final de todas las familias. La fuente variable oficial ocupa 1.982.596
bytes, se usa sin modificar y conserva OFL-1.1 con su fuente fijada y
hashes en los avisos. Las caras anteriores mantienen su prioridad; los
emoji nuevos usan las medidas de Noto, también en texto monospace.

La prueba usa una copia de las 623 cadenas Unicode del catálogo del picker,
sin datos de cuenta ni recursos de terceros. La cadena proporcional anterior
carecía de 124 tipos de caracteres de esa muestra. Con el respaldo, todos
los escalares de dibujo del catálogo tienen una entrada válida en alguna
fuente configurada, en las cinco familias; ZWJ y el selector de variante
se comprueban mediante secuencias, no como glifos aislados. La resolución
real de egui para `🙂🤣🦀🫡` y las métricas previas tienen pruebas separadas.

La cobertura de caracteres no garantiza la composición de cada secuencia.
Las pruebas de layout confirman un glifo compuesto para `❤️`, `☀️`,
`🧑‍💻` y `🏴‍☠️`, incluida la comparación con el carácter base para evitar
confundir una composición con un sufijo descartado. egui 0.36 elige la
fuente del grafema a partir de su primer carácter: si ya existe en una
cara anterior, el nuevo respaldo no puede aportar la ligadura completa.
En las capturas sintéticas, `🇦🇷` conserva dos letras, `#️⃣` se ve como `#`
y `🏳️‍🌈` como una bandera simple. El color y la composición completa de
esas secuencias quedan pendientes; el fallback no reemplaza las fuentes
previas para ocultar esa limitación.

Validación del PR: 49 tests de limn-egui, 31 de galería y un doctest verdes,
más Clippy de ambos paquetes para todos los targets con warnings como
errores. Se inspeccionaron capturas nativas claras/oscuras de la galería
con las cinco familias. El PR está apilado sobre #40 y se fusionó sólo en
la rama local `rune/integration`, commit
`2618b37d1c17b73ef70c43e67f5641ec4da41a3a`. El pin y las capturas de RUNE
se validan por separado en la integración; estos resultados corresponden
a limn.

## Fuentes del sistema como respaldo (2026-10-17)

La primera prueba con una cuenta real mostró cuadraditos en etiquetas de
servidor y nombres (por ejemplo, después del nombre de una persona y en
un autor «{ □ }< …»). No se pudo saber qué caracteres eran (sólo había
capturas), pero los candidatos comunes en Discord no están en ninguna
fuente de RUNE: hangul, kana y CJK, y el relleno hangul «ㅤ» (U+3164) que
mucha gente usa como espacio en su nombre. La prueba de
`apps/desktop/src/fallback_fonts.rs` lo confirma para «ㅤ» y «가».

Como un navegador, RUNE ahora le pide a fontconfig la fuente para lo que
le falta: después de cada layout mira el texto que se pinta (sólo lo que
no es ASCII), y un carácter que ninguna cara instalada dibuja (según su
`cmap`, leído con `skrifa`, la misma biblioteca de egui) se busca fuera
del hilo de la interfaz con `fc-match -s :charset=…`
(`rune_platform::fonts::fallbacks`). La primera cara OpenType de la lista
que realmente lo dibuja se agrega **al final** de cada familia, así que
nada de lo que RUNE ya dibujaba cambia. Límites: como mucho seis caras
por sesión, 512 caracteres buscados, cada archivo hasta 32 MiB (el de
`rune_platform::fonts`). En esta máquina, «ㅤ», «가» y «★» los resuelve
Noto Sans CJK (31 MB en memoria sólo si aparece uno de ellos).

No corre en capturas ni en el benchmark (dependería de las fuentes de cada
máquina). Los caracteres de formato (anchos cero, selectores de variante,
etiquetas) no se buscan. Si el sistema no tiene ninguna fuente para un
carácter, sigue el cuadradito; se busca una sola vez por sesión.

Pendiente: probarlo con la cuenta real (qué caracteres eran y si se ven);
egui elige la fuente de un grafema por su primer carácter, así que una
secuencia combinada que empieza en una cara anterior no toma la nueva.

