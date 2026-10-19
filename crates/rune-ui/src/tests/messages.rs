//! Messages as a person uses them: the bar and its menu, replying, editing
//! in place, deleting with and without asking, reactions and their picker,
//! the composer's emoji, «+» and completions, spoilers and marking unread.

use limn::{Key, Point, UiAction, UiEvent};
use rune_components::composer::ids as composer;
use rune_components::message::{editor_id as edit_field, react_ids};
use rune_model::{ChannelId, MessageId, MessageKind};
use rune_provider::Command;

use super::*;

impl Host {
    /// Write `text` in the open conversation and have it confirmed: the
    /// account's own message.
    fn mg_own(&mut self, channel: ChannelId, text: &str) -> MessageId {
        self.confirm = true;
        self.type_text(text);
        self.key(Key::Enter, false, false);
        self.frame();
        self.store
            .timeline(channel)
            .unwrap()
            .messages()
            .last()
            .unwrap()
            .id
    }

    /// The newest plain message of `channel` someone else wrote.
    fn mg_others(&self, channel: ChannelId) -> MessageId {
        let me = self.store.me();
        self.store
            .timeline(channel)
            .unwrap()
            .messages()
            .iter()
            .rev()
            .find(|message| Some(message.author) != me && message.kind == MessageKind::Default)
            .unwrap()
            .id
    }

    /// Press `id` with Shift held, as the window reports it.
    fn mg_shift_click(&mut self, id: &str) {
        let frame = self.frame();
        let region = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id && region.kind == limn::HitKind::Button)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        let center = Point::new(
            region.rect.x + region.rect.width / 2.0,
            region.rect.y + region.rect.height / 2.0,
        );
        self.send(UiEvent::PointerDownModified {
            position: center,
            shift: true,
        });
        self.send(UiEvent::PointerUp(center));
    }

    fn mg_said(&mut self, text: &str) -> bool {
        self.painted().iter().any(|painted| painted.contains(text))
    }

    fn mg_buttons(&mut self) -> Vec<String> {
        self.frame()
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
            .map(|region| region.id.clone())
            .collect()
    }

    /// What the window answers to `id` reported, without a pointer.
    fn mg_report(&mut self, id: &str) -> Vec<Intent> {
        update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        )
    }
}

#[test]
fn typing_support_is_rechecked_without_losing_drafts_or_spending_the_cooldown() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.store.provider.as_mut().unwrap().capabilities.typing = false;
    let changed = |value: &str| UiAction::InputChanged {
        id: editor_id(channel),
        value: value.into(),
    };
    let input = changed("borrador local");
    let intents = update(&mut host.ui, &mut host.store, &input, host.now);
    assert!(intents.is_empty(), "unsupported typing stays local");
    assert_eq!(host.ui.drafts.get(&channel).unwrap(), "borrador local");

    host.store.provider.as_mut().unwrap().capabilities.typing = true;
    let expected = vec![Intent::Command(Command::StartTyping { channel })];
    assert_eq!(
        update(&mut host.ui, &mut host.store, &input, host.now),
        expected,
        "a rejected notification did not consume the cooldown"
    );
    assert!(update(
        &mut host.ui,
        &mut host.store,
        &input,
        host.now + Duration::from_secs(7)
    )
    .is_empty());
    let next = host.now + Duration::from_secs(8);
    assert!(update(&mut host.ui, &mut host.store, &changed("  "), next).is_empty());
    assert_eq!(
        update(&mut host.ui, &mut host.store, &input, next),
        expected,
        "whitespace does not consume the next notification"
    );
}

#[test]
fn stale_composer_input_cannot_notify_after_permissions_or_provider_are_revoked() {
    use rune_model::Permissions;
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.store.provider.as_mut().unwrap().capabilities.typing = true;
    let original = host.store.channel(channel).unwrap().clone();
    let input = UiAction::InputChanged {
        id: editor_id(channel),
        value: "borrador conservado".into(),
    };
    for permissions in [Permissions::VIEW, Permissions::SEND, Permissions::NONE] {
        let mut revoked = original.clone();
        revoked.permissions = permissions;
        // Keep the old input event: it may have been queued before the update.
        host.store.apply(Event::ChannelChanged(revoked), host.now);
        assert!(update(&mut host.ui, &mut host.store, &input, host.now).is_empty());
        assert_eq!(host.ui.drafts.get(&channel).unwrap(), "borrador conservado");
        host.store
            .apply(Event::ChannelChanged(original.clone()), host.now);
        assert_eq!(
            update(&mut host.ui, &mut host.store, &input, host.now),
            vec![Intent::Command(Command::StartTyping { channel })]
        );
        host.now += Duration::from_secs(8);
    }
    let provider = host.store.provider.take();
    assert!(update(&mut host.ui, &mut host.store, &input, host.now).is_empty());
    host.store.provider = provider;
    host.store
        .apply(Event::ChannelRemoved { channel }, host.now);
    assert!(update(&mut host.ui, &mut host.store, &input, host.now).is_empty());
}

