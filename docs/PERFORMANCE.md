# Rendimiento de RUNE

Todo lo que sigue se **midió** el 2026-10-14 en esta máquina: NixOS con
Wayland, AMD Ryzen 9 270 (Radeon 780M integrada) y NVIDIA GeForce RTX 5060
Laptop, Rust 1.95, perfil de energía "balanced", con cargador. Nada es
estimado. La segunda tanda (después de elegir la GPU integrada, ver
"Arranque") reemplaza a la primera donde las dos midieron lo mismo.

## Cómo medir

| Qué | Cómo |
| --- | --- |
| Panel en vivo | F12 o Ajustes → Apariencia → Mostrar métricas |
| Scroll continuo (GPU real) | `cargo run --release -p rune-desktop -- --bench` |
| Coste de layout sin GPU | `cargo test -p rune-ui five_thousand -- --nocapture` |
| Procesamiento de audio sintético, sin dispositivos | `cargo run --release -p rune-media --example processing` |
| Supresión de ruido: voz y ruidos sintéticos por la cadena completa | `cargo test --release -p rune-media suppression_table -- --ignored --nocapture` |
| Codificar la pantalla compartida, cuadros sintéticos | `cargo run --release -p rune-media --example encode_bench -- [alto] [fps] [segundos] [kbit/s] [--hw\|--va\|--nvidia]` (sin bandera, OpenH264) |
| Escalar la pantalla de un portátil a lo que se manda | `cargo test --release -p rune-media scaling_cost -- --ignored --nocapture` |

El panel mide en cada cuadro: construcción de la vista, layout de limn,
envío del pintado a egui, CPU del cuadro según eframe (incluye teselado y
envío a wgpu, sin la espera de vsync), latencia entrada→píxeles (del primer
evento del cuadro al final del pintado), CPU del proceso y RSS (`/proc`),
tiempo hasta el primer cuadro, mensajes en memoria y filas construidas.

## Resultados

### Scroll continuo (`--bench`, release, 601 cuadros sobre #rendimiento)

| Parte | p50 | p95 | máx. |
| --- | --- | --- | --- |
| Vista (`rune_ui::view`) | 0,21 ms | 0,40 ms | 1,55 ms |
| Layout (limn) | 1,20 ms | 1,71 ms | 7,22 ms |
| Envío del pintado | 0,08 ms | 0,14 ms | 0,94 ms |
| CPU del cuadro (eframe, con teselado y wgpu) | ≈2,1–2,2 ms | | |

60 cuadros/s sostenidos (limitado por vsync), 26 % de un núcleo durante el
scroll continuo, 450 mensajes cargados mientras se subía.

Frente a la primera tanda (vista 0,09 ms, layout 0,76 ms, CPU del cuadro
3,1–3,4 ms, 35 % de un núcleo): el cuadro completo cuesta menos porque wgpu
ya no trabaja con la GPU NVIDIA, pero vista y layout subieron. Entre una y
otra se agregaron la línea de entrega, el panel de llamada, la lista de
miembros virtualizada y los permisos por conversación; la ventana además la
dimensiona el compositor y no fue la misma en las dos tandas. No se aisló
una causa única más allá de la que sigue. Todo queda por debajo de un
décimo del presupuesto de un cuadro a 60 Hz (16,6 ms).

### 5 000 mensajes cargados (`five_thousand_messages_cost_what_a_screenful_costs`)

| Posición | Filas construidas | Vista + layout (debug) | Vista + layout (release) |
| --- | --- | --- | --- |
| Final | 4 993..5 000 | 2,02 ms | 1,21 ms |
| Medio (`scroll_to_index(2 500)`) | 2 500..2 513 | 2,87 ms | 1,73 ms |
| Inicio | 0..8 | 1,58 ms | 0,82 ms |

