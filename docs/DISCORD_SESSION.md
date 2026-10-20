# Sesión personal experimental de Discord

`rune-discord-session` es el proveedor **no oficial** de RUNE: entra con la
sesión web de la propia cuenta, como Serein, y habla con la API REST y el
Gateway que usa el cliente web. Es un experimento personal: autorizarlo no
implica aprobación de Discord, que publica que automatizar cuentas normales
fuera de OAuth2/bots está prohibido y puede terminar en la suspensión de la
cuenta ([política](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)).
La ruta oficial es otra: [DISCORD_OAUTH.md](DISCORD_OAUTH.md).

**Estado.** El dueño confirmó con su cuenta (2026-10-14) el login, la
identidad y la lectura de servidores, canales e historial. **Todo lo demás
de este documento está probado sólo con fixtures en loopback (HTTP,
WebSocket y UDP en 127.0.0.1) y la simulación; nada se probó contra el
servicio real.** Lo que el dueño tiene que probar está en
[tasks/todo.md](../tasks/todo.md).

## Decisiones del dueño

Las reglas completas están en [AGENTS.md](../AGENTS.md) y
[SECURITY.md](SECURITY.md); este proveedor existe por ellas:

- **2026-10-14:** sesión personal no oficial con login temporal propio en
  una WebView aislada (nunca para la interfaz ni las conversaciones), como
  Serein. La credencial queda dentro de RUNE; no se importa de otros
  navegadores o clientes.
- **2026-10-16:** guardar la credencial en el llavero del sistema y recordar
  varias cuentas; usar las rutas no documentadas que usa Serein («si lo usa
  Serein, vamos con ello»); REA estática del cliente oficial (paquete
  instalado y bundles públicos) para conocer rutas y comportamiento;
  aceptación nativa de invitaciones con el hCaptcha resuelto por la persona.
- **2026-10-17:** paridad con Serein; usar componentes de bots y comandos
  de aplicación; hCaptcha humano en cualquier escritura; `discord-ipc` y
  detección de juegos apagados por defecto.
- **2026-10-18:** caché local cifrada por cuenta también para la cuenta
  real; eventos programados y AutoMod aunque Serein no los tenga.
- **2026-10-19:** presentarse como el cliente web de Discord en un navegador
  (user agent, `X-Super-Properties`, propiedades del Identify), con una sola
  identidad coherente.

Siempre prohibido: automatizar, evadir o reenviar CAPTCHA/MFA; leer sesiones
o datos locales del cliente oficial; acceder a cuentas ajenas; credenciales
en el chat del agente, logs, capturas, SQLite o configuración.

**Fuentes.** Rutas, cuerpos y casos borde salen de Serein
(`references/serein` y `references/serein-current`, fuera de git) y de la
REA estática de los bundles públicos del cliente web, bajados sin sesión
([REA.md](REA.md)), más la documentación oficial donde existe. Todo está
reimplementado; no se copia código ni recursos. El módulo de onboarding se
reescribió desde la REA porque una revisión lo encontró demasiado parecido
al de Serein.

## Compilar y ejecutar

El motor web está detrás de la feature `session-login` y sólo lo usa el
proceso de login (Linux con WebKitGTK; Windows con WebView2). La
compilación normal conserva la simulación y OAuth2.

```sh
nix-shell --arg withLogin true   # o: nix develop .#session
cargo run -p rune-desktop --features session-login -- --provider discord-session
```

No hace falta Application ID ni secreto: es un mecanismo distinto de OAuth2
y ninguno cae en el otro. En Linux hay que ejecutar dentro de ese entorno
aunque el binario ya esté compilado: WebKitGTK necesita GStreamer con los
plugins base, good y bad (`appsink`, `autoaudiosink`, `fakevideosink`,
`webvttenc`) y los descubre por `GST_PLUGIN_SYSTEM_PATH_1_0`, que pone el
shell; sin ellos el proceso web aborta al crear el sink de audio. Un
empaquetado propio tiene que conservar esa variable en su lanzador.

## Login y entrada

- **Elegir proveedor.** `--provider mock|discord|discord-session`; sin
  argumento, la última elección (`$XDG_CONFIG_HOME/rune/provider`, sólo la
  clave), y si no hay, la sesión personal cuando la compilación la trae
  (las releases nunca abren solas en la simulación). Desde la ventana:
  Ajustes → Conexión → «Elegí el proveedor» o «Usar otro proveedor» en el
  acceso; cada opción dice su origen (simulado, oficial, no oficial), por
  qué no está disponible y, la no oficial, la advertencia. Cambiar pide
  confirmación y crea un `Client` y un `Store` nuevos en caliente, limpiando
  todo como al cambiar de cuenta (se conservan preferencias, tema y cuentas
  recordadas). El origen del proveedor se ve al pie de la columna de canales.
- **Primera entrada.** Sin cuentas recordadas, el host abre la página de
  Discord **antes** de crear la ventana de RUNE (Wayland no deja ocultar una
  ventana creada): es la única ventana hasta que hay sesión y cerrarla
  cierra RUNE; el resultado llega con `PrefetchedLogin`. Con la ventana ya
  abierta, la UI emite `BeginSignIn` una sola vez, para que aparezca la
  página de Discord con su QR; tras cancelar o cerrar sesión, «Iniciar
  sesión» lo emite con un clic y nunca reabre la ventana sola.
- **Flujo.** El proveedor pasa a `AwaitingExternal` y abre el auxiliar
  `rune-login`; su WebView efímera termina antes de que
  [`HostedLogin`](../crates/rune-login/src/lib.rs) entregue el resultado. El
  [adaptador](../crates/rune-discord-session/src/provider.rs) verifica la
  identidad (`GET /users/@me`) y emite `Ready` y `SignedIn`; la UI entra al
  espacio de trabajo limn aunque la cuenta no tenga conversaciones.
  Verificar la identidad y recibir la navegación son estados distintos:
  mientras abre el Gateway se ve el estado de conexión.
- **Pantalla de inicio.** Hasta el `READY`: «Conectando…», «Iniciando
  sesión…» (cancelable) y «Cargando servidores…»; a los 10 s, el motivo con
  «Reintentar», «Cambiar de cuenta» (primero las cuentas recordadas) y
  «Cerrar sesión» (pregunta, y olvida la cuenta en el llavero aunque todavía
  se esté verificando). Con una sesión cargada, una pérdida de conexión se
  avisa sobre la conversación con «Reintentar ya» (`Command::ReconnectNow`,
  `Capabilities::reconnect_now`), que corta la espera del Gateway entre
  intentos y reanuda la misma sesión.
- **Foco al volver.** Al cerrarse el auxiliar, el host pide su ventana con
  `ViewportCommand::Focus` y `RequestUserAttention`. En X11 la activa; en
  Wayland cada compositor decide si la enfoca o la marca urgente (winit 0.30
  y eframe no permiten activar una ventana existente con un token ajeno).

### Aislamiento del auxiliar

`apps/desktop` compone `rune-discord-session` con `rune-login`. El adaptador
sólo conoce `SessionLogin` y recibe un `SessionSecret` sin `Clone`, sin
serialización y con `Debug` redactado; eventos y comandos llevan estados,
nunca la credencial.

`rune-login` ejecuta el mismo binario como proceso auxiliar antes de iniciar
eframe; GTK/WebKit (o WebView2) poseen ese bucle aislado. El resultado
vuelve por pipes anónimos acotados y asociados a un intento: nunca por
argumentos, variables de entorno, portapapeles ni archivos. Cancelar invalida
el resultado y termina el auxiliar y sus procesos.

El motor usa una sesión efímera; no importa perfiles, cookies ni
almacenes. El puente observa sólo el tráfico de autenticación de la ventana
que la persona abrió en RUNE; no lee campos de contraseña, correo o MFA ni
mensajes. Documento principal, origen, intento y tamaño se comprueban en la
parte nativa, no en el callback JavaScript. Login, MFA y CAPTCHA los completa
la persona; un desafío no compatible termina el intento. TLS normal, sin
aceptar certificados inválidos. El auxiliar distingue caída del motor,
límite de memoria, cierre pedido y causa desconocida con mensajes fijos.

