//! Profiles and the account's status, from plain props: the card of a person
//! (opened from the member list or from a message's author), the account's
//! own popout over its panel with the status menu and how long a status
//! lasts, the dialog that sets the custom status, the menus of the
//! microphone's and the headphones' arrows, and the full-window profile
//! editor with its live preview and a read-only account view.
//!
//! Measures follow the reference captures at 1361×797 logical pixels: a
//! popout (300 px, the official client's since 2026-10-18) with an 80 px
//! mark over its banner and action rows 52 px apart, a 310 px status menu
//! with 200 px menus of durations, 235 px device
//! menus and a 510.5 px status dialog. The editor adapts its form and preview
//! to one or two columns, with fixed close and save controls outside the scroll.

use limn::{
    Button, ButtonVariant, Color, Component, ComponentPalette, DropdownMenu, DropdownMenuEntry,
    DropdownMenuItem, Element, FontFamily, Icon, IconButton, Modal, Point, Popover,
    PopoverPlacement, ScrollArea, Size, Skeleton, Slider, Tab, Tabs, TabsVariant, TextArea,
    TextField, Tooltip, TooltipPlacement,
};
use rune_i18n::{es, t, tr};
use rune_model::{Rgb, Status, Timestamp};

use crate::server_settings::{self as parts, Swatches, UnsavedBar};
use crate::voice::{menu_entries, menu_width, MenuEntry};
use crate::{centered, emoji, gap, icon, line, strong_line, text, time, Look};

/// The actions these pieces report. Those that name a message, a status or
/// a device are built with the helpers or the prefixes here.
pub mod ids {
    /// The account's popout, opened from its panel.
    pub const DISMISS: &str = "pp:dismiss";
    /// Opens the profile editor, from here or from the settings.
    pub const EDIT: &str = "profile:edit";
    /// The bubble of the custom status: opens its dialog.
    pub const CUSTOM: &str = "pp:custom";
    pub const COPY_ID: &str = "pp:copy-id";
    pub const COPY_NAME: &str = "pp:copy-name";
    /// Followed by a status's key, and by `:` and a duration's key when it
    /// lasts a while: `pp:status=idle:1h`.
    pub const STATUS: &str = "pp:status=";
    /// Rows that only open a submenu start with this: choosing them does
    /// nothing.
    pub const SUBMENU: &str = "pp:sub:";
    /// The popout's status row, which opens the menu of statuses.
    pub const STATUS_MENU: &str = "pp:status-menu";
    pub const STATUS_MENU_DISMISS: &str = "pp:status-menu:dismiss";
    pub const SWITCH_ACCOUNT: &str = "pp:switch-account";

    /// The arrows of the microphone and the headphones: the quick voice
    /// panel of each, [`crate::quick_voice`].
    pub const INPUT: &str = "pp:input";
    pub const OUTPUT: &str = "pp:output";

    /// The dialog of the custom status.
    pub const CUSTOM_CLOSE: &str = "pp:custom:close";
    pub const CUSTOM_TEXT: &str = "pp:custom:text";
    pub const CUSTOM_EMOJI: &str = "pp:custom:emoji";
    /// Followed by the emoji chosen.
    pub const CUSTOM_EMOJI_PICK: &str = "pp:custom:emoji=";
    pub const CUSTOM_EMOJI_DISMISS: &str = "pp:custom:emoji:dismiss";
    pub const CUSTOM_EXPIRY: &str = "pp:custom:expiry";
    /// Followed by a [`super::ClearAfter`] key.
    pub const CUSTOM_EXPIRY_PICK: &str = "pp:custom:expiry=";
    pub const CUSTOM_EXPIRY_DISMISS: &str = "pp:custom:expiry:dismiss";
    pub const CUSTOM_SAVE: &str = "pp:custom:save";
    pub const CUSTOM_CANCEL: &str = "pp:custom:cancel";

    /// The card of a message's author.
    pub const AUTHOR_DISMISS: &str = "pp:author:dismiss";
    /// The card of someone of the member list.
    pub const MEMBER_DISMISS: &str = "profile:dismiss";
    /// The round buttons over a card's banner, each followed by the person's
    /// id: the friendship's (which opens its menu, or sends a request when
    /// there is none) and «…». Their menus close on `…:dismiss`.
    pub const CARD_RELATION: &str = "pp:relation:";
    pub const CARD_MORE: &str = "pp:more:";
    pub const CARD_RELATION_DISMISS: &str = "pp:relation:dismiss";
    pub const CARD_MORE_DISMISS: &str = "pp:more:dismiss";
    /// «Compartir perfil» and «Denunciar perfil de usuario» of a card's «…»,
    /// each followed by the person's id.
    pub const SHARE: &str = "pp:share:";
    pub const REPORT: &str = "pp:report:";
    /// A card's mutual friends or servers: followed by the person's id and
    /// `:friends` or `:servers`, they open the full profile on that tab.
    pub const MUTUAL: &str = "pp:mutual:";
    /// The «+» beside a card's roles opens the checklist of roles; its
    /// `…:dismiss` closes it. Followed by the person's id.
    pub const CARD_ROLES: &str = "pp:roles:";
    pub const CARD_ROLES_DISMISS: &str = "pp:roles:dismiss";
    /// The field under a card that writes to its person: followed by their id.
    pub const MESSAGE: &str = "pp:message:";
    /// Opens someone's full profile: the prefix and their id.
    pub const FULL: &str = "pp:full:";
    pub const FULL_CLOSE: &str = "pp:full:close";
    /// Followed by a [`super::FullTab`] key: `pp:full:tab=servers`.
    pub const FULL_TAB: &str = "pp:full:tab=";
    /// «Ver biografía completa» of a card: the full profile, from the bio.
    pub const FULL_BIO: &str = "pp:full-bio:";
    /// The note's icon of a card: the full profile, where the note is.
    pub const FULL_NOTE: &str = "pp:full-note:";
    /// The body of a card, scrolling between its head and its buttons;
    /// see [`card_body`].
    pub const CARD_BODY: &str = "pp:card:body:";
    /// The effect over a card, which gives way under the pointer.
    pub const CARD_EFFECT: &str = "pp:card:effect";

    /// The profile editor.
    pub const EDITOR_CLOSE: &str = "profile:close";
    pub const NAME: &str = "profile:name";
    pub const PRONOUNS: &str = "profile:pronouns";
    pub const ABOUT: &str = "profile:about";
    /// Prefixes the color picker's ids: see [`crate::server_settings::Swatches`].
    pub const COLOR: &str = "profile:color";
    pub const COLOR_HEX: &str = "profile:color:hex";
    /// The selector of profiles at the top of the editor's column.
    pub const SERVERS: &str = "profile:servers";
    pub const SERVERS_DISMISS: &str = "profile:servers:dismiss";
    pub const MAIN_PROFILE: &str = "profile:main";
    /// Prefixes the bar's ids: `profile:reset` and `profile:save`.
    pub const BAR: &str = "profile";
    pub const RESET: &str = "profile:reset";
    pub const SAVE: &str = "profile:save";
    /// The pictures: choose one through the system's selector, or take it away.
    pub const AVATAR_PICK: &str = "profile:avatar:pick";
    pub const AVATAR_REMOVE: &str = "profile:avatar:remove";
    pub const BANNER_PICK: &str = "profile:banner:pick";
    pub const BANNER_REMOVE: &str = "profile:banner:remove";
    /// The name the account goes by in the server whose profile is edited.
    pub const NICK: &str = "profile:nick";
    /// Edits the profile of a server: the prefix and the server's id.
    pub const SCOPE: &str = "profile:scope=";
    /// Where the pictures start, to scroll them into view.
    pub const PICTURES: &str = "profile:pictures";
    // Profile editing (edicion-de-perfil).
    /// The picture being framed, dragged over to move the frame, and the
    /// slider that brings it closer.
    pub const FRAME: &str = "profile:frame";
    pub const FRAME_ZOOM: &str = "profile:frame:zoom";

    /// The name of the author of `message`, which opens their card.
    pub fn author_name(message: u64) -> String {
        format!("pp:name:{message}")
    }

    /// The mark of the author of `message`, which opens their card.
    pub fn author_mark(message: u64) -> String {
        format!("pp:avatar:{message}")
    }

    /// The body of the card of `handle` (unique per account): one for each
    /// person, since the window keeps a scroll per ID (2026-10-18: one
    /// person's card scrolled opened the next one scrolled too).
    pub fn card_body(handle: &str) -> String {
        format!("{CARD_BODY}{handle}")
    }
}

/// A popout's width: the official client's 300 px (owner's decision of
/// 2026-10-18; it was 325 px, from the reference captures).
pub const WIDTH: f32 = 300.0;
/// The room between a member list's row and its card; 8 px from an
/// author's name, as the official popout stands (REA, 2026-10-17).
pub const LIST_GAP: f32 = 16.0;
/// The bio a card shows: three lines, about 60 px, as the official
/// client's popout; longer ones lead to the full profile (2026-10-17).
const BIO_LINES: usize = 3;
const BIO_CHARS: usize = 120;
const BIO_HEIGHT: f32 = 60.0;
const BANNER: f32 = 105.0;
const AVATAR: f32 = 80.0;
/// The ring of the card's color around the mark.
const RING: f32 = 6.0;
/// Inside a card, left and right.
const INSET: f32 = 16.0;
/// A person's card: its corners, and its head, 140 px as the official
/// popout's. The mark (80 px at (16, 61)) overlaps the banner by 44 px and
/// hangs 36 px below it; its 6 px ring is cut into the banner, so the ring's
/// box starts at (10, 55).
const CARD_RADIUS: f32 = 8.0;
const HEAD: f32 = 140.0;
const MARK_TOP: f32 = 61.0;
/// The status dot of a card's mark: 16 px, in a 28 px disc of the ground.
const DOT: f32 = 16.0;
/// The decoration is 1.2 times the mark, centred on it.
const DECORATION: f32 = AVATAR * 1.2;
/// The skeleton of a card being read: its height, and its four text lines
/// (REA §5b, from the card's 268 px body).
const SKELETON_HEIGHT: f32 = 350.0;
const SKELETON_LINES: [f32; 4] = [241.0, 201.0, 228.0, 161.0];
/// The status menu beside the popout, and the menus of durations beside it.
pub const STATUS_MENU_WIDTH: f32 = 310.0;
pub const DURATION_MENU_WIDTH: f32 = 200.0;
/// The dialog of the custom status, and its menu of when it goes.
pub const DIALOG_WIDTH: f32 = 510.5;
/// The full profile, as wide as the reference's; a fixed height keeps it
/// from resizing as its tabs change.
const FULL_WIDTH: f32 = 600.0;
const FULL_HEIGHT: f32 = 640.0;
const EXPIRY_MENU_WIDTH: f32 = 274.0;
/// The editor's maximum form width; smaller windows stack the preview below it.
pub const EDITOR_FORM: f32 = 427.0;
const PREVIEW_GAP: f32 = 28.0;
/// The narrowest editor whose save bar fits its message and buttons on one row.
const WIDE_BAR: f32 = 560.0;
const CLOSE: f32 = 34.0;

/// The longest display name, pronouns, «Sobre mí» and custom status.
pub const NAME_MAX: usize = 32;
pub const PRONOUNS_MAX: usize = 40;
pub const ABOUT_MAX: usize = 190;
pub const CUSTOM_MAX: usize = 128;

/// The statuses the account can choose: the status, its key in the ids, and
/// what it means when that is not plain.
pub const STATUSES: [(Status, &str, Option<&str>); 4] = [
    (Status::Online, "online", None),
    (Status::Idle, "idle", None),
    (
        Status::DoNotDisturb,
        "dnd",
        Some(es("Los demás ven que no quieres interrupciones.")),
    ),
    (
        Status::Invisible,
        "invisible",
        Some(es("Apareces sin conexión.")),
    ),
];

/// How long a chosen status lasts: its key in the ids, what the menu says,
/// and its minutes. Choosing the status itself keeps it until changed.
pub const DURATIONS: [(&str, &str, i64); 5] = [
    ("15m", es("Durante 15 minutos"), 15),
    ("1h", es("Durante 1 hora"), 60),
    ("8h", es("Durante 8 horas"), 8 * 60),
    ("24h", es("Durante 24 horas"), 24 * 60),
    ("3d", es("Durante 3 días"), 3 * 24 * 60),
];

/// The key of a status in the ids.
pub fn status_key(status: Status) -> &'static str {
    STATUSES
        .iter()
        .find(|(candidate, _, _)| *candidate == status)
        .map_or("online", |(_, key, _)| key)
}

/// The status a key names.
pub fn status_of(key: &str) -> Option<Status> {
    STATUSES
        .iter()
        .find(|(_, candidate, _)| *candidate == key)
        .map(|(status, _, _)| *status)
}

/// When a custom status goes by itself.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClearAfter {
    #[default]
    Day,
    FourHours,
    Hour,
    HalfHour,
    Never,
}

impl ClearAfter {
    pub const ALL: [Self; 5] = [
        Self::Day,
        Self::FourHours,
        Self::Hour,
        Self::HalfHour,
        Self::Never,
    ];

    pub fn minutes(self) -> Option<i64> {
        match self {
            Self::Day => Some(24 * 60),
            Self::FourHours => Some(4 * 60),
            Self::Hour => Some(60),
            Self::HalfHour => Some(30),
            Self::Never => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Day => "24h",
            Self::FourHours => "4h",
            Self::Hour => "1h",
            Self::HalfHour => "30m",
            Self::Never => "never",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.key() == key)
    }

    /// What its menu says, set at `now`: "24 horas (mañana a las 07:22)".
    pub fn label(self, now: Timestamp) -> String {
        let length = match self {
            Self::Day => t("24 horas"),
            Self::FourHours => t("4 horas"),
            Self::Hour => t("1 hora"),
            Self::HalfHour => t("30 minutos"),
            Self::Never => return t("No eliminar").into(),
        };
        let until = now.plus_minutes(self.minutes().unwrap_or_default());
        let when = if time::day(until) == time::day(now) {
            time::clock(until)
        } else {
            until_label(until, now)
        };
        format!("{length} ({when})")
    }
}

/// When something that ends at `until` ends, seen at `now`: "hoy a las
/// 07:52", "mañana a las 07:22" or "el 17/10 a las 07:22".
pub fn until_label(until: Timestamp, now: Timestamp) -> String {
    let (day, today) = (time::day(until), time::day(now));
    let clock = time::clock(until);
    if day == today {
        tr!("hoy a las {clock}", clock)
    } else if Some(day) == today.succ_opt() {
        tr!("mañana a las {clock}", clock)
    } else {
        tr!(
            "el {date} a las {clock}",
            date = day.format(t("%d/%m")),
            clock
        )
    }
}

/// The mark of a status, `size` across, on `ground`: a dot while online, a
/// crescent while idle, a dot with a bar while not to be disturbed, a ring
/// while invisible or offline.
pub fn status_mark(status: Status, look: &Look, size: f32, ground: Color) -> Element {
    let color = look.status(status);
    let disc = |diameter: f32, fill: Color| {
        Element::panel(Element::spacer())
            .size(Some(diameter), Some(diameter))
            .radius(diameter / 2.0)
            .background(fill)
    };
    match status {
        Status::Online => disc(size, color),
        Status::Idle => Element::stack(vec![disc(size, color), disc(size * 0.62, ground)])
            .size(Some(size), Some(size)),
        Status::DoNotDisturb => Element::stack(vec![
            disc(size, color),
            crate::centered(
                Element::panel(Element::spacer())
                    .size(Some(size * 0.6), Some((size * 0.22).max(2.0)))
                    .radius(size * 0.11)
                    .background(ground),
            ),
        ])
        .size(Some(size), Some(size)),
        Status::Invisible | Status::Offline => Element::panel(Element::spacer())
            .size(Some(size), Some(size))
            .radius(size / 2.0)
            .border(color, (size * 0.28).max(2.0)),
    }
}

/// `child`, `size` across, with the mark of `status` over its bottom right
/// corner, ringed in `ground` so it reads on any color.
fn marked(child: Element, size: f32, status: Status, look: &Look, ground: Color) -> Element {
    let dot = (size * 0.25).max(10.0);
    let ring = (dot * 0.25).max(2.0);
    let mark = Element::panel(status_mark(status, look, dot, ground))
        .padding(ring)
        .radius(dot / 2.0 + ring)
        .background(ground);
    Element::stack(vec![
        child,
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![Element::spacer().flex(1.0), mark]),
        ]),
    ])
    .size(Some(size), Some(size))
}

