//! What the account does with apps: the commands a conversation offers, and
//! what it tells an app when it runs one, presses one of its buttons, picks
//! in one of its menus or fills one of its modals. A provider lists the
//! commands and carries each use; the app answers with messages, a modal or
//! suggestions for an option being written.

use rune_i18n::{t, tr};

use crate::components::{MenuOption, Select};
use crate::{
    ChannelId, ChannelKind, MessageId, Permissions, RemoteImage, RoleId, ServerId, UploadToken,
    UserId,
};

/// Options a command has at most, at each level.
pub const MAX_OPTIONS: usize = 25;
/// Choices an option offers at most, its own or suggested.
pub const MAX_CHOICES: usize = 25;
/// The longest a command's text option may be.
pub const MAX_TEXT: usize = 6000;

/// Where a command runs: in the composer as `/name`, on a person from their
/// menu, or on a message from its menu.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CommandKind {
    /// Typed in the composer, with options.
    #[default]
    ChatInput,
    /// Run on a person ([`CommandUse::target`]), with no options.
    User,
    /// Run on a message. Listed, but RUNE offers none yet.
    Message,
}

/// A command an app offers, as listed for a conversation.
#[derive(Clone, Debug, PartialEq)]
pub struct AppCommand {
    pub id: u64,
    /// The version it was listed at, given back when it runs.
    pub version: u64,
    pub app: UserId,
    pub app_name: String,
    pub app_icon: Option<RemoteImage>,
    pub name: String,
    pub description: String,
    pub kind: CommandKind,
    pub options: Vec<CommandOption>,
    /// Only in this server's conversations; `None`, wherever its app is.
    pub server: Option<ServerId>,
    /// Where it may run: in servers, in direct conversations.
    pub in_servers: bool,
    pub in_direct: bool,
    /// What the account needs in the channel to run it, unless an
    /// override says otherwise; `None`, nothing beyond using commands.
    pub needs: Option<Permissions>,
    pub access: CommandAccess,
    /// Only in channels for adults.
    pub nsfw: bool,
}

/// What a server's overrides say of a command for the account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandAccess {
    /// What an override for the account or one of its roles says, above
    /// what it needs; `None` without one.
    pub granted: Option<bool>,
    /// What overrides say channel by channel.
    pub channels: Vec<(ChannelId, bool)>,
    /// What an override says for every channel of the server.
    pub all_channels: Option<bool>,
}

impl AppCommand {
    /// The ways to run it, as `/name sub` with what each does: the command
    /// itself, or each of its subcommands, in groups or not.
    pub fn leaves(&self) -> Vec<(Vec<String>, &str)> {
        if !self
            .options
            .iter()
            .any(|option| matches!(option.kind, OptionKind::Subcommand | OptionKind::Group))
        {
            return vec![(Vec::new(), self.description.as_str())];
        }
        let mut found = Vec::new();
        for option in &self.options {
            match option.kind {
                OptionKind::Subcommand => {
                    found.push((vec![option.name.clone()], option.description.as_str()))
                }
                OptionKind::Group => {
                    for sub in &option.options {
                        found.push((
                            vec![option.name.clone(), sub.name.clone()],
                            sub.description.as_str(),
                        ));
                    }
                }
                _ => {}
            }
        }
        found
    }

    /// The options asked for by the subcommand at `path`; `None` when there
    /// is no such subcommand.
    pub fn options_at(&self, path: &[String]) -> Option<&[CommandOption]> {
        let mut options = self.options.as_slice();
        for name in path {
            let found = options.iter().find(|option| {
                option.name == *name
                    && matches!(option.kind, OptionKind::Subcommand | OptionKind::Group)
            })?;
            options = &found.options;
        }
        let branches = options
            .iter()
            .any(|option| matches!(option.kind, OptionKind::Subcommand | OptionKind::Group));
        (!branches).then_some(options)
    }

