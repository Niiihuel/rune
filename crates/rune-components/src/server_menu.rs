//! The server's menu and the dialogs it opens, from plain props: the menu
//! under the server's name, a new channel or category (and who may see it
//! when it is private), the notification settings, the account's name in
//! the server, leaving it, and a new server from the rail's "+".

use limn::{
    ActionRow, AlertDialog, Button, ButtonVariant, Checkbox, Color, Component, ControlSize,
    DropdownMenu, Element, Icon, ListRow, Modal, RadioGroup, RadioOption, SearchField,
    SelectorSizing, Swatch, SwatchPicker, TextField,
};
use rune_i18n::{es, t, tr};
use rune_model::Rgb;

use crate::channel_menu::{dropdown_entries, Entry};
use crate::{avatar, centered, gap, line, strong_line, text, Look};

/// The actions these pieces report.
pub mod ids {
    /// The server's name at the top of the column: it opens the menu.
    pub const TOGGLE: &str = "sm:toggle";
    pub const DISMISS: &str = "sm:dismiss";

    pub const CHANNEL_CLOSE: &str = "newchan:close";
    pub const CHANNEL_TEXT: &str = "newchan:kind=text";
    pub const CHANNEL_VOICE: &str = "newchan:kind=voice";
    pub const CHANNEL_NAME: &str = "newchan:name";
    pub const CHANNEL_PRIVATE: &str = "newchan:private";
    pub const CHANNEL_SUBMIT: &str = "newchan:submit";
    pub const CHANNEL_BACK: &str = "newchan:back";
    pub const CHANNEL_QUERY: &str = "newchan:query";
    /// Followed by a role's ID.
    pub const CHANNEL_ROLE: &str = "newchan:role:";
    /// Followed by a person's ID.
    pub const CHANNEL_MEMBER: &str = "newchan:member:";

    pub const NOTIFY_CLOSE: &str = "sm:notify:close";
    pub const NOTIFY_MUTE: &str = "sm:notify:mute";
    /// Followed by `all`, `mentions` or `nothing`.
    pub const NOTIFY_LEVEL: &str = "sm:notify:level=";
    /// Followed by the key of one of [`super::MUTE_DURATIONS`].
    pub const NOTIFY_FOR: &str = "sm:notify:for=";
    /// The account's own: `@everyone` and roles stop counting as mentions.
    pub const NOTIFY_EVERYONE: &str = "sm:notify:everyone";
    pub const NOTIFY_ROLES: &str = "sm:notify:roles";

    pub const PRIVACY_CLOSE: &str = "sm:privacy:close";
    /// Followed by the server's number: whether its members may write.
    pub const PRIVACY_DMS: &str = "sm:privacy:dms-in:";
    /// Followed by the server's number: whether its messages are requests.
    pub const PRIVACY_REQUESTS: &str = "sm:privacy:requests-in:";

    pub const NICK: &str = "sm:nick";
    pub const NICK_SAVE: &str = "sm:nick:save";
    pub const NICK_CLOSE: &str = "sm:nick:close";

    pub const LEAVE_CONFIRM: &str = "sm:leave:confirm";
    pub const LEAVE_CANCEL: &str = "sm:leave:cancel";

    /// The rail's "+".
    pub const SERVER_OPEN: &str = "newsrv:open";
    pub const SERVER_OWN: &str = "newsrv:own";
    pub const SERVER_REVIEW_INVITATION: &str = "invitation:input-open";
    pub const SERVER_BACK: &str = "newsrv:back";
    pub const SERVER_NAME: &str = "newsrv:name";
    /// Followed by the color as six hex digits.
    pub const SERVER_COLOR: &str = "newsrv:color=";
    pub const SERVER_CREATE: &str = "newsrv:create";
    pub const SERVER_CLOSE: &str = "newsrv:close";
}

/// The menu's width, as measured under the server's name.
pub const MENU_WIDTH: f32 = 232.5;
/// A dialog's width when the window has room, and a confirmation's.
pub const DIALOG_WIDTH: f32 = 510.0;
pub const CONFIRM_WIDTH: f32 = 425.0;

