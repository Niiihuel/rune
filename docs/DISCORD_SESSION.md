# Sesión personal experimental de Discord

El 2026-10-14 el dueño pidió expresamente usar un mecanismo de sesión como
el de Serein, documentarlo como **no oficial** y permitir una WebView temporal
para el login. La interfaz principal sigue hecha en limn. Esta decisión
reemplaza la restricción anterior del proyecto sobre ese mecanismo; no
constituye autorización o aprobación de Discord.

**Estado: login, identidad y lectura real confirmados manualmente por el
dueño el 2026-10-14.** Después del arreglo de GStreamer confirmó el acceso;
una captura posterior mostró servidores, canales e historial en RUNE.
No se copian las capturas ni los datos de cuenta a este repositorio.
Envío, edición, borrado, stickers, votos, reacciones Unicode, typing y fijados
están verificados con fixtures locales y requieren una prueba manual
independiente contra Discord. La ruta OAuth2 sigue
disponible por separado en [DISCORD_OAUTH.md](DISCORD_OAUTH.md).

## Alcance actual

**Invitaciones, 2026-10-14:** se pausó la aceptación experimental, que sólo
se había probado con fixtures; los cierres del Gateway conservan desde
entonces su código numérico (sin el motivo remoto) y un 4004 descarta la
sesión y sus trabajos pendientes. Se reactivó el 2026-10-16 con el hCaptcha
resuelto por la persona ([INVITATIONS.md](INVITATIONS.md)).

La revisión siguiente adapta el ciclo de errores y sesión de un clon nuevo
de Serein, sin reactivar invitaciones: [mecanismo y diferencias](SEREIN_SESSION_FLOW.md).

- Login manual en una ventana temporal propia de RUNE, con sesión web nueva.
- Candidato de sesión entregado al adaptador por un canal privado del proceso.
- Verificación HTTP de identidad y entrada al espacio de trabajo nativo.
- Gateway JSON: carga inicial de servidores, canales y conversaciones
  privadas existentes; eventos de mensajes y cambios de navegación/permisos.
- Historial paginado y acciones de envío, edición y borrado desde el composer
  y los controles de mensajes. El dueño confirmó lectura; las escrituras
  todavía se validan sólo con fixtures, sin enviar mensajes reales.
- Catálogos de emojis custom y stickers del servidor, traducción legible de
  referencias recibidas y envío explícito de un sticker disponible.
- Cancelación, rechazo de respuestas tardías y cierre local de sesión.
- Vista previa y aceptación explícita de invitaciones de servidor, con el
  hCaptcha que pida Discord resuelto por la persona en una ventana aislada
  (reactivada el 2026-10-16; sin probar con una cuenta real). Desde el
  2026-10-18 también de grupo, de amistad y de ingreso como invitado, y una
  tarjeta por invitación aunque haya texto alrededor (hasta diez; sólo
  fixtures). Ver [INVITATIONS.md](INVITATIONS.md).
