//! Local message search through the real limn tree, including live changes.

use super::*;

const QUERY: &str = "message-search:query";
const RESULTS: &str = "message-search:results";

fn local_available(host: &mut Host, available: bool) {
    let intents = local_history_changed(&mut host.ui, &host.store, available);
    host.run(intents);
}

fn local_page(
    host: &mut Host,
    request: u64,
    channel: ChannelId,
    ids: &[u64],
    continuation: Option<u64>,
) {
    let result = rune_model::SearchPage {
        hits: ids
            .iter()
            .map(|&id| rune_model::SearchHit {
                channel,
                id: MessageId(id),
                author: host.store.me().unwrap(),
                timestamp: Timestamp(1_790_000_000_000),
                snippet: format!("needle guardada {id}"),
                files: false,
                pinned: false,
            })
            .collect(),
        continuation: continuation.map(rune_model::SearchCursor),
    };
    let intents = local_search_result(
        &mut host.ui,
        &host.store,
        request,
        channel,
        Ok(result),
        host.now,
    );
    host.run(intents);
}

#[test]
fn device_search_requires_local_history_and_displays_request_scoped_lightweight_hits() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.key(Key::F, false, true);
    local_available(&mut host, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.type_text("needle");
    let request = host
        .local_searches
        .last()
        .expect("device scope dispatches local search")
        .0;
    assert_eq!(host.local_searches.last().unwrap().1.channel, channel);
    local_page(&mut host, request - 1, channel, &[900_001], None);
    assert!(result_ids(&mut host).is_empty());
    local_page(&mut host, request, channel, &[900_002], None);
    assert_eq!(
        result_ids(&mut host),
        vec![format!("message-search:go:{channel}:900002")]
    );
    assert!(host
        .store
        .timeline(channel)
        .unwrap()
        .get(MessageId(900_002))
        .is_none());
}

#[test]
fn cached_pagination_preserves_finished_cursor_and_cancels_only_pending_or_dismissed_reads() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.type_text("needle");
    let first = host.local_searches.last().unwrap().0;
    local_page(&mut host, first, channel, &[900_020], Some(77));
    host.sent.clear();
    host.click("message-search:next");
    let (second, _, cursor) = host.local_searches.last().unwrap();
    let second = *second;
    assert_eq!(*cursor, Some(rune_model::SearchCursor(77)));
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::Cancel(request) if *request == first)));
    local_page(&mut host, second, channel, &[900_010], None);
    let reads = host.local_searches.len();
    host.click("message-search:previous");
    assert_eq!(
        host.local_searches.len(),
        reads,
        "previous page is already held as lightweight hits"
    );
    assert_eq!(
        result_ids(&mut host),
        vec![format!("message-search:go:{channel}:900020")]
    );
    host.click("message-search:refresh");
    let pending = host.local_searches.last().unwrap().0;
    host.key(Key::Escape, false, false);
    assert!(host
        .sent
        .iter()
        .any(|command| matches!(command, Command::Cancel(request) if *request == pending)));
    local_page(&mut host, pending, channel, &[900_000], None);
    assert!(host.ui.search.is_none());
}

#[test]
fn cached_hit_loads_explicit_context_and_returns_to_recent_without_changing_the_draft() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.type_text("borrador intacto");
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.type_text("needle");
    let query = host.local_searches.last().unwrap().0;
    local_page(&mut host, query, channel, &[900_001], None);
    host.click(&format!("message-search:go:{channel}:900001"));
    let (request, found_channel, anchor, source) = *host
        .context_reads
        .last()
        .expect("unloaded hit loads a context by ID");
    assert_eq!(
        (found_channel, anchor, source),
        (
            channel,
            MessageId(900_001),
            rune_model::HistorySource::Cached
        )
    );
    let mut found = message(&host, channel, 0, "needle contexto");
    found.id = anchor;
    let applied = host.store.apply_cached_context(
        request,
        channel,
        rune_model::CachedContext {
            anchor,
            messages: vec![found],
            has_older_cached: true,
            has_newer_cached: true,
        },
    );
    let intents = on_applied(&mut host.ui, &mut host.store, &applied, host.now);
    host.run(intents);
    host.frame();
    assert!(host.ui.search.is_none());
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("borrador intacto")
    );
    host.tree.scroll_to_end(&timeline_id(channel));
    host.sent.clear();
    host.frame();
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "jump:latest"));
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::MarkRead { .. })));
    host.click("jump:latest");
    assert!(host.store.timeline(channel).unwrap().latest);
    assert_eq!(host.store.timeline(channel).unwrap().context_source(), None);
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("borrador intacto")
    );
}

