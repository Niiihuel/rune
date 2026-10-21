//! The settings of a server, driven as a person would, with the simulation
//! answering every change as a session would.

use rune_components::server_settings::permission_id;
use rune_model::{Permissions, RoleId, ServerId, UserId};
use rune_provider::RoleEdit;

use super::*;
use crate::server_settings::{Page, NAME, REASON, ROLE_NAME};

/// Whether the simulation answers `command` in `World::manage`.
fn managed(command: &Command) -> bool {
    matches!(
        command,
        Command::EditServer { .. }
            | Command::DeleteServer { .. }
            | Command::CreateRole { .. }
            | Command::EditRole { .. }
            | Command::DeleteRole { .. }
            | Command::SetMemberRoles { .. }
            | Command::SetNick { .. }
            | Command::Kick { .. }
            | Command::Ban { .. }
            | Command::Unban { .. }
            | Command::LoadBans { .. }
            | Command::LoadAudit { .. }
            | Command::LoadInvites { .. }
            | Command::RevokeInvite { .. }
            | Command::RenameExpression { .. }
            | Command::DeleteExpression { .. }
            // Administration (19).
            | Command::TimeoutMember { .. }
            | Command::CreateInvite { .. }
            | Command::CreateExpression { .. }
            | Command::LoadMembers { .. }
            | Command::PruneMembers { .. }
            | Command::LoadWebhooks { .. }
            | Command::CreateWebhook { .. }
            | Command::EditWebhook { .. }
            | Command::DeleteWebhook { .. }
            | Command::LoadIntegrations { .. }
            | Command::DeleteIntegration { .. }
            // Server administration, rest (P7).
            | Command::LoadServerProfile { .. }
            | Command::ReorderChannels { .. }
            | Command::LoadRoleCounts { .. }
    )
}

impl Host {
    /// An action as the tree reports it.
    fn act(&mut self, action: UiAction) {
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
    }

    fn press(&mut self, id: &str) {
        self.act(UiAction::Clicked { id: id.into() });
    }

    /// Write `value` in the field `id`, as typing it all would.
    fn write(&mut self, id: &str, value: &str) {
        self.tree.set_input_value(id, value);
        self.act(UiAction::InputChanged {
            id: id.into(),
            value: value.into(),
        });
    }

    /// The simulation answers the management commands sent so far, and
    /// their events come back as they would from a session. Answers that
    /// lead to more commands (a reorder's next move, a reload) are answered
    /// too. Returns the commands answered.
    fn answer(&mut self) -> Vec<Command> {
        let mut answered = Vec::new();
        for _ in 0..8 {
            let (asked, rest): (Vec<Command>, Vec<Command>) = std::mem::take(&mut self.sent)
                .into_iter()
                .partition(managed);
            self.sent = rest;
            if asked.is_empty() {
                break;
            }
            for command in asked {
                answered.push(command.clone());
                for event in self.provider.manage(command) {
                    self.event(event);
                }
            }
        }
        answered
    }

    fn lab(&self) -> ServerId {
        self.server_named("Laboratorio limn")
    }

    fn server_named(&self, name: &str) -> ServerId {
        self.store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .unwrap()
            .id
    }

    fn open_settings(&mut self, server: ServerId) {
        self.press(&format!("ss:open:{server}"));
        assert!(self.ui.server_settings.is_some());
        self.frame();
    }

    fn settings(&self) -> &crate::server_settings::ServerSettings {
        self.ui
            .server_settings
            .as_ref()
            .expect("the settings are open")
    }

    fn role_named(&self, server: ServerId, name: &str) -> RoleId {
        self.store
            .server(server)
            .unwrap()
            .roles
            .iter()
            .find(|role| role.name == name)
            .unwrap()
            .id
    }

    /// Someone of `server` other than the account, and not its owner.
    fn someone(&self, server: ServerId, without: Option<RoleId>) -> UserId {
        let owner = self.store.server(server).unwrap().owner;
        self.store
            .members(server)
            .iter()
            .find(|member| {
                Some(member.user) != self.store.me()
                    && Some(member.user) != owner
                    && without.is_none_or(|role| !member.roles.contains(&role))
            })
            .unwrap()
            .user
    }

    fn on_screen(&mut self, id: &str) -> bool {
        let frame = self.frame();
        frame
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Button)
    }
}

fn painted_has(host: &mut Host, text: &str) -> bool {
    host.painted().iter().any(|painted| painted.contains(text))
}

#[test]
fn renaming_the_owned_server_saves_and_shows_the_name_the_provider_confirms() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    assert!(painted_has(&mut host, "Perfil del servidor"));
    assert!(
        painted_has(&mut host, "sin conexión real"),
        "the simulation is still labelled as such"
    );
    assert!(!painted_has(&mut host, "Tenés cambios sin guardar"));
    host.write(NAME, "Laboratorio de limn");
    assert!(painted_has(
        &mut host,
        "¡Cuidado! Tenés cambios sin guardar"
    ));
    host.click("ss:save");
    assert!(matches!(
        host.sent.last(),
        Some(Command::EditServer { server, edit })
            if *server == lab && edit.name.as_deref() == Some("Laboratorio de limn")
                && edit.description.is_none() && edit.accent.is_none()
    ));
    assert_eq!(
        host.store.server(lab).unwrap().name,
        "Laboratorio limn",
        "nothing changes before the provider says so"
    );
    host.answer();
    assert_eq!(host.store.server(lab).unwrap().name, "Laboratorio de limn");
    assert!(!host.settings().dirty(&host.store));
    assert!(!painted_has(&mut host, "Tenés cambios sin guardar"));
    assert!(
        painted_has(&mut host, "LABORATORIO DE LIMN"),
        "the sidebar's heading follows"
    );
}

#[test]
fn the_accent_and_description_are_saved_together_and_reset_drops_them() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:accent:e74c3c");
    host.write(crate::server_settings::DESCRIPTION, "Donde se prueba limn.");
    host.click("ss:reset");
    assert!(!host.settings().dirty(&host.store));
    host.frame();
    assert_eq!(
        host.tree.input_value(crate::server_settings::DESCRIPTION),
        host.store.server(lab).unwrap().description.as_deref()
    );
    host.click("ss:accent:e74c3c");
    host.write(crate::server_settings::DESCRIPTION, "Donde se prueba limn.");
    host.click("ss:save");
    host.answer();
    let server = host.store.server(lab).unwrap();
    assert_eq!(server.accent, rune_model::Rgb(0xE7_4C_3C));
    assert_eq!(server.description.as_deref(), Some("Donde se prueba limn."));
}

#[test]
fn a_new_role_opens_in_the_editor_and_its_permissions_are_saved_as_bits() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:roles");
    assert!(painted_has(&mut host, "Permisos predeterminados"));
    host.click("ss:role:create");
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateRole { server, name, .. }) if *server == lab && name == "nuevo rol"
    ));
    host.answer();
    let role = host
        .settings()
        .role
        .expect("the editor opened on the new role");
    assert_eq!(
        host.store
            .server(lab)
            .unwrap()
            .roles
            .iter()
            .find(|candidate| candidate.id == role)
            .unwrap()
            .name,
        "nuevo rol"
    );
    assert!(painted_has(&mut host, "EDITAR ROL: NUEVO ROL"));
    host.click("ss:tab:permissions");
    host.click(&permission_id(Permissions::VIEW));
    host.click(&permission_id(Permissions::MANAGE_CHANNELS));
    assert!(painted_has(&mut host, "Tenés cambios sin guardar"));
    host.click("ss:save");
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditRole {
            server: lab,
            role,
            edit: RoleEdit {
                permissions: Some(Permissions::VIEW | Permissions::MANAGE_CHANNELS),
                ..RoleEdit::default()
            },
        })
    );
    host.answer();
    let saved = host
        .store
        .server(lab)
        .unwrap()
        .roles
        .iter()
        .find(|candidate| candidate.id == role)
        .unwrap()
        .clone();
    assert_eq!(
        saved.permissions,
        Permissions::VIEW | Permissions::MANAGE_CHANNELS
    );
    assert!(!host.settings().dirty(&host.store));
    // «Borrar permisos» turns them all off; Restablecer puts them back.
    host.click("ss:perms:clear");
    assert!(host.settings().dirty(&host.store));
    host.click("ss:reset");
    assert!(!host.settings().dirty(&host.store));
}

