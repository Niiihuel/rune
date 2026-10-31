//! A simulated app, RuneBot, for commands and interactions (P3): the
//! commands it offers, with groups, choices, bounds and suggestions; what
//! its buttons and menus answer; a modal; replies only the account sees;
//! and a refusal to show how one looks. Everything it says is labeled as
//! simulated, and the same request gets the same answer.

use std::time::Duration;

use rune_i18n::t;
use rune_model::components::{Button, MenuKind, MenuOption, Picked, Select};
use rune_model::interactions::{
    AppCommand, CommandAccess, CommandKind, CommandOption, CommandUse, FieldValue, Interaction,
    Modal, ModalField, ModalInput, OptionChoice, OptionKind, OptionValue,
};
use rune_model::{
    ButtonStyle, ChannelId, ChannelKind, Component, Components, Message, Permissions, Timestamp,
    UserId,
};

use super::{mix, Session, World};
use crate::{Command, Event, Failure, PreparedUpload, RequestId};

/// RuneBot's account, which answers as the app.
pub(super) const APP: UserId = UserId(9);
const APP_NAME: &str = "RuneBot";
/// Mixed into the app's answers, apart from every other stream of the world.
const SALT: u64 = 0x4150_5021;
/// Flags of a message only the account sees, and of one made only of parts.
const EPHEMERAL: u64 = 1 << 6;
const WHOLE: u64 = 1 << 15;
/// What the app suggests for `/buscar`.
const TOPICS: [&str; 8] = [
    "rust",
    "limn",
    "recetas",
    "redes",
    "río de la plata",
    "radio",
    "música",
    "mate",
];

/// What the simulated app remembers: the modal it opened last.
#[derive(Default)]
pub(super) struct AppSim {
    modal: Option<u64>,
}

impl Session {
    /// Carry out a command of P3, as the app would answer it.
    pub(super) fn interact(&mut self, command: Command) {
        let latency = self.config.latency;
        let now = (self.config.clock)();
        match command {
            Command::LoadAppCommands { request, channel } => {
                self.later(latency, move |world| world.app_commands(request, channel))
            }
            Command::SuggestChoices {
                request, command, ..
            } => self.later(latency / 2, move |_| suggest(request, &command)),
            Command::Interact {
                request,
                channel,
                interaction,
                files,
            } => {
                let checked = self
                    .world
                    .lock()
                    .unwrap()
                    .check(channel, &interaction)
                    .and_then(|()| carried(&interaction, &files));
                if let Err(reason) = checked {
                    self.later(latency, move |_| vec![failed(request, channel, reason)]);
                    return;
                }
                // Got at once, answered after a moment of «thinking».
                self.later(latency, move |_| {
                    vec![Event::Interaction {
                        request,
                        done: false,
                    }]
                });
                self.later(latency + Duration::from_millis(900), move |world| {
                    world.answer(request, channel, interaction, &files, now)
                });
            }
            _ => {}
        }
    }
}

/// Whether `files` are the ones the modal's answers name, each once, in
/// file fields of RuneBot's form, as many and of the kinds each takes: what
/// the unofficial session checks before anything leaves.
fn carried(interaction: &Interaction, files: &[PreparedUpload]) -> Result<(), String> {
    let wrong = || t("Los archivos no coinciden con el formulario (simulado).").to_owned();
    let mut named = interaction.files();
    let mut given: Vec<_> = files.iter().map(|file| file.metadata().token).collect();
    named.sort_by_key(|token| token.0);
    given.sort_by_key(|token| token.0);
    if named != given || named.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(wrong());
    }
    let Interaction::Modal { id, answers, .. } = interaction else {
        return Ok(());
    };
    let form = modal(*id);
    for (custom_id, value) in answers {
        let FieldValue::Files(tokens) = value else {
            continue;
        };
        let field = form
            .fields
            .iter()
            .map(|field| &field.input)
            .find(|input| input.custom_id() == Some(custom_id.as_str()))
            .ok_or_else(wrong)?;
        let ModalInput::Files { max, .. } = field else {
            return Err(wrong());
        };
        let fits = tokens.iter().all(|token| {
            files
                .iter()
                .find(|file| file.metadata().token == *token)
                .is_some_and(|file| field.accepts(&file.metadata().filename))
        });
        if tokens.len() > usize::from(*max) || !fits {
            return Err(wrong());
        }
    }
    Ok(())
}

