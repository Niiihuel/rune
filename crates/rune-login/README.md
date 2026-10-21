# rune-login

Ventana temporal de acceso y de hCaptcha del proveedor no oficial.
`HostedLogin` implementa `SessionLogin`: inicia el mismo ejecutable con
`--session-login-helper` (o `--session-captcha-helper`,
`--session-engine-helper`) antes de eframe, y ese proceso auxiliar tiene su
propio bucle WebKitGTK (Linux, `webview.rs`) o WebView2 (Windows,
`windows.rs`); lo común está en `surface.rs`. Sin la feature `webview` no
hace nada. Flujo y reglas en [docs/DISCORD.md](../../docs/DISCORD.md) y
[docs/SECURITY.md](../../docs/SECURITY.md).

- Sesión web efímera (en Windows, perfil InPrivate en una carpeta temporal
  que se borra); nunca perfiles, cookies ni credenciales de otros
  programas. Sin ventanas nuevas, descargas, permisos ni herramientas de
  desarrollo.
- El puente sólo observa la cabecera `Authorization` de las solicitudes a
  la API del documento principal propio; la parte nativa valida origen,
  generación, plazo, capacidad aleatoria y tamaño. El resultado vuelve una
  vez por una tubería anónima privada (en Linux, sólo FIFO).
- La ventana de hCaptcha carga la página propia
  `https://captcha.rune.invalid/` (`captcha.html`, `captcha.js`) con CSP
  limitada a hCaptcha; recibe `sitekey`, `rqdata` y si es invisible, nunca
  la sesión. Cerrar (código 2), vencer (3) o no cargar hCaptcha (10) no
  devuelven nada; sin WebView2 Runtime los auxiliares terminan con 11.
- WebView2 crea el controlador con el HWND oculto y conserva
  `IsVisible = false`: RUNE fija `SetIsVisible(true)` para login y
  verificación, o la página queda en blanco.
- En Linux, WebKitGTK necesita los plugins de GStreamer base, good y bad
  (`GST_PLUGIN_SYSTEM_PATH_1_0`, que pone `nix-shell --arg withLogin true`);
  sin `autoaudiosink` el proceso web se cae.

Pruebas, sin credenciales ni red:

```sh
node crates/rune-login/tests/handoff.cjs
node crates/rune-login/tests/captcha.cjs
nix-shell --arg withLogin true --run 'cargo test -p rune-login --features webview -- --include-ignored --test-threads=1'
```

En Windows: `cargo test -p rune-login --features webview --locked -- windows::tests::native_ --ignored --test-threads=1`.