#[test]
fn the_display_tab_edits_name_color_and_switches_in_one_save() {
    let mut host = Host::new();
    let lab = host.lab();
    let bots = host.role_named(lab, "Bots");
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.click(&format!("ss:role:{bots}:edit"));
    assert_eq!(host.settings().role, Some(bots));
    host.frame();
    assert_eq!(host.tree.input_value(ROLE_NAME), Some("Bots"));
    host.write(ROLE_NAME, "Autómatas");
    assert!(
        painted_has(&mut host, "EDITAR ROL: AUTÓMATAS"),
        "the title follows the field"
    );
    host.click("ss:color:3498db");
    // Below the role's gradient and icon (P7): scrolled into view.
    host.press("ss:mentionable");
    host.click("ss:save");
    let Some(Command::EditRole { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent.last());
    };
    assert_eq!(edit.name.as_deref(), Some("Autómatas"));
    assert_eq!(edit.color, Some(Some(rune_model::Rgb(0x34_98_DB))));
    assert_eq!(edit.mentionable, Some(true));
    assert_eq!(edit.hoisted, None, "only what changed is sent");
    host.answer();
    let role = host
        .store
        .server(lab)
        .unwrap()
        .roles
        .iter()
        .find(|role| role.id == bots)
        .unwrap()
        .clone();
    assert_eq!(role.name, "Autómatas");
    assert!(role.mentionable);
}

#[test]
fn members_are_given_a_role_from_its_editor_and_have_it_taken_away() {
    let mut host = Host::new();
    let lab = host.lab();
    let role = host.role_named(lab, "Colaboración");
    let person = host.someone(lab, Some(role));
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.click(&format!("ss:role:{role}"));
    host.click("ss:tab:members");
    host.click("ss:role-members:add");
    assert!(painted_has(&mut host, "Añadir miembros"));
    assert!(
        !host.on_screen("ss:add:confirm"),
        "nothing to add until someone is chosen"
    );
    host.press(&format!("ss:add:{person}"));
    host.click("ss:add:confirm");
    let Some(Command::SetMemberRoles { user, roles, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent.last());
    };
    assert_eq!(user, person);
    assert!(roles.contains(&role));
    host.answer();
    assert!(host
        .store
        .member_of(lab, person)
        .unwrap()
        .roles
        .contains(&role));
    host.click(&format!("ss:role-member:{person}:remove"));
    host.answer();
    assert!(!host
        .store
        .member_of(lab, person)
        .unwrap()
        .roles
        .contains(&role));
}

#[test]
fn escape_with_unsaved_changes_keeps_the_settings_open_and_insists() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.write(NAME, "Otro nombre");
    host.key(Key::Escape, false, false);
    assert!(
        host.ui.server_settings.is_some(),
        "nothing is dropped silently"
    );
    assert!(host.settings().nudged);
    assert!(painted_has(
        &mut host,
        "Guardá o restablecé los cambios antes de salir"
    ));
    // Leaving the page is refused the same way.
    host.click("ss:page:roles");
    assert_eq!(host.settings().page, Page::Profile);
    host.click("ss:close");
    assert!(host.ui.server_settings.is_some());
    host.click("ss:reset");
    host.frame();
    assert_eq!(host.tree.input_value(NAME), Some("Laboratorio limn"));
    host.key(Key::Escape, false, false);
    assert!(
        host.ui.server_settings.is_none(),
        "with nothing to save, Esc closes"
    );
}

#[test]
fn escape_closes_a_dialog_before_the_settings() {
    let mut host = Host::new();
    let lab = host.lab();
    let bots = host.role_named(lab, "Bots");
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.press(&format!("ss:role:{bots}:delete"));
    assert_eq!(host.settings().delete_role, Some(bots));
    host.key(Key::Escape, false, false);
    assert!(host.ui.server_settings.is_some());
    assert_eq!(host.settings().delete_role, None);
    host.key(Key::Escape, false, false);
    assert!(host.ui.server_settings.is_none());
}

#[test]
fn deleting_a_role_asks_first_and_goes_back_to_the_list() {
    let mut host = Host::new();
    let lab = host.lab();
    let bots = host.role_named(lab, "Bots");
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.click(&format!("ss:role:{bots}"));
    host.click("ss:editor:menu");
    host.click(&format!("ss:role:{bots}:delete"));
    assert!(painted_has(
        &mut host,
        "¿Seguro que querés eliminar el rol «Bots»?"
    ));
    host.click("ss:role-delete:confirm");
    assert!(matches!(host.sent.last(), Some(Command::DeleteRole { role, .. }) if *role == bots));
    host.answer();
    assert!(host
        .store
        .server(lab)
        .unwrap()
        .roles
        .iter()
        .all(|role| role.id != bots));
    assert_eq!(
        host.settings().role,
        None,
        "the editor of a deleted role closes"
    );
    assert_eq!(host.settings().page, Page::Roles);
}

#[test]
fn everyone_only_has_permissions() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.click("ss:role:everyone");
    assert_eq!(host.settings().role, Some(RoleId(lab.0)));
    assert!(painted_has(&mut host, "Permisos generales del servidor"));
    assert!(!host.on_screen("ss:tab:display"));
    assert!(!host.on_screen("ss:tab:members"));
    host.press(&permission_id(Permissions::MENTION_EVERYONE));
    host.click("ss:save");
    host.answer();
    assert!(host
        .store
        .server(lab)
        .unwrap()
        .everyone()
        .unwrap()
        .permissions
        .contains(Permissions::MENTION_EVERYONE));
}

#[test]
fn a_server_the_account_does_not_own_shows_why_nothing_can_be_changed() {
    let mut host = Host::new();
    let taller = host.server_named("Taller de Rust");
    assert!(!host.store.owns(taller));
    host.open_settings(taller);
    assert!(painted_has(
        &mut host,
        "No tenés permiso para cambiar el servidor en Taller de Rust"
    ));
    assert!(
        !host.on_screen("ss:accent:e74c3c"),
        "the colors are not offered"
    );
    assert!(
        !host.on_screen("ss:delete-server"),
        "only who owns it deletes it"
    );
    host.click("ss:page:roles");
    assert!(painted_has(
        &mut host,
        "No tenés permiso para gestionar roles"
    ));
    assert!(!host.on_screen("ss:role:create"));
    host.click("ss:page:audit");
    assert!(painted_has(&mut host, "ver el registro de auditoría"));
    assert!(
        !host
            .sent
            .iter()
            .any(|command| matches!(command, Command::LoadAudit { .. })),
        "nothing is asked that would be refused"
    );
    host.click("ss:page:members");
    let person = host.someone(taller, None);
    host.press(&format!("ss:member:{person}:kick"));
    assert!(host.settings().removal.is_none(), "kicking is not offered");
    host.press(&format!("ss:member:{person}:menu"));
    assert!(painted_has(&mut host, "No tenés permiso para expulsar"));
}

/// The account, in a server it does not own, given Moderación and with
/// it the permission to manage roles: as a service would report it.
fn make_moderator(host: &mut Host, server: ServerId) -> RoleId {
    let moderation = host.role_named(server, "Moderación");
    let mut found = host.store.server(server).unwrap().clone();
    let role = found
        .roles
        .iter_mut()
        .find(|role| role.id == moderation)
        .unwrap();
    role.permissions = role.permissions | Permissions::MANAGE_ROLES;
    host.event(Event::ServerChanged(found));
    let me = host.store.me().unwrap();
    host.event(Event::MemberChanged(rune_model::Member {
        user: me,
        server,
        nick: None,
        roles: vec![moderation],
        ..Default::default()
    }));
    moderation
}

