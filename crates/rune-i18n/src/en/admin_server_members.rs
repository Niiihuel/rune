//! Server administration and members (admin-servidor-y-miembros): the
//! server's banner and moving icon, its own emojis in traits and a forum's
//! reaction, becoming a community, a forum's archive time and tag filter,
//! and the simulation's and the session's answers to them.

pub(super) const ENTRIES: super::Entries = &[
    ("3 días", "3 days"),
    (
        "Al filtrar, se muestran las publicaciones con todas las etiquetas elegidas, en vez de las que tienen alguna.",
        "When filtering, show the posts with every tag chosen instead of those with any of them.",
    ),
    ("Archivar después de", "Archive after"),
    ("Cambiar banner", "Change banner"),
    (
        "Canal de novedades de la comunidad",
        "Community updates channel",
    ),
    ("Canal de reglas", "Rules channel"),
    ("Como decida el servicio", "As the service decides"),
    ("Comunidad", "Community"),
    (
        "Cuánto tiempo sin actividad espera una publicación nueva antes de archivarse.",
        "How long a new post waits without activity before it is archived.",
    ),
    (
        "Donde el servicio avisa a la moderación de cambios y novedades.",
        "Where the service tells the moderators about changes and news.",
    ),
    (
        "El emoji con que empieza cada publicación nueva, para reaccionar con un clic; también «:nombre:» de uno del servidor.",
        "The emoji each new post starts with, to react in one click; also “:name:” of one of the server's own.",
    ),
    (
        "Elige el canal de reglas y el de novedades de la comunidad.",
        "Choose the community's rules channel and updates channel.",
    ),
    ("Es una comunidad", "It is a community"),
    ("Ese emoji no es de este servidor.", "That emoji is not this server's."),
    (
        "Este servidor no tiene desbloqueado el ícono animado.",
        "This server has not unlocked an animated icon.",
    ),
    (
        "Este servidor no tiene desbloqueado ese banner.",
        "This server has not unlocked that banner.",
    ),
    (
        "Este servidor todavía no tiene el banner animado desbloqueado",
        "This server has not unlocked an animated banner yet",
    ),
    (
        "Este servidor todavía no tiene el banner desbloqueado",
        "This server has not unlocked a banner yet",
    ),
    (
        "Este servidor todavía no tiene el ícono animado desbloqueado",
        "This server has not unlocked an animated icon yet",
    ),
    (
        "Falta algo de lo que una comunidad necesita: los dos canales, verificación y revisar el contenido de todos.",
        "Something a community needs is missing: both channels, verification and scanning everyone's content.",
    ),
    (
        "Filtrar por todas las etiquetas elegidas",
        "Filter by every tag chosen",
    ),
    (
        "Hasta cinco cosas que lo definen, con un emoji o «:nombre:» de uno del servidor. Se muestran en su perfil.",
        "Up to five things that define it, with an emoji or “:name:” of one of the server's own. They show on its profile.",
    ),
    ("No se guardó la comunidad", "The community was not saved"),
    ("Quitar banner", "Remove banner"),
    (
        "Sobre la lista de canales, en 16:9; se recorta al centro.",
        "Above the channel list, 16:9; cropped to the center.",
    ),
    (
        "Sobre la lista de canales. En la simulación se guarda su nombre, no la imagen.",
        "Above the channel list. The simulation keeps its name, not the picture.",
    ),
    (
        "Tiene un canal de reglas y otro donde el servicio avisa novedades a la moderación. Al activarla, la verificación sube a baja si no tenía y se revisa el contenido de todos.",
        "It has a rules channel and another where the service tells the moderators about news. Turning it on raises verification to low if there was none and scans everyone's content.",
    ),
    (
        "Una comunidad necesita al menos verificación baja y revisar el contenido de todos.",
        "A community needs at least low verification and scanning everyone's content.",
    ),
    (
        "Una comunidad necesita un canal de reglas, uno de novedades, verificación y revisar el contenido de todos.",
        "A community needs a rules channel, an updates channel, verification and scanning everyone's content.",
    ),
    (
        "activar o desactivar la comunidad",
        "turn the community on or off",
    ),
    ("cambiar el banner del servidor", "change the server's banner"),
    (
        "cambiar los canales de la comunidad",
        "change the community's channels",
    ),
    (
        "esa reacción o ese archivado no están disponibles",
        "that reaction or archive time is not available",
    ),
    ("ese emoji no es de este servidor", "that emoji is not this server's"),
    ("este servidor no tiene banner", "this server has no banner"),
    (
        "este servidor no tiene el banner animado",
        "this server has no animated banner",
    ),
    (
        "este servidor no tiene el ícono animado",
        "this server has no animated icon",
    ),
    (
        "una comunidad necesita un canal de reglas y uno de novedades",
        "a community needs a rules channel and an updates channel",
    ),
    ("{n} minutos", "{n} minutes"),
    (
        "{reason}: un GIF se usa quieto.",
        "{reason}: a GIF is used still.",
    ),
];
