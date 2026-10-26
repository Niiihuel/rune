//! People's rows under a voice channel in the column (voz-filas-y-arrastre):
//! the pointer on someone live shows the card of their stream, with its
//! latest picture and «Ver transmisión».

use super::*;

impl Host {
    /// Rest the pointer on `id`'s left part.
    fn rows_point(&mut self, id: &str) {
        let rect = self.vz_rect(id);
        self.send(UiEvent::PointerMove(Point::new(
            rect.x + 20.0,
            rect.y + rect.height / 2.0,
        )));
        self.frame();
    }
}

#[test]
fn the_pointer_on_someone_live_shows_their_stream_s_card_and_it_watches_it() {
    let mut host = Host::new();
    let room = host.channel_named("Sala común");
    let other = host.vz_other(room);
    let mut state = host
        .store
        .voice_in(room)
        .into_iter()
        .find(|state| state.user == other)
        .cloned()
        .unwrap();
    state.streaming = true;
    host.event(Event::VoiceChanged(state));
    let watch = ids::live_watch(other.0);
    assert!(!host.vz_shows(&watch), "not until the pointer is on them");
    host.rows_point(&ids::row(other.0));
    assert_eq!(host.ui.profiles.hovered, Some(other));
    let shown = host.vz_texts();
    assert!(shown.contains(&"Ver transmisión".to_owned()), "{shown:?}");
    assert!(shown.contains(&"Vista previa simulada".to_owned()));
    // Outside the call it cannot be watched yet: shown, disabled.
    assert!(host.vz_shows(&watch) && !host.vz_usable(&watch));
    // Its picture is asked for while the card shows.
    let server = host.store.channel(room).unwrap().server.unwrap();
    let asked = crate::video_previews(&mut host.ui, &host.store, Instant::now());
    assert!(matches!(
        asked.first(),
        Some(Intent::Command(Command::LoadStreamPreview { server: on, channel, owner }))
            if (*on, *channel, *owner) == (Some(server), room, other)
    ));
    // In the call, «Ver transmisión» watches it.
    host.vz_join("Sala común");
    host.rows_point(&ids::row(other.0));
    assert!(host.vz_usable(&watch));
    let from = host.sent.len();
    host.click(&watch);
    assert!(matches!(
        host.sent[from..],
        [Command::WatchStream { owner, .. }] if owner == other
    ));
    // Watched, the card still shows it, and its picture is still asked for.
    host.ui.video.previews.clear();
    host.rows_point(&ids::row(other.0));
    assert!(host.vz_shows(&watch));
    let asked = crate::video_previews(&mut host.ui, &host.store, Instant::now());
    assert!(asked.iter().any(|intent| matches!(
        intent,
        Intent::Command(Command::LoadStreamPreview { owner, .. }) if *owner == other
    )));
    // Someone not live has no card.
    let quiet = host
        .store
        .voice_in(room)
        .into_iter()
        .map(|state| state.user)
        .find(|user| *user != other && *user != host.vz_me())
        .unwrap();
    host.rows_point(&ids::row(quiet.0));
    assert!(!host.vz_shows(&watch));
    assert!(!host.vz_shows(&ids::live_watch(quiet.0)));
}