/// What a card shows of a person.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub image: Option<limn::RasterImage>,
    /// A decoded wide image; a missing or loading banner retains the accent.
    pub banner: Option<limn::RasterImage>,
    /// Without a picture, the banner's color: the profile's accent when it
    /// has one, else the mark's dominant color. `None` paints `raised`.
    pub banner_tint: Option<Color>,
    /// The unique handle, without a leading @.
    pub handle: String,
    /// Their color: the banner, and behind their initial.
    pub accent: Color,
    pub status: Status,
    /// Whether the provider publishes presence; otherwise no status is displayed.
    pub presence: bool,
    /// The line they set beside their status.
    pub custom: Option<String>,
    /// A server's emoji before that line, as a picture (2026-10-17).
    pub custom_emoji: Option<crate::identity::TextPicture>,
    pub pronouns: Option<String>,
    pub about: Option<String>,
    /// What they are doing: "Jugando a Factorio".
    pub activity: Option<String>,
    /// Their roles where they are seen, highest first, in their colors.
    pub roles: Vec<(String, Color)>,
    pub bot: bool,
    /// What keeps them from taking part where they are seen: "Aislado
    /// hasta hoy a las 18:30".
    pub restriction: Option<String>,
    /// Since when, the account first and then the server it is seen in.
    /// Only what is known.
    pub member_since: Vec<Since>,
    /// What they share with the account, once known: "2 servidores en común".
    pub mutual: Option<String>,
    /// The action that opens a direct conversation with them, if any: the
    /// full profile's «Enviar mensaje».
    pub message_action: Option<String>,
    /// The action that opens their full profile, if any.
    pub full_action: Option<String>,
    /// The account's private note on them, where it keeps notes.
    pub note: Option<NoteField>,
    /// What they do, where from, their badges and marks (P5).
    pub identity: crate::identity::CardIdentity,
    /// Their profile is being read and none of it is kept yet: the card is
    /// its skeleton, [`card_skeleton`].
    pub loading: bool,
    /// The friendship's round button on the banner; `None` hides it: the
    /// account's own card, bots, blocked people, a provider without requests.
    pub relation: Option<Relation>,
    /// The «…» menu's entries; `None` hides its button, on the account's own card.
    pub more: Option<Vec<MenuEntry>>,
    /// The card's menu that is open, if any: its entries are the ones above.
    pub open_menu: Option<CardMenu>,
    /// The account blocks or ignores them: a notice under the name.
    pub notice: Option<Notice>,
    /// Who they share with the account, as the card's row shows it:
    /// nothing while unknown or loading, or when there is none.
    pub mutual_friends: Mutual,
    pub mutual_servers: Mutual,
    /// Per role of `roles`: the action that takes it away, when the account
    /// may. Only what the checklist lists can be taken away.
    pub role_removals: Vec<Option<String>>,
    /// The roles' checklist, when the account may change them.
    pub role_menu: Option<Vec<MenuEntry>>,
    /// The field under the card that writes to them, when the account may.
    pub message: Option<MessageField>,
    /// The account's own card offers «Editar perfil».
    pub edit_profile: bool,
}

/// The friendship with someone, as the round button on a card's banner says
/// it (REA §3): none (the button sends a request), a friend, a request sent,
/// or one received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Friendship {
    None,
    Friend,
    Outgoing,
    Incoming,
}

/// The friendship button of a card: its state, the action it sends (the
/// request when there is none, else the toggle of its menu), and the entries
/// of that menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Relation {
    pub state: Friendship,
    pub action: String,
    pub menu: Vec<MenuEntry>,
}

/// The menus a card opens from its buttons: the friendship's, «…», and the
/// roles checklist beside the roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardMenu {
    Relation,
    More,
    Roles,
}

/// The field under a card that writes to its person (REA §4.5): what is
/// written so far; Enter sends it.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageField {
    pub id: String,
    pub text: String,
}

/// Why a card's person is set apart: the account blocks them, or ignores them.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    Blocked,
    Ignored {
        /// The action of «Dejar de ignorar».
        unignore: String,
    },
}

/// The account's private note on someone, as its field shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteField {
    /// The field's own: Enter keeps what is written.
    pub id: String,
    pub text: String,
    /// Still being read: the field waits.
    pub loading: bool,
    /// Why it cannot be written now, when it cannot.
    pub denied: Option<String>,
}

/// «Nota»: a field only the account sees, saved with Enter.
fn note_section(note: &NoteField, look: &Look) -> Element {
    let placeholder = if note.loading {
        t("Cargando la nota…")
    } else {
        t("Haz clic para agregar una nota")
    };
    let field = TextField::new(
        note.id.clone(),
        note.text.clone(),
        placeholder,
        look.menu_palette(),
    )
    .accessibility_label(t("Nota"));
    let field = if note.loading || note.denied.is_some() {
        field.disabled()
    } else {
        field
    };
    let hint = note
        .denied
        .clone()
        .unwrap_or_else(|| t("Solo la ves tú. Enter la guarda.").into());
    section(
        t("Nota"),
        Element::column(vec![field.render(), text(hint, 12.0, look.faint)]).gap(4.0),
        look,
    )
}

/// A small picture of a server or a person: their image, else their initial
/// on their color.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub name: String,
    pub accent: Color,
    pub image: Option<limn::RasterImage>,
}

/// One date of «Miembro desde»: since when, and where. `face` is the
/// server's; the account has none.
#[derive(Clone, Debug, PartialEq)]
pub struct Since {
    /// "Cuenta", or the server's name.
    pub scope: String,
    pub face: Option<Face>,
    /// "3 de mayo de 2019".
    pub date: String,
}

/// A loaded list of a full profile: not reported, on its way, or known.
#[derive(Clone, Debug, PartialEq)]
pub enum Mutual {
    Unknown,
    Loading,
    Known(Vec<Face>),
}

/// The tabs of the full profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FullTab {
    #[default]
    About,
    Servers,
    Friends,
}

impl FullTab {
    pub const ALL: [Self; 3] = [Self::About, Self::Servers, Self::Friends];

    pub fn key(self) -> &'static str {
        match self {
            Self::About => "about",
            Self::Servers => "servers",
            Self::Friends => "friends",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.key() == key)
    }

    fn label(self) -> &'static str {
        match self {
            Self::About => t("Sobre mí"),
            Self::Servers => t("Servidores en común"),
            Self::Friends => t("Amigos en común"),
        }
    }
}

/// What the full profile shows beyond the card.
#[derive(Clone, Debug, PartialEq)]
pub struct FullProfile {
    pub profile: Profile,
    /// The server the card is seen in: its roles, nickname and date are of it.
    pub server: Option<String>,
    /// Their nickname in that server, when they set one.
    pub nick: Option<String>,
    pub mutual_servers: Mutual,
    pub mutual_friends: Mutual,
    pub tab: FullTab,
    /// The accounts elsewhere they show (P5).
    pub connections: Vec<crate::identity::ConnectionRow>,
    /// Their profile is being read and none of it is kept yet: the About
    /// tab shows its lines instead of claiming there is no more data.
    pub loading: bool,
}

/// `content` floating against `anchor` on a card's surface, `WIDTH` wide,
/// while there is content. Escape and a click outside report `dismiss`.
pub fn popout(
    anchor: Element,
    content: Option<Element>,
    dismiss: &str,
    placement: PopoverPlacement,
    look: &Look,
) -> Element {
    popout_radius(anchor, content, dismiss, placement, 12.0, look)
}

/// [`popout`] for a person's card, with its 8 px corners (2026-10-31).
pub fn card_popout(
    anchor: Element,
    content: Option<Element>,
    dismiss: &str,
    placement: PopoverPlacement,
    look: &Look,
) -> Element {
    popout_radius(anchor, content, dismiss, placement, CARD_RADIUS, look)
}

fn popout_radius(
    anchor: Element,
    content: Option<Element>,
    dismiss: &str,
    placement: PopoverPlacement,
    radius: f32,
    look: &Look,
) -> Element {
    let Some(content) = content else {
        return anchor;
    };
    let surface = Element::panel(content)
        .size(Some(WIDTH), None)
        .background(look.menu)
        .border(look.border, 1.0)
        .radius(radius)
        .shadow(look.shadow, Point::new(0.0, 9.0), 22.0, 0.0)
        .clip_children();
    Element::popover(anchor, surface, dismiss, placement).popover_gap(8.0)
}

/// A thought bubble: the custom status, or an invitation to set one. It
/// opens the dialog when given an action.
fn bubble(
    content: &str,
    emoji: Option<&crate::identity::TextPicture>,
    invitation: bool,
    action: Option<&str>,
    look: &Look,
) -> Element {
    let mut parts = Vec::new();
    if invitation {
        parts.push(icon(Icon::Add, look.muted).centered_y());
    }
    // A server's emoji before the line: 16 px, 32 alone, as the official
    // client's popout draws it.
    if let Some(emoji) = emoji {
        let side = if content.is_empty() { 32.0 } else { 16.0 };
        parts.push(crate::identity::picture(emoji, side, look).centered_y());
    }
    if !content.is_empty() {
        parts.push(
            text(
                content,
                13.0,
                if invitation { look.muted } else { look.text },
            )
            .font_family(FontFamily::SansItalic)
            .flex(1.0),
        );
    }
    let body = Element::row(parts).gap(6.0).padding(10.0);
    let body = match action {
        Some(id) => Element::clickable(id, body)
            .hover_background(look.selected)
            .accessible(limn::Access::new(limn::Role::Button).name(if invitation {
                t("Establecer estado personalizado").to_owned()
            } else {
                tr!("Estado personalizado: {content}", content)
            })),
        None => Element::panel(body),
    };
    body.radius(14.0)
        .background(look.raised)
        .border(look.border, 1.0)
}

/// What someone set beside their status: its line and a server's emoji
/// before it, when they set either.
fn custom_status(profile: &Profile) -> Option<(&str, Option<&crate::identity::TextPicture>)> {
    let emoji = profile.custom_emoji.as_ref();
    match profile.custom.as_deref() {
        Some(custom) => Some((custom, emoji)),
        None => emoji.map(|emoji| ("", Some(emoji))),
    }
}

/// How large a head is: the popout's, or the full profile's.
#[derive(Clone, Copy)]
struct HeadSize {
    banner: f32,
    avatar: f32,
    /// The banner's corners: the full profile's sits inside the dialog's padding.
    radius: f32,
}

const CARD_HEAD: HeadSize = HeadSize {
    banner: BANNER,
    avatar: AVATAR,
    radius: 0.0,
};
const FULL_HEAD: HeadSize = HeadSize {
    banner: 150.0,
    avatar: 104.0,
    radius: 8.0,
};

/// The top of the account's own popout and of its custom status dialog: the
/// popout's head as it was (the card of someone else has [`card_head`]).
fn head(profile: &Profile, bubble: Option<Element>, look: &Look) -> Element {
    head_sized(profile, bubble, CARD_HEAD, look)
}

/// The top of a card (REA §2): the banner, its color when it has no picture,
/// and the mark over its bottom edge with the ring cut into the banner, the
/// decoration 1.2 times the mark, the status dot, and `bubble` beside it.
fn card_head(profile: &Profile, bubble: Option<Element>, look: &Look) -> Element {
    let banner = Element::panel(match &profile.banner {
        Some(image) => Element::raster_image(image.clone().cover()).size(None, Some(BANNER)),
        None => Element::spacer(),
    })
    .size(None, Some(BANNER))
    .background(profile.banner_tint.unwrap_or(look.raised))
    .clip_children();
    let face =
        crate::avatar_with_image(&profile.name, profile.accent, AVATAR, profile.image.clone());
    let face = if profile.presence {
        card_mark(face, profile.status, look)
    } else {
        face
    };
    let ring = Element::panel(face)
        .padding(RING)
        .radius((AVATAR + 2.0 * RING) / 2.0)
        .background(look.menu);
    let (mark_x, mark_y) = (INSET - RING, MARK_TOP - RING);
    // A column keeps the banner its height: a stack child would take the head's.
    let mut layers = vec![Element::column(vec![banner]), at(mark_x, mark_y, ring)];
    if let Some(decoration) = &profile.identity.marks.decoration {
        let centre = INSET + AVATAR / 2.0;
        let middle = MARK_TOP + AVATAR / 2.0;
        layers.push(at(
            centre - DECORATION / 2.0,
            middle - DECORATION / 2.0,
            Element::raster_image(decoration.clone()).size(Some(DECORATION), Some(DECORATION)),
        ));
    }
    if let Some(bubble) = bubble {
        // Beside the mark, as high as the banner's edge rises above its ring.
        layers.push(Element::column(vec![
            gap(0.0, mark_y + RING / 2.0 + 6.0),
            Element::row(vec![
                gap(INSET + AVATAR + 8.0, 0.0),
                bubble.flex(1.0),
                gap(INSET, 0.0),
            ]),
        ]));
    }
    // Over everything, so the buttons take their clicks.
    layers.extend(banner_buttons(profile, look));
    Element::stack(layers).size(Some(WIDTH), Some(HEAD))
}

/// The status of a card's mark: a 16 px dot in the bottom right corner,
/// ringed in the card's ground so it reads over the picture.
fn card_mark(face: Element, status: Status, look: &Look) -> Element {
    let mark = Element::panel(status_mark(status, look, DOT, look.menu))
        .padding(6.0)
        .radius(14.0)
        .background(look.menu);
    Element::stack(vec![
        face,
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![Element::spacer().flex(1.0), mark]),
        ]),
    ])
    .size(Some(AVATAR), Some(AVATAR))
}

fn head_sized(profile: &Profile, bubble: Option<Element>, size: HeadSize, look: &Look) -> Element {
    let HeadSize {
        banner: tall,
        avatar: across,
        radius,
    } = size;
    let ring = across + 2.0 * RING;
    let banner = Element::panel(match &profile.banner {
        Some(image) => Element::raster_image(image.clone().cover()).size(None, Some(tall)),
        None => Element::spacer(),
    })
    .size(None, Some(tall))
    .radius(radius)
    .background(profile.accent)
    .clip_children();
    let face = crate::identity::decorated(
        |side| crate::avatar_with_image(&profile.name, profile.accent, side, profile.image.clone()),
        across,
        profile.identity.marks.decoration.clone(),
    );
    let face = if profile.presence {
        marked(face, across, profile.status, look, look.menu)
    } else {
        face
    };
    let face = Element::panel(face)
        .padding(RING)
        .radius(ring / 2.0)
        .background(look.menu);
    let beside = match bubble {
        Some(bubble) => Element::column(vec![gap(0.0, ring / 2.0 + 6.0), bubble]),
        None => Element::spacer(),
    };
    Element::stack(vec![
        Element::column(vec![banner, gap(0.0, ring / 2.0 + 6.0)]),
        Element::column(vec![
            gap(0.0, tall - ring / 2.0),
            Element::row(vec![
                gap(INSET, 0.0),
                face,
                beside.flex(1.0),
                gap(INSET, 0.0),
            ])
            .gap(8.0),
        ]),
    ])
}

/// A small mark beside a name, such as BOT.
fn tag(label: &str, look: &Look) -> Element {
    Element::panel(
        Element::text(label)
            .font_family(FontFamily::SansBold)
            .font_size(9.0)
            .foreground(look.on_accent),
    )
    .padding(3.0)
    .radius(3.0)
    .background(look.accent)
}

/// The name, 20 px (24 in the full profile), and the handle with the
/// pronouns under it, then what they share with the account. The
/// account's own card offers to copy the handle; someone else's card, with
/// `note`, the icon of the account's note on them, which opens their full
/// profile where it is written, as the official popout does (2026-10-18:
/// the card held the note's field).
fn names(profile: &Profile, copy: bool, note: Option<&str>, size: f32, look: &Look) -> Element {
    let mut top = vec![Element::truncated_text(profile.name.clone())
        .font_family(FontFamily::SansBold)
        .font_size(size)
        .foreground(look.text)
        .shrink_to_fit()
        .centered_y()];
    if let (Some(action), Some(field)) = (note, &profile.note) {
        let hint = if field.text.is_empty() {
            t("Agregar una nota").to_owned()
        } else {
            tr!("Nota: {note}", note = field.text.as_str())
        };
        top.push(
            IconButton::new(action, Icon::Edit, look.menu_palette())
                .variant(ButtonVariant::Ghost)
                .tooltip(hint)
                .ink(look.muted)
                .render()
                .centered_y(),
        );
    }
    if let Some(chip) = &profile.identity.marks.tag {
        top.push(crate::identity::tag_chip(chip, 14.0, look).centered_y());
    }
    if profile.bot {
        top.push(tag("BOT", look).centered_y());
    }
    if copy {
        top.push(
            IconButton::new(ids::COPY_NAME, Icon::Copy, look.menu_palette())
                .variant(ButtonVariant::Ghost)
                .tooltip(t("Copiar nombre de usuario"))
                .ink(look.muted)
                .render()
                .centered_y(),
        );
    }
    top.push(Element::spacer().flex(1.0));
    let mut second = profile.handle.clone();
    if let Some(pronouns) = &profile.pronouns {
        second = format!("{second} • {pronouns}");
    }
    let mut column = vec![Element::row(top).gap(6.0), text(second, 14.0, look.muted)];
    if let Some(mutual) = &profile.mutual {
        column.push(
            Element::row(vec![
                icon(Icon::Users, look.muted)
                    .size(Some(14.0), Some(14.0))
                    .centered_y(),
                line(mutual.clone(), 13.0, look.muted)
                    .shrink_to_fit()
                    .centered_y(),
            ])
            .gap(6.0),
        );
    }
    Element::column(column).gap(2.0)
}

/// A titled part of a card: «Sobre mí», «Roles», under a small muted heading.
fn section(title: &str, body: Element, look: &Look) -> Element {
    Element::column(vec![
        line(title, 12.0, look.muted).font_family(FontFamily::SansBold),
        body,
    ])
    .gap(6.0)
}

/// A hairline between the parts of a card.
fn divider(look: &Look) -> Element {
    Element::panel(Element::spacer())
        .size(None, Some(1.0))
        .background(look.border)
}

