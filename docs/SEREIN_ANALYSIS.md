# Análisis de Serein como referencia de arquitectura para RUNE

> Documento de referencia. Serein se estudió como **ejemplo de arquitectura**, no como código para copiar.
> No se compiló ni se ejecutó Serein. No se extrajo, imprimió ni usó ningún token o secreto.
> **Decisión posterior del dueño:** implementar una sesión personal no oficial
> con login temporal propio, descrita en [DISCORD_SESSION.md](DISCORD_SESSION.md).
> Las recomendaciones de excluir ese login que siguen describen el alcance
> original; permanecen excluidas la suplantación de cliente y la evasión.

**Convenciones.** Las rutas son relativas a `references/serein/` (repositorio clonado e ignorado por git en RUNE).
- **[V]**: hecho verificado leyendo el código o los manifiestos, con evidencia `ruta:línea`.
- **[D]**: lo afirma solo la documentación de Serein; no se comprobó en el código.
- **[I]**: inferencia mía.

Donde la documentación y el código se contradicen, manda el código y la contradicción se señala.

---

## Resumen

Serein es un **cliente de escritorio nativo, no oficial, para cuentas de usuario normales de Discord**. Está escrito en Rust y su interfaz usa **egui/eframe sobre wgpu**. Es un workspace Cargo con 13 crates en `crates/`, la aplicación `apps/desktop`, tres herramientas en `tools/` y un SDK de extensiones Wasm en `extensions/sdk`.

**Lo que aporta como referencia de arquitectura (muy valioso):**
- Un único dueño del estado en el hilo de UI. La red entrega eventos etiquetados con una *generación* por colas acotadas, y la UI devuelve comandos tipados.
- Todo está acotado por número de elementos y por bytes: colas, cachés, payloads y texturas.
- Repintado reactivo, sin bucle continuo.
- Virtualización de la lista de mensajes con alturas estimadas y medidas, y anclaje del scroll.
- Una ventana de 500 mensajes en RAM, respaldada por SQLite en modo WAL.
- Gateway con Resume y backoff, y credenciales en el almacén de credenciales del sistema operativo.

**Lo que NO se debe adoptar:**
- Inicia sesión con el **token personal de usuario**, que captura inyectando JavaScript en un webview de `discord.com/login`.
- **Falsifica la identidad de cliente**: User-Agent de Chrome/Safari, `X-Super-Properties`, `client_build_number` fijo. Lo hace con el objetivo declarado de que el anti-spam de Discord no lo detecte.
- Usa opcodes y endpoints solo para usuarios (op 13, 37, 43, 18–20, relaciones, `settings-proto`, `call/ring`, etc.) e implementa voz y vídeo para cuentas de usuario.

Su propio README advierte que esto viola los Términos de Servicio de Discord. Además, su README y su documentación **afirman que no falsifica la huella del cliente, y el código lo contradice**.

**Licencia:** el código propio es `MIT OR Apache-2.0`. Copiar código a un proyecto MIT/Apache-2.0 está permitido conservando el aviso de copyright y el texto de la licencia. Hay excepciones que **no** deben copiarse: el `hpke-rs` vendorizado (MPL-2.0), los sonidos de Discord (propietarios), Twemoji (CC BY 4.0) y las fuentes (OFL).

---

## Commit analizado

**Reverificación de continuidad (2026-10-14).** El checkout local sigue en el
commit completo indicado abajo y `git status --short` no muestra cambios.
Esta revisión volvió a leer estado, cola de red, almacenamiento, audio y
renderizado; no compiló ni ejecutó Serein. El código propio sigue declarado
como `MIT OR Apache-2.0` (`Cargo.toml:10`, `LICENSE-MIT:3`); los avisos de
terceros se evalúan por recurso, no por la licencia raíz. No se incorporó
código ni activos de la referencia a RUNE.

| Campo | Valor |
|---|---|
| Repositorio | https://github.com/ViceVerse-cz/Serein |
| Commit (HEAD de `main`) | `d1af883c9d7dda90d1261a2d15fc852a5445204d` |
| Fecha del commit | 2026-10-14 01:45:57 +0000 |
| Mensaje | `chore(release): update Homebrew cask for 1.0.0-nightly.20261014.58 [skip ci]` |
| Nº de commits | 880 |
| Toolchain fijada | Rust 1.98.1 (`rust-toolchain.toml:2`), edición 2024, resolver 3 (`Cargo.toml:4`, `Cargo.toml:8`) |
| Tamaño aproximado | ~320 000 líneas de Rust; `crates/ui` sola tiene ~131 000 |

---

## Mapa de módulos

Los miembros del workspace son `["apps/desktop", "crates/*", "tools/*"]` (`Cargo.toml:2`) **[V]**. Las responsabilidades salen del código y de los comentarios `//!` de cada crate, contrastados con `AGENTS.md` y `docs/architecture.md:12-21`.

| Módulo pedido | ¿Existe? | Ruta real | Responsabilidad (verificada) |
|---|---|---|---|
| client-core | Sí | `crates/client-core` | Estado de la sesión con un único dueño en el hilo de UI. Contiene el reductor `State::apply(Envelope)`, los enums `Command` y `Event`, las generaciones, los lifecycles de envío, la navegación, la voz y la huella de cliente (`fingerprint.rs`). `lib.rs:1` dice: "Single UI-thread state owner. Adapters deliver generation-tagged typed events." |
| session-cache | Sí | `crates/session-cache` | Línea temporal acotada en RAM: 500 mensajes / 4 MiB / 1024 mutaciones, con *patches* y *tombstones* (`lib.rs:1`, `lib.rs:5-7`). |
| model | Sí | `crates/model` | Entidades neutrales respecto a la UI, sin E/S de disco ni de red (`lib.rs:1`). |
| discord-protocol | Sí | `crates/discord-protocol` | DTOs del cable y decodificación acotada. "JSON values never become application state" (`lib.rs:1`). |
| discord-api | Sí | `crates/discord-api` | Cliente REST con `reqwest`. Origen fijo, 4 permisos concurrentes, cooldown compartido, cuerpos acotados, cabeceras de huella falsificada (`lib.rs:151-187`, `lib.rs:218`). |
| discord-gateway | Sí | `crates/discord-gateway` | Tarea Tokio del Gateway: JSON con zlib-stream, Hello/heartbeat, Identify/Resume, backoff y señalización de voz (`lib.rs:1`). |
| discord-voice | Sí | `crates/discord-voice` | WebSocket de voz, UDP/RTP, cifrado de transporte, DAVE (E2EE con MLS), Opus, CPAL, AEC/NS/AGC, cámara, captura de pantalla y H.264 (`README.md`, `src/`). |
| local-store | Sí | `crates/local-store` | Caché SQLite acotada y aislada por cuenta. "This is not Discord's authoritative state" (`lib.rs:1`). |
| platform | Sí | `crates/platform` | Almacén de credenciales del SO, webview temporal de login y de captcha, notificaciones, bandeja, hotkeys globales, decodificación de vídeo de adjuntos, enumeración de procesos e IPC local de Rich Presence (`lib.rs:1`, `processes.rs:1-2`, `game_activity.rs:1-3`). |
| ui | Sí | `crates/ui` | Paneles egui, virtualización, Markdown, temas y emoji. Solo emite comandos tipados y no toca la red ni los tokens (`docs/architecture.md:20`, `lib.rs:3959`). |

**Crates y miembros adicionales [V]:**

| Crate | Ruta | Responsabilidad |
|---|---|---|
| `serein` (la app) | `apps/desktop` | Cableado de servicios, runtime Tokio, colas, ciclo de login, hidratación de caché, opciones de eframe y actualizador (`main.rs`, `connection.rs`, `cache.rs`, `credentials.rs`, `voice.rs`). |
| `extensions` | `crates/extensions` | Extensiones locales versionadas en un sandbox **wasmi**, sin acceso a Discord ni a la plataforma (`lib.rs:1`; `docs/threat-model.md:3-20`). |
| `test-support` | `crates/test-support` | Datos sintéticos; "never evidence of live compatibility" (`lib.rs:1`). |
| `replay-bench` | `tools/replay-bench` | Benchmark y *soak test* de reductores. |
| `tray-debug` | `tools/tray-debug` | Prueba offline de la bandeja en Linux (`main.rs:1`). |
| `xtask` | `tools/xtask` | Automatización: checks, empaquetado y licencias. |
| `serein-extension-sdk` | `extensions/sdk` | ABI para autores de extensiones; está en otro workspace (`extensions/Cargo.toml`). |

Además hay forks vendorizados en `vendor/` (`davey`, `gpu-allocator`, `hpke-rs`, `wry`) aplicados con `[patch.crates-io]` en `Cargo.toml:187-196` **[V]**.

**Dependencias entre crates [V]** (sección `[dependencies]` de cada `Cargo.toml`):
- `model` no depende de nadie (solo de serde y zeroize).
- `session-cache` depende de `model`.
- `client-core` depende de `model` y `session-cache`.
- `discord-api` y `discord-gateway` dependen de `model`, `client-core` y `discord-protocol`.
- `ui` depende de `model`, `client-core` y `extensions`. **No depende de ningún transporte.**
- `apps/desktop` lo une todo.

