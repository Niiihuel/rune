//! The channel list, its menus and drags, and the header's tools, driven as
//! a person would, with the simulation answering what they ask of it.

use limn::Frame;
use rune_components::channel_menu::ids;
use rune_components::Look;
use rune_model::{ChannelMove, Overwrite, OverwriteTarget, Permissions, ServerId};
use rune_provider::ChannelEdit;

use super::*;
use crate::channel_list::{self as list, ListDialog, Target};

impl Host {
    /// Have the simulation answer the management and pin commands sent from
    /// `from` on, those its answers lead to included, and apply what it reports.
    fn cl_answer(&mut self, from: usize) {
        let mut next = from;
        while next < self.sent.len() {
            let command = self.sent[next].clone();
            next += 1;
            if matches!(
                command,
                Command::CreateChannel { .. }
                    | Command::EditChannel { .. }
                    | Command::ReorderChannels { .. }
                    | Command::DeleteChannel { .. }
                    | Command::SetOverwrite { .. }
                    | Command::PinMessage { .. }
                    | Command::UnpinMessage { .. }
                    | Command::LoadPins { .. }
            ) {
                for event in self.provider.manage(command) {
                    self.event(event);
                }
            }
        }
    }

    fn cl_server(&mut self, name: &str) -> ServerId {
        let server = self
            .store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .map(|server| server.id)
            .unwrap();
        self.click(&format!("server:{server}"));
        server
    }

    /// The channel or category `name` of `server`.
    fn cl_named(&self, server: ServerId, name: &str) -> ChannelId {
        self.store
            .channel_tree(server)
            .into_iter()
            .flat_map(|(category, channels)| category.into_iter().chain(channels))
            .find(|channel| channel.name == name)
            .map(|channel| channel.id)
            .unwrap()
    }

    /// What a click on `id` reports, without the window answering it: for
    /// items inside a submenu, which opens on hover.
    fn cl_choose(&mut self, id: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents);
    }

    fn cl_rect(frame: &Frame, id: &str) -> limn::Rect {
        frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"))
            .rect
    }

    /// A secondary click on the left part of the control `id`.
    fn cl_right_click(&mut self, id: &str) {
        let frame = self.frame();
        let rect = Self::cl_rect(&frame, id);
        let at = Point::new(rect.x + 12.0, rect.y + rect.height / 2.0);
        self.send(UiEvent::ContextMenu { position: at });
    }

    /// Press on the row `from`, drag it over `to`, and let go there.
    fn cl_drag(&mut self, from: &str, to: &str) {
        let frame = self.frame();
        let point = |id: &str| {
            let rect = Self::cl_rect(&frame, id);
            Point::new(rect.x + 24.0, rect.y + rect.height / 2.0)
        };
        let (start, end) = (point(from), point(to));
        self.send(UiEvent::PointerDown(start));
        self.send(UiEvent::PointerMove(Point::new(start.x, start.y + 8.0)));
        self.send(UiEvent::PointerMove(end));
        self.send(UiEvent::PointerUp(end));
    }

    fn cl_buttons(&mut self) -> Vec<String> {
        self.frame()
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
            .map(|region| region.id.clone())
            .collect()
    }

    fn cl_access(&mut self, id: &str) -> limn::AccessNode {
        self.frame()
            .access
            .iter()
            .find(|node| node.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} is not said"))
    }

    fn cl_said(&mut self) -> String {
        self.painted()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn cl_saved(&self, intents_from: &[Intent], key: &str) -> bool {
        intents_from
            .iter()
            .any(|intent| matches!(intent, Intent::SaveSetting { key: kept, .. } if kept == key))
    }
}

const WALL: Timestamp = Timestamp(1_790_000_100_000);

#[test]
fn local_read_scope_labels_channels_and_categories_and_clears_only_local_markers() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    let development = host.cl_named(lab, "Desarrollo");
    host.store.provider.as_mut().unwrap().capabilities = Default::default();

    host.cl_right_click(&format!("channel:{core}"));
    assert_eq!(
        host.cl_access(&format!("cl:read:{core}")).name,
        "Marcar como leído (solo en RUNE)"
    );
    host.key(Key::Escape, false, false);
    host.cl_right_click(&format!("category:{development}"));
    assert_eq!(
        host.cl_access(&format!("cl:read:{development}")).name,
        "Marcar como leído (solo en RUNE)"
    );
    host.key(Key::Escape, false, false);

    let taller = host.server_id_for_cl("Taller de Rust");
    let general = host.cl_named(taller, "general");
    assert!(host.store.read(general).unread());
    host.sent.clear();
    host.cl_choose(&format!("cl:read:{general}"));
    assert!(!host.store.read(general).unread());
    assert!(host.sent.is_empty(), "no unsupported read acknowledgement");
}

#[test]
fn the_gear_is_there_only_where_the_account_may_edit_and_asks_for_the_settings() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    assert_eq!(host.ui.channel(), Some(core), "the first channel opens");
    let frame = host.frame();
    let gear = Host::cl_rect(&frame, &format!("cs:open:{core}"));
    let row = Host::cl_rect(&frame, &format!("channel:{core}"));
    assert!(
        gear.x > row.x && gear.x + gear.width <= row.x + row.width,
        "inside its row: {gear:?} {row:?}"
    );
    let center = Point::new(gear.x + gear.width / 2.0, gear.y + gear.height / 2.0);
    host.tree.handle_event(UiEvent::PointerDown(center));
    let actions = host.tree.handle_event(UiEvent::PointerUp(center));
    assert_eq!(
        actions,
        vec![UiAction::Clicked {
            id: format!("cs:open:{core}")
        }],
        "the gear asks for the channel settings, not to open the channel"
    );
    // Every channel of an owned server has one; none of a server of others.
    let gears = host
        .cl_buttons()
        .iter()
        .filter(|id| id.starts_with("cs:open:"))
        .count();
    assert_eq!(gears, 5, "three text channels, a voice one and a forum");
    host.cl_server("Taller de Rust");
    assert!(!host
        .cl_buttons()
        .iter()
        .any(|id| id.starts_with("cs:open:")));
}

#[test]
fn the_plus_of_a_category_makes_a_channel_in_it_only_for_whoever_may() {
    let mut host = Host::new();
    let taller = host.server_id_for_cl("Taller de Rust");
    assert!(!host
        .cl_buttons()
        .iter()
        .any(|id| id.starts_with(&format!("newchan:{taller}:"))));
    let lab = host.cl_server("Laboratorio limn");
    let development = host.cl_named(lab, "Desarrollo");
    host.click(&format!("newchan:{lab}:{development}"));
    assert!(matches!(
        &host.ui.server_menu.dialog,
        Some(Dialog::Channel(form)) if form.parent == Some(development) && form.server == lab
    ));
}

impl Host {
    fn server_id_for_cl(&self, name: &str) -> ServerId {
        self.store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .map(|server| server.id)
            .unwrap()
    }
}

