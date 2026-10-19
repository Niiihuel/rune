# Paridad con Serein (2026-10-17)

Regla del dueño (2026-10-17): «todo lo que lo haga serein lo implementamos
nosotros»; RUNE sigue su enfoque, con mejor interfaz e integraciones. Se
reimplementa, nunca se copia. Este inventario sale de leer
`references/serein` y `references/serein-current` y buscar cada función en
RUNE; deja fuera lo que ya cubren los recorridos 01–22 de [todo.md](todo.md)
(video fase 5, privados/grupos/amigos, hilos, mensajes enriquecidos y
visores, adjuntos y plataforma, cuentas y perfiles, administración).
Tamaños estimados: S, M, L.

## P1. Cuenta sincronizada

Hecho el 2026-10-17 en la rama `cuenta-sincronizada`, sólo con fixtures
(ver [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#cuenta-sincronizada-2026-10-17-rama-cuenta-sincronizada)).

- [x] Publicar estado propio y personalizado (op 3), restaurarlo al iniciar,
  `SESSIONS_REPLACE` (S-M). Además: Identify invisible hasta leer la cuenta.
- [x] Notificaciones por servidor y canal sincronizadas: `user_guild_settings`,
  `USER_GUILD_SETTINGS_UPDATE`, `PATCH /users/@me/guilds/{g}/settings` (M).
- [x] Carpetas de servidores y orden del riel (`settings-proto/1`, protobuf
  mínimo propio) (M-L). Desde menús y, desde el 2026-10-18, arrastrando.
- [x] Privacidad de mensajería: DMs por servidor, solicitudes de mensajes,
  filtro de spam, quién puede mandar solicitudes de amistad (M). Además:
  contenido explícito, aplicado desde el 2026-10-18 (rama
  `contenido-de-mensajes`: difumina por servidor, amigo u otro; Serein lo
  trata como spoiler sin mirar el ajuste).
- [x] Notas privadas por persona (`/users/@me/notes/{id}`, `USER_NOTE_UPDATE`) (S).

## P2. Lecturas, búsqueda y miembros remotos

Hecho el 2026-10-17 (rama `lecturas-y-busqueda`), sólo con fixtures en
loopback y en la simulación; nada se probó contra Discord. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#lecturas-búsqueda-y-miembros-remotos-2026-10-17).

- [x] ACK al servidor (`.../messages/{m}/ack` con su token, `/guilds/{g}/ack`),
  marcar como no leído (`manual`) y `MESSAGE_ACK` entrante, con la versión
  del `read_state` del `READY`; capacidades `read_state`, `remote_unread` y
  `server_read`. Hilos por la misma ruta de canal. Falta el ACK en lote
  (`/read-states/ack-bulk`), que Serein no usa.
- [x] `PASSIVE_UPDATE_V2` y `V1`: no leídos (`Event::LatestMessages`) y voz.
- [x] Búsqueda remota con filtros (`from:`, `mentions:`, `has:` con sticker,
  `in:`, `before:`/`after:`/`during:`, `pinned:`, `author_type:`), páginas
  de 25, índice no listo con un reintento, autocompletado de filtros y
  combinación con lo cargado; capacidad `message_search`.
- [x] Buscar miembros con op 8 / `GUILD_MEMBERS_CHUNK` para `@` y para
  `from:`/`mentions:`, con nonce, límite, espera al tipeo, ritmo y caché;
  capacidad `member_search`.

## P3. Comandos de aplicación e interacciones

Hecho el 2026-10-17 en la rama `comandos-e-interacciones`, sólo con
fixtures (ver [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones)
y [todo.md](todo.md#paridad-p3-comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones)).

- [x] Comandos de barra de aplicaciones (`application-command-index` de
  servidor y de privado con bot, con caché de 5 min e invalidación por
  `GUILD_APPLICATION_COMMAND_INDEX_UPDATE` y READY; grupos, subcomandos,
  opciones tipadas con `choices`, rangos y largos validados en local,
  sugerencias del bot por tipo 4; permisos, overrides y contextos como
  Serein) (L). **Fuera de alcance** (el dueño, 2026-10-18: sólo lo que
  tiene Serein, y Serein no lo tiene): opciones de adjunto, índice de
  aplicaciones del usuario, reintentos del `202`, mensaje provisional,
  menús contextuales (tipos 2 y 3) y `ephemerality_reason`.
- [x] Interacciones: botones, menús de texto, personas, roles, canales y
  menciones (con valores por defecto), modales (texto corto y largo,
  menús, opción única, varias, casilla), `INTERACTION_CREATE/SUCCESS/FAILURE`,
  efímeros con «Descartar» (M-L). Habilitadas por el dueño el 2026-10-17.
  Subir archivos en un modal (19), hecho el 2026-10-18 en la rama
  `comandos-de-apps`, sólo con fixtures.
- [x] Comandos locales `/shrug`, `/tableflip`, `/unflip`, `/me`, `/spoiler`,
  `/gif`, `/sticker`, `/msg`, mezclados con los de aplicaciones en `/` y
  marcados «RUNE» (S).

## P4. Acciones de mensaje, compositor y moderación

Hecho el 2026-10-17 (rama `acciones-de-mensaje`), sólo con fixtures en
loopback y en la simulación; nada se probó contra Discord. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#acciones-de-mensaje-compositor-y-moderación-2026-10-17-rama-acciones-de-mensaje).

- [x] Reenviar un mensaje con selector de destino: búsqueda por nombre,
  servidor o personas, hasta 5 destinos, mensaje opcional después;
  capacidad `forward_messages`.
- [x] Crear encuestas (pregunta, 2 a 10 respuestas con emoji, 1 h a 1
  semana, respuesta múltiple) y terminarlas antes, con confirmación;
  `create_polls`. Además: ver quién votó cada respuesta (Serein no lo hace),
  `poll_voters`.
- [x] Ver quién reaccionó, por emoji y en páginas de 100; `reaction_lists`.
  Súper reacciones: se muestran aparte con sus colores desde el 2026-10-18
  (Serein sólo lee `me_burst`); no se mandan ni se lista quién las hizo.
- [x] Copiar el enlace del mensaje; abrir `discord.com/channels/...` (y
  `ptb.`, `canary.`, `discordapp.com`) dentro de RUNE con pastillas
  «Servidor › #canal › mensaje»; un canal no cargado se pide por id
  (`load_channels`; Serein no lo hace).
- [x] Confirmar antes de abrir enlaces externos, con «Confiar en este
  dominio» recordado en el equipo (Serein sólo tiene un interruptor global)
  y aviso de enlaces enmascarados; los del servicio abren sin preguntar
  (`trusted_links`).
- [x] `@silent` (flag 4096), con aviso en el compositor y entre las opciones
  de `@`; «@ ACTIVADO/DESACTIVADO» al responder (`replied_user`). Con
  archivos, una respuesta sigue sin mencionar.
- [x] Atajos Ctrl+B/I/U/E y Ctrl+Shift+X/C/P que envuelven la selección, la
  mantienen seleccionada y la desenvuelven al repetirlos (PR #74 a limn).
- [x] Marcas `<t:…>` en la zona local, con la fecha completa en su tooltip
  (PR #82 a limn, 2026-10-18; el aviso al hacer clic sigue, para el
  teclado) y las relativas al día cada 30 s.
- [x] Marcas de Discord en el Markdown, como las dibuja Serein:
  `__subrayado__`, `~~tachado~~`, `***negrita cursiva***` y `-# subtexto`
  (PR #82 a limn, 2026-10-18), y un enlace dentro de `||spoiler||` sin los
  `||` a la vista. Faltan `_cursiva_`, `>>>` y encabezados en negrita (ver
  `todo.md`).
- [x] Resaltado de sintaxis en bloques de código (PR #73 a limn); desde el 2026-10-18 con los 24 lenguajes de Serein (paquete 10).
- [x] Moderación de voz: silenciar, ensordecer, mover y desconectar desde el
  menú de cada persona, con permisos y jerarquía; `voice_moderation`. Desde
  el 2026-10-18 también arrastrando a la persona a otro canal de voz, como
  Serein (que sólo toma la caída en la fila del canal; RUNE también en la
  gente que está debajo).
- [x] Abandonar un servidor sin `manage_servers` (`leave_servers`).

## P5. Presencia rica, identidad visual y expresiones

Hecho el 2026-10-17 en la rama `presencia-e-identidad`, sólo con fixtures
en loopback y en la simulación; nada se probó contra Discord (ver
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#presencia-rica-identidad-visual-y-expresiones-2026-10-17-rama-presencia-e-identidad),
[PROFILES.md](../docs/PROFILES.md#presencia-rica-e-identidad-visual-2026-10-17-p5),
[EXPRESSIONS.md](../docs/EXPRESSIONS.md#plan-de-la-cuenta-paquetes-y-gif-del-servicio-2026-10-17-p5)
y [todo.md](todo.md#paridad-p5-presencia-rica-identidad-visual-y-expresiones-2026-10-17-rama-presencia-e-identidad)).

- [x] Presencia con varias actividades (hasta 4): imágenes de la aplicación
  y `mp:` por el CDN y el proxy de Discord, transcurrido/restante y barra
  de un tema, plataforma por `client_status`, en la tarjeta, el perfil
  completo y la lista (M). El ícono de la aplicación cuando la actividad
  no trae imagen, como el cliente oficial (`/applications/{id}/public`,
  2026-10-17). Tapas de Spotify, Twitch y YouTube desde sus hosts desde el
  2026-10-18 (rama `perfil-tarjeta-y-cosmeticos`). Botones: Serein no
  los muestra.
- [x] Emoji de servidor como imagen en la línea: mensajes, respuestas,
  biografía y estado personalizado; grandes en un mensaje que es sólo
  emoji (PR #79 a limn, `TextSpan::image`, 2026-10-17).
- [x] Insignias y cuentas conectadas en el perfil, con glifos propios y
  enlaces a las cuentas como Serein (S-M).
- [x] Íconos y emoji de rol, nombres en el degradé del rol (PR #75 a limn,
  `Element::foreground_gradient`) y etiqueta de servidor en mensajes,
  miembros y perfil (S-M).
- [x] Decoraciones de avatar (animadas en el perfil abierto, APNG),
  nameplates en la lista y efectos de perfil sobre la tarjeta, con
  movimiento reducido respetado (M). **Falta**: marcos de perfil (tipo 3),
  que necesitan las medidas de cada capa y fixtures reales. Desde el
  2026-10-18: animadas bajo el puntero, nameplates por sku y, como Serein,
  interruptores para ocultar decoraciones, nameplates y efectos sin
  descargarlos (rama `perfil-tarjeta-y-cosmeticos`).
- [x] `premium_type`: emojis y stickers de otros servidores y animados
  según el plan, con candado y motivo en el selector; paquetes de stickers
  estándar (`/sticker-packs`) en el selector y enviables (S-M).
- [x] GIFs del propio servicio (`/gifs/search`, tendencias con categorías,
  `/gifs/suggest` y `/gifs/trending-search`) junto al GIPHY opcional, con
  los mismos favoritos (S-M).
- [x] Limpieza: un solo lector de `settings-proto` para la cuenta (1) y los
  GIF favoritos (2).

## P6. Escritorio, idioma y voz local

Hecho el 2026-10-17 en la rama `escritorio`; las traducciones, el mismo
día en la rama `idiomas`. Probado con buses D-Bus y sockets privados, la
simulación y fixtures en loopback, nada en un escritorio ni contra Discord.
Detalle en [PLATFORM.md](../docs/PLATFORM.md),
[ARCHITECTURE.md](../docs/ARCHITECTURE.md#idiomas) y
[todo.md](todo.md#paridad-p6-escritorio-idioma-y-voz-local-2026-10-17-rama-escritorio).

- [x] Bandeja del sistema (StatusNotifierItem y dbusmenu sobre zbus, sin
  GTK) con abrir, silenciar, ensordecer, estado y salir, punto rojo y
  tooltip con menciones y no leídos; «Cerrar a la bandeja» apagado por
  defecto (Serein lo trae encendido y su bandeja de Linux no cuenta nada) (M).
- [x] Instancia única por socket en `$XDG_RUNTIME_DIR`; además pasa los
  enlaces a la primera (Serein no) (S).
- [x] Iniciar con la sesión por XDG autostart (Serein no lo tiene en
  Linux) (S).
- [x] Interfaz en español e inglés (Serein: 13 idiomas con Fluent) (L):
  ajuste «Idioma» (Sistema, Español, English) que se aplica en vivo, el
  sistema por `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`; crate `rune-i18n`
  sin dependencias, con el español como clave y unas 3700 entradas en
  inglés, fechas relativas y absolutas, plurales y 12/24 h; tests de
  catálogo completo y de la interfaz simulada en inglés. Más idiomas
  quedan fuera (decisión del dueño: español e inglés).
- [x] Tema claro/oscuro del sistema por el portal Settings, en vivo (S).
- [x] Temas propios (2026-10-19, rama `temas-propios`): editor de los 22
  colores sobre Ónix, Oscuro o Claro con vista previa, hasta ocho en el
  equipo, importar y exportar como archivo de datos validado (sin Wasm,
  imágenes ni catálogo remoto), color principal con preajustes y
  `ColorPicker`, y transparencia de la ventana de 0 a 90 % (el desenfoque,
  del compositor). Serein además trae imágenes de fondo, tipografía y
  métricas por tema, y pide el desenfoque nativo de KDE (M-L).
- [x] Elegir la fuente de la interfaz entre las del sistema, con sus pesos
  medio y negrita ([PR #76 de limn](https://github.com/Niiihuel/limn/pull/76)); la escala ya
  existía (zoom 50–200 %; Serein 50–150 %) (S-M).
- [x] Ancho de la lista de canales y de miembros ajustable y recordado
  (Serein sólo la de canales), canales favoritos, privados fijados y
  mostrar canales ocultos (S-M).
- [x] Código de privacidad de la llamada (DAVE) en Detalles de voz y en
  Ajustes, y el código de verificación con cada persona (Serein no lo
  muestra) (S).
- [x] Atajos globales de silenciar, ensordecer y «pulsar para silenciar»,
  en la misma sesión del portal que «pulsar para hablar» (S).
- [x] Llamadas en privados y grupos (2026-10-17, rama `llamadas-privadas`):
  llamar, sonar, aceptar, rechazar, volver a llamar y dejar de llamar a
  alguien, en otro dispositivo, transmitir y cámara, con aviso entrante,
  notificación del sistema y timbres propios; sólo fixtures (M-L).
- [x] Socket local `discord-ipc-N` (Rich Presence de otras apps, sin TCP) y
  detección de juegos por nombre de proceso con la lista pública de
  Discord, cada uno apagado por defecto con su interruptor (M).
- [x] Ocultar las decoraciones de la ventana (Serein: «Hide window
  decorations» en Linux) y elegir la GPU que dibuja («Render with», con la
  actual y «se aplica la próxima vez»): 2026-10-18, rama
  `escritorio-linux`. RUNE ofrece cada fabricante detectado en vez de
  «alto rendimiento / ahorro», y las variables del entorno siguen mandando
  (S).
- Corrector ortográfico: Serein no lo tiene; queda el diseño del recorrido
  21.

Menores: proxy HTTP para REST. Hecho: hora de inicio del canal de voz
(op 43, 2026-10-18, `voz-protocolo`, sólo fixtures; RUNE lo muestra en la
fila de la sala, Serein en su panel de voz).
Hecho: conversión de emoticones (2026-10-18, `envios-y-subidas`,
Apariencia › Mensajes, apagada por defecto). ThumbHash: hecho el
2026-10-18 (rama `contenido-de-mensajes`, sólo fixtures), con «Copiar
imagen» y «Copiar video» como Serein. El detalle de emoji se hizo el
2026-10-18 (rama `emoji-y-estados`): tarjeta al tocar un emoji de un
mensaje o con clic secundario en una reacción, con «Copiar emoji».
Lectura y caché de imágenes: hecho el 2026-10-18 (rama
`imagenes-y-lectura`). Caché de imágenes en disco con medios y arte
compartido separados, uso, «Borrar» y retención, por cuenta y sellada con
su clave (Serein la guarda en claro); «Animar GIF» (apagado, anima al
pasar el puntero), «Ocultar los enlaces de imagen y GIF» y velocidad de la
rueda y del trackpad. El ajuste de Serein «Keep the server member list
open when the window is wide enough» ya existía en RUNE como
`members_open`, que se recuerda; «Mantener la lista de miembros en
ventanas angostas» (la conversación baja a 280 px antes de esconderla) es
una extensión de RUNE, no de Serein, que salió de leer así el plan.
Quedan de ese grupo de Serein la reacción con doble clic y la lista de
miembros en privados y grupos (RUNE no muestra lista ahí); el
desplazamiento suave sigue atado al movimiento reducido.

## P7. Administración que falta (después del recorrido 19)

Hecho el 2026-10-17 en la rama `administracion-2`, sólo con fixtures en
loopback y en la simulación; nada se probó contra Discord. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#resto-de-la-administración-p7-2026-10-17-rama-administracion-2)
y [todo.md](todo.md#paridad-p7-administración-restante-2026-10-17-rama-administracion-2).

- [x] Perfil del servidor con ícono y rasgos (`PATCH /guilds/{g}/profile`,
  rasgos leídos con `GET`) (S-M). Desde el 2026-10-19 (rama
  `admin-servidor-y-miembros`) también rasgos con emojis propios
  (`:nombre:`), banner e ícono animado donde las mejoras los desbloquean.
- [x] Seguridad y Participación: canal del sistema y sus cuatro mensajes,
  notificaciones por defecto, canal y tiempo de inactividad, verificación y
  filtro de contenido (`PATCH /guilds/{g}`, flags leídos frescos; una
  comunidad conserva sus mínimos) (M). Página propia en los ajustes. Desde
  el 2026-10-19 también activar o desactivar la comunidad con sus canales
  de reglas y de novedades (Serein no lo hace).
- [x] Pausar invitaciones (`features` con `INVITES_DISABLED`, leídas
  frescas) (S).
- [x] Reordenar canales en lote (`PATCH /guilds/{g}/channels`) (S-M), en una
  página «Canales» con arrastre (limn ya lo permite) y menú (subir, bajar,
  mover a una categoría o sacarlo). Desde el 2026-10-18 (rama
  `admin-canales-y-voz`) la barra lateral también mueve en lote, pregunta
  si sincroniza permisos al pasar a otra categoría (`lock_permissions`,
  como el cliente oficial; Serein los sincroniza siempre) y no corre a los
  canales que la cuenta no ve.
- [x] Ajustes de foro: etiquetas (crear, editar, borrar, emoji, moderada),
  reacción por defecto, vista, orden y etiqueta obligatoria (M). Desde el
  2026-10-19 la reacción puede ser un emoji del servidor, y están la
  duración de archivado y el modo de etiquetas (`default_tag_setting`).
- [x] Ícono (imagen o emoji Unicode) y degradé de rol donde el servidor lo
  permite (`ROLE_ICONS`, `ENHANCED_ROLE_COLORS`), conteo de miembros por rol
  (`GET /guilds/{g}/roles/member-counts`) (S-M).
- [x] Página de miembros con orden (cuatro, como Serein), filtro de los
  últimos 7 días y forma de entrada (`join_source_type` y el código de la
  invitación del buscador de miembros) (S-M).

## Lo que RUNE no replica

- Huella de cliente falsa: User-Agent de navegador, `X-Super-Properties`,
  build fijo, `properties` del Identify (también en los WebView).
- Captcha automatizado. (El dueño autorizó el 2026-10-17 el hCaptcha que
  resuelve la persona en cualquier escritura: hecho e integrado.)
- Captura del token por JS inyectado o pegado: RUNE tiene su login propio.
- Servidor IPC/RPC `discord-ipc-N` y detección de juegos por procesos se
  hicieron (P6), apagados por defecto con su interruptor (dueño,
  2026-10-17). No se replica su WebSocket en `127.0.0.1:6463–6472`: RUNE
  no abre puertos TCP.
- Copiar la URL de un webhook: lleva su token, una credencial; RUNE lo
  descarta al leer la lista.
- Token de Spotify de las conexiones, subidas a servicios externos (Catbox,
  Litterbox, x0.at), conservar mensajes borrados por otros, extensiones Wasm,
  sonidos o locales copiados.

## Pedido, para después

- Actualizador propio (pedido del dueño el 2026-10-18; antes estaba en la
  lista de arriba): RUNE se actualiza desde las etiquetas (tags) de su
  repositorio, por HTTPS, verifica el hash o la firma de lo que baja y pide
  confirmación a la persona antes de instalar. **Hecho el 2026-10-19**
  (rama `actualizaciones`, [UPDATES.md](../docs/UPDATES.md)): Serein sólo
  empaqueta releases; RUNE además publica un manifiesto firmado, instala
  por usuario de forma atómica y distingue obligatorias (antes de abrir)
  de opcionales (con confirmación).
