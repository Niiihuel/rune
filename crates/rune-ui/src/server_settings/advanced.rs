//! The administration added with recorrido 19: timeouts, new invitations
//! with their link, new emojis and stickers, pages of members, pruning,
//! webhooks and integrations. The settings' rules hold: nothing changes
//! before the provider says so (a deletion shows at once and comes back if
//! refused, see `Store::ahead`), and the permission is checked again when
//! a dialog is confirmed; if it was lost, nothing is sent and a notice says
//! why.

use std::time::Instant;

use limn::RasterImage;
use rune_i18n::{es, t, tr};
use rune_model::{
    valid_webhook_name, ChannelId, ChannelKind, IntegrationId, Permissions, UserId, WebhookId,
    WebhookKind,
};
use rune_provider::{Command, Created, ExpressionImage, ExpressionKind, Failure, NewExpression};
use rune_state::{Applied, Store};

use super::{member_lock, refused, set_input, Page, ServerSettings};
use crate::{Intent, Ui};

/// The fields of the dialogs, whose text limn keeps by id.
pub(crate) const TIMEOUT_REASON: &str = "ss:timeout:reason";
pub(crate) const UPLOAD_NAME: &str = "ss:upload:name";
pub(crate) const UPLOAD_TAGS: &str = "ss:upload:tags";
pub(crate) const UPLOAD_DESCRIPTION: &str = "ss:upload:description";
pub(crate) const WEBHOOK_NAME: &str = "ss:webhook:name";

/// How long a timeout lasts, in minutes, as the reference offers them.
pub const TIMEOUTS: [(i64, &str); 6] = [
    (1, es("60 segundos")),
    (5, es("5 minutos")),
    (10, es("10 minutos")),
    (60, es("1 hora")),
    (24 * 60, es("1 día")),
    (7 * 24 * 60, es("1 semana")),
];
/// How long a new invitation lasts, in seconds; 0 never expires.
pub const INVITE_AGES: [(u32, &str); 7] = [
    (30 * 60, es("30 minutos")),
    (60 * 60, es("1 hora")),
    (6 * 60 * 60, es("6 horas")),
    (12 * 60 * 60, es("12 horas")),
    (24 * 60 * 60, es("1 día")),
    (7 * 24 * 60 * 60, es("7 días")),
    (0, es("Nunca")),
];
/// The history a ban deletes, as the dialog offers it: its seconds, and what
/// it says. Discord's most is seven days.
pub const BAN_HISTORY: [(u32, &str); 7] = [
    (0, es("No eliminar nada")),
    (60 * 60, es("Última hora")),
    (6 * 60 * 60, es("Últimas 6 horas")),
    (12 * 60 * 60, es("Últimas 12 horas")),
    (24 * 60 * 60, es("Últimas 24 horas")),
    (3 * 24 * 60 * 60, es("Últimos 3 días")),
    (7 * 24 * 60 * 60, es("Últimos 7 días")),
];
/// How many times a new invitation can be used; 0 has no limit.
pub const INVITE_USES: [(u32, &str); 7] = [
    (0, es("Sin límite")),
    (1, es("1 uso")),
    (5, es("5 usos")),
    (10, es("10 usos")),
    (25, es("25 usos")),
    (50, es("50 usos")),
    (100, es("100 usos")),
];
/// The days without activity a prune can ask for.
pub const PRUNE_DAYS: [u32; 3] = [1, 7, 30];

/// Timing someone out: whom, for how long (an index of [`TIMEOUTS`]), why.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeoutDraft {
    pub user: UserId,
    pub length: usize,
    pub reason: String,
}

/// A new invitation being chosen, sent (`sending`) or made (`made`, its code).
#[derive(Clone, Debug, PartialEq)]
pub struct InviteDraft {
    pub channel: Option<ChannelId>,
    pub age: usize,
    pub uses: usize,
    pub temporary: bool,
    pub sending: Option<u64>,
    pub made: Option<String>,
}

/// A new emoji or sticker: its picture, ready to send, and what is written.
#[derive(Clone, Debug, PartialEq)]
pub struct UploadDraft {
    pub kind: ExpressionKind,
    pub image: ExpressionImage,
    pub preview: RasterImage,
    pub name: String,
    pub tags: String,
    pub description: String,
    pub sending: Option<u64>,
}

