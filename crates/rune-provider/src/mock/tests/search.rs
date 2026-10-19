//! The simulation's reminders, whole server reads, searches and member
//! lookups (P2), deterministic for a seed.

use super::*;
use rune_model::{Has, RemoteOrder, RemoteSearch, RemoteSearchError};

fn channel(world: &World, name: &str) -> ChannelId {
    world
        .channels
        .iter()
        .find(|channel| channel.name == name)
        .unwrap()
        .id
}

fn page(events: Vec<Event>) -> Result<rune_model::RemotePage, RemoteSearchError> {
    match events.into_iter().last() {
        Some(Event::SearchResults { request: 4, result }) => result,
        other => panic!("search results expected: {other:?}"),
    }
}

#[test]
fn a_search_pages_through_every_message_and_its_filters() {
    let mut world = World::build(&config());
    let general = channel(&world, "general");
    let server = world.channel(general).unwrap().server;
    let search = RemoteSearch {
        channel: general,
        server,
        text: "rust".into(),
        ..Default::default()
    };
    let first = page(world.search_messages(4, &search, 0)).unwrap();
    assert!(first.total > 25, "{} found", first.total);
    assert_eq!(first.messages.len(), 25);
    assert!(first.fits(&search, 0));
    assert!(first
        .messages
        .iter()
        .all(|message| message.content.to_lowercase().contains("rust")));
    let second = page(world.search_messages(4, &search, 25)).unwrap();
    assert!(second.fits(&search, 25));
    assert!(rune_model::newer(
        first.messages.last().unwrap(),
        &second.messages[0]
    ));
    assert_eq!(
        page(world.search_messages(4, &search, 0)).unwrap(),
        first,
        "the same seed finds the same"
    );
    // Only links, of one author, in one channel.
    let author = first.messages[0].author;
    let narrow = RemoteSearch {
        text: String::new(),
        authors: vec![author],
        has: vec![Has::Link],
        channels: vec![general],
        ..search.clone()
    };
    let found = page(world.search_messages(4, &narrow, 0)).unwrap();
    assert!(found.messages.iter().all(|message| message.author == author
        && message.channel == general
        && message.content.contains("https://")));
    assert!(page(world.search_messages(4, &search, 7)).is_err());
}

#[test]
fn polls_links_to_a_site_and_relevance_are_searched_too() {
    let mut world = World::build(&config());
    let general = channel(&world, "general");
    let server = world.channel(general).unwrap().server;
    let base = RemoteSearch {
        channel: general,
        server,
        ..Default::default()
    };
    let polls = RemoteSearch {
        has: vec![Has::Poll],
        ..base.clone()
    };
    let found = page(world.search_messages(4, &polls, 0)).unwrap();
    assert!(found.total > 0, "the simulated poll");
    assert!(found.messages.iter().all(|message| message.poll.is_some()));
    let linked = RemoteSearch {
        link_hosts: vec!["github.com".into()],
        ..base.clone()
    };
    let found = page(world.search_messages(4, &linked, 0)).unwrap();
    assert!(found.total > 0);
    assert!(found
        .messages
        .iter()
        .all(|message| message.content.contains("https://github.com")));
    // The most mentions of the text first, whatever their dates: the oldest
    // message, saying it twice, leads ranked and is nowhere near by date.
    let mut oldest = world.message(general, message_id(general, 0)).unwrap();
    oldest.content = "Rust, siempre rust".into();
    world.store(general, oldest.clone());
    let ranked = RemoteSearch {
        text: "rust".into(),
        order: RemoteOrder::Relevance,
        ..base
    };
    let first = page(world.search_messages(4, &ranked, 0)).unwrap();
    assert!(first.fits(&ranked, 0));
    assert_eq!(first.messages[0].id, oldest.id);
    let times = |message: &Message| message.content.to_lowercase().matches("rust").count();
    assert!(first
        .messages
        .windows(2)
        .all(|pair| times(&pair[0]) >= times(&pair[1])));
    assert_eq!(page(world.search_messages(4, &ranked, 0)).unwrap(), first);
    let newest = RemoteSearch {
        order: RemoteOrder::Newest,
        ..ranked
    };
    let by_date = page(world.search_messages(4, &newest, 0)).unwrap();
    assert!(by_date
        .messages
        .iter()
        .all(|message| message.id != oldest.id));
}

#[test]
fn the_first_search_through_the_big_channel_waits_for_its_index() {
    let mut world = World::build(&config());
    let big = channel(&world, "rendimiento");
    let search = RemoteSearch {
        channel: big,
        server: world.channel(big).unwrap().server,
        text: "benchmark".into(),
        ..Default::default()
    };
    assert_eq!(
        page(world.search_messages(4, &search, 0)),
        Err(RemoteSearchError::Indexing {
            retry_after_ms: 1_500
        })
    );
    assert!(page(world.search_messages(4, &search, 0)).is_ok());
}

#[test]
fn members_are_found_by_name_with_the_crowd_of_the_first_server() {
    let world = World::build(&config());
    let first = world.servers[0].id;
    let found = world.search_members(first, "Agus");
    let Some(Event::MembersFound {
        server,
        query,
        members,
    }) = found.last()
    else {
        panic!("members found")
    };
    assert_eq!((*server, query.as_str()), (first, "agus"));
    assert!(!members.is_empty() && members.len() <= 100);
    assert_eq!(found.len(), members.len() + 1, "their accounts first");
    assert!(world
        .snapshot()
        .users
        .iter()
        .all(|user| user.id != members[0].user));
    // Known people too, and nothing for a server the account is not in.
    let someone = world
        .members
        .iter()
        .find(|member| member.server == first && member.user != ME)
        .and_then(|member| world.users.iter().find(|user| user.id == member.user))
        .unwrap();
    let known = world.search_members(first, &someone.name[..2]);
    assert!(
        matches!(known.last(), Some(Event::MembersFound { members, .. }) if !members.is_empty())
    );
    assert!(world.search_members(ServerId(999), "ada").is_empty());
}

#[test]
fn a_reminder_moves_the_cursor_back_and_a_server_read_forward() {
    let mut world = World::build(&config());
    let general = channel(&world, "general");
    let server = world.channel(general).unwrap().server.unwrap();
    let newest = world
        .snapshot()
        .last_messages
        .iter()
        .find(|(id, _)| *id == general)
        .unwrap()
        .1;
    let read = world.mark_server_read(ME, server);
    assert!(read.iter().any(|event| matches!(event,
        Event::ReadStateConfirmed { channel, message, request: None, .. }
            if *channel == general && *message == newest)));
    let Event::MarkedUnread {
        through,
        anchor,
        version,
        ..
    } = world.mark_unread(ME, general, newest, 0)
    else {
        panic!("a reminder")
    };
    assert_eq!(anchor, Some(newest));
    assert_eq!(through, Some(MessageId(newest.0 - 1)));
    assert!(version > 1);
    assert!(matches!(
        world.mark_unread(ME, general, MessageId(1), 0),
        Event::Failed { .. }
    ));
}
