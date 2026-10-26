//! Message actions and voice moderation (P4), driven over the simulation:
//! forwarding, polls, who reacted, links, moments, `@silent`, the reply's
//! mention, formatting shortcuts and moderating voice.

use super::*;
use rune_components::message_actions::ids;
use rune_model::UserId;
use rune_provider::{NewPoll, VoiceModeration};

impl Host {
    /// Report `id` as clicked, as the tree would, and carry out its answer.
    fn ma_click(&mut self, id: &str) -> Vec<Intent> {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents.clone());
        intents
    }

    fn ma_input(&mut self, id: &str, value: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::InputChanged {
                id: id.into(),
                value: value.into(),
            },
            self.now,
        );
        self.run(intents);
    }

    /// Answer the commands sent since `from` as the simulation's session would.
    fn answer_actions(&mut self, from: usize) {
        let sent: Vec<Command> = self.sent[from..].to_vec();
        for command in sent {
            for event in self.provider.message_action(command) {
                self.event(event);
            }
        }
    }

    /// The newest plain message of `channel` that has been confirmed.
    fn plain_message(&self, channel: ChannelId) -> rune_model::Message {
        self.store
            .timeline(channel)
            .unwrap()
            .messages()
            .iter()
            .rev()
            .find(|message| {
                message.kind == rune_model::MessageKind::Default
                    && message.poll.is_none()
                    && !message.content.is_empty()
            })
            .map(|message| (**message).clone())
            .unwrap()
    }
}

#[test]
fn a_forward_goes_once_per_conversation_with_the_note_after_it() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let original = host.plain_message(general);
    let ayuda = host.channel_named("ayuda");
    let direct = host.store.directs()[0].id;
    host.ma_click(&format!("msg:{}:forward", original.id));
    assert!(host.ui.message_actions.forward.is_some());
    // Searching lists only where the account may write.
    host.ma_input(ids::FORWARD_QUERY, "ayu");
    host.ma_click(&ids::forward_pick(ayuda));
    host.ma_input(ids::FORWARD_QUERY, "");
    host.ma_click(&ids::forward_pick(direct));
    host.ma_input(ids::FORWARD_NOTE, "Mira esto");
    let from = host.sent.len();
    host.ma_click(ids::FORWARD_SEND);
    assert!(host.ui.message_actions.forward.is_none());
    let sent = &host.sent[from..];
    let order: Vec<(ChannelId, bool)> = sent
        .iter()
        .filter_map(|command| match command {
            Command::ForwardMessage { to, .. } => Some((*to, true)),
            Command::SendMessage {
                channel, content, ..
            } if content == "Mira esto" => Some((*channel, false)),
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        vec![
            (ayuda, true),
            (ayuda, false),
            (direct, true),
            (direct, false)
        ]
    );
    // The simulation makes the copy where it was sent.
    host.answer_actions(from);
    host.page(ayuda, None);
    assert!(host
        .store
        .timeline(ayuda)
        .unwrap()
        .messages()
        .iter()
        .any(|message| message
            .forwarded
            .as_ref()
            .is_some_and(|copy| copy.content == original.content)));
}

