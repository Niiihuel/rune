//! The server's menu and its dialogs, driven as a person would, with the
//! simulation answering what they ask of it.

use rune_components::server_menu::ids;
use rune_model::{Overwrite, OverwriteTarget, Permissions, Rgb, ServerId};

use super::*;

impl Host {
    /// Have the simulation carry out the commands sent from `from` on, those
    /// its answers lead to included, and apply what it reports.
    fn answer_from(&mut self, from: usize) {
        let mut next = from;
        while next < self.sent.len() {
            let command = self.sent[next].clone();
            next += 1;
            if matches!(
                command,
                Command::CreateServer { .. }
                    | Command::EditServer { .. }
                    | Command::LeaveServer { .. }
                    | Command::CreateChannel { .. }
                    | Command::SetOverwrite { .. }
                    | Command::SetNick { .. }
            ) {
                for event in self.provider.manage(command) {
                    self.event(event);
                }
            }
        }
    }

    fn server_id_named(&self, name: &str) -> ServerId {
        self.store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .map(|server| server.id)
            .unwrap()
    }

    fn open_server_named(&mut self, name: &str) -> ServerId {
        let server = self.server_id_named(name);
        self.click(&format!("server:{server}"));
        server
    }

    fn clicked_id(&mut self, id: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents);
    }

    fn buttons(&mut self) -> Vec<String> {
        self.frame()
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
            .map(|region| region.id.clone())
            .collect()
    }

    fn access(&mut self, id: &str) -> Option<limn::AccessNode> {
        self.frame()
            .access
            .iter()
            .find(|node| node.id == id)
            .cloned()
    }
}

#[test]
fn the_menu_of_an_owned_server_offers_what_the_owner_may_do() {
    let mut host = Host::new();
    // «Copiar ID» is offered in developer mode.
    host.ui.prefs.developer = true;
    let lab = host.open_server_named("Laboratorio limn");
    host.click(ids::TOGGLE);
    assert!(host.ui.server_menu.open);
    // Under the server's name, as wide as the column from there.
    let column = rune_components::rail::WIDTH;
    let settings = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == format!("ss:open:{lab}"))
        .map(|region| region.rect)
        .unwrap();
    assert!(
        settings.x >= column + 8.0
            && settings.x + settings.width <= column + rune_components::sidebar::WIDTH
            && settings.y >= rune_components::sidebar::HEADER - 8.0,
        "{settings:?}"
    );
    let buttons = host.buttons();
    for id in [
        format!("ss:open:{lab}"),
        format!("newchan:{lab}"),
        format!("newcat:{lab}"),
        format!("sm:notifications:{lab}"),
        format!("sm:invite:{lab}"),
        format!("ss:open:{lab}:delete"),
        format!("sm:profile:{lab}"),
        format!("sm:copy-id:{lab}"),
    ] {
        assert!(buttons.contains(&id), "{id} can be chosen: {buttons:?}");
    }
    // Who owns it deletes it instead of leaving: leaving is not offered at all.
    assert!(host.access(&format!("sm:leave:{lab}")).is_none());
    host.clicked_id(&format!("sm:leave:{lab}"));
    assert!(host.ui.server_menu.dialog.is_none(), "not even asked for");
    // The settings belong to another window: the menu asks for them and closes.
    host.click(ids::TOGGLE);
    host.click(&format!("ss:open:{lab}"));
    assert!(!host.ui.server_menu.open);
}

#[test]
fn the_menu_of_a_server_of_others_says_why_its_items_are_off_and_leaving_asks_first() {
    let mut host = Host::new();
    let taller = host.server_id_named("Taller de Rust");
    assert_eq!(host.ui.server, Some(taller));
    host.click(ids::TOGGLE);
    let buttons = host.buttons();
    // What the account cannot do is not offered at all.
    for id in [
        format!("ss:open:{taller}"),
        format!("newchan:{taller}"),
        format!("newcat:{taller}"),
        format!("ss:open:{taller}:delete"),
    ] {
        assert!(!buttons.contains(&id), "{id} is not offered");
        assert!(host.access(&id).is_none());
    }
    assert!(buttons.contains(&format!("sm:leave:{taller}")));
    // Asking for a channel anyway, from elsewhere, says why not.
    host.clicked_id(&format!("newchan:{taller}"));
    assert!(host.ui.server_menu.dialog.is_none() && !host.ui.notices.is_empty());
    host.click(ids::TOGGLE);
    host.click(&format!("sm:leave:{taller}"));
    assert_eq!(host.ui.server_menu.dialog, Some(Dialog::Leave(taller)));
    let from = host.sent.len();
    host.click(ids::LEAVE_CONFIRM);
    assert_eq!(
        host.sent.last(),
        Some(&Command::LeaveServer { server: taller })
    );
    host.answer_from(from);
    assert!(host.store.server(taller).is_none());
    assert!(host.ui.server.is_some_and(|server| server != taller));
}

