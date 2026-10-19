# Imágenes y expresiones

## Recursos del proveedor

`RemoteImage` es una referencia de dominio independiente del servicio. Los
usuarios llevan un avatar opcional, los servidores un icono opcional y los
catálogos de expresiones sus miniaturas. El adaptador traduce identificadores
y hashes válidos a rutas públicas del CDN y conserva las URLs de adjuntos que
recibe del servicio. La UI no construye esas rutas ni recibe credenciales de
cuenta. Las URLs de adjuntos pueden llevar firmas temporales de acceso;
`RemoteImage` y `RemoteFile` las ocultan en `Debug`. Los campos nuevos de mensajes, usuarios y servidores
persistidos llevan `serde(default)`: no cambian las tablas de SQLite.

Después del layout, `rune-ui::wanted_images` enumera recursos visibles: riel,
conversaciones, miembros, perfil, filas del timeline y celdas del selector.
`rune-core::Images` mantiene una caché en RAM y despierta la ventana al terminar.
Los componentes muestran iniciales o un marcador mientras falta la imagen.
Un fallo de imagen no impide leer la conversación.

| Límite | Implementación |
| --- | --- |
| Concurrencia | 4 descargas/decodificaciones; los decoders en curso conservan su permiso al cancelar |
| Respuesta de artwork | 1 MiB; comprobación antes y durante la descarga |
| Artwork de origen | PNG, WebP o primer cuadro GIF, hasta 512 × 512; asignación del decoder acotada a 4 MiB |
| Miniatura de artwork | PNG de 128 × 128, conserva proporción y añade margen transparente |
| Caché | 256 entradas y 16 MiB de base64; LRU en memoria, sin disco |
| Red | Cliente separado sin cabeceras de cuenta ni cookies; HTTPS y origen exacto declarado por el proveedor; sin redirecciones |
| Reintentos | Fallos transitorios: al menos 30 s; 429/5xx conservan la espera por origen incluso al purgar la caché; `Retry-After` no interpretable bloquea ese origen hasta cerrar sesión |
| Cierre de sesión | Cancela tareas, cambia generación, vacía recursos de core/UI y texturas de limn |

La transformación ocurre con `spawn_blocking`, fuera del hilo de UI. limn
recibe su tipo público `RasterImage`; su backend mantiene dos trabajadores
de decodificación, una cola de 32 trabajos/16 MiB y una limpieza explícita de
caché. El límite de 16 MiB de RUNE contabiliza el base64 retenido, no toda la
RAM del proceso: existen además miniaturas en curso y texturas GPU.
El host limita la caché de texturas de limn a 64 entradas fuera de
pantalla; para los previews de mayor tamaño admitido son, como máximo,
96 MiB de píxeles RGBA retenidos, más las texturas de lo que está en
pantalla, además de las colas, los decoders y el resto de recursos de la
aplicación. limn expulsa primero lo que se dibujó hace más tiempo y nunca
una imagen dibujada en el paso de pintado actual o en el anterior
(2026-10-19, rama `emoji-parpadeo`): antes expulsaba la más vieja aunque se
estuviera dibujando, y con el selector de emoji abierto (hasta 72 emoji de
servidor, más los íconos del carril y los avatares) la pantalla pasaba de
64 imágenes, cada paso volvía a decodificar lo que el anterior había
expulsado, las imágenes se turnaban para aparecer y cada decodificación
pedía otro cuadro.