/// How long a server can be muted for: the key its action ends with, what
/// the button says, and the milliseconds (`None` until turned back on).
pub const MUTE_DURATIONS: [(&str, &str, Option<i64>); 6] = [
    ("15m", "15 min", Some(15 * 60_000)),
    ("1h", "1 h", Some(60 * 60_000)),
    ("3h", "3 h", Some(3 * 60 * 60_000)),
    ("8h", "8 h", Some(8 * 60 * 60_000)),
    ("24h", "24 h", Some(24 * 60 * 60_000)),
    ("always", es("Siempre"), None),
];

/// The colors a new server's mark can take, with their names.
pub const SERVER_COLORS: [(Rgb, &str); 8] = [
    (Rgb(0xE0_A4_3C), es("Ámbar")),
    (Rgb(0x5E_6A_D2), es("Índigo")),
    (Rgb(0x3F_B9_7A), es("Verde")),
    (Rgb(0x3C_A6_8F), es("Turquesa")),
    (Rgb(0x4F_A3_D9), es("Celeste")),
    (Rgb(0xB3_6A_D9), es("Violeta")),
    (Rgb(0xE0_55_61), es("Rojo")),
    (Rgb(0x7A_85_99), es("Pizarra")),
];

/// A text channel's name as such names go: lowercase, a dash for each run
/// of spaces. "Prueba Canal" is `prueba-canal`.
pub fn channel_name(name: &str) -> String {
    name.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// The server's menu on `anchor`, its entries as the channel menus draw them
/// (groups apart by a line, submenus, checks), while `open`.
pub fn menu(anchor: Element, entries: Vec<Entry>, open: bool, look: &Look) -> Element {
    let entries = dropdown_entries(entries, look);
    let count = entries.len();
    DropdownMenu::new(anchor, entries, open, ids::DISMISS, look.menu_palette())
        .sizing(SelectorSizing {
            preferred_width: MENU_WIDTH,
            min_width: MENU_WIDTH,
            row_height: 38.0,
            // An item that cannot be used says why on a second line.
            detail_row_height: 50.0,
            icon_size: 20.0,
            label_size: 13.0,
            detail_size: 12.0,
            padding: 8.0,
            gap: 10.0,
            // Every item in view: the menu never scrolls.
            max_visible_items: count,
            ..SelectorSizing::COMPACT
        })
        .width(MENU_WIDTH)
        .render()
}

/// A new channel or category, as its dialog shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelForm {
    pub category: bool,
    pub voice: bool,
    /// What is written in the name field.
    pub name: String,
    pub private: bool,
    /// The second step of a private one: who may see it.
    pub audience: Option<Audience>,
}

impl ChannelForm {
    /// The name it will be made with.
    pub fn final_name(&self) -> String {
        if self.category || self.voice {
            self.name.trim().to_owned()
        } else {
            channel_name(&self.name)
        }
    }
}

/// Who may see a private channel: the roles and people to choose from.
#[derive(Clone, Debug, PartialEq)]
pub struct Audience {
    pub query: String,
    pub roles: Vec<Choice>,
    /// `None` while nothing is searched: people are found by name.
    pub members: Option<Vec<Choice>>,
}

/// A role or a person that can be chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub chosen: bool,
}

/// The dialog that makes a channel or a category, over `root`.
pub fn channel_dialog(root: Element, form: &ChannelForm, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let name = form.final_name();
    let made = if form.category {
        t("Crear categoría")
    } else {
        t("Crear un canal")
    };
    let (title, description, body, back, primary, enabled) = match &form.audience {
        None => (
            made,
            String::new(),
            channel_step(form, &name, look),
            (t("Cancelar"), ids::CHANNEL_CLOSE),
            if form.private { t("Siguiente") } else { made },
            !name.is_empty(),
        ),
        Some(audience) => {
            let anything = audience.roles.iter().any(|role| role.chosen)
                || audience
                    .members
                    .iter()
                    .flatten()
                    .any(|member| member.chosen);
            (
                t("Añadir miembros o roles"),
                if form.category || form.voice {
                    name.clone()
                } else {
                    format!("#{name}")
                },
                audience_picker(ids::CHANNEL_QUERY, audience, look),
                (t("Atrás"), ids::CHANNEL_BACK),
                if anything { made } else { t("Omitir") },
                true,
            )
        }
    };
    Modal::new(root, true, title, description, ids::CHANNEL_CLOSE, palette)
        .width(width)
        .body(body)
        .footer(buttons(back, (primary, ids::CHANNEL_SUBMIT, enabled), look))
        .initial_focus(if form.audience.is_some() {
            ids::CHANNEL_QUERY
        } else {
            ids::CHANNEL_NAME
        })
        .render()
}

