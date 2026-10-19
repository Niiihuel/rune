//! Provider-neutral composer expression actions. Catalog choices are rechecked
//! against current state, and selection never sends a message implicitly.

use std::time::Instant;

use limn::{Button, ButtonVariant, Component, Element};
use rune_components::{
    composer::ids,
    emoji::{CatalogItem, PickerTab},
    Look,
};
use rune_i18n::t;
use rune_model::{ChannelId, CustomEmoji, EmojiId, Sticker, StickerId};
use rune_state::Store;

use crate::{editor_id, Intent, Ui};

fn custom_denied(store: &Store, channel: ChannelId, emoji: &CustomEmoji) -> Option<String> {
    if let Some(reason) = store
        .abilities(channel)
        .and_then(|a| a.write.reason().map(str::to_owned))
    {
        return Some(reason);
    }
    // Another server's, or an animated one, only as the plan allows (P5).
    if let Some(reason) = store.emoji_perk(channel, emoji).reason() {
        return Some(reason.to_owned());
    }
    if !emoji.available {
        return Some(t("Este emoji no está disponible").into());
    }
    if !emoji.roles.is_empty()
        && !store
            .me()
            .and_then(|me| store.member_of(emoji.server, me))
            .is_some_and(|member| member.roles.iter().any(|role| emoji.roles.contains(role)))
    {
        return Some(t("No tenés el rol necesario para usar este emoji").into());
    }
    None
}

/// Why `content` cannot go to `channel`: the first server emoji it names
/// outside code that the account may not use there, with the picker's
/// reason, instead of the service refusing it (2026-10-18). Unknown perks
/// keep another server's emoji out of the picker, but do not hold back a
/// message that names one: the service decides.
pub(crate) fn unusable_emoji(store: &Store, channel: ChannelId, content: &str) -> Option<String> {
    let said = rune_components::text::outside_code(content);
    let here = store.channel(channel).and_then(|channel| channel.server);
    // Known only from a session, as `Store::emoji_perk` reads it.
    let external = !store.from_cache
        && store
            .abilities(channel)
            .is_some_and(|can| !can.external_emoji.allowed());
    store
        .servers()
        .iter()
        .flat_map(|server| &server.emojis)
        .filter(|emoji| !emoji.insert.is_empty() && said.contains(&emoji.insert))
        .filter(|emoji| store.perks().is_some() || Some(emoji.server) == here || external)
        .find_map(|emoji| {
            custom_denied(store, channel, emoji).map(|reason| format!(":{}: {reason}", emoji.name))
        })
}

/// Tell why `content` cannot go to `channel` (`title` heads the notice),
/// keeping the draft; false when it can (2026-10-18).
pub(crate) fn refuse_unusable(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    content: &str,
    title: &str,
    now: Instant,
) -> bool {
    let Some(reason) = unusable_emoji(store, channel, content) else {
        return false;
    };
    ui.notify(title, Some(reason), true, now);
    true
}

/// The servers whose expressions the picker offers in `channel`: its own
/// first, then, once the provider told the account's perks, every other
/// (P5); before that, only its own.
fn catalog_servers(store: &Store, channel: ChannelId) -> Vec<&rune_model::Server> {
    let here = store.channel(channel).and_then(|c| c.server);
    let mut servers: Vec<_> = here.and_then(|id| store.server(id)).into_iter().collect();
    if store.perks().is_some() {
        servers.extend(
            store
                .servers()
                .iter()
                .filter(|server| Some(server.id) != here),
        );
    }
    servers
}

/// The heading of a server's expressions outside it: its name.
fn section(store: &Store, channel: ChannelId, server: &rune_model::Server) -> Option<String> {
    (store.channel(channel).and_then(|c| c.server) != Some(server.id))
        .then(|| server.shown_name().into())
}

pub(crate) fn custom_choices<'a>(
    store: &'a Store,
    channel: ChannelId,
    query: &str,
) -> Vec<&'a CustomEmoji> {
    let query = query.trim().trim_matches(':').to_ascii_lowercase();
    catalog_servers(store, channel)
        .into_iter()
        .flat_map(|server| &server.emojis)
        .filter(|emoji| {
            emoji.name.to_ascii_lowercase().contains(&query)
                && custom_denied(store, channel, emoji).is_none()
        })
        .take(500)
        .collect()
}