#[test]
fn a_channel_menu_offers_what_the_owner_may_and_says_why_not_elsewhere() {
    let mut host = Host::new();
    // «Copiar ID» is offered in developer mode.
    host.ui.prefs.developer = true;
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    host.cl_right_click(&format!("channel:{core}"));
    assert!(matches!(
        host.ui.channel_list.menu,
        Some((Target::Channel(channel), _)) if channel == core
    ));
    let buttons = host.cl_buttons();
    for id in [
        format!("cs:open:{core}"),
        format!("cl:duplicate:{core}"),
        format!("cl:create:{core}"),
        format!("cl:delete:{core}"),
        format!("cl:copy-id:{core}"),
        format!("{}mute:{core}", ids::SUBMENU),
        format!("{}notify:{core}", ids::SUBMENU),
        format!("{}move:{core}", ids::SUBMENU),
    ] {
        assert!(buttons.contains(&id), "{id} can be chosen: {buttons:?}");
    }
    let read = host.cl_access(&format!("cl:read:{core}"));
    assert!(read.disabled && read.description.as_deref() == Some("No hay nada sin leer"));
    // It opens where the click was, over the column.
    let menu = Host::cl_rect(&host.frame(), &format!("cl:copy-id:{core}"));
    assert!(menu.x < rune_components::rail::WIDTH + rune_components::sidebar::WIDTH);
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_list.menu.is_none(), "Escape closes it");

    let taller = host.cl_server("Taller de Rust");
    let general = host.cl_named(taller, "general");
    host.cl_right_click(&format!("channel:{general}"));
    let buttons = host.cl_buttons();
    assert!(
        buttons.contains(&format!("cl:read:{general}")),
        "it has news"
    );
    assert!(buttons.contains(&format!("cl:copy-id:{general}")));
    for (id, why) in [
        (
            format!("cs:open:{general}"),
            "No tenés permiso para gestionar el canal en #general",
        ),
        (
            format!("cl:duplicate:{general}"),
            "No tenés permiso para gestionar canales en Taller de Rust",
        ),
        (
            format!("cl:delete:{general}"),
            "No tenés permiso para gestionar el canal en #general",
        ),
        (
            format!("{}move:{general}", ids::SUBMENU),
            "No tenés permiso para ordenar los canales en Taller de Rust",
        ),
    ] {
        assert!(!buttons.contains(&id), "{id} is off");
        let node = host.cl_access(&id);
        assert!(node.disabled, "{id}");
        assert_eq!(node.description.as_deref(), Some(why), "{id}");
    }
    // Deleting anyway, from elsewhere, says why and asks nothing.
    host.cl_choose(&format!("cl:delete:{general}"));
    assert!(host.ui.channel_list.dialog.is_none() && !host.ui.notices.is_empty());
}

#[test]
fn the_menus_of_a_category_and_of_the_list_have_their_own_items() {
    let mut host = Host::new();
    // «Copiar ID» is offered in developer mode.
    host.ui.prefs.developer = true;
    let lab = host.cl_server("Laboratorio limn");
    let development = host.cl_named(lab, "Desarrollo");
    host.cl_right_click(&format!("category:{development}"));
    assert!(matches!(
        host.ui.channel_list.menu,
        Some((Target::Category(category), _)) if category == development
    ));
    let said = host.cl_said();
    for label in [
        "Minimizar categoría",
        "Minimizar todas las categorías",
        "Silenciar categoría",
        "Editar categoría",
        "Eliminar categoría",
        "Copiar ID de la categoría",
    ] {
        assert!(said.contains(label), "{label} in {said}");
    }
    host.click(&format!("cl:collapse:{development}"));
    assert!(host.ui.collapsed.contains(&development));
    // The «+» opens the category's menu too.
    host.cl_right_click(&format!("newchan:{lab}:{development}"));
    assert!(matches!(
        host.ui.channel_list.menu,
        Some((Target::Category(_), _))
    ));
    host.key(Key::Escape, false, false);
    // Below the last channel, the list's own menu.
    let frame = host.frame();
    let list = Host::cl_rect(&frame, "channels");
    host.send(UiEvent::ContextMenu {
        position: Point::new(list.x + 40.0, list.y + list.height - 12.0),
    });
    assert_eq!(
        host.ui.channel_list.menu.map(|(target, _)| target),
        Some(Target::List(lab))
    );
    let buttons = host.cl_buttons();
    for id in [
        "cl:hide-muted".to_owned(),
        format!("newchan:{lab}"),
        format!("newcat:{lab}"),
    ] {
        assert!(buttons.contains(&id), "{id}: {buttons:?}");
    }
    host.click(&format!("newcat:{lab}"));
    assert!(host.ui.channel_list.menu.is_none());
    assert!(matches!(
        host.ui.server_menu.dialog,
        Some(Dialog::Channel(_))
    ));
}

#[test]
fn muting_a_channel_for_an_hour_dims_it_hides_its_mark_and_is_kept() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    let taller = host.server_id_for_cl("Taller de Rust");
    let general = host.cl_named(taller, "general");
    let channel = host.store.channel(general).unwrap().clone();
    assert_eq!(
        list::marks(&host.ui, &host.store, &channel, WALL),
        (true, 0, false)
    );
    let waiting = list::server_unread(&host.ui, &host.store, taller, WALL);
    assert!(waiting.0);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("cl:mute:{general}:1h"),
        },
        host.now,
    );
    assert!(host.cl_saved(&intents, list::NOTIFY_SETTING));
    let until = host.ui.channel_list.notify[&general].muted_until.unwrap();
    assert!(until != crate::server_menu::FOREVER && until > WALL);
    assert_eq!(
        list::marks(&host.ui, &host.store, &channel, WALL),
        (false, 0, true),
        "no mark of what is new"
    );
    // Drawn dim.
    let look = host.ui.look();
    let ink = host
        .frame()
        .commands
        .iter()
        .find_map(|command| match command {
            limn::DrawCommand::Text { text, color, .. } if text == "general" => Some(*color),
            _ => None,
        })
        .unwrap();
    assert_eq!(ink, Look::alpha(look.faint, 150));
    // Muted, the menu turns it back on and says until when.
    host.cl_right_click(&format!("channel:{general}"));
    let unmute = host.cl_access(&format!("cl:unmute:{general}"));
    assert_eq!(unmute.name, "Desactivar silencio del canal");
    assert!(host.cl_said().contains("Silenciado hasta:"));
    // Kept, and taken back by the next run.
    let kept = host.ui.settings_to_keep();
    let mut next = Ui::default();
    next.restore(|key| {
        kept.iter()
            .find(|(kept, _)| *kept == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(next.channel_list.notify, host.ui.channel_list.notify);
    // A mute that ended marks again.
    let after = Timestamp(until.0 + 1);
    assert_eq!(
        list::marks(&host.ui, &host.store, &channel, after),
        (true, 0, false)
    );
    host.cl_choose(&format!("cl:unmute:{general}"));
    assert!(host.ui.channel_list.notify.is_empty());
}

#[test]
fn a_channel_that_marks_nothing_marks_nothing_even_on_the_rail() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    let taller = host.server_id_for_cl("Taller de Rust");
    let tree = host.store.channel_tree(taller);
    let busy: Vec<ChannelId> = tree
        .iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| host.store.read(channel.id).unread())
        .map(|channel| channel.id)
        .collect();
    assert!(!busy.is_empty());
    for channel in &busy {
        host.cl_choose(&format!("cl:notify:{channel}:nothing"));
    }
    assert_eq!(
        list::server_unread(&host.ui, &host.store, taller, WALL),
        (false, 0)
    );
    // Back to the category's level, it marks again.
    host.cl_choose(&format!("cl:notify:{}:inherit", busy[0]));
    assert!(list::server_unread(&host.ui, &host.store, taller, WALL).0);
    // A category's level reaches the channels that take it.
    let category = host.store.channel(busy[0]).unwrap().parent.unwrap();
    host.cl_choose(&format!("cl:notify:{category}:mentions"));
    let channel = host.store.channel(busy[0]).unwrap().clone();
    assert_eq!(
        list::effective(&host.ui, &host.store, &channel, WALL).0,
        NotifyLevel::Mentions
    );
    assert!(!list::marks(&host.ui, &host.store, &channel, WALL).0);
}

#[test]
fn marking_read_from_the_menu_tells_the_provider() {
    let mut host = Host::new();
    let taller = host.server_id_for_cl("Taller de Rust");
    let general = host.cl_named(taller, "general");
    host.cl_right_click(&format!("channel:{general}"));
    host.click(&format!("cl:read:{general}"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::MarkRead { channel, .. }) if *channel == general
    ));
    assert!(!host.store.read(general).unread());
    assert!(
        host.ui.channel_list.menu.is_none(),
        "choosing closes the menu"
    );
    // A category marks read all of its channels.
    let ayuda = host.cl_named(taller, "ayuda");
    let category = host.store.channel(ayuda).unwrap().parent.unwrap();
    host.cl_choose(&format!("cl:read:{category}"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::MarkRead { channel, .. }) if *channel == ayuda
    ));
}