impl UploadDraft {
    pub fn expression(&self) -> NewExpression {
        NewExpression {
            kind: self.kind,
            name: self.name.trim().to_owned(),
            image: self.image.clone(),
            tags: self.tags.trim().to_owned(),
            description: self.description.trim().to_owned(),
        }
    }
}

/// A prune being counted (`request`), shown (`count`) or done (`removing`).
#[derive(Clone, Debug, PartialEq)]
pub struct PruneDraft {
    /// An index of [`PRUNE_DAYS`].
    pub days: usize,
    pub request: Option<u64>,
    /// `Some(None)`: counted, but the provider did not say how many.
    pub count: Option<Option<u32>>,
    pub removing: bool,
}

/// A webhook being made (`webhook: None`) or changed.
#[derive(Clone, Debug, PartialEq)]
pub struct WebhookDraft {
    pub webhook: Option<WebhookId>,
    pub name: String,
    pub channel: Option<ChannelId>,
}

/// What floats over the settings for these pages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Advanced {
    pub timeout: Option<TimeoutDraft>,
    pub invite: Option<InviteDraft>,
    pub upload: Option<UploadDraft>,
    /// The picture being chosen with this request, for this kind.
    pub picking: Option<(u64, ExpressionKind)>,
    pub prune: Option<PruneDraft>,
    pub webhook: Option<WebhookDraft>,
    pub delete_webhook: Option<WebhookId>,
    pub delete_integration: Option<IntegrationId>,
    /// The dropdown of a dialog that is open: `timeout-length`, `invite-channel`,
    /// `invite-age`, `invite-uses`, `prune-days`, `webhook-channel`, `removal-history`.
    pub select: Option<&'static str>,
}

impl Advanced {
    /// Close the innermost of these; false when none was open.
    pub(super) fn close(&mut self) -> bool {
        self.select.take().is_some()
            || self.timeout.take().is_some()
            || self.invite.take().is_some()
            || self.upload.take().is_some()
            || self.prune.take().is_some()
            || self.webhook.take().is_some()
            || self.delete_webhook.take().is_some()
            || self.delete_integration.take().is_some()
    }

    fn toggle(&mut self, select: &'static str) {
        self.select = if self.select == Some(select) {
            None
        } else {
            Some(select)
        };
    }
}

/// The channels of `server` an invitation can lead to, in the list's order.
pub(crate) fn invite_channels(store: &Store, server: rune_model::ServerId) -> Vec<ChannelId> {
    store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| {
            channel.kind != ChannelKind::Category
                && channel.permissions.contains(Permissions::CREATE_INVITE)
        })
        .map(|channel| channel.id)
        .collect()
}

/// The channels of `server` a webhook can post in, in the list's order.
pub(crate) fn webhook_channels(store: &Store, server: rune_model::ServerId) -> Vec<ChannelId> {
    store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| {
            matches!(
                channel.kind,
                ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum
            )
        })
        .map(|channel| channel.id)
        .collect()
}

/// The link of the invitation `code`, or its code where the provider has no links.
pub(crate) fn invite_link(store: &Store, code: &str) -> String {
    match store
        .provider
        .as_ref()
        .and_then(|info| info.capabilities.invite_links)
    {
        Some(base) => format!("{base}{code}"),
        None => code.to_owned(),
    }
}

/// What a page asks for when it opens, beyond what the settings ask: the
/// first page of members, the webhooks and integrations.
pub(super) fn load(
    settings: &ServerSettings,
    page: Page,
    ui: &mut Ui,
    store: &Store,
) -> Vec<Command> {
    let server = settings.server;
    let Some(abilities) = store.server_abilities(server) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    match page {
        Page::Members if abilities.member_pages.allowed() => {
            commands.push(Command::LoadMembers {
                request: ui.fresh_request(),
                server,
                after: None,
                query: settings.rest.query,
            });
        }
        Page::Integrations => {
            if abilities.manage_webhooks.allowed() {
                commands.push(Command::LoadWebhooks {
                    request: ui.fresh_request(),
                    server,
                });
            }
            if abilities.manage_integrations.allowed() {
                commands.push(Command::LoadIntegrations {
                    request: ui.fresh_request(),
                    server,
                });
            }
        }
        _ => {}
    }
    commands
}

