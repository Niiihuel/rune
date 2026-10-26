//! The settings of one channel or category, on the frame of the server's
//! settings ([`crate::server_settings`]: sidebar, close button, bar of
//! unsaved changes, permission rows): what an overwrite can allow or deny for
//! each kind of channel, the control of three states that sets one
//! permission, the cards of the permissions page (in sync with the category,
//! private), and the list of roles and members with overwrites with the
//! popout that adds one. Pure functions of props; `rune-ui` keeps the drafts.
//!
//! Measures follow the reference captures at 1361×797 logical pixels: in
//! the 705 px page column, a 223 px list of roles and members and the
//! permissions 24.5 px right of it, each with a 101×28 control.

use limn::{
    Access, Button, ButtonSize, ButtonVariant, Color, Component, ControlSize, DropdownOption,
    Element, FontFamily, Icon, Popover, PopoverPlacement, Role, ScrollArea, ScrollbarAppearance,
    Select, SelectVariant, Switch, TextField, TextSpan,
};
use rune_i18n::{es, t, tr};
use rune_model::{ChannelKind, Permissions};

use crate::server_settings::{color_dot, context_menu, MenuAction, PermissionInfo};
use crate::{centered, gap, line, strong_line, text, Look};

/// How long slow mode makes each member wait between two messages, and what
/// the choice says (Spanish: show it through `t`). The provider takes up to
/// six hours.
pub const SLOWMODE: [(u32, &str); 14] = [
    (0, es("Apagado")),
    (5, es("5 segundos")),
    (10, es("10 segundos")),
    (15, es("15 segundos")),
    (30, es("30 segundos")),
    (60, es("1 minuto")),
    (120, es("2 minutos")),
    (300, es("5 minutos")),
    (600, es("10 minutos")),
    (900, es("15 minutos")),
    (1_800, es("30 minutos")),
    (3_600, es("1 hora")),
    (7_200, es("2 horas")),
    (21_600, es("6 horas")),
];

/// What slow mode of `secs` is called, translated: one of [`SLOWMODE`], or
/// the seconds when a provider set a wait the list does not have.
pub fn slowmode_label(secs: u32) -> String {
    SLOWMODE
        .iter()
        .find(|(value, _)| *value == secs)
        .map_or_else(
            || tr!("{seconds} segundos", seconds = secs),
            |(_, label)| t(label).to_owned(),
        )
}

/// The most a channel's topic holds, in characters.
pub const TOPIC_LIMIT: usize = 1024;

/// The list of roles and members with overwrites, and the space after it.
pub const TARGET_LIST_WIDTH: f32 = 223.0;
pub const TARGET_GAP: f32 = 24.5;
/// The control of three states.
pub const TRI_STATE_WIDTH: f32 = 101.0;
pub const TRI_STATE_HEIGHT: f32 = 28.0;

type Groups = &'static [(&'static str, &'static [PermissionInfo])];

/// What an overwrite of a text channel can allow or deny.
const TEXT: Groups = &[
    (
        es("Permisos generales de los canales"),
        &[
            (
                Permissions::VIEW,
                es("Ver el canal"),
                es("Deja que los miembros vean este canal."),
            ),
            (
                Permissions::MANAGE_CHANNELS,
                es("Gestionar canal"),
                es(
                    "Deja cambiar el nombre, el tema y los ajustes de este canal, y también eliminarlo.",
                ),
            ),
            (
                Permissions::MANAGE_ROLES,
                es("Gestionar permisos"),
                es("Deja cambiar los permisos de este canal."),
            ),
        ],
    ),
    (
        es("Permisos de membresía"),
        &[(
            Permissions::CREATE_INVITE,
            es("Crear invitación"),
            es("Deja invitar gente nueva al servidor con un enlace que lleva a este canal."),
        )],
    ),
    (
        es("Permisos de los canales de texto"),
        &[
            (
                Permissions::SEND,
                es("Enviar mensajes"),
                es("Deja escribir en este canal."),
            ),
            (
                Permissions::SEND_IN_THREADS,
                es("Enviar mensajes en hilos"),
                es("Deja escribir en los hilos de este canal."),
            ),
            (
                Permissions::CREATE_THREADS,
                es("Crear hilos públicos"),
                es("Deja abrir hilos que todos en este canal pueden ver."),
            ),
            (
                Permissions::CREATE_PRIVATE_THREADS,
                es("Crear hilos privados"),
                es("Deja abrir hilos que solo ven quienes son invitados y la moderación."),
            ),
            (
                Permissions::EMBED_LINKS,
                es("Insertar enlaces"),
                es("Los enlaces que se compartan aquí muestran una vista previa."),
            ),
            (
                Permissions::ATTACH,
                es("Adjuntar archivos"),
                es("Deja subir archivos e imágenes a este canal."),
            ),
            (
                Permissions::SEND_POLLS,
                es("Crear encuestas"),
                es("Deja empezar encuestas en este canal."),
            ),
            (
                Permissions::REACT,
                es("Añadir reacciones"),
                es(
                    "Deja poner reacciones nuevas a los mensajes de este canal. Sin este permiso se puede sumar a las que ya están.",
                ),
            ),
            (
                Permissions::EXTERNAL_EMOJI,
                es("Usar emojis externos"),
                es("Deja usar en este canal emojis de otros servidores."),
            ),
            (
                Permissions::EXTERNAL_STICKERS,
                es("Usar stickers externos"),
                es("Deja enviar en este canal stickers de otros servidores."),
            ),
            (
                Permissions::MENTION_EVERYONE,
                es("Mencionar a @everyone, @here y a todos los roles"),
                es(
                    "Deja avisar a todo el canal o a quienes estén en línea, y mencionar cualquier rol aunque no sea mencionable.",
                ),
            ),
            (
                Permissions::MANAGE_MESSAGES,
                es("Gestionar mensajes"),
                es("Deja borrar mensajes de otros miembros en este canal."),
            ),
            (
                Permissions::PIN_MESSAGES,
                es("Fijar mensajes"),
                es("Deja fijar y desfijar mensajes en este canal."),
            ),
            (
                Permissions::MANAGE_THREADS,
                es("Gestionar hilos"),
                es("Deja renombrar, cerrar y borrar los hilos de este canal, y ver los privados."),
            ),
            (
                Permissions::READ_HISTORY,
                es("Leer el historial de mensajes"),
                es(
                    "Deja leer lo que se escribió antes. Sin este permiso solo se ve lo que llega mientras se está en línea.",
                ),
            ),
        ],
    ),
];

