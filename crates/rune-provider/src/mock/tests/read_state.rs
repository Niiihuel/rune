use super::*;

fn marker(request: RequestId, channel: ChannelId, message: MessageId) -> Command {
    Command::MarkRead {
        request,
        account: ME,
        channel,
        message,
    }
}

fn cursor(snapshot: &Snapshot, channel: ChannelId) -> Option<MessageId> {
    snapshot
        .read_states
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1
}

fn version(snapshot: &Snapshot, channel: ChannelId) -> u64 {
    snapshot
        .read_versions
        .iter()
        .find(|state| state.0 == channel)
        .map_or(0, |state| state.1)
}

async fn outcome(session: &mut Session) -> Event {
    tokio::time::timeout(Duration::from_secs(1), session.next())
        .await
        .expect("a read marker needs a correlated outcome")
}

fn confirmed(event: Event, request: u64, channel: ChannelId) -> (MessageId, u64) {
    let Event::ReadStateConfirmed {
        account,
        request: actual,
        channel: found,
        message,
        version,
    } = event
    else {
        panic!("read acknowledgement expected: {event:?}")
    };
    assert_eq!((account, actual, found), (ME, Some(request), channel));
    assert!(version > 0);
    (message, version)
}

#[test]
fn snapshot_read_versions_are_positive_sorted_and_match_legacy_read_cursors() {
    let provider = MockProvider::new(config());
    let snapshot = provider.snapshot();
    assert!(!snapshot.read_states.is_empty());
    assert_eq!(snapshot.read_versions.len(), snapshot.read_states.len());
    assert!(snapshot
        .read_versions
        .windows(2)
        .all(|pair| pair[0].0 < pair[1].0));
    for (channel, value) in &snapshot.read_versions {
        assert!(*value > 0);
        assert!(snapshot.read_states.iter().any(|state| state.0 == *channel));
    }
    assert_eq!(
        snapshot,
        provider.snapshot(),
        "reading a snapshot changes no version"
    );
}

#[tokio::test(start_paused = true)]
async fn simulated_direct_and_group_conversations_support_history_context_and_read_acknowledgements(
) {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let directs: Vec<_> = session
        .snapshot
        .channels
        .iter()
        .filter(|channel| matches!(channel.kind, ChannelKind::Direct | ChannelKind::Group))
        .cloned()
        .collect();
    assert!(directs
        .iter()
        .any(|channel| channel.kind == ChannelKind::Direct));
    assert!(directs
        .iter()
        .any(|channel| channel.kind == ChannelKind::Group));
    for (index, channel) in directs.into_iter().enumerate() {
        assert!(
            channel
                .permissions
                .contains(Permissions::MEMBER | Permissions::READ_HISTORY),
            "simulated {:?} must declare its readable history",
            channel.kind
        );
        let target = session
            .snapshot
            .last_messages
            .iter()
            .find(|state| state.0 == channel.id)
            .unwrap()
            .1;
        let request = 600 + index as u64 * 2;
        session
            .send(Command::LoadContext {
                request,
                channel: channel.id,
                anchor: target,
                limit: 3,
            })
            .await;
        let Event::ContextLoaded {
            request: found,
            channel: actual,
            anchor,
            messages,
        } = outcome(&mut session).await
        else {
            panic!("a simulated direct conversation must support its declared message context")
        };
        assert_eq!((found, actual, anchor), (request, channel.id, target));
        assert!(!messages.is_empty() && messages.len() <= 3);
        assert!(messages.iter().all(|message| message.channel == channel.id));
        assert!(messages.iter().any(|message| message.id == target));

        session.send(marker(request + 1, channel.id, target)).await;
        let (actual, accepted_version) =
            confirmed(outcome(&mut session).await, request + 1, channel.id);
        assert_eq!(actual, target);
        let snapshot = world.lock().unwrap().snapshot();
        assert_eq!(cursor(&snapshot, channel.id), Some(target));
        assert_eq!(version(&snapshot, channel.id), accepted_version);
    }
}

