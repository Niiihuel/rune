use super::*;
use rune_model::Thread;
use rune_provider::Provider;

fn store() -> Store {
    let provider = rune_provider::mock::MockProvider::new(Default::default());
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    store
}

fn named(store: &Store, name: &str) -> Channel {
    store
        .channels
        .values()
        .find(|channel| channel.name == name)
        .cloned()
        .unwrap()
}

#[test]
fn threads_hang_from_their_channel_newest_first_and_only_where_it_is_seen() {
    let mut store = store();
    let gallery = named(&store, "galería");
    let server = gallery.server.unwrap();
    let tree: Vec<ChannelId> = store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .map(|channel| channel.id)
        .collect();
    assert!(tree
        .iter()
        .all(|id| !store.channel(*id).unwrap().is_thread()));
    let threads = store.threads(gallery.id, false);
    assert_eq!(threads.len(), 2);
    let activity: Vec<_> = threads
        .iter()
        .map(|t| t.thread.as_ref().unwrap().last_activity)
        .collect();
    assert!(activity.windows(2).all(|pair| pair[0] >= pair[1]));

    // A session starts with the forum's active posts; archived ones come
    // when asked for (see `threads_come_page_by_page_once_each`).
    let forum = named(&store, "consultas");
    assert_eq!(store.threads(forum.id, false).len(), 3);
    assert_eq!(store.threads(forum.id, true).len(), 3);

    // A private thread the account is not in is hidden, unless it moderates threads.
    let mut private = named(&store, "Jurado del concurso de octubre");
    private.thread.as_mut().unwrap().joined = false;
    store.apply(Event::ChannelChanged(private.clone()), Instant::now());
    assert_eq!(store.threads(gallery.id, false).len(), 1);
    private.permissions = private.permissions | Permissions::MANAGE_THREADS;
    store.apply(Event::ChannelChanged(private), Instant::now());
    assert_eq!(store.threads(gallery.id, false).len(), 2);

    // Without its parent in view, none of its threads is listed.
    let mut hidden = gallery.clone();
    hidden.permissions = Permissions::NONE;
    store.apply(Event::ChannelChanged(hidden), Instant::now());
    assert!(store.threads(gallery.id, false).is_empty());
}

#[test]
fn the_store_says_why_a_thread_takes_no_reply() {
    let mut store = store();
    let mut thread = named(&store, "Salida fotográfica del sábado");
    assert!(store.abilities(thread.id).unwrap().write.allowed());

    // In a thread the bit to write is its own, not the channel's.
    thread.permissions = thread.permissions.without(Permissions::SEND_IN_THREADS);
    store.apply(Event::ChannelChanged(thread.clone()), Instant::now());
    assert_eq!(
        store.abilities(thread.id).unwrap().write.reason(),
        Some("No tenés permiso para escribir en #Salida fotográfica del sábado")
    );
    thread.permissions = thread.permissions | Permissions::SEND_IN_THREADS;

    let reasons = |store: &mut Store, info: Thread, permissions: Permissions| {
        let mut changed = thread.clone();
        changed.thread = Some(info);
        changed.permissions = permissions;
        store.apply(Event::ChannelChanged(changed), Instant::now());
        let abilities = store.abilities(thread.id).unwrap();
        (
            abilities.write.reason().map(str::to_owned),
            abilities.react.allowed(),
            abilities.attach.allowed(),
            abilities.typing.reason().map(str::to_owned),
        )
    };
    let base = thread.thread.clone().unwrap();
    let archived = Thread {
        archived: true,
        ..base.clone()
    };
    // A reply reopens an archived thread, as the service does; nothing else does.
    let (write, react, attach, typing) = reasons(&mut store, archived.clone(), thread.permissions);
    assert_eq!(write, None);
    assert_eq!(
        typing.as_deref(),
        Some("Este hilo está archivado: respondé para abrirlo otra vez")
    );
    assert!(!react && !attach);
    let closed_and_locked = Thread {
        locked: true,
        ..archived
    };
    assert_eq!(
        reasons(&mut store, closed_and_locked, thread.permissions)
            .0
            .as_deref(),
        Some("Este hilo está bloqueado: solo la moderación puede responder")
    );
    let locked = Thread {
        locked: true,
        ..base.clone()
    };
    let (write, react, _, _) = reasons(&mut store, locked.clone(), thread.permissions);
    assert_eq!(
        write.as_deref(),
        Some("Este hilo está bloqueado: solo la moderación puede responder")
    );
    assert!(react, "a locked thread is still open to reactions");
    let moderator = thread.permissions | Permissions::MANAGE_THREADS;
    assert_eq!(reasons(&mut store, locked, moderator).0, None);

    // A provider that does not write in threads says so first.
    store.provider.as_mut().unwrap().capabilities.thread_replies = false;
    assert_eq!(
        reasons(&mut store, base, moderator).0.as_deref(),
        Some("Este proveedor no permite responder en hilos")
    );
    // A forum is a list of posts: nothing is written in it directly.
    let forum = named(&store, "consultas");
    assert!(!store.abilities(forum.id).unwrap().write.allowed());
}

