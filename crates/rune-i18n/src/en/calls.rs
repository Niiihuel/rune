//! Private calls (2026-10-17): calls in direct and group conversations,
//! across the provider, the state, the components and the interface.

pub(super) const ENTRIES: super::Entries = &[
    // Provider and state.
    ("hacer sonar la llamada", "ring the call"),
    ("hacer llamadas", "make calls"),
    (
        "Bloqueaste a {name}: desbloqueala para llamarle",
        "You blocked {name}: unblock them to call them",
    ),
    (
        "Entrá a la llamada para hacerla sonar.",
        "Join the call to ring it.",
    ),
    (
        "La llamada dejó de estar disponible en Discord.",
        "The call stopped being available on Discord.",
    ),
    // Components and interface.
    ("Llamada entrante…", "Incoming call…"),
    ("Llamada simulada entrante…", "Simulated incoming call…"),
    ("Llamada simulada", "Simulated call"),
    ("Llamando…", "Ringing…"),
    ("Volver a llamar", "Ring again"),
    ("Dejar de llamar", "Stop ringing"),
    ("Iniciar llamada", "Start call"),
    ("Atender la llamada", "Answer call"),
    ("Unirse a la llamada", "Join call"),
    ("Estás en la llamada", "You're in the call"),
    ("No se puede llamar", "Can't call"),
    ("La llamada sigue sin nadie adentro", "The call goes on with nobody in it"),
    ("{one} está en la llamada", "{one} is in the call"),
    ("{one} y {two} están en la llamada", "{one} and {two} are in the call"),
    ("{count} personas están en la llamada", "{count} people are in the call"),
    ("Sin respuesta", "No answer"),
    ("{name} rechazó la llamada", "{name} declined the call"),
    ("Rechazaron la llamada", "They declined the call"),
    ("{name} cortó", "{name} hung up"),
    ("Todos salieron de la llamada", "Everyone left the call"),
    ("Esperando a que alguien entre", "Waiting for someone to join"),
    ("{name} te está llamando", "{name} is calling you"),
    (
        "Estás en esta llamada desde otro dispositivo",
        "You're in this call on another device",
    ),
    ("Unirse desde acá", "Join from here"),
    ("Llamada entrante", "Incoming call"),
    ("Llamando a alguien", "Calling someone"),
    // The simulation's tests.
    ("Recibir una llamada", "Receive a call"),
    (
        "La persona de tu primer privado te llama: el aviso, el sonido y la notificación.",
        "The person of your first direct message calls you: the notice, the sound and the notification.",
    ),
    ("Llamar", "Call"),
    ("Atenderla en otro dispositivo", "Answer it on another device"),
    (
        "La llamada que suena sigue en otro de tus dispositivos.",
        "The ringing call goes on on another of your devices.",
    ),
    ("Atender", "Answer"),
    ("recibir una llamada entrante sonar", "receive an incoming call ring"),
    ("atender en otro dispositivo llamada", "answer on another device call"),
    // The call's sounds in the settings.
    (
        "sonido llamada entrante timbre sonar privado",
        "sound incoming call ringtone ring direct message",
    ),
    (
        "sonido llamando a alguien timbre esperar privado",
        "sound calling someone ringtone wait direct message",
    ),
];