/// A role as a chip on a neutral ground: only its dot carries its color,
/// then its picture or emoji when it has one.
fn role_chip(
    name: &str,
    color: Color,
    mark: Option<&crate::identity::RoleMark>,
    look: &Look,
) -> Element {
    let mut inside = vec![parts::color_dot(color, 10.0).centered_y()];
    if let Some(mark) = mark {
        inside.push(crate::identity::role_mark(mark, 14.0, look).centered_y());
    }
    inside.push(line(name, 12.0, look.text).centered_y());
    Element::panel(Element::row(inside).gap(5.0).padding(6.0))
        .size(None, Some(24.0))
        .radius(6.0)
        .background(look.raised)
        .border(look.border, 1.0)
}

/// Roles as chips, in as many lines as they need, with their marks.
fn role_chips(
    roles: &[(String, Color)],
    marks: &[Option<crate::identity::RoleMark>],
    look: &Look,
) -> Element {
    Element::row(
        roles
            .iter()
            .enumerate()
            .map(|(index, (name, color))| {
                role_chip(
                    name,
                    *color,
                    marks.get(index).and_then(Option::as_ref),
                    look,
                )
            })
            .collect(),
    )
    .gap(4.0)
    .wrap()
}

/// A face `size` across: their picture, or their initial on their color.
fn face(face: &Face, size: f32) -> Element {
    crate::avatar_with_image(&face.name, face.accent, size, face.image.clone())
}

/// «Miembro desde»: one line per scope, its mark, the date and where.
fn member_since(entries: &[Since], look: &Look) -> Element {
    Element::column(
        entries
            .iter()
            .map(|since| {
                let mark = match &since.face {
                    Some(server) => face(server, 18.0),
                    None => icon(Icon::At, look.muted).size(Some(18.0), Some(18.0)),
                };
                Element::row(vec![
                    mark.centered_y(),
                    line(since.date.clone(), 14.0, look.text).centered_y(),
                    line(since.scope.clone(), 13.0, look.muted)
                        .shrink_to_fit()
                        .centered_y(),
                ])
                .gap(8.0)
            })
            .collect(),
    )
    .gap(6.0)
}

/// What the person does: their activity cards, or else the line their
/// presence gives.
fn activity_parts(profile: &Profile, width: f32, look: &Look) -> Vec<Element> {
    let identity = &profile.identity;
    if identity.activities.is_empty() {
        return profile
            .activity
            .iter()
            .map(|activity| text(activity.clone(), 13.0, look.text))
            .collect();
    }
    vec![Element::column(
        identity
            .activities
            .iter()
            .map(|card| crate::identity::activity_card(card, width, look))
            .collect(),
    )
    .gap(8.0)]
}

/// Someone's bio as Markdown, `width` across. A card (`clamp`) shows three
/// lines, as the official client's popout does, and leads to the full
/// profile for the rest (2026-10-17); the full profile shows all of it.
fn bio(profile: &Profile, clamp: bool, width: f32, look: &Look) -> Option<Element> {
    let about = profile.about.as_ref()?;
    let mut bio = crate::identity::with_pictures(about, &profile.identity.pictures, width, look);
    let long = about.lines().count() > BIO_LINES || about.chars().count() > BIO_CHARS;
    if clamp && long {
        bio = Element::column(vec![
            bio.max_height(BIO_HEIGHT).clip_children(),
            match &profile.full_action {
                Some(action) => Element::clickable(
                    action.replacen(ids::FULL, ids::FULL_BIO, 1),
                    line(t("Ver biografía completa"), 13.0, look.accent),
                )
                .hover_foreground(look.text),
                None => Element::spacer().size(None, Some(0.0)),
            },
        ])
        .gap(4.0);
    }
    Some(bio)
}

/// The parts of the full profile under its names: what they do, what
/// restricts them, «Sobre mí» and «Miembro desde».
fn about_parts(profile: &Profile, width: f32, look: &Look) -> Vec<Element> {
    let mut body = Vec::new();
    let identity = &profile.identity;
    if !identity.platforms.is_empty() && profile.presence {
        body.push(crate::identity::platforms(&identity.platforms, look));
    }
    body.extend(activity_parts(profile, width, look));
    if profile.bot {
        body.push(text(t("Cuenta automatizada (bot)."), 12.0, look.faint));
    }
    if let Some(restriction) = &profile.restriction {
        body.push(restriction_badge(restriction, look));
    }
    if let Some(about) = bio(profile, false, width, look) {
        body.push(section(t("Sobre mí"), about, look));
    }
    if !profile.member_since.is_empty() {
        body.push(section(
            t("Miembro desde"),
            member_since(&profile.member_since, look),
            look,
        ));
    }
    body
}

/// A round button over a card's banner, 32 px on a dark disc with a white
/// icon, its tooltip the label (REA §3).
fn banner_button(id: &str, glyph: Icon, label: &str, look: &Look) -> Element {
    let glyph = icon(glyph, Color::rgb(255, 255, 255)).size(Some(16.0), Some(16.0));
    let disc = Element::panel(centered(glyph))
        .size(Some(32.0), Some(32.0))
        .radius(16.0)
        .background(Color::rgba(0, 0, 0, 143))
        .border(Color::rgba(255, 255, 255, 20), 1.0);
    let button = Element::clickable(id, disc)
        .accessible(limn::Access::new(limn::Role::Button).name(label.to_owned()));
    Tooltip::new(button, id, label, look.menu_palette()).render()
}

/// The menu a card's button opens, `placement` from the button; closed, the
/// anchor alone. Its width and rows are the person menu's.
fn card_menu(
    anchor: Element,
    entries: Vec<MenuEntry>,
    open: bool,
    dismiss: &str,
    placement: PopoverPlacement,
    look: &Look,
) -> Element {
    let width = menu_width(&entries);
    DropdownMenu::new(
        anchor,
        menu_entries(entries, look),
        open,
        dismiss,
        look.menu_palette(),
    )
    .context_menu_style()
    .context_metrics(crate::look::MENU_METRICS)
    .width(width)
    .placement(placement)
    .render()
}

/// The round buttons over a card's banner, at its top right: the friendship's
/// and «…», each with its menu. Nothing when neither shows.
fn banner_buttons(profile: &Profile, look: &Look) -> Option<Element> {
    let mut buttons = Vec::new();
    if let Some(relation) = &profile.relation {
        let (glyph, label) = match relation.state {
            Friendship::None => (Icon::Add, t("Añadir amigos")),
            Friendship::Friend => (Icon::Check, t("Amigo")),
            Friendship::Outgoing => (Icon::Clock, t("Solicitud de amistad enviada")),
            Friendship::Incoming => (Icon::Clock, t("Solicitud de amistad entrante")),
        };
        let open = profile.open_menu == Some(CardMenu::Relation);
        let entries = if open {
            relation.menu.clone()
        } else {
            Vec::new()
        };
        buttons.push(card_menu(
            banner_button(&relation.action, glyph, label, look),
            entries,
            open,
            ids::CARD_RELATION_DISMISS,
            PopoverPlacement::BottomEnd,
            look,
        ));
    }
    if let (Some(entries), Some(action)) = (&profile.more, profile.full_action.as_ref()) {
        let open = profile.open_menu == Some(CardMenu::More);
        let id = action.replacen(ids::FULL, ids::CARD_MORE, 1);
        let entries = if open { entries.clone() } else { Vec::new() };
        buttons.push(card_menu(
            banner_button(&id, Icon::More, t("Más"), look),
            entries,
            open,
            ids::CARD_MORE_DISMISS,
            PopoverPlacement::BottomEnd,
            look,
        ));
    }
    if buttons.is_empty() {
        return None;
    }
    Some(Element::column(vec![
        gap(0.0, 8.0),
        Element::row(vec![
            Element::spacer().flex(1.0),
            Element::row(buttons).gap(8.0),
            gap(12.0, 0.0),
        ]),
    ]))
}

/// `child` at (`x`, `y`) of the stack it is in, placed by spacers.
fn at(x: f32, y: f32, child: Element) -> Element {
    Element::column(vec![gap(0.0, y), Element::row(vec![gap(x, 0.0), child])])
}

/// The palette of a skeleton on a card's or a modal's surface: its shapes
/// pulse between two strengths of the text color. limn's `raised` is opaque
/// and only five levels off the menu's, so the shapes vanished at the bottom
/// of each pulse.
fn skeleton_palette(look: &Look) -> ComponentPalette {
    ComponentPalette {
        raised: Look::alpha(look.text, 16),
        hover: Look::alpha(look.text, 30),
        ..look.menu_palette()
    }
}

/// The four text lines of a skeleton, 10 px apart, each as wide as its share.
fn skeleton_lines(id: &str, look: &Look) -> Element {
    Element::column(
        SKELETON_LINES
            .iter()
            .enumerate()
            .map(|(n, width)| {
                Skeleton::new(format!("{id}:line:{n}"), skeleton_palette(look))
                    .width(*width)
                    .height(12.0)
                    .radius(4.0)
                    .render()
            })
            .collect(),
    )
    .gap(10.0)
}

/// The card while its profile is read: the shapes of the card's parts where
/// they will be, on its surface, as Discord's skeleton (REA §5b). `id` keeps
/// their ids stable across frames.
pub fn card_skeleton(id: &str, look: &Look) -> Element {
    let palette = skeleton_palette(look);
    let block = |part: &str, width: f32, height: f32, radius: f32| {
        Skeleton::new(format!("{id}:{part}"), palette)
            .width(width)
            .height(height)
            .radius(radius)
            .render()
    };
    // The 6 px ring is the card's own surface, inside the 80 px avatar.
    let avatar = Element::panel(
        Skeleton::circle(format!("{id}:avatar"), palette, AVATAR - 2.0 * RING).render(),
    )
    .padding(RING)
    .radius(AVATAR / 2.0)
    .background(look.menu);
    let chips = Element::row(vec![
        block("chip:0", 52.0, 16.0, 4.0),
        gap(16.0, 0.0),
        block("chip:1", 72.0, 16.0, 4.0),
        gap(16.0, 0.0),
        block("chip:2", 48.0, 16.0, 4.0),
    ]);
    let divider = Element::panel(Element::spacer())
        .size(Some(WIDTH), Some(1.0))
        .background(look.border);
    Element::stack(vec![
        at(0.0, 0.0, block("banner", WIDTH, BANNER, 0.0)),
        at(INSET, 65.0, avatar),
        at(INSET, 157.0, block("name", 147.0, 18.0, 4.0)),
        at(INSET, 183.0, block("handle", 94.0, 12.0, 4.0)),
        at(INSET, 211.0, chips),
        at(0.0, 239.0, divider),
        at(INSET, 256.0, skeleton_lines(id, look)),
    ])
    .size(Some(WIDTH), Some(SKELETON_HEIGHT))
    // An id is what lets the platform hear it: without one the status is dropped.
    .accessible(
        limn::Access::new(limn::Role::Status)
            .id(id)
            .name(t("Cargando perfil")),
    )
}

/// The card of someone else (REA §4): the head with its buttons, then the
/// name with the notice of an ignored or blocked person, what restricts
/// them, their mutual friends and servers, what they do, their bio, their
/// roles, and the note when there is no full profile to open. Its body
/// scrolls between the head and the footer when the window is too short for
/// all of it (a popover is never taller than the window); the effect gives
/// way while the pointer is over the card. While its profile is being read,
/// it is its [`card_skeleton`].
pub fn card(profile: &Profile, look: &Look) -> Element {
    if profile.loading {
        return card_skeleton(&format!("profile:loading:{}", profile.handle), look);
    }
    let bubble =
        custom_status(profile).map(|(custom, emoji)| bubble(custom, emoji, false, None, look));
    let note = profile
        .full_action
        .as_ref()
        .map(|action| action.replacen(ids::FULL, ids::FULL_NOTE, 1));
    let width = WIDTH - 2.0 * INSET;
    let mut body = vec![card_names(profile, note.as_deref(), look)];
    if let Some(notice) = &profile.notice {
        body.push(card_notice(notice, look));
    }
    if let Some(restriction) = &profile.restriction {
        body.push(restriction_badge(restriction, look));
    }
    body.extend(card_mutuals(profile, look));
    body.extend(card_about(profile, width, look));
    body.extend(card_roles(profile, look));
    // Without a full profile to open (the account's own), the field stays.
    if let Some(note) = profile.note.as_ref().filter(|_| note.is_none()) {
        body.push(note_section(note, look));
    }
    let mut parts = vec![
        card_head(profile, bubble, look),
        ScrollArea::new(
            ids::card_body(&profile.handle),
            Element::column(vec![
                gap(0.0, 4.0),
                Element::row(vec![
                    gap(INSET, 0.0),
                    Element::column(body).gap(12.0).flex(1.0),
                    gap(INSET, 0.0),
                ]),
                gap(0.0, 8.0),
            ]),
        )
        .render()
        .flex(1.0),
    ];
    if let Some(footer) = card_footer(profile, look) {
        parts.push(footer);
    }
    crate::identity::with_effect(
        Element::column(parts).hover_region(),
        WIDTH,
        profile.identity.effect.as_ref(),
        Some(ids::CARD_EFFECT),
    )
}

/// The footer of a card (REA §4.5): a one-line field to write to its person,
/// whose hint names them, or on the account's own card «Editar perfil».
fn card_footer(profile: &Profile, look: &Look) -> Option<Element> {
    let row = match &profile.message {
        Some(field) => {
            let hint = tr!("Enviar un mensaje a @{name}", name = profile.name);
            TextField::new(
                field.id.clone(),
                field.text.clone(),
                hint.clone(),
                look.menu_palette(),
            )
            .accessibility_label(hint)
            .render()
            .flex(1.0)
        }
        None if profile.edit_profile => {
            Button::new(ids::EDIT, t("Editar perfil"), look.accent_palette())
                .icon(Icon::Edit)
                .render()
                .flex(1.0)
        }
        None => return None,
    };
    Some(Element::column(vec![
        Element::row(vec![gap(INSET, 0.0), row, gap(INSET, 0.0)]),
        gap(0.0, 12.0),
    ]))
}

/// Badges a card's name row shows before «+N» (REA §4.1).
const BADGES_SHOWN: usize = 6;

/// The name of a card (REA §4.1): the display name at 20 px, its note's icon
/// while the pointer is on the card, and the BOT mark; then the handle, the
/// pronouns, the server tag and the badges, in as many lines as they need.
fn card_names(profile: &Profile, note: Option<&str>, look: &Look) -> Element {
    let mut top = vec![Element::truncated_text(profile.name.clone())
        .font_family(FontFamily::SansBold)
        .font_size(20.0)
        .foreground(look.text)
        .shrink_to_fit()
        .centered_y()];
    if let (Some(action), Some(field)) = (note, &profile.note) {
        let hint = if field.text.is_empty() {
            t("Agregar nota (solo visible para ti)")
        } else {
            t("Editar nota (solo visible para ti)")
        };
        top.push(
            IconButton::new(action, Icon::Edit, look.menu_palette())
                .variant(ButtonVariant::Ghost)
                .tooltip(hint)
                .ink(look.muted)
                .render()
                .centered_y()
                .reveal_on_hover(format!("pp:card:{}:note", profile.handle)),
        );
    }
    if profile.bot {
        top.push(tag("BOT", look).centered_y());
    }
    let mut second = vec![text(profile.handle.clone(), 14.0, look.muted).centered_y()];
    if let Some(pronouns) = &profile.pronouns {
        second.push(
            Element::panel(Element::spacer())
                .size(Some(4.0), Some(4.0))
                .radius(2.0)
                .background(look.text)
                .centered_y(),
        );
        second.push(
            Element::text(pronouns.clone())
                .font_family(FontFamily::SansMedium)
                .font_size(14.0)
                .foreground(look.text)
                .centered_y(),
        );
    }
    if let Some(chip) = &profile.identity.marks.tag {
        second.push(card_tag(chip, look).centered_y());
    }
    second.extend(crate::identity::badge_items(
        &profile.identity.badges,
        20.0,
        BADGES_SHOWN,
        look,
    ));
    Element::column(vec![
        Element::row(top).gap(6.0),
        Element::row(second).gap(6.0).wrap(),
    ])
    .gap(2.0)
}

/// The server tag beside a name: its badge and letters on a raised pill,
/// 24 px tall (REA §4.1).
fn card_tag(tag: &crate::identity::TagChip, look: &Look) -> Element {
    let mut inside = Vec::new();
    if let Some(badge) = &tag.badge {
        inside.push(
            Element::raster_image(badge.clone())
                .size(Some(14.0), Some(14.0))
                .centered_y(),
        );
    }
    inside.push(text(tag.text.clone(), 12.0, look.text).centered_y());
    let hint = match &tag.server {
        Some(server) => tr!("Etiqueta de servidor de {server}", server),
        None => t("Etiqueta de servidor").to_owned(),
    };
    Element::panel(Element::row(vec![
        gap(8.0, 0.0),
        Element::row(inside).gap(4.0).centered_y(),
        gap(8.0, 0.0),
    ]))
    .size(None, Some(24.0))
    .radius(8.0)
    .background(look.raised)
    .border(look.border, 1.0)
    .accessible(limn::Access::default().description(format!("{hint}: {}", tag.text)))
}