#[test]
fn stale_server_save_keeps_the_draft_without_sending_or_waiting() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.write(NAME, "Un cambio pendiente");
    host.sent.clear();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .manage_servers = false;
    host.press("ss:save");
    assert!(host.sent.is_empty());
    assert!(!host.settings().saving());
    assert!(host.settings().dirty(&host.store));
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "Tus permisos cambiaron"));
}

#[test]
fn demotion_blocks_pending_role_edits_reorders_and_delete_confirmation() {
    for action in ["edit", "reorder", "delete"] {
        let mut host = Host::new();
        let server = host.server_named("Taller de Rust");
        make_moderator(&mut host, server);
        let bots = host.role_named(server, "Bots");
        host.open_settings(server);
        host.press("ss:page:roles");
        match action {
            "edit" => {
                host.press(&format!("ss:role:{bots}"));
                host.write(ROLE_NAME, "Pendiente");
            }
            "reorder" => host.press(&format!("ss:role:{bots}:up")),
            _ => host.press(&format!("ss:role:{bots}:delete")),
        }
        host.event(Event::MemberChanged(rune_model::Member {
            user: host.store.me().unwrap(),
            server,
            nick: None,
            roles: vec![],
            ..Default::default()
        }));
        host.sent.clear();
        host.press(if action == "delete" {
            "ss:role-delete:confirm"
        } else {
            "ss:save"
        });
        assert!(host.sent.is_empty(), "obsolete {action} must not be sent");
        assert!(!host.settings().saving());
    }
}

#[test]
fn a_permission_lost_after_drafting_cannot_be_granted_by_save() {
    let mut host = Host::new();
    let server = host.server_named("Taller de Rust");
    let moderation = make_moderator(&mut host, server);
    let bots = host.role_named(server, "Bots");
    host.open_settings(server);
    host.press("ss:page:roles");
    host.press(&format!("ss:role:{bots}"));
    host.press("ss:tab:permissions");
    host.press(&permission_id(Permissions::KICK));
    assert!(host.settings().dirty(&host.store));
    let mut changed = host.store.server(server).unwrap().clone();
    let role = changed
        .roles
        .iter_mut()
        .find(|role| role.id == moderation)
        .unwrap();
    role.permissions = role.permissions.without(Permissions::KICK);
    host.event(Event::ServerChanged(changed));
    host.sent.clear();
    host.press("ss:save");
    assert!(host.sent.is_empty());
    assert!(!host.settings().saving());
}

#[test]
fn moderation_confirmations_recheck_the_current_capabilities() {
    for action in ["nick", "kick", "ban"] {
        let mut host = Host::new();
        let server = host.lab();
        let user = host.someone(server, None);
        host.open_settings(server);
        host.press("ss:page:members");
        host.press(&format!("ss:member:{user}:{action}"));
        host.sent.clear();
        host.store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .moderation = false;
        host.press(if action == "nick" {
            "ss:nick:save"
        } else {
            "ss:removal:confirm"
        });
        assert!(host.sent.is_empty(), "obsolete {action} must not be sent");
    }
}

#[test]
fn revoked_read_permissions_stop_automatic_admin_page_reloads() {
    for page in ["audit", "bans"] {
        let mut host = Host::new();
        let server = host.lab();
        host.open_settings(server);
        host.press(&format!("ss:page:{page}"));
        assert!(host.sent.iter().any(|command| matches!(
            command,
            Command::LoadAudit { .. } | Command::LoadBans { .. }
        )));
        host.sent.clear();
        let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
        capabilities.audit_log = false;
        capabilities.moderation = false;
        let user = host.someone(server, None);
        let member = host.store.member_of(server, user).unwrap().clone();
        host.event(Event::MemberChanged(member));
        assert!(host.sent.is_empty(), "revoked {page} must not reload");
    }
}

#[test]
fn a_moderator_cannot_touch_roles_at_or_above_their_own() {
    let mut host = Host::new();
    let taller = host.server_named("Taller de Rust");
    let moderation = make_moderator(&mut host, taller);
    let (collaboration, bots) = (
        host.role_named(taller, "Colaboración"),
        host.role_named(taller, "Bots"),
    );
    host.open_settings(taller);
    host.click("ss:page:roles");
    assert!(
        host.on_screen("ss:role:create"),
        "managing roles is allowed"
    );
    // Nothing goes above the account's own role.
    host.act(UiAction::DragStarted {
        id: format!("ss:role:{bots}"),
        position: Point::new(0.0, 0.0),
    });
    host.act(UiAction::Dropped {
        id: format!("ss:role:{bots}"),
        position: Point::new(0.0, 0.0),
        target: Some(format!("ss:role:{moderation}")),
    });
    assert_eq!(host.settings().order, None);
    host.press(&format!("ss:role:{bots}:up"));
    assert_eq!(
        host.settings().order,
        Some(vec![moderation, bots, collaboration])
    );
    host.click("ss:reset");
    // Its own role opens, read only, saying why.
    host.click(&format!("ss:role:{moderation}"));
    assert!(painted_has(&mut host, "a la altura del tuyo o por encima"));
    host.press("ss:mentionable");
    host.press(&format!("ss:role:{moderation}:delete"));
    assert!(!host.settings().dirty(&host.store));
    assert_eq!(host.settings().delete_role, None);
    // A role below it can be changed, but not with what the account lacks.
    host.press(&format!("ss:role:{collaboration}"));
    host.press("ss:tab:permissions");
    host.press(&permission_id(Permissions::ADMINISTRATOR));
    assert!(
        !host.settings().dirty(&host.store),
        "nobody grants what they lack"
    );
    host.press(&permission_id(Permissions::KICK));
    host.press("ss:save");
    // The promotion above was made up here, so the simulation is not asked.
    assert!(matches!(
        host.sent.last(),
        Some(Command::EditRole { role, edit, .. })
            if *role == collaboration
                && edit.permissions.is_some_and(|bits| bits.contains(Permissions::KICK))
    ));
    host.sent.clear();
    // On the members page, whoever ranks as high cannot be kicked.
    host.event(Event::Failed {
        failure: rune_provider::Failure::Role { server: taller },
        reason: "rechazado".into(),
        ambiguous: false,
    });
    assert!(!host.settings().saving(), "a refusal ends the wait");
    host.click("ss:reset");
    host.click("ss:page:members");
    let peer = host
        .store
        .members(taller)
        .iter()
        .find(|member| {
            member.roles.contains(&moderation)
                && Some(member.user) != host.store.me()
                && Some(member.user) != host.store.server(taller).unwrap().owner
        })
        .map(|member| member.user)
        .expect("someone else moderates the server");
    host.press(&format!("ss:member:{peer}:kick"));
    assert!(host.settings().removal.is_none());
    host.press(&format!("ss:member:{peer}:menu"));
    assert!(painted_has(&mut host, "Su rol está a la altura del tuyo"));
}

#[test]
fn kicking_and_banning_from_the_members_page_send_the_commands() {
    let mut host = Host::new();
    let lab = host.lab();
    let first = host.someone(lab, None);
    host.open_settings(lab);
    host.click("ss:page:members");
    assert!(painted_has(&mut host, "Miembros del servidor"));
    host.click(&format!("ss:member:{first}:menu"));
    host.click(&format!("ss:member:{first}:kick"));
    assert!(host.settings().removal.is_some());
    host.write(REASON, "spam");
    host.click("ss:removal:confirm");
    assert_eq!(
        host.sent.last(),
        Some(&Command::Kick {
            server: lab,
            user: first,
            reason: Some("spam".into()),
        })
    );
    host.answer();
    assert!(host.store.member_of(lab, first).is_none());
    let second = host.someone(lab, None);
    host.press(&format!("ss:member:{second}:ban"));
    host.click("ss:removal:confirm");
    assert_eq!(
        host.sent.last(),
        Some(&Command::Ban {
            server: lab,
            user: second,
            reason: None,
        })
    );
    host.answer();
    assert!(host.store.member_of(lab, second).is_none());
    // The ban shows on its page, and is revoked from there.
    host.click("ss:page:bans");
    host.answer();
    let name = host.store.display_name(second, None);
    assert!(painted_has(&mut host, &name), "the banned person is listed");
    host.click(&format!("ss:ban:{second}:revoke"));
    host.click("ss:unban:confirm");
    assert!(matches!(host.sent.last(), Some(Command::Unban { user, .. }) if *user == second));
    // Gone at once, before the provider answers, and still gone after.
    assert!(!painted_has(&mut host, &name));
    host.answer();
    assert!(!painted_has(&mut host, &name));
}

