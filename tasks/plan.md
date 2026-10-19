# Plan de continuidad de RUNE

## Continuidad desde el análisis de Discord (2026-10-15)

El dueño solicitó corregir los pendientes ya resueltos, ampliar la inspección
estática del ejecutable y módulos distribuidos de Discord con REA y las
herramientas disponibles, y continuar cada recorrido con subagentes
independientes. La lista canónica de trabajo es la de
[21 recorridos](todo.md#recorridos-pendientes-2026-10-15), trasladada del canvas
`RUNE-discord-gap-analysis.canvas.tsx`; este documento registra cómo integrarlos.

La entrada de invitación, los filtros locales combinados, el alcance local
de lecturas y la presencia condicional de privados ya están implementados.
Se conservan sus checkboxes y las **651 pruebas aprobadas, seis ignoradas**
como resultado de la entrega anterior. Esta revisión documental no vuelve a
ejecutar esa suite ni convierte la nueva inspección REA en una implementación.
La implementación posterior ya produjo un checkpoint nuevo de **716 pruebas
aprobadas, seis ignoradas**, con formato, Clippy y release correctos, más cuatro
capturas REA nativas de fixtures. Ver [registro](../docs/evidence/implementation-checkpoint-2026-10-15.json).

### Orden, contratos y responsables

Cada recorrido se asigna a un trabajador con archivos y contratos propios;
el integrador revisa y cierra un incremento verificable antes de integrar el
siguiente. La investigación estática y las pruebas independientes pueden
avanzar en paralelo, coordinando Cargo y cualquier objetivo REA compartido.
Las dependencias de la lista prevalecen sobre su numeración:

- Comenzar por 01 (`storage`): identidad/cuenta/sesión, coherencia de rangos y
  colas. La persistencia de proveedores reales sigue apagada hasta verificar
  aislamiento y los requisitos de seguridad aplicables.
- Cerrar 02–04 sin rehacer lo existente: invitaciones con aceptación pausada,
  integración por capacidad y reproducción del riesgo de caché del adaptador.
  No implementar un arreglo de eviction antes de demostrar la necesidad.
- Adelantar la frontera de plataforma 08 que necesita selección de archivos
  de 07. Contexto por ID permite 05/06 y también navegación 18 e hilos 13.
- La salida de audio local 09 es independiente de privados/relaciones; debe
  alimentar reproducción 16 y preceder transporte/mezcla de llamadas 17.
  Cámara/pantalla 20 requieren su propio permiso y ciclo de vida.
- Cuentas 10, privados 11, relaciones 12, hilos/foros 13, contenido 14,
  perfiles 15 y administración 19 se integran por contratos concretos; las
  dependencias de uploads y capacidades no se sustituyen por botones.
  Ortografía y atajos 21 quedan separados de la captura multimedia.

Por recorrido: tipos/capacidades y eventos → estado y permisos → interacción
limn completa → fixtures/cancelación/errores → adaptador cuando exista una vía
apropiada verificable. Se reutilizan timeline, composer, perfiles y visores.
`rune-platform` ya se creó con su primera frontera Linux y Core lo consume; no se crea
un crate de red vacío. Toda carencia de limn sigue su repositorio/worktree,
tests, documentación, changelog, galería y PR.

### Reconciliación concurrente de fijados de 03, verificada

Una lectura activa por canal/petición conserva un delta de hasta 256 IDs con
estado fijado/desfijado/borrado. No duplica textos ni URLs. Un borrado es terminal
y se respetan lápidas y versiones antes de publicar la instantánea; un overflow
impide publicarla y conserva un sentinel hasta su respuesta o plazo terminal
`Failure::Pins`, seguido de reintento explícito. La entrada
provisional no aparece como un resultado completo.

Store valida `begin_pins` y limpia `cancel_pins`. La UI cancela al cerrar,
reemplazar o perder permiso; el host conecta esos hooks antes de enviar el
comando. Client correlaciona la única lectura admitida, su plazo y sus resultados,
incluido el fallo terminal al desconectar. Los timers se pausan con la presión
de SQLite como los de historial/envío. Estado/UI, cliente y host se reparten
entre tres trabajadores con revisión cruzada y pruebas de regresión.
El checkpoint pasó 741 pruebas de workspace, con seis ignoradas; formato,
Clippy estricto y release correctos. Las capturas nativas del panel cargado y
loading usan sólo simulación: [registro](../docs/evidence/pins-checkpoint-2026-10-15.json).

### Frontera implementada y verificada para 04

Primero reproducir la expulsión con más de 512 mensajes del adaptador y un
mensaje propio aún visible. La recuperación de autoría para editar/borrar
requiere un GET acotado por canal/id, seguido de nueva validación de cuenta,
visibilidad, permisos y autor antes de cualquier escritura. La documentación
pública de [Get Channel Message](https://docs.discord.com/developers/resources/message#get-channel-message)
describe esa lectura; no se infiere una vía de aceptación ni se permite
autoría desconocida. Se conservan cola de 16 operaciones, una REST en vuelo
y caché de 512 mensajes.

Para actualizaciones parciales tras expulsión, preferir un `MessagePatch`
tipado de dominio, `Event::MessagePatched` y `MessageMutation::Patch`, frente
a lanzar GETs por cada evento desconocido. Ausencia conserva campos;
colección o contenido vacío los vacía; nullable presente puede limpiar el
valor. Autor sólo puede afirmar la identidad original; no cambia autoría.
Contenido y referencias se actualizan juntos. No se parchean id, canal,
fecha de creación, tipo ni entrega; no se reconstruye un mensaje ausente ni
se resucita una lápida. Una versión de edición explícitamente anterior se
descarta. Los patches deben actualizar también la base de una reversión
optimista pendiente, sin eliminarla por una reacción u otro evento ajeno.

El checkpoint de 01/02 y los primeros guards de 03 precede esta ampliación.
`MessagePatch`, `Event::MessagePatched` y `MessageMutation::Patch` están
integrados. El checkpoint pasó 781 pruebas de workspace, con seis ignoradas,
formato, Clippy estricto y release correctos. REA verificó 131 pruebas del
adaptador y 13 regresiones de State; [registro y límites](../docs/evidence/message-patches-checkpoint-2026-10-15.json).

### Búsqueda guardada y contexto por ID de 05, verificados

Se integraron consultas SQLite cancelables y paginadas, filtros combinados,
cursores por identidad/consulta/revisión y navegación al ID exacto. El alcance
guardado tiene una respuesta fiable con crédito previo, un lector activo y
una intención pendiente; los previews opcionales no ocupan indefinidamente sus
slots si los eventos del proveedor consumen el cupo del cuadro. Un contexto
disperso no establece rangos ni marca leído; volver a recientes reemplaza esa
ventana. Mutaciones concurrentes invalidan la respuesta antes de persistir y
antes de repoblar autoría en el adaptador.

Revisiones independientes reprodujeron y cerraron carreras de snapshots,
cancelación, permisos, slots llenos y continuación local al perder red; también
errores de salida del contexto y selección de autores. La entrega pasó **851
pruebas**, seis ignoradas y 23 suites, formato/Clippy estricto/release y tres
capturas nativas con datos simulados. REA verificó 101 pruebas del núcleo y
150 del adaptador: [checkpoint](../docs/evidence/search-context-checkpoint-2026-10-15.json).
Se usaron componentes existentes de limn y no se agregaron dependencias.
La búsqueda remota completa y la persistencia real siguen separadas y pendientes.

### Contrato de 06, integración local y simulada

La posición elegida en RUNE (`ReadMark.through`, revisión local) y el
recordatorio manual (`unread_from`, ID exacto) son independientes del cursor
confirmado y su versión de proveedor. Marcar no leído nunca retrocede el cursor
remoto. Las confirmaciones llevan cuenta, canal, petición y versión; el núcleo
acepta sólo peticiones admitidas y respuestas que cubran su destino, o avances
espontáneos de la cuenta vigente. Desconexión, revocación, cancelación y plazo
terminan la espera con efecto remoto desconocido. Una respuesta vieja no borra
una elección manual ni las menciones posteriores.

La tabla `read_marks`, migración SQLite v4, guarda elecciones por cuenta/canal
mediante la cola fiable y su barrera de cierre. La restauración espera identidad
coincidente y nunca sustituye una elección local posterior. Borrar datos locales
también descarta cambios sucios previos para no repoblarlos en el mismo cuadro.
La persistencia de cuentas reales y su capacidad de confirmación siguen apagadas.

La bandeja en limn tiene una fila virtualizada por conversación visible, filtros
de canales no leídos/con menciones y navegación por contexto de 05. El primer ID
no leído puede venir explícitamente del proveedor; sin ese dato sólo se indica
la primera fila realmente disponible, sin sumar ni restar IDs. El recordatorio
manual conserva su ancla exacta aunque el mensaje ya no esté disponible. Ventana
inactiva, diálogos y contextos dispersos impiden confirmar lectura por scroll.

Almacenamiento/worker, estado/UI y proveedor simulado se implementaron con tres
trabajadores y revisión cruzada; el integrador conectó contratos, núcleo y host.
Las regresiones reprodujeron ACK sin scope, ausencia de plazo, pérdida de
elecciones locales, reintentos que consumían mensajes nuevos y revisión saturada.
La elección completa se conserva aunque la revisión alcance su máximo. La
confirmación real sigue pendiente de un contrato verificable por adaptador;
las capturas de este incremento usan exclusivamente simulación propia.

El cierre local pasó **925 pruebas**, seis ignoradas y 23 suites; formato,
Clippy estricto y release correctos. Cuatro capturas revisadas muestran bandeja,
recordatorio persistido, salto a contexto y estado vacío compacto. REA reprodujo
115 pruebas del núcleo (una ignorada), 94 de State y 32 del host sin truncación.
El [checkpoint](../docs/evidence/reads-inbox-checkpoint-2026-10-15.json) conserva
hashes, procedencia de fixtures, revisión cruzada y límites de las observaciones.

### Prerrequisito Linux de 08 para uploads 07, verificado

Se creó `rune-platform` con `file_dialogs`: un selector asíncrono de un archivo
por portal y un worker fijo que prepara su descriptor de sólo lectura sin leer
contenido. El servicio es independiente de cuentas/proveedores. En el checkpoint
de 08 todavía no estaba conectado a Core, Desktop ni composer; la integración
de 07 descrita abajo ya conserva el descriptor en Core y valida ese alcance.
La UI recibe sólo identidad opaca y metadatos. Elegir no envía.

La revisión cruzada reprodujo y cerró carreras de respuesta temprana,
cancelación durante `OpenFile`, fallos de `Close`, cambio de propietario del
portal y descarte del futuro fuera del contexto Tokio. Se conserva una conexión
D-Bus dedicada y el mismo worker al renovar; el siguiente intento espera la
limpieza previa. Las pruebas de archivos incluyen FIFO, sustitución de rutas,
UTF-8/no UTF-8 y una selección que cambiaba de destino por normalizar `..`
después de un symlink. Se rechazan las URI ambiguas antes de abrirlas.

La suite pasó **34 pruebas específicas** y el workspace **959 aprobadas, seis
ignoradas**, 25 suites; formato, Clippy con warnings como errores y release con
todas las features correctos. REA repitió las 34 sin truncación y salida 0:
[proceso](../docs/evidence/file-picker-tests-process-2026-10-15.json).
La prueba nativa observó apertura/cancelación GTK y seleccionó un fixture de
50 bytes; tamaño, dispositivo e inodo del descriptor coinciden, sin leer su
contenido: [selección](../docs/evidence/file-picker-native-select-final-2026-10-15.json).
Los intentos previos fallidos de QA se conservan; se corrigió el orden del
observador y la operación de foco, sin cambiar el binario de la selección final.
El [checkpoint](../docs/evidence/file-picker-checkpoint-2026-10-15.json) reúne
la procedencia y los límites.

Wayland se verificó sin parent; desde el 2026-10-18 la ventana se exporta con
`xdg_foreign` v2 (rama `escritorio-linux`, sin portal real que lo importe:
[diálogos sobre la ventana](../docs/FILES.md#diálogos-sobre-la-ventana-2026-10-18)).
Esto no acredita uploads, «Guardar como», notificaciones, portapapeles,
atajos, keyring ni Windows/macOS. Los checkboxes completos de 07 y 08 siguen
pendientes. [FILES.md](../docs/FILES.md#selector-local-linux-prerrequisito-de-07)
describe los contratos y las restricciones del worker.

### 07: primer recorrido de un archivo integrado

El [contrato de uploads](uploads-contract.md) se implementó con trabajadores
separados para Provider/Mock, Core y State/UI; el integrador conectó Desktop y
serializó Cargo. Los DTO de metadatos no tienen rutas ni bytes. Core conserva
un selector persistente, un worker fijo de preparación y un registro con
crédito de 32 MiB/ocho entradas que acompaña a los clones/slices de Bytes.
File-only, texto/respuesta, progreso y reintento usan el mismo recorrido;
la capacidad está habilitada sólo en la simulación local.

Las regresiones reprodujeron sobreasignación, cambios del descriptor,
cancelación con cola llena, petición/generación vieja, metadatos de ACK
incompletos, colisión de nonce entrante y pérdida de certeza. Se verificó que
el último drop normal del payload no ocurra en UI y que una preparación
cancelada se drene sin otro pick. La UI conserva el borrador nuevo al consumir
un envío anterior; reintentar nunca sustituye el archivo por texto.

La revisión reprodujo la retención de metadatos al eliminar canal/servidor y
se aplicó limpieza de los pendientes. El snapshot final pasó **1032 pruebas**
con todas las features, seis ignoradas y 25 suites, formato y Clippy estricto.
Un primer intento encontró ENOSPC en la caché incremental generada; se limpió
esa caché y la suite completa pasó al repetirla. Release del workspace con
todas las features pasó para 11 crates en 66 segundos. REA repitió Core
146 aprobadas/una ignorada, State 107, UI 299, Provider 50 y Desktop 33, con
salida 0 y terminal completo.

El checkpoint **07.1** verificó en ventana nativa el fixture propio de 50 B:
[preparado con borrador intacto](../docs/img/upload-prepared-2026-10-15.png) y
[enviado explícitamente y confirmado por Mock](../docs/img/upload-sent-2026-10-15.png),
con composer vacío y sin pendiente. El primer intento de QA abortó antes de
Enter porque esperaba el título inicial «RUNE»; el host ya mostraba
«RUNE — general». La ejecución final validó ejecutable/PID/ventana activa y
título después de cerrar el portal. Ambos intentos se conservan sin atribuir
el fallo del guard a producto. El [registro](../docs/evidence/uploads-checkpoint-2026-10-15.json)
documenta hashes, observador auxiliar, recuperación de espacio y límites de
atribución; no prueba entrega a un proveedor real.

Al cerrar 07.1, varios archivos, arrastrar/pegar y transporte remoto quedaban
para incrementos posteriores. No se habilitaron
uploads de Discord ni nuevas operaciones autenticadas. [Flujo y límites](../docs/FILES.md#adjuntos-de-salida-de-un-archivo).

### 07.2: lote múltiple implementado y verificado en local

Se amplió el mismo selector, worker y servicio: `choose_files` devuelve todos
los descriptores o un error, `PickRequest` declara cantidad/tamaños y
`Prepared` sólo un vector de metadatos. Mock admite ocho archivos, 20 MiB por
archivo y 25 MiB por mensaje incluido texto. Core conserva 32 MiB y ocho
entradas, cuenta leases aún retenidas por clones y reserva todos los créditos
y el rango de tokens antes de leer. Fallo/cancelación del segundo archivo
descarta también el primero. Publicación y callback son completos; el borrador
previo se conserva. Retry mantiene orden, bytes y nonce. ACK exige IDs remotos
distintos; cerrar sesión/cambiar cuenta entrega la cancelación del picker viejo
sin suprimir el terminal ni exponer metadatos de esa cuenta.

La revisión reprodujo límites/rango de tokens incorrectos, ACK con identidad
duplicada, terminal perdido tras cambio de cuenta y segmentos punto ocultos
por `%2F`. Las correcciones tienen RED/GREEN. El workspace pasó **1073 pruebas**,
seis ignoradas y 25 suites con todas las features, formato y Clippy estricto;
release compiló 10 crates en 49,72 s y los ejemplos cuatro en 7,21 s.
REA repitió Core 161 aprobadas/una ignorada, State 109, UI 307, Provider 54,
Platform 45 y Desktop 33 con salida 0 y terminal completo. El
[registro](../docs/evidence/uploads-multiple-checkpoint-2026-10-15.json) identifica
ocho binarios retenidos y hashes; la selección y el envío nativos de varios
archivos se verificaron en Linux/Wayland con GTK y Mock. El ejemplo entregó
dos descriptores de 48 B con dispositivo/inodo coincidentes; las capturas de
[preparación](../docs/img/upload-multiple-prepared-2026-10-15.png) y
[confirmación](../docs/img/upload-multiple-sent-2026-10-15.png) conservan el
borrador y luego muestran ambos adjuntos confirmados con editor vacío.
Los intentos 01 y 03 permanecen como QA sin selección/preparación acreditada,
separados de los finales 02/04/05. El bundle tiene 55 registros y cuatro
incógnitas. La evidencia 07.1 conserva su alcance histórico de un archivo.

07 y 08 completos siguen abiertos. El [contrato vigente](uploads-contract.md)
y la [continuación 07.3](uploads-next.md#073-arrastre-y-portapapeles) separan
arrastrar/pegar y transporte remoto del checkpoint anterior. El pin de limn no
cambió en el checkpoint 07.2; el PR upstream se preparó en su propio worktree.
La [PR #49](https://github.com/Niiihuel/limn/pull/49) está abierta; el primer
commit quedó en integración `b91772b` y la corrección posterior del orden de
entrada dejó el pin histórico de 07.3.1 en `1ad33eb`. Los artefactos retenidos 07.2 permanecen
en `2618b37`; las pruebas y el recorrido nuevo se describen por separado.

### 07.3.1: ingreso local implementado, recorrido nativo bloqueado

El prototipo 07.3.1 consumía `FilesDropped` y movía sus rutas directamente a
`Client::prepare_attachment_paths`; la política 07.3.2 de abajo retiró ese
ingreso del host. La API Core permanece: valida metadatos y comparte el Job,
control, rango de tokens y cancelación con Picker. El mismo worker fijo usa
`open_local_files`, reserva todas las leases y lee el lote completo o devuelve
error. `validate_local_paths` no hace I/O; `FileChooser::wait_idle` drena una
selección anterior sin conectar/renovar el portal. Paths no crea un selector.

La validación del prototipo rechazaba el lote completo ante exceso o rutas
inválidas, sin truncar ni leer `DroppedFile::bytes`. El pin del snapshot 07.3.1 de limn
`1ad33eb224b6b4fb48de9423b23a89d6230e087f` entrega archivos después de la entrada
ordinaria; el prototipo reconstruía la vista para volver a comprobar destino,
overlays, permisos, cuenta y cierre. La barrera de `Submitted` impedía enviar
texto antes de preparar el lote, también en accesibilidad. El host actual la
conserva para los drops rechazados, sin consumir borrador con Enter.
Shift+Enter y acciones de otros controles siguen disponibles. No se afirma
una cronología de eventos del SO ni soporte nuevo
del wrapper de touch.

Verificación dirigida: **50 pruebas Core uploads + 6 del host**, UI 6,
plataforma 54 y componente 1. Las regresiones reprodujeron entrada al destino
viejo y pérdida del borrador con drop+Enter; se corrigieron en la PR #49 y el
host. La integración limn pasó 1405 aprobadas, dos ignoradas, 13 suites en
0,92 s y Clippy estricto. La suite integral RUNE pasó **1098 pruebas**, seis
ignoradas y 25 suites en 5,47 s con todas las features. Formato y Clippy estricto
de todos los targets/features correctos; release de ocho crates en 51,13 s.
El [registro local 07.3.1](../docs/evidence/uploads-drop-checkpoint-2026-10-15.json)
conserva seis binarios, 223 archivos fuente en `1ad33eb` y cinco replays REA
canónicos con 670 aprobadas/una ignorada, salida 0 sin truncar. La QA nativa no
cerró: dos gestos con fixtures propios no prepararon adjuntos, sin Enter/envío
ni PNG aceptado. El bundle tiene 65 registros y cuatro incógnitas.

La vía RawInput usa winit 0.30.13: Wayland no produce eventos DnD de archivos
y X11 no expone su posición. La [PR #4571](https://github.com/rust-windowing/winit/pull/4571)
aporta otra API en 0.31 beta; eframe/egui 0.36.2/main siguen con 0.30.13 y
[la migración #7731](https://github.com/emilk/egui/pull/7731) es un borrador
incompleto. Hay otra vía Wayland existente: `limn-dnd` ya está en esos fuentes
git, pero no en el grafo ni el bridge de RUNE. Su
[auditoría estática](../docs/evidence/limn-dnd-existing-backend-audit-2026-10-15.json)
motivó el nuevo receiver de la [PR #55](https://github.com/Niiihuel/limn/pull/55),
Draft abierta: implementación Copy acotada, 124 pruebas de biblioteca y tres
Gallery, con QA de metadatos/Accept/Reject/cierre normal sólo en Weston privado.
[Registro](../docs/evidence/limn-drop-receiver-weston-2026-10-15.json).
Faltan el owner SDK compartido para la convivencia clipboard/DnD en Hyprland,
pin y consumidor RUNE. No inferir destino ni crear un parche/fork implícito en RUNE. 07.3.1 nativo y 07/08
permanecen abiertos; pegado y transporte remoto siguen como incrementos propios.
El owner de biblioteca #62 y el opt-in entrante #63 ya están publicados en el
[checkpoint actual](../docs/evidence/limn-text-clipboard-matrix-2026-10-15.json); queda su integración SDK/RUNE y verificación Hyprland.

### 07.3.2: rechazo del arrastre legacy sin posición fiable

La política actual limpia todos los `dropped_files` y `hovered_files` de
`RawInput` **antes del bridge**. El fallback también los rechaza: ni la última
posición del puntero ni una posición reconstruida al final del cuadro acreditan
el destino del lote. El host ya no llama a `Client::prepare_attachment_paths`;
la API Core, su worker y sus pruebas quedan para una entrada futura fiable.
El selector Adjuntar continúa conectado y la barrera de `Submitted` conserva
el borrador ante drop/rechazo junto a Enter.

Ocho regresiones del host y siete de avisos comprueban esta política y su
disposición. El pin actual `22e0fd93c14b3f3c89375135f83c1e9c41307330` incorpora
por cherry-pick propio la [PR #50](https://github.com/Niiihuel/limn/pull/50),
abierta sin merge público: altura opt-in y avisos/lista acotados con scroll.
El aviso queda encima del editor, con altura natural si es corto; reply, ocho
archivos y borrador multilínea siguen accesibles a 720×480.

**El rechazo negativo 07.3.2 y su pulido visual están verificados.** Dolphin
soltó dos fixtures propios de 54/55 B sobre Mock RUNE X11: aviso completo y
«Borrador conservado» visibles, sin adjuntos preparados ni Enter. La
[captura canónica](../docs/img/uploads-drop-native-rejected-2026-10-15.png)
eleva el inventario Mock a 18 (17 históricas y una negativa); no acredita
importación funcional. El no-envío automático lo prueban las regresiones del
host, no ese gesto sin Enter. Workspace: **1120 aprobadas, seis ignoradas,
25 suites en 5,50 s**; formato y Clippy estricto correctos, release de seis
crates en 49,56 s. REA repitió 320 UI + 101 componentes + 41 host: 462 aprobadas,
sin filtros ni ignoradas, tres suites con salida 0 sin truncar.
[Registro final](../docs/evidence/uploads-drop-rejected-checkpoint-2026-10-15.json),
[auditoría independiente](../docs/evidence/uploads-drop-rejected-audit-2026-10-15.json)
y [alcance actual](../docs/FILES.md#rechazo-del-arrastre-legacy-0732).

### 08.1: SaveFile de plataforma completo; integración pendiente

`SaveRequest`, `FileChooser::save_file` y `SaveDestination` reutilizan el dueño
del portal y sus límites/cancelación. El resultado privado sólo entrega el
destino al servicio consumidor: plataforma no crea, abre ni sobrescribe ese
archivo. **64 unitarias y un doctest**, Clippy estricto y ejemplo correctos.
La selección nativa GTK coincidió con el destino exacto esperado; el archivo
nuevo siguió ausente y el existente intacto. La cancelación produjo terminal
`Cancelled` a los ocho segundos; el observador tardío no vio una ventana, por
lo que ese intento no demuestra el cierre de una ventana observada. Los dos
recortes GTK de inspección no se cuentan como capturas canónicas de Mock.

El [checkpoint 08.1](../docs/evidence/save-file-picker-checkpoint-2026-10-15.json)
retiene dos binarios y tres replays REA con salida 0 sin truncar. El snapshot
histórico de SaveFile, con pin `1ad33eb` y la política 07.3.2, pasó **1112 pruebas**, seis ignoradas y 25 suites
en 5,56 s; formato, Clippy estricto de workspace/targets/features y release de
tres crates en 45,98 s correctos. Esto cierra sólo el selector de plataforma:
**08.2** debe integrar Core/UI/descargas y `SharedNativeDialogs`; **08.3** cubre
reemplazo autorizado. Los puntos generales 07/08 permanecen pendientes.
[Contrato de continuidad](save-as-contract.md).

### Fuentes para contexto y futuros incrementos de 03

La documentación pública actual de [mensajes](https://docs.discord.com/developers/resources/message)
ofrece consultas alrededor de un ID (`around`, excluyente con `before`/`after`,
límite 1–100) para el contexto de navegación. La búsqueda local debe indicar
«historial guardado» y conservar sus límites de retención, sin presentarlo como
todo el historial del proveedor. La búsqueda de servidor publicada requiere
su propia capacidad, restricciones y fixtures; no se deduce del soporte de
historial existente.

Antes de integrar fijados remotos, distinguir los endpoints actuales de los
deprecados y el permiso `PIN_MESSAGES` de `MANAGE_MESSAGES`. El soporte de
lectura tampoco autoriza mutar. Estos datos de documentación son contratos
para trabajo posterior, no evidencia de operaciones reales de esta entrega.

### Evidencia y criterio de cierre

La inspección estática ampliada cerró su pasada focalizada; la cobertura
total sigue **parcial**. Ghidra 12.1.2 produjo 14 dossiers REA de plataforma
y 23 descompilaciones seleccionadas de voz como evidencia suplementaria.
El preset global de voz venció su plazo de 20 minutos; Hopper no estuvo
operativo por HTTP 403 de la descarga oficial. El renderer remoto exacto
sigue desconocido. Procedencia, artefactos/hash y límites están en
[REA.md](../docs/REA.md#discord-instalado-paquete-y-módulos-nativos-2026-10-15).
Registrar herramienta realmente utilizada, cobertura, observación e inferencia.
Un wrapper o símbolo sólo acredita la frontera observada; no prueba el flujo
completo, una llamada real ni la autorización de una capacidad.

Cada entrega debe identificar por separado:

- Implementación local y verificación dirigida de la interacción.
- Comportamiento simulado, con su etiqueta visible y fixtures reproducibles.
- Adaptador validado mediante fixtures HTTP/Gateway o transporte controlado.
- Confirmación contra un servicio real, si existe, con operación y alcance
  concretos. La confirmación previa de login/identidad/lectura no acredita
  envíos, membresías, administración ni llamadas.

Los criterios de cierre en `todo.md` son verificables; las casillas de 01 y del
incremento de tarjetas/caducidad/cooldown de 02 registran su cierre local.
Los demás criterios siguen pendientes. Se marca cada recorrido sólo al
terminar su alcance y registrar sus límites.
El integrador ejecuta los checks apropiados, revisa la UI nativa pertinente y
actualiza resultados sin reutilizar los conteos históricos como pruebas nuevas.
No se tocan credenciales, sesiones de navegador ajenas ni la cuenta abierta
del dueño. La aceptación de invitaciones de producción permanece pausada,
independientemente de esta autorización de inspección. No se copia código ni activos de las referencias.

## Entrega anterior desde Serein (2026-10-14)

Solicitud del dueño del 2026-10-14: revisar los checkouts de Serein,
identificar funcionalidades faltantes y continuar RUNE con agentes en paralelo.
El inventario se integra en `docs/SEREIN_COMPARISON.md`; las tareas de esta
entrega se registran en [todo.md](todo.md).

### Alcance y criterios de la entrega anterior

- La UI principal sigue siendo limn, con textos en español y sin protocolos
  concretos por encima de rune-provider. Serein se lee, nunca se copia.
- El integrador y tres trabajadores revisan código real, además de las
  especificaciones existentes de arquitectura, invitaciones y búsqueda.
- Cada incremento debe completar una interacción local verificable. Las
  capacidades remotas no se habilitan por tener UI o pasar fixtures.
- Se conserva la pausa de aceptación experimental. No se usan cuentas ni
  credenciales reales.

### División y dependencias de la entrega anterior

1. **Invitaciones:** entrada nativa para pegar y revisar un enlace, usando
   el reconocimiento del proveedor y el preview ya existente. Sin enlaces
   reconocidos no se abre el navegador como efecto secundario. Trabajador
   `audit_auth_invites`: invitations, server_menu y cableado mínimo de Ui/host.
2. **Búsqueda:** filtros locales combinables (propios, archivos, fijados),
   conservando alcance de mensajes cargados, selección estable y borrador.
   Trabajador `audit_ui`: search.rs y tests/search.rs exclusivamente.
3. **Lectura/proveedor:** revisar qué se actualiza localmente y qué confirma
   el proveedor; hacer explícito ese límite sin inventar sincronización.
   Trabajador `audit_provider_media`: contrato y consumidores acordados antes
   de editar; no toca los archivos de los otros trabajadores.
4. **Integración:** el integrador mantiene el inventario, documenta siguientes
   etapas y ejecuta Cargo en secuencia. Revisión cruzada después de integrar.

La revisión añadió una corrección acotada a presencia en privados:
`view.rs::direct_rows` mostraba Offline por defecto sin soporte de presencia.
El trabajador de proveedor corrige esa presentación en view/tests de layout,
sin habilitar presencia remota ni cambiar contratos.

Los frentes 1–3 son independientes una vez acordadas sus interfaces. El frente
4 depende de sus entregas. Las carencias que requieran limn se trabajan en
su repositorio/worktree con PR, tests, documentación, changelog y galería.

### Conservación y verificación de la entrega anterior

El repositorio tiene índice y archivos de trabajo anteriores, y aún no tiene
commits. Se conserva una copia de las fuentes/documentos anteriores en
`/tmp/rune-parallel-baseline-1xuojq3l`; no se crean commits que mezclen ese
trabajo. Los trabajadores editan archivos acordados sin cambiar de rama.

Base medida: `nix-shell --arg withLogin true --arg withDocs true --run
'rtk cargo test --workspace --all-features'`: 636 aprobadas, 6 ignoradas.
Cada incremento lleva regresiones dirigidas; al integrar se ejecutan formato,
workspace tests con todas las features y Clippy con warnings como errores.
Las capturas, si corresponden, usan fixtures propios y datos simulados.

Resultado integrado: 651 aprobadas, seis ignoradas; formato, Clippy estricto
y release correctos. Dos capturas nativas revisadas: búsqueda con filtro de
archivos y entrada de invitación. Evidencia y hashes en
[native-workflows-2026-10-14.json](../docs/evidence/native-workflows-2026-10-14.json).
