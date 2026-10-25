//! The person menu outside voice channels: opened by a secondary click on a
//! member-list row or on a message's author, offering what the account may
//! do with that person, and no more.

use rune_components::voice::MenuEntry;
use rune_model::{Member, Permissions, Role, RoleId, RoleStyle, ServerId, UserId};

use super::*;
use crate::person_menu::{entries, Place};
use crate::server_settings::advanced::invite_channels;

/// The labels of `menu`'s entries, in order: what it offers, on or off.
fn labels(menu: &[MenuEntry]) -> Vec<String> {
    menu.iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(_, label, _)
            | MenuEntry::Danger(_, label)
            | MenuEntry::Submenu(label, _)
            | MenuEntry::Disabled(_, label, _) => Some(label.clone()),
            _ => None,
        })
        .collect()
}

/// What is off in `menu`: each label with the reason it cannot be chosen.
fn off(menu: &[MenuEntry]) -> Vec<(String, String)> {
    menu.iter()
        .filter_map(|entry| match entry {
            MenuEntry::Disabled(_, label, reason) => Some((label.clone(), reason.clone())),
            _ => None,
        })
        .collect()
}

impl Host {
    /// A press of `id`, as the tree reports a click on it.
    fn tap(&mut self, id: &str) {
        let action = UiAction::Clicked { id: id.into() };
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
    }

    /// The direct conversation the home opens first: its id and person.
    fn first_direct(&mut self) -> (ChannelId, UserId) {
        let direct = self
            .store
            .directs()
            .into_iter()
            .find(|channel| channel.kind == ChannelKind::Direct)
            .map(|channel| (channel.id, channel.recipients[0]))
            .expect("a direct conversation");
        self.click("home");
        self.click(&format!("channel:{}", direct.0));
        direct
    }

    /// A secondary click in the middle of the control `id`, as a person makes it.
    fn person_right_click(&mut self, id: &str) {
        let frame = self.frame();
        let region = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        let position = limn::Point::new(
            region.rect.x + region.rect.width / 2.0,
            region.rect.y + region.rect.height / 2.0,
        );
        self.send(UiEvent::ContextMenu { position });
    }

    /// The members the member list shows now.
    fn listed(&mut self) -> Vec<UserId> {
        self.frame()
            .regions
            .iter()
            .filter_map(|region| region.id.strip_prefix("member:")?.parse().ok())
            .map(UserId)
            .collect()
    }

    /// Open `server` at one of its text channels, with its member list.
    fn open_server(&mut self, server: ServerId) {
        let channel = self
            .store
            .channel_tree(server)
            .into_iter()
            .flat_map(|(_, channels)| channels)
            .find(|channel| channel.kind == ChannelKind::Text)
            .expect("a text channel")
            .id;
        self.ui.members_open = true;
        self.click(&format!("server:{server}"));
        self.click(&format!("channel:{channel}"));
        self.frame();
    }
}

/// A server the account owns, and one it is a plain member of.
fn owned_and_plain(host: &Host) -> (ServerId, ServerId) {
    let me = host.store.me();
    let owned = host
        .store
        .servers()
        .iter()
        .find(|server| server.owner == me)
        .expect("an owned server")
        .id;
    let plain = host
        .store
        .servers()
        .iter()
        .find(|server| {
            server.owner != me
                && host
                    .store
                    .server_abilities(server.id)
                    .is_some_and(|abilities| !abilities.kick.allowed())
        })
        .expect("a server the account is a plain member of")
        .id;
    (owned, plain)
}

#[test]
fn a_secondary_click_on_a_member_opens_their_menu_in_the_list() {
    let mut host = Host::new();
    host.ui.members_open = true;
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let user = host
        .listed()
        .into_iter()
        .find(|user| Some(*user) != host.store.me())
        .expect("someone is listed");
    host.person_right_click(&format!("member:{user}"));
    let menu = host.ui.person_menu.expect("their menu opens");
    assert_eq!(
        (menu.user, menu.place),
        (user, Place::MemberList { server })
    );
    assert!(host.ui.voice.menu.is_none());
    let shown = entries(&host.ui, &host.store, user, menu.place);
    // Their profile opens their full profile, as outside a call.
    assert!(
        shown.contains(&MenuEntry::Item(
            format!("pp:full:{user}"),
            "Perfil".into(),
            None
        )),
        "{shown:?}"
    );
    assert_eq!(
        labels(&shown).contains(&"Mensaje".to_owned()),
        host.store.social(user).message.allowed()
    );
}

