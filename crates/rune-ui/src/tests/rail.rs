//! The rail and notifications (2026-10-18): servers and folders dragged,
//! folders kept open, the hint's rows as the official client's, the
//! server's default level, mentions of the account's roles and muting a
//! direct or group conversation, through the whole window over the
//! simulation.

use super::*;
use rune_components::rail::RailVoice;
use rune_model::{NotificationChange, RoleId, ServerId, UserId, VoiceActivity, MUTED_FOREVER};

impl Host {
    /// The account's settings read, as a session starts with them.
    fn rl_load(&mut self) {
        let from = self.sent.len();
        self.sent.push(Command::LoadAccountSettings);
        self.answer_account(from);
        for event in self.provider.manage(Command::SetNotifications {
            server: None,
            change: NotificationChange::HideMuted(false),
        }) {
            self.event(event);
        }
    }

    fn rl_center(&mut self, id: &str) -> Point {
        let frame = self.frame();
        let rect = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"))
            .rect;
        Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    /// Drag the mark `source` onto the zone `zone`, as a person does, and
    /// give the provider's answer.
    fn rl_drag(&mut self, source: &str, zone: &str) {
        let from = self.sent.len();
        let start = self.rl_center(source);
        self.send(UiEvent::PointerDown(start));
        self.send(UiEvent::PointerMove(Point::new(start.x, start.y + 8.0)));
        assert!(self.ui.account.drag.is_some(), "a drag of {source}");
        // Now the zones are drawn.
        let end = self.rl_center(zone);
        self.send(UiEvent::PointerMove(end));
        self.send(UiEvent::PointerUp(end));
        assert!(self.ui.account.drag.is_none());
        self.answer_account(from);
    }

    fn rl_layout(&self) -> Vec<(Option<u64>, Vec<u64>)> {
        self.store
            .folders()
            .unwrap()
            .iter()
            .map(|folder| (folder.id, folder.servers.iter().map(|s| s.0).collect()))
            .collect()
    }

    fn rl_hint(&self, server: ServerId) -> RailVoice {
        let look = self.ui.look();
        let voice = self.store.voice_by_server();
        crate::rail::voice(
            &self.ui,
            &self.store,
            self.store.server(server).unwrap(),
            voice.get(&server),
            &look,
            rune_provider::mock::wall_clock(),
        )
    }
}

#[test]
fn servers_and_folders_are_dragged_to_reorder_them_and_make_folders() {
    let mut host = Host::new();
    host.rl_load();
    assert_eq!(
        host.rl_layout(),
        [(None, vec![1]), (None, vec![2]), (Some(1), vec![3, 4])]
    );
    // Taller de Rust onto Laboratorio limn: a new folder with both, open.
    host.rl_drag("server:1", "rail:drop:inside:server:2");
    assert_eq!(
        host.rl_layout(),
        [(Some(2), vec![2, 1]), (Some(1), vec![3, 4])]
    );
    assert!(host.ui.account.open_folders.contains(&2), "it shows open");
    let kept = host.ui.settings_to_keep();
    assert!(
        kept.contains(&("open_folders", Some("2".into()))),
        "{kept:?}"
    );
    // A folder before another, by the gap over it.
    host.rl_drag("folder:1", "rail:drop:before:folder:2:gap");
    assert_eq!(
        host.rl_layout(),
        [(Some(1), vec![3, 4]), (Some(2), vec![2, 1])]
    );
    // A server out of its open folder, after the closed one.
    host.rl_drag("server:1", "rail:drop:after:folder:1");
    assert_eq!(
        host.rl_layout(),
        [(Some(1), vec![3, 4]), (None, vec![1]), (Some(2), vec![2])]
    );
    // Dropped where it is, or nowhere, nothing is asked.
    let from = host.sent.len();
    let start = host.rl_center("server:1");
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(start.x, start.y + 8.0)));
    host.send(UiEvent::PointerMove(Point::new(start.x + 300.0, start.y)));
    host.send(UiEvent::PointerUp(Point::new(start.x + 300.0, start.y)));
    assert_eq!(host.sent.len(), from);
    // The menus still do it all.
    host.click("folder:1");
    assert!(host.ui.account.open_folders.contains(&1));
}

