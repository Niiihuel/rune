//! The settings of a channel or a category, driven as a person would, with
//! the simulation answering every change as a session would.

use rune_model::{permissions, Overwrite, OverwriteTarget, Permissions, RoleId, ServerId};
use rune_provider::ChannelEdit;

use super::*;
use crate::channel_settings::{Page, ACCESS_QUERY, NAME, TOPIC};

/// Whether the simulation answers `command` in `World::manage`.
fn cs_managed(command: &Command) -> bool {
    matches!(
        command,
        Command::EditChannel { .. }
            | Command::DeleteChannel { .. }
            | Command::SetOverwrite { .. }
            | Command::RemoveOverwrite { .. }
    )
}

fn deny(target: OverwriteTarget, deny: Permissions) -> Overwrite {
    Overwrite {
        target,
        allow: Permissions::NONE,
        deny,
    }
}

impl Host {
    fn cs_act(&mut self, action: UiAction) {
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
    }

    /// Report a click on `id` straight to the window, as the tree would:
    /// for controls scrolled out of view.
    fn cs_press(&mut self, id: &str) {
        self.cs_act(UiAction::Clicked { id: id.into() });
    }

    /// Write `value` in the field `id`, as typing it all would.
    fn cs_write(&mut self, id: &str, value: &str) {
        self.tree.set_input_value(id, value);
        self.cs_act(UiAction::InputChanged {
            id: id.into(),
            value: value.into(),
        });
    }

    /// The simulation answers the channel commands sent so far, and their
    /// events come back as they would from a session. Returns them.
    fn cs_answer(&mut self) -> Vec<Command> {
        let (asked, rest): (Vec<Command>, Vec<Command>) = std::mem::take(&mut self.sent)
            .into_iter()
            .partition(cs_managed);
        self.sent = rest;
        for command in &asked {
            for event in self.provider.manage(command.clone()) {
                self.event(event);
            }
        }
        asked
    }

    /// Change the simulated world directly, as someone else would.
    fn cs_elsewhere(&mut self, command: Command) {
        for event in self.provider.manage(command) {
            self.event(event);
        }
    }

    fn cs_server(&self, name: &str) -> ServerId {
        self.store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .unwrap()
            .id
    }

    fn cs_category(&self, server: ServerId, name: &str) -> ChannelId {
        self.store
            .channel_tree(server)
            .into_iter()
            .filter_map(|(category, _)| category)
            .find(|category| category.name == name)
            .unwrap()
            .id
    }

    fn cs_role(&self, server: ServerId, name: &str) -> RoleId {
        self.store
            .server(server)
            .unwrap()
            .roles
            .iter()
            .find(|role| role.name == name)
            .unwrap()
            .id
    }

    fn cs_open(&mut self, channel: ChannelId) {
        self.cs_press(&format!("cs:open:{channel}"));
        assert!(self.ui.channel_settings.is_some(), "the settings open");
        self.frame();
    }

    fn cs_settings(&self) -> &crate::channel_settings::ChannelSettings {
        self.ui
            .channel_settings
            .as_ref()
            .expect("the settings are open")
    }

    fn cs_dirty(&self) -> bool {
        self.cs_settings().dirty(&self.store)
    }

    fn cs_painted(&mut self, text: &str) -> bool {
        self.painted().iter().any(|painted| painted.contains(text))
    }

    fn cs_shows(&mut self, id: &str) -> bool {
        self.frame()
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Button)
    }

    /// Scroll the control `id` into view, as a person scrolling to it would.
    fn cs_reveal(&mut self, id: &str) {
        self.frame();
        self.tree.reveal(id);
        self.frame();
    }

    fn cs_overwrites(&self, channel: ChannelId) -> Vec<Overwrite> {
        self.store.channel(channel).unwrap().overwrites.clone()
    }
}

