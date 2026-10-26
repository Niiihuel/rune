//! Rules and onboarding (2026-10-18): what a server asks of someone who
//! joins it (`rune-model`, `rune-components` and `rune-ui` `onboarding.rs`),
//! the page that edits it (`rune-ui/src/server_settings`), and what the
//! simulation and the personal session say about them.

pub(super) const ENTRIES: super::Entries = &[
    // The model's checks.
    ("Acepta las reglas para seguir", "Accept the rules to continue"),
    ("Las reglas cambiaron: ábrelas otra vez", "The rules changed: open them again"),
    ("RUNE todavía no completa este campo", "RUNE can't fill in this field yet"),
    ("Responde «{title}»", "Answer “{title}”"),
    (
        "Elige una sola opción en «{title}»",
        "Pick only one option in “{title}”",
    ),
    (
        "Esa opción ya no está: abre las preguntas otra vez",
        "That option is gone: open the questions again",
    ),
    ("Hasta {max} preguntas", "Up to {max} questions"),
    ("Cada pregunta necesita un título", "Each question needs a title"),
    (
        "El título de una pregunta lleva hasta {max} caracteres",
        "A question's title takes up to {max} characters",
    ),
    (
        "«{title}» necesita al menos una opción",
        "“{title}” needs at least one option",
    ),
    ("Hasta {max} opciones por pregunta", "Up to {max} options per question"),
    (
        "Cada opción de «{title}» necesita un título",
        "Each option of “{title}” needs a title",
    ),
    (
        "El título de una opción lleva hasta {max} caracteres",
        "An option's title takes up to {max} characters",
    ),
    (
        "La descripción de una opción lleva hasta {max} caracteres",
        "An option's description takes up to {max} characters",
    ),
    (
        "Para activarla, elige al menos un canal por defecto donde todos puedan escribir.",
        "To turn it on, pick at least one default channel where everyone can write.",
    ),
    (
        "El emoji de una opción de «{title}» no vale",
        "An option's emoji in “{title}” isn't valid",
    ),
    // The provider's contract and the state.
    ("completar las reglas del servidor", "complete the server's rules"),
    ("completar las preguntas del servidor", "answer the server's questions"),
    ("guardar la incorporación", "save the onboarding"),
    ("cambiar la incorporación", "change the onboarding"),
    (
        "Solo una comunidad tiene incorporación: este servidor no lo es",
        "Only a community has onboarding: this server isn't one",
    ),
    (
        "La moderación de {server} está revisando tus respuestas: vas a poder participar cuando las apruebe",
        "{server}'s moderators are reviewing your answers: you can take part once they approve them",
    ),
    (
        "La moderación de {server} rechazó tu solicitud",
        "{server}'s moderators rejected your request",
    ),
    // The dialog.
    ("Antes de participar en {server}", "Before taking part in {server}"),
    (
        "Cargando lo que pide el servidor…",
        "Loading what the server asks…",
    ),
    ("Elige todas las que quieras", "Pick as many as you like"),
    ("Reglas del servidor", "Server rules"),
    ("Leí y acepto las reglas", "I've read and accept the rules"),
    (
        "RUNE todavía no completa este campo: respóndelo desde el sitio del servicio.",
        "RUNE can't fill in this field yet: answer it on the service's website.",
    ),
    ("Más tarde", "Later"),
    ("Terminar", "Finish"),
    (
        "Listo: tus respuestas ya están en {server}.",
        "Done: your answers are in {server}.",
    ),
    (
        "Listo: ya puedes participar en {server}.",
        "Done: you can take part in {server} now.",
    ),
    (
        "Enviado: la moderación de {server} va a revisar tus respuestas.",
        "Sent: {server}'s moderators will review your answers.",
    ),
    (
        "La moderación de {server} rechazó tu solicitud.",
        "{server}'s moderators rejected your request.",
    ),
    (
        "La moderación de {server} está revisando tus respuestas.",
        "{server}'s moderators are reviewing your answers.",
    ),
    (
        "Se perdió la conexión: vuelve a intentarlo.",
        "The connection was lost: try again.",
    ),
    (
        "Simulación local: no se envía nada a ningún servicio.",
        "Local simulation: nothing is sent to any service.",
    ),
    // The bar over the composer.
    (
        "Antes de participar, {server} pide que aceptes sus reglas.",
        "Before you take part, {server} asks you to accept its rules.",
    ),
    ("Ver las reglas", "See the rules"),
    (
        "Responde las preguntas de {server} para elegir tus roles y canales.",
        "Answer {server}'s questions to pick your roles and channels.",
    ),
    ("Responder las preguntas", "Answer the questions"),
    // The settings' page.
    ("Incorporación", "Onboarding"),
    (
        "Lo que se le pregunta a quien entra, y los roles y canales que recibe según lo que responda.",
        "What newcomers are asked, and the roles and channels they get for what they answer.",
    ),
    ("No se pudo leer la incorporación.", "The onboarding couldn't be read."),
    ("Activar la incorporación", "Turn on onboarding"),
    (
        "Quien entra responde las preguntas antes de ver el servidor.",
        "Newcomers answer the questions before they see the server.",
    ),
    ("Qué cuenta para los requisitos", "What counts toward the requirements"),
    (
        "El servicio pide canales donde todos puedan escribir; elige si cuentan sólo los canales por defecto o también los que abren las preguntas.",
        "The service asks for channels where everyone can write; pick whether only the default channels count or also the ones the questions open.",
    ),
    ("Sólo los canales por defecto", "Only the default channels"),
    (
        "Los canales por defecto y los de las preguntas",
        "The default channels and the questions' ones",
    ),
    ("Canales por defecto", "Default channels"),
    (
        "Los canales que recibe todo el que entra, responda lo que responda.",
        "The channels every newcomer gets, whatever they answer.",
    ),
    ("{channel} · todos pueden escribir", "{channel} · everyone can write"),
    ("Preguntas", "Questions"),
    (
        "Hasta {max} preguntas. Cada opción puede dar roles y abrir canales.",
        "Up to {max} questions. Each option can give roles and open channels.",
    ),
    ("Agregar pregunta", "Add question"),
    ("Por ejemplo: ¿Qué te trae por aquí?", "For example: What brings you here?"),
    ("Título de la pregunta", "Question title"),
    ("Quitar la pregunta", "Remove the question"),
    ("Una sola respuesta", "Only one answer"),
    ("Obligatoria", "Required"),
    ("Al entrar", "When joining"),
    ("Lista desplegable", "Dropdown"),
    ("Agregar opción", "Add option"),
    ("Roles: {roles} · Canales: {channels}", "Roles: {roles} · Channels: {channels}"),
    ("Opción sin título", "Untitled option"),
    ("Quitar la opción", "Remove the option"),
    ("Emoji de la opción", "Option emoji"),
    ("Título de la opción", "Option title"),
    ("Descripción (opcional)", "Description (optional)"),
    ("Descripción de la opción", "Option description"),
    ("Roles que da", "Roles it gives"),
    ("Este servidor no tiene roles para dar.", "This server has no roles to give."),
    ("Canales que abre", "Channels it opens"),
    ("No se guardó la incorporación", "The onboarding wasn't saved"),
    (
        "Puede que la incorporación se haya guardado",
        "The onboarding may have been saved",
    ),
    (
        "«{emoji}» no es un solo emoji: escribe uno, como 📷.",
        "“{emoji}” isn't a single emoji: type one, like 📷.",
    ),
    // The simulation.
    ("la simulación no hace eso", "the simulation doesn't do that"),
    (
        "Un taller que revisa cada solicitud.",
        "A workshop that reviews every request.",
    ),
    ("este servidor no pide reglas", "this server asks for no rules"),
    ("las reglas cambiaron: ábrelas otra vez", "the rules changed: open them again"),
    ("ya aceptaste las reglas", "you already accepted the rules"),
    ("este servidor no tiene preguntas", "this server has no questions"),
    (
        "las preguntas cambiaron: ábrelas otra vez",
        "the questions changed: open them again",
    ),
    ("la incorporación es de las comunidades", "onboarding is for communities"),
    (
        "un rol o un canal ya no está en el servidor",
        "a role or a channel is no longer in the server",
    ),
    // The personal session.
    ("Ese servidor ya no está disponible.", "That server is no longer available."),
    (
        "Ya aceptaste las reglas de este servidor.",
        "You already accepted this server's rules.",
    ),
    (
        "No tienes permiso para cambiar la incorporación.",
        "You don't have permission to change the onboarding.",
    ),
    ("Solo una comunidad tiene incorporación.", "Only a community has onboarding."),
    (
        "Un rol o un canal ya no está en el servidor.",
        "A role or a channel is no longer in the server.",
    ),
    (
        "La incorporación no cumple lo que Discord pide; revísala.",
        "The onboarding doesn't meet what Discord asks; review it.",
    ),
    (
        "Las respuestas no están completas: revísalas.",
        "The answers aren't complete: review them.",
    ),
];
