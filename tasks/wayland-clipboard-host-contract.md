# 07.3: propietario Wayland compartido entre portapapeles y DnD

**SDK#1, SDK#2 y SDK#3 implementados y verificados localmente** el 2026-10-15.
SDK#1 (`ac2ff2fec18c637a8841bcd4b626a98de3324d72`) incorpora backend custom y
constructor State; SDK#2 (`df0b452d09c363c5e5ec7f7a45a69c37fd4fdf4c`) añade la
fábrica eframe y el fallback explícito de viewport inmediato. SDK#3
(`c152f4b7692da7496a74d524755bb32f74db6805`) añade el token y marcador de
solicitud de pegado diferido. Son commits locales, sin push ni PR SDK. La QA
nativa anterior verificó sólo la fábrica sintética WGPU/Glow en Weston headless
privado, sin clipboard del SO. El destino capturado ya está implementado en
limn, incorporado después al pin RUNE propio, con CPU y galería nativa
sintética verificadas en la
[PR #56 Draft](https://github.com/Niiihuel/limn/pull/56), CI SUCCESS observado.
El bridge de entrada indexada se publicó en el commit `b52284a0`,
[PR #59 Draft](https://github.com/Niiihuel/limn/pull/59), con CI SUCCESS del HEAD
observado a las 20:01:58 UTC. Después se incorporó al pin RUNE propio `6ae42859`.
La base DnD multiventana del owner ya se publicó en la
[PR #62 Draft](https://github.com/Niiihuel/limn/pull/62), HEAD
`06a08883634c5e5c2abe5afb4a5d624a68e98aef`, con CI SUCCESS observado a las
22:06:00 UTC. Después se aceptó A1 local de metadata/admisión: límites/errores
públicos y Reserve/ACK/claim privados. A2 posterior añade body/completion
internos; B/C ya conecta Selection/keyboard, Receive, lectura y publicación al
runtime/watcher reales para cinco recorridos privados verificados. Su candidato
v1 conserva su checkpoint histórico sin formato. Después se implementó y probó
la fachada pública de texto entrante y, después, la matriz dirigida de 184 pruebas con GO
funcional v4, checks locales estrictos y dos smoke nativos Weston. El commit
`d254180819252a653c1d6e27aeb35dc8dc86c591` ya está publicado en la
[PR #63 Draft](https://github.com/Niiihuel/limn/pull/63), basada en la rama del
owner #62; CI de ese HEAD terminó [SUCCESS](https://github.com/Niiihuel/limn/actions/runs/38832943514/job/116755557558)
a las 01:05:13 UTC del 2026-10-16. PR Draft sin merge. Primary,
copy y adaptación SDK/RUNE siguen pendientes; no se instala detrás del SDK stock.
[Contrato del owner](wayland-data-owner-contract.md) y
[evidencia canónica](../docs/evidence/limn-shared-wayland-owner-2026-10-15-v2.json).
El cambio de pin y el consumidor orden/foco posteriores tienen
[checkpoint separado](../docs/evidence/rune-indexed-input-host-2026-10-15.json);
no habilitan clipboard diferido ni DnD y conservan el orden del punto 07.3.
El corte de Selection/texto tiene
[contrato v4 y checkpoints internos/público](wayland-text-clipboard-contract.md), tras revisión
independiente. La [evidencia A1](../docs/evidence/limn-text-clipboard-admission-2026-10-15.json)
registra 152 pruebas limn-dnd pasadas, cero fallos/ignoradas tras formato y check
acotados: cuatro de límites, cinco grupos de admisión y una de runtime nuevas.
Se conservaron la fixture MIME corregida y las dos regresiones DnD corregidas;
las dos pruebas DnD originales siguen intactas. En A1 no había nuevo commit/PR/pin
ni checks estrictos Clippy/workspace de la rama incompleta. Las matrices anteriores
conservan su alcance histórico.

La [evidencia A2 posterior](../docs/evidence/limn-text-clipboard-body-2026-10-15.json)
acepta body/staging/publicación/poll único/TTL internos: 157 limn-dnd pasadas,
cero fallos/ignoradas tras formato y check acotados, 0,80 s de tests/6,583 s wall.
Sus cinco grupos nuevos pasaron en el primer intento, sin rebajar aserciones A1;
revisión fresca GO sin bloqueantes materiales. La fixture usa la precondición
sin watcher instalado; ese checkpoint no certifica un FD de texto real.

El [checkpoint B/C posterior](../docs/evidence/limn-text-clipboard-runtime-2026-10-15.json)
acepta el consumidor permanente interno con servidor/cliente Wayland privados,
Receive tipado, decoder UTF-8 acotado y publicación sólo tras retirar el Generic
real. Sus cinco recorridos cubren pre-registro/EOF, cancelación staged y de
prefijo abierto, orden Done/Enter y fence vencido esperando root. Pasaron 162
limn-dnd, cero fallos/ignoradas (0,80 s de tests/3,242 s wall); revisión exacta
`bc_contract_challenge` GO sin hallazgos materiales. Su candidato v1 histórico estaba sin
formatear, sin checks estrictos de entrega, commit, PR nuevo o pin RUNE. Fachada
pública y matriz adicional estaban entonces en autoría; se aceptaron después
como se describe abajo. Ese corte no prueba compositor vivo ni interoperabilidad
del clipboard del SO. Los helpers `keyboard_capability`/`selection_none` advertían
uso ausente allí; ahora la matriz les da consumidores genuinos en tests, sin
supresiones. Las 162 incluyen las 157 previas y no se suman a otras matrices.

El [checkpoint histórico público/runtime 174](../docs/evidence/limn-text-clipboard-public-runtime-2026-10-15.json)
acepta attach/reserve/request públicos, Debug redactado y vida/Ping, con cinco
pruebas públicas más siete de cruce DnD, payload/cupo, contexto y TTL sin poll.
Pasaron 174 limn-dnd, cero fallos/ignoradas (1,76 s de cuerpos/4,18 s wall).
Contexto v2 refuerza Incoming libre⇒Weak FD exacto muerto y rotula el MIME tardío
como adversarial fuera del orden ordinario; conserva v1 y aserciones previas.
Published no drenado retiene Incoming/rechaza DnD hasta TTL real, y el worker
admite B sin poll/cancel A ni nueva Selection. Su testigo de publicación sólo
vale en un contexto estable, con wake tras Receive/Generic y FD muerto+Busy;
no es observador general de fase ni prueba aislada del Ping frente al tick.
Copy en ese cruce es acción DnD, no CopyText. Revisión GO exacta sin bloqueantes.
En 174 quedaban sin formato conjunto/entrega estricta y en autoría los demás
cortes Published, dos seats, epoch/cupo8/9, callbacks/lápidas, routing viejo,
fairness y cleanup. Esos pendientes de texto entrante se resolvieron después;
la canónica 174 no se reescribe.

La [matriz actual de 184 pruebas](../docs/evidence/limn-text-clipboard-matrix-2026-10-15.json)
añade diez cuerpos a 174 y tiene GO funcional independiente v4, cero bloqueadores
y ningún trabajo necesario de implementación/pruebas dirigidas identificado.
Incluye tres cuerpos de seats/barrera/cupo, uno de Published, uno de fairness, uno
de cleanup, uno de ReadTag, dos de callbacks y uno de lápida; no es una matriz cartesiana. La carrera barrier v1 se corrigió
con coordinación real de pump y aserciones intactas. El RED fairness real precedió
la corrección independiente del cursor al retirar un seat. Replay ReadTag es
privado, sin callback nativo tardío observado. Los plazos/Ping y el borde de body
combinan testigos reales con revisión de fuente, sin instrumentar allocator.

184/0/0 limn-dnd, workspace 1706/0/5 y no-default `-p limn`1331/0/2 son snapshots
separados. Pasaron formato final acotado, Clippy estricto workspace y Gallery hot,
y rustdoc workspace `-D warnings`. Cuatro vistas sintéticas de galería recibieron
PASS visual. Dos smoke públicos Weston15.0.1 privado/Pixman/fake-seat verificaron
RAW de 29 B UTF-8/emoji/CRLF y vacío de 0 B, poll único y Detached/Closed con host vivo;
Send1 anterior a attach y Send2 tras ACK. App con salida 0, sin watchdog ni errores de cleanup,
procesos propios retirados; avisos de cursores Weston retenidos. El probe tiene
lock standalone propio de 52 dependencias, no grafo workspace idéntico. Sólo ese compositor y
seat están probados: no SDK/UI/RUNE/Hyprland ni clipboard completo, copy/Primary.
PR #63 Draft no cambia el pin RUNE 6ae, las 1128 pruebas/18 capturas Mock ni 5/21.
[Evidencia SDK#1](../docs/evidence/egui-clipboard-injection-2026-10-15.json),
[evidencia SDK#2 CPU](../docs/evidence/egui-viewport-clipboard-factory-2026-10-15.json),
[QA nativa sintética](../docs/evidence/egui-clipboard-factory-native-2026-10-15.json),
[evidencia SDK#3](../docs/evidence/egui-deferred-paste-marker-2026-10-15.json),
[destino capturado limn](../docs/evidence/limn-captured-paste-target-2026-10-15.json)
y [entrada indexada](../docs/evidence/limn-indexed-input-2026-10-15.json).

## Hechos comprobados y alcance

La demo de limn recibió dos archivos en Weston anidado privado: Accept produjo
un Finish local y Reject no produjo Finish. En Hyprland instalado, el primer
`wl_data_device` del mismo cliente recibió los eventos y el receptor adicional
no los recibió. Esto acredita una incompatibilidad observada, no una obligación
normativa de broadcast ni un arreglo de Hyprland. Véase el
[contrato del receptor](drop-receiver-contract.md) y la evidencia conservada en
`/tmp/limn-drop-nested-native-20261015-8ty058fq/native-cli-evidence.json`.

El cambio externo necesario es permitir elegir el portapapeles **antes** de
construir el backend predeterminado. La biblioteca DnD publicada ya ofrece
un owner/worker por display y un data_device por seat compatible, con endpoints
por superficie, pero destruye Selection y no bindea Primary. Las 142 pruebas
DnD se ejecutaron dentro de 1664 del workspace; la configuración alternativa
sin defaults pasó 1331. No se suman matrices ni a las 1128 históricas de RUNE.
Root revisó dark/light de su galería estática, sin attach nativo ni lectura de
clipboard. RUNE sigue con pin 6ae/SDK stock y 07.3 parcial: sustituir ese clipboard
requiere todavía operaciones reales y acotadas. Cambiar binding, desactivar
texto o sustituir userdata de objetos ajenos no resuelve ese contrato.

## Fuente local examinada

Base de todos los paths siguientes:
`/home/nihuel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
Son fuentes instaladas anteriores al commit SDK#1, no capacidades deducidas
de versiones futuras; sus hashes se conservan como línea base histórica.

- `egui-winit-0.36.2/src/lib.rs`: State por viewport, línea 82; campo concreto
  Clipboard privado, 109; `State::new`, 138; construcción del portapapeles,
  166–168. `clipboard_text`/`set_clipboard_text`, 219–225; Ctrl+V llama `get`
  inline, 1027–1034; CopyText/CopyImage, 1124–1129.
- `egui-winit-0.36.2/src/clipboard.rs`: `Clipboard::new`, 32–53, inicializa
  arboard y smithay; get/set_text/set_image, 56/93/123; creación smithay con
  display extranjero, 174–184. No hay proveedor o fábrica públicos.
- `eframe-0.36.2/src/native/wgpu_integration.rs`: root crea State en 298–305,
  antes de `app_creator` en 333. `Viewport::initialize_window`, 1052, crea los
  secundarios en 1079–1086; el viewport inmediato también pasa por ella,
  1169–1172. La fábrica deberá viajar en SharedState para ese camino.
- `eframe-0.36.2/src/native/glow_integration.rs`:
  `GlutinWindowContext::initialize_window`, 1250; State en 1310–1320.
  La fábrica deberá conservarse en ese contexto, incluidos secundarios.
- `eframe-0.36.2/src/epi.rs`: NativeOptions, 296; Clone, 406; Default, 434.
  Los hooks actuales de event loop/window builder no sustituyen Clipboard.
  `eframe-0.36.2/Cargo.toml`, 234–237, activa `egui-winit/clipboard` incluso
  con default-features=false. Features Cargo aditivas no permiten apagarlo
  desde limn o RUNE. `egui-winit/Cargo.toml`, 62–66, incluye arboard y smithay.
- `smithay-clipboard-0.7.3/src/lib.rs`: new crea una conexión guest y worker,
  34–46; load espera `recv`, 52–60; Drop hace join, 96–102.
  `src/worker.rs`, 49 y 94, usa registry_queue_init y WaylandSource.
- `smithay-clipboard-0.7.3/src/state.rs`: dispositivos propios en
  `new_capability`, 299–303; selección con foco/serial, 112–173 y 505–532;
  DnD enter/motion/drop vacíos, 391–407. Todo esto es privado al crate.
- `smithay-client-toolkit-0.20.0/src/data_device_manager/data_device.rs`:
  el dispatcher acumula undetermined_offers antes del callback, 132–135, y
  destruye drag_offer anterior al Enter, 137–155. En `data_offer.rs` acumula
  Vec<String> en 194–195/343 y pierde Action pre-Enter en 207–213/278–282.
  Añadir sólo callbacks a esos handlers no conserva los límites/identidades.

Identidades para volver a comprobar estas referencias:

- egui-winit/lib.rs: SHA256 `c7adae2eca7b6e874db0af8776c8c87bc3a5b62e128eddc95a94802cb3eee9e4`.
- egui-winit/clipboard.rs: `571b22d4c85478a23bb2ba18448ecfa1d4559ef24ceacc17464e7c59630fe96e`.
- eframe/wgpu_integration.rs: `b776ccf1b1642229abce524a22fc2ec53b362be5bb97bbe326c56d0ca2a54934`.
- eframe/glow_integration.rs: `65a3542c285c387ab83e7983add4cb9969aba1416278e511cdcf99e2ec5fc76a`.
- smithay-clipboard/state.rs: `7c5e872387b7ebdbe061c667e7fb09ce3771e26572ae6a2a349fa6bdedf456e3`.

Root verificó por consulta de refs el tag oficial egui 0.36.2,
`49682f8baa058bf49e011035cfbd6e825f88a5ef`. La investigación original usó el paquete
local. Después, Root clonó ese tag en `egui-upstream-0.36` y creó el worktree
`egui-clipboard-hook`; las líneas anteriores siguen correspondiendo al paquete
instalado. El punto externo de propuesta es emilk/egui, no un parche local en RUNE.

## SDK#1 implementado localmente: inyección, independiente de RUNE

APIs implementadas en el commit local `ac2ff2fec18c637a8841bcd4b626a98de3324d72`;
no están publicadas upstream ni integradas en el pin RUNE:

```rust
pub trait ClipboardBackend: Send {
    fn get(&mut self) -> Option<String>;
    fn set_text(&mut self, text: String);
    fn set_image(&mut self, image: &egui::ColorImage);
}

impl Clipboard {
    pub fn from_backend(backend: Box<dyn ClipboardBackend>) -> Self;
}

impl State {
    pub fn new_with_clipboard(
        egui_ctx: egui::Context,
        viewport_id: egui::ViewportId,
        clipboard: clipboard::Clipboard,
        native_pixels_per_point: Option<f32>,
        theme: Option<winit::window::Theme>,
        max_texture_side: Option<usize>,
    ) -> Self;
}
```

`from_backend` no instancia arboard ni smithay. Los tres métodos actuales de
Clipboard despachan al custom cuando existe. `State::new` y Clipboard::new
conservan su conducta predeterminada; new delega al mismo constructor interno.
Sólo el constructor predeterminado consulta display_target para crear Clipboard;
new_with_clipboard no recibe ni consulta un handle de display. El bound Send
conserva esa posibilidad de uso del tipo; no exige Sync al backend.
No hay setter tardío que primero cree y después destruya un dispositivo.
El proveedor custom cubre también CopyImage: no se lo sustituye silenciosamente
por un no-op ni por un segundo backend del sistema. `epaint::Mutex::get_mut()`
permite acceder al backend mediante `&mut self` sin adquirir un lock. Clipboard
conserva Send+Sync y RefUnwindSafe, aunque el backend sólo requiere Send.
**El Clipboard poseído deja de ser UnwindSafe** por el borrado de tipo del backend;
es un cambio estático de API documentado, no compatibilidad total de auto-traits.

Se verificaron 11 pruebas de mutex epaint (35 filtradas) y la matriz egui-winit:
cinco con x11 sin defaults, tres con defaults y cinco con wayland sin defaults.
Son ejecuciones de configuraciones distintas, no trece pruebas únicas. También
pasaron Clippy estricto de ambos crates con todos los targets/features,
rustdoc estricto y rustfmt de los tres archivos propios. El mutante que
despachaba get pero devolvía None fue detectado; se restauró la fuente y se
repitieron los checks. Es sensibilidad de prueba, no un fallo heredado.
La [evidencia SDK#1](../docs/evidence/egui-clipboard-injection-2026-10-15.json)
conserva los hashes exactos de commit/snapshots, checks y limitaciones.

Las pruebas usan un backend sintético en RAM, sin ventana, compositor ni
portapapeles del SO. El trait sigue siendo síncrono: este incremento no prueba
pegado del sistema no bloqueante, Drop del backend ni interoperabilidad DnD.

## SDK#2 implementado localmente: fábrica en eframe

El commit local `df0b452d09c363c5e5ec7f7a45a69c37fd4fdf4c`, hijo de SDK#1,
implementa `NativeOptions.clipboard_factory: Option<ClipboardFactoryHook>`:

```rust
pub type ClipboardFactoryHook = std::rc::Rc<
    dyn Fn(
        egui::ViewportId,
        &std::sync::Arc<winit::window::Window>,
        &egui::Context,
    ) -> Result<
        egui_winit::clipboard::Clipboard,
        Box<dyn core::error::Error + Send + Sync>,
    >,
>;
```

La fábrica se invoca en UI, una vez por State realmente creado, con la ventana
del viewport correspondiente. Rc comparte el mismo cierre entre clones de
NativeOptions y entre renderers inmediatos; sus capturas no requieren Send ni
Sync. La fábrica puede clonar el Arc de la ventana para conservar su dueño real;
no debe crear ni renderizar viewports inmediatos recursivamente durante esa
llamada. Sin fábrica se conserva el camino predeterminado; una fábrica puede
delegar explícitamente al Clipboard predeterminado en otra plataforma.

Un Err nunca crea un backend de clipboard predeterminado. El error de raíz se
propaga como `Error::ClipboardCreation` con viewport y causa conservados, también
si aparece durante callbacks de UI o al procesar su salida en el mismo cuadro.
Un deferred fallido conserva un latch por instancia y queda sin State utilizable,
sin reintento por frame ni ventana parcial publicada. Si falla un immediate
antes de ejecutar su UI, el renderer señala `ImmediateViewport::on_failure`:
Context ejecuta esa UI una vez como `EmbeddedWindow`, con input y clipboard
existentes del padre. Mantiene la declaración del hijo para conservar su latch;
no construye un backend hijo ni interpreta el error como permiso para usar el
default. Si el callback ya produjo resultado, éste gana y no se repite embebido.
Un renderer que omite callback y señal sigue siendo un error de integración.

Una recreación explícita reinicia el latch. Glow descarta el State custom antes
de retirar la ventana al suspender; conserva la conducta previa sin fábrica.
Un error OS/GL posterior a State válido no se convierte en fallo de clipboard
ni vuelve a ejecutar la fábrica para ese mismo par State/ventana.

Son ocho archivos nuevos o modificados en egui/eframe, además de SDK#1. La
[evidencia SDK#2](../docs/evidence/egui-viewport-clipboard-factory-2026-10-15.json)
registra hashes y checks: 54 pruebas egui, incluidas cuatro de fallback Context;
nueve eframe con Glow+x11 sin defaults y nueve con WGPU/default. Las dos
configuraciones eframe se ejecutaron antes de ajustar sólo el harness negativo
de Context a `should_panic` y RAII; producción y demás pruebas quedaron iguales,
y las 54 egui se repitieron después. No se suman como pruebas únicas ni a RUNE.
Clippy estricto de ambos crates con todos los targets/features, rustdoc estricto
y formato de los ocho archivos propios pasaron sobre la fuente final.

Compatibilidad pública: los literales exhaustivos de `NativeOptions` necesitan
el campo nuevo o `..Default`; `ImmediateViewport` añade el callback prestado
público `on_failure`, que deben incluir sus constructores/desestructuraciones
exhaustivos. La pérdida de UnwindSafe del Clipboard poseído sigue siendo el
cambio documentado de SDK#1. La evidencia CPU original conserva su alcance
sin ventanas ni clipboard del SO; la QA nativa posterior tiene registro separado.
No hay push/PR SDK, cambio de pin o implementación RUNE en este incremento.

### QA nativa acotada de la fábrica sintética

El fixture propio quedó en el commit local
`0bff2dd4ada2a32b23f611488f3c7e7b99561d32`, hijo de SDK#2. WGPU y Glow
terminaron normalmente con exit 0 en aproximadamente 3,29 y 3,39 segundos,
respectivamente, sobre Weston 15.0.1 headless/Pixman y desktop-shell privados.
Ambos usaron el mismo ejecutable SHA256
`1866dad0ad142b626b30d0aa25429fdf654462bd16fd6557568cd6a7a8ce95be`;
no se inspeccionó el adaptador ni se acredita aceleración por hardware.

La fábrica se invocó una vez para ROOT, un deferred, un immediate y otro
immediate deliberadamente fallido. Se verificaron el Arc raíz contra AppCreator,
IDs/clases/padres de los hijos y WindowIds vivos distintos. El hijo fallido se
embebió en ROOT y devolvió un valor no Clone en al menos dos frames raíz,
con una sola llamada de fábrica y su Weak nativo expirado. Después se dejaron
de declarar hijos, se observaron sus Weak expirados, se pidió Close raíz y
se comprobaron los cuatro Weak expirados tras retornar run_native. Sólo se
usó el backend sintético en RAM; no se creó Clipboard::new ni se consultó una
selección del SO. Los errores raíz y deferred siguen cubiertos por CPU;
el fallo nativo deliberado fue únicamente el immediate.

El primer ensayo WGPU con kiosk queda como **parcial, no aprobado**: alcanzó
las cuatro llamadas y el fallback, pero un watchdog externo de 25 segundos
terminó la aplicación propia al no cerrar. La hipótesis de falta de redraw por
oclusión no es causalidad probada; la repetición también añadió hitos acotados.
El plazo interno de diez segundos depende de ticks logic, por lo que sigue
siendo obligatorio el watchdog externo ante starvation o bloqueo de arranque.

La [evidencia nativa](../docs/evidence/egui-clipboard-factory-native-2026-10-15.json)
retiene los tres intentos y remite a un manifiesto de 105 archivos verificados.
El ejecutable probado se conserva gzip, con SHA256 y tamaño de la descompresión
verificados; el commit posterior sólo ajustó dos líneas de documentación sobre
el deadline. Se conservaron diagnósticos DBus/FUSE/display y EGL/MESA; la salida
externa WGPU fue condensada/truncada por RTK y sus archivos app/compositor están
retenidos. Es orquestación CLI sin ID REA, no QA libre de diagnósticos. Sólo se
acredita el cierre de los procesos propios y sus hijos de compositor registrados.
No prueba clipboard real, DnD, owner compartido, Hyprland, pegado asíncrono ni RUNE.

## SDK#3 implementado localmente: identidad y marcador de pegado diferido

El commit local `c152f4b7692da7496a74d524755bb32f74db6805`, hijo del fixture
`0bff2dd4ada2a32b23f611488f3c7e7b99561d32`, añade `egui::PasteRequestToken`.
Su payload privado usa UserData fresco con valor unitario; cada `new()` crea
una identidad distinta, Clone conserva Eq/Hash por identidad y no es Copy.
Default y replay deserializado quedan vacíos: serde descarta la identidad y no
revive autoridad. Crear/clonar un token no reserva recursos ni autoriza lectura.

```rust
pub enum ClipboardPaste {
    Immediate(Option<String>),
    Deferred(egui::PasteRequestToken),
    Rejected,
}
```

`ClipboardBackend::request_paste` tiene implementación predeterminada
`Immediate(self.get())`, que **sigue siendo síncrona y puede bloquear**.
`Clipboard::request_paste` delega al backend seleccionado sin combinar resultados
con el sistema; `get` y `State::clipboard_text` legacy siguen disponibles.
Un custom puede devolver Deferred después de reservar capacidad sin leer datos,
o Rejected sin getter ni fallback. SDK#3 define ese contrato pero no implementa
el servicio propietario de la reserva.

`State::request_paste()` conserva normalización exclusiva CRLF→LF y filtro de
vacío para Immediate. Deferred no vacío añade `Event::PasteRequested { token }`
en su posición de entrada; tokens vacíos y Rejected no añaden nada ni llaman
get. Los widgets egui no insertan ese marcador como texto. Ctrl+V y los dos
`ActionRequested::Paste` de WGPU/Glow usan ahora el mismo helper. Las tres rutas
ya normalizaban CRLF y filtraban vacío; se preserva esa conducta, no se acredita
una corrección de regresión previa. La variante nueva de Event requiere adaptar
matches exhaustivos externos.

La [evidencia SDK#3](../docs/evidence/egui-deferred-paste-marker-2026-10-15.json)
conserva seis fuentes/snapshots y un manifiesto de 23 archivos. El RED de API
no ejecutó cuerpos porque faltaban las capacidades nuevas; después pasaron
tres pruebas token/evento y seis de despacho. La matriz completa registró 60
egui con persistence; egui-winit 11 x11 sin defaults, 9 default y 11 wayland
sin defaults; eframe 9 Glow+x11 sin defaults y 9 WGPU/default. Son configuraciones
repetidas y no se suman como pruebas únicas ni a RUNE. Pasaron Clippy estricto
de los tres crates con todos los targets/features, rustdoc estricto y formato
de los seis archivos propios. Debug info/incremental se desactivaron para
recuperar caché generada; no se desactivaron debug assertions ni semánticas de test.

Es verificación CPU/API, sin I/O de clipboard ni runtime de pegado diferido.
La QA nativa SDK#2 permanece histórica y sólo prueba su fábrica sintética.
SDK#3 por sí solo no implementa owner/ACK/completion ni destino limn, y no
aprueba un pin o consumidor RUNE. El destino se verificó después como un
incremento independiente de limn, descrito abajo. Tampoco cambia manifests/lock ni publica el SDK.

## Propietario compartido posterior en limn

La base DnD de este diseño ya existe en PR #62. A1/A2 y B/C posteriores tienen
checkpoints internos: B/C verificó cinco recorridos del consumidor real en su
candidato v1 histórico sin formatear. La fachada pública y el alcance entrante
v4 dirigido ya tienen aceptación local de 184 pruebas, checks estrictos y smoke Weston;
la entrega continúa en PR #63 Draft. Primary, copy y el backend clipboard del
SDK/RUNE siguen pendientes. La galería ilustra lifecycle/texto sintéticos;
los dos smoke públicos son pruebas separadas, con scope Weston privado.

La fábrica permite que limn instale otro backend sin interceptar userdata.
Ese backend unificado todavía debe implementarse y revisarse en limn: la
fábrica sola no convierte dos dispositivos existentes en uno.

- Un owner explícito por conexión real, compartido por los backends de todos
  los viewports. Un solo `wl_data_device` por seat; no crear otro por ventana,
  endpoint DnD o recreación de State. No mapa global basado sólo en raw pointers.
- El owner raw Dispatch clasifica ofertas Selection/DnD antes de retenerlas.
  Mantiene MIME como flags acotados, Action exacta incluso pre-Enter y el
  cap total de ocho ofertas, incluidas retenidas/activas/sin clasificar.
  Las ofertas de texto no pasan por el descarte Selection del receiver actual.
- Enter enruta por ObjectId completo de la superficie a una inscripción
  explícita. Device/seat/oferta siguen identificados durante Motion/Drop/Leave.
  Superficie no registrada se rechaza, sin importarla en la ventana enfocada.
  La inscripción tiene su propio Coordinator/control/mailbox/epoch y usa el
  mismo codec, pipe y protocolo de decisiones; no una segunda máquina de dominio.
- Busy B no reemplaza A; una decisión o callback de otra superficie/época no
  puede leer ni Finish A. El alcance inicial no promete drops concurrentes:
  un único pipe DnD activo en el owner basta, con rechazo explícito del segundo.
  Selección de texto sigue separada de esa admisión y no se convierte en Drop.
- Copy/primary selection mantienen seat, foco y serial reales de teclado/puntero;
  no se elige el primer seat ni se infiere intención del cursor global.
  El texto y las imágenes no pasan a paths, Core ni al actor de uploads.
- Conservar los límites que PR #62 ya implementa: cuatro permisos worker y
  cuatro superficie globales separados, un reaper, 64 GetRegistry no renovables,
  cuatro seats, ocho ofertas totales y cada buzón 5 fiables + 1 hover;
  URI/timeout permanecen configurables. Los endpoints reservan su superficie
  sin duplicar JoinHandle/owner. Clipboard futuro no autoriza registros ni
  colas ilimitados, ni cambia esos cupos implícitamente.
- Mantener owner de la conexión/ventanas hasta cleanup y join fuera de UI;
  cerrar un viewport retira su inscripción, no el device compartido por los demás.
  Último cierre pasa Closing→Closed sólo tras limpieza real. UI nunca hace join,
  SDK ni Drop final pesado bajo un guard o durante un callback.
- Guest pump conserva Ping + readiness + tick máximo 20 ms, sin otro lector
  del socket. Se conserva el límite honesto: atasco/flood interno SDK puede
  retener Closing y los recursos; no se promete teardown de pared duro.

## Pegado no bloqueante y cierre: contratos separados, obligatorios

El trait síncrono inicial refleja el API existente y permite probar la fábrica.
No se propone que el backend limn llame a smithay Clipboard::load/Drop: esperar
recv o join en UI violaría el contrato de RUNE. Tampoco sirve devolver texto de
una selección anterior o leer/precachear el portapapeles sin petición de pegar.

### Reserva y completion: texto entrante aceptado; integración SDK/RUNE pendiente

**Reserve/ACK/claim privados de A1 ya tienen consumidor real interno B/C.**
A1 comparte un único Incoming DnD/texto, congela el snapshot lógico y valida
parent/SurfaceId, vida, foco/epoch y plazos; cancelación y limpieza usan ID exacto.
Token, tres respuestas y marcador ya existen en SDK#3. B/C ya liga Selection,
lector/runtime y publicación al retiro real de su watcher para cinco recorridos
internos. La fachada owner/request pública ya se implementó y verificó después;
el alcance entrante v4 dirigido ya pasó matriz de 184 pruebas, entrega local estricta y dos
smoke Weston. PR #63 Draft sigue separada del backend SDK funcional necesario
para conectar Deferred al consumidor RUNE.
Body/staging/publicación/poll/TTL ya existen como control interno A2: un slot fuerte del worker, String RAW único, poll/error
únicos y revalidación fresca. Destruye bodies fuera de guards antes de liberar
Incoming; Drop vacía el slot antes de soltar su Arc fuerte permanente; un Arc
temporal de UI sólo puede quedar último con el slot vacío. A2 no incluye
todavía protocolo, decoder ni certificación del descriptor de texto real en
su propio checkpoint; esa conexión se verifica después en los recorridos B/C.
La aplicación retendría el mismo owner que captura la fábrica. Reserva, ACK y
completion pasarían sólo por él; no se propone otro ACK genérico del SDK,
target egui ni un poll genérico que importe texto.

La integración conservará la identidad fresca ligada al endpoint y vida de
State, con un único Incoming compartido en el owner, cancelación/Drop y plazo
absoluto. Un handle obsoleto no autoriza otro State y ACK sólo consume la reserva
vigente una vez. El claim de metadata A1 revalida el snapshot antes de autorizar
el intento; B/C lo liga a la selección/protocolo reales. La matriz dirigida de 184 pruebas
ya verifica cambios de oferta y contexto sin sustituir la identidad del request.
No hay lectura/receive antes del ACK. El embedded usa el endpoint real del padre.
La completion queda tokenizada hasta comprobar y editar el destino atómicamente,
sin convertirla en un Paste genérico para el viewport activo. Selection no usa
Finish ni SetActions de DnD.

El destino limn ya está **implementado y disponible en el pin RUNE `6ae42859`**:
`capture_paste_target`, `invalidate_paste_targets` y `UiEvent::PasteCaptured`,
con PasteTarget opaco de un solo uso y EditStateToken del buffer real. El
[contrato de destino](captured-paste-target-contract.md) y su
[evidencia](../docs/evidence/limn-captured-paste-target-2026-10-15.json) registran
core `9316d75d`, HEAD `eb6ea299` y PR #56 Draft OPEN con CI SUCCESS. Los seis
grupos Tree y tres Editor están incluidos en 1571 pruebas limn, cinco ignoradas,
20 suites; las tres pruebas de galería son separadas. La QA de cuatro variantes
dark/light y normales/square pasó sólo para texto sintético en Weston headless
privado con fake-seat. No solicita clipboard I/O; eframe puede inicializar su
default. Se retienen el intento sin seat y las dos v1 con defectos visuales;
el cambio a `ñ`/`λ` no aporta cobertura CJK. Nada se suma a RUNE 1120/18 Mock.

`PasteTo` legacy por ID/layout sigue sin cubrir A→B→A. El target nuevo valida
identidad/época, foco/selección y estado editable observado, incluidos IME/Vim,
con validación y edición bajo el mismo try_lock para CodeEditor. No contiene
texto, buffer ni un owner de clipboard. RUNE todavía debe añadir epochs de
cuenta/contexto/canal/borrador, modales y vida de sesión, e invalidar por cortes
invisibles al árbol. El foco de memoria egui no sustituye ese destino.
Antes de ACK debe existir layout fresco, foco resuelto y control editable vigente;
si falta alguno, no se acusa ni se lee. El permiso es de edición de texto en
ese destino: no reutiliza SEND/ATTACH como autorización genérica de pegado,
y no inicia envío ni preparación de archivos.

La auditoría histórica del host 1120 identificó un prepass host_keys y Focus
aplicado tarde. Ese tramo ya se corrigió: el pin propio `6ae42859` integra el
bridge y [handle_input:1246](../apps/desktop/src/main.rs:1246) recorre todos los
índices del mismo snapshot normalizado. Aplica la Key/modifiers de cada índice,
Intents previos, layout y foco antes del siguiente; un foco inexistente limpia
el anterior. WindowFocused se reenvía en su posición incluso closing y hay
fallback input.focused sólo cuando el snapshot no trae callbacks de foco.
Los antiguos main 1277/1465/816/1887 eran referencias de esa base, no actuales.

El [contrato del bridge](limn-egui-ordered-input-contract.md) y su
[evidencia upstream](../docs/evidence/limn-indexed-input-2026-10-15.json) siguen
separados del [checkpoint RUNE](../docs/evidence/rune-indexed-input-host-2026-10-15.json).
RUNE pasó 1128 pruebas, seis ignoradas y 25 suites; fmt, Clippy estricto y build
con all-features correctos. Ocho
tests App incluyen modifiers, navegación/ABA, foco, closing y query del switcher.
La QA final Mock en Xvfb privado verificó borradores A/B y query limpia; la
primera, parcial, descubrió el input obsoleto que se corrigió con SetInput vacío
antes de Focus. Cinco imágenes finales y cinco parciales quedan separadas de
las 18 históricas. La ejecución nativa usa pausas, no acredita batching físico.

El seam API está disponible, pero PasteRequested del SDK no está conectado a
RUNE. A1 implementa reserva/ACK/claim privados de metadata y A2 body/completion
internos; B/C añade el consumidor permanente real para sus cinco recorridos.
La fachada pública entrante tiene aceptación dirigida v4, checks locales y smoke
Weston; su PR #63 Draft no conecta el SDK ni cambia el pin RUNE. Los índices no reparan los límites
whole-frame de puntero/click/modifiers ni aportan cronología del SO. Los tests futuros deben combinar captura con epochs de cuenta/canal,
borrador/modal/reconexión/endpoint: el retorno a los mismos IDs no revive una
completion. Closing, layout no fresco o foco sin resolver producirán cero ACK
y cero lectura. En embedded se usa el endpoint real del padre. Límites de
texto y plazos entrantes ya están fijados y validados en A1. A2 implementa la
publicación interna bajo precondición post-watcher; B/C conecta lectura/decoder
y retirada del watcher de texto real para los recorridos probados. Ese alcance
entrante y su matriz dirigida ya están aceptados; sigue pendiente la integración
SDK/RUNE y la entrega upstream final de PR #63.

El owner deberá fijar por separado selección saliente, fuentes y transferencias
de copy, y sus plazos; texto entrante tiene el contrato v4 enlazado arriba.
No se reutiliza el límite de bytes URI ni se heredan los Vec/colas sin límite
de smithay. Sin esos números y pruebas no se aprueba el backend completo.
El cierre usa el reaper/retención
ya acordados: no invoca Drop::join de smithay en UI ni simula Closed temprano.

## Secuencia RED y revisión

SDK#1, SDK#2 y SDK#3 completaron sus boundaries y pruebas CPU sin compositor
ni portapapeles real. La QA de ventanas SDK#2 usa sólo clipboard sintético;
backend clipboard completo del owner y consumidor asíncrono RUNE siguen
pendientes; la API pública de texto entrante ya tiene checkpoint local.
A1 tiene checkpoint de metadata Reserve/ACK/claim y A2 de body/completion internos. El checkpoint B/C posterior añade el consumidor real,
con cinco recorridos de protocolo/FD verificados; el público/runtime posterior
añadió la fachada y siete grupos en174; el checkpoint 184 posterior aceptó la
cobertura dirigida v4 y sus checks/smoke locales, sin promesa cartesiana.
La base DnD del owner tiene checkpoint separado en PR #62; no habilita esas operaciones. El destino limn
completó después sus pruebas y galería sintética como incremento separado; el
bridge indexado se verificó externamente antes de su consumidor orden/foco
RUNE posterior:

1. Trait y constructor tuvieron API boundary RED como capacidad nueva, seguido
   de GREEN. SDK#2 y SDK#3 también tuvieron RED de API para capacidades nuevas y GREEN;
   no se presentan como regresiones funcionales heredadas.
2. SDK#1 verificó con backend espía que get/set_text/set_image se despachan
   exactamente una vez al custom y no al default. `get→None` también conserva
   ese None sin caer al sistema. Un backend Send pero no Sync prueba el bound.
   El constructor custom no requiere un display; no se fabrica un Arc<Window>
   inválido para simular una ventana en las pruebas unitarias.
3. Selección de fábrica antes de default: custom→cero creaciones default;
   Err→cero default; ausencia de fábrica→conducta default. Usar seam unitario
   de creación, no conectarse al clipboard para contar side effects.
4. Clonar NativeOptions conserva la misma fábrica. WGPU root/secundario/inmediato
   y Glow consumen el seam probado de selección y el latch por instancia.
   Tests prueban custom/default/error y fallo persistente con IDs de viewport;
   builds de ambos renderers y revisión de llamadas verifican ese wiring.
   Esos tests CPU no son ejecución de ventanas. La QA posterior descrita arriba
   usa ventanas reales propias y backend sintético, sin Arc<Window> falsificado.
5. Casos default y plataforma no Wayland mantienen CopyText/CopyImage/paste;
   se validan features clipboard/wayland/x11 y ambos renderers. Un mock no
   acredita interop nativa, sólo la inyección y el orden de inicialización.

Después, en porciones separadas del owner y del pegado asíncrono:

1. Servidor Wayland privado: dos viewports en la misma conexión crean un solo
   data_device por seat. Enrutamiento first-device conserva selección de texto
   y DnD sin depender del orden de inicialización. Seat y superficie ajenos
   no reciben payload de otro endpoint; cierre de A deja B utilizable.
2. Selection y DnD intercalados, MIME flood/cap ocho, Action antes de Enter,
   Copy estricto y A/B callbacks. Reusar las pruebas codec/claim/EOF/terminal
   del receiver; ningún handler SCTK acumula antes de aplicar los topes.
3. A→B durante lectura o paste pendiente: respuesta vieja no se importa ni
   pega en B. Accept/Cancel/epoch conservan arbitraje, deadlines y un solo Finish
   exacto; nunca Finish para selección de texto.
4. Texto privado sintético: copiar/pegar tras EOF, rechazo de exceso/plazo,
   escritor sin EOF, petición repetida y cierre. UI retorna inmediatamente,
   no usa caché vieja ni carga selección sin gesto; límites quedan comprobados.
5. Cleanup con worker/SDK detenido conserva owner y cupo, Drop no hace join UI;
   sólo tras cleanup/join reales se libera. Preservar las pruebas existentes
   de TLS/panic/reaper y probar que un endpoint no destruye recursos compartidos.

La demo del owner compartido integrado con dos fixtures propias es una porción
posterior a GREEN y revisión independiente. Publicación upstream y cambio de pin se registran por
separado. Repetir QA positiva y negativa en
compositores concretos; no extrapolar Weston a Hyprland. La integración del owner en RUNE y los archivos pegados siguen pendientes;
el orden/foco del host ya está verificado. Esta porción
no los implementa ni adelanta los puntos 08/10.