#[test]
fn search_cannot_reopen_retained_history_after_read_permission_is_revoked() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.key(Key::F, false, true);
    let mut revoked = host.store.channel(channel).unwrap().clone();
    revoked.permissions = rune_model::Permissions::VIEW | rune_model::Permissions::SEND;
    host.event(Event::ChannelChanged(revoked));
    assert!(host.ui.search.is_none());
    host.key(Key::F, false, true);
    assert!(
        host.ui.search.is_none(),
        "retained messages do not grant history access"
    );
}

#[test]
fn failed_context_outside_search_has_an_accessible_close_action() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let intents = open_message(
        &mut host.ui,
        &host.store,
        channel,
        MessageId(900_001),
        rune_model::HistorySource::Provider,
    );
    host.run(intents);
    let request = host.context_reads.last().unwrap().0;
    host.event(Event::Failed {
        failure: rune_provider::Failure::Context { request, channel },
        reason: "No se pudo cargar".into(),
        ambiguous: false,
    });
    host.click("context:cancel");
    assert!(host.ui.context_navigation.is_none());
}

#[test]
fn context_scope_survives_dismissing_highlight_and_explains_a_latest_preview() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let intents = open_message(
        &mut host.ui,
        &host.store,
        channel,
        MessageId(900_001),
        rune_model::HistorySource::Provider,
    );
    host.run(intents);
    let request = host.context_reads.last().unwrap().0;
    let mut found = message(&host, channel, 0, "mensaje en ventana de contexto");
    found.id = MessageId(900_001);
    host.event(Event::ContextLoaded {
        request,
        channel,
        anchor: found.id,
        messages: vec![found.clone()],
    });
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Ventana de contexto del proveedor")));
    host.key(Key::Escape, false, false);
    assert!(host.ui.context_navigation.is_none());
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Ventana de contexto del proveedor")));
    let applied = host.store.apply_cached(channel, vec![found]);
    let intents = on_applied(&mut host.ui, &mut host.store, &applied, host.now);
    host.run(intents);
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Vista provisional de recientes")));
    assert!(!host.store.timeline(channel).unwrap().latest);
    host.tree.set_root(view(
        &host.ui,
        &host.store,
        &ViewContext {
            loading_latest: true,
            ..host.context()
        },
    ));
    assert!(host.tree.layout(WINDOW).commands.iter().any(|command| {
        matches!(command, limn::DrawCommand::Text { text, .. } if text.contains("Actualizando recientes…"))
    }));
}

#[test]
fn escape_cancels_loading_context_and_its_late_answer_cannot_navigate() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let intents = open_message(
        &mut host.ui,
        &host.store,
        channel,
        MessageId(900_001),
        rune_model::HistorySource::Provider,
    );
    host.run(intents);
    let request = host.context_reads.last().unwrap().0;
    host.key(Key::Escape, false, false);
    assert!(host.ui.context_navigation.is_none());
    assert!(host
        .sent
        .iter()
        .any(|command| matches!(command, Command::Cancel(found) if *found == request)));
    let mut found = message(&host, channel, 0, "respuesta vieja");
    found.id = MessageId(900_001);
    host.event(Event::ContextLoaded {
        request,
        channel,
        anchor: found.id,
        messages: vec![found],
    });
    assert!(host
        .store
        .timeline(channel)
        .unwrap()
        .get(MessageId(900_001))
        .is_none());
}

