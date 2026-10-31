//! Presence and visual identity (P5) as the window shows them: what a
//! person carries beside their name and over their face, the cards of
//! what they are doing with their times, where they are connected from,
//! their badges and connected accounts, and the pictures all that needs.

use std::collections::HashSet;
use std::time::Duration;

use limn::UiAction;
use rune_components::identity::{
    ActivityButton, ActivityCard, ActivityKind, ActivityTime, BadgeChip, CardEffect, CardIdentity,
    ConnectionRow, Marks, Plate, Platform, RoleMark, SpotifyCard, TagChip, TextPicture,
};
use rune_components::voice::GameMark;
use rune_components::Look;
use rune_i18n::t;
use rune_model::{Activity, RemoteImage, Role, ServerId, Timestamp, UserId};
use rune_provider::Command;
use rune_state::{Applied, Store};

use crate::{Intent, Ui};

/// The marks of `user` in `server` this device shows: a cosmetic hidden in
/// the settings is neither drawn nor asked for (2026-10-18).
pub(crate) fn identity(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
) -> rune_model::Identity {
    let mut identity = store.identity_in(server, user);
    let shown = ui.prefs.cosmetics;
    if !shown.decorations {
        identity.decoration = None;
    }
    if !shown.nameplates {
        identity.nameplate = None;
    }
    identity
}

/// The effect of `user`'s profile in `server`, unless effects are hidden.
fn effect<'a>(
    ui: &Ui,
    store: &'a Store,
    user: UserId,
    server: Option<ServerId>,
) -> Option<&'a rune_model::ProfileEffect> {
    store
        .profile(user, server)?
        .effect
        .as_ref()
        .filter(|_| ui.prefs.cosmetics.effects)
}

/// Whether `user`'s nameplate moves: their row is under the pointer or
/// their card is open, and motion is not reduced.
fn plate_moves(ui: &Ui, user: UserId) -> bool {
    !ui.reduced_motion && (ui.profiles.hovered == Some(user) || ui.profile == Some(user))
}