    /// Why `used` cannot run as this command, when it cannot: a path it
    /// does not have, an option it does not take or one given twice, a
    /// required one missing, or a value of the wrong kind or out of bounds.
    pub fn check(&self, used: &CommandUse) -> Result<(), String> {
        // A user command runs on a person, a chat command on none; a message
        // command runs nowhere yet.
        let fits = match self.kind {
            CommandKind::ChatInput => used.target.is_none(),
            CommandKind::User => used.target.is_some(),
            CommandKind::Message => false,
        };
        if !fits || used.command != self.id || used.app != self.app {
            return Err(t("Ese comando ya no está disponible aquí").into());
        }
        let options = self
            .options_at(&used.path)
            .ok_or_else(|| t("Elige un subcomando de la lista"))?;
        for (index, (name, value)) in used.values.iter().enumerate() {
            let option = options
                .iter()
                .find(|option| option.name == *name)
                .ok_or_else(|| tr!("El comando no tiene la opción «{name}»", name))?;
            if used.values[..index].iter().any(|(seen, _)| seen == name) {
                return Err(tr!("La opción «{name}» va una sola vez", name));
            }
            let kind_matches = matches!(
                (option.kind, value),
                (OptionKind::Text, OptionValue::Text(_))
                    | (OptionKind::Integer, OptionValue::Integer(_))
                    | (OptionKind::Number, OptionValue::Number(_))
                    | (OptionKind::Boolean, OptionValue::Boolean(_))
                    | (OptionKind::User, OptionValue::User(_))
                    | (OptionKind::Channel, OptionValue::Channel(_))
                    | (OptionKind::Role, OptionValue::Role(_))
                    | (
                        OptionKind::Mentionable,
                        OptionValue::User(_) | OptionValue::Role(_)
                    )
            );
            if !kind_matches {
                return Err(tr!("La opción «{name}» no es de ese tipo", name));
            }
            option
                .parse(&value.written())
                .map_err(|reason| format!("{name}: {reason}"))?;
        }
        if let Some(missing) = options.iter().find(|option| {
            option.required && !used.values.iter().any(|(name, _)| *name == option.name)
        }) {
            return Err(tr!("Falta la opción «{name}»", name = missing.name));
        }
        Ok(())
    }
}

/// An option of a command, or one of its subcommands or groups.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandOption {
    pub kind: OptionKind,
    pub name: String,
    pub description: String,
    pub required: bool,
    /// The only values it takes, by name.
    pub choices: Vec<OptionChoice>,
    /// A group's subcommands, or a subcommand's options.
    pub options: Vec<CommandOption>,
    /// The kinds of channel it takes; empty, any.
    pub channels: Vec<ChannelKind>,
    /// A number's bounds.
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// A text's bounds, in characters.
    pub min_length: Option<u16>,
    pub max_length: Option<u16>,
    /// Its app suggests values as it is written.
    pub autocomplete: bool,
}

impl CommandOption {
    /// An option of `kind` named `name`, with nothing else said of it.
    pub fn new(kind: OptionKind, name: &str, description: &str) -> Self {
        Self {
            kind,
            name: name.into(),
            description: description.into(),
            required: false,
            choices: Vec::new(),
            options: Vec::new(),
            channels: Vec::new(),
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            autocomplete: false,
        }
    }

