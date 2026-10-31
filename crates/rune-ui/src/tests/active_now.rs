//! The «Activo ahora» column of the friends page, over the simulation: which
//! cards it builds and in what order, what a card's menu offers, and the clicks
//! that answer it.

use super::*;
use crate::active_now::cards;
use rune_components::active_now::{ids, ActiveCard, ActivityView, HeaderMark};
use rune_components::friends::ids as friends_ids;
use rune_components::voice::MenuEntry;
use rune_model::UserId;

const ADA: UserId = UserId(10);
const BRUNO: UserId = UserId(11);
const CAMILA: UserId = UserId(12);
const ELENA: UserId = UserId(14);
const HUGO: UserId = UserId(17);
const LUCAS: UserId = UserId(21);
/// The window's clock, as the host's tests read it. The simulation dates its
/// activities from the end of its history, 100 s before this.
const NOW: Timestamp = Timestamp(1_790_000_100_000);

impl Host {
    /// The friends page, its online tab, as a person opens it.
    fn open_friends_page(&mut self) {
        self.click("home");
        self.click(friends_ids::HOME);
        self.click(&format!("{}online", friends_ids::TAB));
    }

    /// The column's cards now, as the window builds them.
    fn active_cards(&self) -> Vec<ActiveCard> {
        cards(&self.ui, &self.store, NOW)
    }

    /// Carry out the social command `command` as the simulation answers it.
    fn social(&mut self, command: Command) {
        for event in self.provider.manage(command) {
            self.event(event);
        }
    }

    /// The center of the control `id` on screen, scrolled into view first.
    fn center_of(&mut self, id: &str) -> Point {
        if !self.shows(id) {
            self.tree.reveal(id);
        }
        let frame = self.frame();
        let region = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        Point::new(
            region.rect.x + region.rect.width / 2.0,
            region.rect.y + region.rect.height / 2.0,
        )
    }

    fn shows(&mut self, id: &str) -> bool {
        self.frame()
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Button)
    }
}

/// Whole minutes from when `user`'s first activity started, as the column
/// rounds them down.
fn minutes_since(host: &Host, user: UserId) -> i64 {
    let started = host.store.presence(user).activities[0]
        .started
        .expect("the activity has a start");
    (NOW.0 - started.0) / 60_000
}

/// The card whose title is `title`.
fn titled(found: &[ActiveCard], title: &str) -> ActiveCard {
    found
        .iter()
        .find(|card| card.title == title)
        .cloned()
        .unwrap_or_else(|| panic!("no card titled {title:?} in {found:#?}"))
}

#[test]
fn the_column_shows_the_groups_of_friends_with_something_to_show_in_order() {
    let mut host = Host::new();
    host.open_friends_page();
    let found = host.active_cards();
    let titles: Vec<&str> = found.iter().map(|card| card.title.as_str()).collect();
    // Elena streams in her voice channel, so her card leads; Ada's group is
    // several people; Hugo's and Lucas's are one person each, Spotify last.
    assert_eq!(titles.len(), 4, "{titles:?}");
    assert!(titles[0].starts_with("Elena, "), "{titles:?}");
    assert_eq!(titles[1], "Ada, Bruno y 1 otro");
    assert_eq!(titles[2], "Hugo");
    assert_eq!(titles[3], "Lucas");
    let painted = host.painted();
    assert!(painted.contains(&"Activo ahora".to_owned()), "{painted:?}");
    assert!(
        painted.contains(&"Ada, Bruno y 1 otro".to_owned()),
        "{painted:?}"
    );
}

#[test]
fn ada_and_bruno_share_their_voice_channel_and_bruno_s_game_shows_its_time() {
    let mut host = Host::new();
    host.open_friends_page();
    let card = titled(&host.active_cards(), "Ada, Bruno y 1 otro");
    assert_eq!(card.key, format!("channel:{}", card.voice[0].channel.0));
    assert!(card.members.iter().any(|person| person.user == ADA));
    assert!(card.members.iter().any(|person| person.user == CAMILA));
    let voice = &card.voice[0];
    assert_eq!(voice.channel_name, "Sala común");
    assert_eq!(voice.members.len(), 3);
    let minutes = minutes_since(&host, BRUNO);
    assert_eq!(card.subtitle, Some(format!("Celeste – {minutes} min")));
    assert!(
        matches!(card.mark, HeaderMark::None),
        "no picture is loaded here"
    );
    assert!(matches!(
        card.activities.as_slice(),
        [ActivityView::Rich { title, .. }] if title.as_str() == "Capítulo 7"
    ));
}

#[test]
fn hugo_s_party_card_shows_his_game_and_its_elapsed_time() {
    let mut host = Host::new();
    host.open_friends_page();
    let card = titled(&host.active_cards(), "Hugo");
    assert_eq!(card.members.len(), 1);
    let minutes = minutes_since(&host, HUGO);
    assert_eq!(card.subtitle, Some(format!("Factorio – {minutes} min")));
    assert!(card.voice.is_empty());
    match card.activities.as_slice() {
        [ActivityView::Rich {
            title,
            state,
            timer,
            ..
        }] => {
            assert_eq!(title, "Construyendo una fábrica de circuitos");
            assert_eq!(state.as_deref(), Some("Partida multijugador"));
            // 47 minutes of history and the 100 s the window's clock runs ahead.
            assert_eq!(timer.as_deref(), Some("Transcurrido: 48:40"));
        }
        other => panic!("{other:#?}"),
    }
}