#[test]
fn invitation_message_actions_revalidate_the_message_account_and_capability() {
    use rune_model::{TextReference, TextTarget};
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(9_000_000_000_000_091);
    message.content = "https://fixture.invalid/invite/local".into();
    message.text_references = vec![TextReference {
        source: message.content.clone(),
        target: TextTarget::Invitation,
    }];
    let id = message.id;
    let source = message.content.clone();
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    let button = format!("msg:{id}:invitation:0:review");
    // While lookups are allowed, the card shows the server instead of «Revisar».
    assert!(!host.mg_buttons().contains(&button));
    let sent = host.sent.len();
    let intents = host.mg_report(&button);
    assert_eq!(
        intents,
        vec![Intent::ReviewInvitation {
            channel,
            message: id,
            account: host.store.me().unwrap(),
            source: source.clone()
        }]
    );
    assert!(
        host.ui.invitation.is_none(),
        "rendering and reporting never fetch metadata"
    );
    assert_eq!(host.sent.len(), sent, "a review is not an account command");
    assert_eq!(
        host.mg_report(&format!("msg:{id}:copy")),
        vec![Intent::Copy(source)]
    );
    host.ui.editing = Some(id);
    assert!(host.mg_report(&button).is_empty());
    host.ui.editing = None;
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = false;
    assert!(host.mg_report(&button).is_empty());
    assert!(!host.mg_buttons().contains(&button));
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.store.update_local(channel, id, |message| {
        message.content = "edited message".into()
    });
    assert!(host.mg_report(&button).is_empty());
    host.store.update_local(channel, id, |message| {
        message.content = message.text_references[0].source.clone()
    });
    let original = host
        .store
        .timeline(channel)
        .unwrap()
        .get(id)
        .unwrap()
        .as_ref()
        .clone();
    host.event(Event::MessageDeleted {
        channel,
        message: id,
    });
    assert!(host.mg_report(&button).is_empty());
    host.event(Event::MessageCreated {
        message: original,
        nonce: None,
    });
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert!(host.mg_report(&button).is_empty());
}

#[test]
fn a_text_naming_an_emoji_it_may_not_use_here_stays_with_why() {
    use rune_model::{CustomEmoji, EmojiId, Permissions};
    let mut host = Host::new();
    let core = host.open_channel("limn-core");
    let here = host.store.channel(core).unwrap().server.unwrap();
    let mut other = host
        .store
        .servers()
        .iter()
        .find(|server| server.id != here)
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
    host.event(Event::ServerChanged(other));
    // The account's perks unknown: the service decides (2026-10-18).
    let mine = host.mg_own(core, "ya <:far:990>");
    let sends = |host: &Host| {
        host.sent
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    Command::SendMessage { .. } | Command::EditMessage { .. }
                )
            })
            .count()
    };
    let sent = sends(&host);
    let mut room = host.store.channel(core).unwrap().clone();
    room.permissions = room.permissions.without(Permissions::EXTERNAL_EMOJI);
    host.event(Event::ChannelChanged(room));
    // In code it names nothing.
    host.type_text("`<:far:990>`");
    host.key(Key::Enter, false, false);
    assert_eq!(sends(&host), sent + 1);
    host.type_text("hola <:far:990>");
    host.key(Key::Enter, false, false);
    assert_eq!(sends(&host), sent + 1, "nothing goes");
    let notice = host.ui.notices.last().unwrap();
    assert_eq!(notice.title, "No se puede enviar el mensaje");
    assert!(
        notice
            .detail
            .as_deref()
            .is_some_and(|why| why
                .starts_with(":far: No tenés permiso para usar emojis de otros servidores")),
        "{notice:?}"
    );
    host.frame();
    assert_eq!(
        host.tree.input_value(&editor_id(core)),
        Some("hola <:far:990>"),
        "the draft stays"
    );
    // A local command and an edit say the same.
    let _ = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: editor_id(core),
            value: "/me <:far:990>".into(),
        },
        host.now,
    );
    assert_eq!(sends(&host), sent + 1);
    assert_eq!(
        host.ui.notices.last().unwrap().title,
        "No se pudo usar el comando"
    );
    host.ui.editing = Some(mine);
    let _ = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: edit_field(mine),
            value: "ahora <:far:990>".into(),
        },
        host.now,
    );
    assert_eq!(sends(&host), sent + 1, "the edit does not go");
    assert_eq!(host.ui.editing, Some(mine), "and stays open");
    assert_eq!(
        host.ui.notices.last().unwrap().title,
        "No se puede editar el mensaje"
    );
}