#[test]
fn a_poll_is_written_checked_and_comes_back_when_refused() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.ma_click(rune_components::composer::ids::POLL);
    assert!(host.ui.message_actions.poll.is_some());
    let from = host.sent.len();
    host.ma_click(ids::POLL_POST);
    assert_eq!(host.sent.len(), from, "nothing without a question");
    host.ma_input(ids::POLL_QUESTION, "¿Qué vemos?");
    host.ma_input(&format!("{}0", ids::POLL_ANSWER), "Async");
    host.ma_click(&format!("{}1", ids::POLL_EMOJI));
    host.ma_click(&format!("{}1=🧩", ids::POLL_EMOJI_PICK));
    host.ma_click(&format!("{}4", ids::POLL_HOURS));
    host.ma_click(ids::POLL_MULTI);
    host.ma_click(ids::POLL_POST);
    let Some(Command::CreatePoll {
        channel,
        poll,
        nonce,
    }) = host.sent.last().cloned()
    else {
        panic!("{:?}", host.sent.last())
    };
    assert_eq!(channel, general);
    assert_eq!(
        poll,
        NewPoll {
            question: "¿Qué vemos?".into(),
            answers: vec![
                rune_provider::NewPollAnswer {
                    text: "Async".into(),
                    emoji: None,
                },
                rune_provider::NewPollAnswer {
                    text: String::new(),
                    emoji: Some("🧩".into()),
                },
            ],
            hours: 4,
            multiselect: true,
        }
    );
    assert!(!host.painted().iter().any(|text| text == "Crear encuesta"));
    // Refused: the dialog comes back with why, and no notice repeats it.
    host.event(Event::Failed {
        failure: rune_provider::Failure::NewPoll {
            channel: general,
            nonce,
        },
        reason: "sin permiso".into(),
        ambiguous: false,
    });
    let draft = host.ui.message_actions.poll.as_ref().unwrap();
    assert_eq!(draft.posting, None);
    assert!(draft.error.as_deref().unwrap().contains("sin permiso"));
    assert!(host.ui.notices.is_empty(), "{:?}", host.ui.notices);
    // Posted again, the simulation starts it.
    host.ma_click(ids::POLL_POST);
    let from = host.sent.len() - 1;
    host.answer_actions(from);
    assert!(host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .any(|message| message
            .poll
            .as_ref()
            .is_some_and(|poll| poll.question == "¿Qué vemos?")));
}

#[test]
fn who_reacted_and_who_voted_come_by_tab_and_the_author_ends_a_poll() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let reacted = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .rev()
        .find(|message| !message.reactions.is_empty())
        .map(|message| (message.id, message.reactions[0].count))
        .unwrap();
    let from = host.sent.len();
    host.ma_click(&format!("msg:{}:reactions", reacted.0));
    assert!(matches!(
        host.sent.last(),
        Some(Command::LoadReactions { message, .. }) if *message == reacted.0
    ));
    host.answer_actions(from);
    let people = host.ui.message_actions.people.as_ref().unwrap();
    assert_eq!(people.people.len(), reacted.1 as usize);
    assert!(people.reached_end && people.request.is_none());
    host.key(Key::Escape, false, false);
    assert!(host.ui.message_actions.people.is_none());
    // The account's own open poll: its votes, and ending it after asking.
    let own = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .find(|message| {
            Some(message.author) == host.store.me()
                && message
                    .poll
                    .as_ref()
                    .is_some_and(|poll| !poll.ended(Timestamp(1_790_000_100_000)))
        })
        .map(|message| message.id)
        .expect("the simulation's open poll of the account");
    host.ma_click(&format!("msg:{own}:voters"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::LoadPollVoters { answer: 1, .. })
    ));
    host.ma_click(&format!("{}2", ids::PEOPLE_TAB));
    assert!(matches!(
        host.sent.last(),
        Some(Command::LoadPollVoters { answer: 3, .. })
    ));
    host.ma_click(ids::PEOPLE_CLOSE);
    host.ma_click(&format!("msg:{own}:end-poll"));
    assert_eq!(host.ui.message_actions.ending, Some((general, own)));
    host.ma_click("poll:end:confirm");
    assert_eq!(
        host.sent.last(),
        Some(&Command::EndPoll {
            channel: general,
            message: own
        })
    );
}

#[test]
fn an_emoji_with_only_super_reactions_has_no_tab_of_people() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    // The showcase's 🎉 has normal and super reactions, its 🚀 only super ones.
    let showcase = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .find(|message| {
            message
                .reactions
                .iter()
                .any(|reaction| reaction.count == 0 && reaction.burst > 0)
        })
        .map(|message| message.id)
        .expect("the simulation's message with super reactions alone");
    host.ma_click(&format!("msg:{showcase}:reactions"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::LoadReactions { emoji, .. }) if emoji == "🎉"
    ));
    let before = host.sent.len();
    host.ma_click(&format!("{}1", ids::PEOPLE_TAB));
    assert_eq!(host.sent.len(), before, "no tab asks for the 🚀 one");
}

