//! Apps from the account (P3), as the official web client does it (static
//! REA of its public bundles, 2026-10-17) and as Serein does it, without
//! copying code: the command index of a server or of a conversation with a
//! bot; `POST /interactions` of type 2 (a command), 3 (a part of a message),
//! 4 (suggestions for an option) and 5 (a modal) with the Gateway's session;
//! and the Gateway's answers, matched by nonce. A modal's file fields (type
//! 19) name files staged as a message's are, by their index in
//! `data.attachments` (comandos-de-apps, as Serein's `valid_uploads`).
//! Only fixtures: none of this was tried against the service.

use rune_i18n::{es, t};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use rune_model::components::{Component, MenuKind};
use rune_model::interactions::{
    AppCommand, CommandAccess, CommandKind, CommandOption, CommandUse, FieldValue, Interaction,
    Modal, ModalField, ModalInput, OptionChoice, OptionKind, OptionValue,
};
use rune_model::{ChannelId, Permissions, RoleId, ServerId, UserId};
use rune_provider::{Command, Event, Failure, PreparedUpload, RequestId, Snapshot};
use serde_json::{json, Map, Value};
use zeroize::Zeroizing;

use crate::{wire, LoginError};

/// Discord's epoch, for the snowflake nonces the official client sends.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;
/// Commands an index may hold, and its bytes on the wire.
const MAX_COMMANDS: usize = 2000;
pub(crate) const MAX_INDEX: usize = 4 * 1024 * 1024;
/// The largest interaction RUNE sends.
const MAX_BODY: usize = 256 * 1024;
/// Interactions remembered while their answer may come.
const KEPT: usize = 16;

/// What the session keeps of apps.
#[derive(Default)]
pub(crate) struct Apps {
    /// The Gateway session interactions go with.
    session: Option<Zeroizing<String>>,
    /// Interactions admitted, the oldest first.
    sent: VecDeque<Sent>,
    /// The commands last listed, as the service wrote them, by id: a
    /// command runs with its own definition.
    index: Arc<Mutex<HashMap<u64, Value>>>,
    /// The modal last opened: its id, its app and its parts as written.
    modal: Option<(u64, u64, Value)>,
}

/// An interaction on its way, until the Gateway answers it.
struct Sent {
    request: RequestId,
    channel: ChannelId,
    nonce: String,
    body: Value,
    /// It asked for suggestions.
    suggest: bool,
}

/// What a command of P3 does over HTTP.
pub(crate) enum Work {
    /// Read the index at `path`, for `channel`.
    Index {
        path: String,
        request: RequestId,
        channel: ChannelId,
        server: Option<ServerId>,
        roles: Vec<RoleId>,
        cache: Arc<Mutex<HashMap<u64, Value>>>,
    },
    /// Stage `files` in `channel`, then send `body`, naming them, to
    /// `/interactions`.
    Post {
        body: Value,
        channel: ChannelId,
        files: Vec<PreparedUpload>,
    },
}

pub(crate) fn is_app_command(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadAppCommands { .. } | Command::Interact { .. } | Command::SuggestChoices { .. }
    )
}

/// The channel of a command of P3.
fn channel_of(command: &Command) -> Option<ChannelId> {
    match command {
        Command::LoadAppCommands { channel, .. }
        | Command::Interact { channel, .. }
        | Command::SuggestChoices { channel, .. } => Some(*channel),
        _ => None,
    }
}

/// Whether the account may use apps in the command's channel: it sees it,
/// and in a server, it may use apps' commands there.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let channel = channel_of(command).ok_or_else(wire::invalid)?;
    let found = snapshot
        .channels
        .iter()
        .find(|known| known.id == channel && crate::messages::holds_chat(known.kind))
        .ok_or_else(|| LoginError::new(es("La conversación no está disponible.")))?;
    crate::messages::unrestricted(snapshot, channel)?;
    let needs = match (command, found.server) {
        (
            Command::Interact {
                interaction: Interaction::Part { .. } | Interaction::Modal { .. },
                ..
            },
            _,
        )
        | (_, None) => Permissions::VIEW,
        (_, Some(_)) => Permissions::VIEW | Permissions::USE_APP_COMMANDS,
    };
    if !found.permissions.contains(needs) {
        return Err(LoginError::new(es(
            "No tienes permiso para usar aplicaciones en el canal.",
        )));
    }
    // Files are staged in the channel, as a message's are, within the same
    // bounds as a forum post's.
    if let Command::Interact { files, .. } = command {
        if !files.is_empty() && !found.permissions.contains(Permissions::ATTACH) {
            return Err(LoginError::new(es(
                "No tienes permiso para adjuntar archivos en el canal.",
            )));
        }
        let ceiling = crate::uploads::CEILING;
        if files.len() > ceiling.max_files
            || files
                .iter()
                .map(|file| file.metadata().size_bytes)
                .sum::<u64>()
                > ceiling.max_message_bytes
        {
            return Err(LoginError::new(es(
                "El formulario supera los archivos o el tamaño permitidos.",
            )));
        }
    }
    Ok(())
}