/// What `user` carries beside their name and over their face in `server`
/// (or outside any).
pub(crate) fn marks(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Marks {
    let identity = identity(ui, store, server, user);
    let top = server.and_then(|server| store.top_role(user, server));
    Marks {
        decoration: ui.picture(identity.decoration.as_ref()),
        gradient: top
            .filter(|_| ui.prefs.role_colors)
            .map(|role| {
                role.style
                    .gradient
                    .iter()
                    .map(|color| look.ink(*color))
                    .collect()
            })
            .unwrap_or_default(),
        role: server
            .and_then(|server| store.role_mark(user, server))
            .and_then(|role| role_mark(ui, role)),
        tag: identity.tag.map(|tag| TagChip {
            badge: ui.picture(tag.badge.as_ref()),
            server: store
                .server(tag.server)
                .map(|server| server.shown_name().into()),
            text: tag.tag,
        }),
        plate: identity.nameplate.map(|plate| Plate {
            image: ui.picture(Some(&plate.image)),
            colors: plate
                .colors
                .map(|(from, to)| (look.ink(from), look.ink(to))),
            // Once loaded, it shows under the pointer, or for good while
            // their card is open (2026-10-18).
            animated: ui
                .picture(plate.animated.as_ref())
                .filter(|_| !ui.reduced_motion),
            chosen: ui.profile == Some(user),
        }),
        mobile: store.presence(user).platforms.mobile_only(),
    }
}

/// A role's mark: its picture, else its emoji.
pub(crate) fn role_mark(ui: &Ui, role: &Role) -> Option<RoleMark> {
    match (&role.style.icon, &role.style.emoji) {
        (Some(icon), _) => Some(RoleMark::Image {
            role: role.name.clone(),
            image: ui.picture(Some(icon)),
        }),
        (None, Some(emoji)) => Some(RoleMark::Emoji {
            role: role.name.clone(),
            emoji: emoji.clone(),
        }),
        (None, None) => None,
    }
}

fn kind(kind: rune_model::ActivityKind) -> ActivityKind {
    match kind {
        rune_model::ActivityKind::Playing => ActivityKind::Playing,
        rune_model::ActivityKind::Listening => ActivityKind::Listening,
        rune_model::ActivityKind::Watching => ActivityKind::Watching,
        rune_model::ActivityKind::Streaming => ActivityKind::Streaming,
        rune_model::ActivityKind::Competing => ActivityKind::Competing,
    }
}

/// The id prefixes of an activity's buttons and Spotify track: on a profile,
/// and on a card beside a voice row (two cards can show the same person).
const PROFILE_ACTIVITY: &str = "activity:";
pub(crate) const LIVE_ACTIVITY: &str = "vz:live:activity:";

/// The card of `activity`, the `index`th of `user`'s presence, with its times
/// at `wall`; its buttons take ids under `prefix`.
pub(crate) fn activity_card(
    ui: &Ui,
    store: &Store,
    user: UserId,
    index: usize,
    activity: &Activity,
    wall: Timestamp,
    prefix: &str,
) -> ActivityCard {
    let seconds = |from: Timestamp, to: Timestamp| (to.0 - from.0) / 1000;
    let time = match (activity.started, activity.ends) {
        (Some(start), Some(end)) if activity.is_track() => Some(ActivityTime::Track {
            at: seconds(start, wall.min(end)),
            length: seconds(start, end),
            progress: activity.progress(wall).unwrap_or(0.0),
        }),
        // A countdown: an end and no start. Past its end it shows nothing.
        (None, Some(end)) => (end > wall).then(|| ActivityTime::Remaining(seconds(wall, end))),
        _ => activity
            .timer_start()
            .filter(|start| *start <= wall)
            .map(|start| ActivityTime::Elapsed(seconds(start, wall))),
    };
    // Without a large picture the small one takes its place, as the official
    // client shows it.
    let (large, small) = match (&activity.image, &activity.small_image) {
        (None, Some(small)) => (Some(small), None),
        (large, small) => (large.as_ref(), small.as_ref()),
    };
    // A button is asked of the session it runs in: without one it says why.
    let buttons = activity
        .buttons
        .iter()
        .enumerate()
        .map(|(button, label)| ActivityButton {
            label: label.clone(),
            id: format!("{prefix}{}:{index}:button:{button}", user.0),
            enabled: match activity.session {
                Some(_) => Ok(()),
                None => Err(t("No disponible en esta conexión.").to_owned()),
            },
        })
        .collect();
    ActivityCard {
        kind: kind(activity.kind),
        name: activity.name.clone(),
        detail: activity.detail.clone(),
        state: activity.artists().or_else(|| activity.state.clone()),
        party: activity.party.as_ref().and_then(|party| party.size),
        spotify: activity.is_spotify().then(|| SpotifyCard {
            id: format!("{prefix}{}:{index}:spotify", user.0),
            open: spotify_open(store, activity),
        }),
        large_text: activity.large_text.clone(),
        image: ui.picture(large),
        small_image: ui.picture(small),
        time,
        buttons,
    }
}

/// Where a Spotify track's button goes: its page, or why it cannot go there.
fn spotify_open(store: &Store, activity: &Activity) -> Result<String, String> {
    if store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == rune_provider::ProviderKind::Simulated)
    {
        return Err(t("Simulado: no abre Spotify").to_owned());
    }
    activity
        .spotify_track_url()
        .ok_or_else(|| t("No disponible en esta conexión.").to_owned())
}

/// The cards of everything `user` does, with their times at `wall`.
pub(crate) fn activity_cards(
    ui: &Ui,
    store: &Store,
    user: UserId,
    wall: Timestamp,
) -> Vec<ActivityCard> {
    let presence = store.presence(user);
    presence
        .activities
        .iter()
        .enumerate()
        .map(|(index, activity)| {
            activity_card(ui, store, user, index, activity, wall, PROFILE_ACTIVITY)
        })
        .collect()
}

