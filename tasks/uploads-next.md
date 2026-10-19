# Adjuntos: lotes 07.2 y continuación 07.3

Diseño del 2026-10-15, actualizado con 07.2, Core Paths y la política 07.3.2.
El coordinador verificó workspace, replays y selección/envío nativos de dos
archivos con GTK/Mock. Se cierra sólo 07.2 local; el checkpoint 07.1 conserva su evidencia
de un archivo. 07.3.1 probó Core Paths; el host actual 07.3.2 rechaza el arrastre
legacy sin posición ligada al lote. Su recorrido nativo sigue abierto con
winit 0.30.13.
La integración del portapapeles en RUNE sigue pendiente; la API entrante limn y
su matriz dirigida de 184 pruebas ya tienen aceptación local, descrita abajo. No se abrió ni
se accedió a Discord durante esta investigación.
Uploads reales apagados.

## 07.2: selección y envío de varios archivos

Implementado y probado en local con un servicio, un selector, un request y
un envío lógico por lote. La API interna se actualizó en todos sus consumidores;
no se mantienen variantes `choose_file`/`choose_files` ni DTO v1/v2 en paralelo.

```rust
PickRequest {
    id: PickId,
    max_files: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
    parent: Option<ParentWindow>,
    current_folder: Option<PathBuf>,
}

FileChooser::choose_files(&mut self, &PickRequest, CancelSignal)
    -> impl Future<Output = Result<Vec<SelectedFile>, PickError>> + Send;

UploadUpdate::Prepared {
    account: UserId,
    channel: ChannelId,
    request: RequestId,
    result: Result<Vec<PendingAttachment>, UploadError>,
}
```

`SelectedFile` sigue siendo el dueño individual de File/Metadata privados,
sin Clone ni Serialize. No se añadió un wrapper de lote ni se transportan
descriptores, rutas o buffers en los resultados de UI. `SendFiles`, pendientes,
retry y borradores ya contienen vectores.

### Plataforma

- `OpenFile` usa `multiple = max_files > 1`, con el mismo owner, sender,
  path, handle_token, guard RAII y validación del handle anterior a publicar.
  Título singular/plural según la selección permitida.
- `response_uris(message, max_files)` conserva 64 KiB de respuesta, 4096 bytes
  por URI y los límites existentes de RPC, diálogo, limpieza y conexión.
  Acepta entre uno y `max_files` elementos; cero es respuesta inválida y el
  exceso da `PickError::TooMany`.
- Todas las URIs se validan antes de I/O con el parser estricto: esquemas/hosts
  locales, escapes, controles y segmentos punto. Se rechazan separadores `%2F`
  que podrían ocultar segmentos y rutas decodificadas repetidas, sin deduplicar
  o truncar silenciosamente.
  Dos rutas distintas que sean hardlinks no son por sí solas un error.
- El job y resultado del FileWorker son vectores; abre secuencialmente y
  valida archivo regular, tamaño individual y suma comprobada. Un error cierra
  el lote entero. No crea workers, spawn_blocking ni requests por archivo.
- `max_total_bytes == 0` puede admitir sólo archivos vacíos; no confundirlo
  con `max_file_bytes == 0` o `max_files == 0`, que son solicitud inválida.
  El portal no impone el límite de cantidad ni bytes: RUNE debe comprobarlos.

### Core y presupuesto

- Se mantienen ocho entradas y 32 MiB globales. La capacidad del picker se
  calcula con el espacio del borrador actual, slots y leases disponibles.
  En el registro,
  los tokens listos del canal no reclamados por `send.tokens` corresponden
  al borrador; los archivos de un envío pendiente no deben contarse como
  otro borrador del mismo mensaje.
- `PickControl.token` inicia la reserva de hasta ocho tokens. next_token
  avanza con checked_add(capacity) antes del job; base+índice también se
  comprueba. Los tokens no utilizados no se reciclan. Se conserva correlación
  sin otro allocator.
- PrepareWorker recibe un único job de lote: cada input posee descriptor,
  metadata, token y lease. `reserve_many(sizes: &[usize])` reserva suma de bytes
  y entradas bajo un solo lock antes de leer; el rechazo no consume crédito.
- Cada buffer sellado posee su propia lease, retenida por todos sus
  clones/slices. Una lease global del lote retrasaría innecesariamente la
  liberación de los archivos ya descartados. Los créditos retenidos por un
  transporte siguen contando aunque el token desaparezca del registro.
- La lectura es secuencial y conserva las comprobaciones fstat de identidad,
  tamaño, mtime/ctime antes y después. Si falla el segundo archivo, descarta
  también el primero preparado; no publica un éxito parcial. El lote no
  representa una instantánea simultánea del filesystem: cada buffer queda
  congelado al terminar su lectura validada.
- Scope, permisos, cancelación y límites se revalidan antes de insertar todo
  el resultado en registro/control y publicar Prepared. Si falta un token en
  poll, se liberan los restantes y se devuelve error; no se entrega un subset.
- Cancelar no interrumpe una syscall. El receiver/slot y las leases se conservan
  hasta drenarla; la siguiente preparación espera los mismos workers. Los
  vectores y buffers se descartan en servicios. La UI conserva la cancelación
  hasta su terminal; cambiar cuenta devuelve un error del ámbito original sin
  suprimir ese terminal ni publicar metadatos cruzados.

### Provider, State y UI

- Política implementada de Mock: ocho archivos por mensaje, 20 MiB por archivo
  y 25 MiB por mensaje incluido texto, etiquetada como simulación. La capacidad
  se elevó tras GREEN; límites/transporte reales siguen deshabilitados.
- Mock ya calcula digest sobre cantidad, orden, nombres, tamaños y todos los
  cuerpos. Rechaza UploadToken repetido y nueve tokens distintos con TooMany.
  Progreso agregado: inicio y suma final, sin
  confirmar el mensaje sólo por completar el cuerpo.
- ACK múltiple requiere MessageId válido, Delivery::Sent, tipo adecuado,
  cantidad/metadatos esperados y AttachmentId remotos no cero y distintos.
  El proveedor debe preservar el orden del envío congelado para el contrato
  actual de comparación. Un resultado incompatible conserva el envío Unknown.
- Retry reutiliza nonce y lote íntegro; un request nuevo identifica el intento.
  No puede reenviar sólo los archivos restantes ni sólo el texto. ACK positivo
  tardío válido puede resolver el mismo envío lógico; fallos y progreso siguen
  correlacionados con el intento.
- La UI valida todo el vector contra el borrador vigente y añade el lote con
  una revisión. Ante callback obsoleto, revocación o exceso, devuelve todos los
  tokens nuevos. Mantiene texto, respuesta y archivos anteriores. Ok([]) no es
  una preparación exitosa. Elegir prepara; enviar requiere una acción posterior.
- Quitar antes de enviar actúa por token; cancelar durante el envío actúa
  sobre el mensaje completo. La lista de preparados usa ScrollArea con altura
  acotada para mantener visible el editor con ocho elementos. El pin de limn
  no se modificó para esta porción.

### Evidencia local y nativa de 07.2

Las pruebas cubren dos archivos con Response anterior al reply, handle
incorrecto sin grants, URI inválida al final, duplicados, exceso de cantidad
y suma, segundo archivo modificado, cancelación entre lecturas, clones y
rollback completo, callback obsoleto, borrador anterior intacto, file-only
múltiple, retry sin duplicación, segundo payload cambiado e IDs ACK duplicados.

El workspace pasó **1073 pruebas**, seis ignoradas y 25 suites con todas las
features, formato y Clippy estricto. Release: 10 crates en 49,72 s; ejemplos:
cuatro en 7,21 s. Seis replays REA terminaron con salida 0 y terminal completo:
Core 161 aprobadas/una ignorada, State 109, UI 307, Provider 54, Platform 45 y
Desktop 33. El [registro](../docs/evidence/uploads-multiple-checkpoint-2026-10-15.json)
conserva ocho binarios y sus hashes. El ejemplo nativo entregó dos descriptores
de 48 B con dispositivo/inodo coincidentes. Las capturas limn de
[preparación](../docs/img/upload-multiple-prepared-2026-10-15.png) y
[confirmación](../docs/img/upload-multiple-sent-2026-10-15.png) muestran borrador
intacto y luego un mensaje Mock con ambos adjuntos y editor vacío. Los intentos
01/03 permanecen separados de los finales 02/04/05. El bundle conserva 55
registros y cuatro incógnitas; las capturas 07.1 siguen siendo históricas.

