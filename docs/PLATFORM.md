# Integración con el escritorio (P6, 2026-10-17)

Lo que RUNE hace con el escritorio Linux además de su ventana: bandeja,
instancia única, inicio con la sesión, tema y fuente del sistema, paneles,
atajos de voz, la actividad de este equipo y los códigos de cifrado de las
llamadas. Rama `escritorio`; paridad con Serein
([serein-parity.md](../tasks/serein-parity.md#p6-escritorio-idioma-y-voz-local)),
reimplementado. Todo está probado con buses D-Bus privados (`dbus-daemon`
propio de las pruebas), sockets en carpetas temporales, la simulación y
fixtures en loopback; **nada se probó todavía en un escritorio real ni
contra Discord**. Las traducciones (es+en) son un trabajo aparte.

Las preferencias se guardan como las demás (`Intent::SaveSetting`), con
cualquier proveedor desde el 2026-10-17 (commit `9ebfd35`): «Cerrar a la
bandeja», la fuente, los anchos, atajos y los interruptores de actividad son
del equipo; favoritos, fijados y el último lugar, de cada cuenta
(`rune_ui::ACCOUNT_SETTINGS`, `apps/desktop/src/kept.rs`). «Iniciar con la
sesión» es un archivo aparte.

Cada servicio vive en `rune-platform`, en un hilo propio con su runtime y
su conexión, y termina al soltar su handle; el host (`apps/desktop/src/desktop.rs`)
los arranca y los para según las preferencias y traduce lo que informan a
llamadas de `rune-ui` (`rune_ui::desktop`). Capturas (`--screenshot`) y
benchmark no arrancan ninguno.

## Bandeja del sistema

`rune_platform::tray`: un StatusNotifierItem con su menú
`com.canonical.dbusmenu`, sobre zbus (el crate que RUNE ya usa), sin GTK
ni `ksni`. Se registra con su nombre único en `org.kde.StatusNotifierWatcher`
y vuelve a registrarse si el panel reaparece; sin bandeja dice
`Unavailable`. El ícono es el de la aplicación (la runa blanca sobre la placa
ámbar, `assets/icons/hicolor`), en color, a 16, 22, 24, 32, 48 y 64 px para
que el panel tome el que pida (en Windows, al tamaño chico del sistema): el
dibujado para ese tamaño o el siguiente achicado; con un punto rojo mientras
haya menciones; el tooltip cuenta
menciones y lugares sin leer (`rune_ui::desktop::tray_counts`, las mismas
marcas del riel). Menú: «Abrir RUNE», «Silenciar micrófono» y «Ensordecer»
(casillas), «Estado» (En línea, Ausente, No molestar, Invisible; con una
cuenta), «Salir de RUNE». Las acciones son los mismos clics de la interfaz
(`me:mute`, `me:deafen`, `pp:status=…`); «Salir» trae la ventana, guarda
como al cerrar y sale. Menú y tooltip se escriben en el idioma de la
interfaz (`TrayState::language`); cambiarlo vuelve a publicarlos como
cualquier otro cambio de estado. Lo mismo la etiqueta del llavero y el
`Comment` del autostart, que se escriben al crearse
([Idiomas](ARCHITECTURE.md#idiomas)).

«Cerrar a la bandeja» (Ajustes › Sistema › Escritorio) viene **apagado**
(Serein lo trae encendido) y sólo actúa si el escritorio mostró el ícono.
Cerrar oculta la ventana: en X11 con `Visible(false)`; Wayland no oculta
superficies, así que en Hyprland la ventana va al workspace especial
`special:rune` por su socket de comandos (`rune_platform::hyprland`, como
Serein) y vuelve al workspace a la vista; en otros compositores se
minimiza. Oculta, eframe sólo llama `App::logic`: ahí RUNE sigue aplicando
eventos (notificaciones incluidas) y atendiendo la bandeja y las
ejecuciones nuevas. Si la bandeja desaparece, la ventana vuelve.

![Bandeja e inicio con la sesión](img/escritorio-bandeja-sesion-2026-10-17.png)

## Instancia única y enlaces

`rune_platform::single_instance`: la primera ejecución interactiva escucha
en `$XDG_RUNTIME_DIR/rune-instance.sock` (0600; cada conexión se verifica
con `SO_PEERCRED` como del mismo usuario; sin carpeta de runtime privada no
hay instancia única, nunca `/tmp`). Una segunda ejecución se conecta,
manda `RUNE1`, `show` y sus enlaces `https://`/`http://` de la línea de
comandos (hasta ocho, 2048 bytes cada uno) y sale; la primera se trae al
frente (en Wayland pidiendo atención) y abre cada enlace como uno cliqueado
en un mensaje (`rune_ui::desktop::open_link`): un
`…/channels/{servidor|@me}/{canal}[/{mensaje}]` de un dominio propio del
proveedor abre la conversación en RUNE, una invitación pasa por la revisión
de invitaciones, el resto pregunta antes de ir al navegador. Un socket de
una ejecución caída (nadie responde) se reemplaza. Serein no pasa enlaces.

## Iniciar con la sesión

`rune_platform::autostart` escribe `~/.config/autostart/rune.desktop`
(`$XDG_CONFIG_HOME`) con el ejecutable actual citado como pide la
especificación de entradas de escritorio, por escritura aparte y
renombrado; apagarlo lo borra. En una copia instalada el ejecutable es
`~/.local/share/rune/app/current/rune`, que sobrevive a las
actualizaciones ([UPDATES.md](UPDATES.md#linux)); `set_menu_entry` escribe
con la misma cita la entrada del menú de `rune --install`. El estado es el archivo (con `Hidden=true` o
`X-GNOME-Autostart-enabled=false` cuenta como apagado). El portal
`Background` pediría un ID de aplicación instalado que RUNE todavía no
tiene. Serein sólo lo hace en Windows y macOS.

## Tema del sistema

`rune_platform::appearance` lee `org.freedesktop.appearance color-scheme`
del portal `Settings` (`ReadOne`, o `Read` en portales viejos) y sigue sus
`SettingChanged`, sólo del dueño del portal. «Sistema» es el cuarto tema en
Apariencia: oscuro o sin preferencia es Ónix, claro es Claro; elegir otro
deja de seguirlo. Se guarda como `theme = system`.

Desde el 2026-10-18 (rama `escritorio-linux`) también sigue al portal si se
reinicia: escucha `NameOwnerChanged` de `org.freedesktop.portal.Desktop`
(sólo del bus), y con un dueño nuevo vuelve a leer el esquema y a
suscribirse a sus cambios; los del dueño anterior ya no cuentan. Si el
portal se va y no vuelve, queda el último esquema; si no había portal al
arrancar, lo sigue en cuanto aparece. Como el bus lo arranca a pedido y
nadie reinicia uno que se cayó, cada vez que no tiene dueño RUNE le pide al
bus que lo arranque (`StartServiceByName`, como los diálogos), tres veces
como mucho, para no arrancar sin fin uno que siempre falla.

![Tema del sistema](img/escritorio-tema-sistema-2026-10-17.png)

## Temas propios, color principal y transparencia (2026-10-19)

Rama `temas-propios`, paridad con los temas de Serein
(`references/serein/docs/theme-api.md`, `crates/ui/src/theme_editor.rs`),
sin extensiones, Wasm, imágenes ni catálogo remoto. Todo en Apariencia ›
Tema:

- **Temas propios.** «Crear tema» abre el editor sobre una base (Ónix,
  Oscuro o Claro; si hay un tema propio en uso, una copia suya): nombre,
  base, una vista previa que se repinta con cada cambio
  (`rune_components::theme::preview`, una ventana en miniatura que no es
  la app: nada en ella es un control) y los 22 colores de `Look`
  agrupados (Superficies, Texto, Acciones y estados, Presencia). Cada
  color abre el `ColorPicker` de limn y se puede devolver al de la base
  («Restablecer»). La ventana cambia al guardar, no antes: un color
  ilegible no deja la página sin leer. Se guardan hasta ocho, en este
  equipo (ajustes `custom_themes`, una lista JSON, y `custom_theme`, el
  nombre del que está en uso), con «Editar», «Exportar…» y «Borrar» (que
  pregunta antes) para el que está en uso. Elegir Claro, Oscuro, Ónix o
  Sistema deja de usarlo sin borrarlo.
- **Importar y exportar.** Un tema es un archivo de datos
  (`*.rune-theme.json`): `{"rune_theme": 1, "name": …, "base": …,
  "colors": {"accent": "#rrggbb", "hover": "#rrggbbaa", …}}`. Se lee
  entero y se rechaza cualquier otro campo, una versión distinta, un
  nombre vacío o de más de 48 caracteres, una base o un color que RUNE no
  conoce y todo archivo de más de 64 KB; nunca se ejecuta nada de él.
  «Importar…» usa el selector del portal (`rune-core::theme_files`, sobre
  `FileChooser`, leído con límite fuera del hilo de la UI); «Exportar…», el
  diálogo «Guardar como» del portal, y escribe un archivo nuevo: si ya hay
  uno con ese nombre no lo pisa y lo dice. Un nombre repetido al importar
  recibe un número. Las tres grandes superficies (riel, columnas,
  conversación) de un tema son siempre opacas: la transparencia es del
  equipo, no del tema.
- **Color principal.** El del tema (la primera muestra), uno de los
  veinte preajustes de los selectores de color de RUNE (los de los roles)
  o cualquiera con el `ColorPicker`. Tiñe botones,
  menciones e indicadores sobre cualquier tema, propio o no, como en
  Serein: el texto sobre él es blanco o negro según cuál contraste más
  (WCAG) y el fondo de una mención lo sigue con el alfa del tema. Se guarda
  como `accent_color`.
- **Transparencia de la ventana**, de 0 a 90 % en pasos de 5. Se lee de
  `~/.config/rune/transparency` antes de abrir la ventana: sólo si es mayor
  que 0 la ventana se abre transparente (`ViewportBuilder::with_transparent`,
  y `App::clear_color` en `[0, 0, 0, 0]`); si se abrió opaca, el cambio se
  aplica al volver a abrir RUNE y la página lo dice. Con la ventana
  transparente, el riel, las columnas y la conversación no pintan su fondo
  (`Look::see_through`) y la raíz pinta una sola vez el color de la
  conversación con el alfa elegido (`Look::backdrop`), así los paneles
  anidados no se suman; los menús, diálogos, ajustes y editores de página
  completa siguen opacos. Volver a 0 lo hace opaco en el momento (la
  superficie con alfa queda hasta reiniciar). Capturas, benchmark y
  pruebas no la leen ni la escriben.

**El desenfoque lo pone el compositor**, no RUNE: Wayland no tiene un
protocolo estándar para pedirlo, y RUNE no lo pide. En KDE Plasma, KWin
desenfoca lo que hay detrás de una ventana translúcida con el efecto
«Desenfoque» (Preferencias del sistema › Comportamiento del espacio de
trabajo › Efectos del escritorio) para las ventanas que lo piden; para
forzarlo en RUNE, una regla de ventana o un script de KWin sobre la clase
`rune`. En Hyprland, `decoration { blur { enabled = true } }` desenfoca
toda ventana translúcida; `windowrulev2 = opacity …, class:^(rune)$`
cambia además la opacidad entera de la ventana (texto incluido), que no es
lo mismo que esto. Sin compositor (X11 sin uno) la ventana se ve con
fondo negro. Serein pide el desenfoque nativo en macOS, Windows 11 y KDE;
RUNE todavía no habla el protocolo de desenfoque de KDE
(`org_kde_kwin_blur`).

Límites conocidos: con transparencia, el riel y las columnas toman el
color de la conversación (se separan por sus líneas), y los anillos de
estado sobre las caras no recortan. El token `card` (tarjetas dentro de
un mensaje, rama `tarjetas-con-contraste`) está en el editor, en
Superficies, y `raised` pasó a llamarse «Campos y botones»; un archivo de
tema sin `card` toma el de su base.

![Editor de temas](img/temas-propios-editor-2026-10-19.png)
![Color principal y transparencia](img/temas-propios-color-2026-10-19.png)

## Ventana y gráficos (2026-10-18)

Rama `escritorio-linux`, paridad con Serein (Ajustes › General › Window y
Graphics). En Ajustes › Sistema:

- **Ventana › Barra de título** (rama `barra-e-icono`, reemplaza a «Ocultar
  las decoraciones»): Automática, la de RUNE o la del sistema, en vivo
  (`ViewportCommand::Decorations`) y al abrir. Automática es la de RUNE en
  Windows y en X11, y la del sistema en Wayland (`WAYLAND_DISPLAY`), como
  Surco. La de RUNE quita el marco del sistema y pone `WindowControls` de
  limn al final de la cabecera; arrastrar su franja libre la mueve
  (`StartDrag`), doble clic la maximiza o la restaura, y el borde de 5 px la
  cambia de tamaño (`BeginResize`) mientras no esté maximizada; un clic
  sobre un control de la cabecera es del control. Cerrar es el cierre del
  sistema (guarda primero, o va a la bandeja). Con la de RUNE la ventana no
  baja de 868 × 490 (los 720 × 480 de siempre, los 138 px de los botones y
  los bordes), y la cabecera pliega sus herramientas en «más» 138 px antes,
  para que nada quede bajo los botones. Se guarda en
  `~/.config/rune/decorations` (`hidden`: la de RUNE, como quedaba al
  ocultar las decoraciones; `shown`: la del sistema; sin archivo:
  automática). `--title-bar auto|rune|system` la elige para una ejecución
  sin guardarla (capturas).
- **Ícono de la aplicación** (rama `barra-e-icono`): el que hizo el dueño,
  la runa blanca sobre la placa ámbar (`assets/icons/rune.png` y los PNG de
  `assets/icons/hicolor`; 16 y 24 px con el trazo más grueso de
  `rune-small.svg`). La ventana lo fija a 256 px
  (`ViewportBuilder::with_icon`): X11 y Windows lo muestran en la barra de
  tareas. Wayland no lleva ícono por ventana: lo busca por el `app_id`
  (`rune`) en la entrada del menú, y `rune --install` instala cada tamaño
  en `~/.local/share/icons/hicolor` (y borra el `scalable/apps/rune.svg` de
  la marca que dejaba antes). Las notificaciones nombran el ícono `rune` y
  la entrada `rune` (`desktop-entry`). La marca de trazo
  (`assets/rune-mark.svg`) queda como glifo dentro de la interfaz (carril,
  pantalla de carga, perfil).
- **Gráficos › Dibujar con**: Automático (lo de siempre: en una laptop con
  dos GPU, la que maneja la pantalla) o cada fabricante de GPU que sysfs
  muestra (AMD, Intel, NVIDIA; dos del mismo fabricante son una sola opción,
  porque el filtro del cargador de Vulkan no las distingue). «Ahora dibuja
  con {gpu}. Se aplica la próxima vez que abras RUNE.»: la GPU es la que
  wgpu eligió (`AdapterInfo`). Si una variable del entorno decide
  (`VK_LOADER_DRIVERS_SELECT`, `VK_DRIVER_FILES`, `VK_ICD_FILENAMES`,
  `RUNE_GPU`) la elección no se usa y la página lo dice; si la GPU elegida
  no abre la ventana, RUNE arranca otra vez con cualquiera y también lo
  dice. Detalle en [PERFORMANCE.md](PERFORMANCE.md).

Las dos se leen antes de abrir la ventana, así que no van con los demás
ajustes (la base local se abre después): son dos archivos de una línea,
`~/.config/rune/decorations` (`hidden`) y `~/.config/rune/gpu` (`amd`,
`intel` o `nvidia`), como `~/.config/rune/provider`. Nunca una credencial.
Un solo hilo los escribe, en el orden de los cambios (el último gana), cada
uno entero (un temporal que se renombra encima). Se leen una sola vez, y
cerrar sesión o cambiar de proveedor no los olvida: la página sigue
mostrando lo que la ventana tiene. Si el arranque de respaldo vino del
filtro automático, la página dice «La GPU que eligió RUNE no pudo abrir la
ventana…», no «La GPU elegida…». Capturas, benchmark y pruebas no los
escriben.
[Captura](img/escritorio-linux-ventana-2026-10-18.png).

Los diálogos del portal se abren sobre la ventana: ver
[FILES.md](FILES.md#diálogos-sobre-la-ventana-2026-10-18).

## Fuente de la interfaz

Apariencia › Fuente lista las familias del sistema (`fc-list`, en un hilo
aparte, al abrir la lista). Elegida una, `rune_platform::fonts::load` pide a
`fc-match` sus pesos regular, medio y negrita (aceptando colecciones `.ttc`
por índice y fuentes variables que cubran el peso) y lee los archivos (hasta
32 MiB cada uno) fuera del hilo de UI; el host la instala con
`limn_egui::font_definitions_with` (nueva en limn, ver
[LIMN_INTEGRATION.md](LIMN_INTEGRATION.md)) y vuelve a medir el árbol. Sólo
se guarda el nombre (`ui_font`): si la familia desaparece, RUNE vuelve a
Ubuntu y lo dice. Serein guarda el archivo entero en SQLite. La escala de
la interfaz ya existía (zoom 50–200 %).

![Fuente](img/escritorio-fuente-2026-10-17.png)

## Paneles, favoritos, fijados y canales ocultos

- Los bordes de la lista de canales (220–440 px) y de la de miembros
  (220–400 px) se arrastran con el `ResizeHandle` de limn; Esc o un
  arrastre cancelado vuelve al ancho anterior, soltar lo guarda
  (`sidebar_width`, `members_width`) y «Restablecer» en Apariencia vuelve a
  los de siempre. Serein sólo ajusta la de canales.
- «Favorito» en el menú de un canal lo sube a «Favoritos», arriba de su
  servidor y fuera de su categoría; «Fijar en la lista» en el menú de un
  privado o grupo lo pone bajo «Fijados». Hasta 64 de cada uno, los nuevos
  primero, sólo en este equipo (`favorite_channels`, `pinned_directs`).
- «Mostrar los canales ocultos» (menú de la lista de canales o Apariencia)
  lista los canales del servidor que la cuenta no puede ver (sin
  `VIEW`), atenuados, con un ojo tachado, sin abrirse ni marcas de no leído.
  La simulación tiene «moderación» en el primer servidor para verlo.

![Favoritos y ocultos](img/escritorio-favoritos-ocultos-2026-10-17.png)
![Fijados](img/escritorio-privados-fijados-2026-10-17.png)

## Atajos del micrófono y el sonido

Ajustes › Sistema › Atajos de teclado › Voz (hasta el 2026-10-19 era una
sección propia, «Atajos del micrófono y el sonido»; ahora es la parte de voz
de la tabla común de atajos, con su aviso de conflictos): silenciar,
ensordecer y «pulsar para silenciar» se graban como el de «pulsar para
hablar» y se piden al portal `GlobalShortcuts` **en la misma sesión** que
él (un solo diálogo del escritorio), sólo combinaciones con Ctrl/Alt/Shift
o teclas F. Silenciar y ensordecer actúan una vez por pulsación; con la
ventana enfocada actúan también si el escritorio no los asignó. Mientras
se mantiene «pulsar para silenciar» el micrófono no pasa
(`audio.muted_held`). Ver [MEDIA.md](MEDIA.md#pulsar-para-hablar-2026-10-16).

![Atajos](img/escritorio-atajos-voz-2026-10-17.png)

## Actividad de este equipo

Dos fuentes, cada una **apagada por defecto** con su interruptor en
Ajustes › Sistema › Actividad (decisión del dueño del 2026-10-17; ver
[SECURITY.md](SECURITY.md#actividad-local-rich-presence-y-juegos-p6)):

- **Programas** (`rune_platform::rich_presence`): el socket local
  `discord-ipc-N` en `$XDG_RUNTIME_DIR`, como el cliente oficial y Serein.
  Saludo, `READY` con la cuenta, `SET_ACTIVITY` (tipo, detalles, estado,
  tiempos, imágenes; botones, secretos y grupos se descartan),
  `INVITE_BROWSER` (trae la ventana al frente y abre la invitación como un
  enlace que entrega otro arranque: en el diálogo de invitación de RUNE,
  donde aceptar es un clic de la persona), ping; el resto se
  rechaza con el código 4000. Hasta 8 programas; al irse se borra su
  actividad. Sin WebSocket ni puertos TCP.
- **Juegos** (`rune_platform::games`): cada 10 s se miran los procesos
  del usuario —sólo `/proc/N/exe` y los primeros 512 bytes del primer
  argumento de `cmdline`, que es donde Wine y Proton dicen el nombre del
  `.exe`— contra la lista del proveedor (`Command::LoadDetectableGames`;
  en la sesión, la pública `/applications/detectable` de Discord), por la
  cola más larga de la ruta; el juego reconocido se mantiene mientras
  siga abierto.

Gana la actividad más nueva de un programa; si no hay, el juego. RUNE la
muestra como la actividad propia (`Store::presence` de la cuenta) y la
publica con `Command::SetLocalActivity` si el proveedor declara
`local_activity` ([DISCORD_SESSION.md](DISCORD_SESSION.md#escritorio-y-actividad-local-2026-10-17-rama-escritorio)).

![Actividad](img/escritorio-actividad-2026-10-17.png)

## Código de privacidad de la llamada

Con `voice_privacy_code`, Detalles de voz tiene una pestaña «Privacidad»
(y Ajustes › Voz y video, durante una llamada, la misma sección): el
código de privacidad de la llamada (30 dígitos en grupos de cinco, con
Copiar), que cambia con cada grupo DAVE, y por persona «Ver código»: los
45 dígitos que la cuenta y esa persona comparan por otro medio. La
simulación inventa códigos fijos y lo dice. Detalle del cálculo en
[MEDIA.md](MEDIA.md#código-de-privacidad-dave-2026-10-17).

![Código de privacidad](img/escritorio-codigo-privacidad-2026-10-17.png)

## Registro a archivo (2026-10-18)

Lo que RUNE escribe en stderr (las líneas `rune: voice …`, `rune: gateway:
…` y los errores del cliente, y un pánico si lo hay) queda también en
`$XDG_STATE_HOME/rune/rune.log`, o `~/.local/state/rune/rune.log`, para
mandar el diagnóstico de una llamada fallida aunque la terminal ya no
esté. `apps/desktop/src/log.rs`: al arrancar, el descriptor 2 pasa a una
tubería y un hilo copia lo que llega al stderr original y al archivo; nada
de los crates cambia ni hay dependencias nuevas (sólo `dup2` e `ioctl` de
la biblioteca de C). Rota por tamaño: 5 MB por archivo, tres archivos
(`rune.log`, `rune.log.1`, `rune.log.2`); la carpeta es `0700` y los
archivos `0600` (y `~/.local/state` también es `0700` si RUNE la crea). Un
arranque nuevo sigue el archivo más nuevo y empieza con una línea
`--- rune start <segundos UNIX> pid <pid>`, sólo en el archivo, para saber
a qué ejecución y a qué hora pertenece lo que sigue. Sólo en
ejecuciones interactivas en Unix: `--screenshot`, `--bench`, la segunda
ejecución que reenvía sus enlaces y los errores de la línea de órdenes van
sólo a stderr. Si el archivo no se puede escribir (disco lleno), stderr
sigue igual. Qué puede y qué no puede llevar una línea: [SECURITY.md](SECURITY.md#datos-locales).

Como la terminal también recibe todo por la tubería, lo último que se
escribe antes de una caída compite con la muerte del proceso. Un pánico
espera hasta 200 ms a que el hilo copie lo pendiente antes de seguir, así
también llega el que termina en un abort (en un callback de C, como
PipeWire o cpal, o un pánico durante otro). Un abort o una señal que vienen
del código de C (un `assert` de un driver o de GStreamer, un `SIGSEGV`) no
pasan por ahí: lo último antes de esa caída puede no llegar ni a la
terminal ni al archivo.

## Qué falta probar en un escritorio real

- Bandeja en Hyprland (waybar), KDE y GNOME con AppIndicator: ícono,
  menú, punto de menciones, «Cerrar a la bandeja» y volver (Hyprland por
  el workspace especial; KDE minimiza, no oculta).
- Segunda ejecución con un enlace `https://discord.com/channels/…`.
- Entrada de autostart al iniciar sesión.
- Portal Settings en vivo (cambiar el tema del escritorio), y reiniciarlo
  (`systemctl --user restart xdg-desktop-portal`) para ver que RUNE lo
  sigue después.
- Diálogos sobre la ventana (2026-10-18): en Hyprland y en GNOME/KDE,
  «Subir un archivo», «Guardar como», «Elegir…» de la carpeta de sonidos y
  compartir pantalla tienen que abrirse sobre RUNE y, si el portal lo hace,
  bloquearla; si el compositor no exporta, stderr dice «the compositor
  cannot export the window».
- Barra de título de RUNE en X11 (KDE, GNOME en X11, i3), en Hyprland y
  GNOME en Wayland forzada desde Ajustes: mover arrastrando la cabecera
  (con el acople a los bordes del gestor), doble clic, cambiar el tamaño
  por los bordes y las esquinas, minimizar, maximizar y restaurar, cerrar
  (con «Cerrar a la bandeja» encendido y apagado), y volver a la del
  sistema en vivo. Ícono de la ventana en la barra de tareas de X11, y con
  `rune --install` el de Wayland (por el `app_id`) y el de las
  notificaciones.
- «Dibujar con»
  NVIDIA y Automático en la laptop híbrida, mirando la GPU que dice la
  página tras reabrir.
- Una fuente del sistema con pesos y una variable.
- Atajos globales en el diálogo del portal (KDE/GNOME/Hyprland).
- Un juego o programa real con Rich Presence (por ejemplo uno que use
  `discord-rpc`) y la detección de un juego de Steam/Proton.
- Código de privacidad y verificación en una llamada real con otra
  persona.

## Windows (2026-10-19, rama `windows-base`)

Primera etapa del port, pedida por el dueño («esto lo quiero para Windows,
no sólo Linux»). RUNE compila y enlaza para Windows x86_64, y sus
pruebas pasan con Wine salvo dos (abajo); los servicios del sistema tienen su versión de Windows detrás de la
misma API de `rune-platform` que usa Linux, así que nada por encima cambió
salvo lo que la interfaz dice distinto en Windows. **Nada se probó todavía
en un Windows real** (ver abajo qué se verificó y cómo). Serein corre en
Windows; se miró cómo resuelve cada servicio, sin copiar código.

### Qué se verificó y cómo

- **Compila y enlaza** para `x86_64-pc-windows-gnu` desde NixOS: clippy
  `--workspace --all-targets --all-features -D warnings` limpio (también
  `rune-update`), y `cargo build -p rune-desktop` da un `rune.exe`.
- **Pruebas con Wine** (`cargo test --target x86_64-pc-windows-gnu
  --workspace`): pasan todas menos dos, que dependen de Wine o de la carga
  de la máquina: una medición de tiempo de `rune-components`
  (`a_long_run_costs_its_length_not_more`, 2,1 s contra 0,5 s) y un cierre
  del Gateway de prueba que Wine ve como reset de la conexión
  (`gateway_other_terminal_close_stays_offline_without_automatic_login`).
  Con `--ignored`: la clave `Run` va y vuelve; el Administrador de
  credenciales guarda, lista, lee y borra, pero Wine no guarda los
  atributos (nombre y avatar); la tubería pasa los enlaces, pero Wine deja
  crear una segunda «primera instancia», que Windows rechaza. Wine encontró
  dos errores que se corrigieron: un cliente de la tubería que escribía y
  se iba antes de la espera se perdía, y «Guardar como» no notaba otro
  archivo puesto en el nombre (sin `dev`/`inode` en Windows).
- **`rune.exe` con Wine** abre la simulación y pinta (`--screenshot`):

![rune.exe con Wine](img/windows-base-wine-2026-10-19.png)

- **Sin ejecutar**: el target MSVC, el job `windows` de la CI y todo en un
  Windows real (bandeja, toasts, diálogos, atajos, audio WASAPI, la
  ventana sin consola y su registro a archivo).

### Compilar y probar desde Linux

Las versiones se compilan con el target MSVC (`x86_64-pc-windows-msvc`), en
Windows: el job `windows` de [CI](../.github/workflows/ci.yml)
(`windows-latest`: clippy `-D warnings`, `cargo test` y las pruebas de los
servicios reales con `--ignored`). Desde NixOS se comprueba con el target
GNU, que comparte casi todo el código:

```sh
nix-shell --arg withWindows true
cargo clippy --target x86_64-pc-windows-gnu --workspace --all-targets --all-features -- -D warnings
cargo build  --target x86_64-pc-windows-gnu -p rune-desktop        # target/x86_64-pc-windows-gnu/debug/rune.exe
cargo test   --target x86_64-pc-windows-gnu --workspace            # bajo Wine
cargo test   --target x86_64-pc-windows-gnu -p rune-platform -- --ignored
```

`withWindows` suma el GCC cruzado de MinGW-w64 de nixpkgs (compila el C que
traen los crates: SQLite, libopus, ring) con su winpthreads, y Wine: `cargo
test` con ese target corre las pruebas con Wine (`CARGO_TARGET_…_RUNNER`),
en un prefijo dentro de `target/wine`. Wine implementa la API de Windows,
pero no es Windows: que algo pase con Wine no dice que funcione en Windows,
y al revés. El target MSVC (`cargo-xwin`) no se usa: descarga el SDK de
Microsoft aceptando su licencia, y eso lo decide el dueño. El shell aplica
el `rpath` de Linux sólo al target de este equipo
(`CARGO_TARGET_<host>_RUSTFLAGS`), así no llega a un `.exe`.

### Servicios

| Servicio | Linux | Windows |
| --- | --- | --- |
| Carpetas (`rune_platform::folders`) | XDG: `~/.local/share/rune`, `~/.config/rune`, `~/.cache/rune`, `~/.local/state/rune` | `%LOCALAPPDATA%\RUNE` (base local), `%APPDATA%\RUNE` (archivos de antes de la ventana), `%LOCALAPPDATA%\RUNE\cache`, `%LOCALAPPDATA%\RUNE\logs`; carpetas conocidas del shell, también Descargas donde la persona la haya movido |
| Abrir enlaces y carpetas (`rune_platform::open`) | `xdg-open` | `ShellExecuteW` en un hilo propio; sólo `http(s)` de una línea, o una carpeta que existe |
| Elegir archivos, «Guardar como», carpeta (`file_dialogs::FileChooser`) | portal `FileChooser` | `IFileOpenDialog`/`IFileSaveDialog` modales a la ventana, en un hilo con COM; mismas validaciones y mismo worker de archivos. El diálogo no pregunta por un archivo que ya está: pregunta RUNE, como en Linux. Arrastrar y pegar archivos usa el mismo `open_local_files`, que en Windows abre sólo rutas de disco o de red, nunca un dispositivo |
| Llavero (`secrets::system`) | Secret Service | Administrador de credenciales: una credencial genérica `RUNE/{servicio}/{cuenta}` por entrada, del usuario (DPAPI), sólo de este equipo; nombre y avatar como atributos, como en Linux. Sirve a la sesión no oficial y a las claves de la caché cifrada |
| Notificaciones | `org.freedesktop.Notifications` | toasts de Windows (WinRT) con el ID `RUNE.Desktop`, registrado en `HKCU\Software\Classes\AppUserModelId\RUNE.Desktop`; el acceso directo del instalador en Inicio y el proceso de RUNE llevan el mismo ID; clic y botones mientras RUNE corre; una llamada entrante usa el escenario `incomingCall` |
| Tema claro/oscuro y acento | portal `Settings` | `AppsUseLightTheme` y `DWM\AccentColor` del registro, seguidos con `RegNotifyChangeKeyValue`. **Acento nuevo**: con el tema «Sistema» y sin color principal propio, el color de acento de Windows tiñe la ventana (Linux todavía no lo informa) |
| Iniciar con la sesión (`autostart::{state, change}`) | `~/.config/autostart/rune.desktop` | valor `RUNE` de `HKCU\…\CurrentVersion\Run`, entre comillas y con `--start-minimized` si va a la bandeja; si el Administrador de tareas lo deshabilitó (`StartupApproved\Run`) cuenta como apagado, y activarlo en RUNE lo vuelve a habilitar |
| Instancia única | socket en `$XDG_RUNTIME_DIR` | tubería con nombre `\\.\pipe\rune-instance-{sesión}`: la primera instancia la crea primero (`FILE_FLAG_FIRST_PIPE_INSTANCE`), sólo el usuario (y administradores y sistema) puede escribirle, sin clientes remotos; una segunda ejecución comprueba que el servidor sea de su sesión, le permite traer la ventana al frente (`AllowSetForegroundWindow`) y le pasa sus enlaces |
| Bandeja | StatusNotifierItem | ícono del área de notificación (`Shell_NotifyIconW`) de una ventana oculta en su hilo, con el mismo menú; vuelve si Explorer se reinicia (`TaskbarCreated`) |
| Atajos globales | portal `GlobalShortcuts` | `RegisterHotKey` por atajo, sin ganchos de teclado; Windows no avisa al soltar, así que mientras uno está apretado el hilo mira la tecla cada 15 ms. Un atajo que tiene otro programa no se asigna. Mientras RUNE lo tiene, los demás programas no reciben esa combinación |
| Portapapeles de archivos e imágenes | `arboard` (X11) | `arboard` (Windows): archivos copiados (`CF_HDROP`) e imágenes; lo copiado espera en `%LOCALAPPDATA%\RUNE\copied` |
| Registro a archivo | tubería en el descriptor 2 | tubería en el `STD_ERROR_HANDLE`; `rune.exe` es un programa de ventana (sin consola), así que el archivo es el único lugar de sus líneas |
| Audio | cpal sobre PipeWire (filtra los monitores de salida) | cpal sobre WASAPI; el filtro de monitores es sólo de PipeWire |

### Login y hCaptcha con WebView2 (rama `windows-login`)

Segunda etapa. La sesión no oficial ya se puede usar en Windows: el
selector de proveedor la ofrece en una compilación con `--features
session-login`, como en Linux. **Sin probar en un Windows real.**

- **Ventanas.** `rune-login/src/windows.rs`: login y hCaptcha con WebView2
  (`webview2-com`, la vinculación más chica, con el `windows` 0.62 que RUNE
  ya compila), con el mismo contrato que WebKitGTK: un proceso auxiliar por
  ventana, perfil InPrivate propio en `%TEMP%\RUNE-webview\<al azar>` que
  se borra al cerrar, la página principal propia como única que devuelve
  algo, la tubería privada y un único reintento del hCaptcha que resuelve la
  persona. La ventana lleva el ícono de RUNE (`assets/icons/rune.ico`)
  ([rune-login](../crates/rune-login/README.md),
  [SECURITY.md](SECURITY.md#datos-locales)).
- **Identidad.** La del WebView2 instalado: al arrancar el proveedor, un
  auxiliar oculto lee lo que una página ve de su navegador y REST, el
  Gateway, la voz y las ventanas presentan ese mismo
  ([DISCORD_SESSION.md](DISCORD_SESSION.md#identidad-por-plataforma-2026-10-19-rama-windows-login)).
  Linux queda como estaba.
- **Sin WebView2 Runtime** (viene con Windows 11 y con las actualizaciones
  de Windows 10): el inicio de sesión falla con «Falta WebView2 Runtime de
  Microsoft…» y el enlace oficial del instalador,
  <https://go.microsoft.com/fwlink/p/?LinkId=2124703>.
- **Cargador.** Con MSVC, `rune.exe` lleva enlazado el cargador estático
  de WebView2 (aviso en `THIRD_PARTY_NOTICES.md`); con el target GNU de este
  chequeo, `rune.exe` importa `WebView2Loader.dll`, que hay que poner al
  lado (`target/x86_64-pc-windows-gnu/debug/build/webview2-com-sys-*/out/x64/`).
  El instalador (rama `windows-instalador`) compila con
  `--features session-login`, y armado con MinGW lleva el cargador con
  `/DLoader=` ([UPDATES.md](UPDATES.md#windows)).
- **Verificado sin Windows:** clippy del workspace para
  `x86_64-pc-windows-gnu` con `--all-targets --all-features -D warnings`
  limpio; con Wine, las pruebas de `rune-login --features webview` (13) y
  las de identidad de `rune-discord-session` (también la del proceso hijo
  que presenta un WebView2 de ejemplo y mira REST, Gateway e Identify);
  bajo Wine, que no trae WebView2 Runtime, el auxiliar termina con el código
  de «falta el runtime» (prueba ignorada `native_probe_reads_its_own_engine`)
  y `rune.exe` (con `session-login`) también: los auxiliares del login y
  del motor terminan con ese código sin escribir nada, y con la entrada o la
  salida en un archivo en vez de una tubería se niegan (código 6). El ícono:
  la prueba de qué imagen del `.ico` toca a cada tamaño. **Sin ejecutar:** WebView2
  de verdad (Wine sin el runtime de Microsoft, que no se bajó: aceptar su
  licencia lo decide el dueño), las ventanas, el login, el hCaptcha, el
  borrado de la carpeta y el target MSVC.

### Apagado en Windows (y la interfaz lo dice)

- **Pantalla compartida, cámara y video en mensajes** (PipeWire, GStreamer,
  OpenH264): la interfaz dice «La vista previa de pantalla y cámara solo
  funciona en Linux por ahora»; un video adjunto suena sin imagen.
- **Detectar juegos** y **Actividad de otros programas** (`discord-ipc`,
  que en Windows es una tubería con nombre): interruptores deshabilitados
  con su motivo.
- **Fuentes del sistema**: no hay fontconfig; la lista queda en la de RUNE y
  la nota lo dice. **Dibujar con**: sin `sysfs`, sólo «Automático» (Windows
  elige la GPU por aplicación en Configuración › Pantalla › Gráficos).
- **Instalador** por usuario (Inno Setup, en `%LOCALAPPDATA%\Programs\RUNE`,
  con acceso directo, desinstalador y modo silencioso para el updater) e
  ícono y versión en `rune.exe`: rama `windows-instalador`, detalle en
  [UPDATES.md](UPDATES.md#windows). El ícono es `assets/icons/rune.ico`
  (rama `barra-e-icono`; cómo se hizo en
  [ARTWORK.md](ARTWORK.md)).

### Qué tiene que probar el dueño en Windows

En un Windows 10 u 11 x86_64, con la compilación de la CI (MSVC) o una
local (`cargo build --release -p rune-desktop` en Windows):

- Instalador (`rune-x86_64-windows-setup.exe` de una release, o armado
  como dice [UPDATES.md](UPDATES.md#windows)): sin pedir permisos de
  administrador; SmartScreen avisa por la falta de firma. Queda en
  `%LOCALAPPDATA%\Programs\RUNE`, con «RUNE» en Inicio y en Configuración ›
  Aplicaciones (con su ícono y versión); `rune.exe` muestra el ícono en el
  Explorador y la versión en Propiedades › Detalles. El paso `DLLs` del
  workflow lista sólo DLLs de Windows. Instalarlo de nuevo con RUNE
  abierto: el asistente ofrece cerrarlo.
- Anclar RUNE a la barra de tareas desde Inicio; cerrarlo, abrirlo con el
  inicio de sesión (o tras una actualización) y ver que usa el ícono
  anclado, sin un segundo botón.
- Una actualización: RUNE se cierra, el instalador corre sin ventanas y
  RUNE vuelve a abrir con la versión nueva (Configuración › Aplicaciones
  la muestra); en `%TEMP%\Setup Log …txt` queda lo que hizo, y
  `%TEMP%\rune-updates` se vacía en la búsqueda siguiente.
- Desinstalar desde Configuración › Aplicaciones: se van el programa, el
  acceso directo y el inicio con la sesión; `%LOCALAPPDATA%\RUNE` y
  `%APPDATA%\RUNE` quedan.

- Abrir RUNE con la simulación (`rune.exe --provider mock`): ventana, sin
  consola, y `%LOCALAPPDATA%\RUNE\logs\rune.log` con la línea
  `--- rune start`.
- Cerrar y abrir: la base local en `%LOCALAPPDATA%\RUNE\rune.db` y los
  archivos de antes de la ventana en `%APPDATA%\RUNE`.
- Segunda ejecución con un enlace (`rune.exe https://discord.com/channels/…`):
  trae la primera al frente y abre el enlace.
- Bandeja: ícono con punto rojo con menciones, menú (abrir, silenciar,
  ensordecer, estado, salir), «Cerrar a la bandeja» y volver; reiniciar
  Explorer y ver que el ícono vuelve.
- Barra de título de RUNE (automática en Windows): mover arrastrando la
  cabecera, Aero Snap al soltarla en un borde, doble clic, los bordes,
  minimizar, maximizar, restaurar y cerrar; el ícono en la barra de tareas
  y en Alt+Tab.
- Notificación de un mensaje y de una llamada entrante (botones), con clic.
- Tema «Sistema»: cambiar claro/oscuro y el color de acento en
  Configuración › Personalización › Colores, con RUNE abierto.
- Iniciar con la sesión, también minimizado; deshabilitarlo en el
  Administrador de tareas y ver que Ajustes lo dice apagado.
- Subir un archivo, «Guardar como» (también sobre un archivo que ya está),
  importar/exportar un tema y elegir la carpeta de sonidos: diálogos
  modales a RUNE.
- Pegar una imagen y archivos copiados en el Explorador; copiar una imagen
  de un mensaje y pegarla en otro programa.
- Pulsar para hablar y los atajos de voz con otra ventana enfocada.
- Micrófono y salida (WASAPI): lista de dispositivos, prueba de micrófono,
  una llamada en la simulación.
- Llavero: la caché cifrada por cuenta se ve en el Administrador de
  credenciales como `RUNE/rune-cache/…`; con la sesión no oficial, también
  la sesión recordada.
- Sesión no oficial (compilación con `--features session-login`): elegirla
  en el selector; la primera vez abre la ventana «RUNE · Iniciar sesión en
  Discord» (WebView2), sin consola. Iniciar sesión, cerrar la ventana a
  mitad (RUNE dice que se canceló), y otra vez hasta entrar. Ver que en
  `%TEMP%\RUNE-webview` no queda ninguna carpeta unos segundos después de
  cerrar cada ventana.
- Aceptar una invitación que pida hCaptcha: ventana «RUNE · Verificación»,
  resolverlo, y que la aceptación se reintente una vez. Mirar si Discord
  pide el teléfono o rechaza; si el Gateway conecta; mandar un mensaje y un
  adjunto.
- La identidad: en `%LOCALAPPDATA%\RUNE\logs\rune.log` no aparece nada
  de ella (ni la credencial); si hace falta compararla, las herramientas de
  desarrollo de Edge sobre `discord.com` en el mismo equipo muestran el user
  agent y las client hints que RUNE copia.
- Sin WebView2 Runtime (por ejemplo, en una máquina virtual de Windows 10
  sin actualizar): RUNE dice que falta y da el enlace de Microsoft.
