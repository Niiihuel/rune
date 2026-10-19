# Diseño general: una interfaz compacta con limn

RUNE conserva su lenguaje visual compacto con limn. Discord y Serein sirven
como referencias de comportamiento —jerarquía, foco, hover y separación de
zonas—, no como un diseño que haya que reproducir píxel a píxel. Esta es la
preferencia explícita del dueño en la revisión del 2026-10-14.

Las medidas de abajo documentan el punto de partida histórico tomado de
capturas de Discord (tema «Ónix», zoom 67 %, contenido de 1361×797 px
lógicos). Las pruebas de columnas en `crates/rune-ui/src/tests/layout.rs`
conservan esas proporciones mientras las correcciones de interacción tienen
sus propios contratos. No son evidencia de dimensiones internas de Discord.

## Contrato de interacción actual

- Composer de una línea de 48 px, con texto e iconos centrados verticalmente.
  Crece con el texto; los controles se mantienen junto a la primera línea.
- Separación de 8 px entre el viewport del timeline y el composer. Esa
  franja no activa mensajes ni muestra sus acciones al pasar el puntero.
- Las acciones superpuestas no aumentan la altura ni cambian el wrapping
  del mensaje. La navegación por teclado conserva acceso a las acciones.
- Las insignias de menciones mantienen sus números centrados dentro de la
  cápsula, con espacio horizontal para `99+` y sin padding vertical que
  reduzca el alto disponible para el texto.
- Los textos, iconos, colores y recursos originales de RUNE se conservan.
  No se añade un control sólo por aparecer en la referencia: su acción debe
  existir y respetar las capacidades del proveedor.

Evidencia de la referencia y límites: [UI_BEHAVIOR.md](UI_BEHAVIOR.md).

Todas las coordenadas son px lógicos. Las «y» de la captura cuentan su barra
de título (34.5 px); RUNE no la dibuja (ver diferencias), así que lo que
cuelga del borde superior queda 34.5 px más arriba y lo que se apoya en el
borde inferior queda igual.

## Medidas

| Zona | Captura | RUNE |
|---|---|---|
| Barra de título | alto 34.5, dibujada por la app | no hay: la pone el sistema |
| Carril de servidores | x 0–85.5 + divisor 1 px | igual (`rail::WIDTH` = 86.5) |
| Inicio / servidor / «+» | 42×42 en x 21.5, radio ≈10 | igual |
| Paso entre servidores | ≈52–53 | 52.5 |
| Separador bajo inicio | 33.5×1 en x 26, ≈12 sobre el primer servidor | igual |
| Píldora del carril | 4.5 de ancho; 41 (elegido), 19 (hover), 6.5 (no leído) | igual |
| Insignia de menciones | círculo ≈17, aro 3 del color del carril | igual |
| Lista de canales | x 86.5–380 (293.5) + divisor | igual (`sidebar::WIDTH` = 294.5) |
| Cabeceras (servidor y canal) | alto 52 + divisor 1 px | igual |
| Nombre del servidor | 16 px en x 108; píldora de hover desde x 96, alto 31.5 | igual |
| Fila de canal | x 96–370.5, alto 38, paso 40.5, radio ≈6 | igual |
| Icono / nombre del canal | x 108 / x 139 | igual (icono 20) |
| Categoría | 14 px en x 107; centro 33 sobre la 1.ª fila, 50 desde la última | 33.5 / 50 |
| Panel de usuario | x 11.5–369, y 724.5–786 (357.5×61.5), radio ≈8 | igual |
| Avatar / nombre del panel | 32–34 en x 26 / nombre en x 68.5 | 32 en x 26 / x 68.5 |
| Micrófono / auriculares | 197.5–232 + flecha 233–250 / 258.5–310.5, alto 34 | 197–231.5 + 232.5–249.5 / 258–310.5, alto 34 |
| Engranaje | icono 20 en x 326–346 | botón 34 en x 319–353 (icono centrado en 336) |
| Chat | x 381–1074.5 (693.5) | igual |
| Icono / nombre en la cabecera | 18 px en x 403 / 16 px en x 435 | igual |
| Iconos de la cabecera | ≈20, paso ≈45, «Miembros» en 1051–1069.5 | botones de 32, paso 45, «Miembros» centrado en 1060 |
| Buscador | x 1093.5–1352 × y 44.5–77 (258.5×32.5) | igual |
| Compositor | x 392.5–1062.5 (670), márgenes 11.5 | márgenes 11.5 → x 392.5, ancho 670 |
| Barra del chat | pulgar 9 en x 1061.5–1070.5, sin carril | pulgar 9 centrado en 1066, sin carril |
| Lista de miembros | x 1075.5–1361 (285.5) + divisor en 1074.5 | igual (`members::WIDTH` = 286.5) |
| Encabezado de grupo | 14 px en x 1097, centro a 34.5 bajo la cabecera | igual |
| Fila de miembro | 269×43 en x 1084, radio ≈6; avatar 34 en x 1094.5; nombre en x 1142 | igual |
| Corona del dueño | 13 px tras el nombre | igual (ícono propio) |
| Barras de columnas y menús | pulgar ≈4.5 | 4.5 (el carril la muestra solo en hover) |

