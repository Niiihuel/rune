use super::*;
use rune_provider::Provider;

// The index behind `channel_tree`, `threads` and `server_unread` must give the
// same ids in the same order as a full scan of every channel, after every kind
// of change that touches channels. The scans below are the reference.

fn store() -> (Store, rune_provider::mock::MockProvider) {
    let provider = rune_provider::mock::MockProvider::new(Default::default());
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    (store, provider)
}

fn reference_tree(store: &Store, server: ServerId) -> Vec<(Option<ChannelId>, Vec<ChannelId>)> {
    let mut of_server: Vec<&Channel> = store
        .channels
        .values()
        .filter(|channel| {
            channel.server == Some(server) && !channel.is_thread() && Store::visible(channel)
        })
        .collect();
    of_server.sort_by_key(|channel| (channel.position, channel.id));
    let mut tree: Vec<(Option<&Channel>, Vec<&Channel>)> = vec![(None, Vec::new())];
    for category in of_server
        .iter()
        .filter(|channel| channel.kind == ChannelKind::Category)
    {
        tree.push((Some(category), Vec::new()));
    }
    for channel in of_server
        .iter()
        .filter(|channel| channel.kind != ChannelKind::Category)
    {
        let slot = tree
            .iter()
            .position(|(category, _)| category.map(|category| category.id) == channel.parent)
            .unwrap_or(0);
        tree[slot].1.push(channel);
    }
    tree.retain(|(category, channels)| category.is_some() || !channels.is_empty());
    tree.into_iter()
        .map(|(category, channels)| {
            (
                category.map(|category| category.id),
                channels.into_iter().map(|channel| channel.id).collect(),
            )
        })
        .collect()
}

fn reference_threads(store: &Store, parent: ChannelId, archived: bool) -> Vec<ChannelId> {
    if !store.channel(parent).is_some_and(Store::visible) {
        return Vec::new();
    }
    let mut threads: Vec<&Channel> = store
        .channels
        .values()
        .filter(|channel| {
            channel.is_thread()
                && channel.parent == Some(parent)
                && Store::visible(channel)
                && (archived || !channel.thread.as_ref().is_some_and(|t| t.archived))
        })
        .collect();
    threads.sort_by_key(|channel| {
        let last = channel.thread.as_ref().map(|t| t.last_activity);
        (std::cmp::Reverse(last), std::cmp::Reverse(channel.id))
    });
    threads.into_iter().map(|channel| channel.id).collect()
}

fn reference_unread(store: &Store, server: ServerId) -> (bool, u32) {
    store
        .channels
        .values()
        .filter(|channel| {
            channel.server == Some(server) && channel.kind.has_messages() && Store::visible(channel)
        })
        .map(|channel| store.read(channel.id))
        .fold((false, 0), |(unread, mentions), read| {
            (
                unread || read.unread(),
                mentions.saturating_add(read.mentions),
            )
        })
}

/// Every server, a server that does not exist, and every channel as a parent.
fn assert_matches_scans(store: &Store) {
    let mut servers: Vec<ServerId> = store.servers().iter().map(|server| server.id).collect();
    servers.push(ServerId(9_999));
    for server in servers {
        let tree: Vec<(Option<ChannelId>, Vec<ChannelId>)> = store
            .channel_tree(server)
            .into_iter()
            .map(|(category, channels)| {
                (
                    category.map(|category| category.id),
                    channels.into_iter().map(|channel| channel.id).collect(),
                )
            })
            .collect();
        assert_eq!(tree, reference_tree(store, server), "tree of {server:?}");
        assert_eq!(
            store.server_unread(server),
            reference_unread(store, server),
            "unread of {server:?}"
        );
    }
    let mut ids: Vec<ChannelId> = store.channels.keys().copied().collect();
    ids.sort();
    for parent in ids {
        for archived in [false, true] {
            let threads: Vec<ChannelId> = store
                .threads(parent, archived)
                .into_iter()
                .map(|channel| channel.id)
                .collect();
            assert_eq!(
                threads,
                reference_threads(store, parent, archived),
                "threads of {parent:?} archived {archived}"
            );
        }
    }
}

fn first_where(store: &Store, pred: impl Fn(&Channel) -> bool) -> Channel {
    let mut found: Vec<&Channel> = store.channels.values().filter(|c| pred(c)).collect();
    found.sort_by_key(|channel| channel.id);
    found.first().cloned().cloned().unwrap()
}

#[test]
fn the_channel_index_matches_a_full_scan_through_every_kind_of_change() {
    let (mut store, provider) = store();
    assert!(store.servers().len() >= 2, "the mock has several servers");
    assert!(store.channels.values().any(|c| c.is_thread()));
    assert!(store
        .channels
        .values()
        .any(|c| c.kind == ChannelKind::Category));
    let now = Instant::now();
    assert_matches_scans(&store);

    // A channel moves to the top of its server.
    let mut moved = first_where(&store, |c| c.server.is_some() && !c.is_thread());
    moved.position = -1;
    store.apply(Event::ChannelChanged(moved.clone()), now);
    assert_matches_scans(&store);

    // A thread gets a new last activity, so its place in `threads` changes.
    let thread = first_where(&store, |c| c.is_thread() && c.parent.is_some());
    let mut newer = thread.clone();
    newer.thread.as_mut().unwrap().last_activity = rune_model::Timestamp(i64::MAX / 2);
    store.apply(Event::ChannelChanged(newer), now);
    assert_matches_scans(&store);

    // A new message in a thread changes its count and activity in place.
    let mut message = super::message(1, ME);
    message.channel = thread.id;
    message.timestamp = rune_model::Timestamp(i64::MAX / 4);
    store.thread_message(&message, true);
    assert_matches_scans(&store);

    // A thread is archived: it leaves `threads(parent, false)` only.
    let mut archived = first_where(&store, |c| c.is_thread() && c.parent.is_some());
    archived.thread.as_mut().unwrap().archived = true;
    store.apply(Event::ChannelChanged(archived), now);
    assert_matches_scans(&store);

    // A thread is created under a channel.
    let parent = first_where(&store, |c| c.kind.has_messages() && !c.is_thread());
    let mut created = thread.clone();
    created.id = ChannelId(9_001);
    created.parent = Some(parent.id);
    created.server = parent.server;
    created.thread.as_mut().unwrap().last_activity = rune_model::Timestamp(7);
    store.apply(Event::ChannelChanged(created), now);
    assert_matches_scans(&store);

    // A channel is removed, then a whole server goes with its channels.
    store.apply(
        Event::ChannelRemoved {
            channel: ChannelId(9_001),
        },
        now,
    );
    assert_matches_scans(&store);
    let server = store.servers()[0].id;
    store.apply(
        Event::ServerRemoved {
            server,
            reason: None,
        },
        now,
    );
    assert_matches_scans(&store);

    // The same account reads again: paged threads are kept or dropped as before.
    store.apply(Event::Ready(Box::new(provider.snapshot())), now);
    assert_matches_scans(&store);

    // Another account: everything it had is replaced.
    let mut other = provider.snapshot();
    other.me = other.me.map(|mut me| {
        me.id = UserId(9_000);
        me
    });
    other.channels.truncate(3);
    store.apply(Event::Ready(Box::new(other)), now);
    assert_matches_scans(&store);

    // Sign-out empties the store.
    store.apply(Event::Authentication(AuthenticationState::SignedOut), now);
    assert_matches_scans(&store);
    assert!(store.channels.is_empty());
}