---

## Coordinación asíncrona

### Runtimes e hilos

**Runtime principal [V].**
- Un único runtime Tokio *multi-thread* con solo **2 workers** (`apps/desktop/src/main.rs:1448-1451`).
- Se crea dentro del closure de `eframe::run_native` (`main.rs:574`) y es propiedad del `Desktop`.
- Las tareas de conexión y de voz se lanzan sobre él (`apps/desktop/src/connection.rs:160`, `apps/desktop/src/voice.rs:1622`).

**Runtimes locales [V].** Hay runtimes *current-thread* propios de ciertos hilos: audio de adjuntos (`audio.rs:850`), temporizador de voz (`crates/discord-voice/src/timer.rs:105`), captura en Linux (`crates/discord-voice/src/screen/linux.rs:100`) y extensiones (`extensions.rs:1546`).

**Hilos `std` dedicados [V]** para todo lo bloqueante:

| Hilo | Función | Evidencia |
|---|---|---|
| Almacenamiento | Dueño exclusivo de la conexión SQLite | `apps/desktop/src/cache.rs:319-366` |
| Credenciales | Llamadas al keyring del SO | `apps/desktop/src/credentials.rs:51-71` |
| `avatar-cache` | Descarga y decodificación de imágenes con `block_on` sobre el runtime principal | `apps/desktop/src/avatars.rs:469-492` |
| `voice-audio` | DSP de audio | `crates/discord-voice/src/audio.rs:195-197` |
| Vídeo | `remote-video`, `screen-encoder`, `serein-camera` | `video_receive.rs:493`, `screen.rs:159`, `camera.rs:99` |
| Otros | descargas, notificaciones, extensiones, instancia única | `downloads.rs:161`, `notification_sounds.rs:42`, `extensions.rs:453`, `platform/src/single_instance.rs:157` |

**Decodificación [V].** Usa `spawn_blocking` detrás de un semáforo global de **8 permisos** (`apps/desktop/src/avatars.rs:1420-1435`).

### Canales y capacidades [V]

| Flujo | Tipo | Capacidad | Evidencia |
|---|---|---|---|
| Red → UI, eventos fiables | `tokio::mpsc` + semáforo de bytes | 4008 elementos, 32 MiB, máx. 4 MiB por evento | `connection.rs:881-911` |
| Red → UI, *typing* (con pérdida) | `tokio::mpsc` | 8 | `connection.rs:132` |
| Estados de presencia, juego, fallo terminal | `tokio::sync::watch` | último valor | `connection.rs:133-150` |
| UI → red, comandos | `tokio::mpsc` | 16 (`COMMAND_SLOTS`) | `connection.rs:129`, `crates/client-core/src/lib.rs:69` |
| UI → red, subidas | `tokio::mpsc` | 1 | `connection.rs:130` |
| Cola serial de escrituras de mensajes | `tokio::mpsc` | 16 | `connection.rs:254-263` |
| Control de voz del Gateway | `tokio::mpsc` | 8 | `connection.rs:194` |
| Reconexión inmediata | `tokio::sync::Notify` | — | `connection.rs:156` |
| UI ↔ almacenamiento | `std::sync::mpsc::sync_channel` | 16 en cada sentido + presupuesto de 48 MiB | `cache.rs:13`, `cache.rs:320-321` |
| UI ↔ credenciales | `sync_channel` | 4 en cada sentido | `credentials.rs:49-50` |
| Imágenes | `tokio::mpsc` | 1024 peticiones; 128 resultados / 128 MiB | `avatars.rs:29`, `avatars.rs:457` |
| Audio en tiempo real | `rtrb` (SPSC sin locks) | 8 tramas | `crates/discord-voice/src/audio.rs:630-631`, `audio.rs:733` |

No se usan `crossbeam` ni `CancellationToken`/`tokio-util` (búsqueda sin resultados) **[V]**.

### Cómo se despierta la UI [V]

- Cada productor llama a `ctx.request_repaint()` justo después de encolar: eventos (`connection.rs:966`), almacenamiento (`cache.rs:357`), credenciales (`credentials.rs:69`) e imágenes (`avatars.rs:381`).
- El consumidor drena como máximo `EVENT_SLOTS = 8` eventos fiables y 8 de *typing* por frame (`main.rs:5743-5757`). Si queda trabajo, vuelve a pedir repintado (`connection.rs:893-899`).
- Los plazos usan `request_repaint_after`. Por ejemplo, durante una llamada hay un latido de 1 s "so a call does not repaint at 20 Hz" (`main.rs:6471-6475`).

### Arquitectura de servicios y cancelación [V]

`Connection::start` (`connection.rs:120`) lanza una tarea raíz. Esa tarea:
1. Autentica.
2. Arranca el Gateway (`connection.rs:211-252`).
3. Arranca el *worker* serial de escrituras REST, para que un envío lento no bloquee colgar o silenciar una llamada (`connection.rs:254-263`).
4. Arranca la presencia, la actividad y Spotify (`connection.rs:177-199`).
5. Entra en un `select!` (`connection.rs:283-641`) con **una ranura reemplazable `AbortTask` por tipo de lectura**: historial, perfil, búsqueda, GIFs, etc. (`connection.rs:264-279`). Una petición nueva aborta la anterior al soltarla (`connection.rs:95-100`, `connection.rs:598-607`).

**Concurrencia REST [V].** `Semaphore::new(4)` (`crates/discord-api/src/lib.rs:218`).

**Apagado [V].**
- `Drop for Connection` hace `task.abort()` (`connection.rs:90-94`).
- Los hilos de almacenamiento y credenciales terminan cuando se cierra su canal (`cache.rs:328`, `credentials.rs:52`).
- **[I]** Las tareas hijas mueren con la raíz porque sus `AbortTask` son variables locales del futuro raíz.

**Sobrecarga = cortar la conexión, no perder eventos [V].** Si la cola fiable se llena, `emit_event` devuelve `"Account synchronization event queue is full; connection stopped"` (`connection.rs:955-966`). La UI suelta la conexión y marca como `Ambiguous` los mensajes que estaban en `Sending` (`main.rs:6021-6053`). El *typing*, en cambio, se descarta en silencio (`connection.rs:920-925`). También pasa por un filtro previo: un evento por usuario cada 2 s y solo del canal seleccionado (`connection.rs:970-1005`).

---

## Estado y sincronización

### Patrón: dueño único + reductor [V]

**Tipos clave** (`crates/client-core/src/lib.rs`):
- `pub enum Command` (`lib.rs:71`, unas 220 líneas de variantes).
- `pub enum Event` (`lib.rs:424`).
- `pub struct Envelope { generation: u64, event: Event }` (`lib.rs:644-647`).
- `pub struct State` (`lib.rs:677`).

**Reductor.** `State::apply(&mut self, Envelope)` (`lib.rs:2323`). Lo primero que hace es descartar el sobre si `envelope.generation != self.generation` (`lib.rs:2332-2334`).

**Generaciones.** La generación se incrementa al conectar y al cerrar sesión (`main.rs:2391`, `main.rs:2420-2423`, `lib.rs:1084-1089`). Cada evento emitido se envuelve con la generación con la que arrancó la conexión (`connection.rs:167`). Así, cualquier respuesta de una sesión anterior se ignora sola.

**Dentro de una generación** se usan identificadores de petición. Por ejemplo, `Event::History` solo se aplica si el canal sigue seleccionado, si `request == self.request` y si había historial pendiente (`lib.rs:3297-3311`). Las credenciales llevan su propio token de petición (`credentials.rs:33-37`).

**Vuelta de la UI.** `MessagingUi::show(&mut self, ui, state: &mut State) -> Vec<Command>` (`crates/ui/src/lib.rs:3959`), llamada en `main.rs:6844`. Los comandos se despachan con `try_send`. Si la cola está llena, el estado registra `command_rejected`: un envío muestra "Work queue full" y el resto marca el estado como `Stale` (`lib.rs:2303-2321`, `main.rs:4400-4404`).

**No hay snapshot copiado por frame.** La UI toma prestado `&mut State` directamente. Las cachés de render se invalidan con contadores de revisión (`crates/client-core/src/view_revisions.rs:15-30`).

### Por qué funciona (inferencia)

**[I]** Al no compartir el estado (`Arc<Mutex<State>>`) entre hilos, no hay contención ni bloqueos en el render. Todo el razonamiento sobre consistencia se concentra en un reductor determinista que se puede probar con datos sintéticos; `tools/replay-bench` existe para eso. El coste es un `State` "dios" muy grande: `client-core/src/lib.rs` tiene 6151 líneas y `apps/desktop/src/main.rs` tiene 7840.

---

## Caché y persistencia

### RAM: `session-cache` [V]