#[test]
fn a_copy_of_a_channel_gets_its_permissions_and_settings_once_it_is_made() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    let moderation = host.store.server(lab).unwrap().assignable_roles()[0].id;
    let overwrite = Overwrite {
        target: OverwriteTarget::Role(moderation),
        allow: Permissions::MANAGE_MESSAGES,
        deny: Permissions::NONE,
    };
    for command in [
        Command::SetOverwrite {
            channel: core,
            overwrite,
        },
        Command::EditChannel {
            channel: core,
            edit: ChannelEdit {
                topic: Some(Some("El núcleo de limn.".into())),
                slowmode_secs: Some(5),
                ..ChannelEdit::default()
            },
        },
    ] {
        for event in host.provider.manage(command) {
            host.event(event);
        }
    }
    host.cl_right_click(&format!("channel:{core}"));
    host.click(&format!("cl:duplicate:{core}"));
    assert!(matches!(
        &host.ui.channel_list.dialog,
        Some(ListDialog::Duplicate { channel, name }) if *channel == core && name == "limn-core"
    ));
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(ids::DUPLICATE_NAME));
    assert_eq!(
        host.tree.input_value(ids::DUPLICATE_NAME),
        Some("limn-core")
    );
    let said = host.cl_said();
    assert!(
        said.contains("Duplicar canal") && said.contains("en Desarrollo"),
        "{said}"
    );
    host.tree.set_input_value(ids::DUPLICATE_NAME, "");
    host.type_text("Núcleo Dos");
    let from = host.sent.len();
    host.key(Key::Enter, false, false);
    let parent = host.store.channel(core).unwrap().parent;
    assert!(
        matches!(
            &host.sent[from..],
            [Command::CreateChannel { kind: ChannelKind::Text, name, parent: made_in, private: false, server, .. }]
                if name == "núcleo-dos" && *made_in == parent && *server == lab
        ),
        "only the channel, before it is made: {:?}",
        &host.sent[from..]
    );
    assert!(host.ui.channel_list.dialog.is_none());
    host.cl_answer(from);
    let copy = host.ui.channel().unwrap();
    assert_ne!(copy, core, "the copy opens");
    assert!(host.sent.contains(&Command::SetOverwrite {
        channel: copy,
        overwrite
    }));
    let made = host.store.channel(copy).unwrap();
    assert_eq!(made.name, "núcleo-dos");
    assert!(made.overwrites.contains(&overwrite));
    assert_eq!(made.topic.as_deref(), Some("El núcleo de limn."));
    assert_eq!(made.slowmode_secs, 5);
    assert!(host.ui.notices.is_empty(), "{:?}", host.ui.notices);
}

#[test]
fn deleting_asks_first_and_enter_does_not_delete() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let parts = host.cl_named(lab, "componentes");
    host.cl_right_click(&format!("channel:{parts}"));
    host.click(&format!("cl:delete:{parts}"));
    assert_eq!(host.ui.channel_list.dialog, Some(ListDialog::Delete(parts)));
    assert!(host
        .cl_said()
        .contains("¿Seguro que querés eliminar #componentes?"));
    let from = host.sent.len();
    host.key(Key::Enter, false, false);
    assert_eq!(
        host.sent.len(),
        from,
        "Enter cancels: {:?}",
        &host.sent[from..]
    );
    assert!(host.ui.channel_list.dialog.is_none());
    host.cl_choose(&format!("cl:delete:{parts}"));
    host.click(ids::DELETE_CONFIRM);
    assert_eq!(
        host.sent.last(),
        Some(&Command::DeleteChannel { channel: parts })
    );
    host.cl_answer(from);
    assert!(host.store.channel(parts).is_none());
}

#[test]
fn dragging_a_channel_to_another_category_moves_it_there() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let speed = host.cl_named(lab, "rendimiento");
    let voice = host.cl_named(lab, "Voz");
    let from = host.sent.len();
    // While it is dragged, the voice channel it cannot land on is dimmed.
    let frame = host.frame();
    let start = Host::cl_rect(&frame, &format!("channel:{speed}"));
    let start = Point::new(start.x + 24.0, start.y + start.height / 2.0);
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(start.x, start.y + 8.0)));
    assert_eq!(
        host.ui.channel_list.drag.as_ref().map(|drag| drag.source),
        Some(speed)
    );
    let meeting = host.cl_named(lab, "Reunión semanal");
    let over_voice = Host::cl_rect(&host.frame(), &format!("voice:{meeting}"));
    host.send(UiEvent::PointerMove(Point::new(
        over_voice.x + 24.0,
        over_voice.y + over_voice.height / 2.0,
    )));
    assert_eq!(
        host.ui.channel_list.drag.as_ref().unwrap().over,
        None,
        "text does not land among voice channels"
    );
    let heading = Host::cl_rect(&host.frame(), &format!("category:{voice}"));
    let end = Point::new(heading.x + 24.0, heading.y + heading.height / 2.0);
    host.send(UiEvent::PointerMove(end));
    assert_eq!(
        host.ui.channel_list.drag.as_ref().unwrap().over,
        Some(format!("category:{voice}"))
    );
    // The line where it lands, under the heading; the voice channel dimmed.
    let look = host.ui.look();
    let frame = host.frame();
    let line = frame
        .commands
        .iter()
        .find_map(|command| match command {
            limn::DrawCommand::FillRect { rect, color, .. }
                if *color == look.online && rect.height == 4.0 =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("an insertion line");
    assert!(
        (line.y + line.height - (heading.y + heading.height)).abs() < 1.0,
        "{line:?} under {heading:?}"
    );
    let dimmed = frame.commands.iter().any(|command| {
        matches!(command, limn::DrawCommand::Text { text, color, .. }
            if text == "Reunión semanal" && *color == Look::alpha(look.faint, 70))
    });
    assert!(dimmed, "where it cannot land is dimmed");
    host.send(UiEvent::PointerUp(end));
    assert!(host.ui.channel_list.drag.is_none());
    // At once: it goes first, and the voice channel after it.
    assert_eq!(
        &host.sent[from..],
        &[Command::ReorderChannels {
            server: lab,
            moves: vec![
                ChannelMove {
                    channel: speed,
                    parent: Some(voice),
                    position: 0,
                    lock_permissions: false,
                },
                ChannelMove {
                    channel: meeting,
                    parent: Some(voice),
                    position: 1,
                    lock_permissions: false,
                },
            ],
        }]
    );
    host.cl_answer(from);
    assert_eq!(host.store.channel(speed).unwrap().parent, Some(voice));
    let in_voice: Vec<String> = host
        .store
        .channel_tree(lab)
        .into_iter()
        .find(|(category, _)| category.map(|category| category.id) == Some(voice))
        .map(|(_, channels)| {
            channels
                .iter()
                .map(|channel| channel.name.clone())
                .collect()
        })
        .unwrap();
    assert_eq!(in_voice, ["rendimiento", "Reunión semanal"]);
}

#[test]
fn dragging_reorders_channels_and_categories_and_a_plain_click_still_opens() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    let parts = host.cl_named(lab, "componentes");
    let speed = host.cl_named(lab, "rendimiento");
    let development = host.cl_named(lab, "Desarrollo");
    let step = |channel, parent, position| ChannelMove {
        channel,
        parent,
        position,
        lock_permissions: false,
    };
    let from = host.sent.len();
    host.cl_drag(&format!("channel:{core}"), &format!("channel:{speed}"));
    // From above, it lands below the one it is dropped on.
    assert_eq!(
        &host.sent[from..],
        &[Command::ReorderChannels {
            server: lab,
            moves: vec![
                step(parts, Some(development), 0),
                step(speed, Some(development), 1),
                step(core, Some(development), 2),
            ],
        }]
    );
    host.cl_answer(from);
    let order: Vec<String> = host
        .store
        .channel_tree(lab)
        .into_iter()
        .find(|(category, _)| category.map(|category| category.id) == Some(development))
        .map(|(_, channels)| {
            channels
                .iter()
                .map(|channel| channel.name.clone())
                .collect()
        })
        .unwrap();
    assert_eq!(order, ["componentes", "rendimiento", "limn-core"]);
    // Categories among categories.
    let voice = host.cl_named(lab, "Voz");
    let from = host.sent.len();
    host.cl_drag(
        &format!("category:{voice}"),
        &format!("category:{development}"),
    );
    // Only the categories: their channels stay as they are.
    assert_eq!(
        &host.sent[from..],
        &[Command::ReorderChannels {
            server: lab,
            moves: vec![step(voice, None, 0), step(development, None, 1)],
        }]
    );
    host.cl_answer(from);
    assert_eq!(
        host.store.channel_tree(lab)[0]
            .0
            .map(|category| category.id),
        Some(voice)
    );
    // A press without moving is a click.
    host.click(&format!("channel:{speed}"));
    assert_eq!(host.ui.channel(), Some(speed));
    // Nothing moves in a server of others.
    let taller = host.cl_server("Taller de Rust");
    let general = host.cl_named(taller, "general");
    let ayuda = host.cl_named(taller, "ayuda");
    let from = host.sent.len();
    host.cl_drag(&format!("channel:{general}"), &format!("channel:{ayuda}"));
    assert!(!host.sent[from..].iter().any(|command| matches!(
        command,
        Command::EditChannel { .. } | Command::ReorderChannels { .. }
    )));
}

