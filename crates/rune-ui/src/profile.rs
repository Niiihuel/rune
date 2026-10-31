//! The account's profile and status, and the cards of people: the popout
//! that opens from the account's panel (its status menu, how long a status
//! lasts, its custom status), the profile
//! editor over the whole window, and the cards of a member or of a
//! message's author. Its actions start with `pp:` or `profile:`.
//!
//! A status chosen for a while is a timer of this client: once it is due,
//! [`tick`] sets back the status it replaced. A custom status set to clear
//! itself works the same way. Both timers are kept between runs. Nothing of
//! the profile or the status changes before the provider says so.

use std::time::{Duration, Instant};

use limn::{Element, Key, UiAction, UiEvent};
use rune_components::chrome::{self, UserPanel};
use rune_components::look::activity_line;
use rune_components::profile::{
    self as parts, ids, CardMenu, ClearAfter, CustomStatusDialog, EditorErrors, EditorPictures,
    EditorScope, Face, Friendship, FullProfile, FullTab, Mutual, OwnPopout, Profile, Relation,
    Since,
};
use rune_components::server_settings::{hex, parse_hex, UnsavedBar};
use rune_components::time::date_label;
use rune_components::voice::MenuEntry;
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{
    MessageId, RelationshipKind, Restriction, Rgb, ServerId, Status, Timestamp, User, UserId,
    UserProfile,
};
use rune_provider::{
    Command, Created, Failure, ProfileEdit, ProfileImage, ProfileImageKind, ServerProfileEdit,
    VoiceSupport,
};
use rune_state::{Applied, Store};

use crate::connection;
use crate::server_menu::wall_now;
use crate::{settings, Intent, SettingsPage, Ui, ViewContext};

/// The settings that keep the timers and the chosen output between runs.
pub(crate) const STATUS_TIMER: &str = "status_timer";
pub(crate) const CUSTOM_UNTIL: &str = "custom_status_until";
pub(crate) const SPEAKER: &str = "speaker";
/// A card whose profile is still being read shows nothing for this long
/// after it opens, then its skeleton: a quick answer never flashes one
/// (REA §5d, Discord's 250 ms).
const SKELETON_AFTER_MS: i64 = 250;

/// What is open of the profiles, and the account's status timers.
#[derive(Debug, Default)]
pub struct Profiles {
    /// The card of a message's author: the message clicked, and whether on
    /// the author's mark (else on their name).
    pub author: Option<(MessageId, bool)>,
    /// The menu of statuses is open beside the popout.
    pub status_open: bool,
    /// The quick voice panel of an arrow, while open.
    pub devices: Option<crate::quick_voice::QuickVoice>,
    /// The custom status dialog, while open.
    pub custom: Option<CustomDraft>,
    /// The profile editor, while open.
    pub editor: Option<Editor>,
    /// A status chosen for a while.
    pub timed: Option<Timed>,
    /// When the custom status clears itself.
    pub custom_until: Option<Timestamp>,
    /// Someone's full profile, while open, and the server it is seen in.
    pub full: Option<(UserId, Option<ServerId>)>,
    /// The tab of the full profile shown.
    pub full_tab: FullTab,
    /// Where the member list's card opened, taken from its row: it stays
    /// there while the list moves or rebuilds under it, as the official
    /// client's popout does, instead of following a row that a busy
    /// server's list keeps moving (2026-10-17).
    pub member_at: Option<(UserId, limn::Point)>,
    /// The same for the card of a message's author.
    pub author_at: Option<((MessageId, bool), limn::Point)>,
    /// The menu open on a card's banner or roles, and whose card it is.
    pub menu: Option<(UserId, CardMenu)>,
    /// The text written in a card's field, and whose card holds it.
    pub message: Option<(UserId, String)>,
    /// The person whose conversation is being opened to receive the field's
    /// text: it goes once the conversation exists.
    pub message_waiting: Option<UserId>,
    /// The card shown and since when, by the store's clock: the time of
    /// its effect's layers.
    pub effect_since: Option<(UserId, Option<ServerId>, Timestamp)>,
    /// Whose presence was asked to be kept current, and where.
    pub people_watch: Option<(ServerId, Vec<UserId>)>,
    // Profile card and cosmetics (perfil-tarjeta-y-cosmeticos).
    /// Whose decoration and nameplate play because the pointer is over
    /// their row, message or tile, as of the last layout.
    pub hovered: Option<UserId>,
    /// While a member's or an author's card is open: the list or the
    /// conversation it stands against, and the card. The person scrolling
    /// the first, outside the card, closes it.
    pub scroll_close: Option<(limn::Rect, limn::Rect)>,
}

/// A status chosen until `until`, and the one to set back then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timed {
    pub status: Status,
    pub until: Timestamp,
    pub then: Status,
}

/// What the custom status dialog holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomDraft {
    pub emoji: Option<String>,
    pub text: String,
    pub clear_after: ClearAfter,
    pub emoji_open: bool,
    pub expiry_open: bool,
}

/// The profile editor while open. The draft keeps only what was touched
/// here; what matches the account is not a change.
#[derive(Debug, Default)]
pub struct Editor {
    pub edit: ProfileEdit,
    /// The server whose profile is edited; `None` for the one shown everywhere.
    pub server: Option<ServerId>,
    /// The draft of that server's profile.
    pub server_edit: ServerProfileEdit,
    /// What the pictures chosen here look like, until saved.
    pub avatar_preview: Option<limn::RasterImage>,
    pub banner_preview: Option<limn::RasterImage>,
    /// The picture being chosen or framed, by its request, and whether it
    /// was asked to keep moving (edicion-de-perfil).
    pub picking: Option<(u64, ProfileImageKind, bool)>,
    /// The custom color field, while open.
    pub hex: Option<String>,
    pub servers_open: bool,
    /// The change left and its answer has not come yet.
    pub saving: bool,
    /// Someone tried to leave with changes: the bar insists.
    pub nudged: bool,
    /// Why the provider refused the last change.
    pub failure: Option<String>,
    /// The settings page it was opened over, to go back to.
    back_to: Option<SettingsPage>,
    // Profile editing (edicion-de-perfil).
    /// The picture last chosen, whole, to frame it while it is the draft's.
    pub framing: Option<crate::framing::Framing>,
}

impl Editor {
    /// Whether anything shown differs from the account.
    pub(crate) fn dirty(&self, store: &Store) -> bool {
        match self.server {
            Some(server) => {
                server_changes(&self.server_edit, store, server) != ServerProfileEdit::default()
            }
            None => store
                .me_user()
                .is_some_and(|user| changes(&self.edit, user) != ProfileEdit::default()),
        }
    }

    /// The same editor on the profile of `server`, or the main one, with
    /// nothing changed yet.
    fn on(&self, server: Option<ServerId>) -> Self {
        Self {
            server,
            back_to: self.back_to,
            ..Self::default()
        }
    }
}

/// `edit` without what the account's profile in `server` already is.
fn server_changes(edit: &ServerProfileEdit, store: &Store, server: ServerId) -> ServerProfileEdit {
    let me = store.me();
    let member = me.and_then(|me| store.member_of(server, me));
    let loaded = me.and_then(|me| store.profile(me, Some(server)));
    ServerProfileEdit {
        nick: edit
            .nick
            .as_ref()
            .map(clean)
            .filter(|nick| *nick != member.and_then(|member| member.nick.clone())),
        about: edit
            .about
            .as_ref()
            .map(clean)
            .filter(|about| *about != loaded.and_then(|loaded| loaded.server_about.clone())),
        avatar: edit.avatar.clone().filter(|avatar| {
            avatar.is_some() || member.is_some_and(|member| member.avatar.is_some())
        }),
        banner: edit.banner.clone().filter(|banner| {
            banner.is_some() || loaded.is_some_and(|loaded| loaded.server_banner.image().is_some())
        }),
    }
}

/// What a server profile's fields hold: the name there and «Sobre mí» there.
fn server_fields(edit: &ServerProfileEdit, store: &Store, server: ServerId) -> (String, String) {
    let me = store.me();
    let pick = |edited: &Option<Option<String>>, current: Option<String>| {
        edited.clone().unwrap_or(current).unwrap_or_default()
    };
    (
        pick(
            &edit.nick,
            me.and_then(|me| store.member_of(server, me)?.nick.clone()),
        ),
        pick(
            &edit.about,
            me.and_then(|me| store.profile(me, Some(server))?.server_about.clone()),
        ),
    )
}

/// Put the account's profile in `server` in the editor's fields.
fn fill_server_fields(store: &Store, server: ServerId) -> Vec<Intent> {
    let (nick, about) = server_fields(&ServerProfileEdit::default(), store, server);
    vec![set_input(ids::NICK, nick), set_input(ids::ABOUT, about)]
}

