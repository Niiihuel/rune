//! Desktop session and games (escritorio-sesion-y-juegos): starting in the
//! tray, the account's activity sharing, the registered games and the
//! providers' proxy (`rune-ui`, `rune-provider`, `rune-discord`).

pub(super) const ENTRIES: super::Entries = &[
    // Starting in the tray.
    ("Iniciar minimizado", "Start minimized"),
    (
        "Al iniciar la sesión, RUNE arranca en la bandeja sin abrir la ventana: abrila desde su ícono.",
        "When your session starts, RUNE starts in the tray without opening its window: open it from its icon.",
    ),
    (
        "Sin bandeja del sistema, RUNE abre su ventana igual.",
        "Without a system tray, RUNE opens its window anyway.",
    ),
    (
        "Primero activá «Iniciar con la sesión».",
        "Turn on «Start with the session» first.",
    ),
    (
        "iniciar minimizado en la bandeja segundo plano sin ventana",
        "start minimized in the tray background without window",
    ),
    // The account's activity sharing.
    ("Compartir tu actividad", "Share your activity"),
    (
        "Leyendo el ajuste de tu cuenta…",
        "Reading your account's setting…",
    ),
    (
        "Tu cuenta muestra tu actividad. La privacidad de cada servidor y de tus amigos sigue valiendo.",
        "Your account shows your activity. Each server's and your friends' privacy still applies.",
    ),
    (
        "Tu cuenta oculta tu actividad: aunque RUNE detecte un juego, nadie lo ve.",
        "Your account hides your activity: even if RUNE detects a game, nobody sees it.",
    ),
    (
        "Tu cuenta oculta tu actividad a los demás.",
        "Your account hides your activity from others.",
    ),
    ("Activar en la cuenta", "Turn on for the account"),
    ("Activando…", "Turning on…"),
    (
        "Tu cuenta la muestra a los demás.",
        "Your account shows it to others.",
    ),
    (
        "La sesión la recibió; todavía no se sabe si tu cuenta la muestra.",
        "The session received it; it isn't known yet whether your account shows it.",
    ),
    (
        "Tu cuenta la oculta: nadie la ve.",
        "Your account hides it: nobody sees it.",
    ),
    (
        "Tu cuenta no la muestra: revisá tus juegos y la privacidad de cada servidor.",
        "Your account doesn't show it: check your games and each server's privacy.",
    ),
    (
        "Esperando que tu cuenta la confirme.",
        "Waiting for your account to confirm it.",
    ),
    (
        "compartir tu actividad mostrar el juego a los demás visible oculta cuenta",
        "share your activity show the game to others visible hidden account",
    ),
    (
        "cambiar si se comparte tu actividad",
        "change whether your activity is shared",
    ),
    // Registered games.
    ("Juegos registrados", "Registered games"),
    ("Ningún juego detectado", "No game detected"),
    (
        "Cuando abras un juego que RUNE reconozca, aparece acá.",
        "When you open a game RUNE recognizes, it shows up here.",
    ),
    (
        "Activá «Detectar juegos» para que RUNE reconozca los que abrís.",
        "Turn on «Detect games» so RUNE recognizes the ones you open.",
    ),
    ("Ahora: {name}", "Now: {name}"),
    (
        "Detectado en este equipo. ¿No es este juego? Ocultalo y no se detecta más.",
        "Detected on this computer. Not this game? Hide it and it won't be detected again.",
    ),
    ("No es este juego", "Not this game"),
    ("Nombre del juego", "Game name"),
    ("Agregar un programa", "Add a program"),
    (
        "¿No ves tu juego? Elegí uno de tus programas abiertos: RUNE lo detecta como juego desde ahora, sin el proveedor.",
        "Not seeing your game? Choose one of your open programs: RUNE detects it as a game from now on, without the provider.",
    ),
    (
        "Leyendo los programas abiertos…",
        "Reading the open programs…",
    ),
    (
        "No hay otros programas abiertos.",
        "There are no other open programs.",
    ),
    ("Juegos agregados", "Added games"),
    (
        "Todavía ninguno. Los que RUNE detecte se suman solos, para renombrarlos u ocultarlos.",
        "None yet. The ones RUNE detects join by themselves, to rename or hide them.",
    ),
    ("Restaurar", "Restore"),
    ("Dejar de detectar", "Stop detecting"),
    ("Detectado", "Detected"),
    ("Agregado a mano", "Added by hand"),
    ("Oculto: no se detecta.", "Hidden: not detected."),
    ("Última vez: {day}", "Last time: {day}"),
    ("Juego sin nombre", "Unnamed game"),
    (
        "juego actual detectado ahora no es este juego renombrar",
        "current game detected now not this game rename",
    ),
    (
        "agregar un programa como juego no ves tu juego",
        "add a program as a game not seeing your game",
    ),
    (
        "juegos agregados detectados ocultos renombrar quitar restaurar última vez jugado",
        "added games detected hidden rename remove restore last time played",
    ),
    // The providers' proxy.
    ("Proxy", "Proxy"),
    ("Proxy para la API", "Proxy for the API"),
    ("Automático (el del sistema)", "Automatic (the system's)"),
    ("Sin proxy", "No proxy"),
    ("Esta dirección", "This address"),
    ("Dirección del proxy", "Proxy address"),
    ("Ahora: {url}", "Now: {url}"),
    (
        "Sólo las llamadas a la API (REST) de los proveedores; la conexión en vivo y la voz no pasan por él. Sin usuario ni contraseña: RUNE no guarda credenciales de un proxy. Se aplica la próxima vez que abras RUNE o cambies de proveedor.",
        "Only the providers' API (REST) calls; the live connection and voice don't go through it. No user or password: RUNE doesn't keep a proxy's credentials. It applies the next time you open RUNE or change providers.",
    ),
    (
        "proxy http https api rest red intermediario dirección sin proxy",
        "proxy http https api rest network intermediary address no proxy",
    ),
    (
        "Usa la dirección de un proxy HTTP o HTTPS, sin usuario ni contraseña, ruta, consulta ni fragmento: http://host:puerto.",
        "Use the address of an HTTP or HTTPS proxy, without user or password, path, query or fragment: http://host:port.",
    ),
    (
        "La dirección del proxy no es válida.",
        "The proxy's address isn't valid.",
    ),
];