«Sólo en memoria» describe la retención de RUNE: no garantiza borrar todas
las copias dentro del motor, la swap o volcados del sistema.

## Identidad de navegador

Decisión del 2026-10-19 ([SECURITY.md](SECURITY.md)): el adaptador se
presenta como el cliente web en un navegador, con **una sola identidad** en
REST, Gateway, voz y las ventanas de login y hCaptcha. La identidad es la
del motor de esas ventanas, el que de verdad resuelve el desafío
(`rune-discord-session::browser`, a partir de un `Engine`: user agent,
`navigator.vendor`, `userAgentData`, idioma y zona), con las reglas del
cliente web (REA del bundle `web.4d4b2f75fdea5470.js`, build 641004; Serein
como segunda referencia).

| Pieza | Linux (WebKitGTK) | Windows (WebView2) |
| --- | --- | --- |
| Motor | Conocido de antemano: el de fábrica de WebKitGTK 2.52, fijado en las ventanas | Al arrancar el proveedor, un auxiliar oculto (`rune --session-engine-helper`) abre WebView2 con un perfil temporal, carga una página vacía de RUNE (`https://captcha.rune.invalid/`, servida desde memoria) y devuelve esos valores validados por la tubería privada |
| User agent | `Mozilla/5.0 (X11; Linux {arq}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/60.5 Safari/605.1.15` | El de WebView2, tal cual |
| `os` / `os_version` | `Linux` / vacío | `Windows` / `10` |
| `browser` / versión | `Safari` (vendor de Apple: el cliente web lo clasifica así) / `60.5` | `Edge` si las marcas traen `Microsoft Edge`, si no `Chrome` / el número de `Edg/` |
| Idioma | `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG` (`es_AR.UTF-8` → `es-AR`), o `en-US` | `navigator.languages[0]` |
| Zona (`X-Discord-Timezone`) | `TZ` o `/etc/localtime`, o `UTC` | La de `Intl` |
| Client hints (`Sec-CH-UA…`) | No | En cada pedido a la API |

- **`X-Super-Properties`:** base64 del JSON del cliente web en su orden
  (`os`, `browser`, `device` vacío, `system_locale`, `has_client_mods:
  false`, `browser_user_agent`, `browser_version`, `os_version`,
  `referrer…` vacíos, `release_channel: stable`, `client_build_number`,
  `client_event_source` y `launch_signature` en `null`, `client_launch_id` y
  `client_heartbeat_session_id` UUID v4 por proceso,
  `client_app_state: focused`). El Identify lleva las mismas más
  `is_fast_connect: false` y `gateway_connect_reasons: "COLD_START"`, sin
  `capabilities` (así las cargas siguen con la forma que lee el adaptador).
- **Número de build:** al arrancar, antes de todo, `GET https://discord.com/app`
  sin credencial y se lee `window.GLOBAL_ENV.BUILD_NUMBER`; si falla (5 s,
  1 MiB), 641004.
- **Cabeceras:** `X-Discord-Locale` (el idioma llevado a uno de Discord como
  el cliente web: `es-AR` → `es-419`), `Accept-Language`,
  `Referer: https://discord.com/channels/@me` y `Origin` en escrituras;
  aceptar una invitación suma `X-Context-Properties` con lo que mostró la
  vista previa. El apretón del Gateway y de la voz lleva el user agent y
  `Origin`. Las subidas al almacenamiento, sólo el user agent. El user agent
  va en cada pedido, no en el cliente HTTP (en Windows el cliente existe
  antes de conocer el motor).
- **No se manda:** `X-Fingerprint` (el cliente web lo descarta al iniciar
  sesión), `X-Installation-ID` ni `X-Debug-Options` (un navegador nuevo no
  los tiene); `launch_signature` va en `null` en vez de inventar la firma
  que calcula el WebAssembly del cliente web.
- **Sin motor no sale nada:** si el auxiliar de Windows no puede leerlo (por
  ejemplo, falta WebView2 Runtime), el proveedor no hace ningún pedido y el
  login falla con el enlace oficial para instalarlo.
- **Diferencias que quedan** (se cambian si Discord las distingue): API
  `/api/v10` y Gateway `v=10` en JSON sin `zlib-stream` (el web usa v9
  comprimido); el idioma es el del sistema, no el elegido en la cuenta;
  `rune-core` baja imágenes del CDN con su propio user agent porque no
  conoce al proveedor. En Windows, el apretón del WebSocket no lleva client
  hints y leer el motor cuesta un arranque oculto de WebView2 (~1 s).

Las pruebas (`browser::tests`, `browser::assert_presented`) comprueban sobre
lo que sale que REST, Gateway, Identify y ventanas presentan lo mismo y que
la credencial sólo va en `Authorization`.

## Sesión recordada en el llavero

[`rune_platform::secrets`](../crates/rune-platform/src/secrets.rs) define
`SecretStore` (listar, cargar, guardar, borrar; una entrada por servicio y
cuenta). En Linux lo implementa Secret Service (crate `secret-service`,
sesión cifrada DH); fuera de Linux no hay llavero. Atributos:
`application=rune`, `service=discord-session`, `account=<id>`,
`name=<nombre visible>` y `avatar=<hash>` (público: nunca la credencial ni
una URL). Listar no lee secretos. Cada guardado crea la entrada de nuevo y
después borra la vieja, así la lista sale ordenada por último uso y un fallo
conserva la anterior.

- **Al arrancar** se listan las cuentas (`RememberedAccounts`) y se verifica
  sólo la más reciente, emitiendo `Exchanging` (nunca `SignedOut`, que
  abriría el login). Éxito: `SignedIn` y se guarda de nuevo. Un 401 o una
  credencial inválida borra esa entrada y emite `SignedOut` con la lista que
  queda (sin ninguna, abre el login una vez). Un fallo transitorio conserva
  la entrada y ofrece «Volver a intentar»; sin reintentos automáticos.
- **Login nuevo:** la credencial se guarda en segundo plano; si el llavero
  falla, la sesión sigue en RAM con un aviso no bloqueante.
- **Cambiar de cuenta** (`UseAccount(Some(id))`) cierra la sesión como un
  cierre normal, sin borrar su entrada, y verifica sólo la elegida;
  `UseAccount(None)` abre el login para agregar otra. Requiere
  `Capabilities::saved_accounts`.
- **Cerrar sesión** espera la escritura pendiente y borra la entrada de esa
  cuenta (límite de 15 s); un 401 o el cierre 4004 del Gateway también la
  borran. **Olvidar** (`ForgetAccount`, `ForgetSession`) borra la entrada
  sin tocar las demás; `ForgetSession` mantiene abierta la sesión. Cerrar
  sesión no afirma revocación remota.
- **Interfaz:** el acceso muestra «Continuar como <nombre>» con su avatar
  (o el por defecto de Discord), «Olvidar» con confirmación y «Usar otra
  cuenta»; Ajustes → Mi cuenta → Sesión lista las demás y Ajustes →
  Conexión dice si la sesión está recordada.

Límites: el orden por fecha de creación tiene resolución de segundos; una
escritura abandonada por el límite (por ejemplo, esperando un diálogo de
desbloqueo) podría completarse después en el servicio. Las pruebas usan un
llavero falso en memoria; en `cfg(test)` el constructor público usa un
llavero ausente.

## Caché local cifrada

