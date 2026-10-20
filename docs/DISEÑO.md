# Diseño general: una interfaz compacta con limn

RUNE conserva su lenguaje visual compacto. Discord y Serein sirven como
referencias de comportamiento —jerarquía, foco, hover y separación de
zonas—, no como un diseño que haya que reproducir píxel a píxel
(preferencia explícita del dueño, 2026-10-14). Los textos, iconos, colores y
recursos son de RUNE. No se agrega un control sólo porque aparece en la
referencia: su acción tiene que existir y respetar las capacidades del
proveedor.

Las medidas de abajo salen de capturas de Discord (tema «Ónix», zoom 67 %,
contenido de 1361×797 px lógicos) y las pruebas de columnas de
`crates/rune-ui/src/tests/layout.rs` las conservan. No son evidencia de
dimensiones internas de Discord. Los patrones de interacción adoptados,
con su fuente, están en [DISCORD_RESEARCH.md](DISCORD_RESEARCH.md).

## Reglas de interacción

- **Composer**: una línea de 48 px como mínimo, con texto, placeholder e
  iconos centrados en vertical por un único cálculo de primera línea. Crece
  con el texto y los controles se quedan junto a la primera línea. Un
  placeholder largo se trunca en una línea y no agranda la tarjeta.
- **Timeline y composer**: 8 px estables entre los dos. Esa franja no activa
  mensajes ni muestra sus acciones, tampoco cuando crece el editor. Mostrar
  u ocultar el indicador de escritura no mueve el último mensaje, y crecer
  el composer mantiene el final visible sólo si la persona ya lo seguía; al
  leer historia se conserva el ancla.
- **Acciones al pasar sobre un mensaje**: van superpuestas, no cambian la
  altura ni el ajuste de líneas de la fila ni la posición del scroll. Pasar
  del mensaje a la barra no la hace parpadear; el foco de teclado o un menú
  abierto la retienen; se quedan dentro del timeline en la primera y la
  última fila y en ventanas estrechas (lo que no entra va al menú de
  desborde). El teclado llega a todas las acciones.
- **Contadores del carril y la lista de canales**: menciones en canales de
  servidor y no leídos en directos; un canal sin acceso no cuenta. El
  número queda centrado en la cápsula, con lugar para `99+` y sin padding
  vertical que le quite alto al texto; la insignia se ancla a la caja del
  icono y su anillo entra entero. El nombre del canal se trunca antes de la
  insignia y la fila no cambia de alto.
- **Bienvenida y acceso**: marca centrada, título de 24 pt, tarjeta de hasta
  480 y acción de 44, con los tokens del tema; sin campos de credenciales
  ni QR sin flujo detrás. Las esperas conservan Cancelar, los errores
  largos se desplazan y el contenido se adapta a ventanas chicas.

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

**Tarjetas dentro de un mensaje.** Invitaciones, embeds y vistas previas
de enlaces, adjuntos, encuestas, mensajes de voz, contenedores de bots,
bloques de código y citas, reacciones y la subida en curso se pintan con
`card` y un borde de 1 px en `border`. Como en el cliente oficial, en Ónix
la tarjeta queda entre el negro y la fila bajo el puntero y la separa el
borde; con `raised` desaparecía con el puntero encima. En Oscuro es
`#12151c` y en Claro `#e5e7ed`. Un test de `look.rs` exige en los tres temas
ΔL\* ≥ 2,5 entre la tarjeta y el chat y entre la tarjeta y la fila bajo el
puntero, y ΔL\* ≥ 4 entre el borde y los tres. Un spoiler revelado usa
`selected`, el relleno del código en línea.
[Captura](img/tarjetas-con-contraste-2026-10-19.png) con el puntero sobre
las invitaciones simuladas.

Claro y Oscuro siguen disponibles en Ajustes → Apariencia.

## Diferencias deliberadas

- **Inicio del carril a 5 px del borde superior.** En Discord el botón de
  inicio queda pegado a su propia barra de título; RUNE no tiene una franja
  aparte, así que lo centra en la franja de 52 px de las cabeceras para que
  no toque el borde de la ventana. El resto del carril baja 5 px con él.

- **Barra de título en la cabecera**: sin franja propia encima. Cuando RUNE
  dibuja su marco, `WindowControls` de limn (minimizar, maximizar o
  restaurar, cerrar; 46 px de ancho cada uno, rojo de Windows al cerrar)
  va al final de la franja superior de cada vista, con 52 px de alto: la
  cabecera de la conversación o del escenario de voz, que guardan sus
  138 px para no tapar nada, y arriba a la derecha en Ajustes, en el inicio
  de sesión y en la pantalla de carga. Arrastrar la franja libre mueve la
  ventana, doble clic la maximiza o la restaura, y un borde de 5 px del
  color del carril la cambia de tamaño cuando no está maximizada. Pantalla
  completa no los muestra. Automática: la de RUNE en Windows y X11, la del
  sistema en Wayland; Ajustes › Sistema › Ventana › Barra de título la
  fuerza. [Normal](img/barra-de-titulo-2026-10-19.png),
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

## Tipografía, tooltips y barras

- **Seminegrita**: `FontFamily::SansSemibold` de limn (Ubuntu Sans
  SemiBold, porque Ubuntu no tiene ese peso), sólo donde la REA la pide: el
  título de los tooltips del carril y el nombre de la tarjeta de actividad
  (14 px). Lo demás va en medium.
- **Tooltips del carril**: abren al instante, con el título a 16 px en
  seminegrita sobre `#0a0a0c` en los aspectos oscuros (`Tooltip::delay_ms`,
  `title_font_size`, `title_font_family`, `background`, `arrow`). Los demás
  tooltips usan los 350 ms y los 12 px de limn.
- **Barra del chat**: aparece al desplazar, al usarla y al poner el puntero
  sobre el chat (`ScrollbarVisibility::AutoHideUnlessHovered`); se queda
  600 ms y se desvanece en 200.
