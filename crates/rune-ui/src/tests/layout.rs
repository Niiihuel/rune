//! The window's columns and bands at the size of the reference captures
//! (1361×797), against what was measured on them (docs/ARCHITECTURE.md). RUNE has
//! no title bar of its own: what hangs from the top sits 34.5 higher than on
//! the captures, what stands on the bottom edge sits where it was.

use limn::{DrawCommand, Rect};

use super::*;

const REFERENCE: Size = Size::new(1361.0, 797.0);
/// The captures' title bar, which the system's decorations stand for.
const TITLE_BAR: f32 = 34.5;

/// The frame of the reference window with `general` open and the members shown.
fn reference() -> (Host, Frame) {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.ui.members_open = true;
    host.stir(&timeline_id(general));
    host.tree.set_root(view(
        &host.ui,
        &host.store,
        &ViewContext {
            window: REFERENCE,
            ..host.context()
        },
    ));
    let frame = host.tree.layout(REFERENCE);
    (host, frame)
}

fn region(frame: &Frame, id: &str) -> Rect {
    frame
        .regions
        .iter()
        .find(|region| region.id == id)
        .unwrap_or_else(|| panic!("{id} is not laid out"))
        .rect
}

/// `rect` is at `x`, `y` and `width`×`height`, give or take a pixel.
#[track_caller]
fn at(rect: Rect, x: f32, y: f32, width: f32, height: f32) {
    let near = |one: f32, other: f32| (one - other).abs() <= 1.0;
    assert!(
        near(rect.x, x) && near(rect.y, y) && near(rect.width, width) && near(rect.height, height),
        "{rect:?} is not at {x}, {y} {width}×{height}"
    );
}