El layout depende de lo visible, no del total: es la propiedad de
`measured_list` (PR limn #25). En release, de esos números, la vista son
~0,11 ms y el layout 0,72 ms; en el medio el layout sube a 1,66 ms porque
mide por primera vez las filas que entran.

La vista sí tenía una parte proporcional al total: juntaba los autores
recorriendo los 5 000 mensajes con una búsqueda en un `HashMap` por
mensaje, 0,70 ms en debug y 0,06 ms en release. Ahora busca una vez por
racha de un mismo autor (`chunk_by`), que es lo común en un chat.

### Memoria

| Proceso | RSS |
| --- | --- |
| Ejemplo mínimo de limn (`counter`, un botón), todos los drivers | 232 MB |
| RUNE con 4 servidores, ~30 personas y 450 mensajes, todos los drivers | 244 MB |
| Lo mismo, solo el driver de la GPU integrada (por defecto ahora) | 129 MB |

La línea base la fija el renderizador: con todos los drivers se cargan el
cargador Vulkan y cinco (NVIDIA, radeon, nouveau, lavapipe, dzn) más EGL
([REA.md](REA.md)); no cargar los otros cuatro ahorra unos 115 MB. Con
todos cargados, lo propio de RUNE eran unos 12 MB sobre el ejemplo mínimo.

### Arranque

Del inicio de `main` al primer cuadro pintado.

| Configuración | Primer cuadro | GPU NVIDIA |
| --- | --- | --- |
| Antes: todos los drivers, `WGPU_POWER_PREF=high` | 1,0–1,1 s (3,1 s en frío) | se despierta y queda activa |
| Antes: todos los drivers, `WGPU_POWER_PREF=low` | 0,94–0,95 s | se despierta y queda activa |
| Ahora, por defecto: solo el driver de la integrada | **74–105 ms** | suspendida todo el tiempo |

Medido con REA (`capture_process_scenario`) y leyendo
`/sys/bus/pci/devices/0000:01:00.0/power/runtime_status` antes, durante y
después, con la GPU NVIDIA suspendida antes de cada corrida. El tiempo no
lo gastaba RUNE sino el cargador Vulkan: abre todos los drivers instalados
para enumerar adaptadores, y abrir el de NVIDIA la despierta (~0,9 s) aunque
después se use la integrada.

Por eso `apps/desktop/src/gpu.rs` decide antes de abrir la ventana: si hay
una GPU integrada que maneja la pantalla (`boot_vga` en sysfs) y otra
discreta, limita el cargador al driver de la integrada
(`VK_LOADER_DRIVERS_SELECT`, `*radeon*` para AMD, `*intel*` para Intel).
Respeta cualquier elección previa (`VK_LOADER_DRIVERS_SELECT`,
`VK_DRIVER_FILES`, `VK_ICD_FILENAMES`) y `RUNE_GPU` con cualquier valor
(por ejemplo `RUNE_GPU=any`) la desactiva; `WGPU_POWER_PREF` no, porque no
evita que el cargador abra todos los drivers. Si la ventana no abre con ese filtro, RUNE se vuelve a
ejecutar sin él (`RUNE_GPU=fallback`, y Ajustes lo dice). Desde el
2026-10-18 la persona también elige en Ajustes › Sistema › Gráficos
(«Dibujar con»: Automático o cada fabricante detectado, guardado en
`~/.config/rune/gpu`, para el próximo arranque): AMD `*radeon*`, Intel
`*intel*`, NVIDIA `*nvidia*,*nouveau*`. Esas mismas cuatro variables
(`CHOSEN_BY_PERSON` en `gpu.rs`) siguen mandando sobre esa elección. Las capturas de esta iteración pintaron su primer cuadro
en 77–96 ms.

## Un defecto encontrado midiendo

La primera versión del timeline tardaba **15 s por layout**: la función que
decide si un mensaje continúa el grupo de su autor se llamaba
recursivamente a sí misma a través de otra, con coste exponencial en grupos
largos. Reescrita como un recorrido lineal de la racha del autor
(`group_starts_at`), el mismo layout cuesta **1,4 ms**. Un test
(`a_long_run_costs_its_length_not_more`) protege contra la regresión.

## Decisiones que sostienen estos números

- Solo se construyen las filas visibles; cada mensaje se mide una vez y su
  altura se recuerda por id.
- Los mensajes se comparten (`Arc`) entre el estado y la vista; un cuadro no
  copia mensajes.
- La red, el proveedor y SQLite viven fuera del hilo de UI; la UI drena como
  mucho 256 eventos por cuadro.
- Se repinta solo cuando algo cambia (animación, escritura que caduca,
  avisos, medidor); en reposo la ventana no consume CPU.
- Las conversaciones cerradas se recortan a 150 mensajes en memoria.
- `limn-egui` decodifica imágenes fuera del hilo de UI y limita su caché de
  texturas (se usará cuando haya imágenes reales).

## Pendiente

- El primer arranque después de encender la máquina (caché de shaders y de
  drivers fría) no se volvió a medir con el filtro de GPU.
- Probar el filtro en una máquina solo-Intel y en una sola GPU discreta
  (en ese caso `gpu.rs` no filtra; está cubierto por tests, no por
  hardware).
- Medir latencia de entrada con eventos reales de teclado (el `--bench` usa
  rueda sintética, por eso el panel muestra "—").
- Medir con imágenes reales cuando exista un proveedor que las sirva.

## Corrección del contador de cuadros (2026-10-14)

La captura MCP de REA descrita en [REA.md](REA.md) reveló un defecto en
`Metrics::rate`: retenía como máximo 120 muestras y dividía su cantidad por
dos segundos fijos. Por ello **los 60 cuadros/s informados en mediciones
anteriores no demuestran una frecuencia real de 60 Hz**; el contador nunca
podía superar ese valor y subestimaba el arranque. Los tiempos individuales
de vista, layout y pintado no dependían de esa fórmula.

Ahora se divide la cantidad de intervalos completados por el tiempo
realmente observado hasta la consulta. El cálculo usa hasta 120 muestras,
conserva el intervalo que cruza el límite de dos segundos para incluir
pausas, y devuelve cero sin intervalos o tras dos segundos sin pintar.
A frecuencia alta, la ventana observada puede ser menor de dos segundos.
Es una tasa del bucle de pintado de RUNE, no una medición GPU ni de cuadros
presentados por el compositor. Cinco pruebas con tiempos inyectados cubren
el techo anterior (200 Hz), arranque, inactividad, pausa y borde de ventana.

## Validación de continuidad: correcciones de caché (2026-10-14)

Se construyó de nuevo con
`nix-shell --run 'cargo build --release -p rune-desktop'` y se ejecutó
`target/release/rune --bench` desde un shell normal, en la sesión Wayland.
El benchmark desactiva SQLite; comprueba el renderizado y el arranque, no el
rendimiento de las escrituras corregidas. La revisión de caché tiene pruebas
funcionales, sin una afirmación de aceleración.

Dos corridas consecutivas, 601 cuadros y 350 mensajes cargados cada una:

- Primera: arranque 170,3 ms; vista p50/p95 0,36/0,51 ms; layout
  1,78/3,63 ms (máximo 11,78 ms); pintado 0,11/0,19 ms; RSS 131,5 MB.
- Segunda: arranque 81,8 ms; vista p50/p95 0,31/0,52 ms; layout
  1,81/3,56 ms (máximo 9,82 ms); pintado 0,11/0,19 ms; RSS 131,2 MB.

Ambas mostraron 60,0 cuadros/s y 41,9 % de CPU del proceso en la última
muestra. La primera coincidió al principio con una prueba de layout; la
segunda se ejecutó sin otra compilación ni prueba de Cargo de esta sesión.
No se controló la actividad del resto del escritorio. Los números anteriores
de este documento corresponden a otras versiones y tamaños de ventana, y
no forman una comparación antes/después de estos cambios. La diferencia de
arranque entre estas dos corridas tampoco permite aislar una causa.

Validación de esta revisión: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings` y
`cargo test --workspace` dentro de `nix-shell`: **289 pruebas aprobadas**,
incluidas cuatro nuevas sobre borrados en caché, poda por conversación y
prioridad del historial confirmado. El test de 5 000 mensajes también pasó
al ejecutarlo por separado; aquí no se registra una nueva medición de sus
tiempos porque el resumen de la herramienta no los conservó.

También se verificó visualmente la aplicación con dos capturas de su
proveedor simulado y una base temporal aislada mediante `XDG_DATA_HOME`:
primero `--screenshot connected.png --data --channel general`, después
`--screenshot cached.png --data --latency 8000 --channel general`. El segundo
arranque mostró los mensajes guardados, la presencia desconectada y el
aviso de conexión mientras esperaba al proveedor. Los primeros cuadros
fueron 91,6 y 95,7 ms, respectivamente; las capturas se guardaron y ambas
ejecuciones cerraron normalmente. No se usaron los datos locales habituales.

## Validación de búsqueda y métricas (2026-10-14)

Sobre los cambios finales pasaron `cargo fmt --all -- --check`,
`cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
y `cargo test --workspace`: **305 pruebas aprobadas**. Se agregaron once
pruebas de búsqueda y cinco del contador de cuadros. La regresión de
5 000 coincidencias comprueba que se construyen menos de 12 filas visibles
y que navegar hasta el último resultado lo pone a la vista. Es una prueba
funcional de virtualización, no una medición de latencia de búsqueda.

Se compiló `cargo build --release -p rune-desktop` y se abrió el resultado
en Wayland con `--screenshot`, sin `--data`. Se escribió mediante eventos
de teclado reales solo después de comprobar que la ventana activa tenía
el PID del proceso de prueba. La consulta `limn` mostró el estado sin
resultados; `rust` mostró **13 coincidencias** sobre 50 mensajes cargados,
con selección, autor, hora, texto recortado y scrollbar. Ambas capturas se
revisaron visualmente, con la etiqueta de simulación visible; no se midió
latencia de entrada. Los primeros cuadros fueron 77,1 y 82,0 ms. Evidencia
local temporal: `/tmp/rune-search-final/search-results.png` (sin resultados)
y `/tmp/rune-search-final/search-matches.png` (con coincidencias).

El salto, foco, preservación de borradores, actualización de resultados y
coexistencia con otros diálogos se validaron además con el árbol real de
limn en pruebas sin ventana. La revisión detectó y corrigió selección
reiniciada al actualizar un perfil, búsqueda detrás de otro modal y texto
anterior retenido por el ID de input al reabrir. No se agregaron
dependencias ni se modificó limn.

## Procesamiento local de audio (2026-10-14)

Se ejecutó el ejemplo `processing` de `rune-media`, compilado en release,
mediante `capture_process_scenario` de REA. La supresión local usa Sonora
0.2.0 con filtro pasaaltos y nivel Moderate; AEC y AGC están desactivados.
El ejemplo genera ruido determinista y un tono intermitente con armónicos,
sin abrir dispositivos ni transmitir o grabar audio. Conserva la frecuencia
de entrada; Sonora convierte internamente cuando corresponde.

Cada caso procesa cuadros mono de 10 ms: 100 de calentamiento y 1000
medidos, sin esperar entre cuadros. El tiempo viene de `Instant` dentro de
`CaptureProcessor::process`: incluye sanitización, DSP y cálculo de ambos
niveles; excluye generación de la señal, creación/reset del motor, callback,
colas, apertura del dispositivo y transporte. Se pausaron las compilaciones
y pruebas de esta sesión durante la captura. El resto del escritorio no se
aisló ni se fijó afinidad de CPU.

| Frecuencia | Modo | p50 | p95 | Máximo | p95 / 10 ms | Cuadros ≥10 ms |
| --- | --- | --- | --- | --- | --- | --- |
| 44 100 Hz | Off | <1 µs | <1 µs | 4 µs | <0,01 % | 0/1000 |
| 44 100 Hz | Supresión de ruido | 42 µs | 47 µs | 365 µs | 0,47 % | 0/1000 |
| 48 000 Hz | Off | <1 µs | <1 µs | <1 µs | <0,01 % | 0/1000 |
| 48 000 Hz | Supresión de ruido | 34 µs | 37 µs | 536 µs | 0,37 % | 0/1000 |
| 96 000 Hz | Off | <1 µs | <1 µs | 1 µs | <0,01 % | 0/1000 |
| 96 000 Hz | Supresión de ruido | 46 µs | 50 µs | 110 µs | 0,50 % | 0/1000 |

Los valores `<1 µs` se imprimieron como cero por truncarse a microsegundos
enteros: no significan coste cero. Son resultados de una corrida sobre este
equipo, no una garantía de plazo ni una medición de latencia acústica. El
cociente RMS que también imprime el ejemplo corresponde a toda la señal
sintética, no a una medida de reducción de ruido sobre voz. No se evaluaron
inteligibilidad, artefactos percibidos ni cancelación de eco.

La prueba funcional `stationary_noise_is_attenuated_after_adaptation`
comprueba por separado que, después de tres segundos sintéticos de
adaptación, el RMS de ruido estacionario sea menor que el 85 % de la entrada
y mayor que cero. Pasó junto a las pruebas de bypass, silencio, longitudes,
frecuencias, cambio de modo/reset, valores no finitos y límites de amplitud.
Esta prueba tampoco acredita calidad de voz humana.

REA observó salida normal (`exit 0`) y conservó el texto completo en
[la evidencia exportada](evidence/rea-audio-processing-2026-10-14.json),
registro `ev_47695cf4a9f254ddee7bc60189050827df2df158d6a7ca8194316e468c7f9c7e`.
El SHA-256 del ejecutable, igual antes y después, fue
`f8b6ceb436a58d90828cd1d4a2111cc77082843764cd3b28b806c29da2bfbb43`.
La captura muestrea procesos y no inspecciona red ni archivos; esas
limitaciones constan en la evidencia. La ausencia de apertura de micrófono
procede del código del ejemplo, no de una auditoría de syscalls por REA.

## Codificar la pantalla compartida (2026-10-17)

Se ejecutó el ejemplo `encode_bench` de `rune-media`, compilado en release
(`cargo build --release -p rune-media --example encode_bench`), en la
máquina de arriba (Ryzen 9 270, 16 hilos), con el escritorio en uso y otra
compilación terminada poco antes (carga media de 1 al medir). Cada corrida
dura 10 s.

Método: el ejemplo dibuja antes de arrancar el reloj 30 cuadros RGBA del
tamaño pedido en los que cambia cada píxel de uno a otro (bandas que se
corren y una barra que barre: peor que una pantalla quieta, parecido a un
juego o un video) y los entrega a `video::encode::Encoding` al ritmo
pedido, como lo hace la captura: conversión RGBA → I420 y OpenH264
Constrained Baseline en `ScreenContentRealTime`, a la tasa máxima de esa
calidad (9 Mbit/s para 1080p y 720p60; 3,5 para 720p30), sin
realimentación de la red. La CPU es la del hilo `rune-video-encode` leída
de `/proc/self/task/*/stat` (tiempo de usuario más sistema, en ticks de
10 ms); no incluye dibujar los cuadros. La conversión a I420 sola se mide
aparte sobre los mismos 30 cuadros.

| Calidad | Unidades por segundo | Mbit/s | CPU del hilo | ms por unidad | RGBA → I420 |
| --- | --- | --- | --- | --- | --- |
| 1080p60 | 35,4 – 39,5 (de 60) | 6,2 – 6,9 | 99 % de un núcleo | 25,2 – 28,1 | 0,9 ms |
| 1080p30 | 30,0 | 5,3 | 41 – 47 % | 13,6 – 15,8 | 0,9 ms |
| 720p60 | 60,0 | 5,3 | 48 – 49 % | 8,1 | 0,4 ms |
| 720p30 | 30,0 | 2,5 | 21 % | 7,1 | 0,4 ms |

Los rangos son de tres corridas de 1080p60 y dos de 1080p30. **1080p60 no
alcanza por software en esta máquina**: el hilo del codificador se satura
en un núcleo y salen entre 35 y 40 cuadros por segundo; la contrapresión de
la fase 3 hace que se salteen cuadros en vez de acumularse, y la vista
previa muestra los fps reales. La conversión de color no es el cuello de
botella (menos de 1 ms); lo es OpenH264, que con un solo corte por cuadro
no reparte el trabajo en hilos aunque se le pidan (`num_threads` 0, el
modo automático, es lo que usa RUNE). 1080p30 y 720p60 caben holgados en un
núcleo. Pendiente, si hace falta 1080p60: varios cortes por cuadro con
hilos de OpenH264 (el crate no expone la cantidad de cortes, sólo su largo
máximo), o un codificador por hardware: hecho el mismo día, ver
[Video por hardware](#video-por-hardware-2026-10-17-rama-video-por-hardware).

## Supresión de ruido, medida (2026-10-17, rama `audio`)

El dueño probó la voz contra Discord y pidió mejorar mucho la supresión
de ruido. Se midió la cadena completa de RUNE (captura a la tasa del
micrófono, procesamiento, compuerta de la llamada en cuadros de 20 ms) con
señales **sintéticas y deterministas** de `rune-media::processing::eval`:
voz de fuente-filtro (tren de pulsos por formantes de cinco vocales,
fricativas de ruido, sílabas, palabras y pausas de 0,7–1,4 s; una voz
grave de 118 Hz y una aguda de 205 Hz, −20 dBFS) con ventilador (ruido
ancho con una joroba en 600 Hz y el tono de las aspas, −42 dBFS), teclado
(clics en ráfagas, picos cerca de −10 dBFS), ruido blanco (−45) y rosa
(−40). Treinta segundos por caso, release, en esta máquina.

Métricas: **pausas** (salida contra entrada desde 0,5 s después de la voz:
−120 dB es silencio digital), **abierto** (tramos de 20 ms de las pausas
con algo: el micrófono abierto con ruido), **voz** (nivel de salida
contra la voz limpia mientras se habla), **cortes** (tramos de 20 ms de
sílabas que salen vacíos), **demora** (correlación con la voz limpia) y
**CPU** por cuadro de 10 ms. El SDR segmental que también imprime depende
de la fase (el filtro de WebRTC lo deja negativo sin dañar la voz), así
que sirve para comparar RNNoise con RNNoise, no entre supresores.

| Caso (voz grave / aguda) | Antes: perfil por defecto | Ahora |
| --- | --- | --- |
| Teclado: micrófono abierto en pausas | **59,7 % / 14,7 %** | **0,0 % / 0,0 %** |
| Teclado: ruido en pausas | −6,1 / −9,3 dB | −120 dB (silencio) |
| Ventilador, blanco, rosa: ruido en pausas (lo que sale) | −120 dB | −120 dB |
| Lo mismo en «Escucharme» (sin la compuerta de la llamada) | RNNoise solo: −28,6 / −11,4 / −29,8 dB | −120 dB |
| Voz conservada con ruido | −0,1 a +0,9 dB | −0,1 a +0,9 dB |
| Cortes de voz | 0–0,5 % | 0,8–1,8 % |
| Micrófono a 44,1 kHz: supresor | WebRTC (RNNoise sólo a 48 kHz); teclado abierto 69,4 / 62,8 % | RNNoise tras remuestrear; teclado 0,0 % |
| Demora | 10 ms (RNNoise) | 10 ms |
| CPU por cuadro de 10 ms | 54 µs p50 | 55 µs p50, 59 µs p95 |
| Eco que queda (AEC3 + RNNoise + compuerta, eco a −6 dB con reverberación) | — | −78,4 dB |

Sin procesamiento (perfil Estudio), la compuerta de la llamada que ya no
abre con un clic baja el teclado abierto de 24,6 / 18,0 % a 18,7 / 8,6 %.

**Por qué rendía peor, en el código** (Serein tiene la misma cadena:
RNNoise, AEC3 de Sonora y AGC2, ver `references/serein`):

- La supresión **sí** estaba encendida por defecto (Aislamiento de voz =
  RNNoise + AEC + AGC), pero RNNoise sólo corría con el micrófono a 48 kHz;
  a otra tasa caía a WebRTC. Serein pide 48 kHz al dispositivo y, si no,
  remuestrea. Ahora RUNE hace lo mismo.
- La probabilidad de voz que calcula RNNoise se tiraba. La compuerta de la
  llamada decidía por nivel: 15 dB sobre el piso, mínimo −42 dBFS. RNNoise
  baja el ruido de fondo, el piso baja con él y cada tecla supera el umbral
  y abre el micrófono 350 ms: con RNNoise el teclado abría **más** que sin
  él (59,7 % contra 24,6 %).
- Nada distinguía un transitorio (una tecla) de la voz.
- Prender o apagar la cancelación de eco (al empezar o terminar de sonar
  algo) reiniciaba también RNNoise y la ganancia automática.
- Orden, ganancia y mezcla estaban bien: volumen → AEC → RNNoise → AGC2,
  la compuerta después, 100 % procesado (mezclar la señal sin procesar a
  −20 dB subió el SDR menos de 1 dB y devolvió ruido: no se adoptó).

**Qué se cambió:** captura a 48 kHz (pedida al dispositivo o remuestreada);
compuerta de voz al final de la cadena (`CaptureProcessor::set_speech_gate`):
se abre con dos cuadros seguidos de probabilidad ≥ 0,6 que además no sean
un clic (el cuarto más bajo del cuadro sin procesar a menos de 12 dB del
total), se mantiene con ≥ 0,5 y 300 ms después, con rampas de 5 ms al abrir
y 20 ms al cerrar; la llamada deja pasar lo que pasó la compuerta
(`VoiceActivity::GATED_ON`, −55 dBFS) y sigue abriéndose por nivel
sostenido, así un clic suelto no la abre (`update_sustained`). Va con la
sensibilidad automática y RNNoise, lo predeterminado; con Pulsar para
hablar o un umbral elegido a mano decide el nivel, como antes. El AEC ya no
reinicia RNNoise ni la ganancia. Se probó y se descartó: mirar 10 ms hacia
adelante (los cortes bajan de 1,7 a 1,4 % por 10 ms más de demora) y
umbrales de probabilidad más altos (no cambian lo medido).

**Límites.** La voz es sintética: RNNoise la reconoce (probabilidad 0,9–0,97
hablando, 0,10–0,15 en pausas), pero una voz real, susurrada o cantada
puede comportarse distinto; los cortes de 1–2 % son sobre todo la fricativa
que abre una frase. Nadie escuchó esto todavía: ver qué probar en
[MEDIA.md](MEDIA.md#supresión-de-ruido-mejorada-2026-10-17).

## Fluidez de la pantalla compartida (2026-10-17, rama `audio`)

El dueño compartió la pantalla completa contra Discord real y la notó «poco
fluida», sin Nitro (720p30, el preajuste por defecto). Sin acceso a su
red, se midió cada etapa por separado en esta máquina (release, carga
media 3,5–4 por otro build) y se agregó el registro que hacía falta.

**Diagnóstico nuevo** (stderr, cada 5 s mientras se transmite; sólo
tamaños, ritmos y conteos):

- `rune: voice share: video …` (el host): tamaño, formato, tipo de
  búfer (`memfd`, `memptr` o `dmabuf`) y ritmo que negoció PipeWire
  (`0/1` es variable), cuadros entregados y guardados por segundo, tamaño
  codificado, codificados por segundo, salteados por el control de tasa de
  OpenH264, cuadros clave, rechazados, reemplazados antes de que el
  codificador los tomara (contrapresión), ms de conversión y de
  codificación por cuadro, kbit/s producidos y la tasa del codificador.
- `rune: voice share: network …` (la conexión): unidades por segundo,
  cuadros clave, descartadas esperando uno, paquetes por segundo, kbit/s
  enviados y de retransmisiones, pérdida media y peor de los reportes RTCP,
  REMB, tasa objetivo, bajadas y cuadros clave pedidos (PLI/FIR).

**Lo que se encontró, con números:**

| Causa revisada | Medición | Qué se hizo |
| --- | --- | --- |
| Tasa inicial de 600 kbit/s que sube 5 % por segundo, y sólo con reportes | `encode_bench 720 30` con cuadros que cambian enteros: a 600 kbit/s salen **2,5 cuadros/s** (220 de 240 salteados por el control de tasa); 1 000 → 6; 1 500 → 17,8; 2 000 → 24,8; 3 500 → **30,0** | Empieza en el máximo de la calidad (3,5 Mbit/s en 720p30), como Serein, y baja con la pérdida |
| Bajar 20 % con 5 % de pérdida | Con NACK y RTX, 5 % de pérdida se repara; cortar ahí hundía la tasa en un enlace con pérdida pero ancho | Regla de pérdida de WebRTC: > 10 % baja la mitad de la fracción perdida, 2–10 % se queda, < 2 % sube 8 % tras 2 s tranquilos; el REMB sigue de techo |
| Cada cambio de tasa ≥ 15 %/25 % rehacía el codificador (un cuadro clave) | Test: cuatro cambios de tasa, cero cuadros clave nuevos, cero reinicios | La tasa nueva va al codificador en marcha (`ENCODER_OPTION_BITRATE`, como WebRTC con OpenH264); reiniciar queda de respaldo |
| Quitar el salteo de cuadros de OpenH264 | Sin salteo, a 600 kbit/s salen 30 cuadros/s pero a **1,49 Mbit/s** (2,5× la tasa; OpenH264 avisa que no la controla) | Se deja el salteo: una tasa alta es lo que da los 30 cuadros |
| Ritmo de captura: se guardaba un cuadro si pasaba 0,9 del intervalo desde el anterior | Simulado con 3 000 llegadas a 30 cuadros/s con variación uniforme: con ±2 ms se guardaba el 98,6 %, con ±4 ms el **83,7 %** y con ±6 ms el 74,1 % (cada cuadro perdido, un salto de 66 ms); con la regla nueva, el 100 % en los tres | Ritmo con reloj propio (`video::Pace`): se guarda cada cuadro de una fuente al ritmo pedido aunque varíe, uno de cada dos de una al doble, y nada en ráfaga tras una pausa (test). Se le pide a PipeWire el ritmo de envío (30 o 60) |
| Escalar en CPU desde la resolución nativa | `scaling_cost`: 2560×1600 → 1152×720 **1,23 ms**; 1920×1080 → 1280×720 1,24 ms; 2560×1600 → 1080p 2,65 ms | Barato frente a los 33 ms de un cuadro: no se tocó (vecino más cercano: se ve escalonado en texto fino, no entrecortado) |
| Ritmo de envío y marcas de tiempo | Lotes cada 2 ms a 2,5× la tasa; marca RTP a 90 kHz del momento en que el codificador toma el cuadro (a lo sumo 4 ms después de capturado). Un cuadro clave de `encode_bench` pesa 42,8 KB a 600 kbit/s y tardaba **228 ms** en salir (siete cuadros sin tomar); a 3,5 Mbit/s pesa 62,1 KB y sale en 57 ms | Sin cambios en el ritmo: lo arregla la tasa |

Con esto, la explicación más probable de lo que vio el dueño es la tasa:
720p30 empezaba en 600 kbit/s y OpenH264 salteaba la mayoría de los cuadros
de una pantalla en movimiento hasta que la tasa subía (a 5 % por segundo,
más de 30 s hasta 3,5 Mbit/s, y nunca si Discord no manda reportes). **No
se probó contra Discord**: si sigue poco fluido, las dos líneas de arriba
dicen en qué etapa (ritmo de PipeWire, salteos, contrapresión, pérdida o
bajadas de tasa).

## Video por hardware (2026-10-17, rama `video-por-hardware`)

`encode_bench` en release, en la misma máquina (Ryzen 9 270, Radeon 780M
con Mesa 26.1, RTX 5060 Laptop con el driver 595.84, GStreamer 1.26.11),
con el escritorio en uso, 10 s por corrida. Mismo método que arriba
(cuadros sintéticos que cambian enteros, a la tasa máxima de la calidad,
sin realimentación de la red), pero la CPU es ahora la de **todos los
hilos del proceso** mientras codifica y mientras decodifica (de
`/proc/self/task/*/stat`, ticks de 10 ms): el de RUNE, los de GStreamer
(`src:src`) y los del driver (`cs0` de Mesa, `cuda-EvtHandlr`). El hilo del
codificador incluye copiar cada cuadro sintético (3,7 MB en 720p, 8,3 MB en
1080p), como la captura copia uno propio. «ms» es lo que tarda un cuadro
de entrar al codificador a tener su unidad (con la GPU, la espera entre
hilos incluida). Decodificar: las mismas unidades por `video::decode`, al
mismo ritmo, con el mismo motor, hasta RGBA.

Antes es OpenH264 (sin bandera); después, `--hw` (VA-API en la 780M, lo
que elige RUNE) y `--nvidia` (NVENC/NVDEC, el siguiente). Carga media de
2,3 a 3,9 (otro build terminando); NVDEC, de una segunda pasada con carga
de 6,3 a 6,8, después de un arreglo (abajo).

| Calidad | Motor | Cuadros/s | Mbit/s | CPU al codificar | Por hilo | ms | CPU al decodificar | ms de CPU por cuadro |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 720p30 | OpenH264 | 30,0 | 2,53 | 20 % | encode 20 % | 6,5 | 8 % | 2,8 |
| 720p30 | VA-API | 30,0 | 3,22 | 20 % | encode 18 %, `src:src` 2 % | 6,5 | 5 % | 1,7 |
| 720p30 | NVENC / NVDEC | 30,0 | 3,60 | 12 % | encode 10 %, `src:src` 2 % | 3,9 | 13 % | 4,4 |
| 720p60 | OpenH264 | 60,0 | 5,34 | 53 % | encode 53 % | 8,6 | 16 % | 2,7 |
| 720p60 | VA-API | 60,0 | 8,19 | 22 % | encode 16 %, `src:src` 5 %, `cs0` 2 % | 7,1 | 9 % | 1,4 |
| 720p60 | NVENC / NVDEC | 60,0 | 9,20 | 14 % | encode 10 %, `src:src` 4 % | 3,9 | 21 % | 3,5 |
| 1080p30 | OpenH264 | 30,0 | 5,27 | 44 % | encode 44 % | 13,9 | 17 % | 5,6 |
| 1080p30 | VA-API | 30,0 | 8,32 | 22 % | encode 16 %, `src:src` 4 %, `cs0` 1 % | 10,5 | 8 % | 2,6 |
| 1080p30 | NVENC / NVDEC | 30,0 | 9,17 | 13 % | encode 10 %, `src:src` 3 % | 4,5 | 19 % | 6,5 |
| **1080p60** | OpenH264 | **33,3** (de 60) | 5,82 | **99 %** | encode 99 % | 25,0 | 20 % (a 33/s) | 5,9 |
| **1080p60** | **VA-API** | **60,0** | 8,22 | **30 %** | encode 20 %, `src:src` 8 %, `cs0` 2 % | 10,2 | **11 %** | 1,8 |
| **1080p60** | NVENC / NVDEC | **60,0** | 9,20 | 21 % | encode 12 %, `src:src` 8 % | 5,3 | 39 % | 6,5 |

Lo que dicen:

- **1080p60 alcanza con la GPU**: 60 cuadros por segundo con VA-API (30 %
  de un núcleo entre todos los hilos, copia del cuadro incluida) o NVENC
  (21 %), contra 33 de 60 con un núcleo entero por software. Ninguna
  corrida por GPU tuvo salteos; OpenH264 salteó 55 a 1080p60.
- Las GPU gastan la tasa entera (8–9 Mbit/s) con estos cuadros que
  cambian enteros; OpenH264 se queda en 5–6. La tasa es un techo, así que
  la diferencia es calidad, no exceso.
- **Mirar 1080p60** con VA-API cuesta 11 % (1,8 ms por cuadro, la mayor
  parte bajando el RGBA de la GPU); por software, unos 35 % a 60 cuadros/s
  (5,9 ms por cuadro; el codificador sólo dio 33). NVDEC decodifica en la
  GPU pero convierte NV12 a RGBA en la CPU (`videoconvert`, sin NVRTC no
  hay conversión de CUDA): 39 % a 1080p60, igual o peor que OpenH264; queda
  de respaldo de VA-API.
- Una segunda pasada completa con carga de 6,3 a 9 (otro build) dio por
  software 74 % (720p60), 64 % (1080p30) y 27 cuadros/s (1080p60), y por
  GPU casi lo mismo que la tabla (VA-API 1080p60: 22 % al codificar, 12 %
  al decodificar): la GPU no compite por la CPU con lo demás.

**Arreglado midiendo:** NVDEC tarda en arrancar y las unidades que
esperaban lo hacían contar como atrasado: descartaba hasta una clave que el
banco nunca pide (13 cuadros mostrados de 600 a 1080p60). Sus primeras 30
imágenes pueden vaciar hasta 120 unidades; en una llamada real habría
pedido una clave y se habría recuperado.

**La captura en CPU** (sin DMA-BUF): escalar la pantalla del portátil
(2560×1600) a 1080p cuesta 2,65 ms por cuadro (`scaling_cost`, arriba), un
16 % de núcleo a 60 cuadros/s en `rune-video-pw`, más la subida a la GPU
que ya está en `src:src`. Es lo que ahorraría DMA-BUF hacia VA-API; no se
hizo (por qué, en
[VIDEO.md](VIDEO.md#la-captura-y-dma-buf)).

## Versión completa, P1–P7 e idiomas (2026-10-17)

Release de `rune/base` 5c1fb3e con `--features session-login`, en la misma
máquina, con el escritorio en uso (video en el navegador) y nada más
compilando. La ventana corrió en un Weston propio sin pantalla
(`--backend=headless --renderer=gl`, 1280×800, GPU real por EGL) con
carpetas XDG temporales, sin tocar el escritorio ni los ajustes del dueño;
siempre con la simulación. CPU y memoria leídas de `/proc/<pid>`: ticks de
usuario más sistema en 20 s, después de 10 s de asentarse.

| Caso | CPU | RSS | PSS |
| --- | --- | --- | --- |
| Quieta en #general (es) | 0,3 % de un núcleo | 187 MB | 132 MB |
| Quieta en #general (en) | 0,2 % | 186 MB | 132 MB |
| Quieta en #galería | 0,1 % | 184 MB | 119 MB |
| Scroll continuo (`--bench`, 600 cuadros, 59,9 cuadros/s) | 40,6 % | 184 MB | — |

`--bench`: vista p50 0,86 ms, layout p50 3,07 ms (p95 5,62), pintado p50
0,23 ms, CPU del cuadro 5,4 ms, primer cuadro a los 225 ms. Frente al
2026-10-14 (layout 1,20 ms, 26 % de un núcleo) el scroll cuesta más: las
filas traen ahora identidad, marcas de rol, degradés y más acciones, y las
condiciones no son iguales (otro compositor y otra GPU para presentar,
navegador reproduciendo video). Sigue holgado dentro de los 16,6 ms de un
cuadro a 60 Hz; queda anotado para revisar el layout de las filas.

**Ver una transmisión** (`encode_bench`, que ahora también decodifica las
unidades que produjo, al mismo ritmo y por `video::decode`, con la
conversión a RGBA): 720p30 3,3 ms por cuadro (10 % de un núcleo), 720p60
3,4 ms (20–21 %), 1080p30 7,5 ms (22 %), 1080p60 6,9 ms por cuadro, unos
41 % de un núcleo a 60 cuadros/s (calculado: el codificador sólo dio 29–31
por segundo, así que no hubo 60 para decodificar). Codificar con el
navegador reproduciendo video dio 27 % (720p30), 71 % (720p60), 77 %
(1080p30) y 99 % con 29–31 cuadros/s (1080p60): peor que la tanda de
arriba, por la carga.

**Audio de una llamada**: el procesamiento local (`processing`) cuesta
36–55 µs por cuadro de 10 ms con supresión de ruido, menos del 1 % de un
núcleo; Opus y la red no están en esa cifra. No se midió una llamada en la
ventana: abriría el micrófono y los parlantes del dueño.

Sin medir, y por qué: memoria con imágenes y GIF (la simulación no trae
vistas previas: «Archivo sin vista previa») y una cuenta real con muchos
servidores. El panel F12 muestra CPU y RSS en vivo para medirlo con la
cuenta.