    /// What `text`, written for it, gives its app; `Ok(None)` for nothing
    /// written, which only an optional option allows. The error says why
    /// it is not a value it takes. People, roles and channels are picked,
    /// not written: their text is an id.
    pub fn parse(&self, text: &str) -> Result<Option<OptionValue>, String> {
        let text = text.trim();
        if text.is_empty() {
            return if self.required {
                Err(t("Completa este campo").into())
            } else {
                Ok(None)
            };
        }
        if let Some(choice) = self
            .choices
            .iter()
            .find(|choice| choice.name == text || choice.value.written() == text)
        {
            return Ok(Some(choice.value.clone()));
        }
        if !self.choices.is_empty() && !self.autocomplete {
            return Err(t("Elige una de las opciones").into());
        }
        let range = |number: f64| {
            if self.min.is_some_and(|min| number < min) || self.max.is_some_and(|max| number > max)
            {
                Err(match (self.min, self.max) {
                    (Some(min), Some(max)) => tr!("Tiene que estar entre {min} y {max}", min, max),
                    (Some(min), None) => tr!("Tiene que ser al menos {min}", min),
                    (_, Some(max)) => tr!("Tiene que ser como mucho {max}", max),
                    (None, None) => unreachable!(),
                })
            } else {
                Ok(())
            }
        };
        let id = |text: &str| text.parse::<u64>().ok().filter(|id| *id != 0);
        Ok(Some(match self.kind {
            OptionKind::Text => {
                let length = text.chars().count();
                let max = self
                    .max_length
                    .map_or(MAX_TEXT, |max| usize::from(max).min(MAX_TEXT));
                if self.min_length.is_some_and(|min| length < usize::from(min)) {
                    return Err(tr!(
                        "Escribe al menos {min} caracteres",
                        min = self.min_length.unwrap_or(0)
                    ));
                }
                if length > max {
                    return Err(tr!("Escribe como mucho {max} caracteres", max));
                }
                OptionValue::Text(text.into())
            }
            OptionKind::Integer => {
                let number: i64 = text
                    .parse()
                    .map_err(|_| t("Escribe un número entero").to_owned())?;
                range(number as f64)?;
                OptionValue::Integer(number)
            }
            OptionKind::Number => {
                let number: f64 = text
                    .replace(',', ".")
                    .parse()
                    .ok()
                    .filter(|number: &f64| number.is_finite())
                    .ok_or_else(|| t("Escribe un número").to_owned())?;
                range(number)?;
                OptionValue::Number(number)
            }
            OptionKind::Boolean => match text {
                "true" | "sí" | "si" => OptionValue::Boolean(true),
                "false" | "no" => OptionValue::Boolean(false),
                _ => return Err(t("Elige sí o no").into()),
            },
            OptionKind::User => {
                OptionValue::User(UserId(id(text).ok_or_else(|| t("Elige a alguien"))?))
            }
            OptionKind::Role => {
                OptionValue::Role(RoleId(id(text).ok_or_else(|| t("Elige un rol"))?))
            }
            OptionKind::Channel => {
                OptionValue::Channel(ChannelId(id(text).ok_or_else(|| t("Elige un canal"))?))
            }
            OptionKind::Mentionable => match text.split_once(':') {
                Some(("role", number)) => OptionValue::Role(RoleId(
                    id(number).ok_or_else(|| t("Elige a alguien o un rol"))?,
                )),
                _ => OptionValue::User(UserId(
                    id(text).ok_or_else(|| t("Elige a alguien o un rol"))?,
                )),
            },
            OptionKind::Subcommand | OptionKind::Group | OptionKind::Attachment => {
                return Err(t("RUNE no completa este tipo de opción").into())
            }
        }))
    }
}

/// What an option takes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OptionKind {
    Subcommand,
    /// Subcommands under a name.
    Group,
    Text,
    Integer,
    Boolean,
    User,
    Channel,
    Role,
    /// Someone or a role.
    Mentionable,
    Number,
    /// A file.
    Attachment,
}

/// A value an option takes, by the name it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionChoice {
    pub name: String,
    pub value: OptionValue,
}

/// What the account gives an option.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionValue {
    Text(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    User(UserId),
    Channel(ChannelId),
    Role(RoleId),
}

impl OptionValue {
    /// The text of a text value.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// How it is written in the form: what [`CommandOption::parse`] reads.
    pub fn written(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Integer(number) => number.to_string(),
            Self::Number(number) => number.to_string(),
            Self::Boolean(true) => "true".into(),
            Self::Boolean(false) => "false".into(),
            Self::User(id) => id.0.to_string(),
            Self::Channel(id) => id.0.to_string(),
            Self::Role(id) => format!("role:{}", id.0),
        }
    }
}

