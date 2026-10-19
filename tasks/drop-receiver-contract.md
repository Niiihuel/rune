# Recepción Wayland de archivos: contrato de 07.3

**Estado: biblioteca implementada en el worktree de limn; integración RUNE
pendiente.** El contrato del 2026-10-15 para `FileDropReceiver`, receive-only
dentro del crate existente `limn-dnd`, guió la implementación y revisión del
integrador y del revisor independiente. ROOT4 verificó metadatos, decisiones
explícitas y cierre normal sólo en Weston privado anidado; la interoperabilidad
SDK/clipboard en Hyprland sigue abierta. La implementación está publicada en
la [PR #55 de limn](https://github.com/Niiihuel/limn/pull/55), Draft abierta,
head `73382702c5a1137ba2cfcbad5d5ebc2585697e6a`, con biblioteca
`0f8a74c47a27620cf322434af5bf9d8cb6fb23ac`. La API no está en el pin RUNE ni
tiene consumidor RUNE. El
[check CI](https://github.com/Niiihuel/limn/actions/runs/38772048890/job/116550099462)
terminó en SUCCESS el 2026-10-15 a las 16:50:38 UTC; la PR sigue Draft abierta,
sin merge. No crea otro framework ni crate. Las cifras, evidencias y el
[rechazo legacy 07.3.2](uploads-next.md#0732-rechazo-del-ingreso-legacy-actual)
permanecen intactos.

La [auditoría estática](../docs/evidence/limn-dnd-existing-backend-audit-2026-10-15.json)
identifica un backend existente, pero también Copy|Move, ofertas sin identidad
suficiente, colas/lecturas sin límite y cierre con join. Este contrato endurece
esa frontera antes de conectar un consumidor RUNE.

## Objetivo y API

Recibir la lista URI de la oferta exacta que cayó sobre la superficie real,
con punto, presupuesto y cancelación propios. El receptor no abre archivos:
lee únicamente el pipe `text/uri-list`. Accept completa el protocolo Copy tras
la admisión explícita del consumidor; no significa lectura, preparación ni
envío en Core. Linux/Wayland es el alcance; X11, Windows/macOS, portapapeles y
salida `start_drag` quedan fuera del criterio de cierre.

Firmas implementadas en el worktree de limn, todavía fuera del pin RUNE:

```rust
unsafe fn FileDropReceiver::attach_owned<W>(
    window: Arc<W>,
    limits: ReceiveLimits,
    wake: impl Fn() + Send + 'static,
) -> Result<Option<FileDropReceiver>, ReceiveError>
where
    W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static;

FileDropReceiver::poll(&self) -> Vec<ReceiveEvent>;
FileDropReceiver::decide(&self, id: OfferId, decision: DropDecision)
    -> Result<(), ReceiveError>;
FileDropReceiver::cancel(&self, id: OfferId);
FileDropReceiver::invalidate_pending(&self) -> Result<ReceiveEpoch, ReceiveError>;
FileDropReceiver::close(&self);
FileDropReceiver::status(&self) -> ReceiveStatus;
```

`attach_owned` retorna después de validar límites/handles, reservar y hacer
spawn; no espera al compositor. `Ok(None)` significa handles no Wayland, sin
fallback. Los errores inmediatos de validación/reserva/spawn son síncronos;
manager/protocolo ausente o incompatible se publica asíncronamente como
Closed con causa Unavailable y wake. `OfferId` es opaco, Copy/Eq/Hash,
sin serialización ni reutilización, incluso entre receptores. Incrementos
comprobados rechazan admisión al agotarse, sin saturar ni envolver IDs.
`SurfacePoint { x: f64, y: f64 }` usa unidades de `wl_surface`, no de limn
después de zoom/DPI.

Eventos propuestos:

- `Hovered { id, point }` y `Left { id }`: proyección coalescida del hover.
- `DropStarted { id, epoch: ReceiveEpoch, point }`: gesto ocurrido, punto congelado antes de
  solicitar/leer el payload.
- `DropReady { id, paths: Vec<PathBuf> }`: EOF y lista completa validada.
- `DropEnded { id, result: Result<(), ReceiveError> }`: único terminal de un
  drop admitido que produjo Started.

Los eventos con payload no son Clone; Debug y errores no exponen URI/path.
`DropDecision` tiene Accept/Reject. `ReceiveStatus` tiene Starting, Ready,
Closing y Closed con causa opcional. Ready exige inicialización completa;
Closed exige limpieza y join real fuera de UI, no la solicitud de cierre ni
que sólo haya terminado la función principal del worker.

Errores: Unavailable, Busy, ResourceLimit, InvalidLimits, InvalidUriList,
TooLarge, TooMany, Cancelled, Timeout, Io, StaleOffer, InvalidPhase y Closed.
Son tipados y redactados; Reject termina con Cancelled, sin inventar un fallo
de preparación.

`decide(...)->Ok` sólo acredita admisión de la decisión para procesarla en el
worker, no éxito de finish. Cancel/invalidate/close pueden ganar antes del claim
aun después de ese Ok de admisión. Sólo `DropEnded(Ok)` acredita el intento local
con `send_request` tipado Ok, sujeto a los límites de garantía descritos abajo.

## Límites acordados

`ReceiveLimits` configura `max_list_bytes`, `max_files`, `max_path_bytes`,
`max_total_path_bytes`, `startup_timeout`, `transfer_timeout` y
`decision_timeout`, independientemente de uploads de RUNE.

- Defaults: lista 64 KiB, 32 archivos, path 4096 bytes y paths agregados 64 KiB.
  Techos de validación: 1 MiB, 256 archivos, 64 KiB por path y 1 MiB agregado.
  Todos positivos, con sumas/multiplicaciones comprobadas antes de asignar.
- Deadlines por fase: startup 5 s, transferencia 5 s, decisión 2 s. Duraciones
  mayores que cero, como máximo 60 s y válidas para `Instant::checked_add`.
  Startup tiene un único deadline global desde Starting hasta el primer Ready;
  invalidaciones o nuevos Arm no lo reinician. Un byte o movimiento de hover
  tampoco reinicia el deadline absoluto de su fase.
- Un receptor por pareja display/surface y **cuatro hilos de recepción más un
  único reaper fijo por proceso**, incluyendo receptores Closing. No son cuatro
  hilos totales. Busy no crea otro hilo, pipe ni oferta. Permisos y reserva de
  superficie se retienen hasta limpieza y join real fuera de UI.
  La reserva de superficie/cupo se comparte con el worker principal legacy;
  la garantía de cierre asíncrono y cuatro más uno corresponde al camino
  Receiver, sin acreditar límites nuevos de los lectores legacy por drop.
- Como límite independiente, **64 intentos propios de `get_registry` por
  proceso**, con presupuesto atómico no renovable. Se gasta inmediatamente
  antes de emitir cada request y no se devuelve si el intento, startup o cierre
  falla. No hay mapa de displays/owners ni reset por reutilización de punteros.
  Los rechazos anteriores a ese punto no gastan crédito. Agotamiento en
  Starting produce Closed(ResourceLimit) y wake, sin bloquear UI; Busy queda
  reservado al cupo temporal de cuatro workers/superficie. Es un límite de
  nuevos arranques, **no de drops**: los receptores Ready siguen atendiendo
  ofertas sin consumir otro crédito. Sólo reiniciar el proceso repone este
  presupuesto; close, cleanup y reattach no lo restauran.
- Una oferta activa y un pipe no bloqueante; no hay hilo por drop. Buzón de
  cinco eventos fiables más un slot de hover coalescido: `poll` devuelve como
  máximo seis eventos. No existe una cola de desborde.

El drop reserva sus créditos al admitir Started, antes de recibir datos. Los
cinco slots cubren Started, Ready y Ended, con margen; Ended conserva crédito
aunque Ready espere decisión. No se admite otro drop hasta que poll retire
Ended. El hover previo no reserva terminal: Leave anterior al drop libera la
oferta y publica la proyección Left sin esperar un Ended que nunca se prometió.
Hovered/Left comparten el slot coalescido; una oferta posterior sustituye el
hover anterior sin acumular eventos. No se entrega hover viejo después del
Started/terminal de su ID. Busy/MIME rechazado no genera spam de eventos.
Un hover legal previo a Started puede durar mientras siga vigente: no tiene
un plazo nuevo ni pipe. Leave, cierre o retirada del device liberan ese estado.

Esto acota estado y colas propios, no toda la memoria del transporte Wayland,
colas C o SDK. RUNE podrá configurar ocho archivos, 4096 bytes por path y
32 KiB agregados al integrarlo, sin hardcodear esa política en limn.

## Copy, identidad y decisiones

Se exige `wl_data_device_manager` anunciado en versión 3 o superior y se hace
bind explícitamente en versión 3, suficiente para negociar Copy en este diseño.
Sólo se ofrece Copy. Move/Ask/None, acción desconocida o versión antigua se
rechazan; Copy debe estar confirmado en la oferta exacta antes de aceptar.
Un Drop/start con acción distinta de Copy rechaza y libera esa oferta antes
de Started, sin reutilizarla para negociar otro intento ni esperar Leave.
No emite Ended porque no prometió Started; puede limpiar hover con Left
coalescido, evitando dejar el slot Busy indefinidamente.

La operación conserva ID local, seat, data device, objeto de oferta y punto.
Una oferta nueva se rechaza con MIME None mientras el drop anterior ocupa el
slot; no sobrescribe su estado. Un callback/control de A nunca completa,
cancela ni mueve B. Destrucción/invalidez del device u oferta cancela esa
operación exacta. No se comparte una oferta global sin identidad.

Started se encola antes de iniciar recepción. Ready sólo existe tras EOF y
validación íntegra, con el mismo ID. El Leave normal posterior a Started sólo
limpia hover: no cancela la transferencia ni cambia su punto congelado.

Accept exige Ready vigente, Copy confirmado y deadline vivo. La primera
decisión se arbitra mediante un **claim atómico anterior a la llamada SDK**,
contra invalidación, cancelación, cierre y timeout. Si gana un control previo,
hay cero intentos de finish. Si Accept gana el claim, autoriza un único intento
de `send_request` tipado sobre la oferta válida, aunque se invalide después.
No se retiene un lock que bloquee UI durante la llamada SDK. El claim es
interno: no agrega una fase pública ni garantiza éxito. Reject/cancel pueden
cortar antes de Ready. Un ID viejo
devuelve StaleOffer, o es no-op para cancel; Accept prematuro devuelve
InvalidPhase. Sólo Accept válido hace finish sobre esa oferta, una vez.

`DropEnded(Ok(()))` acredita sólo ese intento local autorizado de finish con
`send_request` Ok. No garantiza almacenamiento ni transmisión en libwayland,
recepción o ACK del compositor: el protocolo no ofrece ese ACK al receptor.
Un Err del SDK produce Ended Err, aunque Accept haya ganado el claim. Los
controles posteriores al claim no revierten el intento autorizado ni generan
otro terminal. Flush WouldBlock se reintenta sin bloqueo
mientras vive el worker, sin afirmar entrega; cleanup/cierre no prometen un
flush remoto. Se usa `Proxy::send_request` tipado, que devuelve Result con
InvalidId; no el helper generado `.finish()`, que descarta ese resultado.
El marshal C retorna void y no amplía la garantía de un Ok del SDK. Tras Ended
no hay otro Ready, hover ni terminal de ese ID. No se añade
StartDrag al receptor nuevo.

## Pipe y codec íntegro

El mismo worker registra un pipe no bloqueante y lee hasta el límite más un
byte de detección. Exceso devuelve TooLarge, nunca un prefijo válido. Entre
lecturas comprueba oferta, cancelación y deadline; dribble no prolonga el plazo.

El codec valida toda la lista: URI local absoluta, escapes, NUL/controles,
autoridad, cantidad, duplicados y límites por path/agregados. Comentarios y
separadores válidos de URI-list no se convierten en archivos. Un elemento
inválido rechaza todo; no se usa filter_map ni normalización hacia otra ruta.
Rechaza segmentos punto incluso ocultos por escapes/separadores codificados.
Conserva nombres Unix no UTF-8 correctamente escapados y literales `%252e`.
No hace canonicalize, stat, apertura ni lectura de contenido de los archivos.

Cancelar cierra el pipe y descarta el lote en el worker. El slot sigue ocupado
hasta su terminal/limpieza. Poll/decide/cancel no esperan capacidad; buffers
internos propios se retiran y destruyen fuera de locks de control y fuera de
UI. DropReady transfiere su Vec de paths al consumidor: éste es responsable
de su descarte; la biblioteca no garantiza en qué hilo libera un caller ese Vec.

## Cola guest, propietario y cierre

`attach_owned` es **unsafe**. Arc retiene W, pero no prueba por sí solo la vida
del `wl_display`, superficie, EventLoop o despacho de la conexión extranjera.
El caller conserva la conexión y mantiene display/surface/EventLoop válidos
hasta cleanup; coordina los lectores existentes mediante libwayland/SDK, sin
lecturas crudas externas del socket. No se exige un único
lector host: el stack eframe ya puede tener un helper de clipboard coordinado.
El host mantiene progreso de lectura/despacho hasta cleanup, junto al tick de
la cola guest; llamar close antes de destruir EventLoop no basta. El receptor
nuevo no lee ni prepara una lectura de la conexión.

El worker registra registry/sync manuales y usa `dispatch_pending` de su cola
guest. No usa registry_queue_init, roundtrip, socket read/read_events,
blocking_dispatch, una lectura preparada que bloquee al host ni WaylandSource.
Flush WouldBlock se difiere sin esperar en UI. Un wait/tick acotado entre
intentos revisa controles/deadlines sin un bucle de CPU infinito.

Esto se verificó en fuentes locales: egui-winit 0.36.2 inicializa
[smithay-clipboard 0.7.3](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-clipboard-0.7.3/src/lib.rs:35>)
sobre el mismo display; su
[worker](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-clipboard-0.7.3/src/worker.rs:94>)
usa WaylandSource y
[calloop-wayland-source 0.4.1](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/calloop-wayland-source-0.4.1/src/lib.rs:188>)
coordina prepare_read con el SDK. Sus
[handlers DnD](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-clipboard-0.7.3/src/state.rs:392>)
son no-op y no reciben el contenido del drop. Leer esos fuentes no consultó
el portapapeles. La convivencia real del receptor con ese helper sigue
pendiente de QA; el receptor tampoco solicita contenidos de selección.

### Límites del backend, todavía DISEÑO

Se retienen como máximo cuatro seats anunciados con versión al menos 5;
cada bind usa versión 5 para disponer de seat.release. Como máximo hay ocho
ofertas pendientes **en total por receptor**, entre todos los seats y fases,
no ocho por seat. Sólo se retienen proxy/identidad y flags mínimos necesarios,
sin Vec de MIME ni nombres de seat. Un exceso de cualquiera de esos cupos
solicita Closing(ResourceLimit): no expulsa una entrada para seguir funcionando.

Discovery y Arm correlacionan el ObjectId exacto del callback y su fase, con
un solo callback en vuelo entre ambas fases. Un callback obsoleto o de otra
fase no avanza discovery ni rearma. Antes del primer Ready también se comprueba
el deadline global de startup, aunque las invalidaciones hayan generado Arm
nuevos. Al terminar discovery sin un seat compatible anunciado con versión
al menos 5, se solicita Closing(Unavailable); sólo el reaper publica Closed
tras cleanup y join reales.

Hay un único bind de data_device_manager por receptor. Si se retira ese global,
se cierra el receptor, sin una cadena ilimitada de rebinds. La versión 3 elegida
no dispone de release; el XML sí declara ese destructor desde versión 4, que
no se utiliza ni se presupone en esta porción. El cleanup destruye
ofertas, después data_device.release y finalmente seat.release. El receptor no
crea pointer, keyboard ni touch propios. El
[XML de wayland-client 0.31.15](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wayland-client-0.31.15/wayland.xml:1073>)
declara data_device.release desde versión 2 y seat.release desde versión 5.

El [protocolo oficial de wl_display](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_display)
advierte que los recursos de servidor de get_registry permanecen hasta la
desconexión del cliente; destruir el proxy local no basta. Por eso el cap de
workers activos no sustituye el presupuesto acumulado de 64 requests propios.
El receptor nunca desconecta al host para recuperarlos.
[`wl_fixes::destroy_registry`](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_fixes)
podría mejorar esto después, pero no se asume disponible
ni se inventa un ACK; no alteran el presupuesto de esta primera porción.

**Límite SDK explícito:** dispatch_pending puede drenar colas C sin cuota ni
interrupción. Los deadlines propios se comprueban antes/después de retornar;
no se promete timeout duro total ni teardown acotado bajo flood/bloqueo del
SDK. En ese atasco, close/Drop no hacen join en UI: el estado queda Closing,
owner y permisos retenidos, sin falso Closed ni reattach. Los cuatro slots de
recepción más el único reaper impiden ocultarlo creando hilos sucesivos.

Cancel/close operan fuera del buzón, incluso lleno. Close es idempotente y
rechaza nuevas ofertas/decisiones. Drop de Receiver sólo señala cierre y conserva/transfiere
propiedad en segundo plano. No hace join, roundtrip ni destrucción grande en
UI. El receptor cerrado no reabre implícitamente.

### Join real y reaper acotado

[`JoinHandle::is_finished`](https://doc.rust-lang.org/std/thread/struct.JoinHandle.html#method.is_finished)
sólo comprueba el fin de la función principal y puede preceder al fin del hilo.
[`join`](https://doc.rust-lang.org/std/thread/struct.JoinHandle.html#method.join)
espera también los destructores thread-local. Por ello is_finished es sólo una
pista para el único reaper; no prueba Closed ni permite devolver un permiso.

Los cuatro slots se reservan antes de spawn. En el camino Receiver conservan
owner Arc<W> con tipo borrado, permiso y JoinHandle; deben quedar registrados antes de
habilitar el trabajo del receptor, incluso si éste termina o entra en panic
inmediatamente. El owner no depende de la pila del worker: sobrevive también
a su unwind y a los destructores thread-local. Si el reaper no puede arrancar,
attach falla antes de emitir get_registry, sin un receptor huérfano.

El worker limpia pipe/ofertas/proxies/cola guest y su backend extranjero;
después señala una Condvar con bit persistente por slot: la señal anterior al
handoff del JoinHandle se conserva y el handoff notifica, sin polling constante.
El reaper puede tomar ese handle y hacer join **fuera de locks y de UI** aun
si is_finished sigue false por retorno/TLS; la señal no acredita Closed.
Extraer el handle para Joining no libera el slot: owner,
permiso y exclusión de superficie siguen retenidos durante el join. Tras su
retorno, el reaper descarta el resultado o panic payload, después suelta el
owner, libera el slot y publica Closed+wake sin guards de control retenidos.
Los descartes y el último Drop del owner también ocurren fuera de locks/UI.

Si SDK, TLS, descarte de payload/owner o reaper se atascan, se conserva Closing
y la propiedad pendiente, sin timeout duro de teardown ni reattach. No se
suelta owner en UI para esquivar el atasco ni se crea otro reaper/receptor por
drop. Tampoco se abre otra conexión ni se desconecta al host. Este límite de
hilos/slots es independiente del presupuesto no renovable de 64 registries.

Un panic que termina el reaper marca permanentemente el pool como fallido:
todos sus receptores owned pasan a Closing y se rechazan nuevas reservas
owned con Unavailable. Una reserva anterior tampoco puede habilitar un Job
después del fallo; conserva Job, owner, handle si existe y cupo, sin descartarlos
en UI ni publicar Closed. El gate arbitra ese fallo con la toma del Job bajo
el mismo lock; un Job ya tomado conserva la obligación de cierre del worker.
Un panic del callback final, incluido el descarte de su panic payload, se
aísla para que no termine el servicio de limpieza de otros receptores.

`FileDnd::attach` legacy también pasa a unsafe con la precondición real y sin
supresiones de lint. Conserva la firma prestada y su Drop/join síncrono actual:
el caller mantiene owner/conexión/superficie válidos hasta que retorna Drop.
Comparte reserva display+surface/cupo de cuatro y presupuesto de 64 registries,
pero no recibe un Arc inexistente ni se hace detach. Sólo Receiver es owned,
asíncrono y observado mediante Closed/reaper. No es el camino RUNE ni se
endurecen aquí sus lectores por drop; outgoing queda fuera del DoD. Legacy y
receptor nuevo no pueden actuar simultáneamente sobre una superficie.

## Prerrequisito antes de habilitar RUNE: corte por epoch

Un Drop anterior puede seguir en el transporte, cola guest o Vec ya retirado
por poll cuando el host cambia de A a B. Cancelar sólo el ID activo no corta
esa carrera: al drenar después podría asignarse el contexto nuevo al gesto
viejo. Esta barrera es requisito antes de conectar RUNE; no bloquea el primer
incremento B1a del codec ni declara una integración existente.

`invalidate_pending()` desarma síncronamente el receptor y cambia su epoch
antes de retornar, sin esperar al worker ni capacidad de cola. `ReceiveEpoch`
es opaco, Copy/Eq y monotónico con incremento comprobado, sin cuenta/proveedor.
Closing/Closed devuelve Err(Closed). Agotar el contador devuelve
Err(ResourceLimit), conserva el desarme y solicita Closing; Closed sólo llega
tras cleanup real. No se satura ni envuelve la identidad.

El worker mantiene **un `wl_display.sync` pendiente y el último epoch deseado**.
A→B→C se coalesce en ese último valor, sin cola adicional ni un sync por cambio.
Callback de sync y todos los proxies device/oferta pertenecen a la misma cola
guest desde su creación: no se mueven después ni usan la default queue del
host. Se descartan Enter/Motion/Drop anteriores al corte. El callback vigente
sólo permite esperar una **nueva Enter posterior a ese callback**; no relabela
una oferta previa cuyo Drop llegue después. El epoch de Started se fija al
admitir esa Enter nueva, nunca al drenar Drop.

Un callback obsoleto no rearma: solicita el sync del último epoch pendiente.
Error o deadline tampoco rearman. Antes del primer Ready, todos los Arm quedan
acotados por el único deadline global de startup. Después, cada Arm emitido
usa startup_timeout como plazo absoluto; coalescer epochs no reinicia el del
sync en vuelo. ObjectId y fase deben coincidir con el callback pendiente. Con reader
pausado y sin flood, vencer lleva a Timeout/Closing y cleanup/Closed, sin armar
ni bloquear UI. Se conserva el límite SDK ya documentado si dispatch no retorna.
La barrera sólo ordena eventos del protocolo; no acredita una cronología física
del SO ni el instante exacto del gesto respecto del render de RUNE.

Invalidación y Accept usan el mismo claim atómico previo al SDK: si gana la
invalidación, cero finish y terminal Cancelled para el drop viejo que ya tuvo
Started; si Accept ganó el claim, el intento autorizado sigue, incluso con
la llamada SDK en vuelo. Invalidar sigue siendo síncrono y no espera esa llamada;
no revierte el intento ni duplica terminal. Sólo el resultado tipado posterior
determina Ended Ok/Err; ganar el claim no anticipa un resultado exitoso.
Se conserva el orden/terminal de drops Started publicados; Ready interno aún
no entregado puede retirarse en el worker. Hover previo a Started se limpia
sin inventar Ended. Invalidar no modifica lotes ya admitidos por Core, que
conservan su scope original.

El host compara Started.epoch con el epoch de su contexto vigente incluso si
poll entregó un Vec viejo antes de invalidar. Ready/Ended pueden mantener sólo
OfferId: Ready exige un Started(epoch) vigente previamente registrado, nunca
crea contexto para un ID desconocido. Así un callback/payload viejo no puede
rearmar ni reinterpretarse como un gesto para B o C.

Regresiones exigidas: Drop antiguo retenido; oferta previa con Drop posterior
al corte; callback B tardío tras invalidar C; Vec de poll viejo; invalidación
contra Accept antes/después del claim y con SDK en vuelo; reader pausado sin rearme. También
comprobar coalescing A→B→C sin cola extra y ausencia de Ended para hover-only.

## Consumidor futuro y siete grupos RED/GREEN

El ejemplo nuevo será receive-only, aislado, con fixtures propios. Se desarrolla
en limn con tests, docs, changelog y ejemplo/galería; no se parchea RUNE ni se
ejecutan ambos backends en la ventana de prueba.

La futura integración RUNE procesa Started antes de Ready, congela punto y
contexto de destino/epoch/cuenta/canal/revisión de borrador. Navegación A→B,
modal, cambio de cuenta/permisos o borrador invalidan esa intención. Al llegar
Ready no se vuelve a elegir punto/destino desde el foco o puntero actuales.
El host transforma DPI/zoom, revalida overlays/Store y sólo hace Accept después
de admitir el lote completo en Core; Busy/error rechaza sin fallback. Esto no
es una integración implementada ni una cronología acreditada del SO.

1. **Copy/decisión:** Move/Ask/None/versión antigua rechazados; EOF no hace
   finish; Accept exacto hace un intento mediante send_request tipado y sólo
   su Ok admite Ended Ok; Err produce Ended Err. Invalidación/cancel/close/timeout
   antes del claim hacen cero; después no revierten el intento ni generan otro
   terminal. Con SDK en vuelo, invalidar no espera un lock y el resultado sigue
   siendo único, tanto para Ok como Err. WouldBlock
   no se presenta como ACK/entrega remota.
2. **Identidad:** A retenida frente a B/otro seat/device/punto; callbacks y
   controles viejos no afectan B. Leave posterior a Started no cancela.
3. **Codec:** límites exactos/+1, URI mixta/remota/malformada, duplicados,
   dotsegments/escapes y nombres no UTF-8; sin subset ni ruta normalizada.
4. **Pipe:** escritor retenido, dribble, I/O, EOF, cancelación y plazos absolutos;
   un lector, sin hilos extra ni lectura de contenido de los archivos.
5. **Créditos:** hover storm/buzón ocupado, Started→Ready→Ended y Left;
   terminal reservado, Busy/MIME sin spam, nueva admisión tras drenar Ended y
   hover-only que no queda bloqueado. Demostrar peor caso en cinco slots.
6. **Guest/progreso:** startup sin respuesta/host reader pausado sin flood
   vencen por plazo; ausencia de manager/protocolo produce Closed(causa)+wake
   asíncronos; flush WouldBlock y ninguna llamada prohibida. Con
   dispatchGate detenido: close retorna, Closing/owner/slot retenidos; liberar
   gate permite cleanup antes de owner/permiso/Closed. No llamar timeout
   completado al atasco SDK.
7. **Vida/recursos:** último Drop sin join UI, orden guest→backend→join real→
   resultado/panic payload→owner→permiso→Closed/wake, con un reaper fijo y
   cuatro slots de recepción/exclusión por superficie durante cierre, attach repetido,
   fallos de arranque y último owner. Probar limpieza real del host, no sólo
   el orden de llamadas: TLS retenido aunque termine la función principal,
   handle retirado para join sin devolver cupo, panic del worker conservando
   owner hasta join y reaper que no arranca sin get_registry. Verificar el tope
   cuatro hilos de recepción más uno reaper, incluso durante Closing.
   Con presupuesto privado aislado: 64 requests admitidos,
   el 65 produce ResourceLimit asíncrono y cero requests nuevos; concurrencia
   no supera 64. Rechazos anteriores al request no gastan crédito; intento
   fallido o cancelado después del gasto, cleanup y retry no lo reponen.
   Agotar el presupuesto de arranques no impide drops de receptores Ready.

Primero RED funcional, luego GREEN; un fallo de compilación no se cuenta como
prueba de comportamiento. Fixtures/gates deterministas prueban carreras, no
acreditan por sí solos un drop real. Comandos previstos, sin ejecutar ahora y
a serializar por el integrador:

```sh
rtk cargo test -p limn-dnd --all-targets --offline
rtk cargo clippy -p limn-dnd --all-targets --offline -- -D warnings
rtk env RUSTDOCFLAGS="-D warnings" rtk cargo doc -p limn-dnd --no-deps --offline
```

El cierre funcional exige luego QA Wayland entrante con punto/lote/identidad y
limpieza observados, seguida del consumidor RUNE. Aprobar esta especificación
no cambia legacy, transporte remoto ni los checkpoints existentes.

## Prerrequisito de coexistencia con el clipboard del host

**Bloqueo observado en QA2; solución todavía pendiente.** En el ensayo Wayland
de `/tmp/limn-drop-native-debug-20261015-msp79j6t`, la traza muestra
`get_data_device` para el device 135 del clipboard a 4281138,473 ms y para el
device guest 145 a 4281148,442 ms: 9,969 ms después. Enter, Motion y Leave
llegaron sólo al 135; el origen emitió un `start_drag` real. La demo alcanzó
`ReceiveStatus::Ready`, pero no produjo `DropStarted`, `DropReady` ni metadatos
de archivos. El
[registro REA de QA2](</tmp/limn-drop-native-debug-20261015-msp79j6t/native-demo-rea.json>)
(`ev_dcf6eab5d9842d793865157b3334785d67703b5128321de44030126071ccc225`)
y su [salida](</tmp/limn-drop-native-debug-20261015-msp79j6t/demo-exit.json>)
conservan el proceso con código 0 en 107,4944 s, sin señal automática;
`product_result_verified` es false. Ese cierre no acredita recepción funcional.
Las trazas respaldan un fallo de interoperabilidad del entorno ensayado;
no prueban una violación de la especificación ni un broadcast de eventos que
el receptor pueda dar por garantizado.

### QA3 anidada: recepción parcial y cancelación de la demo

El ensayo posterior ROOT3, en Weston privado anidado bajo
`/tmp/limn-drop-nested-native-20261015-prv7xlxv`, produjo `DropStarted` y
`DropReady` de la misma oferta, con dos nombres de fixtures propios
(`rune-arrastre-a.txt`, `rune-arrastre-b.txt`), y luego
`DropEnded(Err(Cancelled))`. El texto de diagnóstico fue «Context cut: target
layout changed»: los detalles/log hicieron aparecer la barra reservada del
scroll externo, reduciendo 14 px el ancho del target. No se admitió Accept ni
Reject; no se acredita intento local de Finish. El
[registro REA de ROOT3](</tmp/limn-drop-nested-native-20261015-prv7xlxv/native-rea.json>)
(`ev_c9d0d97e51980f789da64b2618624fdbc25a727b4e414ba43b521b9b009eaed4`,
SHA-256 `8104609677b993b35926c61180c91bdc9b29140d69b45264593a9aec837baf85`)
terminó por timeout y SIGTERM (señal 15, sin código de salida normal), sin
truncamiento. No se acredita cierre normal ni cleanup completo de ese ensayo.

La regresión pura de UiTree falló en 700×760 y 980×800 por el cambio de 14 px
(RED `890c6fe92c56`). El arreglo de presentación conserva header, target y Close
fijos; sólo instrucciones, detalles, nombres y log desplazan el scroll interno.
No cambia las validaciones de geometría/epoch ni las decisiones del receiver.
Las tres pruebas Gallery pasaron en 0,01 s (`a90b27`); la demo formateada
`5f171991a3942fee1910c4a40097ed92bef778f7e05444817bec7f183e879f68` pasó
Clippy estricto de Gallery y build (salida 0, `4c38c2`). El comando posterior
`cargo test --workspace --offline` pasó 1646 pruebas, cinco ignoradas y 21 suites
en 6,36 s (`342826`); ese total combinado de limn incluye doctests. Las 124 de
la biblioteca y tres Gallery son partes de ese resultado, no pruebas RUNE.

ROOT3 sólo acredita recepción parcial de metadatos en ese compositor. No
revoca el bloqueo SDK de QA2 ni prueba convivencia portable, Finish o cierre
nativo completo. Ese resultado histórico no se sustituye por el ensayo posterior.

### QA4 anidada: decisiones explícitas y cierre normal

ROOT4, en `/tmp/limn-drop-nested-native-20261015-8ty058fq`, verificó el alcance
receive-only en Weston privado anidado, con la misma fuente de demo
`5f171991a3942fee1910c4a40097ed92bef778f7e05444817bec7f183e879f68` y dos
fixtures propios. El [stdout](</tmp/limn-drop-nested-native-20261015-8ty058fq/demo.stdout>)
conserva oferta 1 con Started→Ready, dos nombres y Ended Ok tras clic nativo
explícito en Accept Copy; oferta 2 con Started→Ready, dos nombres y Ended
Cancelled tras Reject explícito. La observación de Root relaciona cada clic
con su oferta; el log por sí solo no acredita una acción de entrada.

Después, Close se activó con Tab→Space y la demo terminó normalmente con
código 0. El [resultado del runner](</tmp/limn-drop-nested-native-20261015-8ty058fq/runner-exit.json>)
registra que el origen y Weston recibieron pidfd/SIGTERM sólo después de esa
salida; Weston no fue señalado mientras la demo seguía viva. No se inyectó
Enter y la demo no abrió contenido de archivos. Esta ejecución CLI natural no tiene
ID REA y no se presenta como replay REA. El flag automático
`product_result_verified` del runner sigue false: el alcance positivo procede
de stdout, acciones y capturas observadas por Root y del resultado de cierre,
no de convertir ese flag en una acreditación automática.

Este resultado sólo acredita recepción de metadatos, decisiones explícitas y
cierre normal en ese Weston. Ended Ok conserva su definición de intento local
tipado de Finish: no es ACK del compositor ni acredita preparación/envío.
No corrige QA2 en Hyprland ni prueba clipboard funcional, otros compositores
o integración RUNE. El owner SDK compartido y los requisitos siguientes siguen
pendientes. RUNE conserva sus 1120 pruebas, pin `22e0fd93` y 18 capturas Mock;
no se habilita este backend ni se incrementa ese inventario.

El [registro canónico upstream/Weston](../docs/evidence/limn-drop-receiver-weston-2026-10-15.json)
retiene el informe CLI congelado `8afa5cc89ff5ecfaba317d5edebcaba593c804b1cb93c97704e8f21735f61961`,
el archivo fuente de 32 entradas y los checks. El output final del proceso es
una transcripción expandida del resultado de herramienta, no un rawstream
independiente. Se conservaron diagnósticos Documents/FUSE, EGL y Weston; no
se afirma una ejecución libre de mensajes. El cierre se correlaciona con
Release de device/seat, Destroy posterior de la superficie y salida 0, sin
inventar un evento Closed en stdout. Clippy estricto del workspace y formato
de archivos propios volvieron a pasar (`c7df75`). La PR #55 es Draft abierta;
publicarla no actualiza el pin ni habilita el consumidor RUNE.

### SDK compartido: pendiente de implementación

La fuente del SDK explica por qué adjuntar en `AppCreator` llega tarde:
[`egui-winit::State`](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-winit-0.36.2/src/lib.rs:109>)
posee un campo Clipboard privado y lo
[crea en su constructor](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-winit-0.36.2/src/lib.rs:166>);
[`eframe` crea State antes de AppCreator](</home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/eframe-0.36.2/src/native/wgpu_integration.rs:298>).
Ese constructor no ofrece hoy un hook de inyección para compartir el owner del
data device. Crear otro device para el mismo cliente/seat y depender de cuál
recibe primero no es una estrategia portable. Adelantar el guest para que
gane primero tampoco resuelve el contrato: puede perjudicar Selection.

El siguiente incremento debe separar tres responsabilidades concretas, aún
sin implementación ni API de SDK publicada:

- **Inyección del host antes de Clipboard.** El SDK debe aceptar el recurso
  común antes de construir `egui-winit::State`/Clipboard, y eframe debe poder
  suministrarlo antes de AppCreator. Un hook posterior en `CreationContext`
  no basta. La modificación corresponde al SDK/upstream; no se sustituye por
  un parche privado en RUNE ni por desactivar el clipboard.
- **Un owner de data device y routing por identidad.** Un módulo nativo común
  posee un único `wl_data_device` por cliente/conexión y seat. Los consumidores
  de Selection y DnD se registran en ese owner; registrar una ventana receptora
  no crea otro device. Selection conserva su camino de texto existente; la
  selección primaria mantiene su protocolo/camino propio. DnD se entrega sólo
  al receptor de la superficie exacta de Enter, conservando seat/device/offer;
  no se distribuye un mismo drop a todas las ventanas ni al foco actual.
- **Adaptador al coordinador acotado existente.** Se reutilizan codec, pipe,
  créditos, decisiones, claim y epochs. Los proxies y callbacks que ordenan
  el corte deben nacer en la misma cola común de despacho; no se mueven de
  cola después. Cada superficie conserva su contexto y barrera: cambiar o
  cerrar A no rearma B ni libera el servicio de Selection que B todavía usa.
  El owner de conexión/seat sigue retenido hasta cleanup y salida real de
  sus consumidores, sin join en UI para el nuevo receiver. No se añaden colas
  sin límite ni workers por oferta; el pool y los cupos de receptores siguen
  siendo los anteriores.

Estos cambios requieren revisar la propiedad de los proxies del backend
compartido; no autorizan a ejecutar ese owner y el backend guest independiente
actual simultáneamente para el mismo cliente/seat. Conservan los límites,
rechazos por exceso y condiciones de cierre ya establecidos. Tampoco añaden
lectura del portapapeles al alcance del receiver.

Pasos verificables antes de habilitar el consumidor RUNE:

1. Añadir una fixture privada adversarial que entregue DnD sólo al primer
   device registrado. Es un modelo de prueba, no una regla universal del
   compositor. Reproducir ambos órdenes de registro de consumidores,
   clipboard→receiver y receiver→clipboard. Exigir un único GetDataDevice por
   seat, routing de Selection intacto y Copy exacto tras EOF/Accept.
2. Incorporar el punto de inyección previo al clipboard y el owner común, con
   pruebas de construcción/fallo/cierre que mantengan texto y selección
   primaria. Usar ofertas sintéticas: no consultar el clipboard del usuario.
3. Conectar el adaptador al coordinador y probar dos superficies, epochs
   obsoletos, cierre de una ventana con otra activa, cancelación y liberación
   tras join real, conservando las regresiones anteriores y la ruta de Selection.
4. Repetir QA Wayland con dos fixtures propios y el clipboard del host activo:
   Started/Ready deben conservar oferta, epoch, punto y lote; Accept debe
   producir sólo el intento Copy local exacto y el cierre debe observarse.
   No leer ni escribir contenidos del clipboard durante esa QA; su routing y
   preservación de protocolos se comprueban con las fixtures privadas.

Hasta aceptar estas pruebas y el recorrido nativo, no se habilita RUNE. El
rechazo legacy actual, los checkpoints históricos y los criterios de calidad
permanecen sin cambios.