#[test]
fn opening_settings_cancels_the_search_and_context_it_replaces() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.type_text("needle");
    let query = host.local_searches.last().unwrap().0;
    let intents = open_message(
        &mut host.ui,
        &host.store,
        channel,
        MessageId(900_001),
        rune_model::HistorySource::Cached,
    );
    host.run(intents);
    let context = host.context_reads.last().unwrap().0;
    let intents = settings::open_settings(&mut host.ui, &host.store, SettingsPage::default());
    host.run(intents);
    assert!(host.ui.search.is_none());
    assert!(host.ui.context_navigation.is_none());
    for request in [query, context] {
        assert!(host
            .sent
            .iter()
            .any(|command| matches!(command, Command::Cancel(found) if *found == request)));
    }
}

#[test]
fn deleting_the_last_context_message_keeps_a_way_back_to_recent() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let intents = open_message(
        &mut host.ui,
        &host.store,
        channel,
        MessageId(900_001),
        rune_model::HistorySource::Provider,
    );
    host.run(intents);
    let request = host.context_reads.last().unwrap().0;
    let mut found = message(&host, channel, 0, "último mensaje del contexto");
    found.id = MessageId(900_001);
    host.event(Event::ContextLoaded {
        request,
        channel,
        anchor: found.id,
        messages: vec![found],
    });
    host.event(Event::MessageDeleted {
        channel,
        message: MessageId(900_001),
    });
    assert!(host.store.timeline(channel).unwrap().is_empty());
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "jump:latest"));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Este contexto ya no tiene mensajes disponibles")));
    host.click("jump:latest");
    assert!(host.store.timeline(channel).unwrap().latest);
}

#[test]
fn unknown_partial_update_invalidates_device_hits_without_starting_a_query_loop() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.type_text("needle");
    let request = host.local_searches.last().unwrap().0;
    local_page(&mut host, request, channel, &[900_001], None);
    let count = host.local_searches.len();
    host.event(Event::MessagePatched(rune_model::MessagePatch {
        channel,
        id: MessageId(900_001),
        content: Some("otro texto".into()),
        ..Default::default()
    }));
    assert!(result_ids(&mut host).is_empty());
    assert_eq!(host.local_searches.len(), count);
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Actualizá la búsqueda")));
    host.click("message-search:refresh");
    assert!(host.local_searches.len() > count);
}

#[test]
fn author_picker_can_find_a_known_user_outside_the_loaded_authors() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut user = host.store.me_user().unwrap().clone();
    user.id = rune_model::UserId(900_777);
    user.name = "Autora ausente del tramo cargado".into();
    host.event(Event::UserChanged(user));
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.click("message-search:author");
    host.frame();
    host.tree.focus("message-search:author-query");
    host.type_text("Autora ausente");
    host.click("message-search:author:900777");
    assert_eq!(
        host.local_searches.last().unwrap().1.author,
        Some(rune_model::UserId(900_777))
    );
    assert_eq!(host.local_searches.last().unwrap().1.channel, channel);
}

#[test]
fn author_picker_prioritizes_an_exact_name_beyond_the_first_64_partial_matches() {
    let mut host = Host::new();
    host.open_channel("general");
    let template = host.store.me_user().unwrap().clone();
    for index in 0..80 {
        let mut user = template.clone();
        user.id = rune_model::UserId(910_000 + index);
        user.name = format!("ana{index}");
        user.display_name = None;
        host.event(Event::UserChanged(user));
    }
    // Choose a user outside the old pre-sort cap without relying on HashMap order.
    let mut exact = host
        .store
        .users()
        .filter(|user| (910_000..910_080).contains(&user.id.0))
        .nth(64)
        .unwrap()
        .clone();
    exact.name = "ana".into();
    let target = exact.id;
    host.event(Event::UserChanged(exact));
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.click("message-search:author");
    host.frame();
    host.tree.focus("message-search:author-query");
    host.type_text("ana");
    host.click(&format!("message-search:author:{target}"));
    assert_eq!(host.local_searches.last().unwrap().1.author, Some(target));
}