/// A command as the account runs it.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandUse {
    pub command: u64,
    pub version: u64,
    pub app: UserId,
    pub name: String,
    /// The server the command belongs to, when it belongs to one.
    pub server: Option<ServerId>,
    /// The group and subcommand chosen, outermost first.
    pub path: Vec<String>,
    /// Each option given, by name.
    pub values: Vec<(String, OptionValue)>,
    /// For suggestions: the option being written and its text so far.
    pub focused: Option<(String, String)>,
    /// The person a user command runs on; `None` for the other kinds.
    pub target: Option<UserId>,
}

impl CommandUse {
    /// `command` run at `path` with `values`.
    pub fn of(command: &AppCommand, path: Vec<String>, values: Vec<(String, OptionValue)>) -> Self {
        Self {
            command: command.id,
            version: command.version,
            app: command.app,
            name: command.name.clone(),
            server: command.server,
            path,
            values,
            focused: None,
            target: None,
        }
    }
}

/// A form an app opened for the account to fill, in answer to an interaction.
#[derive(Clone, Debug, PartialEq)]
pub struct Modal {
    /// The service's id of the interaction that opened it.
    pub id: u64,
    pub app: UserId,
    pub custom_id: String,
    pub title: String,
    pub fields: Vec<ModalField>,
}

/// One field of a modal, with what it asks over it.
#[derive(Clone, Debug, PartialEq)]
pub struct ModalField {
    pub label: Option<String>,
    pub description: Option<String>,
    pub input: ModalInput,
}

/// What a field of a modal takes.
#[derive(Clone, Debug, PartialEq)]
pub enum ModalInput {
    /// Markdown to read.
    Text(String),
    /// Text to write, on one line or several (`long`).
    TextInput {
        custom_id: String,
        long: bool,
        required: bool,
        min_length: Option<u16>,
        max_length: Option<u16>,
        value: Option<String>,
        placeholder: Option<String>,
    },
    /// A menu, as in messages.
    Menu(Select),
    /// One of a few options, side by side.
    Radio {
        custom_id: String,
        required: bool,
        options: Vec<MenuOption>,
    },
    /// Some of a few options, each with its box.
    Checks {
        custom_id: String,
        min: u8,
        max: u8,
        options: Vec<MenuOption>,
    },
    /// One box, on or off.
    Check { custom_id: String, value: bool },
    /// Files from the account's computer, `min` to `max` of them; they go
    /// with the modal (comandos-de-apps). `file_types`, when any, are the
    /// kinds it takes: see [`ModalInput::accepts`].
    Files {
        custom_id: String,
        required: bool,
        min: u8,
        max: u8,
        file_types: Vec<String>,
    },
    /// A kind RUNE does not fill, by the service's number: the modal can
    /// only be closed.
    Unsupported(u16),
}

impl ModalInput {
    /// What its app calls it; `None` for text to read.
    pub fn custom_id(&self) -> Option<&str> {
        match self {
            Self::TextInput { custom_id, .. }
            | Self::Radio { custom_id, .. }
            | Self::Checks { custom_id, .. }
            | Self::Check { custom_id, .. }
            | Self::Files { custom_id, .. } => Some(custom_id),
            Self::Menu(menu) => menu.custom_id.as_deref(),
            Self::Text(_) | Self::Unsupported(_) => None,
        }
    }

    /// Whether a file field takes a file named `filename`: any, unless it
    /// lists kinds; then one whose extension is a listed `.ext`, or of a
    /// listed family (`image`, `video`, `audio`). By name, never by
    /// content, as Serein and the service match them. Other fields take none.
    pub fn accepts(&self, filename: &str) -> bool {
        let Self::Files { file_types, .. } = self else {
            return false;
        };
        if file_types.is_empty() {
            return true;
        }
        let Some((_, extension)) = filename.rsplit_once('.') else {
            return false;
        };
        let extension = extension.to_ascii_lowercase();
        let family = match extension.as_str() {
            "apng" | "avif" | "bmp" | "gif" | "heic" | "heif" | "ico" | "jpeg" | "jpg" | "png"
            | "svg" | "tif" | "tiff" | "webp" => "image",
            "3gp" | "avi" | "m4v" | "mkv" | "mov" | "mp4" | "mpeg" | "mpg" | "ogv" | "webm" => {
                "video"
            }
            "aac" | "aif" | "aiff" | "flac" | "m4a" | "mp3" | "oga" | "ogg" | "opus" | "wav"
            | "wma" => "audio",
            _ => "",
        };
        file_types.iter().any(|kind| match kind.strip_prefix('.') {
            Some(listed) => listed.eq_ignore_ascii_case(&extension),
            None => !family.is_empty() && kind.eq_ignore_ascii_case(family),
        })
    }

