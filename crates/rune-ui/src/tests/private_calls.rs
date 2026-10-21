//! Private calls (2026-10-17), driven as a person would: calling from a
//! conversation's header, the call under its header through each way it
//! goes, a call coming in with its notice, and declining it. The
//! simulation's answers are given as it gives them.

use rune_components::private_call::ids;
use rune_components::voice::ids as voice_ids;
use rune_model::{CallPhase, PrivateCall, UserId, VoiceState};
use rune_provider::Failure;

use super::*;

fn state(user: UserId, channel: ChannelId) -> VoiceState {
    VoiceState {
        user,
        channel,
        muted: false,
        deafened: false,
        speaking: false,
        streaming: false,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    }
}

impl Host {
    /// Open the first direct conversation from the home: its id and person.
    fn pc_direct(&mut self) -> (ChannelId, UserId) {
        let direct = self
            .store
            .directs()
            .into_iter()
            .find(|channel| channel.kind == ChannelKind::Direct)
            .map(|channel| (channel.id, channel.recipients[0]))
            .unwrap();
        self.click("home");
        self.click(&format!("channel:{}", direct.0));
        direct
    }

    fn pc_rings(&mut self, channel: ChannelId, ringing: &[UserId]) {
        self.event(Event::PrivateCall(PrivateCall {
            channel,
            ringing: ringing.to_vec(),
        }));
    }

    /// The service connects the account's call in `channel`.
    fn pc_connected(&mut self, channel: ChannelId) {
        let me = self.store.me().unwrap();
        self.event(Event::VoiceChanged(state(me, channel)));
        self.event(Event::CallPhase {
            channel,
            phase: CallPhase::Connected,
            quality: None,
        });
    }

    fn pc_says(&mut self, text: &str) -> bool {
        self.painted().iter().any(|painted| painted == text)
    }

    fn pc_offers(&mut self, id: &str) -> bool {
        self.frame()
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Button)
    }
}

#[test]
fn calling_from_the_header_rings_and_the_call_shows_how_it_goes() {
    let mut host = Host::new();
    let (direct, other) = host.pc_direct();
    assert!(host.pc_offers(&ids::start(direct.0)), "the header's phone");
    let from = host.sent.len();
    host.click(&ids::start(direct.0));
    // Asked about first, once however often it is pressed.
    host.click(&ids::start(direct.0));
    assert_eq!(host.sent[from..], [Command::CheckCall { channel: direct }]);
    let from = host.sent.len();
    host.event(Event::CallChecked {
        channel: direct,
        ringable: true,
    });
    assert_eq!(
        host.sent[from..],
        [
            Command::JoinVoice {
                channel: direct,
                muted: false,
                deafened: false
            },
            Command::RingCall {
                channel: direct,
                users: Vec::new()
            }
        ]
    );
    // Connected, ringing: the call shows under the header with who it rings.
    host.pc_connected(direct);
    host.pc_rings(direct, &[other]);
    assert!(host.pc_says("Llamando…"), "{:?}", host.painted());
    assert!(host.pc_offers(voice_ids::LEAVE), "the bar to hang up");
    // Ringing them may stop, for them alone.
    let from = host.sent.len();
    host.click(&ids::stop_ringing(other.0));
    assert_eq!(
        host.sent[from..],
        [Command::StopRinging {
            channel: direct,
            users: vec![other]
        }]
    );
    assert!(
        host.ui.channel() == Some(direct),
        "the conversation stays open"
    );
    // Answered, then hung up: the account may call again.
    host.event(Event::VoiceChanged(state(other, direct)));
    host.pc_rings(direct, &[]);
    assert!(!host.pc_says("Llamando…"));
    host.event(Event::VoiceLeft {
        user: other,
        channel: direct,
    });
    let name = host.store.channel_title(direct);
    assert!(
        host.pc_says(&format!("{name} cortó")),
        "{:?}",
        host.painted()
    );
    let from = host.sent.len();
    host.click(ids::RING_AGAIN);
    assert_eq!(
        host.sent[from..],
        [Command::RingCall {
            channel: direct,
            users: Vec::new()
        }]
    );
    // Hanging up ends it here.
    let from = host.sent.len();
    host.click(voice_ids::LEAVE);
    assert_eq!(host.sent[from..], [Command::LeaveVoice]);
    assert!(host.store.call().is_none());
}

