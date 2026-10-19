//! Why an invitation was refused, as the service says it, and the way out
//! through the browser (invitaciones-motivo: `rune-provider`, `rune-ui`).

pub(super) const ENTRIES: super::Entries = &[
    (
        "Esta cuenta ya tiene lo que da la invitación.",
        "This account already has what the invite gives.",
    ),
    (
        "Llegaste al máximo de servidores: salí de uno para entrar.",
        "You're at the server limit: leave one to join.",
    ),
    (
        "Te banearon de este servidor.",
        "You're banned from this server.",
    ),
    ("El servidor está lleno.", "The server is full."),
    (
        "El servidor pausó sus invitaciones.",
        "The server paused its invites.",
    ),
    (
        "Este servidor no está disponible en tu región.",
        "This server isn't available in your region.",
    ),
    (
        "El servicio no deja entrar a este servidor por la edad de la cuenta.",
        "The service doesn't let this account join this server because of its age.",
    ),
    (
        "Las funciones de este servidor están limitadas: no admite ingresos ahora.",
        "This server's features are limited: it isn't taking new members now.",
    ),
    (
        "El servicio limitó esta cuenta y no la deja entrar a servidores.",
        "The service limited this account and doesn't let it join servers.",
    ),
    ("Ya estás en este grupo.", "You're already in this group."),
    ("Ya son amigos.", "You're already friends."),
    (
        "Ya sos miembro de este servidor.",
        "You're already a member of this server.",
    ),
    ("Abrir en el navegador", "Open in browser"),
    (
        "Se abre con la cuenta de tu navegador, no con la de RUNE.",
        "It opens with your browser's account, not RUNE's.",
    ),
];