/// Some text, or nothing when only spaces are left.
fn clean(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// `edit` without what `user` already is.
fn changes(edit: &ProfileEdit, user: &User) -> ProfileEdit {
    ProfileEdit {
        display_name: edit
            .display_name
            .as_ref()
            .map(clean)
            .filter(|name| *name != user.display_name),
        pronouns: edit
            .pronouns
            .as_ref()
            .map(clean)
            .filter(|pronouns| *pronouns != user.pronouns),
        about: edit
            .about
            .as_ref()
            .map(clean)
            .filter(|about| *about != user.about),
        accent: edit.accent.filter(|accent| *accent != user.accent),
        avatar: edit
            .avatar
            .clone()
            .filter(|avatar| avatar.is_some() || user.avatar.is_some()),
        banner: edit
            .banner
            .clone()
            .filter(|banner| banner.is_some() || user.banner.image().is_some()),
    }
}

/// `user` as the editor shows it: with what was changed here.
fn shown(user: &User, edit: &ProfileEdit) -> User {
    let changed = changes(edit, user);
    User {
        display_name: changed
            .display_name
            .unwrap_or_else(|| user.display_name.clone()),
        pronouns: changed.pronouns.unwrap_or_else(|| user.pronouns.clone()),
        about: changed.about.unwrap_or_else(|| user.about.clone()),
        accent: changed.accent.unwrap_or(user.accent),
        ..user.clone()
    }
}

/// What the editor's fields hold: the name, the pronouns and «Sobre mí».
fn fields(edit: &ProfileEdit, user: &User) -> (String, String, String) {
    let pick = |edited: &Option<Option<String>>, current: &Option<String>| {
        edited
            .clone()
            .unwrap_or_else(|| current.clone())
            .unwrap_or_default()
    };
    (
        pick(&edit.display_name, &user.display_name),
        pick(&edit.pronouns, &user.pronouns),
        pick(&edit.about, &user.about),
    )
}

fn set_input(id: &str, value: impl Into<String>) -> Intent {
    Intent::SetInput {
        id: id.to_owned(),
        value: value.into(),
    }
}

/// Put the account's values in the editor's fields.
fn fill_fields(user: &User) -> Vec<Intent> {
    let (name, pronouns, about) = fields(&ProfileEdit::default(), user);
    vec![
        set_input(ids::NAME, name),
        set_input(ids::PRONOUNS, pronouns),
        set_input(ids::ABOUT, about),
    ]
}

/// Why the profile cannot be changed, when the provider cannot.
fn edit_denied(store: &Store) -> Option<String> {
    (!store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.edit_profile))
    .then(|| t("Este proveedor no permite cambiar el perfil.").into())
}

/// Why the custom status cannot be set, when the provider cannot.
fn custom_denied(store: &Store) -> Option<String> {
    (!store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.custom_status))
    .then(|| t("Este proveedor no permite un estado personalizado.").into())
}

/// The card of `user`, as seen in `server` (or outside any).
pub fn card_of(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Option<Profile> {
    let account = store.user(user)?;
    let presence = store.presence(user);
    let held: Vec<&rune_model::Role> = server
        .and_then(|server| {
            let member = store.member_of(server, user)?;
            let mut roles: Vec<_> = store
                .server(server)?
                .roles
                .iter()
                .filter(|role| member.roles.contains(&role.id))
                .collect();
            roles.sort_by_key(|role| std::cmp::Reverse(role.position));
            Some(roles)
        })
        .unwrap_or_default();
    let roles = held
        .iter()
        .map(|role| {
            (
                role.name.clone(),
                role.color.map_or(look.muted, |color| look.ink(color)),
            )
        })
        .collect();
    let wall = store.wall();
    let loaded = store.profile(user, server);
    let (mutual_servers, mutual_friends) = if store.me() == Some(user) {
        (Mutual::Unknown, Mutual::Unknown)
    } else {
        mutual_lists(ui, store, server, user, look)
    };
    let (role_removals, role_menu) = role_controls(store, server, user, &held);
    let mut member_since = Vec::new();
    if let Some(created) = loaded.and_then(|profile| profile.created) {
        member_since.push(Since {
            scope: t("Cuenta").into(),
            face: None,
            date: date_label(created),
        });
    }
    if let Some((place, joined)) = server.and_then(|server| {
        Some((
            store.server(server)?,
            store
                .member_of(server, user)
                .and_then(|member| member.joined)
                .or(loaded?.joined)?,
        ))
    }) {
        member_since.push(Since {
            scope: place.name.clone(),
            face: Some(server_face(ui, place, look)),
            date: date_label(joined),
        });
    }
    Some(Profile {
        restriction: server
            .and_then(|server| store.restriction_of(server, user))
            .map(|restriction| restriction_label(restriction, wall)),
        member_since,
        mutual: loaded.and_then(mutual_summary),
        full_action: (store.me() != Some(user)).then(|| format!("{}{user}", ids::FULL)),
        note: crate::account::note_field(ui, store, user),
        image: ui.picture(store.avatar_in(server, user)),
        banner: banner(account, loaded)
            .and_then(|source| ui.images.get(source))
            .cloned(),
        // Without a picture the banner takes the profile's accent, else the
        // avatar's dominant color, as the official popout does; a letter
        // avatar's color is the account's own.
        banner_tint: loaded
            .and_then(|loaded| loaded.accent)
            .or_else(|| ui.picture_color(store.avatar_in(server, user)))
            .or(Some(account.accent))
            .map(|color| look.ink(color)),
        // The nickname the profile reported, while the membership is not known here.
        name: loaded
            .and_then(|loaded| loaded.nick.clone())
            .filter(|_| {
                server
                    .and_then(|server| store.member_of(server, user))
                    .is_none()
            })
            .unwrap_or_else(|| store.display_name(user, server)),
        handle: account.name.clone(),
        accent: look.ink(
            loaded
                .and_then(|loaded| loaded.accent)
                .unwrap_or(account.accent),
        ),
        status: presence.status,
        presence: has_presence(store),
        custom: presence.custom.clone(),
        custom_emoji: presence
            .custom_emoji
            .as_ref()
            .map(|emoji| crate::identity::text_picture(ui, emoji)),
        // What they set for this server first, then their profile, then what came with the account.
        pronouns: loaded
            .and_then(|loaded| loaded.server_pronouns.clone().or(loaded.pronouns.clone()))
            .or(account.pronouns.clone()),
        about: loaded
            .and_then(|loaded| loaded.server_about.clone().or(loaded.about.clone()))
            .or(account.about.clone()),
        message_action: store
            .open_direct(user)
            .allowed()
            .then(|| format!("profile:dm:{user}")),
        // An ignored person's activity stays hidden (2026-10-31).
        activity: presence
            .activity()
            .filter(|_| !store.is_ignored(user))
            .map(activity_line),
        roles,
        bot: account.bot,

        identity: crate::identity::card(ui, store, server, user, &held, look),
        loading: loaded.is_none() && store.profile_loading_in(user, server),
        relation: relation(store, user),
        more: (store.me() != Some(user)).then(|| more_entries(ui, store, user, server)),
        open_menu: ui
            .profiles
            .menu
            .filter(|(open, _)| *open == user)
            .map(|(_, menu)| menu),
        notice: notice(store, user),
        mutual_servers,
        mutual_friends,
        role_removals,
        role_menu,
        message: message_field(ui, store, user),
        edit_profile: store.me() == Some(user) && edit_denied(store).is_none(),
    })
}

/// The field under `user`'s card, when the account may write to them: its
/// action, and the text written so far.
fn message_field(ui: &Ui, store: &Store, user: UserId) -> Option<parts::MessageField> {
    (store.me() != Some(user) && store.open_direct(user).allowed()).then(|| parts::MessageField {
        id: format!("{}{user}", ids::MESSAGE),
        text: ui
            .profiles
            .message
            .as_ref()
            .filter(|(who, _)| *who == user)
            .map(|(_, text)| text.clone())
            .unwrap_or_default(),
    })
}

/// What the account does with `user` that a card says under their name: a
/// block, or an ignore that can be lifted from the card.
fn notice(store: &Store, user: UserId) -> Option<rune_components::profile::Notice> {
    if store.blocked(user) {
        Some(rune_components::profile::Notice::Blocked)
    } else if store.is_ignored(user) {
        Some(rune_components::profile::Notice::Ignored {
            unignore: format!("friend:{}:unignore", user.0),
        })
    } else {
        None
    }
}

/// Who `user` shares with the account in `server`: the servers and the
/// friends, each known, loading, or not reported. The card and the full
/// profile show the same lists.
fn mutual_lists(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> (Mutual, Mutual) {
    let loaded = store.profile(user, server);
    let list = |faces: Option<Vec<Face>>| match faces {
        Some(faces) => Mutual::Known(faces),
        None if loaded.is_none() && store.profile_loading(user) => Mutual::Loading,
        None => Mutual::Unknown,
    };
    let servers = list(loaded.and_then(|loaded| {
        let ids = loaded.mutual_servers.as_ref()?;
        Some(
            ids.iter()
                .filter_map(|id| Some(server_face(ui, store.server(*id)?, look)))
                .collect(),
        )
    }));
    let friends = list(loaded.and_then(|loaded| {
        let ids = loaded.mutual_friends.as_ref()?;
        Some(
            ids.iter()
                .map(|id| {
                    let account = store.user(*id);
                    Face {
                        name: store.display_name(*id, None),
                        accent: account.map_or(look.faint, |account| look.ink(account.accent)),
                        image: account.and_then(|account| ui.picture(account.picture())),
                    }
                })
                .collect(),
        )
    }));
    (servers, friends)
}

/// The roles a card may give and take in `server`: the checklist the person
/// menu's «Roles» has, and for each role the card holds the action that
/// takes it away, when the checklist lists it. Nothing for the account's own
/// card, whose roles are not changed from it.
fn role_controls(
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    held: &[&rune_model::Role],
) -> (Vec<Option<String>>, Option<Vec<MenuEntry>>) {
    let checklist = server
        .filter(|_| store.me() != Some(user))
        .and_then(|server| crate::server_settings::member_menu_group(store, server, user).roles)
        .and_then(|entry| match entry {
            MenuEntry::Submenu(_, entries) => Some(entries),
            _ => None,
        });
    let removals = held
        .iter()
        .map(|role| {
            let (server, entries) = (server?, checklist.as_ref()?);
            let action = format!("ss:member-of:{server}:{user}:role:{}", role.id);
            entries
                .iter()
                .any(|entry| matches!(entry, MenuEntry::Role { id, .. } if *id == action))
                .then_some(action)
        })
        .collect();
    (removals, checklist)
}

/// The friendship's button of `user`'s card, and the menu it opens: none for
/// the account, bots, blocked people, and a provider that sends no requests.
fn relation(store: &Store, user: UserId) -> Option<Relation> {
    if store.me() == Some(user)
        || store.user(user).is_some_and(|account| account.bot)
        || store.blocked(user)
    {
        return None;
    }
    let social = store.social(user);
    let person = |action: &str| format!("friend:{}:{action}", user.0);
    let item = |id: String, label: &str| MenuEntry::Item(id, label.to_owned(), None);
    let (state, menu) = match store.relationship(user).map(|known| known.kind) {
        None if store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.friend_requests) =>
        {
            (Friendship::None, Vec::new())
        }
        Some(RelationshipKind::Friend) => (
            Friendship::Friend,
            social
                .remove
                .allowed()
                .then(|| item(person("remove"), t("Eliminar amigo")))
                .into_iter()
                .collect(),
        ),
        Some(RelationshipKind::OutgoingRequest) => (
            Friendship::Outgoing,
            social
                .remove
                .allowed()
                .then(|| item(person("cancel"), t("Cancelar solicitud de amistad")))
                .into_iter()
                .collect(),
        ),
        Some(RelationshipKind::IncomingRequest) => {
            let mut menu = Vec::new();
            if social.accept.allowed() {
                menu.push(item(person("accept"), t("Aceptar")));
            }
            if social.remove.allowed() {
                menu.push(item(person("dismiss"), t("Ignorar")));
            }
            (Friendship::Incoming, menu)
        }
        _ => return None,
    };
    let action = match state {
        Friendship::None => person("add"),
        _ => format!("{}{}", ids::CARD_RELATION, user.0),
    };
    Some(Relation {
        state,
        action,
        menu,
    })
}

/// The entries of the «…» menu of `user`'s card, in three groups: their
/// profile, sharing and inviting; ignoring, blocking; and the developer's ID.
fn more_entries(ui: &Ui, store: &Store, user: UserId, server: Option<ServerId>) -> Vec<MenuEntry> {
    let item = |id: String, label: &str| MenuEntry::Item(id, label.to_owned(), None);
    let mut groups = Vec::new();
    let mut view = vec![item(
        format!("{}{user}", ids::FULL),
        t("Ver perfil completo"),
    )];
    if store
        .provider
        .as_ref()
        .is_some_and(|info| !info.capabilities.trusted_links.is_empty())
    {
        view.push(item(format!("{}{user}", ids::SHARE), t("Compartir perfil")));
    }
    view.extend(crate::person_menu::invite_submenu(ui, store, user, server));
    groups.push(view);
    let mut safety = crate::ignore::menu(store, user);
    safety.extend(crate::person_menu::block_items(store, user));
    if store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.report_form.is_some())
    {
        safety.push(MenuEntry::Danger(
            format!("{}{user}", ids::REPORT),
            t("Denunciar perfil de usuario").into(),
        ));
    }
    groups.push(safety);
    if ui.prefs.developer {
        groups.push(vec![item(
            format!("{}{}", rune_components::voice::ids::COPY_ID, user.0),
            t("Copiar ID del usuario"),
        )]);
    }
    let mut entries = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.extend(group);
    }
    entries
}

