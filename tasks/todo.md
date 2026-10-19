# Tareas de continuidad de RUNE

## Entrega completada del 2026-10-14

- [x] Leer arquitectura, seguridad, workflow y estado previo; conservar fuentes.
- [x] Repartir auditoría de UI, login/invitaciones y proveedor/multimedia.
- [x] Ejecutar tests base: 636 aprobadas, 6 ignoradas, 23 suites.
- [x] Revisar invitación pegada desde un diálogo nativo, con validación del proveedor.
  - Aceptación: inválida permanece en el diálogo; válida abre preview; cerrar,
    cambiar de cuenta y cancelar no aplican respuestas obsoletas; aceptación pausada
    entonces (reactivada el 2026-10-16, ver recorrido 02).
  - Verificar: regresiones de rune-ui/invitations y pruebas del host.
- [x] Combinar filtros locales de búsqueda sin modificar borradores.
  - Aceptación: propios, archivos y fijados se combinan con texto o sin texto;
    las ediciones/borrados actualizan resultados y el salto usa el ID vigente.
  - Verificar: regresiones de rune-ui/search, teclado y tamaño mínimo.
- [x] Hacer explícito el alcance local/remoto de marcar leído.
  - Aceptación: el proveedor declara soporte y el consumidor respeta ese contrato;
    no se promete sincronización remota sin soporte.
  - Verificar: pruebas del contrato, estado y consumidor afectado.
- [x] Corregir el indicador de presencia de privados sin soporte del proveedor.
  - Aceptación: no muestra desconectado ni estado personalizado no recibido;
    conserva indicadores cuando el proveedor declara presencia y contador de grupos.
  - Verificar: regresión de layout con presencia de prueba y soporte deshabilitado.
- [x] Integrar inventario con evidencia por familia, prioridad y dependencias.
- [x] Revisar cambios cruzados y pasar fmt, workspace tests y Clippy.
- [x] Registrar resultados, capturas pertinentes y límites de pruebas reales.

Resultado anterior: 651 pruebas aprobadas, seis ignoradas; formato, Clippy y release
correctos. Capturas nativas con simulación revisadas. Evidencia en
[native-workflows-2026-10-14.json](../docs/evidence/native-workflows-2026-10-14.json).
Esta lista completa sólo esa entrega. La revisión documental del 2026-10-15
contrasta sus cuatro incrementos con el código y conserva su evidencia; no
registra una nueva ejecución de las 651 pruebas.

## Recorridos pendientes (2026-10-15)

Los 21 identificadores y el orden del inventario provienen del canvas
`RUNE-discord-gap-analysis.canvas.tsx`. Los módulos nuevos de esta lista son
propuestas hasta que exista su implementación revisada. Las dependencias
determinan el orden de ejecución; la numeración identifica el recorrido.
El 22 (`video`) se sumó el 2026-10-16, fuera del canvas.
El [plan](plan.md#continuidad-desde-el-análisis-de-discord-2026-10-15) define
la integración y la distinción entre evidencia local, simulación e integración
real. La ampliación estática produjo 14 dossiers REA/Ghidra de plataforma y
23 descompilaciones focalizadas de voz suplementarias. Su cobertura total
sigue parcial: ver [registro](../docs/REA.md#discord-instalado-paquete-y-módulos-nativos-2026-10-15)
y [ledger](../docs/evidence/discord-native-2026-10-15.json). No habilita
aceptación de invitaciones de producción.

### 01. `storage` — Persistencia por identidad y coherencia

- [x] Completar el aislamiento y la reconciliación de almacenamiento local.
- Estado: implementación local actualizada a SQLite v3, con identidad opt-in
  de proveedor/cuenta, reconciliación de rangos, lápidas persistentes,
  respuestas por época/petición y colas no bloqueantes con tickets. Cierre y
  borrado esperan confirmación; borradores/ajustes conservan cambios rechazados.
  La migración elimina presencia/voz previamente guardadas y las escrituras
  nuevas las excluyen. Checkpoint integral: **716 aprobadas, seis ignoradas**,
  23 suites; formato, Clippy con `-D warnings` y release correctos. REA verificó
  cierre con borrador persistido y reapertura desde caché con ocho segundos de
  latencia simulada. Ver [registro](../docs/evidence/implementation-checkpoint-2026-10-15.json)
  y [alcance nativo](../docs/REA.md#checkpoint-local-de-storage-e-invitaciones-2026-10-15).
  Desde el 2026-10-18 la caché está cifrada por cuenta, con la clave en el
  llavero, y la usa también el proveedor no oficial (ver abajo y
  [SECURITY.md](../docs/SECURITY.md#datos-locales)); OAuth2 sigue sin ella.
- Módulos: extender `rune-core::storage`, `rune-storage` y configuración del
  host; introducir identidad estable y contexto de lectura en crates existentes.
- Dependencias: identidad confirmada y contrato de rangos del proveedor.
- Cierre verificable: dos cuentas no comparten mensajes ni borradores;
  respuestas de sesiones anteriores se descartan; páginas persistidas borran
  lo ausente de su rango sin afectar otros; cola llena no bloquea UI ni pierde
  borradores/borrados. Cambios de esquema llevan migración y regresión.
- [x] 2026-10-18 (rama `almacenamiento-cifrado`, decisión del dueño en
  AGENTS.md): caché local cifrada por cuenta, también para la cuenta real.
  - SQLite v5: cada valor de una cuenta (mensajes, borradores, instantánea,
    marcas, lo encontrado) sellado con XChaCha20-Poly1305 y su lugar como
    dato asociado; una clave al azar por cuenta en el llavero (`rune-cache`),
    nunca en disco; sin llavero, sólo memoria. Las filas en texto plano de v4
    se sellan al desbloquear su cuenta; una base más nueva se rechaza. AEAD
    por columna y no SQLCipher: por qué, y qué queda en claro, en
    [SECURITY.md](../docs/SECURITY.md#datos-locales).
  - `Event::StorageIdentity`: el proveedor no oficial nombra la cuenta al
    verificarla; vale hasta el fin de la sesión y un `Ready` de otra cuenta
    sigue apagando la caché.
  - `Event::ForgetStorage`: cerrar sesión u «Olvidar» borra filas y clave;
    cambiar de cuenta no.
  - Lo que encuentra la búsqueda en todo el historial se guarda aparte
    (tabla `found`): la búsqueda local lo lee, las páginas no lo toman por
    vecino, lápidas y ediciones lo alcanzan.
  - Arranque desde la instantánea con la cuenta real: al restaurar una
    sesión recordada no se manda la instantánea pelada de la verificación;
    `Update::Restored` trae instantánea, borradores y marcas, y el host la
    muestra como caché (sin confirmar) hasta el READY.
  - Correcciones de la revisión (rama `almacenamiento-cifrado-fix`):
    «Borrar datos locales» funciona sin clave (sin llavero no trababa más
    los ajustes ni el cierre); una clave nueva borra lo sellado con la
    perdida, así la búsqueda local y las marcas no fallan en cada arranque;
    olvidar una cuenta borra su clave aunque la base no abra; las operaciones
    del llavero van de a una y en orden; lo que restaura un desbloqueo se
    paga al encolarlo (las marcas vuelven siempre, o su error); una página
    autoritativa borra también lo encontrado ausente de su rango; el
    namespace viejo de otra cuenta se borra en vez de quedar en claro; cambiar
    de proveedor no espera al llavero en el hilo de la interfaz
    (`Client::wait_for_key` sólo al arrancar).
  - En la mezcla: cerrar sesión mientras se verifica una cuenta recordada,
    que la olvida, también borra sus datos locales (`Event::ForgetStorage`).
  - Pendiente (baja): los borradores en memoria no se vacían al cambiar de
    cuenta, así que uno de la cuenta anterior puede tapar el restaurado de la
    nueva en un canal compartido; hace falta saber de qué cuenta es cada uno.
  - Sólo fixtures (núcleo, almacenamiento, proveedor y host), sin probar
    contra Discord ni contra un Secret Service real. Pendiente, diferido como
    optimización: medir y ajustar el arranque desde la caché.

### 02. `invites` — Invitaciones con aceptación nativa y verificación

- [x] Completar tarjetas de enlace completo, caducidad y reintento tras cooldown.
- [x] Reactivar la aceptación nativa con el hCaptcha resuelto por la persona
  (decisión del dueño del 2026-10-16).
- [x] Tarjetas también con texto alrededor, una por invitación y como mucho
  diez por mensaje (tope del cliente oficial por REA), fuera de código y
  spoilers (rama `invitaciones`, 2026-10-18).
- [x] Invitaciones de grupo (tipo 1), de amistad (tipo 2) y de ingreso como
  invitado (bandera 1): vista previa, tarjeta, modal y aceptación por el
  mismo `POST /invites/{code}`, cada una con su capacidad
  (`group_invitations`, `friend_invitations`, `guest_invitations`), en la
  sesión y en la simulación. Sólo fixtures, **sin probar contra el servicio
  real** ([detalle](../docs/INVITATIONS.md#todas-las-clases-y-tarjetas-en-texto-mezclado-2026-10-18)).
- [x] Motivo del rechazo (rama `invitaciones-motivo`, 2026-10-19): líneas
  `rune: invitation:` con el estado HTTP y el código entero de error (nada
  del cuerpo, token, invitación ni cuenta); causas con nombre por código
  verificado con REA (30001, 40007, 30019, 40069, 50095, edad, 4000xx,
  40068); ya ser miembro es `AlreadyJoined` y ofrece abrir lo que dio;
  «Volver a intentar» sólo tras `Unavailable` o `RateLimited`; «Abrir en
  el navegador» siempre con el proveedor no oficial y tras cualquier
  rechazo. Simulación: `simulado.invalid/invite/cerrado` se rechaza
  siempre. Sólo fixtures, **sin probar contra el servicio real**
  ([detalle](../docs/INVITATIONS.md#motivo-del-rechazo-y-salida-por-el-navegador-2026-10-19)).
  Pendiente del dueño: si una aceptación falla, mandar las
  líneas `rune: invitation:`; con eso se sabe si fue un 403 con código, un
  401 o un rechazo local.
- Estado: entrada limn, reconocimiento y preview ligados a cuenta/solicitud;
  cooldown con reintento explícito y caducidad opcional. La tarjeta limn de
  enlace completo requiere «Revisar invitación» antes de consultar.
  `invitation_accept=true` en el adaptador de sesión: preflight anónimo,
  revalidación y un único POST. Un HTTP 400 con hCaptcha se muestra como
  «Discord pide una verificación» y «Verificar» abre la ventana aislada de
  `rune-login`; con la respuesta, el adaptador repite esa aceptación una vez
  con `X-Captcha-*`. Cerrar o vencer no envía nada. Forma confirmada con REA
  sobre el cliente web ([registro](../docs/REA.md#cliente-web-oficial-aceptar-invitaciones-y-hcaptcha-2026-10-16)).
  Suite integral de 1446 aprobadas y ocho ignoradas; ventana nativa con HTML
  sintético. **No probado contra el servicio real.** Discord puede pedir
  teléfono o restringir la cuenta ([SECURITY.md](../docs/SECURITY.md#verificación-de-invitaciones-autorizada-el-2026-10-16)).
  Los enlaces mezclados con otro texto y las invitaciones de grupo, amistad
  e invitado ya tienen tarjeta y aceptación (2026-10-18); crear, listar y
  revocar están en P7. Las reglas del servidor y el onboarding al entrar
  llegaron el 2026-10-18 (rama `reglas-y-onboarding`, al final). Las
  invitaciones de evento llegaron el 2026-10-19 (paquete
  «eventos-programados», al final; sólo fixtures).
- Módulos: `rune-ui::invitations`, `rune-core::invitation_acceptance`,
  contrato de `rune-provider`, adaptador de sesión y `rune-login`;
  reglas y onboarding con su modelo propio (`rune_model::onboarding`).
- Dependencias: contrato de metadatos y errores; la validación real depende
  de una prueba del dueño con su cuenta.
- Cierre verificable: enlaces inválidos conservan error sin abrir navegador;
  cierre/cambio de cuenta descarta previews viejos; caducidad y cooldown se
  explican sin afirmar membresía; un desafío sólo se resuelve con un clic y
  produce a lo sumo un reintento. Crear/listar/revocar (P7) y reglas y
  onboarding (2026-10-18) tienen contratos propios.

### 03. `caps` — Integrar acciones existentes por capacidad

- [ ] Completar cada recorrido soportado sin habilitar bits en bloque.
- Estado: reacciones, typing, fijados, perfiles, presencia, administración y
  llamada tienen contratos/UI o simulación; integración real parcial. El
  incremento actual revalida typing antes de consumir cooldown, separa
  lectura/mutación de fijados y termina su loading ante error con reintento
  explícito. Los temporizadores de perfil limpian su estado local vencido sin
  enviar comandos cuando la capacidad ya no está. Estos guards no habilitan
  bits remotos. La reconciliación concurrente de fijados quedó integrada:
  **741 aprobadas, seis ignoradas**, 23 suites, formato, Clippy estricto y
  release correctos. Panel cargado y loading revisados con REA sobre el
  ejecutable propio: [checkpoint](../docs/evidence/pins-checkpoint-2026-10-15.json).
- Módulos: contratos de `rune-provider`, `Store::abilities`,
  `server_abilities`, adaptadores y UI existente; sin crate nuevo por familia.
- Dependencias: una vía verificable apropiada para cada operación.
- Cierre verificable: confirmación/evento/fallo por acción; revocación impide
  confirmar; rechazo revierte optimismo; una operación no soportada explica
  su motivo. Cada capacidad registra por separado fixtures y prueba real.
- [x] Un borrado/desfijado durante `LoadPins` prevalece sobre su instantánea
  atrasada; se conservan ediciones recientes y lápidas. El delta está acotado
  a 256 IDs, con fallo/reintento ante overflow; provisional no significa vacío.
  Cierre, revocación, timeout y desconexión terminan la lectura exacta. Los
  resultados de otra petición/canal/sesión no se publican. La integración
  remota de cada capacidad sigue pendiente.
- [x] Sesión no oficial (`rune-discord-session`): `reactions`, `typing` y
  `pins` habilitados, cada uno con su ruta por la cola única, revalidación de
  permisos al admitir, al salir de la cola y en vuelo, un solo intento y
  fallos clasificados. Moderación, canales, servidores y respuestas en hilos
  siguen apagados y la UI explica el motivo.
  - Reacciones (Unicode): PUT/DELETE `.../reactions/{emoji}/@me`; la
    dirección la dice el comando (`add`, desde el 2026-10-18; el GET de
    recuperación, si falta el mensaje, sólo valida); 204 y
    eco del Gateway cuentan una vez; 403/429 definitivos, revocación en cola
    sin petición, revocación en vuelo ambigua; el rechazo revierte el
    optimismo en `Store` (pruebas de State existentes). **Fixtures: sí. Prueba
    real: no.**
  - Typing: POST `/channels/{c}/typing` (204 o 200), silencioso ante fallo,
    descartado tras 5 s en cola; `TYPING_START` ajeno → `Event::Typing`.
    **Fixtures: sí. Prueba real: no** (tampoco se sabe si el Gateway de la
    cuenta entrega typing de servidores sin suscripción).
  - Fijados: PUT/DELETE `/channels/{c}/messages/pins/{m}` (DM libre; en un
    servidor, desde el 2026-10-18, `PIN_MESSAGES` y el historial, como el
    cliente oficial), confirmación por
    `MessagePatched`; lista paginada hasta 250 sin cortar.
    `CHANNEL_PINS_UPDATE` no se usa (no dice qué mensaje). **Fixtures: sí.
    Prueba real: no.**
  - ~~Un 429 de typing frena también los envíos~~: resuelto el 2026-10-18
    (`envios-y-subidas`), typing tiene su propia retención. Hecho el
    2026-10-18 (rama `reacciones-fijados-y-busqueda`): la reacción lleva el
    estado pedido (gana el último clic aunque el primero falle; los fijados
    ya eran estado) y `PIN_MESSAGES` (bit 51).

### 04. `adapter-cache` — Edición y patches tras expulsión de caché

- [x] Corregir edición y patches después de expulsión de la caché del adaptador.
- Estado: caché acotada de 512 mensajes; escritura rechaza autoría desconocida.
  Tres regresiones con 513 expulsiones reprodujeron patch descartado y rechazo
  de edición/borrado propios antes de recuperar autoría. `MessagePatch` ahora
  llega a Store y SQLite sin reconstruir mensajes desconocidos; mantiene
  versiones, lápidas, referencias y reversión optimista. El adaptador recupera
  autoría mediante GET acotado y revalida antes de escribir; borrado/revocación/
  logout impiden continuar. Conserva caché512, cola16 y una REST en vuelo.
  **781 aprobadas, seis ignoradas**, 23 suites; formato, Clippy estricto y release
  correctos. REA ejecutó 131 pruebas del adaptador y 13 regresiones de State:
  [checkpoint](../docs/evidence/message-patches-checkpoint-2026-10-15.json).
- Módulos: `rune-discord-session::messages` y fixtures Gateway/REST existentes.
- Dependencias: fixture con más de 512 mensajes y mensaje propio aún visible.
- Cierre verificable: distinguir rechazo intencional de pérdida de patch;
  recuperar contexto si se requiere sin omitir autoría ni crecer sin límite.
  Sólo se usaron fixtures locales; no se habilitaron capacidades nuevas ni
  persistencia de la sesión real.

### 05. `search` — Historial guardado y contexto por ID

- [x] Añadir consultas paginadas cancelables y apertura de contexto por ID.
- Estado: «Cargados» y «En este equipo», con texto/autor/archivos/fijados/fechas
  combinados; búsqueda SQLite fuera de UI, páginas de 20 y buffer de cinco
  páginas ligeras. Cursores ligados a consulta/canal/época/revisión; una pérdida
  de red no invalida por sí sola una consulta local. Cambios de mensajes vacían
  resultados y ofrecen actualizar, sin loops. Contexto local de hasta 51 mensajes
  con ancla exacta, origen visible y cobertura desconocida; contexto del proveedor
  por capacidad independiente. UI/host/Client/Store/adaptador cancelan, validan
  permisos y rechazan respuestas obsoletas antes de persistir o repoblar autoría.
  Se preservan borradores, lápidas y reversión optimista. Volver a recientes pide
  una página real; ver contexto no marca leído. **851 aprobadas, seis ignoradas**,
  23 suites, formato/Clippy estricto/release correctos. Tres capturas nativas
  revisadas; REA verificó 101 pruebas del núcleo y 150 del adaptador:
  [checkpoint](../docs/evidence/search-context-checkpoint-2026-10-15.json).
- Módulos: `rune-model::message_search`, `rune-storage::message_search`,
  `rune-core::{local_history,context,storage::local_reads}`,
  `rune-state::context`, `rune-ui::{search,context}` y host; contrato genérico
  `LoadContext`/`ContextLoaded`. Se reutiliza limn, sin extensión ni dependencia nueva.
- Dependencias: 01 y un contrato de contexto por ID verificable.
- Cierre verificado: consultas y callbacks aislados por identidad, canal y
  petición; cancelación y plazos terminales; 16 slots ocupados no impiden avanzar
  lecturas locales. Cursor viejo, datos corruptos/grandes y ancla ausente producen
  error explícito. Las instantáneas concurrentes no revierten patches recientes.
  El historial guardado conserva su límite de retención. La búsqueda remota
  (2026-10-17) y la caché cifrada de cuentas reales (2026-10-18, ver 01) se
  hicieron después, sólo con fixtures.
- [x] 2026-10-17 (P2 de [paridad](serein-parity.md), rama `lecturas-y-busqueda`):
  búsqueda en todo el historial del proveedor. Capacidad `message_search`,
  `Command::SearchMessages` con `rune_model::RemoteSearch` (texto, `from:`,
  `mentions:`, `has:` con sticker, `in:`, `before:`/`after:`/`during:`,
  `pinned:`, `author_type:`, también en castellano) y `Event::SearchResults`
  con páginas de 25 validadas. La UI suma el alcance «Todo el historial»:
  sugiere filtros mientras se escriben (personas, canales, tipos, fechas;
  ↑↓, Enter/Tab, Esc), muestra al instante lo cargado que coincide, Enter
  pide la página, la combina con lo cargado sin duplicar y dice el origen
  de cada resultado; pagina, avisa del índice en preparación y lo vuelve a
  pedir una sola vez, solo. Un resultado de otro canal se abre por ID con el
  contexto de este recorrido. «Cargados» sin resultados sigue con Enter en
  todo el historial. `from:`/`mentions:` y `@` buscan miembros en el
  proveedor (`member_search`). La simulación busca en todos sus mensajes y
  prepara una vez el índice de #rendimiento; la sesión de Discord usa
  `/guilds/{g}/messages/search` y `/channels/{c}/messages/search` como
  Serein, sólo con fixtures. Workspace: **1806 aprobadas, nueve ignoradas**,
  25 suites; formato y Clippy estricto correctos. Capturas:
  [filtros](../docs/img/search-filters-2026-10-17.png),
  [resultados](../docs/img/search-remote-2026-10-17.png),
  [índice en preparación](../docs/img/search-indexing-2026-10-17.png).
  Pendiente: probarlo contra Discord. Orden por relevancia, `has:poll` y
  `link_hostname`: hechos el 2026-10-18 (rama `reacciones-fijados-y-busqueda`).
  Desde el 2026-10-18 los mensajes encontrados se guardan cifrados, aparte de
  la historia (ver 01).

### 06. `reads` — Confirmaciones de lectura e inbox

- [x] Añadir contrato de confirmaciones/versiones, reconciliación y bandeja global.
- Estado: cursor local, recordatorio manual y confirmación de proveedor separados;
  ACK por cuenta/canal/petición y versión positiva, plazos terminales y reintento
  explícito. SQLite v4 conserva marcas por identidad, incluso para canales
  temporalmente ausentes; fallo al restaurar mantiene nuevas elecciones en RAM
  con aviso. La bandeja limn agrega hasta 1000 canales visibles, filtra menciones
  conocidas y abre el ID exacto por contexto. La apertura normal revela el primer
  no leído conocido, sin aritmética de IDs ni confirmación al ver contexto disperso.
  La integración local y con proveedor simulado está implementada; **sincronización
  de lectura con proveedores reales sigue pendiente y su capacidad deshabilitada**.
- Módulos: `rune-model::read_mark`, `rune-core::read_states`,
  `rune-state::reads`, `rune-ui::{reads,inbox}`, simulación y host;
  extender contratos y `rune-storage`/worker existentes. Sin dependencia nueva.
- Dependencias: contexto por ID de 05 y contrato remoto cuando exista soporte.
- Cierre local verificado: sin capacidad no sale escritura al proveedor; ACK
  viejo no retrocede lectura, borra recordatorios nuevos ni consume menciones
  posteriores. Reintentar no marca mensajes nuevos. Cola llena conserva cambios
  y cierre espera su barrera; revisión saturada compara la elección completa.
  Inbox respeta visibilidad/permisos y conserva el borrador al cargar un ancla
  fuera de la ventana. **925 aprobadas, seis ignoradas**, 23 suites; formato,
  Clippy estricto y release correctos. Cuatro capturas nativas revisadas:
  bandeja con privado, recordatorio tras reinicio, contexto fuera de ventana y
  bandeja vacía compacta. REA ejecutó 115 pruebas del núcleo (una ignorada),
  94 de State y 32 del host, sin truncación:
  [checkpoint](../docs/evidence/reads-inbox-checkpoint-2026-10-15.json).
- [x] 2026-10-17 (P2 de [paridad](serein-parity.md), rama `lecturas-y-busqueda`):
  **la sesión de Discord habilita `read_state`**. Manda el ACK como Serein
  (`POST /channels/{c}/messages/{m}/ack` con el token anterior), uno en vuelo
  por canal (lo leído mientras tanto sale al contestar), por la cola REST
  común y el más nuevo por canal, así que un hCaptcha se
  ofrece como en cualquier escritura; aplica la versión del `read_state` del
  `READY` y `MESSAGE_ACK` de otros dispositivos con versiones propias que
  crecen entre reconexiones. Capacidades nuevas: `remote_unread`
  (`Command::MarkUnread` → ACK `manual` con el cursor anterior y las menciones;
  `Event::MarkedUnread`, también de otro dispositivo, que `rune-state` pone en
  el primer mensaje cargado después del cursor) y `server_read`
  (`Command::MarkServerRead` → `/guilds/{g}/ack`, «Marcar como leído» en el
  menú del servidor y en el de la lista de canales). `PASSIVE_UPDATE_V2` y
  `V1` traen los no leídos (`Event::LatestMessages`) y la voz de servidores
  pasivos. La simulación hace todo esto de forma determinista. «Marcar como
  no leído» dice «solo en RUNE» si el recordatorio no llega al proveedor.
  [Captura](../docs/img/server-read-2026-10-17.png) y
  [detalle](../docs/DISCORD_SESSION.md#lecturas-búsqueda-y-miembros-remotos-2026-10-17).
  Pendiente: probar contra Discord (ver lo que tiene que probar el dueño en
  ese detalle) y el ACK en lote `/read-states/ack-bulk`, que Serein no usa.

### 07. `uploads` — Seleccionar, subir y enviar archivos

- [ ] Completar adjuntos de borrador, carga, cancelación y envío confirmado.
- **2026-10-17 (rama `adjuntos`):**
  - [x] **Subida real en `rune-discord-session`** por la ruta de Serein:
    destino por archivo (`POST /channels/{id}/attachments`), `PUT` sin sesión
    sólo al almacenamiento de Discord, mensaje con los archivos guardados y
    el nonce de la sesión; progreso cada 256 KiB, cancelación sin envío antes
    del mensaje, ambiguo después; lugar propio en el bucle, una a la vez;
    capacidad `attachments` habilitada. Límites por plan y mejoras del
    servidor con `Event::UploadLimit` (UI valida contra el del canal; Core
    recorta a 32 MiB). Plazo +1 s/128 KiB; confirmación por orden y tamaño.
    **Sólo fixtures en loopback, sin probar contra el servicio real**;
    captcha en esta escritura no implementado.
    [Detalle](../docs/DISCORD_SESSION.md#adjuntos-de-salida-2026-10-17).
  - [x] **Pegar del portapapeles**: menú «+» → «Pegar del portapapeles» y
    Ctrl+V de nombres de archivos copiados (retenido hasta 500 ms con el
    `PasteTarget` de limn), por `rune_platform::clipboard` (arboard/X11,
    XWayland en Wayland) en el worker de Core; imagen como `imagen.png`.
    Límite: una imagen sola no produce Ctrl+V en egui-winit (sin texto); se
    pega desde el menú. [Detalle](../docs/FILES.md#pegar-del-portapapeles-2026-10-17).
  - [ ] **Arrastrar y soltar**: sigue bloqueado; `limn-dnd` (PR #55/#62/#63
    ya mezcladas) necesita el dueño Wayland compartido dentro de eframe para
    no chocar con el portapapeles de egui-winit en Hyprland (SDK#1–3 sin
    publicar), así que el drop se sigue rechazando (07.3.2). Revisado el
    2026-10-17 (rama `estabilidad`): sigue bloqueado, ver la sección de
    estabilidad al final.
  - Verificación: **1616 pruebas, 0 fallos, 8 ignoradas, 25 suites**
    (todas las features), fmt y Clippy estricto correctos.
- **2026-10-17 (rama `social-2`):**
  - [x] **hCaptcha en las subidas**: si Discord lo pide al nombrar un
    archivo o al enviar el mensaje, la persona lo resuelve en la ventana
    aislada y el envío se repite una vez desde el principio (se vuelven a
    subir los archivos, con el tiempo que tenía); Core corre el plazo y
    reinicia el progreso. Ver 13 y
    [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#verificación-en-cualquier-escritura-2026-10-17).
    *Atajo:* no se guardan los nombres ya subidos para el reintento.
  - [x] **Archivos en publicaciones nuevas de foro** (`CreatePost.files`):
    «Adjuntar archivos» en el diálogo, los mismos tokens del composer (el
    foro guarda los suyos), `Client::create_post` une los bytes en el actor
    de subidas, la sesión los prepara en el foro como Serein. Van por la
    cola REST; desde el 2026-10-18 (`envios-y-subidas`) con barra de
    progreso (`Event::PostProgress`). Sólo fixtures.
    [Captura](../docs/img/forum-post-files-2026-10-17.png).
  - Para el dueño contra el servicio real: subir uno y varios archivos (con y
    sin texto, como respuesta, en privado y en un servidor con mejoras),
    cancelar a mitad de un archivo grande, un archivo sobre el límite de la
    cuenta, y pegar archivos/imagen en Hyprland con XWayland.
- Estado: selección de **hasta diez archivos** (ocho hasta el 2026-10-18) integrada en composer limn,
  host, Core y Mock: elegir/preparar, borrador, file-only o texto, progreso,
  cancelación, ACK y reintento tipado. Core conserva FD/Bytes; Ui/Store sólo
  metadatos y tokens. Capacidad real deshabilitada. Los archivos de borrador
  vuelven tras reiniciar desde el 2026-10-18 (`envios-y-subidas`): se guarda
  su ruta y tamaño, nunca los bytes. Core Paths de 07.3.1 conserva
  apertura en el mismo servicio sin portal y pruebas dirigidas/integrales.
  La política actual 07.3.2 rechaza el ingreso legacy del host, porque la vía RawInput de winit
  0.30.13 no aporta posición fiable ligada al lote: Wayland no produce eventos
  DnD de archivos y X11 no expone su posición. No se llama a la fachada Paths.
- Módulos existentes: `rune-core::uploads::{actor,prepare,runtime}`,
  `rune-provider::uploads`, Mock, `rune-state::uploads`, `rune-ui::uploads`,
  `rune-components::uploads` y puente Desktop. [Contrato](uploads-contract.md).
- Dependencias: selector Linux de 08 conectado; falta un contrato/transporte
  remoto verificable para habilitar proveedores reales.
- Cierre verificable: elegir no envía; límites se validan antes/durante lectura;
  progreso y cancelación efectivos; cambiar cuenta descarta cargas; fallo y
  reintento conservan borrador sin duplicar adjuntos. Archivo/red fuera de UI.
- Pendiente: resolver el backend nativo con posición/lote reales y repetir QA de 07.3.1,
  implementar pegado de archivos y transporte remoto.
  El checkpoint **07.1** verificó en ventana nativa
  selección/preparación sin consumir borrador y envío explícito de un fixture
  de 50 B confirmado por Mock, con composer vacío y sin pendiente.
  Incluida la limpieza de metadatos al eliminar canal/servidor: **1032
  aprobadas con todas las features, seis ignoradas**, 25 suites, formato,
  Clippy estricto y release correctos. Cinco replays REA con salida 0 y dos
  capturas revisadas en el [registro](../docs/evidence/uploads-checkpoint-2026-10-15.json).
  La extensión **07.2 está implementada y probada localmente**: lote completo
  o error, reserva atómica, retry íntegro y lista acotada. **1073 aprobadas,
  seis ignoradas**, 25 suites, formato, Clippy estricto y release correctos;
  seis replays REA con salida 0 en el
  [registro 07.2](../docs/evidence/uploads-multiple-checkpoint-2026-10-15.json).
  La selección y el envío nativos de dos fixtures de 48 B se verificaron con
  GTK/Mock: borrador preparado y luego ambos adjuntos confirmados con editor
  vacío. **07.2 local queda cerrado**, con binarios/fuentes en el pin `2618b37`.
  La PR #49 y el prototipo de drop 07.3.1 son una etapa separada: pin histórico
  `1ad33eb`, API `Client::prepare_attachment_paths` implementada; la conexión
  desde el host fue retirada en 07.3.2.
  Verificación dirigida: Core uploads 50 + host 6, UI 6, plataforma 54 y
  componente 1. El prototipo revalidaba el destino después de la entrada
  ordinaria; drop junto a Enter conservaba el borrador, incluso con lote rechazado. Suite
  integral: **1098 aprobadas, seis ignoradas**, 25 suites en 5,47 s con todas
  las features; formato, Clippy estricto de todos los targets/features y release
  de ocho crates en 51,13 s correctos. Cinco replays REA canónicos: 670 aprobadas,
  una ignorada, salida 0 sin truncar. Dos gestos nativos no prepararon adjuntos;
  no hubo Enter/envío ni PNG aceptado. [Registro local](../docs/evidence/uploads-drop-checkpoint-2026-10-15.json).
  La [API nueva de winit](https://github.com/rust-windowing/winit/pull/4571)
  requiere 0.31 beta y [la migración egui/eframe](https://github.com/emilk/egui/pull/7731)
  sigue en borrador; un pin sólo de limn no migra esa capa. Como alternativa
  Wayland, `limn-dnd` ya existe en los fuentes git, fuera del grafo/bridge RUNE:
  su endurecimiento receive-only está implementado en la
  [PR #55](https://github.com/Niiihuel/limn/pull/55), Draft abierta, con QA
  limitada a metadatos/decisiones/cierre normal en Weston privado.
  [Registro](../docs/evidence/limn-drop-receiver-weston-2026-10-15.json).
  Siguen pendientes owner SDK/clipboard compartido para Hyprland, pin y
  consumidor RUNE. No obliga a migrar winit ni acredita DnD en RUNE; el alcance
  nativo 07.3.1 queda abierto.
  El owner de biblioteca #62 y el opt-in entrante #63 ya están publicados en el
  [checkpoint actual](../docs/evidence/limn-text-clipboard-matrix-2026-10-15.json); queda su integración SDK/RUNE y verificación Hyprland.
  [Contrato](uploads-next.md#0731-arrastre-de-archivos-implementado).
  Política actual **07.3.2**: limpiar todos los archivos legacy de `RawInput`
  antes del bridge y rechazar también el fallback, sin inferir destino ni
  leer `DroppedFile::bytes`. **Rechazo negativo y pulido visual cerrados**:
  ocho regresiones del host y siete de avisos verdes. Pin actual
  `22e0fd93c14b3f3c89375135f83c1e9c41307330`, con la
  [PR #50](https://github.com/Niiihuel/limn/pull/50) abierta, incorporada por
  cherry-pick propio. Avisos en flujo sobre el editor y altura/lista acotadas
  conservan el borrador accesible. Dolphin soltó dos fixtures de 54/55 B sobre
  Mock RUNE X11: aviso completo, «Borrador conservado» visible y sin adjuntos
  preparados; no se inyectó Enter. El no-envío lo prueban los tests del host.
  Una [captura negativa](../docs/img/uploads-drop-native-rejected-2026-10-15.png)
  eleva las canónicas Mock a 18 (17 históricas); no cierra el arrastre funcional.
  **1120 aprobadas, seis ignoradas**, 25 suites en 5,50 s; formato, Clippy
  estricto y release de seis crates en 49,56 s correctos. Tres replays REA
  completos suman 462 aprobadas, sin filtros/ignoradas y con salida 0 sin truncar.
  [Registro final](../docs/evidence/uploads-drop-rejected-checkpoint-2026-10-15.json),
  [auditoría](../docs/evidence/uploads-drop-rejected-audit-2026-10-15.json) y
  [política](uploads-next.md#0732-rechazo-del-ingreso-legacy-actual).
  El snapshot histórico SaveFile conserva 1112 pruebas y pin `1ad33eb`.
  Esto no cierra el recorrido completo ni acredita transporte real.

### 08. `platform` — Archivos, avisos y servicios del sistema

- [ ] Implementar la frontera de plataforma con resultados y cancelación.
- **2026-10-17 (rama `adjuntos`):**
  - [x] **08.2 «Guardar como»**: menú del adjunto y visor de imágenes
    (`ImageViewer::action`, [PR #72 de limn](https://github.com/Niiihuel/limn/pull/72),
    pin `rune/viewer-actions` `514993f`); Core valida antes del diálogo,
    SaveFile en el selector del actor de adjuntos (uno a la vez), parcial
    privado y hard link al nombre exacto, nunca sobrescribe; progreso,
    cancelación, cambio de cuenta. Desvíos del contrato (sin
    `SharedNativeDialogs` aparte, sin anclar el directorio en un
    descriptor) en [su estado](save-as-contract.md#estado-de-082-2026-10-17).
    08.3 (reemplazo confirmado) hecho el 2026-10-18 (rama
    `escritorio-linux`, abajo).
  - [x] **Notificaciones del sistema** (`org.freedesktop.Notifications`)
    para menciones y privados: respetan silencios de servidor, categoría y
    canal, «No molestar», la conversación leída y la opción de ocultar el
    contenido; un clic abre sólo si es de la cuenta actual. Ajustes →
    Notificaciones. [Detalle](../docs/FILES.md#notificaciones-del-sistema-2026-10-17).
  - [x] **Sonido de mensajes** propio por el mismo camino que los de la
    llamada, con su preferencia.
  - Sin QA de escritorio en esta iteración: probar en Hyprland el diálogo
    Guardar como, notificaciones y su clic, y el sonido. Windows/macOS sin
    soporte.
- Estado: `rune-platform::file_dialogs` implementado para elegir hasta diez archivos
  (ocho hasta el 2026-10-18) local en Linux mediante portal, con descriptor de sólo lectura, límites,
  worker fijo y cancelación/limpieza acotada. Apertura, cancelación y selección
  GTK reales verificadas con un archivo propio, sin leer su contenido.
  La ampliación múltiple tiene 45 pruebas del crate y selección nativa de dos
  descriptores verificada. 07.3.1 añade `validate_local_paths`, `open_local_files`
  y `FileChooser::wait_idle`: apertura por rutas en el worker fijo Core sin
  conexión portal nueva, con 54 pruebas dirigidas del crate y suite integral
  verificada. El host actual 07.3.2 rechaza el drop legacy antes del bridge;
  la API Core Paths queda para una integración fiable futura. El drop nativo
  sigue pendiente de un backend integrado con posición/lote fiables, incluida
  la evaluación de `limn-dnd` descrita en 07; no es un fallo del selector GTK ya verificado. Pegado y Windows/macOS
  no acreditados.
  Core ya consume el crate para 07; Desktop usa su fachada, sin dependencia
  directa de plataforma. Rutas y enlaces siguen en host.
  **SaveFile 08.1 completo sólo en plataforma**: `SaveRequest`,
  `FileChooser::save_file` y destino privado, con 64 unitarias y un doctest.
  Elección GTK del destino exacto verificada sin crear archivos ni modificar
  el existente; cancelación terminal `Cancelled` con observador tardío sin
  ventana observada. Dos recortes GTK no suman capturas Mock. El
  [checkpoint 08.1](../docs/evidence/save-file-picker-checkpoint-2026-10-15.json)
  conserva tres replays REA con salida 0 sin truncar. 08.2 (Core, UI y
  descargas) hecho el 2026-10-17, arriba, sin un `SharedNativeDialogs`
  aparte; 08.3 (reemplazo autorizado) hecho el 2026-10-18 en la rama
  `escritorio-linux`. Notificaciones del SO y otras plataformas siguen
  pendientes. [Contrato](save-as-contract.md).
- Módulos: `file_dialogs` conectado a 07; proponer `rune-platform::{paths,
  clipboard,notifications,shortcuts}` cuando tengan su propia implementación.
  Conectar host, intents, descargas y preferencias según cada recorrido.
- Dependencias: política de privacidad/silencios e identidad estable.
- Cierre verificable: selector sin sobrescritura implícita; avisos respetan
  silencios y privacidad; clic de otra cuenta no abre sus datos; errores no
  bloquean UI. Anunciar soporte sólo de los sistemas efectivamente verificados.
  Keyring se implementa al abordar persistencia de sesión autorizada.
- Prerrequisito verificado: **34 pruebas específicas**; workspace **959
  aprobadas, seis ignoradas**, 25 suites, formato, Clippy estricto y release
  con todas las features correctos. Wayland usaba el diálogo sin parent
  hasta contar con un token `xdg_foreign`: desde el 2026-10-18 la ventana se
  exporta (rama `escritorio-linux`); no se afirma modalidad sin un portal
  real ni soporte Windows/macOS. [Frontera y límites](../docs/FILES.md#selector-local-linux-prerrequisito-de-07)
  y [checkpoint](../docs/evidence/file-picker-checkpoint-2026-10-15.json).

### 09. `local-voice` — Prueba audible y referencia de eco

- [x] Salida seleccionada, prueba audible y referencia para AEC: código y
  pruebas sintéticas (2026-10-16).
  [Detalle](../docs/MEDIA.md#implementado-salida-local-prueba-audible-y-referencia-de-eco-2026-10-16).
  `rune-media::Playback`: ring SPSC acotado; underrun = silencio contado;
  overrun = descarta lo más viejo, contado; remuestreo lineal acotado;
  volumen y ensordecer por atómico; reconstrucción al cambiar o perder la
  salida y al mudarse el predeterminado. `Feed` para escucharse (y luego
  para llamadas); `EchoReference` con lo que realmente sonó y AEC3 de Sonora
  sólo con esa referencia y el ajuste. Ajustes → Voz y video: altavoz,
  volumen, «Probar sonido», «Escucharme», «Cancelación de eco».
- Verificado sin dispositivos: callback sin asignación (asignador contador) y
  sin locks por construcción; políticas, gating de la referencia, mute
  (la captura silenciada no alimenta «Escucharme») y AEC con eco sintético.
- [ ] Pendiente: escucha del dueño (`cargo run -p rune-media --example
  playback`, ver MEDIA.md); AEC con altavoces reales; latencia de captura e
  ida y vuelta (hoy sólo la de salida que predice cpal); comparar muestras
  autorizadas; remuestreador de banda limitada si un dispositivo sin 48 kHz
  suena mal.

### 10. `auth` — Proveedores, cuentas y sesiones

- [x] Añadir selector de proveedor y gestión de varias cuentas/sesiones desde UI
  (proveedor 2026-10-17; cuentas 2026-10-16, abajo).
- Estado: el recorrido **login WebView temporal → identidad verificada →
  workspace nativo limn** ya está implementado. El auxiliar Linux termina antes
  de entregar la sesión; el adaptador verifica identidad y emite SignedIn.
  Login/cancelación/cierre y limpieza local existen. El dueño confirmó antes
  acceso y lectura; esta revisión sólo leyó fuentes, sin repetir operaciones
  reales. [Flujo actual](../docs/DISCORD_SESSION.md#entrada-y-transición-al-workspace).
- [x] Elegir el proveedor desde limn (2026-10-17): Ajustes → Conexión →
  «Elegí el proveedor» y «Usar otro proveedor» en la página de acceso; cada
  opción con su origen (simulado, oficial, no oficial), por qué no está
  disponible (OAuth2 sin ID de aplicación, sesión sin la feature) y la
  advertencia de experimento en la no oficial; cambiar se confirma. El host
  sustituye en caliente `Client` y `Store` (`Intent::UseProvider`,
  `rune_ui::provider_changed`) y la ventana se limpia como al cambiar de
  cuenta (dominio, borradores, búsquedas, visores, subidas y trabajos;
  quedan preferencias y cuentas recordadas). `--provider` es el valor
  inicial; sin él, la última elección de `$XDG_CONFIG_HOME/rune/provider`
  (sólo la clave). El origen se ve siempre al pie de la columna y en el
  acceso. Pruebas: `rune-ui/src/tests/providers.rs` y del host
  (`switching_the_provider_clears_the_account_and_keeps_the_window`).
  Capturas: `docs/img/providers-2026-10-17.png`,
  `docs/img/sign-in-providers-2026-10-17.png`. Sin probar: el cambio en
  caliente hacia la sesión real y OAuth2 con una cuenta.
  [Detalle](../docs/DISCORD_SESSION.md#entrada-y-transición-al-workspace).
  Sigue habiendo una sesión activa a la vez; las cuentas recordadas se
  alternan.
- [x] Recordar la sesión personal en el llavero del sistema y restaurarla al
  arrancar (autorizado 2026-10-16): verificación de identidad antes de
  `SignedIn`, 401 borra la entrada y abre el login, cerrar sesión la borra,
  sin llavero queda en RAM con aviso; Ajustes → Conexión muestra el estado y
  «Olvidar esta sesión». Probado con llavero falso y bus D-Bus privado; sin
  probar con un Secret Service real ni contra Discord.
  [Detalle](../docs/DISCORD_SESSION.md#sesión-recordada-en-el-llavero-2026-10-16).
- [x] Varias cuentas guardadas (autorizado 2026-10-16): una entrada del
  llavero por cuenta con su nombre visible como atributo; al arrancar se
  verifica sólo la última usada (fecha de creación de la entrada); la
  pantalla de acceso lista «Continuar como X», «Olvidar» con confirmación y
  «Usar otra cuenta»; Ajustes → Mi cuenta → «Cambiar de cuenta» lista las
  otras y «Agregar una cuenta». Cambiar pasa por el mismo `SignedOut` que
  limpia la cuenta y conserva su entrada. Probado con llavero falso y
  pruebas de UI; sin probar con un Secret Service real ni contra Discord.
  [Detalle](../docs/DISCORD_SESSION.md#sesión-recordada-en-el-llavero-2026-10-16).
- [x] «Cambiar de cuenta» también en el menú del panel de usuario: abre
  Ajustes → Mi cuenta con las cuentas recordadas (2026-10-16).
- [x] Avatar real en la lista de cuentas (2026-10-17): el hash va como
  atributo no secreto `avatar` del llavero junto al nombre; vuelve en
  `RememberedAccount.avatar` y se carga por el CDN como los demás avatares,
  con la inicial de respaldo, en el acceso y en Mi cuenta. Llavero falso y
  prueba de UI; sin probar con Secret Service real.
- [x] Foco del SO al cerrar el auxiliar (2026-10-17): al salir de
  `AwaitingExternal` (completado o cancelado) el host manda
  `ViewportCommand::Focus` y `RequestUserAttention`. X11 la activa; en
  Wayland winit 0.30 sólo puede activar con su propio token xdg-activation
  (lo que haga el compositor); pasar el token del auxiliar no está expuesto
  por winit/eframe. Prueba de la transición (`login_closed`); **sin probar**
  en Hyprland/GNOME/KDE con un login real.
- Módulos: ampliar `rune-ui::auth`, ciclo de sesión en core y coordinador en host/core;
  el llavero vive en `rune-platform::secrets`.
- Dependencias: identidad estable de 01 antes de persistir cuentas reales.
- Cierre verificable: cambio de cuenta limpia dominio, borradores, búsquedas,
  visores y trabajos; secretos fuera de eventos/logs/configuración; origen del
  proveedor visible. Canje OAuth2 requiere su propia validación real registrada.

### 11. `conversations` — Crear privados y administrar grupos

- [x] Iniciar/cerrar privados y gestionar grupos con confirmación
  (2026-10-17; falta prueba real contra el servicio).
- Estado: 11.1–11.3 abajo; en la simulación y, sólo con fixtures, en la
  sesión no oficial.
- [x] **11.1 (2026-10-16)**: `Command::{OpenDirect, RenameGroup,
  AddGroupMember, RemoveGroupMember, LeaveGroup}`, `Failure::{Direct, Group}`
  y capacidades `open_direct`/`manage_groups`. `Channel.owner`
  (`serde(default)`, sin cambio de `SCHEMA`). «Mensaje» (perfil o amigo) abre
  el privado existente sin red; si no hay, envía `OpenDirect` y sólo abre el
  canal que el proveedor nombra con `Created`; un segundo clic mientras espera
  no repite el pedido, y el Mock devuelve el mismo canal al repetir.
  `rune-state::conversations` decide `GroupAbilities` (renombrar/añadir/salir;
  quitar sólo quien es dueño); listar grupos no concede gestionarlos. Diálogo
  limn «Gestionar grupo» desde la cabecera, con confirmación para quitar y
  abandonar; los cambios se ven cuando llega `ChannelChanged`/`ChannelRemoved`.
  Sesión no oficial: sólo lectura de `owner_id` y `CHANNEL_RECIPIENT_ADD/REMOVE`;
  escrituras desactivadas por capacidad. Pendiente: icono del grupo, cerrar un
  privado, mensajes de sistema simulados de alta/baja, y prueba real.
- [x] **11.2 (2026-10-17, rama `social`)**: `Command::{CreateGroup,
  SetGroupIcon, CloseDirect}`, `Failure::{NewGroup, Close}`, capacidades
  `create_groups`, `group_members` (añadir/quitar ya no viene con
  `manage_groups`) y `close_direct`; `Channel.icon` (`serde(default)`, sin
  `SCHEMA`); `GroupIcon` = PNG acotado (256 KiB, 1024 px). UI limn: «+»
  junto a «Mensajes directos» abre «Crear grupo» (búsqueda, casillas, tope
  de 9 amigos; con uno solo abre su privado), que se abre sólo con
  `Created`; clic secundario en la lista: cerrar un privado (con
  confirmación) o gestionar/abandonar un grupo; imagen del grupo en la
  lista, la cabecera y el diálogo («Cambiar imagen»/«Quitar imagen»):
  `Intent::PickGroupIcon` → `Client::pick_group_icon` elige con
  `rune-platform::file_dialogs` fuera del hilo de UI, lee hasta 8 MB, recorta
  a cuadrado y manda un PNG chico. Mock: crear grupos, imagen (se acepta y la
  interfaz dice que no se dibuja), cerrar sin perder historia (vuelve al
  abrirlo o cuando la otra persona escribe) y líneas de sistema de alta,
  baja, nombre e imagen en el historial. Sesión no oficial: escrituras por
  las rutas de Serein (ver
  [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#amigos-y-conversaciones-fuera-de-servidores-2026-10-17)):
  abrir y cerrar privados, renombrar, imagen y abandonar; la respuesta
  actualiza la lista de la sesión; un privado cerrado vuelve con su próximo
  mensaje. Crear grupos y añadir/quitar quedan apagados por capacidad: Serein
  no usa esas rutas (el REA del bundle web las confirma, si el dueño las
  quiere). «Dueño nuevo»: Discord no tiene tipo de mensaje para eso (enum de
  0 a 69); se ve en la corona. [Crear grupo](../docs/img/new-group-2026-10-17.png),
  [gestionar](../docs/img/group-icon-2026-10-17.png). Pendiente: prueba
  real de todo lo de la sesión no oficial; el apodo de un amigo todavía no
  se usa como título de su privado.
- [x] **11.3 (2026-10-17, rama `social-2`)**: sesión no oficial con
  `create_groups`, `group_members` y la capacidad nueva `group_owner`
  (`Command::SetGroupOwner`), por las rutas del cliente web oficial (REA
  estática de su bundle: `POST /users/@me/channels {"recipients"}`,
  `PUT`/`DELETE /channels/{id}/recipients/{usuario}`, `PATCH /channels/{id}
  {"owner"}`), validadas antes de salir (2 a 9 amigos, sólo amigos y con
  lugar, quitar y pasar sólo quien es dueño). `GroupAbilities::owner`;
  «Hacer dueño» con confirmación en «Gestionar grupo»; el Mock mueve la
  corona. El apodo de un amigo titula su privado (cabecera, lista y
  buscador), como Serein. Sólo fixtures.
  [Pasar la propiedad](../docs/img/group-owner-2026-10-17.png),
  [apodo como título](../docs/img/friend-nickname-title-2026-10-17.png).
  Pendiente: prueba real (lista en DISCORD_SESSION.md).
- Módulos: contratos específicos, Channel y perfiles/sidebar; crear
  `rune-state::conversations` y `rune-ui::conversations`.
- Dependencias: identidad y capacidades sociales de proveedor.
- Cierre verificable: iniciar privado repetido converge al canal confirmado;
  no se fabrica canal remoto; integrantes/nombre/icono/propiedad y salida
  reconcilian eventos. Listar privados no concede gestionar grupos.

### 12. `relationships` — Amigos, solicitudes y bloqueos

- [x] Modelar relaciones y completar listas/acciones sociales
  (2026-10-17; falta prueba real contra el servicio).
- Estado: 12.1–12.3 abajo; en la simulación y, sólo con fixtures, en la
  sesión no oficial.
- [x] **12.1 (2026-10-16)**: `rune-model::{Relationship, RelationshipKind}`
  (`since`/`nickname` con `serde(default)`); `Snapshot.relationships:
  Option<Vec<_>>` (`None` = no informado, distinto de vacío); eventos
  `RelationshipChanged`/`RelationshipRemoved`; comandos de solicitud,
  aceptación, eliminación y bloqueo; capacidades `relationships`,
  `friend_requests`, `block_users`. `rune-state::relationships` indexa por
  persona (repetidos/tardíos no duplican; un evento suelto no inventa la
  lista), `Store::social` y `friends_list` dan permisos con motivo, y bloquear
  quita escribir/reaccionar/adjuntar en su privado. Página limn «Amigos» en
  Inicio (En línea/Todos/Pendientes/Bloqueados/Añadir amigo, búsqueda, menú
  «más», confirmación para eliminar y bloquear). Mock: amigos, solicitudes en
  ambos sentidos y un bloqueo fijos (sin tocar el RNG), escrituras confirmadas
  al instante y etiquetadas como simulación. `rune-discord-session::social`
  lee READY y `RELATIONSHIP_ADD/UPDATE/REMOVE` (tipos 1–4, filas inválidas o
  spam descartadas, tope 10 000) sin escribir nada.
  [Captura](../docs/img/friends-2026-10-16.png). Pendiente: probar la lectura
  con una cuenta real, apodos editables, carpeta de spam y ocultar mensajes de
  personas bloqueadas en servidores. Con 11.1: formato, Clippy estricto y
  **1161 aprobadas, seis ignoradas, 25 suites** (todas las features).
- [x] **12.2 (2026-10-17, rama `social`)**: `Command::SetFriendNickname`,
  capacidades `friend_nicknames` y `friend_presence`, `Relationship.spam`
  (`serde(default)`). Sesión no oficial: solicitud por nombre de usuario,
  aceptar, ignorar/cancelar/eliminar/desbloquear, bloquear y apodo por las
  rutas de Serein; las confirma `RELATIONSHIP_*` (nada se fabrica desde la
  respuesta); rechazos con código (80004, 80001, 80007, 30002, 340007…) en
  español y hCaptcha informado, no resuelto; spam (`is_spam_request`) se
  conserva marcado; presencia de amigos desde READY, READY_SUPPLEMENTAL y
  `PRESENCE_UPDATE` sin servidor. UI: «Cambiar apodo» con diálogo, carpeta
  «Posible spam» aparte y fuera de la cuenta de pendientes, «En línea» con
  `presence` o `friend_presence` (sin ninguna, lo dice), y en el timeline los
  mensajes seguidos de alguien bloqueado se pliegan en «N mensajes
  bloqueados · Mostrar». Mock: una solicitud de spam y apodos. Sólo
  fixtures: falta probarlo con una cuenta real (lista en
  DISCORD_SESSION.md). Con 11.2: formato, Clippy estricto y **1574
  aprobadas, ocho ignoradas, 25 suites** (todas las features).
- [x] **12.3 (2026-10-17, rama `social-2`)**: el hCaptcha de una solicitud
  (o cualquier escritura social) ya no sólo se informa: la persona lo
  resuelve en la ventana aislada y se repite una vez (ver 13). El Mock pide
  una verificación simulada al mandarle una solicitud a `pablo`.
  [Captura](../docs/img/challenge-2026-10-17.png). El apodo titula el
  privado (ver 11.3). Sólo fixtures.
- Módulos: `relationships` en modelo, estado y UI; componentes puros de
  contactos, comandos/eventos/capacidades de relación.
- Dependencias: 11 para abrir nuevos privados y soporte social verificable.
- Cierre verificable: transiciones confirmadas de solicitud/amistad/bloqueo;
  eventos tardíos/repetidos no duplican; bloquear actualiza acciones; ausencia
  de soporte se distingue de una lista vacía. Simulación etiquetada.

### 13. `threads` — Hilos, foros y permisos

- [ ] Hilos, foros y escenarios implementados (2026-10-17) en la simulación y,
  en la sesión no oficial, **sólo con fixtures, sin probar contra el
  servicio real**. Falta la prueba real del dueño (abajo).
- Primer corte (2026-10-16): `ChannelKind::{Announcement, Forum, Thread}`,
  `Channel::thread` y `Channel::tags` con `#[serde(default)]`, hilos activos
  bajo su canal, foro con tarjetas, timeline y composer reutilizados,
  lectura de `THREAD_*` en el adaptador. Captura:
  [docs/img/threads-forums-2026-10-16.png](../docs/img/threads-forums-2026-10-16.png).
- Hecho (2026-10-17), por ítem:
  1. Escrituras en `rune-discord-session` (`http/threads.rs`,
     `bootstrap/threads.rs`): responder en hilos con
     `SEND_MESSAGES_IN_THREADS`; crear hilos desde un mensaje o del canal
     (públicos o privados); publicaciones de foro con título, etiquetas y
     primer mensaje; seguir, editar (nombre, cerrar, bloquear, etiquetas) y
     eliminar. Rutas de Serein y documentadas, validadas antes de salir y
     confirmadas por la respuesta o el Gateway; una respuesta ilegible queda
     ambigua y no se repite. Detalle en
     [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#hilos-y-foros-2026-10-17).
     Contrato: `Command::{CreateThread, CreatePost, LoadThreads,
     FollowThread, EditThread, DeleteThread}`, `Event::ThreadsLoaded`,
     `Failure::{NewThread, Thread, ThreadList}`, capacidades
     `create_threads`, `thread_pages`, `follow_threads`, `manage_threads`,
     `channel_links`. Permiso nuevo `CREATE_PRIVATE_THREADS` (bit 32).
  2. Interfaz (`rune-ui::threads`, `rune-components::threads` y `forum`):
     «Crear hilo» en la lista de hilos del canal y en el menú de un
     mensaje; «Nueva publicación» en el foro, con etiquetas (las moderadas
     sólo para la moderación y la obligatoria si el foro la pide). Los
     permisos se vuelven a preguntar al Store al confirmar; el diálogo espera
     al proveedor y abre el hilo cuando éste lo nombra.
  3. Páginas: abrir un foro o la lista de un canal pide una vez los hilos
     abiertos que la sesión no conoce (el Gateway de una cuenta personal sólo
     avisa de los seguidos); «Cargar más» sigue con archivados públicos y
     después privados (todos con `MANAGE_THREADS`, si no los propios), de a
     una página, sin duplicar aunque una página llegue repetida o tarde; una
     página nunca pisa un hilo conocido y `THREAD_LIST_SYNC` ya no borra los
     archivados.
  4. Seguir y dejar de seguir desde el menú; la columna cuelga bajo cada
     canal los hilos seguidos (y el abierto) y la lista del canal los separa
     en «Seguidos», «Activos» y «Archivados».
  5. Responder en un hilo archivado y no bloqueado se permite y lo reabre
     (Store, simulación y sesión); escribir el aviso, reaccionar o votar ahí,
     no; uno bloqueado sólo acepta a la moderación.
  6. Un `MessageCreated` nuevo suma uno al contador del hilo y mueve su
     última actividad; uno repetido no cuenta.
  7. Escenarios: se entra al público, se levanta o baja la mano, se acepta
     o rechaza una invitación y se vuelve al público; la moderación
     (`MUTE_MEMBERS`) invita o pasa al público desde el menú de la persona.
     `VoiceState::{suppressed, speak_request}` y `StageSeat`, permiso
     `REQUEST_TO_SPEAK` (bit 33), `Command::{RaiseHand, SetSpeaker}`,
     capacidad `stages`. Rutas del cliente web oficial (REA de su bundle;
     Serein no tiene). Cubre lo pendiente de «pedir la palabra» del 17.
     Captura: [docs/img/stage-2026-10-17.png](../docs/img/stage-2026-10-17.png).
  8. Foro: orden por actividad o creación, lista o galería (la que pida el
     foro hasta que la persona elija) y filtro por etiquetas; las archivadas
     cargadas van después. Captura:
     [docs/img/forum-2026-10-17.png](../docs/img/forum-2026-10-17.png).
  9. Menú contextual de hilos y publicaciones: seguir, marcar como leído,
     copiar enlace (si el proveedor tiene), editar nombre y etiquetas,
     cerrar/abrir, bloquear/desbloquear y eliminar con confirmación, cada uno
     con su motivo cuando no se puede. Captura de la lista:
     [docs/img/threads-2026-10-17.png](../docs/img/threads-2026-10-17.png).
- Simulado (`mock/threads.rs`, `mock/stage.rs`): foro con 3 publicaciones
  abiertas y 8 archivadas, `#galería` con 2 hilos abiertos y 5 archivados
  (uno privado propio, uno ajeno), de a 5 por página; crea, sigue, cierra,
  bloquea, renombra, etiqueta y borra según los permisos; una respuesta
  reabre. Un escenario con dos oradores y tres en el público; la moderación
  simulada invita a hablar un momento después de levantar la mano.
- **2026-10-17 (rama `social-2`), hCaptcha en cualquier escritura**
  (decisión del dueño): contrato `Event::ChallengeRequired { challenge,
  failure }` y `Command::SolveChallenge { challenge, solve }` con
  `CHALLENGE_LIFETIME` (5 min); Core corre el plazo del mensaje o la
  subida; pregunta limn «Verificación requerida»/«simulada» con
  «Verificar»/«Cancelar» (`rune-ui::challenges`, `rune-components::challenge`).
  Sesión: `request_answer` marca la ruta de la escritura frenada; el
  reintento va una vez con `X-Captcha-Key`/`-Rqtoken`/`-Session-Id` sólo en
  esa ruta; cancelar, cerrar, vencer, perder la conexión o un segundo
  desafío terminan con motivo. Cubre mensajes, subidas, amigos, privados y
  grupos, hilos y publicaciones, escenarios y perfil; las invitaciones
  comparten las cabeceras y una sola ventana a la vez. Sólo fixtures.
  Adjuntos en una publicación nueva: hechos (ver 07); crear un hilo no
  tiene primer mensaje en RUNE, así que no aplica.
- **Hecho (2026-10-18, rama `hilos-foros-y-escenarios`)**, sólo fixtures y
  simulación, sin probar contra Discord:
  1. Encabezado angosto: por debajo de 800 px la bandeja y la lista de
     hilos se pliegan en un menú «más» (`cl:more`,
     `channel_menu::more`); cualquier clic lo cierra.
  2. Un `Ready` nuevo de la misma cuenta conserva los hilos leídos por
     página que no nombra (bajo un padre visible, con sus permisos); uno
     abierto que seguía se va. Lo hacen el Store y también la sesión no
     oficial en su propia instantánea (`bootstrap::keep_paged_threads`), así
     el historial, escribir, reabrir y los eventos del Gateway de esos hilos
     siguen andando (corrección del 2026-10-18, test de sesión). El foro
     abierto (que `on_ready` vuelve a abrir) y la lista de hilos abierta
     vuelven a pedir la primera página.
  3. Archivados con el instante del cursor incluido y los repetidos
     descartados por id (`ArchiveCursor`): ya no se saltean los archivados
     en el mismo instante (salvo más de 100 en uno solo, *atajo*).
  4. Tarjeta de publicación: comienzo del primer mensaje, su imagen y la
     reacción más dada o la predeterminada sin número (REA del cliente
     oficial); de `first_messages`, como Serein. Captura:
     [docs/img/forum-cards-2026-10-18.png](../docs/img/forum-cards-2026-10-18.png).
  5. Silenciar un hilo que se sigue: `Command::MuteThread`,
     `Capabilities::mute_threads`, `Thread::muted_until`,
     `ThreadAbilities::mute`; ruta de ajustes del miembro confirmada por
     REA; menú del hilo con duraciones; alertas y no leídos lo respetan.
     Las tarjetas del foro y la lista de hilos marcan no leídos como la
     columna marca ese hilo (silencio y nivel propio o heredado: con «Sólo
     menciones» no marcan; test). Con el hilo abierto, la campana del encabezado lo muestra silenciado y
     ofrece «Dejar de silenciar el hilo»; dejar de seguirlo termina el
     silencio. Captura: [docs/img/thread-mute-2026-10-18.png](../docs/img/thread-mute-2026-10-18.png).
  6. Chat de un escenario: el del canal de voz (`voice_text`), en el Store,
     la sesión y la simulación. Captura:
     [docs/img/stage-chat-2026-10-18.png](../docs/img/stage-chat-2026-10-18.png).
  7. Simulación: un hilo toma siempre los permisos de su padre.
  Detalle en [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#silenciar-hilos-tarjetas-de-foro-y-chat-de-escenarios-2026-10-18).
- Pendiente: prueba real (abajo); un escenario sin DAVE si Discord no lo
  negocia ahí; la tarjeta de las publicaciones archivadas (el cliente
  oficial pide `/channels/{foro}/post-data`; RUNE todavía no); la tarjeta no
  se actualiza si el primer mensaje cambia después; un hilo silenciado
  tampoco avisa de menciones (igual que un canal silenciado en RUNE; el
  oficial sí las marca); no hay captura del encabezado angosto (las capturas
  van a 1280 px; lo cubre un test). El comienzo de la tarjeta muestra el
  texto como está escrito (`**negrita**`, `<@id>`, `<:emoji:id>`), igual
  que el fragmento de una respuesta y las notificaciones: falta un texto
  plano compartido con menciones y emoji resueltos. El «más» abierto queda
  abierto, sin verse, si la ventana se agranda a 800 px o más, hasta el
  próximo clic (*atajo*: la `Ui` no conoce el ancho). Volver a oír un hilo
  manda sólo `{"muted":false}`, como el cliente oficial según la REA; falta
  confirmar que un silencio con fin no vuelve.
- Para probar contra el servicio (dueño): crear un hilo desde un mensaje y
  otro privado; seguirlo y dejarlo; escribir; cerrarlo, responder y ver que
  se reabre; renombrar; abrir un foro y ver publicaciones no seguidas; crear
  una publicación con etiquetas; «Cargar más» hasta los archivados públicos y
  privados; filtrar, ordenar y cambiar la vista; con moderación, bloquear y
  eliminar. En un escenario: entrar, levantar la mano, aceptar una
  invitación, hablar y volver al público; con moderación, invitar a alguien
  y pasarlo al público. Desde `social-2`: una publicación con uno y varios
  archivos (y sin texto); y, si Discord pide hCaptcha en alguna escritura,
  resolverlo, cancelarlo y cerrar la ventana. Desde
  `hilos-foros-y-escenarios`: silenciar un hilo seguido 15 min y para
  siempre, ver que no avisa ni marca no leídos y que otro cliente lo muestra
  silenciado, y dejar de silenciarlo; las tarjetas de un foro con imagen,
  texto y reacción (y la predeterminada sin reacciones); «Cargar más» en un
  canal con muchos hilos archivados a la vez (sin saltos ni repetidos);
  cortar la red con un hilo archivado abierto y ver que sigue al volver; el
  chat de un escenario (leer y escribir); la ventana a menos de 800 px.

### 14. `rich-messages` — Contenido enriquecido

- [x] Añadir embeds, reenvíos, encuestas y más mensajes de sistema
  (2026-10-17; falta prueba real contra el servicio).
- Estado: 14.1–14.7 abajo, sobre Markdown, respuestas, reacciones,
  adjuntos, stickers y referencias; spoilers de texto y de tarjetas.
- [x] **14.1 (2026-10-15)**: `rune-model::{Embed, EmbedField, Forwarded}` con
  `serde(default)` (sin cambio de `SCHEMA`); `MessageKind` suma alta/baja de
  grupo, llamada, renombre, icono, fijado, boost con nivel, hilo creado y
  `Other(n)` con fallback visible. `rune-discord-session::rich` mapea `type`,
  acota embeds (10, límites de la API pública, sólo enlaces http(s)), descarta
  los malformados sin perder el mensaje y lee `message_snapshots`. **Arreglo**:
  un reenvío (`message_reference.type = 1`) ya no se muestra como respuesta.
  `MESSAGE_UPDATE` con sólo `embeds` (vista previa tardía) se aplica como patch.
  Tarjeta limn con franja de color, sitio, autor, título enlazado, Markdown,
  campos inline adaptativos y pie con fecha relativa; reenvío con regla y
  «Reenviado»; líneas de sistema que envuelven. Mock: tarjetas simuladas al
  3 % con RNG separado. `--capture-rich` y
  [captura](../docs/img/rich-messages-2026-10-15.png). **1133 aprobadas, seis
  ignoradas**, 25 suites; formato y Clippy `-D warnings` correctos. No probado
  contra el servicio real.
- [x] **14.2 (2026-10-15)**, con Serein como referencia (sin copiar código):
  imágenes, miniaturas y póster de vídeo (▶ abre la página) cargadas **sólo**
  por el proxy de Discord (`media`/`images-ext-1/2`, `/external/`), nunca por
  la URL del tercero; tamaño acotado como un adjunto y reservado antes de
  cargar. Un embed `image`/`gifv` se dibuja como imagen sola y oculta el
  enlace si el texto es sólo ese enlace (sin spoiler). Respeta «vistas previas
  de imágenes». Presupuesto total de 64 KiB por mensaje. Menciones y emoji en
  descripción/campos resueltos con `text_references` propias (no notifican).
  Encuestas `Poll` de sólo lectura: barras, voto propio, cierre restante,
  multiselección; resultados ausentes ≠ cero y un `MESSAGE_UPDATE` sin
  resultados conserva los conocidos (`retain_results`, idea de Serein). Tipo 46
  → `MessageKind::PollResult` con ganador y porcentaje. `--capture-rich` suma
  imagen y encuesta: [captura](../docs/img/rich-polls-2026-10-15.png).
  **1138 aprobadas, seis ignoradas**, 25 suites; formato y Clippy `-D warnings`
  correctos. No probado contra el servicio real.
- [x] **14.3 (2026-10-16)**, con Serein como referencia (sin copiar código):
  votar y reacciones/votos en vivo. `Command::VotePoll` (respuestas exactas;
  vacío quita el voto), `Failure::Vote`, capacidad `poll_votes` y
  `Abilities::vote` (VIEW + READ_HISTORY). Eventos de cambio
  `ReactionChanged`, `ReactionsCleared` y `PollVoteChanged` con `me`; el
  `Store` los aplica a la base confirmada y rebasa encima lo optimista
  (`rebase_local` también para el voto), así el eco propio cuenta una vez y
  un rechazo restaura la base con los votos ajenos llegados mientras tanto.
  Deduplicación: el Gateway ya descarta secuencias vistas (un resume sólo
  reenvía lo posterior) y el adaptador recuerda el último cambio por
  (canal, mensaje, usuario, emoji+burst | respuesta), acotado a 4096 y
  reiniciado en READY; REMOVE_ALL/REMOVE_EMOJI lo olvidan. Reacciones con
  emoji propio se ignoran como en `wire::reactions`; un cambio mal formado se
  descarta sin cortar la conexión. Interfaz: con permiso y encuesta abierta las
  respuestas son opciones (selección única vota al pulsar; multiselección
  marca y «Votar»), resultados tras votar o al terminar, «Quitar voto»; sin
  permiso, resultados y motivo. Mock: votación y reacciones/votos ajenos con
  RNG propio, encuesta simulada en #general. Sesión: `PUT
  /channels/{c}/polls/{m}/answers/@me` (204) por la misma cola, revalidación
  y único intento que las escrituras de mensajes, sólo sobre una encuesta
  cargada, abierta y con respuestas válidas; capacidad activada.
  [Captura](../docs/img/poll-voting-2026-10-16.png) (ventana estrecha por el
  mosaico del gestor de ventanas). **1154 aprobadas, seis ignoradas**, 25
  suites; formato y Clippy `-D warnings` correctos. **No probado contra el
  servicio real.**
  Atajos: sin plazo propio en core (como reacciones y ediciones; el HTTP de
  sesión vence a los 15 s y revierte); los cambios en vivo no se guardan en
  SQLite (la caché se corrige al recargar historial) ni invalidan una carga
  de contexto en curso. Resuelto el 2026-10-18 (rama
  `reacciones-fijados-y-busqueda`): el voto tiene plazo en core y los
  cambios en vivo se guardan e invalidan el contexto; reacciones y
  ediciones siguen sin plazo propio en core.
- [x] **14.4 (2026-10-16)**, decisión del dueño: **mostrar y reproducir**
  mensajes de voz (sin grabar ni enviar). REA del cliente web y Serein como
  referencia, sin copiar código ([REA.md](../docs/REA.md#cliente-web-oficial-mensajes-de-voz-2026-10-16)).
  `rune-model::VoiceClip` en `Attachment::voice` con `serde(default)` (sin
  cambio de `SCHEMA`). `wire` lo reconoce por `duration_secs` con límites
  (0–3600 s, forma de onda base64 ≤ 256 bytes); fuera de límites queda como
  archivo y una forma de onda inválida se dibuja plana, sin perder el
  mensaje. `rune-core::clips` descarga a memoria sólo al pulsar (≤ 8 MiB,
  orígenes validados, reutiliza el `fetch` de los PDF). `rune-media::clip`
  lee Ogg con un lector propio y decodifica con el Opus de las llamadas, a
  ritmo de reloj en el `Feed` de la salida elegida, sin bloquear la UI.
  Componente puro en `rune-components::message::voice` (reproducir/pausar,
  barras con las medidas del cliente oficial que se iluminan y se arrastran
  para buscar, tiempo restante), estado en `Ui::voice_player`, pedidos con
  `Intent::{PlayVoice, StopVoice}`, IDs `attachment:{mensaje}:{archivo}:voice`
  y `:seek`. Uno por vez; cambiar de conversación o cuenta lo detiene. Un
  mensaje de voz no se edita y una respuesta lo cita como «Mensaje de voz».
  Mock: un mensaje de voz **simulado** en #general que reproduce
  `assets/voice-sample.ogg` (tono sintetizado). `--capture-voice` lo
  reproduce en la salida real y
  [captura](../docs/img/voice-message-2026-10-16.png) el reproductor
  avanzando. **1450 aprobadas, seis ignoradas**, 25 suites; formato y
  Clippy `-D warnings` correctos. **No probado contra el servicio real**;
  el audio no se escuchó (sólo se vio avanzar la posición con la salida
  real abierta). No hecho: Opus con más de dos canales, CRC de páginas Ogg
  (hechos el 2026-10-18, ver «Audio de mensajes» al final).
  - Completado el mismo día: **velocidad** 0,75/1/1,5/2× sin cambiar el
    tono (WSOLA propio en `rune-media::stretch`, botón «Velocidad 1,5×» con
    ID `attachment:{mensaje}:{archivo}:speed`, una velocidad para la
    sesión), **posición recordada por clip** (hasta 64 en `Ui`, se olvidan al
    cambiar de cuenta o cerrar sesión) y **reproducir durante una llamada**:
    el clip se suma a la salida de la llamada en su motor
    (`CallMedia::play_alongside`), nunca al micrófono; si la llamada empieza
    o termina en medio, sigue desde donde iba en el otro destino.
    `--capture-voice` lo reproduce a 1,5× en la salida real
    ([captura](../docs/img/voice-speed-2026-10-16.png)). **1476 aprobadas,
    ocho ignoradas** (con `--all-features`), 25 suites; formato y Clippy
    `-D warnings` correctos. **No escuchado** (ni estirado ni mezclado), sin
    llamada real. No hecho: el reproductor de un clip sin empezar no
    muestra la posición recordada (la retoma al pulsar); en filas de menos
    de unos 170 px el botón de velocidad no se muestra (hechos el
    2026-10-18, ver «Audio de mensajes» al final).
- [x] **14.5 (2026-10-16)**, con REA estática del cliente web oficial
  ([REA.md](../docs/REA.md#cliente-web-oficial-resultados-de-encuestas-tarjeta-de-voz-y-componentes-2026-10-16))
  y Serein como referencia (sin copiar código); sólo para mostrar:
  - Resultado de encuesta: `EmbedKind::PollResult` y `Embed::field`; la
    línea del sistema más una tarjeta como la del cliente (emoji del ganador,
    respuesta y «Respuesta ganadora • N %», empate o «No hubo un ganador»)
    y «Ver encuesta», que salta a la encuesta por `reply_to` con el
    `open_message` existente. Sin tarjeta genérica repetida.
  - Tarjeta `voice_channel`: `EmbedKind::VoiceChannel` y `Embed::channel`;
    «Unirse» usa el `join` de voz existente sólo si `Store::abilities` lo
    permite, si no queda deshabilitado con el motivo. **Campos no
    confirmados**: el renderizador oficial no estaba en los bundles; el canal
    sale sólo de un enlace `discord.com/channels/{s}/{c}`.
  - Componentes (`rune_model::components`, `Message::components`,
    `Embed::components`, todo con `serde(default)`, sin cambio de `SCHEMA`):
    filas, botones (los de enlace abren por el mismo manejador de enlaces;
    los demás y los menús deshabilitados con «RUNE no envía interacciones a
    aplicaciones»), y v2 (`IS_COMPONENTS_V2`): contenedor con color y
    spoiler, sección con accesorio, texto Markdown con menciones resueltas,
    miniatura, galería y archivo (imágenes sólo por el proxy de
    `rich::media`; un archivo v2 usa la tarjeta del adjunto, que no se
    repite), separador. Embed `components` igual. Acotado: 40 partes,
    profundidad 4, 16 KiB de texto; tipo desconocido con línea visible;
    malformado se descarta sin perder el mensaje. IDs `msg:{id}:component:{n}`.
  - Mock: encuesta cerrada y su resultado, tarjeta de voz y un mensaje v2 en
    `#general`, etiquetados como simulados (`mock/showcase.rs`, flujo propio).
    `--capture-rich` los suma:
    [captura](../docs/img/rich-components-2026-10-16.png) (Xvfb privado
    1280×800). **1444 aprobadas, seis ignoradas**, 25 suites; formato y
    Clippy `-D warnings` correctos. **No probado contra el servicio real.**
  - Atajos: las vistas previas de los archivos de un mensaje v2 se piden con
    la fila aunque sólo se muestren por sus partes. Los botones de enlace
    llevan `Icon::ExternalLink` al final
    ([limn PR #69](https://github.com/Niiihuel/limn/pull/69), integrada en
    `rune/integration-2026-10-16-color-emoji`).
  - Cierre de huecos (2026-10-16, segunda pasada de REA en
    [REA.md](../docs/REA.md#segunda-pasada-voice_channel-menús-con-valores-y-emoji-de-componentes-2026-10-16)):
    - Cada spoiler de las partes (contenedor, miniatura, ítem de galería,
      archivo) se revela solo, con `msg:{id}:component:{n}:reveal`; `n` es su
      orden de dibujo (los de una caja oculta cuentan) más la base de su
      lista (`first_spoiler`: partes propias, cada tarjeta, el reenvío y sus
      tarjetas). El estado sigue en `Ui::messages.revealed`, desde
      `PARTS_SPOILER`; `components::pictures` pide sólo las imágenes de los
      revelados.
    - Reenvíos: `Forwarded::components` (`serde(default)`) desde
      `message_snapshots[0].message` con sus límites y flags, dibujados dentro
      del reenvío (un v2 muestra sus archivos sólo donde lo dicen sus
      partes). Como sus archivos, una copia no pide imágenes de los archivos
      del original: queda su descripción; una imagen de página por
      `/external/` del proxy sí carga.
    - Emoji propios como imagen (la fija del selector, `:nombre:` mientras
      carga o si falla) en el ganador de un resultado de encuesta (miniatura
      de la tarjeta) y en las opciones `default` de un menú de texto, que
      ahora se muestran en lugar del placeholder como en el cliente
      (`Select::chosen`, `serde(default)`). Se piden como los stickers, aun
      sin vistas previas.
    - Botones con emoji propio: su imagen antes de la etiqueta (sola si no
      tiene etiqueta), `:nombre:` mientras carga o si falla
      (`Button::picture`, `serde(default)`), con `limn::Button::image`
      ([limn PR #71](https://github.com/Niiihuel/limn/pull/71), integrada en
      `rune/integration-2026-10-16-color-emoji`).
    - `voice_channel`: el cliente no lo dibuja ni lee sus campos (5879 + 279
      chunks revisados); queda el enlace de canal. **Sin confirmar.**
    - [Captura del reenvío](../docs/img/rich-forward-spoilers-2026-10-16.png)
      con un spoiler oculto y otro revelado, y la de arriba actualizada (menú
      con opción elegida). **No probado contra el servicio real.**
    - No hecho: valores por defecto de menús 5–8 (requieren
      resolver usuarios, roles y canales). Los emoji animados se mueven
      desde el 2026-10-19 (`limn-teclado-y-superficies`).
- [x] **14.6 (2026-10-17)**, rama `mensajes-y-visores`, con Serein como
  referencia de comportamiento (sin copiar código); lo que quedaba de 14:
  - **Galería**: las tarjetas que siguen a otra con el mismo enlace y sólo
    suman una imagen se dibujan dentro de la primera
    (`rune_model::card_groups`): 2 lado a lado, 3 con una alta, 4 en
    cuadrado, recortadas a su celda; hasta 4 como el cliente oficial.
  - **Visor**: pulsar una imagen de tarjeta (sola, de galería, miniatura o
    GIF) abre el visor de adjuntos, ahora de un archivo o de una imagen de
    tarjeta (`rune_ui::Viewed`), que pagina la galería con flechas. El
    póster de un video sigue abriendo su página.
  - **`gifv`**: `Embed::video` con el MP4 de `video.proxy_url`
    (`ImageKind::Clip`, sólo por el proxy); el núcleo lo convierte en
    cuadros de animación con OpenH264 y los presupuestos de un GIF; anima
    donde animan los GIF y se ve en el visor.
  - **Spoilers en tarjetas**: el texto `||…||` de descripción, campos,
    partes de apps y texto reenviado queda oculto hasta pulsar cada uno
    (`Spoilers::first` numera los de cada tarjeta); una tarjeta cuyo enlace
    el mensaje escribió entre `||` se tapa entera y una con spoiler en su
    texto oculta sus imágenes (`Embed::spoiled`/`hides_pictures`), sin pedir
    píxeles antes del revelado (`card_spoiler`).
  - **Reenvíos**: los archivos de `message_snapshots` conservan su dirección
    (canal de origen, también la forma `/attachments/{c}/{m}/{a}/{nombre}`)
    y se ven, reproducen, abren en el visor o como PDF y guardan como los
    propios (`Message::files`).
  - **Texto de tarjetas** en la búsqueda local y en el extracto de una
    respuesta a un mensaje sin texto (`rune_model::card_words`).
  - `--capture-media`: [tarjetas](../docs/img/rich-cards-2026-10-17.png) y
    [visor de la galería](../docs/img/card-viewer-2026-10-17.png). Sin cambio
    de `SCHEMA` (`Embed::video` con `serde(default)`). **1644 aprobadas,
    nueve ignoradas** (con `--all-features`), 25 suites; formato y Clippy
    `-D warnings` correctos. **No probado contra el servicio real.**
  - Pendiente: ~~un enlace entre `||` en el texto del mensaje se dibuja con
    sus `||` visibles~~ (resuelto el 2026-10-18, rama `limn-texto`: el
    Markdown de limn metía el `||` de cierre en el enlace); interactuar con botones y menús
    de apps es otro trabajo; valores por defecto de menús 5–8 y emoji
    animados siguen como antes.
- [x] **14.7 (2026-10-17)**, rama `comandos-e-interacciones`, decisión del
  dueño del 2026-10-17 (reemplaza «componentes sólo visibles»): los
  componentes de los mensajes de apps se **usan**. Botones (no los de
  enlace ni de compra) y menús de texto, personas, roles, canales y
  menciones mandan su `custom_id` y valores a la app
  (`Interaction::Part`, capacidad `interactions`, `Store::interaction_abilities`);
  los menús 5–8 muestran sus **valores por defecto** resueltos por nombre y
  ofrecen personas, roles o canales del servidor con búsqueda (25 a la vez,
  tipos de canal respetados); con varios, se marcan y se mandan al cerrar.
  Mientras espera, el control pulsado gira y los demás se deshabilitan
  («Enviando a la aplicación…» / «La aplicación está pensando…»); un error
  se dice bajo las partes del mensaje, no en un aviso; a los 30 s RUNE deja
  de esperar. Modelo: `components::Select` (antes variante con campos),
  `Button::custom_id`, `Components::{app, flags, ephemeral}`, todo
  `serde(default)`: las filas viejas se leen igual (test). Una copia
  reenviada, una muestra o un mensaje sin app siguen mostrando «Estos
  controles no se pueden usar desde aquí». Detalle y lo demás de P3 en
  [Paridad P3](#paridad-p3-comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones).
  Emoji animados siguen como antes.
- Módulos: Message/MessageKind, componentes y adaptadores; `rich_content`,
  `embeds` o `polls` al definir cada contrato, incluidos resultados interactivos.
- Dependencias: política de previews y capacidad específica de cada contenido.
- Cierre verificable: filas antiguas deserializan con `serde(default)`;
  contenido desconocido conserva fallback; sin HTML/scripts ejecutables;
  acciones sin soporte se deshabilitan con motivo y edición conserva fuente.

### 15. `profiles` — Perfiles, miembros y restricciones

- [x] Restricciones de miembro (timeout, verificación pendiente, cuarentena) en
  modelo, estado, adaptador no oficial, simulación y UI (2026-10-16).
- [x] Perfil por demanda (`LoadProfile`/`ProfileLoaded`) con guardia de petición
  y alcance; desconocido ≠ ninguno; perfil completo en modal limn.
- [x] Perfil ampliado remoto en el adaptador no oficial (2026-10-16):
  biografía, pronombres, banner/color, apodo/biografía/banner del servidor,
  fecha de ingreso y servidores/amigos en común (desconocido ≠ ninguno),
  por la ruta que usa Serein; sólo fixtures.
- [x] Lista de miembros por suscripción de Gateway (op 37,
  `GUILD_MEMBER_LIST_UPDATE`) con grupos y cuentas, rangos según el scroll,
  cambios incrementales y salida al cambiar de canal/servidor (2026-10-16);
  sólo fixtures.
- [x] Edición confirmada con medios (2026-10-17): avatar, banner, nombre,
  pronombres, «Sobre mí» y color, y el perfil por servidor (apodo, avatar,
  banner, «Sobre mí»). Selector de archivos del sistema, recorte al centro
  (512×512 / 960×384), PNG ≤ 1 MiB, vista previa; `profile_images` y
  `server_profiles`; `Store::profile_abilities` con motivos; usuario, correo
  y contraseña deshabilitados (piden la contraseña). Sesión no oficial por
  `PATCH /users/@me`, `/users/@me/profile`, `/guilds/{g}/members/@me` y
  `/guilds/{g}/profile/@me` (Serein y REA del bundle), con lectura posterior;
  hCaptcha informado. Simulación completa. Sólo fixtures.
  [Detalle](../docs/PROFILES.md#edición-con-imágenes-y-perfil-por-servidor-2026-10-17).
- [x] Avatar por servidor (`Member.avatar`, `#[serde(default)]`) en mensajes,
  miembros, tarjeta y perfil completo (`Store::avatar_in`) (2026-10-17).
- [x] Cuarentena por etiqueta de servidor: bit 10 de `flags`
  (`AUTOMOD_QUARANTINED_SERVER_TAG`) → `tag_quarantined` con su motivo
  (2026-10-17).
- [x] Lista de miembros: prueba de extremo a extremo de la resuscripción
  tras `READY` y reintento si no llega `SYNC` (15/30/60/120 s) con prueba
  (2026-10-17).
- [x] Edición de perfil (2026-10-18, rama `edicion-de-perfil`): encuadre
  manual (arrastrar sobre la imagen entera con `area_slider`, zoom con
  `Slider` hasta 4×, recorte de nuevo en `rune-core` al soltar, `Crop` en
  `rune-provider`); GIF/WebP animados subidos como GIF con el mismo
  encuadre en cada cuadro (siempre hecho de nuevo, sin los metadatos del
  archivo) cuando el plan lo permite, el primer cuadro con el motivo si no o
  si la animación pasa de 1000 cuadros, 20 s por tamaño u 8 MiB; `Perks` y
  `profile_abilities` avisan antes de que Discord rechace (avatar animado
  con Classic o Nitro; banner y perfil por servidor sólo con Nitro, REA del
  catálogo del bundle), también si el plan llega después de elegir
  («Guardar» dice por qué); la simulación guarda y sirve las imágenes
  guardadas (`Provider::hosted_image`, con su propio candado y tope de
  8 MiB) y su plan no tiene avatar animado, para que el aviso se vea. Sólo
  fixtures. Corrección del 2026-10-18 (rama `edicion-de-perfil-fix`).
  Pendiente: una capacidad propia para imágenes de perfil animadas
  (hoy la cubre `profile_images`, cuyo `ProfileImage` ya admite GIF).
  [Detalle](../docs/PROFILES.md#encuadre-imágenes-animadas-y-plan-2026-10-18-rama-edicion-de-perfil),
  captura `docs/img/profile-framing-2026-10-18.png`.
- [ ] Prueba real de perfil, edición y lista contra Discord (el dueño).
- Estado: `MemberRestrictions` y `Member.joined` (`#[serde(default)]`) viajan
  por `Member`. `Store::abilities` y `server_abilities` niegan participar con
  motivo mientras dure; el reloj del `Store` (`clock`, inyectable en tests)
  levanta el timeout sin eventos y `restriction_expires` agenda el repintado
  (al vencer y cada minuto, para el «quedan N min» del compositor). Lectura y
  borrar lo propio siguen permitidos. El adaptador no oficial mapea
  `communication_disabled_until`, `pending` y el bit 7 de `flags`, conserva lo
  omitido en actualizaciones parciales, ya no omite miembros restringidos y su
  compuerta REST rechaza escrituras restringidas. `LoadProfile` va por red
  desde el 2026-10-16 (ver abajo). La simulación aísla a la
  cuenta 95 min en Café Nocturno y a otra persona en Taller de Rust, deja a
  otra pendiente de verificación, fecha membresías y calcula datos en común
  con semillas propias. Tarjeta: insignia de restricción, «Miembro desde»
  (cuenta y servidor), resumen en común y «Ver perfil completo»; lista de
  miembros: reloj tras el nombre; compositor: «Estás aislado hasta … · quedan
  …». Captura: `docs/img/profiles-2026-10-16.png`.
- Perfil y lista (2026-10-16): `LoadProfile` es un GET a
  `/users/{id}/profile` por la cola REST (256 KiB, persona/servidor
  validados, una tarjeta a la vez); `WatchMemberList`/`UnwatchMemberList`
  llevan a op 37 con ritmo propio (20 tramas/min, 500 ms) y las operaciones
  de lista se aplican en el adaptador; `MemberListChanged` lleva grupos,
  cuentas y filas al `Store`; el panel muestra filas de espera. Capacidades
  `profiles` y `member_list` habilitadas; `presence` no. Detalle en
  `docs/DISCORD_SESSION.md` y `docs/PROFILES.md`.
- Pendiente/no probado: nada se probó contra Discord (perfil, edición y
  lista sólo con fixtures en loopback); con el plan desconocido, el banner y
  el perfil por servidor quedan a lo que diga el servicio; la vista previa
  del editor no anima el GIF elegido (primer cuadro) y una animación de más
  de 4 MP de lienzo se encuadra quieta; el archivo elegido queda en memoria
  hasta la próxima elección; el ícono animado del servidor espera a la orden
  28 (`animated: false`); presencia sólo
  de filas cargadas; el panel muestra hasta 50 000 filas (límite de la lista
  medida de limn); el texto largo del compositor se trunca en ventanas
  angostas. Capturas: `docs/img/profile-pictures-2026-10-17.png`,
  `docs/img/server-profile-2026-10-17.png`,
  `docs/img/profile-framing-2026-10-18.png`.
- Módulos: User/Member, `MemberRestrictions`, consultas de core, perfiles,
  roster y `Store::abilities`.
- Dependencias: 07 para avatar/banner y consultas de perfil/miembros soportadas.
- Cierre verificable: timeout/verificación impiden acciones con motivo/vigencia;
  respuesta obsoleta no cambia el perfil actual; desconocido no se presenta
  como ausencia confirmada. Perfiles por servidor y datos mutuos con alcance
  explícito; edición de medios reutiliza cargas.

### 16. `media-viewers` — Animación, reproducción y documentos

- [x] Completar animación, audio/vídeo y documentos multipágina (2026-10-17;
  falta sólo probarlo contra el servicio real y los códecs de abajo).
- [x] 16.1 GIF/WebP animados en el timeline (adjuntos y stickers), 2026-10-16:
  cuadros decodificados en `rune-core` con límites de lienzo, cuadros, tiempo
  y memoria (16 MiB por animación, 64 MiB en total, luego vuelven al primer
  cuadro); se presentan en superficies limn. Pausan con movimiento reducido,
  visor de PDF abierto, ventana minimizada o fila fuera de vista; cerrar sesión
  libera cuadros y texturas. Probado con tests y captura
  `--capture-animation` (cuadros locales; en `#general` hace falta
  `--action jump:latest`, porque la vista queda en «NUEVO»); no se probó con un GIF real de
  Discord ni se midió CPU.
- [x] 16.5 Adopción de limn PR #66 (2026-10-16): la superficie se ajusta con
  `SurfaceFit::Contain` y el radio de la caja (sin densidad calculada en el
  host); cada cuadro se convierte una vez y su `SurfaceFrame` se clona en cada
  vuelta; el visor de adjuntos reproduce el GIF con `ImageViewer::surface`
  (con el visor abierto sólo juega esa imagen); las flechas del visor de PDF
  dicen «Página anterior»/«Página siguiente». Probado con tests y captura;
  `Contain` agrandaba un GIF pequeño en el visor más allá de su cuadro
  quieto: resuelto el 2026-10-18 con `SurfaceFit::ScaleDown` (paquete 10,
  ver LIMN_INTEGRATION.md).
- [x] 16.2 PDF multipágina, 2026-10-16: páginas bajo demanda con el mismo
  sandbox, total por `pdfinfo` aislado, cancelación al cambiar de página y
  4 páginas en caché; el PDF vive en RAM solo con el visor abierto. Test
  ignorado real (Nix withDocs) verificó 2 páginas y la página 2.
- [x] 16.3 Spoilers de adjuntos (`SPOILER_`/flag 8), 2026-10-16: tarjeta sin
  píxeles hasta «Hacé clic para mostrar»; sin preview, visor ni PDF antes.
- [x] 16.4 Audio y video (2026-10-17, rama `mensajes-y-visores`), con
  Serein como referencia (sin copiar código): reproductor en la línea
  (reproducir/pausar, barra de tiempo que busca al soltar, `m:ss / m:ss`,
  silenciar y volumen) y video a ventana completa (`Modal` de limn, Espacio
  y Esc). Sonido MP3, M4A/AAC, Ogg Vorbis/Opus, WAV, FLAC y WebM de sólo
  sonido con **Symphonia** (dependencia nueva, Rust puro, justificada en su
  `Cargo.toml`; Opus por el libopus de las llamadas), mezclado a mono y
  remuestreado; video MP4 H.264 (Baseline/Main/High con B-frames) con un
  lector MP4 propio y acotado (`rune_media::mp4`) y el OpenH264 de las
  transmisiones, en un hilo que sigue el reloj del sonido. Es el mismo
  `rune_media::Clip` de los mensajes de voz (uno a la vez, también durante
  una llamada). Hasta 50 MiB en memoria, sólo del host de archivos de
  Discord. VP8/VP9/AV1 (WebM/MKV) se suman el 2026-10-18 (abajo); H.265 y
  archivos más grandes muestran el motivo con «Abrir en el navegador» y
  «Guardar como…». Un video fuera de
  vista se pausa (salvo a ventana completa); nada se reproduce solo; cambiar
  de conversación o de cuenta y cerrar sesión lo detienen y liberan archivo
  y superficie. Simulación: un audio y un video en #general con muestras
  generadas (`assets/audio-sample.mp3`, `assets/video-sample.mp4`).
  `--capture-player` lo reproduce en la salida real
  ([captura](../docs/img/media-player-2026-10-17.png)). Detalle en
  [MEDIA.md](../docs/MEDIA.md#implementado-reproducir-audio-y-video-adjuntos-y-gif-de-tarjetas-2026-10-17).
  **No probado contra el servicio real**; no se escuchó ni se midió CPU con
  videos de 1080p.
- [x] 16.6 Cierre (2026-10-17): **búsqueda en el PDF** (lupa del visor o
  Ctrl+F: `pdftotext` del mismo Poppler y el mismo sandbox, hasta 4 MiB de
  texto, sólo en memoria con el visor abierto; lista páginas con extracto y
  lleva a la elegida; el resaltado en la página llega el 2026-10-18;
  [captura](../docs/img/pdf-search-2026-10-17.png); test ignorado real con
  withDocs verificado) y **resolución original** en el visor de imágenes
  («Ver en resolución original» si la imagen supera su vista previa:
  `ImageKind::Original`, hasta 4096 px, guardada como su archivo, una a la
  vez y fuera del presupuesto de las vistas previas). Con 16.4: **1644
  aprobadas, nueve ignoradas** (con `--all-features`), 25 suites; formato
  y Clippy `-D warnings` correctos.
- Estado: visor de imagen con zoom, galería y resolución original;
  animación de GIF, WebP y GIF de tarjetas; spoilers; PDF multipágina con
  búsqueda (opcional Linux/Nix con sandbox); audio y video MP4 H.264.
- Módulos: attachments/documents, descarga/decoder, reproducción y presupuesto
  de cuadros; APIs limn faltantes se desarrollan mediante PR en limn.
- Dependencias: 09 para salida y 08 para «Guardar como».
- Cierre verificable: movimiento reducido/viewport pausan reproducción; spoiler
  de adjunto requiere revelado antes de descargar; PDF mantiene sandbox/límites
  y cancelación; cierre/cambio de cuenta libera medios. Navegación/búsqueda y
  resolución original requieren su propia verificación. (Los cuatro primeros
  puntos cubiertos por tests el 2026-10-16; búsqueda en PDF, resolución
  original y la pausa de un video fuera de vista, el 2026-10-17.)
- Pendiente de 16: H.265 (abre en el navegador o se guarda: el único
  decodificador por software a mano, libde265, trae sus patentes); sonido
  multicanal se mezcla a mono como todo lo que RUNE reproduce; probar con
  archivos reales de Discord. VP8/VP9/AV1 y el resaltado en el PDF, hechos
  el 2026-10-18 (sección «Video y documentos» al final).

### 17. `calls` — Codec, transporte, mezcla y llamadas

- [ ] Conectar una ruta de audio bidireccional con fases y métricas reales.
- Estado (2026-10-16): `rune-media::{codec, jitter, mixer, call}` (Opus 48 kHz
  mono 20 ms con FEC/PLC, jitter de 16 ranuras, mezcla de hasta 64 fuentes con
  volumen/mute por persona, hilo de medios con compuerta de PTT/actividad de
  voz) y transporte de voz de Discord en `rune-discord-session::voice`
  (op 4, gateway de voz v8 con resume, IP discovery, UDP/RTP
  `aead_xchacha20_poly1305_rtpsize`, DAVE v1 con `davey`). `VoiceSupport::Real`
  para el proveedor de sesión; Mock sigue simulado. **Sólo fixtures; ninguna
  llamada real todavía.**
- Cubierto con fixtures deterministas: pérdida/reordenamiento/duplicados del
  jitter, cotas de memoria, reanudación tras 4015 y cierres terminales,
  `Connected` sólo con transporte listo (y grupo DAVE si hay otros), mute
  sin transmitir, ensordecer sin reproducir, salida que libera enlace y
  claves, unión/salida/mute por el proveedor, op 4 del Gateway.
- Pendiente: prueba real registrada (pasos en `docs/MEDIA.md`); `CallQuality`
  (hecho el 2026-10-18 en `voz-protocolo`: RTT del latido y pérdida por
  huecos de secuencia, sólo fixtures); `aead_aes256_gcm_rtpsize`; jitter
  adaptativo; `instant` sin mantenimiento (RUSTSEC-2024-0384, ignorado con
  motivo en `deny.toml`) por la feature `js` de openmls que activa `davey`:
  bloqueado aguas arriba.
- Resuelto después (2026-10-17/18): las llamadas privadas existen (rama
  `llamadas-privadas`) y el video se transmite (22 y `video-por-hardware`).
  «Escucharme» se desactiva durante una llamada, como Serein (decisión de
  `voz-estilo-serein`, `rune-ui/src/settings/pages.rs`): la salida es de la
  llamada.
- Hecho (2026-10-16): parche de `hpke-rs` 0.6.1 en `vendor/hpke-rs`
  (`sha3` de RustCrypto en lugar de `libcrux-sha3`, sin backend libcrux):
  `cargo audit` pasa de 8 vulnerabilidades `libcrux-*` a 0
  (RUSTSEC-2026-0207/0208/0212 incluidas); ver `vendor/hpke-rs/RUNE-PATCH.md`.
- Hecho (2026-10-16, sólo fixtures, sin llamada real): llamada tomada por
  otro dispositivo (otra `session_id` confirmada) termina sin op 4 de
  salida y no se retoma tras `READY`; `server_muted`/`server_deafened`
  aparte del mute propio, en rojo, y micrófono retenido; `Abilities::speak`
  (sólo escuchar) y `voice_activity` (pulsar para hablar obligatorio);
  reunirse con op 4 hasta 3 veces (1, 3, 10 s; se reinicia tras 30 s
  conectada) cuando los resume se agotan; `ChannelKind::Stage` listado y no
  unible; fase `Securing`. Detalle en `docs/DISCORD_SESSION.md`.
- Pendiente de esa tanda: pedir la palabra en escenarios. La fase
  `Waiting` y mirar los `GUILD_CREATE` tardíos antes de reunirse se hicieron
  el 2026-10-18 (`voz-protocolo`, abajo).
- Hecho (2026-10-16, sin escuchar): volumen 0–200 % y «Silenciar para mí»
  por persona en su menú, guardados en el equipo y nunca enviados, con marca
  en fila y recuadro; chat del canal de voz al lado del escenario desde 720 px
  (capacidad `voice_text`: simulación y sesión no oficial, ésta sólo con
  fixtures); sonidos propios de entrar/salir, silenciar, ensordecer y
  desconexión con un interruptor por familia en Ajustes → Voz y video. Ver
  `docs/MEDIA.md` y [captura](../docs/img/voice-chat-2026-10-16.png).
- Cierre verificable: pérdida/reordenamiento/reconexión con fixtures deterministas;
  mute/deafen afectan captura/salida; Connected sólo con transporte listo;
  memoria acotada y liberación de dispositivos/claves al terminar. Prueba real
  registrada antes de afirmar llamadas contra un servicio.

### 18. `navigation` — Navegación, teclado y accesibilidad

- [x] Historial atrás/adelante por cuenta (`rune-ui::navigation`).
  - Alt+←/Alt+→ y botones laterales del mouse (`UiEvent::PointerBack`/
    `PointerForward` de limn desde la PR #66, leídos por el host). Hasta 50 destinos por sentido; descarta los que ya no existen o
    no son visibles; vuelve al primer mensaje que se veía (o al final) y al
    control enfocado si sigue en el árbol, si no al compositor. Otra cuenta
    empieza sin historial. Volver a un canal no leído no lo marca hasta el final.
  - Verificado: tests de `rune-ui` (`tests/navigation.rs`) y del host
    (`input_tests.rs`) sobre el árbol limn, sin ventana ni proveedor real.
- [x] F6/Shift+F6 entre servidores, canales, mensajes, compositor y miembros;
  regiones vacías se saltan; con un diálogo abierto no se mueve. Probado sobre
  el árbol limn y con eventos nativos de egui en el host.
  - Desde la PR #66 de limn (2026-10-16) las cinco son regiones `Role::Region`
    con nombre («Servidores», «Canales», «Mensajes», «Escribir un mensaje»,
    «Miembros») y sus controles salen de `focusables_within`; el host lee F6,
    Ctrl+/, Alt+flechas y F12 del evento de limn y los consume antes que el
    árbol, para aterrizar en el control propio de RUNE.
- [x] «Atajos de teclado» (Ctrl+/) como modal limn; [captura](../docs/img/navigation-2026-10-16.png).
- [x] Escape cierra y Tab no sale de los diálogos del menú del servidor,
  ajustes, servidor nuevo, atajos y selector rápido; los controles accionables
  de la conversación, un mensaje con el puntero encima, los privados y esos
  diálogos tienen nombre accesible (nodos `Frame::access`). No se encontraron
  controles sin nombre en esos estados.
- [x] Ctrl+/ con distribuciones donde «/» exige Shift, flechas atrás/adelante
  visibles en la cabecera y desplazamiento restaurado al píxel (2026-10-19,
  rama `atajos-y-navegacion`; ver
  [Atajos editables](#atajos-editables-y-navegación-atrásadelante-2026-10-19-rama-atajos-y-navegacion)).
- [ ] Pendiente: lector de pantalla real (AccessKit/Orca/NVDA), escalado y
  F6/botones laterales en una ventana real.
- Estado previo: Ctrl+K/Ctrl+F/Alt+arriba-abajo, foco y IDs estables existentes.
  El ancla inicial al primer no leído y la apertura desde bandeja quedaron
  integradas en 06.
- Módulos: `rune-ui::navigation`, host y componentes; carpetas/favoritos del
  riel, paneles ajustables y atajos editables requieren estado propio.
- Dependencias: contexto por ID y APIs limn que falten mediante PR.
- Cierre verificable: atrás/adelante restituye destino/foco/ancla por cuenta;
  cambio de cuenta limpia destinos; salto a no leído no marca el final;
  secciones/diálogos accesibles por teclado, F6, escalado y lector de pantalla
  comprobados en ejecución real.

### 19. `advanced` — Administración y expresiones avanzadas

- [x] Operaciones remotas, paginación y familias de Serein por capacidad
  (2026-10-17; simulación, interfaz y `rune-discord-session` sólo con
  fixtures, sin probar contra Discord).
- [x] Edición del onboarding (2026-10-18, rama `reglas-y-onboarding`;
  simulación, interfaz y `rune-discord-session` sólo con fixtures).
- [x] Reglas de AutoMod (2026-10-19, rama `automod`; simulación,
  interfaz y `rune-discord-session` sólo con fixtures):
  [AutoMod](#automod-2026-10-19-rama-automod).
- [x] Eventos programados (2026-10-19, rama `eventos-programados`; ver
  [su sección](#eventos-programados-2026-10-19-rama-eventos-programados)).
- Estado: administración completa en la simulación y en la interfaz;
  el proveedor no oficial declara y hace todas las familias por rutas de
  Serein, documentadas o del cliente oficial; falta la prueba del dueño.
- Hecho (2026-10-16, solo simulación): Ajustes del servidor suman
  «Invitaciones» (creador, canal, usos, vencimiento; revocar con
  confirmación), «Emojis» y «Stickers» (lista, renombrar y eliminar con
  confirmación). «Registro de auditoría» pagina de 50 en 50, más reciente
  primero, con filtros por usuario y acción que aplica el proveedor
  (`AuditQuery`); «Baneos» pagina por id con «Cargar más», y la búsqueda
  avisa que solo lee lo cargado. `rune-state::admin` fusiona páginas por id,
  descarta una página que no continúa el final (sin huecos), ubica un veto
  en vivo solo dentro del rango cargado y limita la auditoría a 500
  entradas por filtro. Revocar, desvetar, renombrar y eliminar se muestran
  al confirmar y vuelven si el proveedor rechaza (`Store::ahead`). Al
  confirmar se revalidan permiso y jerarquía: sin ellos no se envía nada y
  un aviso dice por qué (también para expulsar, vetar, apodo y eliminar
  rol). Capacidades por familia: `audit_log`, `manage_invites`,
  `manage_expressions`; la simulación las declara con datos deterministas
  propios (sal aparte); `rune-discord-session` y `rune-discord` no
  (pruebas lo fijan). Captura: [server-admin-2026-10-16.png](../docs/img/server-admin-2026-10-16.png).
- Hecho (2026-10-17, rama `administracion`):
  - **Contrato** (bloque «Administration (19)» al final de `Command`,
    `Event`, `Failure`, `Capabilities` y `Created`): `TimeoutMember`,
    `CreateInvite`/`InviteCreated` (`create_invites`, `invite_links`),
    `CreateExpression` con `NewExpression`/`ExpressionImage` (alcance de
    subida propio, límites del servicio: emoji ≤ 256 KB y 128 px, sticker
    ≤ 512 KB y 320 × 320; `upload_expressions`), `LoadMembers`/
    `MembersLoaded` con cursor opaco (`member_pages`), `PruneMembers`/
    `Pruned` (`prune`), webhooks e integraciones (tipos `Webhook`,
    `Integration` en `rune-model`; `webhooks`, `integrations`), y los GIF
    favoritos del proveedor (`LoadGifFavorites`, `SetGifFavorite`,
    `GifFavorites`; `synced_gif_favorites`). Auditoría: `MemberTimedOut`,
    `InviteCreated`, `EmojiCreated`, `StickerCreated` y `Other` (lo que RUNE
    no describe, nunca filtro). Permiso nuevo `MANAGE_WEBHOOKS`.
  - **Simulación** (`mock::admin`): todo determinista y anotado en su
    auditoría; webhooks e integraciones inventados (quitar una aplicación
    saca a su bot), códigos que no llevan a ningún lado, imágenes nombradas
    y no alojadas, favoritos sólo locales.
  - **Estado** (`rune-state::admin`): páginas de miembros fusionadas con los
    miembros conocidos (una vez por persona; el cursor sólo avanza con la
    página que continúa la última, así un evento en vivo nunca duplica);
    invitación creada al frente y una sola vez; webhooks e integraciones con
    borrado mostrado antes de la respuesta y devuelto si se rechaza; GIF
    favoritos del proveedor con el cambio mostrado antes y devuelto si se
    rechaza. Habilidades nuevas: `timeout`, `create_invites`,
    `upload_expressions`, `member_pages`, `prune`, `manage_webhooks`,
    `manage_integrations` (y un aislamiento propio las quita).
  - **Imagen de una expresión** (`rune-core::expression_images`): el
    selector de archivos de siempre, un hilo de servicio; un GIF que ya
    entra queda animado, lo demás se hace PNG cuadrado del tamaño del tipo.
  - **Interfaz** (`server_settings::advanced`, `view/admin.rs`): «Aislar» y
    «Quitar el aislamiento» en el menú de un miembro (60 s a 1 semana,
    motivo); «Crear invitación» (canal, vencimiento, usos, temporal) que
    termina en el enlace para copiar, y copiar desde cada fila; «Subir
    emoji/sticker» con vista previa, nombre, emoji relacionado y
    descripción; «Podar» con la cuenta previa por 1, 7 o 30 días; página
    **Integraciones** (webhooks: crear, renombrar, mover, eliminar;
    integraciones: quitar); «Cargar más» miembros y «Mostrando N de M». Cada
    diálogo revalida permiso y jerarquía al confirmar. La pestaña GIF tiene
    **Favoritos** con corazones en resultados y enlaces usados, y dice dónde
    se guardan: «Sincronizados con tu cuenta de …» o «Guardados sólo en este
    equipo» (ajustes locales). Emojis y stickers no tienen favoritos: Serein
    tampoco los sincroniza (sólo frecuentes de la sesión).
  - **Discord** (`rune-discord-session::admin`, `http/admin.rs`): todas las
    familias anteriores y las que ya mostraba la interfaz (servidor, canales,
    permisos, roles y su orden, apodos, expulsar, vetar, auditoría, vetos,
    invitaciones, emojis, stickers), con validación previa contra la sesión,
    bits de permisos que conservan los que RUNE no modela, la respuesta
    aplicada a la sesión como su dispatch y `X-Audit-Log-Reason`. Rutas,
    origen y lista de prueba para el dueño en
    [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#administración-del-servidor-2026-10-17).
    Un hCaptcha en estas escrituras falla con su motivo hasta que lo
    enganche el mecanismo común (rama `social-2`).
  - Capturas: [integraciones](../docs/img/server-integrations-2026-10-17.png),
    [invitación](../docs/img/server-invite-2026-10-17.png),
    [sticker](../docs/img/server-sticker-upload-2026-10-17.png),
    [aislar](../docs/img/server-timeout-2026-10-17.png),
    [podar](../docs/img/server-prune-2026-10-17.png),
    [GIF favoritos](../docs/img/gif-favorites-2026-10-17.png).
- Pendiente, con motivo:
  - ~~**Reglas de AutoMod**~~: hechas el 2026-10-19
    ([AutoMod](#automod-2026-10-19-rama-automod)).
  - ~~**Eventos programados**~~: hechos el 2026-10-19
    ([Eventos programados](#eventos-programados-2026-10-19-rama-eventos-programados)).
  - ~~**Onboarding**~~: hecho el 2026-10-18, como miembro y como
    administración ([Reglas del servidor y onboarding](#reglas-del-servidor-y-onboarding-2026-10-18-rama-reglas-y-onboarding)).
  - **Paridad de administración restante de Serein**: perfil del servidor
    con ícono y rasgos, Seguridad y Participación (`PATCH /guilds/{g}`:
    canal del sistema, verificación, filtro, notificaciones, AFK), pausar
    invitaciones (`INVITES_DISABLED`), reordenar canales en lote, ajustes de
    foro (etiquetas, reacción y vista por defecto), ícono y degradé de rol,
    conteo de miembros por rol, filtros y orden de la página de miembros y
    su forma de entrada. Copiar la URL de un webhook queda fuera a
    propósito: es una credencial.
  - ~~La página de miembros, en una ventana angosta, deja sin lugar la columna
    del nombre (columna de roles fija de 300 px; anterior a este recorrido).~~
    Hecho el 2026-10-19 ([rama `admin-servidor-y-miembros`](#administración-del-servidor-y-miembros-2026-10-19-rama-admin-servidor-y-miembros)).
  Jerarquía: se aplica donde el destino es un miembro o un rol; invitaciones,
  expresiones, webhooks e integraciones sólo exigen el permiso.
- Módulos: server/channel settings, expresiones, permisos y contratos;
  onboarding/events/integrations sólo al definir tipos y operaciones.
- Dependencias: 03 y 07 para gestión de medios.
- Cierre verificable: revocación/jerarquía revalidadas al confirmar; listas y
  auditoría paginadas sin duplicación ante eventos; gestión de expresiones e
  invitaciones con confirmación; favoritos declaran alcance local/sincronizado.
  Reglas/eventos/integraciones tienen criterios propios, sin habilitación global.

### 20. `screens` — Pantalla y cámara

- [x] Implementar selección, permisos y ciclo de vida de fuentes (vista
  previa local, Linux, 2026-10-16).
- [x] Transmitir pantalla y cámara (recorrido 22, `video`,
  [docs/VIDEO.md](../docs/VIDEO.md); falta prueba real contra el servicio).
  La pantalla se transmite desde la fase 3 y la cámara también:
  `Capabilities::video` es verdadero en la simulación (`mock.rs`) y en la
  sesión no oficial (`provider.rs`, H264 por DAVE, sólo fixtures). «La
  transmisión llega con las llamadas» queda sólo para un proveedor sin
  `video`.
- [ ] Probar con hardware real (pasos abajo) y anotar el resultado en
  `docs/MEDIA.md`: ningún frame real se verificó todavía.
- [ ] Pendientes: DMA-BUF, Windows/macOS. Hecho: `restore_token` y
  `persist_mode` (`rune-platform` `screencast.rs`, con el portal v4 o más).
  Cámaras que sólo dan MJPEG: hechas el 2026-10-18 con una imagen
  sintética; falta probarlas con una cámara de verdad.
- Estado: `rune-platform::screencast` (portales ScreenCast y Camera con
  `zbus`, dueño fijado, handles verificados, `Session.Closed`, cierre en
  cancelación/denegación/`Drop`); `rune-media::video` (PipeWire 0.10 en hilo
  propio, ≤ 1280×720, ≤ 15 fps, cola de 2 que descarta lo viejo);
  `apps/desktop/src/video.rs` (sostiene/suelta fuentes, presenta superficies
  limn); UI en el panel y la barra de la llamada, mosaicos de vista previa con
  «Dejar de compartir»/«Apagar la cámara», y Ajustes → Voz y video → Video con
  cámara y «Probar video». Detalle y evidencia en
  [docs/MEDIA.md](../docs/MEDIA.md#implementado-vista-previa-local-de-pantalla-y-cámara-2026-10-16).
- Módulos: `rune-platform::screencast`, `rune-media::video`, `rune-ui::video`,
  `rune-components::voice::{VideoControls, Preview}`.
- Dependencias: 08, 17 y presupuestos de frames/encoder.
- Prueba manual (Linux con xdg-desktop-portal y PipeWire; nada se guarda):
  1. `nix-shell --arg withLogin true --run 'cargo run -p rune-desktop'`.
  2. Unirse a «Sala común» (simulada) y abrir su escenario.
  3. «Compartir pantalla»: debe abrir el selector del sistema. Cancelar →
     aviso «Se canceló en el diálogo del sistema», botón apagado. Repetir y
     elegir un monitor o ventana → mosaico «Tu pantalla» con la imagen.
  4. Detener desde el indicador del sistema (o cerrar la ventana compartida)
     → aviso «Se dejó de compartir la pantalla». «Dejar de compartir» y
     «Desconectar» también la sueltan.
  5. «Encender la cámara»: el sistema pide permiso la primera vez → mosaico
     «Tu cámara». Desenchufar la cámara → «Se apagó la cámara».
  6. Ajustes → Voz y video → Video: «Probar video», elegir otra cámara de la
     lista; cerrar los ajustes lo detiene.
  7. Con `pw-cli ls Node` (o `pw-top`) confirmar que el stream
     `rune-preview` desaparece al detener, colgar o cerrar RUNE.
- Cierre verificable: sin permiso no se adquieren frames; denegación/cancelación,
  hotplug/cierre/cambio de cuenta liberan fuente y sesión; fuente activa visible
  y detenible. ScreenCast/PipeWire es propuesta para pantalla Linux; cámara
  requiere una ruta de autorización/adquisición separada y verificada.

### 21. `spelling` — Ortografía opcional y atajos globales

- [x] Captura/registro/liberación del atajo de Pulsar para hablar (2026-10-16).
- [x] Atajos de silenciar, ensordecer y «pulsar para silenciar» en la misma
  sesión del portal (2026-10-17, P6, rama `escritorio`; ver
  [Paridad P6](#paridad-p6-escritorio-idioma-y-voz-local-2026-10-17-rama-escritorio)).
- [ ] Atajos globales probados en un compositor real.
- [x] Ortografía: no se hace. Evaluada (ver abajo); revisado el 2026-10-17
  para P6: Serein no tiene corrector (ni código ni crates de diccionarios),
  así que no es parte de la paridad. El 2026-10-18 el dueño decidió no tener
  corrector (decisiones del plan de construcción); el diseño de abajo y sus
  tres PR a limn quedan sólo como registro.
- [x] Atajos internos editables (2026-10-19, rama `atajos-y-navegacion`):
  tabla común de acciones con reasignación, conflictos y «Restablecer»; ver
  [Atajos editables](#atajos-editables-y-navegación-atrásadelante-2026-10-19-rama-atajos-y-navegacion).
- Estado: «Pulsar para hablar» en Voz y video con grabador (Esc, perder el
  foco, salir de la página/ajustes o 10 s lo terminan sin cambiar nada),
  retraso al soltar 0–2000 ms, PTT en la ventana por las teclas que limn
  guarda sostenidas (antes, el estado de teclas de egui) y global por `rune_platform::shortcuts` (portal GlobalShortcuts), sólo para
  combinaciones o teclas F. `rune_media::PushToTalk` condiciona la marca de
  «hablando» y `ui.audio.transmitting` (para la 17; no hay transporte). Una
  sesión por cuenta y atajo; desactivar, cambiar atajo o cuenta, o cerrar la
  ventana la cierra; sesión cerrada, portal desaparecido o foco perdido
  liberan la tecla. Probado con portal falso en bus privado, grabador,
  liberación por retraso/fin de sesión/foco y caminos sin portal o sin
  interfaz; **sin** compositor real (diálogo de KDE/GNOME/Hyprland, trigger
  asignado distinto del pedido, comportamiento de repetición). Detalle en
  [MEDIA.md](../docs/MEDIA.md#pulsar-para-hablar-2026-10-16).
- Ortografía, evaluación: `spellbook` (Rust puro, compatible con
  diccionarios Hunspell `.aff/.dic`, `check` y `suggest`, sin C/C++) es la
  opción liviana; `hunspell-rs`/`nuspell` traen C/C++ y enlace nativo;
  `zspell` es Rust puro pero menos completo. Diseño propuesto:
  `rune-platform::spellcheck` busca diccionarios del sistema en
  `XDG_DATA_DIRS/{hunspell,myspell}` (nada empaquetado ni descargado), carga
  en un worker, desactivado por defecto con idioma elegido en Idioma y hora;
  revisa palabras del borrador con debounce, excluyendo código, menciones,
  enlaces, `:emoji:` y la preedición IME; las sugerencias sólo se aplican
  con un clic explícito y nunca se manda texto fuera del equipo. Falta en
  limn: subrayado ondulado en `TextHighlight` (p. ej. `underline:
  Option<Underline { color, wavy }>`), el offset de carácter bajo el clic
  derecho en el evento de menú contextual del `Composer`/`TextArea`, y
  `UiTree::replace_input_range(id, range, text)` que conserve cursor y
  deshacer. Esas tres van como PR a limn antes de implementar.
- Módulos: `rune-platform::shortcuts` (hecho), `spellcheck` pendiente;
  preferencias/composer y soporte de edición limn mediante PR si falta.
- Dependencias: 08 y soporte de edición en limn.
- Cierre verificable: asignar atajo termina con blur/cancelación/timeout;
  PTT global opera fuera de foco y libera registro al desactivar/cerrar/cambiar
  sesión; recupera una tecla liberada perdida. Ortografía opcional no envía
  texto fuera implícitamente ni modifica borradores sin acción explícita.

### 22. `video` — Mirar y transmitir pantalla y cámara

- [x] Protocolo confirmado por REA estática del cliente oficial (2026-10-16):
  ops 18–22 y despachos `STREAM_*`, conexión de stream con
  `rtc_server_id`, op 0/1/4/12/15 con video, H264 101/102, RTX, PLI/NACK/REMB,
  DAVE por códec con `davey`, calidades. Evidencia y lo que falta confirmar en
  [docs/REA.md](../docs/REA.md#video-go-live-cámaras-y-transporte-de-video-2026-10-16).
- [x] Diseño por fases en [docs/VIDEO.md](../docs/VIDEO.md).
- [x] Decisión del dueño sobre la licencia de H264 (2026-10-16): `openh264`
  desde la fuente, sin la licencia de patentes de Cisco (dicho en
  [docs/MEDIA.md](../docs/MEDIA.md#implementado-mirar-la-pantalla-que-comparte-otra-persona-2026-10-16)).
- [x] Fase 1: mirar streams de pantalla (Go Live), con audio del stream
  (2026-10-16). **Sólo fixtures: no probado contra el servicio real.**
  Ops 20/19/21 y `STREAM_*` en `rune-discord-session::voice`; segunda
  conexión de voz con `rtc_server_id`, op 0/1/12/15 de video, H264 (unidad
  simple, STAP-A, FU-A) en `voice::h264`, DAVE de video, PLI `rtpsize`;
  `rune-media::video::decode` (OpenH264 en su hilo, colas acotadas, 150 ms);
  `Capabilities::watch_streams`, `Command::WatchStream`/`StopWatching`,
  `Event::StreamPhase`, `Abilities::watch_stream`, `Store::watching`;
  «EN VIVO», «Ver transmisión», «Dejar de ver» y el stream en el escenario;
  simulación con cuadros inventados «simulada»
  ([captura](../docs/img/stream-watch-2026-10-16.png)). Ajustes al plan en
  [docs/VIDEO.md](../docs/VIDEO.md#ajustes-de-la-fase-1-2026-10-16).
  Pendiente: prueba real del dueño (mirar el stream de un cliente oficial de
  confianza y anotarlo en `docs/DISCORD_SESSION.md`), pantalla completa y
  ventana aparte (PR a limn).
- [x] Fase 2: mirar las cámaras de la llamada (2026-10-16). **Sólo
  fixtures: no probado contra el servicio real.** La conexión de la llamada
  con `video: true` y H264 para decodificar; `VoiceState::camera` desde
  `self_video`; SSRC de cámara por op 12; op 15 por SSRC (100 a la vista, 0
  el resto) al cambiar y cada 5 s (1 s sin imagen); `VideoKind`,
  `VideoRequest::Wants`, `MAX_CAMERAS = 4`, `Capabilities::watch_cameras`,
  `Abilities::watch_camera`; el host reparte unidades por fuente y sostiene
  hasta 4 decodificadores; el mosaico muestra la cámara en lugar del avatar;
  simulación con «Cámara simulada»
  ([captura](../docs/img/camera-watch-2026-10-16.png)). Ajustes al plan en
  [docs/VIDEO.md](../docs/VIDEO.md#ajustes-de-la-fase-2-2026-10-16).
  Pendiente: prueba real del dueño (ver la cámara de un cliente oficial de
  confianza y anotarlo en `docs/DISCORD_SESSION.md`).
- [x] Fase 3: compartir la pantalla propia (2026-10-16). **Sólo fixtures:
  no probado contra el servicio real.** Op 18/19/21 con la clave propia y
  `STREAM_*` repartidos por clave; conexión de stream como emisor (op 0 con
  `streams` de pantalla, SSRC de Ready, RTX = SSRC + 1, op 1 `encode`, op 12
  con `max_bitrate`/`max_framerate`/`max_resolution`); SPS normalizado
  (`voice::sps`), DAVE `Codec::H264`, STAP-A/FU-A ≤ 1200 bytes, ritmo cada
  2 ms; PLI/FIR → clave, RR/REMB → tasa (`voice::send`);
  `rune-media::video::encode` (OpenH264 Constrained Baseline en su hilo,
  contrapresión, clave al empezar/pedir/descartar) y `Video::share`;
  `Capabilities::share_screen`, `Abilities::share_screen`,
  `Command::StartStream`/`StopStream`, `Store::sharing`; selector de calidad
  (480p/720p/1080p × 15/30/60, 720p30 por defecto, sin tope por Nitro) antes
  del diálogo del sistema, fps y kbit/s reales en la vista previa; la
  simulación lo marca «simulada»
  ([captura](../docs/img/screen-share-2026-10-16.png)). Ajustes al plan en
  [docs/VIDEO.md](../docs/VIDEO.md#ajustes-de-la-fase-3-2026-10-16).
  Pendiente: prueba real del dueño (compartir a un cliente oficial de
  confianza y anotarlo en `docs/DISCORD_SESSION.md`), medir CPU a 1080p60.
- [x] Fase 4: enviar la cámara (2026-10-17). **Sólo fixtures: no probado
  contra el servicio real.** `Command::SetCamera`, `Failure::Camera`,
  `Capabilities::video` y `Abilities::send_video`; op 4 `self_video`; la
  conexión de la llamada anuncia siempre una cámara (op 0) y la describe o
  retira con op 12; colas propias en `VideoLink`; OpenH264 en modo cámara
  (720p30, 0,15–2,5 Mbit/s); la vista previa dice fps y kbit/s y la
  simulación lo marca «simulada». Ajustes al plan en
  [docs/VIDEO.md](../docs/VIDEO.md#ajustes-de-la-fase-4-2026-10-17).
  Pendiente: prueba real del dueño (mandar la cámara a un cliente oficial
  de confianza y anotarlo en `docs/DISCORD_SESSION.md`).
- [x] Fase 5: extras (2026-10-17). **Sólo fixtures: no probado contra el
  servicio real.** RTX al enviar (historial de 1 s, 2048 paquetes, 2 MiB) y
  NACK al recibir (orden con espera de 200 ms; Serein no lo manda, sale de
  la REA); pausa con op 22 (`Command::PauseStream`, «Pausar»/«Reanudar»;
  Serein no la usa); hasta 3 transmisiones a la vez en el escenario
  (`MAX_STREAMS`); vistas previas con `GET /streams/{clave}/preview` bajo la
  fila de quien transmite (`Capabilities::stream_previews`, ≤ 32 guardadas,
  cada 60 s); sonido de las aplicaciones al compartir por PipeWire sin el de
  RUNE (op 5 SOUNDSHARE, «Con sonido») y volumen propio de cada transmisión
  mirada; una sola capa de cada cámara enviada en simulcast; el portal
  recuerda la pantalla (`restore_token`, guardado con las preferencias,
  «Elegir otra pantalla»); pantalla completa y ventana aparte (sin PR a
  limn: viewports de eframe con limn adentro); CPU medida: 1080p60 no
  alcanza por software en esta máquina (35 a 40 fps), 1080p30 y 720p60 usan
  medio núcleo
  ([PERFORMANCE.md](../docs/PERFORMANCE.md#codificar-la-pantalla-compartida-2026-10-17)).
  Capturas: [varias](../docs/img/streams-multi-2026-10-17.png),
  [pausa](../docs/img/stream-pause-2026-10-17.png),
  [vista previa](../docs/img/stream-preview-2026-10-17.png),
  [pantalla completa](../docs/img/stream-fullscreen-2026-10-17.png). Ajustes
  al plan en
  [docs/VIDEO.md](../docs/VIDEO.md#ajustes-de-la-fase-5-2026-10-17).
  No hecho, con su porqué en VIDEO.md: subir la vista previa propia (Serein
  no la usa: queda afuera por la regla del dueño), mandar la cámara propia
  en dos capas (Serein no lo hace, el cliente oficial sólo con un
  experimento, y anunciar una capa obliga a mandarla). Las transmisiones
  en llamadas privadas llegaron con la rama `llamadas-privadas`
  (2026-10-17). Fuera del recorrido: DMA-BUF,
  Windows y macOS. Pendiente: prueba real del dueño (NACK y RTX, pausa,
  varias transmisiones, vista previa, el sonido de las aplicaciones con
  PipeWire real, el token del portal del escritorio y la ventana aparte en
  su sesión), y 1080p60 si hace falta (cortes con hilos de OpenH264 o
  hardware).
- Dependencias: 17 (llamada real probada), 20 (fuentes locales).
- Cierre verificable por fase: fixtures del servidor de voz con emisor
  sintético (H264 codificado en la prueba, cifrado en las dos capas), cotas
  de colas y liberación al soltar; prueba real del dueño registrada antes de
  afirmar que funciona contra Discord.

## Paridad P1: cuenta sincronizada (2026-10-17, rama `cuenta-sincronizada`)

Hecho, sólo con fixtures (nada contra Discord); detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#cuenta-sincronizada-2026-10-17-rama-cuenta-sincronizada).

- Contrato: `rune-model/src/account.rs` (`NotifyLevel` se mudó acá desde
  rune-components, que lo reexporta), capacidades `synced_notifications`,
  `server_folders`, `messaging_privacy` y `notes`; comandos
  `LoadAccountSettings`, `SetNotifications`, `SetServerFolders`,
  `SetMessagingPrivacy`, `LoadNote`, `SetNote`; `SetCustomStatus` lleva
  aparte el emoji y el vencimiento. `Store` guarda lo de la cuenta,
  `Store::rail` ordena el riel por carpetas y `Store::account_abilities`
  dice qué se puede cambiar.
- Simulación (`mock/account.rs`), determinista y declarada.
- Sesión: estado por Identify y op 3 con ritmo y en `settings-proto/1`,
  restaurado tras cada READY, `SESSIONS_REPLACE`; notificaciones de READY,
  `USER_GUILD_SETTINGS_UPDATE` y `PATCH …/guilds/{g|@me}/settings`;
  carpetas y privacidad en `settings-proto/1` con decodificador protobuf
  propio (`settings_proto.rs`, sin dependencia nueva); notas. Todas las
  escrituras pasan por el hCaptcha común.
- Interfaz: con la capacidad, riel, lista de canales y notificaciones del
  sistema leen la cuenta; silencios, niveles, `@everyone`, roles y ocultar
  silenciados piden el cambio. Carpetas plegables en el riel con menús
  (mover, meter, sacar, nueva, renombrar, color, desarmar) y diálogo.
  Ajustes → Datos y privacidad → «Mensajes directos y solicitudes». Nota en
  el perfil. Capturas `docs/img/{folders,folders-closed,folder-dialog,account-notifications,messaging-privacy}-2026-10-17.png`.
- Desktop: `--action context:{id}@{x},{y}` y `--action click:{id}` para
  capturas.

Pendiente y por qué:

- ~~Arrastrar servidores y carpetas en el riel~~, ~~nivel por defecto del
  servidor~~, ~~menciones de roles~~, ~~silenciar privados y grupos~~ y
  ~~carpetas abiertas entre corridas~~: hechos el 2026-10-18 (rama
  `riel-y-notificaciones`, más abajo).
- RUNE no difumina contenido explícito: el ajuste se guarda en la cuenta
  para los otros clientes.
- Prueba del dueño contra Discord (lista en DISCORD_SESSION.md).

## Paridad P3: comandos e interacciones (2026-10-17, rama `comandos-e-interacciones`)

Hecho, sólo con fixtures (nada contra Discord), con Serein y una REA
estática del cliente web oficial como referencia, sin copiar código;
detalle en [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones)
y [REA.md](../docs/REA.md#cliente-web-oficial-interacciones-y-comandos-de-aplicación-2026-10-17).

- Contrato: `rune-model/src/interactions.rs` (`AppCommand` con contextos,
  permisos y overrides ya resueltos, `CommandOption` con validación local
  de requeridos, tipos, rangos, largos y `choices`, `AppCommand::check`,
  `CommandUse`, `Modal` y sus campos con `problem`, `Interaction`), menús y
  botones con `custom_id` en `components.rs`, `Permissions::USE_APP_COMMANDS`;
  comandos `LoadAppCommands`, `Interact`, `SuggestChoices`; eventos
  `AppCommands`, `Interaction`, `ModalOpened`, `CommandChoices`,
  `AppCommandsChanged`; fallos `AppCommands` e `Interaction`; capacidades
  `interactions` y `app_commands`.
- Store (`rune-state/src/interactions.rs`): una interacción a la vez con
  espera de 30 s (`interaction_tick` despierta al host), el error donde
  ocurrió, el modal abierto, el índice por servidor o privado con caché de
  5 min e invalidación, `usable_commands` (contexto, servidor del comando,
  adultos, `needs`, overrides; hilos como su canal; administración todo),
  sugerencias por pedido y efímeros que no cuentan como no leídos (16 a la
  vez) y se descartan. Core no los guarda en SQLite (test).
- Simulación (`mock/interactions.rs`): RuneBot (simulado) con un mensaje de
  partes (confirmar, un error, un menú de horario) y otro de menús de
  personas, roles, canales y menciones con valores por defecto y un botón al
  formulario; comandos `/dado`, `/recordatorio` (choices), `/buscar`
  (sugerencias), `/saludar` (persona), `/config avisos activar|desactivar`
  y `/config ver` (grupo y subcomandos), `/formulario` (modal), `/secreto`
  (efímero) y `/moderar` (pide gestionar mensajes); respuestas efímeras o
  públicas, deterministas por pedido.
- Interfaz: partes activas con espera y error en su lugar
  (`rune-components/src/message/components.rs`), «Sólo vos podés ver esto ·
  Descartar», modal limn (`rune-components/src/interactions.rs`,
  `rune-ui/src/interactions.rs`), y `/` en el compositor
  (`rune-ui/src/commands.rs`, sin tocar `@`): comandos locales de RUNE
  marcados «RUNE» y los de cada aplicación bajo su nombre; un comando de
  app muestra sus opciones sobre el compositor (texto, número, entero,
  sí/no, persona, rol, canal, mención, choices) con los errores de cada una
  al intentar, las sugerencias del bot como fichas, Enter o «Enviar
  comando», Escape o la cruz para salir. `/gif` y `/sticker` abren el
  selector en su pestaña con la búsqueda; `/msg @persona texto` abre el
  privado con el texto en su compositor. La respuesta de una app a un
  comando muestra arriba «Invitado usó /dado», como una respuesta
  (`Components::invoked`, de `interaction_metadata` o `interaction`).
- Sesión: ver DISCORD_SESSION.md; el `session_id` del Gateway llega en un
  evento interno propio antes del READY.
- Desktop: `--action after:{id}` difiere acciones de captura (con
  `app:`/`app-pick:`/`app-close:` por `custom_id`).
- Capturas: [partes esperando](../docs/img/app-parts-waiting-2026-10-17.png),
  [menú de personas](../docs/img/app-menus-2026-10-17.png),
  [efímero y error](../docs/img/app-ephemeral-2026-10-17.png),
  [modal](../docs/img/app-modal-2026-10-17.png),
  [`/`](../docs/img/slash-commands-2026-10-17.png),
  [opciones de un comando](../docs/img/slash-command-form-2026-10-17.png),
  [sugerencias](../docs/img/slash-suggestions-2026-10-17.png),
  [respuesta a un comando](../docs/img/slash-command-answer-2026-10-17.png)
  (Xvfb privado 1280×800, simulación).
- **1834 aprobadas, nueve ignoradas** (con `--all-features`), 25 suites; formato y
  Clippy `-D warnings` correctos.

Pendiente y por qué (subir archivos en un modal: hecho el 2026-10-18; lo
demás, fuera de alcance por decisión del dueño del 2026-10-18, porque
Serein no lo tiene; ver
[Comandos de aplicación](#comandos-de-aplicación-sólo-lo-que-tiene-serein-2026-10-18-rama-comandos-de-apps)):

- **Opciones de adjunto** en comandos: hoy la opción dice «RUNE todavía no
  adjunta archivos a comandos» y, si es obligatoria, no deja enviar. Serein
  tampoco las completa. Fuera de alcance.
- Índice de las aplicaciones instaladas por el usuario
  (`/users/@me/application-command-index`): RUNE, como Serein, sólo ofrece
  comandos del servidor o de un privado con un bot. Fuera de alcance.
- El mensaje provisional «Enviando comando…» en la conversación: el estado
  se ve sobre el compositor, como en Serein. Fuera de alcance.
- Reintentos del `202` del índice (en preparación): se informa como fallo y
  el próximo `/` lo pide otra vez. Fuera de alcance.
- Comandos de menú contextual (tipos 2 y 3 del índice). Fuera de alcance.
- Motivo `ephemerality_reason` de una respuesta efímera. Fuera de alcance.

## Paridad P4: acciones de mensaje, compositor y moderación (2026-10-17, rama `acciones-de-mensaje`)

Hecho, sólo con fixtures (nada contra Discord); detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#acciones-de-mensaje-compositor-y-moderación-2026-10-17-rama-acciones-de-mensaje).

- Contrato, en bloques propios: `rune-provider/src/message_actions.rs`
  (`NewPoll` y sus límites, `VoiceModeration`, `SILENT` y
  `without_silent`, `MAX_FORWARDS`); comandos `ForwardMessage`,
  `CreatePoll`, `EndPoll`, `LoadReactions`, `LoadPollVoters`,
  `LoadChannel` y `ModerateVoice`; eventos `PeopleLoaded` y
  `ChannelLoaded`; fallas `Forward`, `NewPoll`, `EndPoll`, `People` y
  `Place`; `SendMessage` lleva `mention_reply`. Capacidades
  `forward_messages`, `create_polls`, `reaction_lists`, `poll_voters`,
  `load_channels`, `silent_messages`, `reply_mentions`,
  `voice_moderation`, `leave_servers` y `trusted_links`; `channel_links`
  documenta `@me` y el mensaje. Modelo: `TextTarget::Link`,
  `TextTarget::Time` con `TimeStyle`, `Permissions::SEND_POLLS`.
- Estado: `Abilities` (`forward`, `create_poll`, `reaction_list`,
  `poll_voters`), `ServerAbilities` (`mute_members`, `deafen_members`,
  `move_members`; `leave` ya no depende de `manage_servers`),
  `Store::message_link`, `end_poll`, `forward_targets`,
  `voice_moderation` (permiso y rango), `voice_state_of`, `quiet_reply`
  (un reintento no vuelve a mencionar), `Applied::People` y
  `Applied::ChannelLoaded`.
- Simulación (`mock/message_actions.rs`), determinista: reenvía (una copia
  por nonce), crea y termina encuestas, inventa quién reaccionó y votó desde
  los números (y lo dice en la interfaz), devuelve un privado cerrado desde
  su enlace, modera la voz con permiso y rango; sus enlaces usan
  `simulado.invalid` y sólo abren en RUNE. En #general: una encuesta propia
  abierta y un mensaje con un enlace y dos marcas `<t:…>`.
- Sesión: rutas de Serein y documentadas, `@silent` con flag 4096,
  `replied_user`, enlaces de `discord.com`/`ptb.`/`canary.`/`discordapp.com`
  y marcas `<t:…>` en `text::references`, `SEND_POLLS` (bit 49), la
  moderación de voz por `admin` como el resto de la administración.
- Interfaz (`rune-ui/src/message_actions/` y
  `rune-components/src/message_actions.rs`): menú del mensaje («Reenviar»,
  «Copiar enlace del mensaje», «Ver reacciones», «Ver votos», «Terminar
  encuesta»), diálogos de reenviar, crear encuesta y personas, «Crear
  encuesta» en el «+», «@ ACTIVADO/DESACTIVADO» y el aviso de silencioso en
  el compositor, `@silent` en las opciones de `@`, atajos de formato que
  mantienen la selección (`Intent::SelectInput`), pastillas de enlaces,
  pregunta antes de abrir enlaces externos con dominios de confianza
  guardados (`trusted_link_domains`), marcas `<t:…>` (fecha completa al
  hacer clic, `moments_tick` cada 30 s) y la moderación en el menú de voz.
  «Crear encuestas» en los permisos de servidor y canal.
- limn: [PR #73](https://github.com/Niiihuel/limn/pull/73) (bloques de
  código coloreados) y [PR #74](https://github.com/Niiihuel/limn/pull/74)
  (`UiTree::set_input_selection`); RUNE apunta a `rune/code-highlight`
  (`afba53f`).
- Desktop: acciones de captura `last:{qué}`, `voice-menu` y `send`, que
  esperan los mensajes. Capturas
  `docs/img/{message-forward,message-reactions,poll-create,poll-voters,link-confirm,message-links,reply-silent,voice-moderation,code-highlight}-2026-10-17.png`.

Pendiente y por qué:

- ~~Mencionar al autor al responder con archivos~~: hecho el 2026-10-18
  (`envios-y-subidas`), `Command::SendFiles.mention_reply`.
- ~~Tooltip con la fecha completa de una marca `<t:…>`~~: resuelto el
  2026-10-18 (rama `limn-texto`, limn PR #82, `TextSpan::tooltip`).
- Súper reacciones: se muestran aparte desde el 2026-10-18 (rama
  `reacciones-fijados-y-busqueda`); no se mandan ni se listan.
- ~~Arrastrar personas entre canales de voz~~: hecho el 2026-10-18 (rama
  `admin-canales-y-voz`); el menú «Mover a» sigue.
- Más lenguajes en el coloreado (Serein tiene 24): hecho el 2026-10-18
  (paquete 10): el analizador por línea de limn suma 16 (Go, Java, Kotlin,
  Swift, C, C++, C#, PHP, Ruby, Lua, Shell, YAML, SQL, Diff, Dart y Zig),
  los mismos 24 de Serein más TSX y Markdown.
- Prueba del dueño contra Discord (lista en DISCORD_SESSION.md).

## Paridad P6: escritorio, idioma y voz local (2026-10-17, rama `escritorio`)

Hecho salvo las traducciones (trabajo aparte), sólo con buses D-Bus y
sockets privados, la simulación y fixtures en loopback: nada en un
escritorio real ni contra Discord. Detalle en
[PLATFORM.md](../docs/PLATFORM.md); el proveedor en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#escritorio-y-actividad-local-2026-10-17-rama-escritorio).

- Plataforma (`rune-platform`, módulos nuevos, cada uno en su hilo):
  `tray` (StatusNotifierItem + dbusmenu sobre zbus, sin GTK ni crates
  nuevos; ícono dibujado en código con punto de menciones), `single_instance`
  y `rich_presence` (sockets 0600 en `$XDG_RUNTIME_DIR`, `SO_PEERCRED`;
  `local_socket` compartido), `autostart` (XDG), `appearance` (portal
  Settings), `fonts` (`fc-list`/`fc-match`), `games` (`/proc`: exe y primer
  argumento acotado) y `hyprland` (ocultar en un workspace especial).
  Dependencias: `serde_json` (ya del workspace) y las features `net` y
  `process` de `rustix`.
- Contrato, en bloques propios: `rune_model::{LocalActivity,
  LocalActivityKind, ActivitySource, DetectableGame}`;
  `Command::{SetLocalActivity, LoadDetectableGames, VoiceVerificationCode}`;
  `Event::{DetectableGames, CallPrivacyCode, VoiceVerification}`;
  capacidades `local_activity`, `detectable_games`, `voice_privacy_code`.
  `Store`: actividad local mostrada como propia (`set_local_activity`,
  `presence`), `privacy_code`, `verification`/`begin_verification`,
  `detectable_games`, `channel_tree_with_hidden`; `Applied::DetectableGames`.
- Simulación (`mock/desktop.rs`): la actividad vuelve como presencia,
  dos juegos inventados, códigos deterministas que la interfaz llama
  simulados, y el canal «moderación» que sólo ve su moderación (agregado
  último, sin mover ids).
- Sesión: op 3 con la actividad local (nombre e imágenes resueltos por las
  rutas de Serein), `/applications/detectable` sin credencial, y la
  conexión de voz informa el código de privacidad y calcula el de
  verificación fuera de su tarea.
- Interfaz (`rune-ui/src/desktop.rs`, `settings/pages/desktop.rs`,
  `rune-components/src/desktop.rs`): preferencias nuevas en `Prefs`
  (cerrar a la bandeja, tema del sistema, fuente, anchos, favoritos,
  fijados, ocultos, tres atajos, Rich Presence y juegos, estos dos
  apagados), Ajustes › Sistema (Escritorio, Atajos del micrófono y el
  sonido, Actividad), Apariencia (tema «Sistema», Fuente, Paneles y
  canales), Voz y video (cifrado, durante una llamada), pestaña
  «Privacidad» en Detalles de voz, menús «Favorito», «Fijar en la lista» y
  «Mostrar los canales ocultos», bordes arrastrables.
- Host (`apps/desktop/src/desktop.rs`): arranca y para los servicios según
  las preferencias, cierra a la bandeja (X11 oculta; Hyprland por socket;
  otros minimizan), `App::logic` sigue aplicando eventos con la ventana
  oculta, instancia única antes de abrir la ventana (enlaces https de la
  línea de comandos), autostart y fuentes en hilos aparte.
- limn: `limn_egui::font_definitions_with` ([PR #76](https://github.com/Niiihuel/limn/pull/76) desde `feat/ui-faces`;
  RUNE en `rune/ui-faces`, `e56128c`).
- Capturas `docs/img/escritorio-{bandeja-sesion,actividad,atajos-voz,favoritos-ocultos,privados-fijados,codigo-privacidad,tema-sistema,fuente}-2026-10-17.png`.

Pendiente y por qué:

- ~~Traducciones: trabajo aparte, decidido por el dueño~~: hechas en la rama
  `idiomas` (ver abajo).
- ~~Las preferencias nuevas no persisten con los proveedores reales~~:
  resuelto en la rama `estabilidad` (ver abajo).
- Corrector ortográfico: Serein no lo tiene (ver el recorrido 21).
- «Iniciar minimizado» con la sesión (Serein lo tiene en Windows/macOS):
  Wayland no crea una ventana oculta; habría que arrancar en la bandeja.
- ~~`INVITE_BROWSER` de un programa no abre la invitación~~: resuelto en
  la rama `estabilidad`.
- Lista propia de juegos (agregar, renombrar u ocultar uno, como Serein) y
  caché en disco de la lista de Discord (hoy una vez por sesión).
- ~~Marca de canal privado (candado de Serein para `@everyone` sin `VIEW`)~~:
  hecha en la rama `adopcion-limn` (ver abajo).
- KWin: Serein lo oculta con un script; RUNE lo minimiza.
- Prueba en escritorio real y contra Discord: lista en PLATFORM.md y
  DISCORD_SESSION.md.

## Paridad P5: presencia rica, identidad visual y expresiones (2026-10-17, rama `presencia-e-identidad`)

Hecho, sólo con fixtures (nada contra Discord), con Serein y la REA
estática del bundle web oficial como referencia, sin copiar código ni
recursos; detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#presencia-rica-identidad-visual-y-expresiones-2026-10-17-rama-presencia-e-identidad),
[PROFILES.md](../docs/PROFILES.md#presencia-rica-e-identidad-visual-2026-10-17-p5)
y [EXPRESSIONS.md](../docs/EXPRESSIONS.md#plan-de-la-cuenta-paquetes-y-gif-del-servicio-2026-10-17-p5).

- Contrato (`rune-model/src/identity.rs`): `Presence.activities` (hasta 4)
  y `platforms`, `Activity` con segunda línea, imágenes, inicio y fin
  (`progress`, `is_track`); `Identity` (decoración, nameplate, etiqueta) en
  `User` y `Member`; `RoleStyle` en `Role`; `Badge`, `Connection` y
  `ProfileEffect` en `UserProfile`; `Perks`, `StickerPack`; todo lo que se
  guarda con `#[serde(default)]` (filas viejas probadas), sin cambio de
  tablas. Comandos `LoadStickerPacks`, `SearchGifs`, `SuggestGifs`; eventos
  `Perks`, `StickerPacks`, `GifResults`, `GifSuggestions`; fallos
  `StickerPacks` y `Gifs`; capacidades `sticker_packs` y `gif_search`;
  `GifPage.categories`.
- Store (`rune-state/src/identity.rs`): plan y paquetes por sesión (se
  olvidan con otra cuenta), `identity_in`, `role_mark`, `emoji_perk`,
  `sticker_perk`, `known_sticker`.
- Simulación (`mock/identity.rs`): actividades con imágenes y tiempos,
  plataformas, marcas, roles con emoji, imagen y degradé, insignias,
  conexiones y un efecto en perfiles, plan «Plus (simulado)», dos paquetes,
  GIF y sugerencias deterministas; ahora envía stickers.
- Sesión: ver DISCORD_SESSION.md. Además un solo lector de protobuf para
  `settings-proto/1` y `/2` (`settings_proto.rs`; `admin/favorites.rs` sólo
  conserva el esquema de los favoritos).
- Interfaz: `rune-components/src/identity.rs` (tarjetas de actividad,
  plataformas, insignias, conexiones, etiqueta, marca de rol, decoración,
  nameplate, efecto) y `rune-ui/src/identity.rs`; tarjeta, perfil completo,
  lista, mensajes, selector con secciones, candado y paquetes, pestaña GIF
  con fuentes, categorías y sugerencias. `rune_ui::activity_tick` repinta
  cada segundo un tiempo que se ve. `rune-core` decodifica APNG
  (decoraciones animadas) y expone `normalize_image` para las imágenes
  simuladas de las capturas.
- limn: `Element::foreground_gradient` en la rama `rune/text-gradient`
  (05061cd, sobre 514993f) y la [PR #75](https://github.com/Niiihuel/limn/pull/75)
  desde `feat/text-gradient` sobre main.
- Capturas: [actividades y marcas](../docs/img/presence-identity-2026-10-17.png),
  [efecto, insignias y degradés](../docs/img/profile-effect-2026-10-17.png),
  [stickers con plan y paquetes](../docs/img/sticker-packs-2026-10-17.png),
  [GIF del servicio](../docs/img/gifs-service-2026-10-17.png) (Xvfb privado
  1280×800, simulación).
- **1893 aprobadas, nueve ignoradas** (con `--all-features`), 25 suites;
  formato y Clippy `-D warnings` correctos.

Pendiente y por qué:

- **Marcos de perfil** (collectibles de tipo 3): grapas, rieles y bordes
  que desbordan la tarjeta necesitan las medidas de cada capa (la UI no
  las tiene hoy) y fixtures reales para no inventar la composición.
- ~~Tapa de Spotify~~: desde `i.scdn.co`, con Twitch y YouTube, por decisión
  del dueño (2026-10-18, rama `perfil-tarjeta-y-cosmeticos`). Ícono de la aplicación cuando una
  actividad no trae imagen (Serein lo pide a `/applications/{id}/rpc`).
- Efectos: un pedido de producto por perfil abierto, sin caché; las capas
  se animan juntas desde el principio (sin `start`/`loopDelay`).
- ~~Decoraciones: animadas sólo en el perfil abierto, no al pasar el
  puntero~~: también bajo el puntero (rama `perfil-tarjeta-y-cosmeticos`).
- GIF del servicio: una página de 50, sin «Más GIF». Hecho en la rama
  `emoji-y-estados` (2026-10-18), sin probar contra Discord.
- `USE_EXTERNAL_EMOJIS`/`USE_EXTERNAL_STICKERS` no se modelan: Discord
  rechaza si faltan. Hecho en `emoji-y-estados` (2026-10-18).
- Editar íconos y degradés de rol en la administración: hecho en P7
  (rama `administracion-2`).
- Prueba del dueño contra Discord (lista en DISCORD_SESSION.md).

## Paridad P7: administración restante (2026-10-17, rama `administracion-2`)

Hecho, sólo con fixtures (nada contra Discord), con Serein como referencia
de rutas y casos borde, sin copiar código; detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#resto-de-la-administración-p7-2026-10-17-rama-administracion-2)
y [serein-parity.md](serein-parity.md#p7-administración-que-falta-después-del-recorrido-19).

- Contrato (`rune-model/src/server_admin.rs`): `ServerOptions` en `Server`
  (canal y mensajes del sistema, notificaciones por defecto, inactividad,
  `Verification`, `MediaScan`, invitaciones pausadas, `features` de íconos y
  degradés de rol, comunidad, `ServerTrait`), `MemberQuery`/`MemberOrder`,
  `JoinMethod` y `ChannelMove`; `ForumDefaults.reaction`. Todo con
  `#[serde(default)]` (servidor viejo probado), sin cambio de tablas.
  `ServerEdit`, `ChannelEdit` y `RoleEdit` suman campos (`RoleIcon`);
  comandos `LoadServerProfile`, `ReorderChannels`, `LoadRoleCounts`;
  `LoadMembers` y `MembersLoaded` llevan su consulta (y las entradas);
  evento `RoleCounts`; fallos `ServerLoad` y `ChannelOrder`; capacidades
  `server_identity`, `server_safety`, `pause_invites`, `reorder_channels`,
  `forum_settings`, `role_styles`, `role_counts`, `member_queries`.
- Store: `ServerAbilities` suma `edit_identity`, `edit_safety`,
  `pause_invites`, `reorder_channels`, `role_icons`, `role_gradients`
  (también miran el `feature` del servidor), `role_counts` y
  `member_queries`; `Abilities.forum_settings`; las restricciones de la
  cuenta las niegan como al resto. `role_count` y las páginas de miembros
  por consulta (`MemberPages.query`, `listed`, `joins`).
- Simulación (`mock/server_admin.rs`): opciones y rasgos del servidor
  propio, un foro propio («propuestas», categoría «Comunidad»), ícono como
  nombre simulado, lote de canales todo o nada, foro con ids nuevos para las
  etiquetas, estilo de rol donde el servidor lo permite, conteo por rol, y
  miembros con fecha y forma de entrada deterministas en el orden pedido.
- Sesión: `admin/rest.rs` (comprobaciones y formato) y `http/admin.rs`;
  opciones leídas del `READY` y de `GUILD_UPDATE`; reacción por defecto del
  foro. Prueba de que un hCaptcha al pausar invitaciones se ofrece y se
  reintenta una vez con la respuesta (`tests/session/admin.rs`).
- Interfaz: páginas nuevas «Seguridad y participación» y «Canales»
  (arrastre y menú, guardado en lote) en los ajustes del servidor; ícono y
  rasgos en el perfil (con la tarjeta); botón para pausar invitaciones con
  su aviso; en el editor de rol, degradé (con vista previa) e ícono (imagen
  o emoji) después del color, y el conteo del proveedor en la lista; orden,
  «Últimos 7 días» y forma de entrada en miembros; página «Foro» en los
  ajustes de un foro, que ahora se abren también para foros. La nota del
  proveedor queda fija bajo la barra lateral de ajustes. Módulos nuevos:
  `rune-ui/src/server_settings/rest.rs` y `view/rest.rs`,
  `rune-ui/src/channel_settings/forum.rs`, `rune-components/src/server_admin.rs`.
- Capturas (Xvfb privado 1280×800, simulación):
  [perfil](../docs/img/server-profile-traits-2026-10-17.png),
  [seguridad](../docs/img/server-safety-2026-10-17.png),
  [canales](../docs/img/server-channels-order-2026-10-17.png),
  [rol](../docs/img/role-style-2026-10-17.png),
  [miembros](../docs/img/member-query-2026-10-17.png),
  [foro](../docs/img/forum-settings-2026-10-17.png),
  [invitaciones pausadas](../docs/img/invites-paused-2026-10-17.png).

Pendiente y por qué:

- ~~La barra lateral de canales sigue moviendo de a un canal~~: en lote
  desde el 2026-10-18 (rama `admin-canales-y-voz`).
- ~~Ícono del servidor animado (necesita el `feature` de mejoras) y banner;
  rasgos con emojis propios del servidor; reacción por defecto propia del
  servidor; activar comunidad; duración de archivado y modo de etiquetas de
  un foro.~~ Hecho el 2026-10-19 ([rama `admin-servidor-y-miembros`](#administración-del-servidor-y-miembros-2026-10-19-rama-admin-servidor-y-miembros)).
  ~~`lock_permissions` al mover~~: desde la barra lateral el
  2026-10-18; la página «Canales» sigue sin sincronizar.
- ~~La lista de miembros con orden o filtro elegidos muestra sólo lo que
  trajeron sus páginas: quien entra mientras tanto aparece al recargar.~~
  Hecho el 2026-10-19 (misma rama).
- ~~El orden del lote cuenta los canales que la cuenta ve~~: desde el
  2026-10-18 los ocultos conservan su posición y los visibles se numeran
  alrededor; si no hay lugar entre dos ocultos, uno puede correrse (nunca
  se mandan).
- Prueba del dueño contra Discord (lista en DISCORD_SESSION.md).

## Administración de canales y voz (2026-10-18, rama `admin-canales-y-voz`)

Sólo fixtures (loopback y simulación), sin probar contra Discord; Serein y la
REA del cliente oficial ([REA.md](../docs/REA.md#cliente-web-oficial-mover-canales-y-sincronizar-permisos-2026-10-18))
como referencia, sin copiar código.

- [x] **Barra lateral en lote**: el arrastre y «Mover a…» mandan
  `Command::ReorderChannels` con todo lo que cambia de lugar
  (`channel_list::move_to`), y dependen de `reorder_channels`. Al pasar a
  otra categoría, «¿Sincronizar permisos?» con «Sincronizar permisos»,
  «Mover sin sincronizar» y «Cancelar», en las mismas condiciones que el
  oficial. `ChannelMove.lock_permissions` (`#[serde(default)]`): la sesión
  lo manda con `parent_id` y pide `MANAGE_ROLES` en canal y categoría; la
  simulación copia los permisos de la categoría.
- [x] **Ocultos**: `server_settings::rest::layout_moves` numera cada grupo
  alrededor de los canales que la cuenta no ve
  (`Store::channel_tree_with_hidden`): conservan lugar y posición y nunca se
  mandan; los visibles quedan en orden siempre. Lo usan la barra y la
  página «Canales». Sólo se numeran de nuevo los grupos cuyo orden cambió
  (el de las categorías, o el de la categoría a la que llega el canal o en
  la que cambia de lugar): los demás, con huecos o no, no se mandan, y una
  categoría ajena sin `MANAGE_CHANNELS` no frena el lote. «¿Sincronizar
  permisos?» guarda sólo el destino y arma el lote al responder, con lo que
  haya entonces; se cierra si se borra el canal o la categoría.
- [x] **Arrastrar personas entre canales de voz** (`rune-ui/src/voice_drag.rs`):
  la fila de alguien se levanta si la cuenta puede moverlo
  (`Store::voice_moderation`); mientras se arrastra, cada canal de voz y su
  gente toman la caída con zonas (`vz:drop:{canal}:{parte}`, como las del
  riel); anillo (`DragMark::Inside`) donde caería y atenuado donde no puede.
  Manda `ModerateVoice::Move`, como «Mover a», que sigue.
- [x] **Permisos de canal**: el ⊕ y el diálogo de canal privado de los
  ajustes del canal, y quién puede ver un canal privado al crearlo
  (`server_menu::lookup`), buscan en el proveedor (`SearchMembers`, op 8)
  lo escrito, una vez por nombre; los encontrados se suman a las opciones.
- [x] **Auditoría por permisos**: los tipos 13, 14 y 15 por separado y
  mezclados por id (`admin::audit_merge`), con el límite pedido.
- Pendiente: la página «Canales» no ofrece sincronizar permisos (mueve sin
  `lock_permissions`); el arrastre de canales en la barra sigue decidiendo
  arriba o abajo por la dirección, no con zonas: las zonas del riel
  sirven para soltar *dentro* de algo (una persona en un canal de voz), y un
  canal sólo entra en una categoría, que ya lo toma al soltarlo sobre su
  título; la línea de inserción, probada, ya marca dónde cae. Sin captura
  nueva (el diálogo y el anillo necesitan un arrastre o permisos que la
  captura de la simulación no arma). Probar contra Discord: lista en
  [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#resto-de-la-administración-p7-2026-10-17-rama-administracion-2).

## Estabilidad (2026-10-17, rama `estabilidad`)

- [x] **Test de voz inestable** (`a_watched_stream_brings_decryptable_h264…`):
  no era el PLI ni `PLI_GAP`. El servidor sintético responde latidos sólo
  cuando el test lee el socket; con 200 ms de intervalo, mientras el test
  esperaba el PLI por UDP un latido quedaba sin respuesta, la conexión daba
  (con razón) al servidor por muerto y se quedaba esperando una reanudación
  que nadie atiende. Los cuatro tests de medios usan un latido por minuto
  (`QUIET`). Reproducido fijado a un núcleo con cuatro bucles ocupados:
  1/40 fallaba antes, 0/80 después. Producción no cambia.
- [x] **Salto del timeline al final**: no hay salto visible. La fila que
  trae la flecha nunca se había medido y es 288 px más alta que su
  estimación; la lista medida de limn mueve el desplazamiento para que lo
  visible quede quieto, así que el número sube 240 px mientras las filas
  bajan exactamente un paso. El test vuelve a probar desde el final y mira
  las filas en pantalla. limn no cambia.
- [x] **Ajustes con proveedores reales**: el host no les daba SQLite y Core
  lo abría sólo con `storage_identity`. Ahora el archivo se abre con
  cualquier proveedor; sin identidad, sólo para ajustes (caché, borradores,
  instantánea y marcas siguen apagados). Los de la cuenta
  (`rune_ui::ACCOUNT_SETTINGS`) van bajo `account:{proveedor}:{id}:` y se
  devuelven al entrar la cuenta (`apps/desktop/src/kept.rs`). Sin cambio
  de esquema. Ver [ARCHITECTURE.md](../docs/ARCHITECTURE.md#estado).
- [x] **Invitación pedida por `discord-ipc`**: entra a la cola de enlaces
  de otros arranques y se abre en el diálogo de invitación de RUNE (vista
  previa y aceptar con un clic de la persona).
- [ ] **Arrastrar y soltar**: sigue bloqueado. eframe/egui-winit 0.37.0
  salió, pero sigue en winit 0.30.13 (sin arrastre en Wayland, sin
  posición en X11) y con su portapapeles fijo, sin inyección de otro
  dueño. Destrabarlo pide parchear egui dentro de RUNE, que no se hace.
  Ver [FILES.md](../docs/FILES.md#arrastrar-y-soltar).
- ~~Riesgo que queda: `a_call_handshakes_secures…` (el test de latidos y
  reanudación, que necesita latidos de 200 ms) sólo falla con hambre de CPU
  extrema (un núcleo, cuatro bucles ocupados y los 48 tests de voz a la
  vez); con el binario entero en uno o dos núcleos pasó siempre.~~
  Resuelto el 2026-10-18: ver «Inicio de sesión, idiomas y robustez».

## Idiomas: español e inglés (2026-10-17, rama `idiomas`)

Último ítem de paridad de P6. Hecho y probado con la simulación, fixtures y
capturas en Xvfb privado; nada contra Discord ni en un escritorio real.
Diseño en [ARCHITECTURE.md](../docs/ARCHITECTURE.md#idiomas).

- [x] Infraestructura: crate `rune-i18n` sin dependencias, debajo de todos.
  El español es la clave (`t`, `tr!` con `{lugares}` con nombre, `es` para
  tablas `const` y textos que una capa de abajo pasa hacia arriba); el
  inglés en `crates/rune-i18n/src/en/`, un archivo por zona. Serein usa
  Fluent con 13 catálogos y claves con nombre: con dos idiomas que comparten
  la regla del plural alcanza una tabla, y el texto queda legible donde se
  usa. Plurales con una clave por forma; fechas (meses, días, «hace…», «Hoy
  a las…», orden de la fecha corta, a. m./p. m.) como claves; 12/24 h
  sigue siendo su ajuste.
- [x] Ajuste «Idioma» en Idioma y hora: Sistema (con el idioma detectado),
  Español, English. Ajuste del equipo (`language`, fuera de
  `ACCOUNT_SETTINGS`); el sistema por `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` o
  `LANG`, español si no es ninguno de los dos. Se aplica en el mismo cuadro
  (la vista fija el idioma de su hilo; el host el del proceso para core,
  proveedores y bandeja). `--language es|en` para capturas.
- [x] Capas: cada una traduce donde arma el texto. Lo que se arma una vez y
  se muestra mucho tiempo viaja como clave y se traduce al mostrarse
  (nombre y aviso del proveedor, motivos de desconexión, errores de texto
  fijo en su `Display`, opciones de proveedor y problemas del host). El
  login y el captcha reciben el idioma en `RUNE_LANGUAGE`; su página es una
  plantilla con los textos escapados, sin tocar la CSP. La bandeja recibe
  el idioma en `TrayState::language`. Los textos propios de limn
  (`limn::set_labels`) siguen al idioma en cada cuadro.
- [x] Traducción de todo el texto de interfaz: 3704 entradas (unos 3400
  textos distintos) en 16 archivos: componentes 1021, ventana 1428,
  proveedor y simulación 338, estado/core/medios 370, sesión/login/
  plataforma 440, host 65, calendario y comunes 42.
- [x] Tests: `rune-i18n/tests/catalog.rs` (cada texto marcado con su inglés
  y al revés, mismos `{lugares}`, una sola traducción por clave, sin
  letras del español en el inglés); `rune-ui/src/tests/english.rs` (39
  pantallas en inglés sin restos de español salvo contenido simulado, y el
  cambio en vivo); fechas en inglés en `rune-components/src/time.rs`. Los
  tests siguen en español aunque la máquina esté en inglés.
- [x] Capturas en inglés (simulación, Xvfb privado 1280×800):
  [general](../docs/img/idiomas-general-en-2026-10-17.png),
  [Idioma y hora](../docs/img/idiomas-ajuste-idioma-en-2026-10-17.png),
  [roles del servidor](../docs/img/idiomas-ajustes-servidor-en-2026-10-17.png),
  [voz y video](../docs/img/idiomas-voz-en-2026-10-17.png).

Sin traducir, a propósito:

- Contenido: mensajes, nombres de personas, servidores, canales y roles,
  comandos, botones y menús de bots, también los de la simulación (sólo
  su etiqueta «(simulado)» se traduce).
- La sintaxis de búsqueda: acepta `from:`/`de:`, `has:`/`tiene:`,
  `in:`/`en:`; las sugerencias muestran la inglesa.
- El panel de métricas y la salida del benchmark (desarrollo), registros
  de diagnóstico, ids y protocolo.
- «Español» en la lista de idiomas (cada idioma con su nombre).

Pendiente y por qué:

- ~~Lo que un proveedor arma una vez y guarda (el plan «Plus (simulado)»,
  «Conversación», «Servidor no disponible», nombres de paquetes simulados)
  queda en el idioma de cuando se armó hasta reconectar~~ (resuelto el
  2026-10-18); avisos pasajeros ya mostrados, todavía.
- ~~Algunos textos con dos sentidos cambiaron su español sin que lo mire un
  test~~ (fijados por un test el 2026-10-18); el resto comparte un inglés
  que sirve para los dos («Mostrar» → «Show» en la pestaña del rol, que
  Discord llama «Display»).
- Más idiomas: fuera de alcance (decisión del dueño).
- Prueba del dueño: cambio de idioma en vivo en la ventana real, el
  «Sistema» con su sesión (con `LANG=en_US` arranca en inglés), la bandeja
  en su panel, y las ventanas de login y captcha en inglés contra Discord.

## Voz real: primera prueba del dueño (2026-10-17, rama `voz-real`)

Evidencia y detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#primera-prueba-real-y-diagnóstico-de-voz-2026-10-17-rama-voz-real).
Todo con fixtures en loopback; **sin probar contra el servicio real**.

Hecho:

- Diagnóstico de voz siempre activo en stderr (`rune: voice call|stream|share: …`
  y `rune: gateway: …`): cada paso de la unión, el apretón de manos, DAVE
  (sólo opcodes), cierres, resume, reunión y motivo final; sin secretos ni
  IP propia (test).
- Endpoint de voz y de transmisión con cualquier puerto de
  `*.discord.media` (`:80` → 443), como el cliente oficial y Serein; uno
  inválido termina el intento al momento, con motivo.
- Sesión de este dispositivo por la del Gateway principal (regla del
  oficial); sin ella, la de Serein (candidato más nuevo hasta que el servidor
  de voz lo acepta; estados viejos ignorados). Tests de las carreras en los
  dos órdenes.
- Reunión tras cualquier fallo de una llamada que conectó (salvo 4014), con
  Identify nuevo con la misma sesión y servidor más op 4.
- IP discovery con reintentos, ACK tarde tolerado una vez, opcodes
  desconocidos ignorados, `channel_id` en Identify y resume, op 5 tras resume.
- Transmisión propia: `rtc_server_id` nulo espera, token hasta 2048 bytes.
- «Reconectando la voz…» ya no queda pegado tras un corte breve del Gateway.

Pendiente y por qué:

- Prueba del dueño con el log (qué mandar: ver el doc). La causa del
  primer fallo es una inferencia (puerto del endpoint) hasta ver un log.
- Op 5 `VOICE_SERVER_PING` del Gateway principal en cada corte, como el
  oficial: no se agregó porque su efecto en el servidor no se puede ver
  estáticamente.
- ~~Mandar op 18 recién cuando el portal devuelve la fuente~~: hecho en la
  rama `audio` (2026-10-17), ver abajo.
- `?v=9` del gateway de voz y `aead_aes256_gcm_rtpsize`: se quedan como
  Serein (v8, XChaCha20) hasta tener una prueba real.
- La cuenta dos veces en el canal: por el código son dos ids distintos
  (probablemente otra cuenta llamada igual); confirmar con el log
  (`CLIENTS_CONNECT (op 11)`).

## Perfil con una cuenta real (2026-10-17, rama `perfil-real`)

Primera prueba del dueño con su cuenta de Discord (proveedor no oficial):
seis problemas de la tarjeta de perfil, más lo que mostró la REA estática
del popout oficial. Detalle en
[PROFILES.md](../docs/PROFILES.md#primera-prueba-con-una-cuenta-real-2026-10-17-rama-perfil-real),
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#tarjeta-de-perfil-con-una-cuenta-real-2026-10-17-rama-perfil-real),
[REA.md](../docs/REA.md#cliente-web-oficial-popout-de-perfil-2026-10-17) y
[FONTS.md](../docs/FONTS.md#fuentes-del-sistema-como-respaldo-2026-10-17).

Hecho (sólo fixtures y simulación):

- [x] Parpadeo de la tarjeta de la lista: queda donde se abrió aunque la
  lista se mueva o pierda la fila; también la del autor de un mensaje
  (dos tests que fallaban antes del arreglo).
- [x] Voz: avatar en filas y mosaicos; la tarjeta pide perfil, nota e
  imágenes; la nota que no se pudo leer lo dice; el `member` de un estado
  de voz se conoce; la tarjeta suscribe a su persona (op 37 `members`)
  para su estado.
- [x] Biografía con los emoji del servidor como imagen (animados), tres
  líneas y «Ver biografía completa»; emoji Unicode del estado.
- [x] Actividad: `mp:` con codificación por ciento a 160 px, la chica como
  grande, tamaños oficiales; nada se pisa (test).
- [x] Efecto: a todo el ancho con su forma, capas en su tiempo, nada hasta
  que cargan, `reducedMotionSrc`.
- [x] Fuentes del sistema como respaldo para caracteres sin glifo.

Pendiente y por qué:

- ~~PR a limn de imágenes dentro de `TextSpan`~~: hecho en
  `emoji-en-linea` (PR #79), con la biografía en un solo texto y los
  emoji como imagen en los mensajes.
- ~~Ícono de la aplicación sin imágenes de actividad~~ (hecho en
  `emoji-en-linea`); ~~portadas de Spotify/Twitch/YouTube~~ (hechas en
  `perfil-tarjeta-y-cosmeticos`).
- ~~Emoji de servidor en el estado personalizado~~ (hecho en
  `emoji-en-linea`, `Presence.custom_emoji`).
- ~~Ancho de 300 px, cuerpo desplazable y cerrar al desplazar, como el
  popout oficial; nameplates por sku y animados~~: hechos en
  `perfil-tarjeta-y-cosmeticos` (ver abajo).
- Probar todo contra Discord con la cuenta del dueño (lista en
  DISCORD_SESSION.md).

## Audio: supresión de ruido, sonidos y pantalla compartida (2026-10-17, rama `audio`)

Pedido del dueño tras la primera prueba real. Detalle y números en
[PERFORMANCE.md](../docs/PERFORMANCE.md#supresión-de-ruido-medida-2026-10-17-rama-audio),
[MEDIA.md](../docs/MEDIA.md#supresión-de-ruido-mejorada-2026-10-17) y
[MEDIA.md](../docs/MEDIA.md#sonidos-de-la-llamada-y-carpeta-propia-2026-10-17).
Nada de esto se escuchó ni se probó contra Discord.

Hecho:

- Supresión de ruido medida con voz y ruidos sintéticos deterministas
  (`rune-media::processing::eval`, tabla con `--ignored`, un test que fija
  lo esencial). Captura siempre a 48 kHz (pedida o remuestreada) para que
  RNNoise corra con cualquier micrófono; compuerta de voz con la
  probabilidad de RNNoise y rechazo de clics (teclado abierto en pausas de
  59,7 % a 0 %), predeterminada con sensibilidad automática; la compuerta
  por nivel ya no abre con un clic; el AEC no reinicia RNNoise ni la
  ganancia. Ajustes explican la compuerta. `devices`/`playback --gate`.
- Sonidos de la llamada: trece eventos con interruptor, «Probar», volumen y
  carpeta propia (archivos por nombre de evento, validados fuera del hilo
  de la interfaz); tonos propios regenerables con `scripts/sounds.sh`.
- Pantalla compartida: op 18 recién cuando la persona eligió la fuente en
  el diálogo y llegan cuadros; diagnóstico cada 5 s (`rune: voice share:
  video …` y `… network …`); tasa que empieza en el máximo y sigue la regla
  de pérdida de WebRTC, cambio de tasa sin cuadro clave, captura con ritmo
  propio. Números en PERFORMANCE.md.

Pendiente y por qué:

- ~~Elegir la carpeta de sonidos con el diálogo del sistema~~: hecho en la
  rama `escritorio-linux` (2026-10-18), abajo.
- La compuerta pierde 1–2 % de las sílabas al empezar frases con
  fricativa («s», «f»); mirar 10 ms adelante casi no ayudó. Ajustar con la
  voz real del dueño.
- Escalado de la pantalla por vecino más cercano (1,2 ms por cuadro): fluido,
  pero escalonado en texto fino; uno bilineal si se nota.
- Las marcas RTP de la pantalla salen del momento en que el codificador toma
  el cuadro (hasta 4 ms después de capturado), no del de captura.
- Prueba del dueño: escucharse con «Escucharme» o
  `cargo run --release -p rune-media --example playback -- --loopback --rnnoise --gate`,
  una llamada real (cortes al empezar frases, teclado), los sonidos, y una
  transmisión con las líneas `rune: voice share:` para mandar si sigue poco
  fluida.


## Emoji en línea (2026-10-17, rama `emoji-en-linea`)

Los emoji de un servidor como imagen dentro del texto, con limn nuevo
(imágenes en un `TextSpan`), y el ícono de la aplicación de una actividad
sin imágenes. Detalle en
[PROFILES.md](../docs/PROFILES.md#emoji-en-línea-2026-10-17-rama-emoji-en-linea),
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#emoji-en-línea-y-el-ícono-de-la-aplicación-2026-10-17-rama-emoji-en-linea),
[REA.md](../docs/REA.md#cliente-web-oficial-emoji-en-línea-y-el-ícono-de-la-aplicación-2026-10-17)
y [LIMN_INTEGRATION.md](../docs/LIMN_INTEGRATION.md#imágenes-dentro-de-un-texto-pr-79-2026-10-17).

Hecho (sólo fixtures y simulación):

- [x] limn: `TextSpan::image` y `TextSpan::surface` (PR #79; RUNE en
  `rune/inline-images`, `f601989`).
- [x] Mensajes y texto de tarjetas: el emoji como imagen en la línea,
  animado, `:nombre:` mientras carga o si falla.
- [x] Mensaje que es sólo emoji (hasta 30): 48 px, salvo en modo compacto.
- [x] Respuesta citada con sus emoji a 1,25 veces su letra.
- [x] Biografía: un solo texto con Markdown y saltos de línea; emoji
  animados (antes, el cuadro quieto).
- [x] Estado personalizado con el emoji del servidor (de otros y propio;
  el propio ya no se pierde al cambiar de estado).
- [x] Ícono de la aplicación sin imágenes (`/applications/{id}/public`,
  caché por sesión).
- [x] Tema del canal con sus emoji, en el encabezado y en su diálogo
  (Discord los dibuja ahí; en los nombres de canal, no).
- [x] Simulación: dos emoji de servidor en #general, en el tema de
  #ayuda y en el estado de Ada;
  `--action last:emoji` para las capturas.

Pendiente y por qué:

- Línea de estado de la lista de miembros y de amigos: sigue sin el emoji
  del servidor (es una línea de texto recortada; habría que pasarla a
  texto formateado con recorte). Hecho en `emoji-y-estados` (2026-10-18).
- Fijados y extractos de búsqueda o bandeja: no usan las referencias del
  mensaje (o no son Markdown); los tokens se ven como vienen. Hecho en
  `emoji-y-estados`, salvo la imagen en los extractos de búsqueda (dicen
  `:nombre:`; bloqueado en limn, ver «Emoji y estados»).
- ~~El tema con emoji en el encabezado se corta sin «…»~~: resuelto el
  2026-10-18 (rama `limn-texto`, `Element::max_lines`).
- ~~Texto de un reenvío: el modelo no trae sus referencias.~~ Hecho en
  `contenido-de-mensajes` (2026-10-18).
- Jumbo en una pantalla escalada: la imagen es de 48 px (el oficial pide 96).
  Hecho en `emoji-y-estados`.
- Emoji Unicode jumbo: se agranda la letra (no hay imágenes de emoji
  Unicode); un keycap (1️⃣) no cuenta como emoji. Hecho en `emoji-y-estados`.
- Probar todo contra Discord con la cuenta del dueño (lista en
  DISCORD_SESSION.md).

## Llamadas privadas (2026-10-17, rama `llamadas-privadas`)

Llamadas en privados y grupos como en Serein y en el cliente oficial (REA
estática de su bundle web público; sin copiar código). Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#llamadas-privadas-2026-10-17-rama-llamadas-privadas),
[REA.md](../docs/REA.md#cliente-web-oficial-llamadas-privadas-2026-10-17)
y capturas `docs/img/llamadas-privadas-*-2026-10-17.png`.

Hecho (sólo fixtures y simulación; nada probado contra Discord):

- [x] Dominio y contrato: `PrivateCall`, capacidad `private_calls`,
  `JoinVoice` para privados y grupos, `RingCall`, `StopRinging`, eventos
  `PrivateCall`/`PrivateCallEnded`, fallo `Ring`; `Store::call_status`
  (sonando, en vivo, sin respuesta, rechazada, cortó, sola, entrante, en
  curso, en otro dispositivo), `incoming_calls`, `Abilities::ring`; un
  privado no pide permisos de canal para hablar, transmitir o filmar;
  nadie bloqueado recibe llamadas.
- [x] Proveedor no oficial: op 13 antes de unirse y al leer un privado;
  op 4 con `guild_id` nulo; `CALL_CREATE/UPDATE/DELETE` con
  `ongoing_rings` (oficial) y `ringing` (Serein); ring retenido hasta que
  Discord tiene la llamada y enviado por la cola REST común (hereda el
  captcha); `stop-ringing` para rechazar y «Dejar de llamar»; voz por el
  canal (`server_id`, DAVE); transmisiones `call:{canal}:{dueño}` con op 18
  de tipo `call`; diagnóstico `rune: voice private call: …` sin secretos.
- [x] Simulación determinista: Ada contesta (y corta a los 45 s), Bruno
  rechaza, Camila deja sonar hasta que el servicio se cansa (30 s); en el
  grupo, una de cada; «Pruebas de la simulación»: recibir una llamada y
  atenderla en otro dispositivo; quien llama se cansa a los 30 s.
- [x] Interfaz: teléfono en la cabecera; la llamada bajo la cabecera del
  privado con el escenario de los canales de voz (mosaicos, quién habla,
  silenciar, ensordecer, cámara, pantalla, colgar), mosaicos de quien
  suena con «Dejar de llamar», «Volver a llamar»; Aceptar/Rechazar,
  Unirse, «Unirse desde acá»; aviso entrante sobre la ventana; notificación
  del sistema sin foco (no en No molestar); textos en español e inglés.
- [x] Sonidos originales `ring` (cada 2,5 s, se oye ensordecido) y
  `ring_out` (cada 3 s) por `scripts/sounds.sh`, con interruptor y «Probar».

Pendiente y por qué:

- Probar todo contra Discord con otra cuenta o persona de confianza
  (lista en DISCORD_SESSION.md).
- «Rechazada» frente a «sin respuesta» es una inferencia por tiempo (20 s):
  Discord sólo saca a la persona de `ringing`; ajustar con lo que se vea.
- ~~Sin `GET /channels/{id}/call` (`ringable`) ni el diálogo de «no son
  amigos» del oficial.~~ Hecho en `llamadas-y-panel-de-voz` (2026-10-18).
- Rechazo del aviso oficial y comportamiento con otro dispositivo: el chunk
  que los tiene no estaba entre los descargados; se siguió a Serein.
- ~~Notificación del sistema sin botones Aceptar/Rechazar.~~ Hecho en
  `llamadas-y-panel-de-voz` (2026-10-18).
- ~~Mensaje de llamada perdida o duración (tipo de mensaje «llamada»).~~
  Hecho en `contenido-de-mensajes` (2026-10-18).

## Video por hardware y sonido de la pantalla (2026-10-17, rama `video-por-hardware`)

Diseño y hallazgos en
[VIDEO.md](../docs/VIDEO.md#video-por-hardware-2026-10-17-rama-video-por-hardware),
[el sonido](../docs/VIDEO.md#sonido-de-la-pantalla-sin-las-voces-de-la-llamada-2026-10-17-rama-video-por-hardware),
[REA.md](../docs/REA.md#video-por-hardware-y-sonido-de-la-transmisión-en-linux-2026-10-17)
y los números en
[PERFORMANCE.md](../docs/PERFORMANCE.md#video-por-hardware-2026-10-17-rama-video-por-hardware).

Hecho (fixtures y la GPU de esta máquina; nada contra Discord):

- [x] Vía: GStreamer 0.25 (como Serein), `shell.nix` con core, base y bad,
  rutas de plugins compiladas (`RUNE_GST_PLUGINS`).
- [x] Codificar por GPU con el contrato de `video::encode`: VA-API (la GPU
  integrada), después NVENC, OpenH264 de respaldo y si una GPU falla en
  medio; Constrained Baseline, clave a pedido, tasa nueva en marcha (NVENC
  sin clave; VA-API rehace la secuencia y espera a sumar −15 %/+25 %).
- [x] Ajuste «Aceleración por hardware de H.264» en Voz y video; la vista
  previa dice quién codifica.
- [x] Decodificar por GPU lo que se mira (VA-API, NVDEC, OpenH264), con el
  SPS ajustado para que GStreamer no retenga cada imagen hasta la siguiente.
- [x] Diagnóstico: `rune: voice share: video encoder …` con la caída, y
  `rune: voice watch: video decoder …`.
- [x] `encode_bench --hw|--va|--nvidia` con CPU por hilo; números antes y
  después.
- [x] Arreglado: el lector de SPS del proveedor no oficial leía un bit de
  más (`video_signal_type`) y habría roto el SPS de NVENC.
- [x] Sonido de la pantalla: RUNE se capturaba a sí mismo (el registro de
  PipeWire no trae el pid del nodo); ahora pide quién es cada cliente y deja
  afuera a RUNE, las llamadas (`Communication`, `Phone`) y otros clientes
  de voz; la línea `rune: voice share: sound:` dice qué se mezcla y qué no.

Pendiente y por qué:

- DMA-BUF de la captura a VA-API sin pasar por la CPU (VIDEO.md: la vista
  previa y el respaldo necesitan los píxeles, y en esta máquina la pantalla
  y NVENC pueden estar en GPU distintas).
- NVDEC convierte a RGBA en la CPU (sin NVRTC no hay conversión de CUDA):
  no ahorra CPU frente a OpenH264; queda de respaldo.
- Capturar sólo el sonido de la ventana compartida: el portal no dice de
  qué proceso es.
- Probar contra Discord: compartir 1080p60 con VA-API y con NVENC, mirar la
  transmisión de otro por la GPU, y el sonido de la pantalla con el cliente
  oficial abierto en la misma llamada (las líneas de log lo dicen).

## Pulido de la interfaz real (2026-10-17, rama `pulido-ui-real`)

Pedidos del dueño tras usar RUNE con su cuenta (capturas 9 y 11), cada
uno como el cliente oficial (REA estática en [REA.md](../docs/REA.md)).

- [x] Lista de miembros: 1 px libre arriba y abajo del fondo de cada fila
  (2 px entre fondos y placas) y radio 8; la tarjeta se ancla al fondo.
  Filas sin cargar: esqueleto del oficial (círculo y barra de 14, ancho
  según el lugar, pulso de limn).
- [x] Avatar por defecto de Discord desde el CDN (`embed/avatars/{i}.png`)
  en todos lados; la simulación conserva la inicial. Sólo fixtures.
- [x] Tooltip del riel: nombre, caras de quienes están en voz (24 px,
  máx. 6; con más, 5 y «+N») y de quienes transmiten, «Silenciado» al
  pie. limn `Tooltip::faces`, [PR #80](https://github.com/Niiihuel/limn/pull/80);
  RUNE apunta a `rune/tooltip-faces` (`1af991c`).
- [x] Esqueleto del historial con forma de mensajes, que llena la ventana
  y empieza en el final (`message::history_skeleton`); sin animación, como
  el oficial. Fijados cargando: spinner, como el oficial.
- [x] Pantalla de inicio hasta READY (`rune_ui::splash`) con etapa, motivo
  y botones a los 10 s, fundido al entrar; `--capture-splash <ms>`.
  Capturas en `docs/img/` (`splash-`, `esqueleto-historial-`, `rail-voz-`
  `2026-10-17.png`).

Pendiente y por qué:

- Tooltip: el canal AFK, los silenciados, las filas de escenario y de
  apps y el orden por canal quedaron hechos el 2026-10-18 (rama
  `riel-y-notificaciones`); el tooltip de limn abre con 350 ms de demora y
  título de 12 px (el oficial: al instante, 16 px). Opciones de limn
  aparte si el dueño lo pide.
- ~~Pantalla de inicio: «Reintentar» reinicia todo el proveedor (como la
  recarga del oficial); no hay «reintentar ya» dentro de la espera del
  Gateway. «Cambiar de cuenta» abre el login de una cuenta nueva; cerrarlo
  muestra la lista de cuentas recordadas.~~ Resuelto el 2026-10-18: ver
  «Inicio de sesión, idiomas y robustez».
- Probar con la cuenta real: avatares por defecto (miembros, mensajes,
  webhooks, voz, privados), separación de nameplates, tooltip con gente en
  voz, pantalla de inicio al restaurar la sesión y con la red cortada.

## Voz como Serein: panel rápido y prueba de sonido (2026-10-18, rama `voz-estilo-serein`)

Pedido del dueño: el diseño visual y funcional de Serein para la voz y la
prueba de sonido. Detalle en
[MEDIA.md](../docs/MEDIA.md#implementado-panel-rápido-de-voz-y-prueba-de-sonido-como-serein-2026-10-18-rama-voz-estilo-serein).

Hecho (simulación y tests sin ventana; nada contra Discord):

- [x] Panel rápido desde la flecha del micrófono y de los auriculares, en
  el panel de la cuenta y en la barra de la llamada, con nivel en vivo y la
  marca del umbral (Serein no lo tiene).
- [x] Modo de entrada como selector, supresión de ruido, actualizar
  dispositivos y restablecer niveles, desde el panel.
- [x] Listas con «Predeterminado del sistema», hasta 32 dispositivos,
  nombres recortados y «Dispositivo no disponible».
- [x] Volúmenes de 0 a 200 % con el valor a la derecha y paso de 1 %.
- [x] Prueba de micrófono como Serein (dBFS, verde/amarillo/rojo) con la
  marca del umbral; funciona en llamada; «Escucharme» se desactiva en una
  llamada con audio real.
- [x] Prueba de salida con el sonido de entrar a la llamada, también en
  llamada; se quitó el tono que cortaba su salida.
- [x] Página de Voz en el orden de Serein, con la aceleración por hardware
  en «Cámara y video»; ninguna opción perdida.
- [x] Componentes puros en `rune-components::quick_voice` con tests; textos
  en español con su inglés (`rune-i18n/src/en/voice_panel.rs`).

Pendiente y por qué:

- Pista con el nombre entero en cada opción de la lista: hecho el
  2026-10-18 con `DropdownOption::tooltip` (paquete 10).
- Vista previa de la cámara en espejo y ventana emergente de cámara desde
  la barra (Serein): la ventana emergente está hecha en
  `llamadas-y-panel-de-voz` (2026-10-18); el espejo, el 2026-10-19
  (`limn-teclado-y-superficies`).
- ~~La altura del cuerpo del panel es fija (420 px).~~ Hecho en
  `llamadas-y-panel-de-voz` (2026-10-18): en ventanas bajas se desplaza.
- Probar con un micrófono real: el medidor en vivo y la marca del umbral,
  la prueba en una llamada de Discord, y «Probar sonido» durante una.

## Profesionalización (2026-10-18, rama `profesionalizacion`)

Dejar el repositorio verificable y presentable sin tocar código de producto.

Hecho:

- [x] CI de GitHub Actions en Linux (`.github/workflows/ci.yml`): en cada
  push y pull request, con el shell del repositorio (`nix develop`) y Rust
  1.95.0: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D
  warnings` (hoy limpio) y `cargo test --workspace` sin los `#[ignore]`.
- [x] `cargo deny check` en la CI con un `deny.toml` propio: licencias
  reales, fuentes (crates.io y limn), wildcards sólo en rutas propias y
  avisos ignorados con motivo (RUSTSEC-2024-0384 y
  RUSTSEC-2026-0370/0371/0372). Local con cargo-deny 0.19.9: todo `ok`
  ([SECURITY.md](../docs/SECURITY.md#cargo-deny-en-la-ci-2026-10-18)).
- [x] README público: advertencia de cliente no oficial arriba, estado
  corregido (voz y archivos sí existen), capturas de la simulación,
  instalación con proveedores y features, licencia, qué es simulado y qué
  no se probó contra Discord.
- [x] Registro a archivo (`apps/desktop/src/log.rs`): stderr también en
  `$XDG_STATE_HOME/rune/rune.log`, 5 MB × 3 archivos, carpeta 0700 y
  archivos 0600, sólo en ejecuciones interactivas en Unix; sin dependencia
  nueva. El navegador de `xdg-open` ya no hereda stderr. Tests: rotación,
  permisos, copia con la terminal rota y el descriptor 2 real en un proceso
  hijo ([PLATFORM.md](../docs/PLATFORM.md#registro-a-archivo-2026-10-18)).
- [x] Docs que contradecían el código: PLATFORM (ajustes con cualquier
  proveedor), MEDIA (`restore_token`, codificación, escucha propia y
  reproducción), FILES (visor animado, gifv, «Guardar como»),
  DISCORD_SESSION (parche de `hpke-rs`, emoji en línea y del estado),
  ARTWORK (spoilers, animación, stickers), SEREIN_COMPARISON (P0–P2 al día,
  selector de proveedor, autorizaciones posteriores) y ROADMAP (estado y
  orden del dueño del 2026-10-18).
- [x] `todo.md`: casillas de 11, 12 y 14 (falta prueba real) y las líneas
  viejas de 17, 20 y 22; `serein-parity.md`: el actualizador pasa a
  «Pedido, para después».
- [x] Revisión (rama `profesionalizacion-fix`): un pánico espera hasta
  200 ms a que el hilo del registro copie lo pendiente, así llega a la
  terminal y al archivo aunque después aborte (test con un proceso hijo
  que sale sin soltar el registro); cabecera `--- rune start` por arranque,
  sólo en el archivo; `~/.local/state` se crea 0700. La CI corre clippy con
  todas las features en el shell `session` (la WebView de login y
  hCaptcha), con `--locked`, acciones fijadas por commit y sólo en push a
  `main` y pull requests; MPL-2.0 se permite crate por crate. Docs: qué
  más guarda `rune.log` y que sobrevive a cerrar sesión (SECURITY), el
  límite de los abort de C (PLATFORM), credencial en el llavero
  (ARCHITECTURE, SEREIN_COMPARISON), escenarios (SEREIN_COMPARISON),
  emojis, stickers y GIF de P5 (DISCORD_SESSION) y la captura de acceso
  del README, que no es de la simulación.

Pendiente y por qué:

- La CI no corrió todavía en GitHub (el repositorio no se sube desde acá):
  el primer push dirá si el runner tiene disco y tiempo para el workspace.
  Los tests corren sin `--all-features` (el shell por defecto no trae
  WebKit); clippy sí las cubre.
- Un abort o una señal del código de C (driver, GStreamer) puede cortar lo
  último que quedaba en la tubería del registro: sin arreglo mientras
  stderr pase por ella ([PLATFORM.md](../docs/PLATFORM.md#registro-a-archivo-2026-10-18)).
- `inout 0.2.3` está retirado de crates.io (advertencia de `cargo deny`):
  `cargo update -p inout` cuando se toque el `Cargo.lock` por otra cosa.
- Avisos ignorados en `deny.toml`, bloqueados aguas arriba: `instant` (por
  `davey`) y zbus 5 (por accesskit y `secret-service`).
- `rune-media` `video::decode::tests::a_key_frame_decodes_to_its_size_and_color_and_late_deltas_ask_for_another`
  agotó una vez su espera de 5 s con la máquina cargada (dos builds a la
  vez); solo y en la corrida siguiente pasa. En la CI puede pasar lo mismo:
  si se repite, alargar la espera o medir por cuadros en vez de por tiempo.
- El registro a archivo es sólo Unix (`dup2`) y busca la carpeta por XDG:
  Windows lo suma con su build y macOS tendrá que usar su carpeta propia.

## Adopción de limn: lo que ya trae la rama fijada (2026-10-18, rama `adopcion-limn`)

La rama `rune/tooltip-faces` (`1af991c`) ya traía piezas que RUNE componía
a mano con un `// limn-gap:`. Sin tocar limn ni `Cargo.toml`. Detalle en
[LIMN_INTEGRATION.md](../docs/LIMN_INTEGRATION.md#adopción-de-lo-que-trae-runetooltip-faces-2026-10-18).

Hecho (simulación y tests sin ventana; nada contra Discord):

- [x] Íconos de limn en lugar de los SVG propios: candado, «heredar»
  (`Icon::Neutral`), chevrón hacia arriba, invitar, salir y agarre.
- [x] Candado en los canales privados de la barra (`@everyone` sin
  `VIEW`), también junto al ojo de un canal oculto que se lista, con
  `Store::private` en `rune-state`; los ajustes del canal usan la misma
  regla. Capturas: `docs/img/adopcion-limn-candado-2026-10-18.png` y
  `docs/img/adopcion-limn-roles-2026-10-18.png`.
- [x] Menús: casillas y radios de verdad, segunda línea en los ítems con
  submenú, menús del clic derecho con `DropdownMenu::at`, motivo de un ítem
  apagado con pista (y la plataforma lo oye entero); el menú de estado se
  abre al pasar el puntero y no se cierra mientras se eligen duraciones.
- [x] El clic derecho en un mensaje abre el menú completo (reacciones
  rápidas, submenú de reacciones, «Eliminar» en rojo) donde se hizo el clic.
- [x] Formularios: buscador de ajustes con limpiar de limn, `Select` como
  campo (modo lento y listas de los ajustes), radios con ícono, interruptor
  `Spacious` claro sobre el acento, slider con marcas y etiquetas de limn.
- [x] Color: muestras de limn en el servidor nuevo; selector de color
  completo (saturación y brillo, tono, hex) en los seis colores
  personalizados (cinco borradores en vivo; el color propio del avatar se
  pide al proveedor al soltar el selector); pestañas subrayadas de limn en
  el editor de roles, que dicen «Miembros (N)» cuando la larga no entra.
- [x] `Markdown::font_size` en mensajes y tarjetas; test de que `# Título`
  sale como encabezado.
- [x] La barra del chat se oculta (`ScrollbarVisibility::AutoHide`).
- [x] Inventario de carencias al día en LIMN_INTEGRATION.md y DISEÑO.md.

Pendiente y por qué: lo de limn quedó hecho en el paquete 8
([limn-avisos-y-compositor](#limn-avisos-insignias-íconos-chicos-nivel-del-slider-y-compositor-2026-10-18-rama-limn-avisos-y-compositor)):
interruptor de 50×26, `Select` con cinco filas a la vista, valor a 16 px,
chevrón hacia arriba y borde de acento, candado chico sobre el `#`, barra
del chat que aparece bajo el puntero, y el menú de un mensaje que se cierra
cuando su fila sale de la lista (también «Más»).

- Probar contra Discord: el candado con permisos reales y el menú del clic
  derecho de un mensaje en la ventana (el punto del clic lo pasa el host).

## Inicio de sesión, idiomas y robustez (2026-10-18, rama `inicio-sesion-e-idiomas`)

Hecho y probado con la simulación, el llavero falso de los tests y
fixtures en loopback; nada contra Discord. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#avatar-por-defecto-y-pantalla-de-inicio-2026-10-17-rama-pulido-ui-real).

- [x] **«Cerrar sesión» mientras se verifica la cuenta guardada** (regla
  del llavero de AGENTS.md): el proveedor recuerda qué cuenta restaura
  (también si su verificación no pudo terminar) y `SignOut` la borra del
  llavero con el mismo `forget` y `VAULT_TIMEOUT`; `UseAccount` y
  cancelar la conservan. La pantalla de inicio pregunta antes de cerrar
  sesión. Tests: `tests::keyring::signing_out_while_…`,
  `switching_while_…`, `signing_out_after_…`.
- [x] **«Cambiar de cuenta» muestra primero las cuentas recordadas**, con
  «Agregar cuenta» (la página de login) y «Volver», en la misma pantalla
  de inicio (`splash::Asking`).
- [x] **«Reintentar ya»** dentro de la espera del Gateway entre intentos:
  `Command::ReconnectNow` (declarado con `Capabilities::reconnect_now`)
  corta la espera sin rehacer el proveedor; la sesión se reanuda. Un
  pedido fuera de la espera no queda guardado. Se ofrece en el aviso
  «Se perdió la conexión» sobre la conversación: la sesión conoce la
  cuenta (y la pantalla de inicio se va) antes de arrancar su Gateway;
  la pantalla de inicio lo ofrece sólo a un proveedor que espere antes de
  conocerla. El apuro respeta los 5 s entre IDENTIFY cuando no hay sesión
  que reanudar, y un intento apurado que falla antes del READY no gasta
  los reintentos. Tests: `gateway::tests::trying_again_now_…` (los dos),
  `tests::session::gateway::reconnect_now_…` (del comando al Gateway) y
  `tests::splash::inside_the_gateways_wait_…` (con el orden real de
  eventos de la sesión). La simulación no lo declara: su pérdida simulada
  espera en el núcleo (`supervise`), sin sesión que apurar.
- [x] **Textos armados por el proveedor**: «Conversación» y «Servidor no
  disponible» se guardan en español (`Channel::UNNAMED`,
  `Server::UNNAMED`) y se traducen donde se muestran
  (`Store::channel_title`, sólo fuera de servidores, y `Server::shown_name`
  donde la interfaz muestra el nombre de un servidor: riel, cabecera,
  iniciales, privacidad, carpetas, buscador de ajustes, bandeja,
  búsqueda, perfiles, expresiones, ajustes del servidor); la
  notificación de un grupo usa `channel_title`. En la simulación, el plan
  «Plus (simulado)» y los nombres de los paquetes de stickers, igual.
  Test en `rune-state`.
- [x] **Textos con dos sentidos** fijados en `rune-i18n/tests/catalog.rs`:
  «Hilo bloqueado»/«Bloqueado», «Activar el sonido»/«Activar sonido»,
  «Todo el mundo»/«Todos», su español donde se usan y su inglés.
- [x] **Ningún `Command` declarado cae en «Esta acción todavía no está
  disponible»**: `tests::session::unavailable` manda uno de cada (110) a
  una sesión con un servidor propio, rechaza cada pedido y falla si
  alguno termina en el rechazo genérico; uno que nadie espera debe llegar
  a su pedido. Una clasificación sin comodín obliga a ubicar cada
  `Command` nuevo, y su brazo `None` es la lista de la macro
  `classified!`: un comando puesto ahí sin muestra en `samples` hace
  fallar el test. Probado contra dos roturas a propósito (atrapadas).
- [x] **Test de voz bajo hambre de CPU**: `a_call_handshakes_secures…`
  usa el latido de un minuto (el cliente late cada 5 s como mucho), así
  que dos latidos sin respuesta superan cualquier espera del test. Con
  ese latido sólo sale el primero (que reconoce el Hello), así que lo
  leído se comprueba en la reanudación: su `seq_ack`, el mismo que llevan
  los latidos, es exactamente el del último mensaje mandado.

Pendiente y por qué:

- Avisos pasajeros ya mostrados y lo que la simulación arma al empezar
  (mensajes, escenario, comandos de bots con su «(simulado)») quedan en el
  idioma de entonces: contenido o avisos de segundos, no vale guardarlos
  aparte. El nombre de rol de reserva «Rol» de la sesión (Discord siempre
  manda el nombre) tampoco sigue el idioma.
- Los nombres de reserva se reconocen por su texto: un servidor que se
  llame justo «Servidor no disponible», un grupo llamado «Conversación» o
  un paquete de stickers real cuyo nombre sea una clave del catálogo se
  muestran traducidos en inglés. Distinguirlos de verdad pide un campo
  nuevo (o un nombre vacío, que dejaría en blanco cualquier lugar que lea
  el nombre crudo); los canales de servidor ya no se comparan.
- «Reintentar ya» no está para la espera del núcleo tras una pérdida del
  proveedor entero (ahí está «Reintentar»).
- Probar con la cuenta real: «Cerrar sesión» en la pantalla de inicio
  mientras se verifica la cuenta guardada (al volver a abrir RUNE, la
  cuenta ya no está en el llavero ni en la lista); «Cambiar de cuenta»;
  «Reintentar ya» con la red cortada y vuelta.

## Texto de limn: marcas, tooltip de un tramo, «…» y relleno (2026-10-18, rama `limn-texto`)

Cuatro carencias de limn que RUNE rodeaba en `rune-components/src/text.rs`,
resueltas en limn ([PR #82](https://github.com/Niiihuel/limn/pull/82),
rama de integración `rune/limn-texto`) y adoptadas. Detalle en
[LIMN_INTEGRATION.md](../docs/LIMN_INTEGRATION.md#marcas-del-texto-tooltip-de-un-tramo--y-relleno-del-resaltado-pr-82-2026-10-18).

Hecho (simulación y tests sin ventana; nada contra Discord):

- [x] Markdown: `__subrayado__`, `~~tachado~~`, `***negrita cursiva***` y
  `-# subtexto`, anidables; la negrita y la cursiva usan sus caras. La
  simulación tiene un mensaje con las cuatro en #general.
- [x] Un enlace dentro de `||…||` se oculta con el spoiler y funciona al
  mostrarlo: el corte estaba en limn (la dirección se llevaba el `||`).
- [x] Tooltip en un tramo: la fecha completa de `<t:…>` y la hora de la
  edición en «(editado)». La fecha conserva la acción `moment:` (aviso al
  hacer clic o con Enter) para que el teclado llegue a ella y vea el
  tooltip.
- [x] Un emoji de servidor con `__` en el nombre se resuelve entero aunque
  el párrafo tenga otro `__` (limn no abre marcas dentro de `<…>`), y dos
  textos con el mismo tooltip no muestran dos (revisión, 2026-10-18).
- [x] «…» por cantidad de líneas en texto formateado: el tema del canal con
  emoji en el encabezado.
- [x] Relleno del resaltado: cada `@palabra` y `#palabra` del borrador se
  ve sobre el relleno de una pastilla.
- Capturas: [mensaje](../docs/img/limn-texto-2026-10-18.png) (el texto del
  mensaje de la simulación, escrito en el compositor) y
  [borrador](../docs/img/limn-texto-composer-2026-10-18.png).

Pendiente y por qué:

- `_cursiva_` con un guion bajo y las citas `>>> ` de varias líneas, que
  Discord también tiene: no estaban en este paquete; serían otra PR a limn.
- Los encabezados `#`–`###` de limn no van en negrita (Discord sí).
- La línea de estado de miembros y amigos con emoji puede usar
  `max_lines(1)`: queda para el paquete emoji-y-estados.
- El tooltip de un tramo no lo lee un lector de pantalla (el texto sí).
  «(editado)» no se enfoca con el teclado, así que su hora sólo se ve con
  el puntero (como en el cliente oficial).
- El borrador rellena cualquier `@palabra` o `#palabra`, aunque no nombre a
  nadie ni a ningún canal: no sabe qué nombres existen.
- Cuatro carencias en una sola PR (#82), no en cuatro como pedía el plan:
  comparten tipos y código (ver LIMN_INTEGRATION.md); se separan apiladas
  si el dueño lo pide.
- `rune/limn-texto` (`152c5a8`) se subió a Niiihuel/limn al mezclar
  (2026-10-18), y `Cargo.toml` apunta ahí.
- Probar contra Discord: mensajes reales con estas marcas, un enlace dentro
  de un spoiler y el tooltip de una fecha `<t:…>` y de «(editado)».

## Reacciones, fijados y búsqueda remota (2026-10-18, rama `reacciones-fijados-y-busqueda`)

Paquete 7 del plan de construcción. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#reacciones-súper-reacciones-fijados-y-búsqueda-2026-10-18-rama-reacciones-fijados-y-busqueda)
y la REA en [REA.md](../docs/REA.md). Sólo fixtures y simulación; nada se
probó contra Discord.

- [x] Dos pedidos rápidos: el toggle era el de las reacciones (la sesión
  deducía la dirección del mensaje cargado). `ToggleReaction` lleva `add`;
  gana el último pedido aunque el primero falle. Los fijados ya eran
  estado (Pin/Unpin por una cola serial): un test lo deja dicho.
- [x] `PIN_MESSAGES` (bit 51, confirmado en el cliente web): en el modelo,
  la sesión, la simulación y los editores de permisos. Como el cliente
  oficial, en un servidor fijar pide ese bit y leer el historial;
  gestionar mensajes solo ya no lo ofrece (corrección tras la revisión: el
  plan pedía aceptar cualquiera de los dos, pero la REA del cliente no lo
  hace y que Discord lo tome no está probado).
- [x] Reacciones y votos en vivo: se guardan en SQLite sobre la fila
  (`MessageMutation::Tally`, sin cambio de `SCHEMA`), invalidan una carga de
  contexto en curso (Store, Client y sesión) y el voto tiene plazo propio en
  `rune-core` (30 s y el de una verificación).
- [x] Súper reacciones: `burst`, `me_burst`, `burst_colors` (serde default)
  de `count_details` y de `MESSAGE_REACTION_ADD/REMOVE` (`type`/`burst`),
  `Event::BurstReactionChanged`; pastilla aparte con sus colores, sin
  botón; mensaje simulado en #general
  ([captura](../docs/img/super-reactions-2026-10-18.png)).
- [x] Búsqueda remota: «Más relevantes» (`sort_by=relevance`, orden del
  proveedor), `has:poll` y `link_from:`/`linkFrom:`/`sitio:`
  (`link_hostname`) en la UI, la sesión y la simulación
  ([captura](../docs/img/search-order-2026-10-18.png)).

Pendiente y por qué:

- Ver quién hizo súper reacciones (la lista pide las normales) y mandar
  súper reacciones: fuera del paquete (no se mandan por decisión).
- Reacciones con emoji propio de servidor: siguen sin modelarse.
- Orden «más antiguos» de la búsqueda: no se pidió; la mezcla con lo
  cargado necesitaría otra regla.
- Reacciones y ediciones siguen sin plazo propio en core (el HTTP de la
  sesión vence a los 15 s).
- El plazo del voto cuenta desde que core lo admite (como los envíos) y,
  con la sesión, sólo lo contesta el eco propio del Gateway: si Discord no
  lo manda, o el voto espera más de 30 s detrás de un 429, se deshace un
  voto aceptado. Arreglarlo pide que la sesión confirme el 204 con los
  votos cambiados (diferencia con los del mensaje en caché) y que ese eco
  se descarte por el registro de `Tallies`, como las reacciones propias.
  Además, cualquier `MessageUpdated` del mensaje contesta el voto (con la
  sesión, también un embed); entonces vale el plazo del HTTP.
- Una reacción o un voto en vivo del canal hace fallar la carga de contexto
  en curso («Volvé a intentarlo»). Aplicar el cambio al lote podría contarlo
  dos veces si la respuesta ya lo traía; reintentar solo una vez pide otro
  id de pedido y correlación nueva en la UI.
- Probar contra Discord (lista en DISCORD_SESSION.md).

## Envíos y subidas (2026-10-18, rama `envios-y-subidas`)

Paquete 9 de la construcción. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#adjuntos-de-salida-2026-10-17)
y [FILES.md](../docs/FILES.md#archivos-del-borrador-entre-ejecuciones).
Sólo fixtures y simulación; nada se probó contra Discord. Capturas con la
simulación (Xvfb privado):
[respuesta con archivo y «@»](../docs/img/reply-files-mention-2026-10-18.png),
[interruptor de emoticones](../docs/img/emoticons-setting-2026-10-18.png),
[progreso en una publicación de foro](../docs/img/forum-post-progress-2026-10-18.png).

Hecho:

- [x] Mencionar al autor al responder con archivos: `Command::SendFiles`
  lleva `mention_reply` desde `Intent::SendAttachments` por Core (cuerpo
  congelado: un reintento lo conserva) hasta la sesión
  (`allowed_mentions.replied_user`) y la simulación. El interruptor «@» se
  muestra también con archivos.
- [x] Hasta 10 archivos por mensaje: selector y rutas de `rune-platform`,
  `MAX_ENTRIES` de Core, la interfaz, el arrastre del host y la simulación
  (antes 8). El total de rutas admite diez rutas de 4096 bytes.
- [x] Archivos del borrador al reiniciar: Core recuerda de qué ruta leyó
  cada archivo; el host guarda ruta y tamaño (tabla `draft_files`, esquema
  6, sellada por cuenta como los borradores, `#[serde(default)]`) al ritmo
  del texto; al abrir la conversación, Core abre cada ruta otra vez con
  todas las comprobaciones y deja afuera la que falta, cambió de tamaño o
  no se puede leer, y la interfaz avisa cuáles. Una ruta repetida vuelve
  una vez; sin lugar todavía, esperan sin perderse. Con la simulación al
  arrancar y con una cuenta real al llegar la clave de su caché
  (`Restored.draft_files`; el paquete 5 viene incluido en esta rama).
- [x] Progreso de los archivos de una publicación nueva de foro:
  `Event::PostProgress` desde la sesión (el mismo cuerpo que cuenta los
  bytes de un mensaje) y la simulación; barra y «Enviando adjuntos» en el
  diálogo.
- [x] Un 429 de `POST /channels/{c}/typing` retiene sólo typing, salvo un
  límite global (`X-RateLimit-Global`, `X-RateLimit-Scope: global` o
  `"global": true`). Test con fixture.
- [x] Captura: `{open}` también en las acciones inmediatas de
  `--action` (antes sólo en `after:`), para abrir el diálogo de un foro.
- [x] Conversión de emoticones en emoji al enviar o editar (y en `/me`,
  `/spoiler`, `/shrug`…), con interruptor en Apariencia › Mensajes, apagado
  por defecto como Serein. Reimplementada: palabras enteras, sin tocar
  código, enlaces, menciones ni escapados.

Pendiente y por qué:

- Una imagen pegada (sin archivo) y una ruta que no es UTF-8 no se guardan
  con el borrador: no hay ruta que reabrir (atajo anotado en el código).
- Los archivos de una publicación de foro a medio escribir no se guardan:
  esperan un diálogo que no sigue abierto al reiniciar.
- Corrección (2026-10-18): esta rama incluye `almacenamiento-cifrado-fix`
  (paquete 5) para resolver el choque de `SCHEMA`: el paquete 5 queda en v5
  (`found` y sellado) y `draft_files` pasa a v6, sellada y en
  `seal_plain`/`unlock_new`. Al integrar, el paquete 5 va antes que este (o
  este solo, que ya lo trae).
- Restaurar un borrador cuando falta memoria a mitad de camino (una
  carrera: Core comprueba el lugar antes) muestra el error de memoria y no
  reintenta en esa ejecución; la fila queda intacta para la próxima.

Para el dueño contra Discord:

- Responder con un archivo con el «@» encendido y apagado: el autor recibe
  (o no) la mención.
- Mandar un mensaje con 10 archivos.
- Publicar en un foro con un archivo grande y ver la barra.
- Escribir rápido hasta que Discord limite typing (si pasa) y ver que los
  mensajes siguen saliendo.
- Con la conversión encendida, mandar `:) <3` y un bloque de código con `:)`.
- Archivos del borrador al reiniciar, con la simulación y con una cuenta
  real (el paquete 5 ya está integrado): adjuntar, cerrar, borrar o cambiar
  un archivo y volver a abrir.

## Contenido de mensajes (2026-10-18, rama `contenido-de-mensajes`)

Reenvíos, mensajes de sistema, ThumbHash, contenido explícito y copiar
medios. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#contenido-de-mensajes-2026-10-18-rama-contenido-de-mensajes),
la REA en
[REA.md](../docs/REA.md#cliente-web-oficial-contenido-explícito-y-registros-de-llamadas-2026-10-18)
y las capturas `docs/img/contenido-de-mensajes-2026-10-18.png` y
`docs/img/contenido-de-mensajes-llamadas-2026-10-18.png`.

Hecho (sólo fixtures y simulación; nada probado contra Discord):

- [x] Reenvíos: `Forwarded::text_references` (de `content` del snapshot) y
  las personas de `mentions` del snapshot; el texto de la copia muestra
  sus emoji de servidor (animados), menciones y canales como un mensaje
  propio, y sus emoji se piden y se animan.
- [x] Mensajes de sistema: el autor y la persona nombrada son tramos con
  acción `member:{id}` (abren el perfil); el texto de alguien no puede
  hacerse pasar por un nombre. `Message::call` (`participants`,
  `ended_timestamp`, también en `MESSAGE_UPDATE`): «Te perdiste una
  llamada de X (que duró …)» si la cuenta no participó y terminó, «X
  inició una llamada que duró …» si sí, con la duración redondeada como
  el oficial (`time::lasted`).
- [x] ThumbHash: `Attachment::placeholder` (`placeholder_version` 1, base64
  validado) como `ImageKind::ThumbHash`; `rune-core` lo decodifica al
  instante con código propio (`images/thumbhash.rs`, comparado con la
  implementación de referencia) y se dibuja en la caja reservada mientras
  la imagen carga o si falla.
- [x] Filtro de contenido explícito: bit 4 (`CONTAINS_EXPLICIT_MEDIA`) de
  `flags` de adjuntos y tarjetas; `Store::obscures_explicit` decide por
  servidor, amigo u otro (sin elegir: se ve en servidores y de amigos, se
  difumina de los demás; la cuenta propia nunca); `ImageKind::Obscured`
  difumina en CPU (24 px y desenfoque) y la imagen se ve con un clic; un
  video explícito se tapa; las tarjetas explícitas muestran su imagen
  difuminada bajo la tapa. Se quitó el aviso «RUNE todavía no difumina».
- [x] Correcciones de la revisión: el reenvío simulado conserva las
  referencias del original; las imágenes de las partes de apps se tapan
  (bit 1 de `content_scan_metadata.flags` o adjunto explícito por
  `attachment_id`); un adjunto o una tarjeta que es spoiler y explícito
  muestra la tapa de explícito antes del clic; sin niveles leídos todavía
  (caché antes de READY), todo lo explícito se tapa; un audio de más de
  50 MiB ya no ofrece «Copiar video»; los textos de error de la descarga
  no nombran Descargas al copiar; hasta ocho ThumbHash por cuadro.
- [x] Copiar imagen y Copiar video en el menú del adjunto: se baja como una
  descarga (mismos límites y cupos) a `$XDG_RUNTIME_DIR/rune-copied`
  (0700); la imagen va como píxeles (`arboard::set_image`, hasta 4096 px
  de lado) y el video como archivo (`text/uri-list`), que queda hasta la
  próxima copia; en Wayland sin XWayland, error claro.
- [x] Simulación: en #general, una foto que no llega (su ThumbHash) y otra
  marcada explícita, y un reenvío con @Ada, #anuncios y un emoji; con Ada,
  una llamada atendida de 12 minutos y una perdida de un minuto;
  `--action last:forward` para las capturas.

Pendiente y por qué:

- Probar todo contra Discord (lista en DISCORD_SESSION.md).
- Contenido violento y autolesiones (`goreContentSettings`, bits 6 y 7):
  RUNE no lee ni escribe esos ajustes todavía; sólo se aplica lo explícito.
- Escaneo pendiente: el oficial también tapa adjuntos sin
  `content_scan_version` mientras Discord los revisa; RUNE no, para no
  tapar mensajes viejos que no lo traigan (ver cómo llegan en una cuenta
  real).
- El valor «sin elegir» supone una cuenta adulta; el oficial cambia los
  valores para cuentas de menores (`nsfwAllowed`), que RUNE no lee.
- «Unirse a la llamada» en el registro de una llamada en curso (el oficial
  lo ofrece); en RUNE se entra desde la cabecera del privado.
- Copiar en Windows y macOS: `rune-platform` sólo escribe el portapapeles
  en Linux.
- Copiar baja el archivo antes de saber si hay portapapeles (Wayland sin
  XWayland lo descubre al final), y un video copiado reemplaza al anterior
  antes de escribir el portapapeles: si esa escritura falla, el anterior ya
  no está. Hace falta una consulta previa en `rune-platform`.

## Riel y notificaciones (2026-10-18, rama `riel-y-notificaciones`)

Hecho, sólo con fixtures (nada contra Discord), con Serein y una REA
estática del cliente web oficial como referencia, sin copiar código;
detalle en [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#riel-y-notificaciones-2026-10-18-rama-riel-y-notificaciones)
y [REA.md](../docs/REA.md#cliente-web-oficial-tooltip-del-servidor-y-apps-en-voz-2026-10-18).

- [x] Arrastrar servidores y carpetas en el riel, como Serein: cada marca
  es `draggable` y, mientras se arrastra, tiene tres zonas `drop_target`
  (antes, adentro, después; una carpeta sólo antes y después) y otra en
  cada hueco (antes de la marca de abajo, y después de la última encima de
  «+»). Adentro de un servidor solo hace una carpeta con los dos (abierta);
  adentro de una carpeta o de uno de sus servidores, al final de ella;
  antes o después de un servidor de una carpeta, en ella. Anillo o línea
  verde donde caería. Los menús siguen. Sin cambios en limn
  (`rune-components::rail::{RailDrag, Placement, drop_zone}`,
  `rune_ui::rail::dragged`, `account::Edit::Drop`).
- [x] Carpetas abiertas del riel guardadas por cuenta (`open_folders`,
  `ACCOUNT_SETTINGS`).
- [x] Tooltip del riel como el oficial: sin el canal AFK, sin canales
  silenciados ni bloqueados; fila de escenario (máx. 3) con «Escuchando N»;
  fila de apps en voz (máx. 6, `Event::VoiceActivity`, actividades
  embebidas de `GUILD_CREATE`, `READY_SUPPLEMENTAL` y
  `EMBEDDED_ACTIVITY_UPDATE_V2`); orden por canal y después por llegada.
  El altavoz de la marca sigue la misma regla y no se enciende en un
  servidor silenciado (salvo la llamada propia). La simulación tiene un
  escenario y una app. [Captura](../docs/img/rail-filas-2026-10-18.png).
- [x] Nivel de notificación por defecto del servidor
  (`ServerOptions::mentions_only`, ya leído de READY y `GUILD_CREATE`)
  cuando no hay nivel propio: riel, lista de canales y avisos, con o sin
  notificaciones en la cuenta.
- [x] Menciones de roles: `Message::mention_roles` (con
  `#[serde(default)]`) del `mention_roles` de Discord; cuentan como
  mención, avisan y resaltan el mensaje, salvo «Silenciar las @menciones
  de roles». Se quitó el aviso «RUNE todavía no distingue…». @everyone y
  @here igual, con `Message::mention_everyone` (Discord ya miró si el autor
  puede) y salvo «Silenciar @everyone y @here»: el texto solo no avisa. La
  simulación los calcula al enviar y tiene una mención de rol en
  #componentes.
- [x] «Silenciar conversación» / «Desactivar silencio» en el menú de un
  privado o un grupo (para siempre, como Serein); el silenciado no marca
  ni suma en el inicio ni en la bandeja del sistema, y no avisa.

Pendiente y por qué:

- El número de oyentes del escenario va en una fila aparte: la fila de
  caras de limn (`Tooltip::faces`) no lleva un texto al final. Sería un PR
  a limn (ver REA.md); no se tocó limn en este paquete.
- El hueco entre marcas y las zonas sólo existen mientras se arrastra;
  soltar fuera del riel no hace nada. No hay desplazamiento automático al
  arrastrar cerca del borde (Serein lo tiene): limn no lo ofrece en un
  `ScrollArea` común y el riel rara vez desborda.
- El orden dentro de un canal es el de llegada; el oficial además ordena
  por afinidad, que RUNE no tiene.
- Un lanzamiento de app que terminó mientras su servidor no estaba
  disponible queda hasta el próximo READY (`GUILD_CREATE` agrega, no
  reemplaza); el tooltip ya no cuenta a quien dejó el canal.
- La simulación arranca sin roles propios: la mención al rol «Moderación»
  de #componentes (Laboratorio limn) avisa recién cuando la cuenta se lo da
  en los ajustes del servidor.
- Prueba del dueño contra Discord: arrastrar y que el orden quede en el
  cliente oficial; un servidor grande con «Sólo menciones» por defecto;
  una mención a un rol propio (y con «Silenciar las @menciones de roles»);
  silenciar un privado y un grupo; el tooltip con alguien en el AFK, en un
  canal silenciado, en un escenario y en una app (Ver juntos, un juego).

## Comandos de aplicación: sólo lo que tiene Serein (2026-10-18, rama `comandos-de-apps`)

Decisión del dueño (2026-10-18): de este paquete, hacer sólo lo que
Serein tiene. Verificado en `references/serein-current` (sin copiar
código). Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones).

Hecho (fixtures en loopback y simulación; nada contra Discord):

- [x] **Subir archivos en un modal** (componente 19), como Serein
  (`valid_uploads`, `Destination::Interaction` de `upload.rs`): modelo
  `ModalInput::Files` y `FieldValue::Files` (tokens);
  `Command::Interact::files`, que Core une como los de una publicación de
  foro (`Client::interact_with_files`, `Intent::InteractWithFiles`); la
  sesión exige ADJUNTAR, prepara cada archivo por la ruta de los mensajes y
  manda `data.attachments` con el campo por índices; el `Store` da
  `InteractionAbilities::files` y espera 2 min con archivos (la REA dice
  2 min en el cliente oficial). En la interfaz, «Elegir archivos» en el
  campo, aparte del borrador del compositor, con la cruz de cada archivo;
  cerrar el modal o que la app conteste los suelta; un rechazo los deja
  para otro intento. RuneBot (simulado) tiene un campo de archivos en su
  formulario y devuelve los archivos con su respuesta.
- Capturas (Xvfb privado 1280×800, simulación):
  [campo de archivos](../docs/img/app-modal-files-2026-10-18.png),
  [respuesta con el archivo](../docs/img/app-modal-files-answer-2026-10-18.png).
  Para capturarlas, una captura responde la elección de archivos con
  `assets/audio-sample.mp3` (sin diálogo) y `--action after:reveal:{id}`
  desplaza hasta un control.

No hecho, porque Serein no lo tiene (decisión del dueño del 2026-10-18):

- Opciones de tipo adjunto (11) en comandos de barra: Serein dice
  «Attachment arguments are not supported yet» y sólo manda archivos en
  modales (`interaction()` rechaza adjuntos fuera del tipo 5).
- Índice de apps del usuario (`/users/@me/application-command-index`):
  Serein sólo pide el del servidor o el del privado.
- Reintentar el `202` del índice: Serein sólo reintenta el `202` de la
  búsqueda.
- Mensaje provisional «Enviando comando…» en la conversación: Serein
  muestra «Waiting for the application…» sobre el compositor, como RUNE.
- Comandos de menú contextual (tipos 2 y 3): el índice de Serein descarta
  todo lo que no es tipo 1.
- Motivo `ephemerality_reason` de una respuesta efímera: Serein no lo lee.

Correcciones de la revisión (2026-10-18, rama `comandos-de-apps-fix`):

- **Tipos de archivo permitidos** (`file_types`, como Serein): la sesión
  los lee (hasta 10, `.ext` o familia `image`, `video`, `audio`; uno mal
  formado descarta el modal), el campo los muestra («Tipos permitidos») y
  se comprueban por extensión al elegir (interfaz), antes de preparar nada
  (sesión) y en la simulación (`ModalInput::accepts`).
- **Canal de sólo lectura**: los archivos de un modal piden ver y adjuntar,
  no escribir, como la sesión y Serein (`InteractionAbilities::files`).
  Core prepara archivos con VIEW y ADJUNTAR y sigue exigiendo ESCRIBIR para
  mandar un mensaje o una publicación.
- **Reintento después de la espera**: si la app no contestó y la persona
  vuelve a enviar el modal, Core deja de reservar los archivos para el
  pedido viejo (antes respondía «Ocupado» para siempre); su respuesta
  tardía ya no se lleva los archivos del nuevo. Quitar un archivo lo saca
  también de lo reservado.
- La sesión ya no guarda los bytes de los archivos con lo que espera la
  respuesta: viajan con la orden y se sueltan cuando termina (o falla); el
  único reintento por hCaptcha los vuelve a preparar.
- Defensa en el borde de la sesión: como mucho los archivos y el tamaño de
  una publicación de foro (`CEILING`) y no más que el `max_values` de cada
  campo; sin ADJUNTAR dice «No tenés permiso para adjuntar archivos en el
  canal.» La simulación comprueba lo mismo (campo de archivos del
  formulario, máximo, tipos y ADJUNTAR).
- Mientras los archivos se envían, «Cancelar» no cierra el modal (ya no se
  pueden retirar). La elección de archivos de un modal ya no aparece como
  del borrador del compositor ni bloquea su envío.
- La captura responde la elección con `assets/audio-sample.mp3` resuelto
  desde el directorio de trabajo (antes, una ruta absoluta de compilación
  quedaba en el binario).
- Tests: Core (reintento tras la espera, canal de sólo lectura), sesión
  (tipos, máximo, sin ADJUNTAR sin pedidos HTTP), Store (canal sin
  ESCRIBIR), interfaz (tipo rechazado, «Cancelar» durante el envío y
  respuesta de la app que suelta los archivos), simulación y modelo.
- **2121 aprobadas, 16 ignoradas** (`cargo test --workspace
  --all-features`, 28 suites); formato y Clippy `-D warnings` correctos.
  Dos pruebas de video de `rune-media` (plazos de 5 s) fallaron una vez con
  la máquina cargada y pasaron al repetir; esta rama no toca `rune-media`.

Límites:

- Los archivos de un modal se suben sin barra de progreso (como los de una
  publicación de foro) dentro de la única operación REST de la sesión:
  mientras suben, las demás operaciones REST (historial, mensajes,
  reacciones) esperan detrás. Los mensajes con archivos no, porque usan su
  propio carril (`SendFiles`).
- La interfaz retiene como mucho 10 archivos preparados entre todos los
  borradores y el modal (8 antes de la mezcla con el límite de 10 por
  mensaje): uno con mínimo alto se bloquea si hay archivos en
  otros borradores; al pulsar «Elegir archivos» avisa «Quitá un archivo
  preparado antes de agregar otro.»
- Probar contra Discord: un modal con un campo de archivos, con uno y con
  dos archivos, y con tipos permitidos; el bot los recibe; en un canal de
  sólo lectura con permiso de adjuntar se puede; sin permiso de adjuntar,
  «Elegir archivos» no se habilita.

## Emoji y estados (2026-10-18, rama `emoji-y-estados`)

Paquete 15 del plan: que los emoji del servidor y las actividades aparezcan
donde los muestra el oficial, y completar las expresiones. Detalle en
[EXPRESSIONS.md](../docs/EXPRESSIONS.md#emoji-y-estados-2026-10-18-rama-emoji-y-estados),
rutas en [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#emoji-y-estados-2026-10-18-rama-emoji-y-estados)
y REA en [REA.md](../docs/REA.md#cliente-web-oficial-gif-emoji-grandes-y-permisos-externos-2026-10-18).
Capturas `docs/img/emoji-y-estados-*-2026-10-18.png`. Sólo fixtures y
simulación; nada se probó contra Discord.

- [x] Emoji del servidor en las líneas de estado de miembros, amigos y
  privados: imagen de 14 px y las palabras recortadas con «…». No hizo
  falta el recorte de texto formateado de limn-texto: un estado tiene a lo
  sumo un emoji y siempre adelante.
- [x] Actividad en la fila de un privado, como en la lista de miembros.
- [x] Fijados con sus emoji (también se mueven los animados). La bandeja no
  muestra extractos de mensajes.
- [ ] Imagen de los emoji en los extractos de búsqueda: **no hecho**. El
  extracto dice `:nombre:` en lugar del token, pero no dibuja la imagen.
  Bloqueado en limn (ver «Pendiente»).
- [x] Jumbo en pantallas escaladas: 96 px para un mensaje de sólo emoji.
- [x] Detector de sólo emoji con el índice de Twemoji (keycaps y las
  flechas del paquete, como ➡ o ↔, sí; `©`, `®`, `™`, dígitos, `→` y un
  emoji con VS15 no); emoji Unicode jumbo como imagen de 48 px.
- [x] Tarjeta de detalle al tocar un emoji o con clic secundario en una
  reacción, con «Copiar emoji».
- [x] `USE_EXTERNAL_STICKERS` modelado y los dos permisos externos en
  `Abilities`; el selector y el compositor dicen por qué.
- [x] «Más GIF» con el proveedor no oficial (`offset`, hasta la décima
  página; una página repetida corta).

Pendiente y por qué:

- Imagen de los emoji en los extractos de búsqueda: hacen falta dos cosas
  de limn. El recorte con «…» de un texto formateado (`Element::max_lines`,
  PR #82 de `limn-texto`, ya en la rama fijada desde la mezcla) y un subtítulo de `ListRow`
  que acepte un elemento (por ejemplo `ListRow::subtitle_element`, un PR
  nuevo). El trabajador corrector no pudo abrir un worktree de limn desde
  su worktree aislado. Al adoptarlo, `SearchHit` tiene que llevar las
  referencias de emoji del mensaje y la vista, dibujarlas con
  `text::with_pictures`.
- Jumbo Unicode en pantallas con escala mayor que 1,5: el PNG del paquete
  es de 72 px y se dibuja a 48 (96 físicos a escala 2), así que se
  agranda. Haría falta arte más grande en el paquete.

Correcciones de la revisión (2026-10-18, rama `emoji-y-estados-fix`):

- La línea de respuesta a un mensaje de sólo emoji del servidor volvió a
  dibujar la imagen (las filas también llevan la imagen de 48 px).
- La tarjeta busca el servidor de origen por id (`TextTarget::Emoji::id`)
  y toma el nombre actual del catálogo. Se cierra al cambiar de
  conversación o si el mensaje ya no está.
- El aviso de emoji que no se pueden usar ignora el código y los enlaces,
  cubre la edición y los comandos locales (/me, /shrug…), y no frena el
  envío cuando los beneficios de la cuenta son desconocidos.
- Desde la caché local no se niega un emoji o sticker externo (la caché
  puede ser anterior al bit 37).
- La simulación niega los emoji y stickers externos en #off-topic.
- Reacciones con emoji del servidor: el modelo de reacción sólo lleva
  Unicode en esta base (lo trae otro paquete); su tarjeta no las cubre.
- Que Discord respete `offset` en `/gifs/search`: el cliente oficial no
  pagina; si no lo respeta, «Más GIF» se apaga al repetirse la página.
- Probar contra Discord (lista en DISCORD_SESSION.md).

## Video y documentos (2026-10-18, rama `video-y-documentos`)

WebM/MKV, cámaras MJPEG, ajuste de video en caliente, resaltado en el PDF y
stickers Lottie. Detalle en [MEDIA.md](../docs/MEDIA.md),
[VIDEO.md](../docs/VIDEO.md), [DOCUMENTS.md](../docs/DOCUMENTS.md) y
[EXPRESSIONS.md](../docs/EXPRESSIONS.md). Todo con fixtures propios; nada
probado contra Discord ni con hardware real.

- [x] WebM y MKV en la conversación (VP8, VP9, AV1 y también H.264 en
  MKV): GStreamer lee y decodifica las imágenes (`matroskademux`, `vp8dec`,
  `vp9dec`, `av1dec` de libaom o `dav1ddec` si está, `openh264dec`) desde el
  archivo en memoria, siguiendo el reloj del sonido, que sigue con
  Symphonia; saltar busca en el pipeline. Un lector propio de la cabecera
  Matroska (`rune_media::mkv`) dice el códec y la duración. `shell.nix` suma
  `gst-plugins-good`. H.265 sigue con «Abrir en el navegador». Simulación:
  `demo.webm` junto a `demo.mp4` en #general. Fixtures de 1–2 s generados
  con FFmpeg.
- [x] La aceleración por hardware se aplica en el momento: rehace
  codificadores y decodificadores de lo que se manda y se mira desde una
  imagen clave. Sin verificar con GPU real; rehacer el codificador de lo
  que se manda no tiene test (pide una fuente de PipeWire real).
- [x] Cámaras que sólo dan MJPEG: se ofrece MJPG después del crudo y cada
  imagen se decodifica con zune-jpeg (el de `image`), con las tablas
  Huffman estándar si la cámara las omite. Probado con una imagen sintética,
  con y sin tablas.
- [x] La búsqueda del PDF resalta lo que encontró en la página
  ([captura](../docs/img/pdf-highlight-2026-10-18.png)): `pdftotext -bbox`
  en el mismo sandbox da las cajas; el núcleo pinta las marcas sobre la
  página renderizada. Test ignorado real con withDocs verificado.
- [x] Stickers Lottie con rasterlottie (Rust puro, tiny-skia): primer cuadro
  y animación en la caché de animaciones, como en Serein pero animados
  ([captura](../docs/img/lottie-stickers-2026-10-18.png)). Los que usan
  expresiones, efectos o texto siguen sin dibujo. Una precomposición en
  ciclo o muy anidada se rechaza antes del rasterizador.
- [x] Corrección tras la revisión (rama `video-y-documentos-fix`): la guarda
  de precomposiciones mira el modelo de rasterlottie y resuelve los ids como
  él (un asset sin id o repetido ya no la saltea); repetidores, estrellas y
  el total de capas y formas acotados; sólo una imagen de expresión `.json`
  se lee como Lottie. El resaltado del PDF cae en su lugar en páginas
  giradas (`/Rotate` 90/270). Un WebM o MKV de más de 3840×2160 según su
  cabecera se rechaza con su motivo antes de decodificar, y una imagen que
  no se puede mostrar o un salto que el pipeline no acepta terminan el
  video con motivo en vez de dejarlo sin imagen. Las marcas de la búsqueda
  se calculan en una pasada.

Pendiente y por qué:

- H.265 en MP4/MKV: sólo con el decodificador de la GPU (VA-API/NVDEC) se
  evitaría sumar uno con patentes; sin probar con hardware, no se habilitó.
- Un WebM sin sonido que no dice cuánto dura (grabación en vivo) no se
  reproduce: no tiene reloj.
- Sin captura del WebM: se ve igual que el MP4 de
  [media-player](../docs/img/media-player-2026-10-17.png).
- Probar contra Discord: un WebM y un MKV reales, un sticker Lottie de los
  paquetes de Discord, la búsqueda resaltada en un PDF largo, y una cámara
  USB que sólo dé MJPEG.

## Audio de mensajes: Ogg/Opus, reproductor de voz y tono mezclado (2026-10-18, rama `audio-de-mensajes`)

Sólo fixtures sintéticos y pruebas sin ventana: nadie lo escuchó y no se
probó contra el servicio real. **2300 aprobadas, 16 ignoradas** (con
`--all-features`), 28 suites; formato y Clippy `-D warnings` correctos.

- [x] Lector Ogg/Opus propio, ahora en `rune-media::clip::ogg`:
  - **Más de dos canales** (mapeo 1, hasta 7.1): el decodificador
    multistream de la misma libopus por `opusic-sys` (las bindings de
    `opus`, sin biblioteca nueva), canales promediados a mono. Los mapeos 2,
    3 y 255 se rechazan con su motivo, como antes el 1. Fixture
    `crates/rune-media/fixtures/surround.ogg` (FFmpeg/libopus, 3 canales en
    2 streams, tono sólo en el derecho).
  - **CRC de cada página** (CRC-32 de Ogg, tabla en `const`): una página que
    no coincide rechaza el archivo. Prueba con un bit cambiado en la última
    página de `assets/voice-sample.ogg` y la misma página resellada.
  - **Recorte final**: el gránulo de la última página marca el final; el
    relleno del codificador ya no suena, también después de buscar, y la
    duración del clip se conoce al leerlo (`Clip::duration`, a la muestra).
    Fixture `short.ogg` (110 ms: sin recorte sonaban 168 muestras de más).
    Corrección (rama `audio-de-mensajes-fix`): los gránulos se cuentan desde
    el inicio del stream (primera página de audio menos sus muestras); un
    final que no pasa del pre-skip se ignora y suena todo, en vez de nada;
    uno de más de 4 h rechaza el archivo. Las recetas de FFmpeg de los dos
    fixtures, que vuelven a darlos byte a byte, están en `docs/MEDIA.md`.
- [x] Reproductor de mensajes de voz: un clip que no suena muestra dónde se
  dejó (barras encendidas y tiempo que queda) con `Live::voice_left`, que
  `rune-ui` llena de `VoiceMemory`; el botón de velocidad, en filas de menos
  de 166 px, pasa debajo del reproductor en vez de desaparecer. Corrección:
  la barra y el tiempo usan la duración leída del archivo, la misma con
  que se busca al arrastrar (antes, la anunciada).
- [x] El tono de aviso ya no se pierde con un mensaje de voz sonando fuera
  de una llamada: `Playback::sound_feed` suma cada tono al `Feed` de la
  salida en el hilo de la salida (hasta cuatro, como el motor de la
  llamada), así que ni el mensaje de voz ni la prueba de escucharse se
  cortan. Ensordecido, el mensaje de voz se calla también mientras suena el
  tono de ensordecer. Quitado el atajo de `apps/desktop/src/audio.rs`.
  Corrección: ensordecido, la captura tampoco alimenta el monitor, así la
  prueba de escucharse no se oye bajo el tono. Pruebas nuevas del paso de
  `sound_feed` al hilo de la salida (el quinto suelta al más viejo) y del
  volumen del clip ensordecido.

Pendiente y por qué:

- La posición recordada de un archivo de audio o video (no de voz) sigue
  sin verse antes de pulsarlo: su duración no se sabe hasta leer el archivo.
- Sin captura: el lugar recordado sólo se ve en un clip que no es el
  reproductor actual, y llegar ahí pide reproducir, salir de la
  conversación y volver; `--capture-voice` sólo lo deja sonando, y sumar
  otro recorrido de captura a `apps/desktop/src/main.rs` no se hizo. Una
  fila de menos de 166 px no aparece con la ventana mínima de 720 px. Lo
  cubren pruebas de componentes (barras y tiempo) y de `rune-ui`
  (`Live::voice_left`).
- Probar contra Discord: un mensaje de voz real (que suene entero, sin
  cola, y que la duración coincida con la del cliente), un Ogg de más de dos
  canales si alguno llega, y un mensaje o mención que suene mientras se
  reproduce un mensaje de voz fuera de una llamada.

## Protocolo de voz (2026-10-18, rama `voz-protocolo`)

Sólo fixtures, sin probar contra el servicio real. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#protocolo-de-voz-2026-10-18-rama-voz-protocolo).

- [x] `CallQuality` en la sesión: la conexión de la llamada mide en cada ACK
  del latido de voz (op 6 con el `t` del último op 3) su RTT y la pérdida
  del audio de los demás desde el anterior, por los huecos de sus números
  de secuencia (sin RTCP RR: Serein no los lee y no se sabe si Discord los
  manda para el audio). Llega con `Event::CallPhase`, como en la
  simulación, y la interfaz ya lo mostraba.
- [x] Fase `CallPhase::Waiting` («Esperando a los demás»): conectada y sola
  en el canal; `CallPhase::is_connected` la cuenta como conectada en la
  interfaz, el audio, los sonidos y las transmisiones (una propia sin
  espectadores sigue en vivo). La simulación también la dice, según quién
  está en el canal, con textos propios que la marcan como simulada.
- [x] `GUILD_CREATE` tardíos: un `READY` sin el servidor de la llamada (o
  con `unavailable`) deja el op 4 esperando su `GUILD_CREATE`, que puede
  mostrar la voz de la cuenta en otro dispositivo; a los 15 s lo pide igual.
  Mientras espera no sale ningún op 4 (los controles van con el que la
  termina, y un reingreso pendiente se cancela), y la fila con la sesión del
  Gateway nuevo cuenta como de este dispositivo.
- [x] Hora de inicio del canal de voz: op 43 `{guild_id, fields:
  ["voice_start_time"]}` una vez por conexión y servidor, al ver el
  servidor en la columna de canales (`Command::WatchServer`, con la lista de
  miembros abierta o cerrada; como el cliente oficial, REA) y al entrar a
  su llamada (como Serein); `CHANNEL_INFO` y `VOICE_CHANNEL_START_TIME_UPDATE` llegan como
  `Event::VoiceStartTimes` y la fila de cada sala con gente dice cuánto hace
  («1 h 05 min»), también al lector de pantalla
  ([captura](../docs/img/voice-protocol-2026-10-18.png)).
- [x] Extensión RTP del sonido de la pantalla compartida: ID 9, valor 4, como
  Serein, con un codificador propio de RFC 8285 (forma de un byte). Los ID
  no están verificados (REA.md, «Sin confirmar»).

Pendiente y por qué:

- Probar contra Discord (lista en DISCORD_SESSION.md): la latencia y la
  pérdida frente al cliente oficial, `Waiting` al quedarse solo, el tiempo
  de las salas, y que el sonido de la pantalla se oiga como el de la
  transmisión (si no, el ID 9/valor 4 no es el que espera Discord).
- Un `READY` con el servidor de la llamada `unavailable` no se puede
  provocar: sólo se verá si pasa en `rune.log` («without the call's server
  yet»).
- El panel de voz sigue contando desde que la cuenta se conectó (Serein
  cuenta desde que empezó el canal): el tiempo del canal está en su fila.

## Llamadas y panel de voz (2026-10-18, rama `llamadas-y-panel-de-voz`)

Lo que quedaba de `llamadas-privadas` y `voz-estilo-serein`. Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#llamadas-privadas-2026-10-17-rama-llamadas-privadas),
[REA.md](../docs/REA.md#cliente-web-oficial-llamadas-privadas-2026-10-17) y
[MEDIA.md](../docs/MEDIA.md#implementado-panel-rápido-de-voz-y-prueba-de-sonido-como-serein-2026-10-18-rama-voz-estilo-serein).

Hecho (fixtures, simulación y un demonio de notificaciones falso; nada
contra Discord ni en un escritorio real):

- [x] `ringable` y «no son amigos», como el cliente oficial (REA del
  módulo 387755): `Command::CheckCall` (`GET /channels/{id}/call`,
  capacidad `call_check`) antes de una llamada 1:1; suena sólo con
  `ringable` verdadero; rechazada, no entra y abre «No se puede llamar
  a…» con «Enviar solicitud de amistad» o «Aceptar solicitud de amistad»
  ([captura](../docs/img/llamadas-no-amigos-2026-10-18.png)). Simulación:
  amigos suenan, con solicitud entra sin sonar, el resto se rechaza.
  Diagnóstico `voice private call: may it ring (GET /call)? …`.
- [x] Notificación de llamada entrante con «Aceptar» y «Rechazar»
  (acciones de D-Bus, `call.incoming`, crítica, sin vencimiento), que se
  retira al dejar de sonar; los botones de una llamada que ya no suena no
  atienden nada.
- [x] Alto del panel rápido de voz: el cuerpo cede alto y se desplaza en
  ventanas bajas ([captura](../docs/img/voz-panel-bajo-2026-10-18.png)).
- [x] Panel de la cámara desde la flecha de la cámara de la barra
  (`vz:camera-options`): cámara, vista previa y «Probar video» sin
  enviarla ([captura](../docs/img/llamadas-camara-2026-10-18.png)).
- [x] Vista previa en espejo: hecha el 2026-10-19 en
  `limn-teclado-y-superficies` (`Element::surface_mirrored`).
- [x] Arreglado de paso: un panel abierto desde la barra quedaba abierto
  (sin verse) al terminar la llamada, y el micrófono que medía seguía
  abierto; ahora se cierra con la llamada y apaga su prueba.
- [x] Correcciones de la revisión (`llamadas-y-panel-de-voz-fix`): el
  diálogo «no son amigos» sale sólo si Discord rechazó el GET (400, 403 o
  404, `Failure::CallCheck { refused }`); sin respuesta, un límite, un 5xx o
  la cola llena son un aviso. Con `ringable` falso «Volver a llamar» queda
  apagado mientras dure esa llamada. Sin «Enviar solicitud de amistad» si la
  persona quedó bloqueada o ya es amiga con el diálogo abierto. La
  notificación de una llamada se retira aunque el servicio esté ocupado
  (se reintenta en el cuadro siguiente), no se olvida entre las 32
  recordadas y se cierra al cerrarse RUNE. El panel de la cámara muestra la
  elegida antes de que el sistema liste las cámaras.

Pendiente y por qué:

- ~~La vista previa no está espejada.~~ Hecho el 2026-10-19
  (`limn-teclado-y-superficies`).
- El diálogo usa textos propios: el catálogo de textos del oficial
  (`My50nf`, `IdKo2z`) no está entre los chunks descargados.
- Probar contra Discord: los puntos 7 y 8 de «Qué probar contra Discord»
  en DISCORD_SESSION.md (llamar a alguien que no es amigo, `ringable`, y
  los botones de la notificación en GNOME/KDE).

## Imágenes en disco y lectura (2026-10-18, rama `imagenes-y-lectura`)

La caché de imágenes en disco y los ajustes de lectura de Serein que
faltaban, con Serein como referencia (sin copiar código). Detalle en
[SECURITY.md](../docs/SECURITY.md#datos-locales) y
[ARCHITECTURE.md](../docs/ARCHITECTURE.md#imágenes-en-disco-2026-10-18-rama-imagenes-y-lectura).
Todo con fixtures y la simulación; nada probado contra Discord.

- [x] Caché de imágenes en disco (`rune-core/src/images/disk.rs`): los bytes
  descargados, por cuenta en `$XDG_CACHE_HOME/rune/images/<digest>/`, en
  dos partes acotadas (medios y arte compartido, por `ImageKind`), nombres
  HMAC-SHA-256 de la URL con una subclave de la cuenta (`Key::file_name`),
  escritura atómica 0600, carpetas 0700, LRU por `mtime` (poda hasta el
  90 %) y retención. Cada archivo sellado con la clave de la cuenta de la
  caché cifrada (`Key::seal_file`): sin llavero no se escribe nada; una
  clave nueva vacía la carpeta; olvidar la cuenta o cerrar sesión con el
  proveedor no oficial la borra (cerrando antes los discos que una
  descarga todavía tenga); «Borrar datos locales» vacía las dos partes,
  también si el disco todavía se está abriendo. El disco sigue a
  `storage_active`: el Ready de otra cuenta o el corte por `TooLarge` no
  escriben ahí. Una descarga lee el disco antes que la red y escribe sólo
  lo que se decodificó; mientras el disco abre, no sale ninguna. El disco
  espera los límites del host para no podar con otros (el host los da en
  cuanto llega la clave, y los que cambian mientras abre le llegan al
  terminar). Corregido el 2026-10-18 (rama `imagenes-y-lectura-fix`): la
  lectura, la escritura y el vaciado se reescribieron sin la forma del
  código de Serein, del que habían quedado demasiado cerca. Tests: sellado,
  nombres con clave y permisos; presupuestos y retención; clave nueva y
  olvido con descargas en curso; borrar y cambiar límites mientras abre y
  una apertura que falla; una descarga en loopback que la ejecución
  siguiente muestra sin red aunque el primer cuadro pida antes de que abra;
  y en `encrypted_storage`, con un llavero falso, la espera de la clave y
  los límites, una imagen que la sesión de la cuenta baja y guarda, otra
  que tras el Ready de otra cuenta no se guarda, y el olvido.
- [x] Página «Caché de imágenes» en Datos y privacidad
  ([captura](../docs/img/cache-imagenes-2026-10-18.png)): uso de cada parte
  sobre su presupuesto, «Borrar» por parte (`Intent::ClearImageCache`), los
  presupuestos (256 MB–4 GB y 64–512 MB) y «Borrar las imágenes sin usar
  después de» 7–365 días, como Serein. Sin disco abierto lo dice y no
  ofrece borrar.
- [x] «Animar GIF» en Apariencia › Lectura
  ([captura](../docs/img/lectura-ajustes-2026-10-18.png)): apagado, el host
  sólo anima lo que se ve bajo el puntero (`animations::now_playing` y
  `under`, que no cuentan lo recortado ni lo tapado por algo opaco pintado
  después); lo demás queda en su primer cuadro. Vale para todo lo animado
  (GIF, stickers, emoji, decoraciones), como en Serein. El movimiento
  reducido sigue frenando todo.
- [x] «Ocultar los enlaces de imagen y GIF»: el ocultamiento de 14.2 es
  ahora `MessageStyle::hide_media_links`, encendido por defecto.
- [x] «Velocidad de desplazamiento» (25–300 %, de a 5, «Restablecer»): el
  host escala en `handle_ui_event` los deltas de `Scroll` (rueda),
  `ScrollDirect` (trackpad) y `Scroll2D` antes del árbol (probado por
  `handle_ui_event`). limn ya separa la rueda del trackpad, así que no hizo
  falta tocarlo.
- [x] «Mantener la lista de miembros en ventanas angostas»: la conversación
  baja hasta 280 px en lugar de esconder la lista; la búsqueda del
  encabezado sigue el ancho de siempre. Es una extensión de RUNE: el ajuste
  de Serein («Keep the server member list open when the window is wide
  enough») ya estaba como `members_open`, que se recuerda.

Pendiente y por qué:

- Las URL firmadas de adjuntos (`ex`/`is`/`hm`) cambian al renovarse: esa
  imagen se vuelve a bajar y la vieja se va por retención o presupuesto.
  Una clave sin la firma necesita que el proveedor diga qué parte de la URL
  es estable (el núcleo no conoce el servicio); no se hizo.
- Un origen en espera por un 429 tampoco sirve lo que está en disco hasta
  que termina la espera (la lectura del disco va en la misma tarea que la
  descarga).
- Sólo la caché de `images.rs`: las imágenes de perfiles, expresiones e
  íconos de grupo que se eligen para subir no pasan por ella (no se
  descargan).
- De la lectura de Serein quedan la reacción con doble clic y la lista de
  miembros en privados y grupos (RUNE no muestra lista ahí).
- Probar contra Discord: arrancar dos veces y ver que avatares, emoji y
  vistas previas aparecen sin bajarse (el uso de la página crece la primera
  vez y no la segunda); «Borrar» de cada parte; cambiar el presupuesto a
  uno menor; cerrar sesión y ver que `~/.cache/rune/images` queda sin la
  carpeta de la cuenta; «Animar GIF» apagado con un GIF, un sticker y un
  emoji animado; un mensaje que es sólo un enlace de Tenor/Giphy con el
  interruptor apagado; la velocidad con rueda y trackpad reales; la lista
  de miembros en una ventana de 960 px.
## Perfil: popout como el oficial, cosméticos, portadas y caché que vence (2026-10-18, rama `perfil-tarjeta-y-cosmeticos`)

Decisiones del dueño del 2026-10-18: popout de 300 px y portadas de
Spotify, Twitch y YouTube desde sus hosts (nunca tokens). Sólo fixtures y
simulación; **sin probar contra el servicio real**. Detalle en
[PROFILES.md](../docs/PROFILES.md#tarjeta-como-la-oficial-y-cosméticos-2026-10-18-rama-perfil-tarjeta-y-cosmeticos),
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#portadas-nameplates-por-sku-y-cosméticos-2026-10-18-rama-perfil-tarjeta-y-cosmeticos),
[REA.md](../docs/REA.md#cliente-web-oficial-portadas-nameplates-y-decoraciones-2026-10-18)
y [SECURITY.md](../docs/SECURITY.md#portadas-de-spotify-twitch-y-youtube-2026-10-18).

- [x] Popout de 300 px, a 16 px de la fila en la lista; cuerpo desplazable
  entre cabecera y botones (nunca más alto que la ventana); nota como
  ícono que abre el perfil completo; el efecto baja al 35 % bajo el puntero;
  la tarjeta de un miembro o un autor se cierra cuando la persona desplaza
  la lista o la conversación, no cuando la lista se corrige sola.
- [x] Nameplates por sku (`media/v1/collectibles-shop/{sku}/animated`):
  la animada sólo bajo el puntero o con la tarjeta abierta, sin moverse con
  movimiento reducido.
- [x] Decoraciones animadas al pasar el puntero en miembros, mensajes (cara,
  nombre o fila) y mosaicos de voz, que ahora la dibujan.
- [x] Ajustes › Apariencia › «Cosméticos de los perfiles»
  (`settings/pages/cosmetics.rs`): decoraciones, nameplates y efectos; lo
  oculto no se pide al CDN; se guarda entre ejecuciones.
- [x] Perfiles que vencen al minuto, como el oficial: lo guardado se muestra
  y se vuelve a pedir.
- [x] Banner del perfil completo cuando la identidad no lo trae: el popout
  propio ahora pide su perfil completo (las tarjetas ya lo tomaban).
- [x] Capturas: [tarjeta](../docs/img/perfil-tarjeta-2026-10-18.png) y [cosméticos](../docs/img/perfil-cosmeticos-2026-10-18.png) (Xvfb privado, simulación).
- [x] Portadas de `spotify:`, `twitch:` y `youtube:` desde sus hosts
  exactos, por HTTPS y con los límites de un banner (1 MiB, 2048 px).
- [x] Corrección tras la revisión (rama `perfil-tarjeta-y-cosmeticos-fix`):
  en limn (rama `rune/hover-dim`, commit `637990d`; PR #83 a
  `Niiihuel/limn` desde `feat/hover-dim`)
  `Element::dim_on_hover` deja el efecto al 35 % bajo el puntero, y
  `Element::hover_region_id` con `UiTree::hovered_region` dice qué fila de
  mensaje tiene el puntero (se borró la deducción por anclas y la posición
  guardada del puntero; una fila de sistema ya no anima a su autor y un
  modal encima tapa la fila). Una mención anima al autor del mensaje, no a
  quien nombra. El cuerpo de cada tarjeta tiene su propio ID
  (`pp:card:body:{handle}`): la de otra persona ya no abre desplazada. Con
  los efectos ocultos no se pide el producto del efecto
  (`LoadProfile { effect }`), como Serein, ni al releer el perfil propio
  tras editarlo. Las portadas se decodifican con
  los límites de un banner, no con los de un adjunto. Un reloj que vuelve
  atrás deja el perfil vencido. La memoria de la caché cuenta la nameplate
  animada. Tests nuevos: dos tarjetas seguidas, efecto al 35 %, mención,
  16 px entre fila y tarjeta, mosaico de voz con decoración y bajo el
  puntero, efectos ocultos de punta a punta.

Pendiente y por qué:

- Marcos de perfil: sin interruptor, porque RUNE todavía no los dibuja.
- Filas de voz de la columna: sin decoración (el bundle oficial no la
  muestra ahí, que se haya visto); sí los mosaicos.
- Un perfil leído con los efectos ocultos no trae su efecto al volver a
  mostrarlos hasta que se vuelve a pedir (al minuto, al reabrir la tarjeta).
- Probar contra Discord: ancho y lugar de la tarjeta, cuerpo desplazable en
  una ventana baja, cerrar al desplazar la lista grande, nameplate animada
  de una cuenta real (formato de `animated`), decoraciones bajo el puntero,
  ocultar cosméticos, reabrir una tarjeta pasado un minuto, banner en el
  popout propio y portadas de Spotify, Twitch y YouTube.

## Reglas del servidor y onboarding (2026-10-18, rama `reglas-y-onboarding`)

Aceptar las reglas y responder el onboarding al entrar a un servidor, como
Serein, y editar el onboarding desde la administración. Detalle en
[INVITATIONS.md](../docs/INVITATIONS.md#reglas-y-onboarding-al-entrar-2026-10-18),
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#reglas-del-servidor-y-onboarding-2026-10-18-rama-reglas-y-onboarding)
y [REA.md](../docs/REA.md#cliente-web-oficial-reglas-del-servidor-y-onboarding-2026-10-18).
Todo con la simulación y fixtures en loopback; **nada probado contra
Discord**.

- [x] Modelo propio (`rune_model::onboarding`: `ServerRules`, `RulesField`,
  `RulesAnswer`, `JoinOutcome`, `Onboarding`, `Prompt`, `PromptChoice`,
  límites del cliente oficial y sus validaciones), `Command`/`Event`/
  `Failure` (`LoadServerRules`, `AnswerServerRules`, `LoadOnboarding`,
  `AnswerOnboarding`, `SaveOnboarding`) y `Capabilities` (`server_rules`,
  `onboarding`, `manage_onboarding`). `ServerOptions` suma
  `rules_screening` y `onboarding`; `MemberRestrictions`, `onboarded`
  (todos con `#[serde(default)]`).
- [x] Aceptar las reglas al entrar: `Store::join_steps` dice qué debe la
  cuenta; mientras las reglas esperan, el compositor está cerrado y dice
  «Aceptá las reglas de {servidor} para participar» (antes «Completá la
  verificación…»); en revisión o rechazada, lo dice la moderación
  (`Store::rules_outcome`). Diálogo «Antes de participar en {servidor}»
  con las reglas numeradas, «Leí y acepto las reglas» y los campos de
  texto u opción múltiple; un tipo desconocido se muestra sin completarlo.
- [x] Responder el onboarding al entrar: el mismo diálogo, antes de las
  reglas, con cada pregunta (una o varias opciones, obligatoria o no) y
  cada opción con su emoji y descripción. Se abre solo tras aceptar una
  invitación a un servidor que pide algo, y desde la barra sobre el
  compositor si se dejó para más tarde. Las respuestas van primero y las
  reglas cuando el proveedor las confirma; los roles que dan llegan como
  la actualización del miembro. Simulación: el club de la invitación
  `club` (dos preguntas, roles «Fotografía» y «Música», el canal oculto
  `#fotos` que abre la primera, y tres reglas); el cuarto oscuro de
  `revelado` (reglas con texto corto, opción múltiple y párrafo, que la
  moderación revisa: «Enviado…» y el compositor lo dice); la radio de
  `radio`, con onboarding que a un invitado no se le pide.
  Capturas: [diálogo](../docs/img/onboarding-join-2026-10-18.png) y
  [barra](../docs/img/onboarding-bar-2026-10-18.png).
- [x] Editar el onboarding como administración: Ajustes del servidor ›
  Incorporación (sólo en una comunidad; `ServerAbilities::manage_onboarding`
  pide `MANAGE_GUILD` y `MANAGE_ROLES`): activarla, qué cuenta para los
  requisitos (modo), canales por defecto (dice cuáles permiten escribir a
  todos), preguntas (título, una sola respuesta, obligatoria, al entrar,
  lista desplegable) y opciones (título, descripción, emoji, roles que da
  y canales que abre). La barra de cambios sin guardar la guarda; una
  pregunta u opción incompleta o un onboarding activo sin un canal donde
  todos escriban no se manda y el aviso dice por qué. Simulación:
  «Laboratorio limn» es una comunidad con onboarding
  ([captura](../docs/img/onboarding-settings-2026-10-18.png)).
- [x] Sesión no oficial: rutas del cliente web oficial (REA) y de Serein,
  bandera `COMPLETED_ONBOARDING` y features `GUILD_ONBOARDING` y
  `MEMBER_VERIFICATION_GATE_ENABLED`, `IS_GUEST`; hCaptcha común en las
  tres escrituras (un test con fixtures por cada una). `--capture-onboarding`
  en el escritorio une el club simulado y abre el diálogo para la captura.
- [x] Correcciones tras la revisión (2026-10-18, rama
  `reglas-y-onboarding-fix`): la capa de cables de la sesión y los tests
  del modelo se reescribieron con estructura y fixtures propios (estaban
  demasiado cerca de Serein); las reglas se devuelven tal como vinieron
  (`RulesField::echo`); el diálogo sólo usa lo elegido en preguntas que se
  hacen al entrar, no abre ni pide el onboarding a un invitado, vuelve a
  pedir la parte que no llegó antes de mandar nada, no deja editar lo que
  está en camino, deja Escape a la verificación y se cierra si ya no se
  debe nada; la página de edición espera mientras guarda y exige un solo
  emoji por opción; el resultado de las reglas sobrevive a una reconexión.

Pendiente y por qué:

- Cambiar las respuestas del onboarding después de entrar («Canales y
  roles» del oficial, `PUT …/onboarding-responses`): no estaba en el
  paquete y Serein no lo tiene.
- Reordenar preguntas y opciones en la página de edición, y elegir un
  emoji del servidor (sólo Unicode por texto; uno del servidor que ya
  estaba se conserva mientras no se cambie su nombre).
- Discord documenta al menos 7 canales por defecto con 5 donde todos
  escriban para activar el onboarding: RUNE pide sólo uno y deja el resto a
  Discord (su rechazo llega con su motivo).
- El resultado de unas reglas en revisión vive en memoria: sobrevive a una
  reconexión de la misma cuenta, pero al cerrar RUNE se pierde y el
  diálogo vuelve a ofrecer las reglas (otro `PUT …/requests/@me`). Falta
  confirmar por REA cómo lee el oficial el estado de una solicitud
  (`GET …/requests/@me`?) para preguntarlo al abrir el diálogo.
- Los límites (15 preguntas al guardar, 50 opciones, largos de textos)
  salen de Serein y del oficial sin confirmación de la REA; se leen hasta
  50 preguntas para no dejar ilegible un servidor con más.
- Probar contra Discord: aceptar una invitación a una comunidad con reglas
  y onboarding (diálogo, roles y canales que da, compositor que se abre al
  aceptarlas), una con reglas revisadas por moderación (`SUBMITTED`), el
  hCaptcha si aparece en alguna de las tres escrituras, y editar el
  onboarding de un servidor propio que sea comunidad (incluido lo que
  Discord rechaza por requisitos). Mirar también que la barra no aparezca
  en servidores donde ya se participa (miembros anteriores al onboarding
  sin `COMPLETED_ONBOARDING`: Serein los trata igual).

## Escritorio en Linux (2026-10-18, rama `escritorio-linux`)

Servicios de escritorio de Linux y dos ajustes de ventana de Serein.
Windows y macOS quedan diferidos. Todo con fixtures, buses D-Bus privados y
la simulación; **nada contra un portal real**. Detalle en
[FILES.md](../docs/FILES.md#reemplazar-en-guardar-como-083-2026-10-18) y
[PLATFORM.md](../docs/PLATFORM.md#ventana-y-gráficos-2026-10-18).

- [x] **«Guardar como»: reemplazar con confirmación (08.3).** Un archivo
  común con el nombre elegido se pregunta antes de descargar
  (`SaveUpdate::Exists`); «Reemplazar» descarga al parcial privado y lo
  publica con `rename(2)` (de una vez); una carpeta o un enlace se siguen
  rechazando, y justo antes de publicar se exige el mismo archivo
  (dispositivo e inodo) que se preguntó
  ([captura](../docs/img/escritorio-linux-reemplazar-2026-10-18.png)).
- [x] **Diálogos con parent.** X11: el id de la ventana. Wayland: la
  superficie exportada con xdg-foreign v2 sobre la conexión de winit
  (`rune_platform::wayland_parent`), una vez, al preparar la ventana; llega a
  adjuntar, «Guardar como», la carpeta de sonidos, las imágenes de perfil,
  grupo y expresiones, y la pantalla compartida. Verificado sólo que un
  sway 1.12 headless privado da el handle; ningún portal real lo importó.
- [x] **Carpeta de sonidos con el diálogo del sistema.** «Elegir…» en la
  página de sonidos: `FolderRequest` y `FileChooser::choose_folder`
  (`OpenFile` con `directory`) en `rune-platform`, el servicio
  `Client::choose_folder` en `rune-core` sobre el mismo lugar de diálogo
  que adjuntar y «Guardar como»
  ([captura](../docs/img/escritorio-linux-sonidos-2026-10-18.png)).
- [x] **Portal de apariencia que se reinicia.** `NameOwnerChanged` del
  portal: con un dueño nuevo se lee y se sigue otra vez; sin portal al
  arrancar, se sigue cuando aparece.
- [x] **Ocultar las decoraciones** (Ajustes › Sistema › Ventana), al abrir y
  en vivo (`ViewportCommand::Decorations`), en `~/.config/rune/decorations`.
- [x] **Elegir la GPU** (Ajustes › Sistema › Gráficos, «Dibujar con»):
  Automático o cada fabricante detectado, «Se aplica la próxima vez» y la
  GPU que dibuja ahora; `~/.config/rune/gpu`, leído por `gpu.rs` antes de
  la ventana. Las variables del entorno siguen mandando y la página lo dice;
  si la elegida no abre la ventana, RUNE arranca con cualquiera y lo dice
  ([captura](../docs/img/escritorio-linux-ventana-2026-10-18.png)).

Pendiente y por qué:

- Dos GPU del mismo fabricante no se pueden elegir por separado: el filtro
  del cargador de Vulkan (`VK_LOADER_DRIVERS_SELECT`) elige drivers, no
  placas. Haría falta elegir el adaptador en wgpu, que igual abriría los
  dos drivers (lo que `gpu.rs` evita).
- AMDVLK (`amd_icd64.json`) no entra en `*radeon*`: con ese driver,
  elegir AMD termina en el arranque de respaldo con cualquiera.
- Las imágenes de perfil, grupo y expresiones siguen abriendo cada una su
  propio selector (no el lugar compartido del actor de adjuntos); sólo
  ganaron el parent.
- El diálogo de `BindShortcuts` (atajos globales del portal) sigue sin
  parent (`""`): `GlobalShortcuts::start` arranca sin la ventana, y llevarle
  el `ParentWindow` pide cambiar su API y la de `ptt.rs`.
- Probar en un escritorio real (ver
  [PLATFORM.md](../docs/PLATFORM.md#qué-falta-probar-en-un-escritorio-real)):
  diálogos sobre la ventana en Hyprland, GNOME y KDE; reemplazar un archivo
  con «Guardar como» (el diálogo GTK pregunta también); «Elegir…» de la
  carpeta de sonidos; reiniciar el portal y cambiar el tema; ocultar las
  decoraciones; «Dibujar con» NVIDIA y Automático.

## Tipografía y opciones de limn (2026-10-18, rama `limn-tipografia-y-opciones`)

Paquete 10 del plan. Todo en limn, en la rama
`rune/limn-tipografia-y-opciones` (`3284386`) y la
[PR #84](https://github.com/Niiihuel/limn/pull/84) desde `main`; detalle en
[LIMN_INTEGRATION.md](../docs/LIMN_INTEGRATION.md#tipografía-y-opciones-seminegrita-lenguajes-scaledown-pista-por-opción-y-avatarcolor-pr-84-2026-10-18).

Hecho:

- [x] Seminegrita: `FontFamily::SansSemibold` con Ubuntu Sans SemiBold
  (Ubuntu no tiene ese peso; misma licencia, avisos en el `NOTICE.md` de
  limn-egui) y `Tooltip::title_family`. Sólo donde la REA la pide: título
  de los tooltips del carril (servidores, carpetas, «Añadir un servidor»)
  y nombre de la tarjeta de actividad. La fuente elegida en Apariencia
  también aporta su peso 180 (`UiFaces::semibold`; sin él, su negrita).
- [x] 16 lenguajes más en el coloreado por línea (Go, Java, Kotlin, Swift,
  C, C++, C#, PHP, Ruby, Lua, Shell, YAML, SQL, Diff, Dart y Zig) con sus
  alias de cerca: la lista de Serein, consultada sin copiar código.
- [x] `SurfaceFit::ScaleDown`: el visor de adjuntos ya no agranda un GIF
  chico más allá de su cuadro quieto.
- [x] `DropdownOption::tooltip`: cada dispositivo recortado del panel
  rápido de voz dice su nombre entero en su pista.
- [x] `Avatar::color` en `avatar_with_image`.

Pendiente y por qué:

- ~~Tamaño de 16 px y apertura sin demora del título del tooltip del
  carril~~: llegaron con el paquete 8 al mezclar (2026-10-19).
- La PR desde `main` no lleva `UiFaces::semibold` ni `ScaleDown`: `main`
  todavía no tiene `UiFaces` (PR #76) ni las superficies encajadas
  (PR #66); siguen en la rama de integración hasta que se fusionen.
- Sin captura nueva: el cambio visible (título del carril en seminegrita)
  es de unos píxeles; lo cubre el test `the_hint_names_the_server_in_semibold_as_the_official_client`.
- Probar en la ventana real: el título de un servidor en el carril, la
  tarjeta de actividad, un GIF chico en el visor y la pista de un
  micrófono de nombre largo. Nada de esto toca el proveedor ni la red.

## limn: avisos, insignias, íconos chicos, nivel del slider y compositor (2026-10-18, rama `limn-avisos-y-compositor`)

Paquete 8 del plan. limn: rama `rune/limn-avisos-y-compositor` en el clon
local de lienzo (`3cbe6ed`, sobre `rune/hover-dim` `637990d`; el
integrador la sube a GitHub y cambia `Cargo.toml`) y
[PR #85](https://github.com/Niiihuel/limn/pull/85) desde
`feat/limn-avisos-y-compositor` (`952daf5`, sobre `main`). Detalle en
[LIMN_INTEGRATION.md](../docs/LIMN_INTEGRATION.md#avisos-insignias-íconos-chicos-nivel-del-slider-y-compositor-pr-85-2026-10-18).
Simulación y tests sin ventana; nada contra Discord.

- [x] Tooltip con demora, tamaño y cara del título, flecha y fondo propios
  (por defecto como antes); el carril abre al instante, título de 16 px
  medium sobre `#0a0a0c` (`Look::hint`, menús en el aspecto claro).
  **El dueño tiene que confirmar las opciones nuevas.**
- [x] `Badge` en cualquier esquina y de cualquier alto, con anillo, tinta y
  cara propios: las menciones del carril.
- [x] Íconos de 8 a 16 px (`Element::small_icon`): altavoz del carril
  (12 px) y candado chico sobre el `#` de un canal privado. Íconos de
  categorías de emoji (árbol, huevo, mando, auto, lamparita, bandera) en el
  selector; el mando reemplaza `assets/gamepad.svg`.
- [x] `Slider::level`: el umbral de voz a mano dibuja el nivel del
  micrófono sobre su pista.
- [x] `Composer` de limn en el compositor (controles junto a la primera
  línea mientras el editor crece, la misma geometría) y aviso largo que
  pasa a la línea siguiente; sugerencias de miembros con avatar, color del
  rol y usuario a la derecha, roles con su color.
- [x] Lo del paquete 2: interruptor de 50×26; `Select` de campo con valor
  a 16 px, cinco filas a la vista, chevrón hacia arriba y borde de acento
  al abrirse; barra del chat que aparece bajo el puntero; menú de un
  mensaje («Más» o clic derecho) que se cierra cuando su fila sale de la
  lista.
- [x] Capturas: [carril](../docs/img/limn-avisos-riel-2026-10-18.png),
  [sugerencias](../docs/img/limn-avisos-sugerencias-2026-10-18.png) y
  [emoji](../docs/img/limn-avisos-emoji-2026-10-18.png) (Xvfb privado,
  simulación).

Pendiente y por qué:

- Seminegrita del título del tooltip: paquete 10 (`limn-tipografia-y-opciones`).
- El candado chico no se ve en la simulación: no hay un canal privado que
  la cuenta vea sin ocultarlo (`#moderación` va como oculto, con su fila
  propia); está cubierto por un test de la barra.
- La pastilla de «99+» del carril es 2 px más angosta que antes (el número
  queda a 3 px del anillo dentro de limn, antes a 4 del relleno).
- Los `Select` de los ajustes de la cuenta (`settings/pages.rs`) siguen a
  13 px y sin tope de filas: el pedido era el del modo lento.
- Probar contra Discord: tooltip del carril con un servidor real (demora,
  título y fondo frente al oficial), menciones del carril con más de 99,
  candado sobre el `#` con permisos reales, sugerencias con avatares y
  colores de roles reales, aviso de aislamiento largo en una ventana
  angosta, barra del chat bajo el puntero, umbral del micrófono con el
  nivel en la pista mientras se prueba, menú de un mensaje que se cierra al
  desplazar.

Mezcla con el paquete 10 (2026-10-19): limn en `rune/integration-2026-10-19`
(`a7464c5`, sólo en el clon local); el título del tooltip del carril queda a
16 px, al instante y en seminegrita, y `Tooltip::title_family` (paquete 10)
se unificó en `Tooltip::title_font_family`
([detalle](../docs/LIMN_INTEGRATION.md#rama-de-integración-del-2026-10-19)).

## limn: teclado completo, modificadores, superficie en espejo y botón animado (2026-10-19, rama `limn-teclado-y-superficies`)

Paquete 6 del plan. limn en `rune/integration-2026-10-19` (`ed11ee8`), que
mezcla `rune/limn-teclado-y-superficies` (`8128560`)
([detalle](../docs/LIMN_INTEGRATION.md#teclado-completo-modificadores-superficie-en-espejo-y-botón-animado-pr-86-2026-10-19)).

- [x] `limn::Key` completo (dígitos, puntuación, F13–F35), liberación y
  teclas sostenidas: el grabador de teclas y Pulsar para hablar en la
  ventana (y silenciar, ensordecer y pulsar para silenciar) leen sólo de
  limn; se fueron `NativeHostEvent::Key` y `host_key`. Los atajos guardados
  se siguen leyendo (mismos nombres que egui); un toggle actúa una vez por
  pulsación, no por la repetición; perder el foco suelta todo y cancela el
  grabador.
- [x] Cambio de modificadores: con Shift sostenido la barra de un mensaje
  cambia las reacciones rápidas por fijar/desfijar, no leído, copiar
  enlace, texto e ID y eliminar (sin preguntar), sólo lo que la cuenta
  puede, en el orden de Serein
  ([captura](../docs/img/limn-teclado-shift-2026-10-19.png); en capturas,
  `--action shift`).
- [x] Vista previa de la propia cámara en espejo (panel rápido, escenario
  y Ajustes › Voz y video); lo enviado no cambia.
- [x] Emoji animados en botones y opciones de menús de aplicaciones
  (`Button::surface`, `DropdownOption::image`/`surface`): se mueven salvo
  con movimiento reducido o «Animar GIF» apagado; las opciones abiertas
  muestran la imagen en lugar de `:nombre:`. El proveedor no oficial pide
  la imagen animada (`animated=true`) cuando el emoji lo es (sólo
  fixtures, sin probar contra el servicio real).

Pendiente y por qué:

- Probar contra Discord: un botón y un menú de un bot con un emoji animado
  de servidor, y Pulsar para hablar con una tecla de dígito o puntuación.

## Atajos editables y navegación atrás/adelante (2026-10-19, rama `atajos-y-navegacion`)

Paquete 29 del plan. Sin cambios en limn: usa el `limn::Key` completo del
paquete 6 (`ed11ee8`) y `UiTree::anchor_offset`. Serein (`keybinds.rs`)
consultado sin copiar código. Detalle en
[ARCHITECTURE.md](../docs/ARCHITECTURE.md#atajos-editables). Simulación y
tests sin ventana; nada toca el proveedor ni la red.

- [x] Atajos internos editables: `rune_ui::keybinds`, una tabla de 24
  acciones en cuatro partes (navegación, mensajes, formato y voz), cada una
  con su atajo por defecto. Ajustes › Sistema › Atajos de teclado: clic en
  la caja de teclas, pulsar la combinación (Esc cancela) y «Restablecer»
  («Quitar» en los de voz, que no tienen atajo por defecto). Se rechaza con
  el motivo en la fila una combinación que ya usa otra acción («Ya lo usa
  «Selector rápido de conversaciones».»), Tab, Shift+F10, Ctrl+A/C/V/X/Z/Y
  o una tecla que sola escribe o mueve el cursor (salvo Enter y flechas
  para los de mensajes, y cualquiera para los de voz, que se sostienen);
  «Restablecer» espera si otra acción tomó ese atajo. Se guarda sólo lo
  cambiado, en el ajuste `shortcuts`; los de voz siguen en `mute_key` y
  compañía. La lista de Ctrl+/ (ahora por partes, con desplazamiento) y las
  pistas de la cabecera leen la tabla; el host despacha por ella
  (`apps/desktop/src/shortcuts.rs`) y `intercept` lo del compositor:
  formato, editar el último, enviar y línea nueva. Si Enviar deja de ser
  Enter, Enter escribe una línea nueva (la inserta RUNE antes que el
  compositor de limn, que sigue enviando con Enter cuando es el atajo). La
  sección «Atajos del micrófono y el sonido» pasó a ser la parte de voz de
  la tabla.
- [x] Ctrl+/ donde «/» exige Shift: una tecla de puntuación o un dígito
  coincide con o sin Shift (la tecla traducida de limn: Shift+7 en una
  distribución española llega como `Key::Slash` con Shift), y se graba sin
  él. Probado con el evento nativo de egui en el host.
- [x] Flechas atrás/adelante en la cabecera de la conversación, con su pista
  y su atajo; sin destino quedan deshabilitadas y más tenues (el `muted`
  de su paleta, que es lo que pinta limn). En ventanas de menos de 800 px
  son de 24 px y ocupan el lugar del glifo del canal. Sirven también con las
  decoraciones ocultas. [Captura](../docs/img/atajos-y-navegacion-cabecera-2026-10-19.png).
- [x] Desplazamiento al píxel: cada lugar guarda, además del primer mensaje
  a la vista, cuánto de él estaba fuera (`anchor_offset`); al volver, tras
  el salto a la fila se corrige con `Intent::SetScrollOffset` hasta quedar a
  menos de medio píxel (hasta 8 layouts). Mientras se corrige, salir otra
  vez guarda el lugar original.
- [x] Capturas (Xvfb privado, simulación, `--language es`):
  [ajustes](../docs/img/atajos-y-navegacion-2026-10-19.png),
  [lista de Ctrl+/](../docs/img/atajos-y-navegacion-lista-2026-10-19.png) y
  [cabecera](../docs/img/atajos-y-navegacion-cabecera-2026-10-19.png).

Pendiente y por qué:

- La edición de un mensaje (no el compositor) sigue guardando con Enter y
  Ctrl+Enter, como limn: Enviar y línea nueva reasignados sólo valen en el
  compositor, que es el que tiene selección propia en RUNE.
- Con Enviar en una combinación que no es Enter ni Ctrl+Enter, Ctrl+Enter
  sigue enviando: limn lo hace en todo compositor.
- La búsqueda dentro de un documento abierto sigue en Ctrl+F (es del
  visor, no de la tabla).
- Los conflictos se miran entre todas las acciones, como Serein; no hay
  ámbitos (una combinación sólo del compositor no puede repetirse en otra
  acción global aunque no chocaran).
- La nota de voz «También fuera de RUNE» no distingue una tecla sola (que el
  portal no recibe) de una combinación.
- Probar en una ventana real: Ctrl+/ con una distribución española o
  alemana, una reasignación con teclas F13+ o puntuación, las flechas de la
  cabecera con el mouse y la vuelta al píxel en un canal con imágenes que
  cambian de alto. Nada de esto toca el proveedor ni la red.

## Micrófonos sin las salidas (2026-10-19, rama `microfonos-sin-salidas`)

- [x] Con PipeWire, la lista de entradas ya no trae los monitores de las
  salidas (`default_sink`, `alsa_output.*` y cualquier sumidero, que cpal
  lista como `Duplex`): elegir uno mandaba lo que suena en el equipo. La
  regla (`rune_media::records_output`) tiene su test con datos fijos; ALSA,
  WASAPI y CoreAudio no se filtran. Verificado en esta máquina con
  `cargo run -p rune-media --example devices`: quedan `default_input` y
  `alsa_input.*`.
- [x] Un monitor ya guardado como micrófono se ve como desconectado y la
  captura abre el micrófono predeterminado
  ([detalle](../docs/MEDIA.md#corregido-los-micrófonos-ya-no-incluyen-las-salidas-2026-10-19-rama-microfonos-sin-salidas)).

Pendiente y por qué:

- Un nodo `Audio/Duplex` genuino de PipeWire (raro) también se oculta: cpal
  no expone la clase del nodo.
- La salida sigue listando `default_sink` junto a `default_output` (los dos
  siguen al predeterminado): no se pidió.
- Probar a mano: Ajustes → Voz con un micrófono guardado que era el
  monitor (debe decir «Dispositivo no disponible» y medir el micrófono
  predeterminado).
## Temas propios, color principal y transparencia (2026-10-19, rama `temas-propios`)

Paquete 31, paridad con los temas de Serein (`docs/theme-api.md`,
`crates/ui/src/theme_editor.rs`, leídos sin copiar), sin extensiones, Wasm,
imágenes de fondo, métricas ni catálogo remoto. Todo en Ajustes ›
Apariencia › Tema; detalle en
[PLATFORM.md](../docs/PLATFORM.md#temas-propios-color-principal-y-transparencia-2026-10-19).

- [x] Editor de temas: base (Ónix, Oscuro o Claro), nombre, vista previa
  en miniatura que se repinta con cada cambio y los 22 colores de `Look`
  en cuatro grupos, cada uno con el `ColorPicker` de limn y
  «Restablecer»; la ventana cambia al guardar. Hasta ocho temas, en este
  equipo (`custom_themes`, `custom_theme`), con Editar, Exportar y Borrar
  (que pregunta). Elegir un tema integrado o Sistema deja el propio sin
  usar. `Ui::look()` reemplaza a `Theme::look()` en la vista.
- [x] Importar y exportar: `*.rune-theme.json`, datos validados enteros
  (versión, nombre, base, colores conocidos `#rrggbb[aa]`, nada más,
  64 KB como mucho); el selector y «Guardar como» del portal en
  `rune-core::theme_files`, fuera del hilo de la UI; la exportación nunca
  pisa un archivo.
- [x] Color principal: el del tema, los veinte preajustes de los
  selectores de color o cualquiera con el `ColorPicker`; texto sobre él
  blanco o negro por contraste WCAG y menciones teñidas (`accent_color`).
- [x] Transparencia de la ventana, 0–90 %: `with_transparent` y
  `clear_color` transparente si al abrir era mayor que 0
  (`~/.config/rune/transparency`); el riel, las columnas y la conversación
  no pintan su fondo y la raíz pinta uno solo con alfa
  (`Look::see_through`, `Look::backdrop`); menús, diálogos y ajustes
  siguen opacos. El desenfoque es del compositor (KWin, Hyprland):
  documentado.
- [x] Pruebas: formato, lista y validación de archivos, `Look` (tokens,
  acento, transparencia), vista previa y fila de color sin ventana, el
  flujo entero en `rune-ui` (crear, guardar, usar, borrar, importar,
  exportar, acento, transparencia), la página sobre la simulación y la
  escritura en `rune-core` (archivo nuevo, nunca encima). Capturas:
  [editor](../docs/img/temas-propios-editor-2026-10-19.png) y
  [color principal](../docs/img/temas-propios-color-2026-10-19.png)
  (Xvfb privado, simulación).

Pendiente y por qué:

- La transparencia no tiene captura: Xvfb no tiene compositor ni visual
  con alfa. Probarla en KDE Plasma y Hyprland, con y sin desenfoque.
- Con transparencia el riel y las columnas toman el color de la
  conversación (los paneles anidados no pueden sumar alfa sin verse
  disparejos) y los anillos de estado no recortan.
- El `ColorPicker` es opaco: un color translúcido del tema (bajo el
  puntero, elegido, sombra, mención) cambia sólo su RGB en el editor y
  conserva su alfa; el alfa se elige en el archivo.
- RUNE no pide el desenfoque nativo de KDE (`org_kde_kwin_blur`), como
  Serein; se puede sumar en `rune-platform`.
- Sin imágenes de fondo, tipografía ni métricas por tema (Serein las
  tiene): no estaban en el paquete.
- [x] El token `card` (tarjetas dentro de un mensaje) está en
  `Look::token_mut` y en `TOKENS` (rama `tarjetas-con-contraste`).
- Probar en el escritorio real: importar y exportar con el portal (KDE y
  GNOME), un nombre de archivo existente al exportar, la transparencia
  después de reiniciar, el color principal sobre cada tema.

## Administración del servidor y miembros (2026-10-19, rama `admin-servidor-y-miembros`)

Paquete 28. Sólo fixtures (loopback y simulación), sin probar contra
Discord; detalle de rutas en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#banner-ícono-animado-emojis-propios-comunidad-y-foro-2026-10-19-rama-admin-servidor-y-miembros).
Serein como referencia para el foro (no hace banner, ícono animado ni
comunidad: ahí, las rutas documentadas de `PATCH /guilds/{g}`), sin copiar
código.

- [x] **Ícono animado y banner del servidor**: `ServerEdit::banner`,
  `ProfileImageKind::ServerBanner` (16:9) y la subida animada de la edición
  de perfil (`Intent::PickProfileImage` con `animated` según
  `ServerAbilities::{animated_icon, animated_banner}`; si no, primer
  cuadro y la página lo dice). `ServerOptions::{animated_icon,
  banner_allowed, animated_banner, banner}` desde `ANIMATED_ICON`, `BANNER`,
  `ANIMATED_BANNER` y `banner`. Capacidad `server_banner`;
  `ServerAbilities::edit_banner`. La tarjeta del perfil muestra el banner.
- [x] **Emojis propios**: `ServerTrait::{emoji_id, emoji_animated}` y
  `ForumDefaults::reaction_emoji`; en la interfaz, `:nombre:` en el campo
  del emoji del rasgo o de la reacción (`server_settings::rest::own_emoji`).
  La sesión los lee y los escribe por id; la simulación y la sesión
  rechazan un emoji de otro servidor.
- [x] **Activar la comunidad**: `ServerEdit::{community, rules_channel,
  updates_channel}` en un solo `PATCH /guilds/{g}` con los `features`
  leídos frescos; `ServerOptions::{rules_channel, updates_channel}` y
  `community_ready()`. Capacidad `community`; `ServerAbilities::
  {toggle_community (administrador), edit_community (gestionar)}`. Sección
  «Comunidad» al final de «Seguridad y participación»: al activarla sube
  verificación a baja si no tenía y pone el filtro para todos; sin los dos
  canales, «No se guardó la comunidad» y no se manda nada.
- [x] **Foro**: `ForumDefaults::{archive_after, match_all_tags}` con
  `ARCHIVE_AFTER` (1 h, 24 h, 3 días, 1 semana); «Archivar después de» y
  «Filtrar por todas las etiquetas elegidas» en la página «Foro»; la vista
  del foro filtra con todas las etiquetas cuando el foro lo pide.
- [x] **Miembros con orden o filtro**: `MemberPages::since` (el reloj de la
  tienda al llegar la primera página) y `Store::list_newcomer`: quien entra
  después y cumple el filtro (últimos 7 días) aparece donde lo pone el orden
  (primero, por id de cuenta, o al final si ya no quedan páginas); si una
  página por venir lo traería, espera a esa página.
- [x] **Miembros en una ventana angosta**: la columna de roles ocupa a lo
  sumo 300 px y no más del 45 % de la fila (`roles_column`), y muestra dos,
  una o ninguna pastilla más «+n»; los `Select` de esas páginas no pasan
  del ancho de la columna, y «Últimos 7 días» baja bajo el orden.
- [x] Simulación: «Laboratorio limn» tiene ícono animado y banner (no banner
  animado), canal de reglas y de novedades; deja de ser comunidad (y apaga
  reglas e incorporación) y vuelve a serlo con lo que necesita.
- [x] Tests: modelo, simulación (banner, comunidad, emoji, foro), permisos,
  tienda (quien entra), sesión (lectura y cuerpos; loopback de comunidad
  con banner y rasgo propio, y rechazos sin salir), interfaz (banner y
  rasgo, comunidad, foro, filtro de la vista) y componente (fila angosta).
- Capturas (Xvfb privado, simulación, en español):
  [tarjeta con banner](../docs/img/server-banner-card-2026-10-19.png),
  [banner y rasgo `:bug:`](../docs/img/server-banner-traits-2026-10-19.png),
  [comunidad sin canal de reglas](../docs/img/server-community-2026-10-19.png),
  [foro: reacción propia, todas las etiquetas y archivado](../docs/img/forum-archive-tags-2026-10-19.png)
  y [miembros en 760 px](../docs/img/members-narrow-2026-10-19.png)
  (`--action ss:open:2 …`; para bajar en una página, `--action
  after:reveal:<id>`).

Pendiente y por qué:

- Las publicaciones nuevas de la sesión siguen pidiendo 3 días de archivado
  (`AUTO_ARCHIVE_MINUTES`) en vez del valor del foro: la capa HTTP no ve
  los ajustes del foro, y omitir el campo (que Discord use el del foro)
  cambia el cuerpo que prueban los tests de hilos; queda para quien toque
  `CreatePost`.
- El banner guardado se ve en la tarjeta si su imagen llega; la simulación
  sólo guarda un nombre, así que ahí se ve el color.
- Elegir un emoji del servidor es escribir `:nombre:`: no hay un selector
  con las imágenes en esos campos.
- Probar contra Discord (lista en DISCORD_SESSION.md).
## Tarjetas con contraste (2026-10-19, rama `tarjetas-con-contraste`)

- [x] Token nuevo `Look::card`, al final del struct y de cada tema: Ónix
  `#0a0a0c`, Oscuro `#12151c`, Claro `#e5e7ed`. Las tarjetas de un mensaje
  (invitaciones, embeds y vistas previas, adjuntos, encuestas, mensajes de
  voz, contenedores de bots, bloques de código y citas, reacciones, subidas
  en curso) dejan `raised`, que se confundía con la fila bajo el puntero, y
  llevan borde de 1 px en `border`; un spoiler revelado usa `selected`.
- [x] Test `look::tests::a_card_stands_apart_from_the_conversation_and_a_hovered_row`:
  ΔL\* ≥ 2,5 de la tarjeta contra el chat y contra la fila bajo el puntero,
  y ≥ 4 del borde, en los tres temas (con `card` = `raised` falla en Ónix
  con ΔL\* 0).
- [x] Capturas antes/después en Ónix y valores en
  [DISEÑO.md](../docs/DISEÑO.md#colores-del-tema-ónix-el-predeterminado).
- [x] Tras la mezcla con los temas propios: `card` en `Look::token_mut` y
  en `TOKENS` (Superficies, «Tarjetas de un mensaje»; `raised` pasa a
  «Campos y botones»), en español e inglés; un archivo de tema sin `card`
  toma el de su base (test en `theme.rs`).

Pendiente: el editor en lugar de un mensaje y la barra de acciones siguen
en `raised` (campo con borde y barra flotante, no tarjetas).
## Identidad de navegador (2026-10-19, rama `identidad-de-navegador`)

Decisión del dueño del 2026-10-19:
el proveedor no oficial se presenta como el cliente web en un navegador,
como Serein, con una sola identidad. **Sólo fixtures, sin probar contra el
servicio real.** Detalle en
[DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#identidad-de-navegador-2026-10-19-rama-identidad-de-navegador).

- [x] `rune-discord-session::browser`: el cliente web en WebKit sobre
  Linux (user agent de WebKitGTK 2.52, `X-Super-Properties` en el orden del
  cliente web, idioma y zona del sistema, Identify con las mismas
  propiedades). WebKit y no Chrome: es el motor de las ventanas.
- [x] Número de build de `https://discord.com/app` al arrancar
  (`GLOBAL_ENV.BUILD_NUMBER`, sin credencial, 5 s), respaldo 641004.
- [x] Todos los pedidos a la API pasan por `Http::api` con las cabeceras
  del cliente web; `Origin` sólo en escrituras; sin `X-Fingerprint` (el
  cliente web lo borra al iniciar sesión).
- [x] La aceptación de invitaciones lleva `X-Context-Properties` del modal
  de invitación, con la vista previa leída antes (también en el reintento).
- [x] Gateway y voz: mismo user agent y `Origin` en el apretón.
- [x] `rune-login`: las ventanas de login y hCaptcha fijan el mismo user
  agent.
- [x] Pruebas que afirmaban la identidad propia ahora afirman la del
  navegador (`browser::assert_presented`): user agent, propiedades
  decodificables y la credencial sólo en `Authorization`.
- [x] Docs: SECURITY.md, DISCORD_SESSION.md y REA.md.

Pendiente y por qué:

- Lo que todavía difiere del cliente web, fuera de este pedido: API v10 y
  Gateway v10 sin `zlib-stream` (el cliente web usa v9), el cuerpo `{}` de
  la aceptación sin el `session_id` de Gateway, el idioma de la cuenta (se
  usa el del sistema) y el user agent `RUNE/…` de las descargas del CDN de
  `rune-core`.
- `launch_signature` va en `null`: la firma el WebAssembly del cliente web.
- Probar contra Discord: volver a iniciar sesión, aceptar una invitación (y
  el hCaptcha si lo pide), ver si pide teléfono y si el resto sigue andando.
## Parpadeo de los emoji del selector (2026-10-19, rama `emoji-parpadeo`)

Con la cuenta real, los emoji de servidor del selector parpadeaban y bajaba
el rendimiento. El video del dueño muestra que no eran sólo los emoji: en
cuadros alternos faltaban también los íconos del carril (quedaban las
iniciales), los avatares de los miembros y la imagen de un mensaje.

- [x] Causa: la caché de texturas de `limn-egui` (64 entradas en RUNE)
  expulsaba la imagen más vieja aunque se estuviera dibujando. Con el
  selector abierto la pantalla pasaba de 64 imágenes, así que cada paso de
  pintado volvía a decodificar lo que el anterior había expulsado, las
  imágenes se turnaban para aparecer y cada decodificación que terminaba
  pedía otro cuadro.
- [x] Arreglo en limn (`rune/emoji-parpadeo`, `a99d980`): se expulsa
  primero lo dibujado hace más tiempo, nunca lo dibujado en el paso actual
  o el anterior. Cubre todo lo que pinta un `RasterImage` (mensajes,
  reacciones, carril, miembros, autocompletado, ajustes del servidor).
  PR #91 a `Niiihuel/limn` desde `fix/raster-cache-on-screen` (`0cee5a2`).
- [x] Tests que fallan sin el arreglo: uno en limn-egui y
  `apps/desktop/src/image_cache_tests.rs` (90 imágenes con el límite de
  RUNE: cada una se sube una sola vez y la pantalla se asienta).
- [x] Medición sin ventana, 90 imágenes a 60 pasos por segundo: antes,
  unas 1780 texturas subidas y liberadas por segundo y un cuadro pedido en
  todos los pasos; después, 90 subidas en total y ningún cuadro pedido
  desde el segundo segundo.

Pendiente y por qué:

- La simulación tiene 6 emoji de servidor: no llega a las 64 imágenes en
  pantalla, así que una captura no reproduce el parpadeo. Se probó sin
  ventana con imágenes simuladas.
- Probar contra Discord: abrir el selector con muchos servidores y emoji
  animados bloqueados sin Nitro, dejarlo quieto y confirmar que nada
  parpadea (emoji, carril, miembros, imágenes del canal) y que el uso de CPU
  baja al quedarse quieto.

## Eventos programados (2026-10-19, rama `eventos-programados`)

Paquete 32 del plan. Serein no los tiene; el dueño los incluyó el
2026-10-18. Simulación y tests sin ventana; la sesión no oficial **sólo
con fixtures en loopback, sin probar contra Discord**. Rutas y forma de la
REA del cliente web oficial ([REA.md](../docs/REA.md#cliente-web-oficial-eventos-programados-2026-10-19-rama-eventos-programados)).

- [x] **Modelo y proveedor.** `rune_model::scheduled_events`: nombre,
  descripción, inicio y fin, lugar (escenario, canal de voz u otro lugar
  escrito), estado (programado, en curso, terminado, cancelado), portada,
  quién lo creó, cuántos interesados y si le interesa a la cuenta; el
  borrador valida lo que pide Discord (nombre, lugar, fin si es afuera,
  inicio futuro, fin después del inicio). Bloque «Scheduled events» al
  final de `Command`, `Event`, `Failure` y `Capabilities`
  (`scheduled_events`, `manage_scheduled_events`); permisos
  `CREATE_EVENTS` y `MANAGE_EVENTS` (bits 39 y 40 de RUNE; 44 y 33 de
  Discord) en el catálogo de permisos de rol. Simulación (`mock::events`):
  reunión y asado en el servidor propio, charla en curso en el escenario y
  taller en el primero, salida del club; escrituras con permisos y
  validación como un servicio. Sesión (`events.rs`, `http/events.rs`):
  lista con `with_user_count` y lo que le interesa a la cuenta,
  interesados, «Me interesa» (`PUT`/`DELETE …/users/@me`), crear, editar,
  iniciar, terminar y borrar, y `GUILD_SCHEDULED_EVENT_*` del Gateway;
  hCaptcha común en las escrituras. Detalle en
  [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#eventos-programados-2026-10-19-rama-eventos-programados).
- [x] **Estado.** `rune_state::events`: la lista de cada servidor una vez
  leída en la sesión, ordenada (en curso, próximos, terminados), al día
  con lo que se informa; un cambio informado no pisa los interesados y el
  interés propio cuenta una vez. `Store::event_abilities` y
  `Store::may_change_event` (cualquiera con «Gestionar eventos»; lo propio
  con «Crear eventos»; nada terminado; aislamiento y reglas pendientes
  cierran crear).
- [x] **Interfaz.** Entrada «Eventos» arriba de los canales con cuántos
  vienen (pide la lista una vez por sesión). Diálogo con la lista (cada
  evento con cuándo, dónde, cuántos interesados y «Me interesa»), el
  detalle (portada, descripción, quién lo creó, interesados por nombre,
  «Editar», «Iniciar»/«Terminar» y «Borrar» para quien puede), el
  formulario con el `DateTimePicker` de limn (¿dónde?, nombre,
  descripción, inicio y fin) y la pregunta antes de borrar. Nada cambia
  antes de la respuesta; Esc retrocede. Textos en español y en inglés
  (`rune-i18n/src/en/events.rs`).
- [x] **Invitaciones de evento.** `?event={id}` en el enlace, el evento en
  `InvitationPreview::event` y la tarjeta con el evento sobre el servidor:
  «Ver evento» si la cuenta ya está, o la aceptación común y después el
  detalle. Ver [INVITATIONS.md](../docs/INVITATIONS.md#invitaciones-de-evento-2026-10-19).
- [x] Capturas simuladas (Xvfb privado de 1280×800):
  [lista](../docs/img/eventos-lista-2026-10-19.png) (`--channel limn-core
  --action after:ev:open`), [detalle](../docs/img/eventos-detalle-2026-10-19.png)
  (`… --action after:ev:show:8000000101`),
  [crear](../docs/img/eventos-crear-2026-10-19.png) (`… --action after:ev:new`)
  e [invitación](../docs/img/eventos-invitacion-2026-10-19.png)
  (`--channel general --action last:event-invitation`). Para la última, el
  escritorio suma la acción `last:event-invitation` (trae a la vista el
  mensaje más nuevo con una invitación de evento) y una captura de la
  simulación consulta las tarjetas de sus propios enlaces, como fuera de
  una captura (antes quedaban en «Consultando invitación…»; nada sale del
  equipo).

Pendiente y por qué:

- Portada al crear o editar: necesita el selector de imágenes y su
  recorte; se lee y se muestra, no se cambia.
- Eventos que se repiten (`recurrence_rule`, excepciones), cancelar sin
  borrar (`status: 4`) y la privacidad pública: el pedido era crear, editar
  y borrar; al editar, la repetición y la portada quedan como estaban.
- Enlaces `discord.com/events/{g}/{e}` en mensajes: sólo las invitaciones
  con `?event=`.
- Los eventos que trae el `GUILD_CREATE` no se leen: la lista se pide al
  mostrar el servidor (un `GET` por servidor y sesión).
- La entrada suma una fila arriba de los canales; en una ventana baja con
  el panel de voz abierto, los últimos canales quedan más abajo (un test
  de voz ahora los trae a la vista).
- Probar contra Discord (el dueño): la lista y los interesados frente al
  oficial, «Me interesa» en los dos sentidos, crear en un canal de voz y en
  otro lugar, editar, iniciar, terminar y borrar con «Gestionar eventos»,
  sin ese permiso sólo lo propio, cambios del oficial por el Gateway, una
  invitación con `?event=` y el hCaptcha si aparece.

## AutoMod (2026-10-19, rama `automod`)

Paquete 30. Serein no lo tiene; el dueño lo incluyó el 2026-10-18. Detalle
en [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#automod-2026-10-19-rama-automod),
[REA.md](../docs/REA.md#cliente-web-oficial-automod-2026-10-19) y
[ARCHITECTURE.md](../docs/ARCHITECTURE.md#administración-servidores-canales-roles-y-permisos).
Todo con la simulación y fixtures en loopback; **nada probado contra
Discord**.

- [x] Modelo y proveedor: `rune_model::automod` (`AutoModRule` con
  disparador de palabras, spam, listas del servicio, menciones o perfiles;
  palabras, expresiones regulares, permitidas y límite de menciones;
  acciones de bloquear con mensaje propio, alertar en un canal, aislar y
  bloquear la participación; roles y canales exentos; un tipo o una
  acción desconocidos se conservan y la regla no se edita). Límites de la
  documentación de Discord y `AutoModRule::problem`, que parsea las
  expresiones regulares con `regex-syntax` (dependencia nueva en
  `rune-model`, ya en el árbol por `regex`; la sintaxis que Discord dice
  usar). `Command`/`Event`/`Failure` (`LoadAutoModRules`,
  `SaveAutoModRule`, `SetAutoModRuleEnabled`, `DeleteAutoModRule`;
  `AutoModRules`, `AutoModRuleChanged`, `AutoModRuleRemoved`,
  `AutoModExecuted`, `AutoModBlocked`), capacidad `automod` y
  `ServerAbilities::manage_automod` (`MANAGE_GUILD`; un aislamiento propio
  la quita). `rune-state::automod` guarda las reglas leídas (una vez cada
  una; un cambio sólo entra en una lista ya leída) y las últimas 20
  acciones por servidor; una sesión nueva las vuelve a leer.
- [x] Simulación (`mock::automod`): «Laboratorio limn» tiene tres reglas
  («Sin spoilers» con palabras, una expresión regular, permitida, mensaje
  propio, alerta y un canal exento; «Spam de menciones» que bloquea y
  aísla; «Lo que parece spam», apagada) y «Taller de Rust» una que la
  cuenta, miembro común, no ve: bloquea `discord.gg/…` con «No compartas
  invitaciones a otros servidores.». Las palabras se comparan como lo
  documenta Discord (`*` a los lados, palabra entera si no); quien puede
  gestionar el servidor está exento; una respuesta simulada en el
  laboratorio que cae en una regla no se publica y la página la muestra en
  «Lo que hizo hace poco». Expresiones regulares, listas, spam y
  menciones se guardan pero no actúan, y la página lo dice.
- [x] Página «AutoMod» en Ajustes del servidor (bajo Moderación): las
  reglas con su interruptor, editar y eliminar (con confirmación); «Crear
  una regla» por tipo, mientras el servidor tenga lugar (6 de palabras, 1
  de cada otro); el editor en lugar de la lista, con Guardar y Cancelar
  arriba, nombre, palabras separadas por comas, expresiones regulares una
  por línea, permitidas, listas, límite de menciones y ataques de
  menciones, acciones (mensaje propio, canal de la alerta, duración del
  aislamiento) y exenciones; lo que Discord rechazaría no se manda y el
  aviso dice por qué (expresión regular inválida, sin palabras, límites,
  un canal o rol que ya no está), y el permiso se pregunta otra vez al
  guardar. Nada cambia antes de la respuesta. Capturas:
  [lista](../docs/img/automod-2026-10-19.png) y
  [editor](../docs/img/automod-editor-2026-10-19.png).
- [x] Mensaje bloqueado: la sesión lee el `message` de un `400` con
  código 200000 a 200002 (sólo para esos códigos, acotado y sin la frase
  genérica de Discord) y manda `Event::AutoModBlocked`; `rune-core` lo
  convierte en el `Event::Failed` del envío con «la moderación automática
  del servidor lo bloqueó: «…»», y la línea de fallo que ya existía dice
  «No se envió: …» con Reintentar y Descartar. Vale también para ediciones,
  envíos con archivos y títulos. Sin tocar el compositor. Captura:
  [mensaje bloqueado](../docs/img/automod-bloqueado-2026-10-19.png).
- [x] Sesión no oficial: rutas documentadas y del cliente web oficial
  (REA): `GET`/`POST …/auto-moderation/rules`, `PATCH`/`DELETE
  …/rules/{id}` (el `PATCH` sin `trigger_type`; activar o desactivar con
  sólo `enabled`), eventos `AUTO_MODERATION_RULE_*` y
  `AUTO_MODERATION_ACTION_EXECUTION`, validación previa (permiso, regla,
  `MODERATE_MEMBERS` para aislar, canal de la alerta) y el hCaptcha común.
  Tests con fixtures: leer, crear, editar, desactivar, borrar, sin
  permiso, regla incompleta, mensaje bloqueado con y sin mensaje propio,
  eventos del Gateway y verificación.

Pendiente y por qué:

- Probar contra Discord (el dueño): listar, crear, editar, desactivar y
  borrar en un servidor propio y compararlo con el oficial; con otra
  cuenta, escribir una palabra de una regla y ver el mensaje propio en la
  línea de fallo; ver si llegan los eventos del Gateway a una sesión de
  usuario (el bundle del oficial no los usa) y si el `message` del `400`
  trae el mensaje propio de la regla.
- No se hizo: reordenar reglas, la alerta de ataques de menciones del
  oficial (`clear-mention-raid`, `alert-action`), el aviso de cuarentena
  del perfil propio (ya existía como restricción del miembro), validar en
  el servicio (`…/rules/validate`) y los disparadores sin documentar
  (`SERVER_POLICY`, `APPLICATION`): se muestran pero no se editan. Al
  elegir el canal de la alerta el oficial lo agrega a los exentos; RUNE
  deja que lo elija la persona.
## Actualizaciones por tag (2026-10-19, rama `actualizaciones`)

Pedido del dueño: RUNE se actualiza desde las GitHub Releases de
`Niiihuel/rune`; una versión es obligatoria si el mensaje de su tag anotado
dice «obligatoria» y entonces se instala antes de abrir; si no, se ofrece
sin bloquear. Serein no tiene actualizador (sólo empaqueta releases).
Detalle en [UPDATES.md](../docs/UPDATES.md).

- [x] `crates/rune-update`: manifiesto `rune-update.json` (versión, fecha,
  notas, `minimum_version`, URL/tamaño/SHA-256 por plataforma) y su firma
  Ed25519 en `rune-update.json.sig`; la firma se verifica antes de leer el
  JSON con la clave compilada (`release-key.pub`, vacía hasta que el dueño
  la genere). Ed25519, SemVer y gzip ya estaban en el árbol.
- [x] Obligatoria contra opcional por `minimum_version`, la obligatoria más
  nueva hasta la publicada según los tags anotados.
- [x] Descarga con progreso y reanudación (`Range` desde el `.part`
  nombrado por el digest); se instala sólo si tamaño y SHA-256 coinciden.
- [x] Linux: instalación por usuario en `~/.local/share/rune/app` con
  `versions/` y el enlace `current` cambiado con un rename (todo o nada),
  la versión anterior conservada; `rune --install` con lanzador, entrada
  del menú e ícono; el inicio con la sesión apunta a `current/rune`.
- [x] Windows: contrato del instalador por usuario (nombre, carpeta,
  argumentos silenciosos, relanzar) y su código detrás de `cfg(windows)`.
- [x] Al iniciar (copia instalada o `RUNE_UPDATE_FEED`), búsqueda con 4 s
  de límite antes de cualquier login o ventana; la instancia única se
  reclama antes para que un segundo inicio no espere. Obligatoria: ventana
  limn propia con progreso y, si falla, el motivo con «Reintentar» y
  «Salir»; instalada, `exec` de la nueva. Opcional: aviso en la esquina
  («Actualizar y reiniciar», «Más tarde», «Notas»), que cierra guardando y
  arranca la nueva. Ajustes › Actualizaciones con versión, «Buscar ahora»,
  última verificación y «Buscar al iniciar» (encendido).
- [x] `.github/workflows/release.yml`: tag anotado = versión del workspace,
  borrador, build de Linux en Ubuntu 24.04 (apt, `session-login`), job de
  Windows apagado por la variable `RUNE_WINDOWS_RELEASES`, manifiesto,
  firma con el secreto (programa compilado antes), verificación con la
  clave compilada, licencias y publicación. `actionlint` limpio.
- [x] `scripts/update-key.sh`: la clave en la máquina del dueño, la pública
  a `release-key.pub` y la privada al secreto con `gh secret set`.
- [x] `examples/release.rs` (keygen, tag, manifest, sign, verify) y
  `examples/test_feed.rs` (feed local firmado con clave descartable, lento
  o corrupto).
- [x] Tests sin red real: firma válida e inválida, manifiesto alterado,
  otra clave, clave débil, obligatoria/opcional, `minimum_version`, SHA y
  tamaño que no coinciden, reanudación con y sin `Range`, instalación
  atómica en carpeta temporal, feed → descarga → instalación en loopback;
  interfaz (componentes, clics, aviso) y entrada del menú.
- [x] Probado a mano en NixOS con el feed local: `rune --install` en un
  `HOME` de prueba, la obligatoria de punta a punta (ventana, descarga,
  verificación, cambio de `current`, `exec`) y la opcional desde la ventana
  principal. Capturas en Xvfb: `docs/img/actualizaciones-*-2026-10-19.png`.

Pendiente y por qué:

- Lo hace el dueño: generar la clave (`scripts/update-key.sh`), commitear
  `release-key.pub`, hacer público el repositorio (hasta entonces las
  descargas piden autenticación) y empujar el primer tag.
- Sin probar: el build del workflow en Ubuntu (la primera release será la
  primera corrida), el binario genérico en una distribución común y en
  NixOS con `nix-ld` (configuración propuesta en UPDATES.md), GitHub
  Releases de verdad, y el código de Windows (compila sólo con el port).
- El instalador de Windows lo arma la rama `windows-instalador` (abajo);
  falta que el dueño ponga `RUNE_WINDOWS_RELEASES=true` en las variables
  del repositorio.
- Las releases genéricas no traen el visor de PDF (sus herramientas sólo se
  aceptan desde `/nix/store`) y el video por GPU depende de los plugins de
  GStreamer de la distribución.
- ~~`THIRD_PARTY_NOTICES.md` completo antes de distribuir binarios~~:
  hecho el 2026-10-19 (rama `preparar-publicacion`).

## Windows, primera etapa (2026-10-19, rama `windows-base`)

Pedido del dueño: «esto lo quiero para Windows, no sólo Linux». Detalle en
[PLATFORM.md](../docs/PLATFORM.md#windows-2026-10-19-rama-windows-base).
Nada se probó en un Windows real.

- [x] Compila y enlaza para Windows x86_64. Desde NixOS, con el target
  `x86_64-pc-windows-gnu`: `nix-shell --arg withWindows true` suma el GCC
  de MinGW-w64 y Wine (`cargo test` de ese target corre con Wine). El
  `rpath` de Linux del shell pasó a `CARGO_TARGET_<host>_RUSTFLAGS`, así no
  llega a un `.exe`. Las versiones van con MSVC en la CI.
- [x] Lo exclusivo de Linux sigue detrás de `cfg(target_os = "linux")` (o
  `unix`): video y pantalla, juegos, Rich Presence, fuentes, KWin/Hyprland,
  la WebView de login. Pruebas de Unix (enlaces simbólicos, modos, el
  descriptor 2) quedan en `cfg(unix)`.
- [x] `rune-platform`, cada servicio en su `windows.rs` detrás de la API de
  Linux: carpetas (`folders`, nuevo, también para Linux) y abrir enlaces y
  carpetas (`open`, nuevo); diálogos de archivo (`IFileOpenDialog`/
  `IFileSaveDialog`, y `open_local_files` abre rutas de Windows); llavero
  (Administrador de credenciales); notificaciones (toasts WinRT con
  `RUNE.Desktop` registrado en `HKCU`); tema claro/oscuro y **color de
  acento** (registro, seguido en vivo; con el tema «Sistema» tiñe la
  ventana); inicio con Windows (clave `Run`, respeta el Administrador de
  tareas; `autostart::{state, change}` nuevos para los dos sistemas);
  instancia única (tubería con nombre de la sesión); bandeja
  (`Shell_NotifyIconW`); atajos globales (`RegisterHotKey`, soltar por
  sondeo mientras se mantiene); portapapeles (`arboard`).
- [x] Registro a archivo también en Windows (`%LOCALAPPDATA%\RUNE\logs`), y
  `rune.exe` es un programa de ventana (`windows_subsystem`), sin consola.
- [x] Audio: cpal con WASAPI; el filtro de monitores ya era sólo de
  PipeWire.
- [x] Apagado en Windows y dicho en Ajustes: detectar juegos, actividad de
  otros programas, fuentes del sistema; pantalla y cámara ya lo decían; el
  login no oficial ya decía que hace falta Linux.
- [x] CI: job `windows` (`windows-latest`, actions por commit): clippy
  `-D warnings`, `cargo test` y las pruebas de servicios reales
  (`-p rune-platform -- --ignored`). **No corrió todavía**: corre cuando
  el dueño suba la rama.
- [x] Pruebas en Linux de lo que no necesita Windows: menú, comandos,
  tooltip e ícono de la bandeja; XML y botones de los toasts; atajos a
  teclas de Windows; comando del `Run` y el estado del Administrador de
  tareas; valores del registro a tema y acento; padre Win32 de un diálogo;
  protocolo de la instancia única; carpetas; enlaces que se abren; el
  acento del sistema en la interfaz. Con `--ignored`, en Windows: ida y
  vuelta del Administrador de credenciales, de la clave `Run` y de la
  tubería.

Verificado (detalle en PLATFORM.md): Linux, fmt, clippy y tests de todo
el workspace en verde; Windows gnu desde NixOS, clippy limpio, `rune.exe`
enlaza y abre con Wine; tests con Wine, todos menos una medición de tiempo
y un cierre del Gateway de prueba (corregido después, rama
`ci-windows-gateway`). Wine encontró dos errores, corregidos:
la tubería perdía a un cliente que escribía y se iba antes de la espera, y
«Guardar como» no notaba otro archivo puesto en el nombre. Sin ejecutar:
MSVC, la CI y un Windows real.

Qué tiene que probar el dueño en Windows: la lista de
[PLATFORM.md](../docs/PLATFORM.md#qué-tiene-que-probar-el-dueño-en-windows).

Pendiente y por qué:

- Otra etapa (otro trabajador): pantalla, cámara y video en mensajes;
  detección de juegos. La WebView2 del login y del hCaptcha la hizo la
  rama `windows-login`, y el instalador por usuario y el ícono del `.exe`
  la rama `windows-instalador` (secciones siguientes).
- `discord-ipc` en Windows es una tubería con nombre: no se hizo en esta
  etapa. Las fuentes del sistema (sin fontconfig) y los respaldos de
  fuentes del sistema tampoco: RUNE usa las suyas.
- El registro a archivo de Windows no se probó con un pánico que aborta
  (la prueba de Unix usa `_exit` y el descriptor 2).

## Windows, segunda etapa: login y hCaptcha con WebView2 (2026-10-19, rama `windows-login`)

Detalle en [PLATFORM.md](../docs/PLATFORM.md#login-y-hcaptcha-con-webview2-rama-windows-login)
y [DISCORD_SESSION.md](../docs/DISCORD_SESSION.md#identidad-por-plataforma-2026-10-19-rama-windows-login).
Sólo fixtures; nada se probó en un Windows real ni contra el servicio.

- [x] `rune-login/src/windows.rs`: las dos ventanas aisladas con WebView2
  (`webview2-com` 0.39 sobre el `windows` 0.62 de RUNE): proceso auxiliar
  por ventana, perfil InPrivate en una carpeta temporal propia que se borra
  cuando su navegador termina (la que deja un auxiliar matado, la borra el
  siguiente), navegación principal sólo a la página propia y marcos sólo a
  hCaptcha, sin ventanas nuevas, descargas, permisos, autenticación básica,
  herramientas, menús, atajos, contraseñas ni autocompletado; despertar
  sin datos y toma validada en el documento principal, como en Linux.
- [x] Lo común de las dos plataformas pasó a `rune-login/src/surface.rs`;
  `handoff.js` y `captcha.js` despiertan por el canal del motor que haya
  (WebKit o `chrome.webview`), tomado antes que los scripts de la página.
- [x] Sin WebView2 Runtime, los auxiliares terminan con el código 11 y RUNE
  dice que falta con el enlace oficial del instalador de Microsoft.
- [x] Identidad del motor: `rune-discord-session::browser::Engine`. Un
  auxiliar oculto (`--session-engine-helper`) lee en WebView2 el user
  agent, el vendor, las marcas de client hints, el idioma y la zona; el
  proveedor lo aprende antes de cualquier pedido y deriva `os`, `browser`,
  `browser_version` y `os_version` con las reglas del cliente web (REA:
  `getBrowser` mira `Microsoft Edge` en las marcas antes que el user agent;
  platform.js). REST suma las client hints. El user agent va ahora en cada
  pedido. Linux sigue con WebKitGTK, derivado por las mismas reglas.
- [x] El proveedor no oficial se ofrece en Windows (`session-login`).
- [x] Las ventanas de WebView2 llevan el ícono de RUNE
  (`assets/icons/rune.ico`, de `barra-e-icono`), la imagen del `.ico` más
  cercana a cada tamaño de Windows.
- [x] Pedido del integrador: reescritos a otra estructura los ajustes de
  WebKit, `discord_origin` y los callbacks de `captcha.js`. El fixture de
  node de `captcha.js` estaba roto (le faltaba `texts`) y se arregló.
- [x] `THIRD_PARTY_NOTICES.md` regenerado; la plantilla suma el cargador
  estático de WebView2 de Microsoft (BSD, del SDK de NuGet) que MSVC
  enlaza en `rune.exe`.

Verificado: Linux fmt, clippy `--all-targets --all-features -D warnings`
y tests del workspace; pruebas nativas de WebKitGTK de `rune-login` en un
Xvfb privado (25, también el hCaptcha sintético con el user agent);
fixtures de node de los dos scripts (WebKit y WebView2). Windows gnu:
clippy del workspace limpio; con Wine, pruebas de `rune-login --features
webview` y de la identidad, y el camino «falta WebView2 Runtime» (Wine no
lo trae).

Pendiente y por qué:

- Probar en un Windows real (lista en PLATFORM.md): Wine no tiene el
  WebView2 Runtime y bajarlo implica aceptar la licencia de Microsoft, que
  decide el dueño. Ninguna ventana WebView2 se abrió todavía.
- El apretón del WebSocket no lleva client hints y `Accept-Language` usa la
  regla de Linux: se cambia si Discord lo distingue.
- Leer el motor cuesta un arranque oculto de WebView2 en cada inicio del
  proveedor (alrededor de un segundo, sin medir).
- Los toasts necesitan que Windows acepte el ID registrado en `HKCU` sin
  acceso directo en el menú Inicio; por las dudas el instalador
  (`windows-instalador`) crea ese acceso directo con el mismo ID.
- Con Wine fallan dos pruebas (tiempo y cierre del Gateway de prueba como
  reset de TCP); si la CI de Windows también las ve, el cierre de la prueba
  necesita el saludo de cierre completo. `rune-update` compila para
  Windows (se gatearon sus importaciones y ayudas de prueba de Unix), sin
  ejecutar su instalación.

## Barra de título e ícono (2026-10-19, rama `barra-e-icono`)

Pedido del dueño: «usar el ícono que creé para la app y usar el componente
de minimizar, maximizar y cerrar de limn como usa surco».

- [x] El ícono del dueño (la runa blanca sobre la placa ámbar) en todos
  lados donde la app se representa: la ventana y la barra de tareas
  (`with_icon`, 256 px), `rune --install` (los PNG de `hicolor` de 16 a 512
  en lugar de la marca SVG, que se borra si quedó), las notificaciones de
  Linux (ícono `rune` y `desktop-entry`) y la bandeja de Linux y de Windows
  (en color, a 16, 22, 24, 32, 48 y 64 px o al tamaño chico de Windows,
  con el punto de menciones; decodificado con `png`, que ya se compilaba).
  Los PNG del repo están bien recortados (824 de 1024, la grilla de macOS)
  y con fondo transparente; no hizo falta regenerarlos. La placa del
  original deja pasar el 1 % (alfa 252), y así quedó.
- [x] `assets/icons/rune.ico` para el ejecutable y el instalador de
  Windows: 16 a 128 en BMP de 32 bits y 256 en PNG (`icotool` de icoutils
  desde los PNG de `hicolor`, pasados a 8 bits con ImageMagick).
- [x] Barra de título propia con `WindowControls` de limn (PR #89, fijado
  en `1407eef` de `rune/integration-2026-10-19`), como Surco: `Chrome`
  `System`/`Drawn` en `apps/desktop/src/frame.rs`; automática la de RUNE en
  Windows y X11 y la del sistema en Wayland. Los botones van arriba a la
  derecha sobre todo (`rune_ui::title_bar`), a 52 px de alto; la cabecera
  de la conversación y la del escenario de voz guardan sus 138 px.
  Arrastrar la franja libre mueve (`StartDrag`), doble clic maximiza o
  restaura, el borde de 5 px cambia el tamaño (`BeginResize`) sin
  maximizar; un clic sobre un control de limn no arrastra. Pantalla
  completa no los muestra. Cerrar es el cierre del sistema (guardar o
  bandeja). Con la de RUNE la ventana no baja de 868 px de ancho y la
  cabecera pliega sus herramientas 138 px antes, así nada queda bajo los
  botones ni en la ventana más angosta.
- [x] Ajustes › Sistema › Ventana › Barra de título: Automática, la de
  RUNE, la del sistema (en vivo); reemplaza «Ocultar las decoraciones», y
  quien las había ocultado queda con la de RUNE. `--title-bar` para una
  ejecución sin guardar.
- [x] Tests: decisión por plataforma y entorno, valor guardado, ruteo de
  `window:minimize/maximize/close` a `Intent::Window` y a los comandos de
  ventana, que un clic sobre un control no arrastre, doble clic, bordes,
  pantalla completa y maximizada, el ícono de 256, los tamaños de la
  bandeja y que ninguna cabecera quede bajo los botones (conversación,
  escenario de voz, inicio de sesión). Capturas en Xvfb con openbox:
  `docs/img/barra-de-titulo-*-2026-10-19.png`.

Pendiente y por qué:

- Sin probar en un escritorio real: mover y acoplar con el gestor de X11 y
  Aero Snap en Windows, cambiar el tamaño por los bordes en Windows (winit
  sin decoraciones), el ícono en la barra de tareas de Windows con el del
  `.exe`, y en Wayland el ícono después de `rune --install`.
- En Ajustes de usuario los botones (52 px) pisan los 8,5 px de arriba del
  panel flotante a la derecha, que ahí está vacío; sólo se nota al pasar el
  puntero.
- Wayland automático deja la barra del sistema: en Hyprland y GNOME es la de
  winit (CSD). Quien no la quiera elige la de RUNE.

## Preparar la publicación del repositorio (2026-10-19, rama `preparar-publicacion`)

Pedido del dueño: dejar el repositorio listo para hacerlo público.

- [x] Fixtures sin restos de la cuenta real: el ID y los nombres de un emoji
  de servidor, la biografía que los nombraba (también su forma en las
  pruebas de `rune-components`, del canal y del estado) y el apodo de la
  prueba de voz pasan a datos inventados (snowflake `1234567890123456789`,
  `Rana`/`rana`, `ada` / «Ada de prueba»).
- [x] Los bloques que seguían a Serein casi línea por línea, reescritos con
  otra estructura y los mismos tests (cuentas conectadas, enlaces de
  invitación, tipo MIME de subidas, GIF favoritos, carpetas, URL del
  Gateway, servidor MLS de prueba, casos del proxy). Método y lo que queda
  parecido a propósito: [SECURITY.md](../docs/SECURITY.md#revisión-antes-de-publicar-2026-10-19).
  `rune-login` (webview.rs, protocol.rs, captcha.js) lo reescribe la rama
  `windows-login`.
- [x] `THIRD_PARTY_NOTICES.md` generado con cargo-about (`about.toml`,
  `about.hbs`) para Linux y Windows, con las fuentes de egui y de limn,
  Twemoji, la marca de GIPHY, `vendor/hpke-rs` y ThumbHash escritos en la
  plantilla. El programa lo imprime (`rune --licenses`); el workflow de
  release lo regenera y corta si difiere, compara la salida del programa en
  Linux y en Windows y lo sube con la release
  ([UPDATES.md](../docs/UPDATES.md#avisos-de-terceros)).
- [x] Origen de los SVG propios de `assets/` en
  [ARTWORK.md](../docs/ARTWORK.md#recursos-propios-de-assets).
- [x] Lo que le pasó a la cuenta del dueño fuera del árbol:
  `docs/INCIDENT_DISCORD_INVITE.md` y el texto quitado de los demás
  archivos quedan en `privado/` (ignorado por git); las reglas y decisiones
  técnicas se quedan, dichas como decisiones del proyecto.

Pendiente y por qué:

- La historia de git sigue teniendo todo lo que se sacó (el documento del
  incidente, los nombres reales de `f50facf` y los fixtures de `ebef518`):
  publicar el repositorio con su historia lo publica. Decide el dueño:
  reescribir la historia o publicar una historia nueva.
- `THIRD_PARTY_NOTICES.md` hay que regenerarlo después de cada mezcla que
  cambie `Cargo.lock` (por ejemplo `windows-login` o el cambio de limn a
  GitHub); si no, el workflow de release se detiene.
- Sin probar: el workflow (instala cargo-about 0.9.0 en Ubuntu) y
  `rune.exe --licenses` con el subsistema gráfico de Windows.

## Windows, instalador y releases (2026-10-19, rama `windows-instalador`)

Decisión del dueño: instalador por usuario, sin permisos de
administrador, que el updater pueda reemplazar solo. Detalle en
[UPDATES.md](../docs/UPDATES.md#windows).

- [x] Instalador de Inno Setup 6 (`packaging/windows/rune.iss`), elegido
  porque trae hecho lo que pide el contrato (NSIS, el de Serein, necesita
  la API de Windows a mano): `PrivilegesRequired=lowest`, carpeta fija
  `%LOCALAPPDATA%\Programs\RUNE`, acceso directo en Inicio con el
  `AppUserModelID` `RUNE.Desktop` de los toasts, desinstalador (borra
  programa, acceso directo, ID de las notificaciones y valor `Run`; no los
  datos), modo silencioso con los argumentos que ya pasa el updater, cierra
  un RUNE abierto con el Restart Manager (`CloseApplications=force`) y lo
  vuelve a abrir, reemplaza siempre el `.exe`, español e inglés, registro
  en `%TEMP%` (`SetupLogging`), y junto a `rune.exe`
  `THIRD_PARTY_NOTICES.md`, `LICENSE-*` y `assets/licenses` (en
  `licenses\`). El contrato con el updater no cambió.
- [x] `rune.exe` sin DLLs extra: MSVC con `+crt-static` en la release;
  el workflow lista sus DLLs con `dumpbin`.
- [x] Ícono e información de versión dentro de `rune.exe` con
  `apps/desktop/build.rs` (`winresource` sin features, sólo actúa para
  Windows) y en el instalador (`SetupIconFile`), los dos con el
  `assets/icons/rune.ico` de la rama `barra-e-icono` (al mezclar se dejó
  el suyo en lugar del que había generado esta rama).
- [x] RUNE se pone el `AppUserModelID` del acceso directo al arrancar
  (`rune_platform::notifications::claim_application_id`): sin eso, abierto
  por el inicio con la sesión o por una actualización tendría otro botón
  en la barra de tareas que el anclado.
- [x] Updater de Windows: la búsqueda que no encuentra nada más nuevo
  borra `%TEMP%\rune-updates` (antes quedaba un instalador por versión).
  `test_feed --platform windows-x86_64` sirve el instalador sin comprimir.
- [x] Job `windows` de `release.yml` (`windows-latest`, actions por
  commit): build MSVC con `session-login`, DLLs en el log, Inno Setup 6.7.3
  verificado por SHA-256, instalador, tamaño y SHA-256 como salidas del job
  que el paso del manifiesto compara con lo que bajó del borrador antes de
  firmar. Sigue detrás de `RUNE_WINDOWS_RELEASES`. `actionlint` limpio.
- [x] Probado con Wine 11 WoW64 en Xvfb: ISCC arma el instalador;
  instalación silenciosa (también encima de otra) con archivos, acceso
  directo, registro y `[Run]`; ícono y versión en el `.exe`; el ciclo
  entero del updater (un RUNE instalado toma una 9.9.9 obligatoria del
  feed local, ejecuta el instalador y sale; el instalador reemplaza,
  sube la versión y vuelve a abrir RUNE; la búsqueda siguiente borra la
  descarga); desinstalación silenciosa que deja los datos.

Pendiente y por qué:

- Sin probar: el `AppUserModelID` dentro del acceso directo (Wine no lo
  guarda), cerrar un RUNE abierto con el Restart Manager (el de Wine no ve
  procesos), el asistente con ventanas, el build MSVC con `+crt-static`,
  el job del workflow y todo en un Windows real. La primera corrida del job
  dirá si algún crate de C no respeta `crt-static`.
- Firma Authenticode: no hay; SmartScreen avisa en la primera instalación
  y, si está encendido, el Control inteligente de aplicaciones de
  Windows 11 bloquea RUNE.
  Opciones en UPDATES.md; lo decide el dueño.
- El desinstalador no cierra un RUNE abierto: si corre, `rune.exe` queda
  en la carpeta. Cerrar RUNE antes de desinstalar.
- La CI de pull requests no arma el instalador: un error del `.iss` se ve
  recién en el job de la release (o con Wine, como dice UPDATES.md).
- El instalador no instala el WebView2 Runtime del login (rama
  `windows-login`): Windows 11 lo trae, Windows 10 por sus actualizaciones;
  sin él, el login da el enlace de Microsoft. Con MSVC el cargador va
  enlazado; un instalador armado con MinGW (prueba con Wine) lleva
  `WebView2Loader.dll` con `/DLoader=`.

## Windows, segunda etapa: medios y escritorio (2026-10-19, rama `windows-medios`)

Lo que la primera etapa dejó apagado, salvo el login (`windows-login`) y el
instalador (`windows-instalador`). Detalle en
[PLATFORM.md](../docs/PLATFORM.md#segunda-etapa-medios-y-escritorio-2026-10-19-rama-windows-medios)
y [MEDIA.md](../docs/MEDIA.md#implementado-pantalla-cámara-y-sonido-de-la-pantalla-en-windows-2026-10-19-rama-windows-medios).
Nada se probó en un Windows real.

- [x] Pantalla compartida: Windows.Graphics.Capture (1903+) con Direct3D 11
  propio, al mismo codificador (OpenH264); selector de RUNE en el menú de
  calidad («Pantallas», «Ventanas»), listado en un hilo; ventana cerrada o
  pantalla desconectada terminan con el motivo.
- [x] Sonido de la pantalla: loopback por proceso de WASAPI (2004+), sin
  los procesos de RUNE; mismo `Mix` y Opus que Linux.
- [x] Cámara: Media Foundation (lista por nombre, elige formato, lectura
  asíncrona, NV12/YUY2/I420/MJPEG convertidos por RUNE); permisos,
  ocupada, sin Media Foundation: lo dice.
- [x] Detectar juegos: procesos de la sesión con su ruta, misma lista.
  Apagado por defecto.
- [x] `discord-ipc`: tubería `\\?\pipe\discord-ipc-N`, mismo protocolo
  (`rich_presence/protocol.rs`), sólo la sesión. Apagado por defecto.
- [x] Fuentes del sistema y sus respaldos con DirectWrite.
- [x] Ajustes ya no dice que eso falta en Windows; «Aceleración por
  hardware de H.264» queda deshabilitada y dice que va OpenH264.
- [x] Pruebas: con Wine pasan las nuevas (formato y planos de la cámara,
  ids y orden del selector, ventanas ofrecidas, pantallas de Wine,
  procesos, la tubería de punta a punta, DirectWrite) y OpenH264; en Linux,
  `deliver`, MJPEG, el selector en `rune-components` y `rune-ui`.

Pendiente y por qué:

- Probar en Windows 10/11 (lista en PLATFORM.md): Wine no tiene
  Windows.Graphics.Capture, cámaras ni el loopback por proceso.
- Codificador por hardware en Windows (MFT H.264 de Media Foundation) y
  respaldo con DXGI Desktop Duplication: no hicieron falta para la primera
  versión; sin ellos va OpenH264 y Windows 10 1903+.
- El loopback no distingue aplicaciones: otra llamada abierta (el cliente
  oficial) se oiría en el sonido de la pantalla.
- Video en mensajes en Windows (no estaba en este pedido): MP4 con OpenH264
  es directo; WebM y MKV necesitan otro decodificador.

## CI de Windows: el cierre del Gateway de prueba (2026-10-19, rama `ci-windows-gateway`)

- [x] `gateway_4004_discards_pending_work_and_allows_only_a_new_explicit_login`
  fallaba en el job de Windows de la CI (y la del 4014 con Wine, 11 de 30
  vueltas): el Gateway de prueba mandaba el cierre y soltaba la conexión
  con frames del cliente sin leer, eso la resetea, y Windows descarta el
  cierre que el cliente no leyó todavía. El servidor de prueba ahora
  completa el cierre (lee hasta el del cliente o 1 s); con Wine, 0 fallas
  en 60 vueltas. Los otros servidores de prueba del Gateway y de voz
  guardan el socket hasta que el cliente terminó, salvo
  `transient_failures_exhaust_a_finite_reconnect_budget`, donde un reset y
  su 4000 dan el mismo reintento: quedó así.
- [x] El cliente real: no se puede leer lo que el sistema ya tiró, pero el
  código no se pierde; el reset es una caída, reconecta y una sesión
  rechazada vuelve a cerrarse con 4004, que la detiene. Prueba nueva en
  `gateway_tests.rs`. [PLATFORM.md](../docs/PLATFORM.md#qué-se-verificó-y-cómo).
## Visor de imágenes y videos (2026-10-19, rama `visor-de-medios`)

Pedido del dueño: «el componente donde puedo ver las imágenes y videos y
hacer zoom, pulirlo y mejorarlo muchísimo». Investigación (iced, gpui/Zed,
egui y Oculante/simp, GNOME Loupe, Discord, Telegram Desktop, Fotos de
Windows), decisiones y capturas antes/después en
[VISOR.md](../docs/VISOR.md). limn en la rama `rune/visor-de-medios`
(`Element::zoom_view`, `ImageViewer` nuevo, limn-egui).

- [x] Zoom que guarda el árbol (limn): rueda ×1,3 anclada al puntero,
  pellizco y Ctrl+touchpad, desplazar arrastrando o con el touchpad con
  inercia, doble clic a 2,5× y de vuelta, `+ = - 0 1 2 3`, Ctrl+flechas,
  Ctrl+R; de encaje a 20×, sin perder la imagen fuera de la vista; 200 ms,
  nada con movimiento reducido; porcentaje en una insignia y en la barra.
- [x] Imágenes chicas a su tamaño, centradas, sobre un fondo oscuro que
  deja ver la ventana (como Loupe, Discord y Telegram); antes el zoom de una
  imagen chica no mostraba nada.
- [x] Nitidez: vecino más cercano desde 2 píxeles de pantalla por píxel;
  copia reducida al achicar; damero sólo detrás de píxeles transparentes.
- [x] Galería: los archivos de un mismo mensaje (antes sólo las tarjetas),
  flechas, ←/→, Inicio/Fin y tira de miniaturas.
- [x] Acciones: copiar imagen (Ctrl+C), guardar como (Ctrl+S), abrir en el
  navegador, reenviar (con permiso), resolución original, girar; cerrar con
  Esc, el botón o un clic al costado.
- [x] Carga progresiva: ThumbHash → vista previa al tamaño final → original,
  que se pide solo al hacer zoom más allá de la vista previa.
- [x] Video en el mismo visor (antes un modal aparte), con sus controles,
  Espacio y ←/→ ±5 s; GIF animados siguen siendo superficies del host.
- [x] PDF: las páginas usan el mismo zoom.
- [x] `--capture-viewer` y `--action after:key:{tecla}` para capturas.
- [x] Miniaturas de la galería (rama `visor-miniaturas`): la captura del
  álbum mostraba el mismo paisaje en las tres; eran sus datos simulados
  (las tres tarjetas usaban la imagen animada de la tarjeta de GIF), no la
  tira. El álbum de `--capture-media` tiene ahora tres imágenes propias, y
  un test compara cada miniatura con la imagen que abre.
- [x] Tests: geometría y estado del zoom en limn (sin ventana), visor y
  limn-egui; en `rune-ui`, galería, teclas, zoom informado y original,
  reenviar, clic afuera, PDF y video.

Pendiente y por qué:

- Probar con la cuenta real: una imagen chica, una foto grande (el
  original al hacer zoom), un PNG transparente, un GIF, un video y un PDF.
- Ocultar los controles tras unos segundos quieto, rebote elástico en los
  bordes y giro animado: limn no tiene inactividad del puntero ni resortes
  para esto todavía.
- Pellizco en Linux: winit 0.30 no lo entrega (sí en macOS; en Windows
  llega como Ctrl+rueda).
- limn: PR #112 (`feat/visor-de-medios`, `14c2a6e`) apilada sobre
  `rune/integration-2026-10-19`: depende de las PR sin mezclar #84 y #86 y
  de `ImageViewer::action`. Falta subir `rune/visor-de-medios` y volver
  `Cargo.toml` a GitHub.
