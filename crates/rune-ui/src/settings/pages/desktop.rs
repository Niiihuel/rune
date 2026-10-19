//! The settings of the desktop work (P6): the tray, starting with the
//! session, the interface's font, the panels, hidden channels, the activity
//! of this device and the call's encryption. The global shortcuts of the
//! microphone and the sound are in the list of shortcuts (`shortcuts.rs`).

use limn::{Component, Element};

use rune_components::user_settings as parts;
use rune_i18n::{es, t, tr};

use super::{select, Ctx, Section};
use crate::settings::SettingsPage::{Appearance, Connection, System, Voice};

/// Whether a section of these can show: the call's encryption during a call.
pub(super) fn available(key: &str, store: &rune_state::Store) -> bool {
    key != "e2ee" || store.call().is_some()
}

/// Their sections, after each page's own.
pub(super) const SECTIONS: &[Section] = &[
    Section {
        page: Voice,
        key: "e2ee",
        title: es("Cifrado de la llamada"),
        rows: &[("privacy-code", es("código de privacidad cifrado de extremo a extremo dave verificar llamada seguridad"))],
    },
    Section {
        page: Appearance,
        key: "font",
        title: es("Fuente"),
        rows: &[("ui-font", es("fuente letra tipografía familia del sistema interfaz texto"))],
    },
    Section {
        page: Appearance,
        key: "panels",
        title: es("Paneles y canales"),
        rows: &[
            ("panel-widths", es("ancho de los paneles lista de canales miembros arrastrar restablecer")),
            ("hidden-channels", es("mostrar canales ocultos sin acceso privados ver favoritos fijados")),
        ],
    },
    Section {
        page: System,
        key: "desktop",
        title: es("Escritorio"),
        rows: &[
            ("tray", es("bandeja del sistema cerrar a la bandeja minimizar segundo plano ícono")),
            ("autostart", es("iniciar con la sesión al encender arrancar automáticamente inicio")),
            // Desktop session and games (escritorio-sesion-y-juegos).
            ("autostart-minimized", es("iniciar minimizado en la bandeja segundo plano sin ventana")),
        ],
    },
    Section {
        page: System,
        key: "activity",
        title: es("Actividad"),
        rows: &[
            ("rich-presence", es("rich presence actividad de otros programas juegos reproductores socket local discord ipc")),
            ("game-activity", es("detectar juegos procesos jugando actividad")),
            ("activity-now", es("actividad actual publicada ahora")),
            // Desktop session and games (escritorio-sesion-y-juegos).
            ("activity-sharing", es("compartir tu actividad mostrar el juego a los demás visible oculta cuenta")),
        ],
    },
    // Desktop session and games (escritorio-sesion-y-juegos).
    Section {
        page: System,
        key: "games",
        title: es("Juegos registrados"),
        rows: &[
            ("games-now", es("juego actual detectado ahora no es este juego renombrar")),
            ("games-add", es("agregar un programa como juego no ves tu juego")),
            ("games-list", es("juegos agregados detectados ocultos renombrar quitar restaurar última vez jugado")),
        ],
    },
    Section {
        page: Connection,
        key: "proxy",
        title: es("Proxy"),
        rows: &[("proxy", es("proxy http https api rest red intermediario dirección sin proxy"))],
    },
    // Linux desktop (escritorio-linux).
    Section {
        page: System,
        key: "window",
        title: es("Ventana"),
        rows: &[(
            "decorations",
            es("barra de título decoraciones de la ventana bordes del sistema sin marco minimizar maximizar cerrar"),
        )],
    },
    Section {
        page: System,
        key: "graphics",
        title: es("Gráficos"),
        rows: &[(
            "gpu",
            es("dibujar con gpu placa de video gráficos integrada dedicada aceleración por hardware"),
        )],
    },
];