/// The solid fills of the frame.
fn fills(frame: &Frame) -> Vec<Rect> {
    frame
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { rect, color, .. } if color.a == 255 => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn the_columns_and_bands_are_where_the_reference_has_them() {
    let (host, frame) = reference();
    let fills = fills(&frame);
    let filled = |x: f32, y: f32, width: f32, height: f32| {
        assert!(
            fills.iter().any(|rect| (rect.x - x).abs() <= 1.0
                && (rect.y - y).abs() <= 1.0
                && (rect.width - width).abs() <= 1.0
                && (rect.height - height).abs() <= 1.0),
            "nothing filled at {x}, {y} {width}×{height}"
        );
    };
    let top = |y: f32| y - TITLE_BAR;

    // The rail: 85.5 of marks, then the hairline at 85.5, which runs on
    // behind the account's panel; it ends where the panel begins.
    at(region(&frame, "rail"), 0.0, 0.0, 85.5, 730.0);
    filled(85.5, 0.0, 1.0, 730.0);
    filled(85.5, 730.0, 1.0, 67.0);
    // Its marks: 42 squares at 21.5, 52.5 apart. Home sits 5 below the top
    // (centered in the 52 band of the headers): Discord's touches its own
    // title bar, which RUNE does not draw.
    at(region(&frame, "home"), 21.5, top(34.5) + 5.0, 42.0, 42.0);
    let servers: Vec<Rect> = host
        .store
        .servers()
        .iter()
        .map(|server| region(&frame, &format!("server:{}", server.id)))
        .collect();
    at(servers[0], 21.5, 70.5, 42.0, 42.0);
    assert!((servers[1].y - servers[0].y - 52.5).abs() <= 1.0);
    // The chosen server's pill: 41 high, against the edge, on the mark's middle.
    let chosen = host.ui.server.unwrap();
    let mark = region(&frame, &format!("server:{chosen}"));
    assert!(
        frame.commands.iter().any(|command| matches!(command,
            DrawCommand::FillRect { rect, .. }
                if rect.x == -rune_components::rail::PILL_WIDTH
                    && rect.width == rune_components::rail::PILL_WIDTH * 2.0
                    && rect.height == rune_components::rail::PILL_SELECTED
                    && (rect.y + rect.height / 2.0 - (mark.y + mark.height / 2.0)).abs() <= 1.0)),
        "no pill on {mark:?}"
    );

    // The column of channels: 86.5 to 380, its hairline at 380; a header of
    // 52 and its hairline; the list under it.
    filled(86.5, top(34.5), 293.5, 52.0);
    filled(86.5, top(86.5), 293.5, 1.0);
    filled(380.0, 0.0, 1.0, 730.0);
    filled(380.0, 730.0, 1.0, 67.0);
    let list = region(&frame, "channels");
    at(list, 86.5, top(87.5), 293.5, list.height);
    // The server's name, as a pill of 31.5 from 96.
    let name = region(&frame, "sm:toggle");
    at(name, 96.0, top(45.0), name.width, 31.5);
    // Rows of 38 from 96 to 370.5, 40.5 apart.
    let general = host.channel_named("general");
    let row = region(&frame, &format!("channel:{general}"));
    at(row, 96.0, row.y, 274.5, 38.0);
    let rows: Vec<Rect> = frame
        .regions
        .iter()
        .filter(|region| region.id.starts_with("channel:"))
        .map(|region| region.rect)
        .collect();
    assert!(rows
        .windows(2)
        .any(|pair| (pair[1].y - pair[0].y - 40.5).abs() <= 1.0));

    // The account's box floats over the foot of both: 357.5×56 from 11.5,
    // 11 above the bottom edge, its buttons 32 square and 8 apart, 8 in.
    filled(11.5, 730.0, 357.5, 56.0);
    at(region(&frame, "me:mute"), 215.0, 742.0, 32.0, 32.0);
    at(region(&frame, "pp:input"), 248.0, 742.0, 16.0, 32.0);
    at(region(&frame, "me:deafen"), 272.0, 742.0, 32.0, 32.0);
    at(region(&frame, "me:settings"), 329.0, 742.0, 32.0, 32.0);

    // The conversation: 381 to 1074.5, under a header of 52 that runs over
    // the member list too, and its hairline.
    filled(381.0, top(34.5), 980.0, 52.0);
    filled(381.0, top(86.5), 980.0, 1.0);
    let timeline = region(&frame, &format!("timeline:{general}"));
    at(timeline, 381.0, top(87.5), 693.5, timeline.height);
    // Native limn rail at the column's right edge, beside the messages.
    let bar = frame
        .regions
        .iter()
        .find(|region| region.kind == limn::HitKind::Scrollbar && region.id == timeline_id(general))
        .expect("the conversation overflows")
        .rect;
    assert!(
        (bar.width - limn::ScrollbarAppearance::default().rail_width).abs() <= 1.0
            && (bar.x + bar.width - 1074.5).abs() <= 1.0,
        "{bar:?}"
    );
    // The header's search, a box of 258.5×32.5 at the right, and the
    // member list's toggle 45 before the box's gap.
    at(
        region(&frame, "message-search:open"),
        1093.5,
        top(44.5),
        258.5,
        32.5,
    );
    let toggle = region(&frame, "members:toggle");
    assert!(
        (toggle.x + toggle.width / 2.0 - 1060.25).abs() <= 1.0,
        "{toggle:?}"
    );

    // The member list: its hairline at 1074.5, then 1075.5 to the edge; a
    // heading of 50, then rows of 43 from 1084 to 1353.
    filled(1074.5, top(87.5), 1.0, 797.0 - top(87.5));
    at(
        region(&frame, "members"),
        1075.5,
        top(87.5),
        285.5,
        797.0 - top(87.5),
    );
    let first = frame
        .regions
        .iter()
        .find(|region| region.id.starts_with("member:"))
        .unwrap()
        .rect;
    // The ground of a row: 1 px clear of its slot above and below.
    at(first, 1084.0, top(137.5) + 1.0, 269.0, 41.0);
}

#[test]
fn timeline_native_scrollbar_keeps_controls_clear_and_scrolls_with_its_arrow() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let id = timeline_id(channel);
    host.stir(&id);
    let frame = host.frame();
    let bar = frame
        .regions
        .iter()
        .find(|region| region.id == id && region.kind == limn::HitKind::Scrollbar)
        .unwrap()
        .rect;
    let arrows = frame
        .commands
        .iter()
        .filter(|command| match command {
            DrawCommand::FillTriangle { points, .. } => {
                points.iter().all(|point| bar.contains(*point))
            }
            _ => false,
        })
        .count();
    assert_eq!(
        arrows, 2,
        "limn's native scrollbar has an arrow at each end"
    );
    // From the end, as the channel opens. The row the arrow brings in was
    // never measured and is taller than estimated: the list keeps the rows
    // on screen where they were, so the offset, in the content's
    // coordinates, can grow while the view moves up. The rows on screen
    // tell what the person sees.
    assert!(host.tree.is_scroll_at_end(&id, 1.0));
    let shown = |frame: &Frame, name: Option<&str>| {
        frame
            .regions
            .iter()
            .find(|region| match name {
                Some(name) => region.id == name,
                None => region.id.starts_with("pp:name:") && region.rect.height > 16.0,
            })
            .map(|region| (region.id.clone(), region.rect.y))
            .unwrap()
    };
    let (name, before) = shown(&frame, None);
    let up = Point::new(bar.x + bar.width / 2.0, bar.y + bar.width / 2.0);
    host.tree.handle_event(UiEvent::PointerDown(up));
    host.tree.handle_event(UiEvent::PointerUp(up));
    let frame = host.frame();
    let (_, after) = shown(&frame, Some(&name));
    assert!(
        after > before && !host.tree.is_scroll_at_end(&id, 1.0),
        "the arrow must scroll the timeline up: {name} from {before} to {after}"
    );
    let author = frame
        .regions
        .iter()
        .find(|region| region.id.starts_with("pp:name:") && region.rect.height > 16.0)
        .unwrap()
        .rect;
    host.tree.handle_event(UiEvent::PointerMove(Point::new(
        author.x + author.width / 2.0,
        author.y + author.height / 2.0,
    )));
    let frame = host.frame();
    let buttons: Vec<_> = frame
        .regions
        .iter()
        .filter(|region| region.id.starts_with("msg:") && region.kind == limn::HitKind::Button)
        .collect();
    assert!(
        !buttons.is_empty(),
        "hover must reveal message actions for the geometry check"
    );
    for region in buttons {
        assert!(
            region.rect.x + region.rect.width <= bar.x + 0.5,
            "{} overlaps the scrollbar: {:?} vs {bar:?}",
            region.id,
            region.rect,
        );
    }
}

