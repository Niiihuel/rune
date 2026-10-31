//! The friends page and conversations outside servers, over the simulation:
//! answers come from the simulated provider, never from the interface.

use super::*;
use rune_components::friends::ids;
use rune_components::voice::MenuEntry;
use rune_model::{RelationshipKind, UserId};

const ELENA: UserId = UserId(14);
const JULIAN: UserId = UserId(19);
const YAGO: UserId = UserId(34);
const BRUNO: UserId = UserId(11);

impl Host {
    /// Have the simulation answer every social command sent from `from` on.
    fn answer_social(&mut self, from: usize) {
        let mut next = from;
        while next < self.sent.len() {
            let command = self.sent[next].clone();
            next += 1;
            if matches!(
                command,
                Command::SendFriendRequest { .. }
                    | Command::AcceptFriendRequest { .. }
                    | Command::RemoveRelationship { .. }
                    | Command::BlockUser { .. }
                    | Command::OpenDirect { .. }
                    | Command::RenameGroup { .. }
                    | Command::AddGroupMember { .. }
                    | Command::RemoveGroupMember { .. }
                    | Command::LeaveGroup { .. }
                    | Command::CreateGroup { .. }
                    | Command::SetGroupIcon { .. }
                    | Command::CloseDirect { .. }
                    | Command::SetFriendNickname { .. }
                    | Command::SetGroupOwner { .. }
                    | Command::IgnoreUser { .. }
                    | Command::UnignoreUser { .. }
            ) {
                for event in self.provider.manage(command) {
                    self.event(event);
                }
            }
        }
    }

    fn friends(&mut self, tab: &str) {
        self.click("home");
        self.click(ids::HOME);
        self.click(&format!("{}{tab}", ids::TAB));
    }

    /// A secondary click in the middle of the control `id`.
    fn secondary(&mut self, id: &str) {
        let frame = self.frame();
        let region = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        let position = Point::new(
            region.rect.x + region.rect.width / 2.0,
            region.rect.y + region.rect.height / 2.0,
        );
        self.send(UiEvent::ContextMenu { position });
    }

    fn offers(&mut self, id: &str) -> bool {
        let frame = self.frame();
        frame
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Button)
    }
}

#[test]
fn the_page_lists_friends_and_requests_and_accepting_waits_for_the_provider() {
    let mut host = Host::new();
    host.friends("pending");
    assert!(crate::relationships::visible(&host.ui));
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "RECIBIDAS — 2"),
        "{painted:?}"
    );
    assert!(painted.iter().any(|text| text == "ENVIADAS — 1"));
    // The service's spam folder waits apart and is not counted as waiting.
    assert!(painted.iter().any(|text| text == "POSIBLE SPAM — 1"));
    assert!(painted.iter().any(|text| text == "Pendientes · 2"));
    let accept = ids::person(JULIAN, "accept");
    let sent = host.sent.len();
    host.click(&accept);
    assert_eq!(
        host.sent[sent..],
        [Command::AcceptFriendRequest { user: JULIAN }]
    );
    // Nothing changes before the answer.
    assert_eq!(
        host.store.relationship(JULIAN).unwrap().kind,
        RelationshipKind::IncomingRequest
    );
    host.answer_social(sent);
    assert_eq!(
        host.store.relationship(JULIAN).unwrap().kind,
        RelationshipKind::Friend
    );
    assert!(!host.offers(&accept));
    host.click(&format!("{}all", ids::TAB));
    assert!(host.offers(&ids::person(JULIAN, "message")));
    // Opening a conversation leaves the page.
    host.click("home");
    let direct = host.store.direct_with(BRUNO).unwrap();
    host.click(&format!("channel:{direct}"));
    assert!(!crate::relationships::visible(&host.ui));
}

#[test]
fn blocking_asks_first_and_then_takes_away_their_conversation() {
    let mut host = Host::new();
    host.friends("all");
    host.click(&ids::person(BRUNO, "more"));
    let sent = host.sent.len();
    host.click(&ids::person(BRUNO, "block"));
    assert_eq!(host.sent.len(), sent, "the question comes first");
    host.click(ids::CONFIRM_CANCEL);
    assert_eq!(host.sent.len(), sent);
    host.click(&ids::person(BRUNO, "more"));
    host.click(&ids::person(BRUNO, "block"));
    host.click(ids::CONFIRM);
    assert_eq!(host.sent[sent..], [Command::BlockUser { user: BRUNO }]);
    host.answer_social(sent);
    assert!(host.store.blocked(BRUNO));
    let direct = host.store.direct_with(BRUNO).unwrap();
    let write = host.store.abilities(direct).unwrap().write;
    assert!(write.reason().unwrap().starts_with("Bloqueaste a Bruno"));
    host.click(&format!("{}blocked", ids::TAB));
    let unblock = ids::person(BRUNO, "unblock");
    assert!(host.offers(&unblock) && host.offers(&ids::person(YAGO, "unblock")));
    let sent = host.sent.len();
    host.click(&unblock);
    assert_eq!(
        host.sent[sent..],
        [Command::RemoveRelationship { user: BRUNO }]
    );
    host.answer_social(sent);
    assert!(!host.store.blocked(BRUNO));
}