## Colores del tema Ónix (el predeterminado)

| Token de `Look` | Captura | Ónix |
|---|---|---|
| `rail`, `sidebar`, `canvas` | `#000000` | `#000000` |
| `raised` (botones del carril) | `#121213` | `#121213` |
| `menu` | `#121214` | `#121214` |
| `hover` | `#121213` en filas, `#222225` en menús | blanco al 7 %: `#121212` sobre negro, `#232325` sobre menú |
| `selected` | `#242426` | `#242426` |
| `border` (divisores) | `#1e1e20` | `#1e1e20` |
| `text` | `#dcdcdf` | `#dcdcdf` |
| `muted` | `#96979e` | `#96979e` |
| `dim` (leído, categorías, iconos de cabecera) | `#7a7b83` | `#7a7b83` |
| `faint` (horas) | `#73747d` | `#73747d` |
| `danger_fill` (insignias) | `#d22d39` | `#d22d39` |
| `danger` (texto: menús, errores; silenciado) | `#eb5f5e` | `#eb5f5e` (≈6:1 sobre negro y menú; `#d22d39` no llega a 4.5:1) |
| Pulgar del chat | `#595a63` | `muted` al 59 % ≈ `#58595d` |
| `accent` | `#5865f2` | ámbar propio `#e0a43c` |
| `card` (tarjetas dentro de un mensaje) | `#0a0a0c` con borde gris al 20 % (`--embed-background`, `--border-subtle` del CSS público) | `#0a0a0c` con borde `border` `#1e1e20` |

**Tarjetas dentro de un mensaje (2026-10-19).** Invitaciones, embeds y
vistas previas de enlaces, adjuntos, encuestas, mensajes de voz,
contenedores de bots, bloques de código y citas, reacciones y la subida en
curso se pintan con `card` y un borde de 1 px en `border`. Antes usaban
`raised`, que en Ónix es igual a la fila bajo el puntero (`#121213` contra
`#121212`) y en Oscuro y Claro casi igual: la tarjeta desaparecía con el
puntero encima. Como el cliente oficial, en Ónix la tarjeta queda entre el
negro y la fila bajo el puntero y la separa el borde. En Oscuro es
`#12151c` (debajo del chat y distinto de las columnas) y en Claro `#e5e7ed` (debajo de la fila bajo el
puntero, porque el blanco casi no se distingue del chat). Un test de
`look.rs` exige en los tres temas ΔL\* ≥ 2,5 entre la tarjeta y el chat y
entre la tarjeta y la fila bajo el puntero, y ΔL\* ≥ 4 entre el borde y los
tres. Un spoiler revelado pasó de `raised` a `selected`, el relleno del
código en línea. Antes y después en Ónix, con el puntero sobre las
invitaciones simuladas:
[antes](img/tarjetas-con-contraste-antes-2026-10-19.png),
[después](img/tarjetas-con-contraste-2026-10-19.png).