#[test]
fn nothing_is_dragged_while_the_provider_may_not_change_folders() {
    let mut host = Host::new();
    host.rl_load();
    host.store.connection = rune_provider::ConnectionState::Connecting;
    let start = host.rl_center("server:1");
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(start.x, start.y + 30.0)));
    assert!(host.ui.account.drag.is_none());
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id.starts_with("rail:drop:")));
}

#[test]
fn open_folders_are_kept_for_the_account_and_taken_back() {
    let mut host = Host::new();
    host.rl_load();
    let action = UiAction::Clicked {
        id: "folder:1".into(),
    };
    let intents = update(&mut host.ui, &mut host.store, &action, host.now);
    assert!(intents.iter().any(|intent| matches!(intent,
        Intent::SaveSetting { key, value } if key == "open_folders" && value.as_deref() == Some("1"))));
    assert!(crate::ACCOUNT_SETTINGS.contains(&"open_folders"));
    let mut other = Ui::default();
    other.restore_account(|key| (key == "open_folders").then(|| "1,7,x".into()));
    assert_eq!(
        other.account.open_folders,
        [1, 7]
            .into_iter()
            .collect::<std::collections::HashSet<u64>>()
    );
}

#[test]
fn the_hint_leaves_out_the_idle_channel_muted_ones_and_who_uses_an_app() {
    let mut host = Host::new();
    host.rl_load();
    let first = ServerId(1);
    let before = host.rl_hint(first);
    assert_eq!((before.stage.len(), before.listeners), (2, 3), "its stage");
    // Two of a room use an app there: they go to its own row.
    let voice = host.store.voice_by_server();
    let room = voice[&first]
        .people
        .iter()
        .find(|state| !state.streaming)
        .map(|state| state.channel)
        .unwrap();
    let users: Vec<UserId> = voice[&first]
        .people
        .iter()
        .filter(|state| state.channel == room && !state.streaming)
        .map(|state| state.user)
        .take(2)
        .collect();
    drop(voice);
    host.event(Event::VoiceActivity(VoiceActivity {
        id: format!("{room}:1"),
        channel: room,
        people: users.clone(),
    }));
    let apps = host.rl_hint(first);
    assert_eq!(apps.activities.len(), users.len());
    assert_eq!(apps.people.len(), before.people.len() - users.len());
    host.event(Event::VoiceActivity(VoiceActivity {
        id: format!("{room}:1"),
        channel: room,
        people: Vec::new(),
    }));
    assert_eq!(host.rl_hint(first), before, "the app ended");
    // A muted room says nothing of who is in it.
    let in_room = host.store.voice_in(room).len();
    let from = host.sent.len();
    host.sent.push(Command::SetNotifications {
        server: Some(first),
        change: NotificationChange::ChannelMute(room, Some(MUTED_FOREVER)),
    });
    host.answer_account(from);
    let muted = host.rl_hint(first);
    assert_eq!(
        muted.people.len() + muted.streaming.len(),
        before.people.len() + before.streaming.len() - in_room
    );
    // Nor of who uses an app there: the speaker stays as without it.
    host.event(Event::VoiceActivity(VoiceActivity {
        id: format!("{room}:1"),
        channel: room,
        people: users.clone(),
    }));
    let muted_app = host.rl_hint(first);
    assert!(muted_app.activities.is_empty());
    assert_eq!(muted_app.count(), muted.count());
    // The owned server's people are all in its idle channel.
    let owned = ServerId(2);
    let afk = host
        .store
        .server(owned)
        .unwrap()
        .options
        .afk_channel
        .unwrap();
    let idle: Vec<UserId> = host.store.voice_in(afk).iter().map(|s| s.user).collect();
    assert!(idle.len() > 1);
    assert_eq!(host.rl_hint(owned).count(), 0);
    // An app there lights nothing either.
    host.event(Event::VoiceActivity(VoiceActivity {
        id: format!("{afk}:1"),
        channel: afk,
        people: idle,
    }));
    assert_eq!(host.rl_hint(owned).count(), 0);
}

#[test]
fn an_app_shows_only_whoever_is_still_in_its_room() {
    let mut host = Host::new();
    host.rl_load();
    let first = ServerId(1);
    let voice = host.store.voice_by_server();
    let state = voice[&first]
        .people
        .iter()
        .find(|state| !state.streaming)
        .cloned()
        .unwrap();
    drop(voice);
    let gone = host
        .store
        .members(first)
        .iter()
        .map(|member| member.user)
        .find(|user| host.store.voice_state_of(*user).is_none() && Some(*user) != host.store.me())
        .unwrap();
    let before = host.rl_hint(first);
    // An app that ended unseen still names someone who left the room.
    host.event(Event::VoiceActivity(VoiceActivity {
        id: "stale".into(),
        channel: state.channel,
        people: vec![gone, state.user],
    }));
    let after = host.rl_hint(first);
    assert_eq!(after.activities.len(), before.activities.len() + 1);
    assert_eq!(after.count(), before.count());
}

