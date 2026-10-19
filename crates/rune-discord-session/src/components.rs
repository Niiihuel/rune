//! Message components, read to draw them and, with their `custom_id`, to
//! tell their app what was done with them (P3). Bounded in depth, count and
//! text; an unknown kind keeps a visible mark, a malformed part is dropped
//! without dropping the message.

use rune_model::components::{Button, Choice, Media, MenuKind, MenuOption, Picked, Select};
use rune_model::{ButtonStyle, ChannelId, Component, Components, RoleId, UserId};
use serde_json::Value;

use crate::rich::{link, media, text};

/// The service allows forty parts a message; nothing nests deeper than a
/// container's section's accessory.
const MAX_PARTS: usize = 40;
const MAX_DEPTH: usize = 4;
/// All the texts of a message's parts keep within this many bytes.
const TEXT_BUDGET: usize = 16 * 1024;

/// The flag that makes the parts the whole message.
const IS_COMPONENTS_V2: u64 = 1 << 15;
/// The flag of a reply only the account sees.
pub(crate) const EPHEMERAL: u64 = 1 << 6;

/// A message's parts, from its `components` and `flags`, and the app that
/// answers their use: its `application_id`, or a bot that wrote it.
pub(crate) fn message(value: &Value) -> Components {
    // A part's picture of one of these files is as explicit as the file.
    let explicit_files = value["attachments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|file| {
            file["flags"]
                .as_u64()
                .is_some_and(|flags| flags & crate::wire::EXPLICIT_MEDIA != 0)
        })
        .filter_map(|file| crate::wire::id(&file["id"]).ok())
        .collect();
    let parts = list(
        &value["components"],
        1,
        &mut Budget {
            explicit_files,
            ..Budget::default()
        },
    );
    let mut text_references = Vec::new();
    for text in texts(&parts) {
        for reference in crate::text::references(text) {
            if !text_references.contains(&reference) {
                text_references.push(reference);
            }
        }
    }
    let (app, flags) = marks(value);
    Components {
        whole: flags & IS_COMPONENTS_V2 != 0,
        parts,
        text_references,
        app,
        flags,
        ephemeral: flags & EPHEMERAL != 0,
        invoked: invoked(value),
    }
}

/// The app that answers the use of a message's parts, its `application_id`
/// or a bot that wrote it (not a webhook), and its flags.
pub(crate) fn marks(value: &Value) -> (Option<UserId>, u64) {
    let flags = value["flags"].as_u64().unwrap_or(0);
    let bot = value["author"]["bot"].as_bool() == Some(true) && value["webhook_id"].is_null();
    let app = crate::wire::id(&value["application_id"])
        .ok()
        .or_else(|| {
            bot.then(|| crate::wire::id(&value["author"]["id"]).ok())
                .flatten()
        })
        .map(UserId);
    (app, flags)
}

/// The command a message answers, and who ran it: from
/// `interaction_metadata`, or the older `interaction`, of an application
/// command (type 2) with its name.
pub(crate) fn invoked(value: &Value) -> Option<rune_model::components::Invoked> {
    let said = [&value["interaction_metadata"], &value["interaction"]]
        .into_iter()
        .find(|said| said["type"].as_u64() == Some(2) && said["name"].is_string())?;
    let command = said["name"]
        .as_str()
        .filter(|name| !name.is_empty() && name.chars().count() <= 100)?
        .to_owned();
    let user = crate::wire::id(&said["user"]["id"]).ok()?;
    Some(rune_model::components::Invoked {
        user: UserId(user),
        command,
    })
}

/// A forwarded copy's parts, from its snapshot. A copy never fetches the
/// original's files (see `rich::forwarded`): a picture of one is dropped and
/// its description stands in; a page's picture through the proxy stays. Its
/// app is not told anything from here.
pub(crate) fn forwarded(value: &Value) -> Components {
    let mut read = message(value);
    drop_files(&mut read.parts);
    read.app = None;
    read.ephemeral = false;
    read
}