/// What a voice card lists of `activities`, with their places: a game or a
/// watch that has something to show, and every listening; an application
/// once. Competing and streams are not listed, as Discord's list.
pub(crate) fn listed(activities: &[Activity]) -> Vec<(usize, &Activity)> {
    let mut seen = HashSet::new();
    let mut listed = Vec::new();
    for (index, activity) in activities.iter().enumerate() {
        let shown = match activity.kind {
            rune_model::ActivityKind::Listening => true,
            rune_model::ActivityKind::Playing | rune_model::ActivityKind::Watching => {
                !activity.name.is_empty()
                    || activity.application.is_some()
                    || activity.image.is_some()
                    || activity.small_image.is_some()
                    || activity.state.is_some()
                    || activity.detail.is_some()
                    || activity.party.is_some()
            }
            rune_model::ActivityKind::Competing | rune_model::ActivityKind::Streaming => false,
        };
        let key = match activity.application {
            Some(application) => format!("app:{application}"),
            None => format!("name:{}", activity.name),
        };
        if shown && seen.insert(key) {
            listed.push((index, activity));
        }
    }
    listed
}

/// The game `user` plays, with its picture, for the mark at the end of their
/// row in voice; `None` while it has no picture to show.
pub(crate) fn game_mark(ui: &Ui, store: &Store, user: UserId) -> Option<GameMark> {
    let (name, picture) = game(store, user)?;
    Some(GameMark {
        name,
        image: ui.picture(Some(&picture))?,
    })
}

/// The first activity `user` plays, with the picture it has (the session
/// fills in its application's when it has none of its own).
fn game(store: &Store, user: UserId) -> Option<(String, RemoteImage)> {
    let presence = store.presence(user);
    let activity = presence
        .activities
        .iter()
        .find(|activity| activity.kind == rune_model::ActivityKind::Playing)?;
    let picture = activity
        .image
        .clone()
        .or_else(|| activity.small_image.clone())?;
    Some((activity.name.clone(), picture))
}

/// The person a voice card's id names: `vz:live:{kind}:{user}…`.
fn live_user(id: &str) -> Option<u64> {
    let mut parts = id.strip_prefix("vz:live:")?.split(':');
    parts.next()?;
    parts.next()?.parse().ok()
}

