//! What an app laid out with a message besides its text: rows of buttons and
//! menus, and the blocks of a message made only of them. A button or a menu
//! with a `custom_id` tells its app what the account did with it, through
//! [`crate::interactions::Interaction::Part`]; the others are only read.

use serde::{Deserialize, Serialize};

use crate::{ChannelId, ChannelKind, EmbedImage, RemoteImage, RoleId, TextReference, UserId};

/// The parts of a message an app laid out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Components {
    #[serde(default)]
    pub parts: Vec<Component>,
    /// They are the whole message, without text or cards of its own: its
    /// files show only where its parts place them.
    #[serde(default)]
    pub whole: bool,
    /// Mentions and emoji found in its texts.
    #[serde(default)]
    pub text_references: Vec<TextReference>,
    // Commands and interactions (P3).
    /// The app that answers when its parts are used; `None` when nobody
    /// does, as for a webhook's message or a forwarded copy.
    #[serde(default)]
    pub app: Option<UserId>,
    /// The service's own flags of the message, given back as they came
    /// when one of its parts is used.
    #[serde(default)]
    pub flags: u64,
    /// An app's answer only the account sees, and only while this session
    /// lasts: it is never kept.
    #[serde(default)]
    pub ephemeral: bool,
    /// The command it answers, and who ran it.
    #[serde(default)]
    pub invoked: Option<Invoked>,
}

/// A command someone ran, which an app's message answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Invoked {
    pub user: UserId,
    /// `name sub`, without the `/`.
    pub command: String,
}

impl Components {
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// One part, as the service numbers its kinds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Component {
    /// Buttons side by side, or one menu.
    Row(Vec<Component>),
    Button(Button),
    /// A menu to pick from.
    Select(Select),
    /// Markdown source.
    Text(String),
    /// Texts with one part beside them: a thumbnail or a button.
    Section {
        parts: Vec<Component>,
        accessory: Box<Component>,
    },
    Thumbnail(Media),
    /// Up to ten pictures together.
    Gallery(Vec<Media>),
    /// One of the message's files, by its name.
    File {
        name: String,
        spoiler: bool,
    },
    /// Room between parts, with a line across when `divider`.
    Separator {
        divider: bool,
        large: bool,
    },
    /// Parts in a box with a stripe of `accent` (`0xRRGGBB`).
    Container {
        accent: Option<u32>,
        spoiler: bool,
        parts: Vec<Component>,
    },
    /// A kind RUNE does not draw, by the service's number.
    Unknown(u16),
}

/// A button: one that leads to a page opens it, one with a `custom_id`
/// tells its app it was pressed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub label: Option<String>,
    /// A Unicode emoji, or a server's own emoji as `:name:`.
    pub emoji: Option<String>,
    /// The picture of a server's own emoji; its `:name:` stands in until it
    /// loads.
    #[serde(default)]
    pub picture: Option<RemoteImage>,
    pub style: ButtonStyle,
    /// Where a link button leads: only `http` or `https`.
    pub url: Option<String>,
    pub disabled: bool,
    /// What its app calls it, given back when it is pressed.
    #[serde(default)]
    pub custom_id: Option<String>,
}

/// A menu, by what it says before something is picked, or by what its app
/// left picked: then it shows those, not the placeholder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Select {
    pub placeholder: Option<String>,
    /// The options of a text menu its app left picked, as they are drawn.
    #[serde(default)]
    pub chosen: Vec<Choice>,
    /// What its app calls it, given back with what is picked.
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub kind: MenuKind,
    /// A text menu's options; the others offer people, roles or channels.
    #[serde(default)]
    pub options: Vec<MenuOption>,
    /// How few and how many may be picked at once.
    #[serde(default)]
    pub min: u8,
    #[serde(default = "one")]
    pub max: u8,
    #[serde(default)]
    pub disabled: bool,
    /// What a menu of people, roles or channels starts with picked.
    #[serde(default)]
    pub defaults: Vec<Picked>,
    /// The kinds of channel a channel menu offers; empty, any.
    #[serde(default)]
    pub channels: Vec<ChannelKind>,
    /// In a modal: it must be answered.
    #[serde(default)]
    pub required: bool,
}

fn one() -> u8 {
    1
}

