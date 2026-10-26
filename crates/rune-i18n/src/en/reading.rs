//! Reading settings and pictures on disk (imagenes-y-lectura): the
//! «Lectura» and «Caché de imágenes» sections of the settings
//! (`rune-ui/src/settings/pages/reading.rs` and `image_cache.rs`).

pub(super) const ENTRIES: super::Entries = &[
    // Reading.
    ("Lectura", "Reading"),
    (
        "animar gif animaciones reproducir solos automáticamente pasar el puntero stickers emoji",
        "animate gif animations play by themselves automatically hover pointer stickers emoji",
    ),
    (
        "ocultar los enlaces de imagen y gif vista previa enlace solo url",
        "hide image and gif links preview standalone link url",
    ),
    (
        "velocidad de desplazamiento rueda del mouse ratón trackpad panel táctil scroll",
        "scrolling speed mouse wheel trackpad touchpad scroll",
    ),
    (
        "mantener la lista de miembros en ventanas angostas personas panel chica",
        "keep the member list in narrow windows people panel small",
    ),
    ("Animar GIF", "Animate GIFs"),
    (
        "Con el movimiento reducido no se anima nada, ni al pasar el puntero.",
        "With reduced motion nothing animates, not even under the pointer.",
    ),
    (
        "Los GIF, stickers y emoji animados se mueven solos. Apagado, muestran su primer cuadro y se animan al pasar el puntero.",
        "GIFs, stickers and animated emoji move by themselves. Off, they show their first frame and animate under the pointer.",
    ),
    (
        "Ocultar los enlaces de imagen y GIF",
        "Hide image and GIF links",
    ),
    (
        "Si un mensaje es sólo el enlace de una imagen o un GIF que se muestra, se ve la imagen sin el enlace.",
        "When a message is only the link of an image or GIF that shows, you see the image without the link.",
    ),
    ("Velocidad de desplazamiento", "Scrolling speed"),
    (
        "Rueda del mouse y trackpad. 100 % es lo normal.",
        "Mouse wheel and trackpad. 100% is the default.",
    ),
    (
        "Mantener la lista de miembros en ventanas angostas",
        "Keep the member list in narrow windows",
    ),
    (
        "La lista de miembros de un servidor queda abierta aunque la ventana sea angosta: la conversación se angosta en su lugar.",
        "A server's member list stays open even in a narrow window: the conversation narrows instead.",
    ),
    // The image cache.
    ("Caché de imágenes", "Image cache"),
    (
        "caché de medios vistas previas banners imágenes disco espacio borrar",
        "media cache previews banners images disk space clear delete",
    ),
    (
        "caché de arte compartido emoji avatares íconos insignias stickers disco borrar",
        "shared artwork cache emoji avatars icons badges stickers disk clear delete",
    ),
    (
        "borrar las imágenes sin usar después de días retención caché",
        "remove unused images after days retention cache",
    ),
    ("Caché de medios", "Media cache"),
    (
        "Vistas previas de adjuntos y enlaces, banners e imágenes a tamaño original.",
        "Previews of attachments and links, banners and full-size images.",
    ),
    ("Caché de arte compartido", "Shared artwork cache"),
    (
        "Avatares, emoji, stickers, íconos e insignias, aparte para que los medios grandes no los saquen.",
        "Avatars, emoji, stickers, icons and badges, kept apart so large media never pushes them out.",
    ),
    ("Usa {used} de {limit}.", "{used} of {limit} used."),
    (
        "Sin almacén seguro del sistema, las imágenes quedan solo en memoria.",
        "Without the system's secure store, images stay only in memory.",
    ),
    ("{count} días", "{count} days"),
    (
        "Borrar las imágenes sin usar después de",
        "Remove unused images after",
    ),
    (
        "Las imágenes que no se vuelven a mostrar en ese tiempo se borran del disco. Borrar estas imágenes no toca los mensajes, los borradores ni tu sesión.",
        "Cached images that are not shown again for this long are deleted from disk. Deleting these images leaves messages, drafts and your login alone.",
    ),
];