/// What an overwrite of a voice channel can allow or deny. Its chat is left
/// out: voice channels hold no messages here.
const VOICE: Groups = &[
    (
        es("Permisos generales de los canales"),
        &[
            (
                Permissions::VIEW,
                es("Ver el canal"),
                es("Deja que los miembros vean este canal de voz."),
            ),
            (
                Permissions::MANAGE_CHANNELS,
                es("Gestionar canal"),
                es(
                    "Deja cambiar el nombre y los ajustes de este canal de voz, y también eliminarlo.",
                ),
            ),
            (
                Permissions::MANAGE_ROLES,
                es("Gestionar permisos"),
                es("Deja cambiar los permisos de este canal."),
            ),
        ],
    ),
    (
        es("Permisos de membresía"),
        &[(
            Permissions::CREATE_INVITE,
            es("Crear invitación"),
            es("Deja invitar gente nueva al servidor con un enlace que lleva a este canal de voz."),
        )],
    ),
    (
        es("Permisos de los canales de voz"),
        &[
            (
                Permissions::CONNECT,
                es("Conectar"),
                es("Deja entrar a este canal de voz y escuchar."),
            ),
            (
                Permissions::SPEAK,
                es("Hablar"),
                es(
                    "Deja hablar en este canal. Sin este permiso se entra silenciado hasta que alguien con «Silenciar miembros» lo permita.",
                ),
            ),
            (
                Permissions::STREAM,
                es("Video"),
                es("Deja compartir la cámara o la pantalla en este canal."),
            ),
            (
                Permissions::USE_VOICE_ACTIVITY,
                es("Usar Actividad de voz"),
                es(
                    "Deja hablar sin pulsar una tecla. Sin este permiso hay que usar pulsar para hablar.",
                ),
            ),
            (
                Permissions::PRIORITY_SPEAKER,
                es("Prioridad de palabra"),
                es("Mientras habla, baja el volumen de quienes no tienen este permiso."),
            ),
            (
                Permissions::MUTE_MEMBERS,
                es("Silenciar miembros"),
                es("Deja silenciar a otros en este canal, para todos."),
            ),
            (
                Permissions::DEAFEN_MEMBERS,
                es("Ensordecer miembros"),
                es("Deja que otros no puedan hablar ni escuchar en este canal."),
            ),
            (
                Permissions::MOVE_MEMBERS,
                es("Mover miembros"),
                es(
                    "Deja desconectar a otros de este canal o llevarlos a otro al que se tenga acceso.",
                ),
            ),
            (
                Permissions::REQUEST_TO_SPEAK,
                es("Pedir la palabra"),
                es("Deja levantar la mano para hablar, si este canal es un escenario."),
            ),
        ],
    ),
];

