//! Message content: forwards, records of calls, explicit pictures and
//! copying pictures and videos (contenido-de-mensajes).

pub(super) const ENTRIES: super::Entries = &[
    // rune-components message.rs
    (
        "Te perdiste una llamada de {who} que duró {duration}.",
        "You missed a call from {who} that lasted {duration}.",
    ),
    ("Te perdiste una llamada de {who}.", "You missed a call from {who}."),
    (
        "{who} inició una llamada que duró {duration}.",
        "{who} started a call that lasted {duration}.",
    ),
    // rune-components message/attachments.rs and message/embeds.rs
    ("CONTENIDO EXPLÍCITO", "EXPLICIT CONTENT"),
    ("Mostrar contenido explícito: {name}", "Show explicit content: {name}"),
    ("Mostrar contenido explícito", "Show explicit content"),
    ("Copiar imagen", "Copy image"),
    ("Copiar video", "Copy video"),
    // rune-ui account.rs
    (
        "RUNE difumina las imágenes que el servicio marca como explícitas, y un clic las muestra; bloquear lo hace el servicio, y lo que llega igual se difumina. Sin elegir: se ven en servidores y de amigos, y se difuminan las de los demás.",
        "RUNE blurs the images the service marks as explicit, and a click shows them; blocking is up to the service, and whatever arrives is blurred too. If you don't choose: they're shown in servers and from friends, and blurred from everyone else.",
    ),
    // rune-platform clipboard.rs
    (
        "No hay un portapapeles disponible en esta sesión.",
        "No clipboard is available in this session.",
    ),
    (
        "En Wayland, RUNE copia imágenes y videos con su ventana propia del portapapeles de XWayland, y XWayland no está activo en esta sesión. Guardá el archivo y copialo desde ahí.",
        "On Wayland, RUNE copies images and videos with its own XWayland clipboard window, and XWayland isn't running in this session. Save the file and copy it from there.",
    ),
    ("El portapapeles no aceptó la copia.", "The clipboard didn't accept the copy."),
    // rune-core files/copy.rs
    (
        "No hay una carpeta privada para el archivo copiado.",
        "There's no private folder for the copied file.",
    ),
    ("No se pudo copiar el archivo.", "Couldn't copy the file."),
    ("No se pudo leer la imagen.", "Couldn't read the image."),
    (
        "No se pudo copiar la imagen: es demasiado grande, está dañada o no es una imagen.",
        "Couldn't copy the image: it's too large, damaged or not an image.",
    ),
    ("No se pudo preparar el archivo copiado.", "Couldn't prepare the copied file."),
    // rune-provider mock/showcase.rs
    (
        "(simuladas): una no llega nunca y la otra está marcada como explícita.",
        "(simulated): one never arrives and the other is marked as explicit.",
    ),
    ("dejó todo listo (copia simulada)", "left everything ready (simulated copy)"),
    // apps/desktop main.rs
    ("Copiando…", "Copying…"),
    ("No se pudo copiar", "Couldn't copy"),
    ("Imagen copiada", "Image copied"),
    ("Video copiado", "Video copied"),
];