/// What `user`'s card shows beyond who they are, as seen in `server`;
/// `roles` are the card's own, in order.
pub(crate) fn card(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    roles: &[&Role],
    look: &Look,
) -> CardIdentity {
    let presence = store.presence(user);
    let platforms = [
        (presence.platforms.desktop, Platform::Desktop),
        (presence.platforms.mobile, Platform::Mobile),
        (presence.platforms.web, Platform::Web),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, place)| place)
    .collect();
    let badges = store
        .profile(user, server)
        .and_then(|loaded| loaded.badges.as_ref())
        .map(|badges| {
            badges
                .iter()
                .map(|badge| BadgeChip {
                    description: badge.description.clone(),
                    image: ui.picture(badge.icon.as_ref()),
                    link: badge.link.is_some(),
                })
                .collect()
        })
        .unwrap_or_default();
    CardIdentity {
        // Nothing of it until all of its pictures arrived, as the official
        // client waits; then what shows at this moment of it.
        effect: effect(ui, store, user, server)
            .filter(|effect| {
                effect_sources(ui, effect)
                    .iter()
                    .all(|source| ui.images.contains_key(*source))
            })
            .map(|effect| CardEffect {
                layers: effect_showing(ui, effect, effect_elapsed(ui, store, user, server))
                    .into_iter()
                    .filter_map(|(source, at)| Some((ui.picture(Some(source))?, at)))
                    .collect(),
            }),
        marks: Marks {
            role: None,
            plate: None,
            mobile: false,
            ..marks(ui, store, server, user, look)
        },
        activities: activity_cards(ui, store, user, store.wall()),
        platforms,
        badges,
        role_marks: roles.iter().map(|role| role_mark(ui, role)).collect(),
        pictures: store
            .profile(user, server)
            .map(|loaded| {
                loaded
                    .pictures
                    .iter()
                    .map(|picture| text_picture(ui, picture))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// A picture a text names, as its component draws it: playing while the
/// host plays its animation.
pub(crate) fn text_picture(ui: &Ui, picture: &rune_model::TextPicture) -> TextPicture {
    let image = ui.picture(Some(&picture.image));
    TextPicture {
        token: picture.token.clone(),
        name: picture.name.clone(),
        playing: image
            .as_ref()
            .is_some_and(|image| ui.animated.contains(image.sha256())),
        image,
    }
}

/// What a list says under someone's name: what they do first, else their
/// custom status with its server's emoji (2026-10-18: the members, friends
/// and conversations lists left the emoji out; the conversations, also
/// what they do). Someone the account ignores shows no activity, only the
/// status.
pub(crate) fn status_line(
    ui: &Ui,
    presence: &rune_model::Presence,
    ignored: bool,
) -> (Option<String>, Option<TextPicture>) {
    match presence.activity().filter(|_| !ignored) {
        Some(activity) => (Some(rune_components::look::activity_line(activity)), None),
        None => (
            presence.custom.clone(),
            presence
                .custom_emoji
                .as_ref()
                .map(|emoji| text_picture(ui, emoji)),
        ),
    }
}

/// The server emoji a message's text names, as pictures of a text shown
/// outside the timeline, such as a pinned message (2026-10-18).
pub(crate) fn reference_pictures(
    ui: &Ui,
    references: &[rune_model::TextReference],
) -> Vec<TextPicture> {
    references
        .iter()
        .filter_map(|reference| match &reference.target {
            rune_model::TextTarget::Emoji {
                name,
                image: Some(image),
                ..
            } => Some(text_picture(
                ui,
                &rune_model::TextPicture {
                    token: reference.source.clone(),
                    name: name.clone(),
                    image: image.clone(),
                },
            )),
            _ => None,
        })
        .collect()
}

/// The pictures a text names, as its component draws them.
pub(crate) fn text_pictures(ui: &Ui, pictures: &[rune_model::TextPicture]) -> Vec<TextPicture> {
    pictures
        .iter()
        .map(|picture| text_picture(ui, picture))
        .collect()
}

/// Every picture of an effect: its layers, or its still while motion is
/// reduced.
fn effect_sources<'a>(ui: &Ui, effect: &'a rune_model::ProfileEffect) -> Vec<&'a RemoteImage> {
    if ui.reduced_motion {
        return effect.still.iter().collect();
    }
    effect.layers.iter().map(|layer| &layer.image).collect()
}

/// The pictures of an effect showing `elapsed` milliseconds after its card
/// opened, and where each one's top left goes: the layers in their time,
/// or the still while motion is reduced.
fn effect_showing<'a>(
    ui: &Ui,
    effect: &'a rune_model::ProfileEffect,
    elapsed: i64,
) -> Vec<(&'a RemoteImage, limn::Point)> {
    if ui.reduced_motion {
        return effect
            .still
            .iter()
            .map(|still| (still, limn::Point::new(0.0, 0.0)))
            .collect();
    }
    effect
        .layers
        .iter()
        .filter(|layer| layer.shows(elapsed))
        .map(|layer| {
            let at = limn::Point::new(layer.x as f32, layer.y as f32);
            (&layer.image, at)
        })
        .collect()
}

/// How long the card of `user` in `server` has been shown, in milliseconds:
/// the time of its effect (0 the frame it opens).
pub(crate) fn effect_elapsed(
    ui: &Ui,
    store: &Store,
    user: UserId,
    server: Option<ServerId>,
) -> i64 {
    ui.profiles
        .effect_since
        .filter(|(shown, at, _)| *shown == user && *at == server)
        .map_or(0, |(_, _, since)| (store.wall().0 - since.0).max(0))
}

