//! Local reminders, actual window activity, and the channel-scoped inbox.

use super::*;
use rune_model::{HistorySource, Permissions};
use rune_provider::{ConnectionState, Failure};

fn clear_everywhere(host: &mut Host) {
    let channels: Vec<_> = host
        .store
        .read_channels()
        .map(|channel| channel.id)
        .collect();
    for channel in channels {
        host.store.mark_read(channel);
    }
}

fn unread_action(host: &mut Host, channel: ChannelId, message: MessageId) {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{message}:unread"),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(host.store.read(channel).unread_from, Some(message));
}

#[test]
fn an_inactive_window_never_marks_visible_messages_read() {
    let mut host = Host::new();
    host.active = false;
    let channel = host.open_channel("general");
    assert!(host.store.read(channel).unread());
    let sent = host.sent.len();
    host.frame();
    host.frame();
    assert!(host.store.read(channel).unread());
    assert!(!host.sent[sent..]
        .iter()
        .any(|command| matches!(command, Command::MarkRead { .. })));
    host.active = true;
    host.tree.scroll_to_end(&timeline_id(channel));
    host.frame();
    assert!(!host.store.read(channel).unread());
}

#[test]
fn a_manual_unread_bar_survives_leaving_and_reopening_without_a_remote_rewind() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let target = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .rev()
        .find(|message| Some(message.author) != host.store.me() && !message.delivery.is_local())
        .unwrap()
        .id;
    let through = host.store.read(channel).last_read;
    let sent = host.sent.len();
    unread_action(&mut host, channel, target);
    assert_eq!(host.store.read(channel).last_read, through);
    assert!(!host.sent[sent..]
        .iter()
        .any(|command| matches!(command, Command::MarkRead { .. })));
    host.open_channel("ayuda");
    host.open_channel("general");
    assert_eq!(host.store.read(channel).unread_from, Some(target));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Marcar como leído")));
    host.click(crate::messages::UNREAD_CLEAR);
    assert_eq!(host.store.read(channel).unread_from, None);
}

#[test]
fn inbox_filters_channels_with_mentions_and_removes_revoked_rows_immediately() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    clear_everywhere(&mut host);
    let mut mention = host.store.timeline(channel).unwrap().messages()[0]
        .as_ref()
        .clone();
    mention.id = MessageId(u64::MAX / 4);
    mention.mentions = vec![host.store.me().unwrap()];
    mention.author = host
        .store
        .users()
        .find(|user| Some(user.id) != host.store.me())
        .unwrap()
        .id;
    host.active = false;
    host.event(Event::MessageCreated {
        message: mention,
        nonce: None,
    });
    host.click("inbox:open");
    host.click("inbox:mentions");
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Canales con menciones")));
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == format!("inbox:go:{channel}")));
    let mut revoked = host.store.channel(channel).unwrap().clone();
    revoked.permissions = Permissions::VIEW;
    host.event(Event::ChannelChanged(revoked));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == format!("inbox:go:{channel}")));
    let before = host.context_reads.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("inbox:go:{channel}"),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(host.context_reads.len(), before);
}

#[test]
fn inbox_opens_an_unloaded_exact_reminder_through_context_and_preserves_the_draft() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    clear_everywhere(&mut host);
    let (history, _) = host.provider.history(channel, None, 1000);
    let target = history[2].id;
    assert!(host.store.timeline(channel).unwrap().get(target).is_none());
    host.type_text("borrador que sigue aquí");
    host.store.mark_unread(channel, target);
    host.click("inbox:open");
    host.click(&format!("inbox:go:{channel}"));
    let (request, found, anchor, source) = host
        .context_reads
        .last()
        .copied()
        .expect("one context read");
    assert_eq!(
        (found, anchor, source),
        (channel, target, HistorySource::Provider)
    );
    host.active = true;
    host.frame();
    assert_eq!(host.store.read(channel).unread_from, Some(target));
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("borrador que sigue aquí")
    );
    host.event(Event::ContextLoaded {
        request,
        channel,
        anchor: target,
        messages: history.into_iter().take(30).collect(),
    });
    host.frame();
    assert_eq!(
        host.store.timeline(channel).unwrap().context_source(),
        Some(HistorySource::Provider)
    );
    assert_eq!(host.store.read(channel).unread_from, Some(target));
}

