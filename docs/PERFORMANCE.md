# Rendimiento de RUNE

Todo lo que sigue se **midió** en la máquina de desarrollo: NixOS con
Wayland, AMD Ryzen 9 270 (Radeon 780M integrada) y NVIDIA GeForce RTX 5060
Laptop, Rust 1.95. Nada es estimado; cada tabla dice sus condiciones.

## Cómo medir

| Qué | Cómo |
| --- | --- |
| Panel en vivo | F12 o Ajustes → Apariencia → Mostrar métricas |
| Scroll continuo (GPU real) | `cargo run --release -p rune-desktop -- --bench` |
| Coste de layout sin GPU | `cargo test -p rune-ui five_thousand -- --nocapture` |
| Procesamiento de audio sintético, sin dispositivos | `cargo run --release -p rune-media --example processing` |
| Supresión de ruido: voz y ruidos sintéticos por la cadena completa | `cargo test --release -p rune-media suppression_table -- --ignored --nocapture` |
| Codificar y decodificar la pantalla compartida, cuadros sintéticos | `cargo run --release -p rune-media --example encode_bench -- [alto] [fps] [segundos] [kbit/s] [--hw\|--va\|--nvidia]` (sin bandera, OpenH264) |
| Escalar la pantalla de un portátil a lo que se manda | `cargo test --release -p rune-media scaling_cost -- --ignored --nocapture` |

El panel mide en cada cuadro: construcción de la vista, layout de limn,
envío del pintado a egui, CPU del cuadro según eframe (incluye teselado y
envío a wgpu, sin la espera de vsync), latencia entrada→píxeles, CPU del
proceso y RSS (`/proc`), tiempo hasta el primer cuadro, mensajes en memoria y
filas construidas. Los cuadros por segundo son la tasa del bucle de pintado
de RUNE (intervalos completados sobre el tiempo observado, hasta 120
muestras), no cuadros presentados por el compositor.

## Ventana

Release con `--features session-login`, simulación, Weston propio sin
pantalla (`--backend=headless --renderer=gl`, 1280×800, GPU real por EGL),
con el escritorio en uso (video en el navegador). CPU de `/proc/<pid>` en
20 s, tras 10 s de asentarse.

| Caso | CPU | RSS | PSS |
| --- | --- | --- | --- |
| Quieta en #general | 0,2–0,3 % de un núcleo | 186–187 MB | 132 MB |
| Quieta en #galería | 0,1 % | 184 MB | 119 MB |
| Scroll continuo (`--bench`, 600 cuadros, 59,9 cuadros/s) | 40,6 % | 184 MB | — |

`--bench`: vista p50 0,86 ms, layout p50 3,07 ms (p95 5,62), pintado p50
0,23 ms, CPU del cuadro 5,4 ms, primer cuadro a los 225 ms. Holgado dentro de
los 16,6 ms de un cuadro a 60 Hz; el layout de las filas (identidad, marcas
de rol, degradés, acciones) es lo primero a revisar si crece.

### 5 000 mensajes cargados

`five_thousand_messages_cost_what_a_screenful_costs`:

| Posición | Filas construidas | Vista + layout (debug) | Vista + layout (release) |
| --- | --- | --- | --- |
| Final | 4 993..5 000 | 2,02 ms | 1,21 ms |
| Medio (`scroll_to_index(2 500)`) | 2 500..2 513 | 2,87 ms | 1,73 ms |
| Inicio | 0..8 | 1,58 ms | 0,82 ms |

El layout depende de lo visible, no del total: es la propiedad de
`measured_list`. En el medio sube porque mide por primera vez las filas que
entran. La vista junta autores una vez por racha de un mismo autor
(`chunk_by`), no por mensaje.

La agrupación por autor camina sólo la racha del autor (`group_starts_at`);
el test `a_long_run_costs_its_length_not_more` protege contra un coste
mayor que el largo de la racha.

### Memoria y arranque

| Configuración | RSS | Primer cuadro | GPU NVIDIA |
| --- | --- | --- | --- |
| Todos los drivers Vulkan | 244 MB | 0,94–1,1 s (3,1 s en frío) | se despierta y queda activa |
| Sólo el driver de la integrada (por defecto) | 129 MB | **74–105 ms** | suspendida todo el tiempo |

