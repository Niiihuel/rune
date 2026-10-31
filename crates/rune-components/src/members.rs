//! The member list: people grouped by role and by whether they are around,
//! and the card that opens on one of them (see [`crate::profile`]).

use limn::{Color, Element, FontFamily, Icon, PopoverPlacement};
use rune_i18n::t;

use crate::{avatar_with_image, gap, line, strong_line, with_status, Look};

pub use crate::profile::Profile;

/// The list's width, its divider included: a hairline and 285.5 of list.
pub const WIDTH: f32 = 286.5;
/// A person's row is this high.
pub const ROW: f32 = 43.0;
/// Clear above and below a row's ground, its hover or nameplate: two rows'
/// grounds never touch. The official client pads each row 1 px each way
/// (`padding: 1px 0` on the row, so 2 px between grounds).
pub const ROW_MARGIN: f32 = 1.0;
/// The corners of a row's ground: the official client's `--radius-sm`.
pub const RADIUS: f32 = 8.0;
/// A heading is this high: room above it, its line, and a little below.
pub const HEADING: f32 = 50.0;
/// From the list's left and right edges to its rows.
pub const INSET: (f32, f32) = (8.5, 8.0);
/// A crown, after the name of whoever owns the server.
pub const CROWN: &str = crate::glyphs::glyph!("crown");

/// One person of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberRow {
    pub id: String,
    pub name: String,
    pub image: Option<limn::RasterImage>,
    pub ink: Color,
    pub accent: Color,
    pub status: Color,
    pub status_line: Option<String>,
    /// A server's emoji before `status_line`, when that is their custom
    /// status (2026-10-18).
    pub status_emoji: Option<crate::identity::TextPicture>,
    pub offline: bool,
    pub bot: bool,
    /// They own the server: a crown follows their name.
    pub owner: bool,
    /// What keeps them from taking part here: a clock follows their name.
    pub restricted: Option<String>,
    /// Their decoration, role mark, server tag, nameplate and phone (P5).
    pub marks: crate::identity::Marks,
}

/// A group of the list: a heading and how many people it has, all of
/// them, also those not loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub title: String,
    pub count: usize,
}

/// A heading of the list, its group as named: "Moderación — 3".
pub fn heading(group: &Group, look: &Look) -> Element {
    Element::column(vec![
        gap(0.0, 24.5),
        Element::row(vec![
            gap(13.0, 0.0),
            line(format!("{} — {}", group.title, group.count), 14.0, look.dim)
                .font_family(FontFamily::SansMedium)
                .centered_y(),
        ])
        .size(None, Some(20.0)),
    ])
    .size(None, Some(HEADING))
}

/// The `index`th person of the list not loaded yet: a pulsing face and a
/// name bar, its size. As the official client draws them: a bar 14 high,
/// rounded, 40 to 79 % of the room after the face, varying row to row
/// (here by place, so a frame and the next agree); still under reduced
/// motion, as any limn skeleton.
pub fn placeholder(index: usize, look: &Look) -> Element {
    let id = format!("members:loading:{index}");
    let share = 0.40 + (index * 37 % 40) as f32 / 100.0;
    Element::row(vec![
        gap(10.5, 0.0),
        limn::Component::render(limn::Skeleton::circle(
            format!("{id}:face"),
            look.palette(),
            34.0,
        ))
        .centered_y(),
        gap(13.5, 0.0),
        Element::row(vec![
            limn::Component::render(
                limn::Skeleton::new(format!("{id}:name"), look.palette())
                    .height(14.0)
                    .radius(RADIUS),
            )
            .flex(share)
            .centered_y(),
            Element::spacer().flex(1.0 - share),
        ])
        .flex(1.0),
        gap(8.0, 0.0),
    ])
    .size(None, Some(ROW))
    .accessible(limn::Access::new(limn::Role::ListItem).name(t("Cargando miembro")))
}

/// `face` with a phone in their status's color over its corner: they are
/// only on a phone.
fn on_phone(face: Element, status: Color, look: &Look) -> Element {
    let phone = Element::panel(
        Element::icon(Icon::Phone)
            .foreground(status)
            .size(Some(11.0), Some(13.0)),
    )
    .padding(1.5)
    .radius(4.0)
    .background(look.sidebar)
    .accessible(limn::Access::default().description(t("En el teléfono").to_owned()));
    Element::stack(vec![
        face,
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![Element::spacer().flex(1.0), phone]),
        ]),
    ])
    .size(Some(34.0), Some(34.0))
}

