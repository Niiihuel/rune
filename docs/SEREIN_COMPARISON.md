# Serein frente a RUNE: comparación de ingeniería

> Comparación a nivel de código entre Serein (referencia de solo lectura) y RUNE + limn.
> Continúa [SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md) (visión general, mapa de crates,
> licencias y mecanismos no oficiales), que aquí no se repite.
> El estudio de Serein es de solo lectura; no se ejecutó la referencia ni se
> incorporó su código. Las pruebas de RUNE se documentan por separado.

## Actualización de continuidad (2026-10-14)

### Inventario para la siguiente entrega paralela

Esta revisión contrasta los archivos actuales de RUNE con los dos clones:
`references/serein` en `d1af883c9d7dda90d1261a2d15fc852a5445204d` y
`references/serein-current` en `5f83077e33e10971159ec7dd56b5c7f21a9902b8`.
El segundo es la referencia principal de este inventario. Se leyó su fuente,
sin ejecutarla ni copiar código o recursos. Una función encontrada en Serein
no se considera probada ni una autorización para usar su protocolo.

**La brecha principal es de integración, además de UI.** RUNE ya tiene
componentes, estado, permisos y operaciones simuladas para varias funciones
que la sesión experimental todavía no ofrece. `rune-discord-session`, en
`src/provider.rs::info`, declara autenticación, servidores, conversaciones
privadas existentes, historial, envío/edición/borrado y stickers. La aceptación
de invitaciones está pausada. Las pruebas base de esta revisión pasaron:
636 aprobadas, seis ignoradas; no se hicieron operaciones sobre cuentas reales.