/// What an overwrite of a category can allow or deny: for every channel of
/// it, of either kind.
const CATEGORY: Groups = &[
    (
        es("Permisos generales de las categorías"),
        &[
            (
                Permissions::VIEW,
                es("Ver canales"),
                es("Deja que los miembros vean los canales de esta categoría."),
            ),
            (
                Permissions::MANAGE_CHANNELS,
                es("Gestionar canales"),
                es("Deja cambiar el nombre y los ajustes de estos canales, y también eliminarlos."),
            ),
            (
                Permissions::MANAGE_ROLES,
                es("Gestionar permisos"),
                es("Deja cambiar los permisos de estos canales."),
            ),
        ],
    ),
    (
        es("Permisos de membresía"),
        &[(
            Permissions::CREATE_INVITE,
            es("Crear invitación"),
            es("Deja invitar gente nueva al servidor con un enlace que lleva a estos canales."),
        )],
    ),
    (
        es("Permisos de los canales de texto"),
        &[
            (
                Permissions::SEND,
                es("Enviar mensajes"),
                es("Deja escribir en estos canales."),
            ),
            (
                Permissions::SEND_IN_THREADS,
                es("Enviar mensajes en hilos"),
                es("Deja escribir en los hilos de estos canales."),
            ),
            (
                Permissions::CREATE_THREADS,
                es("Crear hilos públicos"),
                es("Deja abrir hilos que todos en estos canales pueden ver."),
            ),
            (
                Permissions::CREATE_PRIVATE_THREADS,
                es("Crear hilos privados"),
                es("Deja abrir hilos que solo ven quienes son invitados y la moderación."),
            ),
            (
                Permissions::EMBED_LINKS,
                es("Insertar enlaces"),
                es("Los enlaces que se compartan en estos canales muestran una vista previa."),
            ),
            (
                Permissions::ATTACH,
                es("Adjuntar archivos"),
                es("Deja subir archivos e imágenes a estos canales."),
            ),
            (
                Permissions::SEND_POLLS,
                es("Crear encuestas"),
                es("Deja empezar encuestas en estos canales."),
            ),
            (
                Permissions::REACT,
                es("Añadir reacciones"),
                es(
                    "Deja poner reacciones nuevas a los mensajes de estos canales. Sin este permiso se puede sumar a las que ya están.",
                ),
            ),
            (
                Permissions::EXTERNAL_EMOJI,
                es("Usar emojis externos"),
                es("Deja usar en estos canales emojis de otros servidores."),
            ),
            (
                Permissions::EXTERNAL_STICKERS,
                es("Usar stickers externos"),
                es("Deja enviar en estos canales stickers de otros servidores."),
            ),
            (
                Permissions::MENTION_EVERYONE,
                es("Mencionar a @everyone, @here y a todos los roles"),
                es(
                    "Deja avisar a todos o a quienes estén en línea, y mencionar cualquier rol aunque no sea mencionable, en estos canales.",
                ),
            ),
            (
                Permissions::MANAGE_MESSAGES,
                es("Gestionar mensajes"),
                es("Deja borrar mensajes de otros miembros en estos canales."),
            ),
            (
                Permissions::PIN_MESSAGES,
                es("Fijar mensajes"),
                es("Deja fijar y desfijar mensajes en estos canales."),
            ),
            (
                Permissions::MANAGE_THREADS,
                es("Gestionar hilos"),
                es(
                    "Deja renombrar, cerrar y borrar los hilos de estos canales, y ver los privados.",
                ),
            ),
            (
                Permissions::READ_HISTORY,
                es("Leer el historial de mensajes"),
                es(
                    "Deja leer lo que se escribió antes en estos canales. Sin este permiso solo se ve lo que llega mientras se está en línea.",
                ),
            ),
        ],
    ),
    (
        es("Permisos de los canales de voz"),
        &[
            (
                Permissions::CONNECT,
                es("Conectar"),
                es("Deja entrar a estos canales de voz y escuchar."),
            ),
            (
                Permissions::SPEAK,
                es("Hablar"),
                es(
                    "Deja hablar en estos canales. Sin este permiso se entra silenciado hasta que alguien con «Silenciar miembros» lo permita.",
                ),
            ),
            (
                Permissions::STREAM,
                es("Video"),
                es("Deja compartir la cámara o la pantalla en estos canales."),
            ),
            (
                Permissions::USE_VOICE_ACTIVITY,
                es("Usar Actividad de voz"),
                es(
                    "Deja hablar sin pulsar una tecla. Sin este permiso hay que usar pulsar para hablar.",
                ),
            ),
            (
                Permissions::PRIORITY_SPEAKER,
                es("Prioridad de palabra"),
                es("Mientras habla, baja el volumen de quienes no tienen este permiso."),
            ),
            (
                Permissions::MUTE_MEMBERS,
                es("Silenciar miembros"),
                es("Deja silenciar a otros en estos canales, para todos."),
            ),
            (
                Permissions::DEAFEN_MEMBERS,
                es("Ensordecer miembros"),
                es("Deja que otros no puedan hablar ni escuchar en estos canales."),
            ),
            (
                Permissions::MOVE_MEMBERS,
                es("Mover miembros"),
                es(
                    "Deja desconectar a otros de estos canales o llevarlos a otro al que se tenga acceso.",
                ),
            ),
            (
                Permissions::REQUEST_TO_SPEAK,
                es("Pedir la palabra"),
                es("Deja levantar la mano para hablar en los escenarios."),
            ),
        ],
    ),
];

/// The permissions an overwrite of a `kind` channel sets, grouped and named
/// as the settings show them; their texts are Spanish, shown through `t`.
pub fn permission_groups(kind: ChannelKind) -> Groups {
    match kind {
        ChannelKind::Voice => VOICE,
        ChannelKind::Category => CATEGORY,
        _ => TEXT,
    }
}

