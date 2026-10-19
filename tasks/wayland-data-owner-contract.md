# 07.3: owner Wayland explícito y endpoints DnD

**Biblioteca DnD multiventana implementada y verificada localmente el
2026-10-15: pool/control, State, runtime y fachada pública conectados, con las
142 pruebas DnD ejecutadas.** Los checkpoints 130/133/136 permanecen históricos.
La [PR #62 Draft abierta](https://github.com/Niiihuel/limn/pull/62) publica el
HEAD `06a08883634c5e5c2abe5afb4a5d624a68e98aef`, con CI SUCCESS del HEAD exacto
observado a las 22:06:00 UTC. La presentación dark/light pasó la revisión Root,
**sólo como ilustración sintética**, sin attach del owner ni prueba de clipboard real.
No se acredita interoperabilidad nativa ni integración RUNE. V4 se mantiene.
La base es la [PR #55 de limn](https://github.com/Niiihuel/limn/pull/55), HEAD
`73382702c5a1137ba2cfcbad5d5ebc2585697e6a`; el trabajo está en
`/home/nihuel/projects/personal/limn-wayland-owner`, rama
`feat/shared-wayland-data-owner`. `WaylandDataOwner`, `FileDropEndpoint` y
`EndpointStatus` ya forman parte de esta biblioteca local.

El RED público histórico inicial falló con `E0432` por imports ausentes de
`WaylandDataOwner`, `FileDropEndpoint` y `EndpointStatus`: exit 101, **cero cuerpos
de prueba ejecutados**. Es un RED de capacidad, no una regresión demostrada de
la biblioteca anterior. El registro y sus fuentes están en
[/tmp/limn-wayland-owner-20261015-l36rkxcr/public-api-red.json](/tmp/limn-wayland-owner-20261015-l36rkxcr/public-api-red.json).
No hay QA de interoperabilidad nativa del owner ni integración RUNE. Ese
resultado histórico no describe la API actual: sus exports y pruebas públicas
ya están conectados.

## Estado parcial verificado: pool/control privado

El [corte aceptado del pool](/tmp/limn-wayland-owner-20261015-l36rkxcr/pool-accepted.json)
registra **130 pruebas limn-dnd pasadas, cero ignoradas**, 0,31 s de suite y
3,589 s de wall time. Incluye seis grupos nuevos y los 124 tests heredados
intactos. Verifica permisos globales separados de cuatro workers/cuatro
superficies, exclusión, autoridad padre, retirada y vidas; no acredita aún su
consumo por State/protocolo compartidos.

El sexto grupo cubre el wake del seed endpoint no reclamado, tanto al descartar
Reservation como ante spawn fallido. Se detectó y corrigió con RED retenido que
ese único wake se destruía sin notificar Detached: ahora atraviesa el acuse,
se invoca después de Detached y se descarta una sola vez fuera del caller y de
guards, con aislamiento de panic. Se corrigió además la carrera de la fixture
que esperaba la destrucción del Sender; ambos RED y el GREEN final permanecen
en el registro aceptado. La revisión final de Cache encontró cero hallazgos
materiales; formato de archivos propios y diff check terminaron con exit 0.

El checkpoint aceptado del pool no incluía resultado final de workspace,
Clippy estricto ni interoperabilidad nativa. En ese corte `PendingSurface.key`
todavía esperaba al consumidor State; su cierre privado posterior se describe
abajo, sin convertirlo en aceptación del runtime/fachada ni silenciar pendientes
con allowances.
El archivo público de tres pruebas
conservaba en ese checkpoint 3501 bytes y SHA-256
`c801e481fef6b1cbf38eab5740bde7f629f72b04879d4581e9d41448736591d7`.
Su registro `cfg(test)` se retiró temporalmente para comprobar el pool mientras
faltaba la fachada, con restauración obligatoria antes de las pruebas finales.
Esa restauración ya se completó en el cierre de 142; el resultado histórico130 no
incluía esos tres cuerpos públicos.

### Historia State 133: candidato verde antes de resolver la revisión

El [primer GREEN State](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-first-green.json)
ejecutó tres grupos nuevos de protocolo real: tres pasaron, cero fallaron y 130
quedaron filtrados, 0,00 s de suite y 4,515 s de wall time. Las
[regresiones del candidato](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-regressions.json)
ejecutaron 133 pruebas de biblioteca, cero fallos/ignoradas/filtradas, 0,31 s
de suite y 2,597 s de wall time. Este total conserva las 130 anteriores y añade
los tres grupos; no sustituye el checkpoint del pool y, en ese momento, no
acreditó el cierre de State. Los registros fijan los snapshots de ese candidato.

La revisión posterior encontró **dos fallos entonces abiertos**:

- El primer Ready debe vencer desde admisión. El candidato compara con un
  tiempo viejo mientras un sync renovaba su plazo, por lo que aquel candidato no
  garantizaba ese deadline absoluto.
- El scheduler puede favorecer de nuevo el índice cero: invalidaciones
  repetidas de A pueden relegar el armado de B.

En ese checkpoint los dos RED específicos aún se estaban preparando. El
cierre posterior conserva ese estado histórico y los RED ejecutados, sin
reescribir el candidato 133 ni el pool 130.

### State 136 privado: corregido, revisado y aceptado

El [cierre privado State](/tmp/limn-wayland-owner-20261015-l36rkxcr/state-accepted.json)
acepta el [GREEN final formateado](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-final-green.json):
**136 pasadas, cero fallos/ignoradas/filtradas**, 0,80 s de suite y 4,213 s de
wall time. Se resolvieron tres fallos bajo la misma autoridad que publica Ready:

- El primer Ready comprueba con tiempo fresco el deadline original de
  **admisión**, bajo el header; un sync posterior no renueva ese límite.
- El deadline del **sync propio** también se comprueba con tiempo fresco bajo
  ese header, incluida la rama de callback obsoleto. No basta con un reloj
  tomado antes de esperar a obtener la autoridad.
- Los turnos circulares acotados, con cursor estable al retirar registros,
  impiden que invalidaciones repetidas de A releguen a B por volver siempre
  al índice cero.

Los [RED de deadline y turnos](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-deadline-fairness-behavior-red.json)
retienen tres grupos pasados y dos fallidos; el
[RED de reloj fresco del sync](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-sync-fresh-clock-red.json)
retiene un grupo fallido. El cierre distingue además un error previo de filtro
que no constituyó RED de comportamiento. No se debilitó ninguna aserción;
se conservaron los seis adaptadores de paths de campos legacy y se verificó
el cambio de formato de los ocho archivos propios. La revisión final de Cache
registra cero hallazgos materiales.

Aquel cierre de 136 acreditó State/protocolo **privados**, no el ciclo vivo del
owner. En ese momento seguían pendientes `spawn_owner`, RuntimeWatcher, el
límite watcher → `finish_retirements` con FD real, la fachada y restaurar las
tres pruebas públicas. Los dos grupos runtime estaban autoreándose sin
cableado/ejecución y no formaban parte de las 136. Ese checkpoint tampoco
incluía cierre final de workspace/Clippy ni QA nativa; su alcance histórico no
se amplía con los resultados posteriores.

### Cierre actual de biblioteca 142 y comprobaciones requeridas

El [cierre final local](/tmp/limn-wayland-owner-20261015-l36rkxcr/final-ci-accepted.json)
registra las puertas CI requeridas ejecutadas sobre las mismas
[27 fuentes congeladas](/tmp/limn-wayland-owner-20261015-l36rkxcr/final-scoped-sources.json):

- Workspace: **1664 pasadas, cero fallos y cinco ignoradas preexistentes**, 21
  suites, 6,74 s sumando tiempos de tests y 28,473 s de wall time.
- `limn --no-default-features`: **1331 pasadas, cero fallos y dos ignoradas
  preexistentes**, 11 suites, 2,07 s de tests y 33,784 s de wall time. Es otra
  configuración; no se suma al workspace.
- Clippy estricto de workspace/all-targets y galería hot, rustdoc workspace con
  `-D warnings`, y formato comprobado del paquete limn-dnd: exit 0. No se
  atribuye un formato global nuevo a los 169 archivos heredados ajenos al corte,
  que se restauraron exactamente a HEAD.

Las **142 pruebas DnD** están incluidas en ese workspace, no se añaden a 1664.
Incluyen las tres pruebas públicas preservadas y restauradas, dos grupos de
runtime vivo y el flujo público completo añadido. Runtime `spawn_owner`,
Watcher y fachada pública ya no están sin cablear. El checkpoint no añade
ignore ni supresiones y no rebaja aserciones. La ejecución del límite real de
FD/retirada queda separada de cualquier afirmación de compositor nativo.

El catálogo se generó con 93 IDs y su test de sincronía pasó dentro del
workspace. Documentación y página estática de galería fueron revisadas; la
página sólo ilustra lifecycle sintético, sin owner/handles/clipboard reales.
Root verificó las dos capturas de presentación dark/light y ambas ejecuciones
terminaron normalmente, con cleanup propio. La página conserva su etiqueta
sintética: no adjunta owner ni consulta clipboard. Los resultados automáticos
y la revisión visual Root se conservan separados. Esto no prueba DnD nativo
multiventana, convivencia con clipboard, Hyprland ni consumidor RUNE.

El HEAD publicado `06a08883634c5e5c2abe5afb4a5d624a68e98aef` está en la
[PR #62 Draft abierta](https://github.com/Niiihuel/limn/pull/62), apilada sobre
PR #55/base `73382702c5a1137ba2cfcbad5d5ebc2585697e6a`, sin merge. El
[check del HEAD](https://github.com/Niiihuel/limn/actions/runs/38814443370/job/116695462384)
concluyó SUCCESS a las 22:06:00 UTC del 2026-10-15; la observación inicial en
progreso se conserva como historia. La
[evidencia canónica v2](../docs/evidence/limn-shared-wayland-owner-2026-10-15-v2.json)
separa CPU/CI, galería sintética y pendientes, sin modificar checkpoints previos.

La copia duplicada de `SurfaceLease.id`, su inicialización y getter son
**sólo observación `cfg(test)`** del SID que genera realmente producción.
Los IDs checked de slots, seeds, State y SyncTag siempre permanecen en
producción; no hay máquina alternativa ni consumidor ficticio para silenciar
la observación. El [audit del cambio exacto](/tmp/limn-wayland-owner-20261015-l36rkxcr/surface-lease-observation.json)
conserva esa distinción y la revisión de Cache.

Clipboard/Primary, reserva/ACK, integración SDK y consumidor/pin RUNE siguen
abiertos. RUNE no cambió su fuente ni el pin `6ae42859f86f5d255e107a23142aca3c19c86590`
por este cierre y conserva SDK stock 0.36.2; sus pruebas y progreso general
no se incrementan con los resultados de limn. Se mantienen cinco de 21 flujos
cerrados y 07.3 parcial. Las secciones siguientes mantienen v4 con biblioteca
publicada y los límites anteriores; no acreditan integración final de 07.3.

## Relación y alcance

Deriva del [contrato de host y clipboard compartido](wayland-clipboard-host-contract.md)
y conserva el [contrato del receptor receive-only](drop-receiver-contract.md).
El resultado de esta porción será DnD multiventana sobre una conexión: un único
`wl_data_device` por seat y un runtime compartido. Reutiliza State, Coordinator,
control, codec, pipe y pool existentes; no crea otra máquina DnD ni otro backend.

**Este primer owner sólo recibe DnD.** Destruye Selection al clasificarla y no
bindea Primary, igual que el receiver actual. No retiene selecciones sin un
consumidor que pueda retirarlas ni ocupa por ellas indefinidamente sus ocho
slots de ofertas. No incorpora un flag de pruebas, API incompleta de clipboard,
lectura de texto, fuentes salientes, CopyImage ni reserva/ACK. Una porción
posterior añadirá Selection/Primary funcionales y sus operaciones y límites
sólo al owner explícito. No se exportan stubs para esas capacidades futuras.

No se instala este owner en eframe como `ClipboardBackend`, no se suprime el
backend predeterminado con uno falso y no se cambia el SDK, el pin ni el
consumidor RUNE. Una demo que requiera clipboard esperará al owner funcional;
este corte no necesita otro host gráfico.

`FileDropReceiver` conserva firmas públicas, descarte de Selection, límites y
`Closed` posterior a cleanup/join. Su wrapper público puede permanecer intacto:
el `spawn_owned` privado adaptará un registro al runtime común y su control
servirá también al cierre de ese owner privado. `FileDnd` legacy conserva su
comportamiento, firma prestada unsafe y Drop con join; este incremento no
reescribe outgoing ni acredita hardening nuevo de sus lectores por drop.

## API pública implementada en la biblioteca local

Los tipos nuevos tienen campos privados:

```rust
pub struct WaylandDataOwner { /* private */ }
pub struct FileDropEndpoint { /* private */ }
pub enum EndpointStatus {
    Registering,
    Ready,
    Detaching,
    Detached(Option<ReceiveError>),
}
```

Firmas del owner:

```rust
unsafe fn attach_owned<W>(
    display_owner: Arc<W>,
    limits: ReceiveLimits,
    wake: impl Fn() + Send + 'static,
) -> Result<Option<WaylandDataOwner>, ReceiveError>
where
    W: HasDisplayHandle + Send + Sync + 'static;

unsafe fn register_surface_owned<W>(
    &self,
    window: Arc<W>,
    wake: impl Fn() + Send + 'static,
) -> Result<FileDropEndpoint, ReceiveError>
where
    W: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static;

fn status(&self) -> ReceiveStatus;
fn close(&self);
```

Estas firmas describen la API pública local; no son snippets completos de uso
ni indican que esté instalada en RUNE.
`attach_owned` valida `ReceiveLimits` antes de consultar handles; una plataforma
no Wayland devuelve `Ok(None)`. Retorna antes de discovery. No agrega lector
Wayland guest. El owner no implementa Clone; puede compartirse mediante Arc.
Su Drop sólo solicita cierre.

`register_surface_owned` exige la misma conexión viva y el ObjectId completo y
válido de la `WlSurface`. No sustituye su userdata ni su cola. No crea otro
registry, data_device, worker o conexión. La admisión reserva el registro antes
de publicarlo; una inscripción pendiente ya ocupa su permiso de superficie.

Los métodos del endpoint usan `&self`:

```rust
fn poll(&self) -> Vec<ReceiveEvent>;
fn decide(&self, id: OfferId, decision: DropDecision) -> Result<(), ReceiveError>;
fn cancel(&self, id: OfferId);
fn invalidate_pending(&self) -> Result<ReceiveEpoch, ReceiveError>;
fn detach(&self);
fn status(&self) -> EndpointStatus;
```

Cancel y detach son infalibles; Drop solicita detach. `decide(...)->Ok` acredita
sólo admisión de la decisión, no éxito de finish. Close, detach, cancel e
invalidación no dependen de espacio en una cola ni esperan al worker. Poll sigue
acotado a seis eventos. Se conserva la identidad y semántica de `ReceiveEvent`:
`DropStarted` lleva ID, epoch y punto congelado antes de recibir; `DropReady`
transfiere el lote de paths sólo tras EOF y validación completa; `DropEnded`
conserva el resultado terminal único de ese drop.

### Obligaciones unsafe y wakes

El caller mantiene la superficie válida hasta `EndpointStatus::Detached` y
la conexión/display con lectura coordinada del host hasta `ReceiveStatus::Closed`
del owner. Arc por sí solo no prueba la vida del display, la superficie ni el
progreso del EventLoop. Cada inscripción retiene exactamente el Arc admitido.
Los lectores existentes del host se coordinan mediante libwayland/SDK, sin
lecturas crudas externas. El receptor no lee el socket, no prepara lectura ni
hace roundtrip; el host debe seguir despachando hasta completar el cierre real.

Owner y endpoints tienen wakes independientes `Box<dyn Fn() + Send>`, poseídos
por worker/reaper. No se comparte un `Arc<Fn>` que exija Sync. Toda invocación y
su descarte ocurren fuera de guards. Las garantías fuera de UI cubren recursos
**admitidos**: los argumentos rechazados antes de admisión pueden destruirse en
el caller, fuera de locks. No se promete una cola ilimitada para esos rechazos.
Los Vec de paths ya entregados pertenecen al consumidor; su lugar de descarte
queda bajo responsabilidad de éste.

## Recursos compartidos, exclusión y vidas

Hay **cuatro permisos worker y cuatro permisos superficie globales separados**,
compartidos por owner, `FileDropReceiver` y `FileDnd`. Un receptor standalone o
legacy reserva atómicamente uno de cada. Un owner explícito reserva un worker;
sus endpoints usan superficies: el máximo es cuatro endpoints globales, no
cuatro por owner. Un owner vacío puede permanecer vivo y reutilizar sus devices.

Una entrada worker contiene su JoinHandle único, dueño de conexión, control y
wake lifecycle. Cada entrada superficie contiene registro fresco opaco,
SurfaceKey, worker asociado, Arc de ventana, control y acuse de retirada; no
contiene Job ni JoinHandle. Las inscripciones pendientes viven en slots fijos,
con Ping y coalescing; no en un canal o Vec de comandos sin límite.

El owner explícito reserva exclusión de display dentro del mismo pool:

- Otro owner, receiver o legacy sobre ese display devuelve Busy.
- El owner también rechaza si ya hay receiver/legacy en el display.
- Entre receptores anteriores se mantiene su exclusión actual por superficie.

No hay búsqueda automática de owners ni mapa global adicional de punteros de
display. Los leases retienen los dueños hasta liberar y evitan ABA por
reutilización del puntero. El integrador externo no debe construir dispositivos
de datos/clipboard paralelos en esa conexión; el pool no controla código ajeno.

### Cierre padre, claim y detach

`close()` y un fallo global cortan **síncronamente** la admisión y la autoridad
de todos los endpoints. Registrar y ganar un claim comparten la autoridad del
owner: el claim adquiere **owner header → endpoint header**. Cancelar A y luego
B sin ese vínculo permitiría ganar un claim B después del corte global.

El punto de arbitraje es el claim atómico previo a la llamada SDK, sin mantener
un guard accesible desde UI durante ella. Si close/invalidate/cancel gana antes,
no hay finish. Si Accept ya ganó el claim, su intento autorizado continúa;
ningún cierre posterior lo revierte ni duplica el terminal. Sólo `send_request`
tipado Ok acredita `DropEnded(Ok)`; Err produce terminal de error. No se usa el
helper generado que descarta el resultado. Incluso Ok acredita sólo el intento
local mediante una oferta válida: no garantiza almacenamiento/transmisión en
libwayland, recepción por el compositor ni ACK remoto, ni preparación/envío de
archivos por el consumidor.

Detach desarma y cancela el control exacto. El worker elimina routing,
callbacks atribuibles, pipe/watcher, ofertas SDK y Ready aún interno; después
retira el Arc del registro fuera de guards. Sólo tras esa limpieza, el descarte
del Arc y cualquier claim previo terminado se libera el permiso superficie y
se publica `Detached` para **ese registro viejo**. Panic o cleanup incierto
retienen el permiso y `Detaching`.

`Detached` acredita retirada local, no salida de thread ni destrucción de la
ventana: el dueño del display puede conservarla. A detach no cierra devices o
registry ni afecta B. El status del endpoint proyecta Starting/Ready/Closing
del control más el acuse de retirada; no publica `ReceiveStatus::Closed` por
retirar una superficie.

Owner close solicita detach de todos, cleanup de devices/seats/queue/backend
y join real mediante el reaper existente, fuera de UI y locks, incluido TLS.
Después descarta payload, dueño y permiso worker/exclusión de display, y sólo
entonces publica `Closed` y wake. El camino nuevo conserva hasta cuatro hilos
de recepción más un reaper fijo; no un hilo por endpoint o drop. El legacy
conserva su join síncrono y devuelve su reserva después de él. No se le atribuye
la retención Arc ni el cierre asíncrono del owner nuevo.

SDK/TLS/cleanup incierto conserva Closing y todo permiso todavía no retirado.
No se promete teardown duro bajo atasco/flood SDK. Se conserva el fallo
permanente del reaper: owned existentes Closing, nuevas admisiones owned
Unavailable y recursos sin cleanup certificado retenidos. EndpointStatus no es
prueba de join del owner; extraer un JoinHandle tampoco devuelve su cupo.

## Un solo State: ofertas, terminales y barreras

State contiene hasta cuatro entradas endpoint con el Coordinator, control y
buzón existentes. Un único lease `(registro, OfferId)` selecciona la pipe DnD
activa. Busy B rechaza B sin reemplazar A. El routing compara ObjectId completo,
device y seat; no usa foco global ni un número de protocolo aislado.

El lease sigue ocupado en AwaitingDecision/TerminalQueued hasta que se drene
el Ended exacto **mientras el endpoint continúe inscrito**, incluso sin pipe.
La excepción es un **detach completado**: puede liberar ese lease sin consumir
el terminal viejo, sólo después de retirar autoridad, routing, watcher/pipe y
oferta SDK, y terminar un claim previo. Con SDK detenido sigue Detaching.
Started/Ended reservados continúan drenables desde el control viejo; su poll o
acuse nunca libera el lease de B ni afecta una inscripción nueva.

Se conserva la invalidación síncrona de epoch y la exigencia de una Enter
nueva posterior a una barrera válida. Hay **un solo sync en vuelo**, con
userdata `(registro, epoch)`; callback, device y ofertas nacen en la misma cola
guest. Retirar/recrear la misma WlSurface no permite rearmar con callbacks
viejos. Retirar un endpoint tampoco permite emitir otro sync mientras su
callback siga pendiente: se retiene un **tombstone de identidad** hasta retirar
ese callback. No hay cola ilimitada de reemplazos ni cronología física del SO
inferida de la barrera.

Discovery/readiness del owner pertenecen al startup global de State, sin
endpoint ficticio para arrancar un owner vacío. Un callback confirma devices
antes de Owner Ready bajo el deadline global original. Cada inscripción arma
su Coordinator mediante la barrera identificada. Su primer Ready tiene deadline
absoluto desde **admisión**, incluyendo la espera del único sync; tomar su seed,
emitir sync, recibir callback o invalidar no lo renueva. Timeout retira sólo ese
endpoint, sin cerrar el owner ni B.

## Límites y protocolo que se conservan

`ReceiveLimits` continúa independiente de los límites de uploads RUNE:

- Defaults: lista URI 64 KiB, 32 archivos, 4096 bytes por path y 64 KiB agregados.
  Techos: 1 MiB, 256 archivos, 64 KiB por path y 1 MiB agregado. Todos positivos;
  sumas y tamaños se comprueban antes de reservar/asignar.
- Startup 5 s, transferencia 5 s, decisión 2 s. Cada duración debe ser positiva,
  como máximo 60 s y válida para `Instant::checked_add`. El primer Ready del
  owner conserva un deadline global; el del endpoint parte de su admisión.
  Bytes, hover, invalidación y callbacks viejos no renuevan plazos absolutos.
- Manager anunciado ≥3, bind explícito3; seat anunciado ≥5, bind5, máximo cuatro
  seats. Un bind de manager por owner; retirada de ese global solicita cierre,
  sin rebind ilimitado. Cleanup mantiene ofertas → data_device.release → seat.release.
- Ocho ofertas **totales** activas y pendientes por runtime, MIME mediante flags
  mínimos sin Vec de strings. Selection se destruye tras clasificarla, sin
  Receive/Finish; no se bindea Primary. Action pre-Enter se acumula.
- Copy estricto confirmado en la oferta exacta. Un Drop no Copy se rechaza y
  libera antes de Started, sin Ended que nunca se prometió. Leave pre-drop
  libera hover/oferta; Leave después de Started no cancela la lectura.
- Cinco slots fiables y un hover coalescido por buzón; máximo seis eventos por
  poll, sin cola de desborde. Started reserva créditos antes de recibir; Ready
  espera decisión y Ended conserva su crédito. Hover previo no reserva terminal.
- OfferId global checked y registros/epochs no reutilizables; agotamiento
  produce ResourceLimit, sin saturación que reutilice autoridad.
- Un único pipe no bloqueante, codec URI estricto del lote entero, EOF antes de
  Ready y reloj fresco después del decode. Error/cancel/timeout no publica un
  prefijo. Ping, readiness Level sobre el mismo ArcFD y tick guest/deadlines
  ≤20 ms; sin sleeps por chunk, lector guest ni roundtrip.
- **64 requests propios de get_registry por proceso**, presupuesto atómico no
  renovable gastado inmediatamente antes del request real. Un owner usa uno;
  registrar endpoints no gasta otro. Error, close y cleanup no reponen crédito.
  Agotarlo impide nuevos arranques, no drops de owners Ready; se informa
  ResourceLimit. Rechazos anteriores a emitir el request no lo gastan.

Los cupos acotan estado y colas propios, no toda la memoria C/SDK/transporte.
Los plazos se comprueban al recuperar control del SDK; bajo dispatch atascado
no se acredita timeout o cierre ya completado. Wake, SDK y descartes de payload
no se ejecutan bajo guards. No hay límites nuevos de texto porque su lectura
no pertenece a este corte.

## Primer corte: interfaz privada del pool implementada localmente

Se preservan las firmas actuales `reserve_owned`, `reserve_legacy` y
`Reservation::spawn`. Alias existentes: `Owner = Arc<dyn Any + Send + Sync>` y
`Wake = Box<dyn Fn() + Send>`. `OwnerId` envuelve el Token worker checked;
`SurfaceId` tiene índice y generación checked privados, Clone/Copy/Eq.
`SurfaceLease` no es Clone y transporta control y acuse mediante Arc pequeños.
Su copia del ID/getter sólo se compila para observación de tests; la identidad
productiva sigue en slot/seed/State/SyncTag.

```rust
struct PendingSurface {
    id: SurfaceId,
    key: SurfaceKey,
    control: ReceiveControl,
    wake: Wake,
    ready_deadline: Instant,
}

fn reserve_owner(
    self: &Arc<Self>, display: usize, owner: Owner,
    control: ReceiveControl, endpoint_timeout: Duration,
) -> Result<(OwnerId, Reservation), ReceiveError>;

fn reserve_surface(
    self: &Arc<Self>, owner: OwnerId, key: SurfaceKey,
    window: Owner, wake: Wake,
) -> Result<SurfaceLease, ReceiveError>;

fn take_pending_surfaces(&self, owner: OwnerId)
    -> Result<Vec<PendingSurface>, ReceiveError>;
fn retire_surface(&self, owner: OwnerId, surface: SurfaceId)
    -> Result<(), ReceiveError>;
fn close_owner(&self, owner: OwnerId, cause: Option<ReceiveError>);
```

Son métodos privados implementados en el pool/control aceptado y consumidos
por el State/runtime y la fachada local conectados.
`take_pending_surfaces` transfiere cada seed una vez, como máximo cuatro;
el Arc de ventana permanece en el slot al transferirla.
`retire_surface` sólo lo invoca worker/reaper tras cleanup local.
SurfaceLease expone `control() -> ReceiveControl` (clon Arc),
`request_retire()` e `is_detached() -> bool`; Drop sólo solicita retiro.
`id() -> SurfaceId` existe únicamente bajo `cfg(test)` como observación del SID
real, según el audit enlazado; no es parte de la API pública ni autoridad extra.
El pool construye el control con
`ReceiveControl::new_for_owner(ReceiveEpoch, ReceiveControl) -> Result<Self, ReceiveError>`;
el segundo control es el lifecycle padre. No admite un control arbitrario que
omita la autoridad global del claim.

Un mismo mutex protege `workers[4]` (OwnedSlot/Legacy, display y tipo
standalone/owner) y `surfaces[4]` (SurfaceKey, worker token, SurfaceId, window,
control, acuse y seed). Las reglas concretas son:

1. Obtener clon del control owner bajo pool y soltarlo; después adquirir
   **owner header → pool**, revalidar token/display/status, reservar y publicar.
   Nunca adquirir header bajo pool. En ese mismo header se estampa
   `Instant::now().checked_add(endpoint_timeout)`; positivo y ≤60 s. No hay
   holds de admisión ni cola adicional.
2. `close_owner` conserva el header owner, pone Closing, toma snapshot acotado
   de controles bajo pool, lo suelta y desarma todos bajo sus headers. Fallos
   permanentes del worker usan también este corte. Soltar guards/clones antes
   de callbacks/destructores. La adaptación standalone no adquiere dos veces
   el mismo header.
3. `retire_surface` mantiene Retiring ocupado, extrae window/seed y suelta el
   mutex para su último Drop fuera de locks/UI. Después retira metadata, suelta
   guard y publica el acuse Detached del registro viejo. Si el descarte panica,
   conserva permiso; no se libera antes del destructor. La excepción de lease
   terminal y el tombstone sync siguen las reglas de la sección anterior.
4. `CoupledToWorker` (standalone/legacy) no admite retiro temprano: devuelve la
   superficie sólo después del join correspondiente. `ExplicitEndpoint` puede
   devolverla tras Detached local, sin liberar worker/display. Un owner vacío
   mantiene su display y las obligaciones de lectura del host hasta Closed.
5. Seeds de spawn rechazado van al reaper; cleanup incierto retiene State,
   slots y dueños. Esto no extiende la garantía off-UI a rechazos preadmisión.

Errores privados: Busy para cupo/conflicto temporal; Closed para owner obsoleto
o cerrado; Unavailable para display ajeno o cero; ResourceLimit para generación
agotada; InvalidLimits para timeout inválido. Un ID viejo jamás opera sobre un
slot nuevo. Los errores siguen redactados, sin URI, path ni nombre de archivo.

## Orden de implementación y pruebas necesarias

Pool/control y State/protocolo privados fueron los primeros cortes aceptados.
El cierre de 142 conectó después spawn_owner, RuntimeWatcher y fachada pública,
reutilizando módulos y el nuevo `receive/data_owner.rs` con exports mínimos.
Los límites globales no se redujeron para las fixtures. Los grupos normativos
y su estado son:

1. **Pool privado compartido, aceptado:** dos owners con dos superficies cada uno, quinta
   Busy; legacy incluido; conflicto display en ambas direcciones; dos endpoints
   comparten un JoinHandle. Register contra close con gate, detach A seguido de
   registro fresco con B vivo, ArcDrop fuera de guards. Detached mientras
   gate/TLS conserva worker/display; superficie coupled sin liberación temprana;
   spawn/reaper failure conserva cierre honesto. El sexto grupo verifica el wake
   del seed no reclamado en Reservation Drop y spawn fallido. Fixtures existentes
   reutilizadas, sin reducir cupos.
2. **State y protocolo privados reales, aceptados:** servidor Wayland privado, dos superficies sobre
   una conexión y un device por seat; fixture first-device-only enruta A/B.
   Foreign se rechaza y Busy B no pisa A. EOF/Accept/Finish único reales;
   Cancel/Reject preservados, sin modelo paralelo de la máquina. Los tres grupos
   iniciales no cubrieron los fallos posteriores; el cierre de 136 incluye sus
   regresiones y correcciones de autoridad y turnos.
3. **Identidad y retirada: State y runtime vivo conectados y probados:** A → retiro → nuevo A; callbacks/epochs viejos no
   rearman; B sigue útil; el único sync conserva tombstone. Owner startup sin
   endpoint ficticio, plazo de endpoint desde admisión y timeout local. Claim
   frente a close/invalidate antes y después del arbitraje; no perder terminal
   reservado ni bloquear UI; detach completado libera sólo su lease exacto.
4. **Compatibilidad Selection conectada al runtime receive-only:** Selection/NULL/MIME intercalados
   destruyen sólo la selección exacta. Sin GetPrimaryDevice, Receive de texto ni
   Finish de Selection. Suite anterior intacta. El RED público por API ausente
   se distingue de resultados de cuerpos ejecutados y de interoperabilidad GUI.

Las pruebas privadas y públicas ejecutadas no equivalen a QA nativa del owner
en un compositor, clipboard funcional, instalación eframe o integración RUNE.
Todo ello permanece separado.

## Fuentes congeladas de este contrato

- [Contrato público v4](/tmp/limn-shared-wayland-owner-20261015-b34otz3y/owner-api-contract-v4.md):
  9956 bytes, SHA-256
  `14a8238fe6212701f1460d7a59ae21758bd3ca582ee5b236fef79b7a7ce9449d`.
- [Interfaz privada pool v4](/tmp/limn-shared-wayland-owner-20261015-b34otz3y/owner-pool-red-interface-v4.md):
  5272 bytes, SHA-256
  `44e446741ba42c9c87b528f3786c145d5fbcb29bb8ed33ba45d5cb26a4ba380a`.
- [Freeze revisado](/tmp/limn-shared-wayland-owner-20261015-b34otz3y/owner-v4.freeze.json):
  contratos revisados por Cache, GO para RED del pool, sin producción/GUI/RUNE.
- [Log del RED público](/tmp/limn-wayland-owner-20261015-l36rkxcr/public-api-red.log):
  2311 bytes, SHA-256
  `15904adb139cbbe1d5b48c9bc162d96e65a12033120edb3f9835ad36e77802d2`.

- [Pool/control aceptado](/tmp/limn-wayland-owner-20261015-l36rkxcr/pool-accepted.json):
  2906 bytes, SHA-256
  `c29243aaae6429eb3bc46da9b93a73eafd783d6879aaf283eccd042e09a4f80a`.
  Contiene los hashes de pool, control, seis grupos nuevos, pruebas públicas y
  registro temporal, junto a los RED/GREEN, formato y revisión final. Sus cinco
  fuentes se contrastaron con los bytes y hashes retenidos al actualizar este
  documento.

- [Primer GREEN State](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-first-green.json):
  10553 bytes, SHA-256
  `6ef2051e1c977f83a08273f3ca4e12b8a9fa41d6dc794ee388eda1fa16c42c02`.
- [Regresiones State](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-regressions.json):
  10541 bytes, SHA-256
  `c66eb5b5d37528debe012aa0378a5431895e76c0b85ffe0b778bab73455a9396`.
  Ambos registros retienen sus 31 snapshots fuente y logs exactos; sus tamaños
  y hashes se verificaron sin tomar el worktree mutable como evidencia congelada.

- [State privado aceptado](/tmp/limn-wayland-owner-20261015-l36rkxcr/state-accepted.json):
  2460 bytes, SHA-256
  `7558fd816f9d713d2922379597004eb408d73993309164a902996e92f904b2fe`.
- [State GREEN final](/tmp/limn-wayland-owner-20261015-l36rkxcr/owner-state-final-green.json):
  10873 bytes, SHA-256
  `85f5a1b65a621f6e3836acbe115f5e9a36d4cbdc9a4970406e2047ffd57011b6`.
  Se verificaron sus 32 snapshots fuente y el log retenido. La revisión, RED,
  prueba de formato y límites del cierre están enlazados desde state-accepted.

- [Comprobaciones finales locales](/tmp/limn-wayland-owner-20261015-l36rkxcr/final-ci-accepted.json):
  3674 bytes, SHA-256
  `4125b0a46a2c483c983b16cff215a968260edd0191eaa8cc8b783884efb739dc`.
  Se cotejaron los seis registros y sus logs contra sus hashes.
- [Fuentes finales del corte](/tmp/limn-wayland-owner-20261015-l36rkxcr/final-scoped-sources.json):
  8350 bytes, SHA-256
  `762256e3df670ead735f85789e39472b7ac97141dc57a54adcf88a9fcf1b719e`;
  27 archivos verificados contra el manifiesto.
- [Observación SurfaceLease](/tmp/limn-wayland-owner-20261015-l36rkxcr/surface-lease-observation.json):
  575 bytes, SHA-256
  `9de0939ab1d421ad603b04eb6764f38ef40c4a7d9354d8f12dc6f191ae8dc170`.

Los defaults y techos se contrastaron además con
`crates/limn-dnd/src/receive_limits.rs` de la base local indicada. Los v4 y el
RED público conservan su carácter histórico de diseño/capacidad; pool 130,
State 133 intermedio, State 136 aceptado y biblioteca 142 conectada son
checkpoints distintos. Ninguno acredita por sí mismo interoperabilidad nativa
del owner o instalación SDK/RUNE ya completadas.