#[test]
fn the_chat_scrollbar_wakes_under_the_pointer_and_hides_away_from_it() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let id = timeline_id(channel);
    let frame = host.frame();
    let chat = frame
        .regions
        .iter()
        .find(|region| region.id == id && region.kind == limn::HitKind::Scroll)
        .unwrap()
        .rect;
    let bar = |frame: &Frame| {
        frame
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Scrollbar)
    };
    // Away from the chat, the bar is gone once its hold and fade are over.
    host.tree
        .handle_event(UiEvent::PointerMove(Point::new(chat.x - 40.0, 5.0)));
    let later = Instant::now() + Duration::from_secs(5);
    assert!(!bar(&host.tree.layout_at(WINDOW, later)));
    // Over it, without scrolling, it shows.
    host.tree.handle_event(UiEvent::PointerMove(Point::new(
        chat.x + chat.width / 2.0,
        chat.y + chat.height / 2.0,
    )));
    assert!(bar(&host
        .tree
        .layout_at(WINDOW, later + Duration::from_secs(1))));
}

#[test]
fn unsupported_direct_presence_hides_status_and_stale_custom_text() {
    let mut host = Host::new();
    host.click("home");
    let (channel, recipient) = host
        .store
        .directs()
        .into_iter()
        .find(|channel| channel.kind == ChannelKind::Direct)
        .map(|channel| (channel.id, channel.recipients[0]))
        .unwrap();
    let status = rune_model::Status::Online;
    let custom = "Presencia de prueba";
    host.event(Event::PresenceChanged {
        user: recipient,
        presence: rune_model::Presence {
            status,
            custom: Some(custom.into()),
            custom_emoji: None,
            activities: Vec::new(),
            platforms: Default::default(),
        },
    });
    let ink = host.ui.look().status(status);
    let has_dot = |frame: &Frame| {
        let row = region(frame, &format!("channel:{channel}"));
        frame.commands.iter().any(|command| match command {
            DrawCommand::FillRect { rect, color, .. } => {
                *color == ink
                    && rect.width < 20.0
                    && row.contains(Point::new(
                        rect.x + rect.width / 2.0,
                        rect.y + rect.height / 2.0,
                    ))
            }
            _ => false,
        })
    };
    let has_custom = |frame: &Frame| {
        frame
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == custom))
    };
    for supported in [true, false, true] {
        let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
        // Friends' presence alone also shows on direct conversations.
        capabilities.presence = supported;
        capabilities.friend_presence = supported;
        let frame = host.frame();
        assert_eq!(
            has_dot(&frame),
            supported,
            "presence capability: {supported}"
        );
        assert_eq!(
            has_custom(&frame),
            supported,
            "custom presence capability: {supported}"
        );
    }
}