/// What the account does with someone it blocks or ignores, as a panel under
/// the name: for an ignored one, a link to lift it (REA §4.6).
fn card_notice(notice: &Notice, look: &Look) -> Element {
    let parts = match notice {
        Notice::Blocked => vec![text(t("Bloqueaste a este usuario"), 14.0, look.muted)],
        Notice::Ignored { unignore } => vec![
            text(t("Ignoraste a este usuario"), 14.0, look.muted),
            text(" • ", 14.0, look.muted),
            Element::clickable(
                unignore.clone(),
                line(t("Dejar de ignorar"), 14.0, look.accent),
            )
            .hover_foreground(look.text),
        ],
    };
    Element::panel(Element::column(vec![
        gap(0.0, 8.0),
        Element::row(vec![
            gap(12.0, 0.0),
            Element::row(parts).wrap(),
            gap(12.0, 0.0),
        ]),
        gap(0.0, 8.0),
    ]))
    .radius(8.0)
    .background(look.raised)
}

/// The mutual friends and servers of a card, each part opening the full
/// profile on its tab (REA §4.2): up to three faces overlapping, then the
/// count. Nothing while they are unknown or there are none.
fn card_mutuals(profile: &Profile, look: &Look) -> Option<Element> {
    let known = |mutual: &Mutual| match mutual {
        Mutual::Known(faces) if !faces.is_empty() => Some(faces.clone()),
        _ => None,
    };
    let (friends, servers) = (
        known(&profile.mutual_friends),
        known(&profile.mutual_servers),
    );
    if friends.is_none() && servers.is_none() {
        return None;
    }
    let base = profile
        .full_action
        .as_ref()?
        .replacen(ids::FULL, ids::MUTUAL, 1);
    let both = friends.is_some() && servers.is_some();
    let size = if both { 12.0 } else { 14.0 };
    let mut parts = Vec::new();
    if let Some(faces) = &friends {
        let words = match faces.len() {
            1 => t("1 amigo en común").to_owned(),
            n => tr!("{n} amigos en común", n),
        };
        parts.push(mutual_part(
            format!("{base}:friends"),
            faces,
            words,
            size,
            look,
        ));
    }
    if both {
        parts.push(
            Element::panel(Element::spacer())
                .size(Some(4.0), Some(4.0))
                .radius(2.0)
                .background(look.muted)
                .centered_y(),
        );
    }
    if let Some(faces) = &servers {
        let words = match faces.len() {
            1 => t("1 servidor en común").to_owned(),
            n => tr!("{n} servidores en común", n),
        };
        parts.push(mutual_part(
            format!("{base}:servers"),
            faces,
            words,
            size,
            look,
        ));
    }
    Some(Element::row(parts).gap(6.0).wrap())
}

/// One part of the mutuals: its faces (16 px, the first three overlapping) and
/// its words, as a button that opens the full profile on its tab.
fn mutual_part(id: String, faces: &[Face], words: String, size: f32, look: &Look) -> Element {
    const FACE: f32 = 16.0;
    const STEP: f32 = 10.0;
    let shown: Vec<Element> = faces
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, known)| at(index as f32 * STEP, 0.0, face(known, FACE)))
        .collect();
    let width = FACE + shown.len().saturating_sub(1) as f32 * STEP;
    let pictures = Element::stack(shown).size(Some(width), Some(FACE));
    Element::clickable(
        id,
        Element::row(vec![
            pictures.centered_y(),
            text(words.clone(), size, look.muted).centered_y(),
        ])
        .gap(6.0),
    )
    .accessible(limn::Access::new(limn::Role::Button).name(words))
}

/// The parts of a card under its mutuals: what the person does, the bot's
/// line and the bio (see [`bio`]), without the headings of the full profile.
fn card_about(profile: &Profile, width: f32, look: &Look) -> Vec<Element> {
    let mut body = activity_parts(profile, width, look);
    if profile.bot {
        body.push(text(t("Cuenta automatizada (bot)."), 12.0, look.faint));
    }
    body.extend(bio(profile, true, width, look));
    body
}

/// The roles of a card, as chips in as many lines as they need. Each the
/// account may take away has a «×»; a «+» (or «Añadir roles» with none) opens
/// the checklist of roles when the account may give them (REA §4.4).
fn card_roles(profile: &Profile, look: &Look) -> Option<Element> {
    let mut chips: Vec<Element> = profile
        .roles
        .iter()
        .enumerate()
        .map(|(index, (name, color))| {
            card_role(
                name,
                *color,
                profile
                    .identity
                    .role_marks
                    .get(index)
                    .and_then(Option::as_ref),
                profile
                    .role_removals
                    .get(index)
                    .and_then(|removal| removal.as_deref()),
                look,
            )
        })
        .collect();
    if let (Some(entries), Some(action)) = (&profile.role_menu, &profile.full_action) {
        let id = action.replacen(ids::FULL, ids::CARD_ROLES, 1);
        let open = profile.open_menu == Some(CardMenu::Roles);
        let menu = if open { entries.clone() } else { Vec::new() };
        let anchor = if profile.roles.is_empty() {
            Button::new(id, t("Añadir roles"), look.menu_palette())
                .variant(ButtonVariant::Ghost)
                .icon(Icon::Add)
                .render()
        } else {
            square_button(
                &id,
                Icon::Add,
                t("Añadir roles"),
                t("Añadir roles").to_owned(),
                look,
            )
        };
        chips.push(card_menu(
            anchor,
            menu,
            open,
            ids::CARD_ROLES_DISMISS,
            PopoverPlacement::BottomStart,
            look,
        ));
    }
    (!chips.is_empty()).then(|| Element::row(chips).gap(4.0).wrap())
}

/// A role of a card: a 24 px chip with its color dot, its picture or emoji and
/// its name, and the «×» that takes it away when `remove` names the action.
fn card_role(
    name: &str,
    color: Color,
    mark: Option<&crate::identity::RoleMark>,
    remove: Option<&str>,
    look: &Look,
) -> Element {
    let mut inside = vec![gap(2.0, 0.0), parts::color_dot(color, 16.0).centered_y()];
    if let Some(mark) = mark {
        inside.push(crate::identity::role_mark(mark, 14.0, look).centered_y());
    }
    inside.push(text(name, 12.0, look.text).centered_y());
    match remove {
        Some(action) => inside.push(square_button(
            action,
            Icon::Close,
            t("Eliminar rol"),
            tr!("Quitar el rol {role}", role = name),
            look,
        )),
        None => inside.push(gap(8.0, 0.0)),
    }
    Element::panel(Element::row(inside).gap(4.0).centered_y())
        .size(None, Some(24.0))
        .radius(8.0)
        .background(look.raised)
        .border(look.border, 1.0)
}

/// A 24 px square button with an icon of 12 px: its tooltip is `hint`, and
/// `name` what a reader hears of it.
fn square_button(id: &str, glyph: Icon, hint: &str, name: String, look: &Look) -> Element {
    let button = Element::clickable(
        id,
        centered(icon(glyph, look.muted).size(Some(12.0), Some(12.0))).size(Some(24.0), Some(24.0)),
    )
    .radius(12.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::Button).name(name));
    Tooltip::new(button, id, hint, look.menu_palette()).render()
}

/// A warning pill: a clock and what restricts them.
fn restriction_badge(label: &str, look: &Look) -> Element {
    Element::panel(
        Element::row(vec![
            icon(Icon::Clock, look.danger).centered_y(),
            text(label, 13.0, look.text).flex(1.0).centered_y(),
        ])
        .gap(6.0),
    )
    .padding(8.0)
    .radius(8.0)
    .background(look.raised)
    .border(look.danger, 1.0)
    .accessible(limn::Access::new(limn::Role::Status).name(label.to_owned()))
}

/// A list of the full profile, as known: unknown is never shown as none.
fn mutual_list(mutual: &Mutual, look: &Look) -> Element {
    match mutual {
        Mutual::Unknown => text(t("No disponible en esta conexión."), 14.0, look.muted),
        Mutual::Loading => text(t("Cargando…"), 14.0, look.muted),
        Mutual::Known(faces) if faces.is_empty() => text(t("Ninguno en común."), 14.0, look.muted),
        Mutual::Known(faces) => Element::column(
            faces
                .iter()
                .map(|known| {
                    Element::panel(
                        Element::row(vec![
                            face(known, 32.0).centered_y(),
                            line(known.name.clone(), 15.0, look.text)
                                .shrink_to_fit()
                                .centered_y(),
                        ])
                        .gap(12.0)
                        .padding(6.0),
                    )
                    .radius(8.0)
                    .background(look.raised)
                })
                .collect(),
        )
        .gap(6.0),
    }
}

/// The label of a tab of the full profile, with its count once known.
fn tab_label(tab: FullTab, full: &FullProfile) -> String {
    let list = match tab {
        FullTab::About => None,
        FullTab::Servers => Some(&full.mutual_servers),
        FullTab::Friends => Some(&full.mutual_friends),
    };
    match list {
        Some(Mutual::Known(faces)) => format!("{} · {}", tab.label(), faces.len()),
        _ => tab.label().to_owned(),
    }
}

/// The full profile over `root`: a larger head with a way to write to them,
/// who they are, and tabs: «Sobre mí» with the server-scoped part named
/// after its server, then the servers and the friends in common.
pub fn full_profile(root: Element, full: &FullProfile, window_width: f32, look: &Look) -> Element {
    let profile = &full.profile;
    let mut beside = vec![match custom_status(profile) {
        Some((custom, emoji)) => bubble(custom, emoji, false, None, look).flex(1.0),
        None => Element::spacer().flex(1.0),
    }];
    if let Some(action) = &profile.message_action {
        beside.push(
            Button::new(action.clone(), t("Enviar mensaje"), look.accent_palette())
                .icon(Icon::Chat)
                .render(),
        );
    }
    let tabs = FullTab::ALL
        .into_iter()
        .map(|tab| {
            let label = tab_label(tab, full);
            // shortcut: widths estimated from the characters, as the friends' tabs do.
            let width = label.chars().count() as f32 * 7.5 + 12.0;
            Tab::new(format!("{}{}", ids::FULL_TAB, tab.key()), label, width)
                .active(tab == full.tab)
        })
        .collect();
    let tabs = Tabs::new(tabs, look.menu_palette())
        .variant(TabsVariant::Underlined)
        .accent(look.accent)
        .id("pp:full:tabs")
        .height(32.0)
        .gap(20.0)
        .render();
    let shown = match full.tab {
        FullTab::About => {
            let mut parts = about_parts(
                profile,
                Modal::fit_width(window_width, FULL_WIDTH) - 2.0 * INSET,
                look,
            );
            if let Some(server) = &full.server {
                let mut scoped = Vec::new();
                if let Some(nick) = &full.nick {
                    scoped.push(text(tr!("Apodo: {nick}", nick), 14.0, look.text));
                }
                scoped.push(if profile.roles.is_empty() {
                    text(t("Sin roles."), 14.0, look.muted)
                } else {
                    role_chips(&profile.roles, &profile.identity.role_marks, look)
                });
                parts.push(section(
                    &tr!("En {server}", server),
                    Element::column(scoped).gap(8.0),
                    look,
                ));
            }
            if !full.connections.is_empty() {
                parts.push(section(
                    t("Conexiones"),
                    crate::identity::connections(&full.connections, look),
                    look,
                ));
            }
            // While the read is in flight nothing of the body is known yet, so
            // it is its lines, never "no hay más datos".
            if full.loading {
                parts = vec![skeleton_lines(
                    &format!("profile:loading:{}:about", profile.handle),
                    look,
                )];
            } else if parts.is_empty() {
                parts.push(text(
                    t("No hay más datos de este perfil."),
                    14.0,
                    look.muted,
                ));
            }
            if let Some(note) = &profile.note {
                parts.push(note_section(note, look));
            }
            Element::column(parts).gap(16.0)
        }
        FullTab::Servers => mutual_list(&full.mutual_servers, look),
        FullTab::Friends => mutual_list(&full.mutual_friends, look),
    };
    let content = Element::column(vec![
        head_sized(
            profile,
            Some(Element::row(beside).gap(8.0)),
            FULL_HEAD,
            look,
        ),
        Element::column({
            let mut top = vec![names(profile, false, None, 24.0, look)];
            if !profile.identity.badges.is_empty() {
                top.push(crate::identity::badges(&profile.identity.badges, look));
            }
            top.extend([tabs, shown]);
            top
        })
        .gap(16.0)
        .padding(INSET),
    ]);
    let content = crate::identity::with_effect(
        content,
        Modal::fit_width(window_width, FULL_WIDTH),
        profile.identity.effect.as_ref(),
        None,
    );
    Modal::new(
        root,
        true,
        t("Perfil"),
        "",
        ids::FULL_CLOSE,
        look.menu_palette(),
    )
    .width(Modal::fit_width(window_width, FULL_WIDTH))
    .height(FULL_HEIGHT)
    .body(content)
    .render()
}

/// The account's own popout.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnPopout {
    pub profile: Profile,
    /// Until when the chosen status lasts, as a person reads it: "hasta
    /// mañana a las 08:00". `None` while it lasts until changed.
    pub until: Option<String>,
    /// Why the custom status cannot be set, when it cannot.
    pub custom_denied: Option<String>,
    /// Why the profile cannot be changed, when it cannot.
    pub edit_denied: Option<String>,
    /// The menu of statuses is open beside the status row.
    pub status_open: bool,
    /// Developer mode is on: the account's ID can be copied.
    pub copy_id: bool,
    /// The provider remembers accounts: «Cambiar de cuenta» is offered.
    pub switch_account: bool,
}

/// A row of the popout's groups: an icon, a label and what goes after it.
fn action_row(id: &str, glyph: Element, label: &str, after: Vec<Element>, look: &Look) -> Element {
    let mut parts = vec![
        Element::row(vec![crate::centered(glyph)])
            .size(Some(20.0), Some(20.0))
            .centered_y(),
        line(label, 16.0, look.text).shrink_to_fit().centered_y(),
    ];
    parts.extend(after.into_iter().map(Element::centered_y));
    Element::clickable(id, Element::row(parts).gap(10.0).padding(8.0))
        .size(None, Some(36.0))
        .radius(6.0)
        .hover_background(look.hover)
        .accessible(limn::Access::new(limn::Role::MenuItem).name(label.to_owned()))
}

/// Rows on a lighter ground, a line between each two.
fn group(rows: Vec<Element>, look: &Look) -> Element {
    let mut children = Vec::new();
    for row in rows {
        if !children.is_empty() {
            children.push(
                Element::panel(Element::spacer())
                    .size(None, Some(1.0))
                    .background(look.border),
            );
        }
        children.push(row);
    }
    Element::panel(Element::column(children).gap(8.0).padding(8.0))
        .radius(8.0)
        .background(look.raised)
}

/// The surface of a menu beside a row.
fn menu_surface(rows: Vec<Element>, width: f32, look: &Look) -> Element {
    Element::panel(Element::column(rows).gap(2.0).menu_navigation())
        .padding(6.0)
        .size(Some(width), None)
        .background(look.menu)
        .border(look.border, 1.0)
        .radius(8.0)
        .shadow(look.shadow, Point::new(0.0, 9.0), 22.0, 0.0)
}

/// The menu of statuses, opened beside the popout's status row: each status
/// but online opens how long it lasts while the pointer is over it.
fn status_menu(current: Status, look: &Look) -> Element {
    let mut rows = Vec::new();
    for (status, key, description) in STATUSES {
        let label = crate::look::status_label(status);
        let mut copy = vec![line(label, 15.0, look.text)];
        if let Some(description) = description {
            copy.push(text(t(description), 12.0, look.muted));
        }
        let mut parts = vec![
            Element::row(vec![crate::centered(status_mark(
                status, look, 10.0, look.menu,
            ))])
            .size(Some(16.0), Some(16.0))
            .centered_y(),
            Element::column(copy).gap(2.0).flex(1.0).centered_y(),
        ];
        if current == status {
            parts.push(icon(Icon::Check, look.accent).centered_y());
        }
        if status != Status::Online {
            parts.push(icon(Icon::ChevronRight, look.muted).centered_y());
        }
        let row = Element::clickable(
            format!("{}{key}", ids::STATUS),
            Element::row(parts).gap(10.0).padding(8.0),
        )
        // A description may take two lines: the row grows with it.
        .min_size(0.0, if description.is_some() { 56.0 } else { 40.0 })
        .radius(4.0)
        .hover_background(look.hover)
        .accessible(
            limn::Access::new(limn::Role::MenuItem)
                .name(label)
                .selected(current == status),
        );
        if status == Status::Online {
            rows.push(row);
            rows.push(
                Element::panel(Element::spacer())
                    .size(None, Some(1.0))
                    .background(look.border),
            );
            continue;
        }
        let durations = DURATIONS
            .iter()
            .map(|(duration, label, _)| {
                DropdownMenuEntry::Item(DropdownMenuItem::new(
                    format!("{}{key}:{duration}", ids::STATUS),
                    t(label),
                ))
            })
            .chain([DropdownMenuEntry::Item(DropdownMenuItem::new(
                format!("{}{key}:forever", ids::STATUS),
                t("Para siempre"),
            ))])
            .collect();
        rows.push(
            DropdownMenu::new(
                row,
                durations,
                true,
                format!("{}{key}", ids::SUBMENU),
                look.menu_palette(),
            )
            .width(DURATION_MENU_WIDTH)
            .placement(PopoverPlacement::RightStart)
            .render()
            .popover_on_hover(),
        );
    }
    menu_surface(rows, STATUS_MENU_WIDTH, look)
}