#[test]
fn the_more_menu_offers_what_the_account_may_do_and_the_id_in_developer_mode() {
    let mut host = Host::new();
    let core = host.open_channel("limn-core");
    let mine = host.mg_own(core, "hola");
    host.click(&format!("msg:{mine}:actions:more"));
    assert_eq!(host.ui.messages.more, Some(mine));
    for label in [
        "Agregar reacción",
        "Editar mensaje",
        "Responder",
        "Copiar texto",
        "Fijar mensaje",
        "Marcar como no leído",
        "Eliminar mensaje",
    ] {
        assert!(host.mg_said(label), "{label}");
    }
    assert!(!host.mg_said("Copiar ID del mensaje"));
    host.click(&format!("msg:{mine}:actions:more"));
    assert_eq!(host.ui.messages.more, None, "the button closes it too");
    host.ui.prefs.developer = true;
    host.click(&format!("msg:{mine}:actions:more"));
    assert!(host.mg_said("Copiar ID del mensaje"));
    assert_eq!(
        host.mg_report(&format!("msg:{mine}:copy-id")),
        vec![Intent::Copy(mine.0.to_string())],
        "the id, without a notice"
    );
    assert_eq!(host.ui.messages.more, None, "choosing closes the menu");
}

#[test]
fn a_secondary_click_on_a_message_opens_its_whole_menu_there() {
    let mut host = Host::new();
    let core = host.open_channel("limn-core");
    let mine = host.mg_own(core, "hola");
    // A point of the row, left of its bar.
    let bar = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == format!("msg:{mine}:actions:more"))
        .unwrap()
        .rect;
    let at = Point::new(bar.x - 600.0, bar.y + bar.height / 2.0);
    assert_eq!(
        host.frame().context_menu_at(at),
        Some(format!("msg:{mine}:menu").as_str())
    );
    host.send(UiEvent::ContextMenu { position: at });
    assert_eq!(host.ui.messages.context, Some((mine, at)));
    assert_eq!(host.ui.messages.more, None, "not the bar's");
    // Quick reactions first, deleting in the danger color, from the point.
    let frame = host.frame();
    let rect = |id: &str| {
        frame
            .regions
            .iter()
            .find(|region| region.id.starts_with(id))
            .unwrap_or_else(|| panic!("{id} is in the menu"))
            .rect
    };
    let quick = rect(&format!("msg:{mine}:menu-quick="));
    let reply = rect(&format!("msg:{mine}:reply"));
    let delete = rect(&format!("msg:{mine}:delete"));
    assert!(quick.y < reply.y, "{quick:?} over {reply:?}");
    let danger = host.ui.look().danger;
    assert!(frame.commands.iter().any(|command| matches!(
        command,
        limn::DrawCommand::Text { text, color, .. }
            if text == "Eliminar mensaje" && *color == danger
    )));
    // At the foot of the window it opens above the point, its corner there.
    assert!(
        delete.x >= at.x && delete.y < at.y,
        "{delete:?} from {at:?}"
    );
    host.click(&format!("msg:{mine}:reply"));
    assert_eq!(host.ui.reply_to, Some(mine));
    assert_eq!(host.ui.messages.context, None, "choosing closes the menu");
    // Asked from the keyboard, with no point, the bar's menu opens.
    // (Replying asked for the composer's focus, which the next frame gives.)
    host.frame();
    host.tree.focus(&format!("msg:{mine}:actions:more"));
    host.send(UiEvent::ContextMenuKeyboard);
    assert_eq!(host.ui.messages.more, Some(mine));
    assert_eq!(host.ui.messages.context, None);
}

#[test]
fn a_messages_menu_closes_once_its_row_leaves_the_list() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "hola");
    let list = timeline_id(general);
    host.tree.scroll_to_end(&list);
    let bar = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == format!("msg:{mine}:actions:more"))
        .unwrap()
        .rect;
    let at = Point::new(bar.x - 600.0, bar.y + bar.height / 2.0);
    host.send(UiEvent::ContextMenu { position: at });
    host.frame();
    assert_eq!(
        host.ui.messages.context,
        Some((mine, at)),
        "drawn, it stays"
    );
    // Scrolled far up, the row is not built, nor its menu: it closes, and
    // does not come back with the row.
    host.tree.set_scroll_offset(&list, 0.0);
    host.frame();
    assert_eq!(host.ui.messages.context, None);
    host.tree.scroll_to_end(&list);
    host.frame();
    assert_eq!(host.ui.messages.context, None);
    // «Más» too.
    host.click(&format!("msg:{mine}:actions:more"));
    host.frame();
    assert_eq!(host.ui.messages.more, Some(mine));
    host.tree.set_scroll_offset(&list, 0.0);
    host.frame();
    assert_eq!(host.ui.messages.more, None);
}