/// After a layout: the clock of the effect of a card shown starts as it
/// opens, and stops as it closes; the presence of whoever it shows in a
/// server is kept current while it shows, as the official client
/// subscribes its popouts' members (a voice channel's people may be in no
/// row of the list).
pub(crate) fn after_layout(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let now = shown(ui, store);
    let same = matches!(
        (now, ui.profiles.effect_since),
        (Some((user, server)), Some((kept, at, _))) if user == kept && server == at
    );
    if !same {
        ui.profiles.effect_since = now.map(|(user, server)| (user, server, store.wall()));
    }
    let want = now
        .filter(|(user, _)| store.me() != Some(*user))
        .and_then(|(user, server)| Some((server?, vec![user])));
    if want == ui.profiles.people_watch {
        return Vec::new();
    }
    let kept = std::mem::replace(&mut ui.profiles.people_watch, want.clone());
    match want.or(kept.map(|(server, _)| (server, Vec::new()))) {
        Some((server, people)) => vec![Intent::Command(rune_provider::Command::WatchPeople {
            server,
            people,
        })],
        None => Vec::new(),
    }
}

/// After a layout: whose marks play because the pointer is over them (see
/// [`playing`]).
pub(crate) fn hold_hovered(ui: &mut Ui, store: &Store, tree: &limn::UiTree) {
    ui.profiles.hovered = hovered(ui, store, tree);
}

/// Who is under the pointer: the author of the message whose row it is
/// over (its face, its name, its text, a mention in it: Discord animates
/// nothing for a mention), a row of the member list, or someone in voice.
/// A system message's row shows no face, so it plays nothing.
fn hovered(ui: &Ui, store: &Store, tree: &limn::UiTree) -> Option<UserId> {
    let row = tree
        .hovered_region()
        .and_then(|id| id.strip_prefix("msg:")?.strip_suffix(":row")?.parse().ok());
    if let Some(message) = row {
        let message = store
            .timeline(ui.channel()?)?
            .get(rune_model::MessageId(message))?;
        return message.kind.is_said().then_some(message.author);
    }
    let id = tree.hovered_id()?;
    let number = |prefix: &str| id.strip_prefix(prefix)?.parse::<u64>().ok();
    // A mention elsewhere shares the row's ID: only the list's rows count.
    let member = number("member:").filter(|_| crate::profile::member_row(tree, id).is_some());
    member
        .or_else(|| number("vz:user:"))
        .or_else(|| number("vz:tile:"))
        .or_else(|| live_user(id))
        .map(UserId)
}