#[test]
fn moderation_is_offered_only_where_the_account_may_use_it() {
    let mut host = Host::new();
    let (owned, plain) = owned_and_plain(&host);
    let me = host.store.me();
    for (server, moderates) in [(owned, true), (plain, false)] {
        host.open_server(server);
        let user = host
            .listed()
            .into_iter()
            .find(|user| Some(*user) != me)
            .expect("someone is listed");
        host.person_right_click(&format!("member:{user}"));
        let menu = host.ui.person_menu.expect("their menu opens");
        let shown = labels(&entries(&host.ui, &host.store, user, menu.place));
        let punishes = shown.iter().any(|label| label.starts_with("Expulsar a "));
        assert_eq!(punishes, moderates, "{server}: {shown:?}");
        assert_eq!(
            shown.contains(&"Cambiar apodo".to_owned()),
            moderates,
            "{server}: {shown:?}"
        );
        host.ui.person_menu = None;
    }
}

#[test]
fn what_the_rank_or_the_owner_blocks_is_off_with_its_reason() {
    let mut host = Host::new();
    let me = host.store.me().unwrap();
    let (_, server) = owned_and_plain(&host);
    let owner = host.store.server(server).unwrap().owner.unwrap();
    let peer = host
        .store
        .member_groups(server)
        .into_iter()
        .flat_map(|group| group.members)
        .find(|user| *user != me && *user != owner)
        .expect("someone else is a member");
    // A role the account has, ranked at 50 and allowed to kick, ban and time out.
    let role = RoleId(900);
    let mut found = host.store.server(server).unwrap().clone();
    found.roles.push(Role {
        id: role,
        name: "Moderación".into(),
        color: None,
        position: 50,
        hoisted: false,
        mentionable: false,
        permissions: Permissions::KICK
            | Permissions::BAN
            | Permissions::MANAGE_NICKS
            | Permissions::TIMEOUT_MEMBERS,
        style: RoleStyle::default(),
    });
    host.event(Event::ServerChanged(found));
    let mut mine: Member = host.store.member_of(server, me).unwrap().clone();
    mine.roles.push(role);
    host.event(Event::MemberChanged(mine));
    // Another with the same role is no one the account outranks.
    let mut theirs: Member = host.store.member_of(server, peer).unwrap().clone();
    theirs.roles.push(role);
    host.event(Event::MemberChanged(theirs));
    assert!(host
        .store
        .server_abilities(server)
        .is_some_and(|abilities| abilities.kick.allowed()));

    let place = Place::MemberList { server };
    let owner_off = off(&entries(&host.ui, &host.store, owner, place));
    assert!(
        owner_off
            .iter()
            .any(|(label, reason)| label.starts_with("Expulsar a ") && reason.contains("dueño")),
        "{owner_off:?}"
    );
    let peer_off = off(&entries(&host.ui, &host.store, peer, place));
    assert!(
        peer_off
            .iter()
            .any(|(label, reason)| label.starts_with("Expulsar a ") && reason.contains("altura")),
        "{peer_off:?}"
    );
    // Nothing is off for the account itself.
    assert!(off(&entries(&host.ui, &host.store, me, place)).is_empty());
}

#[test]
fn a_secondary_click_on_an_author_opens_their_menu_not_the_messages() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let (id, author) = host
        .frame()
        .regions
        .iter()
        .find_map(|region| {
            let message: u64 = region.id.strip_prefix("pp:name:")?.parse().ok()?;
            let author = host
                .store
                .timeline(general)?
                .get(rune_model::MessageId(message))?
                .author;
            Some((region.id.clone(), author))
        })
        .expect("an author's name is on screen");
    host.person_right_click(&id);
    let menu = host.ui.person_menu.expect("the person's menu opens");
    assert_eq!(
        (menu.user, menu.place),
        (author, Place::Author { channel: general })
    );
    assert!(host.ui.messages.context.is_none());
}