#[test]
fn a_new_text_channel_gets_its_name_lowercase_and_dashed_and_opens() {
    let mut host = Host::new();
    let lab = host.open_server_named("Laboratorio limn");
    host.click(ids::TOGGLE);
    host.click(&format!("newchan:{lab}"));
    assert!(matches!(
        host.ui.server_menu.dialog,
        Some(Dialog::Channel(_))
    ));
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(ids::CHANNEL_NAME));
    let dialog = host.access("newchan:close:dialog").unwrap();
    assert_eq!(
        (dialog.name.as_str(), dialog.rect.width),
        ("Crear un canal", 510.0)
    );
    // Enter does nothing while there is no name.
    let from = host.sent.len();
    host.key(Key::Enter, false, false);
    assert_eq!(host.sent.len(), from);
    host.type_text("Prueba Canal");
    assert!(host
        .painted()
        .contains(&"Se va a llamar #prueba-canal".to_owned()));
    host.key(Key::Enter, false, false);
    assert!(
        matches!(
            host.sent.last(),
            Some(Command::CreateChannel { server, kind: ChannelKind::Text, name, parent: None, private: false, .. })
                if *server == lab && name == "prueba-canal"
        ),
        "{:?}",
        host.sent.last()
    );
    assert!(host.ui.server_menu.dialog.is_none());
    host.answer_from(from);
    let made = host.ui.channel().unwrap();
    let channel = host.store.channel(made).unwrap();
    assert_eq!(
        (channel.name.as_str(), channel.server),
        ("prueba-canal", Some(lab))
    );
}

#[test]
fn a_private_channel_is_shown_to_the_roles_chosen_once_it_is_made() {
    let mut host = Host::new();
    let lab = host.open_server_named("Laboratorio limn");
    host.click(ids::TOGGLE);
    host.click(&format!("newchan:{lab}"));
    host.type_text("Secreto");
    host.click(ids::CHANNEL_PRIVATE);
    assert!(host.painted().contains(&"Siguiente".to_owned()));
    host.click(ids::CHANNEL_SUBMIT);
    let painted = host.painted();
    assert!(
        painted.contains(&"Añadir miembros o roles".to_owned())
            && painted.contains(&"#secreto".to_owned())
    );
    assert_eq!(host.tree.focused_id(), Some(ids::CHANNEL_QUERY));
    // Back keeps what was written; on again.
    host.click(ids::CHANNEL_BACK);
    assert!(
        matches!(&host.ui.server_menu.dialog, Some(Dialog::Channel(form)) if form.name == "Secreto" && form.private)
    );
    host.click(ids::CHANNEL_SUBMIT);
    let role = host.store.server(lab).unwrap().assignable_roles()[0].id;
    host.click(&format!("{}{role}", ids::CHANNEL_ROLE));
    let from = host.sent.len();
    host.click(ids::CHANNEL_SUBMIT);
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateChannel { private: true, name, .. }) if name == "secreto"
    ));
    assert_eq!(host.sent.len(), from + 1, "nothing else before it is made");
    host.answer_from(from);
    let made = host.ui.channel().unwrap();
    let shown_to = Overwrite {
        target: OverwriteTarget::Role(role),
        allow: Permissions::VIEW,
        deny: Permissions::NONE,
    };
    assert!(host.sent.contains(&Command::SetOverwrite {
        channel: made,
        overwrite: shown_to,
    }));
    assert!(
        host.store
            .channel(made)
            .unwrap()
            .overwrites
            .contains(&shown_to),
        "the simulation took it"
    );
    assert!(host.ui.notices.is_empty(), "{:?}", host.ui.notices);
}