La cuenta real conserva mensajes, borradores, marcas de lectura y la
instantánea entre arranques, cifrados con una clave por cuenta en el
llavero ([SECURITY.md](SECURITY.md#datos-locales)); sin llavero, sólo
memoria.

- La cuenta se conoce al verificarla: el proveedor manda
  `Event::StorageIdentity` (`rune.discord-session.v1` y el id) y el núcleo
  pide la clave fuera del hilo de la interfaz.
- Al restaurar una sesión recordada, lo guardado (`Update::Restored`) se
  muestra como de caché («Conectando…», lo que pide conexión deshabilitado)
  hasta el `READY`, que lo reemplaza. Un login nuevo manda primero su
  instantánea vacía.
- Cerrar sesión y «Olvidar» mandan `Event::ForgetStorage`: se borran sus
  filas y su clave. Cambiar de cuenta conserva todo.
- Los resultados de la búsqueda remota se guardan aparte (tabla `found`) y
  la búsqueda local los lee. Los mensajes efímeros nunca se guardan.

## Errores, ritmo y fin de sesión

Las escrituras y lecturas REST van por una cola de 16 entradas con **una
solicitud a la vez** y espera global tras `429` o
`x-ratelimit-remaining: 0`. Cada escritura es **un solo intento**: un `5xx`,
un corte o una respuesta ilegible después de enviar quedan como resultado
ambiguo y no se repiten. Las lecturas nunca son ambiguas. Un `429` de
`POST /channels/{c}/typing` retiene sólo el indicador de escritura (tiene
su propio bucket), salvo que el límite sea global.

`http/rejections.rs` clasifica los rechazos (lectura acotada a 64 KiB; el
mensaje remoto y el contenido del desafío no salen del adaptador, sólo
etiquetas locales y si termina la sesión), con la
[tabla oficial de códigos](https://docs.discord.com/developers/topics/opcodes-and-status-codes#json):

| Respuesta | Tratamiento |
| --- | --- |
| 401 autenticado | Cerrar la sesión sin esperar el cuerpo. |
| 400/403, código 50014 | Credencial inválida: cerrar la sesión. |
| 400/403, código 40002 o 50178 | La cuenta requiere verificación: detener la sesión, sin resolver ni repetir. |
| 400/403, código 60003 | MFA para esa operación: rechazarla y conservar la sesión. |
| 400 con `captcha_key` y `captcha_service: "hcaptcha"` en una escritura | [Verificación humana](#verificación-hcaptcha-en-cualquier-escritura). Otro servicio de desafío remite a la aplicación oficial. |
| Consulta anónima con esos códigos | No invalida la sesión de la cuenta. |
| 429 | Espera compartida; ninguna repetición automática de escritura. |

Los códigos de cuenta tienen prioridad sobre los campos de CAPTCHA (Serein
agrupa 60003 y 50014; RUNE los separa porque MFA de una operación no es una
credencial inválida). No se diagnostica suspensión a partir de estas
categorías. Se exige raíz de objeto en el JSON (Serde aceptaría un array
posicional como rechazo).

Al terminar la sesión el adaptador descarta Gateway, cola y credencial en
RAM y publica `AuthenticationState::Failed`, que limpia dominio y UI. Los
cierres del Gateway conservan sólo su código numérico; 4004 descarta la
sesión. Cerrar sesión cancela Gateway, HTTP y tareas antes de soltarla.

## Verificación hCaptcha en cualquier escritura

Decisión del dueño (2026-10-16 para invitaciones, 2026-10-17 para cualquier
escritura).

- **Cuándo:** una escritura REST (`request_answer`: mensajes, stickers,
  ediciones, reacciones, encuestas, fijados, amigos, grupos, hilos,
  escenarios, perfil, cuenta, administración, interacciones, eventos,
  AutoMod) o una subida recibe el desafío hCaptcha. Una lectura nunca se
  ofrece.
- **Qué pasa:** el proveedor guarda la escritura (`challenges::Challenges`,
  una a la vez) y emite `Event::ChallengeRequired`. La interfaz pregunta
  «Verificación requerida» con «Verificar» y «Cancelar»; nada se abre solo.
  Core corre un plazo de cinco minutos (`CHALLENGE_LIFETIME`).
- **La ventana:** «Verificar» (`Command::SolveChallenge`) llama a
  `SessionLogin::verify`, una WebView aislada de `rune-login` en una página
  propia de RUNE, con `sitekey`, `rqdata` e invisible, y la identidad de
  navegador; `rqtoken` y `captcha_session_id` quedan en el adaptador. Una
  sola ventana a la vez, contando la de una invitación.
- **Reintento único:** la misma orden vuelve al frente de la cola, se
  revisa como la primera vez y sale una vez con `X-Captcha-Key` y, si
  vinieron, `X-Captcha-Rqtoken` y `X-Captcha-Session-Id`. Una escritura de
  varios pasos (editar tras leer, `settings-proto`, subir archivos) rehace
  sus pasos previos y la respuesta va con el pedido frenado.
- **Termina sin enviar:** cancelar, cerrar la ventana, vencer el plazo,
  perder la conexión o cambiar de cuenta (`Event::Failed` con el motivo).
  Un segundo desafío en el reintento no se ofrece.
- **Nunca** se resuelve automáticamente ni se reenvía a servicios externos.
- **Simulación:** una solicitud de amistad a `pablo` pide una verificación
  simulada, etiquetada como tal y sin ventana
  ([captura](img/challenge-2026-10-17.png)).

Las invitaciones conservan su propio flujo (`VerifyInvitation`) y comparten
las cabeceras: ver [INVITATIONS.md](INVITATIONS.md).

## Gateway, navegación y permisos

`bootstrap.rs` traduce `READY` (variante de cuenta personal:
`merged_members`, `recipient_ids`, `read_state`, `properties`) al
`Snapshot` común: servidores, categorías, canales de texto, anuncios, voz,
escenarios (tipo 13), foros y medios, hilos y privados. El Gateway conserva
heartbeat, secuencia y reanudación dentro del adaptador, con límites de
tamaño y cola, plazos y reintentos acotados; tras un `READY` nuevo vuelve a
pedir lo suscrito (voz, lista de miembros, op 13).

- **Permisos:** los bits se traducen explícitamente
  (`bootstrap::PERMISSION_BITS`, incluidos `PIN_MESSAGES` 51,
  `SEND_POLLS` 49, `USE_EXTERNAL_STICKERS` 37, `CREATE_PUBLIC/PRIVATE_THREADS`
  35/36, `CREATE_EVENTS` 44, `MANAGE_EVENTS` 33, `MANAGE_WEBHOOKS` 29,
  `USE_APPLICATION_COMMANDS` 31). Roles, sobrescrituras y membresía propia
  deciden visibilidad y acciones; no se heredan los permisos de la
  simulación. Datos ausentes dejan el canal sin acceso; datos malformados
  detienen la sesión. Cada cambio de roles, canales o membresía recalcula
  antes de aceptar la siguiente acción.
- **Restricciones:** `communication_disabled_until`, `pending` y las
  cuarentenas de `flags` (bits 7, 8 y 10) llegan como `MemberRestrictions`;
  `Store::abilities` y la compuerta REST rechazan escribir, reaccionar o
  votar mientras duren (borrar lo propio sigue permitido).
  `COMPLETED_ONBOARDING` e `IS_GUEST` también se leen.
- **Lista de miembros** (`WatchMemberList`, op 37): se suscribe la página
  de 100 filas visible o las dos adyacentes; el nombre de la lista es
  `everyone` o el MurmurHash3 de las sobrescrituras de ver, como Serein. Al
  abrir un servidor se manda `channels: {}` y luego los rangos; cambiar de
  lista desuscribe la anterior. Ritmo: el último pedido gana, a lo sumo 20
  tramas por minuto y 500 ms entre cambios. Sin `SYNC`, se reenvía a los 15,
  30, 60 y 120 s. `GUILD_MEMBER_LIST_UPDATE` (`SYNC`, `INSERT`, `UPDATE`,
  `DELETE`, `INVALIDATE`) se aplica sobre una copia acotada; algo
  malformado se descarta sin cortar la sesión. Las personas de una tarjeta
  abierta (`WatchPeople`, hasta 8) van en `members` del mismo op 37.
- **Actualizaciones pasivas:** `PASSIVE_UPDATE_V1/V2` marcan no leídos y
  estados de voz de servidores no abiertos sin traer mensajes.
- **Hora de inicio de las salas** (op 43 `voice_start_time`, una vez por
  conexión y servidor mostrado; `CHANNEL_INFO` y
  `VOICE_CHANNEL_START_TIME_UPDATE`): la lista de canales muestra cuánto
  hace que hay gente («12 min»). Sólo salas de voz, como el cliente oficial.
- **Apps en voz:** `activity_instances` (`GUILD_CREATE`,
  `READY_SUPPLEMENTAL`) y `EMBEDDED_ACTIVITY_UPDATE_V2` llegan como
  `Event::VoiceActivity` (Serein no las lee).
- **Avisos del sistema:** las tarjetas `safety_system_notification` y
  `safety_policy_notice` de la cuenta «Discord» se muestran como aviso con
  título, texto, fecha y «Más información» sólo hacia dominios de Discord
  ([captura simulada](img/system-notice-2026-10-16.png)); otros tipos
  desconocidos, como tarjeta genérica con una nota.

## Mensajes

- **Historial y escritura:** REST pagina el historial al elegir una
  conversación; enviar, editar y borrar son comandos explícitos de la UI. La
  caché de autoría conserva 512 mensajes; si una edición o un borrado
  necesita recuperar el autor, un GET acotado (1 MiB, 15 s) vuelve al bucle
  para revalidar cuenta, canal y permisos antes de escribir.
  `message_context` lee alrededor de un id (`around`, 1–100, 8 MiB, 15 s) y
  exige el ancla; cualquier mutación del canal invalida la lectura.
- **Reacciones:** `ToggleReaction { add }` lleva el estado pedido (de dos
  clics rápidos gana el último); `PUT`/`DELETE …/reactions/{emoji}/@me`. El
  `204` y el eco del Gateway comparten un registro: el cambio cuenta una
  vez. Súper reacciones: se leen (`count_details`, `burst_colors`,
  `me_burst`, `MESSAGE_REACTION_*` con `type` 1) y se muestran como
  pastilla aparte; no se mandan. Reacciones y votos en vivo se guardan en
  SQLite sobre la fila del mensaje.
- **Encuestas:** crear (`poll` en el mensaje, `SEND_POLLS`), terminar
  (`…/polls/{m}/expire`), votar y ver quién votó
  (`…/polls/{m}/answers/{a}`). Un voto se confirma con el eco
  `MESSAGE_POLL_VOTE_*` propio; si no llega en 30 s (más el tiempo de un
  hCaptcha), se deshace. **Riesgo sin probar:** si Discord no manda ese eco,
  un voto aceptado se deshace.
- **Fijados:** `PUT`/`DELETE /channels/{c}/messages/pins/{m}`; en un
  servidor pide `PIN_MESSAGES` y `READ_MESSAGE_HISTORY`, como el cliente
  oficial (`MANAGE_MESSAGES` solo no alcanza). La lista lee páginas de 50
  hasta 250.
- **Typing:** `POST /channels/{c}/typing` sin evento de fallo; uno que
  esperó más de 5 s en cola se descarta. `TYPING_START` ajeno llega como
  `Event::Typing`.
- **Reenviar:** `message_reference` de tipo 1 sin contenido; el mensaje
  opcional sale después. Los reenvíos recibidos muestran texto con sus
  referencias y sus archivos (validados contra el canal de origen).
- **`@silent`** manda `flags: 4096`; **mencionar al responder** sigue el
  interruptor «@» (`allowed_mentions.replied_user`), también con archivos.
- **Enlaces:** `discord.com/channels/…` (con `ptb.`, `canary.`,
  `discordapp.com`) se muestran como pastilla y abren en RUNE; un canal no
  cargado se pide por id (`GET /channels/{id}`). Los dominios de Discord
  abren sin preguntar; los demás preguntan, con «Confiar en {dominio}».
  Marcas `<t:…>` en la zona local.
- **Contenido:** ThumbHash (`placeholder` v1) mientras la imagen carga;
  contenido explícito (bit 4 de `flags` de adjuntos y tarjetas,
  `content_scan_metadata`) tapado según el nivel de la cuenta (servidores,
  amigos, otros; sin niveles todavía, se tapa todo); `gifv` por su
  `proxy_url` de Discord, nunca el host del tercero; adjuntos de audio y
  video hasta 50 MiB; spoilers de tarjetas por `||url||`; registros de
  llamada con duración y llamada perdida; nombres de mensajes de sistema
  clicables ([captura](img/contenido-de-mensajes-2026-10-18.png)).
- **Emoji de servidor en línea:** `<:nombre:id>` y `<a:nombre:id>` se
  dibujan desde `cdn.discordapp.com/emojis/{id}.webp` (48 o 96 px si el
  mensaje es sólo emoji), también en biografías, temas de canal y estados.
- **Efímeros** (marca 64): sólo en la sesión, con «Descartar», sin contar
  como no leídos y sin guardarse.
- **Mensaje bloqueado por AutoMod:** ver [AutoMod](#automod).

### Adjuntos

Por la ruta de Serein (`http/uploads.rs`):

| Paso | Ruta |
| --- | --- |
| Destino de cada archivo | `POST /channels/{c}/attachments` `{"files": [{"id", "filename", "file_size", "is_clip": false}]}` |
| Bytes | `PUT {upload_url}` sin sesión ni cookies, sólo a `https://discord-attachments-uploads-prd.storage.googleapis.com` |
| Mensaje | `POST /channels/{c}/messages` con `attachments: [{"id", "filename", "uploaded_filename"}]`, nonce y `allowed_mentions` |

Core manda `Command::SendFiles` con los bytes congelados; el proveedor
exige VER, ESCRIBIR y ADJUNTAR (e HISTORIAL si responde), hasta 10 archivos
y el límite del canal. Corre al lado de la cola REST (un texto no espera una
subida), una a la vez. El progreso cuenta bytes entregados a HTTP cada
256 KiB. Cancelar antes del `POST` del mensaje no envía nada; después, el
fallo es ambiguo. El cliente del almacenamiento no reintenta ni sigue
redirecciones.

Límites: 500 MiB con Nitro, 50 MiB con Classic o Basic, 20 MiB sin plan;
las mejoras del servidor (nivel 2 y 3) suben a 50 y 100 MiB
(`Event::UploadLimit`). RUNE además los recorta a su presupuesto en memoria
(32 MiB). Las publicaciones de foro y los campos de archivo de un modal
preparan sus archivos igual y los nombran en su propio pedido.

## Lecturas y búsqueda

| Qué | Ruta o mensaje |
| --- | --- |
| Confirmar lectura | `POST /channels/{c}/messages/{m}/ack` `{"token", "manual": false}`; una en vuelo por canal, la más nueva reemplaza a la encolada |
| Marcar no leído | `POST …/messages/{m−1}/ack` `{"manual": true, "mention_count"}` (sólo el adaptador resta ids) |
| Servidor leído | `POST /guilds/{g}/ack`; después se confirman localmente sus canales legibles |
| Otro dispositivo | `MESSAGE_ACK` (uno con `version` menor se descarta) y `read_state` del `READY` |
| Buscar mensajes | `GET /guilds/{g}/messages/search` o `/channels/{c}/messages/search` con `content`, `author_id`, `mentions`, `has` (incl. `poll`, `sticker`), `channel_id`, `author_type`, `pinned`, `link_hostname`, `min_id`/`max_id`, `offset`; orden por fecha o relevancia |
| Índice no listo | `202` o código 110000–119999 con `retry_after`: la interfaz reintenta una vez |
| Buscar miembros | op 8 → `GUILD_MEMBERS_CHUNK` por nonce; 350 ms sin tipear, 1 s entre consultas, 15 por minuto, respuestas guardadas 5 min |

El adaptador numera cada confirmación con un contador propio que crece entre
reconexiones (el contrato pide versión creciente por cuenta y canal); la
`version` de Discord sólo descarta `MESSAGE_ACK` atrasados. El token del ACK
vive sólo en la sesión. Los resultados de búsqueda no son historial; las
fechas son el día local. Deliberadamente no: `ack-bulk` (una categoría
manda un ACK por canal), `has:forward`, `embed_type`, buscar en todo Discord.

## Perfiles e identidad visual

- **Perfil completo** (`LoadProfile`): `GET /users/{id}/profile` con
  mutuos sólo para otra persona y `guild_id` si hay servidor; respuesta de
  hasta 256 KiB, con ids validados. Lee biografía, pronombres, banner,
  color, datos del servidor, `joined_at`, insignias (cuenta y servidor),
  cuentas conectadas con enlaces seguros, y el efecto de perfil
  (`GET /collectibles-products/{sku}`, capas por `zIndex`). Una lectura a la
  vez; el `Store` guarda 32 perfiles que vencen al minuto. Un fallo deja la
  tarjeta «No disponible» sin toast.
- **Editar el perfil:**

  | Qué | Ruta |
  | --- | --- |
  | Nombre global y avatar | `PATCH /users/@me` `{global_name, avatar}` |
  | Biografía, pronombres, color, banner | `PATCH /users/@me/profile` |
  | Apodo y avatar en un servidor | `PATCH /guilds/{g}/members/@me` |
  | Biografía y banner en un servidor | `PATCH /guilds/{g}/profile/@me` |

  Las imágenes se eligen con el selector del sistema, se recortan donde la
  persona eligió (512×512 avatar, 960×384 banner) y viajan como data URI
  PNG, o GIF si se mueven y el plan lo permite (siempre re-codificadas: no
  sale nada del archivo original). Después se relee el perfil y eso es lo
  que se informa; si la relectura falla, el cambio queda «no confirmado».
  El plan (`premium_type`) decide avatar animado, banner y perfil por
  servidor (`Store::profile_abilities`). **Nunca** usuario, correo ni
  contraseña: Discord pide la contraseña y RUNE no la pide.
- **Datos visuales:** actividades (hasta 4, con imágenes de `app-assets`,
  `mp:` validado y portadas de Spotify, Twitch y YouTube desde sus hosts
  exactos, con límites de banner); `client_status`; decoraciones de avatar
  (`passthrough` sólo para animadas vigentes), nameplates (quieta por
  `asset`, animada por sku bajo el puntero), etiquetas de servidor; íconos,
  emoji y degradés de rol; avatar por servidor; avatar por defecto de
  Discord (`embed/avatars/{i}.png`, índice como el cliente oficial); ícono
  de la aplicación de una actividad sin imagen
  (`GET /applications/{id}/public`, una vez por aplicación y sesión). Una
  marca rota nunca hace fallar la cuenta; los cosméticos ocultos en Ajustes
  no piden sus imágenes.

## Cuenta sincronizada

| Qué | Cómo | Capacidad |
| --- | --- | --- |
| Estado y estado personalizado | `presence` del Identify y op 3; `settings-proto/1`, campo 11 | `presence`, `custom_status` |
| Estado de otras sesiones | `SESSIONS_REPLACE` | — |
| Notificaciones por servidor, canal y privado | `user_guild_settings`, `USER_GUILD_SETTINGS_UPDATE`, `PATCH /users/@me/guilds/{g\|@me}/settings` | `synced_notifications` |
| Carpetas y orden del riel | `settings-proto/1`, campo 14 | `server_folders` |
| Privacidad de mensajería y contenido explícito | `settings-proto/1`, campos 6, 8, 27 y 29 | `messaging_privacy` |
| Notas privadas | `GET`/`PUT /users/@me/notes/{id}`, `USER_NOTE_UPDATE` | `notes` |
| GIF favoritos | `settings-proto/2` (`FrecencyUserSettings`) | — |

`settings_proto.rs` es un decodificador protobuf mínimo propio (un paso de
generación no se justificaba), acotado por subárbol y respuesta. Cada
escritura lee primero, cambia sólo sus campos, conserva byte a byte el resto
(incluso lo desconocido) y manda `required_data_version`; si la respuesta no
coincide, el fallo es ambiguo y se relee. `USER_SETTINGS_PROTO_UPDATE` pide
releer.

- **Estado:** mientras la cuenta no se leyó, el Identify dice «invisible»
  (una cuenta invisible nunca aparece en línea al conectar). Cada cambio sale
  por op 3 (a lo sumo uno cada 5 s) con el estado personalizado y la
  actividad local, y se guarda en la cuenta. El emoji de servidor del estado
  se conserva.
- **Notificaciones:** nivel, silencio con vencimiento, `@everyone`, roles y
  ocultar silenciados; un servidor sin nivel propio toma su
  `default_message_notifications`; `mention_roles` y `mention_everyone`
  cuentan como mención salvo que se silencien
  ([captura](img/account-notifications-2026-10-17.png)). Sin la capacidad,
  todo sigue local.
- **Carpetas:** se normalizan como las muestra el cliente oficial; escribir
  exige que Discord tenga todavía lo último que se mostró (si otro
  dispositivo cambió algo, se avisa y se relee). Se arrastra o se usa el
  menú del riel ([captura](img/folders-2026-10-17.png)). Abierta o cerrada
  se guarda en este equipo.
- **Privacidad:** servidores sin mensajes directos, filtro de solicitudes,
  quién manda solicitudes de amistad, filtro de spam y nivel de contenido
  explícito ([captura](img/messaging-privacy-2026-10-17.png)).
- **Notas:** se leen al abrir un perfil (404 = vacía) y se guardan con
  Enter (hasta 256 caracteres).

## Amigos, privados y grupos

| Acción | Ruta | Confirma |
| --- | --- | --- |
| Enviar solicitud | `POST /users/@me/relationships` `{"username", "discriminator": null}` | `RELATIONSHIP_ADD` |
| Aceptar / bloquear | `PUT /users/@me/relationships/{id}` `{}` / `{"type": 2}` | `RELATIONSHIP_ADD/UPDATE` |
| Ignorar, cancelar, eliminar, desbloquear | `DELETE /users/@me/relationships/{id}` | `RELATIONSHIP_REMOVE` |
| Apodo de un amigo | `PATCH /users/@me/relationships/{id}` `{"nickname"}` | `RELATIONSHIP_UPDATE` |
| Abrir un privado / crear un grupo | `POST /users/@me/channels` `{"recipient_id"}` / `{"recipients": [ids]}` | la respuesta |
| Renombrar, imagen, pasar la propiedad | `PATCH /channels/{id}` `{"name"}`, `{"icon"}`, `{"owner"}` | la respuesta |
| Añadir / quitar en un grupo | `PUT` / `DELETE /channels/{id}/recipients/{usuario}` | `CHANNEL_RECIPIENT_ADD/REMOVE` |
| Abandonar un grupo, cerrar un privado | `DELETE /channels/{id}` | la respuesta |

Crear grupos, cambiar integrantes y pasar la propiedad salen de la REA del
cliente web (Serein no los tiene). Las relaciones las confirma el Gateway
(un evento repetido reemplaza); las de canal, la respuesta, para poder
escribir en un privado recién abierto. Un privado cerrado se recuerda (hasta
64) y vuelve con su próximo mensaje. Antes de salir se validan nombre de
usuario (como Serein), apodo, nombre de grupo (1–100), imagen PNG (256 KiB,
1024 px), de 2 a 9 amigos para crear, lugar libre (10) y que quien quita o
pasa la propiedad sea dueño. Los rechazos con código (10013, 30002, 30004,
40003, 50007, 80001–80007, 340007…) se explican en español. El apodo de un
amigo titula su privado; las solicitudes marcadas como spam van a «Posible
spam». La presencia de amigos sale de `presences`, `merged_presences` y
`PRESENCE_UPDATE` sin servidor.

## Expresiones

- Catálogos de emojis y stickers del servidor desde el estado inicial; el
  adaptador valida ids, nombres, roles y disponibilidad. El selector
  inserta en el cursor; enviar un sticker es una acción explícita que
  revalida canal, capacidad y catálogo.
- Con el plan conocido (`Event::Perks`): Classic y Basic permiten emojis de
  otros servidores y animados; Nitro y Basic, stickers de otros servidores.
  RUNE no envía lo que el plan no permite (el selector lo dice).
- Paquetes estándar (`GET /sticker-packs`, hasta 128); los Lottie se
  dibujan desde su JSON.
- GIF del servicio (`/gifs/search`, `trending-gifs`, `trending`, `suggest`,
  `trending-search`; «Más GIF» hasta la décima página), sólo desde hosts de
  KLIPY y Tenor; se inserta el enlace, nunca se envía solo.

Detalle de la interfaz en [ARTWORK.md](ARTWORK.md).

## Hilos y foros

| Acción | Ruta |
| --- | --- |
| Hilo desde un mensaje / propio | `POST /channels/{c}/messages/{m}/threads` / `POST /channels/{c}/threads` (`type` 11 o 12) |
| Publicación de foro | `POST /channels/{foro}/threads` con `applied_tags` y `message` (y archivos) |
| Seguir / dejar | `PUT` / `DELETE /channels/{hilo}/thread-members/@me` |
| Silenciar | `PATCH /channels/{hilo}/thread-members/@me/settings` `{"muted", "mute_config"}` |
| Renombrar, cerrar, bloquear, etiquetas | `PATCH /channels/{hilo}` |
| Eliminar | `DELETE /channels/{hilo}` |
| Abiertos de un foro | `GET /channels/{foro}/threads/search?archived=false…` (no documentada; Serein), con `first_messages` para las tarjetas |
| Abiertos de un canal | `GET /guilds/{g}/threads/active` filtrado |
| Archivados | `…/threads/archived/public`, `…/archived/private`, `…/users/@me/threads/archived/private` |

El Gateway de una cuenta personal sólo avisa de los hilos que sigue: por eso
abrir un foro pide la primera página de abiertos y «Cargar más» sigue con
los archivados (públicos y después privados). Las páginas se validan como en
Serein; una página nunca reemplaza un hilo que la sesión ya conoce, y un
`READY` nuevo conserva los leídos por página. El cursor de archivados
recuerda los ids del mismo instante para no saltearse ninguno. Antes de
salir se comprueban nombre, tipo de canal, permisos (crear público o
privado, `SEND_MESSAGES_IN_THREADS`, `MANAGE_THREADS` para bloquear y
eliminar), etiquetas (hasta 5, obligatoria si el foro la exige) y
aislamiento. Responder en un hilo archivado no bloqueado lo reabre, como el
cliente oficial. Un hilo silenciado no avisa de nada. Los escenarios tienen
chat por las mismas rutas que un canal de voz.

## Comandos e interacciones

| Qué | Cómo | Capacidad |
| --- | --- | --- |
| Índice de comandos | `GET /guilds/{g}/application-command-index` (o del canal en un privado con un bot) | `app_commands` |
| Correr un comando / sugerencias | `POST /interactions` tipo 2 / tipo 4 y `APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE` | `app_commands` |
| Botones y menús | `POST /interactions` tipo 3 | `interactions` |
| Modales (incl. archivos) | `INTERACTION_MODAL_CREATE` y tipo 5 | `interactions` |
| Respuestas | `INTERACTION_CREATE`, `_SUCCESS`, `_FAILURE` por nonce | — |

- Cada interacción lleva el `session_id` del Gateway (que nunca sale del
  adaptador) y un nonce snowflake que asocia las respuestas; lo de otra
  sesión se descarta y cada `READY` olvida lo anterior.
- El índice se acota y resuelve en el proveedor los overrides de usuario,
  roles y canales, como Serein; el `Store` decide qué comandos ofrece en
  cada canal y exige `USE_APPLICATION_COMMANDS`.
- Botones y menús se revisan contra la caché (existe, habilitado, valores
  válidos). Un modal de más de 10 partes o malformado se descarta; un campo
  que RUNE no completa se muestra y no deja enviar. Los campos de archivo
  (tipo 19) preparan sus archivos como un mensaje y respetan `file_types`,
  `min_values` y `max_values`; con archivos se espera 2 min.
- Sin respuesta en 30 s, RUNE deja de esperar y lo dice; una interacción a
  la vez. Las respuestas muestran «{persona} usó /{comando}».
- `/shrug`, `/tableflip`, `/unflip`, `/me`, `/spoiler`, `/gif`, `/sticker`
  y `/msg` son locales y sirven con cualquier proveedor.

## Administración del servidor

Rutas de Serein y, donde Serein no tiene nada, las documentadas o las del
cliente web. Mismo camino para todo: `admin::allowed` antes de salir, un
intento por escritura, la respuesta aplicada como el dispatch que representa
(`admin::confirmed`, así una orden siguiente valida sin esperar al Gateway),
y hCaptcha humano.

| Acción | Ruta |
| --- | --- |
| Crear servidor | `POST /guilds` (plantilla `2TffvPucqHkN`) |
| Perfil: nombre, descripción, color, ícono (GIF con `ANIMATED_ICON`), rasgos | `GET`/`PATCH /guilds/{g}/profile` |
| Opciones, seguridad, banner, comunidad, invitaciones pausadas | `PATCH /guilds/{g}` (`features` y flags leídos frescos antes) |
| Eliminar / salir | `DELETE /guilds/{g}` / `DELETE /users/@me/guilds/{g}` `{"lurking": false}` |
| Canales y categorías | `POST /guilds/{g}/channels`, `PATCH`/`DELETE /channels/{c}`; lote `PATCH /guilds/{g}/channels` con `lock_permissions` |
| Permisos de canal | `PATCH /channels/{c}` con la lista completa leída fresca |
| Foro | `PATCH /channels/{c}` (etiquetas, reacción, vista, orden, archivado, `default_tag_setting`, flags) |
| Roles | `POST`, `PATCH`, `DELETE /guilds/{g}/roles…`, orden por `PATCH /guilds/{g}/roles`, ícono/emoji/degradé, `roles/member-counts` |
| Miembros | roles `PUT`/`DELETE …/members/{u}/roles/{r}`; apodo y aislamiento `PATCH …/members/{u}`; expulsar `DELETE`; vetos `PUT`/`DELETE /guilds/{g}/bans/{u}` y su lista; páginas `POST /guilds/{g}/members-search` (no documentada); poda |
| Voz | `PATCH /guilds/{g}/members/{u}` `{"mute"}`, `{"deaf"}`, `{"channel_id"}` (mover, arrastrar o desconectar) |
| Registro de auditoría | `GET /guilds/{g}/audit-logs` (los tipos 13, 14 y 15 de «Permisos» se piden por separado y se mezclan) |
| Invitaciones | `GET /guilds/{g}/invites`, `POST /channels/{c}/invites`, `DELETE /invites/{código}` |
| Emojis y stickers | `/guilds/{g}/emojis`, `/guilds/{g}/stickers` (multipart) |
| Webhooks e integraciones | `/guilds/{g}/webhooks`, `/channels/{c}/webhooks`, `/webhooks/{id}`, `/guilds/{g}/integrations` |

Expulsar, vetar y aislar llevan el motivo en `X-Audit-Log-Reason`. Al
escribir permisos se conservan los bits que RUNE no modela. Antes de salir
se comprueban permisos, jerarquía (nadie actúa sobre su rango o el del
dueño, ni da permisos que no tiene), largos y límites documentados,
`features` del servidor para ícono animado, banner y degradés, y los
requisitos de una comunidad (canales de reglas y novedades, verificación
baja, filtro para todos). En el registro de auditoría sólo se leen las
claves que describen la acción: un token o secreto nunca sale del
adaptador. El token de un webhook se descarta al leer (copiar su URL no se
ofrece).

### Reglas del servidor y onboarding

`MEMBER_VERIFICATION_GATE_ENABLED` y `GUILD_ONBOARDING` en `features`
encienden `ServerOptions::rules_screening` y `onboarding`.

- **Leer:** `GET /guilds/{g}/member-verification?with_guild=false` (hasta 5
  campos; un tipo desconocido se muestra como no completable) y
  `GET /guilds/{g}/onboarding`.
- **Responder:** primero `POST /guilds/{g}/onboarding-responses`, después
  `PUT /guilds/{g}/requests/@me` con cada campo tal como vino más su
  respuesta; `application_status` da aprobado, rechazado o en revisión. A un
  invitado nunca se le pide el onboarding.
- **Editar:** `PUT /guilds/{g}/onboarding` (documentado), con `MANAGE_GUILD`
  y `MANAGE_ROLES` en una comunidad. Los ids nuevos son la hora en
  milisegundos, como el cliente oficial (sin probar si Discord los acepta).
- Capacidades `server_rules`, `onboarding` y `manage_onboarding`.

### Eventos programados

| Qué | Ruta |
| --- | --- |
| Lista | `GET /guilds/{g}/scheduled-events?with_user_count=true` y `GET /users/@me/scheduled-events?guild_ids={g}` |
| Interesados | `GET …/scheduled-events/{e}/users?limit=100&with_member=true` |
| «Me interesa» | `PUT`/`DELETE …/{e}/users/@me` |
| Crear, editar, iniciar, terminar, borrar | `POST`, `PATCH` (`status` 2 o 3), `DELETE /guilds/{g}/scheduled-events…` |

Las escrituras piden `CREATE_EVENTS` o `MANAGE_EVENTS` y un borrador
completo; de quién es el evento lo decide Discord. El Gateway trae
`GUILD_SCHEDULED_EVENT_*`. Un enlace de invitación con `?event={id}` muestra
su evento ([INVITATIONS.md](INVITATIONS.md)). Capacidades
`scheduled_events` y `manage_scheduled_events`.

### AutoMod

- **Reglas:** `GET`, `POST`, `PATCH`, `DELETE
  /guilds/{g}/auto-moderation/rules[/{id}]` con los tipos 1 (palabras), 3
  (spam), 4 (listas del servicio), 5 (menciones) y 6 (perfiles) y las
  acciones bloquear, alertar, aislar y bloquear la participación. Activar o
  desactivar manda sólo `{"enabled"}`. Una regla con un tipo o una acción
  desconocidos se muestra, se activa y se borra, pero no se edita (se
  perdería). Antes de salir: `MANAGE_GUILD`, límites documentados,
  expresiones regulares que parsea `regex-syntax` y `MODERATE_MEMBERS` si
  aísla.
- **Gateway:** `AUTO_MODERATION_RULE_*` y `AUTO_MODERATION_ACTION_EXECUTION`
  («Lo que hizo hace poco»). **Sin confirmar** que Discord los mande a una
  sesión de usuario.
- **Mensaje bloqueado:** un `400` con código 200000–200002 en un envío,
  edición o título termina con «la moderación automática del servidor lo
  bloqueó: «…»», con el `message` de la respuesta (las frases genéricas de
  Discord cuentan como sin mensaje).
- Capacidad `automod`; la UI pregunta a `ServerAbilities::manage_automod`.
  Capturas de la simulación: [lista](img/automod-2026-10-19.png),
  [editor](img/automod-editor-2026-10-19.png),
  [mensaje bloqueado](img/automod-bloqueado-2026-10-19.png).

## Invitaciones

Vista previa y aceptación nativa de invitaciones de servidor, grupo, amistad,
invitado y evento, con el hCaptcha resuelto por la persona: ver
[INVITATIONS.md](INVITATIONS.md).

## Escritorio y actividad local

La parte de escritorio está en [PLATFORM.md](PLATFORM.md); el proveedor
aporta:

- **Actividad local** (`SetLocalActivity`, `local_activity`): va en el mismo
  op 3 que el estado, primero en `activities` (invisible manda `[]`). La de
  un programa espera su nombre (`GET /applications/{id}/rpc`) y sus imágenes
  (`/oauth2/applications/{id}/assets`, `/applications/{id}/external-assets`);
  una aplicación que Discord no nombra no se publica.
- **Juegos reconocibles** (`LoadDetectableGames`): `GET
  /applications/detectable`, pública y **sin** la credencial, hasta 24 MiB,
  una vez por sesión mientras la detección esté encendida.
- **Códigos DAVE** (`voice_privacy_code`): `Event::CallPrivacyCode` y
  `VoiceVerificationCode` → `Event::VoiceVerification`; ver
  [MEDIA.md](MEDIA.md).

Socket `discord-ipc` y detección de juegos van apagados por defecto, cada
uno con su interruptor.

## Voz

Se une a la voz con los mismos mensajes que Serein, más lo que muestra la
REA del bundle web y de `discord_voice.node`. Lo de audio, video y captura
del lado de RUNE está en [MEDIA.md](MEDIA.md) y [VIDEO.md](VIDEO.md).

### Gateway principal

- `JoinVoice` envía op 4 (`guild_id` o `null` en privados, `channel_id`,
  `self_mute`, `self_deaf`, `self_video`); salir, el mismo op con canal
  nulo. Va por un `watch` (gana el último pedido) dentro del presupuesto de
  escritura.
- `VOICE_STATE_UPDATE` da quién está en voz y, de la cuenta, su
  `session_id` (nunca sale del adaptador); `VOICE_SERVER_UPDATE`, token y
  endpoint. El endpoint sólo se acepta en `*.discord.media` con el puerto
  que diga Discord (`:80` pasa a 443, como el cliente oficial); otro termina
  el intento enseguida. Sin servidor en 15 s, la llamada termina con motivo.
- **Este dispositivo:** como el cliente oficial, un estado de la cuenta es
  de este dispositivo si su sesión es la del Gateway propio. Uno con otra
  sesión antes del propio se ignora; después, la llamada siguió en otro
  dispositivo: termina con «Tu llamada siguió en otro dispositivo.» sin
  mandar op 4 de salida (colgaría al otro). Si un `READY` nuevo llega sin el
  servidor de la llamada, el op 4 espera su `GUILD_CREATE` (15 s) para
  aplicar la misma regla.
- **Silencio del servidor** (`mute`/`suppress`, `deaf`) va aparte del propio
  y retiene el micrófono. Sin `SPEAK` se entra a escuchar; sin `USE_VAD`,
  pulsar para hablar.
- **Reunirse:** si una llamada que conectó se pierde (salvo el cierre 4014),
  se reúne hasta 3 veces a 1, 3 y 10 s identificando con la misma sesión y
  servidor y mandando op 4 otra vez; el contador vuelve a cero tras 30 s
  conectada.
- **Escenarios** (tipo 13, `Capabilities::stages`; rutas de la REA, Serein
  no los tiene): el lugar de cada persona sale de `suppress` y
  `request_to_speak_timestamp`. Levantar la mano, aceptar la invitación,
  subir y bajar con `PATCH /guilds/{g}/voice-states/@me`; la moderación
  invita o pasa al público con `…/voice-states/{usuario}`.
- **Fases:** `Securing` («Asegurando el audio…») entre transporte y grupo
  DAVE; `Waiting` («Esperando a los demás») conectada y sola.

### Gateway de voz y transporte

- `wss://<endpoint>/?v=8`: Hello → Identify (`server_id`, `channel_id`,
  `user_id`, `session_id`, `token`, `max_dave_protocol_version: 1`) → Ready
  (SSRC, IP pública, modos) → descubrimiento de IP por UDP (reintento cada
  segundo hasta 8 s) → Select Protocol → Session Description (clave de 32
  bytes en `Zeroizing`; sin `dave_protocol_version: 1` la llamada no
  empieza). Heartbeat cada `min(intervalo, 5 s)`, tolerando un ACK tarde.
  Op 5 anuncia el SSRC antes de cualquier medio y tras un resume. Opcodes
  desconocidos se ignoran.
- Cierres: 4015 y caídas retoman con op 7 hasta 3 veces; 4014, 4006, 4009,
  4004, 4011/4012/4016 y 4017 terminan con un motivo en español. Límites de
  mensajes (64 KiB, 256 por segundo), datagramas y 30 s para conectar y para
  acordar DAVE.
- **Cifrado de transporte:** sólo `aead_xchacha20_poly1305_rtpsize` (el que
  Discord exige soportar y usa Serein); nunca se reusa un nonce.
  `aead_aes256_gcm_rtpsize` no está implementado.
- **DAVE (E2EE, protocolo 1)** sobre `davey`: identidad efímera por
  llamada, paquetes de claves, propuestas sólo de personas anunciadas,
  commit/welcome, transiciones y recuperación (op 31, hasta 3 veces). Pedir
  protocolo 0 corta la llamada. Nada sale sin grupo listo. Las claves viven
  en la conexión y se borran al salir, cambiar de cuenta, cerrar sesión o
  perder el Gateway.
- **Calidad:** cada ACK del latido mide RTT y pérdida (huecos de secuencia
  del audio ajeno) y llega como `quality` de `Event::CallPhase`.
- **Dependencia:** `davey` arrastra `hpke-rs`; RUNE lo parchea
  ([`vendor/hpke-rs`](../vendor/hpke-rs/RUNE-PATCH.md)) para cambiar
  `libcrux-sha3`, afectado por RUSTSEC-2026-0207/0208, por `sha3` de
  RustCrypto ([SECURITY.md](SECURITY.md)).

### Llamadas privadas

Capacidad `private_calls`, en privados y grupos.

- **Llamar:** op 13 `{channel_id}` y op 4 con `guild_id: null`; el ring
  (`POST /channels/{id}/call/ring`) espera a que Discord tenga la llamada
  (`CALL_CREATE`), como el cliente oficial. «Dejar de llamar» a alguien es
  `…/call/stop-ringing`. En un privado 1:1 se pide antes
  `GET /channels/{id}/call` (`CheckCall`): sólo suena si `ringable`; si
  Discord rechaza el pedido (400/403/404) y no son amigos, el diálogo «No se
  puede llamar a…» ofrece la solicitud de amistad.
- **Llamadas:** `CALL_CREATE/UPDATE/DELETE`; a quién suena sale de
  `ongoing_rings` y `ringing`. Una entrante muestra Aceptar/Rechazar
  (rechazar es `stop-ringing` con la cuenta), suena cada 2,5 s y avisa por
  notificación del sistema con botones si la ventana no tiene el foco;
  nada de eso en No molestar.
- **Estados:** sonando, conectando, en vivo, sin respuesta, rechazada,
  cortó, sola, entrante, en curso sin la cuenta y en otro dispositivo.
  Rechazada frente a sin respuesta es una **inferencia** (salir de
  `ringing` antes o después de 20 s).
- La voz identifica con `server_id` = el canal; transmitir usa op 18 de tipo
  `call` y claves `call:{canal}:{dueño}`. No hay vistas previas de
  transmisiones en privados.

### Video

`watch_streams`, `watch_cameras` y `share_screen`; el diseño completo está
en [VIDEO.md](VIDEO.md).

- **Mirar una transmisión:** op 20 `{stream_key}` (clave
  `guild:{servidor}:{canal}:{dueño}`), op 19 para soltar y op 21 para pedir
  de nuevo, siempre después del op 4. `STREAM_CREATE`,
  `STREAM_SERVER_UPDATE` (token hasta 2048 bytes, mismo control de
  endpoint), `STREAM_UPDATE` y `STREAM_DELETE` (con `unavailable` reconecta;
  otros motivos terminan en español). Una segunda conexión de voz con
  `server_id = rtc_server_id` recibe H264 (101, RTX 102) cifrado con DAVE,
  lo arma (`voice::h264`) y lo entrega al host en Annex B; ante pérdida o
  descifrado fallido manda PLI (uno cada 500 ms por SSRC). El sonido de la
  transmisión llega como fuente aparte. Hasta 3 transmisiones a la vez.
- **Cámaras:** la conexión de la llamada también decodifica; op 15 pide 100
  para las cámaras visibles y 0 para el resto (hasta 4), repetido cada 5 s;
  con simulcast pide una sola capa.
- **Compartir pantalla:** op 18 crea la transmisión recién cuando la persona
  eligió la pantalla en el diálogo del sistema y llegan sus cuadros; la
  conexión emisora anuncia `streams` y op 12 con la calidad elegida, cifra
  con DAVE, normaliza el SPS, empaqueta en RTP de hasta 1200 bytes y marca
  el ritmo. PLI y FIR piden cuadro clave; pérdida y REMB mueven la tasa
  (`voice::send::Rate`, entre 250 kbit/s y el máximo de la calidad). Op 22
  pausa. El sonido de la pantalla sale por su SSRC con op 5 `speaking: 2` y
  la extensión RTP ID 9 valor 4 de Serein (**sin verificar**).
- **RTX y NACK:** quien envía guarda 1 s de historial y reenvía por RTX;
  quien recibe pide lo que falta con NACK (Serein no manda NACK; el cliente
  oficial sí).
- **Vistas previas:** `GET /streams/{clave}/preview`, aceptada sólo desde
  el CDN de Discord bajo su clave (`stream_previews`).

### Diagnóstico

Cada paso de la voz deja una línea `rune: <ámbito>: …` en stderr, con ámbitos
`voice call`, `voice private call`, `voice stream`, `voice share` y
`gateway`. Nunca llevan token, `session_id`, claves, mensajes MLS, ids de
personas ni la IP propia (un test lo comprueba): son seguras de compartir.

| Línea | Qué dice |
| --- | --- |
| `gateway: op 4 sent…`, `op 13 sent` | se pidió la llamada o su estado |
| `voice call: own VOICE_STATE_UPDATE: …` | el estado propio y si es de este dispositivo |
| `voice call: VOICE_SERVER_UPDATE: endpoint <host:puerto> (accepted\|refused…)` | el servidor asignado |
| `HELLO`, `IDENTIFY sent`, `READY (op 2): modes offered […]` | apretón y cifrados ofrecidos |
| `IP discovery request sent (try N)` / `reply received` | UDP; sin respuesta en 8 s suele ser un cortafuegos |
| `SESSION_DESCRIPTION (op 4): mode …, dave_protocol_version …` | transporte acordado |
| `DAVE op N received/sent` | E2EE, sólo el número |
| `connected: media may flow` | llamada lista |
| `voice gateway closed by Discord: code C «razón»` | por qué cortó Discord |
| `dropped (…); resume N/3`, `lost: …; rejoin attempt N`, `ended: …` | reanudación, reunión o final |
| `voice private call: CALL_CREATE…`, `ringing (call/ring)`, `may it ring (GET /call)? ringable: …` | llamadas privadas |
| `voice share: STREAM_CREATE…`, `STREAM_DELETE: reason …`, `video …`, `network …` | transmisión propia: por qué terminó y, cada 5 s, captura, codificador y red |

Si algo falla: correr RUNE desde una terminal y copiar las líneas
`rune: voice …` y `rune: gateway …` junto con el texto del aviso.

## Límites y diferencias deliberadas

- **Nunca:** cambiar usuario, correo o contraseña; copiar la URL de un
  webhook; mandar súper reacciones; reenviar encuestas (Discord no lo
  permite); automatizar desafíos.
- **Protocolo:** voz `?v=8` (el oficial usa v9/v10), sin `flags` ni
  `preferred_regions` en op 4/18, sin `rtc_connection_id`/`experiments` en
  Select Protocol, sin op 5 `VOICE_SERVER_PING` al reconectar, sin
  `stop-ringing {}` al colgar (Discord termina la llamada vacía). Lectura en
  lote (`ack-bulk`) no se usa.
- **Atajos con límite conocido:** el producto del efecto de perfil se lee en
  cada perfil abierto, sin caché; la presencia de quien muestra su tarjeta
  sólo se suscribe con la lista de miembros abierta; las tarjetas de foros
  archivados no traen primer mensaje (falta `post-data`); un lanzamiento de
  app en voz terminado mientras el servidor no estaba disponible queda hasta
  el próximo `READY`.
- **Sin confirmar:** el formato real de la nameplate animada por sku, la
  extensión RTP del sonido de pantalla, que `MANAGE_MESSAGES` solo ya no
  fije, el eco propio de un voto, los eventos de AutoMod para cuentas de
  usuario, la exigencia de DAVE en escenarios.

## Riesgos

- Discord puede rechazar, limitar o suspender la cuenta por usar su API
  fuera de los clientes oficiales; presentarse como el cliente web no lo
  cambia. Un rechazo se documenta como incompatibilidad, no se convierte en
  promesa de acceso completo.
- Las rutas no documentadas (perfil completo, lista por suscripción,
  búsqueda en foros, `members-search`, `settings-proto`) pueden cambiar sin
  aviso.
- Todo lo que no sea login, identidad y lectura depende de una prueba manual
  del dueño contra el servicio; hasta entonces no se afirma que funcione.

## Pruebas

Todas en `crates/rune-discord-session` con servidores HTTP, WebSocket y UDP
en loopback, identidades sintéticas y login falso: `tests/session/*` (el
proveedor de punta a punta por área), `gateway_tests.rs`, `voice::tests` y
`voice/tests/private.rs` (servidor de voz sintético con su propio emisor
MLS, H264 codificado con OpenH264 en la prueba), `tests/keyring.rs` (llavero
falso), `browser::tests`, `settings_proto.rs` y los tests de cada módulo.
Las pruebas nativas del auxiliar de login piden pantalla:

```sh
nix-shell --arg withLogin true --run 'cargo test -p rune-login --features webview -- --include-ignored --test-threads=1'
node crates/rune-login/tests/handoff.cjs
```