#[test]
fn a_server_never_set_apart_takes_its_own_default_level() {
    let mut host = Host::new();
    host.rl_load();
    let server = ServerId(1);
    let wall = rune_provider::mock::wall_clock();
    let level = |host: &Host| crate::account::server_notify(&host.ui, &host.store, server).level;
    assert_eq!(level(&host), crate::server_menu::NotifyLevel::All);
    let mut changed = host.store.server(server).unwrap().clone();
    changed.options.mentions_only = true;
    host.event(Event::ServerChanged(changed));
    assert_eq!(level(&host), crate::server_menu::NotifyLevel::Mentions);
    // The rail marks only its mentions.
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, server, (true, 2), wall),
        (false, 2, false)
    );
    // Set apart, the account's own level wins.
    let from = host.sent.len();
    host.sent.push(Command::SetNotifications {
        server: Some(server),
        change: NotificationChange::Level(crate::server_menu::NotifyLevel::All),
    });
    host.answer_account(from);
    assert_eq!(level(&host), crate::server_menu::NotifyLevel::All);
    // Kept on this machine, the same: the default until set.
    let mut local = Host::new();
    local.local_notifications();
    let mut changed = local.store.server(server).unwrap().clone();
    changed.options.mentions_only = true;
    local.event(Event::ServerChanged(changed));
    assert_eq!(level(&local), crate::server_menu::NotifyLevel::Mentions);
}

/// Someone else's message in `channel`, new, telling `roles`.
fn to_roles(
    host: &Host,
    channel: rune_model::ChannelId,
    roles: Vec<RoleId>,
) -> rune_model::Message {
    let me = host.store.me().unwrap();
    let timeline = host.store.timeline(channel).unwrap();
    let mut message = timeline
        .messages()
        .iter()
        .find(|message| message.author != me)
        .unwrap()
        .as_ref()
        .clone();
    message.id = rune_model::MessageId(timeline.messages().last().unwrap().id.0 + 1);
    message.content = "<@&9> mirá".into();
    message.mentions.clear();
    message.mention_roles = roles;
    message
}

#[test]
fn mentions_of_the_account_s_roles_count_and_alert_unless_silenced() {
    let mut host = Host::new();
    host.rl_load();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let me = host.store.me().unwrap();
    let role = host.store.server(server).unwrap().assignable_roles()[0].id;
    let mut member = host.store.member_of(server, me).unwrap().clone();
    member.roles.retain(|known| *known != role);
    host.event(Event::MemberChanged(member.clone()));
    let wall = rune_provider::mock::wall_clock();
    // Not one of its roles: nothing.
    let other = to_roles(&host, general, vec![role]);
    assert!(!host.store.mentions_my_roles(&other));
    assert_eq!(
        message_alert(&host.ui, &host.store, &other, false, wall),
        None
    );
    member.roles.push(role);
    host.event(Event::MemberChanged(member));
    let told = to_roles(&host, general, vec![role]);
    assert!(host.store.mentions_my_roles(&told));
    assert!(message_alert(&host.ui, &host.store, &told, false, wall).is_some());
    // It counts as a mention where it waits.
    host.ui.server = None;
    let before = host.store.read(general).mentions;
    host.event(Event::MessageCreated {
        message: told.clone(),
        nonce: None,
    });
    assert_eq!(host.store.read(general).mentions, before + 1);
    // Silenced: no alert, no count.
    let from = host.sent.len();
    host.sent.push(Command::SetNotifications {
        server: Some(server),
        change: NotificationChange::SuppressRoles(true),
    });
    host.answer_account(from);
    assert!(host.store.suppresses_roles(Some(server)));
    let quiet = to_roles(&host, general, vec![role]);
    assert_eq!(
        message_alert(&host.ui, &host.store, &quiet, false, wall),
        None
    );
    let before = host.store.read(general).mentions;
    host.event(Event::MessageCreated {
        message: quiet,
        nonce: None,
    });
    assert_eq!(host.store.read(general).mentions, before);
}

