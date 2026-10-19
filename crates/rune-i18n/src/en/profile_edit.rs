//! Profile editing (edicion-de-perfil): framing a chosen picture
//! (`rune-ui/src/framing.rs`, `rune-components/src/profile.rs`), moving
//! pictures (`rune-core/src/profile_images.rs`) and what the plan allows
//! (`rune-state/src/profile_edit.rs`).

pub(super) const ENTRIES: super::Entries = &[
    // What the plan allows, said as «Necesitás {upgrade} para usar {what}».
    ("un banner", "a banner"),
    ("un avatar animado", "an animated avatar"),
    (
        "un perfil propio en cada servidor",
        "a profile of your own in each server",
    ),
    // Framing.
    ("Encuadre", "Framing"),
    ("Zoom", "Zoom"),
    ("Zoom del encuadre", "Framing zoom"),
    (
        "Arrastrá sobre la imagen para elegir qué parte se ve; el zoom la acerca.",
        "Drag over the image to choose which part shows; zoom brings it closer.",
    ),
    (
        "Se mueve: se sube como GIF, con el mismo encuadre en cada cuadro.",
        "It moves: it's uploaded as a GIF, with the same framing on every frame.",
    ),
    (
        "{reason}. Se usa sólo el primer cuadro.",
        "{reason}. Only the first frame is used.",
    ),
    (
        "{reason}. Mové el encuadre para usar sólo el primer cuadro.",
        "{reason}. Move the framing to use only the first frame.",
    ),
    // Moving pictures.
    ("Elegí la imagen otra vez.", "Choose the image again."),
    (
        "La animación es demasiado larga o pesada para un perfil.",
        "The animation is too long or too heavy for a profile.",
    ),
];