#[test]
fn the_overview_of_a_text_channel_saves_in_one_edit_once_the_provider_answers() {
    let mut host = Host::new();
    let channel = host.channel_named("limn-core");
    host.cs_open(channel);
    assert_eq!(host.cs_settings().page, Page::Overview);
    for text in [
        "Vista general",
        "LIMN-CORE",
        "DESARROLLO",
        "Tema del canal",
        "Modo lento",
    ] {
        assert!(host.cs_painted(text), "{text}");
    }
    assert!(
        host.cs_painted("sin conexión real"),
        "the simulation is still labelled as such"
    );
    assert!(host.cs_shows("cs:delete"), "the owner may delete it");
    assert!(!host.cs_painted("Tienes cambios sin guardar"));

    host.cs_write(NAME, "Núcleo de Limn");
    assert!(
        host.cs_painted("Se va a llamar #núcleo-de-limn"),
        "a text channel's name is lowercase and dashed"
    );
    host.cs_write(TOPIC, "Donde se habla del núcleo.");
    host.click("cs:slowmode");
    host.click("cs:slowmode:30");
    assert!(host.cs_painted("30 segundos"));
    host.click("cs:nsfw");
    assert!(host.cs_painted("¡Cuidado! Tienes cambios sin guardar"));
    host.click("cs:save");
    let edits: Vec<&Command> = host
        .sent
        .iter()
        .filter(|command| matches!(command, Command::EditChannel { .. }))
        .collect();
    assert_eq!(
        edits,
        vec![&Command::EditChannel {
            channel,
            edit: ChannelEdit {
                name: Some("núcleo-de-limn".into()),
                topic: Some(Some("Donde se habla del núcleo.".into())),
                parent: None,
                position: None,
                slowmode_secs: Some(30),
                nsfw: Some(true),
                tags: None,
                forum: None,
            },
        }],
        "one edit with every field changed"
    );
    assert_eq!(
        host.store.channel(channel).unwrap().name,
        "limn-core",
        "nothing changes before the provider says so"
    );
    assert!(host.cs_settings().saving());
    host.cs_answer();
    let saved = host.store.channel(channel).unwrap();
    assert_eq!(saved.name, "núcleo-de-limn");
    assert_eq!(saved.topic.as_deref(), Some("Donde se habla del núcleo."));
    assert_eq!((saved.slowmode_secs, saved.nsfw), (30, true));
    assert!(!host.cs_dirty() && !host.cs_settings().saving());
    assert!(!host.cs_painted("Tienes cambios sin guardar"));
    assert!(
        host.cs_painted("NÚCLEO-DE-LIMN"),
        "the sidebar's heading follows"
    );
    assert_eq!(host.tree.input_value(NAME), Some("núcleo-de-limn"));
}

#[test]
fn unsaved_changes_keep_the_settings_and_the_page_until_saved_or_reset() {
    let mut host = Host::new();
    let channel = host.channel_named("componentes");
    host.cs_open(channel);
    host.cs_write(TOPIC, "Piezas de limn.");
    host.key(Key::Escape, false, false);
    assert!(
        host.ui.channel_settings.is_some(),
        "Esc does not drop changes"
    );
    assert!(host.cs_settings().nudged);
    assert!(host.cs_painted("Guarda o restablece los cambios antes de salir"));
    host.click("cs:page:permissions");
    assert_eq!(
        host.cs_settings().page,
        Page::Overview,
        "nor does going to another page"
    );
    assert!(host.cs_settings().nudged);
    host.click("cs:close");
    assert!(host.ui.channel_settings.is_some(), "nor the close button");
    host.click("cs:reset");
    assert!(!host.cs_dirty());
    host.frame();
    assert_eq!(
        host.tree.input_value(TOPIC).unwrap_or_default(),
        host.store
            .channel(channel)
            .unwrap()
            .topic
            .as_deref()
            .unwrap_or_default(),
        "the field shows what is saved again"
    );
    // An open list closes first, then the settings.
    host.click("cs:slowmode");
    assert!(host.cs_settings().slowmode_open);
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_settings.is_some() && !host.cs_settings().slowmode_open);
    host.key(Key::Escape, false, false);
    assert!(host.ui.channel_settings.is_none());
}

