# Pendientes de RUNE

Lo que falta de verdad, con su porqué, y lo que el dueño tiene que probar
contra el servicio real, por área. Lo hecho sale de esta lista (queda en
git y en los docs del área). Todo lo del proveedor no oficial está probado
sólo con fixtures en loopback y la simulación, salvo lo que el dueño ya
confirmó (login, identidad, servidores, canales, historial, miembros y
perfiles, 2026-10-14 y 2026-10-17). Las listas detalladas de pasos están en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md) y
[PLATFORM.md](../docs/PLATFORM.md).

## Orden del dueño (2026-10-18)

1. Terminar lo pendiente de esta lista.
2. **Optimización**: arranque desde caché con cuentas reales, costo del
   layout y del scroll, listas grandes, pedidos repetidos y las mediciones
   que faltan en [PERFORMANCE.md](../docs/PERFORMANCE.md).
3. **Windows y macOS**: Windows compila, tiene sus servicios, login,
   medios e instalador, sin probar en un Windows real; macOS sin empezar.

## Rendimiento

Pendiente de medir (números actuales en
[PERFORMANCE.md](../docs/PERFORMANCE.md)):

- Memoria con imágenes y GIF reales y una cuenta con muchos servidores
  (el panel F12 muestra CPU y RSS en vivo).
- Latencia de entrada con eventos reales de teclado (`--bench` usa rueda
  sintética); arranque en frío con el filtro de GPU.
- El filtro de GPU en una máquina sólo Intel y en una de una sola GPU
  discreta (cubierto por tests, no por hardware).
- El layout de las filas del timeline si sigue creciendo (p50 3,07 ms en
  `--bench`); la CPU de una llamada en la ventana.

## Bloqueado aguas arriba

- `instant` sin mantenimiento (RUSTSEC-2024-0384): llega por la feature
  `js` de openmls que activa `davey` (DAVE). Ignorado con motivo en
  `deny.toml`; quitarlo cuando `davey` deje de activarla.
- zbus 5 (RUSTSEC-2026-0370/0371/0372, sin vulnerabilidad): lo piden
  accesskit y `secret-service`; ignorado con motivo en `deny.toml`.
- Arrastrar y soltar archivos: eframe/egui-winit 0.37 sigue en winit
  0.30.13 (sin arrastre en Wayland, sin posición en X11) y con su
  portapapeles fijo; `limn-dnd` necesita un dueño Wayland compartido dentro
  de eframe. Destrabarlo pide parchear egui dentro de RUNE, que no se hace;
  se destraba si egui publica la inyección del portapapeles o pasa a winit
  0.31 (`DataTransfer`). Hoy el drop se rechaza conservando el borrador y
  `Client::prepare_attachment_paths` ya está listo.
- Pegar una imagen sola con Ctrl+V: egui-winit no entrega Ctrl+V sin texto;
  hoy se pega desde «Pegar del portapapeles». Mismo bloqueo.
- La inyección del portapapeles en egui (`ClipboardBackend`, fábrica por
  viewport, pegado diferido) son commits locales sin publicar en
  `~/projects/personal/egui-clipboard-hook`, rama
  `feat/clipboard-backend-factory`, sobre egui 0.36.2: rebasar a 0.37 y
  proponerla a emilk/egui. El `WaylandDataOwner` de limn todavía no bindea
  Primary ni copia, así que no reemplaza a smithay-clipboard.
- `vendor/hpke-rs`: quitar el parche cuando openmls acepte
  `hpke-rs >= 0.7`.
- Pellizco en Linux: winit 0.30 no lo entrega (el visor ya lo maneja en
  macOS y como Ctrl+rueda en Windows).
- Foco del SO al cerrar el login en Wayland: winit/eframe no exponen pasar
  el token xdg-activation del auxiliar; sólo queda `request_user_attention`.

## Repositorio y publicación

Pendiente:

- `THIRD_PARTY_NOTICES.md` se regenera después de cada mezcla que cambie
  `Cargo.lock`; si no, el workflow de release se detiene.
- `inout 0.2.3` está retirado de crates.io: `cargo update -p inout` la
  próxima vez que se toque `Cargo.lock`.
- Si se distribuyen con RUNE las bibliotecas nativas LGPL (GTK,
  WebKitGTK, GStreamer), sumar sus avisos y condiciones.
- `video::decode::tests::a_key_frame_decodes_…` y otras pruebas de video
  con plazos de 5 s fallaron alguna vez con la máquina cargada: si se
  repite en la CI, medir por cuadros en vez de por tiempo.
