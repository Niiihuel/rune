//! Calls in direct and group conversations (2026-10-17): the header's way to
//! call, answer or join; the call under the conversation's header, on the
//! stage of voice channels with what such a call adds; the notice of a call
//! coming in, with «Aceptar» and «Rechazar»; and what the host shows and
//! plays for one (a notification of the system, the ring). Its actions are
//! `call:{canal}:start|accept|decline` and the stage's `vz:ring-again` and
//! `vz:decline`.
//!
//! Since 2026-10-18 (llamadas-y-panel-de-voz), a direct call is asked about
//! first when the provider can ([`Command::CheckCall`]), as the official
//! client does: it starts, and rings only if it may (and «Volver a llamar»
//! does not ring it either); refused by the service, a dialog says they are
//! not friends and offers to ask them (`call:{canal}:befriend`,
//! `call:{canal}:not-now`), and any other failure is a notice. The system's
//! notification answers or declines.

use std::time::{Duration, Instant};

use limn::{ButtonVariant, Component, Element, Icon, IconButton, Size, TooltipSpec, UiAction};
use rune_components::private_call::{self as parts, ids, Incoming, PrivateStage, Refused};
use rune_components::voice::Participant;
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, MessageId, RelationshipKind, Status, UserId};
use rune_provider::{Command, Failure};
use rune_state::{Applied, CallStatus, Store};

use crate::{voice, Alert, Intent, Ui, ViewContext};

/// The calls coming in the system was told about, while they ring.
#[derive(Debug, Default)]
pub struct PrivateCalls {
    told: Vec<ChannelId>,
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// Direct calls asked about, until the provider answers.
    checking: Vec<ChannelId>,
    /// The direct call that could not be made, with what the provider
    /// said: its dialog is open.
    refused: Option<(ChannelId, String)>,
    /// Who was asked to be friends from that dialog, until it is known.
    befriended: Option<UserId>,
    /// The account's direct call the provider said may not ring, while the
    /// account is in it.
    silent: Option<ChannelId>,
}

/// The other person of the direct conversation `channel`.
fn other(store: &Store, channel: ChannelId) -> Option<UserId> {
    store
        .channel(channel)
        .filter(|found| found.kind == ChannelKind::Direct)
        .and_then(|found| found.recipients.first().copied())
}

/// The provider asks about a direct call before it starts.
fn asks_first(store: &Store, channel: ChannelId) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.call_check)
        && other(store, channel).is_some()
}

/// A failed check of `channel` is the dialog's: refused by the service, and
/// not with a friend (with whom it is something else). Anything else, as an
/// answer that never came, is said as a notice.
fn refusal(store: &Store, channel: ChannelId, refused: bool) -> bool {
    refused
        && other(store, channel).is_some_and(|user| {
            store
                .relationship(user)
                .is_none_or(|known| known.kind != RelationshipKind::Friend)
        })
}

/// The dialog says this failure; no notice repeats it.
pub(crate) fn shows_failure(store: &Store, failure: &Failure) -> bool {
    matches!(failure, Failure::CallCheck { channel, refused } if refusal(store, *channel, *refused))
}

/// A direct or group conversation, with calls of its own here.
pub(crate) fn callable(store: &Store, channel: ChannelId) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.private_calls)
        && store
            .channel(channel)
            .is_some_and(|found| matches!(found.kind, ChannelKind::Direct | ChannelKind::Group))
}

/// The account's own call is in `channel`.
fn mine(status: Option<CallStatus>) -> bool {
    matches!(
        status,
        Some(
            CallStatus::Live
                | CallStatus::Ringing
                | CallStatus::Unanswered
                | CallStatus::Declined
                | CallStatus::HungUp
                | CallStatus::Alone
        )
    )
}

/// The header's button of a conversation with calls: to call, answer or
/// join, lit while the account is in it.
pub(crate) fn header_tool(store: &Store, channel: ChannelId, look: &Look) -> Option<Element> {
    if !callable(store, channel) {
        return None;
    }
    let abilities = store.abilities(channel)?;
    let status = store.call_status(channel);
    let hint = match status {
        Some(CallStatus::Incoming) => t("Atender la llamada"),
        Some(CallStatus::InProgress | CallStatus::Elsewhere) => t("Unirse a la llamada"),
        _ if mine(status) => t("Estás en la llamada"),
        _ => t("Iniciar llamada"),
    };
    let denied = match status {
        None => abilities
            .ring
            .reason()
            .or(abilities.join_voice.reason())
            .map(str::to_owned),
        _ => abilities.join_voice.reason().map(str::to_owned),
    };
    let button = IconButton::new(ids::start(channel.0), Icon::Phone, look.menu_palette())
        .variant(ButtonVariant::Ghost)
        .accessibility_label(hint)
        .ink(if mine(status) {
            look.online
        } else {
            look.muted
        })
        .active(mine(status));
    Some(match denied {
        Some(reason) => button
            .tooltip_with(TooltipSpec::new(hint).description(reason))
            .disabled()
            .render(),
        None => button.tooltip(hint).render(),
    })
}

