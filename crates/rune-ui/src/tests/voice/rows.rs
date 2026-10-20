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
            if (*on, *channel, *owner) == (server, room, other)
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
