# Investigación: patrones de UX y arquitectura de rendimiento de Discord (referencia para RUNE)

**Última verificación: 2026-10-14**

**Revisión de continuidad (2026-10-14):** se verificó la superficie oficial de
OAuth2 y Social SDK y sus límites de historial, presencia y voz. Los detalles
y las fuentes están en [COMPATIBILITY.md](COMPATIBILITY.md). Esta revisión no
revalida todas las observaciones históricas de UX que siguen ni constituye una
prueba de integración con Discord.

RUNE es un cliente de chat nativo en Rust con **identidad visual propia**. Este documento recoge los patrones de usabilidad de Discord que se pueden observar y la arquitectura de rendimiento que Discord describe en público. Sirve como **referencia de comportamiento**, no como especificación visual. Discord no se copia: se toman patrones de interacción, y la apariencia (colores, tipografías, iconos, sonidos, formas) es original de RUNE.

## Método y reglas de este documento

- **Solo fuentes públicas:** artículos de support.discord.com (Help Center), documentación oficial de la API (docs.discord.com, antes discord.com/developers/docs; la URL antigua redirige con 301), blog de ingeniería de discord.com, una charla pública (GitNation 2021), repositorios open source publicados por Discord y prensa o blogs de terceros (marcados como tales). **No se decompiló, inspeccionó ni depuró el cliente de Discord ni se leyeron datos de usuarios.**
- Las páginas HTML del Help Center devuelven un desafío de Cloudflare a los clientes automáticos. Su contenido se leyó mediante la API pública del Help Center (`https://support.discord.com/api/v2/help_center/en-us/articles/<id>.json`), que entrega el mismo texto del artículo. En el documento se citan las URLs públicas de los artículos.
- **Dimensiones:** no se midió ni se reporta ninguna dimensión en píxeles de Discord. Los umbrales numéricos que aparecen en las propuestas son **"valor de diseño RUNE, no medido en Discord"**, salvo que se cite una fuente.
- **Niveles de confianza:**
  - **alta**: lo dice de forma explícita una fuente oficial (Help Center, documentación de la API o blog de ingeniería).
  - **media**: fuente oficial indirecta, varias fuentes de terceros que coinciden, o una fuente oficial antigua que puede haber cambiado.
  - **baja**: un reporte comunitario aislado, o una observación común que no figura en ninguna documentación.
- "Componentes de limn" se nombran de forma genérica. Otro ingeniero los mapeará a la API real de limn.
- **Disponibilidad para RUNE:** una función observable en el cliente oficial
  no implica que exista una API autorizada para implementarla. Las propuestas
  de este documento son ideas de dominio/UI; las capacidades concretas las
  declara cada proveedor y las evalúa `Store::abilities`. La investigación de
  bots no define el producto: RUNE sigue independiente de proveedores y no
  incorpora un administrador de bots.

---

## Resumen ejecutivo: reglas que RUNE debería implementar primero

