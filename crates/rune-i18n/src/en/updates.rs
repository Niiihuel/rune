//! RUNE's own updates (actualizaciones): what `rune-update` reports, the
//! mandatory update screen, the notice of an optional one and Ajustes ›
//! Actualizaciones.

pub(super) const ENTRIES: super::Entries = &[
    // rune-update.
    (
        "Esta compilación no tiene la clave de las actualizaciones.",
        "This build has no update key.",
    ),
    (
        "No se pudo conectar con el servidor de actualizaciones.",
        "Could not reach the update server.",
    ),
    (
        "El servidor de actualizaciones respondió {status}.",
        "The update server answered {status}.",
    ),
    (
        "La respuesta del servidor de actualizaciones es demasiado grande.",
        "The update server's answer is too large.",
    ),
    (
        "La firma de la actualización no es válida: no se instaló nada.",
        "The update's signature is not valid: nothing was installed.",
    ),
    (
        "No se pudo leer la descripción de la actualización.",
        "Could not read the update's description.",
    ),
    (
        "Esta versión no tiene descarga para este sistema.",
        "This version has no download for this system.",
    ),
    (
        "La descarga quedó incompleta: se retoma al reintentar.",
        "The download is incomplete: retrying resumes it.",
    ),
    (
        "Lo descargado no coincide con lo firmado: se borró.",
        "What was downloaded does not match what was signed: it was deleted.",
    ),
    (
        "No se pudo escribir la actualización en el disco.",
        "Could not write the update to disk.",
    ),
    (
        "Esta copia de RUNE no se instaló con su instalador: actualizala desde donde la compilaste.",
        "This copy of RUNE was not installed by its installer: update it where you built it.",
    ),
    (
        "La actualización se interrumpió. Reintentá para completarla.",
        "The update was interrupted. Try again to finish it.",
    ),
    // rune-components: the mandatory screen and the optional notice.
    ("Actualización obligatoria", "Mandatory update"),
    (
        "RUNE {version} reemplaza a la {installed} que tenés instalada, que ya no se puede usar. RUNE se actualiza antes de abrir.",
        "RUNE {version} replaces the {installed} you have installed, which can no longer be used. RUNE updates before it opens.",
    ),
    ("RUNE {version} está disponible", "RUNE {version} is available"),
    (
        "Tenés la {installed}. Se instala y RUNE vuelve a abrir.",
        "You have {installed}. It installs and RUNE opens again.",
    ),
    ("Actualizar y reiniciar", "Update and restart"),
    ("Más tarde", "Later"),
    ("Notas", "Notes"),
    ("Descargando… {have} de {whole}", "Downloading… {have} of {whole}"),
    ("Instalando…", "Installing…"),
    ("Reiniciando RUNE…", "Restarting RUNE…"),
    (
        "Instalada; reiniciá RUNE para usarla.",
        "Installed; restart RUNE to use it.",
    ),
    ("Reiniciar ahora", "Restart now"),
    ("Reintentar", "Try again"),
    ("Salir", "Quit"),
    // rune-ui: Ajustes › Actualizaciones.
    ("Actualizaciones", "Updates"),
    (
        "versión instalada actual número dónde instalación",
        "installed version current number where installation",
    ),
    (
        "buscar actualizaciones ahora nueva versión descargar instalar reiniciar última verificación",
        "check for updates now new version download install restart last check",
    ),
    (
        "buscar al iniciar abrir comprobar automáticamente obligatoria",
        "check at start open look automatically mandatory",
    ),
    (
        "Instalada en {folder}. Una actualización la reemplaza entera, o no la toca.",
        "Installed in {folder}. An update replaces it whole, or leaves it alone.",
    ),
    (
        "Esta copia no se instaló con el instalador de RUNE: avisa si hay una versión nueva, pero se actualiza desde donde la compilaste.",
        "This copy was not installed by RUNE's installer: it tells you of a new version, but you update it where you built it.",
    ),
    ("Versión instalada", "Installed version"),
    ("Instalando RUNE {version}…", "Installing RUNE {version}…"),
    ("Novedades de RUNE {version}", "What's new in RUNE {version}"),
    ("Ver en GitHub", "View on GitHub"),
    (
        "novedades notas cambios versión github imágenes videos",
        "whats new notes changes version github pictures videos",
    ),
    ("Buscar ahora", "Check now"),
    ("Buscar al iniciar", "Check at start"),
    (
        "Al abrir, RUNE pregunta si hay una versión nueva, con unos segundos de límite. Una obligatoria se instala antes de abrir; las demás se ofrecen sin interrumpir.",
        "When it opens, RUNE asks whether there is a new version, for a few seconds at most. A mandatory one installs before it opens; the rest are offered without interrupting.",
    ),
    // The settings card (rune-components::updates::check_card).
    ("Buscando actualizaciones…", "Checking for updates…"),
    ("Disponible", "Available"),
    ("Descargando RUNE {version}", "Downloading RUNE {version}"),
    ("Descargando", "Downloading"),
    ("Instalando", "Installing"),
    ("RUNE {version} está instalada", "RUNE {version} is installed"),
    ("Instalada", "Installed"),
    ("No se pudo actualizar", "Could not update"),
    ("Error", "Error"),
    (
        "No se pudo buscar actualizaciones",
        "Could not check for updates",
    ),
    ("Tenés la última versión", "You have the latest version"),
    ("Al día", "Up to date"),
    ("Todavía no se buscó", "Not checked yet"),
    (
        "Instalada: RUNE {installed}. Última verificación: {when}.",
        "Installed: RUNE {installed}. Last checked: {when}.",
    ),
    ("Instalada: RUNE {installed}.", "Installed: RUNE {installed}."),
    ("{have} de {whole}", "{have} of {whole}"),
];
