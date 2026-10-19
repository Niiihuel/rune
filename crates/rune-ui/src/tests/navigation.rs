//! Back and forward, F6 between regions, the shortcuts list and Escape on
//! every dialog, over the laid-out limn tree. Nothing here exercises a real
//! screen reader: the access nodes are what limn would publish to one.

use rune_model::UserId;

use super::*;

impl Host {
    fn travel(&mut self, back: bool) {
        let intents = travel(&mut self.ui, &mut self.store, back);
        self.run(intents);
        self.frame();
        self.frame();
    }

    fn f6(&mut self, reverse: bool) -> Option<String> {
        self.frame();
        let intents = cycle_focus(&self.ui, &self.tree, reverse);
        self.run(intents);
        self.frame();
        self.tree.focused_id().map(str::to_owned)
    }

    /// Every control limn would publish has a name a screen reader can say.
    fn assert_named(&mut self, what: &str) {
        let frame = self.frame();
        let unnamed: Vec<_> = frame
            .access
            .iter()
            .filter(|node| node.actionable && node.name.trim().is_empty())
            .map(|node| format!("{} ({:?})", node.id, node.role))
            .collect();
        assert!(
            unnamed.is_empty(),
            "{what}: controls without a name: {unnamed:?}"
        );
    }

    fn first_in_view(&self, channel: ChannelId) -> MessageId {
        let rows = self.tree.visible_rows(&timeline_id(channel)).unwrap();
        self.store.timeline(channel).unwrap().messages()[rows.start].id
    }

    /// Go to `channel` as the quick switcher does, from any server.
    fn click_channel(&mut self, channel: ChannelId) {
        let server = self.store.channel(channel).unwrap().server;
        let intents = open(&mut self.ui, &mut self.store, server, channel);
        self.run(intents);
        self.frame();
        self.frame();
    }
}

#[test]
fn back_and_forward_return_to_the_conversations_visited() {
    let mut host = Host::new();
    let start = host.ui.channel().unwrap();
    let general = host.channel_named("general");
    let slow = host.channel_named("rendimiento");
    host.frame();
    host.click_channel(general);
    host.click_channel(slow);
    host.travel(true);
    assert_eq!(host.ui.channel(), Some(general));
    host.travel(true);
    assert_eq!(host.ui.channel(), Some(start));
    host.travel(true);
    assert_eq!(host.ui.channel(), Some(start), "nothing further back");
    host.travel(false);
    assert_eq!(host.ui.channel(), Some(general));
    host.travel(false);
    assert_eq!(host.ui.channel(), Some(slow));
    // A new place drops what was ahead.
    host.travel(true);
    host.click_channel(start);
    host.travel(false);
    assert_eq!(
        host.ui.channel(),
        Some(start),
        "nothing ahead after a new place"
    );
    host.travel(true);
    assert_eq!(host.ui.channel(), Some(general));
}

#[test]
fn back_restores_the_message_in_view_and_the_focused_control() {
    let mut host = Host::new();
    let slow = host.open_channel("rendimiento");
    // Read something in the middle, away from the end.
    host.tree.scroll_to_index(&timeline_id(slow), 10);
    host.frame();
    host.frame();
    let watched = host.first_in_view(slow);
    assert!(!host.tree.is_scroll_at_end(&timeline_id(slow), 24.0));
    assert!(host.tree.focus("members:toggle") || host.tree.focus("jump:latest"));
    let focused = host.tree.focused_id().unwrap().to_owned();
    host.frame();
    let general = host.channel_named("general");
    host.click_channel(general);
    assert_eq!(
        host.tree.focused_id(),
        Some(editor_id(general).as_str()),
        "opening focuses the composer"
    );
    host.travel(true);
    host.frame();
    assert_eq!(host.ui.channel(), Some(slow));
    let rows = host.tree.visible_rows(&timeline_id(slow)).unwrap();
    let index = host
        .store
        .timeline(slow)
        .unwrap()
        .messages()
        .iter()
        .position(|message| message.id == watched)
        .unwrap();
    assert!(
        rows.contains(&index),
        "{watched} is in view again: {rows:?}"
    );
    assert!(!host.tree.is_scroll_at_end(&timeline_id(slow), 24.0));
    assert_eq!(host.tree.focused_id(), Some(focused.as_str()));
    assert_eq!(
        crate::context::highlighted(&host.ui, slow),
        None,
        "a return is not a highlighted jump"
    );
}

