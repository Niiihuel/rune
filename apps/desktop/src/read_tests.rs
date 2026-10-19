//! The real simulated service, runtime coordinator and domain state without a window.

use super::*;

fn drain_until(ctx: &egui::Context, app: &mut App, ready: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.drain(ctx, Instant::now());
        if ready(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the real mock read protocol did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_real_mock_ack_crosses_client_and_host_without_erasing_a_newer_local_reminder() {
    let ctx = egui::Context::default();
    let provider = Arc::new(MockProvider::new(MockConfig {
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
        ..MockConfig::default()
    }));
    let mut app = App::new(
        &ctx,
        Instant::now(),
        Arguments {
            screenshot: Some("unused-headless-read-protocol.png".into()),
            ..Arguments::default()
        },
        provider.clone(),
        None,
    );
    // Let EventSink and Client deliver the real Ready; injecting it into Store
    // would leave the coordinator unaware of the account under test.
    drain_until(&ctx, &mut app, |app| app.store.me().is_some());
    let channel = provider
        .snapshot()
        .channels
        .into_iter()
        .find(|channel| channel.name == "general")
        .unwrap()
        .id;
    let before = app.store.read(channel);
    let target = before.last_message.unwrap();
    let command = app
        .store
        .mark_read_command(channel)
        .expect("the simulated service supports correlated read acknowledgements");
    assert!(app.store.read(channel).sync_pending());
    app.run(&ctx, vec![Intent::Command(command)]);

    // The network outcome may already be queued, but this newer local choice
    // happens before the host applies it. Confirming progress cannot erase it.
    app.store.mark_unread(channel, target);
    drain_until(&ctx, &mut app, |app| {
        let read = app.store.read(channel);
        read.confirmed_read == Some(target) && read.confirmed_version > before.confirmed_version
    });
    let read = app.store.read(channel);
    assert!(!read.sync_pending() && !read.sync_failed());
    assert_eq!(read.last_read, Some(target));
    assert_eq!(read.unread_from, Some(target));
    assert!(read.unread());
    let remote = provider.snapshot();
    assert_eq!(
        remote
            .read_states
            .iter()
            .find(|state| state.0 == channel)
            .unwrap()
            .1,
        Some(target)
    );
    assert_eq!(
        remote
            .read_versions
            .iter()
            .find(|state| state.0 == channel)
            .unwrap()
            .1,
        read.confirmed_version
    );
    // The reminder reaches the simulated service too (P2) and comes back
    // versioned, moving its cursor before the message.
    let command = app
        .store
        .mark_unread_command(channel, target)
        .expect("the simulation takes reminders");
    app.run(&ctx, vec![Intent::Command(command)]);
    drain_until(&ctx, &mut app, |app| {
        app.store.read(channel).confirmed_read < Some(target)
    });
    let read = app.store.read(channel);
    assert_eq!(read.unread_from, Some(target));
    assert!(read.confirmed_version > before.confirmed_version);
}
