# Pendientes de RUNE

Sólo trabajo pendiente. Lo hecho sale de la lista (queda en git). Si algo
falla contra Discord, mandar las líneas `rune: …` del registro
([DISCORD.md](../docs/DISCORD.md#diagnóstico)).

## Orden del dueño (2026-10-18)

1. Terminar lo pendiente de esta lista.
2. **Optimización**: arranque desde caché con cuentas reales, costo del
   layout y del scroll, listas grandes, pedidos repetidos.
3. **macOS**: sin empezar.

## Rendimiento

- Medir memoria con imágenes y GIF reales y una cuenta con muchos
  servidores, latencia de entrada con teclado real (`--bench` usa rueda
  sintética) y la CPU de una llamada en la ventana.
- El filtro de GPU en una máquina sólo Intel y en una de una sola GPU
  discreta (cubierto por tests, no por hardware).

## Bloqueado aguas arriba

- `instant` (RUSTSEC-2024-0384) llega por `davey` → openmls `js`; zbus 5
  (RUSTSEC-2026-0370/0371/0372) por accesskit y `secret-service`. Ignorados
  con motivo en `deny.toml`; quitarlos cuando se destraben.
- `vendor/hpke-rs`: quitar el parche cuando openmls acepte `hpke-rs >= 0.7`.
- Arrastrar y soltar archivos y pegar una imagen sola con Ctrl+V: eframe
  sigue en winit 0.30 y con su portapapeles fijo. Se destraba si egui
  publica la inyección del portapapeles (commits locales en
  `~/projects/personal/egui-clipboard-hook`, rama
  `feat/clipboard-backend-factory`: rebasar y proponer a emilk/egui) o pasa
  a winit 0.31. `Client::prepare_attachment_paths` ya está listo.
- Pellizco en Linux y foco del SO al cerrar el login en Wayland: winit 0.30
  no los entrega.

## Repositorio y publicación

- `inout 0.2.3` está retirado de crates.io: `cargo update -p inout` la
  próxima vez que se toque `Cargo.lock`.
- Si se distribuyen con RUNE las bibliotecas nativas LGPL (GTK, WebKitGTK,
  GStreamer), sumar sus avisos.
- Las pruebas de video con plazos de 5 s fallaron alguna vez con la máquina
  cargada: si se repite en la CI, medir por cuadros.
- Código que sólo usan los tests y quedó a propósito (limpieza del
  2026-10-19): `screencast::camera_present` (su versión con bus la prueban
  los tests del portal), `StorageWorker::save` (siembra de los tests) y los
  `pub` de `rune-storage`, `rune-model` y otros que sólo nombran sus tests.
  Decidir si la cámara se usa o se borra junto con su test.
- Firma Authenticode del instalador de Windows: decide el dueño
  ([UPDATES.md](../docs/UPDATES.md#windows)).

## Cuentas, sesión y almacenamiento

- Un login interactivo nuevo manda un `Ready` provisional sin servidores:
  una lectura que esperaba la conexión justo ahí sale antes de tiempo
  (`rune-core/src/connecting.rs`).
- Los borradores en memoria no se vacían al cambiar de cuenta.
- No se guardan con el borrador: una imagen pegada sin archivo, una ruta no
  UTF-8 y los archivos de una publicación de foro a medio escribir.
- Caché de imágenes: una URL firmada renovada se vuelve a bajar; un origen
  en espera por 429 no sirve lo que ya está en disco.
- Identidad de navegador, lo que difiere del cliente web: API y Gateway v10
  sin `zlib-stream`, aceptación de invitación sin el `session_id`, idioma
  del sistema en vez del de la cuenta, user agent propio en las descargas
  del CDN, `client_heartbeat_session_id` por proceso; en Windows, el
  WebSocket sin client hints. Se cambian si Discord las distingue.
- Llavero: una escritura abandonada por el límite de 15 s al cerrar sesión
  puede completarse después.
- Leer el motor de WebView2 cuesta un arranque oculto en cada inicio del
  proveedor (~1 s, sin medir).
- OAuth2: validar el canje con una aplicación registrada.

## Invitaciones, reglas y onboarding

- El resultado de unas reglas en revisión vive en memoria: al cerrar RUNE el
  diálogo vuelve a ofrecerlas (confirmar por REA cómo lo lee el oficial).
- Editor del onboarding: reordenar preguntas y opciones, emoji del
  servidor; límites sin confirmar por REA.
- Invitaciones de aplicaciones o transmisiones (`target_type`): se
  rechazan.

## Mensajes y contenido

- Emoji como imagen en los extractos de búsqueda (falta en limn un
  subtítulo de `ListRow` que acepte un elemento).
- Markdown: `_cursiva_` con un guion bajo, citas `>>>` de varias líneas y
  encabezados en negrita (PR a limn).
- Un texto plano compartido con menciones y emoji resueltos (tarjeta de
  foro, fragmento de respuesta, notificaciones y extractos muestran tokens
  crudos).
- Reacciones con emoji propio de servidor; súper reacciones sin la paleta de
  contraste de `burst_colors`.
- Voto de encuesta: con la sesión sólo lo confirma el eco del Gateway;
  confirmar por el 204 y descartar el eco por `Tallies`.
- Una reacción o un voto en vivo hace fallar la carga de contexto en curso;
  reacciones y ediciones no tienen plazo propio en core.
- Contenido: `goreContentSettings` no se lee; no se tapan adjuntos con
  escaneo pendiente; «sin elegir» supone una cuenta adulta.
- «Unirse a la llamada» desde el registro de una llamada en curso.
- Copiar imagen o video baja el archivo antes de saber si hay portapapeles;
  macOS sin portapapeles.
- Fuentes: una secuencia (bandera, keycap, ZWJ) cuyo primer carácter está en
  una cara anterior no toma el respaldo nuevo.
- Buscar en un mensaje de voz Ogg decodifica desde el principio.
- Jumbo de emoji Unicode con escala mayor que 1,5: el arte es de 72 px.
- El tooltip de un tramo no lo lee un lector de pantalla; el borrador
  resalta cualquier `@palabra`.

## Hilos, foros y escenarios

- Tarjetas de publicaciones archivadas (`/channels/{foro}/post-data`); la
  tarjeta no se actualiza si cambia el primer mensaje.
- Las publicaciones nuevas piden 3 días de archivado en vez del valor del
  foro.
- Un hilo silenciado tampoco avisa de menciones (el oficial sí).
- Más de 100 archivados en un mismo instante; el menú «más» queda abierto
  sin verse si la ventana se agranda.

## Amigos y llamadas privadas

- «Rechazada» frente a «sin respuesta» es una inferencia por tiempo (20 s).
- El diálogo «no son amigos» usa textos propios.

## Voz y llamadas

- `aead_aes256_gcm_rtpsize` y el gateway de voz `?v=9` (hoy v8 y
  XChaCha20, como Serein).
- Jitter adaptativo (hoy retardo fijo); op 5 `VOICE_SERVER_PING` en cada
  corte; `flags`/`preferred_regions` en op 4 y `rtc_connection_id` en
  Select Protocol.
- Calibrar los umbrales de `VoiceActivity` y la compuerta con otros
  micrófonos y la voz real del dueño.
- Eco: el primer medio segundo tras dejar de estar silenciado; AEC3 de
  Sonora quita poco mientras la persona habla (su ajuste no está expuesto);
  con una salida que no es la predeterminada, lo que suena en la
  predeterminada no se cancela; en Windows el loopback no sigue un cambio de
  salida predeterminada hasta reabrir la captura.
- Remuestreador de banda limitada si un dispositivo sin 48 kHz suena mal.
- Un nodo `Audio/Duplex` genuino de PipeWire se oculta de los micrófonos.
- Menú de una persona: confirmar en el bundle qué ítems pinta de rojo el
  oficial además de `remove-friend`.
- Tarjeta de quien transmite: tamaño y contenido sin confirmar por REA;
  «Ver transmisión» fuera de la llamada no entra al canal.
- La simulación pone a la misma persona en voz en dos servidores
  (`voice_state_of` da el primero).

## Video y pantalla

- DMA-BUF de la captura a VA-API; NVDEC convierte a RGBA en la CPU.
- Sonido sólo de la ventana compartida (el portal no dice de qué proceso
  es; en Windows el loopback no distingue aplicaciones).
- 1080p60 por software no alcanza (35–40 fps); H.265 sin habilitar.
- Un WebM sin sonido y sin duración no se reproduce.
- Windows: codificador por hardware (MFT de Media Foundation), DXGI para
  Windows anterior a 1903, miniaturas en el selector, video en mensajes.
- Barra de la llamada: en una ventana angosta no se achica como la oficial;
  zoom de la transmisión enfocada; control de volumen al final de la barra.
- La vista previa de una transmisión en una llamada privada (`call:`) no se
  pide.

## Archivos, visor y documentos

- Visor: ocultar los controles quieto, rebote elástico, giro animado y
  damero detrás de GIF o video (faltan en limn).
- Portapapeles de archivos sólo en X11 o XWayland.
- «Guardar como» publica con la ruta, no con un descriptor de la carpeta.
- PDF: sin soporte en Windows ni macOS, ni sandbox fuera de Nix.
- La posición recordada de un audio o video no se ve antes de pulsarlo.

## Perfiles, presencia y riel

- Marcos de perfil (collectibles tipo 3), estilos de nombre y banners
  animados.
- Efectos de perfil: sin caché, capas animadas juntas (sin
  `start`/`loopDelay`).
- Editor de perfil: capacidad propia para imágenes animadas; la vista previa
  no anima el GIF.
- La tarjeta de voz sigue a su fila; sin la lista de miembros abierta no se
  suscribe a la presencia de su persona.
- Lista de miembros: presencia sólo de filas cargadas; hasta 50 000 filas.
- Riel: oyentes de un escenario en una fila aparte (falta texto al final de
  `Tooltip::faces`); sin desplazamiento al arrastrar cerca del borde; orden
  por llegada; un lanzamiento de app de un servidor no disponible queda
  hasta el próximo `READY`.

## Administración

- «Canales» no ofrece sincronizar permisos (`lock_permissions`).
- Emoji del servidor en rasgos y reacción del foro sin selector con
  imágenes.
- AutoMod: reordenar reglas, alerta de ataques de menciones, validar en el
  servicio, disparadores sin documentar.
- Eventos: portada, repetición, cancelar sin borrar, privacidad, enlaces
  `discord.com/events/…`, eventos del `GUILD_CREATE`.

## Escritorio

- «Iniciar minimizado» con la sesión en Wayland (habría que arrancar en la
  bandeja).
- Lista propia de juegos y caché en disco de la lista de Discord.
- GPU: dos del mismo fabricante no se eligen por separado; AMDVLK no entra
  en `*radeon*`.
- Las imágenes de perfil, grupo y expresiones abren su propio selector; el
  diálogo de `BindShortcuts` va sin parent.
- Temas: con transparencia el riel y las columnas toman el color de la
  conversación; `ColorPicker` opaco; sin desenfoque nativo de KDE.
- El acento del sistema sólo se lee en Windows.
- Autostart por el portal `Background` (pide un ID de aplicación
  instalado).
- Un abort de C (driver, GStreamer) puede cortar lo último del registro.
- Windows: el desinstalador no cierra un RUNE abierto; la CI de pull
  requests no arma el instalador.

## Teclado y accesibilidad

- Lector de pantalla real (AccessKit con Orca/NVDA) y escalado.
- Atajos editables: la edición de un mensaje sigue con Enter/Ctrl+Enter de
  limn; los conflictos no tienen ámbitos.

## Idiomas

- Avisos ya mostrados, lo que la simulación arma al empezar y el rol de
  reserva quedan en el idioma de entonces.
- Los nombres de reserva se reconocen por su texto.

## limn

- Mezclar las PR abiertas (#76, #84, #86 y las que dependen de ellas). La
  #84 desde `main` no lleva `UiFaces::semibold` ni `ScaleDown` hasta que
  entren #76 y #66; #84 y #85 definen lo mismo y queda la de #85. Con una
  versión publicada, pasar a crates.io.
- `measured_list`: `.estimate(|i| f32)` y `.row_revision(|i| u64)`; una
  columna flexible que mida a sus hijos con su ancho; imágenes por
  referencia (`ImageRef`).
- PR nuevas: subtítulo de `ListRow` como elemento, `_cursiva_`/`>>>`/
  encabezados, texto al final de `Tooltip::faces`, inactividad del puntero y
  resortes.

## Paridad con Serein

Objetivo del dueño (2026-10-17): todo lo que hace Serein lo hace RUNE,
reimplementado. Falta: reacción con doble clic, lista de miembros en
privados y grupos, proxy HTTP para REST, desplazamiento suave independiente
del movimiento reducido, temas con imágenes de fondo y tipografía propia,
iniciar minimizado, lista propia de juegos.

No se replica, a propósito: captcha automatizado o reenviado, captura del
token por JS, el servidor RPC por TCP, copiar la URL de un webhook, el
token de Spotify, subidas a servicios externos, conservar mensajes borrados
por otros, extensiones Wasm, recursos copiados.

## Fuera de alcance por decisión del dueño

- Corrector ortográfico (2026-10-18) y más idiomas que español e inglés.
- De los comandos de aplicación, lo que Serein no tiene (2026-10-18).
- Mandar súper reacciones y la cámara propia en dos capas de simulcast.
