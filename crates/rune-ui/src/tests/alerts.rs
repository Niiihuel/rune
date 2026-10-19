use super::*;
use rune_model::{Message, Presence, Status};

/// Someone else's message in `channel`, built from one already there.
fn incoming(host: &Host, channel: ChannelId, content: &str) -> Message {
    let me = host.store.me().unwrap();
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| message.author != me)
        .expect("someone else wrote here")
        .as_ref()
        .clone();
    message.channel = channel;
    message.content = content.into();
    message.mentions.clear();
    message.attachments.clear();
    message
}

fn alert(host: &Host, message: &Message) -> Option<Alert> {
    message_alert(&host.ui, &host.store, message, false, wall())
}

fn wall() -> Timestamp {
    rune_provider::mock::wall_clock()
}

#[test]
fn only_mentions_and_direct_messages_alert_with_what_the_settings_allow() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let me = host.store.me().unwrap();
    let plain = incoming(&host, general, "hola a todos");
    assert_eq!(alert(&host, &plain), None, "a plain server message");
    let mut mention = incoming(&host, general, "<@1> mirá esto");
    mention.mentions.push(me);
    // Sounds are on and notifications asked for: off until turned on.
    let found = alert(&host, &mention).unwrap();
    assert!(found.sound && !found.notify);
    assert!(found.title.ends_with(" en #general"), "{}", found.title);
    host.ui.prefs.desktop_notifications = true;
    let found = alert(&host, &mention).unwrap();
    assert!(found.notify);
    assert_eq!(found.body, "<@1> mirá esto");
    assert_eq!((found.account, found.channel), (me, general));
    host.ui.prefs.notification_content = false;
    assert_eq!(alert(&host, &mention).unwrap().body, "Nuevo mensaje");
    // Reading that conversation in a focused window: nothing.
    assert_eq!(
        message_alert(&host.ui, &host.store, &mention, true, wall()),
        None
    );
    // The account's own message never alerts.
    let mut own = mention.clone();
    own.author = me;
    assert_eq!(alert(&host, &own), None);
    host.ui.prefs.desktop_notifications = false;
    host.ui.prefs.message_sounds = false;
    assert_eq!(alert(&host, &mention), None, "both off");
    host.ui.prefs.message_sounds = true;
    let direct = host.store.directs()[0].clone();
    let mut dm = incoming(&host, general, "");
    dm.channel = direct.id;
    dm.author = direct.recipients[0];
    let found = alert(&host, &dm).expect("a direct message alerts");
    assert_eq!(
        found.body, "Nuevo mensaje",
        "nothing to say but that it came"
    );
}

#[test]
fn do_not_disturb_and_muted_or_silent_conversations_never_alert() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    host.ui.prefs.desktop_notifications = true;
    let general = host.open_channel("general");
    let me = host.store.me().unwrap();
    let mut mention = incoming(&host, general, "che");
    mention.mentions.push(me);
    assert!(alert(&host, &mention).is_some());
    let server = host.store.channel(general).unwrap().server.unwrap();
    host.ui.server_menu.notify.insert(
        server,
        crate::server_menu::Notify {
            level: rune_components::server_menu::NotifyLevel::All,
            muted_until: Some(crate::server_menu::FOREVER),
        },
    );
    assert_eq!(alert(&host, &mention), None, "muted server");
    host.ui.server_menu.notify.clear();
    host.ui.channel_list.notify.insert(
        general,
        crate::ChannelNotify {
            level: Some(rune_components::server_menu::NotifyLevel::Nothing),
            muted_until: None,
        },
    );
    assert_eq!(alert(&host, &mention), None, "set to notify nothing");
    host.ui.channel_list.notify.clear();
    host.event(Event::PresenceChanged {
        user: me,
        presence: Presence {
            status: Status::DoNotDisturb,
            custom: None,
            custom_emoji: None,
            activities: Vec::new(),
            platforms: Default::default(),
        },
    });
    assert_eq!(alert(&host, &mention), None, "No molestar");
}

#[test]
fn a_click_opens_only_a_conversation_of_the_same_account() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let me = host.store.me().unwrap();
    let other = host.open_channel("ayuda");
    assert_eq!(host.ui.channel(), Some(other));
    let intents = open_alert(&mut host.ui, &mut host.store, UserId(me.0 + 1), general);
    assert!(intents.is_empty());
    assert_eq!(
        host.ui.channel(),
        Some(other),
        "another account's opens nothing"
    );
    assert_eq!(
        host.ui.notices.last().unwrap().title,
        "Esa notificación ya no se puede abrir"
    );
    let intents = open_alert(&mut host.ui, &mut host.store, me, general);
    host.run(intents);
    assert_eq!(host.ui.channel(), Some(general));
}