- Amigos, solicitudes, bloqueos y apodos; abrir y cerrar privados;
  nombre, imagen y salida de grupos; presencia de amigos. Por las rutas
  que usa Serein y sólo con fixtures locales (ver
  [Amigos y conversaciones](#amigos-y-conversaciones-fuera-de-servidores-2026-10-17));
  crear grupos, cambiar sus integrantes y pasar la propiedad por las rutas
  del cliente web oficial (REA), también sólo con fixtures.
- hCaptcha en cualquier escritura: la persona lo resuelve en la ventana
  aislada de RUNE y la escritura se repite una sola vez (ver
  [Verificación](#verificación-en-cualquier-escritura-2026-10-17)); sólo
  fixtures.
- Editar el perfil propio (nombre, avatar, banner, biografía, pronombres y
  color) y el de cada servidor (apodo, avatar, banner y biografía), sólo
  con fixtures (ver [Editar el perfil](#editar-el-perfil-propio-2026-10-17));
  usuario, correo y contraseña nunca, porque piden la contraseña.
- Voz en canales de servidor con DAVE, escenarios (público, mano
  levantada, invitación y moderación), mirar la pantalla que comparte
  otra persona en la llamada, sus cámaras y compartir la pantalla propia,
  sólo con fixtures locales (ver
  [Voz](#voz-en-canales-de-servidor-2026-10-16)); llamadas en privados y
  grupos, también sólo con fixtures (ver
  [Llamadas privadas](#llamadas-privadas-2026-10-17-rama-llamadas-privadas)).
  Archivos adjuntos por la ruta de
  Serein, sólo con fixtures (ver [Adjuntos](#adjuntos-de-salida-2026-10-17)).
- Estado propio y personalizado (op 3 y la configuración de la cuenta),
  notificaciones por servidor y canal, carpetas de servidores, privacidad de
  mensajería y notas privadas, sincronizados con la cuenta, sólo con
  fixtures (ver
  [Cuenta sincronizada](#cuenta-sincronizada-2026-10-17-rama-cuenta-sincronizada)).
- Aplicaciones: botones y menús de sus mensajes, modales, respuestas
  efímeras y comandos de barra con opciones y sugerencias, por
  `POST /interactions` con la sesión del Gateway, como Serein y el cliente
  web oficial (REA), sólo con fixtures (ver
  [Comandos e interacciones](#comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones)).
- Hilos y foros: leerlos (tipos 5, 10–12, 15 y 16; `THREAD_*`), responder,
  crear hilos y publicaciones, seguir, cerrar, bloquear, renombrar,
  etiquetar, eliminar y paginar abiertos y archivados, por las rutas de
  Serein y documentadas, sólo con fixtures locales (ver
  [Hilos y foros](#hilos-y-foros-2026-10-17)).
- Administración de servidores: crear, editar, eliminar y salir; canales,
  categorías y sus permisos; roles y su orden; roles, apodo, aislamiento,
  expulsión y veto de miembros; invitaciones; emojis y stickers; registro
  de auditoría; miembros de a páginas, poda, webhooks e integraciones; y los
  GIF favoritos de la cuenta. Por las rutas de Serein y las documentadas,
  sólo con fixtures locales (ver
  [Administración](#administración-del-servidor-2026-10-17)).
- Lecturas sincronizadas (ACK, no leído y servidor leído, `MESSAGE_ACK` y
  actualizaciones pasivas), búsqueda en todo el historial con filtros y
  miembros buscados por nombre, por las rutas de Serein y sólo con
  fixtures (ver
  [Lecturas y búsqueda](#lecturas-búsqueda-y-miembros-remotos-2026-10-17)).
  Los datos de la cuenta permanecen en RAM, sin SQLite. La credencial,
  además, se recuerda en el llavero del sistema (ver
  [Sesión recordada](#sesión-recordada-en-el-llavero-2026-10-16)).

No se presentan scopes OAuth2 para una sesión personal. La credencial no
está limitada a `identify guilds`. La identificación del proveedor y las
capacidades están en Ajustes → Conexión; los detalles de este mecanismo
quedan aquí, sin avisos repetidos en el acceso o el chat. Verificar la
identidad y recibir la navegación son estados distintos: mientras se abre
Gateway, el espacio de trabajo muestra el estado de conexión correspondiente.

Discord publica que automatizar cuentas normales fuera de sus interfaces
OAuth2/bot está prohibido y puede terminar en la suspensión de la cuenta.
La etiqueta «experimental» o «uso personal» no elimina esa condición.
[Política publicada](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots).

## Compilar

El motor web está detrás de una feature y solo se utiliza en el proceso de
login para Linux. La compilación normal conserva mock y OAuth2.

```sh
nix-shell --arg withLogin true
cargo run -p rune-desktop --features session-login -- --provider discord-session
```

La variante del entorno flake es `nix develop .#session`. No requiere un
Application ID, redirect OAuth2 ni secreto de aplicación: es un mecanismo
distinto. No se habilitan fallbacks de sesión personal desde el proveedor
OAuth2.

La ejecución también debe hacerse dentro de ese entorno Nix, aunque el
binario ya esté compilado. GStreamer descubre sus plugins mediante
`GST_PLUGIN_SYSTEM_PATH_1_0`, que configura el setup hook del shell. El
RPATH permite encontrar bibliotecas, pero no sustituye ese registro de
directorios de plugins. Un empaquetado independiente deberá conservar esa
configuración en su lanzador.

## Fronteras

### Entrada y transición al workspace

El recorrido solicitado de WebView temporal y continuación en Rust nativo ya
existe. Sin cuentas recordadas, el host abre la página de Discord **antes**
de crear la ventana de RUNE (2026-10-16): es la única ventana hasta que hay
sesión, y cerrarla sin entrar cierra RUNE. El resultado se entrega al proveedor
con `PrefetchedLogin`. Wayland no permite ocultar una ventana ya creada, por eso
no se crea. Con la ventana ya abierta, la UI emite `BeginSignIn` sola una única vez
(2026-10-16), así la página de login de Discord, con su QR, aparece directo;
tras cancelar o cerrar sesión, la pantalla limn «Iniciar sesión» lo emite con un
clic y nunca reabre la ventana por su cuenta. El proveedor
personal pasa a `AwaitingExternal` y abre el auxiliar `rune-login`. Su WebView
es efímera y termina antes de que `HostedLogin` entregue el resultado por el
pipe privado. El adaptador verifica la identidad y emite `Ready` y `SignedIn`;
la UI deja de mostrar la pantalla de acceso y entra al workspace limn, incluso
si la cuenta no tiene conversaciones. Gateway e historial continúan en Rust.
No se carga el chat dentro de la WebView ni hace falta crear otra transición.

Las fronteras están implementadas en [HostedLogin](../crates/rune-login/src/lib.rs),
el [adaptador](../crates/rune-discord-session/src/provider.rs) y
[auth limn](../crates/rune-ui/src/auth.rs). La
[prueba de UI](../crates/rune-ui/src/tests/auth.rs)
`signed_in_without_conversations_opens_the_workspace_and_can_sign_out`
verifica la entrada y el regreso al acceso; esta revisión documental no volvió
a ejecutar el login ni a usar una cuenta.

**Elegir el proveedor (2026-10-17).** El [host](../apps/desktop/src/main.rs)
arranca con `--provider mock|discord|discord-session`; sin argumento usa la
última elección, guardada en `$XDG_CONFIG_HOME/rune/provider` (sólo la clave
del proveedor, nunca credenciales), y si no hay, el inicio de sesión no
oficial cuando la compilación lo trae (las releases), o Mock si no (o con
`--data`/`--latency`, opciones de la simulación). Una release nunca abre
sola en la simulación (2026-10-19, al instalar la 0.1.0). Desde la ventana se
elige en Ajustes → Conexión → «Elegí el proveedor» y en la página de acceso
(«Usar otro proveedor»): cada opción dice su origen (simulado, oficial, no
oficial), lo que es, por qué no se puede usar en este equipo (OAuth2 sin el
ID público de la aplicación; la sesión personal sin Linux o sin la feature
`session-login`) y, la no oficial, la advertencia de experimento. Cambiar
pide confirmación; el host crea un `Client` y un `Store` nuevos con el
proveedor elegido (en caliente, sin reiniciar) y la ventana se limpia como al
cambiar de cuenta: dominio, borradores, búsquedas, visores, subidas y
trabajos; se conservan preferencias, tema y cuentas recordadas (éstas viven
en el llavero de cada proveedor). La sesión personal elegida así no abre la
página de Discord antes de la ventana: la UI emite `BeginSignIn` sola, como
con la ventana ya abierta. El origen del proveedor queda visible siempre al
pie de la columna de canales y en la página de acceso.

**Foco al volver del login (2026-10-17).** Cuando el auxiliar se cierra
(`AwaitingExternal` → `Exchanging`, `SignedOut` o `Failed`), el host pide su
ventana con `ViewportCommand::Focus` y `RequestUserAttention`. En X11, winit
la activa (`_NET_ACTIVE_WINDOW`). En Wayland `focus_window` de winit 0.30 no
hace nada; `request_user_attention` pide un token xdg-activation propio y
activa la superficie, y cada compositor decide si la enfoca o la marca como
urgente. Pasar un token del auxiliar a la ventana existente no es posible:
ni winit 0.30 ni eframe exponen activar una ventana ya creada con un token
ajeno. **Sin probar** en Hyprland, GNOME o KDE con un login real.

`discord-session` necesita Linux y la feature `session-login`; OAuth2 sigue
siendo un proveedor separado.

### Aislamiento del auxiliar

`apps/desktop` compone `rune-discord-session` con `rune-login`. El adaptador
solo conoce `SessionLogin`; obtiene un `SessionSecret` sin `Clone`, sin
serialización y con `Debug` redactado. Los eventos y comandos generales
transportan estados y acciones, nunca la credencial.

`rune-login` ejecuta el mismo binario como proceso auxiliar antes de
inicializar eframe. GTK/WebKit poseen ese bucle aislado. El resultado vuelve
por pipes anónimos acotados, asociados a un intento; no por argumentos,
variables de entorno, clipboard ni un archivo. Cancelar el futuro invalida
el resultado y termina el auxiliar.

El motor usa una sesión efímera; no importa perfiles, cookies, tokens o
almacenes de otros navegadores/clientes. El puente observa únicamente el
tráfico de autenticación de la ventana que la persona abrió dentro de RUNE.
No lee campos de contraseña, email o códigos MFA, ni cuerpos de mensajes.
La cuenta se verifica antes de anunciar éxito. Un callback JavaScript por
sí solo no demuestra procedencia: el documento principal, origen, intento
y límite de tamaño deben comprobarse en la parte nativa.

## Restricciones conservadas

- TLS normal, sin aceptar certificados inválidos.
- Desde el 2026-10-19 se presenta como el cliente web en un navegador
  WebKit, con una sola identidad en REST, Gateway, voz y las ventanas de
  login y hCaptcha ([detalle](#identidad-de-navegador-2026-10-19-rama-identidad-de-navegador)).
  Reimplementada: no se copia la de Serein ni su lógica.
- Login, MFA y CAPTCHA los completa la persona; un rechazo del motor o
  desafío no compatible termina el intento. No se aplican mecanismos para
  eludirlo.
- Sin lectura del cliente oficial, datos de otros perfiles o cuentas ajenas.
- Sin credenciales en chat del agente, logs, capturas, SQLite o configuración.
- La credencial sola se guarda en el llavero del sistema (autorizado el
  2026-10-16); cerrar sesión la borra de ahí y descarta los datos locales;
  no afirma revocación remota. Desde el 2026-10-18 la cuenta verificada
  tiene caché local cifrada, como la simulación (ver abajo).

## Caché local cifrada (2026-10-18)

Decisión del dueño: la cuenta real conserva mensajes, borradores, marcas de
lectura y la instantánea entre arranques, como Serein (que guarda una caché
SQLite aislada por cuenta, sin cifrar). RUNE la cifra con una clave por
cuenta en el llavero; ver [SECURITY.md](SECURITY.md#datos-locales).

- La cuenta se conoce recién al verificarla: entonces el proveedor manda
  `Event::StorageIdentity` (`rune.discord-session.v1` y el id). El núcleo
  pide su clave al llavero fuera del hilo de la interfaz y desbloquea su
  parte de la caché.
- Al restaurar una sesión recordada no se manda la instantánea pelada de la
  verificación: lo guardado de esa cuenta (`Update::Restored`) se muestra
  como de la caché («Conectando…», sin confirmar, lo que pide estar
  conectado deshabilitado) hasta el READY del Gateway, que lo reemplaza. Sin
  caché, la pantalla de inicio dice «Cargando servidores…». Un inicio de
  sesión nuevo sigue mandando su instantánea pelada primero.
- Cerrar sesión y «Olvidar» una cuenta mandan `Event::ForgetStorage`: se
  borran sus filas y su clave. Cambiar de cuenta conserva todo.
- Lo que encuentra la búsqueda en todo el historial se guarda aparte de la
  historia (tabla `found`), y la búsqueda local lo lee.
- Sólo fixtures (núcleo, proveedor y host): sin probar contra el servicio
  real ni contra un Secret Service real.

## Sesión recordada en el llavero (2026-10-16)

El dueño autorizó el 2026-10-16 guardar la credencial de esta sesión en el
llavero del sistema operativo para no iniciar sesión en cada arranque, y
recordar varias cuentas como Serein.

- **Frontera:** [`rune_platform::secrets`](../crates/rune-platform/src/secrets.rs)
  define `SecretStore` (listar, cargar, guardar, borrar; una entrada por
  servicio y cuenta). En Linux la implementa Secret Service con el crate
  `secret-service` (async sobre el zbus/Tokio que ya usa `rune-platform`,
  sesión cifrada DH, cripto en Rust puro). Atributos de la entrada:
  `application=rune`, `service=discord-session`, `account=<id>`,
  `name=<nombre visible>`; la etiqueta es «RUNE: sesión guardada de
  <nombre> (discord-session)». Listar no lee ningún secreto. Cada guardado
  crea la entrada de nuevo (y después borra la vieja de esa cuenta, así un
  fallo conserva la anterior), de modo que su fecha de creación es la del
  último uso: la lista sale ordenada por ella. Fuera de Linux no hay
  llavero. Los errores son textos fijos; `Saved` tiene `Debug` redactado y
  `Entry` (cuenta y nombre) no lleva secreto.
- **Al arrancar** el adaptador lista las cuentas y emite
  `RememberedAccounts` antes de anunciar nada. Si hay alguna, carga sólo la
  más reciente, emite `Exchanging` (nunca `SignedOut` primero, que abriría la
  ventana de login) y verifica la identidad con el mismo `GET /users/@me`
  de un login nuevo. Las demás no se verifican hasta que se eligen. Éxito:
  `SignedIn`, y la entrada se guarda otra vez (nombre al día, pasa a ser la
  última usada) antes de `SessionRemembered(true)`. Un 401 o una credencial
  que no pasa la validación local borra esa entrada, emite la lista que
  queda y después `SignedOut`: con otras cuentas la UI las muestra; sin
  ninguna abre la página de login una vez. Un fallo transitorio (red, 5xx)
  conserva la entrada y emite `Failed`; «Volver a intentar» vuelve a probar
  esa entrada, no abre el login. Sin reintentos automáticos.
- **Login nuevo:** tras verificar la identidad se guarda la credencial en
  segundo plano (sin bloquear el loop), junto a las de otras cuentas. Si el
  llavero falla la sesión sigue en RAM y aparece un aviso no bloqueante.
- **Cambiar de cuenta** (`Command::UseAccount(Some(id))`) termina la sesión
  abierta como un cierre de sesión (mismo `SignedOut`: dominio, borradores,
  búsquedas, visores, historial y trabajos se limpian) pero **sin** borrar
  su entrada, y después verifica sólo la cuenta elegida. Si esa entrada ya
  no está, queda en `SignedOut` con el motivo. **Agregar una cuenta**
  (`UseAccount(None)`) hace lo mismo y abre la página de login.
- **Cerrar sesión** espera la escritura pendiente y borra la entrada de esa
  cuenta antes de emitir `SignedOut` (con límite de 15 s); un fallo se
  avisa. Un 401 durante la sesión o el cierre Gateway 4004 también la borra.
  Las otras cuentas quedan.
- **Pantalla de acceso:** con cuentas recordadas muestra «Continuar como
  <nombre>» (con su avatar desde 2026-10-17, ver abajo; la inicial mientras
  no llega o si no tiene), «Olvidar» con confirmación
  (`Command::ForgetAccount`) y «Usar otra cuenta»; tras un fallo transitorio
  también «Volver a intentar». Sin cuentas, el «Iniciar sesión» de siempre.
- **Ajustes → Mi cuenta → Sesión → Cambiar de cuenta** lista las otras
  cuentas recordadas y «Agregar una cuenta» (sólo si el proveedor declara
  `Capabilities::saved_accounts`). **Ajustes → Conexión** muestra «Sesión
  recordada» con «Olvidar esta sesión» (`Command::ForgetSession`: borra la
  entrada y mantiene la sesión abierta) o «Sesión no recordada». Otros
  proveedores no muestran esas filas.

Pruebas (`crates/rune-discord-session/src/tests/keyring.rs`, llavero falso
en memoria ordenado como el real): restauración sin ventana de login; con
varias cuentas sólo se verifica la última usada (una sola petición);
cambiar de cuenta pasa por `SignedOut`, conserva ambas y restaura la otra
con su propia credencial; agregar una cuenta conserva la abierta y abre el
login; una cuenta sin entrada no se verifica; olvidar una conserva las
demás y la sesión abierta; una credencial revocada se borra y deja la lista
del resto antes de `SignedOut`; fallo transitorio que conserva la entrada;
guardado/olvido/borrado al cerrar sesión; y llavero no disponible con la
sesión sólo en RAM. Ningún evento contiene ninguna de las credenciales. En
`rune-ui` (`tests/auth.rs`): la lista reemplaza la apertura automática del
login, confirmación de «Olvidar», cambio desde Ajustes que limpia la cuenta
y conserva la lista sin abrir otro login. `rune-platform` comprueba contra
un bus D-Bus privado sin Secret Service que todo falla como «no
disponible». Las pruebas nunca usan el llavero real: el constructor público
usa un llavero ausente en `cfg(test)`.

**Avatar de las cuentas recordadas (2026-10-17).** El guardado agrega el
atributo `avatar=<hash>` junto a `name` (público, no secreto: el mismo hash
que viaja en cualquier objeto de usuario; nunca la credencial ni una URL).
`Entry` suma `avatar` (vacío en entradas viejas o sin avatar) y
`RememberedAccount` suma `avatar: Option<RemoteImage>`: el adaptador
reconstruye la ruta pública `avatars/{id}/{hash}.png?size=128` del CDN con
la misma validación de hash que el resto de los avatares; sin avatar o con
un hash malo, el avatar por defecto de Discord (desde 2026-10-17, ver
abajo). La página de acceso y Ajustes → Mi cuenta → Cambiar de
cuenta piden esas imágenes por el servicio de imágenes del host (orígenes
del proveedor, sin cabeceras de cuenta), como cualquier avatar. Prueba:
`tests/keyring.rs` (`the_picture_is_kept_beside_the_name…`) con el llavero
falso; sólo fixtures.

**Sin probar:** un Secret Service real (GNOME Keyring, KWallet), sus
diálogos de desbloqueo, el orden por fecha de creación en ellos (resolución
de segundos: dos guardados en el mismo segundo empatan) y una restauración
o un cambio de cuenta contra Discord. Si se abandona una escritura que
espera un diálogo de desbloqueo (límite al cerrar sesión), el servicio
podría completarla después; queda como limitación conocida. Listar y cargar
al arrancar tienen 15 s cada uno.

«Solo en memoria» describe la retención de RUNE. No garantiza eliminación
criptográfica de todas las copias dentro del motor, swap o volcados del SO.
Los fixtures comprueban el límite del resultado, la invalidación de intentos
y el cierre del proceso auxiliar y de sus descendientes WebKit observados.
No miden toda la memoria del motor ni prueban su comportamiento ante cualquier
fallo del sistema.

## Voz en canales de servidor (2026-10-16)

Con la autorización del 2026-10-16 («si lo usa Serein, vamos con ello») el
adaptador se une a canales de voz de servidor con los mismos mensajes que
Serein. **Probado sólo con fixtures locales** (WebSocket y UDP en 127.0.0.1,
con un servidor de voz sintético y su propio emisor MLS). **No se hizo
ninguna llamada real**: hasta que el dueño la registre, no se afirma que
funcione contra Discord. No se cambia user agent ni ninguna huella del
cliente, no hay CAPTCHA en este camino y no se reintenta nada sin límite.

**Gateway principal.** `JoinVoice` en un canal de voz de servidor envía op 4
(`guild_id`, `channel_id`, `self_mute`, `self_deaf`, `self_video: false`);
`LeaveVoice` el mismo op con `channel_id: null`; `SetVoiceSelf` lo repite
con mute/deafen nuevos. Va por un `watch` (gana el último pedido, una ráfaga
de clics manda uno) y por el presupuesto de escritura existente. Tras un
`READY` nuevo el op 4 se manda de nuevo si había llamada, salvo que los
`voice_states` de ese `READY` muestren a la cuenta en voz con otra sesión
(siguió en otro dispositivo). Se reciben
`VOICE_STATE_UPDATE` (de todos: `VoiceChanged`/`VoiceLeft`; de la cuenta: su
`session_id`, que nunca sale del adaptador) y `VOICE_SERVER_UPDATE` (token y
endpoint; `endpoint: null` espera el siguiente). Los `voice_states` de cada
`GUILD_CREATE` llenan quién está en voz (hasta 4096; uno malformado se
ignora). Si el servidor de voz no llega en 15 s, la llamada termina con
motivo; si a la cuenta la mueven o la desconectan, también. Desde el
2026-10-17 un `VOICE_SERVER_UPDATE` fuera de `*.discord.media` (o con un
puerto inválido) termina el intento enseguida con «Discord envió un
servidor de voz inválido.» en vez de esperar esos 15 s. Las llamadas
privadas (DM y grupos, con «ring» por REST) están desde el 2026-10-17: ver
[Llamadas privadas](#llamadas-privadas-2026-10-17-rama-llamadas-privadas).

**Correcciones de voz (2026-10-16). No probado contra el servicio real;
sólo fixtures locales.**
- *Otro dispositivo* (corregido el 2026-10-17, ver abajo). El cliente
  oficial distingue sus estados de voz por la sesión del Gateway principal
  (`CONNECTION_OPEN` guarda el `session_id` del `READY`; en
  `VOICE_STATE_UPDATES` compara `t.sessionId===r`). RUNE hace lo mismo
  (`Calls::gateway_session`): un estado de la cuenta con otra sesión antes
  del propio es otro cliente de la cuenta y se ignora; después, ese
  dispositivo se quedó con la llamada: termina con «Tu llamada siguió en
  otro dispositivo.» y **no** manda op 4 de salida (colgaría al otro
  dispositivo). Sin la sesión del Gateway para comparar, como Serein: el
  candidato más nuevo gana hasta que el servidor de voz lo acepta (la
  conexión pasa de `Joining`), y antes de eso los estados de otro canal o
  sin canal se toman por viejos. Lo mismo si el `READY` nuevo la muestra con
  otra sesión aceptada. Desde el 2026-10-18 (`voz-protocolo`), si ese
  `READY` trae el servidor de la llamada como `unavailable` (o no lo trae),
  el op 4 espera su `GUILD_CREATE` y mira ahí la misma regla; si no llega
  en 15 s, lo pide igual.
- *Silencio del servidor.* `mute`/`suppress` y `deaf` van a
  `VoiceState::server_muted`/`server_deafened`, aparte de `self_mute` y
  `self_deaf`; la interfaz los marca en rojo y el micrófono de la llamada
  queda cerrado mientras dure (`Store::mic_held`).
- *Permisos.* `Abilities::speak` (bit 21 `SPEAK`) y `voice_activity` (bit
  25 `USE_VAD`): sin hablar se entra sólo a escuchar, con mute y el motivo;
  sin detección de voz la llamada usa pulsar para hablar.
- *Reunirse* (cambiado el 2026-10-17). Si una llamada que había conectado
  se pierde por cualquier motivo salvo el cierre 4014 (te sacaron), se
  reúne hasta 3 veces, a 1, 3 y 10 s, informando `Reconnecting { attempt }`:
  identifica de nuevo con la misma sesión y el mismo servidor (como el
  cliente oficial, que nunca pide op 4 para esto) y además manda op 4 otra
  vez (como Serein), por si Discord le da otro servidor. Antes borraba la
  sesión y esperaba un `VOICE_STATE_UPDATE` que Discord puede no repetir
  para el mismo canal, y terminaba a los 15 s. Un servidor de voz que no
  llega en 15 s gasta el intento. Tras 30 s conectada el contador vuelve a
  cero.
- *Escenarios (2026-10-17, sólo fixtures).* El tipo 13 es
  `ChannelKind::Stage` y se entra con la misma conexión de voz que una sala
  (`Capabilities::stages`). Serein no tiene escenarios: las rutas salen de
  la REA estática del bundle web oficial (`web.c5b338cc7e610833.js`,
  descargado sin sesión). El lugar de cada persona se lee de `suppress` y
  `request_to_speak_timestamp` como lo lee ese cliente: con `suppress` y sin
  pedido, público; con ambos, mano levantada; sin `suppress` y con pedido,
  invitada (micrófono retenido hasta aceptar); sin ninguno, oradora.
  Levantar o bajar la mano: `PATCH /guilds/{g}/voice-states/@me`
  `{"channel_id", "request_to_speak_timestamp": ahora o null}` (necesita
  `REQUEST_TO_SPEAK`, bit 32). Aceptar la invitación o subir como
  moderación: `{"channel_id", "suppress": false, "request_to_speak_timestamp":
  null}`; volver al público, lo mismo con `suppress: true`. La moderación
  (`MUTE_MEMBERS`) invita con `PATCH …/voice-states/{usuario}` `{"channel_id",
  "suppress": false, "request_to_speak_timestamp": ahora}` y pasa al público
  con `{"channel_id", "suppress": true}`. Un intento, sin repetir; confirma
  `VOICE_STATE_UPDATE`. *Por probar:* según el soporte de Discord la
  exigencia de E2EE es para llamadas que no son escenarios; si un escenario
  no negocia DAVE, la conexión de voz tiene que aceptarlo.
- *Fases.* `Securing` («Asegurando el audio…») entre el transporte listo y
  el grupo DAVE. `Waiting` («Esperando a los demás», 2026-10-18,
  `voz-protocolo`): conectada y sola en el canal; `CallPhase::is_connected`
  la cuenta como conectada en la interfaz, el audio y los sonidos.

**Gateway de voz v8** (`voice::connection`): `wss://<endpoint>/?v=8`, sólo
hosts `*.discord.media`, con el puerto que diga Discord (`:80` va a 443,
como el cliente oficial; antes del 2026-10-17 sólo se aceptaba 443).
Hello → Identify (`server_id`, `channel_id`, `user_id`, `session_id`,
`token`, `max_dave_protocol_version: 1`) → Ready (SSRC, IP, puerto y
modos; la IP debe ser pública y el modo `aead_xchacha20_poly1305_rtpsize`
estar ofrecido) → descubrimiento de IP por UDP (74 bytes; el pedido se
repite cada segundo hasta 8 s) → Select Protocol → Session Description
(clave de 32 bytes, `Zeroizing`, y `dave_protocol_version: 1` o la llamada
no empieza). Heartbeat op 3 `{t, seq_ack}` cada `min(intervalo, 5 s)`; dos
intervalos seguidos sin ACK se retoman (uno tarde se tolera, como Serein).
Un opcode desconocido se ignora y queda en el diagnóstico. Op 5 anuncia el
SSRC antes de cualquier medio y otra vez tras un resume, luego
`speaking: 1` al empezar a hablar y `speaking: 0` tras cinco tramas de
silencio `F8 FF FE` cuando el host deja 100 ms sin mandar. Ping UDP de 8
bytes cada 5 s (como Serein). Cierres: 4015 y caídas del socket retoman con
op 7 (`server_id`, `channel_id`, `session_id`, `token`, `seq_ack`) hasta 3
veces seguidas;
4014, 4006, 4009, 4004, 4011/4012/4016 y 4017 terminan con un motivo en
español. Límites: mensajes de 64 KiB, 256 mensajes de control por segundo,
datagramas de 4 KiB, 64 datagramas por vuelta, 30 s para conectar y para
acordar DAVE con otros.

**Cifrado de transporte.** Sólo `aead_xchacha20_poly1305_rtpsize` (el que
Discord exige soportar y el único que usa Serein): cabecera RTP (con CSRC y
preámbulo de extensión) en claro y autenticada, cuerpo de extensión y
carga sellados, contador de 32 bits big-endian al final como inicio del
nonce de 24 bytes. Nunca se reusa un nonce (el contador no da la vuelta).
`aead_aes256_gcm_rtpsize` no está implementado.

**DAVE (E2EE, protocolo 1)** sobre `davey =0.1.5`: identidad efímera por
llamada, paquete de claves (op 26, el `KeyPackage` TLS sin envoltorio, como
libdave y Serein), emisor externo (25), propuestas (27, sólo de personas
anunciadas por op 11) respondidas con commit/welcome (28), commit (29) y
welcome (30) con su transición, `ready` (23) para transiciones ≠ 0,
ejecutar (22), época nueva (24), y op 31 con un paquete nuevo si un grupo
no se puede procesar (hasta 3 veces). Un grupo con alguien no anunciado se
rechaza. Pedir protocolo 0 (quitar el cifrado) corta la llamada. Nada sale
sin grupo listo; solo en el canal no se transmite (no hay a quién). Se
desencripta sólo de quien está anunciado y tiene SSRC (op 5 u op 12).
Claves: transporte y DAVE viven en el futuro de la conexión y se borran al
salir, al cambiar de cuenta, al cerrar sesión o si Gateway cae.

**Fixtures** (`voice::tests`, `tests/session/voice.rs`, `gateway_tests.rs`):
apretón de manos completo, descubrimiento de IP, Select Protocol, grupo
DAVE con un segundo participante sintético, audio en los dos sentidos
(cifrado en ambas capas y verificado del otro lado), fin de ráfaga con
silencios, datagrama falsificado descartado, reanudación tras 4015,
silencio al estar en mute, salida que libera el enlace, cierres 4014/4017,
modo de cifrado ausente, unión/salida/mute por la API del proveedor y op 4
del Gateway principal. Vector XChaCha20-Poly1305 del borrador IRTF A.3.1
para el AEAD; el formato rtpsize se comprueba byte a byte contra él.

**Dependencia con aviso.** `davey` arrastra `openmls 0.8` → `hpke-rs 0.6.1`
→ `libcrux-sha3 0.0.8`, afectado por RUSTSEC-2026-0207 (salida incorrecta de
SHAKE incremental) y RUSTSEC-2026-0208 (pánico de SHAKE-256 AVX2). DAVE usa
la suite 2 (P-256, AES-128-GCM, SHA-256), que no pasa por SHAKE según el
diseño de HPKE **(no verificado en el código)**. Desde el 2026-10-16 RUNE
también parchea `hpke-rs` (`[patch.crates-io]` del `Cargo.toml` del
workspace): la copia de [`vendor/hpke-rs`](../vendor/hpke-rs/RUNE-PATCH.md)
cambia `libcrux-sha3` por `sha3` de RustCrypto y quita el backend libcrux;
`cargo audit` pasa de 8 vulnerabilidades a 0
([SECURITY.md](SECURITY.md#voz-y-dave-parche-de-hpke-rs-2026-10-16)).

### Primera prueba real y diagnóstico de voz (2026-10-17, rama `voz-real`)

**Lo que vio el dueño** con su cuenta: entrar a la voz a veces conecta y a
veces muestra «reconectando» y sale tras un tiempo límite; compartir una
ventana anduvo, la pantalla completa se canceló con «Se dejó de compartir la
pantalla — Discord envió una transmisión inválida.»; la cuenta pareció dos
veces en el canal. El stderr no decía nada de la voz.

**Diagnóstico siempre activo** (`voice::diag`). Cada paso deja una línea
`rune: <ámbito>: …` en stderr; el ámbito es `voice call` (la llamada),
`voice stream` (mirar una transmisión), `voice share` (la propia) o
`gateway` (el Gateway principal). Nunca llevan token, `session_id`, claves,
mensajes MLS, ids de personas ni la IP propia (un test de fixture lo
comprueba); sí el host del servidor de voz de Discord, opcodes, códigos de
cierre, modos de cifrado y conteos. Cómo leerlas, en orden:

| Línea | Qué dice |
| --- | --- |
| `gateway: op 4 sent: join…` | se pidió la llamada al Gateway principal |
| `voice call: own VOICE_STATE_UPDATE: …` | el estado propio que respondió Discord: canal, si la sesión es la de este dispositivo y si ya fue aceptada |
| `voice call: VOICE_SERVER_UPDATE: endpoint <host:puerto> (accepted\|refused…)` | el servidor de voz asignado; `null endpoint` espera otro |
| `voice call: opening the voice gateway (v=8) at <host>` / `voice gateway open` | el websocket de voz |
| `HELLO`, `IDENTIFY sent`, `READY (op 2): modes offered […]` | el apretón de manos y los cifrados que ofreció Discord |
| `IP discovery request sent (try N)` / `reply received` | el UDP; sin respuesta en 8 s suele ser un cortafuegos |
| `SELECT_PROTOCOL sent`, `SESSION_DESCRIPTION (op 4): mode …, dave_protocol_version …` | transporte acordado |
| `DAVE op N received/sent` | cifrado de extremo a extremo, sólo el número |
| `connected: media may flow` | la llamada está lista |
| `voice gateway closed by Discord: code C «razón»` | por qué cortó Discord |
| `dropped (…); resume N/3`, `RESUMED (op 9)` | reanudación |
| `lost: …; rejoin attempt N in S s` / `ended: …` | reunión o final, con el motivo que vio la interfaz |
| `gateway: reconnecting (…)`, `closed by Discord: code C`, `READY: new session`, `RESUMED` | cortes del Gateway principal (la llamada sigue si es `resume`) |
| `voice share: STREAM_CREATE…`, `STREAM_SERVER_UPDATE: endpoint … (accepted\|refused…), token …`, `STREAM_DELETE: reason …` | la transmisión propia: por qué se la dio por inválida |

**Causas encontradas y corregidas** (sólo fixtures; sin probar contra el
servicio real). Evidencia: REA estática del bundle web oficial
`web.c5b338cc7e610833.js` (SHA-256 `6f842876…0aec`, módulos 613681
`RTCControlSocket`, 576991/595528/763827 del `RTCConnectionStore`;
confirmado en `web.4d4b2f75fdea5470.js` y el canary `web.345eaa8eb96a3f0a.js`)
y de `discord_voice.node` 1.0.161 (SHA-256 `28300009…a23`), y Serein
(`crates/discord-gateway/src/voice.rs`, `crates/discord-voice/src/transport.rs`,
`apps/desktop/src/voice.rs`), sin copiar código.

1. **Puerto del servidor de voz.** RUNE sólo aceptaba `:443` o ninguno y
   descartaba en silencio cualquier otro `VOICE_SERVER_UPDATE`: la llamada
   esperaba 15 s y terminaba con «Discord no asignó un servidor de voz a
   tiempo». El setter `endpoint` del cliente oficial conserva el puerto
   (`` (80===i||443===i)&&(e=`${th}//${t}`) ``: sólo `:80` se cambia por 443) y
   Serein también. Como el servidor asignado puede cambiar en cada intento,
   es la explicación más probable del «a veces sí» (no confirmada: falta el
   log de un fallo). La misma regla rechazaba la transmisión
   propia con «Discord envió una transmisión inválida.»; al terminar la
   transmisión la interfaz suelta la captura y RUNE cierra la sesión del
   portal: por eso xdg-desktop-portal-hyprland registra
   `SHAREDATA returned selection 0` → `Session destroyed` sin `Start`. No
   tiene que ver con elegir monitor o ventana, ni con el tamaño o el
   `restore_token`. También: el token de stream tenía tope de 512 bytes
   (Serein admite 2048) y un `STREAM_CREATE` sin `rtc_server_id` terminaba la
   transmisión (el oficial lo ignora y espera).
2. **Qué sesión es de este dispositivo.** Ver *Otro dispositivo* arriba:
   la primera sesión que llegaba se tomaba como propia y una distinta
   después cortaba la llamada.
3. **Reunirse.** Ver *Reunirse* arriba: antes esperaba un estado nuevo que
   Discord puede no mandar; el oficial nunca se rinde (resume hasta 4 veces
   con espera 1–5 s y después identifica de nuevo con 1–10 s, sin límite) y
   en cada corte manda al Gateway principal op 5 `VOICE_SERVER_PING`; RUNE
   no manda ese op 5 (efecto en el servidor no verificable estáticamente) y
   mantiene un presupuesto de 3 reuniones.
4. **Robustez del apretón de manos.** IP discovery con reintento cada
   segundo (el módulo nativo reintenta a 1, 2 y 3 s); un ACK tarde una vez
   no corta; opcodes desconocidos se ignoran; Identify y resume llevan
   `channel_id` como el oficial; op 5 del SSRC tras un resume, como Serein.
5. **«Reconectando» pegado.** Un corte breve del Gateway principal ponía
   la llamada en «Reconectando la voz…» y nunca volvía aunque la voz
   siguiera; ahora vuelve a la fase que dijo el proveedor
   (`Store::call_reported`). Antes, ese corte no dejaba nada en stderr.

**Diferencias que quedan, a propósito.** RUNE usa `?v=8` (como Serein y
discord.js); el cliente oficial usa `?v=9` (y `?v=10` con un experimento).
No manda `flags` (clips, grabación) ni `preferred_regions` en el op 4, ni
`rtc_connection_id`/`experiments` en Select Protocol. Prefiere
`aead_xchacha20_poly1305_rtpsize`; el oficial elige
`aead_aes256_gcm_rtpsize` cuando la CPU tiene AES (RUNE no lo implementa).

**La cuenta dos veces en el canal.** Por el código no es un duplicado de
RUNE: `Store` guarda un estado por persona, `people()` sólo agrega la
cuenta si no está ya por su id, y el escenario ordena la cuenta primero.
En la captura la primera es la cuenta (nombre con letras cursivas Unicode,
el mismo de la barra inferior) y la otra, «Nihuel» sin cursiva, tiene que
ser otra cuenta (otro id) en ese canal, por ejemplo una segunda cuenta en
el cliente oficial para la prueba. Para confirmarlo: con RUNE solo en el
canal, `CLIENTS_CONNECT (op 11)` no aparece; si aparece «1 more», hay otra
persona.

**Qué mandar si vuelve a fallar.** Correr RUNE desde una terminal y, tras
el fallo, copiar todas las líneas que empiezan con `rune: voice` y
`rune: gateway` (son seguras de compartir: no tienen credenciales ni la IP
propia), más el texto del aviso. Con eso se ve en qué paso se detuvo y por
qué: el último `VOICE_SERVER_UPDATE` (host y si se aceptó), el último
`closed by Discord: code …`, el `timed out … waiting for …` o el
`lost: …`/`ended: …` final. Para la pantalla compartida, las líneas
`rune: voice share:` y `rune: gateway: op 18 sent`. Mientras se transmite,
cada 5 s salen `rune: voice share: video …` (lo que entrega PipeWire y hace
el codificador) y `rune: voice share: network …` (lo que sale, la pérdida y
la tasa): con un par de cada una se ve si falta fluidez y en qué etapa. Op
18 sale recién cuando la persona eligió la pantalla en el diálogo del
sistema y llegan sus cuadros, como en el cliente oficial (2026-10-17): un
diálogo cancelado no pide nada a Discord.

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

### Mirar la pantalla de otra persona (Go Live, 2026-10-16)

Fase 1 de [VIDEO.md](VIDEO.md), con lo confirmado por REA
([REA.md](REA.md#video-go-live-cámaras-y-transporte-de-video-2026-10-16)) y
lo que Serein usa. **Probado sólo con fixtures locales; no se miró ningún
stream real.** `Capabilities::watch_streams` lo declara este proveedor.

- **Gateway principal.** `VoiceWant` suma `stream` (de quién) y
  `stream_request`. Con el mismo patrón de último deseo que el op 4, el
  Gateway manda primero el op 4 si cambió y después op 20 `{stream_key}`
  para mirar, op 19 para soltar (también al colgar) y op 21 para pedir de
  nuevo el mismo stream. La clave es `guild:{servidor}:{canal}:{dueño}`,
  armada con ids validados y sólo del canal de la llamada propia. El stream
  se pide recién cuando se conoce la sesión de voz de esta conexión; tras un
  `READY` nuevo eso hace que se pida otra vez después del op 4.
- **Despachos.** Sólo los de la clave mirada: `STREAM_CREATE`
  (`rtc_server_id` numérico, `rtc_channel_id` o `rtc_server_id − 1` como el
  cliente oficial, `paused`; sin `rtc_server_id` todavía, espera, como el
  oficial), `STREAM_SERVER_UPDATE` (token de hasta 2048 bytes imprimibles
  como Serein, endpoint `*.discord.media` con su puerto, igual que la voz;
  endpoint nulo espera otro), `STREAM_UPDATE`
  (pausa) y `STREAM_DELETE`: con `unavailable` pasa a «Reconectando», suelta
  la conexión y manda op 21; si no, termina con el motivo en español
  (`stream_full`, `unauthorized`, `safety_guild_rate_limited`, otro) o en
  silencio con `user_requested`. Sin servidor en 15 s, termina.
- **Conexión del stream.** El mismo `voice::connection::run` con un
  `VideoPort`: `server_id = rtc_server_id`, la sesión de voz de la llamada,
  el token del stream y DAVE con el canal `rtc_channel_id`. Op 0 con
  `video: true` y `streams: []` (quien mira no anuncia streams, como Serein;
  el cliente oficial manda `[{type: "screen"…}]`, ver VIDEO.md). Op 1 con
  `codecs` Opus 120 y H264 101/RTX 102, `encode: false`, `decode: true`. No
  toma el micrófono. Op 12 asocia `video_ssrc` y los `streams` de tipo
  `video`/`screen` a la persona; cero los quita. Op 15 `{any: 100}` tras la
  Session Description y con cada stream nuevo. RTP 101 se arma con
  `voice::h264` (unidad simple, STAP-A, FU-A; 2048 paquetes, 2 MiB, 500 ms),
  se descifra con `davey` `MediaType::VIDEO` y va al host como unidad Annex
  B; RTX 102 se descarta (sin NACK sólo llega relleno). Una pérdida, un
  descifrado fallido o la cola del host llena esperan un cuadro clave y
  mandan un PLI (RTCP PSFB 1 sellado como `rtpsize` con los primeros 8 bytes
  en claro, como Serein; a lo sumo uno cada 500 ms por SSRC). El audio del
  stream (Opus del SSRC de audio de quien transmite) va al `VoicePort` con
  `stream: true` y el host lo mezcla como fuente aparte.
- **Fixtures** (`voice::tests`, `gateway_tests.rs`): ops 20/21/19 en orden
  tras el op 4; despachos `STREAM_*`; flujo completo de `Calls` (pedir,
  `STREAM_CREATE` + `STREAM_SERVER_UPDATE` → segunda conexión con
  `server_id = rtc_server_id` y el token del stream, `unavailable` → op 21,
  motivos, colgar suelta); y un servidor de voz sintético con un emisor que
  manda H264 codificado en la prueba con OpenH264, cifrado con DAVE (H264)
  y en `rtpsize`, en FU-A: el host recibe cada unidad igual a la codificada
  y OpenH264 la decodifica a 320×180; el sonido del stream llega marcado; el
  pedido de cuadro clave del host y la pérdida de un fragmento mandan un PLI
  que el servidor abre.

### Mirar las cámaras de la llamada (2026-10-16)

Fase 2 de [VIDEO.md](VIDEO.md#ajustes-de-la-fase-2-2026-10-16). **Probado
sólo con fixtures locales; no se vio ninguna cámara real.**
`Capabilities::watch_cameras` lo declara este proveedor.

- **Estado de voz.** `self_video` → `VoiceState::camera`
  (`#[serde(default)]`).
- **Conexión de la llamada.** Con el `VideoPort` del proveedor, la conexión
  de la llamada también se identifica con `video: true` y `streams: []` y su
  op 1 ofrece H264 101/102 para decodificar; sigue mandando el micrófono y
  su audio no es de stream. Op 12 asocia `video_ssrc` y los `streams` de
  tipo `video` a la persona, cero los quita, op 13 también. Sólo arma los
  paquetes de las cámaras que el host ve; las unidades van al host con
  `kind: Camera`.
- **Op 15 por SSRC.** `{"<ssrc>": 100}` para las cámaras que se ven y `0`
  para las demás conocidas, sin `any`; nada mientras no se conoce ninguna.
  Se manda tras la Session Description, con cada SSRC nueva, al cambiar lo
  que se ve, y cada 5 s (cada segundo si una fuente pedida lleva un segundo
  sin traer una unidad). La conexión del stream repite su `{any: 100}` con
  el mismo reloj.
- **Pedidos del host.** `voice::Calls` toma la cola de pedidos del
  `VideoPort` con la primera llamada y descarta lo pedido antes de cada
  una. Reparte: claves de pantalla a la conexión del stream y de cámara a la
  de la llamada (PLI acotado a uno cada 500 ms por SSRC); `Wants` de cámara
  a una lista de hasta 4 personas que sobrevive a las reconexiones de la
  misma llamada.
- **Fixtures** (`voice::tests`, `connection::endpoint_tests`): op 15 por
  SSRC (100, 0, vacío y `any` del stream); un servidor de voz sintético
  con un participante que manda una cámara codificada en la prueba con
  OpenH264, cifrada con DAVE (H264) y en `rtpsize`, en FU-A: fuera de
  pantalla no llega nada; al verla se pide su SSRC con 100, se repite al
  segundo sin imagen, y el host recibe la unidad igual a la codificada con
  `kind: Camera`, que OpenH264 decodifica a 320×180; apagada (op 12 con
  cero) no llega nada más. Otro arma `Calls` y comprueba el reparto de
  pedidos, el tope de 4 y que lo pedido antes de la llamada se descarta.
  `self_video` leído del `GUILD_CREATE`.

### Compartir la pantalla propia (Go Live, 2026-10-16)

Fase 3 de [VIDEO.md](VIDEO.md#ajustes-de-la-fase-3-2026-10-16). **Probado
sólo con fixtures locales; no se transmitió nada a Discord ni se hizo
ningún pedido nuevo al servicio.** `Capabilities::share_screen` lo declara
este proveedor; la cámara (`video`) sigue sin enviarse.

- **Gateway principal.** `VoiceWant` suma `share` (la cuenta, mientras
  comparte) y `share_request`. Tras el op 4, op 18 `{type: "guild",
  guild_id, channel_id, preferred_region: null}` crea el stream propio
  (recién cuando se conoce la sesión de voz, como el op 20); op 19 con la
  clave propia `guild:{servidor}:{canal}:{cuenta}` lo suelta (también al
  colgar), y op 21 con esa clave lo pide de nuevo tras `unavailable`.
- **Despachos.** `STREAM_CREATE`, `STREAM_SERVER_UPDATE` y `STREAM_DELETE`
  se reparten por clave: la propia va al stream propio con las mismas
  validaciones que el mirado. `STREAM_DELETE` termina con el motivo en
  español (`unauthorized`: «Discord no te dejó transmitir en este canal.»)
  o en silencio con `user_requested`. Nada se reintenta con otra calidad.
- **Conexión que envía.** El mismo `voice::connection::run` con
  `Video::send`: `server_id = rtc_server_id`, la sesión de voz de la
  llamada, el token del stream y DAVE con `rtc_channel_id`. Op 0 con
  `video: true` y `streams: [{type: "screen", rid: "100", quality: 100}]`;
  Ready trae el SSRC de la pantalla en `streams` (RTX = SSRC + 1 si no lo
  nombra; sin uno, la transmisión termina). Op 1 con H264 101/102
  `encode: true`, `decode: false`; una Session Description con otro
  `video_codec` termina la transmisión. Tras el op 5 y el key package, op 12
  `{audio_ssrc, video_ssrc, rtx_ssrc, streams: [{type: "screen", rid: "100",
  ssrc, rtx_ssrc, active: true, quality: 100, max_bitrate, max_framerate,
  max_resolution: {type: "fixed", width, height}}]}` con los valores de la
  calidad elegida. No manda op 15 ni toma el micrófono.
- **Cada unidad del host** (Annex B, de `VideoLink::outgoing`): se toma
  sólo cuando la anterior salió entera; sin grupo DAVE listo (nadie mira
  todavía) se descarta y la próxima que sale es un cuadro clave, pedido al
  host (`Feedback::KeyFrame`, a lo sumo cada 500 ms). El SPS se normaliza
  (`voice::sps`: VUI con `bitstream_restriction`, `max_num_reorder_frames =
  0`, `max_dec_frame_buffering = max_num_ref_frames`), la unidad se cifra con
  `davey` `MediaType::VIDEO` / `Codec::H264`, se empaqueta (`voice::h264`:
  STAP-A y FU-A, carga ≤ 1168 bytes, datagrama ≤ 1200, marcador en el
  último) con reloj de 90 kHz desde el momento de captura, y sale en lotes
  cada 2 ms con un presupuesto de 2,5 veces la tasa (ráfaga de 4800 bytes).
- **RTCP entrante.** Sólo esta conexión lo abre (`rtpsize`, 8 bytes en
  claro y autenticados): PLI y FIR a su SSRC piden un cuadro clave al host;
  la fracción perdida de SR/RR y REMB mueven la tasa una vez por segundo
  (`voice::send::Rate`, desde el 2026-10-17: empieza en el máximo de la
  calidad, como Serein, entre 250 kbit/s y ese máximo; con > 10 % de
  pérdida baja la mitad de la fracción perdida, entre 2 y 10 % se queda,
  con < 2 % sube 8 % tras 2 s tranquilos, nunca sobre un REMB menor, quieta
  sin reportes; antes empezaba en 600 kbit/s y bajaba 20 % desde 5 %, ver
  [PERFORMANCE.md](PERFORMANCE.md#fluidez-de-la-pantalla-compartida-2026-10-17-rama-audio))
  y se la dice al host (`Feedback::Bitrate`), que la pasa al codificador en
  marcha, sin cuadro clave. NACK se ignora: sin RTX al enviar (fase 5).
- **Fixtures** (`voice::tests`, `gateway_tests.rs`, `voice::{h264, sps,
  rtp, send}`): ops 18/21/19 tras el op 4; `Calls` pide, conecta como
  emisor con el servidor y el token del stream, suelta la conexión (y la
  cola del host) con `STREAM_DELETE`, «Dejar de compartir», colgar y el fin
  de la sesión; un servidor de voz sintético recibe lo que RUNE manda: op 0,
  op 1, op 12 exactos; un delta antes de la clave sólo pide clave; la clave
  (con un SPS sin VUI) y un delta llegan en RTP ≤ 1200 bytes, en orden, con
  33 ms a 90 kHz entre ellas; el espectador sintético (Bob) los descifra con
  su grupo DAVE, el SPS llega normalizado y OpenH264 decodifica 320×180; un
  PLI sellado pide clave al host y un RR con 50 % de pérdida baja la tasa a
  480 kbit/s; abortar la conexión libera la cola. Empaquetador ida y vuelta
  por el depaquetizador (STAP-A, FU-A, límites exactos), SPS con y sin VUI
  y con reordenamiento, RTCP abierto y leído, tasa y ritmo.

### Extras de video (fase 5, 2026-10-17)

Fase 5 de [VIDEO.md](VIDEO.md#ajustes-de-la-fase-5-2026-10-17). **Probado
sólo con fixtures locales; no se transmitió nada a Discord ni se hizo
ningún pedido nuevo al servicio.**

- **RTX y NACK.** La conexión que envía (pantalla o cámara) guarda lo que
  salió (`voice::nack::History`: 1 s, 2048 paquetes, 2 MiB) y contesta un
  NACK genérico (PT 205, FMT 1) a su SSRC reenviando por la SSRC de RTX
  (PT 102, la secuencia original primero; como Serein); un paquete ya no
  guardado pide un cuadro clave al host. Al recibir, un hueco de secuencia
  retiene lo que sigue y pide lo que falta con un NACK sellado en
  `rtpsize` como el PLI (8 bytes en claro); la retransmisión se restaura
  por la SSRC de RTX de esa persona (`rtx_ssrc` de op 12, o la siguiente).
  Serein no manda NACK; el cliente oficial sí (REA).
- **Pausa.** Op 22 `{stream_key, paused}` por el Gateway principal cuando
  la cuenta pausa o reanuda su stream (`Command::PauseStream`, sólo una vez
  creado); la fase llega en el acto como `StreamPhase::Paused`/`Live`.
  Serein no usa op 22; su forma sale de la REA.
- **Varias transmisiones.** `VoiceWant::streams` lleva hasta 3 dueños con
  su pedido; el Gateway manda op 19 para los que se sueltan y op 20 u op 21
  para cada uno, sin soltar a los demás. Los despachos `STREAM_*` se
  reparten por clave a cada conexión de stream.
- **Vista previa.** `GET /streams/guild:{servidor}:{canal}:{dueño}/preview`
  (la ruta que usa Serein), uno a la vez en la cola REST, sólo de un canal
  de voz del servidor nombrado; la respuesta `{url}` se acepta sólo de
  `cdn.discordapp.com` o `media.discordapp.net` bajo `streams/{clave}/` con
  un nombre hexadecimal de 16 a 128 caracteres, y se baja por el servicio de
  imágenes sin credenciales. Un 429 retiene todas las rutas su
  `retry_after`, como cualquier otra; sin imagen no hay aviso.
  `Capabilities::stream_previews`. Subirla no: Serein no lo hace.
- **Sonido de la pantalla.** La conexión que comparte toma las tramas Opus
  estéreo de `VideoLink::sound` en vez del micrófono y las manda por su
  SSRC de audio con op 5 `speaking: 2` (SOUNDSHARE), cifradas con DAVE.
- **Simulcast al recibir.** Con dos capas de una cámara en op 12 (`rid`
  "100" y "50" con su `quality`), op 15 pide 100 para una sola (la mejor con
  hasta dos cámaras a la vista, la más liviana pasado eso) y 0 para la otra,
  que se descarta antes de armarla.
- **Fixtures** (`voice::tests`, `gateway_tests.rs`, `tests::rest`,
  `voice::{nack, rtp, send}`): una pieza perdida del stream mirado se pide
  con NACK y la retransmisión por la SSRC 24 completa el cuadro que llega
  entero; sin retransmisión, a los 200 ms se pide clave; un NACK al que
  comparte devuelve el paquete por la SSRC 34 con la secuencia original y la
  misma carga, y uno de algo ya no guardado pide clave; en pausa la
  conexión no toma unidades y al reanudar pide clave; op 22 pausa y reanuda;
  cuatro transmisiones dejan las tres más nuevas y cada `STREAM_DELETE`
  termina la suya; op 20 para dos a la vez y op 19 sólo para la que se
  suelta; la vista previa es un GET bajo su clave, URLs de otro origen, de
  otra clave, con `..` o un hash corto no se aceptan, un 404 no da imagen,
  un 429 retiene el siguiente pedido y un 401 termina la sesión; el sonido
  de la pantalla sale con op 5 SOUNDSHARE y Bob lo descifra; una cámara en
  dos capas pide sólo la mejor y la otra no llega al host.

## Perfil completo y lista de miembros (2026-10-16)

Con la ampliación del 2026-10-16 ([SECURITY.md](SECURITY.md)) el adaptador
usa dos mecanismos no documentados que Serein ya usa. Son **no oficiales**,
están probados sólo con fixtures locales (HTTP y WebSocket en loopback) y
**no se probaron contra Discord**. No cambian user agent ni propiedades de
Identify, no tocan CAPTCHAs y no reintentan solos.

**Perfil completo.** `Command::LoadProfile` es un único
`GET /users/{id}/profile?with_mutual_guilds=…&with_mutual_friends=…&with_mutual_friends_count=false[&guild_id=…]`
por la cola REST existente (16 entradas, una solicitud a la vez, misma
espera global tras `429`/`x-ratelimit-remaining: 0`, misma clasificación de
rechazos; `401` cierra la sesión). Los datos en común se piden sólo sobre
otra persona. Respuesta acotada a 256 KiB. Se valida que `user.id`,
`guild_member.user.id` y `guild_member_profile.guild_id` sean los pedidos;
si no, la respuesta se rechaza. Se leen biografía y pronombres (de
`user_profile`, si no de `user`), banner y color, apodo, biografía,
pronombres y banner del servidor (ruta CDN documentada de banner de
miembro) y `joined_at`. Textos recortados (biografía 1024, pronombres 64,
apodo 128 caracteres). Listas en común: ausente o `null` = desconocido,
`[]` = ninguno; más de 100 o un amigo malformado = desconocido. Una lectura
nueva reemplaza la que estaba en cola o en curso (una tarjeta a la vez) y
el `Store` sigue aceptando sólo la respuesta del pedido vigente, persona y
alcance. Un fallo emite `Failure::ProfileLoad` sin toast: la tarjeta queda
con «No disponible». Caché: los 32 perfiles del `Store`, que vencen al
minuto (`PROFILE_FRESH`): uno más viejo se muestra mientras se vuelve a pedir
al abrir su tarjeta, y se vacían al cerrar sesión. Con los efectos de perfil
ocultos (`LoadProfile { effect: false }`) no se pide el producto del efecto.

**Lista de miembros.** `Command::WatchMemberList { server, channel, first,
last }` (y `UnwatchMemberList`) eligen qué filas suscribir: la página de 100
filas visible, o las dos páginas adyacentes que terminan en la última
visible. El nombre de la lista es `everyone` si todos ven el canal; si no,
MurmurHash3 (32 bits, semilla 0) de las sobrescrituras de ver ordenadas, como
Serein; antes del primer `SYNC` se acepta el nombre de un `SYNC` poblado del
mismo servidor que no sea de las últimas 8 listas dejadas. El Gateway envía
op 37 (`subscriptions` con `typing`, `activities`, `channels: {canal:
rangos}`): al abrir un servidor primero `channels: {}`, luego los rangos; un
cambio de rangos de la misma lista manda sólo los rangos; cambiar de lista o
de servidor manda antes `typing: false, channels: {}` al anterior. Ritmo: el
último pedido gana (`watch` de Tokio), como máximo 20 tramas por minuto y
500 ms entre cambios, muy por debajo del presupuesto de escritura de 100.
Tras un `READY` nuevo se vuelve a suscribir (prueba de extremo a extremo
contra un Gateway en loopback desde 2026-10-17, ver abajo).

**Reintento por silencio (2026-10-17).** Cuando se abre una lista y no
llega ningún `SYNC` de ese servidor, el Gateway la vuelve a mandar desde
cero (las tramas de apertura y de rangos) a los 15 s, y después a los 30,
60 y 120 s, como Serein (`discord-gateway/src/lib.rs`, `retry_delay`). El
primer `GUILD_MEMBER_LIST_UPDATE` con `SYNC` del servidor vigilado detiene
la espera y reinicia la cuenta; abrir otra lista, o un `READY` nuevo, la
reinicia también. Los reenvíos pasan por el mismo ritmo de 20 tramas por
minuto. Prueba: `gateway_tests.rs`
(`a_member_list_is_sent_again_without_a_sync_and_after_every_new_ready`):
abre la lista tras `READY`, sin `SYNC` la reenvía, con `SYNC` deja de
hacerlo, y tras un op 9 no reanudable y un `READY` nuevo la suscribe otra
vez. En modo de prueba la espera es de 300 ms.

`GUILD_MEMBER_LIST_UPDATE` aplica `SYNC`, `INSERT`, `UPDATE`, `DELETE` e
`INVALIDATE` sobre una copia: hasta 1024 operaciones, 514 grupos, 100
elementos por `SYNC` e índices hasta 10 000 000; algo malformado descarta esa
actualización y conserva la anterior, sin cerrar la sesión. Un elemento
ilegible es un hueco. Se guardan sólo las filas de las páginas suscritas,
sin repetir a nadie. Cada persona llega como `UserChanged`/`MemberChanged`
y, si trae presencia, `PresenceChanged` (estado, estado personalizado y
primera actividad; hasta 16 actividades); la cuenta propia no se toca.
Después llega `MemberListChanged` con grupos, cuentas y filas. `PRESENCE_UPDATE`
del servidor se aplica sólo a quien está en las filas cargadas.
`Capabilities::member_list` y `profiles` quedan habilitadas (y `presence`
desde el 2026-10-17, ver [Cuenta sincronizada](#cuenta-sincronizada-2026-10-17-rama-cuenta-sincronizada)).



Se estudió el clon de Serein en
`d1af883c9d7dda90d1261a2d15fc852a5445204d`, sin ejecutar su login ni copiar
código o activos. Su `docs/authentication.md:54` distingue explícitamente el
handoff de OAuth2; la validación de identidad y el Gateway son etapas
separadas. Detalle y referencias de código en
[SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md#revisión-enfocada-cómo-inicia-sesión-y-qué-cambia-para-rune-2026-10-14).

## Editar el perfil propio (2026-10-17)

Paridad con Serein (que edita nombre global, biografía, pronombres, color y
avatar por `PATCH /users/@me`) más lo que el cliente web oficial hace por
sus propias rutas (REA estática del bundle público ya descargado,
`web.c5b338cc7e610833.js`, SHA-256
`6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec`):

| Qué | Ruta y cuerpo | Fuente |
| --- | --- | --- |
| Nombre global y avatar | `PATCH /users/@me` `{global_name, avatar}` | Serein `discord-api/src/profile_edit.rs`; bundle: `Endpoints.ME` con `global_name`, `avatar` |
| Biografía, pronombres, color, banner | `PATCH /users/@me/profile` `{bio, pronouns, accent_color, banner}` | bundle: `USER_PROFILE(ME)` y `USER_PROFILE_UPDATE_SUCCESS` con esos campos |
| Apodo y avatar en un servidor | `PATCH /guilds/{g}/members/@me` `{nick, avatar}` | bundle: `SET_GUILD_MEMBER` |
| Biografía y banner en un servidor | `PATCH /guilds/{g}/profile/@me` `{bio, banner}` | bundle: `USER_GUILD_PROFILE(g, ME)` |

Las imágenes viajan como `data:image/png;base64,…`, o
`data:image/gif;base64,…` si se mueven (ver «Imágenes animadas y encuadre»
abajo): el host las elige con el selector de archivos del sistema
(`rune-platform`), `rune-core` las decodifica con los límites de los íconos
de grupo (8 MB, 8192 px) y las recorta donde la persona eligió (`Crop`) a
512×512 (avatar) o 960×384 (banner, 5:2), con tamaños menores si el PNG
pasa de 1 MiB. `null` quita la imagen; `""` vacía biografía
y pronombres. Cada escritura es un intento, sin reintentos; como Serein,
después se lee el perfil (`GET /users/{yo}/profile`, con `guild_id` en un
servidor) y eso es lo que se informa: `UserChanged` y `ProfileChanged`, o
`MemberChanged` y `ProfileChanged`. Si la lectura falla, el cambio llega
igual pero con un `Failed` ambiguo («Se guardó, pero no se pudo confirmar
cómo quedó»). Se valida que el usuario y el miembro de la respuesta sean la
cuenta; textos acotados (nombre y apodo 32, biografía 190, pronombres 40
caracteres) antes de mandar nada.

- **Lo que no se hace:** cambiar el nombre de usuario, el correo o la
  contraseña: el cliente oficial los manda con `password` y RUNE no pide
  contraseñas. La UI los muestra deshabilitados con ese motivo. Tampoco se
  manda la cabecera de MD5 de la imagen que usa el cliente oficial, ni
  decoraciones, efectos, placas o colores de nombre.
- **hCaptcha:** si Discord pide un desafío en estas rutas, la persona lo
  resuelve en la ventana aislada y el `PATCH` frenado se repite una vez (ver
  [Verificación](#verificación-en-cualquier-escritura-2026-10-17)). Si lo
  pide el segundo `PATCH`, el reintento rehace el primero (es el mismo) y
  la respuesta va con el segundo.
- **Nitro (2026-10-18, rama `edicion-de-perfil`):** `premium_type` da
  también `Perks::{animated_avatar, profile_banner, server_profiles}` como el
  catálogo del cliente oficial (REA de `web.c5b338cc7e610833.js`): avatar
  animado con Classic (1) y Nitro (2); banner y perfil por servidor (avatar,
  banner, biografía; no el apodo) sólo con Nitro; Basic (3), ninguno. La UI
  lo dice y no lo manda antes de que Discord lo rechace
  (`Store::profile_abilities`). Un `READY` sin `premium_type` cuenta como
  sin plan (0) y lo niega; un `USER_UPDATE` sin el campo no cambia el plan.
  Sólo si el plan todavía no llegó responde el servicio.
- **Imágenes animadas y encuadre (2026-10-18):** la imagen se recorta donde
  la persona eligió (`Crop`); un GIF o WebP animado viaja como
  `data:image/gif;base64,…` con el mismo recorte en cada cuadro, siempre
  hecho de nuevo en RUNE (no sale nada del archivo elegido: comentarios,
  autores, programas), hasta 8 MiB; sin el plan, o si pasa de 1000 cuadros,
  20 s por tamaño o 8 MiB en todos los tamaños, el primer cuadro en PNG y el
  editor lo dice. El ícono del servidor (`admin/rest.rs`) usa el mismo
  `mime()`. Sólo fixtures.
- **Permisos y restricciones:** `profiles::edit_allowed` exige que la cuenta
  siga en el servidor y no esté restringida ahí (aislada, con verificación
  pendiente o en cuarentena); el apodo, además, `CHANGE_NICKNAME` (vía
  `Store::profile_abilities`).
- **Capacidades:** `edit_profile`, `profile_images` y `server_profiles`.

**Avatar por servidor y cuarentena por etiqueta.** Los miembros (`READY`,
`READY_SUPPLEMENTAL`, `GUILD_MEMBER_ADD/UPDATE`, la lista por suscripción y
la respuesta de `PATCH …/members/@me`) traen `avatar`: se arma la ruta CDN
`guilds/{g}/users/{u}/avatars/{hash}.png` (`GUILD_MEMBER_AVATAR` en el
bundle) con la misma validación de hash. El bit 10 de `flags`
(`AUTOMOD_QUARANTINED_SERVER_TAG`, nombre del bundle; el bit 7 es
`AUTOMOD_QUARANTINED_USERNAME_OR_GUILD_NICKNAME` y el 8
`AUTOMOD_QUARANTINED_BIO`) pasa a `MemberRestrictions.tag_quarantined`, que
restringe como la otra cuarentena con su propio motivo.

Pruebas con fixtures en loopback (`tests/profile_edit.rs`): rutas, cuerpos y
lectura posterior de la cuenta y del servidor; desafío informado; lectura
fallida como cambio no confirmado; textos largos que no salen; admisión
fuera del servidor o con restricción. **Nada de esto se probó contra
Discord.**

## Amigos y conversaciones fuera de servidores (2026-10-17)

Regla del dueño: «si lo usa Serein, vamos con ello». Las rutas salen de
`references/serein-current` (`crates/discord-api/src/user_actions.rs` y
`group_actions.rs`, leídos el 2026-10-17) y se reimplementaron sin copiar
código. **Sólo fixtures en loopback (HTTP y Gateway locales); nada se probó
contra Discord.** Van por la cola REST de los mensajes (16 entradas, una
solicitud a la vez, misma espera global tras `429`), con un solo intento:
un `5xx`, un corte o una respuesta ilegible quedan como resultado ambiguo y
no se repiten. No cambian user agent ni Identify.

| Acción | Ruta | Qué confirma el cambio |
| --- | --- | --- |
| Enviar solicitud | `POST /users/@me/relationships` `{"username", "discriminator": null}` | `RELATIONSHIP_ADD` |
| Aceptar | `PUT /users/@me/relationships/{id}` `{}` | `RELATIONSHIP_ADD`/`UPDATE` |
| Ignorar, cancelar, eliminar, desbloquear | `DELETE /users/@me/relationships/{id}` | `RELATIONSHIP_REMOVE` |
| Bloquear | `PUT /users/@me/relationships/{id}` `{"type": 2}` | `RELATIONSHIP_ADD` (tipo 2) |
| Apodo de un amigo | `PATCH /users/@me/relationships/{id}` `{"nickname": texto o null}` | `RELATIONSHIP_UPDATE` |
| Abrir un privado | `POST /users/@me/channels` `{"recipient_id"}` | la respuesta (y `CHANNEL_CREATE`) |
| Renombrar un grupo | `PATCH /channels/{id}` `{"name"}` | la respuesta (y `CHANNEL_UPDATE`) |
| Imagen del grupo | `PATCH /channels/{id}` `{"icon": data URI PNG o null}` | la respuesta (y `CHANNEL_UPDATE`) |
| Abandonar un grupo, cerrar un privado | `DELETE /channels/{id}` | la respuesta (y `CHANNEL_DELETE`) |

Las relaciones no se informan desde la respuesta (`204`): las confirma el
Gateway, así que un evento repetido o tardío reemplaza en vez de duplicar.
Las de canal sí: la respuesta es el canal y actualiza la lista propia de la
sesión, para poder escribir en un privado recién abierto antes de que llegue
`CHANNEL_CREATE`; `Created` sale sólo si es el privado de esa persona (otra
respuesta queda ambigua). Un privado cerrado (por RUNE o por
`CHANNEL_DELETE`) se recuerda —hasta 64— y vuelve a la lista con
`ChannelChanged` antes del `MESSAGE_CREATE` que llegue en él, como el
cliente oficial lo muestra de nuevo; abrirlo otra vez devuelve el mismo
canal.

Antes de salir se valida: nombre de usuario como lo valida Serein (2 a 32
bytes, minúsculas, dígitos, `_` o `.`, sin `..`; RUNE lo pasa a minúsculas
y le quita la `@`), apodo de hasta 32 caracteres sin controles y sólo para
amigos, nombre de grupo de 1 a 100 caracteres sin controles (dejarlo sin
nombre no se ofrece: Serein no lo hace), imagen PNG de hasta 256 KiB y
1024 px por lado (la prepara `rune-core`), y que la persona o el canal sean
conocidos por la sesión (aceptar sólo una solicitud recibida, cerrar sólo un
privado, renombrar o abandonar sólo un grupo).

Rechazos con código JSON explicados en español (nombres de `AbortCodes`
del bundle web oficial, ver [REA](REA.md)): 10013 persona inexistente,
30002 máximo de amigos, 30004 grupo lleno, 40003 demasiados privados
nuevos, 50007 no recibe mensajes directos tuyos, 80001 no acepta
solicitudes, 80002 bot, 80003/50096 vos mismo, 80004 nombre de usuario
inexistente, 80007 ya son amigos, 340007 solicitudes limitadas. Un hCaptcha
en estas rutas lo resuelve la persona en la ventana aislada y la escritura
se repite una vez (ver
[Verificación](#verificación-en-cualquier-escritura-2026-10-17)).

**Grupos (2026-10-17, rama `social-2`).** Serein no crea grupos ni cambia
sus integrantes; el módulo de acciones de canales privados del bundle web
oficial (`web.c5b338cc7e610833.js`, inspección estática: `openPrivateChannel`,
`addRecipient`, `removeRecipient`, `setDMOwner`) da las rutas, y quedan
habilitadas con `create_groups`, `group_members` y la capacidad nueva
`group_owner` (`Command::SetGroupOwner`):

| Acción | Ruta | Qué confirma el cambio |
| --- | --- | --- |
| Crear un grupo | `POST /users/@me/channels` `{"recipients": [ids]}` | la respuesta (y `CHANNEL_CREATE`); `Created` sólo si es un grupo con esas personas |
| Añadir a alguien | `PUT /channels/{id}/recipients/{usuario}`, sin cuerpo | `CHANNEL_RECIPIENT_ADD` |
| Quitar a alguien | `DELETE /channels/{id}/recipients/{usuario}` | `CHANNEL_RECIPIENT_REMOVE` |
| Pasar la propiedad | `PATCH /channels/{id}` `{"owner": id}` | la respuesta (y `CHANNEL_UPDATE`), sólo si nombra al dueño pedido |

Antes de salir: de 2 a 9 amigos distintos para crear; añadir sólo a un amigo
que no esté y si queda lugar (10 personas con la cuenta); quitar y pasar la
propiedad sólo quien es dueño y a alguien del grupo que no sea la cuenta.
El cliente oficial, con un `201` al añadir a alguien a un privado, abre un
grupo nuevo: RUNE sólo añade a grupos y no lo usa. Discord no tiene mensaje
de sistema para el dueño nuevo: se ve en la corona.

**Apodo como título.** Como Serein (`conversation_name`) y el cliente
oficial, el apodo de un amigo titula su privado en la cabecera, la lista de
mensajes directos y el buscador de conversaciones (`Store::channel_title`).

**Lecturas.** `is_spam_request` (el cliente oficial lo guarda como
`isSpamRequest` y lo cuenta aparte) ya no se descarta: la solicitud llega
con `Relationship.spam` y la página la muestra en «Posible spam», sin
contarla como pendiente; una actualización sin el campo lo conserva. La
imagen del grupo sale de `icon` como
`https://cdn.discordapp.com/channel-icons/{canal}/{hash}.png?size=128`.
Presencia de amigos como la lee Serein: `presences` de READY,
`merged_presences.friends` de READY_SUPPLEMENTAL y `PRESENCE_UPDATE` sin
`guild_id`, sólo de amigos y de la otra persona de un privado; sin dato, un
amigo figura desconectado, como en el cliente oficial.
`Capabilities::friend_presence` queda habilitada; `presence`, desde el
2026-10-17 también, con el estado propio (ver
[Cuenta sincronizada](#cuenta-sincronizada-2026-10-17-rama-cuenta-sincronizada)). Los mensajes de sistema de grupo (tipos 1,
2, 4 y 5) ya se leían con su `mentions`; Discord no tiene un tipo para
«dueño nuevo» (el enum `MessageType` del bundle va de 0 a 69 sin uno), así
que el cambio se ve en la corona que pone `owner_id`.

Para probar contra el servicio (dueño, con una cuenta propia): enviar una
solicitud por nombre y cancelarla; aceptar e ignorar una recibida; poner y
quitar un apodo; bloquear y desbloquear; abrir un privado nuevo y escribir
enseguida; cerrarlo y ver que vuelve con un mensaje; renombrar un grupo,
cambiarle la imagen y abandonarlo; mirar «En línea» y la carpeta de spam.
Desde el 2026-10-17: crear un grupo con dos amigos, añadir y quitar a
alguien, pasarle la propiedad a otra persona (y ver la corona), y ponerle
un apodo a un amigo para ver que titula su privado.

## Adjuntos de salida (2026-10-17)

Por la ruta que usa Serein (`crates/discord-api/src/upload.rs` de
`references/serein-current`, leído el 2026-10-17), reimplementada en
`http/uploads.rs`. **Sólo fixtures en loopback; nada se subió a Discord.**

| Paso | Ruta | Detalle |
| --- | --- | --- |
| Destino de cada archivo | `POST /channels/{id}/attachments` `{"files": [{"id", "filename", "file_size", "is_clip": false}]}` | con la sesión; responde `{"attachments": [{"id", "upload_url", "upload_filename"}]}` |
| Bytes | `PUT {upload_url}` | sin la sesión ni cookies, `Content-Type` por extensión y `Content-Length`; sólo `https://discord-attachments-uploads-prd.storage.googleapis.com` (443, sin usuario ni fragmento) |
| Mensaje | `POST /channels/{id}/messages` `{"attachments": [{"id", "filename", "uploaded_filename"}], "nonce", "allowed_mentions", "content"?, "message_reference"?}` | un solo intento; la respuesta es el mensaje confirmado |

Core manda `Command::SendFiles` con los bytes congelados. El proveedor lo
admite sólo con la sesión conectada, la misma cuenta y generación, el canal
conocido con VER, ESCRIBIR y ADJUNTAR (más HISTORIAL si responde), hasta 10
archivos sin tokens repetidos y cada uno dentro del límite del canal; si no,
`Failed { SendFiles }` definitivo con el motivo de `UploadError`. Corre en un
lugar propio del bucle, al lado de la cola REST (una lectura o un mensaje de
texto no esperan una subida de minutos), una a la vez: otra recibe `Busy`.
El progreso (`UploadProgress`) cuenta bytes entregados a HTTP cada 256 KiB,
no recepción. Cancelar o vencer antes del `POST` del mensaje no envía nada;
desde ahí el fallo es ambiguo y se deja terminar para que la respuesta diga
qué pasó. El mensaje lleva el nonce de la sesión, así que la copia del
Gateway también lo confirma. Cerrar sesión, cambiar de cuenta o un `401`
descartan la subida. El cliente del almacenamiento no reintenta, no sigue
redirects y corta tras 30 s sin leer; el plazo total lo pone Core (el de un
mensaje más 1 s cada 128 KiB). Core confirma por orden y tamaño, no por
nombre: Discord puede reescribirlo.

Límites: el plan de la cuenta (`premium_type` del READY) da 500 MiB con
Nitro, 50 MiB con Classic o Basic y 20 MiB sin plan, como en Serein; las
mejoras del servidor (`premium_tier` 2 y 3) suben a 50 y 100 MiB en sus
canales, como muestra el cliente oficial. Se informan con
`Event::UploadLimit` (cuenta y servidores que la superan, y de nuevo con
`GUILD_CREATE`/`GUILD_UPDATE`); la UI rechaza al preparar un archivo que no
entra. RUNE además los recorta a su presupuesto en memoria (32 MiB). Los
privados permiten adjuntar. Si Discord pide hCaptcha al nombrar los
archivos o al enviar el mensaje, la persona lo resuelve y el envío se repite
una vez desde el principio (los archivos se vuelven a subir), con todo su
tiempo; ver [Verificación](#verificación-en-cualquier-escritura-2026-10-17).

**Publicaciones de foro con archivos (2026-10-17, rama `social-2`).** Como
Serein (`Destination::Post` de `upload.rs`): cada archivo se prepara en el
foro (`POST /channels/{foro}/attachments` y `PUT` al almacenamiento) y
`POST /channels/{foro}/threads` los nombra en `message.attachments`
(`{"id", "filename", "uploaded_filename"}`); hasta ese pedido no se publica
nada. Con archivos, el primer mensaje puede ir vacío. Se comprueba el
permiso `ATTACH_FILES` del foro y el techo de archivos y bytes. El pedido va
por la cola REST; los bytes los une a la orden el actor de subidas de Core,
así que la interfaz sólo maneja tokens. Desde el 2026-10-18
(envios-y-subidas) sus archivos informan el progreso como los de un mensaje,
con `Event::PostProgress { request }` (bytes entregados a HTTP cada 256 KiB),
y el diálogo muestra la barra mientras espera.

**Envíos y subidas (2026-10-18, rama `envios-y-subidas`). Sólo fixtures,
sin probar contra el servicio real.**

- Una respuesta con archivos lleva
  `allowed_mentions.replied_user` según el interruptor «@» del compositor,
  como una de texto (`Command::SendFiles.mention_reply`); antes iba siempre
  en `false`. Un reintento conserva la elección: es parte del cuerpo
  congelado en Core.
- Hasta 10 archivos por mensaje en toda la cadena (selector, rutas,
  arrastre, Core, interfaz y simulación), como Discord y Serein.
- Un `429` de `POST /channels/{c}/typing` retiene sólo el indicador de
  escritura, que tiene su propio bucket en Discord; los mensajes siguen
  saliendo. Si la respuesta dice que el límite es global
  (`X-RateLimit-Global: true`, `X-RateLimit-Scope: global` o
  `"global": true` en el cuerpo), retiene todo, como antes. Un `429` de
  cualquier otra ruta sigue reteniendo todas, también el indicador.

## Verificación en cualquier escritura (2026-10-17)

Decisión del dueño del 2026-10-17 (ver [SECURITY.md](SECURITY.md#ampliaciones-del-2026-10-17)).
**Sólo fixtures en loopback; nada se probó contra Discord.**

- **Cuándo.** Una escritura REST (`request_answer`: mensajes, stickers,
  ediciones, borrados, reacciones, encuestas, fijados, amigos, privados y
  grupos, hilos y publicaciones, escenarios, perfil) o una subida
  (`http/uploads.rs`) recibe el `400` con `captcha_key` y
  `captcha_service: "hcaptcha"` que lee `rejections::inspect`. Una lectura
  nunca se ofrece. Otro servicio de desafío o una verificación de cuenta
  siguen remitiendo a la aplicación oficial.
- **Qué pasa.** El proveedor guarda la escritura (`challenges::Challenges`,
  una a la vez) con el desafío y la ruta frenada (`"POST /channels/1/messages"`)
  y emite `Event::ChallengeRequired { challenge, failure }`. La escritura no
  falla todavía: Core corre su plazo (`CHALLENGE_LIFETIME`, cinco minutos,
  más su margen; una subida reinicia su progreso). La interfaz pregunta
  «Verificación requerida» con «Verificar» y «Cancelar»; nada se abre solo.
- **La ventana.** «Verificar» (`Command::SolveChallenge { solve: true }`)
  llama a `SessionLogin::verify`, la misma ventana aislada de las
  invitaciones (`rune-login`), con `sitekey`, `rqdata` e invisible; el
  `rqtoken` y el `captcha_session_id` quedan en el adaptador. Una sola
  ventana a la vez, contando la de una invitación.
- **El reintento.** Con la respuesta, la misma orden vuelve al frente de la
  cola y se revisa como la primera vez; el `Http` del reintento lleva la
  respuesta sólo para esa ruta y la entrega una vez, con `X-Captcha-Key` y,
  si vinieron, `X-Captcha-Rqtoken` y `X-Captcha-Session-Id` (las cabeceras
  del cliente oficial, ver [REA](REA.md), y las de Serein). Una edición de
  mensaje rehace su lectura previa y la respuesta va con el `PATCH`. Una
  subida se repite desde el principio con el tiempo que tenía al frenarse.
- **Cómo termina sin enviar.** Cancelar, cerrar la ventana, vencer los cinco
  minutos, perder la conexión o cambiar de cuenta terminan la acción con
  `Event::Failed` y un motivo («Cancelaste la verificación; no se envió
  nada.», etc.). Un segundo desafío en el reintento no se ofrece:
  «Discord volvió a pedir una verificación; la acción no se repite otra
  vez.» Otro desafío mientras uno espera falla con su motivo.
- **Invitaciones.** Conservan su flujo (`VerifyInvitation`, con sus plazos
  en Core) y comparten las cabeceras (`http::challenges::with_answer`).
- **Sin** resolución automática ni servicios externos. Hasta el 2026-10-19
  tampoco llevaba `X-Super-Properties`, número de build ni user agent de
  navegador; desde entonces el reintento presenta la
  [identidad de navegador](#identidad-de-navegador-2026-10-19-rama-identidad-de-navegador),
  igual que el intento. Las pruebas lo comprueban sobre lo que sale.
- **Simulación.** Una solicitud de amistad a `pablo` pide una verificación
  simulada: la pregunta dice «Verificación simulada», no se abre ninguna
  ventana, «Verificar» la manda una vez y «Cancelar» no manda nada.
  [Captura](img/challenge-2026-10-17.png).

Para probar contra el servicio (dueño): cuando Discord pida hCaptcha al
enviar un mensaje, una solicitud de amistad, una subida o una publicación,
resolverlo y ver que la acción sale una vez; cancelar y cerrar la ventana y
ver que no sale nada.

## Cuenta sincronizada (2026-10-17, rama `cuenta-sincronizada`)

Bloque P1 de [la paridad con Serein](../tasks/serein-parity.md): lo que la
cuenta guarda en Discord además del perfil. Las rutas salen de
`references/serein-current` (`discord-api/src/activity_sharing.rs`,
`guild_folders.rs`, `messaging_permissions.rs`, `server_actions.rs`,
`channel_actions.rs`, `user_actions.rs`; `discord-gateway/src/activity.rs` y
`lib.rs`; `discord-protocol/src/settings_update.rs` y `notifications.rs`,
leídos el 2026-10-17) y el esquema protobuf, del bundle web oficial ya
descargado (`web.c5b338cc7e610833.js`, REA estática:
`discord_protos.discord_users.v1.PreloadedUserSettings`, `StatusSettings`,
`CustomStatus`, `GuildFolders`, `GuildFolder`, `PrivacySettings`,
`TextAndImagesSettings`, `ExplicitContentSettings` y los enums
`DmSpamFilterV2` y `ExplicitContentRedaction`). Se reimplementó todo, sin
copiar código. **Sólo fixtures en loopback (HTTP y Gateway locales); nada
se probó contra Discord.**

| Qué | Cómo | Capacidad |
| --- | --- | --- |
| Estado y estado personalizado | `presence` en el Identify y op 3 (documentados); `GET`/`PATCH /users/@me/settings-proto/1`, campo 11 | `presence`, `custom_status` |
| Restaurarlo al iniciar | `GET settings-proto/1` después de cada READY | (las de arriba) |
| Estado de las otras sesiones | `SESSIONS_REPLACE` (la entrada `all`) | (las de arriba) |
| Notificaciones por servidor y canal | `user_guild_settings` del READY, `USER_GUILD_SETTINGS_UPDATE`, `PATCH /users/@me/guilds/{g\|@me}/settings` | `synced_notifications` |
| Carpetas y orden del riel | `settings-proto/1`, campo 14 (`GuildFolders`) | `server_folders` |
| Privacidad de mensajería | `settings-proto/1`, campos 8 y 6 | `messaging_privacy` |
| Notas privadas | `GET`/`PUT /users/@me/notes/{id}`, `USER_NOTE_UPDATE` | `notes` |

**`settings-proto/1`.** `settings_proto.rs` es un decodificador protobuf
mínimo propio (cinco subárboles de números fijos; una dependencia y un paso
de generación no se justificaban): varints, fixed64, mensajes y wrappers
acotados (128 KiB por subárbol, 6 MiB por respuesta, 8192 campos por
mensaje); un subárbol que no se entiende se rechaza entero. Cada escritura
lee primero, cambia sólo los campos que le tocan dentro de su subárbol,
conserva byte a byte los demás (incluso los desconocidos de cada carpeta) y
manda `required_data_version` con la versión leída, como Serein; lo que la
respuesta dice es lo que se informa, y si no coincide con lo pedido el
fallo es ambiguo («Discord no confirmó el cambio; se volvió a leer cómo
quedó»). `USER_SETTINGS_PROTO_UPDATE` del tipo 1 pide leerlo otra vez (no
se decodifica el parcial, como en Serein).

- **Estado.** El Identify lleva el estado sin actividades; mientras la
  cuenta no se leyó es «invisible», así una cuenta invisible nunca aparece
  en línea al conectar, y la lectura posterior al READY lo corrige. Cada
  cambio sale por op 3 en el acto (sólo el último de una ráfaga, a lo sumo
  uno cada 5 s) con el estado personalizado como actividad de tipo 4
  (`state` y `emoji.name`; nada si es invisible) y se guarda en la cuenta;
  si Discord no lo guarda, la sesión vuelve a lo que la cuenta tiene y el
  fallo lo dice. El vencimiento del estado personalizado va en
  `expires_at_ms` y lo quita el servicio; RUNE conserva además su propio
  temporizador. Los emojis de servidor en un estado personalizado no se
  muestran (sólo los Unicode). `SESSIONS_REPLACE` muestra el estado que ven
  los demás (por ejemplo, «No molestar» puesto en el teléfono), sin cambiar
  lo que esta sesión pide; las notificaciones del sistema ya callan en «No
  molestar».
- **Notificaciones.** Nivel (todos, menciones, nada, o el de arriba con
  `3`), silencio con vencimiento (`mute_config.end_time` ISO y
  `selected_time_window` en segundos; `-1` y sin fin para siempre; activar
  manda `mute_config: null` para que no quede silenciado), `@everyone`,
  roles y ocultar silenciados por servidor, y nivel y silencio por canal o
  categoría (`channel_overrides`). Los privados usan `@me`. La respuesta del
  `PATCH` confirma el cambio. Con `synced_notifications`, el riel, la lista
  de canales y las notificaciones del sistema leen la cuenta (`Store`) y no
  este equipo; sin la capacidad todo sigue local, como antes. Desde el
  2026-10-18 (rama `riel-y-notificaciones`) un servidor sin nivel propio
  toma el de sus miembros (`default_message_notifications`, que READY y
  `GUILD_CREATE` ya traían en `ServerOptions::mentions_only`), como Serein;
  y los `mention_roles` de cada mensaje se comparan con los roles de la
  cuenta: cuentan como mención y avisan salvo con «Silenciar las @menciones
  de roles». Sólo fixtures, sin probar contra el servicio real.
  [Captura](img/account-notifications-2026-10-17.png).
- **Carpetas.** Se leen normalizadas como el cliente oficial las muestra
  (un servidor repetido queda donde apareció primero, un grupo sin id se
  separa, una carpeta vacía o con id 0 o repetido se arregla). El riel
  muestra primero los servidores que la disposición no nombra, después la
  disposición; los servidores que la cuenta ya dejó siguen guardados pero no
  se ven. Escribir exige que Discord tenga todavía lo último que se mostró:
  si otro dispositivo lo cambió no se escribe nada, se avisa y se vuelve a
  leer. Se escriben arrastrando, como en Serein (desde el 2026-10-18), o
  desde el menú secundario del riel: mover arriba o abajo, meter en una
  carpeta nueva o existente, sacar de la carpeta; y el de la carpeta:
  nombre y color (diálogo propio), mover, desarmar. Abierta o cerrada se
  guarda en este equipo, por cuenta (`open_folders`).
  [Capturas](img/folders-2026-10-17.png),
  [cerrada](img/folders-closed-2026-10-17.png),
  [diálogo](img/folder-dialog-2026-10-17.png).
- **Privacidad.** `PrivacySettings`: servidores donde los miembros no
  pueden escribir (3) y el valor para los nuevos (4 y 27, se escriben los
  dos, como Serein), servidores sin filtro de solicitudes (16) y su valor
  (17), quién manda solicitudes de amistad (11: 2 amigos de amigos, 4
  miembros de servidores, 8 todos). `TextAndImagesSettings`: filtro de spam
  (27) y contenido explícito en servidores, de amigos y de otros (29). Lo
  explícito RUNE lo aplica desde `contenido-de-mensajes` (2026-10-18): ver
  [Contenido de mensajes](#contenido-de-mensajes-2026-10-18-rama-contenido-de-mensajes).
  En Ajustes → Datos y privacidad, con una lista por servidor.
  [Captura](img/messaging-privacy-2026-10-17.png).
- **Notas.** Se leen al abrir un perfil (una vez por persona y sesión); un
  `404` es una nota vacía, como en Serein. Enter en el campo «Nota» del
  perfil la guarda (hasta 256 caracteres; vacía la quita);
  `USER_NOTE_UPDATE` trae las de otros dispositivos.
- **hCaptcha.** Todas las escrituras van por la cola REST y el mecanismo
  común: si Discord lo pide, la persona lo resuelve y la escritura se repite
  una vez (un `settings-proto` vuelve a leer y la respuesta va con el
  `PATCH`). Ver [Verificación](#verificación-en-cualquier-escritura-2026-10-17).
- **Simulación.** `MockProvider` guarda todo en `mock/account.rs`, igual en
  cada corrida: «Comunidades» (Café Nocturno y Club de Fotografía), mensajes
  directos apagados en Club de Fotografía y una nota sobre Ada; lo manda
  después de `Ready` y responde cada cambio.
- **Pruebas.** `settings_proto.rs` (ida y vuelta de cada subárbol,
  disposiciones desordenadas, respuestas rotas), `account.rs` (filas del
  Gateway, admisión), `gateway_tests.rs` (Identify invisible, op 3 con
  ritmo) y `tests/session/account.rs` (el proveedor entero: estado en los
  dos caminos, `SESSIONS_REPLACE` y relectura, silencio de un privado,
  carpetas con cambio ajeno, privacidad, notas y un hCaptcha en
  `settings-proto`).

Para probar contra el servicio (dueño, con una cuenta propia): poner
«Invisible» y reiniciar RUNE (no tiene que aparecer en línea); poner un
estado personalizado con emoji y vencimiento y verlo en el cliente oficial;
cambiar el estado en el teléfono y ver que RUNE lo muestra; silenciar un
servidor y un canal por una hora y verlo en el cliente oficial; armar,
renombrar, colorear y desarmar una carpeta y mirar el orden en el cliente
oficial; cambiar cada ajuste de privacidad; escribir una nota y verla en el
otro cliente.

## Comandos e interacciones (2026-10-17, rama `comandos-e-interacciones`)

Bloque P3 de [la paridad con Serein](../tasks/serein-parity.md), habilitado
por el dueño el 2026-10-17 (reemplaza la decisión de «componentes sólo
visibles» del 2026-10-16). Rutas y comportamiento de
`references/serein-current` (`discord-api/src/interactions.rs`,
`discord-gateway/src/interactions.rs`, `client-core/src/interactions.rs` y
`application_commands.rs`, `ui/src/slash_builtin.rs` y `slash_commands.rs`,
leídos el 2026-10-17) y de una REA estática del cliente web oficial
(bundles públicos del build 641004, sin cookies ni sesión; ver
[REA.md](REA.md#cliente-web-oficial-interacciones-y-comandos-de-aplicación-2026-10-17)).
Se reimplementó todo, sin copiar código. **Sólo fixtures en loopback (HTTP y
Gateway locales); nada se probó contra Discord.**

| Qué | Cómo | Capacidad |
| --- | --- | --- |
| Índice de comandos | `GET /guilds/{g}/application-command-index`, o `/channels/{c}/application-command-index` en un privado con un bot | `app_commands` |
| Correr un comando | `POST /interactions` tipo 2 | `app_commands` |
| Sugerencias de una opción | `POST /interactions` tipo 4 y `APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE` | `app_commands` |
| Botones y menús | `POST /interactions` tipo 3 | `interactions` |
| Modales | `INTERACTION_MODAL_CREATE` y `POST /interactions` tipo 5 | `interactions` |
| Archivos de un modal (2026-10-18) | `POST /channels/{c}/attachments`, `PUT` al almacenamiento y `data.attachments` del tipo 5 | `interactions` y `attachments` |
| Respuesta y error | `INTERACTION_CREATE`, `INTERACTION_SUCCESS`, `INTERACTION_FAILURE` por nonce | (las de arriba) |
| Índice viejo | `GUILD_APPLICATION_COMMAND_INDEX_UPDATE`, y cada READY | `app_commands` |
| Efímeros | `MESSAGE_CREATE` con la marca 64 | — |

- **Sesión y nonce.** El Gateway entrega el `session_id` del READY en un
  evento interno propio, antes del READY y nunca en la instantánea; cada
  interacción lo lleva, con un nonce snowflake (milisegundos desde 2015 y el
  pedido en los bits bajos) que es lo único que asocia las respuestas del
  Gateway: lo de otra sesión o desconocido se descarta. Cada READY olvida
  las interacciones y el modal de la sesión anterior.
- **Índice.** Sólo comandos de barra (tipo 1), con el nombre y la
  descripción por defecto antes de la traducción (`name_default`), acotado
  (2000 comandos, 4 MiB, tres niveles de opciones, 25 por nivel y 25
  opciones fijas). Contextos (0 servidor, 1 privado con el bot),
  `guild_id` del comando, `nsfw` y `default_member_permissions` (con «0»
  sólo administración) van al modelo; los overrides de usuario y de roles
  (con los roles propios del snapshot; el de `@everyone` como respaldo) y
  de canales (el id del servidor menos uno para todos) se resuelven en el
  proveedor, como Serein: un `false` de la aplicación quita el permiso por
  defecto salvo que el comando diga otra cosa. El `Store` decide qué
  comandos ofrece en cada canal (los hilos siguen a su canal) y además
  exige `USE_APPLICATION_COMMANDS` (bit 31, ahora modelado) en servidores o
  un privado con un bot fuera de ellos, como Serein. La definición de cada
  comando se guarda sin sus permisos y viaja como `application_command`.
  Un `202` (índice en preparación) se informa como fallo y el próximo `/`
  lo pide de nuevo.
- **Tipo 2.** `version`, `id`, `name`, `type: 1`, `guild_id` sólo si el
  comando es de un servidor, opciones anidadas por grupo (2) y subcomando
  (1), cada valor con el tipo de su opción (ids como texto, números como
  números) y `attachments: []`; arriba, `guild_id` del canal y
  `analytics_location: "slash_ui"` como el cliente web.
- **Tipo 4.** El mismo `data`, con la opción que se escribe marcada
  `focused: true` y su texto tal cual (aun si es un número); sin
  `analytics_location`. Las opciones con `choices` fijas no piden
  sugerencias. Una sugerencia fallida no se informa: sólo no aparece.
- **Tipo 3.** `message_id`, `message_flags` del mensaje (guardadas como
  vinieron), `application_id` del mensaje o del bot que lo escribió (nunca
  un webhook ni una copia reenviada), `data.component_type`, `custom_id` y,
  en menús, `type` (el mismo número) y `values`. El proveedor revisa contra
  su caché que el control exista, no esté deshabilitado y que los valores
  sean de sus opciones y estén entre `min_values` y `max_values`.
- **Tipo 5.** `data.id` del `INTERACTION_MODAL_CREATE`, su `custom_id` y
  los componentes con la misma estructura que llegaron: fila (1) o etiqueta
  (18) alrededor del campo, texto (10) como `{type: 10}`, texto (4) con
  `value` o `null`, menús 3 y 5–8 con `values`, opción única (21) con
  `value`, varias (22) con `values`, casilla (23) con `value` booleano. Un
  modal de más de 10 partes, título de más de 45 caracteres o malformado se
  descarta; un campo que RUNE no completa se muestra y no deja enviarlo.
- **Archivos en un modal (19, 2026-10-18, rama `comandos-de-apps`).** Como
  Serein (`valid_uploads`, `valid_file_types` y `Destination::Interaction`
  de `upload.rs`) y la REA: el campo trae `min_values` (1 si no dice),
  `max_values` (1 a 10) y `required`. La persona elige archivos con el
  selector, aparte del borrador del compositor; Core los une a la orden
  (`Command::Interact::files`), como los de una publicación de foro. El
  proveedor exige ADJUNTAR en el canal, prepara cada archivo como los de un
  mensaje (`POST /channels/{c}/attachments` y `PUT` al almacenamiento, sin
  barra de progreso y dentro de la operación REST en curso: las demás
  esperan detrás) y recién entonces manda el tipo 5 con
  `data.attachments` (`{id, filename, uploaded_filename}`) y el campo como
  `{type: 19, custom_id, values}` con los índices de sus archivos. Cada
  archivo va una sola vez y nombrado por un campo; si no, no sale nada. Un
  fallo antes del `POST /interactions` no envió nada. Con archivos, la
  espera es de 2 min (la del cliente oficial) en vez de 30 s. Un
  rechazo deja los archivos preparados para otro intento; cerrar el
  modal o que la app lo conteste los suelta. Los tipos permitidos
  (`file_types`, como Serein: hasta 10, `.ext` o familia `image`, `video`,
  `audio`, comparados por extensión) se muestran y se comprueban al elegir,
  antes de preparar y en la simulación. Basta ver el canal y ADJUNTAR (no
  ESCRIBIR); como mucho los archivos y el tamaño de una publicación de foro
  y el `max_values` de cada campo. Los bytes viajan con la orden, no con lo
  que espera la respuesta. Sólo fixtures.
- **Respuestas.** `INTERACTION_CREATE` es «la aplicación lo recibió»
  (pasa de «Enviando…» a «está pensando…»), `INTERACTION_SUCCESS` termina,
  `INTERACTION_FAILURE` falla con su motivo (`reason_code` 2: no respondió
  a tiempo) y `INTERACTION_MODAL_CREATE` abre el modal; sin respuesta en
  30 s (como Serein; el cliente oficial no tiene reloj propio para las
  partes) RUNE deja de esperar y lo dice. Una interacción a la vez.
- **Quién corrió qué.** La respuesta de una app lee `interaction_metadata`
  (o el `interaction` anterior) de tipo 2 con su `name` y su usuario, y
  RUNE la muestra con «{persona} usó /{comando}» arriba, como una respuesta.
- **Efímeros.** Un mensaje con la marca 64 (con partes o sólo texto) se
  muestra sólo en la sesión, con «Sólo vos podés ver esto · Descartar»; no
  cuenta como no leído, no se guarda en SQLite (ni su edición) y descartarlo
  es local, como en el cliente oficial (16 a la vez como mucho).
- **hCaptcha.** Todas las interacciones pasan por la cola REST común: si
  Discord pide verificación, la persona la resuelve y la misma interacción
  (mismo nonce) sale una sola vez más; mientras la resuelve, la espera de
  30 s no corre.
- **Comandos locales.** `/shrug`, `/tableflip`, `/unflip`, `/me`,
  `/spoiler`, `/gif`, `/sticker` y `/msg`, como en Serein, se resuelven en
  la interfaz y funcionan con cualquier proveedor: no tocan esta sesión.
- **Pruebas.** `components.rs` (custom_id, menús 5–8 con valores por
  defecto, app y marcas), `interactions.rs` (modal de ida y vuelta, índice
  con overrides) y `tests/session/interactions.rs` (el proveedor entero:
  botón y menú con la sesión del Gateway, respuestas por nonce, fallo por
  tiempo, valor ajeno rechazado, efímero de texto, índice viejo, índice,
  sugerencias, comando, modal, archivos de un modal por índice y un
  hCaptcha con su único reintento).

Para probar contra el servicio (dueño, con una cuenta propia y un servidor
con un bot que tenga botones, menús, modales y comandos): pulsar un botón
y elegir en un menú (también de personas, roles o canales con valores por
defecto) y ver que el bot responde; abrir un modal y enviarlo; recibir una
respuesta efímera y descartarla; escribir `/`, elegir un comando con
subcomando y opciones, usar una opción con sugerencias del bot y enviarlo;
probar un comando sin permiso (no tiene que aparecer) y uno en un privado
con el bot; mirar que un error del bot se vea en su lugar. Desde el
2026-10-18: un modal con un campo de archivos (por ejemplo, el de un bot de
soporte o de tickets) con uno y con dos archivos, ver que el bot los
recibe, y que sin permiso de adjuntar el botón «Elegir archivos» no se
habilita.

## Presencia rica, identidad visual y expresiones (2026-10-17, rama `presencia-e-identidad`)

Bloque P5 de [la paridad con Serein](../tasks/serein-parity.md). Campos y
rutas de `references/serein-current` (`discord-protocol/src/presence.rs`,
`collectibles.rs`, `profile.rs`, `gifs.rs`, `stickers.rs`;
`client-core/src/permissions.rs` y `stickers.rs`; leídos el 2026-10-17) y
de la REA estática del bundle web oficial ya descargado
(`web.c5b338cc7e610833.js`: `GIFS_SEARCH`, `GIFS_TRENDING`,
`GIFS_TRENDING_GIFS`, `GIFS_SUGGEST` con `q`, `limit` y `locale`,
`GIFS_TRENDING_SEARCH`; `clan-badges/{guild}/{badge}.png`;
`badge-icons/{icon}.png`; `avatar-decoration-presets/{asset}.png`).
Reimplementado, sin copiar código ni recursos. **Sólo fixtures en loopback;
nada se probó contra Discord.**

| Qué | De dónde | Capacidad |
| --- | --- | --- |
| Actividades (hasta 4) y plataformas | `activities` y `client_status` de cada presencia (`PRESENCE_UPDATE`, listas de miembros, READY) | — (datos) |
| Decoración, nameplate, etiqueta | `avatar_decoration_data`, `collectibles.nameplate`, `primary_guild` (o `clan`) de cada usuario; los del miembro pisan los de la cuenta | — (datos) |
| Ícono, emoji y degradé de rol | `icon`, `unicode_emoji`, `colors.secondary_color`/`tertiary_color` de cada rol | — (datos) |
| Insignias y cuentas conectadas | `badges`, `guild_badges`, `connected_accounts` del perfil completo | `profiles` |
| Efecto de perfil | `collectibles` de tipo 1 (o `profile_effect`) del perfil y `GET /collectibles-products/{sku}` | `profiles` |
| Plan de la cuenta | `premium_type` del READY y de `USER_UPDATE` → `Event::Perks` | — (evento) |
| Paquetes estándar | `GET /sticker-packs` | `sticker_packs` |
| GIF del servicio | `/gifs/search`, `/gifs/trending-gifs` + `/gifs/trending`, `/gifs/suggest`, `/gifs/trending-search` | `gif_search` |

- **Actividades.** Tipos 0, 1, 2, 3 y 5 (el 4 es el estado personalizado,
  que sigue yendo aparte); nombre, `details`, `state`, inicio y fin
  (milisegundos, el fin sólo si es posterior al inicio), y las imágenes:
  un asset numérico de la aplicación como
  `cdn.discordapp.com/app-assets/{app}/{asset}.png?size=128` y uno `mp:`
  como `media.discordapp.net/{ruta}?format=png`, con la ruta validada
  (`external/` o `attachments/`, sin `..`, espacios, `?`, `#`, `%` ni `\`).
  Las portadas `spotify:{id}`, `twitch:{login}` y `youtube:{id}` se cargan
  desde sus hosts exactos desde el 2026-10-18 ([abajo](#portadas-nameplates-por-sku-y-cosméticos-2026-10-18-rama-perfil-tarjeta-y-cosmeticos);
  antes no se cargaban). Un juego repetido con el mismo
  nombre queda una vez. Botones, `party` y `sync_id` no se leen (Serein
  tampoco los muestra). `client_status` da escritorio, móvil y web.
- **Marcas.** Decoración: el `asset` debe ser un hash; `passthrough=true`
  sólo para uno `a_` (animado) y se cae si `expires_at` ya pasó. Nameplate:
  `assets/collectibles/{asset}static.png` con el `asset` validado
  (`nameplates/…/`), el color de su `palette` (pares propios de RUNE para
  cada nombre) y su `label`. Etiqueta: sólo con `identity_enabled`, de 1 a
  4 caracteres, con su insignia en `clan-badges/{servidor}/{hash}.png`.
  Una marca rota es ninguna: nunca hace fallar la cuenta.
- **Roles.** Ícono en `role-icons/{rol}/{hash}.png`, emoji Unicode acotado,
  y el degradé si hay color secundario (el terciario sólo con él).
- **Perfil completo.** Insignias de la cuenta y luego del servidor (hasta
  16; ícono si es un hash o un nombre simple; su enlace sólo si es HTTPS).
  Cuentas conectadas (hasta 16) con su etiqueta y su enlace como Serein
  (GitHub, Twitch, X, Reddit, Instagram, TikTok, Bluesky, eBay, Steam,
  Spotify, YouTube, Roblox, dominio y Mastodon), sólo con nombres seguros.
  Si lleva un efecto, se lee su producto (hasta 256 KiB): capas del
  `assets/content` o `media/v1/collectibles-shop` del CDN por `zIndex`,
  con su lugar en el lienzo de 450×880, y su imagen quieta. Sin producto,
  el perfil se muestra igual. *shortcut*: se lee en cada perfil abierto,
  sin la caché de 64 de Serein.
- **Plan.** Classic (1) y Basic (3) dejan usar emojis de otros servidores y
  animados; Nitro (2) y Basic, además stickers de otros servidores, como
  lo lee Serein. RUNE no envía un emoji o sticker que el plan no permite
  (el selector lo dice); el envío de un sticker revisa además que sea del
  servidor del canal, de un paquete leído o, con el plan, de otro servidor
  de la cuenta. `USE_EXTERNAL_EMOJIS`/`STICKERS` no se comprueban acá:
  Discord rechaza si faltan.
- **Paquetes.** Hasta 128 paquetes de hasta 500 stickers; un paquete con
  un sticker de servidor hace ilegible la respuesta. Los Lottie (formato
  3) se dibujan desde su JSON (2026-10-18) y se pueden enviar.
- **GIF.** Búsqueda con `media_format=tinygif&locale=es-ES&limit=50` (sin
  `provider`, como el cliente oficial; Serein fuerza `klipy`); tendencias
  con dos lecturas (GIF y categorías). Sólo direcciones y vistas previas
  de KLIPY y Tenor (`klipy.com`, `static*.klipy.com`, `tenor.com`,
  `media.tenor.com`, `c.tenor.com`), sumadas a los orígenes de imágenes de
  la sesión; se inserta el enlace en el borrador, nunca se envía solo. Una
  búsqueda nueva reemplaza la que está en curso o en cola; un fallo
  responde con su error y una sugerencia fallida no responde. *shortcut*:
  una página de 50, sin paginar.

Pruebas: `identity.rs` (actividades, marcas, roles, plan, insignias,
conexiones, paquetes, GIF, rutas y efecto) y `tests/session/identity.rs`
(paquetes y envío de un sticker de paquete, GIF con categorías,
sugerencias y una búsqueda que reemplaza a otra, `USER_UPDATE` con plan,
perfil con efecto), todo en loopback.

Para probar contra Discord (el dueño): que se vean las actividades de
amigos (con juegos, Spotify y una transmisión) y su tiempo; decoraciones,
nameplates y etiquetas en mensajes y miembros; roles con ícono y degradé;
insignias y conexiones de un perfil y sus enlaces; un efecto de perfil;
los paquetes de stickers y enviar uno estándar; con y sin Nitro, un emoji
de otro servidor; buscar GIF, una categoría y las sugerencias.

### Emoji y estados (2026-10-18, rama `emoji-y-estados`)

Sólo fixtures, sin probar contra el servicio real
([EXPRESSIONS.md](EXPRESSIONS.md#emoji-y-estados-2026-10-18-rama-emoji-y-estados),
[REA.md](REA.md#cliente-web-oficial-gif-emoji-grandes-y-permisos-externos-2026-10-18)).

- `USE_EXTERNAL_STICKERS` (bit 37) se lee y se escribe como
  `Permissions::EXTERNAL_STICKERS`; el bit 18 ya era `EXTERNAL_EMOJI`.
- «Más GIF»: `/gifs/search?…&limit=50&offset={n}` y
  `/gifs/trending-gifs?…&offset={n}` desde la segunda página, hasta la
  décima; `next_offset` cuando la página vino llena.
- Un emoji de un mensaje lleva dos imágenes: `emojis/{id}.webp?size=48` en
  una línea y `size=96` (`jumbo`) para un mensaje de sólo emoji, y su id
  (`TextTarget::Emoji::id`), con el que la tarjeta encuentra su servidor
  aunque el token tenga un nombre viejo.

Para probar contra Discord (el dueño): «Más GIF» en una búsqueda y en
tendencias (¿trae GIF nuevos o repite la página?); en un canal donde un
rol no tenga «Usar emojis externos» / «Usar stickers externos», que el
selector y el compositor lo digan; un mensaje de sólo emoji del servidor
en una pantalla con escala 2; la tarjeta de un emoji y «Copiar emoji»
(también la de un emoji renombrado: su nombre actual y «De {servidor}»);
la respuesta a un mensaje de sólo emoji; editar un mensaje o usar /me con
un emoji de otro servidor donde no se puede; el emoji del estado en
miembros, amigos y privados.

## Hilos y foros (2026-10-17)

Rutas de `references/serein-current` (`crates/discord-api/src/channel_actions.rs`,
`archives.rs`, `forum.rs` y `create_post` en `lib.rs`, leídos el
2026-10-17) y de la documentación de Discord; reimplementadas sin copiar
código. **Sólo fixtures en loopback; nada se probó contra Discord.** Van por
la misma cola REST de los mensajes (16 entradas, una solicitud a la vez,
espera global tras `429`), con un solo intento por escritura: un `5xx`, un
corte o una respuesta ilegible quedan ambiguos y no se repiten. Las lecturas
de páginas nunca son ambiguas. No cambian user agent ni Identify.

| Acción | Ruta | Qué confirma el cambio |
| --- | --- | --- |
| Hilo desde un mensaje | `POST /channels/{c}/messages/{m}/threads` `{"name", "auto_archive_duration": 4320}` | la respuesta (y `THREAD_CREATE`) |
| Hilo propio del canal | `POST /channels/{c}/threads` `{"name", "auto_archive_duration": 4320, "type": 11 o 12}` | la respuesta (y `THREAD_CREATE`) |
| Publicación de foro | `POST /channels/{foro}/threads` `{"name", "auto_archive_duration", "applied_tags", "message": {"content", "allowed_mentions", "attachments"?}}` | la respuesta (y `THREAD_CREATE`) |
| Seguir / dejar de seguir | `PUT` / `DELETE /channels/{hilo}/thread-members/@me` | el `204` (y `THREAD_MEMBER(S)_UPDATE`) |
| Renombrar, cerrar, bloquear, etiquetas | `PATCH /channels/{hilo}` `{"name"?, "archived"?, "locked"?, "applied_tags"?}` | la respuesta (y `THREAD_UPDATE`) |
| Eliminar | `DELETE /channels/{hilo}` | la respuesta (y `THREAD_DELETE`) |
| Abiertos de un foro | `GET /channels/{foro}/threads/search?archived=false&sort_by=last_message_time&sort_order=desc&limit=25&offset={n}` (no documentada; la usa Serein) | — |
| Abiertos de un canal, o de un foro si Discord rechaza la búsqueda | `GET /guilds/{g}/threads/active`, filtrados por padre | — |
| Archivados públicos | `GET /channels/{c}/threads/archived/public?limit=25&before={archive_timestamp}` | — |
| Archivados privados (moderación) | `GET /channels/{c}/threads/archived/private?limit=25&before={archive_timestamp}` | — |
| Archivados privados propios | `GET /channels/{c}/users/@me/threads/archived/private?limit=25&before={id}` | — |

Un hilo que nombra una respuesta se informa recién cuando la sesión lo
guarda al lado de su padre (`bootstrap::thread_confirmed`), con el acceso
que hereda de él; si el padre no es un canal conocido del mismo servidor,
se descarta, y su `Created` también. Una página **nunca** reemplaza un hilo
que la sesión ya conoce (el Gateway lo tiene más al día), así que una página
repetida o tardía no duplica ni retrocede nada; `THREAD_LIST_SYNC` sólo
reemplaza los hilos *abiertos* de su alcance y deja los archivados leídos por
página. El autor de un hilo nuevo figura en él. Una respuesta sin objeto
`member` o sin contadores conserva lo que se sabía.

**Páginas.** El Gateway de una cuenta personal avisa sólo de los hilos
abiertos que la cuenta sigue: por eso abrir un foro o la lista de hilos de
un canal pide una vez la primera página de abiertos y después, con «Cargar
más», los archivados. El orden es abiertos → archivados públicos → privados
(todos con `MANAGE_THREADS`, si no los propios). Se valida como en Serein:
cada hilo del padre pedido, archivado (o abierto) según la página, del tipo
de la página, sin repetidos; `archive_timestamp` sin crecer y antes del
cursor (los propios, por id estrictamente decreciente); `has_more` con una
página vacía es un error. Los abiertos de un foro llegan hasta 200, de a 25.
Desde el 2026-10-18 los archivados no se saltean: el `before` de Discord es
exclusivo, así que el cursor guarda el instante del último hilo y los ids
de los leídos en ese mismo instante (`{archive_timestamp}|{id},{id}`, hasta
100); la página siguiente se pide un milisegundo después de ese instante y
descarta los ya leídos (los posteriores al instante y los nombrados). Si una
página entera eran repetidos, o hubo más de 100 en un instante, sigue
estrictamente antes de él (*atajo*: el resto de ese instante se saltea).

**Antes de salir** se comprueba contra la sesión (`bootstrap::thread_allowed`):
nombre de 1 a 100 caracteres en una línea; hilos sólo en canales de texto o
anuncios, privados sólo en texto y sin mensaje; `CREATE_PUBLIC_THREADS` o
`CREATE_PRIVATE_THREADS` (bits 35 y 36, ahora separados en RUNE), más
historial si parte de un mensaje; publicaciones con `SEND_MESSAGES` en el
foro, primer mensaje de 1 a 2000, hasta 5 etiquetas del foro y distintas,
alguna si el foro la exige (flag `1 << 4`) y moderadas sólo con
`MANAGE_THREADS`; cerrar, renombrar y etiquetar para quien abrió el hilo o
la moderación, bloquear y eliminar sólo con `MANAGE_THREADS`, y reabrir uno
bloqueado también; seguir sólo un hilo abierto. Un aislamiento (timeout)
impide crear y editar.

**Responder en hilos** usa `SEND_MESSAGES_IN_THREADS`. Responder en un hilo
archivado y no bloqueado se permite: Discord lo reabre, como en el cliente
oficial, y avisa con `THREAD_UPDATE`. Reaccionar, votar, editar o avisar que
se escribe en uno archivado, no; uno bloqueado sólo acepta respuestas de la
moderación. El contador de mensajes y la última actividad suben en el Store
con cada `MESSAGE_CREATE` nuevo (uno repetido no cuenta).

**Foros.** `default_forum_layout` (2: galería), `default_sort_order` (1: por
creación) y el flag de etiqueta obligatoria llegan como `ForumDefaults`; la
interfaz los usa hasta que la persona elige orden o vista. «Copiar enlace»
arma `https://discord.com/channels/{servidor}/{hilo}`.

Un hCaptcha en estas rutas lo resuelve la persona y la escritura se repite
una vez (ver [Verificación](#verificación-en-cualquier-escritura-2026-10-17)).
Una publicación nueva puede llevar archivos (ver
[Adjuntos](#adjuntos-de-salida-2026-10-17)); un hilo no tiene primer
mensaje en RUNE.

Para probar contra el servicio (dueño, con una cuenta propia y un servidor
de prueba): crear un hilo desde un mensaje y otro privado; seguirlo y dejar
de seguirlo; escribir en él; cerrarlo, responder y ver que se reabre;
renombrarlo; abrir un foro y ver que aparecen publicaciones que la cuenta no
sigue; crear una publicación con etiquetas; «Cargar más» hasta los
archivados públicos y privados; filtrar por etiqueta y cambiar el orden y la
vista; con moderación, bloquear y eliminar un hilo.

### Silenciar hilos, tarjetas de foro y chat de escenarios (2026-10-18)

Rama `hilos-foros-y-escenarios`. **Sólo fixtures; nada se probó contra
Discord.** Evidencia del cliente oficial en
[REA.md](REA.md#cliente-web-oficial-silenciar-hilos-y-tarjetas-de-foro-2026-10-18).

| Qué | Ruta o mensaje | Origen |
| --- | --- | --- |
| Silenciar un hilo que se sigue | `PATCH /channels/{hilo}/thread-members/@me/settings` `{"muted": true, "mute_config": {"end_time": ISO o null, "selected_time_window": segundos o -1}}` | cliente oficial (`setNotificationSettings`) y Serein |
| Volver a oírlo | la misma ruta, `{"muted": false}` | cliente oficial |
| Su estado | `member.muted` y `member.mute_config.end_time` (sin fin: para siempre) en `READY`/`GUILD_CREATE`, `THREAD_CREATE`/`UPDATE`, `THREAD_MEMBER_UPDATE` propio, `added_members` de `THREAD_MEMBERS_UPDATE`, `members` de `THREAD_LIST_SYNC` y de las páginas | cliente oficial (`JoinedThreadsStore`) |
| Tarjeta de publicación | `first_messages` de `/channels/{foro}/threads/search`, y `message` de la respuesta al crear una publicación | Serein; la regla de la reacción, del cliente oficial |

`Command::MuteThread { thread, until }` (`None` vuelve a oírlo,
`MUTED_FOREVER` es para siempre) sale sólo si la cuenta sigue el hilo y el
fin es futuro (`thread_allowed`); el `200` lo confirma
(`thread_confirmed`) y el `THREAD_MEMBER_UPDATE` tiene la última palabra.
Dejar el hilo termina el silencio, también en cuanto se confirma el
`FollowThread` (antes del `THREAD_MEMBERS_UPDATE`). Un hilo silenciado no
avisa de nada, ni de sus menciones, y sólo marca cuántas menciones tiene,
como un canal silenciado en RUNE.

Un `READY` nuevo (siempre de la misma cuenta) reemplaza la instantánea de la
sesión, pero conserva los hilos leídos por página que no nombra, bajo un
padre que sigue visible y con sus permisos recalculados; uno abierto que la
cuenta seguía se va, porque el `READY` lo habría nombrado
(`bootstrap::keep_paged_threads`, la misma regla que
`Store::keep_paged_threads`). Sin eso, el Store los mostraba y la sesión
rechazaba su historial, escribir, reabrirlos y sus eventos del Gateway.

La tarjeta guarda en `Thread::starter` el comienzo del primer mensaje (sin
texto si tiene un spoiler), su primera imagen (un adjunto, o la de una
tarjeta; nunca tras spoiler ni explícita) y sus reacciones; muestra la más
dada o, sin ninguna, la predeterminada del foro sin número. Una página de
abiertos actualiza sólo eso en un hilo que la sesión ya conocía. Un archivado
leído por `threads/archived/public` no trae primer mensaje (*atajo*: el
cliente oficial pide `/channels/{foro}/post-data` con `thread_ids`; RUNE
todavía no).

Un escenario (tipo 13) tiene chat por las mismas rutas y eventos que el de un
canal de voz (`messages::holds_chat`).

## Lecturas, búsqueda y miembros remotos (2026-10-17)

Bloque P2 de [paridad con Serein](../tasks/serein-parity.md). Rutas, cuerpos
y ritmo de `references/serein-current` (`crates/discord-api/src/lib.rs`:
`mark_read`, `mark_guild_read` y `search`; `crates/discord-protocol/src/`:
`read_state.rs`, `search.rs` y `ready.rs`; `crates/discord-gateway/src/`:
`lib.rs` y `member_search.rs`; `crates/client-core/src/`: `read_state.rs`,
`search.rs` y `member_search.rs`, leídos el 2026-10-17), reimplementadas sin
copiar código. **Sólo fixtures en loopback; nada se probó contra Discord.**
No cambian user agent, `X-Super-Properties` ni Identify (que sigue sin
`capabilities`).

| Qué | Ruta o mensaje | Origen | Ritmo |
| --- | --- | --- | --- |
| Confirmar lectura | `POST /channels/{c}/messages/{m}/ack` `{"token": <el último recibido o null>, "manual": false}`; la respuesta trae el `token` siguiente | Serein | una en vuelo por canal (lo leído mientras tanto sale cuando contesta, `Store::read_follow_up`); cola REST común, una solicitud a la vez; la más nueva de un canal reemplaza a la encolada; hasta 64 encoladas aparte de las 16 de la cola |
| Marcar como no leído | `POST /channels/{c}/messages/{m−1}/ack` `{"manual": true, "mention_count": n}` | Serein | igual |
| Marcar el servidor como leído | `POST /guilds/{g}/ack` | Serein | igual |
| Lectura en otro dispositivo | `MESSAGE_ACK` `{channel_id, message_id, version, manual?, mention_count?}` | Serein | uno con `version` menor que la vista en ese canal se descarta |
| Estado al conectar | `read_state` del `READY` (lista o `{version, entries}`) | ya se leía; ahora con versión | — |
| Servidores pasivos | `PASSIVE_UPDATE_V2` (`updated_channels`, `updated_voice_states`, `removed_voice_states`) y `PASSIVE_UPDATE_V1` (`channels`, `voice_states`) | Serein (V2); V1 por el mismo camino | — |
| Buscar mensajes | `GET /guilds/{g}/messages/search?…` en un servidor, `GET /channels/{c}/messages/search?…` fuera de servidores: `limit=25&sort_by=timestamp&sort_order=desc` y `content`, `author_id`, `mentions`, `has`, `channel_id`, `author_type`, `pinned`, `min_id`, `max_id`, `offset` | Serein; `has=sticker` y `during:` como el cliente oficial | una a la vez; una nueva reemplaza a la encolada o en vuelo |
| Índice no listo | `202`, o `code` entre 110000 y 119999, con `retry_after` | Serein | la interfaz vuelve a pedir una sola vez, sola, cuando vence |
| Buscar miembros | op 8 `{"guild_id", "query", "limit": 100, "nonce"}` → `GUILD_MEMBERS_CHUNK` con ese `nonce` (`chunk_index` 0 de 1, hasta 100) | Serein | 350 ms sin tipear, 1 s entre consultas, 15 por minuto, sólo la más nueva; respuestas guardadas 5 min |

**Versiones.** El contrato de RUNE pide confirmaciones con versión creciente
por cuenta y canal. El adaptador numera cada confirmación (el `READY`, cada
`MESSAGE_ACK`, cada respuesta a un ACK) con un contador propio del
proveedor, que sigue creciendo después de una reconexión; la `version` de
Discord sólo sirve para descartar un `MESSAGE_ACK` atrasado. La respuesta a
un ACK confirma lo más nuevo conocido del canal (si otro dispositivo ya
avanzó más, eso). El token del ACK vive sólo en la sesión y no se muestra.

**No leído.** El cursor de Discord es «leído hasta»; marcar no leído desde
`M` manda el cursor `M−1` (los ids son números: es «todo lo anterior a
`M`»). El adaptador es el único que resta: la interfaz y el estado nunca
fabrican ids. Un `MESSAGE_ACK` manual de otro dispositivo sólo trae el
cursor: `rune-state` pone el recordatorio en el primer mensaje cargado
después de él o, si no hay ninguno cargado, en el más nuevo conocido.

**Servidor leído.** Después del `2xx` se confirman, sin petición, los
canales legibles del servidor hasta su mensaje más nuevo conocido (del
`READY`, de `MESSAGE_CREATE` o de las actualizaciones pasivas).

**Actualizaciones pasivas.** Los canales y su último mensaje salen como
`LatestMessages`: marcan no leído sin traer los mensajes. Las voces entran
como si fueran `VOICE_STATE_UPDATE` (los quitados, con `channel_id` nulo),
por el mismo código de llamadas. Los miembros que traen se ignoran: la lista
y el perfil los leen. Como Identify no declara `capabilities`, Discord puede
mandar V1 o ninguna; se leen las dos.

**Búsqueda.** Los resultados no son historial: no entran en ninguna
conversación ni en SQLite. Sus autores llegan antes como `UserChanged`. Se
valida la página (hasta 25, cada mensaje una vez, el más nuevo primero, en
el canal pedido si no es un servidor) y de cada grupo se toma el mensaje con
`hit: true` (o el único). El texto se codifica byte por byte: no puede
agregar parámetros. Las fechas son el día local: `before:` corta a la
medianoche de ese día, `after:` a la del siguiente y `during:` cubre el día.
`in:` sólo vale en un servidor.

**Deliberadamente no:** `POST /read-states/ack-bulk` (el ACK en lote del
cliente oficial; Serein no lo usa): una categoría marcada como leída manda
un ACK por canal, en fila. Tampoco `has:forward`, `embed_type`, el orden
«más antiguos» ni buscar en todo Discord a la vez. `has:poll`,
`link_hostname` y el orden por relevancia llegaron el 2026-10-18
([abajo](#reacciones-súper-reacciones-fijados-y-búsqueda-2026-10-18-rama-reacciones-fijados-y-busqueda)).

Para probar contra el servicio (dueño, con su cuenta y un servidor propio):
leer un canal en RUNE y verlo leído en el teléfono; leerlo en el teléfono y
ver que RUNE quita el no leído y las menciones; marcar como no leído en RUNE
y verlo en el cliente oficial, y al revés; marcar un servidor como leído
desde su menú; buscar con cada filtro (`from:`, `mentions:`, `has:` con cada
tipo, `in:`, `before:`, `after:`, `during:`, `pinned:`, `author_type:`),
pasar de página, abrir un resultado de otro canal; buscar en un servidor
grande o nuevo para ver el aviso de índice y su reintento; escribir `@` en
un servidor grande y encontrar a alguien que no figura en la lista; en un
servidor grande no abierto, ver aparecer los no leídos sin entrar.

## Acciones de mensaje, compositor y moderación (2026-10-17, rama `acciones-de-mensaje`)

Bloque P4 de [paridad con Serein](../tasks/serein-parity.md). Rutas y cuerpos
de `references/serein-current` (`crates/discord-api/src/lib.rs`: reenvío,
encuesta, `expire`, reacciones, `message_options`; `server_admin.rs` y
`server_actions.rs`; `crates/client-core/src/`: `forwarding.rs`, `polls.rs`,
`reactions.rs`, `replies.rs`, `server_admin.rs`; `crates/ui/src/`:
`markdown.rs`, `formatting.rs`, `channel_pill.rs`, `local_time.rs`, leídos el
2026-10-17), reimplementadas sin copiar código. Lo que Serein no hace
(quién votó, ensordecer en el servidor, leer un canal por su id) sale de las
rutas documentadas de Discord. **Sólo fixtures en loopback; nada se probó
contra Discord.** No cambian user agent, `X-Super-Properties` ni Identify.
Todas las escrituras pasan por `request_answer`: un hCaptcha se ofrece como
en cualquier otra.

| Qué | Ruta o mensaje | Origen | Capacidad |
| --- | --- | --- | --- |
| Reenviar | `POST /channels/{destino}/messages` `{"message_reference": {"type": 1, "channel_id", "message_id", "guild_id"?}, "nonce", "allowed_mentions": {"parse": [], "replied_user": false}}`, sin `content`; el mensaje opcional sale después, como un mensaje común | Serein | `forward_messages` |
| Crear encuesta | `POST /channels/{c}/messages` `{"poll": {"question": {"text"}, "answers": [{"poll_media": {"text", "emoji": {"name"}?}}], "duration": horas, "allow_multiselect", "layout_type": 1}, "nonce", …}` | Serein | `create_polls` |
| Terminar encuesta | `POST /channels/{c}/polls/{m}/expire`; la respuesta es el mensaje | Serein | `create_polls` |
| Quién reaccionó | `GET /channels/{c}/messages/{m}/reactions/{emoji}?limit=100[&after=id]`, el emoji byte por byte | Serein | `reaction_lists` |
| Quién votó | `GET /channels/{c}/polls/{m}/answers/{a}?limit=100[&after=id]` → `{"users": […]}` | documentada | `poll_voters` |
| Conversación de un enlace | `GET /channels/{id}`: un privado entra como `ChannelChanged`; uno de servidor, como si fuera `CHANNEL_CREATE` o `THREAD_CREATE` (con sus permisos calculados) | documentada | `load_channels` |
| `@silent` | quita el marcador y un espacio, y manda `"flags": 4096` (también con archivos) | Serein | `silent_messages` |
| Mencionar al responder | `allowed_mentions.replied_user` según el interruptor (antes siempre `false`) | Serein | `reply_mentions` |
| Moderar la voz | `PATCH /guilds/{g}/members/{u}` con `{"mute"}`, `{"deaf"}`, `{"channel_id": destino}` o `{"channel_id": null}` | Serein (`deaf`: documentada) | `voice_moderation` |
| Abandonar | `DELETE /users/@me/guilds/{g}` `{"lurking": false}` | ya estaba | `leave_servers` |

**Enlaces.** `text::references` reconoce, además de menciones, los enlaces
`https://(www.|ptb.|canary.)discord.com/channels/{servidor o @me}/{canal}[/{mensaje}]`
(y `discordapp.com`, con `:443` opcional, consulta y fragmento ignorados,
ids sin cero inicial y de hasta 20 dígitos) y las marcas `<t:segundos[:t|T|d|D|f|F|R]>`.
La interfaz muestra un enlace escrito tal cual como pastilla («Servidor ›
#canal › mensaje») y lo abre en RUNE; uno enmascarado conserva su texto pero
también abre en RUNE. Un canal no cargado se pide por su id; si Discord lo
niega, la interfaz lo dice y no se mueve. Los enlaces de `discord.com`,
`discord.gg`, `discordapp.com` y `discordapp.net` (y sus subdominios, por
`https` y sin puerto) abren sin preguntar (`trusted_links`); los demás
preguntan, con «Confiar en {dominio} de ahora en más» guardado en este
equipo y el aviso de enlace enmascarado.

**Comprobaciones antes de mandar** (`message_actions::allowed` y
`admin::allowed`): reenviar pide ver e historial en el origen y escribir en
el destino (`SEND_IN_THREADS` en hilos), sin aislamiento; una encuesta,
`SEND_POLLS` (bit 49) en servidores; terminarla, que la encuesta esté
cargada y sea propia; las listas de personas, historial; moderar la voz,
`MUTE_MEMBERS`, `DEAFEN_MEMBERS` o `MOVE_MEMBERS`, que la persona esté en
voz en ese servidor, por debajo del rango propio (como el resto de la
administración; Discord no lo exige) y, para mover, a otro canal de voz al
que pueda entrar.

**Deliberadamente no:** mandar súper reacciones (desde el 2026-10-18 se
leen y se muestran, ver [más abajo](#reacciones-súper-reacciones-fijados-y-búsqueda-2026-10-18-rama-reacciones-fijados-y-busqueda)), reenviar encuestas (Discord no lo
permite), mencionar al autor al responder con archivos (sigue sin notificar;
el interruptor se oculta mientras haya archivos). Arrastrar personas entre
canales de voz se hace desde el 2026-10-18 (rama `admin-canales-y-voz`): es
el mismo `ModerateVoice::Move` que «Mover a», con sus comprobaciones.

Para probar contra el servicio (dueño, con su cuenta y un servidor propio):
reenviar un mensaje a un canal, a un privado y a un grupo, con y sin
mensaje; crear una encuesta con emoji y varias respuestas, votar desde el
teléfono, ver los votos y terminarla antes; ver quién reaccionó en un
mensaje con muchas reacciones (más de 100: «Cargar más»); copiar el enlace
de un mensaje y pegarlo en otro lado; abrir enlaces de `discord.com`,
`ptb.` y `canary.` a canales, hilos archivados, privados cerrados y
servidores ajenos; mandar `@silent hola` y una respuesta con «@ DESACTIVADO»
y comprobar que no llega notificación; silenciar, ensordecer, mover y
desconectar a alguien (y ver el rechazo con alguien de rango igual o
mayor); abandonar un servidor; ver las marcas `<t:…>` en otra zona horaria.

## Escritorio y actividad local (2026-10-17, rama `escritorio`)

Bloque P6 de [paridad con Serein](../tasks/serein-parity.md), la parte del
proveedor; lo de escritorio está en [PLATFORM.md](PLATFORM.md). Rutas y
forma del op 3 de `references/serein-current`
(`crates/discord-gateway/src/activity.rs`, `apps/desktop/src/game_activity.rs`,
`crates/discord-api/src/detectable.rs`, `crates/discord-protocol/src/rpc.rs`,
leídos el 2026-10-17), reimplementadas. **Sólo fixtures en loopback; nada
se probó contra Discord.** No cambian user agent, `X-Super-Properties` ni
Identify.

- **Actividad local** (`Command::SetLocalActivity`, capacidad
  `local_activity`): el Gateway la manda en el mismo op 3 que el estado y
  el estado personalizado, primero en `activities`, con el mismo ritmo
  (uno cada 5 s como mucho) y de nuevo en cada conexión; invisible manda
  `[]`. Un juego (con nombre propio y sin imágenes que resolver) sale
  enseguida; la actividad de un programa espera su nombre
  (`GET /applications/{id}/rpc`, comprobando id y nombre) y sus imágenes:
  claves por `GET /oauth2/applications/{id}/assets` (refrescadas a lo sumo
  cada 60 s), `https` por `POST /applications/{id}/external-assets` (las
  ocho últimas recordadas, `mp:` + la ruta) y `mp:` tal cual; una imagen
  que no resuelve se va con su texto. Tope de 3 KiB (sin imágenes si no
  entra). La más nueva gana sobre una en cola o en camino; una aplicación
  que Discord no nombra no se publica, sin aviso. Las búsquedas pasan por
  la misma cola REST que el resto (`request_answer`, así que un hCaptcha en
  `external-assets` se ofrecería como en cualquier escritura).
- **Juegos reconocibles** (`Command::LoadDetectableGames`, capacidad
  `detectable_games`): `GET /applications/detectable`, pública, **sin**
  la credencial, hasta 24 MiB, leída en `spawn_blocking` con tipos propios
  (no un `Value` entero); sin lanzadores, hasta 16 programas por juego y
  50 000 juegos. Sin caché en disco: se pide una vez por sesión mientras
  la detección esté encendida.
- **Códigos DAVE** (capacidad `voice_privacy_code`): `Event::CallPrivacyCode`
  desde la conexión de la llamada y `Command::VoiceVerificationCode` →
  `Event::VoiceVerification`; ver [MEDIA.md](MEDIA.md#código-de-privacidad-dave-2026-10-17).
  Una sola verificación en curso por llamada; otra la reemplaza.

Para probar contra el servicio: encender «Actividad de otros programas»,
abrir un juego o programa con Rich Presence y ver la actividad en el perfil
desde otra cuenta (con imágenes de la aplicación y por dirección); cerrar el
programa y verla desaparecer; ponerse invisible y comprobar que no se ve.
Encender «Detectar juegos», abrir un juego de la lista (nativo y por Proton)
y ver «Jugando a…». En una llamada con otra persona, comparar el código de
privacidad con el del cliente oficial y el de verificación de cada uno.

## Administración del servidor (2026-10-17)

Rutas de `references/serein-current` (`crates/discord-api/src/`:
`create_guild` en `lib.rs`, `server_settings.rs`, `server_actions.rs`,
`server_roles.rs`, `server_admin.rs`, `server_audit_log.rs`,
`server_invites.rs`, `server_integrations.rs` y `gif_favorites.rs`, leídos
el 2026-10-17) y, donde Serein no tiene nada (vetos, aislamiento, el motivo
para el registro), las que Discord documenta; reimplementadas sin copiar
código. **Sólo fixtures en loopback; nada se probó contra Discord.** Van por
la misma cola REST (16 entradas, una solicitud a la vez, espera global tras
`429`, respuestas acotadas), con un solo intento por escritura: un `5xx`, un
corte o una respuesta ilegible quedan ambiguos y no se repiten. Las lecturas
(también la búsqueda de miembros, que es un `POST`) nunca son ambiguas. Una
escritura que primero lee lo que cambia falla sin ambigüedad mientras lee.
No cambian user agent, `X-Super-Properties` ni Identify.

| Acción | Ruta | Origen | Qué confirma el cambio |
| --- | --- | --- | --- |
| Crear servidor | `POST /guilds` `{"name", "icon": null, "channels": [], "system_channel_id": null, "guild_template_code": "2TffvPucqHkN"}` | Serein | la respuesta (como `GUILD_CREATE`, si el Gateway no lo trajo antes) y `Created` |
| Nombre, descripción y color | `PATCH /guilds/{g}/profile` `{"name"?, "description"?, "brand_color_primary"?: "#rrggbb"}` | Serein | la respuesta (como `GUILD_UPDATE`) |
| Eliminar / salir | `DELETE /guilds/{g}` / `DELETE /users/@me/guilds/{g}` `{"lurking": false}` | Serein (el cuerpo, del cliente oficial) | el `204` (como `GUILD_DELETE`) |
| Crear canal o categoría | `POST /guilds/{g}/channels` `{"name", "type", "parent_id"?, "permission_overwrites"?}` | Serein | la respuesta (`CHANNEL_CREATE`) y `Created` |
| Editar / eliminar canal | `PATCH /channels/{c}` (campos parciales) / `DELETE /channels/{c}` | Serein | la respuesta (`CHANNEL_UPDATE` / `CHANNEL_DELETE`) |
| Poner o quitar un permiso del canal | `GET /channels/{c}` y `PATCH /channels/{c}` `{"permission_overwrites": [lista completa]}` | Serein | la respuesta (`CHANNEL_UPDATE`) |
| Crear rol | `POST /guilds/{g}/roles` `{"name", "permissions": "0"}` | Serein | la respuesta (`GUILD_ROLE_CREATE`) y `Created` |
| Editar rol | `GET /guilds/{g}/roles` (si cambian permisos) y `PATCH /guilds/{g}/roles/{r}`; orden: `PATCH /guilds/{g}/roles` `[{"id", "position"}]` | Serein | la respuesta (`GUILD_ROLE_UPDATE`; el orden, `GUILD_UPDATE`) |
| Eliminar rol | `DELETE /guilds/{g}/roles/{r}` | Serein | el `204` (`GUILD_ROLE_DELETE`) |
| Roles de un miembro | `PUT` / `DELETE /guilds/{g}/members/{u}/roles/{r}`, uno por rol, y `GET /guilds/{g}/members/{u}` | Serein | la lectura final (`GUILD_MEMBER_UPDATE`) |
| Apodo | `PATCH /guilds/{g}/members/{u}` `{"nick"}`; el propio, `/members/@me` | Serein; `@me`, documentada | la respuesta (`GUILD_MEMBER_UPDATE`) |
| Aislar / quitar el aislamiento | `PATCH /guilds/{g}/members/{u}` `{"communication_disabled_until": ISO 8601 o null}` | documentada | la respuesta (`GUILD_MEMBER_UPDATE`) |
| Expulsar | `DELETE /guilds/{g}/members/{u}` | Serein | el `204` (`GUILD_MEMBER_REMOVE`) |
| Vetar / revocar | `PUT /guilds/{g}/bans/{u}` `{"delete_message_seconds": 0}` / `DELETE /guilds/{g}/bans/{u}` | documentada | el `204` (y `GUILD_BAN_ADD` / `GUILD_BAN_REMOVE`) |
| Vetos de a páginas | `GET /guilds/{g}/bans?limit={n}&after={u}` | documentada | — |
| Registro de auditoría | `GET /guilds/{g}/audit-logs?limit={n}[&user_id][&action_type][&before]` | Serein | — |
| Invitaciones | `GET /guilds/{g}/invites`; `POST /channels/{c}/invites` `{"max_age", "max_uses", "temporary", "unique": true}`; `DELETE /invites/{código}` | Serein | la respuesta |
| Emojis | `POST /guilds/{g}/emojis` `{"name", "image": "data:image/png;base64,…"}` (o `image/gif`); `PATCH` / `DELETE /guilds/{g}/emojis/{id}` | Serein | la respuesta (y `GUILD_EMOJIS_UPDATE`) |
| Stickers | `POST /guilds/{g}/stickers` (multipart: `name`, `description`, `tags`, `file`); `PATCH` / `DELETE /guilds/{g}/stickers/{id}` | Serein | la respuesta (y `GUILD_STICKERS_UPDATE`) |
| Miembros de a páginas | `POST /guilds/{g}/members-search` `{"limit": 100, "sort": 1, "after"?: {"user_id", "guild_joined_at"}}` (no documentada) | Serein | — |
| Podar | `GET /guilds/{g}/prune?days={d}` / `POST /guilds/{g}/prune` `{"days", "compute_prune_count": false}` | Serein | `pruned` / `GUILD_MEMBER_REMOVE` |
| Webhooks | `GET /guilds/{g}/webhooks`; `POST /channels/{c}/webhooks` `{"name"}`; `PATCH` / `DELETE /webhooks/{id}`, después de encontrarlo en la lista del servidor | Serein | la respuesta |
| Integraciones | `GET /guilds/{g}/integrations`; `DELETE /guilds/{g}/integrations/{id}` | Serein | el `204` |
| GIF favoritos | `GET` y `PATCH /users/@me/settings-proto/2` `{"settings", "required_data_version"}` (protobuf `FrecencyUserSettings`; no documentada) | Serein | la respuesta, que debe traer el cambio |

Expulsar, vetar y aislar llevan el motivo en `X-Audit-Log-Reason`
(documentado; codificado con `%`, hasta 512 caracteres). Los bits de permisos
se traducen en los dos sentidos con la misma tabla
(`bootstrap::PERMISSION_BITS`, que ahora incluye `MANAGE_WEBHOOKS`, bit 29
de Discord); al escribir los de un rol o un permiso de canal se lee primero
el valor actual y se conservan los bits que RUNE no modela (por ejemplo,
texto a voz o comandos de aplicación). Un permiso de canal se escribe como la
lista completa, leída fresca, para no pisar el cambio de otra persona.

**La respuesta entra en la sesión.** Para que una orden siguiente valide
sin esperar al Gateway (un permiso en el canal recién creado, editar el rol
recién creado), la respuesta se aplica como el dispatch que la representa
(`admin::confirmed`): misma validación, recálculo de permisos y
`ChannelChanged` que el Gateway, y sus eventos salen antes que los de la
respuesta (`Created` al final). Una creación que el Gateway ya trajo no se
reemplaza; una respuesta que no se puede aplicar deja la sesión como
estaba, a la espera del Gateway; un error no aplica nada. Los emojis y
stickers se guardan en su servidor (`ServerChanged`) y los miembros de una
página también, para poder cambiarles los roles. `GUILD_BAN_ADD`,
`GUILD_BAN_REMOVE` y `GUILD_STICKERS_UPDATE` se reenvían ahora desde el
Gateway. Mientras una solicitud está en vuelo sólo se cancela si su
servidor deja de ser de la cuenta: el Gateway suele avisar del cambio
(`GUILD_ROLE_DELETE`, `GUILD_MEMBER_REMOVE`) antes que la respuesta.

**Antes de salir** se comprueba contra la sesión (`admin::allowed`): el
servidor y la membresía propia conocidos; los permisos de la cuenta
(`MANAGE_SERVER`; `MANAGE_CHANNELS` y, para los permisos de canal,
`MANAGE_ROLES` en ese canal; `KICK`, `BAN`, `TIMEOUT_MEMBERS`;
`MANAGE_NICKNAMES`, o `CHANGE_NICKNAME` para el propio; `VIEW_AUDIT_LOG`;
`MANAGE_SERVER` para invitaciones e integraciones; `CREATE_INVITE` en el
canal; `MANAGE_EMOJI`; `MANAGE_WEBHOOKS`; `KICK` y `MANAGE_SERVER` para
podar; alguno de moderación para la página de miembros); la jerarquía (el
rol más alto de la cuenta por encima del de la persona y del rol que crea,
edita, da, quita, borra o cuyo permiso de canal cambia; nadie actúa sobre
quien es dueño; nadie se expulsa, veta ni aísla a sí mismo); nadie da
permisos que no tiene; nombres y largos (servidor 2–100, canal y rol 1–100,
apodo hasta 32, motivo hasta 512, descripción hasta 300, tema hasta 1024 o
4096 en un foro, modo lento hasta 6 h), aislamiento en el futuro y hasta 28
días (nunca a quien administra), invitaciones hasta 7 días y 100 usos,
emojis y stickers como pide `NewExpression::problem`, webhooks como pide
`valid_webhook_name`, poda de 1 a 30 días, ids distintos de cero y que la
expresión, el rol o el canal existan. Sólo quien es dueño elimina el
servidor, y no puede salir de él. Una cuenta aislada o retenida en el
servidor sólo puede salir. Lo demás se rechaza con «Esta acción todavía no
está disponible.».

**Registro de auditoría.** Se piden `limit` entradas (hasta 100) y se
describen en las palabras de RUNE: tipos 1, 10–15, 20–25, 30–32, 40, 42,
60–62 y 90–92 con su acción propia (el 24 es apodo o aislamiento según el
cambio), el resto como `Other` con una frase («creó un webhook», «eliminó
un mensaje»). Sólo se leen las claves que describen la acción (`name`,
`code`, `nick`, `communication_disabled_until`, `$add`, `$remove`): los
valores de otras (un token, un secreto) nunca salen del adaptador. Un nombre
que la entrada no trae se completa con el de la sesión o «(sin nombre)».
Como el tipo 24 de Discord son dos de RUNE, cada página se filtra otra vez
con la consulta; si eso la vacía y Discord tiene más, se pide la siguiente
(hasta 5) en vez de ofrecer una página vacía. ~~*Atajo:* filtrar por
permisos pide sólo el tipo 14; los permisos creados o quitados (13 y 15)
aparecen sin filtro.~~ *Desde el 2026-10-18* (rama `admin-canales-y-voz`,
sólo con fixtures): la ruta acepta un único `action_type`, así que
«Permisos» pide el 13, el 14 y el 15 por separado con el mismo cursor y
`admin::audit_merge` mezcla las tres páginas por id, la más nueva primero:
nada más viejo que la última entrada de una página llena (ese tipo puede
tener más), y como mucho `limit` entradas; la siguiente página empieza
después de la última mostrada.

**GIF favoritos.** El cambio se hace sobre una lectura fresca y reescribe
sólo el campo 2 del protobuf, conservando byte a byte las entradas y campos
que RUNE no entiende; el nuevo va con `order` = mayor + 1 y la lista se
muestra por `order`. Una respuesta `out_of_date` es un rechazo (no se
guardó); una que no trae el cambio, ambigua.

**Deliberadamente no:** copiar la URL de un webhook (es una credencial: su
`token` se descarta al leer), eventos programados,
banner del servidor y plantillas propias (el ícono llegó con P7, más
abajo). La edición del onboarding llegó después, con su sección propia
([Reglas del servidor y onboarding](#reglas-del-servidor-y-onboarding-2026-10-18-rama-reglas-y-onboarding)),
y también las reglas de AutoMod ([AutoMod](#automod-2026-10-19-rama-automod)).
Un miembro que la sesión no cargó cuenta como el de menor rango (Discord
decide); cambiarle los roles pide cargarlo antes en la página de miembros.
Mientras Discord indexa la búsqueda de miembros responde `202` y RUNE lo
informa como respuesta incompatible.

Un hCaptcha en estas rutas se ofrece como en cualquier escritura (ver
[Verificación en cualquier escritura](#verificación-en-cualquier-escritura-2026-10-17)):
van por el mismo camino REST (`request_answer`, `Failure::of` y
`challenges::offer`); P7 lo prueba al pausar invitaciones.

Para probar contra el servicio (dueño, con una cuenta propia y un servidor
de prueba propio): crear un servidor y ver que aparece con sus canales;
cambiarle nombre, descripción y color; crear una categoría, un canal dentro
y otro privado; poner y quitar un permiso de rol en el canal y comprobar en
el cliente oficial que los demás permisos siguen; crear un rol, darle
permisos, subirlo y bajarlo, dárselo y quitárselo a una cuenta de prueba;
cambiar un apodo propio y ajeno; aislar unos minutos y levantarlo; expulsar
y vetar (con motivo, y verlo en el registro), listar vetos y revocar;
crear, listar y revocar una invitación; subir un emoji PNG y otro GIF y un
sticker, renombrarlos y borrarlos; filtrar el registro por persona y por
acción y paginar; paginar miembros, contar la poda (sin ejecutarla);
crear, renombrar, mover y borrar un webhook; listar integraciones; marcar y
desmarcar un GIF favorito y verlo en el cliente oficial; salir de un
servidor ajeno y eliminar el de prueba.

## Resto de la administración (P7, 2026-10-17, rama `administracion-2`)

Lo que faltaba de la administración de Serein (`references/serein-current`,
`crates/discord-api/src/`: `server_settings.rs`, `server_invites.rs`,
`channel_actions.rs`, `server_roles.rs` y `server_admin.rs`; y
`crates/discord-protocol/src/` de los mismos nombres y `forum.rs`, leídos el
2026-10-17), reimplementado sin copiar código. **Sólo fixtures en loopback;
nada se probó contra Discord.** Mismo camino que la administración de arriba
(`admin::allowed` antes de salir, un intento por escritura, la respuesta
aplicada como su dispatch con `admin::confirmed`, y el captcha humano de
cualquier escritura). Las comprobaciones y el formato viven en
`admin/rest.rs`.

| Acción | Ruta | Origen | Qué confirma el cambio |
| --- | --- | --- | --- |
| Ícono y rasgos del perfil | `PATCH /guilds/{g}/profile` `{"icon"?: "data:image/png;base64,…" o null, "traits"?: [{"label", "emoji_name", "emoji_id": null, "emoji_animated": false, "position"}]}` | Serein | la respuesta: `icon_hash` y `traits` (como `GUILD_UPDATE`) |
| Leer los rasgos | `GET /guilds/{g}/profile` | Serein | — (una vez por sesión, al abrir los ajustes si faltan) |
| Seguridad y participación | `GET /guilds/{g}` (si cambian los mensajes del sistema) y `PATCH /guilds/{g}` `{"system_channel_id"?, "system_channel_flags"?, "default_message_notifications"?, "afk_channel_id"?, "afk_timeout"?, "verification_level"?, "explicit_content_filter"?}` | Serein | la respuesta, sólo sus claves de opciones (como `GUILD_UPDATE`) |
| Pausar o reanudar invitaciones | `GET /guilds/{g}` y `PATCH /guilds/{g}` `{"features": [las leídas, con o sin "INVITES_DISABLED"]}` | Serein | la respuesta (`features`) |
| Reordenar canales en lote | `PATCH /guilds/{g}/channels` `[{"id", "position", "parent_id", "lock_permissions"?: true}]` | Serein (y la REA del cliente oficial para `lock_permissions`) | el `204`: cada canal movido como `CHANNEL_UPDATE` parcial (los permisos sincronizados llegan con el `CHANNEL_UPDATE` del Gateway) |
| Ajustes de un foro | `GET /channels/{c}` (sus `flags`) y `PATCH /channels/{c}` `{"available_tags"?: [{"id"?, "name", "moderated", "emoji_id": null, "emoji_name"}], "default_forum_layout", "default_sort_order", "default_reaction_emoji", "flags"}` | Serein | la respuesta (`CHANNEL_UPDATE`) |
| Ícono, emoji y degradé de rol | `PATCH /guilds/{g}/roles/{r}` `{"icon"?: data URI, "unicode_emoji"?, "colors"?: {"primary_color", "secondary_color", "tertiary_color"}}` | Serein | la respuesta (`GUILD_ROLE_UPDATE`) |
| Miembros por rol | `GET /guilds/{g}/roles/member-counts` | Serein | — |
| Miembros con orden y filtro | `POST /guilds/{g}/members-search` `{"limit", "sort": 1–4, "and_query"?: {"guild_joined_at": {"range": {"gte"}}}, "after"?}`; lee `join_source_type` y `source_invite_code` | Serein | — |

- **Opciones del servidor.** `ServerOptions` se lee del `READY` (sus
  `properties`) y de cada `GUILD_UPDATE`, clave por clave: lo que no viene,
  o no es un valor documentado, se conserva. `features` da
  `INVITES_DISABLED`, `ROLE_ICONS`, `ENHANCED_ROLE_COLORS` y `COMMUNITY`.
  Los rasgos no vienen en el `READY`: se leen del perfil.
- **Lo que se escribe entero se lee fresco.** Los flags del canal del
  sistema (se cambian sólo los cuatro bits que RUNE muestra), los `features`
  y los `flags` de un foro (`1 << 4`, etiqueta obligatoria) se leen justo
  antes de escribir, como en Serein, para no pisar lo que RUNE no conoce.
  Perfil y servidor son dos escrituras: si la segunda falla después de la
  primera, el fallo queda ambiguo.
- **Antes de salir** (`admin::rest`): `MANAGE_SERVER` para perfil,
  seguridad e invitaciones; `MANAGE_CHANNELS` para el lote (y en la
  categoría destino) y para el foro; `MANAGE_ROLES` y la jerarquía para el
  rol, y además el `feature` del servidor para ícono o degradé; hasta 5
  rasgos de hasta 24 caracteres; canal del sistema de texto y de
  inactividad de voz, del servidor; tiempos 60, 300, 900, 1800 o 3600 s; una
  comunidad no baja de verificación baja ni deja de revisar a todos; hasta
  20 etiquetas, nombres cortos, las existentes por su id; ícono de rol sin
  animación; uno o dos colores de degradé; cada canal del lote una vez, una
  categoría nunca dentro de otra.
- **Deliberadamente no** (hasta el 2026-10-19, ver abajo): rasgos con
  emojis propios del servidor, reacción por defecto propia del servidor,
  activar comunidad, el canal de reglas y el de actualizaciones, la
  duración de archivado y el `default_tag_setting` de un foro.
- **Desde el 2026-10-18** (rama `admin-canales-y-voz`, sólo fixtures): la
  barra lateral también mueve en lote; `lock_permissions` va sólo con un
  `parent_id` y pide `MANAGE_ROLES` en el canal y en la categoría (el
  cliente oficial lo ofrece así, ver REA.md); los canales que la cuenta no
  ve nunca van en el lote: conservan su posición y los visibles se numeran
  alrededor.

Para probar contra el servicio (dueño, con un servidor de prueba propio):
cambiar el ícono del servidor y agregar, cambiar y quitar rasgos (y verlos
en el cliente oficial); cambiar canal y mensajes del sistema, notificaciones
por defecto, canal y tiempo de inactividad, verificación y filtro, y
comprobar que otros ajustes (por ejemplo un flag que RUNE no muestra)
siguen; pausar y reanudar las invitaciones; reordenar varios canales y
pasar uno a otra categoría en la página «Canales» y arrastrando en la barra
lateral (con «Sincronizar permisos» y con «Mover sin sincronizar», y ver en
el cliente oficial los permisos y el orden, también de un canal que la
cuenta no ve entre los movidos); arrastrar a alguien de un canal de voz a
otro; filtrar la auditoría por «Permisos» con permisos creados, cambiados y
quitados; buscar a alguien que no está cargado en el ⊕ de los permisos de
un canal; en un foro, crear,
renombrar, marcar como moderada y borrar etiquetas, poner y quitar la
reacción por defecto, cambiar vista, orden y etiqueta obligatoria; en un
servidor con mejoras, darle a un rol un emoji, una imagen y un degradé; ver
el conteo de miembros por rol; ordenar miembros de las cuatro maneras,
filtrar los últimos 7 días y ver cómo entró cada uno. Si Discord pide
hCaptcha en alguna, resolverlo en la ventana de RUNE y ver que se envía una
vez.

### Banner, ícono animado, emojis propios, comunidad y foro (2026-10-19, rama `admin-servidor-y-miembros`)

**Sólo fixtures en loopback; nada se probó contra Discord.** Serein no hace
banner, ícono animado ni comunidad: las rutas son las documentadas de
`PATCH /guilds/{g}` (como el resto de Seguridad). Archivado y modo de
etiquetas del foro, como Serein (`channel_actions.rs`, leído 2026-10-19).
Mismo camino que el resto de P7 (`admin::allowed`, un intento por
escritura, captcha humano de cualquier escritura).

| Acción | Ruta | Qué confirma el cambio |
| --- | --- | --- |
| Banner del servidor | `PATCH /guilds/{g}` `{"banner": "data:image/png;base64,…" o GIF, o null}` | la respuesta (`banner`, como `GUILD_UPDATE`) |
| Ícono animado | el mismo `PATCH /guilds/{g}/profile` del ícono, con un GIF | la respuesta (`icon_hash`) |
| Activar o desactivar la comunidad | `GET /guilds/{g}` y `PATCH /guilds/{g}` `{"features": [las leídas, con o sin "COMMUNITY"], "rules_channel_id", "public_updates_channel_id", "verification_level"?, "explicit_content_filter"?}`, todo en una escritura | la respuesta (`features`, canales) |
| Canales de la comunidad | `PATCH /guilds/{g}` `{"rules_channel_id"?, "public_updates_channel_id"?}` | la respuesta |
| Rasgo con emoji propio | `PATCH /guilds/{g}/profile` con `"emoji_id": "<id>", "emoji_name": "<nombre>", "emoji_animated"` | la respuesta (`traits`) |
| Reacción propia, archivado y modo de etiquetas de un foro | `PATCH /channels/{c}` `{"default_reaction_emoji": {"emoji_id": "<id>", "emoji_name": null}, "default_auto_archive_duration"?: 60, 1440, 4320 o 10080, "default_tag_setting": "match_some" o "match_all"}` | la respuesta (`CHANNEL_UPDATE`) |

- **Lectura.** `features` también da `ANIMATED_ICON`, `BANNER` y
  `ANIMATED_BANNER` (lo que desbloquean las mejoras); `banner`,
  `rules_channel_id` y `public_updates_channel_id` se leen del `READY` y de
  `GUILD_UPDATE`. Un rasgo con `emoji_id` ya no deja los rasgos sin leer:
  se lee con su id. El foro lee `default_reaction_emoji.emoji_id`,
  `default_auto_archive_duration` (sólo los cuatro valores) y
  `default_tag_setting`.
- **Antes de salir** (`admin::rest`): un GIF como ícono pide
  `ANIMATED_ICON`; un banner, `BANNER`, y uno que se mueve,
  `ANIMATED_BANNER`; un emoji de un rasgo o de la reacción del foro tiene
  que ser del servidor; los canales de la comunidad son de texto y del
  servidor; activar o desactivar la comunidad pide administrador (cambiar
  sus canales, `MANAGE_SERVER`); una comunidad, después del cambio, tiene
  que tener los dos canales, verificación al menos baja y el filtro para
  todos. Desactivarla conserva los demás `features`.
- **Interfaz.** Perfil: «Cambiar banner» y «Quitar banner» con su vista en
  la tarjeta; el ícono y el banner se eligen con la subida animada de la
  edición de perfil (un GIF sigue moviéndose sólo si las mejoras lo
  desbloquean; si no, se usa el primer cuadro y la página lo dice).
  Rasgos y reacción del foro aceptan `:nombre:` de un emoji del servidor.
  Seguridad: sección «Comunidad» con el interruptor (al activarla sube la
  verificación a baja si no tenía y pone el filtro para todos), el canal de
  reglas y el de novedades, y lo que falta. Foro: «Archivar después de» y
  «Filtrar por todas las etiquetas elegidas», que también usa el filtro de
  la vista del foro.

Para probar contra el servicio (dueño, servidor de prueba propio, con y sin
mejoras): poner, cambiar y quitar el banner (PNG y, con nivel 2, GIF) y un
ícono GIF; ver que sin mejoras RUNE usa el primer cuadro; activar la
comunidad eligiendo los dos canales desde un servidor sin verificación ni
filtro y comprobar en el cliente oficial que quedó con verificación baja y
filtro para todos; cambiar los canales; desactivarla y ver que el resto de
los `features` siguen; poner un rasgo con un emoji del servidor; en un
foro, una reacción propia, el archivado de una semana y «todas las
etiquetas».

## Corrección del runtime GStreamer (2026-10-14)

Dos intentos de abrir el acceso terminaron con `SIGABRT` en
`WebKitWebProcess` 2.52.4. Se consultaron los frames nativos que
`systemd-coredump` había incluido en el journal; no se abrieron volcados,
memoria ni variables locales. Ambos mostraban la misma ruta:
`HTMLMediaElement::loadResource` → `MediaPlayer::load` →
`MediaPlayerPrivateGStreamer::createAudioSink` → `WTFCrashWithInfo`.

El código de WebKitGTK 2.52.4 comprueba `RELEASE_ASSERT(audioSink)` después
de crear el sink. Su ruta de salida automática requiere `autoaudiosink`.
El shell disponía de bibliotecas de WebKit, pero no exponía los directorios
de plugins GStreamer: los inputs transitivos de la compilación de WebKit
no bastaban para descubrirlos al ejecutar RUNE.
[Creación del sink en WebKit](https://raw.githubusercontent.com/WebKit/WebKit/webkitgtk-2.52.4/Source/WebCore/platform/graphics/gstreamer/MediaPlayerPrivateGStreamer.cpp),
[selección del sink de plataforma](https://raw.githubusercontent.com/WebKit/WebKit/webkitgtk-2.52.4/Source/WebCore/platform/graphics/gstreamer/GStreamerCommon.cpp).

La reproducción usó un probe propio y HTML de audio sintético, sin cuenta
ni handoff. Con rutas de plugins vacías, faltaban `appsink` y
`autoaudiosink`: el proceso web terminaba como `Crashed`, salida 1 del
probe. El mismo binario y HTML, con plugins base y good visibles desde el
shell corregido, sobrevivieron los 20 segundos de observación y el probe
terminó con salida 0. Esto reproduce y corrige el fallo de inicialización
multimedia; no valida el login contra Discord.

`shell.nix` incorpora GStreamer y los paquetes base, good y bad únicamente
cuando `withLogin = true`. El hook del paquete GStreamer configura
`GST_PLUGIN_SYSTEM_PATH_1_0`; los plugins quedan fuera de `runtime`, por lo
que este cambio conserva el RPATH y `RUSTFLAGS`. Base aporta
`appsink`/`appsrc`; good aporta `autoaudiosink`. Bad completa dos elementos
que WebKit solicitó durante la reproducción: `fakevideosink`, usado en su
ruta de audio, y `webvttenc`, usado por el combinador de subtítulos. No se
agregaron paquetes libav o ugly.
[Rutas de descubrimiento](https://gstreamer.freedesktop.org/documentation/gstreamer/running.html),
[autoaudiosink](https://gstreamer.freedesktop.org/documentation/autodetect/autoaudiosink.html),
[fakevideosink](https://gstreamer.freedesktop.org/documentation/debugutilsbad/fakevideosink.html),
[webvttenc](https://gstreamer.freedesktop.org/documentation/subenc/webvttenc.html),
[combinador de subtítulos de WebKit](https://raw.githubusercontent.com/WebKit/WebKit/webkitgtk-2.52.4/Source/WebCore/platform/graphics/gstreamer/TextCombinerGStreamer.cpp).

Con registros temporales nuevos, `gst-inspect-1.0` verificó las factories
`appsink`, `appsrc`, `autoaudiosink`, `giostreamsrc`, `decodebin`,
`playbin3`, `audioconvert`, `audioresample`, `fakevideosink` y `webvttenc`
en GStreamer 1.26.11. La regresión nativa del auxiliar usa un WAV PCM
silencioso generado para el fixture, sin red ni reproducción: entrega el
resultado sintético solo después de recibir metadatos válidos del audio.
Conserva las opciones del motor usadas en producción.

El auxiliar también distingue caída del motor, límite de memoria,
terminación solicitada por API y causa desconocida mediante mensajes
estáticos. El arreglo de plugins no cambia User-Agent, TLS, sandbox ni
configuración de GPU. El dueño confirmó después que el acceso llegó a
«Identidad verificada»; no se atribuye este fallo a NVIDIA ni a controles de Discord.

## Validación controlada (2026-10-14)

Los conteos siguientes corresponden a la validación anterior a la
corrección de GStreamer; se conservan como evidencia de esa etapa.

- Workspace con todas las features: `cargo check`, Clippy con `-D warnings`
  y 364 pruebas aprobadas, tres fixtures nativos ignorados en la ejecución
  automática. Estos últimos se ejecutaron por separado en la sesión gráfica.
- Workspace sin features opcionales: `cargo check`, Clippy con `-D warnings`
  y 362 pruebas aprobadas. `cargo fmt --all -- --check` y `git diff --check`
  también pasaron. Release compilado con `session-login`.
- REA ejecutó el release con `--provider discord-session --screenshot` en
  Wayland: exit 0, captura inicial revisada, sin iniciar sesión. Evidencia en
  [REA.md](REA.md#proveedor-de-sesión-personal-opcional-2026-10-14).
- Adaptador: 16 pruebas de contrato y HTTP local, más dos pruebas de
  compilación que impiden clonar o serializar `SessionSecret`.
- Auxiliar: 11 pruebas con `webview` e `--include-ignored`, incluyendo
  fixtures nativos en Wayland con WebKitGTK 2.52.4. El HTML local lleva CSP
  sin red y nunca envía la petición sintética. Se comprueban el handoff
  esperado, la salida del auxiliar y la terminación de sus procesos WebKit
  observados al cancelar. El cambio de generación se prueba en el estado
  Rust; no se presenta como una prueba nativa de todas las carreras de
  navegación.
- Puente JavaScript: fixture Node aprobado con valores sintéticos.
- Auditoría Cargo con base RustSec local: 577 entradas, cero avisos
  conocidos. No cubre bibliotecas nativas; sus licencias y el alcance se
  detallan en [SECURITY.md](SECURITY.md).

### Corrección de GStreamer y pantalla de acceso

La validación posterior del mismo día pasó `cargo check --workspace
--all-features`, Clippy para todos los targets con `-D warnings` y **370
pruebas**, con cinco fixtures nativos ignorados en esa ejecución general.
La suite del auxiliar se ejecutó por separado con `--include-ignored`:
**16 pruebas unitarias y nativas aprobadas**, incluida la carga multimedia
antes de entregar el resultado sintético. El nuevo fixture se aísla en un
subproceso porque GTK no permite inicializarse en distintos hilos del
harness de Rust.

La pantalla de acceso sigue implementada con limn: tarjeta compacta, fondo
gris oscuro, acento azul violeta y marca original RUNE. Conserva errores
desplazables, navegación por teclado, cancelación y el alcance del proveedor
visible en esa revisión. En la revisión siguiente, por indicación del dueño,
se retiró esa repetición del acceso y se trasladó a Ajustes → Conexión.
Estas pruebas automáticas son independientes de la confirmación manual
posterior del login por el dueño.

Para repetir la comprobación nativa, desde la sesión gráfica propia:

```sh
nix-shell --arg withLogin true --run 'cargo test -p rune-login --features webview -- --include-ignored --test-threads=1'
node crates/rune-login/tests/handoff.cjs
```

El login y la lectura de servidores, canales e historial están confirmados
por el dueño. Las operaciones de escritura quedan separadas y pendientes
de validación real.
No se reutilizan resultados de OAuth2 como evidencia de este proveedor.
Un rechazo
de la sesión personal sin imitar el cliente oficial
queda documentado como incompatibilidad; no se convierte en una promesa de
acceso completo a Discord.

## Navegación y chat (2026-10-14)

`bootstrap.rs` traduce `READY` al contrato común `Snapshot`: servidores,
categorías, canales de texto/anuncios, canales de voz como navegación y DMs
existentes. Los tipos de canal que el dominio todavía no representa se
omiten. Anuncios, foros (también los de medios) e hilos activos tienen tipo
propio; un hilo hereda el acceso de su canal padre y se responde en él con
`SEND_MESSAGES_IN_THREADS` (ver [Hilos y foros](#hilos-y-foros-2026-10-17)).
Sin prueba real todavía.
La variante personal de `READY` (`properties`, `merged_members`,
`recipient_ids`, `read_state`) se estudió en el código público de Serein,
sin copiarlo. Es evidencia de esa implementación, no una garantía de
compatibilidad con el servicio ni una interfaz oficialmente autorizada.

Los bits de permisos se traducen explícitamente a los de RUNE. Roles,
sobrescrituras y membresía propia determinan la visibilidad y las acciones.
No se heredan los permisos predeterminados del proveedor simulado. Datos de
permisos ausentes dejan el canal sin acceso; datos malformados detienen la
sesión en vez de conservar permisos antiguos. Los cambios de roles, canales
y membresía recalculan los permisos antes de aceptar la siguiente acción.
Desde el recorrido 15, `communication_disabled_until`, `pending` y el bit 7
de `flags` (cuarentena automática) llegan al dominio como
`MemberRestrictions`; una fila que omite esos campos conserva lo conocido y
un valor malformado termina la sesión. La membresía restringida ya no se
omite: sus canales se leen, y tanto `Store::abilities` como la compuerta REST
rechazan escribir, editar, reaccionar o votar mientras dure (borrar un mensaje
propio sigue permitido). `LoadProfile` ahora consulta el perfil completo por
REST (ver [Perfil completo y lista de miembros](#perfil-completo-y-lista-de-miembros-2026-10-16)).

El transporte conserva heartbeat, secuencia y datos de reanudación dentro
del adaptador. Usa propiedades que identifican RUNE, JSON sin compresión,
límites de tamaño/cola, plazos y reintentos acotados. No aplica los campos de
fingerprint o suplantación de cliente presentes en otras implementaciones.
Un rechazo de autenticación, incompatibilidad o rate limit termina el
intento con texto local; no expone cuerpos remotos ni redirecciones.

REST pagina el historial al elegir una conversación. Enviar, editar y
borrar corresponden a comandos explícitos de la UI, con respuestas y errores
traducidos a eventos de dominio. Las escrituras no se repiten automáticamente
si el resultado es ambiguo. La caché conserva 512 mensajes; los patches
tipados llegan al dominio aunque su baseline ya no esté en ese caché, sin
borrar campos ausentes ni reasignar autoría. Una edición/borrado que necesita
recuperar autoría usa un GET de hasta 1 MiB y 15 s; vuelve al loop para validar
cuenta, canal y permisos antes de escribir. Una prueba pequeña de autoría
sobrevive a otras expulsiones sin ampliar la caché. Borrado del destino,
revocación y cambio de sesión cancelan la recuperación; los comandos aún en
cola no se intentan después de un borrado conocido. Se conservan cola16 y
una operación REST en vuelo. Un moderador ya autorizado no necesita ese GET.
La capacidad independiente `message_context` permite una lectura alrededor
del ID exacto con `around` (sin mezclar `before`/`after`), límite 1–100,
respuesta de hasta 8 MiB y plazo de 15 s. El adaptador exige canal, IDs únicos,
orden y presencia del ancla; una lista vacía no se interpreta como borrado.
Cancelación, desconexión, revocación y mutaciones del mismo canal invalidan
la lectura en cola o en vuelo antes de repoblar la caché de autoría. La UI
normal solicita como máximo 51 mensajes y no deduce cobertura de historial.
Logout
cancela Gateway, operaciones HTTP y tareas pendientes antes de descartar la
sesión. No se anuncian archivos, voz o administración como capacidades
disponibles.

Reacciones, typing y fijados usan la misma cola, la misma revalidación de
permisos y un solo intento por acción. Una reacción lleva el estado pedido
(`add`); si el mensaje no está en la caché, un GET acotado lo recupera antes,
sólo para validarlo; el 204 y
el eco `MESSAGE_REACTION_*` del Gateway comparten un registro, así que el
cambio propio cuenta una sola vez llegue primero cualquiera de los dos. Sólo
reacciones Unicode: `PUT`/`DELETE
/channels/{c}/messages/{m}/reactions/{emoji}/@me` con el emoji escapado
entero. Typing es `POST /channels/{c}/typing`, sin cuerpo y sin evento de
fallo; uno que esperó más de 5 s en cola se descarta. `TYPING_START` de otra
persona en un canal visible llega como `Event::Typing`. Fijar usa las rutas
documentadas `PUT`/`DELETE /channels/{c}/messages/pins/{m}`: en un DM basta
estar en él; en un servidor se exigen `PIN_MESSAGES` (bit 51) y
`READ_MESSAGE_HISTORY`, como el cliente oficial; `MANAGE_MESSAGES` solo ya
no alcanza (ver la sección del 2026-10-18). El 204 confirma con un `MessagePatched { pinned }`, igual
que el `MESSAGE_UPDATE` posterior; `CHANNEL_PINS_UPDATE` no trae qué mensaje
cambió y no se usa. La lista de fijados lee `GET
/channels/{c}/messages/pins` en páginas de 50 hasta 250; más que eso se
rechaza en vez de mostrarse cortada.

El [checkpoint del 2026-10-15](evidence/message-patches-checkpoint-2026-10-15.json)
registra 781 pruebas de workspace, formato/Clippy/release correctos y replay REA
de 131 pruebas del adaptador, con fixtures sintéticos HTTP/Gateway. Las dos
pruebas de documentación completan las 133 de su ejecución dirigida. No se
realizaron operaciones nuevas sobre una cuenta real.

El [checkpoint de contexto](evidence/search-context-checkpoint-2026-10-15.json)
posterior registra 851 pruebas de workspace y replay REA de 150 pruebas
unitarias del adaptador, además de dos doctests verificados con Cargo. La
capacidad `message_context` quedó implementada y validada con HTTP/Gateway
sintéticos; no se atribuye una nueva prueba sobre una cuenta real. La búsqueda
completa del proveedor sigue sin habilitarse.

El contrato común de lectura posterior separa avance local, recordatorio manual
y confirmación versionada por cuenta/canal/petición. La bandeja y las marcas
persistidas se verifican con el proveedor simulado. Hasta el 2026-10-17 este
adaptador conservaba `read_state=false`; desde entonces envía y recibe
confirmaciones (ver [Lecturas y búsqueda](#lecturas-búsqueda-y-miembros-remotos-2026-10-17)),
sólo con fixtures. No habilita SQLite.

Fuentes para los objetos y el comportamiento documentado del protocolo:
[Gateway](https://docs.discord.com/developers/events/gateway),
[eventos](https://docs.discord.com/developers/events/gateway-events),
[canales](https://docs.discord.com/developers/resources/channel),
[mensajes](https://docs.discord.com/developers/resources/message) y
[permisos](https://docs.discord.com/developers/topics/permissions).
La existencia de estos objetos públicos no autoriza el uso de credenciales
de sesión personal con ellos.

La presencia sigue deshabilitada como capacidad. Los fragmentos de
membresía recibidos sirven para calcular permisos. Desde el 2026-10-16 la
lista de miembros llega por suscripción de Gateway (sección siguiente); sus
filas no entran en la instantánea de permisos.

### Validación de esta etapa

El 2026-10-14 pasaron `cargo fmt --all -- --check`,
`cargo check --workspace --all-features`,
`cargo test --workspace --all-features` (**424 aprobadas, 5 ignoradas,
23 suites**) y `cargo clippy --workspace --all-targets --all-features --
-D warnings`, dentro de `nix-shell --arg withLogin true`.

Las regresiones cubren la entrada al workspace al autenticar, logout y
limpieza del estado, controles según capacidades, permisos de roles y
sobrescrituras, membresías contradictorias, duplicados en datos
suplementarios, historial y mensajes HTTP locales, heartbeat, reanudación,
una ráfaga de 96 eventos, colas acotadas y cancelación. Los cinco fixtures
nativos ignorados no se reejecutaron en esta etapa: el auxiliar de login no
cambió respecto de la validación anterior.

La actualización de fuentes de limn tiene una PR separada:
[#38](https://github.com/Niiihuel/limn/pull/38). El lock de RUNE apunta a la
integración local `896c3dc0f06bcb894fffff17c47677aeaea166b5`; las pruebas
sintéticas verifican alfabetos matemáticos estilizados, sin leer el nombre
privado del dueño. Ver [FONTS.md](FONTS.md).

El release con `session-login` también compiló. REA comprobó el arranque en
Wayland y la pantalla inicial, salida 0, con captura revisada y sin iniciar
sesión: [evidencia del artefacto actual](REA.md#entrada-al-workspace-y-chat-artefacto-actual-2026-10-14).
La nueva versión se dejó abierta para la prueba manual del dueño.

### Confirmación manual de lectura

El dueño confirmó después el acceso a servidores, canales e historial y
aportó una captura de su sesión. Esa observación acredita la navegación y
lectura en su prueba; no demuestra el envío, edición, borrado, reconexión de
largo plazo ni todas las variantes de cuenta. No se incluyeron datos ni
recursos privados de esa captura en los fixtures o la documentación.

## Composer y expresiones

El catálogo custom llega por el estado inicial y sus actualizaciones; el
adaptador valida IDs, nombres, roles y disponibilidad y construye una
inserción opaca. La UI busca en el catálogo del servidor actual y permite
completar `:` o seleccionar una tarjeta. Las imágenes del catálogo usan una
vista previa WebP estática. El picker muestra hasta 72 coincidencias y
avisa cuando hay más; busca sobre un catálogo limitado a 500 entradas.
Seleccionar Unicode, custom o un enlace GIF inserta en el cursor del
composer o reemplaza su selección, conservando el resto del texto. Una
posición guardada de otro canal o borrador se descarta y la inserción pasa
al final. El cursor queda después del contenido insertado.
Desde el 2026-10-17 un emoji de servidor recibido se dibuja como imagen en
la línea, animado si se mueve
([Emoji en línea](#emoji-en-línea-y-el-ícono-de-la-aplicación-2026-10-17-rama-emoji-en-linea)).
Desde P5 (2026-10-17), con el plan de la cuenta conocido (`Event::Perks`),
el selector ofrece también los emojis de los otros servidores, con candado
y motivo en lo que el plan no permite
([EXPRESSIONS.md](EXPRESSIONS.md#plan-de-la-cuenta-paquetes-y-gif-del-servicio-2026-10-17-p5)).

La pestaña Stickers muestra los del servidor actual; con el plan conocido,
también los de los otros servidores, y al final los paquetes estándar. Se eligen
PNG, APNG, GIF y Lottie; en el timeline los APNG y GIF animan. Un Lottie trae
su JSON en `cdn.discordapp.com/stickers/{id}.json` y se dibuja y anima desde
el 2026-10-18 ([EXPRESSIONS.md](EXPRESSIONS.md#stickers-lottie-2026-10-18-rama-video-y-documentos);
sólo fixtures); un sticker de paquete sin dibujo muestra su nombre y se
envía igual. Elegir una tarjeta prepara la selección y
**Enviar sticker** realiza la acción explícita. Antes de enviarlo se
revalidan canal, capacidad y catálogo; el texto del borrador no se borra.
Cambiar de canal descarta el sticker pendiente. Enviar stickers tiene
pruebas locales, sin envío real confirmado todavía.

Las referencias recibidas a usuarios, roles y canales se muestran con
nombres cuando están disponibles en el estado. Un emoji de servidor
recibido se dibuja como imagen; `:nombre:` queda sólo como respaldo, por
ejemplo en lo guardado antes sin imagen. El texto fuente se conserva
para editar; la representación visual no agrega permisos ni notificaciones.
El autocompletado saliente de menciones de la simulación no se presenta como
una integración verificada de notificaciones de cuenta.

La pestaña GIF inserta un enlace HTTPS en el borrador. Desde P5
(2026-10-17) busca con las rutas `/gifs/*` del servicio (capacidad
`gif_search`: tendencias, categorías y sugerencias, con vistas previas
animadas; [EXPRESSIONS.md](EXPRESSIONS.md#plan-de-la-cuenta-paquetes-y-gif-del-servicio-2026-10-17-p5)).
Insertar texto tampoco envía el mensaje: la persona conserva el
control del envío normal del composer.

Los tests sintéticos cubren catálogo malformado, snapshot antiguo, texto
opaco, selección y confirmación separadas, capacidad revocada, índice
obsoleto, límites del borrador, foco y teclado. No constituyen validación
real de envío. Evidencia pública, licencias y limitaciones de los formatos
en [SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md#continuidad-composer-expresiones-e-imágenes-2026-10-14).

## Avisos del sistema de Discord (2026-10-16)

La cuenta oficial «Discord» manda sus avisos por mensaje directo como
tarjetas de un tipo propio, que RUNE dibujaba crudas: campos
`client_version_message`, `body`, `icon_type`, `header`, `timestamp` (época
en segundos con decimales), `theme`, `ctas`, `learn_more_link` y
`classification_id`. Serein no trata este tipo (sólo `poll_result` y una
nota para tipos no admitidos); el nombre del tipo sale de la documentación
comunitaria de embeds, no de una captura de tráfico.

`rich.rs` reconoce `safety_system_notification` y `safety_policy_notice`
como `EmbedKind::SystemNotice`: `header` pasa a título, `body` a texto,
`timestamp` a fecha y `learn_more_link` a dirección sólo si es http(s) de
`discord.com`, `discordapp.com`, `discord.gg` o `dis.gd` (o subdominios).
Los campos de máquina se descartan; las tarjetas de bots con los mismos
nombres de campo los conservan. Otros tipos desconocidos pasan a
`EmbedKind::Other`: se dibujan como tarjeta con la nota «RUNE todavía no
muestra todo lo que trae este tipo de tarjeta». La UI dibuja el aviso con su
marca, título, texto, fecha y «Más información», que abre la página por el
manejador de enlaces del timeline ([captura simulada](img/system-notice-2026-10-16.png)).

## GIF de tarjetas, reenvíos con archivos y adjuntos que se reproducen (2026-10-17)

Sólo fixtures; **sin probar contra el servicio real**. Con Serein como
referencia (sin copiar código):

- **`gifv`**: `rich.rs` guarda `video.proxy_url` como `Embed::video` con
  `ImageKind::Clip`, con la misma validación que las imágenes de tarjeta
  (HTTPS, `media.discordapp.net` o `images-ext-1/2.discordapp.net`, ruta
  `/external/` o `/attachments/`, lados de 1 a 4096 px). `video.url` (Tenor
  o Giphy directamente) nunca se usa: el tercero no ve quién mira.
- **Reenvíos**: los archivos de `message_snapshots[0].message.attachments`
  conservan `url` como descarga y vista previa, validados contra
  `message_reference.channel_id`. Discord los direcciona como
  `/attachments/{canal}/{mensaje}/{archivo}/{nombre}` (la forma que Serein
  acepta para descargarlos); `wire::attachment_url` acepta esa forma
  además de la de tres partes. Una dirección de otro canal pierde su
  enlace, no la tarjeta.
- **Adjuntos de audio y video**: se reconocen por `content_type` o nombre
  en la interfaz; se descargan por la misma ruta validada de los mensajes
  de voz (`cdn.discordapp.com`/`media.discordapp.net`, firmas `ex/is/hm`
  tal como vienen), hasta 50 MiB.
- **Spoilers de tarjetas**: no hay campo del servicio; la interfaz tapa la
  tarjeta cuyo `url` aparece entre `||` en el contenido, como el cliente
  oficial, y las imágenes de una tarjeta con `||` en su texto.

## Tarjeta de perfil con una cuenta real (2026-10-17, rama `perfil-real`)

Lo que cambió en el proveedor no oficial tras la primera prueba del dueño
con su cuenta (síntomas y arreglos de la interfaz en
[PROFILES.md](PROFILES.md#primera-prueba-con-una-cuenta-real-2026-10-17-rama-perfil-real);
lo visto en el cliente oficial en
[REA.md](REA.md#cliente-web-oficial-popout-de-perfil-2026-10-17)).
**Sólo fixtures, sin probar contra el servicio real.**

- **Emoji de la biografía** (`emoji::pictures`): los `<:nombre:id>` y
  `<a:nombre:id>` de `bio` (cuenta y servidor) se resuelven a
  `cdn.discordapp.com/emojis/{id}.webp?size=48`, con `&animated=true` si
  se mueve (WebP animado, que el caché ya decodifica); a lo sumo 64, uno
  por token, nombre de 2 a 32 caracteres y un id sin ceros a la izquierda.
- **Estado personalizado**: el emoji Unicode (`emoji.name` sin `id`) va
  antes del texto, o solo. El de un servidor va, desde la rama
  `emoji-en-linea` (2026-10-17), en `Presence.custom_emoji` con su imagen
  ([abajo](#emoji-en-línea-y-el-ícono-de-la-aplicación-2026-10-17-rama-emoji-en-linea)).
- **Imágenes de actividades**: `mp:` acepta la codificación por ciento de
  las direcciones externas (antes se rechazaban) y sigue rechazando `?`,
  `#`, espacios, `..` y `\`; pide `format=png` (un `.gif`,
  `format=webp&animated=true`, como el cliente oficial) con
  `width=160&height=160` para entrar en el presupuesto de ilustraciones
  (512 px, 1 MiB). `app-assets` sigue a 128 px. `spotify:`, `twitch:` y
  `youtube:` no se cargaban (otros hosts, en JPEG); desde el 2026-10-18 sí,
  con los límites de un banner ([abajo](#portadas-nameplates-por-sku-y-cosméticos-2026-10-18-rama-perfil-tarjeta-y-cosmeticos)).
- **Efectos**: cada capa conserva `start`, `duration`, `loop` (sin largo,
  se queda) y `loopDelay`; el ancho y alto de una capa no vienen en la
  configuración (se dibujan al ancho de la tarjeta con su forma). Para
  movimiento reducido se prefiere `reducedMotionSrc`, como el cliente
  oficial, y después `staticFrameSrc`.
- **Gente en voz**: un `VOICE_STATE_UPDATE` con `member` emite
  `UserChanged` y `MemberChanged` antes de que la llamada lo lea
  (`members::in_voice`), salvo los de la cuenta; uno roto no dice nada.
- **Presencia de quien muestra su tarjeta** (`Command::WatchPeople`): las
  personas van en `members` del op 37 de la lista suscrita (a lo sumo 8,
  sólo del servidor de esa lista; otro servidor las vacía) y su
  `PRESENCE_UPDATE` se acepta aunque no tengan fila. Sin la lista abierta
  no se suscribe a nadie (shortcut: el op 37 de una persona sin lista
  necesitaría un canal; el popout oficial lo hace igual con su servidor).

Tests: `emoji` (tokens), `profiles` (biografía con emoji), `identity`
(`mp:` y GIF), `members_tests` (estado con emoji, miembro del estado de
voz, suscripción de personas) y `tests/session/identity.rs` (tiempos del
efecto y su imagen quieta).

Para probar con la cuenta real: abrir una tarjeta desde la lista en un
servidor grande y dejarla abierta un rato; abrir la de alguien en voz
(avatar, banner, estado, «Miembro desde», nota); una biografía con emoji
animados; una actividad con imagen (`mp:`) y otra sin grande; un perfil con
efecto (y con movimiento reducido); un nombre o etiqueta con caracteres
coreanos, japoneses o «ㅤ».


## Emoji en línea y el ícono de la aplicación (2026-10-17, rama `emoji-en-linea`)

Lo que cambió en el proveedor no oficial para dibujar los emoji de un
servidor como imagen (interfaz en
[PROFILES.md](PROFILES.md#emoji-en-línea-2026-10-17-rama-emoji-en-linea);
lo visto en el cliente oficial en
[REA.md](REA.md#cliente-web-oficial-emoji-en-línea-y-el-ícono-de-la-aplicación-2026-10-17)).
**Sólo fixtures, sin probar contra el servicio real.**

- **Mensajes** (`text::references`): un `<:nombre:id>` o `<a:nombre:id>`
  lleva su imagen, `cdn.discordapp.com/emojis/{id}.webp?size=48` con
  `&animated=true` si se mueve (`emoji::inline`, la misma de la biografía).
  Lo guardado antes (sin imagen) se sigue leyendo y muestra `:nombre:`.
- **Estado personalizado de otros** (`members::presence`): el emoji con
  `id` del estado (tipo 4) va en `Presence.custom_emoji` (nombre de 2 a 32
  caracteres `[A-Za-z0-9_]`, `animated` del objeto); el texto queda sin él.
- **Estado propio** (ajustes protobuf, campo 2 `emoji_id` y 3 nombre): se
  lee como `server_emoji` (antes se descartaba); escribir el estado lo
  conserva (antes, cambiar de estado lo borraba) y la presencia del Gateway
  lo manda con su `id`. Los ajustes no dicen si se mueve: se muestra quieto.
- **Tema del canal** (`bootstrap::channel` y `CHANNEL_UPDATE`): sus
  `<:nombre:id>` van en `Channel.topic_pictures` (`#[serde(default)]`), como
  los de la biografía; se rehacen cuando cambia el tema.
- **Ícono de la aplicación** (`app_icons`): una actividad sin imagen grande
  ni chica, con `application_id`, se completa con
  `GET /applications/{id}/public` → `icon` (32 hex) →
  `cdn.discordapp.com/app-icons/{id}/{icon}.png?size=128`, como el cliente
  oficial. Una lectura a la vez, aparte de la cola de REST; cada aplicación
  una vez por sesión, también si falla (como el oficial, que no reintenta);
  la presencia que esperaba se vuelve a contar con el ícono. Cotas: 512
  aplicaciones, 64 en cola, 256 personas esperando, respuesta de 256 KiB.
  Un 429 o un 401 en esa lectura cuentan como «sin ícono»; nunca cortan la
  sesión ni avisan a la interfaz.

Tests: `text` (emoji con su imagen), `bootstrap` (emoji del tema),
`members_tests` (estado con emoji de
servidor), `settings_proto` (estado propio con emoji de servidor que
sobrevive a un cambio de estado), `app_icons` (una lectura, falla
guardada, la presencia más nueva gana), `tests/rest.rs` (la ruta) y
`tests/session/identity.rs` (de punta a punta: presencia, lectura y
presencia con el ícono).

Para probar con la cuenta real: un mensaje con emoji de un servidor
(quieto y animado) y uno que sea sólo emoji (grande), en modo cómodo y
compacto; una respuesta a un mensaje con emoji; seleccionar y copiar un
mensaje con emoji (se copia `:nombre:`); una biografía con emoji, negrita y
saltos de línea; un canal con un emoji de servidor en su tema (encabezado
y «Tema del canal»); alguien con un emoji de servidor en su estado, y el
propio (cambiar de «En línea» a «Ausente» y ver que el emoji sigue, también
en el cliente oficial); alguien jugando algo sin imagen (por ejemplo, una
aplicación de escritorio), para ver su ícono.

## Llamadas privadas (2026-10-17, rama `llamadas-privadas`)

Llamadas en privados y grupos, como en Serein y en el cliente oficial.
**Sólo fixtures locales; ninguna llamada privada se probó contra Discord.**
Capacidad `private_calls`; la interfaz pregunta a `Store::abilities`
(`ring`, y `join_voice`/`speak`/`share_screen`… que en un privado no piden
permisos de canal) y a `Store::call_status`. Evidencia: REA estática del
bundle web público `web.c5b338cc7e610833.js` (SHA-256 `6f842876…0aec`; el
mismo código en `web.4d4b2f75fdea5470.js`, `667a771c…084ae`) y Serein
(`discord-gateway/src/voice.rs`, `discord-api/src/lib.rs`,
`client-core/src/voice.rs`, `ui/src/voice.rs`), sin copiar código; detalle
en [REA.md](REA.md#cliente-web-oficial-llamadas-privadas-2026-10-17).

Qué hace RUNE (`voice/private.rs`, `voice/mod.rs`, `gateway.rs`,
`http/calls.rs`):

- **Llamar.** El teléfono de la cabecera manda `JoinVoice` del privado y
  `RingCall` sin personas. Antes del op 4 sale el **op 13**
  `{channel_id}` (lo que Discord sabe de esa llamada; Serein lo manda antes
  de cada unión, el oficial al abrir un privado que no conoce) y el
  **op 4** lleva `guild_id: null`. El ring **espera a que Discord tenga la
  llamada**: sale con su `CALL_CREATE`, como el oficial (que lo encola con
  `CALL_ENQUEUE_RING`), como `POST /channels/{id}/call/ring
  {"recipients": null}` por la cola REST común (así hereda la ventana del
  captcha). «Volver a llamar» repite el ring; «Dejar de llamar» en el
  mosaico de quien suena es `POST …/call/stop-ringing {"recipients": [id]}`.
- **Op 13 también** la primera vez que se lee el historial de un privado en
  cada sesión del Gateway, y de nuevo para el último pedido al reconectar
  (como el oficial con el canal abierto).
- **Llamadas.** `CALL_CREATE`/`CALL_UPDATE`/`CALL_DELETE` (ahora en la lista
  de despachos del Gateway). A quién suena se lee de `ongoing_rings` (el
  oficial: claves = a quién suena, valor = quién llamó; un valor nulo no
  suena) **y** de `ringing` (Serein); si no viene ninguno, queda como estaba.
  Sólo cuentan la cuenta y la gente de esa conversación (hasta 64). Los
  `voice_states` de `CALL_CREATE` entran como estados de voz.
  `CALL_DELETE` termina la llamada (con `unavailable: true` también, con
  «La llamada dejó de estar disponible en Discord.»), saca a quien quedara
  y, si la cuenta estaba, sale con op 4 nulo como el oficial.
- **Entrante.** Suena si la cuenta está en `ringing`/`ongoing_rings` y no
  está en esa llamada: aviso arriba con Aceptar (op 4 al privado; el ring
  para la cuenta lo saca Discord) y Rechazar (`stop-ringing` con
  `{"recipients": ["<cuenta>"]}`, sin op 4, como Serein; la ruta de rechazo
  del oficial está en un chunk que no se bajó). Sonido `ring` cada 2,5 s y
  notificación del sistema si la ventana no tiene el foco; ninguno de los
  dos en No molestar (el oficial suprime todo en No molestar).
- **Voz.** `VOICE_SERVER_UPDATE` de un privado trae `guild_id` nulo y su
  `channel_id`; la conexión de voz identifica con `server_id` = el canal
  (el oficial: «stream server id, si no guild, si no canal»; Serein igual)
  y el grupo DAVE es el canal. Transmitir: op 18 `{"type": "call",
  "guild_id": null, …}` y claves `call:{canal}:{dueño}` para op 19/20/21/22.
- **Estados** (`CallStatus`): sonando, conectando (fase de la llamada),
  en vivo, sin respuesta, rechazada, la otra persona cortó, sola, entrante,
  en curso sin la cuenta, y en curso en otro dispositivo (el estado de voz
  de la cuenta está en ese privado sin llamada de este dispositivo; si la
  llamada de este dispositivo pasa a otra sesión, termina como en los
  canales). **Rechazada vs. sin respuesta es una inferencia**: Discord sólo
  saca a la persona de `ringing`; si eso pasa antes de 20 s sin que entre,
  RUNE dice «rechazó»; si después, «sin respuesta». Ni el oficial (en lo
  que se bajó) ni Serein lo distinguen, y no hay plazo de ring del cliente:
  lo decide el servidor.

**Diferencias a propósito.** Al colgar no se manda `stop-ringing {}` (Serein sí):
Discord termina la llamada cuando queda vacía. Sin `flags` ni
`preferred_region` en op 4/18. Sin vista previa de transmisiones en
privados (sólo existe para servidores).

**Diagnóstico** (`rune: voice private call: …`, además de las líneas de
`voice call` y `gateway` de siempre; sin ids, tokens, sesiones ni región):

| Línea | Qué dice |
| --- | --- |
| `voice private call: asking the Gateway what it knows of the call (op 13)` / `gateway: op 13 sent: …` | se pidió el estado de la llamada |
| `gateway: op 4 sent: join or stay in a voice channel …` | se unió (en un privado el `guild_id` va nulo) |
| `voice private call: ring held until the service has the call (CALL_CREATE)` | el ring espera |
| `voice private call: CALL_CREATE: rings N (…), M in it` | Discord tiene la llamada: a cuántos suena y si es entrante |
| `voice private call: the service has the call: ringing (call/ring)` / `ringing (call/ring)` | el ring salió por REST |
| `voice private call: CALL_UPDATE: rings N (…)` | cambió a quién suena (contestó, rechazó o se cansó) |
| `voice private call: CALL_DELETE[: unavailable …]` | la llamada terminó |
| `voice private call: may it ring (GET /call)? ringable: true\|false` / `… refused: {motivo}` / `… failed: {motivo}` | lo que respondió Discord antes de llamar en un privado 1:1 (2026-10-18): `refused` si lo rechazó (400, 403 o 404), `failed` si no llegó la respuesta, hubo un límite o un error suyo |
| `voice call: VOICE_SERVER_UPDATE: …` | el servidor de voz, como en un canal |

Tests: `voice/tests/private.rs` (unirse sin guild, ring retenido hasta
`CALL_CREATE`, `server_id` = canal, `ongoing_rings`/`ringing`, claves
`call:`, `CALL_DELETE` y diagnóstico sin secretos; entrantes; cuerpos de
`call/ring` y `stop-ringing`), `gateway_tests.rs` (op 13 antes de op 4,
`guild_id` nulo, op 18 de tipo `call`, despachos `CALL_*`) y
`tests/session/voice.rs` (de punta a punta: op 13, op 4, `CALL_CREATE`,
`POST /call/ring` y salida).

**`ringable` y «no son amigos» (2026-10-18, rama `llamadas-y-panel-de-voz`;
sólo fixtures).** Como el cliente oficial (módulo 387755 de su bundle web,
[REA.md](REA.md#cliente-web-oficial-llamadas-privadas-2026-10-17)), antes
de llamar en un privado 1:1 se pide `GET /channels/{id}/call`
(`Command::CheckCall`, capacidad `call_check`, por la cola REST común y sin
ambigüedad: es una lectura). Con respuesta, RUNE entra (op 13 y op 4) y
hace sonar sólo si `ringable` es `true` (ausente cuenta como `false`, como
lo lee el oficial); si no suena, avisa «La llamada no va a sonar». Si
Discord rechaza el pedido, no entra ni hace sonar: el diálogo «No se puede
llamar a {nombre}» («Solo se puede llamar a amigos.» y el motivo que dio
Discord) ofrece «Enviar solicitud de amistad» (o «Aceptar solicitud de
amistad» si la mandó la otra persona; nada si ya se mandó). El oficial
muestra ese diálogo con cualquier falla del GET; RUNE lo muestra sólo si
Discord rechazó el pedido (400, 403 o 404: `RestError::refused`, llevado
como `Failure::CallCheck { refused: true }`) y la otra persona no es amiga.
Sin conexión, con la cola llena, un 429, un 5xx o una respuesta ilegible es
un aviso con el motivo, sin ofrecer amistad. Con `ringable` falso, «Volver a
llamar» tampoco suena mientras dure esa llamada. Los grupos no se preguntan (el oficial tampoco). La simulación:
amigos suenan, con una solicitud de por medio entra sin sonar y con el
resto rechaza («Llamada simulada: no son amigos.»);
[captura](img/llamadas-no-amigos-2026-10-18.png). Tests: `allowed` de
`CheckCall` en `voice/tests/private.rs` y `tests/session/voice.rs`
(`ringable` verdadero, falso y ausente; 403 y 404 rechazados, 500 y 502
no; una conversación que no es privada falla sin pedido).

**Notificación con botones (2026-10-18).** La notificación del sistema de
una llamada entrante ofrece «Aceptar» y «Rechazar» (acciones `accept` y
`decline` de `org.freedesktop.Notifications`, categoría `call.incoming`,
urgencia crítica y sin vencimiento) y se retira cuando deja de sonar.
Aceptar abre el privado y entra; Rechazar manda el mismo `stop-ringing`
que el aviso de la ventana y no trae la ventana al frente. Un botón de una
llamada que ya no suena no atiende ni rechaza nada (Aceptar abre la
conversación). Sólo se aceptan los botones que la notificación ofreció y
del dueño actual del servicio. Probado con un demonio falso en un bus
privado (`notifications/linux_tests.rs`), no con un escritorio real.

### Qué probar contra Discord (para el dueño)

Con una **segunda cuenta propia o una persona de confianza** en el cliente
oficial (en otro equipo o en el teléfono), que tenga un privado con la
cuenta de RUNE y, para grupos, un grupo con ambas:

1. Desde RUNE, abrir el privado y tocar el teléfono de la cabecera.
   Esperado: «Llamando…» y el mosaico de la otra persona; en el cliente
   oficial suena. Que atienda: se oyen los dos. Que cuelgue: RUNE dice
   «… cortó» y ofrece «Volver a llamar».
2. Llamar otra vez y que rechace; otra vez y que no atienda hasta que deje
   de sonar (anotar cuánto tarda Discord en cortar el ring): RUNE dice
   «rechazó» o «Sin respuesta». Si se confunden, anotar los segundos.
3. Desde el cliente oficial, llamar a la cuenta de RUNE: con RUNE en otro
   canal, aparece el aviso con Aceptar/Rechazar y suena; con la ventana
   minimizada o en la bandeja, también la notificación del sistema.
   Probar Rechazar y, en otra llamada, Aceptar.
4. Con la cuenta de RUNE también abierta en el teléfono: atender en el
   teléfono una llamada que suena en los dos. RUNE deja de sonar y muestra
   «Estás en esta llamada desde otro dispositivo» con «Unirse desde acá».
5. En una llamada privada: silenciar, ensordecer, cámara y compartir la
   pantalla; la otra persona mira la transmisión desde el cliente oficial.
   Y al revés: que la otra persona transmita y mirarla desde RUNE.
6. En un grupo: llamar con dos personas más; «Dejar de llamar» en el
   mosaico de una de ellas.
7. (2026-10-18) Antes de cada llamada 1:1 sale la línea
   `voice private call: may it ring (GET /call)? ringable: …`. Con un
   amigo, `true` y suena como en el punto 1. Llamar a alguien que **no** es
   amigo (un privado abierto desde un servidor en común): esperado
   `… refused: …`, RUNE no entra a la llamada y muestra «No se puede llamar
   a…»; «Enviar solicitud de amistad» la manda (verla en el cliente
   oficial). Si alguna vez sale `ringable: false`, anotar con quién:
   RUNE entra sin sonar y dice «La llamada no va a sonar».
8. (2026-10-18) Con RUNE minimizado, que la otra cuenta llame: la
   notificación del escritorio trae «Aceptar» y «Rechazar» y queda hasta
   que deja de sonar. Probar cada botón en una llamada distinta, y que la
   otra persona cuelgue antes: la notificación se va sola. Anotar el
   escritorio (GNOME, KDE…) si los botones no aparecen.

**Si algo falla, mandar** el texto del aviso y todas las líneas que
empiezan con `rune: voice private call`, `rune: voice call`,
`rune: voice share`, `rune: voice stream` y `rune: gateway` (son seguras:
no tienen credenciales, ids, sesión ni la IP propia). Lo más útil: si salió
`gateway: op 13 sent`, si llegó `CALL_CREATE` (y cuántos suena), si salió
`ringing (call/ring)` o quedó `ring held…`, el `VOICE_SERVER_UPDATE` y el
`closed by Discord: code …` o `ended: …` final. Si el ring falla con un
aviso, copiar el aviso: dice lo que respondió Discord (sin el cuerpo).

## Avatar por defecto y pantalla de inicio (2026-10-17, rama `pulido-ui-real`)

- **Avatar por defecto.** Quien no tiene avatar propio se ve con el de
  Discord: `User::default_avatar`, que el adaptador arma con el índice del
  cliente oficial (REA, ver [REA.md](REA.md)): discriminador viejo → `% 5`,
  nombre nuevo → `(id >> 22) % 6`, bot «0000» (webhook) → 0, y pide al CDN
  público `https://cdn.discordapp.com/embed/avatars/{i}.png` como cualquier
  avatar (nunca empaquetado). `User::picture()` y `Store::avatar_in` lo
  usan en miembros, mensajes, voz, perfiles, privados, amigos, llamadas,
  ajustes y el tooltip del riel; el editor de perfil ofrece «Quitar» sólo
  para el propio. Las cuentas recordadas sin avatar también. La
  simulación conserva la inicial; la inicial se ve además mientras carga
  o si falla. **Sólo fixtures, sin probar contra el servicio real.**
- **Pantalla de inicio.** Hasta el READY de la cuenta, en vez de la
  interfaz vacía: «Conectando…», «Iniciando sesión…» (la cuenta guardada
  se verifica; se puede cancelar) y «Cargando servidores…»; a los 10 s el
  motivo y «Reintentar» (arranca de nuevo el proveedor), «Cambiar de
  cuenta» y «Cerrar sesión». El aviso de conexión perdida queda para una
  sesión ya cargada. Corregido el 2026-10-18 (rama
  `inicio-sesion-e-idiomas`):
  - «Cerrar sesión» pregunta primero y olvida la cuenta en el llavero
    también mientras su sesión guardada se verifica o si no se pudo
    verificar (antes cancelaba sin olvidarla, contra la regla de
    AGENTS.md): el proveedor recuerda qué cuenta está restaurando y la
    borra con el mismo `forget` y el mismo `VAULT_TIMEOUT` que una sesión
    abierta. Cambiar de cuenta, cancelar o elegir otra la conservan.
  - «Cambiar de cuenta» muestra primero las cuentas recordadas («Continuar
    como…», `UseAccount(Some)`) con «Agregar cuenta» (`UseAccount(None)`,
    la página de login) y «Volver»; la cuenta que cargaba sigue recordada.
  - «Reintentar ya» dentro de la espera del Gateway entre intentos
    (`ConnectionState::Reconnecting`): `Command::ReconnectNow` corta esa
    espera y la misma sesión se reanuda sin rehacer el proveedor
    (`Capabilities::reconnect_now`; la simulación no lo declara: su
    pérdida simulada espera en el núcleo, sin sesión). Un pedido fuera de
    una espera no queda guardado para la próxima. Se ve en el aviso «Se
    perdió la conexión» sobre la conversación, no en la pantalla de
    inicio: la sesión manda la cuenta verificada (y la pantalla se va)
    antes de arrancar el Gateway. El apuro no corta los 5 s entre
    IDENTIFY sin sesión que reanudar, y un intento apurado que falla
    antes del READY no cuenta para el límite de reintentos.
  **Sólo fixtures (llavero falso, Gateway en loopback), sin probar contra
  el servicio real.**

## Reacciones, súper reacciones, fijados y búsqueda (2026-10-18, rama `reacciones-fijados-y-busqueda`)

Lo que faltaba de reacciones, votos, fijados y búsqueda remota. Fuentes:
REA estática del cliente web (bundles públicos ya descargados sin sesión,
leídos con `grep` y scripts propios; ver [REA.md](REA.md)) y Serein como
referencia, sin copiar código. **Sólo fixtures en loopback y la
simulación; nada se probó contra Discord.**

| Qué | Cómo | Origen |
| --- | --- | --- |
| Reaccionar | `Command::ToggleReaction { add }`: el estado pedido, no un cambio. PUT o DELETE `.../reactions/{emoji}/@me` según `add`; la recuperación del mensaje (GET) sigue para validar, pero ya no decide la dirección | causa: con la dirección deducida del mensaje cargado, dos clics rápidos con el primero rechazado terminaban en la dirección del primero |
| Fijar | `PinMessage`/`UnpinMessage` ya eran estados por la cola serial: de dos pedidos rápidos gana el último aunque el primero falle (test) | — |
| Permiso de fijar | bit 51 `PIN_MESSAGES` → `Permissions::PIN_MESSAGES`; en un servidor fija quien tenga `PIN_MESSAGES` **y** `READ_MESSAGE_HISTORY`, como el cliente oficial (en `Abilities`, en la revalidación de la sesión y en la simulación); `MANAGE_MESSAGES` solo no lo ofrece. En los editores de permisos de rol, canal y categoría | `xBc.PIN_MESSAGES: vI(51)`; el cliente pide `PIN_MESSAGES` y `READ_MESSAGE_HISTORY` (o un privado); Serein pide `MANAGE_MESSAGES` y ofrece el bit en sus editores. Que el servicio todavía tome `MANAGE_MESSAGES` solo no está confirmado |
| Súper reacciones en mensajes | `count_details {normal, burst}` → `count` y `burst`; `me_burst`; `burst_colors` (`#rrggbb`, hasta 8); sin `count_details`, `count` es de las normales | el cliente web hace lo mismo (`burst_count = count_details.burst ?? 0`, `count = count_details.normal ?? 0`) |
| Súper reacciones en vivo | `MESSAGE_REACTION_ADD/REMOVE` con `type` 1 (o, sin `type`, `burst: true`) → `Event::BurstReactionChanged { colors }`; se cuentan aparte, también en el registro de cambios ya vistos. Un `type` desconocido, o uno que `burst` contradice, se descarta | `type` 0/1 = normal/burst en el cliente y en Serein |
| Mandar súper reacciones | **no**: la pastilla se muestra sin botón | decisión del paquete |
| Ordenar la búsqueda | `sort_by=relevance&sort_order=desc` con «Más relevantes»; la página conserva el orden del servicio | `MOST_RELEVANT → {sort_by:"relevance", sort_order:"desc"}` |
| Encuestas | `has=poll` (`has:poll` o `tiene:encuesta`) | `has` del cliente (`poll`) |
| Enlaces a un sitio | `link_hostname=<host>` por cada `link_from:` (`linkFrom:` como el cliente, o `sitio:`); el host se valida (letras, dígitos, `.` y `-`) y se codifica byte por byte | `ANSWER_LINK_FROM` con `queryKey:"link_hostname"`, token `linkFrom` |

**En vivo.** Reacciones, súper reacciones y votos que llegan del Gateway se
guardan en SQLite sobre la fila del mensaje (`MessageMutation::Tally`, sólo
si está guardada y sin lápida), así que la caché ya no espera a recargar el
historial. También invalidan una carga de contexto del mismo canal en curso
(Store, Client y la sesión), como ya lo hacían las ediciones. Un voto tiene
plazo propio en `rune-core` (el de los envíos, 30 s, más el tiempo de una
verificación hCaptcha): si nada lo contesta (`PollVoteChanged` propio, el
mensaje entero o un fallo), se da por dudoso y se deshace lo mostrado. Con
la sesión, el 204 del voto no cuenta como respuesta: lo contesta el eco
`MESSAGE_POLL_VOTE_ADD/REMOVE` propio del Gateway. **Riesgo sin probar:** si
Discord no le manda ese eco a quien votó, un voto aceptado se deshace a los
30 s (y uno que espere más de 30 s en la cola, detrás de un 429, también).
La lista de quién reaccionó sólo muestra las normales: un emoji con sólo
súper reacciones no tiene pestaña ni ofrece «Ver reacciones».

**Interfaz.** Cada emoji con súper reacciones muestra una pastilla aparte,
antes de la normal, teñida con el primer color que dio Discord (con el
acento si no dio ninguno), con borde si la súper reacción es propia, y su
tooltip («Vos y 1 persona más hicieron súper reacciones con :tada:»). No
es un botón. La simulación muestra una en el anuncio de la juntada de
#general ([captura](img/super-reactions-2026-10-18.png)) y, de vez en
cuando, una súper reacción ajena en vivo. La búsqueda en
todo el historial suma el selector de orden
([captura](img/search-order-2026-10-18.png); su menú abre hacia arriba,
dentro del diálogo), `has:poll` y `link_from:`, que sugiere los sitios de
los enlaces cargados. Por relevancia, lo cargado sólo se suma cuando la
página llega hasta el último resultado (`offset + mensajes >= total`), no
con una página corta que tiene más detrás.

**Pendiente:** ver quién hizo súper reacciones (la lista de reacciones
sigue pidiendo las normales), reacciones con emoji propio de servidor (no
modeladas) y el orden «más antiguos».

**Para probar contra Discord** (dueño, con su cuenta): reaccionar y sacar la
reacción enseguida con dos clics, en un canal donde reaccionar dé error y en
uno normal, y ver que queda como el último clic; en un servidor propio,
dar a un rol sólo «Fijar mensajes» (sin «Gestionar mensajes») y fijar y
desfijar con esa cuenta, y a otro sólo «Gestionar mensajes» (sin «Fijar
mensajes»): RUNE no le ofrece fijar, como el cliente oficial; ver si
Discord igual lo acepta; votar en una encuesta y esperar 30 s: el voto
sigue (llegó el eco propio del Gateway); un mensaje con súper reacciones (hechas desde el
cliente oficial con Nitro) se ve con su pastilla aparte y sus colores, y
una súper reacción nueva o quitada en vivo cambia la cuenta; cerrar RUNE y
abrirlo: las reacciones y votos recibidos en vivo siguen; buscar con
«Más relevantes», con `has:poll` y con `linkFrom:` (por ejemplo
`linkFrom:github.com`).

## Contenido de mensajes (2026-10-18, rama `contenido-de-mensajes`)

Lo que trae un mensaje y RUNE no mostraba entero, más el filtro de
contenido explícito y copiar imágenes y videos. Rutas y campos por REA del
cliente web oficial ([REA.md](REA.md#cliente-web-oficial-contenido-explícito-y-registros-de-llamadas-2026-10-18))
y Serein (que guarda el ThumbHash y trata los bits de contenido sensible
como spoiler, sin mirar los ajustes). **Sólo fixtures, sin probar contra el
servicio real.**

- **Reenvíos.** El texto de `message_snapshots[0].message.content` lleva sus
  referencias (`Forwarded::text_references`): emoji de servidor como
  imagen, `<@id>`, `<#id>`, enlaces y momentos, como un mensaje propio. Las
  personas de `mentions` del snapshot pasan al almacén para que su nombre
  se vea; una mal formada se ignora.
- **Mensajes de sistema.** Los nombres del autor y de la persona nombrada
  (añadir o quitar del grupo) son tramos con la acción `member:{id}`, que
  abre el perfil como una mención. El registro de una llamada (tipo 3) lee
  `call.participants` y `call.ended_timestamp`, también cuando llegan en
  `MESSAGE_UPDATE` al terminar: si terminó y la cuenta no está entre los
  participantes, «Te perdiste una llamada de X (que duró …)»; si no, «X
  inició una llamada (que duró …)». La duración se redondea como el
  oficial (unos segundos, un minuto, N minutos, una hora…).
- **ThumbHash.** `placeholder` con `placeholder_version` 1 (base64, sin
  relleno, hasta 64 caracteres, al menos 5 bytes) se guarda como
  `ImageKind::ThumbHash` y `rune-core` lo decodifica en el momento (≤ 32 px,
  código propio); se dibuja cubriendo la caja reservada mientras la imagen
  carga o si no carga. Otra versión, o uno mal formado, se ignora. Se
  decodifican hasta ocho por cuadro; los demás, en el siguiente.
- **Contenido explícito.** El bit 4 de `flags` (16,
  `CONTAINS_EXPLICIT_MEDIA`) de un adjunto o de una tarjeta lo marca
  explícito; una imagen de las partes de una app (miniatura o galería), el
  bit 1 de `media.content_scan_metadata.flags` o ser la de un adjunto
  explícito (`media.attachment_id`). `Store::obscures_explicit(canal,
  autor)` elige el nivel del protobuf 29: servidores (1), amigos (2) u
  otros (3, privados y grupos con quien no es amigo); sin elegir, como el
  oficial para una cuenta adulta: se ve en servidores y de amigos y se tapa
  de los demás. Los mensajes propios nunca. Mientras los niveles no
  llegaron (desde la caché antes de READY, o si no se pudieron leer), todo
  lo explícito se tapa. «Difuminar» y «Bloquear» difuminan: bloquear lo
  hace Discord; si algo llega igual, se difumina. Una imagen tapada se pide
  como `ImageKind::Obscured` (la misma vista previa, achicada a 24 px y
  desenfocada en CPU, sin animación) con «CONTENIDO EXPLÍCITO · Hacé clic
  para mostrar»; el clic la muestra (y recién ahí se pide la imagen). Un
  video explícito se tapa entero. Una tarjeta explícita muestra su primera
  imagen difuminada bajo la tapa y se revela como un spoiler; la imagen de
  una parte de app, igual, con su propio número de spoiler. Algo que además
  es spoiler muestra la tapa de explícito sin nada de la imagen, y el único
  clic revela las dos. Ni el visor ni «Copiar imagen» llegan a una imagen
  tapada.
- **Copiar imagen y Copiar video.** En el menú de un adjunto que se puede
  bajar: una imagen con vista previa ofrece «Copiar imagen» y un video
  «Copiar video». Se baja como una descarga (sin credenciales, mismos
  límites y los dos cupos compartidos) a `$XDG_RUNTIME_DIR/rune-copied`
  (0700, nunca `/tmp`); la imagen se decodifica (hasta 4096 px de lado) y
  va al portapapeles como píxeles; el video queda como archivo y se copia
  como `text/uri-list`, hasta la próxima copia. El portapapeles se escribe
  con `arboard` (X11, como la lectura de adjuntos), que se mantiene vivo
  mientras RUNE corre; en Wayland llega por XWayland. Sin XWayland, el
  aviso dice que RUNE no tiene ahí una ventana propia del portapapeles.
- **Simulación.** En #general, «Fotos de la costa»: una que no llega nunca
  (se ve su ThumbHash) y otra marcada explícita (se difumina con
  «Difuminar» en servidores); un reenvío con un emoji, @Ada y #anuncios;
  con Ada, una llamada de 12 minutos y una perdida de un minuto. Capturas:
  [explícito, ThumbHash y reenvío](img/contenido-de-mensajes-2026-10-18.png)
  (`--channel general --action jump:latest --action
  click:us:privacy:explicit-servers=blur --action last:forward`) y
  [llamadas](img/contenido-de-mensajes-llamadas-2026-10-18.png)
  (`--channel Ada --action jump:latest`).

**Para probar contra Discord** (cuenta del dueño):

1. Reenviar a RUNE un mensaje con un emoji de servidor, una mención y un
   canal: el texto del reenvío los muestra resueltos.
2. Un privado o grupo donde alguien sale o es agregado: clic en los nombres
   del registro abre su perfil.
3. Una llamada privada atendida y otra perdida (otra cuenta de confianza):
   al cortar, los registros dicen «inició una llamada que duró …» y «Te
   perdiste una llamada de … que duró …», sin volver a abrir el privado.
4. Abrir un canal con imágenes en una conexión lenta: se ve el ThumbHash
   antes de la imagen.
5. En Ajustes → Datos y privacidad, «Difuminar» en servidores; una imagen
   marcada explícita (la marca la pone Discord) se ve difuminada y un clic
   la muestra. Comparar con el cliente oficial en el mismo mensaje; también
   una imagen explícita en la galería de un bot (componentes V2), si se
   consigue una.
6. «Copiar imagen» y pegar en otra app (imagen); «Copiar video» y pegar en
   un gestor de archivos o un chat (archivo). Probar en la sesión Wayland
   del dueño: si no pega, avisar cómo falla.

## Protocolo de voz (2026-10-18, rama `voz-protocolo`)

Sólo fixtures, sin probar contra el servicio real. Lo que no está
confirmado se hace como Serein y queda anotado para la prueba del dueño.

- **Calidad de la llamada.** La conexión de la llamada (no la de una
  transmisión) mide en cada ACK del latido del gateway de voz (op 6 con el
  `t` del último op 3, cada `min(intervalo, 5 s)`) el RTT de ese latido y
  la pérdida desde el anterior: los huecos en los números de secuencia del
  audio de los demás (un salto de más de 250 paquetes es un reinicio, no
  pérdida; un paquete atrasado llena su hueco; hasta 256 fuentes). Llega
  como `Event::CallPhase` con `quality` (la fase de ese momento), y la
  interfaz ya lo mostraba (latencia en el ícono de señal, historial y
  pérdida en «Detalles de voz»). Sin audio de nadie, la pérdida es 0. No se
  usan los RTCP RR: Serein no los lee y no se sabe si Discord los manda
  para el audio.
- **Fase `Waiting`.** Conectada y sola en el canal (DAVE sin nadie más):
  «Esperando a los demás» en el panel, «Nadie más en la llamada» en el
  escenario. Pasa a `Connected` cuando alguien entra y el grupo DAVE queda
  listo, y vuelve si se van todos. El reloj de la llamada sigue: es la
  misma conexión. Una transmisión propia sin espectadores sigue «en vivo».
  Simulación: igual, según quién está en el canal, con sus textos propios
  («Esperando a los demás (simulada)», «Nadie más en la llamada
  simulada»).
- **`GUILD_CREATE` tardíos.** Ver *Otro dispositivo* arriba: un `READY`
  sin el servidor de la llamada deja el op 4 esperando su `GUILD_CREATE`
  (15 s como mucho), que puede mostrar la voz de la cuenta en otro
  dispositivo (termina sin colgarlo). Mientras espera no sale ningún op 4:
  silenciar, la cámara o una transmisión cambian los controles y van con el
  op 4 que termina la espera, y un reingreso pendiente (1, 3 o 10 s) se
  cancela. En ese `GUILD_CREATE` (y en el `READY`) la fila de la cuenta con
  la sesión del Gateway nuevo es de este dispositivo, no de otro. Cada
  conexión nueva mide su propio tiempo conectada, para el presupuesto de
  reingresos.
- **Hora de inicio del canal de voz (op 43).** `{"op": 43, "d":
  {"guild_id", "fields": ["voice_start_time"]}}` una vez por conexión y
  servidor: al mostrar el servidor en la columna de canales
  (`Command::WatchServer`, con la lista de miembros abierta o cerrada; como
  el cliente oficial, que lo pide para cada servidor que muestra, REA en
  [REA.md](REA.md#cliente-web-oficial-hora-de-inicio-de-los-canales-de-voz-2026-10-18))
  y al entrar a una llamada de servidor (como Serein, `voice.rs:445`).
  `CHANNEL_INFO` `{guild_id, channels: [{id, voice_start_time}]}` reemplaza
  las del servidor; `VOICE_CHANNEL_START_TIME_UPDATE` `{id, guild_id,
  voice_start_time}` cambia una. El tiempo va en segundos desde 1970,
  `null` sin nadie; una fila con un tiempo fuera de rango (antes de 2015 o
  después de 2100) o de otro tipo, o con un id inválido, se descarta sin
  cortar la sesión ni borrar lo que se sabía. El `Store` sólo toma canales
  conocidos de ese servidor, y borra la hora de una sala que queda vacía.
  Llega como
  `Event::VoiceStartTimes`; la lista de canales muestra en la fila de cada
  sala con alguien cuánto hace («12 min», «1 h 05 min», al minuto) y lo dice
  al lector de pantalla. El cliente oficial lo guarda sólo para salas de
  voz (`GUILD_VOICE`), no escenarios; RUNE igual. Simulación: las salas
  ocupadas empiezan entre 7 y 186 minutos antes, por canal; una sala que
  se ocupa o se vacía después, por quien sea (la cuenta, la gente de una
  llamada, la moderación), empieza en ese momento o se apaga.
- **Extensión RTP del sonido de la pantalla.** El sonido de la pantalla
  compartida sale con la extensión RFC 8285 de un byte que usa Serein: ID 9,
  valor 4 (`rtp::SCREEN_SOUND`; un codificador propio de la forma de un
  byte arma el preámbulo `BE DE` con su largo en claro y autenticado, y el
  elemento con su relleno sellado con la carga). **Ni el ID ni el valor
  están verificados**:
  el cliente oficial no los muestra como cadenas (REA.md, «Sin confirmar»).

### Qué probar contra Discord (para el dueño)

1. Entrar solo a una sala: «Esperando a los demás»; que entre la otra
   cuenta: «Voz conectada» y el audio en los dos sentidos; que salga: vuelve
   a esperar sin cortar el audio ni reiniciar el reloj.
2. Con la llamada conectada, mirar «Detalles de voz»: la latencia tiene que
   parecerse a la que muestra el panel de conexión del cliente oficial, y la
   pérdida quedar cerca de 0 % en una red sana.
3. Cortar la red unos segundos con una llamada en un servidor grande (que
   Discord mande `unavailable` en el `READY` no se puede forzar: anotar si
   alguna vez se ve en `rune.log` la línea «without the call's server yet»;
   si pasa, que la llamada vuelva sola y no termine con «Tu llamada siguió en
   otro dispositivo»).
4. Ver un servidor con gente en voz, con la lista de miembros abierta y
   cerrada: en la fila de la sala, cuánto hace que está en voz, comparado
   con lo que muestre el cliente oficial; que se actualice al vaciarse la
   sala y al volver alguien.
5. Compartir la pantalla con sonido y que la otra cuenta lo escuche en el
   cliente oficial como sonido de la transmisión (no como voz); si no se
   oye o suena como micrófono, el ID 9/valor 4 no es el que espera Discord.

## Riel y notificaciones (2026-10-18, rama `riel-y-notificaciones`)

Sólo fixtures, sin probar contra el servicio real.

- **Apps en voz.** Las actividades embebidas (una app lanzada en un canal
  de voz) se leen como el cliente oficial (REA, ver
  [REA.md](REA.md#cliente-web-oficial-tooltip-del-servidor-y-apps-en-voz-2026-10-18)):
  `activity_instances` de cada guild en `GUILD_CREATE` y en
  `READY_SUPPLEMENTAL`, y después `EMBEDDED_ACTIVITY_UPDATE_V2`, que
  reemplaza el lanzamiento que nombra y lo termina sin participantes.
  Cada una se identifica por canal y aplicación
  (`location.channel_id`:`application_id`); sólo en un canal de voz de
  un servidor que la cuenta conoce, hasta 100 personas, y 50 por servidor
  en la lectura inicial (después, una por canal y aplicación). Una mal
  formada se descarta sin cortar la sesión. Llegan como
  `Event::VoiceActivity`; Serein no las lee. El tooltip cuenta sólo a
  quien sigue en el canal de la app: un lanzamiento que terminó mientras
  el servidor no estaba disponible no se borra hasta el próximo READY
  (pendiente: que `GUILD_CREATE` reemplace los del servidor).
- **Menciones de roles y de todos.** `mention_roles` y `mention_everyone`
  del mensaje (y de su edición, que si no los trae conserva lo que había)
  van a `Message::mention_roles` y `Message::mention_everyone`. Sólo dicen
  a quién avisar: una lista mal formada no menciona roles (se descartan
  los ids inválidos) y un `mention_everyone` que no es booleano cuenta como
  falso, sin invalidar el mensaje ni cortar la sesión. @everyone y @here
  cuentan como mención y avisan, como en Serein, sólo con
  `mention_everyone` (Discord ya miró si el autor puede) y salvo con
  «Silenciar @everyone y @here».
- **Nivel por defecto y canal AFK.** Ya se leían de READY y `GUILD_CREATE`
  (`properties.default_message_notifications`, `properties.afk_channel_id`,
  desde P7); ahora lo usan el riel, la lista de canales y los avisos.
- **Privados y grupos silenciados.** «Silenciar conversación» en su menú
  manda `PATCH /users/@me/guilds/@me/settings` con el `channel_overrides`
  del privado (`muted: true` y `selected_time_window: -1`, para siempre,
  como Serein; volver a activarlo manda `muted: false` y
  `mute_config: null`), lo que el proveedor ya hacía con `server: None`.

## Portadas, nameplates por sku y cosméticos (2026-10-18, rama `perfil-tarjeta-y-cosmeticos`)

Sólo fixtures y simulación; **sin probar contra el servicio real**. Rutas
y formas por REA estática del bundle web oficial (ver
[REA.md](REA.md#cliente-web-oficial-portadas-nameplates-y-decoraciones-2026-10-18));
nada se copió.

- **Portadas.** `assets.large_image` / `small_image` con `spotify:{id}`,
  `twitch:{login}` o `youtube:{id}` se arman como el cliente oficial:
  `https://i.scdn.co/image/{id}`,
  `https://static-cdn.jtvnw.net/previews-ttv/live_user_{login}-160x160.jpg`
  (los 160 px que pide su tarjeta de actividad) y
  `https://i.ytimg.com/vi/{id}/hqdefault_live.jpg`. El identificador sólo
  con letras y dígitos ASCII (y `_`, `-` donde el servicio los usa), hasta
  64. Son JPEG quietos: se piden con los límites de un banner (`Banner`,
  1 MiB, 2048 px y 2 Mpx, un cuadro, normalizadas a 512×256; al principio
  iban como vistas previas, 8 MiB y 4096 px). Los tres orígenes se suman a
  `image_origins` (`provider::IMAGE_ORIGINS`); ningún otro host de esos
  servicios. Nunca se lee el token de Spotify de las conexiones. Detalle de
  la ampliación en [SECURITY.md](SECURITY.md#portadas-de-spotify-twitch-y-youtube-2026-10-18).
- **Nameplates por sku.** `collectibles.nameplate.sku_id` da, como el
  cliente oficial en Linux (que no reproduce el video), la versión animada
  `cdn.discordapp.com/media/v1/collectibles-shop/{sku}/animated`; la quieta
  sigue saliendo de `asset` (`assets/collectibles/{asset}static.png`, la que
  ya funcionó con la cuenta del dueño) y, si `asset` falta o es inválido,
  de `…/{sku}/static`. La animada se pide sólo con la fila bajo el puntero
  o con la tarjeta de esa persona abierta, y se decodifica como cualquier
  imagen animada (APNG, WebP o GIF; un video no se decodifica y queda la
  quieta). **Sin confirmar**: el formato real de `animated`.
- **Decoraciones.** Siguen por `avatar-decoration-presets/{asset}.png` con
  `passthrough` (el bundle de esta build las pide por sku si lo trae; no se
  cambió lo que ya funciona). Se animan bajo el puntero en la lista, en los
  mensajes y en los mosaicos de voz.
- **Perfiles.** Lo guardado se vuelve a pedir al reabrir la tarjeta pasado
  un minuto (`Store::profile_stale`, como el `type=popout` oficial); el
  popout propio pide `GET /users/{yo}/profile` sin mutuos (el oficial lo
  precarga como `account_popout`), así su banner, efecto e insignias no
  dependen de lo que trajo `READY`.
- **Cosméticos ocultos.** Con un cosmético apagado en Ajustes, la interfaz
  no pide sus imágenes; el producto del efecto (`GET
  /collectibles-products/{sku}`) se sigue leyendo con el perfil (es JSON de
  la API, no una imagen del CDN).

## Reglas del servidor y onboarding (2026-10-18, rama `reglas-y-onboarding`)

Sólo fixtures en loopback: nada de esto se probó contra Discord. Las rutas
salen de la REA estática del cliente web oficial
([REA.md](REA.md#cliente-web-oficial-reglas-del-servidor-y-onboarding-2026-10-18))
y coinciden con las que usa Serein (`discord-api` y `discord-protocol`,
`onboarding.rs`). Una revisión encontró partes de la primera versión de
`src/onboarding.rs` (limpiar textos, armar el cuerpo de las reglas y el de
las respuestas, leer el estado de la solicitud y los tests con sus
fixtures) demasiado parecidas a las de Serein; se reescribieron el
2026-10-18 desde la REA, con estructura y fixtures propios (`src/onboarding.rs`
y `src/http/onboarding.rs`).

- **Qué pide un servidor.** De `features` del servidor (READY,
  `GUILD_CREATE`, `GUILD_UPDATE`): `MEMBER_VERIFICATION_GATE_ENABLED`
  enciende `ServerOptions::rules_screening` y `GUILD_ONBOARDING`,
  `ServerOptions::onboarding`. Del miembro propio: `pending` (ya se leía) y
  la bandera documentada `COMPLETED_ONBOARDING` (`1 << 1` de `flags`), que
  va a `MemberRestrictions::onboarded`; `IS_GUEST` (`1 << 4`) va a
  `MemberRestrictions::guest`, y a un invitado nunca se le pide el
  onboarding (el oficial tampoco lo abre con `IS_GUEST_INVITE`). Terminar
  el onboarding no restringe nada; `pending` sí (el compositor lo dice).
- **Leer.** `GET /guilds/{g}/member-verification?with_guild=false`
  (versión, descripción y hasta 5 campos: `TERMS`, `TEXT_INPUT`,
  `PARAGRAPH`, `MULTIPLE_CHOICE`; un tipo desconocido se conserva por su
  nombre y la interfaz dice que RUNE no lo completa) y
  `GET /guilds/{g}/onboarding` (preguntas con opciones, roles y canales,
  canales por defecto, modo, si está activo y lo que la cuenta eligió en
  `responses`, de las que la interfaz usa sólo las de preguntas que se
  hacen al entrar). Lo que se muestra se recorta y pierde los caracteres de
  control; listas de más son una respuesta incompatible. Se leen hasta 50
  preguntas (tope defensivo, como Serein) aunque RUNE guarde hasta 15: los
  límites salen de Serein y del oficial, y la REA no confirmó ninguno.
- **Responder.** Primero las preguntas:
  `POST /guilds/{g}/onboarding-responses` con
  `{onboarding_responses, onboarding_prompts_seen, onboarding_responses_seen}`
  (cada pregunta y opción mostrada cuenta como vista ahora, como el
  oficial); después las reglas: `PUT /guilds/{g}/requests/@me` con
  `{version, form_fields}`, cada campo tal como vino (la copia que guarda
  `RulesField::echo`, con las claves que RUNE no modela) más su
  `response`. Unas respuestas que no se pueden mandar no salen, con un
  motivo local («Las respuestas no están completas: revisalas.»), no el de
  una respuesta de Discord.
  `application_status` dice el resultado: `APPROVED` (o ninguno) es
  `JoinOutcome::Approved`, `REJECTED` es `Rejected` y el resto
  (`SUBMITTED`) es `InReview`. Aprobado, la sesión deja escribir antes del
  `GUILD_MEMBER_UPDATE` (`onboarding::confirmed`); respondidas las
  preguntas, marca `onboarded`. Los roles que dan llegan como la
  actualización del miembro.
- **Editar.** `PUT /guilds/{g}/onboarding` (documentado por Discord): cada
  pregunta con `id`, `type` (0 opciones, 1 lista), `title`,
  `single_select`, `required`, `in_onboarding` y sus opciones con
  `emoji_id`, `emoji_name`, `emoji_animated`, `role_ids` y `channel_ids`;
  `default_channel_ids`, `enabled` y `mode` (0 sólo los canales por
  defecto, 1 también los de las preguntas). Una pregunta u opción nueva
  lleva por id la hora en milisegundos, como las del oficial
  (`String(Date.now())`), o uno más que la última hecha así en el mismo
  milisegundo; los snowflakes de Discord no cuentan. Sin probar si
  Discord los acepta. Un `200` con un cuerpo que RUNE no lee deja la
  escritura ambigua (no se supone lo que guardó); un `204` guardó lo
  mandado. Antes de
  mandarlo, la sesión exige `MANAGE_GUILD` y `MANAGE_ROLES`, que sea una
  comunidad, que roles y canales sigan en el servidor y que, encendido,
  haya un canal donde `@everyone` pueda escribir. Discord documenta además
  al menos 7 canales por defecto con 5 donde todos escriban: RUNE no lo
  repite (lo decide Discord y su rechazo llega con su motivo).
- **Verificación.** Las tres escrituras pasan por el hCaptcha común de
  cualquier escritura: la persona lo resuelve en la ventana aislada y la
  misma escritura sale una sola vez más con la respuesta (un test con
  fixtures por cada una de las tres).
- **Capacidades:** `server_rules`, `onboarding` y `manage_onboarding`.

## Identidad de navegador (2026-10-19, rama `identidad-de-navegador`)

Decisión del dueño del 2026-10-19 ([SECURITY.md](SECURITY.md#identidad-de-navegador-autorizada-el-2026-10-19)):
el adaptador se presenta como el cliente web de Discord en un navegador,
como Serein, con una sola identidad. **Sólo fixtures, sin probar contra el
servicio real.**

**Qué navegador.** El cliente web en WebKit sobre Linux, porque las dos
ventanas de `rune-login` son WebKitGTK: el cliente web que corre en la de
login se clasifica como Safari (`/Apple/.test(navigator.vendor)` va antes
que cualquier mirada al user agent) y hCaptcha ve el motor WebKit. Serein
manda Chrome en Linux desde su WebKitGTK; acá el user agent, las
propiedades y el motor del desafío dicen lo mismo. En Windows el motor es
WebView2 y la identidad sale de él
([identidad por plataforma](#identidad-por-plataforma-2026-10-19-rama-windows-login)).

| Pieza | Valor |
| --- | --- |
| User agent | `Mozilla/5.0 (X11; Linux {arquitectura}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/60.5 Safari/605.1.15`: el que WebKitGTK 2.52 trae de fábrica (`x86_64` en esta máquina) |
| `X-Super-Properties` | base64 del JSON del cliente web, en su orden: `os` `Linux`, `browser` `Safari`, `device` y `device_model_name` vacíos, `system_locale`, `has_client_mods` `false`, `browser_user_agent` (el de arriba), `browser_version` `60.5`, `os_version` y los cuatro `referrer…` vacíos, `release_channel` `stable`, `client_build_number`, `client_event_source` y `launch_signature` `null`, `client_launch_id` y `client_heartbeat_session_id` (UUID v4 por proceso), `client_app_state` `focused` |
| Identify (`properties`) | Las mismas, más `is_fast_connect: false` y `gateway_connect_reasons: "COLD_START"`; sin `capabilities`, así las cargas siguen con la forma que el adaptador lee |
| Número de build | Al arrancar el proveedor, antes de todo lo demás, `GET https://discord.com/app` sin credencial y lee `window.GLOBAL_ENV.BUILD_NUMBER`; si falla (5 s, 1 MiB), 641004 |
| `system_locale` | El idioma del sistema como WebKit lo da a las páginas (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`: `es_AR.UTF-8` es `es-AR`); sin uno, `en-US` |
| `X-Discord-Locale` | Ese idioma llevado a uno de Discord como hace el cliente web cuando la cuenta no eligió otro (`es-AR` → `es-419`, `es` → `es-ES`) |
| `X-Discord-Timezone` | La zona del sistema (`TZ` o el enlace `/etc/localtime`), o `UTC` |
| Otras cabeceras de la API | `Accept-Language` (`es-AR,es;q=0.9`), `Referer: https://discord.com/channels/@me` y, en las escrituras, `Origin: https://discord.com` |
| Aceptar una invitación | `X-Context-Properties` = base64 de `{"location":"Desktop Invite Modal","location_guild_id":…,"location_channel_id":…,"location_channel_type":…}`, con lo que mostró la vista previa leída justo antes (el reintento con hCaptcha manda el mismo) |
| Gateway y voz | El mismo user agent y `Origin: https://discord.com` en el apretón del WebSocket |
| Ventanas de login y hCaptcha | `WebKitSettings` con ese mismo user agent fijo |
| Subidas al almacenamiento | Sólo el user agent (no son la API de Discord) |

**De dónde sale.** REA del bundle público `web.4d4b2f75fdea5470.js` (build
641004, bajado sin sesión el 2026-10-17): cómo arma las propiedades, las
cabeceras de cada pedido, el Identify y el contexto del modal de
invitación ([REA.md](REA.md#cliente-web-oficial-identidad-de-navegador-2026-10-19)).
Serein se miró como segunda referencia (`fingerprint.rs`, sus cabeceras
REST y su Identify); no se copió código.

**Qué no se manda, y por qué.** `X-Fingerprint`: el cliente web lo borra al
iniciar sesión (`LOGIN_SUCCESS`) y sólo lo usa sin sesión.
`X-Installation-ID` y `X-Debug-Options`: un navegador nuevo no los tiene.
`launch_signature`: lo firma el WebAssembly del cliente web con su entorno;
va en `null`, como antes de que ese módulo cargue, en vez de inventar una
firma.

**Lo que todavía difiere del cliente web** (fuera de este pedido; se cambia
si Discord lo distingue): la API va por `/api/v10` y el Gateway por `v=10`
en JSON sin `zlib-stream` (el cliente web usa v9 y comprime); la aceptación
manda el cuerpo `{}` sin el `session_id` de Gateway; el
`client_heartbeat_session_id` dura lo que el proceso (el cliente web lo
renueva tras un rato inactivo); el idioma es el del sistema, no el que la
cuenta eligió en Discord; las imágenes y archivos que baja `rune-core` del
CDN siguen con su user agent `RUNE/…`, porque esa capa no conoce al
proveedor.

**Pruebas (loopback y datos sintéticos).** `browser::tests` (user agent,
propiedades en el orden del cliente web, las mismas ids en cada pedido,
cabeceras de lectura y escritura, build de la página, idiomas);
`http::api::tests` (la página se pide sin credencial ni cabeceras de la
API; el contexto de la invitación inspeccionada y de ninguna otra); la
verificación de la cuenta, la vista previa y la aceptación, el reintento de
cualquier escritura con hCaptcha y el de una invitación comprueban con
`browser::assert_presented` el user agent, las propiedades decodificadas y
que la credencial sólo aparece en `Authorization`; el apretón y el Identify
del Gateway llevan el mismo user agent, `Origin` y propiedades, y el token
una sola vez; la prueba nativa de la ventana de hCaptcha (ignorada, pide
pantalla) sólo responde si `navigator.userAgent` es el del adaptador y
`navigator.vendor` es el de Apple.

**Qué probar contra Discord (para el dueño).** Iniciar sesión otra vez en
la ventana de RUNE (el login ahora lleva el user agent fijo), aceptar una
invitación y, si Discord pide hCaptcha, resolverlo en la ventana; mirar si
pide el teléfono, qué motivo muestra RUNE si rechaza, si el Gateway conecta
igual y si mandar mensajes y adjuntos sigue andando.

## Identidad por plataforma (2026-10-19, rama `windows-login`)

La identidad es la del motor de las ventanas de login y hCaptcha, el que
de verdad resuelve el desafío. `rune-discord-session::browser` la arma de
lo que una página en ese motor lee de su navegador (`Engine`: user agent,
`navigator.vendor`, las marcas de `navigator.userAgentData`, su
`platform`, `navigator.languages[0]` y la zona de `Intl`) con las reglas
del cliente web ([REA](REA.md#cliente-web-oficial-identidad-en-webview2-2026-10-19-rama-windows-login)).
**Sólo fixtures; nada se probó contra el servicio real ni en un Windows
real.**

| Pieza | Linux (WebKitGTK) | Windows (WebView2) |
| --- | --- | --- |
| De dónde sale | Se sabe de antemano: el user agent de fábrica de WebKitGTK 2.52 (fijado en las ventanas), vendor de Apple, sin client hints, idioma y zona del sistema como WebKit se los da a las páginas | Al arrancar el proveedor, antes de cualquier pedido, un auxiliar oculto (`rune --session-engine-helper`) abre WebView2 con un perfil temporal propio, carga una página vacía de RUNE (`https://captcha.rune.invalid/`, servida desde memoria) y lee esos valores; vuelven como JSON por la tubería privada y se validan |
| User agent | `Mozilla/5.0 (X11; Linux x86_64) … Version/60.5 Safari/605.1.15` | El de WebView2 instalado, tal cual (p. ej. `… Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0`) |
| `os` / `os_version` | `Linux` / vacío | `Windows` / `10` (también en Windows 11: dice `NT 10.0`) |
| `browser` | `Safari` (vendor de Apple) | `Edge` si las marcas traen `Microsoft Edge`; si no, `Chrome` (el user agent dice `Edg/`, no `Edge`) |
| `browser_version` | `60.5` | El número de `Edg/` |
| `system_locale`, `X-Discord-Locale`, `Accept-Language` | Del idioma del sistema (`LANG`…) | De `navigator.languages[0]` de WebView2 |
| `X-Discord-Timezone` | `TZ` o `/etc/localtime` | La zona de `Intl` de WebView2 |
| Client hints (`Sec-CH-UA`, `-Mobile`, `-Platform`) | No (WebKit no las manda) | En cada pedido a la API, con las marcas y versiones leídas, como las manda Chromium |
| Ventanas | `WebKitSettings` con ese user agent | El user agent propio de WebView2; RUNE lo fija sólo si el runtime se actualizó desde que se leyó (fijarlo puede borrar las client hints de la página) |

**El user agent va en cada pedido**, no en el cliente HTTP: en Windows el
cliente existe antes de saber el motor. Lo llevan los pedidos a la API, la
página pública del build, las subidas al almacenamiento, el apretón del
Gateway y de la voz, y el Identify.

**Sin motor no sale nada.** Si el auxiliar no puede leerlo (por ejemplo, no
está WebView2 Runtime), el proveedor no pide el build, y la comprobación
de la sesión guardada o el login vuelven a intentarlo y fallan con el
motivo, que en ese caso es el enlace oficial de Microsoft para instalarlo.

**Lo que todavía difiere en Windows** (además de lo de Linux): el apretón
del WebSocket no lleva client hints (no se verificó qué manda Chromium
ahí); `Accept-Language` sigue la regla de Linux (idioma, y el idioma base
con `q=0.9`), no la lista que arma Chromium; leer el motor cuesta un
arranque oculto de WebView2 (alrededor de un segundo) en cada inicio del
proveedor.

**Pruebas (sin Windows, datos sintéticos).** `browser::tests` arma la
identidad de un WebView2 de ejemplo (con y sin la marca `Microsoft Edge`,
sin client hints), valida lo que una página puede decir (nada que rompa
una cabecera o el JSON) y comprueba que un motor inválido nunca se
presenta. `tests::a_learnt_engine_is_what_rest_the_gateway_and_the_windows_present`
lo presenta en un proceso hijo y comprueba sobre lo que sale que REST, el
apretón del Gateway, el Identify y lo que reciben las ventanas llevan el
mismo, y que la credencial sólo va en `Authorization`.

## Eventos programados (2026-10-19, rama `eventos-programados`)

Serein no los tiene; el dueño los incluyó el 2026-10-18. Rutas del
cliente web oficial ([REA](REA.md#cliente-web-oficial-eventos-programados-2026-10-19-rama-eventos-programados))
y las documentadas por Discord: `src/events.rs` (lectura, validación previa
y Gateway) y `src/http/events.rs` (rutas). **Sólo fixtures en loopback:
nada se probó contra Discord.**

| Qué | Ruta | Responde |
| --- | --- | --- |
| La lista | `GET /guilds/{g}/scheduled-events?with_user_count=true` y `GET /users/@me/scheduled-events?guild_ids={g}` | `Event::ScheduledEvents`, con `account_interested` |
| Quiénes | `GET /guilds/{g}/scheduled-events/{e}/users?limit=100&with_member=true` | `Event::EventInterested` (arreglo o `{users}`) |
| «Me interesa» | `PUT …/{e}/users/@me` con `{response: 1}`; quitarlo, `DELETE` | `Event::EventInterest` de la cuenta |
| Crear | `POST /guilds/{g}/scheduled-events` (`privacy_level` 2) | el evento devuelto |
| Editar | `PATCH /guilds/{g}/scheduled-events/{e}`, sin `image` ni `recurrence_rule` (quedan como estaban) | el evento devuelto |
| Iniciar o terminar | `PATCH` con `{status: 2}` o `{status: 3}` | el evento devuelto |
| Borrar | `DELETE /guilds/{g}/scheduled-events/{e}` | `Event::ScheduledEventDeleted` |

- **Antes de mandar** (`events::allowed`): el servidor está en la sesión,
  la membresía cargó, una escritura pide `CREATE_EVENTS` o `MANAGE_EVENTS`
  (bits 44 y 33, ahora en `PERMISSION_BITS`), el borrador está completo
  (nombre, lugar, fin si es en otro lugar, inicio futuro al crear) y el
  canal es un escenario o un canal de voz de ese servidor. De quién es el
  evento lo decide Discord: la sesión no guarda los eventos.
- **Lectura**: un tipo o un estado que RUNE no conoce no se muestra; uno
  de otro servidor o sin nombre es una respuesta mal formada. La portada
  es `cdn.discordapp.com/guild-events/{e}/{hash}.png`.
- **Gateway**: `GUILD_SCHEDULED_EVENT_CREATE`, `UPDATE`, `DELETE`,
  `USER_ADD` y `USER_REMOVE` de los servidores de la sesión; los de una
  sola ocurrencia de un evento que se repite se ignoran. Un cambio
  informado no trae interesados: el estado conserva los que tenía.
- **Verificación**: un hCaptcha en cualquier escritura de eventos usa el
  mecanismo común (ventana de RUNE, una sola vez; test con fixtures al
  crear).
- **Invitaciones**: un enlace con `?event={id}` se reconoce con su evento
  y la consulta pública pide `guild_scheduled_event_id`; se muestra sólo
  el evento que nombra el enlace, del servidor al que lleva. Aceptar es la
  aceptación común, y su `X-Context-Properties` suma
  `invite_guild_scheduled_event_id` como el cliente web
  ([INVITATIONS.md](INVITATIONS.md#invitaciones-de-evento-2026-10-19)).
- **Capacidades:** `scheduled_events` y `manage_scheduled_events`.

Qué tiene que probar el dueño con su cuenta: que la lista de un servidor
con eventos coincida con la del cliente oficial (también los interesados y
«Te interesa»); marcar y desmarcar «Me interesa» y verlo en el oficial;
crear uno en un canal de voz y otro en otro lugar, editarlo, iniciarlo,
terminarlo y borrarlo con «Gestionar eventos»; que sin ese permiso sólo se
puedan cambiar los propios; que un cambio hecho en el oficial llegue por
el Gateway; y una invitación con `?event=` en un mensaje.

## AutoMod (2026-10-19, rama `automod`)

Sólo fixtures en loopback: nada de esto se probó contra Discord. Serein no
administra AutoMod (sólo nombra sus entradas en el registro de
auditoría); el dueño lo incluyó igual el 2026-10-18. Las rutas y los
cuerpos son los que Discord documenta y los que muestra la REA estática
del cliente web oficial
([REA.md](REA.md#cliente-web-oficial-automod-2026-10-19)); el código es
propio (`src/automod.rs` y `src/http/automod.rs`).

- **Leer.** `GET /guilds/{g}/auto-moderation/rules` (hasta 64 reglas, 2 MiB).
  Cada regla: `trigger_type` 1 palabras, 3 spam, 4 listas del servicio
  (`presets` 1 groserías, 2 contenido sexual, 3 insultos
  discriminatorios), 5 spam de menciones (`mention_total_limit`,
  `mention_raid_protection_enabled`) y 6 perfiles; `trigger_metadata` con
  `keyword_filter`, `regex_patterns` y `allow_list`; `actions` 1 bloquear
  (`custom_message`), 2 alertar (`channel_id`), 3 aislar
  (`duration_seconds`) y 4 bloquear la participación (el bundle lo llama
  `QUARANTINE_USER`); `exempt_roles`, `exempt_channels`, `enabled` y
  `creator_id`. Un tipo o una acción que RUNE no conoce se conserva como
  desconocido: la regla se muestra, se activa o desactiva y se borra, pero
  no se edita (guardarla la perdería).
- **Crear.** `POST /guilds/{g}/auto-moderation/rules` con `name`,
  `event_type` (1 al enviar mensajes; 2 al entrar o cambiar el perfil, para
  las reglas de perfiles), `trigger_type`, `trigger_metadata`, `actions`,
  `enabled`, `exempt_roles` y `exempt_channels`. El oficial manda además
  `guild_id`, `creator_id` y `position`; RUNE manda sólo lo documentado.
- **Editar.** `PATCH /guilds/{g}/auto-moderation/rules/{id}` con lo mismo
  menos `trigger_type`: Discord no documenta cambiar el tipo de una regla
  (el oficial lo manda igual). **Activar o desactivar**: el mismo `PATCH`
  con sólo `{"enabled": …}`, así no se reescribe nada que RUNE no lea.
  **Borrar**: `DELETE` a la misma ruta (`204`). La respuesta de cada
  escritura es la regla guardada; si no se puede leer, la escritura queda
  ambigua.
- **Antes de mandar.** La sesión exige `MANAGE_GUILD` (o ser dueño), que la
  membresía esté cargada y sin restricciones, una regla que
  `AutoModRule::problem` acepte (nombre, palabras y patrones dentro de los
  límites, expresiones regulares que parsea `regex-syntax`, la sintaxis de
  Rust que Discord dice usar, acciones que el tipo admite),
  `MODERATE_MEMBERS` si aísla y un canal de alerta que esté en el
  servidor. Los límites (1000 palabras de 60 caracteres, 10 patrones de
  260, 100 excepciones o 1000 en las listas, 50 menciones, 150 caracteres
  de mensaje, cuatro semanas de aislamiento, 20 roles y 50 canales
  exentos, 6 reglas de palabras y 1 de cada otro tipo) son los que Discord
  documenta; 1000, 60, 10, 260, 100, 50 y 20 están también entre las
  constantes del bundle, junto a los tipos (150 y 6 no se vieron ahí).
- **Gateway.** `AUTO_MODERATION_RULE_CREATE`/`UPDATE` (la regla) y
  `_DELETE` (`id`) cambian la lista ya leída; `AUTO_MODERATION_ACTION_EXECUTION`
  (`rule_id`, `user_id`, `channel_id`, `action`, `matched_keyword` o
  `matched_content`) se muestra en «Lo que hizo hace poco». Sólo de
  servidores que la sesión conoce; uno mal formado se descarta sin
  desconectar. **Sin confirmar:** el bundle del oficial no maneja ninguno
  de los cuatro (relee la lista al abrir la página), así que no se sabe si
  Discord se los manda a una sesión de usuario.
- **Mensaje bloqueado.** Un `400` con código `200000`
  (`AUTOMOD_MESSAGE_BLOCKED`), `200001` (`AUTOMOD_TITLE_BLOCKED`) o
  `200002` en un envío, una edición, un envío con archivos o un título
  termina el comando con `Event::AutoModBlocked { failure, notice }`, y
  `rune-core` lo convierte en el `Event::Failed` de siempre con el motivo
  «la moderación automática del servidor lo bloqueó: «…»», que la línea de
  fallo del mensaje muestra («No se envió: …»). `notice` es el `message`
  de la respuesta, como lo muestra el oficial: se lee sólo para esos
  códigos, sin caracteres de control y hasta 300 caracteres; las frases
  genéricas en inglés de Discord («Message was blocked by automatic
  moderation», «Title was blocked…») cuentan como sin mensaje y RUNE lo
  dice en el idioma de la persona. **Sin confirmar:** que el `message`
  traiga el mensaje propio de la regla (el oficial muestra lo que venga, y
  si no viene nada, su texto), y qué frase genérica manda Discord en otro
  idioma ahora que la sesión manda `X-Discord-Locale` (identidad de
  navegador): una traducida se mostraría como si fuera el mensaje de la
  regla, redundante pero cierta.
- **Verificación.** Las tres escrituras pasan por el hCaptcha común de
  cualquier escritura (test con fixtures para crear una regla).
- **Capacidad:** `automod`; la interfaz pregunta a
  `ServerAbilities::manage_automod` (`MANAGE_GUILD`).
- **Capturas (simulación):** [lista](img/automod-2026-10-19.png),
  [editor](img/automod-editor-2026-10-19.png) y
  [mensaje bloqueado](img/automod-bloqueado-2026-10-19.png) (Xvfb
  privado; `--action ss:open:2 --action ss:page:automod`, más
  `--action after:ss:am:rule:900001:edit` para el editor, y
  `--channel general --capture-draft 'Vengan a discord.gg/rustaceos'
  --action jump:latest --action send` para el bloqueo).

Para probar contra Discord (el dueño): en un servidor propio, abrir
Ajustes del servidor › AutoMod y comprobar que lista las reglas que muestra
el cliente oficial; crear una de palabras con un mensaje propio, una
alerta y un aislamiento; editarla, desactivarla y borrarla, mirando en el
oficial que cada cambio quedó; con otra cuenta sin permisos, escribir una
palabra de la regla y ver que la línea de fallo dice el mensaje propio; y
ver si llegan los eventos del Gateway (una regla cambiada en el oficial
debería verse en RUNE sin reabrir la página).
