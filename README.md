<p align="center">
  <img src="assets/icons/hicolor/256x256/apps/rune.png" alt="RUNE" width="128">
</p>

<h1 align="center">RUNE</h1>

> [!WARNING]
> **RUNE es un cliente no oficial.** No está afiliado a Discord ni aprobado
> por Discord. Su proveedor de **sesión personal** usa tu cuenta de usuario
> fuera de la API documentada: eso va contra los
> [Términos de Servicio de Discord](https://discord.com/terms) y puede
> costarte la cuenta (suspensión o cierre). Usalo bajo tu responsabilidad,
> con una cuenta que aceptes perder. El proveedor **OAuth2
> oficial** sólo autoriza tu identidad y la lista de tus servidores
> (`identify guilds`): no da mensajes ni llamadas. Sin cuenta, RUNE abre una
> **simulación local**, marcada como tal en la interfaz.

**Rust UI, No Electron.** Un cliente de chat de escritorio nativo, escrito
en Rust, con una interfaz propia construida con
[limn](https://github.com/Niiihuel/limn). Pinta con Vulkan a través de wgpu,
sin Electron ni motor web; sólo el inicio de sesión y el hCaptcha que
resuelve la persona usan una WebView temporal, en otro proceso.

Chat con servidores, hilos y foros, archivos, voz y video (Opus, DAVE,
pantalla y cámara), amigos y grupos, perfiles, administración, bandeja y
atajos globales, en español e inglés.

![RUNE con la simulación](docs/img/presence-identity-2026-10-17.png)

## Plataformas

Linux y Windows (x86_64). macOS, más adelante.

## Instalar

Las [releases](https://github.com/Niiihuel/rune/releases) traen
`rune-x86_64-linux.gz` (se instala por usuario con `./rune --install`) y
`rune-x86_64-windows-setup.exe`. Después RUNE se actualiza solo, con un
manifiesto firmado ([docs/UPDATES.md](docs/UPDATES.md)).

## Compilar y ejecutar

Requiere Rust 1.95 o más nuevo y, en Linux, Nix para las bibliotecas del
sistema:

```sh
nix-shell                     # o: nix develop
cargo run -p rune-desktop     # abre la ventana con la simulación
cargo test --workspace
```

| Proveedor | Cómo |
| --- | --- |
| Simulación local | `cargo run -p rune-desktop` (o `-- --provider mock`) |
| OAuth2 oficial | `cargo run -p rune-desktop -- --provider discord --discord-client-id TU_APPLICATION_ID` |
| Sesión personal no oficial | `nix-shell --arg withLogin true` y `cargo run -p rune-desktop --features session-login -- --provider discord-session` |

Más órdenes (capturas, benchmark, Windows desde Linux) en
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Lo que RUNE escribe en
stderr queda también en `~/.local/state/rune/rune.log`.

## Documentación

| Documento | Contenido |
| --- | --- |
| [AGENTS.md](AGENTS.md) | Reglas del proyecto y autorizaciones del dueño |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, flujo de datos, contratos, idiomas, medios, rendimiento |
| [docs/SECURITY.md](docs/SECURITY.md) | Autorizaciones, credenciales, datos locales, superficies, licencias |
| [docs/DISCORD.md](docs/DISCORD.md) | Los tres proveedores, login y captcha, fuentes de la REA |
| [docs/UPDATES.md](docs/UPDATES.md) | Releases y actualizaciones |
| [docs/LIMN_INTEGRATION.md](docs/LIMN_INTEGRATION.md) | Cómo se agrega a limn lo que falta |
| [tasks/todo.md](tasks/todo.md) | Lo pendiente |

## Licencia

MIT o Apache-2.0, a elección ([LICENSE-MIT](LICENSE-MIT),
[LICENSE-APACHE](LICENSE-APACHE)). La marca, los iconos y los colores de
RUNE son propios. Twemoji (CC BY 4.0), las fuentes y los sonidos
predeterminados, que son efectos públicos de Discord, tienen su procedencia
en [assets/licenses](assets/licenses) y
[docs/SECURITY.md](docs/SECURITY.md#procedencia-de-lo-que-rune-trae); los
avisos de las dependencias, en
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