/// A snowflake for now, with `request` in its low bits, as the official
/// client's nonces are.
fn nonce(request: RequestId) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    (now.saturating_sub(DISCORD_EPOCH) << 22 | (request & 0x3F_FFFF)).to_string()
}

impl Apps {
    /// A new connection: interactions of the last one are not answered.
    pub fn ready(&mut self, session: Zeroizing<String>) {
        self.session = Some(session);
        self.sent.clear();
        self.modal = None;
    }

    /// Prepare `command` to go: what it sends, checked against what the
    /// session knows; why not, when it cannot.
    pub fn admit(
        &mut self,
        snapshot: &Snapshot,
        messages: &crate::messages::Messages,
        command: &Command,
    ) -> Result<(), LoginError> {
        allowed(snapshot, command)?;
        let (request, channel, body, suggest) = match command {
            Command::LoadAppCommands { .. } => return Ok(()),
            Command::Interact {
                request,
                channel,
                interaction,
                files,
            } => (
                *request,
                *channel,
                self.body(snapshot, messages, *channel, interaction, files)?,
                false,
            ),
            Command::SuggestChoices {
                request,
                channel,
                command: used,
            } => (
                *request,
                *channel,
                self.command_body(snapshot, *channel, used, true)?,
                true,
            ),
            _ => return Err(wire::invalid()),
        };
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))?;
        let nonce = nonce(request);
        let mut body = body;
        body["session_id"] = json!(session.as_str());
        body["nonce"] = json!(nonce);
        if serde_json::to_vec(&body).map_or(true, |bytes| bytes.len() > MAX_BODY) {
            return Err(LoginError::new(es(
                "La interacción es demasiado grande: no se envió.",
            )));
        }
        self.sent.retain(|kept| kept.request != request);
        if self.sent.len() >= KEPT {
            self.sent.pop_front();
        }
        self.sent.push_back(Sent {
            request,
            channel,
            nonce,
            body,
            suggest,
        });
        Ok(())
    }

    /// What `command` does over HTTP, once admitted.
    pub fn work(&self, snapshot: &Snapshot, command: &Command) -> Option<Work> {
        match command {
            Command::LoadAppCommands { request, channel } => {
                let found = snapshot
                    .channels
                    .iter()
                    .find(|known| known.id == *channel)?;
                let me = snapshot.me.as_ref()?.id;
                let path = match found.server {
                    Some(server) => format!("/guilds/{server}/application-command-index"),
                    None => format!("/channels/{channel}/application-command-index"),
                };
                let roles = found
                    .server
                    .and_then(|server| {
                        snapshot
                            .members
                            .iter()
                            .find(|member| member.server == server && member.user == me)
                    })
                    .map(|member| member.roles.clone())
                    .unwrap_or_default();
                Some(Work::Index {
                    path,
                    request: *request,
                    channel: *channel,
                    server: found.server,
                    roles,
                    cache: self.index.clone(),
                })
            }
            // A modal's files ride on the command, never on what is kept
            // for its answer: their bytes go once it is done, or refused.
            Command::Interact { request, .. } | Command::SuggestChoices { request, .. } => self
                .sent
                .iter()
                .find(|sent| sent.request == *request)
                .map(|sent| Work::Post {
                    body: sent.body.clone(),
                    channel: sent.channel,
                    files: match command {
                        Command::Interact { files, .. } => files.clone(),
                        _ => Vec::new(),
                    },
                }),
            _ => None,
        }
    }

    /// The body of `interaction`, without its session and nonce.
    fn body(
        &self,
        snapshot: &Snapshot,
        messages: &crate::messages::Messages,
        channel: ChannelId,
        interaction: &Interaction,
        files: &[PreparedUpload],
    ) -> Result<Value, LoginError> {
        let server = guild(snapshot, channel);
        // Only a modal carries files.
        if !files.is_empty() && !matches!(interaction, Interaction::Modal { .. }) {
            return Err(wire::invalid());
        }
        match interaction {
            Interaction::Command(used) => self.command_body(snapshot, channel, used, false),
            Interaction::Part {
                message,
                custom_id,
                values,
            } => {
                let found = messages.get(channel, *message).ok_or_else(|| {
                    LoginError::new(es("Carga el mensaje otra vez antes de usarlo."))
                })?;
                let app = found
                    .components
                    .app
                    .filter(|_| found.forwarded.is_none())
                    .ok_or_else(|| {
                        LoginError::new(es("Este mensaje no pertenece a una aplicación."))
                    })?;
                let kind = match rune_model::components::control(&found.components.parts, custom_id)
                {
                    Some(Component::Button(button))
                        if !button.disabled && button.url.is_none() && values.is_empty() =>
                    {
                        2
                    }
                    Some(Component::Select(menu)) if !menu.disabled && menu.accepts(values) => {
                        match menu.kind {
                            MenuKind::Text => 3,
                            MenuKind::User => 5,
                            MenuKind::Role => 6,
                            MenuKind::Mentionable => 7,
                            MenuKind::Channel => 8,
                        }
                    }
                    _ => return Err(LoginError::new(es("Ese control ya no se puede usar."))),
                };
                let mut data = json!({"component_type": kind, "custom_id": custom_id});
                if kind != 2 {
                    data["type"] = json!(kind);
                    data["values"] = json!(values);
                }
                let mut body = json!({"type": 3, "channel_id": channel.to_string(),
                    "message_flags": found.components.flags, "message_id": message.to_string(),
                    "application_id": app.to_string(), "data": data});
                if let Some(server) = server {
                    body["guild_id"] = json!(server.to_string());
                }
                Ok(body)
            }
            Interaction::Modal {
                id,
                app,
                custom_id,
                answers,
            } => {
                let Some((open, opened_by, parts)) = &self.modal else {
                    return Err(LoginError::new(es("El formulario ya no está abierto.")));
                };
                if open != id || *opened_by != app.0 {
                    return Err(LoginError::new(es("El formulario ya no está abierto.")));
                }
                let answers: HashMap<&str, &FieldValue> = answers
                    .iter()
                    .map(|(custom_id, value)| (custom_id.as_str(), value))
                    .collect();
                let mut used = Vec::new();
                let components = parts
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|part| submitted(part, &answers, files, &mut used, 0))
                    .collect::<Result<Vec<_>, _>>()?;
                // Each file goes once, named by one field: its index in
                // `data.attachments`, which the request adds once staged.
                used.sort_unstable();
                if used.len() != files.len() || used.iter().enumerate().any(|(i, at)| i != *at) {
                    return Err(LoginError::new(es(
                        "Los archivos no coinciden con el formulario: no se envió nada.",
                    )));
                }
                let mut body = json!({"type": 5, "application_id": app.to_string(),
                    "channel_id": channel.to_string(),
                    "data": {"id": id.to_string(), "custom_id": custom_id, "components": components}});
                if let Some(server) = server {
                    body["guild_id"] = json!(server.to_string());
                }
                Ok(body)
            }
        }
    }

    /// The body of a command run (type 2), or of the suggestions asked for
    /// its focused option (type 4).
    fn command_body(
        &self,
        snapshot: &Snapshot,
        channel: ChannelId,
        used: &CommandUse,
        suggest: bool,
    ) -> Result<Value, LoginError> {
        let definition = self
            .index
            .lock()
            .map_err(|_| wire::invalid())?
            .get(&used.command)
            .cloned()
            .filter(|definition| {
                wire::id(&definition["version"]).ok() == Some(used.version)
                    && wire::id(&definition["application_id"]).ok() == Some(used.app.0)
            })
            .ok_or_else(|| LoginError::new(es("Vuelve a cargar los comandos: este cambió.")))?;
        let kinds = option_kinds(&definition, &used.path)
            .ok_or_else(|| LoginError::new(es("Ese subcomando ya no existe.")))?;
        let mut leaf: Vec<Value> = Vec::new();
        for (name, value) in &used.values {
            if suggest
                && used
                    .focused
                    .as_ref()
                    .is_some_and(|(focused, _)| focused == name)
            {
                continue;
            }
            let kind = kinds
                .get(name.as_str())
                .copied()
                .ok_or_else(|| LoginError::new(es("El comando no tiene esa opción.")))?;
            leaf.push(json!({"type": kind, "name": name, "value": option_value(value)}));
        }
        if suggest {
            let (name, text) = used.focused.as_ref().ok_or_else(wire::invalid)?;
            let kind = kinds
                .get(name.as_str())
                .copied()
                .ok_or_else(|| LoginError::new(es("El comando no tiene esa opción.")))?;
            // The text as written, even for a number.
            leaf.push(json!({"type": kind, "name": name, "value": text, "focused": true}));
        }
        let mut options = leaf;
        for (depth, name) in used.path.iter().enumerate().rev() {
            let kind = if depth + 1 == used.path.len() { 1 } else { 2 };
            options = vec![json!({"type": kind, "name": name, "options": options})];
        }
        let mut data = json!({"version": used.version.to_string(), "id": used.command.to_string(),
            "name": used.name, "type": 1, "application_command": definition, "attachments": []});
        match used.target {
            // Discord's client sends `options` for every command, empty on a person.
            Some(user) => {
                data["type"] = json!(2);
                data["target_id"] = json!(user.0.to_string());
                data["options"] = json!([]);
            }
            None => data["options"] = json!(options),
        }
        if let Some(server) = used.server {
            data["guild_id"] = json!(server.to_string());
        }
        let mut body = json!({"type": if suggest { 4 } else { 2 },
            "application_id": used.app.to_string(), "channel_id": channel.to_string(), "data": data});
        if !suggest && used.target.is_none() {
            body["analytics_location"] = json!("slash_ui");
        }
        if let Some(server) = guild(snapshot, channel) {
            body["guild_id"] = json!(server.to_string());
        }
        Ok(body)
    }

    /// What the Gateway says of apps, or `None` for other dispatches.
    pub fn dispatch(&mut self, kind: &str, data: &Value) -> Option<Vec<Event>> {
        if !matches!(
            kind,
            "INTERACTION_CREATE"
                | "INTERACTION_SUCCESS"
                | "INTERACTION_FAILURE"
                | "INTERACTION_MODAL_CREATE"
                | "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE"
                | "GUILD_APPLICATION_COMMAND_INDEX_UPDATE"
        ) {
            return None;
        }
        if kind == "GUILD_APPLICATION_COMMAND_INDEX_UPDATE" {
            let server = ServerId(wire::id(&data["guild_id"]).ok()?);
            return Some(vec![Event::AppCommandsChanged {
                server: Some(server),
            }]);
        }
        // Only an answer to an interaction of this session counts.
        let said = match &data["nonce"] {
            Value::String(nonce) => nonce.clone(),
            Value::Number(nonce) => nonce.to_string(),
            _ => return Some(Vec::new()),
        };
        let Some(index) = self.sent.iter().position(|sent| sent.nonce == said) else {
            return Some(Vec::new());
        };
        let (request, channel, suggest) = {
            let sent = &self.sent[index];
            (sent.request, sent.channel, sent.suggest)
        };
        let events = match kind {
            "INTERACTION_CREATE" if !suggest => vec![Event::Interaction {
                request,
                done: false,
            }],
            "INTERACTION_SUCCESS" if !suggest => {
                self.sent.remove(index);
                vec![Event::Interaction {
                    request,
                    done: true,
                }]
            }
            "INTERACTION_FAILURE" if !suggest => {
                self.sent.remove(index);
                let reason = match data["reason_code"].as_u64() {
                    Some(2) => t("La aplicación no respondió a tiempo."),
                    _ => t("Esta interacción falló."),
                };
                vec![Event::Failed {
                    failure: Failure::Interaction { request, channel },
                    reason: reason.into(),
                    ambiguous: false,
                }]
            }
            "INTERACTION_MODAL_CREATE" if !suggest => {
                let opened = data["channel_id"]
                    .as_str()
                    .is_none_or(|said| said == channel.to_string());
                let Some((modal, parts)) = modal(data).filter(|_| opened) else {
                    return Some(Vec::new());
                };
                self.sent.remove(index);
                self.modal = Some((modal.id, modal.app.0, parts));
                vec![
                    Event::ModalOpened {
                        request,
                        channel,
                        modal,
                    },
                    Event::Interaction {
                        request,
                        done: true,
                    },
                ]
            }
            "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE" if suggest => {
                self.sent.remove(index);
                vec![Event::CommandChoices {
                    request,
                    choices: data["choices"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(rune_model::interactions::MAX_CHOICES)
                        .filter_map(choice)
                        .collect(),
                }]
            }
            _ => Vec::new(),
        };
        Some(events)
    }
}

