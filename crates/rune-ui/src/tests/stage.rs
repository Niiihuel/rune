//! A stage from the account's seat: listening in the audience, raising a
//! hand, accepting an invitation and speaking, as the provider reports it.

use super::*;
use rune_model::{CallPhase, StageSeat, VoiceState};

fn seat(host: &Host, stage: ChannelId, suppressed: bool, request: Option<i64>) -> Event {
    let me = host.store.me().unwrap();
    Event::VoiceChanged(VoiceState {
        user: me,
        channel: stage,
        muted: false,
        deafened: false,
        speaking: false,
        streaming: false,
        // Held until a speaker, as a service holds the microphone.
        server_muted: suppressed || request.is_some(),
        server_deafened: false,
        camera: false,
        suppressed,
        speak_request: request.map(Timestamp),
    })
}

#[test]
fn on_a_stage_the_account_listens_raises_its_hand_and_speaks_when_invited() {
    let mut host = Host::new();
    let stage = host.channel_named("Escenario (simulado)");
    let me = host.store.me().unwrap();
    let from = host.sent.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("voice:{stage}"),
        },
        host.now,
    );
    host.run(intents);
    assert!(matches!(host.sent[from..], [Command::JoinVoice { channel, .. }] if channel == stage));
    // The provider seats it in the audience.
    host.event(seat(&host, stage, true, None));
    host.event(Event::CallPhase {
        channel: stage,
        phase: CallPhase::Connected,
        quality: None,
    });
    let painted = host.painted();
    assert!(
        painted.iter().any(|t| t == "Estás en el público"),
        "{painted:?}"
    );
    // The simulated audience has a raised hand; speakers list first.
    let hands = host
        .store
        .voice_in(stage)
        .iter()
        .filter(|state| state.stage_seat() == StageSeat::HandRaised)
        .count();
    assert_eq!(hands, 1);
    assert_eq!(
        host.store.mic_held().as_deref(),
        Some("Estás en el público: levantá la mano para pedir la palabra")
    );
    assert!(!host.store.abilities(stage).unwrap().speak.allowed());

    let from = host.sent.len();
    host.click(&format!("stage:hand:{stage}:up"));
    assert_eq!(
        host.sent[from..],
        [Command::RaiseHand {
            channel: stage,
            raised: true
        }]
    );
    // Nothing moves until the provider says so.
    assert_eq!(host.store.stage_seat(stage, me), Some(StageSeat::Audience));
    host.event(seat(&host, stage, true, Some(1_790_000_100_000)));
    let painted = host.painted();
    for text in [
        "Levantaste la mano",
        "Simulado: la moderación te invita en un momento",
        "Bajar la mano",
    ] {
        assert!(painted.iter().any(|t| t == text), "{text}: {painted:?}");
    }
    // Only moderation steps up without an invitation.
    let from = host.sent.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("stage:speak:{stage}"),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(host.sent.len(), from);

    // The invitation arrives; accepting it asks the provider.
    host.event(seat(&host, stage, false, Some(1_790_000_102_500)));
    assert!(host.painted().iter().any(|t| t == "Te invitaron a hablar"));
    host.click(&format!("stage:speak:{stage}"));
    assert_eq!(
        host.sent[from..],
        [Command::SetSpeaker {
            channel: stage,
            user: me,
            speaker: true
        }]
    );
    host.event(seat(&host, stage, false, None));
    assert!(host
        .painted()
        .iter()
        .any(|t| t == "Estás hablando en el escenario"));
    assert_eq!(host.store.mic_held(), None);
    // Moderation's entries are not the account's here.
    assert!(!host
        .store
        .stage_abilities(stage)
        .unwrap()
        .moderate
        .allowed());
    let from = host.sent.len();
    host.click(&format!("stage:down:{stage}"));
    assert!(matches!(host.sent[from..],
        [Command::SetSpeaker { speaker: false, user, .. }] if user == me));

    // As moderation, others are invited to speak or sent to the audience.
    let mut moderated = host.store.channel(stage).unwrap().clone();
    moderated.permissions = moderated.permissions | rune_model::Permissions::MUTE_MEMBERS;
    host.event(Event::ChannelChanged(moderated));
    let listener = host
        .store
        .voice_in(stage)
        .into_iter()
        .find(|state| state.user != me && state.stage_seat() == StageSeat::Audience)
        .unwrap()
        .user;
    let entries = crate::stage::menu_group(&host.store, stage, listener);
    assert!(matches!(&entries[..],
        [rune_components::voice::MenuEntry::Item(id, label, None)]
            if *id == format!("stage:invite:{stage}:{listener}") && label == "Invitar a hablar"));
    let from = host.sent.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("stage:invite:{stage}:{listener}"),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(
        host.sent[from..],
        [Command::SetSpeaker {
            channel: stage,
            user: listener,
            speaker: true
        }]
    );
}