#[test]
fn the_menu_of_a_secondary_click_takes_the_keys_and_closes_with_its_message() {
    let mut host = Host::new();
    let core = host.open_channel("limn-core");
    let mine = host.mg_own(core, "hola");
    host.ui.reply_to = Some(mine);
    let editor = editor_id(core);
    let bar = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == format!("msg:{mine}:actions:more"))
        .unwrap()
        .rect;
    let at = Point::new(bar.x - 600.0, bar.y + bar.height / 2.0);
    host.send(UiEvent::ContextMenu { position: at });
    assert_eq!(host.ui.messages.context, Some((mine, at)));
    assert_eq!(
        host.tree.focused_id(),
        Some(editor.as_str()),
        "a secondary click leaves the focus in the empty composer"
    );
    // Up there would edit the last own message: the menu takes it.
    host.key(Key::ArrowUp, false, false);
    assert_eq!(host.ui.editing, None);
    let focused = host.tree.focused_id().unwrap_or_default().to_owned();
    assert!(focused.starts_with(&format!("msg:{mine}:")), "{focused}");
    // Escape closes the menu first; the reply stays.
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.messages.context, None);
    assert_eq!(host.ui.reply_to, Some(mine));
    // Another menu of a secondary click closes it.
    host.send(UiEvent::ContextMenu { position: at });
    assert!(host.ui.messages.context.is_some());
    let channel = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == format!("channel:{core}"))
        .unwrap()
        .rect;
    host.send(UiEvent::ContextMenu {
        position: Point::new(channel.x + 20.0, channel.y + channel.height / 2.0),
    });
    assert_eq!(host.ui.messages.context, None);
    // So does deleting the message, which takes the menu off the screen.
    host.key(Key::Escape, false, false);
    host.send(UiEvent::ContextMenu { position: at });
    assert!(host.ui.messages.context.is_some());
    host.event(Event::MessageDeleted {
        channel: core,
        message: mine,
    });
    host.frame();
    assert_eq!(host.ui.messages.context, None);
}

#[test]
fn copy_text_copies_what_was_written_with_its_markup() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "**hola** @everyone ||secreto||");
    let intents = host.mg_report(&format!("msg:{mine}:copy"));
    assert_eq!(
        intents,
        vec![Intent::Copy("**hola** @everyone ||secreto||".into())]
    );
}

fn mg_poll(host: &mut Host, channel: ChannelId, multiselect: bool) -> MessageId {
    let other = host.mg_others(channel);
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .get(other)
        .unwrap()
        .as_ref()
        .clone();
    let answer = |id, votes| rune_model::PollAnswer {
        id,
        text: format!("Opción {id}"),
        votes,
        ..Default::default()
    };
    message.poll = Some(rune_model::Poll {
        question: "¿Cuál preferís?".into(),
        answers: vec![answer(1, 2), answer(2, 0), answer(3, 1)],
        multiselect,
        results_known: true,
        ..Default::default()
    });
    host.store.apply(
        rune_provider::Event::MessageUpdated(message),
        std::time::Instant::now(),
    );
    other
}

fn mg_votes(host: &Host, channel: ChannelId, message: MessageId) -> Vec<(u32, bool)> {
    let shown = host.store.timeline(channel).unwrap().get(message).unwrap();
    shown
        .poll
        .as_ref()
        .unwrap()
        .answers
        .iter()
        .map(|answer| (answer.votes, answer.me))
        .collect()
}

#[test]
fn a_single_choice_poll_votes_on_click_and_takes_the_vote_back() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let poll = mg_poll(&mut host, general, false);
    let answer = format!("msg:{poll}:poll:answer:2");
    assert!(host.mg_buttons().contains(&answer));
    assert_eq!(
        host.mg_report(&answer),
        vec![Intent::Command(Command::VotePoll {
            channel: general,
            message: poll,
            answers: vec![2],
        })]
    );
    assert_eq!(
        mg_votes(&host, general, poll),
        [(2, false), (1, true), (1, false)]
    );
    let buttons = host.mg_buttons();
    assert!(buttons.contains(&format!("msg:{poll}:poll:remove")));
    assert!(!buttons.contains(&answer), "voted: results, not choices");
    assert!(host.mg_report(&answer).is_empty(), "no second vote on top");
    assert_eq!(
        host.mg_report(&format!("msg:{poll}:poll:remove")),
        vec![Intent::Command(Command::VotePoll {
            channel: general,
            message: poll,
            answers: vec![],
        })]
    );
    assert_eq!(
        mg_votes(&host, general, poll),
        [(2, false), (0, false), (1, false)]
    );
}

#[test]
fn a_multiselect_poll_picks_answers_then_votes_them_together() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let poll = mg_poll(&mut host, general, true);
    let vote = format!("msg:{poll}:poll:vote");
    assert!(!host.mg_buttons().contains(&vote), "nothing picked yet");
    assert!(host.mg_report(&vote).is_empty());
    for answer in [1, 3, 2, 2] {
        assert!(host
            .mg_report(&format!("msg:{poll}:poll:answer:{answer}"))
            .is_empty());
    }
    assert_eq!(host.ui.messages.poll_choice, Some((poll, vec![1, 3])));
    assert_eq!(
        mg_votes(&host, general, poll),
        [(2, false), (0, false), (1, false)]
    );
    assert!(host.mg_buttons().contains(&vote));
    assert_eq!(
        host.mg_report(&vote),
        vec![Intent::Command(Command::VotePoll {
            channel: general,
            message: poll,
            answers: vec![1, 3],
        })]
    );
    assert_eq!(host.ui.messages.poll_choice, None);
    assert_eq!(
        mg_votes(&host, general, poll),
        [(3, true), (0, false), (2, true)]
    );
}