#[test]
fn local_calendar_filter_can_be_cleared_without_replacing_the_query_or_draft() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.type_text("borrador conservado");
    local_available(&mut host, true);
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:device");
    host.type_text("needle");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: "message-search:from".into(),
            value: "2024-02-29".into(),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(
        host.local_searches.last().unwrap().1.from,
        rune_components::time::local_date_boundary("2024-02-29", false)
    );
    host.click("message-search:from-open");
    host.click("message-search:from:clear");
    assert_eq!(host.local_searches.last().unwrap().1.from, None);
    assert_eq!(host.local_searches.last().unwrap().1.text, "needle");
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("borrador conservado")
    );
}

fn change_query(host: &mut Host, query: &str) {
    host.tree.focus(QUERY);
    host.key(Key::A, false, true);
    host.type_text(query);
}

fn result_ids(host: &mut Host) -> Vec<String> {
    host.frame()
        .regions
        .iter()
        .filter(|region| region.id.starts_with("message-search:go:"))
        .map(|region| region.id.clone())
        .collect()
}

fn message(host: &Host, channel: ChannelId, index: usize, content: &str) -> rune_model::Message {
    let mut message = host.store.timeline(channel).unwrap().messages()[index]
        .as_ref()
        .clone();
    message.content = content.into();
    message.edited = Some(Timestamp(1_800_000_000_000));
    message
}

#[test]
fn header_searches_loaded_messages_and_keeps_conversation_switching_on_ctrl_k() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.type_text("Mi borrador pendiente");
    host.click("message-search:open");
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(QUERY));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("mensajes cargados")));
    assert!(
        result_ids(&mut host).is_empty(),
        "an empty query has no results"
    );
    host.type_text("una búsqueda independiente");
    host.key(Key::Escape, false, false);
    host.frame();
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("Mi borrador pendiente")
    );
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
    host.key(Key::K, false, true);
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(settings::SWITCHER_QUERY));
}

#[test]
fn changing_conversations_closes_the_search_before_showing_the_next_timeline() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let found = message(&host, channel, 10, "needle previous channel");
    host.event(Event::MessageUpdated(found));
    host.key(Key::F, false, true);
    host.type_text("needle previous channel");
    assert_eq!(result_ids(&mut host).len(), 1);
    let intents = navigate(&mut host.ui, &mut host.store, true);
    host.run(intents);
    host.frame();
    assert_ne!(host.ui.channel(), Some(channel));
    assert!(host.ui.search.is_none());
    assert_eq!(
        host.tree.focused_id(),
        Some(editor_id(host.ui.channel().unwrap()).as_str())
    );
    assert!(result_ids(&mut host).is_empty());
}

#[test]
fn search_is_case_insensitive_and_limited_to_the_open_conversation() {
    let mut host = Host::new();
    let other = host.open_channel("rendimiento");
    let other_message = message(&host, other, 10, "ÚNICO needle elsewhere");
    host.event(Event::MessageUpdated(other_message));
    let channel = host.open_channel("general");
    let found = message(&host, channel, 10, "Texto ÚNICO Needle local");
    let expected = format!("message-search:go:{channel}:{}", found.id);
    host.event(Event::MessageUpdated(found));
    host.key(Key::F, false, true);
    host.type_text("único needle");
    assert_eq!(result_ids(&mut host), vec![expected]);
    change_query(&mut host, "no existe este texto");
    assert!(result_ids(&mut host).is_empty());
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Sin resultados")));
    host.key(Key::Escape, false, false);
    host.open_channel("rendimiento");
    host.key(Key::F, false, true);
    host.frame();
    assert_eq!(host.tree.input_value(QUERY), Some(""));
    assert!(
        result_ids(&mut host).is_empty(),
        "another conversation starts a new query"
    );
}

#[test]
fn keyboard_selects_an_older_result_and_jumps_without_marking_the_end_read() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    for index in [10, 20] {
        let found = message(&host, channel, index, "needle keyboard");
        host.event(Event::MessageUpdated(found));
    }
    host.key(Key::F, false, true);
    host.type_text("needle keyboard");
    host.key(Key::ArrowDown, false, false);
    host.sent.clear();
    host.key(Key::Enter, false, false);
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
    let visible = host.tree.visible_rows(&timeline_id(channel)).unwrap();
    assert!(
        visible.contains(&10),
        "the older result is visible: {visible:?}"
    );
    assert!(!host.tree.is_scroll_at_end(&timeline_id(channel), 24.0));
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::MarkRead { .. })));
}