#[test]
fn the_account_s_own_menu_offers_no_one_else_s_actions() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let me = host.store.me().unwrap();
    let shown = labels(&entries(
        &host.ui,
        &host.store,
        me,
        Place::MemberList { server },
    ));
    assert!(shown.contains(&"Perfil".to_owned()), "{shown:?}");
    for not in ["Mensaje", "Añadir amigo", "Bloquear"] {
        assert!(!shown.contains(&not.to_owned()), "{shown:?}");
    }
    assert!(!shown.iter().any(|label| label.starts_with("Expulsar a ")));
}

#[test]
fn a_direct_conversation_offers_a_call_that_rings_it_and_a_note() {
    let mut host = Host::new();
    let (channel, user) = host.first_direct();
    let shown = labels(&entries(
        &host.ui,
        &host.store,
        user,
        Place::Direct { channel },
    ));
    assert!(shown.contains(&"Llamar".to_owned()), "{shown:?}");
    assert!(shown.contains(&"Añadir nota".to_owned()), "{shown:?}");
    host.tap(&format!("pm:{user}:call"));
    let rang = host.sent.iter().any(|command| {
        matches!(command, Command::RingCall { channel: rung, .. } | Command::CheckCall { channel: rung }
            if *rung == channel)
    });
    assert!(
        rang,
        "the call asks to ring the conversation: {:?}",
        host.sent
    );
    // The note is the profile's, where it is written.
    host.tap(&format!("pp:full-note:{user}"));
    assert_eq!(host.ui.profiles.full.map(|(who, _)| who), Some(user));
}

#[test]
fn hiding_a_camera_is_a_choice_of_this_device() {
    let mut host = Host::new();
    let (_, user) = host.first_direct();
    let voice = host
        .store
        .channel_tree(host.store.servers()[0].id)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .find(|channel| channel.kind == ChannelKind::Voice)
        .expect("a voice channel")
        .id;
    let video = |host: &Host| {
        entries(
            &host.ui,
            &host.store,
            user,
            Place::VoiceRow { channel: voice },
        )
        .into_iter()
        .find_map(|entry| match entry {
            MenuEntry::Item(id, label, checked) if id == format!("pm:{user}:video") => {
                Some((label, checked))
            }
            _ => None,
        })
    };
    // Offered while their camera is on or hidden; hidden now, so checked.
    host.ui.prefs.hidden_video.insert(user.0);
    assert_eq!(
        video(&host),
        Some(("Desactivar video".to_owned(), Some(true)))
    );
    host.tap(&format!("pm:{}:video", user.0));
    assert!(!host.ui.prefs.video_hidden(user.0));
    // Shown again only while their camera is on, and then unchecked.
    assert!(video(&host).is_none_or(|(_, checked)| checked == Some(false)));
}

#[test]
fn an_invitation_to_a_server_is_sent_by_their_conversation() {
    let mut host = Host::new();
    let me = host.store.me().unwrap();
    let invitable = |host: &Host, server: ServerId| {
        host.store
            .server_abilities(server)
            .is_some_and(|abilities| abilities.create_invites.allowed())
            && !invite_channels(&host.store, server).is_empty()
    };
    let server = host
        .store
        .servers()
        .iter()
        .find(|found| invitable(&host, found.id))
        .expect("a server the account may invite to")
        .id;
    let user = host
        .store
        .servers()
        .iter()
        .filter(|found| found.id != server)
        .flat_map(|found| {
            host.store
                .members(found.id)
                .iter()
                .map(|member| member.user)
        })
        .find(|user| *user != me && host.store.member_of(server, *user).is_none())
        .expect("someone who is not in that server");
    let shown = entries(&host.ui, &host.store, user, Place::MemberList { server });
    let invites = shown.iter().find_map(|entry| match entry {
        MenuEntry::Submenu(label, items) if label == "Invitar al servidor" => Some(items),
        _ => None,
    });
    assert!(
        invites.is_some_and(|items| items.iter().any(|entry| matches!(
            entry,
            MenuEntry::Item(id, _, _) if *id == format!("pm:{user}:invite:{server}")
        ))),
        "{shown:?}"
    );
    let from = host.sent.len();
    host.tap(&format!("pm:{user}:invite:{server}"));
    let created = host.sent.len();
    host.answer_account(from);
    host.answer_account(created);
    assert!(
        host.sent.iter().any(
            |command| matches!(command, Command::SendMessage { content, .. } if !content.is_empty())
        ),
        "the link is sent: {:?}",
        host.sent
    );
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "Invitación enviada"));
}