#[test]
fn a_new_direct_conversation_opens_only_once_the_provider_names_it() {
    let mut host = Host::new();
    assert!(host.store.direct_with(ELENA).is_none());
    host.friends("all");
    let message = ids::person(ELENA, "message");
    let sent = host.sent.len();
    host.click(&message);
    host.click(&message);
    let asked: Vec<_> = host.sent[sent..]
        .iter()
        .filter(|command| matches!(command, Command::OpenDirect { user, .. } if *user == ELENA))
        .collect();
    assert_eq!(asked.len(), 1, "a second click waits for the first answer");
    assert!(
        crate::relationships::visible(&host.ui),
        "nothing opens before it"
    );
    host.answer_social(sent);
    let channel = host.store.direct_with(ELENA).expect("the provider made it");
    assert_eq!(host.ui.channel(), Some(channel));
    assert!(!crate::relationships::visible(&host.ui));
    // Again from the page: the same conversation, without asking.
    host.friends("all");
    let sent = host.sent.len();
    host.click(&message);
    assert_eq!(host.ui.channel(), Some(channel));
    assert!(!host.sent[sent..]
        .iter()
        .any(|command| matches!(command, Command::OpenDirect { .. })));
}

#[test]
fn a_request_by_handle_says_how_it_went_beside_the_form() {
    let mut host = Host::new();
    host.friends("add");
    assert!(host
        .painted()
        .iter()
        .any(|text| text.starts_with("Simulación local")));
    for (handle, expected, error) in [
        (
            "nadie",
            "no encontramos a nadie con ese nombre de usuario",
            true,
        ),
        ("@zoe", "Solicitud de amistad enviada a Zoe", false),
    ] {
        host.ui.friends.username = handle.into();
        let sent = host.sent.len();
        host.click(ids::SEND);
        host.answer_social(sent);
        let painted = host.painted();
        assert!(painted.iter().any(|text| text == expected), "{painted:?}");
        assert!(
            !host.ui.notices.iter().any(|notice| notice.error),
            "the form says it, not the corner (failed: {error})"
        );
    }
    assert_eq!(
        host.store.relationship(UserId(35)).unwrap().kind,
        RelationshipKind::OutgoingRequest
    );
}

#[test]
fn a_read_only_provider_shows_the_list_but_offers_no_writes() {
    let mut host = Host::new();
    let mut info = host.provider.info();
    info.capabilities.friend_requests = false;
    info.capabilities.block_users = false;
    info.capabilities.open_direct = false;
    info.capabilities.manage_groups = false;
    host.store.provider = Some(info);
    host.friends("pending");
    assert!(!host.offers(&ids::person(JULIAN, "accept")));
    host.click(&format!("{}all", ids::TAB));
    // A friend with a conversation can still be written to; one without cannot.
    assert!(host.offers(&ids::person(BRUNO, "message")));
    assert!(!host.offers(&ids::person(ELENA, "message")));
    host.click(&format!("{}add", ids::TAB));
    assert!(!host.offers(ids::SEND));
    // Without the list at all, the page says so instead of showing nobody.
    let mut info = host.provider.info();
    info.capabilities.relationships = false;
    host.store.provider = Some(info);
    host.click(&format!("{}all", ids::TAB));
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Esta conexión no ofrece la lista de amigos"));
}

