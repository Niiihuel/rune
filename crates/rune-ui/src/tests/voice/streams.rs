//! Streams (transmisiones): focusing one watched and coming back to the
//! grid, a stream's own menu and its volume apart from the person's, the
//! pictures of streams asked for as the official client asks them, and the
//! account's own picture going up.

use std::time::Duration;

use rune_model::StreamPhase;

use super::*;

impl Host {
    /// `count` people in `room` (the account among them), the first
    /// `streaming` of the others streaming: who streams.
    fn tr_streamers(&mut self, room: ChannelId, count: usize, streaming: usize) -> Vec<UserId> {
        let already = self.store.voice_in(room).len();
        self.vz_fill(room, count.max(already));
        let me = self.vz_me();
        let states: Vec<_> = self
            .store
            .voice_in(room)
            .into_iter()
            .filter(|state| state.user != me)
            .take(streaming)
            .cloned()
            .collect();
        let owners = states.iter().map(|state| state.user).collect();
        for mut state in states {
            state.streaming = true;
            self.event(Event::VoiceChanged(state));
        }
        owners
    }

    /// Watch `owner`'s stream in `room`, live.
    fn tr_watch(&mut self, room: ChannelId, owner: UserId) {
        self.click(&ids::watch(owner.0));
        self.event(Event::StreamPhase {
            channel: room,
            owner,
            phase: StreamPhase::Live,
        });
        // The host shows its frames.
        self.ui.video.streams.retain(|(shown, _)| *shown != owner);
        self.ui
            .video
            .streams
            .push((owner, crate::Live::Showing(1280, 720)));
    }

    /// Where `owner`'s stream is drawn.
    fn tr_surface(&mut self, owner: UserId) -> limn::Rect {
        let surface = ids::stream_surface(owner.0);
        self.frame()
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Surface { id, rect, .. } if *id == surface => Some(*rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{surface} is not drawn"))
    }
}

#[test]
fn a_stream_watched_is_focused_with_a_click_and_the_grid_comes_back() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owners = host.tr_streamers(room, 4, 2);
    let (first, second) = (owners[0], owners[1]);
    host.tr_watch(room, first);
    host.tr_watch(room, second);
    let side = host.tr_surface(first);
    assert_eq!(side.width, host.tr_surface(second).width, "side by side");

    // A click on one: it takes the stage, the other goes small under it.
    host.click(&ids::stream_tile(first.0));
    assert_eq!(host.ui.video.spotlight, Some(first));
    let (big, small) = (host.tr_surface(first), host.tr_surface(second));
    assert!(big.width > side.width && small.width < side.width);
    assert!(small.y > big.y + big.height);
    assert!(host.vz_usable(&ids::spotlight(first.0)), "the way back");
    assert!(!host.vz_shows(&ids::stop_watching(second.0)));
    // A click on the one in the strip focuses it instead.
    host.click(&ids::stream_tile(second.0));
    assert_eq!(host.ui.video.spotlight, Some(second));
    // «Volver a la grilla», a click on it again, or Escape: the grid.
    host.click(&ids::spotlight(second.0));
    assert_eq!(host.ui.video.spotlight, None);
    host.click(&ids::stream_tile(second.0));
    host.click(&ids::stream_tile(second.0));
    assert_eq!(host.ui.video.spotlight, None);
    host.click(&ids::stream_tile(first.0));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.video.spotlight, None);
    assert_eq!(host.tr_surface(first).width, side.width);
    // Letting go of the one focused brings the grid back too.
    host.click(&ids::stream_tile(first.0));
    host.click(&ids::stop_watching(first.0));
    assert_eq!(host.ui.video.spotlight, None);
}

#[test]
fn a_streams_menu_has_its_own_volume_and_its_ways_to_show_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owner = host.tr_streamers(room, 3, 1)[0];
    host.tr_watch(room, owner);
    let sound = owner.0 | rune_media::mixer::STREAM_SOUND;

    // A secondary click on the stream opens its own menu, not the person's.
    host.vz_right_click(&ids::stream_tile(owner.0));
    assert_eq!(host.ui.video.menu.map(|(shown, _)| shown), Some(owner));
    assert!(host.ui.voice.menu.is_none());
    for id in [
        ids::volume(sound),
        ids::local_mute(sound),
        ids::spotlight(owner.0),
        ids::fullscreen(owner.0),
        ids::window(owner.0),
        ids::stop_watching(owner.0),
    ] {
        assert!(host.vz_shows(&id), "{id}");
    }
    assert!(
        !host.vz_shows(&ids::volume(owner.0)),
        "the person's is theirs"
    );
    assert!(host
        .vz_texts()
        .contains(&"Simulado: la llamada no lleva audio".to_owned()));
    // Its volume and silence are the stream's sound, apart from the voice's.
    let intents = crate::update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::SliderChanged {
            id: ids::volume(sound),
            fraction: 0.25,
            committed: true,
        },
        Instant::now(),
    );
    assert_eq!(intents.len(), 1, "kept");
    assert_eq!(host.ui.prefs.voice_volume(sound), (0.5, false));
    host.click(&ids::local_mute(sound));
    assert_eq!(host.ui.prefs.voice_volume(sound), (0.5, true));
    assert_eq!(host.ui.prefs.voice_volume(owner.0), (1.0, false));
    assert!(host.ui.video.menu.is_none(), "an item closes it");
    // Focusing it, full screen and a window of its own, from there too.
    host.vz_right_click(&ids::stream_tile(owner.0));
    host.click(&ids::spotlight(owner.0));
    assert_eq!(host.ui.video.spotlight, Some(owner));
    host.vz_right_click(&ids::stream_tile(owner.0));
    assert!(host.vz_texts().contains(&"Volver a la grilla".to_owned()));
    host.click(&ids::window(owner.0));
    assert_eq!(host.ui.video.windows, [owner]);
    host.vz_right_click(&ids::stream_tile(owner.0));
    assert!(!host.vz_shows(&ids::window(owner.0)), "already in one");
    host.click(&ids::fullscreen(owner.0));
    assert_eq!(host.ui.video.focus, Some(owner));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.video.focus, None);
    // «Dejar de ver», and the menu goes with the stream.
    host.vz_right_click(&ids::stream_tile(owner.0));
    let from = host.sent.len();
    host.click(&ids::stop_watching(owner.0));
    assert!(matches!(
        host.sent[from..],
        [Command::StopWatching { owner: stopped, .. }] if stopped == owner
    ));
    assert!(host.ui.video.menu.is_none() && host.store.watching().is_empty());
}