#[test]
fn a_nickname_is_changed_from_the_member_menu() {
    let mut host = Host::new();
    let lab = host.lab();
    let person = host.someone(lab, None);
    host.open_settings(lab);
    host.click("ss:page:members");
    host.press(&format!("ss:member:{person}:nick"));
    host.write(crate::server_settings::NICK, "Apodada");
    host.click("ss:nick:save");
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetNick {
            server: lab,
            user: person,
            nick: Some("Apodada".into()),
        })
    );
    host.answer();
    assert_eq!(host.store.display_name(person, Some(lab)), "Apodada");
}

#[test]
fn the_audit_log_tells_what_happened_in_sentences() {
    let mut host = Host::new();
    let lab = host.lab();
    let person = host.someone(lab, None);
    let name = host.store.display_name(person, Some(lab));
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.click("ss:role:create");
    host.answer();
    host.click("ss:page:members");
    host.press(&format!("ss:member:{person}:kick"));
    host.write(REASON, "probando");
    host.click("ss:removal:confirm");
    host.answer();
    host.click("ss:page:audit");
    assert!(host
        .sent
        .iter()
        .any(|command| matches!(command, Command::LoadAudit { .. })));
    host.answer();
    let painted = host.painted().join("");
    assert!(
        painted.contains("creó el rol") && painted.contains("nuevo rol"),
        "{painted}"
    );
    assert!(
        painted.contains("expulsó a") && painted.contains(&name),
        "{painted}"
    );
    let kicked = host
        .store
        .audit(lab, &Default::default())
        .unwrap()
        .items
        .iter()
        .find(|entry| matches!(entry.action, rune_model::AuditAction::MemberKicked { .. }))
        .unwrap()
        .id;
    host.click(&format!("ss:audit:{kicked}"));
    let painted = host.painted().join("");
    assert!(
        painted.contains("Motivo: ") && painted.contains("probando"),
        "{painted}"
    );
    // Filtered to role changes only, the kick goes: the provider filters.
    host.press("ss:audit-action:5");
    assert!(
        matches!(host.sent.last(), Some(Command::LoadAudit { query, before: None, .. })
        if query.action == Some(rune_model::AuditActionKind::RoleCreated))
    );
    host.answer();
    let painted = host.painted().join("");
    assert!(
        painted.contains("creó el rol") && !painted.contains("expulsó a"),
        "{painted}"
    );
}

#[test]
fn dragging_roles_reorders_them_and_saving_moves_one_at_a_time() {
    let mut host = Host::new();
    let lab = host.lab();
    let (moderation, collaboration, bots) = (
        host.role_named(lab, "Moderación"),
        host.role_named(lab, "Colaboración"),
        host.role_named(lab, "Bots"),
    );
    host.open_settings(lab);
    host.click("ss:page:roles");
    let drag = |host: &mut Host, role: RoleId, onto: RoleId| {
        let id = format!("ss:role:{role}");
        host.act(UiAction::DragStarted {
            id: id.clone(),
            position: Point::new(0.0, 0.0),
        });
        host.act(UiAction::DragMoved {
            id: id.clone(),
            position: Point::new(0.0, 0.0),
            over: Some(format!("ss:role:{onto}")),
        });
        host.act(UiAction::Dropped {
            id,
            position: Point::new(0.0, 0.0),
            target: Some(format!("ss:role:{onto}")),
        });
    };
    drag(&mut host, bots, moderation);
    drag(&mut host, collaboration, bots);
    assert_eq!(
        host.settings().order,
        Some(vec![collaboration, bots, moderation])
    );
    assert!(painted_has(&mut host, "Tenés cambios sin guardar"));
    host.click("ss:save");
    let answered = host.answer();
    assert_eq!(
        crate::server_settings::ranked(&host.store, lab),
        vec![collaboration, bots, moderation]
    );
    let moves = answered
        .iter()
        .filter(
            |command| matches!(command, Command::EditRole { edit, .. } if edit.position.is_some()),
        )
        .count();
    assert_eq!(moves, 2, "one move per role out of place");
    assert!(!host.settings().dirty(&host.store));
    assert!(!host.settings().saving());
}

#[test]
fn a_real_pointer_drag_on_the_roles_page_moves_the_role() {
    let mut host = Host::new();
    let lab = host.lab();
    let (moderation, bots) = (
        host.role_named(lab, "Moderación"),
        host.role_named(lab, "Bots"),
    );
    host.open_settings(lab);
    host.click("ss:page:roles");
    let frame = host.frame();
    let center = |id: String| {
        let region = frame
            .regions
            .iter()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        Point::new(
            region.rect.x + 40.0,
            region.rect.y + region.rect.height / 2.0,
        )
    };
    let (from, to) = (
        center(format!("ss:role:{bots}")),
        center(format!("ss:role:{moderation}")),
    );
    host.send(UiEvent::PointerMove(from));
    host.send(UiEvent::PointerDown(from));
    host.send(UiEvent::PointerMove(Point::new(from.x, from.y - 20.0)));
    host.send(UiEvent::PointerMove(to));
    host.send(UiEvent::PointerUp(to));
    assert_eq!(
        host.settings()
            .order
            .as_ref()
            .and_then(|order| order.first()),
        Some(&bots),
        "{:?}",
        host.settings().order
    );
}

#[test]
fn a_role_renamed_elsewhere_shows_its_new_name_unless_it_is_being_edited_here() {
    let mut host = Host::new();
    let lab = host.lab();
    let moderation = host.role_named(lab, "Moderación");
    host.open_settings(lab);
    host.click("ss:page:roles");
    host.click(&format!("ss:role:{moderation}"));
    let rename = |host: &mut Host, name: &str| {
        let mut server = host.store.server(lab).unwrap().clone();
        server
            .roles
            .iter_mut()
            .find(|role| role.id == moderation)
            .unwrap()
            .name = name.into();
        host.event(Event::ServerChanged(server));
    };
    rename(&mut host, "Moderadores");
    host.frame();
    assert_eq!(host.tree.input_value(ROLE_NAME), Some("Moderadores"));
    assert!(!host.settings().dirty(&host.store));
    host.write(ROLE_NAME, "Mods");
    rename(&mut host, "Moderación");
    host.frame();
    assert_eq!(
        host.tree.input_value(ROLE_NAME),
        Some("Mods"),
        "what is typed stays"
    );
    assert!(host.settings().dirty(&host.store));
}

#[test]
fn the_settings_close_when_their_server_goes() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.event(Event::ServerRemoved {
        server: lab,
        reason: Some("El servidor se borró".into()),
    });
    assert!(host.ui.server_settings.is_none());
}

#[test]
fn deleting_the_server_asks_for_its_name() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    // A community's longer list ends below the window: scrolled, as a
    // person would (rules and onboarding, 2026-10-18).
    host.tree.scroll_to_end("ss:sidebar");
    host.click("ss:delete-server");
    assert!(painted_has(&mut host, "Eliminar «Laboratorio limn»"));
    assert!(
        !host.on_screen("ss:delete-server:confirm"),
        "not before the name"
    );
    host.write(crate::server_settings::DELETE_NAME, "Laboratorio");
    assert!(!host.on_screen("ss:delete-server:confirm"));
    host.write(crate::server_settings::DELETE_NAME, "Laboratorio limn");
    host.click("ss:delete-server:confirm");
    assert_eq!(
        host.sent.last(),
        Some(&Command::DeleteServer { server: lab })
    );
    host.answer();
    assert!(host.store.server(lab).is_none());
    assert!(host.ui.server_settings.is_none());
    assert_ne!(host.ui.server, Some(lab), "the window went somewhere else");
}