fn drop_files(parts: &mut [Component]) {
    let drop = |media: &mut Media| {
        let page = media.image.as_ref().is_some_and(|image| {
            url::Url::parse(&image.source.url).is_ok_and(|url| url.path().starts_with("/external/"))
        });
        if !page {
            media.image = None;
        }
    };
    for part in parts {
        match part {
            Component::Row(parts) | Component::Container { parts, .. } => drop_files(parts),
            Component::Section { parts, accessory } => {
                drop_files(parts);
                drop_files(std::slice::from_mut(accessory));
            }
            Component::Thumbnail(media) => drop(media),
            Component::Gallery(items) => items.iter_mut().for_each(drop),
            _ => {}
        }
    }
}

/// A list of parts; an absent or malformed one is none.
pub(crate) fn parts(value: &Value) -> Vec<Component> {
    list(value, 1, &mut Budget::default())
}

/// The Markdown texts of `parts`, for their mentions and emoji.
pub(crate) fn texts(parts: &[Component]) -> Vec<&str> {
    let mut found = Vec::new();
    for part in parts {
        match part {
            Component::Text(text) => found.push(text.as_str()),
            Component::Row(parts) | Component::Container { parts, .. } => {
                found.extend(texts(parts))
            }
            Component::Section { parts, .. } => found.extend(texts(parts)),
            _ => {}
        }
    }
    found
}

struct Budget {
    parts: usize,
    text: usize,
    /// The message's files the service found explicit, by ID.
    explicit_files: Vec<u64>,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            parts: 0,
            text: TEXT_BUDGET,
            explicit_files: Vec::new(),
        }
    }
}

impl Budget {
    /// `value` as text, within what is left; `None` once it is spent.
    fn text(&mut self, value: &Value, limit: usize) -> Option<String> {
        let mut said = text(value, limit)?;
        if said.len() > self.text {
            let mut end = self.text;
            while !said.is_char_boundary(end) {
                end -= 1;
            }
            said.truncate(end);
        }
        self.text -= said.len();
        (!said.trim().is_empty()).then_some(said)
    }
}

fn list(value: &Value, depth: usize, budget: &mut Budget) -> Vec<Component> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_PARTS)
        .filter_map(|part| component(part, depth, budget))
        .collect()
}

fn component(value: &Value, depth: usize, budget: &mut Budget) -> Option<Component> {
    if depth > MAX_DEPTH || budget.parts >= MAX_PARTS {
        return None;
    }
    let kind = value["type"].as_u64()?;
    budget.parts += 1;
    let spoiler = value["spoiler"].as_bool().unwrap_or(false);
    Some(match kind {
        1 => Component::Row(list(&value["components"], depth + 1, budget)).filter_empty()?,
        2 => Component::Button(button(value, budget)?),
        // String, user, role, mentionable and channel menus.
        3 | 5..=8 => Component::Select(menu(value, kind, budget)),
        9 => {
            let parts = list(&value["components"], depth + 1, budget);
            let accessory = component(&value["accessory"], depth + 1, budget)?;
            if parts.is_empty() {
                return None;
            }
            Component::Section {
                parts,
                accessory: Box::new(accessory),
            }
        }
        10 => Component::Text(budget.text(&value["content"], 4000)?),
        11 => Component::Thumbnail(item(value, budget)),
        12 => Component::Gallery(
            value["items"]
                .as_array()?
                .iter()
                .take(10)
                .map(|entry| item(entry, budget))
                .collect(),
        )
        .filter_empty()?,
        13 => Component::File {
            name: budget.text(&value["name"], 256).or_else(|| {
                value["file"]["url"]
                    .as_str()?
                    .strip_prefix("attachment://")
                    .and_then(|name| budget.text(&Value::from(name), 256))
            })?,
            spoiler,
        },
        14 => Component::Separator {
            divider: value["divider"].as_bool().unwrap_or(true),
            large: value["spacing"].as_u64() == Some(2),
        },
        17 => Component::Container {
            accent: value["accent_color"]
                .as_u64()
                .filter(|color| *color <= 0xFF_FF_FF)
                .map(|color| color as u32),
            spoiler,
            parts: list(&value["components"], depth + 1, budget),
        }
        .filter_empty()?,
        other => Component::Unknown(u16::try_from(other).unwrap_or(u16::MAX)),
    })
}