#[test]
fn back_restores_the_scroll_to_the_pixel_not_only_the_row() {
    let mut host = Host::new();
    let slow = host.open_channel("rendimiento");
    let list = timeline_id(slow);
    host.tree.scroll_to_index(&list, 10);
    host.frame();
    host.frame();
    // Part of the first row in view scrolled past its top.
    let offset = host.tree.scroll_offset(&list);
    host.tree.set_scroll_offset(&list, offset + 13.0);
    host.frame();
    host.frame();
    let watched = host.first_in_view(slow);
    let anchor = format!("anchor:{watched}");
    let before = host.tree.anchor_offset(&anchor).unwrap();
    assert!(before < -1.0, "the row is part way out: {before}");
    let general = host.channel_named("general");
    host.click_channel(general);
    host.travel(true);
    for _ in 0..4 {
        host.frame();
    }
    assert_eq!(host.ui.channel(), Some(slow));
    assert_eq!(host.first_in_view(slow), watched);
    let after = host.tree.anchor_offset(&anchor).unwrap();
    assert!(
        (after - before).abs() < 1.0,
        "{watched} stands where it stood: {before} → {after}"
    );
    // Leaving again keeps the same place.
    host.click_channel(general);
    host.travel(true);
    for _ in 0..4 {
        host.frame();
    }
    let again = host.tree.anchor_offset(&anchor).unwrap();
    assert!((again - before).abs() < 1.0, "{before} → {again}");
}

#[test]
fn the_header_s_arrows_go_back_and_forward_and_dim_without_a_place() {
    let mut host = Host::new();
    let start = host.ui.channel().unwrap();
    let back = rune_components::chrome::BACK;
    let forward = rune_components::chrome::FORWARD;
    let enabled = |host: &mut Host, id: &str| {
        let frame = host.frame();
        let node = frame.access.iter().find(|node| node.id == id);
        assert!(node.is_some(), "{id} is shown");
        node.is_some_and(|node| !node.disabled)
    };
    assert!(!enabled(&mut host, back), "nothing behind yet");
    assert!(!enabled(&mut host, forward));
    let general = host.channel_named("general");
    host.click_channel(general);
    assert!(enabled(&mut host, back));
    assert!(!enabled(&mut host, forward));
    host.click(back);
    host.frame();
    assert_eq!(host.ui.channel(), Some(start));
    assert!(!enabled(&mut host, back));
    assert!(enabled(&mut host, forward));
    host.click(forward);
    host.frame();
    assert_eq!(host.ui.channel(), Some(general));
    // Their hints name the keys chosen.
    host.ui.prefs.shortcuts = vec![(
        crate::keybinds::Action::Back,
        crate::keybinds::Chord::plain(Key::F9),
    )];
    let nav = crate::navigation::header(&host.ui, &host.store, false);
    assert_eq!(nav.back_keys.as_deref(), Some("F9"));
    assert_eq!(nav.forward_keys.as_deref(), Some("Alt+→"));
    assert_eq!(nav.search_keys.as_deref(), Some("Ctrl+F"));
}

#[test]
fn returning_at_the_end_stays_at_the_end_and_focuses_the_composer() {
    let mut host = Host::new();
    let slow = host.open_channel("rendimiento");
    assert!(host.tree.is_scroll_at_end(&timeline_id(slow), 24.0));
    let general = host.channel_named("general");
    host.click_channel(general);
    host.travel(true);
    assert_eq!(host.ui.channel(), Some(slow));
    assert!(host.tree.is_scroll_at_end(&timeline_id(slow), 24.0));
    assert_eq!(host.tree.focused_id(), Some(editor_id(slow).as_str()));
}

#[test]
fn returning_to_an_unread_conversation_does_not_mark_it_read_to_the_end() {
    let mut host = Host::new();
    let general = host.channel_named("general");
    host.frame();
    host.click_channel(general);
    assert!(
        host.store.read(general).unread(),
        "opened at its first unread"
    );
    let slow = host.channel_named("rendimiento");
    host.click_channel(slow);
    let sent = host.sent.len();
    host.travel(true);
    host.frame();
    assert_eq!(host.ui.channel(), Some(general));
    assert!(!host.tree.is_scroll_at_end(&timeline_id(general), 24.0));
    let newest = host.store.timeline(general).unwrap().newest().unwrap().id;
    assert!(
        !host.sent[sent..].iter().any(|command| matches!(
            command,
            Command::MarkRead { channel, message, .. } if *channel == general && *message == newest
        )),
        "the return does not read to the end"
    );
    assert!(host.store.read(general).unread());
}