- Código que sólo usan los tests (limpieza del 2026-10-21): se borró
  `screencast::camera_present` y lo que sólo probaba su propio test; lo que
  siembra o mira los tests de su crate quedó tras `#[cfg(test)]`
  (`StorageWorker::save`, `Client::account`, `Storage::delete_message`…).
  Siguen `pub` porque los usan tests de otros crates: `Storage::in_memory`,
  `save_messages`, `setting` y `count`, los atajos de `MockProvider`,
  `Timeline::newest`, `Voice::speaking`, `rune_i18n::catalog` y
  `rune_update::serve`.

Lo hace o prueba el dueño:

- La CI (Linux y Windows) no corrió todavía en GitHub; el primer push dirá
  si el runner alcanza. Los tests corren sin `--all-features` (sin WebKit);
  clippy sí las cubre.
- Actualizaciones: generar la clave (`scripts/update-key.sh`), commitear
  `release-key.pub`, hacer público el repositorio, poner
  `RUNE_WINDOWS_RELEASES=true` si se quieren releases de Windows y empujar
  el primer tag (el job de Windows, primero con una pre-release
  `v0.1.x-rc.1`). Sin probar: el workflow en Ubuntu, el binario genérico en
  Ubuntu/Fedora/Arch y en NixOS con `nix-ld`, una actualización opcional y
  una obligatoria reales desde GitHub Releases. Las releases genéricas no traen el visor de PDF (sus
  herramientas sólo se aceptan desde `/nix/store`).
- Firma Authenticode del instalador de Windows: no hay; SmartScreen avisa
  y el Control inteligente de aplicaciones puede bloquear. Decide el dueño
  (Azure Artifact Signing, CA OV, SignPath Foundation o no firmar);
  opciones en [UPDATES.md](../docs/UPDATES.md).

## Cuentas, sesión y almacenamiento

Pendiente:

- Un login interactivo nuevo manda un `Ready` provisional sin servidores:
  una lectura que esperaba la conexión justo en ese instante se mandaría
  antes de tiempo (poco probable; `rune-core/src/connecting.rs`).
- Borradores en memoria no se vacían al cambiar de cuenta: uno de la cuenta
  anterior puede tapar el restaurado en un canal compartido (baja; hace
  falta saber de qué cuenta es cada uno).
- Una imagen pegada sin archivo, una ruta que no es UTF-8 y los archivos de
  una publicación de foro a medio escribir no se guardan con el borrador.
- Caché de imágenes: una URL firmada que se renueva se vuelve a bajar (el
  núcleo no sabe qué parte de la URL es estable); un origen en espera por
  429 no sirve lo que ya está en disco.
- Identidad de navegador, lo que todavía difiere del cliente web: API y
  Gateway v10 sin `zlib-stream` (el web usa v9), aceptación de invitación
  sin el `session_id` del Gateway, idioma del sistema en lugar del de la
  cuenta, user agent `RUNE/…` en las descargas del CDN, `launch_signature`
  nulo (lo firma el WebAssembly del cliente web),
  `client_heartbeat_session_id` que dura todo el proceso; en Windows, el
  WebSocket sin client hints y `Accept-Language` con la regla de Linux. Se
  cambian si Discord las distingue.
- Llavero: una escritura abandonada por el límite de 15 s al cerrar sesión
  puede completarse después en el servicio.
- Caché cifrada por columna: SQLCipher cifraría también ids y ajustes
  (mejora opcional, si hiciera falta).
- Leer el motor de WebView2 cuesta un arranque oculto en cada inicio del
  proveedor (alrededor de un segundo, sin medir).

Probar contra el servicio real:

- Volver a iniciar sesión con la identidad de navegador; si Discord pide
  teléfono; que el resto siga andando.
- Sesión recordada en un Secret Service real (GNOME Keyring, KWallet, con
  sus diálogos de desbloqueo), varias cuentas, avatar en la
  lista, «Cambiar de cuenta», «Cerrar sesión» mientras se verifica la
  cuenta guardada (al reabrir ya no está), «Reintentar ya» con la red
  cortada.
- Caché cifrada con la cuenta real: arranque desde la instantánea, cerrar
  sesión borra datos y clave.
- Caché de imágenes en disco: segundo arranque sin bajar avatares ni emoji,
  «Borrar» por parte, presupuesto menor, que cerrar sesión borre la carpeta
  de la cuenta.
- Cambio de proveedor en caliente hacia la sesión real.
- OAuth2: registrar el redirect `http://127.0.0.1:48731/oauth/callback` en
  la aplicación (el primer intento falló por `redirect_uri` no válido),
  consentimiento y canje PKCE sin secreto, y la revocación al cerrar sesión.
- hCaptcha en cualquier escritura: resolverlo, cancelarlo y cerrar la
  ventana.
- Foco al cerrar el login en Hyprland, GNOME y KDE.

## Invitaciones, reglas y onboarding

Pendiente:

- El resultado de unas reglas en revisión vive en memoria: al cerrar RUNE
  el diálogo vuelve a ofrecerlas. Falta confirmar por REA cómo lee el
  oficial el estado de una solicitud (`GET …/requests/@me`?).
- Editor del onboarding: reordenar preguntas y opciones y elegir un emoji
  del servidor (hoy sólo Unicode por texto). RUNE pide un canal por defecto
  donde todos escriban; el resto de los requisitos los rechaza Discord con
  su motivo. Límites (15 preguntas, 50 opciones, largos) sin confirmar por
  REA.
- Invitaciones de aplicaciones o transmisiones (`target_type`): se
  rechazan al aceptar. Las verificaciones de teléfono o correo remiten a la
  aplicación oficial.
- Cambiar las respuestas del onboarding después de entrar
  (`PUT …/onboarding-responses`): sin pedir; Serein no lo tiene.

Probar contra el servicio real:

- Vista previa automática de tarjetas, modal, caducidad y 429.
- Aceptar invitaciones de servidor, grupo, amistad, invitado y evento, con
  hCaptcha si aparece (un solo reintento) y «Abrir en el navegador» tras un
  rechazo. Si una aceptación falla, mandar las líneas
  `rune: invitation:` del log (dicen si fue un 403 con código, un 401 o un
  rechazo local).
- Una comunidad con reglas y onboarding (roles y canales que da), reglas
  revisadas por moderación (`SUBMITTED`), editar el onboarding de un
  servidor propio, y que la barra no aparezca donde ya se participa.

## Mensajes y contenido

Pendiente:

- Emoji como imagen en los extractos de búsqueda: falta en limn un
  subtítulo de `ListRow` que acepte un elemento (PR nueva); después
  `SearchHit` lleva las referencias de emoji.
- Markdown: `_cursiva_` con un guion bajo, citas `>>>` de varias líneas y
  encabezados en negrita (PR a limn).
- Falta un texto plano compartido con menciones y emoji resueltos: la
  tarjeta de foro, el fragmento de una respuesta y las notificaciones
  muestran los tokens crudos.
- Reacciones con emoji propio de servidor: no se modelan. Súper
  reacciones sin la paleta de contraste de `burst_colors`.
- Orden «más antiguos» en la búsqueda remota: no se pidió; la mezcla con lo
  cargado necesitaría otra regla.
- Búsqueda (diseño del 2026-10-19): los filtros escritos (`from:`, `has:`…)
  sólo se entienden en «Todo el historial»; en «Cargados» el texto es
  literal (decisión anterior, con test). Lo guardado en este equipo no
  trae adjuntos (`SearchHit` sólo dice si hay archivos), así que sus
  resultados no muestran miniaturas; las menciones del extracto siguen
  crudas (mismo pendiente que el texto plano compartido).
- Voto de encuesta: con la sesión sólo lo confirma el eco del Gateway; sin
  eco, o tras más de 30 s detrás de un 429, se deshace un voto aceptado.
  Arreglo: confirmar por el 204 y descartar el eco por `Tallies`.
- Una reacción o un voto en vivo hace fallar la carga de contexto en curso
  («Volvé a intentarlo»). Reacciones y ediciones no tienen plazo propio en
  core (vence el HTTP de 15 s).
- Contenido: `goreContentSettings` (violencia, autolesiones) no se lee; no
  se tapan adjuntos con escaneo pendiente; el valor «sin elegir» supone una
  cuenta adulta (`nsfwAllowed`).
- «Unirse a la llamada» desde el registro de una llamada en curso.
- Copiar imagen/video: baja el archivo antes de saber si hay portapapeles,
  y un video copiado reemplaza al anterior antes de escribirlo; falta una
  consulta previa en `rune-platform`. macOS sin portapapeles.
- Fuentes: una secuencia (bandera, keycap, ZWJ) cuyo primer carácter está
  en una cara anterior no toma el respaldo nuevo (egui elige la fuente por
  el primer carácter).
- Buscar en un mensaje de voz Ogg decodifica desde el principio (atajo).
- Jumbo de emoji Unicode en pantallas con escala mayor que 1,5: el arte es
  de 72 px.
- Tooltip de un tramo (fecha `<t:…>`, «(editado)») no lo lee un lector de
  pantalla; el borrador resalta cualquier `@palabra` aunque no exista.