#[test]
fn results_follow_edits_deletions_and_new_messages_and_reject_a_stale_click() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut found = message(&host, channel, 10, "needle live");
    let id = format!("message-search:go:{channel}:{}", found.id);
    host.key(Key::F, false, true);
    host.type_text("needle live");
    assert!(result_ids(&mut host).is_empty());
    host.event(Event::MessageUpdated(found.clone()));
    assert_eq!(result_ids(&mut host), vec![id.clone()]);
    found.content = "edited away".into();
    found.edited = Some(Timestamp(1_800_000_000_001));
    host.event(Event::MessageUpdated(found.clone()));
    assert!(result_ids(&mut host).is_empty());
    found.id = MessageId(host.store.timeline(channel).unwrap().newest().unwrap().id.0 + 1);
    found.content = "needle live new".into();
    host.event(Event::MessageCreated {
        message: found.clone(),
        nonce: None,
    });
    assert_eq!(result_ids(&mut host).len(), 1);
    host.event(Event::MessageDeleted {
        channel,
        message: found.id,
    });
    assert!(result_ids(&mut host).is_empty());
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id },
        host.now,
    );
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::ScrollToIndex { .. })));
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(QUERY));
}

#[test]
fn clicking_a_result_reveals_it_and_returns_focus_to_the_composer() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let found = message(&host, channel, 15, "needle pointer");
    let id = format!("message-search:go:{channel}:{}", found.id);
    host.event(Event::MessageUpdated(found));
    host.key(Key::F, false, true);
    host.type_text("needle pointer");
    host.click(&id);
    host.frame();
    assert!(host
        .tree
        .visible_rows(&timeline_id(channel))
        .unwrap()
        .contains(&15));
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
}

#[test]
fn five_thousand_search_results_build_only_visible_rows_and_arrows_reveal_selection() {
    let mut host = Host::new();
    let channel = host.open_channel("rendimiento");
    let (mut messages, _) = host.provider.history(channel, None, 5_000);
    for message in &mut messages {
        message.content = format!("needle {}", message.id);
    }
    host.event(Event::HistoryLoaded {
        request: 0,
        channel,
        before: None,
        messages,
        reached_start: true,
    });
    host.key(Key::F, false, true);
    host.type_text("needle");
    host.frame();
    let visible = host.tree.visible_rows(RESULTS).unwrap();
    assert!(visible.len() < 12, "{visible:?}");
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("5000 resultados")));
    host.key(Key::ArrowUp, false, false);
    host.frame();
    let visible = host.tree.visible_rows(RESULTS).unwrap();
    assert!(
        visible.contains(&4_999),
        "keyboard selection is visible: {visible:?}"
    );
    assert!(visible.len() < 12, "{visible:?}");
}

#[test]
fn search_shortcut_leaves_the_visible_custom_status_dialog_and_its_focus_alone() {
    use rune_components::profile::ids;

    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::CUSTOM);
    host.type_text("Estado pendiente");
    host.key(Key::F, false, true);
    host.frame();
    assert!(host.ui.search.is_none());
    assert_eq!(host.tree.focused_id(), Some(ids::CUSTOM_TEXT));
    assert_eq!(
        host.ui.profiles.custom.as_ref().unwrap().text,
        "Estado pendiente"
    );
    host.key(Key::Escape, false, false);
    assert!(
        host.ui.profiles.custom.is_none(),
        "Escape closes the visible dialog"
    );
}

#[test]
fn search_shortcut_does_not_stack_over_other_dialogs_or_confirmations() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.ui.server_menu.dialog = Some(Dialog::Server(None));
    host.key(Key::F, false, true);
    assert!(host.ui.search.is_none());
    host.key(Key::Escape, false, false);
    assert!(host.ui.server_menu.dialog.is_none());

    host.ui.channel_list.dialog = Some(crate::channel_list::ListDialog::Duplicate {
        channel,
        name: "Copia pendiente".into(),
    });
    host.key(Key::F, false, true);
    assert!(host.ui.search.is_none());
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_list.dialog.is_none());

    assert!(host.store.channel(channel).unwrap().topic.is_some());
    host.ui.channel_list.topic = Some(channel);
    host.key(Key::F, false, true);
    assert!(host.ui.search.is_none());
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_list.topic.is_none());

    host.ui.confirm_delete = Some(host.store.timeline(channel).unwrap().newest().unwrap().id);
    host.key(Key::F, false, true);
    assert!(host.ui.search.is_none());
    host.key(Key::Escape, false, false);
    assert!(host.ui.confirm_delete.is_none());
}

