//! What the settings of a channel show, from their state and the store.

use limn::{
    AlertDialog, Button, ButtonVariant, Color, Component, ComponentPalette, ControlSize, Element,
    FontFamily, Icon, Modal, TextArea, TextField, TextSpan,
};
use rune_components::channel_settings::{
    self as own, AccessLine, AddChoice, FieldSelect, PrivateCard, TargetItem, TargetMark, SLOWMODE,
    TARGET_GAP, TOPIC_LIMIT,
};
use rune_components::server_menu::{audience_picker, Audience, Choice};
use rune_components::server_settings::{
    self as parts, MenuAction, Metrics, SidebarEntry, UnsavedBar, DEFAULT_ROLE_COLOR,
};
use rune_components::{gap, line, text, Look};
use rune_i18n::{t, tr};
use rune_model::{
    Channel, ChannelKind, OverwriteTarget, Permissions, RemoteImage, Role, Server, UserId,
};
use rune_provider::ProviderKind;
use rune_state::{Abilities, Store};

use super::{
    everyone, final_name, permission_lock, sync_commands, target_lock, ChannelSettings, Page,
    ACCESS_QUERY, NAME, TOPIC,
};
use crate::{Ui, ViewContext};

/// The height of the settings' text fields, as the reference measures them.
const FIELD_HEIGHT: f32 = 41.0;
/// The topic's field: the reference's editor without its 35 px of format buttons.
const TOPIC_HEIGHT: f32 = 214.0;

/// The settings over the whole window, while they are open.
pub fn view(ui: &Ui, store: &Store, context: &ViewContext) -> Option<Element> {
    let settings = ui.channel_settings.as_ref()?;
    let channel = store.channel(settings.channel)?;
    let server = store.server(channel.server?)?;
    let abilities = store.abilities(channel.id)?;
    let look = &ui.look();
    let metrics = Metrics::new(context.window.width, false);
    let view = View {
        settings,
        store,
        channel,
        server,
        abilities: &abilities,
        look,
        metrics,
        advanced: ui.advanced_permissions,
        developer: ui.prefs.developer,
        ui,
    };
    let region = match settings.page {
        Page::Overview => view.overview(),
        Page::Permissions => view.permissions(),
        // Server administration, rest (P7).
        Page::Forum => {
            super::forum::page(settings, channel, &server.emojis, &abilities, look, metrics)
        }
    };
    let bar = settings.dirty(store).then(|| {
        parts::unsaved_bar_for(
            "cs",
            UnsavedBar {
                saving: settings.saving(),
                nudged: settings.nudged,
            },
            look,
        )
    });
    let root = parts::shell_for(
        "cs",
        context.window,
        metrics,
        view.sidebar(),
        region,
        bar,
        look,
    );
    Some(view.dialogs(root, context.window.width))
}

/// The people the ⊕ popout offers: those not listed yet whose name or
/// handle has what is searched, by name. Only the first few that match:
/// typing narrows them, and the provider is asked for the people it has
/// not sent yet (`lookup`).
fn offered_people(
    settings: &ChannelSettings,
    store: &Store,
    channel: &Channel,
    server: &Server,
) -> Vec<(String, UserId)> {
    let query = settings.add_query.trim().to_lowercase();
    let listed = settings.listed(channel);
    let handle = |user: UserId| {
        store
            .user(user)
            .map_or_else(|| user.to_string(), |account| account.name.clone())
    };
    let mut people: Vec<(String, UserId)> = store
        .members(server.id)
        .iter()
        .filter(|member| !listed.contains(&OverwriteTarget::Member(member.user)))
        .map(|member| {
            (
                store.display_name(member.user, Some(server.id)),
                member.user,
            )
        })
        .filter(|(name, user)| {
            name.to_lowercase().contains(&query) || handle(*user).to_lowercase().contains(&query)
        })
        .collect();
    people.sort_by_key(|(name, _)| name.to_lowercase());
    people.truncate(8);
    people
}

