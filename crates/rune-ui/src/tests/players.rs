//! Audio and video files: their players, the full window and a video that
//! pauses out of view.

use super::*;
use rune_components::message::{attachment_ids, media_surface};
use rune_model::{Attachment, AttachmentId};

/// #general's simulated file named `name`, opened.
fn opened(name: &str) -> (Host, MessageId, AttachmentId) {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let found = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .enumerate()
        .find_map(|(index, message)| {
            let file = message
                .attachments
                .iter()
                .find(|file| file.filename == name)?;
            Some((index, message.id, file.id))
        })
        .expect("the simulation has it");
    host.tree.scroll_to_index(&timeline_id(channel), found.0);
    host.frame();
    host.frame();
    (host, found.1, found.2)
}

fn act(host: &mut Host, action: UiAction) -> Vec<Intent> {
    update(&mut host.ui, &mut host.store, &action, host.now)
}

fn click(host: &mut Host, message: MessageId, attachment: AttachmentId, verb: &str) -> Vec<Intent> {
    act(
        host,
        UiAction::Clicked {
            id: attachment_ids::action(message, attachment, verb),
        },
    )
}

fn slide(
    host: &mut Host,
    message: MessageId,
    attachment: AttachmentId,
    verb: &str,
    fraction: f32,
) -> Vec<Intent> {
    act(
        host,
        UiAction::SliderChanged {
            id: attachment_ids::action(message, attachment, verb),
            fraction,
            committed: true,
        },
    )
}

fn player(host: &Host) -> &VoicePlayer {
    host.ui.voice_player.as_ref().expect("a player")
}

#[test]
fn an_audio_file_plays_seeks_and_keeps_its_volume() {
    let (mut host, message, attachment) = opened("ensayo.mp3");
    assert!(host.painted().iter().any(|text| text == "0:00 / --:--"));
    let intents = click(&mut host, message, attachment, "play");
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PlayVoice {
                request,
                source: None,
                kind: Played::Audio,
                ..
            } => Some(*request),
            _ => None,
        })
        .expect("its file is asked for, as audio");
    voice_loaded(&mut host.ui, request, Ok(()));
    assert_eq!(player(&host).state, VoiceState::Playing);
    // Its length is known once the host read its file.
    slide(&mut host, message, attachment, "seek", 0.5);
    assert_eq!(player(&host).seek, None, "nothing to seek in yet");
    host.ui.voice_player.as_mut().unwrap().duration = Some(10.0);
    slide(&mut host, message, attachment, "seek", 0.5);
    assert_eq!(player(&host).seek, Some(5.0));
    slide(&mut host, message, attachment, "volume", 0.3);
    assert_eq!(player(&host).volume, 0.3);
    click(&mut host, message, attachment, "mute");
    assert_eq!(player(&host).volume, 0.0);
    click(&mut host, message, attachment, "mute");
    assert_eq!(player(&host).volume, 0.3);
    click(&mut host, message, attachment, "play");
    assert_eq!(player(&host).state, VoiceState::Paused);
    assert!(host.painted().iter().any(|text| text == "0:05 / 0:10"));
}

#[test]
fn a_webm_video_plays_as_an_mp4_one_does() {
    let (mut host, message, attachment) = opened("demo.webm");
    let intents = click(&mut host, message, attachment, "play");
    assert!(intents.iter().any(|intent| matches!(
        intent,
        Intent::PlayVoice {
            kind: Played::Video,
            source: None,
            ..
        }
    )));
    assert_eq!(player(&host).attachment, attachment);
}

#[test]
fn a_video_fills_the_window_and_pauses_out_of_view_unless_it_does() {
    let (mut host, message, attachment) = opened("demo.mp4");
    let intents = click(&mut host, message, attachment, "play");
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PlayVoice {
                request,
                kind: Played::Video,
                ..
            } => Some(*request),
            _ => None,
        })
        .expect("its file is asked for, as video");
    voice_loaded(&mut host.ui, request, Ok(()));
    // Its pictures show once the host presented one.
    let surface = |frame: &limn::Frame| {
        frame.commands.iter().any(|command| {
            matches!(command, limn::DrawCommand::Surface { id, .. } if *id == media_surface(attachment))
        })
    };
    assert!(!surface(&host.frame()));
    host.ui.voice_player.as_mut().unwrap().picture = true;
    assert!(surface(&host.frame()));
    click(&mut host, message, attachment, "fullscreen");
    assert!(player(&host).fullscreen);
    host.frame();
    assert!(host.painted().iter().any(|text| text == "demo.mp4"));
    // Space pauses it there; Esc leaves the full window.
    host.key(Key::Space, false, false);
    assert_eq!(player(&host).state, VoiceState::Paused);
    host.key(Key::Space, false, false);
    host.key(Key::Escape, false, false);
    assert!(!player(&host).fullscreen);
    assert_eq!(player(&host).state, VoiceState::Playing);
    // Scrolled away, it pauses.
    let channel = host.ui.channel().unwrap();
    host.tree.scroll_to_index(&timeline_id(channel), 0);
    host.frame();
    host.frame();
    assert_eq!(player(&host).state, VoiceState::Paused);
}

#[test]
fn a_file_rune_does_not_play_offers_the_browser_and_saving_it() {
    let (mut host, message, _) = opened("demo.mp4");
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    updated.attachments = vec![Attachment {
        id: AttachmentId(600),
        filename: "clip.avi".into(),
        size_bytes: 1_000,
        content_type: Some("video/x-msvideo".into()),
        dimensions: Some((640, 360)),
        preview: None,
        download: Some(rune_model::RemoteFile {
            url: "https://cdn.example.test/clip.avi".into(),
        }),
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }];
    host.event(Event::MessageUpdated(updated));
    host.frame();
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text.contains("formato de video")),
        "{painted:?}"
    );
    assert!(click(&mut host, message, AttachmentId(600), "play").is_empty());
    assert!(host.ui.voice_player.is_none());
    let regions: Vec<_> = host
        .frame()
        .regions
        .into_iter()
        .map(|region| region.id)
        .collect();
    assert!(regions.contains(&attachment_ids::action(message, AttachmentId(600), "open")));
    assert!(regions.contains(&attachment_ids::action(
        message,
        AttachmentId(600),
        "save-as"
    )));
}
