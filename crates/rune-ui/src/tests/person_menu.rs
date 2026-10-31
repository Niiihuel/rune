//! The person menu outside voice channels: opened by a secondary click on a
//! member-list row or on a message's author, offering what the account may
//! do with that person, and no more.

use rune_components::voice::MenuEntry;
use rune_model::interactions::Interaction;
use rune_model::{Member, Permissions, Role, RoleId, RoleStyle, ServerId, UserId};

use super::*;
use crate::person_menu::{entries, PersonCall, Place, ASK_SETTING};
use crate::server_settings::advanced::invite_channels;

/// The labels of `menu`'s entries, in order: what it offers, on or off.
fn labels(menu: &[MenuEntry]) -> Vec<String> {
    menu.iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(_, label, _)
            | MenuEntry::Danger(_, label)
            | MenuEntry::DangerChecked(_, label, _)
            | MenuEntry::Described { label, .. }
            | MenuEntry::Inert(_, label)
            | MenuEntry::Submenu(label, _)
            | MenuEntry::Disabled(_, label, _) => Some(label.clone()),
            MenuEntry::Volume { label, .. } => Some(label.to_string()),
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
        labels(&shown).contains(&"Enviar mensaje".to_owned()),
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
fn what_the_rank_or_the_owner_blocks_is_not_offered() {
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

    // As in Discord, what they outrank or own is not offered at all.
    let place = Place::MemberList { server };
    for user in [owner, peer] {
        let shown = labels(&entries(&host.ui, &host.store, user, place));
        for prefix in ["Expulsar a ", "Banear a ", "Suspender a ", "Cambiar apodo"] {
            assert!(
                !shown.iter().any(|label| label.starts_with(prefix)),
                "{user}: {shown:?}"
            );
        }
    }
    // Nothing is hidden or off for the account itself.
    assert!(entries(&host.ui, &host.store, me, place)
        .iter()
        .all(|entry| !matches!(entry, MenuEntry::Disabled(..))));
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
    for not in ["Enviar mensaje", "Añadir amigos", "Bloquear"] {
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
    assert!(
        shown.contains(&"Iniciar una llamada".to_owned()),
        "{shown:?}"
    );
    assert!(shown.contains(&"Añadir nota".to_owned()), "{shown:?}");
    // The question comes first; its «Iniciar llamada» rings the conversation.
    host.tap(&format!("pm:{user}:call"));
    assert!(host.ui.person_call.is_some_and(|call| call.user == user));
    host.tap(rune_components::voice::ids::PERSON_CALL);
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
    // Their menu is in another server: the one it names is offered, as the invitations' rows are.
    let menu_in = host
        .store
        .servers()
        .iter()
        .map(|found| found.id)
        .find(|found| *found != server)
        .expect("another server");
    let shown = entries(
        &host.ui,
        &host.store,
        user,
        Place::MemberList { server: menu_in },
    );
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

#[test]
fn a_friend_in_a_voice_row_has_the_official_groups_in_their_order() {
    let mut host = Host::new();
    host.ui.prefs.developer = true;
    let room = host.vz_join("Sala común");
    let friend = host.vz_other(room);
    host.event(Event::RelationshipChanged(rune_model::Relationship {
        user: friend,
        kind: rune_model::RelationshipKind::Friend,
        since: None,
        nickname: Some("Pili".into()),
        spam: false,
        ignored: false,
    }));
    let shown = entries(
        &host.ui,
        &host.store,
        friend,
        Place::VoiceRow { channel: room },
    );
    let names = labels(&shown);
    // Each group where the official client puts it, and the ID last.
    let order = [
        "Perfil",
        "Enviar mensaje",
        "Cambiar apodo de amigo",
        "Volumen de usuario",
        "Silenciar",
        "Eliminar amigo",
        "Bloquear",
        "Copiar ID del usuario",
    ];
    let mut last = 0;
    for wanted in order {
        let at = names
            .iter()
            .position(|name| name == wanted)
            .unwrap_or_else(|| panic!("{wanted} in {names:?}"));
        assert!(at >= last, "{wanted} out of order in {names:?}");
        last = at;
    }
    // Separators only between groups, never first, last, or twice in a row.
    assert!(!matches!(shown.first(), Some(MenuEntry::Separator)));
    assert!(!matches!(shown.last(), Some(MenuEntry::Separator)));
    assert!(shown
        .windows(2)
        .all(|pair| !matches!(pair, [MenuEntry::Separator, MenuEntry::Separator])));
    // Removing a friend is an ordinary item, not a danger one.
    assert!(shown.contains(&MenuEntry::Item(
        format!("friend:{}:remove", friend.0),
        "Eliminar amigo".into(),
        None
    )));
    // A note is said to be theirs alone, under its label.
    assert!(
        shown.iter().any(|entry| matches!(
            entry,
            MenuEntry::Described { label, description, .. }
                if (label == "Añadir nota" || label == "Editar nota")
                    && description == "Solo tú puedes ver esto"
        )),
        "{shown:?}"
    );
}

#[test]
fn the_invitation_rows_leave_out_this_server_and_list_one_the_person_is_in() {
    let mut host = Host::new();
    let me = host.store.me().unwrap();
    let invitable = |host: &Host, server: ServerId| {
        host.store
            .server_abilities(server)
            .is_some_and(|abilities| abilities.create_invites.allowed())
            && !invite_channels(&host.store, server).is_empty()
    };
    let servers: Vec<ServerId> = host
        .store
        .servers()
        .iter()
        .map(|found| found.id)
        .filter(|server| invitable(&host, *server))
        .collect();
    let (here, there) = match servers.as_slice() {
        [here, there, ..] => (*here, *there),
        _ => panic!("two servers to invite to: {servers:?}"),
    };
    // Someone who is in `there`, and so not in the server the menu is in.
    let user = host
        .store
        .members(there)
        .iter()
        .map(|member| member.user)
        .find(|user| *user != me)
        .expect("someone who is in that server");
    let row = |host: &Host| {
        entries(
            &host.ui,
            &host.store,
            user,
            Place::MemberList { server: here },
        )
        .into_iter()
        .find_map(|entry| match entry {
            MenuEntry::Submenu(label, items) if label == "Invitar al servidor" => Some(items),
            _ => None,
        })
        .expect("an invitation submenu")
    };
    let ids: Vec<String> = row(&host)
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(id, _, _) | MenuEntry::Inert(id, _) => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert!(
        !ids.contains(&format!("pm:{user}:invite:{here}")),
        "{ids:?}"
    );
    assert!(
        ids.contains(&format!("pm:{user}:invite:{there}")),
        "{ids:?}"
    );
    // Once sent, that server's row says so, and it is off.
    host.tap(&format!("pm:{user}:invite:{there}"));
    assert!(row(&host).iter().any(|entry| matches!(
        entry,
        MenuEntry::Inert(id, label)
            if *id == format!("pm:{user}:invite:{there}") && label == "Invitación enviada"
    )));
}

#[test]
fn a_call_is_asked_about_until_the_account_says_not_to_ask_again() {
    let mut host = Host::new();
    let (channel, user) = host.first_direct();
    assert!(host.ui.prefs.ask_before_call, "asked at first");
    host.tap(&format!("pm:{user}:call"));
    assert_eq!(
        host.ui.person_call,
        Some(PersonCall {
            user,
            dont_ask_again: false,
        })
    );
    // The check box is ticked, and «Iniciar llamada» keeps the choice with the account.
    host.tap(rune_components::voice::ids::PERSON_CALL_FORGET);
    assert!(host.ui.person_call.is_some_and(|call| call.dont_ask_again));
    host.tap(rune_components::voice::ids::PERSON_CALL);
    assert!(!host.ui.prefs.ask_before_call);
    assert!(host
        .ui
        .settings_to_keep()
        .iter()
        .any(|(key, value)| *key == ASK_SETTING && value.as_deref() == Some("false")));
    assert!(
        host.sent.iter().any(|command| matches!(
            command,
            Command::RingCall { channel: rung, .. } | Command::CheckCall { channel: rung }
                if *rung == channel
        )),
        "{:?}",
        host.sent
    );
    // Next time there is no question.
    host.tap(&format!("pm:{user}:call"));
    assert!(host.ui.person_call.is_none(), "no question");
}

#[test]
fn a_call_to_someone_without_a_conversation_opens_it_and_rings_once_it_is_there() {
    let mut host = Host::new();
    host.ui.prefs.ask_before_call = false;
    let me = host.store.me().unwrap();
    let user = host
        .store
        .servers()
        .iter()
        .flat_map(|found| {
            host.store
                .members(found.id)
                .iter()
                .map(|member| member.user)
                .collect::<Vec<_>>()
        })
        .find(|user| {
            *user != me
                && host.store.direct_with(*user).is_none()
                && !host.store.user(*user).is_some_and(|account| account.bot)
                && host.store.open_direct(*user).allowed()
        })
        .expect("someone without a conversation");
    let from = host.sent.len();
    host.tap(&format!("pm:{user}:call"));
    assert!(
        host.sent[from..].iter().any(|command| matches!(
            command,
            Command::OpenDirect { user: opened, .. } if *opened == user
        )),
        "{:?}",
        host.sent
    );
    assert!(host.ui.person_calling.is_some(), "the call waits for it");
    host.answer_account(from);
    let channel = host
        .store
        .direct_with(user)
        .expect("their conversation is there");
    assert!(
        host.sent.iter().any(|command| matches!(
            command,
            Command::RingCall { channel: rung, .. } | Command::CheckCall { channel: rung }
                if *rung == channel
        )),
        "{:?}",
        host.sent
    );
    assert!(host.ui.person_calling.is_none());
}

/// The rows of the «Apps ›» submenu in `menu`; `None` when it is not offered.
fn apps_rows(menu: &[MenuEntry]) -> Option<Vec<MenuEntry>> {
    menu.iter().find_map(|entry| match entry {
        MenuEntry::Submenu(label, rows) if label == "Apps" => Some(rows.clone()),
        _ => None,
    })
}

/// How many times the interface asked the provider for a conversation's apps.
fn asked_for_apps(host: &Host) -> usize {
    host.sent
        .iter()
        .filter(|command| matches!(command, Command::LoadAppCommands { .. }))
        .count()
}

/// A member of the account's server who is a person, not a bot, and not the account.
fn a_person_listed(host: &mut Host) -> UserId {
    let me = host.store.me();
    let listed = host.listed();
    listed
        .into_iter()
        .find(|user| {
            Some(*user) != me && host.store.user(*user).is_some_and(|account| !account.bot)
        })
        .expect("someone is listed")
}

#[test]
fn apps_lists_the_user_commands_of_the_apps_once_read() {
    let mut host = Host::new();
    host.ui.members_open = true;
    let general = host.open_channel("general");
    let user = a_person_listed(&mut host);
    host.person_right_click(&format!("member:{user}"));
    let place = host.ui.person_menu.expect("their menu opens").place;
    // The menu asks for the list once, as it is laid out, and says it is read.
    host.frame();
    assert_eq!(asked_for_apps(&host), 1);
    let loading = vec![MenuEntry::Inert(
        format!("pm:{user}:apps:loading"),
        "Cargando…".into(),
    )];
    assert_eq!(
        apps_rows(&entries(&host.ui, &host.store, user, place)),
        Some(loading)
    );
    host.list_commands();
    host.frame();
    assert_eq!(asked_for_apps(&host), 1, "not asked again while open");
    // One submenu per app, holding its user commands only: «Saludar», not the chat ones.
    let rows = apps_rows(&entries(&host.ui, &host.store, user, place)).expect("Apps is offered");
    let [MenuEntry::Submenu(app, items)] = rows.as_slice() else {
        panic!("{rows:?}")
    };
    assert_eq!(app, "RuneBot (simulado)");
    let [MenuEntry::Item(greet, label, None)] = items.as_slice() else {
        panic!("{items:?}")
    };
    assert_eq!(label, "Saludar");
    assert!(greet.starts_with(&format!("pm:{user}:app-command:{}:", general.0)));
}

#[test]
fn apps_is_not_offered_for_a_bot() {
    let mut host = Host::new();
    host.ui.members_open = true;
    host.open_channel("general");
    host.person_right_click("member:9");
    let place = host.ui.person_menu.expect("the bot's menu opens").place;
    host.frame();
    let shown = entries(&host.ui, &host.store, UserId(9), place);
    assert!(apps_rows(&shown).is_none(), "{shown:?}");
    assert_eq!(asked_for_apps(&host), 0, "no list is asked for a bot");
}

#[test]
fn apps_is_not_offered_in_a_direct_conversation_with_a_person() {
    let host = Host::new();
    // Outside servers the apps are only a bot's: a person's conversation has none.
    let (channel, user) = host
        .store
        .directs()
        .into_iter()
        .filter(|channel| channel.kind == ChannelKind::Direct)
        .map(|channel| (channel.id, channel.recipients[0]))
        .find(|(_, user)| host.store.user(*user).is_some_and(|account| !account.bot))
        .expect("a direct conversation with a person");
    let shown = entries(&host.ui, &host.store, user, Place::Direct { channel });
    assert!(apps_rows(&shown).is_none(), "{shown:?}");
}

#[test]
fn choosing_a_user_command_runs_it_on_that_person_and_closes_the_menu() {
    let mut host = Host::new();
    host.ui.members_open = true;
    host.open_channel("general");
    let user = a_person_listed(&mut host);
    host.person_right_click(&format!("member:{user}"));
    let place = host.ui.person_menu.expect("their menu opens").place;
    host.frame();
    host.list_commands();
    host.frame();
    let rows = apps_rows(&entries(&host.ui, &host.store, user, place)).expect("Apps is offered");
    let [MenuEntry::Submenu(_, items)] = rows.as_slice() else {
        panic!("{rows:?}")
    };
    let [MenuEntry::Item(greet, _, _)] = items.as_slice() else {
        panic!("{items:?}")
    };
    host.tap(greet);
    assert!(host.ui.person_menu.is_none(), "choosing closes the menu");
    let (_, interaction) = host.interacted().expect("it went to the app");
    let Interaction::Command(used) = interaction else {
        panic!("{interaction:?}")
    };
    assert_eq!(used.name, "Saludar");
    assert_eq!(used.target, Some(user), "it runs on them");
    assert_eq!(used.app, UserId(9));
    assert!(used.path.is_empty() && used.values.is_empty());
}
