//! What the settings of a server show, from their state and the store.

use limn::{
    AlertDialog, Button, ButtonVariant, Component, DropdownOption, Element, Empty, FormField, Icon,
    Modal, SearchField, Select, TextArea, TextField,
};
use rune_components::server_settings::{
    self as parts, audit_details, audit_label, audit_sentence, AuditKind, AuditLine, BanLine,
    ExpressionLine, InviteLine, MemberLine, MenuAction, Metrics, PersonLine, RoleChoice, RoleItem,
    RoleLine, ServerCard, SidebarEntry, Swatches, UnsavedBar, AUDIT_ACTIONS, DEFAULT_ROLE_COLOR,
};
use rune_components::{gap, line, strong_line, text, time, Look};
use rune_i18n::{t, tr};
use rune_model::{ExpressionId, Role, RoleId, Server, UserId};
use rune_provider::ProviderKind;
use rune_state::{ServerAbilities, Store};

use super::{
    grant_lock, member_lock, nick_lock, permission_lock, role_lock, shown_order, shown_role,
    shown_server, Page, RoleTab, ServerSettings, ADD_QUERY, BANS_QUERY, DELETE_NAME, DESCRIPTION,
    EXPRESSION_NAME, MEMBERS_QUERY, NAME, NICK, PERMISSIONS_QUERY, REASON, ROLES_QUERY,
    ROLE_MEMBERS_QUERY, ROLE_NAME,
};
use crate::{Ui, ViewContext};

mod admin;
// Server administration, rest (P7).
mod rest;
// Rules and onboarding (2026-10-18).
mod onboarding;
// AutoMod (automod).
mod automod;

/// The height of the settings' text fields, as the reference measures them.
const FIELD_HEIGHT: f32 = 41.0;

/// The settings over the whole window, while they are open.
pub fn view(ui: &Ui, store: &Store, context: &ViewContext) -> Option<Element> {
    let settings = ui.server_settings.as_ref()?;
    let server = store.server(settings.server)?;
    let abilities = store.server_abilities(server.id)?;
    let look = &ui.look();
    let editing = settings
        .role
        .and_then(|id| server.roles.iter().find(|role| role.id == id))
        .filter(|_| settings.page == Page::Roles);
    let wide = editing.is_none() && matches!(settings.page, Page::Profile | Page::Members);
    let metrics = Metrics::new(context.window.width, wide);
    let view = View {
        settings,
        store,
        server,
        abilities: &abilities,
        look,
        metrics,
        developer: ui.prefs.developer,
        ui,
    };
    let region = match (settings.page, editing) {
        (Page::Roles, Some(role)) => view.editor(role),
        (Page::Roles, None) => view.roles(),
        (Page::Profile, _) => view.profile(),
        (Page::Members, _) => view.members(),
        (Page::Invites, _) => view.invites(context),
        (Page::Emojis, _) => view.expressions(false),
        (Page::Stickers, _) => view.expressions(true),
        (Page::Audit, _) => view.audit(context),
        (Page::Bans, _) => view.bans(),
        (Page::Integrations, _) => view.integrations(),
        (Page::Safety, _) => view.safety(),
        (Page::Channels, _) => view.channels(),
        (Page::Onboarding, _) => view.onboarding(),
        // AutoMod (automod).
        (Page::AutoMod, _) => view.automod(),
    };
    let bar = settings.dirty(store).then(|| {
        parts::unsaved_bar(
            UnsavedBar {
                saving: settings.saving(),
                nudged: settings.nudged,
            },
            look,
        )
    });
    let root = parts::shell(context.window, metrics, view.sidebar(), region, bar, look);
    Some(view.dialogs(root))
}

/// What every part of the view reads.
struct View<'a> {
    settings: &'a ServerSettings,
    store: &'a Store,
    server: &'a Server,
    abilities: &'a ServerAbilities,
    look: &'a Look,
    metrics: Metrics,
    /// Developer mode: the menus offer to copy ids.
    developer: bool,
    /// For the pictures of expressions already loaded.
    ui: &'a Ui,
}

impl View<'_> {
    /// `menu` without «Copiar ID» unless in developer mode.
    fn ids(&self, mut menu: Vec<MenuAction>) -> Vec<MenuAction> {
        menu.retain(|action| self.developer || !action.id.ends_with(":copy-id"));
        menu
    }
}