/// The card of `user` as a popout shows it. While its profile is read and
/// none is kept, a card has no content for its first 250 ms, then its
/// skeleton ([`SKELETON_AFTER_MS`]).
pub(crate) fn popout_card(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Option<Profile> {
    card_of(ui, store, server, user, look).filter(|card| {
        !card.loading
            || crate::identity::effect_elapsed(ui, store, user, server) >= SKELETON_AFTER_MS
    })
}

/// A server as the profiles show it: its icon, else its initial.
fn server_face(ui: &Ui, server: &rune_model::Server, look: &Look) -> Face {
    Face {
        name: server.shown_name().into(),
        accent: look.ink(server.accent),
        image: ui.image(&server.icon),
    }
}

/// The banner a card shows: the server's, else the profile's, else the
/// account's, each only when it reported one.
pub(crate) fn banner<'a>(
    account: &'a rune_model::User,
    loaded: Option<&'a UserProfile>,
) -> Option<&'a rune_model::RemoteImage> {
    loaded
        .and_then(|loaded| loaded.server_banner.image().or(loaded.banner.image()))
        .or(account.banner.image())
}

/// What restricts someone, as their card and the member list say it.
pub(crate) fn restriction_label(restriction: Restriction, wall: Timestamp) -> String {
    match restriction {
        Restriction::TimedOut { until } => {
            tr!(
                "Aislado hasta {until}",
                until = parts::until_label(until, wall)
            )
        }
        Restriction::Pending => t("Verificación pendiente").into(),
        Restriction::Quarantined => t("Retenido por la moderación automática").into(),
        Restriction::TagQuarantined => {
            t("Retenido por la moderación automática por su etiqueta de servidor").into()
        }
    }
}

/// What the composer says while the account is timed out in `server`:
/// until when, and how long is left. Other restrictions keep their reason.
pub(crate) fn timeout_notice(store: &Store, server: Option<ServerId>) -> Option<String> {
    let Restriction::TimedOut { until } = store.restriction(server?)? else {
        return None;
    };
    let wall = store.wall();
    // Rounded up: a timeout is never shown as over before it is.
    let minutes = (until.0 - wall.0 + 59_999) / 60_000;
    let left = match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    };
    Some(tr!(
        "Estás aislado hasta {until} · quedan {left}",
        until = parts::until_label(until, wall),
        left
    ))
}