/// What a field's new text changes; false when it is not one of these.
pub(super) fn typed(settings: &mut ServerSettings, id: &str, value: &str) -> bool {
    let advanced = &mut settings.advanced;
    let field = match id {
        TIMEOUT_REASON => advanced.timeout.as_mut().map(|draft| &mut draft.reason),
        UPLOAD_NAME => advanced.upload.as_mut().map(|draft| &mut draft.name),
        UPLOAD_TAGS => advanced.upload.as_mut().map(|draft| &mut draft.tags),
        UPLOAD_DESCRIPTION => advanced.upload.as_mut().map(|draft| &mut draft.description),
        WEBHOOK_NAME => advanced.webhook.as_mut().map(|draft| &mut draft.name),
        _ => return false,
    };
    if let Some(field) = field {
        *field = value.to_owned();
    }
    true
}

/// What Enter in a field of these dialogs confirms.
pub(super) fn submitted(id: &str) -> Option<&'static str> {
    Some(match id {
        TIMEOUT_REASON => "timeout:confirm",
        UPLOAD_NAME | UPLOAD_TAGS | UPLOAD_DESCRIPTION => "upload:confirm",
        WEBHOOK_NAME => "webhook:save",
        _ => return None,
    })
}

/// A click on one of these; false when it is not one of them.
pub(super) fn clicked(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    parts: &[&str],
    now: Instant,
    intents: &mut Vec<Intent>,
) -> bool {
    let server = settings.server;
    let Some(abilities) = store.server_abilities(server) else {
        return false;
    };
    let user = |part: &str| part.parse().ok().map(UserId);
    let index = |part: &str| part.parse::<usize>().ok();
    let advanced = &mut settings.advanced;
    match parts {
        ["select", "dismiss"] => advanced.select = None,

        // The history a ban deletes, in its dialog.
        ["removal", "history"] => advanced.toggle("removal-history"),
        ["removal-history", choice] => {
            if let (Some(removal), Some(choice)) = (&mut settings.removal, index(choice)) {
                if choice < BAN_HISTORY.len() {
                    removal.history = choice;
                }
            }
            advanced.select = None;
        }

        // Timeouts, from the members page.
        ["member", id, "timeout"] => {
            settings.member_menu = None;
            if let Some(user) = user(id).filter(|user| {
                member_lock(store, server, *user, &abilities.timeout).is_none()
                    && Some(*user) != store.me()
                    && store.member_of(server, *user).is_some()
            }) {
                intents.push(set_input(TIMEOUT_REASON, ""));
                intents.push(Intent::Focus(TIMEOUT_REASON.into()));
                advanced.timeout = Some(TimeoutDraft {
                    user,
                    length: 0,
                    reason: String::new(),
                });
            }
        }
        ["member", id, "untimeout"] => {
            settings.member_menu = None;
            let Some(user) = user(id) else { return true };
            match member_lock(store, server, user, &abilities.timeout) {
                None if timed_out(store, server, user) => {
                    intents.push(Intent::Command(Command::TimeoutMember {
                        server,
                        user,
                        until: None,
                        reason: None,
                    }));
                }
                None => {}
                reason => refused(ui, t("No se quitó el aislamiento"), reason, now),
            }
        }
        ["timeout", "length"] => advanced.toggle("timeout-length"),
        ["timeout-length", choice] => {
            if let (Some(draft), Some(choice)) = (&mut advanced.timeout, index(choice)) {
                if choice < TIMEOUTS.len() {
                    draft.length = choice;
                }
            }
            advanced.select = None;
        }
        ["timeout", "confirm"] => {
            if let Some(draft) = advanced.timeout.take() {
                match member_lock(store, server, draft.user, &abilities.timeout) {
                    None => {
                        let minutes = TIMEOUTS[draft.length.min(TIMEOUTS.len() - 1)].0;
                        let reason = Some(draft.reason.trim().to_owned()).filter(|r| !r.is_empty());
                        intents.push(Intent::Command(Command::TimeoutMember {
                            server,
                            user: draft.user,
                            until: Some(store.wall().plus_minutes(minutes)),
                            reason,
                        }));
                    }
                    reason => refused(ui, t("No se aisló a la persona"), reason, now),
                }
            }
            advanced.select = None;
        }
        ["timeout", "cancel"] => {
            advanced.timeout = None;
            advanced.select = None;
        }

        // A new invitation, from the invitations page.
        ["invite", "new"] if abilities.create_invites.allowed() => {
            let channel = invite_channels(store, server).first().copied();
            advanced.invite = Some(InviteDraft {
                channel,
                age: 5,
                uses: 0,
                temporary: false,
                sending: None,
                made: None,
            });
        }
        ["invite", "channel"] => advanced.toggle("invite-channel"),
        ["invite", "age"] => advanced.toggle("invite-age"),
        ["invite", "uses"] => advanced.toggle("invite-uses"),
        ["invite-channel", id] => {
            let chosen = id.parse().ok().map(ChannelId);
            if let Some(draft) = &mut advanced.invite {
                if chosen.is_some_and(|channel| invite_channels(store, server).contains(&channel)) {
                    draft.channel = chosen;
                }
            }
            advanced.select = None;
        }
        ["invite-age", choice] => {
            if let (Some(draft), Some(choice)) = (&mut advanced.invite, index(choice)) {
                if choice < INVITE_AGES.len() {
                    draft.age = choice;
                }
            }
            advanced.select = None;
        }
        ["invite-uses", choice] => {
            if let (Some(draft), Some(choice)) = (&mut advanced.invite, index(choice)) {
                if choice < INVITE_USES.len() {
                    draft.uses = choice;
                }
            }
            advanced.select = None;
        }
        ["invite", "temporary"] => {
            if let Some(draft) = &mut advanced.invite {
                draft.temporary = !draft.temporary;
            }
        }
        ["invite", "create"] => {
            advanced.select = None;
            let Some(draft) = advanced
                .invite
                .as_mut()
                .filter(|draft| draft.sending.is_none() && draft.made.is_none())
            else {
                return true;
            };
            let channel = draft
                .channel
                .filter(|channel| invite_channels(store, server).contains(channel));
            match (abilities.create_invites.reason(), channel) {
                (None, Some(channel)) => {
                    let request = ui.fresh_request();
                    draft.sending = Some(request);
                    intents.push(Intent::Command(Command::CreateInvite {
                        request,
                        server,
                        channel,
                        max_age_secs: INVITE_AGES[draft.age.min(INVITE_AGES.len() - 1)].0,
                        max_uses: INVITE_USES[draft.uses.min(INVITE_USES.len() - 1)].0,
                        temporary: draft.temporary,
                    }));
                }
                (None, None) => refused(
                    ui,
                    t("No se creó la invitación"),
                    Some(t("No podés invitar a ningún canal de este servidor.").into()),
                    now,
                ),
                (reason, _) => {
                    advanced.invite = None;
                    refused(
                        ui,
                        t("No se creó la invitación"),
                        reason.map(str::to_owned),
                        now,
                    );
                }
            }
        }
        ["invite", "copy"] => {
            if let Some(code) = advanced
                .invite
                .as_ref()
                .and_then(|draft| draft.made.clone())
            {
                copy_invite(ui, store, &code, now, intents);
            }
        }
        ["invite", code, "copy"] => {
            if store
                .invites(server)
                .is_some_and(|invites| invites.iter().any(|invite| invite.code == *code))
            {
                copy_invite(ui, store, code, now, intents);
            }
        }
        ["invite", "done" | "cancel"] => {
            advanced.invite = None;
            advanced.select = None;
        }

        // New emojis and stickers, from their pages.
        ["upload", kind @ ("emoji" | "sticker")] if abilities.upload_expressions.allowed() => {
            if advanced.picking.is_none() {
                let kind = if *kind == "emoji" {
                    ExpressionKind::Emoji
                } else {
                    ExpressionKind::Sticker
                };
                let request = ui.fresh_request();
                advanced.picking = Some((request, kind));
                intents.push(Intent::PickExpressionImage { request, kind });
            }
        }
        ["upload", "confirm"] => {
            let Some(draft) = advanced
                .upload
                .as_mut()
                .filter(|draft| draft.sending.is_none())
            else {
                return true;
            };
            let expression = draft.expression();
            match abilities.upload_expressions.reason() {
                // Kept open: the dialog says what is missing.
                None if expression.problem().is_some() => {}
                None => {
                    let request = ui.fresh_request();
                    draft.sending = Some(request);
                    intents.push(Intent::Command(Command::CreateExpression {
                        request,
                        server,
                        expression,
                    }));
                }
                reason => {
                    advanced.upload = None;
                    refused(ui, t("No se agregó"), reason.map(str::to_owned), now);
                }
            }
        }
        ["upload", "cancel"] => advanced.upload = None,

        // Pruning, from the members page.
        ["prune"] if abilities.prune.allowed() => {
            let request = ui.fresh_request();
            advanced.prune = Some(PruneDraft {
                days: 1,
                request: Some(request),
                count: None,
                removing: false,
            });
            intents.push(Intent::Command(Command::PruneMembers {
                request,
                server,
                days: PRUNE_DAYS[1],
                remove: false,
            }));
        }
        ["prune", "days"] => advanced.toggle("prune-days"),
        ["prune-days", choice] => {
            advanced.select = None;
            let Some(choice) = index(choice).filter(|choice| *choice < PRUNE_DAYS.len()) else {
                return true;
            };
            if let Some(draft) = advanced.prune.as_mut().filter(|draft| !draft.removing) {
                let request = ui.fresh_request();
                *draft = PruneDraft {
                    days: choice,
                    request: Some(request),
                    count: None,
                    removing: false,
                };
                intents.push(Intent::Command(Command::PruneMembers {
                    request,
                    server,
                    days: PRUNE_DAYS[choice],
                    remove: false,
                }));
            }
        }
        ["prune", "confirm"] => {
            advanced.select = None;
            let Some(draft) = advanced
                .prune
                .as_mut()
                .filter(|draft| !draft.removing && draft.count.flatten() != Some(0))
            else {
                return true;
            };
            match abilities.prune.reason() {
                None => {
                    let request = ui.fresh_request();
                    draft.request = Some(request);
                    draft.removing = true;
                    intents.push(Intent::Command(Command::PruneMembers {
                        request,
                        server,
                        days: PRUNE_DAYS[draft.days],
                        remove: true,
                    }));
                }
                reason => {
                    advanced.prune = None;
                    refused(ui, t("No se podó"), reason.map(str::to_owned), now);
                }
            }
        }
        ["prune", "cancel"] => {
            advanced.prune = None;
            advanced.select = None;
        }

        // Webhooks and integrations.
        ["webhook", "new"] if abilities.manage_webhooks.allowed() => {
            intents.push(set_input(WEBHOOK_NAME, ""));
            intents.push(Intent::Focus(WEBHOOK_NAME.into()));
            advanced.webhook = Some(WebhookDraft {
                webhook: None,
                name: String::new(),
                channel: webhook_channels(store, server).first().copied(),
            });
        }
        ["webhook", id, "edit"] if abilities.manage_webhooks.allowed() => {
            let found = id.parse().ok().map(WebhookId).and_then(|id| {
                store
                    .webhooks(server)?
                    .iter()
                    .find(|webhook| webhook.id == id && webhook.kind == WebhookKind::Incoming)
            });
            if let Some(found) = found {
                intents.push(set_input(WEBHOOK_NAME, &found.name));
                intents.push(Intent::Focus(WEBHOOK_NAME.into()));
                advanced.webhook = Some(WebhookDraft {
                    webhook: Some(found.id),
                    name: found.name.clone(),
                    channel: Some(found.channel),
                });
            }
        }
        ["webhook", "channel"] => advanced.toggle("webhook-channel"),
        ["webhook-channel", id] => {
            let chosen = id.parse().ok().map(ChannelId);
            if let Some(draft) = &mut advanced.webhook {
                if chosen.is_some_and(|channel| webhook_channels(store, server).contains(&channel))
                {
                    draft.channel = chosen;
                }
            }
            advanced.select = None;
        }
        ["webhook", "save"] => {
            advanced.select = None;
            let Some(draft) = advanced.webhook.clone() else {
                return true;
            };
            let name = draft.name.trim().to_owned();
            let channel = draft
                .channel
                .filter(|channel| webhook_channels(store, server).contains(channel));
            match (abilities.manage_webhooks.reason(), channel) {
                // Kept open: the dialog says what a name needs.
                (None, _) if !valid_webhook_name(&name) => {}
                (None, Some(channel)) => {
                    advanced.webhook = None;
                    match draft.webhook {
                        None => intents.push(Intent::Command(Command::CreateWebhook {
                            server,
                            channel,
                            name,
                        })),
                        Some(webhook) => {
                            let Some(known) = store
                                .webhooks(server)
                                .and_then(|list| list.iter().find(|known| known.id == webhook))
                            else {
                                refused(
                                    ui,
                                    t("No se guardó el webhook"),
                                    Some(t("Ya no existe.").into()),
                                    now,
                                );
                                return true;
                            };
                            let name = (known.name != name).then_some(name);
                            let channel = (known.channel != channel).then_some(channel);
                            if name.is_some() || channel.is_some() {
                                intents.push(Intent::Command(Command::EditWebhook {
                                    server,
                                    webhook,
                                    name,
                                    channel,
                                }));
                            }
                        }
                    }
                }
                (None, None) => refused(
                    ui,
                    t("No se guardó el webhook"),
                    Some(t("Elegí un canal de texto.").into()),
                    now,
                ),
                (reason, _) => {
                    advanced.webhook = None;
                    refused(
                        ui,
                        t("No se guardó el webhook"),
                        reason.map(str::to_owned),
                        now,
                    );
                }
            }
        }
        ["webhook", "cancel"] => {
            advanced.webhook = None;
            advanced.select = None;
        }
        ["webhook", id, "delete"] if abilities.manage_webhooks.allowed() => {
            advanced.delete_webhook = id.parse().ok().map(WebhookId).filter(|id| {
                store
                    .webhooks(server)
                    .is_some_and(|list| list.iter().any(|known| known.id == *id))
            });
        }
        ["webhook-delete", "confirm"] => {
            if let Some(webhook) = advanced.delete_webhook.take() {
                match abilities.manage_webhooks.reason() {
                    None => {
                        intents.push(Intent::Command(Command::DeleteWebhook { server, webhook }))
                    }
                    reason => refused(
                        ui,
                        t("No se eliminó el webhook"),
                        reason.map(str::to_owned),
                        now,
                    ),
                }
            }
        }
        ["webhook-delete", "cancel"] => advanced.delete_webhook = None,
        ["integration", id, "delete"] if abilities.manage_integrations.allowed() => {
            advanced.delete_integration = id.parse().ok().map(IntegrationId).filter(|id| {
                store
                    .integrations(server)
                    .is_some_and(|list| list.iter().any(|known| known.id == *id))
            });
        }
        ["integration-delete", "confirm"] => {
            if let Some(integration) = advanced.delete_integration.take() {
                match abilities.manage_integrations.reason() {
                    None => intents.push(Intent::Command(Command::DeleteIntegration {
                        server,
                        integration,
                    })),
                    reason => refused(
                        ui,
                        t("No se quitó la integración"),
                        reason.map(str::to_owned),
                        now,
                    ),
                }
            }
        }
        ["integration-delete", "cancel"] => advanced.delete_integration = None,

        // More members.
        ["members", "more"] => {
            let next = store
                .member_pages(server)
                .and_then(|pages| pages.next.clone());
            if let (Some(after), true) = (next, abilities.member_pages.allowed()) {
                intents.push(Intent::Command(Command::LoadMembers {
                    request: ui.fresh_request(),
                    server,
                    after: Some(after),
                    query: settings.rest.query,
                }));
            }
        }
        _ => return false,
    }
    true
}