/// The setting `key` of these sections; `None` for the rest.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Option<Element>> {
    let (ui, store, look) = (ctx.ui, ctx.store, ctx.look);
    let prefs = &ui.prefs;
    let width = ctx.geometry.column;
    let toggle = |id: &str, label: &str, note: &str, on: bool, denied: Option<&str>| {
        parts::toggle(id, label, Some(note), on, denied, look)
    };
    let capable = |check: fn(&rune_provider::Capabilities) -> bool| {
        store
            .provider
            .as_ref()
            .is_some_and(|info| check(&info.capabilities))
    };
    Some(Some(match key {
        "privacy-code" => {
            if !capable(|can| can.voice_privacy_code) {
                return Some(Some(parts::setting(
                    t("Código de privacidad"),
                    Some(t("Este proveedor no cifra las llamadas de extremo a extremo.")),
                    None,
                    false,
                    look,
                )));
            }
            match crate::desktop::privacy(ui, store, look) {
                Some(privacy) => privacy,
                None => parts::setting(
                    t("Código de privacidad"),
                    Some(t("Durante una llamada cifrada aparece acá y en Detalles de voz, con el código para verificar a cada persona.")),
                    None,
                    true,
                    look,
                ),
            }
        }
        "ui-font" => {
            let fonts = ui.desktop.fonts.as_deref().unwrap_or_default();
            let chosen = prefs.ui_font.as_deref();
            let mut options: Vec<(String, String, bool, bool)> =
                vec![("us:font-default".into(), t("Ubuntu (la de RUNE)").into(), chosen.is_none(), true)];
            options.extend(fonts.iter().enumerate().map(|(at, family)| {
                (format!("us:font={at}"), family.clone(), chosen == Some(family.as_str()), true)
            }));
            if let Some(family) = chosen.filter(|family| !fonts.iter().any(|known| known == family)) {
                options.push(("us:select:font".into(), family.to_owned(), true, false));
            }
            if ui.desktop.fonts.is_none() && ui.settings_state.select == Some("font") {
                options.push(("us:select:font".into(), t("Buscando las fuentes del sistema…").into(), false, false));
            }
            let borrowed: Vec<(&str, &str, bool, bool)> = options
                .iter()
                .map(|(id, label, chosen, enabled)| (id.as_str(), label.as_str(), *chosen, *enabled))
                .collect();
            let note = ui.desktop.font_problem.as_deref().map_or_else(|| {
                windows(es("La del texto de toda la ventana. En Windows RUNE todavía no lista las fuentes del sistema: usa la suya."))
                    .unwrap_or(t("La del texto de toda la ventana. Las fuentes del sistema se leen con fontconfig; si una se desinstala, RUNE vuelve a la suya."))
            }, t);
            Element::column(vec![
                parts::setting(t("Fuente de la interfaz"), Some(note), None, true, look),
                select(ctx, "font", t("Fuente de la interfaz"), &borrowed, width),
                Element::spacer().size(None, Some(12.0)),
            ])
        }
        "panel-widths" => parts::setting(
            t("Ancho de los paneles"),
            Some(&tr!(
                "Arrastrá el borde de la lista de canales o de la de miembros. Ahora: {channels} y {members} px.",
                channels = format!("{:.0}", prefs.sidebar_width),
                members = format!("{:.0}", prefs.members_width),
            )),
            Some(parts::secondary_button("us:widths-reset", t("Restablecer"), true, look)),
            true,
            look,
        ),
        "hidden-channels" => toggle(
            "us:hidden-channels",
            t("Mostrar los canales ocultos"),
            t("Lista también los canales que no podés ver, atenuados y sin abrirse. Los favoritos se marcan desde el menú de cada canal, y los mensajes directos se fijan desde el suyo."),
            prefs.show_hidden,
            None,
        ),
        "tray" => {
            let denied = match ui.desktop.tray {
                Some(false) => Some(windows(es("El área de notificación de Windows no está disponible: cerrar sale de RUNE.")).unwrap_or(t("El escritorio no tiene una bandeja del sistema (StatusNotifierItem): cerrar sale de RUNE."))),
                _ => None,
            };
            toggle(
                "us:tray-close",
                t("Cerrar a la bandeja"),
                t("Cerrar la ventana deja a RUNE en la bandeja del sistema, con su menú: abrir, silenciar, ensordecer, estado y salir."),
                prefs.close_to_tray,
                denied,
            )
        }
        "autostart" => {
            let note = ui.desktop.autostart_problem.as_deref().map_or_else(|| {
                windows(es("Agrega RUNE a las aplicaciones que Windows abre al iniciar sesión (Configuración › Aplicaciones › Inicio)."))
                    .unwrap_or(t("Agrega RUNE a los programas que abre tu sesión (~/.config/autostart)."))
            }, t);
            match ui.desktop.autostart {
                Some(on) => toggle("us:autostart", t("Iniciar con la sesión"), note, on, None),
                None => parts::toggle(
                    "us:autostart",
                    t("Iniciar con la sesión"),
                    None,
                    false,
                    Some(t("Leyendo…")),
                    look,
                ),
            }
        }
        "rich-presence" => {
            let denied = windows(es("En Windows RUNE todavía no abre el canal local discord-ipc."))
                .or((!capable(|can| can.local_activity)).then_some(t("Este proveedor no publica actividades.")));
            let note = match (&ui.desktop.rich_presence_problem, prefs.rich_presence) {
                (Some(problem), true) => t(problem),
                _ => t("Otros programas de este equipo (juegos, reproductores) pueden decir qué estás haciendo por el socket local discord-ipc, como con el cliente oficial; RUNE lo publica como tu actividad. Sólo los programas de tu usuario, sin puertos de red."),
            };
            toggle(
                "us:rich-presence",
                t("Actividad de otros programas"),
                note,
                prefs.rich_presence,
                denied,
            )
        }
        "game-activity" => {
            let denied = windows(es("En Windows RUNE todavía no detecta juegos."))
                .or((!capable(|can| can.local_activity && can.detectable_games)).then_some(t("Este proveedor no reconoce juegos.")));
            toggle(
                "us:game-activity",
                t("Detectar juegos"),
                t("Cada 10 s RUNE mira el nombre de tus programas abiertos (nada más) y, si es un juego que el proveedor reconoce o que agregaste, lo muestra como «Jugando a…»."),
                prefs.game_activity,
                denied,
            )
        }
        "activity-now" => {
            let now = match &ui.desktop.activity {
                Some(activity) => {
                    let name = activity.name.clone().unwrap_or_else(|| match activity.application {
                        Some(application) => tr!("la aplicación {application}", application),
                        None => t("una aplicación").into(),
                    });
                    match activity.source {
                        rune_model::ActivitySource::Game => tr!("Jugando a {name}, detectado en este equipo.", name),
                        rune_model::ActivitySource::Program => tr!("{name} dice lo que estás haciendo.", name),
                    }
                }
                None if prefs.rich_presence || prefs.game_activity => t("Ninguna por ahora.").into(),
                None => return Some(None),
            };
            // Desktop session and games (escritorio-sesion-y-juegos).
            let now = match seen(store) {
                Some(seen) => format!("{now} {seen}"),
                None => now,
            };
            parts::setting(t("Ahora"), Some(&now), None, true, look)
        }
        // Desktop session and games (escritorio-sesion-y-juegos).
        "autostart-minimized" => {
            let denied = match ui.desktop.autostart {
                None => Some(t("Leyendo…")),
                Some(false) => Some(t("Primero activá «Iniciar con la sesión».")),
                Some(true) => None,
            };
            let note = match ui.desktop.tray {
                Some(false) => t("Sin bandeja del sistema, RUNE abre su ventana igual."),
                _ => t("Al iniciar la sesión, RUNE arranca en la bandeja sin abrir la ventana: abrila desde su ícono."),
            };
            toggle(
                "us:autostart-minimized",
                t("Iniciar minimizado"),
                note,
                ui.desktop.autostart_minimized,
                denied,
            )
        }
        "activity-sharing" => return Some(activity_sharing(ctx)),
        "games-now" | "games-add" | "games-list" => return Some(super::games::setting(ctx, key)),
        "proxy" => proxy(ctx),
        // Linux desktop (escritorio-linux).
        // The window's own title bar (barra-e-icono).
        "decorations" => title_bar(ctx),
        "gpu" => graphics(ctx),
        _ => return None,
    }))
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// Whether others see the activity published, as the account's sessions
/// say it; `None` without one, or a provider that does not say.
fn seen(store: &rune_state::Store) -> Option<String> {
    use rune_model::ActivityVisibility::*;
    store
        .provider
        .as_ref()
        .filter(|info| info.capabilities.activity_sharing)?;
    store.local_activity()?;
    let seen = match store.activity_visibility() {
        Some(Listed) => t("Tu cuenta la muestra a los demás."),
        Some(Received) => t("La sesión la recibió; todavía no se sabe si tu cuenta la muestra."),
        Some(Hidden) => t("Tu cuenta la oculta: nadie la ve."),
        Some(Missing) => {
            t("Tu cuenta no la muestra: revisá tus juegos y la privacidad de cada servidor.")
        }
        Some(Unconfirmed) | None => t("Esperando que tu cuenta la confirme."),
    };
    Some(if crate::voice::simulated(store) {
        format!("{seen} {}", t("(simulado)"))
    } else {
        seen.to_owned()
    })
}

