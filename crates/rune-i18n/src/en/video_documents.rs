//! Video and documents (2026-10-18): WebM and MKV videos in conversations
//! (`rune-media/src/clip`), highlights in a PDF's search
//! (`rune-ui/src/documents.rs`) and Lottie stickers.

pub(super) const ENTRIES: super::Entries = &[
    // WebM and MKV videos.
    (
        "RUNE no puede leer este archivo WebM o MKV.",
        "RUNE can't read this WebM or MKV file.",
    ),
    (
        "Este video no dice cuánto dura y no tiene sonido.",
        "This video doesn't say how long it lasts and has no sound.",
    ),
    (
        "Este equipo no tiene el decodificador de video que hace falta.",
        "This computer doesn't have the video decoder it needs.",
    ),
    (
        "Las imágenes de este video son más grandes de lo que RUNE muestra.",
        "This video's pictures are larger than RUNE shows.",
    ),
];