#[test]
fn snapshot_first_unread_anchors_exist_and_skip_deleted_messages() {
    let mut world = World::build(&config());
    let channel = general(&world);
    let snapshot = world.snapshot();
    assert!(!snapshot.first_unread.is_empty());
    assert!(snapshot
        .first_unread
        .windows(2)
        .all(|pair| pair[0].0 < pair[1].0));
    for (channel, anchor) in &snapshot.first_unread {
        assert!(world.message(*channel, *anchor).is_some());
        assert!(cursor(&snapshot, *channel).is_none_or(|read| *anchor > read));
    }
    let first = snapshot
        .first_unread
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    world
        .sims
        .get_mut(&channel)
        .unwrap()
        .overlay
        .insert(first, None);
    let snapshot = world.snapshot();
    let next = snapshot
        .first_unread
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    assert!(next > first);
    assert!(world.message(channel, next).is_some());

    world.read.remove(&channel);
    let first_message = message_id(channel, 0);
    world
        .sims
        .get_mut(&channel)
        .unwrap()
        .overlay
        .insert(first_message, None);
    let snapshot = world.snapshot();
    let first = snapshot
        .first_unread
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    assert_eq!(first, message_id(channel, 1));
    assert!(world.message(channel, first).is_some());
}

#[test]
fn a_read_boundary_accepts_the_last_generated_message_after_its_deletion() {
    let mut world = World::build(&config());
    let channel = general(&world);
    let target = world.history(channel, None, 1).0[0].id;
    let baseline = version(&world.snapshot(), channel);
    world
        .sims
        .get_mut(&channel)
        .unwrap()
        .overlay
        .insert(target, None);
    assert!(world.message(channel, target).is_none());

    let (actual, accepted_version) =
        confirmed(world.confirm_read(550, ME, channel, target), 550, channel);
    assert_eq!(actual, target);
    assert!(accepted_version > baseline);
    assert_eq!(cursor(&world.snapshot(), channel), Some(target));
    assert!(
        world.message(channel, target).is_none(),
        "a through boundary never restores its message"
    );
}

#[test]
fn a_read_boundary_accepts_a_posted_message_after_its_deletion() {
    let mut world = World::build(&config());
    let channel = general(&world);
    let target = world
        .post(channel, ME, "mensaje eliminado".into(), None, Timestamp(42))
        .unwrap()
        .id;
    world.delete(channel, target).unwrap();
    assert!(world.message(channel, target).is_none());

    let (actual, accepted_version) =
        confirmed(world.confirm_read(551, ME, channel, target), 551, channel);
    assert_eq!(actual, target);
    let (again, idempotent_version) =
        confirmed(world.confirm_read(552, ME, channel, target), 552, channel);
    assert_eq!((again, idempotent_version), (target, accepted_version));
    assert!(world.message(channel, target).is_none());
}

#[test]
fn read_boundaries_reject_zero_future_and_other_channel_message_ids() {
    let mut world = World::build(&config());
    let channel = general(&world);
    let other = *world
        .sims
        .keys()
        .find(|candidate| **candidate != channel)
        .unwrap();
    let foreign = world.history(other, None, 1).0[0].id;
    let next = message_id(channel, world.sims[&channel].generated);
    let baseline = world.snapshot();
    for (index, target) in [MessageId(0), foreign, next, MessageId(u64::MAX)]
        .into_iter()
        .enumerate()
    {
        let request = 560 + index as u64;
        assert!(
            matches!(world.confirm_read(request, ME, channel, target), Event::Failed {
            failure: Failure::ReadState { request: found, channel: actual }, ambiguous: false, ..
        } if found == request && actual == channel)
        );
        let snapshot = world.snapshot();
        assert_eq!(cursor(&snapshot, channel), cursor(&baseline, channel));
        assert_eq!(version(&snapshot, channel), version(&baseline, channel));
    }
}

#[tokio::test(start_paused = true)]
async fn a_read_marker_acknowledges_its_exact_scope_and_survives_reconnection() {
    let provider = MockProvider::new(config());
    let (world, hosted) = (provider.world.clone(), provider.hosted.clone());
    let control = provider.control();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let target = session
        .snapshot
        .last_messages
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    let baseline_version = version(&session.snapshot, channel);
    session.send(marker(501, channel, target)).await;
    let (message, accepted_version) = confirmed(outcome(&mut session).await, 501, channel);
    assert_eq!(message, target);
    assert!(accepted_version > baseline_version);
    control.drop_connection();
    assert!(matches!(
        session.task.await.unwrap(),
        Err(ProviderError::Transient(_))
    ));
    let restarted = MockProvider {
        config: config(),
        hosted,
        world,
        control: Arc::default(),
    };
    let session = Session::start(restarted).await;
    assert_eq!(cursor(&session.snapshot, channel), Some(target));
    assert_eq!(version(&session.snapshot, channel), accepted_version);
}

