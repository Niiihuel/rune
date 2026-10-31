//! The «Activo ahora» column of the friends page (Discord's «Active Now»): its
//! cards, built from the store, the menu of a card and the column's clicks.
//! A card is one group of friends: the people in a voice channel the account
//! can see, else in one game party, else one person. Its people join the
//! card, and so do the others in its channel.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use limn::{Element, UiAction};
use rune_components::active_now::{
    self as parts, ids, ActiveCard, ActivityView, HeaderMark, Person, StreamView, VoiceView,
};
use rune_components::voice::{self as voice_parts, MenuEntry};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{
    Activity, ActivityKind, ChannelId, ChannelKind, RelationshipKind, ServerId, Timestamp, UserId,
};
use rune_state::{Permit, Store};

use crate::{Intent, Ui, ViewContext};

/// The column's own state: the card whose menu is open, by its group's key.
#[derive(Debug, Default)]
pub struct ActiveNow {
    pub menu: Option<String>,
}

/// The column beside the friends page, `page` wide, or `None` where the
/// window is too narrow for it.
pub(crate) fn column(
    ui: &Ui,
    store: &Store,
    look: &Look,
    context: &ViewContext,
    page: f32,
) -> Option<Element> {
    if !parts::shown_in(context.window.width) {
        return None;
    }
    let cards = cards(ui, store, context.wall);
    Some(parts::column(&cards, page, context.window.height, look))
}

/// When the column changes next: each second while someone's activity runs a
/// timer, else every 10 s, for the minutes the subtitles show.
pub fn tick(ui: &Ui, store: &Store) -> Option<Duration> {
    if !crate::relationships::visible(ui) {
        return None;
    }
    let timed = candidates(store).into_iter().any(|user| {
        store
            .presence(user)
            .activities
            .first()
            .is_some_and(|activity| {
                activity.kind == ActivityKind::Playing
                    && !activity.is_spotify()
                    && (activity.ends.is_some() || activity.timer_start().is_some())
            })
    });
    let period: i64 = if timed { 1_000 } else { 10_000 };
    let wall = store.wall().0;
    Some(Duration::from_millis(
        (period - wall.rem_euclid(period)) as u64,
    ))
}

/// The friends the column may show: no bots, and not the account.
fn candidates(store: &Store) -> Vec<UserId> {
    let me = store.me();
    store
        .relationships()
        .unwrap_or_default()
        .into_iter()
        .filter(|known| known.kind == RelationshipKind::Friend)
        .map(|known| known.user)
        .filter(|user| Some(*user) != me && !store.user(*user).is_some_and(|account| account.bot))
        .collect()
}

/// The voice channel `user` is in, when the account sees it. A stage is not
/// one here: the column has no wording for its audience.
fn shown_voice(store: &Store, user: UserId) -> Option<ChannelId> {
    let channel = store.voice_state_of(user)?.channel;
    store
        .channel(channel)
        .is_some_and(|found| found.kind == ChannelKind::Voice && Store::visible(found))
        .then_some(channel)
}

/// The key of the card `user` is in: their voice channel, else their first
/// activity's party, else themselves.
fn group_key(store: &Store, user: UserId) -> String {
    let party = store
        .presence(user)
        .activities
        .first()
        .and_then(|activity| activity.party.as_ref())
        .and_then(|party| party.id.clone());
    key_of(shown_voice(store, user), party, user)
}

/// The key of a group from what its person shares: the voice channel, else the
/// game party, else the person alone.
fn key_of(channel: Option<ChannelId>, party: Option<String>, user: UserId) -> String {
    match (channel, party) {
        (Some(channel), _) => format!("channel:{}", channel.0),
        (None, Some(party)) => format!("party:{party}"),
        (None, None) => format!("user:{}", user.0),
    }
}

/// Where a card goes in the column; `false` sorts first. Several people, then
/// streams, then voice, then activities; Spotify last; then by the names.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Rank {
    alone: bool,
    no_stream: bool,
    no_voice: bool,
    no_activity: bool,
    spotify: bool,
    names: String,
}

