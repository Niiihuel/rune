# Seguridad y privacidad de RUNE

Este documento reúne las autorizaciones del dueño, las reglas que no cambian
con ellas y cómo RUNE protege lo que guarda y lo que recibe. Las decisiones
llevan su fecha porque son reglas, no historia.

## Excepción de alcance autorizada el 2026-10-14

El dueño pidió un proveedor de sesión personal **no oficial** y una WebView
exclusivamente temporal para el login. La excepción sustituye la prohibición
anterior de manejar esa sesión dentro de la aplicación. No habilita recibir
credenciales en el chat, importar otros perfiles, copiar evasiones de Serein
ni inspeccionar sesiones del cliente oficial. Flujo en [DISCORD.md](DISCORD.md). Autorizar el experimento personal
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
cuenta. Ver [DISCORD.md](DISCORD.md).

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
- **Paridad con Serein.** Se usan los componentes de bots (botones, menús,
  modales) y los comandos de aplicación como en Serein. El socket local
  `discord-ipc` y la detección de juegos existen apagados por defecto, cada
  uno con su interruptor (abajo).
- Sigue prohibido todo lo de [Reglas](#reglas).

### Actividad local: Rich Presence y juegos

- **Apagados por defecto**, cada uno con su interruptor en Ajustes ›
  Sistema › Actividad. Apagado, el socket no existe y no se mira ningún
  proceso.
- **Socket `discord-ipc-N`**: sólo en `$XDG_RUNTIME_DIR` si es privado de
  la persona (en Windows, una tubería de la sesión), 0600 y verificado con
  `SO_PEERCRED`. Sin WebSocket ni puerto TCP (Serein abre
  `127.0.0.1:6463–6472`; RUNE no). Sólo saludo, `SET_ACTIVITY` acotado e
  `INVITE_BROWSER`, que abre el diálogo de invitación para que la persona
  decida; todo lo demás se rechaza. Ningún programa recibe credenciales ni
  mensajes.
- **Juegos**: de los procesos del usuario se lee sólo la ruta del ejecutable
  y el comienzo del primer argumento, contra la lista pública de Discord
  pedida sin credencial.

## Identidad de navegador autorizada el 2026-10-19

El dueño quitó la prohibición de presentar el proveedor no oficial como el
cliente web de Discord en un navegador, como hace Serein: user agent,
`X-Super-Properties`, huella y propiedades del Identify, con una sola
identidad coherente en REST, Gateway y la WebView donde la persona resuelve
el hCaptcha. Reimplementado, sin copiar ([DISCORD.md](DISCORD.md)).

- La identidad es la del motor que de verdad resuelve el desafío (WebKitGTK
  en Linux, WebView2 en Windows), la misma en REST, Gateway, voz y las
  ventanas de `rune-login`.
- **Nada de la cuenta en la identidad:** la credencial va sólo en
  `Authorization` (y en el `token` de Identify y Resume); las propiedades
  llevan ids aleatorios por proceso, idioma y zona horaria. Las pruebas lo
  comprueban sobre lo que sale.
- `launch_signature`, que firma el WebAssembly del cliente web, va en
  `null`: no se imita. Sin motor (por ejemplo, sin WebView2 Runtime) no sale
  ningún pedido.
- **Riesgo:** presentarse como navegador no asegura que Discord acepte una
  invitación ni que no pida el teléfono o restrinja la cuenta.

## Pulsar para hablar global

Autorizado por el dueño el 2026-10-26 (AGENTS.md).

### Windows: gancho de bajo nivel

- **Qué ve.** `WH_KEYBOARD_LL` recibe cada tecla que se pulsa o suelta en la
  sesión, con su código virtual, y `WH_MOUSE_LL` cada botón medio o lateral
  del ratón. Sólo se mira lo que coincide con el atajo ligado.
- **Qué hace.** Compara con el atajo ligado. Guarda un indicador por atajo y
  los modificadores pulsados, ambos de esos mismos eventos (izquierda y
  derecha cuentan igual). Siempre pasa el evento al siguiente gancho
  (`CallNextHookEx`): **nunca consume entrada**.
- **Qué no hace.** No guarda, no almacena en búfer, no envía ni registra
  teclas ni botones, y no cuenta pulsaciones. El registro sólo dice que los
  ganchos se instalaron o no, con el código de error de Windows.
- **Entrada inyectada.** Lo que envían otros programas (`SendInput`) cuenta
  igual, como para cualquier gancho.
- **Elevación.** Windows puede no entregar a los ganchos la entrada de una
  ventana con más privilegios (UIPI). Entonces el atajo no funciona fuera de
  RUNE salvo que RUNE también se ejecute como administrador. No está
  comprobado en hardware; Ajustes lo indica en la fila de Pulsar para hablar.
- **Hilo y vida.** Un hilo propio con su bucle de mensajes. Los ganchos se
  quitan al cerrar la sesión del atajo y al cerrar RUNE.
- **Límites conocidos.** Las teclas se comparan con los códigos del teclado
  de EE. UU. (los signos pueden no coincidir en otra disposición). Los
  botones del ratón no se pueden grabar todavía: limn no entrega el botón
  central ni suelta los laterales (un límite para subir a limn antes de
  usarlos). Por eso hoy ningún atajo es un botón del ratón.

### Linux: portal y Hyprland

- Cualquier tecla puede ser el atajo global; el escritorio la pide por el
  portal `GlobalShortcuts`.
- Hyprland no asigna la tecla por sí mismo. Ajustes muestra la línea para
  pegar en su configuración, con botón «Copiar»: la forma `hl.bind` si existe
  `~/.config/hypr/hyprland.lua` (sólo se comprueba que exista) o la clásica
  `bind = …, global, rune:push-to-talk` en otro caso. No se escribe ningún
  archivo del compositor.
- El registro tiene una línea por estado (sesión, cuántos atajos asignó el
  escritorio, motivo de fallo), sin teclas.

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
     payloads, comportamiento e interfaz. Las fuentes están en
     [DISCORD.md](DISCORD.md). Se reimplementa: no se copia código,
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
   [DISCORD.md](DISCORD.md).
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
| Diagnóstico de voz y del Gateway, y lo demás que RUNE escribe en stderr (errores, pánicos) | stderr, una línea por paso (`rune: voice …`, `rune: gateway: …`); también `$XDG_STATE_HOME/rune/rune.log` (o `~/.local/state/rune/rune.log`), rotado a 5 MB con tres archivos, sólo en ejecuciones interactivas (`apps/desktop/src/log.rs`; en Windows, ver abajo) | Directorio `0700`, archivos `0600`. Voz y Gateway: sólo opcodes, códigos de cierre, modos de cifrado, conteos y el host del servidor de voz; nunca token, `session_id`, claves, mensajes MLS, ids de personas ni la IP propia (test `diagnostics_follow_each_step_and_never_show_secrets`). El resto: errores con rutas locales y motivos de red, nombres de las aplicaciones que suenan al transmitir la pantalla, una cabecera por arranque y el texto de los pánicos, que puede citar un mensaje o nombre llegado de la red. Ninguna línea lleva credenciales ni respuestas de hCaptcha: las ventanas de login y hCaptcha y el navegador que abre un enlace mandan su stderr a `/dev/null`. El archivo sobrevive a cerrar sesión y a «Borrar datos locales»: se borra a mano |
| Identidad y servidores obtenidos con OAuth2 | Sólo memoria | Sin caché ni restauración (sólo ajustes); se borran del estado al salir o fallar |

### Windows

Las mismas filas viven en el perfil del
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

Las ventanas de login y hCaptcha son procesos propios con un perfil
WebView2 InPrivate en una carpeta nueva de `%TEMP%\RUNE-webview`, que se
borra al terminar; nunca otro perfil de Edge o WebView2 ni datos del
cliente oficial. No se promete borrado forense.

### Caché cifrada por cuenta

Decisión del dueño del 2026-10-18 (AGENTS.md). Cada cuenta tiene su clave
(32 bytes al azar) en el llavero, nunca en disco. Con ella se sellan valor
por valor, con XChaCha20-Poly1305, los mensajes, lo encontrado por una
búsqueda, los borradores y sus archivos, la instantánea y las marcas de
lectura; cada valor lleva como dato asociado su lugar (cuenta,
conversación, fila), así copiado a otra fila u otra cuenta no abre. Las
imágenes en disco van selladas con la misma clave y nombradas con un HMAC
de su URL.

Se eligió AEAD por columna y no SQLCipher: `chacha20poly1305` ya estaba en
el árbol, no cambia el SQLite incluido y permite una clave por cuenta, que
deja ilegible lo de una cuenta olvidada aunque quede una copia vieja del
archivo. Queda en claro lo que ubica las filas (ids de cuenta,
conversaciones y mensajes), los ajustes, y de las imágenes cuántas hay, su
fecha y su tamaño.

Sin llavero (o si no responde) los datos de la cuenta quedan sólo en
memoria, con aviso, nunca en texto plano. La caché exige una identidad
estable del proveedor y la cuenta, nunca el nombre visible; el proveedor
OAuth2 no la usa.

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

## Superficies de entrada

- **Enlaces**: sólo `http(s)` con host y sin credenciales; los dominios
  ajenos al proveedor preguntan antes de abrirse.
- **Markdown**: subconjunto acotado de limn, sin HTML ni imágenes remotas.
- **Imágenes, audio y video remotos**: un cliente aparte sin credenciales,
  cookies ni redirecciones, sólo a los orígenes HTTPS exactos que declara el
  proveedor (CDN de Discord, KLIPY y Tenor para GIF, y las portadas de
  Spotify, Twitch y YouTube por decisión del dueño del 2026-10-18).
  Formato verificado por contenido, tamaños, dimensiones, cuadros y
  concurrencia acotados; decodificación fuera del hilo de UI. Un spoiler no
  se pide hasta que la persona lo revela. Nunca se lee el token de Spotify
  de las conexiones.
- **WebM y MKV**: los plugins C de GStreamer corren en el mismo proceso, sin
  sandbox, tras un lector Matroska propio que acota la cabecera; es el
  riesgo de parser más alto que queda.
- **PDF**: Poppler corre en `bubblewrap --unshare-all` con `prlimit` (sin
  red ni hogar, 512 MiB, 5 s de CPU); sin ese aislamiento no hay vista
  previa. Sólo en Linux con Nix.
- **Pantalla y cámara**: sólo por los portales con el diálogo del sistema
  (en Windows, sus APIs); nunca `/dev/video*`. Cuadros acotados y sólo en
  memoria.
- **Escritorio**: señales D-Bus sólo del dueño de cada servicio; órdenes
  fijas por el socket de Hyprland.
- **SQLite**: consultas parametrizadas; campos nuevos con
  `#[serde(default)]`.
- **OAuth2**: callback sólo en loopback IPv4 y puerto fijo, `state`
  aleatorio, PKCE S256, sin reflejar códigos.

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
  quedó viejo ([UPDATES.md](UPDATES.md#avisos-de-terceros)). Lo demás que
  RUNE trae, abajo.
- Quedan fuera las bibliotecas del sistema que el programa enlaza en
  ejecución (GTK, WebKitGTK, GStreamer, PipeWire, ALSA en Linux): las pone
  la distribución. GTK, GLib y libsoup son LGPL y WebKit LGPL/BSD; quien
  distribuya RUNE con ellas tiene que cumplir sus condiciones. Cargo no
  audita esas bibliotecas nativas.

### Procedencia de lo que RUNE trae

- **Íconos y marca**: propios. El set de `assets/glyphs` sigue las métricas
  del sistema de íconos de Discord (estudiado por REA), pero cada forma se
  construye con primitivas y coordenadas propias en `tools/rune-icons`; no
  se copió ningún trazo.
- **Emoji a color**: Twemoji 17.0.3 sin modificar, CC BY 4.0
  ([TWEMOJI.md](../assets/licenses/TWEMOJI.md)).
- **Fuentes**: Ubuntu y Noto, archivos oficiales sin modificar
  ([FONT-NOTICES.md](../assets/licenses/FONT-NOTICES.md)).
- **Sonidos predeterminados**: efectos públicos de Discord, autorizados el
  2026-10-21 ([SOURCES.md](../assets/sounds/discord/SOURCES.md),
  [AUDIO-NOTICES.md](../assets/licenses/AUDIO-NOTICES.md)).
- **Muestras de la simulación** (`assets/*-sample.*`, fixtures de
  `rune-media`): tonos y patrones generados con `ffmpeg`, sin voz ni
  grabación de nadie.
- **H264**: OpenH264 se compila desde la fuente; la licencia de patentes de
  Cisco no cubre esta compilación ([ARCHITECTURE.md](ARCHITECTURE.md#medios-rune-media)).

### Criptografía de DAVE parcheada

`davey 0.1.5` (DAVE) y el backend RustCrypto de openmls 0.8 que usa
(`openmls_rust_crypto 0.5.1`, `openmls_basic_credential 0.5.0`,
`hpke-rs-rust-crypto 0.6`) dependen de `p256 0.13`, que llega por
`elliptic-curve 0.13` a `crypto-bigint 0.5.5` (RUSTSEC-2026-0387,
operaciones que debían ser de tiempo constante; corregido sólo desde 0.7.6).
Ninguna versión publicada lo arregla todavía. RUNE usa copias vía
`[patch.crates-io]`, cada una el archivo de crates.io con su checksum
verificado, su licencia y un `RUNE-PATCH.md` con el diff:

- [`vendor/davey`](../vendor/davey/RUNE-PATCH.md),
  [`vendor/openmls_rust_crypto`](../vendor/openmls_rust_crypto/RUNE-PATCH.md)
  y [`vendor/openmls_basic_credential`](../vendor/openmls_basic_credential/RUNE-PATCH.md)
  (MIT): pasan a `p256 0.14`, `hpke-rs 0.8` y `rand_core 0.10` con las mismas
  llamadas que ya usa openmls `main`; openmls sigue en 0.8 de crates.io.
- [`vendor/hpke-rs`](../vendor/hpke-rs/RUNE-PATCH.md) 0.8.0 (MPL-2.0):
  `libcrux-sha3` sustituido por `sha3` de RustCrypto para el SHAKE256 de
  tamaño fijo y sin el backend libcrux, como la copia anterior de 0.6.1;
  así no vuelve `proc-macro-error2` (sin mantenimiento, RUSTSEC-2026-0173).

Sin cambio de algoritmo ni de formato: firmas ECDSA P-256 en DER, claves
SEC1 sin comprimir y HPKE de RFC 9180, comprobado contra las versiones de
crates.io en las dos direcciones y con los vectores de hpke-rs. Las copias
se retiran cuando davey publique una versión sobre `p256 >= 0.14`.

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
  las copias de `vendor/` son rutas locales.
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
