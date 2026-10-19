//! Calls and the voice panel (2026-10-18, llamadas-y-panel-de-voz): asking
//! whether a direct call may ring and the dialog when it may not, the
//! answer buttons of a call's notification, and the camera's panel of the
//! call's bar.

pub(super) const ENTRIES: super::Entries = &[
    // Asking whether a direct call may ring.
    ("preparar la llamada", "set up the call"),
    (
        "Llamada simulada: no son amigos.",
        "Simulated call: you're not friends.",
    ),
    ("La llamada no va a sonar", "The call won't ring"),
    (
        "{name} no recibe el aviso, pero puede entrar desde la conversación.",
        "{name} won't be notified, but can join from the conversation.",
    ),
    ("No se puede llamar a {name}", "You can't call {name}"),
    (
        "Solo se puede llamar a amigos.",
        "You can only call friends.",
    ),
    ("Ahora no", "Not now"),
    ("Enviar solicitud de amistad", "Send friend request"),
    ("Aceptar solicitud de amistad", "Accept friend request"),
    (
        "Ya le enviaste una solicitud de amistad.",
        "You already sent them a friend request.",
    ),
    // The camera's panel.
    ("Opciones de cámara", "Camera options"),
    ("Ajustes de cámara", "Camera settings"),
];
