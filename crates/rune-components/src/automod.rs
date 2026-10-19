//! The pieces of the AutoMod page of a server's settings (automod): a
//! rule as the list shows it, with its switch, and a kind of rule to make.
//! Controls are named `ss:am:…`.

use limn::{Button, ButtonSize, ButtonVariant, Component, Element, Icon, Switch};
use rune_i18n::t;
use rune_model::AutoModKind;

use crate::server_settings::list_row;
use crate::{icon, line, strong_line, text, Look};

/// The mark of a kind of rule; `None` for one RUNE does not know.
pub fn kind_glyph(kind: Option<AutoModKind>) -> Icon {
    match kind {
        Some(AutoModKind::Keywords) => Icon::Text,
        Some(AutoModKind::Mentions) => Icon::Bell,
        Some(AutoModKind::Spam) => Icon::WarningTriangle,
        Some(AutoModKind::Presets) => Icon::Book,
        Some(AutoModKind::Profile) => Icon::Users,
        None => Icon::Puzzle,
    }
}

/// A rule as the page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleLine {
    pub id: u64,
    pub name: String,
    /// What it looks for and what it does.
    pub notes: Vec<String>,
    pub glyph: Icon,
    pub enabled: bool,
    /// RUNE can change it without losing what it does not know.
    pub editable: bool,
    /// The account may change it.
    pub can: bool,
}

fn lead(glyph: Icon, look: &Look) -> Element {
    Element::panel(crate::centered(icon(glyph, look.muted)))
        .size(Some(32.0), Some(32.0))
        .radius(16.0)
        .background(look.hover)
}

fn small(id: String, glyph: Icon, label: String, enabled: bool, look: &Look) -> Element {
    Button::new(id, "", look.palette())
        .icon(glyph)
        .variant(ButtonVariant::Outline)
        .button_size(ButtonSize::Small)
        .enabled(enabled)
        .accessibility_label(label.clone())
        .tooltip(label)
        .render()
}

/// A row of the list: its mark, name and notes, its switch, and buttons to
/// edit it (when RUNE can) and delete it.
pub fn rule_row(row: &RuleLine, look: &Look) -> Element {
    let mut copy = vec![strong_line(&row.name, 15.0, look.text)];
    if !row.notes.is_empty() {
        copy.push(line(row.notes.join(" · "), 13.0, look.muted));
    }
    let id = row.id;
    let mut buttons = vec![Switch::new(
        format!("ss:am:rule:{id}:enabled"),
        row.enabled,
        look.palette(),
    )
    .accessibility_label(format!("{} {}", t("Activar"), row.name))
    .enabled(row.can)
    .render()];
    if row.editable {
        buttons.push(small(
            format!("ss:am:rule:{id}:edit"),
            Icon::Edit,
            format!("{} {}", t("Editar"), row.name),
            row.can,
            look,
        ));
    }
    buttons.push(small(
        format!("ss:am:rule:{id}:delete"),
        Icon::Trash,
        format!("{} {}", t("Eliminar"), row.name),
        row.can,
        look,
    ));
    list_row(lead(row.glyph, look), copy, buttons, 64.0, look)
}

/// A kind of rule to make: what it does, and «Crear» while the server has
/// room for one more and the account may.
pub fn new_rule_row(
    key: &str,
    kind: AutoModKind,
    title: &str,
    description: &str,
    enabled: bool,
    look: &Look,
) -> Element {
    let copy = vec![
        strong_line(title, 15.0, look.text),
        text(description, 13.0, look.muted),
    ];
    let create = Button::new(format!("ss:am:new:{key}"), t("Crear"), look.palette())
        .variant(ButtonVariant::Outline)
        .button_size(ButtonSize::Small)
        .enabled(enabled)
        .accessibility_label(format!("{} {title}", t("Crear")))
        .render();
    list_row(
        lead(kind_glyph(Some(kind)), look),
        copy,
        vec![create],
        72.0,
        look,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    const LOOK: Look = Look::DARK;

    fn row(editable: bool) -> RuleLine {
        RuleLine {
            id: 7,
            name: "Sin spoilers".into(),
            notes: vec!["Palabras propias".into(), "Bloquea".into()],
            glyph: kind_glyph(Some(AutoModKind::Keywords)),
            enabled: true,
            editable,
            can: true,
        }
    }

    #[test]
    fn a_rule_shows_its_switch_and_only_the_buttons_rune_can_honor() {
        let (_, laid) = frame(rule_row(&row(true), &LOOK), Size::new(700.0, 80.0));
        let ids = controls(&laid);
        for id in [
            "ss:am:rule:7:enabled",
            "ss:am:rule:7:edit",
            "ss:am:rule:7:delete",
        ] {
            assert!(ids.iter().any(|found| found == id), "{id}: {ids:?}");
        }
        assert!(texts(&laid)
            .iter()
            .any(|text| text.contains("Palabras propias · Bloquea")));
        let (_, unknown) = frame(rule_row(&row(false), &LOOK), Size::new(700.0, 80.0));
        assert!(!controls(&unknown).iter().any(|id| id.ends_with(":edit")));
    }

    #[test]
    fn a_kind_offers_to_create_one() {
        let element = new_rule_row(
            "mentions",
            AutoModKind::Mentions,
            "Spam de menciones",
            "Cuando alguien menciona a demasiadas personas.",
            true,
            &LOOK,
        );
        let (_, laid) = frame(element, Size::new(700.0, 90.0));
        assert!(controls(&laid).iter().any(|id| id == "ss:am:new:mentions"));
    }
}