/// The first step: kind, name and whether it is private.
fn channel_step(form: &ChannelForm, name: &str, look: &Look) -> Element {
    let palette = look.menu_palette();
    let mut parts = Vec::new();
    if !form.category {
        parts.push(label(t("Tipo de canal"), look));
        parts.push(
            RadioGroup::new(
                "newchan:kind",
                vec![
                    RadioOption::new(ids::CHANNEL_TEXT, t("Texto"))
                        .icon(Icon::Hash)
                        .description(t("Mensajes, imágenes, emojis y lo que quieras escribir")),
                    RadioOption::new(ids::CHANNEL_VOICE, t("Voz"))
                        .icon(Icon::Volume)
                        .description(t("Charlar por voz con quien esté en la sala")),
                ],
                Some(if form.voice {
                    ids::CHANNEL_VOICE
                } else {
                    ids::CHANNEL_TEXT
                }),
                palette,
            )
            .render(),
        );
        parts.push(gap(0.0, 4.0));
    }
    parts.push(label(
        if form.category {
            t("Nombre de la categoría")
        } else {
            t("Nombre del canal")
        },
        look,
    ));
    let (prefix, placeholder) = match (form.category, form.voice) {
        (true, _) => (None, t("Nueva categoría")),
        (false, true) => (Some(Icon::Volume), t("Canal nuevo")),
        (false, false) => (Some(Icon::Hash), t("canal-nuevo")),
    };
    parts.push(field(
        ids::CHANNEL_NAME,
        &form.name,
        placeholder,
        prefix,
        look,
    ));
    // Text channels take their names lowercase and dashed: say how it will be.
    if !form.category && !form.voice && !name.is_empty() && name != form.name {
        parts.push(line(tr!("Se va a llamar #{name}", name), 12.0, look.muted));
    }
    let (private, whom) = if form.category {
        (
            t("Categoría privada"),
            t("Solo los miembros y roles que elijas van a poder verla."),
        )
    } else {
        (
            t("Canal privado"),
            t("Solo los miembros y roles que elijas van a poder verlo."),
        )
    };
    parts.push(
        ActionRow::new(private, palette)
            .icon(Icon::Lock)
            .subtitle(whom)
            .trailing(
                limn::Switch::new(ids::CHANNEL_PRIVATE, form.private, palette)
                    .accessibility_label(private)
                    .render(),
            )
            .render(),
    );
    Element::column(parts).gap(8.0)
}

/// The roles and people that may see a private channel, to check, with the
/// field `query_id` that searches them: the second step of a new private
/// channel, and who may see one from its settings.
pub fn audience_picker(query_id: &str, audience: &Audience, look: &Look) -> Element {
    let palette = look.menu_palette();
    let choices = |choices: &[Choice]| -> Vec<Element> {
        choices
            .iter()
            .map(|choice| {
                Checkbox::new(choice.id.clone(), choice.chosen, palette)
                    .label(choice.label.clone())
                    .render()
            })
            .collect()
    };
    let mut parts = vec![
        SearchField::new(
            query_id,
            audience.query.clone(),
            t("Por ejemplo: moderación, @alguien"),
            palette,
        )
        .accessibility_label(t("Buscar roles o miembros"))
        .render(),
        line(
            t("Escribe @ para buscar miembros, o el nombre de un rol."),
            12.0,
            look.muted,
        ),
        gap(0.0, 4.0),
        label(t("Roles"), look),
    ];
    if audience.roles.is_empty() {
        parts.push(line(t("Ningún rol coincide."), 13.0, look.faint));
    }
    parts.extend(choices(&audience.roles));
    if let Some(members) = &audience.members {
        parts.push(gap(0.0, 4.0));
        parts.push(label(t("Miembros"), look));
        if members.is_empty() {
            parts.push(line(t("Nadie coincide."), 13.0, look.faint));
        }
        parts.extend(choices(members));
    }
    Element::column(parts).gap(8.0)
}

/// How much a muted server tells, as the notification settings offer it.
pub use rune_model::NotifyLevel;