#[test]
fn a_channel_the_account_cannot_see_keeps_its_place_when_others_move_around_it() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let development = host.cl_named(lab, "Desarrollo");
    let core = host.cl_named(lab, "limn-core");
    let parts = host.cl_named(lab, "componentes");
    let speed = host.cl_named(lab, "rendimiento");
    // «componentes», between the two, is hidden from the account.
    let mut hidden = host.store.channel(parts).unwrap().clone();
    hidden.permissions = hidden.permissions.without(Permissions::VIEW);
    host.event(Event::ChannelChanged(hidden));
    let positions = |host: &Host| {
        [core, parts, speed].map(|channel| host.store.channel(channel).unwrap().position)
    };
    let [at_core, at_parts, at_speed] = positions(&host);
    assert!(
        at_core < at_parts && at_parts < at_speed,
        "{:?}",
        positions(&host)
    );
    let from = host.sent.len();
    host.cl_drag(&format!("channel:{core}"), &format!("channel:{speed}"));
    let Some(Command::ReorderChannels { moves, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", &host.sent[from..])
    };
    assert!(
        moves.iter().all(|step| step.channel != parts),
        "the hidden one is never sent: {moves:?}"
    );
    host.cl_answer(from);
    let [at_core, at_parts, at_speed] = positions(&host);
    assert!(
        at_speed < at_parts && at_parts < at_core,
        "it lands after «rendimiento», and «componentes» stays between: {:?}",
        positions(&host)
    );
    let shown: Vec<String> = host
        .store
        .channel_tree(lab)
        .into_iter()
        .find(|(category, _)| category.map(|category| category.id) == Some(development))
        .map(|(_, channels)| {
            channels
                .iter()
                .map(|channel| channel.name.clone())
                .collect()
        })
        .unwrap();
    assert_eq!(shown, ["rendimiento", "limn-core"]);
}

#[test]
fn no_batch_goes_when_a_hidden_channel_leaves_no_position_after_it() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let development = host.cl_named(lab, "Desarrollo");
    let parts = host.cl_named(lab, "componentes");
    let meeting = host.cl_named(lab, "Reunión semanal");
    // Hidden from the account, at the very last position there is.
    let mut hidden = host.store.channel(parts).unwrap().clone();
    hidden.permissions = hidden.permissions.without(Permissions::VIEW);
    hidden.position = i32::MAX;
    host.event(Event::ChannelChanged(hidden));
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{meeting}:to:{development}"));
    assert!(host.sent[from..].is_empty(), "{:?}", &host.sent[from..]);
}

#[test]
fn a_move_writes_only_the_groups_it_changes_whatever_gaps_the_others_have() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let development = host.cl_named(lab, "Desarrollo");
    let core = host.cl_named(lab, "limn-core");
    let parts = host.cl_named(lab, "componentes");
    let speed = host.cl_named(lab, "rendimiento");
    let voice = host.cl_named(lab, "Voz");
    let meeting = host.cl_named(lab, "Reunión semanal");
    // Positions as Discord leaves them after deletions: with gaps, in the
    // categories and in the other category's channels.
    for (channel, position) in [(voice, 7), (meeting, 5), (speed, 9)] {
        let mut gapped = host.store.channel(channel).unwrap().clone();
        gapped.position = position;
        host.event(Event::ChannelChanged(gapped));
    }
    let step = |channel, position| ChannelMove {
        channel,
        parent: Some(development),
        position,
        lock_permissions: false,
    };
    let from = host.sent.len();
    host.cl_drag(&format!("channel:{parts}"), &format!("channel:{core}"));
    assert_eq!(
        &host.sent[from..],
        &[Command::ReorderChannels {
            server: lab,
            moves: vec![step(parts, 0), step(core, 1), step(speed, 2)],
        }]
    );
    // Into another category, the one it leaves keeps its gaps; the one it
    // goes into is numbered from the start, as the official client does.
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{core}:to:{voice}"));
    let into = |channel, position| ChannelMove {
        channel,
        parent: Some(voice),
        position,
        lock_permissions: false,
    };
    assert_eq!(
        &host.sent[from..],
        &[Command::ReorderChannels {
            server: lab,
            moves: vec![into(meeting, 0), into(core, 1)],
        }]
    );
}

#[test]
fn the_question_before_syncing_moves_what_is_there_when_answered_and_goes_with_its_category() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let development = host.cl_named(lab, "Desarrollo");
    let speed = host.cl_named(lab, "rendimiento");
    let voice = host.cl_named(lab, "Voz");
    let meeting = host.cl_named(lab, "Reunión semanal");
    let overwrite = Overwrite {
        target: OverwriteTarget::Role(rune_model::RoleId(lab.0)),
        allow: Permissions::NONE,
        deny: Permissions::VIEW,
    };
    for event in host.provider.manage(Command::SetOverwrite {
        channel: voice,
        overwrite,
    }) {
        host.event(event);
    }
    // Dropped first in «Voz», the meeting room would go after it...
    host.cl_drag(&format!("channel:{speed}"), &format!("category:{voice}"));
    assert!(matches!(
        host.ui.channel_list.dialog,
        Some(ListDialog::Sync { channel, category, index: 0 }) if channel == speed && category == voice
    ));
    // ...but someone else takes it to «Desarrollo» before the answer, and
    // the answer leaves it there.
    let mut taken = host.store.channel(meeting).unwrap().clone();
    taken.parent = Some(development);
    taken.position = 3;
    host.event(Event::ChannelChanged(taken));
    let from = host.sent.len();
    host.click(ids::SYNC_KEEP);
    assert_eq!(
        &host.sent[from..],
        &[Command::ReorderChannels {
            server: lab,
            moves: vec![ChannelMove {
                channel: speed,
                parent: Some(voice),
                position: 0,
                lock_permissions: false,
            }],
        }]
    );
    // Asked again, the question goes when the category it is about does:
    // nothing is left open where it cannot be seen or answered.
    host.cl_drag(&format!("channel:{speed}"), &format!("category:{voice}"));
    assert!(host.ui.channel_list.dialog.is_some());
    host.event(Event::ChannelRemoved { channel: voice });
    assert!(host.ui.channel_list.dialog.is_none());
}

