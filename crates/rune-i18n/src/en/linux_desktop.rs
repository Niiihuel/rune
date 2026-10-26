//! Linux desktop (escritorio-linux): replacing a file in «Guardar como»,
//! the folder of the call's sounds chosen in the system's dialog, and the
//! window's decorations and GPU (`rune-ui`, `rune-core`, `rune-platform`).

pub(super) const ENTRIES: super::Entries = &[
    // «Guardar como» over a file already there.
    (
        "Ya hay un archivo con ese nombre",
        "There's already a file with that name",
    ),
    (
        "Reemplazarlo borra el que está ahí. Cancela para elegir otro nombre.",
        "Replacing it deletes the one that's there. Cancel to choose another name.",
    ),
    ("Reemplazar", "Replace"),
    (
        "No se pudo reemplazar el archivo en esa carpeta.",
        "Couldn't replace the file in that folder.",
    ),
    (
        "El archivo cambió mientras se descargaba. Vuelve a guardarlo.",
        "The file changed while downloading. Save it again.",
    ),
    // The folder of sounds.
    ("Elegir una carpeta", "Choose a folder"),
    ("Elegir…", "Choose…"),
    ("Eligiendo…", "Choosing…"),
    (
        "RUNE no puede guardar el nombre de esa carpeta. Elige otra.",
        "RUNE can't keep that folder's name. Choose another one.",
    ),
    // The window and its GPU.
    // Who draws the title bar (barra-e-icono).
    (
        "barra de título decoraciones de la ventana bordes del sistema sin marco minimizar maximizar cerrar",
        "title bar window decorations system borders frameless minimize maximize close",
    ),
    ("Barra de título", "Title bar"),
    ("Automática", "Automatic"),
    ("La de RUNE", "RUNE's"),
    ("La del sistema", "The system's"),
    (
        "Automática: la de RUNE, salvo en Wayland, donde usa la del sistema.",
        "Automatic: RUNE's, except on Wayland, where it uses the system's.",
    ),
    ("Gráficos", "Graphics"),
    (
        "dibujar con gpu placa de video gráficos integrada dedicada aceleración por hardware",
        "draw with render gpu graphics card integrated discrete dedicated hardware acceleration",
    ),
    ("Dibujar con", "Draw with"),
    ("Automático", "Automatic"),
    (
        "Ahora dibuja con {gpu}. Se aplica la próxima vez que abras RUNE.",
        "Drawing with {gpu} now. Takes effect the next time you open RUNE.",
    ),
    (
        "Se aplica la próxima vez que abras RUNE.",
        "Takes effect the next time you open RUNE.",
    ),
    (
        "La GPU elegida no pudo abrir la ventana: esta vez dibuja cualquiera.",
        "The chosen GPU couldn't open the window: any GPU draws it this time.",
    ),
    (
        "La GPU que eligió RUNE no pudo abrir la ventana: esta vez dibuja cualquiera.",
        "The GPU RUNE chose couldn't open the window: any GPU draws it this time.",
    ),
    (
        "La variable {name} elige la GPU mientras esté.",
        "The {name} variable chooses the GPU while it's set.",
    ),
];
