# RUNE sobre limn

RUNE construye toda su interfaz principal con [limn](https://github.com/Niiihuel/limn):
el modelo declarativo (la ventana es una función del estado, reconstruida
cada cuadro), el `UiTree` que guarda foco, hover, scroll y texto por ID, y el
backend `limn-egui` (egui + wgpu) que pinta y traduce la entrada. egui no se
usa como framework de componentes: solo `apps/desktop` toca eframe/egui, y
únicamente para abrir la ventana y capturar pantalla; los atajos de la
ventana (los de la tabla editable `rune_ui::keybinds` y los botones laterales
del mouse), el grabador de teclas y Pulsar para hablar llegan como eventos de
limn.

La excepción autorizada por el dueño el 2026-10-14 es una WebView temporal
para el login del proveedor no oficial. Vive en un proceso auxiliar opcional,
con GTK/WebKit en Linux; no renderiza conversaciones ni sustituye componentes
de limn. Su alcance y validación pendiente están en
[DISCORD_SESSION.md](DISCORD_SESSION.md).

## Cómo se conecta

| Pieza de limn | Uso en RUNE |
| --- | --- |
| `UiTree`, `Element`, `UiAction`, `UiEvent` | Bucle de cuadros en `apps/desktop`; `rune-ui` traduce acciones en `Intent` |
| `ScrollArea::measured_list` + `follow_end` | Timeline virtualizado de altura variable (PR #25) |
| `SearchField`, `Modal`, `ScrollArea::uniform_list`, `UiTree::scroll_to_index` | Búsqueda local: consulta, resultados virtualizados y salto al mensaje; usa APIs existentes, sin extensión de limn |
| `Button`, `Avatar`, `Spinner`, `ScrollArea`, `Toast::layer` | Autorización explícita, identidad sin presencia inventada, servidores de sólo lectura y avisos; la página usa APIs existentes de limn |
| `UiTree::visible_rows`, `anchor_offset`, `scroll_to_end`, `is_scroll_at_end` | Cargar historial al acercarse al inicio, marcar leído, "Ir a lo más reciente" |
| `Composer` + `enter_submits` + `pill_controls` + `inline_controls` + `auto_grow` | Composer: Enter envía, Shift+Enter salta línea (PR #26); el editor crece entre «+» y el emoji, que quedan junto a la primera línea (PR #85) |
| `SuggestionList` (`SuggestionItem::leading`, `ink`, `detail`) | Autocompletado de @menciones con avatar, color del rol y usuario a la derecha (la ventana mueve la selección con ↑↓ y acepta con Tab/Enter) |
| `Markdown` (+ `MarkdownColors`) | Contenido de los mensajes: negrita, cursiva, código, bloques, citas, listas, enlaces |
| `MessageActions` (visibilidad `Hover`) + `hover_region` / `reveal_on_hover` | Acciones del mensaje al pasar el puntero |
| `hover_region_id` + `UiTree::hovered_region`, `dim_on_hover` | La fila de mensaje bajo el puntero (anima la decoración y la nameplate de su autor) y el efecto de la tarjeta al 35 % bajo el puntero (rama `rune/hover-dim`, 2026-10-18) |
| `ContextMenu::open_action` + `DropdownMenu::at` | El clic derecho en un mensaje pide su menú completo y RUNE lo abre en ese punto (reacciones rápidas con `DropdownMenuEntry::custom`, borrar con `::destructive`) |
| `Popover` | Selector de emoji (composer y reacciones), tarjeta de perfil |
| `DropdownMenu` (`::at`, `::checkbox`, `::radio`, `::description`, `::tooltip_with`, `::submenu`) + `popover_on_hover` | Menús contextuales de canales, personas en voz y servidores; menú de estado del usuario, que se abre al pasar el puntero |
| `Select` (`SelectVariant::Field`), `SwatchPicker`, `ColorPicker`, `RadioOption::icon`, `SearchField::clearable`, `Slider::mark_labels`, `Tabs` (`TabsVariant::Underlined`) | Formularios de ajustes y de administración (2026-10-18) |
| `ScrollbarVisibility::AutoHideUnlessHovered` | La barra de la línea de tiempo aparece con el puntero encima o al desplazar y se desvanece |
| `Badge` (`placement`, `height`, `outline`), `Tooltip` (`delay_ms`, `title_font_size`, `background`, `faces`) | Menciones en el riel, tooltip de cada servidor como el oficial |
| `Avatar`, `Attachment`, `SkeletonGroup`, `Spinner`, `Empty` | Avatares, archivos, carga, estado vacío |
| `Modal`, `AlertDialog`, `Toast::layer` | Ajustes y selector rápido, confirmación de borrado, avisos |
| `SegmentedControl`, `PreferencesGroup`, `ActionRow`, `Switch`, `ListRow`, `Kbd`, `SearchField` | Ajustes, selector rápido, atajos |
| `DictationWaveform` | Medidor del micrófono en ajustes |
| `Icon` (Bootstrap Icons) + `Icon::Custom` | Iconos semánticos; la marca de RUNE y "auriculares tachados" son SVG propios en `assets/` |
| `set_labels(Labels::SPANISH)` | Textos propios de limn en español |
| `Role::Region` + `UiTree::focusables_within` | Cinco regiones con nombre (servidores, canales, mensajes, composer, miembros); F6 las recorre con el aterrizaje propio de RUNE (PR #66) |
| `Element::surface` + `SurfaceFit::Contain` + `radius`, `SurfaceFrame` clonable, `ImageViewer::surface` | GIF y stickers animados en su caja redondeada y en el visor a tamaño completo (PR #66); en el visor nunca más grandes que sus píxeles (`SurfaceFit::ScaleDown`, paquete 10) |
| `FontFamily::SansSemibold`, `Tooltip::title_font_family`, `UiFaces::semibold` | Seminegrita donde la REA la pide: título de los tooltips del carril y nombre de la tarjeta de actividad (paquete 10) |
| `DropdownOption::tooltip`, `Avatar::color` | Nombre entero de cada dispositivo recortado en el panel rápido de voz; color de la persona tras su inicial (paquete 10) |

## Página de autorización

`crates/rune-ui/src/auth.rs` compone la página con elementos y componentes
existentes de limn. Recibe `Store::authentication`, `ProviderInfo` y los
datos de dominio; no importa adaptadores concretos, no conoce endpoints y no
recibe credenciales. `Capabilities::authentication` controla su
disponibilidad. `SignedIn` abre el espacio de trabajo aunque todavía no haya
conversaciones: conserva riel, navegación, perfil y acceso a ajustes, con un
estado vacío y conexión explícitos. No inventa canales ni habilita composer,
presencia o voz sin capacidades. Los detalles del proveedor y los scopes
quedan en Ajustes → Conexión; `ProviderKind::Unofficial` no se presenta como
OAuth2. La pantalla de acceso sólo conserva la acción principal, estado de
espera o error y cancelación. El cierre de sesión está en Mi cuenta.

Los botones producen los comandos genéricos `BeginSignIn`, `CancelSignIn`
y `SignOut`. En OAuth2, abrir el navegador produce `Intent::OpenUrl`, que
resuelve el host con el navegador del sistema. `AwaitingExternal` muestra
que el login continúa en otra ventana y ofrece cancelar; su apertura y la
entrega privada del resultado corresponden al proveedor y al auxiliar,
fuera de `rune-ui`. No hay campos para credenciales en la página limn.
Cada cambio de fase pide foco para el control correspondiente después del layout. Tab
y activación siguen a cargo de limn; Escape cancela una autorización
pendiente y los atajos de conversación no abren controles ajenos a esa
página. Después de entrar, el foco pasa a Inicio y los ajustes funcionan
también si no existe un canal seleccionado.

`ScrollArea` permite usar la página en la ventana mínima. `Toast::layer` se comparte con la vista de chat para
que un fallo de revocación no desaparezca al cerrar la sesión. El motivo
de caducidad se mantiene también como texto de la página. Las pruebas sin
ventana verifican acciones, ventana mínima, identidad sin conversaciones,
limpieza del estado de UI y avisos visibles, además de foco/cancelación del
login externo y ausencia de scopes o servidores inventados. No acreditan el
intercambio OAuth2 real ni el handoff del auxiliar. Esta integración no
requirió modificar limn.

## Lo que faltaba en limn: pull requests

Por indicación del autor de limn, lo que RUNE necesitaba y limn no tenía se
implementó **dentro de limn** y se envió como PR, con tests, documentación,
entrada de changelog y demo en la galería:

### Registro histórico de PR del 2026-10-14

| PR | Qué añade | Por qué |
| --- | --- | --- |
| [#25](https://github.com/Niiihuel/limn/pull/25) `feat/measured-list` | `ScrollArea::measured_list` / `Element::measured_list`: lista virtual de filas de altura propia, medidas al entrar en vista; anclaje por clave de fila (insertar arriba o crecer no mueve lo visible, también durante un scroll suave: `ScalarTransition::shift`); `follow_end`; `UiTree::visible_rows` | `uniform_list` solo admite filas de una altura; un chat necesita miles de mensajes de alturas distintas sin saltos al cargar historial |
| [#26](https://github.com/Niiihuel/limn/pull/26) `feat/enter-submits` (apilado sobre #25) | `TextArea::enter_submits`, `Composer::enter_submits`, `Element::text_area_enter_submits` | Enter debía enviar y Shift+Enter saltar línea; limn hacía lo contrario |
| [#27](https://github.com/Niiihuel/limn/pull/27) `feat/conversation-icons` | `Icon::MicrophoneMuted`, `Headphones`, `Emoji`, `Reply`, `At`, `Phone`, `HangUp`, `Screen`, `VideoOff` (Bootstrap Icons 1.13.2, MIT) | Iconos semánticos de conversación y llamada |
| [#38](https://github.com/Niiihuel/limn/pull/38) `fix/math-font-fallback` | Fallback Noto Sans Math 3.000 (OFL-1.1) en las familias de limn-egui, con licencia y procedencia incluidas | Los alfabetos Unicode matemáticos de nombres y mensajes se veían como cuadrados |

En la segunda iteración del 2026-10-14, #25–#27 se registraron como abiertas
y `main` sin cambios desde 0.1.41. Es una observación histórica de aquel
momento, no su estado actual. La segunda iteración no necesitó otra extensión.

### Estado consultado y pins por checkpoint (2026-10-15)

Consulta de lectura a GitHub mediante `gh pr view`: #25, #27 y #38 figuran
fusionadas; #26, #39, #40 y #41 figuran cerradas sin `mergedAt`. Esto describe
el estado de cada PR, no deduce qué commits equivalentes puedan existir en
otras ramas. La [PR #49](https://github.com/Niiihuel/limn/pull/49) está abierta.

Los binarios y el snapshot fuente retenidos de **07.2** fijan los tres paquetes
`limn`, `limn-editor` y `limn-egui` en
`2618b37d1c17b73ef70c43e67f5641ec4da41a3a`. El primer commit de #49 llegó a
integración en `b91772bbb486291a5e0d272987b83aa04ef22971`. Los checkpoints
**07.3.1 y SaveFile 08.1** fijan `1ad33eb224b6b4fb48de9423b23a89d6230e087f`,
con la corrección posterior del orden de entrada del bridge. El pin histórico
de **07.3.2** es `22e0fd93c14b3f3c89375135f83c1e9c41307330`, con la
[PR #50](https://github.com/Niiihuel/limn/pull/50) abierta y consumida por
cherry-pick en `rune/integration`; no está fusionada públicamente. El rechazo
negativo y el aviso ya tienen QA nativa; la importación por arrastre sigue abierta. La
alternativa Wayland `limn-dnd` existente se distingue en el análisis de abajo.
No se atribuye ese pin nuevo a las capturas ni pruebas históricas de 07.2.
Las pruebas de la rama limn y las del workspace RUNE son verificaciones separadas.

El pin actual del consumidor ordenado es
`6ae42859f86f5d255e107a23142aca3c19c86590`, publicado en la rama propia
`rune/ordered-input-integration`, con padres `22e0fd93` y `b52284a0` (PR #59 Draft).
Preserva Composer/Column; no constituye merge público de las PR. El converter
indexado conserva derivados de frame en prefijo None y sustituye el after-input
artificial previo. RUNE mantiene rechazo legacy y barrera drop+Enter.

La integración limn pasó 1595 pruebas, cinco ignoradas, 21 suites/6,66 s sumadas,
con checks estrictos. El host RUNE pasó 1128/6/25, con ocho regresiones App de
orden/foco/query y checks independientes; QA propia Mock/Xvfb final verificada,
con primer intento parcial conservado. No acredita clipboard compartido,
owner/ACK, DnD ni cierre de 07.3. Diez imágenes QA adicionales no se suman a las 18
Mock históricas. [Checkpoint](evidence/rune-indexed-input-host-2026-10-15.json).

### Cómo depende RUNE de esas ramas

`Cargo.toml` del workspace apunta a la rama de integración de limn que
reúne las extensiones sucesivas (PR #66–#80, #82, #83, #85 y #86). Desde el 2026-10-17
las ramas van en [Niiihuel/limn](https://github.com/Niiihuel/limn) para
que RUNE compile en cualquier máquina, y una rama de integración nueva se
sube a GitHub antes de mezclar a RUNE la rama que apunta a ella. Un
trabajador apunta a su clon local (`file:///…/lienzo`), y el integrador
la sube y cambia `Cargo.toml` a GitHub antes de mezclar. Hoy RUNE apunta a
`rune/integration-2026-10-19` en GitHub (`1407eef`, subida el 2026-10-19 para la primera release):
`rune/hover-dim` (`637990d`: `rune/limn-texto` `152c5a8` más
`dim_on_hover` y `hover_region_id`, PR #83 desde `feat/hover-dim`; subida el
2026-10-18 al mezclar `perfil-tarjeta-y-cosmeticos`) más el paquete 10
(PR #84), el paquete 8 (PR #85), el paquete 6 (PR #86), la caché de texturas (PR #91) y `WindowControls` (PR #89), unidos el 2026-10-19
([abajo](#rama-de-integración-del-2026-10-19)). El commit consumido lo
determina el lock,
no el HEAD actual de esa rama. Es una dependencia **git**, no de ruta: así
`cargo fmt --all` no reformatea las fuentes de limn (lo haría con una
dependencia de ruta) y el commit queda fijado en `Cargo.lock`.

Tras cambiar algo en limn:

```sh
cd ~/projects/personal/limn-rune-ordered  # worktree en rune/ordered-input-integration
git commit …                              # o merge de la rama del PR
cd ~/projects/personal/rune && cargo update -p limn -p limn-egui
```

Cuando una versión publicada reúna los cambios necesarios, se podrá sustituir
por una dependencia de crates.io después de verificar la integración.

La revisión de fuentes del 2026-10-14 usó el commit de integración `896c3dc`:
conservaba las mejoras anteriores y añadía el PR #38 antes de su fusión pública.
La regresión falla sin la fuente en U+1D597 y pasa con las muestras
sintéticas `𝖗𝖚𝖓𝖊`, `𝓡𝓤𝓝𝓔` y los alfanuméricos asignados del bloque
U+1D400–U+1D7FF, en todas las familias. Se comprobaron también métricas
latinas sin cambios y capturas claras/oscuras. No se inspeccionó el texto
real del nombre de cuenta; esto amplía la cobertura verificada, no demuestra
que cubra cualquier nombre, CJK o emoji en color. La fuente añade 1.015.396
bytes al recurso embebido y conserva sus avisos OFL en limn-egui.

### Símbolos decorativos y límites del decoder de imágenes

El [PR #40](https://github.com/Niiihuel/limn/pull/40), rama
`fix/symbol-font-fallback`, añade Noto Sans Symbols 2 v2.008 después de
Math. El OTF oficial sin modificar pesa 404.576 bytes y conserva su licencia
OFL-1.1. Las familias proporcionales también reutilizan al final las caras
monospace ya incluidas por egui para dibujar separadores como `├─│╭╰║`.
La prueba consulta charmaps de símbolos sintéticos en las cinco familias
y verifica que egui resuelva `✦`, ausente en la cadena anterior. Se mantiene
la prioridad de las caras previas y las métricas latinas. Las capturas
nativas claras/oscuras de la galería muestran las muestras; no acreditan
los caracteres exactos de ninguna cuenta ni cobertura completa de Unicode,
CJK o emoji en color. Procedencia y avisos: [FONTS.md](FONTS.md).

El [PR #39](https://github.com/Niiihuel/limn/pull/39), rama
`fix/bounded-image-decode`, sustituye el hilo por imagen por dos workers
por hilo de UI, con cola no bloqueante de hasta 32 fuentes y 16 MiB de
datos codificados. `limn_egui::clear_image_cache()` libera las texturas y
fuentes en cola al cambiar de cuenta. Como máximo dos decodes activos
pueden terminar; su generación anterior impide publicar el resultado.
El host debe retirar también las fuentes anteriores de su modelo y árbol
UI. La galería ImageViewer incluye una acción de recarga de caché; las
pruebas verifican límites, liberación de la cola, invalidación de resultados
tardíos y que la fuente máxima admitida por `RasterImage` cabe en la cola.
Los límites de dimensiones, formato, hash y asignación del decoder siguen
vigentes. El backend de navegador conserva su camino síncrono.

En esa revisión del 2026-10-14, ambos PR estaban apilados sobre #38 e integrados
en `rune/integration`, sin fusionarse públicamente ni tocar `main`. Sus tests de limn-egui/galería
y Clippy focalizados pasaron. El workspace y el pin de RUNE se validan por
separado durante la integración, sin confundir esos resultados con los de
limn. La integración de ambos dejó el commit
`0f17f2ddff06761bd5e9e3724bdce336e70b9973`.

### Respaldo de emoji monocromos

El [PR #41](https://github.com/Niiihuel/limn/pull/41), rama
`fix/emoji-font-fallback`, añade Noto Emoji 3.002 después de todas las
fuentes existentes, sin reemplazar sus glifos ni métricas. El archivo
variable oficial añade 1.982.596 bytes, con OFL-1.1, procedencia fijada y
hashes. No requiere nuevas dependencias de ejecución. La galería Text
incluye ejemplos de las cinco familias y muestra las secuencias que siguen
limitadas por la selección de fuente del backend.

La fixture de 623 cadenas Unicode del picker carecía de 124 caracteres
en la cadena proporcional previa; con el respaldo, las tablas de las
fuentes configuradas cubren todos los escalares de dibujo del catálogo en
las cinco familias. Las pruebas también ejercitan la búsqueda real de egui
para caracteres nuevos, conservan las medidas del texto anterior y
comprueban composición para `❤️`, `☀️`, `🧑‍💻` y `🏴‍☠️`. Los flags
regionales, keycaps y algunas secuencias cuyo primer carácter usa una cara
anterior siguen incompletos. Esto no añade emoji en color ni cobertura
universal de secuencias; ver [FONTS.md](FONTS.md).

Pasaron 49 tests de limn-egui, 31 de galería, un doctest y Clippy de ambos
paquetes con todos los targets y warnings como errores. Se verificaron
capturas nativas claras/oscuras con texto sintético. En esa validación el PR
se registró abierto, apilado sobre #40; su merge local dejó `rune/integration` en
`2618b37d1c17b73ef70c43e67f5641ec4da41a3a`. La actualización del lock y la
validación de RUNE forman parte de la integración, por separado.

### Composer como destino de archivos: PR #49 (2026-10-15)

La [PR #49](https://github.com/Niiihuel/limn/pull/49), rama
`feat/composer-file-drop`, añade `Composer::drop_target()` como opción
desactivada por defecto. Sólo el editor habilitado acepta archivos y conserva
su ID como `UiAction::FilesDropped.target`: no agrega otra región de hit test,
no mueve foco, no modifica texto ni envía el borrador. El host sigue siendo
responsable de validar todo el lote, cuenta/permisos y acceso a archivos.

Hover y drop respetan las superficies flotantes: no alcanzan un destino de
fondo cubierto por un menú contextual o popover. Un modal bloquea el fondo
incluso fuera de su tarjeta y permite el destino de su propio cuerpo. Las
regresiones ejercitan esos límites y el bridge nativo conserva dos rutas sin
leer sus bytes; sin posición del puntero no inventa un destino. La última
posición informada por el SO aún puede llegar atrasada durante un arrastre.

La galería Editor muestra el opt-in y guarda sólo el número de archivos;
conserva el borrador y no serializa rutas. El PR incluye documentación de API
y backend, changelog y catálogo de galería regenerado. Su commit es
`a1a51af9c593d7e633ef7c9d75b78c1813137708`; el cherry-pick en el worktree
`limn-rune`, rama `rune/integration`, es
`b91772bbb486291a5e0d272987b83aa04ef22971`.

El segundo commit de la PR, `6b83daf3e3130649484e82946ea3201295ce59ee`,
se incorporó como `1ad33eb224b6b4fb48de9423b23a89d6230e087f`, pin de los
snapshots 07.3.1/08.1. El bridge entrega hover/drop después de la entrada ordinaria del cuadro,
para que el host aplique antes un clic, Escape o cambio de vista y vuelva a
comprobar el editor. La regresión reprodujo el orden anterior. egui agrupa la
metadata de archivos por separado: el orden no representa una cronología del
SO. El wrapper de touch todavía puede añadir acciones derivadas después;
RUNE usa `events` directamente y no implementó otro wrapper ni acredita aquí
una nueva ruta de touch.

El coordinador verificó esa integración: **1405 pruebas aprobadas, dos
ignoradas, 13 suites, 0,92 s**, y Clippy estricto de todos los targets sin
advertencias. La captura de galería con conteo sintético no acredita un
arrastre real del SO. **El checkpoint RUNE 07.2 está fijado en `2618b37`**.
La actualización del lock para 07.3.1 se hizo después de retenerlo. El prototipo
host/Core verificó **50 Core uploads + 6 host**, UI 6, plataforma 54 y componente
1. Su revalidación de vista no ligaba una posición nativa al lote. El host
bloquea `Submitted` del editor en cuadros con un drop rechazado, también en
accesibilidad. Se conserva el borrador ante drop+Enter;
enviar requiere otra acción. Shift+Enter y los otros controles no quedan
bloqueados por esa barrera. El workspace RUNE pasó **1098 pruebas**, seis
ignoradas, 25 suites en 5,47 s con todas las features, formato y Clippy estricto
de todos los targets/features. Release de ocho crates: 51,13 s. Dos gestos
nativos no produjeron preparación, sin Enter/envío ni PNG aceptado; el alcance
local y los intentos quedan en el [registro 07.3.1](evidence/uploads-drop-checkpoint-2026-10-15.json).
La política actual **07.3.2** limpia todo `dropped_files`/`hovered_files` legacy
antes del bridge y rechaza también el fallback de `FilesDropped`. El host no
llama a `Client::prepare_attachment_paths`; esa API Core conserva sus pruebas
para una entrada futura fiable. El snapshot histórico de SaveFile pasó **1112
pruebas**, seis ignoradas y 25 suites en 5,56 s, con pin `1ad33eb`; se conserva
por separado del cierre negativo descrito abajo.
No se afirma soporte comprobado de drop en Windows/macOS.
La selección múltiple 07.2 ya se verificó con portal GTK y dos fixtures en Mock;
no equivale a recibir archivos por arrastre o portapapeles. Ver
[contrato y continuación](../tasks/uploads-next.md#073-arrastre-y-portapapeles).

### Avisos en flujo y altura opt-in (PR #50, 2026-10-15)

La [PR #50](https://github.com/Niiihuel/limn/pull/50), abierta sin merge público,
añade `Element::shrink_height_to_fit()` como opt-in para hijos de una columna.
Conserva la altura natural si cabe y cede espacio a hermanos fijos/flexibles;
no cambia `shrink_to_fit()`, que reduce ancho en filas. El contenido reducido
se combina con ScrollArea para seguir siendo alcanzable. Las regresiones
incluyen mínimos flexibles, medidas infinitas, padding y scroll; la galería
muestra el opt-in. El checkpoint 07.3.2 usó el cherry-pick propio `22e0fd93…`;
el pin actual `6ae42859` lo conserva sin implicar fusión pública de la PR.

Verificación de la rama pública en `28286246f2e6f70c610076e85f52ffc24c15195e`:
**1536 aprobadas, cinco ignoradas, 21 suites**.
La rama de integración pasó **1508 aprobadas, cinco ignoradas, 20 suites**;
sin features predeterminadas, **1324 aprobadas, dos ignoradas, 11 suites**.
Clippy normal/hot, rustdoc con advertencias denegadas y `git diff --check`
correctos. **No se acredita `cargo fmt` de limn**; `git diff --check` comprueba
whitespace del diff. La verificación RUNE es independiente: **1120 aprobadas,
seis ignoradas, 25 suites**, fmt y Clippy estricto correctos; release de seis
crates en 49,56 s. Siete regresiones de avisos y ocho del host están verdes.

En la QA negativa X11, Dolphin soltó dos fixtures de 54/55 B sobre Mock RUNE:
el aviso completo quedó por encima del editor y el borrador siguió visible,
sin adjuntos ni Enter. [Captura](img/uploads-drop-native-rejected-2026-10-15.png),
[checkpoint](evidence/uploads-drop-rejected-checkpoint-2026-10-15.json) y
[auditoría estricta](evidence/uploads-drop-rejected-audit-2026-10-15.json).
Hay 18 capturas canónicas Mock, incluida esta negativa. Eso cierra únicamente
el rechazo/pulido 07.3.2; el no-envío automático lo prueban los tests del host.
La importación nativa funcional, pegado y transporte remoto siguen pendientes.

El bloqueo está debajo del bridge limn: eframe/egui **0.36.2**, también en su
[rama principal](https://github.com/emilk/egui/blob/main/Cargo.toml), dependen
de winit **0.30.13**. Esa vía no produce eventos DnD de archivos en Wayland y
X11 no expone la posición de arrastre. La [PR #4571](https://github.com/rust-windowing/winit/pull/4571),
fusionada el 2026-07-16, introduce una API distinta en 0.31 beta. La migración
[egui #7731](https://github.com/emilk/egui/pull/7731) continúa abierta/en borrador:
su HEAD parte de egui 0.33.2, conserva handlers de archivos comentados y no es
una actualización operativa. La restricción `winit = "0.30.13"` no admite 0.31;
un cambio de pin sólo en limn no migra eframe/egui-winit. La migración es una
alternativa, no una obligación universal: existe también la vía Wayland que se
describe a continuación. Ninguna habilita un parche/fork implícito ni acredita
el recorrido nativo 07.3.1 por haber compilado.

**Corrección por inspección de fuente:** `limn-dnd` ya existe con el mismo árbol
git `5d357bea…` en `2618b37`, `1ad33eb` y main `a3ce02a` del 2026-10-15; no es
una capacidad nueva del cambio de pin. Está fuera del grafo Cargo efectivo de
RUNE y no tiene consumidor en `limn-egui`. Su `FileDnd::attach` conecta una cola
Wayland privada al display/surface de la ventana real; enter/motion y la oferta
aportan posición y lote sin depender del productor de winit. Eso lo hace un
candidato a una integración Wayland sin fork de winit, **todavía no probada**.
[Fuentes, hashes y alcance estático](evidence/limn-dnd-existing-backend-audit-2026-10-15.json).

El backend legacy de esa inspección no se debe conectar tal cual: anuncia
Copy|Move y finaliza antes de validar,
carece de identidad de oferta en el resultado asíncrono, usa colas sin límite
y un hilo lector por drop sin cancelación/plazo, y su Drop hace join. La API,
segura en ese snapshot, sólo documentaba que el dueño de los handles debía
sobrevivir; no lo retenía.
eframe ofrece handles y un `Arc<Window>`; el SDK cacheado muestra una Connection
en WindowState, pero esa cadena no sustituye una prueba de vida del display,
despacho y limpieza durante el cierre. El test de conexión de ese snapshot no
probaba un lote entrante y retornaba sin ejercicio si faltaba compositor.
X11/Windows/macOS no se acreditan por el comentario de fallback a winit del crate.

El nuevo receiver ya está implementado en la [PR #55](https://github.com/Niiihuel/limn/pull/55),
**Draft abierta**, head `73382702c5a1137ba2cfcbad5d5ebc2585697e6a`, con biblioteca
`0f8a74c47a27620cf322434af5bf9d8cb6fb23ac`. El
[check CI](https://github.com/Niiihuel/limn/actions/runs/38772048890/job/116550099462)
terminó en SUCCESS el 2026-10-15 a las 16:50:38 UTC, verificado por lectura
primaria posterior a la observación inicial en progreso. Verificó en Weston privado dos
lotes de dos nombres, Accept Copy/Reject explícitos y cierre normal. Son 124
pruebas de biblioteca y tres Gallery dentro de las 1646 de limn, incluidos
doctests; no aumentan las 1120 históricas de RUNE ni sus 18 capturas Mock. El host
posterior 1128 se registra aparte, sin incorporar todavía el receiver. El
[registro canónico](evidence/limn-drop-receiver-weston-2026-10-15.json) conserva
fuentes, verificación CLI y límites. La incompatibilidad observada en Hyprland
sigue requiriendo un owner SDK compartido de clipboard/DnD y routing por
superficie. PR pública no significa merge, pin actualizado ni consumidor RUNE.

La [PR #62 Draft abierta](https://github.com/Niiihuel/limn/pull/62), HEAD
`06a08883634c5e5c2abe5afb4a5d624a68e98aef`, se apila sobre PR #55 y publica
el owner DnD multiventana: un owner/worker por display y un data_device por
seat compatible, con endpoints y exclusión de display. La
[CI del HEAD exacto](https://github.com/Niiihuel/limn/actions/runs/38814443370/job/116695462384)
terminó SUCCESS el 2026-10-15 a las 22:06:00 UTC. Las 142 pruebas DnD se ejecutaron
dentro de 1664 workspace/5 ignoradas preexistentes/21 suites; no-default 1331/2/11
es otra configuración, sin suma. Pasaron Clippy workspace y Gallery hot
estrictos, rustdoc estricto y formato DnD. Root aprobó dos PNG dark/light de
una página **estática y sintética**, sin adjuntar owner nativo ni probar clipboard.
[Registro canónico v2](evidence/limn-shared-wayland-owner-2026-10-15-v2.json).
El owner aún destruye Selection y no bindea Primary; no reemplaza el clipboard
del SDK stock y no se instala detrás de él. Faltan esa integración funcional,
reserva/ACK, pin y consumidor RUNE. El pin actual 6ae, las 1128 pruebas históricas,
las 18 capturas Mock y 07.3 parcial no se incrementan por este checkpoint.

SaveFile **08.1** ya verificó la API y un ejemplo de plataforma: 64 unitarias
y un doctest, destino exacto elegido sin crearlo y fixture existente intacto.
La cancelación tuvo terminal correcto, sin acreditar ventana observada en ese
intento tardío. [Registro](evidence/save-file-picker-checkpoint-2026-10-15.json).
El diálogo del SO no introduce una interfaz de conversaciones ni una extensión
nueva de limn. Core/UI y el dueño único NativeDialogs 08.2 siguen pendientes,
igual que el reemplazo 08.3; las dos franjas GTK no suman capturas Mock. El
inventario actual de 18 incluye la negativa 07.3.2 descrita arriba.
[Contrato](../tasks/save-as-contract.md).

## Inventario histórico de carencias y seguimiento

La tabla nació el 2026-10-14. La fila de adjuntos se actualizó el 2026-10-15
y las de Markdown, `Avatar` e imágenes en línea el 2026-10-18; el estado
actual del pin y las PR se distingue arriba. Las demás propuestas se
revalidan contra las fuentes al iniciar su incremento.

| Carencia | Efecto hoy en RUNE | Propuesta |
| --- | --- | --- |
| `UiEvent::Key` no lleva Alt | Resuelto por la PR #66 (2026-10-16): Alt+flechas llega en el evento de limn | — |
| `Markdown` no tiene menciones ni emojis personalizados | Resuelto: RUNE corta los tramos de `Markdown` en cada mención (`text::decorate`) y el emoji de un servidor va como imagen con `TextSpan::image` (PR #79) | — |
| `Markdown` no tiene subrayado, tachado ni spoilers | Resuelto por la [PR #82](https://github.com/Niiihuel/limn/pull/82) (2026-10-18): subrayado, tachado, negrita cursiva y subtexto; los spoilers siguen en RUNE (`text.rs`), sobre los tramos | — |
| Sin ícono de enlace externo | Resuelto por la [PR #69](https://github.com/Niiihuel/limn/pull/69) (2026-10-16): `Icon::ExternalLink` (`box-arrow-up-right`) al final de los botones de enlace de los componentes | — |
| `Button` sin imagen antes de la etiqueta | Resuelto por la [PR #71](https://github.com/Niiihuel/limn/pull/71) (2026-10-16): `Button::image(RasterImage)` en el lugar del ícono; los botones de los componentes muestran el emoji propio como imagen, `:nombre:` mientras carga | — |
| Emoji en color y composición de secuencias | Resuelto por la PR #67 (2026-10-16): Twemoji sobre el glifo de cada grafema; lo que no está en el paquete conserva Noto Emoji monocromo | — |
| `Avatar` sin color propio | Resuelto el 2026-10-18 (paquete 10): `Avatar::color(Color)`; RUNE ya no pasa `selected` = color del usuario | — |
| Entrada de archivos por arrastre al composer | Adjuntos múltiples 07.2 verificados con selector GTK y Mock; Core Paths probado, host 07.3.2 rechaza todo legacy sin posición ligada al lote. Rechazo/pulido nativos verificados, importación funcional abierta; pegado y Windows/macOS no verificados | PR #49 aporta opt-in, guard de superficies y orden del bridge. Pin actual `6ae42859` conserva PR #50 por cherry-pick propio para altura/avisos e integra procedencia/orden con foco del host; no es merge público de PR59. El host limpia metadata antes del bridge y protege drop+Enter, sin llamar Core Paths. Falta backend integrado con posición/lote reales; FileDropReceiver está en PR #55 Draft, verificado en Weston privado; PR #62 Draft añade owner DnD multiventana y 142 pruebas, con galería sintética. Faltan clipboard/Primary funcionales, convivencia SDK en Hyprland, reserva/ACK, pin y consumidor RUNE, separados de migrar winit/egui. Históricos: 07.2 `2618b37`, 07.3.1/08.1 `1ad33eb` |
| Una fila dentro de la columna de un mensaje no recibe un ancho acotado, así que `shrink_to_fit` no actúa | La línea "No se envió…" empujaba Reintentar/Descartar fuera de la vista (visto en captura). RUNE le da el ancho del mensaje explícitamente, como ya hacía con `Markdown::width` | Revisar si una columna flexible debería medir a sus hijos con su propio ancho |
| `measured_list`: una sola estimación de altura por lista y sin revisión de contenido por fila | Requiere medir casos con imágenes o ediciones fuera de vista | `.estimate(|index| f32)`, `.row_revision(|index| u64)` ([SEREIN_COMPARISON.md](SEREIN_COMPARISON.md#ideas-para-limn), idea 6). El pin histórico `0da2317` ya podaba alturas huérfanas al superar `2 × filas + 256`; esa parte de la propuesta estaba resuelta. |
| Sin imágenes en línea en el texto ni imágenes por referencia | Resuelto en parte por la PR #79: `TextSpan::image` y `TextSpan::surface` pintan el emoji de un servidor en el texto | `ImageRef` + `UiTree::set_image` siguen como idea; la cola y limpieza de caché del PR #39 ya están integradas ([SEREIN_COMPARISON.md](SEREIN_COMPARISON.md#ideas-para-limn), ideas 1, 3 y 4) |

### Adopción de la integración del 2026-10-16 (PR #66)

RUNE depende de `rune/integration-2026-10-16`, que integra la PR #66 de
limn. Con ella se quitaron estos rodeos del host:

- `UiEvent::Key { alt, .. }`, `Key::F6`, `Key::Slash`, `Key::F12` y
  `UiEvent::PointerBack`/`PointerForward`: el host lee Alt+flechas, F6,
  Ctrl+/, F12 y los botones laterales del evento traducido, en su posición
  de origen (`host_shortcut` en `apps/desktop/src/main.rs`), en lugar del
  evento nativo de egui.
- Regiones `Role::Region` con nombre en español; `cycle_focus` toma los
  controles de cada una con `focusables_within`, no por contención de
  rectángulos. El host intercepta F6 antes que el árbol para aterrizar en el
  servidor o canal elegido, el mensaje más nuevo a la vista o el campo del
  composer, en lugar del primer control de la región.
- Las animaciones se dibujan con `SurfaceFit::Contain` y el radio de la
  caja; ya no se calcula una densidad por caja. Cada cuadro se convierte una
  vez y su `SurfaceFrame` se clona en cada vuelta. El visor de adjuntos
  reproduce el GIF con `ImageViewer::surface`
  ([captura](img/limn-adoption-2026-10-16.png): animación local redondeada
  en la línea de tiempo).
- El visor de PDF nombra sus flechas con `navigation_labels`; la opción no
  elegida de una encuesta usa `Icon::Circle`.

Seguía en el host: grabar la tecla de Pulsar para hablar, porque `limn::Key`
no tenía dígitos, la mayoría de la puntuación ni teclas más allá de F12, ni
evento de liberación. Resuelto el 2026-10-19 por el paquete 6
([abajo](#teclado-completo-modificadores-superficie-en-espejo-y-botón-animado-pr-86-2026-10-19)).

Diferencia observada: `SurfaceFit::Contain` agranda un cuadro más allá de sus
píxeles, mientras que un `RasterImage` en la misma caja no pasa de su tamaño
(`fit_image` lo limita a escala 1). En RUNE no se nota en la línea de tiempo,
porque la caja de la vista previa nunca supera los píxeles de la imagen, pero
en el visor a tamaño completo un GIF pequeño se veía más grande que su cuadro
quieto. Resuelto el 2026-10-18: el visor usa `SurfaceFit::ScaleDown`
(paquete 10).

### Emoji a color: PR #67 (2026-10-16)

La [PR #67](https://github.com/Niiihuel/limn/pull/67), rama
`feat/color-emoji-text`, añade `limn_egui::set_color_emoji(Some(fuente))`:
el host da un PNG por grafema extendido y limn-egui lo pinta sobre el glifo
en cada `DrawCommand::Text`. El layout del árbol no cambia: el glifo sigue
ocupando su lugar, se pinta transparente y la imagen se ajusta a ese lugar
(de 1 a 1,2 em), centrada en la línea y con el alfa del texto. Por eso no
hizo falta convertir etiquetas, botones o tooltips a texto con formato, y el
texto editable del composer también sale en color. limn no incluye arte; su
galería Text usa discos sintéticos.

RUNE dependía de `rune/integration-2026-10-16-color-emoji`, creada desde
`rune/integration-2026-10-16` con un merge de esa rama. El paquete Twemoji,
el índice y la búsqueda son de RUNE; ver [ARTWORK.md](ARTWORK.md#emoji-a-color-twemoji).

### Acciones del visor de imágenes: PR #72 (2026-10-17)

«Guardar como…» desde el visor necesitaba un control propio en el
encabezado de `ImageViewer`. La [PR #72](https://github.com/Niiihuel/limn/pull/72),
rama `feat/image-viewer-actions` sobre `main`, añade
`ImageViewer::action(id, icon, nombre)`: controles del host antes de los de
zoom, cada uno informa su ID. Prueba, guía, changelog y galería incluidos.
RUNE dependía de `rune/viewer-actions` (commit `514993f`): la rama de
integración `rune/integration-2026-10-16-color-emoji` (`70ef22d8`) más ese
commit.

### Código coloreado y selección del host: PR #73 y #74 (2026-10-17)

Las acciones de mensaje (P4) necesitaban dos cosas de limn:

- [PR #73](https://github.com/Niiihuel/limn/pull/73), rama
  `feat/markdown-code-highlight` (`649adc0`) sobre `main`: el bloque de
  código del Markdown se colorea por el lenguaje de su cerca (Rust, JS, TS,
  TSX, Python, JSON, TOML, CSS, HTML y Markdown, con sus alias) con el
  analizador por línea de limn, también sin la feature `syntax` (RUNE la
  apaga). Hasta 64 KiB o 2000 líneas; más, o un lenguaje desconocido, queda
  como antes. Colores por paleta clara u oscura, con contraste 4,5:1.
- [PR #74](https://github.com/Niiihuel/limn/pull/74), rama
  `feat/input-selection` (`9dabb01`) sobre `main`:
  `UiTree::set_input_selection(id, inicio, fin)`, para que el texto
  envuelto por Ctrl+B y los demás atajos siga seleccionado.

RUNE depende ahora de `rune/code-highlight` (`afba53f`): `514993f` más los
dos commits cherry-pickeados (`5e7fade` y `afba53f`). La rama de integración
es local (`file://`), sin publicar; las dos PR sí lo están.

### Fuente de la interfaz: `font_definitions_with` (2026-10-17)

Elegir la fuente de la interfaz (P6) necesitaba poner caras propias primero
también en las familias de peso medio y negrita, cuyos nombres son privados
de limn-egui. `limn_egui::font_definitions_with(UiFaces { regular, medium,
bold })` devuelve las definiciones de siempre con esas caras delante (un
peso que falta usa la regular; monoespaciada intacta) y RUNE las instala
con `Context::set_fonts`. Test, Theming, la tabla del backend y el
changelog. [PR #76](https://github.com/Niiihuel/limn/pull/76), rama `feat/ui-faces` sobre `main`
(`13257c3`); RUNE apunta a `rune/ui-faces` (`e56128c`): `afba53f` más ese
commit.

### Imágenes dentro de un texto: PR #79 (2026-10-17)

Los emoji de un servidor se veían como la pastilla `:nombre:` porque un
`TextSpan` no podía llevar una imagen, y la biografía se armaba como
palabras e imágenes sueltas. `TextSpan::image(raster)` y
`TextSpan::surface(id)` pintan una imagen (o la superficie del host, para
una animación) en lugar del texto del tramo: un cuadrado de
`INLINE_IMAGE_SCALE` (1,375) veces su tamaño de letra, centrado en la línea
y que no se parte; su texto (`:nombre:`) sigue siendo lo que se copia (la
imagen entera o nada) y lo que lee la accesibilidad; el caché de texto
formateado la distingue. Tests, Layout, changelog y la sección «Inline
pictures» de la galería (Text).
[PR #79](https://github.com/Niiihuel/limn/pull/79), rama
`feat/inline-text-images` sobre `main` (`430aea5`; sin `SurfaceFit`, que
llega con la #66). RUNE apunta a `rune/inline-images` (`f601989`):
`caefe2b` más `bb57cd2` (la superficie encajada con `SurfaceFit::Contain`)
y el ajuste de la galería.

### Caras en un tooltip: PR #80 (2026-10-17)

El tooltip de un servidor en el riel nombraba a quienes estaban en voz,
porque una fila de un tooltip sólo tenía íconos y etiquetas (el
`limn-gap` de `rail.rs`). `Tooltip::faces(icon, faces, more)` agrega una
fila pasiva: el ícono (20 px), caras redondas de 24 px superpuestas 6 y
recortadas por un anillo del fondo del tooltip (`TooltipFace`: imagen, o
la inicial sobre su color) y «+N» en una pastilla; las que no entran en
el ancho se suman a «+N». Tests, docs de Tooltip, changelog y la sección
«Faces» de la galería.
[PR #80](https://github.com/Niiihuel/limn/pull/80), rama
`feat/tooltip-faces` sobre `main` (`5ffa75b`). RUNE apunta a
`rune/tooltip-faces` (`1af991c`): `f601989` más ese commit.

### Adopción de lo que trae `rune/tooltip-faces` (2026-10-18)

La rama fijada (`1af991c`) ya traía piezas que RUNE seguía componiendo a mano
con un `// limn-gap:`. Se adoptaron sin tocar limn ni `Cargo.toml` (rama
`adopcion-limn`):

- Íconos: `Icon::Lock` (canal privado en sus ajustes, al crear uno, en
  foros e hilos), `Icon::Neutral` (el «heredar» de un permiso, la barra
  `slash-lg`), `Icon::ChevronUp`, `Icon::Invite`, `Icon::Leave` y
  `Icon::DragHandle` (roles y canales que se arrastran). Se borraron los SVG
  propios `LOCK`, `SLASH` y `CHEVRON_UP`.
- Candado en la barra: un canal cuyo `@everyone` no tiene `VIEW` lleva
  `Icon::Lock` después del nombre y la plataforma oye «Canal privado»;
  oculto y listado con «Mostrar los canales ocultos», lleva el candado y el
  ojo tachado, como Serein («Hidden · Limited»). `Store::everyone`,
  `Store::private_by` y `Store::private` (en `rune-state`) son lo que antes
  era sólo de los ajustes del canal, que ahora los usan
  ([captura](img/adopcion-limn-candado-2026-10-18.png): `#moderación` de la
  simulación, Xvfb privado).
- Menús: `DropdownMenuItem::checkbox` (opciones que se prenden y apagan, en
  canales y en voz) y `::radio` (el nivel de notificaciones); `::description`
  también en un ítem que abre un submenú; `DropdownMenu::at` con el estilo
  contextual para los menús del clic derecho; `::description` +
  `::tooltip_with` para el motivo de un ítem del menú del servidor que no se
  puede usar (la plataforma oye el motivo entero aunque la línea se corte).
  El menú de estado del usuario se abre al pasar el puntero
  (`popover_on_hover`) y sigue abierto mientras se usan las duraciones de
  cada estado, anidadas en él; un clic lo deja fijo como antes.
- El clic derecho en un mensaje: `ContextMenu::open_action` pide el menú y
  RUNE abre el completo, el mismo de «Más» (reacciones rápidas como
  `DropdownMenuEntry::custom`, el submenú de reacciones, borrar con
  `::destructive`), con `DropdownMenu::at` en el punto del clic. Pedido desde
  el teclado, sin punto, se abre el de «Más». El clic derecho deja el foco
  donde estaba (casi siempre el compositor): mientras el menú está abierto,
  RUNE no toma las flechas ni Escape, que van al menú. Un mensaje que todavía
  no se envió no ofrece reacciones rápidas, y uno sin nada que ofrecer no
  abre menú. Otro menú del clic derecho o borrar el mensaje lo cierran.
- Formularios: `SearchField::clearable` (buscador de los ajustes),
  `Element::leading_icon` (ya estaba en uso; sobraba el comentario),
  `Select` con `SelectVariant::Field` (modo lento con `height(41)`, listas de
  los ajustes con `ControlSize::Spacious`, sin el ícono que sólo servía para
  alinear), `RadioOption::icon` (`#` y altavoz al crear un canal),
  `Switch` `Spacious` con `SwitchVariant::Accent` y la tinta del acento, y
  `Slider::mark_labels` + `default_stop` en lugar de las marcas compuestas
  (la marca predeterminada sale en el acento, la tinta del slider, en vez
  del verde de antes).
- Color y pestañas: `SwatchPicker` en los colores de un servidor nuevo;
  `ColorPicker` (cuadro de saturación y brillo, barra de tono y hex) en el
  color personalizado de los seis selectores (perfil, acento propio, acento
  del servidor, color de un rol y los dos del degradado). `rune-ui`
  (`color_picker.rs`) convierte el cuadro y la barra en el cambio del campo
  hex que cada página ya atendía, y guarda el `Hsv` mientras se usa para que
  un gris o el negro no pierdan el tono. Las cinco páginas con borrador lo
  aplican en vivo; el color propio del avatar es del proveedor y se pide al
  soltar el cuadro o la barra (o con Enter en el hex), sin cerrar el
  selector. El hex sigue la regla de las páginas: seis cifras. `TabsVariant::Underlined` en el
  editor de roles, con anchos estimados por caracteres como amigos y perfil
  ([captura](img/adopcion-limn-roles-2026-10-18.png): pestañas y selector
  de color de un rol, simulación en Xvfb privado).
- `Markdown::font_size` reemplaza el escalado del árbol en mensajes y
  tarjetas; un test comprueba que `# Título` sale como encabezado, con su
  proporción a cualquier tamaño, y sin la marca.
- `ScrollbarVisibility::AutoHide { hold_ms: 600, fade_ms: 200 }` en la línea
  de tiempo.

Lo que no alcanzó quedó con su `limn-gap` para el paquete 8
(`limn-avisos-y-compositor`), que lo resolvió
([abajo](#avisos-insignias-íconos-chicos-nivel-del-slider-y-compositor-pr-85-2026-10-18)):

- `Switch` llega a 44×24 (`Spacious`); la referencia mide 50×26.
- `Select` no tiene un número de filas a la vista: la lista del modo lento
  muestra lo que entra en un menú contextual; la referencia, cinco.
- `SelectVariant::Field` dibuja el valor a 13 px fijos (la referencia y la
  versión compuesta, 16) y, abierto, no da vuelta el chevrón ni pone el
  borde de 2 px en el acento.
- Íconos de menos de 16 px: el candado de la barra va después del nombre y
  no sobre el `#`, como en Discord y Serein.
- La barra que se oculta no se despierta con sólo poner el puntero sobre el
  chat (oculta no tiene región).

### Marcas del texto, tooltip de un tramo, «…» y relleno del resaltado: PR #82 (2026-10-18)

Cuatro `limn-gap` de `rune-components/src/text.rs`, en una PR y no en
cuatro, como pedía el plan: las cuatro agregan campos a los mismos tipos
(`TextSpan`, el fragmento de texto formateado y su caché) y tocan el mismo
bucle de disposición y pintura de `tree.rs`, la misma entrada del
changelog y la misma página de la galería; separadas desde `main`
chocarían entre sí y habría que apilarlas igual. Si el dueño las prefiere
separadas, se parten en PRs apiladas.

- `Markdown` entiende `__subrayado__`, `~~tachado~~`, `***negrita
  cursiva***` y líneas `-# subtexto` (0,8125 del cuerpo, en el gris
  apagado), y las marcas se anidan (`__**ambas**__`). `**negrita**` usa la
  cara negrita y `*cursiva*` la itálica (antes eran sólo un color); hay
  una familia nueva, `FontFamily::SansBoldItalic`. Una dirección web ya no
  se lleva los `|` de su final: un enlace dentro de `||spoiler||` se
  ocultaba con los `||` a la vista porque el Markdown metía el `||` de
  cierre en el enlace (el corte estaba en limn, no en el preprocesado de
  RUNE). Una marca no abre ni cierra dentro de un `<…>` sin espacios,
  como `<:pepe__love:1>`: los nombres de emoji admiten `_`, y el `__` del
  nombre subrayaba hasta el `__` siguiente y partía el token (Discord
  resuelve el emoji antes que las marcas).
- `TextSpan::underline()`, `.strikethrough()` y `.tooltip(título,
  paleta)`: el tooltip aparece tras la espera de siempre sobre la parte del
  tramo bajo el puntero; un tramo sin acción es un objetivo pasivo que deja
  seleccionar el texto, con un id propio de su texto, y con el puntero el
  tooltip sólo sale en el texto que está debajo (dos «(editado)» del mismo
  minuto, o el mismo `<t:…>` en dos mensajes, no muestran dos). RUNE pone ahí la fecha completa
  de un `<t:…>`, que conserva su acción `moment:` (el aviso al hacer clic o
  con Enter) para que el teclado llegue a él y vea el tooltip, y la de la
  edición en «(editado)», que no se enfoca.
- `Element::max_lines(n)`: un texto formateado de `n` líneas que termina en
  «…» (que no se copia). El tema del canal con emoji lo usa en lugar de
  recortar con `max_height`; la línea de estado con emoji puede usarlo
  después (paquete emoji-y-estados).
- `TextHighlight::background`: un relleno por rango en un `TextArea` (y en
  el editor del `Composer`). Cada `@palabra` y `#palabra` del borrador se
  ve sobre el relleno de una pastilla; no sabe si alguien se llama así, así
  que una que no nombra a nadie también se rellena y sale como texto.

Tests, docs (Layout, Markdown, TextArea), changelog y galería (Text,
Markdown, TextArea). [PR #82](https://github.com/Niiihuel/limn/pull/82),
rama `feat/limn-texto` sobre `main` (`90d4408`, sin las imágenes en línea
de la #79): `61933ff` más las correcciones de la revisión, `9ff3d94` y
`c463979`, y `d7a54c6` (el changelog con sólo su entrada). RUNE apunta a
`rune/limn-texto`: `1af991c` más `a4fa20e` (el mismo cambio que `61933ff`,
adaptado a esa base, que ya tiene las imágenes en línea), `4b3bb4c` y
`152c5a8` (los mismos que `9ff3d94` y `c463979`).
[Mensaje con las marcas](img/limn-texto-2026-10-18.png) (el texto del
mensaje de la simulación, escrito en el compositor: por eso es de la
cuenta y no dice «(simulado)») y
[borrador con pastillas](img/limn-texto-composer-2026-10-18.png).

### Tipografía y opciones: seminegrita, lenguajes, `ScaleDown`, pista por opción y `Avatar::color`: PR #84 (2026-10-18)

Paquete 10 del plan (`limn-tipografia-y-opciones`), en una rama de limn:

- **Seminegrita.** `FontFamily::SansSemibold`, entre medium y negrita.
  Ubuntu no tiene ese peso; limn-egui trae Ubuntu Sans SemiBold 1.006, su
  sucesora del mismo proyecto y con la misma licencia (UFL 1.0, sin
  modificar, 487.284 bytes; procedencia y hash en el `NOTICE.md` de los
  recursos). Sus letras se parecen a las de Ubuntu sin ser idénticas, por
  eso sólo se usa donde se pide seminegrita. La cara del título de un
  tooltip la elige `Tooltip::title_font_family`, del paquete 8 (abajo). `UiFaces`
  suma `semibold`: RUNE pide a fontconfig el peso 180 de la fuente elegida
  (`rune_platform::fonts::load`) y, si la familia no lo tiene, limn usa su
  negrita, después su medium. RUNE la usa sólo donde la REA la pide: el
  título de los tooltips del carril (servidores, carpetas y «Añadir un
  servidor») y el nombre de la tarjeta de actividad.
- **Lenguajes.** El analizador por línea suma Go, Java, Kotlin, Swift, C,
  C++, C#, PHP, Ruby, Lua, Shell, YAML, SQL, Diff, Dart y Zig, con los
  alias de la cerca (`golang`, `c++`, `c#`, `bash`, `yml`, `patch`…): los
  24 de Serein, consultados sin copiar código, más TSX y Markdown. Cada uno
  se describe con una tabla chica (comentarios, bloques y si se anidan,
  comillas triples, `#` y `'` sólo al empezar una palabra en shell y YAML,
  directivas de C, palabras clave sin distinguir mayúsculas en SQL y PHP);
  un diff colorea líneas enteras. Mismos límites (64 KiB, 2.000 líneas) y
  la misma tinta por tema. En RUNE llega solo: los bloques de código de
  los mensajes ya pasaban por `CodeLanguage::from_code_fence`.
- **`SurfaceFit::ScaleDown`.** Encaja sin agrandar más allá de sus píxeles,
  como se muestra un `RasterImage`. `ImageViewer::surface` la usa, así que
  en el visor de adjuntos un GIF chico queda del tamaño de su cuadro quieto.
- **`DropdownOption::tooltip`.** Cada opción recortada de las listas de
  dispositivos del panel rápido de voz dice su nombre entero en su pista
  (antes sólo el elegido).
- **`Avatar::color`.** El fondo de la inicial; RUNE ya no pasa una paleta
  con `selected` igual al color de la persona (sólo el blanco de la letra).

Tests (limn y RUNE), docs (Components, Layout, Theming, Backend y las
guías), changelog y galería (Text, Tooltip, Select, Avatar, ImageViewer,
Markdown). RUNE apunta a `rune/limn-tipografia-y-opciones` (`3284386`:
`rune/hover-dim` `637990d` más ese commit), en el clon local
(`file:///home/nihuel/projects/personal/lienzo`); el integrador la sube y
cambia `Cargo.toml` a GitHub. La
[PR #84](https://github.com/Niiihuel/limn/pull/84) (rama
`feat/limn-tipografia-y-opciones`, `aca505f`) va desde `main`, que no tiene
`UiFaces` (#76) ni las superficies encajadas (#66): lleva todo menos
`UiFaces::semibold` y `ScaleDown`, que siguen en la rama de integración
hasta que esas dos se fusionen.

### Avisos, insignias, íconos chicos, nivel del slider y compositor: PR #85 (2026-10-18)

Paquete 8 del plan (`limn-avisos-y-compositor`): los `limn-gap` del
carril, la barra de canales, el selector de emoji, los ajustes y el
compositor, más lo que la adopción de `rune/tooltip-faces` dejó pendiente.
Una PR y no ocho: todas son opciones nuevas de componentes que ya existen,
con el mismo changelog y la misma galería; partirlas sólo apilaría
conflictos en `tree.rs`, `element.rs` y `lib.rs`. Cada opción deja el
comportamiento por defecto como estaba (salvo el chevrón de un `Select`
de campo abierto, que ahora apunta hacia arriba). **El dueño tiene que
confirmar las opciones nuevas del tooltip.**

- `Tooltip::delay_ms`, `title_font_size`, `title_font_family`, `arrow` y
  `background` (`TooltipOptions` en `ElementKind::Tooltip`): el árbol
  espera la demora de cada tooltip. Los tooltips del carril abren al
  instante, con el título a 16 px en seminegrita (`HINT_TITLE`, paquete 10)
  sobre `#0a0a0c` en los aspectos oscuros (`Look::hint`), como el oficial;
  la flecha ya la tenía limn y se conserva.
- `Badge::placement` (`BadgePlacement`), `height`, `outline(color,
  ancho)`, `ink`, `font_size` y `font_family`: las menciones del carril son
  un `Badge` abajo a la derecha, 17 px blanco sobre rojo con un anillo de
  3 px del color del carril. El número queda a 3 px del anillo (antes, a 4
  dentro del relleno: la pastilla de «99+» es 2 px más angosta).
- `Element::small_icon` (8 a 16 px): el altavoz del carril a 12 px y el
  candado de un canal privado a 10 px sobre la esquina del `#` (antes iba
  después del nombre). `Icon::Tree`, `FriedEgg`, `Gamepad`, `Car`,
  `Lightbulb` y `Flag` (Bootstrap Icons 1.13.2): las categorías del
  selector de emoji, en vez de un emoji a color cada una; el `Gamepad`
  reemplaza el SVG propio de juegos (`assets/gamepad.svg`, borrado).
- `Slider::level`: el umbral de voz a mano es un `Slider` que dibuja el
  nivel del micrófono sobre su pista (verde por encima del umbral, amarillo
  por debajo); el medidor aparte queda sólo para el umbral automático.
- `Switch::track_size`: el interruptor de los ajustes mide 50×26.
- `Select::max_visible_items`, `font_size` e `ink`: los `Select` de campo
  (modo lento y demás) muestran el valor a 16 px, cinco filas a la vista y,
  abiertos, el chevrón hacia arriba y el borde de 2 px en el acento.
- `ScrollbarVisibility::AutoHideUnlessHovered`: la barra del chat aparece
  también con el puntero encima.
- `Composer` con `inline_controls` y `auto_grow`: el editor crece entre
  sus controles, que quedan junto a la primera línea; `editor_padding` y
  `card_corner_radii`. RUNE pasa al `Composer` de limn con la misma
  geometría (caja de 48 px, el texto a 74 px del borde, «+» y el emoji
  centrados en sus columnas de 60 y 52, esquinas de arriba rectas bajo la
  respuesta), y un aviso largo del compositor («Estás aislado hasta… ·
  quedan…») pasa a la línea siguiente en ventanas angostas en vez de
  cortarse. `SuggestionItem::leading`, `ink` y `detail`: las sugerencias de
  miembros llevan avatar, el nombre en el color del rol (si los colores de
  rol están activos) y el usuario a la derecha; los roles, su color.
- Sin limn: el menú de un mensaje («Más» o el del clic derecho) se cierra
  cuando su fila sale de la lista (`messages::forget_unshown_menus`, después
  de cada disposición: si el menú ya no tiene superficie, no vuelve con la
  fila).

Tests, guías, changelog y galería en limn.
[PR #85](https://github.com/Niiihuel/limn/pull/85), rama
`feat/limn-avisos-y-compositor` sobre `main` (`90d4408`): `952daf5`. RUNE
apunta a `rune/limn-avisos-y-compositor`: `637990d` (la rama
`rune/hover-dim`) más `c51d1ab` (el mismo cambio, adaptado a esa base, que
ya tiene las caras del tooltip y los tooltips de un tramo) y `3cbe6ed`
(guías, galería y changelog). Capturas con la simulación en Xvfb privado:
[tooltip del carril](img/limn-avisos-riel-2026-10-18.png),
[sugerencias de miembros](img/limn-avisos-sugerencias-2026-10-18.png) y
[categorías del selector de emoji](img/limn-avisos-emoji-2026-10-18.png).

### Teclado completo, modificadores, superficie en espejo y botón animado: PR #86 (2026-10-19)

Paquete 6 del plan (`limn-teclado-y-superficies`). Una PR, como la #85:
son cuatro agregados chicos que tocan los mismos archivos de limn.

- `limn::Key` tiene todas las teclas de egui (dígitos, puntuación,
  `Insert`, F13–F35), con `Key::name` (los nombres de egui) y
  `Key::from_name`; `UiEvent::KeyReleased` y `UiEvent::ModifiersChanged`, y
  el árbol guarda lo sostenido (`UiTree::key_held`, `UiTree::modifiers`,
  que se vacían al perder la ventana el foco; `observe_keys` para lo que el
  host contesta antes que el árbol). En RUNE el grabador de teclas y Pulsar
  para hablar en la ventana (también silenciar, ensordecer y pulsar para
  silenciar) leen sólo de limn: se fue `NativeHostEvent::Key` y `host_key`
  de `apps/desktop/src/main.rs`, y `ptt.rs` ya no conoce `egui::Key`. Los
  atajos guardados siguen leyéndose: el nombre de cada tecla es el de egui.
  Un atajo de ventana actúa una vez por pulsación, nunca por la repetición
  de una tecla sostenida.
- Con Shift sostenido, la barra sobre un mensaje cambia las reacciones
  rápidas por las acciones directas que la cuenta puede hacer (fijar o
  desfijar, marcar como no leído, copiar enlace, texto e ID, eliminar sin
  preguntar), en el orden de Serein (`ui.messages.shift_held`, de
  `ModifiersChanged`; perder el foco lo suelta;
  [captura](img/limn-teclado-shift-2026-10-19.png) con la simulación en
  Xvfb privado, `--capture-hover --action shift`).
- `Element::surface_mirrored`: la vista previa de la propia cámara (la del
  panel rápido, la del escenario y la de Ajustes › Voz y video) se ve en
  espejo, como en Serein; lo que se manda a la llamada no cambia.
- `Button::surface` y `DropdownOption::image`/`surface` (también
  `DropdownMenuItem`): el emoji de un servidor en un botón o en una opción
  de un menú de una aplicación se mueve cuando es animado (el proveedor no
  oficial pide su imagen con `animated=true`), salvo con el movimiento
  reducido o «Animar GIF» apagado, como el resto de lo animado; las
  opciones abiertas muestran la imagen en lugar de `:nombre:`.

Tests, guías, changelog y galería en limn.
[PR #86](https://github.com/Niiihuel/limn/pull/86), rama
`feat/limn-teclado-y-superficies` sobre `main`, apilada sobre las #66, #71
y #79 de las que depende: `9f3f822`. RUNE apunta a
`rune/integration-2026-10-19` (`ed11ee8`), que mezcla
`rune/limn-teclado-y-superficies` (`8128560`, el mismo cambio sobre
`rune/hover-dim`).

### Rama de integración del 2026-10-19

`rune/integration-2026-10-19` (`a7464c5`) une
`rune/limn-tipografia-y-opciones` y `rune/limn-avisos-y-compositor` sobre
`rune/hover-dim`. Las dos daban al título del tooltip una cara propia:
`Tooltip::title_family` en `TooltipStyle` (PR #84) y
`Tooltip::title_font_family` en `TooltipOptions` (PR #85). La integración
se queda con la segunda, que viaja junto al tamaño del título, y quita la
primera (galería, guía y changelog incluidos); la PR #84 tiene que hacer lo
mismo cuando la #85 entre en `main`, o la #85 al revés. El carril la usa
con las dos cosas: 16 px, al instante y en seminegrita. Tests de limn en la
rama: 1800 aprobados, 5 ignorados. Vive sólo en el clon local: falta
subirla y cambiar `Cargo.toml` a GitHub.

Desde `ed11ee8` mezcla también `rune/limn-teclado-y-superficies`
(paquete 6): `DropdownOption::item()` lleva a la vez el `tooltip` del
paquete 10 y la imagen o superficie del 6.
### Texturas en pantalla (2026-10-19)

`rune/emoji-parpadeo` (`a99d980`, desde `ed11ee8`) arregla el parpadeo del
selector de emoji con la cuenta real. La caché de texturas de `limn-egui`
expulsaba la imagen insertada hace más tiempo al pasar el límite que fija
`set_image_cache_entries` (64 en RUNE), aunque se estuviera dibujando: una
pantalla con más imágenes que eso se expulsaba a sí misma en cada paso,
volvía a decodificar y pedía otro cuadro sin parar. Ahora expulsa lo dibujado
hace más tiempo y nunca lo dibujado en el paso actual o el anterior.
Medición con 90 imágenes de 128 px a 64 entradas, 60 pasos por segundo:
antes, unas 1780 texturas subidas y liberadas por segundo y un cuadro pedido
en cada paso, sin asentarse nunca; ahora, 90 subidas una sola vez y ningún
cuadro pedido desde el segundo segundo. Test en limn
(`more_pictures_on_screen_than_the_limit_settle_and_leave_once_off_screen`)
y en RUNE (`image_cache_tests.rs`). PR #91 desde `fix/raster-cache-on-screen`
(`0cee5a2`, el mismo cambio sobre `main`). `rune/emoji-parpadeo` vive en el
clon local; desde el 2026-10-19 está en `rune/integration-2026-10-19` (avance rápido a `a99d980`), que falta subir.

### Visor de imágenes: `Element::zoom_view` e `ImageViewer` nuevo (2026-10-19)

`rune/visor-de-medios` (desde `1407eef`, la cabeza de
`rune/integration-2026-10-19`) rehace el visor con Loupe, Discord, Telegram,
iced, egui, Zed y Oculante como referencia ([VISOR.md](VISOR.md)).
`Element::zoom_view` guarda el zoom en el árbol, como un desplazamiento:
rueda anclada, pellizco (`UiEvent::Zoom`), arrastre con inercia, doble clic,
teclas, límites y animaciones con movimiento reducido;
`UiAction::Zoomed` informa la escala. `ImageViewer` lo usa a pantalla
completa sobre un fondo oscuro, con barra, porcentaje (`zoom_level`),
`subtitle`, `picture_size`, `placeholder`, `thumbnail`, `footer` e
`initial_focus`; `ImageViewer::zoom` y `MAX_ZOOM` quedan deprecados y RUNE
dejó de guardar el nivel. limn-egui dibuja la imagen completa agrandada,
con vecino más cercano desde 2 píxeles por píxel, una copia reducida al
achicar, damero detrás de píxeles transparentes y giro por cuartos.
`Icon::Forward`. Tests de limn en la rama: 1915 aprobados.

RUNE apunta temporalmente a `file:///home/nihuel/projects/personal/lienzo`
con esa rama (`14c2a6e`); falta subirla y volver `Cargo.toml` a GitHub.
PR #112 desde `feat/visor-de-medios` (el mismo `14c2a6e`), apilada sobre
`rune/integration-2026-10-19`, porque usa teclas de la #86, `ScaleDown` de
la #84 y `ImageViewer::action`, que todavía no están en `main`.

### Carencias encontradas en la tercera iteración

Al reproducir los flujos de Discord, RUNE compuso estas piezas con lo que
limn tiene y las marcó en el código con `// limn-gap:`. Estado al
2026-10-18:

| Grupo | Carencia | Dónde se compuso en RUNE | Estado |
| --- | --- | --- | --- |
| Íconos | `Lock`, `Slash`, `ChevronUp`, `PersonPlus`, salir (puerta con flecha), agarre para arrastrar | `channel_settings.rs`, `server_menu.rs`, `server_settings.rs` | Resuelto: ya estaban en la rama fijada |
| Menús | ítem deshabilitado que dice por qué; ítems con casilla o radio; ítem de dos líneas que abre submenú; submenú por hover que sigue abierto mientras se usa uno anidado; menú abierto en el punto del clic derecho | `channel_menu.rs`, `profile.rs`, `server_menu.rs`, `voice.rs`, `message.rs` | Resuelto |
| Formularios | `TextField` con ícono inicial; `SearchField` con botón para limpiar; `Select` del alto de un campo con el valor a la izquierda; `RadioOption` con ícono; `Switch` de otro tamaño y colores; `Slider` con marcas y etiquetas | `server_menu.rs`, `user_settings.rs`, `channel_settings.rs`, `settings/pages.rs` | Resuelto (el `Switch` de 50×26 y las filas a la vista de `Select`, en la PR #85) |
| Color | selector de muestras (radio de colores) y selector completo (cuadro saturación/valor, barra de tono, hex) | `server_menu.rs`, `server_settings.rs` | Resuelto |
| Pestañas | variante subrayada de `Tabs` | `server_settings.rs` | Resuelto |
| Disposición | fila que reparte sus hijos en varias filas (flujo) | `profile.rs` | Resuelto antes (`Element::wrap`) |
| Markdown | tamaño de texto configurable | `message.rs` | Resuelto |
| Texto | Resuelto por la PR #82 (2026-10-18): `TextSpan::tooltip`; la fecha completa de un momento `<t:…>` y la hora de «(editado)» van en su tooltip | `text.rs` | Resuelto |
| Listas | el subtítulo de `ListRow` es sólo texto (haría falta `ListRow::subtitle_element`): un extracto de búsqueda dice `:nombre:` en lugar de la imagen del emoji (2026-10-18); el recorte con «…» de texto formateado (`max_lines`, PR #82) ya está en la rama fijada | `rune-ui/src/search/view.rs` | Sin PR |
| Teclado | `limn::Key` ya tiene letras, F1–F12 y `/` (PR #66), pero no dígitos ni el resto de la puntuación, ni evento de liberación ni estado «sostenida»: grabar un atajo y Pulsar para hablar se leían de egui | `apps/desktop/src/main.rs` (`host_key`), `apps/desktop/src/ptt.rs` | Resuelto (paquete 6, 2026-10-19) |
| Ortografía | `TextHighlight` cambia tinta, itálica y relleno (PR #82): falta subrayado ondulado por rango; falta saber en qué rango cae el clic derecho o el cursor (offset de carácter en el evento de menú contextual) y reemplazar un rango del valor conservando cursor y deshacer | evaluación de la pista 21 en `tasks/todo.md`; nada compuesto todavía | Sin paquete: el dueño decidió no tener corrector (2026-10-18; ver la [pista 21](../tasks/todo.md#21-spelling--ortografía-opcional-y-atajos-globales)) |

Lo que sigue faltando en limn, por paquete del plan del 2026-10-18:

- Paquete 4 (`limn-texto`): resuelto por la PR #82 (arriba).
- Paquete 6 (`limn-teclado-y-superficies`): resuelto el 2026-10-19
  ([arriba](#teclado-completo-modificadores-superficie-en-espejo-y-botón-animado-pr-86-2026-10-19)).
- Paquete 8 (`limn-avisos-y-compositor`): resuelto por la PR #85 (arriba).
- Paquete 10 (`limn-tipografia-y-opciones`): hecho el 2026-10-18
  ([PR #84, arriba](#tipografía-y-opciones-seminegrita-lenguajes-scaledown-pista-por-opción-y-avatarcolor-pr-84-2026-10-18)).

Estas son propuestas: se envían como PR a limn cuando RUNE las necesita
para algo que está implementando, con tests y demo, como #25–#27.