#[test]
fn the_settings_lay_out_at_every_width_without_spilling() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    for page in ["profile", "members", "roles", "audit", "bans"] {
        host.press(&format!("ss:page:{page}"));
        host.answer();
        for width in [720.0, 1024.0, 1600.0] {
            let size = Size::new(width, 700.0);
            host.tree.set_root(view(
                &host.ui,
                &host.store,
                &ViewContext {
                    window: size,
                    ..host.context()
                },
            ));
            let frame = host.tree.layout(size);
            for region in frame
                .regions
                .iter()
                .filter(|region| region.kind == limn::HitKind::Button)
            {
                assert!(
                    region.rect.x + region.rect.width <= width + 1.0,
                    "{page}: {} spills at {width}: {:?}",
                    region.id,
                    region.rect
                );
            }
        }
    }
}

/// Whether a notice titled `title` says `detail`.
fn noticed(host: &Host, title: &str, detail: &str) -> bool {
    host.ui.notices.iter().any(|notice| {
        notice.title == title
            && notice
                .detail
                .as_deref()
                .is_some_and(|said| said.contains(detail))
    })
}

#[test]
fn the_audit_log_loads_more_from_its_oldest_entry_without_repeating() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:audit");
    host.answer();
    let query = Default::default();
    let first = host.store.audit(lab, &query).unwrap().clone();
    assert_eq!(first.items.len(), crate::server_settings::PAGE);
    assert!(!first.reached_end);
    // Something happens meanwhile: the head is asked for again.
    host.click("ss:page:roles");
    host.click("ss:role:create");
    host.answer();
    host.click("ss:page:audit");
    host.answer();
    // At the end of the scrolled list: pressed as limn reports it.
    host.press("ss:audit:more");
    let oldest = host
        .store
        .audit(lab, &query)
        .unwrap()
        .items
        .last()
        .unwrap()
        .id;
    assert!(
        matches!(host.sent.last(), Some(Command::LoadAudit { before: Some(before), .. }) if *before == oldest)
    );
    host.answer();
    let all = host.store.audit(lab, &query).unwrap();
    assert!(all.reached_end);
    let mut ids: Vec<u64> = all.items.iter().map(|entry| entry.id).collect();
    assert!(ids.windows(2).all(|pair| pair[0] > pair[1]), "newest first");
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "no entry twice");
    host.sent.clear();
    host.press("ss:audit:more");
    assert!(host.sent.is_empty(), "nothing more to load");
}

#[test]
fn bans_load_more_after_the_last_one_and_the_search_says_what_it_read() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:bans");
    let banned = UserId(10);
    // A provider with more than a page: the first one ends at `banned`.
    host.sent.clear();
    host.event(Event::BansLoaded {
        request: 1,
        server: lab,
        after: None,
        bans: vec![rune_model::Ban {
            user: banned,
            reason: None,
        }],
        reached_end: false,
    });
    host.write(crate::server_settings::BANS_QUERY, "zz");
    assert!(painted_has(
        &mut host,
        "Se buscó entre los 1 vetos cargados."
    ));
    host.click("ss:bans:more");
    assert!(
        matches!(host.sent.last(), Some(Command::LoadBans { after: Some(after), .. }) if *after == banned)
    );
}

#[test]
fn revoking_an_invitation_asks_first_and_shows_at_once() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:invites");
    host.answer();
    let invites = host.store.invites(lab).unwrap().to_vec();
    let code = invites[0].code.clone();
    let painted = host.painted().join("");
    assert!(
        painted.contains(&code) && painted.contains("Creada por"),
        "{painted}"
    );
    assert!(
        painted.contains("No vence") && painted.contains("Vence: "),
        "{painted}"
    );
    host.click(&format!("ss:invite:{code}:revoke"));
    assert_eq!(
        host.settings().revoke_invite.as_deref(),
        Some(code.as_str())
    );
    assert!(host.sent.is_empty(), "nothing before confirming");
    host.click("ss:invite-revoke:confirm");
    assert_eq!(
        host.sent.last(),
        Some(&Command::RevokeInvite {
            server: lab,
            code: code.clone()
        })
    );
    assert!(!painted_has(&mut host, &code), "gone before the answer");
    host.answer();
    assert_eq!(host.store.invites(lab).unwrap().len(), invites.len() - 1);
    assert!(!painted_has(&mut host, &code));
}

#[test]
fn a_permission_lost_before_confirming_sends_nothing_and_says_why() {
    for (page, open, confirm) in [
        ("invites", "invite", "ss:invite-revoke:confirm"),
        ("bans", "ban", "ss:unban:confirm"),
        ("emojis", "emoji", "ss:expression-delete:confirm"),
        ("stickers", "rename", "ss:rename:save"),
    ] {
        let mut host = Host::new();
        let lab = host.lab();
        host.open_settings(lab);
        host.click(&format!("ss:page:{page}"));
        host.answer();
        let server = host.store.server(lab).unwrap().clone();
        let target = match open {
            "invite" => format!(
                "ss:invite:{}:revoke",
                host.store.invites(lab).unwrap()[0].code
            ),
            "ban" => format!(
                "ss:ban:{}:revoke",
                host.store.bans(lab).unwrap().items[0].user
            ),
            "emoji" => format!("ss:emoji:{}:delete", server.emojis[0].id),
            _ => format!("ss:sticker:{}:rename", server.stickers[0].id),
        };
        host.click(&target);
        host.sent.clear();
        let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
        capabilities.manage_invites = false;
        capabilities.moderation = false;
        capabilities.manage_expressions = false;
        host.press(confirm);
        assert!(host.sent.is_empty(), "{page}: nothing is sent");
        assert!(
            host.ui.notices.iter().any(|notice| notice.error
                && notice
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.starts_with("Este proveedor no permite"))),
            "{page}: the notice says why"
        );
        let settings = host.settings();
        assert!(
            settings.revoke_invite.is_none()
                && settings.unban.is_none()
                && settings.delete_expression.is_none()
                && settings.rename.is_none(),
            "{page}: the dialog closed"
        );
    }
}

#[test]
fn a_demotion_before_confirming_a_kick_sends_nothing_and_says_why() {
    let mut host = Host::new();
    let taller = host.server_named("Taller de Rust");
    let moderation = make_moderator(&mut host, taller);
    let target = host.someone(taller, Some(moderation));
    host.open_settings(taller);
    host.click("ss:page:members");
    host.press(&format!("ss:member:{target}:kick"));
    assert!(host.settings().removal.is_some());
    // Their role rises to the account's own before it confirms.
    host.event(Event::MemberChanged(rune_model::Member {
        user: target,
        server: taller,
        nick: None,
        roles: vec![moderation],
        ..Default::default()
    }));
    host.sent.clear();
    host.press("ss:removal:confirm");
    assert!(host.sent.is_empty());
    assert!(noticed(
        &host,
        "No se sacó a la persona",
        "a la altura del tuyo"
    ));
}