Probar contra el servicio real: reacciones (también súper reacciones
mostradas), fijados (`PIN_MESSAGES`), typing (y si llega el de servidores
sin suscripción), búsqueda remota (relevancia, `has:poll`, `sitio:`),
lecturas y no leídos con otro dispositivo, encuestas (crear, votar,
terminar, quién votó), reenvíos con referencias, componentes y comandos de
apps (botones, menús 5–8, modales con archivos y tipos permitidos, emoji
animados en botones), `@silent`, enlaces de mensajes, marcas Markdown y
`<t:…>`, mensajes de sistema y registro de llamadas, ThumbHash,
difuminado de explícito, emoji en línea, jumbo y en estados, «Más GIF»
(¿Discord respeta `offset`?), GIF favoritos sincronizados, búsqueda GIPHY
con una clave propia, candados de Nitro y permisos externos, conversión de
emoticones. Fijar con un rol que sólo tiene «Gestionar mensajes» (¿lo
acepta Discord?). Súper reacciones hechas desde el oficial (colores,
cambios en vivo, tras reiniciar). Nombres reales que antes se veían como
cuadraditos (respaldo de fuentes del sistema).

## Hilos, foros y escenarios

Pendiente:

- Tarjetas de publicaciones archivadas (el oficial pide
  `/channels/{foro}/post-data`); la tarjeta no se actualiza si cambia el
  primer mensaje.
- Las publicaciones nuevas piden 3 días de archivado
  (`AUTO_ARCHIVE_MINUTES`) en vez del valor del foro.
- Un hilo silenciado tampoco avisa de menciones (el oficial sí).
- Atajos: archivados de más de 100 en un mismo instante; el menú «más»
  queda abierto sin verse si la ventana se agranda.
- Escenario sin DAVE, si Discord no lo negocia ahí.

Probar contra el servicio real: crear hilos (público y privado), seguir,
escribir, cerrar y reabrir respondiendo, renombrar, foro con
publicaciones no seguidas, etiquetas, archivos y «Cargar más» hasta los
archivados; silenciar un hilo 15 min y para siempre (y que el silencio con
fin no vuelva); tarjetas de foro; escenario (entrar, levantar la mano,
hablar, moderar y su chat).

## Amigos, privados, grupos y llamadas privadas

Hecho (amigos-avatares, 2026-10-19): Amigos (todas las pestañas), los
diálogos de grupo y las sugerencias de @ piden la foto de quien muestran;
antes sólo se veía la de quien otra lista ya había pedido (por ejemplo, un
privado abierto) y el resto quedaba con la inicial.

Pendiente:

- «Rechazada» frente a «sin respuesta» es una inferencia por tiempo (20 s).
- El diálogo «no son amigos» usa textos propios (el catálogo del oficial no
  estaba en los chunks descargados).

Probar contra el servicio real: solicitudes, apodos, spam y bloqueos;
abrir y cerrar privados; crear grupos, miembros, imagen, dueño y
abandonar; llamar, sonar, aceptar, rechazar y atender en otro
dispositivo, `ringable` con alguien que no es amigo, botones de la
notificación en GNOME/KDE.

## Voz y llamadas

Pendiente:

- `aead_aes256_gcm_rtpsize` y el gateway de voz `?v=9`: se quedan como
  Serein (v8, XChaCha20) hasta una prueba real.
- Jitter adaptativo (hoy retardo fijo); op 5 `VOICE_SERVER_PING` en cada
  corte (su efecto no se ve estáticamente); sin `flags`/`preferred_regions`
  en op 4 ni `rtc_connection_id`/`experiments` en Select Protocol.
- Calibrar los umbrales de `VoiceActivity` con otros micrófonos y entornos.
- La compuerta pierde 1–2 % de las sílabas que empiezan con fricativa:
  ajustar con la voz real del dueño.
- Pantalla compartida: escalado por vecino más cercano (bilineal si se nota
  en texto fino); las marcas RTP salen del codificador, no de la captura.
- Latencia de captura e ida y vuelta (hoy sólo la de salida); remuestreador
  de banda limitada si un dispositivo sin 48 kHz suena mal.
- Un nodo `Audio/Duplex` genuino de PipeWire se oculta de los micrófonos
  (cpal no da la clase del nodo).
- El panel de voz cuenta desde que la cuenta se conectó; el tiempo del canal
  está en su fila.
- Menú de una persona (2026-10-19, hecho: casillas al borde, roles con su
  punto de color y casilla sin cerrar el menú, castigos en rojo): falta
  confirmar en el bundle qué ítems pinta de rojo el oficial además de
  `remove-friend` (el resto sale de la captura del dueño); los roles van
  del más alto al más bajo, como en el perfil.
