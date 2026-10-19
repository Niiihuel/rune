//! The parts the rest of a server's administration (P7) adds to its
//! settings: a channel's row on the page that reorders them, a trait of the
//! server's profile and a forum's tag while they are edited. Pure functions
//! of props, like the rest of the crate.

use limn::{Button, ButtonSize, ButtonVariant, Component, Element, Icon, TextField};
use rune_i18n::{t, tr};
use rune_model::ChannelId;

use crate::server_settings::{menu, square_button, DropEdge, MenuAction};
use crate::{gap, icon, line, strong_line, Look};

/// A channel or a category as the page that reorders them lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSlot {
    pub channel: ChannelId,
    pub name: String,
    pub category: bool,
    /// Under a category: drawn a step in.
    pub nested: bool,
    pub glyph: Icon,
    pub movable: bool,
    pub menu: Vec<MenuAction>,
    pub menu_open: bool,
    pub drop: Option<DropEdge>,
}

/// The id a channel's row reports, dragged and dropped on.
pub fn slot_id(channel: ChannelId) -> String {
    format!("ss:chan:{channel}")
}

/// A row of the page that reorders channels: its handle, its kind, its
/// name, and its menu to move it without dragging.
pub fn channel_slot(row: &ChannelSlot, look: &Look) -> Element {
    let handle = if row.movable {
        icon(Icon::DragHandle, look.faint)
    } else {
        gap(16.0, 16.0)
    };
    let name = if row.category {
        strong_line(row.name.to_uppercase(), 13.0, look.muted)
    } else {
        line(&row.name, 15.0, look.text)
    };
    let body = Element::panel(
        Element::row(vec![
            handle.centered_y(),
            gap(if row.nested { 18.0 } else { 0.0 }, 0.0),
            icon(row.glyph, look.muted).centered_y(),
            name.flex(1.0).centered_y(),
        ])
        .gap(10.0)
        .padding(10.0),
    )
    .radius(6.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::ListItem).name(row.name.clone()));
    let body = Element::clickable(slot_id(row.channel), body);
    let body = if row.movable {
        body.draggable().drop_target()
    } else {
        body.drop_target()
    };
    let more = menu(
        square_button(
            &format!("ss:chan:{}:menu", row.channel),
            Icon::More,
            t("Mover"),
            row.movable,
            look,
        ),
        &row.menu,
        row.menu_open,
        "ss:chan-menu:dismiss",
        look,
    );
    let edge = |at: DropEdge| {
        Element::panel(Element::spacer())
            .size(None, Some(2.0))
            .background(if row.drop == Some(at) {
                look.accent
            } else {
                limn::Color::TRANSPARENT
            })
    };
    Element::column(vec![
        edge(DropEdge::Above),
        Element::row(vec![body.flex(1.0), more.centered_y()])
            .gap(8.0)
            .size(None, Some(42.0)),
        edge(DropEdge::Below),
    ])
}

/// One trait of the server's profile while it is edited: its emoji, its
/// label, and the button that takes it out. Fields `ss:trait:{index}:emoji`
/// and `ss:trait:{index}:label`.
pub fn trait_row(index: usize, emoji: &str, label: &str, enabled: bool, look: &Look) -> Element {
    Element::row(vec![
        TextField::new(
            format!("ss:trait:{index}:emoji"),
            emoji.to_owned(),
            "🙂",
            look.palette(),
        )
        .accessibility_label(tr!("Emoji del rasgo {n}", n = index + 1))
        .enabled(enabled)
        .render()
        // Room for `:name:` of one of the server's own (2026-10-19).
        .size(Some(110.0), None),
        TextField::new(
            format!("ss:trait:{index}:label"),
            label.to_owned(),
            t("Por ejemplo: Rust"),
            look.palette(),
        )
        .accessibility_label(tr!("Rasgo {n}", n = index + 1))
        .enabled(enabled)
        .render()
        .flex(1.0),
        remove_button(
            &format!("ss:trait:{index}:remove"),
            t("Quitar el rasgo"),
            enabled,
            look,
        ),
    ])
    .gap(8.0)
}