Claro y Oscuro siguen disponibles en Ajustes → Apariencia.

## Diferencias deliberadas

- **Inicio del carril a 5 px del borde superior.** En Discord el botón de
  inicio queda pegado a su propia barra de título; RUNE no tiene una franja
  aparte, así que lo centra en la franja de 52 px de las cabeceras para que
  no toque el borde de la ventana. El resto del carril baja 5 px con él.

- **Barra de título en la cabecera** (2026-10-19, rama `barra-e-icono`,
  como Surco): sin franja propia encima, como la de Discord. Cuando RUNE
  dibuja su marco, `WindowControls` de limn (minimizar, maximizar o
  restaurar, cerrar; 46 px de ancho cada uno, rojo de Windows al cerrar)
  va al final de la franja superior de cada vista, con 52 px de alto: la
  cabecera de la conversación o del escenario de voz, que guardan sus 138 px
  para no tapar nada, y arriba a la derecha en Ajustes, en la pantalla de
  inicio de sesión y en la de carga. Arrastrar la franja libre de la
  cabecera mueve la ventana, doble clic la maximiza o la restaura, y un
  borde de 5 px del color del carril la cambia de tamaño cuando no está
  maximizada. Pantalla completa no los muestra. Automática: en Windows y en
  X11; Wayland deja la del sistema. Ajustes › Sistema › Ventana › Barra de
  título fuerza la de RUNE o la del sistema.
  [Normal](img/barra-de-titulo-2026-10-19.png) (cerrar bajo el puntero),
  [maximizada](img/barra-de-titulo-maximizada-2026-10-19.png),
  [el ajuste](img/barra-de-titulo-ajustes-2026-10-19.png).
  Bandeja de entrada y ayuda no existen en RUNE.
- **Acento propio**: ámbar en lugar del azul de Discord (servidor elegido,
  hover del carril). La corona del dueño también va en el acento.
- **Sin funciones que RUNE no tiene**: no hay filas «Eventos» ni «Mejoras
  del servidor» (la lista empieza bajo la cabecera) ni botón «Descubrir».
- **Proveedor a la vista**: al pie de la lista de canales, una línea dice qué
  proveedor está activo («Simulación local · sin conexión real»).
- **Buscador**: busca en los mensajes cargados de la conversación (Ctrl+F).
  El selector rápido de conversaciones se abre con Ctrl+K. En ventanas que
  no tienen lugar para la lista de miembros pasa a ser un icono.
- **Lista de miembros**: se oculta por debajo de 1147.5 px de ancho
  (carril + lista + miembros + 480 de conversación mínima). El umbral de
  Discord no se midió.
- **Panel de voz**: va en su propio recuadro sobre el panel de usuario; en
  la captura es el mismo recuadro que crece hacia arriba (sección voz).

## Pendiente en limn

- Peso seminegrita: resuelto el 2026-10-18 (paquete 10): limn trae
  `FontFamily::SansSemibold`, con Ubuntu Sans SemiBold porque Ubuntu no
  tiene ese peso. RUNE la usa sólo donde la REA la pide: el título de los
  tooltips del carril y el nombre de la tarjeta de actividad (14 px
  seminegrita). Lo demás sigue en medium.
- Tooltips del carril (resuelto el 2026-10-18, ramas
  `limn-tipografia-y-opciones` y `limn-avisos-y-compositor`): abren al
  instante, con el título a 16 px en seminegrita sobre `#0a0a0c` en los
  aspectos oscuros y la flecha de siempre (`Tooltip::delay_ms`,
  `title_font_size`, `title_font_family`, `background`, `arrow`). Los demás
  tooltips siguen con los 350 ms y los 12 px de limn.
- Barras de desplazamiento que se ocultan: la del chat aparece al
  desplazar, al usar la barra y, desde el 2026-10-18, con sólo poner el
  puntero sobre el chat (`ScrollbarVisibility::AutoHideUnlessHovered`); se
  queda 600 ms después y se desvanece en 200.