#[test]
fn denying_a_permission_to_a_role_saves_its_overwrite() {
    let mut host = Host::new();
    let lab = host.cs_server("Laboratorio limn");
    let channel = host.channel_named("limn-core");
    let collaboration = host.cs_role(lab, "Colaboración");
    host.cs_open(channel);
    host.click("cs:page:permissions");
    assert!(host.cs_painted("Permisos del canal"));
    assert!(
        host.cs_painted("Permisos sincronizados con la categoría: "),
        "a channel without overwrites is in sync with its category"
    );
    assert!(!host.cs_shows("cs:add"), "the advanced view starts folded");
    host.click("cs:advanced");
    assert!(host.ui.advanced_permissions);
    assert!(host.cs_painted("ROLES/MIEMBROS"));
    assert!(host.cs_painted("@everyone"));
    host.click("cs:add");
    assert!(host.cs_settings().adding);
    host.click(&format!("cs:add:role:{collaboration}"));
    let target = OverwriteTarget::Role(collaboration);
    assert_eq!(host.cs_settings().selected, Some(target));
    assert!(
        !host.cs_dirty(),
        "a role added with nothing set changes nothing yet"
    );
    assert!(host.cs_painted("Colaboración"));
    let send = format!("cs:perm:{}:deny", Permissions::SEND.0.trailing_zeros());
    host.cs_reveal(&send);
    host.click(&send);
    assert!(host.cs_dirty());
    host.click("cs:save");
    let asked = host.cs_answer();
    assert_eq!(
        asked,
        vec![Command::SetOverwrite {
            channel,
            overwrite: deny(target, Permissions::SEND),
        }]
    );
    assert_eq!(
        host.cs_overwrites(channel),
        vec![deny(target, Permissions::SEND)]
    );
    assert!(!host.cs_dirty());
    // Whoever has the role loses it here; the account owns the server, so
    // its own permissions stay whole.
    let server = host.store.server(lab).unwrap().clone();
    let member = host
        .store
        .members(lab)
        .iter()
        .find(|member| member.roles == [collaboration])
        .unwrap()
        .clone();
    let found = host.store.channel(channel).unwrap();
    assert!(!permissions::in_channel(&server, &member, found).contains(Permissions::SEND));
    assert!(found.permissions.contains(Permissions::SEND));
    assert!(host.store.abilities(channel).unwrap().write.allowed());
    host.click("cs:page:permissions");
    // The notice heads the page, scrolled away while the permission was set.
    host.cs_reveal("cs:sync");
    assert!(
        host.cs_painted("Los permisos no están sincronizados con la categoría: "),
        "it no longer matches its category"
    );
}

#[test]
fn removing_an_overwrite_sends_its_removal() {
    let mut host = Host::new();
    let lab = host.cs_server("Laboratorio limn");
    let channel = host.channel_named("componentes");
    let bots = OverwriteTarget::Role(host.cs_role(lab, "Bots"));
    host.cs_elsewhere(Command::SetOverwrite {
        channel,
        overwrite: deny(bots, Permissions::EMBED_LINKS),
    });
    host.ui.advanced_permissions = true;
    host.cs_open(channel);
    host.click("cs:page:permissions");
    let row = format!("cs:target:role:{}", host.cs_role(lab, "Bots"));
    host.click(&row);
    assert_eq!(host.cs_settings().selected, Some(bots));
    host.click(&format!("{row}:remove"));
    assert!(host.cs_dirty());
    assert_eq!(host.cs_settings().selected, None, "back to @everyone");
    assert!(!host.cs_shows(&row), "it leaves the list at once");
    host.click("cs:save");
    assert_eq!(
        host.cs_answer(),
        vec![Command::RemoveOverwrite {
            channel,
            target: bots
        }]
    );
    assert!(host.cs_overwrites(channel).is_empty());
    assert!(!host.cs_dirty());
}

