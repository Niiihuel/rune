//! Calls in direct and group conversations (2026-10-17), from plain props:
//! what the call's stage adds for them (its line, those it rings, ringing
//! again, answering or declining) and the notice of a call coming in.

use limn::{Button, ButtonVariant, Color, Component, Element, Icon, Size};
use rune_i18n::t;

use crate::voice::Participant;
use crate::{centered, icon, line, strong_line, Look};

/// The actions these pieces report.
pub mod ids {
    /// The header's way to call, answer or join: `call:{canal}:start`.
    pub fn start(channel: u64) -> String {
        format!("call:{channel}:start")
    }

    /// The notice's «Aceptar» and «Rechazar» of the call of a conversation.
    pub fn accept(channel: u64) -> String {
        format!("call:{channel}:accept")
    }

    pub fn decline(channel: u64) -> String {
        format!("call:{channel}:decline")
    }

    /// The dialog of a direct call that could not be made
    /// (llamadas-y-panel-de-voz): asking to be friends, and closing it.
    pub fn befriend(channel: u64) -> String {
        format!("call:{channel}:befriend")
    }

    pub fn not_now(channel: u64) -> String {
        format!("call:{channel}:not-now")
    }

    /// On the stage: «Volver a llamar», and «Rechazar» beside joining.
    pub const RING_AGAIN: &str = "vz:ring-again";
    /// The header's «Expandir»/«Reducir»: the call over the whole
    /// conversation, with its chat beside it, or under its header again.
    pub const EXPAND: &str = "vz:expand-call";
    pub const DECLINE: &str = "vz:decline";
    /// «Dejar de llamar» on the tile of someone it rings.
    pub const STOP_RINGING: &str = "vz:stop-ringing:";

    pub fn stop_ringing(user: u64) -> String {
        format!("{STOP_RINGING}{user}")
    }
}

/// What a conversation's call adds to the stage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrivateStage {
    /// The line beside its title: «Invitando a la llamada», «Sin respuesta»…;
    /// `None` leaves the phase's.
    pub status: Option<String>,
    /// Who it rings and is not in it yet: their tiles say so.
    pub ringing: Vec<Participant>,
    /// «Volver a llamar», while nobody else is in it and it rings nobody;
    /// with why not, when it cannot.
    pub ring_again: Option<Result<(), String>>,
    /// Outside the call: who is in it, and what joining says.
    pub invitation: Option<(String, String)>,
    /// It rings the account: «Rechazar» beside answering.
    pub incoming: bool,
    /// It takes the whole conversation (its header offers «Reducir»), or
    /// shows under the conversation's header («Expandir»).
    pub expanded: bool,
    /// VOICE mode (Discord's): nobody has a camera on or streams, so its
    /// people are a row of avatars, not tiles.
    pub voice_mode: bool,
}

/// «Aceptar» (`accept`) or «Rechazar», in limn's green and red.
fn answer(id: String, accept: bool, look: &Look) -> Element {
    let (label, variant, icon) = if accept {
        (t("Aceptar"), ButtonVariant::Success, Icon::Phone)
    } else {
        (t("Rechazar"), ButtonVariant::Destructive, Icon::HangUp)
    };
    Button::new(id, label, look.palette())
        .variant(variant)
        .icon(icon)
        .render()
}

/// The tile of someone it rings: their picture on their color, dimmed, with
/// «Invitando a la llamada», the way to stop ringing them, and their name in
/// the pill of a person's tile. VOICE mode shows them as an avatar instead.
pub fn ringing_tile(person: &Participant, size: Size, look: &Look) -> Element {
    let stop = Button::new(
        ids::stop_ringing(person.user),
        t("Dejar de llamar"),
        look.palette(),
    )
    .variant(ButtonVariant::Secondary)
    .size(limn::ControlSize::Compact)
    .render();
    let face = crate::voice::face_size(size);
    let body = centered(
        Element::column(vec![
            crate::avatar_with_image(&person.name, person.accent, face, person.image.clone()),
            centered(line(t("Invitando a la llamada"), 13.0, look.muted)).size(None, Some(18.0)),
            centered(stop).size(None, Some(28.0)),
        ])
        .gap(8.0),
    );
    let mut bottom: Vec<Element> = crate::voice::tile_name(person, size, look)
        .into_iter()
        .map(Element::shrink_to_fit)
        .collect();
    bottom.push(Element::spacer().flex(1.0));
    let bottom = Element::row(bottom).padding(crate::voice::margin(size));
    let fill = |element: Element| element.size(Some(size.width), Some(size.height));
    let picture = fill(Element::panel(body).radius(8.0).background(person.accent));
    let dim = fill(
        Element::panel(Element::spacer())
            .radius(8.0)
            .background(Color::rgba(0, 0, 0, 153)),
    );
    let over = fill(Element::column(vec![Element::spacer().flex(1.0), bottom]));
    fill(Element::panel(Element::stack(vec![picture, dim, over])))
        .radius(8.0)
        .border(Look::alpha(look.online, 90), 2.0)
}

/// «Volver a llamar», or why it cannot.
pub fn ring_again(permit: &Result<(), String>, look: &Look) -> Element {
    let button = Button::new(ids::RING_AGAIN, t("Volver a llamar"), look.palette())
        .variant(ButtonVariant::Secondary)
        .icon(Icon::Phone);
    match permit {
        Ok(()) => button.render(),
        Err(reason) => button
            .tooltip_with(limn::TooltipSpec::new(t("Volver a llamar")).description(reason.clone()))
            .disabled()
            .render(),
    }
}

/// «Rechazar» beside answering, on the stage.
pub fn decline(look: &Look) -> Element {
    answer(ids::DECLINE.into(), false, look)
}