impl Default for Select {
    fn default() -> Self {
        Self {
            placeholder: None,
            chosen: Vec::new(),
            custom_id: None,
            kind: MenuKind::Text,
            options: Vec::new(),
            min: 0,
            max: 1,
            disabled: false,
            defaults: Vec::new(),
            channels: Vec::new(),
            required: false,
        }
    }
}

impl Select {
    /// What starts picked, as its app is told: the default options of a
    /// text menu, the default people, roles or channels of the others.
    pub fn starting(&self) -> Vec<String> {
        match self.kind {
            MenuKind::Text => self
                .options
                .iter()
                .filter(|option| option.default)
                .map(|option| option.value.clone())
                .collect(),
            _ => self.defaults.iter().map(|picked| picked.value()).collect(),
        }
    }

    /// Whether `values` may be sent: between `min` and `max` of them, each
    /// once, and of a text menu, each one of its options.
    pub fn accepts(&self, values: &[String]) -> bool {
        let (min, max) = (usize::from(self.min), usize::from(self.max.clamp(1, 25)));
        values.len() >= min
            && values.len() <= max
            && values.iter().enumerate().all(|(i, value)| {
                !value.is_empty()
                    && value.len() <= 100
                    && !values[..i].contains(value)
                    && match self.kind {
                        MenuKind::Text => self.options.iter().any(|option| option.value == *value),
                        _ => value.parse::<u64>().is_ok_and(|id| id != 0),
                    }
            })
    }
}

/// What a menu offers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum MenuKind {
    /// The options its app wrote.
    #[default]
    Text,
    User,
    Role,
    /// People and roles.
    Mentionable,
    Channel,
}

/// An option of a text menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MenuOption {
    pub label: String,
    /// What its app is told when it is picked.
    pub value: String,
    pub description: Option<String>,
    /// A Unicode emoji, or a server's own emoji as `:name:`.
    pub emoji: Option<String>,
    pub picture: Option<RemoteImage>,
    /// Picked to start with.
    pub default: bool,
}

/// Someone, a role or a channel picked in a menu.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Picked {
    User(UserId),
    Role(RoleId),
    Channel(ChannelId),
}

impl Picked {
    /// What its app is told: the id.
    pub fn value(self) -> String {
        match self {
            Self::User(id) => id.0.to_string(),
            Self::Role(id) => id.0.to_string(),
            Self::Channel(id) => id.0.to_string(),
        }
    }
}

/// The button or menu of `parts` whose `custom_id` is `custom_id`, and
/// whether it is a menu.
pub fn control<'a>(parts: &'a [Component], custom_id: &str) -> Option<&'a Component> {
    parts.iter().find_map(|part| match part {
        Component::Button(button) if button.custom_id.as_deref() == Some(custom_id) => Some(part),
        Component::Select(menu) if menu.custom_id.as_deref() == Some(custom_id) => Some(part),
        Component::Row(parts) | Component::Container { parts, .. } => control(parts, custom_id),
        Component::Section { parts, accessory } => control(parts, custom_id)
            .or_else(|| control(std::slice::from_ref(accessory), custom_id)),
        _ => None,
    })
}

/// An option of a menu its app left picked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    /// A Unicode emoji, or a server's own emoji as `:name:`.
    pub emoji: Option<String>,
    /// The picture of a server's own emoji; its `:name:` stands in until it
    /// loads.
    pub picture: Option<RemoteImage>,
}

/// How a button looks, as the app chose it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ButtonStyle {
    Primary,
    #[default]
    Secondary,
    Success,
    Danger,
    /// It opens a page.
    Link,
    /// It offers something to buy.
    Premium,
}

/// A picture of a part, through the service's image proxy; `None` when it is
/// not a picture RUNE may load.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Media {
    pub image: Option<EmbedImage>,
    pub description: Option<String>,
    pub spoiler: bool,
    /// The service found it explicit: the account's filter may blur it
    /// (contenido-de-mensajes).
    #[serde(default)]
    pub explicit: bool,
}

impl Media {
    /// Whether it waits for a reveal of its own: a spoiler, or explicit,
    /// which the account's filter may hide. Either takes a spoiler's number,
    /// whatever the filter, so numbers do not move when it changes.
    pub fn covered(&self) -> bool {
        self.spoiler || self.explicit
    }
}