#[test]
fn syncing_with_the_category_takes_its_overwrites_and_the_notice_goes() {
    let mut host = Host::new();
    let lab = host.cs_server("Laboratorio limn");
    let channel = host.channel_named("rendimiento");
    let category = host.cs_category(lab, "Desarrollo");
    let everyone = OverwriteTarget::Role(RoleId(lab.0));
    let bots = OverwriteTarget::Role(host.cs_role(lab, "Bots"));
    host.cs_elsewhere(Command::SetOverwrite {
        channel,
        overwrite: deny(everyone, Permissions::ATTACH),
    });
    host.cs_elsewhere(Command::SetOverwrite {
        channel: category,
        overwrite: deny(bots, Permissions::SEND),
    });
    host.cs_open(channel);
    host.click("cs:page:permissions");
    assert!(host.cs_painted("Los permisos no están sincronizados con la categoría: "));
    assert!(host.cs_painted("Desarrollo"));
    host.click("cs:sync");
    assert!(host.cs_painted("¿Sincronizar permisos?"));
    host.click("cs:sync:confirm");
    assert!(!host.cs_settings().sync);
    let asked = host.cs_answer();
    assert_eq!(
        asked,
        vec![
            Command::RemoveOverwrite {
                channel,
                target: everyone
            },
            Command::SetOverwrite {
                channel,
                overwrite: deny(bots, Permissions::SEND),
            },
        ]
    );
    assert_eq!(host.cs_overwrites(channel), host.cs_overwrites(category));
    assert!(host.cs_painted("Permisos sincronizados con la categoría: "));
    assert!(!host.cs_shows("cs:sync"));
    assert!(!host.cs_dirty());
}

#[test]
fn a_private_channel_lets_in_the_roles_chosen() {
    let mut host = Host::new();
    let lab = host.cs_server("Laboratorio limn");
    let channel = host.channel_named("limn-core");
    let moderation = host.cs_role(lab, "Moderación");
    host.cs_open(channel);
    host.click("cs:page:permissions");
    host.click("cs:private");
    assert!(host.cs_dirty());
    assert!(host.cs_painted("¿Quién puede acceder a este canal?"));
    assert!(
        host.cs_painted("Propietario del servidor"),
        "the owner always sees it"
    );
    host.click("cs:access:add");
    assert!(host.cs_painted("Añadir miembros o roles"));
    host.cs_write(ACCESS_QUERY, "mod");
    host.click(&format!("cs:pick:role:{moderation}"));
    host.click("cs:pick:done");
    assert!(host.cs_settings().access.is_none());
    assert!(host.cs_shows(&format!("cs:access:role:{moderation}:remove")));
    host.click("cs:save");
    let everyone = OverwriteTarget::Role(RoleId(lab.0));
    assert_eq!(
        host.cs_answer(),
        vec![
            Command::SetOverwrite {
                channel,
                overwrite: deny(everyone, Permissions::VIEW),
            },
            Command::SetOverwrite {
                channel,
                overwrite: Overwrite {
                    target: OverwriteTarget::Role(moderation),
                    allow: Permissions::VIEW,
                    deny: Permissions::NONE,
                },
            },
        ]
    );
    assert!(!host.cs_dirty());
    assert!(host
        .cs_settings()
        .is_private(host.store.channel(channel).unwrap(), everyone));
    assert!(
        host.store.channel(channel).is_some(),
        "the owner still sees the channel"
    );
}