La documentación de Discord aclara que los stickers ignoran el parámetro
`size`, por eso se admite su tamaño de origen antes de reducirlos. Los emojis
se solicitan como WebP para admitir también los subidos en WebP/AVIF. Sólo se
pinta un fotograma; no se afirma reproducción de animaciones.
[Formatos del CDN](https://docs.discord.com/developers/reference#image-formatting).

## Adjuntos recibidos

`Attachment.preview` contiene un `RemoteImage` de clase `AttachmentPreview`;
`Attachment.download` contiene el `RemoteFile` original. Ambos campos son
opcionales y llevan `serde(default)`, al igual que la clase de imagen, cuyo
valor antiguo es `Artwork`. Un archivo sin preview sigue siendo un archivo:
no se inventan píxeles ni conectividad. Las dimensiones de los metadatos
reservan su lugar en el timeline antes de cargarlo; los componentes distinguen
carga pendiente, imagen disponible y fallo.

| Límite de preview | Implementación |
| --- | --- |
| Respuesta | Hasta 8 MiB, tanto por longitud anunciada como por bytes recibidos |
| Formatos | PNG, JPEG, WebP y primer cuadro GIF comprobados por su contenido, no sólo por extensión |
| Dimensiones de origen | Hasta 4096 por lado, 16 777 216 píxeles; límite de asignación del decoder de 80 MiB |
| Salida | PNG de hasta 768 × 512, sin recortar, añadir márgenes ni ampliar imágenes pequeñas |
| Concurrencia | Hasta dos previews dentro del total de cuatro trabajos de imagen |
| Caché | Comparte las 256 entradas/16 MiB de base64; no crea otro almacén persistente |

El límite del decoder no representa el consumo total de un trabajo: también
existen el cuerpo HTTP, los píxeles decodificados y la imagen reducida. Los
semáforos se conservan durante la decodificación aunque se cancele su tarea;
al cerrar sesión, cualquier resultado de la generación anterior se descarta.
`image_failures` comunica a la UI sólo los recursos fallidos visibles, sin
exponer el cuerpo de errores HTTP ni las firmas de sus URLs.

Mientras la preview carga, o si falla, su ThumbHash (`Attachment.placeholder`,
`ImageKind::ThumbHash`) cubre la caja reservada: el núcleo lo decodifica al
instante, sin red, en a lo sumo 32 px. Una imagen explícita que el filtro de
la cuenta tapa se pide como `ImageKind::Obscured`: la misma preview,
achicada a 24 px de lado y desenfocada en CPU, quieta, hasta que la persona
la revela (contenido-de-mensajes, 2026-10-18; ver
[DISCORD_SESSION.md](DISCORD_SESSION.md#contenido-de-mensajes-2026-10-18-rama-contenido-de-mensajes)).

El adaptador de Discord usa `url` y, como alternativa para preview,
`proxy_url`, campos documentados del [objeto Attachment](https://docs.discord.com/developers/resources/message#attachment-object).
Acepta HTTPS en los hosts exactos `cdn.discordapp.com` y
`media.discordapp.net`, con una ruta de adjunto cuyos ids coincidan con el
canal y archivo recibidos; rechaza credenciales en la URL, fragmentos,
puertos no estándar y rutas incompatibles. El cliente de recursos vuelve a
validar el origen y no sigue redirecciones ni envía cabeceras de cuenta.
Los parámetros de firma se conservan exactamente como los entregó el servicio.
Discord documenta que los [enlaces de adjuntos tienen caducidad](https://docs.discord.com/developers/reference#signed-attachment-cdn-urls);
RUNE no fabrica firmas ni implementa un mecanismo de renovación. Un enlace
vencido puede fallar hasta recibir datos nuevos del mensaje.

Los spoilers (`IS_SPOILER` o prefijo `SPOILER_`) no se cargan
automáticamente: la tarjeta ocupa su lugar sin mostrar nada y no pide
píxeles hasta que la persona pulsa «Hacé clic para mostrar»
([FILES.md](FILES.md)). GIF, APNG y WebP animados animan en las filas
visibles del timeline y en el visor, con presupuesto de cuadros y nunca con
movimiento reducido; el audio y el video adjuntos se reproducen en la línea
([MEDIA.md](MEDIA.md#implementado-reproducir-audio-y-video-adjuntos-y-gif-de-tarjetas-2026-10-17)).
SVG, AVIF y HEIC siguen sin decodificarse como previews. Los archivos
generales se descargan sólo mediante una acción del usuario, por el
servicio separado de descargas.

### Referencia de Serein

Se revisó el checkout `d1af883c9d7dda90d1261a2d15fc852a5445204d`, sin copiar
código ni recursos. Su [modelo de adjuntos](https://github.com/ViceVerse-cz/Serein/blob/d1af883c9d7dda90d1261a2d15fc852a5445204d/crates/model/src/attachments.rs)
conserva metadatos acotados, y su [adaptador](https://github.com/ViceVerse-cz/Serein/blob/d1af883c9d7dda90d1261a2d15fc852a5445204d/crates/discord-protocol/src/attachments.rs)
separa su traducción de la autorización de URLs y la decodificación. Esa
separación aplica a RUNE mediante modelo, proveedor y core.

La [caché multimedia de Serein](https://github.com/ViceVerse-cz/Serein/blob/d1af883c9d7dda90d1261a2d15fc852a5445204d/crates/ui/src/avatars/media.rs)
separa los presupuestos de imagen estática, animación y visor, y selecciona
resoluciones según el destino. Su [UI de adjuntos](https://github.com/ViceVerse-cz/Serein/blob/d1af883c9d7dda90d1261a2d15fc852a5445204d/crates/ui/src/attachments.rs)
conserva la proporción de las imágenes y trunca nombres largos dentro de la
tarjeta. RUNE adopta la separación de presupuestos y la reserva de espacio,
con componentes propios de limn y un único preview estático acotado; no
implementa su escalera de resoluciones, sus animaciones ni su caché en disco.
Las [descargas de Serein](https://github.com/ViceVerse-cz/Serein/blob/d1af883c9d7dda90d1261a2d15fc852a5445204d/apps/desktop/src/downloads.rs)
son acciones explícitas separadas del cacheado visual, otra distinción que
se mantiene en RUNE. Esta comparación arquitectónica no acredita autorización
de Discord para la sesión personal experimental.

Validación focal de esta ampliación: 11 pruebas de `rune-core::images`, seis
del mapeo `rune-discord-session::wire` y 11 de `rune-model` aprobadas. Incluyen
compatibilidad de datos anteriores, URLs firmadas ocultas en diagnósticos,
límites de respuesta y resolución, JPEG, proporción, concurrencia y
cancelación. Las peticiones de prueba van a servidores locales sintéticos;
no descargan adjuntos de una cuenta real. Los resultados completos del
workspace se registran aparte cuando termine la integración de la UI.

## Composer y timeline

El selector tiene pestañas de emojis, stickers y GIF, con búsqueda por nombre
en los catálogos del servidor actual. Los emojis disponibles respetan las
restricciones de rol conocidas. El adaptador entrega el texto opaco que se
inserta: la UI no conoce la sintaxis del protocolo. La búsqueda recorre hasta
500 elementos y presenta hasta 72 coincidencias, indicando cuándo refinarla.

Seleccionar un sticker lo prepara en el composer. **Enviar sticker** produce
un mensaje separado y conserva el borrador de texto. El comando pasa por
las capacidades, los permisos y el catálogo vigente; fallos, confirmaciones
y reintentos conservan su identidad como sticker. Los APNG y GIF animan en
las filas visibles del timeline como las demás imágenes; desde P5
(2026-10-17) el selector ofrece los stickers de otros servidores según el
plan de la cuenta ([EXPRESSIONS.md](EXPRESSIONS.md)). Lottie se dibuja y
anima desde el 2026-10-18
([EXPRESSIONS.md](EXPRESSIONS.md#stickers-lottie-2026-10-18-rama-video-y-documentos)).

La pestaña GIF inserta un enlace HTTPS en el borrador y guarda hasta 12 enlaces
recientes sólo durante la sesión. No consulta un buscador ni descarga una
vista previa. El envío sigue requiriendo la acción habitual del composer.

Los mensajes recibidos conservan el contenido original. El adaptador genera
referencias tipadas para que el renderizado muestre nombres de usuarios,
roles y canales, sin cambiar lo que se copia o edita. Los emojis personalizados
recibidos se muestran como `:nombre:`; las imágenes en línea y las menciones
salientes con representación visual requieren trabajo adicional. El código y
los spoilers ocultos no se convierten en enlaces de mención.

## Recursos propios de `assets/`

Los SVG de `assets/` se dibujaron para RUNE: trazos escritos a mano en una
grilla de 16 unidades (24 la marca), en `currentColor` para que el tema los
tiña, junto a los Bootstrap Icons que trae limn. No salen de Discord, de
Serein ni de otro juego de íconos. Son de RUNE, bajo su licencia (MIT o
Apache-2.0).

| Archivo | Qué dibuja | Dónde |
| --- | --- | --- |
| `rune-mark.svg` | La marca: un asta y dos trazos, la runa ᚱ | `rune_components::MARK`, el glifo monocromo de la interfaz (carril, pantalla de carga, perfil) |
| `crown.svg` | Una corona, tras el nombre de quien es dueño del servidor | `rune_components::members::CROWN` |
| `headphones-off.svg` | Auriculares tachados, para la cuenta ensordecida | `rune_components::HEADPHONES_OFF` |
| `person.svg` | El contorno de una persona, para la página de la cuenta | `rune_components::user_settings::PERSON` |
| `shield.svg` | Un escudo con una tilde, para datos y privacidad | `rune_components::user_settings::SHIELD` |
| `signal.svg` | Tres barras de señal, para la conexión de voz | `rune_components::voice::SIGNAL` |
| `icons/rune-small.svg` | El ícono de la aplicación a 16 y 24 px: la marca más gruesa sobre la placa ámbar entera | Origen de `icons/hicolor/16x16` y `24x24` |
| `icons/rune.png`, `icons/hicolor/*` | El ícono de la aplicación, hecho por el dueño: la marca en blanco sobre una placa del ámbar del acento, en la grilla de íconos de macOS (824 de 1024) | Ventanas y barra de tareas (`with_icon`, 256), bandeja de Linux y Windows (`rune-platform/src/tray/mark.rs`), `rune --install`, notificaciones, entrada de escritorio, inicio automático, README |
| `icons/rune.ico` | El mismo ícono para Windows: 16, 24, 32, 48, 64 y 128 en BMP de 32 bits y 256 en PNG | Ejecutable e instalador de Windows. Generado (rama `barra-e-icono`) con `icotool -c` de icoutils desde los PNG de `icons/hicolor`, pasados antes a 8 bits por canal con ImageMagick (`magick … -depth 8 PNG32:…`); el de 256 entra con `-r`, sin recomprimir |

Los sonidos de llamada (`assets/sounds/*.ogg`) también son propios: tonos
sintetizados por `scripts/sounds.sh`, sin grabaciones
([AUDIO-NOTICES.md](../assets/licenses/AUDIO-NOTICES.md)). Lo de terceros
(Twemoji, la marca de GIPHY, las fuentes) tiene sus avisos en
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).

## Símbolos y licencias

limn añade Noto Sans Symbols 2 y reutiliza el fallback monoespaciado para
símbolos decorativos y líneas. Noto Emoji amplía la cobertura monocroma del
catálogo: los escalares visibles de sus 623 entradas tienen glifos, pero
algunas banderas, keycaps y secuencias compuestas siguen incompletas. Es el
respaldo de lo que Twemoji no dibuja (ver abajo). Fuentes, versiones y
avisos: [FONTS.md](FONTS.md).
El código y los recursos gráficos de Serein no se copiaron; el análisis de
sus decisiones está en [SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md).

Las mejoras de limn se desarrollaron en worktrees independientes:
[cola de imágenes #39](https://github.com/Niiihuel/limn/pull/39) y
[símbolos #40](https://github.com/Niiihuel/limn/pull/40) y
[respaldo emoji #41](https://github.com/Niiihuel/limn/pull/41). RUNE fija la integración
local `2618b37d1c17b73ef70c43e67f5641ec4da41a3a` en su lock; las PR siguen
separadas de la rama principal de limn.

### Emoji a color (Twemoji)

RUNE pinta los emoji Unicode con los gráficos de
[Twemoji](https://github.com/jdecked/twemoji) 17.0.3 (CC BY 4.0), los mismos
que Discord acredita en sus [avisos de licencias](https://discord.com/licenses).
El texto sigue siendo Unicode: limn mide, parte líneas, selecciona, copia y
anuncia a la accesibilidad los caracteres originales; sólo cambia el dibujo.

| Pieza | Dónde |
| --- | --- |
| Paquete | `assets/twemoji/pngs.bin`: los 4009 PNG de 72 × 72 de `assets/72x72` del release, sin modificar, uno tras otro (4.248.486 bytes) |
| Índice | `assets/twemoji/index.tsv`: secuencia sin U+FE0F, posición, largo y presentación por defecto (`t` texto, `e` emoji) según Unicode `emoji-data.txt` 17.0.0 (120.898 bytes) |
| Generador | `scripts/twemoji.py`: sólo biblioteca estándar; fija el commit `b6b55fef…` y los SHA-256 del archivo y de `emoji-data.txt`, verifica 4009 PNG de 72 px y secuencias únicas, y produce los mismos bytes en cada ejecución |
| Atribución | [assets/licenses/TWEMOJI.md](../assets/licenses/TWEMOJI.md) y la licencia completa `Twemoji-CC-BY-4.0.txt` |
| Búsqueda | `crates/rune-components/src/twemoji.rs` (antes en `apps/desktop`), `picture(grafema)`; se instala con `limn_egui::set_color_emoji`. Desde el 2026-10-18 también `emoji_len(texto)`, la tabla de qué es un emoji (secuencia más larga del paquete: un keycap `1️⃣` es uno; `©`, `®` y `™` nunca, como el cliente oficial) e `image(secuencia)`, el PNG como `RasterImage` |
| Jumbo | Un mensaje de sólo emoji dibuja cada emoji Unicode como imagen en la línea (`TextSpan::image`), 48 px como uno del servidor; antes se agrandaba la letra y el color quedaba en 1–1,2 em (35–42 px) |
| Dibujo | limn-egui, [PR #67](https://github.com/Niiihuel/limn/pull/67): cada `DrawCommand::Text` |

Se eligió un paquete de PNG en lugar de un atlas: se decodifica sólo lo que
aparece en pantalla, al tamaño dibujado, sin reservar los ~83 MiB RGBA que
ocuparía un atlas de 4009 celdas de 72 px. 72 px cubre el emoji en línea
(1,2 em, unos 19 px a 16 px) incluso a escala 3 y el selector a escala 2;
el paquete añade 4,2 MB al binario, menos que la fuente Noto Emoji
monocroma (1,9 MB) más el SVG que haría falta para tamaños grandes.

Reglas de presentación: el grafema extendido completo es la unidad
(familias ZWJ, tonos de piel, banderas regionales y de subdivisión, keycaps).
U+FE0F se ignora al buscar; U+FE0E pide texto y deja el glifo. Un único
punto de código cuya presentación por defecto es texto (`❤`, `©`, `☀`,
`↔`) se dibuja en color sólo con U+FE0F. Lo que no está en el paquete
conserva el glifo de la fuente (Noto Emoji monocromo o el recuadro de
reemplazo). Mientras un PNG se decodifica (como mucho 24 por cuadro) se ve
el glifo un cuadro.

Cobertura: todo el texto que limn pinta, porque el reemplazo ocurre en el
backend y no en cada componente: Markdown de mensajes, nombres de
servidores, canales, usuarios y apodos, tooltips del riel, reacciones y
barra de reacciones rápidas, selector de emoji, sugerencias del composer,
avisos y también el texto editable del composer. No se copió código ni
recursos de Serein; sólo se tomó la idea de dibujar Twemoji como imagen.

![Reacciones, selector y borrador con Twemoji](img/color-emoji-2026-10-16.png)

Captura con el proveedor simulado (`--channel ayuda`, `--action
composer:emoji`, `--capture-draft`): mensaje y reacciones, selector y un
borrador con ZWJ, banderas, keycap, tono de piel y `❤` sin VS16 (queda
monocromo, como corresponde). Límite conocido: el ancho lo sigue dando la
fuente monocroma. Una secuencia que Noto Emoji no compone (la familia
👨‍👩‍👧‍👦, las banderas regionales) ocupa varios glifos y su imagen queda
centrada con aire a los lados; un keycap ocupa el ancho de su dígito y la
imagen (mínimo 1 em) invade un poco el espacio vecino. Los nombres de
servidor, canal y apodos y los tooltips usan el mismo camino de dibujo pero
el proveedor simulado no tiene emoji en ellos, así que no tienen captura.

GPUI Kit sigue otro camino: delega al sistema tipográfico de GPUI y a las
fuentes instaladas o aportadas por la aplicación. El backend Linux actual
puede rasterizar contornos y bitmaps a color de la fuente mediante
CosmicTextSystem/Swash; no proporciona por ello un estilo uniforme de Discord.
La demo web tiene un fallback Canvas propio y no sirve para deducir el
resultado nativo. Referencias primarias:
[fuentes de GPUI Kit](https://github.com/longbridge/gpui-kit/blob/main/website/docs/fonts.md),
[renderer Linux de GPUI](https://github.com/zed-industries/zed/blob/main/crates/gpui_wgpu/src/cosmic_text_system.rs).

## Validación de código (2026-10-14)

`cargo check --workspace --all-features` y `cargo test --workspace
--all-features` pasaron dentro de `nix-shell --arg withLogin true`: **462
pruebas aprobadas**, cinco fixtures nativos de login ignorados en la suite
general. El auxiliar de login no cambió en esta iteración.

Las pruebas nuevas cubren HTTP local sin cabeceras de cuenta, rechazo de
redirecciones, formatos y límites de imagen, expiración y conservación de
`Retry-After` al purgar la caché, cancelación y recursos visibles. También
ejercitan catálogos/permisos, inserción sin envío automático, foco y teclado,
conservación del borrador, inserción sobre selección Unicode, continuidad de
foco entre eventos del mismo cuadro, referencias de mensaje y envío/reintento de
stickers con respuestas sintéticas. No usan la sesión real ni descargan
recursos de la cuenta del dueño; la compatibilidad de esas nuevas funciones
contra Discord necesita su comprobación manual.

Después de ajustar el espacio alrededor de las pestañas y del botón de
inserción, pasaron de nuevo las **254 pruebas de `rune-components` y
`rune-ui`**, incluida una regresión que comprueba la altura del área de clic.
También pasaron `cargo fmt --all -- --check`, Clippy del workspace con todas
las features y `-D warnings`, y la compilación release con `session-login`.
La suite completa de 462 pruebas corresponde a la revisión anterior a ese
último ajuste visual; no se volvió a ejecutar completa después del ajuste.

REA ejecutó el binario final en Wayland con el proveedor simulado y capturó
las tres pestañas. Se revisaron las imágenes, el contraste de la selección,
la cobertura de glifos y el tamaño de los controles. Los identificadores de
evidencia y el hash del artefacto están en [REA.md](REA.md). Estas capturas no
validan descargas autenticadas ni envíos reales, y los emojis Unicode
continúan siendo monocromos hasta integrar los recursos de Twemoji.