#[test]
fn everyone_tells_as_the_provider_says_unless_silenced() {
    let mut host = Host::new();
    host.rl_load();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let wall = rune_provider::mock::wall_clock();
    host.ui.server = None;
    // Written by someone who may not tell everyone: the words tell nobody.
    let mut words = to_roles(&host, general, Vec::new());
    words.content = "@everyone mirá".into();
    assert_eq!(
        message_alert(&host.ui, &host.store, &words, false, wall),
        None
    );
    let before = host.store.read(general).mentions;
    host.event(Event::MessageCreated {
        message: words,
        nonce: None,
    });
    assert_eq!(host.store.read(general).mentions, before);
    let mut told = to_roles(&host, general, Vec::new());
    told.content = "@here mirá".into();
    told.mention_everyone = true;
    assert!(message_alert(&host.ui, &host.store, &told, false, wall).is_some());
    host.event(Event::MessageCreated {
        message: told,
        nonce: None,
    });
    assert_eq!(host.store.read(general).mentions, before + 1);
    // Silenced: no alert, no count.
    let from = host.sent.len();
    host.sent.push(Command::SetNotifications {
        server: Some(server),
        change: NotificationChange::SuppressEveryone(true),
    });
    host.answer_account(from);
    let mut quiet = to_roles(&host, general, Vec::new());
    quiet.mention_everyone = true;
    assert_eq!(
        message_alert(&host.ui, &host.store, &quiet, false, wall),
        None
    );
    host.event(Event::MessageCreated {
        message: quiet,
        nonce: None,
    });
    assert_eq!(host.store.read(general).mentions, before + 1);
}

#[test]
fn a_direct_or_group_conversation_is_muted_from_its_menu() {
    let mut host = Host::new();
    host.rl_load();
    // What the other person writes there next.
    let general = host.open_channel("general");
    let mut said = to_roles(&host, general, Vec::new());
    host.click("home");
    let direct = host.store.directs()[0].id;
    said.channel = direct;
    said.author = host.store.directs()[0].recipients[0];
    let wall = rune_provider::mock::wall_clock();
    assert!(message_alert(&host.ui, &host.store, &said, false, wall).is_some());
    let id = format!("channel:{direct}");
    let at = host.rl_center(&id);
    host.send(UiEvent::ContextMenu { position: at });
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Silenciar conversación"));
    let from = host.sent.len();
    let action = UiAction::Clicked {
        id: format!("group:{direct}:mute"),
    };
    let intents = update(&mut host.ui, &mut host.store, &action, host.now);
    host.run(intents);
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetNotifications {
            server: None,
            change: NotificationChange::ChannelMute(direct, Some(MUTED_FOREVER)),
        })
    );
    host.answer_account(from);
    assert!(crate::conversations::muted(
        &host.ui,
        &host.store,
        direct,
        wall
    ));
    assert_eq!(
        message_alert(&host.ui, &host.store, &said, false, wall),
        None,
        "muted, it does not alert"
    );
    // Muted, it waits without saying so on the home's mark.
    let unread = host
        .store
        .directs()
        .iter()
        .filter(|channel| host.store.read(channel.id).unread())
        .count() as u32;
    let (_, shown) = crate::conversations::directs_unread(&host.ui, &host.store, wall);
    let muted_waiting = u32::from(host.store.read(direct).unread());
    assert_eq!(shown, unread - muted_waiting);
    // And the menu turns it back on.
    host.send(UiEvent::ContextMenu { position: at });
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "Desactivar silencio"),
        "{painted:?}"
    );
    let from = host.sent.len();
    let intents = update(&mut host.ui, &mut host.store, &action, host.now);
    host.run(intents);
    host.answer_account(from);
    assert!(!crate::conversations::muted(
        &host.ui,
        &host.store,
        direct,
        wall
    ));
    assert!(message_alert(&host.ui, &host.store, &said, false, wall).is_some());
    // Kept on this machine when the provider does not keep it.
    let mut local = Host::new();
    local.local_notifications();
    let intents = update(&mut local.ui, &mut local.store, &action, local.now);
    assert!(intents
        .iter()
        .any(|intent| matches!(intent, Intent::SaveSetting { .. })));
    assert!(crate::conversations::muted(
        &local.ui,
        &local.store,
        direct,
        wall
    ));
}