Medido leyendo `/sys/bus/pci/devices/0000:01:00.0/power/runtime_status`
antes, durante y después, con la NVIDIA suspendida antes de cada corrida. La
línea base la fija el renderizador: con todos los drivers se cargan el
cargador Vulkan y cinco drivers (NVIDIA, radeon, nouveau, lavapipe, dzn) más
EGL, y abrir el de NVIDIA para enumerar adaptadores la despierta (~0,9 s).

Por eso RUNE limita el cargador Vulkan al driver de la GPU integrada antes
de abrir la ventana ([ARCHITECTURE.md](ARCHITECTURE.md#elección-de-gpu-appsdesktopsrcgpurs)).

## Audio

**Procesamiento local** (`processing`, cuadros mono de 10 ms, sin
dispositivos): 34–55 µs por cuadro con supresión de ruido, menos del 1 % de
un núcleo, a 44,1, 48 y 96 kHz; sin procesamiento, menos de 1 µs. Opus y la
red no están en esa cifra. Es una corrida en este equipo, no una garantía de
plazo ni una medición de latencia acústica.

### Supresión de ruido

Cadena completa (captura a la tasa del micrófono, procesamiento, compuerta
de la llamada en cuadros de 20 ms) con señales **sintéticas y deterministas**
de `rune-media::processing::eval`: voz de fuente-filtro (una grave de 118 Hz
y una aguda de 205 Hz, −20 dBFS, con sílabas y pausas) con ventilador
(−42 dBFS), teclado (clics en ráfagas cerca de −10 dBFS), ruido blanco (−45)
y rosa (−40). Treinta segundos por caso, release.

| Medida | Resultado |
| --- | --- |
| Teclado: micrófono abierto en pausas | 0,0 % (voz grave y aguda) |
| Ruido en pausas (teclado, ventilador, blanco, rosa; también en «Escucharme») | −120 dB (silencio digital) |
| Voz conservada con ruido | −0,1 a +0,9 dB |
| Cortes de voz (tramos de 20 ms de sílabas que salen vacíos) | 0,8–1,8 % |
| Micrófono a 44,1 kHz | RNNoise tras remuestrear; teclado 0,0 % |
| Demora | 10 ms |
| CPU por cuadro de 10 ms | 55 µs p50, 59 µs p95 |
| Eco que queda (AEC3 + RNNoise + compuerta, eco a −6 dB con reverberación) | −78,4 dB |

Por qué la cadena es así (Serein usa la misma base: RNNoise, AEC3 de Sonora
y AGC2):

- RNNoise sólo corre a 48 kHz, así que la captura se pide a 48 kHz al
  dispositivo o se remuestrea; sin eso, a otra tasa se caía a WebRTC y el
  teclado abría el micrófono más del 60 % de las pausas.
- Una compuerta por nivel sola no sirve: RNNoise baja el piso de ruido y
  cada tecla supera el umbral. La compuerta de voz
  (`CaptureProcessor::set_speech_gate`) va al final de la cadena: abre con
  dos cuadros seguidos de probabilidad de voz ≥ 0,6 que no sean un clic (el
  cuarto más bajo del cuadro sin procesar a menos de 12 dB del total), se
  mantiene con ≥ 0,5 y 300 ms después, con rampas de 5 ms al abrir y 20 ms al
  cerrar. La llamada deja pasar lo que pasó la compuerta
  (`VoiceActivity::GATED_ON`, −55 dBFS) y sigue abriéndose por nivel
  sostenido (`update_sustained`), así un clic suelto no la abre. Va con la
  sensibilidad automática; con Pulsar para hablar o un umbral a mano decide
  el nivel.
- Prender o apagar la cancelación de eco no reinicia RNNoise ni la ganancia.
- Orden: volumen → AEC → RNNoise → AGC2 → compuerta, 100 % procesado (mezclar
  señal sin procesar devolvía ruido). Mirar 10 ms hacia adelante y umbrales
  más altos se probaron y no mejoran lo medido.

Límite: la voz es sintética (RNNoise la reconoce con probabilidad 0,9–0,97);
una voz real, susurrada o cantada puede comportarse distinto. Qué probar, en
[MEDIA.md](MEDIA.md).

### Eco

El camino completo de la captura (`rune-media::capture::eval`), con los
tiempos de los dispositivos: callbacks de salida, monitor y micrófono cada
cuántum de PipeWire (1024 muestras), el hilo de salida pasando la
referencia cada 4 ms y el de captura procesando cada 4 ms con su propio
reloj (los dos derivan), salida con 23 ms de latencia y un auricular que
devuelve lo que suena al micrófono 1 ms después, opaco y con un rebote
(−12 dB; «débil», −34 dB). Las voces son las sintéticas de arriba; la música,
acordes, bajo, bombo y platillos a −20 dBFS, y el juego, otra voz encima.
Veinte segundos por caso, release:
`cargo test --release -p rune-media echo_table -- --ignored --nocapture`.

ERLE: cuánto más bajo sale lo que se manda que lo que oyó el micrófono, sólo
con eco, AEC3 solo. «Se manda eco»: cuadros de 20 ms con eco en el micrófono
que la cadena por defecto (Aislamiento de voz: AEC3, RNNoise, AGC2 y la
compuerta) deja salir.

| Caso | ERLE | Se manda eco |
| --- | --- | --- |
| Voces de la llamada | 62,2 dB | 0,0 % |
| Voces de la llamada, eco débil | 43,6 dB | 0,0 % |
| Voces, la salida se queda sin audio 15 ms cada 2,5 s | 62,1 dB | 0,0 % |
| Voces, 2 s después de dejar de estar silenciado | 42,2 dB | 4,8 % |
| Juego y música, referencia de RUNE | 2,6 dB | 100 % |
| Juego y música, referencia del sistema | 51,5 dB | 0,0 % |
| Llamada, juego y música, referencia del sistema | 54,3 dB | 0,0 % |

Con la persona hablando encima, su voz sale entre −0,8 y +1,1 dB de como
sale sin eco. El sonido de otros programas sólo se cancela con la
referencia del sistema (el loopback de la salida): RNNoise oye la voz de un
juego como voz y la deja pasar.

Al dejar de estar silenciado, el primer medio segundo deja pasar algo
(14 dB los primeros 250 ms), después nada. Mientras la persona habla, AEC3
de Sonora quita poco del eco (estimado repitiendo todo con el signo del eco
invertido: 1 a 4 dB, aun con el retardo ideal); lo que lo frena es la
supresión cuando la persona calla. El ajuste de AEC3 que lo gobierna no está
expuesto en Sonora.

## Video

### Codificar la pantalla compartida

`encode_bench` dibuja antes de arrancar el reloj 30 cuadros RGBA en los que
cambia cada píxel (peor que una pantalla quieta, parecido a un juego o un
video) y los entrega a `video::encode::Encoding` al ritmo pedido, como la
captura, a la tasa máxima de la calidad y sin realimentación de la red;
después decodifica las mismas unidades por `video::decode` hasta RGBA. La
CPU es la de **todos los hilos del proceso** (`/proc/self/task/*/stat`): el
de RUNE (incluye copiar cada cuadro), los de GStreamer (`src:src`) y los del
driver. «ms» es lo que tarda un cuadro de entrar al codificador a tener su
unidad. Radeon 780M con Mesa 26.1, RTX 5060 con el driver 595.84, GStreamer
1.26.11; 10 s por corrida.

| Calidad | Motor | Cuadros/s | Mbit/s | CPU al codificar | ms | CPU al decodificar | ms de CPU por cuadro |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 720p30 | OpenH264 | 30,0 | 2,53 | 20 % | 6,5 | 8 % | 2,8 |
| 720p30 | VA-API | 30,0 | 3,22 | 20 % | 6,5 | 5 % | 1,7 |
| 720p30 | NVENC / NVDEC | 30,0 | 3,60 | 12 % | 3,9 | 13 % | 4,4 |
| 720p60 | OpenH264 | 60,0 | 5,34 | 53 % | 8,6 | 16 % | 2,7 |
| 720p60 | VA-API | 60,0 | 8,19 | 22 % | 7,1 | 9 % | 1,4 |
| 720p60 | NVENC / NVDEC | 60,0 | 9,20 | 14 % | 3,9 | 21 % | 3,5 |
| 1080p30 | OpenH264 | 30,0 | 5,27 | 44 % | 13,9 | 17 % | 5,6 |
| 1080p30 | VA-API | 30,0 | 8,32 | 22 % | 10,5 | 8 % | 2,6 |
| 1080p30 | NVENC / NVDEC | 30,0 | 9,17 | 13 % | 4,5 | 19 % | 6,5 |
| **1080p60** | OpenH264 | **33,3** (de 60) | 5,82 | **99 %** | 25,0 | 20 % (a 33/s) | 5,9 |
| **1080p60** | **VA-API** | **60,0** | 8,22 | **30 %** | 10,2 | **11 %** | 1,8 |
| **1080p60** | NVENC / NVDEC | **60,0** | 9,20 | 21 % | 5,3 | 39 % | 6,5 |

- **1080p60 sólo alcanza con la GPU.** Por software el hilo del codificador
  se satura en un núcleo: OpenH264 con un solo corte por cuadro no reparte
  el trabajo en hilos. La contrapresión hace que se salteen cuadros en vez de
  acumularse. La conversión RGBA → I420 cuesta menos de 1 ms: no es el cuello
  de botella.
- Las GPU gastan la tasa entera (8–9 Mbit/s) con estos cuadros; la tasa es un
  techo, así que la diferencia es calidad, no exceso.
- **Mirar** 1080p60 con VA-API cuesta 11 % (casi todo bajando el RGBA de la
  GPU). NVDEC decodifica en la GPU pero convierte NV12 a RGBA en la CPU
  (sin NVRTC no hay conversión de CUDA): 39 %, por eso queda de respaldo de
  VA-API. NVDEC tarda en arrancar: sus primeras 30 imágenes pueden vaciar
  hasta 120 unidades sin contarlas como atrasadas.
- Con carga alta de otro build, la GPU rinde casi igual; el software cae
  (27 cuadros/s a 1080p60).
- Escalar en CPU desde la pantalla del portátil cuesta poco frente a los
  33 ms de un cuadro (`scaling_cost`): 2560×1600 → 1152×720 1,23 ms,
  1920×1080 → 1280×720 1,24 ms, 2560×1600 → 1080p 2,65 ms (16 % de un
  núcleo a 60 cuadros/s). Es lo que ahorraría DMA-BUF hacia VA-API; por qué
  no se hizo, en [VIDEO.md](VIDEO.md).

### Fluidez de la pantalla compartida

Mientras se transmite, RUNE escribe en stderr cada 5 s (sólo tamaños, ritmos
y conteos):

- `rune: voice share: video …`: tamaño, formato, tipo de búfer (`memfd`,
  `memptr` o `dmabuf`) y ritmo que negoció PipeWire (`0/1` es variable),
  cuadros entregados y guardados por segundo, codificados, salteados por el
  control de tasa, cuadros clave, rechazados, reemplazados por
  contrapresión, ms de conversión y codificación, kbit/s producidos y tasa
  del codificador.
- `rune: voice share: network …`: unidades por segundo, cuadros clave,
  descartadas esperando uno, paquetes por segundo, kbit/s enviados y de
  retransmisiones, pérdida media y peor de los reportes RTCP, REMB, tasa
  objetivo, bajadas y cuadros clave pedidos (PLI/FIR).

Decisiones que salieron de medir cada etapa:

| Qué | Medición | Decisión |
| --- | --- | --- |
| Tasa inicial | A 600 kbit/s una pantalla en movimiento da 2,5 cuadros/s en 720p30 (el control de tasa de OpenH264 saltea el resto); 1 500 → 17,8; 3 500 → 30 | Empieza en el máximo de la calidad, como Serein, y baja con la pérdida |
| Reacción a la pérdida | Con NACK y RTX, 5 % de pérdida se repara | Regla de WebRTC: > 10 % baja la mitad de la fracción perdida, 2–10 % se queda, < 2 % sube 8 % tras 2 s tranquilos; el REMB es techo |
| Cambio de tasa | Rehacer el codificador costaba un cuadro clave por cambio | La tasa nueva va al codificador en marcha (`ENCODER_OPTION_BITRATE`); reiniciar queda de respaldo |
| Salteo de OpenH264 | Sin salteo, a 600 kbit/s salen 30 cuadros/s pero a 1,49 Mbit/s | Se deja el salteo |
| Ritmo de captura | Guardar un cuadro si pasó 0,9 del intervalo perdía 16–26 % con ±4–6 ms de variación | Reloj propio (`video::Pace`): cada cuadro de una fuente al ritmo pedido, uno de cada dos de una al doble, nada en ráfaga tras una pausa; se le pide a PipeWire el ritmo de envío |
| Ritmo de envío | Un cuadro clave de 62 KB a 3,5 Mbit/s sale en 57 ms | Sin cambios |

## Lo que no se midió

En [tasks/todo.md](../tasks/todo.md#rendimiento).
