//! A forum's own settings (P7): its tags (made, renamed, given an emoji,
//! kept to the moderation, deleted), the reaction a new post starts with,
//! and how it lists its posts until each person picks otherwise. As the
//! rest of the channel's settings, it is a draft in the channel's
//! [`ChannelEdit`] until saved, and the permit is asked again on save.
//! Since 2026-10-19 the reaction may be one of the server's own emojis
//! (`:name:`), and the forum says after how long its posts are put away and
//! whether filtering by tags asks for all of them.

use limn::{Button, ButtonSize, ButtonVariant, Component, Element, Icon, TextField};
use rune_components::channel_settings::{field_select, FieldSelect};
use rune_components::server_admin::{self as admin_parts, TagLine};
use rune_components::server_settings::{self as parts, Metrics};
use rune_components::{text, Look};
use rune_i18n::{t, tr};
use rune_model::{Channel, CustomEmoji, ForumDefaults, ForumTag, TagId, ARCHIVE_AFTER, MAX_TAGS};
use rune_provider::ChannelEdit;
use rune_state::{Abilities, Store};

use super::{set_input, ChannelSettings};
use crate::Intent;

/// The field of the reaction a new post starts with.
pub(crate) const REACTION: &str = "cs:forum:reaction";

/// The tags and defaults of `edit` without what `channel` already has.
pub(super) fn changes(
    edit: &ChannelEdit,
    channel: &Channel,
) -> (Option<Vec<ForumTag>>, Option<ForumDefaults>) {
    let tags = edit
        .tags
        .as_ref()
        .map(|tags| tidy(tags))
        .filter(|tags| !same_tags(tags, &channel.tags));
    let forum = edit.forum.clone().filter(|forum| *forum != channel.forum);
    (tags, forum)
}

/// Whether `drafted` are the tags `stored` has: a new one (id 0) is the
/// stored one in its place once the provider gave it an id.
fn same_tags(drafted: &[ForumTag], stored: &[ForumTag]) -> bool {
    drafted.len() == stored.len()
        && drafted.iter().zip(stored).all(|(drafted, stored)| {
            (drafted.id.0 == 0 || drafted.id == stored.id)
                && ForumTag {
                    id: stored.id,
                    ..drafted.clone()
                } == *stored
        })
}

/// Tags as they would be saved: trimmed, without the nameless.
fn tidy(tags: &[ForumTag]) -> Vec<ForumTag> {
    tags.iter()
        .map(|tag| ForumTag {
            name: tag.name.trim().to_owned(),
            emoji: tag
                .emoji
                .as_deref()
                .map(str::trim)
                .filter(|emoji| !emoji.is_empty())
                .map(str::to_owned),
            ..tag.clone()
        })
        .filter(|tag| !tag.name.is_empty())
        .collect()
}

/// The tags as the page shows them.
fn shown_tags(settings: &ChannelSettings, channel: &Channel) -> Vec<ForumTag> {
    settings
        .edit
        .tags
        .clone()
        .unwrap_or_else(|| channel.tags.clone())
}

// Server emoji, archive and tag filter (2026-10-19).
/// What the reaction's field shows: the emoji, or `:name:` for one of the
/// server's own.
fn reaction_text(forum: &ForumDefaults, emojis: &[CustomEmoji]) -> String {
    match forum.reaction_emoji {
        Some(id) => emojis
            .iter()
            .find(|emoji| emoji.id == id)
            .map(|emoji| format!(":{}:", emoji.name))
            .unwrap_or_default(),
        None => forum.reaction.clone().unwrap_or_default(),
    }
}

/// How an archive time reads.
fn archive_label(minutes: u32) -> String {
    match minutes {
        60 => t("1 hora").into(),
        1440 => t("24 horas").into(),
        4320 => t("3 días").into(),
        10080 => t("1 semana").into(),
        other => tr!("{n} minutos", n = other),
    }
}

/// The defaults as the page shows them.
fn shown_defaults(settings: &ChannelSettings, channel: &Channel) -> ForumDefaults {
    settings
        .edit
        .forum
        .clone()
        .unwrap_or_else(|| channel.forum.clone())
}

/// The tags' and the reaction's fields, as limn keeps their text.
pub(super) fn refresh(
    settings: &mut ChannelSettings,
    channel: &Channel,
    emojis: &[CustomEmoji],
    intents: &mut Vec<Intent>,
) {
    let (tags, forum) = changes(&settings.edit, channel);
    if tags.is_none() {
        settings.edit.tags = None;
        for (index, tag) in channel.tags.iter().enumerate() {
            intents.push(set_input(&format!("cs:tag:{index}:name"), &tag.name));
            intents.push(set_input(
                &format!("cs:tag:{index}:emoji"),
                tag.emoji.as_deref().unwrap_or(""),
            ));
        }
    }
    if forum.is_none() {
        settings.edit.forum = None;
        intents.push(set_input(REACTION, &reaction_text(&channel.forum, emojis)));
    }
}