**Límites.** `MAX_MESSAGES = 500`, `MAX_BYTES = 4 MiB`, `MAX_MUTATIONS = 1024` (`crates/session-cache/src/lib.rs:5-7`). Es **una sola ventana activa**. El historial inactivo vive en SQLite (`docs/architecture.md:15`).

**Estructura.** `Timeline` guarda `BTreeMap<Id, Option<Message>>`, donde `None` es un *tombstone* posicional, junto con los *patches* y los borrados (`lib.rs:21-34`).

**Expulsión.** Al paginar hacia atrás se expulsa el mensaje más nuevo (`pop_last`) para conservar el ancla de lectura; en el resto de casos, el más viejo (`lib.rs:267-276`).

**Ediciones durante una carga.** Las ediciones que llegan mientras se carga una página se guardan como *patches* (máx. 1 MiB) y se aplican al llegar la página (`lib.rs:224-226`, `lib.rs:372-454`).

### Disco: `local-store` (SQLite) [V]

**Motor.** `rusqlite =0.40.2` con `bundled` (SQLite incluido) y `cache` (`Cargo.toml:38`). Una única conexión pertenece al hilo de almacenamiento (`crates/local-store/src/lib.rs:34`, `apps/desktop/src/cache.rs:319-366`).

**Ubicación.** `dirs::data_local_dir()/serein/client.sqlite3`, que se puede sobrescribir con `SEREIN_DATA_DIR`. Los directorios se crean con modo 0700 (`local-store/src/lib.rs:205-236`).

**PRAGMAs** (`local-store/src/lib.rs:286`):
- `journal_mode=WAL`, `synchronous=NORMAL`.
- `max_page_count=16384`, con páginas de 4 KiB: un **techo duro de 64 MiB**.
- `secure_delete=ON`, `auto_vacuum=INCREMENTAL`.
- `wal_autocheckpoint=256`, `journal_size_limit=8 MiB`.

**Esquema.**
- Tabla `messages(account, channel, id, …)` con clave primaria `(account, channel, id)` (`lib.rs:287-288`).
- Tablas `channels`, `drafts`, `appearance`, `accounts`, preferencias, etc. (`lib.rs:287-293`, `lib.rs:431-488`).
- Los datos anidados (embeds, adjuntos, reacciones, componentes) se guardan como **JSON en columnas TEXT** con límites de tamaño (`lib.rs:947-1061`).
- **Todo va con clave de cuenta**, así que las cuentas quedan aisladas.

**Migraciones.** Usa `PRAGMA user_version` (versión 29) junto con `pragma_table_info` y `ALTER TABLE` dentro de una transacción. Rechaza un esquema más nuevo que el que conoce (`lib.rs:14-15`, `lib.rs:282-285`, `lib.rs:295-595`).

**Límites globales.**
- 20 canales y 48 MiB de contenido. Se expulsa el canal tocado hace más tiempo y luego se ejecuta `incremental_vacuum` (`lib.rs:1064-1101`).
- Borradores: 8 KiB cada uno, 64 en total (`lib.rs:1334-1358`).
- Al guardar se omiten las filas que no cambiaron (`lib.rs:948-954`).

**Cifrado en reposo: no hay.** Está documentado como riesgo abierto en `README.md:410` y `docs/threat-model.md:28`.

**Cierre de sesión.** `forget_account` borra las tablas de la cuenta, trunca el WAL y hace vacuum (`lib.rs:1560-1580`).

### Caché de imágenes en disco [V]

- **Ruta.** `<data_dir>/avatars/<account_id>/` (`apps/desktop/src/avatars.rs:424-435`).
- **Límites.** 1 GiB / 4096 archivos por defecto, configurable entre 256 MiB y 4 GiB; retención de 90 días (`crates/model/src/cache_preferences.rs:6-10`, `cache_preferences.rs:44-74`).
- **Formato.** Bytes originales. Los nombres son el SHA-256 de la URL, siempre con la extensión `.png`. Los archivos se escriben como `.part` con modo 0600 y luego se renombran (`avatars.rs:1064-1083`, `avatars.rs:2147-2189`).
- **Expulsión.** LRU por `mtime` (`avatars.rs:2191-2241`).

**Discrepancia documental [V].** `docs/architecture.md:27` dice que el presupuesto del *worker* de almacenamiento es de 16 MiB, pero el código usa 48 MiB (`apps/desktop/src/cache.rs:12-13`).

### Persistencia bajo concurrencia: qué conservar del patrón [V]

La revisión de continuidad encontró una distinción que importa más que el
motor SQLite: **el orden de los eventos y la validez de los resultados también
forman parte del contrato de almacenamiento**.

- `Cache::queue` reserva bytes sin esperar y usa `try_send`; devuelve `false`
  si no puede admitir el trabajo (`apps/desktop/src/cache.rs:233-318`). El
  límite es tanto de 16 trabajos como de 48 MiB. El productor debe resolver
  ese rechazo. Las respuestas también están acotadas, y la espera ocurre
  dentro del trabajador (`:320-357`), fuera de la UI.
- Un trabajo lleva `(generation, account, epoch, operation, reservation)`;
  `HistorySafety` invalida la época ante un borrado y puede bloquear el uso
  del historial mientras una limpieza no se confirma (`:145-179`,
  `:216-232`). Así, numerar la conexión de red y numerar la validez de la
  caché resuelven problemas distintos.
- El host acumula IDs afectados durante el drenaje, aplica cada evento al
  estado y, al terminar, obtiene los mensajes que va a persistir del
  **timeline resultante** (`apps/desktop/src/main.rs:5770-5772`,
  `:5884`, `:5996-6020`). No guarda ciegamente todos los cuerpos recibidos
  antes de un borrado posterior en el mismo lote.

**Aplicación a RUNE [I].** La implementación propia puede ser mucho menor:
un lote de efectos persistentes que preserve su orden, o un cambio final por
mensaje, más identificadores para descartar lecturas obsoletas. La prueba
decisiva es crear/editar y borrar el mismo mensaje en un drenaje, reiniciar y
comprobar que no reaparece. El detalle de generación/época queda para cuando
las lecturas y cambios de cuenta realmente lo requieran; no exige trasladar
la estructura completa de Serein.

---

## Renderizado y virtualización

### Framework de UI [V]

- **egui/eframe 0.36.2** desde git, rev `bb622334e2f58afd84f6bb61980824f480c7e3d4` (`Cargo.toml:23-24`; `Cargo.lock:1797-1799`).
- Backend **wgpu** (`eframe::Renderer::Wgpu`, `apps/desktop/src/main.rs:530`); no usa glow.
- `PresentMode::Fifo` con `SurfaceConfig::LOW_LATENCY` (`main.rs:563-567`).
- En Windows, DX12 por defecto (`main.rs:536-546`).
- Accesibilidad con `accesskit`.

### Renderizado eficiente [V]

- **Modo reactivo.** Solo se repinta cuando un productor despierta la UI (`main.rs:6080`, `cache.rs:357`). El repintado continuo se limita a las animaciones de scroll (`crates/ui/src/timeline.rs:2531`, `timeline.rs:2543`).
- **Decodificación fuera del hilo de UI** con 8 ranuras globales (`avatars.rs:1420-1435`). La UI consume como máximo 32 resultados por frame (`main.rs:5376`).
- **Texturas con presupuesto por pool y expulsión** (`crates/ui/src/avatars.rs:450-467`, `crates/ui/src/avatars/media.rs:22-34`):
  - avatares: 512 / 64 MiB
  - emoji: 1024 / 16 MiB
  - imágenes inline: 384 / 96 MiB
  - animaciones: 96 / 128 MiB
- **Animaciones.** Reutilizan una sola textura con `texture.set(...)` y suben como mucho un frame cada 16 ms (`crates/ui/src/avatars.rs:109-125`).
- **Markdown.** `pulldown-cmark` limitado a 8192 bytes y 512 eventos (`crates/ui/src/markdown.rs:9-10`). Caché MRU de 512 entradas / 1 MiB por `(Id, u16)` (`markdown.rs:227-276`).
- **Vídeo.** Los decodificadores entregan planos NV12/I420 y un *render pass* de wgpu los convierte a RGBA: se sube 1,5 B/píxel en lugar de 4 (`apps/desktop/src/gpu_video.rs:1-5`).

### Virtualización de la lista de mensajes [V]

Todo está en `crates/ui/src/timeline.rs`, un archivo de 10 827 líneas.

**Estado.** `TimelineView` guarda:
- `rows: Vec<(Id, f32)>`
- `heights: BTreeMap<Id, (u64 key, f32)>`
- `anchor: Option<(Id, f32 inset)>`
- `following`

(`timeline.rs:112-127`).

**Selección de filas.**
- `visible_range` recorre las alturas de forma **lineal**, sumándolas hasta `min` y luego hasta `max` (`timeline.rs:176-190`). No usa sumas prefijas; con un tope de 500 filas es aceptable.
- Se usa dentro de `ScrollArea::vertical().show_viewport` con 100 px de *overscan* (`timeline.rs:2493-2497`, `timeline.rs:2585-2595`).
- Las filas por encima del viewport se maquetan en un `Ui` hijo con un *clip* de altura cero: se miden sin pintarlas (`timeline.rs:2598-2614`).
- Lo que queda por debajo se rellena con `add_space` (`timeline.rs:4192-4193`).