- Eco (2026-10-19, hecho: referencia continua, todo lo que sonó antes de
  cada cuadro, AEC3 que no se reinicia, y el sonido de la PC por el
  loopback de la salida; ver PERFORMANCE.md#eco). Falta: el primer medio
  segundo tras dejar de estar silenciado; AEC3 de Sonora quita poco eco
  mientras la persona habla (su ajuste no está expuesto: PR a Sonora o
  ajustar a mano); con una salida elegida que no es la predeterminada, lo
  que suena en la predeterminada no se cancela; en Windows el loopback no
  sigue un cambio de salida predeterminada hasta reabrir la captura.

Probar contra el servicio real:

- Una llamada con otra cuenta o un cliente oficial y el log
  (`rune: voice …`, `rune: gateway: …`): audio en los dos sentidos, mute,
  ensordecer, pulsar para hablar, colgar, código de privacidad DAVE igual al
  del oficial y verificación con cada persona, cortes y reunión; si la
  cuenta aparece dos veces, mirar `CLIENTS_CONNECT` (op 11). Si falla,
  mandar esas líneas y el texto del aviso.
- Latencia y pérdida frente al oficial, `Waiting` al quedarse solo, tiempo
  de las salas, el sonido de la pantalla (ID 9, valor 4 de la extensión RTP).
- Escucharse («Escucharme» o `cargo run --release -p rune-media --example
  playback -- --loopback --rnnoise --gate`), AEC con altavoces, sonidos de
  la llamada y carpeta propia, volumen por persona, AGC con voz real,
  panel rápido y prueba de micrófono con un micrófono real, un micrófono
  guardado que era un monitor. Si la pantalla compartida va poco fluida,
  mandar las líneas `rune: voice share: video …` y `… network …`.
- Eco con los auriculares del dueño: en una llamada, que otra persona hable
  mientras el dueño calla, habla encima, se silencia y vuelve; y con un
  juego o música sonando en la PC, que nadie lo oiga por el micrófono (y
  que con «Cancelar también el sonido de la PC» apagado sí se oiga). En
  Ajustes › Voz y video la nota de esa opción dice si escucha la salida. En
  Windows, lo mismo y que el loopback abra.

### Filas de personas en voz (voz-filas-y-arrastre, 2026-10-19)

Hecho: resaltado alineado con el de los canales (mismo margen derecho y
radio, 1 px adentro arriba y abajo); tarjeta de quien transmite al pasar el
puntero (vista previa, juego, «Ver transmisión» que abre el escenario);
arrastre a otro canal con la fila levantada como fantasma, destino rodeado,
Escape o soltar afuera cancelan. limn: PR #114 (`rune/hover-popover-anchor`,
47570cc), que mantiene abierta la tarjeta sobre las marcas de la fila.
Capturas `docs/img/voz-filas-*-2026-10-19.png`; acciones de captura
`press:{id}` y `move:{id}` para arrastrar.

Pendiente:

- Tamaño y contenido de la tarjeta sin confirmar por REA: su renderizador
  vive en un chunk diferido que no está entre los bajados; se tomó 288×162
  (16:9, la mitad del 512×288 de la subida) y el contenido de Serein.
- «Ver transmisión» fuera de la llamada no entra al canal (el oficial
  entra y la mira): queda deshabilitado con el motivo.
- La simulación pone a la misma persona en voz en dos servidores; quien
  está en otro canal de un servidor sin permiso de mover no se puede
  arrastrar aunque en este sí se pueda (`voice_state_of` da el primero).

Probar contra el servicio real: el puntero sobre alguien en vivo (que
aparezca la imagen de `GET /streams/{clave}/preview`), «Ver transmisión»
desde la tarjeta, y arrastrar a alguien a otro canal con permiso de mover
(`PATCH /guilds/{g}/members/{u}` `{"channel_id"}`).

## Video y pantalla

Pendiente:

- DMA-BUF de la captura a VA-API (la vista previa y el respaldo necesitan
  los píxeles; pantalla y NVENC pueden estar en GPU distintas).
- NVDEC convierte a RGBA en la CPU (sin NVRTC): no ahorra frente a
  OpenH264.
- Sonido sólo de la ventana compartida: el portal no dice de qué proceso
  es. En Windows el loopback no distingue aplicaciones.
- 1080p60 por software no alcanza en esta máquina (35–40 fps).
- H.265: sólo por el decodificador de la GPU (el de software trae
  patentes); sin hardware para probarlo, no se habilitó.
- Un WebM sin sonido y sin duración (grabación en vivo) no se reproduce.
- Windows: codificador por hardware (MFT de Media Foundation: NVENC,
  Quick Sync, AMF; hoy OpenH264 y el ajuste deshabilitado), respaldo con
  DXGI Desktop Duplication para Windows anterior a 1903, miniaturas en el
  selector de pantallas y ventanas, video en mensajes (MP4 con OpenH264 es
  directo; WebM/MKV necesita otro decodificador).

Probar contra el servicio real y con hardware:

- Mirar la pantalla y la cámara de un cliente oficial de confianza;
  compartir la pantalla y mandar la cámara; NACK/RTX, pausa, varias
  transmisiones, vista previa, sonido de las aplicaciones con PipeWire real,
  token del portal, ventana aparte, simulcast entrante; 1080p60 con VA-API
  y NVENC (perfil y SPS frente a DAVE) y sin Nitro (¿recorta el servidor?);
  cambiar «Aceleración por hardware de H.264» en medio de una transmisión;
  que el sonido de la pantalla no lleve las voces de la llamada.
- xdg-desktop-portal-hyprland con una cámara real (YUY2/NV12/MJPEG) y
  `restore_token`. Cámara (2026-10-19): «use input buffers: -22» al probar
  el video era el stream sin `dataType` en sus búferes (un nodo V4L2 los
  dejaba en su mapeo propio); arreglado y probado en esta máquina con la
  UVC USB2.0 FHD (MJPEG 1280×720 a 30 fps por DMA-BUF exportado,
  `--example camera`, sin guardar cuadros). Falta que el dueño lo vea en
  «Probar video» y en una llamada; la pantalla ahora también pide memoria
  compartida explícita (ver que siga compartiendo). La cámara IR de la
  misma UVC (sólo `GREY`) se lista con el mismo nombre y no se puede leer.
- Transmisiones (2026-10-19, sólo fixtures): enfocar una y volver a la
  grilla, su menú propio (volumen y silencio de la transmisión), la vista
  previa en el mosaico pedida como el oficial (`?version`, 120 s, 429) y la
  subida de la propia (`POST /streams/{clave}/preview`, cada 300 s). Falta
  ver que Discord acepte la miniatura de RUNE y que el oficial la muestre;
  la vista previa de una llamada privada (`call:`) no se pide todavía.
- Llamada como el oficial (2026-10-19, REA de los bundles): la barra con
  la medida de la bandeja oficial; en una transmisión, doble clic a
  pantalla completa (otro la devuelve a la grilla), al pasar el puntero su
  sonido y «⋯», y enfocada «Ventana aparte» y «Pantalla completa» al final
  de la barra y «Dejar de ver» en lugar de colgar; la única de otros en el
  canal se enfoca al mirarla. Falta: en una ventana angosta la barra no se
  achica como la oficial (≤ 550 px esconde la segunda sección), el zoom de
  la transmisión enfocada, el control de volumen al final de la barra y
  probar el doble clic con el mouse real (egui da el doble clic a los
  300 ms, no según el sistema).
- Vista previa local (`cargo run -p rune-desktop`, «Sala común»):
  «Compartir pantalla» abre el portal, cancelar avisa, detener desde el
  sistema avisa; «Encender la cámara» pide permiso, desenchufarla avisa;
  `pw-cli ls Node` no deja `rune-preview` al colgar. Una cámara USB sólo
  MJPEG.

## Archivos, visor y documentos

Pendiente:

- Visor: ocultar los controles quieto, rebote elástico y giro animado
  (limn no tiene inactividad del puntero ni resortes).
- Visor: damero detrás de un GIF o video transparente (las superficies no
  informan transparencia).
- Portapapeles de archivos (pegar, copiar imagen o video) sólo en X11 o
  XWayland: Wayland sin XWayland no lo tiene.
- «Guardar como» publica con la ruta, no con un descriptor de la carpeta
  (atajo; `openat`/`linkat` lo cerraría).
- PDF: sin soporte en Windows ni macOS, ni sandbox empaquetado fuera de
  Nix.
- La posición recordada de un audio o video (no de voz) no se ve antes de
  pulsarlo: su duración se conoce al leer el archivo.

Probar contra el servicio real: subir uno y diez archivos (con y sin
texto, como respuesta con y sin «@»), cancelar un archivo grande, uno sobre
el límite, pegar archivos e imagen en Hyprland con XWayland, archivos del
borrador tras reiniciar, una publicación de foro con barra de progreso;
«Guardar como» y reemplazar con el portal real (GTK y KDE), descargas y
copiar imagen o video sobre adjuntos reales, reenvíos con archivos; el visor con una imagen chica, una foto
grande (original al abrir si el ajuste lo pide, o al hacer zoom), un PNG transparente; en densidad 2, vistas previas, avatares grandes,
banners y stickers nítidos (el `size=` del CDN por la densidad), un GIF, un video y
un PDF; mensajes de voz reales (duración, sin cola, un mensaje que suene
encima), WebM y MKV reales, stickers Lottie, búsqueda resaltada en un PDF
largo.

## Perfiles, presencia y riel

Pendiente:

- Marcos de perfil (collectibles tipo 3): necesitan las medidas de cada
  capa y fixtures reales.
- Efectos de perfil: un pedido por perfil abierto sin caché; las capas se
  animan juntas (sin `start`/`loopDelay`). Un perfil leído con los efectos
  ocultos no trae el efecto al volver a mostrarlos hasta el próximo pedido.