/// What «Ver el canal» adds for `@everyone`: taking it away makes it private.
pub fn everyone_view_note(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Voice => t("Si se lo quitas a @everyone, el canal de voz pasa a ser privado."),
        ChannelKind::Category => {
            t("Si se lo quitas a @everyone, los canales de la categoría pasan a ser privados.")
        }
        _ => t("Si se lo quitas a @everyone, el canal pasa a ser privado."),
    }
}

/// The id of a permission's control: `cs:perm:{bit}`; its parts end in
/// `:deny`, `:inherit` and `:allow`.
pub fn permission_id(permission: Permissions) -> String {
    format!("cs:perm:{}", permission.0.trailing_zeros())
}

/// Deny ✕, leave as it is ⧸, or allow ✓: one is always chosen. `state` is
/// what [`rune_model::Overwrite::state`] says.
pub fn tri_state(
    id: &str,
    state: Option<bool>,
    enabled: bool,
    label: &str,
    look: &Look,
) -> Element {
    let white = Color::rgb(255, 255, 255);
    let segment = |which: &str, glyph: Icon, name: &str, fill: Color, ink: Color, chosen: bool| {
        let body = Element::panel(centered(
            Element::icon(glyph)
                .foreground(if chosen && fill != look.selected {
                    white
                } else {
                    ink
                })
                .size(Some(14.0), Some(14.0)),
        ))
        .size(
            Some((TRI_STATE_WIDTH - 2.0) / 3.0),
            Some(TRI_STATE_HEIGHT - 2.0),
        )
        .radius(5.0)
        .background(if chosen { fill } else { Color::TRANSPARENT });
        let access = Access::new(Role::RadioButton)
            .name(format!("{label}: {name}"))
            .selected(chosen)
            .disabled(!enabled);
        // The chosen one too, so all three are told with which is chosen.
        if enabled {
            Element::clickable(format!("{id}:{which}"), body)
                .radius(5.0)
                .hover_background(look.hover)
                .accessible(access)
        } else {
            body.accessible(access)
        }
    };
    // Disabled, the choice still reads, only quieter.
    let dim = |color: Color| {
        if enabled {
            color
        } else {
            Look::alpha(color, 110)
        }
    };
    Element::panel(
        Element::row(vec![
            segment(
                "deny",
                Icon::Close,
                t("denegar"),
                dim(look.danger),
                dim(look.danger),
                state == Some(false),
            ),
            segment(
                "inherit",
                Icon::Neutral,
                t("heredar"),
                look.selected,
                dim(look.text),
                state.is_none(),
            ),
            segment(
                "allow",
                Icon::Check,
                t("permitir"),
                dim(look.online),
                dim(look.online),
                state == Some(true),
            ),
        ])
        .padding(1.0),
    )
    .size(Some(TRI_STATE_WIDTH), Some(TRI_STATE_HEIGHT))
    .radius(6.0)
    .border(look.border, 1.0)
}

/// Whether a channel's overwrites are its category's, as a card on the
/// permissions page; out of sync, with «Sincronizar ahora» (`cs:sync`).
pub fn sync_card(category: &str, synced: bool, can_sync: bool, look: &Look) -> Element {
    let (glyph, lead) = if synced {
        (
            Icon::Refresh,
            t("Permisos sincronizados con la categoría: "),
        )
    } else {
        (
            Icon::WarningTriangle,
            t("Los permisos no están sincronizados con la categoría: "),
        )
    };
    let sentence = Element::formatted_text(vec![
        TextSpan::new(lead, look.text, 16.0, FontFamily::Sans),
        TextSpan::new(category, look.text, 16.0, FontFamily::SansBold),
    ]);
    let mut parts = vec![
        Element::icon(glyph)
            .foreground(look.idle)
            .size(Some(20.0), Some(20.0))
            .centered_y(),
        sentence.flex(1.0).centered_y(),
    ];
    if !synced {
        parts.push(
            Button::new("cs:sync", t("Sincronizar ahora"), look.menu_palette())
                .variant(ButtonVariant::Secondary)
                .button_size(ButtonSize::Small)
                .enabled(can_sync)
                .render()
                .centered_y(),
        );
    }
    Element::panel(Element::row(parts).gap(12.0).padding(16.0))
        .size(None, Some(if synced { 56.0 } else { 68.0 }))
        .radius(8.0)
        .background(look.raised)
        .border(look.border, 1.0)
        .accessible(Access::new(Role::Status).name(format!("{lead}{category}")))
}

/// A role or a person that may see a private channel.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessLine {
    pub name: String,
    /// «Rol», or the person's handle.
    pub detail: String,
    /// The role's color, or the person's accent.
    pub color: Color,
    pub person: bool,
    /// The person's picture, once loaded.
    pub image: Option<limn::RasterImage>,
    /// Said before the button, such as «Propietario del servidor».
    pub note: Option<String>,
    /// The action that takes their access away, when it can be taken.
    pub remove: Option<String>,
}

