//! The «Activo ahora» column of the friends page (2026-10-31). The texts it
//! reuses from other zones (Escuchando {name}, Enviar mensaje…) stay there.

pub(super) const ENTRIES: super::Entries = &[
    ("Activo ahora", "Active Now"),
    ("No hay nadie, por ahora...", "It's quiet for now..."),
    (
        "¡Cuando un amigo inicie una actividad, como jugar o pasar el rato en el chat de voz, lo podrás ver aquí!",
        "When a friend starts an activity—like playing a game or hanging out on voice—we'll show it here!",
    ),
    ("{first} y {second}", "{first} and {second}"),
    ("{first}, {second} y {n} otro", "{first}, {second}, and {n} other"),
    ("{first}, {second} y {n} otros", "{first}, {second}, and {n} others"),
    ("{name} – {n} min", "{name} – {n}m"),
    ("{name} – {n} h", "{name} – {n}h"),
    ("{name} – {n} d", "{name} – {n}d"),
    ("{name} - justo ahora", "{name} - just now"),
    ("Escuchando Spotify", "Listening to Spotify"),
    ("En un canal de voz", "In a Voice Channel"),
    ("Jugando varios juegos", "Playing Multiple Games"),
    ("Transcurrido: {duration}", "{duration} elapsed"),
    ("Restantes: {duration}", "{duration} left"),
    ("{n} persona", "{n} person"),
    ("{n} personas", "{n} people"),
    ("Compartiendo su pantalla", "Sharing their screen"),
    ("Mirar transmisión de {name}", "Watch {name}'s Stream"),
    ("Unirse al canal de voz", "Join Voice Channel"),
];