/// A server's notification settings, as their dialog shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct NotifyForm {
    pub server: String,
    pub level: NotifyLevel,
    pub muted: bool,
    /// Until when it is muted, as a person reads it: "Hoy a las 18:30".
    /// `None` until it is turned back on.
    pub until: Option<String>,
    /// Kept with the account rather than on this machine.
    pub synced: bool,
    /// Whether `@everyone` and `@here`, and the account's roles, stop
    /// counting as mentions; `None` where they cannot be set.
    pub suppress_everyone: Option<bool>,
    pub suppress_roles: Option<bool>,
}

/// The notification settings of a server, over `root`.
pub fn notify_dialog(root: Element, form: &NotifyForm, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let mute_title = tr!("Silenciar {server}", server = form.server);
    let mut parts = vec![
        text(
            if form.synced {
                t(
                    "Se guardan en tu cuenta y se ven igual en tus otros dispositivos. \
                 Cambian lo que se marca en la barra de servidores y qué te avisa.",
                )
            } else {
                t(
                    "Se guardan en este equipo. Cambian lo que se marca en la barra de \
                 servidores y qué te avisa.",
                )
            },
            12.0,
            look.muted,
        ),
        ActionRow::new(mute_title.clone(), palette)
            .subtitle(t(
                "Al silenciar un servidor no se marcan sus mensajes sin leer, salvo \
                 cuando te mencionan.",
            ))
            .trailing(
                limn::Switch::new(ids::NOTIFY_MUTE, form.muted, palette)
                    .accessibility_label(mute_title)
                    .render(),
            )
            .render(),
    ];
    if form.muted {
        let until = match &form.until {
            Some(until) => tr!("Silenciado hasta: {until}.", until),
            None => t("Silenciado hasta que lo vuelvas a activar.").into(),
        };
        parts.push(line(until, 12.0, look.muted));
        parts.push(
            Element::row(
                MUTE_DURATIONS
                    .iter()
                    .map(|(key, label, _)| {
                        Button::new(format!("{}{key}", ids::NOTIFY_FOR), t(label), palette)
                            .variant(ButtonVariant::Outline)
                            .size(ControlSize::Compact)
                            .render()
                    })
                    .collect(),
            )
            .gap(6.0),
        );
    }
    parts.push(separator(look));
    parts.push(label(t("Ajustes de las notificaciones del servidor"), look));
    parts.push(
        RadioGroup::new(
            "sm:notify:level",
            [
                (
                    NotifyLevel::All,
                    t("Todos los mensajes"),
                    t("Se marca todo lo que no leíste."),
                ),
                (
                    NotifyLevel::Mentions,
                    t("Solo @menciones"),
                    t("Solo se marcan las menciones; los mensajes nuevos, no."),
                ),
                (
                    NotifyLevel::Nothing,
                    t("Nada"),
                    t("No se marca nada, ni siquiera las menciones."),
                ),
            ]
            .into_iter()
            .map(|(level, title, description)| {
                RadioOption::new(format!("{}{}", ids::NOTIFY_LEVEL, level.key()), title)
                    .description(description)
            })
            .collect(),
            Some(&format!("{}{}", ids::NOTIFY_LEVEL, form.level.key())),
            palette,
        )
        .render(),
    );
    parts.push(separator(look));
    for (title, id, on, note) in [
        (
            t("Silenciar @everyone y @here"),
            ids::NOTIFY_EVERYONE,
            form.suppress_everyone,
            t("No cuentan como menciones ni te avisan."),
        ),
        (
            t("Silenciar las @menciones de roles"),
            ids::NOTIFY_ROLES,
            form.suppress_roles,
            t("No cuentan como menciones ni te avisan."),
        ),
    ] {
        parts.push(match on {
            Some(on) => ActionRow::new(title, palette)
                .subtitle(note)
                .trailing(
                    limn::Switch::new(id, on, palette)
                        .accessibility_label(title)
                        .render(),
                )
                .render(),
            // What the domain cannot tell apart is shown, and why it is off.
            None => ActionRow::new(title, palette)
                .subtitle(t(
                    "Este proveedor no distingue las menciones a todos ni a roles.",
                ))
                .trailing(limn::Switch::new("", false, palette).disabled().render())
                .disabled()
                .render(),
        });
    }
    Modal::new(
        root,
        true,
        t("Ajustes de notificaciones"),
        "",
        ids::NOTIFY_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(parts).gap(10.0))
    .footer(
        Button::new(ids::NOTIFY_CLOSE, t("Listo"), look.accent_palette())
            .size(ControlSize::Spacious)
            .render(),
    )
    .initial_focus(ids::NOTIFY_MUTE)
    .render()
}

