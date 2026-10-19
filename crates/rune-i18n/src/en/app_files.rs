//! Files in apps' modals (comandos-de-apps): the field, its limits, and why
//! they did not go.

pub(super) const ENTRIES: super::Entries = &[
    // rune-model/src/interactions.rs
    ("Adjuntá un archivo", "Attach a file"),
    (
        "Adjuntá al menos {min} archivos",
        "Attach at least {min} files",
    ),
    (
        "Adjuntá como mucho {max} archivos",
        "Attach at most {max} files",
    ),
    // rune-components/src/interactions.rs
    ("Elegir archivos", "Choose files"),
    ("Un archivo", "One file"),
    ("Hasta {max} archivos", "Up to {max} files"),
    ("Tipos permitidos: {kinds}", "Allowed types: {kinds}"),
    ("imágenes", "images"),
    ("videos", "videos"),
    ("audio", "audio"),
    // rune-discord-session/src/interactions.rs
    (
        "Los archivos no coinciden con el formulario: no se envió nada.",
        "The files don't match the form: nothing was sent.",
    ),
    (
        "No tenés permiso para adjuntar archivos en el canal.",
        "You don't have permission to attach files in the channel.",
    ),
    (
        "El formulario supera los archivos o el tamaño permitidos.",
        "The form exceeds the allowed files or size.",
    ),
    (
        "Un archivo no es de un tipo que acepte el formulario: no se envió nada.",
        "A file isn't of a type the form accepts: nothing was sent.",
    ),
    // rune-provider/src/mock/interactions.rs
    (
        "Los archivos no coinciden con el formulario (simulado).",
        "The files don't match the form (simulated).",
    ),
    (
        "No tenés permiso para adjuntar archivos (simulado).",
        "You don't have permission to attach files (simulated).",
    ),
    // rune-state/src/interactions.rs
    (
        "No tenés permiso para adjuntar archivos aquí",
        "You don't have permission to attach files here",
    ),
    // rune-ui/src/interactions.rs
    (
        "{name} no es de un tipo que acepte este campo",
        "{name} isn't of a type this field accepts",
    ),
    (
        "Los archivos ya se están enviando",
        "The files are already being sent",
    ),
    (
        "Esperá la respuesta de la aplicación.",
        "Wait for the app's answer.",
    ),
];
