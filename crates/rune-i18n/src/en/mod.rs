//! The English of every interface text, keyed by its Spanish, one file per
//! area of the code. A text used in several areas may be in each of their
//! files, always with the same English.

type Entries = &'static [(&'static str, &'static str)];

mod common;
mod components_channels;
mod components_expressions;
mod components_messages;
mod components_people;
mod components_servers;
mod desktop;
mod provider;
mod session;
mod state_core;
mod ui_account;
mod ui_admin;
mod ui_chat;
mod ui_server_settings;
mod ui_settings;
mod ui_social;
// Private calls.
mod calls;
// Serein-style voice (2026-10-18).
mod voice_panel;
// Message content (contenido-de-mensajes).
mod message_content;
// Files in apps' modals (comandos-de-apps).
mod app_files;
// Emoji and states (2026-10-18).
mod emoji_states;
// Video and documents (2026-10-18).
mod video_documents;
// Profile editing (edicion-de-perfil).
mod profile_edit;
// Voice protocol (voz-protocolo).
mod voice_protocol;
// Calls and the voice panel (llamadas-y-panel-de-voz).
mod calls_panel;
// Reading settings and pictures on disk (imagenes-y-lectura).
mod reading;
// Profile card and cosmetics (2026-10-18).
mod profile_card;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Linux desktop (escritorio-linux).
mod linux_desktop;
// Threads, forums and stages (2026-10-18).
mod threads_stages;
// Channels and voice administration (admin-canales-y-voz).
mod admin_channels_voice;
// Desktop session and games (escritorio-sesion-y-juegos).
mod desktop_session;
// Editable shortcuts (atajos-y-navegacion).
mod shortcuts;
// Own themes, main color and transparency (temas-propios).
mod themes;
// Server administration and members (admin-servidor-y-miembros).
mod admin_server_members;
// Why an invitation was refused (invitaciones-motivo).
mod invitation_refusals;
// Scheduled events (eventos-programados).
mod events;
// AutoMod (automod).
mod automod;
// RUNE's own updates (actualizaciones).
mod updates;
// Windows (windows-base).
mod windows;

/// Each file's entries, by the file's name.
pub(crate) const ALL: &[(&str, Entries)] = &[
    ("common", common::ENTRIES),
    ("components_messages", components_messages::ENTRIES),
    ("components_expressions", components_expressions::ENTRIES),
    ("components_channels", components_channels::ENTRIES),
    ("components_servers", components_servers::ENTRIES),
    ("components_people", components_people::ENTRIES),
    ("ui_settings", ui_settings::ENTRIES),
    ("ui_account", ui_account::ENTRIES),
    ("ui_server_settings", ui_server_settings::ENTRIES),
    ("ui_admin", ui_admin::ENTRIES),
    ("ui_chat", ui_chat::ENTRIES),
    ("ui_social", ui_social::ENTRIES),
    ("provider", provider::ENTRIES),
    ("state_core", state_core::ENTRIES),
    ("session", session::ENTRIES),
    ("desktop", desktop::ENTRIES),
    ("calls", calls::ENTRIES),
    ("voice_panel", voice_panel::ENTRIES),
    ("message_content", message_content::ENTRIES),
    ("app_files", app_files::ENTRIES),
    ("emoji_states", emoji_states::ENTRIES),
    ("video_documents", video_documents::ENTRIES),
    ("profile_edit", profile_edit::ENTRIES),
    ("voice_protocol", voice_protocol::ENTRIES),
    ("calls_panel", calls_panel::ENTRIES),
    ("reading", reading::ENTRIES),
    ("profile_card", profile_card::ENTRIES),
    ("onboarding", onboarding::ENTRIES),
    ("linux_desktop", linux_desktop::ENTRIES),
    ("threads_stages", threads_stages::ENTRIES),
    ("admin_channels_voice", admin_channels_voice::ENTRIES),
    ("desktop_session", desktop_session::ENTRIES),
    ("shortcuts", shortcuts::ENTRIES),
    ("themes", themes::ENTRIES),
    ("admin_server_members", admin_server_members::ENTRIES),
    ("invitation_refusals", invitation_refusals::ENTRIES),
    // Scheduled events (eventos-programados).
    ("events", events::ENTRIES),
    // AutoMod (automod).
    ("automod", automod::ENTRIES),
    ("updates", updates::ENTRIES),
    ("windows", windows::ENTRIES),
];
