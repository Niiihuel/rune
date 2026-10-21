# Patrones de Discord que RUNE adopta

RUNE toma de Discord **patrones de interacción**, no su apariencia. Esta
tabla junta los valores y comportamientos que el código sigue, con su
fuente pública y la confianza que merece como afirmación sobre Discord.
Las rutas y mensajes del proveedor no oficial salen de la REA del cliente
y de Serein ([REA.md](DISCORD.md), [DISCORD_SESSION.md](DISCORD.md)),
no de este documento.

Confianza: **alta** = lo dice una fuente oficial (Help Center, docs de la
API, blog de ingeniería); **media** = fuente oficial indirecta o varias de
terceros que coinciden; **baja** = un reporte aislado o una observación
común sin documentar. Donde Discord no publica un valor, el de RUNE es una
decisión de diseño propia y así se marca.

## Valores adoptados

| Patrón | En RUNE | Confianza | Fuente |
|---|---|---|---|
| Agrupación de mensajes por autor | Mismo autor, sin cambio de día, sin respuesta ni divisor en medio y a menos de **7 min del primer mensaje del grupo** (`GROUP_MINUTES` en `rune-components::message`). Discord no publica el umbral; un reporte de 2023 dice 8 min desde el primero y corte a medianoche. Los 7 min son valor de diseño RUNE. Respuestas y mensajes de sistema siempre abren grupo | baja | [post comunitario](https://support.discord.com/hc/en-us/community/posts/12799818806551-Undo-the-recent-change-to-message-grouping) |
| Divisor de fecha | Un cambio de día local corta el grupo e inserta un divisor | baja | mismo post |
| Página de historial | 50 mensajes; anclas `before` / `after` / `around` excluyentes; respuesta de más nuevo a más viejo. Una página corta puede ser un límite del proveedor y no el comienzo real: no se borra lo que queda fuera del rango cubierto | alta | [Get Channel Messages](https://docs.discord.com/developers/resources/message) |
| Snowflakes | Ordenan por tiempo (`(id >> 22) + 1420070400000` ms) y sirven de cursor de paginación y de rango por fecha | alta | [API reference](https://docs.discord.com/developers/reference) |
| Anclaje del scroll | Discord no lo documenta. RUNE: seguir el final sólo si la persona ya estaba abajo; anteponer historial conserva la primera fila visible; los cambios de alto por encima del ancla se compensan; los medios reservan su alto con `width`/`height` | diseño RUNE | — |
| Barra de nuevos mensajes y volver al presente | Divisor de no leídos, barra «N mensajes nuevos» y salto al presente; Esc marca el canal leído | alta / media (Esc salta al presente) | [Keyboard Navigation FAQ](https://support.discord.com/hc/en-us/articles/1500000056121), [Windows Hotkeys](https://support.discord.com/hc/en-us/articles/225977308) |
| Enviar / nueva línea | Enter envía; Shift+Enter inserta un salto | alta | [Text Channels](https://support.discord.com/hc/en-us/articles/4412085582359) |
| Editar el último mensaje | Flecha arriba con el composer vacío (terceros) y Shift+↑ (guía oficial actual): RUNE acepta ambos | media | [Guía de atajos](https://support.discord.com/hc/en-us/articles/31232432266647) |
| Escape | Cancela edición o respuesta; si no hay nada pendiente, marca el canal leído; en la navegación de mensajes devuelve el foco al composer | alta | [Guía de atajos](https://support.discord.com/hc/en-us/articles/31232432266647), [Keyboard Navigation FAQ](https://support.discord.com/hc/en-us/articles/1500000056121) |
| Selector rápido | Ctrl+K; prefijos `*` servidores, `@` directos, `#` texto, `!` voz; Enter con la búsqueda vacía vuelve al canal anterior | alta | [Quick Switcher](https://support.discord.com/hc/en-us/articles/115000070311) |
| Navegación por teclado | Alt+↑/↓ canales, Alt+Shift+↑/↓ no leídos, Ctrl+Alt+↑/↓ servidores, F6 / Shift+F6 entre secciones. El carril es una lista que se recorre con flechas, fuera del orden de Tab | alta | [Windows Hotkeys](https://support.discord.com/hc/en-us/articles/225977308), [Keyboard Navigation FAQ](https://support.discord.com/hc/en-us/articles/1500000056121) |
| Movimiento reducido | Por defecto sigue al sistema; los saltos a mensajes son instantáneos | alta | [Reduced Motion](https://support.discord.com/hc/en-us/articles/360040613412) |
| Estados de lectura | Uno por canal y cuenta, con contador de menciones que vuelve a 0 al leer | alta | [blog de ingeniería](https://discord.com/blog/why-discord-is-switching-from-go-to-rust) |
| Separar red y render | El hilo de UI no espera red ni disco; los eventos llegan ya traducidos al dominio y la lista se virtualiza | alta (principio) | [blog de ingeniería](https://discord.com/blog/supercharging-discord-mobile-our-journey-to-a-faster-app) |

Las medidas y colores de la interfaz están en [DISEÑO.md](DISEÑO.md).

## Lo que no se usa

Logo, nombre y mascotas de Discord, el color Blurple como acento, las
tipografías Whitney y gg sans, sus iconos y los nombres de producto
(«Nitro», «Go Live»…): RUNE usa recursos propios o con licencia abierta y
terminología genérica. Ver [ARTWORK.md](ARTWORK.md) y [FONTS.md](FONTS.md).
La excepción son los sonidos predeterminados, efectos públicos de Discord
que el dueño autorizó el 2026-10-21 ([MEDIA.md](MEDIA.md#sonidos)).