| Regla | Valor / comportamiento | Confianza | Fuente |
|---|---|---|---|
| Página de historial | `limit` por defecto **50**, rango 1–100. Anclas `before` / `after` / `around` mutuamente excluyentes. Respuesta de más nuevo a más viejo | alta | [Get Channel Messages](https://docs.discord.com/developers/resources/message) |
| Agrupación por autor | Mismo autor y ventana temporal. Discord no publica el valor oficial. Un reporte comunitario de 2023 dice 8 min (antes medidos desde el último mensaje, ahora desde el primero del grupo) y que los grupos se cortan a medianoche. **RUNE: 7 min desde el primer mensaje del grupo, configurable** | baja | [post comunitario](https://support.discord.com/hc/en-us/community/posts/12799818806551-Undo-the-recent-change-to-message-grouping) |
| Enviar / nueva línea | Enter envía; Shift+Enter inserta un salto de línea | alta / media | [Text Channels](https://support.discord.com/hc/en-us/articles/4412085582359), [Known Issues Chat Box](https://support.discord.com/hc/en-us/articles/360037842511) |
| Editar el último mensaje | Flecha arriba con el composer vacío (terceros). La guía oficial actual dice Shift+↑ (Win) / ⌥+↑ (Mac). **RUNE: ambos** | media | [Guía de atajos](https://support.discord.com/hc/en-us/articles/31232432266647), [HowToGeek](https://www.howtogeek.com/853659/10-discord-features-you-should-be-using/) |
| Escape | Cancela edición/respuesta o marca el canal como leído. En navegación de mensajes, devuelve el foco al composer | alta | [Guía de atajos](https://support.discord.com/hc/en-us/articles/31232432266647), [Keyboard Navigation FAQ](https://support.discord.com/hc/en-us/articles/1500000056121) |
| Quick switcher | Ctrl/⌘+K. Prefijos `*` servidores, `@` DMs, `#` texto, `!` voz. Enter con la búsqueda vacía vuelve al canal anterior | alta | [Quick Switcher](https://support.discord.com/hc/en-us/articles/115000070311) |
| Navegación | Alt+↑/↓ canales; Alt+Shift+↑/↓ canales no leídos; Ctrl+Alt+↑/↓ servidores; F6 / Shift+F6 saltan entre secciones | alta | [Windows Hotkeys](https://support.discord.com/hc/en-us/articles/225977308), [Keyboard Navigation FAQ](https://support.discord.com/hc/en-us/articles/1500000056121) |
| Anclaje de scroll | Discord no lo documenta. **RUNE:** modo "seguir al final" solo si el usuario ya está abajo; al anteponer historial, se conserva el desplazamiento del primer mensaje visible; los cambios de altura por encima del ancla se compensan; se reserva el alto de los adjuntos con `width`/`height` | diseño RUNE (patrones observables: alta) | ver P4 |
| Movimiento reducido | Por defecto "sincronizar con el sistema". Los saltos a mensajes pasan a ser instantáneos y los menús hacen fade | alta | [Reduced Motion](https://support.discord.com/hc/en-us/articles/360040613412) |

---

# Parte 1: UI/UX

## 1.1 Barra de servidores (rail)

#### F-01. Rail de servidores y botón Home (DMs)
1. **Funcionalidad investigada:** columna vertical de servidores, con acceso a mensajes directos.
2. **Comportamiento observado:** la columna izquierda lista los servidores. Arriba está el botón **Home**, que lleva a los DMs, listados bajo "Direct Messages" a la derecha del rail. Se puede reordenar arrastrando, también con teclado (Ctrl/⌘+D levanta el ítem, las flechas lo mueven, Enter/Espacio lo suelta y Esc cancela). La navegación entre servidores es Ctrl+Alt+↑/↓ (guía antigua de Windows) o Ctrl+Alt+→/← (Win) y ⌘+⌥+↑/↓ (Mac) según la guía nueva. **Foco:** el rail es una lista. Se recorre con flechas y no forma parte del orden de Tab ("Because these things can have hundreds of entries at a time, they often aren't included in the Tab order"). Para lectores de pantalla, el rail se expone como *tree view* (problema conocido con NVDA 2020.1). **Scroll:** no está documentado. En móvil, el rail se virtualizó y solo se mantienen en memoria los servidores visibles.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360041721052-Video-Calls (botón Home → DMs), https://support.discord.com/hc/en-us/articles/4408877527703-Accessible-Drag-and-Drop-FAQ, https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://support.discord.com/hc/en-us/articles/360043781772--Known-Issue-Expanding-Server-List-with-Screen-Readers, https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide, https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `enum RailItem { Home, Guild(GuildId), Folder(FolderId) }` y `struct Rail { items: Vec<RailItem>, selected: RailSel }`, donde el orden lo define el usuario y se persiste en la configuración. La navegación con teclado usa un índice de foco *roving* (`focused: usize`) que se mueve con flechas. El reordenamiento por teclado es una máquina de estados `enum DragKbd { Idle, Lifted { from: usize, to: usize } }`. Cada movimiento anuncia la posición por la capa de accesibilidad (región *live*). Los iconos se cargan bajo demanda, solo para los ítems visibles (lista virtualizada), y se liberan al salir de la vista.
6. **Componentes de limn necesarios:** lista virtualizada (vertical), tooltip (nombre del servidor), menú contextual, drag & drop con soporte de teclado, anillo de foco, badge.
7. **Dependencias y limitaciones:** requiere un árbol de accesibilidad (p. ej. AccessKit, si limn lo integra) para exponer el rol tree/list. La forma visual (íconos redondos, "píldora") **no se copia**: RUNE define su propio indicador.

#### F-02. Carpetas de servidores
1. **Funcionalidad investigada:** agrupar servidores en carpetas.
2. **Comportamiento observado:** se crea una carpeta arrastrando un servidor encima de otro y soltándolo "unos segundos" después. El clic derecho sobre la carpeta ofrece Folder Settings (renombrar, color) y "Mark Server Folder as Read". Una carpeta admite hasta **100 servidores**. No se pueden fusionar dos carpetas arrastrando. La carpeta vacía se borra sola. La carpeta muestra un badge agregado cuando alguno de sus servidores tiene notificaciones.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360030853132-Server-Folders-101
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct Folder { id: FolderId, name: Option<String>, color: Option<ThemeColorToken>, guilds: Vec<GuildId>, expanded: bool }` con un límite `const MAX_GUILDS_PER_FOLDER: usize = 100`. El badge agregado es `fn folder_badge(f) -> Badge { Badge::sum(f.guilds.iter().map(guild_badge)) }` (suma de menciones, OR de "no leído"). Al soltar sobre otra carpeta no se fusiona: se rechaza o se inserta como hermano. Si `guilds.is_empty()`, la carpeta se elimina.
6. **Componentes de limn necesarios:** lista virtualizada con ítems expandibles, drag & drop (destino "soltar dentro" frente a "soltar entre"), modal (ajustes de carpeta), selector de color, menú contextual, badge.
7. **Dependencias y limitaciones:** Discord no documenta el tiempo exacto de *hover* para crear la carpeta ("a few seconds"). RUNE debe elegir su propio valor (valor de diseño RUNE).

#### F-03. Indicadores de no leído y menciones
1. **Funcionalidad investigada:** señalización de actividad no leída en servidores, canales y DMs.
2. **Comportamiento observado:** los servidores y canales con mensajes no leídos muestran "unread dots" (puntos). Las menciones muestran indicadores rojos con el **número** de menciones. Silenciar un canal o servidor oculta los puntos de no leído, pero **las menciones directas (@usuario, @rol, @everyone, @here) siguen apareciendo** con su contador rojo. En el chat de un canal de voz, las menciones usan un número rojo y los no leídos un número gris. Para el reconocimiento de lectura existe un "Read State" por usuario y canal, con contadores de menciones.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/209791877-How-do-I-mute-and-disable-notifications-for-specific-channels, https://support.discord.com/hc/en-us/articles/4412085582359-Text-Channels-Text-Chat-In-Voice-Channels, https://discord.com/blog/why-discord-is-switching-from-go-to-rust
4. **Nivel de confianza:** alta (comportamiento); baja para la forma visual exacta (no documentada)
5. **Implementación propuesta en Rust:** `struct ReadState { last_read: Option<MessageId>, mention_count: u32 }` por canal. `fn unread(ch) = ch.last_message_id > rs.last_read` compara snowflakes, que son ordenables. `enum Badge { None, Unread, Mentions(u32) }` se calcula con `if muted { mentions-only } else { unread | mentions }`. Los agregados por servidor y carpeta se derivan de forma perezosa y se cachean con un `version: u64` que se invalida cuando cambia un `ReadState` o `last_message_id`.
6. **Componentes de limn necesarios:** badge (numérico), indicador de estado (punto o barra), tooltip.
7. **Dependencias y limitaciones:** depende de que el backend exponga `last_message_id` por canal y el estado de lectura. La representación (color, forma) es original de RUNE: en el resto del mundo el rojo significa "mención", pero el tono y la forma son propios.

## 1.2 Sidebar de canales

#### F-04. Categorías y colapso
1. **Funcionalidad investigada:** agrupar canales en categorías colapsables.
2. **Comportamiento observado:** los canales viven bajo categorías. Las categorías se crean con clic derecho en el ícono del servidor o con el chevron del nombre del servidor, y se reordenan arrastrando. Al **colapsar** una categoría, **solo se muestran los canales con mensajes no leídos** y las notificaciones siguen llegando. Si además se **silencia**, se ocultan todos los canales y los no leídos, y un canal solo reaparece ante una mención directa o de rol. El estado colapsado se **sincroniza entre clientes de escritorio** (Collapse Syncing).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/115001580171-Channel-Categories-101, https://support.discord.com/hc/en-us/articles/1500006623222-Collapse-Syncing
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct Category { id, name, position: i32, collapsed: bool, muted: bool }`. Las filas visibles se derivan en una función pura:
   ```rust
   fn visible_channels(cat: &Category, chans: &[ChannelRow]) -> impl Iterator<Item=&ChannelRow> {
       chans.iter().filter(move |c| match (cat.collapsed, cat.muted) {
           (false, _) => true,
           (true, false) => c.unread || c.mentions > 0 || c.selected,
           (true, true) => c.mentions > 0 || c.selected,
       })
   }
   ```
   (mantener visible el canal seleccionado es una decisión de diseño RUNE). `collapsed` se persiste en la configuración del usuario y, si el backend lo permite, se sincroniza.
6. **Componentes de limn necesarios:** lista virtualizada con encabezados colapsables (sección), chevron/disclosure, menú contextual, drag & drop.
7. **Dependencias y limitaciones:** la sincronización entre dispositivos requiere almacenamiento de configuración en el servidor de RUNE.

#### F-05. Tipos de canal (texto, voz, hilos, foro) y DMs
1. **Funcionalidad investigada:** distinción visual y funcional de tipos de canal.
2. **Comportamiento observado:** los canales de texto se marcan con `#` y los de voz con un ícono de altavoz. Un candado indica que el usuario no tiene permiso para unirse a un canal de voz. Los canales de voz tienen chat de texto embebido (botón "Open Chat") y vista previa de hasta 3 mensajes nuevos. Hay canales de foro (solo contienen hilos). Los hilos activos se descubren pasando el puntero sobre el canal en el sidebar o con el botón Threads. Seleccionar un canal lo resalta y muestra su contenido en la vista principal. Tipos de la API: GUILD_TEXT(0), DM(1), GUILD_VOICE(2), GROUP_DM(3), GUILD_CATEGORY(4), hilos (10/11/12) y GUILD_FORUM(15).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/4412085582359-Text-Channels-Text-Chat-In-Voice-Channels, https://support.discord.com/hc/en-us/articles/19583625604887-Voice-Channels-FAQs, https://support.discord.com/hc/en-us/articles/4403205878423-Threads-FAQ, https://docs.discord.com/developers/resources/channel
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `enum ChannelKind { Text, Voice, Category, Forum, Thread { parent: ChannelId, private: bool }, Dm, GroupDm }`. El ícono se resuelve con `fn channel_icon(kind, locked: bool) -> IconId` usando un set de íconos propio de RUNE. Bajo cada canal de voz se muestra la lista de participantes conectados (`Vec<VoiceParticipant>`), alimentada por eventos de estado de voz.
6. **Componentes de limn necesarios:** lista virtualizada, íconos, tooltip/popover ("hover" sobre canal → hilos activos), indicador de bloqueo.
7. **Dependencias y limitaciones:** los íconos `#` y altavoz son convenciones genéricas, pero el dibujo debe ser propio.

## 1.3 Timeline de mensajes

#### F-06. Agrupación de mensajes por autor
1. **Funcionalidad investigada:** reglas para fusionar mensajes consecutivos bajo un solo encabezado (avatar + nombre + hora).
2. **Comportamiento observado:** los mensajes consecutivos del mismo autor se agrupan y solo el primero muestra avatar y hora. **Discord no publica el umbral.** Un post comunitario (2023) afirma que antes el grupo terminaba tras **8 minutos sin mensajes** y que después cambió a "el grupo termina **8 minutos después del primer mensaje**". El mismo post dice que los grupos "se cortan a medianoche" (cambio de día). Otros proyectos citan que Discord tiene un límite temporal sin conocer el valor (Zed eligió 5 min). La cifra "7 minutos", que circula informalmente, **no se pudo verificar en ninguna fuente**. El modo compacto muestra hora en cada línea y quita avatares.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/community/posts/12799818806551-Undo-the-recent-change-to-message-grouping, https://git.secluded.site/zed/commit/7d1cb8b072c6eeeaee253a7158e323401cb529d5, https://support.discord.com/hc/en-us/articles/217047657-How-do-I-switch-to-compact-text-mode
4. **Nivel de confianza:** baja (umbral y regla exacta); alta (existencia del agrupamiento y del modo compacto)
5. **Implementación propuesta en Rust:** la agrupación es una función pura que depende solo del mensaje anterior y del inicio del grupo, así que se recalcula de forma incremental (O(1) por inserción al final y solo la frontera al anteponer).
   ```rust
   /// valor de diseño RUNE (no oficial): ventana medida desde el 1.er mensaje del grupo
   pub const GROUP_WINDOW_MS: i64 = 7 * 60 * 1000;

   pub fn starts_group(prev: Option<&Message>, cur: &Message, group_start_ms: i64,
                       divider_between: bool, tz: &TimeZone) -> bool {
       let Some(p) = prev else { return true };
       divider_between                                   // "nuevos mensajes" o fecha
           || p.author != cur.author
           || p.kind != MessageKind::Default || cur.kind != MessageKind::Default
           || cur.reply_to.is_some()                     // respuesta siempre lleva encabezado
           || local_day(p.ts_ms, tz) != local_day(cur.ts_ms, tz)
           || cur.ts_ms - group_start_ms > GROUP_WINDOW_MS
           || p.webhook_identity != cur.webhook_identity // webhooks con nombre/avatar distinto
   }
   ```
   Fila derivada: `enum Row { DateDivider(NaiveDate), UnreadDivider, Message { id: MessageId, group_start: bool } }`. **Requiere un test** de tabla: mismo autor dentro y fuera de la ventana, cambio de día, respuesta y mensaje de sistema.
6. **Componentes de limn necesarios:** lista virtualizada de alto variable, avatar, etiqueta de tiempo con tooltip (fecha completa al pasar el puntero), separador.
7. **Dependencias y limitaciones:** necesita la zona horaria local (crate `chrono`/`jiff` o equivalente). Que las respuestas y los webhooks siempre corten el grupo es una **decisión de diseño RUNE** inspirada en lo observable (confianza baja como afirmación sobre Discord).

#### F-07. Divisores de fecha
1. **Funcionalidad investigada:** separadores entre días.
2. **Comportamiento observado:** el corte de grupos "a medianoche" (post comunitario) implica que el cambio de día interrumpe el grupo. El divisor de fecha en sí es una observación común, sin documentación oficial.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/community/posts/12799818806551-Undo-the-recent-change-to-message-grouping
4. **Nivel de confianza:** baja (no documentado oficialmente)
5. **Implementación propuesta en Rust:** al derivar `Vec<Row>`, se inserta `Row::DateDivider(day)` cuando `local_day(prev) != local_day(cur)`. La clave estable es `RowKey::Date(day)` para que funcione como ancla de scroll.
6. **Componentes de limn necesarios:** separador con etiqueta, lista virtualizada.
7. **Dependencias y limitaciones:** formato de fecha localizado (i18n).

#### F-08. Divisor de "nuevos mensajes", barra "New/Old messages" y "Jump to present"
1. **Funcionalidad investigada:** marcar dónde empiezan los no leídos y permitir volver al presente.
2. **Comportamiento observado:** existe una **barra de "New/Old messages"** y una acción **"Jump to Messages"**. Con movimiento reducido, "slide animation removed and instead send you directly to the message" (oficial). La barra roja "NEW MESSAGES" persiste hasta marcar el canal como leído; **Esc marca el canal como leído** y, según respuestas comunitarias, salta al último mensaje. **Shift+Esc** marca todo el servidor como leído. **Shift+PageUp** salta al mensaje no leído más antiguo. **PageUp/PageDown** desplazan el chat. **Alt+Enter** sobre un mensaje enfocado lo marca como no leído.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting, https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys, https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://support.discord.com/hc/en-us/community/posts/360032558971-Desktop-Notifications-and-NEW-MESSAGES-Red-Bar, https://support.discord.com/hc/en-us/community/posts/360032437011-Jump-to-present
4. **Nivel de confianza:** alta (atajos y existencia de la barra); media (Esc salta al presente, según comunidad)
5. **Implementación propuesta en Rust:** al abrir el canal se fija `unread_anchor = read_state.last_read`. Si el canal tiene no leídos, se inserta `Row::UnreadDivider` antes del primer mensaje con `id > unread_anchor` y la vista se posiciona en ese divisor (ver P4). Mientras el divisor no esté visible y haya no leídos por encima, una barra superior muestra "N mensajes nuevos" con las acciones "Ir" y "Marcar leído". Si el usuario está "desanclado" del presente (`segment.reached_latest == false` o lejos del final), una barra inferior ofrece "Volver al presente". `Esc` (con el composer sin estado pendiente) ejecuta `ack(channel, latest)` y `jump_to_present()`. Con `reduced_motion`, todos los saltos son instantáneos.
6. **Componentes de limn necesarios:** barra flotante superior/inferior (banner) dentro de la lista, botón, separador con etiqueta, scroll programático con y sin animación.
7. **Dependencias y limitaciones:** el texto exacto y el color de Discord no se copian. Requiere que el backend acepte el *ack* de lectura.

#### F-09. Respuestas (replies)
1. **Funcionalidad investigada:** responder a un mensaje con contexto.
2. **Comportamiento observado:** el botón Reply aparece al pasar el puntero sobre el mensaje, también en el menú "…" o con clic derecho, y con la tecla **r** en un mensaje enfocado por teclado. Las respuestas **mencionan (ping) al autor por defecto** y el composer tiene un interruptor para desactivarlo. En la API, la respuesta lleva `message_reference` (solo `message_id` es obligatorio) y `referenced_message`; si este es `null`, el original fue borrado.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360057382374-Replies-FAQ, https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://docs.discord.com/developers/resources/message
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `Message { reply_to: Option<MessageId>, reply_preview: Option<ReplyPreview> }` con `enum ReplyPreview { Loaded { author, excerpt: Arc<str> }, Deleted, NotLoaded }`. El estado del composer es `ComposerMode::Replying { to: MessageId, ping: bool /* default true */ }`. Al hacer clic en la vista previa se ejecuta `jump_to(message_id)` (fetch `around` si no está cargado) y el mensaje se resalta temporalmente (el resaltado es estático con movimiento reducido).
6. **Componentes de limn necesarios:** barra de acciones al hover, chip/indicador sobre el composer con botón cerrar, toggle, texto truncado de una línea.
7. **Dependencias y limitaciones:** la vista previa de un mensaje no cargado requiere fetch (o que el backend la incluya, como `referenced_message`).

#### F-10. Hilos (threads) y foros
1. **Funcionalidad investigada:** subconversaciones temporales.
2. **Comportamiento observado:** se crean desde "…" > Create Thread sobre un mensaje, con título opcional. Se cierran automáticamente tras inactividad: 3 días por defecto, con opciones de 1 h, 24 h y 7 días (la API admite 60/1440/4320/10080 min). Al escribir en un hilo cerrado, se reabre. Los hilos privados invitan por @mención. Notificaciones por hilo: todo / menciones / nada. Buscar en un canal incluye sus hilos. Los hilos activos aparecen en el sidebar al pasar el puntero sobre el canal ("See All").
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/4403205878423-Threads-FAQ, https://docs.discord.com/developers/resources/channel, https://support.discord.com/hc/en-us/articles/6208479917079-Forum-Channels-FAQ
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** un hilo es un canal (`ChannelKind::Thread { parent }`) con `ThreadMeta { archived: bool, auto_archive_min: u32, locked: bool, member: bool }`. La vista de hilo reutiliza el mismo `ChannelTimeline` y la misma lista virtualizada en un panel lateral (*split view*). El mensaje inicial muestra un "chip" del hilo con su contador.
6. **Componentes de limn necesarios:** panel lateral redimensionable (split), lista virtualizada, popover (lista de hilos), modal (crear/editar hilo), selector.
7. **Dependencias y limitaciones:** el soporte de hilos depende del backend de RUNE. Para la v1 puede omitirse.

#### F-11. Reacciones
1. **Funcionalidad investigada:** reacciones con emoji.
2. **Comportamiento observado:** el botón "Add Reaction" está en la barra de hover. Tras la última reacción aparece otro botón "+" para añadir más. **Shift+clic** en el selector permite elegir varias. El menú contextual tiene "Quick React" con los emojis más usados. Hacer clic sobre una reacción propia la retira. "View Reactions" muestra quién reaccionó con qué. Con teclado, **+** sobre un mensaje enfocado añade una reacción. Las "Super Reactions" se animan y se desactivan con "Play animated emoji". En la API, `MESSAGE_REACTION_ADD` lleva `burst` y `burst_colors`.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/12102061808663-Reactions-and-Super-Reactions-FAQ, https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://docs.discord.com/developers/events/gateway-events
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct Reaction { emoji: EmojiRef, count: u32, me: bool }` en un `SmallVec<[Reaction; 4]>`. Las actualizaciones optimistas usan `pending: Option<ReactionOp>` y se revierten si hay error. Los eventos de reacción solo modifican la fila afectada: `version` por mensaje → re-layout de una fila, no de la lista. Los usados recientemente se guardan en `VecDeque<EmojiRef>` (MRU) para "reacción rápida".
6. **Componentes de limn necesarios:** chips/botones toggle con contador, popover (selector de emoji con búsqueda y lista virtualizada en grilla), tooltip (quién reaccionó), modal (ver reacciones).
7. **Dependencias y limitaciones:** datos de emoji Unicode (p. ej. crate `emojis` o tabla propia). Las imágenes de emoji deben ser de un set con licencia libre (ver Recursos visuales).

#### F-12. Menciones y resaltado
1. **Funcionalidad investigada:** @usuario, @rol, @everyone, @here y resaltado de mensajes que te mencionan.
2. **Comportamiento observado:** los mensajes que te mencionan se resaltan con un fondo amarillo (lo implica la lista de bugs conocidos: "If you mention yourself it won't highlight the message with a yellow background"). Las menciones aparecen en Inbox > Mentions (filtrables por @everyone, @role y todos los servidores; se borran a los 7 días). @here aparece en Unreads, no en Mentions. Se pueden suprimir @everyone/@here y las menciones de rol por servidor.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360037842511--Known-Issues-New-Chat-Text-Box, https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ, https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101
4. **Nivel de confianza:** alta (comportamiento); el color es irrelevante para RUNE
5. **Implementación propuesta en Rust:** se calcula una vez al recibir el mensaje: `mentions_me = msg.mentions.contains(&me) || msg.mention_roles ∩ my_roles ≠ ∅ || (msg.mention_everyone && !suppress_everyone)`. Se guarda en `Message.flags` (bitflags). El resaltado usa el token de tema `mention_highlight_bg` (color propio de RUNE).
6. **Componentes de limn necesarios:** fondo de fila resaltada, "pill" de mención inline (texto enriquecido), popover de perfil al hacer clic en la mención.
7. **Dependencias y limitaciones:** requiere conocer los roles del usuario en el servidor.

#### F-13. Embeds y vistas previas de enlaces
1. **Funcionalidad investigada:** previsualización de enlaces e imágenes enlazadas.
2. **Comportamiento observado:** al compartir un enlace, un bot del servidor visita la página y construye una tarjeta (título, descripción, imagen). Las imágenes y videos se sirven **a través de los servidores de Discord** para no exponer la IP del lector y se cachean un tiempo. El usuario puede desactivar las vistas previas (Ajustes > Chat / Text & Images). Para un enlace concreto, se suprime envolviéndolo en `< >`.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/42500550752919-About-Discord-Link-Previews-and-the-Discordbot, https://support.discord.com/hc/en-us/articles/206342858--How-do-I-disable-auto-embed, https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** las tarjetas las genera **el servidor de RUNE** (nunca el cliente, para no filtrar la IP). El cliente recibe `struct Embed { url, title, description, thumbnail: Option<MediaRef { proxy_url, width, height }> }` y reserva el alto con `width/height` antes de cargar (ver P5). El parser marca `<url>` como `suppress_embed`.
6. **Componentes de limn necesarios:** tarjeta (contenedor con borde/acento), imagen asíncrona con placeholder, enlace.
7. **Dependencias y limitaciones:** necesita un servicio de proxy de medios en el backend. Sin él, RUNE debería desactivar las previsualizaciones remotas por defecto por privacidad.

#### F-14. Adjuntos (imágenes, mosaico, spoiler, texto alternativo)
1. **Funcionalidad investigada:** visualización de archivos adjuntos.
2. **Comportamiento observado:** varias imágenes, videos o GIFs en un mensaje se muestran como **mosaico**. Los archivos que no son imagen se muestran como tarjeta de descarga, sin vista previa. Los adjuntos se pueden marcar como spoiler antes de enviar. Se puede añadir **texto alternativo** (ALT), que se muestra con una etiqueta "ALT" si el ajuste está activo. Se pueden adjuntar archivos con "+", arrastrando y soltando, o con Ctrl+Shift+U. En la API: `url`, `proxy_url`, `width`/`height` (solo imagen/video) y URLs firmadas que expiran (`ex`, `is`, `hm`).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ, https://support.discord.com/hc/en-us/articles/360022320632-Spoiler-Tags, https://support.discord.com/hc/en-us/articles/360034632292-Sending-Messages, https://docs.discord.com/developers/resources/message, https://docs.discord.com/developers/reference
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct Attachment { id, filename, size, content_type, url, proxy_url, width: Option<u32>, height: Option<u32>, alt: Option<String>, spoiler: bool }`. El layout del mosaico es una función pura `fn mosaic_layout(n, aspect_ratios, max_w) -> Vec<Rect>` (filas de 1–3, valor de diseño RUNE). Mientras `spoiler && !revealed`, la imagen se dibuja borrosa u oculta y el estado de revelado vive en la vista (se reinicia al salir del canal, como en Discord).
6. **Componentes de limn necesarios:** imagen asíncrona, grilla/mosaico, overlay de spoiler, tarjeta de archivo, visor modal (lightbox), zona de drop, tooltip (ALT).
7. **Dependencias y limitaciones:** decodificación fuera del hilo de UI. Las URLs firmadas caducan, así que hay que refrescar los mensajes o URLs según la política del backend.

#### F-15. Formato Markdown
1. **Funcionalidad investigada:** sintaxis de formato del mensaje.
2. **Comportamiento observado:** `*cursiva*`/`_cursiva_`, `**negrita**`, `__subrayado__`, `~~tachado~~`, combinaciones (`***`, `__**…**__`), `# / ## / ###` encabezados (con espacio), `-# ` subtexto, `[texto](url)` enlaces enmascarados, listas `-`/`*` (sangría de 2 espacios), `` ` `` código en línea, ```` ``` ```` bloque de código, `> ` cita de una línea, `>>> ` cita multilínea y `||spoiler||` (anulado dentro de un bloque de código). En el composer, `>` y `>>>` se comportan igual. Límite de **2000 caracteres** (4000 con suscripción); si se excede, el texto se convierte en archivo.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/210298617-Markdown-Text-101-Chat-Formatting-Bold-Italic-Underline, https://support.discord.com/hc/en-us/articles/360022320632-Spoiler-Tags, https://support.discord.com/hc/en-us/articles/360037842511--Known-Issues-New-Chat-Text-Box, https://support.discord.com/hc/en-us/articles/360034632292-Sending-Messages
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** un **parser propio** del subconjunto (no CommonMark: `__` es subrayado y `||` es spoiler), que produce un AST `enum Inline { Text, Bold(Vec<Inline>), Italic, Underline, Strike, Spoiler, Code(String), Link{text,url}, Mention(..), Emoji(..) }` y `enum Block { Para, Heading(u8), Subtext, Quote, CodeBlock{lang, text}, List }`. **Se parsea una sola vez al recibir el mensaje** y se cachea el AST en `Message`. Discord iOS identificó el parseo de Markdown como el cuello de botella al cargar mensajes y lo mejoró sacando las regex compiladas del bucle. Es necesario un test de tabla con los casos del artículo.
6. **Componentes de limn necesarios:** texto enriquecido (spans con estilos), bloque de código con scroll horizontal, cita con barra lateral, spoiler clicable.
7. **Dependencias y limitaciones:** el resaltado de sintaxis en bloques de código es opcional (p. ej. crate `syntect`, que es pesado; posponerlo).

#### F-16. Barra de acciones al hover, edición, borrado, fijados, reenvío
1. **Funcionalidad investigada:** acciones por mensaje.
2. **Comportamiento observado:** al pasar el puntero aparece una barra en la esquina superior derecha del mensaje con Add Reaction, Reply, Edit (lápiz, solo en mensajes propios), Forward y More (…). **Editar:** Enter guarda, **Esc cancela**. **Fijar:** pide confirmación y genera un mensaje de sistema; la lista de fijados (Ctrl+P) va del más reciente al más antiguo y permite saltar al mensaje. **Reenviar:** hasta 5 destinos, con etiqueta "Forwarded" y sin autor original. **Teclas sobre un mensaje enfocado:** `q` citar, `+` reaccionar, `r` responder, `e` editar, `Backspace` borrar, `p` fijar, Ctrl/⌘+C copiar, Alt+Enter marcar no leído, Esc volver al composer. Con movimiento reducido se desactiva el resaltado al pasar el puntero sobre un mensaje.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/207618817-How-do-I-edit-my-messages, https://support.discord.com/hc/en-us/articles/221421867-Pin-Messages-FAQ, https://support.discord.com/hc/en-us/articles/24640649961367-Message-Forwarding, https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://support.discord.com/hc/en-us/articles/12102061808663-Reactions-and-Super-Reactions-FAQ, https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `enum MessageAction { React, Reply, Edit, Forward, Pin, Unpin, Delete, CopyText, CopyLink, CopyId, MarkUnread, Quote, CreateThread }`. Una sola tabla `fn actions_for(msg, perms, me) -> SmallVec<[MessageAction; 8]>` alimenta **a la vez** la barra de hover, el menú contextual y los atajos de teclado (una fuente de verdad). La edición en línea reemplaza la fila por un campo de texto multilínea (`RowMode::Editing(draft)`). Enter guarda y Esc cancela y devuelve el foco al composer.
6. **Componentes de limn necesarios:** barra flotante (toolbar) anclada a la fila, botón de ícono con tooltip, menú contextual, modal de confirmación, campo de texto multilínea en línea, popover (lista de fijados).
7. **Dependencias y limitaciones:** los permisos (borrar o fijar mensajes ajenos) dependen del modelo de roles del backend.

## 1.4 Composer (campo de mensaje)

#### F-17. Envío, saltos de línea y límite
1. **Funcionalidad investigada:** comportamiento de Enter y multilínea.
2. **Comportamiento observado:** "Enter your message in the text box … and press the Enter key to send". **Shift+Enter** inserta un salto de línea (lo confirma la lista de bugs conocidos: "If you hold shift-enter to make the text box scroll…"). El campo crece y luego hace scroll interno. Hay contador de caracteres. Por encima de 2000 caracteres el contenido se envía como archivo.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/4412085582359-Text-Channels-Text-Chat-In-Voice-Channels, https://support.discord.com/hc/en-us/articles/360037842511--Known-Issues-New-Chat-Text-Box, https://support.discord.com/hc/en-us/articles/360034632292-Sending-Messages
4. **Nivel de confianza:** alta (Enter); media (Shift+Enter, por evidencia indirecta oficial)
5. **Implementación propuesta en Rust:**
   ```rust
   enum ComposerMode { Normal, Replying { to: MessageId, ping: bool }, Editing { id: MessageId } }
   struct Composer { text: String, caret: usize, mode: ComposerMode, popup: Option<Autocomplete>, drafts: HashMap<ChannelId, String> }
   // Orden de prioridad de teclas (diseño RUNE):
   // Enter: popup abierto → aceptar sugerencia; si no → enviar (si !text.trim().is_empty())
   // Shift+Enter → insertar '\n'
   ```
   Los borradores se guardan por canal y se restauran al volver. El límite es `const MAX_LEN: usize = 2000` (valor del backend de RUNE; aquí se cita el de Discord solo como referencia). Al excederlo, RUNE avisa en vez de convertir el texto en archivo (decisión de diseño).
6. **Componentes de limn necesarios:** campo de texto multilínea con altura automática y máximo, contador, botón de adjuntar, botones de emoji/GIF (opcionales).
7. **Dependencias y limitaciones:** el soporte de IME (CJK, composición) depende de limn: Discord tuvo muchos bugs de IME con su editor nuevo. Es crítico probarlo.

#### F-18. Editar el último mensaje y Escape
1. **Funcionalidad investigada:** atajos de edición y cancelación.
2. **Comportamiento observado:** **flecha arriba** en el composer vacío abre el último mensaje propio en modo edición (HowToGeek y un hilo comunitario de Discord). La **guía oficial actual** lista "Edit your last message: Shift + Up Arrow (Win) / ⌥ + Up Arrow (Mac)". **Esc**: "Cancel message or mark channel as read". En edición, Esc cancela. Atajo comunitario: `s/viejo/nuevo` + Enter edita el último mensaje y `+:emoji:` reacciona al último mensaje.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide, https://www.howtogeek.com/853659/10-discord-features-you-should-be-using/, https://support.discord.com/hc/en-us/articles/207618817-How-do-I-edit-my-messages, https://support.discord.com/hc/en-us/articles/360037842511--Known-Issues-New-Chat-Text-Box (menciona `s/`)
4. **Nivel de confianza:** media (hay discrepancia entre ↑ y Shift+↑); alta para Esc
5. **Implementación propuesta en Rust:**
   ```rust
   // ↑ con text vacío y caret en 0, o Shift/Alt+↑ en cualquier caso → editar último propio
   fn last_own_message(tl: &ChannelTimeline, me: UserId) -> Option<MessageId> {
       tl.loaded_rev().find(|m| m.author == me && m.kind == MessageKind::Default).map(|m| m.id)
   }
   // Esc, en orden de prioridad (diseño RUNE):
   // 1) cerrar popup de autocompletado  2) cancelar edición  3) cancelar respuesta
   // 4) si no hay nada pendiente: marcar canal leído + saltar al presente
   ```
   La flecha arriba con texto presente mueve el cursor (comportamiento normal del campo).
6. **Componentes de limn necesarios:** campo de texto multilínea con acceso a cursor/selección, manejo de teclas con prioridad (los eventos de teclado no consumidos burbujean).
7. **Dependencias y limitaciones:** el último mensaje propio puede no estar cargado si el usuario está "desanclado". En ese caso, solo se buscan mensajes en el segmento cargado.

#### F-19. Autocompletado: @menciones, :emoji:, #canales, /comandos
1. **Funcionalidad investigada:** sugerencias al escribir.
2. **Comportamiento observado:** el autocompletado se activa con `/`, `@` y `s/` al inicio (bugs conocidos). `:` + nombre sugiere emojis y Enter completa (HowToGeek). Un blog de terceros dice que hacen falta ≥2 caracteres y que la coincidencia es por prefijo (no verificado). Tab completa (hilo comunitario). **Mention Suggestions** (experimento) sugiere usuarios **sin** `@` y Tab inserta la mención; busca por username, display name, apodo de servidor y apodo de amigo, **solo entre los datos cargados localmente**, con la coincidencia hecha en el dispositivo; se puede desactivar. **Sticker suggestions:** ↑ entra en la ventana de sugerencias, ←/→ eligen, Enter envía, **Esc la cierra hasta que se vacía el campo**. Slash commands: `/` muestra los comandos con autocompletado.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360037842511--Known-Issues-New-Chat-Text-Box, https://www.howtogeek.com/853659/10-discord-features-you-should-be-using/, https://support.discord.com/hc/en-us/community/posts/360041291631-Tabbing-to-complete-emoji-and-entries, https://support.discord.com/hc/en-us/articles/35692242798743-Mention-Suggestions-FAQ, https://support.discord.com/hc/en-us/articles/4403135977495-Sticker-Suggestions-FAQ, https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide
4. **Nivel de confianza:** alta (disparadores `@`, `/`; Mention Suggestions; stickers); media (`:` emoji, Tab); baja (≥2 caracteres, prefijo)
5. **Implementación propuesta en Rust:**
   ```rust
   enum Trigger { User, Channel, Emoji, Command }      // '@' '#' ':' '/'(solo al inicio)
   struct Autocomplete { trigger: Trigger, query_range: Range<usize>, items: Vec<Suggestion>, selected: usize, dismissed_until_clear: bool }
   ```
   La detección busca hacia atrás desde el cursor hasta un espacio o el inicio y comprueba el carácter disparador (para `:`, exigir ≥2 caracteres; valor de diseño RUNE). El ranking: prefijo exacto > prefijo de palabra > subsecuencia, y desempate por recencia o actividad en el canal. Los usuarios salen primero de los miembros cargados y, si hay pocos resultados, se consulta al servidor (similar a `Request Guild Members` con `query`, que devuelve como máximo 100 resultados). ↑/↓ mueven la selección, Enter/Tab aceptan y Esc cierra.
6. **Componentes de limn necesarios:** popover anclado al cursor o al composer con lista (virtualizada si hay más de unas decenas de ítems), avatar e ícono por ítem, scroll a la selección.
7. **Dependencias y limitaciones:** la búsqueda de usuarios en servidores grandes depende de un endpoint del backend. El matcher fuzzy puede ser propio (unas decenas de líneas) antes de añadir un crate.

#### F-20. Indicador de respuesta e indicador de escritura (typing)
1. **Funcionalidad investigada:** "respondiendo a…" y "X está escribiendo…".
2. **Comportamiento observado:** el texto por defecto es "is typing…". Si escribe más de una persona, se vuelve al indicador por defecto (FAQ del indicador personalizado). En la API, `POST /channels/{id}/typing` crea un indicador que "**expires after 10 seconds**". El evento `TYPING_START` incluye `user_id`, `channel_id` y `timestamp` (en segundos). La barra de respuesta aparece sobre el composer con el interruptor de mención (F-09).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/42962943077271-Custom-Typing-Indicator-FAQ, https://docs.discord.com/developers/resources/channel, https://docs.discord.com/developers/events/gateway-events
4. **Nivel de confianza:** alta (expiración de 10 s, texto por defecto); baja (los formatos con 2, 3 o más personas no están documentados)
5. **Implementación propuesta en Rust:**
   ```rust
   const TYPING_TTL: Duration = Duration::from_secs(10);           // según la API
   struct Typing { by_channel: HashMap<ChannelId, SmallVec<[(UserId, Instant); 4]>> }
   // Emisor: mientras hay pulsaciones, enviar typing como máximo cada ~8 s (diseño RUNE, < TTL);
   // al enviar el mensaje, quitar localmente al autor de la lista.
   ```
   Se purga en cada frame o temporizador y se excluye al propio usuario. El texto: "A escribe…", "A y B escriben…", "A, B y C escriben…", "Varias personas escriben…" (diseño RUNE). Con movimiento reducido, la animación de puntos queda estática.
6. **Componentes de limn necesarios:** línea de estado bajo o sobre el composer, animación opcional, chip de respuesta con botón cerrar y toggle.
7. **Dependencias y limitaciones:** tráfico de red: limitar la tasa de envío.

## 1.5 Lista de miembros

#### F-21. Lista de miembros agrupada por rol con presencia
1. **Funcionalidad investigada:** panel derecho de miembros.
2. **Comportamiento observado:** se muestra u oculta con **Ctrl/⌘+U**. Los colores de rol se ven "in member lists" y en los **grupos de rol separados del panel de miembros (cuando están activados)**: en la API es el campo `hoist`, "if this role is pinned in the user listing". El color del nombre sale del **rol más alto**. Los roles con la misma `position` se ordenan por id. La presencia puede ser `online`, `idle`, `dnd` u `offline`, por plataforma (`desktop`, `mobile`, `web`). En servidores grandes, el gateway deja de enviar miembros desconectados por encima de `large_threshold` (50–250). Al hacer clic en un miembro se abre el mini perfil. Discord **solo envía las actualizaciones de la porción visible de la lista** (ver P8).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/214836687-Discord-Roles-and-Permissions, https://docs.discord.com/developers/topics/permissions, https://docs.discord.com/developers/events/gateway-events, https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys, https://support.discord.com/hc/en-us/articles/4409388345495-Per-Server-Profiles, https://discord.com/blog/using-rust-to-scale-elixir-for-11-million-concurrent-users
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** orden total `GroupKey = (Hoisted(role_position desc, role_id) | Online | Offline)`, después nombre visible normalizado y después `UserId`. La vista es `enum MemberRow { Header { group: GroupId, count: u32 }, Member(MemberSummary) }` dentro de una lista virtualizada. Con datos locales, se usa un vector ordenado con búsqueda binaria (o "vector de vectores" en trozos si hay más de ~10⁵ entradas, igual que el SortedSet de Discord). Con datos remotos, se suscribe al rango visible (P8). `struct Presence { status: Status, by_platform: [Option<Status>; 3], activity: Option<Activity> }`.
6. **Componentes de limn necesarios:** lista virtualizada con encabezados de grupo, avatar con indicador de estado, popover de perfil, menú contextual, panel lateral plegable.
7. **Dependencias y limitaciones:** en servidores grandes no se puede tener la lista completa en memoria: se necesita soporte del backend para suscribirse a rangos.

## 1.6 Popouts de usuario / perfiles

#### F-22. Mini perfil (popout) y perfil completo
1. **Funcionalidad investigada:** tarjeta de usuario al hacer clic en avatar o nombre.
2. **Comportamiento observado:** el **clic izquierdo** en un avatar (lista de miembros, canal de texto o canal de voz) abre el "mini profile pop-up" con acciones (p. ej. Edit Profile para uno mismo). El **clic derecho** abre un menú (p. ej. "Edit Per-server Profile"). Los perfiles pueden tener banner, "About Me", tema, decoraciones y perfiles por servidor. Accesibilidad: la opción "Sync profile themes" aplica el tema del cliente a las tarjetas de perfil. Los avatares GIF no se animan con movimiento reducido salvo al abrir el perfil.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/4409388345495-Per-Server-Profiles, https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab, https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct ProfileCard { user: UserId, guild: Option<GuildId>, data: Loadable<Profile> }`. La tarjeta se abre **inmediatamente** con los datos en caché (nombre, avatar, roles) y completa el resto de forma asíncrona (`enum Loadable<T> { Loading, Ready(T), Failed }`). La posición se calcula junto al elemento ancla, con volteo si no cabe (*flip*/*shift*).
6. **Componentes de limn necesarios:** popover posicionado con colisión de bordes, avatar, lista de roles (chips), botón, campo de texto ("enviar mensaje"), trampa de foco y retorno de foco.
7. **Dependencias y limitaciones:** el popover debe atrapar el foco y devolverlo al cerrar (ver F-24).

## 1.7 Menús contextuales

#### F-23. Menús contextuales por entidad
1. **Funcionalidad investigada:** clic derecho en mensajes, canales, servidores, carpetas, usuarios y campo de texto.
2. **Comportamiento observado:**
   - **Mensaje:** Reply, Quick React (emojis frecuentes arriba), Add Reaction, View Reactions, Create Thread, Pin, Forward, Bookmark (experimental), copiar enlace o ID (con el modo desarrollador en Ajustes > Advanced).
   - **Canal:** Notification Settings, Duplicate Channel, Edit Channel.
   - **Servidor (ícono):** Mute Server, crear categoría, ajustes de notificación.
   - **Carpeta:** Folder Settings, Mark Server Folder as Read.
   - **Avatar propio:** Edit Per-server Profile.
   - **Campo de texto:** Suggestions > Mentions (activar o desactivar sugerencias).

   Los menús se navegan con flechas ("context menus" entra en las listas navegables). Con movimiento reducido, los menús, tooltips y popouts **aparecen con un fade**.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/12102061808663-Reactions-and-Super-Reactions-FAQ, https://support.discord.com/hc/en-us/articles/214836687-Discord-Roles-and-Permissions, https://support.discord.com/hc/en-us/articles/209791877-How-do-I-mute-and-disable-notifications-for-specific-channels, https://support.discord.com/hc/en-us/articles/360030853132-Server-Folders-101, https://support.discord.com/hc/en-us/articles/35692242798743-Mention-Suggestions-FAQ, https://support.discord.com/hc/en-us/articles/206346498-Where-can-I-find-my-User-Server-Message-ID, https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct MenuItem { label: String, action: ActionId, enabled: bool, danger: bool, submenu: Option<Vec<MenuItem>>, shortcut_hint: Option<KeyCombo> }`. Los constructores por entidad (`fn message_menu(..)`, `fn channel_menu(..)`) reutilizan las tablas de acciones de F-16. La tecla Menú o Shift+F10 sobre el ítem enfocado abre el menú (accesibilidad, diseño RUNE).
6. **Componentes de limn necesarios:** menú contextual con submenús, separadores, ítems deshabilitados, navegación por flechas y type-ahead.
7. **Dependencias y limitaciones:** el menú contextual del campo de texto (copiar, pegar, corrector) depende de limn y de la plataforma.

## 1.8 Modales y popovers

#### F-24. Modales, popouts y gestión del foco
1. **Funcionalidad investigada:** capas superpuestas (modales, popouts de fijados, menciones/Inbox, ajustes).
2. **Comportamiento observado:** hay popouts para Pins (Ctrl+P), Inbox (Ctrl+I) y emoji (Ctrl+E). Los ajustes se cierran con fade (sin animación con movimiento reducido). Al intentar cerrar una página de ajustes con **cambios sin guardar**, la página tiembla (animación que se desactiva con movimiento reducido). Discord publicó **focus-layers**, una librería que **atrapa el foco** dentro de una capa, mantiene una **pila de capas** (la de arriba es la activa) y **devuelve el foco** al elemento que lo tenía al abrir. También publicó **react-focus-rings** para dibujar anillos de foco que respetan los contenedores con scroll.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys, https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ, https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting, https://github.com/discord/focus-layers, https://discord.com/blog/how-discord-implemented-app-wide-keyboard-navigation
4. **Nivel de confianza:** alta (comportamiento de la librería y atajos); media (que el cliente use exactamente esa librería en todas las capas)
5. **Implementación propuesta en Rust:**
   ```rust
   struct Layer { id: LayerId, kind: LayerKind /* Modal | Popover | Menu | Tooltip */, return_focus: Option<WidgetId>, dismiss_on_outside: bool }
   struct LayerStack(Vec<Layer>);  // el foco solo puede estar en la capa superior (salvo tooltips)
   // push: guarda focused actual en return_focus y enfoca el primer elemento enfocable de la capa
   // pop: restaura return_focus si sigue existiendo; si no, enfoca el composer
   ```
   Esc cierra la capa superior. Un clic fuera cierra los popovers y menús, no los modales con cambios sin guardar. Los modales con estado sucio muestran una barra de "cambios sin guardar" y bloquean el cierre (la animación de aviso se omite con movimiento reducido).
6. **Componentes de limn necesarios:** modal, popover, menú, tooltip, capa de superposición (overlay/scrim), trampa de foco, anillo de foco.
7. **Dependencias y limitaciones:** si limn no tiene una pila de capas con trampa de foco, este es un requisito bloqueante para la accesibilidad.

## 1.9 Quick switcher

#### F-25. Quick switcher (Ctrl/⌘+K)
1. **Funcionalidad investigada:** navegación rápida por nombre.
2. **Comportamiento observado:** Ctrl/⌘+K abre una búsqueda para saltar a canales, DMs o servidores. Prefijos: `*` servidores, `@` DMs/usuarios, `#` canales de texto, `!` canales de voz. Pulsar **Enter** "to return to your previous channel" (con la búsqueda vacía).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/115000070311-Quick-Switcher, https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:**
   ```rust
   enum SwitchTarget { Guild(GuildId), Channel(ChannelId), Dm(ChannelId), Voice(ChannelId) }
   fn parse_query(q: &str) -> (Option<Filter>, &str)  // '*','@','#','!'
   fn rank(q, candidates) -> Vec<(score, SwitchTarget)> // fuzzy + bonus por no leídos/menciones/recencia (diseño RUNE)
   ```
   El historial de navegación es `VecDeque<SwitchTarget>` (también sirve para Alt+← "navigate back"). Al abrir, el foco va al input; ↑/↓ seleccionan, Enter navega y cierra, Esc cierra y **devuelve el foco** al elemento anterior.
6. **Componentes de limn necesarios:** modal centrado (sin scrim pesado), campo de texto de una línea, lista con selección (virtualizada si hay muchos resultados), resaltado de coincidencias.
7. **Dependencias y limitaciones:** el índice de candidatos sale de los datos cargados (servidores, canales y DMs). Los usuarios fuera de los DMs requieren consulta al servidor.

## 1.10 Búsqueda

#### F-26. Búsqueda de mensajes
1. **Funcionalidad investigada:** búsqueda en servidor o DM.
2. **Comportamiento observado:** **Ctrl/⌘+F** busca en el canal actual y **Ctrl/⌘+Shift+F** en todos. El alcance por defecto es el servidor o DM actual. Filtros con atajo de texto: `from:`, `in:`, `mentions:`, `has:` (links, embeds, polls, files…) y "More filters" (rango de fechas, tipo de autor). Los resultados se muestran en un **panel lateral**, ordenables por Newest, Oldest o Most Relevant. Al **seleccionar un resultado se salta** al mensaje en el canal. Búsqueda en un canal = incluye sus hilos. Ingeniería (2017): indexado perezoso (un servidor solo se indexa si alguien busca), pequeño retraso antes de que un mensaje nuevo sea buscable y contexto de **2 mensajes antes y después** de cada coincidencia.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/115000468588-How-to-Use-Search-on-Discord, https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide, https://discord.com/blog/how-discord-indexes-billions-of-messages, https://support.discord.com/hc/en-us/articles/4403205878423-Threads-FAQ
4. **Nivel de confianza:** alta (UI); media (detalles de 2017, pueden haber cambiado)
5. **Implementación propuesta en Rust:** el parser de consulta es `struct SearchQuery { text: String, from: Vec<UserId>, in_: Vec<ChannelId>, mentions: Vec<UserId>, has: BitFlags<Has>, before/after: Option<MessageId> }`. Los rangos de fecha se convierten a snowflakes con `(ts_ms - EPOCH) << 22` (técnica documentada). El panel de resultados es una lista virtualizada con páginas. Al hacer clic se llama a `jump_to(channel, id)` → fetch `around` y se resalta el mensaje. Si el backend no ofrece búsqueda, el respaldo es SQLite FTS5 sobre la caché local (solo mensajes ya vistos).
6. **Componentes de limn necesarios:** campo de búsqueda con chips de filtro y autocompletado de `from:`/`in:`, panel lateral, lista virtualizada, selector de orden.
7. **Dependencias y limitaciones:** la búsqueda completa requiere un índice en el servidor. La búsqueda local es parcial.

## 1.11 Notificaciones e Inbox

#### F-27. Niveles de notificación, silencio e Inbox
1. **Funcionalidad investigada:** configuración de alertas.
2. **Comportamiento observado:**
   - **Por servidor:** All / Mentions / Nothing, Mute Server con duración, suprimir @everyone/@here, suprimir menciones de rol, silenciar eventos nuevos y push móvil.
   - **Por canal (overrides):** All / Mentions / Nothing / Mute. Se acceden con clic derecho en el canal > Notification Settings. Silenciar un servidor no bloquea @everyone.
   - **Inbox (Ctrl/⌘+I):** pestañas Mentions y Unreads. Las menciones se borran a los 7 días. "Mark Inbox As Read" marca todo. **Ctrl/⌘+Shift+E** marca como leído el primer mensaje. El botón "Jump" salta al mensaje.
   - **Notificaciones de reacciones** (experimental): All / DMs / Never, respetando los silencios.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101, https://support.discord.com/hc/en-us/articles/209791877-How-do-I-mute-and-disable-notifications-for-specific-channels, https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ, https://support.discord.com/hc/en-us/articles/12102061808663-Reactions-and-Super-Reactions-FAQ
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:**
   ```rust
   enum NotifyLevel { All, Mentions, Nothing }
   struct NotifySettings { level: NotifyLevel, muted_until: Option<SystemTime>, suppress_everyone: bool, suppress_roles: bool }
   fn should_notify(msg, guild: &NotifySettings, chan: Option<&NotifySettings>, focused: bool) -> bool
   // resolución: override de canal > categoría > servidor; nunca notificar el canal enfocado con ventana activa
   ```
   Las notificaciones de escritorio usan las APIs nativas (en Linux, `org.freedesktop.Notifications` vía crate `notify-rust`). **Requiere un test** de tabla para `should_notify`.
6. **Componentes de limn necesarios:** modal de ajustes por servidor, selector (radio), toggles, popover de Inbox con pestañas y lista virtualizada.
7. **Dependencias y limitaciones:** las notificaciones del sistema dependen del entorno de escritorio. Los sonidos deben ser originales.

## 1.12 Voz y llamadas

#### F-28. UI de voz y video
1. **Funcionalidad investigada:** unirse a voz, panel de estado, controles.
2. **Comportamiento observado:**
   - Un clic en el canal de voz une al usuario, sin timbre. Los demás ven quién está conectado.
   - El **panel de voz** está abajo a la izquierda, con mute y deafen junto al avatar del usuario, botón de video y botón de "Go Live" (compartir pantalla).
   - Atajos: **Ctrl/⌘+Shift+M** mute y **Ctrl/⌘+Shift+D** deafen. Para llamadas entrantes, Ctrl+Enter contesta y Esc rechaza. **Ctrl+[** inicia una llamada en DM.
   - Modos de entrada: Voice Activity (sensibilidad automática o manual, con un **retardo inherente de 200 ms**) y Push to Talk (con *release delay*; en navegador solo con la ventana enfocada).
   - Video en servidor: hasta **25** personas. Vista **Grid** (por defecto) y **Focus** (ocultar o mostrar a los demás). Ventana **Pop Out** (con "Stay On Top") y **Full Screen**. Hasta 50 personas compartiendo video o pantalla.
   - Junto a cada botón del panel, una flecha abre ajustes rápidos (dispositivo y modo de entrada).
   - Arquitectura: SFU (*selective forwarding unit*) en el servidor, señalización por WebSocket de voz que también transporta "speaking indication", y silencio sin enviar audio.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360045138571-Beginner-s-Guide-to-Discord, https://support.discord.com/hc/en-us/articles/33030151293079-Discord-Voice-Video-Streaming-Guide, https://support.discord.com/hc/en-us/articles/211376518-Voice-Input-Modes-101-Push-to-Talk-Voice-Activated, https://support.discord.com/hc/en-us/articles/360041721052-Video-Calls, https://support.discord.com/hc/en-us/articles/360040816151-Go-Live-and-Screen-Share, https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys, https://discord.com/blog/how-discord-handles-two-and-half-million-concurrent-voice-users-using-webrtc
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `enum VoiceConn { Disconnected, Connecting, Connected { channel, rtt_ms }, Reconnecting }`, `struct SelfVoice { muted: bool, deafened: bool /* implica muted */, input: InputMode { Vad { threshold: Option<f32> }, Ptt { key, release_delay_ms } } }`. El audio va en un **hilo dedicado** de tiempo real (crate `cpal`) con Opus (crate `opus`/`audiopus`). El estado "hablando" se publica a la UI como eventos (`Speaking(UserId, bool)`) con *debounce*. Push to Talk global requiere hooks de teclado del sistema (en Wayland, el portal GlobalShortcuts). WebRTC (crates `webrtc` o `str0m`) solo si el backend lo usa.
6. **Componentes de limn necesarios:** panel inferior persistente, botones toggle con estado (mute/deafen), slider (sensibilidad, volumen por usuario), grilla de video, ventana secundaria (pop-out), indicadores de "hablando", menú desplegable de dispositivos.
7. **Dependencias y limitaciones:** es la parte más costosa. Recomendación: **posponerla** hasta tener texto estable. Push to Talk global en Wayland es limitado. Los sonidos de unirse o salir deben ser originales.

## 1.13 Estructura de ajustes

#### F-29. Organización de User Settings
1. **Funcionalidad investigada:** jerarquía de ajustes.
2. **Comportamiento observado:** se abre con Ctrl/⌘+, o con el engranaje junto al avatar (abajo a la izquierda). Secciones citadas en los artículos:
   - Cuenta y perfiles.
   - **App Settings:** Appearance (tema Light/Ash/Dark/Onyx/Sync, UI Density Compact/Default/Spacious, Chat Message Display Default/Compact, Zoom, Chat Font Scaling, Message Group Spacing), **Accessibility**, **Voice & Video**, **Chat / Text & Images** (vistas previas, alt text, stickers), **Notifications**, **Keybinds** (grabar combinaciones; los atajos se desactivan mientras se editan), **Streamer Mode** y **Advanced** (Developer Mode).
   - **Activity Settings:** Activity Privacy, Registered Games.

   El modal de ajustes ocupa la pantalla, con navegación lateral, y hace fade al abrir y cerrar.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide, https://support.discord.com/hc/en-us/articles/207260127-How-to-Change-Discord-Color-Themes-and-Customize-Appearance-Settings, https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab, https://support.discord.com/hc/en-us/articles/217083547-How-do-I-add-different-Keybinds, https://support.discord.com/hc/en-us/articles/206346498-Where-can-I-find-my-User-Server-Message-ID, https://support.discord.com/hc/en-us/articles/22045487931799-Members-List-Recent-Activity-FAQ, https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting
4. **Nivel de confianza:** alta (secciones citadas); media (orden exacto actual)
5. **Implementación propuesta en Rust:** una sola `struct Settings` serializable (crate `serde` + `toml`/`json`) con subestructuras `Appearance`, `Accessibility`, `VoiceVideo`, `Chat`, `Notifications`, `Keybinds`, `Advanced`. La UI se genera a partir de una tabla `&[SettingsPage { id, title, render: fn(&mut Settings, &mut Ui) }]`. Para "cambios sin guardar" se compara `draft != saved`.
6. **Componentes de limn necesarios:** modal de pantalla completa o vista con navegación lateral, toggles, sliders, radios, selectores, campo de grabación de atajos, barra de "cambios sin guardar".
7. **Dependencias y limitaciones:** solo deben existir los ajustes que RUNE realmente soporta. No hay que replicar la lista de Discord.

## 1.14 Atajos de teclado (lista citada)

#### F-30. Tabla de atajos
1. **Funcionalidad investigada:** atajos oficiales de escritorio y navegador.
2. **Comportamiento observado:** atajos según la guía oficial actual (actualizada el 2026-10-12):

   | Acción | Windows | Mac |
   |---|---|---|
   | Abrir ajustes | Ctrl+, | ⌘+, |
   | Menú de atajos | Ctrl+/ | ⌘+/ |
   | Crear o unirse a servidor | Ctrl+Shift+N | ⌘+Shift+N |
   | Navegar atrás | Alt+← | Alt+← |
   | Buscar en canal / en todo | Ctrl+F / Ctrl+Shift+F | ⌘+F / ⌘+Shift+F |
   | Selector de emoji / GIF | Ctrl+E / Ctrl+G | ⌘+E / ⌘+G |
   | Recorrer historial del canal | Ctrl+↑/↓ | ⌘+↑/↓ |
   | Editar último mensaje | Shift+↑ | ⌥+↑ |
   | Cancelar mensaje o marcar canal leído | Esc | Esc |
   | Moverse entre canales | Alt+↑/↓ | ⌥+↑/↓ |
   | Quick switcher | Ctrl+K | ⌘+K |
   | Mute / Deafen | Ctrl+Shift+M / Ctrl+Shift+D | ⌘+Shift+M / ⌘+Shift+D |
   | Cambiar de servidor | Ctrl+Alt+→/← | ⌘+⌥+↑/↓ |
   | Elementos interactivos / activar | Tab / Enter, Espacio | Tab / Return, Espacio |
   | Fijados / Menciones | Ctrl+P / Ctrl+Alt+@ | ⌘+P / ⌘+⌥+@ |
   | Lista de miembros | Ctrl+U | ⌘+U |
   | Subir archivo / Ayuda | Ctrl+Shift+U / Ctrl+Shift+H | ⌘+⇧+U / ⌘+⇧+H |
   | Negrita / Cursiva / Subrayado | Ctrl+B / Ctrl+I / Ctrl+U | ⌘+B / ⌘+I / ⌘+U |
   | Código en línea | Ctrl+Shift+M | — |

   Atajos adicionales de la tabla antigua de Windows: canales no leídos Alt+Shift+↑/↓, no leídos con menciones Ctrl+Shift+Alt+↑/↓, marcar servidor leído Shift+Esc, volver al canal de voz activo Ctrl+Alt+A, PageUp/PageDown para desplazar, Shift+PageUp para ir al no leído más antiguo, Ctrl+Shift+T para grupo privado, Ctrl+[ para iniciar llamada, Ctrl+Enter o Esc para contestar o rechazar una llamada y Tab para enfocar el área de texto. Inbox: Ctrl/⌘+I abre el Inbox y Ctrl/⌘+Shift+E marca el primer mensaje. Navegación por teclado: F6 / Shift+F6 para cambiar de sección y las teclas por mensaje de F-16. Arrastrar con teclado: Ctrl/⌘+D.

   **Conflictos dentro de la propia documentación de Discord:** Ctrl+U (lista de miembros / subrayado), Ctrl+Shift+M (mute / código en línea), Ctrl+I (Inbox o menciones / cursiva), Ctrl+B ("volver al canal de texto anterior" en la tabla antigua / negrita), y editar el último mensaje con ↑ (terceros) o Shift+↑ (guía oficial).
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide, https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys, https://support.discord.com/hc/en-us/articles/225878307--macOS-Discord-Hotkeys (tabla solo en imagen), https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ, https://support.discord.com/hc/en-us/articles/4408877527703-Accessible-Drag-and-Drop-FAQ
4. **Nivel de confianza:** alta (texto oficial). Los conflictos indican que la documentación mezcla versiones.
5. **Implementación propuesta en Rust:** un registro único `struct Keymap { bindings: Vec<Binding { combo: KeyCombo, scope: Scope, action: ActionId }> }` con `enum Scope { Global, Composer, MessageFocused, List, Modal }`. Se resuelve **del scope más específico al global**, lo que elimina los conflictos: Ctrl+B/I/U formatean solo con el composer enfocado y en cualquier otro lugar ejecutan la acción global. Los atajos de RUNE deben elegirse **sin dobles significados** (p. ej. código en línea con Ctrl+Shift+C en lugar de Ctrl+Shift+M). Hay que mapear Ctrl↔⌘ por plataforma, generar el panel "Ctrl+/" desde el mismo registro y añadir **un test** que detecte combos duplicados en el mismo scope.
6. **Componentes de limn necesarios:** manejo de atajos con scopes (propagación de eventos), modal de ayuda de atajos, campo de grabación de atajos.
7. **Dependencias y limitaciones:** los atajos globales (fuera de la ventana) dependen del sistema operativo y del portal en Wayland.

## 1.15 Accesibilidad

#### F-31. Movimiento reducido
1. **Funcionalidad investigada:** reducción de animaciones.
2. **Comportamiento observado:** las opciones son "Sync with computer" (**por defecto**) y "Enable Reduced Motion". Cuando está activo:
   - La barra New/Old messages y Jump to Messages dejan de deslizarse y saltan directo.
   - Desaparece el temblor de "cambios sin guardar".
   - La animación del banner del servidor al hacer scroll se minimiza.
   - Los ajustes abren y cierran al instante.
   - **Se desactiva el resaltado al pasar el puntero sobre un mensaje.**
   - Los fondos animados del login se desactivan.
   - Los avatares GIF no se reproducen salvo en el perfil.
   - Tooltips, menús y popouts aparecen con fade.
   - El spinner de carga queda estático.
   - Los emojis animados quedan estáticos.

   En móvil, además, se fuerzan a "off" "Automatically play GIFs" y "Play animated emoji".
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `enum ReducedMotion { SyncWithOs, On, Off }` → `fn motion(&self, os: bool) -> bool`. Todas las animaciones pasan por un único `fn animate(spec) -> AnimSpec` que, con movimiento reducido, devuelve `Instant` o `Fade` según una tabla `AnimKind → ReducedBehavior`. Así no hay animaciones "olvidadas". La preferencia del sistema se lee con la API de cada plataforma (en Linux GNOME, la clave `enable-animations` de GSettings o el portal de configuración; la API exacta está por verificar).
6. **Componentes de limn necesarios:** sistema de animación centralizado que se pueda desactivar, consulta de la preferencia del sistema.
7. **Dependencias y limitaciones:** en Linux no hay un estándar único para la preferencia de movimiento reducido: hay que detectar varias fuentes y dejar que el ajuste manual mande.

#### F-32. Saturación, alto contraste y colores de rol
1. **Funcionalidad investigada:** ajustes de color.
2. **Comportamiento observado:**
   - **Saturation slider** de 0 a 100 %. Según la charla de 2021, los colores de marca se guardan en hex, se convierten a HSL y la saturación se escala con una variable de 0 a 1.
   - **High Contrast Mode** para todos los temas. En Windows se puede sincronizar con el alto contraste del sistema.
   - **Role Colors:** mostrar el color en el nombre, junto al nombre, o no mostrarlo.
   - Los colores de rol se ajustan en brillo según el modo (más claros en oscuro, más oscuros en claro) para mantener la legibilidad.
   - "Sync profile themes".
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab, https://gitnation.com/contents/accessibility-at-discord
4. **Nivel de confianza:** alta (ajustes); media (detalles de implementación de 2021)
5. **Implementación propuesta en Rust:** los colores del tema son tokens (`ThemeToken → Oklch`) que se resuelven **una vez** al cambiar el tema: `resolved = base.with_chroma(base.c * saturation)`. Es más barato que filtrar cada frame. Los colores de rol pasan por `fn legible_role_color(c, bg) -> Color`, que ajusta la luminosidad hasta alcanzar el contraste mínimo de WCAG (4.5:1 para texto). El modo de alto contraste es una paleta alternativa completa. `enum RoleColorMode { InName, BesideName, Hidden }`.
6. **Componentes de limn necesarios:** sistema de temas por tokens con recarga en caliente, slider, radios.
7. **Dependencias y limitaciones:** el cálculo de contraste necesita una librería de color (p. ej. crate `palette`) o unas 40 líneas propias. El umbral WCAG es un estándar público, no un dato de Discord.

#### F-33. Escalado de fuente, zoom, densidad y espaciado
1. **Funcionalidad investigada:** tamaño de texto y densidad.
2. **Comportamiento observado:**
   - **Zoom Level** escala toda la UI (Ctrl/⌘ + `+`/`-`).
   - **Chat Font Scaling** escala solo el texto del chat, y el zoom la amplifica.
   - **Message Group Spacing** separa los grupos.
   - **UI Density** (Compact/Default/Spacious) afecta a la navegación, no a los mensajes.
   - **Compact message display** elimina los avatares.
   - En modo compacto, los emojis no se agrandan ("won't wumboji"). Se infiere que en modo normal un mensaje formado solo por emojis los muestra grandes.

   Discord no publica los tamaños base: **no se reportan valores**.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/207260127-How-to-Change-Discord-Color-Themes-and-Customize-Appearance-Settings, https://support.discord.com/hc/en-us/articles/217047657-How-do-I-switch-to-compact-text-mode, https://support.discord.com/hc/en-us/articles/360041139231-How-to-Add-Emojis-on-Discord
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `struct Typography { ui_zoom: f32, chat_scale: f32 }`. El tamaño efectivo del chat es `base_chat * chat_scale * ui_zoom`. `enum Density { Compact, Default, Spacious }` multiplica los paddings de navegación. `group_spacing` es un token. **Un cambio de escala invalida la caché de alturas** de la lista virtualizada (P2/P4), conservando el ancla de scroll.
6. **Componentes de limn necesarios:** escalado global (factor de escala), estilos por tokens, relayout de lista con ancla.
7. **Dependencias y limitaciones:** el texto debe ser de alto variable. No se pueden asumir alturas fijas de fila.

#### F-34. Navegación por teclado, anillo de foco y salto de secciones
1. **Funcionalidad investigada:** uso completo sin ratón.
2. **Comportamiento observado:**
   - **Tab** activa el "Keyboard Mode" y aparece un **anillo de foco**.
   - Tab recorre botones y enlaces. Las listas largas (servidores, canales, mensajes, menús contextuales) **no están en el orden de Tab** y se recorren con flechas.
   - En mensajes: Tab entra, las flechas cambian de mensaje y Tab entra en su contenido.
   - **F6 / Shift+F6** saltan entre lista de servidores → canales → mensajes → miembros (o resultados de búsqueda).
   - El foco persiste al cambiar de canal con el teclado.
   - Los anillos de foco se dibujan en un "scope" que respeta los contenedores con scroll o recorte y adapta su color al fondo.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ, https://discord.com/blog/how-discord-implemented-app-wide-keyboard-navigation, https://gitnation.com/contents/accessibility-at-discord
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `enum InputModality { Pointer, Keyboard }` se actualiza con cada evento: el anillo solo se ve con `Keyboard`, igual que `:focus-visible`. Las listas usan **roving focus**: un único `focused_index` por lista y un único tab-stop. Las secciones son `const SECTIONS: [SectionId; 4] = [Rail, Channels, Messages, Members]` (más Search si está abierta), y F6 recorre las presentes y visibles. Cada lista recuerda su `focused_index` al perder el foco. Al hacer scroll programático hacia el ítem enfocado, se usa "scroll mínimo para hacerlo visible".
6. **Componentes de limn necesarios:** gestión de foco con roving tabindex, anillo de foco dibujado en una capa que respeta el recorte, scroll-into-view, lista virtualizada que **mantiene el ítem enfocado montado** aunque salga de la vista.
7. **Dependencias y limitaciones:** la virtualización puede desmontar el elemento enfocado. La lista debe "fijarlo" (pin) mientras tenga el foco.

#### F-35. Lector de pantalla, arrastrar con teclado y texto alternativo
1. **Funcionalidad investigada:** soporte de tecnologías de asistencia.
2. **Comportamiento observado:** Discord funciona con lectores de pantalla en escritorio y navegador (el artículo está "en actualización"). El arrastrar y soltar accesible anuncia los cambios mediante **live regions**: JAWS necesita "Announce Live Region Updates". El rail se expone como tree view. Las imágenes admiten texto ALT. Hay control de animación de stickers (siempre, al interactuar, nunca) y velocidad de texto a voz.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/7180791233559-Using-a-Screen-Reader-on-Discord, https://support.discord.com/hc/en-us/articles/4408877527703-Accessible-Drag-and-Drop-FAQ, https://support.discord.com/hc/en-us/articles/360043781772--Known-Issue-Expanding-Server-List-with-Screen-Readers, https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ, https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab
4. **Nivel de confianza:** alta (live regions, ALT); baja (cómo se anuncian los mensajes nuevos: no documentado)
5. **Implementación propuesta en Rust:** cada widget expone un rol, un nombre y un estado en un árbol de accesibilidad (AccessKit es el estándar de facto en Rust y se conecta con AT-SPI, UIA y NSAccessibility). Cada mensaje se expone como "artículo/ítem de lista" con la etiqueta "autor, hora, contenido" construida desde el AST, no desde los glifos. Los anuncios se hacen con un `Announcer` (región live *polite*) que llama `announce(text)` para operaciones de arrastre y, opcionalmente, para mensajes nuevos del canal enfocado (con límite de tasa; diseño RUNE). Las imágenes usan `alt` como nombre accesible.
6. **Componentes de limn necesarios:** árbol de accesibilidad, región live, roles de lista o árbol, nombres accesibles.
7. **Dependencias y limitaciones:** **depende por completo de que limn soporte accesibilidad.** Si no lo hace, es un riesgo alto que conviene resolver temprano.

## 1.16 Animaciones y transiciones

#### F-36. Inventario de animaciones
1. **Funcionalidad investigada:** qué se anima.
2. **Comportamiento observado:** según la lista de movimiento reducido, se animan:
   - El deslizamiento de la barra New/Old messages y de "Jump to message".
   - El temblor de cambios sin guardar.
   - El banner del servidor al hacer scroll en la lista de canales.
   - El fade de las páginas de ajustes.
   - El resaltado de mensaje al pasar el puntero.
   - Los fondos del login.
   - Avatares GIF, emojis animados, stickers y Super Reactions.
   - Tooltips, menús y popouts: con movimiento reducido pasan a fade, lo que implica que sin él usan otra transición, probablemente escala o desplazamiento (no documentado).
   - El spinner de carga.

   No se publican duraciones ni curvas.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting, https://support.discord.com/hc/en-us/articles/12102061808663-Reactions-and-Super-Reactions-FAQ
4. **Nivel de confianza:** alta (inventario); duraciones: no disponibles
5. **Implementación propuesta en Rust:** `enum AnimKind { Popover, Menu, Tooltip, Modal, JumpScroll, Banner, Hover, Shake }` con una tabla `AnimKind → (duración, curva, comportamiento con movimiento reducido)` que **define RUNE** (duraciones cortas; valores de diseño RUNE). Las animaciones de imágenes (GIF, WebP animado) solo corren si están visibles, la ventana tiene el foco y el movimiento reducido está desactivado: se pausan fuera de pantalla para ahorrar CPU.
6. **Componentes de limn necesarios:** motor de animación/transiciones, reproducción de imágenes animadas con pausa.
7. **Dependencias y limitaciones:** las animaciones de imágenes cuestan CPU y GPU. Hay que limitar cuántas corren a la vez.

---

# Parte 2: Arquitectura de rendimiento descrita públicamente

#### P1. Timeline con actualizaciones frecuentes (separar datos de render)
1. **Funcionalidad investigada:** cómo se mantiene fluido el chat con muchos eventos.
2. **Comportamiento observado:**
   - **iOS:** la vista de chat es nativa (`UITableView`), "extensively controlled by Javascript". JS calcula las fuentes de datos y Objective-C renderiza con Yoga. Las imágenes de chat reutilizan una caché de imágenes común.
   - **Android:** la lista de chat es "fully native and only uses JavaScript to fetch message data". Hay *pools* de reciclaje para componentes y mosaicos, el pool se **pre-llena** antes de abrir el chat, el inflado de elementos raros (spoilers, overlays de subida) es perezoso y la medición de contenedores y reacciones se aceleró. Resultado: ~12 % menos memoria en la lista de chat y hasta un 60 % menos de frames lentos.
   - **Lecciones (iOS):** las actualizaciones del store de usuarios re-renderizaban un componente raíz (~30 ms por emisión); se movió la suscripción a los hijos. El parseo de Markdown era el cuello de botella al cargar mensajes, y sacar la regex compilada del bucle ahorró ~30 %.
3. **Evidencia disponible:** https://discord.com/blog/why-discord-is-sticking-with-react-native, https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app, https://discord.com/blog/how-discord-achieves-native-ios-performance-with-react-native
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** el **store** (modelo) es la única fuente de verdad, con un contador `version: u64` por canal, por mensaje y por usuario. Las vistas guardan la `version` que renderizaron y solo re-layoutan si cambió (suscripción granular, sin re-render global). Los eventos de red se **aplican en lote una vez por frame** (`drain` con presupuesto de tiempo). Las presencias y los typing se coalescen (gana el último por usuario). El parseo de Markdown y la medición de texto ocurren al recibir el mensaje, no al dibujarlo, y su resultado se cachea en `Message` (AST) y en `HeightCache` (alto por ancho de columna).
6. **Componentes de limn necesarios:** lista virtualizada con invalidación por fila, layout de texto cacheable, bucle de frame con hook "antes de layout".
7. **Dependencias y limitaciones:** el diseño concreto de los widgets depende de limn (modo inmediato o retenido). La regla "invalidar por fila con versión" vale en ambos.

#### P2. Virtualización y reciclaje
1. **Funcionalidad investigada:** renderizar solo lo visible.
2. **Comportamiento observado:**
   - Discord adaptó **su componente `List` virtualizado de la web** para React Native (FastList), con encabezados *sticky* y reciclaje de vistas. Las listas de RN montaban unas 2000 vistas en servidores grandes.
   - En móvil, la **lista de servidores se virtualizó**: "only keeps information about the servers that are visible in memory" (−14 % de memoria al arrancar con más de 100 servidores, −10 % de tiempo de arranque).
   - FastestList (selector de emoji) usa *view portaling* y muestra un placeholder en lugar de dejar un hueco en blanco.
3. **Evidencia disponible:** https://discord.com/blog/how-discord-achieves-native-ios-performance-with-react-native, https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:**
   ```rust
   struct VirtualList<K> {
       keys: Vec<K>,                 // RowKey estable (MessageId / divisor)
       heights: Vec<f32>,            // medido o estimado
       measured: Vec<bool>,
       prefix: Vec<f32>,             // sumas prefijas; recálculo O(n) perezoso (n acotado por la ventana en memoria)
       est: f32,                     // altura estimada por tipo de fila
       overscan: usize,              // filas extra arriba/abajo (diseño RUNE)
       pinned: Option<K>,            // fila con foco: nunca se desmonta (F-34)
   }
   fn visible_range(&self, scroll_top: f32, viewport_h: f32) -> Range<usize> // búsqueda binaria en prefix
   ```
   Las filas fuera de la vista se desmontan. Las "plantillas" de widgets por tipo de fila se reciclan si limn lo permite. Si n crece mucho o el recálculo O(n) aparece en el profiler, se cambia a un árbol de Fenwick (`shortcut: prefix O(n), pasar a Fenwick si n > ~10⁴`). Las filas no medidas se dibujan como placeholder del alto estimado, nunca en blanco.
6. **Componentes de limn necesarios:** lista virtualizada de alto variable (servidores, canales, mensajes, miembros, emoji en grilla, resultados).
7. **Dependencias y limitaciones:** la medición del texto requiere el motor de texto de limn. El ancho de la columna afecta a todas las alturas: al redimensionar, se invalidan y se re-estiman de forma perezosa.

#### P3. Paginación del historial
1. **Funcionalidad investigada:** cargar mensajes antiguos y recientes por páginas.
2. **Comportamiento observado:** `GET /channels/{id}/messages` devuelve los mensajes "from newest to oldest". Parámetros: `around`, `before`, `after` (snowflake, **mutuamente excluyentes**) y `limit` 1–100, **por defecto 50**. Sin `READ_MESSAGE_HISTORY` no se devuelve nada. Los snowflakes son ordenables por tiempo: `timestamp = (id >> 22) + 1420070400000`. Para paginar desde una fecha se genera un snowflake con `(ts_ms - EPOCH) << 22`. En servidor, los mensajes se particionan por canal y "bucket" temporal y las lecturas concurrentes de la misma fila se coalescen.
3. **Evidencia disponible:** https://docs.discord.com/developers/resources/message, https://docs.discord.com/developers/reference, https://discord.com/blog/how-discord-stores-trillions-of-messages
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:**
   ```rust
   const PAGE: u8 = 50;            // igual al default documentado
   enum PageReq { Latest, Before(MessageId), After(MessageId), Around(MessageId) }
   struct Segment { msgs: VecDeque<Arc<Message>>, reached_oldest: bool, reached_latest: bool } // ascendente por id
   struct ChannelTimeline { segments: Vec<Segment>, active: usize, inflight: Option<PageReq>, version: u64 }
   ```
   - Abrir el canal: `Latest`, o `Around(last_read)` si hay muchos no leídos.
   - Acercarse al borde superior: `Before(first.id)`. Si la respuesta trae menos de `PAGE`, `reached_oldest = true`.
   - Saltar a un mensaje: `Around(id)` crea un segmento nuevo. Se **fusionan** segmentos cuando se solapan (por búsqueda binaria en ids).
   - Un solo `inflight` por dirección y canal: se descartan las peticiones duplicadas.
   - La respuesta (de nuevo a viejo) se invierte antes de insertar.
   - Los mensajes en vivo (`MESSAGE_CREATE`) solo se añaden al segmento que tiene `reached_latest`. Si el usuario está en otro segmento, solo se incrementa un contador de "nuevos".

   **Requiere un test** de fusión de segmentos.
6. **Componentes de limn necesarios:** lista virtualizada con callbacks de "cerca del borde", indicador de carga en la parte superior o inferior.
7. **Dependencias y limitaciones:** el adaptador debe declarar qué rangos puede recuperar. Por ejemplo, el Social SDK solo documenta hasta 200 mensajes y 72 horas de historial DM, y exige que ambos participantes hayan jugado el juego ([guía oficial](https://docs.discord.com/developers/discord-social-sdk/development-guides/sending-direct-messages), consultada el 2026-10-14). Una página corta puede significar que se alcanzó ese límite, no el comienzo real de la conversación. La fusión de páginas no debe tratar como borrado lo que está fuera del rango cubierto por la respuesta. `before`/`after`/`around` son capacidades a comprobar en cada proveedor.

#### P4. Anclaje de scroll al anteponer historial
1. **Funcionalidad investigada:** mantener la posición visual al insertar contenido por encima.
2. **Comportamiento observado:** **Discord no documenta su algoritmo.** Lo observable y documentado es: saltos a mensajes (búsqueda, fijados, respuestas, Inbox) con resaltado, la barra "New/Old messages", "Jump to present", Shift+PageUp al no leído más antiguo, Esc al presente y saltos instantáneos con movimiento reducido. En iOS la lista es una `UITableView` con "dynamic cell heights". Las APIs dan `width`/`height` de imágenes y videos, lo que permite reservar el espacio antes de cargarlos.
3. **Evidencia disponible:** https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting, https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys, https://support.discord.com/hc/en-us/articles/115000468588-How-to-Use-Search-on-Discord, https://discord.com/blog/why-discord-is-sticking-with-react-native, https://docs.discord.com/developers/resources/message
4. **Nivel de confianza:** baja como afirmación sobre Discord (no documentado). Las reglas de abajo son **diseño RUNE** basado en técnicas estándar.
5. **Implementación propuesta en Rust:**
   ```rust
   enum Follow { Latest, Anchored(Anchor) }
   struct Anchor { key: RowKey, delta: f32 }   // delta = top_fila - scroll_top (puede ser negativo)

   // 1) Antes de mutar: si follow == Latest y estamos "abajo" → seguir abajo tras la mutación.
   //    Si no: anchor = primera fila cuyo borde inferior > scroll_top.
   // 2) Mutar (prepend/append/cambios de alto).
   // 3) Tras layout: scroll_top = offset_of(index_of(anchor.key)) - anchor.delta.
   ```
   **Reglas propuestas:**
   - (a) **Seguir al final** solo si el usuario estaba al final (umbral pequeño, valor de diseño RUNE) cuando llega el mensaje. Si no, no mover la vista y mostrar "N nuevos" (F-08).
   - (b) **Anteponer historial** nunca mueve el contenido visible: el ancla es la primera fila visible.
   - (c) Los cambios de alto **por encima** del ancla (imagen cargada, embed, edición, reacción) se compensan. Los de **abajo** no hacen falta.
   - (d) Reservar el alto de los medios con `width`/`height` para minimizar los saltos.
   - (e) El mensaje propio enviado siempre lleva al final.
   - (f) Al abrir un canal con no leídos, el ancla es el `UnreadDivider` con un pequeño margen superior. Sin no leídos, `Follow::Latest`.
   - (g) Al saltar a un mensaje, se centra la fila y se resalta. Con movimiento reducido, el salto es instantáneo.
   - (h) Disparar `Before(...)` cuando faltan menos de ~1 viewport de contenido por arriba (valor de diseño RUNE).

   **Requiere tests** de (b) y (c) con alturas sintéticas.
6. **Componentes de limn necesarios:** lista virtualizada con API de scroll programático (set_offset sin animación), callback post-layout, medición de filas.
7. **Dependencias y limitaciones:** si limn no expone un hook post-layout antes de pintar, habrá un frame con salto visible. Es un requisito para la lista.

#### P5. Caché y proxy de imágenes
1. **Funcionalidad investigada:** cómo se sirven y redimensionan los medios.
2. **Comportamiento observado:**
   - Discord pasa las imágenes por **Media Proxy** (Go + C++/Lilliput) porque enlazarlas directamente "would leak users' IP addresses" y gastaría ancho de banda. Hay una capa de caché enrutada por hash de URL que **coalesce peticiones**. El proxy extrae el primer frame de MP4/WEBM para las vistas previas, decodifica los GIF frame a frame (memoria predecible) y rechaza imágenes enormes leyendo la cabecera antes de decodificar.
   - **CDN:** `?size=` acepta "any power of two between 16 and 4096". Los animados se piden como WebP con `?animated=true`.
   - Los adjuntos tienen `proxy_url` (imagen, video y audio; otros tipos devuelven 415) y URLs firmadas con `ex`/`is`/`hm` que el cliente refresca.
   - En móvil, los emojis animados pasaron de GIF a WebP animado vía el proxy (60 fps en gama baja). En iOS, la vista previa se pide a 256×256 para aprovechar la caché de PhotoKit (dato del blog, no de Discord desktop).
3. **Evidencia disponible:** https://discord.com/blog/how-discord-resizes-150-million-images-every-day-with-go-and-c, https://docs.discord.com/developers/reference, https://docs.discord.com/developers/resources/message, https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app, https://support.discord.com/hc/en-us/articles/42500550752919-About-Discord-Link-Previews-and-the-Discordbot
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:**
   ```rust
   #[derive(Hash, Eq, PartialEq, Clone)]
   struct ImageKey { url: Arc<str>, size: u16 }            // size = potencia de 2 en 16..=4096
   fn size_bucket(display_px: f32, scale: f32) -> u16 {
       ((display_px * scale).ceil() as u32).next_power_of_two().clamp(16, 4096) as u16
   }
   struct ImageCache {
       mem: lru::LruCache<ImageKey, Arc<Decoded>>,   // presupuesto en BYTES, no en entradas
       bytes: usize, budget: usize,
       inflight: HashMap<ImageKey, Vec<oneshot::Sender<Arc<Decoded>>>>, // coalescing como el proxy
   }
   ```
   - La descarga va en el runtime de red; la decodificación, en un pool de hilos (`image` u otro decodificador). Al hilo de UI solo llega el bitmap listo, o se sube como textura.
   - Hay caché en disco por `ImageKey` (directorio de caché XDG).
   - Se cancelan las peticiones de filas que salen de la vista antes de terminar.
   - Las imágenes animadas solo se decodifican frame a frame si son visibles (F-36).
   - Siempre se pide el tamaño redimensionado al proxy de RUNE, nunca el original, salvo en el visor a pantalla completa.
6. **Componentes de limn necesarios:** imagen asíncrona con placeholder del tamaño reservado, gestión de texturas o atlas, visor (lightbox).
7. **Dependencias y limitaciones:** requiere un proxy de medios en el backend de RUNE (también por privacidad). Crates posibles: `lru` (o una caché LRU propia), `image`. El presupuesto de memoria en bytes es un valor de diseño RUNE.

#### P6. Control de memoria
1. **Funcionalidad investigada:** límites y estrategias de memoria.
2. **Comportamiento observado:**
   - **Virtualización** de la lista de servidores (−14 % de memoria al arrancar) y de la lista de chat (~−12 %).
   - El cliente de escritorio es Electron con varios procesos (main, renderer, GPU, crash reporter y overlay).
   - Discord pasó a 64 bits en escritorio porque en 32 bits "occasionally still hit the limit".
   - **Diciembre de 2025:** experimento en Windows que **reinicia la app si supera 4 GB**, solo si lleva ≥1 h abierta, con ≥30 min de inactividad y sin llamada, como máximo una vez al día. Según la prensa, el uso típico ronda 1 GB (se describe como solución temporal frente a fugas de memoria).
   - En el servidor, Read States usa una caché LRU con *commit* diferido.
3. **Evidencia disponible:** https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app, https://support.discord.com/hc/en-us/articles/225733808-Why-does-Discord-have-multiple-processes, https://discord.com/blog/how-discord-seamlessly-upgraded-millions-of-users-to-64-bit-architecture, https://www.pcworld.com/article/3005317/discord-admits-windows-11-app-hogs-ram-tries-solving-it-with-auto-restarts.html, https://alternativeto.net/news/2025/12/discord-admits-excessive-ram-usage-on-windows-app-and-tests-auto-restarts-as-temporary-fix, https://discord.com/blog/why-discord-is-switching-from-go-to-rust
4. **Nivel de confianza:** alta (virtualización, procesos); media (reinicio por 4 GB: prensa citando a Discord)
5. **Implementación propuesta en Rust:** memoria **acotada por diseño** en lugar de reinicios:
   - (1) Por canal, ventana máxima de mensajes en memoria (p. ej. N páginas; valor de diseño RUNE). Al alejarse, se recorta el extremo opuesto del segmento y se marca `reached_* = false`.
   - (2) LRU de canales: los inactivos sueltan sus segmentos y conservan solo `ReadState` y `last_message_id`.
   - (3) Caché de imágenes con presupuesto en bytes (P5).
   - (4) `Arc<str>` para contenido y nombres repetidos (internado de usernames).
   - (5) Métrica interna de memoria (contadores por caché) visible en un panel de depuración.
6. **Componentes de limn necesarios:** ninguno específico (es lógica del modelo). Opcionalmente, un panel de depuración.
7. **Dependencias y limitaciones:** Rust no tiene recolector, pero las cachés sin límite se comportan igual que una fuga: todas deben tener presupuesto.

#### P7. Separación entre red y render; protocolo en tiempo real
1. **Funcionalidad investigada:** cómo el tráfico en tiempo real no bloquea la UI.
2. **Comportamiento observado:**
   - En Android, la lista nativa solo usa JS para obtener datos (P1).
   - **Gateway:** compresión zlib o **zstd en streaming**, con un contexto por conexión. Zstd casi duplica la compresión de `MESSAGE_CREATE` y, junto con Passive Sessions v2, reduce casi un 40 % el ancho de banda del cliente.
   - Las **sesiones pasivas** evitan enviar la mayoría de los mensajes de servidores que el usuario no tiene abiertos: alrededor del 90 % de las conexiones usuario-servidor en servidores grandes son pasivas. V2 envía deltas en lugar de snapshots.
   - Heartbeat con *jitter* inicial. Al perder el ACK se reconecta y se hace *Resume*, que reproduce los eventos perdidos en orden.
   - Límite de 120 eventos enviados cada 60 s por conexión (bots).
   - REST con buckets de *rate limit* y 429 con `retry_after`.
3. **Evidencia disponible:** https://discord.com/blog/how-discord-reduced-websocket-traffic-by-40-percent, https://discord.com/blog/maxjourney-pushing-discords-limits-with-a-million-plus-online-users-in-a-single-server, https://docs.discord.com/developers/events/gateway, https://docs.discord.com/developers/topics/rate-limits, https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:**
   ```rust
   // Hilo(s) de red: runtime tokio con gateway (tokio-tungstenite) + REST (reqwest), descompresión zstd/zlib.
   enum NetEvent { Ready(Snapshot), MessageCreate(Message), MessageUpdate(Patch), MessageDelete{..},
                   Reaction{..}, Typing{..}, Presence{..}, MemberListOps{..}, Page{ch, req, msgs}, Conn(ConnState) }
   enum UiCmd { Send{..}, Fetch(ChannelId, PageReq), Ack{..}, SubscribeMembers{guild, range}, FocusChannel(ChannelId) }
   // UI ⇄ red por canales acotados (p. ej. async-channel/crossbeam). La UI drena NetEvent por frame con presupuesto.
   ```
   - El parseo de JSON y Markdown y el pre-cálculo se hacen **en el lado de red**, no en el hilo de UI.
   - `FocusChannel` le dice al backend qué servidor o canal está activo, para que pueda aplicar algo análogo a las sesiones pasivas: no enviar todo lo de servidores no abiertos.
   - El *resume* guarda `session_id` y `seq`, con *backoff* exponencial con jitter.
6. **Componentes de limn necesarios:** integración del bucle de eventos de limn con canales externos (despertar la UI cuando llega un `NetEvent`).
7. **Dependencias y limitaciones:** depende del protocolo autorizado de cada proveedor. `FocusChannel`, `SubscribeMembers` y `MemberListOps` son propuestas de contrato propio, no endpoints ni opcodes públicos de Discord. No se debe deducir su disponibilidad del blog del cliente oficial. RUNE ya separa proveedor, núcleo y UI y despierta el render ante eventos; ver [ARCHITECTURE.md](ARCHITECTURE.md#flujo-de-un-cuadro).

#### P8. Lista de miembros perezosa
1. **Funcionalidad investigada:** cómo se maneja la lista de miembros en servidores grandes.
2. **Comportamiento observado:**
   - "We could just send down the updates for the **visible portion of the Member List**", lo que reduce red, CPU y batería.
   - En el servidor, una estructura ordenada que "can report back **indices** of where things are being added and removed" (SortedSet en Rust: ~0,6–3,7 µs por operación hasta 1 000 000 de elementos).
   - API para bots: `large_threshold` (50–250) deja de enviar miembros desconectados. `Request Guild Members` devuelve *chunks* de **hasta 1000** miembros (`chunk_index`/`chunk_count`), con un máximo de 100 por `query` o por `user_ids`.
   - El protocolo exacto de suscripción a rangos que usa el cliente oficial **no está documentado públicamente**.
3. **Evidencia disponible:** https://discord.com/blog/using-rust-to-scale-elixir-for-11-million-concurrent-users, https://github.com/discord/sorted_set_nif, https://docs.discord.com/developers/events/gateway-events
4. **Nivel de confianza:** alta (principio); baja (protocolo cliente, no documentado y no se debe copiar)
5. **Implementación propuesta en Rust:** un protocolo **propio de RUNE** inspirado en el principio:
   ```rust
   enum MemberListOp { Sync { range: Range<u32>, rows: Vec<MemberRow> }, Insert { index: u32, row: MemberRow },
                       Update { index: u32, row: MemberRow }, Delete { index: u32 }, Invalidate { range: Range<u32> } }
   struct MemberListView { total: u32, rows: Vec<Option<MemberRow>>, subscribed: Vec<Range<u32>> }
   ```
   - El cliente se suscribe al rango visible más un margen (*overscan*) al hacer scroll, con *debounce*.
   - Las filas fuera del rango quedan como `None` y se dibujan como placeholder.
   - En el servidor (si es de RUNE), "vector de vectores" ordenado o `BTreeMap` con contadores para obtener índices.
   - **Requiere un test** de aplicación de ops (inserciones y borradas que desplazan índices).
6. **Componentes de limn necesarios:** lista virtualizada con alto total conocido y filas placeholder, encabezados de grupo.
7. **Dependencias y limitaciones:** requiere soporte del backend. En servidores pequeños se puede enviar la lista completa y ordenar localmente.

#### P9. Estados de lectura (Read States)
1. **Funcionalidad investigada:** seguimiento de leído y no leído.
2. **Comportamiento observado:** hay "One Read State per User per Channel", con contadores atómicos (p. ej. menciones) que se reinician a 0. Se accede en cada conexión, cada mensaje enviado y cada mensaje leído (ruta caliente). En el servidor hay una caché LRU con *commit* a la base de datos a los 30 s de una actualización.
3. **Evidencia disponible:** https://discord.com/blog/why-discord-is-switching-from-go-to-rust
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `HashMap<ChannelId, ReadState>` en el cliente. El *ack* se envía con *debounce* (agrupar varias lecturas seguidas; valor de diseño RUNE). Se considera "leído" cuando el final del canal ha sido visible con la ventana enfocada durante un breve intervalo, o con Esc. Al recibir `MessageCreate` de otro usuario: `if !channel_focused_and_at_bottom { mark_unread; if mentions_me { mention_count += 1 } }`.
6. **Componentes de limn necesarios:** ninguno específico (modelo). Se usan los badges de F-03.
7. **Dependencias y limitaciones:** el backend debe persistir y sincronizar los read states entre dispositivos.

#### P10. Snowflakes como cursor y reloj
1. **Funcionalidad investigada:** identificadores ordenables.
2. **Comportamiento observado:** 64 bits: 42 de marca temporal (ms desde 2015-01-01, `1420070400000`), 5 de worker, 5 de proceso y 12 de incremento. Se usan como cursores de paginación y para consultas de rango por fecha (búsqueda).
3. **Evidencia disponible:** https://docs.discord.com/developers/reference, https://discord.com/blog/how-discord-indexes-billions-of-messages
4. **Nivel de confianza:** alta
5. **Implementación propuesta en Rust:** `#[derive(Copy, Ord, Hash)] struct MessageId(u64)` con `fn timestamp_ms(self) -> i64` según la época **del backend de RUNE** (si usa un esquema similar). Comparar ids sustituye a comparar fechas en orden, *unread* y fusión de segmentos.
6. **Componentes de limn necesarios:** ninguno.
7. **Dependencias y limitaciones:** solo vale si el backend de RUNE emite ids monotónicos por tiempo. Si no, hay que usar `(timestamp, id)` como clave.

---

## Recursos visuales (propiedad de Discord: NO usar)

| Recurso | Estado | Qué hace RUNE |
|---|---|---|
| Logo de Discord (incluido el "Clyde"/controlador) y nombre "Discord" | Marca registrada. Las directrices de marca prohíben editar, recolorear o incorporar las marcas en nombres de productos | Logo y nombre propios. No mencionar "Discord" en la UI ni en el empaquetado como si hubiera afiliación |
| Color **Blurple** (`#5865F2`, según discord.com/branding) y paleta de marca | Elemento de identidad de marca | Paleta propia de RUNE (tokens). Evitar azul-violeta saturado como acento principal para no generar confusión |
| Tipografías **Whitney** (Colophon Foundry, licenciada por Discord hasta 2022) y **gg sans** (propietaria de Discord desde diciembre de 2022) | No son de uso libre | Fuentes con licencia abierta (OFL), p. ej. Inter, IBM Plex Sans o Atkinson Hyperlegible (esta orientada a legibilidad). Verificar la licencia antes de empaquetar |
| Iconos de la app (canales, altavoz, ajustes, etc.) | Diseños de Discord | Set de íconos propio o con licencia permisiva (verificar licencia; p. ej. sets MIT/ISC) dibujado con estilo coherente |
| Sonidos (notificación, unirse o salir de voz, llamada) | Propiedad de Discord | Sonidos originales o CC0, verificados |
| Mascotas e ilustraciones (Wumpus, Clyde, fondos de login, spinner) | Propiedad de Discord | Ilustraciones propias o ninguna |
| Nombres de producto ("Nitro", "Boost", "Super Reactions", "Go Live") | Marcas o nombres comerciales | Terminología genérica en español ("compartir pantalla", "reacción", "servidor") |

Regla general: RUNE toma **patrones de interacción** (que son convenciones de la industria), nunca la *trade dress* (combinación distintiva de colores, formas y disposición). Hay que evitar replicar la composición exacta de tres columnas con los mismos colores y formas: proporciones, radios, iconografía y paleta deben ser reconocibles como RUNE.

Evidencia: https://discord.com/branding, https://primagames.com/tips/did-discord-change-its-font-answered, https://piunikaweb.com/2022/12/02/discord-gg-sans-font-announcement-fans-asking-for-proper-reveal/ (la historia de gg sans y Whitney viene de terceros: confianza media).

---

## Revisión acotada: búsqueda y multimedia nativa (2026-10-14)

Esta revisión contrasta documentación primaria actual con el código de RUNE.
No es una observación de una cuenta de Discord ni un resultado de REA sobre
su cliente. **Dimensiones, curvas de animación y tiempos de respuesta de
Discord: no medidos.** Las propuestas siguientes son decisiones para RUNE;
su alcance no supone una integración de red. Complementan F-26 y F-28.

Se inspeccionaron también los fotogramas finales de tres GIF públicos
incluidos en la guía oficial de búsqueda:
[consulta](https://support.discord.com/hc/article_attachments/36138588966807),
[ordenación](https://support.discord.com/hc/article_attachments/36138588976151)
y [salto](https://support.discord.com/hc/article_attachments/36138548306455).
Sus lienzos miden respectivamente 800×561, 800×749 y 800×807 píxeles; son
dimensiones de material publicado, **no medidas del layout del cliente**.
Se observan el campo de búsqueda con filtros visibles, contador de
resultados, tarjetas a la derecha con autor, fecha y contexto de canal,
coincidencias resaltadas y acción «Jump». El timeline y el composer siguen
visibles junto a los resultados. Confianza alta sobre esos fotogramas,
sin verificar la versión actual, foco, scroll ni latencia mediante ellos.
La duración de un GIF no mide rendimiento. Los archivos se conservaron
solo como evidencia temporal en `/tmp/rune-public-search-zxew7epu`; no son
recursos de RUNE ni se incorporaron al repositorio.

La primera implementación de RUNE usa un diálogo propio con texto literal,
alcance local y orden reciente. No implementa aún los filtros, panel
lateral persistente ni resaltado del material de referencia.

#### R-01. Alcance de búsqueda y filtros explícitos

1. **Funcionalidad investigada:** buscar mensajes sin perder el contexto de navegación.
2. **Comportamiento documentado:** el índice consultado corresponde al servidor o DM actual. La guía de atajos distingue Ctrl/⌘+F (canal) y Ctrl/⌘+Shift+F (todos los canales); no demuestra búsqueda conjunta entre servidores. La búsqueda entre DMs en escritorio se describe como experimento limitado. Hay filtros por autor, canal, menciones, contenido y fecha; `from:`, `in:`, `mentions:` y `has:` son abreviaturas documentadas.
3. **Evidencia disponible:** [Search, actualizado 2025-12-12](https://support.discord.com/hc/en-us/articles/115000468588-How-to-Use-Search-on-Discord) y [guía de atajos, actualizada 2025-04-14](https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide). En RUNE, `Store::timeline` ya permite consultar mensajes por conversación; el encabezado antes de esta iteración abre el selector de canales.
4. **Nivel de confianza:** alto para lo documentado; disponibilidad exacta del experimento en una cuenta concreta, no verificada.
5. **Propuesta Rust:** comenzar con búsqueda local de texto sobre los mensajes cargados del canal y mostrar ese alcance. Modelar explícitamente conversación/servidor al ampliar el alcance. Aplicar visibilidad desde `Store::abilities`, sin recorrer conversaciones ocultas. La consulta y selección pertenecen a `Ui`; las coincidencias se recalculan cuando cambian consulta o mensajes. Una futura búsqueda SQLite debe ejecutarse en el trabajador existente, con cuenta y conversación como filtros, y descartar resultados de una consulta anterior.
6. **Componentes limn:** `SearchField`, `Modal` o panel compuesto con `Element`, estados vacíos y lista virtualizada existentes. No hace falta otra biblioteca UI ni un parser de protocolo.
7. **Dependencias y límites:** primera entrega sin dependencias nuevas y sin solicitud de red. “Sin coincidencias” solo describe el conjunto cargado, no el historial completo. FTS5, filtros avanzados y búsqueda remota permanecen ampliaciones; no se adopta el endpoint de bots para este producto.

#### R-02. Resultados, salto y foco

1. **Funcionalidad investigada:** pasar de un resultado al mensaje original con teclado.
2. **Comportamiento documentado:** Discord presenta resultados laterales ordenables por recientes, antiguos o relevancia; elegir uno navega al mensaje. Su guía de accesibilidad describe Tab para controles, flechas para listas, Enter/Espacio para activar y F6/Shift+F6 entre secciones; con búsqueda abierta, los resultados sustituyen la sección de miembros en ese recorrido. Escape desde un mensaje enfocado vuelve al composer. La guía no especifica a qué control retorna el foco al cerrar búsqueda.
3. **Evidencia disponible:** [resultados y salto](https://support.discord.com/hc/en-us/articles/115000468588-How-to-Use-Search-on-Discord), [Keyboard Navigation FAQ](https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ) y `rune-ui/src/channel_list.rs` (`jump` ya usa `Intent::ScrollToIndex` y carga páginas antiguas para fijados).
4. **Nivel de confianza:** alto como comportamiento documentado; la guía de navegación es de 2020 y no se probó el cliente actual.
5. **Propuesta Rust:** IDs de resultados derivados de conversación y mensaje, selección por teclado y salto mediante el intent existente. Al activar, resolver de nuevo el ID contra el timeline: una edición, borrado o carga puede cambiar su índice. Cerrar y devolver foco al composer es una decisión de RUNE. Conservar la consulta mientras se inspecciona un resultado evita repetir trabajo. Un resultado eliminado debe producir un estado explicativo, no saltar a otro mensaje por índice obsoleto.
6. **Componentes limn:** `UiTree` para foco, acciones de teclado, lista virtualizada y `scroll_to_index` del host ya utilizado por RUNE. El índice se calcula al activar; la identidad permanece en el ID.
7. **Dependencias y límites:** salto a mensajes ya cargados puede probarse sin servidor. El proveedor futuro necesitará búsqueda e historial alrededor de un ID para resultados fuera de memoria; no hay que recorrer indefinidamente páginas antiguas. Destacar texto coincidente en Markdown y un recorrido F6 completo son ampliaciones separadas, no capacidades verificadas de la búsqueda inicial.

#### R-03. De medidor local a audio procesado

1. **Funcionalidad investigada:** captura, procesamiento y reproducción sin bloquear renderizado.
2. **Comportamiento documentado:** CPAL entrega audio mediante callbacks y expone configuraciones de dispositivos. PipeWire exige evitar operaciones bloqueantes en el callback de tiempo real y devolver buffers al stream. Opus conserva estado por flujo; a 48 kHz, un frame de 20 ms contiene 960 muestras por canal. Esto describe las bibliotecas, no demuestra el pipeline interno de Discord.
3. **Evidencia disponible:** [CPAL 0.18.2](https://docs.rs/cpal/0.18.2/cpal/), [PipeWire Streams](https://docs.pipewire.org/page_streams.html), [Opus Encoder](https://opus-codec.org/docs/opus_api-1.5/group__opus__encoder.html), `rune-media/src/lib.rs` y [evaluación local existente](MEDIA.md).
4. **Nivel de confianza:** alto para los contratos públicos y el código leído; latencia de una cadena completa de RUNE, no medida.
5. **Propuesta Rust:** conservar el medidor atómico existente. Para una futura prueba de loopback, copiar muestras a un buffer circular acotado y procesar fuera del callback; agrupar por frames, remuestrear solo si la configuración lo exige y registrar underruns/overruns. Medir tiempo de codec y muestras pendientes antes de elegir tamaño de buffer. El formato del dispositivo y el del transporte deben ser contratos distintos.
6. **Componentes limn:** selector de dispositivos y `DictationWaveform` ya usados; estados de permiso, dispositivo ausente y fallo como props. La UI recibe niveles/estadísticas, nunca buffers PCM.
7. **Dependencias y límites:** hoy RUNE mide entrada y actividad; no tiene reproducción de llamadas ni codec. Opus y un buffer circular se justificarán al implementar y probar loopback; no se agregan por esta investigación. Una prueba local no acredita una llamada Discord. Las opciones y restricciones de dependencias continúan en [MEDIA.md](MEDIA.md).

#### R-04. Pantalla y cámara en Linux/Wayland

1. **Funcionalidad investigada:** seleccionar una fuente local y detener la captura limpiamente.
2. **Comportamiento documentado:** ScreenCast define `CreateSession` → `SelectSources` → `Start` → `OpenPipeWireRemote`; el inicio presenta normalmente el selector del sistema y devuelve streams. La sesión puede cerrarse desde el portal. En v6 se recomienda `pipewire-serial` para conectar al objeto porque el node ID puede reutilizarse. Esto es una interfaz de escritorio pública, no una API de compartir pantalla de Discord.
3. **Evidencia disponible:** [XDG ScreenCast](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html), [XDG Camera](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Camera.html) y [MEDIA.md §2.5](MEDIA.md#25-xdg-desktop-portal-ashpd), que registra una comprobación previa del portal local v5.
4. **Nivel de confianza:** alto para la especificación; versiones y permisos del escritorio de esta ejecución, no comprobados de nuevo.
5. **Propuesta Rust:** sesión local con estados solicitando/activa/finalizada/fallida, propietario del descriptor y tarea cancelable. Consultar versión y capacidades antes de elegir opciones; cerrar al recibir `Session::Closed`. En v5 conservar compatibilidad por node ID dentro de la sesión, sin guardarlo como identidad persistente. Para cámara, usar su portal específico. Una previsualización local debe entregar solo el frame más reciente a la UI, con presupuesto de memoria y liberación de recursos al cerrar.
6. **Componentes limn:** botones y estados existentes; antes de añadir video, verificar una superficie de textura actualizable y su ciclo de vida. Si falta, implementarla en limn mediante worktree, pruebas y PR, como exige [LIMN_INTEGRATION.md](LIMN_INTEGRATION.md).
7. **Dependencias y límites:** `ashpd` y PipeWire directo son candidatos ya evaluados; la combinación de features debe evitar dos versiones con el mismo `links`. El selector del portal sigue siendo la autoridad sobre fuentes. No hay captura ni preview implementadas por esta revisión, ni se presupone transporte hacia Discord.

#### R-05. Transporte multimedia independiente del proveedor

1. **Funcionalidad investigada:** elegir dónde viven red, codec y multimedia.
2. **Comportamiento documentado:** `str0m` es WebRTC sans-I/O: la aplicación conduce tiempo y sockets, y su API de frames usa medios codificados. El descubrimiento de interfaces y la obtención de sockets TURN quedan fuera. GStreamer `webrtcbin` ofrece otra composición basada en RTP y negociación; no es un framework de interfaz gráfica.
3. **Evidencia disponible:** [str0m, descripción y bucle de ejecución](https://github.com/algesten/str0m), [webrtcbin](https://gstreamer.freedesktop.org/documentation/webrtc/index.html), [MEDIA.md](MEDIA.md) y [límites de integración Discord](COMPATIBILITY.md).
4. **Nivel de confianza:** alto para responsabilidades declaradas por cada proyecto; interoperabilidad con un proveedor futuro de RUNE, no probada.
5. **Propuesta Rust:** mantener captura/reproducción en `rune-media`; señalización y capacidades pertenecen al proveedor. Un adaptador de transporte recibe frames y publica eventos de sesión, sin que `rune-ui` conozca SDP/RTP. Elegir una implementación solo con una prueba entre pares controlados y un proveedor compatible; simular tiempo y pérdida en pruebas de la máquina de estados.
6. **Componentes limn:** panel de participantes y estados de llamada existentes; video requerirá la superficie citada en R-04. El motor WebRTC no entra en el árbol UI.
7. **Dependencias y límites:** ninguna biblioteca WebRTC concede acceso por sí misma a llamadas, grupos o pantalla de Discord. No se añade `str0m`, GStreamer ni un adaptador de bots en esta iteración. La licencia y el grafo de la versión elegida deberán auditarse al adoptarla; las evaluaciones previas de [MEDIA.md](MEDIA.md) no reemplazan esa validación.

## Lagunas y elementos no verificados

- **Umbral de agrupación:** no hay valor oficial. El "8 min" sale de un post comunitario y el "7 min" no se pudo verificar.
- **Algoritmo de anclaje de scroll** del cliente de escritorio: no hay fuente pública. P4 es diseño RUNE.
- **Protocolo de suscripción a la lista de miembros** del cliente oficial: no está documentado (solo el principio de "porción visible").
- **Duraciones y curvas de animación, tamaños base de fuente y dimensiones de layout:** no publicados, así que **no se reportan**.
- **Editar el último mensaje:** la guía oficial (Shift+↑ / ⌥+↑) y las fuentes de terceros (↑ con el composer vacío) no coinciden.
- **Disparador `:` de emoji con ≥2 caracteres** y coincidencia por prefijo: solo blogs de terceros.
- **Formato del texto "X y Y están escribiendo…"** con varias personas: no documentado.
- El artículo "How Discord Indexes Trillions of Messages" y las notas de parche de octubre de 2024 se consultaron solo a través de resúmenes de búsqueda.
- El post sobre iOS (react-native) está en Medium y en discord.com/blog. Se leyó la versión de discord.com.

---

## Fuentes

**Help Center de Discord (support.discord.com)**
- https://support.discord.com/hc/en-us/articles/31232432266647-Discord-Commands-Shortcuts-and-Navigation-Guide
- https://support.discord.com/hc/en-us/articles/1500000056121-Keyboard-Navigation-FAQ
- https://support.discord.com/hc/en-us/articles/225977308--Windows-Discord-Hotkeys
- https://support.discord.com/hc/en-us/articles/225878307--macOS-Discord-Hotkeys
- https://support.discord.com/hc/en-us/articles/115000070311-Quick-Switcher
- https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab
- https://support.discord.com/hc/en-us/articles/360040613412-Reduced-Motion-Setting
- https://support.discord.com/hc/en-us/articles/7180791233559-Using-a-Screen-Reader-on-Discord
- https://support.discord.com/hc/en-us/articles/4408877527703-Accessible-Drag-and-Drop-FAQ
- https://support.discord.com/hc/en-us/articles/360043781772--Known-Issue-Expanding-Server-List-with-Screen-Readers
- https://support.discord.com/hc/en-us/articles/207260127-How-to-Change-Discord-Color-Themes-and-Customize-Appearance-Settings
- https://support.discord.com/hc/en-us/articles/217047657-How-do-I-switch-to-compact-text-mode
- https://support.discord.com/hc/en-us/articles/210298617-Markdown-Text-101-Chat-Formatting-Bold-Italic-Underline
- https://support.discord.com/hc/en-us/articles/360022320632-Spoiler-Tags
- https://support.discord.com/hc/en-us/articles/4403205878423-Threads-FAQ
- https://support.discord.com/hc/en-us/articles/6208479917079-Forum-Channels-FAQ
- https://support.discord.com/hc/en-us/articles/360057382374-Replies-FAQ
- https://support.discord.com/hc/en-us/articles/12102061808663-Reactions-and-Super-Reactions-FAQ
- https://support.discord.com/hc/en-us/articles/360030853132-Server-Folders-101
- https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101
- https://support.discord.com/hc/en-us/articles/209791877-How-do-I-mute-and-disable-notifications-for-specific-channels
- https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ
- https://support.discord.com/hc/en-us/articles/115001580171-Channel-Categories-101
- https://support.discord.com/hc/en-us/articles/1500006623222-Collapse-Syncing
- https://support.discord.com/hc/en-us/articles/115000468588-How-to-Use-Search-on-Discord
- https://support.discord.com/hc/en-us/articles/35692242798743-Mention-Suggestions-FAQ
- https://support.discord.com/hc/en-us/articles/4403135977495-Sticker-Suggestions-FAQ
- https://support.discord.com/hc/en-us/articles/360037842511--Known-Issues-New-Chat-Text-Box
- https://support.discord.com/hc/en-us/articles/4412085582359-Text-Channels-Text-Chat-In-Voice-Channels
- https://support.discord.com/hc/en-us/articles/19583625604887-Voice-Channels-FAQs
- https://support.discord.com/hc/en-us/articles/211376518-Voice-Input-Modes-101-Push-to-Talk-Voice-Activated
- https://support.discord.com/hc/en-us/articles/360041721052-Video-Calls
- https://support.discord.com/hc/en-us/articles/360040816151-Go-Live-and-Screen-Share
- https://support.discord.com/hc/en-us/articles/33030151293079-Discord-Voice-Video-Streaming-Guide
- https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ
- https://support.discord.com/hc/en-us/articles/206342858--How-do-I-disable-auto-embed
- https://support.discord.com/hc/en-us/articles/42500550752919-About-Discord-Link-Previews-and-the-Discordbot
- https://support.discord.com/hc/en-us/articles/214836687-Discord-Roles-and-Permissions
- https://support.discord.com/hc/en-us/articles/4409388345495-Per-Server-Profiles
- https://support.discord.com/hc/en-us/articles/42962943077271-Custom-Typing-Indicator-FAQ
- https://support.discord.com/hc/en-us/articles/221421867-Pin-Messages-FAQ
- https://support.discord.com/hc/en-us/articles/207618817-How-do-I-edit-my-messages
- https://support.discord.com/hc/en-us/articles/360034632292-Sending-Messages
- https://support.discord.com/hc/en-us/articles/24640649961367-Message-Forwarding
- https://support.discord.com/hc/en-us/articles/360041139231-How-to-Add-Emojis-on-Discord
- https://support.discord.com/hc/en-us/articles/217083547-How-do-I-add-different-Keybinds
- https://support.discord.com/hc/en-us/articles/206346498-Where-can-I-find-my-User-Server-Message-ID
- https://support.discord.com/hc/en-us/articles/225733808-Why-does-Discord-have-multiple-processes
- https://support.discord.com/hc/en-us/articles/360045138571-Beginner-s-Guide-to-Discord
- https://support.discord.com/hc/en-us/articles/22045487931799-Members-List-Recent-Activity-FAQ

**Foro comunitario de Discord (confianza baja)**
- https://support.discord.com/hc/en-us/community/posts/12799818806551-Undo-the-recent-change-to-message-grouping
- https://support.discord.com/hc/en-us/community/posts/360032437011-Jump-to-present
- https://support.discord.com/hc/en-us/community/posts/360032558971-Desktop-Notifications-and-NEW-MESSAGES-Red-Bar
- https://support.discord.com/hc/en-us/community/posts/360041291631-Tabbing-to-complete-emoji-and-entries

**Documentación oficial de la API (docs.discord.com)**
- https://docs.discord.com/developers/resources/message (Get Channel Messages, objeto Attachment, replies)
- https://docs.discord.com/developers/reference (Snowflakes, Image Formatting, Signed Attachment CDN URLs, paginación)
- https://docs.discord.com/developers/events/gateway (compresión, heartbeat, resume, rate limit)
- https://docs.discord.com/developers/events/gateway-events (large_threshold, Request Guild Members, chunks, Typing Start, Presence)
- https://docs.discord.com/developers/resources/channel (tipos de canal, typing 10 s, auto-archivado)
- https://docs.discord.com/developers/topics/permissions (role hoist, position)
- https://docs.discord.com/developers/topics/rate-limits

**Blog de ingeniería de Discord (discord.com/blog)**
- https://discord.com/blog/how-discord-stores-trillions-of-messages
- https://discord.com/blog/how-discord-resizes-150-million-images-every-day-with-go-and-c
- https://discord.com/blog/using-rust-to-scale-elixir-for-11-million-concurrent-users
- https://discord.com/blog/maxjourney-pushing-discords-limits-with-a-million-plus-online-users-in-a-single-server
- https://discord.com/blog/how-discord-reduced-websocket-traffic-by-40-percent
- https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app
- https://discord.com/blog/how-discord-achieves-native-ios-performance-with-react-native
- https://discord.com/blog/why-discord-is-sticking-with-react-native
- https://discord.com/blog/why-discord-is-switching-from-go-to-rust
- https://discord.com/blog/how-discord-indexes-billions-of-messages
- https://discord.com/blog/how-discord-indexes-trillions-of-messages (solo resumen)
- https://discord.com/blog/how-discord-handles-two-and-half-million-concurrent-voice-users-using-webrtc
- https://discord.com/blog/how-discord-implemented-app-wide-keyboard-navigation
- https://discord.com/blog/how-discord-seamlessly-upgraded-millions-of-users-to-64-bit-architecture (solo resumen)
- https://discord.com/blog/discord-patch-notes-october-1-2024 (solo resumen)
- https://discord.com/branding

**Charlas y open source publicados por Discord**
- https://gitnation.com/contents/accessibility-at-discord (Brandon Dail, 2021)
- https://github.com/discord/focus-layers
- https://github.com/discord/react-dnd-accessible-backend
- https://github.com/discord/sorted_set_nif

**Terceros (confianza media o baja)**
- https://www.howtogeek.com/853659/10-discord-features-you-should-be-using/
- https://www.pcworld.com/article/3005317/discord-admits-windows-11-app-hogs-ram-tries-solving-it-with-auto-restarts.html
- https://alternativeto.net/news/2025/12/discord-admits-excessive-ram-usage-on-windows-app-and-tests-auto-restarts-as-temporary-fix
- https://blog.bytebytego.com/p/how-discord-serves-15-million-users
- https://git.secluded.site/zed/commit/7d1cb8b072c6eeeaee253a7158e323401cb529d5
- https://primagames.com/tips/did-discord-change-its-font-answered
- https://piunikaweb.com/2022/12/02/discord-gg-sans-font-announcement-fans-asking-for-proper-reveal/
