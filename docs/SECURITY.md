# Seguridad y privacidad de RUNE

## Excepción de alcance autorizada el 2026-10-14

El dueño pidió un proveedor de sesión personal **no oficial** y una WebView
exclusivamente temporal para el login. Esta excepción sustituye la prohibición
anterior de manejar esa sesión dentro de la aplicación. No habilita recibir
credenciales en el chat, importar otros perfiles, copiar evasiones de Serein
ni inspeccionar sesiones del cliente oficial. Alcance, retención y riesgos en
[DISCORD_SESSION.md](DISCORD_SESSION.md). El dueño confirmó manualmente el
login y la identidad. La ampliación de navegación/chat está probada con
fixtures locales y requiere validación real por separado.

## Verificación de invitaciones autorizada el 2026-10-16

El dueño decidió reactivar la aceptación nativa de invitaciones en el
proveedor no oficial y mostrar en una WebView aislada el hCaptcha que Discord
pida, para que lo resuelva la persona. El alcance es solo ese desafío de una
aceptación de invitación:

- La persona pulsa «Verificar» en el modal; nada se abre ni se envía solo.
  La ventana (`rune-login`, proceso propio como el login) carga una página de
  RUNE en `https://captcha.rune.invalid/`, con CSP que solo admite
  `hcaptcha.com`/`*.hcaptcha.com`, navegación limitada a esa página y a
  hCaptcha, sesión efímera, sin herramientas de desarrollo, descargas,
  ventanas nuevas ni permisos. No se presenta como `discord.com`. Desde el
  2026-10-19 lleva el user agent de la
  [identidad de navegador](#identidad-de-navegador-autorizada-el-2026-10-19),
  el mismo que el motor trae de fábrica.
- El ayudante recibe solo el `sitekey`, el `rqdata` y si es invisible; el
  `rqtoken` y el `captcha_session_id` quedan en el adaptador. La respuesta
  vuelve una vez por la tubería privada, ligada a una capacidad aleatoria, con
  tamaño acotado, y se borra de la memoria propia al descartarse.
- El desafío queda ligado a cuenta, invitación y solicitud, con cinco minutos
  de vida y dentro del plazo de la orden. Con una respuesta, el adaptador
  repite esa misma aceptación una sola vez con `X-Captcha-Key` y, si vinieron,
  `X-Captcha-Rqtoken` y `X-Captcha-Session-Id`. Otro desafío en ese reintento
  ya no se ofrece. Cerrar la ventana, vencer el plazo, perder la conexión o
  cambiar de cuenta no envía nada.
- Sin resolución automática ni servicios de terceros. Otros servicios de
  desafío o la verificación de cuenta siguen remitiendo a la aplicación
  oficial. Hasta el 2026-10-19 tampoco llevaba `X-Super-Properties`,
  `X-Context-Properties`, número de build ni user agent de navegador; desde
  entonces lleva la
  [identidad de navegador](#identidad-de-navegador-autorizada-el-2026-10-19).

Discord puede seguir pidiendo verificación por teléfono o restringir la
cuenta, también con la identidad de navegador. Ver
[INVITATIONS.md](INVITATIONS.md).

## Ampliaciones del 2026-10-17

- **Verificación en cualquier escritura.** La misma ventana y las mismas
  condiciones de arriba valen para cualquier escritura del proveedor no
  oficial en que Discord pida hCaptcha (solicitudes de amistad, mensajes y
  demás): la resuelve la persona, el desafío queda ligado a cuenta y
  solicitud, y la respuesta permite un único reintento de esa misma
  escritura.
  **Implementado el 2026-10-17 (rama `social-2`), sólo con fixtures:**
  - Sólo escrituras: una lectura con desafío no se ofrece. Sólo hCaptcha
    válido y acotado (`rejections::challenge`); otro servicio o una
    verificación de cuenta remiten a la aplicación oficial.
  - Nada se abre solo: la escritura espera y la interfaz pregunta
    («Verificar» / «Cancelar»). Una sola ventana a la vez, contando la de
    las invitaciones; un desafío más mientras otro espera falla con motivo.
  - Atado a la conexión, a la cuenta y a esa escritura: el desafío vive cinco
    minutos; perder la conexión, cambiar de cuenta, cerrar la ventana,
    cancelar o vencer el plazo terminan la acción sin enviar nada.
  - La respuesta va una sola vez, sólo en el pedido de la misma ruta que se
    frenó (`X-Captcha-Key`, y `X-Captcha-Rqtoken`/`X-Captcha-Session-Id` si
    Discord los dio), se borra de memoria al usarse o descartarse y nunca
    sale en registros, eventos ni errores (`Debug` redactado). Un segundo
    desafío en el reintento termina la acción con motivo.
  - El reintento presenta la misma identidad que el intento que repite
    (desde el 2026-10-19, la
    [de navegador](#identidad-de-navegador-autorizada-el-2026-10-19)); lo
    comprueban las pruebas sobre los pedidos que salen. Detalle en
    [DISCORD_SESSION.md](DISCORD_SESSION.md#verificación-en-cualquier-escritura-2026-10-17).
- **Paridad con Serein.** Se usan los componentes de bots (botones, menús,
  modales) y los comandos de aplicación como en Serein. El socket local
  `discord-ipc` y la detección de juegos por la lista de procesos existen
  apagados por defecto, cada uno con su interruptor; la detección sólo mira
  nombres de procesos, no lee sus datos.
- Sigue prohibido todo lo de [Reglas](#reglas), tampoco en esos casos. La
  identidad de navegador dejó de estar prohibida el 2026-10-19 (abajo).

### Actividad local: Rich Presence y juegos (P6)

Implementado el 2026-10-17 (rama `escritorio`), sólo con fixtures y
sockets en carpetas temporales; detalle en
[PLATFORM.md](PLATFORM.md#actividad-de-este-equipo).

- **Apagados por defecto**, cada uno con su interruptor y su explicación en
  Ajustes › Sistema › Actividad. Apagado, el socket no existe y no se mira
  ningún proceso.
- **Socket `discord-ipc-N`**: sólo en `$XDG_RUNTIME_DIR`, y sólo si es una
  carpeta del usuario que nadie más puede abrir (nunca `/tmp` ni otra
  compartida); el socket es 0600 y cada conexión se verifica con
  `SO_PEERCRED` como del mismo usuario. No hay WebSocket ni puerto TCP
  (Serein abre `127.0.0.1:6463–6472`; RUNE no). Un socket de otra
  ejecución sólo se reemplaza si es de este usuario y nadie responde.
- Se habla sólo el protocolo local: saludo con `v: 1` y un `client_id`
  numérico en 10 s, tramas de hasta 16 KiB que terminan en 5 s una vez
  empezadas, hasta 8 programas, una pausa de 100 ms tras cada orden.
  `SET_ACTIVITY` exige `nonce` y `pid`; texto acotado (128 caracteres, 512
  las imágenes) y sin caracteres de control; botones, secretos, grupos y
  `name` se descartan; `type` 1 (transmitir) se rechaza. `INVITE_BROWSER`
  valida el código y abre el diálogo de invitación de RUNE (vista previa y
  confirmación de la persona); nunca une a nada por sí solo. `AUTHORIZE`, `SUBSCRIBE` y todo lo demás
  se rechazan con el código 4000: ningún programa recibe credenciales,
  mensajes ni datos de la cuenta más allá de su id y nombre en `READY`.
- Publicar es del proveedor: la sesión no oficial nombra la aplicación con
  `GET /applications/{id}/rpc`, resuelve claves de imágenes con
  `GET /oauth2/applications/{id}/assets` y direcciones `https` con
  `POST /applications/{id}/external-assets` (como Serein), con caché, y
  manda la actividad en el op 3 documentado junto al estado; invisible no
  publica nada. Una aplicación que Discord no nombra no se publica. Con la
  misma identidad de navegador que el resto del adaptador (2026-10-19).
- **Juegos**: cada 10 s, de los procesos del usuario (los ajenos no se
  pueden leer), sólo la ruta de `/proc/N/exe` y los primeros 512 bytes del
  primer argumento de `/proc/N/cmdline`; nunca el resto de la línea de
  comandos, su memoria ni sus archivos. Hasta 4096 procesos por vuelta. La
  lista de juegos es la pública de Discord (`/applications/detectable`),
  pedida **sin** la credencial, acotada a 24 MiB y leída fuera del runtime;
  sólo se guarda en memoria.
- **Instancia única**: el mismo tipo de socket (`rune-instance.sock`, mismo
  usuario); lo que llega son a lo sumo ocho enlaces de una línea, que se
  abren como uno cliqueado en RUNE (los ajenos preguntan antes).

## Identidad de navegador autorizada el 2026-10-19

El dueño quitó la prohibición de presentar el proveedor no
oficial como el cliente web de Discord en un navegador, como hace Serein:
user agent, `X-Super-Properties`, huella y propiedades del Identify, con una
sola identidad coherente en REST, Gateway y la WebView donde la persona
resuelve el hCaptcha. **Implementado el 2026-10-19 (rama
`identidad-de-navegador`), sólo con fixtures, sin probar contra el servicio
real.** Detalle y valores exactos en
[DISCORD_SESSION.md](DISCORD_SESSION.md#identidad-de-navegador-2026-10-19-rama-identidad-de-navegador).

- **Una sola identidad** (`rune-discord-session`, módulo `browser`): el
  cliente web en WebKit sobre Linux. La presentan todos los pedidos a la API
  (también los públicos, sin la credencial), el Gateway y la voz (apretón e
  Identify), las subidas al almacenamiento (sólo el user agent) y las dos
  ventanas de `rune-login`, login y hCaptcha, que fijan el mismo user agent.
- **WebKit, no Chrome.** Las ventanas son WebKitGTK: el cliente web que
  corre en la de login se clasifica como Safari por `navigator.vendor`, y
  hCaptcha ve el motor WebKit diga lo que diga el user agent. Serein manda
  Chrome en Linux desde un WebKitGTK; RUNE dice el motor que de verdad
  resuelve el desafío, como Serein hace en macOS.
- **Qué se manda:** user agent, `X-Super-Properties`, `X-Discord-Locale`,
  `X-Discord-Timezone`, `Accept-Language`, `Referer` y, en las escrituras,
  `Origin`; la aceptación de invitaciones suma `X-Context-Properties` como
  el modal de invitación del cliente web. `X-Fingerprint` no: el cliente
  web lo borra al iniciar sesión y sólo lo usa sin sesión.
- **Número de build:** se lee al arrancar de la página pública
  `https://discord.com/app` (`window.GLOBAL_ENV.BUILD_NUMBER`), sin
  credencial, con 5 s y 1 MiB de tope; si falla, 641004 (el último leído
  por REA).
- **Nada de la cuenta en la identidad:** la credencial va sólo en
  `Authorization` (y en el `token` de Identify y Resume). Las propiedades
  llevan dos ids aleatorios por proceso, el idioma y la zona horaria del
  sistema, nada más; las pruebas lo comprueban sobre lo que sale.
- **Siguen prohibidos:** automatizar, resolver o reenviar CAPTCHA o MFA (los
  resuelve la persona en la ventana de RUNE, con un único reintento), leer
  sesiones o datos locales del cliente oficial y acceder a cuentas ajenas.
  `launch_signature`, que firma el WebAssembly del cliente web, va en `null`
  como antes de que ese módulo cargue: no se imita.
- **Riesgo:** presentarse como navegador no asegura que Discord acepte una
  invitación ni que no pida el teléfono o restrinja la cuenta.
- **Windows (rama `windows-login`, sin probar en un Windows real):** las
  ventanas son WebView2, y la identidad es la de ese motor: un auxiliar
  oculto lee, en una página vacía propia de RUNE, lo que el cliente web
  usaría (user agent, marcas de client hints, idioma y zona), y REST, el
  Gateway, la voz y las ventanas presentan eso mismo; los pedidos a la API
  suman las client hints que manda Chromium. Lo leído se valida (texto
  visible y acotado, sin comillas en las marcas) antes de ir a una
  cabecera, y no lleva nada de la cuenta. Sin WebView2 Runtime no sale
  ningún pedido y RUNE da el enlace oficial de Microsoft para instalarlo
  ([detalle](DISCORD_SESSION.md#identidad-por-plataforma-2026-10-19-rama-windows-login)).

## Reglas

1. **Credenciales fuera del agente y del almacenamiento general.** No
   recibir contraseñas, tokens o MFA en chat, logs, capturas ni configuración.
   El proveedor no oficial recibe únicamente la sesión generada dentro de su
   propia ventana temporal, con interacción del dueño. No hay importación
   desde otros navegadores o el cliente oficial, ni entrada manual de tokens.
2. **Inspección estática de artefactos autorizada el 2026-10-15**, para
   entender módulos y flujos y guiar la reimplementación nativa. No incluye
   perfiles, credenciales, memoria de sesiones, evasión de controles ni
   importación de código o recursos propietarios. Tampoco autoriza inventar endpoints.
   **Ampliación del 2026-10-16:** el dueño autorizó usar en el proveedor no
   oficial las rutas y mensajes de Gateway no documentados que Serein ya usa
   (por ejemplo el perfil completo con datos en común y la lista de miembros
   por suscripción), con el mismo ritmo, cola y clasificación de rechazos que
   el resto del adaptador. Siguen excluidos: procesar o reenviar CAPTCHAs
   (salvo el hCaptcha que resuelve la persona, autorizado arriba) y
   cualquier evasión de controles antiabuso. La exclusión de cambiar la
   identidad del cliente se levantó el 2026-10-19
   ([identidad de navegador](#identidad-de-navegador-autorizada-el-2026-10-19)).
   **Ampliación del 2026-10-16 (REA):** el dueño autorizó usar REA sobre el
   cliente oficial de Discord (el paquete instalado y los bundles JavaScript
   públicos que sirve discord.com) como fuente principal, para ir más rápido
   que leyendo solo Serein: rutas REST, mensajes de Gateway, formas de los
   payloads, comportamiento e interfaz. Lo encontrado puede usarse en el
   proveedor no oficial con el mismo ritmo, cola y clasificación de rechazos.
   Cada hallazgo se registra en [REA.md](REA.md) con su evidencia. Se
   reimplementa: no se copia código, estilos, íconos, sonidos ni otros
   recursos. Siguen excluidos (la identidad del cliente, desde el 2026-10-19,
   ya no): CAPTCHAs, evasión antiabuso,
   leer el perfil, la caché, el almacenamiento local o la memoria del cliente
   oficial (pueden tener sesiones y mensajes) y ejecutar el cliente oficial
   instrumentado con la cuenta sin una autorización aparte.
   Los objetos públicos del protocolo y las extensiones personales estudiadas
   en Serein se distinguen en la documentación del adaptador; estas últimas
   siguen siendo no oficiales. La investigación de interfaz se hizo con fuentes
   públicas ([DISCORD_RESEARCH.md](DISCORD_RESEARCH.md)); las inspecciones
   REA de RUNE y del artefacto instalado de Discord se distinguen en
   [REA.md](REA.md), con sus límites y tipos de evidencia.
3. **OAuth2 con consentimiento:** el proveedor experimental conserva el
   access token limitado a `identify guilds` solo en memoria. Se obtiene
   mediante código y PKCE, nunca pegándolo en la interfaz. No se guarda en
   SQLite, logs ni configuración; no se usa refresh token. Una futura
   persistencia de credenciales requerirá el keyring del sistema. No hay
   proveedor de bots. Ver [DISCORD_OAUTH.md](DISCORD_OAUTH.md).
4. **Etiquetado honesto**: cada proveedor declara su tipo y capacidades; la UI
   muestra "Simulación local · sin conexión real" en el panel del usuario y en
   Ajustes → Conexión, y deshabilita con explicación lo que no se puede hacer
   (adjuntos en los adaptadores reales, escribir en canales de solo lectura).
5. **Solo acciones iniciadas por la persona**: RUNE no envía nada por su
   cuenta. Las respuestas y la actividad "ambiental" existen solo dentro del
   proveedor simulado.

## Datos locales

| Qué | Dónde | Protección |
| --- | --- | --- |
| Últimos 500 mensajes por conversación y cuenta, los que encontró una búsqueda en todo el historial (aparte, otros 500 por conversación), borradores (con la ruta y el tamaño de sus archivos, desde el 2026-10-18, nunca su contenido), marcas locales de lectura y recordatorios de no leído, una instantánea de servidores/canales/personas para arrancar sin esperar la red; con la simulación y, desde el 2026-10-18, también con el proveedor no oficial | `$XDG_DATA_HOME/rune/rune.db` (o `~/.local/share/rune/rune.db`) | Directorio `0700`, archivo `0600` en Unix. **Cifrado por cuenta** (XChaCha20-Poly1305, clave en el llavero, fila de abajo). Sin llavero, nada de la cuenta llega al archivo: queda en memoria |
| Imágenes descargadas (desde el 2026-10-18, rama `imagenes-y-lectura`): avatares, emoji, stickers, íconos e insignias («arte compartido») y vistas previas de adjuntos y tarjetas, banners e imágenes a tamaño original («medios»), **también las de conversaciones privadas** | `$XDG_CACHE_HOME/rune/images/<digest>/{media,artwork}/` (o `~/.cache/rune/…`); la carpeta de la cuenta, con un SHA-256 sin clave de su nombre en la caché; cada archivo, con un HMAC-SHA-256 de la URL bajo una clave derivada de la de la cuenta. Ni ids ni URL a la vista | Directorios `0700`, archivos `0600` (escritos aparte con un nombre al azar y movidos enteros). **Sellados con la clave de la cuenta** de la fila de abajo (XChaCha20-Poly1305; dato asociado: cuenta, parte y nombre del archivo). Sin llavero, nada se escribe; tampoco en una sesión que la base no verificó como de la cuenta. Acotadas por parte (1 GiB de medios y 128 MiB de arte por defecto) y por retención (90 días sin mostrarse); «Borrar» en Datos y privacidad › Caché de imágenes |
| Clave de los datos locales de cada cuenta (desde el 2026-10-18) | Llavero del sistema (Secret Service en Linux), servicio `rune-cache`, una entrada por cuenta (`account=<nombre de la cuenta en la caché>`) | 32 bytes al azar; nunca en el archivo, logs ni configuración. Se borra al cerrar sesión u olvidar la cuenta; cambiar de cuenta la conserva |
| Identidad, navegación y mensajes del proveedor no oficial | Memoria; lo que se guarda, en la caché cifrada de la fila de arriba una vez verificada la cuenta | Cierre local no equivale a revocación remota |
| Ajustes con cualquier proveedor (desde el 2026-10-17): los del equipo y, por cuenta, último servidor y canal, favoritos y privados fijados, niveles de notificación, canales ocultos, categorías plegadas y temporizadores de estado | Tabla `settings` de ese mismo `rune.db`; los de la cuenta bajo `account:{proveedor}:{id de la cuenta}:` | Sólo ids y valores de la interfaz: nunca credenciales, mensajes, nombres ni datos de perfil. «Borrar datos locales» no los toca, como a los del equipo |
| Credencial del proveedor no oficial | Memoria; con la autorización del dueño (2026-10-16), también el llavero del sistema operativo para restaurar la sesión, una entrada por cuenta recordada | Nunca SQLite, archivos, logs ni configuración; cerrar sesión borra la de esa cuenta; cambiar de cuenta la conserva; «Olvidar» borra la elegida; sin llavero disponible queda solo en memoria |
| Lista de cuentas recordadas del proveedor no oficial | Atributos (no secretos) de esas mismas entradas del llavero: `account=<id>` y `name=<nombre visible>`; la etiqueta repite el nombre. La fecha de creación de la entrada indica la última usada | Ningún otro dato personal (sin avatar, correo ni teléfono); se borra con la entrada |
| Diagnóstico de voz y del Gateway del proveedor no oficial (desde el 2026-10-17), y lo demás que RUNE escribe en stderr (errores del cliente, pánicos) | stderr del proceso, una línea por paso (`rune: voice …`, `rune: gateway: …`); desde el 2026-10-18 también `$XDG_STATE_HOME/rune/rune.log` (o `~/.local/state/rune/rune.log`), rotado a 5 MB con tres archivos (`rune.log`, `.1`, `.2`), sólo en ejecuciones interactivas en Unix (`apps/desktop/src/log.rs`) | Directorio `0700`, archivos `0600`. Las líneas de voz y del Gateway: sólo opcodes, códigos de cierre, modos de cifrado, conteos y el host del servidor de voz de Discord; nunca token, `session_id`, claves, mensajes MLS, ids de personas ni la IP propia (test `diagnostics_follow_each_step_and_never_show_secrets`). El resto: errores del cliente con rutas locales (la de `rune.db`, con la carpeta personal) y motivos de red, los nombres de las aplicaciones que suenan al transmitir la pantalla, una cabecera por arranque (hora y pid) y el texto de los pánicos, que puede citar la cadena que los causó (un mensaje o un nombre llegado de la red). Ninguna línea lleva credenciales ni respuestas de hCaptcha: las ventanas de login y de hCaptcha y el navegador que abre un enlace no escriben en él (su stderr va a `/dev/null`). El archivo sobrevive a cerrar sesión y a «Borrar datos locales»: se borra a mano |
| Identidad y listado de servidores obtenidos con OAuth2 | Solo memoria del proceso | El proveedor Discord no tiene caché ni restauración de datos (sólo ajustes, fila de arriba); se borran del estado al salir o fallar |

- **Windows** (rama `windows-base`, 2026-10-19, sin probar en un Windows
  real): las mismas filas viven en el perfil del usuario, que sólo él (y
  los administradores) puede leer, en vez de modos `0700`/`0600`:
  `%LOCALAPPDATA%\RUNE\rune.db`, `%LOCALAPPDATA%\RUNE\cache\images\…`,
  `%LOCALAPPDATA%\RUNE\logs\rune.log` y los archivos de antes de la ventana
  en `%APPDATA%\RUNE`. El llavero es el Administrador de credenciales: una
  credencial genérica `RUNE/{servicio}/{cuenta}` por entrada (`rune-cache`
  para las claves de la caché), protegida por el sistema para el usuario,
  guardada sólo en este equipo (no viaja con un perfil móvil); el nombre y
  el avatar son atributos legibles, como en Secret Service. Su secreto se
  borra de la memoria que devuelve el sistema antes de liberarla. Lo
  copiado al portapapeles espera en `%LOCALAPPDATA%\RUNE\copied`.
- **Ventanas de login y hCaptcha en Windows** (rama `windows-login`): cada
  ventana, y la lectura del motor, es un proceso propio con un perfil
  WebView2 InPrivate (lo navegado queda en memoria) en una carpeta nueva
  `%TEMP%\RUNE-webview\<al azar>`, del usuario. Se borra cuando ese
  navegador termina (se lo espera hasta 5 s); la que deja un auxiliar
  matado o caído la borra el siguiente, pasados 20 minutos. Nunca otro
  perfil de Edge o WebView2, nunca datos del cliente oficial. La sesión
  vuelve sólo por la tubería privada, como en Linux, nunca a logs. Que el
  modo InPrivate no deje la sesión de la página en esa carpeta lo dice
  Microsoft, no se verificó en un Windows real; por eso la carpeta se
  borra igual. No se promete borrado forense de lo que el motor haya
  tenido en memoria o en el archivo de paginación.
- **Caché cifrada por cuenta** (decisión del dueño del 2026-10-18, registrada
  en AGENTS.md). Cada cuenta tiene su clave: 32 bytes de `getrandom`,
  guardados en el llavero (`rune-cache`), nunca en disco. Con ella se sellan,
  valor por valor, con XChaCha20-Poly1305 y un nonce al azar de 24 bytes, los
  cuerpos de los mensajes (los de la historia y los que encontró una
  búsqueda), los borradores y la ruta y el tamaño de sus archivos, la
  instantánea y las marcas de lectura. Cada
  valor lleva como dato asociado su lugar (qué es, la cuenta, la
  conversación, la fila): copiado a otra fila u otra cuenta no abre. Un
  valor dañado se lee como ausente en la historia, los borradores y la
  instantánea; en la búsqueda local y en las marcas de lectura es un error
  explícito, como antes. Si el llavero ya no tiene la clave de una cuenta (se
  borró la entrada, se reinició el llavero) o la que tiene no se lee, RUNE
  hace una nueva y, al desbloquear con ella, borra lo que se selló con la
  perdida, que ya no abriría nunca y haría fallar esas lecturas en cada
  arranque; las filas en texto plano de v4 se sellan igual.
  Se eligió AEAD por columna y no SQLCipher porque es lo más chico que cubre
  todo: `chacha20poly1305` ya estaba en el árbol (voz), no cambia el SQLite
  incluido ni agrega OpenSSL, y permite una clave por cuenta, que es lo que
  hace ilegible lo de una cuenta olvidada aunque quede una copia vieja del
  archivo. El costo, a la vista: quedan en claro lo que ubica las filas (el
  nombre de la cuenta en la caché, que incluye su id; ids de conversaciones
  y mensajes; las lápidas, que son sólo ids) y los ajustes, que no tienen
  contenido y se leen antes de saber qué cuenta hay. SQLCipher cifraría
  también eso; queda como mejora si hiciera falta.
- **Imágenes en disco** (2026-10-18, rama `imagenes-y-lectura`). Como en
  Serein, las imágenes descargadas se guardan entre ejecuciones para no
  bajarlas de nuevo, y entre ellas están las de conversaciones privadas.
  A diferencia de Serein (que las guarda en claro), cada archivo es la
  imagen tal como llegó, sellada con la misma clave de la cuenta que la
  base: sin la clave (otra cuenta, el llavero borrado) no se lee, y movida
  a otro nombre o a otra cuenta tampoco abre; un archivo que no abre se
  borra al leerlo. El disco recién se abre cuando la cuenta tiene su clave
  y el host dio los límites elegidos, así nunca se poda con otros. Una
  clave nueva borra lo sellado con la perdida; olvidar la cuenta o cerrar
  sesión con el proveedor no oficial borra su carpeta, y una descarga que
  termina después no escribe nada (se cierra bajo su candado antes de
  borrar). «Borrar datos locales» vacía las dos partes; «Borrar» en la
  página de la caché, sólo una (mensajes, borradores y sesión siguen). El
  disco sigue a la base cifrada: si llega el Ready de otra cuenta o la
  sesión deja de guardarse por el límite de la cola, lo que se baja desde
  ese momento no se lee ni se escribe en disco.
  Lo que queda a la vista sin la clave: cuántos archivos hay, su fecha
  (la última vez que se mostró cada uno) y su tamaño exacto, que es el de
  la imagen más 41 bytes; con eso, quien tenga la carpeta y una imagen
  pública conocida (un avatar, un emoji) puede sospechar que se miró, por
  el tamaño, pero no confirmarlo por el nombre: los nombres llevan la
  clave. La carpeta por cuenta, en cambio, es un SHA-256 sin clave del
  nombre de la cuenta en la caché (proveedor e id), porque olvidar una
  cuenta tiene que encontrarla sin su clave: no muestra el id, pero quien
  adivine un id puede confirmar que esa carpeta es de esa cuenta. Las URL
  firmadas de adjuntos cambian al renovarse: esa imagen se vuelve a bajar
  y la vieja se va por retención o presupuesto.
- Sin llavero (o si no responde, o no se desbloquea) los datos de la cuenta
  quedan sólo en memoria, con aviso, y nunca en texto plano; un llavero que
  falla no hace crear una clave nueva, que dejaría ilegible lo guardado.
  «Borrar datos locales» funciona igual: borrar filas no necesita la clave.
  Las operaciones del llavero corren de a una y en orden: una clave no se
  crea dos veces para la misma cuenta, ni queda en el llavero si la cuenta se
  olvidó mientras se pedía.
- La caché exige una identidad estable del proveedor y la cuenta, nunca el
  nombre visible. La simulación la declara desde el inicio
  (`Provider::storage_identity`); el proveedor no oficial, recién cuando
  verificó la cuenta (`Event::StorageIdentity`), y vale hasta que la sesión
  termina. Migrar el namespace viejo de la simulación requiere una
  instantánea de la misma cuenta y conserva valores y borrados vigentes del
  destino. Un `Ready` de otra cuenta deshabilita la caché. El proveedor
  OAuth2 sigue sin usarla: con él el archivo guarda sólo ajustes.
- **Olvidar una cuenta borra sus datos y su clave.** Cerrar sesión con el
  proveedor no oficial, u «Olvidar» una cuenta recordada (también la abierta,
  que sigue abierta en memoria), manda `Event::ForgetStorage`: el núcleo borra
  en una transacción todas sus filas (con `secure_delete`, y vaciando el log
  WAL) y su entrada `rune-cache` del llavero. Así una copia vieja del archivo
  tampoco se puede leer; la clave se borra aunque la base no haya podido
  abrirse (un archivo de una versión más nueva, uno dañado). Cambiar de
  cuenta no borra nada, y dejar de
  recordar la sesión abierta (`Command::ForgetSession`) tampoco: la cuenta
  sigue abierta y vuelve a encontrar su caché si entra de nuevo. Si el llavero no deja
  borrar la clave, los datos ya se borraron y la interfaz lo avisa. La
  simulación no borra su caché al cerrar sesión; «Borrar datos locales» sí.
  Si Discord da por terminada una sesión recordada, el proveedor deja de
  recordarla pero no la da por olvidada: su caché sigue sellada con su clave
  hasta que esa cuenta entre otra vez y cierre sesión (o borre sus datos
  locales). Los ajustes por cuenta (fila de la tabla: ids de servidores y
  canales, sin contenido) tampoco se borran al olvidarla.
- `read_marks` guarda por cuenta y canal el cursor local deseado (`through`),
  el ancla manual de no leído (`unread_from`) y su revisión local. No acredita
  confirmación remota. Estas marcas sobreviven a la poda o al borrado de
  mensajes; se incluyen en la migración del namespace y en el borrado de datos
  de la cuenta. La restauración se limita a 65 536 filas; ver
  [el contrato de marcas persistidas](ARCHITECTURE.md#contratos-entre-proveedor-núcleo-y-estado).
- La instantánea no guarda presencias ni estados de voz: son efímeros y
  dicen dónde está alguien. La revisión del 2026-10-15 corrigió una
  discrepancia anterior: se descartaban al mostrar la caché, pero todavía
  se escribían. Ahora se excluyen antes de encolar y la migración v2→v3
  elimina también los valores antiguos, conservando los otros campos.
- `secure_delete` está activo: lo borrado se sobrescribe con ceros en vez de
  quedar en páginas libres del archivo. `auto_vacuum` incremental devuelve
  el espacio en la poda.
- Tamaño acotado: 500 mensajes por conversación (`CACHED_PER_CHANNEL`) y
  128 MiB para el archivo (`max_page_count`); al tope, SQLite rechaza
  escrituras en vez de crecer. La poda corre en reposo, no al arrancar.
- Las colas de SQLite se acotan a 1 024 trabajos, 16 respuestas y 8 MiB
  compartidos de datos retenidos. Las escrituras de runtime no esperan
  capacidad: el emisor conserva el lote rechazado y aplica contrapresión.
  Los tickets avanzan sólo al confirmar el trabajo. Un payload individual
  que excede el límite deshabilita la caché de la sesión con aviso visible de
  modo en memoria; los trabajos ya aceptados siguen confirmándose.
  Un fallo persistente no se anuncia como guardado ni borrado satisfactorio.
- **Ajustes → Conexión → Datos locales → Borrar** borra todo lo guardado de
  la cuenta (`Client::forget_local_data`, `Storage::forget_account`) en una
  transacción. Es una operación local, no una garantía de cumplimiento de
  todas las obligaciones del servicio.
- El esquema actual es **v6**: v3 añadió las lápidas persistentes, v4
  `read_marks`, v5 el sellado por cuenta y la tabla `found` (lo que encontró
  una búsqueda, aparte de la historia para que los límites de página nunca lo
  tomen por vecino) y v6 la tabla `draft_files` (dónde está cada archivo de
  un borrador y su tamaño, sellados como el texto del borrador). Las filas que v4 dejó en texto plano necesitan la clave
  de su cuenta, que sólo llega al desbloquearla: entonces se sellan todas en
  una transacción y se vacía el log WAL; mientras tanto no se leen. Si esa
  clave no llega (sin llavero), siguen en claro hasta que llegue o hasta
  «Borrar datos locales», que no la necesita. Sólo la simulación pudo guardar
  filas de cuenta antes de v5; su namespace más viejo («Simulación local»),
  si no es de la misma cuenta, se borra al abrir: ninguna clave lo sellaría. Una base de una
  versión más nueva se rechaza (`Error::Newer`) en vez de modificarse: así un
  RUNE viejo nunca escribe texto plano en una base sellada.

## Superficies de entrada

| Superficie | Medida |
| --- | --- |
| Enlaces de mensajes | limn solo convierte en enlace direcciones `http(s)` con host y sin credenciales; el host vuelve a exigir `http://`/`https://` antes de llamar a `xdg-open`/`open` |
| Markdown | Subconjunto acotado de limn: sin HTML, sin imágenes remotas; límite de 20 000 caracteres por fuente |
| Imágenes (futuro) | `limn-egui` decodifica fuera del hilo de UI, verifica SHA-256 y formato, y limita dimensiones (4096 px, 12 Mpx, 64 MiB) |
| Dispositivos de audio | Solo lectura local del nivel; nada se graba ni se envía |
| Mensajes de voz recibidos | Se descargan sólo al pulsar reproducir: orígenes validados, HTTPS sin credenciales ni redirecciones, hasta 8 MiB y tamaño exacto; sólo en memoria. Un lector Ogg propio y acotado (2026-10-18): CRC-32 obligatorio en cada página, paquetes de hasta 64 KiB y mapeo de canales 0 (mono o estéreo, al decodificador Opus de las llamadas) o 1 (hasta 8 canales); el 2, el 3 y el 255 se rechazan. El mapeo 1 llega al decodificador multistream de libopus (`opus_multistream_decoder_create` y `opus_multistream_decode_float`, la misma libopus por `opusic-sys`) desde código `unsafe` propio de RUNE: los conteos de streams y la tabla de canales vienen del archivo remoto; RUNE exige un byte de tabla por canal, libopus valida conteos y tabla (sin estado si no encajan) y la salida tiene lugar para 120 ms de cada canal, lo más que escribe un paquete. El final se recorta por el gránulo de la última página, contado desde el inicio del stream; uno de más de 4 h rechaza el archivo. Un archivo inválido falla con aviso y no corta nada más. El mismo lector lee los `.ogg` y `.opus` Opus de la carpeta de sonidos propia de la persona (hasta 1 MiB y 5 s) |
| Audio y video adjuntos (2026-10-17) | Como los mensajes de voz, hasta 50 MiB y sólo en memoria mientras su reproductor existe. Se leen en hilos propios, nunca en el de la interfaz: Symphonia (Rust puro) para el sonido, un lector MP4 propio que compara cada cantidad con sus bytes antes de reservar y cada muestra con el largo del archivo (432 000 muestras como mucho), y OpenH264 para imágenes de hasta 3840×2160, escaladas a 1920×1080. Un archivo inválido o en otro códec termina el reproductor con su motivo. WebM y MKV (2026-10-18): un lector Matroska propio recorre sólo la cabecera hasta el primer `Cluster`, cada tamaño contra sus bytes, y rechaza una pista de más de 3840×2160 según su cabecera antes de decodificar; después los plugins C de GStreamer corren **en el mismo proceso, sin sandbox**: `appsrc` sobre el archivo en memoria, `matroskademux` y un decodificador fijo por códec (`vp8dec`/`vp9dec` de libvpx, `dav1ddec` o `av1dec` de libaom, `h264parse ! openh264dec`), sin `decodebin` ni autodetección; una imagen que no se puede mostrar termina el reproductor. Riesgo que queda: un flujo cuyos cuadros digan un tamaño mayor que su cabecera llega al decodificador, que reserva según el flujo |
| GIF de tarjetas (`gifv`) | Sólo por el proxy de imágenes de Discord, como una vista previa (8 MiB); sus imágenes se decodifican con el mismo lector y OpenH264 en 3 s como mucho y se guardan como cuadros con los presupuestos de una animación |
| Texto de PDF para buscar (2026-10-17) | `pdftotext -bbox` del mismo Poppler, en el mismo bubblewrap/prlimit que el renderer; salida estándar (XHTML) leída hasta 24 MiB y 15 s, sólo su cuerpo (`<doc>`), palabras sin caracteres de control y con su caja recortada a la página; la rotación de cada página, de `pdfinfo` en el mismo sandbox (hasta 1 MiB y 10 s). Sólo en memoria con el visor abierto |
| Stickers Lottie (2026-10-18) | Sólo el JSON de una imagen de expresión cuya ruta termina en `.json` (`cdn.discordapp.com/stickers/{id}.json`), por el mismo cliente de imágenes sin credenciales, orígenes validados y hasta 1 MiB; ninguna otra imagen se lee como vectores. rasterlottie (Rust puro, sin funciones por defecto) en la tarea de decodificación. Antes, sobre su propio modelo y resolviendo ids como él: precomposiciones sin ciclos y de hasta 8 niveles (un ciclo desbordaría la pila, que no se atrapa), repetidores de hasta 64 copias, estrellas de hasta 64 puntas, ambos fijos, y a lo sumo 20 000 capas y formas por cuadro. Lienzo de hasta 1024 px, cuadros con los presupuestos de una animación (2 s, 120 cuadros, 16 MiB) |
| Pantalla y cámara | Solo por los portales ScreenCast/Camera con el diálogo del sistema; sin permiso no hay remote ni frames, y nunca se abre `/dev/video*`. Respuestas solo del dueño único fijado del portal, handles verificados, fd exigido como socket. Frames acotados (≤ 8192 px de origen, ≤ 1280×720 y ≤ 15 fps de vista previa, cola de 2), solo en memoria; el MJPEG de una cámara lo decodifica zune-jpeg (Rust puro) con lados de hasta 8192 px: nada se guarda, graba ni transmite. Se liberan al detener, al terminar la llamada, al cambiar de cuenta, al cerrar la ventana o cuando el sistema cierra la sesión |
| SQLite | Consultas parametrizadas (`rusqlite::params!`); los mensajes se guardan como JSON propio; campos nuevos con `#[serde(default)]` para leer filas viejas |
| Callback OAuth2 | Solo loopback IPv4 y puerto fijo, state aleatorio, PKCE S256, host/ruta/método/origen estrictos, límites de bytes y tiempo; sin reflejar códigos en HTML |
| HTTP OAuth2 | TLS, endpoints fijos, sin redirecciones, respuestas de hasta 1 MiB y timeout; errores estáticos sin cuerpos remotos ni credenciales |
| Página pública del cliente web (2026-10-19) | Sólo `GET https://discord.com/app` al arrancar el proveedor no oficial, sin credencial ni cabeceras de la API, sin redirecciones, 5 s y 1 MiB; de ella sólo se lee el número entre comillas tras `"BUILD_NUMBER":"` (1 a 9 dígitos, distinto de cero); si no, queda el de respaldo |
| Escritorio (P6) | Bandeja, tema y atajos por D-Bus, aceptando señales sólo del dueño del servicio; Hyprland por su socket de comandos (`$XDG_RUNTIME_DIR/hypr/…`, firma validada) con órdenes fijas; fuentes por `fc-list`/`fc-match` con el nombre escapado y archivos de hasta 32 MiB; la entrada de autostart cita el ejecutable según la especificación y rechaza rutas relativas o con controles. Ver [PLATFORM.md](PLATFORM.md) |
| Variables de entorno de GPU | `apps/desktop/src/gpu.rs` solo fija `VK_LOADER_DRIVERS_SELECT` con un patrón propio (`*radeon*`/`*intel*`) y respeta lo que ya eligió el entorno; no ejecuta nada que venga del entorno salvo a sí mismo (`current_exe`) para reintentar sin filtro |

## Referencia visual de Discord (tercera iteración)

Para que la interfaz se parezca a Discord en estructura, medidas y flujos, se
capturó el cliente oficial **solo como píxeles de pantalla**, a pedido del
dueño y en su propia cuenta:

- Todo dentro de un servidor de prueba propio («RUNE prueba»). No se abrieron
  mensajes directos, amigos ni otros servidores, y no se cambió ningún ajuste
  de la cuenta: en los ajustes de usuario solo se miró.
- En esa iteración no se inspeccionó el proceso, el DOM, las herramientas de desarrollo, los
  paquetes ni el almacenamiento local de Discord, y REA no se usó sobre él.
  Las capturas se tomaron con `grim` y la entrada con `dotool`.
- No se abrieron páginas con datos sensibles (dispositivos y sesiones,
  facturación, conexiones, apps autorizadas, revelar email o teléfono,
  contraseña, 2FA). Los datos de otras personas y los personales del perfil
  quedaron tapados en las capturas.
- Las capturas y las especificaciones viven **fuera del repositorio**
  (`~/projects/personal/rune-reference`), no se distribuyen, y en RUNE no hay
  archivos, estilos, íconos, sonidos ni textos largos de Discord: solo
  medidas, estructura y etiquetas cortas reimplementadas.

## Fuentes embebidas y sus avisos

RUNE conserva las licencias de Noto Sans Math 3.000 y de Ubuntu Regular,
Medium y Bold 0.83 en [assets/licenses](../assets/licenses/FONT-NOTICES.md),
con copyright, fuentes de distribución fijadas y SHA-256. Noto aporta
respaldo para alfabetos matemáticos sin descargar fuentes en ejecución;
su TTF se incorpora sin modificar bajo SIL OFL-1.1. Ubuntu conserva su
Ubuntu Font Licence 1.0. Los textos de licencia y sus avisos deben acompañar
la distribución: van en `THIRD_PARTY_NOTICES.md`, que cada artefacto lleva
(ver [Dependencias](#dependencias)). Alcance de la cobertura y pruebas
sintéticas en [FONTS.md](FONTS.md).

## Dependencias

Licencias de las dependencias Rust directas de la base inicial (permisivas):
`tokio` (MIT), `rusqlite` + SQLite incluido (MIT / dominio público), `serde` y
`serde_json` (MIT/Apache-2.0), `chrono` (MIT/Apache-2.0), `cpal` (Apache-2.0),
`eframe` (MIT/Apache-2.0), `image` (MIT/Apache-2.0). limn es MIT/Apache-2.0.
No se copió código de Serein (MIT/Apache-2.0, ver
[SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md)); solo ideas de arquitectura. Antes
de publicar el repositorio (2026-10-19) se compararon los dos árboles: los
bloques que seguían a Serein casi línea por línea se reescribieron (ver
[abajo](#revisión-antes-de-publicar-2026-10-19)).

Auditoría del árbol Rust anterior al login opcional (`cargo metadata`,
2026-10-14): las licencias admiten alternativas permisivas. Las que ofrecen
una opción copyleft son duales
(`r-efi`: MIT/Apache/LGPL; `self_cell`: Apache/GPL) y se usan bajo su
alternativa permisiva. Las fuentes que pinta egui/limn (`epaint_default_fonts`
y Ubuntu de `limn-egui`) son OFL-1.1 y Ubuntu Font License: al distribuir
binarios hay que incluir sus avisos.

Desde el 2026-10-19 esos avisos y los de cada dependencia Rust, en Linux y
en Windows, están en [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md),
generado con cargo-about (`about.toml`, `about.hbs`) junto con lo que no
viene de Cargo: las fuentes de `epaint_default_fonts` (Hack, Ubuntu Light,
Noto Emoji, emoji-icon-font) y de `limn-egui` (Ubuntu, Ubuntu Sans, Noto,
Bootstrap Icons), Twemoji (CC BY 4.0), la marca de GIPHY y la copia
modificada de `hpke-rs` (MPL-2.0, con su código en `vendor/hpke-rs`). El
programa lo lleva adentro (`rune --licenses`) y el workflow de release
corta si quedó viejo ([UPDATES.md](UPDATES.md#avisos-de-terceros)). Quedan
fuera las bibliotecas del sistema que el programa enlaza en ejecución
(GTK, WebKitGTK, GStreamer, PipeWire, ALSA en Linux): las pone la
distribución, no RUNE.

Desde el 2026-10-18, `cargo deny check` corre en la CI (ver
[abajo](#cargo-deny-en-la-ci-2026-10-18)).

La integración OAuth2 añade `reqwest`, `url`, `sha2`, `base64` y `getrandom`
(MIT/Apache-2.0). Las dependencias TLS incluyen `ring` (Apache-2.0/ISC),
`rustls` (Apache-2.0/ISC/MIT), `webpki` (ISC) y raíces bajo
CDLA-Permissive-2.0; hay que conservar sus avisos al distribuir. No se
incorpora el Social SDK ni código de Serein.

Auditoría 2026-10-14 con `cargo-audit 0.22.1`, instalado temporalmente por
Nix: **0 vulnerabilidades y 0 advertencias** para las 545 dependencias del
lock de la etapa OAuth2; también 0/0 para el lock previo del índice. Base RustSec con
1 314 avisos. JSON local en `/tmp/rune-oauth-audit-c7a6dk5l/`. Es una
comprobación de avisos conocidos, no una prueba de ausencia de defectos.

Con `session-login`, la auditoría posterior del mismo día encontró **577
entradas en Cargo.lock, 0 vulnerabilidades y 0 advertencias**, usando la
misma base RustSec local de 1 314 avisos sin actualizarla. Evidencias en
`/tmp/rune-session-audit-20261014-j3baoe_w/`. El inventario de metadata pasa
de 547 nodos predeterminados a 577 con todas las features; los 30 nodos
opcionales Rust tienen licencia MIT o MIT/Apache-2.0.

Esto **no** implica que todas las bibliotecas del ejecutable opcional sean
permisivas: GTK, GLib y libsoup incluyen LGPL; WebKit combina partes LGPL
y BSD. Sus bindings Rust y las bibliotecas nativas tienen licencias
distintas. Antes de distribuir este modo hay que preparar los avisos y
cumplir sus condiciones de enlace/distribución; el inventario no es esa
revisión completa. Cargo Audit tampoco analiza vulnerabilidades de las
bibliotecas nativas. Fuentes: [GTK](https://www.gtk.org/),
[WebKit](https://webkit.org/licensing-webkit/).

La corrección del login añadió GStreamer core y sus plugins base/good/bad
al entorno Nix opcional para exponer los elementos que WebKit necesita en
ejecución. No cambió `Cargo.lock` ni añadió dependencias Rust. La auditoría
Cargo anterior no evalúa esos plugins nativos ni todas sus dependencias;
su inventario y avisos deben formar parte de la revisión de distribución.

### Gateway y chat: auditoría actualizada (2026-10-14)

Después de añadir el transporte WebSocket y actualizar limn, `cargo-audit
0.22.1` revisó las **588 entradas de Cargo.lock**: **0 vulnerabilidades y
0 advertencias**. Esta vez se actualizó RustSec a 1 329 avisos, commit
`edb58f5974b83eb8302332821cb6d389777aa351`. Evidencia local:
`/tmp/rune-chat-audit-20261014.json`; inventario de licencias en
`/tmp/rune-chat-metadata-20261014.json`.

Las incorporaciones Rust del transporte usan MIT/Apache-2.0
(`tokio-tungstenite`, `tungstenite`, `futures-util`); las raíces TLS conservan
CDLA-Permissive-2.0. Se mantienen los límites anteriores de la auditoría:
no revisa bibliotecas nativas ni acredita compatibilidad con el servicio.
Las pruebas HTTP/WebSocket se ejecutaron en loopback con valores sintéticos,
sin acceder a la sesión real del dueño ni enviar mensajes a Discord.

### Imágenes y expresiones (2026-10-14)

Los recursos públicos se descargan con un cliente separado, sin credenciales,
cookies ni redirecciones. El proveedor declara los orígenes permitidos; el
adaptador de sesión declara los 12 de `provider::IMAGE_ORIGINS`:
`https://cdn.discordapp.com`, `https://media.discordapp.net`,
`https://images-ext-1.discordapp.net` y `https://images-ext-2.discordapp.net`;
los de la búsqueda de GIF (`https://static.klipy.com`,
`https://static1.klipy.com`, `https://static2.klipy.com`,
`https://media.tenor.com` y `https://c.tenor.com`), y desde 2026-10-18 los
de las portadas (`https://i.scdn.co`, `https://static-cdn.jtvnw.net` y
`https://i.ytimg.com`, abajo). La misma lista vale para imágenes,
descargas, documentos y clips, y cada fuente valida además su propio host
y ruta (`attachment_url`, las de medios). La única excepción es
`Provider::hosted_image` (2026-10-18, rama `edicion-de-perfil`): bytes que
el proveedor guardó él mismo en el equipo, nunca bajados de la red (hoy sólo
las imágenes de perfil que guarda la simulación); no pasan por la lista de
orígenes, `rune-core` descarta los de más de 8 MiB
(`ProfileImage::MAX_ANIMATED_BYTES`) y los decodifica con los mismos límites
de lienzo y cuadros que una descarga. Hay límites de
respuesta, dimensiones, asignación, concurrencia y caché; se descarta el
trabajo de generaciones anteriores al cerrar sesión. Los GIF del composer
son enlaces insertados por la persona, sin peticiones automáticas ni relay
de búsqueda. Detalle de límites y pruebas en [ARTWORK.md](ARTWORK.md).

Se reutilizan `reqwest`, `url`, `sha2` y `base64` ya presentes en el lock.
`image` y sus decoders PNG/WebP también estaban presentes por limn; ahora
`rune-core` los declara directamente para normalizar miniaturas fuera de la
UI. La iteración de adjuntos habilita además JPEG únicamente para previews,
con presupuestos separados del artwork; un spoiler no se carga hasta que la
persona lo revela. Los cuadros de GIF/WebP animados tienen sus propios límites
de lienzo, cuadros, tiempo y memoria ([FILES.md](FILES.md)).
Las descargas explícitas usan su propio cliente sin credenciales, archivos
parciales privados y publicación sin sobrescritura; ver [FILES.md](FILES.md).
Los previews de invitaciones usan únicamente GET público con respuesta acotada,
cancelación y `Retry-After`, sin aceptar membresías; ver [INVITATIONS.md](INVITATIONS.md).
Sus comentarios Cargo explican el uso. No se añaden SDKs de GIF ni
recursos de Serein. Las nuevas fuentes llevan los avisos enlazados en
[FONTS.md](FONTS.md); las auditorías anteriores conservan su alcance y fecha.
La comparación del lock de esta iteración con el inventario de Gateway/chat
conserva las 588 entradas y no añade versiones de paquetes. Cambia el commit
de integración de limn; no se presenta esa comparación como una nueva
auditoría de código.

#### Portadas de Spotify, Twitch y YouTube (2026-10-18)

Decisión del dueño (2026-10-18): las actividades muestran la portada que
nombra su clave, como el cliente oficial (REA de sus claves de imagen,
[REA.md](REA.md#cliente-web-oficial-portadas-nameplates-y-decoraciones-2026-10-18)).
El adaptador de sesión suma **sólo** estos orígenes exactos a los de
Discord y a los de la búsqueda de GIF:

| Clave | Dirección | Origen |
| --- | --- | --- |
| `spotify:{id}` | `https://i.scdn.co/image/{id}` | `https://i.scdn.co` |
| `twitch:{login}` | `https://static-cdn.jtvnw.net/previews-ttv/live_user_{login}-160x160.jpg` | `https://static-cdn.jtvnw.net` |
| `youtube:{id}` | `https://i.ytimg.com/vi/{id}/hqdefault_live.jpg` | `https://i.ytimg.com` |

- El identificador se acepta sólo con letras y dígitos ASCII (más `_` en
  Twitch y `_`/`-` en YouTube) y hasta 64 caracteres; cualquier otra
  forma, prefijo o servicio no se carga.
- Mismo cliente de imágenes que el resto: HTTPS sin puerto, sin
  credenciales, cookies ni redirecciones (una portada fuera de línea que
  redirige no se sigue), y los límites de un banner, más estrechos que los
  de un adjunto: JPEG/PNG/WebP/GIF, hasta 1 MiB, 2048 px por lado y 2 Mpx,
  un solo cuadro (no se decodifica animación), normalizada a 512×256
  (2026-10-18: al principio iban con los de un adjunto, 8 MiB y 4096 px).
- Nunca se lee ni se pide el token de Spotify de las conexiones ni otra
  API de esos servicios: sólo la imagen que la presencia ya nombra.
- Privacidad: como el cliente oficial, cargar una portada le muestra al
  servicio la IP del equipo y cuándo se miró esa tarjeta. Ocultar la
  actividad no está en el alcance; sólo se piden las portadas de la
  tarjeta o el perfil abiertos.
- Sólo fixtures; sin probar contra el servicio real.

### Voz y DAVE: parche de `hpke-rs` (2026-10-16)

`davey 0.1.5` (DAVE) llega a `hpke-rs 0.6.1` por `openmls_rust_crypto`, y esa
versión dependía de `libcrux-sha3 0.0.8` (RUSTSEC-2026-0207/0208). `hpke-rs
0.7.0` lo corrige, pero openmls exige `^0.6`. RUNE usa una copia en
[`vendor/hpke-rs`](../vendor/hpke-rs/RUNE-PATCH.md) vía `[patch.crates-io]`:
el archivo de crates.io con su checksum verificado (MPL-2.0, texto de la
licencia incluido), `libcrux-sha3` sustituido por `sha3` de RustCrypto para
el SHAKE256 de tamaño fijo y sin el backend opcional libcrux. Sin cambio de
algoritmo ni de formato; prueba con vectores independientes.

`cargo-audit 0.22.1` (RustSec de 1 329 avisos): el lock anterior tenía
**8 vulnerabilidades**, todas en crates `libcrux-*`; con el parche, **0
vulnerabilidades** en 703 entradas y una advertencia: `instant 0.1.13`
sin mantenimiento (RUSTSEC-2024-0384), que llega porque `davey` activa la
feature `js` de openmls en todas las plataformas
(`fluvio-wasm-timer` → `parking_lot 0.11` → `instant`). `cargo deny check
advisories` (0.19.9, sin `deny.toml`) solo marca esa misma advertencia. El
parche se retira cuando openmls acepte `hpke-rs >= 0.7`.

### `cargo deny` en la CI (2026-10-18)

[`deny.toml`](../deny.toml) revisa el grafo bloqueado con todas las
features y sin filtro de plataforma, y `.github/workflows/ci.yml` lo corre
en cada push a `main` y cada pull request con `EmbarkStudios/cargo-deny-action`
(las acciones van fijadas por commit, y clippy y los tests con `--locked`
compilan el mismo `Cargo.lock` que revisa). Con `cargo-deny 0.19.9`
y la base de RustSec del 2026-10-18 da `advisories ok, bans ok, licenses ok,
sources ok`, con una advertencia: `inout 0.2.3` fue retirado de crates.io
(llega por `secret-service` → `aes`/`cbc` → `cipher`); no es un aviso de
seguridad.

- **Licencias**: las que declaran las dependencias reales (MIT, Apache-2.0,
  BSD-2/3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0, CDLA-Permissive-2.0,
  Apache-2.0 con excepción LLVM). MPL-2.0, que obliga por archivo, se
  permite crate por crate, así una dependencia MPL nueva falla hasta que
  alguien la mire: `hpke-rs` (el parche de `vendor/hpke-rs`) y `symphonia`
  (audio adjunto), con sus subcrates. Las fuentes que embebe egui (`epaint_default_fonts`) tienen su excepción
  OFL-1.1 y Ubuntu Font License. Los crates de RUNE (`publish = false`) no
  se revisan.
- **Fuentes**: sólo crates.io y el repositorio de limn en
  `github.com/Niiihuel/limn`; `vendor/hpke-rs` es una ruta local.
- **Avisos ignorados, con motivo**: RUSTSEC-2024-0384 (`instant` sin
  mantenimiento, por la feature `js` de openmls que activa `davey`;
  bloqueado aguas arriba) y RUSTSEC-2026-0370/0371/0372 (`zvariant`,
  `zbus_names` y `zvariant_derive` sin mantenimiento porque se fusionaron en
  zbus 6, sin vulnerabilidad; accesskit, por eframe, y `secret-service`
  todavía piden zbus 5). Se quitan de `deny.toml` cuando se resuelvan.

### Revisión antes de publicar (2026-10-19)

Antes de hacer público el repositorio se buscó, con un script propio, cada
tramo de 5 o más líneas significativas seguidas que el árbol de RUNE
comparte con `references/serein` y `references/serein-current` (sin
`target/`, `vendor/` ni `crates/rune-login`, que se revisa aparte): una vez
con las líneas normalizadas (espacios y `,`/`;` finales) y otra con además
los identificadores reemplazados, para ver lo copiado con otros nombres.
Lo que seguía a Serein casi línea por línea se reescribió con otra
estructura y el mismo comportamiento: las páginas de las cuentas
conectadas, la lectura de enlaces de invitación, el tipo MIME de una
subida, el cambio de un GIF favorito, el orden de las carpetas de
servidores, la URL de reanudación del Gateway, el servidor MLS de las
pruebas de voz y los casos de prueba del proxy. Lo que queda parecido es
lo que impone una API o un formato y no admite otra forma: literales de
`egui::RawInput` y de la configuración de `sonora`, la lista de variantes
de formato de texto, la llamada de `openmls` que firma una propuesta
externa, las cabeceras de ThumbHash (el formato de Evan Wallace, MIT;
acreditado en `THIRD_PARTY_NOTICES.md`) y los umbrales del tiempo relativo
(los de moment.js). `vendor/hpke-rs` coincide porque los dos parten del
mismo crate publicado.

También se cambiaron por datos inventados los restos de una cuenta real
que quedaban en fixtures (el ID y el nombre de un emoji de servidor, una
biografía y un apodo).

## Actualizaciones (2026-10-19)

RUNE se actualiza solo desde las GitHub Releases de `Niiihuel/rune`
([UPDATES.md](UPDATES.md)). Cada release lleva un manifiesto firmado con
Ed25519; la clave pública va compilada (`crates/rune-update/release-key.pub`)
y la privada vive sólo en la máquina del dueño y en el secreto
`RUNE_UPDATE_SIGNING_KEY` de GitHub Actions, que el workflow usa únicamente
en el paso de firma, con el programa ya compilado. La firma se verifica
antes de leer el manifiesto y el artefacto se instala sólo si su tamaño y
su SHA-256 coinciden; nunca se baja de versión. `RUNE_UPDATE_KEY` (otra
clave, para el feed de prueba) sólo existe en compilaciones de depuración.
Detalle y límites en [UPDATES.md](UPDATES.md#seguridad).
