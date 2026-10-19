//! Threads, forums and stages (2026-10-18): silencing a thread
//! (`rune-ui/src/threads.rs`, `rune-state/src/threads.rs`, the simulation
//! and the session), a narrow header's «más» and the simulation's threads
//! taking their parent's permissions.

pub(super) const ENTRIES: super::Entries = &[
    // Silencing a thread.
    ("Silenciar hilo", "Mute thread"),
    ("Silenciar publicación", "Mute post"),
    ("Dejar de silenciar el hilo", "Unmute thread"),
    ("Dejar de silenciar la publicación", "Unmute post"),
    (
        "Este proveedor no permite silenciar hilos",
        "This provider doesn't allow muting threads",
    ),
    (
        "Seguí el hilo para silenciarlo",
        "Follow the thread to mute it",
    ),
    (
        "seguí el hilo para silenciarlo",
        "follow the thread to mute it",
    ),
    (
        "Seguí el hilo antes de silenciarlo.",
        "Follow the thread before muting it.",
    ),
    ("Elegí cuánto tiempo.", "Choose for how long."),
    // A narrow header.
    ("Más herramientas", "More tools"),
    // The simulation.
    (
        "un hilo toma los permisos de su canal",
        "a thread takes its channel's permissions",
    ),
];