/// Whether the account shows its activity, and the button that turns it
/// on: never off from here, as Serein.
fn activity_sharing(ctx: &Ctx) -> Option<Element> {
    let (ui, store, look) = (ctx.ui, ctx.store, ctx.look);
    store
        .provider
        .as_ref()
        .filter(|info| info.capabilities.activity_sharing)?;
    let detecting = ui.prefs.game_activity || ui.prefs.rich_presence;
    let asked = store.activity_sharing_asked().is_some();
    let (note, off) = match store.activity_sharing() {
        None => (t("Leyendo el ajuste de tu cuenta…"), false),
        Some(true) => (
            t("Tu cuenta muestra tu actividad. La privacidad de cada servidor y de tus amigos sigue valiendo."),
            false,
        ),
        Some(false) if detecting => (
            t("Tu cuenta oculta tu actividad: aunque RUNE detecte un juego, nadie lo ve."),
            true,
        ),
        Some(false) => (t("Tu cuenta oculta tu actividad a los demás."), true),
    };
    let button = off.then(|| {
        parts::secondary_button(
            "us:activity-sharing-on",
            if asked {
                t("Activando…")
            } else {
                t("Activar en la cuenta")
            },
            !asked,
            look,
        )
    });
    Some(parts::setting(
        t("Compartir tu actividad"),
        Some(note),
        button,
        true,
        look,
    ))
}