/// The account's popout over its panel: its card, a bubble to set the
/// custom status, «Editar perfil», the status with its menu, and copying
/// its id.
pub fn own_popout(popout: &OwnPopout, look: &Look) -> Element {
    let profile = &popout.profile;
    let action = popout.custom_denied.is_none().then_some(ids::CUSTOM);
    let bubble = match custom_status(profile) {
        Some((custom, emoji)) => Some(bubble(custom, emoji, false, action, look)),
        None if action.is_some() => Some(bubble(
            t("Establecer estado personalizado"),
            None,
            true,
            action,
            look,
        )),
        None => None,
    };
    let mut body = vec![names(profile, true, None, 20.0, look)];
    if let Some(about) = &profile.about {
        body.push(section(
            t("Sobre mí"),
            crate::identity::with_pictures(
                about,
                &profile.identity.pictures,
                WIDTH - 2.0 * INSET,
                look,
            ),
            look,
        ));
    }
    let edit = action_row(
        ids::EDIT,
        icon(Icon::Edit, look.muted),
        if popout.edit_denied.is_some() {
            t("Ver perfil")
        } else {
            t("Editar perfil")
        },
        Vec::new(),
        look,
    );
    let edit = match &popout.edit_denied {
        Some(reason) => Tooltip::new(edit, ids::EDIT, reason.clone(), look.menu_palette()).render(),
        None => edit,
    };
    let mut after = Vec::new();
    if profile.status == Status::Invisible {
        let warning = icon(Icon::WarningTriangle, look.idle);
        after.push(
            Tooltip::new(
                Element::panel(warning).accessibility_label(t("Atención")),
                "pp:invisible-warning",
                t("Mientras estés invisible, los demás no ven tu estado ni tu actividad."),
                look.menu_palette(),
            )
            .render(),
        );
    }
    after.push(Element::spacer().flex(1.0));
    if let Some(until) = &popout.until {
        after.push(line(until.clone(), 12.0, look.muted));
    }
    after.push(icon(Icon::ChevronRight, look.muted));
    let status = action_row(
        ids::STATUS_MENU,
        status_mark(profile.status, look, 12.0, look.raised),
        crate::look::status_label(profile.status),
        after,
        look,
    );
    // The pointer or the focus on the row opens the status menu, and keeps it
    // while the durations nested in it are used; a click keeps it open.
    let status = Element::popover(
        status,
        status_menu(profile.status, look),
        ids::STATUS_MENU_DISMISS,
        PopoverPlacement::RightStart,
    );
    let status = if popout.status_open {
        status
    } else {
        status.popover_on_hover()
    };
    let mut actions = vec![edit];
    if profile.presence {
        actions.push(status);
    }
    body.push(group(actions, look));
    if popout.switch_account {
        body.push(group(
            vec![action_row(
                ids::SWITCH_ACCOUNT,
                icon(Icon::Users, look.muted),
                t("Cambiar de cuenta"),
                vec![
                    Element::spacer().flex(1.0),
                    icon(Icon::ChevronRight, look.muted),
                ],
                look,
            )],
            look,
        ));
    }
    if popout.copy_id {
        body.push(group(
            vec![action_row(
                ids::COPY_ID,
                icon(Icon::Copy, look.muted),
                t("Copiar ID del usuario"),
                Vec::new(),
                look,
            )],
            look,
        ));
    }
    Element::column(vec![
        head(profile, bubble, look),
        Element::column(body).gap(12.0).padding(INSET),
    ])
}

/// An audio device as the lists show it.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// The one the system uses unless told otherwise.
    pub default: bool,
}

/// The custom status dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomStatusDialog {
    /// The account, as the preview shows it.
    pub profile: Profile,
    pub emoji: Option<String>,
    pub text: String,
    pub emoji_open: bool,
    pub clear_after: ClearAfter,
    pub expiry_open: bool,
    /// The wall clock, for when the status goes.
    pub now: Timestamp,
}

/// The emoji and the text as one line: what the status will say.
pub fn custom_line(emoji: Option<&str>, text: &str) -> Option<String> {
    let text = text.trim();
    match (emoji, text.is_empty()) {
        (Some(emoji), true) => Some(emoji.to_owned()),
        (Some(emoji), false) => Some(format!("{emoji} {text}")),
        (None, true) => None,
        (None, false) => Some(text.to_owned()),
    }
}

/// Why `line` cannot be the custom status, when it cannot.
pub fn custom_error(line: Option<&str>) -> Option<String> {
    line.filter(|line| line.chars().count() > CUSTOM_MAX)
        .map(|_| {
            tr!(
                "El estado puede tener hasta {max} caracteres.",
                max = CUSTOM_MAX
            )
        })
}

/// The dialog that sets the custom status over `root`: a preview whose
/// bubble says what is written, the field with its emoji, when it goes,
/// and saving.
pub fn custom_status_dialog(
    root: Element,
    dialog: &CustomStatusDialog,
    window_width: f32,
    look: &Look,
) -> Element {
    let palette = look.menu_palette();
    let said = custom_line(dialog.emoji.as_deref(), &dialog.text);
    let error = custom_error(said.as_deref());
    let preview = {
        let bubble = bubble(
            said.as_deref().unwrap_or(t("¿Qué estás haciendo?")),
            None,
            said.is_none(),
            None,
            look,
        );
        Element::panel(Element::column(vec![
            head(&dialog.profile, Some(bubble), look),
            Element::column(vec![names(&dialog.profile, false, None, 20.0, look)]).padding(INSET),
        ]))
        .size(Some(WIDTH), None)
        .radius(12.0)
        .background(look.menu)
        .border(look.border, 1.0)
        .clip_children()
    };
    let face = match &dialog.emoji {
        Some(emoji) => Element::text(emoji.clone())
            .font_family(FontFamily::Sans)
            .font_size(20.0)
            .foreground(look.text),
        None => icon(Icon::Emoji, look.muted),
    };
    let emoji_button = Element::clickable(ids::CUSTOM_EMOJI, crate::centered(face))
        .size(Some(40.0), Some(40.0))
        .radius(20.0)
        .hover_background(look.hover)
        .accessible(limn::Access::new(limn::Role::Button).name(t("Elegir un emoji")));
    let emoji_button = Popover::new(
        emoji_button,
        emoji::picker(ids::CUSTOM_EMOJI_PICK, t("Emoji del estado"), look),
        dialog.emoji_open,
        ids::CUSTOM_EMOJI_DISMISS,
        palette,
    )
    .width(emoji::WIDTH + 12.0)
    .placement(PopoverPlacement::BottomStart)
    .render();
    let field = Element::row(vec![
        gap(4.0, 0.0),
        emoji_button.centered_y(),
        Element::text_input(
            ids::CUSTOM_TEXT,
            dialog.text.clone(),
            t("¿Qué estás haciendo?"),
        )
        .foreground(look.text)
        .font_family(FontFamily::Sans)
        .font_size(16.0)
        .padding(8.0)
        .accessibility_label(t("Estado"))
        .flex(1.0)
        .centered_y(),
    ])
    .size(None, Some(49.5));
    let field = Element::panel(field)
        .radius(8.0)
        .background(look.canvas)
        .border(
            if error.is_some() {
                look.danger
            } else {
                look.border
            },
            1.0,
        );
    let mut body = vec![
        Element::row(vec![
            Element::spacer().flex(1.0),
            preview,
            Element::spacer().flex(1.0),
        ]),
        gap(0.0, 12.0),
        line(t("Estado"), 16.0, look.text),
        field,
    ];
    if let Some(error) = &error {
        body.push(text(error.clone(), 13.0, look.danger));
    }
    let when = match dialog.clear_after.minutes() {
        Some(minutes) => tr!(
            "Eliminar {when}",
            when = until_label(dialog.now.plus_minutes(minutes), dialog.now)
        ),
        None => t("No eliminar").to_owned(),
    };
    let expiry = Element::clickable(
        ids::CUSTOM_EXPIRY,
        Element::row(vec![
            line(when, 16.0, look.text).shrink_to_fit().centered_y(),
            icon(Icon::ChevronDown, look.text).centered_y(),
        ])
        .gap(8.0)
        .padding(4.0),
    )
    .radius(4.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Borrar después de")));
    let mut choices = vec![DropdownMenuEntry::Heading(t("Eliminar después").into())];
    choices.extend(ClearAfter::ALL.into_iter().map(|choice| {
        DropdownMenuEntry::Item(
            DropdownMenuItem::new(
                format!("{}{}", ids::CUSTOM_EXPIRY_PICK, choice.key()),
                choice.label(dialog.now),
            )
            .selected(choice == dialog.clear_after),
        )
    }));
    let expiry = DropdownMenu::new(
        expiry,
        choices,
        dialog.expiry_open,
        ids::CUSTOM_EXPIRY_DISMISS,
        palette,
    )
    .width(EXPIRY_MENU_WIDTH)
    .placement(PopoverPlacement::BottomStart)
    .render();
    let footer = Element::row(vec![
        expiry.centered_y(),
        Element::spacer().flex(1.0),
        Button::new(ids::CUSTOM_CANCEL, t("Cancelar"), palette)
            .variant(ButtonVariant::Link)
            .render()
            .centered_y(),
        Button::new(ids::CUSTOM_SAVE, t("Guardar"), look.accent_palette())
            .width(106.0)
            .enabled(error.is_none())
            .render()
            .centered_y(),
    ])
    .gap(12.0)
    .size(None, Some(42.0));
    Modal::new(
        root,
        true,
        t("Establecer tu estado"),
        "",
        ids::CUSTOM_CLOSE,
        palette,
    )
    .width(Modal::fit_width(window_width, DIALOG_WIDTH))
    .body(Element::column(body).gap(8.0))
    .footer(footer)
    .initial_focus(ids::CUSTOM_TEXT)
    .render()
}

/// What the profile editor shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    /// The account as the preview shows it, with what was changed here.
    pub preview: Profile,
    /// What the fields hold.
    pub name: String,
    pub pronouns: String,
    pub about: String,
    pub accent: Rgb,
    /// The custom color field, while open.
    pub hex: Option<String>,
    /// Why the provider refused the last change.
    pub failure: Option<String>,
    /// Why the profile cannot be changed at all, when it cannot.
    pub denied: Option<String>,
    /// The servers, for the name the account goes by in each.
    pub servers: Vec<EditorServer>,
    pub servers_open: bool,
    /// The bar of unsaved changes, while something differs.
    pub bar: Option<UnsavedBar>,
    /// The avatar and the banner, and what may be done with them.
    pub pictures: EditorPictures,
    /// The server whose profile is edited; `None` for the one shown everywhere.
    pub scope: Option<EditorScope>,
    /// Why the username and the email are not changed here, said beside them.
    pub credentials: Option<String>,
    /// Where the color picker's thumbs stand while it is used.
    pub picked: Option<limn::Hsv>,
}

/// The pictures of the profile being edited and what may be done with them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorPictures {
    /// The banner as it would show; without it, its color.
    pub banner: Option<limn::RasterImage>,
    /// Whether there is one to take away.
    pub has_avatar: bool,
    pub has_banner: bool,
    /// The picture being chosen: the avatar (`Some(true)`) or the banner.
    pub picking: Option<bool>,
    /// Why they cannot change, when they cannot.
    pub denied: Option<String>,
    /// A line under them, as the simulation's.
    pub note: Option<String>,
    // Profile editing (edicion-de-perfil).
    /// Why a new banner cannot be chosen, as the plan says; one there may
    /// still be taken away.
    pub banner_denied: Option<String>,
    /// The picture last chosen, to frame it.
    pub framing: Option<EditorFraming>,
}

/// A picture chosen for the profile, whole, and the part of it kept. All
/// shares are of the picture's width and height.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorFraming {
    pub image: limn::RasterImage,
    /// Its width over its height.
    pub aspect: f32,
    /// The part kept: left, top, width and height.
    pub frame: [f32; 4],
    /// Where the frame's middle is asked to be.
    pub x: f32,
    pub y: f32,
    /// How close, from 0 (the whole frame that fits) to 1.
    pub zoom: f32,
    /// An avatar's frame: it shows round.
    pub round: bool,
    /// Whether it moves and stays moving, or why not.
    pub note: Option<String>,
}

/// The server whose profile is edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorScope {
    /// Its name.
    pub server: String,
    /// The name the account goes by there.
    pub nick: String,
    /// Why that name cannot change, when it cannot.
    pub nick_denied: Option<String>,
    /// Why «Sobre mí» there cannot change, when it cannot.
    pub about_denied: Option<String>,
}

/// What is wrong with the editor's fields, by field: each blocks saving.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorErrors {
    pub name: Option<String>,
    pub pronouns: Option<String>,
    pub about: Option<String>,
}

impl EditorErrors {
    /// The errors of fields holding these values.
    pub fn of(name: &str, pronouns: &str, about: &str) -> Self {
        let too_long = |value: &str, max: usize| value.trim().chars().count() > max;
        Self {
            name: too_long(name, NAME_MAX).then(|| {
                tr!(
                    "El nombre puede tener hasta {max} caracteres.",
                    max = NAME_MAX
                )
            }),
            pronouns: too_long(pronouns, PRONOUNS_MAX).then(|| {
                tr!(
                    "Los pronombres pueden tener hasta {max} caracteres.",
                    max = PRONOUNS_MAX
                )
            }),
            about: too_long(about, ABOUT_MAX).then(|| {
                tr!(
                    "«Sobre mí» puede tener hasta {max} caracteres.",
                    max = ABOUT_MAX
                )
            }),
        }
    }

    pub fn any(&self) -> bool {
        self.name.is_some() || self.pronouns.is_some() || self.about.is_some()
    }
}

/// The square button that closes the editor.
fn close_square(look: &Look) -> Element {
    let button = Element::clickable(
        ids::EDITOR_CLOSE,
        crate::centered(icon(Icon::Close, look.text).size(Some(16.0), Some(16.0))),
    )
    .size(Some(CLOSE), Some(CLOSE))
    .radius(8.0)
    .background(look.raised)
    .hover_background(look.selected)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Cerrar (Esc)")));
    Tooltip::new(button, ids::EDITOR_CLOSE, t("Cerrar"), look.menu_palette())
        .shortcut("Esc")
        .placement(TooltipPlacement::Below)
        .render()
}

/// A field's title over it, and its error under it.
fn field(title: &str, control: Element, error: Option<&String>, look: &Look) -> Element {
    let mut parts = vec![line(title, 16.0, look.text), control];
    if let Some(error) = error {
        parts.push(text(error.clone(), 13.0, look.danger));
    }
    Element::column(parts).gap(8.0)
}

/// A server of the profile choice, marked as the rail marks it.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorServer {
    /// What changes the name the account goes by there.
    pub action: String,
    pub name: String,
    pub nick: Option<String>,
    /// Its icon, once loaded; its initials on `accent` without one.
    pub image: Option<limn::RasterImage>,
    pub initials: String,
    pub accent: Color,
}

/// A server's icon in a menu row: its picture, or its first initial on its
/// color, rounded as the rail rounds it.
fn server_mark(server: &EditorServer) -> Element {
    match &server.image {
        Some(image) => Element::raster_image(image.clone().cover()).radius(5.0),
        None => Element::panel(crate::centered(
            Element::text(
                server
                    .initials
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or_default(),
            )
            .font_family(limn::FontFamily::SansMedium)
            .font_size(9.0)
            .foreground(Color::rgb(255, 255, 255)),
        ))
        .radius(5.0)
        .background(server.accent),
    }
}

/// A profile choice, shown only when there are editable server profiles.
fn editor_selector(view: &Editor, look: &Look) -> Element {
    let current = view.scope.as_ref().map_or_else(
        || t("Perfil principal").to_owned(),
        |scope| tr!("Perfil en {server}", server = scope.server),
    );
    let selector = Element::clickable(
        ids::SERVERS,
        Element::row(vec![
            strong_line(current, 16.0, look.text)
                .shrink_to_fit()
                .centered_y(),
            Element::spacer().flex(1.0),
            icon(Icon::ChevronDown, look.muted).centered_y(),
        ])
        .gap(6.0)
        .padding(10.0),
    )
    .radius(6.0)
    .background(look.raised)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Elegir perfil")));
    let mut profiles = vec![
        DropdownMenuEntry::Item(
            DropdownMenuItem::new(ids::MAIN_PROFILE, t("Perfil principal"))
                .icon(Icon::Custom(crate::MARK))
                .selected(view.scope.is_none()),
        ),
        DropdownMenuEntry::Heading(t("Perfil en cada servidor").into()),
    ];
    profiles.extend(view.servers.iter().map(|server| {
        let item = DropdownMenuItem::new(server.action.clone(), server.name.clone())
            .leading(server_mark(server))
            .selected(
                view.scope
                    .as_ref()
                    .is_some_and(|scope| scope.server == server.name),
            );
        DropdownMenuEntry::Item(match &server.nick {
            Some(nick) => item.annotation(nick.clone()),
            None => item,
        })
    }));
    DropdownMenu::new(
        selector,
        profiles,
        view.servers_open,
        ids::SERVERS_DISMISS,
        look.menu_palette(),
    )
    .width(280.0)
    .render()
}

fn editor_avatar(view: &Editor, look: &Look) -> Element {
    Element::row(vec![
        crate::avatar_with_image(
            &view.preview.name,
            view.preview.accent,
            64.0,
            view.preview.image.clone(),
        ),
        Element::column(vec![
            strong_line(t("Avatar"), 15.0, look.text),
            text(t("Imagen de tu cuenta"), 13.0, look.muted),
        ])
        .gap(4.0)
        .centered_y()
        .flex(1.0),
    ])
    .gap(16.0)
    .padding(16.0)
    .radius(10.0)
    .background(look.raised)
}

