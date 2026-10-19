# Video en llamadas: mirar y transmitir pantalla y cámara (diseño)

- **Estado (2026-10-17):** **fases 1 a 4 implementadas** (mirar la
  pantalla que comparte otra persona, con su sonido, las cámaras de los
  demás en la llamada, compartir la pantalla propia y enviar la cámara) y
  **la fase 5 casi entera** (RTX y NACK, pausa, varias transmisiones a la
  vez, vistas previas, sonido de las aplicaciones al compartir, volumen de
  cada transmisión, una capa de cada cámara en simulcast, el portal que
  recuerda la pantalla, pantalla completa y ventana aparte, CPU medida),
  **probadas sólo con fixtures locales: no probado contra el servicio
  real**
  ([DISCORD_SESSION.md](DISCORD_SESSION.md#mirar-la-pantalla-de-otra-persona-go-live-2026-10-16),
  [cámaras](DISCORD_SESSION.md#mirar-las-cámaras-de-la-llamada-2026-10-16),
  [compartir](DISCORD_SESSION.md#compartir-la-pantalla-propia-go-live-2026-10-16),
  [extras](DISCORD_SESSION.md#extras-de-video-fase-5-2026-10-17),
  [MEDIA.md](MEDIA.md#implementado-compartir-la-pantalla-propia-2026-10-16)).
  Lo que cada fase cambió de este plan está en
  [Ajustes de la fase 1](#ajustes-de-la-fase-1-2026-10-16),
  [de la fase 2](#ajustes-de-la-fase-2-2026-10-16),
  [de la fase 3](#ajustes-de-la-fase-3-2026-10-16),
  [de la fase 4](#ajustes-de-la-fase-4-2026-10-17) y
  [de la fase 5](#ajustes-de-la-fase-5-2026-10-17).
- **Protocolo:** lo confirmado por REA estática del cliente oficial, con
  evidencia y lista de lo que falta confirmar, está en
  [REA.md](REA.md#video-go-live-cámaras-y-transporte-de-video-2026-10-16).
  Este documento no repite offsets; cita qué fase usa cada hecho.
- **Alcance:** proveedor de sesión no oficial (Discord), Linux primero.
  Mock sigue simulando y lo dice. El proveedor OAuth2 no lleva video.

## Decisiones del dueño

- **2026-10-16, REA:** se puede usar lo encontrado en el cliente oficial
  (paquete instalado y bundles públicos) en el proveedor no oficial. Se
  reimplementa; no se copia código ni recursos. Sin falsificar la identidad
  del cliente, sin evadir controles y sin leer datos locales del cliente
  oficial ([SECURITY.md](SECURITY.md)).
- **2026-10-16, calidad:** **sin tope por Nitro.** RUNE ofrece 480p, 720p
  y 1080p a 15, 30 o 60 fps, como Serein, sin mirar la suscripción ni el
  nivel de mejoras del servidor. Es elección del dueño y queda registrada
  así: el cliente oficial reserva 720p60/480p60 al nivel 1 y 1080p al
  nivel 2 o a Nitro, y no está confirmado si el servidor lo hace cumplir.
  RUNE no ofrece 1440p ni «fuente». Si Discord rechaza o recorta, la
  interfaz lo dice con el motivo; no se reintenta a escondidas con menos.

## Principios

1. **Capas.** Los opcodes, claves de stream, SSRC, RTP, RTCP y DAVE viven
   sólo en `rune-discord-session`. Por encima, `rune-provider` habla de
   «stream de pantalla de tal persona en tal canal», «cámara de tal
   persona» y unidades de acceso H264 ya descifradas de extremo a extremo.
   `rune-state`, `rune-ui` y `rune-components` no saben de Discord.
2. **Permisos.** Lo que el proveedor puede se declara en `Capabilities`;
   la UI pregunta a `Store::abilities` (permiso `STREAM` del canal para
   transmitir, `CONNECT` y estar en la llamada para mirar).
3. **Hilos.** Red en el runtime del proveedor (el futuro de cada conexión
   de voz, como hoy). Decodificar y codificar en hilos propios de
   `rune-media`, uno por fuente. El hilo de UI sólo toma el último cuadro
   listo y lo presenta con `limn_egui::present_surface`, como la vista
   previa. Nada espera en el hilo de UI.
4. **Colas acotadas que descartan.** Igual que el audio: una cola llena
   tira lo viejo y, si era video, pide un cuadro clave.
5. **DAVE siempre.** Nada sale sin grupo listo; nada se muestra sin
   descifrar. Una caída a protocolo 0 corta, como en audio.
6. **No afirmar sin probar.** Cada fase cierra con fixtures, y la
   afirmación «funciona contra Discord» espera una prueba real del dueño
   registrada en `docs/DISCORD_SESSION.md`.

## Piezas comunes

### Contrato agnóstico (`rune-model`, `rune-provider`)

- `rune-model`: `VideoKind { Screen, Camera }` y
  `StreamPhase { Connecting, Live, Paused, Reconnecting, Ended(Option<String>) }`.
  `VoiceState` gana `#[serde(default)] camera: bool` (fase 2); `streaming`
  ya existe.
- `rune-provider::video` (nuevo, espejo de `voice`): `VideoPort` (lado del
  proveedor) y `VideoLink` (lado del host), tomado una vez con
  `Provider::take_video_link()` (`None` por defecto).
  - Entrante: `VideoUnit { user, kind, keyframe, timestamp, data }` con
    `data` en Annex B, ya descifrado. Cola de 8 unidades.
  - Del host al proveedor: `VideoRequest::KeyFrame { user, kind }` (lo que
    el proveedor traduce a PLI) y `Wants { user, kind, on }` (qué mosaicos
    están a la vista). Cola de 32.
  - Saliente (fases 3 y 4): `VideoUnit` propias con `kind`, cola de 2: si
    la red no drena, el codificador espera o tira y la próxima es clave.
  - Audio de un stream: `VoicePacket` gana `source: AudioSource { Voice, Stream }`
    para que el mezclador lo trate como fuente aparte (no se guarda en
    SQLite).
- `Capabilities`: `watch_streams` (fase 1), `watch_cameras` (fase 2) y el
  `video` existente pasa a significar «transmitir»; se parte en
  `share_screen` (fase 3) y `send_camera` (fase 4) cuando haga falta
  distinguirlos en la interfaz.
- `Command`: `WatchStream { channel, owner }`, `StopWatching { channel, owner }`
  (fase 1); `StartStream { quality }`, `StopStream`, `SetCamera(bool)`
  (fases 3 y 4). `Event`: `StreamPhase { channel, owner, phase }`,
  `StreamViewers { channel, owner, count }`.

### Calidades (`rune-media::video::Quality`)

| Preajuste | Tamaño | fps | Tasa máxima |
| --- | --- | --- | --- |
| 480p | 854×480 | 15 / 30 / 60 | 3,5 Mbit/s |
| 720p (por defecto 30) | 1280×720 | 15 / 30 / 60 | 3,5 Mbit/s a ≤ 30 fps; 9 a 60 |
| 1080p | 1920×1080 | 15 / 30 / 60 | 9 Mbit/s |
| Cámara | 1280×720 máx. | 30 máx. | 2,5 Mbit/s (piso 0,15) |

Las tasas son las del cliente oficial (Go Live 0,5–3,5 Mbit/s, 9 Mbit/s por
encima de 720p o 30 fps; cámara 0,15–2,5). El objetivo arranca bajo y sube
con REMB y pérdidas (fase 3). La fuente se escala con relación de aspecto y
bandas, nunca se agranda.

### Códec

Sólo **H264** (`packetization-mode=1`), `payload_type` 101 y RTX 102, como
Serein y como el primer códec de video del cliente oficial. VP8, AV1 y H265
quedan fuera: el cliente oficial sabe H264 en todas las plataformas y DAVE
lo cubre. RUNE codifica Constrained Baseline (sin cuadros B, con SPS sin
reordenamiento: ver riesgos) y decodifica lo que llegue, que en escritorio
es Main.

### Dependencia nueva: `openh264 =0.9.8`

Una sola dependencia nueva, en `rune-media`, para codificar y decodificar
H264 (`openh264` y `openh264-sys2` 0.9.8, BSD-2-Clause, MSRV 1.85; ya está
en el registro local y Serein la usa). Feature por defecto `source`: compila
OpenH264 de Cisco desde la fuente con `cc`; `nasm` acelera si está (se suma
a `shell.nix`) y sin él compila igual, más lento.

**Decisión del dueño (2026-10-16): feature `source`**, compilada desde la
fuente con la advertencia de abajo asumida.

**Licencia, dicho sin rodeos:** el código de OpenH264 es BSD-2, pero H264
está cubierto por patentes. Cisco paga la licencia de patentes de MPEG LA
**sólo para los binarios que Cisco compila y distribuye**. Si RUNE compila
OpenH264 desde la fuente (feature `source`, y también el paquete
`openh264` de nixpkgs, que se compila), **esa licencia de Cisco no se
aplica** a RUNE ni a quien lo use. Para un experimento personal el dueño
puede asumirlo; distribuir binarios de RUNE con H264 adentro exige revisar
las patentes en cada jurisdicción. La alternativa es la feature
`libloading` con el binario oficial de Cisco descargado por la persona
(verificado por hash): ahí sí rige la licencia de Cisco. La decisión queda
para el dueño antes de la fase 1; el diseño no cambia, sólo la feature.

Desde el 2026-10-17 el video también se codifica y decodifica con la GPU
(VA-API, NVENC/NVDEC) por GStreamer, y OpenH264 queda de respaldo: ver
[Video por hardware](#video-por-hardware-2026-10-17-rama-video-por-hardware).

### Cotas de memoria y latencia

| Pieza | Cota |
| --- | --- |
| Depaquetizador por SSRC | 1 unidad en armado + 1 completa; ≤ 2048 paquetes y ≤ 2 MiB por unidad; un hueco de secuencia o 500 ms sin marcador la descarta y pide clave |
| Orden al recibir (fase 5) | tras un hueco, ≤ 512 paquetes retenidos por SSRC y 200 ms de espera; NACK a lo sumo 3 veces por paquete, cada 60 ms, ≤ 64 números por NACK |
| Cola de decodificación | 4 unidades por fuente; una unidad esperando > 150 ms se tira y se pide clave |
| Cuadros decodificados | 2 por fuente (descarta el viejo), RGBA a ≤ 1920×1080 (8,3 MB cada uno) |
| Fuentes mirando a la vez | hasta 3 streams (`MAX_STREAMS`, fase 5), hasta 4 cámaras a la vista (fase 2), una capa por cámara; el resto con `wants` 0 |
| Pedido de clave (PLI) | a lo sumo uno por fuente cada 500 ms |
| `media sink wants` | al cambiar lo visible, y cada 5 s (cada 1 s si una fuente lleva 1 s sin imagen), como hace Serein |
| Codificador (fases 3–4) | 1 cuadro crudo pendiente; 1 unidad codificada pendiente; paquetes de ≤ 1200 bytes en lotes cada 2 ms dentro de un presupuesto de bytes |
| Historial RTX al enviar (fase 5) | ≤ 1 s, ≤ 2048 paquetes, ≤ 2 MiB; cada paquete a lo sumo 2 reenvíos, 50 ms entre ellos; ≤ 128 reenvíos esperando |
| Sonido de la pantalla (fase 5) | ≤ 32 aplicaciones, ≤ 200 ms en búfer cada una; cola de 8 cuadros de 20 ms hacia el proveedor |
| Vistas previas (fase 5) | ≤ 32 guardadas; ≤ 4 pedidas por vez, cada 60 s por transmisión; un 429 retiene el transporte su `retry_after` |
| Latencia | objetivo de pantalla a pantalla ≤ 300 ms en LAN; se privilegia descartar sobre acumular |

Un SPS que anuncie más de 3840×2160 o un cuadro que no entre en las cotas
se rechaza con un motivo visible, sin tumbar la llamada.

## Fases

### Fase 1: mirar streams de pantalla (Go Live)

Lo que usa del protocolo: ops 20, 19 y 21 del Gateway principal; los
despachos `STREAM_CREATE`, `STREAM_SERVER_UPDATE`, `STREAM_UPDATE` y
`STREAM_DELETE`; la conexión aparte con `server_id = rtc_server_id`; op 0
con `video: true` y `streams` `[{type: "screen", rid: "100", quality: 100}]`;
op 1 con Opus 120 y H264 101/102 (`encode: false`, `decode: true`); op 12 y
op 15; RTP H264 RFC 6184 y RTX; PLI; DAVE `MediaType::VIDEO`.

- `rune-discord-session::voice`:
  - `streams.rs` (nuevo): clave `guild:{g}:{c}:{dueño}` armada y validada
    (ids ≠ 0, sólo del canal de la llamada propia); un `watch` de «qué
    stream quiero» que el Gateway traduce a op 20/19, como `VoiceWant`;
    despachos `STREAM_*` validados (token ≤ 512, endpoint sólo
    `*.discord.media`, `rtc_server_id` numérico); `reason` →
    `StreamPhase::Ended` con texto en español; `unavailable` →
    `Reconnecting` y op 21; `user_requested` en silencio.
  - `connection.rs` generaliza `Credentials` con un `Role` (llamada,
    mirar stream, transmitir) y sigue siendo una sola máquina de estados.
    Acepta PT 101 y 102 además de 120; mapea SSRC de video a persona con
    op 12 y con `streams` de Ready.
  - `h264.rs` (nuevo): depaquetizador (unidad simple, STAP-A, FU-A) que
    arma Annex B con código de inicio de 4 bytes, restaura RTX (PT 102:
    los dos primeros bytes son la secuencia original), descarta lo
    incompleto.
  - `rtcp.rs` (nuevo): PLI sellado en `rtpsize` hacia el SSRC del emisor;
    descarta todo RTCP entrante que no autentique.
  - `dave.rs`: `decrypt_video(user, frame)` con `MediaType::VIDEO`; sólo de
    quien está anunciado, como el audio.
  - Audio del stream: Opus estéreo por la misma conexión, al `VoicePort`
    con `source: Stream`.
- `rune-media::video::decode` (nuevo): hilo `rune-video-decode` por fuente
  con OpenH264; convierte I420 a RGBA y escala a lo que el mosaico pide
  (≤ 1920×1080); reporta «falta clave» para que el host pida PLI.
- `rune-state`: streams del canal desde `VoiceState.streaming`; fase del
  stream mirado; `Abilities::watch_stream` (en la llamada, `CONNECT`,
  `Capabilities::watch_streams`).
- `rune-ui` / `rune-components`: en el escenario, el mosaico de quien
  transmite muestra «En vivo» y «Mirar»; el stream mirado ocupa el
  escenario con «Dejar de mirar» y su fase («Conectando…»,
  «Reconectando…», «Terminó: …»). Ids `stream:{canal}:{persona}`.
- `apps/desktop`: toma el `VideoLink`, sostiene el hilo de decodificación
  mientras se mira y presenta `video:stream:{persona}`.
- Cierre: fixtures (abajo) con un emisor sintético; soltar «Mirar», salir
  de la llamada, cambiar de cuenta o `STREAM_DELETE` liberan conexión,
  claves e hilo.

#### Ajustes de la fase 1 (2026-10-16)

Lo implementado se apartó del plan en esto, por lo que se aprendió al hacerlo:

- **Op 0 de quien mira: `streams: []`**, no `[{type: "screen"…}]`: es lo que
  Serein manda y le funciona; anunciar un stream que nunca se envía no hace
  falta para recibir. Sin confirmar qué prefiere el servidor.
- **Sin RTX al recibir.** RUNE no manda NACK, así que por el 102 sólo llega
  el relleno con que el emisor sondea el ancho de banda: se descarta. Sin
  búfer de reordenamiento, un paquete restaurado nunca llegaría a tiempo.
- **Op 15 `{any: 100}`** tras la Session Description y con cada stream
  nuevo de op 12, no por reloj: cuándo repetirlo no está confirmado.
- **PLI en `rtpsize`** con los primeros 8 bytes en claro y autenticados (la
  forma de Serein; los bundles no la muestran). Todo RTCP entrante se
  descarta sin abrirlo: la fase 1 no usa ninguno.
- **Contrato más chico.** Sin `VideoKind` ni `VideoRequest::Wants` ni
  `Event::StreamViewers` (fase 2 y 5); `VoicePacket` gana `stream: bool` en
  vez de un `AudioSource`; `VideoUnit` lleva `at` (cuándo se completó) para
  la edad de 150 ms; `Command::WatchStream { channel, owner }` y
  `StopWatching { channel, owner }`; `Failure::Stream`. La fase `Paused`
  llega por `STREAM_UPDATE`.
- **El stream se pide (op 20) recién cuando se conoce la sesión de voz** de
  esta conexión de Gateway; tras un `READY` nuevo así va después del op 4.
- **Cola del host llena:** el proveedor descarta la unidad, espera un
  cuadro clave y manda PLI, igual que con una pérdida.
- **Decodificar:** hasta 1920×1080 se convierte con la rutina vectorizada de
  OpenH264; más grande, se reduce al vecino más cercano; más de 3840×2160 no
  se muestra (sin mensaje propio: OpenH264 no pasa de nivel 5.2).
- **Interfaz:** ids `vz:watch:{persona}`, `vz:stop-watching` y la
  superficie `video:stream:{persona}` (como el resto de `vz:`). El stream
  ocupa el escenario solo; la gente sigue en la columna. Sin pantalla
  completa ni ventana aparte: limn no las tiene, quedan para un PR a limn.
- **`shell.nix`** suma `nasm` y `libstdc++` (OpenH264 es C++).

### Fase 2: mirar cámaras

- La conexión de la llamada pasa a identificarse con `video: true` y
  `streams: []`, y su op 1 ofrece H264 para decodificar.
- `VoiceState.camera` desde `self_video`; op 12 de cada persona da su SSRC.
- `Wants` del host → op 15 por SSRC: 100 para los mosaicos visibles, 0 para
  el resto; hasta 4 decodificadores vivos.
- `Capabilities::watch_cameras`; el mosaico de la persona muestra su cámara
  en lugar del avatar.

#### Ajustes de la fase 2 (2026-10-16)

- **Contrato.** `VideoKind { Screen, Camera }` en `rune-model`;
  `VideoUnit.kind`, `VideoRequest::KeyFrame { user, kind }` y
  `Wants { user, kind, on }`, y `MAX_CAMERAS = 4` en `rune-provider::video`.
  `Wants` de `Screen` se ignora: el stream mirado se elige con
  `Command::WatchStream`. `Abilities::watch_camera` (capacidad, `VIEW`,
  `CONNECT` y estar en esa llamada).
- **Pedidos del host.** Los toma `voice::Calls` (no una conexión) y los
  reparte: claves de pantalla a la conexión del stream; claves de cámara y
  qué cámaras se ven a la de la llamada, por un `watch` como el de
  silencio. Al entrar a una llamada se descarta lo pedido antes, y el host
  pide cámaras sólo con la llamada conectada: un pedido nunca cae en una
  llamada que se está reemplazando.
- **Op 15 de la llamada sin `any`:** sólo las SSRC de cámara conocidas, 100
  para las que se ven y 0 para el resto; nada mientras no se conoce
  ninguna. `any` podría hablar también de SSRC que no son cámaras. El
  cliente oficial arranca con `{any: 100}`; qué prefiere el servidor no está
  confirmado.
- **Repetir op 15 también en el stream:** cada 5 s, o cada segundo si una
  fuente pedida lleva un segundo sin traer una unidad completa (la conexión
  no ve los cuadros decodificados). Cambia el ajuste de la fase 1, que lo
  mandaba sólo al empezar y con cada stream nuevo.
- **Una cámara fuera de pantalla no se arma:** sus paquetes se descartan
  antes del depaquetizador, aunque el servidor siga mandándolos.
- **Host:** las unidades se reparten por fuente (persona y tipo) en colas de
  4; una cola llena se vacía, pide un cuadro clave y espera uno. La cuarta
  cámara es la última que se decodifica: el resto, en el orden del
  escenario, sigue con su avatar. Sin paginar el escenario.
- **Interfaz:** sólo con el escenario de la llamada a la vista y sin un
  stream que lo ocupe; la cámara ocupa el mosaico con bandas negras si no es
  16:9. Superficie `video:camera:{persona}`. En la simulación dos personas
  de «Pair programming» tienen la cámara prendida y dibujan cuadros
  inventados con «Cámara simulada» encima
  ([captura](img/camera-watch-2026-10-16.png), Xvfb privado de 1280×800).

### Fase 3: compartir pantalla

Usa op 18 (`type: "guild"`), el `STREAM_CREATE` propio, la conexión de
stream como emisor (Ready trae el SSRC de video; RTX = SSRC + 1), op 12 con
`max_bitrate`, `max_framerate` y `max_resolution`, op 5 con SOUNDSHARE si
hay audio, y PLI/REMB entrantes.

- `rune-media::video::encode` (nuevo): hilo `rune-video-encode` que toma
  cuadros del `Video` de pantalla existente (hoy limitado a 1280×720 y
  15 fps para la vista previa: la fuente gana un modo de captura con el
  tamaño y fps del preajuste), convierte a I420 y codifica Constrained
  Baseline; clave al pedirla (PLI), al empezar y tras cada descarte.
- `rune-discord-session::voice::h264` gana el empaquetador FU-A/STAP-A
  (≤ 1200 bytes) y el ritmo por lotes; `dave.rs` `encrypt_video` con
  `Codec::H264` antes de empaquetar.
- Control de tasa: una vez por segundo, entre 250 kbit/s y el máximo del
  preajuste, bajando 20 % con ≥ 5 % de pérdida y subiendo 5 % sin pérdida
  ni REMB menor; sin realimentación no sube.
- UI: el selector de calidad (480p/720p/1080p × 15/30/60, por defecto
  720p30) antes del diálogo del sistema; «Dejar de compartir» manda op 19.
  `Abilities::send_video` ya pide `CONNECT | STREAM`.
- `Capabilities::share_screen`. El aviso «La transmisión llega con las
  llamadas» sale para este proveedor.

#### Ajustes de la fase 3 (2026-10-16)

- **Contrato.** La calidad es `rune_model::StreamQuality { height, fps }`
  (no `rune-media::video::Quality`: el proveedor la recibe en
  `Command::StartStream { channel, quality }`), con su tamaño y su tasa
  máxima. 480p queda en 3,5 Mbit/s también a 60 fps, como dice la tabla de
  arriba (la regla del cliente oficial le daría 9). `Command::StopStream
  { channel }`, `Failure::Share`; la fase del stream propio llega por
  `Event::StreamPhase` con la cuenta como dueña y vive en
  `Store::sharing`. `VideoLink` suma `outgoing` (cola de 2) y `feedback`
  (`Feedback::KeyFrame`, `Feedback::Bitrate`); `START_BITRATE` = 600 kbit/s
  (quitado el 2026-10-17: la tasa empieza en el máximo de la calidad, ver
  [PERFORMANCE.md](PERFORMANCE.md#fluidez-de-la-pantalla-compartida-2026-10-17-rama-audio)).
- **Capacidades.** `Capabilities::share_screen` y `Abilities::share_screen`
  (`CONNECT | STREAM`, sin mirar el canal AFK); `video` y
  `Abilities::send_video` quedan para la cámara (fase 4). El aviso «La
  transmisión llega con las llamadas» ya no sale para la pantalla con este
  proveedor ni con la simulación; sigue para la cámara.
- **Op 0 y op 12 con `type: "screen"`**, como el cliente oficial en la
  conexión de stream (REA); Serein manda `"video"`. Op 12 sale tras la
  Session Description (el cliente oficial lo manda en cuanto tiene un SSRC
  de video; Serein espera a DAVE). Quien comparte no manda op 15. Sin op 5
  SOUNDSHARE: sin audio del sistema hasta la fase 5; queda el op 5 con
  `speaking: 0` que anuncia el SSRC, como toda conexión.
- **Codificador.** Sin cuadros clave periódicos: al empezar, con PLI/FIR,
  tras cualquier descarte y cuando el grupo DAVE se forma. La pantalla
  quieta se codifica de nuevo cada segundo para quien llega. La tasa nueva
  rehace el codificador sólo con −15 % / +25 % acumulado (API segura de
  OpenH264, como Serein), sin `unsafe`.
- **Contrapresión en vez de descarte.** El codificador no codifica sin
  lugar en la cola del proveedor y la conexión toma una unidad sólo cuando
  la anterior salió: con red lenta bajan los fps (la interfaz lo muestra) en
  lugar de tirar unidades y pedir claves en cadena.
- **Empaquetado.** STAP-A junta NAL chicos (SPS, PPS, SEI); carga ≤ 1168
  bytes para que el datagrama sellado no pase de 1200; sin extensiones RTP
  (Serein tampoco). REMB, fracción perdida de SR/RR, PLI y FIR se leen;
  NACK no (sin RTX al enviar, fase 5).
- **SPS.** Se normaliza como lo haría el reescritor de libwebrtc (valores
  por defecto 2, 1, 16, 16 si falta la restricción; sólo los dos que importan
  si ya está). OpenH264 ya escribe una VUI con
  `max_num_reorder_frames = 0`, así que con él no cambia nada; la
  normalización cubre cualquier otro codificador. Que el receptor oficial no
  toque esa VUI sigue sin confirmar.
- **Interfaz.** El selector es un menú sobre el botón de pantalla de la
  barra del escenario (el del panel abre el escenario con el menú). El
  propio mosaico no muestra «EN VIVO»: lo dice la vista previa con los fps y
  kbit/s que salen. Ids `vz:quality:{alto}`, `vz:fps:{fps}`, `vz:share`,
  `vz:share:dismiss` ([captura](img/screen-share-2026-10-16.png)).
- **Simulación.** Marca a la cuenta transmitiendo y pasa a «En vivo» después
  de la conexión simulada; la vista previa dice «simulada» y nada se
  codifica.

### Fase 4: enviar la cámara

- Op 4 con `self_video: true`; la conexión de la llamada se identifica con
  `streams: [{type: "video", rid: "100", quality: 100}]`, toma el SSRC de
  Ready y lo anuncia con op 12.
- Misma ruta de codificación que la fase 3 con el preajuste de cámara
  (≤ 1280×720, ≤ 30 fps, ≤ 2,5 Mbit/s).
- `Capabilities::send_camera`; apagar la cámara manda op 4 con
  `self_video: false` y op 12 con `video_ssrc: 0`.

#### Ajustes de la fase 4 (2026-10-17)

- **Contrato.** Sin `send_camera`: la capacidad es la `Capabilities::video`
  que ya existía (y `Abilities::send_video`), ahora con su comando
  `Command::SetCamera { channel, on }` y `Failure::Camera`. `VideoLink` suma
  colas propias para la cámara (`camera_outgoing`, `camera_feedback`), así
  la pantalla y la cámara pueden salir a la vez; el preajuste es
  `video::CAMERA` (720p30) entre `CAMERA_MIN_BITRATE` y `CAMERA_MAX_BITRATE`.
- **Conexión de la llamada.** Con video, siempre anuncia una cámara en op 0
  (`type: "video"`, como el cliente oficial), así Ready le da un SSRC y
  encenderla a mitad de llamada no pide otra conexión. Op 12 la describe al
  encenderse y dice `video_ssrc: 0` al apagarse (no mientras reanuda).
  Si Ready no le da lugar o el servidor elige otro códec, la llamada sigue
  sin cámara (la pantalla, en cambio, termina). Op 1 ofrece H264 para
  codificar y decodificar.
- **Codificador.** El mismo de la fase 3 con `UsageType::CameraVideoRealTime`
  y sus propias cotas de tasa.
- **Interfaz.** La cámara sólo se enciende dentro de la llamada; la vista
  previa dice «En la llamada · fps · kbit/s», y «simulada» en la simulación,
  que sólo cambia el estado de voz.

### Fase 5: extras

- Vistas previas: `GET /streams/{clave}/preview` en la lista del canal
  (respetando `retry_after`), y subir la propia (JPEG ≤ 512×288 con el
  `image` que ya está en `rune-core`, sumándole la feature).
- Audio del sistema al compartir (PipeWire, excluyendo a RUNE) y volumen
  propio del stream mirado.
- Pausa (op 22), varios streams a la vez, streams en llamadas privadas
  (`call:{canal}:{dueño}`; hechos el 2026-10-17, ver abajo).
- RTX al enviar (historial acotado) y NACK al recibir.
- Simulcast de cámara (`rid: "50"`), restaurar la elección del portal
  (`restore_token`), DMA-BUF, Windows y macOS.

#### Ajustes de la fase 5 (2026-10-17)

Hecho, en el orden de prioridad del recorrido; todo **sólo con fixtures**:

- **RTX al enviar** (`voice::nack::History`, `voice::send::Pacer`): lo
  enviado se guarda a lo sumo 1 s, 2048 paquetes y 2 MiB; un NACK genérico
  (RTPFB FMT 1) sobre la SSRC propia reenvía cada paquete guardado por la
  SSRC de RTX ya negociada (payload 102, secuencia propia, la secuencia
  original primero, el mismo cifrado DAVE con un sellado de transporte
  nuevo), adelante de la unidad en camino y dentro del mismo presupuesto
  del ritmo; a lo sumo dos veces por paquete y no más de una cada 50 ms;
  lo que ya no se guarda pide un cuadro clave. Serein lo hace igual.
- **NACK al recibir** (`voice::nack::Reorder`): un hueco retiene lo que
  sigue (≤ 512 paquetes), pide lo que falta con un NACK sellado como el PLI
  (hasta 3 veces, cada 60 ms, ≤ 64 números) y suelta en orden cuando la
  retransmisión llega por la SSRC de RTX de esa persona (de op 12, o la
  siguiente); a los 200 ms se da por perdido y se pide clave como antes.
  **Serein no manda NACK**; sale de la REA (el cliente oficial lo manda).
  El relleno de sondeo por RTX se descarta. La fase 1 decía «sin RTX al
  recibir»: ya no.
- **Pausa (op 22)**: `Command::PauseStream`; el Gateway manda `{stream_key,
  paused}` cuando cambia; la conexión deja de tomar unidades (el
  codificador del host saltea cuadros sin codificarlos) y no manda el
  sonido; al reanudar, empieza por un cuadro clave. «Pausar»/«Reanudar» en
  la vista previa propia ([captura](img/stream-pause-2026-10-17.png)). Un
  stream mirado en pausa ya se mostraba desde la fase 1. **Serein no usa
  op 22**; su forma sale de la REA.
- **Varias transmisiones a la vez**: hasta `video::MAX_STREAMS` = 3, cada
  una con su conexión y su decodificador; una más suelta la más vieja. El
  Gateway manda op 20 por cada una sin op 19 de las otras (el «varios a la
  vez» del cliente oficial). En el escenario van en grilla con los mosaicos
  de quienes transmiten y no se miran, para sumarlos
  ([captura](img/streams-multi-2026-10-17.png)). `Store::watching` es una
  lista; `«Dejar de ver»` es `vz:stop-watching:{persona}`.
- **Vistas previas**: `GET /streams/{clave}/preview` como Serein, en la cola
  REST, sólo de canales de voz del servidor; sólo se acepta un `{url}` del
  CDN bajo esa clave con nombre hexadecimal. Un 429 retiene el transporte
  (todas las rutas, como ya hacía el proveedor) su `retry_after`; eso, un
  404 o una respuesta rara se contestan sin imagen y sin aviso. La interfaz
  la pide para quienes transmiten en el servidor abierto y no se miran
  (≤ 4 por vez, cada 60 s) y la muestra de 160×90 bajo su fila; un clic la
  mira ([captura](img/stream-preview-2026-10-17.png), simulada). **Subir la
  propia** (`POST /streams/{clave}/preview`, JPEG ≤ 512×288) **no se hizo**:
  Serein no la usa y la regla del dueño la deja afuera hasta que la
  autorice; tampoco hay capacidad para eso.
- **Sonido de las aplicaciones al compartir** (`rune-media::video::sound`):
  PipeWire, un flujo de captura por cada `Stream/Output/Audio` salvo los de
  RUNE (`application.process.id`; esa exclusión no funcionaba y desde el
  2026-10-17 también quedan afuera las llamadas: ver
  [Sonido de la pantalla](#sonido-de-la-pantalla-sin-las-voces-de-la-llamada-2026-10-17-rama-video-por-hardware)),
  a lo sumo 32, con `node.dont-fallback` y
  `node.dont-reconnect` para que nunca caiga en un micrófono; suma cada
  20 ms, Opus estéreo para música a 128 kbit/s; el silencio no se manda.
  Sale por la SSRC de audio de la conexión que comparte, con op 5
  SOUNDSHARE (2), cifrado con DAVE; silenciar el micrófono no lo toca y en
  pausa no sale. «Con sonido» en el selector (prendido por defecto); si no
  se puede leer, se comparte sin sonido y se dice por qué. Serein lo hace
  con PulseAudio; RUNE con la API de PipeWire que ya enlaza. **La lectura
  de PipeWire no se probó en vivo** (no se capturó el audio de esta
  máquina). Desde el 2026-10-18 (`voz-protocolo`) ese audio lleva la
  extensión RTP con que Serein lo marca (ID 9, valor 4), sin verificar.
- **Volumen de cada transmisión**: el sonido de un stream mirado se mezcla
  con su propia clave (`mixer::STREAM_SOUND`) y tiene volumen y silencio en
  el menú de su dueño, aparte de su voz, guardados como los demás.
- **Simulcast**: al recibir, si alguien manda su cámara en capas (rid
  "100" y "50", con `quality` en op 12) se pide una sola, la mejor con hasta
  dos cámaras a la vista y la más liviana pasado eso, y la otra no se arma
  (antes llegaban las dos al mismo decodificador). **Mandar la cámara
  propia en dos capas no se hizo**: Serein no lo hace, el cliente oficial
  lo prende sólo con un experimento, y anunciar una capa en op 0 obliga a
  mandarla siempre (quien la pida vería negro): hace falta un segundo
  codificador a 640×360 con un cuarto de la tasa y un `Sender` por capa.
  Queda para cuando haya una prueba real.
- **El portal recuerda la pantalla** (`restore_token`): con un portal de
  versión ≥ 4, `persist_mode` 2 y el token anterior; el nuevo se guarda con
  las preferencias (`screen_restore`), no es una credencial. «Elegir otra
  pantalla» en el selector lo olvida. Probado con el portal falso de las
  pruebas, no con el del escritorio.
- **Pantalla completa y ventana aparte**: «Pantalla completa» deja la
  transmisión sola en la ventana y la pone en pantalla completa (Escape o
  «Salir de pantalla completa» vuelve,
  [captura](img/stream-fullscreen-2026-10-17.png)); «Ventana aparte» la
  muestra además en un viewport de eframe con su propio árbol de limn y su
  superficie (`video:window:{persona}`). **No hizo falta un PR a limn**:
  limn-egui ya pinta un `Frame` en cualquier `egui::Ui`, y el viewport y la
  pantalla completa son de la ventana, que es de `apps/desktop`. La
  ventana aparte no se pudo capturar en Xvfb (la captura guarda la ventana
  principal).
- **CPU a 1080p60** medida con `cargo run --release -p rune-media --example
  encode_bench`: no alcanza por software en esta máquina, 35 a 40 cuadros
  por segundo con un núcleo saturado; 1080p30 y 720p60 usan la mitad de uno
  ([PERFORMANCE.md](PERFORMANCE.md#codificar-la-pantalla-compartida-2026-10-17)).

Fuera de esta fase, como se pidió: DMA-BUF, Windows y macOS. Streams en
llamadas privadas (`call:{canal}:{dueño}`): desde el 2026-10-17, con las
llamadas privadas: op 18 de tipo `call` con `guild_id` nulo y las claves
`call:` para mirar, pedir de nuevo, pausar y soltar (REA del oficial y
Serein; [DISCORD_SESSION.md](DISCORD_SESSION.md#llamadas-privadas-2026-10-17-rama-llamadas-privadas)).
Sin vista previa en la columna: no hay columna de voz en un privado y la
ruta es de servidores. Sólo fixtures.

## Video por hardware (2026-10-17, rama `video-por-hardware`)

Todo **sólo con fixtures y con la GPU de esta máquina** (Radeon 780M con
Mesa 26.1 y RTX 5060 Laptop con el driver 595.84, GStreamer 1.26.11);
**sin probar contra Discord**.

### La vía: GStreamer, como Serein

Serein codifica con GStreamer (`vah264enc`, `vaapih264enc`, `nvh264enc`,
en ese orden, y OpenH264 al final) y decodifica con `decodebin`. Las otras
vías eran libva directo (habría que escribir a mano los encabezados de
H264 que VA-API pide y sólo cubre VA-API, no NVENC) y FFmpeg
(`ffmpeg-next` ata la versión de las bibliotecas y suma bindgen sobre
FFmpeg). El cliente oficial en Linux usa libva directo con `cros-codecs` y
`cros-libva` (los de ChromeOS, en un proceso `gpu_encoder_helper`, con
DMA-BUF a superficie VA; REA estática, [REA.md](REA.md#video-por-hardware-y-sonido-de-la-transmisión-en-linux-2026-10-17)):
cubre VA-API (y Vulkan Video), no NVENC, y sus crates son jóvenes.
GStreamer ya estaba en el entorno Nix para el login, nixpkgs trae sus
elementos de VA-API y NVIDIA, y un mismo pipeline sirve para codificar y
decodificar en las dos GPU. Era la opción que [MEDIA.md](MEDIA.md)
reservaba para esto.

- **Dependencias** (`rune-media`, sólo Linux): `gstreamer 0.25.4`,
  `gstreamer-app 0.25.2`, `gstreamer-video 0.25.4`, bindings MIT/Apache-2.0
  (glib 0.22 ya estaba por `rune-login`). GStreamer es LGPL-2.1+ y se
  enlaza dinámicamente: `libgstreamer`, `libgstapp` y `libgstvideo` pasan a
  ser bibliotecas que RUNE necesita para arrancar en Linux, como PipeWire;
  sin los plugins de la GPU sólo se pierde la GPU. `kstring` quedó fijado
  en 2.0.2: 2.0.5 pide Rust 1.96.
- **`shell.nix`** suma `gstreamer` y `gst-plugins-base` (se enlazan, y van
  al `RUSTFLAGS` rpath) y `gst-plugins-bad` (VA-API, nvcodec y `h264parse`,
  que se cargan en ejecución). Compila sus rutas en `RUNE_GST_PLUGINS`, así
  un binario que corre fuera del shell igual las encuentra.
- **Licencia de H264.** Lo de arriba sigue valiendo para OpenH264, que es
  el respaldo. Con la GPU, el códec lo corre el hardware del fabricante y
  el driver (Mesa, NVIDIA); algunas distribuciones (Fedora) apagan H264 en
  el VA-API de Mesa por patentes y nixpkgs lo trae prendido. Distribuir
  binarios sigue pidiendo revisar patentes; esto no es una opinión legal
  **(no verificado)**.

### Codificar (`video::encode`, `video::gpu`)

Mismo hilo, mismo contrato; cambia quién codifica. `Settings::engines`
dice qué motores de GPU probar (`HARDWARE`: VA-API y después NVENC, o
ninguno) y OpenH264 va después de todos:

- **VA-API**: `appsrc` (RGBA) → `vapostproc` (a NV12 en la GPU) →
  `vah264enc` en VBR con la tasa como techo (`target-percentage=100`: en
  CBR rellena una pantalla quieta hasta la tasa entera, 7 Mbit/s medidos
  para una imagen fija), sin cuadros B, `key-int-max=1024` (el GOP más
  largo que acepta: una clave sola cada 17 s a 60 fps). Se usan los
  elementos de cada GPU que no sea de NVIDIA (`va…` o `varenderD129…`),
  así se elige la integrada aunque `nvidia-vaapi-driver` esté instalado.
- **NVENC**: `appsrc` (RGBA, NVENC convierte en la GPU) → `nvh264enc` en
  CBR (no rellena), `gop-size=-1`, sin cuadros B ni lookahead, latencia
  cero.
- Las dos salidas pasan por `h264parse config-interval=-1` con
  `profile=constrained-baseline` fijo: SPS y PPS antes de cada clave, una
  unidad de acceso por buffer, en Annex B. Probado: perfil 66 con
  `constraint_set1` en las dos.
- **Clave a pedido**: `GstForceKeyUnit` hacia el codificador (a lo sumo una
  cada 500 ms mientras se espera la que se pidió). Una unidad que el
  proveedor no toma hace que lo que la GPU ya codificó después se descarte
  hasta la clave nueva (contado como rechazado).
- **Tasa nueva**: NVENC la toma en marcha, sin clave. El elemento de VA-API
  rehace la secuencia (y manda una clave) con cada tasa, así que espera a
  que el cambio sume −15 %/+25 %, como OpenH264 cuando se niega, y cuenta
  como reinicio.
- **Respaldo**: si ninguno arranca, OpenH264. Si uno falla con la
  transmisión andando (un error en el bus de GStreamer, o una imagen que no
  vuelve en 1 s), pasa al siguiente con un cuadro clave, sin cortar.
  Ejemplo probado: NVENC no toma menos de 160×64 y cae a OpenH264.
- A lo sumo dos imágenes esperan en la GPU; el resto espera en la captura,
  como con OpenH264.
- **Ajuste**: Ajustes → Voz y video → Video → «Aceleración por hardware de
  H.264» (prendido; id `us:video-hardware`, clave `video_hardware`).
  Apagado, OpenH264 codifica y decodifica. Se aplica en el momento, también
  a lo que ya se manda o se mira (2026-10-18)
  ([captura](img/video-por-hardware-2026-10-17.png)).
- **Interfaz**: la vista previa dice quién codifica: «En vivo · 60 fps ·
  9000 kbit/s · VA-API» (o NVENC, u OpenH264), y lo mismo la de la cámara.
- **Diagnóstico**: la línea `rune: voice share: video …` empieza con
  `encoder VA-API`, `encoder OpenH264 after VA-API failed`,
  `encoder OpenH264 (no GPU one started)` o `encoder OpenH264 (GPU off)`.

### Decodificar (`video::decode`, `video::gpu`)

Lo mismo para las transmisiones y cámaras que se miran: `vah264dec` (la
GPU integrada) → `vapostproc` a RGBA, o `nvh264dec` → `videoconvert` a RGBA
(en la CPU: los conversores de CUDA piden NVRTC, que no está), OpenH264 de
respaldo. Las dos con `compliance=flexible` (nadie reordena en una
llamada) y `discard-corrupted-frames` (una imagen sin sus referencias no
se muestra, como con OpenH264). Una GPU que falla pasa a la siguiente y se
espera una clave. El log dice una vez por transmisión
`rune: voice watch: video decoder VA-API` (o el respaldo y por qué).

Un hallazgo: los decodificadores de GStreamer 1.26 sólo sacan una imagen
apenas la decodifican si su búfer de imágenes tiene un lugar libre
(`gst_h264_dpb_needs_bump` en modo de baja latencia); OpenH264 y el
reescritor de libwebrtc declaran `max_dec_frame_buffering` igual a las
referencias, y entonces cada imagen esperaba a la siguiente: una pantalla
quieta mostraba su último cambio recién cuando el emisor la repetía (hasta
1 s después). `video::sps::roomy` le suma un lugar a ese campo del SPS
antes de dárselo a la GPU; no cambia nada más (nadie reordena).

### Arreglo en el lector de SPS del proveedor

`rune-discord-session::voice::sps` leía un bit de más en
`video_signal_type` (5 en vez de 4 antes de `colour_description_present`).
Con el SPS de OpenH264 no se notaba (no lo trae); con el de NVENC sí, y
`normalize` habría escrito la restricción en el lugar equivocado del SPS que
se manda a Discord. Arreglado, con el SPS real de NVENC como prueba.

### La captura y DMA-BUF

La captura sigue pidiendo memoria compartida (`memfd`): el formato que
ofrece a PipeWire no lleva modificadores DRM, así que el compositor copia
la pantalla a la CPU; RUNE la escala a RGBA en `rune-video-pw` y la GPU la
vuelve a subir (`vapostproc`, o NVENC). No se pasó a DMA-BUF porque:

- la vista previa y el respaldo a OpenH264 necesitan los píxeles en la CPU
  igual;
- negociar modificadores e importar el DMA-BUF en VA-API (`vapostproc`
  acepta `memory:DMABuf`) sirve sólo si el compositor y el codificador
  están en la misma GPU; en esta máquina la pantalla puede estar en la
  Radeon y NVENC en la NVIDIA, y ahí la copia vuelve;
- medido, el camino de la CPU no es el cuello de botella: escalar la
  pantalla del portátil a 1080p cuesta 2,65 ms por cuadro (16 % de núcleo
  a 60 cuadros/s) y VA-API codifica 1080p60 con 30 % entre todos los hilos
  ([PERFORMANCE.md](PERFORMANCE.md#video-por-hardware-2026-10-17-rama-video-por-hardware)).

Queda para después: ofrecer modificadores cuando el codificador es VA-API
en la misma GPU y mandar el DMA-BUF a `vapostproc` sin pasar por la CPU,
con la vista previa bajada aparte.

### Lo que queda

- Probarlo contra Discord: que los clientes oficiales vean bien lo que sale
  de VA-API y de NVENC (perfil, SPS con su VUI normalizada, claves) y que
  RUNE vea las transmisiones del cliente oficial por la GPU.
- El registro de GStreamer es de toda la cuenta: si otro programa escaneó
  `va` o `nvcodec` sin llegar a la GPU (una web view aislada), quedaban sin
  elementos; RUNE los vuelve a cargar al arrancar. Si igual no aparece
  NVENC, borrar `~/.cache/gstreamer-1.0/registry.*.bin`.
- Cambiar el ajuste se aplica en el momento (2026-10-18): el host rehace
  los codificadores de lo que se manda y los decodificadores de lo que se
  mira, desde una imagen clave, y conserva la fuente y la última imagen.
  Sin verificar con una GPU real: el test cambia el decodificador de lo que
  se mira con `Engine::Software`; rehacer el codificador de lo que se manda
  no tiene test (pide una fuente de PipeWire real) ni se probó con VA-API o
  NVENC.
- NVDEC decodifica en la GPU, pero su conversión a RGBA (`videoconvert`)
  queda en la CPU y cuesta lo mismo que OpenH264 a 1080p; sirve de respaldo
  si VA-API no está, no para ahorrar CPU. Pasarla a la GPU pide NVRTC o GL
  compartido con CUDA.
- DMA-BUF (arriba). AV1 y H265 siguen afuera.

## Sonido de la pantalla: sin las voces de la llamada (2026-10-17, rama `video-por-hardware`)

**Síntoma** (prueba real del dueño): mientras transmitía, sus amigos se
escuchaban a sí mismos dentro del sonido de la transmisión.

**Causa encontrada.** `video::sound` dejaba afuera a RUNE comparando
`application.process.id` en las propiedades de cada nodo que da el
registro de PipeWire, pero ese registro no las trae: para un nodo da
`client.id`, `application.name`, `node.name`, `media.class` y poco más
(comprobado con `pw-cli list-objects` en esta máquina); el pid está en la
información del **cliente**, que hay que pedir atándolo. La comparación
nunca coincidía y RUNE se capturaba a sí mismo: las voces de la llamada que
reproduce volvían a la llamada. Un cliente oficial abierto en la misma
llamada con otra cuenta (o un programa que monitorea el micrófono) sumaba
lo mismo por su lado.

**Qué hace el cliente oficial en Linux** (REA estática, sin cargarlo; ver
[REA.md](REA.md#video-por-hardware-y-sonido-de-la-transmisión-en-linux-2026-10-17)):
el sonido de una transmisión (`soundshare`) se toma de **un proceso**: el
de la fuente elegida (`soundshareId` = `getAudioPid(pid)` en el cliente
web). El módulo nativo (`PulseAudioController`) sigue cada sink input de
PulseAudio, averigua su pid por `application.process.id` o
`pipewire.sec.pid` del cliente, mira el árbol de procesos en
`/proc/<pid>/stat` y graba sólo los del pid de destino con
`pa_stream_set_monitor_stream`. Todo lo demás, el propio Discord incluido,
queda afuera. Serein graba cada aplicación salvo Serein y las de
identidad desconocida. Vesktop usa venmic, que según su documentación
pública excluye a Vesktop y deja excluir por propiedades del nodo (no
revisado acá).

**Qué hace RUNE ahora** (`rune-media::video::sound`):

- Ata cada cliente de PipeWire y lee quién es: `application.process.id`
  (o `pipewire.sec.pid`), `application.process.binary` y
  `application.name`. Un stream espera sin oírse hasta saberlo.
- Deja afuera: lo de RUNE (su pid), los streams con rol de llamada
  (`media.role` `Communication` o `Phone`) y los de otros clientes de voz
  por su nombre, binario o `node.name` (`VOICE_CLIENTS`: Discord y sus
  variantes, el `WEBRTC VoiceEngine` con que suena la voz del oficial,
  Vesktop, Vencord, WebCord, ArmCord, Legcord, Dorion, Serein, Zoom,
  Teams, Skype, Mumble, TeamSpeak, Jitsi, Webex).
- Cada 5 s, con la línea de video: `rune: voice share: sound: mixing
  Firefox (Movie); left out rune (Stream/Output/Audio, own), Discord
  (Stream/Output/Audio, voice client)`. Nombre de la aplicación y rol (o
  clase) solamente: nunca títulos, rutas ni ids.
- **Sólo la aplicación de la ventana compartida**, como el oficial: no se
  puede. El portal ScreenCast no dice de qué proceso es una ventana (sus
  streams traen id, posición, tamaño y tipo de fuente); el oficial lo
  sabe porque elige la ventana con su propio selector. Queda pendiente si
  un portal lo expone.

Tests con propiedades de nodos y clientes simuladas. **Sin probar en
vivo**: no se capturó el sonido de esta máquina.

## Pruebas

Las mismas reglas que el audio: todo con fixtures locales, deterministas,
sin red real.

- **Unidades** (`rune-discord-session`): clave de stream (armado, rechazo
  de prefijos e ids inválidos); despachos `STREAM_*` válidos e inválidos;
  depaquetizador con orden, pérdida, duplicados, STAP-A, FU-A, RTX,
  unidades sobre la cota; PLI sellado y RTCP falsificado descartado;
  `rtpsize` con extensiones en paquetes de video.
- **DAVE de video**: un grupo con el emisor MLS de los fixtures cifra una
  unidad H264 sintética con `Codec::H264` y RUNE la descifra; un byte
  cambiado en el SPS en claro hace fallar la autenticación.
- **Fixture de servidor de voz ampliado** (`voice::tests`): Ready con
  `streams`, Session Description con `video_codec: "H264"`, op 12 de un
  participante sintético que manda una unidad codificada en la prueba con
  OpenH264 (sin archivos binarios en el repo), empaquetada, cifrada en las
  dos capas; se comprueba que `VideoLink` recibe la unidad y que el
  decodificador da un cuadro del tamaño y color esperados.
- **Gateway principal**: op 20 → `STREAM_CREATE` + `STREAM_SERVER_UPDATE` →
  segunda conexión con `server_id = rtc_server_id`; `STREAM_DELETE` con cada
  motivo; op 21 tras `unavailable`; cambiar de cuenta cierra todo.
- **Medios**: cotas de colas, descarte por espera > 150 ms, pedido de clave
  acotado, liberar al soltar; codificador (fase 3) que produce clave al
  pedirla y respeta la tasa.
- **Prueba real del dueño** por fase, con un cliente oficial de confianza,
  anotada antes de afirmar nada.

## Riesgos

- **Licencia de H264** al compilar desde la fuente (arriba).
- **SPS reescrito por el receptor oficial**: si el SPS que manda RUNE no
  declara cero reordenamiento, el receptor oficial lo reescribe antes de
  descifrar y DAVE rechaza el cuadro (hallazgo de Serein, sin confirmar
  aquí). Hecho en la fase 3: el SPS propio se normaliza antes de cifrar y
  el fixture lo comprueba; falta la prueba con un receptor oficial.
- **Perfil Main** de los emisores de escritorio y OpenH264: con la GPU,
  VA-API y NVDEC lo decodifican; OpenH264 queda de respaldo.
- **CPU**: 1080p60 por software no alcanza en esta máquina; con la GPU sí
  ([PERFORMANCE.md](PERFORMANCE.md#video-por-hardware-2026-10-17-rama-video-por-hardware)).
  La interfaz muestra el fps real y quién codifica. No se baja la calidad a
  escondidas.
- **Calidades sin Nitro**: el servidor podría rechazar o recortar; se
  informa.
- **Gateway de voz v8** con video no confirmado (Serein lo usa).
- **Cuenta**: es un cliente no oficial; video amplía la superficie que
  Discord puede notar. Sin falsificar identidad ni evadir controles.