/// How the providers' REST requests go: the environment's proxies, none,
/// or an address; it applies from the next start.
fn proxy(ctx: &Ctx) -> Element {
    use rune_provider::ApiProxy;
    let (ui, look) = (ctx.ui, ctx.look);
    let width = ctx.geometry.column;
    let writing = ui.desktop.proxy_draft.as_ref();
    let current = &ui.desktop.proxy;
    let options = [
        (
            "us:proxy=automatic",
            t("Automático (el del sistema)"),
            *current == ApiProxy::Automatic && writing.is_none(),
        ),
        (
            "us:proxy=direct",
            t("Sin proxy"),
            *current == ApiProxy::Direct && writing.is_none(),
        ),
        (
            "us:proxy=url",
            t("Esta dirección"),
            matches!(current, ApiProxy::Url(_)) || writing.is_some(),
        ),
    ];
    let borrowed: Vec<(&str, &str, bool, bool)> = options
        .iter()
        .map(|(id, label, chosen)| (*id, *label, *chosen, true))
        .collect();
    let mut note = t("Sólo las llamadas a la API (REST) de los proveedores; la conexión en vivo y la voz no pasan por él. Sin usuario ni contraseña: RUNE no guarda credenciales de un proxy. Se aplica la próxima vez que abras RUNE o cambies de proveedor.").to_owned();
    if let (ApiProxy::Url(url), None) = (current, writing) {
        note = format!("{note} {}", tr!("Ahora: {url}", url = url.as_str()));
    }
    let mut column = vec![
        parts::setting(t("Proxy para la API"), Some(&note), None, true, look),
        super::select(ctx, "proxy", t("Proxy para la API"), &borrowed, width),
    ];
    if let Some(draft) = writing {
        column.push(Element::spacer().size(None, Some(8.0)));
        column.push(
            Element::row(vec![
                limn::TextField::new(
                    crate::desktop::PROXY,
                    draft.clone(),
                    "http://127.0.0.1:8080",
                    look.palette(),
                )
                .accessibility_label(t("Dirección del proxy"))
                .render()
                .flex(1.0),
                parts::primary_button("us:proxy-save", t("Guardar"), true, look),
                parts::secondary_button("us:proxy-cancel", t("Cancelar"), true, look),
            ])
            .gap(8.0),
        );
        if let Some(problem) = ui.desktop.proxy_problem {
            column.push(rune_components::line(t(problem), 13.0, look.danger));
        }
    }
    column.push(Element::spacer().size(None, Some(12.0)));
    Element::column(column)
}