/// The server of `channel`, when it is in one.
fn guild(snapshot: &Snapshot, channel: ChannelId) -> Option<ServerId> {
    snapshot
        .channels
        .iter()
        .find(|known| known.id == channel)
        .and_then(|known| known.server)
}

/// The service's number for each option of the subcommand at `path` of a
/// command as written, by name.
fn option_kinds<'a>(definition: &'a Value, path: &[String]) -> Option<HashMap<&'a str, u64>> {
    let mut options = definition["options"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for name in path {
        let found = options.iter().find(|option| {
            option["name"].as_str() == Some(name) && matches!(option["type"].as_u64(), Some(1 | 2))
        })?;
        options = found["options"].as_array().map_or(&[][..], Vec::as_slice);
    }
    Some(
        options
            .iter()
            .filter_map(|option| Some((option["name"].as_str()?, option["type"].as_u64()?)))
            .filter(|(_, kind)| *kind > 2)
            .collect(),
    )
}

/// How a value goes on the wire: ids as text, numbers as numbers.
fn option_value(value: &OptionValue) -> Value {
    match value {
        OptionValue::Text(text) => json!(text),
        OptionValue::Integer(number) => json!(number),
        OptionValue::Number(number) => json!(number),
        OptionValue::Boolean(on) => json!(on),
        OptionValue::User(id) => json!(id.0.to_string()),
        OptionValue::Channel(id) => json!(id.0.to_string()),
        OptionValue::Role(id) => json!(id.0.to_string()),
    }
}