/// Call, answer or join the call of `channel`, as it is now.
fn start(ui: &mut Ui, store: &mut Store, channel: ChannelId, now: Instant) -> Vec<Intent> {
    match store.call_status(channel) {
        status if mine(status) => Vec::new(),
        Some(_) => voice::join(ui, store, channel, now),
        None => {
            let ring = store
                .abilities(channel)
                .and_then(|abilities| abilities.ring.reason().map(str::to_owned));
            if let Some(reason) = ring {
                ui.notify(t("No se puede llamar"), Some(reason), true, now);
                return Vec::new();
            }
            // A direct call is asked about first; once is enough.
            if asks_first(store, channel) {
                if ui.private_calls.checking.contains(&channel) {
                    return Vec::new();
                }
                ui.private_calls.checking.push(channel);
                return vec![Intent::Command(Command::CheckCall { channel })];
            }
            call(ui, store, channel, true, now)
        }
    }
}

/// Start the call of `channel`, ringing everyone in it when `ring`.
fn call(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    ring: bool,
    now: Instant,
) -> Vec<Intent> {
    let mut intents = voice::join(ui, store, channel, now);
    // Everyone in it rings, once the service has the call.
    if ring && !intents.is_empty() {
        intents.push(Intent::Command(Command::RingCall {
            channel,
            users: Vec::new(),
        }));
    }
    intents
}

