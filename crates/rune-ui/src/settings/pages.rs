//! What the settings show: the panel over the window, its sidebar, each
//! page, and the search through them; and the quick switcher.
//!
//! Every page is listed in [`SECTIONS`] with its settings and the words they
//! are found by. A page's column is built from that list: each setting has
//! a key, and [`setting`] draws it. Searching keeps the settings whose words
//! match, the sections and pages that hold them, and their links.

use limn::{
    Button, ButtonVariant, Color, Component, DropdownOption, Element, Icon, Kbd, ListRow, Modal,
    PopoverPlacement, ScrollArea, SearchField, Select, SelectVariant, Size, Slider, TextField,
};
use rune_components::quick_voice as quick;
use rune_components::server_settings::{swatches, swatches_width, Swatches};
use rune_components::user_settings::{
    self as parts, Account, Geometry, Nav, NavAnchor, NavPage, Preview,
};
use rune_components::{line, text, Look};
// `Language` is a page here; the interface's languages are `I18n`.
use rune_i18n::{es, t, tr, Language as I18n};
use rune_model::ChannelKind;
use rune_provider::{ProviderKind, VoiceSupport};
use rune_state::Store;

use super::{
    anchor, destinations, profile_locked, SettingsPage, ACCENT, GROUP_GAPS, NAME, SEARCH,
    SWITCHER_QUERY, TEXT_SIZES, ZOOMS,
};
use crate::{GlobalKeys, Ui, ViewContext};

use SettingsPage::*;

// Desktop, language and local voice (P6).
mod desktop;
// Call sounds (audio).
mod sounds;
// Reading settings and pictures on disk (imagenes-y-lectura).
mod image_cache;
mod reading;
// Profile cosmetics (perfil-tarjeta-y-cosmeticos).
pub(super) mod cosmetics;
// Desktop session and games (escritorio-sesion-y-juegos).
mod games;
// Editable shortcuts (atajos-y-navegacion).
mod shortcuts;
// Own themes, main color and transparency (temas-propios).
mod themes;
// RUNE's own updates (actualizaciones).
mod updates;

/// A section of a page: its key, its title, and its settings, each a key
/// and the words it is found by besides its name.
struct Section {
    page: SettingsPage,
    key: &'static str,
    title: &'static str,
    rows: &'static [(&'static str, &'static str)],
}

const SECTIONS: &[Section] = &[
    Section {
        page: Account,
        key: "account-info",
        title: es("Información de cuenta"),
        rows: &[
            ("display-name", es("nombre para mostrar apodo cómo te ven")),
            ("handle", es("nombre de usuario handle arroba")),
            ("accent", es("color del avatar inicial foto imagen")),
            ("profile", es("editar perfil pronombres biografía sobre mí")),
        ],
    },
    Section {
        page: Account,
        key: "account-session",
        title: es("Sesión"),
        rows: &[
            (
                "accounts",
                es("cambiar de cuenta agregar otra cuenta cuentas recordadas varias"),
            ),
            ("sign-out", es("cerrar sesión salir desconectar cuenta")),
        ],
    },
    // The account's synced settings (P1).
    Section {
        page: Privacy,
        key: "messaging",
        title: es("Mensajes directos y solicitudes"),
        rows: &[
            (
                "server-dms",
                es("permitir mensajes directos de miembros de servidores privacidad"),
            ),
            (
                "message-requests",
                es("solicitudes de mensajes filtrar desconocidos privacidad"),
            ),
            ("spam-filter", es("filtro de spam mensajes directos carpeta")),
            (
                "explicit-content",
                es("contenido explícito imágenes difuminar bloquear sensible"),
            ),
            (
                "friend-requests",
                es("quién puede mandarte solicitudes de amistad amigos de amigos"),
            ),
            (
                "server-privacy",
                es("por servidor mensajes directos solicitudes privacidad de cada servidor"),
            ),
        ],
    },
    Section {
        page: Privacy,
        key: "local-data",
        title: es("Datos locales"),
        rows: &[
            (
                "forget",
                es("borrar lo guardado en este equipo caché borradores instantánea eliminar datos"),
            ),
            (
                "credentials",
                es("credenciales token contraseña cuenta personal seguridad"),
            ),
        ],
    },
    Section {
        page: Notifications,
        key: "overview",
        title: es("Vista general"),
        rows: &[
            ("desktop", es("activar las notificaciones de escritorio sistema avisos")),
            (
                "notification-content",
                es("mostrar el contenido del mensaje privacidad ocultar texto"),
            ),
            ("message-sound", es("sonido de mensajes menciones privados tono")),
        ],
    },
    Section {
        page: Notifications,
        key: "badges",
        title: es("Insignias"),
        rows: &[
            (
                "unread-badges",
                es("marcar servidores con mensajes sin leer insignia riel no leídos"),
            ),
            ("mention-badges", es("número de menciones insignia globo rojo")),
        ],
    },
    Section {
        page: Voice,
        key: "voice",
        // Serein's order (2026-10-18): devices and levels, the test,
        // processing, how the microphone goes through, the camera.
        title: es("Dispositivos y niveles"),
        rows: &[
            (
                "devices",
                es("micrófono altavoz dispositivo de entrada salida auriculares parlantes volumen ganancia actualizar restablecer niveles"),
            ),
            (
                "volumes",
                es("probar sonido prueba de salida altavoz auriculares sonido de la llamada"),
            ),
        ],
    },
    Section {
        page: Voice,
        key: "mic-test",
        title: es("Comprobemos el micrófono"),
        rows: &[
            (
                "meter",
                es("prueba de micrófono nivel medidor probar comprobar entrada procesada"),
            ),
            (
                "loopback",
                es("escucharme oírme retorno monitor auriculares acople"),
            ),
        ],
    },
    Section {
        page: Voice,
        key: "input-profile",
        title: es("Perfil de entrada"),
        rows: &[(
            "input-profile",
            es("perfil aislamiento de voz estudio personalizado procesamiento"),
        )],
    },
    Section {
        page: Voice,
        key: "processing",
        title: es("Procesamiento de voz"),
        rows: &[
            (
                "noise-suppression",
                es("supresión reducción de ruido fondo krisp rnnoise webrtc estándar teclado ventilador"),
            ),
            (
                "echo-cancellation",
                es("cancelación de eco acople altavoces parlantes referencia"),
            ),
            (
                "system-echo",
                es("cancelar el sonido de la pc juegos música videos eco referencia del sistema salida"),
            ),
            (
                "automatic-gain",
                es("control automático de ganancia agc volumen nivel parejo"),
            ),
        ],
    },
    Section {
        page: Voice,
        key: "input-mode",
        title: es("Modo de entrada"),
        rows: &[
            (
                "input-mode",
                es("modo de entrada detección actividad de voz pulsar para hablar ptt"),
            ),
            (
                "sensitivity",
                es("sensibilidad de entrada automáticamente umbral detección de voz indicador habla"),
            ),
            (
                "ptt-key",
                es("atajo tecla pulsar para hablar grabar global fuera de la ventana ptt"),
            ),
            (
                "ptt-release",
                es("retraso al soltar pulsar para hablar demora ptt"),
            ),
        ],
    },
    Section {
        page: Voice,
        key: "call-sounds",
        title: es("Sonidos de la llamada"),
        // Call sounds (audio): drawn in `sounds`.
        rows: sounds::ROWS,
    },
    Section {
        page: Voice,
        key: "video",
        title: es("Cámara y video"),
        rows: &[
            (
                "camera",
                es("cámara web webcam dispositivo de video probar vista previa verse"),
            ),
            (
                "video-hardware",
                es("aceleración por hardware gpu h264 codificador decodificador va-api nvenc nvdec openh264 procesador"),
            ),
        ],
    },
    Section {
        page: Appearance,
        key: "theme",
        title: es("Tema"),
        // Own themes, main color and transparency (temas-propios).
        rows: themes::ROWS,
    },
    Section {
        page: Appearance,
        key: "messages",
        title: es("Mensajes"),
        rows: &[
            (
                "image-previews",
                es("vista previa de las imágenes subidas archivos adjuntos fotos"),
            ),
            ("reactions", es("mostrar reacciones de emojis")),
            (
                "emoticons",
                es("convertir emoticonos en emojis automáticamente caritas :) al enviar"),
            ),
            ("avatars", es("mostrar los avatares de los usuarios compacto")),
            (
                "related-accessibility",
                es("ajustes relacionados accesibilidad tamaño densidad zoom"),
            ),
        ],
    },
    // Profile cosmetics (perfil-tarjeta-y-cosmeticos): drawn in `cosmetics`.
    Section {
        page: Appearance,
        key: "cosmetics",
        title: es("Cosméticos de los perfiles"),
        rows: cosmetics::ROWS,
    },
    Section {
        page: Accessibility,
        key: "readability",
        title: es("Legibilidad del texto"),
        rows: &[("text-size", es("tamaño de texto en el chat fuente letra escala"))],
    },
    Section {
        page: Accessibility,
        key: "density",
        title: es("Densidad visual"),
        rows: &[
            (
                "display",
                es("mostrar el mensaje del chat compacto predeterminado avatares"),
            ),
            ("group-gap", es("espacio entre grupos de mensajes separación")),
            (
                "zoom",
                es("nivel de zoom tamaño de la interfaz escala acercar alejar ctrl"),
            ),
        ],
    },
    Section {
        page: Accessibility,
        key: "color",
        title: es("Color y contraste"),
        rows: &[("role-colors", es("colores de rol nombres"))],
    },
    Section {
        page: Accessibility,
        key: "motion",
        title: es("Movimiento reducido"),
        rows: &[(
            "motion",
            es("activar el movimiento reducido animaciones desplazamiento suave"),
        )],
    },
    Section {
        page: Language,
        key: "language",
        title: es("Idioma"),
        rows: &[("language", es("seleccioná un idioma español inglés lengua"))],
    },
    Section {
        page: Language,
        key: "clock",
        title: es("Formato de hora"),
        rows: &[(
            "clock",
            es("formato de hora 12 horas 24 horas reloj a. m. p. m. tiempo"),
        )],
    },
    Section {
        page: Connection,
        key: "provider",
        title: es("Proveedor"),
        rows: &[
            ("provider", es("proveedor conexión simulación servicio discord")),
            (
                "choose",
                es("elegir cambiar proveedor simulación oficial oauth2 sesión no oficial experimento"),
            ),
            (
                "remembered",
                es("recordar sesión llavero guardada olvidar restaurar al abrir"),
            ),
            (
                "capabilities",
                es("capacidades enviar editar borrar reacciones historial adjuntos presencia voz"),
            ),
            (
                "related-developer",
                es("ajustes relacionados pruebas de la simulación desarrollador"),
            ),
        ],
    },
    Section {
        page: Developer,
        key: "developer",
        title: es("Desarrollador"),
        rows: &[
            ("developer", es("modo desarrollador copiar id identificador")),
            (
                "metrics",
                es("mostrar métricas rendimiento cuadros memoria cpu f12"),
            ),
        ],
    },
    Section {
        page: Developer,
        key: "simulation",
        title: es("Pruebas de la simulación"),
        rows: &[
            ("disconnect", es("simular una desconexión reconexión cortar")),
            (
                "fail-send",
                es("rechazar el próximo envío fallo error reintentar"),
            ),
            (
                "silent-send",
                es("no responder el próximo envío silencio incierto"),
            ),
            // Private calls.
            ("ring-me", es("recibir una llamada entrante sonar")),
            (
                "answer-elsewhere",
                es("atender en otro dispositivo llamada"),
            ),
        ],
    },
];