#[test]
fn polls_offer_no_votes_without_the_ability_or_once_ended() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let poll = mg_poll(&mut host, general, false);
    let answer = format!("msg:{poll}:poll:answer:1");
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .poll_votes = false;
    assert!(!host.mg_buttons().contains(&answer));
    assert!(host.mg_said("Este proveedor no permite votar en encuestas"));
    assert!(
        host.mg_report(&answer).is_empty(),
        "the click is refused too"
    );
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .poll_votes = true;
    assert!(host.mg_buttons().contains(&answer));
    let mut ended = host
        .store
        .timeline(general)
        .unwrap()
        .get(poll)
        .unwrap()
        .as_ref()
        .clone();
    ended.poll.as_mut().unwrap().finalized = true;
    host.store.apply(
        rune_provider::Event::MessageUpdated(ended),
        std::time::Instant::now(),
    );
    let buttons = host.mg_buttons();
    let controls = format!("msg:{poll}:poll:");
    assert!(
        !buttons.iter().any(|id| id.starts_with(&controls)),
        "{buttons:?}"
    );
    assert!(host.mg_report(&answer).is_empty());
}

#[test]
fn two_quick_clicks_ask_for_a_state_each_so_the_last_one_wins() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let other = host.mg_others(general);
    let wanted = |host: &Host| match host.sent.last() {
        Some(Command::ToggleReaction { emoji, add, .. }) if emoji == "👍" => Some(*add),
        _ => None,
    };
    // Both before any answer: the second one asks for the other state.
    host.click(&format!("msg:{other}:quick=👍"));
    let first = wanted(&host).expect("a reaction");
    host.frame();
    host.click(&format!("react:{other}:👍"));
    assert_eq!(wanted(&host), Some(!first));
}

#[test]
fn the_bar_reacts_at_once_and_the_picker_finds_and_picks_with_enter() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let other = host.mg_others(general);
    host.click(&format!("msg:{other}:quick=👍"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::ToggleReaction { emoji, message, .. }) if emoji == "👍" && *message == other
    ));
    host.click(&format!("msg:{other}:react"));
    assert_eq!(host.ui.reacting, Some(other));
    host.frame();
    assert_eq!(
        host.tree.focused_id(),
        Some(rune_components::emoji::ids::search(react_ids::BASE).as_str()),
        "the search takes the focus"
    );
    host.type_text("fire");
    assert!(host
        .mg_buttons()
        .contains(&format!("{}🔥", react_ids::PICK)));
    host.key(Key::Enter, false, false);
    assert!(matches!(
        host.sent.last(),
        Some(Command::ToggleReaction { emoji, .. }) if emoji == "🔥"
    ));
    assert_eq!(host.ui.reacting, None, "it closes once chosen");
    assert_eq!(host.ui.messages.recent[0], "🔥", "and it is used lately");
    host.frame();
    assert!(
        host.mg_buttons().contains(&format!("msg:{other}:quick=🔥")),
        "first on the bar now"
    );
}

#[test]
fn replying_shows_who_is_answered_until_escape_and_sends_the_reply() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let other = host.mg_others(general);
    let author = host
        .store
        .timeline(general)
        .unwrap()
        .get(other)
        .unwrap()
        .author;
    let name = host
        .store
        .display_name(author, host.store.channel(general).unwrap().server);
    host.click(&format!("msg:{other}:reply"));
    assert!(host.mg_said("Respondiendo a ") && host.mg_said(&name));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.reply_to, None);
    assert!(!host.mg_said("Respondiendo a "));
    host.click(&format!("msg:{other}:reply"));
    host.type_text("de acuerdo");
    host.key(Key::Enter, false, false);
    assert!(matches!(
        host.sent.last(),
        Some(Command::SendMessage { reply_to: Some(answered), .. }) if *answered == other
    ));
    assert_eq!(host.ui.reply_to, None);
}

#[test]
fn editing_happens_in_place_saves_on_enter_and_cancels_on_escape() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "primero");
    host.click(&format!("msg:{mine}:edit"));
    host.frame();
    let field = edit_field(mine);
    assert_eq!(host.tree.focused_id(), Some(field.as_str()));
    assert!(host.mg_said("Presioná Escape para "), "how to leave it");
    host.type_text(" y segundo");
    host.key(Key::Enter, false, false);
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditMessage {
            channel: general,
            message: mine,
            content: "primero y segundo".into()
        })
    );
    assert_eq!(host.ui.editing, None);
    let shown = host
        .store
        .timeline(general)
        .unwrap()
        .get(mine)
        .unwrap()
        .content
        .clone();
    assert_eq!(shown, "primero y segundo", "shown at once");
    // Escape leaves it as it was.
    host.click(&format!("msg:{mine}:edit"));
    host.frame();
    host.type_text(" no");
    let sent = host.sent.len();
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.editing, None);
    assert_eq!(host.sent.len(), sent, "nothing is sent");
    // Emptied, Enter asks to delete it, without the tip about Shift.
    host.click(&format!("msg:{mine}:edit"));
    host.frame();
    host.tree.set_input_value(&field, String::new());
    host.key(Key::Enter, false, false);
    assert_eq!(host.ui.confirm_delete, Some(mine));
    assert!(host.mg_said("Eliminar mensaje"));
    assert!(!host.mg_said("CONSEJO ÚTIL"));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.confirm_delete, None);
    assert_eq!(host.ui.editing, Some(mine), "still being edited");
}

