//! The filters a search through the whole history takes in its text
//! (`from:ana has:link`), as Serein and the service's own client write
//! them, with Spanish names too, and what the field suggests while one is
//! being written.

use limn::Icon;
use rune_components::time;
use rune_i18n::{es, t};
use rune_model::{
    AuthorKind, ChannelId, ChannelKind, Has, RemoteSearch, ServerId, Timestamp, UserId,
};
use rune_state::Store;

/// One filter: the key the service takes, the Spanish one, and what it does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Key {
    From,
    Mentions,
    Has,
    In,
    Before,
    After,
    During,
    Pinned,
    AuthorType,
    // Reactions, pins and search (2026-10-18).
    LinkFrom,
}

const KEYS: [(Key, &str, &str, &str); 10] = [
    (Key::From, "from", "de", es("De una persona")),
    (
        Key::Mentions,
        "mentions",
        "menciona",
        es("Que menciona a una persona"),
    ),
    (
        Key::Has,
        "has",
        "tiene",
        es("Enlace, tarjeta, archivo, imagen, video, sonido, sticker o encuesta"),
    ),
    (Key::In, "in", "en", es("En un canal del servidor")),
    (
        Key::Before,
        "before",
        "antes",
        es("Antes de un día (AAAA-MM-DD)"),
    ),
    (
        Key::After,
        "after",
        "despues",
        es("Después de un día (AAAA-MM-DD)"),
    ),
    (
        Key::During,
        "during",
        "durante",
        es("Durante un día (AAAA-MM-DD)"),
    ),
    (
        Key::Pinned,
        "pinned",
        "fijado",
        es("Fijados o no (true, false)"),
    ),
    (
        Key::AuthorType,
        "author_type",
        "autor",
        es("Persona, bot o webhook"),
    ),
    (
        Key::LinkFrom,
        "link_from",
        "sitio",
        es("Con un enlace a un sitio (ejemplo.com)"),
    ),
];

/// A key's letters, in any case, with or without `_`: the service's own
/// client writes `authorType:` and `linkFrom:`.
fn plain(word: &str) -> String {
    word.to_lowercase().replace('é', "e").replace('_', "")
}

fn key(word: &str) -> Option<Key> {
    let word = plain(word);
    KEYS.iter()
        .find(|(_, english, spanish, _)| plain(english) == word || plain(spanish) == word)
        .map(|(key, ..)| *key)
}

/// A token as a filter: its key and value, or `None` for plain text.
fn split(token: &str) -> Option<(Key, &str)> {
    let (name, value) = token.split_once(':')?;
    Some((key(name)?, value))
}

/// A name as one token: spaces become `_`.
fn token(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join("_")
}

/// The people a filter can name in `channel`: its server's members, or the
/// people of a direct conversation, and whoever else is known.
fn people(store: &Store, channel: ChannelId) -> Vec<UserId> {
    let found = store.channel(channel);
    let mut people: Vec<UserId> = match found.and_then(|found| found.server) {
        Some(server) => store
            .members(server)
            .iter()
            .map(|member| member.user)
            .collect(),
        None => store.direct_members(channel),
    };
    people.sort();
    people.dedup();
    people
}

fn person(store: &Store, channel: ChannelId, value: &str) -> Option<UserId> {
    if let Some(id) = value.parse().ok().filter(|id| *id != 0) {
        return Some(UserId(id));
    }
    let wanted = value.trim_start_matches('@').to_lowercase();
    let named = |user: &rune_model::User| {
        user.name.to_lowercase() == wanted
            || user
                .display_name
                .as_deref()
                .is_some_and(|name| token(name).to_lowercase() == wanted)
    };
    people(store, channel)
        .into_iter()
        .filter_map(|id| store.user(id))
        .chain(store.users())
        .find(|user| named(user))
        .map(|user| user.id)
}

fn channels(store: &Store, server: ServerId) -> impl Iterator<Item = &rune_model::Channel> {
    store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| channel.kind.has_messages() || channel.kind == ChannelKind::Forum)
}

fn day(value: &str, next: bool) -> Result<Timestamp, &'static str> {
    time::local_date_boundary(value, next).ok_or(t("Escribe las fechas como AAAA-MM-DD."))
}