## 07.3: arrastre y portapapeles

Son entradas distintas. Comparten el servicio de preparación y envío de 07.2,
sin duplicar registro, leases, workers ni máquina de retry. Soltar o pegar
archivos sólo prepara el borrador. No confundir selección múltiple validada
con soporte completo de estos recorridos.

### Investigación inicial del pin histórico 07.2

Se inspeccionó con git show el commit de Cargo.lock de 07.2
`2618b37d1c17b73ef70c43e67f5641ec4da41a3a`. Las carencias de Composer y del
orden de archivos descritas aquí se resolvieron después mediante la PR #49;
no describen el snapshot posterior 07.3.1 ni el pin actual de 07.3.2.

- limn tree.rs:2333/2338 expone FilesHovered y FilesDropped con paths;
  tree.rs:2508/2516 devuelve target y paths en UiAction. El backend
  limn-egui:1877–1888 traduce los paths nativos sin leer sus bytes.
- `Element::drop_target()` existe. Sólo registra elementos con región/ID;
  aplicarlo al panel que devuelve Composer no marca su TextArea interna.
  Composer no ofrece actualmente una propiedad para hacerlo.
- drag_drop_tests.rs tiene una prueba de archivos externos y targets anidados.
  No prueba drop a través de un modal ni un lote nativo en el bridge.
- El bridge necesita pointer.latest_pos; si falta, no emite FilesDropped.
  Su comentario reconoce que la posición puede llegar atrasada. No sustituir
  una posición desconocida por el canal actual o un target inventado.
- Paste/PasteTo y PasteRequested existen para texto y foco del editor.
  egui::Event::Paste sólo contiene String. egui-winit:1027–1034 consume el
  shortcut, lee texto y retorna aun cuando no pudo obtenerlo. Un portapapeles
  que contiene sólo archivos puede no producir evento para limn.
- arboard 3.6.1 sí expone get().file_list() y get_image(); egui-winit usa
  get_text(). Su clipboard Wayland smithay 0.7.3 admite sólo tres MIME de
  texto. El backend opcional wl-clipboard-rs de arboard no figura en este lock.

### 07.3.1: arrastre de archivos implementado

Esta sección conserva el prototipo local y la API Core probada. El ingreso
desde host se sustituyó por el rechazo 07.3.2 descrito abajo; no es una acción
actual habilitada por haber pasado las pruebas sintéticas.

`App::handle_input` en apps/desktop/src/main.rs llama limn_egui::events,
layout e intercept; luego recorre tree.handle_event(event). El prototipo consumía
UiAction::FilesDropped en ese bucle, antes de rune_ui::update, junto al
manejo de CopyRequested. Movía los paths al servicio, sin guardarlos
en Ui, Store o Intents. Revalidaba target del editor del canal actual, cuenta,
capacidades, permisos, cierre pendiente, modales/menús y preparación activa.

El prototipo `raw_input_hook` rechazaba por completo una lista de metadata que
excedía ocho paths, 4096 bytes por path o un total de 32 KiB antes de que el
bridge copiara paths. No se truncaba para preparar los primeros ocho. Winit acumula
DroppedFile individuales en RawInput: confirmar en QA nativa que un gesto
múltiple llega como un lote; no prometer esa agrupación en todos los sistemas.

APIs implementadas:

```rust
Client::prepare_attachment_paths(request, channel, paths: Vec<PathBuf>)
    -> Result<(), UploadError>;
rune_ui::begin_attachment_import(ui, store, channel, source)
    -> Result<RequestId, UploadError>;

validate_local_paths(paths: &[PathBuf], max_files: usize)
    -> Result<(), PickError>;
open_local_files(paths: &[PathBuf], max_files: usize,
                 max_file_bytes: u64, max_total_bytes: u64,
                 cancel: &CancelSignal) -> Result<Vec<SelectedFile>, PickError>;
FileChooser::wait_idle(&mut self, CancelSignal)
    -> impl Future<Output = Result<(), PickError>> + Send;
```

La UI sólo inicia su estado con source pequeño y recibe Prepared(Vec). El
actor usa Picker/Paths y converge en la misma preparación; Clipboard no está
implementado. La admisión de Paths valida el lote sin I/O y acota las capacidades
de memoria retenidas. El worker fijo Core usa la primitiva bloqueante de
plataforma para abrir, reservar todas las leases y leer el lote. Drena primero
el trabajo previo del preparador y del selector existente; si no había selector
no crea una conexión portal. `wait_idle` sólo drena, sin renovar ni conectar.
Un fallo o cancelación no publica un subset. No se llama DroppedFile::bytes():
la implementación nativa de esa API hace fs::read sin
los límites/leases y sin conservar el descriptor seleccionado.

El bridge actual entrega hover/drop después de la entrada ordinaria del cuadro.
El prototipo reconstruía la vista antes de resolver el destino: un Escape o
cambio de canal previo impedía importar al editor viejo. egui separa la metadata
de archivos del teclado; este orden no es una cronología del SO. La barrera de
`Submitted` evitaba enviar texto o adjuntos anteriores antes de preparar el
nuevo lote, también en accesibilidad. El host actual conserva esa barrera para
los drops rechazados, junto con Shift+Enter y acciones de otros controles.

Verificación dirigida: **50 Core uploads + 6 host**, UI 6, plataforma 54 y
componente 1. Los cuatro recorridos Core cubren congelado hasta envío explícito,
rechazo sin consumir IDs, fallo del segundo archivo con recuperación y el mismo
slot Busy/cancel/drain para Picker y Paths. El host cubre destino vigente,
overlays/cierre, exceso completo y Enter del mismo cuadro sin consumir borrador
ni publicar mensajes en Mock. La suite integral de 07.3.1 pasó **1098 pruebas**,
seis ignoradas y 25 suites en 5,47 s con todas las features; formato y Clippy
estricto de todos los targets/features correctos. Release de ocho crates:
51,13 s. El [registro local 07.3.1](../docs/evidence/uploads-drop-checkpoint-2026-10-15.json)
retiene seis binarios y 223 archivos fuente en `1ad33eb`, con cinco replays REA
canónicos: 670 aprobadas/una ignorada, salida 0 sin truncar. Los intentos
truncados permanecen separados. La verificación nativa no cerró: dos gestos con
fixtures propios no prepararon adjuntos, no se entregó Enter/envío y no hay PNG
nativo aceptado. El bundle conserva 65 registros y cuatro incógnitas.

### PR #49 de limn y límite upstream

