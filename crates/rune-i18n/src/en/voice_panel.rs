//! Serein-style voice (2026-10-18): the quick voice panel of the arrows
//! (`rune-components/src/quick_voice.rs`, `rune-ui/src/quick_voice.rs`) and
//! the voice page's devices, levels and microphone test.

pub(super) const ENTRIES: super::Entries = &[
    // The quick panel.
    ("Ajustes de entrada", "Input settings"),
    ("Ajustes de salida", "Output settings"),
    ("Todos los ajustes de voz", "All voice settings"),
    ("Dispositivo no disponible", "Device unavailable"),
    (
        "El micrófono elegido no está conectado. Conectalo o elegí otro.",
        "The chosen microphone isn't connected. Connect it or pick another.",
    ),
    (
        "La salida elegida no está conectada. Conectala o elegí otra.",
        "The chosen output isn't connected. Connect it or pick another.",
    ),
    (
        "100 % es el nivel original; más alto puede distorsionar.",
        "100 % is the original level; higher may distort.",
    ),
    ("El nivel se ve mientras probás.", "The level shows while you test."),
    ("Probar micrófono", "Test microphone"),
    ("Actualizar dispositivos", "Refresh devices"),
    ("Restablecer niveles", "Reset levels"),
    (
        "Ensordecer apaga lo que oís y también tu micrófono.",
        "Deafening turns off what you hear and your microphone too.",
    ),
    ("{level} dBFS · umbral {threshold} dBFS", "{level} dBFS · threshold {threshold} dBFS"),
    ("{level} dBFS", "{level} dBFS"),
    (
        "La voz es simulada: no se reproduce audio de la llamada. «Probar sonido» suena igual.",
        "Voice is simulated: no call audio plays. “Test sound” still plays.",
    ),
    (
        "«Probar sonido» toca el sonido de entrar a la llamada, también durante una.",
        "“Test sound” plays the join sound, during a call too.",
    ),
    // The voice page.
    ("Dispositivos y niveles", "Devices and levels"),
    ("Cámara y video", "Camera and video"),
    (
        "micrófono altavoz dispositivo de entrada salida auriculares parlantes volumen ganancia actualizar restablecer niveles",
        "microphone speaker input output device headphones speakers volume gain refresh reset levels",
    ),
    (
        "probar sonido prueba de salida altavoz auriculares sonido de la llamada",
        "test sound output test speaker headphones call sound",
    ),
    (
        "En una llamada la salida es suya: podés escucharte de nuevo al colgar.",
        "In a call the output is the call's: you can hear yourself again after hanging up.",
    ),
    (
        "Con el volumen de salida en 0 % no se oye.",
        "With the output volume at 0 % it can't be heard.",
    ),
    (
        "Suena el sonido de entrar a la llamada, mezclado con ella.",
        "The join sound plays, mixed into the call.",
    ),
    (
        "Suena el sonido de entrar a la llamada en la salida elegida.",
        "The join sound plays on the chosen output.",
    ),
    ("Empezar la prueba", "Start test"),
    ("Detener la prueba", "Stop test"),
    (
        "Empezá la prueba y decí algo: la barra muestra tu nivel, y la marca, dónde empieza a contar como voz. Es local; no se envía nada.",
        "Start the test and say something: the bar shows your level, and the mark where it starts counting as voice. It's local; nothing is sent.",
    ),
    (
        "En la llamada mide el mismo micrófono que usa ella, sin cambiar lo que se envía.",
        "In a call it measures the call's own microphone, without changing what is sent.",
    ),
    ("Antes y después del procesamiento", "Before and after processing"),
];