#[test]
fn a_streams_picture_is_asked_for_as_the_official_client_does_and_shows_on_its_tile() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owner = host.tr_streamers(room, 2, 1)[0];
    let server = host.store.channel(room).unwrap().server.unwrap();
    let start = Instant::now();
    let at = |seconds: u64| start + Duration::from_secs(seconds);
    let asked = |host: &mut Host, when| {
        crate::video_previews(&mut host.ui, &host.store, when)
            .iter()
            .any(|intent| {
                matches!(
                    intent,
                    Intent::Command(Command::LoadStreamPreview { server: on, channel, owner: of })
                        if (*on, *channel, *of) == (server, room, owner)
                )
            })
    };
    let answer = |host: &mut Host, image, retry_after| {
        host.event(Event::StreamPreview {
            channel: room,
            owner,
            image,
            retry_after,
        })
    };
    assert!(asked(&mut host, at(0)));
    // Unanswered: again after ten seconds, as a failure.
    assert!(!asked(&mut host, at(9)));
    assert!(asked(&mut host, at(10)));
    // A picture: good for two minutes.
    let image = rune_model::RemoteImage {
        url: "https://cdn.discordapp.com/streams/1.png".into(),
        kind: rune_model::ImageKind::Banner,
    };
    answer(&mut host, Some(image), None);
    assert!(!asked(&mut host, at(129)));
    assert!(asked(&mut host, at(130)));
    // A 429: as long as the service said.
    answer(&mut host, None, Some(Duration::from_secs(45)));
    assert!(!asked(&mut host, at(174)));
    assert!(asked(&mut host, at(175)));
    // None, again and again: ten seconds more each time.
    answer(&mut host, None, None);
    assert!(!asked(&mut host, at(194)), "the second in a row: 20 s");
    assert!(asked(&mut host, at(195)));
    answer(&mut host, None, None);
    assert!(!asked(&mut host, at(224)), "the third: 30 s");
    assert!(asked(&mut host, at(225)));

    // The simulation's has none: its tile on the stage says so, under
    // «EN VIVO» and over the way to watch it.
    let texts = host.vz_texts();
    for expected in ["EN VIVO", "Vista previa simulada", "Ver transmisión"] {
        assert!(
            texts.contains(&expected.to_owned()),
            "{expected} in {texts:?}"
        );
    }
    assert!(host.vz_usable(&ids::watch(owner.0)));
}

#[test]
fn the_accounts_own_streams_picture_goes_up_as_the_official_client_sends_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    let me = host.vz_me();
    let server = host.store.channel(room).unwrap().server;
    let start = Instant::now();
    let at = |millis: u64| start + Duration::from_millis(millis);
    // Nothing while it does not share.
    assert_eq!(
        crate::video_own_preview(&mut host.ui, &host.store, at(0)),
        None
    );
    host.store.begin_share(room);
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });
    // Nor before the screen shows.
    assert_eq!(
        crate::video_own_preview(&mut host.ui, &host.store, at(0)),
        None
    );
    host.ui.video.screen.on = true;
    host.ui.video.screen.live = crate::Live::Showing(1920, 1080);
    let due =
        |host: &mut Host, millis| crate::video_own_preview(&mut host.ui, &host.store, at(millis));
    // Half a second after it shows, then every five minutes.
    assert_eq!(due(&mut host, 0), None);
    assert_eq!(due(&mut host, 499), None);
    assert_eq!(due(&mut host, 500), Some((server, room, me)));
    assert_eq!(due(&mut host, 1_000), None);
    assert_eq!(due(&mut host, 300_499), None);
    assert_eq!(due(&mut host, 300_500), Some((server, room, me)));
    // A picture that could not be taken: a minute later.
    crate::video_own_preview_failed(&mut host.ui, at(301_000));
    assert_eq!(due(&mut host, 360_999), None);
    assert_eq!(due(&mut host, 361_000), Some((server, room, me)));
    // Paused: none, and it looks again each minute.
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Paused,
    });
    assert_eq!(due(&mut host, 661_000), None);
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });
    assert_eq!(due(&mut host, 720_999), None);
    assert_eq!(due(&mut host, 721_000), Some((server, room, me)));
    // Done sharing: it starts over with the next stream.
    host.store.stop_share();
    assert_eq!(due(&mut host, 721_500), None);
    assert_eq!(host.ui.video.own_preview, None);
}