**Resultado de la entrega anterior:** se implementaron la entrada nativa de
invitación, tres filtros locales, el contrato de marcadores de lectura y la
corrección de presencia de privados sin soporte. La integración pasó
651 pruebas (seis ignoradas), formato, Clippy estricto y release; capturas y
límites en [WORKFLOW.md](WORKFLOW.md#continuidad-desde-ambos-clones-de-serein-2026-10-14).
Los puntos siguientes explican el diagnóstico y conservan los pendientes
de cada familia; esos cuatro incrementos ya no se consideran faltantes.

**Revisión documental del 2026-10-15:** se contrastaron de nuevo esos cuatro
recorridos con el árbol actual, sin ejecutar nuevamente Cargo ni modificar
su implementación. El plan vigente pasa a los
[21 recorridos pendientes](../tasks/todo.md#recorridos-pendientes-2026-10-15),
derivados del canvas de brechas de Discord y de este inventario. La
**inspección histórica de los artefactos de Discord quedó parcial y cerrada**.
Las reglas de entonces prohibían ingeniería inversa del cliente oficial.
Desde el 2026-10-16 [AGENTS.md](../AGENTS.md) autoriza la REA estática de
su paquete y sus bundles públicos ([REA.md](REA.md)) y la aceptación nativa
de invitaciones con el hCaptcha que resuelve la persona.

**Estados que se deben conservar al trabajar:** implementado localmente,
implementado con simulación, adaptador probado con fixtures, y confirmado por
el dueño contra el servicio son evidencias diferentes. Login, identidad y
lectura tienen confirmación previa del dueño; ninguna prueba local de esta
entrega amplía esa confirmación a escrituras, membresías o llamadas.

**Estado al 2026-10-18.** Las listas P0–P2 se escribieron el 2026-10-15;
desde entonces se hicieron los recorridos 01–22 de
[todo.md](../tasks/todo.md) y la paridad P1–P7 de
[serein-parity.md](../tasks/serein-parity.md). Cada punto dice ahora qué
hay y qué falta. «Hecho» es en la simulación y, en la sesión no oficial,
con fixtures en loopback; lo probado por el dueño contra Discord se dice
aparte. Sigue sin copiarse código de Serein.

#### P0 — Cerrar los recorridos existentes

- **Acceso y cuenta:** página limn, cancelación, errores, identidad y
  cierre local; sesión recordada en el llavero del sistema y varias cuentas
  (2026-10-16); selector de proveedor en Ajustes → Conexión y «Cambiar de
  cuenta» (2026-10-17). El dueño confirmó acceso, servidores, mensajes,
  miembros y perfiles contra Discord el 2026-10-17. Falta: el auxiliar de
  login fuera de Linux y la validación real del canje OAuth2 público.
  Fuentes: `rune-ui/src/auth.rs`, `rune-login`, `apps/desktop/src/main.rs`,
  [DISCORD_SESSION.md](DISCORD_SESSION.md).
- **Invitaciones:** entrada limn, preview público, tarjetas en mensajes con
  caducidad y reintento tras el cooldown; aceptación nativa reactivada el
  2026-10-16 con el hCaptcha que resuelve la persona en una ventana propia
  ([INVITATIONS.md](INVITATIONS.md)); crear, copiar, pausar y revocar
  (recorrido 19 y P7); reglas del servidor y onboarding al entrar, en un
  diálogo con barra sobre el compositor (2026-10-18, como Serein:
  preguntas y después reglas). Falta la prueba real.
- **Lecturas:** cursor local y, con `read_state`, ACK remoto por mensaje y
  servidor, «Marcar como no leído» y `MESSAGE_ACK` entrante con la versión
  del `READY` (P2); bandeja de menciones y no leídos (06). Falta el ACK en
  lote (`/read-states/ack-bulk`), que Serein tampoco usa.
- **Capacidades de las acciones visibles:** reacciones, escritura en curso,
  lista de miembros por suscripción, presencia, fijados, edición de perfil,
  administración y voz están habilitadas una por una en la sesión no
  oficial, cada una con su capacidad, confirmación y errores
  (`Capabilities`, `Store::abilities`, `Store::server_abilities`). Falta la
  prueba real de cada una.
- **Presencia desconocida en privados:** corregida el 2026-10-14
  (`view.rs::direct_rows`) y vigente: sin la capacidad no se dibuja.

#### P1 — Familias de funciones

- **Amigos, solicitudes y bloqueos:** hechos (recorrido 12): listas,
  solicitudes por nombre de usuario, aceptar, ignorar, bloquear, apodos y
  posible spam, con el hCaptcha resuelto por la persona.
- **Privados y grupos:** hechos (11): abrir y cerrar privados, crear
  grupos, nombre, ícono, integrantes y pasar la propiedad; llamadas en
  privados y grupos con timbre, transmisión y cámara (rama
  `llamadas-privadas`).
- **Hilos, foros y canales especiales:** hechos (13): crear, seguir,
  editar y borrar hilos, archivados, foros con etiquetas y publicaciones
  con archivos, anuncios. Escenarios: entrar al público, levantar la mano
  y pasar a hablar, con la moderación desde el menú de la persona (13.7).
- **Mensajes enriquecidos:** hechos (14): embeds y galerías, reenvíos,
  encuestas, mensajes de sistema, componentes y comandos de aplicaciones
  (P3, habilitados por el dueño el 2026-10-17).
- **Adjuntar y enviar archivos:** hecho (07): hasta ocho archivos con
  progreso y cancelación, pegar del portapapeles, hCaptcha en la subida.
  Falta arrastrar y soltar, bloqueado por el portapapeles de egui-winit en
  Wayland.
- **Reproducción y documentos:** hecho (16, 2026-10-17): animación, audio y
  video adjuntos dentro de RUNE (Symphonia, un lector MP4 propio y
  OpenH264; sin VP8/VP9/AV1/H.265, que ofrecen navegador o «Guardar
  como…»), GIF de tarjetas desde su MP4, galerías paginadas en el visor,
  spoilers de adjuntos y tarjetas, PDF multipágina con búsqueda y la
  resolución original en el visor de imágenes. Ver [MEDIA.md](MEDIA.md),
  [FILES.md](FILES.md) y [DOCUMENTS.md](DOCUMENTS.md).
- **Voz, vídeo y pantalla:** hechos (17, 20, 22): Opus, jitter, mezcla,
  transporte de voz con DAVE, referencia de eco y reducción de ruido,
  pantalla y cámara con H264 por la GPU u OpenH264, sonido de la pantalla.
  El dueño probó la voz el 2026-10-17: entró a veces; la causa probable se
  corrigió en la rama `voz-real`. La calidad de la llamada (RTT y pérdida)
  se hizo el 2026-10-18 en `voz-protocolo`, sólo con fixtures. Falta:
  `aead_aes256_gcm_rtpsize`, DMA-BUF, la prueba real de la calidad y la de
  video.
- **Perfiles y miembros:** hechos (15 y P5): perfil completo por demanda,
  conexiones e insignias, perfil y avatar por servidor, edición con avatar,
  banner, bio y pronombres, restricciones (timeout, verificación,
  cuarentena) y búsqueda de miembros (P2). Falta: marcos de perfil y la
  prueba real de la edición.
- **Servidores y administración avanzada:** hechos (19 y P7): roles con
  ícono y degradé, permisos, vetos, auditoría, invitaciones, emojis y
  stickers, webhooks, integraciones, reordenar canales en lote, ajustes de
  foro, Seguridad y Participación, y edición del onboarding (2026-10-18,
  que Serein no tiene). Los eventos programados, que Serein tampoco tiene,
  llegaron el 2026-10-19 (rama `eventos-programados`, sólo fixtures en la
  sesión), y también AutoMod, que Serein tampoco tiene (2026-10-19, rama
  `automod`, sólo fixtures en la sesión:
  [todo.md](../tasks/todo.md#automod-2026-10-19-rama-automod)).

#### P2 — Comodidad, plataforma y robustez

- **Búsqueda:** local (cargados e historial guardado) y remota con filtros
  (`from:`, `mentions:`, `has:`, `in:`, fechas, `pinned:`), páginas de 25 y
  autocompletado (P2, capacidad `message_search`).
- **Navegación:** Ctrl+K, Alt+↑/↓, atrás/adelante por cuenta, F6, salto al
  primer no leído, carpetas y orden del riel (P1), canales favoritos,
  privados fijados y anchos ajustables (P6), bandeja de menciones (06).
- **Notificaciones:** del sistema (`org.freedesktop.Notifications`) con
  clic que abre la conversación, sonidos propios y niveles por servidor y
  canal sincronizados con la cuenta (P1).
- **Expresiones:** selector con todos los servidores según el plan,
  paquetes de stickers, GIF del servicio y GIPHY opcional con favoritos
  sincronizados, emoji de servidor en la línea y animaciones (P5). Falta:
  stickers Lottie y la animación en el buscador de GIPHY.
- **Plataforma y ajustes:** atajos globales por el portal, español e
  inglés, bandeja, instancia única que pasa enlaces, inicio con la sesión,
  tema y fuente del sistema (P6). Falta: builds y CI de Windows y macOS,
  empaquetado, `THIRD_PARTY_NOTICES.md` y prueba con lector de pantalla. La
  caché local está cifrada por cuenta desde el 2026-10-18 y la usa también
  la sesión no oficial (sólo fixtures). La CI de Linux con `cargo deny` existe desde el
  2026-10-18 ([ROADMAP.md](ROADMAP.md#estado-y-siguientes-pasos-2026-10-18)).
- **Extensiones y cosméticos:** decoraciones, nameplates y efectos de
  perfil (P5); socket local `discord-ipc` y detección de juegos, cada uno
  apagado por defecto con su interruptor (P6, autorizados por el dueño el
  2026-10-17). Las extensiones Wasm de Serein no se replican.

#### Orden de trabajo y límites de interfaces

Entrada de invitación, filtros locales, alcance de lecturas y presencia
condicional son incrementos completados; contratos y evidencia histórica en
[tasks/plan.md](../tasks/plan.md) y [tasks/todo.md](../tasks/todo.md).
El nuevo [orden y sus dependencias](../tasks/plan.md#continuidad-desde-el-análisis-de-discord-2026-10-15)
prioriza identidad/persistencia y contratos existentes. Plataforma precede
la selección de archivos; contexto por ID precede búsqueda completa e inbox.
La reproducción local de audio puede avanzar de forma independiente de
privados/relaciones e hilos/foros. Cada frente
añade primero tipos/capacidades y eventos, después Store y UI, y por último
un adaptador cuando haya una vía verificable; así la UI se mantiene usable.

Se contrastaron las estructuras públicas de [canales](https://docs.discord.com/developers/resources/channel#channel-object-channel-types),
[invitaciones](https://docs.discord.com/developers/resources/invite) y
[scopes OAuth2](https://docs.discord.com/developers/topics/oauth2#shared-resources-oauth2-scopes).
Esas páginas describen recursos, no conceden acceso general a una cuenta.
La lista oficial de canales distingue hilos, foros y escenarios; los scopes
`identify guilds` no sustituyen los contratos de chat. El recurso público de
invitaciones no documenta una aceptación general de servidores para RUNE.

**limn:** menciones/emojis/imágenes inline, emoji en color y secuencias,
animación, modificador Alt y revisión/estimación de filas requieren contrastar
la API actual y, cuando se implementen, PR separado con tests, documentación,
changelog y galería. No se reemplaza limn. Fuentes y límites existentes en
[LIMN_INTEGRATION.md](LIMN_INTEGRATION.md). El decoder acotado y el fallback
de fuentes ya incorporados no deben volver a contarse como ausentes.

Las tablas de las secciones 1–5 y la lista «Adoptar» conservan el diagnóstico
de la **primera iteración**. Sus brechas no describen por sí solas el estado
actual. La revisión del árbol al iniciar esta iteración verificó:

- **Conversaciones y mensajes:** `Timeline` ya reconcilia páginas por rango,
  mantiene borrados recientes y rechaza ediciones más viejas
  (`crates/rune-state/src/timeline.rs`, `upsert`, `delete`, `apply_page`).
  `Delivery` representa envío pendiente, fallo y resultado desconocido;
  `Store` revierte cambios rechazados (`crates/rune-model/src/lib.rs`,
  `Delivery`; `crates/rune-state/src/lib.rs`, `failed`).
- **Sesiones y networking:** `Envelope` lleva generación, `Client::accept`
  descarta sesiones anteriores, hay plazos de historial/envío y el estado
  efímero usa `LatestValues` (`crates/rune-provider/src/lib.rs`, `Envelope`,
  `Latest`; `crates/rune-core/src/lib.rs`, `accept`, `expire`). El transporte
  disponible sigue siendo el proveedor simulado.
- **Presencia y multimedia:** ya existen `Activity`, estado personalizado,
  `CallPhase` y `CallQuality` (`crates/rune-model/src/lib.rs`); `rune-media`
  expone dispositivos, medidor y fallo del micrófono. Son contratos de
  dominio y audio local; no prueban una llamada remota autorizada.
- **Persistencia:** SQLite está en esquema 2, con borradores e instantáneas,
  migración transaccional y rechazo de versiones futuras
  (`crates/rune-storage/src/lib.rs`, `migrate`). La cola de trabajos está
  acotada y la poda se difiere hasta estar ocioso
  (`crates/rune-core/src/storage.rs`).
- **Renderizado:** hay índice `(ServerId, UserId)` para miembros y autores
  restringidos a los mensajes del timeline (`crates/rune-state/src/lib.rs`,
  `member_index`; `crates/rune-ui/src/timeline.rs`, `timeline`). La lista sigue
  usando `ScrollArea::measured_list`. Las revisiones selectivas de Serein
  siguen siendo una optimización futura, cuya necesidad debe medirse.

RUNE tiene ahora un repositorio Git sin commit inicial; el árbol estaba
preparado en el índice al empezar la revisión. La versión de limn que usa
Cargo está fijada en `Cargo.lock` a
`0da2317f6aa0d45daa5d1259ea958484006149f7` desde la rama
`rune/integration` del repositorio local `lienzo`. Las rutas y el commit de
limn citados en el diagnóstico histórico corresponden a su lectura original;
las propuestas de biblioteca deben contrastarse con esa dependencia actual
antes de abrir un PR.

Un ejemplo ya verificado en ese commit de limn: `tree.rs:8341-8344` poda las
alturas de claves ausentes cuando su cantidad supera `2 × filas + 256`.
La propuesta histórica de añadir esa poda ya está resuelta. La caché sigue
indexada solo por clave (`tree.rs:1360-1362`), así que una revisión de contenido
por fila sigue siendo una propuesta distinta que necesita un caso medido.

### Mejora elegida e implementada: coherencia de caché

Se corrigieron tres casos concretos sobre las abstracciones existentes:

1. **Crear/editar/cargar y borrar en el mismo drenaje.** `Client::accept`
   elimina del lote pendiente los cuerpos del `(canal, mensaje)` que se
   acaba de borrar. Antes el borrado se encolaba de inmediato y un guardado
   posterior en el mismo `poll` lo deshacía en SQLite. La regresión abre de
   nuevo la base, comprueba ausencia y cubre `poll(1)` y `poll(64)`, además
   del mismo ID en otro canal
   (`crates/rune-core/src/tests.rs`,
   `a_deleted_message_stays_deleted_in_cache_regardless_of_poll_size`).
2. **Poda aislada por conversación y cuenta.** `Storage::prune` utiliza
   `(channel, id)` además de `account`, como la clave primaria. Un ID
   repetido en otro canal ya no provoca un borrado colateral
   (`crates/rune-storage/src/lib.rs`,
   `pruning_scopes_repeated_message_ids_to_their_conversation_and_account`).
3. **Caché tardía tras un historial vacío autoritativo.** `Store::apply_cached`
   descarta la respuesta si `Timeline.latest` ya está establecido, aunque
   no haya mensajes. Dos pruebas conservan el arranque desde caché antes de
   recibir la página y verifican que una respuesta tardía no repuebla el
   canal ni cambia su revisión (`crates/rune-state/src/tests.rs`,
   `cached_messages_are_shown_before_the_newest_page_arrives`,
   `late_cached_messages_do_not_replace_an_authoritative_empty_page`).

Las regresiones fallaron antes de los cambios y pasaron después:
`rtk nix-shell --run 'rtk cargo test -p rune-core -p rune-storage -p rune-state'`
completó **43 pruebas**. Esta validación corresponde a RUNE; Serein siguió
siendo una referencia de solo lectura.

El patrón de Serein relevante es persistir el resultado final reducido y
delimitar la validez de las lecturas, explicado en
[SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md#persistencia-bajo-concurrencia-qué-conservar-del-patrón-v).
Esa corrección fue más acotada: todavía no añadía épocas ni reconciliación
en disco. La continuación del **2026-10-15** incorpora esos contratos en los
módulos propios de RUNE: mutaciones ordenadas, páginas autoritativas,
lápidas persistentes, identidad opt-in por proveedor/cuenta y respuestas
ligadas a época/petición/canal. La cola usa envío no bloqueante con devolución
de propiedad, un único lote pendiente en el núcleo y 8 MiB de presupuesto
compartido. El host conserva borradores/ajustes y espera una barrera confirmada
para cerrar o anunciar un borrado. No se copia la estructura de Serein.
El comportamiento y sus límites se documentan en
[ARCHITECTURE.md](ARCHITECTURE.md#coherencia-y-cierre-de-caché-revisión-2026-10-15).
Desde el 2026-10-18 la caché está cifrada por cuenta (clave en el llavero) y
la usa también el proveedor no oficial; OAuth2 sigue sin persistencia. Ver
[SECURITY.md](SECURITY.md#datos-locales).

Para limn sigue siendo apropiado estudiar referencias de imagen y cachés
con presupuesto, o revisiones de altura por fila. Este trabajo de coherencia
pertenece a `rune-core`/`rune-state`/`rune-storage` y no requiere ampliar la
biblioteca de UI ni crear otro crate.

## Resumen histórico: primera iteración

RUNE ya tiene, a escala pequeña, buena parte de la estructura que Serein valida:
- un único dueño del estado en el hilo de UI;
- colas acotadas que despiertan la UI;
- repintado reactivo;
- identificadores de petición para el historial;
- envío optimista con nonce;
- una lista virtualizada con ancla.

Lo que le falta es sobre todo **qué pasa cuando algo sale mal**. Serein dedica mucho código a eso.

**Brechas más importantes de RUNE, por orden de impacto:**

1. **Un envío fallido no existe como estado.** Hay tres caminos por los que un mensaje se queda gris "pendiente" para siempre:
   - el proveedor devuelve `Failed` sin nonce;
   - la cola de comandos está llena;
   - la sesión cae con el envío en vuelo.

   Serein modela `Delivery {Sending, Confirmed, Rejected, Ambiguous}` y ofrece "Restaurar al composer".
2. **No hay plazos ni generaciones.**
   - Una petición de historial cuya respuesta se pierde bloquea esa conversación: pedir lo mismo otra vez se ignora.
   - Los eventos no llevan generación de sesión.
3. **La ventana de mensajes puede quedar incoherente.**
   - Una página nueva se *fusiona* con lo que venía de SQLite o de la sesión anterior. Si no se solapan, queda un hueco que nunca se rellena.
   - Los mensajes borrados mientras RUNE estaba cerrado siguen visibles.
   - Las ediciones o borrados que llegan durante una carga se pierden.

   Serein reemplaza la ventana con la página fresca y guarda *patches* y *tombstones*.
4. **El estado efímero viaja "con pérdida".** Presencia y "está hablando" usan `send_lossy`. Si se pierde el último cambio, el estado queda pegado (p. ej. alguien "hablando" para siempre). Serein usa canales de último valor (`watch`) y lotes por usuario.
5. **Trabajo cuadrático por cuadro.** El timeline reconstruye cada cuadro un mapa de autores para *todos* los usuarios, con búsquedas lineales de miembros. La lista de miembros no está virtualizada y se ordena con búsquedas lineales. `Store::revision` existe pero nadie la usa. Serein cachea vistas derivadas con revisiones filtradas por dominio.
6. **SQLite.**
   - Un archivo con un esquema más nuevo se re-etiqueta como v1 en silencio.
   - La cola del hilo de almacenamiento no tiene límite.
   - Los borradores no se guardan, aunque la documentación del crate dice que sí.
   - Al arrancar, la UI espera a la poda de la base antes de pintar.
7. **Voz sin máquina de estados.**
   - No existen "uniéndose", "falló" ni "reconectando".
   - `JoinVoice` no lleva el estado de silencio, y la UI y el proveedor pueden discrepar.
   - Un error del micrófono solo se escribe en stderr.
8. **Emoji.** Serein no usa una fuente de color. Pinta un atlas PNG de **Twemoji (CC BY 4.0)** de 4009 celdas y coloca cada emoji en el texto con un "glifo hueco" del ancho del emoji. limn no tiene imágenes en línea dentro del texto, y por eso 🦀 sale como "□".

**Sorpresas:**
- Serein **no** pinta servidores ni canales desde la caché antes de READY. Solo siembra mensajes en la ventana vacía después de READY.
- Su `THIRD_PARTY_NOTICES.md` se contradice sobre si redistribuye emoji.
- Sus *shortcodes* de emoji derivan del mapa del cliente oficial de Discord.
- No expone calidad de conexión de voz, no usa FEC de Opus y la mezcla recorta "a lo bruto".
- Un `Resync` borra todo el historial en disco de la cuenta.
- RUNE también tiene una contradicción entre documentación y código (los borradores).

## Método de la comparación histórica

- **Versiones leídas:**
  - Serein en el commit `d1af883c` (`references/serein/`).
  - RUNE en el árbol de trabajo actual (no es un repositorio git).
  - limn en `/home/nihuel/projects/personal/limn-rune`, commit `b3658ff` (rama `rune/integration`).
- **Lectura:**
  - RUNE y limn los leí directamente.
  - Serein lo recorrieron cinco lecturas paralelas de solo lectura: estado, red, almacenamiento, voz y renderizado.
  - Comprobé a mano una muestra de sus citas: `view_revisions.rs`, `typing.rs`, `session-cache`, `local-store`, `pending.rs`, `voice.rs`, `mixer.rs`, `activity.rs`, `emoji.rs`, `avatars.rs`, `main.rs` y `THIRD_PARTY_NOTICES.md`. Todas coincidían.
- **Ejecución:** no se ejecutó nada (ni `cargo build` ni tests). Las afirmaciones sobre comportamiento en tiempo de ejecución que no salen directamente del código están marcadas como **(inferencia)**.
- **Rutas:**
  - En la columna Serein, relativas a `references/serein/`.
  - En la columna RUNE, relativas a la raíz de RUNE.
  - Las de limn llevan el prefijo `limn:`, que equivale a `/home/nihuel/projects/personal/limn-rune/`.
  - `:N` repite el archivo de la cita anterior en la misma celda.
- **Esfuerzo:** **S** = un día o menos; **M** = 2 a 5 días; **L** = más de una semana.
- **Alcance de "proveedor":** "Proveedor real" significa una integración autorizada y compatible con el producto descrito en [ROADMAP.md](ROADMAP.md), nunca los mecanismos de Serein listados al final. RUNE no es un administrador de bots.

---

## 1. Estado

| Aspecto | Serein (path:line) | RUNE (path:line) | Brecha | Recomendación (esfuerzo) |
|---|---|---|---|---|
| Aplicación de eventos | `State::apply` descarta los sobres de otra generación (`crates/client-core/src/lib.rs:2323-2334`). Un `match` grande (`:2565-3713`) delega en módulos por dominio (`read_state.rs:565`, `reactions.rs:401`, `voice.rs:734`). Si un brazo devuelve `Err`, el epílogo vacía la ventana y marca `Stale` (`lib.rs:3731-3737`). | `Store::apply(Event, now) -> Applied`, un `match` pequeño (`crates/rune-state/src/lib.rs:186-284`). Sin generación. `revision` crece en cada evento (`:187`) pero no tiene consumidores (no hay llamadas a `revision()`). | No hay salida "esto no cuadra, recargar". La revisión global no se usa. | Mantener el reductor pequeño. Añadir un camino explícito de resincronización por conversación (p. ej. `Applied::Stale(channel)`) para eventos incoherentes (**S**). Revisiones por dominio en §5. |
| Entidades normalizadas | Servidores y canales en `Vec`, con índice `BTreeMap<Id, usize>` reconstruido de forma perezosa (`crates/client-core/src/lib.rs:659-667`, `:1025-1082`). Topes de 131 072 entradas y 128 MiB (`crates/model/src/account.rs:2-3`). Sin tabla global de usuarios (`crates/client-core/src/user_actions.rs:198-223`). | `HashMap` de usuarios, canales y presencias (`crates/rune-state/src/lib.rs:160-166`). Servidores en `Vec` con búsqueda lineal (`:161`, `:440-442`). Miembros como `HashMap<ServerId, Vec<Member>>`, con `find` lineal en `display_name` y `top_role` (`:582-603`). Sin topes. | `display_name` y `top_role` cuestan O(miembros) y se llaman por usuario en cada cuadro (§5): O(usuarios × miembros). | Indexar los miembros por `(ServerId, UserId)`. Poner topes por número de entradas antes de conectar un proveedor real (**S**). |
| Ventanas por conversación | Una ventana activa de 500 mensajes, 4 MiB y 1024 mutaciones (`crates/session-cache/src/lib.rs:5-7`), más 2 aparcadas (`crates/client-core/src/resident.rs:9-11`). Un mensaje de un canal no seleccionado solo actualiza `last_message` (`crates/client-core/src/lib.rs:3427-3454`). | Un `Timeline` por conversación, `Arc<Vec<Arc<Message>>>` (`crates/rune-state/src/lib.rs:24-30`). Las no abiertas se recortan a 150 (`crates/rune-ui/src/lib.rs:189`, `:316`; `crates/rune-state/src/lib.rs:407-413`). Sin tope de bytes ni de número de timelines. | El número de timelines vivas crece con cada conversación abierta en la sesión: 150 × N mensajes (inferencia). | LRU de timelines (p. ej. las 8 últimas abiertas) además del recorte, y tope aproximado de bytes (**S**). El diseño de snapshots `Arc` de RUNE es mejor que la copia de Serein; conservarlo. |
| Página fresca frente a lo que ya había | `begin_page` marca `replace=true`, y `finish_page` conserva solo las filas cambiadas en vivo (`crates/session-cache/src/lib.rs:171-174`, `:336-371`). `Resync` vacía la ventana (`crates/client-core/src/lib.rs:3674-3699`). | `HistoryLoaded{before: None}` hace `merge` sobre lo que haya (`crates/rune-state/src/lib.rs:243-249`): lo sembrado desde SQLite (`:330-339`) o lo que quedó de la sesión anterior (`:319-323`). | Si la página nueva no se solapa con lo que había, quedan dos tramos con un hueco, y `LoadOlder` pide antes del mensaje más viejo (`crates/rune-ui/src/lib.rs:392-395`): el hueco no se rellena nunca. Los mensajes borrados mientras RUNE no miraba siguen visibles (inferencia; ningún test lo cubre). | En una página `before: None`, sustituir el rango que cubre y descartar todo lo anterior si no hay solape. Test: caché vieja + página sin solape (**S**). |
| Ediciones y borrados durante una carga | *Patches* por campo para filas aún no cargadas, con tope de 1 MiB (`crates/session-cache/src/lib.rs:398-453`), aplicados al llegar la fila (`:223-227`). La edición más vieja pierde frente a un `edited_at` más nuevo (`:232-237`). *Tombstones* (`:456-469`). | Un `MessageUpdated` de un mensaje no cargado se ignora (`crates/rune-state/src/lib.rs:222-229`). `MessageDeleted` solo quita el mensaje si ya está cargado (`:230-233`). | Una página en vuelo puede traer la versión vieja o resucitar un borrado (inferencia). | Mientras haya historial en vuelo, guardar *patches* y *tombstones* acotados por timeline y aplicarlos en `HistoryLoaded` (**S**). |
| Envío optimista | `Pending` vive aparte del timeline y se identifica por nonce (`crates/client-core/src/lib.rs:648-657`). `Delivery {Sending, Confirmed, Rejected, Ambiguous}` (`crates/model/src/lib.rs:482-488`). Se confirma por eco del gateway o por la respuesta REST (`crates/client-core/src/lib.rs:3548-3621`, `:3862-3878`). | El pendiente vive dentro del timeline con id `u64::MAX - nonce` y `pending: bool` (`crates/rune-state/src/lib.rs:342-370`; `crates/rune-model/src/lib.rs:244`). Se reemplaza cuando llega `MessageCreated` con el mismo nonce (`crates/rune-state/src/lib.rs:197-201`). | El caso feliz funciona, pero un `bool` no puede representar un fallo. | Cambiar `pending: bool` por `delivery: Delivery` (ver *Adoptar* n.º 1). |
| **Envío fallido y reintento** | `SendResult Err` lleva a `Rejected` o `Ambiguous` (`crates/client-core/src/lib.rs:3600-3617`). Con la cola llena se genera un `SendResult Err` sintético (`:2305-2318`). Si termina la sesión, todo lo que estaba en `Sending` pasa a `Ambiguous` (`apps/desktop/src/main.rs:6048-6054`). La UI muestra "Enviando / Entrega desconocida / No enviado" (`crates/ui/src/pending.rs:49-56`) y ofrece "Restaurar al composer" o "Descartar" (`pending.rs:244-255`; `crates/ui/src/lib.rs:2540-2558`). Nunca reintenta sola: "Never auto-retry writes" (`crates/discord-api/src/lib.rs:1698`). | **No existe el estado fallido.**<br>• `Event::Failed{action, reason}` no lleva nonce (`crates/rune-provider/src/lib.rs:153-156`; `crates/rune-provider/src/mock.rs:280-283`) y solo genera un aviso (`crates/rune-ui/src/lib.rs:362-370`).<br>• Con la cola de comandos llena, el comando se descarta con un `eprintln!` (`crates/rune-core/src/lib.rs:296-300`).<br>• Si la sesión cae, las tareas en vuelo se abortan sin evento (`crates/rune-provider/src/mock.rs:211-213`).<br>En los tres casos el mensaje queda gris para siempre (`crates/rune-components/src/message.rs:199-206`). | Es la brecha más visible para un usuario. | `Delivery {Sending, Failed, Ambiguous}`, fallos correlacionados por nonce, botones "Reintentar" y "Descartar", y `Sending` → `Ambiguous` al reconectar (**M**). Reintentar con un **nonce nuevo**, salvo que el proveedor declare envíos idempotentes (ver *Adoptar* n.º 1). |
| Ediciones y reacciones optimistas | — (fuera del alcance de esta lectura) | `update_local` aplica la edición o la reacción al instante (`crates/rune-ui/src/lib.rs:481-483`, `:527-529`; `crates/rune-state/src/lib.rs:373-389`). Un `Failed` no lo revierte. | Tras un fallo, la UI muestra algo que el servicio no tiene, hasta la próxima recarga. | Guardar el `Arc<Message>` anterior por operación y restaurarlo cuando llega el fallo correlacionado (**S**). |
| No leídos y menciones | `ReadState.entries: BTreeMap<Id,(last_read, epoch)>` (`crates/client-core/src/read_state.rs:62-71`). Versiones de ack monótonas (`:634`) y un solo ack en vuelo (`:525-555`). El contador es el del servicio más lo observado desde el último ack, acotado a 4096 entradas / 128 KiB, así que es una cota inferior (`crates/client-core/src/notifications.rs:114-125`, `:870-938`). El divisor "nuevos" se fija al entrar (`crates/ui/src/timeline.rs:2127-2158`). | `ReadState{last_read, last_message, mentions}` (`crates/rune-state/src/lib.rs:100-105`). Las menciones se cuentan localmente (`:210-212`) y un mensaje propio marca leído (`:207-209`). Se envía `MarkRead` cada vez que el final está a la vista y hay algo nuevo (`crates/rune-ui/src/lib.rs:397-401`). El divisor se fija al abrir (`:307-315`). `directs_unread` cuenta conversaciones no leídas, no menciones (`crates/rune-state/src/lib.rs:540`). | Con un proveedor real, se enviaría un `MarkRead` por cada mensaje recibido mientras se mira el final; puede chocar con los límites de tasa (inferencia). Las menciones solo se reconcilian con el servicio en `Ready`. | Un solo `MarkRead` en vuelo por canal, donde gana el último (coalescer en `rune-core`) (**S**). Documentar que la insignia de DMs cuenta conversaciones. |
| Escritura (*typing*) | Solo la conversación abierta, en 8 ranuras fijas. Plazo de 10 s menos la edad del evento; rechaza marcas más de 5 s en el futuro; guarda el **plazo**, no el inicio; se evalúa de forma perezosa (`crates/client-core/src/typing.rs:12-17`, `:44-117`). La UI repinta en el plazo, o cada 80 ms si tiene el foco (`crates/ui/src/typing.rs:205-212`). | Todos los canales, con un `Vec` sin tope por canal y 8 s desde la llegada (`crates/rune-state/src/lib.rs:19`, `:167`, `:260-265`). Una entrada caducada solo se borra cuando llega un mensaje de su autor (`:202-204`). `typing_expires` recorre todos los canales en cada cuadro (`:559-566`; `apps/desktop/src/main.rs:589`). | Memoria y trabajo crecen sin límite con canales ruidosos (inferencia). | Guardar el plazo, podar al insertar y poner un tope de 8 por canal (**S**). |

## 2. Red y canal de eventos

| Aspecto | Serein (path:line) | RUNE (path:line) | Brecha | Recomendación (esfuerzo) |
|---|---|---|---|---|
| Sobres y generaciones | `Envelope { generation, event }` (`crates/client-core/src/lib.rs:644-647`). La conexión etiqueta cada evento con la generación con la que arrancó (`apps/desktop/src/connection.rs:161-168`). Se descarta dos veces: en el drenaje (`apps/desktop/src/main.rs:5774-5776`) y en el reductor (`crates/client-core/src/lib.rs:2332-2334`). | Sin generación. `Ready` vacía `loading` y `live` (`crates/rune-core/src/lib.rs:189-193`), así que las respuestas de historial viejas se descartan. El resto de eventos de la sesión anterior se aplica. | Con el mock no se nota, porque aborta sus tareas al cerrar la sesión (`crates/rune-provider/src/mock.rs:211-213`). Con un proveedor real, un evento rezagado tras el nuevo `Ready` se aplicaría (inferencia). | `supervise` incrementa una generación por sesión, el `EventSink` la estampa y `Client::poll` descarta las viejas (**S**). |
| Cola fiable y desbordamiento | Cola de 4008 elementos **más** un semáforo de 32 MiB y un tope de 4 MiB por evento (`apps/desktop/src/connection.rs:879-912`). Si se desborda, **se corta la sesión** y se resincroniza (`connection.rs:942-965`). | `mpsc(1024)` solo por elementos (`crates/rune-core/src/lib.rs:120`). `EventSink::send` espera a que haya sitio: contrapresión (`crates/rune-provider/src/lib.rs:233-238`). | La contrapresión es correcta si quien lee el socket no es quien manda los latidos. Si es la misma tarea, un UI lento podría hacer caer la conexión por latidos (inferencia). No hay presupuesto de bytes. | Documentarlo en el trait `Provider`: "los latidos no deben depender de `send`". Añadir un tope por bytes cuando haya un proveedor real (**S**). |
| Con pérdida frente a último valor | *Typing*: `mpsc(8)`, se descarta si está lleno (`connection.rs:132`, `:920-925`), y se filtra en el productor: solo el canal abierto, 8 usuarios cada 2 s (`connection.rs:970-1005`). Presencia, actividad y fallo terminal van por `watch`, que **coalesce** (`connection.rs:133-150`). Presencia en lotes por usuario cada 100 ms (`crates/discord-gateway/src/presence.rs:15-52`). | `send_lossy` usa la **misma** cola de 1024 y descarta si está llena (`crates/rune-provider/src/lib.rs:240-246`). El mock lo usa para presencia (`mock.rs:425`), *typing* (`mock.rs:444`) y "hablando" (`mock.rs:487-489`). | Descartar está bien para eventos (*typing*) y mal para **estado**. Si se pierde el último `PresenceChanged` o el `speaking=false`, el valor queda pegado (inferencia, por construcción). | Un "último valor por clave" en el `EventSink` (mapa coalescente que el cliente drena), o canales `watch` separados (**S/M**). Ver *Adoptar* n.º 5. |
| Drenaje por cuadro | 8 eventos fiables más 8 de *typing* (`apps/desktop/src/main.rs:5740-5759`). Re-arma el repintado solo si queda trabajo (`connection.rs:895-898`). | 256 por cuadro (`apps/desktop/src/main.rs:30`, `:291-294`). Re-arma si llegó al tope. | Ninguna: aplicar es barato en RUNE. | Mantener. |
| Cola de comandos | `mpsc(16)` con `try_send`. Si falla, `command_rejected` convierte el comando en el fallo de su dominio: envío → `SendResult Err`, historial → `Stale` (`crates/client-core/src/lib.rs:1878-2322`; `apps/desktop/src/main.rs:4399-4405`). | `mpsc(256)` con `try_send`. Si falla, solo un `eprintln!` (`crates/rune-core/src/lib.rs:119`, `:296-300`). Los comandos que se encolan durante el *backoff* se entregan a la sesión siguiente (`:328-376`). | Un comando rechazado deja estado colgado (envío pendiente o historial "cargando"). | `Client::command` devuelve el rechazo y el cliente emite el fallo de dominio correspondiente, como `command_rejected` (**S**). |
| Reconexión y *backoff* | `(1000 << min(n,5)) ms` + *jitter* de 0–1000 ms, tope ≈ 32 s (`crates/discord-gateway/src/lib.rs:1189-1190`). Como máximo 6 intentos antes del primer READY (`:1182`). Reinicia tras 60 s estable (`:192-199`). "Reconectar ahora" con `Notify` (`apps/desktop/src/connection.rs:102-112`), que también dispara un envío hecho sin conexión (`apps/desktop/src/main.rs:3395-3401`). | 0,5 s × 2ⁿ⁻¹ hasta 30 s, ±20 % (`crates/rune-core/src/lib.rs:45-69`). El *jitter* es un hash del PID y del intento (`:72-76`). Reinicia tras 20 s (`:354-358`). `Fatal` para (`:346-351`). Sin "reconectar ahora". Reintenta sin límite antes del primer `Ready`. | No se puede forzar la reconexión. Si el primer arranque falla, se reintenta para siempre sin mostrar un error definitivo. | `Notify` que interrumpe el `sleep` (botón "Reconectar ahora") y N intentos antes del primer `Ready`, luego `Disconnected` (**S**). |
| Tras reconectar | `Disconnected`/`Resumed`: `Stale` + cancelar historial (`crates/client-core/src/lib.rs:3638-3672`). `Resync` vacía la ventana (`:3674-3699`). Vuelve a pedir la página visible (`apps/desktop/src/main.rs:5986-5996`). | `Ready` marca todas las timelines `latest=false` y **conserva** los mensajes (`crates/rune-state/src/lib.rs:319-323`). `on_ready` reabre la conversación (`crates/rune-ui/src/lib.rs:350-356`). | Tras una caída larga, mismo hueco que en §1 "Página fresca". | Reemplazo de rango (§1) (**S**). |
| Correlación de peticiones | Un contador por clase (historial, búsqueda, perfil) (`crates/client-core/src/lib.rs:1716`, `search.rs:64`, `profile.rs:367`) y una ranura `AbortTask` por clase: lo nuevo aborta lo viejo (`apps/desktop/src/connection.rs:264-279`). | `RequestId` por petición de historial, una por canal, y `Command::Cancel` (`crates/rune-provider/src/lib.rs:24`, `:36`; `crates/rune-core/src/lib.rs:226-268`). Las respuestas no esperadas se descartan (`:169-171`). | Equivalente; el de RUNE es más fino (por canal). | Mantener. Reutilizar `RequestId` para perfil y búsqueda cuando existan. |
| Nonce de envío | `String` con forma de *snowflake*, para imitar al cliente oficial (`crates/client-core/src/fingerprint.rs:118-126`) — **no adoptar**. | `u64` secuencial desde 1 en cada arranque (`crates/rune-core/src/lib.rs:283-284`). | Se repite entre ejecuciones. Bastaría con que fuera único por proceso (el mapa de pendientes vive en memoria), pero un proveedor que deduplique por nonce en el servidor necesita unicidad entre reinicios (inferencia). | Semilla aleatoria al arrancar + contador (**S**). |
| Plazos de comandos sin respuesta | REST: `timeout(20 s)` y `connect_timeout(10 s)`, sin reintentos (`crates/discord-api/src/lib.rs:179-187`). Gateway: conexión 15 s, Hello 10 s, cada envío 5 s, READY 30 s, ACK de latido por intervalo (`crates/discord-gateway/src/lib.rs:175-184`, `:1212-1303`). Un envío con *timeout* acaba en `Ambiguous` (`crates/discord-api/src/lib.rs:401-407`). | **Ninguno.** Si no llega respuesta a `LoadHistory`, la conversación queda en `loading` para siempre y la misma petición se ignora (`crates/rune-core/src/lib.rs:227-233`). `SendMessage` sin respuesta: pendiente eterno. | Bloqueo silencioso. | Plazo por petición en `Client` (`Loading { since }`, 20 s), comprobado en `poll`. Envío sin respuesta en 30 s → `Ambiguous` (**S**). |
| Escritor serie, concurrencia y tasa | Una tarea serie para las escrituras REST, para que un envío lento no bloquee colgar o silenciar (`apps/desktop/src/connection.rs:253-263`). `Semaphore(4)` (`crates/discord-api/src/lib.rs:218`). *Cooldown* compartido ante 429 o `remaining=0` (`:409-427`, `:482-487`). | El mock lanza una tarea por comando (`crates/rune-provider/src/mock.rs:219-236`). El orden se conserva solo porque la latencia es igual para todos (inferencia). | Con latencias variables, dos envíos podrían llegar desordenados (inferencia). | Para el futuro `rune-network`: escritor serie por cuenta, semáforo y *cooldown* global (**M**, cuando exista). |
| Decodificación acotada | REST hasta 4 MiB y gateway hasta 64 MiB; zlib acotado por trozos (`crates/discord-protocol/src/lib.rs:63`, `:66`; `crates/discord-gateway/src/compression.rs:27-55`). | Sin red todavía. | — | Mismos topes en `rune-network` (**S**, cuando exista). |

## 3. Almacenamiento

| Aspecto | Serein (path:line) | RUNE (path:line) | Brecha | Recomendación (esfuerzo) |
|---|---|---|---|---|
| Esquema | `messages` con PK `(account, channel, id)` e índice; `channels` (LRU); `drafts`; tablas *singleton* de apariencia; `app_preferences`; `channel_preferences`; `account_presence`; `accounts` (`crates/local-store/src/lib.rs:286-294`, `:431-488`). | `messages(account, channel, id, body JSON)` `WITHOUT ROWID` y `settings(key)` **sin cuenta** (`crates/rune-storage/src/lib.rs:51-61`). | Los ajustes se mezclan entre cuentas, aunque el crate dice "every row belongs to an account" (`:3-4`). | `settings(account, key)` y tabla `drafts(account, channel, content)` (**S**). |
| Migraciones | `user_version` = 29. **Rechaza** un archivo más nuevo (`crates/local-store/src/lib.rs:14-15`, `:282-285`). `ALTER TABLE` comprobando `pragma_table_info`, todo en una transacción (`:398-595`). | `SCHEMA = 1`. Crea las tablas si la versión es menor que 1 y escribe `user_version = SCHEMA` **siempre** (`crates/rune-storage/src/lib.rs:18`, `:47-66`). | Un archivo v2 abierto por un binario v1 se re-etiqueta como v1 en silencio (riesgo de corrupción tras volver a una versión anterior). | Si `version > SCHEMA`, devolver un error. Migrar dentro de una transacción (**S**). |
| Qué se persiste | Mensajes (500 por canal), borradores, preferencias, presencia propia, favoritos y últimos canales por servidor (`crates/local-store/src/lib.rs:1334-1378`; `crates/model/src/channel_preferences.rs:23-35`). **No**: entidades, estados de lectura, scroll ni miembros (informe de la lectura; `docs/storage-policy.md:888`). | Mensajes (500 por canal) y ajustes de interfaz: tema, servidor, canal, micrófono… (`crates/rune-ui/src/lib.rs:240-263`). **No**: borradores, aunque el crate lo afirma (`crates/rune-storage/src/lib.rs:2-3`); tampoco entidades ni estados de lectura. Los ajustes solo se guardan al salir o con `SaveSetting` (`apps/desktop/src/main.rs:599-603`). | Documentación y código no coinciden (el mismo error que se le señaló a Serein). Un cierre forzado pierde el servidor y el canal abiertos. | Persistir los borradores (con *debounce*) y corregir la documentación (**S**). |
| Arranque desde caché | **No** pinta servidores ni canales antes de READY: `State::default()` (`apps/desktop/src/main.rs:1458`). Después de READY siembra mensajes en una ventana vacía y en carga, comprobando generación, petición y época (`main.rs:1179-1215`; `crates/session-cache/src/lib.rs:330`), y la página fresca los reemplaza. | Igual en lo esencial: `apply_cached` solo escribe en un timeline vacío (`crates/rune-state/src/lib.rs:330-339`), pedido al abrir (`crates/rune-core/src/lib.rs:214-219`). **Sin** comprobar petición ni generación, y después se *fusiona* (§1). | La fusión provoca el hueco y los mensajes fantasma. | Reemplazo de rango (§1). **Opcional, más allá de Serein**: guardar un snapshot mínimo (servidores, canales, estados de lectura) para pintar el esqueleto al instante con la etiqueta "datos de la última sesión". Antes, revisar la retención exigida por la política del proveedor ([SECURITY.md](SECURITY.md)) (**M**). |
| Límites de tamaño | Techo duro de 64 MiB (`max_page_count=16384`). 20 ventanas y 48 MiB globales con expulsión LRU e `incremental_vacuum` (`crates/local-store/src/lib.rs:286`, `:1063-1101`). 4 MiB por ventana. Topes por columna JSON (`:957-1023`). Borradores: 8 KiB cada uno, 64 en total (`:1334-1357`). Omite filas sin cambios (`:948-954`). | 500 por canal, **podado solo al arrancar** (`crates/rune-core/src/storage.rs:38`). Sin tope global ni de bytes. | Durante una sesión larga, la base crece sin límite. | `max_page_count`, poda periódica (cada N guardados) y un tope global de canales (**S**). |
| Hilo y colas | Hilo dueño de la conexión. `sync_channel(16)` en cada sentido más un presupuesto de 48 MiB, con `try_send` (`apps/desktop/src/cache.rs:12-13`, `:318-330`). Con la cola llena muestra "Local storage queue full" (`apps/desktop/src/main.rs:2672-2675`). No agrupa escrituras de varias operaciones. | Hilo propio, pero `std::sync::mpsc::channel()` **sin límite** (`crates/rune-core/src/storage.rs:33-34`). Escribe en lote por cuadro dentro de una transacción (`crates/rune-core/src/lib.rs:198-201`; `crates/rune-storage/src/lib.rs:70-88`), lo que es mejor que Serein. `setting()` bloquea la UI (`crates/rune-core/src/storage.rs:98-102`) y al arrancar espera a que termine `prune` (`:38`; `apps/desktop/src/main.rs:166-168`). | Un disco lento acumula escrituras en RAM sin límite. Un `prune` lento retrasa el primer cuadro (inferencia). | `sync_channel` acotado con estado visible. Leer los ajustes **antes** de podar, o podar después del primer cuadro (**S**). |
| PRAGMAs y permisos | WAL, `synchronous=NORMAL`, `page_size`, `max_page_count`, `secure_delete`, `auto_vacuum=INCREMENTAL`, `journal_size_limit`, `busy_timeout` 2 s (`crates/local-store/src/lib.rs:281-286`). Directorio 0700 solo si no existía (`:216-231`). | WAL + `NORMAL` (`crates/rune-storage/src/lib.rs:37-38`). Archivo 0600 (`:32-36`). Directorio con los permisos por defecto (`:28-30`). | Sin techo de tamaño ni `secure_delete`. | Añadir `max_page_count`, `secure_delete=ON` y `auto_vacuum=INCREMENTAL` (este último solo surte efecto en bases nuevas o tras un `VACUUM`). Directorio 0700 (**S**). |
| Cerrar sesión y borrar | `forget_account` borra todas las tablas de la cuenta en una transacción, trunca el WAL y hace *vacuum* (`crates/local-store/src/lib.rs:1560-1580`). | No existe; está pendiente en [SECURITY.md](SECURITY.md). | Exigido por la política de datos de un proveedor real. | `Storage::forget_account(account)` y la opción "Borrar datos locales" (**S**). |
| Caché de imágenes en disco | Nombre = SHA-256 de la URL. Archivo `.part` creado con 0600 y renombrado. LRU por `mtime`. Presupuestos configurables: 1 GiB por defecto y 90 días (`apps/desktop/src/avatars.rs:2122-2241`; `crates/model/src/cache_preferences.rs:6-74`). | Desde el 2026-10-18 (`crates/rune-core/src/images/disk.rs`): el mismo diseño, reimplementado (nombre por digest de la URL, aquí con la clave de la cuenta; escritura atómica 0600; LRU por `mtime`; presupuestos y retención de Serein, medios y arte aparte), por cuenta y sellado con su clave; cambiar de cuenta no lo borra. | — | Hecho el 2026-10-18 (rama `imagenes-y-lectura`), reimplementado sin copiar código. |
| Errores de Serein a evitar | Un `Resync`, o quitar canales, borra **todo** el historial en disco de la cuenta (`apps/desktop/src/main.rs:5924-5928`). `end_session` borra la caché de imágenes también al cambiar de cuenta (`main.rs:2459-2462`), contra su propio comentario (`main.rs:2433-2434`). La documentación dice "DELETE journal" y el código usa WAL (`docs/storage-policy.md:711`). | — | — | No copiar. |

## 4. Multimedia (voz como estado; sin protocolo de Discord)

| Aspecto | Serein (path:line) | RUNE (path:line) | Brecha | Recomendación (esfuerzo) |
|---|---|---|---|---|
| Máquina de estados de la llamada | `Phase {Connecting, ConnectingTransport, Discovering, OpeningAudio, Ringing, Securing, Connected, Waiting, Failed}` (`crates/client-core/src/voice.rs:50-60`) dentro de `Call` (`:112-131`). `Progress` solo se acepta si coincide la petición, y `Failed` es definitivo (`:1039-1077`). Volver a unirse: 3 intentos a 1, 3 y 10 s; el presupuesto se repone tras 30 s estable (`apps/desktop/src/voice.rs:298-304`). El error terminal va en un `OnceLock` para que una cola llena no lo pierda (`apps/desktop/src/voice.rs:166-167`). | No hay fase. La pertenencia se deduce de `VoiceState` (`crates/rune-model/src/lib.rs:284-290`; `crates/rune-state/src/lib.rs:168`, `:576-578`). `JoinVoice` se confirma con un `VoiceChanged` (`crates/rune-provider/src/mock.rs:340-343`, `:1338-1350`). | No hay "Conectando…", "Reconectando" ni "Falló". Un *join* sin respuesta no muestra nada. | `CallPhase {Joining, Connected, Reconnecting{attempt}, Failed(reason)}` con id de petición y `Failed` definitivo (**M**). Ver *Adoptar* n.º 8. |
| Participantes | `Participant` (`Copy`): `user, muted, deafened, server_muted, server_deafened, video, streaming`, **sin** "hablando" (`crates/client-core/src/voice.rs:76-87`). Tope de 64 participantes (`:9`). "Hablando" vive aparte (`crates/ui/src/lib.rs:463`). | `VoiceState{user, channel, muted, deafened, speaking}` en un `Vec` sin tope (`crates/rune-model/src/lib.rs:284-290`; `crates/rune-state/src/lib.rs:270-281`). | Mezcla pertenencia (fiable) con "hablando" (efímero) en el mismo evento, y este va con pérdida (`crates/rune-provider/src/mock.rs:487-489`). | Separar `speaking` en un conjunto de último valor (**S**). |
| Detección de voz / VAD | Puerta de entrada por nivel: umbral por defecto −55 dB, histéresis de 3 dB, 200 ms de retención y rampa de 5 ms (`crates/discord-voice/src/activity.rs:21-49`; `crates/model/src/voice_settings.rs:38`). El indicador remoto sale de la energía del audio decodificado: −45 dBFS y 200 ms de retención (`crates/discord-voice/src/activity.rs:1-9`). Llega a la UI cada ≈100 ms, solo si cambia, por `watch<[u64;64]>` (`crates/discord-voice/src/transport.rs:546-555`; `apps/desktop/src/voice.rs:1541`). | Solo un medidor RMS → dBFS con suelo de −60 (`crates/rune-media/src/lib.rs:18`, `:185-198`). La UI lo muestrea cada 80 ms (`apps/desktop/src/main.rs:309-317`). | Sin puerta ni retención. | Puerta con histéresis y retención en el hilo de proceso (fase 2 de [MEDIA.md](MEDIA.md)). Indicador con retención de 200 ms (**S** cuando exista audio). |
| Silencio y ensordecer | Silencio efectivo = propio, sin permiso, servidor, ensordecido o PTT (`apps/desktop/src/voice.rs:895-904`). Atómicos `Gate` en los *callbacks*, más generaciones para tirar el audio en vuelo (`crates/discord-voice/src/audio.rs:54-76`, `:136-149`, `:491-504`). Ensordecer conserva el silencio previo (`crates/client-core/src/voice.rs:138-149`). Se persiste (`crates/local-store/src/lib.rs:64-65`). | `Ui.muted`/`deafened` (`crates/rune-ui/src/lib.rs:155-156`). Activar ensordecer fuerza el silencio, y quitarlo **no** restaura el estado anterior (`:751-766`). `JoinVoice` no lleva el silencio (`crates/rune-provider/src/lib.rs:69-71`) y el mock entra sin silenciar (`mock.rs:1338-1350`). El medidor recibe `set_muted` en cada cuadro (`apps/desktop/src/main.rs:309-310`). | Dos fuentes de verdad: la UI puede mostrar "silenciado" mientras el proveedor transmite (inferencia). | `JoinVoice { channel, muted, deafened }`. Guardar el silencio previo al ensordecer. Que la verdad sea el `VoiceState` propio (**S**). |
| Dispositivos | Se enumeran en un hilo, con topes (`crates/discord-voice/src/audio.rs:26-53`). Cambio en caliente por revisión; si se sigue al predeterminado, se comprueba cada segundo (`:221-236`, `:447-457`). Recuperación: 24 intentos cada 250 ms (`:537-539`). Vigilancia del micrófono: 5 s (`:305-311`). Entrada y salida persistidas. Volumen por usuario `[(u64,u16);64]` (`crates/discord-voice/src/mixer.rs:89-96`). | La enumeración va fuera del hilo de UI (`apps/desktop/src/main.rs:248-258`). Se guarda el id del micrófono (`crates/rune-ui/src/lib.rs:261`, `:284`). Un error del *stream* solo hace `eprintln!` (`crates/rune-media/src/lib.rs:200`), y el medidor se queda en el último valor (inferencia). El `cpal::Stream` vive en el hilo de UI (`apps/desktop/src/main.rs:133`, `:231`), contra la regla 4 de [MEDIA.md](MEDIA.md). | Si se desconecta el micrófono, la UI no se entera. | `AtomicBool failed` que el *callback* de error marca y la UI muestra. Llevar el *stream* al hilo de control previsto en MEDIA.md (**S**). |
| Calidad de conexión | **No** se expone nada (ni RTT, ni pérdida, ni *jitter*). Solo diagnósticos en stderr con `SEREIN_VOICE_DIAGNOSTICS` (`crates/discord-voice/src/diagnostics.rs:242-258`). | Nada. | Ninguno de los dos. | `CallStats { rtt_ms, loss, jitter_ms }` opcional en el modelo, para que un proveedor lo rellene si puede (**S**, baja prioridad). |
| Hilos y colas | *Callbacks* CPAL → `rtrb` de 8 tramas → hilo DSP `voice-audio` → `sync_channel(8)` → hilo de transporte con su propio runtime → UI por `watch` y atómicos. Sin *snapshot* (`crates/discord-voice/src/audio.rs:196`, `:630-631`, `:733`; `apps/desktop/src/voice.rs:1534-1564`; `crates/discord-voice/src/timer.rs:92-128`). | El *callback* hace aritmética y un `AtomicU32::store` (`crates/rune-media/src/lib.rs:185-198`). El diseño de fases 2 y 3 en [MEDIA.md](MEDIA.md) (§4) es el mismo patrón. | Ninguna de diseño; falta implementarlo. | Seguir MEDIA.md. Controles UI → audio por "último valor" (`watch`) en vez de llamadas por cuadro (**M**, fase 2). |
| *Jitter buffer*, Opus y mezcla | 8 ranuras, arranque a 40 ms, descarta duplicados y viejos, PLC hasta 3 seguidos (`crates/discord-voice/src/jitter.rs:2-69`). Opus mono VoIP a 64 kbit/s y 20 ms, **sin FEC ni DTX** (`crates/discord-voice/src/transport.rs:371-384`; `crates/discord-voice/src/mixer.rs:123`). Mezcla con `clamp(-1,1)`, sin limitador (`mixer.rs:147-150`). | Planificado en [MEDIA.md](MEDIA.md) §3, fase 2: adaptativo de 40 a 200 ms, con FEC y limitador. | Ninguna (aún no hay código). | Empezar con el diseño fijo de Serein (8 ranuras + PLC) y añadir dos mejoras baratas que Serein no tiene: decodificar con FEC cuando llega el paquete siguiente y un limitador suave (**M**). |
| AEC / NS / AGC | Sonora (AEC3 + NS), RNNoise (`nnnoiseless`) y AGC2, en el hilo DSP y nunca en el *callback* (`crates/discord-voice/src/audio/echo.rs:59-164`). Ambos BSD-3-Clause (`THIRD_PARTY_NOTICES.md:158-164`, `:172-174`). | Planificado (MEDIA.md §2.9). | — | Usarlos como dependencias; las licencias son compatibles (ver *Licencias*). |

## 5. Renderizado

| Aspecto | Serein (path:line) | RUNE / limn (path:line) | Brecha | Recomendación (esfuerzo) |
|---|---|---|---|---|
| Virtualización | `TimelineView` propio: `rows: Vec<(Id,f32)>`, ancla `(Id, inset)` y `following` (`crates/ui/src/timeline.rs:112-127`). 100 px de *overscan*, medidos en un `Ui` hijo recortado (`:2585-2616`). | `ScrollArea::measured_list` (`crates/rune-ui/src/timeline.rs:207-219`). limn mide solo las filas del viewport (`limn:crates/limn/src/tree.rs:8053-8059`), con hasta 8 pasadas (`:1285`), y ancla por clave (`:8018-8029`). | Equivalente. limn recalcula claves y sumas prefijas de **todas** las filas en cada pasada (`limn:crates/limn/src/tree.rs:7938-7954`); es O(n) y bastante para miles de filas. | Mantener. |
| Caché y estimación de alturas | La clave es un hash del contenido, la agrupación, los separadores y las menciones (`crates/ui/src/timeline.rs:319-357`, `:1049-1073`). Estimación por contenido: `58 + 18·líneas`, más adjuntos (`:2253-2274`). Si cambian la fuente o la escala, se invalida todo; si cambia el ancho, solo se re-miden las visibles (`:2162-2218`). | Las alturas van por **clave de fila** (id del mensaje), no por contenido (`limn:crates/limn/src/tree.rs:1259-1279`). Una estimación fija de 48 px por lista (`crates/rune-ui/src/timeline.rs:19`; `limn:crates/limn/src/tree.rs:7940-7943`). El mapa `heights` no se poda nunca (`:1275`). | Una edición o reacción fuera de pantalla deja una altura vieja hasta que la fila vuelve a verse; el ancla lo compensa. Con una estimación fija, saltar lejos hace bailar la barra de scroll (inferencia). | En limn: estimación por fila y clave `(key, revisión de contenido)`; podar a las claves presentes (ver *Ideas para limn*) (**S**). |
| Paginación | Solo con scroll explícito hacia arriba a menos de 160 px; un *layout* en reposo nunca drena historial (`crates/ui/src/timeline.rs:4399-4408`). | `LoadOlder` cuando la primera fila visible es la ≤5 (`crates/rune-ui/src/lib.rs:392-395`). La deduplicación (`crates/rune-core/src/lib.rs:227-233`) y el ancla evitan las cadenas. | Menor. | Mantener; considerar exigir una intención de scroll si aparecen cargas en cadena. |
| Decodificación de imágenes | Hilo `avatar-cache`: 8 trabajos con un semáforo global de 8 alrededor de `spawn_blocking` (`apps/desktop/src/avatars.rs:469-493`, `:1420-1435`). Resultados con presupuesto de 128 MiB; la UI recoge como mucho 32 por cuadro (`apps/desktop/src/main.rs:5376-5391`). Reduce al tamaño físico necesario con una escalera de tamaños, porque no hay *mipmaps* (`crates/ui/src/avatars/media.rs:12-14`, `:1196-1221`). Usa ThumbHash como marcador de posición (`apps/desktop/src/avatars.rs:1006-1031`). | RUNE no tiene imágenes; muestra un marco con "Vista previa simulada" del tamaño de las dimensiones (`crates/rune-components/src/message.rs:292-316`). limn-egui lanza **un `std::thread::spawn` por imagen** (`limn:crates/limn-egui/src/lib.rs:300-309`) y genera miniaturas de 384 px (`:163`, `:244-252`). `RasterImage` lleva los bytes en base64 + SHA-256 dentro del árbol (`limn:crates/limn/src/raster_image.rs:6-14`). | Sin límite de concurrencia. El modelo de "bytes en el elemento" no sirve para avatares y adjuntos que llegan de la red. | En limn: referencia de imagen + almacén del host + *pool* de decodificación acotado (ver *Ideas para limn*) (**M**). |
| Cachés de texturas | *Pools* separados: emoji 1024 / 16 MiB (para que los medios no los expulsen), general 512 / 64 MiB, inactividad 60 s, LRU (`crates/ui/src/avatars.rs:33-39`, `:450-462`). Animaciones: una textura reutilizada, ≤1 subida cada 16 ms (`:109-125`). | limn-egui: iconos 512 entradas (`limn:crates/limn-egui/src/lib.rs:151`); imágenes 16 + 2 a resolución completa, **solo por número** (`:164-165`, `:317-328`). | Sin presupuesto de bytes ni *pools*. | `ImageCachePolicy` por *pool* en limn-egui (**S/M**). |
| **Emoji** | **Atlas de Twemoji 17.0.3**, no una fuente de color: 4009 celdas de 30 px en un PNG de 2048×2016, más 4009 SVG comprimidos con zstd para tamaños grandes (`assets/twemoji/README.md:10-26`), incluidos con `include_bytes!` (`crates/ui/src/emoji.rs:6-8`). Segmenta por grafemas (`crates/ui/src/markdown.rs:2046-2050`). Búsqueda exacta quitando U+FE0F; U+FE0E se respeta como texto (`crates/ui/src/emoji.rs:123-142`). **Glifo hueco**: un espacio de fuente escalada con el ancho del emoji (`emoji.rs:110-121`); tras maquetar, localiza esos huecos, pinta la celda del atlas encima y devuelve el carácter original para que copiar funcione (`crates/ui/src/markdown.rs:2139-2213`). Tamaño en línea = Body × 1,6 (`crates/ui/src/emoji.rs:106-108`). Licencia **CC BY 4.0** (`THIRD_PARTY_NOTICES.md:146`). | El selector pinta el emoji como texto (`crates/rune-components/src/emoji.rs:8-12`, `:32-35`). limn-egui conserva el *fallback* monocromo de egui (`limn:crates/limn-egui/src/lib.rs:2048-2082`), que no cubre 🦀 y lo pinta como "□" ([LIMN_INTEGRATION.md](LIMN_INTEGRATION.md)). `TextSpan` solo admite un `Icon` en línea (`limn:crates/limn/src/element.rs:509-527`). | Sin color ni cobertura. Faltan imágenes en línea en el texto. | En limn: span de imagen en línea más un proveedor de emoji con atlas (ver *Ideas para limn*). Arte: Twemoji (CC BY 4.0, con atribución) o, como alternativa Apache-2.0, los PNG de `googlefonts/noto-emoji` (inferencia: verificar su `LICENSE`) (**M**). |
| Avatares | Círculo; hasta 2 iniciales en uno de 5 colores elegido por hash del nombre (`crates/ui/src/design.rs:1541-1570`). Se pide `?size=128` (`apps/desktop/src/avatars.rs:765-773`). Se pinta el *fallback* mientras carga (`crates/ui/src/avatars.rs:1453-1457`). | `limn::Avatar` con iniciales y el acento del usuario (`crates/rune-components/src/lib.rs:74-81`). `limn::Avatar` ya acepta `RasterImage` (`limn:crates/limn/src/avatar.rs:7-12`), pero `User` no tiene campo de imagen (`crates/rune-model/src/lib.rs:78-88`). | Faltan el modelo y la carga. | `User.avatar: Option<ImageKey>` cuando exista la referencia de imagen (**S**). |
| Markdown | `pulldown-cmark` acotado a 8192 B, 128 líneas y 512 eventos (`crates/ui/src/markdown.rs:9-14`, `:700-707`). Extensiones: menciones, *spoilers*, `<t:…>`, emoji personalizados (`:1207-1280`, `:726-729`). Caché LRU por `(Id, parte)` validada por igualdad de la fuente: 512 entradas / 1 MiB, podada a las filas mostradas (`:227-281`; `crates/ui/src/timeline.rs:2234-2235`). | `limn::Markdown` por fila (`crates/rune-components/src/message.rs:207-214`), con caché de 64 entradas / 4 MiB (`limn:crates/limn/src/markdown_cache.rs:19-20`). Sin menciones ni *spoilers* ([LIMN_INTEGRATION.md](LIMN_INTEGRATION.md)). | Funciones, no rendimiento. | Ver *Ideas para limn* (**M**). |
| **Trabajo por cuadro** | Revisiones filtradas por dominio (`catalog`, `channel_list`, `labels`, `rail`) que ignoran el ruido de mensajes, reacciones y miembros (`crates/client-core/src/view_revisions.rs:15-73`). El timeline se reconstruye solo si cambia `state.revision` (`crates/ui/src/timeline.rs:2182-2197`). | La vista entera se construye **dos veces** por cuadro y se maqueta dos o tres (`apps/desktop/src/main.rs:513-515`, `:541-552`). En cada construcción:<br>• `timeline()` arma un mapa de autores para **todos** los usuarios, con `top_role` y `display_name` lineales (`crates/rune-ui/src/timeline.rs:153-168`; `crates/rune-state/src/lib.rs:582-603`).<br>• La lista de miembros **no está virtualizada** y se ordena llamando a `display_name` (lineal) dentro del comparador (`crates/rune-ui/src/view.rs:472-500`; `crates/rune-state/src/lib.rs:648-652`).<br>• `server_unread` recorre todos los canales por servidor (`crates/rune-ui/src/view.rs:138`; `crates/rune-state/src/lib.rs:524-532`). | Hoy no se nota con el mundo simulado. Con un servidor real de miles de miembros, O(M² log M) por cuadro (inferencia). | Índices (§1), autores solo de las filas visibles, lista de miembros con `measured_list`, y vistas derivadas cacheadas por revisión de dominio (**M**). Ver *Adoptar* n.º 6. |
| Repintado | Reactivo. `request_repaint_after` para animaciones (≤16 ms), puntos de *typing* (80 ms con foco), marcas de tiempo relativas (20 s) y caducidades. Repintado continuo solo durante fundidos y deslizamientos de scroll (`crates/ui/src/avatars.rs:110-128`; `crates/ui/src/typing.rs:205-212`; `crates/ui/src/markdown.rs:1393-1394`; `crates/ui/src/timeline.rs:2533-2544`). | Reactivo. Mínimo entre animación de limn, caducidad de *typing*, avisos y medidor de 80 ms (`apps/desktop/src/main.rs:582-596`). | Equivalente. | Mantener. Cuando haya marcas de tiempo relativas, repintar cada 20–60 s solo si están visibles. |

## Presencia

| Aspecto | Serein (path:line) | RUNE (path:line) | Brecha | Recomendación (esfuerzo) |
|---|---|---|---|---|
| Estados | Propio: `PresenceStatus {Online, Idle, DoNotDisturb, Invisible}` (`crates/model/src/lib.rs:492-532`). De otros: `status: Option<String>` ("online"/"idle"/"dnd"/"offline") (`:671-693`), con tipos débiles. | `Status {Online, Idle, DoNotDisturb, Offline}` (`crates/rune-model/src/lib.rs:99-105`). | RUNE no puede ponerse "invisible". El enum de RUNE es mejor que las cadenas de Serein. | `Invisible` solo para el estado propio (p. ej. `SetStatus(OwnStatus)`), detrás de una capacidad (**S**). |
| Estado personalizado | `OwnPresence { status, custom_status, expires_at_ms }` (`crates/model/src/lib.rs:535-541`). El emoji Unicode se mete en el texto; los personalizados se descartan; 128 caracteres como máximo (`crates/discord-protocol/src/presence.rs:192-212`). | `Presence.custom: Option<String>` (`crates/rune-model/src/lib.rs:109-112`), sin caducidad ni emoji. No hay comando para fijarlo (`crates/rune-provider/src/lib.rs:78`). | Sin caducidad ni edición. | `CustomStatus { text, emoji: Option<String>, expires_at: Option<Timestamp> }` y `Command::SetCustomStatus` (**S**). |
| Actividades | `RichActivity { kind, name, details, state, image, small_image, started_at, ends_at }`, como máximo 4 (`crates/model/src/lib.rs:490`, `:595-645`). `kind: u8` (0 Jugando, 1 Emitiendo, 2 Escuchando, 3 Viendo, 5 Compitiendo). | No hay. | — | `activities: Vec<Activity>` con `ActivityKind` enum, tope de 4 y capacidad `presence_activities` (**S**). No leer procesos locales ni imitar el IPC de Rich Presence (ver *No adoptar*). |
| Almacén y límites | `direct_presences` solo de amigos y DMs: 256 entradas / 512 KiB, FIFO (`crates/client-core/src/lib.rs:743`; `crates/client-core/src/presence.rs:4-5`, `:191-246`). Las de servidor solo para la lista abierta, con tope de 128 KiB (`presence.rs:318-391`). Actualización por *patch*; pasar a desconectado borra actividades y estado (`presence.rs:10-89`). | `HashMap<UserId, Presence>` sin tope, con reemplazo completo (`crates/rune-state/src/lib.rs:164`, `:266-269`). | Sin tope; con un proveedor real crecería con cada miembro visto (inferencia). | Tope por número de entradas. "Desconectado" borra el estado personalizado (**S**). |
| Transporte | `watch` y lotes por usuario cada 100 ms (`crates/discord-gateway/src/presence.rs:15-52`). | `send_lossy` (`crates/rune-provider/src/mock.rs:425`). | Una presencia perdida queda vieja para siempre. | Último valor por usuario (*Adoptar* n.º 5). |
| Persistencia del estado propio | `account_presence(account, status, custom_status, expires)` (`crates/local-store/src/lib.rs:474-479`). | No se persiste. | — | Guardarlo en ajustes por cuenta (**S**). |

---

## Propuestas de adopción de la primera iteración

Registro histórico de ideas independientes del proveedor, ordenadas por valor.
Varias ya están implementadas, como explica la actualización inicial; no es
una lista vigente de pendientes. Ninguna requiere copiar código de Serein.

1. **Estado de entrega y reintento de envíos** (**M**).
   - **Cambio:**
     - `Message.pending: bool` pasa a `delivery: Delivery {Sent, Sending, Failed{reason}, Ambiguous}`.
     - Fallo correlacionado por nonce.
     - Al reconectar, `Sending` pasa a `Ambiguous`.
     - Botones "Reintentar" (nonce nuevo) y "Descartar / volver al composer".
     - Reintentar con el **mismo** nonce solo si el proveedor declara envíos idempotentes. La API oficial de Discord documenta `enforce_nonce` para esto (inferencia: verificar en la documentación oficial antes de declararlo).
   - **Archivos:**
     - `crates/rune-model/src/lib.rs`: `Message`, `Delivery`.
     - `crates/rune-provider/src/lib.rs`: `Event::SendFailed { nonce, reason, ambiguous }` o `Failed { …, nonce }`; capacidad `idempotent_send`.
     - `crates/rune-provider/src/mock.rs`: emitir el nonce en el fallo; `MockControl::fail_next_send` para tests.
     - `crates/rune-state/src/lib.rs`: `pending`, `mark_failed`, `ambiguous_all`.
     - `crates/rune-core/src/lib.rs`: `send_message` y `command`.
     - `crates/rune-components/src/message.rs`: fila "No enviado · Reintentar · Descartar".
     - `crates/rune-ui/src/lib.rs`: acciones `msg:{id}:retry` y `msg:{id}:discard`.
     - `crates/rune-storage/src/lib.rs`: seguir sin guardar lo que no está confirmado (`:76`).
2. **Ningún comando queda colgado** (**S**).
   - **Cambio:**
     - Un rechazo de `try_send` se convierte en el fallo de su dominio (el `command_rejected` de Serein).
     - Plazo de 20 s por petición de historial y de 30 s por envío, comprobados en `poll`.
     - Después, se permite volver a pedir.
   - **Archivos:**
     - `crates/rune-core/src/lib.rs`: `Loading { since }`, `command`, `poll`.
     - `crates/rune-provider/src/lib.rs`: `request: Option<RequestId>` en `Failed`.
     - `apps/desktop/src/main.rs`: repintar en el próximo plazo.
3. **Generación por sesión** (**S**).
   - **Cambio:** `supervise` numera las sesiones, `EventSink` las estampa y `Client::poll` descarta las anteriores.
   - **Archivos:** `crates/rune-provider/src/lib.rs`, `crates/rune-core/src/lib.rs`.
4. **Coherencia de la ventana de mensajes** (**S/M**).
   - **Cambio:**
     - Una página `before: None` **reemplaza** el rango que cubre y descarta lo que no se solapa, para que no haya huecos ni mensajes fantasma de la caché o de la sesión anterior.
     - *Patches* y *tombstones* acotados mientras hay historial en vuelo.
     - Revertir ediciones y reacciones optimistas cuando llega un fallo correlacionado.
   - **Archivos:**
     - `crates/rune-state/src/lib.rs`: `HistoryLoaded`, `MessageUpdated`, `MessageDeleted`, `apply_cached`, más tests nuevos.
     - `crates/rune-ui/src/lib.rs`: `submit`, `react`.
5. **Último valor en vez de pérdida para el estado efímero; *typing* acotado** (**S/M**).
   - **Cambio:**
     - Presencia y "hablando" se coalescen por clave: el último gana y nunca se pierde.
     - `send_lossy` queda solo para *typing*.
     - *Typing* guarda el plazo, se poda al insertar y tiene 8 ranuras.
   - **Archivos:**
     - `crates/rune-provider/src/lib.rs`: `EventSink::send_latest(key, event)`.
     - `crates/rune-core/src/lib.rs`: drenar los valores coalescidos en `poll`.
     - `crates/rune-provider/src/mock.rs`: líneas 425 y 487-489.
     - `crates/rune-state/src/lib.rs`: `typing`, `typing_expires`.
6. **Vistas derivadas baratas** (**M**).
   - **Cambio:**
     - Índice de miembros por `(ServerId, UserId)`.
     - Mapa de autores solo para las filas visibles.
     - Lista de miembros virtualizada.
     - `member_groups`, `server_unread` y `directs_unread` cacheados por revisiones de dominio (`members_revision`, `reads_revision`, `channels_revision`), como `view_revisions.rs`.
     - Opcional: no construir la vista dos veces cuando no hubo entrada.
   - **Archivos:** `crates/rune-state/src/lib.rs`, `crates/rune-ui/src/timeline.rs`, `crates/rune-ui/src/view.rs`, `apps/desktop/src/main.rs`.
7. **SQLite endurecido y borradores persistentes** (**S/M**).
   - **Cambio:**
     - Rechazar un esquema más nuevo y migrar en una transacción.
     - `max_page_count`, `secure_delete`, `auto_vacuum=INCREMENTAL`; directorio 0700.
     - `settings` y la tabla nueva `drafts` con clave de cuenta.
     - Poda periódica, no solo al arrancar.
     - `sync_channel` acotado en el *worker*.
     - Leer los ajustes antes de podar.
     - `forget_account`.
   - **Archivos:** `crates/rune-storage/src/lib.rs`, `crates/rune-core/src/storage.rs`, `crates/rune-ui/src/lib.rs` (`settings_to_keep`, borradores), `apps/desktop/src/main.rs` (`restore`).
8. **Llamada como estado explícito** (**M**).
   - **Cambio:**
     - `Call { channel, phase: CallPhase {Joining, Connected, Reconnecting{attempt}, Failed(reason)}, request, self_mute, self_deaf }` con `Failed` definitivo.
     - `JoinVoice` lleva el silencio.
     - Ensordecer conserva el silencio previo.
     - "Hablando" va en un conjunto de último valor.
     - El error del micrófono se ve en la UI.
   - **Archivos:**
     - `crates/rune-model/src/lib.rs`
     - `crates/rune-provider/src/lib.rs`: `JoinVoice`, `Event::CallPhase`, `Event::Speaking`.
     - `crates/rune-provider/src/mock.rs`
     - `crates/rune-state/src/lib.rs`
     - `crates/rune-media/src/lib.rs`: `failed: AtomicBool`.
     - `crates/rune-ui/src/lib.rs`, `crates/rune-ui/src/view.rs`.

## Ideas para limn

Son propuestas de la comparación original para capacidades genéricas de UI
que Serein construye sobre egui. La actualización inicial distingue lo que
ya tiene el commit de limn consumido por RUNE. Las firmas siguientes son
propuestas, no código existente.

1. **Imágenes en línea dentro del texto.**
   - **Situación:** `TextSpan` solo admite un `Icon` (`limn:crates/limn/src/element.rs:509-527`). Serein lo resuelve con el "glifo hueco" (`crates/ui/src/emoji.rs:110-121`; `crates/ui/src/markdown.rs:2139-2213`).
   - **API posible:**
     ```rust
     TextSpan::image(image: ImageRef, size_em: f32, alt: impl Into<String>)
     ```
     - Se alinea a la línea base y cuenta como un carácter para el ajuste de línea.
     - Copiar devuelve `alt` (el emoji o `:nombre:`).
     - `Markdown` lo usa para emoji y emoji personalizados.
2. **Emoji en color en limn-egui.**
   - **Cambio:**
     - Segmentación por grafemas al maquetar.
     - Búsqueda en un índice normalizado (quitar U+FE0F y respetar U+FE0E, como `crates/ui/src/emoji.rs:123-142`).
     - Pintado desde un atlas; *fallback* a fuente si no está.
   - **API posible:**
     ```rust
     limn_egui::install_emoji(EmojiAtlas { png: &'static [u8], index: &'static str, cell: u32 })
     ```
     o un `trait EmojiProvider { fn lookup(&self, cluster: &str) -> Option<AtlasCell>; }`, para que el host elija el arte y cargue con su licencia.
3. **Imagen por referencia y almacén del host.**
   - **Situación:** hoy `RasterImage` lleva los bytes en base64 + SHA-256 dentro de cada elemento (`limn:crates/limn/src/raster_image.rs:6-14`). Sirve para imágenes de confianza local, no para avatares ni adjuntos de red.
   - **API posible:**
     - `ImageRef(u64 | Arc<str>)`.
     - `UiTree::set_image(key, ImageData::{Loading{placeholder}, Ready(bytes|rgba), Failed})`.
     - Que `Avatar::image` y `Element::image` acepten un `ImageRef`.
   - Mantener la verificación de formato y límites de `decode_raster_thumbnail` (`limn:crates/limn-egui/src/lib.rs:213-257`).
4. **Política de caché y decodificación.**
   - **Situación:** sustituiría el límite por número (`limn:crates/limn-egui/src/lib.rs:164-176`) y el `thread::spawn` por imagen (`:300-309`).
   - **API posible:**
     ```rust
     limn_egui::set_image_policy(ImagePolicy {
         decode_threads: 4,
         pools: &[(Pool::Emoji, 1024, 16 << 20), (Pool::Media, 384, 96 << 20)],
         idle: Duration::from_secs(60),
         uploads_per_frame: 32,
     })
     ```
     Con escalera de tamaños: decodificar al tamaño físico pedido, no a 384 px fijos.
5. **Imágenes animadas.**
   - **API posible:** `Element::animated_image(ImageRef)`.
   - **Comportamiento:**
     - Una sola textura que se reescribe.
     - Como mucho una subida cada 16 ms.
     - Solo con la ventana enfocada; respeta `reduced_motion`.
     - `animation_repaint_after` incluye el próximo cuadro.
6. **Mejoras de `measured_list`.**
   - **Situación original:** una sola estimación por lista y alturas solo por clave, sin podar (`limn:crates/limn/src/tree.rs:1259-1279`, `:7940-7943`). **Actualización:** el commit `0da2317` ya poda alturas de claves ausentes, como se detalla al inicio; no se necesita volver a implementar esa parte.
   - **API posible:**
     - `.estimate(|index| f32)` para estimar cada fila.
     - `.row_revision(|index| u64)`: la altura guardada vale mientras no cambie la revisión.
     - Podar `heights` a las claves presentes.
     - `overscan(px)` opcional.
7. **Subárboles memorizados por revisión.**
   - **Motivo:** con un árbol declarativo que se reconstruye en cada cuadro, el equivalente a las revisiones de Serein (`crates/client-core/src/view_revisions.rs:15-73`) sería algo como:
     ```rust
     Element::memo(id, revision: u64, || build())
     ```
   - **Comportamiento:** limn reutiliza el elemento, y su medida, mientras `revision` no cambie. Serviría para el riel, la lista de canales y la lista de miembros.
8. **Extensiones de Markdown** (ya listadas en [LIMN_INTEGRATION.md](LIMN_INTEGRATION.md)): menciones con acción, *spoilers* que se revelan con un clic, marcas de tiempo relativas con su propio `repaint_after`, y emoji personalizados en línea mediante la idea 1.
   - **API posible:**
     ```rust
     Markdown::inline_rule(|token| -> Option<TextSpan>)
     ```
     Una regla en línea acotada que el host registra.

## Licencias

| Elemento | Licencia | ¿Reutilizable en RUNE/limn? | Requisito |
|---|---|---|---|
| Código propio de Serein | MIT OR Apache-2.0 (`Cargo.toml:10`; `LICENSE-MIT`: "Copyright (c) 2026 Serein contributors") | Sí | Conservar el aviso MIT, o bajo Apache-2.0 incluir la licencia y marcar los cambios. Si se copia algo, `THIRD_PARTY_NOTICES.md` en RUNE con el commit `d1af883c`. **Recomendación: reimplementar.** Nada de la lista *Adoptar* necesita copiar código. |
| Atlas y vectores de Twemoji (`assets/twemoji/atlas.png`, `vectors.bin`) | **CC BY 4.0** (`THIRD_PARTY_NOTICES.md:146`; `assets/twemoji/README.md:1-21`) | Sí, con condiciones | **Marcado.** Atribución visible ("Twemoji © Twitter, Inc. and other contributors, CC BY 4.0", con enlace), copia de la licencia en los paquetes e indicar los cambios (Serein los redimensionó). Mejor generar el atlas propio desde upstream (jdecked/twemoji). |
| `assets/twemoji/names.tsv` | Unicode License v3 (`assets/twemoji/NAMES.md:13`; `THIRD_PARTY_NOTICES.md:150-153`) | Sí | Aviso de Unicode. Más limpio regenerarlo desde `emoji-test.txt`. |
| `assets/twemoji/discord-shortcodes.tsv` | Derivado del mapa de emoji del cliente oficial de Discord, obtenido a través de un sitio de terceros (`assets/twemoji/NAMES.md:22-31`) | **No** | Origen propietario o de licencia incierta. Usar nombres CLDR/Unicode o shortcodes propios. |
| Emoji de `googlefonts/noto-emoji` (alternativa) | Imágenes Apache-2.0 y fuentes OFL-1.1 (inferencia: comprobar el `LICENSE` del repositorio antes de usarlo) | Probablemente sí | Encajaría mejor con MIT/Apache que CC BY. |
| Fuentes Inter y Noto de Serein | SIL OFL 1.1 (`THIRD_PARTY_NOTICES.md:138`) | Sí, como fuentes | **Marcado** (OFL): incluir la licencia; no venderlas sueltas; un subconjunto modificado no puede usar el nombre reservado. |
| `sonora`, `nnnoiseless` | BSD-3-Clause (`THIRD_PARTY_NOTICES.md:158-164`, `:172-174`) | Sí, como dependencias | Aviso de copyright en los binarios. |
| `rtrb`, `opus2`; libopus | MIT/Apache; BSD (`THIRD_PARTY_NOTICES.md:130-132`) | Sí | Avisos en los binarios. |
| `vendor/hpke-rs`, `symphonia` | **MPL-2.0** ([SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md)) | Solo como dependencia sin modificar | **Marcado.** No copiar archivos. |
| Sonidos de notificación | Propietarios de Discord Inc. ([SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md)) | **No** | — |

**Contradicción en Serein.** `THIRD_PARTY_NOTICES.md:126` dice "No … emoji collection are redistributed", pero `:146` y `assets/twemoji/` incluyen las 4009 imágenes de Twemoji. Si RUNE adopta Twemoji, su aviso debe decirlo sin ambigüedad.

## No adoptar (mecanismos no oficiales)

Solo se nombran; no se describe cómo reproducirlos.

Lista del 2026-10-14. Después el dueño autorizó parte de ella, siempre
reimplementado y sin falsificar la identidad del cliente
([AGENTS.md](../AGENTS.md)): la sesión en el llavero del sistema y varias
cuentas, las rutas no documentadas que usa Serein, el hCaptcha que resuelve
la persona en cualquier escritura (nunca automatizado), los componentes y
comandos de aplicaciones, y el socket `discord-ipc` y la detección de juegos
apagados por defecto. Lo que sigue prohibido está en
[serein-parity.md](../tasks/serein-parity.md#lo-que-rune-no-replica).

- Importar credenciales de otros navegadores/clientes, pedirlas o pegar tokens ([SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md), puntos 1–3). La excepción autorizada el 2026-10-14 permite una **WebView temporal, aislada y propia para login experimental**; la credencial queda en memoria y, desde el 2026-10-16, también en el llavero del sistema, una entrada por cuenta (nunca en SQLite, archivos ni logs; [SECURITY.md](SECURITY.md)). No autoriza importar sesiones ajenas. El puente Linux `session-login` ya existe: [rune-login](../crates/rune-login/src/lib.rs) espera que termine el helper antes de entregar la sesión y el [proveedor](../crates/rune-discord-session/src/provider.rs) la verifica por HTTP antes de Ready/SignedIn/Gateway. El selector visible de proveedor y cuenta existe desde el 2026-10-17 (Ajustes → Conexión y «Cambiar de cuenta»).
- **Falsificación de la huella del cliente**: User-Agent de navegador, `X-Super-Properties`, *build number*, `properties` del Identify (`crates/discord-api/src/lib.rs:151-187`; `crates/discord-gateway/src/lib.rs:1275-1279`). También el **nonce con forma de snowflake** "como el del cliente oficial" (`crates/client-core/src/fingerprint.rs:118-126`).
- **Flujos de captcha** (reenvío de cabeceras de captcha, webview con UA falso).
- **Opcodes y endpoints exclusivos de cuentas de usuario o no documentados**: suscripciones perezosas a la lista de miembros, sincronización de llamadas, relaciones, `settings-proto`, `call/ring`, voz y vídeo con cuenta de usuario.
- **Imitar el IPC de Rich Presence** del cliente oficial y detectar juegos enumerando procesos.
- **Activos propietarios o de origen dudoso**: sonidos de Discord y `discord-shortcodes.tsv`.
- **Decisiones de Serein que no conviene copiar**, aunque no sean "no oficiales":
  - borrar todo el historial en disco de la cuenta en cada `Resync` (`apps/desktop/src/main.rs:5924-5928`);
  - borrar la caché de imágenes al cambiar de cuenta, contra su propio comentario (`main.rs:2433-2434`, `:2459-2462`);
  - mezclar recortando, sin limitador (`crates/discord-voice/src/mixer.rs:147-150`);
  - documentación que contradice al código.
