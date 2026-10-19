# Entrada indexada de limn-egui — incremento 07.3

Estado: **implementado y verificado en limn, incorporado después al pin RUNE propio**. Commit
`b52284a0d94b090784139d8be874f63b9694b48d`, worktree limpio y
[PR #59 Draft OPEN](https://github.com/Niiihuel/limn/pull/59), apilada sobre
`feat/captured-paste-target` (PR #56). El CI del HEAD terminó SUCCESS el
2026-10-15 a las 20:01:58 UTC; las dos observaciones iniciales IN_PROGRESS
quedan históricas y no se acredita merge. Este documento define procedencia y
orden observable del bridge, sin cambiar el pin de RUNE ni implementar clipboard,
reservas, ACK o entrega asíncrona. La
[evidencia nueva](../docs/evidence/limn-indexed-input-2026-10-15.json) conserva
las etapas RED, checks y QA sintética con sus límites.

Base verificada: `eb6ea2998a7332bfe0d8ed26024226c37a015b72` (HEAD de PR56),
worktree `/home/nihuel/projects/personal/limn-indexed-input`, rama
`feat/indexed-native-input`. La PR está apilada sobre
`feat/captured-paste-target`; no supone que PR56 esté fusionada.

## API y snapshot

API pública implementada, neutral respecto del backend de clipboard:

```rust
pub struct IndexedUiEvent {
    pub source_index: Option<usize>,
    pub event: UiEvent,
}

pub fn events_indexed(ctx: &egui::Context, origin: egui::Pos2)
    -> Vec<IndexedUiEvent>;
```

`Some(i)` identifica `input.events[i]` del **mismo viewport y snapshot posterior
a `normalize_keyboard_input`**. No identifica el RawInput anterior ni acredita
cronología física del SO. Normalizar primero puede eliminar teclas y desplazar
índices; la función nueva no vuelve a normalizar ni consume el input.

El host conserva el snapshot: no muta `input.events`, no cambia de viewport y
recorre **todos** sus índices, incluidos desconocidos sin `UiEvent`. Procesa las
traducciones asociadas en orden; varias pueden compartir índice. Los índices
`Some` de salida mantienen orden no decreciente.

El host consulta Key, physical_key, pressed, repeat y modifiers —incluido Alt—
en el evento fuente de ese índice. No amplía `UiEvent::Key` ni copia el converter
en RUNE. Sus atajos se resuelven durante ese recorrido, sin un prepass de
`ctx.key_pressed` que anticipe navegación posterior a un marcador. Antes de una
captura futura aplica los efectos, foco pendiente y layout del prefijo ya
procesado; el destino y la autoridad siguen en los contratos existentes de
[paste capturado](captured-paste-target-contract.md) y
[host clipboard](wayland-clipboard-host-contract.md).

`None` significa derivación del frame sin `egui::Event` fuente. Los actuales
FilesDropped/FilesHovered procedentes de RawInput conservan su posición de
compatibilidad al inicio de la salida, con `None`; no se les inventa orden
nativo. El host debe tratarlos explícitamente, sin atribuirles un índice.
El rechazo legacy de archivos en RUNE no cambia. `events_with_touch` y sus
temporizadores quedan fuera de este incremento; no se anuncian indexados.

`events` y `events_indexed` usan **un único converter**; la función anterior
descarta la procedencia. No hay una segunda ruta de traducción o cola.

## Deduplicación local

Se reemplazan las consultas de TODO el frame por pares **contiguos,
reconocidos y no solapados**, recorridos de izquierda a derecha:

- Key Enter presionada seguida de Text exactamente `\n`, `\r` o `\r\n`:
  conservar la tecla con su índice y omitir el Text duplicado.
- Key C/X presionada con Ctrl/Command seguida respectivamente de Copy/Cut:
  conservar la tecla, su índice y sus modifiers; omitir el segundo evento.
- Key V presionada con Ctrl/Command seguida de Paste: omitir la tecla y
  conservar el payload íntegro con el índice del Paste.

Cada miembro pertenece como máximo a un par. Cualquier evento intermedio,
incluso Zoom o un marcador ignorado, corta la adyacencia. Órdenes inversos y
combinaciones no reconocidas se traducen individualmente. No se reordena ni se
altera el payload. El cambio afecta también a `events`; requiere RED de
comportamiento y nota de compatibilidad, además del RED de API ausente.

## Límites de precisión y hechos de la base

El [converter de la base eb6ea299](https://github.com/Niiihuel/limn/blob/eb6ea2998a7332bfe0d8ed26024226c37a015b72/crates/limn-egui/src/lib.rs#L1860)
calculaba `native_paste`, `clipboard_key` y `enter_pressed` para todo el frame.
Así, Text(newline) → Zoom → Enter podía perder texto anterior a Zoom, y
Copy → Zoom → CtrlC podía perder la primera copia. El RED funcional confirmó
el primer caso; añadir índices solamente no corregía esa dependencia del futuro.
El [converter actual:1891](/home/nihuel/projects/personal/limn-indexed-input/crates/limn-egui/src/lib.rs:1891)
aplica la deduplicación contigua descrita arriba.
[events:1874](/home/nihuel/projects/personal/limn-indexed-input/crates/limn-egui/src/lib.rs:1874)
es su proyección sin procedencia, sin una segunda ruta de traducción.

La [normalización](/home/nihuel/projects/personal/limn-indexed-input/crates/limn-egui/src/keyboard_input.rs:19)
elimina el press físico de fallback seguido inmediatamente de Text imprimible
y su release, incluso entre frames. Conserva el orden de los supervivientes y
ajusta repeat/keys_down. La procedencia se define después de esa transformación.

El puntero inicial usa `input.pointer.hover_pos()`; MouseWheel puede depender de
ese dato de frame hasta un PointerMoved previo. Double/triple click usa el
estado agregado `input.pointer`, no un conteo incluido en PointerButton. Copy/Cut
sin Key no trae modifiers y conserva el fallback `input.modifiers.shift` final.
Son **límites de compatibilidad whole-frame**: un índice no convierte esos
datos en posición, click_count o Shift exactos de cada suceso. No se reconstruye
el historial físico ni se cambia el motor touch.

Fuente instalada de referencia: `egui-winit-0.36.2/src/lib.rs:1021–1040`, bajo
`/home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
Su ruta stock emite Copy/Cut/Paste y retorna sin otra Key; la deduplicación
atiende otros productores, no prueba un defecto de ese SDK.

## Cuatro grupos RED/GREEN

1. **Procedencia tras normalize.** Fallback físico KeyEnter + Text(`>`), Zoom
   ignorado y AltDown. Exigir índices del vector normalizado, lookup de Alt y
   ausencia del Enter eliminado; el release suprimido en el frame siguiente
   tampoco se traduce. No fabricar PasteRequested en una dependencia que no
   lo tiene.
2. **Orden con desconocidos.** Zoom → AltDown frente a AltDown → Zoom. Un
   consumidor de prueba recorre TODOS los índices y procesa navegación/captura
   en ese orden: destino A/B y posterior invalidación según corresponda, usando
   UiTree real. Ningún drain separado de marcadores al final del frame.
3. **Deduplicación y prefijo.** Controles positivos de los tres pares; Text
   newline → Zoom → Enter y Copy → Zoom → CtrlC conservan el prefijo. Comprobar
   texto/selección/acciones de UiTree, no sólo enums. Incluir Paste Unicode
   íntegro y KeyC → Copy → KeyC para probar no solapamiento.
4. **Orden mixto y derivados.** Pointer/Text/IME alrededor de Zoom conservan
   orden e índices. El recorrido host sigue viendo WindowFocused(false/true)
   aunque no haya traducción. Metadata de archivos mantiene `None` y su prefijo
   de compatibilidad; no se presenta como prueba de timing nativo, touch o
   conteo individual de clics.

Basado en el informe local
`/tmp/limn-captured-paste-20261015-bk2wr0cy/ordered-input-research.md`.
Archivos fuente verificados en la base: `lib.rs` SHA256
`914356e1320404aca84a544d22683c2cfc9bad3e26355dfd07f5d976fb2931b1`;
`keyboard_input.rs` SHA256
`c3e5e011aadff29a8a4f999dbc2437086fa140cfa2cf929713bd1b7bf0c6c48d`.
Esos hashes corresponden a la auditoría histórica de eb6ea299, no a la fuente
GREEN. Los cinco archivos finales, snapshots y commit quedan hasheados en la
evidencia nueva.

El RED de API no ejecutó cuerpos: E0432, exit 101, RTK `4ac0ff52d97f`. Tras
implementar sólo procedencia, el RED funcional dio 3 pasadas/1 fallida,
62 filtradas: faltó el prefijo Text newline separado de Enter por Zoom. El
tercer grupo se detuvo en esa primera aserción; no se atribuye ejecución a sus
checks posteriores. El GREEN focused dio cuatro grupos pasados, 62 filtrados.
El workspace limn pasó 1575 pruebas, cinco ignoradas existentes, 20 suites en
6,61 s (sesión Root 67863/chunk 49763e, sin logfile raw separado). Ese resultado
precede a la compactación visual final del modo ordered; después pasaron las
tres pruebas Gallery originales, Clippy estricto workspace y Gallery hot,
rustdoc estricto, formato de nuevos, build y diff check. El formato heredado de
lib se conserva fuera de las regiones cambiadas, con equivalencia normalizada
verificada. No sumar configuraciones, grupos ni Gallery a RUNE 1120/18 Mock.

## Galería y QA nativa acotada

El ejemplo captured_paste reutiliza App/fonts/paint/Run/Close/captura y agrega
`--ordered-input` y `--small`. Un Context de fixture sin ventana obtiene un
snapshot normalizado e índices reales; recorre todos sus eventos, incluido
Zoom como marcador sintético. Alt+Down→marcador entrega a B; marcador→Alt+Down
rechaza la respuesta tardía capturada en A. Usa el mismo UiTree/controlador
visible, sin cambiar el buffer asociado a escondidas. Las dos filas conservan
resultados históricos; los paneles muestran sólo el estado final de la segunda
secuencia. No se fabrica PasteRequested, token SDK, owner o ACK.

Siete ejecuciones seriales terminaron exit 0 con marcador único, PNG y cleanup
propio verificados en Weston 15.0.1 headless Pixman privado con fake-seat,
build WGPU sin inspección del adaptador. Root revisó las cuatro variantes
ordered dark/light × normal/square de 760×820: ambos históricos completos,
campos y footer legibles. En las dos small dark/light de 600×580 se observaron
ScrollArea y footer; los históricos quedan bajo el fold y **no se probó un gesto
de scroll**. Legacy-dark produjo un PNG byte-idéntico al dark/default final
del parent PasteTarget; eso acredita esa continuidad puntual.

Los runners conservan visual_reviewed:false del momento de ejecución. La
observación visual posterior de Root es un registro separado; no se reescriben
los runners. El deadline lógico requiere ticks y hubo watchdog externo de 25 s,
sin activarse en esos siete runs. Se retienen los diagnósticos EGL/MESA. Son
fixtures y capturas propias, no sandbox del SO ni clipboard I/O solicitado;
eframe puede inicializar su backend predeterminado.

Esta demo no acredita orden físico de teclas, todo el motor touch, interop
Hyprland ni DnD/pegado funcional de RUNE. Su baseline RUNE 1120/18 Mock/pin22e
queda histórico. La incorporación posterior del host se documenta a continuación;
owner/reserva/ACK/completion, epochs del dominio y pegado real siguen pendientes.

## Consumidor RUNE: orden, foco y query controlada verificados

El pin propio `6ae42859f86f5d255e107a23142aca3c19c86590`, rama publicada
`rune/ordered-input-integration`, une `22e0fd93` y `b52284a0` y conserva Composer/Column.
No implica merge público de PR59. El converter mantiene el prefijo None de
metadata derivada, sustituyendo el antiguo after-input artificial; RUNE sigue
rechazando esos archivos sin posición fiable y conserva la barrera drop+Enter.

[handle_input:1246](../apps/desktop/src/main.rs:1246) recorre todos los índices,
con metadata de Key/foco copiada sin retener el guard Context durante efectos.
[host_key:1574](../apps/desktop/src/main.rs:1574) procesa cada tecla en su lugar;
[prepare_input_view:1436](../apps/desktop/src/main.rs:1436) resuelve vista/layout
más foco antes del siguiente índice. Un foco solicitado ausente limpia el viejo.
WindowFocused llega al Tree en su posición incluso closing; input.focused
cubre el frame inicial/eventless cuando no hay callbacks de foco.

El [checkpoint propio](../docs/evidence/rune-indexed-input-host-2026-10-15.json)
retiene fixture E0560 sin cuerpos, RED 0/6→GREEN 6, raw-focus RED 6/1→GREEN 7 y
switcher RED 7/1→GREEN 8. Los ocho grupos están incluidos en **1128 pruebas RUNE**,
seis ignoradas, 25 suites,5,77 s sumadas (wall11,027 s); fmt, Clippy estricto
all-targets/all-features y build desktop correctos. Limn integración1595/5/21
es un comando separado, no se suma. El helper compartido abre el switcher con
SetInput vacío antes de Focus; reiniciar sólo Ui no vaciaba el input de Tree.

La primera QA nativa en Xvfb privado terminó0 pero quedó parcial: query
`generalQA-switcher-20261015`. Se conserva junto con sus cinco imágenes. Tras el
fix, Root verificó otras cinco: general conserva QA-A, ayuda QA-B, retorno a
general sólo QA-A y switcher exactamente QA-switcher, sin prefijo anterior.
Son secuencias con pausas, no prueba de batching del SO; ese orden dentro del
frame lo ejercitan los tests App. App/cierre/cleanup propios terminaron0 y se
eliminó la cookie Xauthority. Los runners conservan sus flags visual/product
false y la revisión Root es separada. REA final exit 0/signal 0 sin truncar
retuvo22 muestras de procesos; no es exhaustivo, y FS/network no se observaron.
Las diez imágenes QA no aumentan las 18 capturas Mock históricas.

Esta porción de host queda cerrada. No conecta aún el marcador SDK ni un owner,
ACK, completion o épocas de dominio para un target diferido; 07.3 sigue abierto.