/// «Cambiar» and «Quitar» of one picture: the first chooses a file, loading
/// while the selector is open; the second shows only with one to take away.
fn picture_actions(
    pick: &'static str,
    remove: &'static str,
    has: bool,
    picking: bool,
    pictures: &EditorPictures,
    look: &Look,
) -> Element {
    let can = pictures.denied.is_none();
    let chooses = pick != ids::BANNER_PICK || pictures.banner_denied.is_none();
    let mut actions = vec![Button::new(pick, t("Cambiar"), look.accent_palette())
        .enabled(can && chooses && pictures.picking.is_none())
        .loading(picking)
        .render()];
    if has {
        actions.push(
            Button::new(remove, t("Quitar"), look.menu_palette())
                .variant(ButtonVariant::Ghost)
                .enabled(can && pictures.picking.is_none())
                .render(),
        );
    }
    Element::row(actions).gap(8.0)
}

/// The avatar and the banner, each beside its actions, and what limits them.
fn editor_pictures(view: &Editor, width: f32, look: &Look) -> Element {
    let pictures = &view.pictures;
    let inner = (width - 24.0).max(1.0);
    // The faces share a width, so both titles start in one column.
    let row = |face: Element, title: &str, actions: Element| {
        Element::row(vec![
            Element::row(vec![face.centered_y()]).size(Some(100.0), None),
            strong_line(title, 15.0, look.text).flex(1.0).centered_y(),
            actions.centered_y(),
        ])
        .gap(12.0)
    };
    let avatar = row(
        crate::avatar_with_image(
            &view.preview.name,
            view.preview.accent,
            48.0,
            view.preview.image.clone(),
        ),
        t("Avatar"),
        picture_actions(
            ids::AVATAR_PICK,
            ids::AVATAR_REMOVE,
            pictures.has_avatar,
            pictures.picking == Some(true),
            pictures,
            look,
        ),
    );
    let thumbnail = Element::panel(match &pictures.banner {
        Some(image) => Element::raster_image(image.clone().cover()).size(None, Some(40.0)),
        None => Element::spacer(),
    })
    .size(Some(100.0), Some(40.0))
    .radius(6.0)
    .background(view.preview.accent)
    .clip_children()
    .accessible(limn::Access::new(limn::Role::Image).name(t("Vista previa del banner")));
    let banner = row(
        thumbnail,
        t("Banner"),
        picture_actions(
            ids::BANNER_PICK,
            ids::BANNER_REMOVE,
            pictures.has_banner,
            pictures.picking == Some(false),
            pictures,
            look,
        ),
    );
    let mut parts = vec![avatar, banner];
    let hint = if view.scope.is_some() {
        t("Se muestran en este servidor en lugar de los de tu cuenta. PNG, JPEG, GIF o WebP de hasta 8 MB; después de elegirla, la encuadras.")
    } else {
        t("PNG, JPEG, GIF o WebP de hasta 8 MB: el avatar cuadrado, el banner en 5:2. Después de elegirla, la encuadras.")
    };
    parts.push(text(hint, 12.0, look.muted).size(Some(inner), None));
    for line in [&pictures.denied, &pictures.banner_denied, &pictures.note]
        .into_iter()
        .flatten()
    {
        parts.push(text(line.clone(), 12.0, look.muted).size(Some(inner), None));
    }
    if let Some(framing) = &pictures.framing {
        parts.push(editor_framing(framing, inner, look));
    }
    Element::column(parts)
        .gap(10.0)
        .padding(12.0)
        .radius(10.0)
        .background(look.raised)
        .anchor(ids::PICTURES)
}

/// The picture last chosen, whole, with the part kept framed over it:
/// dragging over it (or its arrows) moves the frame, the slider brings it
/// closer (edicion-de-perfil).
fn editor_framing(view: &EditorFraming, width: f32, look: &Look) -> Element {
    const TALLEST: f32 = 220.0;
    let aspect = if view.aspect.is_finite() && view.aspect > 0.0 {
        view.aspect
    } else {
        1.0
    };
    let shown_width = width.min(TALLEST * aspect).max(1.0);
    let shown_height = shown_width / aspect;
    let [left, top, wide, high] = view.frame;
    let (frame_width, frame_height) = (wide * shown_width, high * shown_height);
    let frame = Element::panel(Element::spacer())
        .size(Some(frame_width), Some(frame_height))
        .radius(if view.round {
            frame_width.min(frame_height) / 2.0
        } else {
            4.0
        })
        .border(look.accent, 2.0)
        .translate(Point::new(left * shown_width, top * shown_height));
    let picture = Element::clickable(
        ids::FRAME,
        Element::stack(vec![
            Element::raster_image(view.image.clone()).size(Some(shown_width), Some(shown_height)),
            // A column keeps the frame at its own size, from the corner.
            Element::column(vec![frame]),
        ])
        .size(Some(shown_width), Some(shown_height)),
    )
    .area_slider(view.x, view.y, 0.0)
    .radius(6.0)
    .clip_children()
    .accessible(
        limn::Access::new(limn::Role::Slider)
            .name(t("Encuadre"))
            .value(view.x)
            .value_text(format!(
                "{}%, {}%",
                (view.x * 100.0).round(),
                (view.y * 100.0).round()
            )),
    );
    let zoom = Slider::new(ids::FRAME_ZOOM, view.zoom, look.palette())
        .ink(look.accent)
        .accessibility_label(t("Zoom del encuadre"))
        .render();
    let mut parts = vec![
        strong_line(t("Encuadre"), 15.0, look.text),
        Element::row(vec![
            Element::spacer().flex(1.0),
            picture,
            Element::spacer().flex(1.0),
        ]),
        Element::row(vec![
            line(t("Zoom"), 13.0, look.muted).centered_y(),
            zoom.flex(1.0).centered_y(),
        ])
        .gap(12.0),
        text(
            t("Arrastra sobre la imagen para elegir qué parte se ve; el zoom la acerca."),
            12.0,
            look.muted,
        )
        .size(Some(width), None),
    ];
    if let Some(note) = &view.note {
        parts.push(text(note.clone(), 12.0, look.muted).size(Some(width), None));
    }
    Element::column(parts).gap(8.0)
}

/// The fields of a server's profile: the name there and «Sobre mí» there.
fn scoped_fields(view: &Editor, scope: &EditorScope, look: &Look) -> Vec<Element> {
    let palette = look.palette();
    let errors = EditorErrors::of(&scope.nick, "", &view.about);
    let mut nick = vec![TextField::new(
        ids::NICK,
        scope.nick.clone(),
        view.preview.handle.clone(),
        palette,
    )
    .height(40.0)
    .enabled(scope.nick_denied.is_none())
    .accessibility_label(t("Apodo"))
    .render()];
    if let Some(reason) = &scope.nick_denied {
        nick.push(text(reason.clone(), 12.0, look.muted));
    }
    let count = view.about.trim().chars().count();
    let about = Element::column(vec![
        TextArea::new(
            ids::ABOUT,
            view.about.clone(),
            t("Cuenta algo para este servidor."),
            palette,
        )
        .accessibility_label(t("Sobre mí en este servidor"))
        .height(100.0)
        .enabled(scope.about_denied.is_none())
        .render(),
        Element::row(vec![
            Element::spacer().flex(1.0),
            line(
                format!("{count}/{ABOUT_MAX}"),
                12.0,
                if count > ABOUT_MAX {
                    look.danger
                } else {
                    look.muted
                },
            ),
        ]),
    ])
    .gap(4.0);
    let about = match &scope.about_denied {
        Some(reason) => {
            Element::column(vec![about, text(reason.clone(), 12.0, look.muted)]).gap(6.0)
        }
        None => about,
    };
    vec![
        field(
            t("Apodo"),
            Element::column(nick).gap(6.0),
            errors.name.as_ref(),
            look,
        ),
        field(t("Sobre mí"), about, errors.about.as_ref(), look),
    ]
}

/// Missing optional data means it was not supplied; it does not prove the user
/// left that field empty on the service.
fn profile_value(label: &str, value: Option<&str>, look: &Look) -> Element {
    let value = value.filter(|value| !value.trim().is_empty());
    Element::column(vec![
        strong_line(label, 13.0, look.muted),
        text(
            value.unwrap_or(t("No disponible")),
            15.0,
            if value.is_some() {
                look.text
            } else {
                look.faint
            },
        ),
    ])
    .gap(6.0)
}

fn editor_fields(view: &Editor, width: f32, look: &Look) -> Vec<Element> {
    let palette = look.palette();
    let errors = EditorErrors::of(&view.name, &view.pronouns, &view.about);
    let mut fields = vec![
        field(
            t("Nombre para mostrar"),
            TextField::new(
                ids::NAME,
                view.name.clone(),
                view.preview.handle.clone(),
                palette,
            )
            .height(40.0)
            .accessibility_label(t("Nombre para mostrar"))
            .render(),
            errors.name.as_ref(),
            look,
        ),
        field(
            t("Pronombres"),
            TextField::new(
                ids::PRONOUNS,
                view.pronouns.clone(),
                t("Agrega tus pronombres"),
                palette,
            )
            .height(40.0)
            .accessibility_label(t("Pronombres"))
            .render(),
            errors.pronouns.as_ref(),
            look,
        ),
    ];
    let count = view.about.trim().chars().count();
    let about = Element::column(vec![
        TextArea::new(
            ids::ABOUT,
            view.about.clone(),
            t("Cuenta algo sobre ti."),
            palette,
        )
        .accessibility_label(t("Sobre mí"))
        .height(100.0)
        .render(),
        Element::row(vec![
            Element::spacer().flex(1.0),
            line(
                format!("{count}/{ABOUT_MAX}"),
                12.0,
                if count > ABOUT_MAX {
                    look.danger
                } else {
                    look.muted
                },
            ),
        ]),
    ])
    .gap(4.0);
    fields.push(field(t("Sobre mí"), about, errors.about.as_ref(), look));
    fields.push(field(
        t("Color del perfil"),
        parts::swatches(
            &Swatches {
                prefix: ids::COLOR.into(),
                chosen: Some(view.accent),
                default: None,
                custom: view.hex.clone(),
                enabled: true,
                narrow: width < parts::swatches_width(false),
                picked: view.picked,
            },
            look,
        ),
        None,
        look,
    ));
    fields.push(editor_pictures(view, width, look));
    if let Some(reason) = &view.credentials {
        fields.push(
            Element::column(vec![
                profile_value(t("Nombre de usuario"), Some(&view.preview.handle), look),
                text(reason.clone(), 12.0, look.muted).size(Some(width), None),
            ])
            .gap(6.0),
        );
    }
    fields
}

/// Keep the save controls outside the scrolling form. Narrow windows use two
/// lines instead of forcing a long warning and two buttons into the same row.
fn editor_footer(bar: UnsavedBar, width: f32, look: &Look) -> Element {
    let message = if bar.nudged {
        t("Guarda o restablece los cambios antes de salir")
    } else {
        t("Cambios sin guardar")
    };
    let actions = Element::row(vec![
        Button::new(ids::RESET, t("Restablecer"), look.menu_palette())
            .variant(ButtonVariant::Secondary)
            .enabled(!bar.saving)
            .width((width - 32.0) / 2.0)
            .render(),
        Button::new(ids::SAVE, t("Guardar"), look.accent_palette())
            .loading(bar.saving)
            .width((width - 32.0) / 2.0)
            .render(),
    ])
    .gap(8.0);
    Element::column(vec![
        text(
            message,
            13.0,
            if bar.nudged { look.danger } else { look.muted },
        ),
        actions,
    ])
    .gap(8.0)
    .padding(12.0)
    .size(Some(width), None)
    .background(look.menu)
    .radius(10.0)
    .border(look.border, 1.0)
}

