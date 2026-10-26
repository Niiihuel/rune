//! Emoji and states (2026-10-18): status lines, emoji details, jumbo
//! emoji, other servers' expressions and paged GIFs.

pub(super) const ENTRIES: super::Entries = &[
    // Other servers' emoji and stickers (`rune-state`, settings, composer).
    ("Usar stickers externos", "Use external stickers"),
    (
        "Permite enviar stickers de otros servidores.",
        "Allows sending stickers from other servers.",
    ),
    (
        "Deja enviar en este canal stickers de otros servidores.",
        "Allows sending stickers from other servers in this channel.",
    ),
    (
        "Deja enviar en estos canales stickers de otros servidores.",
        "Allows sending stickers from other servers in these channels.",
    ),
    (
        "usar emojis de otros servidores",
        "use emoji from other servers",
    ),
    (
        "usar stickers de otros servidores",
        "use stickers from other servers",
    ),
    (
        "no tienes permiso para usar stickers de otros servidores aquí",
        "you are not allowed to use stickers from other servers here",
    ),
    (
        "No se puede enviar el mensaje",
        "The message cannot be sent",
    ),
    (
        "No se puede editar el mensaje",
        "The message cannot be edited",
    ),
    // An emoji's card (`emoji_card.rs`).
    ("Un emoji de un servidor.", "A server's emoji."),
    ("De {server}", "From {server}"),
    (
        "Su servidor no está en esta sesión.",
        "Its server is not in this session.",
    ),
    (
        "Un emoji estándar: se puede usar en cualquier conversación.",
        "A standard emoji: it can be used in any conversation.",
    ),
    ("Copiar emoji", "Copy emoji"),
    ("Detalle del emoji {name}", "Details of the emoji {name}"),
];
