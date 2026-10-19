# Comportamiento de la interfaz: evidencia de Serein y contrato para RUNE

Revisión de fuente realizada el **2026-10-14** sobre el checkout limpio de
[Serein, commit `d1af883c9d7dda90d1261a2d15fc852a5445204d`](https://github.com/ViceVerse-cz/Serein/tree/d1af883c9d7dda90d1261a2d15fc852a5445204d).
Las rutas de Serein son relativas a `references/serein/`. No se compiló ni
ejecutó ese proyecto, no se inspeccionó Discord oficial ni se accedió a una
cuenta. No se copió código ni recursos gráficos.

- **Fuente [C]:** regla leída en código. Confianza alta sobre la intención
  de esa revisión; no demuestra el resultado visual ejecutado.
- **Test [T]:** aserción encontrada en un test de Serein. Se leyó, pero no
  se ejecutó durante esta investigación.
- **Propuesta [I]:** interpretación o contrato sugerido para RUNE; necesita
  sus propios tests y evidencia del binario.

Las medidas siguientes son **unidades lógicas de egui**. No son píxeles
físicos medidos en una captura ni dimensiones normativas de Discord. Zoom,
escala del monitor, fuente y tema pueden alterar el resultado observado.
La evidencia runtime de RUNE se registra por separado en [REA.md](REA.md).

## Barra de acciones al pasar sobre un mensaje

| Aspecto | Evidencia de Serein |
| --- | --- |
| Posición normal [C] | `crates/ui/src/timeline.rs:3928`: ancho `30 × número de acciones`; altura 28; borde superior 10 unidades por encima del borde superior de la fila. Su origen X es el mayor entre el borde izquierdo de la fila y `derecha − 16 − ancho`. |
| Tamaño de controles [C] | `timeline.rs:1138` usa botones de 28; el espacio entre items es 2 (`:3943`). Fondo elevado, radio 6 y borde interior de 1 (`:3947`). |
| Mensaje eliminado [C] | `timeline.rs:3844`: barra reducida de 36×28, con menú; origen en `derecha − 46`, `arriba − 10`. No es la misma fórmula de todas las acciones. |
| Participación en layout [C] | `timeline.rs:3936`: la barra se crea como hijo superpuesto. El comentario explicita que no debe cambiar el wrapping ni la altura cacheada de la fila. |
| Continuidad del hover [C] | `timeline.rs:3783`: el puntero dentro de la barra conserva activa su fila. La barra de otro mensaje impide activar la fila que queda debajo de ella. Un popup abierto o foco de teclado dentro de la barra también la retiene (`:2621`). |
| Selección y gestos [C] | `timeline.rs:2571`: el hover se suspende durante scroll, arrastre y el gesto de selección retenido. Las acciones responden a permisos; Shift cambia cuáles aparecen (`:3912`). |
| Evidencia de regresión [T] | `timeline.rs:7259`, `hover_actions_keep_layout_stable_and_support_keyboard_reply`, configura anchos 900 y 360, comprueba ancho 240 en un caso de ocho acciones, compara la caché de alturas antes/después del hover y ejercita responder y el menú. |

**Límite de la evidencia:** la fórmula limita X por la izquierda, pero no
reduce el ancho de la barra a la fila ni limita explícitamente Y al viewport.
No se puede deducir que todas las combinaciones de Shift, permisos, escala
y ventana estrecha estén dentro de los límites. La barra se crea dentro
del scroll; el recorte efectivo también depende del árbol de egui. No se
midieron esas esquinas ejecutando Serein.

**Aplicación en limn [I]:** el alto del mensaje debe depender sólo de su
contenido, no de si se muestran acciones. RUNE actualmente arma la fila
con `Element::stack` en `crates/rune-components/src/message.rs:262`; aunque
limn mide un stack por el mayor de sus hijos, la barra de RUNE declara
altura cero y no aumenta la altura medida de la fila. Esto no demuestra
por sí solo que sus bounds visuales o su recorte sean correctos.
`Element::overlay` distingue el layout de la base del contenido flotante,
pero su recorte exterior es el viewport de la ventana
(`limn/src/element.rs:2241` en el checkout local). Usarlo exige conservar
los límites del timeline para no invadir el encabezado o el composer.

El contrato para RUNE debe probar: misma altura de fila y posición del
scroll antes/después del hover; transición del puntero fila→barra sin
parpadeo; foco de teclado y menú que conservan la barra; barra contenida en
el rectángulo disponible en primera/última fila y ventanas estrechas.
Si no entran todas las acciones, el menú de desborde debe absorberlas.
Estas son propiedades a implementar y medir, no una orden de copiar las
dimensiones de Serein.

## Separación entre timeline y composer

| Aspecto | Evidencia de Serein |
| --- | --- |
| Reserva del composer [C] | `crates/ui/src/lib.rs:4620`: panel inferior separado antes de dibujar el timeline; sin línea separadora. Márgenes internos izquierdo/derecho 16, superior 2 e inferior 8 (`:4638`). El comentario vincula el margen inferior con la tarjeta de cuenta. |
| Espacio al final del historial [C] | `crates/ui/src/timeline.rs:2382`: el scroll reserva `8 + OVERLAY_HEIGHT`; `crates/ui/src/typing.rs:119` define altura 22. Son 30 unidades de contenido al final, incluso si nadie escribe. Se añaden al final del scroll (`timeline.rs:4241`), no entre cada mensaje. |
| Indicador de escritura [C] | `timeline.rs:4473`: se pinta en las últimas 22 unidades del viewport del timeline, con 16 de margen lateral. Aparece por encima del composer, sin modificar la altura reservada. |
| Final visible [C] | `timeline.rs:2497`: follow-latest usa el alto disponible del cuadro actual para calcular el offset. Esto evita esperar un cuadro después de cambiar el tamaño del panel superior. |
| Acabado visual [C] | `timeline.rs:4421`: rampa de color en la parte inferior, 20 unidades sin escritura o 30 con ella; no agrega espacio de layout. Su tratamiento cambia con transparencia y con el lector situado al final. |
| Evidencia de regresión [T] | `timeline.rs:4870`, `resizing_call_stage_keeps_latest_message_at_the_composer`, ejercita el redimensionamiento de una región superior. `typing.rs:288`, `bounded_names_and_expiry_render_without_idle_repaints_or_scope_leaks`, compara la altura reservada antes/después de caducar el indicador. |

No debe sumarse todo lo anterior y presentarlo como la distancia exacta
entre el último glifo y el borde del composer: la fila tiene métricas y
espaciado propios; el panel y el contenido del scroll son capas distintas.

**Aplicación en limn [I]:** reservar una única zona estable para escritura,
decidir explícitamente si va encima o debajo del composer y evitar sumar
una reserva dentro de la lista y otra debajo por accidente. RUNE hoy
reserva 20 unidades más un gap de 2 **debajo** de la tarjeta
(`crates/rune-components/src/composer.rs:222`); ésa es una elección distinta
de la fuente. El contrato útil es que activar/desactivar escritura no
salte el último mensaje, y que crecer el composer mantenga el final visible
sólo cuando el lector ya seguía el final. Al leer historia debe conservar
su ancla.

## Composer vacío, de una línea y multilínea

**Vacío y una línea [C].** `crates/ui/src/lib.rs:3291` construye una tarjeta
con márgenes horizontales 10, verticales 6 y radio 8. Los iconos miden 28
(`:3339`, `:3385`). El editor es multilínea desde el inicio, con una fila
deseada, ancho flexible, altura mínima igual a `interact_size.y` y
alineación `LEFT_CENTER` (`:3624`). El placeholder vacío se maqueta como
una sola línea truncada (`:3603`), evitando que el nombre largo del canal
haga crecer la tarjeta.

La estructura sugiere una tarjeta nominal de unas 40 unidades cuando los
iconos de 28 dominan la altura, pero **eso es inferencia [I]**, no una
medición: texto, estilo y escala pueden requerir más. El hecho verificable
es que el editor y los controles comparten la alineación central de su fila.

**Multilínea [C].** El editor crece dentro de un scroll cuya altura máxima
es la mitad del viewport (`lib.rs:3581`). Los controles no quedan siempre
en la primera línea: cuando el alto alcanza `3 × 28 + 8 = 92`, se apilan
en columnas alineadas abajo (`:3308`, `:3698`). Para volver al modo de una
fila exige que el editor baje a `interact_size.y + 1`; entre ambos umbrales
conserva el estado anterior. Esta histéresis evita alternar modos porque
el ancho ganado al apilar cambia el wrapping.

La franja de respuesta/edición se une al editor retirando el gap automático
(`lib.rs:3286`) y comparte el ancla de autocompletado (`:3300`). El popup
incluye la franja en sus límites para no cubrirla. No se encontró en esta
revisión un test específico que mida las tres alineaciones del composer;
los tests de edición/IME encontrados prueban otras propiedades.

**Aplicación en limn [I]:** conservar un único cálculo de línea, padding y
alto mínimo para texto vacío y texto escrito. RUNE usa auto-grow y fija sus
controles en la primera línea (`crates/rune-components/src/composer.rs:112`);
no necesita adoptar el apilado de Serein para corregir un descentrado.
Primero deben compararse los rectángulos del placeholder, texto, caret e
iconos con borrador vacío, una línea y varias líneas, a escala normal y
aumentada. Sólo si se adopta un cambio de distribución por altura conviene
guardar un modo con histéresis y validar que no oscile. Las capacidades que
falten deben agregarse a limn según [LIMN_INTEGRATION.md](LIMN_INTEGRATION.md),
sin incorporar egui a los componentes de RUNE.

## Contadores en sidebar y rail

**Semántica [C].** `crates/ui/src/categories.rs:822` usa menciones en canales
de servidor y mensajes sin leer en DMs. Un foro usa un indicador de nuevas
publicaciones separado; un canal sin visibilidad no muestra conteo.
El nombre cambia de énfasis según selección, hover y no leído (`:895`).
El tooltip reconoce que un contador puede ser un límite inferior (`:1209`):
no lo presenta como una medición exacta cuando faltan datos.

**Geometría compartida [C].** `crates/ui/src/notifications.rs:96` dibuja un
badge de alto 18; ancho 18 para un dígito, 23 para dos y 29 para `99+`.
El fondo tiene radio 9 y un anillo exterior de 2 usando el color de la
superficie. El número blanco usa tamaño 11,5 y peso semibold; el tamaño del
texto queda fijo mientras entra la animación. La fila de canal reserva 34
unidades cuando hay badge (`categories.rs:923`), además del espacio de otros
controles, para truncar el nombre antes de la marca. Su centro está a 20
unidades del extremo derecho, desplazado por los otros controles (`:1178`).
Las filas del sidebar miden 34 en servidor y 44 en DMs (`:616`) y se
virtualizan con `show_rows` (`:629`).

**Rail [C].** `notifications.rs:7` define ancho 58, tiles de servidor 40 y
avatares DM 42. El conteo se centra en la esquina inferior derecha menos
(8,8), sólo si es mayor que cero (`:171`). La pastilla lateral tiene alto
7 si hay actividad sin leer, 17 en hover y 34 seleccionada (`:157`); toma el
máximo de esos estados. El indicador de voz ocupa la esquina superior para
no superponerlo con menciones (`:208`). Las carpetas cerradas suman conteos
con saturación (`crates/ui/src/guild_folders.rs:552`).

**Estado y coste [C/T].** `notifications.rs:25` invalida la caché de conteos
por generación/revisión/conexión/llamada, no por cada cuadro. Tiene un test
de reutilización y cambios de lectura, permisos y eliminación (`:502`),
leído sin ejecutarlo aquí.

**Aplicación en limn [I]:** separar badge numérico, estado no leído y
selección; anclar el badge a la caja del icono, no al ancho de toda la
columna. Su tamaño visual debe incluir el anillo al comprobar recortes.
RUNE ya tiene `sidebar::count` y una composición para la esquina inferior
del rail (`crates/rune-components/src/rail.rs:191`). Conviene probar conteos
1, 9, 10, 99 y 100, nombres largos, selección y hover, y verificar que el
número y el anillo queden completos sin alterar la altura de la fila.
No conviene fijar la misma altura total a texto, padding y anillo sin
comprobar qué caja está midiendo limn.

## Licencia y alcance de la validación

`references/serein/Cargo.toml:10` declara `MIT OR Apache-2.0` y
`crates/ui/Cargo.toml:5` hereda esa licencia. Los iconos y otros activos
mantienen avisos propios, detallados en [SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md).
Esta investigación conserva sólo descripción de comportamientos, medidas
y rutas: no importó código, atlas, marcas o sonidos.

No se ejecutaron pruebas Cargo de Serein ni se midió su rendimiento en esta
tarea. Los tests citados acreditan que la referencia contiene esas
aserciones; no se afirma que hayan pasado. Tampoco constituyen evidencia del
cliente oficial de Discord. Las correcciones de RUNE necesitan sus propios
fixtures de layout/interacción y comprobación del artefacto mediante REA;
una captura estática por sí sola no demuestra continuidad del hover, foco,
estabilidad del scroll o comportamiento con múltiples eventos.

## Correcciones implementadas en RUNE (2026-10-14)

- `sidebar::count`: se quitó el padding vertical que dejaba menos espacio
  que la altura del texto. Se mantienen cápsulas compactas de 17 puntos,
  con margen horizontal y el número centrado, incluidos `99+` y rail.
- `composer`: mínimo real de 48 puntos y un único cálculo de primera línea.
  Texto vacío, borrador y controles quedan centrados; crecer a varias líneas
  conserva los controles en la primera. No se amplía toda la UI.
- `view`: 8 puntos estables entre timeline y composer; el área de hover
  termina antes de esa separación, también al crecer el editor.

Las regresiones se probaron con rectángulos/hit-testing, nombres largos,
anchos 320/600/1100, temas claro/oscuro y métricas por defecto/Ubuntu usadas
por la aplicación. Los tests se observaron fallar antes de los cambios y
pasar después. La comprobación nativa y sus límites figuran en [REA.md](REA.md).

## Scroll del chat con los controles de limn

El chat usaba un override de RUNE inspirado en la referencia: thumb de
9 puntos, rail de 17, sin flechas y superpuesto sobre los mensajes. Se
reemplazó por `ScrollbarAppearance::default()` de limn, cambiando sólo el
tinte al color secundario del tema. La galería de limn lo muestra en
`crates/limn-gallery/src/pages/surfaces.rs`, sección `scroll_areas`: rail
reservado, thumb fino y controles en los extremos.

Las filas reservan el ancho del rail para que el texto y las acciones no
queden debajo de sus controles. El scroll sigue siendo una
`ScrollArea::measured_list`, con las mismas claves de mensaje, alturas
medidas y `follow_end(true)`. No se cambia el anclaje de historial ni la
virtualización. La regresión de layout comprueba los controles nativos,
que la flecha desplaza el timeline y que los botones de mensaje quedan
fuera del rail; la validación se registra junto con el resto de la entrega.


## Bienvenida y acceso nativo

La referencia de Serein (`apps/desktop/src/main.rs`, bienvenida, y
`crates/ui/src/design.rs`) separa una bienvenida nativa con una única acción
del acceso posterior. RUNE conserva esa división: marca original centrada,
título de 24 puntos, tarjeta de hasta 480 y acción de 44, usando los tokens
del tema. Onyx mantiene su fondo oscuro con una superficie elevada para
que la tarjeta sea legible. No se agregan campos de credenciales ni un QR
que no tenga un flujo implementado.

`rune-components::auth` compone la página; `rune-ui::auth` conserva las
transiciones y acciones de autorización. Los estados de espera mantienen
Cancelar, los errores completos son desplazables y el foco sigue los
controles de limn. El contenido se adapta a ventanas estrechas y bajas;
los tests cubren los tres temas, la entrada por teclado y cancelación.
No se cambió el proveedor ni se copió código o recursos de Serein.