**Alturas.**
- Hay una **estimación** a partir del número de líneas, embeds, adjuntos y reacciones (`timeline.rs:2253-2275`).
- Encima va una **caché de medidas** cuya clave es un hash del contenido, el agrupamiento, los separadores y las menciones (`timeline.rs:1049-1073`, `timeline.rs:319-358`).
- Si cambian la fuente, la escala o el modo compacto, se invalida todo (`timeline.rs:2179-2186`).
- Si cambia el ancho, solo se vuelven a medir las filas visibles (`timeline.rs:2207-2218`).
- Una medida nueva fuerza otra pasada de layout con `request_discard` (`timeline.rs:4380-4399`).

**Anclaje al cargar páginas antiguas.**
- Cada frame guarda `(id de la primera fila visible, inset en píxeles)` (`timeline.rs:4254-4278`).
- Al reconstruir las filas, restaura el offset con `anchor_offset`, que hace un `partition_point` por `Id` y suma las alturas anteriores (`timeline.rs:302-318`, `timeline.rs:2300-2306`).
- La paginación hacia atrás solo se dispara con scroll explícito a menos de 160 px del borde superior (`timeline.rs:4401-4409`).

**Discrepancia documental.** La documentación dice que la caché de alturas incluye el ancho. En realidad el ancho no forma parte de la clave: se trata aparte, mediante `measured_rows`.

---

## Reconexión

### Gateway principal (`crates/discord-gateway/src/lib.rs`) [V]

- **URL.** Se obtiene de REST `/gateway` y se valida contra `gateway.discord.gg` o `gateway-*.discord.gg`. Después se fuerza `v=10&encoding=json&compress=zlib-stream` (`lib.rs:71-87`).
- **Hello.** Debe llegar en 10 s. El primer heartbeat se retrasa con un *jitter*. Si falta el ACK antes del siguiente tick, la conexión se da por muerta (`lib.rs:1229-1301`, `lib.rs:175-183`).
- **Identify o Resume.** Si hay `session_id` y `seq`, se envía op 6 (Resume); si no, op 2 (Identify) (`lib.rs:1272-1280`). El handshake se pone a cero con zeroize (`lib.rs:1281`). Tras 3 Resume fallidos se vuelve a Identify (`lib.rs:146-154`).
- **Opcodes de servidor.** op 7 provoca Resume (`lib.rs:1589`). op 9 con `d=false` borra la sesión y reintenta tras 1–5 s (`lib.rs:1590-1596`).

**Códigos de cierre** (`lib.rs:64-70`):

| Código | Acción |
|---|---|
| 4004 | Credencial caducada; se detiene |
| 4010–4014, 1008, 1009 | Se detiene |
| 4007, 4009 | Identify nuevo |
| Cualquier otro | Resume |

**Backoff.**
- Retardo `(1000 << min(intento, 5)) ms + jitter(0–1000 ms)`, con un tope aproximado de 32 s (`lib.rs:1189-1191`).
- Antes del primer READY hay como máximo 6 intentos; después, reintentos indefinidos (`lib.rs:1182`).
- El contador se reinicia tras 60 s de conexión estable (`lib.rs:192-199`).
- **[V]** El *jitter* sale de los nanosegundos del reloj, no de un RNG (`lib.rs:270-276`).

**Recuperación manual.** Un `Notify` interrumpe el backoff, el *dial* o la espera del Hello, pero solo después del primer READY (`lib.rs:1188-1222`). Lo disparan "Reconnect now", Refresh o un envío explícito (`apps/desktop/src/connection.rs:103-111`).

**Tras RESUMED.** El estado se marca *stale*, se cancela el historial pendiente (`crates/client-core/src/lib.rs:3666-3672`) y se vuelve a pedir la página visible (`apps/desktop/src/main.rs:5986-5996`). Un READY nuevo emite `Resync` y limpia la línea temporal y los permisos.

**Compresión.** Usa zlib-stream con flate2, comprobando el sufijo `00 00 FF FF` y con un tope de 64 MiB (`compression.rs:1-73`). **Discrepancia [V]:** `docs/architecture.md:17` y `docs/discord-compatibility.md:914` dicen "JSON without compression", pero el código pide zlib-stream (`lib.rs:85`).

### Voz [V]

- **Resume del WebSocket de voz.** Hasta 2 intentos, solo si ya existe la clave de transporte. Usa op 7 con `seq_ack` y un plazo de 30 s (`crates/discord-voice/src/transport.rs:397-425`).
- **Rejoin.** Después, el escritorio hace hasta 3 *rejoin* completos a 1, 3 y 10 s (`apps/desktop/src/voice.rs:298-302`). El presupuesto se repone tras 30 s estables (`voice.rs:304`).
- **Cierre 4014** (expulsado o movido): se abandona la llamada (`transport.rs:775`, `voice.rs:631-662`).

---

## Multimedia

Todo está en `crates/discord-voice`. `crates/platform` no captura pantalla ni cámara: solo decodifica vídeo de adjuntos (`crates/platform/src/video/mod.rs:1-37`) **[V]**.

**Señalización [V].**
- Endpoint `wss://{host}/?v=8`. El host debe terminar en `.discord.media` (`transport.rs:59-75`).
- Opcodes JSON 0, 1, 3, 5, 7, 12 y 15, más los de DAVE 21–31. Los opcodes binarios de DAVE son 25–30 (`transport.rs:332-770`, `crypto.rs:473-538`).

**Transporte [V].**
- Descubrimiento de IP por UDP; se exige que la IP sea pública (`transport.rs:76-100`, `transport.rs:219-236`).
- RTP con PT 120 para Opus y PT 101/102 para H.264/RTX.
- Cifrado **solo `aead_xchacha20_poly1305_rtpsize`** (`crypto.rs:14`).
- **DAVE E2EE** con `davey 0.1.4` sobre `openmls 0.8.1` (`crates/discord-voice/Cargo.toml:19-20`).
- No se envía media hasta que DAVE está listo y no hay *downgrade* (`transport.rs:456`, `transport.rs:634`, `transport.rs:695`).

**Audio [V].**
- Trama: `[f32; 960]`, es decir 20 ms a 48 kHz mono (`lib.rs:26`).
- Los *callbacks* de CPAL solo hacen *downmix*, remuestreo lineal y `push`/`pop` en `rtrb` de 8 tramas, sin locks (`audio.rs:1-2`, `audio.rs:630-632`, `audio.rs:733`).
- El DSP corre en el hilo `voice-audio` en este orden (`audio/echo.rs:17-163`, `audio.rs:377-413`):
  1. Sonora (port en Rust de WebRTC AEC3) + supresión de ruido WebRTC
  2. RNNoise (`nnnoiseless`)
  3. AGC2
  4. Ganancia
  5. *Gate*
- Opus mono VoIP a 64 kbit/s (`transport.rs:372-376`).
- Jitter buffer de 8 ranuras con arranque de 40 ms (`jitter.rs:1-6`).
- La mezcla suma y recorta, sin limitador (`mixer.rs:85-151`).
- Opus, DAVE y la red corren en el hilo `serein-voice`, con un tick de 20 ms (`timer.rs:95-128`).

**Vídeo [V].**
- Cámara a 640×480 y 15 fps (`camera.rs:23-25`).
- Pantalla a 480p, 720p o 1080p y 15/30/60 fps (`crates/client-core/src/screen.rs:36-52`).
- Codificación H.264 por hardware; si no hay, OpenH264 compilado desde el código fuente (`camera.rs:158-200`).
- Captura por plataforma: ScreenCaptureKit en macOS, Windows Graphics Capture en Windows, y portal + PipeWire + GStreamer en Linux.
- Empaquetado FU-A con MTU de 1200 (`video.rs:5-26`).

**Otros [V].** Symphonia (MPL-2.0) reproduce adjuntos MP3/WAV/Ogg y los sonidos de notificación (`apps/desktop/src/audio.rs:490-494`, `notification_sounds.rs:118-140`).

**Diagrama del flujo [V]** (hilos y colas de `apps/desktop/src/voice.rs:1534-1541`):

```
mic → callback CPAL (downmix, 48 kHz) → rtrb(8) → hilo "voice-audio" (AEC3/NS/RNNoise/AGC2)
    → sync_channel(8) → hilo "serein-voice" (tick 20 ms: Opus → DAVE → RTP → XChaCha) → UDP
UDP → "serein-voice" (XChaCha → DAVE → jitter → Opus decode + mezcla) → sync_channel(8)
    → "voice-audio" → rtrb(8) → callback CPAL de salida → altavoces (+ referencia para AEC)
```

---

## Credenciales