/// The account's actual data and a preview, with editing controls only when
/// the provider supports them. No form or footer overlaps the scroll viewport.
pub fn editor(view: &Editor, window: Size, look: &Look) -> Element {
    let can = view.denied.is_none();
    let available = (window.width - 48.0).max(1.0);
    let side_by_side = available >= 320.0 + PREVIEW_GAP + WIDTH;
    let form_width = if side_by_side {
        (available - PREVIEW_GAP - WIDTH).min(EDITOR_FORM)
    } else {
        available.min(EDITOR_FORM)
    };
    let preview_width = if side_by_side {
        WIDTH
    } else {
        form_width.min(WIDTH)
    };
    let content_width = if side_by_side {
        form_width + PREVIEW_GAP + preview_width
    } else {
        form_width
    };
    let horizontal = |child: Element| {
        Element::row(vec![
            Element::spacer().flex(1.0),
            child,
            Element::spacer().flex(1.0),
        ])
    };
    let heading = Element::row(vec![
        Element::text(if can {
            t("Editar perfil")
        } else {
            t("Tu perfil")
        })
        .font_size(22.0)
        .font_family(FontFamily::SansMedium)
        .foreground(look.text)
        .accessible(limn::Access::new(limn::Role::Heading).id("profile:title"))
        .flex(1.0)
        .centered_y(),
        close_square(look),
    ])
    .gap(16.0)
    .size(Some(content_width), None);
    let header = Element::column(vec![gap(0.0, 20.0), horizontal(heading), gap(0.0, 20.0)]);
    let mut form = Vec::new();
    if can && !view.servers.is_empty() {
        form.push(editor_selector(view, look));
    }
    if !can {
        form.push(editor_avatar(view, look));
    }
    if can {
        if let Some(failure) = &view.failure {
            form.push(
                text(
                    tr!("No se pudo guardar: {failure}", failure),
                    13.0,
                    look.danger,
                )
                .accessible(limn::Access::new(limn::Role::Alert).name(failure.clone())),
            );
        }
        match &view.scope {
            Some(scope) => {
                form.extend(scoped_fields(view, scope, look));
                form.push(editor_pictures(view, form_width, look));
            }
            None => form.extend(editor_fields(view, form_width, look)),
        }
    } else {
        form.extend([
            profile_value(t("Nombre para mostrar"), Some(&view.preview.name), look),
            profile_value(t("Nombre de usuario"), Some(&view.preview.handle), look),
            profile_value(t("Pronombres"), view.preview.pronouns.as_deref(), look),
            profile_value(t("Sobre mí"), view.preview.about.as_deref(), look),
            text(
                t("La edición de este perfil todavía no está disponible."),
                12.0,
                look.muted,
            ),
        ]);
    }
    // A hairline between each two parts of the form, as in the reference.
    let mut parted = Vec::new();
    for part in form {
        if !parted.is_empty() {
            parted.push(divider(look));
        }
        parted.push(part);
    }
    let form = Element::column(parted)
        .gap(18.0)
        .size(Some(form_width), None);
    let preview = Element::column(vec![
        strong_line(
            if can { t("Vista previa") } else { t("Perfil") },
            14.0,
            look.muted,
        ),
        Element::panel(card(&view.preview, look))
            .size(Some(preview_width), None)
            .radius(12.0)
            .background(look.menu)
            .border(look.border, 1.0)
            .clip_children(),
    ])
    .gap(10.0)
    .size(Some(preview_width), None);
    let content = if side_by_side {
        Element::row(vec![form, preview]).gap(PREVIEW_GAP)
    } else {
        Element::column(vec![form, preview]).gap(24.0)
    }
    .size(Some(content_width), None);
    let scroll = ScrollArea::new(
        "profile:form",
        Element::column(vec![horizontal(content), gap(0.0, 24.0)]),
    )
    .render()
    .flex(1.0);
    let mut rows = vec![header, scroll];
    if let Some(bar) = view.bar.filter(|_| can) {
        // One row as in the settings when it fits; else stacked.
        let bar = if content_width >= WIDE_BAR {
            parts::unsaved_bar_for(ids::BAR, bar, look).size(Some(content_width), None)
        } else {
            editor_footer(bar, content_width, look)
        };
        rows.push(Element::column(vec![
            gap(0.0, 12.0),
            horizontal(bar),
            gap(0.0, 16.0),
        ]));
    }
    Element::column(rows)
        .size(Some(window.width), Some(window.height))
        .background(look.canvas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};

    const LOOK: Look = Look::DARK;

    fn ada() -> Profile {
        Profile {
            identity: Default::default(),
            image: None,
            banner: None,
            banner_tint: None,
            name: "Ada".into(),
            handle: "ada".into(),
            accent: Color::rgb(90, 120, 200),
            status: Status::Online,
            presence: true,
            custom: Some("leyendo".into()),
            custom_emoji: None,
            pronouns: Some("ella".into()),
            about: Some("Escribo programas.".into()),
            activity: Some("Jugando a Factorio".into()),
            roles: vec![
                ("Moderación".into(), Color::rgb(200, 80, 80)),
                ("Equipo".into(), Color::rgb(80, 200, 80)),
            ],
            bot: false,
            message_action: Some("profile:dm:10".into()),
            message: Some(MessageField {
                id: format!("{}10", ids::MESSAGE),
                text: String::new(),
            }),
            restriction: None,
            member_since: Vec::new(),
            mutual: None,
            full_action: None,
            note: None,
            loading: false,
            relation: None,
            more: None,
            open_menu: None,
            notice: None,
            mutual_friends: Mutual::Unknown,
            mutual_servers: Mutual::Unknown,
            role_removals: vec![None, None],
            role_menu: None,
            edit_profile: false,
        }
    }

    #[test]
    fn a_card_too_tall_for_the_window_scrolls_its_body_and_keeps_its_buttons() {
        let tall = Profile {
            roles: (0..40)
                .map(|n| (format!("Rol número {n}"), Color::rgb(80, 80, 200)))
                .collect(),
            full_action: Some(format!("{}10", ids::FULL)),
            ..ada()
        };
        // A popover is never taller than its window: the card gets less.
        let (_, laid) = frame(card(&tall, &LOOK), Size::new(WIDTH, 420.0));
        let body = laid
            .regions
            .iter()
            .find(|region| {
                region.id == ids::card_body("ada") && region.kind == limn::HitKind::Scroll
            })
            .expect("the body scrolls")
            .rect;
        let button = laid
            .regions
            .iter()
            .find(|region| region.id == format!("{}10", ids::MESSAGE))
            .expect("the field to write stays")
            .rect;
        assert!(body.y + body.height <= button.y, "{body:?} {button:?}");
        assert!(button.y + button.height <= 420.0, "inside the window");
        // With room, the whole card shows and nothing scrolls away.
        let (_, roomy) = frame(card(&ada(), &LOOK), Size::new(WIDTH, 2000.0));
        assert!(controls(&roomy).contains(&format!("{}10", ids::MESSAGE)));
    }

    #[test]
    fn each_persons_card_opens_at_the_top_of_its_body() {
        let tall = |name: &str, handle: &str| Profile {
            name: name.into(),
            handle: handle.into(),
            roles: (0..40)
                .map(|n| (format!("Rol número {n}"), Color::rgb(80, 80, 200)))
                .collect(),
            ..ada()
        };
        let size = Size::new(WIDTH, 420.0);
        let (mut tree, laid) = frame(card(&tall("Ada", "ada"), &LOOK), size);
        let body = laid
            .regions
            .iter()
            .find(|region| region.id == ids::card_body("ada"))
            .unwrap()
            .rect;
        tree.handle_event(limn::UiEvent::ScrollDirect {
            position: limn::Point::new(body.x + 20.0, body.y + 20.0),
            delta_y: -160.0,
        });
        tree.layout(size);
        assert!(tree.scroll_offset(&ids::card_body("ada")) > 0.0);
        // Straight from Ada's row to Bruno's: his opens at its top.
        tree.set_root(card(&tall("Bruno", "bruno"), &LOOK));
        let laid = tree.layout(size);
        assert!(laid
            .regions
            .iter()
            .any(|region| region.id == ids::card_body("bruno")));
        assert_eq!(tree.scroll_offset(&ids::card_body("bruno")), 0.0);
    }

    #[test]
    fn the_note_is_an_icon_that_opens_the_full_profile() {
        let note = |text: &str| NoteField {
            id: "pp:note:10".into(),
            text: text.into(),
            loading: false,
            denied: None,
        };
        let someone = Profile {
            full_action: Some(format!("{}10", ids::FULL)),
            note: Some(note("")),
            ..ada()
        };
        let (_, laid) = frame(card(&someone, &LOOK), Size::new(WIDTH, 900.0));
        let shown = controls(&laid);
        assert!(
            shown.contains(&format!("{}10", ids::FULL_NOTE)),
            "{shown:?}"
        );
        assert!(
            !shown.contains(&"pp:note:10".to_owned()),
            "no field on the card"
        );
        // Without a full profile to open, the field stays.
        let alone = Profile {
            note: Some(note("amiga")),
            ..ada()
        };
        let (_, laid) = frame(card(&alone, &LOOK), Size::new(WIDTH, 900.0));
        assert!(controls(&laid).contains(&"pp:note:10".to_owned()));
    }

    #[test]
    fn a_long_bio_shows_three_lines_on_the_card_and_leads_to_the_full_profile() {
        let long = Profile {
            about: Some("uno\ndos\ntres\ncuatro\ncinco".into()),
            full_action: Some(format!("{}10", ids::FULL)),
            ..ada()
        };
        let (_, laid) = frame(card(&long, &LOOK), Size::new(WIDTH, 900.0));
        assert!(controls(&laid).contains(&format!("{}10", ids::FULL_BIO)));
        let at = |wanted: &str| {
            laid.commands.iter().find_map(|command| match command {
                limn::DrawCommand::Text { text, position, .. } if text == wanted => {
                    Some(position.y)
                }
                _ => None,
            })
        };
        let (first, link) = (at("uno").unwrap(), at("Ver biografía completa").unwrap());
        assert!(
            link - first <= BIO_HEIGHT + 6.0,
            "the rest is cut under three lines: {first} {link}"
        );
        // A short one shows whole, with nothing more to see.
        let short = Profile {
            full_action: long.full_action.clone(),
            ..ada()
        };
        let (_, laid) = frame(card(&short, &LOOK), Size::new(WIDTH, 900.0));
        assert!(!texts(&laid).contains(&"Ver biografía completa".to_owned()));
    }

    #[test]
    fn a_card_shows_who_someone_is_their_roles_and_a_way_to_write() {
        let (_, laid) = frame(
            Element::row(vec![
                Element::column(vec![card(&ada(), &LOOK)]).size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(800.0, 700.0),
        );
        let painted = texts(&laid);
        for expected in [
            "Ada",
            "ada",
            "ella",
            "leyendo",
            "Escribo programas.",
            "Moderación",
            "Equipo",
        ] {
            assert!(
                painted.iter().any(|text| text == expected),
                "{expected}: {painted:?}"
            );
        }
        // The card has no headings for its bio or its roles (REA §4.3, §4.4).
        for gone in ["Sobre mí", "Roles"] {
            assert!(
                !painted.iter().any(|text| text == gone),
                "{gone}: {painted:?}"
            );
        }
        assert!(controls(&laid).contains(&format!("{}10", ids::MESSAGE)));
    }

    #[test]
    fn the_footer_writes_to_someone_with_their_name_or_edits_the_own_profile() {
        let (_, laid) = frame(card(&ada(), &LOOK), Size::new(WIDTH, 900.0));
        assert!(controls(&laid).contains(&format!("{}10", ids::MESSAGE)));
        assert!(texts(&laid).contains(&"Enviar un mensaje a @Ada".to_owned()));
        assert!(!controls(&laid).contains(&ids::EDIT.to_owned()));
        // The account's own card edits its profile instead, full width.
        let own = Profile {
            message: None,
            edit_profile: true,
            full_action: None,
            ..ada()
        };
        let (_, laid) = frame(card(&own, &LOOK), Size::new(WIDTH, 900.0));
        assert!(controls(&laid).contains(&ids::EDIT.to_owned()));
        assert!(texts(&laid).contains(&"Editar perfil".to_owned()));
        // Neither where the account cannot write to them.
        let none = Profile {
            message: None,
            ..ada()
        };
        let (_, laid) = frame(card(&none, &LOOK), Size::new(WIDTH, 900.0));
        assert!(!controls(&laid)
            .iter()
            .any(|id| id.starts_with(ids::MESSAGE) || id == ids::EDIT));
    }

    #[test]
    fn a_card_shows_its_restriction_and_mutuals_but_not_the_dates() {
        let profile = Profile {
            restriction: Some("Aislado hasta hoy a las 18:30".into()),
            member_since: vec![Since {
                scope: "Cuenta".into(),
                face: None,
                date: "3 de mayo de 2019".into(),
            }],
            mutual_friends: Mutual::Known(vec![taller(), taller()]),
            mutual_servers: Mutual::Known(vec![taller()]),
            full_action: Some(format!("{}10", ids::FULL)),
            ..ada()
        };
        let (_, laid) = frame(
            Element::row(vec![
                Element::column(vec![card(&profile, &LOOK)]).size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(800.0, 900.0),
        );
        let painted = texts(&laid);
        for expected in [
            "Aislado hasta hoy a las 18:30",
            "2 amigos en común",
            "1 servidor en común",
        ] {
            assert!(
                painted.iter().any(|t| t == expected),
                "{expected}: {painted:?}"
            );
        }
        // The dates are the full profile's, not the card's.
        for gone in ["Miembro desde", "3 de mayo de 2019"] {
            assert!(!painted.iter().any(|t| t == gone), "{gone}: {painted:?}");
        }
        let shown = controls(&laid);
        assert!(
            shown.contains(&format!("{}10:friends", ids::MUTUAL)),
            "{shown:?}"
        );
        assert!(
            shown.contains(&format!("{}10:servers", ids::MUTUAL)),
            "{shown:?}"
        );
    }

    #[test]
    fn mutuals_read_12_px_beside_each_other_and_14_px_alone_and_nothing_unknown() {
        let size_of = |profile: &Profile, wanted: &str| {
            let (_, laid) = frame(card(profile, &LOOK), Size::new(WIDTH, 900.0));
            laid.commands.iter().find_map(|command| match command {
                limn::DrawCommand::Text {
                    text, font_size, ..
                } if text == wanted => Some(*font_size),
                _ => None,
            })
        };
        let both = Profile {
            mutual_friends: Mutual::Known(vec![taller(), taller()]),
            mutual_servers: Mutual::Known(vec![taller()]),
            full_action: Some(format!("{}10", ids::FULL)),
            ..ada()
        };
        assert_eq!(size_of(&both, "2 amigos en común"), Some(12.0));
        let friends = Profile {
            mutual_servers: Mutual::Known(Vec::new()),
            ..both.clone()
        };
        assert_eq!(size_of(&friends, "2 amigos en común"), Some(14.0));
        // Unknown or loading mutuals show nothing, and none is no mutual.
        for mutual in [Mutual::Unknown, Mutual::Loading, Mutual::Known(Vec::new())] {
            let quiet = Profile {
                mutual_friends: mutual.clone(),
                mutual_servers: mutual,
                ..both.clone()
            };
            let (_, laid) = frame(card(&quiet, &LOOK), Size::new(WIDTH, 900.0));
            assert!(!controls(&laid).iter().any(|id| id.starts_with(ids::MUTUAL)));
        }
    }

    #[test]
    fn a_card_head_has_the_friendship_and_more_buttons_as_their_states_allow() {
        let relation = |state, action: &str, menu: Vec<MenuEntry>| Relation {
            state,
            action: action.into(),
            menu,
        };
        let shown = |profile: &Profile| {
            let (_, laid) = frame(card(profile, &LOOK), Size::new(WIDTH, 900.0));
            controls(&laid)
        };
        // Someone without a friendship: the button sends the request.
        let stranger = Profile {
            relation: Some(relation(Friendship::None, "friend:10:add", Vec::new())),
            more: Some(Vec::new()),
            full_action: Some(format!("{}10", ids::FULL)),
            ..ada()
        };
        let buttons = shown(&stranger);
        assert!(buttons.contains(&"friend:10:add".to_owned()), "{buttons:?}");
        assert!(
            buttons.contains(&format!("{}10", ids::CARD_MORE)),
            "{buttons:?}"
        );
        // A friend's button opens the menu of the friendship, when it is open.
        let friend = Profile {
            relation: Some(relation(
                Friendship::Friend,
                &format!("{}10", ids::CARD_RELATION),
                vec![MenuEntry::Item(
                    "friend:10:remove".into(),
                    "Eliminar amigo".into(),
                    None,
                )],
            )),
            open_menu: Some(CardMenu::Relation),
            ..stranger.clone()
        };
        let (_, laid) = frame(card(&friend, &LOOK), Size::new(WIDTH, 900.0));
        assert!(controls(&laid).contains(&"friend:10:remove".to_owned()));
        assert!(texts(&laid).contains(&"Eliminar amigo".to_owned()));
        // Nothing of the buttons on one's own card, bots or blocked people.
        let own = Profile {
            relation: None,
            more: None,
            full_action: None,
            ..ada()
        };
        assert!(!shown(&own)
            .iter()
            .any(|id| id.starts_with(ids::CARD_RELATION) || id.starts_with(ids::CARD_MORE)));
    }

    #[test]
    fn a_card_name_shows_six_badges_and_counts_the_rest() {
        let badges = (0..8)
            .map(|n| crate::identity::BadgeChip {
                description: format!("Insignia {n}"),
                image: None,
                link: true,
            })
            .collect();
        let profile = Profile {
            identity: crate::identity::CardIdentity {
                badges,
                ..Default::default()
            },
            ..ada()
        };
        let (_, laid) = frame(card(&profile, &LOOK), Size::new(WIDTH, 900.0));
        let shown = controls(&laid);
        assert!(shown.contains(&"pp:badge=5".to_owned()), "{shown:?}");
        assert!(!shown.contains(&"pp:badge=6".to_owned()), "{shown:?}");
        assert!(texts(&laid).contains(&"+2".to_owned()));
    }

    #[test]
    fn an_ignored_person_is_noticed_with_a_way_to_lift_it_and_a_blocked_one_has_none() {
        let ignored = Profile {
            notice: Some(Notice::Ignored {
                unignore: "friend:10:unignore".into(),
            }),
            ..ada()
        };
        let (_, laid) = frame(card(&ignored, &LOOK), Size::new(WIDTH, 900.0));
        let painted = texts(&laid);
        assert!(painted.contains(&"Ignoraste a este usuario".to_owned()));
        assert!(painted.contains(&"Dejar de ignorar".to_owned()));
        assert!(controls(&laid).contains(&"friend:10:unignore".to_owned()));
        let blocked = Profile {
            notice: Some(Notice::Blocked),
            ..ada()
        };
        let (_, laid) = frame(card(&blocked, &LOOK), Size::new(WIDTH, 900.0));
        assert!(texts(&laid).contains(&"Bloqueaste a este usuario".to_owned()));
        assert!(!controls(&laid).iter().any(|id| id.starts_with("friend:")));
    }

    #[test]
    fn roles_on_a_card_come_with_a_remove_button_and_a_plus_for_the_checklist() {
        let action = "ss:member-of:1:10:role:7";
        let profile = Profile {
            role_removals: vec![Some(action.into()), None],
            role_menu: Some(vec![MenuEntry::Role {
                id: action.into(),
                name: "Moderación".into(),
                color: None,
                has: true,
            }]),
            full_action: Some(format!("{}10", ids::FULL)),
            ..ada()
        };
        let (_, laid) = frame(card(&profile, &LOOK), Size::new(WIDTH, 900.0));
        let shown = controls(&laid);
        assert!(shown.contains(&action.to_owned()), "{shown:?}");
        assert!(
            shown.contains(&format!("{}10", ids::CARD_ROLES)),
            "{shown:?}"
        );
        // Someone with no roles reads the plus as «Añadir roles».
        let none = Profile {
            roles: Vec::new(),
            role_removals: Vec::new(),
            ..profile.clone()
        };
        let (_, laid) = frame(card(&none, &LOOK), Size::new(WIDTH, 900.0));
        assert!(texts(&laid).contains(&"Añadir roles".to_owned()));
        // Without the account's permission, neither shows.
        let plain = Profile {
            role_removals: vec![None, None],
            role_menu: None,
            ..profile
        };
        let (_, laid) = frame(card(&plain, &LOOK), Size::new(WIDTH, 900.0));
        assert!(!controls(&laid)
            .iter()
            .any(|id| id.starts_with(ids::CARD_ROLES)));
        assert!(!controls(&laid).contains(&action.to_owned()));
    }

    fn taller() -> Face {
        Face {
            name: "Taller".into(),
            accent: Color::rgb(200, 120, 60),
            image: None,
        }
    }

    fn full_texts(servers: Mutual, friends: Mutual, tab: FullTab) -> Vec<String> {
        let full = FullProfile {
            connections: Default::default(),
            profile: ada(),
            server: Some("Taller".into()),
            nick: Some("Ada L.".into()),
            mutual_servers: servers,
            mutual_friends: friends,
            tab,
            loading: false,
        };
        texts(
            &frame(
                full_profile(Element::spacer(), &full, 900.0, &LOOK),
                Size::new(900.0, 900.0),
            )
            .1,
        )
    }

    #[test]
    fn the_full_profile_never_shows_unknown_mutuals_as_none() {
        let about = full_texts(Mutual::Unknown, Mutual::Loading, FullTab::About);
        for expected in [
            "Sobre mí",
            "Servidores en común",
            "Amigos en común",
            "En Taller",
            "Apodo: Ada L.",
            "Moderación",
        ] {
            assert!(about.iter().any(|t| t == expected), "{expected}: {about:?}");
        }
        let servers = full_texts(Mutual::Unknown, Mutual::Loading, FullTab::Servers);
        assert!(servers
            .iter()
            .any(|t| t == "No disponible en esta conexión."));
        assert!(!servers.iter().any(|t| t == "Ninguno en común."));
        assert!(
            !servers.iter().any(|t| t == "En Taller"),
            "one tab at a time"
        );
        let friends = full_texts(Mutual::Unknown, Mutual::Loading, FullTab::Friends);
        assert!(friends.iter().any(|t| t == "Cargando…"), "{friends:?}");
        // Known: the tabs count them, a known empty list says so.
        let friends = full_texts(
            Mutual::Known(vec![taller()]),
            Mutual::Known(Vec::new()),
            FullTab::Friends,
        );
        assert!(friends.iter().any(|t| t == "Servidores en común · 1"));
        assert!(friends.iter().any(|t| t == "Amigos en común · 0"));
        assert!(
            friends.iter().any(|t| t == "Ninguno en común."),
            "{friends:?}"
        );
        assert!(!friends
            .iter()
            .any(|t| t == "No disponible en esta conexión."));
        let servers = full_texts(
            Mutual::Known(vec![taller()]),
            Mutual::Known(Vec::new()),
            FullTab::Servers,
        );
        assert!(servers.iter().any(|t| t == "Taller"), "{servers:?}");
        assert!(servers.iter().any(|t| t == "T"), "its initial as its face");
    }

    #[test]
    fn the_full_profile_reports_its_tabs_and_offers_to_write() {
        let full = FullProfile {
            connections: Default::default(),
            profile: ada(),
            server: None,
            nick: None,
            mutual_servers: Mutual::Unknown,
            mutual_friends: Mutual::Unknown,
            tab: FullTab::About,
            loading: false,
        };
        let (_, laid) = frame(
            full_profile(Element::spacer(), &full, 900.0, &LOOK),
            Size::new(900.0, 900.0),
        );
        let ids = controls(&laid);
        for tab in FullTab::ALL {
            let id = format!("{}{}", ids::FULL_TAB, tab.key());
            assert!(ids.contains(&id), "{id}: {ids:?}");
            assert_eq!(FullTab::from_key(tab.key()), Some(tab));
        }
        assert!(ids.contains(&"profile:dm:10".to_owned()));
    }

    #[test]
    fn a_card_without_a_banner_paints_its_color_across_the_top() {
        // The banner's color is the profile's (or the mark's), else the raised ground.
        let painted = |tint: Option<Color>| {
            let profile = Profile {
                banner_tint: tint,
                ..ada()
            };
            let (_, laid) = frame(card(&profile, &LOOK), Size::new(WIDTH, 800.0));
            assert!(!laid
                .commands
                .iter()
                .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
            laid.commands.iter().any(|command| {
                matches!(
                    command,
                    limn::DrawCommand::FillRect { rect, color, .. }
                        if *color == tint.unwrap_or(LOOK.raised)
                            && (rect.width - WIDTH).abs() < 1.0
                            && rect.height == BANNER
                )
            })
        };
        let blue = Color::rgb(90, 120, 200);
        assert!(painted(Some(blue)));
        assert!(painted(None));
    }

    #[test]
    fn many_roles_wrap_onto_more_lines_as_chips() {
        let roles: Vec<_> = (0..8)
            .map(|index| (format!("Rol número {index}"), Color::rgb(200, 80, 80)))
            .collect();
        let profile = Profile {
            roles: roles.clone(),
            ..ada()
        };
        let (_, laid) = frame(card(&profile, &LOOK), Size::new(WIDTH, 1200.0));
        let top = |name: &str| {
            laid.commands
                .iter()
                .find_map(|command| match command {
                    limn::DrawCommand::Text { text, position, .. } if text == name => {
                        Some(*position)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name} is painted"))
        };
        let (first, last) = (top("Rol número 0"), top("Rol número 7"));
        assert!(last.y > first.y, "{first:?} {last:?}");
        for (name, _) in &roles {
            let right = top(name).x + name.chars().count() as f32 * 7.0;
            assert!(right <= WIDTH, "{name} stays inside the card: {right}");
        }
    }

    #[test]
    fn profile_banner_covers_the_card_without_changing_its_height() {
        let image = limn::RasterImage::new(
            "b".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "Banner de prueba",
        )
        .unwrap();
        for width in [272.0, WIDTH] {
            let profile = ada();
            let (_, before) = frame(card(&profile, &LOOK), Size::new(width, 800.0));
            let with_image = Profile {
                banner: Some(image.clone()),
                ..profile
            };
            let (_, loaded) = frame(card(&with_image, &LOOK), Size::new(width, 800.0));
            let rects: Vec<_> = loaded
                .commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::RasterImage {
                        image: actual,
                        rect,
                        ..
                    } if actual == &image.clone().cover() => Some(*rect),
                    _ => None,
                })
                .collect();
            assert_eq!(rects.len(), 1);
            assert!((rects[0].width - width).abs() < 1.0, "{:?}", rects[0]);
            assert_eq!(rects[0].height, BANNER);
            assert_eq!(
                before.regions, loaded.regions,
                "loading artwork must not move actions"
            );
        }
    }

    #[test]
    fn the_own_popout_offers_the_custom_status_editing_and_the_id() {
        let popout = OwnPopout {
            profile: Profile {
                image: None,
                custom: None,
                status: Status::Invisible,
                ..ada()
            },
            until: Some("hasta las 10:00".into()),
            custom_denied: None,
            edit_denied: None,
            status_open: true,
            copy_id: true,
            switch_account: true,
        };
        let (_, laid) = frame(
            Element::row(vec![
                Element::column(vec![own_popout(&popout, &LOOK)]).size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(800.0, 700.0),
        );
        let ids = controls(&laid);
        for expected in [
            ids::CUSTOM,
            ids::EDIT,
            ids::COPY_ID,
            ids::STATUS_MENU,
            ids::SWITCH_ACCOUNT,
            "pp:status=dnd",
        ] {
            assert!(ids.contains(&expected.to_owned()), "{expected}: {ids:?}");
        }
        let painted = texts(&laid);
        // At 300 px its words wrap: read them as one line.
        let words = painted.join(" ");
        let words: Vec<&str> = words.split_whitespace().collect();
        assert!(
            words.join(" ").contains("Establecer estado"),
            "the bubble invites to set one: {painted:?}"
        );
        assert!(painted.contains(&"Invisible".to_owned()));
        assert!(painted.contains(&"hasta las 10:00".to_owned()));
    }

    #[test]
    fn a_custom_status_reads_as_its_emoji_and_text_and_has_a_limit() {
        assert_eq!(
            custom_line(Some("🌙"), "  durmiendo "),
            Some("🌙 durmiendo".into())
        );
        assert_eq!(custom_line(None, "   "), None);
        assert!(custom_error(Some(&"x".repeat(CUSTOM_MAX))).is_none());
        assert!(custom_error(Some(&"x".repeat(CUSTOM_MAX + 1))).is_some());
        assert_eq!(ClearAfter::from_key("4h"), Some(ClearAfter::FourHours));
        assert!(ClearAfter::Never.label(Timestamp(0)) == "No eliminar");
    }

    #[test]
    fn the_editor_counts_what_is_written_about_oneself_and_says_when_it_is_too_long() {
        let view = Editor {
            preview: ada(),
            name: "Ada".into(),
            pronouns: "ella".into(),
            about: "x".repeat(ABOUT_MAX + 1),
            accent: Rgb(0x34_98_DB),
            hex: None,
            failure: None,
            denied: None,
            servers: Vec::new(),
            servers_open: false,
            bar: Some(UnsavedBar::default()),
            pictures: EditorPictures::default(),
            scope: None,
            credentials: Some("Pide tu contraseña.".into()),
            picked: None,
        };
        let (_, laid) = frame(
            editor(&view, Size::new(1280.0, 800.0), &LOOK),
            Size::new(1280.0, 800.0),
        );
        let painted = texts(&laid);
        assert!(painted.contains(&format!("{}/{ABOUT_MAX}", ABOUT_MAX + 1)));
        assert!(painted
            .iter()
            .any(|text| text.contains("«Sobre mí» puede tener hasta 190")));
        let ids = controls(&laid);
        for expected in [
            ids::NAME,
            ids::PRONOUNS,
            ids::ABOUT,
            ids::SAVE,
            ids::EDITOR_CLOSE,
        ] {
            assert!(ids.contains(&expected.to_owned()), "{expected}: {ids:?}");
        }
    }

    fn editing(bar: Option<UnsavedBar>) -> Editor {
        Editor {
            preview: Profile {
                name: "Ada Nueva".into(),
                pronouns: Some("elle".into()),
                about: Some("Ahora en Rust.".into()),
                roles: Vec::new(),
                ..ada()
            },
            name: "Ada Nueva".into(),
            pronouns: "elle".into(),
            about: "Ahora en Rust.".into(),
            accent: Rgb(0x34_98_DB),
            hex: None,
            failure: None,
            denied: None,
            servers: Vec::new(),
            servers_open: false,
            bar,
            pictures: EditorPictures::default(),
            scope: None,
            credentials: None,
            picked: None,
        }
    }

    #[test]
    fn each_server_of_the_profile_choice_shows_its_icon_or_its_initial() {
        let server = |id: u64, name: &str, image: Option<limn::RasterImage>| EditorServer {
            action: format!("{}{id}", ids::SCOPE),
            name: name.into(),
            nick: None,
            image,
            initials: name.chars().take(2).collect(),
            accent: Color::rgb(200, 40, 40),
        };
        let mut view = editing(None);
        view.servers = vec![
            server(1, "Taller", Some(chosen())),
            server(2, "Laboratorio", None),
        ];
        view.servers_open = true;
        let window = Size::new(1280.0, 800.0);
        let (_, laid) = frame(editor(&view, window, &LOOK), window);
        let icon = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, image, .. }
                    if *image == chosen().cover() =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("the server's own icon");
        assert_eq!(
            (icon.width, icon.height),
            (limn::ICON_SIZE, limn::ICON_SIZE)
        );
        assert!(
            texts(&laid).contains(&"L".to_owned()),
            "its initial without one"
        );
        assert!(
            !laid.commands.iter().any(|command| matches!(
                command,
                limn::DrawCommand::Icon {
                    icon: Icon::Server,
                    ..
                }
            )),
            "no generic server glyph"
        );
    }

    #[test]
    fn the_editor_offers_saving_only_with_changes_and_its_preview_shows_them() {
        for (window, wide) in [
            (Size::new(1280.0, 800.0), true),
            (Size::new(400.0, 800.0), false),
        ] {
            let (_, laid) = frame(editor(&editing(None), window, &LOOK), window);
            let ids = controls(&laid);
            assert!(!ids.contains(&ids::SAVE.to_owned()) && !ids.contains(&ids::RESET.to_owned()));
            let painted = texts(&laid);
            // The preview is the card itself, with what is being written;
            // a narrow window stacks it under the form, out of view.
            let preview: &[&str] = if wide {
                &["Vista previa", "ada", "elle", "Ahora en Rust."]
            } else {
                &[]
            };
            for expected in preview {
                assert!(
                    painted.iter().any(|t| t == expected),
                    "{expected}: {painted:?}"
                );
            }

            let (_, laid) = frame(
                editor(&editing(Some(UnsavedBar::default())), window, &LOOK),
                window,
            );
            let ids = controls(&laid);
            assert!(ids.contains(&ids::SAVE.to_owned()) && ids.contains(&ids::RESET.to_owned()));
            let painted = texts(&laid);
            let message = if wide {
                "¡Cuidado! Tienes cambios sin guardar"
            } else {
                "Cambios sin guardar"
            };
            assert!(
                painted.iter().any(|t| t == message),
                "{message}: {painted:?}"
            );
        }
    }

    // Profile editing (edicion-de-perfil).
    fn chosen() -> limn::RasterImage {
        limn::RasterImage::new(
            "e".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "Imagen elegida",
        )
        .unwrap()
    }

    #[test]
    fn a_chosen_picture_is_framed_over_its_whole_and_says_whether_it_moves() {
        let framing = EditorFraming {
            image: chosen(),
            aspect: 2.0,
            frame: [0.5, 0.0, 0.5, 1.0],
            x: 0.75,
            y: 0.5,
            zoom: 0.0,
            round: true,
            note: Some("Se usa sólo el primer cuadro.".into()),
        };
        let (_, laid) = frame(
            Element::column(vec![editor_framing(&framing, 300.0, &LOOK)]),
            Size::new(320.0, 600.0),
        );
        let picture = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert_eq!((picture.width, picture.height), (300.0, 150.0));
        // The frame is drawn over the part kept: the right half, round.
        let outline = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::StrokeRect {
                    rect,
                    color,
                    radius,
                    ..
                } if *color == LOOK.accent => Some((*rect, *radius)),
                _ => None,
            })
            .unwrap();
        let (rect, radius) = outline;
        assert_eq!(
            (
                rect.x - picture.x,
                rect.y - picture.y,
                rect.width,
                rect.height
            ),
            (150.0, 0.0, 150.0, 150.0)
        );
        assert_eq!(radius, 75.0);
        for id in [ids::FRAME, ids::FRAME_ZOOM] {
            assert!(laid.regions.iter().any(|region| region.id == id), "{id}");
        }
        assert!(texts(&laid)
            .iter()
            .any(|text| text.contains("primer cuadro")));
    }

    #[test]
    fn a_banner_the_plan_does_not_allow_is_not_chosen_but_may_be_taken_away() {
        let mut view = editing(None);
        view.pictures = EditorPictures {
            has_banner: true,
            banner_denied: Some("Necesitas Nitro para usar un banner".into()),
            ..EditorPictures::default()
        };
        let window = Size::new(1280.0, 2400.0);
        let (_, laid) = frame(editor(&view, window, &LOOK), window);
        let ids = controls(&laid);
        assert!(ids.contains(&ids::AVATAR_PICK.to_owned()), "{ids:?}");
        assert!(ids.contains(&ids::BANNER_REMOVE.to_owned()));
        assert!(!ids.contains(&ids::BANNER_PICK.to_owned()));
        assert!(texts(&laid)
            .iter()
            .any(|text| text == "Necesitas Nitro para usar un banner"));
    }

    #[test]
    fn the_card_being_read_is_a_skeleton_in_the_shapes_of_its_parts() {
        let (_, laid) = frame(
            card_skeleton("profile:loading:ada", &LOOK),
            Size::new(WIDTH, SKELETON_HEIGHT),
        );
        let fills: Vec<limn::Rect> = laid
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::FillRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        let shown = |x: f32, y: f32, width: f32, height: f32| {
            fills
                .iter()
                .any(|rect| (rect.x, rect.y, rect.width, rect.height) == (x, y, width, height))
        };
        // The banner; the avatar's 80 px box at (16, 65), its ring, and the pulse inside.
        assert!(shown(0.0, 0.0, WIDTH, BANNER), "{fills:?}");
        assert!(shown(INSET, 65.0, AVATAR, AVATAR), "{fills:?}");
        assert!(shown(22.0, 71.0, 68.0, 68.0), "{fills:?}");
        for (x, y, width, height) in [
            (16.0, 157.0, 147.0, 18.0),
            (16.0, 183.0, 94.0, 12.0),
            (16.0, 211.0, 52.0, 16.0),
            (84.0, 211.0, 72.0, 16.0),
            (172.0, 211.0, 48.0, 16.0),
            (0.0, 239.0, WIDTH, 1.0),
            (16.0, 256.0, 241.0, 12.0),
            (16.0, 278.0, 201.0, 12.0),
            (16.0, 300.0, 228.0, 12.0),
            (16.0, 322.0, 161.0, 12.0),
        ] {
            assert!(
                shown(x, y, width, height),
                "{x},{y} {width}x{height}: {fills:?}"
            );
        }
        // Faint text, not limn's opaque raised: it stays visible at the bottom of the pulse.
        let banner = laid.commands.iter().find_map(|command| match command {
            limn::DrawCommand::FillRect { rect, color, .. } if rect.height == BANNER => {
                Some(*color)
            }
            _ => None,
        });
        assert_eq!(banner, Some(Look::alpha(LOOK.text, 16)));
    }

    #[test]
    fn a_card_whose_profile_is_read_is_its_skeleton_until_it_arrives() {
        let reading = Profile {
            loading: true,
            ..ada()
        };
        let (_, laid) = frame(card(&reading, &LOOK), Size::new(WIDTH, SKELETON_HEIGHT));
        assert!(laid
            .access
            .iter()
            .any(|node| node.name == "Cargando perfil"));
        assert!(!texts(&laid).iter().any(|text| text == "Ada"));
        assert!(!controls(&laid).contains(&"profile:dm:10".to_owned()));
        let (_, loaded) = frame(card(&ada(), &LOOK), Size::new(WIDTH, 900.0));
        assert!(!loaded
            .access
            .iter()
            .any(|node| node.name == "Cargando perfil"));
        assert!(controls(&loaded).contains(&format!("{}10", ids::MESSAGE)));
    }

    #[test]
    fn the_about_tab_of_a_profile_being_read_shows_lines_not_no_data() {
        // Nothing of them is known: no about, roles, activity or pronouns.
        let bare = Profile {
            about: None,
            pronouns: None,
            activity: None,
            roles: Vec::new(),
            custom: None,
            ..ada()
        };
        let full = |loading: bool| FullProfile {
            connections: Default::default(),
            profile: Profile {
                loading,
                ..bare.clone()
            },
            server: None,
            nick: None,
            mutual_servers: Mutual::Loading,
            mutual_friends: Mutual::Loading,
            tab: FullTab::About,
            loading,
        };
        let (_, reading) = frame(
            full_profile(Element::spacer(), &full(true), 900.0, &LOOK),
            Size::new(900.0, 900.0),
        );
        assert!(!texts(&reading)
            .iter()
            .any(|text| text == "No hay más datos de este perfil."));
        let widths: Vec<f32> = reading
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::FillRect { rect, .. } if rect.height == 12.0 => Some(rect.width),
                _ => None,
            })
            .collect();
        for width in SKELETON_LINES {
            assert!(widths.contains(&width), "{widths:?}");
        }
        // Once the read is over with nothing to show, it says so.
        let (_, settled) = frame(
            full_profile(Element::spacer(), &full(false), 900.0, &LOOK),
            Size::new(900.0, 900.0),
        );
        assert!(texts(&settled)
            .iter()
            .any(|text| text == "No hay más datos de este perfil."));
    }
}