#[test]
fn back_skips_conversations_that_are_gone() {
    let mut host = Host::new();
    let start = host.ui.channel().unwrap();
    let general = host.channel_named("general");
    let slow = host.channel_named("rendimiento");
    host.frame();
    host.click_channel(general);
    host.click_channel(slow);
    host.event(Event::ChannelRemoved { channel: general });
    host.travel(true);
    assert_eq!(
        host.ui.channel(),
        Some(start),
        "the removed one is passed over"
    );
    host.travel(false);
    assert_eq!(host.ui.channel(), Some(slow));
}

#[test]
fn the_history_keeps_a_bounded_number_of_places() {
    let mut host = Host::new();
    let general = host.channel_named("general");
    let slow = host.channel_named("rendimiento");
    for round in 0..crate::navigation::LIMIT + 10 {
        let server = host.store.channel(general).unwrap().server;
        let target = if round % 2 == 0 { general } else { slow };
        let intents = open(&mut host.ui, &mut host.store, server, target);
        host.run(intents);
    }
    assert_eq!(host.ui.history.back.len(), crate::navigation::LIMIT);
}

#[test]
fn another_account_starts_with_no_history() {
    let mut host = Host::new();
    let general = host.channel_named("general");
    host.frame();
    host.click_channel(general);
    assert!(!host.ui.history.back.is_empty());
    let mut snapshot = host.provider.snapshot();
    let mut me = snapshot.me.clone().unwrap();
    me.id = UserId(me.id.0 + 1_000_000);
    snapshot.me = Some(me);
    host.event(Event::Ready(Box::new(snapshot)));
    host.frame();
    assert!(host.ui.history.back.is_empty() && host.ui.history.forward.is_empty());
    let before = host.ui.channel();
    host.travel(true);
    assert_eq!(host.ui.channel(), before);
}

#[test]
fn f6_goes_round_the_regions_of_the_window() {
    let mut host = Host::new();
    host.ui.members_open = true;
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    assert_eq!(host.tree.focused_id(), Some(editor_id(general).as_str()));
    let frame = host.frame();
    let landmarks: Vec<_> = frame
        .access
        .iter()
        .filter(|node| node.role == limn::Role::Region)
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(
        landmarks,
        [
            "Servidores",
            "Canales",
            "Mensajes",
            "Escribir un mensaje",
            "Miembros"
        ],
        "the regions are landmarks a screen reader can name"
    );
    let members = host.f6(false).unwrap();
    assert!(members.starts_with("member:"), "{members}");
    assert_eq!(
        host.f6(false).as_deref(),
        Some(format!("server:{server}").as_str())
    );
    assert_eq!(
        host.f6(false).as_deref(),
        Some(format!("channel:{general}").as_str())
    );
    let message = host.f6(false).unwrap();
    let rows = host.tree.frame().regions.clone();
    let timeline = rows
        .iter()
        .find(|region| region.id == timeline_id(general))
        .unwrap()
        .rect;
    let control = rows
        .iter()
        .find(|region| region.id == message)
        .unwrap()
        .rect;
    assert!(
        timeline.contains(Point::new(control.x + 1.0, control.y + 1.0)),
        "{message} is a control of the messages"
    );
    assert_eq!(host.f6(false).as_deref(), Some(editor_id(general).as_str()));
    // Shift+F6 goes the other way.
    assert_eq!(host.f6(true).as_deref(), Some(message.as_str()));
    assert_eq!(
        host.f6(true).as_deref(),
        Some(format!("channel:{general}").as_str())
    );
}

#[test]
fn f6_skips_a_hidden_member_list_and_stays_inside_a_dialog() {
    let mut host = Host::new();
    host.ui.members_open = false;
    let general = host.open_channel("general");
    let next = host.f6(false).unwrap();
    assert!(next.starts_with("server:"), "{next}");
    host.click("me:settings");
    host.frame();
    let inside = host.tree.focused_id().map(str::to_owned);
    assert_eq!(host.f6(false), inside, "a dialog keeps the focus");
    host.key(Key::Escape, false, false);
    host.frame();
    assert!(host.ui.settings.is_none());
    let _ = general;
}