/// The pages in the sidebar's order, in groups: a heading, then its pages.
const GROUPS: &[(Option<&str>, &[SettingsPage])] = &[
    (None, &[Account, Privacy, Notifications]),
    (
        Some(es("Experiencia")),
        &[Voice, Appearance, Accessibility, System, Language],
    ),
    (None, &[Connection, Updates, Developer]),
];

fn icon_of(page: SettingsPage) -> Icon {
    match page {
        Account => Icon::Custom(parts::PERSON),
        Privacy => Icon::Custom(parts::SHIELD),
        Notifications => Icon::Bell,
        Voice => Icon::Microphone,
        Appearance => Icon::Palette,
        Accessibility => Icon::Sliders,
        System => Icon::Keyboard,
        Language => Icon::Globe,
        Connection => Icon::Laptop,
        Developer => Icon::Code,
        Updates => Icon::Refresh,
    }
}

/// The keys of every section of `page`, in order.
pub(crate) fn section_keys(page: SettingsPage) -> Vec<&'static str> {
    sections()
        .filter(|section| section.page == page)
        .map(|section| section.key)
        .collect()
}

/// The section of `page` named `key`.
pub(crate) fn section_key(page: SettingsPage, key: &str) -> Option<&'static str> {
    section_keys(page).into_iter().find(|found| *found == key)
}

/// The anchor at the top of `page`.
pub(crate) fn top(page: SettingsPage) -> String {
    format!("us:top:{}", page.key())
}

/// How far under the top of the page a section jumped to stands: under the
/// preview where there is one.
pub(crate) fn jump_margin(page: SettingsPage) -> f32 {
    if page == Accessibility {
        parts::PREVIEW_HEIGHT + 56.0
    } else {
        24.0
    }
}

/// Lower case and without accents, to compare what is searched.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            other => other,
        })
        .collect()
}

/// Whether every word of `query` is in one of `texts`.
fn found(query: &str, texts: &[&str]) -> bool {
    let haystack = fold(&texts.join(" "));
    fold(query)
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

/// Whether the provider is the simulation.
fn simulated(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == ProviderKind::Simulated)
}

/// Every section: the pages' own, then the desktop work's (P6), then the
/// reading ones (imagenes-y-lectura).
fn sections() -> impl Iterator<Item = &'static Section> {
    SECTIONS
        .iter()
        // Where the fixed list of shortcuts was, before the desktop's.
        .chain(shortcuts::SECTIONS)
        .chain(desktop::SECTIONS)
        .chain(reading::SECTIONS)
        .chain(image_cache::SECTIONS)
        .chain(updates::SECTIONS)
}

/// Whether a section can show at all: the simulation's tests only with it.
fn available(section: &Section, store: &Store) -> bool {
    match section.key {
        "simulation" => simulated(store),
        "account-session" => super::can_sign_out(store),
        key => desktop::available(key, store),
    }
}

/// The sections of `page` that show for `query`, each with the keys of its
/// settings that do: all of them for an empty query or a title that matches.
fn shown(
    page: SettingsPage,
    query: &str,
    store: &Store,
) -> Vec<(&'static Section, Vec<&'static str>)> {
    let query = query.trim();
    sections()
        .filter(|section| section.page == page && available(section, store))
        .filter_map(|section| {
            // The texts as shown, so a search in either language finds them.
            let title = t(section.title);
            let whole = query.is_empty() || found(query, &[page.title(), title]);
            let rows: Vec<&'static str> = section
                .rows
                .iter()
                .filter(|(key, words)| whole || found(query, &[page.title(), title, key, t(words)]))
                .map(|(key, _)| *key)
                .collect();
            (!rows.is_empty()).then_some((section, rows))
        })
        .collect()
}

fn searching(ui: &Ui) -> Option<&str> {
    Some(ui.settings_state.query.trim()).filter(|query| !query.is_empty())
}

/// While searching, the first section of `page` that matches.
pub(crate) fn first_match(ui: &Ui, store: &Store, page: SettingsPage) -> Option<&'static str> {
    let query = searching(ui)?;
    shown(page, query, store)
        .first()
        .map(|(section, _)| section.key)
}

/// While searching, the first page that has something that matches.
pub(crate) fn first_page(ui: &Ui, store: &Store) -> Option<SettingsPage> {
    let query = searching(ui)?;
    SettingsPage::ALL
        .into_iter()
        .find(|page| !shown(*page, query, store).is_empty())
}

/// What every part of the view reads.
struct Ctx<'a> {
    ui: &'a Ui,
    store: &'a Store,
    look: &'a Look,
    geometry: Geometry,
    context: &'a ViewContext,
}