/// What the provider answered of a direct call asked about, and how
/// asking to be friends from its dialog went.
pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    let calls = &mut ui.private_calls;
    // Not ringing lasts as long as the account's call there.
    if calls.silent != store.call().map(|call| call.channel) {
        calls.silent = None;
    }
    match applied {
        Applied::CallChecked { channel, ringable } => {
            let Some(at) = calls.checking.iter().position(|asked| asked == channel) else {
                return Vec::new();
            };
            calls.checking.remove(at);
            // Someone called meanwhile: that call is the one to join.
            if store.call_status(*channel).is_some() {
                return start(ui, store, *channel, now);
            }
            let intents = call(ui, store, *channel, *ringable, now);
            ui.private_calls.silent = (!ringable && !intents.is_empty()).then_some(*channel);
            if !ringable && !intents.is_empty() {
                let name = store.channel_title(*channel);
                ui.notify(
                    t("La llamada no va a sonar"),
                    Some(tr!(
                        "{name} no recibe el aviso, pero puede entrar desde la conversación.",
                        name
                    )),
                    false,
                    now,
                );
            }
            intents
        }
        Applied::Failed {
            failure: Failure::CallCheck { channel, refused },
            reason,
            ..
        } => {
            calls.checking.retain(|asked| asked != channel);
            if refusal(store, *channel, *refused) {
                calls.refused = Some((*channel, reason.clone()));
            }
            Vec::new()
        }
        Applied::Relationship(user) if calls.befriended == Some(*user) => {
            let (Some(known), Some(account)) = (store.relationship(*user), store.user(*user))
            else {
                return Vec::new();
            };
            calls.befriended = None;
            let name = account.shown_name();
            let said = match known.kind {
                RelationshipKind::Friend => tr!("Ahora sos amigo de {name}", name),
                _ => tr!("Solicitud de amistad enviada a {name}", name),
            };
            ui.notify(said, None, false, now);
            Vec::new()
        }
        // Its failure is said as any other.
        Applied::Failed {
            failure: Failure::Relationship { .. },
            ..
        } => {
            calls.befriended = None;
            Vec::new()
        }
        Applied::Ready => {
            calls.checking.clear();
            calls.refused = None;
            calls.befriended = None;
            calls.silent = None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// «Enviar solicitud de amistad» or «Aceptar solicitud de amistad» in the
/// dialog of a refused call, and what it reads, or why it cannot (they
/// became friends, or were blocked); `None` while one is sent.
fn befriending(store: &Store, user: UserId) -> Option<(String, Result<(), String>)> {
    let permit =
        |permit: rune_state::Permit| permit.reason().map_or(Ok(()), |it| Err(it.to_owned()));
    let send = t("Enviar solicitud de amistad");
    match store.relationship(user).map(|known| known.kind) {
        Some(RelationshipKind::OutgoingRequest) => None,
        Some(RelationshipKind::IncomingRequest) => Some((
            t("Aceptar solicitud de amistad").into(),
            permit(store.social(user).accept),
        )),
        // Either may change while the dialog is open.
        Some(RelationshipKind::Friend) => Some((send.into(), Err(t("Ya son amigos.").into()))),
        Some(RelationshipKind::Blocked) => {
            let name = store.display_name(user, None);
            Some((
                send.into(),
                Err(tr!("Bloqueaste a {name}: desbloqueala para llamarle", name)),
            ))
        }
        None => Some((send.into(), permit(store.add_friend()))),
    }
}

/// Ask the other person of `channel` to be friends, from the dialog, which
/// closes; accepting their request if they sent one.
fn befriend(ui: &mut Ui, store: &Store, channel: ChannelId) -> Vec<Intent> {
    ui.private_calls.refused = None;
    let Some(user) = other(store, channel) else {
        return Vec::new();
    };
    let command = match store.relationship(user).map(|known| known.kind) {
        Some(RelationshipKind::IncomingRequest) if store.social(user).accept.allowed() => {
            Command::AcceptFriendRequest { user }
        }
        Some(
            RelationshipKind::OutgoingRequest
            | RelationshipKind::IncomingRequest
            | RelationshipKind::Friend
            | RelationshipKind::Blocked,
        ) => return Vec::new(),
        None => match store.user(user) {
            Some(account) if store.add_friend().allowed() => Command::SendFriendRequest {
                username: account.name.clone(),
            },
            _ => return Vec::new(),
        },
    };
    ui.private_calls.befriended = Some(user);
    vec![Intent::Command(command)]
}

/// Answer the call of `channel`: its conversation opens, with the call.
fn accept(ui: &mut Ui, store: &mut Store, channel: ChannelId, now: Instant) -> Vec<Intent> {
    let mut intents = if ui.channel() == Some(channel) {
        Vec::new()
    } else {
        crate::open(ui, store, None, channel)
    };
    intents.extend(voice::join(ui, store, channel, now));
    intents
}

/// Decline the call of `channel`: it rings the account no more.
fn decline(store: &mut Store, channel: ChannelId) -> Vec<Intent> {
    let Some(me) = store.me() else {
        return Vec::new();
    };
    store.decline_call(channel);
    vec![Intent::Command(Command::StopRinging {
        channel,
        users: vec![me],
    })]
}

/// Why the account's call in `channel` may not ring now: its permit, or
/// the provider said so when it was asked about.
fn ring_denied(ui: &Ui, store: &Store, channel: ChannelId) -> Option<String> {
    let silent = ui.private_calls.silent == Some(channel)
        && store.call().is_some_and(|call| call.channel == channel);
    store
        .abilities(channel)
        .and_then(|abilities| abilities.ring.reason().map(str::to_owned))
        .or_else(|| silent.then(|| t("La llamada no va a sonar").to_owned()))
}

/// «Volver a llamar»: everyone in the account's call rings again.
fn ring_again(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let Some(channel) = store.call().map(|call| call.channel) else {
        return Vec::new();
    };
    if let Some(reason) = ring_denied(ui, store, channel) {
        ui.notify(t("No se puede llamar"), Some(reason), true, now);
        return Vec::new();
    }
    vec![Intent::Command(Command::RingCall {
        channel,
        users: Vec::new(),
    })]
}

/// Answer these actions. `None` for actions of other parts.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    // «Dejar de llamar» to someone the account's call rings.
    if let Some(user) = id.strip_prefix(ids::STOP_RINGING) {
        let user = UserId(user.parse().ok()?);
        let channel = store.call()?.channel;
        return Some(vec![Intent::Command(Command::StopRinging {
            channel,
            users: vec![user],
        })]);
    }
    match id.as_str() {
        ids::RING_AGAIN => return Some(ring_again(ui, store, now)),
        ids::DECLINE => {
            return Some(
                ui.channel()
                    .map_or_else(Vec::new, |channel| decline(store, channel)),
            )
        }
        _ => {}
    }
    let (channel, what) = id.strip_prefix("call:")?.split_once(':')?;
    let channel = ChannelId(channel.parse().ok()?);
    Some(match what {
        "start" => start(ui, store, channel, now),
        "accept" => accept(ui, store, channel, now),
        "decline" => decline(store, channel),
        "befriend" => befriend(ui, store, channel),
        "not-now" => {
            ui.private_calls.refused = None;
            Vec::new()
        }
        _ => return None,
    })
}

/// Who calls in `channel`: whoever is in its call, else the conversation.
fn caller(store: &Store, channel: ChannelId) -> String {
    let me = store.me();
    store
        .voice_in(channel)
        .iter()
        .find(|state| Some(state.user) != me)
        .map(|state| store.display_name(state.user, None))
        .unwrap_or_else(|| store.channel_title(channel))
}

/// Who is in the call of `channel`, besides the account, said in a line.
fn who(store: &Store, channel: ChannelId) -> String {
    let me = store.me();
    let names: Vec<String> = store
        .voice_in(channel)
        .iter()
        .filter(|state| Some(state.user) != me)
        .map(|state| store.display_name(state.user, None))
        .collect();
    match names.as_slice() {
        [] => t("La llamada sigue sin nadie adentro").into(),
        [one] => tr!("{one} está en la llamada", one),
        [one, two] => tr!("{one} y {two} están en la llamada", one, two),
        more => tr!("{count} personas están en la llamada", count = more.len()),
    }
}

/// What the call of `channel` adds to the stage, while there is one to show.
pub(crate) fn private_stage(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    look: &Look,
) -> Option<PrivateStage> {
    let status = store.call_status(channel)?;
    let me = store.me();
    let in_call = |user: UserId| {
        store
            .voice_in(channel)
            .iter()
            .any(|state| state.user == user)
    };
    let ringing: Vec<Participant> = store
        .ringing(channel)
        .into_iter()
        .filter(|user| Some(*user) != me && !in_call(*user))
        .map(|user| voice::participant(ui, store, None, user, look))
        .collect();
    let direct = store
        .channel(channel)
        .is_some_and(|found| found.kind == ChannelKind::Direct);
    let line = match status {
        CallStatus::Ringing => Some(t("Llamando…").to_owned()),
        CallStatus::Unanswered => Some(t("Sin respuesta").to_owned()),
        CallStatus::Declined if direct => Some(tr!(
            "{name} rechazó la llamada",
            name = store.channel_title(channel)
        )),
        CallStatus::Declined => Some(t("Rechazaron la llamada").to_owned()),
        CallStatus::HungUp if direct => {
            Some(tr!("{name} cortó", name = store.channel_title(channel)))
        }
        CallStatus::HungUp => Some(t("Todos salieron de la llamada").to_owned()),
        CallStatus::Alone => Some(t("Esperando a que alguien entre").to_owned()),
        // Outside the call nothing else says it is the simulation's.
        _ if voice::simulated(store) && !mine(Some(status)) => {
            Some(t("Llamada simulada").to_owned())
        }
        _ => None,
    };
    let ring_again = matches!(
        status,
        CallStatus::Unanswered | CallStatus::Declined | CallStatus::HungUp | CallStatus::Alone
    )
    .then(|| ring_denied(ui, store, channel).map_or(Ok(()), Err));
    let invitation = match status {
        CallStatus::Incoming => Some((
            tr!("{name} te está llamando", name = caller(store, channel)),
            t("Aceptar").to_owned(),
        )),
        CallStatus::InProgress => Some((who(store, channel), t("Unirse a la llamada").to_owned())),
        CallStatus::Elsewhere => Some((
            t("Estás en esta llamada desde otro dispositivo").to_owned(),
            t("Unirse desde acá").to_owned(),
        )),
        _ => None,
    };
    Some(PrivateStage {
        status: line,
        ringing,
        ring_again,
        invitation,
        incoming: status == CallStatus::Incoming,
    })
}

/// The call of the conversation open, under its header, `width` wide.
pub(crate) fn call_view(
    ui: &Ui,
    store: &Store,
    context: &ViewContext,
    look: &Look,
    width: f32,
) -> Option<Element> {
    let channel = ui.channel().filter(|channel| callable(store, *channel))?;
    let private = private_stage(ui, store, channel, look)?;
    let height = (context.window.height * 0.5).clamp(300.0, 480.0);
    let size = Size::new(width, height);
    voice::stage_of(ui, store, channel, size, Ok(false), Some(private), look)
        .map(|stage| stage.size(None, Some(height)))
}

/// The notice of the oldest call coming in, over `root`: not while its
/// conversation is open, which shows it already. Over it, the dialog of a
/// direct call that could not be made, while open.
pub(crate) fn overlay(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let root = incoming_notice(ui, store, root, look);
    let Some((channel, reason)) = &ui.private_calls.refused else {
        return root;
    };
    let Some(user) = other(store, *channel) else {
        return root;
    };
    parts::refused(
        root,
        &Refused {
            channel: channel.0,
            name: store.display_name(user, None),
            reason: reason.clone(),
            befriend: befriending(store, user),
        },
        look,
    )
}

fn incoming_notice(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let Some(channel) = store
        .incoming_calls()
        .into_iter()
        .find(|channel| ui.channel() != Some(*channel) && callable(store, *channel))
    else {
        return root;
    };
    let Some(found) = store.channel(channel) else {
        return root;
    };
    let group = found.kind == ChannelKind::Group;
    let other = found.recipients.first().copied();
    let notice = parts::incoming(
        &Incoming {
            channel: channel.0,
            name: if group {
                store.channel_title(channel)
            } else {
                caller(store, channel)
            },
            accent: if group {
                look.accent
            } else {
                other
                    .and_then(|user| store.user(user))
                    .map_or(look.faint, |user| look.ink(user.accent))
            },
            image: if group {
                ui.image(&found.icon)
            } else {
                other
                    .and_then(|user| store.user(user))
                    .and_then(|user| ui.picture(user.picture()))
            },
            group,
            simulated: voice::simulated(store),
        },
        look,
    );
    Element::stack(vec![
        root,
        Element::column(vec![
            Element::spacer().size(None, Some(64.0)),
            Element::row(vec![
                Element::spacer().flex(1.0),
                notice,
                Element::spacer().flex(1.0),
            ]),
            Element::spacer().flex(1.0),
        ]),
    ])
}

/// The notifications of the system for calls that just started ringing the
/// account: each once while it rings, none while busy, with «Aceptar» and
/// «Rechazar» ([`ANSWER`], [`DECLINE`]); and the conversations whose call
/// stopped ringing it, whose notification goes away.
pub fn call_alerts(ui: &mut Ui, store: &Store) -> (Vec<Alert>, Vec<ChannelId>) {
    let incoming = store.incoming_calls();
    let mut stopped = Vec::new();
    ui.private_calls.told.retain(|channel| {
        let rings = incoming.contains(channel);
        if !rings {
            stopped.push(*channel);
        }
        rings
    });
    let (Some(account), true) = (store.me(), ui.prefs.desktop_notifications) else {
        return (Vec::new(), stopped);
    };
    if store.presence(account).status == Status::DoNotDisturb {
        return (Vec::new(), stopped);
    }
    let mut alerts = Vec::new();
    for channel in incoming {
        if ui.private_calls.told.contains(&channel) || !callable(store, channel) {
            continue;
        }
        ui.private_calls.told.push(channel);
        alerts.push(Alert {
            account,
            channel,
            // A call has no message of its own.
            message: MessageId::default(),
            title: t("Llamada entrante").into(),
            body: tr!("{name} te está llamando", name = caller(store, channel)),
            notify: true,
            sound: false,
            call: true,
        });
    }
    (alerts, stopped)
}

/// The notification buttons of a call coming in: answering and declining.
pub const ANSWER: &str = "accept";
pub const DECLINE: &str = "decline";

/// A button pressed on the notification of a call: answer it (`accept`;
/// its conversation opens with the call) or decline it, if it still rings
/// the account signed in now. Otherwise answering opens the conversation,
/// and declining does nothing.
pub fn answer_alert(
    ui: &mut Ui,
    store: &mut Store,
    account: UserId,
    channel: ChannelId,
    accept: bool,
    now: Instant,
) -> Vec<Intent> {
    let rings = store.me() == Some(account)
        && store.call_status(channel) == Some(CallStatus::Incoming)
        && callable(store, channel);
    match (rings, accept) {
        (true, true) => self::accept(ui, store, channel, now),
        (true, false) => decline(store, channel),
        (false, true) => crate::open_alert(ui, store, account, channel),
        (false, false) => Vec::new(),
    }
}

/// Which ring sounds now: a call coming in, else the account's own while
/// it rings the others; `None` while busy for a call coming in, or neither.
pub fn ringing(store: &Store) -> Option<crate::CallSound> {
    let busy = store
        .me()
        .is_some_and(|me| store.presence(me).status == Status::DoNotDisturb);
    if !busy && !store.incoming_calls().is_empty() {
        return Some(crate::CallSound::Ring);
    }
    let own = store.call()?.channel;
    (store.call_status(own) == Some(CallStatus::Ringing)).then_some(crate::CallSound::RingOut)
}

/// How soon the host looks again at a ring, while one sounds.
pub fn ring_tick(store: &Store) -> Option<Duration> {
    ringing(store).map(|_| Duration::from_millis(250))
}