**Tipo de credencial [V].** Es **exclusivamente el token personal de usuario**. Se envía en `Authorization` sin prefijo (`crates/discord-api/src/lib.rs:359-361`). Se rechazan los prefijos `Bot ` y `Bearer ` (`crates/client-core/src/auth.rs:84-91`), y las cuentas bot se rechazan en `/users/@me` y en READY (`discord-api/src/lib.rs:519`, `discord-gateway/src/lib.rs:1602`). No existe ruta OAuth2 ni de bot (`docs/authentication.md:54`).

**Cómo se obtiene el token [V].**
- Webview temporal de `https://discord.com/login`: Wry en modo incógnito y sin devtools (`crates/platform/src/lib.rs:306-326`), o GTK4/WebKit6 con sesión efímera en Linux (`crates/platform/src/login_linux.rs:121-129`).
- `login-handoff.js` **parchea `XMLHttpRequest.prototype.setRequestHeader` y `window.fetch`** para capturar la primera cabecera `Authorization` enviada a `discord.com/api/v*/`. La envía por IPC con un prefijo de capacidad aleatorio de 32 bytes (`crates/platform/src/login-handoff.js:21-38`, `platform/src/lib.rs:297-305`).
- En Linux también se lee la cabecera en `resource_load_started` (`login_linux.rs:164-185`).
- El webview caduca a los 600 s.
- Email/contraseña, QR y MFA los resuelve la página oficial de Discord. Serein no implementa MFA (**[V]**, búsqueda sin resultados).

**Pegar un token [V].** Existe la opción "Sign in with a token" (`apps/desktop/src/main.rs:5238-5264`, `docs/authentication.md:66`). Recorta las comillas tal como aparecen en el almacenamiento del navegador (`auth.rs:76-93`). **[I]** En la práctica esto facilita pegar un token sacado de las devtools del navegador, aunque el texto de la interfaz lo enmarque como "de otra instalación de Serein".

**Almacenamiento [V].**
- `keyring-core` con el servicio `cz.viceverse.serein` (`crates/platform/src/lib.rs:43-47`, `lib.rs:150-174`).
- Backends:
  - Linux: Secret Service mediante `zbus-secret-service-keyring-store`, con activación de KWallet por D-Bus (`lib.rs:191-248`).
  - macOS/Windows: `keyring 4.2.0` sobre Keychain y Credential Manager.
- Un único hilo serializa las operaciones de lectura, guardado y borrado (`apps/desktop/src/credentials.rs:1-80`).
- **El token se guarda solo después de READY** (`main.rs:5946-5957`). Si no hay keyring, la sesión funciona solo en memoria y nunca se escribe en texto plano.

**En memoria [V].** `SessionSecret` es un `Zeroizing<String>`, no implementa `Clone` y su `Debug` está redactado (`auth.rs:73-102`). La cabecera se marca `set_sensitive(true)` (`discord-api/src/lib.rs:361`).

**Cierre de sesión [V].** Borra la entrada del keyring y la caché local (`main.rs:2435-2528`). **No revoca la sesión remota**: no se llama a `/auth/logout`, y la documentación lo reconoce (`docs/authentication.md:62`).

---

## Licencias

### Serein [V]

- Las licencias son `LICENSE-MIT` ("Copyright (c) 2026 Serein contributors") y `LICENSE-APACHE` (Apache-2.0 sin `NOTICE`).
- `Cargo.toml:10` declara `license = "MIT OR Apache-2.0"` para el workspace. Todos los miembros lo heredan; `cargo metadata --no-deps` lo confirma para los 17 paquetes.
- `deny.toml:10-22` solo permite licencias permisivas, con **excepciones MPL-2.0 de versión exacta** (`deny.toml:27-130`).

### Dependencias directas destacadas

Licencias declaradas en crates.io para la versión fijada:

| Licencia | Crates |
|---|---|
| MIT o MIT/Apache | `tokio`, `reqwest`, `tokio-tungstenite`, `rusqlite`, `wry`, `keyring`, `keyring-core`, `zeroize`, `davey`, `openmls`, `chacha20poly1305`, `opus2`, `libopus_sys`, `rtrb`, `image`, `gstreamer` (bindings), `gtk4`/`webkit6` (bindings), `screencapturekit`, `windows-capture`, `wasmi`, `pulldown-cmark`, `fluent-templates`, `skrifa`, `rasterlottie`, `resvg`, `arboard`, `notify-rust`, `global-hotkey`, `rfd`, `ashpd`, `flate2`, `dirs`, `zip`, `ruzstd`, egui/eframe |
| Apache-2.0 | `cpal`, `winit` |
| BSD-3-Clause | `sonora`, `nnnoiseless` |
| BSD-2-Clause | `openh264`, `openh264-sys2` (incluye el código de Cisco OpenH264 2.6.0) |
| Unlicense | `ksni` |
| **MPL-2.0** | `symphonia` (y sus subcrates), `hpke-rs` (**vendorizado y modificado**, `vendor/hpke-rs/SEREIN-PATCH.md`), `option-ext`; y de forma transitiva `cssparser`, `selectors`, `dtoa-short` |

### Puntos a vigilar

1. **MPL-2.0 (copyleft débil, por archivo).** Usar estos crates como dependencia es compatible con MIT/Apache. Pero **si se copian o modifican sus archivos**, esos archivos siguen siendo MPL-2.0 y hay que ofrecer su código fuente. Es el caso de `vendor/hpke-rs`: no copiarlo a RUNE.
2. **Bibliotecas del sistema LGPL.** GTK4/WebKitGTK/GStreamer se enlazan dinámicamente y no se incluyen (`THIRD_PARTY_NOTICES.md:94-99`, `THIRD_PARTY_NOTICES.md:254-259`).
3. **OpenH264 compilado desde el código fuente.** No tiene la cobertura de patentes H.264 que Cisco ofrece para su binario, como reconoce `THIRD_PARTY_NOTICES.md:181-192`. **[I]** Es un riesgo de patentes, no de copyright.
4. **Sonidos de notificación de Discord Inc.** Son propietarios, se descargaron del CDN de Discord y su redistribución está **pendiente de permiso** (`assets/sounds/README.md:3-24`, `THIRD_PARTY_NOTICES.md:128`). **No copiar.**
5. **Activos.** Twemoji es CC BY 4.0 y exige atribución (`THIRD_PARTY_NOTICES.md:146`); Inter y Noto son OFL-1.1 (`THIRD_PARTY_NOTICES.md:138`).
6. **Fuentes de protocolo consultadas.** Abaddon (GPL-3.0, **[I]** por su licencia conocida), Discord Userdoccers, discord.py-self y un *datamining* del cliente móvil oficial descompilado (`THIRD_PARTY_NOTICES.md:126`, `docs/discord-compatibility.md:10-11`, `docs/discord-compatibility.md:946`). Serein declara que no copió código de implementación (**[D]**, no verificable por completo). Los hechos de protocolo en sí no son objeto de copyright.

### Veredicto

**Sí se permite copiar código original de Serein a un proyecto MIT o Apache-2.0**, porque Serein tiene licencia dual. Requisitos:
- Bajo MIT: conservar el aviso "Copyright (c) 2026 Serein contributors" y el texto MIT en las copias sustanciales.
- Bajo Apache-2.0: incluir la licencia, conservar los avisos y marcar los archivos modificados (§4b).
- Recomendación práctica: añadir a RUNE un `THIRD_PARTY_NOTICES` con el commit de origen.

Quedan excluidos: `vendor/hpke-rs` (MPL), `vendor/*` en general (cada uno con su licencia), los sonidos de Discord, Twemoji sin atribución y las fuentes.

**Recomendación:** que la licencia lo permita no significa que convenga. Gran parte del código está acoplado a mecanismos no oficiales (véase la sección siguiente). En RUNE conviene **reimplementar las ideas, no copiar el código**.

---

## Integraciones no oficiales y riesgos

Los Términos de Servicio y la Developer Policy de Discord prohíben automatizar cuentas de usuario ("self-bots") y el uso de clientes modificados o de terceros. Serein lo reconoce en `README.md:39-40`, `docs/threat-model.md:28` y `docs/discord-compatibility.md:894`. Estos son los mecanismos encontrados:

| # | Mecanismo | Evidencia [V] | Riesgo |
|---|---|---|---|
| 1 | **Login con el token personal de usuario** (no OAuth2 ni bot). Las cuentas bot se rechazan explícitamente. | `crates/discord-api/src/lib.rs:359-361`; `crates/client-core/src/auth.rs:84-91`; `docs/authentication.md:54` | Cliente de terceros sobre una cuenta de usuario: suspensión de la cuenta. |
| 2 | **Captura del token inyectando JS** en la página oficial de login (parchea XHR y `fetch`). En Linux, además, se observan las cabeceras de las peticiones. | `crates/platform/src/login-handoff.js:21-38`; `crates/platform/src/lib.rs:306-326`; `crates/platform/src/login_linux.rs:164-185` | Extracción de credenciales de una sesión web; frágil y contraria a la política. |
| 3 | **Opción para pegar un token** (recorta las comillas del almacenamiento del navegador). | `apps/desktop/src/main.rs:5238-5264`; `crates/client-core/src/auth.rs:76-93`; `docs/authentication.md:66` | Fomenta copiar tokens desde las devtools. |
| 4 | **Huella de cliente falsificada para evadir el anti-spam.** Chrome 128 o Safari 18.5, `client_build_number = 328246` fijo, `release_channel: "stable"`. El comentario lo dice textualmente: "keeps the session classified as an ordinary web client". | `crates/client-core/src/fingerprint.rs:1-7`, `fingerprint.rs:10-14`, `fingerprint.rs:57-92` | **Alto.** Suplantación deliberada del cliente oficial. |
| 5 | **Cabeceras REST falsificadas:** `X-Super-Properties`, `X-Discord-Locale`, `X-Discord-Timezone: UTC`, `Origin`/`Referer` de discord.com y el User-Agent falso, también en las subidas. | `crates/discord-api/src/lib.rs:151-187`; `crates/discord-api/src/upload.rs:476` | Igual que el punto 4. |
| 6 | **Identify del Gateway con `properties` falsificadas** ("the same browser fingerprint as REST"). | `crates/discord-gateway/src/lib.rs:1275-1279` | Igual que el punto 4. |
| 7 | **User-Agent falsificado en los webviews** de login (Linux) y de captcha, para que hCaptcha y Discord acepten el desafío. | `crates/platform/src/login_linux.rs:129`; `crates/platform/src/captcha_linux.rs:96`; `crates/platform/src/captcha.rs:56`; `docs/authentication.md:37-39` | Elude el rechazo del motor de captcha. |
| 8 | **hCaptcha cargado con `host=<sitekey>.react-native.hcaptcha.com`.** **[I]** Hace que el widget se presente como el SDK de React Native. | `crates/platform/src/captcha.js:52` | **[I]** Falsificación de origen del captcha. Los captchas los resuelve el usuario y no se eluden (`docs/invite-verification.md`). |
| 9 | **Reenvío del captcha** con `x-captcha-key`/`x-captcha-rqtoken`/`x-captcha-session-id`, para unirse a invitaciones y enviar solicitudes de amistad. | `crates/discord-api/src/lib.rs:109-150`, `lib.rs:378-388`, `lib.rs:455-465` | Flujos anti-abuso pensados para el cliente oficial. |
| 10 | **Opcodes del Gateway exclusivos del cliente de usuario o no documentados para bots:** op 13 (sincronizar llamada), op 37 (suscripciones de guild en bloque), op 43 (info de canal de voz), op 18/19/20 (streams), op 4 de voz desde una cuenta de usuario (`guild_id: null` en DMs) y op 8. Consume READY_SUPPLEMENTAL, SESSIONS_REPLACE, USER_SETTINGS_PROTO_UPDATE, GUILD_MEMBER_LIST_UPDATE y CALL_*. | `crates/discord-gateway/src/voice.rs:297`, `voice.rs:418`, `voice.rs:445`, `voice.rs:501`; `crates/discord-gateway/src/lib.rs:361-381`; `crates/discord-gateway/src/member_search.rs:108`; `lib.rs:1689`, `lib.rs:1808` | Protocolo no documentado: puede cambiar o ser detectado. |
| 11 | **Endpoints REST solo para usuarios:** relaciones (amistades y bloqueos), notas, `settings-proto/1` y `/2`, perfil, `ack` de lectura, búsqueda de mensajes, `call/ring` y `stop-ringing`, aceptar invitaciones (`POST /invites/{code}`), `POST /interactions` y `application-command-index` desde una cuenta de usuario, el *access token* de Spotify de las conexiones, `applications/detectable` y `members-search`. | `crates/discord-api/src/user_actions.rs:60`, `user_actions.rs:93-221`; `crates/discord-api/src/activity_sharing.rs:9-80`; `crates/discord-api/src/lib.rs:532-584`, `lib.rs:1139`, `lib.rs:1449-1518`; `crates/discord-api/src/interactions.rs:80-191`; `crates/discord-api/src/spotify.rs:91`; `crates/discord-api/src/detectable.rs:57`; `crates/discord-api/src/server_roles.rs:60` | API privada. Unirse a servidores y enviar solicitudes de amistad desde clientes de terceros es especialmente sensible. |
| 12 | **Voz y vídeo para cuentas de usuario**, incluido hacer sonar llamadas en DMs y DAVE. | `crates/discord-voice/src/transport.rs:332`; `apps/desktop/src/voice.rs:1003-1008`; `docs/architecture.md:57` ("unofficial normal-user opcodes 13/4") | Media no oficial con la cuenta del usuario. |
| 13 | **Servidor local de Rich Presence** compatible con Discord: *pipes* y *sockets* `discord-ipc-{n}` y puertos 6463–6472. Además, detección de juegos enumerando procesos. | `crates/platform/src/game_activity.rs:68`, `game_activity.rs:305`; `apps/desktop/src/game_activity.rs:359-367`; `crates/platform/src/processes.rs:1-2` | Suplanta la interfaz IPC del cliente oficial. Es opcional. |
| 14 | **Nonce de mensaje con forma de snowflake**, "like the official client's". | `crates/client-core/src/fingerprint.rs:118-126` | Contribuye a la imitación. |
| 15 | **Ingeniería inversa del cliente oficial** como fuente: cliente móvil descompilado y *bundle* web. | `docs/discord-compatibility.md:10-11`; `assets/sounds/README.md:3-5` | Riesgo legal y de política. |
| 16 | **Terceros fuera de Discord:** subida anónima a Catbox/Litterbox con consentimiento por archivo. | `crates/discord-api/src/upload/external.rs:1`, `external.rs:12-13` | Privacidad (está documentado en `docs/threat-model.md:34-44`). |

**Lo que no se encontró [V]:**
- Llamadas a `/science` o `/track` (telemetría). La única aparición de `/science` está en un test (`login_linux.rs:446`).
- `/auth/login` propio ni MFA propio.
- Scraping del *build number*.
- `X-Debug-Options`.

**Contradicciones entre la documentación y el código [V].** Son importantes porque un lector de la documentación se llevaría una imagen falsa:
- `README.md:413`: "No fingerprint spoofing". Lo contradicen los puntos 4 a 7.
- `docs/discord-compatibility.md:944`: "No spoofed official client user agent/properties are supplied". Lo contradicen `fingerprint.rs` y `discord-api/src/lib.rs:151-187`.
- `docs/authentication.md:57`: "Do not … spoof a fingerprint". Lo contradice el mismo código.
- `docs/invite-verification.md:36`: "no … spoofing, fingerprint override". Lo contradicen `captcha.rs:56` y, **[I]**, `captcha.js:52`.
- **[V]** La huella se introdujo en los commits `5a6fb8ab` (2026-09-11, "present a consistent browser fingerprint") y `644de4de` (2026-09-13). La documentación no se actualizó.

---

## Lecciones aplicables a RUNE

Son ideas de diseño, no código para copiar. Hay que adaptarlas a `limn`.

### Adoptar

1. **Capas de crates con dependencias en un solo sentido:**
   - `model`, sin E/S
   - `protocol`, DTOs con decodificación acotada que no llegan al estado
   - `core`, estado y reductor
   - transportes: REST, tiempo real y voz
   - `store`
   - `platform`
   - `ui`, que no depende de ningún transporte ni toca secretos
   - `app`, el cableado

   La UI solo ve `State` y emite `Command`.
2. **Un único dueño del estado en el hilo de UI y un reductor puro.** `Envelope { generation, event }` descarta en bloque las respuestas de sesiones anteriores, y los *request ids* descartan las respuestas obsoletas dentro de una sesión. La UI devuelve `Vec<Command>`. Así se evita `Arc<Mutex<State>>` y el reductor se puede probar con *replays* sintéticos.
3. **Todo acotado por elementos y por bytes**, con una política explícita ante desbordamiento:
   - Colas fiables: si se desbordan, se **corta la conexión** y se resincroniza; nunca se pierden mutaciones en silencio.
   - Colas efímeras (*typing*, presencia): con pérdida, filtradas en el productor y con `watch` para "último valor".
4. **Repintado reactivo.** Cada productor despierta la UI después de encolar. El consumo por frame tiene presupuesto (por ejemplo, 8 eventos) y se re-arma si queda trabajo. Los plazos usan un "repintar después de X". RUNE debe confirmar que `limn` ofrece un mecanismo equivalente de *wake* desde otros hilos.
5. **Runtime async pequeño (2 workers) y hilos dedicados para todo lo bloqueante** (SQLite, keyring, decodificación), comunicados por `sync_channel` acotados. Lecturas cancelables con una ranura reemplazable por tipo: abortar al soltar.
6. **Lista de mensajes virtualizada:**
   - alturas estimadas más una caché de alturas medidas con clave por hash de contenido y presentación
   - invalidación total si cambian la fuente o la escala; solo las filas visibles si cambia el ancho
   - **ancla `(id, inset)`** para conservar la posición al cargar páginas antiguas
   - paginar solo con scroll explícito

   Mejora para RUNE: usar sumas prefijas o un árbol de Fenwick si la ventana supera unos cientos de filas.