#[test]
fn a_category_change_reaches_the_channels_in_sync_with_it() {
    let mut host = Host::new();
    let lab = host.cs_server("Laboratorio limn");
    let category = host.cs_category(lab, "Desarrollo");
    let apart = host.channel_named("rendimiento");
    let bots = OverwriteTarget::Role(host.cs_role(lab, "Bots"));
    // One channel keeps permissions of its own: it is not in sync.
    host.cs_elsewhere(Command::SetOverwrite {
        channel: apart,
        overwrite: deny(bots, Permissions::REACT),
    });
    host.cs_open(category);
    assert!(host.cs_painted("Nombre de la categoría"));
    assert!(!host.cs_painted("Tema del canal"));
    assert!(host.cs_shows("cs:delete") && host.cs_painted("Eliminar categoría"));
    host.click("cs:page:permissions");
    assert!(host.cs_painted("Ajustes de la categoría"));
    assert!(host.cs_painted("Categoría privada"));
    assert!(
        !host.cs_painted("sincronizados con la categoría"),
        "a category is in sync with nothing"
    );
    host.click("cs:advanced");
    let attach = format!("cs:perm:{}:deny", Permissions::ATTACH.0.trailing_zeros());
    host.cs_reveal(&attach);
    assert!(host.cs_painted("Permisos de los canales de texto"));
    host.click(&attach);
    host.cs_reveal(&format!(
        "cs:perm:{}:deny",
        Permissions::MOVE_MEMBERS.0.trailing_zeros()
    ));
    assert!(host.cs_painted("Permisos de los canales de voz"));
    host.click("cs:save");
    let everyone = OverwriteTarget::Role(RoleId(lab.0));
    let asked = host.cs_answer();
    let targets: Vec<ChannelId> = asked
        .iter()
        .map(|command| match command {
            Command::SetOverwrite { channel, overwrite } => {
                assert_eq!(*overwrite, deny(everyone, Permissions::ATTACH));
                *channel
            }
            other => panic!("only overwrites are set: {other:?}"),
        })
        .collect();
    let limn_core = host.channel_named("limn-core");
    let components = host.channel_named("componentes");
    assert_eq!(targets.len(), 3, "{targets:?}");
    for channel in [category, limn_core, components] {
        assert!(targets.contains(&channel), "{channel} gets it");
        assert_eq!(
            host.cs_overwrites(channel),
            vec![deny(everyone, Permissions::ATTACH)]
        );
    }
    assert_eq!(
        host.cs_overwrites(apart),
        vec![deny(bots, Permissions::REACT)],
        "a channel out of sync keeps its own"
    );
}

#[test]
fn deleting_the_open_channel_asks_and_enter_does_not_delete() {
    let mut host = Host::new();
    let channel = host.open_channel("componentes");
    assert_eq!(host.ui.channel(), Some(channel));
    host.cs_open(channel);
    host.click("cs:delete");
    assert!(host.cs_settings().delete);
    assert!(host.cs_painted("¿Seguro que quieres eliminar #componentes?"));
    host.frame();
    assert_eq!(
        host.tree.focused_id(),
        Some("cs:delete:cancel"),
        "the dialog starts on Cancelar"
    );
    host.key(Key::Enter, false, false);
    assert!(
        !host
            .sent
            .iter()
            .any(|command| matches!(command, Command::DeleteChannel { .. })),
        "Enter does not delete"
    );
    assert!(!host.cs_settings().delete, "it cancels");
    host.click("cs:delete");
    host.click("cs:delete:confirm");
    assert!(
        host.ui.channel_settings.is_some(),
        "until the provider answers"
    );
    assert_eq!(host.cs_answer(), vec![Command::DeleteChannel { channel }]);
    assert!(host.store.channel(channel).is_none());
    assert!(
        host.ui.channel_settings.is_none(),
        "its settings go with it"
    );
    let open = host.ui.channel().expect("another channel opens");
    assert_ne!(open, channel);
}

