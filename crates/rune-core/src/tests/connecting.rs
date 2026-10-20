//! Reads asked before the session is up (arranque-sin-avisos): RUNE opens
//! from its local store and asks at once for the open conversation's
//! history and the server's events; a session still connecting refuses
//! them, as the unofficial one does. Nothing of that reaches the person.

use super::*;
use rune_model::ServerId;

/// A provider that refuses every command until it says it is up, as a
/// session still connecting does, and answers reads after that. The test
/// sends the connection's events.
struct Connecting {
    inner: MockProvider,
    script: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Event>>>,
}

impl Provider for Connecting {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }

    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> rune_provider::BoxFuture<'a, Result<(), ProviderError>> {
        Box::pin(async move {
            let Some(mut script) = self.script.lock().unwrap().take() else {
                return Ok(());
            };
            let mut up = false;
            loop {
                let event = tokio::select! {
                    event = script.recv() => match event {
                        Some(event) => {
                            match &event {
                                Event::Ready(_) | Event::Connection(ConnectionState::Connected) => up = true,
                                Event::Connection(_) => up = false,
                                _ => {}
                            }
                            event
                        }
                        None => return Ok(()),
                    },
                    command = commands.recv() => match (command, up) {
                        (None, _) => return Ok(()),
                        (Some(command), false) => match Failure::of(&command) {
                            Some(failure) => Event::Failed {
                                failure,
                                reason: "La conexión todavía no está disponible.".into(),
                                ambiguous: false,
                            },
                            None => continue,
                        },
                        (Some(Command::LoadHistory { request, channel, before, .. }), true) => {
                            Event::HistoryLoaded { request, channel, before, messages: Vec::new(), reached_start: true }
                        }
                        (Some(Command::LoadScheduledEvents { request, server }), true) => {
                            Event::ScheduledEvents { request, server, events: Vec::new() }
                        }
                        (Some(_), true) => continue,
                    },
                };
                if events.send(event).await.is_err() {
                    return Ok(());
                }
            }
        })
    }
}

fn events(request: u64) -> Command {
    Command::LoadScheduledEvents {
        request,
        server: ServerId(10),
    }
}

/// Poll for a while, every update.
fn settle(client: &mut Client) -> Vec<Update> {
    let mut updates = Vec::new();
    for _ in 0..50 {
        updates.extend(client.poll(64));
        std::thread::sleep(Duration::from_millis(2));
    }
    updates
}

fn failures(updates: &[Update]) -> Vec<&Event> {
    updates
        .iter()
        .filter_map(|update| match update {
            Update::Provider(event @ Event::Failed { .. }) => Some(event),
            _ => None,
        })
        .collect()
}

#[test]
fn reads_asked_while_the_session_connects_wait_for_it_without_a_notice() {
    let (script, receiver) = tokio::sync::mpsc::unbounded_channel();
    let provider = Arc::new(Connecting {
        inner: mock(),
        script: std::sync::Mutex::new(Some(receiver)),
    });
    let snapshot = provider.inner.snapshot();
    let channel = text_channels(&snapshot.channels)[0];
    // A page's wait runs from when it is asked of the session, not before.
    let deadlines = Deadlines {
        history: Duration::from_millis(30),
        ..Deadlines::default()
    };
    let mut client = client_with(provider, None, deadlines);

    // Opened from the local store: the interface asks before the session is up.
    client.load_latest(channel);
    client.command(events(1));
    let early = settle(&mut client);
    assert!(failures(&early).is_empty(), "{:?}", failures(&early));

    // READY: the events asked go out; the interface asks the history again.
    script.send(Event::Ready(Box::new(snapshot))).unwrap();
    script
        .send(Event::Connection(ConnectionState::Connected))
        .unwrap();
    let mut later = settle(&mut client);
    assert!(later
        .iter()
        .any(|update| matches!(update, Update::Provider(Event::Ready(_)))));
    client.load_latest(channel);
    later.extend(settle(&mut client));
    assert!(later.iter().any(|update| matches!(update,
        Update::Provider(Event::HistoryLoaded { channel: loaded, .. }) if *loaded == channel)));
    assert!(later.iter().any(|update| matches!(
        update,
        Update::Provider(Event::ScheduledEvents { request: 1, .. })
    )));

    // Lost and back: what was asked meanwhile goes out as it comes back.
    script
        .send(Event::Connection(ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 10,
        }))
        .unwrap();
    later.extend(settle(&mut client));
    client.load_latest(channel);
    client.command(events(2));
    later.extend(settle(&mut client));
    script
        .send(Event::Connection(ConnectionState::Connected))
        .unwrap();
    let back = settle(&mut client);
    assert!(back.iter().any(|update| matches!(update,
        Update::Provider(Event::HistoryLoaded { channel: loaded, .. }) if *loaded == channel)));
    assert!(back.iter().any(|update| matches!(
        update,
        Update::Provider(Event::ScheduledEvents { request: 2, .. })
    )));
    later.extend(back);
    assert!(failures(&later).is_empty(), "{:?}", failures(&later));

    // Given up: what waits fails, for its view to say so.
    script
        .send(Event::Connection(ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 10,
        }))
        .unwrap();
    settle(&mut client);
    client.command(events(3));
    script
        .send(Event::Connection(ConnectionState::Disconnected {
            reason: "Sin red".into(),
        }))
        .unwrap();
    let gone = settle(&mut client);
    assert!(gone.iter().any(|update| matches!(update,
        Update::Provider(Event::Failed { failure: Failure::ScheduledEvents { request: 3, .. }, reason, .. })
            if reason == "Sin red")));
}