/// Whether `user` is timed out in `server` now.
pub(crate) fn timed_out(store: &Store, server: rune_model::ServerId, user: UserId) -> bool {
    store
        .member_of(server, user)
        .and_then(|member| member.restrictions.timeout_until)
        .is_some_and(|until| until > store.wall())
}

fn copy_invite(ui: &mut Ui, store: &Store, code: &str, now: Instant, intents: &mut Vec<Intent>) {
    let has_link = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.invite_links.is_some());
    ui.notify(
        if has_link {
            t("Enlace copiado")
        } else {
            t("Código copiado")
        },
        None,
        false,
        now,
    );
    intents.push(Intent::Copy(invite_link(store, code)));
}

/// What an event changes in these dialogs.
pub(super) fn on_applied(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    applied: &Applied,
    now: Instant,
) {
    let server = settings.server;
    let advanced = &mut settings.advanced;
    match applied {
        Applied::InviteCreated {
            request,
            server: of,
            code,
        } if *of == server => {
            if let Some(draft) = advanced
                .invite
                .as_mut()
                .filter(|draft| draft.sending == Some(*request))
            {
                draft.sending = None;
                draft.made = Some(code.clone());
            }
        }
        Applied::Created {
            request,
            created: Created::Expression(of, _),
        } if *of == server => {
            if let Some(draft) = advanced
                .upload
                .take_if(|draft| draft.sending == Some(*request))
            {
                let what = match draft.kind {
                    ExpressionKind::Emoji => t("Emoji agregado"),
                    ExpressionKind::Sticker => t("Sticker agregado"),
                };
                ui.notify(what, None, false, now);
            }
        }
        Applied::Pruned {
            request,
            server: of,
            count,
            removed,
            ..
        } if *of == server => {
            let Some(draft) = advanced
                .prune
                .as_mut()
                .filter(|draft| draft.request == Some(*request))
            else {
                return;
            };
            if *removed {
                advanced.prune = None;
                let detail = count.map(|count| match count {
                    1 => t("Se sacó a 1 miembro.").to_owned(),
                    count => tr!("Se sacó a {count} miembros.", count),
                });
                ui.notify(t("Poda hecha"), detail, false, now);
            } else {
                draft.request = None;
                draft.count = Some(*count);
            }
        }
        Applied::Failed { failure, .. } => match failure {
            Failure::NewInvite { request, .. } => {
                if let Some(draft) = advanced
                    .invite
                    .as_mut()
                    .filter(|draft| draft.sending == Some(*request))
                {
                    draft.sending = None;
                }
            }
            Failure::NewExpression { request, .. } => {
                if let Some(draft) = advanced
                    .upload
                    .as_mut()
                    .filter(|draft| draft.sending == Some(*request))
                {
                    draft.sending = None;
                }
            }
            Failure::Prune { request, .. } => {
                advanced
                    .prune
                    .take_if(|draft| draft.request == Some(*request));
            }
            _ => {}
        },
        Applied::Webhooks(of) if *of == server => {
            let gone = |id: &WebhookId| {
                store
                    .webhooks(server)
                    .is_none_or(|list| !list.iter().any(|known| known.id == *id))
            };
            if advanced
                .webhook
                .as_ref()
                .and_then(|draft| draft.webhook)
                .is_some_and(|id| gone(&id))
            {
                advanced.webhook = None;
            }
            if advanced.delete_webhook.as_ref().is_some_and(gone) {
                advanced.delete_webhook = None;
            }
        }
        Applied::Integrations(of) if *of == server => {
            advanced.delete_integration.take_if(|id| {
                store
                    .integrations(server)
                    .is_none_or(|list| !list.iter().any(|known| known.id == *id))
            });
        }
        Applied::Members(of) if *of == server => {
            let gone = |user: UserId| store.member_of(server, user).is_none();
            if advanced
                .timeout
                .as_ref()
                .is_some_and(|draft| gone(draft.user))
            {
                advanced.timeout = None;
            }
        }
        _ => {}
    }
}