/// The pictures `parts` show: one behind a spoiler only once `revealed`
/// holds that spoiler's number; an explicit one, while the account's filter
/// hides it (`obscure`) and it is not revealed, blurred
/// ([`RemoteImage::obscured`]). Spoilers are numbered from 0 in the order
/// they are drawn, those inside a hidden box and explicit pictures
/// included, so each keeps its number whatever else is revealed.
pub fn pictures(
    parts: &[Component],
    obscure: bool,
    revealed: impl Fn(usize) -> bool,
) -> Vec<RemoteImage> {
    let mut found = Vec::new();
    collect(parts, &mut 0, obscure, &revealed, &mut found);
    found
}

/// How many spoilers `parts` hold, nested ones included.
pub fn spoilers(parts: &[Component]) -> usize {
    let mut next = 0;
    collect(parts, &mut next, false, &|_| true, &mut Vec::new());
    next
}

fn collect(
    parts: &[Component],
    next: &mut usize,
    obscure: bool,
    revealed: &dyn Fn(usize) -> bool,
    found: &mut Vec<RemoteImage>,
) {
    // Whether a part is shown, numbering it when it is a spoiler.
    let shown = |next: &mut usize, spoiler: bool| {
        let n = *next;
        *next += usize::from(spoiler);
        !spoiler || revealed(n)
    };
    let picture = |next: &mut usize, media: &Media| {
        let open = shown(next, media.covered());
        let source = &media.image.as_ref()?.source;
        if open || !media.spoiler && !obscure {
            Some(source.clone())
        } else {
            // A spoiler shows nothing; an explicit picture, blurred.
            (!media.spoiler).then(|| source.obscured())
        }
    };
    for part in parts {
        match part {
            Component::Row(parts) => collect(parts, next, obscure, revealed, found),
            Component::Section { parts, accessory } => {
                collect(parts, next, obscure, revealed, found);
                collect(
                    std::slice::from_ref(accessory),
                    next,
                    obscure,
                    revealed,
                    found,
                );
            }
            Component::Container { spoiler, parts, .. } => {
                if shown(next, *spoiler) {
                    collect(parts, next, obscure, revealed, found)
                } else {
                    *next += spoilers(parts)
                }
            }
            Component::Thumbnail(media) => found.extend(picture(next, media)),
            Component::Gallery(items) => {
                for media in items {
                    found.extend(picture(next, media))
                }
            }
            Component::File { spoiler, .. } => {
                shown(next, *spoiler);
            }
            _ => {}
        }
    }
}