/// `text` as a search from `channel`, through its whole server when it has
/// one: filters named by key, the rest as words.
pub(super) fn parse(
    store: &Store,
    channel: ChannelId,
    text: &str,
) -> Result<RemoteSearch, &'static str> {
    let server = store.channel(channel).and_then(|found| found.server);
    let mut search = RemoteSearch {
        channel,
        server,
        ..Default::default()
    };
    let mut words = Vec::new();
    for word in text.split_whitespace() {
        let Some((key, value)) = split(word) else {
            words.push(word);
            continue;
        };
        if value.is_empty() {
            return Err(t("Completa el filtro o elige una sugerencia."));
        }
        match key {
            Key::From | Key::Mentions => {
                let user = person(store, channel, value)
                    .ok_or(t("Elige una persona de las sugerencias."))?;
                let list = if key == Key::From {
                    &mut search.authors
                } else {
                    &mut search.mentions
                };
                if !list.contains(&user) {
                    list.push(user);
                }
            }
            Key::Has => {
                let has = Has::parse(value).ok_or(t(
                    "Elige enlace, tarjeta, archivo, imagen, video, sonido, sticker o encuesta.",
                ))?;
                if !search.has.contains(&has) {
                    search.has.push(has);
                }
            }
            Key::In => {
                let server = server.ok_or(t("«in:» sólo busca en canales de un servidor."))?;
                let name = value.trim_start_matches('#').to_lowercase();
                let found = channels(store, server)
                    .find(|channel| {
                        channel.name.to_lowercase() == name
                            || token(&channel.name).to_lowercase() == name
                            || channel.id.0.to_string() == name
                    })
                    .ok_or(t("Elige un canal de las sugerencias."))?;
                if !search.channels.contains(&found.id) {
                    search.channels.push(found.id);
                }
            }
            Key::Before => search.before = narrower(search.before, day(value, false)?, false),
            Key::After => search.after = narrower(search.after, day(value, true)?, true),
            Key::During => {
                search.after = narrower(search.after, day(value, false)?, true);
                search.before = narrower(search.before, day(value, true)?, false);
            }
            Key::Pinned => {
                search.pinned = Some(match value.to_lowercase().as_str() {
                    "true" | "si" | "sí" => true,
                    "false" | "no" => false,
                    _ => return Err(t("Elige true o false para «pinned:».")),
                })
            }
            Key::AuthorType => {
                let kind = AuthorKind::parse(value).ok_or(t("Elige persona, bot o webhook."))?;
                if !search.author_kinds.contains(&kind) {
                    search.author_kinds.push(kind);
                }
            }
            Key::LinkFrom => {
                let host =
                    rune_model::link_host(value).ok_or(t("Escribe el sitio como ejemplo.com."))?;
                if !search.link_hosts.contains(&host) {
                    search.link_hosts.push(host);
                }
            }
        }
    }
    search.text = words.join(" ");
    Ok(search)
}

/// The tighter of two bounds: the later start, or the earlier end.
pub(super) fn narrower(old: Option<Timestamp>, new: Timestamp, start: bool) -> Option<Timestamp> {
    Some(match old {
        Some(old) if start => old.max(new),
        Some(old) => old.min(new),
        None => new,
    })
}

/// One suggestion: what replaces the token being written, and how it shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Suggestion {
    pub insert: String,
    pub label: String,
    pub detail: String,
    pub icon: Icon,
}

/// The token being written at the end of `text`, where it starts, and its
/// key and value when it is a filter.
pub(super) fn active(text: &str) -> Option<(usize, &str)> {
    if text.ends_with(char::is_whitespace) {
        return None;
    }
    let start = text.rfind(char::is_whitespace).map_or(0, |at| at + 1);
    Some((start, &text[start..]))
}