/// What a field's new text changes; false when it is not one of these.
pub(super) fn typed(
    settings: &mut ChannelSettings,
    store: &Store,
    channel: &Channel,
    id: &str,
    value: &str,
) -> bool {
    if id == REACTION {
        let mut forum = shown_defaults(settings, channel);
        // One of the server's own by `:name:` (2026-10-19).
        let server = channel.server.and_then(|server| store.server(server));
        let own = crate::server_settings::rest::own_emoji(server, value);
        forum.reaction_emoji = own.map(|emoji| emoji.id);
        forum.reaction = match own {
            Some(_) => None,
            None => Some(value.trim().to_owned()).filter(|emoji| !emoji.is_empty()),
        };
        settings.edit.forum = Some(forum);
        return true;
    }
    let Some((index, part)) = id
        .strip_prefix("cs:tag:")
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(index, part)| Some((index.parse::<usize>().ok()?, part)))
    else {
        return false;
    };
    let mut tags = shown_tags(settings, channel);
    if let Some(tag) = tags.get_mut(index) {
        match part {
            "name" => tag.name = value.to_owned(),
            "emoji" => tag.emoji = Some(value.trim().to_owned()).filter(|emoji| !emoji.is_empty()),
            _ => return true,
        }
        settings.edit.tags = Some(tags);
    }
    true
}

/// A click on this page; false when it is not one of these.
pub(super) fn clicked(
    settings: &mut ChannelSettings,
    channel: &Channel,
    abilities: &Abilities,
    parts: &[&str],
    intents: &mut Vec<Intent>,
) -> bool {
    let can = abilities.forum_settings.allowed();
    match parts {
        ["tag", "add"] if can => {
            let mut tags = shown_tags(settings, channel);
            if tags.len() < MAX_TAGS {
                tags.push(ForumTag {
                    id: TagId(0),
                    name: String::new(),
                    emoji: None,
                    moderated: false,
                });
                let index = tags.len() - 1;
                settings.edit.tags = Some(tags);
                intents.push(set_input(&format!("cs:tag:{index}:name"), ""));
                intents.push(set_input(&format!("cs:tag:{index}:emoji"), ""));
                intents.push(Intent::Focus(format!("cs:tag:{index}:name")));
            }
        }
        ["tag", index, what @ ("remove" | "moderated")] if can => {
            let mut tags = shown_tags(settings, channel);
            let Some(index) = index.parse::<usize>().ok().filter(|i| *i < tags.len()) else {
                return true;
            };
            if *what == "remove" {
                tags.remove(index);
                for (index, tag) in tags.iter().enumerate() {
                    intents.push(set_input(&format!("cs:tag:{index}:name"), &tag.name));
                    intents.push(set_input(
                        &format!("cs:tag:{index}:emoji"),
                        tag.emoji.as_deref().unwrap_or(""),
                    ));
                }
            } else {
                tags[index].moderated = !tags[index].moderated;
            }
            settings.edit.tags = Some(tags);
        }
        ["forum", which @ ("gallery" | "by-creation" | "require-tag")] if can => {
            let mut forum = shown_defaults(settings, channel);
            match *which {
                "gallery" => forum.gallery = !forum.gallery,
                "by-creation" => forum.by_creation = !forum.by_creation,
                _ => forum.require_tag = !forum.require_tag,
            }
            settings.edit.forum = Some(forum);
        }
        ["forum", "reaction", "clear"] if can => {
            let mut forum = shown_defaults(settings, channel);
            forum.reaction = None;
            forum.reaction_emoji = None;
            settings.edit.forum = Some(forum);
            intents.push(set_input(REACTION, ""));
        }
        // Archive time and tag filter (2026-10-19).
        ["forum", "archive"] if can => settings.archive_open = !settings.archive_open,
        ["forum", "archive", "dismiss"] => settings.archive_open = false,
        ["forum", "archive", minutes] if can => {
            settings.archive_open = false;
            if let Some(minutes) = minutes
                .parse()
                .ok()
                .filter(|minutes| ARCHIVE_AFTER.contains(minutes))
            {
                let mut forum = shown_defaults(settings, channel);
                forum.archive_after = Some(minutes);
                settings.edit.forum = Some(forum);
            }
        }
        ["forum", "match-all"] if can => {
            let mut forum = shown_defaults(settings, channel);
            forum.match_all_tags = !forum.match_all_tags;
            settings.edit.forum = Some(forum);
        }
        _ => return false,
    }
    true
}

