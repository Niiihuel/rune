# RUNE sobre limn

RUNE construye toda su interfaz principal con [limn](https://github.com/Niiihuel/limn):
el modelo declarativo (la ventana es una función del estado, reconstruida
cada cuadro), el `UiTree` que guarda foco, hover, scroll y texto por ID, y el
backend `limn-egui` (egui + wgpu) que pinta y traduce la entrada. egui no se
usa como framework de componentes: sólo `apps/desktop` toca eframe/egui, y
únicamente para abrir la ventana y capturar pantalla. Todos los atajos de la
ventana (los de `rune_ui::keybinds`, los botones laterales del mouse, el
grabador de teclas y Pulsar para hablar) llegan como eventos de limn.

Las excepciones autorizadas por el dueño son WebViews temporales y aisladas
para el login del proveedor no oficial y para que la persona resuelva un
hCaptcha. Viven en un proceso auxiliar opcional; no renderizan
conversaciones ni sustituyen componentes de limn
([DISCORD_SESSION.md](DISCORD_SESSION.md), [SECURITY.md](SECURITY.md)).

## Si limn no tiene algo

Regla del proyecto: **lo que falta se implementa en limn y se abre un PR** a
`Niiihuel/limn`; no se parchea dentro de RUNE.

1. Worktree propio de limn desde la cabeza de la rama de integración, en
   una rama `rune/<tema>` (y la rama `feat/<tema>` del PR, desde `main`, si
   las dependencias lo permiten; si no, el PR se apila sobre las que usa).
2. Seguir `docs/contributing.md` de limn: tests, docs, changelog y galería
   (regenerar `site/src/data/pages.json` con
   `cargo run -p limn-gallery -- --pages` si se toca la galería).
3. Unir la rama a la de integración, subirla a GitHub y apuntar RUNE a ella;
   después `cargo update -p limn -p limn-egui`.
4. Mientras tanto, si RUNE compone algo a mano, se marca en el código con
   `// limn-gap:` y el motivo.

Varios agregados chicos que tocan los mismos archivos de limn (`tree.rs`,
`element.rs`, el changelog, la galería) van en un solo PR: separados desde
`main` chocarían entre sí y habría que apilarlos igual.

## Cómo depende RUNE de limn

`Cargo.toml` del workspace apunta a la rama `rune/integration-2026-10-19` de
[Niiihuel/limn](https://github.com/Niiihuel/limn), que reúne las extensiones
que RUNE necesitó y todavía no están todas en `main`. El commit consumido lo
fija `Cargo.lock`, no la cabeza de la rama. Es una dependencia **git**, no de
ruta: así `cargo fmt --all` no reformatea las fuentes de limn. Un trabajador
puede apuntar temporalmente a su clon local (`file:///…/lienzo`); antes de
mezclar en RUNE, la rama se sube a GitHub y `Cargo.toml` vuelve a GitHub.
Cuando una versión publicada reúna los cambios, se podrá pasar a crates.io.

## Qué usa RUNE

| Pieza de limn | Uso en RUNE |
| --- | --- |
| `UiTree`, `Element`, `UiAction`, `UiEvent` | Bucle de cuadros en `apps/desktop`; `rune-ui` traduce acciones en `Intent` |
| `ScrollArea::measured_list` + `follow_end`, `UiTree::visible_rows`, `anchor_offset`, `scroll_to_end` | Timeline virtualizado de altura variable anclado al cargar historial; marcar leído e «Ir a lo más reciente» |
| `ScrollArea::uniform_list`, `UiTree::scroll_to_index`, `SearchField` (`clearable`) | Búsqueda, listas largas, buscador de ajustes |
| `Composer` (`enter_submits`, `pill_controls`, `inline_controls`, `auto_grow`), `TextHighlight::background`, `UiTree::set_input_selection` | Compositor: Enter envía, el editor crece entre «+» y el emoji, pastillas sobre `@palabra` y `#palabra`, selección conservada tras Ctrl+B |
| `SuggestionList` (`SuggestionItem::leading`, `ink`, `detail`) | Autocompletado de menciones con avatar, color del rol y usuario |
| `Markdown` (`font_size`, subrayado, tachado, subtexto, código coloreado por lenguaje) y `TextSpan` (`image`, `surface`, `tooltip`, `underline`) | Contenido de mensajes; emoji de servidor en la línea; fecha completa de `<t:…>` y de «(editado)» en su tooltip. Los spoilers y las menciones los arma RUNE sobre los tramos (`text.rs`) |
| `Element::max_lines` | Tema del canal con emoji, recortado con «…» |
| `MessageActions`, `hover_region`, `hover_region_id`, `UiTree::hovered_region`, `dim_on_hover` | Acciones del mensaje bajo el puntero, decoración del autor, efecto de tarjeta al 35 % |
| `ContextMenu::open_action`, `DropdownMenu` (`at`, `checkbox`, `radio`, `description`, `tooltip_with`, `submenu`, `custom`, `destructive`), `popover_on_hover` | Menú de mensaje en el punto del clic derecho, menús de canales, voz y servidores, menú de estado |
| `Popover`, `Modal`, `AlertDialog`, `Toast::layer` | Selector de emoji, tarjeta de perfil, ajustes, confirmaciones, avisos |
| `Select` (`SelectVariant::Field`, `max_visible_items`), `SwatchPicker`, `ColorPicker`, `RadioOption::icon`, `Switch::track_size`, `Slider` (`mark_labels`, `level`), `Tabs` (`Underlined`), `DateTimePicker` | Formularios de ajustes y administración; nivel del micrófono sobre el umbral de voz |
| `SegmentedControl`, `PreferencesGroup`, `ActionRow`, `ListRow`, `Kbd` | Ajustes, selector rápido, atajos |
| `Badge` (`placement`, `outline`), `Tooltip` (`delay_ms`, `title_font_size`, `title_font_family`, `background`, `faces`) | Menciones del riel; tooltip de servidor con quién está en voz |
| `ScrollbarVisibility::AutoHideUnlessHovered` | Barra de la línea de tiempo |
| `Avatar` (`color`), `Attachment`, `SkeletonGroup`, `Spinner`, `Empty` | Avatares, archivos, carga, vacíos |
| `Icon` + `limn_egui::set_icon_pack`, `Icon::Custom`, `Element::small_icon` | Íconos semánticos con el set de RUNE (`assets/glyphs/`, [ARTWORK.md](ARTWORK.md#íconos-de-la-interfaz)); Bootstrap Icons para lo que el set no tiene; la marca |
| `Element::surface` (`SurfaceFit::Contain`, `ScaleDown`), `surface_mirrored`, `Button::surface`, `DropdownOption::image`/`surface` | GIF y stickers animados en su caja; vista previa de la cámara en espejo; emoji animados en botones y opciones de bots |
| `Element::zoom_view`, `ImageViewer` (`action`, `thumbnail`, `footer`, `zoom_level`) | Visor de imágenes, PDF y videos ([VISOR.md](VISOR.md)) |
| `Role::Region` + `UiTree::focusables_within` | Cinco regiones con nombre; F6 las recorre con el aterrizaje propio de RUNE |
| `limn::Key` completo, `UiEvent::KeyReleased`, `ModifiersChanged`, `UiTree::key_held`, `modifiers`, `observe_keys` | Grabador de teclas, Pulsar para hablar en la ventana, barra de acciones directas con Shift |
| `limn_egui::font_definitions_with(UiFaces { … semibold })`, `FontFamily::SansSemibold` | Fuente de interfaz elegida por la persona; seminegrita donde la REA del cliente oficial la pide |
| `limn_egui::set_color_emoji` | Emoji a color (Twemoji de RUNE) sobre el glifo, también en el texto editable ([ARTWORK.md](ARTWORK.md)) |
| `limn_egui::clear_image_cache`, `set_image_cache_entries` | Decodificación acotada fuera del hilo de UI; caché de texturas que nunca expulsa lo dibujado en el paso actual o el anterior |
| `set_labels` | Textos propios de limn en el idioma activo |

## Página de autorización

`crates/rune-ui/src/auth.rs` compone la página con componentes de limn.
Recibe `Store::authentication`, `ProviderInfo` y datos de dominio; no conoce
adaptadores, endpoints ni credenciales, y no tiene campos para credenciales.
`SignedIn` abre el espacio de trabajo aunque todavía no haya conversaciones,
con un estado vacío y la conexión explícitos, sin inventar canales ni
habilitar composer, presencia o voz sin capacidades. Los botones producen
`BeginSignIn`, `CancelSignIn` y `SignOut`; en OAuth2, abrir el navegador es
`Intent::OpenUrl`. `AwaitingExternal` dice que el login sigue en otra ventana
y ofrece cancelar; Escape cancela una autorización pendiente. `Toast::layer`
se comparte con la vista de chat para que un fallo de revocación no
desaparezca al cerrar la sesión. Detalles del proveedor y scopes, en Ajustes
→ Conexión; el cierre de sesión, en Mi cuenta.

## Lo que todavía falta en limn

| Carencia | Efecto en RUNE | Propuesta |
| --- | --- | --- |
| Entrada de archivos por arrastre con posición ligada al lote | Arrastrar archivos se rechaza ([ARCHITECTURE.md](ARCHITECTURE.md#adjuntos-de-salida)) | Conectar el `FileDropReceiver` de `limn-dnd` (PR #55 y el owner Wayland multiventana de la PR #62, ambas en borrador, probadas en Weston privado) con un owner compartido de portapapeles/DnD, o migrar a winit 0.31 cuando egui lo haga. `Composer::drop_target()` (PR #49) ya marca el destino |
| El subtítulo de `ListRow` es sólo texto | Un extracto de búsqueda dice `:nombre:` en lugar de la imagen del emoji (`// limn-gap:` en `rune-ui/src/search/view.rs`) | `ListRow::subtitle_element` |
| Una fila dentro de la columna de un mensaje no recibe ancho acotado | RUNE pasa el ancho del mensaje explícitamente para que `shrink_to_fit` actúe | Que una columna flexible mida a sus hijos con su ancho |
| `measured_list` tiene una sola estimación de altura por lista y no revisa contenido por fila | Saltos posibles con imágenes o ediciones fuera de vista | `.estimate(\|index\| f32)`, `.row_revision(\|index\| u64)` |
| Imágenes por referencia | Cada vista entrega su `RasterImage` | `ImageRef` + `UiTree::set_image` |

El corrector ortográfico no está entre las carencias: el dueño decidió el
2026-10-18 que RUNE no lo tenga.