/// What someone shares with the account, once known: "2 servidores en
/// común · 1 amigo en común". `None` while neither was reported.
fn mutual_summary(profile: &UserProfile) -> Option<String> {
    let servers = profile
        .mutual_servers
        .as_ref()
        .map(|list| match list.len() {
            0 => t("Sin servidores en común").to_owned(),
            1 => t("1 servidor en común").to_owned(),
            n => tr!("{n} servidores en común", n),
        });
    let friends = profile
        .mutual_friends
        .as_ref()
        .map(|list| match list.len() {
            0 => t("Sin amigos en común").to_owned(),
            1 => t("1 amigo en común").to_owned(),
            n => tr!("{n} amigos en común", n),
        });
    let parts: Vec<String> = [servers, friends].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Ask for `user`'s profile in `server` unless a current one is kept or one
/// is on its way; nothing when the provider cannot. One kept but older than
/// a minute shows while it is asked for again.
pub(crate) fn request_profile(
    ui: &mut Ui,
    store: &mut Store,
    user: UserId,
    server: Option<ServerId>,
) -> Vec<Intent> {
    // The account's private note on them comes with their profile.
    let mut intents = crate::account::load_note(ui, store, user);
    if !store.profile_stale(user, server) || store.profile_loading_in(user, server) {
        return intents;
    }
    let request = ui.fresh_request();
    if !store.begin_profile(user, server, request).allowed() {
        return intents;
    }
    intents.push(Intent::Command(Command::LoadProfile {
        request,
        user,
        server,
        // shortcut: one loaded while effects were hidden lacks its effect
        // until it is asked for again, a minute on (`profile_stale`).
        effect: ui.prefs.cosmetics.effects,
    }));
    intents
}

/// After a layout: once the member list's card is open, take where its
/// row is, so the card stays there (see [`Profiles::member_at`]). Only the
/// row of the list counts; a mention of the same person does not.
pub(crate) fn hold_member_card(ui: &mut Ui, tree: &limn::UiTree) {
    let Some(user) = ui.profile else {
        ui.profiles.member_at = None;
        return;
    };
    if ui.profiles.member_at.is_some_and(|(held, _)| held == user) {
        return;
    }
    ui.profiles.member_at = member_row(tree, &format!("member:{user}"))
        .map(|row| (user, limn::Point::new(row.x, row.y)));
}

/// Where the control `id` is in the member list, if it is a row of it as
/// of the last layout: a mention of the same person elsewhere shares its ID.
pub(crate) fn member_row(tree: &limn::UiTree, id: &str) -> Option<limn::Rect> {
    let regions = &tree.frame().regions;
    let list = regions
        .iter()
        .find(|region| {
            region.id == crate::navigation::MEMBERS && region.kind == limn::HitKind::Scroll
        })?
        .rect;
    regions
        .iter()
        .find(|region| {
            region.id == id
                && region.kind == limn::HitKind::Button
                && list.contains(limn::Point::new(
                    region.rect.x + region.rect.width / 2.0,
                    region.rect.y + region.rect.height / 2.0,
                ))
        })
        .map(|region| region.rect)
}

/// After a layout: what scrolling closes the card open, if any: the member
/// list under a member's card, the conversation under an author's.
pub(crate) fn hold_scroll_close(ui: &mut Ui, tree: &limn::UiTree) {
    let (scroller, dismiss) = if ui.profile.is_some() {
        (crate::navigation::MEMBERS.to_owned(), ids::MEMBER_DISMISS)
    } else if let (Some(_), Some(channel)) = (ui.profiles.author, ui.channel()) {
        (crate::timeline_id(channel), ids::AUTHOR_DISMISS)
    } else {
        ui.profiles.scroll_close = None;
        return;
    };
    let regions = &tree.frame().regions;
    let rect = |id: &str, kind: limn::HitKind| {
        regions
            .iter()
            .find(|region| region.id == id && region.kind == kind)
            .map(|region| region.rect)
    };
    ui.profiles.scroll_close = rect(&scroller, limn::HitKind::Scroll).zip(rect(
        &format!("popover_surface:{dismiss}"),
        limn::HitKind::Surface,
    ));
}

/// Before the tree sees `event`: a card closed when the person scrolls
/// what it stands against, as the official popout closes when an ancestor
/// of its anchor scrolls. The list's own corrections of its offset are no
/// event, so they close nothing (2026-10-18: a held card stayed while its
/// list scrolled away). Only the wheel and the touchpad reach the list
/// while a card is open: a press on its scroll bar lands on the card's
/// backdrop, which closes it, and the keys stay inside the card.
pub(crate) fn close_card_on_scroll(ui: &mut Ui, event: &UiEvent) {
    let (UiEvent::Scroll { position, .. } | UiEvent::ScrollDirect { position, .. }) = event else {
        return;
    };
    let closes = ui
        .profiles
        .scroll_close
        .is_some_and(|(under, card)| under.contains(*position) && !card.contains(*position));
    if closes {
        ui.profile = None;
        ui.profiles.author = None;
        ui.profiles.scroll_close = None;
    }
}

/// After a layout: the same for the card of a message's author, from its
/// name or mark, whose rows move as messages arrive (its right edge: the
/// card opens to its right).
pub(crate) fn hold_author_card(ui: &mut Ui, tree: &limn::UiTree) {
    let Some(target) = ui.profiles.author else {
        ui.profiles.author_at = None;
        return;
    };
    if ui
        .profiles
        .author_at
        .is_some_and(|(held, _)| held == target)
    {
        return;
    }
    let (message, on_mark) = target;
    let anchor = if on_mark {
        ids::author_mark(message.0)
    } else {
        ids::author_name(message.0)
    };
    ui.profiles.author_at = tree
        .frame()
        .regions
        .iter()
        .find(|region| region.id == anchor && region.kind == limn::HitKind::Button)
        .map(|region| {
            let corner = limn::Point::new(region.rect.x + region.rect.width, region.rect.y);
            (target, corner)
        });
}

/// The status timer as kept: `status:until:then`.
fn timer_value(timed: &Timed) -> String {
    format!(
        "{}:{}:{}",
        parts::status_key(timed.status),
        timed.until.0,
        parts::status_key(timed.then)
    )
}

fn parse_timer(value: &str) -> Option<Timed> {
    let mut fields = value.split(':');
    let status = parts::status_of(fields.next()?)?;
    let until = Timestamp(fields.next()?.parse().ok()?);
    let then = parts::status_of(fields.next()?)?;
    Some(Timed {
        status,
        until,
        then,
    })
}

fn keep_timer(ui: &Ui) -> Intent {
    Intent::SaveSetting {
        key: STATUS_TIMER.into(),
        value: ui.profiles.timed.as_ref().map(timer_value),
    }
}

fn keep_custom_until(ui: &Ui) -> Intent {
    Intent::SaveSetting {
        key: CUSTOM_UNTIL.into(),
        value: ui.profiles.custom_until.map(|until| until.0.to_string()),
    }
}

/// The settings worth keeping between runs.
pub(crate) fn kept(ui: &Ui) -> Vec<(&'static str, Option<String>)> {
    vec![
        (STATUS_TIMER, ui.profiles.timed.as_ref().map(timer_value)),
        (
            CUSTOM_UNTIL,
            ui.profiles.custom_until.map(|until| until.0.to_string()),
        ),
        (SPEAKER, ui.audio.output.clone()),
    ]
}

/// Take back the account's timers [`kept`] kept; the output is the device's.
pub(crate) fn restore(ui: &mut Ui, setting: &impl Fn(&str) -> Option<String>) {
    ui.profiles.timed = setting(STATUS_TIMER).and_then(|value| parse_timer(&value));
    ui.profiles.custom_until = setting(CUSTOM_UNTIL)
        .and_then(|value| value.parse().ok())
        .map(Timestamp);
}

/// Carry out the timers that are due at `wall`: set back a status chosen
/// for a while, and clear a custom status set to go. The host calls it on
/// every frame; [`next_tick`] says when the next one is due.
pub fn tick(ui: &mut Ui, store: &Store, wall: Timestamp) -> Vec<Intent> {
    let mut intents = Vec::new();
    if let Some(timed) = ui.profiles.timed.filter(|timed| timed.until <= wall) {
        ui.profiles.timed = None;
        intents.push(keep_timer(ui));
        // Only what this client set goes back: a status changed elsewhere stays.
        let current = store.me().map(|me| store.presence(me).status);
        if has_presence(store) && current.is_none_or(|current| current == timed.status) {
            intents.push(Intent::Command(Command::SetStatus(timed.then)));
        }
    }
    if ui.profiles.custom_until.is_some_and(|until| until <= wall) {
        ui.profiles.custom_until = None;
        intents.push(keep_custom_until(ui));
        if custom_denied(store).is_none() {
            intents.push(Intent::Command(Command::SetCustomStatus {
                text: None,
                emoji: None,
                until: None,
            }));
        }
    }
    // A search the provider was still indexing is asked again (P2).
    intents.extend(crate::search::tick(ui, store, wall));
    intents
}

/// How long until a timer of [`tick`] is due, seen at `wall`. Also when a
/// card opened lately may show its skeleton, so the window paints then.
pub fn next_tick(ui: &Ui, wall: Timestamp) -> Option<Duration> {
    [
        ui.profiles.timed.map(|timed| timed.until),
        ui.profiles.custom_until,
        // Only a moment still to come: a card opened long ago is no timer.
        ui.profiles
            .effect_since
            .map(|(_, _, since)| Timestamp(since.0 + SKELETON_AFTER_MS))
            .filter(|due| due.0 > wall.0),
    ]
    .into_iter()
    .flatten()
    .map(|until| Duration::from_millis((until.0 - wall.0).max(0) as u64))
    .chain(crate::search::next_tick(ui, wall))
    .min()
}

/// Answer what an event of the provider changed: a session that starts
/// learns the status chosen for a while; the editor learns how its change
/// went.
pub fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    match applied {
        Applied::Ready => {
            if let Some(editor) = ui.profiles.editor.as_mut() {
                editor.saving = false;
            }
            let mut intents = tick(ui, store, wall_now());
            let current = store.me().map(|me| store.presence(me).status);
            if let Some(timed) = ui
                .profiles
                .timed
                .filter(|timed| has_presence(store) && current != Some(timed.status))
            {
                intents.push(Intent::Command(Command::SetStatus(timed.status)));
            }
            intents
        }
        Applied::User(user) if Some(*user) == store.me() => {
            let (Some(editor), Some(me)) = (ui.profiles.editor.as_mut(), store.me_user()) else {
                return Vec::new();
            };
            // Saved, or changed elsewhere with nothing touched here: the
            // fields show the account as it is now.
            if editor.server.is_some()
                || (!editor.saving && changes(&editor.edit, me) != ProfileEdit::default())
            {
                return Vec::new();
            }
            *editor = editor.on(None);
            fill_fields(me)
        }
        // Saved in a server, or that profile arrived with nothing touched.
        Applied::ProfileSaved {
            server: Some(server),
        } => saved_in(ui, store, *server, true),
        Applied::Profile(user) if Some(*user) == store.me() => {
            match ui.profiles.editor.as_ref().and_then(|editor| editor.server) {
                Some(server) => saved_in(ui, store, server, false),
                None => Vec::new(),
            }
        }
        Applied::Failed {
            failure: Failure::Profile | Failure::ServerProfile { .. },
            reason,
            ..
        } => {
            if let Some(editor) = ui.profiles.editor.as_mut() {
                editor.saving = false;
                editor.failure = Some(reason.clone());
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// A card's button opens its menu, or closes the one it has open.
fn toggle_card_menu(ui: &mut Ui, user: u64, menu: CardMenu) -> Vec<Intent> {
    let target = (UserId(user), menu);
    ui.profiles.menu = (ui.profiles.menu != Some(target)).then_some(target);
    Vec::new()
}

/// Choosing an entry closes a card's menu, except the button that opens it
/// again and what keeps it open (a role's check mark).
pub(crate) fn close_card_menu(ui: &mut Ui, action: &UiAction) {
    let UiAction::Clicked { id } = action else {
        return;
    };
    let toggles = [ids::CARD_RELATION, ids::CARD_MORE, ids::CARD_ROLES]
        .iter()
        .any(|prefix| {
            id.strip_prefix(prefix)
                .is_some_and(|rest| rest.parse::<u64>().is_ok())
        });
    if !toggles && !crate::person_menu::keeps_open(id) {
        ui.profiles.menu = None;
    }
}

/// Apply an action of the profiles; `None` for the rest of the window.
pub fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } => clicked(ui, store, id, now),
        UiAction::InputChanged { id, value } => input(ui, store, id, value),
        UiAction::Submitted { id, .. } if id == ids::CUSTOM_TEXT => Some(save_custom(ui)),
        UiAction::Submitted { id, .. } if id.starts_with(ids::MESSAGE) => {
            Some(send_message(ui, store, id, now))
        }
        // Profile editing (edicion-de-perfil).
        UiAction::AreaSliderChanged { .. } | UiAction::SliderChanged { .. } => {
            crate::framing::update(ui, store, action)
        }
        _ => None,
    }
}

fn input(ui: &mut Ui, store: &Store, id: &str, value: &str) -> Option<Vec<Intent>> {
    if id == ids::CUSTOM_TEXT {
        ui.profiles.custom.as_mut()?.text = value.to_owned();
        return Some(Vec::new());
    }
    if let Some(user) = id
        .strip_prefix(ids::MESSAGE)
        .and_then(|rest| rest.parse::<u64>().ok())
    {
        ui.profiles.message = Some((UserId(user), value.to_owned()));
        return Some(Vec::new());
    }
    if id.starts_with("profile:") && edit_denied(store).is_some() {
        return Some(Vec::new());
    }
    let editor = ui.profiles.editor.as_mut()?;
    let text = Some(Some(value.to_owned()));
    match id {
        ids::NAME => editor.edit.display_name = text,
        ids::PRONOUNS => editor.edit.pronouns = text,
        ids::NICK => editor.server_edit.nick = text,
        ids::ABOUT if editor.server.is_some() => editor.server_edit.about = text,
        ids::ABOUT => editor.edit.about = text,
        ids::COLOR_HEX => {
            editor.hex = Some(value.to_owned());
            if let Some(color) = parse_hex(value) {
                editor.edit.accent = Some(color);
            }
        }
        _ => return None,
    }
    Some(Vec::new())
}

/// Enter in a card's field: the text goes to their conversation and the card
/// closes (REA §4.5). Empty text does nothing. Without a conversation it is
/// opened first, and the text goes once it exists (see [`message_on_applied`]).
fn send_message(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Vec<Intent> {
    let Some(user) = id
        .strip_prefix(ids::MESSAGE)
        .and_then(|rest| rest.parse::<u64>().ok())
        .map(UserId)
    else {
        return Vec::new();
    };
    let has_text = ui
        .profiles
        .message
        .as_ref()
        .is_some_and(|(who, text)| *who == user && !text.trim().is_empty());
    if !has_text {
        return Vec::new();
    }
    let Some(channel) = store.direct_with(user) else {
        ui.profiles.message_waiting = Some(user);
        return crate::conversations::open_direct(ui, store, user, now);
    };
    let text = ui
        .profiles
        .message
        .take()
        .map(|(_, text)| text.trim().to_owned())
        .unwrap_or_default();
    let mut intents = crate::conversations::open_direct(ui, store, user, now);
    intents.push(Intent::Send {
        channel,
        content: text,
        reply_to: None,
        mention_reply: false,
    });
    close_floating(ui);
    intents
}

/// The text of a card waits for its new conversation: it is sent once the
/// provider has opened it. When the conversation cannot open, the card keeps
/// its text and stays.
pub(crate) fn message_on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    let Some(user) = ui.profiles.message_waiting else {
        return Vec::new();
    };
    match applied {
        Applied::Created {
            created: Created::Channel(channel),
            ..
        } if store.direct_with(user) == Some(*channel) => {
            ui.profiles.message_waiting = None;
            let Some((who, text)) = ui.profiles.message.take() else {
                return Vec::new();
            };
            if who != user {
                ui.profiles.message = Some((who, text));
                return Vec::new();
            }
            close_floating(ui);
            vec![Intent::Send {
                channel: *channel,
                content: text.trim().to_owned(),
                reply_to: None,
                mention_reply: false,
            }]
        }
        Applied::Failed {
            failure: Failure::Direct { .. },
            ..
        } => {
            ui.profiles.message_waiting = None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Close what floats from the panel and the cards, before opening another.
pub(crate) fn close_floating(ui: &mut Ui) {
    ui.status_menu = false;
    ui.profiles.status_open = false;
    ui.profile = None;
    ui.profiles.menu = None;
    ui.profiles.message = None;
    ui.profiles.message_waiting = None;
    // The camera's panel takes its test along, unless the voice page shows it.
    if ui.profiles.devices.take().is_some_and(|quick| quick.camera) && ui.settings.is_none() {
        ui.video.testing = false;
    }
    ui.profiles.author = None;
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    // The nickname of a server, from the editor's selector: the server menu opens it.
    if id.starts_with("sm:profile:") {
        if let Some(editor) = ui.profiles.editor.as_mut() {
            editor.servers_open = false;
        }
        return None;
    }
    if id == "me:status" {
        let open = !ui.status_menu;
        close_floating(ui);
        ui.status_menu = open;
        // Its banner, effect and badges come with the full profile, as the
        // official client loads it for this popout (2026-10-18: only what
        // the account's identity carried showed).
        return Some(match store.me().filter(|_| open) {
            Some(me) => request_profile(ui, store, me, None),
            None => Vec::new(),
        });
    }
    if !id.starts_with("pp:") && !id.starts_with("profile:") {
        return None;
    }
    // A late event from an editor rendered before capabilities changed must
    // not create a local draft or a false unsaved-changes guard.
    if edit_denied(store).is_some()
        && (id.starts_with(ids::COLOR)
            || matches!(id, ids::SERVERS | ids::SERVERS_DISMISS | ids::MAIN_PROFILE))
    {
        return Some(Vec::new());
    }
    let number = |prefix: &str| {
        id.strip_prefix(prefix)
            .and_then(|rest| rest.parse::<u64>().ok())
    };
    if let Some(rest) = id.strip_prefix(ids::STATUS) {
        return Some(choose_status(ui, store, rest));
    }
    if id.starts_with(ids::SUBMENU) {
        return Some(Vec::new());
    }
    if let Some(emoji) = id.strip_prefix(ids::CUSTOM_EMOJI_PICK) {
        let draft = ui.profiles.custom.as_mut()?;
        draft.emoji = Some(emoji.to_owned());
        draft.emoji_open = false;
        return Some(vec![Intent::Focus(ids::CUSTOM_TEXT.into())]);
    }
    if let Some(key) = id.strip_prefix(ids::CUSTOM_EXPIRY_PICK) {
        let draft = ui.profiles.custom.as_mut()?;
        draft.clear_after = ClearAfter::from_key(key).unwrap_or_default();
        draft.expiry_open = false;
        return Some(Vec::new());
    }
    if let Some(color) = id
        .strip_prefix(ids::COLOR)
        .and_then(|rest| rest.strip_prefix(':'))
        .filter(|color| *color != "hex")
    {
        let user = store.me_user().cloned();
        let editor = ui.profiles.editor.as_mut()?;
        match color {
            "custom" => {
                let current = editor
                    .edit
                    .accent
                    .or(user.map(|user| user.accent))
                    .unwrap_or(Rgb(0));
                editor.hex = Some(hex(current));
                return Some(vec![set_input(ids::COLOR_HEX, hex(current))]);
            }
            "dismiss" => editor.hex = None,
            color => editor.edit.accent = parse_hex(color).or(editor.edit.accent),
        }
        return Some(Vec::new());
    }
    if let Some(server) = number(ids::SCOPE) {
        return Some(edit_scope(ui, store, Some(ServerId(server))));
    }
    let picture = match id {
        ids::AVATAR_PICK | ids::AVATAR_REMOVE => Some(ProfileImageKind::Avatar),
        ids::BANNER_PICK | ids::BANNER_REMOVE => Some(ProfileImageKind::Banner),
        _ => None,
    };
    if let Some(kind) = picture {
        return Some(change_picture(
            ui,
            store,
            kind,
            matches!(id, ids::AVATAR_PICK | ids::BANNER_PICK),
        ));
    }
    if let Some(user) = number("profile:dm:") {
        ui.profile = None;
        ui.profiles.author = None;
        return Some(crate::conversations::open_direct(
            ui,
            store,
            UserId(user),
            now,
        ));
    }
    if id == ids::FULL_CLOSE {
        ui.profiles.full = None;
        return Some(Vec::new());
    }
    // A card's round buttons open their menus; choosing a share copies the
    // person's page on the provider's first trusted domain.
    if let Some(user) = number(ids::CARD_RELATION) {
        return Some(toggle_card_menu(ui, user, CardMenu::Relation));
    }
    if let Some(user) = number(ids::CARD_MORE) {
        return Some(toggle_card_menu(ui, user, CardMenu::More));
    }
    if let Some(user) = number(ids::CARD_ROLES) {
        return Some(toggle_card_menu(ui, user, CardMenu::Roles));
    }
    // A card's mutual friends or servers open the full profile on their tab.
    if let Some((user, tab)) = id
        .strip_prefix(ids::MUTUAL)
        .and_then(|rest| rest.split_once(':'))
    {
        if let (Ok(user), Some(tab)) = (user.parse::<u64>(), FullTab::from_key(tab)) {
            let (user, server) = (UserId(user), ui.server);
            close_floating(ui);
            ui.profiles.full = Some((user, server));
            ui.profiles.full_tab = tab;
            return Some(request_profile(ui, store, user, server));
        }
    }
    // Reporting copies the person's ID for the provider's form, and opens it.
    if let Some(user) = number(ids::REPORT) {
        let form = store
            .provider
            .as_ref()
            .and_then(|info| info.capabilities.report_form);
        return Some(match form {
            Some(form) => {
                ui.notify(t("ID copiado: pégalo en el formulario"), None, false, now);
                vec![
                    Intent::Copy(user.to_string()),
                    Intent::OpenInBrowser(form.to_owned()),
                ]
            }
            None => Vec::new(),
        });
    }
    if let Some(user) = number(ids::SHARE) {
        let link = store
            .provider
            .as_ref()
            .and_then(|info| info.capabilities.trusted_links.first())
            .map(|domain| format!("https://{domain}/users/{user}"));
        return Some(match link {
            Some(link) => {
                ui.notify(t("Enlace copiado"), None, false, now);
                vec![Intent::Copy(link)]
            }
            None => Vec::new(),
        });
    }
    if let Some(tab) = id.strip_prefix(ids::FULL_TAB).and_then(FullTab::from_key) {
        ui.profiles.full_tab = tab;
        return Some(Vec::new());
    }
    if let Some(user) = number(ids::FULL)
        .or_else(|| number(ids::FULL_BIO))
        .or_else(|| number(ids::FULL_NOTE))
    {
        let (user, server) = (UserId(user), ui.server);
        close_floating(ui);
        ui.profiles.full = Some((user, server));
        ui.profiles.full_tab = FullTab::default();
        return Some(request_profile(ui, store, user, server));
    }
    for (prefix, on_mark) in [("pp:name:", false), ("pp:avatar:", true)] {
        if let Some(message) = number(prefix) {
            let target = (MessageId(message), on_mark);
            let open = ui.profiles.author != Some(target);
            close_floating(ui);
            ui.profiles.author = open.then_some(target);
            let author = ui
                .channel()
                .and_then(|channel| store.timeline(channel)?.get(MessageId(message)))
                .map(|message| message.author)
                .filter(|_| open);
            return Some(match author {
                Some(author) => request_profile(ui, store, author, ui.server),
                None => Vec::new(),
            });
        }
    }
    Some(match id {
        ids::DISMISS => {
            ui.status_menu = false;
            ui.profiles.status_open = false;
            Vec::new()
        }
        ids::STATUS_MENU | ids::STATUS_MENU_DISMISS => {
            ui.profiles.status_open =
                has_presence(store) && id == ids::STATUS_MENU && !ui.profiles.status_open;
            Vec::new()
        }
        ids::EDIT => open_editor(ui, store, now),
        // The remembered accounts and «Agregar una cuenta» live in Mi cuenta.
        ids::SWITCH_ACCOUNT => {
            ui.status_menu = false;
            ui.profiles.status_open = false;
            crate::settings::open_settings(ui, store, crate::settings::SettingsPage::Account)
        }
        ids::CUSTOM => open_custom(ui, store),
        ids::COPY_ID => {
            ui.status_menu = false;
            let Some(me) = store.me() else {
                return Some(Vec::new());
            };
            ui.notify(t("ID copiado"), None, false, now);
            vec![Intent::Copy(me.0.to_string())]
        }
        ids::COPY_NAME => {
            let Some(me) = store.me_user() else {
                return Some(Vec::new());
            };
            let handle = me.name.clone();
            ui.notify(t("Nombre de usuario copiado"), None, false, now);
            vec![Intent::Copy(handle)]
        }
        ids::CUSTOM_CLOSE | ids::CUSTOM_CANCEL => {
            ui.profiles.custom = None;
            Vec::new()
        }
        ids::CUSTOM_EMOJI | ids::CUSTOM_EMOJI_DISMISS => {
            if let Some(draft) = ui.profiles.custom.as_mut() {
                draft.emoji_open = id == ids::CUSTOM_EMOJI && !draft.emoji_open;
            }
            Vec::new()
        }
        ids::CUSTOM_EXPIRY | ids::CUSTOM_EXPIRY_DISMISS => {
            if let Some(draft) = ui.profiles.custom.as_mut() {
                draft.expiry_open = id == ids::CUSTOM_EXPIRY && !draft.expiry_open;
            }
            Vec::new()
        }
        ids::CUSTOM_SAVE => save_custom(ui),
        ids::AUTHOR_DISMISS => {
            ui.profiles.author = None;
            ui.profiles.menu = None;
            Vec::new()
        }
        ids::MEMBER_DISMISS => {
            ui.profile = None;
            ui.profiles.menu = None;
            Vec::new()
        }
        ids::EDITOR_CLOSE => {
            if leave(ui, store) {
                vec![Intent::CancelProfileImage]
            } else {
                Vec::new()
            }
        }
        ids::MAIN_PROFILE => edit_scope(ui, store, None),
        ids::SERVERS | ids::SERVERS_DISMISS => {
            if let Some(editor) = ui.profiles.editor.as_mut() {
                editor.servers_open = id == ids::SERVERS && !editor.servers_open;
            }
            Vec::new()
        }
        ids::RESET => {
            let Some(editor) = ui.profiles.editor.as_mut() else {
                return Some(Vec::new());
            };
            let server = editor.server;
            *editor = Editor {
                servers_open: editor.servers_open,
                ..editor.on(server)
            };
            match server {
                Some(server) => fill_server_fields(store, server),
                None => store.me_user().map(fill_fields).unwrap_or_default(),
            }
        }
        ids::SAVE => save_profile(ui, store),
        _ => Vec::new(),
    })
}

/// Choose the output device `id`, or the system's.
pub(crate) fn choose_output(ui: &mut Ui, id: Option<String>) -> Vec<Intent> {
    ui.audio.output = id.clone();
    vec![Intent::SaveSetting {
        key: SPEAKER.into(),
        value: id,
    }]
}

/// Choose a status: `rest` is its key, and `:` and how long, if not for ever.
fn choose_status(ui: &mut Ui, store: &Store, rest: &str) -> Vec<Intent> {
    if !has_presence(store) {
        return Vec::new();
    }
    let (key, length) = rest.split_once(':').unwrap_or((rest, "forever"));
    let Some(status) = parts::status_of(key) else {
        return Vec::new();
    };
    let minutes = parts::DURATIONS
        .iter()
        .find(|(candidate, _, _)| *candidate == length)
        .map(|(_, _, minutes)| *minutes);
    ui.status_menu = false;
    ui.profiles.status_open = false;
    // What it goes back to: what was before any status chosen for a while.
    let current = store
        .me()
        .map(|me| store.presence(me).status)
        .filter(|status| *status != Status::Offline)
        .unwrap_or(Status::Online);
    let then = ui.profiles.timed.map_or(current, |timed| timed.then);
    ui.profiles.timed = minutes.map(|minutes| Timed {
        status,
        until: wall_now().plus_minutes(minutes),
        then,
    });
    vec![Intent::Command(Command::SetStatus(status)), keep_timer(ui)]
}

/// The emoji a custom status starts with, if one of the picker's, and the rest.
fn split_emoji(line: &str) -> (Option<String>, String) {
    match rune_components::emoji::EMOJI
        .iter()
        .find(|emoji| line.starts_with(**emoji))
    {
        Some(emoji) => (
            Some((*emoji).to_owned()),
            line[emoji.len()..].trim_start().to_owned(),
        ),
        None => (None, line.to_owned()),
    }
}

fn open_custom(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    if custom_denied(store).is_some() {
        return Vec::new();
    }
    close_floating(ui);
    let current = store
        .me()
        .and_then(|me| store.presence(me).custom)
        .unwrap_or_default();
    let (emoji, text) = split_emoji(&current);
    ui.profiles.custom = Some(CustomDraft {
        emoji,
        text: text.clone(),
        ..CustomDraft::default()
    });
    vec![
        set_input(ids::CUSTOM_TEXT, text),
        Intent::Focus(ids::CUSTOM_TEXT.into()),
    ]
}

/// Send the custom status written, unless it is too long; an empty one takes
/// it away. It goes by itself when the dialog said so.
fn save_custom(ui: &mut Ui) -> Vec<Intent> {
    let Some(draft) = &ui.profiles.custom else {
        return Vec::new();
    };
    let text = parts::custom_line(draft.emoji.as_deref(), &draft.text);
    if parts::custom_error(text.as_deref()).is_some() {
        return Vec::new();
    }
    ui.profiles.custom_until = text
        .as_ref()
        .and(draft.clear_after.minutes())
        .map(|minutes| wall_now().plus_minutes(minutes));
    let words = draft.text.trim().to_owned();
    let command = Command::SetCustomStatus {
        text: text.as_ref().and((!words.is_empty()).then_some(words)),
        emoji: text.as_ref().and(draft.emoji.clone()),
        until: ui.profiles.custom_until,
    };
    ui.profiles.custom = None;
    vec![Intent::Command(command), keep_custom_until(ui)]
}

/// Open the editor over the whole window; over the settings, it goes back
/// to them when it closes.
fn open_editor(ui: &mut Ui, store: &mut Store, now: Instant) -> Vec<Intent> {
    let Some(me) = store.me_user().cloned() else {
        return Vec::new();
    };
    close_floating(ui);
    let back_to = ui.settings;
    let mut intents = if back_to.is_some() {
        settings::clicked(ui, store, "settings:close", now)
    } else {
        Vec::new()
    };
    ui.profiles.editor = Some(Editor {
        back_to,
        ..Editor::default()
    });
    if edit_denied(store).is_none() {
        intents.extend(fill_fields(&me));
    }
    intents.push(Intent::Focus(ids::EDITOR_CLOSE.into()));
    intents
}

/// Close the editor, unless something is not saved. True when a picture
/// was being chosen: its selector has to be told.
fn leave(ui: &mut Ui, store: &Store) -> bool {
    let Some(editor) = ui.profiles.editor.as_mut() else {
        return false;
    };
    if edit_denied(store).is_none() && editor.dirty(store) {
        editor.nudged = true;
        return false;
    }
    let back_to = editor.back_to;
    let picking = editor.picking.is_some();
    ui.profiles.editor = None;
    if back_to.is_some() {
        ui.settings = back_to;
    }
    picking
}

/// Send what the editor changed, if the provider can take it and every field
/// is valid.
fn save_profile(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let denied = edit_denied(store);
    let (Some(editor), Some(user)) = (ui.profiles.editor.as_mut(), store.me_user()) else {
        return Vec::new();
    };
    if denied.is_some() || editor.saving || editor.picking.is_some() {
        return Vec::new();
    }
    if let Some(server) = editor.server {
        let (nick, about) = server_fields(&editor.server_edit, store, server);
        if EditorErrors::of(&nick, "", &about).any() {
            return Vec::new();
        }
        let edit = server_changes(&editor.server_edit, store, server);
        let abilities = store.profile_abilities(Some(server));
        let allowed = (edit.nick.is_none() || abilities.nick.allowed())
            && (edit.about.is_none() || abilities.text.allowed())
            && (edit.avatar.is_none() && edit.banner.is_none() || abilities.images.allowed());
        if edit == ServerProfileEdit::default() || !allowed {
            return Vec::new();
        }
        editor.saving = true;
        editor.nudged = false;
        editor.failure = None;
        return vec![Intent::Command(Command::EditServerProfile { server, edit })];
    }
    let (name, pronouns, about) = fields(&editor.edit, user);
    // What is wrong is said under each field; nothing leaves until it is fixed.
    if EditorErrors::of(&name, &pronouns, &about).any() {
        return Vec::new();
    }
    let edit = changes(&editor.edit, user);
    let pictures = edit.avatar.is_some() || edit.banner.is_some();
    let abilities = store.profile_abilities(None);
    if edit == ProfileEdit::default() || (pictures && !abilities.images.allowed()) {
        return Vec::new();
    }
    // What the plan does not allow is said, before the service would refuse
    // it: the plan may have arrived after the picture was chosen.
    let banner = matches!(&edit.banner, Some(Some(_)));
    let moving = matches!(&edit.avatar, Some(Some(avatar)) if avatar.animated());
    let unallowed = (banner.then(|| abilities.banner.reason()).flatten())
        .or(moving.then(|| abilities.animated.reason()).flatten());
    if let Some(reason) = unallowed {
        editor.failure = Some(reason.to_owned());
        return Vec::new();
    }
    editor.saving = true;
    editor.nudged = false;
    editor.failure = None;
    vec![Intent::Command(Command::EditProfile(edit))]
}

/// Escape closes the innermost menu of the custom status dialog, or of the
/// editor, else leaves the editor. Over the nickname dialog, or the custom
/// status dialog without a menu open, the dialog answers it.
pub fn intercept(ui: &mut Ui, store: &Store, event: &UiEvent) -> Option<Vec<Intent>> {
    if !matches!(
        event,
        UiEvent::Key {
            key: Key::Escape,
            ..
        }
    ) || ui.server_menu.dialog.is_some()
    {
        return None;
    }
    if let Some(draft) = ui.profiles.custom.as_mut() {
        // The menus open without the focus: the dialog would take the key.
        let closed = std::mem::take(&mut draft.expiry_open) | std::mem::take(&mut draft.emoji_open);
        return closed.then(Vec::new);
    }
    let editor = ui.profiles.editor.as_mut()?;
    if editor.hex.take().is_none() && !std::mem::take(&mut editor.servers_open) && leave(ui, store)
    {
        return Some(vec![Intent::CancelProfileImage]);
    }
    Some(Vec::new())
}

/// The account's panel, with its popout and the quick voice panel of its
/// arrows when they are open.
pub fn user_panel(ui: &Ui, store: &Store, look: &Look, wall: Timestamp) -> Element {
    let me = store.me();
    let user = me.and_then(|me| store.user(me));
    let presence = me.map(|me| store.presence(me)).unwrap_or_default();
    let popout = me
        .filter(|_| ui.status_menu)
        .and_then(|me| card_of(ui, store, None, me, look))
        .map(|profile| {
            let until = ui
                .profiles
                .timed
                .filter(|timed| timed.status == profile.status)
                .map(|timed| {
                    tr!(
                        "hasta {until}",
                        until = parts::until_label(timed.until, wall)
                    )
                });
            parts::own_popout(
                &OwnPopout {
                    profile,
                    until,
                    custom_denied: custom_denied(store),
                    edit_denied: edit_denied(store),
                    status_open: ui.profiles.status_open,
                    copy_id: ui.prefs.developer,
                    switch_account: store
                        .provider
                        .as_ref()
                        .is_some_and(|info| info.capabilities.saved_accounts),
                },
                look,
            )
        });
    // What the server holds in the call the account is in.
    let held = store.my_voice().filter(|state| {
        store
            .call()
            .is_some_and(|call| call.channel == state.channel)
    });
    chrome::user_panel(
        UserPanel {
            image: user.and_then(|user| ui.picture(user.picture())),
            name: user
                .map(|user| user.shown_name().to_owned())
                .unwrap_or_default(),
            handle: user.map(|user| user.name.clone()).unwrap_or_default(),
            accent: user.map_or(look.faint, |user| look.ink(user.accent)),
            status: has_presence(store).then(|| look.status(presence.status)),
            status_label: if has_presence(store) {
                presence
                    .custom
                    .clone()
                    .unwrap_or_else(|| connection::own_status(store, presence.status))
            } else {
                user.map(|user| user.name.clone()).unwrap_or_default()
            },
            muted: ui.muted,
            deafened: ui.deafened,
            server_muted: held.is_some_and(|state| state.server_muted),
            server_deafened: held.is_some_and(|state| state.server_deafened),
            voice: store
                .provider
                .as_ref()
                .is_some_and(|info| info.capabilities.voice != VoiceSupport::None),
            popout,
            quick: crate::quick_voice::view(ui, store, look, false),
        },
        look,
    )
}

fn has_presence(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.presence)
}

/// The profile editor over the whole window, while open.
pub fn editor_view(ui: &Ui, store: &Store, context: &ViewContext) -> Option<Element> {
    let editor = ui.profiles.editor.as_ref()?;
    let user = store.me_user()?;
    let look = &ui.look();
    let denied = edit_denied(store);
    let abilities = store.profile_abilities(editor.server);
    let unchanged = ProfileEdit::default();
    let edit = if denied.is_none() {
        &editor.edit
    } else {
        &unchanged
    };
    let (name, pronouns, mut about) = fields(edit, user);
    let changed = shown(user, edit);
    let mut preview = card_of(ui, store, editor.server, user.id, look)?;
    // The draft is what this preview shows, even while the read is in flight.
    preview.loading = false;
    preview.name = changed.shown_name().to_owned();
    preview.pronouns = changed.pronouns.clone();
    preview.about = changed.about.clone();
    preview.accent = look.ink(changed.accent);
    // What a picture chosen or taken away here looks like, in its place.
    let (avatar, banner, mut has_avatar, mut has_banner) = match editor.server {
        Some(_) => (
            &editor.server_edit.avatar,
            &editor.server_edit.banner,
            store
                .member_of(editor.server?, user.id)
                .is_some_and(|member| member.avatar.is_some()),
            store
                .profile(user.id, editor.server)
                .is_some_and(|loaded| loaded.server_banner.image().is_some()),
        ),
        None => (
            &edit.avatar,
            &edit.banner,
            user.avatar.is_some(),
            user.banner.image().is_some(),
        ),
    };
    let account_banner = ui.picture(user.banner.image());
    match avatar {
        Some(Some(_)) => preview.image = editor.avatar_preview.clone(),
        // Taken away in a server, the account's shows there again.
        Some(None) if editor.server.is_some() => preview.image = ui.picture(user.picture()),
        // Taken away everywhere, the service's default shows.
        Some(None) => preview.image = ui.picture(user.default_avatar.as_ref()),
        None => {}
    }
    match banner {
        Some(Some(_)) => preview.banner = editor.banner_preview.clone(),
        Some(None) if editor.server.is_some() => preview.banner = account_banner,
        Some(None) => preview.banner = None,
        None => {}
    }
    has_avatar = matches!(avatar, Some(Some(_))) || (has_avatar && avatar.is_none());
    has_banner = matches!(banner, Some(Some(_))) || (has_banner && banner.is_none());
    let scope = editor.server.and_then(|server| {
        let (nick, server_about) = server_fields(&editor.server_edit, store, server);
        preview.name = if nick.trim().is_empty() {
            user.shown_name().to_owned()
        } else {
            nick.trim().to_owned()
        };
        if !server_about.trim().is_empty() {
            preview.about = Some(server_about.trim().to_owned());
        }
        about = server_about;
        Some(EditorScope {
            server: store.server(server)?.shown_name().into(),
            nick,
            nick_denied: abilities.nick.reason().map(str::to_owned),
            about_denied: abilities.text.reason().map(str::to_owned),
        })
    });
    let scoped = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.server_profiles);
    let servers = store
        .servers()
        .iter()
        .map(|server| rune_components::profile::EditorServer {
            // Without profiles per server, the server menu still renames.
            action: if scoped {
                format!("{}{}", ids::SCOPE, server.id)
            } else {
                format!("sm:profile:{}", server.id)
            },
            name: server.shown_name().into(),
            nick: store
                .member_of(server.id, user.id)
                .and_then(|member| member.nick.clone()),
            image: ui.image(&server.icon),
            initials: server.initials(),
            accent: look.ink(server.accent),
        })
        .collect();
    let simulated = store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == rune_provider::ProviderKind::Simulated);
    Some(parts::editor(
        &parts::Editor {
            pictures: EditorPictures {
                banner: preview.banner.clone(),
                has_avatar,
                has_banner,
                picking: editor
                    .picking
                    .map(|(_, kind, _)| kind == ProfileImageKind::Avatar),
                denied: abilities.images.reason().map(str::to_owned),
                note: simulated.then(|| {
                    t("Simulación: la imagen guardada queda sólo en la memoria de esta sesión.")
                        .into()
                }),
                // Profile editing (edicion-de-perfil).
                banner_denied: abilities
                    .images
                    .allowed()
                    .then(|| abilities.banner.reason().map(str::to_owned))
                    .flatten(),
                framing: crate::framing::view(
                    editor,
                    // Only an avatar is kept still by the plan: a banner it
                    // denies is not chosen, and its reason shows by its button.
                    match editor.framing.as_ref().map(|framing| framing.kind) {
                        Some(ProfileImageKind::Avatar) => abilities.animated.reason(),
                        _ => None,
                    },
                ),
            },
            preview,
            name,
            pronouns,
            about,
            accent: changed.accent,
            hex: editor.hex.clone(),
            failure: editor.failure.clone(),
            denied: denied.clone(),
            servers,
            servers_open: editor.servers_open,
            bar: (denied.is_none() && editor.dirty(store)).then_some(UnsavedBar {
                saving: editor.saving,
                nudged: editor.nudged,
            }),
            credentials: editor
                .server
                .is_none()
                .then(|| abilities.credentials.reason().map(str::to_owned))
                .flatten(),
            scope,
            picked: crate::color_picker::picked(ui, ids::COLOR),
        },
        context.window,
        look,
    ))
}

/// Edit the profile of `server`, or the main one, once nothing here is left
/// unsaved; that server's profile is read for its «Sobre mí» and banner.
fn edit_scope(ui: &mut Ui, store: &mut Store, server: Option<ServerId>) -> Vec<Intent> {
    let Some(editor) = ui.profiles.editor.as_mut() else {
        return Vec::new();
    };
    editor.servers_open = false;
    if editor.server == server {
        return Vec::new();
    }
    if editor.dirty(store) {
        editor.nudged = true;
        return Vec::new();
    }
    let picking = editor.picking.is_some();
    *editor = editor.on(server);
    let mut intents = if picking {
        vec![Intent::CancelProfileImage]
    } else {
        Vec::new()
    };
    match (server, store.me()) {
        (Some(server), Some(me)) => {
            intents.extend(fill_server_fields(store, server));
            intents.extend(request_profile(ui, store, me, Some(server)));
        }
        _ => intents.extend(store.me_user().map(fill_fields).unwrap_or_default()),
    }
    intents
}

/// Choose a picture of `kind` for the profile being edited (`pick`), or
/// take the one it has away. Nothing leaves until it is saved.
fn change_picture(ui: &mut Ui, store: &Store, kind: ProfileImageKind, pick: bool) -> Vec<Intent> {
    let Some(editor) = ui.profiles.editor.as_ref() else {
        return Vec::new();
    };
    let abilities = store.profile_abilities(editor.server);
    // A banner the plan does not allow is not chosen; taking one away is.
    let allowed = match kind {
        ProfileImageKind::Banner if pick => abilities.banner.allowed(),
        _ => abilities.images.allowed(),
    };
    if edit_denied(store).is_some() || editor.saving || editor.picking.is_some() || !allowed {
        return Vec::new();
    }
    if pick {
        let animated = crate::framing::may_move(store, editor.server, kind);
        let request = ui.fresh_request();
        if let Some(editor) = ui.profiles.editor.as_mut() {
            editor.picking = Some((request, kind, animated));
        }
        return vec![Intent::PickProfileImage {
            request,
            kind,
            animated,
        }];
    }
    if let Some(editor) = ui.profiles.editor.as_mut() {
        set_picture(editor, kind, None);
    }
    Vec::new()
}

/// Put `picture` (or none) in the draft of the profile being edited.
fn set_picture(
    editor: &mut Editor,
    kind: ProfileImageKind,
    picture: Option<(ProfileImage, limn::RasterImage)>,
) {
    let (image, preview) = picture.unzip();
    // A picture taken away is no longer framed.
    if image.is_none() && editor.framing.as_ref().is_some_and(|f| f.kind == kind) {
        editor.framing = None;
    }
    let scoped = editor.server.is_some();
    let (draft, shown) = match kind {
        ProfileImageKind::Avatar if scoped => {
            (&mut editor.server_edit.avatar, &mut editor.avatar_preview)
        }
        ProfileImageKind::Avatar => (&mut editor.edit.avatar, &mut editor.avatar_preview),
        ProfileImageKind::Banner if scoped => {
            (&mut editor.server_edit.banner, &mut editor.banner_preview)
        }
        // A server's banner is chosen by its settings (2026-10-19), never here.
        ProfileImageKind::Banner | ProfileImageKind::ServerBanner => {
            (&mut editor.edit.banner, &mut editor.banner_preview)
        }
    };
    *draft = Some(image);
    *shown = preview;
}

/// What came of choosing the picture asked for with `request`, or of
/// framing it again: with the editor still waiting for it, it joins the
/// draft and its preview shows; a new choice, with its whole picture
/// (`source`) to frame, is scrolled into view. A file that could not be
/// used says why; a cancelled choice changes nothing.
pub fn profile_image_picked(
    ui: &mut Ui,
    request: u64,
    result: Result<Option<(ProfileImage, limn::RasterImage)>, String>,
    source: Option<crate::PictureSource>,
    now: Instant,
) -> Vec<Intent> {
    // Server administration, rest (P7): a server's picture is chosen the same way.
    if let Some(intents) =
        crate::server_settings::rest::server_icon_picked(ui, request, &result, now)
    {
        return intents;
    }
    let Some(editor) = ui.profiles.editor.as_mut() else {
        return Vec::new();
    };
    let Some((_, kind, asked)) = editor.picking.filter(|(waiting, ..)| *waiting == request) else {
        return Vec::new();
    };
    editor.picking = None;
    // A frame of the picture being framed, rather than a new choice.
    let framed = editor
        .framing
        .as_mut()
        .and_then(|framing| framing.pending.take())
        .filter(|(waiting, _)| *waiting == request)
        .map(|(_, crop)| crop);
    match result {
        Ok(Some(picture)) => {
            let reveal = source.is_some();
            let animated = picture.0.animated();
            // Asked to move, a moving picture came back still: too much to.
            let heavy = |source: &crate::PictureSource| asked && source.animated && !animated;
            if let Some(source) = source {
                let heavy = heavy(&source);
                editor.framing = Some(crate::framing::Framing::new(kind, source, animated, heavy));
            } else if let (Some(framing), Some(crop)) = (editor.framing.as_mut(), framed) {
                framing.kept = crop;
                framing.animated = animated;
                framing.heavy |= heavy(&framing.source);
            } else {
                // A new choice the host could not show whole: nothing frames
                // it, and no earlier picture is framed in its place.
                editor.framing = None;
            }
            set_picture(editor, kind, Some(picture));
            if reveal {
                return vec![Intent::Reveal {
                    id: ids::PICTURES.into(),
                    margin: 16.0,
                }];
            }
        }
        Ok(None) => {}
        Err(reason) => {
            // A frame that failed: the one drawn goes back to the draft's.
            if let (Some(framing), Some(_)) = (editor.framing.as_mut(), framed) {
                framing.crop = framing.kept;
            }
            ui.notify(t("No se pudo usar la imagen"), Some(reason), true, now);
        }
    }
    Vec::new()
}

/// The account's profile in `server` was saved (`saved`) or arrived: unless
/// something is being written there, the editor shows it as it is now.
fn saved_in(ui: &mut Ui, store: &Store, server: ServerId, saved: bool) -> Vec<Intent> {
    let Some(editor) = ui
        .profiles
        .editor
        .as_mut()
        .filter(|editor| editor.server == Some(server))
    else {
        return Vec::new();
    };
    let keep = if editor.saving {
        !saved
    } else {
        editor.dirty(store)
    };
    if keep {
        return Vec::new();
    }
    *editor = editor.on(Some(server));
    fill_server_fields(store, server)
}

/// Someone's full profile, while open.
fn full_view(ui: &Ui, store: &Store, look: &Look) -> Option<FullProfile> {
    let (user, server) = ui.profiles.full?;
    let mut profile = card_of(ui, store, server, user, look)?;
    profile.full_action = None;
    let loaded = store.profile(user, server);
    let (mutual_servers, mutual_friends) = mutual_lists(ui, store, server, user, look);
    Some(FullProfile {
        loading: profile.loading,
        profile,
        server: server.and_then(|server| Some(store.server(server)?.shown_name().into())),
        nick: server
            .and_then(|server| store.member_of(server, user)?.nick.clone())
            .or_else(|| loaded?.nick.clone()),
        mutual_servers,
        mutual_friends,
        tab: ui.profiles.full_tab,
        connections: crate::identity::connections(store, user, server),
    })
}

/// The custom status dialog, or someone's full profile, over `root`.
pub fn overlays(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let root = match full_view(ui, store, look) {
        Some(full) => parts::full_profile(root, &full, context.window.width, look),
        None => root,
    };
    let Some(draft) = &ui.profiles.custom else {
        return root;
    };
    let Some(profile) = store.me().and_then(|me| card_of(ui, store, None, me, look)) else {
        return root;
    };
    parts::custom_status_dialog(
        root,
        &CustomStatusDialog {
            profile,
            emoji: draft.emoji.clone(),
            text: draft.text.clone(),
            emoji_open: draft.emoji_open,
            clear_after: draft.clear_after,
            expiry_open: draft.expiry_open,
            now: context.wall,
        },
        context.window.width,
        look,
    )
}
