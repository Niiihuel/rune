# Instrucciones para agentes

## Qué es esto

RUNE (*Rust UI, No Electron*): cliente de chat de escritorio nativo en Rust,
con la interfaz hecha en [limn](https://github.com/Niiihuel/limn). Leer
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) antes de tocar código.

## Reglas que no se negocian

- El proyecto se llama **RUNE**.
- Toda la interfaz principal es limn. Nada de Electron, egui como framework
  de componentes, iced, Slint ni Tauri. Solo `apps/desktop` habla con
  eframe/egui, y solo para la ventana. Excepción autorizada explícitamente
  por el dueño el 2026-10-14: una WebView temporal y aislada para el login
  del proveedor no oficial; nunca para conversaciones o la interfaz principal.
  El 2026-10-16 el dueño reactivó la aceptación nativa de invitaciones y
  autorizó otra WebView temporal y aislada solo para el hCaptcha que Discord
  pida al aceptar una invitación: la resuelve la persona en una página
  propia de RUNE, nunca se automatiza ni se reenvía a servicios externos, y
  su respuesta permite un único reintento de esa misma aceptación.
  El 2026-10-17 extendió esa misma ventana, con las mismas condiciones, a
  cualquier escritura del proveedor no oficial en que Discord pida hCaptcha.
- **Si limn no tiene algo, se implementa en limn y se abre un PR** a
  `Niiihuel/limn` (rama propia, tests, docs, changelog, galería), en un
  worktree; no se parchea dentro de RUNE. Ver
  [docs/LIMN_INTEGRATION.md](docs/LIMN_INTEGRATION.md).
- Nunca pedir ni recibir contraseñas o credenciales en el chat del agente,
  logs, capturas o archivos de configuración. El dueño autorizó el
  2026-10-14 un proveedor experimental no oficial con login temporal propio,
  separado de OAuth2. La credencial de esa sesión queda dentro de RUNE; no
  se importa de otros navegadores o clientes. El dueño autorizó el
  2026-10-16 guardarla en el llavero del sistema operativo (Secret Service
  en Linux) para restaurar la sesión al arrancar; nunca en SQLite, archivos,
  logs ni configuración, y se borra del llavero al cerrar sesión.
  El 2026-10-18 autorizó la caché local cifrada por cuenta también para las
  cuentas reales (mensajes, borradores, instantánea, marcas de lectura): una
  clave por cuenta en el llavero del sistema, nunca en disco; sin llavero,
  sólo memoria y nunca texto plano; al cerrar sesión u olvidar la cuenta se
  borran sus datos y su clave. La credencial de la sesión sigue fuera de
  SQLite. Ver [docs/SECURITY.md](docs/SECURITY.md#datos-locales).
  El 2026-10-16 autorizó además las rutas no documentadas que Serein usa
  (perfil completo, lista de miembros por suscripción) y recordar varias
  cuentas como Serein.
  El 2026-10-16 autorizó también usar REA sobre el cliente oficial de
  Discord (paquete instalado y bundles JavaScript públicos) para conocer
  rutas, mensajes de Gateway, comportamiento e interfaz, y usar lo
  encontrado en el proveedor no oficial. Se reimplementa; no se copia
  código ni recursos. Excepción autorizada explícitamente por el dueño el
  2026-10-21: RUNE puede copiar y empaquetar como sus sonidos
  predeterminados los efectos de sonido públicos de Discord (mute, desmute,
  ensordecimiento, llamada, mensaje, transmisión y similares), incluidos sus
  archivos de audio. Se obtienen sin autenticación del bundle público que
  sirve Discord; esta excepción no abarca código, otros recursos, perfiles,
  cachés, credenciales ni datos de sesión, ni contenidos de servidores o
  cuentas ajenas.
  El 2026-10-17 fijó el objetivo de paridad con Serein: todo lo que haga
  Serein lo hace RUNE, reimplementado y con mejor interfaz e integraciones.
  Eso incluye usar los componentes de bots (botones, menús, modales) y los
  comandos de aplicación, y el socket local `discord-ipc` y la detección de
  juegos, estos dos apagados por defecto, cada uno con su interruptor.
  Lo que falta está en [tasks/todo.md](tasks/todo.md#paridad-con-serein).
  El 2026-10-19 el dueño quitó la prohibición de presentar el
  proveedor no oficial como el cliente web de Discord en un navegador, como
  hace Serein: user agent, `X-Super-Properties`, huella y propiedades del
  Identify, una sola identidad coherente en REST, Gateway y la WebView
  donde la persona resuelve el hCaptcha. Reimplementado, sin copiar.
  Sin evasión de CAPTCHA/MFA (los resuelve la persona, nunca se automatizan
  ni se reenvían), lectura de sesiones o datos locales del cliente oficial
  ni acceso a cuentas ajenas. Ver [docs/SECURITY.md](docs/SECURITY.md).
- RUNE no es un administrador de bots. La arquitectura se mantiene
  independiente del proveedor: nada por encima de `rune-provider` conoce un
  servicio concreto. Distinguir proveedor oficial, simulado y no oficial;
  autorizar el experimento personal no implica aprobación de Discord.
- No afirmar que algo funciona sin haberlo probado; lo simulado se etiqueta
  como simulado en la interfaz.
- No copiar código de `references/serein` (referencia de solo lectura, fuera
  del control de versiones).

## Comandos

```sh
nix-shell            # o: nix develop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p rune-desktop                                  # la ventana
cargo run -p rune-desktop -- --screenshot shot.png --channel general
cargo run -p rune-desktop -- --screenshot shot.png --data --latency 8000  # arranque desde caché
cargo run --release -p rune-desktop -- --bench             # medición de scroll
cargo run -p rune-media --example devices -- --meter       # audio local
```

`cargo fmt --all` es seguro: limn es una dependencia git, no de ruta.

## Convenciones

- Capas solo hacia abajo: `rune-ui` no conoce `rune-core` ni proveedores;
  pide cosas con `Intent`.
- La UI es controlada: el estado vive en `Store` (dominio) y `Ui`
  (interfaz); los componentes de `rune-components` son funciones puras de
  props a `Element`, probadas sin ventana.
- IDs de controles derivados de ids del dominio (`channel:{id}`,
  `msg:{id}:reply`, `timeline:{canal}`, `composer:{canal}`).
- Textos de interfaz en español y en inglés (decisión del 2026-10-17);
  comentarios y docs de código en inglés, como limn. Un texto nuevo se
  escribe en español donde se usa y pasa por `rune_i18n`: `t("Guardar")`,
  `tr!("{n} mensajes", n)` si lleva valores (con nombre, nunca `{}`),
  `es("…")` en una tabla `const` o si una capa de abajo lo pasa hacia
  arriba (y `t(texto)` donde se muestra). Su inglés va en
  `crates/rune-i18n/src/en/<zona>.rs` con el español idéntico como clave;
  `cargo test -p rune-i18n` falla si falta uno de los dos. El contenido
  (mensajes, nombres) no se traduce. Ver
  [ARCHITECTURE.md](docs/ARCHITECTURE.md#idiomas).
- Una dependencia nueva lleva un comentario en su `Cargo.toml` que diga por qué.
- Nada bloqueante en el hilo de UI: red y SQLite viven en `rune-core`.
- Un campo nuevo de un tipo que se guarda en SQLite lleva `#[serde(default)]`
  (las filas viejas tienen que seguir leyéndose); un cambio de tablas sube
  `SCHEMA` en `rune-storage` con su migración y su test.
- Lo que un proveedor puede hacer se declara en `Capabilities`, y la UI
  pregunta a `Store::abilities`; no decide permisos por su cuenta.