#[test]
fn links_outside_ask_first_unless_trusted_and_masked_ones_say_so() {
    let mut host = Host::new();
    host.open_channel("general");
    let opened = host.ma_click("markdown_link:0:0:https://ejemplo.test/a");
    assert!(opened.is_empty(), "it asks first");
    let link = host.ui.message_actions.external.as_ref().unwrap();
    assert!(!link.masked && link.host == "ejemplo.test");
    host.ma_click(ids::LINK_TRUST);
    let opened = host.ma_click(ids::LINK_OPEN);
    assert!(opened.contains(&Intent::OpenUrl("https://ejemplo.test/a".into())));
    assert!(opened.iter().any(|intent| matches!(intent,
        Intent::SaveSetting { value: Some(value), .. } if value == "ejemplo.test")));
    // Trusted now, with its subdomains.
    let opened = host.ma_click("markdown_link:0:0:https://www.ejemplo.test/b");
    assert_eq!(
        opened,
        vec![Intent::OpenUrl("https://www.ejemplo.test/b".into())]
    );
    // A masked one asks, warning; and so does one of another domain.
    host.ma_click("link_masked:https://otro.test/c");
    assert!(host.ui.message_actions.external.as_ref().unwrap().masked);
    host.key(Key::Escape, false, false);
    assert!(host.ui.message_actions.external.is_none());
    // The provider's own hosts open at once, only over https.
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.trusted_links = &["chat.test"];
    }
    assert_eq!(
        host.ma_click("markdown_link:0:0:https://cdn.chat.test/x"),
        vec![Intent::OpenUrl("https://cdn.chat.test/x".into())]
    );
    assert!(host
        .ma_click("markdown_link:0:0:http://chat.test/x")
        .is_empty());
    // What is trusted is kept between runs.
    let kept: Vec<_> = host.ui.settings_to_keep();
    let mut restored = Ui::default();
    restored.restore(|key| {
        kept.iter()
            .find(|(kept, _)| *kept == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(restored.message_actions.trusted, vec!["ejemplo.test"]);
}

#[test]
fn a_link_to_a_conversation_opens_it_or_reads_it_by_its_id() {
    let mut host = Host::new();
    host.open_channel("general");
    let news = host.channel_named("anuncios");
    host.ma_click(&format!("place:{news}:0"));
    assert_eq!(host.ui.channel(), Some(news));
    // One not loaded is read first; a refusal leaves where it was.
    let from = host.sent.len();
    host.ma_click("place:424242:7");
    assert!(matches!(
        host.sent[from..].last(),
        Some(Command::LoadChannel {
            channel: ChannelId(424242),
            ..
        })
    ));
    host.answer_actions(from);
    assert!(host.ui.message_actions.opening.is_none());
    assert_eq!(host.ui.channel(), Some(news));
}

#[test]
fn the_showcase_link_is_a_pill_and_its_moments_read_locally() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.frame();
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "#anuncios › mensaje"),
        "{painted:?}"
    );
    assert!(painted.iter().any(|text| text.starts_with("en 2 días")
        || text.starts_with("en 1 día")
        || text.starts_with("hace")));
    assert_eq!(
        moments_tick(&host.ui, &host.store),
        Some(Duration::from_secs(30))
    );
    host.open_channel("ayuda");
    assert_eq!(moments_tick(&host.ui, &host.store), None);
    let _ = general;
    // Reached with the keyboard and activated, a moment says its date.
    host.ma_click("moment:0");
    let date = rune_components::message::full_date(rune_model::Timestamp(0));
    assert_eq!(
        host.ui.notices.last().map(|notice| &notice.title),
        Some(&date)
    );
}