/// The card that makes a channel private, and who may see it while it is.
#[derive(Clone, Debug, PartialEq)]
pub struct PrivateCard {
    pub category: bool,
    pub on: bool,
    pub enabled: bool,
    pub roles: Vec<AccessLine>,
    pub members: Vec<AccessLine>,
}

/// «Canal privado» or «Categoría privada» with its switch (`cs:private`),
/// and while on, «Añadir miembros o roles» (`cs:access:add`) and the roles
/// and people who may see it.
pub fn private_card(card: &PrivateCard, look: &Look) -> Element {
    let (title, note) = if card.category {
        (
            t("Categoría privada"),
            t(
                "Si la haces privada, solo los roles y miembros que elijas van a poder verla. Los canales sincronizados con ella toman los mismos permisos.",
            ),
        )
    } else {
        (
            t("Canal privado"),
            t("Si lo haces privado, solo los roles y miembros que elijas van a poder verlo."),
        )
    };
    let head = Element::column(vec![
        Element::row(vec![
            Element::icon(Icon::Lock)
                .foreground(look.text)
                .size(Some(20.0), Some(20.0))
                .centered_y(),
            strong_line(title, 16.0, look.text).flex(1.0).centered_y(),
            Switch::new("cs:private", card.on, look.palette())
                .accessibility_label(title)
                .enabled(card.enabled)
                .render()
                .centered_y(),
        ])
        .gap(10.0),
        text(note, 12.0, look.muted),
    ])
    .gap(8.0)
    .padding(18.0);
    let mut parts = vec![head];
    if card.on {
        let rule = || {
            Element::panel(Element::spacer())
                .size(None, Some(1.0))
                .background(look.border)
        };
        let whom = if card.category {
            t("¿Quién puede acceder a esta categoría?")
        } else {
            t("¿Quién puede acceder a este canal?")
        };
        let mut body = vec![
            Element::row(vec![
                strong_line(whom, 14.0, look.text).flex(1.0).centered_y(),
                Button::new(
                    "cs:access:add",
                    t("Añadir miembros o roles"),
                    look.accent_palette(),
                )
                .button_size(ButtonSize::Small)
                .enabled(card.enabled)
                .render()
                .centered_y(),
            ])
            .gap(12.0),
            rule(),
            strong_line(t("Roles"), 14.0, look.text),
        ];
        if card.roles.is_empty() {
            body.push(
                Element::row(vec![
                    Element::icon(Icon::Users)
                        .foreground(look.faint)
                        .size(Some(16.0), Some(16.0))
                        .centered_y(),
                    line(t("No hay roles"), 14.0, look.faint).centered_y(),
                ])
                .gap(8.0),
            );
        }
        body.extend(card.roles.iter().map(|row| access_row(row, look)));
        body.push(rule());
        body.push(strong_line(t("Miembros"), 14.0, look.text));
        body.extend(card.members.iter().map(|row| access_row(row, look)));
        parts.push(
            Element::panel(Element::column(body).gap(10.0).padding(18.0)).background(look.canvas),
        );
    }
    Element::panel(Element::column(parts))
        .radius(8.0)
        .background(look.raised)
        .border(look.border, 1.0)
        .clip_children()
}

fn access_row(row: &AccessLine, look: &Look) -> Element {
    let mark = if row.person {
        crate::avatar_with_image(&row.name, row.color, 20.0, row.image.clone())
    } else {
        color_dot(row.color, 14.0)
    };
    let mut parts = vec![
        mark.centered_y(),
        line(&row.name, 15.0, look.text).centered_y(),
        line(&row.detail, 13.0, look.muted).flex(1.0).centered_y(),
    ];
    if let Some(note) = &row.note {
        parts.push(line(note, 13.0, look.muted).centered_y());
    }
    if let Some(remove) = &row.remove {
        parts.push(
            Button::new(remove.clone(), "", look.palette())
                .icon(Icon::CloseCircle)
                .variant(ButtonVariant::Ghost)
                .button_size(ButtonSize::Small)
                .accessibility_label(tr!("Quitarle el acceso a {name}", name = row.name))
                .tooltip(t("Quitar"))
                .render()
                .centered_y(),
        );
    }
    Element::row(parts).gap(8.0).size(None, Some(32.0))
}

/// «Permisos avanzados» with its chevron, which opens and closes the view
/// of every overwrite (`cs:advanced`).
pub fn advanced_heading(open: bool, look: &Look) -> Element {
    Element::clickable(
        "cs:advanced",
        Element::row(vec![
            line(t("Permisos avanzados"), 20.0, look.text).centered_y(),
            Element::icon(if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            })
            .foreground(look.text)
            .size(Some(16.0), Some(16.0))
            .centered_y(),
            Element::spacer().flex(1.0),
        ])
        .gap(12.0),
    )
    .size(None, Some(40.0))
    .accessible(
        Access::new(Role::Button)
            .name(t("Permisos avanzados"))
            .expanded(open),
    )
}

