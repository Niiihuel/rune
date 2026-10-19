use super::*;
use rune_components::message::attachment_ids;
use rune_model::AttachmentId;

/// #general's simulated voice message, opened.
fn opened() -> (Host, MessageId, AttachmentId) {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let found = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| message.is_voice())
        .map(|message| (message.id, message.attachments[0].id))
        .expect("the simulation has one");
    (host, found.0, found.1)
}

fn act(host: &mut Host, action: UiAction) -> Vec<Intent> {
    update(&mut host.ui, &mut host.store, &action, host.now)
}

fn play(host: &mut Host, message: MessageId, attachment: AttachmentId) -> Vec<Intent> {
    act(
        host,
        UiAction::Clicked {
            id: attachment_ids::action(message, attachment, "voice"),
        },
    )
}

fn requested(intents: &[Intent]) -> u64 {
    intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PlayVoice { request, .. } => Some(*request),
            _ => None,
        })
        .expect("its audio is asked for")
}

fn state(host: &Host) -> Option<VoiceState> {
    host.ui
        .voice_player
        .as_ref()
        .map(|player| player.state.clone())
}

#[test]
fn a_voice_message_plays_pauses_seeks_and_stops_with_its_conversation() {
    let (mut host, message, attachment) = opened();
    assert!(host.painted().iter().any(|text| text == "0:04"));
    let intents = play(&mut host, message, attachment);
    // The simulation's clip has no file: the host plays its own tone.
    assert!(matches!(
        intents.as_slice(),
        [Intent::PlayVoice { source: None, .. }]
    ));
    let request = requested(&intents);
    assert_eq!(state(&host), Some(VoiceState::Loading));
    voice_loaded(&mut host.ui, request + 1, Ok(()));
    assert_eq!(state(&host), Some(VoiceState::Loading), "not its answer");
    voice_loaded(&mut host.ui, request, Ok(()));
    assert_eq!(state(&host), Some(VoiceState::Playing));
    assert!(play(&mut host, message, attachment).is_empty());
    assert_eq!(state(&host), Some(VoiceState::Paused));
    play(&mut host, message, attachment);
    assert_eq!(state(&host), Some(VoiceState::Playing));
    let seek = |fraction, committed| UiAction::SliderChanged {
        id: attachment_ids::action(message, attachment, "seek"),
        fraction,
        committed,
    };
    act(&mut host, seek(0.5, false));
    let player = host.ui.voice_player.as_ref().unwrap();
    assert_eq!(
        (player.position, player.seek),
        (2.0, None),
        "a drag previews"
    );
    act(&mut host, seek(0.75, true));
    assert_eq!(host.ui.voice_player.as_ref().unwrap().seek, Some(3.0));
    host.ui.voice_player.as_mut().unwrap().position = 3.0;
    assert!(
        host.painted().iter().any(|text| text == "0:01"),
        "time left"
    );
    host.open_channel("anuncios");
    assert_eq!(state(&host), None, "leaving the conversation stops it");
}

#[test]
fn a_failed_voice_message_says_why_and_a_click_fetches_it_again() {
    let (mut host, message, attachment) = opened();
    let first = requested(&play(&mut host, message, attachment));
    voice_loaded(&mut host.ui, first, Err("sin conexión".into()));
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "No se pudo reproducir: sin conexión"));
    let again = play(&mut host, message, attachment);
    assert!(again.contains(&Intent::StopVoice { request: first }));
    assert_ne!(requested(&again), first);
    assert_eq!(state(&host), Some(VoiceState::Loading));
}

#[test]
fn a_voice_message_cannot_be_edited() {
    let (mut host, message, _) = opened();
    let channel = host.ui.channel().unwrap();
    let mut mine = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    mine.author = host.store.me().unwrap();
    mine.content.clear();
    host.event(Event::MessageUpdated(mine));
    assert!(crate::messages::edit(&mut host.ui, &host.store, channel, message).is_empty());
    assert!(host.ui.editing.is_none());
    assert!(host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .is_voice());
}

#[test]
fn a_clip_resumes_where_it_was_left_until_the_account_goes() {
    let (mut host, message, attachment) = opened();
    let request = requested(&play(&mut host, message, attachment));
    voice_loaded(&mut host.ui, request, Ok(()));
    host.ui.voice_player.as_mut().unwrap().position = 2.5;
    play(&mut host, message, attachment);
    host.open_channel("anuncios");
    host.open_channel("general");
    assert_eq!(state(&host), None);
    assert!(
        host.painted().iter().any(|text| text == "0:02"),
        "before it plays again, it shows where it was left"
    );
    let again = play(&mut host, message, attachment);
    assert!(matches!(again.as_slice(), [Intent::PlayVoice { .. }]));
    assert_eq!(host.ui.voice_player.as_ref().unwrap().position, 2.5);
    // Played to its end, the host rewinds it: it starts over next time.
    host.ui.voice_player.as_mut().unwrap().position = 0.0;
    host.open_channel("anuncios");
    host.open_channel("general");
    play(&mut host, message, attachment);
    assert_eq!(host.ui.voice_player.as_ref().unwrap().position, 0.0);
    host.ui.voice_player.as_mut().unwrap().position = 1.5;
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert_eq!(state(&host), None);
    assert_eq!(host.ui.voice_memory.recall(attachment), 0.0, "forgotten");
}

#[test]
fn the_speed_cycles_like_the_client_and_holds_for_every_clip() {
    let (mut host, message, attachment) = opened();
    assert!(host.painted().iter().any(|text| text == "1×"));
    let speed = attachment_ids::action(message, attachment, "speed");
    let mut seen = Vec::new();
    for _ in 0..4 {
        assert!(act(&mut host, UiAction::Clicked { id: speed.clone() }).is_empty());
        seen.push(voice_speed(&host.ui));
    }
    assert_eq!(seen, [1.5, 2.0, 0.75, 1.0]);
    act(&mut host, UiAction::Clicked { id: speed });
    assert!(host.painted().iter().any(|text| text == "1,5×"));
    assert_eq!(state(&host), None, "changing it plays nothing");
}
