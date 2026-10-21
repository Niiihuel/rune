# Ventana temporal de acceso

`HostedLogin` implementa el contrato privado `SessionLogin`. Inicia el mismo
ejecutable con `--session-login-helper`, antes de inicializar eframe. El
proceso auxiliar posee su propio bucle GTK/WebKit (Linux) o Win32/WebView2
(Windows); la aplicación continúa usando limn. La función está desactivada
sin el feature `webview`. Lo común a los dos motores (qué carga cada
ventana, qué acepta y qué devuelve) está en `surface.rs`; WebKitGTK en
`webview.rs` y WebView2 en `windows.rs`.

## Windows (WebView2)

Mismo contrato y mismas garantías que en Linux, sobre WebView2
(`webview2-com`). Cada ventana es un proceso auxiliar con su perfil
InPrivate en `%TEMP%\RUNE-webview\<al azar>`, que se borra cuando ese
navegador termina; la que deja un auxiliar matado la borra el siguiente.
La navegación principal sólo va a la página propia (Discord en el login,
`https://captcha.rune.invalid/` servida desde memoria en la verificación);
los marcos, también a hCaptcha. Sin ventanas nuevas, descargas, permisos,
autenticación básica, herramientas de desarrollo, menús, atajos del
navegador, contraseñas guardadas ni autocompletado. La ventana lleva el
ícono de RUNE (`assets/icons/rune.ico`). El script de la página
despierta a RUNE por `chrome.webview.postMessage(true)`, sin datos; RUNE
evalúa la función de toma en el documento principal y valida origen,
generación, plazo, capacidad y tamaño, como en Linux.

`--session-engine-helper` abre WebView2 oculto y devuelve, como JSON, lo que
una página ve de su navegador (`rune_discord_session::Engine`); el
proveedor presenta ese mismo navegador. El auxiliar del login y del
hCaptcha recibe en `RUNE_USER_AGENT` el user agent presentado y sólo lo
fija si el runtime cambió. Sin WebView2 Runtime, los tres terminan con el
código 11 y RUNE da el enlace oficial de Microsoft para instalarlo.

En MSVC el cargador de WebView2 va enlazado estático; con el target GNU
(chequeo desde NixOS) `rune.exe` necesita `WebView2Loader.dll` al lado
(está en la carpeta `out/x64` de `webview2-com-sys` dentro de `target`).
El controlador se crea mientras el HWND está oculto: WebView2 conserva
`IsVisible = false` incluso después de `ShowWindow`. Por eso RUNE fija
explícitamente `SetIsVisible(true)` para login y verificación, y `false`
para la sonda del motor. Sin eso la página puede cargar y ejecutar
JavaScript, pero la ventana queda en blanco. Comprobado en Windows 11
build 26200 con WebView2 155.0.4283.62: la página pública de Discord carga
su formulario al activar el controlador. No se inició sesión en una cuenta.

El dueño completa el login en `https://discord.com/login`. Es un handoff de
sesión **no oficial**, distinto de OAuth2 y sin garantía de compatibilidad.
El user agent es el del motor (el mismo que presenta el adaptador, ver
`rune-discord-session::browser`); no se automatizan MFA ni CAPTCHA.
No se importan perfiles ni credenciales de otros programas.