trait FilterEmpty: Sized {
    /// `None` for a part that would show nothing.
    fn filter_empty(self) -> Option<Self>;
}

impl FilterEmpty for Component {
    fn filter_empty(self) -> Option<Self> {
        let empty = match &self {
            Component::Row(parts) | Component::Container { parts, .. } => parts.is_empty(),
            Component::Gallery(items) => items.is_empty(),
            _ => false,
        };
        (!empty).then_some(self)
    }
}

fn button(value: &Value, budget: &mut Budget) -> Option<Button> {
    let style = match value["style"].as_u64() {
        Some(1) => ButtonStyle::Primary,
        Some(3) => ButtonStyle::Success,
        Some(4) => ButtonStyle::Danger,
        Some(5) => ButtonStyle::Link,
        Some(6) => ButtonStyle::Premium,
        _ => ButtonStyle::Secondary,
    };
    let (emoji, picture) = emoji(&value["emoji"]);
    let label = budget.text(&value["label"], 80);
    if label.is_none() && emoji.is_none() {
        return None;
    }
    Some(Button {
        label,
        emoji,
        picture,
        url: (style == ButtonStyle::Link)
            .then(|| link(&value["url"]))
            .flatten(),
        // A link or a purchase tells its app nothing.
        custom_id: (!matches!(style, ButtonStyle::Link | ButtonStyle::Premium))
            .then(|| custom_id(&value["custom_id"]))
            .flatten(),
        style,
        disabled: value["disabled"].as_bool().unwrap_or(false),
    })
}

/// What an app calls a part: 1 to 100 characters, none a control.
pub(crate) fn custom_id(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|id| {
            !id.is_empty() && id.chars().count() <= 100 && !id.chars().any(char::is_control)
        })
        .map(str::to_owned)
}

