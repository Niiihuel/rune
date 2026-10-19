# RUNE — `rune-media`: viabilidad del stack multimedia en Rust (documento de decisión)

- **Estado:** captura, procesamiento, salida local (tono de prueba y escucharse) y referencia de eco implementados; Opus, jitter, mezcla y medios de llamada implementados y probados sólo con fixtures (sin llamada real); vista previa local de pantalla y cámara implementada en Linux; mirar la pantalla que comparte otra persona y las cámaras de la llamada, y compartir la pantalla propia y enviar la cámara, implementados con OpenH264 y probados sólo con fixtures; de la fase 5, el sonido de las aplicaciones al compartir (PipeWire, sin probar en vivo) y el volumen de cada transmisión; protocolo confirmado por REA y fases en [VIDEO.md](VIDEO.md)
- **última verificación: 2026-10-17**
- **Alcance:** audio local, Opus, transporte de voz, captura de pantalla y de cámara. Linux/NixOS/Wayland primero; Windows y macOS más adelante.
- **Convención:** todo lo que no se pudo confirmar contra una fuente primaria o en esta máquina lleva la marca **(no verificado)**.
- Las versiones, fechas y licencias de los crates salen de la API de crates.io consultada el 2026-10-14. El estado de los repositorios sale de la API de GitHub/GitLab del mismo día.

---

## Corregido: los micrófonos ya no incluyen las salidas (2026-10-19, rama `microfonos-sin-salidas`)

Con PipeWire, cpal 0.18.2 lista cada nodo `Audio/Sink` (y su propio
`default_sink`) como dispositivo `Duplex`: sale también entre las entradas,
y abrirlo como entrada graba el monitor de la salida, lo que suena en el
equipo. En la máquina del dueño aparecían `default_sink` y
`alsa_output.pci-0000_65_00.6.analog-stereo` como micrófonos, este con el
mismo nombre que el micrófono real.

- `rune_media::devices` deja fuera de `inputs` los `Duplex` del host
  PipeWire (`records_output`, probada con datos fijos). Es lo único que cpal
  expone de la clase del nodo: los nombres (`sink_*`, `alsa_output.*`) no
  cubren sumideros virtuales como los de EasyEffects. Las salidas no
  cambian.
- ALSA, WASAPI y CoreAudio no se filtran: ahí un `Duplex` es un dispositivo
  real con micrófono (una placa `hw:`, unos auriculares USB).
- Un monitor guardado antes en los ajustes ya no está en la lista, así que
  se ve como un dispositivo desconectado («Dispositivo no disponible»), y
  la captura (`CpalBackend::open`) abre el micrófono predeterminado en su
  lugar: nunca lo que suena en el equipo.
- Límite: un nodo `Audio/Duplex` genuino de PipeWire (raro; por ejemplo,
  FireWire) también se oculta, porque cpal lo muestra igual que un sumidero.
- Verificado en esta máquina con el ejemplo `devices`: las entradas son
  `default_input` y `alsa_input.pci-0000_65_00.6.analog-stereo`; las tres
  salidas siguen. Sin probar en vivo: un ajuste viejo que apuntaba al
  monitor (la captura con hardware no tiene test automático).

## Implementado: panel rápido de voz y prueba de sonido como Serein (2026-10-18, rama `voz-estilo-serein`)

Pedido del dueño: el diseño visual y funcional de Serein para la voz y la
prueba de sonido. Se reimplementó en limn; no se copió código, texto ni
recurso de Serein.

- **Panel rápido** (`rune_components::quick_voice`, `rune_ui::quick_voice`).
  La flecha del micrófono abre la mitad de entrada y la de los auriculares
  la de salida, en el panel de la cuenta y también en la barra de la
  llamada (`vz:input-options`, `vz:output-options`, de 20 px). Es una
  ventana de 340 px, margen de 16 y esquinas de 12, con título, cuerpo con
  scroll (180 a 420 px) y «Todos los ajustes de voz» al pie. Sus controles
  son los de la página de Voz (`us:…`): elegir en una lista abierta no la
  cierra; un clic afuera, sí. Entrada: dispositivo, volumen de 0 a 200 %,
  nivel en vivo con la marca del umbral, modo de entrada como selector
  (voz o pulsar para hablar), supresión de ruido, «Actualizar
  dispositivos» y «Restablecer niveles». Salida: dispositivo, volumen,
  «Probar sonido», qué hace ensordecer y, en una llamada que lo tiene, el
  código de privacidad desplegable. Los dispositivos se buscan una vez al
  abrir, sin abrirlos. El nivel en vivo arranca solo si la llamada ya tiene
  el micrófono abierto (conectada y sin silencio); si no, con «Probar
  micrófono». Cerrar el panel apaga la prueba que abrió.
  Desde el 2026-10-18 (rama `llamadas-y-panel-de-voz`), en una ventana
  más baja que el panel el cuerpo cede alto (`shrink_height_to_fit` de
  limn, dentro del lugar que el popover de limn le deja en la ventana) y
  se desplaza: el título y «Todos los ajustes de voz» quedan a la vista
  ([captura a 480 px de alto](img/voz-panel-bajo-2026-10-18.png)).
- **Panel de la cámara** (2026-10-18, rama `llamadas-y-panel-de-voz`), como
  la flecha de la cámara de Serein: `vz:camera-options` junto a la cámara
  de la barra de la llamada abre «Ajustes de cámara» con la lista de
  cámaras de la página de Voz (`us:select:camera`), la vista previa en un
  cuadro 16:9 (la superficie `video:camera`, escalada a su cuadro para
  que pueda verse a la vez que el mosaico de la propia cámara) y «Probar
  video», que abre la cámara sólo en este equipo: no la manda a la
  llamada ni le avisa al proveedor. Con la cámara ya encendida en la
  llamada muestra lo que se envía y no ofrece la prueba. Cerrarlo, otro
  panel o el fin de la llamada apagan la prueba
  ([captura](img/llamadas-camara-2026-10-18.png)). Desde el 2026-10-19
  (`limn-teclado-y-superficies`) la vista previa de la propia cámara se ve
  en espejo, como en Serein (`Element::surface_mirrored`), aquí, en el
  escenario y en Ajustes › Voz y video; lo que se manda a la llamada no
  cambia.
- **Listas de dispositivos** (`quick_voice::device_select`, también en la
  página): «Predeterminado del sistema» primero, hasta 32 dispositivos,
  nombres recortados a 40 caracteres con el nombre entero como pista del
  elegido y de cada opción recortada (`DropdownOption::tooltip`, desde el
  2026-10-18); un elegido que ya no está dice «Dispositivo no disponible».
- **Volúmenes** (`quick_voice::percent_slider`): pista de 28 px con el valor
  y «%» a la derecha; paso de 1 % con las flechas (201 paradas); debajo,
  que 100 % es el nivel original. La página los pone en dos columnas desde
  480 px y uno bajo el otro si no; «Restablecer niveles» sólo aparece si
  cambiaron.
- **Prueba de micrófono**: «Empezar la prueba» / «Detener la prueba»,
  nivel en dBFS como texto y medidor (`quick_voice::level_meter`) de 9 px
  verde, amarillo desde el 70 % y rojo desde el 90 %, con la marca del
  umbral encima (con Personalizado o sensibilidad propia; con Aislamiento
  de voz decide RNNoise y no hay umbral). Siguen las barras de antes y
  después del procesamiento. **En llamada también funciona**: mide la misma
  captura que usa la llamada y no cambia lo que se envía; «Escucharme» se
  desactiva en una llamada con audio real, porque la salida es de ella.
- **Prueba de salida**: «Probar sonido» toca el sonido de entrar a la
  llamada (`CallSound::Join`, el propio o el de la carpeta) por el camino
  de los sonidos: solo en la salida elegida, o mezclado en la llamada sin
  cortarla. Se quitó el tono repetido (`Audio::tone`), que reemplazaba la
  salida de la llamada.
- **Página de Voz** en el orden de Serein: Dispositivos y niveles,
  Comprobemos el micrófono, Perfil de entrada, Procesamiento de voz, Modo
  de entrada (sensibilidad y atajo de pulsar para hablar), Sonidos de la
  llamada, Cámara y video (con la aceleración por hardware). No se perdió
  ninguna opción.

Capturas con la simulación (Xvfb privado 1280×800): panel
[antes](img/voz-panel-antes-2026-10-18.png) y
[después](img/voz-panel-2026-10-18.png); barra de la llamada
[antes](img/voz-barra-antes-2026-10-18.png) y
[después](img/voz-barra-2026-10-18.png); página
[antes](img/voz-ajustes-antes-2026-10-18.png),
[después](img/voz-ajustes-2026-10-18.png) y
[la prueba con la marca del umbral](img/voz-prueba-2026-10-18.png). Las
capturas no abren el micrófono: el medidor en vivo no aparece en ellas.

## Implementado: sonido al compartir, volumen de cada transmisión y otros extras de video (2026-10-17)

