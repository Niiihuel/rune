//! Channels and voice administration (admin-canales-y-voz): moving channels
//! into another category from the column (`rune-components/src/channel_menu.rs`)
//! and the simulation's answer to it.

pub(super) const ENTRIES: super::Entries = &[
    (
        "{name} pasa a {category}. ¿Quieres que tome los permisos de la categoría? Sus permisos propios se reemplazan.",
        "{name} is moving to {category}. Should it take the category's permissions? Its own permissions are replaced.",
    ),
    ("Mover sin sincronizar", "Move without syncing"),
    (
        "solo se sincroniza con una categoría",
        "it can only sync with a category",
    ),
];