/// The pictures of the server emoji `parts` show, behind a spoiler or not:
/// public pictures that reveal nothing of what the spoiler hides.
pub fn emoji(parts: &[Component]) -> Vec<&RemoteImage> {
    let mut found = Vec::new();
    for part in parts {
        match part {
            Component::Row(parts) | Component::Container { parts, .. } => {
                found.extend(emoji(parts))
            }
            Component::Section { parts, accessory } => {
                found.extend(emoji(parts));
                found.extend(emoji(std::slice::from_ref(accessory)));
            }
            Component::Select(menu) => found.extend(
                menu.chosen
                    .iter()
                    .filter_map(|choice| choice.picture.as_ref())
                    .chain(
                        menu.options
                            .iter()
                            .filter_map(|option| option.picture.as_ref()),
                    ),
            ),
            Component::Button(button) => found.extend(button.picture.as_ref()),
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ImageKind, RemoteImage};

    fn media(url: &str, spoiler: bool) -> Media {
        Media {
            image: Some(EmbedImage {
                source: RemoteImage {
                    url: url.into(),
                    kind: ImageKind::AttachmentPreview,
                },
                dimensions: (1, 1),
            }),
            description: None,
            spoiler,
            explicit: false,
        }
    }

    #[test]
    fn pictures_behind_a_spoiler_wait_until_revealed() {
        let parts = vec![
            Component::Thumbnail(media("a", false)),
            Component::Container {
                accent: None,
                spoiler: true,
                parts: vec![Component::Gallery(vec![media("b", true)])],
            },
            Component::Section {
                parts: vec![Component::Text("x".into())],
                accessory: Box::new(Component::Thumbnail(media("c", true))),
            },
        ];
        let urls = |revealed: &[usize]| -> Vec<String> {
            pictures(&parts, false, |n| revealed.contains(&n))
                .iter()
                .map(|image| image.url.clone())
                .collect()
        };
        assert_eq!(urls(&[]), ["a"]);
        // The box is 0, the picture in it 1, the thumbnail beside the text 2.
        assert_eq!(urls(&[2]), ["a", "c"]);
        assert_eq!(urls(&[0]), ["a"], "its spoiler picture still waits");
        assert_eq!(urls(&[0, 1, 2]), ["a", "b", "c"]);
        assert_eq!(spoilers(&parts), 3);
    }

    #[test]
    fn an_explicit_picture_loads_blurred_while_the_filter_hides_it() {
        let explicit = |url: &str, spoiler: bool| Media {
            explicit: true,
            ..media(url, spoiler)
        };
        let parts = vec![Component::Gallery(vec![
            explicit("a", false),
            explicit("b", true),
            media("c", true),
        ])];
        let found = |obscure: bool, revealed: &[usize]| {
            pictures(&parts, obscure, |n| revealed.contains(&n))
        };
        let sharp = |url: &str| media(url, false).image.unwrap().source;
        // Each takes a number, whatever the filter: a 0, b 1, c 2.
        assert_eq!(spoilers(&parts), 3);
        assert_eq!(found(false, &[]), [sharp("a")], "the filter lets it be");
        assert_eq!(found(true, &[]), [sharp("a").obscured()]);
        assert_eq!(found(true, &[0, 2]), [sharp("a"), sharp("c")]);
        // An explicit spoiler shows nothing of itself, blurred or not.
        assert_eq!(found(true, &[0, 1]), [sharp("a"), sharp("b")]);
        // Rows saved before the mark read as not explicit.
        let mut saved = serde_json::to_value(media("d", false)).unwrap();
        saved.as_object_mut().unwrap().remove("explicit");
        assert!(!serde_json::from_value::<Media>(saved).unwrap().explicit);
    }

    #[test]
    fn a_buttons_server_emoji_is_a_picture_to_load_as_well() {
        let button = |url: &str| {
            Component::Button(Button {
                label: None,
                emoji: Some(":y:".into()),
                picture: Some(RemoteImage {
                    url: url.into(),
                    kind: ImageKind::Artwork,
                }),
                style: ButtonStyle::Secondary,
                ..Button::default()
            })
        };
        let parts = vec![
            Component::Row(vec![button("row")]),
            Component::Section {
                parts: Vec::new(),
                accessory: Box::new(button("accessory")),
            },
        ];
        let urls: Vec<_> = emoji(&parts)
            .iter()
            .map(|image| image.url.as_str())
            .collect();
        assert_eq!(urls, ["row", "accessory"]);
        // A row stored before buttons kept their picture still reads.
        let old: Button = serde_json::from_str(
            r#"{"label":"x","emoji":":y:","style":"Secondary","url":null,"disabled":false}"#,
        )
        .unwrap();
        assert_eq!(old.picture, None);
        assert_eq!(old.custom_id, None);
    }

    #[test]
    fn a_menu_stored_before_it_could_be_used_still_reads() {
        let old: Component = serde_json::from_str(
            r#"{"Select":{"placeholder":"Elegí","chosen":[{"label":"A","emoji":null,"picture":null}]}}"#,
        )
        .unwrap();
        let Component::Select(menu) = old else {
            panic!("a menu")
        };
        assert_eq!(menu.chosen.len(), 1);
        assert_eq!((menu.kind, menu.min, menu.max), (MenuKind::Text, 0, 1));
        assert_eq!(menu.custom_id, None);
        let kept: Components = serde_json::from_str(r#"{"parts":[]}"#).unwrap();
        assert!(!kept.ephemeral && kept.app.is_none());
    }

    #[test]
    fn a_menu_accepts_its_own_options_within_its_counts() {
        let option = |value: &str| MenuOption {
            label: value.into(),
            value: value.into(),
            ..MenuOption::default()
        };
        let menu = Select {
            options: vec![option("a"), option("b"), option("c")],
            min: 1,
            max: 2,
            ..Select::default()
        };
        let values = |list: &[&str]| list.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert!(menu.accepts(&values(&["a"])));
        assert!(menu.accepts(&values(&["a", "c"])));
        assert!(!menu.accepts(&values(&[])), "at least one");
        assert!(!menu.accepts(&values(&["a", "b", "c"])), "at most two");
        assert!(!menu.accepts(&values(&["a", "a"])), "each once");
        assert!(!menu.accepts(&values(&["z"])), "only its options");
        let people = Select {
            kind: MenuKind::User,
            ..Select::default()
        };
        assert!(people.accepts(&values(&["12"])));
        assert!(!people.accepts(&values(&["ada"])));
    }
}