#[test]
fn a_channel_of_a_server_not_owned_is_read_only_and_says_why() {
    let mut host = Host::new();
    let channel = host.channel_named("general");
    host.cs_open(channel);
    assert!(host.cs_painted("No tienes permiso para gestionar el canal en #general"));
    assert!(!host.cs_shows("cs:delete"), "nor can it be deleted");
    assert!(!host.cs_shows("cs:slowmode") && !host.cs_shows("cs:nsfw"));
    for id in ["cs:nsfw", "cs:slowmode", "cs:slowmode:30", "cs:delete"] {
        host.cs_press(id);
    }
    assert!(!host.cs_dirty() && !host.cs_settings().delete);
    host.click("cs:page:permissions");
    assert!(host.cs_painted("No tienes permiso para cambiar los permisos en #general"));
    assert!(!host.cs_shows("cs:private"));
    host.cs_press("cs:advanced");
    assert!(!host.cs_shows("cs:add"));
    for id in [
        "cs:private",
        "cs:add",
        "cs:perm:1:deny",
        "cs:access:add",
        "cs:sync",
    ] {
        host.cs_press(id);
    }
    assert!(!host.cs_dirty());
    let settings = host.cs_settings();
    assert!(!settings.adding && settings.access.is_none() && !settings.sync);
}

#[test]
fn roles_at_or_above_the_account_are_read_only_and_say_why() {
    let mut host = Host::new();
    let taller = host.cs_server("Taller de Rust");
    let channel = host.channel_named("ayuda");
    let collaboration = host.cs_role(taller, "Colaboración");
    let moderation = host.cs_role(taller, "Moderación");
    let bots = host.cs_role(taller, "Bots");
    // As if the account had been given a role that manages channels and
    // roles, and the channel had an overwrite for a role above it.
    let mut server = host.store.server(taller).unwrap().clone();
    for role in &mut server.roles {
        if role.id == collaboration {
            role.permissions =
                role.permissions | Permissions::MANAGE_CHANNELS | Permissions::MANAGE_ROLES;
        }
    }
    let me = host.store.me().unwrap();
    let mut member = host.store.member_of(taller, me).unwrap().clone();
    member.roles = vec![collaboration];
    let mut found = host.store.channel(channel).unwrap().clone();
    found.overwrites = vec![deny(OverwriteTarget::Role(moderation), Permissions::SEND)];
    found.permissions = permissions::in_channel(&server, &member, &found);
    host.event(Event::ServerChanged(server));
    host.event(Event::MemberChanged(member));
    host.event(Event::ChannelChanged(found));
    host.ui.advanced_permissions = true;
    host.cs_open(channel);
    host.click("cs:page:permissions");
    host.click("cs:add");
    assert!(
        host.cs_shows(&format!("cs:add:role:{bots}")),
        "below: offered"
    );
    assert!(
        !host.cs_shows(&format!("cs:add:role:{collaboration}")),
        "its own rank: not offered"
    );
    host.cs_press("cs:add:dismiss");
    host.click(&format!("cs:target:role:{moderation}"));
    assert!(host.cs_painted("Este rol está a la altura del tuyo o por encima"));
    host.cs_press("cs:perm:1:allow");
    host.cs_press(&format!("cs:target:role:{moderation}:remove"));
    assert!(!host.cs_dirty(), "nothing about it changes");
    host.click(&format!("cs:target:role:{}", taller.0));
    host.cs_press("cs:perm:1:deny");
    assert!(host.cs_dirty(), "@everyone is within reach");
}

#[test]
fn a_voice_channel_has_its_name_and_its_own_permissions() {
    let mut host = Host::new();
    let channel = host.channel_named("Reunión semanal");
    host.cs_open(channel);
    assert!(host.cs_painted("Nombre del canal"));
    assert!(host.cs_painted("REUNIÓN SEMANAL") && host.cs_painted("VOZ"));
    assert!(!host.cs_painted("Tema del canal") && !host.cs_painted("Modo lento"));
    assert!(host.cs_painted("Eliminar canal"));
    assert_eq!(
        host.tree.focused_id(),
        Some(NAME),
        "the name takes the focus"
    );
    host.cs_write(NAME, "Reunión de los lunes");
    assert!(
        !host.cs_painted("Se va a llamar"),
        "a voice channel keeps its name as written"
    );
    host.click("cs:save");
    host.cs_answer();
    assert_eq!(
        host.store.channel(channel).unwrap().name,
        "Reunión de los lunes"
    );
    host.click("cs:page:permissions");
    host.click("cs:advanced");
    host.cs_reveal(&format!(
        "cs:perm:{}:deny",
        Permissions::MOVE_MEMBERS.0.trailing_zeros()
    ));
    assert!(host.cs_painted("Permisos de los canales de voz"));
    assert!(host.cs_painted("Conectar"));
    assert!(
        !host.cs_shows(&format!(
            "cs:perm:{}:deny",
            Permissions::SEND.0.trailing_zeros()
        )),
        "a voice channel holds no messages here"
    );
}

