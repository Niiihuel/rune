//! Records and an app's parts as a person uses them: a poll's result goes
//! back to its poll, a voice card joins only the channel it names, and each
//! picture of an app's parts behind a spoiler waits for its own reveal.

use rune_model::components::Media;
use rune_model::{
    Component, Components, EmbedImage, EmbedKind, ImageKind, Message, MessageKind, RemoteImage,
};

use super::*;

impl Host {
    fn rc_act(&mut self, id: String) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id },
            self.now,
        );
        self.run(intents);
    }

    fn rc_find(&self, channel: ChannelId, found: impl Fn(&Message) -> bool) -> Message {
        self.store
            .timeline(channel)
            .unwrap()
            .messages()
            .iter()
            .find(|message| found(message))
            .map(|message| (**message).clone())
            .expect("the simulation has it")
    }
}

#[test]
fn a_polls_result_opens_its_poll_and_a_voice_card_joins_its_channel() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let result = host.rc_find(channel, |message| message.kind == MessageKind::PollResult);
    let poll = result.reply_to.expect("it answers its poll");
    let closed = host.rc_find(channel, |message| message.id == poll);
    assert!(closed.poll.as_ref().is_some_and(|poll| poll.finalized));
    host.rc_act(format!("msg:{}:poll-result", result.id));
    assert_eq!(crate::context::highlighted(&host.ui, channel), Some(poll));
    // Another message's «Ver encuesta» does nothing.
    host.rc_act(format!("msg:{}:poll-result", poll));

    let card = host.rc_find(channel, |message| {
        message
            .embeds
            .iter()
            .any(|embed| embed.kind == EmbedKind::VoiceChannel)
    });
    let voice = card.embeds[0].channel.expect("the simulation names it");
    let sent = host.sent.len();
    host.rc_act(format!("msg:{}:voice:{}", card.id, channel));
    assert_eq!(host.sent.len(), sent, "only the channel the card names");
    host.rc_act(format!("msg:{}:voice:{voice}", card.id));
    assert!(matches!(
        host.sent.last(),
        Some(Command::JoinVoice { channel, .. }) if *channel == voice
    ));
}

#[test]
fn pictures_of_parts_behind_a_spoiler_load_once_revealed() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut message = host.rc_find(channel, |message| message.kind == MessageKind::Default);
    message.id = MessageId(u64::MAX / 2);
    message.timestamp = Timestamp(1_790_000_050_000);
    let source = |name: &str| RemoteImage {
        url: format!("https://media.discordapp.net/attachments/1/2/{name}.png"),
        kind: ImageKind::AttachmentPreview,
    };
    let item = |name: &str| Media {
        image: Some(EmbedImage {
            source: source(name),
            dimensions: (64, 64),
        }),
        description: None,
        spoiler: true,
        explicit: false,
    };
    // A server emoji in a menu is no preview: it loads with previews off.
    let emoji = RemoteImage {
        url: "https://cdn.example.test/emojis/22.webp".into(),
        kind: ImageKind::Artwork,
    };
    message.components = Components {
        parts: vec![
            Component::Gallery(vec![item("a"), item("b")]),
            Component::Row(vec![Component::Select(rune_model::components::Select {
                chosen: vec![rune_model::components::Choice {
                    label: "Rojo".into(),
                    emoji: Some(":rojo:".into()),
                    picture: Some(emoji.clone()),
                }],
                ..Default::default()
            })]),
        ],
        whole: true,
        ..Components::default()
    };
    // The same parts in a copy it forwards number their spoilers apart.
    message.forwarded = Some(rune_model::Forwarded {
        components: Components {
            parts: vec![Component::Gallery(vec![item("c")])],
            ..Components::default()
        },
        ..rune_model::Forwarded::default()
    });
    host.event(Event::MessageCreated {
        message: message.clone(),
        nonce: None,
    });
    host.tree.scroll_to_end(&timeline_id(channel));
    host.frame();
    let wanted = |host: &Host| {
        ["a", "b", "c"]
            .map(|name| wanted_images(&host.ui, &host.store, &host.tree).contains(&source(name)))
    };
    assert_eq!(wanted(&host), [false; 3]);
    host.rc_act(format!("msg:{}:component:1:reveal", message.id));
    host.frame();
    assert_eq!(wanted(&host), [false, true, false], "only the one revealed");
    let copied = rune_components::message::first_spoiler(true, None);
    host.rc_act(format!("msg:{}:component:{copied}:reveal", message.id));
    host.frame();
    assert_eq!(wanted(&host), [false, true, true]);
    host.ui.prefs.image_previews = false;
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&emoji));
}