/// A suggested choice, by its type on the wire.
fn choice(value: &Value) -> Option<OptionChoice> {
    let name = value["name"]
        .as_str()
        .filter(|name| !name.is_empty() && name.chars().count() <= 100)?;
    let value = match &value["value"] {
        Value::String(text) if text.chars().count() <= 100 => OptionValue::Text(text.clone()),
        Value::Number(number) => match number.as_i64() {
            Some(whole) => OptionValue::Integer(whole),
            None => OptionValue::Number(number.as_f64().filter(|n| n.is_finite())?),
        },
        _ => return None,
    };
    Some(OptionChoice {
        name: name.to_owned(),
        value,
    })
}

/// A modal's part as it is sent back: its structure as it came, each field
/// with what the account gave it.
fn submitted(
    part: &Value,
    answers: &HashMap<&str, &FieldValue>,
    files: &[PreparedUpload],
    used: &mut Vec<usize>,
    depth: usize,
) -> Result<Value, LoginError> {
    if depth > 3 {
        return Err(wire::invalid());
    }
    let kind = part["type"].as_u64().ok_or_else(wire::invalid)?;
    let custom_id = || -> Result<String, LoginError> {
        crate::components::custom_id(&part["custom_id"]).ok_or_else(wire::invalid)
    };
    let values = |id: &str| match answers.get(id) {
        Some(FieldValue::Values(values)) if !values.is_empty() => json!(values),
        _ => Value::Null,
    };
    Ok(match kind {
        1 => json!({"type": 1, "components": part["components"].as_array().into_iter().flatten()
            .map(|child| submitted(child, answers, files, used, depth + 1)).collect::<Result<Vec<_>, _>>()?}),
        18 => {
            json!({"type": 18, "component": submitted(&part["component"], answers, files, used, depth + 1)?})
        }
        10 => json!({"type": 10}),
        4 => {
            let id = custom_id()?;
            let value = match answers.get(id.as_str()) {
                Some(FieldValue::Text(text)) if !text.is_empty() => json!(text),
                _ => Value::Null,
            };
            json!({"type": 4, "custom_id": id, "value": value})
        }
        3 | 5..=8 | 22 => {
            let id = custom_id()?;
            let values = values(&id);
            json!({"type": kind, "custom_id": id, "values": values})
        }
        19 => {
            let id = custom_id()?;
            let named = match answers.get(id.as_str()) {
                Some(FieldValue::Files(named)) => named.as_slice(),
                _ => &[],
            };
            // As many as the field takes, of the kinds it lists.
            let field = control(part).ok_or_else(wire::invalid)?;
            let ModalInput::Files { max, .. } = &field else {
                return Err(wire::invalid());
            };
            if named.len() > usize::from(*max) {
                return Err(wire::invalid());
            }
            let mut indexes = Vec::with_capacity(named.len());
            for token in named {
                let at = files
                    .iter()
                    .position(|file| file.metadata().token == *token)
                    .ok_or_else(wire::invalid)?;
                if !field.accepts(&files[at].metadata().filename) {
                    return Err(LoginError::new(es(
                        "Un archivo no es de un tipo que acepte el formulario: no se envió nada.",
                    )));
                }
                used.push(at);
                indexes.push(at.to_string());
            }
            json!({"type": 19, "custom_id": id, "values": indexes})
        }
        21 => {
            let id = custom_id()?;
            let value = match answers.get(id.as_str()) {
                Some(FieldValue::Values(values)) => {
                    values.first().map_or(Value::Null, |v| json!(v))
                }
                _ => Value::Null,
            };
            json!({"type": 21, "custom_id": id, "value": value})
        }
        23 => {
            let id = custom_id()?;
            let on = matches!(answers.get(id.as_str()), Some(FieldValue::Check(true)));
            json!({"type": 23, "custom_id": id, "value": on})
        }
        _ => return Err(wire::invalid()),
    })
}