La [PR #49](https://github.com/Niiihuel/limn/pull/49) ya se implementó en su
worktree y está abierta: commit `a1a51af9c593d7e633ef7c9d75b78c1813137708`,
cherry-pick inicial en integración `b91772bbb486291a5e0d272987b83aa04ef22971`.
El segundo commit `6b83daf3e3130649484e82946ea3201295ce59ee` corrige el orden
del bridge, cherry-pick en el **pin histórico de 07.3.1**
`1ad33eb224b6b4fb48de9423b23a89d6230e087f`.
Sus regresiones y galería verifican:

1. Opt-in `Composer::drop_target()` sobre su editor habilitado, usando el ID
   existente. Editor deshabilitado sin target; sin crear un foco/Clickable extra.
2. Drop/hover respetan superficies flotantes y menús contextuales; un modal
   impide alcanzar el fondo incluso fuera de la tarjeta y permite su destino
   interior. Estas regresiones fallaron antes del guard.
3. El bridge conserva dos rutas con origen desplazado, no lee bytes ni inventa
   destino sin posición. La galería retiene sólo el conteo, con documentación,
   changelog y catálogo regenerado.
4. El bridge entrega primero PointerMove/Escape y demás entrada ordinaria,
   luego los archivos. La prueba de regresión reprodujo el orden anterior.
   El wrapper de touch puede añadir acciones derivadas después; RUNE usa
   `events` directamente y no afirma cobertura nueva de ese wrapper.

La integración limn de esa etapa pasó 1405 pruebas, dos ignoradas y 13 suites en 0,92 s, más Clippy
estricto. Eso no prueba arrastre real del SO. El checkpoint 07.2 retiene
`2618b37`; el snapshot 07.3.1 consume `1ad33eb` y pasó las pruebas dirigidas descritas
arriba e integrales. Su recorrido nativo sigue abierto por el backend. Ver
[frontera y estado](../docs/LIMN_INTEGRATION.md#composer-como-destino-de-archivos-pr-49-2026-10-15).

### Bloqueo nativo y compatibilidad de dependencias

El stack fijado es eframe/egui/egui-winit **0.36.2** y winit **0.30.13**.
El backend winit Wayland no produce eventos DnD de archivos en esa versión;
X11 no expone `XdndPosition`. Las pruebas sintéticas de RawInput verifican el recorrido
posterior, no crean los eventos ausentes del SO. No se sustituye posición por
el foco, el canal actual, un puntero global ni otra conexión Wayland.

La [PR #4571 de winit](https://github.com/rust-windowing/winit/pull/4571),
fusionada el 2026-07-16, introduce `DataTransfer`, posición y transferencia de
datos con una API distinta en 0.31 beta. [egui main](https://github.com/emilk/egui/blob/main/Cargo.toml)
sigue usando 0.30.13; la última versión publicada es 0.36.2. La restricción
`^0.30.13` excluye 0.31, por lo que cambiar una dependencia de limn o añadir
otro winit no migra el backend de eframe.

La [PR #7731 de egui](https://github.com/emilk/egui/pull/7731) está abierta/en
borrador: HEAD `e285050` sobre egui 0.33.2 y winit 0.31.0-beta.2, con handlers
de archivos comentados, fallos CI y trabajo funcional pendiente declarado por
su autor. No es un pin operativo listo para RUNE. Migrar esa vía requiere
completar egui-winit/eframe upstream y validar su API DnD; no es la única
alternativa Wayland. Un backport a 0.30 sería
otro trabajo coordinado de backend/protocolo; no se crea un fork ni parche
silencioso en RUNE. La porción nativa 07.3.1 continúa abierta.

### Alternativa Wayland existente: inspección estática de limn-dnd

El crate ya está en los fuentes git de `2618b37` y `1ad33eb`, con el mismo
árbol `5d357bea…` que main `a3ce02a` del 2026-10-15. La ausencia es de
integración: RUNE no declara esa dependencia y `limn-egui` no lo consume. Adjunta una cola
de eventos al **display y superficie de la ventana existente**, no a otra
conexión con una superficie ajena. Recibe coordenadas de la oferta Wayland y
su lista URI, por lo que podría evitar un fork/migración de winit para ese
backend. Es fuente implementada, **no un drop nativo verificado en RUNE**.
El [añadido estático](../docs/evidence/limn-dnd-existing-backend-audit-2026-10-15.json)
conserva rutas, hashes, SDK inspeccionado y límites; no es evidencia REA runtime.

La inspección propuso un incremento upstream receive-only, previo a conectar
el host: Copy sin Move, oferta exacta después de validar, identidad y
cancelación, colas/lector acotados, plazos y lote completo o error. El backend
legacy inspeccionado finalizaba antes de la admisión y usaba una oferta global,
un hilo por drop y colas sin límite. El nuevo `FileDropReceiver` implementa
esas garantías en la [PR #55](https://github.com/Niiihuel/limn/pull/55),
**Draft abierta**, con dueño retenido y cierre sin join en UI. En el SDK cacheado,
winit WindowState conserva una Connection, pero un `Arc<Window>` aislado no
acredita el despacho y la limpieza del backend extranjero: se exige un acuerdo
de cierre y pruebas de vida del display/surface a través del event loop.

Después corresponderían integración de coordenadas/DPI/zoom, permisos,
overlays, navegación/cuenta y cancelación, y QA entrante con fixtures propios.
Las pruebas del backend inspeccionado cubrían URI y conexión/rechazo local;
el test vivo retorna si falta compositor. Nada de esta propuesta habilita drops legacy,
envío automático, proveedores reales ni X11/Windows/macOS. Los checkpoints
históricos y el rechazo actual conservan su alcance.

El [contrato de FileDropReceiver](drop-receiver-contract.md) concreta ese
alcance receive-only, con límites, protocolo Copy, ciclo de vida y siete grupos
RED/GREEN. **Implementación upstream publicada como Draft; integración RUNE
pendiente** en la [PR #55](https://github.com/Niiihuel/limn/pull/55), head
`73382702c5a1137ba2cfcbad5d5ebc2585697e6a` (biblioteca
`0f8a74c47a27620cf322434af5bf9d8cb6fb23ac`). El
[check CI](https://github.com/Niiihuel/limn/actions/runs/38772048890/job/116550099462)
terminó en SUCCESS el 2026-10-15 a las 16:50:38 UTC; esa observación posterior
no implica merge ni actualización de RUNE. En ese incremento,
codec, buzón, coordinador, control, pipe, arranque/ofertas/transferencia guest,
pool/reaper, admisión legacy, worker real y API pública están conectados y
validados contra el servidor Wayland privado. Tras corregir el layout de la
demo, `cargo test --workspace --offline` pasó **1646 pruebas, cinco ignoradas
y 21 suites en 6,36 s**, con salida 0. Este total de **limn incluye doctests**;
no se le suman por separado las siete pruebas de documentación. Incluye las
**124 pruebas de `limn-dnd`** y **tres de la demo Gallery**. Clippy estricto del
workspace y formato de los archivos propios volvieron a pasar tras el ajuste
(`c7df75`); el build de la demo también pasó. La fuente formateada de la demo
tiene SHA-256 `5f171991a3942fee1910c4a40097ed92bef778f7e05444817bec7f183e879f68`.

El snapshot anterior, `source-receiver-124-green.tar.gz`, conserva su resultado
histórico de **1637 pruebas, cinco ignoradas y 18 suites en 6,21 s**, con una
prueba Gallery entonces. Su documentación pasó siete doctests en cinco suites
en 0,21 s; también pasaron formato de los archivos propios y rustdoc estricto.
El binario de biblioteca retenido repitió las **124 pruebas sin ignoradas ni
filtros en 0,71 s** mediante REA, con código y señal de salida 0 y sin
truncamiento. Estos resultados son distintos de las **1120 pruebas y 18
capturas Mock** del checkpoint RUNE 07.3.2, cuyo pin era `22e0fd93`. Los
hitos de 110 pruebas, 113 más un doctest y 121 más dos doctests son
verificaciones históricas de etapas más acotadas, no totales actuales.

El servidor Wayland privado verifica identidad por proxy/device/seat, ocho
ofertas totales, Copy confirmado y punto congelado. Started precede a Receive;
EOF y validación íntegra preceden a Ready; sólo Accept vigente intenta Finish
tipado sobre esa oferta exacta. Cancelar, revocar Copy o vencer el plazo antes
del claim impiden Ready/Finish posteriores. Un claim ya ganado conserva su
único intento autorizado. Se reprodujeron y corrigieron relojes viejos antes
de GetRegistry, Receive, publicación de Ready y claim. El plazo de decisión
parte del instante fresco en que se publica Ready.

El pool probado se importó desde un worktree aislado: cuatro slots compartidos,
un reaper fijo, join real incluyendo TLS, owner retenido hasta cleanup y
descarte fuera de UI, y 64 créditos de registry sin devolución. Un fallo fatal
del reaper deja los owned en Closing y rechaza nuevos workers; los callbacks
finales fallidos se aíslan. La conexión del backend legacy a la reserva y al
presupuesto está incluida en la suite conjunta: `FileDnd::attach` es unsafe y
conserva su Drop/join síncrono, sin endurecer sus lectores por drop. El worker
receive-only usa el pool, notificaciones de control, readiness del pipe y tick
guest; no añade otro lector del display. La fachada pública valida límites
antes de consultar handles, conserva el owner y delega controles sin join en UI.
Las regresiones posteriores cubren el cierre normal y Action recibido antes
de Enter, conservando Copy hasta la decisión explícita.

La demo limn está compilada y **QA1 y QA2 se ejecutaron con resultado negativo
en el compositor actual**. Alcanzó `ReceiveStatus::Ready`, pero los gestos no
produjeron `DropStarted`, `DropReady` ni metadatos. Se corrigió el target que
no registraba una región de hit testing, con prueba RED/GREEN de layout real.
El segundo ensayo reveló un bloqueo adicional: el device 135 del clipboard
se registró 9,969 ms antes que el guest 145 y recibió Enter/Motion/Leave;
el guest no los recibió, aunque el origen emitió `start_drag`. El código de
salida 0 no acredita recepción. La sección de coexistencia del
[contrato](drop-receiver-contract.md#prerrequisito-de-coexistencia-con-el-clipboard-del-host)
exige inyección del SDK antes de Clipboard, owner único por cliente/seat,
routing por superficie y preservación de Selection/texto/Primary, con pruebas
en ambos órdenes de registro. No se asume broadcast ni violación del protocolo.
En **ROOT3, con Weston privado anidado**, sí llegaron `DropStarted` y
`DropReady` de la misma oferta, seguidos de los dos nombres propios
`rune-arrastre-a.txt` y `rune-arrastre-b.txt`. Terminó en
`DropEnded(Err(Cancelled))`: al crecer detalles/log, la barra del scroll externo
redujo 14 px el ancho del target y la validación de geometría cortó el contexto.
No se admitió Accept ni Reject ni se acreditó Finish. La captura REA
[`ev_c9d0d97e51980f789da64b2618624fdbc25a727b4e414ba43b521b9b009eaed4`](</tmp/limn-drop-nested-native-20261015-prv7xlxv/native-rea.json>)
terminó por timeout con SIGTERM, no por cierre normal; ese desenlace no acredita
cleanup nativo completo. El RED de layout reprodujo el salto en 700×760 y
980×800 (dos fallos, recuerdo RTK `890c6fe92c56`). El arreglo dejó header,
target y cierre fijos, con detalles/nombres/log en scroll interno; pasaron las
tres pruebas de Gallery en 0,01 s (`a90b27`) sin relajar geometría ni epochs.

**ROOT4 positiva, limitada a Weston privado anidado.** Con la demo corregida,
la oferta 1 produjo Started→Ready con dos nombres y, tras clic nativo explícito
en Accept Copy, `DropEnded(Ok(()))`. La oferta 2 repitió Started→Ready con los
mismos dos fixtures y, tras Reject explícito, terminó en Cancelled. El
[stdout retenido](</tmp/limn-drop-nested-native-20261015-8ty058fq/demo.stdout>)
conserva ambos recorridos. Después se activó Close con Tab→Space; el
[runner registró salida normal 0 de la demo](</tmp/limn-drop-nested-native-20261015-8ty058fq/runner-exit.json>)
y sólo entonces terminó el origen y Weston mediante pidfd/SIGTERM. No se
inyectó Enter, la demo no abrió contenido de archivos y no intervino RUNE. Fue una
sesión CLI nativa observada, sin ID REA; no se la presenta como replay REA.
La fuente fue la demo `5f171991` ya probada, sin un cambio posterior de código.
Ended Ok acredita únicamente el resultado Ok del intento local tipado de
Finish, sin ACK del compositor ni preparación/envío de archivos.

Este resultado no resuelve la interoperabilidad con el clipboard del host
Hyprland observada en QA2 ni acredita soporte universal o clipboard funcional.
ROOT3 permanece como ensayo parcial cancelado por layout y timeout. El
[registro canónico upstream/Weston](../docs/evidence/limn-drop-receiver-weston-2026-10-15.json)
conserva el informe CLI congelado, trazas, capturas, source archive y límites;
no convierte esa sesión en REA ni afirma ausencia de diagnósticos. La PR #55
ya es pública como Draft; faltan resolver el owner SDK compartido, integrar
la revisión/pin aprobado y conectar el consumidor RUNE. El
ingreso DnD de RUNE sigue abierto; no cambia el cierre negativo 07.3.2 ni se
habilita el ingreso legacy. Al cerrar esa etapa, RUNE conservaba 1120 pruebas,
pin `22e0fd93` y 18
capturas Mock; la demo nativa no aumenta ese inventario.

**SDK#1 local completado, sin integración RUNE.** El commit egui
`ac2ff2fec18c637a8841bcd4b626a98de3324d72` implementa `ClipboardBackend: Send`,
`Clipboard::from_backend`, `State::new_with_clipboard` sin display-target y
`epaint::Mutex::get_mut`, con texto/imagen enviados al backend custom y None
sin fallback. Se verificaron 11 pruebas mutex y la matriz egui-winit de cinco
x11 sin defaults, tres default y cinco wayland sin defaults, más Clippy/docs
estrictos y formato; no se suman como pruebas únicas ni a las 1120 de RUNE.
La [evidencia SDK#1](../docs/evidence/egui-clipboard-injection-2026-10-15.json)
conserva hashes y checks; la pérdida de UnwindSafe del Clipboard poseído queda
explícita. Son pruebas sintéticas, sin clipboard del SO ni QA nativa.
**SDK#2 local completado, con QA nativa sintética acotada.** El commit hijo
`df0b452d09c363c5e5ec7f7a45a69c37fd4fdf4c` implementa la fábrica eframe para
WGPU/Glow, error raíz propagado, latch por instancia deferred y fallback
immediate explícito a EmbeddedWindow usando input/clipboard del padre, sin
backend default nuevo. Pasaron 54 pruebas egui (cuatro Context) y la matriz
eframe nueve Glow+x11 sin defaults/nueve WGPU default, más Clippy/docs estrictos
y formato final. Esa matriz repite configuraciones y precedió sólo un ajuste
del harness negativo Context, después reverificado; no suma pruebas a RUNE.
La [evidencia SDK#2](../docs/evidence/egui-viewport-clipboard-factory-2026-10-15.json)
conserva ocho fuentes exactas y los checks. Los nuevos campos públicos requieren
adaptar literales exhaustivos de NativeOptions e ImmediateViewport.
La [QA nativa posterior](../docs/evidence/egui-clipboard-factory-native-2026-10-15.json)
verificó fábrica ROOT/deferred/immediate y error immediate embebido con backend
sintético en Weston headless privado: WGPU exit 0 en 3,29 s y Glow exit 0 en
3,39 s, mismo binario, hijos y raíz cerrados. El primer WGPU kiosk queda parcial
por watchdog de 25 s, no como éxito; la causa de scheduling es inferida. El
fixture quedó en `0bff2dd4ada2a32b23f611488f3c7e7b99561d32`, con sólo dos líneas
doc ajustadas tras la ejecución. No leyó clipboard del SO ni prueba DnD,
clipboard real, adaptador/hardware, Hyprland, owner compartido o RUNE. Se
conservan diagnósticos y la limitación de que el deadline logic necesita ticks.
**SDK#3 local completado, sólo CPU/API.** El commit
`c152f4b7692da7496a74d524755bb32f74db6805` implementa PasteRequestToken opaco
con replay vacío, Event::PasteRequested, respuestas Immediate/Deferred/Rejected
y un helper State común para Ctrl+V y Paste de WGPU/Glow. Deferred no lee datos;
el getter legacy y la respuesta default siguen síncronos. Conserva las tres
rutas CRLF ya existentes y requiere adaptar matches exhaustivos de Event.
La [evidencia SDK#3](../docs/evidence/egui-deferred-paste-marker-2026-10-15.json)
retiene seis fuentes y 23 archivos: focused 3+6; full egui60 persistence,
egui-winit11/9/11 y eframe9/9 según features, más Clippy/docs/formato estrictos.
No se suman configuraciones como pruebas únicas ni a RUNE. La QA nativa previa
de fábrica no acredita pegado diferido ni I/O de clipboard en este incremento.
El [contrato SDK/owner compartido](wayland-clipboard-host-contract.md) mantenía
en ese corte owner/reserva/ACK/completion como propuestas pendientes. La base
DnD del owner se publicó después, como describe el checkpoint PR #62 de abajo.
A1 posterior acepta sólo metadata Reserve/ACK/claim; en ese corte el servicio
de clipboard y su integración seguían pendientes. Los checkpoints posteriores
se describen abajo. El
destino limn se implementó después, como incremento separado descrito a continuación. Antes del
consumidor se deben ordenar las teclas de host por evento, resolver
Intent::Focus/layout antes del ACK y combinar el target con epochs de RUNE;
no basta el ID del campo ni gates SEND/ATTACH. Los commits SDK siguen locales,
sin push/PR ni cambio de pin o consumidor RUNE; la interfaz principal sigue en
limn; el baseline RUNE de esa etapa fue 1120/18 Mock.

**Checkpoint upstream de destino capturado, previo al cambio de pin RUNE.** Core
`9316d75da8b37b4c0c551e4b53b330b8c012beb0`, HEAD de docs/galería
`eb6ea2998a7332bfe0d8ed26024226c37a015b72`, publicados en la
[PR #56 Draft OPEN](https://github.com/Niiihuel/limn/pull/56), sin merge.
CI SUCCESS observado para ese HEAD el 2026-10-15 a las 19:35:57 UTC; las
observaciones iniciales IN_PROGRESS quedan históricas. La
[evidencia nueva](../docs/evidence/limn-captured-paste-target-2026-10-15.json)
y el [contrato actualizado](captured-paste-target-contract.md) registran
capture/invalidate/PasteCaptured, ticket de un solo uso y EditStateToken opaco.
Validar/editar CodeEditor usa el mismo try_lock; Busy/layout pendiente no
consumen, aceptado vacío/saneado-a-vacío/fallo de inserción sí consumen. IME,
Vim y cortes observados invalidan sin prometer detectar ABA de dominio invisible.

El workspace limn pasó 1571 pruebas, cinco ignoradas, 20 suites en 6,29 s;
los seis grupos Tree y tres Editor están incluidos. La galería tiene tres
pruebas separadas; se ejecutaron ambos snippets completos y pasaron Clippy y
rustdoc estrictos, formato de nuevos/editor/galería y diff check. Tras pulir
sólo colores/fixture se repitieron sus tres pruebas, Clippy, formato y build.
Cuatro variantes dark/light y normales/square terminaron exit 0 y PNG 760×820
revisado por Root en Weston 15.0.1 headless Pixman privado con fake-seat y build
WGPU, sin inspeccionar el adaptador. La demo usa texto sintético y no solicita
clipboard I/O; eframe puede inicializar su default. Su deadline lógico depende
de ticks y las cuatro finales tuvieron watchdog externo de 25 s, no activado.

Se conservan un primer intento sin seat, sin marcador/PNG y con cleanup
posterior, y dos v1 con tofu CJK/títulos light ilegibles. Corregir foreground y
usar `ñ`/`λ` no implementa CJK. La revisión independiente cerró sin hallazgos
materiales. Esta galería no prueba clipboard compartido, ACK, orden nativo ni
RUNE; sus PNG no incrementan las 18 capturas Mock. Owner, completion, foco
nativo/orden del host, pin y consumidor RUNE quedaron pendientes en aquella
etapa; la porción posterior del host se documenta abajo.

**Checkpoint upstream de entrada indexada, previo al consumidor RUNE.** Commit
`b52284a0d94b090784139d8be874f63b9694b48d`,
[PR #59 Draft OPEN](https://github.com/Niiihuel/limn/pull/59), apilada sobre
`feat/captured-paste-target` (PR #56); CI SUCCESS observado a las 20:01:58 UTC,
sin merge. Los dos snapshots iniciales IN_PROGRESS quedan históricos.
El [contrato actualizado](limn-egui-ordered-input-contract.md) y la
[evidencia nueva](../docs/evidence/limn-indexed-input-2026-10-15.json) separan
API RED sin cuerpos ejecutados de RED funcional 3pass/1fail/62filtered: el
converter whole-frame omitía un newline anterior a Zoom/Enter. GREEN focused:
cuatro grupos/62filtered. `events` es la proyección de un único converter
`events_indexed`; Some(i) identifica el snapshot normalizado del viewport,
None no inventa orden para derivados y el host recorre incluso desconocidos.
La deduplicación reconoce sólo pares contiguos no solapados; se conservan los
límites whole-frame de puntero/click_count/modifiers documentados.

El workspace limn pasó 1575 pruebas, cinco ignoradas existentes, 20 suites en
6,61 s antes de la compactación visual final del modo ordered. Después pasaron
Gallery 3 originales byte-idénticas y Clippy estricto workspace/Gallery hot,
rustdoc estricto, formato nuevo, build y diff check. La galería reutiliza el
host capturado mediante `--ordered-input`: navega antes del marcador para
entregar a B, o captura A antes de navegar para rechazar la completion tardía.
Muestra dos históricos reales y los paneles con el estado final de la segunda
secuencia, sobre la misma Tree/controlador, sin rebind oculto ni token/ACK SDK.

Root revisó siete runs seriales exit 0/PNG/cleanup propio en Weston privado
headless Pixman con fake-seat y build WGPU. Cuatro ordered dark/light y
normal/square de 760×820 muestran ambos históricos completos; dos small de 600×580
muestran ScrollArea/footer, con históricos bajo el fold y **sin gesto de scroll
probado**. Legacy-dark tiene PNG byte-idéntico al parent final dark/default.
La revisión visual posterior se conserva aparte de visual_reviewed:false en
los runners. Watchdog externo de 25 s no se activó; el deadline lógico requiere
ticks y se retienen los warnings EGL/MESA. Es simulación de batches/texto, sin
solicitar clipboard I/O (eframe puede inicializar su default), sin acreditar
cronología física, owner/ACK, Hyprland ni consumidor RUNE. Ese checkpoint
conservó 1120/18 Mock/pin22e. El host orden/foco se incorporó después, como se
describe abajo; la integración de texto/completion y pegado diferido en RUNE
siguen pendientes. A1 de metadata/admisión y A2 de body/completion internos posteriores
tienen sus propios checkpoints abajo. B/C posterior verifica el consumidor
permanente real para cinco recorridos internos; la fachada pública y matriz
adicional tienen un checkpoint posterior separado abajo.

### 07.3: consumidor RUNE ordenado y switcher controlado

Pin actual `6ae42859f86f5d255e107a23142aca3c19c86590`, rama propia publicada
`rune/ordered-input-integration`, merge de `22e0fd93` y `b52284a0`. Conserva Composer/Column y el
converter indexado, con derivados None al inicio en lugar del antiguo orden
artificial after-input. No equivale a merge público upstream ni habilita archivos
legacy; rechazo y barrera drop+Enter permanecen.

El host procesa cada fuente Key/foco por índice, incluso eventos sin traducción,
y resuelve Intents/layout/foco entre prefijos. Un foco inexistente limpia el
anterior; callbacks WindowFocused y fallback eventless actualizan UiTree. El
switcher comparte SetInput vacío→Focus para no conservar texto de Tree al
reiniciar Ui. El [checkpoint nuevo](../docs/evidence/rune-indexed-input-host-2026-10-15.json)
registra 1128 pruebas RUNE, seis ignoradas, 25 suites/5,77 s sumadas (wall 11,027 s),
fmt 0, Clippy estricto all-targets/all-features 0 y build 0. Los ocho grupos App
están incluidos: fixture E0560 sin cuerpos no es RED funcional; RED 0/6→GREEN 6,
foco fallback RED 6/1→GREEN 7 y switcher RED 7/1→GREEN 8. La integración limn pasó
1595/5/21 por separado. Los 1120/1127 intermedios quedan históricos, no se suman.

QA nativa propia Mock/Xvfb, con secuencias pausadas: primer run 0 fue parcial por
query `generalQA-switcher-20261015`; tras el fix, el final 0 muestra QA-A en general,
QA-B en ayuda, retorno general conserva sólo QA-A y switcher exactamente
QA-switcher sin consulta vieja. Cinco PNG de cada run, revisados por Root,
quedan separados de las 18 capturas Mock canónicas. Runner visual/product false
se conserva; revisión visual y REA son registros distintos. REA final exit 0 y
signal 0 sin truncar: 22 muestras no exhaustivas, FS/network desconocidos; runner
acredita cleanup propio/cookie eliminada. No prueba batching físico, clipboard,
owner/ACK ni cuenta/proveedor reales. Esta porción del host queda cerrada;
el pegado diferido y DnD funcional siguen pendientes, con 07.3 abierto.

### 07.3: biblioteca owner DnD multiventana publicada, integración pendiente

La [PR #62 Draft abierta](https://github.com/Niiihuel/limn/pull/62), HEAD
`06a08883634c5e5c2abe5afb4a5d624a68e98aef`, se apila sobre PR #55/base
`73382702c5a1137ba2cfcbad5d5ebc2585697e6a`, sin merge. Publica
`WaylandDataOwner` y `FileDropEndpoint`: un owner/worker por display, un
`wl_data_device` por seat compatible y endpoints sobre superficies de esa
misma conexión. Comparte cuatro permisos worker y cuatro superficie globales;
la exclusión de display evita un segundo backend limn paralelo. Detach local
no equivale al join del owner; EOF conserva el lease hasta terminal drenado o
retirada completa, sin que un acuse viejo libere otro drop.
[Contrato](wayland-data-owner-contract.md) y
[canónica v2](../docs/evidence/limn-shared-wayland-owner-2026-10-15-v2.json).

Pasaron las 142 pruebas DnD, incluidas en 1664 de workspace, cinco ignoradas
preexistentes, 21 suites/6,74 s de tests/28,473 s wall. La alternativa
`limn --no-default-features` pasó 1331, dos ignoradas preexistentes, 11 suites/
2,07 s de tests/33,784 s wall; no se suman configuraciones. Clippy estricto
workspace/all-targets y Gallery hot, rustdoc estricto y formato DnD pasaron.
El check CI del HEAD exacto terminó SUCCESS el 2026-10-15 a las 22:06:00 UTC;
la observación inicial en progreso queda retenida. El catálogo 93 y su test de
sincronía pasaron; no hay ignores nuevos, supresiones ni aserciones rebajadas.

Dos PNG dark/light de la página estática de galería pasaron revisión Root,
con salida normal/cleanup propios. Su caption declara simulación: **no adjunta
un owner, no recibe archivos ni solicita lectura/escritura de clipboard**;
el host eframe conserva su backend predeterminado. La QA de presentación no
acredita interoperabilidad multiventana, arreglo de Hyprland ni clipboard del
SDK. La biblioteca DnD destruye Selection y no bindea Primary; no puede
instalarse detrás del clipboard stock ni sustituirlo con un backend no-op.
En ese checkpoint quedaban pendientes Selection/Primary, lecturas/escrituras
acotadas, API de reserva/ACK/completion, SDK y consumidor/pin RUNE. La
metadata privada de reserva/ACK se aceptó después en A1, descrito abajo. RUNE
conserva pin 6ae, SDK stock 0.36.2, 1128 pruebas históricas y 18 capturas Mock canónicas; cinco de 21
flujos cerrados, 07.3 parcial. Ni las pruebas limn ni esos dos PNG aumentan esas
cuentas. Los checkpoints 130/133/136 se conservan con sus RED y correcciones;
la canónica anterior no se reescribe.

### 07.3: checkpoint histórico A1 de admisión; servicio pendiente en ese corte

Sobre la base PR #62 `06a08883`, la rama `feat/shared-wayland-clipboard-text`
implementa `TextClipboardLimits`/`ClipboardError` públicos y la admisión privada
de metadata Reserve/ACK/claim. Parent/SurfaceId son inmutables, los IDs checked
no se reutilizan y un único Incoming excluye DnD/texto. La limpieza exige el ID
exacto; un handle viejo no libera otro request. El [contrato v4 y estado A1](wayland-text-clipboard-contract.md)
conserva el alcance completo y la [canónica A1](../docs/evidence/limn-text-clipboard-admission-2026-10-15.json)
registra **152 pruebas limn-dnd pasadas, cero fallos y cero ignoradas**, después
del formato y check acotados (0,80 s de tests/6,403 s wall). Incluye cuatro de
límites, cinco grupos de admisión y una de runtime nuevas sobre las 142 previas.

La prueba de runtime mantiene Incoming mientras vive el descriptor del arrastre
cancelado, sin detach del endpoint. Se corrigieron dos regresiones DnD con sus
pruebas originales intactas y una fixture MIME que necesitaba una Selection
nueva; los fallos y correcciones quedan retenidos. Revisión independiente sin
hallazgos materiales. En A1 aún no había Clippy/workspace estricto de la rama
incompleta, nuevo commit, push, PR ni pin RUNE.

A1 no retiene Selection/keyboard ni lee texto. En ese checkpoint quedaban A2 y
B/C de protocolo/Receive/lectura reales; A2 se aceptó después como corte interno,
descrito abajo. 07.3 sigue parcial, cinco de 21 flujos cerrados; RUNE conserva
1128 pruebas históricas y 18 capturas Mock. Las 152 limn-dnd no se suman a
PR #62/workspace ni a las matrices RUNE. El historial A1 permanece intacto.

### 07.3: A2 de body/completion internos aceptado (checkpoint histórico)

La [canónica A2](../docs/evidence/limn-text-clipboard-body-2026-10-15.json)
registra un slot fuerte del worker, referencias Weak de control/request y un
único String RAW staged. El finalizador publica sólo bajo la precondición de
watcher retirado; la fixture usa el caso explícito de watcher nunca instalado.
Ese checkpoint A2 no certifica el FD de texto real ni su retiro. Poll revalida reloj y
autoridad, entrega una vez el String exacto (vacío válido, CRLF conservado), con
TTL y error único. Bodies abandonados se destruyen en el worker fuera de todos
los guards antes de liberar Incoming por ID exacto; Drop los vacía antes de
soltar su Arc fuerte permanente; un Arc temporal de UI sólo puede quedar último
con el slot vacío. No finge cleanup nativo ni OwnerClosed.

Tras formato y check acotados pasaron **157 limn-dnd, cero fallos y cero
ignoradas**, 0,80 s de tests/6,583 s wall. Los cinco grupos A2 independientes
pasaron en el primer intento de implementación; aserciones A1 intactas salvo
hook del módulo. RED inicial sólo por API ausente, sin cuerpos ejecutados.
Revisión fresca GO, cero bloqueantes materiales. En A2 aún faltaban checks
estrictos Clippy/workspace, commit, push, PR nuevo y cambio de pin.
Las 157 incluyen las 152 anteriores y no se suman a otras matrices.

El siguiente corte en ese momento era el consumidor permanente B/C en
State/Job/runtime. Se verificó después para cinco recorridos internos, descritos
abajo; la canónica A2 y su alcance quedan intactos. RUNE conserva pin 6ae/SDK
stock, 1128 pruebas/18 capturas Mock y cinco de 21; 07.3 continúa parcial.

### 07.3: checkpoint histórico B/C, candidato v1 interno sin formatear

La [canónica B/C](../docs/evidence/limn-text-clipboard-runtime-2026-10-15.json)
registra el consumidor permanente real en el owner/pool/Job/State/runtime ya
existentes. Retiene Selection/foco de teclado antes de registrar endpoints;
usa fence fresco por SID/epoch, scheduler Sync único, Receive tipado y lector/
decoder UTF-8 acotados. EOF staged no publica hasta el borde posterior al retiro
del Generic/FD real. Selection no emite Accept, Actions ni Finish DnD.

Cinco pruebas nuevas sobre servidor/cliente Wayland privados y watcher real
cubren pre-registro con publicación tras EOF esperando retiro, cancelación tras
staging, cancelación de prefijo abierto sin EOF, orden Done/Enter en un dispatch y fence
vencido esperando root. Pasaron **162 limn-dnd, cero fallos y cero ignoradas**,
0,80 s de tests/3,242 s wall. Incluyen las 157 previas, sin sumar matrices.
Revisión exacta `bc_contract_challenge` GO sin hallazgos materiales. El intento
con filtro incorrecto ejecutó cero cuerpos y queda excluido de prueba funcional.

En ese checkpoint B/C sólo estaba el candidato v1 interno, **sin formatear**, sin commit, push, PR
nuevo ni pin RUNE. En ese corte faltaban formato/checks de entrega y la fachada
estaba en autoría RED independiente. Los helpers `keyboard_capability`/
`selection_none` advertían uso ausente entonces; la matriz posterior ya les da
consumidores genuinos en tests sin supresiones. La fachada pública y parte de
la matriz se aceptaron después; esta canónica B/C no acredita todo v4.

En B/C seguían Primary, CopyText/CopyImage, URI, adaptación SDK, consumidor/pin
RUNE y QA nativa de pegado. Estos cinco recorridos no prueban compositor vivo,
interoperabilidad del portapapeles del SO, Hyprland, cuenta/proveedor conectado
ni uploads remotos. Se mantienen cinco de 21, 07.3 parcial y RUNE 1128/18.
El [contrato v4 y checkpoints](wayland-text-clipboard-contract.md) conserva sus
condiciones normativas; el estado actual del alcance entrante está en184 abajo.

### 07.3: checkpoint histórico público/runtime 174, entonces v4 parcial

La [canónica pública/runtime](../docs/evidence/limn-text-clipboard-public-runtime-2026-10-15.json)
registra attach/reserve y TextPasteRequest públicos implementados y probados,
con vida/Debug redactado/Ping después de soltar guards. No hay join ni Receive
síncrono en wrappers; request no guarda dueño nativo ni body. La revisión de
fuente confirma Ping, aunque el tick puede explicar progreso funcional. RAW
UTF-8/CRLF y EOF vacío se entregan una vez; Cancel/Drop retiran lectores reales
y los handles viejos no afectan B ni retienen ventana/display tras cierre.

Pasaron **174 limn-dnd, cero fallos y cero ignoradas**, 1,76 s de cuerpos/4,18 s
wall: 162 previas, cinco públicas y siete adicionales. Cruce DnD/texto verifica
exclusión en ambos sentidos, incluso terminal sin drenar; Copy aquí es acción
DnD, no CopyText. Cap7 real cubre exact7/UTF-8/CRLF/vacío, inválido, exceso sin
EOF y timeout de 1 s con fuente abierta. Contexto cubre Keyboard perdida sin
Leave con nueva Selection/foco para recuperar, Plain congelado, replace Reading
y NULL Reserved. V2 refuerza Incoming libre⇒Weak FD exacto muerto en ambas
cancelaciones; MIME tardío es adversarial fuera del orden ordinario. V1 queda
retenido, sin debilitar aserciones.

TTL público verifica Published sin poll y rechazo DnD mientras vive: el worker
expira el body y admite B sin poll/cancel A ni nueva Selection; A tardío sólo da
Timeout una vez. La identificación de publicación requiere contexto estable,
baseline wake tras Receive/Generic, wake posterior, FD muerto y Busy junto con
fuente; no es observador público de fase. El rechazo previo al TTL usa una cota
inferior conservadora de su vencimiento. No se instrumentó deallocation de String.

Revisión GO sobre snapshots exactos. RED público sólo API ausente, cero cuerpos;
los grupos pasaron en su primera ejecución y aportan cobertura, no un fallo de
algoritmo previo. En 174 aún faltaban formato/entrega estrictos y commit/push/PR;
seguían en autoría Published/dos seats/epoch/cupo8/9 y pendientes callbacks,
routing viejo, fairness y cleanup incierto. Ambos helpers ya tenían uso genuino,
sin afirmar entonces Clippy estricto. Ese checkpoint privado no acreditaba
compositor vivo y queda histórico; el estado actual es el siguiente.

### 07.3: texto entrante v4, 184 pruebas dirigidas y smoke Weston aceptados

La [canónica nueva de matriz](../docs/evidence/limn-text-clipboard-matrix-2026-10-15.json)
conserva A1/A2/B-C/174 intactas y añade **diez cuerpos**: tres de seats/barrera/cupo,
uno de Published, uno de fairness, uno de cleanup, uno de ReadTag, dos de callbacks
y uno de plazo de lápida. Pasaron **184 limn-dnd,
cero fallos e ignoradas**, 2,64 s de cuerpos/5,140 s wall. Revisión independiente
GO funcional v4, cero bloqueadores y ningún trabajo necesario de implementación
o pruebas dirigidas identificado; sin afirmar cobertura cartesiana.

Barrier v1 conservó dos fallos de coordinación y v2 sólo fortalece la espera por
pump real, sin debilitar aserciones/límites. Fairness tuvo RED de algoritmo
(A=false,B=true frente a true,false) y reparación independiente de nueve líneas
al retirar un seat. Replay ReadTag es privado, con tags/pipes/bytes reales,
sin callback Calloop tardío nativo observado. Lápida Clipboard conserva su plazo
con testigo temporal conservador y decisión Timeout anterior a una renovación;
Closed/join se comprueba después. Cleanup incierto usa fallo sintético del hook:
FD ya muerto e Incoming libre, recursos nativos retenidos en Closing. TTL/Ping y
propiedad combinan testigos reales/fuente, sin instrumentar allocator/destructor.

Pasaron formato final acotado, Clippy estricto workspace/all-targets y Gallery hot,
workspace **1706/0/5** en 21 suites/14,808 s wall, no-default **1331/0/2** de `-p limn`
en 11 suites/35,965 s wall y rustdoc workspace `-D warnings` en 19,834 s wall. Son
configuraciones separadas;184 ya está incluido en workspace 1706. Fallos previos
de Clippy/catálogo/ENOSPC y filtro sin cuerpos quedan retenidos, no se suman.
Cuatro capturas sintéticas de galería dark/light top/end de 1180×820 tienen PASS
visual Root y no son nuevas capturas Mock RUNE ni prueba del clipboard del SO.

Dos smoke de API pública sobre Weston15.0.1 privado/Pixman/fake-seat entregaron
RAW de 29 B UTF-8/emoji/CRLF y vacío de 0 B una vez; Send1 manager antes de attach y Send2
observado tras ACK, poll siguiente None y Detached/Closed con host/source vivos.
App con salida 0, sin watchdog/errores de cleanup y procesos propios fuera. App stderr vacío;
avisos Weston de cursores DnD retenidos. Probe standalone con lock propio de 52 dependencias,
sin afirmar grafo workspace idéntico ni SDK/UI/RUNE/Hyprland. V1 falló sólo por
Display de ReceiveError en el helper; V2 Debug conserva el fallo/lock anteriores.

El commit `d254180819252a653c1d6e27aeb35dc8dc86c591` se publicó en la
[PR #63 Draft](https://github.com/Niiihuel/limn/pull/63), basada en
`feat/shared-wayland-data-owner`; CI del HEAD exacto terminó [SUCCESS](https://github.com/Niiihuel/limn/actions/runs/38832943514/job/116755557558)
a las 01:05:13 UTC del 2026-10-16. PR Draft sin merge. Continúan Primary,
copy/imagen/URI, backend SDK y consumidor/pin
RUNE. Se mantienen pin 6ae, 1128 pruebas históricas, 18 capturas Mock y 5/21; 07.3 sigue
parcial. Ninguna matriz limn ni los dos smoke aumentan esas cuentas de RUNE.

### 07.3.2: rechazo del ingreso legacy actual

El host limpia todos los `RawInput.dropped_files` y `hovered_files` antes del
bridge, sin abrir rutas ni leer contenido. Mantiene los errores de metadata
inválida/excesiva; un lote válido también se rechaza por falta de posición
fiable, indicando usar Adjuntar. El fallback `FilesDropped` sigue la misma
política y no llama `Client::prepare_attachment_paths`. Esa fachada y el worker
Core permanecen probados para una entrada futura; no se infiere destino desde
el puntero global, foco o canal actual. La barrera de Submitted conserva el
borrador ante drop+Enter rechazado.

La regresión funcional reprodujo la admisión indebida; pasaron ocho regresiones
del host y siete de avisos. El pin de ese checkpoint
`22e0fd93c14b3f3c89375135f83c1e9c41307330` incorpora por cherry-pick propio la
[PR #50](https://github.com/Niiihuel/limn/pull/50), abierta sin merge público.
`shrink_height_to_fit()` es un opt-in de altura independiente del contrato de
ancho `shrink_to_fit()`. RUNE lo usa en el Row hijo de la columna de conversación:
aviso corto con altura natural, largo con scroll y editor accesible junto a
reply, ocho archivos y texto multilínea a 720×480. La lista de adjuntos usa una
altura máxima acotada por la ventana. La
[verificación upstream/integración](../docs/LIMN_INTEGRATION.md#avisos-en-flujo-y-altura-opt-in-pr-50-2026-10-15)
se distingue de las pruebas y el pin históricos de PR #49.

**Rechazo negativo y pulido visual 07.3.2 cerrados.** Dolphin soltó dos fixtures
propios de 54/55 B sobre Mock RUNE X11: el aviso completo sobre el editor y
«Borrador conservado» quedaron visibles, sin preparar adjuntos ni inyectar Enter.
La [captura canónica](../docs/img/uploads-drop-native-rejected-2026-10-15.png)
eleva el inventario Mock a 18 (17 históricas y una negativa). El no-envío
automático lo prueban las regresiones del host; el gesto no acredita envío ni
importación funcional. El
[registro final](../docs/evidence/uploads-drop-rejected-checkpoint-2026-10-15.json)
y la [auditoría independiente](../docs/evidence/uploads-drop-rejected-audit-2026-10-15.json)
conservan las fuentes/binarios y separan el intento anterior con editor tapado.
El workspace pasó **1120 pruebas**, seis ignoradas y 25 suites en 5,50 s,
formato y Clippy estricto correctos; release de seis crates en 49,56 s. REA
repitió los tres binarios completos: UI 320 + componentes 101 + host 41,
462 aprobadas sin filtros ni ignoradas, salida 0 sin truncar. El arrastre
nativo funcional, pegado y transporte remoto siguen pendientes.

### 08.1: selector Guardar como verificado, integración pendiente

SaveFile de plataforma ya cerró API y ejemplo local en su snapshot histórico
de 1112 pruebas y pin `1ad33eb`: 64 unitarias y un doctest,
elección del destino exacto sin crearlo y fixture existente intacto. Cancelled
se verificó como terminal, pero el observador tardío no vio una ventana en ese
intento. Las dos franjas GTK de nombre/carpeta no son capturas Mock ni prueban
descarga. [Registro 08.1](../docs/evidence/save-file-picker-checkpoint-2026-10-15.json).
Core/UI y SharedNativeDialogs 08.2 siguen pendientes, igual que reemplazo 08.3;
elegir una ruta no resuelve drop ni habilita proveedores reales.
[Contrato de fases](save-as-contract.md).

Pegar archivos mediante botón o menú explícito puede usar componentes limn
existentes. Ctrl+V de archivos no se arregla añadiendo únicamente un enum a
limn: falta preservar la intención/payload en la integración egui-winit. La
solución completa requiere una extensión upstream que entregue una intención
de pegado incluso sin texto y evite dobles eventos. No usar hooks globales de
teclado ni interpretar texto con apariencia de ruta como permiso de lectura.

El host actual atiende CopyRequested, pero no PasteRequested; la acción del
menú de texto se pierde en rune-ui. Registrar ese caso para una regresión
separada. El callback textual PasteTo debe seguir ligado al editor/foco y
revisión originales si la lectura es asíncrona.

### Backend de portapapeles y límites pendientes

Leer sólo ante una acción explícita y en servicio. Un worker/consulta activa,
cancelación y plazo; no inspección periódica ni lecturas de herramientas sobre
el portapapeles real durante esta investigación.

Para lista de archivos: máximo ocho, 32 KiB de metadata cruda, URI parser
estricto y todo-o-error. `arboard` Linux convierte con filter_map y omite
líneas o nombres no UTF-8; el resultado puede ser parcial sin informarlo.
Su API tampoco recibe un límite previo a la asignación. No usarla sin una
estrategia comprobada para conservar ese contrato; evaluar una lectura MIME
cruda acotada, con selección de clipboard y backend Wayland/X11 explícitos.

Pegar una captura/imagen es otra porción: get_image() devuelve píxeles ya
decodificados, sin reservar primero el presupuesto de 32 MiB. Necesita lectura
codificada acotada, límite de dimensiones/píxeles, reserva antes de decode y
encode PNG en servicio. No poner bytes/píxeles en UiEvent, UiAction o Store ni
escribir un temporal global para simular un archivo seleccionado.

Pruebas: drop sobre modal/otro canal/cierre, lote excesivo sin subset, rutas
remotas/relativas/duplicadas, symlink reemplazado tras abrir FD, cancel/retry
con worker ocupado, foco/cuenta cambiados durante clipboard, sólo texto sin
convertirlo en archivo, lista MIME mixta inválida sin éxito parcial, imagen
excesiva sin asignación previa y pegado único sin duplicación de texto.
QA nativa Linux con fixtures propios; Windows/macOS siguen sin acreditar.

## Fuentes primarias y anclas locales

- [FileChooser OpenFile](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html):
  multiple y resultado uris. No describe límites de cantidad/bytes de RUNE.
- [egui Event 0.36.2](https://docs.rs/egui/0.36.2/egui/enum.Event.html): Paste(String).
- limn pin histórico 2618b37: crates/limn/src/{tree.rs,element.rs,composer.rs,
  drag_drop_tests.rs,context_menu_tests.rs,context_menu_tree.rs};
  crates/limn-egui/src/lib.rs. Revisado por objeto git, no HEAD.
- limn pin histórico 1ad33eb: Composer opt-in, guard de superficies y orden de
  entrada del bridge; commits de la PR #49 y documentación de backend.
- limn pin histórico07.3.2,22e0fd93: PR #50 incorporada por cherry-pick propio;
  opt-in de altura para columnas y avisos/lista acotados de 07.3.2. No habilita
  el ingreso legacy ni integra `limn-dnd`.
- limn pin actual `6ae42859`: merge propio de `22e0fd93` y `b52284a0`, Composer/Column preservados,
  procedencia indexada y prefijo None; rama rune/ordered-input-integration.
- RUNE: apps/desktop/src/main.rs::{handle_input,prepare_input_view,host_key,run},
  apps/desktop/src/uploads.rs, crates/rune-ui/src/uploads.rs y
  crates/rune-core/src/uploads/{actor.rs,prepare.rs}.
- Plataforma: crates/rune-platform/src/{portal.rs,file_dialogs.rs},
  file_dialogs/files.rs y sus fixtures. Mantener la cobertura anterior de
  owner/sender/path, Response temprano, Close, Drop fuera de runtime y renovación.
- Registry fijado por Cargo.lock: egui-winit-0.36.2/src/{lib.rs,clipboard.rs,
  dropped_file.rs}; arboard-3.6.1/src/lib.rs:205 y platform/linux/mod.rs:51;
  smithay-clipboard-0.7.3/src/mime.rs. No acredita clipboard nativo; la QA del host se registra por separado.
