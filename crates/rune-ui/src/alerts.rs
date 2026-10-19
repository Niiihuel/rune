//! What a new message tells the person outside the conversation they read:
//! a desktop notification and the message sound.
//!
//! Only mentions of the account (by name, or through its roles while it
//! does not silence those) and direct conversations alert, never the
//! account's own messages. Nothing alerts while the account is in «No
//! molestar», from a muted server, category or channel, a thread the account
//! silenced, or one set to notify nothing, nor the conversation open in the
//! focused window. The
//! host shows the notification and plays the sound; a click opens the
//! conversation only for the same account.

use rune_components::server_menu::NotifyLevel;
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, Message, MessageId, Status, Timestamp, UserId};
use rune_state::Store;

use crate::Ui;

/// What to tell about one message, and where a click leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub account: UserId,
    pub channel: ChannelId,
    pub message: MessageId,
    pub title: String,
    pub body: String,
    /// Show it as a desktop notification.
    pub notify: bool,
    /// Play the message sound.
    pub sound: bool,
    /// A call ringing the account (llamadas-y-panel-de-voz): the
    /// notification answers or declines it, and stays while it rings.
    pub call: bool,
}

/// The alert for `message`, just created, if it should alert. `reading`:
/// the window is focused on its conversation.
pub fn message_alert(
    ui: &Ui,
    store: &Store,
    message: &Message,
    reading: bool,
    now: Timestamp,
) -> Option<Alert> {
    let prefs = &ui.prefs;
    if !(prefs.desktop_notifications || prefs.message_sounds) || reading {
        return None;
    }
    let account = store.me()?;
    if message.author == account
        || message.delivery.is_local()
        || store.presence(account).status == Status::DoNotDisturb
    {
        return None;
    }
    let channel = store.channel(message.channel)?;
    if !Store::visible(channel) {
        return None;
    }
    let direct = matches!(channel.kind, ChannelKind::Direct | ChannelKind::Group);
    // By name, or through its roles or everyone, as the provider says (the
    // words alone tell nobody when the author may not), unless silenced.
    let mentioned = store.counts_as_mention(message, account);
    let (level, muted) = crate::channel_list::effective(ui, store, channel, now);
    let server_muted = channel
        .server
        .is_some_and(|server| crate::account::server_notify(ui, store, server).muted(now));
    if !(direct || mentioned) || muted || server_muted || level == NotifyLevel::Nothing {
        return None;
    }
    let author = store.display_name(message.author, channel.server);
    let title = match channel.kind {
        ChannelKind::Direct => author,
        ChannelKind::Group => tr!(
            "{author} en {group}",
            author,
            group = store.channel_title(channel.id)
        ),
        _ => tr!("{author} en #{channel}", author, channel = channel.name),
    };
    let body = if !prefs.notification_content {
        t("Nuevo mensaje").to_owned()
    } else if !message.content.trim().is_empty() {
        message.content.clone()
    } else if !message.attachments.is_empty() {
        t("Envió un archivo").to_owned()
    } else if !message.stickers.is_empty() {
        t("Envió un sticker").to_owned()
    } else {
        t("Nuevo mensaje").to_owned()
    };
    Some(Alert {
        account,
        channel: message.channel,
        message: message.id,
        title,
        body,
        notify: prefs.desktop_notifications,
        sound: prefs.message_sounds,
        call: false,
    })
}

/// A clicked notification: open its conversation if it belongs to the
/// account signed in now and is still there. `false` when it does not.
pub fn open_alert(
    ui: &mut Ui,
    store: &mut Store,
    account: UserId,
    channel: ChannelId,
) -> Vec<crate::Intent> {
    let Some(server) = store
        .channel(channel)
        .filter(|found| store.me() == Some(account) && Store::visible(found))
        .map(|found| found.server)
    else {
        ui.notify(
            t("Esa notificación ya no se puede abrir"),
            Some(t("Es de otra cuenta o de una conversación que ya no está.").into()),
            false,
            std::time::Instant::now(),
        );
        return Vec::new();
    };
    crate::open(ui, store, server, channel)
}