#[test]
fn a_group_is_renamed_and_left_through_the_provider_after_asking() {
    let mut host = Host::new();
    let group = host
        .store
        .directs()
        .into_iter()
        .find(|channel| channel.kind == ChannelKind::Group)
        .map(|channel| channel.id)
        .unwrap();
    host.click("home");
    host.click(&format!("channel:{group}"));
    host.click(&format!("group:{group}:manage"));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: format!("group:{group}:name"),
            value: "Equipo RUNE".into(),
        },
        host.now,
    );
    host.run(intents);
    let sent = host.sent.len();
    host.click(&format!("group:{group}:rename"));
    assert_eq!(
        host.sent[sent..],
        [Command::RenameGroup {
            channel: group,
            name: "Equipo RUNE".into()
        }]
    );
    host.answer_social(sent);
    assert_eq!(host.store.channel(group).unwrap().name, "Equipo RUNE");
    let sent = host.sent.len();
    host.click(&format!("group:{group}:add:{}", BRUNO.0));
    host.answer_social(sent);
    assert!(host
        .store
        .channel(group)
        .unwrap()
        .recipients
        .contains(&BRUNO));
    // The provider recorded the addition in the group's history.
    assert!(host
        .store
        .timeline(group)
        .unwrap()
        .messages()
        .iter()
        .any(|message| {
            message.kind == rune_model::MessageKind::RecipientAdd && message.mentions == [BRUNO]
        }));
    let sent = host.sent.len();
    let leaving = |host: &Host| {
        host.sent[sent..]
            .iter()
            .filter(
                |command| matches!(command, Command::LeaveGroup { channel } if *channel == group),
            )
            .count()
    };
    host.click(&format!("group:{group}:leave"));
    assert_eq!(leaving(&host), 0, "the question comes first");
    host.click(rune_components::group::ids::CONFIRM);
    assert_eq!(leaving(&host), 1);
    host.answer_social(sent);
    assert!(host.store.channel(group).is_none());
    assert_ne!(host.ui.channel(), Some(group));
}

#[test]
fn a_group_is_made_with_chosen_friends_once_the_provider_names_it() {
    let mut host = Host::new();
    host.click("home");
    host.click(rune_components::group::ids::NEW);
    // Bruno and Elena are friends.
    host.click(&format!(
        "{}{}",
        rune_components::group::ids::NEW_PICK,
        BRUNO.0
    ));
    host.click(&format!(
        "{}{}",
        rune_components::group::ids::NEW_PICK,
        ELENA.0
    ));
    let sent = host.sent.len();
    host.click(rune_components::group::ids::NEW_CREATE);
    // The button waits disabled; pressed anyway, it sends nothing more.
    let again = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: rune_components::group::ids::NEW_CREATE.into(),
        },
        host.now,
    );
    assert!(again.is_empty());
    let asked: Vec<_> = host.sent[sent..]
        .iter()
        .filter(|command| matches!(command, Command::CreateGroup { .. }))
        .collect();
    assert_eq!(asked.len(), 1, "a second press waits for the answer");
    assert!(matches!(asked[0], Command::CreateGroup { users, .. } if *users == [BRUNO, ELENA]));
    let before = host.store.directs().len();
    host.answer_social(sent);
    assert_eq!(host.store.directs().len(), before + 1);
    let made = host.ui.channel().expect("the provider named it");
    let group = host.store.channel(made).unwrap();
    assert_eq!(group.kind, ChannelKind::Group);
    assert_eq!(group.recipients, [BRUNO, ELENA]);
    assert!(
        !host.offers(rune_components::group::ids::NEW_CREATE),
        "the dialog closed"
    );
}

#[test]
fn a_group_is_handed_over_after_asking_and_then_offers_it_no_more() {
    let mut host = Host::new();
    host.click("home");
    host.click(rune_components::group::ids::NEW);
    for friend in [BRUNO, ELENA] {
        host.click(&format!(
            "{}{}",
            rune_components::group::ids::NEW_PICK,
            friend.0
        ));
    }
    let sent = host.sent.len();
    host.click(rune_components::group::ids::NEW_CREATE);
    host.answer_social(sent);
    let group = host.ui.channel().expect("the new group is open");
    host.click(&format!("group:{group}:manage"));
    let hand = format!("group:{group}:owner:{}", BRUNO.0);
    assert!(host.offers(&hand), "its owner may hand it over");
    let sent = host.sent.len();
    host.click(&hand);
    assert_eq!(host.sent.len(), sent, "the question comes first");
    host.click(rune_components::group::ids::CONFIRM);
    assert_eq!(
        host.sent[sent..],
        [Command::SetGroupOwner {
            channel: group,
            user: BRUNO
        }]
    );
    host.answer_social(sent);
    assert_eq!(host.store.channel(group).unwrap().owner, Some(BRUNO));
    assert!(!host.offers(&format!("group:{group}:owner:{}", ELENA.0)));
}

