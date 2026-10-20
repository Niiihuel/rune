//! Pieces of the desktop work (P6): a channel the account cannot see, as the
//! column lists it when asked to; and the call's end-to-end encryption
//! codes, in the voice details.

use limn::{Button, ButtonVariant, Component, ControlSize, Element, FontFamily, Icon};

use rune_i18n::{t, tr};

use crate::{gap, icon, line, strong_line, text, Look};

/// An eye crossed out: a channel listed that the account cannot open.
pub const EYE_OFF: &str = crate::glyphs::glyph!("eye-off");

/// A channel of a server the account cannot see: dimmed, with the crossed
/// eye, and nothing happens on a click. Its unread marks are never shown.
/// A `private` one (`@everyone` may not see it) has the padlock too.
pub fn hidden_channel(name: &str, glyph: Icon, private: bool, look: &Look) -> Element {
    let ink = Look::alpha(look.faint, 150);
    let mut parts = vec![
        gap(12.0, 0.0),
        icon(glyph, ink).large_icon(20.0).centered_y(),
        gap(11.0, 0.0),
        line(name, 16.0, ink).flex(1.0).centered_y(),
    ];
    let mut access = limn::Access::new(limn::Role::ListItem)
        .name(tr!("{name}: canal oculto, no tenés acceso", name))
        .disabled(true);
    if private {
        parts.push(icon(Icon::Lock, ink).centered_y());
        parts.push(gap(4.0, 0.0));
        access = access.description(t("Canal privado"));
    }
    parts.push(icon(Icon::Custom(EYE_OFF), ink).centered_y());
    parts.push(gap(10.0, 0.0));
    Element::row(parts)
        .size(None, Some(crate::sidebar::ROW))
        .accessible(access)
}

/// The digits of a code in groups of five, `per_line` groups a line.
pub fn code_lines(code: &str, per_line: usize) -> Vec<String> {
    let groups: Vec<&str> = code
        .as_bytes()
        .chunks(5)
        .filter_map(|group| std::str::from_utf8(group).ok())
        .collect();
    groups
        .chunks(per_line.max(1))
        .map(|line| line.join(" "))
        .collect()
}

/// Someone of the call, and how far comparing codes with them got.
#[derive(Clone, Debug, PartialEq)]
pub struct Verifying {
    pub user: u64,
    pub name: String,
    /// `None` until asked; `Some(None)` while computing or when it could
    /// not be known, with `known: false`.
    pub code: Option<Option<String>>,
    pub asking: bool,
}

/// The call's encryption: its privacy code to compare, and the code of
/// each person, asked for one at a time. `simulated` says the codes are
/// made up.
pub fn privacy(code: Option<&str>, people: &[Verifying], simulated: bool, look: &Look) -> Element {
    let mut body = vec![strong_line(
        t("Cifrado de extremo a extremo"),
        14.0,
        look.text,
    )];
    let digits = |lines: Vec<String>| {
        Element::column(
            lines
                .into_iter()
                .map(|line| {
                    Element::text(line)
                        .font_family(FontFamily::Monospace)
                        .font_size(14.0)
                        .foreground(look.text)
                })
                .collect(),
        )
        .gap(2.0)
    };
    match code {
        Some(code) => {
            body.push(text(
                t("Código de privacidad de la llamada: si es el mismo para todos, nadie más tiene sus claves. Cambia cuando alguien entra o sale."),
                12.0,
                look.muted,
            ));
            body.push(
                Element::row(vec![
                    digits(code_lines(code, 3)).flex(1.0),
                    Button::new("e2ee:copy", t("Copiar"), look.menu_palette())
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Compact)
                        .render()
                        .centered_y(),
                ])
                .gap(8.0),
            );
        }
        None => body.push(text(
            t("La llamada todavía no está cifrada: aparece cuando hay alguien más."),
            12.0,
            look.muted,
        )),
    }
    if code.is_some() && !people.is_empty() {
        body.push(line(t("Verificar a cada persona"), 13.0, look.muted));
        for person in people {
            let mut row = vec![line(&person.name, 13.0, look.text).flex(1.0).centered_y()];
            if person.code.is_none() || !person.asking {
                row.push(
                    Button::new(
                        format!("e2ee:verify:{}", person.user),
                        if person.code.is_some() {
                            t("Otra vez")
                        } else {
                            t("Ver código")
                        },
                        look.menu_palette(),
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Compact)
                    .render()
                    .centered_y(),
                );
            }
            let mut column = vec![Element::row(row).gap(6.0)];
            match (&person.code, person.asking) {
                (Some(_), true) => column.push(line(t("Calculando…"), 12.0, look.faint)),
                (Some(Some(code)), false) => column.push(digits(code_lines(code, 3))),
                (Some(None), false) => column.push(line(
                    t("No se pudo calcular: puede que ya no esté en la llamada."),
                    12.0,
                    look.faint,
                )),
                (None, _) => {}
            }
            body.push(Element::column(column).gap(4.0));
        }
        body.push(text(
            t("Compará el código de una persona con el que ella ve para vos, por otro medio."),
            12.0,
            look.faint,
        ));
    }
    if simulated {
        body.push(
            Element::panel(
                Element::row(vec![
                    icon(Icon::InfoCircle, look.muted).centered_y(),
                    text(
                        t("Códigos simulados: la simulación no cifra nada."),
                        12.0,
                        look.muted,
                    )
                    .flex(1.0)
                    .centered_y(),
                ])
                .gap(6.0)
                .padding(8.0),
            )
            .radius(6.0)
            .background(look.raised),
        );
    }
    Element::column(body).gap(10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_reads_in_groups_of_five() {
        assert_eq!(
            code_lines(&"0123456789".repeat(3), 3),
            ["01234 56789 01234", "56789 01234 56789"]
        );
        assert_eq!(code_lines("1234567", 3), ["12345 67"]);
    }

    #[test]
    fn the_privacy_section_lays_out_with_and_without_a_code() {
        use crate::testing::{controls, frame, texts};
        let look = Look::DARK;
        let people = [Verifying {
            user: 7,
            name: "Ana".into(),
            code: Some(Some("1".repeat(45))),
            asking: false,
        }];
        let size = limn::Size::new(319.0, 600.0);
        let (_, shown) = frame(privacy(Some(&"2".repeat(30)), &people, true, &look), size);
        let ids = controls(&shown);
        assert!(ids.contains(&"e2ee:copy".to_owned()) && ids.contains(&"e2ee:verify:7".to_owned()));
        assert!(texts(&shown).contains(&"22222 22222 22222".to_owned()));
        assert!(texts(&shown).contains(&"11111 11111 11111".to_owned()));
        let (_, waiting) = frame(privacy(None, &people, false, &look), size);
        assert!(
            controls(&waiting).is_empty(),
            "nothing to copy or verify yet"
        );
        let (_, hidden) = frame(hidden_channel("secreto", Icon::Hash, true, &look), size);
        assert!(
            controls(&hidden).is_empty(),
            "a hidden channel does not open"
        );
        assert!(hidden.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::Icon {
                icon: Icon::Lock,
                ..
            }
        )));
    }
}