/// The settings over `root` while they are open.
pub fn modal(ui: &Ui, store: &Store, root: Element, look: &Look, context: &ViewContext) -> Element {
    let Some(page) = ui.settings else { return root };
    let ctx = Ctx {
        ui,
        store,
        look,
        geometry: Geometry::new(context.window),
        context,
    };
    let me = store.me_user();
    let account = Account {
        image: me.and_then(|me| ui.picture(me.picture())),
        name: me.map_or_else(
            || t("Sin cuenta").to_owned(),
            |me| me.shown_name().to_owned(),
        ),
        accent: me.map_or(look.faint, |me| look.ink(me.accent)),
        edit: Some("profile:edit".into()),
    };
    let provider = store
        .provider
        .as_ref()
        .map_or(t("sin proveedor"), |info| t(&info.name));
    let footnote = format!("RUNE {} · {provider}", env!("CARGO_PKG_VERSION"));
    let sidebar = parts::sidebar(
        &account,
        parts::search_box(SEARCH, &ui.settings_state.query, look),
        &navigation(&ctx, page),
        &footnote,
        look,
    );
    let card = parts::panel(
        ctx.geometry,
        sidebar,
        page.title(),
        "us:close",
        content(&ctx, page),
        look,
    );
    Element::popover(root, card, "us:close", PopoverPlacement::Center)
        // The search, not the first link: a focus ring on «Editar perfil»
        // is the first thing seen otherwise.
        .popover_initial_focus(SEARCH)
        .popover_scrim(Color::rgba(0, 0, 0, 184))
        .popover_backdrop_dismiss(true)
        .popover_animation_ms(150)
        .popover_open_scale(0.9)
}

