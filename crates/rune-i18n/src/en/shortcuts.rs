//! Editable shortcuts and the history's arrows (atajos-y-navegacion): the
//! table of `rune-ui/src/keybinds.rs`, its page in the settings
//! (`settings/pages/shortcuts.rs`), its rows (`rune-components/src/keybinds.rs`),
//! the list of shortcuts and the header's arrows.

pub(super) const ENTRIES: super::Entries = &[
    ("Adelante", "Forward"),
    ("Adelante en el historial", "Forward in history"),
    ("Atrás en el historial", "Back in history"),
    ("AvPág", "PgDn"),
    ("Bloque de código", "Code block"),
    ("Canal anterior", "Previous channel"),
    ("Canal siguiente", "Next channel"),
    (
        "Cancelar la grabación de {action}",
        "Cancel recording {action}",
    ),
    ("Cursiva", "Italic"),
    ("Código", "Code"),
    (
        "Elegir en las listas y sugerencias",
        "Choose in lists and suggestions",
    ),
    ("Espacio", "Space"),
    ("Estos no se cambian.", "These do not change."),
    ("Fijos", "Fixed"),
    ("Formato", "Formatting"),
    (
        "Grabar otro atajo para {action}",
        "Record another shortcut for {action}",
    ),
    (
        "La usa la edición de texto: elige otra.",
        "Text editing uses it: choose another.",
    ),
    (
        "Los botones laterales del mouse también van atrás y adelante. Se cambian en Ajustes, Sistema.",
        "The mouse's side buttons also go back and forward. Change them in Settings, System.",
    ),
    ("Mostrar esta lista de atajos", "Show this list of shortcuts"),
    ("Negrita", "Bold"),
    ("Pulsa las teclas…", "Press the keys…"),
    ("RePág", "PgUp"),
    (
        "Región anterior de la ventana",
        "Previous region of the window",
    ),
    ("Región siguiente de la ventana", "Next region of the window"),
    (
        "Shift+F10 abre el menú contextual: elige otra.",
        "Shift+F10 opens the context menu: choose another.",
    ),
    (
        "Si Enviar deja de ser Enter, Enter escribe una línea nueva.",
        "If Send is no longer Enter, Enter starts a new line.",
    ),
    (
        "Sola escribe o mueve el cursor: súmale Ctrl o Alt.",
        "Alone it types or moves the caret: add Ctrl or Alt.",
    ),
    ("Spoiler", "Spoiler"),
    ("Subrayado", "Underline"),
    (
        "Tab mueve el foco entre los controles: elige otra.",
        "Tab moves the focus between controls: choose another.",
    ),
    ("Tachado", "Strikethrough"),
    ("Ya lo usa «{action}».", "Already used by “{action}”."),
    (
        "atajos de teclado fijos cerrar escape esc mención tab desplazar página menú contextual zoom acercar alejar",
        "keyboard shortcuts fixed close escape esc mention tab scroll page context menu zoom in out",
    ),
    (
        "atajos de teclado formato negrita cursiva subrayado tachado código bloque spoiler ctrl",
        "keyboard shortcuts formatting bold italic underline strikethrough code block spoiler ctrl",
    ),
    (
        "atajos de teclado mensajes enviar nueva línea editar último enter shift",
        "keyboard shortcuts messages send new line edit last enter shift",
    ),
    (
        "atajos de teclado navegación cambiar reasignar buscador rápido canales historial atrás adelante región lista métricas ctrl k alt flechas f6 f12",
        "keyboard shortcuts navigation change rebind quick switcher channels history back forward region list metrics ctrl k alt arrows f6 f12",
    ),
    (
        "atajos de teclado voz pulsar para hablar silenciar ensordecer micrófono global fuera de la ventana tecla combinación push to mute ptt",
        "keyboard shortcuts voice push to talk mute deafen microphone global outside the window key combination push to mute ptt",
    ),
    ("Windows no puede usar esa tecla como atajo global.", "Windows can't use that key as a global shortcut."),
    (
        "Windows no dejó instalar el escucha de teclado y ratón para el atajo global.",
        "Windows did not let the global shortcut install its keyboard and mouse listener.",
    ),
    ("el sistema no puede dar esa tecla.", "the system can't give that key."),
];