// Linux desktop (escritorio-linux).
/// «Dibujar con»: as RUNE chooses or each GPU found, from the next start,
/// with the one drawing now.
fn graphics(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let graphics = &ui.desktop.graphics;
    let chosen = graphics.chosen.as_deref();
    let mut options: Vec<(String, String, bool, bool)> = vec![(
        "us:gpu=auto".into(),
        t("Automático").into(),
        chosen.is_none(),
        true,
    )];
    options.extend(graphics.gpus.iter().map(|(key, name)| {
        (
            format!("us:gpu={key}"),
            name.clone(),
            chosen == Some(key.as_str()),
            true,
        )
    }));
    // One no longer in this machine stays shown, not chosen again.
    if let Some(key) = chosen.filter(|key| !graphics.gpus.iter().any(|(known, _)| known == key)) {
        options.push(("us:select:gpu".into(), key.to_owned(), true, false));
    }
    let borrowed: Vec<(&str, &str, bool, bool)> = options
        .iter()
        .map(|(id, label, chosen, enabled)| (id.as_str(), label.as_str(), *chosen, *enabled))
        .collect();
    let mut note = match &graphics.drawing {
        Some(gpu) => tr!(
            "Ahora dibuja con {gpu}. Se aplica la próxima vez que abras RUNE.",
            gpu = gpu.as_str()
        ),
        None => t("Se aplica la próxima vez que abras RUNE.").to_owned(),
    };
    if graphics.fallback {
        let failed = if graphics.fallback_chosen {
            t("La GPU elegida no pudo abrir la ventana: esta vez dibuja cualquiera.")
        } else {
            t("La GPU que eligió RUNE no pudo abrir la ventana: esta vez dibuja cualquiera.")
        };
        note = format!("{note} {failed}");
    } else if let Some(name) = &graphics.overridden {
        note = format!(
            "{note} {}",
            tr!(
                "La variable {name} elige la GPU mientras esté.",
                name = name.as_str()
            )
        );
    }
    Element::column(vec![
        parts::setting(t("Dibujar con"), Some(&note), None, true, look),
        select(ctx, "gpu", t("Dibujar con"), &borrowed, ctx.geometry.column),
        Element::spacer().size(None, Some(12.0)),
    ])
}

// The window's own title bar (barra-e-icono).
/// «Barra de título»: as RUNE decides, RUNE's or the system's, now.
fn title_bar(ctx: &Ctx) -> Element {
    use crate::title_bar::TitleBar;
    let chosen = ctx.ui.desktop.title_bar;
    let options = [
        ("us:title-bar=auto", t("Automática"), TitleBar::Automatic),
        ("us:title-bar=rune", t("La de RUNE"), TitleBar::Rune),
        ("us:title-bar=system", t("La del sistema"), TitleBar::System),
    ]
    .map(|(id, label, choice)| (id, label, choice == chosen, true));
    Element::column(vec![
        parts::setting(
            t("Barra de título"),
            Some(t("Automática: la de RUNE en Windows y en X11, la del sistema en Wayland. La de RUNE pone minimizar, maximizar y cerrar al final de la cabecera: arrastrá la cabecera para mover la ventana, hacé doble clic para maximizarla y cambiá su tamaño desde los bordes.")),
            None,
            true,
            ctx.look,
        ),
        select(ctx, "title-bar", t("Barra de título"), &options, ctx.geometry.column),
        Element::spacer().size(None, Some(12.0)),
    ])
}

// Windows (windows-base).
/// `text`, translated, on Windows, where it says what is different there or
/// not there yet; `None` elsewhere.
fn windows(text: &'static str) -> Option<&'static str> {
    cfg!(windows).then(|| t(text))
}