impl View<'_> {
    fn sidebar(&self) -> Element {
        let page = |id: &str, label: &str, page: Page| SidebarEntry::Item {
            id: format!("ss:page:{id}"),
            label: label.into(),
            selected: self.settings.page == page,
            danger: false,
        };
        let mut entries = vec![
            SidebarEntry::Heading(self.server.shown_name().into()),
            page("profile", t("Perfil del servidor"), Page::Profile),
            // Server administration, rest (P7).
            page("safety", t("Seguridad y participación"), Page::Safety),
            page("channels", t("Canales"), Page::Channels),
        ];
        // Rules and onboarding (2026-10-18): only a community has one.
        if self.server.options.community {
            entries.push(page("onboarding", t("Incorporación"), Page::Onboarding));
        }
        entries.extend([
            SidebarEntry::Separator,
            SidebarEntry::Heading(t("Personas").into()),
            page("members", t("Miembros"), Page::Members),
            page("roles", t("Roles"), Page::Roles),
            page("invites", t("Invitaciones"), Page::Invites),
            SidebarEntry::Separator,
            SidebarEntry::Heading(t("Expresiones").into()),
            page("emojis", t("Emojis"), Page::Emojis),
            page("stickers", t("Stickers"), Page::Stickers),
            SidebarEntry::Separator,
            SidebarEntry::Heading(t("Moderación").into()),
            page("audit", t("Registro de auditoría"), Page::Audit),
            page("bans", t("Baneos"), Page::Bans),
            SidebarEntry::Separator,
            SidebarEntry::Heading(t("Aplicaciones").into()),
            page("integrations", t("Integraciones"), Page::Integrations),
        ]);
        // AutoMod (automod): under moderation, after the bans.
        if let Some(at) = entries.iter().position(
            |entry| matches!(entry, SidebarEntry::Item { id, .. } if id == "ss:page:bans"),
        ) {
            entries.insert(at + 1, page("automod", t("AutoMod"), Page::AutoMod));
        }
        if self.store.owns(self.server.id) {
            entries.push(SidebarEntry::Separator);
            entries.push(SidebarEntry::Item {
                id: "ss:delete-server".into(),
                label: t("Eliminar servidor").into(),
                selected: false,
                danger: true,
            });
        }
        let footnote = match &self.store.provider {
            Some(info) if info.kind == ProviderKind::Simulated => {
                tr!("{name} · sin conexión real", name = t(&info.name))
            }
            Some(info) => t(&info.name).to_owned(),
            None => t("Sin proveedor").into(),
        };
        parts::sidebar(&entries, &footnote, self.look)
    }

    /// The profile's form sits beside the preview card when there is room.
    fn beside(&self) -> bool {
        self.metrics.content - 318.5 - 44.0 >= 300.0
    }

    fn form_width(&self) -> f32 {
        if self.beside() {
            (self.metrics.content - 318.5 - 44.0).min(440.0)
        } else {
            self.metrics.content
        }
    }

    /// The role editor's column, right of the list of roles.
    fn editor_width(&self) -> f32 {
        (self.metrics.region() - parts::ROLE_LIST_WIDTH - 27.0).clamp(240.0, parts::EDITOR_WIDTH)
    }

    fn name_of(&self, user: UserId) -> String {
        self.store.display_name(user, Some(self.server.id))
    }

    fn person(&self, user: UserId) -> PersonLine {
        let account = self.store.user(user);
        PersonLine {
            name: self.name_of(user),
            handle: account.map_or_else(|| user.to_string(), |account| account.name.clone()),
            accent: account.map_or(self.look.faint, |account| self.look.ink(account.accent)),
        }
    }

    fn role_color(&self, role: &Role) -> limn::Color {
        self.look.ink(role.color.unwrap_or(DEFAULT_ROLE_COLOR))
    }

    /// Whether `user` matches what is typed in a search: name, handle or id.
    fn matches(&self, user: UserId, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let person = self.person(user);
        person.name.to_lowercase().contains(&query)
            || person.handle.to_lowercase().contains(&query)
            || user.to_string() == query
    }

    fn profile(&self) -> Element {
        let look = self.look;
        let denied = self.abilities.manage_server.reason();
        let can = denied.is_none();
        let shown = shown_server(self.server, &self.settings.profile);
        let members = self.store.members(self.server.id);
        let online = members
            .iter()
            .filter(|member| self.store.presence(member.user).status.is_around())
            .count();
        let mut form = vec![
            parts::field_label(t("Nombre"), true, look),
            TextField::new(
                NAME,
                shown.name.clone(),
                t("Nombre del servidor"),
                look.palette(),
            )
            .height(FIELD_HEIGHT)
            .accessibility_label(t("Nombre del servidor"))
            .enabled(can)
            .render(),
            parts::divider(look),
            parts::field_label(t("Color"), false, look),
            text(
                t("El color de la marca del servidor mientras no tenga imagen."),
                14.0,
                look.muted,
            ),
            parts::swatches(
                &Swatches {
                    prefix: "ss:accent".into(),
                    chosen: Some(shown.accent),
                    default: None,
                    custom: self.settings.accent_hex.clone(),
                    enabled: can,
                    narrow: self.form_width() < parts::swatches_width(false),
                    picked: crate::color_picker::picked(self.ui, "ss:accent"),
                },
                look,
            ),
            parts::divider(look),
            parts::field_label(t("Descripción"), false, look),
            text(
                t("¿De qué se trata el servidor? Se muestra en su perfil."),
                14.0,
                look.muted,
            ),
            TextArea::new(
                DESCRIPTION,
                shown.description.clone().unwrap_or_default(),
                t("Contale a la gente de qué se trata este servidor."),
                look.palette(),
            )
            .accessibility_label(t("Descripción del servidor"))
            .height(120.0)
            .enabled(can)
            .render(),
        ];
        // Server administration, rest (P7).
        form.extend(self.identity_fields());
        let card = parts::server_card(
            &ServerCard {
                name: shown.name.clone(),
                initials: shown.initials(),
                accent: look.ink(shown.accent),
                description: shown.description.clone(),
                online,
                members: members.len(),
                picture: self.settings.rest.icon_preview.clone(),
                traits: self.trait_chips(),
                // Icon, banner and community (2026-10-19).
                banner: self.shown_banner(),
            },
            look,
        );
        let mut children = vec![parts::page_title(
            t("Perfil del servidor"),
            Some(t(
                "Así se presenta el servidor: su nombre, su color y de qué se trata.",
            )),
            look,
        )];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        if self.beside() {
            children.push(
                Element::row(vec![
                    Element::column(form)
                        .gap(10.0)
                        .size(Some(self.form_width()), None),
                    Element::spacer().flex(1.0),
                    card,
                ])
                .gap(24.0),
            );
        } else {
            children.push(Element::column(form).gap(10.0));
            children.push(card);
        }
        parts::page("ss:scroll:profile", self.metrics.content, children)
    }

    fn members_with(&self, role: RoleId) -> usize {
        self.store
            .members(self.server.id)
            .iter()
            .filter(|member| member.roles.contains(&role))
            .count()
    }

    fn roles(&self) -> Element {
        let look = self.look;
        let settings = self.settings;
        let denied = self.abilities.manage_roles.reason();
        let order = shown_order(settings, self.store);
        let query = settings.roles_query.trim().to_lowercase();
        let dragged_at = settings
            .dragging
            .and_then(|role| order.iter().position(|id| *id == role));
        let mut children = vec![parts::page_title(
            t("Roles"),
            Some(t(
                "Usá roles para agrupar a los miembros del servidor y darles permisos.",
            )),
            look,
        )];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        children.push(parts::everyone_card("ss:role:everyone", look));
        if order.is_empty() {
            children.push(
                Empty::new(
                    t("Organizá a tus miembros"),
                    t("Usá roles para agrupar a los miembros del servidor y darles permisos."),
                    look.palette(),
                )
                .icon(Icon::Users)
                .action_element(parts::primary_button(
                    "ss:role:create",
                    t("Crear rol"),
                    denied.is_none(),
                    look,
                ))
                .render(),
            );
            return parts::page("ss:scroll:roles", self.metrics.content, children);
        }
        children.push(
            Element::row(vec![
                SearchField::new(
                    ROLES_QUERY,
                    settings.roles_query.clone(),
                    t("Buscar roles"),
                    look.palette(),
                )
                .accessibility_label(t("Buscar roles"))
                .render()
                .flex(1.0),
                parts::primary_button("ss:role:create", t("Crear rol"), denied.is_none(), look)
                    .centered_y(),
            ])
            .gap(16.0),
        );
        children.push(text(
            t("Los miembros usan el color del rol más alto que tengan en esta lista. Arrastrá los roles para reordenarlos."),
            14.0,
            look.muted,
        ));
        let rows: Vec<Element> = order
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let role = self.server.roles.iter().find(|role| role.id == *id)?;
                if !query.is_empty() && !role.name.to_lowercase().contains(&query) {
                    return None;
                }
                let locked = role_lock(self.store, self.server.id, role);
                let first = index == 0
                    || order[..index].iter().all(|above| {
                        self.server
                            .roles
                            .iter()
                            .find(|role| role.id == *above)
                            .is_some_and(|above| {
                                role_lock(self.store, self.server.id, above).is_some()
                            })
                    });
                let mut menu = vec![
                    MenuAction::new(format!("ss:role:{id}:up"), t("Subir"))
                        .icon(Icon::ArrowUp)
                        .denied(locked.clone().or_else(|| {
                            first.then(|| t("Ya está lo más arriba que podés ponerlo.").to_owned())
                        })),
                    MenuAction::new(format!("ss:role:{id}:down"), t("Bajar"))
                        .icon(Icon::ArrowDown)
                        .denied(locked.clone().or_else(|| {
                            (index + 1 == order.len()).then(|| t("Ya es el último.").to_owned())
                        })),
                    MenuAction::new(format!("ss:role:{id}:copy-id"), t("Copiar ID del rol"))
                        .icon(Icon::Copy),
                ];
                menu.push(
                    MenuAction::new(format!("ss:role:{id}:delete"), t("Eliminar"))
                        .icon(Icon::Trash)
                        .danger()
                        .denied(locked.clone())
                        .separated(),
                );
                let drop = match (dragged_at, settings.drop_over) {
                    (Some(from), Some(over)) if over == *id && from != index => {
                        Some(if from > index {
                            parts::DropEdge::Above
                        } else {
                            parts::DropEdge::Below
                        })
                    }
                    _ => None,
                };
                Some(parts::role_row(
                    &RoleLine {
                        role: role.id,
                        name: role.name.clone(),
                        color: self.role_color(role),
                        members: self.role_members(role.id),
                        movable: locked.is_none() && query.is_empty(),
                        locked,
                        menu: self.ids(menu),
                        menu_open: settings.role_menu == Some(role.id),
                        drop,
                    },
                    look,
                ))
            })
            .collect();
        children.push(
            Element::row(vec![
                line(tr!("ROLES: {count}", count = order.len()), 12.0, look.muted)
                    .font_family(limn::FontFamily::SansBold)
                    .flex(1.0),
                line(t("MIEMBROS"), 12.0, look.muted)
                    .font_family(limn::FontFamily::SansBold)
                    .size(Some(232.0), None),
            ])
            .padding(12.0),
        );
        if rows.is_empty() {
            children.push(text(t("Ningún rol coincide."), 14.0, look.muted));
        } else {
            children.push(Element::column(rows));
        }
        parts::page("ss:scroll:roles", self.metrics.content, children)
    }

    fn editor(&self, role: &Role) -> Element {
        let look = self.look;
        let settings = self.settings;
        let server = self.server.id;
        let everyone = role.id == self.server.everyone_id();
        let locked = role_lock(self.store, server, role);
        let shown = shown_role(role, &settings.role_edit);
        let order = shown_order(settings, self.store);
        let dragged_at = settings
            .dragging
            .and_then(|dragged| order.iter().position(|id| *id == dragged));
        let mut items: Vec<RoleItem> = order
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let found = self
                    .server
                    .roles
                    .iter()
                    .find(|candidate| candidate.id == *id)?;
                let item = if found.id == role.id { &shown } else { found };
                Some(RoleItem {
                    role: found.id,
                    name: item.name.clone(),
                    color: self.role_color(item),
                    selected: found.id == role.id,
                    movable: role_lock(self.store, server, found).is_none(),
                    drop: match (dragged_at, settings.drop_over) {
                        (Some(from), Some(over)) if over == *id && from != index => {
                            Some(if from > index {
                                parts::DropEdge::Above
                            } else {
                                parts::DropEdge::Below
                            })
                        }
                        _ => None,
                    },
                })
            })
            .collect();
        if let Some(found) = self.server.everyone() {
            items.push(RoleItem {
                role: found.id,
                name: found.name.clone(),
                color: self.role_color(found),
                selected: everyone,
                movable: false,
                drop: None,
            });
        }
        let list = parts::role_list(&items, self.abilities.manage_roles.allowed(), look);
        let members: Vec<UserId> = self
            .store
            .members(server)
            .iter()
            .filter(|member| member.roles.contains(&role.id))
            .map(|member| member.user)
            .collect();
        let mut menu = vec![MenuAction::new(
            format!("ss:role:{}:copy-id", role.id),
            t("Copiar ID del rol"),
        )
        .icon(Icon::Copy)];
        if !everyone {
            menu.push(
                MenuAction::new(format!("ss:role:{}:delete", role.id), t("Eliminar"))
                    .icon(Icon::Trash)
                    .danger()
                    .denied(locked.clone())
                    .separated(),
            );
        }
        let menu = self.ids(menu);
        let more = parts::menu(
            parts::square_button(
                "ss:editor:menu",
                Icon::More,
                t("Más opciones"),
                !menu.is_empty(),
                look,
            ),
            &menu,
            settings.editor_menu,
            "ss:editor-menu:dismiss",
            look,
        );
        let tab = if everyone {
            RoleTab::Permissions
        } else {
            settings.tab
        };
        // A narrow editor says it shorter, so the three tabs fit.
        let long = tr!("Gestionar miembros ({count})", count = members.len());
        let members_label = if everyone {
            t("Gestionar miembros").to_owned()
        } else if parts::tabs_width(&[t("Mostrar"), t("Permisos"), &long]) <= self.editor_width() {
            long
        } else {
            tr!("Miembros ({count})", count = members.len())
        };
        let tabs = parts::tabs(
            &[
                (
                    "ss:tab:display".into(),
                    t("Mostrar").into(),
                    tab == RoleTab::Display,
                    !everyone,
                ),
                (
                    "ss:tab:permissions".into(),
                    t("Permisos").into(),
                    tab == RoleTab::Permissions,
                    true,
                ),
                (
                    "ss:tab:members".into(),
                    members_label,
                    tab == RoleTab::Members,
                    !everyone,
                ),
            ],
            look,
        );
        let mut column = vec![
            Element::row(vec![
                strong_line(
                    tr!("EDITAR ROL: {name}", name = shown.name.to_uppercase()),
                    16.0,
                    look.text,
                )
                .font_family(limn::FontFamily::SansBold)
                .flex(1.0)
                .centered_y(),
                more,
            ])
            .gap(12.0),
            tabs,
        ];
        if let Some(reason) = &locked {
            column.push(parts::denied(reason, look));
        }
        let can = locked.is_none();
        match tab {
            RoleTab::Display => column.extend(self.display_tab(&shown, can)),
            RoleTab::Permissions => column.extend(self.permissions_tab(&shown, can)),
            RoleTab::Members => column.extend(self.members_tab(role, &members)),
        }
        let editor_width = self.editor_width();
        let editor = limn::ScrollArea::new(
            "ss:scroll:editor",
            Element::row(vec![
                gap(26.0, 0.0),
                Element::column(vec![
                    gap(0.0, 60.0),
                    Element::column(column).gap(16.0),
                    gap(0.0, 96.0),
                ])
                .size(Some(editor_width), None),
            ]),
        )
        .appearance(limn::ScrollbarAppearance::overlay().buttons(false))
        .render()
        .flex(1.0);
        Element::row(vec![
            list,
            Element::panel(Element::spacer())
                .size(Some(1.0), None)
                .background(look.border),
            editor,
        ])
    }

    fn display_tab(&self, role: &Role, can: bool) -> Vec<Element> {
        let look = self.look;
        let mut column = vec![
            parts::field_label(t("Nombre del rol"), true, look),
            TextField::new(
                ROLE_NAME,
                role.name.clone(),
                t("Nombre del rol"),
                look.palette(),
            )
            .height(FIELD_HEIGHT)
            .accessibility_label(t("Nombre del rol"))
            .enabled(can)
            .render(),
            parts::divider(look),
            parts::field_label(t("Color del rol"), true, look),
            text(
                t("Los miembros usan el color del rol más alto que tengan en la lista de roles."),
                14.0,
                look.muted,
            ),
            parts::swatches(
                &Swatches {
                    prefix: "ss:color".into(),
                    chosen: role.color,
                    default: Some(DEFAULT_ROLE_COLOR),
                    custom: self.settings.color_hex.clone(),
                    enabled: can,
                    narrow: self.editor_width() < parts::swatches_width(true),
                    picked: crate::color_picker::picked(self.ui, "ss:color"),
                },
                look,
            ),
        ];
        // Server administration, rest (P7): its gradient and icon, after its color.
        column.extend(self.role_style_fields(role, can));
        column.extend([
            parts::divider(look),
            parts::switch_row(
                "ss:hoist",
                t("Mostrar a los miembros con este rol por separado"),
                Some(t("En la lista de miembros, quienes lo tengan aparecen en su propio grupo.")),
                role.hoisted,
                can,
                look,
            ),
            parts::divider(look),
            parts::switch_row(
                "ss:mentionable",
                t("Permitir que cualquiera @mencione este rol"),
                Some(t("Quien tenga el permiso «Mencionar @everyone, @here y todos los roles» siempre puede mencionarlo.")),
                role.mentionable,
                can,
                look,
            ),
        ]);
        column
    }

    fn permissions_tab(&self, role: &Role, can: bool) -> Vec<Element> {
        let look = self.look;
        let mut column = vec![SearchField::new(
            PERMISSIONS_QUERY,
            self.settings.permissions_query.clone(),
            t("Buscar permisos"),
            look.palette(),
        )
        .accessibility_label(t("Buscar permisos"))
        .render()];
        let groups = parts::permissions_matching(&self.settings.permissions_query);
        if groups.is_empty() {
            column.push(text(t("Ningún permiso coincide."), 14.0, look.muted));
        }
        for (index, (title, permissions)) in groups.into_iter().enumerate() {
            column.push(parts::permission_heading(title, index == 0 && can, look));
            let rows = permissions
                .into_iter()
                .map(|(permission, label, description)| {
                    let on = role.permissions.contains(permission);
                    // Taking a permission away is always allowed; giving it, only what the account has.
                    let lock = (can && !on)
                        .then(|| permission_lock(self.store, self.server.id, permission))
                        .flatten();
                    parts::permission_row(
                        permission,
                        label,
                        description,
                        on,
                        can && lock.is_none(),
                        lock.as_deref(),
                        look,
                    )
                })
                .collect();
            column.push(Element::column(rows));
        }
        column
    }

    fn members_tab(&self, role: &Role, members: &[UserId]) -> Vec<Element> {
        let look = self.look;
        let granted = grant_lock(self.store, self.server.id, role);
        let query = &self.settings.role_members_query;
        let mut column = vec![Element::row(vec![
            SearchField::new(
                ROLE_MEMBERS_QUERY,
                query.clone(),
                t("Buscar miembros"),
                look.palette(),
            )
            .accessibility_label(t("Buscar miembros del rol"))
            .render()
            .flex(1.0),
            parts::primary_button(
                "ss:role-members:add",
                t("Añadir miembros"),
                granted.is_none(),
                look,
            )
            .centered_y(),
        ])
        .gap(16.0)];
        if let Some(reason) = &granted {
            column.push(parts::denied(reason, look));
        }
        let rows: Vec<Element> = members
            .iter()
            .filter(|user| self.matches(**user, query))
            .map(|user| {
                parts::role_member_row(
                    &self.person(*user),
                    &format!("ss:role-member:{user}:remove"),
                    granted.is_none(),
                    look,
                )
            })
            .collect();
        if rows.is_empty() {
            column.push(
                Element::row(vec![
                    rune_components::icon(Icon::Users, look.faint).centered_y(),
                    text(t("No hay miembros con este rol."), 14.0, look.muted).centered_y(),
                    Button::new(
                        "ss:role-members:add",
                        t("Añadí miembros a este rol."),
                        look.palette(),
                    )
                    .variant(ButtonVariant::Link)
                    .enabled(granted.is_none())
                    .render()
                    .centered_y(),
                ])
                .gap(8.0),
            );
        } else {
            column.push(Element::column(rows).gap(2.0));
        }
        column
    }

    fn members(&self) -> Element {
        let look = self.look;
        let settings = self.settings;
        let server = self.server.id;
        let roles = self.server.assignable_roles();
        let me = self.store.me();
        let mut people: Vec<(String, UserId)> = self
            .store
            .members(server)
            .iter()
            .filter(|member| {
                settings
                    .role_filter
                    .is_none_or(|role| member.roles.contains(&role))
                    && self.matches(member.user, &settings.members_query)
            })
            .map(|member| (self.name_of(member.user).to_lowercase(), member.user))
            .collect();
        people.sort();
        // Server administration, rest (P7): in the order the provider listed.
        if let Some(listed) = self.ordered_members() {
            people.retain(|(_, user)| listed.contains(user));
            people.sort_by_key(|(_, user)| listed.iter().position(|found| found == user));
        }
        // shortcut: every row is built; a server of thousands wants a
        // uniform_list here, as the member list of the window has.
        let rows: Vec<Element> = people
            .iter()
            .filter_map(|(_, user)| {
                let member = self.store.member_of(server, *user)?;
                let person = self.person(*user);
                let theirs: Vec<(String, limn::Color)> = roles
                    .iter()
                    .filter(|role| member.roles.contains(&role.id))
                    .map(|role| (role.name.clone(), self.role_color(role)))
                    .collect();
                let picker_open = settings.role_picker == Some(*user);
                let choices = self.abilities.manage_roles.allowed().then(|| {
                    if !picker_open {
                        return Vec::new();
                    }
                    roles
                        .iter()
                        .map(|role| RoleChoice {
                            id: format!("ss:member:{user}:role:{}", role.id),
                            name: role.name.clone(),
                            color: self.role_color(role),
                            checked: member.roles.contains(&role.id),
                            denied: grant_lock(self.store, server, role),
                        })
                        .collect()
                });
                let mut menu = vec![
                    MenuAction::new(format!("ss:member:{user}:nick"), t("Cambiar apodo"))
                        .icon(Icon::Edit)
                        .denied(nick_lock(self.store, server, *user)),
                    MenuAction::new(
                        format!("ss:member:{user}:copy-id"),
                        t("Copiar ID del usuario"),
                    )
                    .icon(Icon::Copy),
                ];
                if Some(*user) != me {
                    menu.push(
                        MenuAction::new(
                            format!("ss:member:{user}:kick"),
                            tr!("Expulsar a {name}", name = person.name),
                        )
                        .icon(Icon::Close)
                        .danger()
                        .denied(member_lock(self.store, server, *user, &self.abilities.kick))
                        .separated(),
                    );
                    menu.push(
                        MenuAction::new(
                            format!("ss:member:{user}:ban"),
                            tr!("Vetar a {name}", name = person.name),
                        )
                        .icon(Icon::ErrorCircle)
                        .danger()
                        .denied(member_lock(
                            self.store,
                            server,
                            *user,
                            &self.abilities.ban,
                        )),
                    );
                    if self
                        .store
                        .provider
                        .as_ref()
                        .is_some_and(|info| info.capabilities.timeouts)
                    {
                        menu.extend(self.timeout_actions(*user, &person.name));
                    }
                }
                let ink = self
                    .store
                    .top_role(*user, server)
                    .map_or(look.text, |role| self.role_color(role));
                // Server administration, rest (P7): how they joined.
                let handle = match self.joined_note(*user) {
                    Some(note) => format!("{} · {note}", person.handle),
                    None => person.handle,
                };
                Some(parts::member_row(
                    &MemberLine {
                        user: *user,
                        name: person.name,
                        handle,
                        ink,
                        accent: person.accent,
                        owner: self.server.owner == Some(*user),
                        roles: theirs,
                        choices,
                        picker_open,
                        menu: self.ids(menu),
                        menu_open: settings.member_menu == Some(*user),
                    },
                    self.metrics.content,
                    look,
                ))
            })
            .collect();
        let filter_label = settings
            .role_filter
            .and_then(|role| roles.iter().find(|candidate| candidate.id == role))
            .map_or_else(|| t("Todos los roles").to_owned(), |role| role.name.clone());
        let mut options = vec![
            DropdownOption::new("ss:members-filter:all", t("Todos los roles"))
                .selected(settings.role_filter.is_none()),
        ];
        options.extend(roles.iter().map(|role| {
            DropdownOption::new(format!("ss:members-filter:{}", role.id), role.name.clone())
                .selected(settings.role_filter == Some(role.id))
        }));
        let total = self
            .store
            .member_pages(server)
            .and_then(|pages| pages.total)
            .map_or(self.store.members(server).len(), |total| total as usize);
        let mut header = vec![strong_line(
            if total == 1 {
                t("1 miembro").to_owned()
            } else {
                tr!("{total} miembros", total)
            },
            16.0,
            look.text,
        )
        .flex(1.0)
        .centered_y()];
        header.extend(self.prune_button().map(Element::centered_y));
        let header = Element::row(header).gap(10.0).flex(1.0);
        let search = SearchField::new(
            MEMBERS_QUERY,
            settings.members_query.clone(),
            t("Buscar por nombre o ID"),
            look.palette(),
        )
        .accessibility_label(t("Buscar miembros"))
        .width(265.0)
        .render()
        .centered_y();
        let filter = Select::new(
            "ss:members:filter",
            filter_label,
            options,
            settings.filter_open,
            "ss:members-filter:dismiss",
            look.palette(),
        )
        .menu_palette(look.menu_palette())
        .accessibility_label(t("Filtrar por rol"))
        .width(180.0)
        .render()
        .centered_y();
        // The count and its button share a line with the search when it fits them.
        let tools = if self.metrics.content >= 265.0 + 180.0 + 20.0 + 220.0 {
            Element::row(vec![header, search, filter]).gap(10.0)
        } else {
            Element::column(vec![
                Element::row(vec![header]),
                Element::row(vec![search.flex(1.0), filter]).gap(10.0),
            ])
            .gap(10.0)
        };
        let mut children = vec![
            parts::page_title(t("Miembros del servidor"), None, look),
            tools,
        ];
        // Server administration, rest (P7).
        children.extend(self.member_tools());
        children.push(parts::members_heading(self.metrics.content, look));
        let shown = rows.len();
        if rows.is_empty() {
            children.push(text(t("Nadie coincide con la búsqueda."), 14.0, look.muted));
        } else {
            children.push(Element::column(rows));
        }
        self.members_footer(shown, &mut children);
        parts::page("ss:scroll:members", self.metrics.content, children)
    }

    fn bans(&self) -> Element {
        let look = self.look;
        let settings = self.settings;
        let mut children = vec![parts::page_title(
            t("Baneos"),
            Some(t("Quienes fueron vetados no pueden volver a entrar al servidor hasta que se revoque el veto. Se veta desde la página Miembros.")),
            look,
        )];
        if let Some(reason) = self.abilities.ban.reason() {
            children.push(parts::denied(reason, look));
            return parts::page("ss:scroll:bans", self.metrics.content, children);
        }
        children.push(
            SearchField::new(
                BANS_QUERY,
                settings.bans_query.clone(),
                t("Buscar vetos por nombre o ID"),
                look.palette(),
            )
            .accessibility_label(t("Buscar vetos"))
            .render(),
        );
        match self.store.bans(self.server.id) {
            None => children.push(line(t("Cargando…"), 14.0, look.muted)),
            Some(list) if list.items.is_empty() && list.reached_end => children.push(
                Empty::new(
                    t("No hay personas vetadas"),
                    t("No vetaste a nadie… y ojalá no haga falta."),
                    look.palette(),
                )
                .icon(Icon::Users)
                .framed()
                .render(),
            ),
            Some(list) => {
                let rows: Vec<Element> = list
                    .items
                    .iter()
                    .filter(|ban| self.matches(ban.user, &settings.bans_query))
                    .map(|ban| {
                        parts::ban_row(
                            &BanLine {
                                user: ban.user,
                                person: self.person(ban.user),
                                reason: ban.reason.clone(),
                                denied: None,
                            },
                            look,
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    children.push(text(t("Ningún veto coincide."), 14.0, look.muted));
                } else {
                    children.push(Element::column(rows));
                }
                if !list.reached_end {
                    // The search reads only what is loaded: say so.
                    if !settings.bans_query.trim().is_empty() {
                        children.push(text(
                            tr!(
                                "Se buscó entre los {count} vetos cargados.",
                                count = list.items.len()
                            ),
                            13.0,
                            look.muted,
                        ));
                    }
                    children.push(more_button("ss:bans:more", look));
                }
            }
        }
        parts::page("ss:scroll:bans", self.metrics.content, children)
    }

    fn audit(&self, context: &ViewContext) -> Element {
        let look = self.look;
        let settings = self.settings;
        let server = self.server.id;
        let query = settings.audit_query();
        let entries = self.store.audit(server, &query);
        // The provider filters: offer everyone in the server, and whoever is chosen.
        let mut actors: Vec<UserId> = self
            .store
            .members(server)
            .iter()
            .map(|member| member.user)
            .collect();
        if let Some(chosen) = settings.audit_user.filter(|user| !actors.contains(user)) {
            actors.push(chosen);
        }
        actors.sort_by_cached_key(|user| self.name_of(*user).to_lowercase());
        let mut users = vec![
            DropdownOption::new("ss:audit-user:all", t("Todos los usuarios"))
                .selected(settings.audit_user.is_none()),
        ];
        users.extend(actors.iter().map(|user| {
            DropdownOption::new(format!("ss:audit-user:{user}"), self.name_of(*user))
                .selected(settings.audit_user == Some(*user))
        }));
        let mut actions = vec![
            DropdownOption::new("ss:audit-action:all", t("Todas las acciones"))
                .selected(settings.audit_action.is_none()),
        ];
        actions.extend(
            AUDIT_ACTIONS
                .iter()
                .enumerate()
                .map(|(index, (kind, label))| {
                    DropdownOption::new(format!("ss:audit-action:{index}"), t(label))
                        .selected(settings.audit_action == Some(*kind))
                }),
        );
        let filter = |label: &str, select: Element| {
            Element::column(vec![line(label, 14.0, look.muted), select]).gap(6.0)
        };
        // Title and filters share a line when it fits them; else the filters go under it.
        let select = ((self.metrics.content - 16.0) / 2.0).min(230.0);
        let title = strong_line(t("Registro de auditoría"), 20.0, look.text);
        let filters = vec![
            filter(
                t("Filtrar por usuario"),
                Select::new(
                    "ss:audit:user",
                    settings.audit_user.map_or_else(
                        || t("Todos los usuarios").to_owned(),
                        |user| self.name_of(user),
                    ),
                    users,
                    settings.audit_filter == Some(super::AuditFilter::User),
                    "ss:audit-filter:dismiss",
                    look.palette(),
                )
                .menu_palette(look.menu_palette())
                .accessibility_label(t("Filtrar por usuario"))
                .width(select)
                .render(),
            ),
            filter(
                t("Filtrar por acción"),
                Select::new(
                    "ss:audit:action",
                    settings
                        .audit_action
                        .map_or(t("Todas las acciones"), audit_label),
                    actions,
                    settings.audit_filter == Some(super::AuditFilter::Action),
                    "ss:audit-filter:dismiss",
                    look.palette(),
                )
                .menu_palette(look.menu_palette())
                .accessibility_label(t("Filtrar por acción"))
                .width(select)
                .menu_width(300.0)
                .render(),
            ),
        ];
        let header = if self.metrics.content >= 2.0 * 230.0 + 16.0 + 240.0 {
            let mut row = vec![title.flex(1.0).centered_y()];
            row.extend(filters);
            Element::row(row).gap(16.0)
        } else {
            Element::column(vec![title, Element::row(filters).gap(16.0)]).gap(12.0)
        };
        let mut children = vec![header, parts::divider(look)];
        if let Some(reason) = self.abilities.view_audit_log.reason() {
            children.push(parts::denied(reason, look));
            return parts::page("ss:scroll:audit", self.metrics.content, children);
        }
        let Some(entries) = entries else {
            children.push(line(t("Cargando…"), 14.0, look.muted));
            return parts::page("ss:scroll:audit", self.metrics.content, children);
        };
        let person = |user: UserId| self.name_of(user);
        let role = |role: RoleId| {
            self.server
                .roles
                .iter()
                .find(|candidate| candidate.id == role)
                .map_or_else(|| t("un rol borrado").to_owned(), |role| role.name.clone())
        };
        let rows: Vec<Element> = entries
            .items
            .iter()
            .map(|entry| {
                let actor = self.person(entry.actor);
                parts::audit_row(
                    &AuditLine {
                        id: format!("ss:audit:{}", entry.id),
                        actor: actor.name.clone(),
                        accent: actor.accent,
                        kind: AuditKind::of(&entry.action),
                        sentence: audit_sentence(&actor.name, &entry.action, &person),
                        when: time::stamp(entry.when, context.wall),
                        details: audit_details(&entry.action, entry.reason.as_deref(), &role),
                        expanded: settings.expanded.contains(&entry.id),
                    },
                    look,
                )
            })
            .collect();
        if rows.is_empty() {
            children.push(
                Empty::new(
                    t("Sin entradas"),
                    if query == Default::default() {
                        t("Todavía nadie cambió nada en este servidor.")
                    } else {
                        t("Ninguna entrada coincide con los filtros.")
                    },
                    look.palette(),
                )
                .icon(Icon::History)
                .render(),
            );
        } else {
            children.push(Element::column(rows).gap(8.5));
        }
        if entries.items.len() >= rune_state::AUDIT_MAX {
            children.push(text(
                tr!(
                    "Se muestran las {count} entradas más recientes.",
                    count = rune_state::AUDIT_MAX
                ),
                13.0,
                look.muted,
            ));
        } else if !entries.reached_end {
            children.push(more_button("ss:audit:more", look));
        }
        parts::page("ss:scroll:audit", self.metrics.content, children)
    }

    fn invites(&self, context: &ViewContext) -> Element {
        let look = self.look;
        let mut children = vec![parts::page_title(
            t("Invitaciones"),
            Some(t("Los enlaces activos para entrar al servidor. Revocar uno hace que deje de funcionar para quien lo tenga.")),
            look,
        )];
        // Server administration, rest (P7): pausing them.
        let mut pause = self.invites_pause();
        let mut buttons = vec![Element::spacer().flex(1.0)];
        if !pause.is_empty() {
            buttons.push(pause.remove(0));
        }
        buttons.extend(self.new_invite_button());
        if buttons.len() > 1 {
            children.push(Element::row(buttons).gap(10.0));
        }
        children.extend(pause);
        if let Some(reason) = self.abilities.manage_invites.reason() {
            children.push(parts::denied(reason, look));
            return parts::page("ss:scroll:invites", self.metrics.content, children);
        }
        match self.store.invites(self.server.id) {
            None => children.push(line(t("Cargando…"), 14.0, look.muted)),
            Some([]) => children.push(
                Empty::new(
                    t("No hay invitaciones"),
                    t("Nadie creó un enlace para entrar todavía."),
                    look.palette(),
                )
                .icon(Icon::Users)
                .framed()
                .render(),
            ),
            Some(invites) => {
                let rows: Vec<Element> = invites
                    .iter()
                    .map(|invite| {
                        let uses = match (invite.uses, invite.max_uses) {
                            (Some(uses), Some(1)) => tr!("{uses} de 1 uso", uses),
                            (Some(uses), Some(max)) => tr!("{uses} de {max} usos", uses, max),
                            (Some(1), None) => t("1 uso").to_owned(),
                            (Some(uses), None) => tr!("{uses} usos", uses),
                            (None, _) => t("Usos desconocidos").to_owned(),
                        };
                        parts::invite_row(
                            &InviteLine {
                                code: invite.code.clone(),
                                inviter: invite.inviter.map(|user| self.person(user)),
                                channel: invite
                                    .channel
                                    .and_then(|channel| self.store.channel(channel))
                                    .map(|channel| format!("#{}", channel.name)),
                                uses,
                                expires: invite.expires.map_or_else(
                                    || t("No vence").to_owned(),
                                    |when| {
                                        tr!("Vence: {when}", when = time::stamp(when, context.wall))
                                    },
                                ),
                                denied: None,
                                copy: true,
                            },
                            look,
                        )
                    })
                    .collect();
                children.push(Element::column(rows));
            }
        }
        parts::page("ss:scroll:invites", self.metrics.content, children)
    }

    /// The emoji page, or the sticker page with `stickers`.
    fn expressions(&self, stickers: bool) -> Element {
        let look = self.look;
        let (title, description, scroll) = if stickers {
            (
                t("Stickers"),
                t("Los stickers propios del servidor, para enviar en sus canales."),
                "ss:scroll:stickers",
            )
        } else {
            (
                t("Emojis"),
                t("Los emojis propios del servidor, para escribir y reaccionar en él."),
                "ss:scroll:emojis",
            )
        };
        let mut children = vec![parts::page_title(title, Some(description), look)];
        let denied = self.abilities.manage_expressions.reason();
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        children.push(self.upload_bar(stickers));
        let role_name = |role: &RoleId| {
            self.server
                .roles
                .iter()
                .find(|candidate| candidate.id == *role)
                .map_or_else(|| t("un rol borrado").to_owned(), |role| role.name.clone())
        };
        let rows: Vec<Element> = if stickers {
            self.server
                .stickers
                .iter()
                .map(|sticker| {
                    let mut notes = Vec::new();
                    if !sticker.available {
                        notes.push(t("No disponible").to_owned());
                    }
                    ExpressionLine {
                        key: format!("sticker:{}", sticker.id),
                        name: sticker.name.clone(),
                        notes,
                        preview: self.ui.image(&sticker.image),
                        denied: denied.map(str::to_owned),
                    }
                })
                .map(|row| parts::expression_row(&row, look))
                .collect()
        } else {
            self.server
                .emojis
                .iter()
                .map(|emoji| {
                    let mut notes = Vec::new();
                    if emoji.animated {
                        notes.push(t("Animado").to_owned());
                    }
                    if !emoji.roles.is_empty() {
                        let roles: Vec<String> = emoji.roles.iter().map(role_name).collect();
                        notes.push(tr!("Solo para {roles}", roles = roles.join(", ")));
                    }
                    if !emoji.available {
                        notes.push(t("No disponible").to_owned());
                    }
                    ExpressionLine {
                        key: format!("emoji:{}", emoji.id),
                        name: format!(":{}:", emoji.name),
                        notes,
                        preview: self.ui.image(&emoji.image),
                        denied: denied.map(str::to_owned),
                    }
                })
                .map(|row| parts::expression_row(&row, look))
                .collect()
        };
        if rows.is_empty() {
            children.push(
                Empty::new(
                    if stickers {
                        t("No hay stickers")
                    } else {
                        t("No hay emojis")
                    },
                    t("Este servidor no tiene ninguno propio."),
                    look.palette(),
                )
                .icon(Icon::Image)
                .framed()
                .render(),
            );
        } else {
            children.push(line(
                tr!("{count} en total", count = rows.len()),
                14.0,
                look.muted,
            ));
            children.push(Element::column(rows));
        }
        parts::page(scroll, self.metrics.content, children)
    }

    /// The dialogs over the settings: deleting a role or the server, adding
    /// members to a role, a nickname, kicking or banning, revoking a ban.
    fn dialogs(&self, root: Element) -> Element {
        let look = self.look;
        let settings = self.settings;
        let palette = look.menu_palette();
        let role_name = |role: RoleId| {
            self.server
                .roles
                .iter()
                .find(|candidate| candidate.id == role)
                .map_or_else(String::new, |role| role.name.clone())
        };
        let mut root = AlertDialog::new(
            root,
            settings.delete_role.is_some(),
            t("Eliminar rol"),
            tr!(
                "¿Seguro que querés eliminar el rol «{name}»? No se puede deshacer.",
                name = settings.delete_role.map(role_name).unwrap_or_default()
            ),
            "ss:role-delete:confirm",
            "ss:role-delete:cancel",
            palette,
        )
        .confirm_label(t("Eliminar"))
        .destructive(look.danger)
        .width(425.0)
        .render();
        if let (Some(chosen), Some(role)) = (&settings.add_members, settings.role) {
            let candidates: Vec<Element> = self
                .store
                .members(self.server.id)
                .iter()
                .filter(|member| {
                    !member.roles.contains(&role) && self.matches(member.user, &settings.add_query)
                })
                .map(|member| {
                    parts::pick_row(
                        &format!("ss:add:{}", member.user),
                        &self.person(member.user),
                        chosen.contains(&member.user),
                        look,
                    )
                })
                .collect();
            let list = if candidates.is_empty() {
                text(t("Nadie más para agregar."), 14.0, look.muted)
            } else {
                Element::column(candidates).gap(2.0)
            };
            root = Modal::new(
                root,
                true,
                t("Añadir miembros"),
                tr!(
                    "Elegí a quiénes darles el rol «{name}».",
                    name = role_name(role)
                ),
                "ss:add:cancel",
                palette,
            )
            .width(510.0)
            .body(
                Element::column(vec![
                    SearchField::new(
                        ADD_QUERY,
                        settings.add_query.clone(),
                        t("Buscar miembros"),
                        palette,
                    )
                    .accessibility_label(t("Buscar miembros para agregar"))
                    .render(),
                    strong_line(t("Miembros"), 14.0, look.text),
                    list,
                ])
                .gap(10.0),
            )
            .footer(footer(
                "ss:add:cancel",
                Button::new("ss:add:confirm", t("Añadir"), look.accent_palette())
                    .enabled(!chosen.is_empty())
                    .render(),
                look,
            ))
            .initial_focus(ADD_QUERY)
            .render();
        }
        if let Some((user, nick)) = &settings.nick {
            let person = self.person(*user);
            root = Modal::new(
                root,
                true,
                t("Cambiar apodo"),
                tr!(
                    "El nombre que {name} usa en este servidor.",
                    name = person.name
                ),
                "ss:nick:cancel",
                palette,
            )
            .width(440.0)
            .body(
                Element::column(vec![
                    FormField::new(
                        t("Apodo"),
                        TextField::new(
                            NICK,
                            nick.clone(),
                            self.store.user(*user).map_or_else(String::new, |account| {
                                account.shown_name().to_owned()
                            }),
                            palette,
                        )
                        .accessibility_label(t("Apodo"))
                        .height(FIELD_HEIGHT)
                        .render(),
                        palette,
                    )
                    .hint(t("Vacío, se usa su nombre de siempre."))
                    .render(),
                    Button::new("ss:nick:clear", t("Quitar el apodo"), palette)
                        .variant(ButtonVariant::Link)
                        .render(),
                ])
                .gap(8.0),
            )
            .footer(footer(
                "ss:nick:cancel",
                Button::new("ss:nick:save", t("Guardar"), look.accent_palette()).render(),
                look,
            ))
            .initial_focus(NICK)
            .render();
        }
        if let Some(removal) = &settings.removal {
            let name = self.name_of(removal.user);
            let (title, description, confirm) = if removal.ban {
                (
                    tr!("Vetar a {name}", name),
                    t("Sale del servidor y no puede volver hasta que se revoque el veto."),
                    t("Vetar"),
                )
            } else {
                (
                    tr!("Expulsar a {name}", name),
                    t("Sale del servidor; puede volver con una invitación nueva."),
                    t("Expulsar"),
                )
            };
            root = AlertDialog::new(
                root,
                true,
                title,
                description,
                "ss:removal:confirm",
                "ss:removal:cancel",
                palette,
            )
            .body(
                FormField::new(
                    t("Motivo (opcional)"),
                    TextField::new(
                        REASON,
                        removal.reason.clone(),
                        t("Queda en el registro de auditoría"),
                        palette,
                    )
                    .accessibility_label(t("Motivo"))
                    .height(FIELD_HEIGHT)
                    .render(),
                    palette,
                )
                .render(),
            )
            .confirm_label(confirm)
            .destructive(look.danger)
            .render();
        }
        if let Some(user) = settings.unban {
            root = AlertDialog::new(
                root,
                true,
                tr!("Revocar el veto a {name}", name = self.name_of(user)),
                t("Va a poder volver a entrar al servidor con una invitación."),
                "ss:unban:confirm",
                "ss:unban:cancel",
                palette,
            )
            .confirm_label(t("Revocar veto"))
            .width(425.0)
            .render();
        }
        if let Some(code) = &settings.revoke_invite {
            root = AlertDialog::new(
                root,
                true,
                t("Revocar invitación"),
                tr!("El enlace {code} va a dejar de funcionar. Quien ya entró con él sigue en el servidor.", code),
                "ss:invite-revoke:confirm",
                "ss:invite-revoke:cancel",
                palette,
            )
            .confirm_label(t("Revocar"))
            .destructive(look.danger)
            .width(425.0)
            .render();
        }
        if let Some((expression, name)) = &settings.rename {
            let (title, hint, valid) = match expression {
                ExpressionId::Emoji(_) => (
                    t("Renombrar emoji"),
                    t("De 2 a 32 letras sin acentos, números o guiones bajos."),
                    rune_model::valid_emoji_name(name.trim()),
                ),
                ExpressionId::Sticker(_) => (
                    t("Renombrar sticker"),
                    t("De 2 a 30 caracteres."),
                    rune_model::valid_sticker_name(name.trim()),
                ),
            };
            root = Modal::new(root, true, title, hint, "ss:rename:cancel", palette)
                .width(440.0)
                .body(
                    FormField::new(
                        t("Nombre"),
                        TextField::new(EXPRESSION_NAME, name.clone(), "", palette)
                            .accessibility_label(t("Nombre"))
                            .height(FIELD_HEIGHT)
                            .render(),
                        palette,
                    )
                    .render(),
                )
                .footer(footer(
                    "ss:rename:cancel",
                    Button::new("ss:rename:save", t("Guardar"), look.accent_palette())
                        .enabled(valid)
                        .render(),
                    look,
                ))
                .initial_focus(EXPRESSION_NAME)
                .render();
        }
        if let Some(expression) = settings.delete_expression {
            let name =
                super::expression_name(self.store, self.server.id, expression).unwrap_or_default();
            let (title, what) = match expression {
                ExpressionId::Emoji(_) => (t("Eliminar emoji"), tr!("el emoji :{name}:", name)),
                ExpressionId::Sticker(_) => {
                    (t("Eliminar sticker"), tr!("el sticker «{name}»", name))
                }
            };
            root = AlertDialog::new(
                root,
                true,
                title,
                tr!("¿Seguro que querés eliminar {what}? Nadie del servidor va a poder usarlo. No se puede deshacer.", what),
                "ss:expression-delete:confirm",
                "ss:expression-delete:cancel",
                palette,
            )
            .confirm_label(t("Eliminar"))
            .destructive(look.danger)
            .width(425.0)
            .render();
        }
        if let Some(typed) = &settings.delete_server {
            let name = &self.server.name;
            let matches = typed.trim() == name.as_str();
            let mut body = vec![FormField::new(
                t("Escribí el nombre del servidor"),
                TextField::new(DELETE_NAME, typed.clone(), name.clone(), palette)
                    .accessibility_label(t("Nombre del servidor, para confirmar"))
                    .height(FIELD_HEIGHT)
                    .render(),
                palette,
            )
            .render()];
            if let Some(reason) = self.abilities.delete.reason() {
                body.push(text(reason, 13.0, look.danger));
            }
            root = AlertDialog::new(
                root,
                true,
                tr!("Eliminar «{name}»", name),
                tr!(
                    "¿Seguro que querés eliminar {name}? No se puede deshacer.",
                    name
                ),
                "ss:delete-server:confirm",
                "ss:delete-server:cancel",
                palette,
            )
            .body(Element::column(body).gap(8.0))
            .confirm_label(t("Eliminar servidor"))
            .confirm_enabled(matches && self.abilities.delete.allowed())
            .destructive(look.danger)
            .width(510.0)
            .render();
        }
        let root = self.admin_dialogs(root);
        // AutoMod (automod).
        self.automod_dialogs(root)
    }
}

/// The button that asks for the next page of a list.
fn more_button(id: &str, look: &Look) -> Element {
    Button::new(id, t("Cargar más"), look.palette())
        .variant(ButtonVariant::Outline)
        .render()
}

/// A dialog's buttons: Cancelar, and the action.
fn footer(cancel: &str, action: Element, look: &Look) -> Element {
    Element::row(vec![
        Element::spacer().flex(1.0),
        Button::new(cancel, t("Cancelar"), look.menu_palette())
            .variant(ButtonVariant::Outline)
            .render(),
        action,
    ])
    .gap(9.0)
}
