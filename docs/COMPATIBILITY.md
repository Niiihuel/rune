# RUNE: compatibilidad con Discord por mecanismos autorizados

> **Última verificación: 2026-10-14.** Todo lo que sigue se contrastó ese día contra la documentación oficial.
> Las URLs `discord.com/developers/docs/*` hoy redirigen (301) a `docs.discord.com/developers/*`, y las
> políticas redirigen a `support-dev.discord.com`. Se citan los destinos finales.
> Lo que no se pudo confirmar en una fuente oficial está marcado **no verificado**.
> **Revisión de continuidad (2026-10-14):** se volvieron a consultar OAuth2,
> Social SDK (scopes, plataformas, presencia, DMs, canales enlazados y voz),
> Voice Gateway y su tabla de opcodes. Es una verificación documental;
> no se autenticó ninguna cuenta ni se probó una integración con Discord.

## Cambio de alcance pedido por el dueño

Después de esta investigación, el dueño autorizó expresamente una variante
**no oficial** de sesión personal y una ventana temporal de login. Se está
implementando separada del proveedor OAuth2; no cambia la matriz de
capacidades autorizadas de Discord. No se incluye falsificación de cliente,
evasión de controles ni copia de código de Serein. El dueño confirmó el
login y la identidad de la primera entrega. La siguiente añade navegación
y chat mediante Gateway/REST, con pruebas locales y validación real
pendiente. Ver [DISCORD_SESSION.md](DISCORD_SESSION.md).

La aceptación experimental de invitaciones quedó en pausa ese mismo día; el
2026-10-16 el dueño la reactivó con el hCaptcha resuelto por la persona
([INVITATIONS.md](INVITATIONS.md)); sigue sin probarse con una cuenta real.
La comparación con Serein no acredita compatibilidad, aprobación del
servicio ni ausencia de restricciones.

## Resumen