#[test]
fn moving_into_another_category_asks_whether_it_takes_its_permissions() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let speed = host.cl_named(lab, "rendimiento");
    let voice = host.cl_named(lab, "Voz");
    // The voice category hides itself from @everyone; «rendimiento» has
    // its category's permissions (none).
    let overwrite = Overwrite {
        target: OverwriteTarget::Role(rune_model::RoleId(lab.0)),
        allow: Permissions::NONE,
        deny: Permissions::VIEW,
    };
    for event in host.provider.manage(Command::SetOverwrite {
        channel: voice,
        overwrite,
    }) {
        host.event(event);
    }
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{speed}:to:{voice}"));
    assert_eq!(host.sent.len(), from, "nothing moves before the answer");
    assert!(
        matches!(host.ui.channel_list.dialog, Some(ListDialog::Sync { channel, .. }) if channel == speed)
    );
    let said = host.cl_said();
    assert!(said.contains("#rendimiento pasa a Voz"), "{said}");
    // Cancelled, nothing moves.
    host.click(ids::SYNC_CANCEL);
    assert_eq!(host.sent.len(), from);
    assert!(host.ui.channel_list.dialog.is_none());
    // Kept, it moves with its own.
    host.cl_choose(&format!("cl:move:{speed}:to:{voice}"));
    host.click(ids::SYNC_KEEP);
    let Some(Command::ReorderChannels { moves, .. }) = host.sent.last() else {
        panic!("{:?}", &host.sent[from..])
    };
    assert!(moves
        .iter()
        .any(|step| step.channel == speed && step.parent == Some(voice) && !step.lock_permissions));
    // Taken, it moves with the category's.
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{speed}:to:{voice}"));
    host.click(ids::SYNC_CONFIRM);
    let Some(Command::ReorderChannels { moves, .. }) = host.sent.last() else {
        panic!("{:?}", &host.sent[from..])
    };
    assert!(moves
        .iter()
        .any(|step| step.channel == speed && step.lock_permissions));
    assert!(
        moves
            .iter()
            .all(|step| step.channel == speed || !step.lock_permissions),
        "only the one moved in takes them"
    );
    host.cl_answer(from);
    assert_eq!(host.store.channel(speed).unwrap().overwrites, [overwrite]);
    // Within its own category nothing is asked.
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{speed}:first"));
    assert!(host.ui.channel_list.dialog.is_none());
    assert!(matches!(
        host.sent[from..],
        [Command::ReorderChannels { .. }]
    ));
}

#[test]
fn moving_from_the_menu_is_the_way_without_a_pointer() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let parts = host.cl_named(lab, "componentes");
    let voice = host.cl_named(lab, "Voz");
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{parts}:to:{voice}"));
    host.cl_answer(from);
    let from = host.sent.len();
    host.cl_choose(&format!("cl:move:{parts}:first"));
    host.cl_answer(from);
    assert_eq!(host.store.channel(parts).unwrap().parent, Some(voice));
    assert_eq!(host.store.channel(parts).unwrap().position, 0);
    // Its submenu opens on hover; where it already is, it is off, and why.
    host.cl_right_click(&format!("channel:{parts}"));
    let trigger = Host::cl_rect(&host.frame(), &format!("{}move:{parts}", ids::SUBMENU));
    host.send(UiEvent::PointerMove(Point::new(
        trigger.x + 20.0,
        trigger.y + trigger.height / 2.0,
    )));
    let here = host.cl_access(&format!("cl:move:{parts}:to:{voice}"));
    assert!(here.disabled && here.description.as_deref() == Some("Ya está acá"));
    let first = host.cl_access(&format!("cl:move:{parts}:first"));
    assert!(first.disabled, "it is the first already");
    let buttons = host.cl_buttons();
    assert!(buttons.contains(&format!("cl:move:{parts}:to:none")));
    assert!(buttons.contains(&format!("cl:move:{parts}:last")));
}

#[test]
fn pins_header_revalidates_support_and_history_access_before_opening() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.store.provider.as_mut().unwrap().capabilities.pins = false;
    let button = host.cl_access(ids::PINS);
    assert!(button.disabled, "the unsupported header action is disabled");
    assert!(button.description.as_deref().unwrap().contains("proveedor"));
    let from = host.sent.len();
    host.cl_choose(ids::PINS);
    assert_eq!(host.sent.len(), from, "a stale click cannot start a load");
    assert!(host.ui.channel_list.pins.is_none());

    host.store.provider.as_mut().unwrap().capabilities.pins = true;
    let original = host.store.channel(channel).unwrap().clone();
    let mut no_history = original.clone();
    no_history.permissions = Permissions::VIEW;
    host.store
        .apply(Event::ChannelChanged(no_history), host.now);
    assert!(host.cl_access(ids::PINS).disabled);
    host.cl_choose(ids::PINS);
    assert_eq!(
        host.sent.len(),
        from,
        "no false empty list without history access"
    );
    host.store.apply(Event::ChannelChanged(original), host.now);
    host.cl_choose(ids::PINS);
    assert!(
        matches!(host.sent.last(), Some(Command::LoadPins { channel: loaded, .. }) if *loaded == channel)
    );
}

#[test]
fn pins_load_failure_replaces_loading_and_retry_uses_a_new_request() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.ui
        .set_draft(channel, Some("borrador conservado".into()));
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
        panic!("the header asks for pins");
    };
    host.event(Event::Failed {
        failure: rune_provider::Failure::Pins { request, channel },
        reason: "No se pudo conectar".into(),
        ambiguous: false,
    });
    assert!(host
        .cl_said()
        .contains("No se pudieron cargar los mensajes fijados"));
    assert!(host.cl_buttons().contains(&"pins:retry".to_owned()));
    assert!(!host.cl_said().contains("Todavía no hay mensajes fijados"));
    host.cl_choose("pins:retry");
    let Some(Command::LoadPins { request: retry, .. }) = host.sent.last().cloned() else {
        panic!("retry asks explicitly");
    };
    assert!(retry > request);
    host.event(Event::Failed {
        failure: rune_provider::Failure::Pins { request, channel },
        reason: "Fallo viejo".into(),
        ambiguous: false,
    });
    assert!(
        !host.cl_buttons().contains(&"pins:retry".to_owned()),
        "an old failure cannot end the new load"
    );
    assert!(
        host.ui.notices.is_empty(),
        "the popout owns load failures, including stale ones"
    );
    host.event(Event::PinsLoaded {
        request: retry,
        channel,
        messages: Vec::new(),
    });
    assert!(host.cl_said().contains("Todavía no hay mensajes fijados"));
    assert_eq!(host.ui.drafts.get(&channel).unwrap(), "borrador conservado");
}

