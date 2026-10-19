//! Picker insertion through the same controlled tree/focus path as desktop.

use super::*;

fn draft(host: &mut Host, channel: ChannelId, value: &str, cursor: usize) {
    host.ui.set_draft(channel, Some(value.into()));
    host.run(vec![
        Intent::SetInput {
            id: editor_id(channel),
            value: value.into(),
        },
        Intent::FocusInput {
            id: editor_id(channel),
            cursor,
        },
    ]);
    host.frame();
}

fn action(host: &mut Host, id: &str) {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::Clicked { id: id.into() },
        host.now,
    );
    host.run(intents);
    host.frame();
}

#[test]
fn unicode_picker_replaces_character_selection_and_restores_cursor_after_focus() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    draft(&mut host, channel, "a🙂bc終", 1);
    for _ in 0..3 {
        host.key(Key::ArrowRight, true, false);
    }
    host.frame();
    assert_eq!(host.tree.input_selection(&editor_id(channel)), Some((1, 4)));
    host.click("composer:emoji");
    action(&mut host, "composer:emoji=🎉");
    assert_eq!(host.ui.drafts[&channel], "a🎉終");
    assert_eq!(host.tree.input_cursor(&editor_id(channel)), Some(2));
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
    host.type_text("X");
    assert_eq!(host.ui.drafts[&channel], "a🎉X終");
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::SendMessage { .. })));
}

#[test]
fn custom_picker_preserves_surrounding_unicode_and_inserts_opaque_text_at_cursor() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut server = host.store.server(host.ui.server.unwrap()).unwrap().clone();
    server.emojis.push(rune_model::CustomEmoji {
        id: rune_model::EmojiId(987),
        server: server.id,
        name: "hello".into(),
        insert: "opaque-expression".into(),
        animated: false,
        available: true,
        roles: vec![],
        image: None,
    });
    host.event(Event::ServerChanged(server));
    draft(&mut host, channel, "左🙂右", 2);
    host.click("composer:emoji");
    action(&mut host, "composer:emoji:custom=987");
    assert_eq!(host.ui.drafts[&channel], "左🙂opaque-expression右");
    assert_eq!(host.tree.input_cursor(&editor_id(channel)), Some(19));
}

#[test]
fn gif_insertion_separates_both_sides_and_replaces_only_the_selected_text() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    draft(&mut host, channel, "leftXright", 4);
    host.key(Key::ArrowRight, true, false);
    host.frame();
    host.click("composer:emoji");
    host.ui.messages.gif_url = "https://example.com/clip.gif".into();
    action(&mut host, "composer:emoji:gif-insert");
    assert_eq!(
        host.ui.drafts[&channel],
        "left https://example.com/clip.gif right"
    );
    let cursor = "left https://example.com/clip.gif ".chars().count();
    assert_eq!(host.tree.input_cursor(&editor_id(channel)), Some(cursor));
    host.type_text("new ");
    assert_eq!(
        host.ui.drafts[&channel],
        "left https://example.com/clip.gif new right"
    );
}

#[test]
fn replacing_selection_respects_utf16_limit_without_erasing_a_rejected_draft() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let full = "🙂".repeat(1000);
    draft(&mut host, channel, &full, 500);
    action(&mut host, "composer:emoji=🎉");
    assert_eq!(host.ui.drafts[&channel], full);
    host.key(Key::A, false, true);
    host.frame();
    assert_eq!(
        host.tree.input_selection(&editor_id(channel)),
        Some((0, 1000))
    );
    action(&mut host, "composer:emoji=🎉");
    assert_eq!(host.ui.drafts[&channel], "🎉");
    assert_eq!(host.tree.input_cursor(&editor_id(channel)), Some(1));
}

#[test]
fn stale_draft_and_changed_channel_fall_back_without_modifying_the_old_conversation() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    draft(&mut host, channel, "old draft", 0);
    host.ui.set_draft(channel, Some("changed draft".into()));
    action(&mut host, "composer:emoji=🎉");
    assert_eq!(host.ui.drafts[&channel], "changed draft🎉");
    draft(&mut host, channel, "old channel", 0);
    let other = host.channel_named("limn-core");
    let server = host.store.channel(other).unwrap().server;
    host.ui.server = server;
    host.ui.channels.insert(server, other);
    host.ui.set_draft(other, Some("other draft".into()));
    action(&mut host, "composer:emoji=🎉");
    assert_eq!(host.ui.drafts[&other], "other draft🎉");
    assert_eq!(host.ui.drafts[&channel], "old channel");
}

#[test]
fn two_insertions_before_layout_keep_the_new_cursor_in_the_middle() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    draft(&mut host, channel, "ab", 1);
    for glyph in ["🎉", "🚀"] {
        // Desktop may deliver multiple input events between layouts.
        capture_composer_selection(&mut host.ui, &host.tree);
        let intents = update(
            &mut host.ui,
            &mut host.store,
            &limn::UiAction::Clicked {
                id: format!("composer:emoji={glyph}"),
            },
            host.now,
        );
        host.run(intents);
    }
    host.frame();
    assert_eq!(host.ui.drafts[&channel], "a🎉🚀b");
    assert_eq!(host.tree.input_cursor(&editor_id(channel)), Some(3));
}

#[test]
fn typing_in_the_same_event_batch_after_picker_enter_reaches_the_composer() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    draft(&mut host, channel, "ab", 1);
    host.click("composer:emoji");
    host.type_text("crab");
    for event in [
        UiEvent::Key {
            key: Key::Enter,
            shift: false,
            ctrl: false,
            alt: false,
        },
        UiEvent::Text("X".into()),
    ] {
        capture_composer_selection(&mut host.ui, &host.tree);
        let focused = host.tree.focused_id().map(str::to_owned);
        if let Some(intents) = intercept(&mut host.ui, &mut host.store, &event, focused.as_deref())
        {
            host.run(intents);
        } else {
            for action in host.tree.handle_event(event) {
                let intents = update(&mut host.ui, &mut host.store, &action, host.now);
                host.run(intents);
            }
        }
    }
    host.frame();
    assert_eq!(host.ui.drafts[&channel], "a🦀Xb");
    assert_eq!(host.tree.input_cursor(&editor_id(channel)), Some(3));
}
