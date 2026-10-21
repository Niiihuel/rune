# RUNE sobre limn

RUNE construye toda su interfaz principal con [limn](https://github.com/Niiihuel/limn):
el modelo declarativo (la ventana es una función del estado, reconstruida
cada cuadro), el `UiTree` que guarda foco, hover, scroll y texto por ID, y el
backend `limn-egui` (egui + wgpu) que pinta y traduce la entrada. egui no se
usa como framework de componentes: sólo `apps/desktop` toca eframe/egui, y
únicamente para abrir la ventana y capturar pantalla. Todos los atajos de la
ventana (los de `rune_ui::keybinds`, los botones laterales del mouse, el
grabador de teclas y Pulsar para hablar) llegan como eventos de limn.

Las excepciones autorizadas por el dueño son WebViews temporales y aisladas
para el login del proveedor no oficial y para que la persona resuelva un
hCaptcha. Viven en un proceso auxiliar opcional; no renderizan
conversaciones ni sustituyen componentes de limn
([DISCORD.md](DISCORD.md), [SECURITY.md](SECURITY.md)).

## Si limn no tiene algo

Regla del proyecto: **lo que falta se implementa en limn y se abre un PR** a
`Niiihuel/limn`; no se parchea dentro de RUNE.

1. Worktree propio de limn desde la cabeza de la rama de integración, en
   una rama `rune/<tema>` (y la rama `feat/<tema>` del PR, desde `main`, si
   las dependencias lo permiten; si no, el PR se apila sobre las que usa).
2. Seguir `docs/contributing.md` de limn: tests, docs, changelog y galería
   (regenerar `site/src/data/pages.json` con
   `cargo run -p limn-gallery -- --pages` si se toca la galería).
3. Unir la rama a la de integración, subirla a GitHub y apuntar RUNE a ella;
   después `cargo update -p limn -p limn-egui`.
4. Mientras tanto, si RUNE compone algo a mano, se marca en el código con
   `// limn-gap:` y el motivo.

Varios agregados chicos que tocan los mismos archivos de limn (`tree.rs`,
`element.rs`, el changelog, la galería) van en un solo PR: separados desde
`main` chocarían entre sí y habría que apilarlos igual.

## Cómo depende RUNE de limn

`Cargo.toml` del workspace apunta a la rama `rune/integration-2026-10-19` de
[Niiihuel/limn](https://github.com/Niiihuel/limn), que reúne las extensiones
que RUNE necesitó y todavía no están todas en `main`. El commit consumido lo
fija `Cargo.lock`, no la cabeza de la rama. Es una dependencia **git**, no de
ruta: así `cargo fmt --all` no reformatea las fuentes de limn. Un trabajador
puede apuntar temporalmente a su clon local (`file:///…/lienzo`); antes de
mezclar en RUNE, la rama se sube a GitHub y `Cargo.toml` vuelve a GitHub.
Cuando una versión publicada reúna los cambios, se podrá pasar a crates.io.

Lo que todavía falta en limn está en
[tasks/todo.md](../tasks/todo.md#limn).