#[test]
fn profile_updates_refresh_result_names_without_changing_the_selected_message() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    for index in [10, 20] {
        let found = message(&host, channel, index, "needle selection");
        host.event(Event::MessageUpdated(found));
    }
    host.key(Key::F, false, true);
    host.type_text("needle selection");
    host.key(Key::ArrowDown, false, false);
    let author = host.store.timeline(channel).unwrap().messages()[10].author;
    let mut user = host.store.user(author).unwrap().clone();
    user.name = "Nombre actualizado".into();
    host.event(Event::UserChanged(user));
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Nombre actualizado"));
    host.key(Key::Enter, false, false);
    host.frame();
    let visible = host.tree.visible_rows(&timeline_id(channel)).unwrap();
    assert!(
        visible.contains(&10),
        "the selected message stays visible: {visible:?}"
    );
}

#[test]
fn opening_search_dismisses_channel_and_voice_context_menus() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.ui.channel_list.menu = Some((
        crate::channel_list::Target::Channel(channel),
        (300.0, 200.0),
    ));
    host.key(Key::F, false, true);
    host.frame();
    assert!(host.ui.channel_list.menu.is_none());
    assert_eq!(host.tree.focused_id(), Some(QUERY));
    host.key(Key::Escape, false, false);

    host.ui.voice.menu = Some(crate::voice::Menu {
        user: host.store.me().unwrap(),
        channel,
        at: Some((300.0, 200.0)),
        row: true,
    });
    host.key(Key::F, false, true);
    host.frame();
    assert!(host.ui.voice.menu.is_none());
    assert_eq!(host.tree.focused_id(), Some(QUERY));
}

fn filter_fixture(host: &mut Host, channel: ChannelId) -> Vec<rune_model::Message> {
    let account = host.store.me().unwrap();
    let other = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| message.author != account)
        .unwrap()
        .author;
    let mut messages: Vec<_> = (0..4)
        .map(|index| {
            let mut found = message(host, channel, index, "needle filters");
            found.author = if index == 0 { account } else { other };
            found.pinned = index == 0 || index == 2;
            found.attachments.clear();
            found.stickers.clear();
            if index < 2 {
                found.attachments.push(rune_model::Attachment {
                    id: rune_model::AttachmentId(index as u64),
                    filename: "notes.txt".into(),
                    size_bytes: 10,
                    content_type: Some("text/plain".into()),
                    dimensions: None,
                    preview: None,
                    download: None,
                    spoiler: false,
                    voice: None,
                    explicit: false,
                    placeholder: None,
                });
            }
            found
        })
        .collect();
    messages[3].content = "another message".into();
    messages[3].stickers.push(rune_model::Sticker {
        id: rune_model::StickerId(7),
        name: "A sticker is not an attached file".into(),
        image: None,
        available: true,
    });
    host.event(Event::HistoryLoaded {
        request: 0,
        channel,
        before: None,
        messages: messages.clone(),
        reached_start: true,
    });
    messages
}

#[test]
fn search_filters_work_without_text_and_combine_with_literal_text() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let messages = filter_fixture(&mut host, channel);
    host.key(Key::F, false, true);
    assert!(result_ids(&mut host).is_empty());

    host.click("message-search:files");
    assert_eq!(result_ids(&mut host).len(), 2);
    host.click("message-search:mine");
    assert_eq!(
        result_ids(&mut host),
        vec![format!("message-search:go:{channel}:{}", messages[0].id)]
    );
    host.click("message-search:pinned");
    assert_eq!(result_ids(&mut host).len(), 1);
    host.click("message-search:mine");
    host.click("message-search:files");
    assert_eq!(result_ids(&mut host).len(), 2);

    change_query(&mut host, "ANOTHER");
    assert!(result_ids(&mut host).is_empty());
    host.click("message-search:pinned");
    assert_eq!(
        result_ids(&mut host),
        vec![format!("message-search:go:{channel}:{}", messages[3].id)]
    );
    change_query(&mut host, "from:me");
    assert!(
        result_ids(&mut host).is_empty(),
        "query syntax stays literal"
    );
}