/// The accounts elsewhere `user` shows, once their profile is loaded.
pub(crate) fn connections(
    store: &Store,
    user: UserId,
    server: Option<ServerId>,
) -> Vec<ConnectionRow> {
    store
        .profile(user, server)
        .and_then(|loaded| loaded.connections.as_ref())
        .map(|accounts| {
            accounts
                .iter()
                .map(|account| ConnectionRow {
                    service: account.service.clone(),
                    label: account.label.clone(),
                    name: account.name.clone(),
                    verified: account.verified,
                    link: account.link.is_some(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The profile shown now, if any: the full one, else a member's, an
/// author's or a voice channel's card, else the account's own popout; with
/// where it is seen.
pub(crate) fn shown(ui: &Ui, store: &Store) -> Option<(UserId, Option<ServerId>)> {
    if let Some(full) = ui.profiles.full {
        return Some(full);
    }
    if let Some(user) = ui.profile {
        return Some((user, ui.server));
    }
    if let Some((message, _)) = ui.profiles.author {
        let author = store.timeline(ui.channel()?)?.get(message)?.author;
        return Some((author, ui.server));
    }
    if let Some((user, channel, _)) = ui.voice.card {
        return Some((user, store.channel(channel)?.server));
    }
    ui.status_menu
        .then(|| store.me().map(|me| (me, None)))
        .flatten()
}

/// The pictures of the profile shown: activities, badges and marks.
pub(crate) fn wanted(ui: &Ui, store: &Store, add: &mut dyn FnMut(Option<&RemoteImage>)) {
    // The pictures of the card beside the row under the pointer, and of the
    // games played in voice (their marks at the end of the rows).
    if let Some(user) = ui.profiles.hovered {
        let presence = store.presence(user);
        for (_, activity) in listed(&presence.activities) {
            add(activity.image.as_ref());
            add(activity.small_image.as_ref());
        }
    }
    for voice in store.voice_by_server().values() {
        for state in &voice.people {
            if let Some((_, picture)) = game(store, state.user) {
                add(Some(&picture));
            }
        }
    }
    let Some((user, server)) = shown(ui, store) else {
        return;
    };
    let presence = store.presence(user);
    for activity in &presence.activities {
        add(activity.image.as_ref());
        add(activity.small_image.as_ref());
    }
    add(presence.custom_emoji.as_ref().map(|emoji| &emoji.image));
    if let Some(badges) = store
        .profile(user, server)
        .and_then(|loaded| loaded.badges.as_ref())
    {
        for badge in badges {
            add(badge.icon.as_ref());
        }
    }
    face_marks(ui, store, server, user, false, add);
    if let Some(effect) = effect(ui, store, user, server) {
        for source in effect_sources(ui, effect) {
            add(Some(source));
        }
    }
    for picture in store
        .profile(user, server)
        .map_or(&[][..], |loaded| &loaded.pictures)
    {
        add(Some(&picture.image));
    }
    if let (Some(server), Some(member)) = (
        server,
        server.and_then(|server| store.member_of(server, user)),
    ) {
        for role in store
            .server(server)
            .into_iter()
            .flat_map(|server| &server.roles)
            .filter(|role| member.roles.contains(&role.id))
        {
            add(role.style.icon.as_ref());
        }
    }
}

/// The pictures beside `user`'s name and over their face in `server`, and
/// with `plate` the nameplate a row of the member list draws behind them.
pub(crate) fn face_marks(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    plate: bool,
    add: &mut dyn FnMut(Option<&RemoteImage>),
) {
    let identity = identity(ui, store, server, user);
    add(identity.decoration.as_ref());
    add(identity.tag.as_ref().and_then(|tag| tag.badge.as_ref()));
    if let Some(nameplate) = identity.nameplate.as_ref().filter(|_| plate) {
        add(Some(&nameplate.image));
        // The moving one only while it may move: under the pointer or chosen.
        if plate_moves(ui, user) {
            add(nameplate.animated.as_ref());
        }
    }
    if let Some(role) = server.and_then(|server| store.role_mark(user, server)) {
        add(role.style.icon.as_ref());
    }
}

/// When a card shown next changes by itself: each second while it shows a
/// time that moves (the profile shown, or the card beside the row under the
/// pointer, when its person is in voice), and when a layer of its effect
/// comes or goes.
pub fn activity_tick(ui: &Ui, store: &Store) -> Option<Duration> {
    let timed = |user: UserId| {
        store
            .presence(user)
            .activities
            .iter()
            .any(|activity| activity.started.is_some() || activity.ends.is_some())
    };
    let listed_timed = |user: UserId| {
        let presence = store.presence(user);
        listed(&presence.activities)
            .iter()
            .any(|(_, activity)| activity.started.is_some() || activity.ends.is_some())
    };
    let shown = shown(ui, store);
    let hovered = ui
        .profiles
        .hovered
        .filter(|user| store.voice_state_of(*user).is_some() && listed_timed(*user));
    let second = (hovered.is_some() || shown.is_some_and(|(user, _)| timed(user)))
        .then(|| 1000 - store.wall().0.rem_euclid(1000));
    let layer = shown.and_then(|(user, server)| {
        let elapsed = effect_elapsed(ui, store, user, server);
        effect(ui, store, user, server)
            .filter(|_| !ui.reduced_motion)
            .and_then(|effect| {
                effect
                    .layers
                    .iter()
                    .filter_map(|layer| layer.changes_in(elapsed))
                    .min()
            })
    });
    [second, layer]
        .into_iter()
        .flatten()
        .min()
        .map(|ms| Duration::from_millis(ms.max(1) as u64))
}

/// What an activity's card asks for: a rich-presence button, by its number,
/// or the Spotify track.
enum ActivityAction {
    Button(u8),
    Spotify,
}

/// The person, the activity's place and what its card asks for, from the id
/// the card gave it.
fn activity_action(id: &str) -> Option<(UserId, usize, ActivityAction)> {
    let rest = id
        .strip_prefix(PROFILE_ACTIVITY)
        .or_else(|| id.strip_prefix(LIVE_ACTIVITY))?;
    let mut parts = rest.split(':');
    let user = UserId(parts.next()?.parse().ok()?);
    let index = parts.next()?.parse().ok()?;
    let action = match parts.next()? {
        "spotify" => ActivityAction::Spotify,
        "button" => ActivityAction::Button(parts.next()?.parse().ok()?),
        _ => return None,
    };
    Some((user, index, action))
}

/// A badge's or a connected account's link on the profile shown, or a button
/// or Spotify track of an activity on a card.
pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    use rune_components::identity::ids;
    let UiAction::Clicked { id } = action else {
        return None;
    };
    if let Some((user, index, what)) = activity_action(id) {
        return Some(activity_pressed(ui, store, user, index, what));
    }
    let (user, server) = shown(ui, store)?;
    let loaded = store.profile(user, server);
    let link = if let Some(index) = id.strip_prefix(ids::BADGE) {
        let index: usize = index.parse().ok()?;
        loaded?.badges.as_ref()?.get(index)?.link.clone()
    } else if let Some(index) = id.strip_prefix(ids::CONNECTION) {
        let index: usize = index.parse().ok()?;
        loaded?.connections.as_ref()?.get(index)?.link.clone()
    } else {
        return None;
    };
    Some(link.map(Intent::OpenUrl).into_iter().collect())
}

/// A button asks its app for its link, which comes back to `on_applied`; the
/// Spotify track's page goes through the link flow.
fn activity_pressed(
    ui: &mut Ui,
    store: &Store,
    user: UserId,
    index: usize,
    what: ActivityAction,
) -> Vec<Intent> {
    let presence = store.presence(user);
    let Some(activity) = presence.activities.get(index) else {
        return Vec::new();
    };
    match what {
        ActivityAction::Button(button) => {
            // Without a session the button is disabled, so none is asked for.
            let Some(session) = activity.session.clone() else {
                return Vec::new();
            };
            let request = ui.fresh_request();
            vec![Intent::Command(Command::OpenActivityButton {
                request,
                user,
                session,
                application: activity.application,
                index: button,
            })]
        }
        ActivityAction::Spotify => match spotify_open(store, activity) {
            Ok(url) => crate::message_actions::links::external(ui, store, &url, false),
            Err(_) => Vec::new(),
        },
    }
}

/// A button's link arrived: it opens through the link flow, which asks first.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    let Applied::ActivityButtonUrl { url, .. } = applied else {
        return Vec::new();
    };
    crate::message_actions::links::external(ui, store, url, false)
}

/// What may play now: the decoration and nameplate of whoever is under the
/// pointer, in the member list, a message or a voice tile, and of whoever's
/// card is open, as the official client animates them on hover (2026-10-18:
/// only the profile shown played); then the profile shown (P5): its
/// decoration and its effect's layers.
pub(crate) fn playing(ui: &Ui, store: &Store) -> Vec<RemoteImage> {
    let mut playing = Vec::new();
    for user in [ui.profiles.hovered, ui.profile].into_iter().flatten() {
        let identity = identity(ui, store, ui.server, user);
        playing.extend(identity.decoration);
        playing.extend(identity.nameplate.and_then(|plate| plate.animated));
    }
    let Some((user, server)) = shown(ui, store) else {
        return playing;
    };
    playing.extend(identity(ui, store, server, user).decoration);
    // Only the layers showing: one that comes back starts over, as the
    // official client restarts it.
    if let Some(effect) = effect(ui, store, user, server) {
        let elapsed = effect_elapsed(ui, store, user, server);
        playing.extend(
            effect_showing(ui, effect, elapsed)
                .into_iter()
                .map(|(source, _)| source.clone()),
        );
    }
    // The emoji of their bio and of their custom status that move (2026-10-17).
    playing.extend(store.presence(user).custom_emoji.map(|emoji| emoji.image));
    playing.extend(
        store
            .profile(user, server)
            .into_iter()
            .flat_map(|loaded| &loaded.pictures)
            .map(|picture| picture.image.clone()),
    );
    playing
}