/// A server's privacy settings, as their dialog shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct PrivacyForm {
    pub server: String,
    /// The server's number, in its switches' ids.
    pub id: u64,
    pub dms: bool,
    pub requests: bool,
    /// Why the switches cannot change, if they cannot.
    pub denied: Option<String>,
}

/// The privacy settings of a server, over `root`.
pub fn privacy_dialog(root: Element, form: &PrivacyForm, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let enabled = form.denied.is_none();
    let row = |id: String, title: &str, note: &str, on: bool| {
        ActionRow::new(title, palette)
            .subtitle(note)
            .trailing(
                limn::Switch::new(id, on, palette)
                    .accessibility_label(title)
                    .enabled(enabled)
                    .render(),
            )
            .render()
    };
    let mut parts = vec![
        row(
            format!("{}{}", ids::PRIVACY_DMS, form.id),
            t("Mensajes directos"),
            t("Permite que los miembros de este servidor te escriban directamente."),
            form.dms,
        ),
        separator(look),
        row(
            format!("{}{}", ids::PRIVACY_REQUESTS, form.id),
            t("Solicitudes de mensajes"),
            t("Los mensajes de miembros de este servidor que no son tus amigos llegan a Solicitudes."),
            form.requests,
        ),
    ];
    if let Some(reason) = &form.denied {
        parts.push(line(reason, 12.0, look.muted));
    }
    Modal::new(
        root,
        true,
        t("Ajustes de privacidad"),
        &form.server,
        ids::PRIVACY_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(parts).gap(10.0))
    .footer(
        Button::new(ids::PRIVACY_CLOSE, t("Listo"), look.accent_palette())
            .size(ControlSize::Spacious)
            .render(),
    )
    .render()
}

/// The account's name in one server, as its dialog shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct NickForm {
    pub server: String,
    /// What is written in the field.
    pub nick: String,
    /// The account's own name, shown while the field is empty.
    pub own_name: String,
    pub handle: String,
    pub accent: Color,
    /// The color of its highest role here.
    pub ink: Color,
    /// Why the name cannot be changed here, when it cannot.
    pub denied: Option<String>,
    /// Something changed, and it can be kept.
    pub can_save: bool,
}

/// The longest a name in a server can be.
pub const NICK_MAX: usize = 32;

/// The account's profile in a server: its name there, and how it shows.
pub fn nick_dialog(root: Element, form: &NickForm, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let shown = if form.nick.trim().is_empty() {
        form.own_name.clone()
    } else {
        form.nick.trim().to_owned()
    };
    let preview = Element::panel(
        Element::row(vec![
            avatar(&shown, form.accent, 48.0).centered_y(),
            Element::column(vec![
                strong_line(&shown, 16.0, form.ink),
                line(format!("@{}", form.handle), 13.0, look.muted),
            ])
            .gap(2.0)
            .flex(1.0)
            .centered_y(),
        ])
        .gap(12.0),
    )
    .padding(12.0)
    .radius(8.0)
    .background(look.canvas);
    let field = TextField::new(ids::NICK, form.nick.clone(), form.own_name.clone(), palette)
        .accessibility_label(t("Apodo"))
        .size(ControlSize::Spacious);
    let mut parts = vec![
        label(t("Así te ven en este servidor"), look),
        preview,
        gap(0.0, 4.0),
        label(t("Apodo"), look),
        if form.denied.is_some() {
            field.disabled()
        } else {
            field
        }
        .render(),
    ];
    let count = form.nick.trim().chars().count();
    parts.push(match &form.denied {
        Some(reason) => text(reason.clone(), 12.0, look.danger),
        None if count > NICK_MAX => text(
            tr!(
                "Tiene {count} caracteres; el máximo es {max}.",
                count,
                max = NICK_MAX
            ),
            12.0,
            look.danger,
        ),
        None => text(
            tr!(
                "Solo cambia en {server}. Vacío, se usa tu nombre de siempre.",
                server = form.server
            ),
            12.0,
            look.muted,
        ),
    });
    Modal::new(
        root,
        true,
        t("Editar perfil de servidor"),
        form.server.clone(),
        ids::NICK_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(parts).gap(8.0))
    .footer(buttons(
        (t("Cancelar"), ids::NICK_CLOSE),
        (t("Guardar"), ids::NICK_SAVE, form.can_save),
        look,
    ))
    .initial_focus(ids::NICK)
    .render()
}