/// A call coming in, over the window.
#[derive(Clone, Debug, PartialEq)]
pub struct Incoming {
    pub channel: u64,
    /// Who calls, or the group's name.
    pub name: String,
    pub accent: Color,
    pub image: Option<limn::RasterImage>,
    /// A group's call: its mark when it has no picture.
    pub group: bool,
    /// The simulation's: it says so.
    pub simulated: bool,
}

/// The notice of a call coming in: who calls, «Aceptar» and «Rechazar».
pub fn incoming(call: &Incoming, look: &Look) -> Element {
    let face = match (&call.image, call.group) {
        (None, true) => crate::sidebar::group_mark(call.accent, 56.0),
        (image, _) => crate::avatar_with_image(&call.name, call.accent, 56.0, image.clone()),
    };
    let card = Element::column(vec![
        centered(face).size(None, Some(56.0)),
        centered(strong_line(&call.name, 16.0, look.text)).size(None, Some(22.0)),
        centered(
            Element::row(vec![
                icon(Icon::Phone, look.online)
                    .size(Some(14.0), Some(14.0))
                    .centered_y(),
                line(
                    if call.simulated {
                        t("Llamada simulada entrante…")
                    } else {
                        t("Llamada entrante…")
                    },
                    13.0,
                    look.muted,
                )
                .centered_y(),
            ])
            .gap(6.0),
        )
        .size(None, Some(18.0)),
        Element::row(vec![
            Element::spacer().flex(1.0),
            answer(ids::decline(call.channel), false, look),
            answer(ids::accept(call.channel), true, look),
            Element::spacer().flex(1.0),
        ])
        .gap(10.0),
    ])
    .gap(10.0)
    .padding(16.0);
    Element::panel(card)
        .size(Some(260.0), None)
        .radius(14.0)
        .background(look.menu)
        .border(look.border, 1.0)
        .shadow(look.shadow, limn::Point::new(0.0, 8.0), 24.0, 0.0)
        .accessibility_label(format!("{} · {}", t("Llamada entrante…"), call.name))
}

/// A direct call that could not be made (llamadas-y-panel-de-voz): the
/// person is not a friend, as the official client says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Refused {
    pub channel: u64,
    pub name: String,
    /// What the provider said.
    pub reason: String,
    /// What asking to be friends reads (sending a request, or accepting
    /// theirs), or why it cannot be done; `None` when one is on its way.
    pub befriend: Option<(String, Result<(), String>)>,
}

/// The dialog of a refused call over `root`: who, why, and asking to be
/// friends when it can be done.
pub fn refused(root: Element, view: &Refused, look: &Look) -> Element {
    let mut footer = vec![
        Element::spacer().flex(1.0),
        Button::new(ids::not_now(view.channel), t("Ahora no"), look.palette())
            .variant(ButtonVariant::Secondary)
            .render(),
    ];
    let mut body = vec![crate::text(view.reason.as_str(), 13.0, look.muted)];
    match &view.befriend {
        None => body.push(crate::text(
            t("Ya le enviaste una solicitud de amistad."),
            13.0,
            look.text,
        )),
        Some((label, permit)) => {
            let button = Button::new(ids::befriend(view.channel), label.as_str(), look.palette())
                .icon(Icon::Users);
            footer.push(match permit {
                Ok(()) => button.render(),
                Err(reason) => button
                    .tooltip_with(
                        limn::TooltipSpec::new(label.as_str()).description(reason.clone()),
                    )
                    .disabled()
                    .render(),
            });
        }
    }
    limn::Modal::new(
        root,
        true,
        rune_i18n::tr!("No se puede llamar a {name}", name = view.name),
        t("Solo se puede llamar a amigos."),
        ids::not_now(view.channel),
        look.menu_palette(),
    )
    .width(440.0)
    .body(Element::column(body).gap(8.0))
    .footer(Element::row(footer).gap(9.0))
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};

    fn person(user: u64) -> Participant {
        Participant {
            user,
            name: format!("persona {user}"),
            accent: Color::rgb(200, 100, 50),
            muted: false,
            deafened: false,
            speaking: false,
            server_muted: false,
            server_deafened: false,
            silenced: false,
            streaming: false,
            own: false,
            camera: None,
            camera_on: false,
            thumbnail: None,
            image: None,
            decoration: None,
            movable: false,
            carried: false,
            game: None,
        }
    }

    fn shown(element: Element) -> (Vec<String>, Vec<String>) {
        let (_, frame) = frame(element, Size::new(800.0, 600.0));
        (texts(&frame), controls(&frame))
    }

    #[test]
    fn a_call_coming_in_says_who_and_offers_to_answer_or_decline() {
        let look = Look::DARK;
        let notice = incoming(
            &Incoming {
                channel: 7,
                name: "Ada".into(),
                accent: look.accent,
                image: None,
                group: false,
                simulated: false,
            },
            &look,
        );
        let (said, usable) = shown(notice);
        for wanted in ["Ada", "Llamada entrante…", "Rechazar", "Aceptar"] {
            assert!(said.iter().any(|text| text == wanted), "{wanted}: {said:?}");
        }
        assert_eq!(usable, ["call:7:decline", "call:7:accept"]);
        let tile = ringing_tile(&person(3), Size::new(240.0, 220.0), &look);
        let (said, usable) = shown(tile);
        assert!(said.iter().any(|text| text == "Invitando a la llamada"));
        assert_eq!(usable, ["vz:stop-ringing:3"]);
        let (said, usable) = shown(ring_again(&Err("No".into()), &look));
        assert!(said.iter().any(|text| text == "Volver a llamar") && usable.is_empty());
    }
}
