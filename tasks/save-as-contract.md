# Guardar como: contrato de implementación de 08

Contrato aprobado del 2026-10-15. «Guardar como» se divide en **08.1 plataforma**,
**08.2 Core/UI** y **08.3 reemplazo autorizado**. El selector Linux 08.1 está
verificado; **08.2 está implementado desde el 2026-10-17** (ver
[estado de 08.2](#estado-de-082-2026-10-17)); el reemplazo 08.3 está
implementado desde el 2026-10-18 ([abajo](#083-reemplazo-autorizado-2026-10-18)).
El recorrido general 08 continúa abierto.

La política inicial es **CreateNewOnly**: guardar en el nombre exacto elegido
sólo si no existe. No sobrescribe, no trunca y no añade un sufijo automático.
Si existe, muestra: **«Ese archivo ya existe. Elegí un nombre diferente.»**
Una nueva selección exige otra acción explícita. El reemplazo queda para 08.3.

## Alcance y propiedad

La UI principal sigue siendo limn. El selector del sistema es la única ventana
nativa de este recorrido. No se cambian capacidades de proveedores ni se
habilitan uploads reales. Guardar un adjunto recibido no exige SEND o ATTACH.

Ui, Store e Intents conservan sólo IDs, fase y un resumen seguro. Las rutas,
la URI del destino, descriptores y capacidades de escritura permanecen en
plataforma/Core; no aparecen en callbacks UI, logs o SQLite. La QA utiliza
sólo fixtures propios, sin mostrar ni inspeccionar el home del usuario.
Los datos remotos y la URL sólo bajan al servicio después de resolver la
entidad vigente. Nada abre o ejecuta automáticamente el archivo guardado.

La descarga existente «Guardar en Descargas» conserva su acción. La política
de nombre exacto de SaveAs no modifica su elección histórica de nombres.
La integración no debe reutilizar `DownloadResult` con un `PathBuf` hacia la
UI: los resultados de SaveAs contienen únicamente scope y resultado/resumen.

## 08.1: selector de destino en plataforma

**Estado: cerrado en plataforma.** El [checkpoint de evidencia](../docs/evidence/save-file-picker-checkpoint-2026-10-15.json)
registra 65 pruebas de plataforma (64 unitarias y un doctest), 1112 pruebas
de workspace correctas, 6 ignoradas y 25 suites; formato, Clippy estricto y
release correctos. La selección nativa devolvió la URI exacta del fixture,
con carpeta/nombre elegidos y destino todavía ausente, sin modificar archivos.
La cancelación produjo su terminal a los 8 s; en ese intento no se observó
una ventana. Este cierre no acredita descarga ni integración con conversaciones.

API acordada como punto de partida para TDD:

```rust
SaveRequest {
    id: PickId,
    suggested_name: String,
    parent: Option<ParentWindow>,
    current_folder: Option<PathBuf>,
}

FileChooser::save_file(&mut self, &SaveRequest, CancelSignal)
    -> impl Future<Output = Result<SaveDestination, PickError>> + Send;
```

`SaveRequest::validate()` rechaza solicitudes inválidas antes del RPC.
`suggested_name` se traduce a la opción `current_name` del portal.
Plataforma valida el sugerido sin sanearlo; Core sanea su sugerencia remota
antes de construir la solicitud, sin modificar después el nombre elegido.
`SaveDestination` no implementa Clone ni Serialize. Mantiene el path privado
y Debug redactado; `into_path(self)` sólo lo transfiere a un servicio de Core.
Es una selección, no un descriptor ya abierto ni una garantía de que el nombre
siga libre. Plataforma no declara política de overwrite: CreateNewOnly se
aplica en Core durante 08.2. 08.1 no abre, crea o trunca el destino, no resuelve
symlinks y no lee contenido.

Se usa `FileChooser.SaveFile`, no `SaveFiles`. Se aceptan exactamente una URI
local válida y una respuesta del propietario/path esperados. Se conservan
los límites existentes: respuesta útil de 64 KiB, URI/path de 4096 bytes,
nombre sugerido de hasta 180 bytes, RPC de 2 s y diálogo de 300 s. No se hace
trim del nombre elegido: un componente formado por espacios es válido en
Linux y se conserva. La presentación del nombre es independiente del path.
`current_folder` permite una carpeta de fixtures propia para QA; es una
sugerencia, no una condición de autorización. Un parent vacío en Wayland
sigue siendo una limitación explícita hasta disponer de un token exportado.

SaveFile reutiliza la conexión, el FileWorker y el ciclo de vida del mismo
`FileChooser`: suscripción antes de RPC, Response temprana, owner único,
validación del handle, cancelación watch y un único resultado terminal.
`Close` fallido retira la conexión; la renovación conserva el worker. El
descarte fuera del contexto actual del runtime mantiene la protección
RuntimeBound y la limpieza en el runtime original. `wait_idle` drena trabajo
anterior y limpieza antes de otro diálogo; no basta con soltar el future.
No se crea otro runtime, selector o hilo por request.

Cancelación normal del selector produce Cancelled, no un error de descarga.
Las respuestas ajenas, duplicadas o posteriores a un terminal no producen
un segundo resultado ni una concesión.

La [especificación de FileChooser](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)
describe nombre y carpeta sugeridos, un handle de petición y una URI de
resultado. No entrega identidad de inode ni una autorización atómica para
reemplazarlo. La posible confirmación visual del backend no establece ese
contrato para RUNE.

## Prerrequisito de 08.2: un dueño de diálogos nativos

Antes de conectar SaveAs se extrae **sólo la propiedad del FileChooser** del
actor de uploads a `SharedNativeDialogs` en Core. El servicio tiene un único
actor dueño en runtime, una admisión total y un mailbox acotado a uno para
Open/Save. El FileChooser es persistente y se crea de forma lazy sólo cuando
se admite un diálogo. No hay una segunda cola de desborde, otro selector ni
una conexión portal para Paths.

El contrato interno mínimo, privado de Core, es:

```rust
enum DialogAdmissionError { Busy, Unavailable }

SharedNativeDialogs::try_open(&PickRequest, CancelSignal)
    -> Result<PendingDialog<Vec<SelectedFile>>, DialogAdmissionError>;
SharedNativeDialogs::try_save(&SaveRequest, CancelSignal)
    -> Result<PendingDialog<SaveDestination>, DialogAdmissionError>;
SharedNativeDialogs::open_drain_watermark() -> OpenDrain;
SharedNativeDialogs::wait_open_drained(OpenDrain, CancelSignal)
    -> impl Future<Output = Result<(), PickError>> + Send;
SharedNativeDialogs::close();
```

`PendingDialog` se espera y consume sólo en los actores de Core, nunca en UI.
El resultado exitoso viaja en un sobre privado `DialogReply<T>` que conserva
su `AdmissionLease` hasta consumir o descartar el sobre. Los objetos no son
Clone; no hay una copia del resultado con descriptores. Los errores de admisión
pertenecen a Core: **no existe `PickError::Busy`** en plataforma. Busy conserva
la solicitud prestada en el caller sin abrir otra ventana; el job copia sólo
los metadatos validados y acotados cuando se admite. Un mailbox lleno devuelve
Busy; un servicio/mailbox cerrado devuelve Unavailable. Ambos dejan la reserva
libre. Si la
fábrica lazy falla después de admitir el job, entrega un error terminal y
completa su marca sólo tras descartar cualquier trabajo pendiente; no retiene
la lease ni deja Paths esperando indefinidamente.

Cancelar o descartar `PendingDialog` invalida su operación fuera de la cola,
sin necesitar capacidad en el mailbox. Cada operación tiene identidad interna
propia; no se cancela por un request de usuario reutilizado. Los contadores se
incrementan con `checked_add`: al agotarse se rechaza la admisión con
Unavailable, sin saturar, envolver ni reutilizar una identidad.

Un terminal Cancelled puede entregarse pronto para retirar el hold de UI,
pero el actor mantiene la `AdmissionLease` y **drena de forma autónoma** el
FileChooser y su resultado de filesystem pendiente. No espera a otra petición
para liberar ese trabajo. El drenaje llama a `FileChooser::wait_idle` con una
watch interna inicialmente **false** y su sender retenido mientras el servicio
vive; nunca reutiliza la watch de usuario ya cancelada. Hasta completar esa
limpieza, incluso si el terminal ya fue consumido, toda nueva admisión de
Open/Save devuelve Busy. Un resultado exitoso sólo se entrega después del
drenaje; su lease pasa al sobre de respuesta. Si el consumidor desapareció,
el actor descarta el resultado y sus descriptores en segundo plano.

Paths captura `OpenDrain`, la marca del **último Open encolado con éxito**,
antes de iniciar su fase de archivos. El candidato de secuencia y la admisión
se publican sólo tras el enqueue exitoso: un mailbox cerrado/lleno u otro fallo
revierte la reserva y no avanza la marca. Esa marca sólo se completa después
del drenaje total de aquel Open, no al emitir su terminal Cancelled. Paths
espera la marca con una watch cancelable, sin job adicional, RPC, conexión ni
selector. Si no hubo Open anterior, la espera termina inmediatamente. Una
Save admitida después no retrasa esa marca: Paths no espera un diálogo Save
ajeno. Después conserva también el drenaje de su PrepareWorker anterior.

El coordinador no conoce proveedores ni transporta buffers de archivos.
El actor de uploads conserva registro, leases, PrepareWorker y retry; Paths
no solicita un diálogo. La extracción no refactoriza ese registro ni la lectura
de bytes de uploads. Los objetos de plataforma permanecen en servicios.

Al cerrar Client se llama a `SharedNativeDialogs::close` **antes** de soltar
RuntimeOwner: se rechazan nuevas admisiones y se cancelan operaciones y esperas
internas. No hay join síncrono ni drenaje de payloads en UI. La propiedad de
operaciones, resultados pendientes, grants y finalizadores debe quedar en
segundo plano durante el cierre. RuntimeOwner traslada el runtime a su hilo
de shutdown; el Drop de Tokio puede cancelar tareas async, por lo que el orden
de llamadas por sí solo **no garantiza que el drenaje async haya concluido**.
La implementación de 08.2 debe conservar la propiedad necesaria durante ese
handoff y comprobar la limpieza real de ClientDrop, no limitarse a afirmar el
orden de `close`. Esta sección es el contrato objetivo de la extracción;
SharedNativeDialogs todavía no está implementado en 08.1.

## 08.2: Core, host y UI conectados

El menú limn del adjunto añade **«Guardar como…»**, con ID
`attachment:{message}:{attachment}:save-as`. Se conserva el patrón de IDs
de dominio y el control es accesible por teclado.

```rust
Intent::SaveAttachmentAs { request, channel, message, attachment }
Intent::CancelSaveAttachment { request }
```

Estos Intents sólo llevan IDs. Los DTO usan los IDs de dominio existentes
`UserId`, `ChannelId`, `MessageId` y `AttachmentId`, además del request
correlacionado. Core añade la época de sesión; no introduce dependencias de
`rune-model` hacia Core o hacia un proveedor concreto para definirlos.

El scope del trabajo fija cuenta, época, request, canal, mensaje y adjunto.
El host resuelve el adjunto actual en Store y comprueba VIEW y READ_HISTORY
mediante sus abilities; no acepta una URL retenida en un callback viejo.
La fachada de Core vuelve a validar identidad/scope, origen permitido, URL,
tamaño y capacidad de descarga **antes de solicitar el portal**. Los datos
resueltos que recibe el servicio son RemoteFile, nombre y tamaño; no llegan
a plataforma como una operación de proveedor.

SaveAs comparte las dos descargas simultáneas y el límite de 128 MiB de
`core::files`. La reserva incluye la solicitud que espera destino: una cola
llena, origen inválido o tamaño excesivo no debe abrir un diálogo inútil.
No se envían cabeceras de cuenta, no se siguen redirects no autorizados y se
mantienen comprobaciones de longitud durante y al terminar la transferencia.
Nunca se recorta el cuerpo para que entre en el límite.

Ui conserva un único estado pequeño de SaveAs, con fase **Choosing** o
**Saving** y cancelación en curso. El request se obtiene del contador vigente;
no se reinicia al cambiar de canal. Los callbacks públicos sólo llevan scope
y resultados pequeños: transición Saving y terminal Saved/Cancelled/Failed.
Saved incluye el nombre saneado, nunca la ruta. Un rechazo de admisión también
retira el estado UI exacto y no consume borrador ni respuesta.

Choosing muestra **«Eligiendo dónde guardar…»** y mantiene el hold de diálogo
para lectura automática hasta su terminal o transición. Después de aceptar y
limpiar el selector, Saving muestra **«Guardando archivo…»** y libera ese hold;
la descarga no mantiene un modal ficticio durante su plazo de red. Cancelar
Choosing cierra la petición nativa; cancelar Saving detiene el trabajo y
limpia el parcial. Los controles de cancelación no dependen de conservar el
permiso que inició la operación.

Navegar cancela Choosing. Una descarga ya admitida en Saving puede continuar
en la misma cuenta sin mover foco ni selección. Cerrar sesión, cambiar cuenta,
eliminar la entidad o revocar lectura cancela el trabajo no publicado. Se
revalida el scope antes de red y publicación. Un terminal de una época vieja
retira sólo su solicitud original, incluso tras cambiar cuenta, sin publicar
metadatos ni notificaciones de esa cuenta en la nueva. Un callback viejo jamás
afecta una solicitud nueva ni un borrador.

Los textos de resultado son **«Archivo guardado»**, **«No se pudo guardar el
archivo»** y el mensaje de nombre existente indicado arriba. Cancelled sólo
retira el estado; no muestra un toast de error. Timeout, Busy y errores de
escritura tienen mensajes seguros distintos de una cancelación voluntaria.

### Escritura CreateNewOnly

Tras aceptar el destino, el servicio ancla el directorio padre una sola vez
en un descriptor y conserva el nombre final exacto. Comprueba si ya existe
antes de iniciar HTTP; esa comprobación no sustituye la exclusión atómica
final. Un archivo, directorio o symlink preexistente provoca Exists.

La descarga se escribe en un parcial privado del mismo directorio. Sólo se
publica después de verificar tamaño/completitud y terminar la escritura,
mediante una operación atómica que rechaza un destino existente. Si otro
proceso crea ese nombre durante la descarga, se conserva el existente y se
elimina el parcial. No se abre el destino con truncado, no hay reemplazo y no
se prueba otro nombre automáticamente. Si no puede garantizarse esa
publicación, se devuelve error; no se degrada a copiar sobre el nombre final.

Error, cancelación o logout antes de publicar limpian el parcial. Una
publicación ya completada no se deshace por una cancelación tardía; el punto
de commit delimita el resultado. Las operaciones de directorio/archivo y sus
destructores permanecen en servicios, sin I/O bloqueante en UI.

## Verificación y cierre de fases

El coordinador registra RED funcional antes de implementar, luego GREEN.
Una falta de API durante la migración se etiqueta como error de compilación,
no como prueba funcional RED. Cargo se ejecuta de forma serial coordinada.

08.1 requiere pruebas en bus privado para SaveFile/options/URI única,
Response antes del retorno, owner/path incorrectos, cancelación antes y
después de admisión, respuesta duplicada, Close fallido, Drop del future
fuera del contexto actual y renovación conservando worker. El ejemplo nativo
usa una carpeta temporal propia, no red ni cuentas, sin mostrar nombres del
home. La evidencia distingue diálogo/selección de escritura o descarga.

08.2 requiere regresiones de exclusión Open/Save en ambos sentidos, cancelación
con cola ocupada y drenaje sin segundo diálogo. Un resultado FD retenido por
una barrera controlada debe mantener Busy después del terminal Cancelled hasta
su drenaje autónomo, sin otra petición. Paths después de un Open cancelado
espera su marca y libera el resultado previo; Paths durante un Save posterior
no abre ni espera ese portal. Sin Open previo no se llama a la fábrica ni se
crea una conexión. Mailbox cerrado/lleno y secuencia agotada no dejan leases
ni avanzan una marca de Open no admitido. Un fallo de fábrica posterior al
enqueue completa la marca de ese Open sin dejar una espera huérfana. Se prueba descarte
de PendingDialog, respuesta huérfana y último Drop de grants fuera de UI.
ClientDrop debe comprobar la limpieza observable del servicio, con operación
y resultado pendientes, durante el handoff real de RuntimeOwner; una prueba
del orden de llamadas a `close` no basta.
UI/host prueban IDs/entidad vigente, accesibilidad,
Busy/cancelación sin consumir borrador, Saving liberando hold, callbacks
viejos y cambio de cuenta/permiso. Core prueba validación antes del portal,
dos slots y 128 MiB, límites del cuerpo, fallo de red y limpieza.

Los fixtures de escritura prueban destino nuevo exacto, archivo/symlink ya
existente intacto, creación concurrente antes del commit, ausencia de sufijos,
cancelación y fallo sin parcial visible. Un servidor HTTP local sintético
permite probar la integración sin descargar datos de una cuenta real.
Después corresponden workspace, formato, Clippy, release y QA nativa con
fixture propio: selección, cancelación y guardado completo comprobado por
contenido/identidad esperados fuera de la UI. No se afirma un cierre de 08.2
con sólo un selector que devuelve una ruta.

Cada fase conserva binarios, fuentes y evidencia de su alcance antes de
actualizar el estado. Ni 08.1 ni 08.2 cierran el recorrido general 08:
reemplazo, otros servicios del SO y plataformas no verificadas siguen
separados. Ver [recorrido 08](todo.md#08-platform--archivos-avisos-y-servicios-del-sistema)
y [archivos](../docs/FILES.md).

## Estado de 08.2 (2026-10-17)

Implementado y probado con fixtures; sin QA con el portal real.

- Menú del adjunto y encabezado del visor (`ImageViewer::action`, PR #72 de
  limn) con el ID acordado; Intents con IDs; el host resuelve el adjunto
  vigente. UI con una sola operación, fases Choosing/Saving con progreso,
  «Cancelar» y los textos de resultado acordados; Cancelled no avisa.
- Core: `Client::save_attachment_as` valida origen, tamaño y lugar libre
  antes del diálogo; `cancel_save` y `poll_saves` (`SaveUpdate`: Saving,
  Progress, Saved con el nombre, Cancelled, Failed). Comparte los dos lugares
  y los 128 MiB de `core::files`; cerrar sesión o cambiar de cuenta cancela.
- Escritura CreateNewOnly: existencia comprobada antes de HTTP, parcial
  privado en el mismo directorio, verificación de longitud y publicación con
  `hard_link` al nombre exacto (falla si existe); el parcial se borra siempre.
- **Desvíos:** no se extrajo `SharedNativeDialogs`; el actor de adjuntos,
  que ya era dueño único del `FileChooser`, suma `Job::Save` y excluye
  Open/Save entre sí (Busy). El directorio no se ancla en un descriptor: la
  publicación usa la ruta (atajo; un `openat`/`linkat` lo cerraría). No hay
  marca `OpenDrain` propia: Paths sigue drenando con `wait_idle` como antes.

## 08.3: reemplazo autorizado (2026-10-18)

Rama `escritorio-linux`; detalle en
[FILES.md](../docs/FILES.md#reemplazar-en-guardar-como-083-2026-10-18).
Lo que pedía este punto y cómo quedó:

- **Confirmación**: sólo un archivo común con el nombre elegido se
  pregunta (`SaveUpdate::Exists`), antes de descargar; nada se escribe sin
  «Reemplazar». Carpetas y enlaces se siguen rechazando como en
  CreateNewOnly.
- **Identidad**: se guarda el dispositivo y el inodo del archivo
  preguntado y, justo antes de publicar, se exige el mismo; otro archivo
  puesto ahí se conserva y se avisa.
- **Publicación**: parcial privado en la misma carpeta y `rename(2)`, que
  cambia la entrada de una vez.
- **Lo que no se garantiza**: entre la última mirada y el `rename` queda
  una ventana mínima; Linux no tiene un rename que compare el inodo que
  reemplaza (no hay compare-and-swap). No se presenta como garantía de
  reemplazar sólo el objeto que la persona vio, sino como su aproximación
  más cercana. La respuesta del portal sigue sin probar nada del inodo.