#[test]
fn who_may_see_a_new_private_channel_is_looked_up_at_the_provider_too() {
    let mut host = Host::new();
    let lab = host.open_server_named("Laboratorio limn");
    host.click(ids::TOGGLE);
    host.click(&format!("newchan:{lab}"));
    host.type_text("Secreto");
    host.click(ids::CHANNEL_PRIVATE);
    host.click(ids::CHANNEL_SUBMIT);
    let from = host.sent.len();
    host.type_text("@Zo");
    host.type_text(" ");
    assert_eq!(
        &host.sent[from..],
        &[Command::SearchMembers {
            server: lab,
            query: "zo".into(),
        }],
        "after its `@`, and the same name asks once"
    );
    // Someone the provider had not sent is found, and offered.
    let mut zoe = host.store.me_user().unwrap().clone();
    zoe.id = rune_model::UserId(9_000_001);
    zoe.name = "zoe".into();
    zoe.display_name = Some("Zoe Zamora".into());
    host.event(Event::UserChanged(zoe.clone()));
    host.event(Event::MembersFound {
        server: lab,
        query: "zo".into(),
        members: vec![rune_model::Member {
            user: zoe.id,
            server: lab,
            ..Default::default()
        }],
    });
    assert!(host.painted().contains(&"Zoe Zamora".to_owned()));
}

#[test]
fn a_category_is_made_where_it_is_asked_for_and_does_not_open() {
    let mut host = Host::new();
    let lab = host.open_server_named("Laboratorio limn");
    let open_before = host.ui.channel();
    host.click(ids::TOGGLE);
    host.click(&format!("newcat:{lab}"));
    let painted = host.painted();
    assert!(painted.contains(&"Nombre de la categoría".to_owned()));
    assert!(!painted.contains(&"Tipo de canal".to_owned()));
    host.type_text("Proyectos Nuevos");
    let from = host.sent.len();
    host.key(Key::Enter, false, false);
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateChannel { kind: ChannelKind::Category, name, private: false, .. })
            if name == "Proyectos Nuevos"
    ));
    host.answer_from(from);
    assert!(host
        .store
        .channel_tree(lab)
        .iter()
        .any(|(category, _)| category.is_some_and(|category| category.name == "Proyectos Nuevos")));
    assert_eq!(host.ui.channel(), open_before);
    // Another part of the window asks for a channel inside a category.
    let category = host
        .store
        .channel_tree(lab)
        .into_iter()
        .find_map(|(category, _)| category.filter(|found| found.name == "Proyectos Nuevos"))
        .map(|category| category.id)
        .unwrap();
    host.clicked_id(&format!("newchan:{lab}:{category}"));
    host.frame();
    host.type_text("Sala Dos");
    host.click(ids::CHANNEL_VOICE);
    host.click(ids::CHANNEL_SUBMIT);
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateChannel { kind: ChannelKind::Voice, name, parent: Some(parent), .. })
            if name == "Sala Dos" && *parent == category
    ));
}

#[test]
fn escape_closes_the_menu_and_the_dialogs() {
    let mut host = Host::new();
    let lab = host.open_server_named("Laboratorio limn");
    host.click(ids::TOGGLE);
    host.key(Key::Escape, false, false);
    assert!(!host.ui.server_menu.open);
    // The name opens it and closes it again.
    host.click(ids::TOGGLE);
    host.click(ids::TOGGLE);
    assert!(!host.ui.server_menu.open);
    host.click(ids::TOGGLE);
    host.click(&format!("newchan:{lab}"));
    host.type_text("a medias");
    host.key(Key::Escape, false, false);
    assert!(host.ui.server_menu.dialog.is_none());
    // Open again, it starts empty.
    host.click(ids::TOGGLE);
    host.click(&format!("newchan:{lab}"));
    host.frame();
    assert_eq!(host.tree.input_value(ids::CHANNEL_NAME), Some(""));
}