#[test]
fn a_call_coming_in_shows_its_notice_and_is_declined_or_answered() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let (direct, caller) = host
        .store
        .directs()
        .into_iter()
        .find(|channel| channel.kind == ChannelKind::Direct)
        .map(|channel| (channel.id, channel.recipients[0]))
        .unwrap();
    let me = host.store.me().unwrap();
    host.event(Event::VoiceChanged(state(caller, direct)));
    host.pc_rings(direct, &[me]);
    assert!(host.pc_says("Llamada simulada entrante…"), "the notice");
    assert_eq!(crate::call_ringing(&host.store), Some(CallSound::Ring));
    // The system is told once, while it rings.
    host.ui.prefs.desktop_notifications = true;
    let (told, stopped) = crate::call_alerts(&mut host.ui, &host.store);
    assert_eq!(told.len(), 1);
    assert!(told[0].call && stopped.is_empty());
    assert_eq!(told[0].channel, direct);
    assert_eq!(
        crate::call_alerts(&mut host.ui, &host.store),
        (Vec::new(), Vec::new()),
        "once"
    );
    // Declined: it rings no more, and the service is told.
    let from = host.sent.len();
    host.click(&ids::decline(direct.0));
    assert_eq!(
        host.sent[from..],
        [Command::StopRinging {
            channel: direct,
            users: vec![me]
        }]
    );
    assert!(!host.pc_says("Llamada simulada entrante…"));
    assert_eq!(crate::call_ringing(&host.store), None);
    assert_eq!(host.ui.channel(), Some(general), "nothing opened");
    // Its notification goes away with it.
    assert_eq!(
        crate::call_alerts(&mut host.ui, &host.store),
        (Vec::new(), vec![direct])
    );
    // Rung again and accepted: its conversation opens with the call.
    host.pc_rings(direct, &[me]);
    let from = host.sent.len();
    host.click(&ids::accept(direct.0));
    assert_eq!(host.ui.channel(), Some(direct));
    assert!(host.sent[from..].contains(&Command::JoinVoice {
        channel: direct,
        muted: false,
        deafened: false
    }));
    assert!(
        !host.sent[from..]
            .iter()
            .any(|command| matches!(command, Command::RingCall { .. })),
        "answering rings nobody"
    );
}

#[test]
fn a_call_in_progress_or_on_another_device_invites_to_join_from_here() {
    let mut host = Host::new();
    let (direct, other) = host.pc_direct();
    let me = host.store.me().unwrap();
    host.event(Event::VoiceChanged(state(other, direct)));
    host.pc_rings(direct, &[]);
    assert!(host.pc_says("Unirse a la llamada"));
    // The account took it on another device.
    host.event(Event::VoiceChanged(state(me, direct)));
    assert!(host.pc_says("Estás en esta llamada desde otro dispositivo"));
    let from = host.sent.len();
    host.click(voice_ids::JOIN);
    assert_eq!(
        host.sent[from..],
        [Command::JoinVoice {
            channel: direct,
            muted: false,
            deafened: false
        }]
    );
    // The call ends for everyone: nothing shows of it any more.
    host.event(Event::VoiceLeft {
        user: other,
        channel: direct,
    });
    host.event(Event::CallEnded {
        channel: direct,
        reason: None,
    });
    host.event(Event::VoiceLeft {
        user: me,
        channel: direct,
    });
    host.event(Event::PrivateCallEnded { channel: direct });
    assert!(!host.pc_offers(voice_ids::JOIN));
}

#[test]
fn a_direct_call_asks_for_the_cameras_on_its_tiles_under_the_header() {
    let mut host = Host::new();
    let (direct, other) = host.pc_direct();
    host.click(&ids::start(direct.0));
    host.event(Event::CallChecked {
        channel: direct,
        ringable: true,
    });
    host.pc_connected(direct);
    let mut camera = state(other, direct);
    camera.camera = true;
    host.event(Event::VoiceChanged(camera));
    assert_eq!(host.ui.channel(), Some(direct));
    // 2026-10-21: only a voice channel's stage asked for them, and a
    // direct call's tiles kept showing faces.
    assert_eq!(crate::video::cameras(&host.ui, &host.store), vec![other]);
    // A secondary click on their tile, on a mark of it too, opens their menu
    // (their volume, silencing them for the account).
    for target in [
        voice_ids::tile(other.0),
        format!("{}:muted", voice_ids::tile(other.0)),
    ] {
        host.ui.voice.menu = None;
        let opened = crate::voice::secondary_click(
            &mut host.ui,
            &host.store,
            &target,
            Point::new(40.0, 40.0),
        );
        assert!(opened.is_some(), "{target}");
        assert!(
            host.ui
                .voice
                .menu
                .is_some_and(|menu| menu.user == other && menu.channel == direct),
            "{target}"
        );
    }
    host.ui.voice.menu = None;
    // On the stage, with its chat beside it, the chat does not draw the
    // call again.
    host.ui.voice.stage = Some(direct);
    let look = host.ui.look();
    let context = host.context();
    assert!(
        crate::private_calls::call_view(&host.ui, &host.store, &context, &look, 400.0).is_none()
    );
    assert_eq!(crate::video::cameras(&host.ui, &host.store), vec![other]);
}

