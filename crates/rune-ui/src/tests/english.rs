//! The whole window in English over the simulated world: no screen the
//! sweep opens shows Spanish of RUNE's own. What stays Spanish is the
//! simulation's content (its people, servers, channels, messages), which is
//! data, as a real server's would be.

use std::collections::BTreeMap;

use rune_i18n::Language;
use rune_model::{ServerId, UserId};

use super::*;

/// Words frequent in Spanish and rare in English interface text.
const SPANISH_WORDS: &[&str] = &[
    "de",
    "del",
    "la",
    "las",
    "los",
    "el",
    "con",
    "para",
    "por",
    "que",
    "una",
    "y",
    "es",
    "se",
    "sin",
    "al",
    "su",
    "sus",
    "este",
    "esta",
    "hay",
    "ya",
    "aca",
    "todavia",
    "simulado",
    "simulada",
    "canal",
    "canales",
    "servidor",
    "servidores",
    "mensaje",
    "mensajes",
    "ajustes",
    "nuevo",
    "nueva",
    "ver",
    "cuenta",
    "miembros",
    "abrir",
    "cerrar",
    "guardar",
    "buscar",
    "ninguno",
    "ninguna",
    "hoy",
    "ayer",
    "hace",
    "nada",
    "pulsar",
    "elegir",
    "volver",
];

/// Simulated content the sweep cannot find by itself. The role
/// «Moderación» and the embed author «Canal de voz» are also RUNE texts, so
/// they are not taken for content alone; the lines are the simulation's
/// made-up conversation in channels the store has not loaded (the last
/// message of a direct conversation on Home, a thread's preview).
const CONTENT: &[&str] = &[
    "Moderación",
    "Canal de voz",
    "Para mí el problema está en el layout, no en el render.",
    "¿Alguien tiene experiencia con PipeWire desde Rust?",
    "Vamos que el viernes se libera 🚀",
    "Les comparto una foto del setup de hoy.",
    // Short pieces of simulated messages, between a link or a date and
    // code: too short to be looked for inside the messages.
    " · empieza el ",
    "Probé con ",
    // The owned server's onboarding (rules and onboarding, 2026-10-18).
    "¿En qué quieres colaborar?",
    // The owned server's AutoMod rules (automod).
    "Sin spoilers",
    "Spam de menciones",
    "Lo que parece spam",
];

/// RUNE's own words that are the same in every language: a language's
/// name in itself.
const UNTRANSLATED: &[&str] = &["Español"];

/// Whether `text` reads as Spanish.
fn spanish(text: &str) -> bool {
    text.contains(['á', 'é', 'í', 'ó', 'ú', 'ñ', '¿', '¡'])
        || text
            .split(|c: char| !c.is_alphabetic())
            .any(|word| SPANISH_WORDS.contains(&word.to_lowercase().as_str()))
}

/// The simulation's content: every string in what it sent and what the
/// store holds, but RUNE's own texts that travel as keys (`es`), longest
/// first, so a name inside a longer one goes with it.
fn content(host: &Host) -> Vec<String> {
    let mut dump = format!("{:?}", host.provider.snapshot());
    let channels = host
        .store
        .servers()
        .iter()
        .flat_map(|server| host.store.channel_tree(server.id))
        .flat_map(|(_, channels)| channels)
        .map(|channel| channel.id)
        .chain(host.store.directs().iter().map(|direct| direct.id))
        .collect::<Vec<_>>();
    for channel in channels {
        dump.push_str(&format!("{:?}", host.store.channel(channel)));
        if let Some(timeline) = host.store.timeline(channel) {
            dump.push_str(&format!("{:?}", timeline.messages()));
        }
    }
    let mut strings = Vec::new();
    let mut rest = dump.as_str();
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let bytes = after.as_bytes();
        let mut end = 0;
        while end < bytes.len() && bytes[end] != b'"' {
            end += if bytes[end] == b'\\' { 2 } else { 1 };
        }
        let end = end.min(after.len());
        let string = after[..end].replace("\\n", "\n").replace("\\\"", "\"");
        if string.chars().count() >= 3 && rune_i18n::english(&string).is_none() {
            // As drawn, too: without the Markdown that marks it up.
            strings.push(string.replace("**", ""));
            strings.push(string);
        }
        rest = &after[(end + 1).min(after.len())..];
    }
    strings.extend(CONTENT.iter().map(|text| (*text).to_owned()));
    strings.sort_by_key(|string| std::cmp::Reverse(string.len()));
    strings.dedup();
    strings
}