/// What came of choosing the picture asked for with `request`: with the
/// settings still waiting for it, the dialog to name the new expression
/// opens with it. A file that could not be used says why; a cancelled
/// choice changes nothing.
pub fn expression_image_picked(
    ui: &mut Ui,
    request: u64,
    result: Result<Option<(ExpressionImage, RasterImage)>, String>,
    now: Instant,
) -> Vec<Intent> {
    // Server administration, rest (P7): a role's picture is chosen the same way.
    if let Some(intents) = super::rest::role_icon_picked(ui, request, &result, now) {
        return intents;
    }
    let Some(settings) = ui.server_settings.as_mut() else {
        return Vec::new();
    };
    let Some((_, kind)) = settings
        .advanced
        .picking
        .filter(|(waiting, _)| *waiting == request)
    else {
        return Vec::new();
    };
    settings.advanced.picking = None;
    match result {
        Ok(Some((image, preview))) => {
            settings.advanced.upload = Some(UploadDraft {
                kind,
                image,
                preview,
                name: String::new(),
                tags: String::new(),
                description: String::new(),
                sending: None,
            });
            vec![
                set_input(UPLOAD_NAME, ""),
                set_input(UPLOAD_TAGS, ""),
                set_input(UPLOAD_DESCRIPTION, ""),
                Intent::Focus(UPLOAD_NAME.into()),
            ]
        }
        Ok(None) => Vec::new(),
        Err(reason) => {
            ui.notify(t("No se pudo usar la imagen"), Some(reason), true, now);
            Vec::new()
        }
    }
}