#[test]
fn a_direct_call_expands_over_its_conversation_with_the_chat_beside_and_back() {
    let mut host = Host::new();
    let (direct, _) = host.pc_direct();
    host.click(&ids::start(direct.0));
    host.event(Event::CallChecked {
        channel: direct,
        ringable: true,
    });
    host.pc_connected(direct);
    assert!(host.pc_offers(ids::EXPAND), "under the header: «Expandir»");
    host.click(ids::EXPAND);
    assert_eq!(host.ui.voice.stage, Some(direct), "over the conversation");
    assert!(host.ui.voice.chat.is_some(), "its chat beside it");
    assert_eq!(host.ui.channel(), Some(direct));
    assert!(
        host.pc_offers(ids::EXPAND),
        "«Reducir» in the stage's header"
    );
    host.click(ids::EXPAND);
    assert_eq!(host.ui.voice.stage, None, "back under the header");
    assert!(host.ui.voice.chat.is_none());
    assert_eq!(host.ui.channel(), Some(direct));
}

#[test]
fn without_private_calls_the_header_offers_none() {
    let mut host = Host::new();
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.private_calls = false;
    }
    let (direct, _) = host.pc_direct();
    assert!(!host.pc_offers(&ids::start(direct.0)));
}

// Calls and the voice panel (llamadas-y-panel-de-voz).

impl Host {
    /// Call from the header of `direct`, answered by `check`: what it sent.
    fn pc_call_checked(&mut self, direct: ChannelId, check: Event) -> Vec<Command> {
        self.click(&ids::start(direct.0));
        let from = self.sent.len();
        self.event(check);
        self.sent[from..].to_vec()
    }
}

#[test]
fn a_direct_call_that_may_not_ring_starts_without_ringing() {
    let mut host = Host::new();
    let (direct, _) = host.pc_direct();
    let sent = host.pc_call_checked(
        direct,
        Event::CallChecked {
            channel: direct,
            ringable: false,
        },
    );
    assert_eq!(
        sent,
        [Command::JoinVoice {
            channel: direct,
            muted: false,
            deafened: false
        }]
    );
    assert!(
        host.pc_says("La llamada no va a sonar"),
        "{:?}",
        host.painted()
    );
    // An answer nobody waits for starts nothing.
    let from = host.sent.len();
    host.event(Event::CallChecked {
        channel: direct,
        ringable: true,
    });
    assert!(host.sent[from..].is_empty());
    // Nor does «Volver a llamar» ring it, while that call lasts.
    host.pc_connected(direct);
    assert!(host.pc_says("Volver a llamar"), "{:?}", host.painted());
    assert!(!host.pc_offers(ids::RING_AGAIN));
    // The next call is asked about again, and rings if it may.
    host.click(voice_ids::LEAVE);
    let me = host.store.me().unwrap();
    host.event(Event::VoiceLeft {
        user: me,
        channel: direct,
    });
    let sent = host.pc_call_checked(
        direct,
        Event::CallChecked {
            channel: direct,
            ringable: true,
        },
    );
    assert!(
        sent.contains(&Command::RingCall {
            channel: direct,
            users: Vec::new()
        }),
        "{sent:?}"
    );
    host.pc_connected(direct);
    host.pc_rings(direct, &[]);
    assert!(host.pc_offers(ids::RING_AGAIN));
}

