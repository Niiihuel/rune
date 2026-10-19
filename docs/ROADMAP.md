# Hoja de ruta de RUNE

El estado de hoy y el orden de lo que sigue están en
[Estado y siguientes pasos (2026-10-18)](#estado-y-siguientes-pasos-2026-10-18).
La revisión por familias y evidencia está en
[SEREIN_COMPARISON.md](SEREIN_COMPARISON.md#inventario-para-la-siguiente-entrega-paralela).
El plan vigente y los 21 recorridos de continuidad se siguen en
[tasks/plan.md](../tasks/plan.md#continuidad-desde-el-análisis-de-discord-2026-10-15)
y [tasks/todo.md](../tasks/todo.md#recorridos-pendientes-2026-10-15).
Las secciones siguientes conservan el historial de entregas.

## Pendientes corregidos y nueva continuidad (2026-10-15)

La entrega anterior ya completó cuatro incrementos que no deben volver a
listarse como ausentes:

- Entrada limn para pegar y revisar una invitación, con reconocimiento del
  adaptador y resultados ligados a cuenta/solicitud. Aceptación nativa con
  verificación humana desde el 2026-10-16, sin probar contra el servicio real.
- Búsqueda local con texto y filtros «Solo míos», «Con archivos» y «Fijados»
  combinados con AND, selección por ID y borrador conservado.
- Lectura local con `Capabilities::read_state`: sólo se emite `MarkRead`
  cuando el proveedor admite marcadores; esto no confirma sincronización remota.
- Privados sin indicador de presencia ni estado personalizado cuando el
  proveedor no declara esa capacidad; se conservan los contadores de grupos.

La evidencia de esa entrega es de **651 pruebas aprobadas, seis ignoradas**,
formato, Clippy y release correctos, con capturas simuladas. Es un resultado
anterior, no una nueva ejecución en esta revisión documental:
[registro](evidence/native-workflows-2026-10-14.json) y
[workflow](WORKFLOW.md#continuidad-desde-ambos-clones-de-serein-2026-10-14).

Se amplía la inspección estática de los artefactos distribuidos de Discord,
autorizada por el dueño en esta conversación. Sigue en curso: cada hallazgo
debe conservar su artefacto y cobertura; una frontera observada no acredita
el flujo remoto completo. El desarrollo continúa por recorridos con agentes
independientes, integración secuencial y criterios de cierre en la lista de
tareas. Se mantiene la pausa de aceptación de invitaciones y no se habilita
una capacidad remota por disponer de UI o de pruebas sintéticas.

Nada tiene fecha. El orden es el propuesto; cada paso deja el proyecto
compilando y con sus pruebas.

## Primera integración de adjuntos de un archivo (07, 2026-10-15)

El composer limn ya conecta el selector Linux con preparación, borrador,
envío sólo con archivo o con texto, progreso, cancelación y reintento tipado
en **simulación local**. Core conserva descriptores y bytes fuera de UI, con
32 MiB/diez entradas (ocho hasta el 2026-10-18) y un worker fijo; reintentar conserva el mismo cuerpo y
nonce, sin volver a abrir la ruta ni enviar sólo el texto. `rune-platform`
ya es una dependencia efectiva de Core. [Contrato](../tasks/uploads-contract.md)
y [alcance](FILES.md#adjuntos-de-salida-de-un-archivo).

El snapshot integrado, incluida la limpieza de metadatos al eliminar canales,
pasó **1032 pruebas** con todas las features, seis ignoradas y 25 suites,
formato y Clippy estricto. Release con todas las features pasó para 11 crates
en 66 segundos. El checkpoint **07.1** verificó en ventana nativa la
[preparación con borrador intacto](img/upload-prepared-2026-10-15.png) y el
[envío confirmado por Mock](img/upload-sent-2026-10-15.png), con un fixture propio
de 50 bytes. Los cinco replays de tests y el intento previo de QA abortado
antes de Enter quedan en el [registro](evidence/uploads-checkpoint-2026-10-15.json).
La extensión **07.2 de varios archivos ya está implementada y probada en
local**: hasta diez por mensaje en Mock (ocho hasta el 2026-10-18), lote completo o error y retry con el
mismo cuerpo. Pasó **1073 pruebas**, seis ignoradas y 25 suites, formato,
Clippy estricto y release correctos. REA repitió seis binarios de pruebas;
el selector nativo devolvió dos descriptores de 48 B y las capturas verifican
borrador preparado y mensaje confirmado con ambos adjuntos en Mock. Ver
[checkpoint 07.2](evidence/uploads-multiple-checkpoint-2026-10-15.json).
07.3.1 implementó el prototipo de ingreso por rutas de arrastre y la protección
del borrador ante drop+Enter. El pin de ese snapshot es `1ad33eb`; el checkpoint
07.2 permanece en `2618b37`. La suite integral de ese snapshot pasó **1098
pruebas**, seis ignoradas y 25 suites en 5,47 s con todas las features, formato
y Clippy estricto de todos los targets/features correctos; release de ocho
crates en 51,13 s. Su vía RawInput sigue sin importación nativa: winit 0.30.13
no produce archivos DnD en Wayland y X11 no expone su posición. Dos gestos con
fixtures propios no prepararon adjuntos; no hubo Enter/envío ni PNG aceptado.
El [registro local 07.3.1](evidence/uploads-drop-checkpoint-2026-10-15.json)
conserva las pruebas verdes y los intentos por separado. La
[API nueva winit #4571](https://github.com/rust-windowing/winit/pull/4571) exige
0.31 beta; [la migración egui #7731](https://github.com/emilk/egui/pull/7731)
sigue en borrador. La política actual **07.3.2** limpia todos los archivos
legacy de `RawInput` antes del bridge, porque no traen una posición fiable
ligada al lote; el fallback también los rechaza. El host ya no llama a
`Client::prepare_attachment_paths`. La API Core queda probada para una entrada
futura con ese contrato; Adjuntar mediante selector sigue disponible.
**07.3.2 cierra sólo ese rechazo y su pulido visual.** El pin actual
`22e0fd93c14b3f3c89375135f83c1e9c41307330` incorpora la
[PR #50](https://github.com/Niiihuel/limn/pull/50) por cherry-pick propio; la PR
sigue abierta, sin merge público. Un arrastre real desde Dolphin con dos
fixtures de 54/55 B a Mock RUNE X11 mostró el aviso completo encima del editor,
«Borrador conservado» visible y ningún adjunto, sin Enter. La
[captura negativa](img/uploads-drop-native-rejected-2026-10-15.png) eleva el
inventario canónico Mock a 18. No acredita arrastre funcional ni envío; el
no-envío automático se comprueba con regresiones del host.
El [checkpoint final](evidence/uploads-drop-rejected-checkpoint-2026-10-15.json)
pasó **1120 pruebas**, seis ignoradas y 25 suites en 5,50 s, formato y Clippy
estricto correctos; release de seis crates en 49,56 s. Los tres replays REA
completos suman 462 aprobadas sin ignoradas/filtros. La
[auditoría](evidence/uploads-drop-rejected-audit-2026-10-15.json) no tiene errores
ni pendientes. El nuevo receiver de `limn-dnd` está implementado en la
[PR #55](https://github.com/Niiihuel/limn/pull/55), Draft abierta, y verificó
metadatos, decisiones y cierre normal en Weston privado. [Registro limitado](evidence/limn-drop-receiver-weston-2026-10-15.json).
Persisten el owner SDK/clipboard compartido para Hyprland, el pin aprobado y
el consumidor RUNE; no obliga a migrar winit ni cierra DnD en esta app.
El owner de biblioteca #62 y el opt-in entrante #63 ya están publicados en el
[checkpoint actual](evidence/limn-text-clipboard-matrix-2026-10-15.json); queda su integración SDK/RUNE y verificación Hyprland.
[Política actual](FILES.md#rechazo-del-arrastre-legacy-0732).

SaveFile **08.1 está completo sólo como API de plataforma y ejemplo sin red**:
64 pruebas unitarias y un doctest; selección GTK del destino exacto verificada
sin crear un archivo ni modificar el existente. La cancelación terminó en
`Cancelled`, pero su observador tardío no acreditó una ventana del SO. Ver
[checkpoint 08.1](evidence/save-file-picker-checkpoint-2026-10-15.json).
El snapshot histórico de SaveFile, con pin `1ad33eb`, pasó **1112 pruebas**, seis
ignoradas y 25 suites en 5,56 s, formato y Clippy estricto correctos; release de
tres crates en 45,98 s. Faltan integración Core/UI y `SharedNativeDialogs`
(08.2), reemplazo autorizado (08.3), pegado y transporte remoto. Los recorridos
completos 07/08 siguen abiertos.

## Estado de la primera iteración (2026-10-14)

| Área | Estado |
| --- | --- |
| Workspace, Nix (`shell.nix`, `flake.nix`), limn integrado | Hecho |
| Proveedor simulado determinista (servidores, canales, DMs, grupo, voz, historia paginada, respuestas, actividad, caída de conexión) | Hecho |
| Estado central incremental, envío optimista, no leídos y menciones, recorte de memoria | Hecho |
| Timeline virtualizado de altura variable con anclaje (PR limn #25) | Hecho |
| Composer: Enter/Shift+Enter (PR limn #26), respuesta, edición con ↑, @menciones, emoji, escritura | Hecho |
| Reacciones, acciones al pasar el puntero, menú contextual, borrar con confirmación, copiar | Hecho |
| Miembros por rol con presencia y tarjeta de perfil | Hecho |
| Panel de usuario: estado, micrófono, ensordecer, voz simulada, ajustes | Hecho |
| Selector rápido (Ctrl+K), Alt+↑/↓, F12 métricas | Hecho |
| Reconexión con backoff + banner, cancelación de peticiones obsoletas | Hecho |
| SQLite: caché por cuenta y conversación, ajustes | Hecho |
| Audio local: dispositivos (PipeWire) y medidor | Hecho |
| Métricas reales (vista, layout, pintado, CPU, RSS, latencia, arranque) y `--bench` | Hecho |

## Estado de la segunda iteración (2026-10-14)

| Área | Estado |
| --- | --- |
| Comparación con Serein y adopción de 8 ideas ([SEREIN_COMPARISON.md](SEREIN_COMPARISON.md)) | Hecho |
| Sesiones numeradas: lo de una sesión vieja se descarta | Hecho |
| Presencia y "hablando" como últimos valores, fuera de la cola | Hecho |
| Entrega de mensajes (enviando, rechazado, quizá enviado) con Reintentar/Descartar e idempotencia por nonce | Hecho |
| Plazos por petición y fallos sintéticos (cola llena, sin respuesta, conexión caída) | Hecho |
| Timeline coherente: páginas que mandan en su rango, lápidas, edición más nueva | Hecho |
| Habilidades por conversación con motivo (`Abilities`, `Permit`) | Hecho |
| Llamada como máquina de estados (`Call`, `CallPhase`, `CallQuality`), panel de voz | Hecho (simulada) |
| Indicador de voz propio con el micrófono real (`VoiceActivity`) | Hecho |
| Presencia: invisible y actividades ("Jugando a…") | Hecho |
| Arranque desde caché (instantánea) y borradores guardados | Hecho |
| SQLite v2: migración, permisos 0700/0600, `secure_delete`, tope de tamaño, poda en reposo, "Borrar datos locales" | Hecho |
| Lista de miembros virtualizada | Hecho |
| Elección de GPU integrada: primer cuadro en ~80 ms y GPU NVIDIA dormida | Hecho |
| Binario de release ejecutable fuera de `nix-shell` (RUNPATH) | Hecho |
| Pruebas de la simulación desde Ajustes (cortar, rechazar envío, no responder) | Hecho |

## Estado de la tercera iteración (2026-10-14)

A pedido del dueño, la interfaz se llevó a la estructura, medidas y flujos del
Discord oficial, tomados de capturas de su propia cuenta en un servidor de
prueba (ver [SECURITY.md](SECURITY.md#referencia-visual-de-discord-tercera-iteración)).

| Área | Estado |
| --- | --- |
| Modelo de administración: 32 permisos, sobrescrituras por canal, @everyone, dueño, jerarquía de roles, vetos, auditoría, fijados, estado personalizado | Hecho |
| Simulación que valida permisos y jerarquía y registra auditoría | Hecho |
| Menú del servidor y sus diálogos (crear canal/categoría, notificaciones por servidor, apodo, abandonar), crear servidor | Hecho |
| Ajustes del servidor: perfil, roles (editor de permisos, reordenar, miembros), miembros (apodo, expulsar, vetar), vetos, auditoría, eliminar | Hecho |
| Administración avanzada (recorrido 19): aislar, crear invitaciones con enlace, subir emojis y stickers, miembros de a páginas, podar, webhooks, integraciones, GIF favoritos con alcance declarado; también en la sesión de Discord, sólo fixtures | Hecho |
| Edición del onboarding (Ajustes del servidor › Incorporación) y reglas y onboarding al entrar; sesión de Discord sólo con fixtures | Hecho (2026-10-18) |
| Eventos programados: lista, detalle, crear, editar, iniciar, terminar, borrar, «Me interesa» e invitaciones de evento; sesión de Discord sólo con fixtures | Hecho (2026-10-19) |
| AutoMod | Hecho el 2026-10-19 (rama `automod`): simulación, interfaz y sesión no oficial sólo con fixtures |
| Ajustes de canal: vista general (tema, modo lento, +18), permisos simples y avanzados de tres estados, sincronizar con la categoría, eliminar | Hecho |
| Lista de canales: estados, engranaje y «+», arrastrar para reordenar y mover, menús contextuales, silenciar con duración, notificaciones por canal, duplicar | Hecho |
| Mensajes fijados (dominio, simulación, menú y ventana emergente) | Hecho |
| Ajustes de usuario como panel flotante con búsqueda: cuenta, privacidad, notificaciones, voz (umbral de voz real), apariencia (tema Ónix propio), accesibilidad (compacto, tamaño de texto, espacio entre grupos, zoom), sistema (atajos), idioma y hora (12/24 h), conexión, desarrollador | Hecho |
| Popout de perfil propio, estados con duración, estado personalizado, editor de perfil, menús de dispositivos, tarjetas de perfil | Hecho |
| Medidas generales de la ventana, mensajes y voz según las capturas | En curso |

### Matriz de RUNE frente a Discord

La matriz de [COMPATIBILITY.md](COMPATIBILITY.md) dice qué **permite**
Discord mediante interfaces oficiales, y su
[mapeo](COMPATIBILITY.md#mapeo-a-las-abstracciones-de-rune) cómo encaja en los
contratos de RUNE. El adaptador OAuth2 experimental implementa identidad y
servidores básicos, con canje real pendiente de validación. La nueva ruta de
sesión personal no oficial tiene login e identidad confirmados por el dueño.
El dueño confirmó también navegación e historial. Gateway y las acciones de
envío/edición/borrado tienen pruebas locales; las escrituras todavía requieren
su propia validación real. No se presentan como OAuth2. Voz, archivos,
presencia y administración de Discord
continúan sin habilitarse.

## Revisión de coherencia de caché (2026-10-14)

La siguiente mejora elegida sobre la versión funcional fue corregir tres
fallos reproducibles de estado y persistencia:

- Un borrado retira las creaciones, ediciones o páginas anteriores del lote
  de guardado del mismo `poll`; el mensaje no reaparece al reabrir SQLite.
- La poda usa cuenta, canal e id, conservando ids repetidos de otras
  conversaciones y los datos de otras cuentas.
- La caché tardía no rellena una conversación cuyo historial más reciente
  ya llegó vacío; la caché de arranque sigue mostrándose mientras se espera.

Las regresiones fallaron antes del arreglo y pasaron después. Se mantiene
el esquema SQLite v2 y las dependencias existentes. La comparación con
Serein y la investigación oficial de Discord se actualizaron para reflejar
los contratos ya implementados y las limitaciones que siguen abiertas.

## Búsqueda local y conexión de REA (2026-10-14)

- Ctrl+F y el encabezado buscan texto en los mensajes cargados del canal;
  Ctrl+K mantiene el cambio de conversación. Resultados virtualizados con
  salto por ID, teclado, actualización ante cambios y borrador intacto.
- REA 6.4.0 registrado en Codex y comprobado mediante MCP local. Inventario
  del ejecutable propio, benchmark y captura Wayland con evidencias
  exportadas; alcance y limitaciones en [REA.md](REA.md).
- Corregido el contador de cuadros: intervalos sobre duración observada,
  sin techo artificial de 60 cuadros/s, con pruebas de pausas y arranque.
- Investigación ampliada con documentación y material visual público de
  búsqueda de Discord, además de límites de CPAL, Opus, portales y WebRTC.
  No hay proveedor de bots ni conexión a una cuenta Discord.
- Validación: 305 pruebas del workspace, formato, check y Clippy sin
  advertencias; release compilado y búsqueda ejercitada con teclado en
  Wayland. Detalle en [PERFORMANCE.md](PERFORMANCE.md).

## OAuth2 experimental sin bots (2026-10-14)

- Nuevo `rune-discord`: código + PKCE S256, callback loopback, identidad y
  servidores de solo lectura; credenciales efímeras, sin persistencia.
- UI limn de autorización, cancelación, cuenta y cierre; limpieza de estado
  y avisos visibles ante vencimiento o fallo de revocación remota.
- Validación de esta etapa OAuth2: 335 pruebas aprobadas, fmt/check/Clippy y release compilado; auditoría de
  dependencias sin avisos conocidos. Configuración y límites en
  [DISCORD_OAUTH.md](DISCORD_OAUTH.md). Canje real aún por verificar.

## Sesión personal no oficial (2026-10-14)

El dueño pidió experimentar con un mecanismo de sesión como el estudiado
en Serein y autorizó una WebView temporal para el login. Esa decisión cambia
el alcance del proyecto; no constituye aprobación de Discord. Se conserva
OAuth2 como proveedor separado y no se copia código de Serein.

- `rune-discord-session`: identidad HTTP mediante `SessionLogin` y
  `SessionSecret`, sin credenciales en comandos, eventos o UI. La primera
  etapa de identidad ya fue confirmada manualmente por el dueño; la siguiente
  conecta Gateway, navegación, historial y acciones de texto con pruebas locales.
  Sin voz, keyring ni SQLite.
- `rune-login`: auxiliar Linux opcional con GTK/WebKit, sesión web nueva y
  entrega privada del resultado; la interfaz principal continúa en limn.
  Se activa con `session-login` y `--provider discord-session`.
- Contrato y UI genéricos: `ProviderKind::Unofficial`, `AwaitingExternal`,
  cancelación, cierre local y `account_servers` para distinguir una lista
  no consultada de una lista vacía; no se inventan scopes OAuth2.
- La compilación y las pruebas del auxiliar se registran por separado de
  la etapa OAuth2. Pasaron los fixtures nativos de handoff y cancelación con
  datos sintéticos; el dueño confirmó después el login y la identidad contra Discord.
  Detalle en [DISCORD_SESSION.md](DISCORD_SESSION.md).
- La UI entra al espacio de trabajo después del acceso. La explicación del
  proveedor está en Ajustes → Conexión y el cierre en Mi cuenta; el acceso
  y el chat comparten el tema elegido para RUNE.

## Estado y siguientes pasos (2026-10-18)

Reemplaza las listas «Voz: prioridad de la siguiente entrega» y «Otros
pendientes», que seguían en el estado del 14 y 15 de octubre. El detalle por
recorrido está en [tasks/todo.md](../tasks/todo.md) y la paridad con Serein
en [tasks/serein-parity.md](../tasks/serein-parity.md).

**Qué hay.** Todo lo de abajo existe en la simulación, etiquetada como tal.
En la sesión personal no oficial está probado con fixtures en loopback; el
dueño lo usó contra Discord el 2026-10-17 (acceso, servidores, mensajes,
miembros, nameplates y perfiles funcionaron; la voz entró a veces) y cada
recorrido lista lo que falta probar.

- **Voz y video**: Opus, jitter, mezcla, transporte de voz de Discord con
  DAVE, reducción de ruido y referencia de eco, llamadas en servidores,
  privados y grupos, pantalla y cámara (H264 por la GPU con GStreamer u
  OpenH264), el sonido de la pantalla, sonidos propios de la llamada, panel
  rápido y prueba de sonido como Serein ([MEDIA.md](MEDIA.md),
  [VIDEO.md](VIDEO.md)).
- **Mensajes y archivos**: subir, «Guardar como…», imágenes animadas, audio
  y video adjuntos, PDF multipágina con búsqueda, spoilers, tarjetas,
  reenvíos, encuestas, componentes y comandos de aplicaciones, emoji de
  servidor en la línea ([FILES.md](FILES.md), [ARTWORK.md](ARTWORK.md),
  [DOCUMENTS.md](DOCUMENTS.md)).
- **Cuenta y social**: sesiones en el llavero del sistema y varias cuentas,
  caché local cifrada por cuenta con la clave en el llavero (2026-10-18,
  [SECURITY.md](SECURITY.md#datos-locales)),
  selector de proveedor, perfiles con edición y medios, amigos, grupos,
  hilos, foros y escenarios, lecturas y búsqueda remotas, invitaciones con el
  hCaptcha que resuelve la persona (también en cualquier escritura).
- **Administración** (recorrido 19 y P7) y **escritorio** (bandeja, instancia
  única, inicio con la sesión, tema y fuente del sistema, atajos globales,
  `discord-ipc` y detección de juegos apagados por defecto); interfaz en
  español e inglés ([PLATFORM.md](PLATFORM.md)).
- **Repositorio** (2026-10-18): CI en cada push a `main` y cada pull request
  con `cargo fmt`, `clippy -D warnings` con todas las features, tests y
  `cargo deny` (`.github/workflows/ci.yml`,
  [SECURITY.md](SECURITY.md#cargo-deny-en-la-ci-2026-10-18)), y el
  diagnóstico de stderr también en un archivo rotado
  ([PLATFORM.md](PLATFORM.md#registro-a-archivo-2026-10-18)).

**Orden del dueño (2026-10-18).** Primero se termina lo pendiente (la
construcción por paquetes en curso, con el inventario de `tasks/todo.md`);
después, en este orden:

1. **Optimización**: arranque desde caché con cuentas reales, costo del
   layout y del scroll, listas grandes, pedidos repetidos y las mediciones
   que faltan en [PERFORMANCE.md](PERFORMANCE.md).
2. **Windows y macOS**, juntos. Windows empezó el 2026-10-19 (rama
   `windows-base`): compila, enlaza, tiene sus servicios del sistema y un
   job de CI; faltan la WebView2 del login y el hCaptcha, pantalla y
   cámara, juegos, el instalador y la prueba en un Windows real
   ([PLATFORM.md](PLATFORM.md#windows-2026-10-19-rama-windows-base)).
   macOS sigue sin empezar.
3. **Actualizador por etiquetas** del repositorio: HTTPS, verificación del
   hash o la firma y confirmación de la persona antes de instalar
   ([serein-parity.md](../tasks/serein-parity.md#pedido-para-después)).
   Hecho el 2026-10-19, adelantado por pedido del dueño (rama
   `actualizaciones`, [UPDATES.md](UPDATES.md)): manifiesto firmado por
   release, obligatorias por la palabra «obligatoria» del tag (se instalan
   antes de abrir) y opcionales ofrecidas sin bloquear; Linux probado con
   un feed local, Windows con su contrato listo para el port.

**Pendiente que no depende de ese orden.**

- La prueba real de cada recorrido por el dueño (listas en
  [DISCORD_SESSION.md](DISCORD_SESSION.md)); la validación del canje OAuth2
  público con una aplicación registrada.
- Lector de pantalla real (AccessKit con Orca/NVDA); arrastrar y soltar
  archivos, bloqueado por winit 0.30; DMA-BUF en la captura;
  `aead_aes256_gcm_rtpsize`; la prueba real de la calidad de la llamada
  (RTT y pérdida, hechos el 2026-10-18 en `voz-protocolo`, sólo fixtures);
  stickers Lottie; `THIRD_PARTY_NOTICES.md` para distribuir binarios.
- Bloqueado aguas arriba: `instant` sin mantenimiento por la feature `js`
  de openmls que activa `davey` (RUSTSEC-2024-0384) y zbus 5 mientras
  accesskit lo pida (RUSTSEC-2026-0370/0371/0372); ambos ignorados con
  motivo en `deny.toml`.

## Para limn

Ver [LIMN_INTEGRATION.md](LIMN_INTEGRATION.md#inventario-histórico-de-carencias-y-seguimiento):
Alt en `UiEvent::Key`, menciones en `Markdown`, emoji en color, color de
`Avatar`, y las ideas de la comparación con Serein (imágenes en línea e
imágenes por referencia, política de caché de imágenes, estimación por fila
en `measured_list`).