#[test]
fn the_name_in_a_server_is_previewed_and_set() {
    let mut host = Host::new();
    let taller = host.server_id_named("Taller de Rust");
    let me = host.store.me().unwrap();
    host.click(ids::TOGGLE);
    host.click(&format!("sm:profile:{taller}"));
    host.frame();
    assert_eq!(host.tree.focused_id(), Some(ids::NICK));
    host.type_text("Nico del Taller");
    assert!(host.painted().contains(&"Nico del Taller".to_owned()));
    let from = host.sent.len();
    host.key(Key::Enter, false, false);
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetNick {
            server: taller,
            user: me,
            nick: Some("Nico del Taller".into()),
        })
    );
    host.answer_from(from);
    assert_eq!(host.store.display_name(me, Some(taller)), "Nico del Taller");
    // Emptied, the own name comes back.
    host.click(ids::TOGGLE);
    host.click(&format!("sm:profile:{taller}"));
    host.frame();
    assert_eq!(host.tree.input_value(ids::NICK), Some("Nico del Taller"));
    host.tree.set_input_value(ids::NICK, "");
    host.ui.server_menu.dialog = Some(Dialog::Nick {
        server: taller,
        nick: String::new(),
    });
    host.click(ids::NICK_SAVE);
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetNick {
            server: taller,
            user: me,
            nick: None,
        })
    );
}

#[test]
fn notification_settings_change_what_the_rail_marks_and_are_kept() {
    let mut host = Host::new();
    // Kept on this machine: a provider that does not keep them.
    host.local_notifications();
    let taller = host.server_id_named("Taller de Rust");
    let wall = Timestamp(1_790_000_100_000);
    let waiting = (true, 2);
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, taller, waiting, wall),
        (true, 2, false)
    );
    host.click(ids::TOGGLE);
    // The menu's own entries: a level, then a mute, as the submenus choose them.
    host.clicked_id(&format!("sm:level:{taller}:mentions"));
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, taller, waiting, wall),
        (false, 2, false),
        "only the mentions"
    );
    host.clicked_id(&format!("sm:mute:{taller}:always"));
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, taller, waiting, wall),
        (false, 2, true)
    );
    host.clicked_id(&format!("sm:level:{taller}:nothing"));
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, taller, waiting, wall),
        (false, 0, true)
    );
    host.clicked_id(&format!("sm:mute:{taller}:1h"));
    let until = host.ui.server_menu.notify[&taller].muted_until.unwrap();
    assert!(until != crate::server_menu::FOREVER);
    // Kept, and taken back by the next run.
    let kept: Vec<_> = host
        .ui
        .settings_to_keep()
        .into_iter()
        .filter(|(key, _)| *key == "server_notifications")
        .collect();
    assert_eq!(
        kept,
        vec![(
            "server_notifications",
            Some(format!("{}:nothing:{}", taller.0, until.0))
        )]
    );
    let mut next = Ui::default();
    next.restore(|key| {
        kept.iter()
            .find(|(kept, _)| *kept == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(next.server_menu.notify, host.ui.server_menu.notify);
    // A mute that ended marks again.
    let after = Timestamp(until.0 + 1);
    host.ui.server_menu.notify.get_mut(&taller).unwrap().level = NotifyLevel::All;
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, taller, waiting, after),
        (true, 2, false)
    );
}

#[test]
fn a_new_server_is_made_with_its_color_and_opens() {
    let mut host = Host::new();
    host.click(ids::SERVER_OPEN);
    assert_eq!(host.ui.server_menu.dialog, Some(Dialog::Server(None)));
    host.click(ids::SERVER_OWN);
    host.frame();
    let me = host.store.me_user().unwrap().shown_name().to_owned();
    let name = format!("Servidor de {me}");
    assert_eq!(host.tree.input_value(ids::SERVER_NAME), Some(name.as_str()));
    assert_eq!(host.tree.focused_id(), Some(ids::SERVER_NAME));
    host.click(&format!("{}3FB97A", ids::SERVER_COLOR));
    let from = host.sent.len();
    host.click(ids::SERVER_CREATE);
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateServer { name: made, .. }) if *made == name
    ));
    host.answer_from(from);
    let made = host.server_id_named(&name);
    assert!(host.sent.contains(&Command::EditServer {
        server: made,
        edit: rune_provider::ServerEdit {
            accent: Some(Rgb(0x3F_B9_7A)),
            ..Default::default()
        },
    }));
    assert_eq!(host.store.server(made).unwrap().accent, Rgb(0x3F_B9_7A));
    assert_eq!(host.ui.server, Some(made));
    let general = host.ui.channel().unwrap();
    assert_eq!(host.store.channel(general).unwrap().name, "general");
}
