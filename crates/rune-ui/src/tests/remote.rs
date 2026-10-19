//! Reads, searches and members at the provider (P2), through the real limn
//! tree with the simulation's capabilities.

use super::*;
use rune_model::{Message, RemotePage, RemoteSearchError};

const QUERY: &str = "message-search:query";

fn remote_scope(host: &mut Host) {
    host.key(Key::F, false, true);
    host.click("message-search:scope");
    host.click("message-search:scope:remote");
}

fn searched(host: &Host) -> (u64, rune_model::RemoteSearch, u32) {
    host.sent
        .iter()
        .rev()
        .find_map(|command| match command {
            Command::SearchMessages {
                request,
                search,
                offset,
            } => Some((*request, search.clone(), *offset)),
            _ => None,
        })
        .expect("a search at the provider")
}

fn written(host: &mut Host, channel: ChannelId, content: &str, minutes: i64) -> Message {
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content = content.into();
    message.timestamp = message.timestamp.plus_minutes(minutes);
    host.event(Event::MessageCreated {
        message: message.clone(),
        nonce: None,
    });
    message
}

#[test]
fn filters_are_suggested_as_they_are_written() {
    let mut host = Host::new();
    host.open_channel("general");
    remote_scope(&mut host);
    // The empty field lists the filters.
    assert!(host.painted().iter().any(|text| text == "from:"));
    host.type_text("aguja has:li");
    assert!(host.painted().iter().any(|text| text == "enlace"));
    host.sent.clear();
    // Enter takes the suggestion; nothing goes yet.
    host.key(Key::Enter, false, false);
    assert!(host.sent.is_empty());
    assert_eq!(host.tree.input_value(QUERY), Some("aguja has:link "));
    let channel = host.ui.channel().unwrap();
    let server = host.store.channel(channel).unwrap().server.unwrap();
    let someone = host.store.members(server)[1].user;
    let handle = host.store.user(someone).unwrap().name.clone();
    host.type_text(&format!("from:{}", &handle[..2]));
    let shown = format!("@{handle}");
    assert!(host.painted().contains(&shown));
    host.key(Key::Escape, false, false);
    assert!(host.ui.search.is_some(), "Esc closes the suggestions first");
    assert!(!host.painted().contains(&shown));
    host.key(Key::Escape, false, false);
    assert!(host.ui.search.is_none());
}

#[test]
fn the_whole_history_merges_with_what_is_loaded() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let shared = written(&mut host, channel, "aguja compartida", 1);
    let local = written(&mut host, channel, "aguja local", 2);
    remote_scope(&mut host);
    host.type_text("aguja");
    assert!(host
        .sent
        .iter()
        .all(|command| !matches!(command, Command::SearchMessages { .. })));
    host.key(Key::Enter, false, false);
    let (request, search, offset) = searched(&host);
    assert_eq!((search.text.as_str(), offset), ("aguja", 0));
    assert_eq!(search.channel, channel);
    assert!(
        search.server.is_some(),
        "a server's channel searches the server"
    );
    let mut old = shared.clone();
    old.id = MessageId(channel.0 * 10_000_000);
    old.content = "aguja vieja".into();
    old.timestamp = Timestamp(1_000);
    host.event(Event::SearchResults {
        request,
        result: Ok(RemotePage {
            messages: vec![shared.clone(), old],
            total: 2,
            offset: 0,
            partial: false,
        }),
    });
    let painted = host.painted();
    let meta = |text: &str| painted.iter().filter(|line| line.ends_with(text)).count();
    assert_eq!(
        meta("· Simulación local"),
        1,
        "only the provider found the old one"
    );
    assert_eq!(
        meta("· Simulación local y cargado"),
        1,
        "the shared one, once"
    );
    assert_eq!(meta("· cargado"), 1, "the newest is not indexed yet");
    assert!(painted
        .iter()
        .any(|text| text.contains("2 resultados en Simulación local")));
    assert!(painted
        .iter()
        .any(|text| text.contains("1 cargados aún sin indexar")));
    assert!(local.id > shared.id);
}

#[test]
fn polls_sites_and_relevance_go_to_the_provider_in_its_order() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let linked = written(&mut host, channel, "aguja en https://docs.rs/limn", 1);
    remote_scope(&mut host);
    // The service's own spelling of the key, and a site the loaded link names.
    host.type_text("aguja has:poll linkFrom:do");
    assert!(host.painted().iter().any(|text| text == "docs.rs"));
    host.key(Key::Tab, false, false);
    host.key(Key::Enter, false, false);
    let (_, search, _) = searched(&host);
    assert_eq!(search.has, vec![rune_model::Has::Poll]);
    assert_eq!(search.link_hosts, vec!["docs.rs".to_owned()]);
    assert_eq!(search.order, rune_model::RemoteOrder::Newest);
    // Choosing the order asks the same page again, ranked.
    host.click("message-search:order");
    host.click("message-search:order:relevance");
    let (request, search, offset) = searched(&host);
    assert_eq!(
        (search.order, offset),
        (rune_model::RemoteOrder::Relevance, 0)
    );
    let mut old = linked.clone();
    old.id = MessageId(channel.0 * 10_000_000);
    old.content = "aguja vieja".into();
    old.timestamp = Timestamp(1_000);
    host.event(Event::SearchResults {
        request,
        result: Ok(RemotePage {
            messages: vec![old.clone(), linked.clone()],
            total: 2,
            offset: 0,
            partial: false,
        }),
    });
    let painted = host.painted();
    let at = |text: &str| painted.iter().position(|line| line == text);
    assert!(
        at("aguja vieja").unwrap() < at("aguja en https://docs.rs/limn").unwrap(),
        "the provider's ranking stays: {painted:?}"
    );
}