**No probado contra el servicio real.** Fase 5 de
[VIDEO.md](VIDEO.md#ajustes-de-la-fase-5-2026-10-17).

- **Sonido de las aplicaciones** (`video::sound::ScreenSound`): un hilo
  `rune-screen-sound` con su propio bucle de PipeWire (la conexión local, no
  el remoto del portal, que sólo trae video) lista los nodos
  `Stream/Output/Audio` y abre un flujo de captura por cada uno que no es de
  RUNE (`application.process.id` distinto), a lo sumo 32, apuntado a su
  `object.serial` con `node.dont-fallback` y `node.dont-reconnect`: si la
  aplicación se va o no se encuentra, el flujo no cae en otro nodo y jamás
  en un micrófono. Pide F32 estéreo a 48 kHz (PipeWire convierte). Cada
  20 ms suma las aplicaciones que tienen al menos dos cuadros en búfer
  (a lo sumo diez; lo más viejo se tira), recorta y codifica Opus estéreo
  `Audio` a 128 kbit/s; el silencio no se manda. El host lo sostiene
  mientras `rune_ui::video_sound` (compartiendo, «Con sonido», sin pausa) y
  lo manda a `VideoLink::sound`; si termina con un motivo, la pantalla
  sigue sin sonido y la interfaz lo dice. **La lectura de PipeWire no se
  probó en vivo**: sí la mezcla (`Mix`) y el envío con fixtures.
- **Volumen de cada transmisión.** El sonido de un stream mirado entra al
  mezclador con la clave `dueño | mixer::STREAM_SOUND` (bit 63), así su
  volumen y su silencio se eligen aparte de la voz de su dueño, en su menú,
  y se guardan como los demás.
- **Pantalla completa y ventana aparte.** La transmisión sola llena la
  ventana; el host pone la ventana en pantalla completa
  (`ViewportCommand::Fullscreen`). La ventana aparte es un viewport
  inmediato de eframe con su propio `UiTree` de limn; el host presenta el
  mismo cuadro decodificado en una segunda superficie
  (`video:window:{persona}`) a la densidad de esa ventana.
- **El portal recuerda la pantalla.** `Video::share`/`screen` reciben el
  token del portal y `VideoSnapshot::restore_token` devuelve el nuevo.
- **CPU de codificar** medida con el ejemplo `encode_bench`:
  [PERFORMANCE.md](PERFORMANCE.md#codificar-la-pantalla-compartida-2026-10-17).

## Implementado: compartir la pantalla propia (2026-10-16)

**No probado contra el servicio real.** Fase 3 de
[VIDEO.md](VIDEO.md#ajustes-de-la-fase-3-2026-10-16): fixtures locales,
pruebas sin ventana y una captura de la simulación.

- **Captura para enviar.** `Video::share(parent, tamaño, fps)`: la misma
  pantalla del portal, pero cada cuadro se guarda dentro del tamaño de la
  calidad (854×480, 1280×720, 1920×1080, sin agrandar, conservando la forma)
  a lo sumo `fps` veces por segundo, en un único lugar para el codificador
  (`Video::frames`): uno pendiente, el más nuevo; el viejo cuenta como
  descartado. La vista previa recibe una copia a lo sumo 15 veces por
  segundo.
- **Codificar.** `rune-media::video::encode::Encoding`: un hilo
  `rune-video-encode` con OpenH264 (`ScreenContentRealTime`, perfil Baseline
  con `constraint_set1`, o sea Constrained Baseline, control por tasa, sin
  cuadros clave periódicos). Convierte RGBA a I420 con la rutina de OpenH264
  (una columna o fila impar se deja afuera). Cuadro clave al empezar, al
  pedirlo (`Ask::KeyFrame`) y después de una unidad que `send` no tomó. Sólo
  codifica si `room` dice que hay lugar en la cola del proveedor (2
  unidades): si la red no drena, los cuadros se saltean sin codificarse, así
  no hay unidades tiradas ni ráfagas de cuadros clave. Una pantalla quieta
  se vuelve a codificar cada segundo, y el último cuadro sirve para
  responder un pedido de clave. Una tasa nueva (`Ask::Bitrate`, entre
  250 kbit/s y el máximo de la calidad) rehace el codificador sólo con un
  cambio de −15 % o +25 % acumulado, como Serein: la API segura de
  OpenH264 no la cambia en caliente, y rehacerlo cuesta un cuadro clave.
  `Encoding::sent()` dice los cuadros que salieron en el último segundo y la
  tasa actual; soltarlo para el hilo en una vuelta (4 ms).
- **Host.** `apps/desktop/src/video.rs` sostiene la captura en modo envío
  mientras `rune_ui::video_sending` dice que la pantalla va a la llamada, y
  con el `VideoLink` del proveedor un `Encoding` que manda las unidades a
  `outgoing` y traduce `Feedback` a `Ask`; descarta lo que el proveedor dijo
  de un envío anterior. Lo que sale (fps y kbit/s) va a la interfaz. Si la
  captura o el codificador terminan, la transmisión se detiene (`StopStream`)
  y la interfaz dice por qué. Con la simulación no hay `VideoLink`: se
  captura igual y nada se codifica ni sale.
- **Interfaz.** El botón de pantalla, cuando el proveedor la transmite y la
  cuenta puede (`Abilities::share_screen`: capacidad, `CONNECT` y
  `STREAM`), abre primero el selector de calidad en la barra (480p, 720p,
  1080p; 15, 30, 60 fps; 720p30 por defecto, sin mirar Nitro) y recién
  «Compartir pantalla» abre el diálogo del sistema y manda
  `Command::StartStream`. La vista previa «Tu pantalla» dice la fase
  («Conectando…», «En vivo · 58 fps · 2400 kbit/s», «Reconectando…»), con
  « · simulada» en la simulación; «Dejar de compartir» manda `StopStream`.
  Si Discord la termina, se apaga y dice el motivo; no se baja la calidad a
  escondidas ([captura del selector](img/screen-share-2026-10-16.png), Xvfb
  privado de 1280×800).
- **Verificado:** cuadro clave al empezar, al pedirlo (sin cuadro nuevo) y
  tras una unidad no tomada; nada codificado sin lugar; la unidad decodifica
  a su tamaño; tasa acotada y rehecha sólo con cambios grandes; un cuadro
  pendiente para el codificador; liberación al soltar. CPU medida el
  2026-10-17: 1080p60 no alcanza por software en esta máquina
  ([PERFORMANCE.md](PERFORMANCE.md#codificar-la-pantalla-compartida-2026-10-17)).
  Sin verificar: captura de un monitor real y que un cliente oficial lo
  muestre.

## Implementado: mirar la pantalla que comparte otra persona (2026-10-16)

**No probado contra el servicio real.** Fase 1 de [VIDEO.md](VIDEO.md):
fixtures locales, pruebas sin ventana y una captura de la simulación.

- **Decodificar.** `rune-media::video::decode::Decoding`: un hilo
  `rune-video-decode` por stream con OpenH264 (`openh264 =0.9.8`, feature
  por defecto `source`). Toma unidades Annex B que el host le pasa con una
  clausura que no espera, y pide cuadro clave con otra. Empieza en un
  cuadro clave; una unidad no clave que esperó más de 150 ms, o cualquier
  error de decodificación, hace esperar el próximo y pedirlo (a lo sumo
  cada 500 ms). Los cuadros salen RGBA a la cola de dos de las vistas
  previas (descarta el viejo): hasta 1920×1080 con la conversión vectorizada
  de OpenH264; más grandes, reducidos al vecino más cercano
  (`Pixels::I420`); más de 3840×2160 no se muestran. Soltar el `Decoding`
  para el hilo en una vuelta (4 ms).
- **Host.** `apps/desktop/src/video.rs` toma el `VideoLink` del proveedor,
  sostiene un `Decoding` mientras el `Store` dice que se mira un stream
  (no terminado), descarta unidades de otra persona y presenta el último
  cuadro en `video:stream:{persona}` con `limn_egui::present_surface`, como
  las vistas previas; mientras mira consulta cada 16 ms. Con la simulación
  no hay `VideoLink`: dibuja cuadros inventados (bandas de color que se
  mueven, 640×360 a 15 por segundo) y la interfaz dice «simulada». El
  sonido del stream llega por el mismo camino que la voz con
  `VoicePacket::stream` y el host lo mezcla con otra clave (bit 63), así no
  se confunde con la voz de esa persona ni la marca hablando.
- **Interfaz.** «EN VIVO» en la fila y el recuadro de quien transmite
  (`VoiceState::streaming`); «Ver transmisión» (`vz:watch:{id}`) en su
  recuadro, deshabilitado con el motivo de `Store::abilities`
  (`watch_stream`: capacidad `watch_streams`, `VIEW` y `CONNECT`, y estar en
  esa llamada). El stream mirado ocupa el escenario solo (la gente queda en
  la columna), con su fase («Conectando…», «En vivo», «En pausa»,
  «Reconectando…», «Terminó» con el motivo) y «Dejar de ver»
  (`vz:stop-watching`). Pantalla completa y ventana aparte no: limn no las
  tiene ([captura en vivo](img/stream-live-2026-10-16.png),
  [mirando](img/stream-watch-2026-10-16.png), Xvfb privado de 1280×800).
- **Licencia de H264, dicho sin rodeos.** Decisión del dueño del
  2026-10-16: OpenH264 compilado desde la fuente. El código de OpenH264 es
  BSD-2, pero H264 está cubierto por patentes y **la licencia de patentes
  que Cisco paga sólo cubre los binarios que Cisco compila y distribuye**:
  no cubre esta compilación desde la fuente, ni a RUNE ni a quien lo use.
  Para el experimento personal lo asume el dueño; distribuir binarios de
  RUNE con H264 adentro exige revisar las patentes. `shell.nix` suma `nasm`
  (OpenH264 compila igual sin él, más lento) y `libstdc++` al camino de
  bibliotecas, porque OpenH264 es C++.
- **Verificado**: decodificación de un cuadro clave codificado en la prueba
  (tamaño y color), espera de clave y pedido acotado, reducción a 1920×1080,
  liberación al soltar; el resto en
  [DISCORD_SESSION.md](DISCORD_SESSION.md#mirar-la-pantalla-de-otra-persona-go-live-2026-10-16).
  Sin verificar: decodificar lo que mandan los clientes de escritorio
  (perfil Main) y el rendimiento a 1080p60.

## Implementado: mirar las cámaras de la llamada (2026-10-16)

**No probado contra el servicio real.** Fase 2 de [VIDEO.md](VIDEO.md#ajustes-de-la-fase-2-2026-10-16):
fixtures locales, pruebas sin ventana y una captura de la simulación.

- **Host.** `apps/desktop/src/video.rs` reparte las unidades del
  `VideoLink` por fuente (persona y pantalla o cámara) en colas de 4: cada
  `Decoding` toma la suya, y lo de una fuente que nadie decodifica se tira.
  Una cola llena se vacía, pide cuadro clave y espera uno. Sostiene un
  `Decoding` por cámara que `rune_ui::video_cameras` pide (a lo sumo 4: las
  de los mosaicos del escenario de la llamada conectada, sin stream mirado)
  y le dice al proveedor cuáles se ven (`VideoRequest::Wants`), sólo dándolo
  por dicho cuando la cola lo aceptó. Una cámara que deja de verse, colgar
  o cambiar de cuenta suelta su hilo, su cola y su superficie
  `video:camera:{persona}`. Con la simulación, cuadros inventados.
- **Interfaz.** El mosaico muestra la cámara en lugar del avatar, ajustada
  con bandas negras; «Cámara simulada» encima en la simulación
  ([captura](img/camera-watch-2026-10-16.png), Xvfb privado de 1280×800).
- **Verificado:** reparto por fuente y por tipo, cola llena que espera
  clave y la pide, cola que se va con su decodificador, tope de 4 con sus
  `Wants` prendidos y apagados; el resto en
  [DISCORD_SESSION.md](DISCORD_SESSION.md#mirar-las-cámaras-de-la-llamada-2026-10-16).
  Sin verificar: cámaras reales de clientes oficiales y cuatro decodificadores
  a la vez en esta máquina.

## Implementado: volumen por persona, chat del canal de voz y sonidos de la llamada (2026-10-16)

**Nadie escuchó nada de esto todavía**: pruebas sin dispositivos y una
captura de la simulación.

- **Volumen y silencio por persona.** El menú de otra persona (clic
  secundario en su fila o su recuadro, o su «⋯») suma «Volumen del
  usuario» (control deslizante limn de 0 a 200 %, `vz:volume:{id}`) y
  «Silenciar para mí» (`vz:mute:{id}`). Se guardan en este equipo
  (`voice_volumes` en los ajustes: `id:volumen:silencio`, hasta 256, sólo
  los que no son 100 % sin silencio) y nunca se mandan al proveedor: el host
  los pasa a `CallControls::choices` del mezclador. Una elección que vuelve a
  lo normal se olvida y el motor de la llamada la devuelve a 100 %. Quien
  está silenciado para uno lleva una marca (altavoz tachado, «Silenciado para
  vos») en su fila y su recuadro. En la simulación el menú dice «Simulado: la
  llamada no lleva audio». El propio no tiene volumen.
- **Chat del canal de voz.** Capacidad nueva `Capabilities::voice_text`:
  la declaran la simulación (cada sala tiene ocho mensajes inventados) y la
  sesión no oficial (mismas rutas REST y eventos de Gateway que un canal de
  texto; probado sólo con fixtures). `Store::abilities` trata entonces a un
  canal de voz y a un escenario (2026-10-18, mismo chat y mismas rutas) como
  uno con mensajes, con los mismos permisos (ver, historial, escribir…). El botón de chat de la cabecera del escenario («Mostrar
  chat»/«Ocultar chat») abre el canal de voz como conversación al lado del
  escenario (`timeline:{canal}`, `composer:{canal}`), 40 % del ancho entre
  320 y 440 px, desde 720 px de lugar para la conversación como en Serein;
  más angosto el botón queda deshabilitado y dice por qué. Ocultarlo o colgar
  vuelve a la conversación que estaba abierta. Sin la capacidad o sin
  permiso, el botón dice el motivo de `Store::abilities`.
  ([captura](img/voice-chat-2026-10-16.png), Xvfb privado de 1280×800).
- **Sonido de mensajes (2026-10-17).** Un tono propio de 0,38 s
  (`assets/sounds/message.ogg`, mismo método) para cada mención o mensaje
  directo que avisaría (ver [notificaciones](FILES.md#notificaciones-del-sistema-2026-10-17)),
  por el mismo camino que los de la llamada (`Audio::play`). Ajustes →
  Notificaciones → «Sonido de mensajes», guardado como `message_sounds`.
- **Sonidos de la llamada** (reemplazado el 2026-10-17: trece eventos con
  su interruptor, volumen y carpeta propia, ver
  [arriba](#sonidos-de-la-llamada-y-carpeta-propia-2026-10-17)). Siete tonos propios de 0,14 a 0,36 s
  (`assets/sounds/*.ogg`, sintetizados con `ffmpeg` `aevalsrc`, Ogg Opus
  24 kbit/s mono; nada de terceros): alguien entra o sale de la llamada,
  silenciarse y volver, ensordecerse y volver, y desconexión. El host
  compara cada cuadro con el anterior (`apps/desktop/src/sounds.rs`): quién
  más está en `voice_in` de la llamada, los controles propios y si la
  llamada sigue; la primera vista sólo se recuerda (entrar a una sala con
  gente no suena). Con una llamada que lleva audio el tono va por
  `CallMedia::play_sound`, junto a la mezcla y aunque se esté ensordecido (el
  tono de ensordecer es uno); si no, por la salida, que mientras suena no se
  ensordece. Desde el 2026-10-18 (rama `audio-de-mensajes`) un tono fuera de
  una llamada ya no toma la entrada de la salida: va por
  `Playback::sound_feed`, que el hilo de la salida suma muestra a muestra
  con su `Feed` (hasta cuatro a la vez, como el motor de la llamada), así
  suena encima de un mensaje de voz o de la prueba de escucharse sin
  cortarlos. Ensordecido, el mensaje de voz sigue callado mientras suena el
  tono de ensordecer, y la prueba de escucharse también: ensordecido, la
  captura no alimenta el monitor (`CaptureRequest::monitor`). Ajustes → Voz y video →
  **Sonidos de la llamada**: «Alguien entra o sale», «Silenciar y
  ensordecer» y «Desconexión», guardados (`sound_people`, `sound_self`,
  `sound_disconnect`).
- **Verificado**: pruebas de componentes (menú con el control y el silencio,
  marca, botón de chat), de `rune-ui` (volumen guardado y nunca enviado,
  chat al lado y vuelta a la conversación, envío en el canal de voz, ventana
  angosta, ajustes de sonidos), del estado (habilidades con `voice_text`), de
  la sesión (historial y `MESSAGE_CREATE` de un canal de voz con fixtures),
  del motor (un tono se oye ensordecido; una elección quitada vuelve a 100 %)
  y del host (cuándo suena cada tono, y que los siete archivos se decodifican
  y duran lo esperado). **No verificado**: oír los tonos o el volumen por
  persona, ni el chat de voz con una cuenta real.

## Implementado: reproducir mensajes de voz (2026-10-16)

Sólo lectura y reproducción; RUNE no graba ni envía mensajes de voz.

- **Descarga** (`rune-core::clips`): sólo al pulsar reproducir, a memoria,
  hasta 8 MiB, con los mismos orígenes validados que las demás descargas
  (HTTPS, sin credenciales ni redirecciones) y el tamaño anunciado exacto.
  Una petición nueva reemplaza la anterior; cerrar sesión la cancela. El
  archivo no se guarda en disco ni en SQLite.
- **Ogg Opus** (`rune-media::clip::ogg`): un lector propio de páginas Ogg
  (no se sumó una dependencia para eso) entrega los paquetes al
  decodificador Opus que ya usan las llamadas, en mono a 48 kHz. Admite el
  mapeo de canales 0 (mono o estéreo, que se mezcla a mono) y, desde el
  2026-10-18 (rama `audio-de-mensajes`), el 1: hasta ocho canales en varios
  streams, con el decodificador multistream de la misma libopus (por
  `opusic-sys`, las bindings sobre las que está `opus`, sin biblioteca
  nueva), promediados a mono. Los mapeos 2, 3 (ambisonics) y 255 se
  rechazan con su motivo. Cada página se compara con su CRC-32 (polinomio
  0x04C11DB7, el de Ogg); una que no coincide rechaza el archivo. La
  posición de gránulo de la última página marca el final: el relleno del
  codificador ya no suena (hasta un paquete, 20 ms en un mensaje de voz) y
  la duración se sabe al leer el archivo, a la muestra. Los gránulos se
  cuentan desde donde empieza el stream: el de la primera página de audio
  menos las muestras de sus paquetes (RFC 7845 §4.5, un stream cortado de
  otro más largo). Un final que no pasa del pre-skip (un muxer que escribe
  0) no dice nada y suena todo, como se decodifica; uno de más de cuatro
  horas rechaza el archivo, como cualquier otro. Atajo marcado en el código: buscar
  decodifica desde el principio.
- **Salida**: un hilo `rune-clip` empuja el audio al `Feed` de la salida
  elegida, a lo sumo 60 ms por delante del reloj, y espera pausado hasta
  que la salida esté en marcha. La interfaz sólo lee la posición. Uno por
  vez: cambiar de conversación, de cuenta o cerrar la ventana lo detiene.
- **Durante una llamada conectada** el hilo del clip llena una cola propia
  (`rune_media::tap`, 0,5 s) que `CallMedia::play_alongside` entrega al
  motor de la llamada; éste la suma, 20 ms por tic, a lo que mezcla de los
  demás antes del `Feed` de la salida. Nunca toca el micrófono ni lo que se
  envía (la cancelación de eco lo ve como cualquier otro sonido de la
  salida). Más de 200 ms esperando significa que la llamada se atrasó y se
  descarta lo más viejo. Terminar el clip suelta la cola y la llamada sigue;
  si la llamada empieza o termina en medio, el host lo vuelve a abrir en el
  otro destino desde la posición en que iba. Ensordecido no se oye, como
  fuera de una llamada.
- **Velocidad** 0,75 / 1 / 1,5 / 2× como el cliente oficial, sin cambiar el
  tono: `rune-media::stretch`, un WSOLA propio (unas 90 líneas, sin
  dependencia nueva): cuadros de 30 ms con ventana de Hann al 50 %, cada uno
  corrido hasta ±6,7 ms a donde mejor continúa al anterior (correlación
  normalizada con paso de dos muestras). A 1× el audio sale tal cual
  decodificado. El reloj del clip avanza a la velocidad elegida y el adelanto
  de 60 ms se mide en audio de salida. Cambiar de velocidad deja sonar lo ya
  encolado y sigue después; pasar a 1× vacía el estirador con un fundido.
- **Posición por clip** (`rune-ui`): al pausar y reproducir otro, salir de
  la conversación o volver, cada clip sigue donde quedó (hasta 64, en
  memoria; cambiar de cuenta o cerrar sesión las olvida). Uno que terminó
  vuelve a empezar. Desde el 2026-10-18 el reproductor de un clip que no
  suena ya muestra ese lugar (barras encendidas y tiempo que queda) antes
  de pulsarlo (`Live::voice_left`). La velocidad es una sola para todos,
  de la sesión; en una fila de menos de 166 px su botón pasa debajo del
  reproductor en vez de desaparecer.
- **Simulación**: el mensaje de voz de #general no tiene archivo remoto; el
  host reproduce `assets/voice-sample.ogg`, cuatro segundos de un tono
  sintetizado con `ffmpeg` (`aevalsrc`, libopus 16 kbit/s, mono), sin voz de
  nadie. Su forma de onda está en `SIMULATED_VOICE` del proveedor simulado.
- **Fixtures del lector** (`crates/rune-media/fixtures/`), que FFmpeg 8.1.1
  vuelve a dar byte a byte:
  - `short.ogg`, 110 ms de 440 Hz en mono, el último paquete casi todo
    relleno: `ffmpeg -f lavfi -i "aevalsrc=0.5*sin(2*PI*440*t):s=48000:d=0.11"
    -ac 1 -c:a libopus -b:a 24k -map_metadata -1 -fflags +bitexact
    -flags:a +bitexact short.ogg`.
  - `surround.ogg`, un segundo de 440 Hz sólo en el canal derecho de tres
    (3.0; mapeo 1, dos streams, uno estéreo): `ffmpeg -f lavfi -i
    "aevalsrc=0|0.5*sin(2*PI*440*t)|0:s=48000:d=1:c=3.0" -c:a libopus
    -b:a 48k -mapping_family 1 -map_metadata -1 -fflags +bitexact
    -flags:a +bitexact surround.ogg`.
- **Verificado**: pruebas de lectura y decodificación del archivo real,
  ritmo, pausa, búsqueda y fin del hilo, y `--capture-voice`, que reproduce
  el mensaje simulado en la salida real de esta máquina y captura el
  reproductor avanzando
  ([captura](img/voice-message-2026-10-16.png)). Velocidad: una prueba
  mantiene la frecuencia de un tono de 220 Hz (cruces por cero) a 0,75, 1,5 y
  2× mientras la duración escala, y otra termina un clip a 2× en la mitad del
  tiempo; `--capture-voice` ahora lo reproduce a 1,5× en la salida real
  ([captura](img/voice-speed-2026-10-16.png)). Llamada: una prueba del motor
  suma el clip a la salida, deja el micrófono enviado en silencio y suelta la
  cola al terminar sin cortar la llamada. **No verificado**: que se oiga bien
  (nadie lo escuchó, tampoco estirado), reproducir durante una llamada real
  (sólo el motor con fixtures) ni un mensaje de voz real del servicio.

## Implementado: WebM y MKV, y cámaras que sólo dan MJPEG (2026-10-18, rama `video-y-documentos`)

**Videos WebM y MKV en la conversación.** VP8, VP9 y AV1 (y H.264 dentro de
un MKV) se reproducen como los MP4: la tarjeta tiene el mismo reproductor,
pantalla completa, búsqueda y volumen. El sonido sigue con Symphonia (Opus
y Vorbis de WebM ya se leían); las imágenes las lee y decodifica
**GStreamer**, que RUNE ya usa para el video por hardware:

- `rune_media::mkv`, un lector chico de la cabecera Matroska (EBML hasta el
  primer `Cluster`, cada tamaño contra los bytes que lo contienen), dice el
  códec de la primera pista de video, su tamaño (`PixelWidth` y
  `PixelHeight`) y la duración antes de empezar. Una pista de más de
  3840×2160 según su cabecera falla antes de decodificar: «Las imágenes de
  este video son más grandes de lo que RUNE muestra.» Un
  códec que RUNE no reproduce (H.265: el único decodificador por software a
  mano, libde265, trae sus patentes) falla con su nombre, como en MP4, y la
  tarjeta ofrece «Abrir en el navegador» y «Guardar como…».
- `clip::stream`: un pipeline `appsrc ! matroskademux ! <decodificador> !
  videoconvert ! RGBA ! appsink` por video. El `appsrc` es de acceso
  aleatorio sobre el archivo en memoria (le entrega a `matroskademux` las
  partes que pide, sin copiarlas), así el demuxer salta por el índice del
  archivo. Decodificadores, el primero que esté: `vp8dec`, `vp9dec`
  (gst-plugins-good), `dav1ddec` si el sistema lo tiene o `av1dec` (libaom,
  gst-plugins-bad), `h264parse ! openh264dec` (bad). `shell.nix` suma
  `gst-plugins-good` a los plugins de RUNE (`RUNE_GST_PLUGINS`).
- El hilo `rune-clip-video` sigue el reloj del sonido como el de MP4: a lo
  sumo dos imágenes por delante (el `appsink` guarda dos y frena al
  pipeline) y sólo la más nueva que corresponde; un salto busca en el
  pipeline (`FLUSH | ACCURATE`), así la primera imagen es la del lugar
  pedido, también en pausa. Las imágenes se escalan a 1920×1080 como mucho.
  Una imagen que no se puede mostrar (más grande de lo que dijo la
  cabecera) o un salto que el pipeline no acepta en medio segundo terminan
  el video con «RUNE no pudo decodificar este video.», no sin imagen.
- Un WebM sin sonido que no dice cuánto dura (una grabación en vivo) no se
  reproduce: «Este video no dice cuánto dura y no tiene sonido.»
- Simulación: `demo.webm` (VP9 y Opus, `assets/video-sample.webm`, el mismo
  patrón de 4 s) junto a `demo.mp4` en #general; el host elige la muestra
  por el nombre.
- Verificado con fixtures de FFmpeg (`clip-vp8.webm` con Opus,
  `clip-vp9.webm`, `clip-av1.mkv`, `clip-h264.mkv`, `hevc.mkv`): duración,
  primera imagen en pausa, imágenes que avanzan, un salto en pausa que
  muestra la misma imagen que un video empezado en ese lugar, y el final;
  H.265 dice cuál es; una cabecera de 5120×1440 se rechaza con su motivo. **No verificado** con
  archivos reales de Discord ni con 1080p.

**Cámaras que sólo dan MJPEG.** Además del formato crudo (YUY2, NV12,
BGRx, RGBx) se ofrece un `EnumFormat` MJPG; PipeWire elige con la cámara.
Cada imagen es un JPEG que decodifica `zune-jpeg` (el mismo decodificador
de `image`, ya en el árbol) a RGBA y se escala como las demás. Muchas
cámaras omiten las tablas Huffman cuando son las estándar (como el MJPEG
de AVI): RUNE agrega la marca `AVI1`, que hace que el decodificador ponga
las estándar donde faltan. Probado con una imagen sintética de 64×48 en
4:2:2, con y sin sus tablas; **sin probar con una cámara real**.

## Implementado: reproducir audio y video adjuntos y GIF de tarjetas (2026-10-17)

Recorrido 16.4, con Serein como referencia de comportamiento (sin copiar
código; Serein usa GStreamer, Media Foundation o VideoToolbox según el
sistema y Symphonia para el sonido). RUNE eligió la opción más chica que
cubre lo que Discord lleva, sin decodificadores del sistema:

- **Formatos**: sonido MP3, M4A/AAC, Ogg Vorbis, Ogg Opus (`.opus`), WAV,
  FLAC y WebM con sólo sonido; video MP4/MOV/M4V con imágenes H.264
  (Baseline, Main y High con B-frames, verificados con fixtures propios) y
  sonido AAC o sin sonido (WebM y MKV, desde el 2026-10-18: arriba). H.265
  no se reproduce: un MP4 con H.265 lo descubre al leerlo y dice «Este
  video usa H.265 (HEVC), que RUNE no reproduce.» con «Abrir en el
  navegador» y «Guardar como…».
- **Dependencia nueva**: `symphonia` 0.5.5 sin funciones por defecto, con
  `aac`, `flac`, `isomp4`, `mkv`, `mp3`, `ogg`, `pcm`, `vorbis` y `wav`
  (Rust puro, sin bibliotecas del sistema; justificada en
  `crates/rune-media/Cargo.toml`). El Opus de cualquier contenedor va al
  libopus que ya usan las llamadas. Las imágenes H.264 las decodifica el
  OpenH264 que ya mira transmisiones, sin forzar la salida de cada cuadro
  (`flush_after_decode(NoFlush)`), así los B-frames salen en orden.
- **Lector MP4 propio** (`rune_media::mp4`, unas 400 líneas con tests):
  sólo `moov` con sus tablas (`stsd`/`avcC`, `stts`, `ctts`, `stsc`,
  `stsz`/`stz2`, `stco`/`co64`, `stss`), sin fragmentos. Cada cantidad se
  compara con los bytes que la contienen antes de reservar memoria, cada
  muestra con el largo del archivo, y una pista admite a lo sumo 432 000
  muestras (dos horas a 60 cuadros). El sonido de un MP4 lo lee Symphonia.
- **Reproductor** (`rune_media::Clip`, el mismo de los mensajes de voz): un
  hilo `rune-clip` lee el archivo (nada se lee en el hilo de la interfaz),
  mezcla a mono, remuestrea a 48 kHz y empuja al `Feed` de la salida
  elegida (o al de la llamada, como un mensaje de voz) a lo sumo 60 ms por
  delante del reloj, con su volumen. Un video suma un hilo
  `rune-clip-video` que decodifica a lo sumo dos imágenes por delante del
  reloj del sonido (o de un silencio tan largo como el video, si no tiene
  sonido) y deja sólo la más nueva que corresponde; buscar recomienza desde
  la imagen clave anterior. Las imágenes se escalan a 1920×1080 como mucho.
  El host las presenta en una superficie limn (`media:{adjunto}`) hasta 30
  veces por segundo y la olvida, textura incluida, cuando el reproductor
  se detiene.
- **Interfaz**: tarjeta de audio con nombre, tamaño, reproducir/pausar,
  barra de tiempo (`Slider` de limn: busca al soltar), `m:ss / m:ss`,
  silenciar y volumen; un video muestra su imagen en la caja de la tarjeta
  (16:9 si no se conoce su tamaño) y suma «Pantalla completa»: un `Modal`
  de limn del tamaño de la ventana con la imagen y los mismos controles
  (Espacio pausa, Esc sale). Uno por vez; el volumen se recuerda en la
  sesión y la posición de cada clip como en los mensajes de voz.
- **Descarga y memoria**: sólo al pulsar reproducir, a memoria, hasta
  **50 MiB** (`Attachment::MAX_PLAYED_BYTES`; más grande, la tarjeta lo
  dice y ofrece el navegador o guardarlo), por los mismos orígenes
  validados que las demás descargas (el host de archivos de Discord, HTTPS
  sin credenciales ni redirecciones, tamaño exacto). El archivo vive
  mientras su reproductor existe; se suelta al detenerlo, al cambiar de
  conversación o de cuenta, al cerrar sesión o la ventana. Sonido de más de
  4 horas se rechaza.
- **Pausas**: un video que sale de la vista se pausa, salvo a pantalla
  completa (el sonido de un audio sigue, como un mensaje de voz). Nada se
  reproduce solo.
- **GIF de tarjetas (`gifv`)**: Tenor y Giphy llegan como un MP4 corto por
  el proxy de imágenes de Discord (`video.proxy_url`, `images-ext-*`). El
  adaptador lo guarda como `ImageKind::Clip`; el núcleo lo descarga como
  una vista previa (hasta 8 MiB), lo decodifica con el mismo lector y
  OpenH264 y lo convierte en cuadros de animación con los presupuestos de
  un GIF (16 MiB y 120 cuadros por animación, mitad de cuadros si sobran, 3
  s de decodificación). Su primera imagen reemplaza la miniatura y anima
  donde animan los GIF: filas visibles, sin movimiento reducido, ventana no
  minimizada; si no se puede leer queda la miniatura.
- **Simulación**: un audio y un video en #general sin archivo remoto; el
  host reproduce `assets/audio-sample.mp3` (3 s de un tono) y
  `assets/video-sample.mp4` (4 s de un patrón de prueba con tono, H.264 y
  AAC), generados con `ffmpeg` (`sine`, `testsrc2`), sin grabaciones.
  Los tests usan fixtures del mismo origen en `crates/rune-media/fixtures/`
  (unos 100 KB en total).
- **Verificado**: cada formato decodifica a su duración, con su tono, y
  busca a la mitad; un video entrega sus 30 imágenes en el orden en que se
  muestran (High con B-frames) y su primera imagen en pausa; un H.265 falla
  con su motivo; un clip de GIF se vuelve primera imagen más animación de
  10 o 30 cuadros. `--capture-player` reproduce el video simulado en la
  salida real y captura su imagen avanzando
  ([captura](img/media-player-2026-10-17.png)). **No verificado**: que se
  oiga o se vea bien con archivos reales de Discord (sólo fixtures y
  muestras generadas), CPU con videos de 1080p, y archivos con sonido
  multicanal real.

## Sonidos de la llamada y carpeta propia (2026-10-17)

Reemplaza la parte de sonidos de la sección del 2026-10-16 de abajo.
**Nadie escuchó los tonos todavía**; pruebas sin dispositivos y capturas
de la ventana real ([eventos](img/sonidos-2026-10-17.png),
[carpeta](img/sonidos-carpeta-2026-10-17.png), Xvfb privado de 1280×800).

- **Eventos** (`rune_ui::CallSound`, la clave es el nombre del archivo):
  entrar (`join`, la primera vez que la llamada conecta; volver de
  «Reconectando» no suena) y salir (`leave`, al colgar); alguien entra o
  sale (`user_join`, `user_leave`; la primera mirada a una sala con gente
  no suena); silenciar, dejar de silenciar, ensordecer y dejar de
  ensordecer (`mute`, `unmute`, `deafen`, `undeafen`); empezar y terminar la
  transmisión propia (`stream_start`, `stream_stop`, cuando el proveedor la
  da en vivo y cuando termina); alguien empieza a transmitir
  (`user_stream`); desconexión o error (`disconnect`: la llamada falla o el
  servicio la termina con un motivo, una sola vez); mensaje nuevo
  (`message`, el aviso de mención o mensaje directo de siempre); y desde
  las llamadas privadas (2026-10-17) **llamada entrante** (`ring`, cada
  2,5 s mientras suena; no en No molestar) y **llamando a alguien**
  (`ring_out`, cada 3 s mientras la llamada propia hace sonar a otros), los
  únicos que se repiten (`CallSound::repeats`, `Watch::ring`). Si pasan
  varios en el mismo cuadro suena uno: los propios primero, después la
  llamada, la transmisión propia y lo de los demás. El host compara cada
  cuadro con el anterior (`apps/desktop/src/sounds.rs`).
- **Sonidos de RUNE:** trece tonos de 0,16 a 0,40 s, y los dos timbres
  de 1,1 y 0,9 s (2026-10-17), generados por
  `scripts/sounds.sh` (`ffmpeg` `aevalsrc`: notas senoidales con su octava,
  ataque de 4 ms y caída exponencial; Ogg Opus mono de 24 kbit/s, salida
  idéntica byte a byte al correrlo de nuevo). Son de RUNE, con su licencia
  ([AUDIO-NOTICES.md](../assets/licenses/AUDIO-NOTICES.md)). Los de Discord
  no se extraen ni se empaquetan: son de Discord.
- **Ajustes → Voz y video → Sonidos de la llamada:** volumen de los
  sonidos (0–100 %, bajo el de salida, `sound_volume`), un interruptor y un
  «Probar» por evento (`sounds_off`; el del mensaje sigue siendo
  `message_sounds`, el mismo de Notificaciones) y la **carpeta de sonidos**
  (`sound_folder`): se escribe la ruta (desde `/` o `~/`) y «Usar», o
  «Elegir…» la pide al diálogo del sistema (2026-10-18,
  `FileChooser.OpenFile` con `directory`, el mismo lugar de diálogo que
  adjuntar; [captura](img/escritorio-linux-sonidos-2026-10-18.png)): la
  ruta elegida se guarda tal cual, sin recortar espacios; si el diálogo no
  abre, el motivo va debajo sin borrar lo que se sabe de la carpeta en uso,
  y cerrar sesión o cambiar de proveedor lo cierra. La
  página dice cuáles tomó, cuáles no y por qué, y cada evento que usa un
  archivo propio lo dice debajo. «Usar» de nuevo la vuelve a leer (por si
  se agregaron archivos); «Quitar» vuelve a los de RUNE. Los ajustes de
  antes (`sound_people`, `sound_self`, `sound_disconnect`) se leen como los
  eventos que agrupaban.
- **La carpeta** se lee en un hilo propio (`rune-sounds`,
  `rune_media::sounds::load`), nunca en el de la interfaz: por evento, el
  primero de `{nombre}.ogg`, `.opus`, `.mp3`, `.wav` que exista; tiene que
  ser un archivo de hasta 1 MiB que los decodificadores de RUNE (libopus y
  Symphonia) lean entero en hasta 5 s de sonido. Lo que no cumple suena con
  el de RUNE.
- **Reproducción:** como antes, con una llamada que lleva audio va por el
  mezclador de la llamada (`CallMedia::play_sound`), si no sola por la
  salida elegida, al volumen de los sonidos. Ensordecido no suena nada
  salvo ensordecer y dejar de ensordecer, una llamada entrante (2026-10-17:
  nada más la anunciaría con sonido) y «Probar».

## Supresión de ruido mejorada (2026-10-17)

Pedido del dueño tras la primera llamada real. Números, método y por qué
rendía peor en [PERFORMANCE.md](PERFORMANCE.md#supresión-de-ruido-medida-2026-10-17-rama-audio);
**nadie lo escuchó todavía**.

- **48 kHz siempre.** La captura pide 48 kHz al dispositivo (como Serein) y,
  si no lo ofrece, remuestrea antes de procesar: RNNoise corre con
  cualquier micrófono y lo que sale ya está a la tasa de la llamada.
- **Compuerta de voz** (`CaptureProcessor::set_speech_gate`), último paso
  de la cadena: deja pasar lo que RNNoise oye como voz (dos cuadros de
  10 ms seguidos con probabilidad ≥ 0,6 que no sean un clic), lo sostiene
  300 ms tras la última voz y abre y cierra con rampas (5 y 20 ms). Está
  con **Detección de voz + sensibilidad automática + RNNoise**, lo
  predeterminado (perfil Aislamiento de voz); entonces la llamada y el
  anillo de «hablando» toman lo que la compuerta dejó pasar. Con Pulsar para
  hablar, con un umbral elegido a mano o sin RNNoise decide el nivel, como
  antes, pero un clic suelto ya no abre el micrófono (se abre por el nivel
  sostenido del cuadro, `VoiceActivity::update_sustained`).
- **Eco:** la cancelación (AEC3 de Sonora, la misma de Serein) ya no
  reinicia RNNoise ni la ganancia al empezar o terminar de sonar algo.
- **Ajustes → Voz y video:** el perfil dice que incluye la compuerta;
  RNNoise dice que sirve con cualquier micrófono; con la compuerta, la
  sensibilidad dice «Decide RNNoise…» y el umbral «lo que RNNoise oye como
  voz».

**Qué probar con el micrófono** (con auriculares; nada se graba ni se
guarda):

1. En la ventana: Ajustes → Voz y video → Comprobemos el micrófono, con
   «Escucharme» prendido. Se oye lo mismo que oiría la llamada: hablar,
   escribir en el teclado sin hablar (no tiene que oírse nada), dejar el
   ventilador o la música de fondo (silencio entre frases). Cambiar el
   perfil a Estudio para comparar.
2. Sin la ventana, el medidor por consola:
   `nix-shell --run 'cargo run --release -p rune-media --example devices -- --meter --rnnoise --gate'`
   imprime nivel de entrada y procesado cada 100 ms durante 3 s: escribiendo
   sin hablar, «procesado» tiene que quedar en 0,00 con `--gate` y no sin él.
3. Escucharse por consola, con auriculares:
   `nix-shell --run 'cargo run --release -p rune-media --example playback -- --loopback --rnnoise --gate --seconds 20'`
   (sumar `--agc` y `--echo` para la cadena completa; sin `--gate` se oye
   RNNoise solo, con su ruido residual entre palabras).
4. En una llamada real: que la otra persona diga si se corta el principio
   de las frases (sobre todo si empiezan con «s» o «f»), si se oye el
   teclado y si la voz suena natural. Si se corta, Ajustes → sensibilidad
   automática apagada vuelve a la compuerta por nivel.

## Implementado: perfiles de entrada, RNNoise y ganancia automática (2026-10-16)

**Ajustes → Voz y video** sigue ahora el orden de Discord (y de Serein):
**Voz** (dispositivos de entrada y salida lado a lado, con sus volúmenes
debajo y «Probar sonido»), **Comprobemos el micrófono** (un botón, niveles
de entrada y procesada, «Escucharme»), **Perfil de entrada**, **Modo de
entrada** (Detección de voz con sensibilidad automática o propia / Pulsar
para hablar), **Procesamiento de voz** y **Video**.

- **Perfiles:** *Aislamiento de voz* = RNNoise + cancelación de eco +
  ganancia automática; *Estudio* = nada; *Personalizado* = lo elegido
  abajo. Tocar un control de procesamiento con un perfil fijo lo pasa a
  Personalizado partiendo de lo que el perfil mostraba. El predeterminado es
  Personalizado con todo apagado, como antes: nada empieza a procesar solo.
  Discord arranca en su equivalente de Aislamiento de voz; es un cambio de
  una línea (`InputProfile::default`) si se quiere igual.
- **Supresión de ruido:** RNNoise (`nnnoiseless` 0.5.2, puro Rust, BSD-3),
  WebRTC (Sonora, como antes) o Ninguna. Krisp es privativo y no está; la
  interfaz lo dice. RNNoise trabaja sólo a 48 kHz en cuadros de 480
  muestras (10 ms, el cuadro de RUNE a esa tasa); con un micrófono a otra
  tasa se usaba WebRTC (desde el 2026-10-17 se remuestrea a 48 kHz, ver
  arriba). El estado se crea y se
  ceba con un cuadro de silencio en el trabajador; procesar no reserva
  memoria (escala PCM16 en el búfer reutilizable).
- **Cadena:** volumen de entrada → (pasaaltos + NS de WebRTC + AEC) →
  RNNoise → AGC2. RNNoise va después del cancelador, que necesita el
  micrófono lineal respecto de lo reproducido, y antes de la ganancia.
- **Ganancia automática:** AGC2 de Sonora, sólo digital adaptativa (máximo
  20 dB, arranca en 0 dB). No toca el volumen del sistema. Se adapta con su
  propio detector de voz: las señales sintéticas de las pruebas no lo
  mueven, así que **su efecto sobre voz real no está verificado**.
- **Volumen de entrada:** 0–200 %, aplicado antes de todo (y del medidor).
  Viaja por un atómico: arrastrar el control no reinicia el procesamiento.
- **Sensibilidad automática:** `VoiceActivity::automatic` sigue el nivel más
  bajo reciente (baja al instante, sube 1,2 dB/s) y pone el umbral 15 dB
  encima, entre -42 y -15 dBFS. El indicador de «hablando» y la compuerta de
  la llamada usan los mismos umbrales; la página muestra dónde quedó.
- Cambiar supresor, eco o ganancia reconfigura la captura abierta sin
  reabrir el dispositivo (probado con el backend falso).

Ajustes guardados: `input_profile` (`voice-isolation`/`studio`/`custom`),
`noise_suppression` (`off`/`webrtc`/`rnnoise`; el `true` viejo se lee como
`webrtc`), `echo_cancellation`, `automatic_gain`, `input_volume`.

**No se escuchó nada en esta entrega.** Para comparar a oído (con
auriculares):

```sh
nix-shell --run 'cargo run --release -p rune-media --example playback -- --loopback --seconds 20'             # sin procesar
nix-shell --run 'cargo run --release -p rune-media --example playback -- --loopback --seconds 20 --denoise'   # WebRTC
nix-shell --run 'cargo run --release -p rune-media --example playback -- --loopback --seconds 20 --rnnoise'   # RNNoise
nix-shell --run 'cargo run --release -p rune-media --example playback -- --loopback --seconds 20 --rnnoise --agc'
nix-shell --run 'cargo run --release -p rune-media --example devices -- --meter --rnnoise'  # niveles, sin sonido
nix-shell --run 'cargo run --release -p rune-media --example processing'                     # costo por cuadro
```

En la app: Ajustes → Voz y video → **Comprobar**, activar **Escucharme** y
cambiar **Supresión de ruido** mientras se habla con un ventilador o
tecleando; las barras *Entrada* y *Procesada* muestran la diferencia.

## Implementado: códec, jitter, mezcla y medios de una llamada (2026-10-16)

Pista 17. Todo en `rune-media`, sin saber de ningún servicio; el transporte
de Discord está en `rune-discord-session` (ver
[DISCORD_SESSION.md](DISCORD_SESSION.md#voz-en-canales-de-servidor-2026-10-16)).
**Sólo fixtures: no se escuchó ni se hizo ninguna llamada real.**

- **`codec`**: Opus con el crate `opus =0.4.0` (libopus 1.6 incluido,
  compilado con `cmake`, que `shell.nix` agrega). 48 kHz **mono**, tramas de
  20 ms (960 muestras), modo VoIP, 64 kbit/s, FEC en banda y 10 % de
  pérdida esperada. Mono como Serein: la mitad de trabajo y se decodifica
  igual en receptores estéreo. Entradas no finitas → silencio, recorte a
  ±1. Decodificación a mono; una pérdida se reconstruye con el FEC del
  paquete siguiente si llegó, si no con PLC de libopus.
- **`jitter`**: uno por persona, 16 ranuras (320 ms) de hasta 1275 bytes.
  Arranca con 2 paquetes (40 ms), reordena, descarta duplicados y tardíos,
  reporta un hueco como perdido (con el paquete siguiente para FEC) hasta 3
  veces seguidas y después vuelve a almacenar; un salto de más de 16 empieza
  de nuevo. Retardo fijo, no adaptativo (atajo).
- **`mixer`**: hasta 64 fuentes, búfer fijo de 120 ms decodificados por
  fuente (paquetes de 2,5–120 ms, hasta 8 decodificaciones por tick), suma
  con volumen por persona (0–200 %) y silencio por persona elegidos por quien
  escucha (hasta 256 elecciones, sobreviven a que la persona salga y
  vuelva), recorte duro (no es un limitador ni AGC). «Habla» si el RMS
  decodificado supera ≈ −40 dBFS, con 300 ms de sostén.
- **`call::CallMedia`**: hilo `rune-call` cada 20 ms. Toma el micrófono
  procesado de un `Tap` (la captura nueva `Capture::set_transmit`, a 48 kHz;
  se descarta lo que espere más de 60 ms), lo deja pasar sólo si el host
  abre la compuerta (conectado, sin mute ni ensordecer, y la tecla de
  pulsar para hablar si se usa) y, en modo actividad de voz, sólo mientras
  `VoiceActivity` (con los umbrales de Ajustes) dice que se habla. Codifica
  y entrega cada trama al `send` del host. Lo recibido (hasta 256 paquetes
  por tick) va al mezclador y la mezcla al `Feed` de la salida, silencio
  incluido para que el ritmo no cambie. Ensordecido: lo recibido se tira y
  no suena nada. Soltar el handle detiene el hilo y libera tap, feed y
  enlaces (probado).

En `apps/desktop/src/audio.rs`: el proveedor entrega su `VoiceLink` una vez
(`Provider::take_voice_link`, nuevo, `None` por defecto: Mock sigue
simulando). Con una llamada `Connected` y enlace, la salida reproduce el
`Feed` de la llamada, la captura alimenta su `Tap`, y los anillos de
«habla» de los demás salen de la mezcla (`Store::set_speaking`); el propio,
como antes, del medidor. Mute suelta el micrófono (la captura se cierra);
ensordecer pone la salida en 0 y el hilo descarta lo recibido; ambos se
avisan además al servicio (op 4). Mientras dura una llamada real,
«Escucharme» no suena: la salida tiene un solo `Feed` y es de la llamada.
Volumen por persona: ver la sección siguiente.

Pruebas sin dispositivos: tono que sobrevive a Opus y pérdida
reconstruida; reordenamiento, pérdida con FEC, tardíos y duplicados, cotas
de ocultamiento y de memoria del jitter; suma, recorte, volumen, mute y
cotas del mezclador; compuerta, actividad de voz, pulsar para hablar,
retraso acotado del micrófono, reproducción y ensordecer del motor; el hilo
liberando todo al soltarlo; la captura entregando al `Tap` y soltando el
enlace cuando el lector se va.

### Prueba manual de una llamada real (para el dueño)

Nada de esto se probó todavía contra Discord. Con auriculares, en un
servidor propio, con una segunda cuenta o persona de confianza en un
cliente oficial:

1. `nix-shell --arg withLogin true --run 'cargo run --release -p rune-desktop --features session-login -- --provider discord-session'`
   (el login es el mismo de siempre; RUNE nunca pide la contraseña en otro lado).
2. Ajustes → Voz y video: elegir micrófono y altavoz, probar el micrófono y
   el sonido (sin «Escucharme» durante la llamada).
3. Entrar a un canal de voz desde la barra lateral. Esperado: «Conectando…»
   y luego «Voz conectada» en el panel; la otra persona ve a la cuenta en el
   canal. Si falla, el aviso «Se cortó la llamada» dice el motivo: anotarlo.
4. Que la otra persona entre o ya esté: hablar los dos. Esperado: se oyen
   en ambos sentidos, el anillo de quien habla se enciende, el código de
   privacidad del cliente oficial muestra la llamada cifrada.
5. Mute: la otra persona deja de oír y ve el icono de mute. Ensordecer:
   no se oye nada y el otro cliente lo muestra. Pulsar para hablar: sólo
   se oye con la tecla apretada.
6. Salir con el botón de colgar: la cuenta desaparece del canal.
7. Registrar el resultado (fecha, qué funcionó, motivos de error exactos,
   versión/commit) en `docs/DISCORD_SESSION.md` antes de afirmar que las
   llamadas funcionan.

---

## Implementado: vista previa local de pantalla y cámara (2026-10-16)

Solo Linux, y solo **local**: nada se transmite, graba ni guarda. La interfaz
lo dice («Solo vos lo ves. La transmisión llega con las llamadas.») porque
ningún proveedor declara `Capabilities::video`, y
`Abilities::send_video` lo niega con ese motivo. En Windows y macOS los
botones quedan deshabilitados con el motivo.

Flujo de pantalla (`rune-platform::screencast::ScreenCast`, `zbus` directo,
sin `ashpd`, como el FileChooser existente):

1. Conexión D-Bus propia por sesión; se fija el dueño único actual de
   `org.freedesktop.portal.Desktop` (activándolo si hace falta) y se vigila
   `NameOwnerChanged`: otro dueño o la pérdida del bus terminan la sesión.
2. `AvailableSourceTypes` (se piden monitor|ventana; sin ellos, «no
   disponible») y `AvailableCursorModes` (cursor incrustado si existe).
3. Se escucha `Session.Closed` **antes** de `CreateSession`; cada pedido
   escucha su `Request.Response` antes de llamar y exige que el handle
   devuelto sea el propio. `CreateSession` → `SelectSources` (`types`,
   `multiple=false`, `cursor_mode`; desde la versión 4 del portal también
   `persist_mode=2` y el `restore_token` de la vez anterior, validado con
   `valid_restore_token`)
   → `Start` → `streams` (`a(ua{sv})`, máximo 8, node ≠ 0) →
   `OpenPipeWireRemote` (el fd tiene que ser un socket).
4. Respuesta 1 = cancelado, 2 = denegado; cancelar desde RUNE, el plazo o la
   salida del portal cierran el `Request` y la `Session`. `ScreenCast::ended`
   devuelve cuando el sistema cierra la sesión; `close`/`Drop` la cierran.

Flujo de cámara: `IsCameraPresent` (falso → «No hay ninguna cámara
conectada», sin pedir acceso) → `AccessCamera` → `OpenPipeWireRemote`. Sin
portal o sin interfaz `Camera` el motivo es claro y **no** hay fallback a
`/dev/video*`. La cámara se libera con el fd del remote.

Frames (`rune-media::video`, crate `pipewire` 0.10.1, el mismo que ya enlaza
cpal; no suma bibliotecas):

- Cada fuente es un `Video` con su hilo: un runtime Tokio de un hilo para el
  portal, y otro hilo con el `MainLoop` de PipeWire sobre el fd
  (`connect_fd`), un stream de entrada al nodo elegido (pantalla) o con
  `target.object` = cámara elegida, `AUTOCONNECT | MAP_BUFFERS`.
- Formatos ofrecidos: BGRx/BGRA/RGBx/RGBA para pantalla; YUY2, NV12 y
  BGRx/RGBx para cámara (BT.601 rango limitado), y MJPG después
  (decodificado a RGBA, 2026-10-18). Sin DMA-BUF: no se ofrecen
  modificadores, así que el productor tiene que dar memoria mapeable.
- Se valida cada plano contra su `chunk` (offset, stride, tamaño) y lados de
  hasta 8192 px; se muestrea al vecino más cercano a ≤ 1280×720, a ≤ 15 fps
  (los demás buffers se devuelven sin convertir), RGBA opaco.
- Cola de 2 frames que descarta el más viejo; la UI toma solo el más nuevo.
  Memoria acotada a ~7,4 MB por fuente más el último frame que el host
  conserva para volver a presentarlo.
- Terminan la fuente: el handle soltado, `Session.Closed`, error de stream o
  del core, desconexión del stream, el nodo de pantalla o la cámara elegida
  (o la última) quitados del registro, 15 s sin primer frame, o una cámara 5 s
  sin frames. El primer motivo gana; la cola se vacía al terminar.

Host (`apps/desktop/src/video.rs`): sostiene una fuente por tipo mientras
`ui.video.wanted(store)` la pide (en llamada y activada, o «Probar video»),
la suelta si deja de pedirse, si cambia la cuenta (`store.me()`), si cambia
la cámara elegida, al terminar la llamada o fallar, y al cerrar la ventana.
Los frames se presentan con `limn_egui::present_surface` en
`video:screen`/`video:camera` (mismo mecanismo que las animaciones).

UI limn: en el panel de la llamada y en la barra del escenario, «Encender la
cámara»/«Compartir pantalla» se encienden (verde, `active`) y su tooltip
dice «Dejar de compartir»/«Apagar la cámara» y que no se transmite. En el
escenario, mosaicos «Tu pantalla»/«Tu cámara» antes de las personas, con
«Esperando al sistema…» hasta el primer frame y «Dejar de compartir» /
«Apagar la cámara». Ajustes → Voz y video → Video: cámara (lista que llena
el remote del portal), cuadro 16:9 y «Probar video»; cerrar los ajustes o
cambiar de página lo detiene. El selector de pantalla es el del sistema.

### Evidencia y límites (2026-10-16)

- `rune-platform`: 11 pruebas con un portal falso en un bus privado
  (`RUNE_TEST_DBUS_DAEMON`): éxito con nodo y fd que funciona, cancelación y
  denegación en el diálogo cierran la sesión sin remote, cancelar desde RUNE
  cierra `Request` y `Session`, `Session.Closed` se informa y no se vuelve a
  cerrar, `Drop` cierra, respuestas de otra conexión se ignoran, sin portal
  → no disponible; cámara ausente, denegada/cancelada, concedida y cancelada.
- `rune-media::video`: 10 pruebas: tamaños de vista previa, BGRx/RGBx con
  stride, YUY2/NV12, buffers que no alcanzan, cola acotada, denegación sin
  frames, soltar mientras el sistema pregunta cancela, `Closed` termina y
  libera, soltar una fuente concedida libera la sesión, un remote que cuelga
  termina la vista previa (PipeWire real sobre un socketpair, sin daemon).
- **No verificado:** ningún frame real. No se capturó la pantalla ni la
  cámara del dueño; no se probó la negociación con
  xdg-desktop-portal-hyprland ni con una cámara real. Pasos manuales en
  `tasks/todo.md` (sección 20).

## Implementado: captura local y reducción de ruido (2026-10-14)

`rune-media::Capture` reemplaza `InputMeter`. Crear el controlador no abre
dispositivos: `configure(Some(CaptureRequest { device, mode, muted }))` publica
el estado deseado y `configure(None)` detiene la captura. Una sola instancia
sirve la prueba de micrófono y el indicador propio de voz. El host comparte
esa captura; no hay un stream adicional por cada consumidor.

- Un hilo propio posee la apertura, `play`, cierre y procesamiento. La UI no
  espera a CPAL ni al cierre del hilo. `Drop` invalida la captura y señala el
  cierre; una apertura nativa ya en curso se descarta cuando retorna.
- El callback convierte muestras a float finito, limita a `[-1, 1]`, promedia
  los canales y escribe un ring SPSC de `rtrb`. No usa locks, asignaciones,
  logs, archivos, red ni DSP. El callback de error solo publica un atómico.
- El ring guarda como máximo 200 ms de audio mono: menos de 1 MiB con el
  máximo admitido, 192 kHz. Se aceptan de 1 a 8 canales y tasas entre 8 y
  192 kHz divisibles por 100. Las demás configuraciones fallan antes de
  reservar buffers; no se etiqueta una tasa aproximada como compatible.
- `CaptureProcessor` recibe cuadros exactos de 10 ms: 441 muestras a
  44,1 kHz, 480 a 48 kHz y 960 a 96 kHz. Los límites de callback no tienen
  que coincidir con esos cuadros. La salida conserva la tasa del dispositivo;
  Sonora hace sus conversiones internas. No se añadió `rubato`.
- El modo inicial es `Off`. `NoiseSuppression` usa Sonora 0.2.0, filtro
  pasaaltos y supresión moderada, con máximo interno de 48 kHz. AEC y AGC
  permanecen apagados. El ajuste se aplica en el trabajador y reinicia la
  historia del filtro; no modifica el volumen del sistema.
- Mute y stop cierran la admisión inmediatamente. Cada solicitud tiene una
  generación: se descartan cuadros, estados y errores obsoletos; al cambiar
  configuración se vacían el acumulador y la historia DSP. Un callback que
  ya estaba ejecutándose puede terminar, pero sus muestras conservan la
  generación anterior. El medidor devuelve cero mientras está silenciado.
- Si el ring se llena, se descartan muestras nuevas sin bloquear y se cuentan
  en `dropped_samples`. La secuencia identifica el hueco; el trabajador
  reinicia el cuadro parcial y el filtro para no empalmar audio discontinuo.
  Si pasan 250 ms sin un cuadro completo, también limpia los niveles y el
  cuadro parcial. El indicador no queda encendido por el último valor.
- `arc-swap` mantiene una única solicitud más reciente y una instantánea
  coherente, sin backlog de comandos ni mutex de UI. Una solicitud igual es
  un no-op. Reintentar exige `None` y luego la solicitud: un error no provoca
  un bucle de aperturas. Idle, error y mute no despiertan el hilo cada 4 ms.

`CaptureSnapshot` publica `Idle`, `Starting`, `Running` o `Failed`, niveles
crudo/procesado, muestras descartadas, cuadros completos y microsegundos
del último procesamiento. Los niveles usan la escala logarítmica del medidor,
no amplitud PCM. El tiempo medido **no** es latencia extremo a extremo.
`VoiceActivity` sigue siendo un indicador de nivel con histéresis; no es un
clasificador de habla ni una mejora acústica. Ahora rechaza silencio incluso
con umbral cero, normaliza entradas no finitas y permite `reset` explícito.

No se graba ni transmite audio. “Prueba de micrófono” requiere la acción del
usuario; las validaciones automatizadas no abren micrófonos ni salidas. La
reducción de ruido no habilita voz remota de ningún proveedor. Lo único que se
reproduce es local (tono de prueba y «Escucharme»); ver la sección siguiente.

En la interfaz: **Ajustes → Voz y video → Prueba de micrófono**. El control
**Reducción de ruido** cambia el procesamiento local mientras está abierta
la prueba. **Entrada** y **Procesada** muestran niveles.

### Pulsar para hablar (2026-10-16)

**Ajustes → Voz y video → Modo de entrada** elige *Actividad de voz* o
*Pulsar para hablar*. Con el segundo aparecen el atajo (botón **Grabar
atajo**: la próxima tecla pulsada en la ventana; Esc, perder el foco, salir
de la página o 10 s lo cancelan sin cambiar nada) y el **retraso al soltar**
(0–2000 ms, 20 ms por defecto como Discord). Se guardan en los ajustes
locales (`push_to_talk`, `push_to_talk_key`, `push_to_talk_release`).

`rune-media::PushToTalk` decide si el micrófono pasa: mientras la tecla está
sostenida y durante el retraso; `reset` lo corta sin retraso cuando la
captura se detiene. El host le pasa cada cuadro si la tecla está sostenida
*ahora*: en la ventana lo lee de las teclas que el árbol de limn guarda
sostenidas (`UiTree::key_held`, desde el 2026-10-19; antes, del estado de
teclas de egui), que se vacían al perder el foco, y fuera de ella del atajo global; una liberación perdida se
lee como liberada. Todavía no hay transporte: la decisión condiciona la
marca de «hablando» de la llamada y queda en `ui.audio.transmitting` para la
pista 17. La prueba de micrófono sigue mostrando la entrada sin condicionar
y, en este modo, dice si se está transmitiendo.

El atajo global usa `rune_platform::shortcuts::GlobalShortcuts` (portal
`org.freedesktop.portal.GlobalShortcuts`: `CreateSession`, `BindShortcuts`,
señales `Activated`/`Deactivated`/`ShortcutsChanged` y `Session.Closed`) en un
hilo propio. Sólo se pide para combinaciones con Ctrl/Alt/Shift o teclas F,
para no capturar una letra en todo el escritorio. Una sesión por cuenta y
atajo: desactivar el modo, cambiar el atajo o la cuenta, o cerrar la
ventana la cierra (`Session.Close`). Sin portal, sin la interfaz, con el
pedido rechazado o con la sesión cerrada por el escritorio, la tecla queda
liberada y los ajustes dicen por qué; dentro de la ventana sigue
funcionando. Probado con un portal falso en un bus privado; **no** probado
contra un compositor real (KDE, GNOME 48+, Hyprland), cuyo diálogo puede
asignar otro disparador.

Desde el 2026-10-17 (P6, rama `escritorio`) la misma sesión lleva también
los atajos de **silenciar**, **ensordecer** y **pulsar para silenciar**
(Ajustes › Sistema › Atajos de teclado › Voz desde el 2026-10-19, en la
tabla común de atajos con su aviso de conflictos), como Serein: un solo
`BindShortcuts` con todos, ids
`push-to-talk`, `mute`, `deafen` y `ptm`. Silenciar y ensordecer son los
mismos clics de la interfaz (`me:mute`, `me:deafen`) una vez por
`Activated`; con la ventana enfocada actúan por las pulsaciones que limn
informa (una por pulsación, no por la repetición) sólo si el escritorio no los asignó, así no se aplican dos veces. Mientras
se mantiene «pulsar para silenciar» `ui.audio.transmitting` queda en falso,
en cualquier modo de entrada. Ver [PLATFORM.md](PLATFORM.md#atajos-del-micrófono-y-el-sonido).

### Código de privacidad DAVE (2026-10-17)

La conexión de la llamada (`voice/connection.rs`) informa por un `watch` el
código de privacidad de `davey` (`voice_privacy_code`: 30 dígitos del
autenticador de la época MLS, en grupos de cinco) cada vez que un cambio de
grupo lo cambia, y sólo mientras la media va cifrada; el proveedor lo emite
como `Event::CallPrivacyCode`. El código de verificación con una persona
(`Command::VoiceVerificationCode` → `Event::VoiceVerification`) es el de
libdave: scrypt (N = 2¹⁴, r = 8, p = 2) sobre las dos huellas ordenadas, 45
dígitos; la conexión toma las huellas y lo calcula en `spawn_blocking`, para
no cortar el audio (decenas de milisegundos). Un grupo nuevo borra los
códigos pedidos. Serein muestra sólo el de la llamada, sin espacios. Probado
con el par MLS sintético de las pruebas de voz (Alice y Bob ven el mismo
código y el mismo de verificación); **no** con una llamada real.

### Evidencia y límites

`nix-shell --arg withLogin true --run 'cargo test -p rune-media'`: **23 pruebas
pasaron**. Diez regresiones de captura usan entrada sintética y un backend
falso: downmix y valores no finitos, cuadros entre callbacks, límites,
overflow, mute/generaciones, discontinuidades, niveles caducados, aperturas
lentas, solicitudes repetidas y errores/reintento. El caso de apertura lenta
detectó y corrigió un despertar perdido: una llamada nativa puede consumir
el token de `unpark`; el trabajador revisa de nuevo la generación antes de
dormir. Nueve pruebas verifican DSP, formatos, estado y atenuación de ruido
estacionario generado; cuatro cubren métricas e histéresis.

REA volvió a ejecutar el binario de estas 23 pruebas con
`--test-threads=1`: **23 pasaron, exit 0**. Evidencia
`ev_b8cc758bef296ca6f088a15721c3e6d5edb034064047ec1f4db40e06ed5c412b`,
SHA-256 previo al arranque
`50147f821c82e84bf612982c5de3d4ba1c00e410a1c7b7e1439752ec1e0c2d22`.
El [registro](evidence/rea-audio-tests-2026-10-14.json) conserva el comando,
salida PTY completa y límites: algunos cuadros de terminal renderizada se
omitieron por presupuesto; no hubo observación de red ni rutas de filesystem.
Es un replay controlado de fixtures, no evidencia de calidad acústica real.
La identidad del ejecutable es una muestra de hash previa al arranque;
REA no garantiza una vinculación atómica con la imagen del sistema operativo.

El ejemplo `cargo run --release -p rune-media --example processing` mide
señales sintéticas sin E/S de audio. El ejemplo `devices -- --meter
--denoise` sí abre el micrófono por solicitud explícita y no se ejecutó en
esta validación. No se infiere calidad de voz humana de la atenuación de
ruido sintético: falta una comparación auditiva con fixtures de habla de
licencia verificada, prueba en dispositivos reales y medición de latencia
de captura. AGC, Opus y transporte siguen sin implementar; la salida y AEC
están en la sección siguiente.

### Fuentes y dependencias

La API de [Sonora](https://docs.rs/sonora/latest/sonora/struct.AudioProcessing.html)
documenta PCM float normalizado, cuadros de ~10 ms, formatos de entrada y
salida explícitos y referencia de reproducción para AEC. La
[configuración](https://raw.githubusercontent.com/dignifiedquire/sonora/main/crates/sonora/src/config.rs)
deshabilita los módulos por defecto y usa un máximo interno de 32 kHz;
RUNE elige 48 kHz expresamente. Su
[buffer](https://raw.githubusercontent.com/dignifiedquire/sonora/main/crates/sonora/src/audio_buffer.rs)
incorpora remuestreo sinc. Se comprobó también el código del crate publicado
0.2.0, cuyo MSRV 1.91 es compatible con el workspace 1.95.

[nnnoiseless](https://raw.githubusercontent.com/jneem/nnnoiseless/main/src/denoise.rs)
0.5.2 queda como alternativa para una comparación posterior: su contrato es
48 kHz, 480 muestras y float en escala PCM16, con historia y transitorio
inicial. No es un reemplazo intercambiable sin adaptación. El
[RNNoise actual](https://github.com/xiph/rnnoise) tiene otros pesos y un formato
de modelos que cambió; no se presupone equivalencia con el port. Ninguno de
los dos se añadió al árbol de dependencias de RUNE.

[rtrb 0.4.0](https://docs.rs/rtrb/latest/rtrb/) aporta la cola SPSC acotada;
[arc-swap 1.9.2](https://docs.rs/arc-swap/latest/arc_swap/) publica solicitudes
y snapshots sin mutex. Se revisaron los manifests y textos de licencia de
los paquetes descargados por Cargo. Sonora y sus subcrates declaran
BSD-3-Clause; rtrb y arc-swap permiten MIT o Apache-2.0. CPAL 0.18.2 continúa
con Apache-2.0. Los avisos de esta incorporación se conservan en
[AUDIO-NOTICES](../assets/licenses/AUDIO-NOTICES.md); no se copió código de
Serein. Se recorrió el cierre del lockfile desde las tres dependencias nuevas:
30 pares paquete/versión, con textos conservados y la licencia Unicode-3.0
adicional de `unicode-ident`. Esto no es una auditoría del workspace entero
ni de las bibliotecas nativas de CPAL. Los crates AEC/AGC que forman parte de Sonora se compilan aunque sus
algoritmos estén deshabilitados en RUNE.

## Implementado: salida local, prueba audible y referencia de eco (2026-10-16)

`rune-media::Playback` es el controlador de salida, con la forma de
`Capture`: crearlo no abre dispositivos; `configure(Some(PlaybackRequest {
device, source, volume, deafened }))` publica lo deseado y `configure(None)`
detiene. Un hilo propio (`rune-playback`) posee el `cpal::Stream`.

- **Formato interno:** todo lo que suena es mono a 48 kHz (`PLAYBACK_RATE`).
  Al abrir se prefiere una configuración de 48 kHz del dispositivo (f32 y la
  menor cantidad de canales); si no hay, la predeterminada, admitida entre
  8 y 192 kHz y de 1 a 8 canales. Se pide un período fijo de 20 ms cuando el
  dispositivo lo permite. El callback copia la muestra mono a todos los
  canales y convierte al formato del dispositivo.
- **Remuestreo acotado:** interpolación lineal en streaming (`Linear`), sin
  buffers: cada muestra de salida consume a lo sumo `1 + ⌈entrada/salida⌉`
  muestras (≤ 7 con las tasas admitidas). Es un atajo marcado en el código:
  si un dispositivo sin 48 kHz suena mal, va un remuestreador de banda
  limitada. A 48 kHz no interpola (pasa con una muestra de retardo).
- **Buffers y política:** el callback sólo hace `pop` de un ring SPSC
  (`rtrb`) de 200 ms, aplica la ganancia y escribe; sin locks, logs ni
  asignaciones (una prueba lo verifica con un asignador contador). El
  trabajador lo mantiene en 40 ms, o dos períodos del dispositivo si son
  más largos. **Underrun:** silencio, contado en `underruns` sólo mientras
  hay una fuente activa. **Overrun:** un `Feed` con más de 120 ms en cola
  pierde lo más viejo hasta quedar en 40 ms, y lo que no entra en sus 200 ms
  se descarta; ambos se cuentan en `overruns`.
- **Volumen y ensordecer:** un atómico con la ganancia (0–200 %, cero si
  está ensordecido) que lee cada callback: cambian sin reabrir nada.
  Dispositivo o fuente nuevos pasan por el trabajador y reconstruyen el stream.
- **Cambio y pérdida de dispositivo:** elegir otra salida reconstruye el
  stream. Con «Predeterminado del sistema» el trabajador revisa cada segundo
  si el predeterminado cambió y se muda. Un error del stream (que no sea un
  xrun) reintenta cada 500 ms, hasta 10 veces, y luego `Failed`. Un
  dispositivo que no existe al abrir falla de una; no hay bucle de aperturas.
- **Fuentes:** `Tone`, un timbre suave generado en el trabajador (660 Hz y
  880 Hz, 200 ms cada uno con fundidos de 5 ms, luego 600 ms de silencio,
  ≈ −12 dBFS de pico); y `Feed`, lo que empuje el último `Playback::feed()`
  desde un hilo no RT. «Escucharme» conecta el micrófono procesado a ese
  `Feed` (la captura remuestrea su tasa a 48 kHz en su trabajador); las
  llamadas (pista 17) pueden usar la misma entrada para el audio remoto.
- **Latencia medida:** `PlaybackSnapshot` publica la latencia del
  dispositivo que predice el sistema de audio (`playback − callback` de los
  timestamps de cpal) y la cola en ese momento, además de tasa, canales y
  contadores. No es latencia extremo a extremo ni acústica.

**Referencia de eco.** El callback copia a un ring de referencia sólo lo que
realmente sonó: muestras del ring (no el silencio de un underrun), con la
ganancia aplicada, y nada mientras está ensordecido o al 0 %. El trabajador lo
pasa a la `EchoReference` que tiene la captura (`Capture::set_echo_reference`).
Con `CaptureRequest::echo_cancellation`, el trabajador de captura pasa a
Sonora un cuadro de 10 ms de referencia (`process_render_f32`, 48 kHz mono)
antes de cada cuadro de 10 ms del micrófono y usa la latencia de salida como
`set_stream_delay_ms`. Lo que espera más de 80 ms se descarta (contado en
`reference_dropped`): la desalineación que tiene que absorber el estimador de
retardo de AEC3 queda acotada. AEC3 se enciende recién cuando llega
referencia, sigue con silencio en pausas cortas y se apaga tras 200 ms sin
nada; sin referencia o sin el ajuste no hay AEC.
`CaptureSnapshot::echo_cancelling` dice si el último cuadro pasó por él.

En la interfaz, **Ajustes → Voz y video**: selector de **Altavoz** (el mismo
valor que el menú del panel de usuario), **Volumen de salida** (0–200 %,
guardado), **Probar sonido**/**Detener** para el timbre, **Escucharme**
durante la prueba de micrófono (con el aviso de usar auriculares) y
**Cancelación de eco**, apagada hasta activarla, como la reducción de ruido.
Salir de la página o cerrar Ajustes detiene ambas pruebas; si la salida
falla, se detienen y aparece un aviso. Ensordecerse silencia la prueba y la
página lo dice. El audio de las llamadas todavía no se reproduce.

### Evidencia y límites

Las pruebas no abren dispositivos: el callback se ejercita con rings
sintéticos y un backend falso le entrega su estado a la prueba. Cubren
remuestreo (tasa y cota de muestras por salida), ganancia y canales sin
asignar, ensordecer/volumen 0 sin referencia, underrun contado y sin
referencia, overrun que conserva lo más nuevo, objetivo de cola, el timbre,
reconstrucción por cambio y pérdida, mudanza del predeterminado, fallo sin
bucle y el cruce de `Feed`/`EchoReference`. En captura: AEC sólo con
referencia y con el ajuste, apagado tras la pausa, recorte de referencia
vieja y «Escucharme» sólo cuando se pide. Con eco sintético (ruido a la
mitad de nivel y 10 ms de retardo) Sonora dejó el RMS en ≈ 0,2 % tras 5 s
de adaptación: prueba el cableado, **no** la calidad con altavoces,
micrófonos ni habitaciones reales.

**No se escuchó nada en esta entrega.** Para la prueba manual:

```sh
nix-shell --run 'cargo run -p rune-media --example devices'          # ids de salida
nix-shell --run 'cargo run -p rune-media --example playback'         # timbre 5 s en la salida predeterminada
nix-shell --run 'cargo run -p rune-media --example playback -- --device ID --volume 0.5 --seconds 10'
nix-shell --run 'cargo run -p rune-media --example playback -- --deafen-after 2'  # a los 2 s se calla
nix-shell --run 'cargo run -p rune-media --example playback -- --loopback --denoise --echo'  # con auriculares
nix-shell --run 'cargo run -p rune-desktop'  # Ajustes → Voz y video → Probar sonido / Escucharme
```

El ejemplo imprime cada 250 ms estado, tasa y canales, latencia del
dispositivo, cola, underruns y overruns (y con `--loopback`, nivel del
micrófono y si AEC está activo). Qué escuchar: timbre limpio sin clics ni
cortes, volumen proporcional, silencio inmediato al ensordecer, que cambiar
de salida en la ventana mueva el sonido y que desconectar unos auriculares no
deje la salida trabada. Pendiente: la escucha del dueño, AEC con altavoces
de portátil, latencia de captura e ida y vuelta.

---

## 0. Resumen de decisiones

La siguiente tabla conserva las decisiones de diseño por fase. El estado
real está detallado arriba: la escucha propia (loopback, «Escucharme» y la
prueba de micrófono), la reproducción (`rune_media::playback`), el
transporte de voz de Discord y el video (enviar y mirar, [VIDEO.md](VIDEO.md))
ya existen; lo del proveedor no oficial está probado con fixtures y en
llamadas reales del dueño, sin una prueba registrada de cada caso.

| Área | Decisión | Versión fijada | Por qué |
|---|---|---|---|
| E/S de audio | `cpal` con feature `pipewire` en Linux | `=0.18.2` | Ya trae host PipeWire nativo (desde 0.18.0) y en Linux lo prioriza sobre PulseAudio y ALSA. Cubre WASAPI y CoreAudio para las fases multiplataforma. Se compiló y probó en esta máquina. |
| Buffers de tiempo real | `rtrb` (SPSC sin bloqueos y wait-free) | `=0.4.0` | No asigna memoria ni toma locks en el callback. No tiene dependencias. |
| Medidor y mute actuales | Worker de captura, RMS, ring SPSC y snapshot coherente con `arc-swap` | `1.9.2` | La UI no espera a apertura/cierre/DSP; la escucha propia va por `Capture::set_monitor` a la reproducción. |
| Remuestreo externo (futuro, si hace falta) | Evaluar `rubato` | `5.0.1` | El DSP actual conserva la tasa de captura y Sonora realiza su remuestreo interno; la reproducción (`rune_media::playback`, mono a 48 kHz) convierte tasa y canales en su callback, sin `rubato`. |
| Opus (fase 2) | `opus` (que usa `opusic-sys` con libopus 1.6.1 incluido) | `=0.4.0` | Lo mantiene el autor original y salió en 2026-08. libopus se compila desde la fuente, así que Windows y macOS no necesitan la librería del sistema. |
| AEC / NS / AGC (fase 2+) | `sonora` (port puro Rust del APM de WebRTC) y, opcionalmente, `nnnoiseless` | `0.2.0` / `0.5.2` | BSD-3, sin C++ ni dependencias nativas. |
| Transporte | Trait propio `VoiceTransport`. Implementaciones: loopback (fase 2), Discord (UDP/RTP/AEAD + DAVE con `davey`) y quizá WebRTC (`str0m`) | `davey 0.1.5` | La voz de bots de Discord **no es WebRTC**. Usa su propio protocolo RTP/UDP. |
| Pantalla y cámara en Wayland (fase 3) | Portales ScreenCast y Camera con el `zbus` que ya usa `rune-platform` (no se adoptó `ashpd`), más el crate `pipewire` directo | `zbus =5.19.0` / `pipewire 0.10.1` | Es la única vía soportada en Wayland o en un sandbox. Implementado para vista previa local (2026-10-16); ashpd no aportaba nada que el FileChooser propio no tuviera ya, y su feature `pipewire` choca con pipewire 0.10 (§2.5). |
| GStreamer | Sólo para el video por GPU (2026-10-17) | `0.25.4` (`gstreamer`, `-video`), `0.25.2` (`-app`) | Es LGPL y pesa en runtime (Win/mac), así que no reemplaza a `cpal`, `pipewire` ni `opus`; codifica y decodifica H264 con VA-API y NVENC/NVDEC, con OpenH264 de respaldo ([VIDEO.md](VIDEO.md#video-por-hardware-2026-10-17-rama-video-por-hardware)). |

---

## 1. Estado de esta máquina (verificado 2026-10-14)

| Herramienta / recurso | Resultado |
|---|---|
| `nix --version` | Nix 2.34.7. `nixpkgs` (registry `flake:nixpkgs`) apunta a nixos-unstable, `lib.version` = 26.11 |
| `cargo` / `rustc` | 1.95.0 vía rustup (`~/.cargo/bin`), host `x86_64-unknown-linux-gnu` |
| `pkg-config` | **No está en el PATH**. Solo hay un `pkgconf` 2.5.1 en el PATH heredado, `PKG_CONFIG_PATH` está vacío y `pkgconf --list-all` devuelve 0 módulos |
| `pkg-config --list-all \| grep -iE 'alsa\|pipewire\|opus\|gstreamer'` | Sin resultados fuera de un shell de Nix |
| Compilador C (`cc`) | **No está en el PATH**. `cargo build` de `cpal` falla con ``linker `cc` not found`` |
| `pw-cli --version` | Compilado y enlazado con libpipewire **1.6.5** |
| Salidas `-dev` en `/nix/store` | Hay `alsa-lib-1.2.15.3-dev`, `pipewire-1.6.5-dev` y `libopus-1.6.1-dev`, pero no están expuestas a cargo. No hay salida `-dev` de GStreamer |
| Portales en ejecución | `xdg-desktop-portal` 1.20.4 con backends hyprland 1.3.12, gtk y kde. `XDG_CURRENT_DESKTOP=Hyprland` |
| Portal ScreenCast | `version` = **5**, `AvailableSourceTypes` = 7 (monitor, ventana y virtual), `AvailableCursorModes` = 3 (oculto y embebido; **sin** modo metadata) |
| Portal Camera | `version` = 1, `IsCameraPresent` = true. Existen `/dev/video0` a `/dev/video3` |
| Servidor de audio | PipeWire 1.6.5 con WirePlumber |

### 1.1 Prueba de compilación con cargo (shell efímero; no se instaló nada en el sistema)

Comando: `nix-shell -p pkg-config alsa-lib pipewire cmake rustPlatform.bindgenHook`. El proyecto de prueba vive en el scratchpad, fuera del repo.

| Prueba | Resultado |
|---|---|
| `cpal =0.18.2` (por defecto, ALSA) con `pkg-config` y `alsa-lib` | Compila. Al ejecutar, el binario falla con **`libasound.so.2: cannot open shared object file`** (ver §1.2) |
| Lo mismo con `LD_LIBRARY_PATH` | Funciona. `available_hosts = [Alsa]` y lista muchas PCM de ALSA duplicadas (hw, plughw, HDMI…), lo que es ruidoso para una UI |
| `cpal =0.18.2` con `features=["pipewire"]`, `opus =0.4.0` y `rtrb =0.4.0` | Compila. libopus se construye con cmake. `available_hosts = [PipeWire, Alsa]` y el host por defecto es **PipeWire**. Lista nodos legibles como `default_input`, `default_output` y "Ryzen HD Audio Controller Analog Stereo" |
| `ashpd =0.13.13` con `features=["screencast","camera","pipewire"]` junto a `cpal` con `pipewire` | **Falla la resolución**: `pipewire-sys` 0.9.2 y 0.10.1 declaran ambos `links = "pipewire-0.3"` |
| `ashpd =0.13.13` con `["screencast","camera"]` (sin `pipewire`) | Resuelve bien. Usa `zbus` 5.19 puro Rust y no necesita libdbus |

Requisitos de compilación observados:

- `alsa-sys` necesita `pkg-config` y `alsa-lib`. Sin bindgen basta.
- `pipewire-sys` y `libspa-sys` usan **bindgen** y `system-deps`, así que necesitan libclang. En Nix se resuelve con `rustPlatform.bindgenHook`.
- `opusic-sys` necesita `cmake` y un compilador C.

### 1.2 Trampa de NixOS + rustup ≥ 1.90: RUNPATH vacío

Desde Rust 1.90, `x86_64-unknown-linux-gnu` enlaza con **rust-lld** por defecto. En esta máquina, el binario enlazado así solo llevaba `RUNPATH=[…-shell/lib]`, sin alsa-lib ni pipewire, y por eso no encontraba `libasound.so.2`.

Recompilado con `RUSTFLAGS="-C linker-features=-lld"`, el RUNPATH incluye `alsa-lib-1.2.15.3/lib`, `pipewire-1.6.5/lib`, glibc y gcc-lib, y el binario arranca sin `LD_LIBRARY_PATH` (verificado).

Hipótesis, **no verificada**: con lld se salta el `ld-wrapper` de Nix, que es el que inyecta el rpath.

El devShell tiene que hacer una de dos cosas:

- Exportar `LD_LIBRARY_PATH = lib.makeLibraryPath [ alsa-lib pipewire ]`, o
- Fijar `-C linker-features=-lld` en `.cargo/config.toml` o en `RUSTFLAGS`.

Para el paquete final (`rustPlatform.buildRustPackage` con el rustc de nixpkgs), comprobar el RUNPATH con `readelf -d` **(no verificado)**.

**Resuelto en la segunda iteración** con una tercera opción: `shell.nix` fija `RUSTFLAGS = "-C link-arg=-Wl,-rpath,…"` con las bibliotecas que RUNE enlaza (alsa-lib, pipewire) y las que winit/wgpu abren en ejecución (wayland, libxkbcommon, vulkan-loader, libGL). Así se mantiene rust-lld y el binario de release arranca desde cualquier shell (verificado con `readelf -d` y ejecutándolo fuera de `nix-shell`; ver [REA.md](REA.md#segunda-iteración-arranque-del-binario)).

---

## 2. Evaluación por área

### 2.1 E/S de audio: `cpal`

| Campo | Valor |
|---|---|
| Versión | **0.18.2** (2026-08-16). Hay 0.18.1 (2026-06-07) y 0.18.0 (2026-06-06) |
| Licencia | Apache-2.0 |
| MSRV | 1.85 |
| Mantenimiento | Activo (RustAudio). Unos 8,1 M de descargas recientes. El `CHANGELOG` tiene una sección *Unreleased* grande |
| Plataformas | Linux/BSD (ALSA siempre, más PipeWire, PulseAudio y JACK como features), Windows (WASAPI, más ASIO como feature), macOS/iOS (CoreAudio), Android (AAudio) y Web (WebAudio/AudioWorklet) |
| Wayland | No aplica: el audio no depende del servidor gráfico |

**Hosts en Linux según el changelog:**

| Host | Feature | Desde | Dependencia nativa (NixOS) | Comentario |
|---|---|---|---|---|
| ALSA | siempre (`alsa` 0.11, `alsa-sys` 0.4) | histórico | `alsa-lib` (LGPL-2.1+) + `pkg-config` | Fallback universal. Enumera muchas PCM duplicadas |
| **PipeWire** | `pipewire` (`pipewire` 0.10) | **0.18.0** ("New host for Linux and BSD via the native PipeWire API") | `pipewire` (MIT) + `pkg-config` + libclang (bindgen) | Desde 0.18.0, "Default host priority: PipeWire > PulseAudio > ALSA". En 0.18.2 trae muchas correcciones de PipeWire: xruns, nombres por puerto, evitar que la promoción RT bloquee el callback, etc. |
| PulseAudio | `pulseaudio` (crate `pulseaudio` 0.3.1, cliente del protocolo en puro Rust) | **0.18.0** | ninguna | Alternativa sin dependencias nativas. Funciona contra `pipewire-pulse` |
| JACK | `jack` (crate `jack` 0.13.5) | histórico | `libjack2` (o `pipewire.jack`) | No hace falta para un cliente de chat |

**Ojo con la API que viene.** La sección *Unreleased* (futura 0.19) **rompe la API**: `play` pasa a llamarse `start`, `Input/OutputCallbackInfo` se unifican en `CallbackInfo`, llegan streams *duplex* y `StreamTrait: Send + Sync`. Por eso se fija `=0.18.2`.

Otros datos de 0.18:

- `realtime` por sí solo no hace nada en Linux sin `realtime-dbus`, que arrastra libdbus. El host PipeWire hace su propia promoción RT vía rtkit.
- Los dispositivos tienen un `DeviceId` estable, útil para recordar la selección del usuario.

**Veredicto:** adoptar `cpal =0.18.2` con `features = ["pipewire"]` en Linux. `pulseaudio` es un plan B sin dependencias nativas.

### 2.2 Opus

libopus es **BSD-3-Clause** (nixpkgs `libopus.meta.license`) y tiene licencias de patente libres de regalías, otorgadas automáticamente (opus-codec.org/license).

| Crate | Versión (fecha) | Licencia | Backend | Deps nativas (NixOS) | Mantenimiento | Veredicto |
|---|---|---|---|---|---|---|
| **`opus`** (SpaceManiac) | **0.4.0** (2026-08-23) | MIT/Apache-2.0 | `opusic-sys` 0.7.5 con features por defecto, es decir **libopus 1.6.1 incluido y compilado con cmake** | `cmake` + compilador C (no necesita `libopus`) | Autor original; reactivado en 2026. 1,6 M de descargas recientes | **Adoptar en la fase 2** |
| `opus2` (fork) | 0.4.0 (2026-03-26) | MIT/Apache-2.0 | `libopus_sys` (static, dynamic o bundled) o `mousiki` en puro Rust | `libopus` + `pkg-config`, o `cmake` | Un solo mantenedor y 2 estrellas, pero lo usa `songbird` 0.6 | Alternativa. **Si algún día se usa `songbird`, elegir `opus2`** para no enlazar dos libopus estáticas. El riesgo de símbolos duplicados es **(no verificado)** |
| `audiopus` / `audiopus_sys` | 0.2.0 (2019) / 0.3.0-rc.0 (2021) | ISC | libopus | `libopus` o cmake | **Sin mantenimiento**. Songbird lo abandonó en 0.6.0 porque "doesn't build anymore on some platforms" | Descartar |
| `opus-rs` (restsend) | 0.1.37 (2026-10-07) | BSD-3-Clause | **Puro Rust**, port de libopus 1.6, `no_std` | ninguna | Muy activo, pero joven: su README enumera arreglos recientes de estéreo SILK/Hybrid y de VBR | Vigilar. No usarlo todavía para interoperar con Discord |
| `mousiki` | 0.2.1 (2026-03-26) | MIT | Puro Rust | ninguna | Unas 1,1 k descargas. El repo `cijiugechu/mousiki` devuelve 404 **(no verificado)** | Descartar por ahora |
| `unsafe-libopus` | 0.2.0 (2024) | BSD-3-Clause | Transpilado con c2rust | ninguna | Estancado | Descartar |

### 2.3 WebRTC en Rust

Contexto: **Discord no usa WebRTC para bots** (ver §2.10). WebRTC solo importa si RUNE tiene llamadas propias (P2P o con SFU propio).

| Crate | Versión (fecha) | Licencia | Tipo | Deps nativas | Madurez | Comentario |
|---|---|---|---|---|---|---|
| `webrtc` (webrtc-rs) | 0.21.1 (2026-10-11) | MIT/Apache-2.0 | Capa async sobre el núcleo sans-IO `rtc` (reescrito en 2026) | ninguna (crypto `ring` o `aws-lc-rs`) | Pre-1.0. La línea 0.21 es "the run-up to 1.0", así que habrá cambios de API | Puro Rust y async. API tipo `RTCPeerConnection` |
| **`str0m`** | 0.24.1 (2026-10-03) | MIT/Apache-2.0 | **Sans-IO**, sin hilos internos | por defecto `aws-lc-rs` (compila C; puede necesitar cmake/clang, **no verificado**). Existe `rust-crypto`, en puro Rust | En producción como **SFU** (Lookback). "P2P … has received less testing" | **Preferido** si RUNE hace WebRTC: el modelo sans-IO encaja con el trait `VoiceTransport` |
| `libwebrtc` (LiveKit) | 0.3.52 (2026-10-12) | Apache-2.0 (bindings); libwebrtc de Google es BSD | Bindings C++ (cxx) a libwebrtc | **Descarga binarios precompilados** (`WEBRTC_TAG = webrtc-89d790b`) desde GitHub Releases durante el build, salvo que se defina `LK_CUSTOM_WEBRTC`. Pide **clang ≥ 21**, glib y libjpeg | La más completa (APM, codecs y HW). Pesada | Incompatible con el sandbox de Nix sin pre-descargar el zip y apuntar `LK_CUSTOM_WEBRTC`. Solo tiene sentido si se adopta LiveKit |
| `livekit` | 0.9.5 (2026-10-12) | Apache-2.0 | SDK cliente de LiveKit sobre `libwebrtc` | idem | Activo (162 issues abiertas) | Solo si se usa un servidor LiveKit |

### 2.4 PipeWire: crate `pipewire`

| Campo | Valor |
|---|---|
| Versión | **0.10.1** (2026-08-19), igual que `libspa` 0.10.1 |
| Licencia | MIT. PipeWire en sí es MIT, salvo el plugin `libspa-alsa` (LGPL) y `libjackserver` (GPL2), que RUNE no enlaza |
| MSRV | 1.80 |
| Mantenimiento | Oficial, en `gitlab.freedesktop.org/pipewire/pipewire-rs`. 1,5 M de descargas recientes |
| Deps nativas | `pipewire` (`libpipewire-0.3` y `libspa-0.2` vía `system-deps`) y **libclang** para bindgen. En NixOS: `pipewire`, `pkg-config` y `rustPlatform.bindgenHook` |
| Uso en RUNE | Fase 1, indirecto vía `cpal`. Fase 3, directo: consumir los nodos de video que entregan los portales con `pw_context_connect_fd` y `PW_KEY_TARGET_OBJECT` |
| Wayland | Es *la* vía de captura en Wayland, junto con el portal |

Restricción: **solo puede existir una versión de `pipewire-sys`** en el grafo de dependencias (`links = "pipewire-0.3"`). Todas las dependencias tienen que estar en la serie 0.10: `cpal` 0.18.2 y `xcap` 0.9.8 lo están; `ashpd` 0.13.13 con su feature `pipewire` y `scap` no.

### 2.5 XDG Desktop Portal: `ashpd`

| Campo | Valor |
|---|---|
| Versión | **0.13.13** (2026-07-17); repo con actividad en 2026-09-29 |
| Licencia | MIT |
| MSRV | 1.87 |
| Transporte D-Bus | `zbus` 5 (puro Rust): **no necesita libdbus**. Runtime `tokio` por defecto, con opción `async-io` |
| Features relevantes | `screencast`, `camera`, `remote_desktop`, `pipewire` (helper para enumerar las streams de la cámara), `wayland` y `raw_handle` (para el `WindowIdentifier` del diálogo padre) |
| Portal ScreenCast | Versión 6 en la especificación. **Esta máquina expone la v5**, así que no se puede depender de `pipewire-serial`, que es de la v6. Flujo: `CreateSession` → `SelectSources` (types, multiple, cursor_mode, `persist_mode` y `restore_token` desde la v4) → `Start` (devuelve streams con node id, size y source_type) → `OpenPipeWireRemote` (fd) |
| Portal Camera | `AccessCamera` → `OpenPipeWireRemote` (fd) y la propiedad `IsCameraPresent`. La especificación no dice cómo identificar los nodos de cámara dentro del remote (en la práctica es por propiedades del nodo; **no verificado**) |
| **Conflicto** | La feature `pipewire` de ashpd 0.13.13 depende de **`pipewire ^0.9`** y choca con `pipewire 0.10`, que pide `cpal` con `pipewire` (se reprodujo con `cargo generate-lockfile`). **Solución:** no activar esa feature; tomar el fd y el node id de ashpd y abrir la stream con `pipewire` 0.10 directamente |

### 2.6 GStreamer como pipeline alternativo

| Campo | Valor |
|---|---|
| Versión | `gstreamer` **0.25.4** (2026-09-21), `gstreamer-app` 0.25.2, `gstreamer-video` 0.25.4 |
| MSRV | 1.92 |
| Licencias | Bindings MIT/Apache-2.0. **GStreamer core y sus librerías son LGPL-2.1+**. Los plugins oficiales son LGPL; los que tienen problemas de patentes van en `-ugly` |
| Deps nativas (NixOS) | `gst_all_1.gstreamer`, `gst_all_1.gst-plugins-base`, `gst_all_1.gst-plugins-good` (versión 1.26.11) y, opcionalmente, `-bad` o `gst-libav`. `pipewiresrc` lo aporta el paquete `pipewire` **(no verificado en nixpkgs)** |
| Ventajas | Pipelines listos (`pipewiresrc ! videoconvert ! x264enc/vaapi…`) y codificación por hardware |
| Costes | Hay que distribuir el runtime de GStreamer en Windows y macOS **(no verificado su tamaño)**. Es otra capa de hilos y bus. Duplica lo que ya cubren `cpal`, `pipewire` y `opus` |
| Implicación LGPL | Enlace **dinámico**: compatible con cualquier licencia de RUNE, siempre que se cumplan los avisos y se permita sustituir la librería. Enlace **estático**: obliga a dar los objetos para re-enlazar. Para no complicarse, solo enlace dinámico |
| Veredicto | **No adoptar en las fases 1 a 3.** Reevaluar si se necesita codificar video H.264/AV1 por hardware para enviar pantalla |

### 2.7 Captura de pantalla multiplataforma

| Crate | Versión (fecha) | Licencia | Plataformas | Historia en Wayland | Deps nativas Linux | Estado | Veredicto |
|---|---|---|---|---|---|---|---|
| `xcap` | 0.9.8 (2026-08-01) | Apache-2.0 | Linux X11/Wayland, macOS, Windows | El README marca Wayland con ⛔ ("available, but not fully supported in some special scenarios"). Mezcla portal (`zbus`), PipeWire 0.10 y `libwayshot` (wlr-screencopy con gbm/drm/EGL). La grabación por ventana está "to be developed" | `libxcb`, `pipewire`, `mesa`/`libgbm`, `libdrm`, EGL y libclang | Activo | Útil para **capturas puntuales**, no para streaming en vivo. No adoptar |
| `scap` | 0.0.8 (2024-12), 0.1.0-beta.1 (2025-08) | MIT | Win (WGC), macOS (SCK), Linux (PipeWire) | Vía PipeWire y portal | `pipewire ^0.8` (**choca con 0.10**) y el crate `dbus` (**libdbus**) | El repo pasó a `CapSoftware/scap`; último push 2025-08-05 | **Descartar** |
| `crabgrab` | 0.4.0 (2024-06-14) | MIT/Apache-2.0 | Solo Windows y macOS | **No tiene Linux** | — | **Repositorio archivado** | **Descartar** |
| **Portal + `pipewire`** (`ashpd` 0.13.13 y `pipewire` 0.10.1) | — | MIT | Linux (Wayland y X11 con portal) | **Nativo**: selector del compositor, `restore_token` y cursor embebido | `pipewire` y libclang | Oficial | **Adoptar en la fase 3 (Linux)** |

Para Windows y macOS (más adelante) los candidatos son `windows-capture` (Graphics.Capture) y `screencapturekit`. Los usa el proyecto de referencia `references/serein`, pero **no se evaluaron en detalle (no verificado)**.

### 2.8 Captura de cámara

| Opción | Versión | Licencia | Linux | Sandbox (Flatpak) | Deps | Veredicto |
|---|---|---|---|---|---|---|
| `nokhwa` | 0.10.11 (2026-05-15) | Apache-2.0 | V4L2 directo (`v4l` 0.14, de 2023-05). También MSMF (Windows) y AVFoundation (macOS) | Necesita acceso a `/dev/video*`; **no pasa por el portal** | `v4l` (posiblemente libclang, **no verificado**) | Candidato para **Windows y macOS** |
| **Portal Camera + `pipewire`** | `ashpd` 0.13.13 / `pipewire` 0.10.1 | MIT | PipeWire (libcamera o V4L2 por debajo) | **Sí**, con permiso del usuario | `pipewire` y libclang | **Adoptar en Linux (fase 3)**. En esta máquina: Camera v1 con `IsCameraPresent=true` |

### 2.9 Cancelación de eco, supresión de ruido y AGC

| Crate | Versión (fecha) | Licencia | Qué hace | Deps nativas | Veredicto |
|---|---|---|---|---|---|
| **`sonora`** | **0.2.0** (2026-07-29; repo activo en 2026-10-10) | BSD-3-Clause | **Port puro Rust del APM de WebRTC** (M145 más arreglos hasta M156): AEC3, NS, AGC2 y filtro pasa-altos. No porta el estimador neural de eco residual | ninguna | **Adoptar en la fase 2** (AEC y NS). MSRV 1.91 |
| `webrtc-audio-processing` (tonarino) | 2.1.0 (2026-05-13) | BSD-3-Clause según GitHub; crates.io dice "non-standard" | Wrapper del APM de WebRTC empaquetado por PulseAudio (C++) | Enlace dinámico con la librería del sistema, `webrtc-audio-processing` 2.1 en nixpkgs. Con `bundled` compila C++ (requisitos **no verificados**) | Alternativa si `sonora` falla en calidad |
| `nnnoiseless` | 0.5.2 (2025-12-18) | BSD-3-Clause | Port puro Rust de RNNoise (supresión de ruido neural; trabaja a 48 kHz con tramas de 480 muestras, **no verificado en esta revisión**) | ninguna (usar `default-features = false`, porque el default arrastra `clap` y `hound`) | Opcional, como "supresión fuerte" que el usuario activa |
| Módulo `echo-cancel` de PipeWire | — | MIT | AEC a nivel de sistema | PipeWire | Plan B documentado para usuarios de Linux; no depende de RUNE |

### 2.10 Protocolo de voz de Discord (bots, documentación oficial)

Fuente principal: `discord.com/developers/docs/topics/voice-connections`, que hoy redirige con un 301 a `docs.discord.com/developers/topics/voice-connections`. La fuente en GitHub tuvo su último cambio el 2026-03-25.

| Aspecto | Qué dice la documentación oficial |
|---|---|
| Señalización | WebSocket de voz `wss://<endpoint>?v=8` (la v8 es la recomendada; las versiones < 4 se rechazan desde el 2024-11-18). Se necesitan `VOICE_STATE_UPDATE` y `VOICE_SERVER_UPDATE` del gateway principal. Op 0 Identify (con `max_dave_protocol_version`), Op 2 Ready, Op 8 Hello, Op 3/6 heartbeat con `seq_ack` (v8), Op 7/9 resume con buffer |
| Media | **UDP**, con IP discovery por un paquete de 74 bytes. Paquetes **RTP** (`0x80`, payload type `0x78`, seq, timestamp, SSRC) |
| Codec | **Opus, estéreo, 48 kHz**. Antes de enviar audio hay que mandar Op 5 Speaking. Al cortar, enviar 5 frames de silencio `F8 FF FE` |
| Cifrado de transporte | `aead_aes256_gcm_rtpsize` (**preferido**) y `aead_xchacha20_poly1305_rtpsize` (**obligatorio soportarlo**). Los modos `xsalsa20_*` y `aead_aes256_gcm` sin rtpsize están deprecados y discontinuados desde el 2024-11-18. Nonce de 32 bits incremental, añadido al payload |
| **DAVE (E2EE)** | La migración empezó en septiembre de 2024. **"We will only support E2EE calls starting on March 1st, 2026"** para DMs, grupos, canales de voz y Go Live. Si el cliente no soporta DAVE, recibe el close code **4017 "E2EE/DAVE protocol required"**. Usa MLS (RFC 9420) con ciphersuite 2 (DHKEMP256_AES128GCM_SHA256_P256). Los frames Opus se cifran **completos** con AES128-GCM y un tag truncado a 8 bytes; el trailer lleva nonce ULEB128 y el marcador `0xFAFA`. Opcodes 21 a 31, varios binarios. El cifrado de transporte se mantiene además del E2EE |
| ¿Obligatorio? | **Sí**, desde el 2026-03-01 según la doc de desarrolladores. Según el artículo de soporte, que solo se pudo leer a través de resultados de búsqueda porque la descarga directa dio **403**, la exigencia aplica a llamadas **no-Stage** **(no verificado directamente)**. Songbird 0.6.0 (2026-04-05) lo describe como "since 2026 mandatory for all voice connections" |
| Implementaciones | **`davey` 0.1.5** (2026-10-12), MIT, de Snazzah, sobre OpenMLS 0.8. Hay bindings para Node y Python. Su README dice: "there isn't any solid docs on using the Rust crate". Lo usan `songbird` 0.6.x y el proyecto de referencia `serein`. La librería oficial es `discord/libdave` (C++, MIT). El whitepaper está en daveprotocol.com (protocolo v1, documento 1.1.x) |
| **Video / Go Live** | **No está documentado para bots.** La tabla de versiones del gateway menciona "Added video" (v3) y "video sink wants opcode" (v5), y el objeto Voice State expone `self_stream` y `self_video`; la documentación oficial no dice cómo enviar video ni cómo iniciar Go Live. **Actualización 2026-10-16:** con la autorización de REA del dueño, el protocolo (opcodes de stream 18–22, despachos `STREAM_*`, op 0/1/12/15 con video, H264 101/102, DAVE por códec, calidades) quedó confirmado de forma estática en el cliente oficial; ver [REA.md](REA.md#video-go-live-cámaras-y-transporte-de-video-2026-10-16) y el diseño en [VIDEO.md](VIDEO.md) |
| Términos de uso | Lo documentado es para **cuentas bot**. Automatizar una cuenta de usuario (self-bot) está prohibido y puede acabar en el cierre de la cuenta (artículo de soporte 115002192352). Si RUNE pretende hacer voz de Discord con cuentas de usuario, eso queda **fuera de la API documentada** y es un riesgo de ToS |
| Crates de apoyo | Para el AEAD de transporte: `aes-gcm` y `chacha20poly1305` (RustCrypto). Para DAVE: `davey`. Para WebSocket: `tokio-tungstenite`. `songbird` 0.6.1 (ISC) puede servir de **referencia** o de driver completo, pero trae su propio mezclador y reproductor (symphonia), que se solapa con `rune-media` |

---

## 3. Diseño por fases de `rune-media` (propuesta, no contrato implementado)

Principio: `rune-media` no sabe nada de Discord ni de la UI. La API actual es
`Capture`/`CaptureSnapshot`; la escucha propia existe como
`Capture::set_monitor` hacia la reproducción. El canal de eventos y el
trait `VoiceTransport` que siguen son un diseño que no se adoptó: el
transporte de Discord vive en `rune-discord-session::voice`.

### Fase 1: dispositivos locales

**Objetivo:** listar micrófonos y altavoces, medidor de nivel de entrada, mute y prueba de loopback.

| Pieza | Decisión |
|---|---|
| Enumeración | `cpal::default_host()` (PipeWire si está disponible; si no, ALSA). Listar `input_devices()` (en PipeWire, sin los monitores de las salidas; ver [arriba](#corregido-los-micrófonos-ya-no-incluyen-las-salidas-2026-10-19-rama-microfonos-sin-salidas)) y `output_devices()` marcando el dispositivo por defecto. Guardar el `DeviceId` estable para recordar la selección. Reenumerar bajo demanda y cuando llegue `ErrorKind::DeviceNotAvailable` |
| Formato | Pedir 48 kHz en f32 (en 0.18 es la preferencia por defecto). Si un dispositivo no soporta 48 kHz, remuestrear con `rubato` fuera del callback |
| Medidor | En el callback: pico y suma de cuadrados por bloque, publicados en `AtomicU32` (bits de f32). Un hilo de control los lee a unos 30 Hz, calcula dBFS y envía `MediaEvent::InputLevel { rms_db, peak_db }` por canal a la UI |
| Mute | `AtomicBool` leído en el callback: si está activo, se escriben ceros en el ring. El medidor sigue funcionando para que la UI muestre "hablas en mute" |
| Loopback | Micrófono → ring de captura (`rtrb`) → ring de reproducción (`rtrb`) con retardo configurable (0 o 500 ms, para la prueba tipo "eco"). En underrun se rellena con silencio; en overrun se descarta lo más viejo. Así se tolera la deriva de reloj entre dispositivos (para la fase 1 basta) |
| Errores y xruns | Contar los xruns (`ErrorKind::Xrun`, desde 0.18.2) y enviarlos como evento. Nunca hacer `panic` en un callback |
| Criterios de aceptación | (1) Lista correcta en PipeWire y con el fallback ALSA. (2) Medidor fluido y sin cortes. (3) Mute instantáneo. (4) Loopback audible y sin clics tras 10 minutos. (5) Cero asignaciones y cero locks en los callbacks (revisado en code review, con `assert_no_alloc` opcional en debug) |

**Implementado además en la segunda iteración:** `VoiceActivity` decide si la persona habla a partir del nivel del medidor, con histéresis (se activa en 0,55 del medidor, unos −27 dBFS, y se apaga por debajo de 0,45, unos −33 dBFS) y 350 ms de sostén para no cortar entre palabras. Durante una llamada (simulada) enciende el indicador de voz propio. Los umbrales se eligieron sobre el medidor de esta máquina; **falta calibrarlos** con micrófonos y entornos variados.

Crates de la fase 1: **`cpal =0.18.2` (features `["pipewire"]` en Linux)** y **`rtrb =0.4.0`**. `rubato 5.0.1` solo si hay dispositivos que no ofrecen 48 kHz. El canal de eventos es el que ya use la app; si no hay ninguno, `std::sync::mpsc::sync_channel` con `try_send`.

### Fase 2: Opus, jitter buffer y `VoiceTransport`

| Pieza | Decisión |
|---|---|
| Codificación | `opus =0.4.0`, 48 kHz, frames de 20 ms (960 muestras por canal), `Application::Voip`, con FEC in-band y DTX opcionales. Estéreo cuando el destino lo exija (Discord pide estéreo) |
| Procesado | Antes de codificar: `sonora` (AEC3 usando la señal de reproducción como referencia, NS y AGC2) y, opcionalmente, `nnnoiseless` |
| Jitter buffer (propio, en `rune-media`) | Uno por fuente remota. Se ordena por número de secuencia y timestamp RTP; se descartan duplicados y paquetes tardíos. Retardo objetivo adaptativo: arranca en 40–60 ms, crece cuando llegan paquetes tarde y tiene un tope de unos 200 ms. Ante una pérdida: decodificar FEC del paquete siguiente si llegó y, si no, PLC (`decode` con entrada vacía) |
| Mezcla | Suma de las fuentes decodificadas con limitador → ring de reproducción |
| Transporte | Trait agnóstico (boceto abajo). Implementaciones: `LoopbackTransport` (tests y prueba de eco con Opus real), `DiscordVoiceTransport` (WS de voz v8 + UDP/RTP + AEAD rtpsize + DAVE con `davey`) y, más adelante, `str0m` |

Boceto de interfaz. Es solo diseño, no código de RUNE: el transporte recibe y entrega **frames Opus en claro**. El cifrado de transporte y el E2EE (DAVE) son responsabilidad de cada implementación.

```rust
pub trait VoiceTransport: Send {
    /// No bloqueante: encola un frame Opus de 20 ms. `ts` en unidades de 48 kHz.
    fn send_opus(&mut self, frame: &[u8], ts: u32) -> Result<(), TransportError>;
    /// No bloqueante: siguiente frame recibido de cualquier fuente.
    fn try_recv(&mut self) -> Option<IncomingFrame>; // { source: SourceId, seq: u16, ts: u32, payload: Bytes }
    fn set_speaking(&mut self, speaking: bool) -> Result<(), TransportError>;
    fn close(&mut self);
}
```

Las implementaciones de red viven en su propio runtime async (tokio) y se comunican con el hilo de medios mediante colas acotadas. El trait nunca bloquea al hilo de medios.

### Fase 3: pantalla y cámara (portal + PipeWire)

| Pieza | Decisión |
|---|---|
| Pantalla (Linux) | **Implementado** (vista previa local) con `zbus` en `rune-platform::screencast`: types MONITOR\|WINDOW, `cursor_mode` Embedded si existe y, con el portal v4 o más, `persist_mode` hasta que se revoque y el `restore_token` anterior (`rune-platform` `screencast.rs`; el portal recuerda la pantalla elegida). `OpenPipeWireRemote` → `pipewire 0.10.1` (`connect_fd`, stream al node id), solo SHM, escalado propio a ≤ 1280×720. Pendiente: DMA-BUF |
| Cámara (Linux) | **Implementado** (vista previa local): `IsCameraPresent` → `AccessCamera` → `OpenPipeWireRemote`; las cámaras son los nodos `Video/Source` del remote (`node.name`/`node.description`), elegidas con `target.object` |
| Codificación de video | **Implementado** (2026-10-16/17): H264 con `openh264 =0.9.8` (BSD-2; compilado desde la fuente no lo cubre la licencia de patentes de Cisco) en hilos de `rune-media`, y desde el 2026-10-17 por la GPU con GStreamer (VA-API, NVENC/NVDEC) y OpenH264 de respaldo; el transporte es el del cliente oficial, confirmado por REA. Fases y detalle en [VIDEO.md](VIDEO.md) |
| Windows / macOS | `windows-capture` y `screencapturekit` para pantalla; `nokhwa` o APIs nativas para cámara **(no evaluado)** |

---

## 4. Modelo de hilos

```
             ┌──────────── UI (hilo principal) ────────────┐
             │  envía MediaCommand ▼        ▲ recibe MediaEvent (niveles, xruns, dispositivos)
             └─────────────────────┼────────┼───────────────┘
                                   │ mpsc   │ canal acotado (try_send, se descarta si está lleno)
┌──────────────────────────────────▼────────┴──────────────────────────────────┐
│ Hilo de control de medios (no RT): dueño de los cpal::Stream, enumera,       │
│ abre/cierra dispositivos, lee atómicos de nivel a ~30 Hz y emite eventos     │
└───────────────┬──────────────────────────────────────────┬───────────────────┘
                │ crea                                     │ crea
   ┌────────────▼────────────┐                 ┌───────────▼─────────────┐
   │ Callback de CAPTURA (RT)│                 │ Callback de SALIDA (RT) │
   │ mute: AtomicBool        │                 │ pop de rtrb → buffer    │
   │ nivel: AtomicU32        │                 │ underrun → silencio     │
   │ push a rtrb (captura)   │                 └───────────▲─────────────┘
   └────────────┬────────────┘                             │ rtrb (reproducción)
                │ rtrb (captura)                           │
   ┌────────────▼─────────────────────────────────────────┴─────────────┐
   │ Hilo de proceso de medios (no RT, fase 2+): remuestreo, AEC/NS,     │
   │ Opus enc/dec, jitter buffer, mezcla. En fase 1: solo loopback       │
   └────────────┬──────────────────────────────────────────▲────────────┘
                │ cola acotada                             │ cola acotada
   ┌────────────▼──────────────────────────────────────────┴────────────┐
   │ Runtime async (tokio): implementaciones de VoiceTransport           │
   │ (WS de voz, UDP, DAVE/MLS, WebRTC)                                  │
   └─────────────────────────────────────────────────────────────────────┘
```

Reglas, todas obligatorias:

1. **Nada bloquea en los callbacks de audio.** Están prohibidos `Mutex`, `RwLock`, `println!`/logging, asignaciones de memoria, `send` bloqueante, E/S y `panic`. Solo se permiten `rtrb` push/pop, atómicos (`Ordering::Relaxed` o `Acquire/Release`) y aritmética.
2. **SPSC por dirección:** un `rtrb::RingBuffer` para captura (callback → proceso) y otro para reproducción (proceso → callback). Se dimensionan para unos 200 ms (9 600 frames a 48 kHz) y se reservan al abrir el stream.
3. **La UI solo ve eventos.** Los niveles se calculan en el callback (atómicos) y se publican desde el hilo de control a unos 30 Hz con `try_send`. Si la UI va lenta se descartan; nunca hay contrapresión hacia el audio.
4. **Los `cpal::Stream` tienen un único dueño:** el hilo de control. En la 0.18.2 no se puede asumir `Send + Sync` en todos los backends; eso llega en *Unreleased*.
5. **Ningún `await` en los hilos de medios.** El puente con tokio son colas acotadas: en el sentido de envío se usa `try_send` y, si la red se atasca, se descartan frames antiguos.
6. **El trabajo pesado (Opus, AEC, jitter) nunca va en el callback**, sino en el hilo de proceso, que procesa bloques de 10 o 20 ms.

---

## 5. Flake de NixOS: paquetes por fase

| Fase | `nativeBuildInputs` | `buildInputs` | Notas |
|---|---|---|---|
| 1 | `pkg-config`, `rustPlatform.bindgenHook` (libclang para `pipewire-sys` y `libspa-sys`) | `alsa-lib`, `pipewire` | El compilador C lo trae `stdenv` (`mkShell`). En el devShell: `LD_LIBRARY_PATH = lib.makeLibraryPath [ alsa-lib pipewire ]` **o** `-C linker-features=-lld` (ver §1.2) |
| 2 | + `cmake` (libopus incluido vía `opusic-sys`) | sin cambios (`sonora` y `nnnoiseless` son puro Rust) | `libopus` solo hace falta si se elige `opus2` con `dynamic` |
| 3 | sin cambios | sin cambios (`ashpd` usa `zbus` puro Rust) | En runtime: `xdg-desktop-portal` con un backend (`xdg-desktop-portal-hyprland`, `-gnome`, `-kde` o `-wlr`) y WirePlumber. Eso es configuración del sistema, no del flake |
| (opcional) | — | `gst_all_1.gstreamer`, `gst_all_1.gst-plugins-base`, `gst_all_1.gst-plugins-good` | Solo si se adopta GStreamer |
| (opcional) | — | `libjack2` / `webrtc-audio-processing` | Solo con la feature `jack` de cpal o con el crate de tonarino |

Versiones en nixpkgs (unstable) al 2026-10-14: alsa-lib 1.2.15.3, pipewire 1.6.5, libopus 1.6.1, cmake 4.1.2, clang/libclang 21.1.8, GStreamer 1.26.11, webrtc-audio-processing 2.1 y xdg-desktop-portal 1.20.4.

Toolchain: con GStreamer el mínimo es Rust ≥ 1.92 (por su MSRV); sin GStreamer, ≥ 1.91 (por `sonora`). Se recomienda fijar 1.95 en `rust-toolchain.toml`.

---

## 6. Riesgos y bloqueos

| # | Riesgo | Impacto | Mitigación |
|---|---|---|---|
| 1 | Conflicto `links = "pipewire-0.3"` entre ashpd (feature `pipewire` → 0.9) y pipewire 0.10 (verificado) | Bloquea la fase 3 si se activa esa feature | No activarla; usar `pipewire` 0.10 directamente. Revisar futuras versiones de ashpd |
| 2 | rustup ≥ 1.90 con rust-lld no pone el RUNPATH de las libs de Nix (verificado) | El binario no arranca fuera de `nix develop` | `LD_LIBRARY_PATH` en el devShell o `-C linker-features=-lld`. En el paquete: wrapper o autoPatchelf |
| 3 | Sin `cc` ni `pkg-config` en el entorno base (verificado) | `cargo build` falla fuera del devShell | Todo el desarrollo dentro del flake (`nix develop` o direnv) |
| 4 | DAVE obligatorio en Discord desde 2026-03-01 | Sin DAVE no hay voz en Discord (close 4017) | `davey` 0.1.x: joven, sin documentación de la API Rust. Pinear la versión y apoyarse en `songbird` como referencia de uso |
| 5 | Video y Go Live no documentados para bots | No hay vía **documentada**; desde la autorización de REA del 2026-10-16 sí hay una vía confirmada de forma estática en el cliente oficial, para el proveedor no oficial | Implementar por fases según [VIDEO.md](VIDEO.md), con fixtures y prueba real del dueño antes de afirmar nada |
| 6 | Voz de Discord con cuentas de usuario | Riesgo de ToS (self-bots) | Limitar la integración de Discord a cuentas bot, o asumirlo explícitamente |
| 7 | API de `cpal` 0.19 en camino (breaking) | Habrá que migrar | Fijar `=0.18.2` y migrar en un PR aislado |
| 8 | `libwebrtc` (LiveKit) descarga binarios en el build | Rompe el sandbox de Nix | Evitarlo, o pre-descargar con `fetchzip` y usar `LK_CUSTOM_WEBRTC` |

---

## 7. Pendiente / no verificado

- Si el rustc de nixpkgs dentro de `buildRustPackage` sufre el mismo problema de RUNPATH con lld.
- Calidad real de `sonora` frente a `webrtc-audio-processing` en hardware real (AEC con altavoces de portátil).
- Cómo identificar los nodos de cámara dentro del remote del portal Camera: RUNE usa `media.class = Video/Source` y `node.name`; falta comprobarlo con una cámara real.
- Negociación real de la vista previa con xdg-desktop-portal-hyprland y con cámaras (YUY2/NV12 y MJPEG; MJPEG probado sólo con una imagen sintética), y si algún productor exige DMA-BUF.
- El texto exacto del artículo de soporte sobre la exigencia de E2EE (la descarga devolvió 403; se citó a través de resultados de búsqueda).
- Requisitos de build de `aws-lc-rs` (str0m), `v4l` (nokhwa) y `webrtc-audio-processing` con `bundled`.
- `windows-capture` y `screencapturekit` para Windows y macOS.

---

## Fuentes

**Crates (versión, licencia y fecha consultadas en la API de crates.io el 2026-10-14)**
- https://crates.io/crates/cpal · https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md
- https://crates.io/crates/rtrb · https://github.com/mgeier/rtrb
- https://crates.io/crates/rubato
- https://crates.io/crates/opus · https://github.com/SpaceManiac/opus-rs
- https://crates.io/crates/opusic-sys · https://github.com/DoumanAsh/opusic-sys
- https://crates.io/crates/opus2 · https://github.com/cijiugechu/opus2
- https://crates.io/crates/audiopus · https://crates.io/crates/audiopus_sys
- https://crates.io/crates/opus-rs · https://github.com/restsend/opus-rs
- https://crates.io/crates/mousiki · https://crates.io/crates/unsafe-libopus
- https://crates.io/crates/webrtc · https://github.com/webrtc-rs/webrtc · https://webrtc.rs/blog/2026/01/31/async-friendly-webrtc-architecture.html
- https://crates.io/crates/str0m · https://github.com/algesten/str0m
- https://crates.io/crates/libwebrtc · https://crates.io/crates/livekit · https://github.com/livekit/rust-sdks · https://github.com/livekit/rust-sdks/blob/main/webrtc-sys/build/src/lib.rs
- https://crates.io/crates/pipewire · https://gitlab.freedesktop.org/pipewire/pipewire-rs
- https://crates.io/crates/ashpd · https://github.com/bilelmoussaoui/ashpd
- https://crates.io/crates/gstreamer · https://gitlab.freedesktop.org/gstreamer/gstreamer-rs
- https://crates.io/crates/xcap · https://github.com/nashaofu/xcap
- https://crates.io/crates/scap · https://github.com/CapSoftware/scap
- https://crates.io/crates/crabgrab · https://github.com/AugmendTech/CrabGrab
- https://crates.io/crates/nokhwa · https://github.com/l1npengtul/nokhwa
- https://crates.io/crates/sonora · https://github.com/dignifiedquire/sonora
- https://crates.io/crates/webrtc-audio-processing · https://github.com/tonarino/webrtc-audio-processing
- https://crates.io/crates/nnnoiseless · https://github.com/jneem/nnnoiseless
- https://crates.io/crates/davey · https://github.com/Snazzah/davey
- https://crates.io/crates/songbird · https://github.com/serenity-rs/songbird/blob/current/CHANGELOG.md
- https://crates.io/crates/openh264

**Discord**
- https://discord.com/developers/docs/topics/voice-connections (redirige a https://docs.discord.com/developers/topics/voice-connections)
- Fuente: https://github.com/discord/discord-api-docs/blob/main/developers/topics/voice-connections.mdx
- https://docs.discord.com/developers/topics/opcodes-and-status-codes#voice (close code 4017)
- https://docs.discord.com/developers/resources/voice (Voice State: `self_stream`, `self_video`)
- https://daveprotocol.com/ (whitepaper DAVE)
- https://github.com/discord/libdave
- https://support.discord.com/hc/en-us/articles/38749827197591-A-V-E2EE-Enforcement-for-Non-Stage-Voice-Calls (403 al descargar; contenido visto en resultados de búsqueda; **no verificado directamente**)
- https://support.discord.com/hc/articles/115002192352 (self-bots)

**Portales, PipeWire, GStreamer y licencias**
- https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html
- https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Camera.html
- https://gitlab.freedesktop.org/pipewire/pipewire/-/blob/master/LICENSE
- https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html (devolvió 503 el 2026-10-14; se leyó la fuente en https://gitlab.freedesktop.org/gstreamer/gstreamer/-/blob/main/subprojects/gst-docs/markdown/frequently-asked-questions/licensing.md)
- https://opus-codec.org/license/

**Toolchain**
- https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/
- https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable
- https://doc.rust-lang.org/cargo/reference/resolver.html#links

**Antecedente local:** `references/serein` (cliente nativo de Discord, en .gitignore). Usa `cpal =0.18.2`, `opus2 =0.4.0` (bundled), `davey =0.1.4`, `rtrb =0.4.0`, `sonora =0.2.0`, `nnnoiseless =0.5.2` y GStreamer 0.25 para video en Linux.