    /// What it starts with.
    pub fn starting(&self) -> FieldValue {
        match self {
            Self::TextInput { value, .. } => FieldValue::Text(value.clone().unwrap_or_default()),
            Self::Menu(menu) => FieldValue::Values(menu.starting()),
            Self::Radio { options, .. } | Self::Checks { options, .. } => FieldValue::Values(
                options
                    .iter()
                    .filter(|option| option.default)
                    .map(|option| option.value.clone())
                    .collect(),
            ),
            Self::Check { value, .. } => FieldValue::Check(*value),
            Self::Files { .. } => FieldValue::Files(Vec::new()),
            Self::Text(_) | Self::Unsupported(_) => FieldValue::Values(Vec::new()),
        }
    }

    /// Why `value` cannot be sent for it, when it cannot.
    pub fn problem(&self, value: &FieldValue) -> Option<String> {
        match (self, value) {
            (
                Self::TextInput {
                    required,
                    min_length,
                    max_length,
                    ..
                },
                FieldValue::Text(text),
            ) => {
                let length = text.chars().count();
                let max = max_length.map_or(4000, |max| usize::from(max).min(4000));
                if length == 0 {
                    return required.then(|| t("Completa este campo").into());
                }
                if min_length.is_some_and(|min| length < usize::from(min)) {
                    return Some(tr!(
                        "Escribe al menos {min} caracteres",
                        min = min_length.unwrap_or(0)
                    ));
                }
                (length > max).then(|| tr!("Escribe como mucho {max} caracteres", max))
            }
            (Self::Menu(menu), FieldValue::Values(values)) => {
                if values.is_empty() {
                    return (menu.required || menu.min > 0).then(|| t("Elige una opción").into());
                }
                (!menu.accepts(values)).then(|| t("Elige dentro de lo que se permite").into())
            }
            (
                Self::Radio {
                    required, options, ..
                },
                FieldValue::Values(values),
            ) => match values.as_slice() {
                [] => required.then(|| t("Elige una opción").into()),
                [one] if options.iter().any(|option| option.value == *one) => None,
                _ => Some(t("Elige una opción").into()),
            },
            (
                Self::Checks {
                    min, max, options, ..
                },
                FieldValue::Values(values),
            ) => {
                let menu = Select {
                    options: options.clone(),
                    min: *min,
                    max: (*max).max(1),
                    ..Select::default()
                };
                let none_needed = values.is_empty() && *min == 0;
                (!none_needed && !menu.accepts(values))
                    .then(|| t("Elige dentro de lo que se permite").into())
            }
            (Self::Check { .. }, FieldValue::Check(_)) => None,
            (
                Self::Files {
                    required, min, max, ..
                },
                FieldValue::Files(files),
            ) => {
                if files.is_empty() {
                    return (*required && *min > 0).then(|| t("Adjunta un archivo").into());
                }
                if files.len() < usize::from(*min) {
                    return Some(tr!("Adjunta al menos {min} archivos", min));
                }
                (files.len() > usize::from(*max))
                    .then(|| tr!("Adjunta como mucho {max} archivos", max))
            }
            (Self::Text(_), _) => None,
            (Self::Unsupported(_), _) => Some(t("RUNE no completa este campo").into()),
            _ => Some(t("El valor no corresponde al campo").into()),
        }
    }
}

/// What the account gave a field of a modal.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Text(String),
    Values(Vec<String>),
    Check(bool),
    /// Files prepared for the modal's conversation, by token.
    Files(Vec<UploadToken>),
}