- Discord **no ofrece ningún mecanismo oficial para que un cliente de terceros opere una cuenta personal completa**. Automatizar cuentas de usuario fuera de la API OAuth2/bot ("self-bots") está prohibido y puede terminar en el cierre de la cuenta ([soporte](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)). La Developer Policy prohíbe pedir u obtener tokens de inicio de sesión de usuarios ([política, regla 4](https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy)).
- **OAuth2 (Bearer)** concede operaciones según scopes, incluidas algunas escrituras como `role_connections.write`; no es una API general de la cuenta. Identidad y listado de servidores no habilitan lectura ni envío de sus mensajes. Los scopes especializados y el Social SDK se distinguen abajo ([OAuth2](https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes)).
- **Una cuenta bot** accede a muchas rutas REST y al Gateway, con sus permisos e intents. Actúa con identidad de bot. El Social SDK también admite chat en canales enlazados bajo condiciones, por lo que sería incorrecto llamar al bot la única vía oficial de mensajes en servidores ([OAuth2, Bot Users](https://docs.discord.com/developers/topics/oauth2#bot-users), [linked channels](https://docs.discord.com/developers/discord-social-sdk/development-guides/linked-channels)).
- **El Discord Social SDK** expone amigos, presencia, DMs y voz en lobbies. Está orientado a juegos: las comunicaciones admiten desarrollo con límites por aplicación; ampliarlos para producción requiere revisión. En Linux es **Experimental**. Sus términos §2.b(vii) restringen crear productos que compitan con Discord; RUNE no incorpora el SDK ([plataformas](https://docs.discord.com/developers/discord-social-sdk/core-concepts/platform-compatibility), [comunicación](https://docs.discord.com/developers/discord-social-sdk/core-concepts/communication-features)).
- **Decisión vigente:** añadir un proveedor OAuth2 experimental de identidad y servidores (`rune-discord`), conservando la simulación funcional. Configuración y evidencia de pruebas en [DISCORD_OAUTH.md](DISCORD_OAUTH.md). La propuesta de `discord-bot` quedó descartada como dirección de producto; ver [Revisión](#revisión-de-la-recomendación-segunda-iteración).
- **Cómo encajan los mecanismos oficiales en RUNE:** ver [Mapeo a las abstracciones de RUNE](#mapeo-a-las-abstracciones-de-rune).

### Leyenda de estados

| Estado | Significado en este documento |
|---|---|
| Disponible oficialmente | Documentado y usable por cualquier desarrollador sin revisión (solo con los permisos normales de canal o servidor). |
| Disponible con permisos específicos | Documentado, pero exige un *privileged intent*, un toggle en el Developer Portal o un scope concreto. Es autoservicio por debajo de cierto umbral y luego requiere revisión. |
| Restringida | Existe, pero solo para socios aprobados, con solicitud o aprobación de Discord (p. ej. acceso de comunicaciones del Social SDK), o con límites que la hacen inviable para un cliente general. |
| No disponible públicamente | No hay ningún mecanismo público documentado para aplicaciones de terceros. |
| Experimental | Documentado, pero marcado por Discord como experimental o de prueba en el contexto relevante para RUNE. |
| No implementada | Reservado para registrar el estado del código de RUNE. No se usa aquí porque esta matriz describe la disponibilidad de la plataforma. |

## Fuentes

Documentación oficial (todas consultadas el 2026-10-14):

| Tema | URL |
|---|---|
| OAuth2 (scopes, flujos, bot users) | https://docs.discord.com/developers/topics/oauth2 |
| Referencia API (auth, versiones, User-Agent, subida de archivos) | https://docs.discord.com/developers/reference |
| User resource | https://docs.discord.com/developers/resources/user |
| Message / Channel / Guild / Emoji resources | https://docs.discord.com/developers/resources/message · https://docs.discord.com/developers/resources/channel · https://docs.discord.com/developers/resources/guild · https://docs.discord.com/developers/resources/emoji |
| Permisos / Hilos | https://docs.discord.com/developers/topics/permissions · https://docs.discord.com/developers/topics/threads |
| Gateway | https://docs.discord.com/developers/events/gateway |
| Gateway events | https://docs.discord.com/developers/events/gateway-events |
| Revisión de privileged intents | https://docs.discord.com/developers/gateway/getting-started-with-privileged-intent-review |
| Rate limits | https://docs.discord.com/developers/topics/rate-limits |
| Voz (incluye DAVE) | https://docs.discord.com/developers/topics/voice-connections |
| Opcodes de voz y códigos de cierre | https://docs.discord.com/developers/topics/opcodes-and-status-codes#voice |
| RPC local | https://docs.discord.com/developers/topics/rpc |
| Change log | https://docs.discord.com/developers/change-log |
| Social SDK (overview, conceptos, guías) | https://docs.discord.com/developers/discord-social-sdk/overview |
| Social SDK: plataformas | https://docs.discord.com/developers/discord-social-sdk/core-concepts/platform-compatibility |
| Social SDK: scopes OAuth2 | https://docs.discord.com/developers/discord-social-sdk/core-concepts/oauth2-scopes |
| Social SDK: comunicación | https://docs.discord.com/developers/discord-social-sdk/core-concepts/communication-features |
| Social SDK: C++ | https://docs.discord.com/developers/discord-social-sdk/getting-started/using-c++ |
| Social SDK: account linking (desktop) | https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-with-discord |
| Social SDK: account linking (mobile, PKCE) | https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-on-mobile |
| Social SDK: DMs / voz / linked channels / rich presence | https://docs.discord.com/developers/discord-social-sdk/development-guides/sending-direct-messages · https://docs.discord.com/developers/discord-social-sdk/development-guides/managing-voice-chat · https://docs.discord.com/developers/discord-social-sdk/development-guides/linked-channels · https://docs.discord.com/developers/discord-social-sdk/development-guides/setting-rich-presence |
| Discord Terms of Service (vigente desde 29-sep-2025) | https://discord.com/terms |
| Developer Terms of Service (vigente desde 8-jul-2024) | https://support-dev.discord.com/hc/en-us/articles/8562894815383-Discord-Developer-Terms-of-Service |
| Developer Policy (vigente desde 8-jul-2024) | https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy |
| Social SDK Terms (vigente desde 17-mar-2025) | https://support-dev.discord.com/hc/en-us/articles/30225844245271-Discord-Social-SDK-Terms |
| Self-bots (soporte) | https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots |
| Verificación de apps | https://support-dev.discord.com/hc/en-us/articles/23926564536471-How-Do-I-Get-My-App-Verified |
| Blog de ingeniería: DAVE (17-sep-2024) | https://discord.com/blog/meet-dave-e2ee-for-audio-video |
| Protocolo DAVE / libdave | https://daveprotocol.com · https://github.com/discord/libdave |

---

## 1. API REST v10: token de bot vs token Bearer (OAuth2)

**Autenticación.** Hay tres formas: el token de bot (`Authorization: Bot <token>`), un token Bearer de OAuth2 (`Authorization: Bearer <token>`) y Basic con `client_id:client_secret`, esta última solo para los endpoints de token OAuth2 ([referencia, Authentication](https://docs.discord.com/developers/reference#authentication)). Las versiones 10 y 9 figuran como "Available" ([API Versioning](https://docs.discord.com/developers/reference#api-versioning)). Todo cliente HTTP debe enviar un User-Agent válido con el formato `DiscordBot ($url, $versionNumber)`, o puede ser bloqueado por Cloudflare ([User Agent](https://docs.discord.com/developers/reference#user-agent)).

**Bot.** "Unlike the normal OAuth2 flow, bot users have full access to most API routes without using bearer tokens, and can connect to the Real Time Gateway" ([OAuth2, Bot Users](https://docs.discord.com/developers/topics/oauth2#bot-users)). Diferencias documentadas: un bot se añade a servidores por OAuth2 y no acepta invitaciones normales, "cannot have friends or be added to or join Group DMs", y tiene rate limits propios ([Bot vs User Accounts](https://docs.discord.com/developers/topics/oauth2#bot-vs-user-accounts)). Sin verificación no puede pasar de 100 servidores ([verificación](https://support-dev.discord.com/hc/en-us/articles/23926564536471-How-Do-I-Get-My-App-Verified)).

**Bearer (usuario).** Solo puede llamar a los endpoints que cubren sus scopes. Los endpoints `@me` documentados son ([User resource](https://docs.discord.com/developers/resources/user)):

| Endpoint | Scope requerido | Fuente |
|---|---|---|
| `GET /users/@me` | `identify` (sin email); `email` añade el email | [Get Current User](https://docs.discord.com/developers/resources/user#get-current-user) |
| `GET /users/@me/guilds` | `guilds` (información básica; devuelve hasta 200, el máximo de servidores de un usuario no-bot) | [Get Current User Guilds](https://docs.discord.com/developers/resources/user#get-current-user-guilds) |
| `GET /users/@me/guilds/{guild.id}/member` | `guilds.members.read` | [Get Current User Guild Member](https://docs.discord.com/developers/resources/user#get-current-user-guild-member) |
| `GET /users/@me/connections` | `connections` | [Get Current User Connections](https://docs.discord.com/developers/resources/user#get-current-user-connections) |
| `GET/PUT/DELETE /users/@me/applications/{app.id}/role-connection` | `role_connections.write` | [Role Connection](https://docs.discord.com/developers/resources/user#get-current-user-application-role-connection) |
| `PUT /guilds/{guild.id}/members/{user.id}` (lo llama **el bot** con el token del usuario) | `guilds.join` (el bot debe estar ya en el servidor) | [OAuth2 scopes](https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes) |
| `POST /users/@me/channels` (Create Group DM, con `access_tokens` de usuarios) | `gdm.join`. Pensado para el GameBridge SDK, ya deprecado; límite de 10 group DMs activos | [Create Group DM](https://docs.discord.com/developers/resources/user#create-group-dm) |

Los scopes de identidad y servidores anteriores **no habilitan** leer ni enviar
mensajes generales de la cuenta. Esta conclusión no se extiende al Social SDK:
sus scopes y métodos documentados sí permiten comunicación acotada (sección 4).
No se verificó una API REST general de mensajes de cuenta personal accesible con
un Bearer de esos scopes básicos.

## 2. OAuth2

**Authorization code grant.** La URL de autorización es `https://discord.com/oauth2/authorize`, la de token `https://discord.com/api/oauth2/token` y la de revocación `https://discord.com/api/oauth2/token/revoke`. Los endpoints de token solo aceptan `application/x-www-form-urlencoded`. En el flujo confidencial descrito por esa página, las llamadas se autentican con HTTP Basic o con `client_id`/`client_secret` en el cuerpo; clientes públicos se tratan abajo. El `redirect_uri` "is whatever URL you registered when creating your application". El token se refresca con `grant_type=refresh_token` y se recomienda usar `state` contra CSRF ([OAuth2](https://docs.discord.com/developers/topics/oauth2#authorization-code-grant)). En el ejemplo oficial, `expires_in` vale 604800 s ([account linking](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-with-discord)).

**Client credentials.** Es "a quick and easy way for bot developers to get their own bearer tokens for testing purposes" y devuelve un token del dueño del bot. Para aplicaciones de equipo se limita a `identify` y `applications.commands.update` ([OAuth2](https://docs.discord.com/developers/topics/oauth2#client-credentials-grant)).

**PKCE y clientes públicos. Confirmado, con matices:**
- El Developer Portal tiene un toggle **Public Client** en la pestaña OAuth2. Discord advierte que "Public clients cannot securely store client secrets" y que "Most games will not want to ship with Public Client enabled" ([Social SDK OAuth2 scopes](https://docs.discord.com/developers/discord-social-sdk/core-concepts/oauth2-scopes#oauth2-client-types)).
- El endpoint de token acepta `code_verifier`, según los ejemplos oficiales de intercambio ([account linking](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-with-discord), [mobile](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-on-mobile)). PKCE "is **mandatory** for all mobile apps using deep links" ([mobile](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-on-mobile)).
- Los [metadatos oficiales](https://discord.com/.well-known/openid-configuration) declaran PKCE **S256**. La [guía oficial de vinculación web](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-on-web#step-4-exchange-the-code-for-tokens) indica que clientes públicos nativos usan PKCE en lugar de un secreto. La referencia `Client::GetToken` del SDK ofrece ese canje sin secreto.
- **Interoperabilidad pendiente:** la guía OAuth2 general describe clientes confidenciales; no se verificó todavía el canje HTTP público directo de RUNE fuera del SDK. Se implementa como experimento con `code_challenge`, `code_challenge_method=S256` y `code_verifier`, sin alternativas que rebajen la autenticación. Los metadatos no declaran `token_endpoint_auth_methods_supported`.

**Redirect URI en escritorio.** Discord documenta `http://127.0.0.1/callback` para escritorio y `discord-APP_ID:/authorize/callback` para móvil ([account linking](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-with-discord)). **No verificado:** si Discord acepta cualquier puerto de loopback (como pide RFC 8252) o exige que el URI coincida exactamente con el registrado, puerto incluido. Hay que probarlo.

**Scopes y aprobación** ([tabla oficial](https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes)). La documentación avisa: "Some scopes require approval from Discord to use. Requesting them from a user without approval from Discord may cause errors or undocumented behavior in the OAuth2 flow."

| Scope | Estado según Discord |
|---|---|
| `identify`, `email`, `guilds`, `guilds.members.read`, `connections`, `guilds.join`, `gdm.join`, `role_connections.write`, `applications.commands`, `bot`, `webhook.incoming`, `applications.builds.read`, `applications.entitlements`, `applications.store.update` | Sin nota de aprobación |
| `messages.read` | "for local rpc server api access" (solo vía RPC local; el sistema de tokens RPC además "disallows use of the `messages.read` scope", según [RPC](https://docs.discord.com/developers/topics/rpc)) |
| `rpc`, `rpc.activities.write`, `rpc.notifications.read`, `rpc.voice.read`, `rpc.voice.write` | "only available to approved partners" |
| `dm_channels.read` | "only available to approved partners" |
| `voice` | "only available to approved partners" |
| `identify.premium`, `applications.builds.upload` | "only available to approved partners" |
| `relationships.read` | Parte del Social SDK: "submit for access"; aplican los Social SDK Terms |
| `activities.read`, `activities.write` | "not currently available for apps" |
| `openid`, `sdk.social_layer_presence`, `sdk.social_layer` | Scopes del Social SDK. `sdk.social_layer` habilita funciones de comunicación de "limited access" ([Social SDK scopes](https://docs.discord.com/developers/discord-social-sdk/core-concepts/oauth2-scopes)) |

**OAuth2 NO da acceso completo a la cuenta personal.** El acceso depende de
scopes concedidos y de la interfaz usada. El Social SDK también usa OAuth2:
`openid sdk.social_layer_presence` cubre presencia y relaciones;
`openid sdk.social_layer` añade comunicaciones limitadas. No deben confundirse
con autorización para usar cualquier endpoint o el Gateway general
([scopes del SDK](https://docs.discord.com/developers/discord-social-sdk/core-concepts/oauth2-scopes)).

## 3. Gateway

**Flujo** ([Gateway](https://docs.discord.com/developers/events/gateway)):
1. Obtener la URL con `GET /gateway` o `GET /gateway/bot`. Este último "requires authentication using a valid bot token" ([Get Gateway Bot](https://docs.discord.com/developers/events/gateway#get-gateway-bot)).
2. Conectar, p. ej. `wss://gateway.discord.gg/?v=10&encoding=json`.
3. Recibir Hello (op 10) con `heartbeat_interval`. El primer heartbeat se envía tras `heartbeat_interval * jitter` y luego cada `heartbeat_interval`. Si falta el ACK, la conexión está "zombied": hay que cerrarla con un código distinto de 1000/1001 y hacer Resume.
4. Enviar Identify (op 2) con `token`, `properties`, `intents` y, opcionalmente, `compress`, `large_threshold`, `shard` y `presence` ([Identify](https://docs.discord.com/developers/events/gateway-events#identify)).
5. Recibir Ready con `session_id` y `resume_gateway_url`.
6. Para reanudar: abrir una conexión nueva contra `resume_gateway_url` y enviar Resume (op 6) con `session_id` y el último `seq`.

**Límites.** "Clients are limited to 1000 `IDENTIFY` calls to the websocket in a 24-hour period … Upon hitting this limit, all active sessions for the app will be terminated, the bot token will be reset". Cada conexión admite 120 eventos enviados por cada 60 s ([Gateway, Rate Limiting](https://docs.discord.com/developers/events/gateway#rate-limiting)). En REST, 50 req/s globales por bot y 10.000 solicitudes inválidas (401/403/429) cada 10 min antes de un bloqueo temporal por IP ([rate limits](https://docs.discord.com/developers/topics/rate-limits)).

**Compresión y codificación.** `encoding=json|etf`. La compresión de transporte puede ser `compress=zlib-stream` (contexto zlib compartido por conexión, sufijo `00 00 ff ff`) o `zstd-stream`. La compresión de payload (`compress: true` en Identify) existe solo con JSON y es incompatible con la de transporte ([Encoding and Compression](https://docs.discord.com/developers/events/gateway#encoding-and-compression)).

**Intents privilegiados:** `GUILD_PRESENCES`, `GUILD_MEMBERS` y `MESSAGE_CONTENT` ([Privileged Intents](https://docs.discord.com/developers/events/gateway#privileged-intents)).
- **Umbral actual, cambiado el 10-jun-2026.** "Apps with fewer than 10,000 users can access privileged intents by enabling them in the Developer Portal". Por encima de 10.000 usuarios únicos se exige revisión, con 90 días para solicitarla y reaplicación anual. El umbral anterior era de 100 servidores ([guía de revisión](https://docs.discord.com/developers/gateway/getting-started-with-privileged-intent-review), [change log 10-jun-2026](https://docs.discord.com/developers/change-log)). La verificación de la app (necesaria por encima de 100 servidores) ahora es un proceso separado.
- Sin `MESSAGE_CONTENT` llegan vacíos los campos `content`, `embeds`, `attachments`, `components` y `poll`, salvo en mensajes del propio bot, DMs con el bot, menciones al bot y context menus ([Message Content Intent](https://docs.discord.com/developers/events/gateway#message-content-intent)).
- Si se pasa un intent privilegiado no habilitado, el servidor cierra con 4014. Los intents privilegiados también limitan REST: p. ej. List Guild Members requiere `GUILD_MEMBERS`.

**¿Puede un token Bearer conectarse al Gateway? No hay mecanismo documentado.** La documentación contrapone explícitamente el flujo OAuth2 normal a los bots, que "can connect to the Real Time Gateway" ([OAuth2](https://docs.discord.com/developers/topics/oauth2#bot-users), [soporte self-bots](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)). Toda la guía del Gateway habla de "bot client". **No verificado:** una frase literal del tipo "Bearer tokens cannot connect". La conclusión práctica es la misma: no está soportado ni documentado, y RUNE no debe intentarlo.

**Cambio próximo.** A partir del **16-nov-2026**, los canales que el bot no puede ver le llegarán por el Gateway **ofuscados**: solo `id`, `type`, `position` y `parent_id` serán fiables, con el flag `CHANNEL_OBFUSCATED` (`1 << 17`). Por HTTP esos canales se omiten ([Obfuscated Channels](https://docs.discord.com/developers/resources/channel#channel-object-obfuscated-channels)).

## 4. Discord Social SDK

**Qué ofrece** ([core features](https://docs.discord.com/developers/discord-social-sdk/core-concepts/core-features), [comunicación](https://docs.discord.com/developers/discord-social-sdk/core-concepts/communication-features)):
- **Account linking** por OAuth2 y **provisional accounts** para jugadores sin cuenta de Discord.
- **Relaciones**: amigos de Discord, persistentes, y amigos del juego.
- **Presencia y rich presence**, e invitaciones a partidas.
- **DMs 1:1**: `SendUserMessage`, resúmenes con `GetUserMessageSummaries` e historial con `GetUserMessagesWithLimit`: **hasta 200 mensajes y 72 horas**, y ambos participantes deben haber jugado el juego. Esto no ofrece el historial completo de la cuenta. Hay sincronización con Discord cuando al menos uno es usuario completo, sujeta a las condiciones de la guía ([DMs](https://docs.discord.com/developers/discord-social-sdk/development-guides/sending-direct-messages)).
- **Lobbies** con chat de texto y voz.
- **Voz**: "voice calls only work with lobby members". No hay voz en canales de servidor ni en DMs ([voz](https://docs.discord.com/developers/discord-social-sdk/development-guides/managing-voice-chat)).
- **Linked channels**: un lobby enlazado con un **canal de texto** de servidor ([linked channels](https://docs.discord.com/developers/discord-social-sdk/development-guides/linked-channels)).
- **Permisos de linked channels:** enlazar exige permisos de gestionar, ver y escribir en el canal, además de `CanLinkLobby`. Todos los miembros del lobby podrán leer y responder, incluso si no acceden al canal en Discord. Un adaptador no puede tratar esos permisos como equivalentes a los de un canal ordinario ([requisitos](https://docs.discord.com/developers/discord-social-sdk/development-guides/linked-channels#linked-channel-requirements)).
- **Rich presence sin autenticación:** el SDK puede publicarla por RPC con el cliente oficial ejecutándose en escritorio y una aplicación registrada. Account linking no es obligatorio para ese modo; no concede acceso a mensajes ([rich presence](https://docs.discord.com/developers/discord-social-sdk/development-guides/setting-rich-presence#rich-presence-without-authentication)).
- El SDK impone: "you must only do so in response to a user action. You should never automatically send messages."

**Acceso y aprobación:**
- Para habilitar el SDK se rellena un formulario ("share details about your game") en el Developer Portal y se pulsa Submit ([C++](https://docs.discord.com/developers/discord-social-sdk/getting-started/using-c++)).
- Las funciones de **comunicación** (DMs, lobbies, voz, linked channels) requieren el scope `sdk.social_layer`, son de "limited access" y tienen límites de desarrollo **por aplicación**: 100 DMs cada 2 h, 100 mensajes a lobby cada 2 h, etc. ([comunicación](https://docs.discord.com/developers/discord-social-sdk/core-concepts/communication-features)).
- Para subir esos límites hay que solicitar "Comms Access". El conjunto mínimo exigido es "Discord Account Linking, Rich Presence, Game Invites, and Unified Friends List", más material de apoyo (vídeo de la integración). "Discord may approve or deny your access".

**Plataformas** ([compatibilidad](https://docs.discord.com/developers/discord-social-sdk/core-concepts/platform-compatibility)):
- Windows x64/ARM64, macOS x64/ARM64, Android, iOS, Xbox Series y PS5: Generally Available.
- **Linux (glibc 2.31+): "Experimental"**.
- Xbox One y PS4: Experimental.

**Lenguaje y FFI desde Rust.** Hay integraciones para C++ standalone (C++20+), Unity y Unreal. La doc afirma: "The C++ SDK is a header-only wrapper over the C API provided by the shared library" (`libdiscord_partner_sdk.so` en Linux) ([C++](https://docs.discord.com/developers/discord-social-sdk/getting-started/using-c++)). Así que existe una API C usable en principio por FFI desde Rust. **No verificado:** que el archivo de descarga incluya una cabecera C pública y estable, y su ABI. La descarga requiere iniciar sesión en el portal.

**Licencia y distribución** ([Social SDK Terms](https://support-dev.discord.com/hc/en-us/articles/30225844245271-Discord-Social-SDK-Terms)):
- §2.a concede una licencia "limited, non-exclusive, non-transferable … revocable" para usar el SDK y "to distribute the Discord Social SDK as integrated into your Application".
- §2.b(i)-(ii) prohíbe "reverse engineer or otherwise derive source code" y "redistribute" fuera de lo anterior.
- **No verificado:** si incluir el `.so` en un repositorio open source cuenta como redistribución prohibida. Lo prudente es no versionarlo y que cada usuario lo descargue con su cuenta.

**¿Es un cliente de chat de uso general un caso permitido? No verificado; probablemente no encaja.** Los Terms definen las Integrations como "unified friends list, Discord login…, account linking…, rich presence and game detection, cross-platform game invites, and other authorized social integrations". Todo el material (formulario "about your game", requisitos de Comms Access centrados en juegos) da por hecho un juego. No hay ninguna frase que prohíba literalmente apps que no sean juegos, pero tampoco ninguna que las admita. Además, los límites de desarrollo (100 DMs cada 2 h por aplicación) hacen inviable un cliente de chat sin la aprobación de Comms Access.

## 5. Términos y políticas: clientes de terceros, self-bots e ingeniería inversa

**Terms of Service** ([discord.com/terms](https://discord.com/terms), "Effective: September 29, 2025"):
- *"Don't use the services to do harm to Discord"*: "…scraping our services without our written consent, including by using any robot, spider, crawler, scraper, or other automatic device, process, or software; … using any unauthorized software designed to modify the services; … copying, dismantling, or reverse engineering any of our services or using our intellectual property without permission…"
- *"Don't use the services to do harm to yourself or others"*: "…trying to gain access to another user's account or any non-public portions of the services; … spamming, auto-messaging, or auto-dialing people through our services…"
- *Software in Discord's services*: "You may not copy, modify, create derivative works based upon, distribute, sell, lease, or sublicense any of our software or services. You also may not reverse engineer or decompile our software or services, attempt to do so, or assist anyone in doing so, unless you have our written consent or applicable law permits it."

**Self-bots** ([soporte](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)): "Automating normal user accounts (generally called "self-bots") outside of the OAuth2/bot API is forbidden, and can result in an account termination if found." Lo mismo aparece en la doc de OAuth2: "refraining from automating standard user accounts (generally called "self-bots") outside of the OAuth2/bot API" ([Bot vs User Accounts](https://docs.discord.com/developers/topics/oauth2#bot-vs-user-accounts)).

**Developer Policy** ([política](https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy)):
- Regla 4: "Under no circumstances may you or your Application request or attempt to obtain login credentials from Discord users. This includes information such as passwords or account access or login tokens."
- Regla 2: "Do not initiate processes on a user or server's behalf without first obtaining their permission."
- Regla 12: "Do not impersonate other Applications, Discord employees or partners, or Discord services. This includes deception via your and your Application's account and identity."
- Regla 13: "…automating messages to be sent for the purpose of maintaining activity in a Discord server."
- Reglas 20 y 21: "Do not mine or scrape any data…"; "Do not use message content obtained through the APIs to train machine learning or AI models…"

**Developer Terms of Service** ([términos](https://support-dev.discord.com/hc/en-us/articles/8562894815383-Discord-Developer-Terms-of-Service)):
- "You will not … (a) reverse engineer or otherwise derive source code … from the APIs … or (c) access or use the APIs in any way that (i) is not in accordance with the applicable Documentation, (ii) … circumvents any of our technical processes or security measures…"
- Sobre credenciales: "will treat them as Discord confidential information … you will keep API keys and tokens encrypted in any files or other materials accessible by third parties … **developer credentials may not be embedded in open source projects**". La lista de credenciales incluye explícitamente "Application ID, passwords, keys, tokens, and client secrets".
- "Application" incluye "any bot, game, activity, website, or other client". La API Data debe borrarse cuando ya no sea necesaria y hace falta una política de privacidad.

**Conclusión para RUNE:**
- Un cliente que use el token de usuario de una cuenta personal (capturado del cliente oficial o pidiéndolo) infringe la regla 4 de la Developer Policy y el aviso sobre self-bots.
- Reimplementar el protocolo privado del cliente oficial o del Social SDK sería ingeniería inversa prohibida.
- Lo legítimo es solo: cuenta bot, OAuth2 con scopes públicos y Social SDK dentro de sus términos.

## 6. Voz para bots

- **Documentado** ([Voice](https://docs.discord.com/developers/topics/voice-connections)):
  - Voice Gateway WebSocket (versión 8 recomendada; las inferiores a 4 se descontinuaron el 18-nov-2024) más UDP con IP discovery.
  - Audio **Opus** estéreo a 48 kHz sobre RTP.
  - Cifrado de transporte: hay que soportar `aead_xchacha20_poly1305_rtpsize` y preferir `aead_aes256_gcm_rtpsize` cuando esté disponible. Los modos `xsalsa20_*` y `aead_aes256_gcm` sin rtpsize están deprecados o descontinuados.
  - El bot entra al canal con Voice State Update por el Gateway principal y espera *Voice State Update* y *Voice Server Update*.
- **DAVE (cifrado de extremo a extremo):** usa MLS (RFC 9420) y AES128-GCM por frame. "we will **only support E2EE calls starting on March 1st, 2026** for all audio and video conversations in direct messages (DMs), group messages (GDMs), voice channels, and Go Live streams" ([Voice, DAVE](https://docs.discord.com/developers/topics/voice-connections#end-to-end-encryption-dave-protocol)). Según el change log del 2-sep-2025: "After that date, any client or application not updated for DAVE support will no longer be able to participate in Discord calls" ([change log](https://docs.discord.com/developers/change-log)). Por tanto, **DAVE es obligatorio** para un bot de voz hoy. Hay especificación en https://daveprotocol.com y una librería open source en https://github.com/discord/libdave. El blog ([DAVE](https://discord.com/blog/meet-dave-e2ee-for-audio-video)) aclara que los mensajes de texto **no** son E2EE. **No verificado:** que la aplicación efectiva del corte del 1-mar-2026 coincida con lo anunciado (no se probó empíricamente).
- **Video, cámara y Go Live (compartir pantalla) para bots: no documentado.** La tabla de versiones menciona "Added video" (v3) y "video sink wants" (v5), y DAVE cubre "Go Live streams". Pero la guía solo documenta cómo enviar audio Opus. No hay documentación pública de payloads, códecs ni del flujo para que una app envíe o reciba video o streams. Se trata como **no disponible públicamente**.
- **Voz con la cuenta del usuario:** el scope `voice` es "only available to approved partners" ([OAuth2](https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes)). La voz del Social SDK se limita a lobbies del propio juego.

## 7. Enfoque "cliente-bot" (alternativa descartada para RUNE)

Esta sección conserva la evaluación técnica de la primera iteración. No es
una propuesta de implementación ni un flujo de configuración de RUNE.

**Modelo:** el usuario crea una aplicación en el Developer Portal y obtiene el token de su bot. Desmarca "Public Bot" para que solo él pueda añadirlo ("If unchecked, only you can add the bot to guilds", [OAuth2](https://docs.discord.com/developers/topics/oauth2#bot-authorization-flow)). Lo invita con `scope=bot&permissions=…` a servidores donde tenga "proper permissions". **No verificado:** que el permiso exacto requerido sea `MANAGE_GUILD`. RUNE se conecta como ese bot y la UI lo muestra **como bot**.

**Legitimidad:**
- Usa exactamente el mecanismo previsto: "bot users … can connect to the Real Time Gateway" ([OAuth2](https://docs.discord.com/developers/topics/oauth2#bot-users)).
- Las credenciales son del propio usuario y se usan "solely with your Application", que es lo que piden los [Developer Terms](https://support-dev.discord.com/hc/en-us/articles/8562894815383-Discord-Developer-Terms-of-Service).
- Compatible con la Developer Policy siempre que (a) el bot no se haga pasar por la persona (regla 12): nombre y avatar propios, UI etiquetada; (b) solo actúe por acción explícita del usuario y sin mensajes automáticos (reglas 2 y 13, y "auto-messaging" en los [ToS](https://discord.com/terms)); (c) no haga scraping ni archivado masivo (regla 20).
- **No verificado:** ninguna fuente oficial aborda de forma explícita un bot "operado manualmente" por una persona. Los bots se describen como "dedicated to automation", pero no se encontró ninguna prohibición del manejo interactivo.

**Limitaciones documentadas:**
- **Identidad:** todo se publica como el bot. No ve los servidores, DMs, amigos, menciones ni notificaciones del usuario, solo lo que el bot tiene permitido.
- **Servidores:** solo aquellos a los que se añadió el bot. Máximo 100 sin verificación ([verificación](https://support-dev.discord.com/hc/en-us/articles/23926564536471-How-Do-I-Get-My-App-Verified)). No acepta invitaciones normales.
- **Contenido de mensajes:** hace falta `MESSAGE_CONTENT`, autoservicio por debajo de 10.000 usuarios ([guía](https://docs.discord.com/developers/gateway/getting-started-with-privileged-intent-review)). Lo mismo para `GUILD_MEMBERS` (lista de miembros) y `GUILD_PRESENCES` (presencia de otros).
- **DMs:** el bot puede abrir DMs (`POST /users/@me/channels`) y recibir los que le envíen (intents `DIRECT_MESSAGES` y `DIRECT_MESSAGE_TYPING`). Son DMs **del bot**, no del usuario. "Cannot have friends or be added to or join Group DMs" ([Bot vs User](https://docs.discord.com/developers/topics/oauth2#bot-vs-user-accounts)).
- **Indicador de escritura:** "Generally bots should **not** use this route" ([Trigger Typing](https://docs.discord.com/developers/resources/channel#trigger-typing-indicator)).
- **Reconexiones:** como mucho 1000 IDENTIFY cada 24 h; si se superan, **se resetea el token**. RUNE debe priorizar Resume ([Gateway](https://docs.discord.com/developers/events/gateway#identifying)).
- **Canales ofuscados** desde el 16-nov-2026 ([Channel](https://docs.discord.com/developers/resources/channel#channel-object-obfuscated-channels)).
- **Voz:** posible, pero con DAVE obligatorio. Video y pantalla no están documentados.

---

## Matriz de compatibilidad

"Propia" se refiere a la identidad con la que opera RUNE: el bot, o el usuario vía OAuth2 cuando se indica.

| Funcionalidad | Estado | Mecanismo oficial | Fuente (URL) | Notas |
|---|---|---|---|---|
| Identidad del usuario | Disponible oficialmente | OAuth2 scope `identify` (`GET /users/@me`); cuenta bot para la identidad del bot | https://docs.discord.com/developers/resources/user#get-current-user | Solo lectura. `email` es opcional. Permite mostrar "operado por @usuario" junto al bot. |
| Lista de servidores | Disponible oficialmente | OAuth2 scope `guilds` (servidores del usuario, info básica); cuenta bot (servidores del bot) | https://docs.discord.com/developers/resources/user#get-current-user-guilds | Saber en qué servidores está el usuario no da acceso a su contenido. Solo son navegables los servidores donde está el bot. |
| Aceptar una invitación de servidor con la cuenta de RUNE | Experimental, no oficial | No hay aceptación genérica sin bot en el recurso público revisado | https://docs.discord.com/developers/resources/invite | `discord-session` implementa una aceptación explícita tomando el flujo de Serein como referencia; espera pertenencia por Gateway. Pruebas locales, unión real pendiente. [Detalle](INVITATIONS.md). |
| Lista de canales | Disponible oficialmente | Cuenta bot (`GET /guilds/{id}/channels`, `GUILD_CREATE`) | https://docs.discord.com/developers/resources/guild#get-guild-channels | Solo canales visibles para el bot. Ofuscación de los no visibles por Gateway desde el 16-nov-2026 (https://docs.discord.com/developers/resources/channel#channel-object-obfuscated-channels). |
| Leer mensajes de servidor | Disponible con permisos específicos | Cuenta bot + intent privilegiado `MESSAGE_CONTENT` + `VIEW_CHANNEL`/`READ_MESSAGE_HISTORY` | https://docs.discord.com/developers/events/gateway#message-content-intent | Autoservicio por debajo de 10k usuarios; por encima, revisión anual. `messages.read` (OAuth2) es solo para RPC local. |
| Enviar mensajes | Disponible oficialmente | Cuenta bot (`POST /channels/{id}/messages`) | https://docs.discord.com/developers/resources/message#create-message | Se publica como el bot, nunca como el usuario. Solo por acción explícita del usuario. |
| DMs 1:1 | Restringida | Cuenta bot (DMs del bot) / Social SDK (`sdk.social_layer`) / OAuth2 `dm_channels.read` (socios aprobados) | https://docs.discord.com/developers/discord-social-sdk/development-guides/sending-direct-messages | Los DMs personales solo existen vía Social SDK: acceso limitado, 100 DMs cada 2 h por app, Comms Access orientado a juegos. El bot solo tiene sus propios DMs. |
| DMs grupales | No disponible públicamente | Ninguno (`gdm.join` solo crea o une a GDMs de la app; `dm_channels.read` es para socios) | https://docs.discord.com/developers/topics/oauth2#bot-vs-user-accounts | "Bots cannot … be added to or join Group DMs". Create Group DM: GameBridge deprecado, límite de 10. |
| Amigos/relaciones | Restringida | Social SDK / OAuth2 `relationships.read` (requiere solicitud al Social SDK) | https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes | Los bots no pueden tener amigos. Social SDK sujeto a sus Terms y a encaje como "juego" (no verificado). |
| Presencia propia | Disponible oficialmente | Cuenta bot (Gateway op 3 Update Presence) | https://docs.discord.com/developers/events/gateway-events#update-presence | Es la presencia **del bot**. La del usuario no se puede fijar (`activities.write` "not currently available for apps"). |
| Presencia de otros | Disponible con permisos específicos | Cuenta bot + intent privilegiado `GUILD_PRESENCES` | https://docs.discord.com/developers/events/gateway#privileged-intents | Solo miembros de servidores del bot. Presencia de amigos solo vía Social SDK (restringido). |
| Indicador de escritura | Disponible oficialmente | Cuenta bot (intents `GUILD_MESSAGE_TYPING`/`DIRECT_MESSAGE_TYPING`; `POST /channels/{id}/typing`) | https://docs.discord.com/developers/resources/channel#trigger-typing-indicator | Discord desaconseja que los bots lo emitan ("Generally bots should not use this route"). Recibirlo es libre. |
| Reacciones | Disponible oficialmente | Cuenta bot (Create/Delete Reaction, intent `GUILD_MESSAGE_REACTIONS`) | https://docs.discord.com/developers/resources/message#create-reaction | Requiere `ADD_REACTIONS`/`READ_MESSAGE_HISTORY` en el canal. |
| Hilos | Disponible oficialmente | Cuenta bot (endpoints de threads) | https://docs.discord.com/developers/topics/threads | El contenido de los mensajes de hilos también depende de `MESSAGE_CONTENT`. |
| Menciones | Disponible oficialmente | Cuenta bot (formato de mensajes, `allowed_mentions`) | https://docs.discord.com/developers/reference#message-formatting | El bot ve el contenido donde lo mencionan aunque no tenga intent. Las menciones **al usuario** no son visibles como tal. |
| Adjuntos (subir/descargar) | Disponible con permisos específicos | Cuenta bot (`multipart/form-data`, `ATTACH_FILES`) + `MESSAGE_CONTENT` para ver adjuntos ajenos | https://docs.discord.com/developers/reference#uploading-files | Límite por archivo de 20 MiB por defecto. `attachments` está vacío sin `MESSAGE_CONTENT`. |
| Emojis personalizados | Disponible oficialmente | Cuenta bot (Emoji resource, application emojis) | https://docs.discord.com/developers/resources/emoji | Los app emojis no requieren `USE_EXTERNAL_EMOJIS`. |
| Búsqueda de mensajes | Disponible con permisos específicos | Cuenta bot (`GET /guilds/{id}/messages/search`) + `MESSAGE_CONTENT` + `READ_MESSAGE_HISTORY` | https://docs.discord.com/developers/resources/message#search-guild-messages | Documentado el 19-mar-2026. Solo servidores; sin búsqueda en DMs. Puede devolver 202 si no hay índice. |
| Lista de miembros | Disponible con permisos específicos | Cuenta bot + intent privilegiado `GUILD_MEMBERS` | https://docs.discord.com/developers/resources/guild#list-guild-members | Sin el intent: solo búsquedas por prefijo o ID (máx. 100) vía Request Guild Members (https://docs.discord.com/developers/events/gateway-events#request-guild-members). |
| Roles y permisos | Disponible oficialmente | Cuenta bot (Get Guild Roles, cálculo de permisos) | https://docs.discord.com/developers/topics/permissions | Calcular permisos efectivos del bot para mostrar u ocultar acciones. |
| Notificaciones | Restringida | OAuth2 `rpc.notifications.read` (socios aprobados, RPC local) | https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes | Las notificaciones y estados de lectura del usuario no están expuestos. RUNE sí puede generar notificaciones locales a partir de eventos del Gateway del bot. |
| Voz en canal | Disponible oficialmente | Cuenta bot (Voice Gateway v8 + UDP + Opus + DAVE) | https://docs.discord.com/developers/topics/voice-connections | DAVE E2EE obligatorio desde el 1-mar-2026. `CONNECT`/`SPEAK` en el canal. Coste de implementación alto. Scope `voice` de usuario: solo socios. |
| Video/cámara | No disponible públicamente | Ninguno | https://docs.discord.com/developers/topics/voice-connections | La guía solo documenta envío de audio Opus. No hay flujo de video público para apps. |
| Compartir pantalla | No disponible públicamente | Ninguno | https://docs.discord.com/developers/topics/voice-connections | Go Live solo aparece como superficie cubierta por DAVE. No hay API pública para emitirlo. |
| Rich presence | Experimental en Linux | Social SDK con account linking, o mediante RPC sin autenticación al cliente oficial en ejecución | https://docs.discord.com/developers/discord-social-sdk/development-guides/setting-rich-presence | Social SDK en Linux = "Experimental" (https://docs.discord.com/developers/discord-social-sdk/core-concepts/platform-compatibility). Es actividad de la aplicación; no implica control completo del estado de la cuenta. Encaje de una app que no es juego: no verificado. |

---

## Mapeo a las abstracciones de RUNE

Verificado el 2026-10-14. Qué ofrece cada mecanismo oficial para los contratos
que RUNE ya tiene (descritos en [ARCHITECTURE.md](ARCHITECTURE.md#contratos-entre-proveedor-núcleo-y-estado)).
Los contratos de chat siguen implementados contra la simulación. El proveedor
OAuth2 experimental implementa autorización, identidad y servidores básicos;
no habilita los contratos de mensajería ni voz. La tabla sirve para que un proveedor autorizado futuro, de Discord
o de otro servicio, sepa qué traducir; no implica que RUNE vaya a usar cada
mecanismo. La columna OAuth2 siguiente se refiere a los scopes básicos de
identidad/servidores; el Social SDK también se autentica con OAuth2, pero su
superficie está en su propia columna.

| Abstracción de RUNE | Cuenta bot | OAuth2 (Bearer) | Social SDK |
|---|---|---|---|
| `Delivery` + `Capabilities::idempotent_send` (reintentar sin duplicar) | Sí, acotado: Create Message acepta `nonce` y `enforce_nonce`: "If true and nonce is present, it will be checked for uniqueness in the past few minutes. If another message was created by the same author with the same nonce, that message will be returned and no new message will be created" ([Create Message](https://docs.discord.com/developers/resources/message#create-message-json/form-params)). Un proveedor solo debería reutilizar el nonce dentro de esa ventana ("past few minutes", sin cifra exacta); pasada, el reintento es un envío nuevo. | No envía mensajes. | `SendUserMessage`; deduplicación **no verificada**. |
| `Event::Failed { ambiguous }` / `Delivery::Unknown` | Un tiempo de espera de REST sin respuesta es ambiguo; con `enforce_nonce` reintentar es seguro dentro de la ventana. El evento `MESSAGE_CREATE` trae el `nonce` ("Value will appear in the Message Create event"), así que la confirmación por Gateway resuelve la ambigüedad. | — | No verificado. |
| `Presence.status` (`Online`, `Idle`, `DoNotDisturb`, `Invisible`, `Offline`) | Mismos cinco valores en Update Presence: `online`, `idle`, `dnd`, `invisible` ("Invisible and shown as offline"), `offline` ([Update Presence](https://docs.discord.com/developers/events/gateway-events#update-presence)). La presencia propia es la **del bot**; la de otros exige el intent privilegiado `GUILD_PRESENCES`. | No (`activities.write` no disponible). | Presencia de amigos y rich presence (acceso limitado). |
| `Activity { kind, name, detail }` | `ActivityKind::{Playing, Streaming, Listening, Watching, Competing}` corresponde a los tipos 0, 1, 2, 3 y 5 ([Activity Types](https://docs.discord.com/developers/events/gateway-events#activity-object-activity-types)); el tipo 4 (Custom) va a `Presence.custom` (campo `state`: "text used for a custom status"). | No. | Rich presence. |
| `Latest::Presence` (solo el último valor por persona) | Necesario: Presence Update llega por cada cambio de cada miembro visible; un servidor grande produce ráfagas. | — | — |
| `Latest::Speaking` | Voice Gateway opcode 5: la tabla oficial documenta envío desde cliente y servidor para indicar quién habla ([Voice Opcodes](https://docs.discord.com/developers/topics/opcodes-and-status-codes#voice)). La traducción y el manejo de desconexión siguen sin probar en RUNE. | — | Voz solo en lobbies. |
| `Call` + `CallPhase` + `CallQuality` | `Joining`: Voice State Update enviado (con `self_mute`/`self_deaf`); `Connected`: sesión de voz y DAVE listos; `Reconnecting`: reanudación del Voice Gateway; `rtt_ms`: lo puede medir el proveedor con el heartbeat del Voice Gateway; pérdida: no documentada (`None`). DAVE es obligatorio para llamadas desde el 1-mar-2026 ([DAVE](https://docs.discord.com/developers/topics/voice-connections#end-to-end-encryption-dave-protocol)). | Scope `voice`: solo socios. | "voice calls only work with lobby members". |
| `Abilities` / `Permit::Denied(motivo)` | Permisos efectivos calculados de roles y overwrites ([Permissions](https://docs.discord.com/developers/topics/permissions)) más los intents concedidos: sin `MESSAGE_CONTENT`, por ejemplo, el motivo sería "el proveedor no puede leer el contenido". | Solo lectura de identidad y servidores: todo lo demás `Denied`. | Según el acceso concedido a la aplicación. |
| `Envelope { generation }` (sesiones numeradas) | Cada IDENTIFY nuevo (no un Resume) es una generación nueva; lo que llegue del websocket anterior se descarta. Importa porque IDENTIFY está limitado a 1000 cada 24 h. | — | — |
| Instantánea de arranque y "Borrar datos locales" | `forget_account` es una operación local, no prueba cumplimiento de todas las obligaciones del servicio. Para una integración real, los Developer Terms §5.b–c exigen borrado en los casos indicados y cifrado de API Data en reposo ([términos](https://support-dev.discord.com/hc/en-us/articles/8562894815383-Discord-Developer-Terms-of-Service)). La caché simulada actual no contiene datos de Discord. | Ídem. | La guía de comunicaciones indica que no se copie y almacene la información de amigos de Discord y que desaparezca del juego al desvincular. No se debe persistir esa lista en la instantánea genérica ([Full Access to Discord Friends](https://docs.discord.com/developers/discord-social-sdk/core-concepts/communication-features#full-access-to-discord-friends)). |

### Revisión de la recomendación (segunda iteración)

El 2026-10-14 quien dirige el proyecto pidió que RUNE **no** se convierta en un
administrador de bots, que la arquitectura se mantenga independiente de
cualquier proveedor concreto y que permita integraciones autorizadas
futuras. En consecuencia:

1. Se descartó el proveedor basado en cuenta bot. En la iteración OAuth2
   posterior sí se crea `rune-discord`, limitado a identidad y servidores.
   El análisis de las secciones 1 a 7 queda como referencia.
2. El trabajo de esta iteración fue en los contratos que cualquier proveedor
   autorizado implementaría (tabla de arriba): entrega con reintento
   idempotente, fallos con nombre, presencia con actividad, sesiones de voz,
   habilidades por conversación, sesiones numeradas y caché de arranque.
3. La iteración posterior eligió OAuth2 para identidad y servidores; la
   integración de comunicaciones sigue abierta. Lo que este documento establece: por vías autorizadas, Discord no permite operar
   una cuenta personal completa desde un cliente de terceros (DMs, grupos,
   amigos, notificaciones); el Social SDK ofrece parte de la comunicación
   con cuenta vinculada y su encaje para una app que no es un juego no está
   verificado. Un
   servicio cuyo protocolo admita clientes de terceros con la cuenta propia
   encajaría sin esas restricciones; no se evaluó aquí.

## Riesgos y reglas para RUNE

1. **Regla original, modificada por la excepción explícita de sesión personal anterior:** nunca recibir credenciales en el chat, logs o configuración. La variante no oficial opera únicamente el login temporal de su dueño; no importa otros perfiles. **Restricción de Discord:** La Developer Policy (regla 4) lo prohíbe y los self-bots se sancionan con el cierre de la cuenta ([política](https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy), [soporte](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)). No debe existir campo de "token de usuario" ni importación desde el cliente oficial, y el formato de token de usuario debe rechazarse si es distinguible. Detectar ese formato está **no verificado**: no se encontró en la doc pública.
2. **Credenciales de una futura integración autorizada:** el criterio de RUNE es guardarlas en el keyring del SO, nunca en texto plano, logs ni config. Los Developer Terms exigen protegerlas y cifrarlas en materiales accesibles por terceros; el keyring es una decisión de implementación, no un producto concreto exigido por Discord ([términos](https://support-dev.discord.com/hc/en-us/articles/8562894815383-Discord-Developer-Terms-of-Service)). El proveedor OAuth2 actual conserva credenciales efímeras solo en RAM; no hay almacenamiento persistente.
3. **No incrustar credenciales en el repositorio.** El registro de una aplicación y su distribución se definirían al evaluar una integración autorizada; no se requiere que cada usuario de RUNE cree un bot.
4. **Etiquetado honesto en la UI.** Cada proveedor declara identidad y capacidades, y la UI explica las acciones no disponibles. Hoy muestra la simulación local; un proveedor futuro identificará la cuenta con la que opera.
5. **Solo acciones iniciadas por el usuario.** Nada de auto-respuestas, mensajes programados ni actividad de relleno (ToS "auto-messaging"; Developer Policy, reglas 2 y 13).
6. **Sin scraping ni archivado masivo.** La caché local debe ser mínima, con retención limitada y opción de borrado. Hay que borrar la API Data al dejar de usarla (Developer Policy, regla 20; Developer Terms §5). Prohibido usar el contenido para entrenar modelos (regla 21).
7. **Gateway prudente:** priorizar Resume sobre Identify (límite de 1000 cada 24 h con reseteo de token), backoff con jitter, respetar 120 eventos cada 60 s y los rate limits REST (50 req/s, 10k inválidas cada 10 min). Usar User-Agent `DiscordBot (url, versión)` ([Gateway](https://docs.discord.com/developers/events/gateway#rate-limiting), [rate limits](https://docs.discord.com/developers/topics/rate-limits), [referencia](https://docs.discord.com/developers/reference#user-agent)).
8. **Intents:** referencia para aplicaciones Gateway; RUNE no solicita intents ni incorpora un bot. No se pueden convertir scopes OAuth2 en permisos del Gateway.
9. **Nada de ingeniería inversa** del cliente oficial ni del Social SDK, ni uso de endpoints o opcodes no documentados (ToS; Developer Terms §(a) y (c)(i); Social SDK Terms §2.b).
10. **Social SDK:** no se incorpora. Sus [términos §2.b(vii)](https://support-dev.discord.com/hc/en-us/articles/30225844245271-Discord-Social-SDK-Terms) restringen productos que compitan directa o indirectamente con Discord. Su existencia técnica no concede autorización para RUNE; Linux además es Experimental.
11. **OAuth2 en escritorio:** implementado con código, `state`, PKCE S256 y redirect exacto `http://127.0.0.1:48731/oauth/callback`. Requiere aplicación pública registrada. El canje público directo y la revocación remota deben verificarse contra esa aplicación; no se presupone soporte para puertos arbitrarios.

## Recomendación de primer proveedor real

**Primera integración elegida: OAuth2 experimental de identidad y servidores.**
Los contratos de conversaciones, mensajes, presencia y multimedia continúan
probados contra la simulación. El adaptador usa HTTP oficial sin Social SDK;
el flujo real requiere la aplicación registrada y consentimiento del usuario.
Ver [configuración y validación](DISCORD_OAUTH.md). La alternativa bot de la sección 7 no responde al objetivo del
proyecto. Una integración futura necesita un caso de uso autorizado y
capacidades comprobadas; la existencia de métodos del SDK por sí sola no
confirma que RUNE sea elegible.

En particular, un proveedor con historial limitado deberá distinguir el fin
del historial accesible del inicio real de una conversación. Sus resultados
no pueden borrar mensajes de caché fuera del rango que afirma cubrir. Es una
consecuencia de diseño para RUNE, no una garantía del Social SDK. DAVE y la
compatibilidad de audio se validarían dentro del adaptador correspondiente;
el micrófono local de RUNE no demuestra una llamada con Discord.

---

### Pendiente de verificación

- Interoperabilidad del canje y revocación HTTP públicos de RUNE con la aplicación registrada. S256 y el uso de PKCE en clientes públicos sí tienen evidencia oficial.
- Si Discord acepta puertos arbitrarios de loopback en el `redirect_uri`.
- Frase literal que prohíba Bearer en el Gateway (la ausencia de mecanismo sí está confirmada).
- Una autorización específica de Discord para el uso del Social SDK en RUNE, considerando la restricción sobre productos competidores; no se presupone por poder activar Comms Access.
- Existencia y estabilidad de una cabecera C pública en el archivo del Social SDK.
- Si un bot operado manualmente tiene alguna posición oficial explícita.
- Permiso exacto requerido para añadir un bot (probablemente `MANAGE_GUILD`).
- Aplicación efectiva del corte DAVE del 1-mar-2026.
