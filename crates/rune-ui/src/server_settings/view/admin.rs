//! What the administration added with recorrido 19 shows: the
//! integrations page, the buttons it adds to other pages, and its dialogs.

use limn::{
    AlertDialog, Button, Component, DropdownOption, Element, Empty, FormField, Icon, Modal, Select,
    TextField,
};
use rune_components::server_settings::{self as parts, ItemLine, MenuAction};
use rune_components::{line, strong_line, text};
use rune_i18n::{t, tr};
use rune_model::{valid_webhook_name, ChannelId, UserId, WebhookKind};
use rune_provider::{ExpressionKind, ProviderKind};

use super::{footer, more_button, View, FIELD_HEIGHT};
use crate::server_settings::advanced::{
    invite_channels, invite_link, timed_out, webhook_channels, INVITE_AGES, INVITE_USES,
    PRUNE_DAYS, TIMEOUTS, TIMEOUT_REASON, UPLOAD_DESCRIPTION, UPLOAD_NAME, UPLOAD_TAGS,
    WEBHOOK_NAME,
};

impl View<'_> {
    fn simulated(&self) -> bool {
        self.store
            .provider
            .as_ref()
            .is_some_and(|info| info.kind == ProviderKind::Simulated)
    }

    fn channel_label(&self, channel: Option<ChannelId>) -> String {
        channel
            .and_then(|channel| self.store.channel(channel))
            .map_or_else(
                || t("Elegí un canal").to_owned(),
                |found| format!("#{}", found.name),
            )
    }

    /// A dropdown of a dialog, open while `select` names it.
    pub(super) fn choice(
        &self,
        select: &'static str,
        current: String,
        options: Vec<DropdownOption>,
        label: &str,
    ) -> Element {
        Select::new(
            format!("ss:{}", select.replacen('-', ":", 1)),
            current,
            options,
            self.settings.advanced.select == Some(select),
            "ss:select:dismiss",
            self.look.menu_palette(),
        )
        .menu_palette(self.look.menu_palette())
        .accessibility_label(label)
        .width(380.0)
        .menu_width(380.0)
        .render()
    }

    /// The timeout entries of a member's menu.
    pub(super) fn timeout_actions(&self, user: UserId, name: &str) -> Vec<MenuAction> {
        let lock = parts_lock(self, user);
        if timed_out(self.store, self.server.id, user) {
            vec![MenuAction::new(
                format!("ss:member:{user}:untimeout"),
                tr!("Quitar el aislamiento a {name}", name),
            )
            .icon(Icon::Clock)
            .denied(lock)]
        } else {
            vec![MenuAction::new(
                format!("ss:member:{user}:timeout"),
                tr!("Aislar a {name}", name),
            )
            .icon(Icon::Clock)
            .danger()
            .denied(lock)]
        }
    }

    /// What the members page adds above its table: pruning.
    pub(super) fn prune_button(&self) -> Option<Element> {
        self.store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.prune)
            .then(|| {
                Button::new("ss:prune", t("Podar"), self.look.palette())
                    .icon(Icon::Users)
                    .variant(limn::ButtonVariant::Outline)
                    .button_size(limn::ButtonSize::Small)
                    .enabled(self.abilities.prune.allowed())
                    .accessibility_label(t("Podar miembros inactivos"))
                    .tooltip(
                        self.abilities
                            .prune
                            .reason()
                            .unwrap_or(t("Sacar a quienes no tienen rol y no se conectan")),
                    )
                    .render()
            })
    }

    /// What the members page says under its table: how many are loaded, and
    /// the button for more.
    pub(super) fn members_footer(&self, shown: usize, children: &mut Vec<Element>) {
        let look = self.look;
        let server = self.server.id;
        let known = self.store.members(server).len();
        let pages = self.store.member_pages(server);
        let total = pages
            .and_then(|pages| pages.total)
            .map(|total| total as usize);
        children.push(line(
            match total {
                Some(total) if total > known => {
                    if shown == 1 {
                        tr!("Mostrando {shown} miembro de {total}", shown, total)
                    } else {
                        tr!("Mostrando {shown} miembros de {total}", shown, total)
                    }
                }
                _ if shown == 1 => tr!("Mostrando {shown} miembro", shown),
                _ => tr!("Mostrando {shown} miembros", shown),
            },
            14.0,
            look.muted,
        ));
        if pages.is_some_and(|pages| pages.next.is_some()) && self.abilities.member_pages.allowed()
        {
            if !self.settings.members_query.trim().is_empty() {
                children.push(text(
                    tr!("Se buscó entre los {known} miembros cargados.", known),
                    13.0,
                    look.muted,
                ));
            }
            children.push(more_button("ss:members:more", look));
        }
    }

    /// The button that makes an invitation, over the invitations page.
    pub(super) fn new_invite_button(&self) -> Option<Element> {
        self.store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.create_invites)
            .then(|| {
                parts::primary_button(
                    "ss:invite:new",
                    t("Crear invitación"),
                    self.abilities.create_invites.allowed(),
                    self.look,
                )
            })
    }

    /// The button that adds an emoji or a sticker, and what it takes.
    pub(super) fn upload_bar(&self, stickers: bool) -> Element {
        let look = self.look;
        let supported = self
            .store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.upload_expressions);
        if !supported {
            return text(
                t("Este proveedor no permite agregar nuevos: acá se renombran y eliminan."),
                13.0,
                look.muted,
            );
        }
        let (id, label, limits) = if stickers {
            (
                "ss:upload:sticker",
                t("Subir sticker"),
                t("PNG, JPEG, WebP o GIF. Se guarda a 320 × 320, hasta 512 KB; un GIF de ese tamaño queda animado."),
            )
        } else {
            (
                "ss:upload:emoji",
                t("Subir emoji"),
                t("PNG, JPEG, WebP o GIF. Se guarda a 128 × 128; un GIF de hasta 256 KB queda animado."),
            )
        };
        Element::row(vec![
            text(limits, 13.0, look.muted).flex(1.0).centered_y(),
            parts::primary_button(
                id,
                label,
                self.abilities.upload_expressions.allowed()
                    && self.settings.advanced.picking.is_none(),
                look,
            )
            .centered_y(),
        ])
        .gap(16.0)
    }

    /// The webhooks and integrations of the server.
    pub(super) fn integrations(&self) -> Element {
        let look = self.look;
        let server = self.server.id;
        let mut children = vec![parts::page_title(
            t("Integraciones"),
            Some(t("Los webhooks publican mensajes de otras aplicaciones o sitios en un canal; las integraciones conectan aplicaciones y servicios con el servidor.")),
            look,
        )];
        // Webhooks.
        let webhooks_denied = self.abilities.manage_webhooks.reason();
        children.push(
            Element::row(vec![
                strong_line("Webhooks", 16.0, look.text)
                    .flex(1.0)
                    .centered_y(),
                parts::primary_button(
                    "ss:webhook:new",
                    t("Nuevo webhook"),
                    webhooks_denied.is_none(),
                    look,
                )
                .centered_y(),
            ])
            .gap(16.0),
        );
        match (webhooks_denied, self.store.webhooks(server)) {
            (Some(reason), _) => children.push(parts::denied(reason, look)),
            (None, None) => children.push(line(t("Cargando…"), 14.0, look.muted)),
            (None, Some([])) => children.push(
                Empty::new(
                    t("No hay webhooks"),
                    t("Creá uno para que otra aplicación publique en un canal."),
                    look.palette(),
                )
                .icon(Icon::Bolt)
                .framed()
                .render(),
            ),
            (None, Some(list)) => {
                let rows: Vec<Element> = list
                    .iter()
                    .map(|webhook| {
                        let mut notes = vec![self.channel_label(Some(webhook.channel))];
                        notes.push(
                            match webhook.kind {
                                WebhookKind::Incoming => t("Entrante"),
                                WebhookKind::Follower => t("Sigue a otro servidor"),
                                WebhookKind::Application => t("De una aplicación"),
                            }
                            .to_owned(),
                        );
                        if let Some(creator) = webhook.creator {
                            notes.push(tr!("Creado por {name}", name = self.name_of(creator)));
                        }
                        let mut actions = Vec::new();
                        if webhook.kind == WebhookKind::Incoming {
                            actions.push(("edit", Icon::Edit, t("Editar")));
                        }
                        actions.push(("delete", Icon::Trash, t("Eliminar")));
                        parts::item_row(
                            &ItemLine {
                                key: format!("webhook:{}", webhook.id),
                                name: webhook.name.clone(),
                                notes,
                                glyph: Icon::Bolt,
                                actions,
                                denied: None,
                            },
                            look,
                        )
                    })
                    .collect();
                children.push(Element::column(rows));
            }
        }
        children.push(parts::divider(look));
        // Integrations.
        children.push(strong_line(t("Aplicaciones y servicios"), 16.0, look.text));
        match (
            self.abilities.manage_integrations.reason(),
            self.store.integrations(server),
        ) {
            (Some(reason), _) => children.push(parts::denied(reason, look)),
            (None, None) => children.push(line(t("Cargando…"), 14.0, look.muted)),
            (None, Some([])) => children.push(
                Empty::new(
                    t("No hay integraciones"),
                    t("Nadie conectó una aplicación o un servicio todavía."),
                    look.palette(),
                )
                .icon(Icon::Puzzle)
                .framed()
                .render(),
            ),
            (None, Some(list)) => {
                let rows: Vec<Element> = list
                    .iter()
                    .map(|integration| {
                        let mut notes = vec![integration.service.clone()];
                        if !integration.enabled {
                            notes.push(t("Desactivada").into());
                        }
                        if let Some(user) = integration.user {
                            notes.push(tr!("Agregada por {name}", name = self.name_of(user)));
                        }
                        parts::item_row(
                            &ItemLine {
                                key: format!("integration:{}", integration.id),
                                name: integration.name.clone(),
                                notes,
                                glyph: Icon::Puzzle,
                                actions: vec![("delete", Icon::Trash, t("Quitar"))],
                                denied: None,
                            },
                            look,
                        )
                    })
                    .collect();
                children.push(Element::column(rows));
            }
        }
        if self.simulated() {
            children.push(text(
                t("Simulados: estos webhooks no publican nada y las integraciones no conectan nada."),
                13.0,
                look.muted,
            ));
        }
        parts::page("ss:scroll:integrations", self.metrics.content, children)
    }

    /// The dialogs of these pages, over `root`.
    pub(super) fn admin_dialogs(&self, mut root: Element) -> Element {
        let look = self.look;
        let palette = look.menu_palette();
        let advanced = &self.settings.advanced;
        if let Some(draft) = &advanced.timeout {
            let name = self.name_of(draft.user);
            let lengths = TIMEOUTS
                .iter()
                .enumerate()
                .map(|(index, (_, label))| {
                    DropdownOption::new(format!("ss:timeout-length:{index}"), t(label))
                        .selected(index == draft.length)
                })
                .collect();
            root = AlertDialog::new(
                root,
                true,
                tr!("Aislar a {name}", name),
                t("Mientras dure no puede escribir, reaccionar, hablar en la voz ni unirse a llamadas del servidor."),
                "ss:timeout:confirm",
                "ss:timeout:cancel",
                palette,
            )
            .body(
                Element::column(vec![
                    FormField::new(
                        t("Duración"),
                        self.choice(
                            "timeout-length",
                            t(TIMEOUTS[draft.length].1).to_owned(),
                            lengths,
                            t("Duración del aislamiento"),
                        ),
                        palette,
                    )
                    .render(),
                    FormField::new(
                        t("Motivo (opcional)"),
                        TextField::new(
                            TIMEOUT_REASON,
                            draft.reason.clone(),
                            t("Queda en el registro de auditoría"),
                            palette,
                        )
                        .accessibility_label(t("Motivo"))
                        .height(FIELD_HEIGHT)
                        .render(),
                        palette,
                    )
                    .render(),
                ])
                .gap(10.0),
            )
            .confirm_label(t("Aislar"))
            .cancel_label(t("Cancelar"))
            .destructive(look.danger)
            .width(440.0)
            .render();
        }
        if let Some(draft) = &advanced.invite {
            root = match &draft.made {
                Some(code) => {
                    let link = invite_link(self.store, code);
                    let mut body = vec![Element::row(vec![
                        TextField::new("ss:invite:link", link.clone(), "", palette)
                            .accessibility_label(t("Enlace de la invitación"))
                            .height(FIELD_HEIGHT)
                            .enabled(false)
                            .render()
                            .flex(1.0),
                        Button::new("ss:invite:copy", t("Copiar"), look.accent_palette())
                            .render()
                            .centered_y(),
                    ])
                    .gap(8.0)];
                    if self.simulated() {
                        body.push(text(
                            t("Simulada: este código no lleva a ningún lado."),
                            13.0,
                            look.muted,
                        ));
                    }
                    Modal::new(
                        root,
                        true,
                        t("Invitación creada"),
                        t("Compartí el enlace con quien quieras que entre."),
                        "ss:invite:done",
                        palette,
                    )
                    .width(480.0)
                    .body(Element::column(body).gap(8.0))
                    .footer(Element::row(vec![
                        Element::spacer().flex(1.0),
                        Button::new("ss:invite:done", t("Listo"), look.menu_palette()).render(),
                    ]))
                    .render()
                }
                None => {
                    let channels: Vec<DropdownOption> = invite_channels(self.store, self.server.id)
                        .into_iter()
                        .map(|channel| {
                            DropdownOption::new(
                                format!("ss:invite-channel:{channel}"),
                                self.channel_label(Some(channel)),
                            )
                            .selected(draft.channel == Some(channel))
                        })
                        .collect();
                    let option = |prefix: &str, index: usize, label: &str, chosen: usize| {
                        DropdownOption::new(format!("ss:{prefix}:{index}"), t(label))
                            .selected(index == chosen)
                    };
                    let ages = INVITE_AGES
                        .iter()
                        .enumerate()
                        .map(|(index, (_, label))| option("invite-age", index, label, draft.age))
                        .collect();
                    let uses = INVITE_USES
                        .iter()
                        .enumerate()
                        .map(|(index, (_, label))| option("invite-uses", index, label, draft.uses))
                        .collect();
                    Modal::new(
                        root,
                        true,
                        t("Crear invitación"),
                        t("Elegí adónde lleva, cuánto dura y cuántas veces se puede usar."),
                        "ss:invite:cancel",
                        palette,
                    )
                    .width(440.0)
                    .body(
                        Element::column(vec![
                            FormField::new(
                                t("Canal"),
                                self.choice(
                                    "invite-channel",
                                    self.channel_label(draft.channel),
                                    channels,
                                    t("Canal de la invitación"),
                                ),
                                palette,
                            )
                            .render(),
                            FormField::new(
                                t("Vence después de"),
                                self.choice(
                                    "invite-age",
                                    t(INVITE_AGES[draft.age].1).to_owned(),
                                    ages,
                                    t("Vencimiento"),
                                ),
                                palette,
                            )
                            .render(),
                            FormField::new(
                                t("Cantidad máxima de usos"),
                                self.choice(
                                    "invite-uses",
                                    t(INVITE_USES[draft.uses].1).to_owned(),
                                    uses,
                                    t("Usos"),
                                ),
                                palette,
                            )
                            .render(),
                            parts::switch_row(
                                "ss:invite:temporary",
                                t("Miembro temporal"),
                                Some(t(
                                    "Quien entre se va al desconectarse, salvo que le den un rol.",
                                )),
                                draft.temporary,
                                true,
                                look,
                            ),
                        ])
                        .gap(12.0),
                    )
                    .footer(footer(
                        "ss:invite:cancel",
                        Button::new(
                            "ss:invite:create",
                            if draft.sending.is_some() {
                                t("Creando…")
                            } else {
                                t("Crear")
                            },
                            look.accent_palette(),
                        )
                        .enabled(draft.sending.is_none() && draft.channel.is_some())
                        .render(),
                        look,
                    ))
                    .render()
                }
            };
        }
        if let Some(draft) = &advanced.upload {
            let expression = draft.expression();
            let problem = expression.problem();
            let (title, edge) = match draft.kind {
                ExpressionKind::Emoji => (t("Subir emoji"), 96.0),
                ExpressionKind::Sticker => (t("Subir sticker"), 160.0),
            };
            let mut fields = vec![FormField::new(
                t("Nombre"),
                TextField::new(UPLOAD_NAME, draft.name.clone(), "", palette)
                    .accessibility_label(t("Nombre"))
                    .height(FIELD_HEIGHT)
                    .render(),
                palette,
            )
            .hint(match draft.kind {
                ExpressionKind::Emoji => {
                    t("De 2 a 32 letras sin acentos, números o guiones bajos.")
                }
                ExpressionKind::Sticker => t("De 2 a 30 caracteres."),
            })
            .render()];
            if draft.kind == ExpressionKind::Sticker {
                fields.push(
                    FormField::new(
                        t("Emoji relacionado"),
                        TextField::new(UPLOAD_TAGS, draft.tags.clone(), "🦀", palette)
                            .accessibility_label(t("Emoji relacionado"))
                            .height(FIELD_HEIGHT)
                            .render(),
                        palette,
                    )
                    .hint(t("Un emoji o una palabra con la que se lo encuentra."))
                    .render(),
                );
                fields.push(
                    FormField::new(
                        t("Descripción (opcional)"),
                        TextField::new(UPLOAD_DESCRIPTION, draft.description.clone(), "", palette)
                            .accessibility_label(t("Descripción"))
                            .height(FIELD_HEIGHT)
                            .render(),
                        palette,
                    )
                    .render(),
                );
            }
            let mut notes = Vec::new();
            if draft.image.is_gif() {
                notes.push(t("GIF: queda animado.").to_owned());
            }
            if self.simulated() {
                notes.push(t("Simulado: la imagen no se publica, sólo el nombre.").to_owned());
            }
            let mut column = vec![Element::row(vec![
                Element::raster_image(draft.preview.clone()).size(Some(edge), Some(edge)),
                Element::column(fields).gap(10.0).flex(1.0),
            ])
            .gap(16.0)];
            if !notes.is_empty() {
                column.push(text(notes.join(" "), 13.0, look.muted));
            }
            if let Some(problem) = problem.filter(|_| !draft.name.trim().is_empty()) {
                column.push(text(problem, 13.0, look.danger));
            }
            root = Modal::new(
                root,
                true,
                title,
                t("Elegí cómo se llama. Se agrega al servidor para todos."),
                "ss:upload:cancel",
                palette,
            )
            .width(520.0)
            .body(Element::column(column).gap(10.0))
            .footer(footer(
                "ss:upload:cancel",
                Button::new(
                    "ss:upload:confirm",
                    if draft.sending.is_some() {
                        t("Subiendo…")
                    } else {
                        title
                    },
                    look.accent_palette(),
                )
                .enabled(draft.sending.is_none() && problem.is_none())
                .render(),
                look,
            ))
            .initial_focus(UPLOAD_NAME)
            .render();
        }
        if let Some(draft) = &advanced.prune {
            let days = PRUNE_DAYS[draft.days];
            let options = PRUNE_DAYS
                .iter()
                .enumerate()
                .map(|(index, days)| {
                    DropdownOption::new(format!("ss:prune-days:{index}"), days_label(*days))
                        .selected(index == draft.days)
                })
                .collect();
            let said = match draft.count {
                None => t("Contando…").to_owned(),
                Some(None) => tr!(
                    "Se va a sacar a quienes no tienen rol y no se conectaron en {days}.",
                    days = days_label(days).to_lowercase()
                ),
                Some(Some(0)) => t("Nadie cumple esas condiciones.").to_owned(),
                Some(Some(1)) => tr!(
                    "Se va a sacar a 1 miembro sin rol que no se conectó en {days}.",
                    days = days_label(days).to_lowercase()
                ),
                Some(Some(count)) => tr!(
                    "Se va a sacar a {count} miembros sin rol que no se conectaron en {days}.",
                    count,
                    days = days_label(days).to_lowercase()
                ),
            };
            root = AlertDialog::new(
                root,
                true,
                t("Podar miembros"),
                t("Pueden volver a entrar con una invitación nueva."),
                "ss:prune:confirm",
                "ss:prune:cancel",
                palette,
            )
            .body(
                Element::column(vec![
                    FormField::new(
                        t("Sin conectarse durante"),
                        self.choice("prune-days", days_label(days), options, t("Días")),
                        palette,
                    )
                    .render(),
                    text(said, 14.0, look.text),
                ])
                .gap(10.0),
            )
            .confirm_label(if draft.removing {
                t("Podando…")
            } else {
                t("Podar")
            })
            .cancel_label(t("Cancelar"))
            .confirm_enabled(
                !draft.removing && draft.count.is_some() && draft.count.flatten() != Some(0),
            )
            .destructive(look.danger)
            .width(440.0)
            .render();
        }
        if let Some(draft) = &advanced.webhook {
            let channels = webhook_channels(self.store, self.server.id)
                .into_iter()
                .map(|channel| {
                    DropdownOption::new(
                        format!("ss:webhook-channel:{channel}"),
                        self.channel_label(Some(channel)),
                    )
                    .selected(draft.channel == Some(channel))
                })
                .collect();
            let valid = valid_webhook_name(draft.name.trim());
            root = Modal::new(
                root,
                true,
                if draft.webhook.is_some() {
                    t("Editar webhook")
                } else {
                    t("Nuevo webhook")
                },
                t("Publica en el canal que elijas con el nombre que le pongas."),
                "ss:webhook:cancel",
                palette,
            )
            .width(440.0)
            .body(
                Element::column(vec![
                    FormField::new(
                        t("Nombre"),
                        TextField::new(
                            WEBHOOK_NAME,
                            draft.name.clone(),
                            t("Avisos de CI"),
                            palette,
                        )
                        .accessibility_label(t("Nombre del webhook"))
                        .height(FIELD_HEIGHT)
                        .render(),
                        palette,
                    )
                    .hint(t("Hasta 80 caracteres, sin «discord» ni «clyde»."))
                    .render(),
                    FormField::new(
                        t("Canal"),
                        self.choice(
                            "webhook-channel",
                            self.channel_label(draft.channel),
                            channels,
                            t("Canal del webhook"),
                        ),
                        palette,
                    )
                    .render(),
                ])
                .gap(10.0),
            )
            .footer(footer(
                "ss:webhook:cancel",
                Button::new("ss:webhook:save", t("Guardar"), look.accent_palette())
                    .enabled(valid && draft.channel.is_some())
                    .render(),
                look,
            ))
            .initial_focus(WEBHOOK_NAME)
            .render();
        }
        if let Some(webhook) = advanced.delete_webhook {
            let name = self
                .store
                .webhooks(self.server.id)
                .and_then(|list| list.iter().find(|known| known.id == webhook))
                .map(|known| known.name.clone())
                .unwrap_or_default();
            root = AlertDialog::new(
                root,
                true,
                t("Eliminar webhook"),
                tr!("¿Seguro que querés eliminar «{name}»? Lo que lo use deja de poder publicar. No se puede deshacer.", name),
                "ss:webhook-delete:confirm",
                "ss:webhook-delete:cancel",
                palette,
            )
            .confirm_label(t("Eliminar"))
            .cancel_label(t("Cancelar"))
            .destructive(look.danger)
            .width(425.0)
            .render();
        }
        if let Some(integration) = advanced.delete_integration {
            let found = self
                .store
                .integrations(self.server.id)
                .and_then(|list| list.iter().find(|known| known.id == integration));
            let name = found.map(|known| known.name.clone()).unwrap_or_default();
            let bot = found.is_some_and(|known| known.bot.is_some());
            root = AlertDialog::new(
                root,
                true,
                t("Quitar integración"),
                if bot {
                    tr!(
                        "¿Seguro que querés quitar «{name}»? Su bot también sale del servidor.",
                        name
                    )
                } else {
                    tr!(
                        "¿Seguro que querés quitar «{name}»? Deja de estar conectada al servidor.",
                        name
                    )
                },
                "ss:integration-delete:confirm",
                "ss:integration-delete:cancel",
                palette,
            )
            .confirm_label(t("Quitar"))
            .cancel_label(t("Cancelar"))
            .destructive(look.danger)
            .width(425.0)
            .render();
        }
        root
    }
}

/// Why the account cannot time `user` out, when it cannot.
fn parts_lock(view: &View<'_>, user: UserId) -> Option<String> {
    crate::server_settings::member_lock(view.store, view.server.id, user, &view.abilities.timeout)
}

fn days_label(days: u32) -> String {
    match days {
        1 => t("1 día").to_owned(),
        days => tr!("{days} días", days),
    }
}