#[test]
fn filtered_results_follow_live_changes_and_reject_a_stale_result_action() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut messages = filter_fixture(&mut host, channel);
    host.key(Key::F, false, true);
    host.type_text("needle");
    host.key(Key::ArrowDown, false, false);
    let excluded = format!("message-search:go:{channel}:{}", messages[1].id);
    host.click("message-search:pinned");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: excluded },
        host.now,
    );
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::ScrollToIndex { .. })));

    messages[1].pinned = true;
    messages[1].edited = Some(Timestamp(1_800_000_000_001));
    host.event(Event::MessageUpdated(messages[1].clone()));
    assert_eq!(result_ids(&mut host).len(), 3);
    messages[0].pinned = false;
    messages[0].edited = Some(Timestamp(1_800_000_000_001));
    host.event(Event::MessageUpdated(messages[0].clone()));
    assert_eq!(result_ids(&mut host).len(), 2);
    host.tree.focus(QUERY);
    host.key(Key::Enter, false, false);
    host.frame();
    assert!(host
        .tree
        .visible_rows(&timeline_id(channel))
        .unwrap()
        .contains(&2));
}

#[test]
fn search_filters_are_keyboard_accessible_and_fit_the_minimum_window() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    filter_fixture(&mut host, channel);
    host.key(Key::F, false, true);
    host.frame();
    host.tree.focus("message-search:files");
    host.key(Key::Space, false, false);
    assert_eq!(result_ids(&mut host).len(), 2);
    assert_eq!(host.tree.focused_id(), Some("message-search:files"));
    host.key(Key::Escape, false, false);
    host.key(Key::F, false, true);
    assert!(
        result_ids(&mut host).is_empty(),
        "a new search clears filters"
    );

    host.click("message-search:files");
    let mut context = host.context();
    context.window = Size::new(720.0, 480.0);
    host.tree.set_root(view(&host.ui, &host.store, &context));
    let frame = host.tree.layout(context.window);
    for id in [
        QUERY,
        "message-search:mine",
        "message-search:files",
        "message-search:pinned",
        "message-search:close",
    ] {
        let rect = frame
            .regions
            .iter()
            .find(|region| region.id == id)
            .unwrap()
            .rect;
        assert!(
            rect.x >= 0.0
                && rect.y >= 0.0
                && rect.x + rect.width <= 720.0
                && rect.y + rect.height <= 480.0,
            "{id}: {rect:?}"
        );
    }
}

#[test]
fn invitation_preview_discards_search_and_ignores_a_stale_search_open_action() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.type_text("Borrador antes de la invitación");
    host.key(Key::F, false, true);
    host.click("message-search:mine");
    let intents = open_invitation(
        &mut host.ui,
        &host.store,
        rune_provider::InvitationTarget {
            key: "local-preview".into(),
            url: "https://example.invalid/invite/local-preview".into(),
        },
    );
    host.run(intents);
    host.frame();
    assert!(host.ui.search.is_none());
    for id in [
        "message-search:open",
        "message-search:close",
        "message-search:mine",
    ] {
        let intents = update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked { id: id.into() },
            host.now,
        );
        assert!(
            intents.is_empty(),
            "the visible invitation owns focus: {id}"
        );
        assert!(host.ui.search.is_none());
    }
    host.key(Key::F, false, true);
    host.key(Key::Escape, false, false);
    host.frame();
    assert!(host.ui.invitation.is_none());
    assert!(host.ui.search.is_none());
    assert_eq!(
        host.ui.drafts.get(&channel).map(String::as_str),
        Some("Borrador antes de la invitación")
    );
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
}