#[test]
fn pins_reopen_waits_for_its_request_and_revocation_hides_old_cards_and_actions() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request: old, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    host.cl_choose(ids::PINS);
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins {
        request: current, ..
    }) = host.sent.last().cloned()
    else {
        panic!()
    };
    host.event(Event::Failed {
        failure: rune_provider::Failure::Pins {
            request: old,
            channel,
        },
        reason: "Una carga ya cerrada falló".into(),
        ambiguous: false,
    });
    assert!(host.ui.notices.is_empty());
    assert!(!host.cl_buttons().contains(&"pins:retry".to_owned()));
    let mut pinned = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    pinned.id = MessageId(9_000_000_000_000_201);
    pinned.content = "Tarjeta fijada de la solicitud vieja".into();
    pinned.pinned = true;
    host.event(Event::PinsLoaded {
        request: old,
        channel,
        messages: vec![pinned.clone()],
    });
    assert!(!host
        .cl_buttons()
        .contains(&format!("{}{id}", ids::PIN_JUMP, id = pinned.id)));
    assert!(
        !host.cl_said().contains(&pinned.content),
        "an older completion cannot end loading"
    );
    pinned.content = "Tarjeta fijada de la solicitud vigente".into();
    host.event(Event::PinsLoaded {
        request: current,
        channel,
        messages: vec![pinned.clone()],
    });
    assert!(host.cl_said().contains(&pinned.content));
    let mut stale = pinned.clone();
    stale.content = "Tarjeta reemplazada por una solicitud vieja".into();
    host.event(Event::PinsLoaded {
        request: old,
        channel,
        messages: vec![stale.clone()],
    });
    assert!(host.cl_said().contains(&pinned.content));
    assert!(!host.cl_said().contains(&stale.content));
    let mut revoked = host.store.channel(channel).unwrap().clone();
    revoked.permissions = Permissions::VIEW;
    host.event(Event::ChannelChanged(revoked));
    assert!(host.ui.channel_list.pins.is_none());
    assert!(!host.cl_said().contains(&pinned.content));
    let from = host.sent.len();
    host.cl_choose(&format!("{}{id}", ids::PIN_REMOVE, id = pinned.id));
    host.cl_choose(&format!("{}{id}", ids::PIN_JUMP, id = pinned.id));
    host.cl_choose("pins:retry");
    assert_eq!(
        host.sent.len(),
        from,
        "stale popout actions are inert after revocation"
    );
}

#[test]
fn closing_pending_pins_cancels_the_exact_request_and_discards_its_result() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    host.cl_choose(ids::PINS);
    assert_eq!(host.sent.last(), Some(&Command::Cancel(request)));
    host.event(Event::PinsLoaded {
        request,
        channel,
        messages: Vec::new(),
    });
    assert!(
        host.store.pins(channel).is_none(),
        "the closed read cannot become a verified empty list"
    );
}

#[test]
fn pins_merge_live_changes_and_overflow_offers_only_explicit_retry() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    let mut pinned = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    pinned.id = MessageId(9_000_000_000_000_301);
    pinned.content = "Fijado recibido durante la lectura".into();
    pinned.pinned = true;
    host.event(Event::MessageUpdated(pinned.clone()));
    assert!(host.store.pins(channel).is_none());
    host.event(Event::PinsLoaded {
        request,
        channel,
        messages: Vec::new(),
    });
    assert!(host.cl_said().contains(&pinned.content));
    host.cl_choose(ids::PINS);
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    host.event(Event::MessageUpdated(rune_model::Message {
        pinned: false,
        ..pinned.clone()
    }));
    host.event(Event::PinsLoaded {
        request,
        channel,
        messages: vec![pinned.clone()],
    });
    assert!(host.cl_said().contains("Todavía no hay mensajes fijados"));
    host.cl_choose(ids::PINS);
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    for offset in 0..257 {
        host.event(Event::MessageUpdated(rune_model::Message {
            id: MessageId(pinned.id.0 + offset),
            ..pinned.clone()
        }));
    }
    let before = host.sent.len();
    host.event(Event::PinsLoaded {
        request,
        channel,
        messages: Vec::new(),
    });
    assert!(host.cl_buttons().contains(&ids::PINS_RETRY.to_owned()));
    assert_eq!(
        host.sent.len(),
        before,
        "overflow never retries automatically"
    );
    host.cl_choose(ids::PINS_RETRY);
    let Some(Command::LoadPins { request: retry, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    assert!(retry > request);
    host.event(Event::PinsLoaded {
        request: retry,
        channel,
        messages: Vec::new(),
    });
    assert!(host.cl_said().contains("Todavía no hay mensajes fijados"));
}

#[test]
fn pins_permission_revocation_cancels_a_pending_request() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    host.cl_choose(ids::PINS);
    let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
        panic!()
    };
    let mut revoked = host.store.channel(channel).unwrap().clone();
    revoked.permissions = Permissions::VIEW;
    host.event(Event::ChannelChanged(revoked));
    assert_eq!(host.sent.last(), Some(&Command::Cancel(request)));
    assert!(host.ui.channel_list.pins.is_none());
    host.event(Event::PinsLoaded {
        request,
        channel,
        messages: Vec::new(),
    });
    assert!(host.store.pins(channel).is_none());
}

#[test]
fn opening_a_channel_without_history_permission_shows_why_instead_of_loading() {
    let mut host = Host::new();
    let channel = host.channel_named("general");
    let mut restricted = host.store.channel(channel).unwrap().clone();
    restricted.permissions = Permissions::VIEW | Permissions::SEND;
    host.event(Event::ChannelChanged(restricted));

    host.open_channel("general");
    let timeline = host.store.timeline(channel);
    assert!(!timeline.is_some_and(|timeline| timeline.latest));
    let said = host.cl_said();
    assert!(
        said.contains("No podés leer el historial de esa conversación."),
        "{said}"
    );
    assert!(!said.contains("Cargando mensajes"), "{said}");
}

#[test]
fn failed_history_is_shown_inline_without_a_persistent_toast() {
    let mut host = Host::new();
    let server = host.server_id_for_cl("Laboratorio limn");
    for event in host.provider.manage(Command::CreateChannel {
        request: 900,
        server,
        kind: ChannelKind::Text,
        name: "sin-historial".into(),
        parent: None,
        private: false,
    }) {
        host.event(event);
    }
    let channel = host.channel_named("sin-historial");
    host.ui.server = Some(server);
    host.ui.channels.insert(Some(server), channel);
    host.active = false;
    let notices = host.ui.notices.len();
    host.event(Event::Failed {
        failure: rune_provider::Failure::History {
            request: 7,
            channel,
        },
        reason: "No tenés permiso para leer el historial de #general".into(),
        ambiguous: false,
    });

    assert_eq!(host.ui.notices.len(), notices);
    let said = host.cl_said();
    assert!(said.contains("No se pudo cargar el historial."), "{said}");
    assert!(
        said.contains("No tenés permiso para leer el historial de #general"),
        "{said}"
    );
    assert!(!said.contains("Cargando mensajes"), "{said}");
}

#[test]
fn a_history_failure_after_messages_loaded_keeps_its_notice() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let notices = host.ui.notices.len();

    host.event(Event::Failed {
        failure: rune_provider::Failure::History {
            request: 8,
            channel,
        },
        reason: "No se pudo cargar la página anterior".into(),
        ambiguous: false,
    });

    assert_eq!(host.ui.notices.len(), notices + 1);
}

#[test]
fn pins_are_invalidated_when_ready_replaces_the_snapshot() {
    for completed in [false, true] {
        let mut host = Host::new();
        let channel = host.open_channel("limn-core");
        let from = host.sent.len();
        host.cl_choose(ids::PINS);
        let Some(Command::LoadPins { request, .. }) = host.sent.last().cloned() else {
            panic!()
        };
        if completed {
            host.cl_answer(from);
        }
        assert_eq!(host.ui.channel_list.pins, Some(channel));
        host.event(Event::Ready(Box::new(host.provider.snapshot())));
        if !completed {
            assert_eq!(host.sent.last(), Some(&Command::Cancel(request)));
        }
        assert!(
            host.ui.channel_list.pins.is_none(),
            "replaced data cannot confirm either loading or completed pins"
        );
    }
}