#[test]
fn lucas_s_spotify_card_is_last_and_names_the_track_and_its_artists() {
    let mut host = Host::new();
    host.open_friends_page();
    let card = titled(&host.active_cards(), "Lucas");
    assert_eq!(card.members[0].user, LUCAS);
    assert_eq!(card.members.len(), 1, "a stage is not a voice channel here");
    assert_eq!(card.subtitle.as_deref(), Some("Escuchando Spotify"));
    match card.activities.as_slice() {
        [ActivityView::Spotify {
            track,
            artists,
            listeners,
            ..
        }] => {
            assert_eq!(track, "Jazz para programar");
            assert_eq!(artists.as_deref(), Some("Trío Ferroso, Ana Ruiz"));
            assert_eq!(listeners.len(), 1);
        }
        other => panic!("{other:#?}"),
    }
}

#[test]
fn elena_streams_in_her_card_and_her_stream_has_no_game_to_name() {
    let mut host = Host::new();
    host.open_friends_page();
    // Elena's card leads the column: her voice channel is the one she streams in.
    let card = host.active_cards().remove(0);
    // Facundo and the others in her channel are not friends: they join the card,
    // and the title counts them.
    assert_eq!(card.title, "Elena, Facundo y 3 otros");
    assert!(card.members.iter().any(|person| person.name == "Gala"));
    let stream = card
        .streams
        .iter()
        .find(|stream| stream.person.user == ELENA)
        .expect("Elena streams");
    assert_eq!(stream.line, "Compartiendo su pantalla");
    assert_eq!(stream.channel, card.voice[0].channel);
}

#[test]
fn a_blocked_person_in_the_voice_channel_hides_the_whole_card() {
    let mut host = Host::new();
    host.open_friends_page();
    assert!(host
        .active_cards()
        .iter()
        .any(|card| card.title == "Ada, Bruno y 1 otro"));
    host.social(Command::BlockUser { user: CAMILA });
    let found = host.active_cards();
    assert!(
        found.iter().all(|card| card.title != "Ada, Bruno y 1 otro"),
        "{:?}",
        found.iter().map(|card| &card.title).collect::<Vec<_>>()
    );
    assert!(found
        .iter()
        .all(|card| card.members.iter().all(|person| person.user != CAMILA)));
}

#[test]
fn an_ignored_friend_hides_the_card_they_are_in() {
    let mut host = Host::new();
    host.open_friends_page();
    host.social(Command::IgnoreUser { user: BRUNO });
    let found = host.active_cards();
    assert!(found.iter().all(|card| card.title != "Ada, Bruno y 1 otro"));
    assert!(found
        .iter()
        .all(|card| card.members.iter().all(|person| person.user != BRUNO)));
}

#[test]
fn a_card_menu_opens_on_a_click_and_joins_the_voice_channel() {
    let mut host = Host::new();
    host.open_friends_page();
    let card = titled(&host.active_cards(), "Ada, Bruno y 1 otro");
    let channel = card.voice[0].channel;
    host.click(&format!("{}{}", ids::CARD, card.key));
    assert_eq!(host.ui.active_now.menu.as_deref(), Some(card.key.as_str()));
    // Three people: no message, but the voice channel and its server.
    let open = titled(&host.active_cards(), "Ada, Bruno y 1 otro");
    assert!(open.menu_open);
    assert!(
        open.menu.iter().any(|entry| matches!(
            entry,
            MenuEntry::Item(id, label, _) if id == &format!("{}{}", ids::JOIN, channel.0)
                && label == "Unirse al canal de voz"
        )),
        "{:?}",
        open.menu
    );
    assert!(open.menu.iter().any(|entry| matches!(
        entry,
        MenuEntry::Item(_, label, _) if label == "Ir al servidor"
    )));
    assert!(host.shows(&format!("{}{}", ids::JOIN, channel.0)));
    host.click(&format!("{}{}", ids::JOIN, channel.0));
    assert!(
        host.ui.active_now.menu.is_none(),
        "choosing closes the menu"
    );
    assert!(host
        .sent
        .iter()
        .any(|command| matches!(command, Command::JoinVoice { channel: joined, .. } if *joined == channel)));
}

#[test]
fn a_single_person_card_offers_a_message_to_them() {
    let mut host = Host::new();
    host.open_friends_page();
    let card = titled(&host.active_cards(), "Hugo");
    host.click(&format!("{}{}", ids::CARD, card.key));
    let open = titled(&host.active_cards(), "Hugo");
    assert!(
        open.menu.iter().any(|entry| matches!(
            entry,
            MenuEntry::Item(id, label, _) if id == &format!("friend:{}:message", HUGO.0)
                && label == "Enviar mensaje"
        )),
        "{:?}",
        open.menu
    );
}

#[test]
fn the_avatar_of_a_card_opens_its_person_menu_on_a_secondary_click() {
    let mut host = Host::new();
    host.open_friends_page();
    let avatar = format!("{}{}", ids::AVATAR, HUGO.0);
    let at = host.center_of(&avatar);
    host.send(UiEvent::ContextMenu { position: at });
    assert!(host.ui.person_menu.is_some(), "the person menu opened");
    assert!(host.shows(&format!("friend:{}:message", HUGO.0)));
    assert!(host.shows(&format!("friend:{}:remove", HUGO.0)));
}

#[test]
fn the_tick_asks_for_a_second_while_a_game_timer_runs() {
    let mut host = Host::new();
    host.open_friends_page();
    let wait = crate::active_now::tick(&host.ui, &host.store).expect("the page is open");
    assert!(wait <= Duration::from_millis(1_000), "{wait:?}");
    host.ui.friends.open = false;
    assert!(crate::active_now::tick(&host.ui, &host.store).is_none());
}