/// The question before leaving `server`, over `root`.
pub fn leave_dialog(root: Element, server: &str, look: &Look) -> Element {
    AlertDialog::new(
        root,
        true,
        tr!("Abandonar «{server}»", server),
        t(
            "Vas a dejar de ver sus canales y sus mensajes. Para volver, alguien tiene que \
         invitarte otra vez.",
        ),
        ids::LEAVE_CONFIRM,
        ids::LEAVE_CANCEL,
        look.menu_palette(),
    )
    .width(CONFIRM_WIDTH)
    .confirm_label(t("Abandonar servidor"))
    .destructive(look.danger)
    .render()
}

/// A new server being made: `None` while choosing how to start.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerForm {
    pub name: String,
    pub accent: Rgb,
}

/// Account-level actions, with provider-independent reasons for unavailable ones.
#[derive(Default)]
pub struct ServerActions {
    pub create_denied: Option<String>,
    pub invitation_denied: Option<String>,
}

/// The dialog that makes a server, over `root`.
pub fn server_dialog(
    root: Element,
    form: Option<&ServerForm>,
    actions: &ServerActions,
    width: f32,
    look: &Look,
) -> Element {
    let palette = look.menu_palette();
    let Some(form) = form else {
        return Modal::new(
            root,
            true,
            t("Añadir un servidor"),
            t("Crea uno o revisa un enlace de invitación."),
            ids::SERVER_CLOSE,
            palette,
        )
        .width(width)
        .body(
            Element::column(vec![
                ListRow::new(ids::SERVER_OWN, t("Crear el mío"), palette)
                    .icon(Icon::Server)
                    .subtitle(
                        actions
                            .create_denied
                            .as_deref()
                            .unwrap_or(t("Elige un nombre y un color")),
                    )
                    .enabled(actions.create_denied.is_none())
                    .trailing_icon(Icon::ChevronRight)
                    .filled()
                    .render(),
                ListRow::new(
                    ids::SERVER_REVIEW_INVITATION,
                    t("Revisar una invitación"),
                    palette,
                )
                .icon(Icon::Server)
                .subtitle(
                    actions
                        .invitation_denied
                        .as_deref()
                        .unwrap_or(t("Pega un enlace para ver el destino")),
                )
                .enabled(actions.invitation_denied.is_none())
                .trailing_icon(Icon::ChevronRight)
                .filled()
                .render(),
            ])
            .gap(10.0),
        )
        .initial_focus(if actions.create_denied.is_none() {
            ids::SERVER_OWN
        } else if actions.invitation_denied.is_none() {
            ids::SERVER_REVIEW_INVITATION
        } else {
            ids::SERVER_CLOSE
        })
        .render();
    };
    let ink = look.ink(form.accent);
    let initials: String = form
        .name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    let mark = Element::panel(centered(
        Element::text(initials)
            .font_family(limn::FontFamily::SansBold)
            .font_size(26.0)
            .foreground(Color::rgb(255, 255, 255)),
    ))
    .size(Some(80.0), Some(80.0))
    .radius(40.0)
    .background(ink);
    let color_id = |color: Rgb| format!("{}{:06X}", ids::SERVER_COLOR, color.0);
    let swatches = SwatchPicker::new(
        "newsrv:colors",
        SERVER_COLORS
            .iter()
            .map(|(color, name)| Swatch::new(color_id(*color), look.ink(*color), t(name)))
            .collect(),
        Some(&color_id(form.accent)),
        palette,
    )
    .accessibility_label(t("Color del servidor"))
    .render();
    let mut body = vec![
        Element::row(vec![
            Element::spacer().flex(1.0),
            mark,
            Element::spacer().flex(1.0),
        ]),
        Element::row(vec![
            Element::spacer().flex(1.0),
            swatches,
            Element::spacer().flex(1.0),
        ]),
        gap(0.0, 4.0),
        label(t("Nombre del servidor"), look),
        TextField::new(
            ids::SERVER_NAME,
            form.name.clone(),
            t("Mi servidor"),
            palette,
        )
        .accessibility_label(t("Nombre del servidor"))
        .size(ControlSize::Spacious)
        .render(),
    ];
    if let Some(reason) = &actions.create_denied {
        body.push(text(reason, 13.0, look.muted));
    }
    Modal::new(
        root,
        true,
        t("Personaliza tu servidor"),
        t("Dale un nombre y un color. Puedes cambiarlos después."),
        ids::SERVER_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(10.0))
    .footer(Element::row(vec![
        Button::new(ids::SERVER_BACK, t("Atrás"), palette)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Spacious)
            .render(),
        Element::spacer().flex(1.0),
        Button::new(ids::SERVER_CREATE, t("Crear"), look.accent_palette())
            .size(ControlSize::Spacious)
            .enabled(actions.create_denied.is_none() && !form.name.trim().is_empty())
            .render(),
    ]))
    .initial_focus(ids::SERVER_NAME)
    .render()
}