#[test]
fn emojis_and_stickers_are_renamed_and_deleted_after_asking() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:emojis");
    let painted = host.painted().join("");
    assert!(
        painted.contains(":ferris:")
            && painted.contains("Solo para Moderación")
            && painted.contains("Subir emoji")
            && painted.contains("128 × 128"),
        "{painted}"
    );
    let emoji = host.store.server(lab).unwrap().emojis[0].clone();
    host.click(&format!("ss:emoji:{}:rename", emoji.id));
    // An invalid name keeps the dialog open and sends nothing.
    host.write(crate::server_settings::EXPRESSION_NAME, "con espacio");
    host.press("ss:rename:save");
    assert!(host.settings().rename.is_some());
    assert!(host.sent.is_empty());
    host.write(crate::server_settings::EXPRESSION_NAME, "cangrejo");
    host.click("ss:rename:save");
    let rename = Command::RenameExpression {
        server: lab,
        expression: rune_model::ExpressionId::Emoji(emoji.id),
        name: "cangrejo".into(),
    };
    assert_eq!(host.sent.last(), Some(&rename));
    assert!(
        painted_has(&mut host, ":cangrejo:"),
        "shown before the answer"
    );
    // A refusal puts the old name back.
    host.sent.clear();
    host.event(Event::Failed {
        failure: rune_provider::Failure::of(&rename).unwrap(),
        reason: "rechazado".into(),
        ambiguous: false,
    });
    assert!(painted_has(&mut host, ":ferris:"));
    // Deleting a sticker asks first.
    host.click("ss:page:stickers");
    let sticker = host.store.server(lab).unwrap().stickers[0].clone();
    host.click(&format!("ss:sticker:{}:delete", sticker.id));
    assert!(painted_has(&mut host, "Eliminar sticker"));
    host.click("ss:expression-delete:confirm");
    assert!(matches!(
        host.sent.last(),
        Some(Command::DeleteExpression { .. })
    ));
    host.answer();
    assert!(!host
        .store
        .server(lab)
        .unwrap()
        .stickers
        .iter()
        .any(|known| known.id == sticker.id));
    assert!(!painted_has(&mut host, &sticker.name));
}

// Administration (19).

/// The intents a click gives, without carrying them out.
fn intents_of(host: &mut Host, id: &str) -> Vec<Intent> {
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    )
}

#[test]
fn a_timeout_is_chosen_confirmed_and_then_offered_to_lift() {
    let mut host = Host::new();
    let lab = host.lab();
    let target = host.someone(lab, None);
    host.open_settings(lab);
    host.click("ss:page:members");
    host.answer();
    // Only someone of the server.
    host.press("ss:member:999999:timeout");
    assert!(host.settings().advanced.timeout.is_none());
    host.press(&format!("ss:member:{target}:timeout"));
    assert!(host.settings().advanced.timeout.is_some());
    host.press("ss:timeout-length:3");
    host.write(crate::server_settings::advanced::TIMEOUT_REASON, "spam");
    host.sent.clear();
    host.press("ss:timeout:confirm");
    let until = host.store.wall().plus_minutes(60);
    assert_eq!(
        host.sent,
        [Command::TimeoutMember {
            server: lab,
            user: target,
            until: Some(until),
            reason: Some("spam".into()),
        }]
    );
    host.answer();
    assert_eq!(
        host.store
            .member_of(lab, target)
            .unwrap()
            .restrictions
            .timeout_until,
        Some(until)
    );
    host.press(&format!("ss:member:{target}:menu"));
    assert!(painted_has(&mut host, "Quitar el aislamiento"));
    host.press(&format!("ss:member:{target}:untimeout"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::TimeoutMember { until: None, .. })
    ));
}

#[test]
fn a_new_invitation_is_made_with_its_choices_and_its_code_copied() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:invites");
    host.answer();
    host.click("ss:invite:new");
    assert!(painted_has(&mut host, "Crear invitación"));
    host.press("ss:invite-age:6");
    host.press("ss:invite-uses:2");
    host.press("ss:invite:temporary");
    host.sent.clear();
    host.press("ss:invite:create");
    let Some(Command::CreateInvite {
        max_age_secs,
        max_uses,
        temporary,
        ..
    }) = host.sent.last().cloned()
    else {
        panic!("{:?}", host.sent);
    };
    assert_eq!((max_age_secs, max_uses, temporary), (0, 5, true));
    // Twice is still one invitation.
    host.press("ss:invite:create");
    assert_eq!(host.sent.len(), 1);
    host.answer();
    let code = host
        .settings()
        .advanced
        .invite
        .as_ref()
        .unwrap()
        .made
        .clone()
        .unwrap();
    assert_eq!(host.store.invites(lab).unwrap()[0].code, code);
    assert!(painted_has(&mut host, "Invitación creada"));
    // The simulation has no links: the code itself is copied, and it says so.
    assert_eq!(
        intents_of(&mut host, "ss:invite:copy"),
        [Intent::Copy(code.clone())]
    );
    assert!(painted_has(&mut host, "no lleva a ningún lado"));
    host.press("ss:invite:done");
    assert_eq!(
        intents_of(&mut host, &format!("ss:invite:{code}:copy")),
        [Intent::Copy(code)]
    );
}

#[test]
fn a_new_emoji_is_picked_named_and_added() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:emojis");
    host.click("ss:upload:emoji");
    let Some(Intent::PickExpressionImage { request, kind }) = host.upload_intents.pop() else {
        panic!("the host is asked for a picture");
    };
    assert_eq!(kind, rune_provider::ExpressionKind::Emoji);
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(64u32.to_be_bytes());
    png.extend(64u32.to_be_bytes());
    let image = rune_provider::ExpressionImage::png(kind, png.into()).unwrap();
    // A late answer to another choice opens nothing.
    crate::expression_image_picked(
        &mut host.ui,
        request + 1,
        Ok(Some((image.clone(), super::attachments::image()))),
        host.now,
    );
    assert!(host.settings().advanced.upload.is_none());
    let intents = crate::expression_image_picked(
        &mut host.ui,
        request,
        Ok(Some((image, super::attachments::image()))),
        host.now,
    );
    host.run(intents);
    assert!(painted_has(&mut host, "Subir emoji"));
    let name = crate::server_settings::advanced::UPLOAD_NAME;
    host.write(name, "con espacio");
    host.sent.clear();
    host.press("ss:upload:confirm");
    assert!(host.sent.is_empty(), "an invalid name sends nothing");
    host.write(name, "cangrejo");
    host.press("ss:upload:confirm");
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateExpression { expression, .. }) if expression.name == "cangrejo"
    ));
    host.answer();
    assert!(host.settings().advanced.upload.is_none());
    assert!(host
        .store
        .server(lab)
        .unwrap()
        .emojis
        .iter()
        .any(|emoji| emoji.name == "cangrejo"));
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "Emoji agregado"));
}

#[test]
fn a_prune_shows_its_count_before_removing_anyone() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:members");
    host.answer();
    host.sent.clear();
    host.click("ss:prune");
    assert!(matches!(
        host.sent.last(),
        Some(Command::PruneMembers {
            days: 7,
            remove: false,
            ..
        })
    ));
    host.answer();
    let count = host.settings().advanced.prune.as_ref().unwrap().count;
    assert!(count.is_some(), "counted");
    host.press("ss:prune-days:2");
    assert!(matches!(
        host.sent.last(),
        Some(Command::PruneMembers {
            days: 30,
            remove: false,
            ..
        })
    ));
    host.answer();
    let members = host.store.members(lab).len();
    let would = host
        .settings()
        .advanced
        .prune
        .as_ref()
        .unwrap()
        .count
        .flatten()
        .unwrap();
    host.press("ss:prune:confirm");
    if would == 0 {
        assert!(host.settings().advanced.prune.is_some(), "nothing to prune");
        return;
    }
    host.answer();
    assert!(host.settings().advanced.prune.is_none());
    assert_eq!(host.store.members(lab).len(), members - would as usize);
}

#[test]
fn webhooks_and_integrations_are_listed_made_changed_and_removed() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:integrations");
    host.answer();
    let webhooks = host.store.webhooks(lab).unwrap().to_vec();
    assert!(painted_has(&mut host, &webhooks[0].name));
    host.click("ss:webhook:new");
    let name = crate::server_settings::advanced::WEBHOOK_NAME;
    host.write(name, "Despliegues");
    host.sent.clear();
    host.press("ss:webhook:save");
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateWebhook { name, .. }) if name == "Despliegues"
    ));
    host.answer();
    let made = host
        .store
        .webhooks(lab)
        .unwrap()
        .iter()
        .find(|webhook| webhook.name == "Despliegues")
        .unwrap()
        .id;
    // Renamed only: the channel is not sent again.
    host.press(&format!("ss:webhook:{made}:edit"));
    host.write(name, "Despliegues de CI");
    host.press("ss:webhook:save");
    assert!(matches!(
        host.sent.last(),
        Some(Command::EditWebhook {
            name: Some(_),
            channel: None,
            ..
        })
    ));
    host.answer();
    host.press(&format!("ss:webhook:{made}:delete"));
    host.press("ss:webhook-delete:confirm");
    // Gone at once.
    assert!(!host
        .store
        .webhooks(lab)
        .unwrap()
        .iter()
        .any(|webhook| webhook.id == made));
    host.answer();
    let integration = host.store.integrations(lab).unwrap()[0].clone();
    host.press(&format!("ss:integration:{}:delete", integration.id));
    assert!(painted_has(&mut host, "Quitar integración"));
    host.press("ss:integration-delete:confirm");
    host.answer();
    assert!(!host
        .store
        .integrations(lab)
        .unwrap()
        .iter()
        .any(|known| known.id == integration.id));
}