#[test]
fn a_refused_direct_call_offers_to_be_friends_and_says_how_it_went() {
    let mut host = Host::new();
    let (direct, other) = host.pc_direct();
    host.event(Event::RelationshipRemoved { user: other });
    let refused = Event::Failed {
        failure: Failure::CallCheck {
            channel: direct,
            refused: true,
        },
        reason: "Llamada simulada: no son amigos.".into(),
        ambiguous: false,
    };
    let sent = host.pc_call_checked(direct, refused.clone());
    assert!(sent.is_empty(), "no call at all: {sent:?}");
    let name = host.store.display_name(other, None);
    assert!(host.pc_says(&format!("No se puede llamar a {name}")));
    assert!(host.pc_says("Solo se puede llamar a amigos."));
    assert!(
        !host.pc_says("No se pudo preparar la llamada"),
        "the dialog says it, no notice"
    );
    // «Ahora no» closes it; asking to be friends sends the request.
    host.click(&ids::not_now(direct.0));
    assert!(!host.pc_offers(&ids::befriend(direct.0)));
    host.pc_call_checked(direct, refused.clone());
    let from = host.sent.len();
    host.click(&ids::befriend(direct.0));
    let handle = host.store.user(other).unwrap().name.clone();
    assert_eq!(
        host.sent[from..],
        [Command::SendFriendRequest { username: handle }]
    );
    assert!(!host.pc_offers(&ids::befriend(direct.0)), "it closes");
    host.event(Event::RelationshipChanged(rune_model::Relationship {
        user: other,
        kind: rune_model::RelationshipKind::OutgoingRequest,
        since: None,
        nickname: None,
        spam: false,
    }));
    assert!(host.pc_says(&format!("Solicitud de amistad enviada a {name}")));
    // Refused again with the request on its way: nothing more to send.
    host.pc_call_checked(direct, refused);
    assert!(host.pc_says("Ya le enviaste una solicitud de amistad."));
    assert!(!host.pc_offers(&ids::befriend(direct.0)));
    // Blocked meanwhile: no request is offered to them either.
    host.event(Event::RelationshipChanged(rune_model::Relationship {
        user: other,
        kind: rune_model::RelationshipKind::Blocked,
        since: None,
        nickname: None,
        spam: false,
    }));
    assert!(host.pc_says("Enviar solicitud de amistad"));
    assert!(!host.pc_offers(&ids::befriend(direct.0)));
    assert!(host.pc_offers(&ids::not_now(direct.0)));
}

#[test]
fn a_direct_call_refused_with_a_friend_or_without_an_answer_is_a_notice() {
    // A friend refused by the service; a friend, or someone who is not one,
    // when its answer never came (no connection, a limit, its failure).
    for (friend, refused) in [(true, true), (true, false), (false, false)] {
        let mut host = Host::new();
        let (direct, other) = host.pc_direct();
        if !friend {
            host.event(Event::RelationshipRemoved { user: other });
        }
        host.pc_call_checked(
            direct,
            Event::Failed {
                failure: Failure::CallCheck {
                    channel: direct,
                    refused,
                },
                reason: "Sin red".into(),
                ambiguous: false,
            },
        );
        assert!(
            host.pc_says("No se pudo preparar la llamada"),
            "{friend} {refused}: {:?}",
            host.painted()
        );
        assert!(!host.pc_offers(&ids::befriend(direct.0)));
        assert!(!host.pc_offers(&ids::not_now(direct.0)));
    }
}

#[test]
fn a_call_notification_answers_or_declines_only_while_it_rings() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let (direct, caller) = host
        .store
        .directs()
        .into_iter()
        .find(|channel| channel.kind == ChannelKind::Direct)
        .map(|channel| (channel.id, channel.recipients[0]))
        .unwrap();
    let me = host.store.me().unwrap();
    host.event(Event::VoiceChanged(state(caller, direct)));
    host.pc_rings(direct, &[me]);
    // Another account's notification does nothing.
    let other = UserId(me.0 + 1);
    let now = host.now;
    assert!(
        crate::answer_alert(&mut host.ui, &mut host.store, other, direct, false, now).is_empty()
    );
    // «Rechazar» declines it, and the window stays where it was.
    let declined = crate::answer_alert(&mut host.ui, &mut host.store, me, direct, false, now);
    host.run(declined.clone());
    assert!(declined.contains(&Intent::Command(Command::StopRinging {
        channel: direct,
        users: vec![me]
    })));
    assert_eq!(host.ui.channel(), Some(general));
    // Pressed again once it no longer rings: nothing.
    assert!(crate::answer_alert(&mut host.ui, &mut host.store, me, direct, false, now).is_empty());
    // «Aceptar» while it rings: the conversation with the call.
    host.pc_rings(direct, &[me]);
    let accepted = crate::answer_alert(&mut host.ui, &mut host.store, me, direct, true, now);
    assert!(accepted.contains(&Intent::Command(Command::JoinVoice {
        channel: direct,
        muted: false,
        deafened: false
    })));
    assert_eq!(host.ui.channel(), Some(direct));
}
