# REA en RUNE: qué se analizó y qué resultó

[REA](https://github.com/morluto/rea) estuvo disponible por MCP. Se usó solo
sobre material cuya inspección está autorizada.

Estado de herramientas al 2026-10-15: la ampliación con Ghidra y pwntools ya
está realizada y registrada en [Discord instalado](#discord-instalado-paquete-y-módulos-nativos-2026-10-15).
Los apartados de iteraciones anteriores conservan la disponibilidad y los
resultados de su fecha; sus motores ausentes no describen la configuración actual.

## Alcance de las primeras iteraciones

- **El cliente oficial de Discord** no se inspeccionó en las primeras
  iteraciones: se usaron fuentes públicas y capturas aportadas por el dueño.
  La revisión posterior del paquete instalado, solicitada explícitamente,
  se registra abajo con su cobertura parcial. No incluyó perfiles de
  navegador, credenciales ni tráfico autenticado.
- **Serein** es código abierto: se estudió leyendo sus fuentes
  ([SEREIN_ANALYSIS.md](SEREIN_ANALYSIS.md)), que es lo que la propia guía de
  REA recomienda para repositorios de código.

## Qué se analizó: el binario de RUNE

Objetivo: verificar la ausencia de dependencias inesperadas (motor web,
toolkits) en lo que se entrega.

| Paso | Herramienta | Resultado | Confianza |
| --- | --- | --- | --- |
| Abrir `target/release/rune` | `rea open_binary` | ELF x86_64 ejecutable, sha256 `f06f70ba…46acca` | Observado |
| Inventario del artefacto | `rea inspect_artifact` | Un único nodo ELF de 38 597 504 bytes, hash verificado, **sin contenedores hijos** (ningún paquete de recursos web embebido) | Observado |
| Análisis de funciones y cadenas | `rea binary_overview` | **No disponible**: REA no tiene proveedor profundo configurado (Ghidra sin `GHIDRA_INSTALL_DIR` ni Java, Hopper e IDA ausentes) | — |

Complemento con herramientas estándar sobre el mismo binario:

| Comprobación | Resultado |
| --- | --- |
| `readelf -d` (bibliotecas requeridas) | `libpipewire-0.3`, `libasound`, `libgcc_s`, `libm`, `libc`, `ld-linux` |
| Cadenas `webkit/chromium/electron/libcef/webview` | 0 coincidencias |
| Bibliotecas cargadas en ejecución (`/proc/<pid>/maps`, 72) | Wayland, xkbcommon, EGL/GL dispatch, cargador Vulkan y drivers (NVIDIA, radeon, nouveau, lavapipe, dzn), PipeWire, ALSA. **Ninguna** de WebKit, Chromium, Electron, CEF, GTK ni Qt |

Conclusión verificada: RUNE es un binario nativo que pinta con Vulkan/EGL a
través de wgpu, sin motor web ni toolkit gráfico externo.

## Segunda iteración: arranque del binario

Objetivo: medir el arranque del binario de release tal como lo ejecutaría
una persona, fuera del entorno de desarrollo.

| Paso | Herramienta | Resultado | Confianza |
| --- | --- | --- | --- |
| Ejecutar `target/release/rune` desde un shell normal | `rea capture_process_scenario` | **No arrancaba**: `libpipewire-0.3.so.0: cannot open shared object file`. Dentro de `nix-shell` sí, por `LD_LIBRARY_PATH` | Observado |
| Causa | `readelf -d` | El binario no tenía `RUNPATH`: `rust-lld`, el enlazador por defecto, no graba las rutas de Nix | Observado |
| Arreglo | `shell.nix` | `RUSTFLAGS = "-C link-arg=-Wl,-rpath,${runtime}"` con wayland, libxkbcommon, vulkan-loader, libGL, alsa-lib y pipewire. Ahora `RUNPATH` está y el binario arranca desde cualquier shell | Verificado |
| Arranque con cada preferencia de GPU | `rea capture_process_scenario` + `/sys/bus/pci/devices/…/power/runtime_status` | 0,94–1,1 s al primer cuadro y la GPU NVIDIA se despierta, con cualquier `WGPU_POWER_PREF` | Observado |
| Arranque limitando el cargador Vulkan a la integrada | ídem, con `VK_LOADER_DRIVERS_SELECT='*radeon*'` | 80 ms y la NVIDIA suspendida todo el tiempo. Es lo que RUNE hace ahora por defecto (`apps/desktop/src/gpu.rs`) | Observado |

Bibliotecas cargadas hoy en ejecución por defecto (`/proc/<pid>/maps`, 59):
el cargador Vulkan con **solo** `libvulkan_radeon`; EGL (glvnd) todavía
mapea las bibliotecas EGL de NVIDIA al iniciar el backend GL de wgpu, sin
despertar la GPU (`runtime_status` sigue en `suspended`). Limitar wgpu a
Vulkan las evitaría, pero quitaría la alternativa GL en equipos sin Vulkan;
no se hizo.

Detalle de los números en [PERFORMANCE.md](PERFORMANCE.md).

Para un análisis de funciones con REA en el futuro: instalar Ghidra 12.1.x y
Java, exportar `GHIDRA_INSTALL_DIR` y repetir `binary_overview`.

## Revisión de continuidad (2026-10-14): herramientas locales

REA no estaba entre las herramientas conectadas de esta sesión. Los pasos
siguientes se hicieron con Cargo, `sha256sum`, `stat`, `readelf` y el
benchmark incorporado de RUNE; no son una nueva ejecución de REA ni un
análisis de funciones con Ghidra.

Tras `nix-shell --run 'cargo build --release -p rune-desktop'`, el binario
`target/release/rune` midió **41 506 376 bytes**, con SHA-256
`3b8e36d21d7a2e4758aa71861f1199d461f40f13c82d3454089a7757d1c8f019`.
`readelf -d` mostró `RUNPATH` a las bibliotecas de Nix y estas dependencias
dinámicas directas: `libpipewire-0.3.so.0`, `libasound.so.2`,
`libgcc_s.so.1`, `libm.so.6`, `libc.so.6` y `ld-linux-x86-64.so.2`.
Esto identifica dependencias directas; no sustituye una inspección de
bibliotecas cargadas dinámicamente ni prueba por sí solo el contenido de
todo el artefacto.

Se ejecutó el binario fuera de `nix-shell` en Wayland: dos benchmarks
completaron 601 cuadros y el panel informó 60 cuadros/s (contador defectuoso,
identificado más abajo). Resultados y límites de la medición
en [PERFORMANCE.md](PERFORMANCE.md#validación-de-continuidad-correcciones-de-caché-2026-10-14).
Serein se estudió desde sus fuentes públicas; ningún binario ajeno formó
parte de esta revisión.

## Conexión y nueva captura MCP (2026-10-14, 19:03–19:04 UTC)

REA **6.4.0** quedó registrado en `~/.codex/config.toml`, mediante
`rea setup --client codex --yes`, después de revisar su plan. El registro
ejecuta `/home/nihuel/.npm-global/bin/rea mcp`; el respaldo es
`~/.codex/config.toml.rea.backup`. La comparación del TOML conservó las demás
entradas. También se instaló y leyó el workflow
`~/.agents/skills/reverse-engineer-anything/SKILL.md`.

`rea mcp doctor --json` completó un handshake MCP real: servidor `rea@6.4.0`,
protocolo `2025-11-25`, **170 herramientas y 6 prompts**, sin entradas
faltantes ni duplicadas, y una llamada satisfactoria a `binary_session {}`.
El digest del catálogo fue
`85d80276ecacf937bf3e803b7a31748ae1a8286f556fa5cf734eef768cc7d72a`.
Además se hicieron las llamadas de análisis siguientes por stdio, mediante
el SDK MCP ya incluido en el paquete. Esto verifica un servidor utilizable;
el registro persistente se carga al reiniciar o reconectar el cliente y no
inserta herramientas retroactivamente en una conversación abierta.

### Objetivo y evidencias conservadas

Se seleccionó únicamente el ejecutable propio
`/home/nihuel/projects/personal/rune/target/release/rune`. Es el **release
existente anterior a los cambios de búsqueda de esta iteración**, de
**41 506 376 bytes**, SHA-256
`3b8e36d21d7a2e4758aa71861f1199d461f40f13c82d3454089a7757d1c8f019`.
No se recompiló para esta medición. `open_binary` identificó ELF x86_64.

Los documentos Evidence completos, escenarios, schemas y respuestas MCP
quedaron en `/tmp/rune-rea-20261014-vlNyi6/`. Es una ruta local temporal, no
un artefacto versionado; puede desaparecer al limpiar `/tmp`. Los bundles
exportados por REA son `evidence-bundle.json` y
`wayland-evidence-bundle.json`. Los archivos `*.json` conservan
`structuredContent`; `*.mcp.json` conservan también el sobre MCP.

- **Inventario estático** (`inspect_artifact.json`):
  `ev_2cc696c87a2b26546cdf384dca2190720e7c887892aed745b1614c9dbcde420f`.
  Un nodo ELF, una ocurrencia, cero aristas, tamaño y hash verificados.
  Inventario interno:
  `ev_f3ad3f3acd4f56b7f1d7c771c5ef69c8e7afa9a85cba3cecb19640d8212088df`;
  manifiesto
  `agm_a642e5ee6a87046ff81051d5024c8dbfeb4269d99123dbddb7169b4594090b00`.
  La observación «sin contenedores hijos» describe ese inventario: por sí
  sola **no demuestra** ausencia de código web, bibliotecas estáticas ni
  dependencias cargadas después.
- **Primer intento de benchmark** (`bench.json`):
  `ev_1734bfef18c1c14662218ae29e550437c7e25b72c99e43ba1d4a5eee8010d92e`.
  Exit 1: faltaban las variables de pantalla en el servidor iniciado por
  `StdioClientTransport`, cuyo entorno predeterminado no las hereda.
  Se conserva este fallo; no fue un fallo de enlace ni una medición válida.
- **Benchmark Wayland** (`bench-wayland.json`):
  `ev_ccc0d0d3db09b021d83518f4d5caced4bd4b7c3f93fa88ef9b5ea25009a1a395`.
  Exit 0, proceso asentado y grupo propio verificado. El stdout de RUNE
  informó **601 cuadros y 400 mensajes**: primer cuadro **94 ms**; árbol
  UI p50/p95 **0,45/0,58 ms**; layout **2,07/3,96 ms**, máximo **9,72 ms**;
  pintado **0,13/0,21 ms**. El panel informó **60,0 cuadros/s**,
  **132,3 MB RSS** y **49,2 % CPU de proceso**.
- **Ventana Wayland** (`screenshot-wayland.json`):
  `ev_3c129413121c97a19d1fc04924065eae07cf1af5f089b52ae12772997b0361f4`.
  Exit 0 y primer cuadro **80,6 ms**. Se generó y revisó visualmente
  `run/rune-wayland.png`, **523 070 bytes**, SHA-256
  `51f36d04ab724657d22d0faabcf6a2f45fdacb4b593181facbb41a9d8bc44184`.
  Muestra el canal simulado `general`, navegación, timeline, miembros y
  composer, con la etiqueta «Simulación local · sin conexión real».

Las cifras de rendimiento son **instrumentación interna de RUNE capturada
por REA**, una sola ejecución bajo carga concurrente de desarrollo; REA
no midió tiempos GPU ni verificó independientemente los cuadros presentados
por el compositor. No son un benchmark de Discord ni una comparación entre
versiones. Se conservó stdout sin normalizar números. Las capturas no
truncaron salida ni snapshots; el muestreo de procesos no es exhaustivo.
REA registró procesos ajenos cuya propiedad no pudo verificar por `EACCES`,
los dejó sin tocar y verificó la limpieza del grupo propio. El entorno
heredado y la actividad de red permanecen sin observar.

La duración total de la captura del benchmark fue **5,50 s**, mientras
stdout informó 601 cuadros y el panel 60,0 cuadros/s. Esa diferencia motivó
una revisión de `apps/desktop/src/metrics.rs`: en el código del baseline,
`frames` retiene como máximo 120 muestras y `rate()` divide la cantidad de
muestras recientes por 2,0. Por construcción el resultado nunca supera
60, aunque se produzcan más cuadros, y subestima durante el arranque. La
conclusión demostrada es un defecto del contador, no una frecuencia real
del compositor.

El defecto se corrigió en esta iteración: `rate_at` divide los intervalos
completados por su duración observada hasta el instante de consulta. Usa
las hasta 120 muestras conservadas, incluyendo el intervalo que atraviesa
el límite de los últimos dos segundos para no ocultar pausas. Sin un
intervalo medido devuelve cero, y al pasar dos segundos sin cuadros la
tasa caduca. Cinco regresiones deterministas cubren 200 Hz con 120 muestras,
arranque, inactividad, reanudación y borde temporal. Las cifras del baseline
anterior permanecen como evidencia original; esta corrección no convierte
la instrumentación interna en una medición del compositor o de GPU.

### Reproducción y límites actuales

El paquete local está en
`/home/nihuel/.npm-global/lib/node_modules/rea-agents`. Su SDK permite
reproducir la conexión sin instalar dependencias:

```javascript
import { createRequire } from "node:module";
const require = createRequire(
  "/home/nihuel/.npm-global/lib/node_modules/rea-agents/package.json",
);
const { Client } = await import(require.resolve("@modelcontextprotocol/client"));
const { StdioClientTransport } = await import(
  require.resolve("@modelcontextprotocol/client/stdio"),
);
const client = new Client({ name: "rune-research", version: "1" });
const environment = Object.fromEntries(
  ["PATH", "HOME", "USER", "SHELL", "LANG", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DISPLAY"]
    .flatMap((key) => process.env[key] === undefined ? [] : [[key, process.env[key]]]),
);
await client.connect(new StdioClientTransport({
  command: "/home/nihuel/.npm-global/bin/rea",
  args: ["mcp"],
  env: environment,
  stderr: "pipe",
}));
try {
  await client.listTools();
  await client.callTool({ name: "binary_session", arguments: {} });
  await client.callTool({ name: "open_binary", arguments: {
    path: "/home/nihuel/projects/personal/rune/target/release/rune",
  } });
  await client.callTool({ name: "inspect_artifact", arguments: {} });
} finally {
  await client.callTool({ name: "close_binary", arguments: {} });
  await client.close();
}
```

`capture_process_scenario` recibió el contenido de `bench-scenario.json` o
`screenshot-scenario.json`: ejecutable absoluto, argumentos `--bench` o
`--screenshot <archivo> --channel general`, directorio de trabajo aislado
`.../run`, `WINIT_UNIX_BACKEND=wayland`, límite de 20 segundos y observación
de archivos solo en ese directorio. Estos modos desactivan SQLite
(`--screenshot` sin `--data`); no se abrió la caché de la cuenta. La
corrección del segundo intento consistió en propagar al transporte las
variables gráficas de la lista anterior. No se registró el entorno privado.
Equivalentes CLI, desde un shell de la sesión gráfica:

```sh
rtk rea mcp doctor --json
rtk rea inspect-artifact /home/nihuel/projects/personal/rune/target/release/rune --json
rtk rea capture-process /tmp/rune-rea-20261014-vlNyi6/bench-scenario.json --json
```

Se consultaron las disponibilidades y schemas de **6.4.0**, no se infirieron
a partir de README/main. No hay motor profundo configurado: Ghidra requiere
12.1.x (build verificado 12.1.4), JDK completo de 64 bits acorde a sus
metadatos, `GHIDRA_INSTALL_DIR` absoluto y `ps` compatible con procps.
`inspect_binary_layout` requiere Python con pwntools 4.15.0 y
`REA_PWNTOOLS_PYTHON`; tampoco está configurado. No se instalaron motores.
Las herramientas `observe_native_ui` y `capture_native_ui_scenario` de esta
versión requieren macOS: la captura de RUNE en Wayland usó el proceso
seleccionado y su propio modo screenshot, sin automatización nativa de REA.
No se analizó el binario ni el proceso de Discord.

Referencias: [instalación de REA](https://github.com/morluto/rea/blob/main/docs/installation.md),
[captura de procesos](https://github.com/morluto/rea/blob/main/docs/process-capture.md)
y schemas guardados en `tools.json` del directorio de evidencia.

## Release con búsqueda y contador corregido (2026-10-14, 19:15 UTC)

Se repitieron `open_binary`, `inspect_artifact` y
`capture_process_scenario` con `--bench` en una sesión MCP nueva, una vez
cerradas las ventanas de QA y terminadas las compilaciones y pruebas.
El nuevo `target/release/rune` mide **41 548 232 bytes**, SHA-256
`e5d727776747b614ae882c7f4edc8993c85335f2a47abeb2fd6b2b94baa0c800`.
Incluye búsqueda local y el contador que calcula intervalos sobre la
duración observada, en lugar de dividir las muestras por dos segundos.

La evidencia íntegra y el script reproducible `capture.mjs` quedaron en
`/tmp/rune-rea-20261014-search-t1y0wd/`, conservando los archivos del
baseline. `evidence-bundle.json` es la exportación canónica de REA.

- Inventario: `ev_a986059c3668ef5581965354628bbf28120d525ef2e245ccdf01fca5c9526c04`;
  un nodo ELF x86_64 identificado al abrir, tamaño y hash verificados.
- Benchmark: `ev_00d59b186a4671afff965c0e4135e7c6c2315dbcbb4f35b16bea59d69cbc90c2`;
  exit 0, sin truncamiento, grupo propio verificado y directorio temporal
  de REA eliminado. La captura duró **5,445 s**.
- Stdout de RUNE: **601 cuadros, 150 mensajes**, primer cuadro
  **78,9 ms**; UI p50/p95 **0,26/0,47 ms**; layout **1,18/2,94 ms**,
  máximo **9,79 ms**; pintado **0,03/0,16 ms**. El panel informó
  **120,1 cuadros/s**, **130,4 MB RSS** y **41,9 % CPU de proceso**.

La cifra interna ya supera el límite artificial de 60 del baseline.
Sigue contando intervalos de pintura de la aplicación, no presentaciones
verificadas por el compositor. **No se atribuye la diferencia de tiempos a
una optimización**: esta ejecución terminó con 150 mensajes cargados,
frente a 400 en la anterior, y las condiciones de carga del equipo también
eran distintas. Permanecen los límites de muestreo, entorno heredado y
actividad de red no observada descritos arriba. No se ejecutó otra captura
de ventana durante este benchmark ni se analizó software ajeno.

## MCP tras reinicio y pantalla OAuth2 (2026-10-14)

Tras reiniciar Codex, `binary_session` respondió directamente mediante las
herramientas MCP disponibles: REA **6.4.0**, cliente
`codex-mcp-client 0.162.1`, protocolo `2025-06-18`, **170 herramientas**.
No fue necesario reinstalar ni repetir el registro. Se analizaron y
ejecutaron únicamente artefactos propios de RUNE.

La primera captura del login descubrió un fallo real del host: el modo
`--screenshot` esperaba servidores aunque OAuth2 estuviera en `SignedOut`.
REA registró timeout de 15 s y SIGTERM, sin imagen. Se corrigió
`capture_step` y se añadió una regresión que falló antes del arreglo.
La aplicación interactiva no dependía de esa condición de captura.

El release corregido mide **45 385 856 bytes**, ELF x86_64, SHA-256
`cdf98ec689748d6e36085fb68957c9f232f3095f91a4bdce76e8c50f782cc28b`.
`capture_process_scenario` con `--provider discord --discord-client-id
<ID público> --screenshot .../sign-in.png` terminó **exit 0** y guardó
una captura revisada visualmente. RUNE informó primer cuadro a **84,3 ms**;
es una muestra de arranque, no un benchmark comparativo.

- Evidencia de captura corregida:
  `ev_d77f2c1e639b9fc541e2025f68d13be48083a0b2376517931bdbdb9f1dea741b`.
- Imagen: `/tmp/rune-rea-oauth-ll6joyr3/sign-in.png`, 139 834 bytes,
  SHA-256 `0c19aef862f421722d2874f1e538f05234129b338b2adaaa41eebb4df9070923`.
- Bundles canónicos: `/tmp/rune-rea-oauth-ll6joyr3/evidence-bundle.json`
  y `fixed-evidence-bundle.json`; se conserva el intento fallido.
- Entorno gráfico pasado explícitamente: Wayland, `wayland-1`,
  `/run/user/1000`, `DISPLAY=:0`. Observación de archivos limitada a la
  evidencia propia; el proveedor no abrió SQLite.

Esta captura muestra la pantalla previa al consentimiento: no inicia el
login ni demuestra conectividad autenticada. La prueba interactiva posterior
la realiza la persona en su navegador; códigos, tokens y datos de cuenta no
se incluyen en la evidencia de REA. No se inspeccionó el cliente Discord.

## Proveedor de sesión personal opcional (2026-10-14)

Se compiló el release con `session-login`. REA abrió el ELF x86_64 de
**45 742 864 bytes**, SHA-256
`ab6b97998bdd31e40ca6872fd50695ad6271b8090adbad228d1a14b2f1e2a885`,
y ejecutó `--provider discord-session --screenshot .../sign-in.png` con el
entorno Wayland anterior. El proceso terminó **exit 0** y la captura se
revisó visualmente: identifica el proveedor no oficial, el alcance de
identidad y el botón para abrir el login temporal. No se activó ese botón
durante la captura ni se abrió SQLite.

- Evidencia: `ev_c37777ae14ea54cf22dbe2fd37d243944e1181ce4a474e220f94ea1ff0a66911`.
- Directorio: `/tmp/rune-rea-session-20261014-lWjaOX/`.
- Imagen `sign-in.png`: 177 967 bytes, SHA-256
  `fc0952eb003c124c093356ce5ac6d8607655ec025a5af23535d16c944e8f1005`.
- Bundle canónico: `evidence-bundle.json`.
- RUNE informó primer cuadro a **453,5 ms**. Es una sola muestra, con otras
  compilaciones activas en el equipo; no permite atribuir una regresión ni
  comparar rendimiento con los releases anteriores.

REA verificó su grupo de procesos y eliminó su directorio temporal de
ejecución. Reportó además cinco procesos no verificables por `EACCES` al
consultar `/proc/.../environ`; por tanto esta captura no demuestra limpieza
global de procesos. La observación de archivos se limitó al PNG y no es una
traza de todas las escrituras. Tampoco demuestra conectividad autenticada.

El auxiliar GTK/WebKit se probó por separado mediante HTML sintético local
con CSP sin red: entrega, cancelación y cese de ejecución del auxiliar y de
sus descendientes observados. Son fixtures de RUNE, no una inspección del
cliente oficial de Discord. La prueba manual del dueño se abrió después,
fuera de la captura de REA; su resultado estaba pendiente en esa etapa.
El dueño confirmó después la identidad, según la revisión siguiente. Ver
[DISCORD_SESSION.md](DISCORD_SESSION.md).

## Corrección del runtime multimedia y nueva pantalla de acceso (2026-10-14)

Después de corregir el descubrimiento de plugins GStreamer y rediseñar el
acceso nativo con limn, REA abrió el release ELF x86_64 de **45 755 400
bytes**, SHA-256
`05aefe34eb19408108d3c45c93fc2849bc3674b79a158dfd8305fd768e3f4e94`.
La captura `--provider discord-session --screenshot` terminó **exit 0** en
Wayland, sin iniciar sesión. Revisión visual: tarjeta gris oscura compacta,
acento azul violeta, identidad original RUNE, alcance no oficial visible,
sin recortes ni tooltip tapando el botón inicial.

- Evidencia: `ev_ea0b0d410dd9745a8c2da7344ef37f966149a853d36c47308f5d455c5d1bf41a`.
- Directorio: `/tmp/rune-login-fix-20261014-SUa2Gu/`, con `sign-in.png` y
  `evidence-bundle.json`.
- PNG: 143 957 bytes, SHA-256
  `b8bbab3f508d1edf7b5d7333241bf29e2dc3e94cbd526ef1e8d51569cd6bbe46`;
  copia de documentación en `docs/img/rune-login.png`.
- Primer cuadro informado por RUNE: **144,7 ms**, una muestra de arranque,
  sin comparación causal de rendimiento.
- Grupo propio verificado y temporal eliminado. REA reportó un proceso no
  verificable por `EACCES`; no se afirma limpieza global de procesos.

El diagnóstico del fallo se hizo por separado: metadata de stacks nativos
del journal y fixtures propios sin red ni credenciales. Un probe previo
cargó la página pública de acceso en una sesión efímera con la entrada
deshabilitada. No se abrió memoria de los core dumps ni se inspeccionó el
cliente Discord.
El fixture multimedia falló sin plugins y pasó con el entorno corregido;
el detalle está en [DISCORD_SESSION.md](DISCORD_SESSION.md). Esta captura
acredita la pantalla inicial y no una autenticación real. RUNE se reabrió
después con el entorno Nix actualizado para la prueba manual del dueño.

## Entrada al workspace y chat: artefacto actual (2026-10-14)

REA abrió el nuevo release ELF x86_64, **47 671 096 bytes**, SHA-256
`9e797af0f6ff8e6aa48fbd3ee42f4e467e218046f4bcf8d462e7f14f553ebaec`.
`capture_process_scenario` ejecutó `--provider discord-session --screenshot`
en Wayland y terminó **exit 0**. La captura, revisada visualmente, muestra
una tarjeta compacta que usa el tema de RUNE, un texto breve y el botón de
acceso. Reemplaza la copia anterior de `docs/img/rune-login.png`; ya no
repite avisos del proveedor en esa pantalla. El foco del botón es visible,
sin recortes del título ni de la acción.

- Evidencia: `ev_badc2cb216cbc343721c8029577c719359fcbca7a958c94935ff07ecd88a0cff`.
- Directorio: `/tmp/rune-chat-20261014-7uMXZy/`, con `sign-in.png` y
  `evidence-bundle.json`.
- PNG: 93 394 bytes, SHA-256
  `66c080592228f52c5c47409c586121d4c35ef27d7d8d1d3307d75c5311c123a5`.
- Primer cuadro informado por RUNE: **112,6 ms**, una muestra de arranque;
  no acredita una mejora causal respecto de artefactos anteriores.
- Grupo propio verificado y temporal de ejecución eliminado. Las muestras
  de procesos pueden omitir descendientes breves; la observación de
  archivos se limitó al PNG. No se observó tráfico de red.
- El hash del ejecutable es una muestra previa al lanzamiento, no una
  vinculación atómica de la imagen cargada por el sistema operativo.

No se inició sesión durante esta captura. La sesión de análisis del ELF se
cerró y RUNE se abrió después dentro del entorno Nix con los plugins
GStreamer para la prueba del dueño. Su confirmación anterior acredita login
e identidad; la carga real de servidores, conversaciones y operaciones de
chat del nuevo artefacto todavía requiere esa prueba manual. Las pruebas
locales del protocolo están documentadas en [DISCORD_SESSION.md](DISCORD_SESSION.md).

## Imágenes y selector de expresiones (2026-10-14)

REA ejecutó el release final ELF x86_64 de **51 043 896 bytes**, SHA-256
`0ab1cd4951d3d63949d9bcca012cfcca57c1e591b205ac8eff0d36402c1a3524`.
Se usó el proveedor simulado en Wayland, sin iniciar sesión ni consultar
recursos de la cuenta. Las tres ejecuciones finalizaron con **exit 0**.

Los argumentos comunes fueron `--provider mock --screenshot <archivo>
--channel general --action composer:emoji`. Para las otras pestañas se
añadió `--action composer:emoji:tab=stickers` o
`--action composer:emoji:tab=gif`.

El directorio local de evidencia es `/tmp/rune-artwork-validation-bir0d1/`.
El bundle `checked-evidence.json` contiene seis registros y tres incógnitas;
la sesión de análisis del binario se cerró después de exportarlo.

| Captura | Evidencia REA | SHA-256 del PNG |
| --- | --- | --- |
| `checked-emoji.png` | `ev_02c3ca284005e3fbf9ad6176cf6089b84fad474362aa65ccb13f48ee482c972b` | `5605461c3967d60c1a8d5e69a246e0a0424e184c47b4be7fecef7d4ce4140db0` |
| `checked-stickers.png` | `ev_8d4e75c0067b5fdc387f2d930e18a0fba9bafca3bc0e9d778b33c0a36ea877e0` | `d705593153a6a64bcaca0c513ff0c8c544650ae7be46760e3fed8bdf012188fc` |
| `checked-gif.png` | `ev_f054d2ee6d2f68f90faa504e24d7db807a47626ac0b71c83b2da75b12dd3f18c` | `e6c42caac9a18c1bfc9159976937edd4f6da2b266f7c657c22c358da11425dba` |

La revisión visual confirma que los huecos de glifos observados inicialmente
están cubiertos en esa vista, que la pestaña seleccionada tiene contraste y
que las pestañas y el botón de insertar GIF conservan su altura. Los emojis
Unicode siguen siendo monocromos; algunas secuencias compuestas requieren
otra solución. Stickers muestra el estado vacío del proveedor simulado;
GIF muestra la entrada de URL y el estado de recientes. No acredita un
catálogo real, envío de stickers, búsqueda de GIF ni animaciones.

El hash del ejecutable se obtuvo antes del lanzamiento y no vincula de
forma atómica la imagen ejecutada. El muestreo de procesos puede omitir
procesos breves y la observación del archivo compara estados antes y
después, sin trazar todas las llamadas al sistema. El entorno heredado no
quedó registrado. No se analizaron credenciales, tráfico autenticado ni el
cliente oficial. Detalles de implementación y pruebas de componentes en
[ARTWORK.md](ARTWORK.md).

## Composer, indicadores y archivos recibidos (2026-10-14)

Se utilizó la skill `reverse-engineer-anything` y el MCP REA para ejecutar
el **binario propio de RUNE**, exclusivamente con proveedor simulado en
Wayland. Serein se estudió mediante su fuente pública; las capturas aportadas
por el usuario sirvieron como referencia visual. No se inspeccionó el
cliente oficial, una sesión autenticada ni sus recursos privados.

La captura inicial `before.png` ejecutó el release anterior
`0ab1cd4951d3d63949d9bcca012cfcca57c1e591b205ac8eff0d36402c1a3524`.
Evidencia: `ev_576d2d92ca7fc73514d38d3a6ce3c1ccc2f6dc807e6b26543cf9267df41e30ea`.
Una primera prueba con wrapper `nix-shell` falló por ausencia de `NIX_PATH`
en el entorno heredado por REA. Se resolvió lanzando el ELF directamente
con `LD_LIBRARY_PATH` obtenido del entorno Nix; no se alteró la instalación.

Tras corregir composer/badges/gap, el artefacto intermedio
`422dcdc92641f39dec724b765ab5c780bdbbc42f017c24cdbf04e6aef91e8073`
produjo `layout-fixed.png`, evidencia
`ev_8c0955358b8efedab5de97c38a47d7615bd3f12f83a7719dfa93413951bf0886`.
Permitió revisar el texto alineado y la separación del hover antes de
añadir el pipeline de adjuntos.

El **release final**, ELF x86_64, tiene SHA-256
`01c5fdce1341ec0c2f857c263cea7614ef0af898d0a351ea65c45193357049d2`.
Sus tres capturas terminaron con **exit 0**. Directorio local de evidencia:
`/tmp/rune-layout-20261014-8u9Fa6/`. El bundle
`attachments-evidence.json` conserva seis registros y tres incógnitas;
la sesión de análisis se cerró después de exportarlo.

| Captura | Evidencia REA | SHA-256 del PNG |
| --- | --- | --- |
| `attachments.png` | `ev_4450f9deb7dd7cd1dc8ac5893ffdda5c48ccc4e157245fcbf9881e4ea4812466` | `1b5b234a42ace0776ac3311b84e620896029784ededcfd0d50387ac07584e31d` |
| `attachments-clear.png` | `ev_4f9d691677f758782b26eea8441d8c2cd6a16fa602dbb26d4d2972a530623a82` | `20278b25f18f9de0cb8ed56aabd24f7912ec10176a2d53396b3b88d203b73c49` |
| `attachment-viewer.png` | `ev_d5d6416e4949a2bfc63fee8a4a6de2659ae1232255829df4abd136dc4303b13c` | `f16bd006934a8dbde0181a62a66321bd38a44afc080f3c7d34b9bafbd4f3ea2f` |

Se revisaron los PNG: preview proporcional sin recorte, nombre largo con
elipsis, tarjeta de archivo, separación respecto al composer de una/dos
líneas y visor limn con imagen y controles de zoom/cierre. La captura con
hover muestra además el tooltip del archivo; la siguiente permite ver el
nombre de la imagen. Los datos son una imagen geométrica original y un
archivo sintético: el harness no solicita sus URLs. Comandos repetibles
y alcance en [FILES.md](FILES.md).

Las capturas de conversación quedaron en una ventana estrecha controlada
por el compositor; no prueban todos los tamaños. Los tests de layout
complementan la observación con distintos anchos, métricas y temas.
El primer cuadro informado fue 133,7 / 93,4 / 87,7 ms respectivamente;
son tres muestras distintas, no un benchmark ni evidencia de mejora causal.

REA registra el digest antes de lanzar, no una vinculación atómica a la
imagen cargada por el sistema. El muestreo puede omitir procesos breves;
el filesystem se observa antes/después y no se captura tráfico. El entorno
heredado no queda registrado completo. Algunos procesos ajenos no pudieron
verificarse por permisos de `/proc` y quedaron intactos; el grupo propio se
verificó y el temporal de REA se eliminó. La descarga real desde Discord y
el modal de invitaciones no se validaron visualmente en estas capturas.

Validación del workspace: `cargo fmt --all -- --check`,
`cargo check --workspace --all-features`,
`cargo test --workspace --all-features` (**510 aprobadas, 5 ignoradas**),
`cargo clippy --workspace --all-targets --all-features -- -D warnings` y
`cargo build --release -p rune-desktop --features session-login`, correctos.
Las pruebas HTTP utilizan servidores locales sintéticos. No acreditan
conectividad de cuenta, aceptación de invitaciones ni envíos reales.


## Paquete instalado de Discord: revisión estática parcial (2026-10-14)

A pedido explícito del dueño se aplicó la skill REA al artefacto instalado
`discord-unwrapped-1.0.155/opt/Discord/modules/discord_desktop_core/core.asar`,
sin ejecutar JavaScript ni consultar datos de la sesión. SHA-256:
`a16d6be1f5d7d75bf6c01bbbaf1bb266527a3cab997024190570ddc1fa15b4c4`.

`analyze_javascript_application` examinó dos archivos JavaScript y conservó
un grafo **parcial**. Evidencia padre:
`ev_c3f8a243cd0ca1953eefaec9dd61dc033dfdf54a694e9a45ea117c9bd537a1ab`.
El inventario y las vistas se exportaron a
`/tmp/rune-discord-static-20261014-evidence.json` (evidencia local de consulta,
no dependencia ni recurso distribuido con RUNE).

- **Derivado del paquete:** `package.json` declara `bundle.js` como main;
  existe una referencia estática a `mainScreenPreload.js` como preload.
  Esto acredita separación entre proceso principal y preload, no una
  ejecución observada ni la estructura de los componentes de perfiles.
- **Limitación de REA:** `bundle.js` excedió el presupuesto del grafo
  semántico; el análisis semántico del preload agotó 512 MiB de heap.
  Se preservaron inventario y hechos parciales. No se interpreta lo no
  encontrado como inexistente.
- **Candidatos descartados:** los nodos etiquetados como endpoints incluyen
  llamadas `settings.get` y `Headers.get`. Esos nombres no son rutas HTTP
  verificadas. No se incorporan como APIs de RUNE.
- **Sin observación dinámica:** no hubo una captura del flujo oficial de
  aceptar invitaciones, editar perfiles o elegir GIFs. No se extrajeron
  contratos completos de esos flujos. La evidencia útil para implementarlos
  procede por ahora de documentación oficial, capturas del usuario y la
  fuente pública de Serein, identificadas separadamente.

Esta investigación no copia recursos de Discord ni acredita que las rutas
no oficiales de otros clientes estén autorizadas por Discord. Los cambios de
RUNE tienen implementación propia y pruebas con fixtures locales.


## Bienvenida, perfil y scroll nativo: baseline de la ronda paralela

El release `8fc7e92088d5fbd4d2295cbc60efa8af7fcf2a88072921a5e7cabba9df50bd08`
se compiló después de 526 pruebas aprobadas (cinco ignoradas), fmt, check y
clippy correctos. Tres escenarios REA terminaron con exit 0 en Wayland:

| Captura | Evidencia | Alcance |
| --- | --- | --- |
| `login.png` | `ev_2152ba7ee78f332346e69a486ec4bdf1eaf595c7e67ee6bdb393d34c3fe7b4c4` | Bienvenida nativa, sin activar login |
| `profile.png` | `ev_2cfaea7293a8c0e311a7b430a916aeeddc9d3ea5e4d3e8c537ceb1b8daad1edb` | Editor con datos simulados, disposición estrecha desplazable |
| `scroll.png` | `ev_69afb5be299077222f63618c0db3f5df78eff2a6b17d72e86696b1d52e4ec674` | Chat simulado, barra nativa con flechas y separación del composer |

Directorio: `/tmp/rune-profile-login-20261014-fyZvlt/`.
Bundle canónico: `baseline-evidence.json`. Las imágenes se revisaron:
la bienvenida tiene marca y acción centradas; el editor conserva cierre
fuera del contenido; el timeline reserva el rail nativo. El compositor
produjo ventanas estrechas y de diferente altura; las capturas no prueban
todos los tamaños ni los gestos de scroll. Esos casos se cubren además con
pruebas de layout/interacción. Los avatares del perfil simulado son iniciales;
la igualdad de los dos recursos de imagen recibida se probó sin ventana.

El digest del ejecutable es una muestra previa al lanzamiento. REA no
captura el tráfico de red y su muestreo puede omitir procesos breves. No se
inició sesión ni se editaron perfiles reales. Este es el baseline validado
antes de la ronda paralela de PDF, búsqueda GIF y banners de perfil; no
acredita todavía esas implementaciones posteriores.

## Integración final de los frentes paralelos

La ronda de PDF, búsqueda GIF, banners y permisos terminó con 562 pruebas
aprobadas y seis ignoradas en 23 suites. La prueba opcional del sandbox PDF
se ejecutó aparte y pasó. Fmt, check y Clippy con `-D warnings` pasaron.
El release se construyó con `withLogin=true` y `withDocs=true`, SHA-256:
`af9d264aba26d70bf33d77d8a4bdad3d92934cbcb2f288fba8e69ddf25be4a49`.

| Frente | Evidencia REA | Alcance comprobado |
| --- | --- | --- |
| PDF | `ev_674fabbc607fc408df47d607cb7589496adef5bd4388d504b22adcda30baabdb` | Test Rust con PDF propio de dos páginas; rasteriza sólo la primera mediante sandbox |
| GIF | `ev_bf3a1256859dcc146518102edd37194e85740a04cb9423aed722e6634871090c` | Ocho pruebas con servidor local: consultas, límites, cancelación y miniaturas |
| Banner | `ev_77d2c3da06a98cf82edc20d4d6498e08727a0a693f5553357fcc937c89ed3c56` | Geometría sintética del componente limn, sin un perfil real |
| Administración | `ev_93505b1d1cbff4aa69a1be738b0cb3ea0ec827936df28b158a88adf78725db00` | 25 pruebas locales, incluidas revocación de permisos y confirmaciones tardías |
| Ventana final | `ev_eed33b57cd2e738cb831f66b771d6e7856b7909b77e90e9722e172ae968c3fb4` | Release nativo en Wayland con adjuntos sintéticos; salida 0 |

La [captura final](img/rune-attachments.png) muestra la imagen de prueba, la
acción de primera página PDF, scroll nativo y separación del composer. No
activa la descarga PDF: el render real se verificó por separado con el fixture
del sandbox. SHA-256 PNG:
`c083537c7292b2ba327d08b10ee9be840568abaeaed249f5d335b433cefe6817`.
El compositor produjo una ventana estrecha de 1396×1660 píxeles; la captura
no acredita todos los tamaños. Se revisó visualmente tras generarla.

El bundle local final está en
`/tmp/rune-profile-login-20261014-fyZvlt/final-evidence.json`. Los registros
por frente y sus limitaciones están enlazados desde [WORKFLOW.md](WORKFLOW.md).
La búsqueda real GIPHY requiere una clave propia y aún no se probó. No se
enviaron mensajes ni se cambiaron perfiles, roles o servidores reales.

## Captura y procesamiento local de voz

La entrega siguiente usa REA sobre ejecutables propios, sin inspeccionar
Discord ni abrir micrófonos:

| Escenario | Evidencia | Resultado |
| --- | --- | --- |
| Callback, ciclo de vida y DSP sintéticos | `ev_b8cc758bef296ca6f088a15721c3e6d5edb034064047ec1f4db40e06ed5c412b` | 23 pruebas aprobadas |
| Controles de ajustes limn | `ev_8e149e27f7798179bd5f867e776fa527da8584b77e936b0f07906192fdaa0b07` | 18 pruebas aprobadas |
| Procesamiento release, señal generada | `ev_47695cf4a9f254ddee7bc60189050827df2df158d6a7ca8194316e468c7f9c7e` | p95 37 µs a 48 kHz por bloque de 10 ms, 0/1000 fuera de ese plazo |

Los registros preservan las limitaciones de observación y truncamiento de
terminal: [audio](evidence/rea-audio-tests-2026-10-14.json),
[UI](evidence/rea-audio-ui-2026-10-14.json),
[benchmark](evidence/rea-audio-processing-2026-10-14.json).
La metodología y los otros formatos están en [PERFORMANCE.md](PERFORMANCE.md).
Estos escenarios no demuestran calidad perceptual, latencia acústica,
funcionamiento de hardware ni llamadas remotas.

La [ventana de ajustes de voz](img/rune-audio.png) se capturó con datos
simulados, sin iniciar el micrófono: evidencia
`ev_e231b815077061ba54016c3b83a6ecfbd4eacc21d1275e43123cd40a5b191a88`,
salida 0, [registro](evidence/rea-audio-window-2026-10-14.json).
El primer intento no heredó variables de Wayland en el servidor MCP y
terminó sin abrir ventana; el replay exitoso las recibió explícitamente.
La captura se revisó a 1396×1660 píxeles; no acredita todas las geometrías.

## Aceptación de invitaciones (2026-10-14)

Serein se revisó como código público del checkout fijado, sin ejecutarlo;
el flujo y sus fuentes están en [INVITATIONS.md](INVITATIONS.md). REA se
aplicó al binario propio de pruebas de `rune-discord-session`, con servidores
HTTP/Gateway locales y datos sintéticos, filtro `invitation`:

- Evidencia `ev_c4e954855e9cb2f5be40de56f29ef809600ff888f5600c992bf77f2fd5f5dc4f`.
- **22 pruebas aprobadas**, 82 filtradas, salida 0, sin truncamiento.
- SHA-256 del binario observado:
  `1ef858decb2ed7ee5bbe6911aa436265733e103354e322c1f2ef994919995cd9`.
- [Registro completo](evidence/rea-invitations-2026-10-14.json).

Las aserciones cubren consulta anónima, POST explícito, cuenta/destino,
respuesta HTTP separada de membresía, límites y errores, cierre de sesión,
reconexión y plazos absolutos. REA observa salida y ciclo de vida del proceso;
no inspecciona paquetes de red ni prueba el comportamiento de Discord. El
hash previo al lanzamiento no es una vinculación atómica del ejecutable por
el SO. No se accedió a cuentas ni se aceptaron invitaciones reales.

La [ventana nativa](img/rune-invitation.png) usó exclusivamente el fixture
`--capture-invitation` del proveedor simulado. Evidencia
`ev_2648481e2756eec899c071fce11fb37d8b5cf3c247d47d75a48b70ce260cf5cb`,
salida 0, [registro](evidence/rea-invitation-window-2026-10-14.json).
Se revisaron cuenta, destino, conteos y tres acciones sin recortes en la
captura de 1396×1660. Esto no acredita todas las escalas ni una unión remota.
SHA-256 release:
`ff523ce1aaa959a0100e260e6ab98cb164683efe36c1158b1a547e200a1982b1`.

## Discord instalado: paquete y módulos nativos (2026-10-15)

El dueño autorizó expresamente inspección estática y pidió ampliar la pasada
parcial usando REA, Ghidra, Hopper y las herramientas disponibles. El objetivo
fijado fue **Discord 1.0.155 stable Linux x64** del store Nix, no un perfil o
una sesión. No se ejecutó Discord ni sus addons, ni se inspeccionaron cuentas,
memoria autenticada, credenciales o tráfico privado. No se copió código ni
recursos del cliente a RUNE.

El ELF `.Discord-wrapped` mide **206 974 168 bytes**, SHA-256
`74a74702e98e89c8e66aa64b0ce563dd46bed7bd72a5e94e4e6c73bd5b909b9c`.
La primera pasada de REA sobre `app.asar` y `core.asar` fue parcial por
presupuesto de resultado y heap del analizador JavaScript. Los registros padre
son `ev_58a00a77f29ce443ff4aef102019b13348b7129d4aa76c59dcfde99e5759ad45`
y `ev_ffa5989d259e6bf71aee051c553c6ea54a668eb3cea81d9f56fec1525a14043d`.
El export de 67 registros está en
`/tmp/rune-discord-gap-analysis-20261014-final-evidence.json`; el ledger inicial,
en `/tmp/rune-discord-gap-analysis-20261014-findings.json`.

REA extrajo `core.asar` de forma segura
(`ev_eb83854e715d783f09dcf08140e43ed6d611dc344f98d74c87e14f02e118e348`).
La lectura literal suplementaria del preload y bundle identificó fronteras
de `DiscordNative`, selección de fuentes de pantalla/ventana, diálogo de
archivo, revelado, clipboard de imagen y badge. No reconstruyó automáticamente
sus flujos completos. `mainWindow.loadURL(URL_TO_LOAD)` muestra que el shell
carga el renderer remoto; el paquete local no basta para reconstruir chat,
sociales, hilos y foros. Una consulta anónima a `discord.com/app` devolvió
403, sin reintentar ni eludir el rechazo. El renderer exacto sigue desconocido.

### Ampliación real con Ghidra

Se instaló **Ghidra 12.1.2** de forma aislada en Nix, sin cambiar perfiles ni
configuración global, y se reutilizó OpenJDK 21.0.12. REA 6.4.0 produjo
**14 dossiers de funciones** de plataforma sobre `discord_utils.node`.
La inspección ELF con pwntools 4.15.0 funcionó para ese addon; su límite de
32 MiB excluye el addon de voz y el ejecutable principal. `readelf`, `nm` y
`objdump` complementaron los originales, sin alterar ese límite.

El autoanálisis global REA/Ghidra de `discord_voice.node` alcanzó su timeout
de **20 minutos**. Se conservaron el error y los logs; no se repitió el mismo
análisis. Un perfil Ghidra focalizado, sin autoanálisis global y con 2 GiB de
heap, produjo **23 descompilaciones seleccionadas de voz**. Son evidencia
suplementaria: no dossiers REA completos, ni una importación exhaustiva.
Los límites `STT_FUNC` se contrastaron con el ELF porque el descompilador puede
incorporar el destino de un tail-jump como si fuera parte de un wrapper.

El [ledger nativo](evidence/discord-native-2026-10-15.json) conserva hashes,
versiones, 14 observaciones y propuestas de flujo, IDs de evidencia,
procedencia, archivos retenidos y límites. No contiene pseudocódigo ni
recursos propietarios. Los bundles completos y el proyecto Ghidra focalizado
permanecen en `/tmp/rune-discord-native-20261015-*`.

Las observaciones útiles para el diseño propio de RUNE son:

- Algunos métodos Linux de permisos y registro son stubs; no se debe copiar
  una respuesta constante como si acreditara soporte del SO.
- Idle/resume en Wayland usa disponibilidad de protocolo y reloj monotónico;
  registro de entrada distingue press/release y políticas de foco. Esto no
  demuestra entrega de atajos globales en este equipo.
- Voz distingue VAD/PTT, retardo de liberación, procesamiento, dispositivos y
  volumen. La prueba local tiene streams WebRTC y una ruta Opus 48 kHz;
  otras ramas usan PCMU. No se deduce un único formato para toda llamada.
- Cambios de dispositivo reconstruyen streams; configuración y reconexión
  pasan por trabajo encolado, con errores/callbacks distintos.
- El picker Linux de pantalla/ventana distingue disponible, mostrar, error
  y cerrar sesión. Cámara requiere una frontera y permiso propios.
- Filtros validan destino y metadatos de imagen. Estadísticas distinguen
  streams, pérdidas, códec, RTT, resolución y límites de recursos; esos campos
  no prueban cómo es el panel visual del renderer.

Hopper no estuvo operativo: la descarga oficial fijada de la demo devolvió
**HTTP 403** y la API oficial de releases venció su plazo. No se usaron
binarios de terceros ni evasiones. IDA no estaba instalado/configurado ni
se observó licencia. Por eso no se afirma haberlos usado para descompilar.
Los tres artefactos conservaron su SHA-256 al finalizar, y las sesiones y
procesos propios de análisis quedaron cerrados.

La investigación amplía las fronteras observadas, pero sigue **parcial** en
cobertura total y no acredita ejecución, paridad visual, calidad de audio ni
operaciones reales. Los 21 recorridos de RUNE y su implementación se siguen
por separado en [tasks/todo.md](../tasks/todo.md).

## Checkpoint local de storage e invitaciones (2026-10-15)

El [registro del checkpoint](evidence/implementation-checkpoint-2026-10-15.json)
conserva el ejecutable y los hashes de cuatro capturas propias. El workspace
aprobó **716 pruebas**, con seis ignoradas y 23 suites; formato, Clippy con
`-D warnings` y compilación release con todas las features fueron correctos.
Este resultado corresponde al bloque local de almacenamiento, invitaciones y
guards iniciales de capacidades; no completa los 21 recorridos.

REA ejecutó cuatro procesos de RUNE con proveedor simulado y un
`XDG_DATA_HOME` temporal aislado. Todos terminaron con código 0. Se revisaron
visualmente estas capturas de 1396×1660 píxeles:

- [Borrador persistido](img/persisted-draft-2026-10-15.png):
  [evidencia del proceso](evidence/final-persisted-draft-2026-10-15.json).
- [Reapertura desde caché](img/cached-start-2026-10-15.png), con ocho segundos
  de latencia de red simulada y mensajes ya visibles:
  [evidencia del proceso](evidence/final-cached-start-2026-10-15.json).
- [Tarjeta de invitación](img/invitation-card-2026-10-15.png), consulta explícita:
  [evidencia del proceso](evidence/final-invitation-card-2026-10-15.json).
- [Modal de invitación](img/invitation-modal-2026-10-15.png), destino simulado:
  [evidencia del proceso](evidence/final-invitation-modal-2026-10-15.json).

La base resultante tenía esquema 3, 50 mensajes, el borrador esperado y
permisos `0600`. La instantánea persistida no conservó presencia ni voz.
El binario observado pesaba 53 342 120 bytes, SHA-256
`7e0a154bc86ff9827714932a9f1b08bd9ae547e722a4587187e1ba57bd0dac52`.

No se utilizaron sesiones autenticadas ni se aceptaron invitaciones reales.
La persistencia del proveedor real y su aceptación permanecen deshabilitadas.
Las capturas acreditan esa geometría y esos fixtures, sin prometer paridad
visual completa. Los registros observan ciclo de vida y metadatos de archivos
antes/después; no todos los syscalls. El hash previo al lanzamiento no es una
vinculación atómica con la imagen cargada por el sistema operativo.

## Reconciliación de fijados (2026-10-15)

La lectura por petición, su delta concurrente y el ciclo de vida completo
pasaron **741 pruebas** de workspace, seis ignoradas y 23 suites; formato,
Clippy estricto y release correctos. REA ejecutó el panel propio con proveedor
simulado, sin cuentas reales, en dos estados revisados a 1396×1660:
[lista cargada](img/pins-2026-10-15.png) y
[espera con latencia simulada](img/pins-loading-2026-10-15.png).
Ambos procesos terminaron con código 0.

El [checkpoint](evidence/pins-checkpoint-2026-10-15.json) enlaza los registros
[cargado](evidence/pins-ready-2026-10-15.json) y
[loading](evidence/pins-loading-2026-10-15.json), y retiene una copia temporal
del ejecutable observado. El SHA-256 de 53 385 008 bytes es
`f8e8f92ebfd288b21dc571047e1a1d1f0996e2def1bbe061489565baa6a1824e`.
Los límites de observación del checkpoint anterior siguen aplicando: hash
previo al lanzamiento, entorno heredado parcial, muestreo de procesos y
metadatos de archivos. Procesos ajenos con lectura denegada no se consideran
verificados por la limpieza del grupo propio. No se habilitaron fijados
remotos ni se acredita paridad visual completa.

## Patches y expulsión de caché (2026-10-15)

El [checkpoint de implementación](evidence/message-patches-checkpoint-2026-10-15.json)
registra **781 pruebas aprobadas**, seis ignoradas y 23 suites, con formato,
Clippy estricto y release correctos. Tres regresiones iniciales reprodujeron
el descarte de patches y el rechazo de edición/borrado tras 513 expulsiones.
Las pruebas añadidas verifican recuperación acotada de autoría, cancelación,
permisos y la reconciliación de optimismo con datos confirmados.

REA ejecutó los binarios propios ya compilados:

- [Adaptador](evidence/adapter-patches-process-2026-10-15.json): **131 pruebas
  unitarias aprobadas**, salida 0. Terminal crudo completo (14 122 bytes,
  92 frames); el render de terminal retuvo 18 de 92 frames por su presupuesto
  de 65 536 bytes. Las dos pruebas de documentación se verificaron con Cargo.
- [State](evidence/state-patches-process-2026-10-15.json): **13 regresiones
  aprobadas**, 47 filtradas, salida 0 y terminal sin truncamiento.

El registro conserva hashes y copias temporales de ambos binarios de tests
y del release, de 53 466 320 bytes, SHA-256
`7963773356397d310f6ad30d461f91458546555713daf412d580d04095db1a6b`.
Se usaron fixtures sintéticos; la captura acredita recibos de tests y ciclo de
vida, sin trazar tráfico de red ni efectos de filesystem. El hash previo al
lanzamiento tampoco es una vinculación atómica con la imagen del SO. No se
anuncia una nueva prueba visual de este cambio de lógica, ni operaciones de
cuenta, nuevas capacidades remotas o persistencia de la sesión real.


## Búsqueda guardada y contexto por ID (2026-10-15)

El [checkpoint](evidence/search-context-checkpoint-2026-10-15.json) registra
**851 pruebas aprobadas**, seis ignoradas y 23 suites, formato, Clippy estricto
sin avisos y release correcto. El ejecutable retenido tiene 53 863 320 bytes y
SHA-256 `b119ee78673b160a0dfbf8b28ca2ce88f42731644d946d6370a82354cf3e73b8`.

Tres capturas nativas del ejecutable propio, con limn y datos simulados, se
revisaron visualmente y terminaron con código 0:

- [Búsqueda en este equipo](img/search-stored-2026-10-15.png), 1396×1660:
  20 resultados paginados, filtros y alcance incompleto visibles; borrador intacto.
- [Contexto por ID](img/message-context-2026-10-15.png), 1396×1660:
  mensaje anterior a la ventana cargada, resaltado, origen y vuelta a recientes.
- [Carga de contexto](img/message-context-loading-2026-10-15.png), 2824×1660:
  spinner, cancelación y borrador conservado, con ocho segundos de latencia simulada.

Sus registros íntegros son [búsqueda](evidence/search-ready-2026-10-15.json),
[contexto](evidence/context-ready-2026-10-15.json) y
[carga](evidence/context-loading-2026-10-15.json). Ninguno está truncado.
La primera observación de búsqueda fue demasiado amplia: incluyó binarios
retenidos y caché gráfica, agotando enumeración/hash de archivos. Se conserva
como intento inicial en el directorio temporal; se repitió con sólo imagen y
base de datos como rutas observadas. El checkpoint distingue ambos intentos.

REA ejecutó además copias verificadas de dos binarios de pruebas, sin truncar:
[núcleo](evidence/local-history-process-2026-10-15.json), **101 aprobadas y una
ignorada**, y [adaptador](evidence/context-session-process-2026-10-15.json),
**150 aprobadas**, ambos con salida 0. Dos doctests del adaptador pertenecen a
la ejecución Cargo, no al replay. Estos registros de subagentes se conservan
por separado del bundle exportado por el integrador.

La base aislada de la simulación conserva SQLite v3, permisos 0600, 101 mensajes
y el borrador exacto «Borrador de contexto intacto» en el canal104 de
`12:rune.mock.v1:1`. Se inspeccionó con una conexión SQLite de sólo lectura.
La memoria de respuestas sigue acotada; los resultados atrasados no pueden
revertir patches antes de persistir. El contexto no prueba continuidad ni
marca leído; la búsqueda guardada no representa todo el historial remoto.

Se usaron acciones limn instrumentadas y la captura nativa del viewport; esto
no acredita entrada física, lector de pantalla ni paridad exacta con el renderer
de Discord. Los procesos se muestrean, el entorno heredado no se registra por
completo y los snapshots de archivos no son trazas de syscalls. Hash previo al
lanzamiento y copia retenida no prueban una vinculación atómica con la imagen
del SO. No hubo nuevas operaciones de cuenta, persistencia real ni reactivación
de la aceptación de invitaciones.

## Lecturas y bandeja por cuenta (2026-10-15)

El [checkpoint](evidence/reads-inbox-checkpoint-2026-10-15.json) registra
**925 pruebas aprobadas**, seis ignoradas y 23 suites; formato, Clippy con
`-D warnings` y release correctos. El ejecutable final retenido tiene
54 082 288 bytes y SHA-256
`ac60ab0d0ca25bc666b8cb162a1b535321a8ab2c2212e01c01d32c94c14f6dde`.
La migración SQLite v4 guarda elecciones locales por cuenta/canal; las
confirmaciones versionadas se ejercitan con el proveedor simulado. Los
adaptadores reales mantienen `read_state=false` y SQLite deshabilitado.

Cuatro capturas limn del ejecutable propio terminaron con código 0, sin
truncación, y fueron revisadas por integrador y agente independiente:

- [Bandeja](img/inbox-2026-10-15.png), 1396×1660: cuatro conversaciones,
  incluido el privado Ada; alcance de menciones conocidas y borrador visibles.
- [Recordatorio reabierto](img/read-reminder-reopened-2026-10-15.png),
  2824×1660: conserva la elección manual exacta y el borrador tras otro proceso.
- [Salto desde bandeja](img/inbox-context-2026-10-15.png), 2824×1660: ancla
  anterior a la ventana reciente, origen del contexto, resaltado y vuelta a recientes.
- [Bandeja vacía](img/inbox-empty-2026-10-15.png), 2824×1660: mensaje compacto,
  sin lista ni scroll vacío; cuatro acciones explícitas sobre datos simulados.

Los registros completos son [bandeja](evidence/inbox-ready-2026-10-15.json),
[reinicio](evidence/read-reminder-reopened-2026-10-15.json),
[contexto](evidence/inbox-context-2026-10-15.json) y
[vacío](evidence/inbox-empty-2026-10-15.json). La selección previa de un mensaje
cargado se conserva en [su registro](evidence/read-reminder-selected-2026-10-15.json),
con el ejecutable intermedio identificado en el checkpoint. La lectura SQLite
de sólo lectura confirma esquema 4, permisos 0600, `unread_from=1040000850`,
revisión 1 y borrador exacto antes/después del reinicio. El salto fuera de
ventana usa una copia de esa base con ancla sintética explícita `1040000750`,
un mensaje real del proveedor simulado; no se afirma que la UI lo seleccionó
mientras estaba descargado. El contexto obtenido conserva el recordatorio.

REA ejecutó también [115 pruebas del núcleo](evidence/reads-core-process-2026-10-15.json)
con una ignorada, [94 de State](evidence/reads-state-process-2026-10-15.json)
y [32 del host](evidence/reads-desktop-process-2026-10-15.json), todas con
salida 0 y terminal completo. La del host atraviesa Mock/Client/Store para
reconciliar un ACK atrasado sin borrar una elección manual nueva.

El bundle raíz contiene 29 registros y cuatro incógnitas, incluidos intentos
anteriores. Se preservan sus binarios y registros por separado: la primera
captura reveló el permiso de historial ausente del fixture de privados; la
intermedia mostró la lista vacía con scrollbar. Ambas regresiones se corrigieron
y las imágenes canónicas corresponden al release final. La versión estrecha
puede abreviar el aviso superior de no leídos con puntos suspensivos.

Estas observaciones usan acciones limn instrumentadas y fixtures propios,
sin cuenta ni confirmación de lectura real. No prueban entrada física,
lector de pantalla, todas las geometrías o paridad con el renderer de Discord.
Los límites de muestreo de procesos, entorno heredado, metadatos de archivos
y hash previo al lanzamiento siguen aplicando; no son trazas de red o syscalls.

## Selector Linux: prerrequisito de adjuntos (2026-10-15)

El [checkpoint](evidence/file-picker-checkpoint-2026-10-15.json) registra
**959 pruebas aprobadas**, seis ignoradas y 25 suites; formato, Clippy con
`-D warnings` y release correctos. `rune-platform::file_dialogs` prepara un
descriptor de archivo regular mediante el portal Linux, con límites y
cancelación. En aquel checkpoint todavía no estaba conectado con Core,
el composer ni los envíos; la integración posterior se documenta en 07.1
abajo. Los recorridos completos 07 y 08 siguen abiertos.

REA volvió a ejecutar [las 34 pruebas del crate](evidence/file-picker-tests-process-2026-10-15.json)
con salida 0 y terminal completo. Dieciocho verifican filesystem/contrato y
dieciséis un bus privado sin acceso al bus de sesión. Las regresiones incluyen
normalización de rutas, activación del portal, cierre fallido, descarte de
futures y destrucción fuera del contexto Tokio original.

El ejemplo release retenido abrió el diálogo GTK nativo. Se verificaron
[cancelación](evidence/file-picker-native-cancel-2026-10-15.json) y
[selección](evidence/file-picker-native-select-final-2026-10-15.json), ambas
con salida 0 y sin truncación. La selección devolvió 50 bytes, dispositivo
66309 e inodo 19707114, iguales al fixture propio. No se leyó su contenido.
Un observador auxiliar de Hyprland confirmó la ventana nueva y su cierre;
antes de escribir sólo la ruta sintética y Enter comprobó que esa dirección
era la ventana activa. Título, clase, PID y tiempos dan atribución temporal,
sin vinculación criptográfica de la ventana a la petición.

Tres intentos previos de QA quedan conservados: observador iniciado tarde,
separador IPC omitido y sintaxis antigua de foco rechazada por Hyprland Lua.
Ninguno envió entrada. La ejecución final usa el mismo binario y exige que el
diálogo ya tenga el foco. También se revisó una [captura de humo de RUNE](evidence/file-picker-workspace-smoke-2026-10-15.json)
con datos simulados; no se cuenta como una nueva captura canónica de recorrido.

El checkpoint conserva hashes y copias del ejemplo, los tests y el ejecutable
principal. El bundle raíz incluye 36 registros, cuatro incógnitas y evidencia
de etapas anteriores. La observación del compositor es suplementaria a REA.
En ese checkpoint no se anunciaron uploads, Guardar como, avisos del SO,
soporte Windows/macOS, entrada física ni nuevas operaciones de cuenta.
Wayland aún no exporta un parent handle; el runtime original debe seguir
vivo para completar limpieza.

## Adjuntos: recorrido nativo de un archivo (07.1, 2026-10-15)

El [checkpoint](evidence/uploads-checkpoint-2026-10-15.json) registra la
integración del selector Linux con Core, composer limn y envío **simulado**.
El snapshot final, incluida la limpieza de pendientes al eliminar canal o
servidor, pasó **1032 pruebas**, seis ignoradas y 25 suites con todas las
features; la configuración por defecto pasó 1028 con una ignorada. Formato,
Clippy estricto y release con todas las features fueron correctos; release
compiló 11 crates en 66 segundos. Un intento previo agotó el espacio de la
caché incremental generada. El registro conserva su recuperación y el recall
RED de ENOSPC; no se cambiaron fuentes ni evidencia retenida al liberar espacio.

El release retenido tiene **56 667 496 bytes**, SHA-256
`dc963e25fcf83d14cb2b4e0538cb65849a0cae575c3bd8cbf2b30dde4b31cd5d`.
Dos capturas de 1396×1660 del mismo ejecutable terminaron con código 0 y
sin truncación, y fueron revisadas por integrador y agente independiente:

- [Preparado](img/upload-prepared-2026-10-15.png): `seleccionado.txt`, 50 B,
  junto al borrador intacto «Borrador con archivo»; elegir no envió el mensaje.
  [Registro REA](evidence/uploads-native-prepared-2026-10-15.json).
- [Confirmado por Mock](img/upload-sent-2026-10-15.png): «Archivo de prueba
  enviado» con el adjunto de 50 B, composer vacío y sin pendiente.
  [Registro REA](evidence/uploads-native-sent-2026-10-15.json).

El observador auxiliar se inició antes del proceso y comprobó la ventana GTK
nueva y su foco antes de escribir sólo la ruta del fixture propio y Enter.
Después del cierre del portal, el Enter de envío exigió ejecutable retenido,
PID, dirección activa y título «RUNE — general» exactos. El
[intento anterior](evidence/uploads-native-send-attempt-2026-10-15.json)
quedó preparado sin enviar: el guard de QA esperaba «RUNE», mientras el host
ya había actualizado el título al canal. Abortó antes de introducir Enter y
no requirió cambios de producto. Su código de salida 0 acredita el proceso,
no un envío exitoso.

REA ejecutó además los cinco binarios de pruebas retenidos: [Core](evidence/uploads-core-tests-2026-10-15.json)
146 aprobadas y una ignorada, [State](evidence/uploads-state-tests-2026-10-15.json)
107, [UI](evidence/uploads-ui-tests-2026-10-15.json) 299,
[Provider](evidence/uploads-provider-tests-2026-10-15.json) 50 y
[Desktop](evidence/uploads-desktop-tests-tests-2026-10-15.json) 33. Todos
terminaron con salida 0 y terminal completo. El checkpoint identifica los seis
artefactos retenidos, sus hashes y las regresiones RED/GREEN de presupuesto,
cambio de archivo, cancelación, ACK, reintento, cierre y limpieza de metadatos.
El bundle raíz conserva **44 registros y cuatro incógnitas**, incluidas
observaciones e intentos anteriores.

Core mantiene descriptores y bytes fuera de UI con **32 MiB y ocho entradas**;
los límites de Mock en ese checkpoint fueron **un archivo, 20 MiB por archivo y 25 MiB por
mensaje**, incluido el texto. Son políticas locales de simulación. Los bytes
preparados viven en RAM y el reintento usa el mismo cuerpo sin reabrir la ruta.
Los proveedores reales mantienen uploads deshabilitados; al cerrar 07.1, varios archivos,
arrastrar/soltar, pegar archivos y transporte remoto seguían pendientes. Los
checkboxes completos 07 y 08 permanecen abiertos. Ver [contrato](../tasks/uploads-contract.md)
y [límites](FILES.md#adjuntos-de-salida-de-un-archivo).

Las capturas no acreditan entrega remota, entrada física, lector de pantalla
ni todas las geometrías. Wayland continúa sin parent exportado. El muestreo
de procesos y las observaciones de Hyprland son evidencia temporal auxiliar;
no trazas completas de red/syscalls ni vínculo criptográfico de ventana y
petición. REA hereda entorno del llamador no registrado
(`ambient_environment=unknown`); el hash previo y los metadatos sin cambios
no son una prueba atómica de la imagen ejecutada por el SO. Las cuatro
incógnitas del bundle siguen abiertas.

## Adjuntos: lotes nativos con Mock (07.2, 2026-10-15)

El [checkpoint de lotes](evidence/uploads-multiple-checkpoint-2026-10-15.json)
amplía el recorrido anterior a hasta ocho archivos. **1073 pruebas aprobadas,
seis ignoradas y 25 suites**, formato, Clippy estricto con todos los targets
y release de todo el workspace con todas las features pasaron. El release
compiló diez crates en 49,72 s; los ejemplos se comprobaron por separado.
El ejecutable retenido de escritorio tiene **56 843 608 bytes**, SHA-256
`c11d7e1da19243d013420a3896211f5fe2493aa92003976a0c50fcf16ce03ddc`.
No se modificó el pin de limn para este checkpoint.

REA reprodujo seis suites completas de los binarios retenidos: Core 161
aprobadas y una ignorada, State 109, UI 307, Provider 54, Platform 45 y
Desktop 33: **709 aprobadas y una ignorada**, todas con salida 0 y sin
truncación. El ledger identifica sus ocho artefactos y las regresiones
RED/GREEN de admisión atómica, presupuesto incluyendo texto UTF-8, liberación
de lotes, cambio de cuenta, ACK con IDs distintos y foco del composer.

La [selección nativa](evidence/uploads-multiple-native-picker-2026-10-15.json),
`ev_c2cef7541ac859c907ede5871df39a905fc4fc9ba21e5745b69d14618561bf48`,
devolvió dos descriptores de **48 B** del directorio sintético propio. Su
dispositivo e inodos coincidieron exactamente con el manifiesto previo.
Después se revisaron dos capturas del mismo release, de 1396×1660:

- [Preparados](img/upload-multiple-prepared-2026-10-15.png): `segundo.txt`
  y `primero.txt`, ambos de 48 B, junto a «Borrador con dos archivos».
  Elegirlos no envió el mensaje. [Registro REA](evidence/uploads-multiple-native-prepared-2026-10-15.json),
  `ev_30627abdc9dc5cef31daaecdb92b2a2a80246c49a74d4341902e4b1d33762cbc`.
- [Confirmados por Mock](img/upload-multiple-sent-2026-10-15.png): mensaje
  con el texto y ambos adjuntos en el mismo orden, editor vacío y sin
  pendiente. [Registro REA](evidence/uploads-multiple-native-sent-2026-10-15.json),
  `ev_2357e0ba5117c41c756925470a592c36053c9c2c3ea3f58eb6a37c4678ee065f`.

El observador exigió el diálogo GTK nuevo y su identidad/foco exactos antes
de cada fase. Se verificó la entrada y después la barra de navegación del
directorio propio antes de seleccionar. El envío exigió además `/proc/PID/exe`
del release retenido y la misma ventana activa «RUNE — general»; Enter se
introdujo 0,2504 s después del cierre observado del portal. Los recortes
auxiliares de filas se limitaron al directorio sintético ya comprobado.
No se leyó el portapapeles ni se capturó el listado privado del hogar.

Se conservan el [intento 01 cancelado por plazo](evidence/uploads-multiple-native-picker-attempt-01-2026-10-15.json)
y el [intento 03 de preparación fallida](evidence/uploads-multiple-native-prepared-attempt-03-2026-10-15.json).
En 03 la navegación no quedó acreditada y la captura mostró un error, sin
adjuntos preparados; su salida 0 no acredita éxito funcional. La revisión
de GTK mostró que Ctrl+L alterna la entrada y Escape puede cerrar el diálogo;
el driver posterior separó escritura, verificación, navegación y selección,
sin Escape automático. No se atribuye una causa exacta a 03 ni un fallo de
producto que no se reprodujo con ese control por fases.

El bundle retenido en
`/tmp/rune-multiple-native-20261015-5qzkrplj/uploads-multiple-evidence.json`
tiene **1 384 477 bytes, 55 registros y cuatro incógnitas**, SHA-256
`f6543edea108141694201c604e69fb111e43478ec6137f0eae12ffebe4deb937`.
Core conserva ocho entradas y 32 MiB; Mock declara ocho archivos, 20 MiB por
archivo y 25 MiB por mensaje incluido el texto. Son políticas de simulación.
Los proveedores reales siguen sin adjuntos; arrastrar, pegar y transporte
remoto permanecen pendientes, al igual que los puntos completos 07/08.

El [PR limn #49](https://github.com/Niiihuel/limn/pull/49) prepara la siguiente
porción: opt-in del editor de `Composer` y resolución de drops dentro de
superficies flotantes, menús y modales. El cherry-pick en `rune/integration`
pasó 1405 pruebas, dos ignoradas y Clippy estricto. El pin de RUNE de 07.2
sigue en `2618b37`; ese PR por sí solo no conecta el arrastre de archivos.
No acredita pegado, arrastre físico del SO ni Windows/macOS. Continúan los
límites de entorno heredado, identidad temporal de proceso/ventana y cobertura
de la sección anterior; no se realizaron operaciones de cuenta ni red autenticada.

## Adjuntos: arrastre local y bloqueo de backend (07.3.1, 2026-10-15)

El [checkpoint parcial](evidence/uploads-drop-checkpoint-2026-10-15.json)
registra **1098 pruebas aprobadas, seis ignoradas y 25 suites**; formato,
Clippy estricto y release del workspace con todas las features correctos.
REA reprodujo cinco binarios retenidos completos, sin truncación: Core 165
y una ignorada, UI 313, Components 100, Platform 54 y Desktop 38:
**670 aprobadas y una ignorada**. Se conservaron también tres intentos con
PTY completo pero frames renderizados truncados; no son los replays aceptados.
El archivo de 223 fuentes/manifiestos conserva el pin limn `1ad33eb`.

El código conecta rutas nativas con la preparación atómica existente, sin
portal nuevo, envío automático ni rutas/buffers en UI. Revalida superficies,
cierre, permisos y cuenta, y bloquea Submit del editor cuando llega un drop
válido o rechazado en ese mismo cuadro. El [PR limn #49](https://github.com/Niiihuel/limn/pull/49)
añade además el orden de entrada ordinaria antes de metadatos de archivos;
no establece la cronología del SO ni crea eventos nativos ausentes.

**La QA de arrastre no pasó.** Dos gestos desde un Dolphin aislado y dos
fixtures propios no produjeron adjuntos preparados. El primer helper usó
warps del compositor; el segundo emitió eventos de puntero nativos y conservó
la selección múltiple. Ninguno acredita importación. Se guardan los
[intentos 01](evidence/uploads-drop-native-prepared-attempt-01-2026-10-15.json)
y [02](evidence/uploads-drop-native-prepared-attempt-02-2026-10-15.json),
detenidos sólo en sus procesos propios con SIGTERM; la salida 0 con señal 15
no es éxito funcional. No hubo Enter/envío ni PNG canónico nuevo.

La revisión del código fijado de winit 0.30.13 encontró que su backend
Wayland, elegido por este entorno, no produce eventos de archivos DnD. X11
recibe coordenadas XdndPosition pero no las expone. La API nueva de posición
y lote está en [winit #4571](https://github.com/rust-windowing/winit/pull/4571);
la migración de eframe sigue en el [borrador #7731](https://github.com/emilk/egui/pull/7731).
No se sustituye la posición del drop por el canal/foco actual ni por una
consulta global del cursor. Sigue pendiente resolver esa frontera nativa,
además del portapapeles y del transporte remoto.
El [manifiesto de fuentes del bloqueo](evidence/native-drop-backend-blocker-2026-10-15.json)
conserva hashes, búsqueda completa del subárbol Wayland y ubicaciones;
la elección del backend se deriva del entorno y del constructor fijado.

El bundle `/tmp/rune-drop-native-20261015-lui79mxz/uploads-drop-evidence.json`
tiene **3 221 741 bytes, 65 registros y cuatro incógnitas**, SHA-256
`79575781d261857eb08a53983505a0634964d4aaf1f028fd3e09e6e4fddf23ab`.
La auditoría de artefactos/fuentes es válida con `native_complete=false`;
su modo `--require-native` falla expresamente. Los checkpoints 07.1/07.2 y
las 17 capturas canónicas anteriores se conservan intactos. Este checkpoint
no acredita arrastre funcional en Wayland, entrega real ni paridad con Discord.
El [informe de auditoría](evidence/uploads-drop-audit-2026-10-15.json)
enlaza el verificador retenido y sus salidas completas, incluidos ambos resultados.

## Guardar como: selector nativo, sin descarga todavía (08.1, 2026-10-15)

El [checkpoint 08.1](evidence/save-file-picker-checkpoint-2026-10-15.json)
implementa `SaveRequest`, `SaveDestination` privado y
`FileChooser::save_file`. OpenFile y SaveFile comparten protocolo, owner,
handle, respuesta temprana, cancelación, plazos, worker y limpieza. El
servicio devuelve exactamente una ruta local validada, sin abrirla ni
crear/truncar el destino. No equivale a una autorización de sobrescritura.

El workspace de esta revisión pasó **1112 pruebas, seis ignoradas y 25
suites**, formato, Clippy estricto y release con todas las features. Platform
pasó **64 pruebas unitarias y un doctest de no-Clone**; REA reprodujo las
64 unitarias del binario retenido sin truncación:
`ev_39f27fc724eb7b372621ef976da46114e3630c7637909dda8b7e29cd8aedc749`.
El archivo de 226 fuentes/manifiestos mantiene limn `1ad33eb`.

La [selección real](evidence/save-file-native-choice-2026-10-15.json),
`ev_4d57b4f1945546fdb2f00bfab1f722168983c38362f3c2b436687ae2816f8d5e`,
salió con código/señal 0 y sin truncación. Antes de aceptar se revisaron
el [nombre guardado.txt](img/save-file-name-2026-10-15.png) y la
[carpeta propia de fixtures](img/save-file-folder-2026-10-15.png).
El CLI retenido comprobó el destino devuelto exacto. Al terminar, el archivo
nuevo seguía ausente y el fixture existente conservaba identidad/contenido.
Los recortes excluyen listas de archivos y otras ventanas.

La [cancelación](evidence/save-file-native-cancel-2026-10-15.json),
`ev_072015a94539db1fc6f0dafd6011f56a7fdc7ffddb826330b8f254fab2b74209`,
devolvió Cancelled a los ocho segundos; el observador separado llegó tarde
y no vio su ventana. No se acredita una prueba visual de esa cancelación.
El [auditor independiente](evidence/save-file-audit-2026-10-15.json)
verificó 37 archivos y 39 campos de hash sin errores, conservando los tres
checkpoints anteriores. Sus límites distinguen efectos propios del servicio,
estado antes/después de los fixtures y efectos externos de un portal, que
no se observan como trazas completas de filesystem.

El bundle `/tmp/rune-save-native-20261015-gw8qijjd/save-file-evidence.json`
conserva **3 274 750 bytes, 68 registros y cuatro incógnitas**. El ejemplo
retenido mide 5 188 392 bytes, SHA-256
`4df9e844f1c0321af8e22aed1f325aa2dfaa4758d7a49b252b44e300368317d8`.
Las dos imágenes GTK son evidencia del selector; las capturas canónicas de
conversaciones Mock continúan en 17. Este cierre no acredita descarga,
entrada de menú, publicación del archivo ni reemplazo: siguen 08.2/08.3 en
el [contrato de Guardar como](../tasks/save-as-contract.md), y 08 completo
permanece abierto.

La misma revisión corrigió la entrada de arrastre: el host ahora rechaza
todos los drops legacy sin posición ligada al gesto antes de abrir archivos.
Conserva Picker, borrador y barrera de Enter; la API Core de rutas queda
disponible para un caller futuro con intención fiable. Ocho regresiones
del host pasaron. Una prueba X11 real mostró el aviso, pero también que el
toast tapa el editor; su captura se conserva como intento de QA mientras se
pule la ubicación. No se presenta como arrastre funcional ni como prueba
visual de conservación del borrador.

## Rechazo legacy y borrador visible: 07.3.2 verificado (2026-10-15)

El [checkpoint nuevo](evidence/uploads-drop-rejected-checkpoint-2026-10-15.json)
cierra el rechazo del drop legacy y su presentación. Los avisos de una
conversación ocupan una región desplazable encima del composer, con altura
natural y reducción cuando falta espacio. El borrador multilínea, respuesta y
ocho adjuntos permanecen accesibles en las pruebas de 720 × 480, 980 × 800 y
1280 × 800. Login y ventanas superpuestas conservan el aviso pasivo. La lista
de adjuntos también limita su altura según la ventana.

La capacidad faltante se implementó en limn mediante
[PR #50](https://github.com/Niiihuel/limn/pull/50), todavía abierta:
`Element::shrink_height_to_fit()` es opt-in para hijos de Column. Su rama
pública pasó 1536 pruebas, cinco ignoradas y 21 suites; la integración que
consume RUNE pasó 1508, cinco ignoradas y 20 suites. Clippy, rustdoc y
`git diff --check` pasaron. No se acredita `cargo fmt` de limn: su CI no lo
exige y el baseline sin modificar no pasa el rustfmt instalado. Tres capturas
de galería revisadas muestran estado inicial oscuro, claro y esquinas
cuadradas; el acceso al último aviso mediante scroll se prueba por código.
RUNE fija `22e0fd93c14b3f3c89375135f83c1e9c41307330` en las tres entradas
limn de Cargo.lock. No se modifica el checkpoint histórico de PR #49.

RUNE pasó **1120 pruebas, seis ignoradas y 25 suites en 5,50 s**, formato,
Clippy estricto y release de seis crates en 49,56 s con todas las features.
Las siete regresiones de avisos y ocho del host pasaron. REA reprodujo los
binarios completos de [UI](evidence/uploads-drop-rejected-ui-tests-2026-10-15.json),
[componentes](evidence/uploads-drop-rejected-components-tests-2026-10-15.json)
y [host](evidence/uploads-drop-rejected-desktop-tests-2026-10-15.json):
**462 pruebas aprobadas, ninguna ignorada**, sin filtros ni truncación.

Un gesto nativo de 2,0001 s trasladó dos fixtures propios de 54 y 55 B desde
un Dolphin nuevo y aislado hacia el editor Mock de RUNE sobre X11. La
[captura revisada](img/uploads-drop-native-rejected-2026-10-15.png) muestra
el aviso completo y «Borrador conservado» debajo, sin adjuntos preparados.
La [ejecución REA](evidence/uploads-drop-native-rejected-2026-10-15.json),
`ev_0944204a1bcc916cadd143d185e6c7bf8d43787bb1ca5cb2f054bf1797ee62e9`,
terminó con código/señal 0 y sin truncación. No se pulsó Enter. La ausencia
de envío en el mismo cuadro se acredita con las regresiones del host; una
imagen del timeline no demuestra esa propiedad. Los dos fixtures conservaron
sus identidades y hashes después del cierre del proceso fuente propio.
La imagen mide 175 832 bytes, SHA-256
`45fefcba4c1e8533f8b992f0ff1288e005f7aba21b1a980a62946e062fc47bd1`;
eleva las capturas canónicas Mock de 17 a **18**. El intento anterior que
tapaba el editor permanece separado.

El [auditor estricto](evidence/uploads-drop-rejected-audit-2026-10-15.json)
verificó **48 archivos y 68 campos de hash**, sin errores ni pendientes. Sus
cinco regresiones usan entradas shadow y rechazan manifiestos JSON vacíos;
no generan evidencia nativa. El archivo retenido contiene 227 fuentes y
manifiestos. El bundle `/tmp/rune-drop-reject-native-20261015-monauf1z/evidence-bundle.json`
conserva **3 656 614 bytes, 76 registros y cuatro incógnitas**, SHA-256
`ce293716da4c89a742953b9431afbd802e484b82b6a823e3923b250760f22308`.
Los PID normalizados de REA no identifican ventanas del SO; la propiedad
de las dos ventanas se documenta aparte con PID, ejecutable y start ticks.

**El arrastre funcional sigue pendiente.** La
[auditoría estática adicional](evidence/limn-dnd-existing-backend-audit-2026-10-15.json)
encontró `limn-dnd` ya presente en los pins anteriores, pero ausente del
grafo e integración de RUNE. Ofrece una ruta Wayland candidata sin exigir
una migración de winit. Antes de usarla requiere límites, cancelación,
correlación de ofertas, recepción sólo Copy y cierre/lifetime comprobados;
no se acredita todavía una integración ni QA de ese backend. Los límites
del RawInput fijado de winit siguen siendo válidos, sin constituir la única
ruta de solución. Guardar como Core/UI 08.2, reemplazo 08.3, pegado y
transporte remoto siguen abiertos.

## Cliente web oficial: tipos de embed (2026-10-16)

Con la autorización ampliada del dueño (ver [SECURITY.md](SECURITY.md)) se
descargaron sin sesión los bundles públicos que lista `https://discord.com/app`
(355 archivos) y se inspeccionaron de forma estática, sin ejecutarlos.
`web.c5b338cc7e610833.js` (SHA-256 `6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec`) enumera los tipos de embed:
`image`, `video`, `link`, `article`, `tweet`, `rich`, `gifv`,
`application_news`, `auto_moderation_message`,
`auto_moderation_notification`, `text`, `post_preview`, `gift`,
`safety_policy_notice`, `safety_system_notification`,
`age_verification_system_notification`, `voice_channel`,
`gaming_profile`, `poll_result` y `components`. Los tres avisos de
seguridad están en el mismo conjunto que los avisos de moderación y las
encuestas; RUNE los muestra como tarjeta de aviso. Confirma los dos tipos
que se tomaron antes de documentación pública y agrega el de verificación
de edad. El renderizador vive en módulos diferidos que no se descargaron.

## Cliente web oficial: mensajes de voz (2026-10-16)

Misma autorización y método que la sección anterior: bundles públicos
descargados sin sesión, leídos con `grep` y un script propio de contexto
(Python con `-I`), sin ejecutarlos. El dueño decidió el 2026-10-16 mostrar y
**reproducir** mensajes de voz; grabarlos y enviarlos queda fuera.

| Archivo | SHA-256 |
| --- | --- |
| `web.c5b338cc7e610833.js` | `6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec` |
| `07abb3ad74268227.js` | `f2530e7aceb06546ed8f7cb42620bcb23e74d02b4e946b2b89559d954fa4b619` |
| `272b0b0cd90a5cad.js` | `1be248982aae9e989f1f8fe3da10b01888ec16a6fa04ccc8491cf99f5b7cf4e3` |
| `52bdd7060b270ca4.js` | `e9603f533894e14ae1c88c126bfffddff8e68ad44222cf9354117ff4a3d9dbe8` |
| `b1d5b19d5f99db6e.js` | `39168a945f788ad077fd0fd307f014b253cce3586aa353cf07314ebf833349d2` |
| `75c14a3c30599d3f.js` | `90d41947748108aeebf25419620b3ed0418e0cf3d0946df4a0b3109adad05d07` |

`52bdd7060b270ca4.js` formó parte de la lectura previa; en esta revisión no
se le atribuye ningún hallazgo propio.

Hallazgos verificados en esta revisión:

- **Flag del mensaje**: `IS_VOICE_MESSAGE = 8192` (bit 13), entre
  `SUPPRESS_NOTIFICATIONS = 4096` y `HAS_SNAPSHOT = 16384`
  (`web.c5b338cc7e610833.js`).
- **Campos del adjunto**: `duration_secs` y `waveform`; el reproductor recibe
  `src = url`, `durationSecs` y `waveform` (`07abb3ad74268227.js`). La subida
  escribe los mismos campos.
- **Formato**: `audio/ogg` con Opus. El reproductor comprueba
  `canPlayType("audio/ogg; codecs=opus")` y, si el motor no lo admite, usa
  un reproductor por software (`272b0b0cd90a5cad.js`).
- **Forma de onda**: base64; cada byte / 255 es una amplitud de 0 a 1. Un
  base64 inválido deja el reproductor sin forma de onda: se dibuja el
  marcador plano de cinco ceros y el mensaje se conserva.
- **Dibujo** (función `tf` de `web.c5b338cc7e610833.js`): el ancho depende de
  la duración, 40 px hasta 0,5 s, 294 px desde 45 s y proporcional entre
  ambos, redondeado al paso de 6 px (`6·⌊(w+4)/6⌋−4`). Barras =
  `⌊(ancho+4)/6⌋`, de 2 px con 4 px de separación. Con más muestras que
  barras promedia por tramos; con menos, completa con ceros. Altura
  `22·v+2` px centrada, extremos redondeados. Progreso: las barras antes de
  `round(tiempo/duración·barras)` van en el color activo.
- **Controles**: reproducir/pausar con etiqueta accesible; el tiempo muestra
  la duración completa redondeada hacia arriba antes de empezar y lo que
  falta, también hacia arriba, una vez empezado; `--:--` si se desconoce.
  Botón de velocidad que cicla 0,75/1/1,5/2. Las barras se arrastran para
  buscar. La posición se recuerda por clip.
- **Texto**: los extractos usan el contenido si no está vacío; si no, «voice
  message» (`b1d5b19d5f99db6e.js` y `web.c5b338cc7e610833.js`). Un mensaje de
  voz no se puede editar. Existe el permiso aparte `SEND_VOICE_MESSAGES`.
  El menú contextual ofrece guardar el audio (`save-voice-message-audio`,
  `75c14a3c30599d3f.js`).

Qué tomó RUNE (reimplementado, sin copiar código ni recursos): el adaptador
reconoce el mensaje de voz por `duration_secs` en el adjunto, con límites
(0 < duración ≤ 3600 s; forma de onda ≤ 256 bytes, base64 de ≤ 344
caracteres). Fuera de límites queda como archivo común; una forma de onda
inválida sólo pierde el dibujo. No exige el flag 8192: un `MESSAGE_UPDATE`
que trae adjuntos sin `flags` no debe convertirlo en archivo. El
reproductor sigue las medidas del dibujo y el texto del tiempo; el arrastre
sobre las barras busca al soltar. La velocidad (0,75/1/1,5/2, sin cambiar
el tono, con un WSOLA propio en lugar de `preservesPitch`) y la posición
recordada por clip se sumaron después el mismo día ([MEDIA.md](MEDIA.md));
**no se implementó** el reproductor de contenedores Opus con más de dos
canales. Serein se consultó como segunda referencia (acota la forma de onda
a 256 muestras y reproduce con GStreamer); no se copió su código.

## Cliente web oficial: aceptar invitaciones y hCaptcha (2026-10-16)

Misma descarga sin sesión e inspección estática (grep y lectura, sin ejecutar)
que la sección anterior. Archivos y SHA-256:

- `web.c5b338cc7e610833.js` — `6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec`
- `ee08a8c7800d06ae.js` (chunk `536037`, `showCaptchaAsync`) — `ec30810718aa17e2e76a24899af3f0b84938c40b29c84c495d5b8391eac61898`
- `f58d4b99351b2c1c.js` (modal y widget de desafío) — `34246db2c2dd56035fb78ce9cb751213db074825c5b14eda8120a22e59fafeaf`
- `4aab65ddfdfe554a.js` (cargador de hCaptcha) — `3fde4d3f5d331ebc018f0e7900750250e31dcaeb999ffd2484a8aa577738e89e`

Hallazgos (desplazamientos en bytes dentro del archivo):

- **Ruta y cuerpo** (`web…`, ~635 576, `acceptInvite`): despacha
  `INVITE_ACCEPT` y hace `POST` a `Endpoints.INVITE(code)` = `/invites/{code}`
  (~276 288) con cuerpo `{session_id, invite_instance_id,
  received_installation_id}`. `session_id` es el de la sesión Gateway
  (`getSessionId`, asignado en `CONNECTION_OPEN`). Envía `context:` con
  `invite_guild_scheduled_event_id`; el transporte lo serializa como
  `X-Context-Properties` = base64(JSON) (~10 693 826). Antes del POST, una
  cuenta con la marca `QUARANTINED` no intenta unirse.
- **Desafío** (`web…`, ~1 004 707, `interceptResponse`): sólo
  `statusCode === 400 && body.captcha_key` abre el desafío.
  `extractCaptchaPropsFromResponse` (~3 240 065) lee `captcha_service`,
  `captcha_sitekey`, `captcha_session_id`, `captcha_rqdata`,
  `captcha_rqtoken`, `should_serve_invisible` y `user_flow`. Los servicios
  posibles son `hcaptcha`, `recaptcha`, `recaptcha_enterprise` y
  `turnstile` (~10 697 585); sin `captcha_service` el widget asume reCAPTCHA.
- **Reintento** (mismo punto): repite la misma petición con
  `X-Captcha-Key` = respuesta del widget y, si existen,
  `X-Captcha-Rqtoken` = `captcha_rqtoken` de la respuesta 400 y
  `X-Captcha-Session-Id` = `captcha_session_id` (`ee08…` muestra que el
  modal devuelve `{captcha_key, captcha_rqtoken, captcha_session_id}`).
- **Widget** (`f58d…`, ~3 848; `4aab…`, ~645): hCaptcha explícito desde
  `https://js.hcaptcha.com/1/api.js?render=explicit&onload=…`, `setData({rqdata})`
  y `execute()` en la variante invisible; callbacks de verificación,
  expiración del desafío y error. La misma interceptación trata 401 con
  `MFA_REQUIRED` (60003) como MFA, 403 en `GUILD_LIMITED_ACCESS_*`
  (400000–409999) y `RESTRICTED_HOURS_ACTIVE` (40117) con sus propios modales.
- **Confirmación**: el éxito despacha `INVITE_ACCEPT_SUCCESS` con el cuerpo
  (`guild`, `new_member`, `show_verification_form`, canal) y puede abrir el
  onboarding; la membresía en sí llega por Gateway. RUNE mantiene su contrato:
  el ACK no crea servidores; espera el `GUILD_CREATE`.
- **Códigos** (enumeración en `web…`, ~319 708 en adelante):
  `UNKNOWN_INVITE` 10006, `TOO_MANY_USER_GUILDS` 30001, `GUILD_AT_CAPACITY`
  30019, `USER_BANNED` 40007, `USER_QUARANTINED` 40068, `INVITES_DISABLED`
  40069, `RESTRICTED_HOURS_ACTIVE` 40117, `INVALID_COUNTRY_CODE` 50095,
  `MFA_REQUIRED` 60003, `USER_GUILD_JOIN_LARGE_GUILD_UNDERAGE_DISALLOWED`
  700000 y `USER_GUILD_JOIN_AGE_RESTRICTED_IOS_DISALLOWED` 700002. En el
  fallo, el cliente sólo guarda `code` y `message` del cuerpo.

Uso en RUNE: el adaptador ofrece verificación sólo para la forma confirmada
(400, `captcha_key`, `captcha_service: "hcaptcha"`, valores acotados) y
reintenta una vez con las tres cabeceras `X-Captcha-*`. No reproduce
`session_id`, `invite_instance_id`, `received_installation_id` ni
`X-Context-Properties`: no se inventan, y el `session_id` de Gateway queda
dentro del transporte. Desde el 2026-10-19 sí manda `X-Context-Properties`
([identidad de navegador](#cliente-web-oficial-identidad-de-navegador-2026-10-19)). Los códigos de esta lista que RUNE no distingue
siguen en las categorías estáticas existentes (inválida, denegada,
verificación). Nada de esto se probó contra el servicio real.

## Cliente web oficial: resultados de encuestas, tarjeta de voz y componentes (2026-10-16)

Misma descarga pública, sin sesión, leída de forma estática (grep y lectura
de fragmentos; nada se ejecutó). Archivos y SHA-256:

| Archivo | SHA-256 | Qué muestra |
| --- | --- | --- |
| `web.c5b338cc7e610833.js` | `6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec` | flags de mensaje, tipos de mensaje y de componente, conjuntos de tipos de embed, conversión de componentes |
| `8539ed9c57c49568.js` | `1ceb64d590c99586eee539ad81f8acf9a59b670c234f3299d2b39c7f632caa21` | renderizadores de mensajes de sistema, incluido `POLL_RESULT` |
| `52bdd7060b270ca4.js` | `e9603f533894e14ae1c88c126bfffddff8e68ad44222cf9354117ff4a3d9dbe8` | `renderEmbeds` del mensaje |
| `0099045452db6acd.js` | `ea49af9bd770a03ed548ff9b326c5f25b86e72e3c3b5f90bcd31bd9a71ef51e6` | textos en inglés |
| `193625af2e73e3bd.js` | `55e8f338e85d2f9a42621239108b082384c4058485b27bce011cd2b976fea885` | textos en español |
| `aa717b66c341713a.js` | `5bf7534b4e3b7219de433e82bc85b803ad32c2a0e079eb1c6b45f007149809a8` | ajustes del canal de sistema |

**Resultado de encuesta (tipo de mensaje 46).** El renderizador lee el
primer embed y recorre sus `fields` por nombre: `poll_question_text`,
`victor_answer_id`, `victor_answer_text`, `victor_answer_votes` y
`total_votes` (estos dos con `parseInt`), y el emoji del ganador en
`victor_answer_emoji_id`, `victor_answer_emoji_name` y
`victor_answer_emoji_animated` (`"true"`). Estados: `total_votes` 0 → «No
hubo un ganador» con un ícono propio; con `victor_answer_id` → la respuesta,
una marca y «Respuesta ganadora • N %»; sin ganador → «El resultado fue un
empate» y «N %». El porcentaje es `Math.round(votos_ganador / total * 100)`.
La línea dice «La encuesta de {usuario}, {título}, ha terminado.» con el
título enlazado, y la tarjeta lleva «Ver encuesta» (botón secundario); ambos
llaman `jumpToMessage` con `message_reference.channel_id/message_id`,
`flash` y el mensaje de regreso. Textos de `193625af2e73e3bd.js`.

**Embeds que no se dibujan como tarjeta genérica.** `web.c5b338cc7e610833.js`
define dos conjuntos: los tipos que conservan sus `fields` crudos
(`auto_moderation_*`, `rich`, los tres avisos, `voice_channel`,
`poll_result`) y los que `renderEmbeds` salta (avisos de moderación y
seguridad, `application_news`, `post_preview`, `voice_channel`,
`poll_result`). Por eso un resultado de encuesta no repite su embed como
tarjeta. Cualquier embed con `components` los convierte con la misma función
que los del mensaje.

**`voice_channel`.** Sólo aparece en esos dos conjuntos y en ubicaciones de
analítica. Su renderizador no está entre los 5879 archivos descargados, así
que **sus campos no están confirmados**. El tipo de mensaje 45
(`VOICE_HANGOUT_INVITE`) oculta el contenido del mensaje y no tiene
renderizador de sistema; es el candidato para llevarlo, sin confirmar.
El 65 (`VOICE_SESSION`, «inició una sesión de voz») usa
`message_reference.channel_id`, no un embed. RUNE dibuja la tarjeta con las
partes estándar del embed y obtiene el canal sólo de un enlace
`https://discord.com/channels/{servidor}/{canal}` en `url` (suposición
marcada en el código); sin canal, «Unirse» queda deshabilitado con motivo.

**Componentes.** Flag `IS_COMPONENTS_V2 = 1 << 15` (32768); con ella el
mensaje es sólo componentes y, al editar, el texto sale de los
`TEXT_DISPLAY`. Tipos: 1 fila, 2 botón, 3 menú de texto, 4 campo de texto,
5–8 menús de usuario/rol/mencionable/canal, 9 sección, 10 texto, 11
miniatura, 12 galería, 13 archivo, 14 separador, 16 entrada de inventario,
17 contenedor, 18 etiqueta, 19 subida de archivo, 20 tarjeta de checkpoint,
21–23 grupos de opción/casillas. Estilos de botón 1 primario, 2 secundario,
3 éxito, 4 destructivo, 5 enlace, 6 premium; separador con `divider`
(por defecto `true`) y `spacing` 1 pequeño (por defecto) o 2 grande.
Campos que lee: botón `custom_id/style/disabled/url/label/emoji/sku_id`;
menús `placeholder/min_values/max_values/options`; sección `components` +
`accessory` (sin texto o sin accesorio se descarta); texto `content`;
miniatura `media/description/spoiler`; galería `items[].media/description/
spoiler`; archivo `file/name/size/spoiler`; contenedor
`accent_color/spoiler/components`. Un tipo desconocido se descarta con un
aviso en consola; RUNE en cambio deja una línea visible.

Serein (`references/serein-current`, sin copiar código) acota 40 componentes,
profundidad 8 y 128 KiB, y oculta los adjuntos de un mensaje v2; RUNE toma
la idea de los límites y de no duplicar archivos.

### Segunda pasada: `voice_channel`, menús con valores y emoji de componentes (2026-10-16)

Misma regla: archivos públicos, sin sesión, leídos con grep y lectura de
fragmentos; nada se ejecutó. Archivos nuevos que muestran algo:

| Archivo | SHA-256 | Qué muestra |
| --- | --- | --- |
| `bc3966ad8bdd94fd.js` | `8e48f000ff5249c303af87e1b3e7a6df1206ceddd4785d55ff7cf0419c09d4a5` | renderizadores de botón y de menús de componentes |
| `d3907dbcdbac69c5.js` | `7856ff166398e71d6bd88e8498bec5fa1fa42d356fe2a89e6cc577405e1c8c72` | contenido vacío para el tipo 45 |
| `2ce363d68455acf9.js` | `32e8358efb9f1a7bf3c3b20c1a45acc677a4a40a61c720772a91998fdf714afb` | dónde se aplica ese contenido |
| `d02c92bb46da00f3.js` | `3e938488f74774d7833db255134003ea749f9421703d92e113b4c6e93e7ad884` | participantes de `message.call` (tipo 65) |
| `0fabf67c2169a992.js` | `1872b032a2b7836a08e8accf76be5046270c698c4f5f1df9997c04119a651b1e` | consultado en la búsqueda del tipo 65 |

**`voice_channel`: no hay renderizador.** El enum de tipos de embed es el
módulo 508602 de `web.c5b338cc7e610833.js`, reexportado por las constantes.
Su `VOICE_CHANNEL` aparece sólo en los dos conjuntos ya descritos: el que
conserva los `fields` crudos (`{rawName, rawValue, inline}`) y el que
`renderEmbeds` salta entero (`52bdd7060b270ca4.js`). El literal
`"voice_channel"` sale en tres archivos: el enum, un enum de pestañas que no
tiene que ver y una ubicación de analítica. Nadie lee esos campos por nombre:
el cliente no dibuja esta tarjeta. El tipo de mensaje 45
(`VOICE_HANGOUT_INVITE`) vacía el contenido del mensaje y no tiene
renderizador de sistema; el 65 (`VOICE_SESSION`) se dibuja con
`message.call` (participantes y duración) y `message_reference.channel_id`,
sin embed. Las tarjetas «Unirse» de `renderEmbeds` son invitaciones
construidas desde códigos, no este tipo. Buscado: `VOICE_CHANNEL` (54
archivos), `"voice_channel"`, los alias del enum, `VOICE_HANGOUT_INVITE`,
`VOICE_SESSION`, `hangout`, las claves de «Join Voice» y quién llama al
reemplazo de contenido. Se revisaron las 15 funciones `.u=` de nombres de
chunk en 13 archivos (el runtime principal, 11 de workers y 2 de
`libdiscore-wasm-fetch`) y `miniCssF`: nombran 5803 chunks JS, de los que
faltaban 279; se bajaron los 279 sin cookies ni cabeceras de sesión (todos
HTTP 200, 1,3 MB; casi todos stubs de CSS y datos) y no aportan nada. Serein
tampoco lee este embed (sólo nombra el tipo 65). **Sin confirmar**: RUNE
mantiene su tarjeta y el canal sólo desde un enlace
`https://discord.com/channels/{servidor}/{canal}`.

**Menú de texto con valores por defecto.** El parser conserva `default` de
cada opción y su emoji (`id`, `name`, `animated`); el renderizador del menú
de texto arranca con las opciones `default: true` elegidas y, cerrado,
muestra esas opciones en lugar del placeholder: una sola como emoji y
etiqueta, varias como fichas. El placeholder sólo aparece sin nada elegido.
Los menús 5–8 convierten `default_values` buscando usuarios, roles y canales
en sus stores y descartan lo que no encuentran.

**Emoji de botones y opciones.** Ambos van por el mismo componente de emoji
con `emojiId`, `emojiName` y `animated`, que pide
`getEmojiURL({id, animated, size: 44})`: `/emojis/{id}.webp?size=…` (con
`&animated=true` sólo si está enfocado y la reproducción automática o el
puntero lo permiten; sin WebP, `.gif` o `.png`). Sin id, es el emoji Unicode.

Uso en RUNE (no probado contra el servicio real): el menú de texto guarda
sus opciones por defecto (`Choice`, a lo sumo `max_values`, 25 como mucho) y
las muestra como el cliente; los menús 5–8 no resuelven sus
`default_values` y siguen con el placeholder. El emoji propio de una opción
y el del ganador de una encuesta (`victor_answer_emoji_id`, guardado como
miniatura de la tarjeta) usan la misma imagen fija que el selector de emoji
(`/emojis/{id}.webp?size=128`, el primer cuadro aunque sea animado), y
`:nombre:` mientras carga o si falla. Los botones hacen lo mismo con su emoji
propio, antes de la etiqueta (`limn::Button::image`, PR #71).

## Video: Go Live, cámaras y transporte de video (2026-10-16)

Misma autorización (REA estática del cliente oficial, 2026-10-16) y mismo
método: bundles públicos descargados sin sesión, leídos con `grep` y un
script propio de contexto (Python con `-I`), nada ejecutado; del paquete
instalado sólo `strings -t x` sobre `discord_voice.node`, sin cargarlo. No se
abrió ningún perfil, sesión ni tráfico del cliente oficial. Serein
(`references/serein-current`, `docs/voice.md` y
`docs/discord-compatibility.md`) se leyó como evidencia de lo que ya probó
contra Discord; no se copió código. El diseño que sale de esto está en
[VIDEO.md](VIDEO.md).

| Archivo | SHA-256 |
| --- | --- |
| `web.c5b338cc7e610833.js` | `6f842876a0ddbfe90cf9b54e006aa1995912f23c1b17f5356151ac694e2a0aec` |
| `fast-connect.f0cfe6eb7552ae67.js` | `a05f649b4568ee0241848c1e80f0c998aaddbeef26b5693d1822c676505e7f7c` |
| `23a7428fe1da1be1.js` (estado de voz en escenarios) | `0847d16c1f144f5fdf13d0ee6ae68a89bf1ac0038196c96d6f286e0525e4608d` |
| `discord_voice.node` (Discord 1.0.155, Nix) | `0a3d1be6f37bbc6f63cd052590fa95ab2c17c67978d1ebcf2549222dec586343` |
| `davey-0.1.5.crate` (registro de cargo) | `27466c95d1885cc506b29cec686a91ba7c472376cca09850d026cceb9c29a888` |

Los desplazamientos son bytes dentro de `web.c5b338cc7e610833.js` salvo que
se nombre otro archivo; los del nativo, en hexadecimal.

### Confirmado: Gateway principal

- **Opcodes** (enum en ~4 996 747; el mismo en `fast-connect…` ~15 288):
  `STREAM_CREATE = 18`, `STREAM_DELETE = 19`, `STREAM_WATCH = 20`,
  `STREAM_PING = 21`, `STREAM_SET_PAUSED = 22`.
- **Envíos** (~4 998 992): 18 `{type, guild_id, channel_id,
  preferred_region}` con `type` `"guild"` o `"call"`; 20 `{stream_key}`;
  21 `{stream_key}`; 19 `{stream_key}`; 22 `{stream_key, paused}`.
- **Clave del stream** (módulo 652896, ~5 089 698; tipos en ~5 084 910):
  `guild:{servidor}:{canal}:{dueño}` o `call:{canal}:{dueño}`, separada por
  `:`; otro prefijo es error.
- **Despachos** (~4 937 840): `STREAM_CREATE {stream_key, region,
  viewer_ids, rtc_server_id, rtc_channel_id, paused}`;
  `STREAM_SERVER_UPDATE {stream_key, endpoint, token}`;
  `STREAM_UPDATE {stream_key, region, viewer_ids, paused}`;
  `STREAM_DELETE {stream_key, unavailable, reason}`. `STREAM_SERVER_UPDATE`
  comparte el trato de `VOICE_SERVER_UPDATE` (conjunto en ~4 987 806).
- **Motivos de `STREAM_DELETE`** (~528 630): `user_requested`,
  `stream_full`, `unauthorized`, `safety_guild_rate_limited`. Con
  `unavailable: true` el stream pasa a «reconectando», no a terminado
  (~8 972 115). Estados del stream: CONNECTING, ACTIVE, RECONNECTING,
  ENDED, PAUSED, FAILED.
- **Mirar** (~4 955 049, ~711 030): antes de op 20, salvo «varios a la vez»,
  el cliente manda op 19 por cada stream que mira (no el propio). «Varios a
  la vez» se activa solo cuando hay dos o más streams de otros en el canal.
  Si la conexión del stream se cae y va a reintentar, manda op 21 con la
  clave (~4 952 580).
- **Conexión del stream** (~9 368 580–9 369 819): `STREAM_CREATE` (para
  quien transmite y para quien mira) crea una conexión de voz aparte, con el
  `session_id` del Gateway principal (el de `CONNECTION_OPEN`, ~9 367 093),
  `server_id = rtc_server_id` y `channel_id = rtc_channel_id` (si falta,
  `rtc_server_id − 1`, ~2 479 108). `STREAM_SERVER_UPDATE` le da endpoint y
  token y conecta.
- **Op 4** (~4 997 843): `guild_id, channel_id, self_mute, self_deaf,
  self_video, flags` y, si corresponde, `preferred_region(s)`. **No lleva
  `self_stream`**: eso lo pone el servidor al crear el stream. `tracks`
  (`type, rid, quality`) sólo se manda con video en un canal Stage
  (~4 998 195, ~4 906 969).
- **Permiso**: compartir exige `STREAM` en el canal y que no sea el canal AFK
  (~5 090 634). Que la cámara use el mismo permiso lo dice Serein; no se
  buscó aquí.
- **Vista previa** (rutas ~292 006): `GET /streams/{clave}/preview?version=<ms>`
  devuelve `{url}`; un 429 trae `retry_after` en segundos (~712 079). Quien
  transmite sube `POST /streams/{clave}/preview` con `{thumbnail}` = JPEG en
  data URL, escalado para caber en 512×288 (~2 256 790), tras descartar hasta
  60 cuadros negros; repite cada 60 s o 300 s según un estado interno que no
  se identificó. Existe además `/streams/{clave}/notify`.

### Confirmado: Gateway de voz

Clase del socket de control, ~2 391 300–2 407 500:

- **Versión**: `?v=9`, o `?v=10` con un experimento y soporte de
  «actualización de SFU» (~2 395 376). RUNE y Serein usan v8.
- **Op 0** (~2 403 816): `server_id, channel_id, user_id, session_id, token,
  max_dave_protocol_version, video, streams`. `video` dice si el cliente
  maneja video. `streams` (~9 194 248) es `[{type, rid: "100", quality:
  100}]` con `type` `"video"` en la conexión normal y `"screen"` en la de
  stream; con simulcast se suma `{rid: "50", type: "video", quality: 50}`
  (constantes 100 y 50 en el módulo de ~10 763 689).
- **Forma de un stream** (`ek`/`eF`, ~2 391 860): `type` (`audio`, `video`,
  `screen`, `test`), `rid`, `ssrc`, `rtx_ssrc`, `active`, `quality`
  (0–100), `max_bitrate`, `max_framerate`, `max_resolution {type:
  "fixed"|"source", width, height}`.
- **Op 2 Ready**: además de `ip, port, modes, ssrc` trae `streams` (con los
  SSRC que asigna el servidor) y `experiments`.
- **Op 1 Select Protocol** (~2 405 299): `{protocol, data: {address, port,
  mode}, address, port, mode, codecs, rtc_connection_id, experiments}`.
  Cada códec es `{name, type, priority: 1000·prioridad, payload_type,
  rtx_payload_type, encode, decode}` (~2 391 860).
- **Lista de códecs del cliente de escritorio** (~10 785 894, ~10 802 611):
  Opus audio, prioridad 1, `payload_type` 120; los de video salen de
  `[AV1 si un experimento lo señala, H265] + [H264, VP8]`, filtrados por lo
  que el motor nativo puede, en ese orden. El de índice *t* lleva
  `payload_type = 101 + 2t` y `rtx_payload_type = 102 + 2t`, prioridad
  *t + 1*. Con H264 primero (como RUNE y Serein), 101 y 102.
- **Op 4 Session Description** (~2 396 222): además de `mode, secret_key,
  dave_protocol_version` trae `audio_codec`, `video_codec`,
  `media_session_id`, `keyframe_interval` y, a veces,
  `bandwidth_estimation_experiment` o `sdp`.
- **Op 5 Speaking**: `{speaking, delay, ssrc}` con banderas NONE 0, VOICE 1,
  SOUNDSHARE 2, PRIORITY 4, HIDDEN 8 (~10 768 622). El audio de un stream
  usa SOUNDSHARE.
- **Op 12 Video** (envío ~2 405 929): `{audio_ssrc, video_ssrc, rtx_ssrc,
  streams}`; el cliente sólo lo manda desde que tiene un SSRC de video ≠ 0.
  Recibido (~2 396 922): `{user_id, audio_ssrc, video_ssrc, streams}`.
  `rtx_ssrc = video_ssrc + 1` cuando no viene dado (~10 800 413).
- **Op 14**: el cliente manda `{codecs}` si cambian; el servidor puede
  mandar `audio_codec`, `video_codec`, `media_session_id`,
  `keyframe_interval`.
- **Op 15 Media Sink Wants** (envío ~2 406 011, sólo con servidor v ≥ 5;
  recibido ~2 397 366): un mapa `{any: 0–100, "<ssrc>": 0–100}`; el de
  escritorio arranca en `{any: 100}` (~2 407 471). El servidor manda al
  emisor lo que quieren los demás, con `pixelCounts` por SSRC (~10 759 601).
- Otros opcodes recibidos: 16 versiones (el cliente manda `{}` si v ≥ 6),
  18 `flags`, 20 `platform`, 33 actualización de SFU; enviado: 32 «sin
  ruta». Cierres propios del cliente 4800–4804 (~2 391 314).

### Confirmado: códecs, RTP y RTCP

- **Parámetros H264** (~10 825 104): `packetization-mode=1`,
  `level-asymmetry-allowed=1`; para decodificar anuncia `42e034`
  (Constrained Baseline 5.2) y el codificador de escritorio usa `4d0033`
  (**Main** 5.1; Android `42e01f`).
- **Realimentación** (SDP de la variante web, ~10 896 150): `ccm fir`,
  `nack`, `nack pli`, `goog-remb`, `transport-cc`; RTX con `apt=`. El
  nativo tiene «Received REMB/PLI/NACK for ssrc» y «Send RTCP PLI/NACK with
  sender ssrc» (`discord_voice.node`, 0x17516c1–0x17516fd, 0x1755896).
- **Empaquetado**: el nativo es libwebrtc
  (`rtp_format_h264.cc` 0x18289d4, `video_rtp_depacketizer_h264.cc`
  0x17f7c70, más VP8, VP9, AV1 y H265), así que H264 va como RFC 6184
  (unidad simple, STAP-A, FU-A). Extensiones RTP presentes:
  `abs-send-time`, `transport-wide-cc-02`, `playout-delay`,
  `video-content-type`, `video-timing`, `abs-capture-time`,
  `generic-frame-descriptor-00` y la propia
  `https://discord.com/#rtp-hdrext/2018-07-29/speaker` (0x175569a). Está
  `sps_vui_rewriter.cc` (0x18232cd), coherente con lo que encontró Serein:
  el receptor reescribe el SPS.
- **Cifrado de transporte**: el nativo ofrece `aead_aes256_gcm_rtpsize` y
  `aead_xchacha20_poly1305_rtpsize` (0x1762551).
- **DAVE para video**: el nativo trae `discord_common/native/dave/codec_utils.cpp`
  (0x17607ff), `AddUnencryptedBytes` y `DeserializeUnencryptedRanges`:
  rangos sin cifrar por códec. `davey 0.1.5` hace lo mismo:
  `DaveSession::encrypt(MediaType::VIDEO, Codec::H264, frame)` y
  `decrypt(user, MediaType::VIDEO, frame)`, con `Codec` OPUS, VP8, VP9,
  H264, H265 y AV1 (`cryptor/mod.rs`, `frame_processors.rs`). Para H264
  recibe Annex B: por cada NAL deja en claro un código de inicio de 4 bytes,
  la cabecera y lo necesario hasta el `pps_id` en cortes e IDR, y deja en
  claro entero todo NAL que no sea corte (SPS, PPS, SEI); el resto va
  cifrado (`codec_utils.rs`). El cliente le pasa a DAVE el códec acordado
  (`updateLocalUserCodecs`).

### Confirmado: calidades del cliente

- **Tabla de Go Live** (módulo 753070, ~5 086 174): resoluciones 480, 720,
  1080, 1440 y «fuente» (0); 5, 15, 30 y 60 fps. Sin mejora: 720p y 480p a
  5/15/30. 720p60 y 480p60: nivel 1 de mejoras del servidor o calidad
  «media». 1080p y 1440p a 15/30/60: nivel 2 o calidad «media». Fuente:
  calidad «alta». La calidad media o alta sale de la suscripción
  (~5 092 092). Un experimento de servidor (`2026-05-frontier-tuning`,
  ~5 092 978) puede limitar a las cuentas sin mejora a 1080p30 y
  3,5–5 Mbit/s.
- **Tasas** (~10 765 479, ~10 821 042): Go Live por defecto 1280×720 a
  30 fps, 0,5–3,5 Mbit/s con objetivo 0,6; el máximo sube a 9 Mbit/s si
  la altura pasa de 720, si pasa de 30 fps o si es «fuente» con ≥ 10 fps.
  Cámara: captura 1280×720 a 30 fps, 0,15–2,5 Mbit/s; la capa simulcast de
  calidad 50 usa un cuarto del máximo. Piso 150 kbit/s.

### Uso en RUNE

Decisión del dueño (2026-10-16): **sin tope por Nitro**. RUNE ofrece hasta
1080p60 como Serein, sin «fuente» ni 1440p; que Discord acepte o recorte
esas calidades no está confirmado. Mirar streams y cámaras, compartir la
pantalla propia y enviar la cámara (fases 1 a 4) están implementados y
probados sólo con fixtures; de la fase 5 (2026-10-17), lo que Serein no
usa sale de esta REA: op 22 para pausar y el NACK al recibir. Las fases y
sus ajustes están en [VIDEO.md](VIDEO.md).

### Sin confirmar

- Que el servidor acepte H264 como único códec de video ofrecido, y cómo
  elige `video_codec` cuando los clientes ofrecen distintos (Serein lo hace
  y dice que funciona).
- Que la versión 8 del gateway de voz (la de RUNE) reciba video igual que
  la 9; Serein usa 8.
- La forma de RTCP en modo `rtpsize` (qué parte queda en claro y
  autenticada). No está en los bundles; sale de Serein y de bibliotecas
  públicas.
- Los ID de las extensiones RTP: el nativo no los muestra como cadenas.
  Serein marca el audio del stream con la extensión de ID 9, valor 4; no se
  verificó. RUNE la manda igual desde el 2026-10-18 (`voz-protocolo`).
- Si el servidor hace cumplir la tabla de Go Live (rechaza o recorta 1080p60
  sin mejora) o si sólo la impone el cliente.
- Que un SPS sin reordenamiento evite el rechazo DAVE en el receptor
  oficial: es la conclusión de Serein (corrección del 21 de septiembre); el
  nativo trae `sps_vui_rewriter.cc`, pero no se leyó su código.
- Si el decodificador OpenH264 muestra bien el perfil Main (o High por
  hardware) que mandan los clientes de escritorio.
- Los intervalos del refresco de `media sink wants` (Serein: cada 5 s, y
  cada 1 s mientras no llega imagen) y de la subida de vistas previas.
- Llamadas privadas (`call:…`): desde el 2026-10-17 RUNE las usa (ver
  [Llamadas privadas](#cliente-web-oficial-llamadas-privadas-2026-10-17));
  sin probar contra Discord.

## Cliente web oficial: interacciones y comandos de aplicación (2026-10-17)

Autorización: REA estática del cliente oficial (AGENTS.md, 2026-10-16) y
uso de componentes y comandos de aplicaciones (2026-10-17). Método: bundles
JavaScript públicos del cliente web (build 641004, `web.4d4b2f75fdea5470.js`
y los chunks que carga; los con hash se sirven como `/assets/<hash>.js`),
bajados sin cookies, cabeceras de sesión ni login, en un directorio propio
del scratchpad y leídos con `grep`; nada ejecutado, ningún perfil ni sesión
del cliente leído. Se reimplementa; no se copia código. Uso en RUNE:
[DISCORD_SESSION.md](DISCORD_SESSION.md#comandos-e-interacciones-2026-10-17-rama-comandos-e-interacciones).

**`POST /interactions`.** En todos los tipos, `nonce` es un snowflake en
texto hecho con `Date.now()` (en un comando, el id del mensaje provisional
local), `session_id` es el de la sesión del Gateway (`READY`) y `guild_id`
el del canal, ausente en privados.

- Tipo 2 (comando, `handleCommand`): `{type, application_id, guild_id,
  channel_id, session_id, data, nonce, analytics_location: "slash_ui",
  section_name, source}`; `data = {version, id, guild_id (el del comando,
  ausente si es global), name (el nombre sin traducir), type, options,
  application_command (el comando del índice normalizado), target_id (sólo
  menús contextuales), attachments}`. Opciones `{type, name, value,
  focused}`, subcomandos anidados; usuarios, roles, canales y mencionables
  como id en texto, enteros y números como número, adjuntos como índice en
  `data.attachments` (subidos antes a la nube).
- Tipo 3 (parte de un mensaje): `{type: 3, nonce, guild_id, channel_id,
  message_flags (las del mensaje), message_id, application_id (del mensaje,
  o el autor), session_id, data}`; `data = {component_type, custom_id}` en
  botones y, en menús, además `type` (el mismo número) y `values` (valores de
  la opción o ids). Plazo HTTP de 3 s y una interacción pendiente por
  mensaje.
- Tipo 4 (sugerencias): `{type: 4, application_id, guild_id, channel_id,
  session_id, data, nonce}`, sin `analytics_location`; `data` igual al del
  tipo 2 (con `application_command`), la opción que se escribe con
  `focused: true` y su texto tal cual aunque sea un número; los adjuntos y
  las opciones vacías no enfocadas no van. Las respuestas se guardan por
  opción y texto y se olvidan cuando cambian las otras opciones.
- Tipo 5 (modal): `{type: 5, application_id, channel_id, guild_id, data:
  {id, custom_id, components, attachments}, session_id, nonce}`; los
  componentes con la estructura recibida y sin `id`: fila `{type: 1,
  components}`, etiqueta `{type: 18, component}`, texto `{type: 10}`, campo
  de texto `{type: 4, custom_id, value | null}`, menús 3 y 5–8 `{type,
  custom_id, values | null}`, opción única `{type: 21, custom_id, value |
  null}`, varias `{type: 22, custom_id, values | null}`, casilla `{type: 23,
  custom_id, value: bool}` y archivos `{type: 19, custom_id, values}` con
  índices en `data.attachments`. Un 429 se reintenta tras `retry_after`;
  otro error es un `INTERACTION_FAILURE` local.
- Errores: el código 50035 toma el primer `_errors[0]`; versión de comando
  inválida, comando inválido o el código 10005 marcan el índice como viejo.

**Gateway.** `INTERACTION_CREATE {id, nonce}` (recibida),
`INTERACTION_SUCCESS {id, nonce}` (terminada), `INTERACTION_FAILURE {id,
nonce, reason_code}` (1 desconocido, 2 «no respondió a tiempo», 3 en
adelante actividades), `APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE {choices:
[{name, name_localized, value}], nonce}`, `INTERACTION_MODAL_CREATE {id,
channel_id, custom_id, application, title, components, nonce, resolved}`
(borra el mensaje provisional de ese nonce), `GUILD_APPLICATION_COMMAND_INDEX_UPDATE
{guild_id, version}` (marca viejo el índice del servidor y los de los
privados con sus bots); `APPLICATION_COMMAND_PERMISSIONS_UPDATE` se
reenvía pero ningún store lo atiende.

**Índice.** `GET /guilds/{id}/application-command-index`,
`/channels/{id}/application-command-index` (sólo un privado con un bot),
`/users/@me/application-command-index` (aplicaciones del usuario) y
`/applications/{id}/application-command-index`; respuesta
`{applications, application_commands, version}`. El store hace `name =
name_default ?? name` (y lo mismo con descripciones y choices). Un `202` es
un índice en preparación: tres intentos cada 5 s; un 429 espera
`retry_after`. Viejo cuando la versión no coincide con la del servidor; se
vuelve a pedir la próxima vez que se usa `/`. Cada `CONNECTION_OPEN` los
marca viejos a todos.

**Interfaz.** La parte pulsada muestra carga mientras su interacción está
pendiente y sin fallo, y las demás del mensaje se deshabilitan. Línea bajo
el mensaje: «Sending command...», «{applicationName} is thinking...» (creada,
o mensaje con la marca 128), «The application did not respond» (vencida) y
«This interaction failed»; un 429 dice «Slow down! You're clicking too
fast.». Plazos: 3 s por HTTP, 15 min para un mensaje diferido (marca 128),
10 s para un modal enviado (2 min con archivos); las partes no tienen reloj
propio, dependen de `INTERACTION_FAILURE` con `reason_code` 2.

**Efímeros.** Marca 64 (`EPHEMERAL`) y 128 (`LOADING`); «Only you can see
this • Dismiss message» (con un motivo de `ephemerality_reason` si viene).
Descartar no hace pedidos: es un `MESSAGE_DELETE` local.

**Uso en RUNE (sólo fixtures, sin probar contra el servicio).** Tipos 2 a 5
con esos campos (sin `section_name`, `source`, `target_id` ni adjuntos),
índice de servidor y de privado con un bot, `GUILD_APPLICATION_COMMAND_INDEX_UPDATE`
y cada READY como invalidación, `reason_code` 2 con su texto, descartar
local. Espera de 30 s como Serein. Archivos en modales (19) desde el
2026-10-18, con `data.attachments` por índice y 2 min de espera. **No
hecho** (el dueño, 2026-10-18: sólo lo que tiene Serein, y Serein no lo
tiene): índice del usuario (`/users/@me/...`), reintentos del `202`, el
mensaje provisional del comando, el texto de `ephemerality_reason`,
adjuntos en comandos y los menús contextuales (tipos 2 y 3).
**Sin confirmar**: lo que llevan `section_name` y `source`; cómo normaliza
el cliente los componentes de un modal al recibirlos.

## Cliente web oficial: popout de perfil (2026-10-17)

Pedido del dueño tras su primera prueba con la cuenta real («hacele REA a
esa card para entender la lógica»). Autorización: REA estática del cliente
oficial (AGENTS.md, 2026-10-16). Método: el bundle principal ya bajado
(`web.4d4b2f75fdea5470.js`, build 641004) y los chunks perezosos de esa
build (mismos ids de módulo), más las 1271 hojas CSS públicas de la misma
build, bajadas con `curl` sin cookies, cabeceras de sesión ni login a un
directorio propio del scratchpad (`perfil-real-css`); leídos con `grep` y
scripts propios con contexto acotado (Python con `-I`). Nada ejecutado,
ningún perfil, sesión, caché ni tráfico del cliente oficial leído. Se
reimplementa; no se copia código ni recursos. Evidencia como `módulo
(archivo)`; sin archivo, el módulo está en el bundle principal. Uso en
RUNE: [PROFILES.md](PROFILES.md#primera-prueba-con-una-cuenta-real-2026-10-17-rama-perfil-real).

**Lugar del popout** (922016, 623646; 284055 en `bf3e6ee72c2d4db9.js`).
300 px de ancho (`--custom-user-profile-popout-width`). Lista de miembros:
`position: "left"`, `spacing: 16`; autor de un mensaje y persona en voz:
`"right"`, 8; ventanas angostas, centrado. Alineado arriba con el ancla;
sólo se da vuelta en el eje horizontal (el lado con más lugar); en
vertical no se da vuelta, se acota a 12 px del borde
(`top = max(12, H − 12 − alto)`). Se recalcula al montarse, al cambiar el
tamaño de la ventana y con un `ResizeObserver` del popout, y sólo
actualiza si cambió; mira si el ancla se movió. Se cierra si se desplaza
un ancestro del ancla. Alto máximo `100vh − 20px`, con el cuerpo
desplazable entre encabezado y pie fijos. El perfil se precarga al apretar
el botón; a los 250 ms sin respuesta muestra un esqueleto.

**Orden** (727584 en `735c0f47031b8bb8.js`). Encabezado: banner, avatar,
burbuja del estado, botones (relación, «…» con «View Full Profile»).
Cuerpo (gap 12, padding 4 16 8): aviso si falló parte del perfil; nombre
(20 px, abre el perfil completo), ícono de nota, BOT, usuario, «•
pronombres», etiqueta de servidor, insignias (12 por fila); solicitudes;
bloqueado; «N amigos en común • N servidores en común»; perfil privado;
biografía; widgets; «Actividad actual» (sólo con la persona conectada);
roles (con servidor y miembro). Pie: compositor «Message @nombre». No hay
«Member since» en el popout.

**Medidas** (207634; CSS 875762, 253781, 336046). Banner 300×105 con o sin
imagen; sin imagen, el color de acento del perfil o el dominante del
avatar. Avatar 80 px en (16, 61), su centro 4 px sobre el borde del
banner, recortado del banner con un círculo de radio 46; estado 16 px;
decoración 1,2 veces el avatar. Tema: degradé del color primario al
secundario y una capa negra (o blanca) al 60 %.

**Efectos** (480335, 942426, 474012; CSS 291103). No hay ruta propia: el
efecto es un producto, `GET /collectibles-products/{sku}`, ítem
`tenant_metadata.collectibles.item` con `reducedMotionSrc`,
`staticFrameSrc`, `thumbnailPreviewSrc`, `animationType` (1 persistente, 2
intermitente) y `effects[]` con `src`, `loop`, `start`, `duration`,
`loopDelay`, `position{x,y}`, `zIndex` y `randomizedSources`, **sin ancho
ni alto**. Nada se muestra hasta que cargaron todas las capas; un reloj a
24 cuadros por segundo empieza en −500 ms; cada capa (orden `zIndex`) se ve
si `t ≥ start` y (`loop` o `t ≤ start + duration`), con `loopDelay` el
período es `duration + loopDelay` y la capa se oculta en la pausa (al
volver, la imagen animada empieza de nuevo); las aleatorias eligen un
índice. Cada imagen va al **100 % del ancho de la tarjeta**, alto
automático, en `position` sin escalar; contenedor que cubre la tarjeta,
`overflow: hidden`, `pointer-events: none`, arriba de banner y avatar, sin
modo de mezcla ni oscurecimiento (lo único oscuro es un velo mientras se
responde a la persona). Con movimiento reducido, `reducedMotionSrc`. Al
pasar el puntero el efecto baja a 35 % de opacidad en 0,6 s.

**Actividad** (580763, 774926, 970928, 623671 en `cb885370791cbc25.js`).
Imágenes: `mp:<ruta>` contra el media proxy (`.gif` con
`format=webp&animated=true`), `spotify:<id>` → `i.scdn.co/image/<id>`,
`twitch:`, `youtube:`; una clave simple →
`app-assets/{aplicación}/{clave}.png?size=160`; otro prefijo, ninguna. Sin
imagen grande: el ícono del juego, el de la aplicación
(`/applications/{id}/public`, o en lote `/applications/public?application_ids=`),
casos especiales, la chica como grande, y si no hay nada un «?» genérico.
Grande 60 px; chica 24 px redonda a −4 px de la esquina, recortada de la
grande. Tarjeta con padding 8 y gaps 8 (texto 4). Nombre 14 px seminegrita,
detalles 12 px, estado (oculto si hay grupo: «estado (n de m)»). Tiempo:
ícono verde y `m:ss` (sin «transcurrido»), cada segundo; escuchando o
viendo con inicio y fin, barra de 2 px cada 500 ms.

**Biografía, estado y pronombres** (713804). La biografía usa las reglas
del chat menos bloques de código, saltos `br`, menciones y canales; quedan
negrita, cursiva, enlaces, emoji `:nombre:` y `<a:nombre:id>` (1,375 em).
En el popout, tres líneas (55 px) y «View Full Bio». El estado
personalizado es texto plano con su emoji a 16 px (32 px solo). Los
pronombres, texto plano.

**Nota** (612630, 243166). `GET /users/@me/notes/{id}` al abrirse el
popout si no está en caché; cualquier error (también 404) queda como nota
vacía, sin mensaje; en el popout es un ícono que abre el perfil completo.

**Gente en voz** (607567, 860071). La columna junta estados de voz,
usuarios y miembros; un estado sin usuario conocido no se lista. Un
`VOICE_STATE_UPDATE` (o `_BATCH`) con `member` guarda miembro y usuario
antes del estado. No hay pedido por op 8 para la columna (op 8 lleva
`{guild_id, query, limit, user_ids, presences}`, `user_ids` de a 100, para
autores y menciones). El popout suscribe al miembro (op 37 `members`;
avisa pasados 50). Desde voz, el perfil se pide con el servidor del canal.

**Perfil** (903209, 454719). `GET /users/{id}/profile` con `type=popout`,
`with_mutual_guilds`, `with_mutual_friends`, `with_mutual_friends_count`,
`guild_id`, `connections_role_id`; al abrir; lo guardado se muestra al
instante y se vuelve a pedir pasados 60 s; tras un 404 o 429, 60 s sin
reintentar; si falla, el popout abre con «Unable to load parts of profile».

**Etiqueta de servidor** (685073, 534400, 743981). Se muestra con
`identity_enabled`, etiqueta e `identity_guild_id`, salvo cuarentena;
insignia `clan-badges/{servidor}/{insignia}.png?size=16` a 14 px; texto 12
px. **Sin confirmar**: límites de largo o de caracteres.

**Nameplates** (780898, 214881, 351952, 746002, 641886). Por sku:
`media/v1/collectibles-shop/{sku}/{static|animated|video}`; animado sólo
con foco y la fila bajo el puntero o elegida; fondo
`linear-gradient(90deg, …)` con el color de la paleta, imagen a la derecha
al 60 % (90 % elegida) con una máscara bajo el nombre.

**Uso en RUNE** (sólo fixtures y simulación, sin probar contra el
servicio): lugar fijo de la tarjeta, efecto a todo el ancho y en su tiempo
con `reducedMotionSrc`, actividad (`mp:` a 160 px, la chica como grande,
60/24 px), biografía con emoji en su lugar y tres líneas, miembro del
estado de voz, suscripción del miembro del popout, ícono de la
aplicación (en `emoji-en-linea`). Hecho el 2026-10-18 en
la rama `perfil-tarjeta-y-cosmeticos`: ancho de 300 px y separación de
16 px en la lista, alto máximo con cuerpo desplazable (el de la ventana:
limn nunca deja un popover más alto que ella), nota como ícono, cerrar al
desplazar (rueda de la persona, no los ajustes de la lista), el efecto que
baja al 35 % bajo el puntero (`Element::dim_on_hover`, rama
`rune/hover-dim` de limn), portadas de otros hosts y nameplates por sku y
animados (ver abajo).

## Cliente web oficial: portadas, nameplates y decoraciones (2026-10-18)

Paquete `perfil-tarjeta-y-cosmeticos`. Mismo método y misma build que el
popout (bundle principal `web.4d4b2f75fdea5470.js`, ya bajado sin cookies
ni sesión, leído con `grep` y Python con `-I`); nada ejecutado ni copiado.
Uso en RUNE: [DISCORD_SESSION.md](DISCORD_SESSION.md#portadas-nameplates-por-sku-y-cosméticos-2026-10-18-rama-perfil-tarjeta-y-cosmeticos).

**Portadas** (970928). Las claves de imagen con prefijo se deserializan
así: `spotify:` → `https://i.scdn.co/image/{id}` (con
`encodeURIComponent`); `twitch:` →
`https://static-cdn.jtvnw.net/previews-ttv/live_user_{login}-{ancho}x{alto}.jpg`,
con el tamaño que pide quien la usa (exige un par de números); `youtube:`
→ `https://i.ytimg.com/vi/{id}/hqdefault_live.jpg`; `mp:` contra el media
proxy. La tarjeta de actividad pide `[160,160]` y la vista grande de una
transmisión `[900,500]` (926456). RUNE usa 160×160.

**Nameplates y decoraciones por sku** (746002, 253292, 486020).
`getCollectiblesItemAssetUrl({skuId, assetFormat, assetId})` arma
`https://cdn.discordapp.com/media/v1/collectibles-shop/{sku}/{assetId/}{static|animated|video}`.
La nameplate pide `static` y, en Linux o Safari (sin video), `animated`;
si no, `video`. La decoración de avatar de esta build también se pide por
sku (`animated` si puede animarse, si no `static`) y cae a
`avatar-decoration-presets/{asset}` con `passthrough`. **Sin confirmar**:
el formato que devuelve `animated` (RUNE lo decodifica como cualquier
imagen y, si no puede, deja la quieta).

**Animar al pasar el puntero** (589158 y los chunks `655f1b6f6573a968.js`,
`a08ad7776d592570.js`, `143edccaffd47bdf.js`). Un mismo gancho de avatar
(`animateOnHover`) da avatar, decoración y `onMouseEnter`/`onMouseLeave`
a la fila de la lista de miembros, de privados y amigos, al mensaje y al
mosaico de una llamada (`UserTile`); con movimiento reducido no anima.
RUNE: filas de miembros, nombre, cara o fila de un mensaje, y mosaicos de
voz; las filas de voz de la columna siguen sin decoración (no se encontró
en el bundle que la dibujen).


## Cliente web oficial: emoji en línea y el ícono de la aplicación (2026-10-17)

Pedido del dueño: los emoji de un servidor como imagen en los mensajes, y
el tamaño de uno en una línea frente a un mensaje que es sólo emoji. Mismo
método y build que el [popout](#cliente-web-oficial-popout-de-perfil-2026-10-17):
el bundle `web.4d4b2f75fdea5470.js` y las hojas CSS públicas ya bajadas sin
cookies ni sesión, leídos con `grep` y scripts propios (Python con `-I`).
Nada ejecutado; ningún dato local del cliente oficial leído. Se reimplementa.

| Qué | Evidencia | En RUNE |
| --- | --- | --- |
| Emoji en una línea | `--custom-emoji-size-emoji: 1.375em` (`419070…css`); `.emoji{height/width: var(…); object-fit: contain; vertical-align: bottom}` (`552653…css`) | `TextSpan::image` de limn: un cuadrado de 1,375 veces la letra (22 px en un mensaje de 16 px) |
| Mensaje que es sólo emoji | `--custom-emoji-size-jumbo-emoji: 3rem`; `.emoji.jumboable` (`552653…css`); la regla (521981, `d`): todos los nodos son `emoji`, `customEmoji` o `soundboard`, o texto en blanco, y a lo sumo `MAX_EMOJI_TO_BE_JUMBO` (307731: 30) | 48 px (`text::JUMBO`), hasta 30, de servidor o Unicode |
| Compacto | `.compact .messageContent .jumboable{… var(--custom-emoji-size-emoji)}` (`738015…css`) | En modo compacto, el tamaño de la línea |
| Respuesta citada | `.repliedTextPreview .emoji{height/width: 1.25em; vertical-align: text-top}` (`738015…css`) | 1,25 veces los 14 px de la línea de la respuesta |
| Tema del canal | `parseTopic` usa `TOPIC_RULES`: las reglas del chat menos `inlineCode`, `codeBlock`, `br`, `blockQuote`, `subtext` y `soundboard`; `customEmoji` queda. El nombre de un canal es texto plano | El tema con sus emoji (encabezado y diálogo); los nombres, sin cambios |
| Imagen pedida | `getEmojiURL({id, animated, size})` → `emojis/{id}.{webp}?size=…&animated=true` (565645 con el tamaño de 732139: `{reaction: 40, default: 44, jumbo: 96}` por la escala de la pantalla); `EMOJI_URL_BASE_SIZE = 48` (307731) | `size=48` en todos lados (shortcut: un jumbo en una pantalla escalada se agranda; el oficial pide 96). Desde el 2026-10-18, 96 para el jumbo |
| Ícono de la aplicación | `APPLICATION_PUBLIC: /applications/{id}/public` (652215); `fetchApplication` (27363) con `APPLICATION_FETCH_FAIL` y el filtro `didFetchingApplicationFail` (no se vuelve a pedir en la sesión); `getIconURL(size)` → `cdn.discordapp.com/app-icons/{id}/{icon}.png?size=…` (223273) | `app_icons`: una lectura a la vez, caché por sesión con las fallas, `size=128` como `app-assets` |

Serein pide el ícono a `/applications/{id}/rpc` desde su cargador de
imágenes (`apps/desktop/src/avatars.rs`); RUNE sigue al cliente oficial.

## Cliente web oficial: llamadas privadas (2026-10-17)

Misma autorización (REA estática del cliente oficial, 2026-10-16) y mismo
método: bundles públicos ya descargados sin sesión, leídos con `grep` y
ventanas de contexto (Python con `-I`); nada ejecutado, ningún dato local
del cliente oficial leído, ninguna petición a Discord. Se reimplementa; no
se copió código. Desplazamientos en bytes dentro de
`web.c5b338cc7e610833.js` (SHA-256 `6f842876…0aec`); el mismo código está
en `web.4d4b2f75fdea5470.js` (`667a771c…084ae`, otra versión) a
desplazamientos parecidos, y `fast-connect.f0cfe6eb7552ae67.js`
(`a05f649b…7f7c`) tiene la lista de opcodes (`CALL_CONNECT = 13`).

| Qué | Dónde | En RUNE |
| --- | --- | --- |
| Rutas `CALL` (`/channels/{id}/call`), `CALL_RING` (`…/call/ring`) y `CALL_STOP_RINGING` (`…/call/stop-ringing`) | ~278 822 | las tres; `CALL` (GET) desde el 2026-10-18 |
| Llamar: en un privado 1:1 (con el id de la otra persona, salvo si está bloqueada), `GET /call` (`oldFormErrors`, `rejectWithError`); con respuesta, `selectVoiceChannel` y `ring` sólo si se pidió sonar **y** `body.ringable`; con error, no se une: registra `OPEN_POPOUT {type: "Not Friend", source: "Call"}` y abre un diálogo de confirmación (título `My50nf`, cuerpo `IdKo2z` con `{username}`, botones `PMsq/b` y `BddRzS`) cuyo confirmar llama a `addRelationship({userId, context: {location: "Call"}})`. En un grupo, unirse y hacer sonar sin el GET | módulo 387755, ~564 003 (releído el 2026-10-18 en `web.c5b338cc7e610833.js`, ~564 437) | igual desde el 2026-10-18 (`Command::CheckCall`): sin `ringable` verdadero entra sin sonar; el diálogo, sólo si Discord rechazó el GET (400, 403 o 404) y no son amigos (sin respuesta, un límite o un 5xx, un aviso), con «Enviar solicitud de amistad» o «Aceptar solicitud de amistad». Textos propios: el catálogo de `My50nf`/`IdKo2z` no está entre los chunks bajados |
| `ring(canal, recipients)`: con llamada existente (`messageId` y no `unavailable`, ~3 209 699) `POST …/call/ring {recipients, analytics_location}`; si no, `CALL_ENQUEUE_RING` y al llegar `CALL_CREATE` sale con `recipients: null` | ~564 792, ~8 984 063 | el ring espera al `CALL_CREATE`; `{"recipients": null}` o la lista |
| `stopRinging(canal, recipients)` → `POST …/call/stop-ringing {recipients}` | ~565 077 | «Dejar de llamar» y rechazar (con la cuenta, como Serein; el handler del rechazo está en un chunk no descargado) |
| Tipos que se llaman: `CALLABLE = {1, 3}` | ~11 856 272 | privados y grupos |
| `CALL_CREATE {channel_id, message_id, region, ongoing_rings, voice_states[]}`; `CALL_UPDATE` igual sin estados; `CALL_DELETE {channel_id, unavailable}` | ~4 932 787, ~4 933 353, ~4 933 494 | se leen `channel_id`, `ongoing_rings`, `ringing` (Serein), `voice_states`, `unavailable` |
| `ongoing_rings`: claves = a quién suena, valor = quién llamó; `CallStore.ringing = Object.keys(ongoing_rings)` | ~8 982 489 | un valor nulo no suena |
| Entrante: el id propio entre las claves con valor no nulo; se quita al dejar de estar, con `CALL_DELETE`, al entrar a esa voz o si se borra el canal; todo suprimido en No molestar o `quietMode` | ~1 050 325, ~1 050 724 | igual, y el aviso se muestra salvo con ese privado abierto (el oficial también lo oculta así, ~2 178 531) |
| `CALL_DELETE` de la llamada propia: sale de la voz (op 4 nulo) | ~4 953 522 | igual |
| Op 13 `{channel_id}`, sólo con sesión: al elegir un canal fuera de servidores que no conoce, y al abrir/reanudar el Gateway con el canal abierto | ~4 998 935, ~4 954 588 | antes de unirse (Serein) y la primera vez que se lee un privado; de nuevo al reconectar |
| Op 4 `{guild_id, channel_id, self_mute, self_deaf, self_video, flags}`; `guild_id` nulo en una llamada | ~4 997 843 | sin `flags` ni regiones |
| `VOICE_SERVER_UPDATE {guild_id, channel_id, endpoint, token}`: guild nulo o igual, canal nulo o igual | ~4 932 645, ~9 263 765 | en un privado, guild nulo y canal igual |
| `server_id` de la conexión de voz: el del stream, si no el guild, si no el canal | ~2 479 108 | el canal en un privado (y el grupo DAVE) |
| Op 18 `{type: "guild"\|"call", guild_id, channel_id, preferred_region}`; clave `call:{canal}:{dueño}` | ~710 348, ~4 998 992, ~5 089 537 | `type: "call"` y claves `call:` |
| Sonidos: `call_calling` mientras la llamada propia hace sonar a alguien y hay menos de 2 en el canal; `call_ringing` en bucle mientras hay una entrante | ~3 190 259, ~2 178 531 | sonidos propios de RUNE `ring_out` y `ring` (nunca los de Discord) |
| Llamada perdida: el mensaje `call {participants, ended_timestamp}` sin la cuenta | ~6 077 522 | sin cambios (RUNE dibuja el tipo de mensaje «llamada» como antes) |

**Sin confirmar**: cuánto tarda el servidor en dejar de sonar (no hay
plazo en el cliente: lo decide Discord con `CALL_UPDATE`); qué hace el
rechazo del aviso oficial (chunk 417680 no descargado; RUNE sigue a
Serein); el comportamiento con la llamada en otro dispositivo; los textos
en inglés (el catálogo no está entre los chunks descargados).

## Video por hardware y sonido de la transmisión en Linux (2026-10-17)

Misma autorización y método: del paquete instalado, `strings -t x` sobre
`discord_voice.node`, sin cargarlo ni ejecutar nada; del cliente web, el
bundle público que ya estaba descargado, leído con `grep`. No se abrió
ningún perfil, sesión ni dato local del cliente oficial. Lo que sale de
esto está en [VIDEO.md](VIDEO.md#video-por-hardware-2026-10-17-rama-video-por-hardware).

| Archivo | SHA-256 |
| --- | --- |
| `discord_voice.node` (Discord 1.0.161, Nix) | `28300009b1b5a9259e6f77454a6d800155fed808ed013ac53235f8d0b6049a23` |
| `web.4d4b2f75fdea5470.js` | `667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae` |

**Codificar por GPU.** El nativo trae `cros-codecs` y `cros-libva` (rutas
`/root/.cargo/git/checkouts/cros-codecs-…`, `cros_libva::buffer`),
`components/video/coding/vaapi-media-encoding/src/{encoder,amd_backend,vulkan_backend,probe}.rs`,
«Creating AMD VAAPI encoder», «Creating Vulkan VAAPI encoder», «H264 VAAPI
encoder low_power», «Failed to create VAAPI surface from DMA-BUF» y un
`gpu_encoder_helper` aparte. O sea: VA-API directo desde Rust (y Vulkan
Video), con la captura en DMA-BUF hacia la superficie VA, en otro proceso.

**Sonido de la transmisión.** `soundshare/pulseaudio_controller.cpp` y
`soundshare_audio_device_linux.cpp`; cadenas «PulseAudioController
OnPulseAudioEvent sink input added», «Calling
pa_context_get_sink_input_info_list», `application.process.id`,
`pipewire.sec.pid`, «Got client info for index … pid =», «SinkIdentified
name: … target pid:», `/proc/%d/stat`, `pa_stream_set_monitor_stream`,
«game capture», `soundsharePid`. En el web (~bundle de arriba):
`soundshareId` sale de `getAudioPid(pid)` de la fuente
(`getDiscordUtils().getAudioPid`), `supportsScreenSoundshare()` en Linux es
la capacidad nativa `SCREEN_SOUNDSHARE`. Lectura: graba los sink inputs
del pid de destino (y su árbol de procesos), no todo; qué hace con una
pantalla entera sin pid queda **sin confirmar**.

## Cliente web oficial: pulido de la interfaz real (2026-10-17, rama `pulido-ui-real`)

Misma autorización y método: bundles JS y CSS públicos ya descargados sin
sesión (`web.4d4b2f75fdea5470.js`, chunks diferidos y 1271 hojas de
estilo), leídos con `grep` y scripts propios (Python con `-I`); nada
ejecutado, ningún dato local del cliente oficial, ninguna petición. Se
reimplementa; no se copió código, texto ni recurso.

| Qué | Dónde | En RUNE |
| --- | --- | --- |
| Fila de miembro: 42 px (`rowHeight:42` en JS) con `padding: 1px 0`: el fondo de hover o nameplate mide 40 y dos fondos quedan a 2 px; radio `--radius-sm` = 8; 8 px a cada lado; avatar 32 a 12 del nombre | `40791…css` `.member__5d473`, `56606…css` `.container__91a9d` | `members::ROW_MARGIN` (1 px arriba y abajo del fondo) y `RADIUS` 8; la placa con el mismo radio |
| Nameplate: cubre todo el fondo, avatar incluido, con su radio | `752704…css` `.container_df39b2` | igual |
| Miembro sin cargar: círculo 32 y barra de 14 px, radio 8, ancho al azar 40–79 %, pulso de 3 s | `ub` @140070, `placeholderMaxWidth` | ancho según el lugar (determinista), pulso de limn (quieto con movimiento reducido) |
| Avatar por defecto: discriminador > 0 → `% 5`; si no, `(id >> 22) % 6`; un bot «0000» (webhook) → 0. El web usa seis imágenes empaquetadas | @9650628, módulo 3451 | el mismo índice, pedido al CDN público `embed/avatars/{i}.png` (pedido del dueño: nunca empaquetado) |
| Esqueleto del historial: alto H = max(ventana − 145, 400); grupos de 1–4 mensajes; avatar 40 (8 %), nombre 80–120 (10–20 %), líneas 120–320 de 16 en 22 (6 %), ~1 grupo de 4 con imagen 140–400 × 100–320 (3 %); radio 8; sin animación; anclado abajo (`justify-content: flex-end`) | módulo 114212, `551572…css`, `330150…css` | `message::history_skeleton`, medidas de RUNE, semilla del canal, lista que empieza en su final |
| Fijados cargando: spinner, no esqueleto | `.loadingPlaceholder` | spinner de limn |
| Tooltip del servidor: nombre (2 líneas), filas de escenario (máx. 3), voz (todos los canales de voz juntos, sin el AFK ni los silenciados, máx. 6), transmisión y actividades (máx. 6), «Silenciado» al pie; caras de 24 superpuestas −6; con más del máximo, máx − 1 y «+N» (pastilla de 24, radio 12); ancho 196; abre sin demora | `GuildTooltip` @5619566, `UserSummaryItem` 104171, `785332…css` | `Tooltip::faces` (limn #80): escenario (máx. 3), oyentes, voz, transmisión y apps (máx. 6); detalle en la [tabla del 2026-10-18](#cliente-web-oficial-tooltip-del-servidor-y-apps-en-voz-2026-10-18) |
| Pantalla de carga: video del logo y un consejo hasta READY; a los 10 s una sección de problemas (enlaces de soporte y estado); sale con fundido de 200 ms; con movimiento reducido el logo queda quieto; vuelve con `WILL_RECONNECT` | clase `Pm` @1843130–1848806 | `rune_ui::splash`: etapa en vez de consejo, motivo y botones a los 10 s, fundido de limn; «Reconectando» sólo con la sesión cargada (decisión del dueño) |

**Sin confirmar o pendiente:** el canal AFK, los canales silenciados, las
filas de escenario y de apps y el orden por canal quedaron hechos el
2026-10-18 ([sección siguiente](#cliente-web-oficial-tooltip-del-servidor-y-apps-en-voz-2026-10-18)).
El tooltip del carril abre al instante y con el título a 16 px en
seminegrita, como el oficial (0 ms, 16 px semibold), desde las ramas
`limn-tipografia-y-opciones` y `limn-avisos-y-compositor` (2026-10-18;
`Tooltip::delay_ms`, `title_font_size`, `title_font_family`). Que las imágenes del CDN sean idénticas a las empaquetadas
no se comprobó. Serein usa la misma fórmula del avatar por defecto y no
tiene pantalla de carga (muestra un aviso de reconexión).

## Cliente web oficial: clases de invitación y enlaces con tarjeta (2026-10-18)

Rama `invitaciones`. Misma autorización y método: el bundle público ya
descargado sin sesión (`web.4d4b2f75fdea5470.js`, SHA-256
`667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae`),
leído con `grep` y Python con `-I`; nada ejecutado, ningún dato local del
cliente oficial, ninguna petición. Se reimplementa; no se copió código.
Desplazamientos en bytes:

- **Enlaces con tarjeta** (módulo 721779, ~3 837 308): `codedLinks` del
  mensaje sale del contenido entero; antes se borran los bloques y el
  código en línea (~3 840 366, `replaceAll` con espacios), se quita la
  puntuación final (`[.,;:?'*_~|]+$`), se guarda uno por código y el total
  se corta en **diez** (`.slice(0,10)`, ~3 842 812). Las invitaciones son
  `discord.gg/{código}` y `/invite/{código}` en `discord.com`,
  `discordapp.com`, `ptb.` y `canary.`.
- **Clases** (módulo 172799, ~5 761 477): `GUILD` 0, `GROUP_DM` 1,
  `FRIEND` 2. El módulo 346542 (~5 763 161) deduce la clase sin `type`
  (canal de grupo → grupo; sin servidor y con quien invita → amistad).
  Banderas de la invitación (~11 879 461): `IS_GUEST_INVITE` 1,
  `IS_VIEWED` 2, `IS_ENHANCED` 4, `IS_APPLICATION_BYPASS` 8.
- **Aceptar** (`acceptInvite`, ~634 996): un solo `POST /invites/{code}`
  para las tres clases. Con `IS_GUEST_INVITE` no abre el onboarding.
  `transitionToInvite` (~636 954): sin servidor ni canal y con quien
  invita (amistad), abre el privado con esa persona si ya son amigos; con
  canal, va al canal; un invitado no pasa por la pantalla de bienvenida.

Uso en RUNE: [INVITATIONS.md](INVITATIONS.md#todas-las-clases-y-tarjetas-en-texto-mezclado-2026-10-18).
RUNE además oculta las de un spoiler (como Serein) y acepta la bandera de
invitado sólo exacta (1); `IS_VIEWED`, `IS_ENHANCED` y la de aplicaciones
siguen sin flujo propio al aceptar. Nada de esto se probó contra el
servicio real.

## Cliente web oficial: fijados, súper reacciones y búsqueda (2026-10-18, rama `reacciones-fijados-y-busqueda`)

Misma autorización y método: los bundles JS públicos ya descargados sin
sesión (`web.4d4b2f75fdea5470.js` y chunks diferidos del build 641004, y el
catálogo de textos en inglés `933ace571a9c6560.js`), leídos con `grep` y
scripts propios (Python con `-I`); nada ejecutado, ningún dato local del
cliente oficial, ninguna petición. Se reimplementa; no se copió código.
Uso en RUNE: [DISCORD_SESSION.md](DISCORD_SESSION.md#reacciones-súper-reacciones-fijados-y-búsqueda-2026-10-18-rama-reacciones-fijados-y-busqueda).

| Qué | Dónde | En RUNE |
| --- | --- | --- |
| `PIN_MESSAGES` es el bit 51 (`SEND_POLLS` 49, `BYPASS_SLOWMODE` 52) | tabla de permisos `xBc` en `web.4d4b…js` | `Permissions::PIN_MESSAGES`, `PERMISSION_BITS` |
| Fijar: `can(PIN_MESSAGES) && can(READ_MESSAGE_HISTORY)`, o un privado que no sea del sistema | chunk `681e846fab87a0c8.js` (módulo 492841) y la lista de fijados (`1bce0236639292ba.js`) | lo mismo: `PIN_MESSAGES` **y** `READ_HISTORY` en un servidor (Serein sólo mira `MANAGE_MESSAGES`; que el servicio todavía lo tome no está confirmado) |
| Reacción leída: si hay `count_details`, `burst_count = burst ?? 0` y `count = normal ?? 0`; al volver a mandar un mensaje, `count + burst_count` | normalizador de mensajes de `web.4d4b…js` | `Reaction { count, burst }` igual |
| Súper reacción optimista: suma a `burst_count`, marca `me_burst` y toma `burst_colors` del evento sólo si la reacción no tenía | store de mensajes de `web.4d4b…js` (la rama `BURST` del cambio de reacción) | `Message::count_burst` (los primeros colores quedan) |
| Pastilla: tipo `BURST` con fondo y texto de una paleta sacada de `burst_colors` (contraste 5–7, opacidad 0,1–0,5), borde si es propia; en el listado de quién reaccionó, una entrada por tipo | chunks `33d5952c32e4d1bc.js`, `ded0e401f9f76547.js` | pastilla aparte con el primer color (oscurecido en el tema claro), sin la paleta de contraste; no se manda ninguna |
| Orden de la búsqueda: más nuevos `timestamp/desc` (por defecto), más viejos `timestamp/asc`, más relevantes `relevance/desc` | módulo con `MOST_RELEVANT` en `web.4d4b…js` | «Más recientes» y «Más relevantes» |
| Filtros: `from`, `mentions`, `has` (`link`, `poll`, `forward`, …), `in`, `before`, `during`, `after`, `on`, `pinned`, `authorType`, `fileType`, `linkFrom` → `queryKey:"link_hostname"` | catálogo `933ace…js` y tabla de tokens en `web.4d4b…js` | `has:poll`, `link_from:`/`linkFrom:`/`sitio:`; las claves aceptan `_` o camelCase |

**Sin confirmar:** si `link_hostname` también encuentra subdominios (RUNE,
en lo cargado, los cuenta) y el orden exacto de las pastillas de un mismo
emoji en la fila del mensaje (RUNE pone la súper antes de la normal).

## Cliente web oficial: contenido explícito y registros de llamadas (2026-10-18)

Misma autorización y método que las secciones anteriores: bundles JS
públicos ya descargados sin sesión, leídos con `grep` y un script propio
(Python con `-I`); nada ejecutado, ningún dato local del cliente oficial,
ninguna petición. Se reimplementa; no se copió código. Uso en RUNE:
[DISCORD_SESSION.md](DISCORD_SESSION.md#contenido-de-mensajes-2026-10-18-rama-contenido-de-mensajes).

| Archivo | SHA-256 |
| --- | --- |
| `web.4d4b2f75fdea5470.js` | `667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae` |
| `8539ed9c57c49568.js` (mensajes de sistema) | `1ceb64d590c99586eee539ad81f8acf9a59b670c234f3299d2b39c7f632caa21` |
| `b1d5b19d5f99db6e.js` (vista previa de mensajes) | `39168a945f788ad077fd0fd307f014b253cce3586aa353cf07314ebf833349d2` |

**Banderas.** Adjunto (`MessageAttachmentFlags`): `IS_CLIP` 1,
`IS_THUMBNAIL` 2, `IS_REMIX` 4, `IS_SPOILER` 8, `CONTAINS_EXPLICIT_MEDIA`
16, `IS_ANIMATED` 32, `CONTAINS_GORE_CONTENT` 64,
`CONTAINS_SELF_HARM_CONTENT` 128, `HAS_TIMELINE_COMMENTS` 256. Tarjeta
(`MessageEmbedFlags`): `CONTAINS_EXPLICIT_MEDIA` 16,
`IS_CONTENT_INVENTORY_ENTRY` 32, `CONTAINS_GORE_CONTENT` 64,
`CONTAINS_SELF_HARM_CONTENT` 128. Un medio genérico de componentes usa
`content_scan_metadata.flags` (explícito 1, violento 2, autolesión 4). El
adjunto también trae `placeholder` y `placeholder_version` (el ThumbHash).

**Decisión.** Por tipo de daño (explícito, violento, autolesión), el ajuste
del protobuf `textAndImages.explicitContentSettings` (`explicitContentGuilds`,
`explicitContentFriendDm`, `explicitContentNonFriendDm`; valores 0 sin
elegir, 1 mostrar, 2 difuminar, 3 bloquear) se elige por canal y autor:
servidor → guilds; privado o grupo → friend si el autor es amigo, si no
non-friend; el autor propio, o un canal desconocido, nunca. Difuminar y
bloquear tapan igual en el cliente (`[BLOCK, BLUR].includes`): el bloqueo lo
hace el servicio. Sin elegir: con la experiencia `SENSITIVE_CONTENT` (por
edad) privado de no amigo bloquea y lo demás difumina; con `nsfwAllowed ===
false`, servidores difumina y privados según el filtro viejo
(`explicit_content_filter`); si no (adulto), servidores muestra y privados
según el filtro viejo (desactivado: muestra; «no amigos»: amigo muestra, no
amigo bloquea; «todos»: bloquea). Un adjunto sin `content_scan_version`
(escaneo pendiente) también se tapa mientras tanto, salvo un ajuste de
desarrollo. Los reenvíos se revisan en sus `message_snapshots`.

**En RUNE.** Sólo explícito (bit 16 en adjuntos y tarjetas, bit 1 de
`content_scan_metadata.flags` en las imágenes de componentes), con
`Store::obscures_explicit`; sin elegir, el caso adulto con el filtro viejo
«no amigos» (el valor de siempre): servidores y amigos se ven, los demás se
tapan. Sin escaneo pendiente, violento ni autolesión (ver
[todo.md](../tasks/todo.md)).

**Registro de llamada (tipo 3).** El mensaje lleva `call: {participants,
ended_timestamp}`; la duración es `ended_timestamp − timestamp` del mensaje,
humanizada. Está perdida si la llamada no está activa, hay `call` y la
cuenta no está en `participants`: «You missed a call from {username} (that
lasted {callDuration})»; si no, «{username} started a call (that lasted
{callDuration})», con «Join the call» mientras sigue y la cuenta no está en
ella. El nombre es un enlace al perfil (`usernameHook`), como en los demás
mensajes de sistema (añadir, quitar, renombrar, fijar).

## Cliente web oficial: tooltip del servidor y apps en voz (2026-10-18, rama `riel-y-notificaciones`)

Misma autorización y método: el bundle público ya descargado
(`web.4d4b2f75fdea5470.js`), leído con `grep` y Python con `-I`; nada
ejecutado, ningún dato local del cliente oficial, ninguna petición. Se
reimplementa; no se copió código, texto ni recurso.

| Qué | Dónde | En RUNE |
| --- | --- | --- |
| Filas del tooltip: escenario (hablantes de los escenarios salvo el AFK, máx. 3) con el ícono de escenario y, al lado, cuántos escuchan (`getParticipantCount(…, AUDIENCE)`); voz (estados de voz de los canales `GUILD_VOICE` que no están silenciados, salvo el AFK, sin bloqueados ni quienes transmiten o usan una app, máx. 6); transmisión (dueños de transmisiones de canales no silenciados, sin quienes usan una app, máx. 6); apps (participantes de actividades embebidas, salvo las de un `GUILD_SPACE`, máx. 6); «Silenciado» al pie | `iD` y `iL` cerca de `GuildTooltip - nonBlockedUsers` | `rune_ui::rail::voice` y `RailVoice::{stage, listeners, people, streaming, activities}`; el conteo de oyentes va en una fila propia (`icon_row`), no al lado de las caras (ver limn abajo). Desvío: la fila de apps sigue la regla de la de voz (sin el AFK, canales que la cuenta no ve ni silenciados, y sólo quien sigue en el canal), para que el altavoz quede como el oficial. `isChannelMuted` mira sólo el silencio del canal, no el de su categoría (`isCategoryMuted` es aparte), y RUNE igual |
| Orden: canales en el orden de su lista y, dentro de cada uno, como el almacén de estados de voz; después lo reordena por afinidad (`getUserAffinitiesMap`) | `getChannels(t)[VOCAL]`, `ts.L(…)` | `Store::voice_by_server` por canal y después por llegada; RUNE no tiene afinidades |
| Altavoz del riel (`getGuildMediaState`): un servidor silenciado no muestra nada salvo con la llamada propia; `audio` si hay alguien en un canal que no es escenario ni AFK, visible y no silenciado | clase `GuildMediaStateStore`, función `B` | mismo criterio con `RailVoice::count` (voz, transmisión y apps, las tres ya filtradas así) y `RailItem::muted` |
| Apps en voz: `activity_instances` de cada guild en `GUILD_CREATE` y en `CONNECTION_OPEN_SUPPLEMENTAL` (el READY_SUPPLEMENTAL); `EMBEDDED_ACTIVITY_UPDATE_V2` con `{application_id, launch_id, composite_instance_id, location: {id, kind, channel_id, guild_id}, participants: [{user_id, session_id, …}]}`, guardada por lugar y aplicación, y sin participantes se borra | `EmbeddedActivitiesStore`, función `X` | `rune-discord-session::activities`, `Event::VoiceActivity`, `Store::voice_activities` |

**Para limn (sin PR):** la fila de caras de `Tooltip::faces` no lleva un
texto ni un ícono al final; el oficial pone «🎧 N» al lado de los
hablantes del escenario. RUNE lo muestra en una fila aparte
(`icon_row`). Sería `Tooltip::faces_with_trailing(…)` o un `trailing:
Option<TooltipItem>` en la sección de caras.

## Cliente web oficial: GIF, emoji grandes y permisos externos (2026-10-18)

Rama `emoji-y-estados`. Mismo método: el bundle `web.4d4b2f75fdea5470.js`,
sus chunks diferidos y las hojas CSS ya descargados sin sesión por los
trabajos anteriores, leídos con `grep` y scripts propios (Python con
`-I`); nada ejecutado ni descargado de nuevo, ningún dato local del
cliente oficial. Se reimplementa; no se copió código, texto ni recurso.

| Qué | Evidencia | En RUNE |
| --- | --- | --- |
| Búsqueda de GIF sin páginas: `GIFS_SEARCH` con `q`, `media_format`, `locale` y `limit`, sin `offset`; la grilla de resultados pasa `searchOffset: 0` y `searchLimit: null`, y al acercarse al final del scroll pide sugerencias (`GIFS_SUGGEST`), no más GIF | módulo 497685 (`web.4d4b…`), chunk `84dbcf0b0bb53039.js` (clase de la grilla, `handleScroll`) | «Más GIF» manda `offset` desde la segunda página (pedido del dueño); como el oficial no lo usa, no está probado que Discord lo respete: una página repetida corta la paginación |
| Tamaño pedido de un emoji: `getEmojiURL` pide `size = kr(tamaño × devicePixelRatio)`, con `{reaction: 40, default: 44, jumbo: 96}` | módulos 565645 y 732139 | 48 en una línea y 96 para un mensaje de sólo emoji; RUNE normaliza cada emoji a 128 px, así que 96 cubre hasta escala 2 |
| `™`, `©` y `®` (con o sin U+FE0F) nunca son imagen de emoji | módulo 350593 (`new Set(["™", …])`) | `twemoji::emoji_len` no los cuenta como emoji |
| Por qué no se puede usar un emoji: en un canal de servidor sin `USE_EXTERNAL_EMOJIS`, `DISALLOW_EXTERNAL` antes que el plan (`PREMIUM_LOCKED`) y que el emoji animado | `EmojiDisabledReasons` y la función que las elige (`web.4d4b…`) | `Store::emoji_perk` y `sticker_perk` preguntan primero por `external_emoji` / `external_stickers` |
| Estado en las listas: emoji de 14 px junto a texto chico (16 junto a texto mediano), 4 px de separación, texto con `text-overflow: ellipsis` | `691398…css` (`--custom-activity-status-emoji-size`, `.customStatusContent`) | `identity::status_line`: imagen de 14 px y la línea recortada de limn |

**Sin confirmar:** si `/gifs/search` y `/gifs/trending-gifs` aceptan
`offset` (Serein tampoco pagina). El picker oficial de resultados de
búsqueda que arma `limit` vive en un chunk que no está entre los
descargados.

## Cliente web oficial: plan y edición del perfil (2026-10-18)

Rama `edicion-de-perfil`. Mismo método: el bundle público
`web.c5b338cc7e610833.js` (SHA-256 en la sección de tipos de embed), ya
descargado sin sesión, leído con `grep` y Python con `-I`; nada ejecutado
ni descargado de nuevo, ningún dato local del cliente oficial. Se
reimplementa; no se copió código, texto ni recurso.

| Qué | Evidencia | En RUNE |
| --- | --- | --- |
| Qué da cada plan: catálogo por SKU, `TIER_0` (Basic, `premium_type` 3) sin avatar animado, banner ni perfil por servidor; `TIER_1` (Classic, 1) con `ANIMATED_AVATAR`; `TIER_2` (Nitro, 2) con `ANIMATED_AVATAR`, `PREMIUM_GUILD_MEMBER_PROFILE` y `PROFILE_PREMIUM_FEATURES` («custom banner and avatar decoration») | `el = Object.freeze({[TIER_0]: …, [TIER_1]: …, [TIER_2]: …})` junto a `canUseAnimatedAvatar`, `canUsePremiumProfileCustomization` y `canUsePremiumGuildMemberProfile`; `u0` mapea `premium_type` → SKU | `identity::perks` en la sesión; `Store::profile_abilities` |
| Un GIF elegido como avatar sin `canUseAnimatedAvatar`, o un banner sin `canUsePremiumProfileCustomization`, abre la oferta en vez de quedar en el borrador | `maybeOpenProfilePreviewModal` (`"image/gif" !== n.type`) | RUNE no lo manda: usa el primer cuadro con el motivo, o apaga «Cambiar» del banner |
| El selector del avatar acepta archivos de hasta 8 MiB (`maxFileSizeBytes: 8388608`) y hay un modal de recorte de GIF (`GIF_CROPPING_MODAL`) | constantes de analítica y el selector de íconos | `ProfileImage::MAX_ANIMATED_BYTES`; el GIF se recorta igual en cada cuadro |

**Sin confirmar:** cómo recorta el oficial un GIF (el modal vive en un chunk
no descargado) y el tope de bytes que acepta Discord en el `PATCH`.

## Cliente web oficial: hora de inicio de los canales de voz (2026-10-18)

Misma autorización (REA estática del cliente oficial, 2026-10-16) y mismo
método: bundles públicos ya descargados sin sesión, leídos con `grep -bo`;
nada ejecutado, ninguna petición a Discord. Se reimplementa; no se copió
código. Desplazamientos en bytes dentro de `web.c5b338cc7e610833.js`
(SHA-256 `6f842876…0aec`).

| Hallazgo | Desplazamiento | En RUNE |
| --- | --- | --- |
| `REQUEST_CHANNEL_INFO=43` en la lista de opcodes | 4 997 667 | op 43 |
| `requestChannelInfo(e,t){this.send(…REQUEST_CHANNEL_INFO,{guild_id:e,fields:t})}` | 5 000 027 | `{"guild_id", "fields"}` |
| Lo pide una vez por servidor, si no lo pidió y el servidor está disponible, al mostrar un canal `GUILD_VOICE`: `requestChannelInfo(e,["status","voice_start_time"])` | 3 250 929, 3 251 023 | una vez por conexión y servidor, al mostrar el servidor en la columna de canales (`Command::WatchServer`); sólo `voice_start_time` (RUNE no muestra el estado del canal) |
| `VoiceChannelStartTimeStore`: vacía lo pedido en `CONNECTION_OPEN` y `CONNECTION_RESUMED`; `CHANNEL_INFO` reemplaza los del servidor; `VOICE_CHANNEL_START_TIME_UPDATE` cambia uno; `getStartTime` sólo para `GUILD_VOICE`; segundos × 1000 | 3 258 836 | `Store::voice_started`, vaciado en cada `READY`; la conexión pide de nuevo |
| Lectura de `CHANNEL_INFO` (`channels[].{id,status,voice_start_time}`) y de `VOICE_CHANNEL_START_TIME_UPDATE` (`id`, `guild_id`, `voice_start_time ?? undefined`) | 4 925 250, 4 925 119 | `voice::start_times` |

Dónde lo muestra el cliente oficial está en un módulo diferido que no se
descargó; RUNE lo pone en la fila de la sala. Serein manda op 43 sólo al
entrar a una llamada y muestra el tiempo en su panel de voz; RUNE también lo
pide al entrar.

## Cliente web oficial: reglas del servidor y onboarding (2026-10-18)

Rama `reglas-y-onboarding`. Misma autorización y método: el bundle público
ya descargado sin sesión por trabajos anteriores (`web.4d4b2f75fdea5470.js`,
SHA-256 `667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae`),
leído con `grep` y un script propio (Python con `-I`); nada ejecutado ni
descargado de nuevo, ningún dato local del cliente oficial, ninguna
petición. Se reimplementa; no se copió código, texto ni recurso.

| Qué | Evidencia | En RUNE |
| --- | --- | --- |
| Rutas: `GUILD_MEMBER_VERIFICATION` = `/guilds/{g}/member-verification`, `GUILD_MEMBER_REQUEST_TO_JOIN` = `/guilds/{g}/requests/@me`, `GUILD_ONBOARDING` = `/guilds/{g}/onboarding`, `GUILD_ONBOARDING_RESPONSES` = `/guilds/{g}/onboarding-responses` | tabla de rutas (`Rsh`) | `rune-discord-session::onboarding` |
| Leer las reglas: `GET` con `with_guild` (verdadero sólo si la cuenta no es miembro) e `invite_code` si lo hay; guarda `version`, `description` y `form_fields` | `fetchVerificationForm` | `GET …/member-verification?with_guild=false` (la cuenta ya es miembro: la invitación se aceptó antes) |
| Aceptarlas: `PUT /guilds/{g}/requests/@me` con `{version, form_fields}` (los campos tal como vinieron, con su `response`); 429 y 403 tienen mensaje propio; la respuesta es la solicitud con `application_status` | `submitVerificationForm` (`USER_GUILD_JOIN_REQUEST_UPDATE`) | `onboarding::join_request` devuelve cada campo con la copia que guardó al leerlo (`RulesField::echo`) y su `response`; `outcome` lee el estado como Serein (`APPROVED`/ninguno, `REJECTED`, el resto en revisión) |
| Responder las preguntas al entrar: `POST …/onboarding-responses` con `onboarding_responses` (las opciones elegidas de las preguntas del onboarding), `onboarding_prompts_seen` y `onboarding_responses_seen` (cada pregunta y opción mostrada con `Date.now()`); fuera del onboarding, para cambiarlas después, el mismo cuerpo por `PUT` | función que despacha `GUILD_ONBOARDING_UPDATE_RESPONSES_SUCCESS` | `onboarding::responses`, sólo el `POST` (cambiar las respuestas después queda pendiente) |
| Si hay onboarding: la función que lo arranca mira `GuildFeatures.GUILD_ONBOARDING` y la bandera del miembro `COMPLETED_ONBOARDING` | `GUILD_ONBOARDING_LOADED`, `getMember(...).flags` | `ServerOptions::onboarding` y `MemberRestrictions::onboarded` |
| Forma de una pregunta al guardarla: `{id, options: [{id, channel_ids, role_ids, emoji, emoji_id, emoji_name, emoji_animated, title, description}], title, single_select, disabled, required, in_onboarding, type}`; una nueva lleva `id: String(Date.now())` y `type: 0` | funciones que serializan y crean preguntas junto al store de onboarding | `onboarding::edit` y `Onboarding::fresh_id` (la hora, o uno más que la última hecha así en el mismo milisegundo; los snowflakes no cuentan) |
| El onboarding guardado trae `below_requirements`: los requisitos de canales los evalúa Discord | store de onboarding (`belowRequirements`) | RUNE pide localmente sólo un canal donde todos escriban; el resto lo dice el rechazo de Discord |

**Sin confirmar:** el `PUT /guilds/{g}/onboarding` de la página de ajustes
vive en un chunk diferido que no está entre los descargados; RUNE usa la
forma que documenta Discord (Modify Guild Onboarding) y la que el bundle
muestra para las preguntas. Tampoco se vio el cuerpo exacto de un campo
`TEXT_INPUT` o `MULTIPLE_CHOICE` respondido: se manda el texto y el índice
de la opción, como Serein. Ningún límite numérico (cantidad de preguntas,
opciones o campos, largos de los textos) apareció en el bundle descargado
(se buscó el 2026-10-18; la página de edición vive en un chunk que no está):
RUNE usa los de Serein y lee hasta 50 preguntas aunque guarde hasta 15.

## Cliente web oficial: silenciar hilos y tarjetas de foro (2026-10-18)

Rama `hilos-foros-y-escenarios`. Mismo método: el bundle
`web.4d4b2f75fdea5470.js` y los chunks diferidos ya descargados sin sesión
por los trabajos anteriores, leídos con `grep` y scripts propios (Python con
`-I`); nada ejecutado ni descargado de nuevo, ningún dato local del cliente
oficial, ninguna petición. Se reimplementa; no se copió código.
Uso en RUNE: [DISCORD_SESSION.md](DISCORD_SESSION.md#silenciar-hilos-tarjetas-de-foro-y-chat-de-escenarios-2026-10-18).

| Qué | Dónde | En RUNE |
| --- | --- | --- |
| Rutas: `THREAD_MEMBER_SETTINGS` = `/channels/{id}/thread-members/@me/settings`, `FORUM_POSTS` = `/channels/{id}/post-data`, `THREAD_SEARCH`, `ALL_ARCHIVED_THREADS`, `MY_ARCHIVED_THREADS` | tabla de rutas `Rsh` en `web.4d4b…js` | la de ajustes del miembro; `post-data` no se usa todavía |
| `setNotificationSettings(hilo, cambio)`: actualiza en el acto, se une al hilo si no lo seguía y manda `PATCH` a esa ruta con el cambio | módulo de acciones de hilos (`web.4d4b…js`) | RUNE sólo lo ofrece a quien sigue el hilo (el menú oficial lo deshabilita si no: `mute-disabled`) y espera la respuesta |
| Menú: «Silenciar» con duraciones (cada una arma `{muted: true, mute_config: {selected_time_window, end_time}}`), «Dejar de silenciar» con `{muted: false}` y la hora en que termina debajo | chunk `5ab9f1c421ba6b8f.js` (módulo 57907) y el selector de silencio de servidores en `046406f7a5feb3e2.js` (`selected_time_window` en segundos, `-1` para siempre; `end_time` ISO o `null`) | `Command::MuteThread`; las duraciones de RUNE (15 min a 24 h y para siempre) |
| `JoinedThreadsStore`: `muted` y `muteConfig` del miembro propio, de `READY`, `GUILD_CREATE`, `THREAD_CREATE`, `THREAD_LIST_SYNC`, páginas de archivados y búsqueda, `THREAD_MEMBER_UPDATE` (sólo el propio) y `THREAD_MEMBERS_UPDATE` (alta y baja); un temporizador lo apaga al vencer | `web.4d4b…js` (módulo 152007) | `Thread::muted_until` leído de los mismos lugares; vencido, deja de contar por la hora |
| Un hilo silenciado no marca no leídos (sí sus menciones) ni avisa salvo menciones | funciones de no leídos y de notificación de `web.4d4b…js` (`isMuted` de hilos junto a `isGuildOrCategoryOrChannelMuted` del padre) | RUNE lo trata como un canal silenciado propio: sin no leídos y sin avisos, menciones incluidas (desvío conocido, igual que los canales en RUNE) |
| Tarjeta de publicación: la reacción más dada (por `count + burst_count`); sin ninguna, la predeterminada del foro como botón sin número; con el primer mensaje desconocido, ninguna | chunk `d5f6d40503b27ff3.js` (hooks `lQ` y `Ck`) y `357c6dc3a08ebcb8.js` (pie de la tarjeta) | `PostStarter::reaction` |
| Primer mensaje de las publicaciones: `first_messages` de la búsqueda y, para las que faltan, `POST /channels/{foro}/post-data` `{thread_ids}` de a 10 | módulo de carga de publicaciones de `web.4d4b…js` | sólo `first_messages` (como Serein) y la respuesta al crear una publicación |

**Sin confirmar:** si el `PATCH` de ajustes del miembro responde el miembro
entero (RUNE no lo lee) y si Discord compara el `before` de los archivados
con microsegundos (RUNE pide un milisegundo después del instante).

## Cliente web oficial: mover canales y sincronizar permisos (2026-10-18)

Rama `admin-canales-y-voz`. Mismo método: el bundle público
`web.c5b338cc7e610833.js` (SHA-256 en la sección de tipos de embed) y el
chunk diferido `dde481f27ff23f57.js` (SHA-256 `4d3f8cfc…f9d4bdf`), ya
descargados sin sesión por los trabajos anteriores, leídos con `grep` y
Python con `-I`; nada ejecutado ni descargado de nuevo, ningún dato local
del cliente oficial. Se reimplementa; no se copió código, texto ni recurso.

| Qué | Evidencia | En RUNE |
| --- | --- | --- |
| Mover canales arrastrando es un solo `PATCH /guilds/{g}/channels` (`batchChannelUpdate`) con las posiciones que cambian; un error muestra un aviso | chunk `dde481…`, función que llama a `batchChannelUpdate` y su `catch` | la barra lateral manda `Command::ReorderChannels` como la página «Canales» |
| Al pasar un canal a otra categoría pregunta si sincroniza permisos sólo si la cuenta tiene `MANAGE_ROLES` en el canal y en la categoría nueva, el canal no está ya sincronizado con ella y estaba sincronizado con la vieja (o no tenía); confirmar pone `lock_permissions: true` en el canal movido, cancelar mueve igual sin eso. Mover a una categoría que no ve o donde no tiene `MANAGE_CHANNELS` no se hace | mismo chunk: la función con `v.lock_permissions=!0` y `openModalLazy` | `channel_list::offers_sync` y el diálogo «¿Sincronizar permisos?» con «Mover sin sincronizar»; Esc o «Cancelar» no mueven (el oficial mueve sin sincronizar) |
| Las posiciones se renumeran 0…n-1 sobre la lista ordenada, y va cada canal cuya posición difiere del índice (`calculatePositionDeltas`) | módulo 111613 (`DragAndDropUtils`) | `rest::layout_moves` también numera desde 0, sólo los grupos cuyo orden cambia |
| La lista que se ordena (`GuildChannelStore`) deja afuera los canales sin `VIEW_CHANNEL`: el oficial nunca manda los que la cuenta no ve, y un oculto puede quedar empatado con uno visible | módulo 808728 (`T.A.can(VIEW_CHANNEL, r)` al armar `SELECTABLE`/`VOCAL`) | RUNE tampoco los manda, pero los conoce (`channel_tree_with_hidden`) y numera los visibles alrededor de su posición |

**Sin confirmar:** el texto del diálogo oficial (vive en el chunk
`687634`, no descargado) y si Discord acepta en el lote un canal que la
cuenta no ve.

## Cliente web oficial: identidad de navegador (2026-10-19)

Para la [identidad de navegador](SECURITY.md#identidad-de-navegador-autorizada-el-2026-10-19)
se releyeron, sin ejecutar ni bajar nada de nuevo, la página pública
`https://discord.com/app` y el bundle principal que ya habían bajado sin
cookies ni sesión los trabajos anteriores (build 641004):

| Archivo | SHA-256 |
| --- | --- |
| `app.html` (`/app`) | `1b10e05a9e488203f07bd15c89a88d78e8ddb79fe9d0b9c4e0c62772faf56d17` |
| `web.4d4b2f75fdea5470.js` | `667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae` |

Hallazgos (desplazamientos en bytes dentro de `web.…`, salvo la página):

- **Número de build.** La página trae `window.GLOBAL_ENV = {…,
  "BUILD_NUMBER":"641004", …}` (línea 28 de `app.html`); el bundle lo usa
  como `client_build_number` (`parseInt("641004",10)`, ~10 000 441).
- **Super properties** (módulo 562708, ~10 000 237): `os`, `browser`,
  `device`, `device_model_name: ""`, `system_locale`
  (`navigator.languages[0]`), `has_client_mods`; después
  `browser_user_agent` (`navigator.userAgent`), `browser_version`,
  `os_version`; los `referrer`/`referring_domain` de la primera visita y los
  `…_current` de la sesión; luego `release_channel`, `client_build_number`,
  `client_event_source: null` y `client_launch_id` (un UUID por carga).
  `launch_signature` lo agrega el módulo 174459 sólo si el WebAssembly
  `libdiscore` ya cargó (`generateLaunchSignature(globalThis)`); si no, `null`.
  `client_heartbeat_session_id` y `client_app_state`
  (`focused`/`unfocused`) se suman antes de cada pedido (módulo 618651).
  La cabecera es base64 del JSON (`encodeProperties`).
- **Navegador.** `getBrowser` (~9 997 974) devuelve `Safari` si
  `/Apple/.test(navigator.vendor)`, antes de mirar `Chrome` en el user
  agent: en WebKitGTK el cliente web se nombra Safari aunque el user agent
  diga otra cosa. `getOS` da `Linux` para un user agent con `Linux`.
- **Cabeceras de cada pedido** (`prepareRequest`, ~1 001 775): `Authorization`,
  `X-Super-Properties`, `X-Fingerprint` si lo hay, `X-Installation-ID` si lo
  hay, `Accept-Language` sólo en la aplicación de escritorio (en un
  navegador lo pone el navegador), `X-Discord-Locale` (el idioma de la
  interfaz) y `X-Discord-Timezone` (`Intl…resolvedOptions().timeZone`,
  módulo 472229).
- **`X-Fingerprint`.** Viene de la respuesta de `/experiments` sin sesión;
  `LOGIN_SUCCESS`, `REGISTER_SUCCESS` y `UPDATE_TOKEN` lo borran (`et()`,
  ~8 980 096): con sesión no se manda.
- **Idioma de la interfaz** (módulos 773669 y 226536): el que eligió la
  cuenta; si no, el del navegador llevado a la lista de Discord (igual, su
  idioma, `es`→`es-ES`, `nb`/`nn`→`no`, `zh-Hant`→`zh-TW`, o el primero del
  mismo idioma en el orden de la lista, que pone `es-419` antes de `es-ES`).
- **Identify** (~4 907 417): `properties` son las super properties más
  `client_app_state`, `is_fast_connect: false`, `gateway_connect_reasons`
  (`COLD_START` mientras no hubo otro motivo) e `installation_id` si lo hay.
  Manda además `capabilities`, `client_state` y `qos_token`, que RUNE no
  manda (cambian la forma de las cargas).
- **Aceptar una invitación** (módulo 972387, ~2 621 684): el modal de
  invitación llama a `getInviteContext("Desktop Invite Modal", invite)` =
  `{location, location_guild_id, location_channel_id,
  location_channel_type}` y `acceptInvite` le suma
  `invite_guild_scheduled_event_id` (ausente si no hay evento); el
  transporte lo manda como `X-Context-Properties` (sección del
  2026-10-16).

Uso en RUNE: `rune-discord-session::browser` arma esas piezas y
`http::api` las pone en cada pedido; detalle en
[DISCORD_SESSION.md](DISCORD_SESSION.md#identidad-de-navegador-2026-10-19-rama-identidad-de-navegador).
Se reimplementa; no se copió código. Nada de esto se probó contra el
servicio real.

## Cliente web oficial: identidad en WebView2 (2026-10-19, rama `windows-login`)

Para que en Windows la identidad salga del motor de las ventanas (WebView2,
Chromium) se releyeron, sin ejecutar ni bajar nada de nuevo, el mismo
bundle principal (`web.4d4b2f75fdea5470.js`, SHA-256
`667a771c…084ae`) y el chunk donde vive platform.js
(`40715.ce9980776791e583.js`, SHA-256
`901d8ddd43e81f9e4e42483f1a416a7d59ad4baf511022db720cc44740f01a07`), ya
bajados sin cookies ni sesión por los trabajos anteriores:

- **`getBrowser`** (`eA`, ~9 997 974 en `web.…`): antes de mirar el user
  agent recorre `navigator.userAgentData.brands` y devuelve `Edge` si una
  marca es exactamente `Microsoft Edge` (`Opera` si es `Opera`); el orden es
  Opera de Presto, BlackBerry, iOS y Android de Edge/Chrome/Firefox,
  **`/Apple/` en `navigator.vendor` → Safari**, esas marcas, Android, `Edge`
  en el user agent, `Chrome`, Konqueror, Firefox, IE y Gecko. Un user agent
  de Edge moderno dice `Edg/`, no `Edge`: sin la marca `Microsoft Edge` el
  cliente web lo nombra `Chrome`. Por eso RUNE no adivina las marcas de
  WebView2: las lee la página de prueba.
- **`getOS`** (`e_`, ~9 997 319): `Windows` si el user agent dice `Windows`
  (`Windows Mobile` con `Phone`), antes que iOS, Android, Mac y Linux.
- **`browser_version` y `os_version`** (~10 000 635): `et().version` y
  `et().os.version`, donde `et` es el export de platform.js 1.3.6 (módulo
  214958 del chunk `40715.…`, ~25 711). platform.js nombra `Microsoft Edge`
  con `(?:Edge|Edg|EdgA|EdgiOS)`, antes que Chrome, y saca la versión del
  primer token de esa familia, si no de `Version/`, si no del nombre
  (`Chrome/…`); para Windows lleva `NT 10.0` a `10` (también Windows 11, que
  sigue diciendo `NT 10.0`).
- **`system_locale`**: `navigator.languages[0]`; `X-Discord-Timezone`:
  `Intl.DateTimeFormat().resolvedOptions().timeZone` (sección anterior).

Uso en RUNE: `rune-discord-session::browser` (`Engine`) deriva esas piezas
de lo que la página de prueba de WebView2 lee, y Linux las deriva igual de
WebKitGTK; detalle en
[DISCORD_SESSION.md](DISCORD_SESSION.md#identidad-por-plataforma-2026-10-19-rama-windows-login).
Se reimplementa; no se copió código. Nada se probó contra el servicio real
ni en un Windows real.

## Cliente web oficial: por qué se rechaza una invitación (2026-10-19, rama `invitaciones-motivo`)

Misma autorización y método: el bundle público ya descargado sin sesión
(`web.4d4b2f75fdea5470.js`, SHA-256
`667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae`) y su
catálogo de textos en inglés (`933ace571a9c6560.js`, SHA-256
`d9039967920acd48ce3a2b3409e14b63724f49cce42484e63d351f6f264632fc`),
leídos con `grep` y Python con `-I`; nada ejecutado, ningún dato local del
cliente oficial, ninguna petición. Desplazamientos en bytes:

- **`AbortCodes`** (~319 708 en adelante): `UNKNOWN_INVITE` 10006,
  `UNDER_MINIMUM_AGE` 20024, `AGE_GROUP_UNVERIFIED` 20086,
  `TOO_MANY_USER_GUILDS` 30001, `GUILD_AT_CAPACITY` 30019, `USER_BANNED`
  40007 (~322 983), `USER_QUARANTINED` 40068, `INVITES_DISABLED` 40069,
  `INVALID_COUNTRY_CODE` 50095, `GUILD_JOIN_INVITE_LIMITED_ACCESS` 400002
  (dentro de `GUILD_LIMITED_ACCESS_*`, 400000–409999),
  `USER_GUILD_JOIN_LARGE_GUILD_UNDERAGE_DISALLOWED` 700000 y
  `USER_GUILD_JOIN_AGE_RESTRICTED_IOS_DISALLOWED` 700002.
- **Textos del error al unirse** (módulo 4274, ~9 745 092): una función da
  título y texto por código y otra el texto suelto: 30001 «You are at the
  {100|200} server limit» / «You must leave a server first to join», 30019
  «This server is currently full», 400002 «This server's features are
  limited», 50095 «This server is not available in your location», 40069
  «Invites are currently paused for this server», 700000, 20024 y 20086 por
  la edad («You can't join this server because of your age group», «Confirm
  your age to join this server»); cualquier otro, «Unable to accept invite».
  `acceptInvite` (~634 996) sólo guarda `code` y `message` del cuerpo al
  fallar.

Uso en RUNE: `refused` en `rune-discord-session/src/http/invitations.rs`
da a cada código su causa (`InvitationJoinError::ServerLimit`, `Banned`,
`ServerFull`, `InvitesPaused`, `Region`, `AgeRestricted`, `LimitedAccess`,
`AccountLimited`; 10006 es `Invalid`), con textos propios en español e
inglés ([INVITATIONS.md](INVITATIONS.md#motivo-del-rechazo-y-salida-por-el-navegador-2026-10-19)).
40007 y 40068 no tienen texto en ese módulo; se nombran por su
`AbortCode`. No se copió código ni texto. Nada de esto se probó contra el
servicio real.

## Cliente web oficial: eventos programados (2026-10-19, rama `eventos-programados`)

Misma autorización y método: el bundle público ya descargado sin sesión
(`web.4d4b2f75fdea5470.js`, build 641004, SHA-256
`667a771c61ff709e2f0400d395a324f0a5b1b5aa89e11e263289144a12d084ae`), leído
con Python con `-I`; nada ejecutado, ningún dato local del cliente oficial,
ninguna petición. Serein no tiene eventos programados. Se reimplementa; no
se copió código. Desplazamientos en bytes:

- **Rutas** (tabla `Endpoints`, ~295 133): `GUILD_EVENTS_FOR_GUILD` =
  `/guilds/{g}/scheduled-events`; `GUILD_EVENT` =
  `/guilds/{g}/scheduled-events/{e}`; `GUILD_EVENT_USERS` =
  `…/{e}[/{excepción}]/users`; `GUILD_EVENT_USER_COUNTS` =
  `…/{e}/users/counts`; `USER_GUILD_EVENT` = `…/{e}[/{excepción}]/users/@me`;
  `USER_GUILD_EVENTS` = `/users/@me/scheduled-events`.
- **Escrituras** (módulo 496092, ~5 424 543–5 426 929): crear es `POST` a
  la lista con `{name, description, image, privacy_level,
  scheduled_start_time, scheduled_end_time, entity_type, channel_id,
  entity_metadata, recurrence_rule}`; editar, `PATCH` del evento con lo
  mismo (la imagen sólo si es un `data:` nuevo, el canal sólo para
  escenario y voz, los metadatos sólo para otro lugar); iniciar, terminar
  y cancelar, `PATCH` con `{status: 2}`, `{status: 3}` y `{status: 4}`;
  borrar, `DELETE`.
- **Interés** (~5 427 446): `createRsvpForGuildEvent` hace `PUT` a
  `USER_GUILD_EVENT` con `{response}`; `deleteRsvpForGuildEvent`, `DELETE`
  a la misma ruta. `response`: `UNINTERESTED` 0, `INTERESTED` 1
  (~5 430 833). Lo que le interesa a la cuenta:
  `GET /users/@me/scheduled-events` con `guild_ids`; quiénes:
  `GET …/users` con `limit` y `with_member=true` (el cliente lee
  `body.users`; la documentación de Discord dice que es un arreglo: RUNE
  acepta las dos formas).
- **Valores** (~5 430 261): estado `SCHEDULED` 1, `ACTIVE` 2, `COMPLETED`
  3, `CANCELED` 4; lugar `STAGE_INSTANCE` 1, `VOICE` 2, `EXTERNAL` 3
  (`NONE` 0 y `PRIME_TIME` 4 no tienen forma en RUNE y no se muestran);
  privacidad `PUBLIC` 1 y `GUILD_ONLY` 2.
- **Gateway** (store, ~5 421 874): `GUILD_CREATE` trae
  `guild_scheduled_events`; despacha `GUILD_SCHEDULED_EVENT_CREATE`,
  `UPDATE`, `DELETE`, `USER_ADD` y `USER_REMOVE` (estos dos con
  `guild_scheduled_event_exception_id` para una sola ocurrencia).
- **Invitaciones** (módulo 372250, ~5 762 068): la clave de una invitación
  lleva `?event={id}`; al consultarla (~5 764 627) se pide
  `guild_scheduled_event_id` además de `with_counts`, y la respuesta trae
  `guild_scheduled_event`. Aceptarla manda `invite_guild_scheduled_event_id`
  en `X-Context-Properties` (~635 410): desde que la sesión presenta el
  contexto del modal del cliente web (decisión del dueño del 2026-10-19),
  RUNE lo suma cuando el enlace nombra un evento.

Uso en RUNE: [DISCORD_SESSION.md](DISCORD_SESSION.md#eventos-programados-2026-10-19-rama-eventos-programados).
RUNE no lee los eventos del `GUILD_CREATE` (pide la lista cuando muestra
el servidor), no usa `users/counts` (lee `user_count` con
`with_user_count=true`, documentado), ni repeticiones ni excepciones, ni
pone portada al crear o editar. Nada se probó contra el servicio real.

## Cliente web oficial: AutoMod (2026-10-19)

Rama `automod`. Misma autorización y método: el bundle público
`web.c5b338cc7e610833.js` (SHA-256 en la sección de tipos de embed), ya
descargado sin sesión por trabajos anteriores, leído con `grep` y Python
con `-I`; nada ejecutado ni descargado de nuevo, ningún dato local del
cliente oficial, ninguna petición. Se reimplementa; no se copió código,
texto ni recurso. Uso en RUNE:
[DISCORD_SESSION.md](DISCORD_SESSION.md#automod-2026-10-19-rama-automod).

| Qué | Evidencia | En RUNE |
| --- | --- | --- |
| Rutas: `GUILD_AUTOMOD_RULES` = `/guilds/{g}/auto-moderation/rules`, `GUILD_AUTOMOD_RULE` = `…/rules/{id}`, `GUILD_AUTOMOD_VALIDATE_RULE` = `…/rules/validate`, más `clear-mention-raid`, `alert-action`, `false-alarm` | tabla de rutas `Rsh` | sólo las dos primeras; validar se hace localmente con `AutoModRule::problem` |
| Leer, crear, editar y borrar: `GET` de la lista, `POST` (el cuerpo sin `id`), `PATCH` a la regla con el cuerpo entero, `DELETE` | funciones junto al serializador de reglas (las que llaman a esas rutas) | lo mismo, pero el `PATCH` sin `trigger_type` (como lo documenta Discord) y activar o desactivar con sólo `{enabled}` |
| Cuerpo: `id`, `name`, `guild_id`, `event_type`, `trigger_type`, `trigger_metadata` (sin `keyword_lists`), `actions` (`{type, metadata}`), `enabled`, `creator_id`, `position`, `exempt_channels`, `exempt_roles` | la función que pasa la regla a `snake_case` | RUNE manda sólo lo documentado: sin `guild_id`, `creator_id` ni `position` |
| Tipos: acciones 1 `BLOCK_MESSAGE`, 2 `FLAG_TO_CHANNEL`, 3 `USER_COMMUNICATION_DISABLED`, 4 `QUARANTINE_USER`; eventos 1 `MESSAGE_SEND`, 2 `GUILD_MEMBER_JOIN_OR_UPDATE`; disparadores 1 `KEYWORD`, 2 `SPAM_LINK`, 3 `ML_SPAM`, 4 `DEFAULT_KEYWORD_LIST`, 5 `MENTION_SPAM`, 6 `USER_PROFILE`, 7 `SERVER_POLICY`, 8 `APPLICATION`; listas 1 `PROFANITY`, 2 `SEXUAL_CONTENT`, 3 `SLURS` | el módulo de constantes de AutoMod | `AutoModTrigger` modela 1, 3, 4, 5 y 6 (los documentados); el resto queda como desconocido, visible pero no editable |
| Constantes junto a esos tipos, sin nombre en el bundle: 5, 1000, 100, 1000, 10, 60, 1, 1, 260, 100, 1, 50, 20 y 300, y un ejemplo de expresiones regulares | mismo módulo | los límites de `rune_model::automod`, tomados de la documentación de Discord, coinciden con 1000, 60, 10, 260, 100, 50 y 20 |
| Al crear una alerta, el canal elegido se agrega a los canales exentos | el modal de la acción `FLAG_TO_CHANNEL` (`exemptChannels.add`) | RUNE no lo hace solo: la persona lo elige (desvío conocido) |
| Envío rechazado: códigos `AUTOMOD_MESSAGE_BLOCKED` = 200000, `AUTOMOD_TITLE_BLOCKED` = 200001, `AUTOMOD_INVALID_RUST_SERVICE_RESPONSE` = 200002 → `MESSAGE_SEND_FAILED_AUTOMOD` (y `MESSAGE_EDIT_FAILED_AUTOMOD`) con `{code, message}` de la respuesta | `AbortCodes` y la función de envío de mensajes | `rejections::Blocked` y `Event::AutoModBlocked`; `rune-core` lo hace el `Event::Failed` de siempre |
| El aviso que se muestra es el `message` de la respuesta si viene; si no, un texto propio según sea hilo, foro o edición | módulo 753738 (`PD`, `cw`) y `GuildAutomodMessageStore` | RUNE muestra el `message` (acotado) o su propio «la moderación automática del servidor lo bloqueó» |
| Gateway: el bundle no maneja `AUTO_MODERATION_RULE_*` ni `AUTO_MODERATION_ACTION_EXECUTION`; los números 140 a 146 son tipos del registro de auditoría | búsqueda en el bundle | RUNE los lee como Discord los documenta (sin confirmar que lleguen a una sesión de usuario) |

**Sin confirmar:** la página de ajustes de AutoMod y los límites de sus
campos (150 caracteres del mensaje propio, 6 reglas de palabras) viven en
chunks diferidos que no están entre los descargados; RUNE usa los de la
documentación de Discord.