fn failed(request: RequestId, channel: ChannelId, reason: String) -> Event {
    Event::Failed {
        failure: Failure::Interaction { request, channel },
        reason,
        ambiguous: false,
    }
}

/// The topics of `/buscar` that hold what is being written.
fn suggest(request: RequestId, command: &CommandUse) -> Vec<Event> {
    let written = command
        .focused
        .as_ref()
        .map(|(_, text)| text.to_lowercase())
        .unwrap_or_default();
    let choices = TOPICS
        .iter()
        .filter(|topic| topic.contains(written.trim()))
        .map(|topic| OptionChoice {
            name: format!("{topic} {}", t("(simulado)")),
            value: OptionValue::Text((*topic).into()),
        })
        .collect();
    vec![Event::CommandChoices { request, choices }]
}

impl World {
    /// The list of commands, answered at once for tests that drive the
    /// interface without a runtime ([`super::MockProvider::manage`]).
    pub(super) fn app_command(&mut self, command: &Command) -> Option<Result<Vec<Event>, String>> {
        match command {
            Command::LoadAppCommands { request, channel } => {
                Some(Ok(self.app_commands(*request, *channel)))
            }
            _ => None,
        }
    }

    /// The commands `channel` offers: RuneBot's, in a server's channels and
    /// in a conversation with it; none in the others.
    fn app_commands(&self, request: RequestId, channel: ChannelId) -> Vec<Event> {
        let offered = self
            .channel(channel)
            .is_some_and(|found| found.server.is_some() || found.recipients.contains(&APP));
        vec![Event::AppCommands {
            request,
            channel,
            commands: if offered { commands() } else { Vec::new() },
        }]
    }

    /// Whether the app takes `interaction` in `channel`.
    fn check(&self, channel: ChannelId, interaction: &Interaction) -> Result<(), String> {
        let found = self
            .channel(channel)
            .ok_or(t("La conversación no existe (simulado)."))?;
        match interaction {
            Interaction::Part {
                message,
                custom_id,
                values,
            } => {
                let found = self
                    .message(channel, *message)
                    .filter(|found| found.components.app == Some(APP))
                    .ok_or(t("El mensaje ya no está (simulado)."))?;
                match rune_model::components::control(&found.components.parts, custom_id) {
                    Some(Component::Button(button)) if !button.disabled && values.is_empty() => {
                        Ok(())
                    }
                    Some(Component::Select(menu)) if !menu.disabled && menu.accepts(values) => {
                        Ok(())
                    }
                    _ => Err(t("RuneBot no reconoce ese control (simulado).").into()),
                }
            }
            Interaction::Modal { id, app, .. } => {
                if *app != APP || self.app.modal != Some(*id) {
                    return Err(t("Ese formulario ya no está abierto (simulado).").into());
                }
                // Files are staged in the conversation, as a message's are.
                if !interaction.files().is_empty()
                    && !found.permissions.contains(Permissions::ATTACH)
                {
                    return Err(t("No tienes permiso para adjuntar archivos (simulado).").into());
                }
                Ok(())
            }
            Interaction::Command(used) => {
                if used.app != APP || !commands().iter().any(|known| known.id == used.command) {
                    return Err(t("Ese comando ya no existe (simulado).").into());
                }
                Ok(())
            }
        }
    }