/// How a row of the list of overwrites is marked.
#[derive(Clone, Debug, PartialEq)]
pub enum TargetMark {
    /// A role, by its color.
    Role(Color),
    /// `@everyone`, unmarked.
    Everyone,
    /// A person, by their accent and their picture once loaded.
    Member(Color, Option<limn::RasterImage>),
}

/// A role or a person with an overwrite, as the advanced view lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetItem {
    /// What a click reports: it shows their permissions.
    pub id: String,
    pub name: String,
    pub mark: TargetMark,
    pub selected: bool,
    /// Its context menu; the first action, when there is one, is also a
    /// button on the selected row, for those without a secondary button.
    pub menu: Vec<MenuAction>,
}

/// The left column of the advanced view: «ROLES/MIEMBROS» with `add` (the
/// ⊕ and its popout), then the roles, `@everyone` and the people.
pub fn target_list(items: &[TargetItem], add: Element, look: &Look) -> Element {
    let mut rows = vec![Element::row(vec![
        gap(10.5, 0.0),
        line(t("ROLES/MIEMBROS"), 12.0, look.faint)
            .font_family(FontFamily::SansBold)
            .flex(1.0)
            .centered_y(),
        add.centered_y(),
    ])
    .size(None, Some(32.0))];
    for item in items {
        let mark = match &item.mark {
            TargetMark::Role(color) => Some(color_dot(*color, 13.0)),
            TargetMark::Member(accent, image) => Some(crate::avatar_with_image(
                &item.name,
                *accent,
                20.0,
                image.clone(),
            )),
            TargetMark::Everyone => None,
        };
        let mut parts = Vec::new();
        if let Some(mark) = mark {
            parts.push(mark.centered_y());
        }
        parts.push(
            line(
                &item.name,
                16.0,
                if item.selected { look.text } else { look.muted },
            )
            .flex(1.0)
            .centered_y(),
        );
        let body = Element::clickable(item.id.clone(), Element::row(parts).gap(8.0).padding(10.5))
            .radius(6.0)
            .hover_background(look.hover)
            .hover_foreground(look.text)
            .accessible(
                Access::new(Role::ListItem)
                    .name(item.name.clone())
                    .selected(item.selected),
            );
        let body = if item.selected {
            body.background(look.selected)
        } else {
            body
        };
        let mut row = vec![body.flex(1.0)];
        if let Some(first) = item.menu.first().filter(|_| item.selected) {
            row.push(
                Button::new(first.id.clone(), "", look.palette())
                    .icon(first.icon.unwrap_or(Icon::Trash))
                    .variant(ButtonVariant::Ghost)
                    .button_size(ButtonSize::Small)
                    .accessibility_label(first.label.clone())
                    .tooltip(first.denied.clone().unwrap_or_else(|| first.label.clone()))
                    .enabled(first.denied.is_none())
                    .render()
                    .centered_y(),
            );
        }
        let row = Element::row(row).gap(2.0).size(None, Some(34.0));
        rows.push(if item.menu.is_empty() {
            row
        } else {
            context_menu(&format!("{}:context", item.id), row, &item.menu, look)
        });
    }
    rows.push(gap(0.0, 8.0));
    rows.push(
        Element::row(vec![
            gap(10.5, 0.0),
            Element::panel(Element::spacer())
                .size(None, Some(1.0))
                .background(look.border)
                .flex(1.0),
            gap(10.5, 0.0),
        ])
        .size(None, Some(1.0)),
    );
    Element::column(rows)
        .gap(2.0)
        .size(Some(TARGET_LIST_WIDTH), None)
}

/// A role or a person the popout offers to add to the list.
#[derive(Clone, Debug, PartialEq)]
pub struct AddChoice {
    pub id: String,
    pub name: String,
    /// The role's color, or the person's accent.
    pub color: Color,
    pub person: bool,
    /// The person's picture, once loaded.
    pub image: Option<limn::RasterImage>,
}

/// The tallest the popout's list of roles and people gets: eight of them.
const ADD_LIST_HEIGHT: f32 = 8.0 * 44.0;