#[test]
fn changes_made_elsewhere_show_unless_being_edited_and_a_removed_channel_closes() {
    let mut host = Host::new();
    let channel = host.channel_named("limn-core");
    host.cs_open(channel);
    host.cs_write(TOPIC, "Lo que escribo yo.");
    host.cs_elsewhere(Command::EditChannel {
        channel,
        edit: ChannelEdit {
            name: Some("núcleo".into()),
            topic: Some(Some("Lo que escribió otra persona.".into())),
            ..ChannelEdit::default()
        },
    });
    host.frame();
    assert_eq!(host.tree.input_value(NAME), Some("núcleo"));
    assert_eq!(
        host.tree.input_value(TOPIC),
        Some("Lo que escribo yo."),
        "what is being written stays"
    );
    assert!(host.cs_dirty());
    host.cs_elsewhere(Command::DeleteChannel { channel });
    assert!(
        host.ui.channel_settings.is_none(),
        "a channel gone closes its settings"
    );
    host.frame();
}

#[test]
fn a_topic_holds_up_to_its_limit_and_counts_what_is_left() {
    let mut host = Host::new();
    let channel = host.channel_named("limn-core");
    host.cs_open(channel);
    host.cs_write(TOPIC, "");
    assert!(host.cs_painted("1024"));
    host.cs_write(TOPIC, &"a".repeat(1_100));
    host.frame();
    assert_eq!(host.tree.input_value(TOPIC).map(str::len), Some(1_024));
    assert!(host.painted().iter().any(|painted| painted == "0"));
    host.cs_write(NAME, "   ");
    assert!(host.cs_painted("El nombre no puede quedar vacío."));
    host.click("cs:save");
    assert!(
        !host
            .sent
            .iter()
            .any(|command| matches!(command, Command::EditChannel { .. })),
        "an empty name is not sent"
    );
}

// Server administration, rest (P7).