- Editor de perfil: capacidad propia para imágenes animadas (hoy
  `profile_images`); la vista previa no anima el GIF; una animación de más
  de 4 MP se encuadra quieta.
- Estilos de nombre (fuente, efecto) y banners animados en el perfil.
- La tarjeta de voz sigue a su fila (no flota fija como las demás); sin la
  lista de miembros abierta la tarjeta no se suscribe a la presencia de su
  persona (el op 37 de una sola persona necesita un canal).
- Lista de miembros: presencia sólo de filas cargadas; hasta 50 000 filas
  (límite de la lista medida de limn).
- Riel: el número de oyentes de un escenario va en una fila aparte (falta
  texto al final de `Tooltip::faces` en limn); sin desplazamiento
  automático al arrastrar cerca del borde; orden por llegada, no por
  afinidad. Un lanzamiento de app que terminó con su servidor no
  disponible queda hasta el próximo READY (`GUILD_CREATE` agrega, no
  reemplaza).

Probar contra el servicio real: tarjeta de 300 px (lugar, cuerpo
desplazable, cerrar al desplazar), nameplates animadas, decoraciones bajo
el puntero, ocultar cosméticos, perfil vencido al minuto, portadas de
Spotify/Twitch/YouTube, edición de perfil y perfil por servidor (con y
sin Nitro, con hCaptcha, avatar y banner animados), actividades de amigos,
lista de miembros, y los seis problemas de la primera prueba real (tarjeta
que parpadeaba, tarjeta de voz sin datos, emoji en la biografía, imagen de
actividad, efecto angosto, cuadraditos en nombres); riel: arrastrar servidores y carpetas, «Sólo menciones» por
defecto, menciones de roles, silenciar privados, tooltip con AFK,
escenario y apps; avatares por defecto; selector de emoji con muchos
servidores sin parpadeo y con la CPU que baja al quedarse quieto.

## Administración

Pendiente:

- La página «Canales» no ofrece sincronizar permisos (mueve sin
  `lock_permissions`).
- Elegir un emoji del servidor en rasgos y reacción del foro es escribir
  `:nombre:`: no hay selector con imágenes.
- AutoMod: reordenar reglas, alerta de ataques de menciones, validar en el
  servicio, disparadores sin documentar (se muestran, no se editan).
- Eventos: portada al crear o editar, repetición, cancelar sin borrar,
  privacidad, enlaces `discord.com/events/…`, eventos del `GUILD_CREATE`.

Probar contra el servicio real: todas las familias (servidor, canales,
roles, permisos, miembros, aislar, vetos, auditoría, invitaciones,
expresiones, webhooks, integraciones, podar, perfil y rasgos, banner e
ícono animado, comunidad, seguridad, foro, conteo por rol, mover personas
de voz), AutoMod (y si llegan sus eventos y el mensaje propio en el `400`),
eventos programados (lista, interés, crear, editar, iniciar, terminar,
borrar, permisos, cambios del oficial).

## Escritorio Linux

Pendiente:

- «Iniciar minimizado» con la sesión: Wayland no crea una ventana oculta;
  habría que arrancar en la bandeja. En KWin RUNE minimiza (Serein oculta
  con un script).
- Lista propia de juegos (agregar, renombrar, ocultar) y caché en disco de
  la lista de Discord.
- GPU: dos del mismo fabricante no se eligen por separado; AMDVLK no entra
  en `*radeon*`.
- Las imágenes de perfil, grupo y expresiones abren su propio selector;
  el diálogo de `BindShortcuts` va sin parent.
- Temas: con transparencia el riel y las columnas toman el color de la
  conversación; el `ColorPicker` es opaco (el alfa se elige en el archivo);
  no se pide el desenfoque nativo de KDE.
- El acento del sistema sólo se lee en Windows.
- Autostart por el portal `Background`: pide un ID de aplicación instalado
  que RUNE no tiene (hoy XDG autostart).
- Un abort de C (driver, GStreamer) puede cortar lo último del registro.

Probar en un escritorio real: la lista de
[PLATFORM.md](../docs/PLATFORM.md) (bandeja, instancia única, inicio con la
sesión, tema y fuente, notificaciones y su clic, sonidos, diálogos con
parent en Hyprland/GNOME/KDE, carpeta de sonidos, portal reiniciado, barra
de título, «Dibujar con», transparencia con y sin desenfoque, importar y
exportar temas, una fuente variable), atajos globales en el diálogo del
portal (KDE, GNOME 48+, Hyprland), una segunda ejecución con un enlace
`https://discord.com/channels/…`, Rich Presence de un programa real y
detección de un juego de Steam/Proton, actividad local con imágenes.

## Teclado y accesibilidad

Pendiente:

- Lector de pantalla real (AccessKit con Orca/NVDA) y escalado.
- Atajos editables: la edición de un mensaje sigue con Enter/Ctrl+Enter de
  limn, Ctrl+Enter siempre envía, los conflictos no tienen ámbitos, la nota
  «También fuera de RUNE» no distingue una tecla sola.

Probar en una ventana real: F6 y botones laterales del mouse, Ctrl+/ con
distribución española o alemana, teclas F13+ o puntuación, Pulsar para
hablar con un dígito, la vuelta al píxel de atrás/adelante.

## Idiomas

Pendiente:

- Avisos pasajeros ya mostrados, lo que la simulación arma al empezar y el
  rol de reserva «Rol» quedan en el idioma de entonces.
- Los nombres de reserva se reconocen por su texto: un servidor llamado
  «Servidor no disponible» se muestra traducido.

Probar: cambio de idioma en vivo, «Sistema» con `LANG=en_US`, la bandeja,
login y captcha en inglés contra Discord.

## Windows y macOS

Pendiente:

- macOS: sin empezar (registro a archivo, carpetas, servicios).
- Windows: el desinstalador no cierra un RUNE abierto; la CI de pull
  requests no arma el instalador; el instalador no instala el WebView2
  Runtime (Windows 10 lo trae por actualizaciones); el registro a archivo
  no se probó con un pánico que aborta.

Probar en un Windows real: la lista de
[PLATFORM.md](../docs/PLATFORM.md) (login y hCaptcha con WebView2,
toasts, bandeja, atajos, llavero, pantalla, cámara, sonido de la pantalla,
juegos, `discord-ipc`, instalador, actualización, barra de título y Aero
Snap, `AppUserModelID`, build MSVC con `+crt-static`, desinstalar sin
borrar los datos). Que `rune.log` no tenga la identidad ni la credencial, y
comparar user agent y client hints con las herramientas de Edge.

## limn

Pendiente:

- Subir `rune/visor-de-medios` y las ramas de integración; mezclar las PR
  abiertas (#76, #84, #86, #112 y las que dependen de ellas). La PR #84
  desde `main` no lleva `UiFaces::semibold` ni `ScaleDown` hasta que entren
  #76 y #66.
- Al mezclar: #84 (`Tooltip::title_family`) y #85
  (`Tooltip::title_font_family`) definen lo mismo; queda la de #85. Cuando
  haya versión publicada, pasar a crates.io.
- `measured_list`: `.estimate(|i| f32)` y `.row_revision(|i| u64)` para
  filas con imágenes o ediciones fuera de vista; una columna flexible que
  mida a sus hijos con su ancho (hoy RUNE pasa el ancho a mano para
  `shrink_to_fit`); idea: imágenes por referencia (`ImageRef`,
  `UiTree::set_image`).
- PR nuevas que hacen falta: subtítulo de `ListRow` como elemento (emoji en
  búsqueda), `_cursiva_`/`>>>`/encabezados en negrita, texto al final de
  `Tooltip::faces`, inactividad del puntero y resortes (visor).

- PR #113 (`DropdownMenuItem::leading`, rama `rune/menu-leading` sobre
  la de integración): el workspace apunta a esa rama hasta mezclarla.

Probar: el dueño confirma las opciones nuevas del tooltip del riel
(demora, título de 16 px en seminegrita, fondo).

## Paridad con Serein

Objetivo del dueño (2026-10-17): todo lo que hace Serein lo hace RUNE,
reimplementado. Lo que falta:

- Reacción con doble clic y lista de miembros en privados y grupos.
- Proxy HTTP para REST.
- Desplazamiento suave independiente del movimiento reducido.
- Temas: imágenes de fondo, tipografía y métricas por tema; desenfoque
  nativo de KDE.
- Lo de escritorio de arriba: iniciar minimizado, lista propia de juegos,
  ocultar en KWin.

Lo que RUNE no replica, a propósito: captcha automatizado o reenviado,
captura del token por JS, el servidor RPC por TCP en `127.0.0.1:6463–6472`
(RUNE no abre puertos), copiar la URL de un webhook (credencial), el token
de Spotify, subidas a servicios externos, conservar mensajes borrados por
otros, extensiones Wasm, recursos copiados.

## Fuera de alcance por decisión del dueño

- Corrector ortográfico (2026-10-18; Serein no lo tiene).
- Más idiomas que español e inglés.
- De los comandos de aplicación, lo que Serein no tiene (2026-10-18):
  opciones de adjunto, índice de apps del usuario, reintento del `202`,
  mensaje provisional, comandos de menú contextual, `ephemerality_reason`.
- Mandar súper reacciones y listar quién las hizo; subir la vista previa
  propia de una transmisión; la cámara propia en dos capas de simulcast.