pub(crate) fn custom_items(ui: &Ui, store: &Store, channel: ChannelId) -> Vec<CatalogItem> {
    if !ui.emoji_open {
        return Vec::new();
    }
    catalog_servers(store, channel)
        .into_iter()
        .flat_map(|server| {
            server.emojis.iter().map(move |emoji| CatalogItem {
                id: emoji.id.0,
                name: emoji.name.clone(),
                image: ui.image(&emoji.image),
                unavailable: custom_denied(store, channel, emoji),
                section: section(store, channel, server),
            })
        })
        .take(500)
        .collect()
}

/// A sticker the picker offers in `channel`, and its server (`None` for a
/// standard pack's).
fn sticker(store: &Store, channel: ChannelId, id: StickerId) -> Option<&Sticker> {
    let (server, found) = store.known_sticker(id)?;
    let offered = server.is_none_or(|server| {
        catalog_servers(store, channel)
            .iter()
            .any(|known| known.id == server)
    });
    offered.then_some(found)
}

pub(crate) fn staged_sticker<'a>(ui: &Ui, store: &'a Store) -> Option<&'a Sticker> {
    let (channel, id) = ui.messages.pending_sticker?;
    (ui.channel() == Some(channel))
        .then(|| sticker(store, channel, id))
        .flatten()
}

fn sticker_denied(store: &Store, channel: ChannelId, sticker: &Sticker) -> Option<String> {
    if !sticker.available {
        return Some(t("Este sticker no está disponible").into());
    }
    // Another server's only as the plan allows; a pack's anywhere (P5).
    let server = store
        .known_sticker(sticker.id)
        .and_then(|(server, _)| server);
    if let Some(reason) = store.sticker_perk(channel, server).reason() {
        return Some(reason.to_owned());
    }
    store.abilities(channel).map_or_else(
        || Some(t("El canal no está disponible").into()),
        |a| a.stickers.reason().map(str::to_owned),
    )
}

pub(crate) fn sticker_items(ui: &Ui, store: &Store, channel: ChannelId) -> Vec<CatalogItem> {
    if !ui.emoji_open {
        return Vec::new();
    }
    let item = |sticker: &Sticker, section: Option<String>| CatalogItem {
        id: sticker.id.0,
        name: sticker.name.clone(),
        image: ui.image(&sticker.image),
        unavailable: sticker_denied(store, channel, sticker),
        section,
    };
    let servers = catalog_servers(store, channel)
        .into_iter()
        .flat_map(|server| {
            server
                .stickers
                .iter()
                .map(move |sticker| item(sticker, section(store, channel, server)))
        });
    // Then the packs the service offers everyone (P5).
    let packs = store
        .sticker_packs()
        .into_iter()
        .flatten()
        .flat_map(|pack| {
            pack.stickers
                .iter()
                // A provider's pack name may be its text kept in Spanish.
                .map(move |sticker| item(sticker, Some(t(&pack.name).to_owned())))
        });
    servers.chain(packs).take(500).collect()
}

/// The packs to read when the stickers' tab opens: once the provider
/// offers them and while they are not read (P5).
fn load_packs(store: &Store) -> Option<Intent> {
    let offered = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.sticker_packs);
    (offered && store.sticker_packs().is_none())
        .then_some(Intent::Command(rune_provider::Command::LoadStickerPacks))
}

pub(crate) fn with_sticker_stage(
    surface: Element,
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    look: &Look,
) -> Element {
    let Some(sticker) = staged_sticker(ui, store) else {
        return surface;
    };
    let denied = sticker_denied(store, channel, sticker);
    let label = denied.as_ref().map_or_else(
        || format!("Sticker: {}", sticker.name),
        |reason| format!("{}: {reason}", sticker.name),
    );
    let preview = ui
        .image(&sticker.image)
        .map_or_else(
            || {
                rune_components::centered(
                    Element::icon(limn::Icon::Image)
                        .icon_size(24.0)
                        .foreground(look.muted),
                )
            },
            Element::raster_image,
        )
        .size(Some(56.0), Some(56.0))
        .accessible(
            limn::Access::new(limn::Role::Image)
                .id("composer:sticker:preview")
                .name(format!("Sticker: {}", sticker.name)),
        );
    let stage = Element::row(vec![
        preview,
        rune_components::line(label, 14.0, look.text)
            .flex(1.0)
            .centered_y(),
        Button::new("composer:sticker:remove", t("Quitar"), look.menu_palette())
            .variant(ButtonVariant::Ghost)
            .render()
            .centered_y(),
        Button::new(
            "composer:sticker:send",
            t("Enviar sticker"),
            look.menu_palette(),
        )
        .enabled(denied.is_none())
        .render()
        .centered_y(),
    ])
    .gap(8.0)
    .padding(8.0)
    .background(look.raised)
    .radius(8.0);
    Element::column(vec![surface, stage]).gap(6.0)
}