/// What the account tells an app.
#[derive(Clone, Debug, PartialEq)]
pub enum Interaction {
    /// It pressed a button of `message`, or picked `values` in one of its
    /// menus, by the part's `custom_id`.
    Part {
        message: MessageId,
        custom_id: String,
        values: Vec<String>,
    },
    /// It filled the modal `id` of `app`, field by field.
    Modal {
        id: u64,
        app: UserId,
        custom_id: String,
        answers: Vec<(String, FieldValue)>,
    },
    /// It ran a command.
    Command(CommandUse),
}

impl Interaction {
    /// The files a modal's answers carry, field by field.
    pub fn files(&self) -> Vec<UploadToken> {
        let Self::Modal { answers, .. } = self else {
            return Vec::new();
        };
        answers
            .iter()
            .flat_map(|(_, value)| match value {
                FieldValue::Files(files) => files.as_slice(),
                _ => &[],
            })
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> AppCommand {
        let mut count = CommandOption::new(OptionKind::Integer, "caras", "Cuántas caras");
        count.min = Some(2.0);
        count.max = Some(100.0);
        let mut sub = CommandOption::new(OptionKind::Subcommand, "tirar", "Tira un dado");
        sub.options = vec![count];
        let mut group = CommandOption::new(OptionKind::Group, "dado", "Dados");
        group.options = vec![sub];
        AppCommand {
            id: 1,
            version: 1,
            app: UserId(9),
            app_name: "Bot".into(),
            app_icon: None,
            name: "juego".into(),
            description: "Juegos".into(),
            kind: CommandKind::ChatInput,
            options: vec![group],
            server: None,
            in_servers: true,
            in_direct: true,
            needs: None,
            access: CommandAccess::default(),
            nsfw: false,
        }
    }

    #[test]
    fn a_command_with_groups_runs_by_each_subcommand() {
        let command = command();
        let leaves = command.leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, ["dado", "tirar"]);
        let path = vec!["dado".to_owned(), "tirar".to_owned()];
        assert_eq!(command.options_at(&path).unwrap()[0].name, "caras");
        assert!(
            command.options_at(&path[..1]).is_none(),
            "a group is not run"
        );
        assert!(command.options_at(&["otro".to_owned()]).is_none());
        let used =
            |values: Vec<(String, OptionValue)>| CommandUse::of(&command, path.clone(), values);
        assert!(command.check(&used(Vec::new())).is_ok(), "nothing required");
        let count = |n| vec![("caras".to_owned(), OptionValue::Integer(n))];
        assert!(command.check(&used(count(6))).is_ok());
        assert!(command.check(&used(count(200))).is_err(), "out of bounds");
        assert!(
            command
                .check(&used(vec![("caras".into(), OptionValue::Text("6".into()))]))
                .is_err(),
            "of another kind"
        );
        assert!(
            command
                .check(&used(vec![("otra".into(), OptionValue::Integer(1))]))
                .is_err(),
            "not one of its options"
        );
        assert!(
            command
                .check(&CommandUse::of(&command, Vec::new(), Vec::new()))
                .is_err(),
            "a subcommand must be chosen"
        );
    }

    #[test]
    fn a_user_command_runs_on_a_person_and_a_chat_command_on_none() {
        let mut user = command();
        user.kind = CommandKind::User;
        user.options.clear();
        let on = |target| CommandUse {
            target,
            ..CommandUse::of(&user, Vec::new(), Vec::new())
        };
        assert!(user.check(&on(Some(UserId(7)))).is_ok());
        assert!(user.check(&on(None)).is_err(), "a person must be given");
        let chat = command();
        let targeted = CommandUse {
            target: Some(UserId(7)),
            ..CommandUse::of(&chat, vec!["dado".into(), "tirar".into()], Vec::new())
        };
        assert!(
            chat.check(&targeted).is_err(),
            "a chat command takes no person"
        );
    }