/// What a person can read on screen: painted text, and what the platform
/// is told of each control.
fn readable(host: &mut Host) -> Vec<String> {
    let frame = host.frame();
    let mut texts: Vec<String> = frame
        .commands
        .iter()
        .filter_map(|command| match command {
            limn::DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    for node in &frame.access {
        texts.push(node.name.clone());
        texts.extend(node.description.clone());
    }
    texts
}

struct Sweep {
    content: Vec<String>,
    /// All the content in one string, to find a piece of it at once.
    joined: String,
    /// Each Spanish text found, with the first screen it was on.
    found: BTreeMap<String, String>,
    screens: usize,
}

impl Sweep {
    fn new(host: &Host) -> Self {
        let content = content(host);
        let joined = content.join("\u{0}");
        Self {
            content,
            joined,
            found: BTreeMap::new(),
            screens: 0,
        }
    }

    fn look(&mut self, host: &mut Host, screen: &str) {
        self.screens += 1;
        for text in readable(host) {
            // A piece of a message, a name, or one cut short with «…».
            // A short one only as a whole, so a word of RUNE's that some
            // message happens to hold is not let through.
            let piece = text.trim().trim_end_matches('…').trim();
            let found = if piece.chars().count() >= 16 {
                self.joined.contains(piece)
            } else {
                self.content.iter().any(|content| content.trim() == piece)
            };
            if piece.is_empty() || found {
                continue;
            }
            let mut left = text.clone();
            for content in self
                .content
                .iter()
                .map(String::as_str)
                .chain(UNTRANSLATED.iter().copied())
            {
                if left.contains(content) {
                    left = left.replace(content, " ");
                }
            }
            if spanish(&left) {
                self.found.entry(text).or_insert_with(|| screen.to_owned());
            }
        }
    }
}

impl Host {
    fn act_on(&mut self, id: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents);
        self.frame();
    }

    /// The secondary click a person gives `target`: its menu.
    fn menu_of(&mut self, target: &str) {
        self.frame();
        if let Some(intents) =
            secondary_click(&mut self.ui, &self.store, target, Point::new(400.0, 300.0))
        {
            self.run(intents);
        }
        self.frame();
    }

    fn server_called(&self, name: &str) -> ServerId {
        self.store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .unwrap()
            .id
    }

    fn someone_listed(&mut self) -> Option<UserId> {
        let frame = self.frame();
        frame
            .regions
            .iter()
            .filter_map(|region| region.id.strip_prefix("member:")?.parse().ok())
            .map(UserId)
            .find(|user| Some(*user) != self.store.me())
    }
}

#[test]
fn the_simulated_interface_in_english_has_no_spanish_of_its_own() {
    // From the start, so what the simulation labels is written in English.
    let _english = rune_i18n::scoped(Language::English);
    let mut host = Host::new();
    host.ui.prefs.language = Some(Language::English);
    let general = host.open_channel("general");
    host.act_on("jump:latest");
    let mut sweep = Sweep::new(&host);
    sweep.look(&mut host, "general");

    // The account's settings, every page.
    for page in crate::settings::SettingsPage::ALL {
        host.act_on("me:settings");
        host.act_on(&format!("us:page={}", page.key()));
        sweep.look(&mut host, &format!("settings: {}", page.key()));
    }
    host.act_on("us:close");

    // A server the account owns: its settings, every page.
    let lab = host.server_called("Laboratorio limn");
    host.act_on(&format!("ss:open:{lab}"));
    for page in [
        "profile",
        "members",
        "roles",
        "invites",
        "emojis",
        "stickers",
        "audit",
        "bans",
        "integrations",
        "safety",
        "channels",
        // Rules and onboarding (2026-10-18).
        "onboarding",
        // AutoMod (automod).
        "automod",
    ] {
        host.act_on(&format!("ss:page:{page}"));
        // The onboarding's and AutoMod's pages show what the simulation answers.
        let read = |command: &Command| {
            matches!(
                command,
                Command::LoadOnboarding { .. } | Command::LoadAutoModRules { .. }
            )
        };
        let reads: Vec<Command> = host
            .sent
            .iter()
            .filter(|command| read(command))
            .cloned()
            .collect();
        host.sent.retain(|command| !read(command));
        for command in reads {
            for event in host.provider.manage(command) {
                host.event(event);
            }
        }
        sweep.look(&mut host, &format!("server settings: {page}"));
    }
    host.act_on("ss:close");

    // A channel's settings.
    host.act_on(&format!("cs:open:{general}"));
    sweep.look(&mut host, "channel settings");
    host.act_on("cs:page:permissions");
    sweep.look(&mut host, "channel settings: permissions");
    host.act_on("cs:advanced");
    sweep.look(&mut host, "channel settings: advanced");
    host.act_on("cs:close");

    // Menus of a channel, a server and a message.
    host.menu_of(&format!("channel:{general}"));
    sweep.look(&mut host, "channel menu");
    host.key(Key::Escape, false, false);
    let taller = host.server_called("Taller de Rust");
    host.menu_of(&format!("server:{taller}"));
    sweep.look(&mut host, "server menu");
    host.key(Key::Escape, false, false);
    let newest = host
        .store
        .timeline(general)
        .and_then(|timeline| timeline.newest())
        .map(|message| message.id);
    if let Some(message) = newest {
        host.act_on(&format!("msg:{message}:actions:more"));
        sweep.look(&mut host, "message menu");
        host.key(Key::Escape, false, false);
    }

    // Someone's profile, from the member list.
    if let Some(user) = host.someone_listed() {
        host.act_on(&format!("member:{user}"));
        sweep.look(&mut host, "profile card");
        host.key(Key::Escape, false, false);
    }

    // The account's own menus, the pickers and the inbox.
    for (id, screen) in [
        ("me:status", "status menu"),
        ("composer:emoji", "emoji picker"),
        ("inbox:open", "inbox"),
        ("inbox:mentions", "inbox: mentions"),
    ] {
        host.act_on(id);
        sweep.look(&mut host, screen);
        host.key(Key::Escape, false, false);
    }

    // Threads of a busy channel, a voice room joined, a new channel.
    let gallery = host.open_channel("galería");
    host.act_on(&format!("th:list:{gallery}"));
    sweep.look(&mut host, "thread list");
    host.act_on("th:close:close");
    let room = host.channel_named("Sala común");
    host.act_on(&format!("voice:{room}"));
    sweep.look(&mut host, "voice room");
    host.act_on(&format!("newchan:{lab}"));
    sweep.look(&mut host, "new channel");
    host.key(Key::Escape, false, false);

    // Searching the open conversation.
    host.open_channel("general");
    host.key(Key::F, false, true);
    sweep.look(&mut host, "search");
    host.type_text("taller");
    host.key(Key::Enter, false, false);
    sweep.look(&mut host, "search: results");
    host.key(Key::Escape, false, false);

    // Home: friends, direct messages, and joining by invitation.
    host.act_on("home");
    sweep.look(&mut host, "home");
    host.act_on("invitation:input-open");
    sweep.look(&mut host, "invitation");

    assert!(sweep.screens >= 39, "screens seen: {}", sweep.screens);
    assert!(
        sweep.found.is_empty(),
        "{} Spanish texts in English:\n{}",
        sweep.found.len(),
        sweep
            .found
            .iter()
            .map(|(text, screen)| format!("{screen}: {text:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The language changes in the next frame, without starting again.
#[test]
fn choosing_english_shows_at_once_and_spanish_comes_back() {
    let mut host = Host::new();
    host.act_on("me:settings");
    host.act_on("us:page=language");
    assert!(host.painted().iter().any(|text| text == "Idioma y hora"));
    host.act_on("us:lang=en");
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "Language and time"),
        "{painted:?}"
    );
    assert!(!painted.iter().any(|text| text == "Idioma y hora"));
    host.act_on("us:lang=es");
    assert!(host.painted().iter().any(|text| text == "Idioma y hora"));
    rune_i18n::set_thread(None);
}