#[test]
fn a_forum_s_tags_reaction_and_defaults_are_drafted_and_saved_together() {
    let mut host = Host::new();
    // The owned server's forum.
    let forum = host
        .store
        .channel(host.channel_named("propuestas"))
        .unwrap()
        .clone();
    assert_eq!(forum.kind, rune_model::ChannelKind::Forum);
    let known = forum.tags.len();
    host.cs_open(forum.id);
    host.click("cs:page:forum");
    assert!(host.cs_painted("Etiquetas"));
    assert!(host.cs_painted("Reacción predeterminada"));
    host.cs_press("cs:tag:add");
    host.cs_write(&format!("cs:tag:{known}:name"), "Resuelto");
    host.cs_write(&format!("cs:tag:{known}:emoji"), "✅");
    host.cs_press(&format!("cs:tag:{known}:moderated"));
    host.cs_press("cs:tag:0:remove");
    host.cs_write(crate::channel_settings::forum::REACTION, "👍");
    host.cs_press("cs:forum:gallery");
    host.cs_press("cs:forum:require-tag");
    assert!(host.cs_painted("Tienes cambios sin guardar"));
    host.sent.clear();
    host.cs_press("cs:save");
    let Some(Command::EditChannel { channel, edit }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert_eq!(channel, forum.id);
    let tags = edit.tags.unwrap();
    assert_eq!(tags.len(), known, "one added, one taken out");
    let made = tags.last().unwrap();
    assert_eq!(made.id, rune_model::TagId(0));
    assert_eq!(
        (made.name.as_str(), made.emoji.as_deref()),
        ("Resuelto", Some("✅"))
    );
    assert!(made.moderated);
    let defaults = edit.forum.unwrap();
    assert_eq!(defaults.reaction.as_deref(), Some("👍"));
    assert_eq!(defaults.gallery, !forum.forum.gallery);
    assert_eq!(defaults.require_tag, !forum.forum.require_tag);
    // The simulation keeps them, the new tag with an id of its own.
    host.cs_answer();
    let saved = host.store.channel(forum.id).unwrap();
    assert!(saved.tags.iter().all(|tag| tag.id != rune_model::TagId(0)));
    assert!(saved
        .tags
        .iter()
        .any(|tag| tag.name == "Resuelto" && tag.moderated));
    assert_eq!(saved.forum.reaction.as_deref(), Some("👍"));
    assert!(!host.cs_settings().dirty(&host.store));
    // Only a forum has this page.
    host.cs_press("cs:reset");
    host.cs_press("cs:close");
    host.cs_open(host.channel_named("limn-core"));
    host.cs_press("cs:page:forum");
    assert_eq!(host.cs_settings().page, Page::Overview);
}

// Server emoji, archive and tag filter (2026-10-19).
#[test]
fn a_forum_takes_a_server_emoji_as_reaction_an_archive_time_and_all_tags() {
    let mut host = Host::new();
    let forum = host.channel_named("propuestas");
    let server = host.store.channel(forum).unwrap().server.unwrap();
    let own = host.store.server(server).unwrap().emojis[0].clone();
    host.cs_open(forum);
    host.click("cs:page:forum");
    host.cs_write(
        crate::channel_settings::forum::REACTION,
        &format!(":{}:", own.name),
    );
    host.cs_press("cs:forum:match-all");
    host.cs_press("cs:forum:archive");
    assert!(host.cs_settings().archive_open);
    host.cs_press("cs:forum:archive:10080");
    assert!(!host.cs_settings().archive_open);
    host.sent.clear();
    host.cs_press("cs:save");
    let Some(Command::EditChannel { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    let defaults = edit.forum.unwrap();
    assert_eq!(defaults.reaction_emoji, Some(own.id));
    assert!(defaults.reaction.is_none() && defaults.match_all_tags);
    assert_eq!(defaults.archive_after, Some(10080));
    host.cs_answer();
    let saved = host.store.channel(forum).unwrap();
    assert_eq!(saved.forum.reaction_emoji, Some(own.id));
    assert!(!host.cs_settings().dirty(&host.store));
}

#[test]
fn the_people_to_add_are_looked_up_at_the_provider_beyond_those_it_sent() {
    let mut host = Host::new();
    let lab = host.cs_server("Laboratorio limn");
    let channel = host.channel_named("limn-core");
    host.cs_open(channel);
    host.click("cs:page:permissions");
    host.click("cs:advanced");
    host.click("cs:add");
    let from = host.sent.len();
    host.cs_write(crate::channel_settings::ADD_QUERY, "Zo");
    host.cs_write(crate::channel_settings::ADD_QUERY, "zo ");
    assert_eq!(
        &host.sent[from..],
        &[Command::SearchMembers {
            server: lab,
            query: "zo".into(),
        }],
        "the same name asks once"
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
    assert!(host.cs_shows(&format!("cs:add:member:{}", zoe.id)));
    // The private channel's dialog looks up after its `@` too.
    host.key(Key::Escape, false, false);
    host.click("cs:private");
    host.click("cs:access:add");
    let from = host.sent.len();
    host.cs_write(ACCESS_QUERY, "@ad");
    assert_eq!(
        &host.sent[from..],
        &[Command::SearchMembers {
            server: lab,
            query: "ad".into(),
        }]
    );
}