    #[test]
    fn option_text_is_checked_before_it_goes() {
        let mut count = CommandOption::new(OptionKind::Integer, "n", "");
        count.min = Some(2.0);
        count.max = Some(6.0);
        count.required = true;
        assert_eq!(count.parse("4"), Ok(Some(OptionValue::Integer(4))));
        assert!(count.parse("").is_err(), "required");
        assert!(count.parse("7").is_err(), "above its maximum");
        assert!(count.parse("2.5").is_err(), "not whole");
        let mut text = CommandOption::new(OptionKind::Text, "t", "");
        text.max_length = Some(3);
        assert_eq!(text.parse(""), Ok(None));
        assert!(text.parse("hola").is_err());
        let mut pick = CommandOption::new(OptionKind::Text, "c", "");
        pick.choices = vec![OptionChoice {
            name: "Rojo".into(),
            value: OptionValue::Text("red".into()),
        }];
        assert_eq!(
            pick.parse("Rojo"),
            Ok(Some(OptionValue::Text("red".into())))
        );
        assert!(pick.parse("verde").is_err(), "only its choices");
        let number = CommandOption::new(OptionKind::Number, "x", "");
        assert_eq!(number.parse("1,5"), Ok(Some(OptionValue::Number(1.5))));
        let who = CommandOption::new(OptionKind::Mentionable, "m", "");
        assert_eq!(who.parse("role:5"), Ok(Some(OptionValue::Role(RoleId(5)))));
        assert_eq!(
            who.parse(&OptionValue::User(UserId(7)).written()),
            Ok(Some(OptionValue::User(UserId(7))))
        );
    }

    #[test]
    fn a_modal_field_says_why_its_value_cannot_go() {
        let input = ModalInput::TextInput {
            custom_id: "a".into(),
            long: false,
            required: true,
            min_length: Some(2),
            max_length: Some(5),
            value: None,
            placeholder: None,
        };
        assert!(input.problem(&FieldValue::Text(String::new())).is_some());
        assert!(input.problem(&FieldValue::Text("x".into())).is_some());
        assert!(input.problem(&FieldValue::Text("hola".into())).is_none());
        assert!(input
            .problem(&FieldValue::Text("demasiado".into()))
            .is_some());
        let radio = ModalInput::Radio {
            custom_id: "r".into(),
            required: true,
            options: vec![MenuOption {
                value: "a".into(),
                ..MenuOption::default()
            }],
        };
        assert!(radio.problem(&FieldValue::Values(vec![])).is_some());
        assert!(radio
            .problem(&FieldValue::Values(vec!["a".into()]))
            .is_none());
        let files = ModalInput::Files {
            custom_id: "f".into(),
            required: true,
            min: 1,
            max: 2,
            file_types: Vec::new(),
        };
        let some = |n: u64| FieldValue::Files((1..=n).map(UploadToken).collect());
        assert!(files.problem(&files.starting()).is_some(), "required");
        assert!(files.problem(&some(2)).is_none());
        assert!(files.problem(&some(3)).is_some(), "above its maximum");
        assert!(files.accepts("anything") && !radio.accepts("a.png"));
        let optional = ModalInput::Files {
            custom_id: "f".into(),
            required: false,
            min: 1,
            max: 1,
            file_types: vec![".PDF".into(), "image".into(), "unknown".into()],
        };
        assert!(optional.problem(&some(0)).is_none());
        // Only the kinds it lists, by extension, whatever its case.
        for taken in ["informe.pdf", "Informe.Pdf", "foto.JPG", "a.b.webp"] {
            assert!(optional.accepts(taken), "{taken}");
        }
        for refused in ["programa.exe", "pdf", "clip.mp4", "sin-punto", "x.unknown"] {
            assert!(!optional.accepts(refused), "{refused}");
        }
        let sent = Interaction::Modal {
            id: 1,
            app: UserId(9),
            custom_id: "m".into(),
            answers: vec![
                ("f".into(), some(2)),
                ("t".into(), FieldValue::Text("x".into())),
            ],
        };
        assert_eq!(sent.files(), [UploadToken(1), UploadToken(2)]);
    }
}
