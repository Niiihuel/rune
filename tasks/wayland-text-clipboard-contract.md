# 07.3: contrato de Selection/texto con reserva y ACK

Contrato v4 fijado tras revisión independiente; **texto entrante aceptado
localmente con matriz dirigida, checks estrictos y dos smoke nativos Weston**.
La rama `feat/shared-wayland-clipboard-text`, sobre la base
`06a08883634c5e5c2abe5afb4a5d624a68e98aef`, se publicó en el commit
`d254180819252a653c1d6e27aeb35dc8dc86c591` y la
[PR #63 Draft](https://github.com/Niiihuel/limn/pull/63), basada en
`feat/shared-wayland-data-owner`. CI de ese HEAD terminó [SUCCESS](https://github.com/Niiihuel/limn/actions/runs/38832943514/job/116755557558)
a las 01:05:13 UTC del 2026-10-16. PR Draft sin merge; adaptación SDK y
consumidor/pin RUNE siguen pendientes.
V1/v2/v3 permanecen intactas. V4 añade sólo las dos precisiones de revisión
fresca: pérdida Keyboard retira Selection y Reserve exige Ready/barrera resuelta.

## Checkpoint A1 aceptado (2026-10-15)

La [evidencia A1](../docs/evidence/limn-text-clipboard-admission-2026-10-15.json)
registra `TextClipboardLimits` y `ClipboardError` públicos implementados, IDs
checked no reutilizables, parent/SurfaceId inmutables y snapshots de metadata
acotados a cuatro seats. La autoridad única Incoming comparte admisión DnD/texto;
Reserve, ACK y claim privados revalidan identidad, vida, foco/epoch y reloj. Son
transiciones de metadata: en A1 todavía no había consumidor de protocolo para texto.
Cancelación y limpieza por ID exacto impiden que un handle viejo libere B.

Tras el formato acotado y su check pasaron **152 pruebas limn-dnd, cero fallos y
cero ignoradas**, en 0,80 s de tests/6,403 s wall: las 142 DnD anteriores, cuatro
de límites, cinco grupos de admisión y una de runtime con watcher real. La
prueba de runtime conserva Incoming mientras vive el descriptor de un arrastre
cancelado, sin detach del endpoint. Se corrigieron dos regresiones DnD manteniendo
sus pruebas originales y una fixture MIME que necesitaba una nueva identidad de
Selection; los intentos fallidos y sus correcciones se conservan en la evidencia.
Revisión independiente: cero hallazgos materiales. En ese corte aún no había
verificación estricta Clippy/workspace para la rama incompleta.

En ese checkpoint quedaban A2 y B/C, además de la API pública del owner/request.
A2 se aceptó después como corte interno, descrito a continuación; el historial y
la evidencia A1 permanecen intactos.

## Checkpoint A2 interno aceptado (2026-10-15)

La [evidencia A2](../docs/evidence/limn-text-clipboard-body-2026-10-15.json)
registra un único slot de body cuyo dueño fuerte vive en el worker, referencias
Weak desde control/request y staging de un único String RAW. La publicación
interna exige la precondición del finalizador posterior a retirar el watcher;
la fixture A2 usa exclusivamente el caso explícito de watcher nunca instalado.
**Ese checkpoint A2 no demuestra la retirada de un descriptor de texto real.**

Poll revalida autoridad y reloj fresco, entrega el String exacto una sola vez
(incluido vacío) y conserva CRLF; TTL y error único funcionan en este control
privado. El worker destruye bodies abandonados fuera de todos los guards antes
de liberar Incoming por ID exacto. Su Drop vacía el body antes de soltar su
Arc fuerte permanente; un Arc temporal de UI sólo puede quedar último con el
slot vacío. No finge cleanup nativo ni OwnerClosed. No se instrumentó la
liberación del String a nivel de allocator.

Pasaron **157 pruebas limn-dnd, cero fallos y cero ignoradas**, después de formato
y check acotados: 0,80 s de tests/6,583 s wall. Los cinco grupos A2 independientes
pasaron en el primer intento de implementación; las aserciones A1 permanecen
intactas, salvo el hook del nuevo módulo. El RED inicial fue de API ausente,
sin ejecutar cuerpos. Revisión independiente fresca: GO, cero bloqueantes
materiales. En ese corte aún no había checks estrictos Clippy/workspace, commit, push o PR nuevo.

El siguiente corte en ese momento era el consumidor permanente B/C en
State/Job/runtime. Se aceptó después para los cinco recorridos descritos a
continuación; la evidencia A2 conserva su alcance de control interno sin
watcher instalado.

## Checkpoint B/C histórico: candidato v1 interno sin formatear (2026-10-15)

La [evidencia B/C](../docs/evidence/limn-text-clipboard-runtime-2026-10-15.json)
registra el consumidor permanente de Selection/texto en el mismo owner, pool,
Job, queue, State, worker y data_device por seat de DnD, con opt-in interno.
Retiene Selection/foco de teclado antes del registro y exige un fence nuevo
para sellar SID/epoch. Un scheduler Sync alterna Arm/Clipboard; un callback
antiguo conserva su plazo absoluto y el orden Done/Enter decide la cuarentena.
El claim entrega MIME, cupo y deadline al Receive tipado; ReadTag distingue
lectores DnD/texto. Lectura acotada y decode UTF-8 estricto ocurren en worker,
fuera de guards; EOF sólo staged RAW. Publica en el borde real posterior a
sync_pipe. Selection no emite Accept, Actions ni Finish de DnD.

Cinco grupos nuevos pasaron sobre servidor/cliente Wayland privados, runtime
y watcher FD reales: Selection antes de registro con publicación tras EOF
bloqueada por Generic; cancelación tras staging; cancelación de prefijo abierto sin esperar EOF; orden
Done/Enter en un dispatch; y fence vencido mientras espera el guard root.
El poll observado exitoso comprueba que el Weak FD real ya murió; revisión de
fuente confirma el orden retiro→publicación. Son esos cinco recorridos internos,
sin prueba con compositor vivo ni interoperabilidad del portapapeles del SO.

Pasaron **162 limn-dnd, cero fallos y cero ignoradas**, 0,80 s de tests/3,242 s
wall: 157 regresiones previas y cinco B/C. El filtro correcto ejecutó los cinco
(0,58 s/2,983 s wall); un intento previo que filtró todos los cuerpos queda
retenido y no acredita conducta. RED inicial de API ausente, cero cuerpos.
Revisión independiente `bc_contract_challenge`: GO sin hallazgos materiales
sobre el snapshot exacto. Ese candidato v1 aún no había pasado formato ni checks
estrictos Clippy/workspace/docs/galería/entrega nativa. En ese checkpoint los
helpers `keyboard_capability` y `selection_none` advertían uso ausente; la
matriz posterior ya les da consumidores genuinos en tests, sin supresiones.

En ese momento la fachada pública y la matriz adicional estaban en autoría.
Se verificaron después en el checkpoint siguiente; B/C conserva su alcance y
su canónica histórica sin cambios.

## Checkpoint histórico público/runtime: 174 pruebas (2026-10-15)

La [evidencia pública/runtime](../docs/evidence/limn-text-clipboard-public-runtime-2026-10-15.json)
registra attach/reserve y `TextPasteRequest` públicos implementados: valida
ambas políticas antes de handles, conserva opt-in y rechaza endpoint ajeno
antes de Busy. Request sólo guarda metadata y Ping ligero; Debug constante
redactado, sin Clone/Copy/Default/serde, dueño nativo ni body. ACK/poll/cancel/Drop
señalan al worker tras soltar guards, sin join ni Receive síncrono en wrappers;
el orden del Ping está verificado por fuente, no por un observador nuevo.
RAW UTF-8, CRLF y EOF vacío se entregan una vez. Cancel/Drop retiran el lector
real antes de admitir B y los handles viejos no tocan B; request vivo no retiene
ventana/display después del cierre certificado.

A las cinco pruebas públicas se añadieron siete grupos: dos de cruce DnD/texto,
dos de payload/cupo, dos de contexto y uno de TTL público sin poll. Verifican
exclusión real en ambos sentidos, incluidos Hover/Reading/AwaitingDecision y
terminal sin drenar; cap7 llega al lector con EOF vacío, exact7, UTF-8 inválido,
8 bytes válidos TooLarge sin EOF y timeout real de 1 s con fuente abierta.
Keyboard perdida sin Leave retira Selection y lector, y exige Selection/foco
nuevos; replace corta Reading sin EOF y NULL corta Reserved sin Receive.
MIME Plain queda congelado para A y el siguiente request prefiere UTF8. Ese
MIME tardío es cobertura adversarial defensiva, fuera del orden ordinario XML
del protocolo. Contexto v2 exige Incoming libre⇒Weak FD exacto muerto en ambas
observaciones de cancelación, sin debilitar aserciones.

Published sin poll conserva Incoming y rechaza un arrastre con acción Copy
antes de vencer TTL. El worker libera por expiración y admite B sin poll/cancel
A ni reemitir Selection; A tardío da Timeout una vez y no altera B. La prueba
identifica publicación sólo con endpoint/foco/Selection estables: baseline de
wake tras Receive/Generic, wake posterior, FD muerto y reserva Busy, junto con
revisión de fuente. No introduce observador público de fase; el tick de 20 ms
puede explicar progreso. El rechazo DnD ocurre antes de una cota inferior
conservadora del vencimiento real. Copy aquí es acción DnD; CopyText sigue
pendiente. Los testigos FD/Incoming no instrumentan deallocation del String.

Pasaron **174 limn-dnd, cero fallos y cero ignoradas**, 1,76 s de cuerpos/4,18 s
wall; incluyen 162 anteriores, cinco públicas y siete adicionales, sin sumar
matrices. RED público inicial sólo de API ausente, cero cuerpos. Los grupos
pasaron al primer intento: es evidencia de cobertura, no de un fallo de algoritmo
observado. Revisión exacta GO sin bloqueantes materiales, con contexto v1 y su
fortalecimiento v2 retenidos. En el checkpoint 174 aún faltaban formato conjunto,
checks estrictos y entrega nativa, commit/push/PR. Ambos helpers ya tenían uso
real en tests. Seguían en autoría los cortes Published, dos seats, epoch/cupo8/9,
callbacks/lápidas, routing viejo, fairness y cleanup incierto. Ese estado quedó
superado por el checkpoint 184 siguiente; la canónica 174 permanece intacta.

## Estado actual: texto entrante v4 y matriz dirigida de 184 pruebas aceptados (2026-10-15)

La [nueva evidencia de matriz](../docs/evidence/limn-text-clipboard-matrix-2026-10-15.json)
añade diez cuerpos: tres de seats/barrera/cupo, uno de cortes Published, uno de
fairness, uno de cleanup, uno de ReadTag, dos de callbacks y uno de plazo de lápida. Pasaron **184 limn-dnd, cero fallos e
ignoradas**, 2,64 s de cuerpos/5,140 s wall. La revisión independiente final dio
GO funcional v4, cero bloqueadores y ningún trabajo necesario de implementación
o pruebas dirigidas identificado. Es cobertura dirigida y respaldada por fuente,
sin promesa cartesiana de todos los interleavings.

Se conservan las dos carreras de coordinación del barrier v1; v2 espera un pump
real adicional sin cambiar sus aserciones, límites ni único paso del servidor.
Fairness sí tuvo RED de algoritmo: sellaba B antes de A tras retirar X. La
corrección independiente de nueve líneas conserva el siguiente seat superviviente.
ReadTag usa replay **privado** de tags reales retirados sobre pipes/bytes reales,
nunca un callback Calloop nativo tardío observado. El plazo de Clipboard obsoleto
usa una cota de emisión real y observa Timeout antes de una posible renovación;
Closed/join se espera después, sin SLA del SDK. Cleanup incierto retiene recursos
nativos con Closing: FD ya muerto/Incoming libre antes del fallo sintético del hook.
Los testigos no instrumentan el allocator ni el hilo destructor del String.

Pasaron formato acotado final, Clippy estricto workspace/all-targets y Gallery
hot, workspace **1706/0/5** en 21 suites/14,808 s wall, no-default **1331/0/2** de
`-p limn` en 11 suites/35,965 s wall y rustdoc workspace con `-D warnings`
en 19,834 s wall. Son configuraciones separadas, sin sumar snapshots ni RUNE.
Los fallos previos de Clippy, catálogo generado, ENOSPC y filtro sin cuerpos
siguen retenidos; el catálogo se regeneró sin debilitar su prueba. Cuatro vistas
sintéticas de galería dark/light top/end de 1180×820 tienen ejecución y revisión
visual PASS; la página no solicita clipboard del SO.

Dos ejecuciones de API pública en Weston15.0.1 privado/Pixman/fake-seat entregaron
RAW UTF-8/emoji/CRLF de 29 B y String vacío de 0 B, una vez cada uno. Send1 del
manager ocurrió antes de attach; Send2 se observó tras ACK. Detached y Closed
mantuvieron host/source vivos; app con salida 0, sin watchdog/errores de cleanup y procesos
propios retirados. Sólo stderr de app está vacío; los avisos Weston de cursores
DnD quedan retenidos. El probe standalone tiene lock propio de 52 dependencias,
sin afirmar igualdad con el grafo workspace. No acredita Hyprland, SDK/UI/RUNE,
Primary, copy ni imágenes/URI. El contrato normativo siguiente permanece intacto.

## Alcance exacto

Sólo Selection entrante de texto mediante API real del owner. No instalar SDK,
CopyText/CopyImage, URI, Primary, ACK SDK, clipboard cache, lecturas sin ACK ni RUNE.
Owner DnD/receiver/legacy anteriores no cambian su modo ni empiezan a retener Selection.
El modo texto se elige al attach, antes de discovery; admite Selection con0endpoints
para no perder el evento pre-Enter. No es un ClipboardBackend parcial.

## API pública de texto entrante implementada y probada localmente

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextClipboardLimits {
    pub max_text_bytes: usize,          // default65536; rango1..=1048576
    pub acknowledgement_timeout: std::time::Duration, // default2s
    pub read_timeout: std::time::Duration,            // default5s
    pub completion_ttl: std::time::Duration,          // default2s
}
impl Default for TextClipboardLimits { /* valores anteriores */ }
impl TextClipboardLimits {
    pub fn validate(&self) -> Result<(), ClipboardError>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClipboardError {
    Unavailable, Busy, ResourceLimit, InvalidLimits, DeadlineOverflow,
    NoSelection, AmbiguousSeat, StaleRequest, InvalidPhase, Cancelled,
    Timeout, TooLarge, InvalidText, Io, Closed,
}
pub struct TextPasteRequest { /* privado: identidad y control de metadata */ }
impl WaylandDataOwner {
    pub unsafe fn attach_owned_with_text_clipboard<W>(
        display_owner: std::sync::Arc<W>, receive: ReceiveLimits,
        text: TextClipboardLimits, wake: impl Fn() + Send + 'static,
    ) -> Result<Option<Self>, ClipboardError>
    where W: raw_window_handle::HasDisplayHandle + Send + Sync + 'static;
    pub fn reserve_text_paste(
        &self, endpoint: &FileDropEndpoint,
    ) -> Result<TextPasteRequest, ClipboardError>;
}
impl TextPasteRequest {
    pub fn acknowledge(&self) -> Result<(), ClipboardError>;
    pub fn poll(&self) -> Option<Result<String, ClipboardError>>;
    pub fn cancel(&self);
}
```
`TextClipboardLimits`, su validación y `ClipboardError` se implementaron en A1.
`TextPasteRequest`, el attach de texto y `reserve_text_paste` públicos ya están
implementados y probados en el checkpoint público/runtime. El bloque conserva
las firmas del contrato, no constituye por sí solo una implementación compilable.
TextPasteRequest no Clone/Copy/Default/Serialize/Deserialize; Debug sólo redactado.
IDs privados checked no se reutilizan/rotan al agotar; ResourceLimit, sin wrap.
No mapa global de handles viejos: sólo el slot actual; el caller puede conservar
metadata de handles retirados. Request no guarda owner/Window/Proxy/ObjectId/pipe,
callback de usuario ni bodyArc; esos recursos quedan en State/worker. El mecanismo
ligero Ping sí puede acompañar control/request para ACK/cancel/poll; no es la Box
Fn de usuario, cuya propiedad/invocación/Drop siguen en worker/reaper. Snapshot usa sellos lógicos
checked que State liga al ObjectId completo real, device/seat y SurfaceId vigente.

Attach valida AMBOS límites antes de handles/admisión y conserva los bounds unsafe
actuales: surface hasta Detached; display/hostreader hasta OwnerClosed real.
No Wayland=>Ok(None); handles no válidos=>Unavailable. Un fallo de pool/protocolo
mapea exactamente su categoría (Busy/ResourceLimit/Io/Closed/Unavailable), sin payload.
NoSelection incluye Selection NULL/ausente; MIME no admitido=>Unavailable;
varios seats enfocados=>AmbiguousSeat. Identidad/vida se valida antes de Busy.
Register sigue usando las firmas existentes. Wakes siguen siendo Send, no Sync.

## Plazos y cupo únicos

Duraciones positivas≤60s; validate usa checked_add y falla InvalidLimits ante
rango/overflow de configuración. Cada transición vuelve a checked_add con reloj
fresco bajo autoridad; overflow posterior=>DeadlineOverflow, cero nuevo SDK/payload.
Reserva→ACK2s desde Reserve; ACK admitido→EOF/publicación5s desde admitir ACK
(incluye espera del worker); completion2s desde publicación validada fresca.
No bytes/reintentos/callbacks renuevan nada. El borde now>=deadline vence.

Conservar4workers+reaper,4superficies globales,4seats,64registry,8ofertas TOTALES.
Selection actual≤4 cuenta en8, con DnD y pendientes.9ª=>Destroy nueva+ResourceLimit
fail-closed como hoy; nunca evictar A/sustituir request. No promesa de disponibilidad
para toda secuencia transitoria legal bajo ese cap. MIME como flags, sin Vec<String>.

Una autoridad Incoming en el ROOT header, InboundLease::{Drop(reg,id),Text(request_id)},
reemplaza la lease worker-only, sin espejo como segunda autoridad. Text reserva el slot
DESDE Reserve; DnD lo reserva ANTES de Coordinator.enter/wake Hovered. Exclusión
incluye hover, lectura, awaiting-decision y terminal DnD sin ACK, no sólo pipe.
Owner/root→endpoint→request es el orden de guardas; se sueltan antes de Coordinator/SDK/wake.
No se cambia el terminal/ACK heredado DnD ni el detach que libera su lease exacta.

Text usa un slot completion reservado. Error/cancel/timeout/detach devuelve el
slot sólo después de retirar autoridad, pipe/watcher y body admitido, fuera guards.
Error viejo queda una vez en metadata del handle, sin retener slot/body ni tocar B.
Éxito conserva slot hasta poll válido o TTL/cancel; TTL elimina body en worker y
libera aunque UI no drene. Nunca liberar con watcher ArcFD vivo. SDK atascado
puede demorar cleanup; no prometer wall-time duro. Viejo poll/cancel no libera B.

## Foco y snapshot de Selection

Raw wl_keyboard propio por seat/capability, sin cambiar userdata/cola de winit.
Cerrar Keymap FD sin leer/mapear; ignorar keys/contenido, sin logs. Perder Keyboard
capability o retirar el seat corta sello/foco/request/completion SIN esperar Leave.
Destruir/retirar TAMBIÉN la Selection actual y la oferta en cuarentena de ese seat;
recuperar Keyboard requiere NUEVA Selection y nueva generación de foco/fence antes
de Reserve. Enter sin Selection nueva no recupera la oferta vieja. Release del
keyboard viejo; callbacks/fences anteriores no rearman. Otros seats quedan intactos. Enter/Leave
se observan incluso para superficies del cliente AÚN NO registradas. Registrar
esa superficie después permite pegar sin nueva Selection; no define pertenencia
al cliente. Exactly1seat con foco sellado en endpoint=>reserva;0=>Unavailable;
≥2=>AmbiguousSeat. Nunca primer seat/último cursor/egui-memory-focus.
Reserve exige owner Ready, endpoint Ready y barrera clipboard vigente/resuelta.
Registering o fence pendiente=>Unavailable SIN tomar Incoming, crear ID ni deadline;
header.armed DnD nunca sustituye esa condición.

Selection se recibe antes de keyboard.enter y puede no reemitirse A→B interno.
Guardar por seat, independiente del endpoint previo; NULL/replace invalida el
snapshot anterior y destruye sólo esa oferta. Elegir MIME en Reserve con flags
acumulados y prioridad fija text/plain;charset=utf-8 antes de text/plain. MIME
posterior no cambia request admitido. Ambos se decodifican UTF-8 estricto, sin loss.

Leave observado corta generación/reservas inmediatamente y pone oferta en
cuarentena SIN I/O; Enter interno válido puede sellar misma selección para B.
Corte sync identificado en misma queue/scheduler único resuelve cuarentena:
si no hubo Enter correspondiente antes del callback, retira esa selección.
Esto observa un prefijo de protocolo, no demuestra salida física: un Enter
interno posterior al corte NO revive la oferta retirada; rechazo conservador
hasta nueva Selection. A→B→A nunca revive request. No sleeps para inferir foco.
El fence debe emitirse DESPUÉS del corte: un callback ya en vuelo no sirve como
ese fence. Tag conserva seat/generación de foco/selección; un callback viejo no
retira Selection nueva ni sella una generación posterior.

NO usar header.armed DnD como permiso texto. invalidate_pending corta request
previo por epoch inmediatamente; worker observa epoch y sella estado clipboard
con barrera vigente en scheduler común. Después permite texto fresco SIN NUEVO
DnDEnter. Autoridad/fence de clipboard propios; shared scheduler no los mezcla
con el requisito de nueva Enter DnD. Callbacks viejos no sellan epoch nuevo.

## Reserve, claim, transferencia y handoff

Reserve sólo asegura metadata/cupo:0Receive/pipe/get/read/cache. ACK sólo se ENCOLA una vez (no SDK síncrono),
tras captura real del destino por host. Revalidar selección/MIME/foco/epoch/vida/
plazos antes de claim atómico. Cancel/corte gana antes=>0Receive. Claim ganado
antes autoriza intento sin guard durante send_request tipado; no es éxito ni ACK
remoto. Después del claim un corte impide entregar completion aunque SDK termine.
No Finish/SetActions/Accept DnD para Selection; Receive sólo MIME+FD, sin serial.

Misma pipe real/readiness: lector NONBLOCK, writer bloqueante+copia cerrada tras
Receive,4KiB/paso, límite+1 y EOF obligatorio; sin sleeps/chunks parciales como éxito.
EOF+decode sólo produce resultado privado STAGED del worker, nunca éxito público.
Borde obligatorio: pump_readable deja pipe retirada/staged → Runtime.sync_pipe
retira Generic/ArcFD real → Guest.finish_retirements finaliza publicación. No Ready,
pollSuccess ni release/reservaB antes de esa retirada; el helper sin Runtime sólo
usa ese mismo finalizador cuando nunca instaló watcher. Cancel/error/TTL siguen
el mismo borde antes de devolver Incoming. Éste es el gate del RED con FD real.

Revalidar después de dispatch/lectura/decode y dentro autoridad AL PUBLICAR tras
retirar watcher. UTF-8 inválido/Io/TooLarge/timeout produce error único. El servicio
entrega UTF-8 estricto RAW: CRLF permanece sin modificar, también en poll. Futuro
adaptador host/SDK convierte CRLF→LF exactamente una vez antes de editar, como
Immediate; jamás reemplazar texto bajo guard. Sin trim/Unicode normalization ni
fallback a otra selección/MIME. String vacío es completion válida, una sola vez.

Poll revalida autoridad fresca padre/endpoint/snapshot/foco/epoch y TTL ANTES de
tomar body. Si perdió validez, devuelve error/corte y Ping, sin tomar/destruir body
en UI: worker lo retira fuera guards. Poll válido transfiere String al caller;
su Drop posterior es responsabilidad caller. Worker mantiene referencia del
header hasta vaciar body interno antes de soltarlo; old handle finalDrop sólo
metadata. El handle usa referencia débil al slot de body actual para poll; el
worker conserva su dueño fuerte hasta vaciarlo. Upgrade temporal de poll nunca
deja un body interno como último Drop del handle/UI. El único slot body reutilizable
comprueba RequestId EXACTO antes de tomarlo: Weak de A no puede extraer body B. No wake/SDK/decode/destructor
pesado bajo guard ni join en UI. Poll
terminal entrega Some una vez, después None; cancel/Drop son idempotentes.
ACK duplicado=>InvalidPhase; viejo que ya no puede actuar=>StaleRequest.

## RED por porciones sobre producción real

A. Header/slot real: límites+checked ID; reserve excluye DnDHover/awaiting-terminal;
   DnD enter reentrante no permite segunda reserva. ACK único/old/foreign/deadline;
   TTL sin UI libera tras retiro, errores metadata no bloquean slot; body no drop UI.
   Gate real:0SDK antesclaim, claim ganado permite cancel sin bloquear guard.
B. Servidor privado actual:0endpoints Selection→Enter→registro; A→B sin nueva oferta;
   A→B→A stale; ambiguo2seats; preEnter/fence/Enter tardío conservador; NULL/replace/
   MIME tardío fijo; Keyboard perdida SIN Leave destruye Selection/cuarentena;
   recovery+Enter sin Selection nueva rechaza, nueva Selection/foco/fence permite;
   Registering/fence pendiente rechaza sin Incoming/ID, después Ready acepta; invalidate→barrier→texto sin DnDEnter; cap8/9ª/Selection-DnD exactos.
C. ACK→Receive exacto + FD real: prefijo vivo0completion; EOF/decode privado con
   gate antes sync_pipe:0pollSuccess/slotB; tras watcherWeak muerto→publish→poll;
   EOF vacío/UTF-8RAW+CRLF correcto,
   inválido/limit+1/Io/timeout/postdecodefresh; cancel/close/foco/replace tras Ready
   pero ANTES poll=>0String; TTL y reloj viejo. Watcher retirado antes nuevo slot B,
   oldhandle no toca B. Hostreader pausado/reaper/cleanup preservan lifecycle.
No crear fachada pública sin consumidor permanente: A es seam privado REAL usado
por B/C; API ausente es capability RED, ejecución de cuerpos se registra aparte.

## Posterior explícito (sin implementación)

Primary/CopyText/CopyImage/URI requieren contrato separado de cuotas8, seriales
reales para set_selection, fuentes y FDwrite no bloqueantes, límites imagen/PNG
codificados+dimensiones+decoded bytes, y destinos distintos de PasteTarget texto.
SDK4 es adaptación futura, no hook ausente: SDKc152f4b ya tiene factory/token/marker;
limn-dnd no conoce egui. No instalar backend hasta tener conducta real de copy y
pastes necesarios. Marker ordenado→captura target+epoch host→ACK mismo request;
completion tokenizada, nunca Paste genérico al foco actual. Embedded usa padre.
Getter síncrono no oculta I/O/cache/fallback; set_text String es propio y set_image
presta ColorImage (debe poseerse antes de retorno), void no acredita publicación.
Esta v4 no cambia SDK, pin RUNE, contratos existentes ni pruebas fuente.

## Estado en RUNE y evidencia del contrato

La [biblioteca owner DnD](wayland-data-owner-contract.md) está publicada en la
[PR #62 Draft](https://github.com/Niiihuel/limn/pull/62), con
[evidencia separada](../docs/evidence/limn-shared-wayland-owner-2026-10-15-v2.json).
A1 cubre admisión, A2 body/completion y B/C el consumidor permanente interno
para cinco recorridos verificados. El checkpoint público/runtime añade fachada
y siete grupos de matriz. El checkpoint 184 posterior acepta el alcance entrante
v4 dirigido, sus checks locales y smoke Weston; la entrega está en PR #63 Draft,
con CI SUCCESS del HEAD exacto; permanece Draft sin merge. La adaptación SDK y
el consumidor RUNE siguen pendientes; este contrato no cierra 07.3.
RUNE conserva su pin limn `6ae42859` y el SDK stock; no se instala un backend
parcial ni se incrementan las 1128 pruebas o las 18 capturas Mock históricas.
Se mantienen cinco de 21 flujos cerrados localmente.

La fuente normativa v4 se conserva en
[/tmp/limn-shared-clipboard-next-20261015-yox7rzs3/selection-text-contract-v4.md](/tmp/limn-shared-clipboard-next-20261015-yox7rzs3/selection-text-contract-v4.md),
SHA-256 `4eb237783e45fd710e73c682767df9b7dcea8e2fd6e31dc509e5885aa37004b7`.
Las versiones anteriores permanecen históricas. El sello público no acredita
API privada, implementación, pruebas ejecutadas ni interoperabilidad nativa.