#[test]
fn formatting_shortcuts_wrap_the_selection_and_take_it_back() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("hola");
    let editor = editor_id(general);
    host.frame();
    host.tree.set_input_selection(&editor, 0, 4);
    host.key(Key::B, false, true);
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("**hola**")
    );
    host.frame();
    assert_eq!(host.tree.input_selection(&editor), Some((2, 6)));
    host.key(Key::B, false, true);
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("hola")
    );
    // Nothing selected: an empty pair, the caret inside.
    host.frame();
    host.tree.set_input_selection(&editor, 4, 4);
    host.key(Key::X, true, true);
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("hola~~~~")
    );
    host.frame();
    assert_eq!(host.tree.input_selection(&editor), Some((6, 6)));
}

#[test]
fn replies_can_go_without_their_mention_and_silent_is_offered() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let answered = host.plain_message(general).id;
    host.ma_click(&format!("msg:{answered}:reply"));
    host.ma_click(rune_components::composer::ids::MENTION_REPLY);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: editor_id(general),
            value: "dale".into(),
        },
        host.now,
    );
    assert!(intents.contains(&Intent::Send {
        channel: general,
        content: "dale".into(),
        reply_to: Some(answered),
        mention_reply: false,
    }));
    host.run(intents);
    // A new reply mentions again.
    host.ma_click(&format!("msg:{answered}:reply"));
    assert!(host.ui.message_actions.mention_reply);
    host.type_text("@sil");
    let found = completions(&host.ui, &host.store, general).unwrap();
    assert!(found.items.iter().any(|item| item.insert == "@silent"));
}

#[test]
fn a_reply_with_files_keeps_its_mention_switch_and_carries_the_choice() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .attachments = true;
    crate::set_upload_limits(
        &mut host.ui,
        Some(rune_provider::UploadLimits {
            max_files: 10,
            max_file_bytes: 1024,
            max_message_bytes: 1024,
        }),
    );
    let general = host.open_channel("general");
    let answered = host.plain_message(general).id;
    host.ui.uploads.account = host.store.me();
    host.ui
        .uploads
        .drafts
        .entry(general)
        .or_default()
        .push(rune_model::PendingAttachment {
            token: rune_model::UploadToken(1),
            filename: "foto.png".into(),
            size_bytes: 12,
        });
    host.ma_click(&format!("msg:{answered}:reply"));
    let shown = crate::message_actions::composer_actions(&host.ui, &host.store, general, "");
    assert_eq!(shown.mention_reply, Some(true), "files no longer hide it");
    host.ma_click(rune_components::composer::ids::MENTION_REPLY);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: editor_id(general),
            value: "mira".into(),
        },
        host.now,
    );
    assert!(
        intents.iter().any(|intent| matches!(intent,
            Intent::SendAttachments { reply_to, mention_reply: false, .. }
                if *reply_to == Some(answered))),
        "{intents:?}"
    );
}

#[test]
fn moderation_moves_mutes_and_disconnects_within_its_reach() {
    let mut host = Host::new();
    let owned = host
        .store
        .servers()
        .iter()
        .find(|server| server.owner == host.store.me())
        .unwrap()
        .id;
    let room = host
        .store
        .channel_tree(owned)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .find(|channel| channel.kind == ChannelKind::Voice)
        .unwrap()
        .id;
    let someone = host
        .store
        .member_groups(owned)
        .into_iter()
        .flat_map(|group| group.members)
        .find(|user| Some(*user) != host.store.me())
        .unwrap();
    host.event(Event::VoiceChanged(rune_model::VoiceState {
        user: someone,
        channel: room,
        muted: false,
        deafened: false,
        speaking: false,
        streaming: false,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    }));
    let entries = crate::message_actions::voice_menu_group(&host.store, room, someone);
    let labels: Vec<String> = entries
        .iter()
        .filter_map(|entry| match entry {
            rune_components::voice::MenuEntry::Item(_, label, _)
            | rune_components::voice::MenuEntry::Danger(_, label)
            | rune_components::voice::MenuEntry::Submenu(label, _) => Some(label.clone()),
            _ => None,
        })
        .collect();
    assert!(
        labels.contains(&"Silenciar en el servidor".to_owned()),
        "{labels:?}"
    );
    // Disconnecting is drawn in the danger color, as the official client does.
    assert!(entries.iter().any(|entry| matches!(
        entry,
        rune_components::voice::MenuEntry::Danger(_, label) if label == "Desconectar"
    )));
    host.ma_click(&format!("vz:mod:mute:{someone}"));
    assert_eq!(
        host.sent.last(),
        Some(&Command::ModerateVoice {
            server: owned,
            user: someone,
            change: VoiceModeration::Mute(true),
        })
    );
    // Nobody moderates their own voice from here.
    let me = host.store.me().unwrap();
    assert!(crate::message_actions::voice_menu_group(&host.store, room, me).is_empty());
    let _ = UserId(0);
}