#[test]
fn a_direct_conversation_closes_after_asking_and_opens_again_as_it_was() {
    let mut host = Host::new();
    host.click("home");
    let direct = host.store.direct_with(BRUNO).unwrap();
    host.secondary(&format!("channel:{direct}"));
    let close = format!("group:{direct}:close");
    assert!(host.offers(&close), "the menu offers closing it");
    let sent = host.sent.len();
    host.click(&close);
    assert_eq!(host.sent.len(), sent, "the question comes first");
    host.click(rune_components::group::ids::CONFIRM);
    assert_eq!(
        host.sent[sent..],
        [Command::CloseDirect { channel: direct }]
    );
    host.answer_social(sent);
    assert!(host.store.channel(direct).is_none());
    // A group has no such item: it is left instead.
    let group = host
        .store
        .directs()
        .into_iter()
        .find(|channel| channel.kind == ChannelKind::Group)
        .map(|channel| channel.id)
        .unwrap();
    host.secondary(&format!("channel:{group}"));
    assert!(host.offers(&format!("group:{group}:leave")));
    assert!(!host.offers(&format!("group:{group}:close")));
    // From the friends page: the same conversation comes back.
    host.friends("all");
    let sent = host.sent.len();
    host.click(&ids::person(BRUNO, "message"));
    host.answer_social(sent);
    assert_eq!(host.ui.channel(), Some(direct));
}

#[test]
fn a_groups_picture_is_chosen_away_from_the_interface_and_said_to_be_simulated() {
    let mut host = Host::new();
    let group = host
        .store
        .directs()
        .into_iter()
        .find(|channel| channel.kind == ChannelKind::Group)
        .map(|channel| channel.id)
        .unwrap();
    host.click("home");
    host.click(&format!("channel:{group}"));
    host.click(&format!("group:{group}:manage"));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.starts_with("Simulación local: la imagen")));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("group:{group}:icon"),
        },
        host.now,
    );
    assert_eq!(intents, [Intent::PickGroupIcon { channel: group }]);
    // The provider takes the picture, and the history says so.
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x40\0\0\0\x40\x08\x06\0\0\0".to_vec();
    for event in host.provider.manage(Command::SetGroupIcon {
        channel: group,
        icon: rune_provider::GroupIcon::new(png.into()),
    }) {
        host.event(event);
    }
    assert!(host.store.channel(group).unwrap().icon.is_some());
    assert!(host
        .store
        .timeline(group)
        .unwrap()
        .messages()
        .iter()
        .any(|message| { message.kind == rune_model::MessageKind::ChannelIcon }));
    let sent = host.sent.len();
    host.click(&format!("group:{group}:icon:remove"));
    assert!(host.sent[sent..].contains(&Command::SetGroupIcon {
        channel: group,
        icon: None
    }));
}

#[test]
fn a_friend_gets_a_nickname_once_the_provider_confirms_it() {
    let mut host = Host::new();
    host.friends("all");
    host.click(&ids::person(BRUNO, "more"));
    host.click(&ids::person(BRUNO, "nickname"));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: ids::NICKNAME.into(),
            value: "Brunito".into(),
        },
        host.now,
    );
    host.run(intents);
    let sent = host.sent.len();
    host.click(ids::NICKNAME_SAVE);
    assert_eq!(
        host.sent[sent..],
        [Command::SetFriendNickname {
            user: BRUNO,
            nickname: Some("Brunito".into())
        }]
    );
    // Nothing changes before the answer.
    assert!(host.store.relationship(BRUNO).unwrap().nickname.is_none());
    host.answer_social(sent);
    assert_eq!(
        host.store.relationship(BRUNO).unwrap().nickname.as_deref(),
        Some("Brunito")
    );
    assert!(host.painted().iter().any(|text| text == "Brunito"));
}

#[test]
fn messages_of_someone_blocked_fold_until_shown() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let last = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .id;
    let mut said = (*host.store.timeline(channel).unwrap().messages()[0]).clone();
    said.id = rune_model::MessageId(last.0 + 1);
    said.author = YAGO;
    said.content = "un mensaje que no quieres ver".into();
    said.timestamp = rune_model::Timestamp(said.timestamp.0 + 1);
    said.reply_to = None;
    host.event(Event::MessageCreated {
        message: said.clone(),
        nonce: None,
    });
    host.tree.scroll_to_end(&timeline_id(channel));
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "1 mensaje bloqueado"),
        "{painted:?}"
    );
    assert!(!painted.iter().any(|text| text.contains("no quieres ver")));
    host.click(&rune_components::blocked::id(said.id));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("no quieres ver")));
}