/// A menu of `kind`, with what it offers and starts with.
fn menu(value: &Value, kind: u64, budget: &mut Budget) -> Select {
    let kind = match kind {
        5 => MenuKind::User,
        6 => MenuKind::Role,
        7 => MenuKind::Mentionable,
        8 => MenuKind::Channel,
        _ => MenuKind::Text,
    };
    let count = |field: &str, default: u64| {
        value[field].as_u64().map_or(default, |count| count.min(25)) as u8
    };
    let options = if kind == MenuKind::Text {
        value["options"]
            .as_array()
            .into_iter()
            .flatten()
            .take(25)
            .filter_map(|option| {
                let (emoji, picture) = emoji(&option["emoji"]);
                Some(MenuOption {
                    label: budget.text(&option["label"], 100)?,
                    value: option["value"]
                        .as_str()
                        .filter(|value| !value.is_empty() && value.len() <= 100)?
                        .to_owned(),
                    description: budget.text(&option["description"], 100),
                    emoji,
                    picture,
                    default: option["default"].as_bool() == Some(true),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let defaults = value["default_values"]
        .as_array()
        .into_iter()
        .flatten()
        .take(25)
        .filter_map(|picked| {
            let id = crate::wire::id(&picked["id"]).ok()?;
            Some(match (picked["type"].as_str()?, kind) {
                ("user", MenuKind::User | MenuKind::Mentionable) => Picked::User(UserId(id)),
                ("role", MenuKind::Role | MenuKind::Mentionable) => Picked::Role(RoleId(id)),
                ("channel", MenuKind::Channel) => Picked::Channel(ChannelId(id)),
                _ => return None,
            })
        })
        .collect();
    let max = count("max_values", 1).max(1);
    Select {
        placeholder: budget.text(&value["placeholder"], 150),
        chosen: if kind == MenuKind::Text {
            chosen(value, budget)
        } else {
            Vec::new()
        },
        custom_id: custom_id(&value["custom_id"]),
        kind,
        options,
        min: count("min_values", 1).min(max),
        max,
        disabled: value["disabled"].as_bool().unwrap_or(false),
        defaults,
        channels: value["channel_types"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|kind| crate::bootstrap::kind_of(kind.as_u64()?))
            .collect(),
        required: value["required"].as_bool().unwrap_or(true),
    }
}

/// A string menu's options marked `default`, which the reference client shows
/// in place of the placeholder; one only, unless more may be picked. The other
/// menus name users, roles or channels it would look up: none is kept.
fn chosen(value: &Value, budget: &mut Budget) -> Vec<Choice> {
    let most = value["max_values"].as_u64().unwrap_or(1).clamp(1, 25) as usize;
    value["options"]
        .as_array()
        .into_iter()
        .flatten()
        .take(25)
        .filter(|option| option["default"].as_bool() == Some(true))
        .filter_map(|option| {
            let (emoji, picture) = emoji(&option["emoji"]);
            Some(Choice {
                label: budget.text(&option["label"], 100)?,
                emoji,
                picture,
            })
        })
        .take(most)
        .collect()
}

/// A part's emoji: a Unicode one, or a server's own as `:name:` with its
/// picture, which moves when the emoji does (`animated`), as in a line of
/// text; RUNE plays it unless motion is reduced.
fn emoji(value: &Value) -> (Option<String>, Option<rune_model::RemoteImage>) {
    let Some(name) = value["name"]
        .as_str()
        .filter(|name| !name.is_empty() && name.len() <= 64)
    else {
        return (None, None);
    };
    if value["id"].is_null() {
        return (Some(name.to_owned()), None);
    }
    let animated = value["animated"].as_bool() == Some(true);
    let picture = crate::wire::id(&value["id"])
        .ok()
        .map(|id| crate::emoji::part(id, animated));
    (Some(format!(":{name}:")), picture)
}

/// A thumbnail or a gallery's picture: only a picture through the proxy, as
/// a card's picture; anything else keeps its description. It is explicit
/// when its scan says so (`content_scan_metadata.flags` bit 1, as the
/// official client reads it, docs/REA.md) or it shows an explicit file.
fn item(value: &Value, budget: &mut Budget) -> Media {
    let source = &value["media"];
    let picture = source["content_type"]
        .as_str()
        .is_none_or(|kind| kind.starts_with("image/"));
    let explicit = source["content_scan_metadata"]["flags"]
        .as_u64()
        .is_some_and(|flags| flags & 1 != 0)
        || crate::wire::id(&source["attachment_id"])
            .is_ok_and(|file| budget.explicit_files.contains(&file));
    Media {
        image: picture.then(|| media(source)).flatten(),
        description: budget.text(&value["description"], 1024),
        spoiler: value["spoiler"].as_bool().unwrap_or(false),
        explicit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROXIED: &str = "https://media.discordapp.net/attachments/1/2/a.png";

    #[test]
    fn a_whole_message_reads_its_blocks() {
        let value = json!({"flags": 32768, "components": [{
        "type": 17, "accent_color": 0x5865F2, "spoiler": false, "components": [
            {"type": 10, "content": "## Hola <@5>"},
            {"type": 9, "components": [{"type": 10, "content": "Texto"}],
             "accessory": {"type": 11, "media": {"url": "attachment://a.png",
                "proxy_url": PROXIED, "width": 64, "height": 64}}},
            {"type": 14, "divider": false, "spacing": 2},
            {"type": 12, "items": [
                {"media": {"proxy_url": PROXIED, "width": 10, "height": 10}, "spoiler": true},
                {"media": {"proxy_url": "https://media.discordapp.net/attachments/1/2/v.mp4",
                    "content_type": "video/mp4", "width": 10, "height": 10},
                 "description": "Un clip"}]},
            {"type": 13, "file": {"url": "attachment://notas.txt"}},
            {"type": 1, "components": [
                {"type": 2, "style": 5, "label": "Abrir", "url": "https://example.test/x"},
                {"type": 2, "style": 1, "label": "Votar", "custom_id": "v"},
                {"type": 2, "style": 5, "label": "Mal", "url": "javascript:alert(1)"}]},
            {"type": 1, "components": [{"type": 3, "custom_id": "m", "placeholder": "Elegí"}]},
            {"type": 99}
        ]}]});
        let read = message(&value);
        assert!(read.whole);
        assert!(!read.text_references.is_empty(), "the mention is found");
        let Component::Container { accent, parts, .. } = &read.parts[0] else {
            panic!("{:?}", read.parts);
        };
        assert_eq!(*accent, Some(0x5865F2));
        assert_eq!(parts[0], Component::Text("## Hola <@5>".into()));
        assert!(
            matches!(&parts[1], Component::Section { accessory, .. } if matches!(&**accessory, Component::Thumbnail(media) if media.image.is_some()))
        );
        assert_eq!(
            parts[2],
            Component::Separator {
                divider: false,
                large: true
            }
        );
        let Component::Gallery(items) = &parts[3] else {
            panic!()
        };
        assert!(items[0].spoiler && items[0].image.is_some());
        assert!(items[1].image.is_none(), "only pictures load");
        assert_eq!(items[1].description.as_deref(), Some("Un clip"));
        assert_eq!(
            parts[4],
            Component::File {
                name: "notas.txt".into(),
                spoiler: false
            }
        );
        let Component::Row(buttons) = &parts[5] else {
            panic!()
        };
        let urls: Vec<_> = buttons
            .iter()
            .map(|button| match button {
                Component::Button(button) => button.url.as_deref(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(urls, [Some("https://example.test/x"), None, None]);
        let ids: Vec<_> = buttons
            .iter()
            .map(|button| match button {
                Component::Button(button) => button.custom_id.as_deref(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(ids, [None, Some("v"), None], "a link tells its app nothing");
        assert_eq!(
            parts[6],
            Component::Row(vec![Component::Select(Select {
                placeholder: Some("Elegí".into()),
                custom_id: Some("m".into()),
                min: 1,
                required: true,
                ..Select::default()
            })])
        );
        assert_eq!(parts[7], Component::Unknown(99));
        assert!(!message(&json!({"components": []})).whole);
    }

    /// Fixture only, not checked against the service: the explicit mark of
    /// a part's picture, from its scan or from the file it shows.
    #[test]
    fn a_parts_picture_is_explicit_by_its_scan_or_its_file() {
        let value = json!({"flags": 32768,
        "attachments": [{"id": "40", "filename": "a.png", "flags": 16},
                        {"id": "41", "filename": "b.png", "flags": 8}],
        "components": [{"type": 12, "items": [
            {"media": {"proxy_url": PROXIED, "content_scan_metadata": {"version": 2, "flags": 1}}},
            {"media": {"proxy_url": PROXIED, "content_scan_metadata": {"version": 2, "flags": 2}}},
            {"media": {"url": "attachment://a.png", "proxy_url": PROXIED, "attachment_id": "40"}},
            {"media": {"url": "attachment://b.png", "proxy_url": PROXIED, "attachment_id": "41"}}
        ]}]});
        let read = message(&value);
        let Component::Gallery(items) = &read.parts[0] else {
            panic!("{:?}", read.parts);
        };
        let marks: Vec<bool> = items.iter().map(|item| item.explicit).collect();
        assert_eq!(marks, [true, false, true, false]);
        // A list read alone knows no files.
        let alone = parts(&value["components"]);
        let Component::Gallery(items) = &alone[0] else {
            panic!()
        };
        assert!(items[0].explicit && !items[2].explicit);
    }

    #[test]
    fn the_app_of_a_message_and_its_flags_are_read() {
        let read = message(&json!({"flags": 64, "application_id": "12",
            "author": {"id": "7", "bot": true}, "components": []}));
        assert_eq!(read.app, Some(UserId(12)));
        assert!(read.ephemeral);
        assert_eq!(read.flags, 64);
        let bot = message(&json!({"author": {"id": "7", "bot": true}}));
        assert_eq!(bot.app, Some(UserId(7)), "a bot answers for itself");
        let hook = message(&json!({"webhook_id": "3", "author": {"id": "3", "bot": true}}));
        assert_eq!(hook.app, None, "a webhook's message has no app");
        let person = message(&json!({"author": {"id": "7"}}));
        assert_eq!(person.app, None);
        let copy = forwarded(&json!({"application_id": "12", "flags": 64}));
        assert!(copy.app.is_none() && !copy.ephemeral, "a copy tells nobody");
        // The command it answers: the newer metadata, or the older field.
        let answer = invoked(&json!({"interaction_metadata": {"id": "1", "type": 2,
            "name": "dado tirar", "user": {"id": "5"}}}))
        .unwrap();
        assert_eq!(
            (answer.user, answer.command.as_str()),
            (UserId(5), "dado tirar")
        );
        let older = invoked(
            &json!({"interaction": {"id": "1", "type": 2, "name": "ping",
            "user": {"id": "6"}}}),
        );
        assert_eq!(older.map(|said| said.user), Some(UserId(6)));
        assert!(
            invoked(&json!({"interaction_metadata": {"type": 3, "user": {"id": "5"}}})).is_none(),
            "a button's answer names no command"
        );
    }

    #[test]
    fn menus_of_people_roles_and_channels_keep_their_defaults_and_bounds() {
        let read = parts(&json!([
            {"type": 7, "custom_id": "who", "min_values": 0, "max_values": 30,
             "default_values": [{"id": "5", "type": "user"}, {"id": "6", "type": "role"},
                {"id": "7", "type": "channel"}, {"id": "x", "type": "user"}]},
            {"type": 8, "custom_id": "where", "channel_types": [0, 5, 99],
             "default_values": [{"id": "9", "type": "channel"}]},
            {"type": 3, "custom_id": "pick", "options": [
                {"label": "A", "value": "a", "description": "La primera"},
                {"label": "B", "value": ""}]}
        ]));
        let Component::Select(who) = &read[0] else {
            panic!()
        };
        assert_eq!(who.kind, MenuKind::Mentionable);
        assert_eq!((who.min, who.max), (0, 25));
        assert_eq!(
            who.defaults,
            [Picked::User(UserId(5)), Picked::Role(RoleId(6))],
            "a channel is not someone to mention"
        );
        let Component::Select(place) = &read[1] else {
            panic!()
        };
        assert_eq!(
            place.channels,
            [
                rune_model::ChannelKind::Text,
                rune_model::ChannelKind::Announcement
            ]
        );
        assert_eq!(place.starting(), ["9"]);
        let Component::Select(pick) = &read[2] else {
            panic!()
        };
        assert_eq!(
            pick.options.len(),
            1,
            "an option without a value is dropped"
        );
        assert_eq!(pick.options[0].description.as_deref(), Some("La primera"));
    }

    #[test]
    fn a_menu_shows_its_default_options_with_their_emoji() {
        let menu = |max: u64| {
            parts(
                &json!([{"type": 3, "placeholder": "Elegí", "max_values": max, "options": [
                {"label": "Rojo", "value": "r", "default": true,
                 "emoji": {"id": "22", "name": "rojo", "animated": true}},
                {"label": "Verde", "value": "v"},
                {"label": "Azul", "value": "a", "default": true, "emoji": {"name": "🔵"}},
                {"label": "  ", "value": "x", "default": true}]}]),
            )
        };
        let Component::Select(Select { chosen, .. }) = &menu(1)[0] else {
            panic!()
        };
        assert_eq!(chosen.len(), 1, "one only when one may be picked");
        assert_eq!(chosen[0].emoji.as_deref(), Some(":rojo:"));
        // It moves, so its picture is asked for moving.
        assert_eq!(
            chosen[0].picture.as_ref().unwrap().url,
            "https://cdn.discordapp.com/emojis/22.webp?size=128&animated=true"
        );
        let Component::Select(Select { chosen, .. }) = &menu(3)[0] else {
            panic!()
        };
        let said: Vec<_> = chosen
            .iter()
            .map(|choice| {
                (
                    choice.label.as_str(),
                    choice.emoji.as_deref(),
                    choice.picture.is_some(),
                )
            })
            .collect();
        assert_eq!(
            said,
            [("Rojo", Some(":rojo:"), true), ("Azul", Some("🔵"), false)]
        );
        let users = parts(&json!([{"type": 5, "default_values": [{"id": "1", "type": "user"}]}]));
        assert_eq!(
            users[0],
            Component::Select(Select {
                kind: MenuKind::User,
                min: 1,
                defaults: vec![Picked::User(UserId(1))],
                required: true,
                ..Select::default()
            })
        );
        let bad = parts(&json!([{"type": 2, "label": "x", "emoji": {"id": "../1", "name": "y"}}]));
        assert!(
            matches!(&bad[0], Component::Button(button) if button.emoji.as_deref() == Some(":y:") && button.picture.is_none())
        );
        // A button's server emoji brings its picture, as a menu option's.
        let good = parts(&json!([{"type": 2, "label": "x", "emoji": {"id": "22", "name": "y"}}]));
        assert!(matches!(&good[0], Component::Button(button)
            if button.picture.as_ref().is_some_and(|picture| picture.url == "https://cdn.discordapp.com/emojis/22.webp?size=128")));
    }

    #[test]
    fn a_forward_keeps_no_picture_of_the_originals_files() {
        let page = "https://images-ext-1.discordapp.net/external/abc/https/example.test/a.png";
        let read = forwarded(&json!({"flags": 32768, "components": [
            {"type": 12, "items": [
                {"media": {"proxy_url": PROXIED, "width": 10, "height": 10}, "description": "Foto"},
                {"media": {"proxy_url": page, "width": 10, "height": 10}}]},
            {"type": 9, "components": [{"type": 10, "content": "x"}],
             "accessory": {"type": 11, "media": {"proxy_url": PROXIED, "width": 9, "height": 9}}}
        ]}));
        assert!(read.whole);
        let Component::Gallery(items) = &read.parts[0] else {
            panic!()
        };
        assert!(items[0].image.is_none() && items[0].description.as_deref() == Some("Foto"));
        assert_eq!(items[1].image.as_ref().unwrap().source.url, page);
        assert!(
            matches!(&read.parts[1], Component::Section { accessory, .. } if matches!(&**accessory, Component::Thumbnail(media) if media.image.is_none()))
        );
    }

    #[test]
    fn parts_are_bounded_and_malformed_ones_dropped() {
        let many: Vec<_> = (0..60)
            .map(|_| json!({"type": 10, "content": "x"}))
            .collect();
        assert_eq!(parts(&Value::Array(many)).len(), MAX_PARTS);
        let mut deep = json!({"type": 10, "content": "fondo"});
        for _ in 0..6 {
            deep = json!({"type": 17, "components": [deep]});
        }
        assert!(parts(&json!([deep])).is_empty(), "too deep: nothing left");
        let long: Vec<_> = (0..10)
            .map(|_| json!({"type": 10, "content": "y".repeat(4000)}))
            .collect();
        let read = parts(&Value::Array(long));
        let spent: usize = texts(&read).iter().map(|text| text.len()).sum();
        assert!(spent <= TEXT_BUDGET && read.len() < 10);
        for bad in [
            json!({"type": "1"}),
            json!({"type": 1, "components": []}),
            json!({"type": 2, "style": 1}),
            json!({"type": 9, "components": [{"type": 10, "content": "x"}]}),
            json!({"type": 10, "content": "   "}),
            json!({"type": 12, "items": "no"}),
            json!({"type": 13, "file": {"url": "https://x.test/a"}}),
            json!("nada"),
        ] {
            assert!(parts(&json!([bad])).is_empty(), "{bad}");
        }
        assert!(parts(&json!({"type": 10})).is_empty());
    }
}