#[test]
fn a_voice_menu_offers_the_servers_own_actions_and_opens_their_dialogs() {
    let mut host = Host::new();
    let owned = host
        .store
        .servers()
        .iter()
        .find(|server| server.owner == host.store.me())
        .unwrap()
        .id;
    let someone = host
        .store
        .member_groups(owned)
        .into_iter()
        .flat_map(|group| group.members)
        .find(|user| Some(*user) != host.store.me())
        .unwrap();
    let entries = crate::server_settings::member_menu_group(&host.store, owned, someone);
    let mut labels = Vec::new();
    let mut role = None;
    for entry in &entries {
        match entry {
            rune_components::voice::MenuEntry::Item(_, label, _) => labels.push(label.clone()),
            // What removes or punishes is drawn in the danger color.
            rune_components::voice::MenuEntry::Danger(_, label) => {
                labels.push(format!("danger:{label}"))
            }
            rune_components::voice::MenuEntry::Submenu(label, choices) => {
                labels.push(label.clone());
                role = choices.iter().find_map(|choice| match choice {
                    rune_components::voice::MenuEntry::Role { id, has: false, .. } => {
                        Some(id.clone())
                    }
                    _ => None,
                });
            }
            _ => {}
        }
    }
    for wanted in ["Cambiar apodo", "Roles"] {
        assert!(labels.contains(&wanted.to_owned()), "{labels:?}");
    }
    for wanted in ["danger:Aislar a ", "danger:Expulsar a ", "danger:Vetar a "] {
        assert!(
            labels.iter().any(|label| label.starts_with(wanted)),
            "{labels:?}"
        );
    }
    // A role changes right away, without opening the settings, and the menu
    // stays open for the next, as the official client's does.
    let room = host
        .store
        .channel_tree(owned)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .find(|channel| channel.kind == rune_model::ChannelKind::Voice)
        .unwrap()
        .id;
    host.ui.voice.menu = Some(crate::voice::Menu {
        user: someone,
        channel: room,
        at: None,
        row: true,
    });
    host.ma_click(&role.expect("a role the owner can give"));
    assert!(host.ui.voice.menu.is_some(), "the menu stays open");
    assert!(
        matches!(host.sent.last(), Some(Command::SetMemberRoles { user, .. }) if *user == someone),
        "{:?}",
        host.sent.last()
    );
    assert!(host.ui.server_settings.is_none());
    // Kicking opens the members page with the same dialog as there.
    host.ma_click(&format!("ss:member-of:{owned}:{someone}:kick"));
    assert!(host.ui.voice.menu.is_none(), "a dialog closes it");
    let settings = host.ui.server_settings.as_ref().expect("the settings open");
    assert!(settings
        .removal
        .as_ref()
        .is_some_and(|removal| removal.user == someone && !removal.ban));
    // Nobody kicks or bans themselves from here.
    let me = host.store.me().unwrap();
    let own = crate::server_settings::member_menu_group(&host.store, owned, me);
    assert!(!own.iter().any(|entry| matches!(
        entry,
        rune_components::voice::MenuEntry::Danger(_, label) if label.starts_with("Expulsar a ")
    )));
}