#[test]
fn unsupported_member_lists_are_hidden_and_ignore_stale_toggles() {
    let mut host = Host::new();
    host.open_channel("general");
    host.ui.members_open = true;
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .member_list = false;
    let frame = host.frame();
    assert!(!frame.regions.iter().any(|region| {
        region.id == "members" || region.id == "members:toggle" || region.id.starts_with("member:")
    }));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "members:toggle".into(),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert!(
        host.ui.members_open,
        "keep the preference for supported providers"
    );
}

#[test]
fn the_member_list_and_the_search_box_need_the_room_of_a_wide_window() {
    let mut host = Host::new();
    host.open_channel("general");
    host.ui.members_open = true;
    for (width, wide) in [(1361.0, true), (1100.0, false), (1024.0, false)] {
        let size = Size::new(width, 797.0);
        host.tree.set_root(view(
            &host.ui,
            &host.store,
            &ViewContext {
                window: size,
                ..host.context()
            },
        ));
        let frame = host.tree.layout(size);
        let shown = frame.regions.iter().any(|region| region.id == "members");
        assert_eq!(shown, wide, "the member list at {width}");
        let toggle = frame
            .regions
            .iter()
            .any(|region| region.id == "members:toggle");
        assert_eq!(toggle, wide, "the member list's toggle at {width}");
        let search = region(&frame, "message-search:open");
        assert_eq!(
            search.width == rune_components::chrome::SEARCH.0,
            wide,
            "the search box at {width}"
        );
        // The conversation keeps what the member list leaves it.
        let timeline = frame
            .regions
            .iter()
            .find(|region| region.id.starts_with("timeline:"))
            .unwrap()
            .rect;
        assert!(timeline.width >= 480.0, "{timeline:?} at {width}");
    }
}

#[test]
fn timeline_hover_stops_before_the_composer_even_when_it_grows() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    for draft in ["", "Una línea", "Primera\nSegunda\nTercera"] {
        host.ui.drafts.insert(channel, draft.into());
        host.tree.set_input_value(&editor_id(channel), draft);
        let frame = host.frame();
        let timeline = region(&frame, &timeline_id(channel));
        let card = fills(&frame)
            .into_iter()
            .find(|rect| {
                (rect.width - (timeline.width - 23.0)).abs() < 1.0
                    && rect.y >= timeline.y + timeline.height
            })
            .expect("the composer surface below the timeline");
        let clearance = card.y - (timeline.y + timeline.height);
        assert!(
            (8.0..=12.0).contains(&clearance),
            "the last message needs a compact, non-interactive gap: {clearance} ({draft:?})"
        );
        let between = Point::new(card.x + card.width / 2.0, card.y - clearance / 2.0);
        host.tree.handle_event(UiEvent::PointerMove(between));
        let frame = host.frame();
        assert!(
            frame.hit_test(between).is_none(),
            "the gap must not activate a message or composer: {:?}",
            frame.hit_test(between)
        );
    }
}
