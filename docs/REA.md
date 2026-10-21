# REA del cliente oficial: qué usa RUNE

Lo que el proveedor no oficial (`rune-discord-session`) sabe de rutas,
cuerpos, mensajes de Gateway, comportamiento e interfaz del cliente
oficial de Discord y que no sale de la documentación pública ni de Serein.
Cada sección dice qué se encontró, qué hace RUNE con eso y qué queda sin
confirmar. Lo implementado se describe en
[DISCORD_SESSION.md](DISCORD_SESSION.md); el diseño de video, en
[VIDEO.md](VIDEO.md).

**Nada de lo que sigue se probó contra el servicio real**: son lecturas
estáticas, y RUNE las prueba con fixtures en loopback.

## Alcance autorizado

- **2026-10-16:** el dueño autorizó REA sobre el cliente oficial de Discord
  —el paquete instalado y los bundles JavaScript públicos— para conocer
  rutas, mensajes de Gateway, comportamiento e interfaz, y usar lo
  encontrado en el proveedor no oficial. Se reimplementa: no se copia
  código, textos ni recursos (las imágenes y los textos de RUNE son
  propios).
- **2026-10-19:** el proveedor no oficial puede presentarse como el cliente
  web en un navegador (user agent, `X-Super-Properties`, huella,
  propiedades del Identify), con una sola identidad coherente en REST,
  Gateway y la WebView del hCaptcha. Ver
  [SECURITY.md](SECURITY.md).