/// The ⊕ of the list (`cs:add`) and, while `open`, the popout «Añadir:» with
/// its field (`cs:add-query`) and what matches it; Esc or a click outside
/// reports `cs:add:dismiss`.
pub fn add_popout(
    enabled: bool,
    open: bool,
    query: &str,
    choices: &[AddChoice],
    look: &Look,
) -> Element {
    let button = Button::new("cs:add", "", look.palette())
        .icon(Icon::Add)
        .variant(ButtonVariant::Ghost)
        .button_size(ButtonSize::Small)
        .accessibility_label(t("Añadir un rol o un miembro"))
        .tooltip(t("Añadir un rol o un miembro"))
        .enabled(enabled)
        .render();
    let mut content = vec![
        strong_line(t("Añadir:"), 16.0, look.text),
        TextField::new("cs:add-query", query, t("Rol/Miembro"), look.menu_palette())
            .accessibility_label(t("Buscar un rol o un miembro"))
            .render(),
    ];
    if choices.is_empty() {
        content.push(text(t("Nada para añadir."), 13.0, look.muted));
    }
    let mut list = Vec::with_capacity(choices.len());
    for choice in choices {
        let (mark, name_ink, detail) = if choice.person {
            (
                crate::avatar_with_image(&choice.name, choice.color, 24.0, choice.image.clone()),
                look.text,
                String::new(),
            )
        } else {
            (gap(0.0, 0.0), choice.color, t("Rol").to_owned())
        };
        list.push(
            Element::clickable(
                choice.id.clone(),
                Element::row(vec![
                    mark.centered_y(),
                    line(&choice.name, 15.0, name_ink).flex(1.0).centered_y(),
                    line(detail, 12.0, look.muted).centered_y(),
                ])
                .gap(10.0)
                .padding(6.0),
            )
            .size(None, Some(36.0))
            .radius(4.0)
            .hover_background(look.hover)
            .accessible(Access::new(Role::Button).name(choice.name.clone())),
        );
    }
    // A server with many roles and people scrolls inside the popout instead
    // of running past the window's foot (2026-10-21).
    if !list.is_empty() {
        content.push(
            ScrollArea::new("cs:add-list", Element::column(list).gap(8.0))
                .appearance(ScrollbarAppearance::overlay().buttons(false))
                .render()
                .max_height(ADD_LIST_HEIGHT),
        );
    }
    Popover::new(
        button,
        Element::column(content).gap(8.0).padding(6.0),
        open,
        "cs:add:dismiss",
        look.menu_palette(),
    )
    .placement(PopoverPlacement::BottomEnd)
    .width(255.0)
    .render()
}

/// A choice among a list, shown as a text field shows its text.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldSelect {
    /// What the trigger reports; each option reports its own id.
    pub id: String,
    /// What the choice is called, for those who do not see it.
    pub name: String,
    pub value: String,
    /// Each option's id, its label, and whether it is the one chosen.
    pub options: Vec<(String, String, bool)>,
    pub open: bool,
    /// What Esc or a click outside the open list reports.
    pub dismiss: String,
    pub enabled: bool,
    pub width: f32,
}

/// A [`FieldSelect`]: as tall as a text field, the value at its start in 16
/// px and a chevron at its end, the list under it with a check on the
/// choice, five rows in view as the reference's; open, the chevron turns up
/// and the border takes the accent.
pub fn field_select(select: &FieldSelect, look: &Look) -> Element {
    let options = select
        .options
        .iter()
        .map(|(id, label, chosen)| DropdownOption::new(id.clone(), label.clone()).selected(*chosen))
        .collect();
    Select::new(
        select.id.clone(),
        select.value.clone(),
        options,
        select.open,
        select.dismiss.clone(),
        look.palette(),
    )
    .variant(SelectVariant::Field)
    .height(41.0)
    .font_size(16.0)
    .max_visible_items(5)
    .ink(look.accent)
    .width(select.width)
    .menu_palette(look.menu_palette())
    .placement(PopoverPlacement::BottomStart)
    .accessibility_label(select.name.clone())
    .enabled(select.enabled)
    .render()
}