/// A modal the Gateway opened, and its parts as written; `None` when it is
/// malformed or out of bounds.
fn modal(data: &Value) -> Option<(Modal, Value)> {
    let id = wire::id(&data["id"]).ok()?;
    let app = wire::id(&data["application"]["id"]).ok()?;
    let custom_id = crate::components::custom_id(&data["custom_id"])?;
    let title = data["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty() && title.chars().count() <= 45)?
        .to_owned();
    let parts = data["components"]
        .as_array()
        .filter(|parts| parts.len() <= 10)?;
    let mut fields = Vec::new();
    for part in parts {
        fields.push(field(part)?);
    }
    Some((
        Modal {
            id,
            app: UserId(app),
            custom_id,
            title,
            fields,
        },
        Value::Array(parts.clone()),
    ))
}

/// One field of a modal: a row or a label around its control, or text.
fn field(part: &Value) -> Option<ModalField> {
    let text = |value: &Value, limit: usize| {
        value
            .as_str()
            .filter(|said| !said.trim().is_empty())
            .map(|said| said.chars().take(limit).collect::<String>())
    };
    match part["type"].as_u64()? {
        // Legacy: a row with one text input that names itself.
        1 => {
            let input = part["components"].as_array()?.first()?;
            Some(ModalField {
                label: text(&input["label"], 45),
                description: None,
                input: control(input)?,
            })
        }
        18 => Some(ModalField {
            label: text(&part["label"], 45),
            description: text(&part["description"], 100),
            input: control(&part["component"])?,
        }),
        10 => Some(ModalField {
            label: None,
            description: None,
            input: ModalInput::Text(text(&part["content"], 4000)?),
        }),
        other => Some(ModalField {
            label: None,
            description: None,
            input: ModalInput::Unsupported(u16::try_from(other).ok()?),
        }),
    }
}

/// What a field of a modal takes.
fn control(value: &Value) -> Option<ModalInput> {
    let kind = value["type"].as_u64()?;
    let custom_id = crate::components::custom_id(&value["custom_id"]);
    let length = |field: &str| value[field].as_u64().map(|n| n.min(4000) as u16);
    let options = || -> Vec<rune_model::components::MenuOption> {
        value["options"]
            .as_array()
            .into_iter()
            .flatten()
            .take(25)
            .filter_map(|option| {
                Some(rune_model::components::MenuOption {
                    label: option["label"].as_str()?.chars().take(100).collect(),
                    value: option["value"]
                        .as_str()
                        .filter(|value| !value.is_empty() && value.len() <= 100)?
                        .to_owned(),
                    description: option["description"]
                        .as_str()
                        .map(|said| said.chars().take(100).collect()),
                    default: option["default"].as_bool() == Some(true),
                    ..Default::default()
                })
            })
            .collect()
    };
    Some(match kind {
        4 => ModalInput::TextInput {
            custom_id: custom_id?,
            long: value["style"].as_u64() == Some(2),
            required: value["required"].as_bool().unwrap_or(true),
            min_length: length("min_length"),
            max_length: length("max_length"),
            value: value["value"]
                .as_str()
                .map(|said| said.chars().take(4000).collect()),
            placeholder: value["placeholder"]
                .as_str()
                .map(|said| said.chars().take(100).collect()),
        },
        3 | 5..=8 => {
            let parts = crate::components::parts(&Value::Array(vec![value.clone()]));
            match parts.into_iter().next()? {
                Component::Select(menu) => ModalInput::Menu(menu),
                _ => return None,
            }
        }
        21 => ModalInput::Radio {
            custom_id: custom_id?,
            required: value["required"].as_bool().unwrap_or(true),
            options: options(),
        },
        22 => ModalInput::Checks {
            custom_id: custom_id?,
            min: value["min_values"].as_u64().map_or(0, |n| n.min(25) as u8),
            max: value["max_values"]
                .as_u64()
                .map_or(25, |n| n.clamp(1, 25) as u8),
            options: options(),
        },
        23 => ModalInput::Check {
            custom_id: custom_id?,
            value: value["default"].as_bool().unwrap_or(false),
        },
        // As Serein reads it: 1 to 10 files, one unless said, of up to 10
        // kinds (`.ext` or a family); a malformed kind drops the modal.
        19 => {
            let max = value["max_values"]
                .as_u64()
                .map_or(1, |n| n.clamp(1, 10) as u8);
            let kinds = value["file_types"]
                .as_array()
                .map_or(&[][..], Vec::as_slice);
            if kinds.len() > 10 {
                return None;
            }
            let file_types = kinds
                .iter()
                .map(|kind| {
                    kind.as_str()
                        .filter(|kind| {
                            (1..=32).contains(&kind.len())
                                && kind.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
                        })
                        .map(str::to_owned)
                })
                .collect::<Option<Vec<_>>>()?;
            ModalInput::Files {
                custom_id: custom_id?,
                required: value["required"].as_bool().unwrap_or(true),
                min: value["min_values"]
                    .as_u64()
                    .map_or(1, |n| n.min(u64::from(max)) as u8),
                max,
                file_types,
            }
        }
        other => ModalInput::Unsupported(u16::try_from(other).ok()?),
    })
}

/// The commands of an index the account may know of in `server`, or
/// outside servers, and their definitions as written: typed in the composer
/// (type 1), on a person (2) or on a message (3), bounded; a malformed one is
/// left out, a malformed index fails.
pub(crate) fn index(
    value: &Value,
    server: Option<ServerId>,
    roles: &[RoleId],
) -> Result<(Vec<AppCommand>, HashMap<u64, Value>), LoginError> {
    let commands = value["application_commands"]
        .as_array()
        .filter(|commands| commands.len() <= MAX_COMMANDS)
        .ok_or_else(wire::invalid)?;
    let apps: HashMap<u64, &Value> = value["applications"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|app| Some((wire::id(&app["id"]).ok()?, app)))
        .collect();
    let mut found = Vec::new();
    let mut raw = HashMap::new();
    for definition in commands {
        let kind = match definition["type"].as_u64().unwrap_or(1) {
            1 => CommandKind::ChatInput,
            2 => CommandKind::User,
            3 => CommandKind::Message,
            _ => continue,
        };
        let Some(command) = command(definition, &apps, server, roles, kind) else {
            continue;
        };
        let mut kept = definition.clone();
        if let Some(object) = kept.as_object_mut() {
            // What the account may do is not part of the command.
            for field in [
                "permissions",
                "default_member_permissions",
                "application_permissions",
            ] {
                object.remove(field);
            }
            normalized(object);
        }
        raw.insert(command.id, kept);
        found.push(command);
    }
    Ok((found, raw))
}

/// A name as the service defines it: the default before the translation.
fn normalized(object: &mut Map<String, Value>) {
    for (field, default) in [
        ("name", "name_default"),
        ("description", "description_default"),
    ] {
        if let Some(said) = object.get(default).filter(|said| said.is_string()).cloned() {
            object.insert(field.into(), said);
        }
    }
    for list in ["options", "choices"] {
        if let Some(Value::Array(items)) = object.get_mut(list) {
            for item in items.iter_mut().filter_map(Value::as_object_mut) {
                normalized(item);
            }
        }
    }
}

fn named(value: &Value, limit: usize) -> Option<String> {
    value["name_default"]
        .as_str()
        .or_else(|| value["name"].as_str())
        .filter(|name| !name.is_empty() && name.chars().count() <= limit)
        .map(str::to_owned)
}

fn described(value: &Value) -> String {
    value["description_default"]
        .as_str()
        .or_else(|| value["description"].as_str())
        .map(|said| said.chars().take(100).collect())
        .unwrap_or_default()
}

/// One command, with what overrides say of it for the account.
fn command(
    value: &Value,
    apps: &HashMap<u64, &Value>,
    server: Option<ServerId>,
    roles: &[RoleId],
    kind: CommandKind,
) -> Option<AppCommand> {
    let app = wire::id(&value["application_id"]).ok()?;
    let only = match &value["guild_id"] {
        Value::Null => None,
        id => Some(ServerId(wire::id(id).ok()?)),
    };
    if only.is_some() && only != server {
        return None;
    }
    let contexts: Option<Vec<u64>> = value["contexts"]
        .as_array()
        .map(|contexts| contexts.iter().filter_map(Value::as_u64).collect());
    let in_servers = contexts
        .as_ref()
        .is_none_or(|contexts| contexts.contains(&0));
    let in_direct = contexts
        .as_ref()
        .is_none_or(|contexts| contexts.contains(&1));
    let needs = match &value["default_member_permissions"] {
        Value::Null => None,
        bits => {
            let read = crate::bootstrap::permission_bits(bits).ok()?;
            // "0": only administrators.
            Some(if read.is_empty() {
                Permissions::ADMINISTRATOR
            } else {
                read
            })
        }
    };
    let application = apps.get(&app);
    let target = |permissions: &Value| -> Option<bool> {
        let server = server?;
        permissions["user"].as_bool().or_else(|| {
            let roles_said = permissions["roles"].as_object()?;
            roles
                .iter()
                .filter_map(|role| roles_said.get(&role.0.to_string())?.as_bool())
                .reduce(|allowed, next| allowed || next)
                .or_else(|| roles_said.get(&server.0.to_string())?.as_bool())
        })
    };
    let command_target = target(&value["permissions"]);
    let app_target = application.and_then(|app| target(&app["permissions"]));
    let channels = |permissions: &Value| -> Vec<(ChannelId, bool)> {
        permissions["channels"]
            .as_object()
            .into_iter()
            .flatten()
            .take(100)
            .filter_map(|(id, allowed)| Some((ChannelId(id.parse().ok()?), allowed.as_bool()?)))
            .collect()
    };
    let mut overrides = channels(&value["permissions"]);
    for (id, allowed) in application
        .map(|app| channels(&app["permissions"]))
        .unwrap_or_default()
    {
        if !overrides.iter().any(|(known, _)| *known == id) {
            overrides.push((id, allowed));
        }
    }
    // The server's id less one stands for every channel.
    let all = server
        .and_then(|server| server.0.checked_sub(1))
        .map(ChannelId);
    let all_channels = all.and_then(|all| {
        overrides
            .iter()
            .find(|(id, _)| *id == all)
            .map(|(_, allowed)| *allowed)
    });
    overrides.retain(|(id, _)| Some(*id) != all);
    Some(AppCommand {
        id: wire::id(&value["id"]).ok()?,
        version: wire::id(&value["version"]).ok()?,
        app: UserId(app),
        app_name: application
            .and_then(|app| app["name"].as_str())
            .map_or_else(
                || t("Aplicación").into(),
                |name| name.chars().take(100).collect(),
            ),
        app_icon: None,
        name: named(value, 32)?,
        description: described(value),
        kind,
        options: options(&value["options"], 0)?,
        server: only,
        in_servers,
        in_direct,
        needs,
        access: CommandAccess {
            granted: command_target.or(app_target.filter(|allowed| !allowed)),
            channels: overrides,
            all_channels,
        },
        nsfw: value["nsfw"].as_bool().unwrap_or(false),
    })
}

/// A command's options, at most three levels deep and 25 a level.
fn options(value: &Value, depth: usize) -> Option<Vec<CommandOption>> {
    let Some(list) = value.as_array() else {
        return Some(Vec::new());
    };
    if depth > 2 || list.len() > rune_model::interactions::MAX_OPTIONS {
        return None;
    }
    list.iter()
        .map(|option| {
            let kind = match option["type"].as_u64()? {
                1 => OptionKind::Subcommand,
                2 => OptionKind::Group,
                3 => OptionKind::Text,
                4 => OptionKind::Integer,
                5 => OptionKind::Boolean,
                6 => OptionKind::User,
                7 => OptionKind::Channel,
                8 => OptionKind::Role,
                9 => OptionKind::Mentionable,
                10 => OptionKind::Number,
                11 => OptionKind::Attachment,
                _ => return None,
            };
            let mut read = CommandOption::new(kind, &named(option, 32)?, &described(option));
            read.required = option["required"].as_bool().unwrap_or(false);
            read.autocomplete = option["autocomplete"].as_bool().unwrap_or(false);
            read.options = options(&option["options"], depth + 1)?;
            read.choices = option["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .take(rune_model::interactions::MAX_CHOICES)
                .filter_map(|said| {
                    let mut said = said.clone();
                    if let Some(name) = said.get("name_default").cloned() {
                        said["name"] = name;
                    }
                    choice(&said)
                })
                .collect();
            read.channels = option["channel_types"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|kind| crate::bootstrap::kind_of(kind.as_u64()?))
                .collect();
            read.min = option["min_value"].as_f64().filter(|n| n.is_finite());
            read.max = option["max_value"].as_f64().filter(|n| n.is_finite());
            read.min_length = option["min_length"].as_u64().map(|n| n.min(6000) as u16);
            read.max_length = option["max_length"].as_u64().map(|n| n.min(6000) as u16);
            Some(read)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modal_goes_back_with_its_own_structure_and_the_answers() {
        let data = json!({"id": "70", "nonce": "1", "channel_id": "5", "custom_id": "f",
            "title": "Inscripción", "application": {"id": "9"}, "components": [
            {"type": 1, "components": [{"type": 4, "custom_id": "a", "label": "Nombre", "style": 1}]},
            {"type": 18, "label": "Nivel", "component": {"type": 21, "custom_id": "n",
                "options": [{"label": "Uno", "value": "1"}]}},
            {"type": 18, "label": "Avisos", "component": {"type": 23, "custom_id": "c"}},
            {"type": 10, "content": "Gracias"}]});
        let (read, parts) = modal(&data).unwrap();
        assert_eq!(read.fields.len(), 4);
        assert_eq!(read.fields[0].label.as_deref(), Some("Nombre"));
        assert!(matches!(read.fields[1].input, ModalInput::Radio { .. }));
        assert!(matches!(read.fields[3].input, ModalInput::Text(_)));
        let text = FieldValue::Text("Ada".into());
        let level = FieldValue::Values(vec!["1".into()]);
        let check = FieldValue::Check(true);
        let answers = HashMap::from([("a", &text), ("n", &level), ("c", &check)]);
        let sent: Vec<Value> = parts
            .as_array()
            .unwrap()
            .iter()
            .map(|part| submitted(part, &answers, &[], &mut Vec::new(), 0).unwrap())
            .collect();
        assert_eq!(
            Value::Array(sent),
            json!([
                {"type": 1, "components": [{"type": 4, "custom_id": "a", "value": "Ada"}]},
                {"type": 18, "component": {"type": 21, "custom_id": "n", "value": "1"}},
                {"type": 18, "component": {"type": 23, "custom_id": "c", "value": true}},
                {"type": 10}
            ])
        );
        assert!(
            modal(
                &json!({"id": "1", "application": {"id": "9"}, "custom_id": "f",
            "title": "x".repeat(46), "components": []})
            )
            .is_none(),
            "a title longer than the service allows"
        );
    }

    #[test]
    fn an_index_keeps_the_commands_the_account_may_know_with_their_overrides() {
        let index_value = json!({"applications": [{"id": "9", "name": "Bot",
            "permissions": {"roles": {"20": false}}}],
            "application_commands": [
                {"id": "1", "version": "2", "application_id": "9", "type": 1, "name": "dado",
                 "name_default": "dado", "description": "Tira", "default_member_permissions": "0",
                 "permissions": {"channels": {"5": false, "19": true}},
                 "options": [{"type": 4, "name": "caras", "description": "n", "min_value": 2,
                    "choices": [{"name": "Seis", "value": 6}]}]},
                {"id": "3", "version": "1", "application_id": "9", "type": 2, "name": "Perfil"},
                {"id": "4", "version": "1", "application_id": "9", "type": 1, "name": "otro",
                 "description": "", "guild_id": "99"},
                {"id": "6", "version": "1", "application_id": "9", "type": 1, "name": "",
                 "description": ""}
            ], "version": "7"});
        let (commands, raw) = index(&index_value, Some(ServerId(20)), &[RoleId(20)]).unwrap();
        assert_eq!(
            commands.len(),
            2,
            "a person's command goes with the chat one; another server's and a nameless one do not"
        );
        assert_eq!(commands[1].kind, CommandKind::User);
        let dice = &commands[0];
        assert_eq!(dice.needs, Some(Permissions::ADMINISTRATOR));
        assert_eq!(dice.access.granted, Some(false), "the app's role override");
        assert_eq!(dice.access.channels, [(ChannelId(5), false)]);
        assert_eq!(
            dice.access.all_channels,
            Some(true),
            "the server's id less one"
        );
        assert_eq!(dice.options[0].choices[0].value, OptionValue::Integer(6));
        assert!(raw[&1].get("permissions").is_none());
        assert!(raw[&1].get("default_member_permissions").is_none());
    }
}