/// A press of `id` as a person's menu reports it: the menu is not on screen
/// in these tests, so the press goes straight to the action.
fn choose(host: &mut Host, id: &str) {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    );
    host.run(intents);
}

/// Whether `menu` offers the item `id`.
fn offers_item(menu: &[MenuEntry], id: &str) -> bool {
    menu.iter()
        .any(|entry| matches!(entry, MenuEntry::Item(item, _, _) if item == id))
}

#[test]
fn the_ignore_item_is_offered_where_the_account_may_ignore() {
    use crate::person_menu::{entries, Place};
    let mut host = Host::new();
    let general = host.open_channel("general");
    let place = Place::Author { channel: general };
    let menu = entries(&host.ui, &host.store, BRUNO, place);
    assert!(offers_item(&menu, &ids::person(BRUNO, "ignore")));
    assert!(!offers_item(&menu, &ids::person(BRUNO, "unignore")));
    // Blocked people are not offered it, and neither is the account itself.
    assert!(!offers_item(
        &entries(&host.ui, &host.store, YAGO, place),
        &ids::person(YAGO, "ignore")
    ));
    let me = host.store.me().unwrap();
    assert!(!offers_item(
        &entries(&host.ui, &host.store, me, place),
        &ids::person(me.0, "ignore")
    ));
    // Once ignored, the item says so and lifts it.
    let sent = host.sent.len();
    choose(&mut host, &ids::person(BRUNO, "ignore"));
    host.click(ids::CONFIRM);
    host.answer_social(sent);
    let menu = entries(&host.ui, &host.store, BRUNO, place);
    assert!(offers_item(&menu, &ids::person(BRUNO, "unignore")));
    assert!(!offers_item(&menu, &ids::person(BRUNO, "ignore")));
}

#[test]
fn ignoring_asks_first_folds_their_messages_and_lifts_from_the_menu() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let last = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .id;
    let mut said = (*host.store.timeline(general).unwrap().messages()[0]).clone();
    said.id = rune_model::MessageId(last.0 + 1);
    said.author = BRUNO;
    said.content = "un mensaje que no quieres ver".into();
    said.timestamp = rune_model::Timestamp(said.timestamp.0 + 1);
    said.reply_to = None;
    host.event(Event::MessageCreated {
        message: said.clone(),
        nonce: None,
    });
    host.tree.scroll_to_end(&timeline_id(general));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("no quieres ver")));
    // Asked first: nothing is sent until «Ignorar» is pressed.
    let sent = host.sent.len();
    choose(&mut host, &ids::person(BRUNO, "ignore"));
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "¿Deseas ignorar a Bruno?"));
    assert_eq!(host.sent.len(), sent, "the question comes first");
    host.click(ids::CONFIRM_CANCEL);
    assert_eq!(host.sent.len(), sent);
    choose(&mut host, &ids::person(BRUNO, "ignore"));
    host.click(ids::CONFIRM);
    assert_eq!(host.sent[sent..], [Command::IgnoreUser { user: BRUNO }]);
    // Nothing changes before the provider answers.
    assert!(!host.store.is_ignored(BRUNO));
    host.answer_social(sent);
    assert!(host.store.is_ignored(BRUNO));
    // Their message folds into a line of its own, which shows it on request.
    let painted = host.painted();
    assert!(!painted.iter().any(|text| text.contains("no quieres ver")));
    assert!(painted.iter().any(|text| text.contains("ignorad")));
    host.click(&rune_components::blocked::ignored_id(said.id));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("no quieres ver")));
    // Lifting the ignore from the menu takes effect at once, through the provider.
    let sent = host.sent.len();
    choose(&mut host, &ids::person(BRUNO, "unignore"));
    assert_eq!(host.sent[sent..], [Command::UnignoreUser { user: BRUNO }]);
    host.answer_social(sent);
    assert!(!host.store.is_ignored(BRUNO));
    assert!(host.store.relationship(BRUNO).is_some(), "still a friend");
}

#[test]
fn the_ignore_question_offers_blocking_instead_and_asks_that_one() {
    let mut host = Host::new();
    host.friends("all");
    choose(&mut host, &ids::person(BRUNO, "ignore"));
    host.click(ids::IGNORE_BLOCK);
    let sent = host.sent.len();
    host.click(ids::CONFIRM);
    assert_eq!(host.sent[sent..], [Command::BlockUser { user: BRUNO }]);
    host.answer_social(sent);
    assert!(host.store.blocked(BRUNO) && !host.store.is_ignored(BRUNO));
}