7. **Una ventana de mensajes en RAM** (por ejemplo, 500 mensajes / 4 MiB), con *patches* para las ediciones que llegan durante la carga y *tombstones* para los borrados. El historial frío va a SQLite.
8. **SQLite configurado con cuidado:**
   - WAL, `synchronous=NORMAL`, `max_page_count` como techo duro, `secure_delete`, `auto_vacuum=INCREMENTAL`
   - todas las claves por cuenta
   - migraciones con `user_version`, rechazando esquemas más nuevos
   - expulsión LRU por canal
   - borrado completo de la cuenta al cerrar sesión
9. **Reconexión robusta:**
   - tabla de códigos de cierre (detener / Identify / Resume)
   - backoff exponencial con tope y *jitter* (en RUNE, con un RNG de verdad)
   - intentos limitados antes del primer READY e indefinidos después
   - `Notify` para "reconectar ahora"
   - revalidar la página visible tras Resume
10. **Credenciales:** solo en el keyring del SO y sin texto plano como alternativa. Guardarlas solo después de una sesión válida. Tipo secreto `Zeroizing`, sin `Clone` y con `Debug` redactado. Cabecera marcada como sensible. Un *worker* serial para que un guardado nunca adelante a un borrado.
11. **Medios:** decodificación fuera del hilo de UI con un semáforo global; presupuestos de texturas por pool; reutilizar texturas en las animaciones; conversión YUV→RGB en la GPU.
12. **Voz, si algún día se implementa:** *callbacks* de audio sin locks y sin DSP (solo `rtrb`); DSP en un hilo propio; Opus a 20 ms; *jitter buffer* acotado; ningún medio antes de completar el cifrado.
13. **Higiene del proyecto:** `unsafe_code = "forbid"` en todo el workspace (`Cargo.toml:48-49`); `cargo-deny` con una lista permitida de licencias y excepciones de versión exacta; `THIRD_PARTY_NOTICES` con procedencia; `opt-level=3` por paquete en `dev` para los códecs (`Cargo.toml:61-201`).

### Evitar

1. **Cualquier mecanismo de la sección anterior** si RUNE se conecta a Discord: token de usuario, inyección de JS para capturarlo, falsificación de UA, *super-properties* o *build number*, opcodes y endpoints exclusivos de usuario, voz con cuenta de usuario y suplantación del IPC oficial. La alternativa es que RUNE use **su propio backend o protocolo** o, con Discord, únicamente **OAuth2 o bots oficiales**, dentro de lo que permitan sus políticas.
2. **Que la documentación diga lo contrario que el código.** En Serein, la documentación afirma garantías ("no spoofing", "no compression", "16 MiB") que el código ya no cumple. En RUNE: menos documentación narrativa, más tests que la respalden, y revisar la documentación en cada PR que toque esas garantías.
3. **Archivos y tipos "dios".** `main.rs` tiene 7840 líneas, `timeline.rs` 10 827, `client-core/src/lib.rs` 6151, y `Command`/`Event` tienen unas 220 líneas de variantes cada uno. Mejor dividir el reductor por dominio (mensajes, navegación, voz) con sub-enums y sub-estados.
4. **Recorrido lineal de alturas** si la ventana crece, y **jitter basado en el reloj**.
5. **Incluir activos propietarios** (sonidos de Discord) o código MPL modificado sin necesidad.
6. **SQLite sin cifrar** con datos privados. Si RUNE guarda mensajes, al menos documentarlo como Serein. Mejor aún: valorar cifrar con una clave guardada en el keyring.
7. **Acumular funciones periféricas** (actualizador propio, Spotify, Catbox, Rich Presence, extensiones Wasm) antes de tener sólido el núcleo de mensajería.

## Revisión enfocada: cómo inicia sesión y qué cambia para RUNE (2026-10-14)

**[V]** Relectura estática del clon limpio en
`d1af883c9d7dda90d1261a2d15fc852a5445204d`. Existen los módulos
`client-core`, `model`, `discord-protocol`, `discord-api`, `discord-gateway`,
`discord-voice`, `session-cache`, `local-store`, `platform` y `ui`.
El workspace declara MIT OR Apache-2.0 (`Cargo.toml:10`), con archivos
de ambas licencias. No se ejecutó Serein, no se abrieron almacenes de
credenciales ni perfiles reales y no se copió código.

**La diferencia decisiva es la credencial, no el framework de UI.** Serein
no usa el flujo OAuth2 implementado en la primera integración: su documentación distingue
la página oficial de inicio de sesión del traspaso no oficial de la sesión
(`docs/authentication.md:54`). Esa página no convierte la integración
posterior en una autorización OAuth2. Tampoco constituye evidencia de que
el canje público de RUNE vaya a funcionar con una aplicación determinada.

| Paso | Serein: evidencia del clon [V] | RUNE: proveedor OAuth2 |
| --- | --- | --- |
| Credencial | `SessionSecret` admite sesiones de usuario y rechaza prefijos `Bearer` y `Bot` (`crates/client-core/src/auth.rs:84`); REST usa esa credencial de sesión (`crates/discord-api/src/lib.rs:359`). | Autorización explícita OAuth2 con `identify guilds`, PKCE y access token limitado; no se solicita el token de una cuenta personal. |
| Conexión | Verifica identidad REST, obtiene Gateway y lo inicia bajo una generación (`apps/desktop/src/connection.rs:169`). El guardado de la credencial espera un READY aceptado (`apps/desktop/src/main.rs:5934`). | Verifica aplicación, scopes, caducidad e identidad; obtiene datos básicos de servidores. Sin Gateway, conversaciones ni voz. Canje real todavía pendiente de verificación. |
| Persistencia | Un único worker serializa lecturas, guardados y borrados del keyring (`apps/desktop/src/credentials.rs:48`). | Credencial y datos de esta sesión sólo en RAM; el host desactiva SQLite para el adaptador experimental. |
| Salida | Detiene servicios y limpia datos locales (`apps/desktop/src/main.rs:2435`); su documentación no promete revocación remota (`docs/authentication.md:62`). | Limpia dominio y UI inmediatamente, intenta revocar OAuth2 y avisa si la revocación no pudo confirmarse. |

**Ideas reutilizables independientemente del mecanismo de acceso:**

- Vincular toda respuesta al intento que la originó. Serein descarta eventos
  de otra generación (`crates/client-core/src/lib.rs:2332`). RUNE ya aplica
  generaciones de proveedor y cancela la operación OAuth2 al abandonar el
  intento; conviene conservar esa propiedad al añadir renovación de sesión.
- Si RUNE añade persistencia autorizada, usar un worker de keyring ordenado
  y esperar una identidad validada antes de guardar. La clave debe incluir
  proveedor e identidad; el nombre del proveedor que hoy usa SQLite no
  basta para varias cuentas.
- Mantener separados sesión local, disponibilidad del transporte y
  revocación remota. El éxito de una ventana de login o de un socket no
  demuestra por sí solo que el servicio haya autorizado la integración.

La amplitud funcional de Serein depende de su acceso no oficial a sesiones
de usuario, ya analizado arriba. No ofrece una ruta OAuth2 autorizada para
convertir los permisos limitados de RUNE en acceso completo a Discord.

## Continuidad: navegación y mensajes de la sesión personal (2026-10-14)

Tras confirmar manualmente el login de RUNE, se estudiaron en el mismo
clon `discord-protocol/src/lib.rs`, `ready.rs`, `permissions.rs` y
`read_state.rs`, y la coordinación en `discord-gateway/src/lib.rs`.
La evidencia es código público del clon; no se ejecutó el cliente ni se
capturaron sesiones o tráfico de cuentas.

Patrones aplicados con implementación propia en `rune-discord-session`:

- Tratar identidad REST y estado inicial de Gateway como etapas distintas.
- Traducir la variante personal de READY, incluidas propiedades anidadas,
  usuarios compartidos, destinatarios por ID y membresías alineadas por guild.
- Conservar permisos como evidencia explícita: una identidad suplementaria
  sin roles no borra roles conocidos, y duplicados contradictorios no
  conceden acceso. No convertir ausencia de sobrescrituras en una lista vacía.
- Interpretar cursores de lectura cero como ausencia y descartar tipos de
  recurso distintos de canales; no inventar conversaciones al faltar datos.
- Aplicar ediciones de mensajes como parches y reconectar usando secuencia
  y datos de reanudación dentro del adaptador.

RUNE conserva un alcance menor: texto, historial y navegación; no reproduce
la implementación de voz, foros, hilos, keyring o fingerprint de Serein.
El transporte se identifica como RUNE y las pruebas usan HTTP/WebSocket
locales. Esta comparación no acredita interoperabilidad real del nuevo
Gateway. Detalles y límites en [DISCORD_SESSION.md](DISCORD_SESSION.md).

## Continuidad: composer, expresiones e imágenes (2026-10-14)

