# Seguridad y privacidad de RUNE

Este documento reúne las autorizaciones del dueño, las reglas que no cambian
con ellas y cómo RUNE protege lo que guarda y lo que recibe. Las decisiones
llevan su fecha porque son reglas, no historia.

## Excepción de alcance autorizada el 2026-10-14

El dueño pidió un proveedor de sesión personal **no oficial** y una WebView
exclusivamente temporal para el login. La excepción sustituye la prohibición
anterior de manejar esa sesión dentro de la aplicación. No habilita recibir
credenciales en el chat, importar otros perfiles, copiar evasiones de Serein
ni inspeccionar sesiones del cliente oficial. El dueño confirmó manualmente
el login y la identidad; alcance, retención y riesgos en
[DISCORD_SESSION.md](DISCORD.md). Autorizar el experimento personal
no implica aprobación de Discord.

## Verificación de invitaciones autorizada el 2026-10-16

El dueño reactivó la aceptación nativa de invitaciones en el proveedor no
oficial y autorizó mostrar en una WebView aislada el hCaptcha que Discord
pida, para que lo resuelva la persona:

- La persona pulsa «Verificar»; nada se abre ni se envía solo. La ventana
  (`rune-login`, proceso propio como el login) carga una página de RUNE en
  `https://captcha.rune.invalid/`, con CSP que sólo admite
  `hcaptcha.com`/`*.hcaptcha.com`, navegación limitada a esa página y a
  hCaptcha, sesión efímera, sin herramientas de desarrollo, descargas,
  ventanas nuevas ni permisos. No se presenta como `discord.com`; lleva el
  user agent de la [identidad de navegador](#identidad-de-navegador-autorizada-el-2026-10-19).
- El ayudante recibe sólo el `sitekey`, el `rqdata` y si es invisible; el
  `rqtoken` y el `captcha_session_id` quedan en el adaptador. La respuesta
  vuelve una vez por la tubería privada, ligada a una capacidad aleatoria,
  con tamaño acotado, y se borra de la memoria propia al descartarse.
- El desafío queda ligado a cuenta, invitación y solicitud, vive cinco
  minutos y dentro del plazo de la orden. Con una respuesta, el adaptador
  repite esa misma aceptación una sola vez con `X-Captcha-Key` y, si
  vinieron, `X-Captcha-Rqtoken` y `X-Captcha-Session-Id`. Otro desafío en
  ese reintento ya no se ofrece. Cerrar la ventana, vencer el plazo, perder
  la conexión o cambiar de cuenta no envía nada.
- Sin resolución automática ni servicios de terceros. Otros servicios de
  desafío o la verificación de cuenta remiten a la aplicación oficial.

Discord puede seguir pidiendo verificación por teléfono o restringir la
cuenta. Ver [INVITATIONS.md](DISCORD.md).

## Ampliaciones del 2026-10-17

- **Verificación en cualquier escritura.** La misma ventana y las mismas
  condiciones valen para cualquier escritura del proveedor no oficial en
  que Discord pida hCaptcha (solicitudes de amistad, mensajes y demás):
  - Sólo escrituras: una lectura con desafío no se ofrece. Sólo hCaptcha
    válido y acotado (`rejections::challenge`); otro servicio o una
    verificación de cuenta remiten a la aplicación oficial.
  - La escritura espera y la interfaz pregunta («Verificar» / «Cancelar»).
    Una sola ventana a la vez, contando la de las invitaciones; un desafío
    más mientras otro espera falla con motivo.
  - Atado a la conexión, la cuenta y esa escritura, cinco minutos; perder la
    conexión, cambiar de cuenta, cerrar la ventana, cancelar o vencer el
    plazo terminan la acción sin enviar nada.
  - La respuesta va una sola vez, sólo en el pedido de la misma ruta que se
    frenó, se borra de memoria al usarse o descartarse y nunca sale en
    registros, eventos ni errores (`Debug` redactado). Un segundo desafío
    en el reintento termina la acción con motivo. El reintento presenta la
    misma identidad que el intento que repite.
  Detalle en [DISCORD_SESSION.md](DISCORD.md).
- **Paridad con Serein.** Se usan los componentes de bots (botones, menús,
  modales) y los comandos de aplicación como en Serein. El socket local
  `discord-ipc` y la detección de juegos existen apagados por defecto, cada
  uno con su interruptor (abajo).
- Sigue prohibido todo lo de [Reglas](#reglas).

### Actividad local: Rich Presence y juegos

Detalle de uso en [PLATFORM.md](PLATFORM.md#actividad-de-este-equipo).

- **Apagados por defecto**, cada uno con su interruptor y su explicación en
  Ajustes › Sistema › Actividad. Apagado, el socket no existe y no se mira
  ningún proceso.
- **Socket `discord-ipc-N`**: sólo en `$XDG_RUNTIME_DIR`, y sólo si es una
  carpeta del usuario que nadie más puede abrir (nunca `/tmp` ni otra
  compartida); el socket es 0600 y cada conexión se verifica con
  `SO_PEERCRED` como del mismo usuario. Sin WebSocket ni puerto TCP (Serein
  abre `127.0.0.1:6463–6472`; RUNE no). Un socket de otra ejecución sólo se
  reemplaza si es de este usuario y nadie responde.
- Sólo el protocolo local: saludo con `v: 1` y un `client_id` numérico en
  10 s, tramas de hasta 16 KiB que terminan en 5 s una vez empezadas, hasta
  8 programas, una pausa de 100 ms tras cada orden. `SET_ACTIVITY` exige
  `nonce` y `pid`; texto acotado (128 caracteres, 512 las imágenes) y sin
  caracteres de control; botones, secretos, grupos y `name` se descartan;
  `type` 1 (transmitir) se rechaza. `INVITE_BROWSER` valida el código y abre
  el diálogo de invitación de RUNE (vista previa y confirmación de la
  persona); nunca une a nada por sí solo. `AUTHORIZE`, `SUBSCRIBE` y todo
  lo demás se rechazan con el código 4000: ningún programa recibe
  credenciales, mensajes ni datos de la cuenta más allá de su id y nombre
  en `READY`.
- Publicar es del proveedor: la sesión no oficial nombra la aplicación con
  `GET /applications/{id}/rpc`, resuelve claves de imágenes con
  `GET /oauth2/applications/{id}/assets` y direcciones `https` con
  `POST /applications/{id}/external-assets` (como Serein), con caché, y
  manda la actividad en el op 3 junto al estado; invisible no publica nada.
  Una aplicación que Discord no nombra no se publica.
- **Juegos**: cada 10 s, de los procesos del usuario, sólo la ruta de
  `/proc/N/exe` y los primeros 512 bytes del primer argumento de
  `/proc/N/cmdline`; nunca el resto de la línea de comandos, su memoria ni
  sus archivos. Hasta 4096 procesos por vuelta. La lista de juegos es la
  pública de Discord (`/applications/detectable`), pedida **sin** la
  credencial, acotada a 24 MiB, leída fuera del runtime y sólo en memoria.
- **Instancia única**: el mismo tipo de socket (`rune-instance.sock`, mismo
  usuario); llegan a lo sumo ocho enlaces de una línea, que se abren como
  uno cliqueado en RUNE (los ajenos preguntan antes).

## Identidad de navegador autorizada el 2026-10-19

El dueño quitó la prohibición de presentar el proveedor no oficial como el
cliente web de Discord en un navegador, como hace Serein: user agent,
`X-Super-Properties`, huella y propiedades del Identify, con una sola
identidad coherente en REST, Gateway y la WebView donde la persona resuelve
el hCaptcha. Reimplementado, sin copiar. Valores exactos en
[DISCORD_SESSION.md](DISCORD.md).

- **Una sola identidad** (`rune-discord-session`, módulo `browser`): el
  cliente web en WebKit sobre Linux. La presentan todos los pedidos a la API
  (también los públicos, sin la credencial), el Gateway y la voz (apretón e
  Identify), las subidas al almacenamiento (sólo el user agent) y las dos
  ventanas de `rune-login`, login y hCaptcha.
- **WebKit, no Chrome.** Las ventanas son WebKitGTK: el cliente web se
  clasifica como Safari por `navigator.vendor`, y hCaptcha ve el motor
  WebKit diga lo que diga el user agent. RUNE dice el motor que de verdad
  resuelve el desafío.
- **Qué se manda:** user agent, `X-Super-Properties`, `X-Discord-Locale`,
  `X-Discord-Timezone`, `Accept-Language`, `Referer` y, en las escrituras,
  `Origin`; la aceptación de invitaciones suma `X-Context-Properties` como
  el modal de invitación del cliente web. `X-Fingerprint` no: el cliente
  web lo borra al iniciar sesión.
- **Número de build:** se lee al arrancar de la página pública
  `https://discord.com/app` (`window.GLOBAL_ENV.BUILD_NUMBER`), sin
  credencial, con 5 s y 1 MiB de tope; si falla, el último leído por REA.
- **Nada de la cuenta en la identidad:** la credencial va sólo en
  `Authorization` (y en el `token` de Identify y Resume). Las propiedades
  llevan dos ids aleatorios por proceso, el idioma y la zona horaria del
  sistema; las pruebas lo comprueban sobre lo que sale.
- `launch_signature`, que firma el WebAssembly del cliente web, va en
  `null` como antes de que ese módulo cargue: no se imita.
- **Windows:** las ventanas son WebView2 y la identidad es la de ese motor:
  un auxiliar oculto lee, en una página vacía propia de RUNE, lo que el
  cliente web usaría (user agent, marcas de client hints, idioma y zona), y
  REST, el Gateway, la voz y las ventanas presentan eso mismo, con las
  client hints de Chromium. Lo leído se valida (texto visible y acotado,
  sin comillas en las marcas) antes de ir a una cabecera. Sin WebView2
  Runtime no sale ningún pedido y RUNE da el enlace oficial de Microsoft.
- **Riesgo:** presentarse como navegador no asegura que Discord acepte una
  invitación ni que no pida el teléfono o restrinja la cuenta.

## Reglas

1. **Credenciales fuera del agente y del almacenamiento general.** No
   recibir contraseñas, tokens o MFA en chat, logs, capturas ni
   configuración. El proveedor no oficial recibe únicamente la sesión
   generada dentro de su propia ventana temporal, con interacción de la
   persona. Sin importación desde otros navegadores o el cliente oficial, ni
   entrada manual de tokens.
2. **Inspección estática de artefactos (2026-10-15)**, para entender
   módulos y flujos y guiar la reimplementación nativa. No incluye
   perfiles, credenciales, memoria de sesiones, evasión de controles ni
   importación de código o recursos propietarios, ni inventar endpoints.
   - **Rutas de Serein (2026-10-16):** se pueden usar en el proveedor no
     oficial las rutas y mensajes de Gateway no documentados que Serein usa
     (perfil completo, lista de miembros por suscripción), con el mismo
     ritmo, cola y clasificación de rechazos que el resto del adaptador, y
     recordar varias cuentas como Serein.
   - **REA del cliente oficial (2026-10-16):** se puede usar REA sobre el
     paquete instalado de Discord y los bundles JavaScript públicos de
     discord.com como fuente principal: rutas REST, mensajes de Gateway,
     payloads, comportamiento e interfaz. Cada hallazgo se registra en
     [REA.md](DISCORD.md) con su evidencia. Se reimplementa: no se copia código,
     estilos, íconos, sonidos ni otros recursos. Excluido: leer el perfil,
     la caché, el almacenamiento local o la memoria del cliente oficial, y
     ejecutarlo instrumentado con la cuenta sin una autorización aparte.
   - **Sonidos de Discord (2026-10-21):** excepción a lo anterior: RUNE
     empaqueta como sonidos predeterminados los efectos de sonido públicos
     de Discord, bajados sin autenticación del bundle público
     (`assets/sounds/discord`, procedencia en su `SOURCES.md`). No abarca
     código, otros recursos, perfiles, cachés, credenciales ni datos de
     sesión ([AGENTS.md](../AGENTS.md)).
   - **Siempre prohibido:** automatizar, resolver o reenviar CAPTCHA o MFA
     (los resuelve la persona en la ventana de RUNE, con un único
     reintento), cualquier evasión antiabuso, leer sesiones o datos locales
     del cliente oficial y acceder a cuentas ajenas. La identidad del
     cliente dejó de estar prohibida el 2026-10-19 (arriba).
3. **OAuth2 con consentimiento:** el proveedor oficial conserva el access
   token limitado a `identify guilds` sólo en memoria, obtenido con código y
   PKCE, nunca pegándolo en la interfaz. No se guarda en SQLite, logs ni
   configuración; no se usa refresh token. No hay proveedor de bots. Ver
   [DISCORD_OAUTH.md](DISCORD.md).
4. **Etiquetado honesto**: cada proveedor declara su tipo y capacidades; la
   UI muestra «Simulación local · sin conexión real» en el panel del
   usuario y en Ajustes → Conexión, y deshabilita con explicación lo que no
   se puede hacer.
5. **Sólo acciones iniciadas por la persona**: RUNE no envía nada por su
   cuenta. Las respuestas y la actividad «ambiental» existen sólo dentro
   del proveedor simulado.
6. **Sin código ni recursos ajenos.** No se copia código de Serein; antes de
   publicar el repositorio (2026-10-19) se buscaron con un script los tramos
   de 5 o más líneas compartidos con `references/serein*` (normalizados y
   con identificadores reemplazados) y se reescribió lo que seguía a Serein
   casi línea por línea. Lo que queda parecido lo impone una API o un
   formato (literales de `egui::RawInput` y de `sonora`, la llamada de
   `openmls` que firma una propuesta externa, las cabeceras de ThumbHash,
   acreditado, y los umbrales de tiempo relativo de moment.js). Aunque
   Serein es `MIT OR Apache-2.0`, sus recursos no se usan: sus sonidos son
   de Discord y su `discord-shortcodes.tsv` sale del cliente oficial; lo
   que RUNE trae lleva su propia licencia en
   [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md). Los
   fixtures usan datos inventados, nunca de una cuenta real.
7. **Referencia visual de Discord**: se capturó el cliente oficial sólo
   como píxeles de pantalla, en la cuenta del dueño, dentro de un servidor
   de prueba propio, sin abrir privados, amigos, otros servidores ni páginas
   con datos sensibles, y tapando datos de otras personas. Las capturas
   viven **fuera del repositorio** (`~/projects/personal/rune-reference`);
   en RUNE sólo hay medidas, estructura y etiquetas cortas reimplementadas.

## Datos locales

| Qué | Dónde | Protección |
| --- | --- | --- |
| Últimos 500 mensajes por conversación y cuenta, los que encontró una búsqueda en todo el historial (aparte, otros 500 por conversación), borradores (con la ruta y el tamaño de sus archivos, nunca su contenido), marcas locales de lectura y recordatorios de no leído, una instantánea de servidores/canales/personas para arrancar sin esperar la red; con la simulación y con el proveedor no oficial | `$XDG_DATA_HOME/rune/rune.db` (o `~/.local/share/rune/rune.db`) | Directorio `0700`, archivo `0600` en Unix. **Cifrado por cuenta** (XChaCha20-Poly1305, clave en el llavero). Sin llavero, nada de la cuenta llega al archivo: queda en memoria |
| Imágenes descargadas: avatares, emoji, stickers, íconos e insignias («arte») y vistas previas de adjuntos y tarjetas, banners e imágenes a tamaño original («medios»), **también las de conversaciones privadas** | `$XDG_CACHE_HOME/rune/images/<digest>/{media,artwork}/` (o `~/.cache/rune/…`); la carpeta de la cuenta, con un SHA-256 sin clave de su nombre en la caché; cada archivo, con un HMAC-SHA-256 de la URL bajo una clave derivada de la de la cuenta | Directorios `0700`, archivos `0600` (escritos aparte con un nombre al azar y movidos enteros). **Sellados con la clave de la cuenta** (dato asociado: cuenta, parte y nombre). Sin llavero no se escribe nada; tampoco en una sesión que la base no verificó como de la cuenta. Acotadas por parte (1 GiB de medios y 128 MiB de arte por defecto) y por retención (90 días sin mostrarse); «Borrar» en Datos y privacidad › Caché de imágenes |
| Clave de los datos locales de cada cuenta | Llavero del sistema (Secret Service en Linux), servicio `rune-cache`, una entrada por cuenta | 32 bytes al azar; nunca en el archivo, logs ni configuración. Se borra al cerrar sesión u olvidar la cuenta; cambiar de cuenta la conserva |
| Identidad, navegación y mensajes del proveedor no oficial | Memoria; lo que se guarda, en la caché cifrada una vez verificada la cuenta | Cierre local no equivale a revocación remota |
| Ajustes con cualquier proveedor: los del equipo y, por cuenta, último servidor y canal, favoritos y privados fijados, niveles de notificación, canales ocultos, categorías plegadas y temporizadores de estado | Tabla `settings` de `rune.db`; los de la cuenta bajo `account:{proveedor}:{id de la cuenta}:` | Sólo ids y valores de la interfaz: nunca credenciales, mensajes, nombres ni datos de perfil. «Borrar datos locales» no los toca |
| Credencial del proveedor no oficial | Memoria; con la autorización del dueño (2026-10-16), también el llavero del sistema para restaurar la sesión, una entrada por cuenta recordada | Nunca SQLite, archivos, logs ni configuración; cerrar sesión borra la de esa cuenta; cambiar de cuenta la conserva; «Olvidar» borra la elegida; sin llavero queda sólo en memoria |
| Lista de cuentas recordadas del proveedor no oficial | Atributos (no secretos) de esas mismas entradas: `account=<id>`, `name=<nombre visible>` y `avatar=<hash>` (público, nunca una URL); la fecha de la entrada indica la última usada | Ningún otro dato personal (sin correo ni teléfono); se borra con la entrada |
| Diagnóstico de voz y del Gateway, y lo demás que RUNE escribe en stderr (errores, pánicos) | stderr, una línea por paso (`rune: voice …`, `rune: gateway: …`); también `$XDG_STATE_HOME/rune/rune.log` (o `~/.local/state/rune/rune.log`), rotado a 5 MB con tres archivos, sólo en ejecuciones interactivas en Unix (`apps/desktop/src/log.rs`) | Directorio `0700`, archivos `0600`. Voz y Gateway: sólo opcodes, códigos de cierre, modos de cifrado, conteos y el host del servidor de voz; nunca token, `session_id`, claves, mensajes MLS, ids de personas ni la IP propia (test `diagnostics_follow_each_step_and_never_show_secrets`). El resto: errores con rutas locales y motivos de red, nombres de las aplicaciones que suenan al transmitir la pantalla, una cabecera por arranque y el texto de los pánicos, que puede citar un mensaje o nombre llegado de la red. Ninguna línea lleva credenciales ni respuestas de hCaptcha: las ventanas de login y hCaptcha y el navegador que abre un enlace mandan su stderr a `/dev/null`. El archivo sobrevive a cerrar sesión y a «Borrar datos locales»: se borra a mano |
| Identidad y servidores obtenidos con OAuth2 | Sólo memoria | Sin caché ni restauración (sólo ajustes); se borran del estado al salir o fallar |

### Windows

Sin probar en un Windows real. Las mismas filas viven en el perfil del
usuario, que sólo él (y los administradores) puede leer, en vez de modos
`0700`/`0600`: `%LOCALAPPDATA%\RUNE\rune.db`,
`%LOCALAPPDATA%\RUNE\cache\images\…`, `%LOCALAPPDATA%\RUNE\logs\rune.log`
y los archivos de antes de la ventana en `%APPDATA%\RUNE`; lo copiado al
portapapeles espera en `%LOCALAPPDATA%\RUNE\copied`. El llavero es el
Administrador de credenciales: una credencial genérica
`RUNE/{servicio}/{cuenta}` por entrada, sólo en este equipo (no viaja con
un perfil móvil); el nombre y el avatar son atributos legibles, como en
Secret Service, y el secreto se borra de la memoria que devuelve el sistema
antes de liberarla.

Las ventanas de login y hCaptcha (y la lectura del motor) son cada una un
proceso propio con un perfil WebView2 InPrivate en una carpeta nueva
`%TEMP%\RUNE-webview\<al azar>`, que se borra cuando ese navegador termina
(se lo espera hasta 5 s); la que deja un auxiliar caído la borra el
siguiente, pasados 20 minutos. Nunca otro perfil de Edge o WebView2 ni
datos del cliente oficial; la sesión vuelve sólo por la tubería privada.
Que InPrivate no deje la sesión en esa carpeta lo dice Microsoft; por eso
la carpeta se borra igual. No se promete borrado forense.

### Caché cifrada por cuenta

Decisión del dueño del 2026-10-18 (AGENTS.md). Cada cuenta tiene su clave:
32 bytes de `getrandom`, en el llavero (`rune-cache`), nunca en disco. Con
ella se sellan, valor por valor, con XChaCha20-Poly1305 y un nonce al azar
de 24 bytes, los cuerpos de los mensajes (historia y búsqueda), los
borradores y la ruta y el tamaño de sus archivos, la instantánea y las
marcas de lectura. Cada valor lleva como dato asociado su lugar (qué es, la
cuenta, la conversación, la fila): copiado a otra fila u otra cuenta no
abre. Un valor dañado se lee como ausente en la historia, los borradores y
la instantánea; en la búsqueda local y en las marcas de lectura es un
error explícito. Si el llavero ya no tiene la clave de una cuenta o no la
lee, RUNE hace una nueva y, al desbloquear con ella, borra lo sellado con
la perdida, que ya no abriría nunca.

Se eligió AEAD por columna y no SQLCipher porque es lo más chico que cubre
todo: `chacha20poly1305` ya estaba en el árbol (voz), no cambia el SQLite
incluido ni agrega OpenSSL, y permite una clave por cuenta, que hace
ilegible lo de una cuenta olvidada aunque quede una copia vieja del
archivo. El costo: quedan en claro lo que ubica las filas (el nombre de la
cuenta en la caché, que incluye su id; ids de conversaciones y mensajes;
las lápidas) y los ajustes, que no tienen contenido y se leen antes de
saber qué cuenta hay.

Sin llavero (o si no responde o no se desbloquea) los datos de la cuenta
quedan sólo en memoria, con aviso, y nunca en texto plano; un llavero que
falla no hace crear una clave nueva, que dejaría ilegible lo guardado. Las
operaciones del llavero corren de a una y en orden: una clave no se crea
dos veces para la misma cuenta, ni queda si la cuenta se olvidó mientras se
pedía.

La caché exige una identidad estable del proveedor y la cuenta, nunca el
nombre visible. La simulación la declara desde el inicio
(`Provider::storage_identity`); el proveedor no oficial, cuando verificó la
cuenta (`Event::StorageIdentity`), y vale hasta que la sesión termina. Un
`Ready` de otra cuenta deshabilita la caché. El proveedor OAuth2 no la usa:
con él el archivo guarda sólo ajustes.

### Imágenes en disco

Como en Serein, las imágenes descargadas se guardan entre ejecuciones,
incluidas las de conversaciones privadas. A diferencia de Serein (que las
guarda en claro), cada archivo es la imagen tal como llegó, sellada con la
clave de la cuenta: sin la clave no se lee, y movida a otro nombre o a otra
cuenta tampoco abre; un archivo que no abre se borra al leerlo. El disco
recién se abre cuando la cuenta tiene su clave y el host dio los límites,
así nunca se poda con otros. Olvidar la cuenta o cerrar sesión con el
proveedor no oficial borra su carpeta, y una descarga que termina después
no escribe nada. El disco sigue a la base cifrada: si llega el Ready de
otra cuenta o la sesión deja de guardarse por el límite de la cola, lo que
se baja desde ese momento no toca el disco.

Lo que queda a la vista sin la clave: cuántos archivos hay, su fecha
(última vez que se mostró) y su tamaño, que es el de la imagen más 41
bytes; con eso, quien tenga la carpeta y una imagen pública conocida puede
sospechar que se miró, pero no confirmarlo por el nombre. La carpeta por
cuenta es un SHA-256 sin clave del nombre de la cuenta en la caché, porque
olvidar una cuenta tiene que encontrarla sin su clave: quien adivine un id
puede confirmar que esa carpeta es de esa cuenta. Las URL firmadas de
adjuntos cambian al renovarse: esa imagen se vuelve a bajar y la vieja se
va por retención o presupuesto.

### Olvidar y borrar

- Cerrar sesión con el proveedor no oficial, u «Olvidar» una cuenta
  recordada (también la abierta, que sigue en memoria), manda
  `Event::ForgetStorage`: el núcleo borra en una transacción todas sus filas
  (con `secure_delete` y vaciando el log WAL) y su entrada `rune-cache` del
  llavero, aunque la base no haya podido abrirse. Si el llavero no deja
  borrar la clave, los datos ya se borraron y la interfaz lo avisa.
- Cambiar de cuenta no borra nada, ni dejar de recordar la sesión abierta
  (`Command::ForgetSession`). La simulación no borra su caché al cerrar
  sesión; «Borrar datos locales» sí. Si Discord termina una sesión
  recordada, el proveedor deja de recordarla pero su caché sigue sellada
  hasta que esa cuenta entre y cierre sesión. Los ajustes por cuenta (sólo
  ids) no se borran al olvidarla.
- **Ajustes → Conexión → Datos locales → Borrar** borra todo lo guardado de
  la cuenta (`Client::forget_local_data`, `Storage::forget_account`) en una
  transacción, sin necesitar la clave. Es una operación local, no una
  garantía de cumplimiento de las obligaciones del servicio.

### Límites y esquema

- `read_marks` guarda por cuenta y canal el cursor local (`through`), el
  ancla manual de no leído (`unread_from`) y su revisión local; no acredita
  confirmación remota. Sobreviven a la poda y al borrado de mensajes; la
  restauración se limita a 65 536 filas; ver
  [ARCHITECTURE.md](ARCHITECTURE.md).
- La instantánea no guarda presencias ni estados de voz: son efímeros y
  dicen dónde está alguien.
- `secure_delete` sobrescribe con ceros lo borrado; `auto_vacuum`
  incremental devuelve el espacio en la poda.
- 500 mensajes por conversación (`CACHED_PER_CHANNEL`) y 128 MiB para el
  archivo (`max_page_count`); al tope, SQLite rechaza escrituras en vez de
  crecer. La poda corre en reposo, no al arrancar.
- Las colas de SQLite se acotan a 1 024 trabajos, 16 respuestas y 8 MiB
  compartidos. Las escrituras no esperan capacidad: el emisor conserva el
  lote rechazado y aplica contrapresión; los tickets avanzan sólo al
  confirmar. Un payload que excede el límite deshabilita la caché de la
  sesión con aviso de modo en memoria. Un fallo no se anuncia como guardado
  ni borrado.
- Esquema **v6**: v3 añadió las lápidas, v4 `read_marks`, v5 el sellado por
  cuenta y la tabla `found` (lo encontrado por una búsqueda, aparte de la
  historia) y v6 `draft_files`. Las filas que v4 dejó en texto plano se
  sellan en una transacción al desbloquear la clave de su cuenta; mientras
  tanto no se leen. Una base de una versión más nueva se rechaza
  (`Error::Newer`): un RUNE viejo nunca escribe texto plano en una base
  sellada.

## Superficies de entrada

| Superficie | Medida |
| --- | --- |
| Enlaces de mensajes | limn sólo convierte en enlace direcciones `http(s)` con host y sin credenciales; el host vuelve a exigir `http://`/`https://` antes de llamar a `xdg-open`/`open` |
| Markdown | Subconjunto acotado de limn: sin HTML, sin imágenes remotas; límite de 20 000 caracteres por fuente |
| Imágenes | Decodificadas fuera del hilo de UI, con formato verificado y dimensiones, asignación, concurrencia y caché acotadas (ver [Imágenes remotas](#imágenes-remotas)) |
| Dispositivos de audio | Sólo lectura local del nivel; nada se graba ni se envía |
| Mensajes de voz recibidos | Se descargan sólo al pulsar reproducir: orígenes validados, HTTPS sin credenciales ni redirecciones, hasta 8 MiB y tamaño exacto; sólo en memoria. Lector Ogg propio y acotado: CRC-32 obligatorio en cada página, paquetes de hasta 64 KiB y mapeo de canales 0 (mono o estéreo) o 1 (hasta 8 canales); el 2, el 3 y el 255 se rechazan. El mapeo 1 llega al decodificador multistream de libopus desde código `unsafe` propio: los conteos y la tabla de canales vienen del archivo remoto; RUNE exige un byte de tabla por canal, libopus valida conteos y tabla, y la salida tiene lugar para 120 ms de cada canal. El final se recorta por el gránulo de la última página; uno de más de 4 h rechaza el archivo. El mismo lector lee los `.ogg` y `.opus` de la carpeta de sonidos propia (hasta 1 MiB y 5 s) |
| Audio y video adjuntos | Hasta 50 MiB y sólo en memoria mientras su reproductor existe, en hilos propios. Symphonia (Rust puro) para el sonido, un lector MP4 propio que compara cada cantidad con sus bytes antes de reservar (432 000 muestras como mucho), y OpenH264 para imágenes de hasta 3840×2160, escaladas a 1920×1080. WebM y MKV: un lector Matroska propio recorre la cabecera hasta el primer `Cluster` y rechaza una pista de más de 3840×2160; después los plugins C de GStreamer corren **en el mismo proceso, sin sandbox**: `appsrc`, `matroskademux` y un decodificador fijo por códec (`vp8dec`/`vp9dec`, `dav1ddec` o `av1dec`, `h264parse ! openh264dec`), sin `decodebin`. Riesgo que queda: un flujo cuyos cuadros digan un tamaño mayor que su cabecera llega al decodificador, que reserva según el flujo |
| GIF de tarjetas (`gifv`) | Sólo por el proxy de imágenes de Discord, como una vista previa (8 MiB); decodificados con el mismo lector y OpenH264 en 3 s como mucho, con los presupuestos de una animación |
| Texto de PDF para buscar | `pdftotext -bbox` de Poppler, en el mismo bubblewrap/prlimit que el renderer; salida leída hasta 24 MiB y 15 s, sólo su cuerpo, palabras sin caracteres de control y cajas recortadas a la página; la rotación, de `pdfinfo` en el mismo sandbox (1 MiB, 10 s). Sólo en memoria con el visor abierto |
| Stickers Lottie | Sólo el JSON de una expresión cuya ruta termina en `.json`, por el cliente de imágenes, hasta 1 MiB. rasterlottie (Rust puro) en la tarea de decodificación; antes, sobre su modelo: precomposiciones sin ciclos y de hasta 8 niveles, repetidores de hasta 64 copias, estrellas de hasta 64 puntas y a lo sumo 20 000 capas y formas por cuadro. Lienzo de hasta 1024 px, presupuestos de una animación (2 s, 120 cuadros, 16 MiB) |
| Pantalla y cámara | Sólo por los portales ScreenCast/Camera con el diálogo del sistema; nunca se abre `/dev/video*`. Respuestas sólo del dueño único fijado del portal, handles verificados, fd exigido como socket. Frames acotados (≤ 8192 px de origen, ≤ 1280×720 y ≤ 15 fps de vista previa, cola de 2), sólo en memoria; el MJPEG lo decodifica zune-jpeg (Rust puro). Se liberan al detener, al terminar la llamada, al cambiar de cuenta, al cerrar la ventana o cuando el sistema cierra la sesión |
| SQLite | Consultas parametrizadas (`rusqlite::params!`); mensajes como JSON propio; campos nuevos con `#[serde(default)]` |
| Callback OAuth2 | Sólo loopback IPv4 y puerto fijo, state aleatorio, PKCE S256, host/ruta/método/origen estrictos, límites de bytes y tiempo; sin reflejar códigos en HTML |
| HTTP OAuth2 | TLS, endpoints fijos, sin redirecciones, respuestas de hasta 1 MiB y timeout; errores estáticos sin cuerpos remotos ni credenciales |
| Página pública del cliente web | Sólo `GET https://discord.com/app` al arrancar el proveedor no oficial, sin credencial ni redirecciones, 5 s y 1 MiB; sólo se lee el número tras `"BUILD_NUMBER":"` (1 a 9 dígitos, distinto de cero) |
| Escritorio | Bandeja, tema y atajos por D-Bus, aceptando señales sólo del dueño del servicio; Hyprland por su socket de comandos (firma validada) con órdenes fijas; fuentes por `fc-list`/`fc-match` con el nombre escapado y archivos de hasta 32 MiB; la entrada de autostart cita el ejecutable y rechaza rutas relativas o con controles. Ver [PLATFORM.md](PLATFORM.md) |
| Variables de entorno de GPU | `apps/desktop/src/gpu.rs` sólo fija `VK_LOADER_DRIVERS_SELECT` con un patrón propio y respeta lo que ya eligió el entorno; sólo se re-ejecuta a sí mismo (`current_exe`) para reintentar sin filtro |

### Imágenes remotas

Los recursos públicos se descargan con un cliente separado, sin
credenciales, cookies ni redirecciones. El proveedor declara los orígenes
permitidos; el adaptador de sesión declara los de `provider::IMAGE_ORIGINS`:
`https://cdn.discordapp.com`, `https://media.discordapp.net`,
`https://images-ext-1.discordapp.net`, `https://images-ext-2.discordapp.net`;
los de la búsqueda de GIF (`https://static.klipy.com`,
`https://static1.klipy.com`, `https://static2.klipy.com`,
`https://media.tenor.com`, `https://c.tenor.com`) y los de las portadas
(abajo). La misma lista vale para imágenes, descargas, documentos y clips,
y cada fuente valida además su host y ruta. La única excepción es
`Provider::hosted_image`: bytes que el proveedor guardó él mismo en el
equipo (hoy sólo las imágenes de perfil de la simulación), hasta 8 MiB y
con los mismos límites de lienzo y cuadros. Un spoiler no se carga hasta
que la persona lo revela. Las descargas explícitas usan su propio cliente
sin credenciales, archivos parciales privados y publicación sin
sobrescritura ([FILES.md](FILES.md)); los límites de miniaturas y
expresiones están en [ARTWORK.md](ARTWORK.md). Los previews de invitaciones
usan sólo GET público acotado ([INVITATIONS.md](DISCORD.md)).

#### Portadas de Spotify, Twitch y YouTube

Decisión del dueño (2026-10-18): las actividades muestran la portada que
nombra su clave, como el cliente oficial ([REA.md](DISCORD.md)). El adaptador
suma **sólo** estos orígenes:

| Clave | Dirección | Origen |
| --- | --- | --- |
| `spotify:{id}` | `https://i.scdn.co/image/{id}` | `https://i.scdn.co` |
| `twitch:{login}` | `https://static-cdn.jtvnw.net/previews-ttv/live_user_{login}-160x160.jpg` | `https://static-cdn.jtvnw.net` |
| `youtube:{id}` | `https://i.ytimg.com/vi/{id}/hqdefault_live.jpg` | `https://i.ytimg.com` |

- El identificador se acepta sólo con letras y dígitos ASCII (más `_` en
  Twitch y `_`/`-` en YouTube) y hasta 64 caracteres.
- Mismo cliente de imágenes, con los límites de un banner: JPEG/PNG/WebP/GIF,
  hasta 1 MiB, 2048 px por lado y 2 Mpx, un solo cuadro, normalizada a
  512×256. Una portada que redirige no se sigue.
- Nunca se lee el token de Spotify de las conexiones ni otra API de esos
  servicios.
- Privacidad: como en el cliente oficial, cargar una portada le muestra al
  servicio la IP del equipo y cuándo se miró; sólo se piden las de la
  tarjeta o el perfil abiertos.

## Dependencias y licencias

- Las dependencias Rust son permisivas (MIT, Apache-2.0, BSD, ISC, Zlib,
  BSL-1.0, Unicode-3.0, CDLA-Permissive-2.0); las duales con opción
  copyleft se usan bajo la permisiva. limn es MIT/Apache-2.0. Cada
  dependencia nueva lleva en su `Cargo.toml` un comentario con el porqué.
- [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), generado con
  cargo-about (`about.toml`, `about.hbs`), reúne los avisos de cada
  dependencia Rust en Linux y Windows y lo que no viene de Cargo: las
  fuentes de `epaint_default_fonts` y `limn-egui`, Twemoji (CC BY 4.0), la
  marca de GIPHY y la copia modificada de `hpke-rs` (MPL-2.0). El programa
  lo lleva adentro (`rune --licenses`) y el workflow de release corta si
  quedó viejo ([UPDATES.md](UPDATES.md)). Las fuentes embebidas y sus
  licencias están en [FONTS.md](FONTS.md) y
  [assets/licenses](../assets/licenses/FONT-NOTICES.md); la procedencia de
  los sonidos predeterminados, en
  [AUDIO-NOTICES.md](../assets/licenses/AUDIO-NOTICES.md).
- Quedan fuera las bibliotecas del sistema que el programa enlaza en
  ejecución (GTK, WebKitGTK, GStreamer, PipeWire, ALSA en Linux): las pone
  la distribución. GTK, GLib y libsoup son LGPL y WebKit LGPL/BSD; quien
  distribuya RUNE con ellas tiene que cumplir sus condiciones. Cargo no
  audita esas bibliotecas nativas.

### `hpke-rs` parcheado

`davey` (DAVE) llega a `hpke-rs 0.6.1` por `openmls_rust_crypto`, que
dependía de `libcrux-sha3 0.0.8` (RUSTSEC-2026-0207/0208). `hpke-rs 0.7.0`
lo corrige, pero openmls exige `^0.6`. RUNE usa una copia en
[`vendor/hpke-rs`](../vendor/hpke-rs/RUNE-PATCH.md) vía
`[patch.crates-io]`: el archivo de crates.io con su checksum verificado
(MPL-2.0, licencia incluida), con `libcrux-sha3` sustituido por `sha3` de
RustCrypto para el SHAKE256 de tamaño fijo y sin el backend libcrux. Sin
cambio de algoritmo ni de formato; probado con vectores independientes. El
parche se retira cuando openmls acepte `hpke-rs >= 0.7`.

### `cargo deny` en la CI

[`deny.toml`](../deny.toml) revisa el grafo bloqueado con todas las
features y sin filtro de plataforma; la CI lo corre en cada push a `main` y
cada pull request (acciones fijadas por commit; clippy y tests con
`--locked` compilan el mismo `Cargo.lock` que revisa).

- **Licencias**: las de arriba. MPL-2.0, que obliga por archivo, se permite
  crate por crate (`hpke-rs` y `symphonia` con sus subcrates), así una
  dependencia MPL nueva falla hasta que alguien la mire. Las fuentes de
  egui tienen su excepción OFL-1.1 y Ubuntu Font License. Los crates de
  RUNE (`publish = false`) no se revisan.
- **Fuentes**: sólo crates.io y `github.com/Niiihuel/limn`;
  `vendor/hpke-rs` es una ruta local.
- **Avisos ignorados, con motivo**: RUSTSEC-2024-0384 (`instant` sin
  mantenimiento, por la feature `js` de openmls que activa `davey`;
  bloqueado aguas arriba) y RUSTSEC-2026-0370/0371/0372 (`zvariant`,
  `zbus_names` y `zvariant_derive`, fusionados en zbus 6, sin
  vulnerabilidad; accesskit y `secret-service` todavía piden zbus 5). Se
  quitan de `deny.toml` cuando se resuelvan.

## Actualizaciones

RUNE se actualiza sólo desde las GitHub Releases de `Niiihuel/rune`. Cada
release lleva un manifiesto firmado con Ed25519; la clave pública va
compilada (`crates/rune-update/release-key.pub`) y la privada vive sólo en
la máquina del dueño y en el secreto `RUNE_UPDATE_SIGNING_KEY` de GitHub
Actions, que el workflow usa únicamente en el paso de firma. La firma se
verifica antes de leer el manifiesto y el artefacto se instala sólo si su
tamaño y su SHA-256 coinciden; nunca se baja de versión.
`RUNE_UPDATE_KEY` (feed de prueba) sólo existe en compilaciones de
depuración. Detalle en [UPDATES.md](UPDATES.md#seguridad).
