# Invitaciones: cuenta activa, vista previa y aceptación

**Estado actual (2026-10-16): aceptación nativa reactivada** por decisión del
dueño, después de la pausa del 2026-10-14. Si Discord pide
un hCaptcha, el modal muestra «Discord pide una verificación» y «Verificar»
abre una ventana aislada de RUNE donde lo resuelve la persona; con esa
respuesta el adaptador repite la misma aceptación una sola vez. **No probado
contra el servicio real.** Autorización y alcance en
[SECURITY.md](SECURITY.md#verificación-de-invitaciones-autorizada-el-2026-10-16).
Desde el 2026-10-19 un rechazo dice su causa, queda en el log y ofrece
abrir la invitación en el navegador ([abajo](#motivo-del-rechazo-y-salida-por-el-navegador-2026-10-19)).
Las secciones siguientes conservan el registro de cada etapa; donde dicen
«pausada» describen el estado de entonces.

Investigación e implementación inicial del **2026-10-14**. No se abrieron cuentas,
enlaces de invitación reales ni el cliente oficial. No se ejecutó Serein ni
se modificó la autenticación de RUNE. La validación usa datos sintéticos y
servidores locales; no acredita una consulta pública ni una unión real.

## Qué puede hacerse ahora

El «+» del riel abre «Añadir un servidor». «Revisar una invitación» permite
pegar un **enlace completo** en un `TextField` de limn y consultar el destino
después de pulsar «Revisar invitación». No interpreta códigos sueltos.
Las opciones no soportadas muestran el motivo del `Permit` de Store;
crear un servidor revalida ese permiso al abrir y al enviar el formulario.

`Intent::InspectInvitation` pide al host reconocimiento local mediante el
adaptador; la UI no conoce hosts ni rutas de Discord. Un enlace no reconocido
permanece en el formulario con un error y nunca abre el navegador como
fallback. El campo está acotado a 2048 bytes, no se persiste y su `Debug`
oculta el contenido. El resultado debe coincidir con la cuenta y el texto
actuales antes de abrir el preview existente. Escape, cierre y fin de cuenta
descartan el campo; los atajos de conversación no cambian el contexto del
diálogo. Una acción obsoleta del selector tampoco reemplaza un formulario
de creación o una invitación ya abiertos.

Las seis regresiones de la entrada cubren reconocimiento explícito,
cuenta/capacidad vigente, límites, teclado, acciones obsoletas y layout.
En aquella ejecución pasaron las 26 pruebas de invitaciones y la suite
integrada de 651; son resultados históricos anteriores a la ampliación siguiente.
Se revisó la [captura nativa del campo](img/rune-invitation-input.png) usando
el fixture simulado; no es evidencia de consulta ni aceptación real.

Ampliación local del **2026-10-15**: el adaptador marca un mensaje cuando
todo su texto, salvo espacios externos, es un enlace de invitación validado.
`TextTarget::Invitation` conserva el enlace original en `TextReference.source`;
la UI no analiza dominios ni rutas. El timeline muestra una tarjeta limn con
el título genérico «Enlace de invitación» y «Revisar invitación», sin inventar
un servidor, miembros o imagen. El texto sigue visible, seleccionable y
copiable. Los mensajes con texto adicional, múltiples enlaces, código,
spoilers o enlaces Markdown no reciben esta tarjeta en este incremento.

Renderizar la tarjeta no consulta la red. Su acción emite
`Intent::ReviewInvitation { channel, message, account, source }` y revalida
mensaje vigente, contenido completo, cuenta, visibilidad y capacidad actual.
El host repite las comprobaciones de cuenta/mensaje y pide reconocimiento
local al adaptador antes de abrir el modal existente. Un enlace que ya no se
reconoce muestra un aviso y nunca se abre en el navegador como fallback.
La tarjeta no aparece al editar el mensaje ni en la muestra de ajustes;
cuando la consulta no está permitida explica el motivo y deshabilita el botón.

Un 429 del preview muestra una cuenta regresiva que despierta la UI; sólo
ofrece «Volver a consultar» al terminar la espera y exige otro clic. No inicia
consultas ni escrituras automáticamente. El modal conserva la caducidad
absoluta opcional `InvitationPreview.expires_at`: muestra la fecha informada,
«Esta invitación venció» o «Caducidad no informada por el servicio». Despierta
al vencer sin requerir un clic y vuelve a comprobar esa fecha al aceptar en
los fixtures con aceptación. Una invitación vencida puede consultarse de
nuevo explícitamente; nunca se inventa una fecha cuando el servicio la omite.
El plazo monotónico de la orden se acota a `min(60 s, tiempo restante)` cuando
la fecha es conocida; una fecha ya vencida no emite la orden. El preflight
también rechaza caducidad vencida o malformada antes de cualquier POST. Estas
regresiones se ejercitan sólo en fixtures locales con aceptación habilitada;
la capacidad de producción permanece apagada.
El campo corresponde al `expires_at` nullable ISO8601 documentado por
[Invite Resource](https://docs.discord.com/developers/resources/invite#invite-object).
La aceptación de producción sigue pausada.

RUNE incorpora un **modal nativo de limn** para las invitaciones reconocidas
por el adaptador `discord-session`. Muestra el destino y la cuenta conectada
en RUNE y consulta metadatos públicos. El adaptador experimental
`discord-session` conserva el código y los fixtures de aceptación explícita,
pero no habilita la operación en la aplicación. **Es una operación no oficial**,
separada de OAuth2. Las pruebas locales no acreditaron compatibilidad real;
el posterior intento del dueño produjo verificación y cierre de Gateway.
OAuth2 y el proveedor simulado también conservan la aceptación desactivada.

La navegación externa genérica no conserva la identidad de RUNE:

- `crates/rune-ui/src/lib.rs`, `clicked`, reconoce enlaces Markdown y
  produce `Intent::OpenUrl`.
- `apps/desktop/src/main.rs`, `Intent::OpenUrl`, consulta primero al
  adaptador: las invitaciones reconocidas abren el modal. Los demás enlaces
  usan `open_url` y `xdg-open` en Linux. El manejador del sistema elige el
  navegador/aplicación y su sesión; RUNE no transmite ni verifica esa cuenta.
- `crates/rune-login/src/webview.rs:121` crea una sesión WebKit efímera. El
  auxiliar termina después del handoff (`:320`); no conserva una ventana o
  un perfil de navegador reutilizable para aceptar invitaciones.

Por eso una WebView nueva no garantiza la cuenta correcta. Retener aquella
sesión o iniciar otra autenticación sería un cambio de ciclo de vida,
aislamiento y UX; no basta con cargar otra URL. No se propone importar
cookies de otro navegador ni inyectar una credencial de RUNE en una página.

## Interfaces oficiales verificadas

| Operación | Evidencia y alcance |
| --- | --- |
| Consultar una invitación | `GET /invites/{invite.code}` devuelve metadatos; `with_counts` agrega conteos aproximados. La estructura distingue servidor, grupo y amistad; no debe interpretarse todo enlace como un servidor. La página revisada no documenta una operación general de aceptación. [Invite Resource](https://docs.discord.com/developers/resources/invite#get-invite). |
| Agregar un miembro a un servidor | `Add Guild Member` exige autorización OAuth2 `guilds.join` del usuario y un token **bot** de la misma aplicación. El bot debe pertenecer al servidor y tener `CREATE_INSTANT_INVITE`. Puede dejar al miembro pendiente de screening. No satisface la aceptación genérica sin bots solicitada para RUNE. [Guild Resource](https://docs.discord.com/developers/resources/guild#add-guild-member). |
| Alcance OAuth2 | `guilds.join` remite al endpoint de miembros; no concede por sí solo el control general de una cuenta. RUNE actualmente solicita `identify guilds` (`crates/rune-discord/src/auth.rs:49`). [OAuth2](https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes). |
| Invitaciones del Social SDK | `AcceptActivityInvite` entrega el secreto de unión configurado en Rich Presence para juegos/lobbies. No se presenta como aceptación de cualquier enlace de servidor. [Managing Game Invites](https://docs.discord.com/developers/discord-social-sdk/development-guides/managing-game-invites#receiving-game-invites-in-the-sdk). |

La documentación de GET confirma la ruta y el objeto. El adaptador nuevo
realiza la consulta sin credenciales; sus fixtures verifican ese transporte,
pero aún no se comprobó una respuesta real de Discord en esta iteración.
Un enlace consultable nunca concede membresía ni permisos locales. No se
ampliaron scopes ni se añadieron bots.

Las notas públicas de Discord del 4 de junio de 2026 describen una mejora
de accesibilidad que anuncia la cuenta con la que se acepta una invitación.
Esto respalda hacer explícita la identidad en la acción; no documenta un
endpoint de aceptación. [Discord Patch Notes](https://discord.com/blog/discord-patch-notes-june-4-2026).

## Qué hace Serein

Checkout estudiado:
[`d1af883c9d7dda90d1261a2d15fc852a5445204d`](https://github.com/ViceVerse-cz/Serein/tree/d1af883c9d7dda90d1261a2d15fc852a5445204d).
Las rutas de esta sección son relativas a `references/serein/`.
Son observaciones del código, no resultados de ejecución.

| Responsabilidad | Fuente |
| --- | --- |
| Reconocimiento de enlaces y códigos | `crates/ui/src/invites.rs:10` valida esquema, host, ruta, usuario/contraseña y puerto. `input_code` (`:36`) admite además un código pegado explícitamente; no convierte una palabra suelta del chat en tarjeta. `crates/model/src/server_invites.rs:105` limita el código a 100 bytes alfanuméricos ASCII, guion o guion bajo. |
| Tarjetas de mensaje | `ui/src/invites.rs:47` limita a tres códigos por mensaje y evita código Markdown y spoilers. Las tarjetas tienen altura fija 108 (`:91`); solicitan metadatos de forma acotada y muestran estado de miembro, carga o error (`:116`). |
| Modal de consulta/confirmación | `ui/src/join_server.rs:144` sólo precarga el campo. `:202` dibuja un diálogo nativo de ancho 460, ligado a la generación de sesión. `:774` separa consultar y aceptar: nunca hace ambas operaciones en un clic. |
| Caché y respuestas obsoletas | `client-core/src/invites.rs:88` conserva hasta 32 previews, con vida de cinco minutos y una consulta pendiente; `:114` limita el modelo aceptado a 4 KiB. No inserta el preview en el conjunto de servidores de la cuenta. |
| Membresía y navegación | `client-core/src/invites.rs:242` comprueba sesión, conexión, preview vigente y ausencia de solicitud pendiente. La respuesta HTTP no concede acceso local; la navegación espera al servidor recibido por Gateway y expira a los 60 segundos (`:224`). |
| HTTP de consulta | `discord-api/src/lib.rs:547` utiliza el GET documentado y limita la respuesta a 64 KiB. `discord-protocol/src/invites.rs:16` decodifica el resultado a un preview acotado. |
| HTTP de aceptación | `discord-api/src/lib.rs:564` envía `POST /invites/{code}` con `{}`, limita a 64 KiB y extrae el servidor. Lo identifica como endpoint de usuario **no oficial**. Su existencia en Serein no lo convierte en una interfaz pública autorizada. |

El helper HTTP global de Serein agrega fingerprint y User-Agent de otro
cliente (`discord-api/src/lib.rs:151`). Hasta el 2026-10-19 RUNE conservó
su identificación propia; desde entonces presenta, reimplementada, la del
cliente web en un navegador WebKit
([identidad de navegador](DISCORD_SESSION.md#identidad-de-navegador-2026-10-19-rama-identidad-de-navegador)).
La lectura de Serein sigue sin demostrar que la aceptación funcione. Tampoco se
traslada su subsistema de desafíos: una verificación requerida detiene el
intento, sin resolverla, reenviarla ni automatizar otra solicitud.

Hay regresiones de parsing de origen/ruta
(`ui/src/invites.rs:362`, `:385`), consulta sin conversación seleccionada
(`client-core/src/invites.rs:137`) y consulta/confirmación separadas con
limpieza al cambiar de sesión (`ui/src/join_server.rs:929`). Se leyeron sus
aserciones; **no se ejecutaron** en esta tarea.

El workspace de Serein declara `MIT OR Apache-2.0` en `Cargo.toml:10`.
Esta investigación no copia código ni activos. Cualquier futura
reutilización debe conservar los avisos pertinentes; aquí se propone una
implementación propia sobre el contrato de RUNE.

## Contrato implementado en RUNE

La UI no contiene hosts, rutas HTTP ni tipos wire. El adaptador reconoce
la invitación con una función pura y devuelve un destino opaco; los enlaces
comunes conservan su apertura externa. Reconocer un enlace no dispara una
aceptación ni una redirección.

Las responsabilidades están separadas:

1. `rune-provider/src/invitations.rs` define `InvitationTarget`,
   `InvitationPreview`, errores estáticos y `InvitationResult` por petición.
   `Provider::invitation_target` reconoce URLs sin I/O;
   `inspect_invitation` obtiene metadatos. `Capabilities::invitation_preview`
   es falsa por defecto. La escritura es independiente:
   `Command::AcceptInvitation { request, target, server, account }`,
   `InvitationJoinError` y `Event::InvitationAcceptance` declaran petición,
   destino y resultado sin credenciales. `invitation_accept` es otra
   capacidad, falsa por defecto; `Store::accept_invitation` exige cuenta,
   conexión y estado confirmado, nunca una instantánea de SQLite.
2. `rune-discord-session/src/invitations.rs` valida HTTPS/origen/ruta/código,
   normaliza el enlace y usa un cliente HTTP independiente sin credenciales,
   cookies ni redirecciones. Sólo hace GET al endpoint fijo, con máximo
   64 KiB y timeout. Devuelve servidor, nombre, descripción e imagen
   opcionales, conteos numéricos aproximados y caducidad opcional validada
   como RFC3339. Desde el 2026-10-18 también entiende invitaciones de grupo,
   de amistad y de ingreso como invitado ([abajo](#todas-las-clases-y-tarjetas-en-texto-mezclado-2026-10-18)),
   y desde el 2026-10-19 las de evento ([abajo](#invitaciones-de-evento-2026-10-19)).
3. `rune-core/src/invitations.rs` mantiene una tarea y una cola acotada,
   cancela consultas reemplazadas, filtra resultados viejos por generación
   local y conserva `Retry-After` antes de encolar el resultado. Cerrar el
   modal o limpiar la sesión no descarta ese límite público de transporte.
   `Client::poll_invitation` entrega resultados aparte de los eventos de
   cuenta; nada de este flujo se persiste en SQLite.
4. `Store::invitation_preview()` devuelve un `Permit` a partir de capacidad
   y cuenta actual. `rune-ui/src/invitations.rs` guarda `Ui::invitation`,
   vincula respuesta/petición/destino/cuenta y usa `Modal`, `Button`,
   `ScrollArea`, `Spinner` y `RemoteImage` de limn. La identidad procede de
   `Store::me_user()`, nunca del inviter remoto ni del navegador. El primer
   acceso puede partir de un enlace, del campo para pegar un enlace completo
   o de la tarjeta de un mensaje completo reconocido. Las tarjetas no hacen
   consultas automáticas y el campo no acepta códigos sueltos.
5. La aceptación pasa por la cola autenticada de `discord-session`, no por
   el servicio público de previews. Se revalida cuenta/destino/conexión,
   se vuelve a consultar el destino antes de escribir y se envía un único
   POST. La respuesta debe nombrar el servidor mostrado. `GUILD_CREATE`
   entrega la pertenencia y los canales con permisos: el ACK HTTP no crea
   ningún servidor, canal ni permiso en `Store`.
6. `rune-core/src/invitation_acceptance.rs` conserva una única escritura,
   aun si se cierra el modal. Una falta de respuesta a los 60 segundos es
   ambigua, no libera la operación para repetirla. Los resultados se atan
   a la petición y generación de sesión; la UI no recicla sus IDs al cerrar
   sesión. `Command::AcceptInvitation.expires_at` lleva el plazo monotónico desde el clic hasta el
   adaptador, sin persistirse ni salir a la red. Así una cola lenta tampoco
   inicia una escritura después del timeout anunciado por la UI.

El modal mantiene destino, cuenta actual, conteos, **Copiar enlace** y
**Cerrar**. Con la capacidad disponible aparece **Aceptar invitación**;
abrir o consultar el enlace nunca acepta por sí solo. Doble clic no duplica
la operación. Tras el ACK espera el servidor y un canal visible; tolera que
Gateway llegue antes del HTTP. Un servidor confirmado siempre ofrece
**Abrir servidor**, incluso sin canales visibles, sin inventarlos.

Cerrar desarma la navegación automática, sin afirmar que revierte una
solicitud enviada. El timeout de navegación y los límites de frecuencia
despiertan la UI aunque no haya entrada. Los errores inciertos no ofrecen
reenvío y un 429 conserva su espera al cerrar y volver a abrir el modal.

No se abre automáticamente el navegador como fallback de una aceptación
no disponible: volvería a introducir la ambigüedad de cuentas. Una acción
externa separada, si posteriormente se decide ofrecerla, debe nombrar el
navegador y no prometer usar la cuenta de RUNE.

## Límites y validación

Validación dirigida del incremento del 2026-10-15, con `rtk` y
`nix-shell --arg withLogin true --arg withDocs true`:

- `cargo test -p rune-ui -p rune-discord-session --all-features invitation`:
  **54 aprobadas**, 307 filtradas, dos suites (caducidad y cooldown incluidos).
- `cargo test -p rune-components -p rune-ui -p rune-discord-session -p rune-desktop --all-features invitation_`:
  **30 aprobadas**, 454 filtradas, cuatro suites, después de implementar la tarjeta.
  Se comprobó primero el fallo funcional de los nuevos tests de detección,
  componente y acción de UI. Los filtros se solapan: sus cuentas no se suman.
- Después de acotar el deadline y validar fechas en preflight, el filtro
  `invitation` de UI/adaptador pasó **59 pruebas**, 309 filtradas, dos suites.
  Los tres RED previos cubrieron plazo corto, fecha inválida/vencida y GET 200
  vencido sin POST. Es otra ejecución solapada, no un total acumulado.
- El fixture de captura espera el historial simulado antes de insertar el
  mensaje tipado. `--provider mock --screenshot <archivo> --capture-invitation
  --action invitation:close` deja visible la tarjeta para revisión nativa sin
  consultas al servicio. Esta ampliación no acredita todavía una nueva
  captura ni sustituye la suite integrada del agente principal.

La consulta pública usa origen HTTPS fijo y no sigue redirecciones. No recibe
credenciales. La aceptación usa exclusivamente la sesión activa en RAM,
dentro del adaptador; errores y tiempos están acotados. Los destinos
redactan su `Debug`, no se registran cuerpos remotos, y un 403 no inicia un
login ni cambia de endpoint. El modo de capturas del host bloquea estas
consultas para no transformar un fixture gráfico en acceso a red real.

Los fixtures nuevos cubren parsing de origen/ruta, GET sin cabeceras de
cuenta, redirección hacia un servidor trampa, respuestas excesivas,
invitaciones de tipo distinto, cancelación/reemplazo, cooldown incluso si
se cierra antes de consumir un 429, identidad visible desde el Store,
resultados obsoletos, abrir un servidor existente, copiar el enlace y
ausencia de aceptación al consultar o copiar. Validación histórica del preview:

- `cargo test -p rune-discord-session -p rune-ui invitations --lib`:
  **9 tests aprobados** (5 de transporte/parsing, 4 de UI).
- `cargo test -p rune-core invitations --lib`: **3 tests aprobados**.
- `cargo test -p rune-state invitation_preview --lib`: **1 test aprobado**.

Todos los comandos se ejecutaron mediante `rtk` en el entorno Nix. Los
checks de la nueva aceptación se registran abajo; los resultados anteriores
no sustituyen esa validación. Las capturas suministradas por el dueño muestran
el preview real, pero no se guardan en el repositorio ni prueban una aceptación.

**No existe aquí una aceptación por una interfaz pública verificada**:
la implementación pertenece al adaptador experimental no oficial. Entrar al
servidor no implica que todos sus canales permitan escribir. Desde el
2026-10-18 las reglas del servidor (screening) y el onboarding sí están
(ver abajo, [Reglas y onboarding al entrar](#reglas-y-onboarding-al-entrar-2026-10-18));
otras verificaciones (teléfono, correo) siguen sin estar.


## Ajuste del modal después de la revisión visual

El carácter `≈` expresaba conteos aproximados; no era un fallo de fuente.
Ahora la cifra tiene tooltip y descripción accesible «Cantidades aproximadas»
sin añadir ese símbolo a la línea. El cuerpo tiene altura natural y usa
el footer fijo de `Modal`; se eliminó el scroll anidado de 260 puntos que
producía el espacio vacío. Copiar/Cerrar usan variantes Secondary/Ghost.
La apertura de un servidor ya conocido vuelve a comprobar su ID al actuar.

El [flujo público de Discord](https://support.discord.com/hc/en-us/articles/360034842871-How-do-I-join-a-Server)
sí incluye una aceptación explícita después de la invitación. RUNE conserva
esa separación entre consulta y confirmación, con su propio aspecto compacto
en limn. La consulta pública no concede membresía.

## Validación inicial de la aceptación (2026-10-14)

La revisión cruzada corrigió reintentos de transporte implícitos, solicitud
en cola que sobrevivía a reconexión, invalidación indebida de sesión ante
401 de un GET anónimo, doble entrega de una misma petición y liberación
prematura de la operación al fallar el encolado de logout. Un resultado
terminal ambiguo permite consultar otro destino; un timeout local mantiene
el bloqueo hasta que el proveedor termine. Ninguno repite automáticamente
el ingreso ni restaura una navegación cancelada.

En `nix-shell --arg withLogin true --arg withDocs true`:

- `cargo fmt --all -- --check`: correcto.
- `cargo check --workspace --all-features`: correcto.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: correcto.
- `cargo test --workspace --all-features`: **627 aprobadas**, seis ignoradas,
  23 suites. Incluye 20 pruebas de UI de invitaciones, contrato/capacidades,
  cola/deadlines del core y fixtures del adaptador.
- REA ejecutó el binario propio del adaptador con filtro `invitation`:
  **22 aprobadas**, salida 0. [Evidencia completa](evidence/rea-invitations-2026-10-14.json)
  y [límites de observación](REA.md#aceptación-de-invitaciones-2026-10-14).
- `cargo build --release -p rune-desktop --features session-login`: correcto.
  La [captura nativa del modal](img/rune-invitation.png), con fixture simulado,
  fue revisada a 1396×1660 píxeles. El proceso Wayland observado por REA salió
  con código 0; [registro](evidence/rea-invitation-window-2026-10-14.json).

El ejecutable release validado tiene SHA-256
`ff523ce1aaa959a0100e260e6ab98cb164683efe36c1158b1a547e200a1982b1`.
La captura se reproduce con `--provider mock --screenshot <archivo>
--capture-invitation`; el parser rechaza ese fixture con un proveedor real.
La ventana de cuenta ya abierta no se cerró ni reinició: necesita un nuevo
arranque para cargar este ejecutable, y la sesión anterior no se exporta.

Durante esa validación automatizada no se aceptó ninguna invitación real ni
se usó la cuenta abierta. Después la operación quedó en pausa hasta el
2026-10-16.

## Checkpoint de tarjetas, caducidad y cooldown (2026-10-15)

La tarjeta de un mensaje cuyo contenido completo es un enlace reconocido
usa limn y exige «Revisar invitación» antes de consultar. Al actuar, UI y host
comprueban de nuevo cuenta, canal y contenido actual; recibir el mensaje no
dispara consultas ni acepta el destino. Enlaces mezclados con texto todavía
no generan esta tarjeta.

El cooldown muestra el tiempo restante y permite un reintento explícito al
vencer. Una caducidad conocida se muestra y limita tanto el comando en cola
como su preflight; una fecha desconocida aparece como desconocida. La
aceptación de producción sigue deshabilitada.

Las **59 pruebas dirigidas** del incremento y el checkpoint integral de
**716 aprobadas, seis ignoradas**, con formato, Clippy y release correctos,
se complementan con la [tarjeta](img/invitation-card-2026-10-15.png) y el
[modal](img/invitation-modal-2026-10-15.png) capturados con datos simulados.
El [registro](evidence/implementation-checkpoint-2026-10-15.json) y el
[alcance REA](REA.md#checkpoint-local-de-storage-e-invitaciones-2026-10-15)
conservan hashes y limitaciones. No acreditan membresía ni una aceptación real.

## Tarjetas con vista previa automática (2026-10-16)

Reemplaza la regla anterior de «las tarjetas no hacen consultas automáticas».
Con `invitation_preview` permitido, la tarjeta de un mensaje visible consulta
sola el servidor, como Serein (`crates/ui/src/invites.rs`, sin copiar código),
y muestra: «Te invitaron a unirte a un servidor», «Ya sos miembro de» o
«Invitación no disponible»; la marca del servidor (imagen o iniciales),
nombre, en línea y miembros. Sin el permiso sigue la tarjeta genérica.

- **Acotado** (`rune-ui/src/invitation_cards.rs`): sólo filas visibles del
  timeline abierto; una consulta por invitación aunque la repitan varios
  mensajes; como máximo ocho en vuelo (y `rune-core` contesta «no disponible»
  sin preguntar al proveedor si recibe más); una respuesta vale cinco
  minutos, un fallo uno (o el `Retry-After`, compartido con el modal). Una
  consulta sin respuesta en 30 s libera su lugar. Respuestas de otra petición
  u otra cuenta se descartan. Al refrescar se sigue mostrando la anterior.
- **Transporte**: el mismo GET público sin credenciales del modal, no el
  autenticado de Serein. El modal y las tarjetas no se cancelan entre sí.
- **Unirse** abre la página canónica de la invitación en el navegador del
  sistema mediante `Intent::OpenInBrowser`, con el tooltip «Se abre en el
  navegador: unirse desde RUNE está en pausa». No se usa `Intent::OpenUrl`
  porque el host convierte las invitaciones en el modal de RUNE. El adaptador
  ahora da `https://discord.com/invite/{code}` como `InvitationTarget.url`,
  así la UI no construye direcciones del servicio. Ningún botón de la
  tarjeta emite `Command::AcceptInvitation`; `invitation_accept` sigue en pausa.
- **Ir al servidor** aparece cuando el servidor del preview ya está en
  `Store` y lo abre como el modal.

La captura [invite-card-2026-10-16.png](img/invite-card-2026-10-16.png) usa
`--provider mock --screenshot <archivo> --channel general
--capture-invitation --action invitation:close`: el fixture publica tres
enlaces (para unirse, ya miembro, no disponible) y contesta sus consultas
sin proveedor. Es una evidencia simulada, no una consulta real.

## Todas las clases y tarjetas en texto mezclado (2026-10-18)

Rama `invitaciones`. Reemplaza tres reglas anteriores: una tarjeta ya no
exige que el mensaje sea sólo el enlace, la vista previa ya no entiende
sólo servidores, y «Unirse» ya no abre siempre el navegador (la aceptación
nativa volvió el 2026-10-16). **Sólo fixtures, sin probar contra el
servicio real.**

**Qué hace el cliente oficial** (REA estática de los bundles públicos ya
descargados, build 641004, `web.4d4b2f75fdea5470.js`; leídos con `grep`,
nada ejecutado; [registro](REA.md#cliente-web-oficial-clases-de-invitación-y-enlaces-con-tarjeta-2026-10-18)):
busca los enlaces en todo el texto salvo el código, quita la puntuación
final, guarda uno por código y se queda con **los diez primeros** de cada
mensaje (`codedLinks`, `.slice(0,10)`). El tipo de invitación es `GUILD` 0,
`GROUP_DM` 1 y `FRIEND` 2; la bandera `IS_GUEST_INVITE` (1) marca el
ingreso como invitado. Los tres tipos se aceptan con el mismo
`POST /invites/{code}`; después, el de amistad lleva a la conversación con
quien invitó, el de grupo al grupo y el de invitado no pasa por el
onboarding. Serein sólo entiende servidores.

**En RUNE:**

- **Enlaces con texto alrededor.** El adaptador (`text.rs`) marca cada
  invitación del mensaje, escrita sola, entre `<…>` o como dirección de un
  `[texto](…)`, salvo en código (de bloque o en línea) y en spoilers (la
  tarjeta revelaría el destino; Serein hace lo mismo). Una por invitación
  aunque se escriba con dos dominios, como mucho diez. También reconoce
  `ptb.` y `canary.discord.com/invite/`. La UI muestra una tarjeta por
  enlace, en orden, con acciones `msg:{id}:invitation:{n}:{acción}`.
- **Clases.** `InvitationDestination` (en `rune-provider`) dice adónde lleva
  una invitación: `Server`, `Guest { server, channel }`, `Group(canal)` o
  `Friend(persona)`. Reemplaza al `ServerId` en la vista previa, en
  `Command::AcceptInvitation`/`VerifyInvitation`, en
  `Event::InvitationAcceptance` y en `Failure::Invitation`. La sesión lee
  `type` y `flags` del GET público: un grupo se nombra por su nombre o, sin
  él, por sus integrantes (o «Grupo sin nombre»), con el ícono del canal;
  la amistad, por el nombre de quien invita, con su avatar; el invitado,
  por el servidor. Otro tipo, una bandera de invitado fuera de un servidor
  o un id que no es id no llevan a ningún lado («La invitación no es válida
  o esta conexión no la entiende»).
- **Capacidades.** Aceptar sigue pidiendo `invitation_accept`, y cada clase
  nueva la suya: `group_invitations`, `friend_invitations` y
  `guest_invitations` (`Capabilities::accepts`). `Store::accept_invitation`
  dice por qué no: sin la capacidad de esa clase, ya en el grupo, ya son
  amigos, o «Es una invitación tuya». `Store::invitation_joined` dice si la
  cuenta ya tiene lo que da la invitación (el servidor, el grupo o la
  amistad), siempre por sus propios eventos: el ACK nunca crea nada.
- **Aceptar.** La sesión revalida igual que con servidores (cuenta, enlace,
  plazo, que no esté ya en el servidor/grupo ni sea amigo ni sea su propio
  enlace), hace el GET anónimo y comprueba que la invitación siga llevando
  al mismo destino (tipo, ids y bandera exacta: 0, o 1 para invitado) antes
  y después del único `POST`. Un hCaptcha se resuelve en la ventana de RUNE
  y permite un solo reintento, como en servidores.
- **Tarjeta y modal.** La tarjeta dice «Te invitaron a unirte a un grupo»,
  «Te invitaron a ser amigos» o «Te invitaron a entrar como invitado»
  («Como invitado, un canal de voz»); ya dentro, «Ir al grupo» o «Enviar
  mensaje». «Unirse» (o «Aceptar amistad») abre la invitación en el modal
  de RUNE, que pide confirmar con «Aceptar invitación»; si la conexión no
  acepta esa clase, abre la página en el navegador y lo dice. El modal
  titula según la clase, explica qué da un ingreso como invitado y, al
  llegar lo aceptado, abre el servidor, el grupo o la conversación con el
  nuevo amigo si ya existe (crearla es una escritura: nunca sola; «Enviar
  mensaje» la pide).
- **Simulación.** El proveedor simulado tiene enlaces propios
  (`https://simulado.invalid/invite/{club,radio,jueves,amistad}`) en dos
  mensajes de `#general`, uno con tres invitaciones entre texto. Los
  consulta sin red y al aceptarlos manda lo que mandaría un servicio: un
  servidor con sus canales, un servidor con sólo el canal de voz del
  invitado, un grupo o la amistad, y después el ACK.

La captura [invite-kinds-2026-10-18.png](img/invite-kinds-2026-10-18.png)
usa `--capture-invitation` con el fixture ampliado: un mensaje con un
servidor, una radio para invitados y un grupo entre texto, y otro con una
invitación de amistad. Es una evidencia simulada.

**Qué falta:** las de aplicaciones o transmisiones (`target_type`), que
siguen rechazadas al aceptar. Las de evento llegaron el 2026-10-19
([abajo](#invitaciones-de-evento-2026-10-19)). Las reglas y el onboarding después de entrar
llegaron el 2026-10-18 (sección siguiente).

## Reglas y onboarding al entrar (2026-10-18)

Rama `reglas-y-onboarding`. Al aceptar una invitación a un servidor que
pide algo (reglas pendientes, `MemberRestrictions::pending`, u onboarding
sin terminar, `ServerOptions::onboarding` sin `onboarded`), RUNE abre el
diálogo «Antes de participar en {servidor}» apenas abre el servidor
(`rune_ui::onboarding`). Muestra las preguntas (cada opción con su emoji,
su título y su descripción; una o varias según la pregunta) y después las
reglas numeradas con «Leí y acepto las reglas», o los campos de texto y de
opción múltiple que pida. «Terminar» manda primero las respuestas
(`Command::AnswerOnboarding`) y, cuando el proveedor las confirma, las
reglas (`Command::AnswerServerRules`), como Serein y el cliente oficial;
nada se muestra antes de la respuesta. El resultado: «Listo: ya podés
participar», «Enviado: la moderación va a revisar tus respuestas» o el
rechazo. Si se cierra con «Más tarde», queda una barra sobre el
compositor («Antes de participar, {servidor} pide que aceptes sus
reglas.» con «Ver las reglas», o «Respondé las preguntas…» con
«Responder las preguntas») que vuelve a abrirlo. Mientras las reglas
esperan, el compositor está cerrado y dice «Aceptá las reglas de
{servidor} para participar» (`Store::abilities`); en revisión o
rechazado, lo dice la moderación, también después de reconectar (no
después de cerrar RUNE: ver pendientes en `tasks/todo.md`). Entrar como
invitado (`InvitationDestination::Guest`) nunca abre el diálogo ni pide
el onboarding, como el cliente oficial con `IS_GUEST_INVITE`. Si una
parte que se debe no llegó, el diálogo ofrece «Reintentar» antes de
mandar nada; si ya no se debe nada (se respondió desde otro cliente), se
cierra solo.

En la simulación, el club de la invitación `club` pide las dos cosas: dos
preguntas (una obligatoria cuyas opciones dan los roles «Fotografía» y
«Música», y la primera abre el canal oculto `#fotos`) y tres reglas.
El cuarto oscuro de `revelado` pide sólo reglas con un nombre, una opción
múltiple y un párrafo, que su moderación revisa («Enviado: …»); la radio
de `radio` tiene onboarding, que a un invitado no se le pide.
El servidor propio, «Laboratorio limn», es una comunidad con onboarding
que la cuenta edita en Ajustes del servidor › Incorporación (`PUT
/guilds/{g}/onboarding` en la sesión no oficial). Detalle del proveedor
no oficial en [DISCORD_SESSION.md](DISCORD_SESSION.md#reglas-del-servidor-y-onboarding-2026-10-18-rama-reglas-y-onboarding);
sólo fixtures, **sin probar contra el servicio real**.

Capturas simuladas (`--capture-onboarding --language es`, la barra con
`--action after:onb:close`; la página con `--action ss:open:2 --action
ss:page:onboarding`): [diálogo](img/onboarding-join-2026-10-18.png),
[barra sobre el compositor](img/onboarding-bar-2026-10-18.png) y
[Incorporación](img/onboarding-settings-2026-10-18.png).

## Motivo del rechazo y salida por el navegador (2026-10-19)

Rama `invitaciones-motivo`. Hasta entonces un rechazo de «Unirse» terminaba
en «No podés aceptar esta invitación con esta cuenta.» sin rastro en el
log. Ese texto (`InvitationJoinError::Denied`) juntaba un 401, un 403 sin
código conocido, un rechazo con código de la parte social, otra cuenta y ya
estar en el destino. **Sólo
fixtures, sin probar contra el servicio real.**

**El log dice por qué.** Cada respuesta a un paso de la aceptación deja una
línea en stderr y en `~/.local/state/rune/rune.log`, y el resultado que se
le muestra a la persona, otra:

```text
rune: invitation: preflight GET: HTTP 200
rune: invitation: POST: HTTP 403, error code 40007
rune: invitation: not accepted: Banned
```

También `HTTP 403, no error code`, `HTTP 401`, `POST after verification:
…` (el reintento tras el hCaptcha), `no answer (timed out)` y `not sent:
still waiting after a rate limit`. Un `not accepted: …` sin línea HTTP antes
quiere decir que RUNE no mandó nada (otra cuenta, ya estaba, sin conexión,
enlace o plazo inválidos). Las líneas nunca llevan el cuerpo, el token, el
código de la invitación, el destino ni la cuenta (test con fixtures
`a_refused_acceptance_leaves_its_status_and_code_in_the_log_and_nothing_private`).
**Si una aceptación falla, mandar las líneas que empiezan con `rune: invitation:`.**

**Causas con nombre**, por el código JSON de error y verificadas con la REA
del cliente web oficial ([registro](REA.md#cliente-web-oficial-por-qué-se-rechaza-una-invitación-2026-10-19-rama-invitaciones-motivo)):

| Código | Causa (`InvitationJoinError`) | Texto |
| --- | --- | --- |
| 30001 | `ServerLimit` | «Llegaste al máximo de servidores: salí de uno para entrar.» |
| 40007 | `Banned` | «Te banearon de este servidor.» |
| 30019 | `ServerFull` | «El servidor está lleno.» |
| 40069 | `InvitesPaused` | «El servidor pausó sus invitaciones.» |
| 50095 | `Region` | «Este servidor no está disponible en tu región.» |
| 20024, 20086, 700000, 700002 | `AgeRestricted` | «El servicio no deja entrar a este servidor por la edad de la cuenta.» |
| 400000–409999 | `LimitedAccess` | «Las funciones de este servidor están limitadas: no admite ingresos ahora.» |
| 40068 | `AccountLimited` | «El servicio limitó esta cuenta y no la deja entrar a servidores.» |
| 10006 | `Invalid` | «La invitación ya no es válida.» |

Un 400/403 con otro código sigue siendo `Invalid`/`Denied`, y los de
sesión, cuenta y hCaptcha siguen como antes (`rejections.rs`).

**Ya estar ahí no es un rechazo.** Si la sesión ya tiene el servidor, el
grupo o la amistad, el resultado es `AlreadyJoined`: el diálogo dice «Ya
sos miembro de este servidor.», «Ya estás en este grupo.» o «Ya son
amigos.» y, cuando los eventos de la cuenta lo traen, ofrece «Abrir
servidor», «Abrir grupo» o «Enviar mensaje», como ya hacía. El enlace de
amistad propio sigue siendo `Denied`.

**Sin «Volver a intentar» después de un rechazo.** `can_retry` queda sólo
para `Unavailable` (no se mandó nada) y `RateLimited` (al terminar la
espera). `Denied`, `Invalid` y las causas nuevas no lo ofrecen: repetir un
rechazo no cambia la respuesta y puede perjudicar a la cuenta. `Ambiguous`
y las verificaciones ya no lo ofrecían (el hCaptcha sigue con
«Verificar»). Reabrir la misma invitación en la misma sesión muestra el
rechazo anterior.

**Abrir en el navegador.** El botón (`invitation:browser`, con el tooltip
«Se abre con la cuenta de tu navegador, no con la de RUNE.») pide
`Intent::OpenInBrowser` con `InvitationTarget.url`, la página canónica que
da el adaptador (`https://discord.com/invite/{código}`, adonde redirige
`discord.gg/{código}`; la UI no arma direcciones del servicio). El host la
abre con el `open_url` de siempre (`xdg-open`, sin esperar). Aparece
siempre con un proveedor no oficial (`ProviderKind::Unofficial`), aun antes
de la vista previa, y con cualquier otro después de un rechazo. Así la
persona entra con el cliente oficial en su navegador; RUNE no comparte la
sesión con nada, y si entra, el servidor llega por el Gateway como
cualquier otro. Esto reemplaza la regla de [Contrato implementado](#contrato-implementado-en-rune)
de no ofrecer el navegador: no es un respaldo automático, nombra el
navegador y no promete la cuenta de RUNE.

**Simulación.** `https://simulado.invalid/invite/cerrado` («Sala cerrada
(simulado)») siempre se rechaza como `Banned`, y aceptar otra vez algo que
ya se tiene da `AlreadyJoined`. El modal de `--capture-invitation` usa ese
enlace: `--provider mock --screenshot <archivo> --capture-invitation
--language es --action invitation:accept` captura el
[rechazo](img/invitation-refused-2026-10-19.png).

## Invitaciones de evento (2026-10-19)

Rama `eventos-programados`. Un enlace de invitación puede nombrar un evento
programado del servidor (`https://discord.gg/{código}?event={id}`, como lo
arma el cliente oficial: [REA](REA.md#cliente-web-oficial-eventos-programados-2026-10-19-rama-eventos-programados)).

- **Contrato.** `InvitationPreview::event` trae el evento
  (`rune_model::ScheduledEvent`) cuando el enlace lo nombra; sin él, nada
  cambia. La aceptación es la común: el mismo `POST`, la misma
  verificación y el mismo único reintento; en la sesión no oficial su
  `X-Context-Properties` suma `invite_guild_scheduled_event_id`, como el
  cliente web.
- **Adaptador de sesión.** El reconocedor conserva `?event={id}` en la
  página canónica (sólo dígitos; uno mal escrito queda como la invitación
  del servidor) y la clave sigue siendo el código. La consulta pública
  agrega `guild_scheduled_event_id` y lee `guild_scheduled_event`: se
  muestra sólo el evento que nombra el enlace y del servidor al que lleva;
  uno mal formado deja la invitación del servidor como estaba.
- **Tarjeta.** Sobre el servidor, un recuadro con cuándo es («En curso»,
  «Terminó» o el día y la hora), su nombre y dónde (el canal, si la cuenta
  ya lo conoce; si no, «Un canal de voz del servidor» o «Un escenario del
  servidor»; o el lugar escrito). Quien no es miembro ve «Te invitaron a
  un evento» y «Unirse» abre la invitación en RUNE como siempre; al llegar
  lo aceptado se abre el servidor y, salvo que el servidor pida reglas u
  onboarding antes, el detalle del evento. Quien ya es miembro ve «Ver
  evento», que abre el detalle en el diálogo de eventos del servidor.
- **Simulación.** `https://simulado.invalid/invite/club?event=8000000201`
  lleva al club con su «Salida fotográfica»; está en un mensaje de
  `#general` («¿Vamos a la salida?»).

Sólo fixtures: **no se probó contra el servicio real.** Captura simulada:
[invitación de evento](img/eventos-invitacion-2026-10-19.png).