/// The faces the permissions page draws (2026-10-21: they kept their
/// initials): the server's owner, the people with an overwrite, and while
/// the ⊕ popout is open, the people it offers.
pub(crate) fn wanted(ui: &Ui, store: &Store, add: &mut dyn FnMut(Option<&RemoteImage>)) {
    /// How many people with an overwrite ask for their faces.
    const LISTED: usize = 50;
    let Some(settings) = ui
        .channel_settings
        .as_ref()
        .filter(|settings| settings.page == Page::Permissions)
    else {
        return;
    };
    let Some(channel) = store.channel(settings.channel) else {
        return;
    };
    let Some(server) = channel.server.and_then(|server| store.server(server)) else {
        return;
    };
    let id = Some(server.id);
    if let Some(owner) = server.owner {
        add(store.avatar_in(id, owner));
    }
    for user in settings
        .listed(channel)
        .into_iter()
        .filter_map(|target| match target {
            OverwriteTarget::Member(user) => Some(user),
            OverwriteTarget::Role(_) => None,
        })
        .take(LISTED)
    {
        add(store.avatar_in(id, user));
    }
    if settings.adding {
        for (_, user) in offered_people(settings, store, channel, server) {
            add(store.avatar_in(id, user));
        }
    }
}

/// What every part of the view reads.
struct View<'a> {
    settings: &'a ChannelSettings,
    store: &'a Store,
    channel: &'a Channel,
    server: &'a Server,
    abilities: &'a Abilities,
    look: &'a Look,
    metrics: Metrics,
    advanced: bool,
    /// Developer mode: the menus offer to copy ids.
    developer: bool,
    /// For the people's pictures already loaded.
    ui: &'a Ui,
}