#[test]
fn local_progress_shows_an_explicit_confirmation_action_without_layout_retries() {
    let mut host = Host::new();
    host.confirm_reads = false;
    host.active = false;
    let channel = host.open_channel("general");
    host.store.connection = ConnectionState::Disconnected {
        reason: "sin conexión".into(),
    };
    assert_eq!(host.store.mark_read_command(channel), None);
    host.store.connection = ConnectionState::Connected;
    let sent = host.sent.len();
    for _ in 0..3 {
        host.frame();
    }
    assert_eq!(host.sent.len(), sent);
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("sin confirmar")));
    host.click(&format!("reads:retry:{channel}"));
    let Some(Command::MarkRead { request, .. }) = host.sent.last().cloned() else {
        panic!("explicit retry")
    };
    host.event(Event::Failed {
        failure: Failure::ReadState { request, channel },
        reason: "No se confirmó".into(),
        ambiguous: true,
    });
    assert!(!host.store.read(channel).sync_pending());
    let sent = host.sent.len();
    host.frame();
    host.frame();
    assert_eq!(host.sent.len(), sent);
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Reintentar")));
}

#[test]
fn a_stale_or_deleted_unread_action_cannot_mark_another_channel_or_invent_an_anchor() {
    let mut host = Host::new();
    let previous = host.open_channel("general");
    let old = host.store.timeline(previous).unwrap().oldest().unwrap();
    let current = host.open_channel("ayuda");
    let revision = host.store.revision();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{old}:unread"),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert_eq!(host.store.revision(), revision);
    assert_eq!(host.store.read(current).unread_from, None);
    let deleted = host.store.timeline(current).unwrap().oldest().unwrap();
    host.event(Event::MessageDeleted {
        channel: current,
        message: deleted,
    });
    let revision = host.store.revision();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{deleted}:unread"),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert_eq!(host.store.revision(), revision);
    assert_eq!(host.store.read(current).unread_from, None);
}

#[test]
fn retrying_a_read_does_not_consume_a_new_mention_or_a_manual_reminder() {
    let mut host = Host::new();
    host.active = false;
    host.confirm_reads = false;
    let channel = host.open_channel("general");
    let original = host.store.read(channel).last_message.unwrap();
    let command = host.store.mark_read_command(channel).unwrap();
    let Command::MarkRead { request, .. } = command else {
        unreachable!()
    };
    host.event(Event::Failed {
        failure: Failure::ReadState { request, channel },
        reason: "Sin ACK".into(),
        ambiguous: true,
    });
    let mut mention = host.store.timeline(channel).unwrap().messages()[0]
        .as_ref()
        .clone();
    let reminder = mention.id;
    mention.id = MessageId(u64::MAX / 4);
    mention.mentions = vec![host.store.me().unwrap()];
    mention.author = host
        .store
        .users()
        .find(|user| Some(user.id) != host.store.me())
        .unwrap()
        .id;
    host.event(Event::MessageCreated {
        message: mention,
        nonce: None,
    });
    host.store.mark_unread(channel, reminder);
    host.click(&format!("reads:retry:{channel}"));
    assert!(
        matches!(host.sent.last(), Some(Command::MarkRead { message, .. }) if *message == original)
    );
    assert_eq!(host.store.read(channel).last_read, Some(original));
    assert_eq!(host.store.read(channel).unread_from, Some(reminder));
    assert_eq!(host.store.read(channel).mentions, 1);
}

#[test]
fn an_automatic_unread_anchor_deleted_before_latest_is_recomputed_from_real_rows() {
    let mut host = Host::new();
    host.active = false;
    let channel = host.channel_named("general");
    let first = host.store.read(channel).first_unread.unwrap();
    let server = host.store.channel(channel).unwrap().server;
    let _pending = open(&mut host.ui, &mut host.store, server, channel);
    host.event(Event::MessageDeleted {
        channel,
        message: first,
    });
    let (messages, reached_start) = host.provider.history(channel, None, 50);
    host.event(Event::HistoryLoaded {
        request: 77,
        channel,
        before: None,
        messages,
        reached_start,
    });
    assert!(!host
        .context_reads
        .iter()
        .any(|(_, _, anchor, _)| *anchor == first));
    let read = host.store.read(channel);
    let expected = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| Some(message.id) > read.last_read)
        .unwrap()
        .id;
    assert_eq!(host.ui.new_marker.get(&channel), Some(&expected));
}