/// The sidebar's pages; while searching, only those with something that
/// matches. The open page lists its sections.
fn navigation(ctx: &Ctx, open: SettingsPage) -> Vec<Nav> {
    let query = searching(ctx.ui).unwrap_or("");
    let mut nav = Vec::new();
    for (heading, pages) in GROUPS {
        let entries: Vec<Nav> = pages
            .iter()
            .filter_map(|page| {
                let sections = shown(*page, query, ctx.store);
                if !query.is_empty() && sections.is_empty() {
                    return None;
                }
                let selected = *page == open;
                let mut anchors = Vec::new();
                if selected && (sections.len() > 1 || !query.is_empty()) {
                    let active = ctx
                        .ui
                        .settings_state
                        .section
                        .or(sections.first().map(|(section, _)| section.key));
                    anchors = sections
                        .iter()
                        .map(|(section, _)| NavAnchor {
                            id: format!("us:jump={}", section.key),
                            label: t(section.title).to_owned(),
                            active: active == Some(section.key),
                        })
                        .collect();
                }
                Some(Nav::Page(NavPage {
                    id: format!("us:page={}", page.key()),
                    label: page.title().to_owned(),
                    icon: icon_of(*page),
                    selected,
                    anchors,
                }))
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        if !nav.is_empty() {
            nav.push(Nav::Separator);
        }
        if let Some(heading) = heading {
            nav.push(Nav::Heading(t(heading).to_owned()));
        }
        nav.extend(entries);
    }
    if nav.is_empty() {
        nav.push(Nav::Heading(t("Ningún ajuste coincide").into()));
    }
    nav
}

/// The open page's column. While searching it shows what matches there,
/// unless nothing does: then the page shows whole.
fn content(ctx: &Ctx, page: SettingsPage) -> Element {
    let query = searching(ctx.ui).unwrap_or("");
    let mut sections = shown(page, query, ctx.store);
    if sections.is_empty() {
        sections = shown(page, "", ctx.store);
    }
    let sections = sections
        .into_iter()
        .filter_map(|(section, keys)| {
            let rows: Vec<Element> = keys
                .into_iter()
                .filter_map(|key| setting(ctx, key))
                .collect();
            (!rows.is_empty()).then(|| {
                parts::section(&anchor(section.key), t(section.title), None, rows, ctx.look)
            })
        })
        .collect();
    let sticky = (page == Accessibility).then(|| preview(ctx));
    parts::page(
        &page.scroll_id(),
        &top(page),
        ctx.geometry,
        sticky,
        sections,
        ctx.look,
    )
}

/// The preview over the accessibility settings: two messages of the
/// account as the timeline would show them now.
fn preview(ctx: &Ctx) -> Element {
    let look = ctx.look;
    let prefs = &ctx.ui.prefs;
    let me = ctx.store.me_user();
    let accent = me.map_or(look.accent, |me| look.ink(me.accent));
    parts::preview(
        &Preview {
            name: me.map_or_else(|| t("Invitado").to_owned(), |me| me.shown_name().to_owned()),
            ink: if prefs.role_colors { accent } else { look.text },
            accent,
            style: prefs.message_style(),
            now: ctx.context.wall,
        },
        ctx.geometry.column,
        look,
    )
}

/// The setting `key`, as its page shows it; `None` when there is nothing to
/// show for it.
fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    let (ui, store, look) = (ctx.ui, ctx.store, ctx.look);
    let prefs = &ui.prefs;
    let width = ctx.geometry.column;
    let toggle = |id: &str, label: &str, note: Option<&str>, on: bool| {
        parts::toggle(id, label, note, on, None, look)
    };
    let stacked = |label: &str, note: Option<&str>, below: Element| {
        Element::column(vec![
            parts::setting(label, note, None, true, look),
            below,
            Element::spacer().size(None, Some(12.0)),
        ])
    };
    if let Some(shown) = crate::account::privacy_setting(store, key, look, width) {
        return Some(shown);
    }
    if let Some(shown) = desktop::setting(ctx, key) {
        return shown;
    }
    if let Some(shown) = sounds::setting(ctx, key) {
        return Some(shown);
    }
    if let Some(shown) = reading::setting(ctx, key).or_else(|| image_cache::setting(ctx, key)) {
        return Some(shown);
    }
    if let Some(shown) = shortcuts::setting(ctx, key) {
        return Some(shown);
    }
    if let Some(shown) = cosmetics::setting(ctx, key) {
        return Some(shown);
    }
    if let Some(shown) = themes::setting(ctx, key) {
        return shown;
    }
    if let Some(shown) = updates::setting(ctx, key) {
        return Some(shown);
    }
    Some(match key {
        "display-name" => display_name(ctx),
        "handle" => parts::setting(
            t("Nombre de usuario"),
            Some(t("Lo asigna el proveedor: RUNE no puede cambiarlo.")),
            Some(line(
                format!("@{}", store.me_user()?.name),
                16.0,
                look.muted,
            )),
            true,
            look,
        ),
        "accent" => {
            let locked = profile_locked(store);
            Element::column(vec![
                parts::setting(
                    t("Color del avatar"),
                    Some(locked.unwrap_or(
                        t("RUNE no tiene fotos de perfil: tu inicial se dibuja sobre este color."),
                    )),
                    None,
                    locked.is_none(),
                    look,
                ),
                swatches(
                    &Swatches {
                        prefix: ACCENT.into(),
                        chosen: store.me_user().map(|me| me.accent),
                        default: None,
                        custom: ui.settings_state.accent_hex.clone(),
                        enabled: locked.is_none(),
                        narrow: width < swatches_width(false),
                        picked: crate::color_picker::picked(ui, ACCENT),
                    },
                    look,
                ),
                Element::spacer().size(None, Some(12.0)),
            ])
        }
        "profile" => parts::setting(
            t("Perfil"),
            Some(t("Pronombres, biografía y cómo te ven los demás.")),
            Some(parts::secondary_button("profile:edit", t("Editar perfil"), true, look)),
            true,
            look,
        ),
        "accounts" => {
            store
                .provider
                .as_ref()
                .filter(|info| info.capabilities.saved_accounts)?;
            let me = store.me();
            let mut choices: Vec<Element> = store
                .accounts
                .iter()
                .filter(|account| Some(account.id) != me)
                .map(|account| {
                    let name = crate::auth::account_name(account);
                    // Its picture when it was last used, else its initial.
                    Element::row(vec![
                        rune_components::avatar_with_image(
                            name,
                            look.accent,
                            28.0,
                            ui.picture(account.avatar.as_ref()),
                        )
                        .centered_y(),
                        parts::secondary_button(
                            &format!("us:account={}", account.id.0),
                            &tr!("Continuar como {name}", name),
                            true,
                            look,
                        )
                        .centered_y(),
                    ])
                    .gap(10.0)
                })
                .collect();
            choices.push(parts::secondary_button(
                "us:add-account",
                t("Agregar una cuenta"),
                true,
                look,
            ));
            stacked(
                t("Cambiar de cuenta"),
                Some(t("Cambiar cierra esta sesión sin olvidarla: sigue recordada en este equipo.")),
                Element::column(choices).gap(8.0),
            )
        }
        "sign-out" => parts::setting(
            t("Cerrar sesión"),
            Some(t("Desconectá esta cuenta de RUNE en este equipo.")),
            Some(parts::danger_button("us:sign-out", t("Cerrar sesión"), look)),
            true,
            look,
        ),
        "forget" => parts::setting(
            t("Borrar lo guardado en este equipo"),
            Some(t("Mensajes en caché, borradores, instantánea e imágenes guardadas de esta cuenta. Los ajustes se conservan.")),
            Some(parts::danger_button("us:forget", t("Borrar"), look)),
            true,
            look,
        ),
        "credentials" => parts::setting(
            t("Credenciales"),
            Some(t("Los datos de acceso no se guardan en la caché de mensajes. Consultá los detalles de la sesión en Conexión.")),
            None,
            true,
            look,
        ),
        "desktop" => toggle(
            "us:desktop",
            t("Activar las notificaciones de escritorio"),
            Some(t("Las menciones y los mensajes directos avisan con una notificación del sistema, salvo en «No molestar», en lo silenciado y en la conversación que estás leyendo. Hacé clic en una para abrirla.")),
            prefs.desktop_notifications,
        ),
        "notification-content" => toggle(
            "us:notification-content",
            t("Mostrar el contenido del mensaje"),
            Some(t("Apagado, la notificación dice quién escribió y «Nuevo mensaje», sin el texto.")),
            prefs.notification_content,
        ),
        "message-sound" => toggle(
            "us:message-sound",
            t("Sonido de mensajes"),
            Some(t("Un tono breve con cada mención o mensaje directo que avisaría.")),
            prefs.message_sounds,
        ),
        "unread-badges" => toggle(
            "us:unread-badges",
            t("Marcar los servidores con mensajes sin leer"),
            Some(t("Una marca junto al ícono del servidor, en el riel, mientras tiene algo sin leer.")),
            prefs.unread_badges,
        ),
        "mention-badges" => toggle(
            "us:mention-badges",
            t("Mostrar cuántas menciones tenés"),
            Some(t("Un globo con el número sobre el ícono del servidor y el de los mensajes directos.")),
            prefs.mention_badges,
        ),
        "devices" => devices(ctx),
        "volumes" => volumes(ctx),
        "meter" => meter(ctx),
        "loopback" => {
            let note = if ui.audio.loopback && ui.audio.output_starting {
                t("Abriendo la salida…")
            } else {
                t("Oí tu micrófono procesado en el altavoz elegido. Usá auriculares: por altavoces se acopla.")
            };
            parts::toggle(
                "us:loopback",
                t("Escucharme"),
                Some(note),
                ui.audio.loopback,
                crate::quick_voice::call_holds_output(store).then_some(t(
                    "En una llamada la salida es suya: podés escucharte de nuevo al colgar.",
                )),
                look,
            )
        }
        "input-profile" => parts::choices(
            "us:profile",
            t("Perfil de entrada"),
            &[
                (
                    "us:profile=voice-isolation".to_owned(),
                    t("Aislamiento de voz"),
                    Some(t("Solo tu voz: RNNoise, cancelación de eco, ganancia automática y una compuerta que deja pasar lo que RNNoise oye como voz.")),
                ),
                (
                    "us:profile=studio".to_owned(),
                    t("Estudio"),
                    Some(t("Sonido puro: el micrófono tal cual, sin procesamiento.")),
                ),
                (
                    "us:profile=custom".to_owned(),
                    t("Personalizado"),
                    Some(t("Elegí vos el procesamiento de voz, más abajo.")),
                ),
            ],
            &format!("us:profile={}", prefs.input_profile.key()),
            look,
        ),
        "noise-suppression" => {
            let chosen = prefs.effective_processing().suppression;
            let custom = prefs.input_profile == super::InputProfile::Custom;
            stacked(
                t("Supresión de ruido"),
                Some(if custom {
                    t("Discord usa Krisp, que es privativo; RUNE ofrece RNNoise y WebRTC, abiertos y en este equipo.")
                } else {
                    t("Lo elige el perfil; cambiar algo acá lo pasa a Personalizado. Krisp, el de Discord, es privativo: RUNE ofrece RNNoise y WebRTC.")
                }),
                parts::choices(
                    "us:suppression",
                    t("Supresión de ruido"),
                    &[
                        (
                            "us:suppression=rnnoise".to_owned(),
                            "RNNoise",
                            Some(t("Red neuronal abierta: también teclado y ruidos que cambian, y con la sensibilidad automática decide qué es voz. Sirve con cualquier micrófono: RUNE lo pasa a 48 kHz.")),
                        ),
                        (
                            "us:suppression=webrtc".to_owned(),
                            "WebRTC",
                            Some(t("Más liviano, para ruido constante como un ventilador.")),
                        ),
                        ("us:suppression=off".to_owned(), t("Ninguna"), None),
                    ],
                    match chosen {
                        rune_media::ProcessingMode::RnNoise => "us:suppression=rnnoise",
                        rune_media::ProcessingMode::WebRtc => "us:suppression=webrtc",
                        rune_media::ProcessingMode::Off => "us:suppression=off",
                    },
                    look,
                ),
            )
        }
        "echo-cancellation" => toggle(
            "us:echo-cancellation",
            t("Cancelación de eco"),
            Some(t("Evita que el micrófono capte lo que suena en tus auriculares, así nadie se escucha a sí mismo.")),
            prefs.effective_processing().echo_cancellation,
        ),
        "system-echo" => {
            let echo = prefs.effective_processing().echo_cancellation;
            let note = match ui.audio.system_echo.filter(|_| echo && prefs.system_echo) {
                Some(true) => t("Activo: se cancela todo lo que suena en tu salida. Lo que suena se usa sólo como referencia en este equipo: nunca se graba ni se envía."),
                Some(false) => t("No se pudo escuchar tu salida: por ahora se cancela sólo lo que reproduce RUNE."),
                None => t("Juegos, música y videos que suenan en tu salida no se cuelan por el micrófono. Lo que suena se usa sólo como referencia en este equipo: nunca se graba ni se envía."),
            };
            parts::toggle(
                "us:system-echo",
                t("Cancelar también el sonido de la PC"),
                Some(note),
                echo && prefs.system_echo,
                (!echo).then_some(t("Necesita la cancelación de eco.")),
                look,
            )
        }
        "automatic-gain" => toggle(
            "us:automatic-gain",
            t("Control automático de ganancia"),
            Some(t("Sube o baja tu voz para mantenerla pareja. Es digital: no toca el volumen del micrófono en el sistema.")),
            prefs.effective_processing().automatic_gain,
        ),
        "camera" => camera(ctx),
        // Windows has no GPU engine yet: OpenH264 always (windows-medios).
        "video-hardware" if cfg!(windows) => parts::toggle(
            "us:video-hardware",
            t("Aceleración por hardware de H.264"),
            None,
            false,
            Some(t("En Windows RUNE todavía codifica y decodifica el video con OpenH264, en el procesador.")),
            look,
        ),
        "video-hardware" => toggle(
            "us:video-hardware",
            t("Aceleración por hardware de H.264"),
            Some(t("Codifica y decodifica el video con la GPU (VA-API, NVENC o NVDEC) cuando hay una. Apagala para usar OpenH264 en el procesador si la imagen se ve mal. Cambia en el momento, también lo que ya mandás o mirás.")),
            prefs.video_hardware,
        ),
        // The section's title names it already.
        "input-mode" => Element::column(vec![
            Element::spacer().size(None, Some(8.0)),
            parts::choices(
                "us:input",
                t("Modo de entrada"),
                &[
                    (
                        "us:input=voice".to_owned(),
                        t("Detección de voz"),
                        Some(t("El micrófono pasa cuando hablás por encima de la sensibilidad.")),
                    ),
                    (
                        "us:input=ptt".to_owned(),
                        t("Pulsar para hablar"),
                        Some(t("El micrófono pasa sólo mientras mantenés el atajo.")),
                    ),
                ],
                if prefs.push_to_talk {
                    "us:input=ptt"
                } else {
                    "us:input=voice"
                },
                look,
            ),
            Element::spacer().size(None, Some(12.0)),
        ]),
        "ptt-key" => {
            prefs.push_to_talk.then_some(())?;
            push_to_talk_key(ctx)
        }
        "ptt-release" => {
            prefs.push_to_talk.then_some(())?;
            stacked(
                t("Retraso al soltar Pulsar para hablar"),
                Some(
                    tr!(
                        "{ms} ms: cuánto sigue pasando el micrófono después de soltar el atajo.",
                        ms = prefs.push_to_talk_release
                    )
                    .as_str(),
                ),
                Slider::new(
                    "us:ptt-release",
                    f32::from(prefs.push_to_talk_release) / f32::from(super::MAX_RELEASE),
                    look.palette(),
                )
                .width(width)
                .ink(look.accent)
                .accessibility_label(t("Retraso al soltar Pulsar para hablar"))
                .render(),
            )
        }
        "sensitivity" => {
            (!prefs.push_to_talk).then_some(())?;
            sensitivity(ctx)
        }
        "image-previews" => Element::column(vec![
            parts::subtitle(t("Imágenes, archivos y reacciones"), look),
            toggle(
                "us:image-previews",
                t("Mostrar la vista previa de las imágenes subidas"),
                Some(t("Apagado, una imagen se muestra como cualquier otro archivo.")),
                prefs.image_previews,
            ),
        ]),
        "reactions" => toggle(
            "us:reactions",
            t("Mostrar reacciones de emojis"),
            None,
            prefs.reactions,
        ),
        "emoticons" => toggle(
            "us:emoticons",
            t("Convertir los emoticonos de tus mensajes en emojis"),
            Some(t("Al enviar o editar, :) se vuelve 🙂. El código y los enlaces no cambian.")),
            prefs.convert_emoticons,
        ),
        "avatars" => parts::toggle(
            "us:avatars",
            t("Mostrar los avatares de los usuarios"),
            None,
            !prefs.compact,
            Some(t("Para ocultar los avatares, elegí Compacto en Accesibilidad › Mostrar el mensaje del chat.")),
            look,
        ),
        "related-accessibility" => parts::related(
            "us:page=accessibility",
            icon_of(Accessibility),
            t("Accesibilidad"),
            t("Tamaño del texto, densidad de los mensajes y zoom."),
            look,
        ),
        "text-size" => stacked(
            t("Tamaño de texto en el chat"),
            Some(t("Ajustá el tamaño de la fuente del chat.")),
            ticked(
                "us:text-size",
                t("Tamaño de texto en el chat"),
                &TEXT_SIZES.map(|size| format!("{size}px")),
                TEXT_SIZES.iter().position(|size| *size == prefs.text_size),
                1,
                width,
                look,
            ),
        ),
        "display" => stacked(
            t("Mostrar el mensaje del chat"),
            Some(t("Cambiá cómo se ven los mensajes del chat.")),
            parts::choices(
                "us:display",
                t("Mostrar el mensaje del chat"),
                &[
                    (
                        "us:display=cozy".to_owned(),
                        t("Predeterminado"),
                        Some(t("El nombre y el avatar encabezan cada grupo de mensajes.")),
                    ),
                    (
                        "us:display=compact".to_owned(),
                        t("Compacto"),
                        Some(t("Un mensaje por línea, con la hora y el nombre delante, sin avatares.")),
                    ),
                ],
                if prefs.compact {
                    "us:display=compact"
                } else {
                    "us:display=cozy"
                },
                look,
            ),
        ),
        "group-gap" => stacked(
            t("Espacio entre grupos de mensajes"),
            None,
            ticked(
                "us:group-gap",
                t("Espacio entre grupos de mensajes"),
                &GROUP_GAPS.map(|gap| format!("{gap}px")),
                GROUP_GAPS.iter().position(|gap| *gap == prefs.group_gap),
                3,
                width,
                look,
            ),
        ),
        "zoom" => {
            let nearest = ZOOMS
                .iter()
                .enumerate()
                .min_by_key(|(_, zoom)| zoom.abs_diff(prefs.zoom))
                .map(|(mark, _)| mark);
            stacked(
                t("Nivel de zoom"),
                Some(t("Ajustá el tamaño de toda la interfaz. También podés cambiarlo con Ctrl + y Ctrl −, y volver al 100 % con Ctrl + 0.")),
                ticked(
                    "us:zoom",
                    t("Nivel de zoom"),
                    &ZOOMS.map(|zoom| zoom.to_string()),
                    ui.settings_state.zoom_drag.or(nearest),
                    5,
                    width,
                    look,
                ),
            )
        }
        "role-colors" => {
            let options = [
                ("us:role-colors=on", t("En los nombres"), prefs.role_colors, true),
                (
                    "us:role-colors=off",
                    t("No mostrar colores de rol"),
                    !prefs.role_colors,
                    true,
                ),
            ];
            parts::setting(
                t("Colores de rol"),
                Some(t("Si los nombres toman el color de su rol más alto, en el chat y en la lista de miembros.")),
                Some(select(
                    ctx,
                    "role-colors",
                    t("Colores de rol"),
                    &options,
                    (width * 0.4).clamp(180.0, 280.0),
                )),
                true,
                look,
            )
        }
        "motion" => toggle(
            "us:motion",
            t("Activar el movimiento reducido"),
            Some(t("Sin animaciones ni desplazamientos suaves.")),
            ui.reduced_motion,
        ),
        "language" => {
            let selected = match prefs.language {
                None => "us:lang=system",
                Some(I18n::Spanish) => "us:lang=es",
                Some(I18n::English) => "us:lang=en",
            };
            let system = tr!(
                "Sistema ({language})",
                language = ui.desktop.system_language.name()
            );
            stacked(
                t("Idioma de RUNE"),
                Some(t(
                    "El idioma de los menús, los ajustes y los avisos. «Sistema» sigue el del equipo. Los mensajes no se traducen.",
                )),
                parts::choices(
                    "us:lang",
                    t("Idioma de RUNE"),
                    &[
                        ("us:lang=system".to_owned(), &system, None),
                        ("us:lang=es".to_owned(), I18n::Spanish.name(), None),
                        ("us:lang=en".to_owned(), I18n::English.name(), None),
                    ],
                    selected,
                    look,
                ),
            )
        }
        "clock" => stacked(
            t("Formato de hora"),
            Some(t("Cómo se escriben las horas de los mensajes.")),
            parts::choices(
                "us:clock",
                t("Formato de hora"),
                &[
                    ("us:clock=12".to_owned(), t("12 horas"), Some(t("2:05 p. m."))),
                    ("us:clock=24".to_owned(), t("24 horas"), Some("14:05")),
                ],
                if prefs.twelve_hour {
                    "us:clock=12"
                } else {
                    "us:clock=24"
                },
                look,
            ),
        ),
        "provider" => {
            let Some(info) = &store.provider else {
                return Some(parts::setting(
                    t("Sin proveedor"),
                    Some(t("RUNE no está conectado a ningún servicio.")),
                    None,
                    true,
                    look,
                ));
            };
            let mut details = vec![parts::setting(
                t(&info.name),
                Some(t(&info.notice)),
                None,
                true,
                look,
            )];
            if let rune_provider::AuthenticationState::SignedIn { scopes } = &store.authentication {
                if !scopes.is_empty() {
                    details.push(parts::setting(
                        t("Permisos concedidos"),
                        Some(&scopes.join(", ")),
                        None,
                        true,
                        look,
                    ));
                }
            }
            Element::column(details)
        }
        // What the host can start instead, each with its origin.
        "choose" => {
            crate::providers::choosable(ui).then_some(())?;
            stacked(
                t("Elige el proveedor"),
                Some(t("Cada uno dice de dónde vienen sus datos. Cambiar cierra la sesión de este.")),
                Element::column(crate::providers::choices(ui, width, look)).gap(8.0),
            )
        }
        "remembered" => match store.remembered? {
            true => parts::setting(
                t("Sesión recordada"),
                Some(t("Guardada en el llavero del sistema: RUNE la restaura al abrirse. Cerrar sesión también la borra.")),
                Some(parts::danger_button("us:forget-session", t("Olvidar esta sesión"), look)),
                true,
                look,
            ),
            false => parts::setting(
                t("Sesión no recordada"),
                Some(t("Vive sólo en memoria: al cerrar RUNE vas a tener que iniciar sesión otra vez.")),
                None,
                true,
                look,
            ),
        },
        "capabilities" => capabilities(ctx)?,
        "related-developer" => {
            simulated(store).then_some(())?;
            parts::related(
                "us:page=developer",
                icon_of(Developer),
                t("Pruebas de la simulación"),
                t("Cortá la conexión o hacé fallar un envío para ver cómo responde RUNE."),
                look,
            )
        }
        "developer" => toggle(
            "us:developer",
            t("Modo desarrollador"),
            Some(t("Agrega «Copiar ID» a los menús de mensajes, canales, categorías, servidores, roles y miembros.")),
            prefs.developer,
        ),
        "metrics" => toggle(
            "us:metrics",
            t("Mostrar métricas"),
            Some(t("Tiempos de cada cuadro, memoria y CPU, sobre la ventana. También con F12.")),
            ui.show_metrics,
        ),
        "disconnect" => trial(
            t("Simular una desconexión"),
            t("Corta la sesión simulada para ver la reconexión automática."),
            "us:disconnect",
            t("Cortar"),
            look,
        ),
        "fail-send" => trial(
            t("Rechazar el próximo envío"),
            t("El mensaje queda marcado como no enviado, con Reintentar y Descartar."),
            "us:fail-send",
            t("Rechazar"),
            look,
        ),
        "silent-send" => trial(
            t("No responder el próximo envío"),
            t("Pasado el plazo, el mensaje queda como de destino incierto."),
            "us:silent-send",
            t("Silenciar"),
            look,
        ),
        // Private calls.
        "ring-me" => trial(
            t("Recibir una llamada"),
            t("La persona de tu primer privado te llama: el aviso, el sonido y la notificación."),
            "us:ring-me",
            t("Llamar"),
            look,
        ),
        "answer-elsewhere" => trial(
            t("Atenderla en otro dispositivo"),
            t("La llamada que suena sigue en otro de tus dispositivos."),
            "us:answer-elsewhere",
            t("Atender"),
            look,
        ),
        _ => return None,
    })
}

/// A test of the simulation: what it does and its button.
fn trial(title: &str, note: &str, id: &str, label: &str, look: &Look) -> Element {
    parts::setting(
        title,
        Some(note),
        Some(parts::secondary_button(id, label, true, look)),
        true,
        look,
    )
}

/// A slider with marks; a value between marks shows at none.
fn ticked(
    id: &str,
    label: &str,
    labels: &[String],
    index: Option<usize>,
    usual: usize,
    width: f32,
    look: &Look,
) -> Element {
    let names: Vec<&str> = labels.iter().map(String::as_str).collect();
    parts::ticked_slider(
        id,
        label,
        &names,
        index.unwrap_or(usual),
        usual,
        width,
        look,
    )
}

/// A list to choose from, `us:select:{name}`, drawn as a field: each option
/// its action, its label, whether it is chosen and whether it can be; the
/// ones that cannot be chosen yet stay in it, disabled.
fn select(
    ctx: &Ctx,
    name: &str,
    label: &str,
    options: &[(&str, &str, bool, bool)],
    width: f32,
) -> Element {
    let current = options
        .iter()
        .find(|(_, _, chosen, _)| *chosen)
        .map_or("", |(_, label, _, _)| *label);
    let choices = options
        .iter()
        .map(|(id, text, chosen, enabled)| {
            DropdownOption::new(*id, *text)
                .selected(*chosen)
                .enabled(*enabled)
        })
        .collect();
    Select::new(
        format!("us:select:{name}"),
        current,
        choices,
        ctx.ui.settings_state.select == Some(name),
        "us:select:dismiss",
        ctx.look.palette(),
    )
    .menu_palette(ctx.look.menu_palette())
    .variant(SelectVariant::Field)
    .size(limn::ControlSize::Spacious)
    .accessibility_label(label)
    .width(width)
    .menu_width(width)
    .render()
}

/// The display name: what it is and a button to change it, or the field
/// being written with its buttons under it.
fn display_name(ctx: &Ctx) -> Element {
    let look = ctx.look;
    let locked = profile_locked(ctx.store);
    let me = ctx.store.me_user();
    let note = locked.unwrap_or(t(
        "Cómo te ven los demás. Vacío, se muestra tu nombre de usuario.",
    ));
    match &ctx.ui.settings_state.name {
        Some(draft) => Element::column(vec![
            parts::setting(t("Nombre para mostrar"), Some(note), None, true, look),
            Element::row(vec![
                TextField::new(
                    NAME,
                    draft.clone(),
                    me.map_or("", |me| me.name.as_str()),
                    look.palette(),
                )
                .accessibility_label(t("Nombre para mostrar"))
                .render()
                .flex(1.0),
                parts::primary_button("us:name:save", t("Guardar"), true, look),
                parts::secondary_button("us:name:cancel", t("Cancelar"), true, look),
            ])
            .gap(8.0),
            Element::spacer().size(None, Some(12.0)),
        ]),
        None => parts::setting(
            t("Nombre para mostrar"),
            Some(note),
            Some(
                Element::row(vec![
                    line(me.map_or("—", |me| me.shown_name()), 16.0, look.muted)
                        .max_width(220.0)
                        .centered_y(),
                    parts::secondary_button("us:name:edit", t("Editar"), locked.is_none(), look),
                ])
                .gap(16.0),
            ),
            locked.is_none(),
            look,
        ),
    }
}

/// The microphone and the speaker, side by side from 480 px and one under
/// the other below, each with its volume, as Serein lays them out; and
/// finding them again, or putting the volumes back once they changed.
fn devices(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let width = ctx.geometry.column;
    let wide = width >= 480.0;
    let half = if wide { (width - 17.0) / 2.0 } else { width };
    let label = |name: &str| line(name, 14.0, look.text).font_family(limn::FontFamily::SansMedium);
    let side = |output: bool| {
        let list = crate::quick_voice::devices(ui, output);
        let (title, missing, id, name, volume, most) = if output {
            (
                t("Dispositivo de salida"),
                t("La salida elegida no está conectada. Conectala o elegí otra."),
                "us:output-volume",
                t("Volumen de salida"),
                ui.prefs.output_volume,
                rune_media::MAX_VOLUME,
            )
        } else {
            (
                t("Dispositivo de entrada"),
                t("El micrófono elegido no está conectado. Conectalo o elegí otro."),
                "us:input-volume",
                t("Volumen de entrada"),
                ui.prefs.input_volume,
                rune_media::MAX_INPUT_VOLUME,
            )
        };
        let mut column = vec![label(title), quick::device_select(&list, half, look)];
        if list.missing() {
            column.push(text(missing, 12.0, look.danger));
        }
        column.extend([
            Element::spacer().size(None, Some(4.0)),
            label(name),
            quick::percent_slider(id, name, volume, most, half, look),
        ]);
        Element::column(column).gap(8.0)
    };
    let pair = if wide {
        Element::row(vec![side(false), side(true)]).gap(17.0)
    } else {
        Element::column(vec![side(false), side(true)]).gap(16.0)
    };
    let mut column = vec![
        Element::spacer().size(None, Some(12.0)),
        pair,
        quick::volume_note(look),
    ];
    match &ui.audio.devices {
        Some(Err(error)) => column.push(text(
            tr!("No se pudieron leer los dispositivos: {error}", error),
            13.0,
            look.danger,
        )),
        Some(Ok(found)) => column.push(line(
            tr!("Sistema de audio: {backend}", backend = found.backend),
            12.0,
            look.muted,
        )),
        None => {}
    }
    let link = |id: &str, label: &str, glyph: Icon| {
        Button::new(id, label, look.palette())
            .variant(ButtonVariant::Link)
            .icon(glyph)
            .render()
    };
    let mut links = vec![link(
        "us:devices",
        t("Actualizar dispositivos"),
        Icon::Refresh,
    )];
    if crate::quick_voice::levels_changed(ui) {
        links.push(link(
            "us:reset-levels",
            t("Restablecer niveles"),
            Icon::Sliders,
        ));
    }
    links.push(Element::spacer().flex(1.0));
    column.push(Element::row(links).gap(16.0));
    Element::column(column).gap(8.0)
}

/// «Probar sonido»: one of the call's sounds on the chosen output, also
/// during a call, into its sound.
fn volumes(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let note = if ui.prefs.output_volume == 0.0 {
        t("Con el volumen de salida en 0 % no se oye.")
    } else if ctx.store.call().is_some() {
        t("Suena el sonido de entrar a la llamada, mezclado con ella.")
    } else {
        t("Suena el sonido de entrar a la llamada en la salida elegida.")
    };
    Element::column(vec![
        Element::spacer().size(None, Some(8.0)),
        Element::row(vec![
            parts::secondary_button("us:speaker-test", t("Probar sonido"), true, look),
            line(note, 13.0, look.muted).centered_y(),
            Element::spacer().flex(1.0),
        ])
        .gap(12.0),
    ])
}

/// «Comprobemos el micrófono», as Serein's: one button, the level as the
/// call would use it, in dBFS, with where voice starts marked on it; and,
/// for comparing the suppressors, the levels before and after processing.
/// In a call it measures the call's own microphone: nothing more is sent.
fn meter(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let button = if ui.audio.testing {
        parts::secondary_button("us:meter", t("Detener la prueba"), true, look)
    } else {
        parts::primary_button("us:meter", t("Empezar la prueba"), true, look)
    };
    let status = if ui.audio.testing && ui.audio.starting {
        t("Abriendo micrófono…").to_owned()
    } else if ui.audio.testing {
        crate::quick_voice::reading(ui).unwrap_or_else(|| t("Escuchando.").to_owned())
    } else {
        String::new()
    };
    let width = ctx.geometry.column;
    let live = ui.audio.testing && !ui.audio.starting;
    let mut rows = vec![text(
        t("Empezá la prueba y decí algo: la barra muestra tu nivel, y la marca, dónde empieza a contar como voz. Es local; no se envía nada."),
        14.0,
        look.muted,
    )];
    if ctx.store.call().is_some() {
        rows.push(text(
            t("En la llamada mide el mismo micrófono que usa ella, sin cambiar lo que se envía."),
            13.0,
            look.muted,
        ));
    }
    rows.extend([
        Element::spacer().size(None, Some(4.0)),
        Element::row(vec![
            button,
            line(status, 13.0, look.muted).centered_y(),
            Element::spacer().flex(1.0),
        ])
        .gap(12.0),
        quick::level_meter(
            "us:level",
            ui.audio.levels.last().copied().filter(|_| live),
            crate::quick_voice::threshold(ui),
            width,
            look,
        ),
        Element::spacer().size(None, Some(4.0)),
        line(t("Antes y después del procesamiento"), 13.0, look.text)
            .font_family(limn::FontFamily::SansMedium),
    ]);
    let width = (width - 92.0).max(40.0);
    for (id, label, levels) in [
        ("us:raw-levels", t("Entrada"), &ui.audio.raw_levels),
        ("us:levels", t("Procesada"), &ui.audio.levels),
    ] {
        let levels = if live { levels.as_slice() } else { &[][..] };
        rows.push(
            Element::row(vec![
                line(label, 13.0, look.muted)
                    .size(Some(84.0), None)
                    .centered_y(),
                parts::level_bars(id, levels, width, look).accessible(
                    limn::Access::new(limn::Role::Status)
                        .id(id)
                        .name(tr!("Nivel de {label}", label))
                        .description(if !ui.audio.testing {
                            t("Sin prueba")
                        } else if ui.audio.starting {
                            t("Esperando micrófono")
                        } else {
                            t("Prueba local")
                        }),
                ),
            ])
            .gap(8.0),
        );
    }
    Element::column(rows).gap(8.0)
}

/// The push-to-talk key, its recorder, and whether it also works while
/// another window has focus.
fn push_to_talk_key(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let recording = ui.settings_state.recording.is_some();
    let key = ui.prefs.push_to_talk_key.as_deref();
    // Why the key just recorded was refused (atajos-y-navegacion).
    let refused = ui
        .settings_state
        .key_notice
        .as_ref()
        .filter(|(action, _)| *action == crate::keybinds::Action::default())
        .map(|(_, why)| why.as_str());
    let note = if recording {
        t("Pulsá la tecla o la combinación. Esc cancela.")
    } else if let Some(why) = refused {
        why
    } else if key.is_none() {
        t("Todavía no tiene atajo: grabá uno.")
    } else {
        t("Funciona con la ventana de RUNE enfocada.")
    };
    let control = Element::row(vec![
        Kbd::new(
            if recording {
                "…"
            } else {
                key.unwrap_or(t("Sin atajo"))
            },
            look.palette(),
        )
        .render()
        .centered_y(),
        parts::secondary_button(
            "us:ptt-record",
            if recording {
                t("Cancelar")
            } else {
                t("Grabar atajo")
            },
            true,
            look,
        ),
    ])
    .gap(12.0);
    let global = match &ui.audio.global_keys {
        GlobalKeys::Off => None,
        GlobalKeys::Waiting => Some(t("Pidiendo el atajo global al escritorio…").to_owned()),
        GlobalKeys::Bound(trigger) if trigger.is_empty() => {
            Some(t("También fuera de RUNE, con el atajo que asignó el escritorio.").to_owned())
        }
        GlobalKeys::Bound(trigger) => Some(tr!(
            "También fuera de RUNE: el escritorio lo asignó a {trigger}.",
            trigger
        )),
        GlobalKeys::Unavailable(reason) => Some(tr!(
            "Fuera de RUNE no funciona: {reason}",
            reason = t(reason)
        )),
    };
    let mut column = vec![parts::setting(
        t("Atajo de Pulsar para hablar"),
        Some(note),
        Some(control),
        true,
        look,
    )];
    if let Some(global) = global {
        column.push(text(global, 13.0, look.muted));
    }
    if ui.audio.testing {
        column.push(line(
            if ui.audio.transmitting {
                t("Transmitiendo: el micrófono pasa.")
            } else {
                t("En silencio: mantené el atajo para hablar.")
            },
            13.0,
            look.muted,
        ));
    }
    column.push(Element::spacer().size(None, Some(12.0)));
    Element::column(column).gap(4.0)
}

/// The camera to use and «Probar video»: the system asks the first time,
/// and the picture stays on this machine.
fn camera(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let video = &ui.video;
    let chosen = video.device.as_deref();
    let mut options =
        vec![
            DropdownOption::new("us:camera-default", t("Predeterminada del sistema"))
                .icon(Icon::Video)
                .selected(chosen.is_none()),
        ];
    for (id, name) in &video.cameras {
        options.push(
            DropdownOption::new(format!("us:camera={id}"), name.clone())
                .icon(Icon::Video)
                .selected(chosen == Some(id.as_str())),
        );
    }
    let current = chosen.map_or(t("Predeterminada del sistema"), |id| {
        video
            .cameras
            .iter()
            .find(|(found, _)| found == id)
            .map_or(id, |(_, name)| name.as_str())
    });
    let width = ctx.geometry.column.min(480.0);
    let select = Select::new(
        "us:select:camera",
        current,
        options,
        ui.settings_state.select == Some("camera"),
        "us:select:dismiss",
        look.palette(),
    )
    .menu_palette(look.menu_palette())
    .icon(Icon::Video)
    .accessibility_label(t("Cámara"))
    .width(width)
    .menu_width(width)
    .render();
    let size = Size::new(width, width * 9.0 / 16.0);
    let picture = match video.camera.live.frame().filter(|_| video.testing) {
        Some((w, h)) => {
            let scale = (size.width / w as f32).min(size.height / h as f32);
            rune_components::centered(
                // Mirrored, as a webcam shows the person; not what is sent.
                Element::surface(rune_components::voice::ids::CAMERA_SURFACE, t("Tu cámara"))
                    .surface_mirrored(true)
                    .size(Some(w as f32 * scale), Some(h as f32 * scale)),
            )
        }
        None => rune_components::centered(line(
            if video.testing {
                t("Esperando la cámara…")
            } else {
                t("Prueba el video para verte aquí.")
            },
            14.0,
            look.muted,
        )),
    };
    let supported = cfg!(any(target_os = "linux", windows));
    let mut rows = vec![
        Element::spacer().size(None, Some(16.0)),
        line(t("Cámara"), 16.0, look.text).font_family(limn::FontFamily::SansMedium),
        select,
    ];
    if video.cameras.is_empty() {
        rows.push(line(
            t("Las cámaras aparecen cuando el sistema da acceso."),
            12.0,
            look.muted,
        ));
    }
    rows.extend([
        Element::panel(picture)
            .size(Some(size.width), Some(size.height))
            .radius(8.0)
            .background(Color::rgb(0, 0, 0))
            .clip_children(),
        Element::row(vec![
            parts::primary_button(
                "us:video-test",
                if video.testing {
                    t("Detener video")
                } else {
                    t("Probar video")
                },
                supported,
                look,
            ),
            Element::spacer().flex(1.0),
        ]),
        text(
            if supported {
                t("Vista previa local; no envía video. El sistema pide permiso la primera vez.")
            } else {
                t(rune_components::voice::NO_VIDEO)
            },
            13.0,
            look.muted,
        ),
    ]);
    Element::column(rows).gap(8.0)
}

/// When the microphone counts as voice: the room's level, which RUNE
/// follows, or one's own on a slider right under the level heard.
fn sensitivity(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let prefs = &ui.prefs;
    let width = ctx.geometry.column;
    let threshold = if prefs.speech_gate() {
        rune_media::VoiceActivity::GATED_ON
    } else if prefs.auto_sensitivity {
        ui.audio
            .threshold
            .unwrap_or(rune_media::VoiceActivity::default().on)
    } else {
        prefs.sensitivity
    };
    let level = ui
        .audio
        .testing
        .then(|| ui.audio.levels.last().copied())
        .flatten();
    let mut column = vec![parts::toggle(
        "us:auto-sensitivity",
        t("Determinar automáticamente la sensibilidad de entrada"),
        Some(if prefs.speech_gate() {
            t("Decide RNNoise: pasa tu voz, y el teclado, la respiración y el ruido de fondo no abren el micrófono.")
        } else if prefs.auto_sensitivity {
            t("RUNE sigue el ruido de la sala: tu voz tiene que superarlo por 15 dB.")
        } else {
            t("Arrastrá el umbral: lo que suene por encima cuenta como voz.")
        }),
        prefs.auto_sensitivity,
        None,
        look,
    )];
    // Set by hand, the threshold is the thumb of a slider that draws the
    // level on its track, as the official client's; otherwise the meter
    // marks the threshold RUNE follows.
    column.push(if prefs.auto_sensitivity {
        parts::sensitivity(level, threshold, width, look)
    } else {
        let level = level.unwrap_or(0.0);
        let fill = if level >= threshold {
            look.online
        } else {
            look.idle
        };
        Slider::new("us:sensitivity", prefs.sensitivity, look.palette())
            .width(width)
            .ink(look.accent)
            .level(level, fill)
            .accessibility_label(t("Umbral de voz"))
            .render()
    });
    let decibels = threshold * -rune_media::FLOOR_DBFS + rune_media::FLOOR_DBFS;
    let decibels = format!("{decibels:.0}");
    column.push(line(
        if prefs.speech_gate() {
            t("Umbral: lo que RNNoise oye como voz").to_owned()
        } else if prefs.auto_sensitivity {
            tr!("Umbral: {decibels} dBFS · automático", decibels)
        } else {
            tr!("Umbral: {decibels} dBFS", decibels)
        },
        12.0,
        look.muted,
    ));
    Element::column(column).gap(8.0)
}

/// A group of shortcuts, under its title.
/// What the provider can do, one line each.
fn capabilities(ctx: &Ctx) -> Option<Element> {
    let look = ctx.look;
    let info = ctx.store.provider.as_ref()?;
    let yes_no = |value: bool| if value { t("Sí") } else { "No" };
    let capabilities = info.capabilities;
    let voice = match capabilities.voice {
        VoiceSupport::None => "No",
        VoiceSupport::Simulated => t("Simulada: no se transmite audio"),
        VoiceSupport::Real => t("Sí"),
    };
    let rows = [
        (
            t("Inicio y cierre de sesión"),
            yes_no(capabilities.authentication),
        ),
        (
            t("Lista de servidores"),
            yes_no(capabilities.account_servers),
        ),
        (t("Enviar mensajes"), yes_no(capabilities.send_messages)),
        (
            t("Editar y borrar"),
            yes_no(capabilities.edit_messages && capabilities.delete_messages),
        ),
        (t("Reacciones"), yes_no(capabilities.reactions)),
        (t("Historial"), yes_no(capabilities.history)),
        (t("Mensajes directos"), yes_no(capabilities.direct_messages)),
        (t("Adjuntos"), yes_no(capabilities.attachments)),
        (
            t("Reenvío seguro (sin duplicar)"),
            yes_no(capabilities.idempotent_send),
        ),
        (t("Presencia"), yes_no(capabilities.presence)),
        (t("Editar el perfil"), yes_no(capabilities.edit_profile)),
        (t("Voz"), voice),
    ]
    .into_iter()
    .map(|(label, value)| {
        Element::row(vec![
            line(label, 15.0, look.text).flex(1.0),
            line(value, 15.0, look.muted),
        ])
        .gap(16.0)
        .padding(10.0)
    })
    .collect();
    Some(Element::column(vec![
        parts::subtitle(t("Lo que puede hacer"), look),
        Element::panel(Element::column(rows))
            .background(look.raised)
            .border(look.border, 1.0)
            .radius(8.0),
    ]))
}

/// The quick switcher over `root` while it is open.
pub fn switcher(ui: &Ui, store: &Store, root: Element, look: &Look, window: Size) -> Element {
    let Some(state) = &ui.switcher else {
        return root;
    };
    let width = Modal::fit_width(window.width, 560.0);
    let results: Vec<Element> = destinations(ui, store)
        .into_iter()
        .enumerate()
        .map(|(index, destination)| {
            let icon = match destination.kind {
                ChannelKind::Category => Icon::Server,
                ChannelKind::Direct => Icon::At,
                ChannelKind::Group => Icon::Users,
                _ => Icon::Hash,
            };
            ListRow::new(
                format!("switcher:go={}", destination.id),
                destination.title,
                look.menu_palette(),
            )
            .icon(icon)
            .subtitle(destination.detail)
            .filled()
            .title_ink(if index == state.selected {
                look.accent
            } else {
                look.text
            })
            .trailing_icon(if index == state.selected {
                Icon::ArrowRight
            } else {
                Icon::ChevronRight
            })
            .render()
        })
        .collect();
    let results = if results.is_empty() {
        vec![line(t("Nada coincide."), 13.0, look.muted)]
    } else {
        results
    };
    Modal::new(
        root,
        true,
        t("Ir a…"),
        t("Buscá un servidor, un canal o una conversación. ↑↓ para elegir, Enter para ir."),
        "switcher:close",
        look.menu_palette(),
    )
    .width(width)
    .body(
        Element::column(vec![
            SearchField::new(
                SWITCHER_QUERY,
                state.query.clone(),
                t("¿A dónde querés ir?"),
                look.menu_palette(),
            )
            .shortcut("Ctrl+K")
            .render(),
            ScrollArea::new("switcher:results", Element::column(results).gap(2.0))
                .render()
                .size(None, Some(320.0)),
        ])
        .gap(8.0),
    )
    .initial_focus(SWITCHER_QUERY)
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searching_folds_case_and_accents_and_wants_every_word() {
        assert!(found("ZOOM", &["Nivel de zoom"]));
        assert!(found("animacion", &["animaciones"]));
        assert!(found("tamaño texto", &["Tamaño de texto en el chat"]));
        assert!(!found("tamaño voz", &["Tamaño de texto en el chat"]));
    }

    #[test]
    fn every_section_key_is_unique_and_names_a_page_of_its_own() {
        let mut keys: Vec<&str> = sections().map(|section| section.key).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count);
        for page in SettingsPage::ALL {
            assert!(!section_keys(page).is_empty(), "{page:?} has sections");
        }
    }
}
