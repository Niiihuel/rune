//! A stage, from the account's seat: the strip over the voice panel that
//! says where it sits and lets it raise its hand, accept an invitation or
//! step down; moderation's entries in a person's menu to invite them to
//! speak or send them to the audience; and a raised hand beside a name.
//! Every action asks the Store again first; the seat moves when the
//! provider reports it. Actions start with `stage:`.

use std::time::Instant;

use limn::{Element, UiAction};
use rune_components::stage::{self as parts, ids, StageAction, StageStrip};
use rune_components::voice::{MenuEntry, Participant};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, StageSeat, UserId};
use rune_provider::Command;
use rune_state::{Permit, Store};

use crate::{Intent, Ui};

/// The stage of the account's call, if its call is on one.
fn call_stage(store: &Store) -> Option<ChannelId> {
    let channel = store.call()?.channel;
    store.stage_seat(channel, store.me()?).map(|_| channel)
}

/// Answer the actions of a stage; `None` when the action is not one.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let rest = id.strip_prefix("stage:")?;
    let parts: Vec<&str> = rest.split(':').collect();
    let number = |at: usize| parts.get(at).and_then(|n| n.parse::<u64>().ok());
    let channel = ChannelId(number(1)?);
    ui.voice.menu = None;
    let can = store.stage_abilities(channel);
    let refuse = |ui: &mut Ui, permit: Option<&Permit>| {
        let reason = permit
            .and_then(Permit::reason)
            .unwrap_or(t("Ya no estás en ese escenario"));
        ui.notify(t("No se puede"), Some(reason.to_owned()), true, now);
        Some(Vec::new())
    };
    let me = store.me()?;
    let command = |user: UserId, speaker: bool| {
        Some(vec![Intent::Command(Command::SetSpeaker {
            channel,
            user,
            speaker,
        })])
    };
    match parts.first().copied()? {
        "hand" => {
            let raised = parts.get(2) == Some(&"up");
            match &can {
                // A hand always comes down.
                Some(can) if !raised || can.raise_hand.allowed() => {
                    Some(vec![Intent::Command(Command::RaiseHand {
                        channel,
                        raised,
                    })])
                }
                other => refuse(ui, other.as_ref().map(|can| &can.raise_hand)),
            }
        }
        "speak" => match &can {
            Some(can) if can.speak.allowed() => command(me, true),
            other => refuse(ui, other.as_ref().map(|can| &can.speak)),
        },
        "down" => match &can {
            Some(can) if can.step_down.allowed() => command(me, false),
            other => refuse(ui, other.as_ref().map(|can| &can.step_down)),
        },
        what @ ("invite" | "audience") => {
            let user = UserId(number(2)?);
            match &can {
                Some(can)
                    if can.moderate.allowed() && store.stage_seat(channel, user).is_some() =>
                {
                    command(user, what == "invite")
                }
                other => refuse(ui, other.as_ref().map(|can| &can.moderate)),
            }
        }
        _ => Some(Vec::new()),
    }
}

/// The strip over the voice panel while the account's call is on a stage.
pub(crate) fn panel(store: &Store, look: &Look) -> Option<Element> {
    let channel = call_stage(store)?;
    let seat = store.stage_seat(channel, store.me()?)?;
    let can = store.stage_abilities(channel)?;
    let action = |id: String, label: &str, primary: bool| StageAction {
        id,
        label: label.into(),
        primary,
    };
    let (label, mut actions) = match seat {
        StageSeat::Audience => (
            t("Estás en el público"),
            vec![action(
                ids::hand(channel, true),
                t("Levantar la mano"),
                true,
            )],
        ),
        StageSeat::HandRaised => (
            t("Levantaste la mano"),
            vec![action(ids::hand(channel, false), t("Bajar la mano"), false)],
        ),
        StageSeat::Invited => (
            t("Te invitaron a hablar"),
            vec![
                action(ids::speak(channel), t("Aceptar"), true),
                action(ids::step_down(channel), t("Rechazar"), false),
            ],
        ),
        StageSeat::Speaker => (
            t("Estás hablando en el escenario"),
            vec![action(
                ids::step_down(channel),
                t("Pasar al público"),
                false,
            )],
        ),
    };
    // Moderation steps up without waiting for an invitation.
    if matches!(seat, StageSeat::Audience | StageSeat::HandRaised) && can.speak.allowed() {
        actions.push(action(ids::speak(channel), t("Hablar"), false));
    }
    let simulated = crate::voice::simulated(store) && seat == StageSeat::HandRaised;
    Some(parts::strip(
        &StageStrip {
            seat: label.into(),
            detail: match seat {
                _ if simulated => Some(t("Simulado: la moderación te invita en un momento")),
                StageSeat::Audience => Some(t("Levanta la mano para pedir la palabra")),
                StageSeat::HandRaised => Some(t("Espera que la moderación te invite a hablar")),
                StageSeat::Invited => Some(t("Acepta para que te escuchen")),
                StageSeat::Speaker => None,
            }
            .map(str::to_owned),
            actions,
        },
        look,
    ))
}

/// Moderation's entries in the menu of `user` on the stage `channel`.
pub(crate) fn menu_group(store: &Store, channel: ChannelId, user: UserId) -> Vec<MenuEntry> {
    if store.me() == Some(user) {
        return Vec::new();
    }
    let (Some(seat), Some(can)) = (
        store.stage_seat(channel, user),
        store.stage_abilities(channel),
    ) else {
        return Vec::new();
    };
    if !can.moderate.allowed() {
        return Vec::new();
    }
    vec![match seat {
        StageSeat::Speaker | StageSeat::Invited => MenuEntry::Item(
            ids::audience(channel, user),
            t("Pasar al público").into(),
            None,
        ),
        StageSeat::Audience | StageSeat::HandRaised => MenuEntry::Item(
            ids::invite(channel, user),
            t("Invitar a hablar").into(),
            None,
        ),
    }]
}

/// On a stage, a raised hand beside each name and the speakers first.
pub(crate) fn mark(store: &Store, channel: ChannelId, people: &mut [Participant]) {
    if store
        .channel(channel)
        .is_none_or(|found| found.kind != rune_model::ChannelKind::Stage)
    {
        return;
    }
    let seat = |user: u64| store.stage_seat(channel, UserId(user));
    for person in people.iter_mut() {
        let hand = seat(person.user) == Some(StageSeat::HandRaised);
        person.name = parts::name(&person.name, hand);
    }
    people.sort_by_key(|person| seat(person.user) != Some(StageSeat::Speaker));
}