#[test]
fn more_members_load_from_where_the_pages_end() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.sent.clear();
    host.click("ss:page:members");
    assert!(matches!(
        host.sent.last(),
        Some(Command::LoadMembers { after: None, .. })
    ));
    host.answer();
    let pages = host.store.member_pages(lab).unwrap().clone();
    match pages.next {
        Some(next) => {
            host.click("ss:members:more");
            assert!(matches!(
                host.sent.last(),
                Some(Command::LoadMembers { after: Some(after), .. }) if *after == next
            ));
        }
        None => assert!(!host.on_screen("ss:members:more")),
    }
}

#[test]
fn a_permission_lost_before_an_invitation_sends_nothing() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:invites");
    host.press("ss:invite:new");
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .create_invites = false;
    host.sent.clear();
    host.press("ss:invite:create");
    assert!(host.sent.is_empty());
    assert!(noticed(
        &host,
        "No se creó la invitación",
        "Este proveedor no permite"
    ));
    assert!(host.settings().advanced.invite.is_none());
}

// Server administration, rest (P7).

fn png(edge: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(edge.to_be_bytes());
    png.extend(edge.to_be_bytes());
    png
}

#[test]
fn the_profile_reads_its_traits_and_saves_them_with_a_new_picture() {
    let mut host = Host::new();
    let lab = host.lab();
    // Traits not read yet, as a session starts: opening reads them once.
    let mut unread = host.store.server(lab).unwrap().clone();
    unread.options.traits = None;
    host.event(Event::ServerChanged(unread));
    host.open_settings(lab);
    assert!(host.sent.iter().any(
        |command| matches!(command, Command::LoadServerProfile { server, .. } if *server == lab)
    ));
    host.answer();
    assert!(painted_has(&mut host, "🦀 Rust"), "the card shows them");
    // Below the icon and the banner (2026-10-19): scrolled into view.
    host.tree.reveal_at_start("ss:trait:add", 400.0);
    assert!(painted_has(&mut host, "Rasgos"));
    assert!(!painted_has(&mut host, "Tenés cambios sin guardar"));
    host.press("ss:trait:add");
    host.write("ss:trait:3:label", "Gráficos");
    host.write("ss:trait:3:emoji", "🖼️");
    host.press("ss:icon:pick");
    let Some(Intent::PickProfileImage {
        request,
        kind,
        animated,
    }) = host.upload_intents.pop()
    else {
        panic!("the host is asked for a picture");
    };
    assert_eq!(kind, rune_provider::ProfileImageKind::Avatar);
    assert!(animated, "the lab's boosts keep a moving icon moving");
    let image = rune_provider::ProfileImage::new(png(512).into()).unwrap();
    let intents = crate::profile_image_picked(
        &mut host.ui,
        request,
        Ok(Some((image, super::attachments::image()))),
        None,
        host.now,
    );
    host.run(intents);
    assert!(painted_has(&mut host, "Tenés cambios sin guardar"));
    host.sent.clear();
    host.press("ss:save");
    let Some(Command::EditServer { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert_eq!(edit.traits.as_ref().map(Vec::len), Some(4));
    assert!(matches!(edit.icon, Some(Some(_))));
    assert!(edit.name.is_none() && edit.verification.is_none());
    host.answer();
    let server = host.store.server(lab).unwrap();
    assert_eq!(server.options.traits.as_ref().unwrap()[3].label, "Gráficos");
    assert!(server.icon.is_some());
    assert!(!host.settings().dirty(&host.store));
}

// Icon, banner and community (2026-10-19).

#[test]
fn a_banner_is_chosen_still_where_only_a_still_one_is_unlocked_and_a_trait_takes_a_server_emoji() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.tree.reveal_at_start("ss:banner:pick", 200.0);
    assert!(painted_has(&mut host, "Banner"));
    assert!(painted_has(&mut host, "quieto"), "{:?}", host.painted());
    host.press("ss:banner:pick");
    let Some(Intent::PickProfileImage {
        request,
        kind,
        animated,
    }) = host.upload_intents.pop()
    else {
        panic!("the host is asked for a picture");
    };
    assert_eq!(kind, rune_provider::ProfileImageKind::ServerBanner);
    assert!(!animated, "a moving banner is not unlocked");
    let image = rune_provider::ProfileImage::new(png(512).into()).unwrap();
    let intents = crate::profile_image_picked(
        &mut host.ui,
        request,
        Ok(Some((image, super::attachments::image()))),
        None,
        host.now,
    );
    host.run(intents);
    // One of the server's own emojis by its name.
    let own = host.store.server(lab).unwrap().emojis[0].clone();
    host.write("ss:trait:0:emoji", &format!(":{}:", own.name));
    host.sent.clear();
    host.press("ss:save");
    let Some(Command::EditServer { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert!(matches!(edit.banner, Some(Some(_))) && edit.icon.is_none());
    let traits = edit.traits.unwrap();
    assert_eq!(traits[0].emoji_id, Some(own.id));
    assert_eq!(traits[0].emoji.as_deref(), Some(own.name.as_str()));
    host.answer();
    let options = &host.store.server(lab).unwrap().options;
    assert!(options.banner.is_some());
    assert!(!host.settings().dirty(&host.store));
    host.tree.scroll_to_start("ss:scroll:profile");
    assert!(painted_has(&mut host, &format!(":{}: Rust", own.name)));
}

#[test]
fn a_community_is_left_and_made_again_only_with_its_channels() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:safety");
    host.tree.reveal_at_start("ss:community", 200.0);
    assert!(painted_has(&mut host, "Es una comunidad"));
    assert!(painted_has(&mut host, "Canal de reglas"));
    host.press("ss:community");
    host.sent.clear();
    host.press("ss:save");
    let Some(Command::EditServer { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert_eq!(edit.community, Some(false));
    host.answer();
    assert!(!host.store.server(lab).unwrap().options.community);
    // Again, without a rules channel: said, not sent.
    host.press("ss:community");
    host.press("ss:set:rules:none");
    host.sent.clear();
    host.press("ss:save");
    assert!(host.sent.is_empty(), "{:?}", host.sent);
    assert!(noticed(
        &host,
        "No se guardó la comunidad",
        "canal de reglas"
    ));
    let rules = host
        .store
        .server(lab)
        .unwrap()
        .options
        .updates_channel
        .unwrap();
    host.press(&format!("ss:set:rules:{rules}"));
    host.press("ss:save");
    let Some(Command::EditServer { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert_eq!(edit.community, Some(true));
    assert_eq!(edit.media_scan, None, "already everyone's");
    host.answer();
    let options = &host.store.server(lab).unwrap().options;
    assert!(options.community && options.community_ready());
}

#[test]
fn safety_settings_are_drafted_together_and_refused_where_not_managed() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:safety");
    assert!(painted_has(&mut host, "Canal de mensajes del sistema"));
    host.press("ss:pick:verification");
    host.press("ss:set:verification:3");
    host.press("ss:system-messages:joins");
    host.press("ss:set:afk-timeout:900");
    host.press("ss:set:system:none");
    host.press("ss:set:notify:mentions");
    assert!(painted_has(&mut host, "Tenés cambios sin guardar"));
    host.sent.clear();
    host.press("ss:save");
    let Some(Command::EditServer { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert_eq!(edit.verification, Some(rune_model::Verification::High));
    assert_eq!(
        edit.system_messages.map(|messages| messages.joins),
        Some(false)
    );
    assert_eq!(edit.afk_timeout, Some(900));
    assert_eq!(edit.system_channel, Some(None));
    assert_eq!(edit.mentions_only, Some(true));
    host.answer();
    let options = &host.store.server(lab).unwrap().options;
    assert!(options.system_channel.is_none() && options.mentions_only);
    assert!(!host.settings().dirty(&host.store));
    // A server the account does not manage: the page says why, nothing drafts.
    host.press("ss:close");
    let other = host.server_named("Café Nocturno");
    host.open_settings(other);
    host.click("ss:page:safety");
    assert!(painted_has(&mut host, "No tenés permiso"));
    host.press("ss:set:verification:4");
    assert!(!host.settings().dirty(&host.store));
}

#[test]
fn invitations_are_paused_and_resumed_at_once() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:invites");
    host.sent.clear();
    host.click("ss:invites:pause");
    assert!(matches!(host.sent.last(),
        Some(Command::EditServer { edit, .. }) if edit.invites_paused == Some(true)));
    host.answer();
    assert!(painted_has(&mut host, "Las invitaciones están pausadas"));
    host.click("ss:invites:resume");
    assert!(matches!(host.sent.last(),
        Some(Command::EditServer { edit, .. }) if edit.invites_paused == Some(false)));
}

#[test]
fn channels_move_from_their_menu_and_by_dragging_and_save_in_one_batch() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:channels");
    let named = |host: &Host, name: &str| {
        host.store
            .channel_tree(lab)
            .into_iter()
            .flat_map(|(category, channels)| category.into_iter().chain(channels))
            .find(|channel| channel.name == name)
            .unwrap()
            .id
    };
    let components = named(&host, "componentes");
    let voice = named(&host, "Voz");
    let core = named(&host, "limn-core");
    host.press(&format!("ss:chan:{components}:to:{voice}"));
    assert!(painted_has(&mut host, "Tenés cambios sin guardar"));
    // Dragged above `limn-core`, it stays in that category instead.
    host.act(UiAction::DragStarted {
        id: format!("ss:chan:{core}"),
        position: limn::Point::new(0.0, 0.0),
    });
    host.act(UiAction::Dropped {
        id: format!("ss:chan:{core}"),
        target: Some(format!("ss:chan:{voice}")),
        position: limn::Point::new(0.0, 0.0),
    });
    host.sent.clear();
    host.press("ss:save");
    let Some(Command::ReorderChannels { moves, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert!(moves
        .iter()
        .any(|step| step.channel == components && step.parent == Some(voice)));
    host.answer();
    assert_eq!(host.store.channel(components).unwrap().parent, Some(voice));
    assert!(!host.settings().dirty(&host.store));
}

#[test]
fn a_role_takes_an_emoji_a_picture_and_a_gradient_and_counts_its_members() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:roles");
    assert!(host
        .sent
        .iter()
        .any(|command| matches!(command, Command::LoadRoleCounts { .. })));
    host.answer();
    let role = host.store.server(lab).unwrap().assignable_roles()[0].id;
    assert!(host.store.role_count(lab, role).is_some());
    host.press(&format!("ss:role:{role}"));
    // The simulation's roles may show a style already: change both.
    let before = host
        .store
        .server(lab)
        .unwrap()
        .roles
        .iter()
        .find(|found| found.id == role)
        .unwrap()
        .style
        .clone();
    let mut gradient = before.gradient.clone();
    match gradient.first_mut() {
        Some(first) => *first = rune_model::Rgb(0x123456),
        None => gradient.push(rune_model::Rgb(0x123456)),
    }
    host.write(crate::server_settings::rest::ROLE_EMOJI, "🧪");
    host.write("ss:gradient2:hex", "123456");
    host.sent.clear();
    host.press("ss:save");
    let Some(Command::EditRole { edit, .. }) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent);
    };
    assert_eq!(
        edit.icon,
        Some(Some(rune_provider::RoleIcon::Emoji("🧪".into())))
    );
    assert_eq!(edit.gradient, Some(gradient.clone()));
    host.answer();
    let saved = host
        .store
        .server(lab)
        .unwrap()
        .roles
        .iter()
        .find(|found| found.id == role)
        .unwrap()
        .style
        .clone();
    assert_eq!(saved.emoji.as_deref(), Some("🧪"));
    assert_eq!(saved.gradient, gradient);
    // Taken away, the gradient goes with its switch.
    host.press("ss:gradient");
    assert_eq!(host.settings().role_edit.gradient, Some(Vec::new()));
    host.press("ss:reset");
    // A picture instead: chosen as an emoji's, still.
    host.press("ss:role-icon:pick");
    let Some(Intent::PickExpressionImage { request, .. }) = host.upload_intents.pop() else {
        panic!("the host is asked for a picture");
    };
    let image =
        rune_provider::ExpressionImage::png(rune_provider::ExpressionKind::Emoji, png(64).into())
            .unwrap();
    let intents = crate::expression_image_picked(
        &mut host.ui,
        request,
        Ok(Some((image, super::attachments::image()))),
        host.now,
    );
    host.run(intents);
    assert!(matches!(
        host.settings().role_edit.icon,
        Some(Some(rune_provider::RoleIcon::Image(_)))
    ));
    assert!(
        host.settings().advanced.upload.is_none(),
        "not an emoji upload"
    );
}

#[test]
fn members_are_ordered_filtered_and_say_how_they_joined() {
    let mut host = Host::new();
    let lab = host.lab();
    host.open_settings(lab);
    host.click("ss:page:members");
    host.answer();
    host.sent.clear();
    host.press("ss:set:member-order:3");
    assert!(matches!(host.sent.last(),
        Some(Command::LoadMembers { after: None, query, .. })
            if query.order == rune_model::MemberOrder::OldestAccounts && !query.recent));
    host.answer();
    assert!(painted_has(&mut host, "(simulado)"));
    host.press("ss:members:recent");
    assert!(matches!(host.sent.last(),
        Some(Command::LoadMembers { query, .. }) if query.recent));
    host.answer();
    let listed = host.store.member_pages(lab).unwrap().listed.clone();
    assert!(!listed.is_empty());
    assert!(
        listed.len() < host.store.members(lab).len(),
        "only recent joins"
    );
}

/// The people the settings list ask for their pictures, and their rows draw
/// them (2026-10-21: every row kept its initials, as nothing asked).
#[test]
fn the_settings_ask_for_the_faces_they_list() {
    use rune_model::RemoteImage;
    let mut host = Host::new();
    let lab = host.lab();
    // `user` given a picture of their own: the one the settings show.
    let pictured = |host: &mut Host, user: UserId| -> RemoteImage {
        let mut account = host.store.user(user).unwrap().clone();
        account.avatar = Some(RemoteImage {
            url: format!("https://cdn.example.test/face-{user}.png"),
            kind: Default::default(),
        });
        host.event(Event::UserChanged(account));
        host.frame();
        host.store.avatar_in(Some(lab), user).unwrap().clone()
    };
    host.open_settings(lab);
    host.click("ss:page:invites");
    host.answer();
    let inviter = host
        .store
        .invites(lab)
        .unwrap()
        .iter()
        .find_map(|invite| invite.inviter)
        .expect("an invitation says who made it");
    let face = pictured(&mut host, inviter);
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&face));
    host.click("ss:page:members");
    host.answer();
    host.frame();
    let shown = host
        .tree
        .frame()
        .regions
        .iter()
        .find_map(|region| {
            region
                .id
                .strip_prefix("ss:member:")?
                .strip_suffix(":menu")?
                .parse::<u64>()
                .ok()
        })
        .map(UserId)
        .expect("a member's row is in view");
    let face = pictured(&mut host, shown);
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&face));
}
