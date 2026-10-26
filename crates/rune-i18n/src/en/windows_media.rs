//! Windows' media and desktop (windows-medios): the screen picker of RUNE,
//! the camera, the screen's sound, games, discord-ipc and the system's
//! fonts on Windows (`rune-ui`, `rune-media`).

pub(super) const ENTRIES: super::Entries = &[
    (
        "La del texto de toda la ventana. Las fuentes del sistema se leen con DirectWrite; si una se desinstala, RUNE vuelve a la suya.",
        "The font of all the window's text. System fonts are read with DirectWrite; if one is uninstalled, RUNE goes back to its own.",
    ),
    (
        "Otros programas de este equipo (juegos, reproductores) pueden decir qué estás haciendo por la tubería local discord-ipc, como con el cliente oficial; RUNE lo publica como tu actividad. Sólo los programas de tu sesión de Windows, sin puertos de red.",
        "Other programs on this device (games, players) can say what you're doing through the local discord-ipc pipe, as with the official client; RUNE publishes it as your activity. Only your Windows session's programs, with no network ports.",
    ),
    // RUNE's own screen picker and the screen (rune-media, rune-components).
    ("Pantallas", "Screens"),
    ("Ventanas", "Windows"),
    ("Calidad", "Quality"),
    (
        "Pantalla {number} (principal) · {width}×{height}",
        "Screen {number} (main) · {width}×{height}",
    ),
    ("Pantalla {number} · {width}×{height}", "Screen {number} · {width}×{height}"),
    (
        "Esta versión de Windows no permite capturar la pantalla: hace falta Windows 10 (1903) o posterior.",
        "This version of Windows can't capture the screen: it takes Windows 10 (1903) or later.",
    ),
    ("La ventana elegida ya no está abierta.", "The chosen window isn't open anymore."),
    (
        "El sonido de la pantalla necesita Windows 10 (versión 2004) o posterior.",
        "The screen's sound needs Windows 10 (version 2004) or later.",
    ),
    // The camera (rune-media).
    (
        "Windows no deja que las aplicaciones usen la cámara: permítelo en Configuración › Privacidad y seguridad › Cámara.",
        "Windows doesn't let apps use the camera: allow it in Settings › Privacy & security › Camera.",
    ),
    ("La cámara está en uso por otro programa.", "The camera is in use by another program."),
    (
        "La cámara no ofrece imágenes que RUNE pueda leer.",
        "The camera doesn't offer pictures RUNE can read.",
    ),
    (
        "Este Windows no tiene Media Foundation (en las ediciones N, instala el Media Feature Pack).",
        "This Windows doesn't have Media Foundation (on N editions, install the Media Feature Pack).",
    ),
    // Video settings (rune-ui).
    (
        "En Windows RUNE todavía codifica y decodifica el video con OpenH264, en el procesador.",
        "On Windows RUNE still encodes and decodes video with OpenH264, on the processor.",
    ),
];