/// A person of the list; the card opens from it while `profile` is given.
pub fn member(row: &MemberRow, profile: Option<&Profile>, look: &Look) -> Element {
    let face = crate::identity::decorated(
        |side| avatar_with_image(&row.name, row.accent, side, row.image.clone()),
        34.0,
        row.marks.decoration.clone(),
    );
    let face = if row.marks.mobile && !row.offline {
        on_phone(face, row.status, look)
    } else {
        with_status(face, 34.0, row.status, look.sidebar)
    };
    let name = strong_line(
        &row.name,
        16.0,
        if row.offline { look.faint } else { row.ink },
    );
    // Around, in the gradient of their role (P5); offline, faint.
    let name = if row.offline {
        name
    } else {
        crate::identity::inked(name, row.ink, &row.marks.gradient)
    };
    let mut marks = crate::identity::after_name(&row.marks, 14.0, look);
    if row.owner {
        marks.push(
            Element::icon(Icon::Custom(CROWN))
                .foreground(look.accent)
                .size(Some(13.0), Some(13.0))
                .accessible(limn::Access::default().description(t("Dueño del servidor"))),
        );
    }
    if let Some(restricted) = &row.restricted {
        marks.push(
            Element::icon(Icon::Clock)
                .foreground(look.danger)
                .size(Some(13.0), Some(13.0))
                .accessible(limn::Access::default().description(restricted.clone())),
        );
    }
    let name = if marks.is_empty() {
        name
    } else {
        let mut parts = vec![name.shrink_to_fit()];
        for mark in marks {
            parts.push(gap(4.0, 0.0));
            parts.push(mark.centered_y());
        }
        parts.push(Element::spacer().flex(1.0));
        Element::row(parts)
    };
    let mut copy = vec![name];
    copy.extend(crate::identity::status_line(
        row.status_line.as_deref(),
        row.status_emoji.as_ref(),
        13.0,
        look.faint,
        look,
    ));
    let mut parts = vec![
        gap(10.5, 0.0),
        face.centered_y(),
        gap(13.5, 0.0),
        Element::column(copy).flex(1.0).centered_y(),
    ];
    if row.bot {
        parts.push(gap(8.0, 0.0));
        parts.push(
            Element::panel(
                Element::text("BOT")
                    .font_family(FontFamily::SansBold)
                    .font_size(9.0)
                    .foreground(look.on_accent),
            )
            .padding(3.0)
            .radius(3.0)
            .background(look.accent)
            .centered_y(),
        );
    }
    parts.push(gap(8.0, 0.0));
    let ground = ROW - 2.0 * ROW_MARGIN;
    let content = crate::identity::on_plate(
        Element::row(parts),
        ground,
        row.marks.plate.as_ref(),
        &row.id,
    );
    let button = Element::clickable(row.id.clone(), content)
        .size(None, Some(ground))
        .radius(RADIUS)
        .hover_background(look.hover)
        .accessible(limn::Access::new(limn::Role::ListItem).name(row.name.clone()));
    // The card stands against the ground, where a held one stands too.
    let button = crate::profile::card_popout(
        button,
        profile.map(|profile| crate::profile::card(profile, look)),
        crate::profile::ids::MEMBER_DISMISS,
        PopoverPlacement::LeftStart,
        look,
    )
    .popover_gap(crate::profile::LIST_GAP);
    Element::column(vec![gap(0.0, ROW_MARGIN), button, gap(0.0, ROW_MARGIN)]).size(None, Some(ROW))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn row(name: &str) -> MemberRow {
        MemberRow {
            marks: Default::default(),
            id: format!("member:{name}"),
            name: name.into(),
            image: None,
            ink: Look::DARK.text,
            accent: Color::rgb(90, 120, 200),
            status: Look::DARK.online,
            status_line: Some("trabajando".into()),
            status_emoji: None,
            offline: false,
            bot: false,
            owner: false,
            restricted: None,
        }
    }

    #[test]
    fn a_member_opens_their_card_with_a_way_to_write_to_them() {
        let profile = Profile {
            identity: Default::default(),
            image: None,
            banner: None,
            banner_tint: None,
            name: "Ada".into(),
            handle: "ada".into(),
            accent: Color::rgb(90, 120, 200),
            status: rune_model::Status::Online,
            presence: true,
            custom: None,
            custom_emoji: None,
            pronouns: Some("ella".into()),
            about: None,
            activity: Some("Jugando a Factorio".into()),
            roles: vec![("Moderación".into(), Color::rgb(200, 80, 80))],
            bot: false,
            message_action: Some("profile:dm:10".into()),
            restriction: None,
            member_since: Vec::new(),
            mutual: None,
            full_action: None,
            note: None,
            loading: false,
            relation: None,
            more: None,
            open_menu: None,
        };
        let list = Element::column(vec![
            heading(
                &Group {
                    title: "En línea".into(),
                    count: 1,
                },
                &Look::DARK,
            ),
            member(&row("Ada"), Some(&profile), &Look::DARK),
        ]);
        let (_, frame) = frame(
            Element::row(vec![
                Element::spacer().flex(1.0),
                list.size(Some(WIDTH), None),
            ]),
            Size::new(900.0, 600.0),
        );
        assert!(controls(&frame).contains(&"profile:dm:10".to_owned()));
        let painted = texts(&frame);
        assert!(painted.contains(&"En línea — 1".to_owned()), "{painted:?}");
        assert!(painted.contains(&"ada • ella".to_owned()), "{painted:?}");
        assert!(painted.contains(&"Moderación".to_owned()));
    }

    #[test]
    fn two_rows_grounds_stay_apart_by_two_pixels_as_the_official_client_s() {
        let (_, frame) = frame(
            Element::column(vec![
                member(&row("Ada"), None, &Look::DARK),
                member(&row("Bea"), None, &Look::DARK),
            ])
            .size(Some(WIDTH), None),
            Size::new(WIDTH, 2.0 * ROW),
        );
        let ground = |id: &str| {
            frame
                .regions
                .iter()
                .find(|region| region.id == id)
                .map(|region| region.rect)
                .unwrap()
        };
        let (ada, bea) = (ground("member:Ada"), ground("member:Bea"));
        assert_eq!((ada.y, ada.height), (ROW_MARGIN, ROW - 2.0 * ROW_MARGIN));
        assert_eq!(bea.y - (ada.y + ada.height), 2.0 * ROW_MARGIN);
    }

    #[test]
    fn rows_not_loaded_yet_vary_their_name_bar_and_say_what_they_are() {
        let (_, frame) = frame(
            Element::column(
                (0..3)
                    .map(|index| placeholder(index, &Look::DARK))
                    .collect(),
            )
            .size(Some(WIDTH), None),
            Size::new(WIDTH, 3.0 * ROW),
        );
        let bars: Vec<f32> = frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::FillRect { rect, .. } if rect.height == 14.0 => Some(rect.width),
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 3, "{bars:?}");
        assert!(bars[0] != bars[1] && bars[1] != bars[2], "{bars:?}");
    }

    #[test]
    fn a_custom_status_shows_its_servers_emoji_before_its_words() {
        let emoji = crate::identity::TextPicture {
            token: "<:ok:1>".into(),
            name: "ok".into(),
            image: limn::RasterImage::new(
                "a".repeat(64),
                limn::RasterImageFormat::Png,
                3,
                std::sync::Arc::from("AAAA"),
                "ok",
            ),
            playing: false,
        };
        let shown = |status_line: Option<&str>| {
            let (_, frame) = frame(
                member(
                    &MemberRow {
                        status_line: status_line.map(str::to_owned),
                        status_emoji: Some(emoji.clone()),
                        ..row("Ada")
                    },
                    None,
                    &Look::ONYX,
                )
                .size(Some(WIDTH), None),
                Size::new(WIDTH, 60.0),
            );
            frame
        };
        let frame = shown(Some("de vacaciones hasta que vuelva la primavera, y más"));
        let picture = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("the emoji");
        assert_eq!((picture.width, picture.height), (14.0, 14.0));
        let (at, words) = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text { position, text, .. } if text.starts_with("de ") => {
                    Some((*position, text.clone()))
                }
                _ => None,
            })
            .expect("the words");
        assert!(at.x >= picture.x + 14.0 + 4.0 - 0.5, "{at:?}");
        assert!(words.ends_with('…'), "cut with «…»: {words}");
        // An emoji alone, without words.
        let alone = shown(None);
        assert!(texts(&alone).iter().all(|text| !text.starts_with("de ")));
        assert!(alone
            .commands
            .iter()
            .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
    }

    #[test]
    fn only_whoever_owns_the_server_wears_the_crown_after_their_name() {
        let crowns = |owner: bool| {
            let (_, frame) = frame(
                member(
                    &MemberRow {
                        owner,
                        ..row("Ada")
                    },
                    None,
                    &Look::ONYX,
                )
                .size(Some(WIDTH), None),
                Size::new(WIDTH, 60.0),
            );
            frame
                .commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::Icon {
                        rect,
                        icon: limn::Icon::Custom(CROWN),
                        ..
                    } => Some(*rect),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(crowns(false).is_empty());
        let crown = crowns(true);
        // 7 px characters: the name ends at 10.5 + 34 + 13.5 + 21.
        assert!(
            matches!(crown[..], [rect] if (rect.x - 83.0).abs() <= 1.0 && rect.width == 13.0),
            "{crown:?}"
        );
    }

    #[test]
    fn a_restricted_member_wears_a_clock_after_their_name() {
        let clocks = |restricted: Option<&str>| {
            let (_, frame) = frame(
                member(
                    &MemberRow {
                        restricted: restricted.map(str::to_owned),
                        ..row("Ada")
                    },
                    None,
                    &Look::ONYX,
                )
                .size(Some(WIDTH), None),
                Size::new(WIDTH, 60.0),
            );
            frame
                .commands
                .iter()
                .filter(|command| {
                    matches!(
                        command,
                        limn::DrawCommand::Icon {
                            icon: limn::Icon::Clock,
                            ..
                        }
                    )
                })
                .count()
        };
        assert_eq!(clocks(None), 0);
        assert_eq!(clocks(Some("Aislado")), 1);
    }
}