/// What the field suggests for the token being written at the end of `text`:
/// filter keys, then their values. `now` dates the day offered for a date.
pub(super) fn suggestions(
    store: &Store,
    channel: ChannelId,
    text: &str,
    now: Timestamp,
) -> Vec<Suggestion> {
    let server = store.channel(channel).and_then(|found| found.server);
    let keys = |prefix: &str| -> Vec<Suggestion> {
        let prefix = plain(prefix);
        KEYS.iter()
            .filter(|(key, ..)| *key != Key::In || server.is_some())
            .filter(|(_, english, spanish, _)| {
                plain(english).starts_with(&prefix) || plain(spanish).starts_with(&prefix)
            })
            .map(|(_, english, _, detail)| Suggestion {
                insert: format!("{english}:"),
                label: format!("{english}:"),
                detail: t(detail).into(),
                icon: Icon::Search,
            })
            .collect()
    };
    if text.trim().is_empty() {
        return keys("");
    }
    let Some((_, word)) = active(text) else {
        return Vec::new();
    };
    let Some((name, value)) = word.split_once(':') else {
        return keys(word);
    };
    let Some(key) = key(name) else {
        return Vec::new();
    };
    let english = KEYS
        .iter()
        .find(|(found, ..)| *found == key)
        .map(|(_, english, ..)| *english)
        .unwrap_or(name);
    let wanted = value.trim_start_matches(['@', '#']).to_lowercase();
    let fill = |value: String, label: String, detail: String, icon: Icon| Suggestion {
        insert: format!("{english}:{value}"),
        label,
        detail,
        icon,
    };
    match key {
        Key::From | Key::Mentions => {
            let mut found: Vec<_> = people(store, channel)
                .into_iter()
                .filter_map(|id| store.user(id))
                .filter(|user| {
                    user.name.to_lowercase().starts_with(&wanted)
                        || user.shown_name().to_lowercase().starts_with(&wanted)
                })
                .map(|user| (user.shown_name().to_lowercase(), user))
                .collect();
            found.sort_by(|a, b| a.0.cmp(&b.0));
            found
                .into_iter()
                .take(6)
                .map(|(_, user)| {
                    fill(
                        user.name.clone(),
                        store.display_name(user.id, server),
                        format!("@{}", user.name),
                        Icon::At,
                    )
                })
                .collect()
        }
        Key::Has => Has::ALL
            .into_iter()
            .filter(|has| has.key().starts_with(&wanted) || has.label().starts_with(&wanted))
            .map(|has| {
                fill(
                    has.key().into(),
                    has.label().into(),
                    format!("has:{}", has.key()),
                    Icon::Paperclip,
                )
            })
            .collect(),
        Key::In => server
            .map(|server| {
                channels(store, server)
                    .filter(|channel| channel.name.to_lowercase().starts_with(&wanted))
                    .take(6)
                    .map(|channel| {
                        fill(
                            token(&channel.name),
                            channel.name.clone(),
                            String::new(),
                            Icon::Hash,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Key::AuthorType => AuthorKind::ALL
            .into_iter()
            .filter(|kind| kind.key().starts_with(&wanted) || kind.label().starts_with(&wanted))
            .map(|kind| {
                fill(
                    kind.key().into(),
                    kind.label().into(),
                    String::new(),
                    Icon::Users,
                )
            })
            .collect(),
        // The sites the loaded messages link to.
        Key::LinkFrom => {
            let loaded = store
                .timeline(channel)
                .map(|timeline| timeline.messages())
                .unwrap_or_default();
            let mut sites: Vec<String> = loaded
                .iter()
                .flat_map(|message| message.content.split("://").skip(1))
                .filter_map(|rest| rune_model::link_host(rest.split(['/', ' ', '>', ')']).next()?))
                .filter(|site| site.starts_with(&wanted))
                .collect();
            sites.sort();
            sites.dedup();
            sites
                .into_iter()
                .take(6)
                .map(|site| fill(site.clone(), site, String::new(), Icon::Globe))
                .collect()
        }
        Key::Pinned => [("true", t("Fijados")), ("false", t("No fijados"))]
            .into_iter()
            .filter(|(value, _)| value.starts_with(&wanted))
            .map(|(value, label)| fill(value.into(), label.into(), String::new(), Icon::Pin))
            .collect(),
        Key::Before | Key::After | Key::During => {
            let today = time::day(now).to_string();
            if today.starts_with(&wanted) {
                vec![fill(
                    today,
                    t("Hoy").into(),
                    t("AAAA-MM-DD, día local").into(),
                    Icon::Clock,
                )]
            } else {
                Vec::new()
            }
        }
    }
}

/// The person being written after `from:` or `mentions:`, to look up at
/// the provider beyond those known.
pub(super) fn person_query(text: &str) -> Option<&str> {
    let (_, word) = active(text)?;
    let (key, value) = split(word)?;
    let value = value.trim_start_matches('@');
    (matches!(key, Key::From | Key::Mentions) && !value.is_empty() && value.parse::<u64>().is_err())
        .then_some(value)
}
