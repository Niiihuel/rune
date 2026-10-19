use super::*;
use rune_model::{LocalReadError, SearchQuery};

fn wait_read(client: &mut Client) -> LocalReadResult {
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        client.poll(64);
        if let Some(result) = client.poll_local_read() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("local read did not terminate");
}

fn query(channel: ChannelId) -> SearchQuery {
    SearchQuery {
        channel,
        text: "unique_local_needle".into(),
        ..Default::default()
    }
}

#[test]
fn local_reads_without_an_account_store_terminate_immediately() {
    let mut client = client(Arc::new(mock()), None);
    assert!(!client.local_history_available());
    client.search_local(41, query(ChannelId(104)), None);
    assert!(
        client.next_deadline(Instant::now()).is_some(),
        "an immediate result schedules another poll"
    );
    assert!(matches!(
        client.poll_local_read(),
        Some(LocalReadResult::Search {
            request: 41,
            result: Err(LocalReadError::Unavailable),
            ..
        })
    ));
    client.load_cached_context(42, ChannelId(104), MessageId(1));
    assert!(matches!(
        client.poll_local_read(),
        Some(LocalReadResult::Context {
            request: 42,
            result: Err(LocalReadError::Unavailable),
            ..
        })
    ));
    assert!(client.poll_local_read().is_none());
}

#[test]
fn local_search_context_and_cancel_are_correlated_across_account_boundaries() {
    let directory = std::env::temp_dir().join(format!("rune-local-context-{}", std::process::id()));
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(directory.join("rune.db")));
    let channel = text_channels(&ready(&mut client))[2];
    let mut message = provider.history(channel, None, 1).0.pop().unwrap();
    message.content = "unique_local_needle".into();
    let anchor = message.id;
    client.storage.as_ref().unwrap().save(vec![message]);
    assert!(client.local_history_available());
    client.search_local(51, query(channel), None);
    let result = wait_read(&mut client);
    assert!(
        matches!(result, LocalReadResult::Search { request: 51, channel: actual, result: Ok(ref page) } if actual == channel && page.hits.len() == 1 && page.hits[0].id == anchor)
    );
    client.load_cached_context(52, channel, anchor);
    assert!(
        matches!(wait_read(&mut client), LocalReadResult::Context { request: 52, result: Ok(ref page), .. } if page.anchor == anchor && page.messages.len() == 1)
    );

    client.search_local(53, query(channel), None);
    client.command(Command::Cancel(53));
    client.storage.as_ref().unwrap().snapshot();
    assert!(client.poll_local_read().is_none());
    client.search_local(54, query(channel), None);
    let mut snapshot = provider.snapshot();
    snapshot.me.as_mut().unwrap().id = rune_model::UserId(99);
    client.accept(
        Envelope {
            generation: client.generation,
            event: Event::Ready(Box::new(snapshot)),
        },
        &mut Vec::new(),
    );
    assert!(!client.local_history_available());
    assert!(!matches!(
        client.poll_local_read(),
        Some(LocalReadResult::Search { result: Ok(_), .. })
    ));
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn rejected_local_results_are_not_exposed_after_cancel_or_sign_out() {
    let mut client = client(Arc::new(mock()), None);
    client.search_local(61, query(ChannelId(104)), None);
    client.command(Command::Cancel(61));
    assert!(client.poll_local_read().is_none());
    client.search_local(62, query(ChannelId(104)), None);
    client.accept(
        Envelope {
            generation: 0,
            event: Event::Authentication(AuthenticationState::SignedOut),
        },
        &mut Vec::new(),
    );
    assert!(client.poll_local_read().is_none());
}

#[test]
fn local_read_progresses_when_provider_events_use_the_entire_frame_quota() {
    let directory = std::env::temp_dir().join(format!("rune-local-quota-{}", std::process::id()));
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(directory.join("rune.db")));
    let channel = text_channels(&ready(&mut client))[2];
    let mut message = provider.history(channel, None, 1).0.pop().unwrap();
    message.content = "unique_local_needle".into();
    let storage = client.storage.as_ref().unwrap();
    storage.save(vec![message]);
    for request in 100..116 {
        assert!(storage.request_cached(
            CacheRequest {
                epoch: client.cache_epoch,
                request,
                channel
            },
            1
        ));
    }
    let barrier = storage.flush().unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while !storage.committed(barrier) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    let (sender, receiver) = mpsc::channel(2);
    client.events = receiver;
    client.search_local(71, query(channel), None);
    let until = Instant::now() + Duration::from_millis(500);
    let mut result = None;
    while Instant::now() < until {
        sender
            .try_send(Envelope {
                generation: client.generation,
                event: Event::Connection(ConnectionState::Connected),
            })
            .unwrap();
        assert_eq!(client.poll(1).len(), 1);
        result = client.poll_local_read();
        if result.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        matches!(result, Some(LocalReadResult::Search {request:71, result:Ok(ref page), ..}) if page.hits.len()==1),
        "cached preview slots must not starve an admitted local read"
    );
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn local_continuation_survives_a_connection_loss_without_a_cache_mutation() {
    let directory = std::env::temp_dir().join(format!("rune-local-offline-{}", std::process::id()));
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(directory.join("rune.db")));
    let channel = text_channels(&ready(&mut client))[2];
    let mut messages = provider.history(channel, None, 21).0;
    assert_eq!(messages.len(), 21);
    for message in &mut messages {
        message.content = "unique_local_needle".into();
    }
    client.storage.as_ref().unwrap().save(messages);
    client.search_local(81, query(channel), None);
    let Some(cursor) = (match wait_read(&mut client) {
        LocalReadResult::Search {
            result: Ok(page), ..
        } => page.continuation,
        _ => None,
    }) else {
        panic!("first page must have a continuation");
    };
    client.accept(
        Envelope {
            generation: client.generation,
            event: Event::Connection(ConnectionState::Disconnected {
                reason: "synthetic offline transition".into(),
            }),
        },
        &mut Vec::new(),
    );
    assert!(client.local_history_available());
    client.search_local(82, query(channel), Some(cursor));
    assert!(
        matches!(wait_read(&mut client), LocalReadResult::Search {request:82, result:Ok(ref page), ..} if page.hits.len()==1),
        "a network transition must not invalidate a local-only query"
    );
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}
