//! Windows (windows-base): what the desktop's settings say is different on
//! Windows or not there yet (`rune-ui`).

pub(super) const ENTRIES: super::Entries = &[
    (
        "La del texto de toda la ventana. En Windows RUNE todavía no lista las fuentes del sistema: usa la suya.",
        "The font of all the window's text. On Windows RUNE doesn't list the system's fonts yet: it uses its own.",
    ),
    (
        "El área de notificación de Windows no está disponible: cerrar sale de RUNE.",
        "Windows' notification area isn't available: closing quits RUNE.",
    ),
    (
        "Agrega RUNE a las aplicaciones que Windows abre al iniciar sesión (Configuración › Aplicaciones › Inicio).",
        "Adds RUNE to the apps Windows opens when you sign in (Settings › Apps › Startup).",
    ),
    (
        "En Windows RUNE todavía no abre el canal local discord-ipc.",
        "On Windows RUNE doesn't open the local discord-ipc channel yet.",
    ),
    (
        "En Windows RUNE todavía no detecta juegos.",
        "On Windows RUNE doesn't detect games yet.",
    ),
];