fn connected() -> (rune_provider::mock::MockProvider, Store) {
    let provider = rune_provider::mock::MockProvider::new(Default::default());
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    store.apply(
        Event::Connection(rune_provider::ConnectionState::Connected),
        Instant::now(),
    );
    (provider, store)
}

fn deliver(store: &mut Store, events: Vec<Event>) {
    for event in events {
        store.apply(event, Instant::now());
    }
}

#[test]
fn threads_come_page_by_page_once_each_open_then_archived_then_private() {
    let (provider, mut store) = connected();
    let gallery = named(&store, "galería").id;
    let mut seen = Vec::new();
    for request in 1..20 {
        let Some(command) = store.load_threads(gallery, request) else {
            break;
        };
        // One page at a time: asking again while it loads asks for nothing.
        assert_eq!(store.load_threads(gallery, request + 100), None);
        let rune_provider::Command::LoadThreads { scope, .. } = &command else {
            panic!("{command:?}");
        };
        seen.push(*scope);
        let events = provider.manage(command.clone());
        // A late repeat of the same page changes nothing more.
        deliver(&mut store, events.clone());
        deliver(&mut store, events);
    }
    use rune_provider::ThreadScope::{Active, Archived, Joined};
    // The open ones first, then the three public archived ones (one page),
    // then the archived private ones it is in.
    assert_eq!(seen, vec![Active, Archived, Joined]);
    assert!(store.thread_pages(gallery).unwrap().done);
    assert_eq!(
        store.more_threads(gallery).reason(),
        Some("No hay más hilos archivados")
    );
    let archived: Vec<&str> = store
        .threads(gallery, true)
        .into_iter()
        .filter(|c| c.thread.as_ref().unwrap().archived)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(archived.len(), 4, "{archived:?}");
    // The private one without the account never shows.
    assert!(!archived.contains(&"Jurado de septiembre"));
    assert_eq!(store.threads(gallery, false).len(), 2);

    // A page that failed can be asked for again.
    let forum = named(&store, "consultas").id;
    let command = store.load_threads(forum, 50).unwrap();
    let failure = rune_provider::Failure::of(&command).unwrap();
    store.apply(
        Event::Failed {
            failure,
            reason: "sin red".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    assert!(store.load_threads(forum, 51).is_some());
    // A page nobody waits for is ignored.
    deliver(&mut store, provider.manage(command));
    assert_eq!(store.thread_pages(forum).unwrap().next, None);
}

#[test]
fn threads_read_by_page_outlive_a_new_session_of_the_same_account() {
    let (provider, mut store) = connected();
    let gallery = named(&store, "galería").id;
    for request in 1..3 {
        let command = store.load_threads(gallery, request).unwrap();
        deliver(&mut store, provider.manage(command));
    }
    let archived: Vec<ChannelId> = store
        .threads(gallery, true)
        .into_iter()
        .filter(|c| c.thread.as_ref().unwrap().archived)
        .map(|c| c.id)
        .collect();
    assert_eq!(archived.len(), 3);
    // A followed open thread the new session does not name has gone.
    let saturday = named(&store, "Salida fotográfica del sábado").id;
    let mut snapshot = provider.snapshot();
    snapshot.channels.retain(|c| c.id != saturday);
    // The parent allows less now: its threads follow.
    let parent = snapshot
        .channels
        .iter_mut()
        .find(|c| c.id == gallery)
        .unwrap();
    parent.permissions = parent.permissions.without(Permissions::SEND_IN_THREADS);
    store.apply(Event::Ready(Box::new(snapshot.clone())), Instant::now());
    for id in &archived {
        let kept = store.channel(*id).expect("read by page, still there");
        assert!(!kept.permissions.contains(Permissions::SEND_IN_THREADS));
    }
    assert!(store.channel(saturday).is_none());
    // Where the pages went is asked again, from the open ones.
    assert_eq!(store.thread_pages(gallery), None);
    let Some(rune_provider::Command::LoadThreads { scope, before, .. }) =
        store.load_threads(gallery, 9)
    else {
        panic!("the first page again");
    };
    assert_eq!((scope, before), (rune_provider::ThreadScope::Active, None));

    // Another account starts from what its own session says.
    let mut other = snapshot;
    if let Some(me) = other.me.as_mut() {
        me.id = rune_model::UserId(me.id.0 + 1);
    }
    store.apply(Event::Ready(Box::new(other)), Instant::now());
    assert!(archived.iter().all(|id| store.channel(*id).is_none()));
}

#[test]
fn a_new_message_counts_in_its_thread_once() {
    let (provider, mut store) = connected();
    let thread = named(&store, "Salida fotográfica del sábado");
    let before = thread.thread.clone().unwrap();
    let (mut messages, _) = provider.history(thread.id, None, 1);
    let mut message = messages.pop().unwrap();
    message.id = rune_model::MessageId(message.id.0 + 1);
    message.timestamp = rune_model::Timestamp(before.last_activity.0 + 60_000);
    for _ in 0..2 {
        store.apply(
            Event::MessageCreated {
                message: message.clone(),
                nonce: None,
            },
            Instant::now(),
        );
    }
    let after = store.channel(thread.id).unwrap().thread.clone().unwrap();
    assert_eq!(after.messages, before.messages + 1);
    assert_eq!(after.last_activity, message.timestamp);
}

#[test]
fn starting_and_changing_threads_follows_the_permissions() {
    let (_, mut store) = connected();
    let gallery = named(&store, "galería");
    assert!(store.create_thread(gallery.id, false, true).allowed());
    assert!(store.create_thread(gallery.id, true, false).allowed());
    assert_eq!(
        store.create_thread(gallery.id, true, true).reason(),
        Some("Los hilos privados se abren en un canal de texto, sin partir de un mensaje")
    );
    let mut closed = gallery.clone();
    closed.permissions = closed
        .permissions
        .without(Permissions::CREATE_PRIVATE_THREADS);
    store.apply(Event::ChannelChanged(closed), Instant::now());
    assert_eq!(
        store.create_thread(gallery.id, true, false).reason(),
        Some("No tenés permiso para crear hilos privados en #galería")
    );
    assert!(store.create_thread(gallery.id, false, false).allowed());
    let forum = named(&store, "consultas");
    assert!(store.create_post(forum.id).allowed());
    assert!(!store.create_thread(forum.id, false, false).allowed());
    let moderated = forum.tags.iter().find(|t| t.moderated).unwrap().id;
    let open = forum.tags.iter().find(|t| !t.moderated).unwrap().id;
    assert!(store.post_tags(forum.id, &[], &[open]).allowed());
    assert_eq!(
        store.post_tags(forum.id, &[], &[open, moderated]).reason(),
        Some("Esa etiqueta es solo para la moderación")
    );
    // One already on the post may stay.
    assert!(store
        .post_tags(forum.id, &[moderated], &[moderated])
        .allowed());

    // Someone else's thread in a club the account does not moderate.
    let saturday = named(&store, "Salida fotográfica del sábado");
    let can = store.thread_abilities(saturday.id).unwrap();
    assert!(can.follow.allowed());
    assert!(!can.close.allowed() && !can.lock.allowed() && !can.delete.allowed());
    assert_eq!(
        can.link.reason(),
        Some("Este proveedor no tiene enlaces a los hilos")
    );
    // Its own thread it closes and renames, but locks only as moderation.
    let mut own = saturday.clone();
    own.thread.as_mut().unwrap().owner = store.me();
    store.apply(Event::ChannelChanged(own.clone()), Instant::now());
    let can = store.thread_abilities(own.id).unwrap();
    assert!(can.close.allowed() && can.edit.allowed() && !can.lock.allowed());
    // Offline, nothing changes.
    store.apply(
        Event::Connection(rune_provider::ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 500,
        }),
        Instant::now(),
    );
    assert!(!store.thread_abilities(own.id).unwrap().close.allowed());
    assert!(!store.create_thread(gallery.id, false, false).allowed());
}