#[test]
fn a_pinned_message_shows_in_the_popout_once_the_simulation_answers_and_can_be_unpinned() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    host.ui.members_open = false;
    // The message's own menu offers to pin it.
    let frame = host.frame();
    // The viewport and native scrollbar share an ID. Right-click a message
    // inside the viewport, not the reserved rail that owns the last hit region.
    let list = frame
        .regions
        .iter()
        .find(|region| region.id == timeline_id(core) && region.kind == limn::HitKind::Scroll)
        .unwrap()
        .rect;
    host.send(UiEvent::ContextMenu {
        position: Point::new(list.x + list.width / 2.0, list.y + list.height - 30.0),
    });
    let item = host
        .frame()
        .access
        .iter()
        .find(|node| node.name == "Fijar mensaje")
        .map(|node| node.id.clone())
        .expect("the owner may pin");
    let from = host.sent.len();
    host.click(&item);
    let Some(Command::PinMessage { channel, message }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent.last());
    };
    assert_eq!(channel, core);
    assert!(
        !host
            .store
            .timeline(core)
            .unwrap()
            .get(message)
            .unwrap()
            .pinned,
        "nothing changes before the provider answers"
    );
    host.cl_answer(from);
    assert!(
        host.store
            .timeline(core)
            .unwrap()
            .get(message)
            .unwrap()
            .pinned
    );
    // The header's pin opens the popout and asks for the list.
    let from = host.sent.len();
    host.click(ids::PINS);
    assert!(matches!(
        host.sent.last(),
        Some(Command::LoadPins { channel, .. }) if *channel == core
    ));
    assert!(host.store.pins(core).is_none());
    host.cl_answer(from);
    let pinned = host.store.pins(core).unwrap();
    assert_eq!(pinned[0].id, message, "the newest first");
    let buttons = host.cl_buttons();
    assert!(buttons.contains(&format!("{}{message}", ids::PIN_JUMP)));
    assert!(buttons.contains(&format!("{}{message}", ids::PIN_REMOVE)));
    assert!(host.cl_said().contains("Mensajes fijados"));
    let from = host.sent.len();
    host.cl_choose(&format!("{}{message}", ids::PIN_REMOVE));
    assert_eq!(
        host.sent.last(),
        Some(&Command::UnpinMessage {
            channel: core,
            message
        })
    );
    host.cl_answer(from);
    assert!(host
        .store
        .pins(core)
        .unwrap()
        .iter()
        .all(|pinned| pinned.id != message));
    assert_eq!(host.ui.channel_list.pins, Some(core), "the popout stays");
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_list.pins.is_none(), "Escape closes it");
    // Elsewhere, a plain member is not offered to pin.
    let taller = host.cl_server("Taller de Rust");
    let general = host.cl_named(taller, "general");
    host.click(&format!("channel:{general}"));
    assert!(!host.store.abilities(general).unwrap().pin.allowed());
    let frame = host.frame();
    let list = Host::cl_rect(&frame, &timeline_id(general));
    host.send(UiEvent::ContextMenu {
        position: Point::new(list.x + list.width / 2.0, list.y + list.height - 30.0),
    });
    assert!(!host
        .frame()
        .access
        .iter()
        .any(|node| node.name == "Fijar mensaje"));
}

#[test]
fn the_popout_says_when_nothing_is_pinned_and_goes_to_a_pinned_message() {
    let mut host = Host::new();
    let lab = host.cl_server("Laboratorio limn");
    let speed = host.cl_named(lab, "rendimiento");
    host.click(&format!("channel:{speed}"));
    let from = host.sent.len();
    host.click(ids::PINS);
    host.cl_answer(from);
    assert!(host.cl_said().contains("Todavía no hay mensajes fijados"));
    host.click(ids::PINS);
    assert!(host.ui.channel_list.pins.is_none(), "the pin closes it too");
    // An unloaded pin requests one bounded context instead of walking pages.
    let taller = host.cl_server("Taller de Rust");
    let general = host.cl_named(taller, "general");
    host.click(&format!("channel:{general}"));
    let from = host.sent.len();
    host.click(ids::PINS);
    host.cl_answer(from);
    let oldest = host.store.pins(general).unwrap().last().unwrap().id;
    let loaded = host.store.timeline(general).unwrap().oldest().unwrap();
    assert!(oldest < loaded, "the oldest pin is not on the first page");
    host.cl_choose(&format!("{}{oldest}", ids::PIN_JUMP));
    assert!(host.ui.channel_list.pins.is_none());
    let (request, channel, anchor, source) = *host
        .context_reads
        .last()
        .expect("one context was requested");
    assert_eq!(
        (channel, anchor, source),
        (general, oldest, rune_model::HistorySource::Provider)
    );
    let (messages, _) = host
        .provider
        .history(general, Some(MessageId(oldest.0 + 1)), 26);
    host.event(Event::ContextLoaded {
        request,
        channel,
        anchor,
        messages,
    });
    assert_eq!(
        host.store.timeline(general).unwrap().context_source(),
        Some(rune_model::HistorySource::Provider)
    );
    let timeline = host.store.timeline(general).unwrap();
    let index = timeline
        .messages()
        .iter()
        .position(|message| message.id == oldest)
        .expect("its page came");
    host.frame();
    let shown = host.tree.visible_rows(&timeline_id(general)).unwrap();
    let index_now = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .position(|message| message.id == oldest)
        .unwrap();
    assert!(
        shown.contains(&index_now),
        "{index}/{index_now} not in {shown:?}"
    );
}

#[test]
fn the_topic_opens_whole_and_escape_closes_it() {
    let mut host = Host::new();
    host.open_channel("general");
    host.click(ids::TOPIC);
    let said = host.cl_said();
    assert!(
        said.contains("Tema del canal") && said.contains("#general"),
        "{said}"
    );
    assert!(
        said.contains("Charla general del taller. Sé amable."),
        "{said}"
    );
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_list.topic.is_none());
    assert!(!host.cl_said().contains("Tema del canal"));
}

#[test]
fn the_slow_mode_is_said_over_the_composer() {
    let mut host = Host::new();
    host.open_channel("ayuda");
    assert!(host
        .cl_said()
        .contains("Modo lento: un mensaje cada 10 segundos"));
    host.open_channel("general");
    assert!(!host.cl_said().contains("Modo lento"));
}

#[test]
fn the_bell_sets_the_channels_level_and_mutes_it() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    let lab = host.cl_server("Laboratorio limn");
    let core = host.cl_named(lab, "limn-core");
    host.click(ids::BELL);
    assert_eq!(host.ui.channel_list.bell, Some(core));
    let inherit = host.cl_access(&format!("cl:notify:{core}:inherit"));
    assert_eq!(inherit.name, "Usar la categoría predeterminada");
    // A radio item of the menu, the chosen one.
    assert_eq!(inherit.role, limn::Role::MenuItemRadio);
    assert_eq!(inherit.toggled, Some(true));
    assert!(host.cl_said().contains("Todos los mensajes"));
    host.click(&format!("cl:notify:{core}:mentions"));
    assert!(host.ui.channel_list.bell.is_none(), "choosing closes it");
    assert_eq!(
        host.ui.channel_list.notify[&core].level,
        Some(NotifyLevel::Mentions)
    );
    host.click(ids::BELL);
    assert_eq!(
        host.cl_access(&format!("cl:notify:{core}:mentions"))
            .toggled,
        Some(true)
    );
    host.cl_choose(&format!("cl:mute:{core}:always"));
    let channel = host.store.channel(core).unwrap().clone();
    assert!(list::effective(&host.ui, &host.store, &channel, WALL).1);
    host.click(ids::BELL);
    let said = host.cl_said();
    assert!(said.contains("Silenciado hasta reactivarlo"), "{said}");
}

#[test]
fn a_folded_category_keeps_the_open_channel_and_those_with_news() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    let taller = host.server_id_for_cl("Taller de Rust");
    let general = host.cl_named(taller, "general");
    let category = host.store.channel(general).unwrap().parent.unwrap();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("category:{category}"),
        },
        host.now,
    );
    assert!(host.cl_saved(&intents, list::COLLAPSED_SETTING));
    let buttons = host.cl_buttons();
    let ayuda = host.cl_named(taller, "ayuda");
    let show = host.cl_named(taller, "mostrá-tu-proyecto");
    assert!(buttons.contains(&format!("channel:{general}")), "unread");
    assert!(buttons.contains(&format!("channel:{ayuda}")), "unread");
    assert!(
        !buttons.contains(&format!("channel:{show}")),
        "read: folded away"
    );
    // A muted one goes too.
    host.cl_choose(&format!("cl:mute:{ayuda}:always"));
    assert!(!host.cl_buttons().contains(&format!("channel:{ayuda}")));
    // Kept for the next run.
    let kept = host.ui.settings_to_keep();
    let mut next = Ui::default();
    next.restore(|key| {
        kept.iter()
            .find(|(kept, _)| *kept == key)
            .and_then(|(_, value)| value.clone())
    });
    assert!(next.collapsed.contains(&category));
}