#[test]
fn an_edit_saved_with_the_link_turns_emoticons_into_emoji_as_enter_does() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.ui.prefs.convert_emoticons = true;
    let mine = host.mg_own(general, "hecho :)");
    host.click(&format!("msg:{mine}:edit"));
    host.frame();
    host.type_text(" <3");
    host.click(&format!("msg:{mine}:edit-save"));
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditMessage {
            channel: general,
            message: mine,
            content: "hecho 🙂 ❤️".into()
        })
    );
}

#[test]
fn deleting_asks_first_unless_shift_is_held() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let first = host.mg_own(general, "uno");
    host.click(&format!("msg:{first}:actions:more"));
    host.click(&format!("msg:{first}:delete"));
    assert_eq!(host.ui.confirm_delete, Some(first));
    assert!(host.mg_said("¿Seguro que querés eliminar este mensaje?"));
    assert!(
        host.mg_said("CONSEJO ÚTIL"),
        "from the menu it tells about Shift"
    );
    assert!(host.mg_said("uno"), "what it says is shown");
    host.key(Key::Enter, false, false);
    assert_eq!(
        host.sent.last(),
        Some(&Command::DeleteMessage {
            channel: general,
            message: first
        }),
        "Enter confirms"
    );
    let second = host.mg_own(general, "dos");
    host.click(&format!("msg:{second}:actions:more"));
    host.mg_shift_click(&format!("msg:{second}:delete"));
    assert_eq!(host.ui.confirm_delete, None, "nothing asked");
    assert_eq!(
        host.sent.last(),
        Some(&Command::DeleteMessage {
            channel: general,
            message: second
        })
    );
}

#[test]
fn shift_held_widens_the_bar_whose_delete_then_asks_nothing() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "uno");
    let delete = format!("msg:{mine}:delete");
    assert!(!host.mg_buttons().contains(&delete), "only in its menu");
    // Shift pressed alone: limn says the modifiers changed, with no key.
    let shift = limn::Modifiers {
        shift: true,
        ..Default::default()
    };
    host.send(UiEvent::ModifiersChanged(shift));
    let bar = host.mg_buttons();
    // The simulated account may not pin in #general: no pin among them.
    for what in ["unread", "copy", "delete"] {
        assert!(
            bar.contains(&format!("msg:{mine}:{what}")),
            "{what} in {bar:?}"
        );
    }
    assert!(!bar
        .iter()
        .any(|id| id.starts_with(&format!("msg:{mine}:quick="))));
    host.mg_shift_click(&delete);
    assert_eq!(host.ui.confirm_delete, None, "nothing asked");
    assert_eq!(
        host.sent.last(),
        Some(&Command::DeleteMessage {
            channel: general,
            message: mine
        })
    );
    // Released, or the window left, the bar is as it was.
    let other = host.mg_own(general, "dos");
    host.send(UiEvent::ModifiersChanged(shift));
    assert!(host.mg_buttons().contains(&format!("msg:{other}:unread")));
    host.send(UiEvent::ModifiersChanged(limn::Modifiers::default()));
    assert!(!host.mg_buttons().contains(&format!("msg:{other}:unread")));
    host.send(UiEvent::ModifiersChanged(shift));
    crate::window_unfocused(&mut host.ui);
    assert!(!host.mg_buttons().contains(&format!("msg:{other}:unread")));
}

#[test]
fn completions_offer_people_options_with_permission_roles_channels_and_emoji() {
    let mut host = Host::new();
    let core = host.open_channel("limn-core");
    host.type_text("@");
    let found = completions(&host.ui, &host.store, core).expect("people and more");
    assert_eq!(found.title, "MIEMBROS");
    let inserts: Vec<&str> = found
        .items
        .iter()
        .map(|item| item.insert.as_str())
        .collect();
    assert!(
        inserts.contains(&"@everyone"),
        "the owner may notify everyone"
    );
    assert!(
        inserts.contains(&"@Moderación"),
        "a role that can be mentioned"
    );
    assert!(
        !inserts.contains(&"@Colaboración"),
        "one that cannot is not offered"
    );
    host.key(Key::Escape, false, false);
    assert!(
        completions(&host.ui, &host.store, core).is_none(),
        "Escape closes them"
    );
    // Arrows choose, Enter puts it in.
    host.type_text("ever");
    let found = completions(&host.ui, &host.store, core).unwrap();
    assert_eq!(found.title, "OPCIONES QUE COINCIDEN CON @EVER");
    host.key(Key::Enter, false, false);
    host.frame();
    assert_eq!(host.tree.input_value(&editor_id(core)), Some("@everyone "));
    host.type_text("en #");
    let found = completions(&host.ui, &host.store, core).unwrap();
    assert_eq!(found.title, "CANALES DE TEXTO");
    host.type_text("l");
    let found = completions(&host.ui, &host.store, core).unwrap();
    assert_eq!(found.title, "CANALES DE TEXTO QUE COINCIDEN CON #L");
    assert!(found.items.iter().all(|item| item.insert.starts_with("#l")));
    host.key(Key::Backspace, false, false);
    let found = completions(&host.ui, &host.store, core).unwrap();
    let second = found.items[1].insert.clone();
    host.key(Key::ArrowDown, false, false);
    host.key(Key::Tab, false, false);
    host.frame();
    assert_eq!(
        host.tree.input_value(&editor_id(core)),
        Some(format!("@everyone en {second} ").as_str())
    );
    host.type_text(":fir");
    let found = completions(&host.ui, &host.store, core).unwrap();
    assert_eq!(found.items[0].insert, "🔥");
    // A plain member of another server may not notify everyone.
    let general = host.open_channel("general");
    host.type_text("@ev");
    assert!(completions(&host.ui, &host.store, general)
        .is_none_or(|found| found.items.iter().all(|item| item.insert != "@everyone")));
}