/// A dialog's two buttons side by side, as wide as it: the way back and the
/// action.
pub fn dialog_buttons(back: (&str, &str), action: (&str, &str, bool), look: &Look) -> Element {
    let (id, label, enabled) = action;
    Element::row(vec![
        Button::new(back.0, back.1, look.menu_palette())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    const LOOK: Look = Look::DARK;

    #[test]
    fn each_kind_names_its_permissions_once_and_none_of_the_server() {
        let server_only = Permissions::ADMINISTRATOR
            | Permissions::MANAGE_SERVER
            | Permissions::KICK
            | Permissions::BAN
            | Permissions::TIMEOUT_MEMBERS
            | Permissions::MANAGE_NICKS
            | Permissions::CHANGE_NICK
            | Permissions::VIEW_AUDIT_LOG
            | Permissions::MANAGE_EMOJI;
        for kind in [ChannelKind::Text, ChannelKind::Voice, ChannelKind::Category] {
            let mut seen = Permissions::NONE;
            for (_, permissions) in permission_groups(kind) {
                for (permission, label, description) in *permissions {
                    assert!(!seen.intersects(*permission), "{label} twice in {kind:?}");
                    assert!(!label.is_empty() && !description.is_empty());
                    seen = seen | *permission;
                }
            }
            assert!(!seen.intersects(server_only), "{kind:?}");
            assert!(seen.contains(Permissions::VIEW | Permissions::MANAGE_ROLES));
        }
        let text = |kind| {
            permission_groups(kind)
                .iter()
                .flat_map(|(_, permissions)| permissions.iter())
                .fold(Permissions::NONE, |all, (permission, _, _)| {
                    all | *permission
                })
        };
        assert!(text(ChannelKind::Text).contains(Permissions::SEND));
        assert!(!text(ChannelKind::Text).intersects(Permissions::CONNECT));
        assert!(!text(ChannelKind::Voice).intersects(Permissions::SEND));
        assert!(text(ChannelKind::Category).contains(Permissions::SEND | Permissions::CONNECT));
        assert_eq!(permission_id(Permissions::SEND), "cs:perm:1");
    }

    #[test]
    fn slow_mode_names_its_waits() {
        assert_eq!(slowmode_label(0), "Apagado");
        assert_eq!(slowmode_label(21_600), "6 horas");
        assert_eq!(slowmode_label(45), "45 segundos");
        assert!(SLOWMODE.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn the_control_tells_its_three_states_and_which_is_chosen() {
        let control = tri_state("cs:perm:1", Some(false), true, "Enviar mensajes", &LOOK);
        let (_, laid) = frame(control, Size::new(200.0, 40.0));
        assert_eq!(
            controls(&laid),
            vec![
                "cs:perm:1:deny".to_owned(),
                "cs:perm:1:inherit".to_owned(),
                "cs:perm:1:allow".to_owned()
            ]
        );
        let chosen = |id: &str| {
            laid.access
                .iter()
                .find(|node| node.id == id)
                .and_then(|node| node.selected)
        };
        assert_eq!(chosen("cs:perm:1:deny"), Some(true));
        assert_eq!(chosen("cs:perm:1:allow"), Some(false));
        let locked = tri_state("cs:perm:1", None, false, "Enviar mensajes", &LOOK);
        let (_, laid) = frame(locked, Size::new(200.0, 40.0));
        assert!(controls(&laid).is_empty());
    }

    #[test]
    fn the_private_card_lists_who_may_see_it_only_while_on() {
        let card = PrivateCard {
            category: false,
            on: false,
            enabled: true,
            roles: Vec::new(),
            members: Vec::new(),
        };
        let (_, laid) = frame(private_card(&card, &LOOK), Size::new(705.0, 400.0));
        assert_eq!(controls(&laid), vec!["cs:private".to_owned()]);
        let on = PrivateCard {
            on: true,
            members: vec![AccessLine {
                name: "Ana".into(),
                detail: "ana".into(),
                color: LOOK.accent,
                person: true,
                image: None,
                note: Some("Propietario del servidor".into()),
                remove: None,
            }],
            roles: vec![AccessLine {
                name: "Moderación".into(),
                detail: "Rol".into(),
                color: LOOK.danger,
                person: false,
                image: None,
                note: None,
                remove: Some("cs:access:role:21:remove".into()),
            }],
            ..card
        };
        let (_, laid) = frame(private_card(&on, &LOOK), Size::new(705.0, 400.0));
        let ids = controls(&laid);
        assert!(ids.contains(&"cs:access:add".to_owned()), "{ids:?}");
        assert!(ids.contains(&"cs:access:role:21:remove".to_owned()));
        let painted = texts(&laid);
        assert!(painted.contains(&"Propietario del servidor".to_owned()));
    }

    #[test]
    fn a_field_select_opens_its_list_and_checks_the_choice() {
        let select = FieldSelect {
            id: "cs:slowmode".into(),
            name: "Modo lento".into(),
            value: "Apagado".into(),
            options: SLOWMODE
                .iter()
                .map(|(secs, label)| {
                    (
                        format!("cs:slowmode:{secs}"),
                        (*label).to_owned(),
                        *secs == 0,
                    )
                })
                .collect(),
            open: false,
            dismiss: "cs:slowmode:dismiss".into(),
            enabled: true,
            width: 705.0,
        };
        let alone = |select: &FieldSelect| {
            Element::column(vec![
                field_select(select, &LOOK),
                Element::spacer().flex(1.0),
            ])
        };
        let (_, laid) = frame(alone(&select), Size::new(705.0, 400.0));
        assert_eq!(controls(&laid), vec!["cs:slowmode".to_owned()]);
        let trigger = laid
            .regions
            .iter()
            .find(|region| region.id == "cs:slowmode")
            .unwrap();
        assert_eq!((trigger.rect.width, trigger.rect.height), (705.0, 41.0));
        let open = FieldSelect {
            open: true,
            ..select.clone()
        };
        let (_, laid) = frame(alone(&open), Size::new(705.0, 400.0));
        assert!(controls(&laid).contains(&"cs:slowmode:5".to_owned()));
        let disabled = FieldSelect {
            enabled: false,
            open: true,
            ..select
        };
        let (_, laid) = frame(alone(&disabled), Size::new(705.0, 400.0));
        assert!(
            controls(&laid).is_empty(),
            "disabled, it neither opens nor reports"
        );
    }

    #[test]
    fn the_sync_card_offers_to_sync_only_when_out_of_sync() {
        let (_, laid) = frame(
            sync_card("Desarrollo", true, true, &LOOK),
            Size::new(705.0, 80.0),
        );
        assert!(controls(&laid).is_empty());
        let (_, laid) = frame(
            sync_card("Desarrollo", false, true, &LOOK),
            Size::new(705.0, 80.0),
        );
        assert_eq!(controls(&laid), vec!["cs:sync".to_owned()]);
    }
}