#[test]
fn muted_channels_can_be_left_out_of_the_list() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    let taller = host.server_id_for_cl("Taller de Rust");
    let ayuda = host.cl_named(taller, "ayuda");
    host.cl_choose(&format!("cl:mute:{ayuda}:8h"));
    assert!(host.cl_buttons().contains(&format!("channel:{ayuda}")));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "cl:hide-muted".into(),
        },
        host.now,
    );
    assert!(host.cl_saved(&intents, list::HIDE_MUTED_SETTING));
    assert!(!host.cl_buttons().contains(&format!("channel:{ayuda}")));
    // Open, it stays in view.
    host.cl_choose(&format!("channel:{ayuda}"));
    assert!(host.cl_buttons().contains(&format!("channel:{ayuda}")));
}

/// «Laboratorio limn» with a second voice room, «Sala 2», and someone else
/// in «Reunión semanal»: the server, that room, «Sala 2» and who.
fn carried(host: &mut Host) -> (ServerId, ChannelId, ChannelId, UserId) {
    let lab = host.cl_server("Laboratorio limn");
    let voice = host.cl_named(lab, "Voz");
    let meeting = host.cl_named(lab, "Reunión semanal");
    for event in host.provider.manage(Command::CreateChannel {
        request: 900,
        server: lab,
        kind: ChannelKind::Voice,
        name: "Sala 2".into(),
        parent: Some(voice),
        private: false,
    }) {
        host.event(event);
    }
    let other = host.cl_named(lab, "Sala 2");
    let me = host.store.me().unwrap();
    let someone = host
        .store
        .member_groups(lab)
        .into_iter()
        .flat_map(|group| group.members)
        .find(|user| *user != me)
        .unwrap();
    let state = |user, channel| rune_model::VoiceState {
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
    };
    host.event(Event::VoiceChanged(state(someone, meeting)));
    (lab, meeting, other, someone)
}

#[test]
fn someone_dragged_onto_another_voice_channel_is_moved_there() {
    let mut host = Host::new();
    let (lab, meeting, other, someone) = carried(&mut host);
    let me = host.store.me().unwrap();
    let center = |rect: limn::Rect| Point::new(rect.x + 24.0, rect.y + rect.height / 2.0);
    let start = center(Host::cl_rect(&host.frame(), &format!("vz:user:{someone}")));
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(start.x, start.y + 8.0)));
    assert_eq!(host.ui.voice_drag.map(|drag| drag.user), Some(someone));
    // Every voice channel and the people under it take the drop.
    let frame = host.frame();
    assert!(frame
        .regions
        .iter()
        .any(|region| region.id == format!("vz:drop:{meeting}:{someone}")));
    let end = center(Host::cl_rect(&frame, &format!("vz:drop:{other}:channel")));
    host.send(UiEvent::PointerMove(end));
    assert_eq!(host.ui.voice_drag.and_then(|drag| drag.over), Some(other));
    // Where they would go is ringed.
    let look = host.ui.look();
    assert!(host.frame().commands.iter().any(|command| matches!(
        command,
        limn::DrawCommand::StrokeRect { color, .. } if *color == look.online
    )));
    let from = host.sent.len();
    host.send(UiEvent::PointerUp(end));
    assert!(host.ui.voice_drag.is_none());
    assert_eq!(
        &host.sent[from..],
        &[Command::ModerateVoice {
            server: lab,
            user: someone,
            change: rune_provider::VoiceModeration::Move(other),
        }]
    );
    // Let go over a text channel, nothing moves.
    let start = center(Host::cl_rect(&host.frame(), &format!("vz:user:{someone}")));
    let core = host.cl_named(lab, "limn-core");
    let text = center(Host::cl_rect(&host.frame(), &format!("channel:{core}")));
    let from = host.sent.len();
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(start.x, start.y + 8.0)));
    host.send(UiEvent::PointerMove(text));
    host.send(UiEvent::PointerUp(text));
    assert_eq!(host.sent.len(), from, "{:?}", &host.sent[from..]);
    // The account's own row is not picked up: it moves itself by joining.
    let mut mine = host.store.voice_state_of(someone).unwrap().clone();
    mine.user = me;
    host.event(Event::VoiceChanged(mine));
    assert!(!crate::voice_drag::movable(&host.store, me));
}

#[test]
fn someone_carried_follows_the_pointer_and_escape_or_letting_go_outside_cancels() {
    let mut host = Host::new();
    let (_, meeting, other, someone) = carried(&mut host);
    let name = host.store.display_name(someone, host.ui.server);
    let row = Host::cl_rect(&host.frame(), &format!("vz:user:{someone}"));
    // Their name where it is drawn, or `dy` below it.
    let painted = |frame: &Frame, dy: f32| -> usize {
        frame
            .commands
            .iter()
            .filter(|command| {
                matches!(command, limn::DrawCommand::Text { text, position, .. }
                    if *text == name
                        && position.x >= row.x
                        && position.x < row.x + row.width
                        && position.y >= row.y + dy
                        && position.y < row.y + row.height + dy)
            })
            .count()
    };
    assert_eq!(painted(&host.frame(), 0.0), 1);
    let start = Point::new(row.x + 24.0, row.y + row.height / 2.0);
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(
        start.x + 6.0,
        start.y + 8.0,
    )));
    let end = Host::cl_rect(&host.frame(), &format!("vz:drop:{other}:channel"));
    let end = Point::new(end.x + 40.0, end.y + end.height / 2.0);
    host.send(UiEvent::PointerMove(end));
    // A copy of their row follows the pointer, over everything, where it
    // was picked up from; their row stays where it is.
    let frame = host.frame();
    let dy = end.y - start.y;
    assert!(dy.abs() > row.height, "the drop is on another row");
    assert_eq!(painted(&frame, 0.0), 1);
    assert_eq!(painted(&frame, dy), 1, "the copy, under the pointer");
    assert_eq!(host.ui.voice_drag.and_then(|drag| drag.over), Some(other));
    // Escape lets go of them: nothing moves, the copy goes.
    let from = host.sent.len();
    host.send(UiEvent::Key {
        key: limn::Key::Escape,
        shift: false,
        ctrl: false,
        alt: false,
    });
    assert!(host.ui.voice_drag.is_none());
    host.send(UiEvent::PointerUp(end));
    assert_eq!(host.sent.len(), from, "{:?}", &host.sent[from..]);
    assert_eq!(painted(&host.frame(), dy), 0, "the copy is gone");
    // Let go outside the column, nothing moves either.
    host.send(UiEvent::PointerDown(start));
    host.send(UiEvent::PointerMove(Point::new(
        start.x + 6.0,
        start.y + 8.0,
    )));
    assert!(host.ui.voice_drag.is_some());
    let outside = Point::new(WINDOW.width - 40.0, WINDOW.height / 2.0);
    host.send(UiEvent::PointerMove(outside));
    assert_eq!(host.ui.voice_drag.and_then(|drag| drag.over), None);
    host.send(UiEvent::PointerUp(outside));
    assert!(host.ui.voice_drag.is_none());
    assert_eq!(host.sent.len(), from, "{:?}", &host.sent[from..]);
    assert_eq!(
        host.store
            .voice_state_of(someone)
            .map(|state| state.channel),
        Some(meeting)
    );
}