#[test]
fn the_composer_names_the_channel_offers_emoji_and_says_why_no_files() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .attachments = false;
    let general = host.open_channel("general");
    let named = host
        .frame()
        .access
        .iter()
        .find(|node| node.id == editor_id(general))
        .map(|node| node.name.clone());
    assert_eq!(named.as_deref(), Some("Enviar mensaje a #general"));
    host.click(composer::EMOJI);
    host.frame();
    assert_eq!(
        host.tree.focused_id(),
        Some(rune_components::emoji::ids::search(composer::EMOJI).as_str())
    );
    host.type_text("crab");
    host.key(Key::Enter, false, false);
    assert!(!host.ui.emoji_open);
    host.frame();
    assert_eq!(host.tree.input_value(&editor_id(general)), Some("🦀"));
    host.click(composer::PLUS);
    assert!(host.mg_said("Subir un archivo"));
    assert!(!host.mg_buttons().contains(&composer::ATTACH.to_owned()));
}

#[test]
fn a_spoiler_hides_until_clicked_and_mentions_of_everyone_are_marked() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.mg_own(general, "che @everyone mirá");
    assert!(
        !host.painted().contains(&"@everyone".to_owned()),
        "here the account may not notify everyone: plain text"
    );
    let core = host.open_channel("limn-core");
    let mine = host.mg_own(core, "atención @everyone: ||el final||");
    let spoiler = format!("msg:{mine}:spoiler:0");
    assert!(host.mg_buttons().contains(&spoiler));
    host.click(&spoiler);
    assert_eq!(host.ui.messages.revealed.get(&mine), Some(&vec![0]));
    assert!(
        !host.mg_buttons().contains(&spoiler),
        "shown, it is plain text"
    );
    assert!(
        host.painted().contains(&"@everyone".to_owned()),
        "a pill of its own"
    );
}

#[test]
fn local_read_scope_keeps_unread_navigation_without_sending_unsupported_commands() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let other = host.mg_others(general);
    host.store.provider.as_mut().unwrap().capabilities = Default::default();
    let through = host.store.read(general).last_read;

    let intents = host.mg_report(&format!("msg:{other}:unread"));
    assert!(
        intents.is_empty(),
        "local markers must not request remote writes"
    );
    assert!(host.store.read(general).unread());
    assert_eq!(host.store.read(general).last_read, through);
    assert_eq!(host.store.read(general).unread_from, Some(other));
    assert_eq!(host.ui.new_marker.get(&general), Some(&other));
    host.frame();
    assert!(host.mg_said("Marcar como leído (solo en RUNE)"));

    host.open_channel("ayuda");
    host.open_channel("general");
    assert!(host.mg_said("NUEVO"));
    host.click(&format!("msg:{other}:actions:more"));
    assert!(host.mg_said("Marcar como no leído (solo en RUNE)"));
    host.key(Key::Escape, false, false);

    let intents = host.mg_report(crate::messages::UNREAD_CLEAR);
    assert!(intents.is_empty());
    assert!(!host.store.read(general).unread());
    assert!(host.store.read(general).unread_from.is_none());
    assert!(!host.ui.new_marker.contains_key(&general));
}

#[test]
fn marking_unread_keeps_the_line_and_the_bar_until_marked_read() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let other = host.mg_others(general);
    host.click(&format!("msg:{other}:actions:more"));
    let before = host.sent.len();
    host.click(&format!("msg:{other}:unread"));
    let sent = host.sent.len();
    host.frame();
    host.frame();
    assert!(host.mg_said("NUEVO"));
    assert!(host.mg_said("desde las"), "{:?}", host.painted());
    assert!(
        !host.sent[sent..]
            .iter()
            .any(|command| matches!(command, Command::MarkRead { .. })),
        "not marked read while it is open"
    );
    assert!(
        !host.sent[before..]
            .iter()
            .any(|command| matches!(command, Command::MarkRead { .. })),
        "a manual reminder is local"
    );
    assert!(host.store.read(general).unread());
    host.click(crate::messages::UNREAD_CLEAR);
    assert!(!host.mg_said("NUEVO"));
    assert!(host.store.read(general).unread_from.is_none());
    assert!(!host.store.read(general).unread());
    // Left unread, it stays so in the list and shows the line when back.
    host.click(&format!("msg:{other}:actions:more"));
    host.click(&format!("msg:{other}:unread"));
    host.open_channel("ayuda");
    assert!(host.store.read(general).unread());
    host.open_channel("general");
    assert!(host.mg_said("NUEVO"));
}