#[test]
fn a_live_card_s_picture_is_loaded_watched_or_not_and_one_that_failed_says_so() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    let other = host.vz_other(room);
    let mut state = host
        .store
        .voice_in(room)
        .into_iter()
        .find(|state| state.user == other)
        .cloned()
        .unwrap();
    state.streaming = true;
    host.event(Event::VoiceChanged(state));
    // As a real service: nothing made up.
    host.store.provider.as_mut().unwrap().capabilities.voice = rune_provider::VoiceSupport::Real;
    let image = rune_model::RemoteImage {
        url: "https://cdn.discordapp.com/streams/guild:1:2:3/a.jpg".into(),
        kind: rune_model::ImageKind::Banner,
    };
    host.event(Event::StreamPreview {
        channel: room,
        owner: other,
        image: Some(image.clone()),
        retry_after: None,
    });
    // Watched, its card under the pointer still wants its picture.
    host.rows_point(&ids::row(other.0));
    host.click(&ids::live_watch(other.0));
    assert!(host
        .store
        .watching()
        .iter()
        .any(|watching| watching.owner == other));
    host.rows_point(&ids::row(other.0));
    let wanted = crate::wanted_images(&host.ui, &host.store, &host.tree);
    assert!(wanted.contains(&image), "{wanted:?}");
    assert!(host
        .vz_texts()
        .contains(&"Cargando la vista previa…".to_owned()));
    // It did not load: said so, not «Cargando…» for ever.
    host.ui.image_failures.insert(image);
    host.frame();
    let shown = host.vz_texts();
    assert!(
        shown.contains(&"Vista previa no disponible".to_owned()),
        "{shown:?}"
    );
    assert!(!shown.contains(&"Cargando la vista previa…".to_owned()));
}

#[test]
fn one_live_card_at_a_time_and_it_stays_while_the_pointer_is_on_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    let me = host.vz_me();
    let others: Vec<UserId> = host
        .store
        .voice_in(room)
        .iter()
        .map(|state| state.user)
        .filter(|user| *user != me)
        .collect();
    assert!(others.len() >= 2, "{others:?}");
    for user in &others[..2] {
        let mut state = host
            .store
            .voice_in(room)
            .into_iter()
            .find(|state| state.user == *user)
            .cloned()
            .unwrap();
        state.streaming = true;
        host.event(Event::VoiceChanged(state));
    }
    let (a, b) = (others[0], others[1]);
    host.rows_point(&ids::row(a.0));
    assert!(host.vz_shows(&ids::live_watch(a.0)));
    host.rows_point(&ids::row(b.0));
    assert!(host.vz_shows(&ids::live_watch(b.0)));
    assert!(
        !host.vz_shows(&ids::live_watch(a.0)),
        "A's card stayed open"
    );
    // From A's row into A's card: it stays.
    host.rows_point(&ids::row(a.0));
    let rect = host.vz_rect(&ids::live_watch(a.0));
    host.send(UiEvent::PointerMove(Point::new(
        rect.x + 4.0,
        rect.y + rect.height / 2.0,
    )));
    host.frame();
    assert!(host.vz_shows(&ids::live_watch(a.0)));
    // On A's card, above its button (the card itself, not a control): A's
    // alone, not B's too (both cards once shared the surface they hold on).
    host.send(UiEvent::PointerMove(Point::new(
        rect.x + rect.width / 2.0,
        rect.y - 40.0,
    )));
    host.frame();
    assert!(host.vz_shows(&ids::live_watch(a.0)));
    assert!(
        !host.vz_shows(&ids::live_watch(b.0)),
        "B's card opened over A's"
    );
    // Watched from A's card, then the pointer goes to B's row: A's closes.
    host.click(&ids::live_watch(a.0));
    host.rows_point(&ids::row(b.0));
    assert!(host.vz_shows(&ids::live_watch(b.0)));
    assert!(
        !host.vz_shows(&ids::live_watch(a.0)),
        "A's card stayed after watching"
    );
}

#[test]
fn a_live_row_draws_no_picture_of_its_stream_under_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    let other = host.vz_other(room);
    let mut state = host
        .store
        .voice_in(room)
        .into_iter()
        .find(|state| state.user == other)
        .cloned()
        .unwrap();
    state.streaming = true;
    host.event(Event::VoiceChanged(state));
    host.event(Event::StreamPreview {
        channel: room,
        owner: other,
        image: None,
        retry_after: None,
    });
    // Off the pointer, the list is the rows alone: no preview in it.
    assert!(!host
        .vz_texts()
        .contains(&"Vista previa simulada".to_owned()));
    assert!(!host.vz_shows(&format!("vz:preview:{}", other.0)));
}