#[test]
fn a_short_ranked_page_with_more_to_come_leaves_loaded_ones_out() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let loaded = written(&mut host, channel, "aguja local", 1);
    remote_scope(&mut host);
    host.type_text("aguja");
    host.key(Key::Enter, false, false);
    host.click("message-search:order");
    host.click("message-search:order:relevance");
    let (request, search, _) = searched(&host);
    assert_eq!(search.order, rune_model::RemoteOrder::Relevance);
    let mut old = loaded.clone();
    old.id = MessageId(channel.0 * 10_000_000);
    old.content = "aguja vieja".into();
    old.timestamp = Timestamp(1_000);
    // Fewer than a page, but the count says more follow: the loaded one may
    // be ranked on a later page.
    host.event(Event::SearchResults {
        request,
        result: Ok(RemotePage {
            messages: vec![old],
            total: 30,
            offset: 0,
            partial: false,
        }),
    });
    let painted = host.painted();
    let meta = |text: &str| painted.iter().filter(|line| line.ends_with(text)).count();
    assert_eq!(meta("· Simulación local"), 1, "{painted:?}");
    assert_eq!(meta("· cargado"), 0, "{painted:?}");
}

#[test]
fn an_index_in_progress_is_asked_again_once_and_pages_go_by_25() {
    let mut host = Host::new();
    host.open_channel("general");
    remote_scope(&mut host);
    host.type_text("rust");
    host.key(Key::Enter, false, false);
    let (request, ..) = searched(&host);
    host.event(Event::SearchResults {
        request,
        result: Err(RemoteSearchError::Indexing {
            retry_after_ms: 1_000,
        }),
    });
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("preparando el índice")));
    let later = Timestamp(i64::MAX / 2);
    assert!(rune_ui_next_tick(&host).is_some());
    let intents = tick(&mut host.ui, &host.store, later);
    host.run(intents);
    let (again, ..) = searched(&host);
    assert_ne!(again, request);
    host.event(Event::SearchResults {
        request: again,
        result: Err(RemoteSearchError::Indexing {
            retry_after_ms: 1_000,
        }),
    });
    let intents = tick(&mut host.ui, &host.store, later);
    assert!(intents.is_empty(), "never a third time");
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("sigue en preparación")));
    // A page of 25 out of 60: the next one starts at 25.
    host.click("message-search:refresh");
    let (request, search, _) = searched(&host);
    let template = host
        .store
        .timeline(search.channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    let messages = (0..25)
        .map(|index| {
            let mut message = template.clone();
            message.id = MessageId(template.id.0 - 100 - index);
            message.timestamp = Timestamp(template.timestamp.0 - 60_000 * (index as i64 + 1));
            message.content = "rust".into();
            message
        })
        .collect();
    host.event(Event::SearchResults {
        request,
        result: Ok(RemotePage {
            messages,
            total: 60,
            offset: 0,
            partial: true,
        }),
    });
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text.contains("página 1 de 3"))
            && painted
                .iter()
                .any(|text| text.contains("indexa mensajes viejos")),
        "{painted:?}"
    );
    host.click("message-search:next");
    assert_eq!(searched(&host).2, 25);
}

fn rune_ui_next_tick(host: &Host) -> Option<Duration> {
    next_tick(&host.ui, Timestamp(0))
}

#[test]
fn writing_a_mention_looks_members_up_once_per_name() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let server = host.store.channel(channel).unwrap().server.unwrap();
    host.type_text("hola @pa");
    let lookups = |host: &Host| {
        host.sent
            .iter()
            .filter_map(|command| match command {
                Command::SearchMembers { server, query } => Some((*server, query.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(lookups(&host), vec![(server, "pa".to_owned())]);
    host.type_text("b");
    host.frame();
    assert_eq!(
        lookups(&host),
        vec![(server, "pa".to_owned()), (server, "pab".to_owned())]
    );
    // Found members join the completions.
    let crowd = rune_model::User {
        identity: Default::default(),
        id: UserId(900_123),
        name: "pablo.remoto".into(),
        display_name: Some("Pablo Remoto".into()),
        accent: rune_model::Rgb(0),
        avatar: None,
        banner: Default::default(),
        bot: false,
        pronouns: None,
        about: None,
        default_avatar: None,
    };
    host.event(Event::UserChanged(crowd.clone()));
    host.event(Event::MembersFound {
        server,
        query: "pab".into(),
        members: vec![rune_model::Member {
            user: crowd.id,
            server,
            ..Default::default()
        }],
    });
    let found = completions(&host.ui, &host.store, channel).unwrap();
    assert!(found
        .items
        .iter()
        .any(|item| item.insert == "@pablo.remoto"));
}

#[test]
fn a_server_is_marked_read_from_its_menu_and_a_reminder_reaches_the_provider() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let server = host.store.channel(channel).unwrap().server.unwrap();
    let other = host
        .store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .find(|found| found.id != channel && found.kind.has_messages())
        .map(|found| found.id)
        .unwrap();
    host.event(Event::LatestMessages(vec![(
        other,
        MessageId(other.0 * 10_000_000 + 9_999),
    )]));
    assert!(host.store.server_unread(server).0);
    host.sent.clear();
    host.click(rune_components::server_menu::ids::TOGGLE);
    host.click(&format!("sm:read:{server}"));
    assert!(host.sent.iter().any(|command| matches!(command,
        Command::MarkServerRead { server: found, .. } if *found == server)));
    assert!(!host.store.server_unread(server).0);
    // «Marcar como no leído» names the message at the provider.
    let message = host.store.timeline(channel).unwrap().messages()[3].id;
    let command = host.store.mark_unread_command(channel, message);
    assert!(
        matches!(command, Some(Command::MarkUnread { message: found, .. }) if found == message)
    );
}