#[test]
fn both_pickers_show_nine_whole_columns_and_the_whole_search() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let other = host.mg_others(general);
    let check = |host: &mut Host, base: &str| {
        let frame = host.frame();
        let rect = |id: &str| {
            frame
                .regions
                .iter()
                .find(|region| region.id == id)
                .unwrap_or_else(|| panic!("{id}"))
                .rect
        };
        let search = rect(&rune_components::emoji::ids::search(base));
        assert!((search.width - 289.0).abs() < 1.0, "{search:?}");
        let grid = rect(&rune_components::emoji::ids::grid(base));
        let prefix = format!("{base}=");
        let cells: Vec<limn::Rect> = frame
            .regions
            .iter()
            .filter(|region| region.id.starts_with(&prefix))
            .map(|region| region.rect)
            .collect();
        // The widest row: the frequent ones may fill fewer.
        let whole = cells
            .iter()
            .map(|row| {
                cells
                    .iter()
                    .filter(|cell| cell.y == row.y && cell.x + cell.width <= grid.x + grid.width)
                    .count()
            })
            .max();
        assert_eq!(whole, Some(9), "{grid:?} {cells:?}");
    };
    host.click(composer::EMOJI);
    check(&mut host, composer::EMOJI);
    host.key(Key::Escape, false, false);
    host.click(&format!("msg:{other}:react"));
    check(&mut host, react_ids::BASE);
}

#[test]
fn up_edits_the_last_own_message_even_out_of_view() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "el mío");
    let theirs = host.mg_others(general);
    let template = (**host.store.timeline(general).unwrap().get(theirs).unwrap()).clone();
    for step in 1..=60 {
        let message = rune_model::Message {
            id: MessageId(mine.0 + step),
            content: format!("otro {step}"),
            ..template.clone()
        };
        host.event(rune_provider::Event::MessageCreated {
            message,
            nonce: None,
        });
    }
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(editor_id(general).as_str()));
    host.key(Key::ArrowUp, false, false);
    host.frame();
    assert_eq!(host.ui.editing, Some(mine));
    assert_eq!(
        host.tree.focused_id(),
        Some(edit_field(mine).as_str()),
        "its row is shown and its editor takes the keys"
    );
    host.type_text("X");
    assert_eq!(host.tree.input_value(&editor_id(general)), Some(""));
}

#[test]
fn shift_only_skips_asking_on_the_click_it_was_held_for() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "uno");
    // Shift+click in the composer, then delete from the menu by keyboard.
    let field = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == editor_id(general))
        .unwrap()
        .rect;
    let inside = Point::new(field.x + 10.0, field.y + 10.0);
    host.send(UiEvent::PointerDownModified {
        position: inside,
        shift: true,
    });
    host.send(UiEvent::PointerUp(inside));
    host.mg_report(&format!("msg:{mine}:actions:more"));
    host.mg_report(&format!("msg:{mine}:delete"));
    assert_eq!(host.ui.confirm_delete, Some(mine), "asked");
    host.key(Key::Escape, false, false);
    host.send(UiEvent::PointerDownModified {
        position: inside,
        shift: true,
    });
    host.send(UiEvent::PointerUp(inside));
    host.click(&format!("msg:{mine}:actions:more"));
    host.frame();
    host.tree.focus(&format!("msg:{mine}:delete"));
    host.key(Key::Enter, false, false);
    assert_eq!(host.ui.confirm_delete, Some(mine), "asked again");
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::DeleteMessage { .. })));
}

#[test]
fn pinning_from_the_menu_closes_it() {
    let mut host = Host::new();
    let core = host.open_channel("limn-core");
    let mine = host.mg_own(core, "fijame");
    host.click(&format!("msg:{mine}:actions:more"));
    host.click(&format!("msg:{mine}:pin"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::PinMessage { message, .. }) if *message == mine
    ));
    assert_eq!(host.ui.messages.more, None);
}

#[test]
fn going_elsewhere_drops_the_delete_dialog() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let mine = host.mg_own(general, "uno");
    host.click(&format!("msg:{mine}:actions:more"));
    host.click(&format!("msg:{mine}:delete"));
    assert_eq!(host.ui.confirm_delete, Some(mine));
    host.key(Key::K, false, true);
    host.type_text("ayuda");
    host.key(Key::Enter, false, false);
    let ayuda = host.channel_named("ayuda");
    assert_eq!(host.ui.channel(), Some(ayuda));
    assert_eq!(host.ui.confirm_delete, None);
    host.frame();
    host.type_text("hola ayuda");
    host.key(Key::Enter, false, false);
    assert!(matches!(
        host.sent.last(),
        Some(Command::SendMessage { channel, .. }) if *channel == ayuda
    ));
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::DeleteMessage { .. })));
}
