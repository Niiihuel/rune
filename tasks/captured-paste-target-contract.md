# 07.3: destino capturado para respuestas de pegado en limn

**API implementada y disponible en el pin RUNE; entrega diferida pendiente.**
Sobre la base `f8cabafc7910e95ad800d63f76eb71490ab39ca4`, el core quedó en
`9316d75da8b37b4c0c551e4b53b330b8c012beb0` y docs/galería en
`eb6ea2998a7332bfe0d8ed26024226c37a015b72`. La
[PR #56](https://github.com/Niiihuel/limn/pull/56) está Draft OPEN, publicada
con worktree limpio; su CI terminó SUCCESS el 2026-10-15 a las 19:35:57 UTC.
La [evidencia canónica](../docs/evidence/limn-captured-paste-target-2026-10-15.json)
separa checks CPU, galería sintética nativa y los intentos previos. Ese
checkpoint no cambió el pin de RUNE ni acreditó un owner/ACK de clipboard. La
[incorporación posterior y el host ordenado](../docs/evidence/rune-indexed-input-host-2026-10-15.json)
usan ahora el pin propio `6ae42859`; no completan el pegado asíncrono. La auditoría inicial de
la base y el RED de API se conservan como historia; las referencias del final
apuntan a la implementación actual del worktree propio
`/home/nihuel/projects/personal/limn-paste-target`.

Este incremento permite aplicar texto únicamente al estado editable que recibió
la solicitud. Es independiente del token SDK, clipboard, ventanas, proveedor y
cuentas. SDK#3 ya proporciona un marcador ordenado; owner, ACK y completion
siguen pendientes en el [contrato del host](wayland-clipboard-host-contract.md).
El baseline de aquel checkpoint conserva 1120 pruebas, 18 capturas Mock y pin
`22e0fd93c14b3f3c89375135f83c1e9c41307330`. El host posterior pasó 1128
pruebas; las 18 capturas históricas no aumentan con sus diez imágenes QA
separadas. El punto 07.3 sigue abierto.

## API implementada y separación de capas

```rust
// limn; fields private
pub struct PasteTarget { /* private capture identity and stamps */ }

impl UiTree {
    pub fn capture_paste_target(&mut self) -> Option<PasteTarget>;
    pub fn invalidate_paste_targets(&mut self);
}

pub enum UiEvent {
    // existing variants remain
    PasteCaptured { target: PasteTarget, text: String },
}

// limn-editor; fields private
pub struct EditStateToken { /* private buffer/edit/selection identity */ }

impl TextBuffer {
    pub fn edit_state_token(&self) -> EditStateToken;
}
```

`PasteTarget` es Clone, Debug redactado y Eq/PartialEq por identidad de captura:
clones comparan iguales y dos capturas frescas son distintas aunque apunten al
mismo estado. No implementa serde ni permite construir o cambiar sus campos.
Retiene sólo identidades/sellos pequeños e ID de control; no conserva texto,
Rope, snapshot, controller, TextBuffer, ventana o conexión. No se añade mapa de
reservas, contador global ni servicio de clipboard en limn core.

`EditStateToken` es público y opaco, Clone, Debug redactado y Eq/PartialEq. No
expone contenido ni permite autorizar otra instancia de buffer. La variante
nueva de UiEvent requiere adaptar matches exhaustivos externos. `PasteTo`
legacy permanece disponible y conserva su contrato previo basado en ID/foco;
no se lo presenta como protegido contra ABA.

## Captura e invalidación observada

La captura exige layout fresco, ventana con foco y un control actualmente
editable y habilitado, enfocado y dentro de su scope. Un menú/modal que impida
ese destino o un CodeEditor en Vim Normal/read-only lo rechaza. Para CodeEditor
se usa try_lock: si el buffer está ocupado, se devuelve None sin esperar.

El árbol mantiene una identidad de época Arc unitario. Se renueva al cambiar
realmente el foco, perder/recuperar foco nativo o entrar/salir de menú/modal,
y cuando layout observa retiro del control, cambio de kind o cambio de su
editabilidad/read-only. Debe cubrir focus(), focus_from_pointer, navegación y
las asignaciones directas de foco realizadas por layout/popovers; no basta
instrumentar el setter público. Una vuelta A→B→A observada nunca recupera una
captura vieja. Llamadas sin cambio efectivo no invalidan por sí solas.

Los cambios observados de modo/editabilidad de CodeEditor también cortan el
sello, aunque no alteren bytes, revisión ni selección. En particular, Vim Insert
con caret 0→Escape/Normal→Text("i")/Insert invalida la captura anterior: comparar
sólo el modo final Insert perdería ese ABA. Una captura fresca después del
retorno a Insert puede volver a ser válida; no se debilita el rechazo en Normal.

La entrada y salida observadas del lifecycle IME invalidan el destino, incluso
preedit→ImeCancelled sin commit ni cambio de texto/selección, tanto en Input
como en CodeEditor. Con owner de composición activo no cancelado, capture devuelve
None; después de cancelación una captura fresca puede ser válida y la anterior
sigue inválida. Un preedit vacío previo a Commit no libera por sí solo el owner:
se conservan el scope y tombstone de cancelación existentes, sin reasignar la
composición a otro control ni tratar provisional como texto confirmado.

InputState renueva su sello al cambiar texto, caret o selección efectivos.
Volver después al mismo valor/rango no recupera el sello anterior. Los setters
sin cambio efectivo lo conservan. No se clona el texto para comparar capturas.

TextBuffer incorpora identidad fresca por instancia, revisión de edición con
checked_add sin wrap y sello Arc de selección. El sello cambia ante cambios
reales de selección primaria o secundarias, incluso mediante shared_buffer;
una selección normalizada sin cambio lo conserva. Editar y deshacer/rehacer no
recupera un EditStateToken anterior. Crear otro buffer con el mismo texto,
DocumentId aparente o revisión no hereda la identidad del anterior. No se usa
BufferSnapshot, Rope ni un clon del controller como sustituto de este sello.

set_root marca layout pendiente: no permite capturar ni aplicar hasta el próximo
layout. Ese estado temporal no consume una captura. Un layout equivalente o
resize/reflow que conserva el estado editable permite usarla después; no se
invalida cada set_root ni cada layout. Si un layout observa read-only, retiro,
kind o scope distinto, el corte es permanente aunque un layout posterior
restaure las props originales.

## Validación, edición y consumo único

Antes de cambiar texto se comprueban árbol/época, control/kind/foco/scope,
editabilidad y sellos capturados. En CodeEditor, la validación del token de
buffer y la edición usan **el mismo guard try_lock**; no se valida sobre un
snapshot para volver a adquirir el lock después. No se bloquea UI esperando
el buffer ni se ejecuta SDK/I/O desde este camino.

La identidad de captura lleva un ticket compartido privado Arc<AtomicBool>,
sin mapa de tickets. Tras validar y obtener el guard necesario,
se reclama una sola vez antes de editar. Un destino aceptado consume ese ticket
y todos sus clones, incluso con texto vacío, texto que queda vacío al sanearse
o un fallo posterior de inserción. Una segunda entrega no puede aplicar el
texto. Consumir una captura vacía no consume otra captura independiente del
mismo estado. Rechazar por árbol/sellos inválidos ocurre antes del claim.

Busy/try_lock fallido y needs-layout producen cero acciones y no consumen el
ticket. El caller puede reintentar explícitamente sólo si siguen vigentes los
sellos; limn no crea cola, timer, task ni retry automático. El texto del intento
pertenece al caller, no se almacena en PasteTarget.

El camino aceptado conserva la semántica existente de TextInput/TextArea y
CodeEditor: normalización y límites de edición, selección/caret, read-only y
disabled, modos Vim, undo y grupos, invalidación de syntax y acciones normales
InputChanged/EditorChanged. EditorChanged sólo se emite si hubo edición real;
los errores conservan su salida normal, sin afirmar éxito. No se simula un
Paste genérico dirigido al foco que exista al completar.

## Límite de observación y obligaciones del host futuro

No se promete detectar un raw mem::replace A→B→MISMO A del buffer restaurado
entre observaciones, ni un ABA semántico de props sin layout. El caller debe
invocar invalidate_paste_targets en cada corte de dominio/lifetime que limn no
puede observar: cuenta/canal/contexto, endpoint/State, borrador y reconexión,
aunque vuelva a los mismos IDs. No se inspecciona todo el root por frame ni
se deduce la identidad de dominio desde nombres de controles.

El tramo de orden/foco del consumidor RUNE ya está implementado y verificado;
el owner y la entrega diferida siguen pendientes:

- [handle_input:1246](../apps/desktop/src/main.rs:1246) obtiene metadata pequeña
  del snapshot normalizado y consume events_indexed; recorre todos los índices,
  incluidos desconocidos, y resuelve cada tecla del host en su posición. El
  antiguo prepass de host_keys 1277/1465 pertenece a la auditoría histórica.
- [prepare_input_view:1436](../apps/desktop/src/main.rs:1436) aplica la vista,
  layout y foco pendientes antes del siguiente índice. Si el foco solicitado
  falta, [apply_pending_focus:1447](../apps/desktop/src/main.rs:1447) limpia el
  anterior; ya no deja texto entrando en un editor viejo. El paint final también
  usa ese helper. No supone que una futura captura/ACK esté implementada.
- [WindowFocused:1340](../apps/desktop/src/main.rs:1340) se reenvía al UiTree en
  su posición incluso durante closing. Si no hay callbacks de foco en el
  snapshot, [fallback:1293](../apps/desktop/src/main.rs:1293) aplica input.focused
  para el frame inicial/eventless. La nota anterior de ausencia de conexión era
  un hecho de la base 1120, no del host actual.
- Ocho regresiones App verifican orden/modifiers/foco/closing, incluida la
  reapertura del switcher: el helper compartido
  [open_switcher:574](../crates/rune-ui/src/settings/mod.rs:574) emite SetInput
  vacío antes de Focus. El [checkpoint](../docs/evidence/rune-indexed-input-host-2026-10-15.json)
  separa pruebas de batch sin ventana de QA nativa con pausas y conserva el
  primer recorrido parcial que descubrió ese estado controlado obsoleto.
- El bridge indexado no conecta todavía PasteRequested del SDK a RUNE. El owner
  futuro deberá reservar sin lectura, capturar el target tras el prefijo y
  acusar por el MISMO owner; completion conserva token más PasteTarget. Embedded
  usa el endpoint real del padre. Siguen pendientes epochs del dominio y las
  pruebas de sus ABA; ordenar foco/layout no los implementa implícitamente.
- Los gates de texto validan el control editable vigente y su política, no
  reutilizan SEND/ATTACH ni inician envíos/adjuntos. Closing, layout no fresco
  o foco sin resolver deberán producir cero ACK y cero lectura del owner futuro.

Cambios de borrador, modal o reconexión con retorno a los mismos IDs invalidan
las respuestas anteriores. El target limn no reemplaza las épocas del dominio
ni autoriza el I/O del clipboard por sí mismo.

## Pruebas y cierre del incremento limn

Los seis grupos de UiTree están en
[limn/captured_paste_tests.rs](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/captured_paste_tests.rs):

1. Happy path de campo, área y CodeEditor con acción exacta; clones de un solo
   uso, Debug sin texto, capturas independientes y consumo con texto vacío,
   saneado-a-vacío y fallo de inserción en editor limitado.
2. ABA de foco/campo, foco de ventana, menú y modal, sin resurrección al volver;
   preedit/cancel de Input y CodeEditor sin cambios autoritativos, capture None
   durante IME y captura fresca positiva después, respetando preedit vacío/tombstone.
3. Setters de selección y ABA de caret/primaria/secundarias, incluidas
   mutaciones por shared_buffer; los no-op preservan validez.
4. Texto cambiado y restaurado, undo, controller/buffer nuevos; raw restore
   invisible acompañado de invalidate_paste_targets explícito.
5. set_root pendiente y reflow equivalente; retiro/reaparición, read-only ABA,
   Vim Normal y disabled. Insert0→Normal→Insert sin cambio de selección/revisión
   debe invalidar la captura vieja y permitir una fresca. Preservar las reglas
   del editor, no saltarlas. Los casos se mantienen dentro de estos seis grupos.
6. Árbol ajeno, invalidación explícita y buffer ocupado: cero captura/acciones
   mientras está locked; después del release, retry explícito válido edita.

Tres grupos de limn-editor están en
[editor/captured_paste_tests.rs](/home/nihuel/projects/personal/limn-paste-target/crates/limn-editor/src/captured_paste_tests.rs):
identidad/opacity/clone, selección primaria/secundarias con no-op frente a ABA,
y edición/undo/redo frente a instancia nueva. Son pruebas de capacidad nueva;
el primer RED puede ser de API inexistente y no implica que sus cuerpos corrieron.

La prueba Busy usa un worker con canales y escape de dos segundos: exige que
el hilo del test lo libere explícitamente, sin umbral frágil de milisegundos ni
assert en destructor. No puede aprobar esperando que el escape termine el lock.
Los seis grupos Tree y tres Editor pasaron y están incluidos en las **1571
pruebas del workspace limn**, cinco ignoradas, 20 suites en 6,29 s. La galería
aporta tres pruebas separadas, repetidas después del pulido; no se vuelven a
sumar los grupos ni se añaden a RUNE. También se ejecutaron los dos snippets
completos de la guía. Pasaron Clippy y rustdoc estrictos del workspace, formato
de los archivos nuevos/editor/galería y diff check. El formato heredado de seis
fuentes se conservó fuera del cambio, con equivalencia normalizada verificada;
no se presenta como un cargo fmt global del upstream.

La galería nativa sintética pasó en cuatro variantes dark/light y esquinas
normales/square, cada una con exit 0, marcador de verificación y PNG propio de
760×820 revisado por Root. Usa Weston 15.0.1 headless Pixman privado con fake-seat
y build eframe WGPU; no se inspeccionó el adaptador. Demuestra entrega válida,
rechazo tardío y consumo único de vacío en TextArea y CodeEditor con datos
sintéticos. No solicita leer/escribir clipboard, aunque eframe puede inicializar
su backend predeterminado. El deadline lógico necesita ticks; cada ejecución
contó además con watchdog externo de 25 s, sin activarse en las cuatro finales.

Se conservan el primer intento sin fake-seat, fallido sin marcador/PNG y con
cleanup posterior acreditado, y dos ejecuciones v1 con tofu CJK y títulos light
ilegibles. El pulido fijó el foreground y usó `ñ`/`λ`, sin cambiar la lógica ni
las aserciones: **no implementa cobertura CJK**. Las capturas no son nuevas
capturas Mock de RUNE. La revisión independiente de core, galería y evidencia
cerró sin hallazgos materiales. La PR sigue Draft, sin merge; integración del
owner/ACK/completion y entrega de texto asíncrona siguen como incrementos
posteriores; el pin y el tramo de orden/foco RUNE se incorporaron después.

## Fuentes propias examinadas

La investigación inicial se hizo sobre la base f8cabafc y precedió al RED de
API inexistente, sin ejecutar entonces sus cuerpos. Las líneas siguientes
corresponden al HEAD publicado eb6ea299; los 15 archivos y snapshots están
hasheados en la evidencia canónica. No se inspeccionaron cuentas, portapapeles,
Serein ni el cliente oficial.

- [PasteTarget y captura, captured_paste.rs:26](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/captured_paste.rs:26),
  [capture:54](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/captured_paste.rs:54),
  [invalidate:81](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/captured_paste.rs:81)
  y [validación/claim:115](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/captured_paste.rs:115).
- [EditStateToken, editor/lib.rs:46](/home/nihuel/projects/personal/limn-paste-target/crates/limn-editor/src/lib.rs:46),
  [edit_state_token:421](/home/nihuel/projects/personal/limn-paste-target/crates/limn-editor/src/lib.rs:421),
  [set_selection:500](/home/nihuel/projects/personal/limn-paste-target/crates/limn-editor/src/lib.rs:500),
  [set_selections, editing.rs:232](/home/nihuel/projects/personal/limn-paste-target/crates/limn-editor/src/editing.rs:232)
  y [checked revision:894](/home/nihuel/projects/personal/limn-paste-target/crates/limn-editor/src/lib.rs:894).
- [shared_buffer, code_editor.rs:91](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor.rs:91)
  y [paste_captured:1101](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor.rs:1101):
  validar el token y editar con el mismo try_lock preserva los modos y acciones.
- [Vim normal:241](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor_vim.rs:241),
  [begin_insert:248](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor_vim.rs:248)
  y [Escape:301](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor_vim.rs:301):
  el ciclo de modos puede conservar selección y revisión; su sello no revive.
- [Owner/preedit, ime_tree.rs:80](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/ime_tree.rs:80),
  [ImeCancelled:108](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/ime_tree.rs:108),
  [cancel_composition:1536](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor.rs:1536)
  y [preedit:1594](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/code_editor.rs:1594).
- [set_root, tree.rs:3212](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/tree.rs:3212),
  [focus:3357](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/tree.rs:3357),
  [set_window_focused:3436](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/tree.rs:3436),
  [step_focus:6004](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/tree.rs:6004)
  y [reemplazo canónico de foco:171](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/captured_paste.rs:171).
- [PasteTo legacy, context_menu_tree.rs:557](/home/nihuel/projects/personal/limn-paste-target/crates/limn/src/context_menu_tree.rs:557)
  conserva su contrato previo; no equivale a PasteCaptured.
- [Host RUNE, handle_input:1246](../apps/desktop/src/main.rs:1246),
  [events_indexed:1292](../apps/desktop/src/main.rs:1292),
  [foco nativo:1340](../apps/desktop/src/main.rs:1340),
  [foco/layout prefijos:1436](../apps/desktop/src/main.rs:1436) y
  [tecla de host:1574](../apps/desktop/src/main.rs:1574) son referencias actuales.
  Las antiguas líneas 1277/1465/816/1887/1901 describían la auditoría previa.
  limn-egui por sí solo no establece foco en el Tree; hoy el host lo conecta.