impl View<'_> {
    fn category(&self) -> bool {
        self.channel.kind == ChannelKind::Category
    }

    /// The channel as its title names it: `#name`, or its name alone.
    fn title(&self) -> String {
        match self.channel.kind {
            ChannelKind::Text => format!("#{}", self.channel.name),
            _ => self.channel.name.clone(),
        }
    }

    fn sidebar(&self) -> Element {
        let page = |id: &str, label: &str, page: Page| SidebarEntry::Item {
            id: format!("cs:page:{id}"),
            label: label.into(),
            selected: self.settings.page == page,
            danger: false,
        };
        let (icon, parent) = match self.channel.kind {
            ChannelKind::Category => (Icon::FolderFill, false),
            kind => (
                crate::channel_list::glyph(kind).unwrap_or(if kind.is_voice() {
                    Icon::Volume
                } else {
                    Icon::Hash
                }),
                true,
            ),
        };
        let detail = self
            .channel
            .parent
            .filter(|_| parent)
            .and_then(|category| self.store.channel(category))
            .map(|category| category.name.clone());
        let mut entries = vec![
            SidebarEntry::Title {
                icon: Some(icon),
                name: self.channel.name.clone(),
                detail,
            },
            page("overview", t("Vista general"), Page::Overview),
            page("permissions", t("Permisos"), Page::Permissions),
        ];
        // Server administration, rest (P7).
        if self.channel.kind == ChannelKind::Forum {
            entries.push(page("forum", t("Foro"), Page::Forum));
        }
        // Invitations, integrations and webhooks: the provider has none.
        if self.abilities.manage.allowed() {
            entries.push(SidebarEntry::Separator);
            entries.push(SidebarEntry::Item {
                id: "cs:delete".into(),
                label: if self.category() {
                    t("Eliminar categoría")
                } else {
                    t("Eliminar canal")
                }
                .into(),
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

    fn overview(&self) -> Element {
        let look = self.look;
        let settings = self.settings;
        let channel = self.channel;
        let denied = self.abilities.manage.reason();
        let can = denied.is_none();
        let mut children = vec![parts::page_title(t("Vista general"), None, look)];
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        let name = settings
            .edit
            .name
            .clone()
            .unwrap_or_else(|| channel.name.clone());
        let (label, placeholder) = match channel.kind {
            ChannelKind::Category => (t("Nombre de la categoría"), t("Nueva categoría")),
            ChannelKind::Voice => (t("Nombre del canal"), t("Canal nuevo")),
            _ => (t("Nombre del canal"), t("canal-nuevo")),
        };
        children.push(
            Element::column(vec![
                parts::field_label(label, false, look),
                TextField::new(NAME, name.clone(), placeholder, look.palette())
                    .height(FIELD_HEIGHT)
                    .accessibility_label(label)
                    .enabled(can)
                    .render(),
            ])
            .gap(8.0),
        );
        let saved_as = final_name(channel.kind, &name);
        if saved_as.is_empty() {
            children.push(text(
                t("El nombre no puede quedar vacío."),
                13.0,
                look.danger,
            ));
        } else if channel.kind == ChannelKind::Text && saved_as != name {
            // Text channels take their names lowercase and dashed: say how it will be.
            children.push(line(
                tr!("Se va a llamar #{saved_as}", saved_as),
                12.0,
                look.muted,
            ));
        }
        if channel.kind == ChannelKind::Text {
            children.extend(self.text_overview(can));
        }
        parts::page("cs:scroll:overview", self.metrics.content, children)
    }

    /// The topic, slow mode and age restriction of a text channel.
    fn text_overview(&self, can: bool) -> Vec<Element> {
        let look = self.look;
        let settings = self.settings;
        let channel = self.channel;
        let topic = settings
            .edit
            .topic
            .clone()
            .unwrap_or_else(|| channel.topic.clone())
            .unwrap_or_default();
        let left = TOPIC_LIMIT.saturating_sub(topic.chars().count());
        let topic_field = Element::stack(vec![
            TextArea::new(
                TOPIC,
                topic,
                t("Cuéntale a todos para qué es este canal."),
                look.palette(),
            )
            .accessibility_label(t("Tema del canal"))
            .height(TOPIC_HEIGHT)
            .enabled(can)
            .render(),
            // What is left to write, at the field's lower right.
            Element::column(vec![
                Element::spacer().flex(1.0),
                Element::row(vec![
                    Element::spacer().flex(1.0),
                    line(left.to_string(), 14.0, look.muted).font_family(FontFamily::SansBold),
                    gap(14.0, 0.0),
                ]),
                gap(0.0, 10.0),
            ]),
        ])
        .size(None, Some(TOPIC_HEIGHT));
        let slowmode = settings.edit.slowmode_secs.unwrap_or(channel.slowmode_secs);
        let slowmode_select = own::field_select(
            &FieldSelect {
                id: "cs:slowmode".into(),
                name: t("Modo lento").into(),
                value: own::slowmode_label(slowmode),
                options: SLOWMODE
                    .iter()
                    .map(|(secs, label)| {
                        (
                            format!("cs:slowmode:{secs}"),
                            t(label).to_owned(),
                            *secs == slowmode,
                        )
                    })
                    .collect(),
                open: settings.slowmode_open,
                dismiss: "cs:slowmode:dismiss".into(),
                enabled: can,
                width: self.metrics.content,
            },
            look,
        );
        let nsfw = settings.edit.nsfw.unwrap_or(channel.nsfw);
        vec![
            Element::column(vec![
                parts::field_label(t("Tema del canal"), false, look),
                topic_field,
            ])
            .gap(8.0),
            Element::column(vec![
                parts::field_label(t("Modo lento"), false, look),
                slowmode_select,
                text(
                    t("Cuánto tiene que esperar cada miembro entre un mensaje y el siguiente en este canal."),
                    12.0,
                    look.muted,
                ),
            ])
            .gap(8.0),
            parts::divider(look),
            parts::switch_row(
                "cs:nsfw",
                t("Canal con restricción de edad"),
                Some(t("Marca el contenido de este canal como solo para personas mayores de edad.")),
                nsfw,
                can,
                look,
            ),
        ]
    }

    fn role(&self, target: OverwriteTarget) -> Option<&Role> {
        let OverwriteTarget::Role(id) = target else {
            return None;
        };
        self.server.roles.iter().find(|role| role.id == id)
    }

    fn role_color(&self, role: Option<&Role>) -> Color {
        self.look.ink(
            role.and_then(|role| role.color)
                .unwrap_or(DEFAULT_ROLE_COLOR),
        )
    }

    fn name_of(&self, user: UserId) -> String {
        self.store.display_name(user, Some(self.server.id))
    }

    fn accent_of(&self, user: UserId) -> Color {
        self.store
            .user(user)
            .map_or(self.look.faint, |account| self.look.ink(account.accent))
    }

    /// The picture `user` shows in this server, once loaded; [`wanted`]
    /// asks for it.
    fn image_of(&self, user: UserId) -> Option<limn::RasterImage> {
        self.ui
            .picture(self.store.avatar_in(Some(self.server.id), user))
    }

    fn handle_of(&self, user: UserId) -> String {
        self.store
            .user(user)
            .map_or_else(|| user.to_string(), |account| account.name.clone())
    }

    /// What a target is called: a role's name, or a person's.
    fn target_name(&self, target: OverwriteTarget) -> String {
        match target {
            OverwriteTarget::Role(_) => self
                .role(target)
                .map_or_else(|| t("Rol eliminado").to_owned(), |role| role.name.clone()),
            OverwriteTarget::Member(user) => self.name_of(user),
        }
    }

    /// The targets listed, as the reference orders them: roles from the
    /// highest, `@everyone`, and people by name.
    fn ordered_targets(&self) -> Vec<OverwriteTarget> {
        let everyone = everyone(self.channel);
        let listed = self.settings.listed(self.channel);
        let mut roles: Vec<OverwriteTarget> = listed
            .iter()
            .copied()
            .filter(|target| matches!(target, OverwriteTarget::Role(_)) && *target != everyone)
            .collect();
        roles.sort_by_key(|target| -self.role(*target).map_or(0, |role| role.position));
        let mut people: Vec<(String, OverwriteTarget)> = listed
            .iter()
            .filter_map(|target| match target {
                OverwriteTarget::Member(user) => {
                    Some((self.name_of(*user).to_lowercase(), *target))
                }
                OverwriteTarget::Role(_) => None,
            })
            .collect();
        people.sort_by(|a, b| a.0.cmp(&b.0));
        roles.push(everyone);
        roles.extend(people.into_iter().map(|(_, target)| target));
        roles
    }

    fn permissions(&self) -> Element {
        let look = self.look;
        let settings = self.settings;
        let channel = self.channel;
        let everyone = everyone(channel);
        let (title, description) = if self.category() {
            (
                t("Ajustes de la categoría"),
                t("Usa los permisos para elegir quién puede hacer qué en esta categoría."),
            )
        } else {
            (
                t("Permisos del canal"),
                t("Usa los permisos para elegir quién puede hacer qué en este canal."),
            )
        };
        let mut children = vec![parts::page_title(title, Some(description), look)];
        let denied = self.abilities.manage_permissions.reason();
        if let Some(reason) = denied {
            children.push(parts::denied(reason, look));
        }
        let category = channel
            .parent
            .filter(|_| !self.category())
            .and_then(|parent| self.store.channel(parent));
        if let Some(category) = category {
            children.push(own::sync_card(
                &category.name,
                sync_commands(channel, category).is_empty(),
                denied.is_none(),
                look,
            ));
        }
        let private_enabled = target_lock(self.store, channel, everyone).is_none()
            && permission_lock(channel, Permissions::VIEW).is_none();
        children.push(own::private_card(
            &PrivateCard {
                category: self.category(),
                on: settings.is_private(channel, everyone),
                enabled: private_enabled,
                roles: self.access_roles(),
                members: self.access_members(),
            },
            look,
        ));
        children.push(parts::divider(look));
        children.push(own::advanced_heading(self.advanced, look));
        if self.advanced {
            children.push(self.advanced_view());
        }
        parts::page("cs:scroll:permissions", self.metrics.content, children)
    }

    /// The roles that may see the channel while it is private.
    fn access_roles(&self) -> Vec<AccessLine> {
        let everyone = everyone(self.channel);
        self.ordered_targets()
            .into_iter()
            .filter(|target| {
                *target != everyone
                    && matches!(target, OverwriteTarget::Role(_))
                    && self
                        .settings
                        .shown(self.channel, *target)
                        .state(Permissions::VIEW)
                        == Some(true)
            })
            .map(|target| {
                let OverwriteTarget::Role(id) = target else {
                    unreachable!("only roles are kept")
                };
                AccessLine {
                    name: self.target_name(target),
                    detail: t("Rol").into(),
                    color: self.role_color(self.role(target)),
                    person: false,
                    image: None,
                    note: None,
                    remove: target_lock(self.store, self.channel, target)
                        .is_none()
                        .then(|| format!("cs:access:role:{id}:remove")),
                }
            })
            .collect()
    }

    /// Who may see the channel while it is private: its owner always, then
    /// the people let in.
    fn access_members(&self) -> Vec<AccessLine> {
        let owner = self.server.owner;
        let mut lines: Vec<AccessLine> = owner
            .map(|user| AccessLine {
                name: self.name_of(user),
                detail: self.handle_of(user),
                color: self.accent_of(user),
                person: true,
                image: self.image_of(user),
                note: Some(t("Propietario del servidor").into()),
                remove: None,
            })
            .into_iter()
            .collect();
        lines.extend(self.ordered_targets().into_iter().filter_map(|target| {
            let OverwriteTarget::Member(user) = target else {
                return None;
            };
            let allowed = self
                .settings
                .shown(self.channel, target)
                .state(Permissions::VIEW)
                == Some(true);
            (allowed && Some(user) != owner).then(|| AccessLine {
                name: self.name_of(user),
                detail: self.handle_of(user),
                color: self.accent_of(user),
                person: true,
                image: self.image_of(user),
                note: None,
                remove: target_lock(self.store, self.channel, target)
                    .is_none()
                    .then(|| format!("cs:access:member:{user}:remove")),
            })
        }));
        lines
    }

    /// The list of roles and people with overwrites, and the permissions of
    /// the one chosen.
    fn advanced_view(&self) -> Element {
        let look = self.look;
        let settings = self.settings;
        let channel = self.channel;
        let everyone = everyone(channel);
        let selected = settings.selected.unwrap_or(everyone);
        let items: Vec<TargetItem> = self
            .ordered_targets()
            .into_iter()
            .map(|target| {
                let (key, id) = match target {
                    OverwriteTarget::Role(role) => ("role", role.0),
                    OverwriteTarget::Member(user) => ("member", user.0),
                };
                let base = format!("cs:target:{key}:{id}");
                let mark = match target {
                    _ if target == everyone => TargetMark::Everyone,
                    OverwriteTarget::Role(_) => {
                        TargetMark::Role(self.role_color(self.role(target)))
                    }
                    OverwriteTarget::Member(user) => {
                        TargetMark::Member(self.accent_of(user), self.image_of(user))
                    }
                };
                let mut menu = if target == everyone {
                    Vec::new()
                } else {
                    vec![
                        MenuAction::new(format!("{base}:remove"), t("Eliminar"))
                            .icon(Icon::Trash)
                            .danger()
                            .denied(target_lock(self.store, channel, target)),
                        MenuAction::new(
                            format!("{base}:copy-id"),
                            if key == "role" {
                                t("Copiar ID del rol")
                            } else {
                                t("Copiar ID del usuario")
                            },
                        )
                        .icon(Icon::Copy),
                    ]
                };
                menu.retain(|action| self.developer || !action.id.ends_with(":copy-id"));
                TargetItem {
                    id: base,
                    name: self.target_name(target),
                    mark,
                    selected: target == selected,
                    menu,
                }
            })
            .collect();
        let can_add = self.abilities.manage_permissions.allowed();
        let add = own::add_popout(
            can_add,
            settings.adding && can_add,
            &settings.add_query,
            &self.add_choices(),
            look,
        );
        let list = own::target_list(&items, add, look);
        let mut column = Vec::new();
        let locked = target_lock(self.store, channel, selected);
        if let Some(reason) = locked
            .as_deref()
            .filter(|_| self.abilities.manage_permissions.allowed())
        {
            column.push(parts::denied(reason, look));
        }
        let overwrite = settings.shown(channel, selected);
        for (title, permissions) in own::permission_groups(channel.kind) {
            column.push(parts::permission_heading(t(title), false, look));
            let rows = permissions
                .iter()
                .map(|(permission, label, description)| {
                    let description = if *permission == Permissions::VIEW && selected == everyone {
                        format!(
                            "{} {}",
                            t(description),
                            own::everyone_view_note(channel.kind)
                        )
                    } else {
                        t(description).to_owned()
                    };
                    // The target's lock is said once, over the list.
                    let unheld = locked
                        .is_none()
                        .then(|| permission_lock(channel, *permission))
                        .flatten();
                    let control = own::tri_state(
                        &own::permission_id(*permission),
                        overwrite.state(*permission),
                        locked.is_none() && unheld.is_none(),
                        t(label),
                        look,
                    );
                    parts::permission_entry(
                        t(label),
                        &description,
                        unheld.as_deref(),
                        control,
                        look,
                    )
                })
                .collect();
            column.push(Element::column(rows));
        }
        Element::row(vec![
            list,
            gap(TARGET_GAP, 0.0),
            Element::column(column).gap(12.0).flex(1.0),
            // The reference keeps the controls 16 px clear of the column's end.
            gap(16.0, 0.0),
        ])
    }

    /// What the ⊕ popout offers: roles within reach and people, not listed
    /// yet, that match what is searched.
    fn add_choices(&self) -> Vec<AddChoice> {
        let settings = self.settings;
        let channel = self.channel;
        let query = settings.add_query.trim().to_lowercase();
        let listed = settings.listed(channel);
        let rank = self.store.my_rank(self.server.id);
        let mut choices: Vec<AddChoice> = self
            .server
            .assignable_roles()
            .into_iter()
            .filter(|role| {
                role.position < rank
                    && !listed.contains(&OverwriteTarget::Role(role.id))
                    && role.name.to_lowercase().contains(&query)
            })
            .map(|role| AddChoice {
                id: format!("cs:add:role:{}", role.id),
                name: role.name.clone(),
                color: self.role_color(Some(role)),
                person: false,
                image: None,
            })
            .collect();
        let people = offered_people(settings, self.store, channel, self.server);
        choices.extend(people.into_iter().map(|(name, user)| AddChoice {
            id: format!("cs:add:member:{user}"),
            name,
            color: self.accent_of(user),
            person: true,
            image: self.image_of(user),
        }));
        choices
    }

    /// Who the dialog offers to let into a private channel: roles within
    /// reach that cannot see it yet and match what is searched, and, once
    /// something is searched (after an `@` for people only), the people that
    /// match. Who is chosen stays in view.
    fn audience(&self, access: &super::Access) -> Audience {
        let channel = self.channel;
        let query = access.query.trim().to_lowercase();
        let people_only = query.starts_with('@');
        let query = query.trim_start_matches('@');
        let rank = self.store.my_rank(self.server.id);
        let sees = |target: OverwriteTarget| {
            self.settings
                .shown(channel, target)
                .state(Permissions::VIEW)
                == Some(true)
        };
        let roles = if people_only {
            Vec::new()
        } else {
            self.server
                .assignable_roles()
                .into_iter()
                .filter(|role| {
                    role.position < rank
                        && !sees(OverwriteTarget::Role(role.id))
                        && role.name.to_lowercase().contains(query)
                })
                .map(|role| Choice {
                    id: format!("cs:pick:role:{}", role.id),
                    label: role.name.clone(),
                    chosen: access.roles.contains(&role.id),
                })
                .collect()
        };
        let searching = people_only || !query.is_empty();
        let members = (searching || !access.members.is_empty()).then(|| {
            let mut found: Vec<Choice> = self
                .store
                .members(self.server.id)
                .iter()
                .filter(|member| {
                    Some(member.user) != self.server.owner
                        && !sees(OverwriteTarget::Member(member.user))
                })
                .filter_map(|member| {
                    let name = self.name_of(member.user);
                    let chosen = access.members.contains(&member.user);
                    let matches = searching && name.to_lowercase().contains(query);
                    (chosen || matches).then(|| Choice {
                        id: format!("cs:pick:member:{}", member.user),
                        label: name,
                        chosen,
                    })
                })
                .collect();
            found.sort_by_key(|choice| !choice.chosen);
            found.truncate(8 + access.members.len());
            found
        });
        Audience {
            query: access.query.clone(),
            roles,
            members,
        }
    }

    /// The dialogs over the settings: who may see it, taking the category's
    /// permissions, and deleting it.
    fn dialogs(&self, root: Element, window_width: f32) -> Element {
        let look = self.look;
        let settings = self.settings;
        let palette = look.menu_palette();
        let mut root = root;
        if let Some(access) = &settings.access {
            root = Modal::new(
                root,
                true,
                t("Añadir miembros o roles"),
                self.title(),
                "cs:pick:cancel",
                palette,
            )
            .width(Modal::fit_width(window_width, 510.0))
            .body(audience_picker(ACCESS_QUERY, &self.audience(access), look))
            .footer(own::dialog_buttons(
                ("cs:pick:cancel", t("Cancelar")),
                ("cs:pick:done", t("Listo"), true),
                look,
            ))
            .initial_focus(ACCESS_QUERY)
            .render();
        }
        let category = self
            .channel
            .parent
            .and_then(|parent| self.store.channel(parent));
        if let (true, Some(category)) = (settings.sync, category) {
            let question = Element::formatted_text(vec![
                TextSpan::new(t("¿Quieres que "), look.muted, 15.0, FontFamily::Sans),
                TextSpan::new(self.title(), look.text, 15.0, FontFamily::SansBold),
                TextSpan::new(
                    t(" tenga los permisos de "),
                    look.muted,
                    15.0,
                    FontFamily::Sans,
                ),
                TextSpan::new(category.name.clone(), look.text, 15.0, FontFamily::SansBold),
                TextSpan::new(
                    t("? Sus permisos propios se reemplazan."),
                    look.muted,
                    15.0,
                    FontFamily::Sans,
                ),
            ]);
            let danger = ComponentPalette {
                text: Color::rgb(255, 255, 255),
                surface: look.danger,
                hover: look.danger,
                selected: look.danger,
                ..palette
            };
            root = Modal::new(
                root,
                true,
                t("¿Sincronizar permisos?"),
                "",
                "cs:sync:cancel",
                palette,
            )
            .width(Modal::fit_width(window_width, 425.0))
            .body(question)
            .footer(
                Element::column(vec![
                    Button::new(
                        "cs:sync:cancel",
                        t("Mantener los permisos actuales"),
                        palette,
                    )
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Spacious)
                    .render(),
                    Button::new("cs:sync:confirm", t("Sincronizar permisos"), danger)
                        .variant(ButtonVariant::Solid)
                        .size(ControlSize::Spacious)
                        .render()
                        .center_content(),
                ])
                .gap(9.0),
            )
            .initial_focus("cs:sync:cancel")
            .render();
        }
        let (title, confirm, description) = if self.category() {
            (
                t("Eliminar categoría"),
                t("Eliminar categoría"),
                tr!(
                    "¿Seguro que quieres eliminar la categoría {name}? Sus canales quedan sin categoría. No se puede deshacer.",
                    name = self.channel.name
                ),
            )
        } else {
            (
                t("Eliminar canal"),
                t("Eliminar canal"),
                tr!(
                    "¿Seguro que quieres eliminar {name}? No se puede deshacer.",
                    name = self.title()
                ),
            )
        };
        // Cancelar takes the focus: Enter does not delete.
        AlertDialog::new(
            root,
            settings.delete,
            title,
            description,
            "cs:delete:confirm",
            "cs:delete:cancel",
            palette,
        )
        .confirm_label(confirm)
        .cancel_label(t("Cancelar"))
        .destructive(look.danger)
        .width(Modal::fit_width(window_width, 425.0))
        .render()
    }
}
