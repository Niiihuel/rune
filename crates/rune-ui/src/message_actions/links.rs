//! Links in messages: the provider's own open in RUNE, the rest ask before
//! leaving it (unless their domain is trusted) and a masked one says so. A
//! moment written in a message shows its full date in its hint, and in a
//! notice when it is clicked or reached with the keyboard and activated.

use std::time::Instant;

use limn::{Element, Key, Modal, UiAction};
use rune_components::message_actions::{self as view, ids, LinkView};
use rune_components::text::{MASKED_LINK, MOMENT};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, Timestamp};
use rune_provider::{Command, Failure};
use rune_state::{Applied, Store};

use crate::{Intent, Ui, ViewContext};

/// A link outside RUNE waiting for the person's yes.
#[derive(Debug)]
pub(crate) struct ExternalLink {
    pub url: String,
    pub host: String,
    pub masked: bool,
    pub trust: bool,
}

/// The host of an `http` or `https` address, in lower case, and whether it
/// names a port; `None` for anything else, or one with a user in it.
pub(crate) fn host(url: &str) -> Option<(String, bool)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty()
        || authority.contains('@')
        || url.len() > 2048
        || url.chars().any(|c| c.is_control() || c == '\\')
    {
        return None;
    }
    let (name, port) = match authority.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => (name, true),
        _ => (authority, false),
    };
    Some((name.trim_end_matches('.').to_ascii_lowercase(), port))
}

/// `host` is `domain` or one of its subdomains.
fn within(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// Whether `url` opens without asking: an `https` address of the
/// provider's own, with no port, or a domain the person trusts.
fn trusted(ui: &Ui, store: &Store, url: &str) -> bool {
    let Some((name, port)) = host(url) else {
        return false;
    };
    let own = store.provider.as_ref().is_some_and(|info| {
        url.starts_with("https://")
            && !port
            && info
                .capabilities
                .trusted_links
                .iter()
                .any(|domain| within(&name, domain))
    });
    own || ui
        .message_actions
        .trusted
        .iter()
        .any(|domain| within(&name, domain))
}

/// Open `url`, asking first unless it is trusted; a masked one always says
/// what it is when it asks.
pub(crate) fn external(ui: &mut Ui, store: &Store, url: &str, masked: bool) -> Vec<Intent> {
    let Some((host, _)) = host(url) else {
        return Vec::new();
    };
    if trusted(ui, store, url) {
        return vec![Intent::OpenUrl(url.to_owned())];
    }
    ui.message_actions.external = Some(ExternalLink {
        url: url.to_owned(),
        host,
        masked,
        trust: false,
    });
    Vec::new()
}

/// Open the conversation a link names, and its message: read by its id
/// first when it is not loaded and the provider can.
pub(crate) fn open_place(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: Option<MessageId>,
    now: Instant,
) -> Vec<Intent> {
    if let Some(found) = store.channel(channel).filter(|found| Store::visible(found)) {
        let server = found.server;
        return crate::open_at(ui, store, server, channel, message);
    }
    let loads = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.load_channels);
    if !loads || store.channel(channel).is_some() {
        ui.notify(
            t("No puedes abrir ese enlace"),
            Some(t("La conversación no está disponible para esta cuenta").into()),
            true,
            now,
        );
        return Vec::new();
    }
    let request = ui.fresh_request();
    ui.message_actions.opening = Some((request, channel, message));
    vec![Intent::Command(Command::LoadChannel { request, channel })]
}

pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    if let Some(rest) = id.strip_prefix("markdown_link:") {
        // The action is the prefix, the block, the run and the address.
        let url = rest.splitn(3, ':').nth(2)?.to_owned();
        return Some(external(ui, store, &url, false));
    }
    if let Some(url) = id.strip_prefix(MASKED_LINK) {
        return Some(external(ui, store, url, true));
    }
    if let Some(rest) = id.strip_prefix("place:") {
        let (channel, message) = rest.split_once(':')?;
        let channel = ChannelId(channel.parse().ok()?);
        let message = message.parse().ok().filter(|id| *id != 0).map(MessageId);
        return Some(open_place(ui, store, channel, message, now));
    }
    if let Some(at) = id.strip_prefix(MOMENT).and_then(|at| at.parse().ok()) {
        ui.notify(
            rune_components::message::full_date(Timestamp(at)),
            None,
            false,
            now,
        );
        return Some(Vec::new());
    }
    Some(match id.as_str() {
        ids::LINK_TRUST => {
            let link = ui.message_actions.external.as_mut()?;
            link.trust = !link.trust;
            Vec::new()
        }
        ids::LINK_OPEN => open_external(ui),
        ids::LINK_CLOSE => {
            ui.message_actions.external = None;
            Vec::new()
        }
        _ => return None,
    })
}

/// The person said yes: open it, and trust its domain from now on when asked.
fn open_external(ui: &mut Ui) -> Vec<Intent> {
    let Some(link) = ui.message_actions.external.take() else {
        return Vec::new();
    };
    let mut intents = Vec::new();
    if link.trust && !ui.message_actions.trusted.contains(&link.host) {
        ui.message_actions.trusted.push(link.host.clone());
        intents.push(Intent::SaveSetting {
            key: super::TRUSTED_KEY.into(),
            value: super::kept(ui),
        });
    }
    intents.push(Intent::OpenUrl(link.url));
    intents
}

pub(crate) fn keys(ui: &mut Ui, key: Key) -> Option<Vec<Intent>> {
    ui.message_actions.external.as_ref()?;
    match key {
        Key::Escape => {
            ui.message_actions.external = None;
            Some(Vec::new())
        }
        Key::Enter => Some(open_external(ui)),
        _ => None,
    }
}

pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    match applied {
        Applied::ChannelLoaded { request, channel } => {
            let Some((_, wanted, message)) = ui
                .message_actions
                .opening
                .take_if(|(asked, _, _)| asked == request)
            else {
                return Vec::new();
            };
            if wanted != *channel
                || store
                    .channel(*channel)
                    .is_none_or(|found| !Store::visible(found))
            {
                ui.notify(
                    t("No puedes abrir ese enlace"),
                    Some(t("La conversación no está disponible para esta cuenta").into()),
                    true,
                    now,
                );
                return Vec::new();
            }
            let server = store.channel(*channel).and_then(|found| found.server);
            crate::open_at(ui, store, server, *channel, message)
        }
        Applied::Failed {
            failure: Failure::Place { request },
            ..
        } => {
            ui.message_actions
                .opening
                .take_if(|(asked, _, _)| asked == request);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn overlay(ui: &Ui, root: Element, look: &Look, context: &ViewContext) -> Element {
    let Some(link) = &ui.message_actions.external else {
        return root;
    };
    view::link_dialog(
        root,
        LinkView {
            url: &link.url,
            host: &link.host,
            masked: link.masked,
            trust: link.trust,
        },
        Modal::fit_width(context.window.width, 480.0),
        look,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_read_strictly_and_subdomains_count() {
        assert_eq!(
            host("https://Ejemplo.TEST/a?b#c"),
            Some(("ejemplo.test".into(), false))
        );
        assert_eq!(host("http://a.test:8080/"), Some(("a.test".into(), true)));
        assert_eq!(host("https://user@a.test/"), None);
        assert_eq!(host("ftp://a.test/"), None);
        assert_eq!(host("https:///x"), None);
        assert!(within("cdn.discord.com", "discord.com"));
        assert!(within("discord.com", "discord.com"));
        assert!(!within("evildiscord.com", "discord.com"));
        assert!(!within("discord.com.evil.test", "discord.com"));
    }
}