pub(crate) fn public_url(value: &str) -> Option<String> {
    if value.len() > 2000 || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    match url.host()? {
        url::Host::Domain(host)
            if !host.contains('.')
                || host.ends_with(".localhost")
                || host.ends_with(".local")
                || host.ends_with(".internal") =>
        {
            return None
        }
        url::Host::Ipv4(ip)
            if ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast() =>
        {
            return None
        }
        url::Host::Ipv6(ip)
            if ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || ip.to_ipv4_mapped().is_some() =>
        {
            return None
        }
        _ => {}
    }
    Some(url.to_string())
}

/// Match the catalog order for keyboard activation; a sticker is only staged.
pub(crate) fn choose_first(ui: &mut Ui, store: &Store, now: Instant) -> Option<Vec<Intent>> {
    let channel = ui.channel()?;
    let id = match ui.messages.expression_tab {
        PickerTab::Emoji => {
            let first = custom_choices(store, channel, &ui.messages.query)
                .first()
                .copied()?;
            format!("{}:custom={}", ids::EMOJI, first.id.0)
        }
        PickerTab::Stickers => {
            let query = ui.messages.query.trim().trim_matches(':').to_lowercase();
            let packs = store.sticker_packs().into_iter().flatten();
            let first = catalog_servers(store, channel)
                .into_iter()
                .flat_map(|server| &server.stickers)
                .chain(packs.flat_map(|pack| &pack.stickers))
                .find(|item| {
                    item.name.to_lowercase().contains(&query)
                        && sticker_denied(store, channel, item).is_none()
                });
            let Some(first) = first else {
                return Some(Vec::new());
            };
            format!("{}:sticker={}", ids::EMOJI, first.id.0)
        }
        PickerTab::Gif => return Some(Vec::new()),
    };
    clicked(ui, store, &id, now)
}

