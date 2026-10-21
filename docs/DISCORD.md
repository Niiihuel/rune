# Proveedores de Discord

Lo que el código no dice solo: qué proveedores hay, cómo entra la sesión no
oficial y de dónde sale lo que sabe de Discord. Las reglas y las
autorizaciones del dueño están en [SECURITY.md](SECURITY.md); las rutas,
cuerpos y mensajes de Gateway, en el código de `rune-discord-session`.

## Tres proveedores

`ProviderKind::{Simulated, Official, Unofficial}`; nada por encima de
`rune-provider` conoce un servicio concreto.

- **Simulación** (`rune_provider::mock`, `--provider mock`): datos
  inventados y deterministas; la interfaz lo dice («Simulación local · sin
  conexión real»).
- **OAuth2 oficial** (`rune-discord`, `--provider discord
  --discord-client-id <id>` o `RUNE_DISCORD_CLIENT_ID`): sólo `identify
  guilds`, sin mensajes ni llamadas. Aplicación pública con **Public
  Client** y el redirect exacto `http://127.0.0.1:48731/oauth/callback`;
  PKCE S256, sin secreto, token sólo en memoria. El canje contra Discord no
  se validó todavía con una aplicación registrada.
- **Sesión personal no oficial** (`rune-discord-session`, feature
  `session-login`, `--provider discord-session`): la cuenta del dueño por la
  API REST y el Gateway del cliente web, como Serein. Va contra los términos
  de Discord y puede costar la cuenta; autorizarlo no implica aprobación de
  Discord.

Sin `--provider`, RUNE usa la última elección (`~/.config/rune/provider`)
o, si la compilación la trae, la sesión no oficial.

## La sesión no oficial

- **Login**: `rune-login` abre el mismo ejecutable como proceso auxiliar
  con una WebView efímera (WebKitGTK en Linux, WebView2 en Windows) en
  `https://discord.com/login`; la persona entra, con su MFA y su CAPTCHA.
  La credencial vuelve por una tubería privada acotada, como un
  `SessionSecret` sin `Clone`, serialización ni `Debug`; nunca por
  argumentos, entorno, archivos ni logs. El proveedor verifica la identidad
  (`GET /users/@me`) antes de `SignedIn`.
- **Sesión recordada**: la credencial va al llavero del sistema, una entrada
  por cuenta; al arrancar se verifica sólo la más reciente. Cerrar sesión u
  «Olvidar» borra su entrada y la caché cifrada de la cuenta.
- **hCaptcha**: si una escritura recibe un `400` con `captcha_key` y
  `captcha_service: "hcaptcha"`, la escritura espera y la interfaz pregunta.
  «Verificar» abre una WebView aislada en una página propia de RUNE
  (`https://captcha.rune.invalid/`); la respuesta permite **un único
  reintento** de esa misma escritura. Nunca se automatiza ni se reenvía.
- **Identidad**: se presenta como el cliente web en el navegador de esas
  mismas ventanas (user agent, `X-Super-Properties`, Identify), una sola
  identidad en REST, Gateway, voz y ventanas (`rune-discord-session::browser`).
- **Ritmo**: una solicitud REST a la vez con espera global tras un `429`;
  cada escritura es un solo intento (un `5xx` o un corte quedan como
  resultado ambiguo, no se repiten).
- **Entorno**: compilar y ejecutar en `nix-shell --arg withLogin true` (o
  `nix develop .#session`); WebKitGTK necesita los plugins de GStreamer que
  ese entorno expone en `GST_PLUGIN_SYSTEM_PATH_1_0`.

## Diagnóstico

Voz, Gateway e invitaciones dejan líneas `rune: voice …`, `rune: gateway: …`
y `rune: invitation: …` en stderr y en el registro
(`~/.local/state/rune/rune.log`, en Windows `%LOCALAPPDATA%\RUNE\logs`).
Nunca llevan token, `session_id`, claves ni ids de personas: si algo falla
contra Discord, mandar esas líneas y el texto del aviso.

## De dónde sale lo que sabe

- **Serein**, leído en `references/serein*` (fuera de git), sin copiar
  código.
- **REA estática** del cliente oficial (autorizada el 2026-10-16): los
  bundles públicos bajados **sin sesión** desde los que lista
  `https://discord.com/app`, leídos con `grep` y scripts propios sin
  ejecutarlos, y cadenas del paquete de escritorio instalado. Lo que el
  código cita como «REA» sale de estos:

  | Archivo | Build | SHA-256 |
  | --- | --- | --- |
  | `web.4d4b2f75fdea5470.js` | 641004 | `667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae` |
  | `933ace571a9c6560.js` (textos en inglés) | 641004 | `d9039967920acd48ce3a2b3409e14b63724f49cce42484e63d351f6f264632fc` |
  | `discord_voice.node` (Discord 1.0.161) | — | `28300009b1b5a9259e6f77454a6d800155fed808ed013ac53235f8d0b6049a23` |

- La documentación oficial, donde existe.

Todo se reimplementa. Las pruebas usan servidores HTTP, WebSocket y UDP en
loopback con datos sintéticos; nunca cuentas reales ni la red.