    /// What the app answers `interaction`: its messages, a modal, or a
    /// refusal, then that it is done.
    fn answer(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        interaction: Interaction,
        files: &[PreparedUpload],
        now: Timestamp,
    ) -> Vec<Event> {
        let done = Event::Interaction {
            request,
            done: true,
        };
        let me = self.name_of(super::ME);
        // A command's answer says which command it answers, and who ran it.
        let invoked = match &interaction {
            Interaction::Command(used) => Some(rune_model::components::Invoked {
                user: super::ME,
                command: std::iter::once(used.name.as_str())
                    .chain(used.path.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" "),
            }),
            _ => None,
        };
        let said = match interaction {
            Interaction::Part {
                custom_id, values, ..
            } => match custom_id.as_str() {
                "rb:no-puedo" => {
                    return vec![failed(
                        request,
                        channel,
                        t("RuneBot no pudo completar la interacción (simulado).").into(),
                    )]
                }
                "rb:formulario" => return self.open_modal(request, channel),
                "rb:confirmar" => Said::Only(format!(
                    "¡Anotado! Te esperamos el jueves {}.",
                    t("(simulado)")
                )),
                _ => Said::Only(format!(
                    "Elegiste: {} {}.",
                    if values.is_empty() {
                        "nada".to_owned()
                    } else {
                        values
                            .iter()
                            .map(|value| self.value_name(value))
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                    t("(simulado)")
                )),
            },
            Interaction::Modal { answers, .. } => {
                self.app.modal = None;
                let lines: Vec<String> = answers
                    .iter()
                    .map(|(field, value)| {
                        let shown = match value {
                            FieldValue::Text(text) => text.clone(),
                            FieldValue::Values(values) => values.join(", "),
                            FieldValue::Check(on) => if *on { "sí" } else { "no" }.into(),
                            FieldValue::Files(tokens) => files
                                .iter()
                                .filter(|file| tokens.contains(&file.metadata().token))
                                .map(|file| file.metadata().filename.clone())
                                .collect::<Vec<_>>()
                                .join(", "),
                        };
                        format!("- **{field}**: {shown}")
                    })
                    .collect();
                Said::All(format!(
                    "Gracias, {me}. Recibí el formulario {}:\n{}",
                    t("(simulado)"),
                    lines.join("\n")
                ))
            }
            Interaction::Command(used) => match self.run(request, channel, &used, &me) {
                Ok(said) => said,
                Err(events) => return events,
            },
        };
        let message = match said {
            Said::All(text) => self.post_as_app(channel, text, now, false),
            Said::Only(text) => self.post_as_app(channel, text, now, true),
        };
        // The files of a modal come back with the app's answer.
        let attached = self.attachments(files);
        message
            .map(|mut message| {
                message.components.invoked = invoked;
                message.attachments = attached;
                if !message.components.ephemeral {
                    self.store(channel, message.clone());
                }
                message
            })
            .map(|message| Event::MessageCreated {
                message,
                nonce: None,
            })
            .into_iter()
            .chain([done])
            .collect()
    }

    /// What a command says, or the events that answer it otherwise.
    fn run(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        used: &CommandUse,
        me: &str,
    ) -> Result<Said, Vec<Event>> {
        let value = |name: &str| {
            used.values
                .iter()
                .find(|(given, _)| given == name)
                .map(|(_, value)| value)
        };
        // A user command runs on a person: it greets them, whatever its name.
        if let Some(user) = used.target {
            return Ok(Said::All(format!(
                "👋 Hola, {} {}.",
                self.name_of(user),
                t("(simulado)")
            )));
        }
        Ok(match used.name.as_str() {
            "dado" => {
                let faces = match value("caras") {
                    Some(OptionValue::Integer(faces)) => *faces,
                    _ => 6,
                };
                let roll = mix(&[self.seed, request, SALT]).range(1, faces + 1);
                let said = format!(
                    "🎲 {me} tiró un dado de {faces} caras: salió **{roll}** {}.",
                    t("(simulado)")
                );
                if value("privado") == Some(&OptionValue::Boolean(true)) {
                    Said::Only(said)
                } else {
                    Said::All(said)
                }
            }
            "recordatorio" => {
                let text = value("texto").and_then(OptionValue::text).unwrap_or("");
                let minutes = match value("en") {
                    Some(OptionValue::Integer(minutes)) => *minutes,
                    _ => 5,
                };
                Said::Only(format!(
                    "⏰ Te recuerdo «{text}» en {minutes} minutos {}.",
                    t("(simulado: no va a sonar)")
                ))
            }
            "buscar" => {
                let topic = value("tema").and_then(OptionValue::text).unwrap_or("");
                Said::All(format!(
                    "🔎 Resultados para «{topic}»: {}",
                    t("nada que mostrar, es una simulación.")
                ))
            }
            "saludar" => {
                let who = match value("a") {
                    Some(OptionValue::User(user)) => self.name_of(*user),
                    _ => "todos".into(),
                };
                Said::All(format!(
                    "👋 ¡Hola, {who}! Te saluda {me} {}.",
                    t("(simulado)")
                ))
            }
            "config" => Said::Only(format!(
                "⚙️ Ajuste «{}» guardado {}.",
                used.path.join(" "),
                t("(simulado)")
            )),
            "formulario" => return Err(self.open_modal(request, channel)),
            "secreto" => Said::Only(format!("🤫 Sólo tú ves este mensaje {}.", t("(simulado)"))),
            "moderar" => Said::Only(format!("🛡️ {}", t("Moderación simulada: no se hizo nada."))),
            _ => {
                return Err(vec![failed(
                    request,
                    channel,
                    t("RuneBot no conoce ese comando (simulado).").into(),
                )])
            }
        })
    }

    /// The app's form, opened in answer to `request`.
    fn open_modal(&mut self, request: RequestId, channel: ChannelId) -> Vec<Event> {
        let id = self.fresh_id();
        self.app.modal = Some(id);
        vec![
            Event::ModalOpened {
                request,
                channel,
                modal: modal(id),
            },
            Event::Interaction {
                request,
                done: true,
            },
        ]
    }

    /// A message of the app in `channel`: for everyone, or only the account
    /// (`ephemeral`), which the history never shows again.
    fn post_as_app(
        &mut self,
        channel: ChannelId,
        text: String,
        now: Timestamp,
        ephemeral: bool,
    ) -> Option<Message> {
        let mut message = self.post(channel, APP, text, None, now)?;
        message.components.app = Some(APP);
        message.components.ephemeral = ephemeral;
        message.components.flags = if ephemeral { EPHEMERAL } else { 0 };
        if ephemeral {
            if let Some(sim) = self.sims.get_mut(&channel) {
                sim.overlay.insert(message.id, None);
            }
        } else {
            self.store(channel, message.clone());
        }
        Some(message)
    }

    fn name_of(&self, user: UserId) -> String {
        self.users
            .iter()
            .find(|found| found.id == user)
            .map_or_else(|| "alguien".into(), |found| found.shown_name().to_owned())
    }

    /// A picked value as people read it: a person's, role's or channel's
    /// name for its id, else the value itself.
    fn value_name(&self, value: &str) -> String {
        let Ok(id) = value.parse::<u64>() else {
            return value.to_owned();
        };
        if let Some(user) = self.users.iter().find(|user| user.id.0 == id) {
            return user.shown_name().to_owned();
        }
        if let Some(channel) = self.channels.iter().find(|channel| channel.id.0 == id) {
            return format!("#{}", channel.name);
        }
        self.servers
            .iter()
            .flat_map(|server| &server.roles)
            .find(|role| role.id.0 == id)
            .map_or_else(|| value.to_owned(), |role| format!("@{}", role.name))
    }

    /// RuneBot's message with buttons and a menu, in place of a made-up
    /// one of #general.
    pub(super) fn app_message(&self, message: &mut Message) {
        message.author = APP;
        message.components = app_parts();
    }

    /// RuneBot's message of menus of people, roles, channels and mentions,
    /// each starting with something picked, and a button to its form.
    pub(super) fn menus_message(&self, channel: ChannelId, message: &mut Message) {
        let server = self.channel(channel).and_then(|found| found.server);
        let role = self
            .servers
            .iter()
            .find(|found| Some(found.id) == server)
            .and_then(|found| found.roles.iter().find(|role| role.id.0 != found.id.0))
            .map(|role| role.id);
        let menu = |custom_id: &str, kind, placeholder: &str, defaults: Vec<Picked>, max| {
            Component::Row(vec![Component::Select(Select {
                placeholder: Some(placeholder.into()),
                custom_id: Some(custom_id.into()),
                kind,
                max,
                defaults,
                channels: if kind == MenuKind::Channel {
                    vec![ChannelKind::Text]
                } else {
                    Vec::new()
                },
                ..Select::default()
            })])
        };
        message.author = APP;
        message.content = format!("Prueba los menús de RuneBot {}:", t("(simulado)"));
        message.components = Components {
            parts: vec![
                menu(
                    "rb:personas",
                    MenuKind::User,
                    "Elige personas",
                    vec![Picked::User(super::ME)],
                    3,
                ),
                menu(
                    "rb:roles",
                    MenuKind::Role,
                    "Elige un rol",
                    role.map(Picked::Role).into_iter().collect(),
                    1,
                ),
                menu(
                    "rb:canales",
                    MenuKind::Channel,
                    "Elige un canal",
                    vec![Picked::Channel(channel)],
                    1,
                ),
                menu(
                    "rb:menciones",
                    MenuKind::Mentionable,
                    "Elige a quién mencionar",
                    Vec::new(),
                    2,
                ),
                Component::Row(vec![
                    button("Abrir formulario", ButtonStyle::Primary, "rb:formulario"),
                    button("Probar un error", ButtonStyle::Danger, "rb:no-puedo"),
                ]),
            ],
            app: Some(APP),
            ..Components::default()
        };
    }
}

/// What the app says, and to whom.
enum Said {
    All(String),
    /// Only the account, for this session.
    Only(String),
}

fn button(label: &str, style: ButtonStyle, custom_id: &str) -> Component {
    Component::Button(Button {
        label: Some(label.into()),
        style,
        custom_id: Some(custom_id.into()),
        ..Button::default()
    })
}

fn option(label: &str, value: &str, emoji: Option<&str>, default: bool) -> MenuOption {
    MenuOption {
        label: label.into(),
        value: value.into(),
        emoji: emoji.map(str::to_owned),
        default,
        ..MenuOption::default()
    }
}

/// A message an app laid out: a box with a title, a section with a link, a
/// row of two buttons, and a menu with an option already picked.
fn app_parts() -> Components {
    let link = Component::Button(Button {
        label: Some("Abrir agenda".into()),
        style: ButtonStyle::Link,
        url: Some("https://example.invalid/agenda".into()),
        ..Button::default()
    });
    Components {
        parts: vec![Component::Container {
            accent: Some(0x5865F2),
            spoiler: false,
            parts: vec![
                Component::Text(format!(
                    "### Recordatorio del taller {}\nLa reunión es el **jueves a las 18 h**.",
                    t("(simulado)")
                )),
                Component::Separator {
                    divider: true,
                    large: false,
                },
                Component::Section {
                    parts: vec![Component::Text(
                        "La agenda y los materiales están en la página del taller.".into(),
                    )],
                    accessory: Box::new(link),
                },
                Component::Row(vec![
                    button("Confirmar asistencia", ButtonStyle::Primary, "rb:confirmar"),
                    button("No puedo", ButtonStyle::Secondary, "rb:no-puedo"),
                ]),
                // Left picked by its app: the menu shows it, not the placeholder.
                Component::Row(vec![Component::Select(Select {
                    placeholder: Some("Elige un horario".into()),
                    chosen: vec![rune_model::components::Choice {
                        label: format!("Jueves 18 h {}", t("(simulado)")),
                        emoji: Some("🕕".into()),
                        picture: None,
                    }],
                    custom_id: Some("rb:horario".into()),
                    options: vec![
                        option(
                            &format!("Jueves 18 h {}", t("(simulado)")),
                            "jueves",
                            Some("🕕"),
                            true,
                        ),
                        option(
                            &format!("Viernes 19 h {}", t("(simulado)")),
                            "viernes",
                            Some("🕖"),
                            false,
                        ),
                        option(
                            &format!("Sábado 11 h {}", t("(simulado)")),
                            "sabado",
                            Some("🕚"),
                            false,
                        ),
                    ],
                    ..Select::default()
                })]),
            ],
        }],
        whole: true,
        app: Some(APP),
        flags: WHOLE,
        ..Components::default()
    }
}

/// The app's form: text short and long, a menu, a choice, a box and files.
fn modal(id: u64) -> Modal {
    let field = |label: &str, description: Option<&str>, input| ModalField {
        label: Some(label.into()),
        description: description.map(str::to_owned),
        input,
    };
    Modal {
        id,
        app: APP,
        custom_id: "rb:formulario".into(),
        title: format!("Inscripción al taller {}", t("(simulado)")),
        fields: vec![
            field(
                "Nombre",
                None,
                ModalInput::TextInput {
                    custom_id: "nombre".into(),
                    long: false,
                    required: true,
                    min_length: Some(2),
                    max_length: Some(40),
                    value: None,
                    placeholder: Some("Cómo te llamamos".into()),
                },
            ),
            field(
                "¿Qué quieres aprender?",
                Some("Hasta 300 caracteres."),
                ModalInput::TextInput {
                    custom_id: "tema".into(),
                    long: true,
                    required: false,
                    min_length: None,
                    max_length: Some(300),
                    value: None,
                    placeholder: None,
                },
            ),
            field(
                "Nivel",
                None,
                ModalInput::Radio {
                    custom_id: "nivel".into(),
                    required: true,
                    options: vec![
                        option("Empiezo", "inicial", None, false),
                        option("Me defiendo", "medio", None, true),
                        option("Enseño", "avanzado", None, false),
                    ],
                },
            ),
            field(
                "Días que puedes",
                None,
                ModalInput::Menu(Select {
                    custom_id: Some("dias".into()),
                    placeholder: Some("Elige días".into()),
                    options: vec![
                        option("Jueves", "jueves", None, false),
                        option("Viernes", "viernes", None, false),
                        option("Sábado", "sabado", None, false),
                    ],
                    min: 1,
                    max: 3,
                    required: true,
                    ..Select::default()
                }),
            ),
            field(
                "Avisos",
                None,
                ModalInput::Check {
                    custom_id: "avisos".into(),
                    value: true,
                },
            ),
            field(
                "Algo tuyo para mostrar",
                Some("Hasta dos archivos, si quieres."),
                ModalInput::Files {
                    custom_id: "archivos".into(),
                    required: false,
                    min: 1,
                    max: 2,
                    file_types: vec!["image".into(), "audio".into(), ".pdf".into(), ".txt".into()],
                },
            ),
        ],
    }
}

/// RuneBot's commands.
fn commands() -> Vec<AppCommand> {
    let option = |kind, name: &str, description: &str| CommandOption::new(kind, name, description);
    let command =
        |id: u64, name: &str, description: &str, options: Vec<CommandOption>| AppCommand {
            id,
            version: 1,
            app: APP,
            app_name: format!("{APP_NAME} {}", t("(simulado)")),
            app_icon: None,
            name: name.into(),
            description: description.into(),
            kind: CommandKind::ChatInput,
            options,
            server: None,
            in_servers: true,
            in_direct: true,
            needs: None,
            access: CommandAccess::default(),
            nsfw: false,
        };
    let mut faces = option(OptionKind::Integer, "caras", "Cuántas caras tiene el dado");
    faces.min = Some(2.0);
    faces.max = Some(100.0);
    let private = option(OptionKind::Boolean, "privado", "Que sólo lo veas tú");
    let mut text = option(OptionKind::Text, "texto", "Qué te recuerdo");
    text.required = true;
    text.max_length = Some(100);
    let mut when = option(OptionKind::Integer, "en", "Cuándo");
    when.required = true;
    when.choices = [("5 minutos", 5), ("1 hora", 60), ("1 día", 1440)]
        .into_iter()
        .map(|(name, minutes)| OptionChoice {
            name: name.into(),
            value: OptionValue::Integer(minutes),
        })
        .collect();
    let mut topic = option(OptionKind::Text, "tema", "Qué buscar: RuneBot sugiere");
    topic.required = true;
    topic.autocomplete = true;
    let mut whom = option(OptionKind::User, "a", "A quién saludar");
    whom.required = true;
    let mut place = option(OptionKind::Channel, "en", "Dónde avisar");
    place.required = true;
    place.channels = vec![ChannelKind::Text];
    let mut on = option(OptionKind::Subcommand, "activar", "Activa los avisos");
    on.options = vec![place];
    let off = option(OptionKind::Subcommand, "desactivar", "Desactiva los avisos");
    let mut notices = option(OptionKind::Group, "avisos", "Los avisos de RuneBot");
    notices.options = vec![on, off];
    let show = option(OptionKind::Subcommand, "ver", "Mira la configuración");
    let mut moderate = command(
        908,
        "moderar",
        "Sólo para quien gestiona mensajes",
        Vec::new(),
    );
    moderate.needs = Some(Permissions::MANAGE_MESSAGES);
    moderate.in_direct = false;
    // A user command, run from a person's menu: it has no description.
    let mut greet = command(909, "Saludar", "", Vec::new());
    greet.kind = CommandKind::User;
    vec![
        command(901, "dado", "Tira un dado", vec![faces, private]),
        command(
            902,
            "recordatorio",
            "Pídele un recordatorio",
            vec![text, when],
        ),
        command(903, "buscar", "Busca un tema", vec![topic]),
        command(904, "saludar", "Saluda a alguien", vec![whom]),
        command(905, "config", "Configura RuneBot", vec![notices, show]),
        command(
            906,
            "formulario",
            "Abre el formulario del taller",
            Vec::new(),
        ),
        command(907, "secreto", "Un mensaje que sólo ves tú", Vec::new()),
        moderate,
        greet,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockConfig, MockProvider};

    fn general(provider: &MockProvider) -> ChannelId {
        let snapshot = provider.snapshot();
        snapshot
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .expect("#general")
            .id
    }

    #[test]
    fn the_app_lists_its_commands_with_groups_and_choices() {
        let provider = MockProvider::new(MockConfig::default());
        let channel = general(&provider);
        let events = provider.world.lock().unwrap().app_commands(7, channel);
        let [Event::AppCommands {
            request: 7,
            commands,
            ..
        }] = events.as_slice()
        else {
            panic!("{events:?}")
        };
        let config = commands.iter().find(|c| c.name == "config").unwrap();
        let leaves: Vec<_> = config.leaves().into_iter().map(|(path, _)| path).collect();
        assert_eq!(
            leaves,
            [
                vec!["avisos".to_owned(), "activar".to_owned()],
                vec!["avisos".to_owned(), "desactivar".to_owned()],
                vec!["ver".to_owned()],
            ]
        );
        let used = CommandUse {
            focused: Some(("tema".into(), "re".into())),
            ..CommandUse::of(
                commands.iter().find(|c| c.name == "buscar").unwrap(),
                Vec::new(),
                Vec::new(),
            )
        };
        let suggested = suggest(3, &used);
        let [Event::CommandChoices { choices, .. }] = suggested.as_slice() else {
            panic!("{suggested:?}")
        };
        let names: Vec<_> = choices.iter().map(|choice| choice.name.as_str()).collect();
        assert_eq!(names, ["recetas (simulado)", "redes (simulado)"]);
    }

    #[test]
    fn a_user_command_greets_the_person_it_runs_on() {
        let provider = MockProvider::new(MockConfig::default());
        let channel = general(&provider);
        let mut world = provider.world.lock().unwrap();
        let greet = commands()
            .into_iter()
            .find(|c| c.name == "Saludar")
            .unwrap();
        assert_eq!(greet.kind, CommandKind::User);
        let used = CommandUse {
            target: Some(UserId(21)),
            ..CommandUse::of(&greet, Vec::new(), Vec::new())
        };
        let events = world.answer(1, channel, Interaction::Command(used), &[], Timestamp(0));
        let Event::MessageCreated { message, .. } = &events[0] else {
            panic!("{events:?}")
        };
        assert!(
            message.content.starts_with("👋 Hola, "),
            "{}",
            message.content
        );
        assert!(
            message.content.ends_with("(simulado)."),
            "{}",
            message.content
        );
        assert_eq!(message.author, APP, "the app says it");
    }

    #[test]
    fn a_button_answers_only_the_account_and_a_command_answers_the_same_each_time() {
        let provider = MockProvider::new(MockConfig::default());
        let channel = general(&provider);
        let mut world = provider.world.lock().unwrap();
        let (history, _) = world.history(channel, None, 20);
        let app = history
            .iter()
            .find(|message| message.components.whole && message.components.app == Some(APP))
            .expect("the app's message")
            .id;
        let press = |custom_id: &str| Interaction::Part {
            message: app,
            custom_id: custom_id.into(),
            values: Vec::new(),
        };
        assert!(world.check(channel, &press("rb:confirmar")).is_ok());
        assert!(world.check(channel, &press("rb:otro")).is_err());
        let events = world.answer(1, channel, press("rb:confirmar"), &[], Timestamp(0));
        let Event::MessageCreated { message, .. } = &events[0] else {
            panic!("{events:?}")
        };
        assert!(message.components.ephemeral);
        assert_eq!(
            events[1],
            Event::Interaction {
                request: 1,
                done: true
            }
        );
        let (later, _) = world.history(channel, None, 5);
        assert!(
            later.iter().all(|kept| kept.id != message.id),
            "never in the history"
        );
        let failed = world.answer(2, channel, press("rb:no-puedo"), &[], Timestamp(0));
        assert!(matches!(failed[0], Event::Failed { .. }));
        let opened = world.answer(3, channel, press("rb:formulario"), &[], Timestamp(0));
        let Event::ModalOpened { modal, .. } = &opened[0] else {
            panic!("{opened:?}")
        };
        let submit = Interaction::Modal {
            id: modal.id,
            app: APP,
            custom_id: modal.custom_id.clone(),
            answers: vec![("nombre".into(), FieldValue::Text("Ada".into()))],
        };
        assert!(world.check(channel, &submit).is_ok());
        // Files go with the modal: only the ones its answers name, and
        // they come back with the app's answer.
        let file = |token: u64, name: &str| {
            PreparedUpload::new(
                rune_model::PendingAttachment {
                    token: rune_model::UploadToken(token),
                    filename: name.into(),
                    size_bytes: 3,
                },
                bytes::Bytes::from_static(b"abc"),
            )
            .unwrap()
        };
        let with_files = Interaction::Modal {
            id: modal.id,
            app: APP,
            custom_id: modal.custom_id.clone(),
            answers: vec![(
                "archivos".into(),
                FieldValue::Files(vec![rune_model::UploadToken(5)]),
            )],
        };
        assert!(carried(&with_files, &[file(5, "foto.png")]).is_ok());
        assert!(carried(&with_files, &[]).is_err(), "named, not carried");
        assert!(
            carried(&submit, &[file(5, "foto.png")]).is_err(),
            "carried, not named"
        );
        assert!(
            carried(&with_files, &[file(5, "programa.exe")]).is_err(),
            "a kind the field does not take"
        );
        let in_field = |field: &str, tokens: Vec<u64>| Interaction::Modal {
            id: modal.id,
            app: APP,
            custom_id: modal.custom_id.clone(),
            answers: vec![(
                field.into(),
                FieldValue::Files(tokens.into_iter().map(rune_model::UploadToken).collect()),
            )],
        };
        let three = [file(5, "a.png"), file(6, "b.png"), file(7, "c.png")];
        assert!(
            carried(&in_field("archivos", vec![5, 6, 7]), &three).is_err(),
            "above its maximum"
        );
        assert!(
            carried(&in_field("nombre", vec![5]), &three[..1]).is_err(),
            "not a file field"
        );
        assert!(
            carried(&in_field("otro", vec![5]), &three[..1]).is_err(),
            "not in the form"
        );
        let answered = world.answer(4, channel, with_files, &[file(5, "foto.png")], Timestamp(0));
        let Event::MessageCreated { message, .. } = &answered[0] else {
            panic!("{answered:?}")
        };
        assert!(message.content.contains("**archivos**: foto.png"));
        assert_eq!(message.attachments.len(), 1);
        assert_eq!(message.attachments[0].filename, "foto.png");
        let dice = commands().into_iter().find(|c| c.name == "dado").unwrap();
        let roll = |world: &mut World| {
            let used = CommandUse::of(
                &dice,
                Vec::new(),
                vec![("caras".into(), OptionValue::Integer(20))],
            );
            match &world.answer(9, channel, Interaction::Command(used), &[], Timestamp(0))[0] {
                Event::MessageCreated { message, .. } => message.content.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            roll(&mut world),
            roll(&mut world),
            "the same request, the same roll"
        );
    }
}