Se volvió a leer el checkout
`d1af883c9d7dda90d1261a2d15fc852a5445204d`, sin ejecutarlo ni usar una
cuenta. Las rutas de esta sección pertenecen a ese commit. **[V]** significa
evidencia estática de código público, con confianza alta sobre lo que ese
código intenta hacer; no demuestra su comportamiento en producción. Las
decisiones para RUNE son una implementación propia, no código trasladado.

### Hallazgos y aplicación

| Funcionalidad | Evidencia pública en Serein [V] | Decisión y límite en RUNE |
| --- | --- | --- |
| Composer y selección | `crates/ui/src/composer_text.rs:13` conserva rangos del texto fuente; `:187` mantiene el texto de protocolo para selección, copia y undo, mientras pinta etiquetas e imágenes. Su caché incluye fuentes, dimensiones y revisiones de recursos (`:48`, `:57`, `:87`). `crates/ui/src/emoji_picker.rs:31` inserta sobre la selección en índices Unicode. | Conservar el texto editable y los IDs del dominio por separado de su representación. El proveedor entrega una inserción opaca para un emoji; el componente no interpreta Discord. El picker de RUNE inserta en el cursor o reemplaza la selección usando índices Unicode de limn. Los objetos inline editables y el autocompletado general en medio del texto requieren otro cambio. |
| Emojis Unicode y custom | `crates/ui/src/emoji_picker.rs:243` busca por nombre de servidor y emoji; `:278` usa una revisión de catálogo. `crates/model/src/reactions.rs:73` limita por cantidad y bytes. | Mantener el catálogo Unicode existente y añadir el del servidor seleccionado, con disponibilidad y roles. Catálogo acotado a 500 entradas por servidor; búsqueda y autocompletado por nombre. No habilitar emojis de otros servidores ni deducir suscripción o permisos ausentes. |
| Stickers | `crates/ui/src/stickers.rs:27` separa recientes, servidores y packs; `:53` busca nombre, tags y origen. `:185` omite pintar filas fuera de la vista y `:198` consulta capacidades antes de ofrecer envío. | Pestaña del servidor, vista previa y selección preparada. Elegir una tarjeta no envía: hay una confirmación explícita y una revalidación de disponibilidad/permisos. El texto del borrador se conserva. Packs, recientes de stickers y animación no se implementan aquí. |
| Búsqueda GIF | `crates/client-core/src/gifs.rs:13` limita la caché a ocho páginas, 768 KiB y diez minutos; el estado conserva ID de petición y descarta respuestas antiguas. `crates/discord-api/src/lib.rs:1559` identifica su transporte como relay no oficial. | Adoptar cancelación, identidad de petición y presupuestos cuando exista un proveedor de búsqueda independiente. Esta entrega añade un enlace HTTPS al borrador y guarda hasta doce enlaces recientes en RAM; no busca, descarga ni reproduce el GIF y no envía automáticamente. |
| Imágenes y memoria | `apps/desktop/src/avatars.rs` separa descargas y decodificación de la UI; `:620` construye rutas CDN a partir de IDs y formatos, y `:639` usa PNG para stickers PNG/APNG. | `RemoteImage` describe el recurso y el host solicita sólo referencias visibles; `RasterImage` de limn dibuja el resultado. El cargador independiente limita cuerpo, dimensiones, concurrencia y caché, y normaliza las vistas previas. No reutilizar una sesión HTTP autenticada para imágenes. |
| Iconografía y menciones | `assets/icons/README.md` documenta un atlas de Phosphor y algunos iconos de Simple Icons. `crates/ui/src/composer_text.rs` proyecta menciones a etiquetas sin reemplazar su texto fuente. | Mantener iconos de limn y resolver referencias recibidas mediante tipos neutrales. Mostrar un alias no equivale a mencionar o notificar a una persona. No se añade en esta iteración selección saliente de menciones con notificaciones. |

Los componentes actuales usan `Button`, `TextField`, `SearchField`,
`ScrollArea`, `Popover`, `Tooltip` y `RasterImage` de limn. No se añadió
egui a `rune-ui` ni a `rune-components`. Las tres pestañas tienen controles
reales; los items no disponibles explican el motivo y no producen clics.
Enter sigue el orden del catálogo custom; en stickers sólo prepara la
selección. Escape devuelve el foco al composer y la pestaña GIF vuelve a
abrirse enfocando su campo de URL.

RUNE registra la selección del composer junto al canal y al texto fuente
exactos, reutilizando esa copia mientras el texto no cambia. Antes de
insertar compara ambos: una selección obsoleta cae al final del borrador.
Unicode, custom y enlaces GIF comparten la sustitución acotada a 2000
unidades UTF-16; un enlace separa su texto de los dos lados. El host aplica
el cursor después del foco, porque enfocar un input de limn mueve su cursor
al final. Se conservan los bytes del texto no seleccionado; no se proyectan
objetos inline dentro del editor.

La cuadrícula muestra hasta **72 coincidencias** e informa cuándo hay más;
la búsqueda recorre el catálogo recibido, no sólo esas tarjetas. El límite
evita construir cientos de imágenes por cuadro. No se presenta como una
cuadrícula virtualizada: el grid custom comparte el scroll con las
categorías Unicode. Un siguiente paso es aplanar secciones en filas y usar
`ScrollArea::uniform_list` de limn, manteniendo cabeceras, navegación por
categoría y foco; introducir un scroll virtual anidado cambiaría esa UX.

### Contraste con documentación oficial

La [referencia de emojis](https://docs.discord.com/developers/resources/emoji)
documenta identidad, nombre, roles, animación y disponibilidad. También
recomienda WebP: los formatos WebP/AVIF subidos no se convierten bien a
otros formatos. Por eso RUNE solicita `.webp?size=128` sin animación para
el catálogo custom. La existencia de estos objetos documentados no otorga
a una sesión personal autorización OAuth2 para usarlos; el proveedor de
sesión sigue identificado como no oficial.

La [referencia de imágenes](https://docs.discord.com/developers/reference#image-formatting)
documenta `size`, pero su nota específica dice que **stickers y avatares
predeterminados lo ignoran**. No se puede usar `?size=128` como límite de
decodificación: el cargador controla las dimensiones reales y normaliza
después. Esa misma referencia distingue los stickers PNG/APNG en `.png`,
GIF en otro origen y Lottie en `.json`. RUNE admitía sólo la vista previa
estática PNG/APNG; desde el 2026-10-18 dibuja y anima los Lottie con
rasterlottie (Serein-current lo usa para un cuadro quieto).

La [referencia de stickers](https://docs.discord.com/developers/resources/sticker)
separa el objeto completo de los items incluidos en mensajes y enumera
PNG, APNG, Lottie y GIF. La [creación de mensajes](https://docs.discord.com/developers/resources/message#create-message)
admite `sticker_ids` y permisos específicos; RUNE prepara un sticker por
acción y mantiene su envío separado del texto. Los formatos de
[menciones y emojis en mensajes](https://docs.discord.com/developers/reference#message-formatting)
se traducen dentro del adaptador a entidades de RUNE; no se interpretan
en el componente gráfico.

**GIF no es una API pública de búsqueda de Discord verificada aquí.**
Serein llama a `/gifs/search`, `/gifs/trending` y `/gifs/trending-gifs`
con `provider=klipy`, y los identifica expresamente como comportamiento
no oficial (`crates/discord-api/src/lib.rs:1559`). RUNE no implementa ese
relay. Una futura integración independiente debe usar un servicio cuya
API, acceso y condiciones se verifiquen por separado. Por ejemplo, la
[documentación de búsqueda de Tenor](https://developers.google.com/tenor/guides/endpoints)
describe una API con clave propia y `client_key`; su
[guía de integración](https://developers.google.com/tenor/guides/quickstart)
es evidencia de un contrato documentado, no de una clave obtenida ni de
un servicio probado en RUNE. La pestaña actual lo dice y sólo inserta
texto HTTPS, rechazando credenciales embebidas, caracteres de control y
destinos locales comunes. No hace DNS ni verifica que la URL contenga
realmente un GIF.

### Licencias y validación

El workspace de Serein conserva `MIT OR Apache-2.0`; sus recursos tienen
avisos independientes. `assets/icons/LICENSE` es MIT para Phosphor,
`LICENSE-SIMPLE-ICONS` es CC0, y `assets/twemoji/LICENSE-GRAPHICS` exige
atribución CC BY 4.0. No se incorporó código, atlas, marcas ni sonidos de
Serein. RUNE conserva los recursos y avisos de limn; una nueva colección
gráfica requeriría su propia revisión de procedencia y atribución.

Las pruebas de esta entrega usan catálogos, HTTP y acciones UI sintéticas:
selección custom como texto opaco, ausencia de envío implícito, confirmación
de sticker sin borrar el borrador, rechazo de selecciones obsoletas,
cambios de capacidades, límites del borrador y enlaces recientes,
reapertura y foco, teclado y compatibilidad de snapshots antiguos.
No se enviaron mensajes reales como parte de esta investigación. La
confirmación manual del dueño sobre navegación y lectura no valida todavía
el envío de cada tipo de expresión ni su vista previa en el servicio real.