/// A forum's tag while it is edited.
#[derive(Clone, Debug, PartialEq)]
pub struct TagLine {
    pub index: usize,
    pub emoji: String,
    pub name: String,
    pub moderated: bool,
}

/// One tag of a forum's settings: its emoji, its name, whether only the
/// moderation puts it, and the button that takes it out. Fields
/// `cs:tag:{index}:emoji` and `cs:tag:{index}:name`.
pub fn tag_row(row: &TagLine, enabled: bool, look: &Look) -> Element {
    let index = row.index;
    Element::row(vec![
        TextField::new(
            format!("cs:tag:{index}:emoji"),
            row.emoji.clone(),
            "🏷️",
            look.palette(),
        )
        .accessibility_label(tr!("Emoji de la etiqueta {n}", n = index + 1))
        .enabled(enabled)
        .render()
        .size(Some(64.0), None),
        TextField::new(
            format!("cs:tag:{index}:name"),
            row.name.clone(),
            t("Nombre de la etiqueta"),
            look.palette(),
        )
        .accessibility_label(tr!("Etiqueta {n}", n = index + 1))
        .enabled(enabled)
        .render()
        .flex(1.0),
        limn::Toggle::new(
            format!("cs:tag:{index}:moderated"),
            t("Solo moderación"),
            row.moderated,
            look.palette(),
        )
        .enabled(enabled)
        .render()
        .centered_y(),
        remove_button(
            &format!("cs:tag:{index}:remove"),
            t("Quitar la etiqueta"),
            enabled,
            look,
        ),
    ])
    .gap(8.0)
}

fn remove_button(id: &str, label: &str, enabled: bool, look: &Look) -> Element {
    Button::new(id, "", look.palette())
        .icon(Icon::Close)
        .variant(ButtonVariant::Ghost)
        .button_size(ButtonSize::Small)
        .accessibility_label(label)
        .tooltip(label)
        .enabled(enabled)
        .render()
        .centered_y()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    const LOOK: Look = Look::DARK;

    #[test]
    fn a_channel_row_is_dragged_by_its_id_and_moved_from_its_menu() {
        let row = ChannelSlot {
            channel: ChannelId(7),
            name: "general".into(),
            category: false,
            nested: true,
            glyph: Icon::Hash,
            movable: true,
            menu: vec![MenuAction::new("ss:chan:7:up", "Subir")],
            menu_open: true,
            drop: Some(DropEdge::Below),
        };
        let (_, laid) = frame(channel_slot(&row, &LOOK), Size::new(600.0, 200.0));
        let ids = controls(&laid);
        assert!(ids.iter().any(|id| id == "ss:chan:7"), "{ids:?}");
        assert!(ids.iter().any(|id| id == "ss:chan:7:menu"));
        assert!(texts(&laid).iter().any(|text| text == "general"));
    }

    #[test]
    fn a_trait_and_a_tag_name_their_fields_by_their_place() {
        let (_, laid) = frame(
            Element::column(vec![
                trait_row(2, "🦀", "Rust", true, &LOOK),
                tag_row(
                    &TagLine {
                        index: 0,
                        emoji: String::new(),
                        name: "Ayuda".into(),
                        moderated: true,
                    },
                    true,
                    &LOOK,
                ),
            ]),
            Size::new(700.0, 200.0),
        );
        let ids = controls(&laid);
        for id in [
            "ss:trait:2:emoji",
            "ss:trait:2:label",
            "ss:trait:2:remove",
            "cs:tag:0:name",
            "cs:tag:0:moderated",
            "cs:tag:0:remove",
        ] {
            assert!(ids.iter().any(|found| found == id), "{id} in {ids:?}");
        }
    }
}