pub(crate) fn clicked(ui: &mut Ui, store: &Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    let channel = ui.channel()?;
    let base = ids::EMOJI;
    if let Some(tab) = id.strip_prefix(&format!("{base}:tab=")) {
        ui.messages.expression_tab = match tab {
            "emoji" => PickerTab::Emoji,
            "stickers" => PickerTab::Stickers,
            "gif" => PickerTab::Gif,
            _ => return Some(Vec::new()),
        };
        ui.messages.expression_feedback = None;
        let mut intents = if tab == "gif" {
            let mut intents = crate::gif_favorites::load(store);
            intents.extend(crate::gifs::opened(ui, store));
            intents
        } else {
            load_packs(store).into_iter().collect()
        };
        let field = if tab == "gif" {
            if crate::gifs::searchable(ui, store) {
                crate::gifs::QUERY.into()
            } else {
                format!("{base}:gif-url")
            }
        } else {
            rune_components::emoji::ids::search(base)
        };
        intents.push(Intent::Focus(field));
        return Some(intents);
    }
    if let Some(id) = id
        .strip_prefix(&format!("{base}:custom="))
        .and_then(|id| id.parse::<u64>().ok())
        .map(EmojiId)
    {
        let emoji = custom_choices(store, channel, "")
            .into_iter()
            .find(|emoji| emoji.id == id);
        return Some(emoji.map_or_else(Vec::new, |emoji| {
            crate::composer_selection::insert(ui, channel, &emoji.insert, false, now)
        }));
    }
    if let Some(id) = id
        .strip_prefix(&format!("{base}:sticker="))
        .and_then(|id| id.parse::<u64>().ok())
        .map(StickerId)
    {
        if sticker(store, channel, id)
            .is_some_and(|sticker| sticker_denied(store, channel, sticker).is_none())
        {
            ui.messages.pending_sticker = Some((channel, id));
            ui.emoji_open = false;
        }
        return Some(Vec::new());
    }
    if id == "composer:sticker:remove" {
        ui.messages.pending_sticker = None;
        return Some(vec![Intent::Focus(editor_id(channel))]);
    }
    if id == "composer:sticker:send" {
        let staged = ui
            .messages
            .pending_sticker
            .filter(|(target, _)| *target == channel)
            .and_then(|(_, id)| sticker(store, channel, id))
            .filter(|sticker| sticker_denied(store, channel, sticker).is_none())
            .cloned();
        return Some(staged.map_or_else(Vec::new, |sticker| {
            ui.messages.pending_sticker = None;
            vec![Intent::SendSticker { channel, sticker }]
        }));
    }
    let recent = id
        .strip_prefix(&format!("{base}:gif-recent="))
        .and_then(|id| id.parse::<usize>().ok());
    let favorite = id
        .strip_prefix(&format!("{base}:gif-favorite="))
        .and_then(|id| id.parse::<usize>().ok());
    if id == format!("{base}:gif-insert") || recent.is_some() || favorite.is_some() {
        if !store.abilities(channel).is_some_and(|a| a.write.allowed()) {
            return Some(Vec::new());
        }
        let value = if let Some(index) = recent {
            let Some(value) = ui.messages.gif_recent.get(index) else {
                return Some(Vec::new());
            };
            value.clone()
        } else if let Some(index) = favorite {
            let Some(gif) = crate::gif_favorites::shown(ui, store).get(index) else {
                return Some(Vec::new());
            };
            gif.url.clone()
        } else {
            ui.messages.gif_url.clone()
        };
        let Some(url) = public_url(&value) else {
            ui.messages.expression_feedback =
                Some(t("Usá un enlace HTTPS público sin credenciales.").into());
            return Some(Vec::new());
        };
        let intents = crate::composer_selection::insert(ui, channel, &url, true, now);
        if !intents.is_empty() {
            ui.messages.gif_recent.retain(|old| old != &url);
            ui.messages.gif_recent.insert(0, url);
            ui.messages.gif_recent.truncate(12);
            ui.messages.expression_feedback = None;
        }
        return Some(intents);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{RemoteImage, ServerId};
    use rune_provider::{
        mock::{MockConfig, MockProvider},
        Event, Provider,
    };

    fn world() -> (Ui, Store, ChannelId, ServerId) {
        let provider = MockProvider::new(MockConfig::default());
        let mut snapshot = provider.snapshot();
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| {
                channel.server.is_some()
                    && channel.kind.has_messages()
                    && channel.permissions.contains(rune_model::Permissions::SEND)
            })
            .unwrap();
        let (channel, server) = (channel.id, channel.server.unwrap());
        let community = snapshot
            .servers
            .iter_mut()
            .find(|item| item.id == server)
            .unwrap();
        community.emojis = vec![CustomEmoji {
            id: EmojiId(987),
            server,
            name: "hello".into(),
            insert: "opaque-expression-987".into(),
            animated: false,
            available: true,
            roles: vec![],
            image: Some(RemoteImage {
                url: "https://cdn.discordapp.com/emojis/987.webp?size=128".into(),
                kind: Default::default(),
            }),
        }];
        community.stickers = vec![Sticker {
            id: StickerId(988),
            name: "Greeting".into(),
            image: None,
            available: true,
        }];
        let mut store = Store::new();
        let mut info = provider.info();
        info.capabilities.stickers = true;
        store.provider = Some(info);
        store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
        let mut ui = Ui {
            server: Some(server),
            ..Ui::default()
        };
        ui.channels.insert(Some(server), channel);
        ui.set_draft(channel, Some("Existing draft".into()));
        (ui, store, channel, server)
    }

    #[test]
    fn custom_catalog_selection_inserts_opaque_text_without_sending() {
        let (mut ui, store, channel, _) = world();
        let actions =
            clicked(&mut ui, &store, "composer:emoji:custom=987", Instant::now()).unwrap();
        assert_eq!(ui.drafts[&channel], "Existing draftopaque-expression-987");
        assert!(actions
            .iter()
            .all(|intent| matches!(intent, Intent::SetInput { .. } | Intent::FocusInput { .. })));
        assert_eq!(custom_choices(&store, channel, "hello").len(), 1);
    }

    #[test]
    fn another_servers_emoji_without_the_channels_permission_is_refused_with_why() {
        let (_, mut store, channel, server) = world();
        let now = Instant::now();
        store.apply(
            Event::Perks(rune_model::Perks {
                emoji_anywhere: true,
                ..Default::default()
            }),
            now,
        );
        let mut other = store
            .servers()
            .iter()
            .find(|found| found.id != server)
            .unwrap()
            .clone();
        other.emojis = vec![CustomEmoji {
            id: EmojiId(990),
            server: other.id,
            name: "far".into(),
            insert: "<:far:990>".into(),
            animated: false,
            available: true,
            roles: vec![],
            image: None,
        }];
        store.apply(Event::ServerChanged(other), now);
        assert_eq!(unusable_emoji(&store, channel, "hola <:far:990>"), None);
        let mut room = store.channel(channel).unwrap().clone();
        room.permissions = room
            .permissions
            .without(rune_model::Permissions::EXTERNAL_EMOJI);
        store.apply(Event::ChannelChanged(room), now);
        let reason = unusable_emoji(&store, channel, "hola <:far:990>").unwrap();
        assert!(
            reason.starts_with(":far: No tenés permiso para usar emojis de otros servidores"),
            "{reason}"
        );
        assert_eq!(unusable_emoji(&store, channel, "hola"), None);
        assert_eq!(
            unusable_emoji(&store, channel, "```\n<:far:990>\n```"),
            None,
            "in code it names nothing"
        );
        assert!(
            custom_choices(&store, channel, "far").is_empty(),
            "nor offered"
        );
    }

    #[test]
    fn a_sticker_is_staged_and_only_explicit_send_emits_a_command_without_erasing_text() {
        let (mut ui, store, channel, _) = world();
        let actions = clicked(
            &mut ui,
            &store,
            "composer:emoji:sticker=988",
            Instant::now(),
        )
        .unwrap();
        assert!(actions.is_empty());
        assert_eq!(ui.messages.pending_sticker, Some((channel, StickerId(988))));
        assert_eq!(ui.drafts[&channel], "Existing draft");
        let actions = clicked(&mut ui, &store, "composer:sticker:send", Instant::now()).unwrap();
        assert!(
            matches!(&actions[..], [Intent::SendSticker { channel: found, sticker }] if *found == channel && sticker.id == StickerId(988))
        );
        assert_eq!(ui.drafts[&channel], "Existing draft");
        assert!(ui.messages.pending_sticker.is_none());
    }

    #[test]
    fn stale_catalog_choices_and_changed_capabilities_do_not_send() {
        let (mut ui, mut store, channel, server) = world();
        clicked(
            &mut ui,
            &store,
            "composer:emoji:sticker=988",
            Instant::now(),
        );
        store.provider.as_mut().unwrap().capabilities.stickers = false;
        assert!(
            clicked(&mut ui, &store, "composer:sticker:send", Instant::now())
                .unwrap()
                .is_empty()
        );
        let mut community = store.server(server).unwrap().clone();
        community.emojis[0].available = false;
        store.apply(Event::ServerChanged(community), Instant::now());
        assert!(
            clicked(&mut ui, &store, "composer:emoji:custom=987", Instant::now())
                .unwrap()
                .is_empty()
        );
        assert!(custom_choices(&store, channel, "hello").is_empty());
        assert_eq!(ui.drafts[&channel], "Existing draft");
    }

    #[test]
    fn gif_action_only_appends_a_validated_link_and_keeps_bounded_session_recents() {
        let (mut ui, store, channel, _) = world();
        ui.messages.gif_url = "https://tenor.com/view/example-123".into();
        let actions =
            clicked(&mut ui, &store, "composer:emoji:gif-insert", Instant::now()).unwrap();
        assert!(actions
            .iter()
            .all(|intent| matches!(intent, Intent::SetInput { .. } | Intent::FocusInput { .. })));
        assert_eq!(
            ui.drafts[&channel],
            "Existing draft https://tenor.com/view/example-123"
        );
        assert_eq!(ui.messages.gif_recent.len(), 1);
        ui.messages.gif_url = "https://secret@example.com/private".into();
        assert!(
            clicked(&mut ui, &store, "composer:emoji:gif-insert", Instant::now())
                .unwrap()
                .is_empty()
        );
        assert!(ui.messages.expression_feedback.is_some());
        assert_eq!(ui.messages.gif_recent.len(), 1);
        for index in 0..13 {
            ui.messages.gif_url = format!("https://tenor.com/view/example-{index}");
            clicked(&mut ui, &store, "composer:emoji:gif-insert", Instant::now());
        }
        assert_eq!(ui.messages.gif_recent.len(), 12);
        assert!(!ui
            .messages
            .gif_recent
            .iter()
            .any(|url| url.ends_with("example-0")));
    }

    #[test]
    fn gif_links_are_text_only_public_https_without_credentials() {
        assert_eq!(
            public_url("https://tenor.com/view/example-123"),
            Some("https://tenor.com/view/example-123".into())
        );
        for value in [
            "http://tenor.com/view/x",
            "https://user:secret@example.com/x",
            "https://127.0.0.1/x",
            "https://192.168.1.1/x",
            "https://[::1]/x",
            "https://[::ffff:127.0.0.1]/x",
            "https://device.local/x",
            "https://example.com/\n",
        ] {
            assert!(public_url(value).is_none());
        }
    }

    #[test]
    fn stale_recent_links_and_full_drafts_do_not_insert_a_different_value() {
        let (mut ui, store, channel, _) = world();
        ui.messages.gif_url = "https://tenor.com/view/current".into();
        assert!(clicked(
            &mut ui,
            &store,
            "composer:emoji:gif-recent=99",
            Instant::now()
        )
        .unwrap()
        .is_empty());
        assert_eq!(ui.drafts[&channel], "Existing draft");
        let full = "\u{1f642}".repeat(1000);
        ui.set_draft(channel, Some(full.clone()));
        assert!(
            clicked(&mut ui, &store, "composer:emoji:custom=987", Instant::now())
                .unwrap()
                .is_empty()
        );
        assert_eq!(ui.drafts[&channel], full);
        assert!(ui.messages.gif_recent.is_empty());
    }

    #[test]
    fn reopening_gif_focuses_its_existing_input_and_escape_returns_to_composer() {
        let (mut ui, mut store, channel, _) = world();
        ui.messages.expression_tab = PickerTab::Gif;
        let actions = crate::messages::update(
            &mut ui,
            &mut store,
            &limn::UiAction::Clicked {
                id: ids::EMOJI.into(),
            },
            Instant::now(),
        )
        .unwrap();
        assert!(matches!(&actions[..], [Intent::Focus(id)] if id == "composer:emoji:gif-url"));
        let actions = crate::messages::keys(
            &mut ui,
            &mut store,
            limn::Key::Escape,
            false,
            Some("composer:emoji:gif-url"),
        )
        .unwrap();
        assert!(!ui.emoji_open);
        assert!(matches!(&actions[..], [Intent::Focus(id)] if id == &editor_id(channel)));
    }

    #[test]
    fn enter_uses_custom_search_order_and_stages_stickers_without_sending() {
        let (mut ui, mut store, channel, _) = world();
        ui.messages.query = " :hello: ".into();
        let actions = crate::messages::keys(
            &mut ui,
            &mut store,
            limn::Key::Enter,
            false,
            Some("composer:emoji:search"),
        )
        .unwrap();
        assert_eq!(ui.drafts[&channel], "Existing draftopaque-expression-987");
        assert!(actions
            .iter()
            .all(|intent| matches!(intent, Intent::SetInput { .. } | Intent::FocusInput { .. })));
        ui.messages.expression_tab = PickerTab::Stickers;
        ui.messages.query = "Greeting".into();
        assert!(crate::messages::keys(
            &mut ui,
            &mut store,
            limn::Key::Enter,
            false,
            Some("composer:emoji:search")
        )
        .unwrap()
        .is_empty());
        assert_eq!(ui.messages.pending_sticker, Some((channel, StickerId(988))));
    }
}