#[test]
fn a_layout_with_a_pending_scroll_cannot_mark_the_old_end_read() {
    let mut host = Host::new();
    host.active = false;
    let channel = host.open_channel("general");
    host.tree.scroll_to_end(&timeline_id(channel));
    host.layout_only();
    let anchor = host.store.timeline(channel).unwrap().newest().unwrap().id;
    let scroll = open_message(
        &mut host.ui,
        &host.store,
        channel,
        anchor,
        HistorySource::Provider,
    );
    assert!(scroll
        .iter()
        .any(|intent| matches!(intent, Intent::ScrollToIndex { .. })));
    let effects = after_layout(&mut host.ui, &mut host.store, &host.tree, true);
    assert!(!effects
        .iter()
        .any(|intent| matches!(intent, Intent::Command(Command::MarkRead { .. }))));
    assert!(host.store.read(channel).unread());
    host.run(scroll);
    host.active = true;
    host.frame();
    assert!(!host.store.read(channel).unread());
}

#[test]
fn history_arriving_behind_the_inbox_cannot_close_it_or_steal_its_focus() {
    let mut host = Host::new();
    host.active = false;
    let channel = host.channel_named("general");
    let server = host.store.channel(channel).unwrap().server;
    let _pending = open(&mut host.ui, &mut host.store, server, channel);
    host.click("inbox:open");
    let (messages, reached_start) = host.provider.history(channel, None, 50);
    host.event(Event::HistoryLoaded {
        request: 77,
        channel,
        before: None,
        messages,
        reached_start,
    });
    assert!(host.ui.inbox.is_some());
    assert!(host.store.read(channel).unread());
    assert!(host.ui.unread_anchor.is_some());
}

#[test]
fn a_live_ready_for_the_cached_account_preserves_the_inbox_and_focuses_its_selected_tab() {
    let mut host = Host::new();
    host.active = false;
    let channel = host.open_channel("general");
    host.type_text("borrador durante el arranque guardado");
    host.store.from_cache = true;
    host.click("inbox:open");
    host.click("inbox:mentions");
    host.tree.focus("inbox:mentions");
    host.event(Event::Ready(Box::new(host.provider.snapshot())));
    host.frame();
    assert!(
        host.ui.inbox.is_some(),
        "same-account Ready preserves the modal"
    );
    assert_eq!(host.tree.focused_id(), Some("inbox:mentions"));
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("borrador durante el arranque guardado")
    );
    assert!(
        host.ui.unread_anchor.is_some(),
        "background history waits for the modal to close"
    );
    assert!(
        host.ui.context_navigation.is_none(),
        "no previous context survives Ready"
    );
}

#[test]
fn a_ready_for_another_account_closes_the_old_accounts_inbox() {
    let mut host = Host::new();
    host.active = false;
    host.open_channel("general");
    host.click("inbox:open");
    let mut snapshot = host.provider.snapshot();
    let mut other = snapshot.me.take().unwrap();
    other.id = rune_model::UserId(900_007_777);
    snapshot.users.push(other.clone());
    snapshot.me = Some(other);
    host.event(Event::Ready(Box::new(snapshot)));
    host.frame();
    assert!(host.ui.inbox.is_none());
    assert!(!host
        .tree
        .focused_id()
        .is_some_and(|id| id.starts_with("inbox:")));
}

#[test]
fn an_empty_inbox_shows_its_message_without_an_empty_scrollable_list() {
    let mut host = Host::new();
    host.active = false;
    host.open_channel("general");
    clear_everywhere(&mut host);
    host.click("inbox:open");
    let frame = host.frame();
    assert!(frame.commands.iter().any(|command| {
        matches!(command, limn::DrawCommand::Text { text, .. } if text == "No hay canales en este alcance.")
    }));
    assert!(
        !frame.regions.iter().any(|region| region.id == "inbox:rows"),
        "an empty inbox has no scrollable list or phantom row"
    );
    assert!(!frame
        .regions
        .iter()
        .any(|region| region.id.starts_with("inbox:go:") || region.id.starts_with("inbox:read:")));
}