#[test]
fn the_shortcuts_list_opens_traps_the_focus_and_closes_with_escape() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let intents = toggle_shortcuts(&mut host.ui);
    host.run(intents);
    let painted = host.painted();
    assert!(painted.iter().any(|text| text == "Atajos de teclado"));
    // The table's, as bound now; the body scrolls to the rest.
    for action in crate::keybinds::ACTIONS
        .into_iter()
        .filter(|action| action.group() == crate::keybinds::Group::Navigation)
    {
        let what = action.label();
        assert!(painted.iter().any(|text| text == what), "{what} is listed");
    }
    assert!(host.tree.has_open_popover());
    for _ in 0..3 {
        host.key(Key::Tab, false, false);
        let focused = host.tree.focused_id().unwrap_or_default().to_owned();
        assert!(
            focused.starts_with("shortcuts:"),
            "focus stays in it: {focused}"
        );
    }
    // Keys that open other things wait while it is open.
    host.key(Key::K, false, true);
    assert!(host.ui.switcher.is_none());
    assert!(travel(&mut host.ui, &mut host.store, true).is_empty());
    host.key(Key::Escape, false, false);
    host.frame();
    assert!(!host.ui.shortcuts && !host.tree.has_open_popover());
    assert_eq!(host.tree.focused_id(), Some(editor_id(general).as_str()));
}

#[test]
fn every_dialog_closes_with_escape_and_keeps_tab_inside() {
    let mut host = Host::new();
    // A server where the account may make channels.
    let server = host
        .store
        .servers()
        .iter()
        .find(|server| server.name == "Laboratorio limn")
        .unwrap()
        .id;
    host.click(&format!("server:{server}"));
    host.frame();
    for opener in [
        format!("sm:notify:{server}"),
        format!("sm:profile:{server}"),
        // The owner cannot leave its own server: ask of another one.
        format!("sm:leave:{}", host.store.servers()[0].id),
        format!("newchan:{server}"),
        format!("newcat:{server}"),
        "newsrv:open".into(),
        "me:settings".into(),
        crate::navigation::SHORTCUTS_OPEN.into(),
    ] {
        let intents = update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked { id: opener.clone() },
            host.now,
        );
        host.run(intents);
        host.frame();
        assert!(host.tree.has_open_popover(), "{opener} opens a dialog");
        host.assert_named(&opener);
        for _ in 0..4 {
            host.key(Key::Tab, false, false);
            let focused = host.tree.focused_id().unwrap_or_default().to_owned();
            assert!(
                !focused.starts_with("composer:")
                    && !focused.starts_with("channel:")
                    && !focused.starts_with("server:"),
                "{opener}: Tab stays in the dialog, not on {focused}"
            );
        }
        host.key(Key::Escape, false, false);
        host.frame();
        assert!(
            !crate::reads::modal_obscured(&host.ui) && !host.tree.has_open_popover(),
            "{opener}: Escape closes it"
        );
    }
}

#[test]
fn the_switcher_closes_with_escape() {
    let mut host = Host::new();
    host.open_channel("general");
    host.key(Key::K, false, true);
    assert!(host.ui.switcher.is_some());
    host.key(Key::Escape, false, false);
    assert!(host.ui.switcher.is_none());
}

#[test]
fn every_control_of_the_window_has_an_accessible_name() {
    let mut host = Host::new();
    host.ui.members_open = true;
    let general = host.open_channel("general");
    host.assert_named("the conversation");
    // A message's tools show while the pointer is over it.
    let timeline = host
        .tree
        .frame()
        .regions
        .iter()
        .find(|region| region.id == timeline_id(general))
        .unwrap()
        .rect;
    host.tree.handle_event(UiEvent::PointerMove(Point::new(
        timeline.x + timeline.width / 2.0,
        timeline.y + timeline.height - 30.0,
    )));
    host.frame();
    assert!(
        host.tree
            .frame()
            .regions
            .iter()
            .any(|region| region.id.starts_with("msg:")),
        "the message's tools show"
    );
    host.assert_named("a message under the pointer");
    host.click("home");
    host.assert_named("the direct conversations");
}