/// A dialog's two buttons, side by side: the way back and the action.
pub(crate) fn buttons(back: (&str, &str), primary: (&str, &str, bool), look: &Look) -> Element {
    let (label, id, enabled) = primary;
    Element::row(vec![
        Button::new(back.1, back.0, look.menu_palette())
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render()
            .flex(1.0),
        Button::new(id, label, look.accent_palette())
            .size(ControlSize::Spacious)
            .enabled(enabled)
            .render()
            .flex(1.0),
    ])
    .gap(9.0)
}

/// The title over a part of a dialog.
pub(crate) fn label(value: &str, look: &Look) -> Element {
    strong_line(value, 14.0, look.text)
}

/// A field with a mark before what is written, such as the `#` of a channel.
fn field(id: &str, value: &str, placeholder: &str, prefix: Option<Icon>, look: &Look) -> Element {
    let input = Element::text_input(id, value, placeholder);
    let input = match prefix {
        Some(prefix) => input.leading_icon(prefix).leading_icon_tint(look.muted),
        None => input,
    };
    input
        .background(look.raised)
        .border(look.border, 1.0)
        .focus_border(look.accent, 2.0)
        .foreground(look.text)
        .font_family(limn::FontFamily::Sans)
        .font_size(15.0)
        .padding(10.0)
        .radius(8.0)
        .size(None, Some(40.0))
        .accessibility_label(placeholder)
}