En NixOS, tanto la compilación como la ejecución requieren
`nix-shell --arg withLogin true` o `nix develop .#session`. Ese entorno
expone los plugins GStreamer base, good y bad mediante
`GST_PLUGIN_SYSTEM_PATH_1_0`; el RPATH del binario no reemplaza esa
configuración. La ausencia de `autoaudiosink` provocó una caída reproducible
al inicializar audio en WebKitGTK 2.52.4. Ver
[DISCORD_SESSION.md](../../docs/DISCORD_SESSION.md#compilar-y-ejecutar).
El dueño confirmó después el login y la identidad; las pruebas descritas
aquí siguen siendo fixtures locales y no acreditan nuevas operaciones
sobre la cuenta.

## Ventana de verificación hCaptcha

Autorizada por el dueño el 2026-10-16 para invitaciones
([SECURITY.md](../../docs/SECURITY.md#verificación-de-invitaciones-autorizada-el-2026-10-16))
y el 2026-10-17 para cualquier escritura
([SECURITY.md](../../docs/SECURITY.md#ampliaciones-del-2026-10-17)).
`HostedLogin::verify` inicia el ejecutable con `--session-captcha-helper` y
le pasa por la tubería privada la capacidad, `sitekey`, `rqdata` y si el
desafío es invisible; nunca la sesión ni el `rqtoken`. La ventana carga la
página propia `https://captcha.rune.invalid/` (`captcha.html` y
`captcha.js`) con CSP limitada a hCaptcha, misma sesión efímera y mismas
restricciones que el login, sin cambiar el user agent. La persona resuelve el
desafío; la respuesta vuelve una vez como `{capacidad}:{respuesta}`. Cerrar
(código 2), vencer (3) o no poder cargar hCaptcha (10) no devuelven nada.
Ver [INVITATIONS.md](../../docs/INVITATIONS.md) y
[DISCORD_SESSION.md](../../docs/DISCORD_SESSION.md#verificación-hcaptcha-en-cualquier-escritura).

Cada intento abre una sesión WebKit efímera. El puente observa únicamente
cabeceras Authorization de solicitudes API del documento principal propio;
no lee formularios, cuerpos, almacenamiento web ni solicitudes de otros
procesos. IPC comunica una señal booleana. El proceso nativo consulta el
slot del documento principal y valida origen, generación, plazo, capacidad
aleatoria y tamaño. No existe un fallback que inspeccione cualquier recurso.

El resultado se entrega una sola vez por una tubería anónima privada; no
aparece en argumentos, variables de entorno, logs o eventos de UI. El
auxiliar rechaza stdin/stdout conectados a archivos o terminales antes de
iniciar WebKit; en Linux exige descriptores FIFO mediante `/proc/self/fd`.
El proveedor verifica posteriormente la identidad. Cancelar o agotar el plazo
descarta el resultado y termina el proceso auxiliar. Al finalizar normalmente
se cancelan evaluaciones, retiran scripts, termina el proceso web y destruye
la ventana. Las copias propias de Rust se borran al descartarse; no se promete
borrado forense de memoria del motor, swap o volcados del sistema.

Pruebas sin credenciales reales:

```sh
node crates/rune-login/tests/handoff.cjs
node crates/rune-login/tests/captcha.cjs
nix-shell --run 'cargo test -p rune-login'
nix-shell --arg withLogin true --run 'cargo test -p rune-login --features webview'
nix-shell --arg withLogin true --run 'cargo test -p rune-login --features webview -- --include-ignored --test-threads=1'
```

En Windows, con WebView2 Runtime instalado:

```powershell
cargo test -p rune-login --features webview --locked
cargo test -p rune-login --features webview --locked -- windows::tests::native_ --ignored --test-threads=1
```

Las pruebas nativas de Windows sirven HTML local desde memoria: una lee la
identidad del navegador oculto y la otra exige que una página visible llegue
a un `requestAnimationFrame` con `document.visibilityState === 'visible'`.
No se conectan a Discord ni usan credenciales. La segunda detecta el fallo
que la sonda oculta por sí sola no podía ver.

El último comando de Linux incluye pruebas nativas con HTML sintético local y CSP
sin red. La primera crea un XHR y fija una cabecera de prueba, pero nunca lo
envía. Comprueba el puente nativo y el cierre. La segunda inicia un auxiliar
aislado y su WebKitWebProcess, cancela el intento y comprueba que el auxiliar
y sus descendientes observados dejan de ejecutarse. Solo inspecciona PIDs,
nombres y estado de ese árbol propio. No prueba el login real, CAPTCHA ni
Discord.

Los tests de proceso verifican la terminación del auxiliar al cancelar o
agotar el plazo. La prueba nativa pasó con WebKitGTK 2.52.4 en este entorno;
no garantiza el comportamiento de otras versiones del motor ni de procesos
que no alcanzaron a observarse. La exclusión de subframes, origen, caducidad
y entrega única también tiene fixtures JavaScript sin navegador. La carrera
entre generaciones se verifica en el estado Rust, no mediante una navegación
real de Discord.

APIs contrastadas con documentación primaria: [sesiones efímeras](https://world.pages.gitlab.gnome.org/Rust/webkit6-rs/stable/latest/docs/webkit6/struct.NetworkSession.html),
[mensajes JavaScript](https://docs.rs/webkit6/latest/webkit6/struct.UserContentManager.html)
y [evaluación y ciclo de vida del WebView](https://docs.rs/webkit6/latest/webkit6/struct.WebView.html).