- **2026-10-21:** los efectos de sonido públicos de Discord pueden
  empaquetarse como sonidos predeterminados de RUNE
  ([AGENTS.md](../AGENTS.md), [MEDIA.md](MEDIA.md#sonidos)); la excepción
  no abarca código ni otros recursos.
- **Nunca:** ejecutar el cliente oficial o sus addons, leer sus perfiles,
  sesiones, cachés, memoria o tráfico, automatizar o reenviar CAPTCHA/MFA,
  ni usar cuentas reales en las pruebas.

Serein es código abierto: se estudia leyendo sus fuentes en
`references/serein` y `references/serein-current`, sin copiarlo.

## Cómo se hace

Los bundles se bajan **sin cookies, cabeceras de sesión ni login** desde los
que lista `https://discord.com/app` (los chunks con hash se sirven como
`/assets/<hash>.js`, con sus hojas CSS) a un directorio propio del
scratchpad, y se leen con `grep -bo` y scripts propios (Python con `-I`);
nada se ejecuta. Del paquete de escritorio instalado (Nix) sólo se leen
cadenas (`strings -t x`) sin cargar los módulos; Ghidra sirve para
descompilar funciones puntuales de un addon nativo.

Las referencias de abajo son ids de módulo del bundle o desplazamientos en
bytes. Bundles principales:

| Archivo | Build | SHA-256 |
| --- | --- | --- |
| `web.c5b338cc7e610833.js` | anterior | `6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec` |
| `web.4d4b2f75fdea5470.js` | 641004 | `667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae` |
| `fast-connect.f0cfe6eb7552ae67.js` | anterior | `a05f649b4568ee0241848c1e80f0c998aaddbeef26b5693d1822c676505e7f7c` |
| `933ace571a9c6560.js` (textos en inglés) | 641004 | `d9039967920acd48ce3a2b3409e14b63724f49cce42484e63d351f6f264632fc` |
| `discord_voice.node` (Discord 1.0.161) | — | `28300009b1b5a9259e6f77454a6d800155fed808ed013ac53235f8d0b6049a23` |

Las dos versiones de `web.*` tienen el mismo código en desplazamientos
parecidos. El renderizador de muchas pantallas vive en chunks diferidos que
no siempre están entre los bajados; eso se marca como «sin confirmar».

## Identidad de navegador

Página `app.html` (SHA-256 `1b10e05a…6d17`) y `web.4d4b…`. Uso en RUNE:
`rune-discord-session::browser` arma las piezas y `http::api` las pone en
cada pedido.

- **Build:** `window.GLOBAL_ENV.BUILD_NUMBER` de la página; el bundle lo usa
  como `client_build_number`.
- **Super properties** (módulo 562708): `os`, `browser`, `device`,
  `device_model_name: ""`, `system_locale` (`navigator.languages[0]`),
  `has_client_mods`, `browser_user_agent`, `browser_version`, `os_version`,
  los `referrer`/`referring_domain` de la primera visita y los `…_current`,
  `release_channel`, `client_build_number`, `client_event_source: null` y
  `client_launch_id` (un UUID por carga). `launch_signature` sólo si cargó
  el WebAssembly `libdiscore`; si no, `null`. `client_heartbeat_session_id`
  y `client_app_state` (`focused`/`unfocused`) se suman antes de cada pedido
  (módulo 618651). La cabecera es base64 del JSON.
- **Cabeceras** (`prepareRequest`): `Authorization`, `X-Super-Properties`,
  `X-Fingerprint` si lo hay, `X-Installation-ID` si lo hay,
  `Accept-Language` sólo en la aplicación de escritorio, `X-Discord-Locale`
  (idioma de la interfaz) y `X-Discord-Timezone`
  (`Intl.DateTimeFormat().resolvedOptions().timeZone`).
- **`X-Fingerprint`** viene de `/experiments` sin sesión; `LOGIN_SUCCESS`,
  `REGISTER_SUCCESS` y `UPDATE_TOKEN` lo borran: con sesión no se manda.
- **Idioma de la interfaz** (módulos 773669 y 226536): el de la cuenta; si
  no, el del navegador llevado a la lista de Discord (`es`→`es-ES`,
  `nb`/`nn`→`no`, `zh-Hant`→`zh-TW`, o el primero del mismo idioma en el
  orden de la lista, que pone `es-419` antes de `es-ES`).
- **Identify:** `properties` son las super properties más
  `client_app_state`, `is_fast_connect: false`, `gateway_connect_reasons`
  (`COLD_START`) e `installation_id` si lo hay. El cliente manda además
  `capabilities`, `client_state` y `qos_token`, que RUNE no manda (cambian
  la forma de las cargas).
- **Navegador y sistema** (`getBrowser`, `getOS`): `Safari` si
  `/Apple/.test(navigator.vendor)` antes de mirar el user agent (así se
  nombra WebKitGTK); con `navigator.userAgentData.brands`, `Edge` sólo si
  una marca es exactamente `Microsoft Edge` (un user agent moderno dice
  `Edg/`, no `Edge`: sin esa marca es `Chrome`). `getOS`: `Windows` si el
  user agent lo dice, antes que iOS, Android, Mac y Linux.
  `browser_version` y `os_version` salen de platform.js 1.3.6 (chunk
  `40715.ce9980776791e583.js`): `Microsoft Edge` con
  `(?:Edge|Edg|EdgA|EdgiOS)`, la versión del primer token de esa familia, y
  `NT 10.0` → `10` (también Windows 11).

En RUNE (`browser::Engine`) esas piezas se derivan de lo que lee la página
de prueba en la WebView real (WebKitGTK en Linux, WebView2 en Windows); no
se adivinan las marcas. Sin probar en un Windows real.

## Invitaciones y verificación

**Aceptar** (`acceptInvite`): un solo `POST /invites/{code}` para las tres
clases, con cuerpo `{session_id, invite_instance_id,
received_installation_id}` (`session_id` del Gateway) y `context` en
`X-Context-Properties` = base64(JSON). El modal arma el contexto con
`getInviteContext("Desktop Invite Modal", invite)` = `{location,
location_guild_id, location_channel_id, location_channel_type}` y suma
`invite_guild_scheduled_event_id` si el enlace nombra un evento (`?event=`).
Una cuenta en cuarentena no intenta unirse. El éxito despacha
`INVITE_ACCEPT_SUCCESS` (`guild`, `new_member`, `show_verification_form`,
canal); la membresía llega por Gateway, y RUNE espera el `GUILD_CREATE`.
Con `IS_GUEST_INVITE` no abre el onboarding; una invitación de amistad abre
el privado si ya son amigos.

**Clases y banderas** (módulos 172799, 346542): `GUILD` 0, `GROUP_DM` 1,
`FRIEND` 2; sin `type`, canal de grupo → grupo, sin servidor y con quien
invita → amistad. Banderas: `IS_GUEST_INVITE` 1, `IS_VIEWED` 2,
`IS_ENHANCED` 4, `IS_APPLICATION_BYPASS` 8. RUNE acepta la de invitado
sólo exacta y no tiene flujo propio para las otras.

**Enlaces con tarjeta** (módulo 721779): del contenido entero, sin bloques ni
código en línea, sin puntuación final (`[.,;:?'*_~|]+$`), uno por código y
como mucho **diez**. Formas: `discord.gg/{código}` y `/invite/{código}` en
`discord.com`, `discordapp.com`, `ptb.` y `canary.`. RUNE además oculta las
de un spoiler, como Serein.

**Desafío** (`interceptResponse`): sólo `400` con `body.captcha_key` abre el
desafío. `extractCaptchaPropsFromResponse` lee `captcha_service`,
`captcha_sitekey`, `captcha_session_id`, `captcha_rqdata`,
`captcha_rqtoken`, `should_serve_invisible` y `user_flow`; servicios
`hcaptcha`, `recaptcha`, `recaptcha_enterprise` y `turnstile` (sin
`captcha_service`, reCAPTCHA). El widget hCaptcha se carga explícito
(`api.js?render=explicit`, `setData({rqdata})`, `execute()` si es
invisible). El reintento repite la misma petición con `X-Captcha-Key` y, si
existen, `X-Captcha-Rqtoken` y `X-Captcha-Session-Id`. La misma
interceptación trata 401 `MFA_REQUIRED` (60003), 403
`GUILD_LIMITED_ACCESS_*` (400000–409999) y `RESTRICTED_HOURS_ACTIVE`
(40117) con sus propios modales.

En RUNE (`http::challenges`, `http::rejections`): sólo la forma confirmada
(400, `captcha_key`, `captcha_service: "hcaptcha"`, valores acotados) abre
la ventana aislada donde la persona lo resuelve, y se reintenta una vez con
las tres cabeceras. No se inventan `invite_instance_id` ni
`received_installation_id`.

**Por qué se rechaza** (`AbortCodes`; textos del módulo 4274):

| Código | Nombre | Texto oficial (resumen) | En RUNE |
| --- | --- | --- | --- |
| 10006 | `UNKNOWN_INVITE` | — | `Invalid` |
| 20024, 20086, 700000, 700002 | edad | «because of your age group», «Confirm your age» | `AgeRestricted` |
| 30001 | `TOO_MANY_USER_GUILDS` | «You are at the {100\|200} server limit» | `ServerLimit` |
| 30019 | `GUILD_AT_CAPACITY` | «This server is currently full» | `ServerFull` |
| 40007 | `USER_BANNED` | sin texto propio | `Banned` |
| 40068 | `USER_QUARANTINED` | sin texto propio | `AccountLimited` |
| 40069 | `INVITES_DISABLED` | «Invites are currently paused» | `InvitesPaused` |
| 50095 | `INVALID_COUNTRY_CODE` | «not available in your location» | `Region` |
| 400002 | `GUILD_JOIN_INVITE_LIMITED_ACCESS` | «features are limited» | `LimitedAccess` |

Cualquier otro: «Unable to accept invite». El cliente sólo guarda `code` y
`message` del cuerpo. RUNE (`refused` en `http/invitations.rs`) da textos
propios en español e inglés ([INVITATIONS.md](INVITATIONS.md)).

## Mensajes

**Tipos de embed:** `image`, `video`, `link`, `article`, `tweet`, `rich`,
`gifv`, `application_news`, `auto_moderation_message`,
`auto_moderation_notification`, `text`, `post_preview`, `gift`,
`safety_policy_notice`, `safety_system_notification`,
`age_verification_system_notification`, `voice_channel`,
`gaming_profile`, `poll_result` y `components`. Los tres avisos de
seguridad van con los de moderación; RUNE los muestra como tarjeta de aviso.
Hay dos conjuntos: los que conservan sus `fields` crudos
(`auto_moderation_*`, `rich`, los avisos, `voice_channel`, `poll_result`) y
los que `renderEmbeds` salta (avisos, `application_news`, `post_preview`,
`voice_channel`, `poll_result`). Un embed con `components` los convierte
como los del mensaje.

**`voice_channel`: el cliente no lo dibuja.** Nadie lee sus campos (se buscó
en todos los chunks que nombra el runtime). El tipo de mensaje 45
(`VOICE_HANGOUT_INVITE`) vacía el contenido y no tiene renderizador; el 65
(`VOICE_SESSION`) usa `message.call` y `message_reference.channel_id`, sin
embed. RUNE dibuja la tarjeta con las partes estándar del embed y saca el
canal sólo de un enlace `https://discord.com/channels/{servidor}/{canal}`
en `url` (sin canal, «Unirse» queda deshabilitado con motivo).

**Mensajes de voz** (el dueño, 2026-10-16: mostrarlos y reproducirlos;
grabar y enviar queda fuera):

- Flag `IS_VOICE_MESSAGE = 8192`; adjunto con `duration_secs` y `waveform`;
  `audio/ogg` con Opus.
- Forma de onda en base64, cada byte / 255 es una amplitud. Inválida: el
  marcador plano de cinco ceros.
- Dibujo: ancho 40 px hasta 0,5 s, 294 px desde 45 s, proporcional entre
  ambos y redondeado a pasos de 6 (`6·⌊(w+4)/6⌋−4`); barras `⌊(ancho+4)/6⌋`
  de 2 px con 4 de separación (promedio por tramos o ceros); alto `22·v+2`
  centrado; las barras antes de `round(tiempo/duración·barras)` en el color
  activo.
- Controles: tiempo total redondeado hacia arriba antes de empezar y lo que
  falta después (`--:--` si se desconoce); velocidad 0,75/1/1,5/2; arrastrar
  las barras busca; la posición se recuerda por clip. Extracto «voice
  message» sin texto; no se edita; permiso `SEND_VOICE_MESSAGES`; el menú
  ofrece guardar el audio.

En RUNE (`wire.rs`, `message/voice.rs`): se reconoce por `duration_secs`
(0 < duración ≤ 3600 s; onda ≤ 256 bytes, base64 ≤ 344 caracteres), sin
exigir el flag (un `MESSAGE_UPDATE` sin `flags` no debe volverlo archivo).
La velocidad no cambia el tono (WSOLA propio). Sin contenedores Opus de más
de dos canales.

**Resultado de encuesta (tipo 46):** el primer embed, con `fields`
`poll_question_text`, `victor_answer_id`, `victor_answer_text`,
`victor_answer_votes`, `total_votes` y el emoji en
`victor_answer_emoji_id/_name/_animated`. Estados: 0 votos → «No hubo un
ganador»; con ganador → respuesta y «Respuesta ganadora • N %»; sin ganador
→ «El resultado fue un empate». Porcentaje `round(votos / total · 100)`. La
línea y el botón «Ver encuesta» saltan a `message_reference`.

**Componentes:** flag `IS_COMPONENTS_V2 = 1 << 15` (el mensaje es sólo
componentes; al editar, el texto sale de los `TEXT_DISPLAY`). Tipos: 1 fila,
2 botón, 3 menú de texto, 4 campo de texto, 5–8 menús de
usuario/rol/mencionable/canal, 9 sección, 10 texto, 11 miniatura, 12
galería, 13 archivo, 14 separador, 16 inventario, 17 contenedor, 18
etiqueta, 19 subida de archivo, 20 checkpoint, 21–23 grupos de
opción/casillas. Botones: 1 primario, 2 secundario, 3 éxito, 4
destructivo, 5 enlace, 6 premium. Separador `divider` (por defecto `true`)
y `spacing` 1 o 2. Una sección sin texto o sin accesorio se descarta; un
tipo desconocido también (RUNE deja una línea visible). De Serein, los
límites (40 componentes, profundidad 8, 128 KiB) y no duplicar los adjuntos
de un mensaje v2.

- El menú de texto arranca con las opciones `default: true` y, cerrado, las
  muestra en lugar del placeholder. Los menús 5–8 resuelven
  `default_values` en sus stores; RUNE todavía no.
- El emoji de botones y opciones pide `/emojis/{id}.webp?size=…`; sin id,
  es Unicode. RUNE usa la imagen fija del selector (`size=128`) y
  `:nombre:` mientras carga o si falla.

**Contenido explícito** (`rune-state::content_filter`):

- Banderas del adjunto: `IS_CLIP` 1, `IS_THUMBNAIL` 2, `IS_REMIX` 4,
  `IS_SPOILER` 8, `CONTAINS_EXPLICIT_MEDIA` 16, `IS_ANIMATED` 32,
  `CONTAINS_GORE_CONTENT` 64, `CONTAINS_SELF_HARM_CONTENT` 128. Tarjeta: 16,
  64 y 128 iguales. Medio de componentes: `content_scan_metadata.flags`
  (explícito 1, violento 2, autolesión 4).
- Decisión del cliente: por tipo de daño, el ajuste
  `explicitContentSettings` (0 sin elegir, 1 mostrar, 2 difuminar, 3
  bloquear) según canal y autor (servidor; privado de amigo; privado de no
  amigo; el autor propio nunca). Sin elegir, el caso adulto sigue el filtro
  viejo `explicit_content_filter`. Un adjunto sin `content_scan_version`
  también se tapa mientras se escanea. Los reenvíos se miran en
  `message_snapshots`.
- RUNE: sólo explícito (bit 16 y bit 1 de `content_scan_metadata`), con
  `Store::obscures_explicit`; sin elegir, servidores y amigos se ven y los
  demás se tapan.

**Registro de llamada (tipo 3):** `call: {participants, ended_timestamp}`;
la duración es `ended_timestamp − timestamp`. Perdida si la llamada no está
activa y la cuenta no está entre los participantes. El nombre enlaza al
perfil, como en los demás mensajes de sistema.

**Fijados, súper reacciones y búsqueda:**

- `PIN_MESSAGES` es el bit 51; fijar exige `PIN_MESSAGES` **y**
  `READ_MESSAGE_HISTORY` en un servidor (Serein sólo mira
  `MANAGE_MESSAGES`). RUNE hace lo mismo.
- Con `count_details`, `burst_count = burst ?? 0` y `count = normal ?? 0`.
  La súper reacción optimista suma a `burst_count`, marca `me_burst` y toma
  `burst_colors` del evento sólo si no tenía. RUNE: `Reaction { count,
  burst }`, pastilla aparte con el primer color; no manda súper reacciones.
- Orden de la búsqueda: `timestamp/desc`, `timestamp/asc`,
  `relevance/desc`. Filtros `from`, `mentions`, `has` (`link`, `poll`,
  `forward`, …), `in`, `before`, `during`, `after`, `on`, `pinned`,
  `authorType`, `fileType`, `linkFrom` (`link_hostname`).
- Sin confirmar: si `link_hostname` encuentra subdominios; el orden de la
  súper y la normal del mismo emoji.

## Interacciones y comandos de aplicación

Decisión del dueño del 2026-10-17: los componentes de bots y los comandos
de aplicación se usan, como en Serein. En RUNE: `rune-discord-session`
(comandos e interacciones) y `rune-state::interactions`.

**`POST /interactions`:** `nonce` es un snowflake hecho con `Date.now()`,
`session_id` el del `READY`, `guild_id` ausente en privados.

- **2, comando:** `{type, application_id, guild_id, channel_id,
  session_id, data, nonce, analytics_location: "slash_ui", section_name,
  source}`; `data = {version, id, guild_id (sólo si el comando es del
  servidor), name (sin traducir), type, options, application_command,
  target_id, attachments}`. Opciones `{type, name, value, focused}`;
  usuarios, roles y canales como id en texto; adjuntos como índice.
- **3, componente:** `{type, nonce, guild_id, channel_id, message_flags,
  message_id, application_id, session_id, data}`; `data =
  {component_type, custom_id}` y, en menús, `type` y `values`. Una
  interacción pendiente por mensaje.
- **4, sugerencias:** como el 2 sin `analytics_location`, con la opción en
  curso `focused: true` y su texto tal cual; sin adjuntos ni opciones
  vacías. Las respuestas se guardan por opción y texto.
- **5, modal:** `data: {id, custom_id, components, attachments}`;
  componentes sin `id`: fila `{type: 1, components}`, etiqueta `{type: 18,
  component}`, texto `{type: 10}`, campo `{type: 4, custom_id, value}`,
  menús 3 y 5–8 `{type, custom_id, values}`, 21 `{value}`, 22
  `{values}`, 23 `{value: bool}`, archivos `{type: 19, custom_id, values}`
  con índices en `data.attachments`.
- **Errores:** un 429 se reintenta tras `retry_after`; 50035 toma el primer
  `_errors[0]`; versión inválida o 10005 marcan viejo el índice.

**Gateway:** `INTERACTION_CREATE {id, nonce}`, `INTERACTION_SUCCESS`,
`INTERACTION_FAILURE {id, nonce, reason_code}` (2 = «no respondió a
tiempo»), `APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE {choices, nonce}`,
`INTERACTION_MODAL_CREATE {id, channel_id, custom_id, application, title,
components, nonce, resolved}` y `GUILD_APPLICATION_COMMAND_INDEX_UPDATE
{guild_id, version}`.

**Índice:** `GET /guilds/{id}/application-command-index`,
`/channels/{id}/…` (privado con un bot), `/users/@me/…` y
`/applications/{id}/…`; respuesta `{applications, application_commands,
version}`, con `name = name_default ?? name`. Un `202` está en
preparación. Viejo si la versión no coincide o tras cada `CONNECTION_OPEN`.

**Interfaz y plazos:** la parte pulsada carga y las demás se deshabilitan;
«Sending command...», «{app} is thinking...» (marca 128), «The application
did not respond», «This interaction failed». 3 s por HTTP, 15 min para un
diferido, 10 s para un modal (2 min con archivos). Efímeros: marca 64,
«Only you can see this • Dismiss message»; descartar es un borrado local.

**En RUNE:** tipos 2 a 5 (sin `section_name`, `source`, `target_id` ni
adjuntos en comandos), índices de servidor y de privado con bot,
invalidación por `GUILD_APPLICATION_COMMAND_INDEX_UPDATE` y por `READY`,
espera de 30 s como Serein, archivos en modales. El dueño (2026-10-18)
pidió sólo lo que tiene Serein: quedan fuera el índice del usuario, los
reintentos del `202`, el mensaje provisional, `ephemerality_reason` y los
menús contextuales. Sin confirmar: qué llevan `section_name` y `source`.

## Emoji, GIF y estado

| Qué | Cliente oficial | En RUNE |
| --- | --- | --- |
| Emoji en una línea | 1,375 em (`--custom-emoji-size-emoji`), `object-fit: contain` | `TextSpan::image` de limn, 22 px en un mensaje de 16 px |
| Mensaje de sólo emoji | 3 rem, hasta `MAX_EMOJI_TO_BE_JUMBO` = 30, nodos sólo emoji o blanco | 48 px (`text::JUMBO`), hasta 30 |
| Compacto / respuesta citada | tamaño de línea / 1,25 em | igual |
| Tema del canal | reglas del chat sin código, `br`, citas ni subtexto; `customEmoji` queda | el tema con sus emoji; el nombre del canal, texto plano |
| Tamaño pedido | `getEmojiURL` con `{reaction: 40, default: 44, jumbo: 96}` × escala | 48 en línea, 96 en jumbo (normalizado a 128 px) |
| `™ © ®` | nunca imagen de emoji | `twemoji::emoji_len` no los cuenta |
| Emoji no usable | sin `USE_EXTERNAL_EMOJIS`: `DISALLOW_EXTERNAL` antes que el plan y que lo animado | `Store::emoji_perk` y `sticker_perk` miran primero `external_*` |
| Estado en listas | emoji 14 px (16 junto a texto mediano), 4 px, `ellipsis` | `identity::status_line` |
| Búsqueda de GIF | `GIFS_SEARCH` con `q`, `media_format`, `locale`, `limit`, sin `offset`; al final pide sugerencias | «Más GIF» manda `offset` (pedido del dueño); sin confirmar que Discord lo respete |
| Ícono de una aplicación | `GET /applications/{id}/public`; una falla no se repite en la sesión; `app-icons/{id}/{icon}.png?size=…` | `app_icons`: una lectura a la vez, caché por sesión, `size=128` (Serein usa `/rpc`) |

## Perfil

**Popout** (pedido del dueño: «hacele REA a esa card»). Uso en RUNE:
[PROFILES.md](PROFILES.md).

- **Lugar:** 300 px de ancho. Lista de miembros: a la izquierda con 16 px;
  autor de un mensaje y persona en voz: a la derecha con 8; ventanas
  angostas, centrado. Alineado arriba con el ancla; sólo se da vuelta en
  horizontal; en vertical se acota a 12 px del borde. Se cierra si se
  desplaza un ancestro del ancla. Alto máximo `100vh − 20px` con cuerpo
  desplazable. A los 250 ms sin respuesta, esqueleto.
- **Orden:** banner, avatar, burbuja de estado, botones; aviso si falló
  parte del perfil; nombre (20 px), nota (ícono), BOT, usuario, pronombres,
  etiqueta de servidor, insignias; amigos y servidores en común; biografía;
  «Actividad actual»; roles; compositor «Message @nombre». No hay «Member
  since».
- **Medidas:** banner 300×105 (sin imagen, el acento o el color dominante
  del avatar); avatar 80 px en (16, 61), recortado del banner con un círculo
  de radio 46; estado 16; decoración 1,2× el avatar. Tema: degradé del
  primario al secundario con una capa al 60 %.
- **Efectos:** producto `GET /collectibles-products/{sku}`, con
  `reducedMotionSrc`, `staticFrameSrc`, `animationType` y `effects[]`
  (`src`, `loop`, `start`, `duration`, `loopDelay`, `position`, `zIndex`,
  `randomizedSources`), sin tamaño. Nada se ve hasta que cargó todo; reloj
  a 24 cps desde −500 ms; cada capa al 100 % del ancho de la tarjeta, sobre
  banner y avatar, sin mezcla. Movimiento reducido: `reducedMotionSrc`.
  Bajo el puntero baja a 35 % en 0,6 s (`Element::dim_on_hover` de limn).
- **Actividad:** `mp:` contra el media proxy, `spotify:` →
  `i.scdn.co/image/{id}`, `twitch:` →
  `static-cdn.jtvnw.net/previews-ttv/live_user_{login}-{ancho}x{alto}.jpg`,
  `youtube:` → `i.ytimg.com/vi/{id}/hqdefault_live.jpg`, clave simple →
  `app-assets/{app}/{clave}.png?size=160`. Sin imagen grande, el ícono de la
  aplicación o la chica como grande. Grande 60 px, chica 24 recortada de la
  grande; tiempo `m:ss` cada segundo; barra de 2 px con inicio y fin. RUNE
  pide 160×160.
- **Biografía:** reglas del chat menos código, `br`, menciones y canales;
  tres líneas y «View Full Bio». Estado personalizado y pronombres, texto
  plano.
- **Nota:** `GET /users/@me/notes/{id}`; cualquier error es nota vacía.
- **Perfil:** `GET /users/{id}/profile` con `type=popout`,
  `with_mutual_guilds`, `with_mutual_friends`, `with_mutual_friends_count`,
  `guild_id`, `connections_role_id`; lo guardado se ve al instante y se
  vuelve a pedir pasados 60 s; tras 404 o 429, 60 s sin reintentar.
- **Gente en voz:** un `VOICE_STATE_UPDATE` con `member` guarda miembro y
  usuario antes del estado; el popout suscribe al miembro (op 37). Desde
  voz, el perfil se pide con el servidor del canal.
- **Etiqueta de servidor:** con `identity_enabled`, etiqueta e
  `identity_guild_id`, salvo cuarentena; insignia
  `clan-badges/{servidor}/{insignia}.png?size=16` a 14 px.
- **Nameplates y decoraciones por sku:**
  `cdn.discordapp.com/media/v1/collectibles-shop/{sku}/{static|animated|video}`;
  en Linux (sin video) `animated`. La decoración cae a
  `avatar-decoration-presets/{asset}`. Animan al pasar el puntero sobre la
  fila de miembros, privados, el mensaje o el mosaico de llamada, nunca con
  movimiento reducido. Sin confirmar: el formato de `animated`.

**Plan y edición:** `TIER_0` (Basic, `premium_type` 3) sin avatar animado,
banner ni perfil por servidor; `TIER_1` (Classic, 1) con avatar animado;
`TIER_2` (Nitro, 2) con todo. Un GIF de avatar o un banner sin el plan abre
la oferta; RUNE no lo manda (usa el primer cuadro o apaga «Cambiar»). El
selector acepta hasta 8 MiB (`ProfileImage::MAX_ANIMATED_BYTES`). Sin
confirmar: cómo recorta un GIF el oficial y el tope del `PATCH`.

## Interfaz

| Qué | Cliente oficial | En RUNE |
| --- | --- | --- |
| Fila de miembro | 42 px con fondo de 40, radio 8, avatar 32 | `members::ROW_MARGIN` y `RADIUS` 8; la nameplate cubre el fondo |
| Miembro sin cargar | círculo 32 y barra de 14, ancho al azar, pulso de 3 s | ancho determinista, pulso de limn |
| Avatar por defecto | discriminador > 0 → `% 5`; si no, `(id >> 22) % 6`; webhook → 0 | mismo índice, pedido a `embed/avatars/{i}.png` (el dueño: nunca empaquetado) |
| Esqueleto del historial | grupos de 1–4, avatar 40, líneas de 16 en 22, anclado abajo, sin animación | `message::history_skeleton` con semilla del canal |
| Fijados cargando | spinner | spinner de limn |
| Pantalla de carga | logo y consejo hasta READY; a los 10 s, problemas; fundido de 200 ms | `rune_ui::splash`: etapa, motivo y botones a los 10 s; «Reconectando» sólo con sesión cargada |
| Menú de una persona en voz | casillas y flecha de submenú al borde derecho; «Roles ›» con el punto del color del rol, el nombre y una casilla, sin cerrar al marcar; `color:"danger"` en lo que castiga (`remove-friend` confirmado en `web.4d4b2f75fdea5470.js`, byte 1237761; el resto, de la captura del dueño: el menú de usuario vive en chunks perezosos no bajados) | `voice::MenuEntry::{Role, Danger}`; rojo en Aislar, Expulsar, Vetar, Desconectar, Bloquear y Eliminar amigo; «Quitar el aislamiento» y «Desbloquear» sin rojo |

**Tooltip del servidor y altavoz del riel** (`rune_ui::rail`): nombre (2
líneas); escenario (hablantes, máx. 3, con cuántos escuchan); voz (canales
de voz no silenciados salvo el AFK, sin bloqueados ni quien transmite o usa
una app, máx. 6); transmisión (máx. 6); apps en voz (máx. 6); «Silenciado»
al pie. Caras de 24 superpuestas −6, «+N» pasado el máximo; abre sin demora
con título de 16 px seminegrita. Canales en el orden de su lista. El
altavoz no se muestra en un servidor silenciado salvo con la llamada
propia. RUNE no ordena por afinidad y pone el conteo de oyentes en una fila
aparte (a limn le falta un final en `Tooltip::faces`).

**Apps en voz:** `activity_instances` en `GUILD_CREATE` y en el
READY_SUPPLEMENTAL; `EMBEDDED_ACTIVITY_UPDATE_V2 {application_id,
launch_id, composite_instance_id, location {id, kind, channel_id,
guild_id}, participants}`; sin participantes se borra. RUNE:
`rune-discord-session::activities`, `Store::voice_activities`.

## Servidores

### Reglas del servidor y onboarding

En RUNE: `rune-discord-session::onboarding`.

- Rutas: `/guilds/{g}/member-verification`, `/guilds/{g}/requests/@me`,
  `/guilds/{g}/onboarding`, `/guilds/{g}/onboarding-responses`.
- Leer las reglas: `GET` con `with_guild` (sólo si no es miembro) e
  `invite_code`; guarda `version`, `description` y `form_fields`. RUNE pide
  `with_guild=false` (la invitación ya se aceptó).
- Aceptarlas: `PUT …/requests/@me` con `{version, form_fields}` (cada campo
  tal como vino, con su `response`); la respuesta trae
  `application_status`. RUNE devuelve la copia guardada
  (`RulesField::echo`) y lee el estado como Serein.
- Preguntas al entrar: `POST …/onboarding-responses` con
  `onboarding_responses`, `onboarding_prompts_seen` y
  `onboarding_responses_seen` (con `Date.now()`); para cambiarlas después,
  `PUT`. RUNE sólo hace el `POST`.
- Hay onboarding con `GUILD_ONBOARDING` en las features y sin
  `COMPLETED_ONBOARDING` en el miembro.
- Pregunta guardada: `{id, options: [{id, channel_ids, role_ids, emoji…,
  title, description}], title, single_select, disabled, required,
  in_onboarding, type}`; una nueva lleva `id: String(Date.now())` y
  `type: 0` (`Onboarding::fresh_id`). `below_requirements` lo evalúa
  Discord.
- Sin confirmar: el `PUT /guilds/{g}/onboarding` de ajustes (RUNE usa la
  forma documentada), el cuerpo de un `TEXT_INPUT` o `MULTIPLE_CHOICE`
  respondido y los límites numéricos (RUNE usa los de Serein).

### Eventos programados

Serein no los tiene. En RUNE: `rune-discord-session::events`,
`rune_model::scheduled_events`.

- Rutas: `/guilds/{g}/scheduled-events`, `…/{e}`, `…/{e}[/{excepción}]/users`,
  `…/{e}/users/counts`, `…/{e}[/{excepción}]/users/@me`,
  `/users/@me/scheduled-events`.
- Crear: `POST` con `{name, description, image, privacy_level,
  scheduled_start_time, scheduled_end_time, entity_type, channel_id,
  entity_metadata, recurrence_rule}`; editar: `PATCH` (imagen sólo si es un
  `data:` nuevo, canal sólo para escenario y voz, metadatos sólo para otro
  lugar); iniciar, terminar y cancelar: `PATCH {status: 2|3|4}`; borrar:
  `DELETE`.
- Interés: `PUT …/users/@me {response}` (`INTERESTED` 1) y `DELETE`. Los de
  la cuenta: `GET /users/@me/scheduled-events?guild_ids=`; quiénes: `GET
  …/users` con `limit` y `with_member=true` (RUNE acepta `body.users` y el
  arreglo documentado).
- Valores: estado 1–4 (`SCHEDULED`, `ACTIVE`, `COMPLETED`, `CANCELED`);
  lugar `STAGE_INSTANCE` 1, `VOICE` 2, `EXTERNAL` 3; privacidad
  `GUILD_ONLY` 2.
- Gateway: `guild_scheduled_events` en `GUILD_CREATE`;
  `GUILD_SCHEDULED_EVENT_CREATE/UPDATE/DELETE/USER_ADD/USER_REMOVE`.
- Invitaciones con `?event={id}`: la consulta pide
  `guild_scheduled_event_id` y la aceptación lo manda en el contexto.
- RUNE no lee los eventos del `GUILD_CREATE` (pide la lista al mostrar el
  servidor), lee `user_count` con `with_user_count=true`, y no hace
  repeticiones, excepciones ni portada.

### AutoMod

En RUNE: `rune-discord-session::automod`, `rune_model::automod`.

- Rutas: `/guilds/{g}/auto-moderation/rules` y `…/rules/{id}` (`GET`,
  `POST` sin `id`, `PATCH`, `DELETE`); existen `…/validate`,
  `clear-mention-raid`, `alert-action` y `false-alarm`, que RUNE no usa
  (valida con `AutoModRule::problem`). RUNE manda el `PATCH` sin
  `trigger_type` y activa o desactiva con `{enabled}`; sin `guild_id`,
  `creator_id` ni `position`.
- Tipos: acciones 1 bloquear, 2 alerta a un canal, 3 aislar, 4 cuarentena;
  eventos 1 `MESSAGE_SEND`, 2 `GUILD_MEMBER_JOIN_OR_UPDATE`; disparadores 1
  `KEYWORD`, 2 `SPAM_LINK`, 3 `ML_SPAM`, 4 `DEFAULT_KEYWORD_LIST`, 5
  `MENTION_SPAM`, 6 `USER_PROFILE`, 7 `SERVER_POLICY`, 8 `APPLICATION`;
  listas 1–3. `AutoModTrigger` modela 1, 3, 4, 5 y 6; el resto es visible
  pero no editable. Los límites documentados coinciden con las constantes
  del bundle (1000, 60, 10, 260, 100, 50, 20).
- El oficial agrega a exentos el canal de una alerta; RUNE deja que lo
  elija la persona.
- Envío rechazado: 200000, 200001 y 200002 → `MESSAGE_SEND_FAILED_AUTOMOD`
  con `{code, message}`; se muestra el `message` o un texto propio. RUNE:
  `rejections::Blocked`, `Event::AutoModBlocked`.
- El bundle no maneja `AUTO_MODERATION_*` del Gateway; RUNE los lee como los
  documenta Discord, sin confirmar que lleguen a una sesión de usuario.

### Canales, hilos y foros

- **Mover canales:** un solo `PATCH /guilds/{g}/channels` con las posiciones
  que cambian, renumeradas 0…n−1 sobre la lista visible (los canales sin
  `VIEW_CHANNEL` no se mandan). Al pasar a otra categoría pregunta si
  sincroniza permisos sólo con `MANAGE_ROLES` en el canal y en la categoría
  nueva, si no está ya sincronizado y lo estaba con la vieja; confirmar pone
  `lock_permissions: true`. En RUNE: `channel_list::offers_sync`,
  `rest::layout_moves`; Esc o «Cancelar» no mueven.
- **Silenciar hilos:** `PATCH /channels/{id}/thread-members/@me/settings`
  con `{muted: true, mute_config: {selected_time_window, end_time}}`
  (segundos, `-1` para siempre) o `{muted: false}`; sólo para quien sigue
  el hilo. `muted` y `muteConfig` llegan en `READY`, `GUILD_CREATE`,
  `THREAD_CREATE`, `THREAD_LIST_SYNC`, `THREAD_MEMBER(S)_UPDATE`. En RUNE:
  `Command::MuteThread`, `Thread::muted_until`; un hilo silenciado no avisa
  ni con menciones (desvío conocido, igual que los canales).
- **Tarjetas de foro:** la reacción más dada (`count + burst_count`); sin
  ninguna, la predeterminada del foro sin número. El primer mensaje sale de
  `first_messages`; el oficial pide los que faltan con `POST
  /channels/{foro}/post-data {thread_ids}` de a 10, que RUNE no usa.
- **Hora de inicio de la voz:** op 43 `REQUEST_CHANNEL_INFO {guild_id,
  fields: ["status", "voice_start_time"]}`, una vez por servidor y
  conexión; `CHANNEL_INFO` (`channels[].{id, status, voice_start_time}`) y
  `VOICE_CHANNEL_START_TIME_UPDATE`; se vacía en `CONNECTION_OPEN` y
  `RESUMED`; segundos × 1000. En RUNE: `voice::start_times`,
  `Store::voice_started`, sólo `voice_start_time`.

## Llamadas privadas

En RUNE: `voice/private.rs`, `http/calls.rs`.

- **Rutas:** `/channels/{id}/call` (GET), `…/call/ring` y
  `…/call/stop-ringing`. Tipos que se llaman: privados (1) y grupos (3).
- **Llamar en un privado 1:1:** `GET /call`; con respuesta se une y hace
  sonar sólo si se pidió **y** `body.ringable`; con error no se une y ofrece
  una solicitud de amistad. En un grupo, unirse y sonar sin el GET. RUNE
  (`Command::CheckCall`) muestra ese diálogo sólo si Discord rechazó el GET
  (400, 403 o 404) y no son amigos.
- **Sonar:** con llamada existente `POST …/call/ring {recipients}`; si no,
  espera al `CALL_CREATE` y manda `recipients: null`. Dejar de sonar o
  rechazar: `POST …/call/stop-ringing {recipients}`.
- **Gateway:** `CALL_CREATE {channel_id, message_id, region,
  ongoing_rings, voice_states}`, `CALL_UPDATE`, `CALL_DELETE {channel_id,
  unavailable}`. `ongoing_rings`: clave = a quién suena, valor = quién
  llamó (nulo no suena). Entrante: el id propio entre las claves; se quita
  al entrar a esa voz, con `CALL_DELETE` o si se borra el canal; nada en No
  molestar. El aviso no se muestra con ese privado abierto. `CALL_DELETE` de
  la propia sale de la voz.
- **Op 13** `{channel_id}` antes de unirse a un privado desconocido y al
  reconectar con el canal abierto. Op 4 con `guild_id` nulo;
  `VOICE_SERVER_UPDATE` con guild nulo; `server_id` de la conexión de voz:
  el del stream, si no el guild, si no el canal.
- `ring_out` y `ring` suenan con los efectos públicos de Discord
  ([MEDIA.md](MEDIA.md#sonidos)).
- Sin confirmar: cuánto tarda el servidor en dejar de sonar, qué hace el
  rechazo oficial (RUNE sigue a Serein), la llamada en otro dispositivo.

## Voz y video

Decisión del dueño (2026-10-16): **sin tope por Nitro**; RUNE ofrece hasta
1080p60 como Serein, sin «fuente» ni 1440p. Diseño en [VIDEO.md](VIDEO.md).

### Gateway principal

- Opcodes `STREAM_CREATE` 18 `{type: "guild"|"call", guild_id, channel_id,
  preferred_region}`, `STREAM_DELETE` 19, `STREAM_WATCH` 20, `STREAM_PING`
  21 (los tres `{stream_key}`), `STREAM_SET_PAUSED` 22 `{stream_key,
  paused}`.
- Clave: `guild:{servidor}:{canal}:{dueño}` o `call:{canal}:{dueño}`.
- Despachos: `STREAM_CREATE {stream_key, region, viewer_ids,
  rtc_server_id, rtc_channel_id, paused}`, `STREAM_SERVER_UPDATE
  {stream_key, endpoint, token}`, `STREAM_UPDATE`, `STREAM_DELETE
  {stream_key, unavailable, reason}` (`user_requested`, `stream_full`,
  `unauthorized`, `safety_guild_rate_limited`; con `unavailable` pasa a
  «reconectando»).
- Mirar: antes de op 20 manda op 19 por cada stream ajeno que mira, salvo
  «varios a la vez» (con dos o más streams de otros). Si la conexión del
  stream se cae y reintenta, op 21.
- La conexión del stream es aparte: `session_id` del Gateway principal,
  `server_id = rtc_server_id`, `channel_id = rtc_channel_id`.
- Op 4: `guild_id, channel_id, self_mute, self_deaf, self_video, flags`;
  **sin `self_stream`** (lo pone el servidor).
- Compartir exige `STREAM` y que no sea el canal AFK.
- Vista previa: `GET /streams/{clave}/preview?version=<ms>` → `{url}` (429 con
  `retry_after`); quien transmite sube `POST /streams/{clave}/preview
  {thumbnail}` (JPEG en data URL que cabe en 512×288, tras descartar hasta
  60 cuadros negros), cada 60 o 300 s. Del bundle (`shouldFetchPreview`):
  una imagen vale 120 s; sin ella se espera 10 s por cada fallo seguido, o
  el `retry_after` de un 429; un cambio de voz que deja de transmitir
  borra todo. La subida arranca 500 ms después del primer video propio y
  sigue cada 300 s (60 s en pausa o tras un fallo); la frena
  `STREAM_DELETE`. Calidad: la del `toDataURL("image/jpeg")` del canvas.

### Gateway de voz

- Versión `?v=9` (o 10 con un experimento); RUNE y Serein usan 8.
- Op 0: `server_id, channel_id, user_id, session_id, token,
  max_dave_protocol_version, video, streams`, con `streams = [{type, rid:
  "100", quality: 100}]` (`"video"` o `"screen"` en la de stream) y, con
  simulcast, `{rid: "50", quality: 50}`.
- Un stream: `type`, `rid`, `ssrc`, `rtx_ssrc`, `active`, `quality`,
  `max_bitrate`, `max_framerate`, `max_resolution {type, width, height}`.
- Op 2 trae además `streams` (con SSRC asignados) y `experiments`. Op 1:
  `{protocol, data: {address, port, mode}, codecs, rtc_connection_id,
  experiments}`, códecs `{name, type, priority, payload_type,
  rtx_payload_type, encode, decode}`.
- Códecs del escritorio: Opus 120; video `[AV1?, H265] + [H264, VP8]`
  filtrado por el motor; el de índice *t* con `payload_type = 101 + 2t` y
  RTX `102 + 2t`. Con H264 primero (RUNE y Serein), 101 y 102.
- Op 4 trae además `audio_codec`, `video_codec`, `media_session_id`,
  `keyframe_interval`. Op 5 `{speaking, delay, ssrc}` con VOICE 1,
  SOUNDSHARE 2 (audio del stream), PRIORITY 4.
- Op 12 `{audio_ssrc, video_ssrc, rtx_ssrc, streams}`, sólo con SSRC de
  video ≠ 0; `rtx_ssrc = video_ssrc + 1` si no viene. Op 14 cambia códecs.
  Op 15 Media Sink Wants `{any: 0–100, "<ssrc>": 0–100}`, arranca en
  `{any: 100}`.
- Otros: 16 versiones, 18 `flags`, 20 `platform`, 33 actualización de SFU;
  cierres propios 4800–4804.

### Códecs, RTP y cifrado

- H264: `packetization-mode=1`; decodifica `42e034`, el escritorio codifica
  `4d0033` (Main 5.1).
- Realimentación: `ccm fir`, `nack`, `nack pli`, `goog-remb`,
  `transport-cc`; RTX con `apt=`. El nativo es libwebrtc: H264 como RFC 6184
  (simple, STAP-A, FU-A), extensiones `abs-send-time`,
  `transport-wide-cc-02`, `playout-delay`, `video-content-type`,
  `video-timing`, `abs-capture-time`, `generic-frame-descriptor-00` y la de
  hablante propia de Discord; trae `sps_vui_rewriter.cc`.
- Transporte: `aead_aes256_gcm_rtpsize` y `aead_xchacha20_poly1305_rtpsize`.
- DAVE para video: rangos sin cifrar por códec. `davey` hace lo mismo
  (`MediaType::VIDEO`, `Codec::H264`): por cada NAL Annex B deja en claro el
  código de inicio, la cabecera y lo necesario hasta el `pps_id`, y enteros
  los NAL que no son corte (SPS, PPS, SEI).
- Calidades de Go Live (módulo 753070): 480/720/1080/1440/fuente a
  5/15/30/60 fps según el nivel del servidor o la suscripción. Tasas: Go Live
  1280×720@30, 0,5–3,5 Mbit/s (hasta 9 por encima de 720p o 30 fps);
  cámara 1280×720@30, 0,15–2,5 Mbit/s, la capa de calidad 50 con un cuarto
  del máximo; piso 150 kbit/s.

### Escritorio Linux: codificar por GPU y sonido de la transmisión

De `discord_voice.node` (sólo cadenas): VA-API directo desde Rust
(`cros-codecs`, `cros-libva`, encoders AMD y Vulkan, superficie desde
DMA-BUF) en otro proceso; el sonido de una transmisión graba los sink
inputs de PulseAudio/PipeWire del pid de la fuente y su árbol
(`soundshare/pulseaudio_controller.cpp`, `application.process.id`,
`pipewire.sec.pid`), no todo el audio. Qué hace con una pantalla entera sin
pid queda sin confirmar. Ver [VIDEO.md](VIDEO.md).

### Llamada: bandeja y transmisiones

Chunks diferidos del build de `933ace…` (textos en inglés): la llamada del
canal (`91d6578abe9a9597.js`), su mosaico (`d14b3f4aeb1549a4.js`), la
bandeja (`f104b3543f834457.js`, módulo 105225) y sus CSS (`313097…css`,
`730227…css`, `770583…css`). Uso en RUNE: `rune_components::voice::bar`
y `StreamTile`, `rune_ui::streams`.

- **Clic** en un mosaico: `handleSelectParticipant` mira la transmisión si
  no se mira y, en modo video, alterna la selección: el elegido otra vez
  vuelve a la grilla (el mosaico enfocado usa el mismo `onClick`).
- **Doble clic**: `handleFullscreenParticipant` lo selecciona si hace falta
  (contando los dos clics que lo preceden) y alterna la pantalla completa;
  en pantalla completa con ese elegido, deselecciona y sale: vuelve a la
  grilla.
- **Mirar**: `STREAM_WATCH` selecciona la transmisión salvo que haya dos o
  más de otros en el canal (`allowMultiple`) o `noFocus`.
- **Mosaico al pasar el puntero**: «Options» (⋯, abre el menú contextual)
  y silenciar la transmisión («Mute»/«Unmute»), botones de 32
  `overlay-secondary`; el enfocado tiene zoom.
- **Barra** (`videoControls`): grilla de tres columnas, 16 de relleno a los
  lados y abajo. A la izquierda invitar y el chat; al centro la bandeja; a
  la derecha, con una transmisión ajena elegida, su volumen, «Pop Out» y
  «Full Screen»/«Exit Full Screen». Con una transmisión ajena elegida, el
  botón rojo es «Stop Watching». Ancho ≤ 550 esconde la segunda sección y
  ≤ 650 la invitación.
- **Bandeja**: `wrapper` en fila con 12 entre secciones; `buttonSection`
  con borde de 1, radio 12, 4 de relleno y 8 entre botones; botón central
  radio 8 y 8 de relleno con ícono de 20 (36); flecha de 20 pegada con 1
  px; colgar con borde, radio 12 y 4×12 de relleno alrededor del ícono.

### Sin confirmar

- Que el servidor acepte H264 como único códec de video y cómo elige
  `video_codec` (Serein lo hace y funciona).
- Que la versión 8 del gateway de voz reciba video igual que la 9.
- La forma de RTCP en modo `rtpsize` (sale de Serein y de bibliotecas
  públicas).
- Los ID de las extensiones RTP: el nativo no los muestra como cadenas.
  Serein marca el audio del stream con la extensión de ID 9, valor 4; RUNE
  hace lo mismo.
- Si el servidor hace cumplir la tabla de Go Live o sólo el cliente.
- Que un SPS sin reordenamiento evite el rechazo DAVE en el receptor
  oficial (conclusión de Serein).
- Si OpenH264 muestra bien el perfil Main (o High) de los clientes de
  escritorio.
- Los intervalos del refresco de `media sink wants` (Serein: 5 s, y 1 s
  mientras no llega imagen).

## El binario de RUNE

REA también sirve sobre los ejecutables propios. Lo que quedó de eso: RUNE
es un ELF nativo que pinta con Vulkan/EGL a través de wgpu, sin WebKit,
Chromium, Electron, CEF, GTK ni Qt cargados (el login y el hCaptcha usan una
WebView aparte, en otro proceso). `shell.nix` graba un `RUNPATH` a las
bibliotecas de Nix para que arranque fuera del entorno de desarrollo, y
limitar el cargador Vulkan a la GPU integrada evita despertar la discreta
(`apps/desktop/src/gpu.rs`; números en [PERFORMANCE.md](PERFORMANCE.md)).