/// The cards of the column now, in their order.
pub(crate) fn cards(ui: &Ui, store: &Store, wall: Timestamp) -> Vec<ActiveCard> {
    let mut groups: BTreeMap<String, Vec<UserId>> = BTreeMap::new();
    for user in candidates(store) {
        groups.entry(group_key(store, user)).or_default().push(user);
    }
    let mut built: Vec<(Rank, ActiveCard)> = groups
        .into_iter()
        .filter_map(|(key, friends)| card(ui, store, wall, key, &friends))
        .collect();
    built.sort_by(|a, b| a.0.cmp(&b.0));
    built.into_iter().map(|(_, card)| card).collect()
}

/// The card of one group, or `None` when it has nothing to show, or a person
/// in it is blocked or ignored.
fn card(
    ui: &Ui,
    store: &Store,
    wall: Timestamp,
    key: String,
    friends: &[UserId],
) -> Option<(Rank, ActiveCard)> {
    let me = store.me();
    let mut channels: Vec<ChannelId> = Vec::new();
    for user in friends {
        if let Some(channel) = shown_voice(store, *user) {
            if !channels.contains(&channel) {
                channels.push(channel);
            }
        }
    }
    // Everyone in those channels is in the card too, the account aside.
    let mut members = friends.to_vec();
    for channel in &channels {
        for state in store.voice_in(*channel) {
            if Some(state.user) != me && !members.contains(&state.user) {
                members.push(state.user);
            }
        }
    }
    if members
        .iter()
        .any(|user| store.blocked(*user) || store.is_ignored(*user))
    {
        return None;
    }
    // One activity per person, the first, once per application; newest first.
    let mut found: Vec<(Activity, Vec<UserId>)> = Vec::new();
    for user in &members {
        let Some(activity) = store.presence(*user).activities.into_iter().next() else {
            continue;
        };
        match found
            .iter_mut()
            .find(|(known, _)| same_activity(known, &activity))
        {
            Some((_, who)) => who.push(*user),
            None => found.push((activity, vec![*user])),
        }
    }
    if channels.is_empty() && found.is_empty() {
        return None;
    }
    found.sort_by(|(a, _), (b, _)| {
        b.timer_start()
            .cmp(&a.timer_start())
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut people: Vec<Person> = members
        .iter()
        .map(|user| person(ui, store, *user))
        .collect();
    // Only the header's person shows a status, and only when presence is told.
    people[0].status = store
        .friends_presence()
        .then(|| store.presence(members[0]).status);
    let voice: Vec<VoiceView> = channels
        .iter()
        .filter_map(|channel| voice_view(ui, store, *channel, me))
        .collect();
    let mut streams = Vec::new();
    for (index, user) in members.iter().enumerate() {
        let Some(state) = store.voice_state_of(*user) else {
            continue;
        };
        if !state.streaming || !channels.contains(&state.channel) {
            continue;
        }
        let game = store
            .presence(*user)
            .activities
            .into_iter()
            .next()
            .filter(|activity| activity.kind == ActivityKind::Playing && !activity.is_spotify());
        streams.push(StreamView {
            channel: state.channel,
            person: people[index].clone(),
            line: match game {
                Some(game) => tr!("Transmitiendo {name}", name = game.name.as_str()),
                None => t("Compartiendo su pantalla").into(),
            },
            preview: store
                .stream_preview(state.channel, *user)
                .flatten()
                .and_then(|image| ui.picture(Some(image))),
        });
    }
    let activities: Vec<ActivityView> = found
        .iter()
        .filter_map(|(activity, who)| activity_view(ui, store, activity, who, wall))
        .collect();
    let (subtitle, mark) = header(ui, members.len(), &found, !channels.is_empty(), wall);
    let rank = Rank {
        alone: members.len() <= 1,
        no_stream: streams.is_empty(),
        no_voice: voice.is_empty(),
        no_activity: found.is_empty(),
        spotify: found.iter().any(|(activity, _)| activity.is_spotify()),
        names: people
            .iter()
            .map(|person| person.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(" "),
    };
    let menu_open = ui.active_now.menu.as_deref() == Some(key.as_str());
    let menu = if menu_open {
        card_menu(store, &members, &channels)
    } else {
        Vec::new()
    };
    let card = ActiveCard {
        title: title(&people),
        members: people,
        subtitle,
        mark,
        voice,
        streams,
        activities,
        menu,
        menu_open,
        key,
    };
    Some((rank, card))
}

/// Whether two activities are the same thing: one application, else one name.
fn same_activity(a: &Activity, b: &Activity) -> bool {
    match (a.application, b.application) {
        (Some(x), Some(y)) => x == y,
        _ => a.name == b.name,
    }
}

/// A person as the column draws them: their name, their color and picture.
fn person(ui: &Ui, store: &Store, user: UserId) -> Person {
    let account = store.user(user);
    Person {
        user,
        name: name_of(store, user),
        accent: account.map(|account| account.accent),
        image: ui.picture(account.and_then(|account| account.picture())),
        status: None,
    }
}

/// How the column names someone: a friend's private nickname, else their name.
fn name_of(store: &Store, user: UserId) -> String {
    store
        .relationship(user)
        .and_then(|known| known.nickname.clone())
        .or_else(|| {
            store
                .user(user)
                .map(|account| account.shown_name().to_owned())
        })
        .unwrap_or_else(|| t("Usuario desconocido").into())
}

fn voice_view(ui: &Ui, store: &Store, channel: ChannelId, me: Option<UserId>) -> Option<VoiceView> {
    let info = store.channel(channel)?;
    let server = info.server.and_then(|id| store.server(id));
    Some(VoiceView {
        server: info.server,
        server_name: server.map_or_else(
            || info.name.clone(),
            |server| server.shown_name().to_owned(),
        ),
        initials: server.map(|server| server.initials()).unwrap_or_default(),
        server_image: server.and_then(|server| ui.image(&server.icon)),
        channel,
        channel_name: info.name.clone(),
        members: store
            .voice_in(channel)
            .into_iter()
            .filter(|state| Some(state.user) != me)
            .map(|state| person(ui, store, state.user))
            .collect(),
    })
}

/// What the card shows of one activity, from the people who have it; `None`
/// for a kind the column draws nothing for.
fn activity_view(
    ui: &Ui,
    store: &Store,
    activity: &Activity,
    who: &[UserId],
    wall: Timestamp,
) -> Option<ActivityView> {
    let people = || -> Vec<Person> { who.iter().map(|user| person(ui, store, *user)).collect() };
    if activity.is_spotify() {
        return Some(ActivityView::Spotify {
            art: ui.picture(activity.image.as_ref()),
            track: activity
                .detail
                .clone()
                .unwrap_or_else(|| activity.name.clone()),
            artists: activity.artists(),
            listeners: people(),
        });
    }
    let rich = activity.detail.is_some()
        || activity.state.is_some()
        || activity.image.is_some()
        || activity.small_image.is_some()
        || activity.party.is_some();
    match activity.kind {
        ActivityKind::Playing | ActivityKind::Listening if rich => Some(ActivityView::Rich {
            image: ui.picture(activity.image.as_ref()),
            small_image: ui.picture(activity.small_image.as_ref()),
            title: activity
                .detail
                .clone()
                .unwrap_or_else(|| activity.name.clone()),
            state: activity.state.clone(),
            timer: timer(activity, wall),
        }),
        ActivityKind::Playing => {
            let count = who.len();
            Some(ActivityView::Plain {
                name: activity.name.clone(),
                count: if count == 1 {
                    tr!("{n} persona", n = count)
                } else {
                    tr!("{n} personas", n = count)
                },
                people: people(),
            })
        }
        _ => None,
    }
}

/// A game's timer: «Restantes» until its end while that is ahead, else
/// «Transcurrido» since it started. Only for a game (not music).
fn timer(activity: &Activity, wall: Timestamp) -> Option<String> {
    if activity.kind != ActivityKind::Playing {
        return None;
    }
    if let Some(ends) = activity.ends.filter(|ends| ends.0 > wall.0) {
        return Some(tr!(
            "Restantes: {duration}",
            duration = clock(ends.0 - wall.0)
        ));
    }
    let start = activity.timer_start()?;
    Some(tr!(
        "Transcurrido: {duration}",
        duration = clock(wall.0 - start.0)
    ))
}

/// A span of milliseconds as the column shows it: `mm:ss`, `h:mm:ss`.
fn clock(millis: i64) -> String {
    voice_parts::clock(Duration::from_millis(millis.max(0) as u64))
}

/// The card's title: its first person, two, or the first two and a count.
fn title(people: &[Person]) -> String {
    match people {
        [] => String::new(),
        [only] => only.name.clone(),
        [first, second] => tr!(
            "{first} y {second}",
            first = first.name.as_str(),
            second = second.name.as_str()
        ),
        [first, second, rest @ ..] => {
            let others = rest.len();
            if others == 1 {
                tr!(
                    "{first}, {second} y {n} otro",
                    first = first.name.as_str(),
                    second = second.name.as_str(),
                    n = others
                )
            } else {
                tr!(
                    "{first}, {second} y {n} otros",
                    first = first.name.as_str(),
                    second = second.name.as_str(),
                    n = others
                )
            }
        }
    }
}

/// The subtitle and the header's mark: what one person with one activity is
/// doing, an activity's time for several people, «En un canal de voz», or
/// «Jugando varios juegos» for two or more activities.
fn header(
    ui: &Ui,
    members: usize,
    found: &[(Activity, Vec<UserId>)],
    voice: bool,
    wall: Timestamp,
) -> (Option<String>, HeaderMark) {
    let mark = |activity: &Activity| match ui.picture(activity.image.as_ref()) {
        Some(image) => HeaderMark::Picture(image),
        None => HeaderMark::None,
    };
    match found {
        [(activity, _)] if members == 1 => (Some(one_activity(activity, wall)), mark(activity)),
        [(activity, _)] => (
            Some(since(&activity.name, activity.timer_start(), wall)),
            mark(activity),
        ),
        [] if voice => (Some(t("En un canal de voz").into()), HeaderMark::None),
        [_, _, ..] => (
            Some(t("Jugando varios juegos").into()),
            HeaderMark::Controller,
        ),
        [] => (None, HeaderMark::None),
    }
}

/// The subtitle of one person with one activity, by what it is.
fn one_activity(activity: &Activity, wall: Timestamp) -> String {
    let name = activity.name.as_str();
    match activity.kind {
        ActivityKind::Playing => since(name, activity.timer_start(), wall),
        ActivityKind::Listening if activity.is_spotify() => t("Escuchando Spotify").into(),
        ActivityKind::Listening => tr!("Escuchando {name}", name = name),
        ActivityKind::Watching => tr!("Viendo {name}", name = name),
        ActivityKind::Streaming => tr!("Transmitiendo {name}", name = name),
        ActivityKind::Competing => tr!("Compitiendo en {name}", name = name),
    }
}

/// «{name} – {n} min» (or h, or d) since `start`, «{name} - justo ahora» under
/// a minute; just the name without a start.
fn since(name: &str, start: Option<Timestamp>, wall: Timestamp) -> String {
    let Some(start) = start else {
        return name.to_owned();
    };
    let minutes = (wall.0 - start.0).max(0) / 60_000;
    match minutes {
        0 => tr!("{name} - justo ahora", name = name),
        1..=59 => tr!("{name} – {n} min", name = name, n = minutes),
        60..=1_439 => tr!("{name} – {n} h", name = name, n = minutes / 60),
        _ => tr!("{name} – {n} d", name = name, n = minutes / 1_440),
    }
}

/// The card's menu: a message, when the card is one person; the join of its
/// voice channel and its server; and each live stream's watch. Groups are
/// apart by a line.
fn card_menu(store: &Store, members: &[UserId], channels: &[ChannelId]) -> Vec<MenuEntry> {
    let mut groups: Vec<Vec<MenuEntry>> = Vec::new();
    if let [user] = members {
        groups.push(vec![permit_entry(
            format!("friend:{}:message", user.0),
            t("Enviar mensaje"),
            &store.social(*user).message,
        )]);
    }
    if let Some(&channel) = channels.first() {
        let mut join = Vec::new();
        if let Some(abilities) = store.abilities(channel) {
            join.push(permit_entry(
                format!("{}{}", ids::JOIN, channel.0),
                t("Unirse al canal de voz"),
                &abilities.join_voice,
            ));
        }
        if let Some(server) = store.channel(channel).and_then(|found| found.server) {
            join.push(MenuEntry::Item(
                format!("{}{}", ids::SERVER, server.0),
                t("Ir al servidor").into(),
                None,
            ));
        }
        groups.push(join);
    }
    groups.push(
        members
            .iter()
            .filter_map(|user| {
                let state = store.voice_state_of(*user)?;
                (state.streaming && channels.contains(&state.channel)).then(|| {
                    MenuEntry::Item(
                        format!("{}{}", ids::WATCH, user.0),
                        tr!(
                            "Mirar transmisión de {name}",
                            name = name_of(store, *user).as_str()
                        ),
                        None,
                    )
                })
            })
            .collect(),
    );
    let mut entries = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.extend(group);
    }
    entries
}

/// An entry that works, or that is off and says why.
fn permit_entry(id: String, label: &str, permit: &Permit) -> MenuEntry {
    match permit.reason() {
        Some(reason) => MenuEntry::Disabled(id, label.to_owned(), reason.to_owned()),
        None => MenuEntry::Item(id, label.to_owned(), None),
    }
}

/// The column's clicks. A card or its lead's avatar opens its menu (again, it
/// closes); the menu's own entries act; any other choice closes the menu and
/// is left to its owner.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let id = id.as_str();
    if id == ids::MENU_DISMISS {
        ui.active_now.menu = None;
        return Some(Vec::new());
    }
    if let Some(key) = id.strip_prefix(ids::CARD) {
        toggle(ui, key.to_owned());
        return Some(Vec::new());
    }
    if let Some(user) = number(id, ids::AVATAR) {
        toggle(ui, group_key(store, UserId(user)));
        return Some(Vec::new());
    }
    ui.active_now.menu = None;
    if let Some(server) = number(id, ids::SERVER) {
        return Some(crate::open_server(ui, store, Some(ServerId(server))));
    }
    if let Some(channel) = number(id, ids::JOIN) {
        return Some(crate::voice::join(ui, store, ChannelId(channel), now));
    }
    if let Some(user) = number(id, ids::WATCH) {
        let channel = store
            .voice_state_of(UserId(user))
            .map(|state| state.channel);
        return Some(match channel {
            Some(channel) => crate::voice::join(ui, store, channel, now),
            None => Vec::new(),
        });
    }
    None
}

fn toggle(ui: &mut Ui, key: String) {
    ui.active_now.menu = if ui.active_now.menu.as_deref() == Some(key.as_str()) {
        None
    } else {
        Some(key)
    };
}

/// The number after `prefix` in `id`, when there is one.
fn number(id: &str, prefix: &str) -> Option<u64> {
    id.strip_prefix(prefix)?.split(':').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALL: Timestamp = Timestamp(1_790_000_100_000);
    const MINUTE: i64 = 60_000;

    fn started(minutes: i64) -> Option<Timestamp> {
        Some(Timestamp(WALL.0 - minutes * MINUTE))
    }

    #[test]
    fn a_name_shows_how_long_ago_it_started_in_minutes_hours_or_days() {
        assert_eq!(
            since("Factorio", started(0), WALL),
            "Factorio - justo ahora"
        );
        assert_eq!(since("Factorio", started(47), WALL), "Factorio – 47 min");
        assert_eq!(since("Factorio", started(60), WALL), "Factorio – 1 h");
        assert_eq!(since("Factorio", started(25 * 60), WALL), "Factorio – 1 d");
        assert_eq!(since("Factorio", None, WALL), "Factorio");
    }

    #[test]
    fn one_person_with_one_activity_says_what_it_is_doing() {
        let activity = |kind: ActivityKind, name: &str| Activity {
            kind,
            name: name.into(),
            started: started(5),
            ..Activity::default()
        };
        assert_eq!(
            one_activity(&activity(ActivityKind::Playing, "Celeste"), WALL),
            "Celeste – 5 min"
        );
        assert_eq!(
            one_activity(&activity(ActivityKind::Watching, "RustConf"), WALL),
            "Viendo RustConf"
        );
        assert_eq!(
            one_activity(&activity(ActivityKind::Streaming, "Twitch"), WALL),
            "Transmitiendo Twitch"
        );
        assert_eq!(
            one_activity(&activity(ActivityKind::Competing, "AoC"), WALL),
            "Compitiendo en AoC"
        );
        let spotify = Activity {
            name: "Spotify".into(),
            party: Some(rune_model::Party {
                id: Some("spotify:21".into()),
                size: None,
            }),
            ..activity(ActivityKind::Listening, "Spotify")
        };
        assert_eq!(one_activity(&spotify, WALL), "Escuchando Spotify");
        assert_eq!(
            one_activity(&activity(ActivityKind::Listening, "Radio"), WALL),
            "Escuchando Radio"
        );
    }

    #[test]
    fn a_title_names_two_people_or_two_and_a_count() {
        let person = |name: &str| Person {
            user: UserId(1),
            name: name.into(),
            accent: None,
            image: None,
            status: None,
        };
        assert_eq!(title(&[person("Ada")]), "Ada");
        assert_eq!(title(&[person("Ada"), person("Bruno")]), "Ada y Bruno");
        assert_eq!(
            title(&[person("Ada"), person("Bruno"), person("Darío")]),
            "Ada, Bruno y 1 otro"
        );
        assert_eq!(
            title(&[
                person("Elena"),
                person("Inés"),
                person("Facu"),
                person("Gala"),
                person("Julián")
            ]),
            "Elena, Inés y 3 otros"
        );
    }

    #[test]
    fn a_group_is_its_voice_channel_then_its_game_party_then_the_person() {
        assert_eq!(
            key_of(Some(ChannelId(108)), Some("party".into()), UserId(10)),
            "channel:108"
        );
        assert_eq!(
            key_of(None, Some("simulado-factorio".into()), UserId(17)),
            "party:simulado-factorio"
        );
        assert_eq!(key_of(None, None, UserId(12)), "user:12");
    }

    #[test]
    fn the_cards_order_several_people_then_streams_voice_activities_and_spotify_last() {
        let rank = |alone, no_stream, no_voice, no_activity, spotify, names: &str| Rank {
            alone,
            no_stream,
            no_voice,
            no_activity,
            spotify,
            names: names.into(),
        };
        let mut ranks = [
            rank(true, true, true, false, true, "lucas"),
            rank(true, true, true, false, false, "hugo"),
            rank(false, true, false, false, false, "bruno"),
            rank(false, false, false, true, false, "zoe"),
            rank(false, true, false, false, true, "ada"),
        ];
        ranks.sort();
        let names: Vec<&str> = ranks.iter().map(|rank| rank.names.as_str()).collect();
        assert_eq!(names, ["zoe", "bruno", "ada", "hugo", "lucas"]);
    }

    #[test]
    fn a_groups_subtitle_says_what_its_people_share() {
        let ui = Ui::default();
        let game = |name: &str, minutes: i64| Activity {
            kind: ActivityKind::Playing,
            name: name.into(),
            started: started(minutes),
            ..Activity::default()
        };
        // People in a voice channel with nothing to play.
        assert_eq!(
            header(&ui, 2, &[], true, WALL),
            (Some("En un canal de voz".to_owned()), HeaderMark::None)
        );
        // Two games among the people: the column says so, not which.
        let two = [
            (game("Factorio", 5), vec![UserId(17)]),
            (game("Celeste", 9), vec![UserId(11)]),
        ];
        assert_eq!(
            header(&ui, 2, &two, true, WALL),
            (
                Some("Jugando varios juegos".to_owned()),
                HeaderMark::Controller
            )
        );
        // One game shared by several people: its name and time, on one line.
        let one = [(game("Factorio", 47), vec![UserId(17), UserId(11)])];
        assert_eq!(
            header(&ui, 2, &one, true, WALL).0.as_deref(),
            Some("Factorio – 47 min")
        );
    }

    #[test]
    fn a_timer_counts_down_to_an_end_still_ahead_and_up_from_a_start() {
        let game = Activity {
            kind: ActivityKind::Playing,
            name: "Factorio".into(),
            started: started(47),
            ..Activity::default()
        };
        assert_eq!(timer(&game, WALL).as_deref(), Some("Transcurrido: 47:00"));
        let ending = Activity {
            ends: Some(Timestamp(WALL.0 + 3 * MINUTE + 5_000)),
            ..game.clone()
        };
        assert_eq!(timer(&ending, WALL).as_deref(), Some("Restantes: 03:05"));
        let music = Activity {
            kind: ActivityKind::Listening,
            ..game
        };
        assert_eq!(timer(&music, WALL), None);
    }
}