#[tokio::test(start_paused = true)]
async fn stale_or_duplicate_read_markers_confirm_the_current_cursor_without_rewinding() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let channel = general(&world.lock().unwrap());
    let history = provider.history(channel, None, 2).0;
    let older = history[0].id;
    let latest = history[1].id;
    let mut session = Session::start(provider).await;
    session.send(marker(510, channel, latest)).await;
    let (_, first_version) = confirmed(outcome(&mut session).await, 510, channel);
    for (request, target) in [(511, older), (512, latest)] {
        session.send(marker(request, channel, target)).await;
        let (actual, accepted_version) = confirmed(outcome(&mut session).await, request, channel);
        assert_eq!(actual, latest);
        assert_eq!(
            accepted_version, first_version,
            "an idempotent confirmation retains its revision"
        );
    }
    let snapshot = world.lock().unwrap().snapshot();
    assert_eq!(cursor(&snapshot, channel), Some(latest));
    assert_eq!(version(&snapshot, channel), first_version);
}

#[tokio::test(start_paused = true)]
async fn invalid_read_scopes_fail_without_mutating_the_account_marker() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let target = session
        .snapshot
        .last_messages
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    let baseline = cursor(&session.snapshot, channel);
    let baseline_version = version(&session.snapshot, channel);
    let mut wrong_account = marker(520, channel, target);
    if let Command::MarkRead { account, .. } = &mut wrong_account {
        *account = UserId(99);
    }
    for command in [
        wrong_account,
        marker(521, ChannelId(0), target),
        marker(522, ChannelId(u64::MAX), target),
        marker(523, channel, MessageId(0)),
        marker(524, channel, MessageId(u64::MAX)),
        marker(0, channel, target),
    ] {
        let Command::MarkRead {
            request,
            channel: expected,
            ..
        } = command
        else {
            unreachable!()
        };
        session.send(command).await;
        assert!(matches!(outcome(&mut session).await, Event::Failed {
            failure: Failure::ReadState { request: actual, channel: found }, ambiguous: false, ..
        } if actual == request && found == expected));
        let snapshot = world.lock().unwrap().snapshot();
        assert_eq!(cursor(&snapshot, channel), baseline);
        assert_eq!(version(&snapshot, channel), baseline_version);
    }
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_delayed_read_marker_prevents_the_mutation_and_callback() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let target = session
        .snapshot
        .last_messages
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    let baseline = cursor(&session.snapshot, channel);
    let baseline_version = version(&session.snapshot, channel);
    session.send(marker(530, channel, target)).await;
    session.send(Command::Cancel(530)).await;
    session
        .send(Command::LoadHistory {
            request: 531,
            channel,
            before: None,
            limit: 1,
        })
        .await;
    assert!(matches!(
        outcome(&mut session).await,
        Event::HistoryLoaded { request: 531, .. }
    ));
    assert!(
        session.events.try_recv().is_err(),
        "the canceled marker publishes no outcome"
    );
    let snapshot = world.lock().unwrap().snapshot();
    assert_eq!(cursor(&snapshot, channel), baseline);
    assert_eq!(version(&snapshot, channel), baseline_version);
}

#[tokio::test(start_paused = true)]
async fn a_read_marker_uses_current_permissions_at_its_delayed_commit() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let target = session
        .snapshot
        .last_messages
        .iter()
        .find(|state| state.0 == channel)
        .unwrap()
        .1;
    let baseline = cursor(&session.snapshot, channel);
    session.send(marker(540, channel, target)).await;
    world
        .lock()
        .unwrap()
        .channels
        .iter_mut()
        .find(|candidate| candidate.id == channel)
        .unwrap()
        .permissions = Permissions::VIEW | Permissions::SEND;
    assert!(matches!(outcome(&mut session).await, Event::Failed {
        failure: Failure::ReadState { request: 540, channel: found }, ambiguous: false, ..
    } if found == channel));
    assert_eq!(cursor(&world.lock().unwrap().snapshot(), channel), baseline);
}