fn separator(look: &Look) -> Element {
    Element::panel(Element::spacer())
        .size(None, Some(1.0))
        .background(look.border)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_menu::{grouped, Item};
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    const WINDOW: Size = Size::new(900.0, 800.0);

    #[test]
    fn a_text_channel_name_is_lowercase_and_dashed() {
        assert_eq!(channel_name("Prueba Canal"), "prueba-canal");
        assert_eq!(channel_name("  Dos   Espacios "), "dos-espacios");
        assert_eq!(channel_name("ya-bien"), "ya-bien");
        assert_eq!(channel_name("   "), "");
    }

    fn item(id: &str, denied: Option<&str>) -> Vec<Entry> {
        vec![Item::new(id, format!("Ítem {id}"))
            .denied(denied.map(str::to_owned))
            .into()]
    }

    #[test]
    fn a_denied_item_says_why_and_cannot_be_chosen() {
        let anchor = Element::clickable(ids::TOGGLE, Element::spacer()).size(None, Some(40.0));
        let menu = menu(
            anchor,
            grouped(vec![
                [item("a", None), item("b", Some("No tienes permiso"))].concat(),
                item("c", None),
            ]),
            true,
            &Look::DARK,
        );
        let (_, frame) = frame(
            Element::column(vec![
                Element::row(vec![
                    menu.size(Some(236.0), None),
                    Element::spacer().flex(1.0),
                ]),
                Element::spacer().flex(1.0),
            ]),
            WINDOW,
        );
        let ids = controls(&frame);
        assert!(ids.contains(&"a".to_owned()) && ids.contains(&"c".to_owned()));
        assert!(!ids.contains(&"b".to_owned()), "{ids:?}");
        assert!(texts(&frame).contains(&"No tienes permiso".to_owned()));
        let denied = frame.access.iter().find(|node| node.id == "b").unwrap();
        assert!(denied.disabled && denied.description.as_deref() == Some("No tienes permiso"));
    }

    fn form(name: &str, private: bool) -> ChannelForm {
        ChannelForm {
            category: false,
            voice: false,
            name: name.into(),
            private,
            audience: None,
        }
    }

    #[test]
    fn a_channel_cannot_be_made_without_a_name_and_says_how_it_will_be_called() {
        let (_, empty) = frame(
            channel_dialog(Element::spacer(), &form("", false), 510.0, &Look::DARK),
            WINDOW,
        );
        assert!(!controls(&empty).contains(&ids::CHANNEL_SUBMIT.to_owned()));
        assert!(texts(&empty).contains(&"Tipo de canal".to_owned()));
        let (_, named) = frame(
            channel_dialog(
                Element::spacer(),
                &form("Prueba Canal", false),
                510.0,
                &Look::DARK,
            ),
            WINDOW,
        );
        assert!(controls(&named).contains(&ids::CHANNEL_SUBMIT.to_owned()));
        assert!(texts(&named).contains(&"Se va a llamar #prueba-canal".to_owned()));
        let (_, private) = frame(
            channel_dialog(Element::spacer(), &form("x", true), 510.0, &Look::DARK),
            WINDOW,
        );
        assert!(texts(&private).contains(&"Siguiente".to_owned()));
    }

    #[test]
    fn the_second_step_offers_roles_and_skips_until_one_is_chosen() {
        let mut private = form("secreto", true);
        private.audience = Some(Audience {
            query: String::new(),
            roles: vec![Choice {
                id: format!("{}7", ids::CHANNEL_ROLE),
                label: "Moderación".into(),
                chosen: false,
            }],
            members: None,
        });
        let (_, frame) = frame(
            channel_dialog(Element::spacer(), &private, 510.0, &Look::DARK),
            WINDOW,
        );
        let painted = texts(&frame);
        assert!(painted.contains(&"#secreto".to_owned()), "{painted:?}");
        assert!(painted.contains(&"Omitir".to_owned()) && painted.contains(&"Atrás".to_owned()));
        assert!(controls(&frame).contains(&"newchan:role:7".to_owned()));
    }

    #[test]
    fn a_new_server_needs_a_name_and_offers_its_colors() {
        let form = ServerForm {
            name: " ".into(),
            accent: SERVER_COLORS[1].0,
        };
        let (_, frame) = frame(
            server_dialog(
                Element::spacer(),
                Some(&form),
                &ServerActions::default(),
                510.0,
                &Look::DARK,
            ),
            WINDOW,
        );
        let ids = controls(&frame);
        assert!(!ids.contains(&ids::SERVER_CREATE.to_owned()));
        assert_eq!(
            ids.iter()
                .filter(|id| id.starts_with(ids::SERVER_COLOR))
                .count(),
            SERVER_COLORS.len()
        );
        let chosen = frame
            .access
            .iter()
            .find(|node| node.id == "newsrv:color=5E6AD2")
            .unwrap();
        assert_eq!(chosen.selected, Some(true));
    }

    #[test]
    fn a_denied_nick_disables_the_field_and_says_why() {
        let form = NickForm {
            server: "Taller".into(),
            nick: String::new(),
            own_name: "Invitado".into(),
            handle: "invitado".into(),
            accent: Color::rgb(1, 2, 3),
            ink: Color::rgb(200, 200, 200),
            denied: Some("No tienes permiso para cambiar tu apodo en Taller".into()),
            can_save: false,
        };
        let (_, frame) = frame(
            nick_dialog(Element::spacer(), &form, 510.0, &Look::DARK),
            WINDOW,
        );
        let ids = controls(&frame);
        assert!(!ids.contains(&ids::NICK.to_owned()) && !ids.contains(&ids::NICK_SAVE.to_owned()));
        let painted = texts(&frame);
        assert!(painted.contains(&"No tienes permiso para cambiar tu apodo en Taller".to_owned()));
        assert!(
            painted.contains(&"Invitado".to_owned()),
            "the preview shows the own name"
        );
    }
}