/// The forum's page.
pub(super) fn page(
    settings: &ChannelSettings,
    channel: &Channel,
    emojis: &[CustomEmoji],
    abilities: &Abilities,
    look: &Look,
    metrics: Metrics,
) -> Element {
    let denied = abilities.forum_settings.reason();
    let can = denied.is_none();
    let tags = shown_tags(settings, channel);
    let forum = shown_defaults(settings, channel);
    let mut children = vec![parts::page_title(
        t("Foro"),
        Some(t("Las etiquetas que llevan sus publicaciones, la reacción con que empiezan y cómo se listan hasta que cada persona elija otra cosa.")),
        look,
    )];
    if let Some(reason) = denied {
        children.push(parts::denied(reason, look));
    }
    children.push(parts::field_label(t("Etiquetas"), false, look));
    children.push(text(
        t("Hasta 20. Las de «Solo moderación» las pone solo quien modera."),
        14.0,
        look.muted,
    ));
    for (index, tag) in tags.iter().enumerate() {
        children.push(admin_parts::tag_row(
            &TagLine {
                index,
                emoji: tag.emoji.clone().unwrap_or_default(),
                name: tag.name.clone(),
                moderated: tag.moderated,
            },
            can,
            look,
        ));
    }
    children.push(
        Button::new("cs:tag:add", t("Agregar etiqueta"), look.palette())
            .icon(Icon::Add)
            .variant(ButtonVariant::Ghost)
            .button_size(ButtonSize::Small)
            .enabled(can && tags.len() < MAX_TAGS)
            .render(),
    );
    children.push(parts::divider(look));
    children.push(parts::field_label(
        t("Reacción predeterminada"),
        false,
        look,
    ));
    children.push(text(
        t("El emoji con que empieza cada publicación nueva, para reaccionar con un clic; también «:nombre:» de uno del servidor."),
        14.0,
        look.muted,
    ));
    children.push(
        Element::row(vec![
            TextField::new(
                REACTION,
                reaction_text(&forum, emojis),
                t("Ninguna"),
                look.palette(),
            )
            .accessibility_label(t("Reacción predeterminada"))
            .enabled(can)
            .render()
            .size(Some(160.0), None),
            Button::new("cs:forum:reaction:clear", t("Quitar"), look.palette())
                .variant(ButtonVariant::Ghost)
                .enabled(can && (forum.reaction.is_some() || forum.reaction_emoji.is_some()))
                .render()
                .centered_y(),
        ])
        .gap(10.0),
    );
    children.push(parts::divider(look));
    children.extend([
        parts::switch_row(
            "cs:forum:gallery",
            t("Mostrar como galería"),
            Some(t("Tarjetas con su imagen, en vez de una lista.")),
            forum.gallery,
            can,
            look,
        ),
        parts::switch_row(
            "cs:forum:by-creation",
            t("Ordenar por fecha de creación"),
            Some(t(
                "Las publicaciones más nuevas primero, en vez de las de actividad más reciente.",
            )),
            forum.by_creation,
            can,
            look,
        ),
        parts::switch_row(
            "cs:forum:require-tag",
            t("Pedir una etiqueta en cada publicación"),
            None,
            forum.require_tag,
            can,
            look,
        ),
        // Archive time and tag filter (2026-10-19).
        parts::switch_row(
            "cs:forum:match-all",
            t("Filtrar por todas las etiquetas elegidas"),
            Some(t(
                "Al filtrar, se muestran las publicaciones con todas las etiquetas elegidas, en vez de las que tienen alguna.",
            )),
            forum.match_all_tags,
            can,
            look,
        ),
        parts::divider(look),
        parts::field_label(t("Archivar después de"), false, look),
        text(
            t("Cuánto tiempo sin actividad espera una publicación nueva antes de archivarse."),
            14.0,
            look.muted,
        ),
        field_select(
            &FieldSelect {
                id: "cs:forum:archive".into(),
                name: t("Archivar después de").into(),
                value: forum
                    .archive_after
                    .map_or_else(|| t("Como decida el servicio").into(), archive_label),
                options: ARCHIVE_AFTER
                    .iter()
                    .map(|minutes| {
                        (
                            format!("cs:forum:archive:{minutes}"),
                            archive_label(*minutes),
                            forum.archive_after == Some(*minutes),
                        )
                    })
                    .collect(),
                open: settings.archive_open,
                dismiss: "cs:forum:archive:dismiss".into(),
                enabled: can,
                width: metrics.content.min(320.0),
            },
            look,
        ),
    ]);
    parts::page("cs:scroll:forum", metrics.content, children)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nameless_tags_are_left_out_and_names_trimmed() {
        let tags = tidy(&[
            ForumTag {
                id: TagId(1),
                name: " Ayuda ".into(),
                emoji: Some(" ".into()),
                moderated: false,
            },
            ForumTag {
                id: TagId(0),
                name: "  ".into(),
                emoji: None,
                moderated: true,
            },
        ]);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Ayuda");
        assert!(tags[0].emoji.is_none());
    }
}
