# Adjuntos de salida: contrato de implementación 07

Contrato vigente del 2026-10-15. 07.1 verificó un archivo con ventana nativa;
07.2 integra lotes completos, pasó pruebas locales y verificó selección/envío
nativos con dos fixtures propios en Linux/Mock. 07.3.1 implementó y probó Core
Paths; la política actual 07.3.2 rechaza el ingreso legacy desde host por carecer
de posición ligada al lote en la vía RawInput de winit 0.30.13. El arrastre
nativo funcional de RUNE sigue abierto. El nuevo receiver de `limn-dnd` está
en la [PR #55](https://github.com/Niiihuel/limn/pull/55), Draft abierta, con QA
de metadatos/decisiones/cierre normal sólo en Weston privado, sin integrar en
RUNE. [Registro](../docs/evidence/limn-drop-receiver-weston-2026-10-15.json).
El recorrido completo sigue abierto por pegado y transporte remoto.
La capacidad real permanece apagada.

## Propiedad y límites

La UI y Store conservan únicamente `UploadToken` y `PendingAttachment`
(token, filename, size_bytes), tipos pequeños de `rune-model`, sin rutas ni
serialización de concesiones. Descriptores, buffers y `PreparedUpload` viven
en servicios y proveedores. Los resultados hacia UI nunca los contienen.

`rune-provider::uploads` define `UploadLimits`, `UploadError`,
`UploadSendScope`, `PreparedUpload`, `UploadFence` y `UploadUpdate`.
`Provider::upload_limits() -> Option<UploadLimits>` vale None por defecto.
`UploadLimits` contiene max_files, max_file_bytes y max_message_bytes;
Mock permite diez archivos (ocho hasta el 2026-10-18), 20 MiB por archivo y 25 MiB por mensaje incluido
el texto. La primera porción 07.1 permitía un archivo. Estos límites son una
política de la simulación; no acreditan límites ni transporte de una cuenta real.

El registro de Core tiene presupuesto compartido de 32 MiB y hasta diez
entradas (ocho hasta el 2026-10-18), una preparación simultánea y colas acotadas. No expulsa archivos
silenciosamente. Se reserva memoria antes de leer y el crédito pertenece al
buffer sellado: clones y slices lo retienen hasta su última liberación.
`Bytes::from_owner` evita una segunda copia completa. Se comprueba la
identidad y metadatos del descriptor antes/después de una lectura acotada;
reintentar conserva los bytes congelados, sin reabrir una ruta.

## Envío y correlación

`Command::SendFiles` lleva scope, content, reply_to, files y fence. El scope
contiene request, account, channel, generation, nonce y expires_at. El fence
es cancelación viva y no contiene bytes. Mock comprueba scope, deadline,
fence, permisos y política al recibir y antes de publicar. La comparación con
`EventSink::generation()` rechaza comandos encolados de una sesión anterior.

`Failure::SendFiles { request, channel, nonce }` distingue cada intento. La
confirmación reutiliza MessageCreated con nonce, pero exige cuenta/autor y
canal correctos antes de retirar el pendiente. Progreso contiene scope y
contadores; completar el cuerpo no afirma confirmación del mensaje.

Core es el único que construye el comando con payload, incluso al manejar
una cola llena. El actor conserva el envío lógico congelado para retry. El
retry usa nonce lógico estable y request nuevo; no cae nunca a envío de
texto si falta una concesión. Un resultado ambiguo exige garantía vigente
de idempotencia y acción explícita. Ningún fallo provoca retry automático.

La fachada de Client ofrece pick_attachment, prepare_attachment_paths, send_attachments,
retry_attachment_send, cancel_upload, discard_attachment y poll_upload.
Los parámetros de selección/envío llevan request/canal, tokens o texto;
prepare_attachment_paths mueve rutas locales acotadas directamente al servicio.
Ui/Store no reciben esas rutas ni los descriptores.
Los resultados de preparación y progreso llevan cuenta/canal/request. El
servicio conserva una única instancia del selector, incluso al cancelar.
El cierre invalida controles pequeños; nunca mueve el receiver de comandos
con payload a UI. Se verifica el hilo de última liberación al cerrar/cancelar.

## Estado e interfaz

Store añade pendientes de archivos con request y metadata separada del
Attachment remoto. No fabrica AttachmentId, URLs ni confirmaciones. Un fallo
viejo no cambia un intento nuevo. Unsent conserva la clase de envío y sus
tokens para impedir reintentar sólo el texto por accidente.

La UI mantiene listas por canal y un selector/preparación activo. Elegir no
envía. Se admiten mensajes sólo con archivos listos; si queda preparación se
bloquea el envío completo. Un resultado tardío sólo toca el canal y petición
originales. Consumir un borrador compara revisión y tokens enviados; una
confirmación vieja jamás borra texto, respuesta o archivos nuevos. Cambiar
conversación conserva el borrador dentro del presupuesto; cerrar sesión o
cambiar cuenta cancela y limpia. Revocar o borrar canal invalida sus trabajos.

Adjuntar/enviar/retry consultan Store::abilities con VIEW, SEND y ATTACH,
conexión e identidad vigentes. Quitar/cancelar se mantienen disponibles para
liberar recursos aun cuando se pierde el permiso. Toda la vista es limn.

## Verificación requerida

Pruebas funcionales RED antes del cambio, después GREEN: límites, archivo
modificado, presupuesto retenido por clones/slices, cancelación, cola llena,
comando viejo tras reconexión, fallo viejo durante retry, ACK de otra cuenta o
canal, file-only, selección sin envío y borrador nuevo conservado. Fixture
integrado de Mock/Client/Store/UI y captura nativa limn. Sin nuevas operaciones
autenticadas ni activación de uploads reales.

Fuentes del transporte futuro: [multipart publicado](https://docs.discord.com/developers/reference#uploading-files),
[Create Message](https://docs.discord.com/developers/resources/message#create-message)
y [bytes v1.12.1](https://github.com/tokio-rs/bytes/blob/v1.12.1/src/bytes.rs).
No se inventa un protocolo de reserva/PUT firmado no verificado.

## Implementación 07.2: lotes completos

El checkpoint de un archivo conserva su ejecutable y evidencia. El incremento
implementado amplía el mismo servicio: `PickRequest` declara `max_files`,
`max_file_bytes` y `max_total_bytes`; `choose_files` devuelve todos los
descriptores o un error. La respuesta del portal se valida completa antes de
abrir archivos, con cantidad y rutas repetidas comprobadas. No se añade un
selector ni un worker por archivo.

La preparación reserva la suma de bytes y entradas antes de leer. Cada buffer
sellado conserva su propio crédito; fallo o cancelación del segundo archivo
descarta también el primero. Core publica el lote después de revalidar cuenta,
canal, política y cancelación. `UploadUpdate::Prepared` transporta una lista de
metadatos: un callback rechazado devuelve todos sus tokens, sin publicación
parcial. La reserva de IDs avanza por la capacidad del lote con suma comprobada.

La política simulada ya admite diez archivos (ocho hasta el 2026-10-18) tras verificar
el recorrido local, con 20 MiB por archivo y 25 MiB por mensaje; Core conserva diez
entradas globales y
32 MiB de buffers. Se comprueban también tokens e IDs remotos repetidos. Retry
conserva orden, texto, respuesta, todos los bytes y el nonce lógico. La UI
limita la altura del listado y mantiene visible el editor.

Selección múltiple, arrastre y pegado tienen verificaciones distintas. El
incremento de lotes no acredita los dos últimos ni un transporte remoto.
La [evidencia local 07.2](../docs/evidence/uploads-multiple-checkpoint-2026-10-15.json)
registra 1073 aprobadas, seis ignoradas y 25 suites, formato, Clippy estricto
y release correctos. El ejemplo nativo entregó dos descriptores de 48 B con
identidad coincidente; las capturas muestran preparación sin consumir borrador
y confirmación de ambos adjuntos por Mock con editor vacío. Se cierra sólo
07.2 local; la captura de un archivo pertenece al checkpoint histórico 07.1.
El checkpoint 07.2 conserva `2618b37`; los cambios de limn y del ingreso de
rutas de 07.3.1 tienen su propia verificación, descrita a continuación.

## Implementación 07.3.1: rutas de archivos del SO

`Client::prepare_attachment_paths(request, channel, Vec<PathBuf>)` converge
con `pick_attachment` en un `PickInput::{Picker,Paths}`, un solo Job y el mismo
control de preparación. La admisión usa `validate_local_paths` sin I/O: uno a
diez paths absolutos (ocho hasta el 2026-10-18), sin NUL ni repetidos, hasta 4096
bytes por path y 40 KiB agregados. También acota las capacidades de asignación retenidas por el job.
Un rechazo no reserva tokens ni inicia preparación.

`rune-platform::file_dialogs::open_local_files` es síncrona y sólo se llama
en el worker fijo de preparación de Core. Ese hilo abre todo el lote como
descriptores de sólo lectura, reserva todas las leases y después lee. No se
crea otro hilo por archivo ni se abre una conexión portal para Paths. Si ya
había selector, se drenan `FileChooser::wait_idle` y el resultado anterior del
preparador antes de abrir nuevas rutas. Cancelación, scope, presupuesto,
publicación atómica y retry congelado conservan el contrato de 07.2.

El prototipo host 07.3.1 recibía la acción de limn y, tras validar vista y
permisos, iniciaba preparación. Se retiró ese ingreso en 07.3.2. El pin histórico de limn
`1ad33eb224b6b4fb48de9423b23a89d6230e087f` entrega archivos después de la entrada
ordinaria del cuadro; el prototipo reconstruía la vista antes de resolver su
destino. Un drop rechazado bloquea `Submitted` del editor en ese mismo cuadro,
también en la ruta de accesibilidad. Enviar requiere otra acción posterior;
Shift+Enter y acciones de otros controles no se bloquean por esa barrera.

Pruebas dirigidas verificadas: Core uploads 50, host 6, UI 6, plataforma 54 y
componente 1. Cubren preparación congelada sin envío automático, fallo del
segundo archivo sin subset, Busy/cancelación/drain, destino obsoleto o cubierto,
lote rechazado y drop junto a Enter conservando borrador y Mock sin mensajes.
La suite integral del snapshot 07.3.1 pasó **1098 pruebas**, seis ignoradas y
25 suites en 5,47 s con todas las features. Formato y Clippy estricto de todos
los targets/features correctos; release de ocho crates en 51,13 s.
El [registro local 07.3.1](../docs/evidence/uploads-drop-checkpoint-2026-10-15.json)
separa los replays verdes de dos gestos nativos sin preparación, Enter/envío
ni PNG aceptado. La vía RawInput Wayland no produce eventos DnD de archivos con winit 0.30.13;
X11 no expone su posición. No se infiere destino desde foco o puntero global.
La [API nueva de winit](https://github.com/rust-windowing/winit/pull/4571) requiere
0.31 beta y [la migración egui/eframe](https://github.com/emilk/egui/pull/7731)
sigue incompleta. No es la única vía: `limn-dnd` ya existe en esos fuentes git,
fuera del grafo/bridge RUNE. Su
[auditoría estática](../docs/evidence/limn-dnd-existing-backend-audit-2026-10-15.json)
motivó la PR #55 receive-only ya implementada y probada de forma limitada en
Weston; faltan owner SDK compartido para Hyprland, pin y consumidor RUNE.
Repetir QA de RUNE cuando un backend integrado entregue posición/lote reales;
07.3.1 nativo permanece abierto, sin afirmar soporte Windows/macOS.

## Política actual 07.3.2: rechazar legacy sin posición fiable

`raw_input_hook` elimina todos los archivos soltados y hovered antes del bridge;
el fallback `FilesDropped` también rechaza. Un lote dentro de los límites no
se admite por usar la última posición de RawInput, foco o canal actual. El host
no llama a `Client::prepare_attachment_paths` desde esta entrada. Core conserva
la fachada, worker, presupuesto y pruebas para un contrato futuro con posición
fiable. Adjuntar mediante selector mantiene su recorrido verificado.

La regresión reprodujo la admisión anterior y pasaron ocho regresiones del
host y siete de avisos. **El rechazo negativo y el pulido visual están
verificados.** El pin actual `22e0fd93c14b3f3c89375135f83c1e9c41307330` consume
por cherry-pick propio la [PR #50](https://github.com/Niiihuel/limn/pull/50),
abierta sin merge público. Los avisos quedan en flujo sobre el composer, con
altura opt-in y scroll acotado; la lista de adjuntos también acota su altura.
La QA real Dolphin → Mock RUNE X11 con dos fixtures de 54/55 B mostró el
aviso completo y «Borrador conservado» visible, sin adjuntos preparados ni Enter.
El no-envío automático se acredita mediante tests del host, no por ese gesto.

El snapshot final pasó **1120 pruebas**, seis ignoradas y 25 suites en 5,50 s;
formato y Clippy estricto correctos, release de seis crates en 49,56 s. Los
tres replays REA completos suman 462 aprobadas sin filtros ni ignoradas, con
salida 0 sin truncar. El
[registro 07.3.2](../docs/evidence/uploads-drop-rejected-checkpoint-2026-10-15.json),
su [auditoría](../docs/evidence/uploads-drop-rejected-audit-2026-10-15.json) y la
[captura negativa](../docs/img/uploads-drop-native-rejected-2026-10-15.png)
cierran sólo este rechazo. Hay 18 capturas canónicas Mock, 17 históricas y una
negativa; no se declara importación funcional ni envío por arrastre.

SaveFile 08.1 conserva su snapshot histórico de 1112 pruebas y pin `1ad33eb`.
Cerró sólo API/ejemplo de plataforma: 64 unitarias y un doctest,
selección del destino exacto sin crear archivo y cancelación con terminal,
sin ventana acreditada para ese intento. [Registro](../docs/evidence/save-file-picker-checkpoint-2026-10-15.json).
No conecta descargas ni modifica uploads: Core/UI/NativeDialogs son 08.2 y el
reemplazo 08.3, ambos pendientes en [su contrato](save-as-contract.md).
Los puntos generales 07/08 siguen abiertos.

## Transporte real y pegado (2026-10-17)

La sesión no oficial declara `attachments` y sube por la ruta de Serein:
destino por archivo, `PUT` sin sesión al almacenamiento de Discord y un
mensaje con los archivos guardados ([detalle](../docs/DISCORD_SESSION.md#adjuntos-de-salida-2026-10-17)).
Sólo fixtures en loopback. Cambios de contrato:

- `Provider::upload_limits` de la sesión es el techo (10 archivos, 500 MiB);
  `Event::UploadLimit { server, max_file_bytes }` informa el límite real por
  cuenta y servidor, Store lo guarda y la UI valida contra el del canal.
  Core recorta los límites a su presupuesto en memoria (32 MiB).
- El plazo de un envío con archivos suma 1 s cada 128 KiB.
- La confirmación compara cantidad, orden y tamaños, ya no el nombre: el
  servicio puede reescribirlo (Discord cambia espacios por guiones bajos).
- `PickInput::Clipboard` y `UploadError::Empty`: el portapapeles como otra
  fuente del mismo job ([detalle](../docs/FILES.md#pegar-del-portapapeles-2026-10-17)).
