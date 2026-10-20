//! The provider contract, the simulation and the domain model (`rune-provider`, `rune-model`).

pub(super) const ENTRIES: super::Entries = &[
    // rune-model/src/interactions.rs
    (
        "Ese comando ya no está disponible aquí",
        "That command is no longer available here",
    ),
    (
        "Elegí un subcomando de la lista",
        "Choose a subcommand from the list",
    ),
    (
        "El comando no tiene la opción «{name}»",
        "The command has no “{name}” option",
    ),
    (
        "La opción «{name}» va una sola vez",
        "The “{name}” option can only be used once",
    ),
    (
        "La opción «{name}» no es de ese tipo",
        "The “{name}” option is not of that type",
    ),
    ("Falta la opción «{name}»", "The “{name}” option is missing"),
    ("Completá este campo", "Fill in this field"),
    ("Elegí una de las opciones", "Choose one of the options"),
    (
        "Tiene que estar entre {min} y {max}",
        "It must be between {min} and {max}",
    ),
    ("Tiene que ser al menos {min}", "It must be at least {min}"),
    ("Tiene que ser como mucho {max}", "It must be at most {max}"),
    (
        "Escribí al menos {min} caracteres",
        "Write at least {min} characters",
    ),
    (
        "Escribí como mucho {max} caracteres",
        "Write at most {max} characters",
    ),
    ("Escribí un número entero", "Write a whole number"),
    ("Escribí un número", "Write a number"),
    ("Elegí sí o no", "Choose yes or no"),
    ("Elegí a alguien", "Choose someone"),
    ("Elegí un rol", "Choose a role"),
    ("Elegí un canal", "Choose a channel"),
    ("Elegí a alguien o un rol", "Choose someone or a role"),
    (
        "RUNE no completa este tipo de opción",
        "RUNE can't fill in this kind of option",
    ),
    ("Elegí una opción", "Choose an option"),
    (
        "Elegí dentro de lo que se permite",
        "Choose within what is allowed",
    ),
    (
        "RUNE no completa este campo",
        "RUNE can't fill in this field",
    ),
    (
        "El valor no corresponde al campo",
        "The value doesn't match the field",
    ),
    // rune-model/src/message_search.rs
    (
        "El historial guardado no está disponible para esta cuenta.",
        "Saved history isn't available for this account.",
    ),
    (
        "Revisá la consulta: hasta 256 caracteres y un intervalo de fechas válido.",
        "Check the query: up to 256 characters and a valid date range.",
    ),
    (
        "Ese mensaje ya no está guardado en este equipo.",
        "That message is no longer saved on this device.",
    ),
    (
        "Los datos superan el límite de esta lectura local.",
        "The data is over the limit of this local read.",
    ),
    (
        "El historial guardado cambió. Actualizá la búsqueda.",
        "Saved history changed. Refresh the search.",
    ),
    (
        "La lectura local se canceló.",
        "The local read was canceled.",
    ),
    (
        "No se pudo leer el historial guardado. Volvé a intentarlo.",
        "Couldn't read saved history. Try again.",
    ),
    // rune-model/src/remote_search.rs
    ("enlace", "link"),
    ("tarjeta", "embed"),
    ("archivo", "file"),
    ("imagen", "image"),
    ("sonido", "sound"),
    ("persona", "user"),
    (
        "Escribí texto o elegí un filtro para buscar.",
        "Write some text or choose a filter to search.",
    ),
    (
        "La búsqueda admite hasta 256 caracteres.",
        "Searches can be up to 256 characters.",
    ),
    ("Usá como mucho 16 filtros.", "Use at most 16 filters."),
    (
        "Elegí personas y canales de las sugerencias.",
        "Choose people and channels from the suggestions.",
    ),
    (
        "«en:» sólo busca en canales de un servidor.",
        "“in:” only searches channels of a server.",
    ),
    (
        "La fecha «después» tiene que ser anterior a «antes».",
        "The “after” date must be earlier than “before”.",
    ),
    // rune-provider/src/lib.rs: what was being done, inside "Couldn't {action}".
    ("aceptar la invitación", "accept the invite"),
    ("enviar el mensaje", "send the message"),
    ("editar el mensaje", "edit the message"),
    ("borrar el mensaje", "delete the message"),
    ("reaccionar", "react"),
    ("votar en la encuesta", "vote in the poll"),
    ("cargar el historial", "load the history"),
    (
        "abrir el contexto del mensaje",
        "open the message's context",
    ),
    ("confirmar la lectura", "confirm the read"),
    ("conectar la voz", "connect voice"),
    ("ver la transmisión", "watch the stream"),
    ("compartir la pantalla", "share your screen"),
    ("usar la cámara", "use the camera"),
    ("cambiar el estado", "change the status"),
    ("cambiar el servidor", "change the server"),
    ("cambiar el canal", "change the channel"),
    ("cambiar los roles", "change the roles"),
    ("cambiar al miembro", "change the member"),
    ("revocar la invitación", "revoke the invite"),
    ("cambiar la expresión", "change the expression"),
    ("cambiar el perfil", "change the profile"),
    ("cargar el perfil", "load the profile"),
    ("cambiar los mensajes fijados", "change the pinned messages"),
    ("cargar los mensajes fijados", "load the pinned messages"),
    ("cambiar la relación", "change the relationship"),
    ("abrir la conversación", "open the conversation"),
    ("cambiar el grupo", "change the group"),
    ("completar la acción", "complete the action"),
    ("crear el grupo", "create the group"),
    ("cerrar la conversación", "close the conversation"),
    ("crear el hilo", "create the thread"),
    ("cambiar el hilo", "change the thread"),
    ("cargar los hilos", "load the threads"),
    (
        "cambiar tu lugar en el escenario",
        "change your place on the stage",
    ),
    (
        "cambiar tu perfil del servidor",
        "change your server profile",
    ),
    ("crear la invitación", "create the invite"),
    ("agregar la expresión", "add the expression"),
    ("podar a los miembros inactivos", "prune inactive members"),
    ("cambiar el webhook", "change the webhook"),
    ("quitar la integración", "remove the integration"),
    ("guardar los GIF favoritos", "save favorite GIFs"),
    ("leer los ajustes de la cuenta", "read the account settings"),
    ("cambiar las notificaciones", "change notifications"),
    (
        "cambiar las carpetas de servidores",
        "change server folders",
    ),
    ("cambiar la privacidad", "change privacy"),
    ("guardar la nota", "save the note"),
    ("marcar como no leído", "mark as unread"),
    ("marcar el servidor como leído", "mark the server as read"),
    ("buscar mensajes", "search messages"),
    ("cargar los comandos", "load the commands"),
    ("usar la aplicación", "use the app"),
    ("reenviar el mensaje", "forward the message"),
    ("crear la encuesta", "create the poll"),
    ("terminar la encuesta", "end the poll"),
    ("cargar la lista de personas", "load the people list"),
    ("abrir el enlace", "open the link"),
    ("cargar los paquetes de stickers", "load the sticker packs"),
    ("buscar GIF", "search GIFs"),
    ("cargar el perfil del servidor", "load the server profile"),
    ("ordenar los canales", "reorder the channels"),
    // rune-provider/src/admin.rs
    (
        "El nombre del emoji necesita de 2 a 32 letras sin acentos, números o guiones bajos.",
        "An emoji name needs 2 to 32 unaccented letters, numbers or underscores.",
    ),
    (
        "El nombre del sticker necesita de 2 a 30 caracteres.",
        "A sticker name needs 2 to 30 characters.",
    ),
    (
        "Elegí un emoji relacionado o una palabra para buscarlo.",
        "Choose a related emoji or a word to find it by.",
    ),
    (
        "La descripción va vacía o con 2 a 100 caracteres.",
        "The description is either empty or 2 to 100 characters.",
    ),
    // rune-provider/src/gifs.rs
    (
        "El buscador de GIF no está configurado.",
        "GIF search isn't set up.",
    ),
    (
        "Buscá con un texto de hasta 50 caracteres.",
        "Search with a text of up to 50 characters.",
    ),
    (
        "No se pudo cargar la búsqueda. Volvé a intentarlo.",
        "Couldn't load the search. Try again.",
    ),
    (
        "El buscador devolvió una respuesta no compatible.",
        "The search returned an unsupported response.",
    ),
    (
        "La configuración del buscador no tiene acceso.",
        "The search's configuration has no access.",
    ),
    (
        "Esperá antes de volver a buscar.",
        "Wait before searching again.",
    ),
    // rune-provider/src/invitations.rs
    (
        "Esta conexión no permite consultar invitaciones.",
        "This connection can't look up invites.",
    ),
    (
        "La invitación no es válida o esta conexión no la entiende.",
        "The invite isn't valid or this connection doesn't understand it.",
    ),
    (
        "No se pudo consultar la invitación. Podés volver a intentarlo.",
        "Couldn't look up the invite. You can try again.",
    ),
    (
        "Hay que esperar antes de consultar otra invitación.",
        "Wait before looking up another invite.",
    ),
    (
        "Esta conexión no permite aceptar invitaciones.",
        "This connection can't accept invites.",
    ),
    (
        "La invitación ya no es válida.",
        "The invite is no longer valid.",
    ),
    (
        "No podés aceptar esta invitación con esta cuenta.",
        "You can't accept this invite with this account.",
    ),
    (
        "El servicio requiere una verificación para entrar.",
        "The service requires a verification to join.",
    ),
    (
        "El servicio pide una verificación para entrar.",
        "The service asks for a verification to join.",
    ),
    (
        "Esperá antes de volver a aceptar la invitación.",
        "Wait before accepting the invite again.",
    ),
    (
        "No se pudo enviar la solicitud. Podés volver a intentarlo.",
        "Couldn't send the request. You can try again.",
    ),
    (
        "No se pudo confirmar el ingreso. Comprobá si el servidor aparece.",
        "Couldn't confirm you joined. Check whether the server shows up.",
    ),
    // rune-provider/src/message_actions.rs
    ("Escribí una pregunta", "Write a question"),
    ("La pregunta es demasiado larga", "The question is too long"),
    (
        "Una encuesta tiene entre 2 y 10 respuestas",
        "A poll has between 2 and 10 answers",
    ),
    (
        "Cada respuesta necesita texto o un emoji",
        "Each answer needs text or an emoji",
    ),
    ("Una respuesta es demasiado larga", "An answer is too long"),
    ("Elegí cuánto dura", "Choose how long it lasts"),
    // rune-provider/src/server_admin.rs
    (
        "El ícono de un rol va sin animación: elegí un PNG o un JPG.",
        "A role icon can't be animated: choose a PNG or a JPG.",
    ),
    ("Elegí un solo emoji.", "Choose a single emoji."),
    // rune-provider/src/uploads.rs
    ("Se canceló el archivo.", "The file was canceled."),
    (
        "Los archivos no están disponibles en esta conexión.",
        "Files aren't available on this connection.",
    ),
    (
        "No tenés permiso para enviar archivos en esta conversación.",
        "You don't have permission to send files in this conversation.",
    ),
    (
        "Esperá a que termine la preparación o el envío anterior.",
        "Wait for the previous preparation or upload to finish.",
    ),
    (
        "El archivo o el mensaje supera el límite permitido.",
        "The file or the message is over the allowed limit.",
    ),
    (
        "Este envío tiene más archivos de los permitidos.",
        "This upload has more files than allowed.",
    ),
    (
        "El archivo cambió mientras se preparaba. Volvé a seleccionarlo.",
        "The file changed while it was being prepared. Select it again.",
    ),
    (
        "El archivo seleccionado no es válido.",
        "The selected file isn't valid.",
    ),
    (
        "Venció el intento de envío. Podés volver a intentarlo.",
        "The upload attempt expired. You can try again.",
    ),
    (
        "El archivo ya no está preparado. Volvé a seleccionarlo.",
        "The file is no longer prepared. Select it again.",
    ),
    (
        "Quitá un archivo preparado antes de agregar otro.",
        "Remove a prepared file before adding another.",
    ),
    (
        "No se pudo completar el archivo.",
        "Couldn't complete the file.",
    ),
    (
        "El portapapeles no tiene archivos ni imágenes para adjuntar.",
        "The clipboard has no files or images to attach.",
    ),
    // rune-provider/src/mock.rs and mock/: the simulation's labels and refusals.
    ("Simulación local", "Local simulation"),
    (
        "Datos simulados en este equipo: no hay conexión con Discord ni con ningún servicio.",
        "Simulated data on this device: there is no connection to Discord or any service.",
    ),
    (
        "La simulación local no usa autorización de cuentas.",
        "The local simulation doesn't use account authorization.",
    ),
    (
        "No encontramos ese mensaje en este contexto.",
        "We couldn't find that message in this context.",
    ),
    ("rechazado (fallo simulado)", "rejected (simulated failure)"),
    (
        "estás aislado en este servidor (simulado)",
        "you're timed out in this server (simulated)",
    ),
    ("el canal no existe", "the channel doesn't exist"),
    ("simulado", "simulated"),
    ("(simulado)", "(simulated)"),
    ("(simulada)", "(simulated)"),
    ("Servidor simulado: {name}.", "Simulated server: {name}."),
    (
        "Mensaje de voz simulado: un tono generado en este equipo.",
        "Simulated voice message: a tone generated on this device.",
    ),
    ("el mensaje no existe", "the message doesn't exist"),
    (
        "solo se pueden editar los mensajes propios",
        "you can only edit your own messages",
    ),
    (
        "solo podés borrar tus mensajes",
        "you can only delete your messages",
    ),
    ("el mensaje no tiene encuesta", "the message has no poll"),
    ("la encuesta terminó", "the poll ended"),
    (
        "esas respuestas no valen para la encuesta",
        "those answers aren't valid for the poll",
    ),
    (
        "no tenés permiso para fijar mensajes",
        "you don't have permission to pin messages",
    ),
    (
        "un canal admite hasta {max} mensajes fijados",
        "a channel allows up to {max} pinned messages",
    ),
    (
        "Esa persona no está transmitiendo en tu llamada.",
        "That person isn't streaming in your call.",
    ),
    (
        "Entrá a la llamada para compartir la pantalla.",
        "Join the call to share your screen.",
    ),
    (
        "Entrá a la llamada para encender la cámara.",
        "Join the call to turn on your camera.",
    ),
    // mock/account.rs
    ("esas carpetas no son válidas", "those folders aren't valid"),
    ("el servidor no existe", "the server doesn't exist"),
    (
        "la nota puede tener hasta 256 caracteres",
        "the note can be up to 256 characters",
    ),
    (
        "el canal no es de ese servidor",
        "the channel isn't in that server",
    ),
    // mock/admin.rs
    (
        "esa invitación ya no existe",
        "that invite no longer exists",
    ),
    ("ese emoji ya no existe", "that emoji no longer exists"),
    (
        "el nombre del emoji necesita de 2 a 32 letras, números o guiones bajos",
        "an emoji name needs 2 to 32 letters, numbers or underscores",
    ),
    ("ese sticker ya no existe", "that sticker no longer exists"),
    (
        "el nombre del sticker necesita de 2 a 30 caracteres",
        "a sticker name needs 2 to 30 characters",
    ),
    ("Aplicación", "App"),
    (
        "el aislamiento dura de un minuto a 28 días",
        "a timeout lasts from one minute to 28 days",
    ),
    ("esa persona no es miembro", "that person isn't a member"),
    (
        "no podés hacerte eso a vos",
        "you can't do that to yourself",
    ),
    (
        "nadie puede hacerle eso a quien es dueño",
        "nobody can do that to the owner",
    ),
    (
        "su rol está a la altura del tuyo o por encima",
        "their role is as high as yours or higher",
    ),
    (
        "ese canal no es de este servidor",
        "that channel isn't in this server",
    ),
    (
        "no tenés permiso para invitar en ese canal",
        "you don't have permission to invite in that channel",
    ),
    (
        "una invitación dura hasta 7 días y hasta 100 usos",
        "an invite lasts up to 7 days and up to 100 uses",
    ),
    (
        "el servidor ya tiene {max} emojis",
        "the server already has {max} emoji",
    ),
    (
        "el servidor ya tiene {max} stickers",
        "the server already has {max} stickers",
    ),
    (
        "no tenés permiso para ver los miembros",
        "you don't have permission to see the members",
    ),
    ("esa página no existe", "that page doesn't exist"),
    (
        "se poda por 1 a 30 días sin actividad",
        "pruning takes 1 to 30 days without activity",
    ),
    (
        "podó {count} miembros inactivos",
        "pruned {count} inactive members",
    ),
    (
        "un webhook publica en un canal de texto del servidor",
        "a webhook posts in a text channel of the server",
    ),
    (
        "el nombre necesita de 1 a 80 caracteres, sin «discord» ni «clyde»",
        "the name needs 1 to 80 characters, without “discord” or “clyde”",
    ),
    ("creó el webhook {name}", "created the webhook {name}"),
    ("ese webhook ya no existe", "that webhook no longer exists"),
    (
        "ese webhook es de otro servidor o de una aplicación",
        "that webhook belongs to another server or to an app",
    ),
    ("actualizó el webhook {name}", "updated the webhook {name}"),
    ("eliminó el webhook {name}", "deleted the webhook {name}"),
    (
        "esa integración ya no existe",
        "that integration no longer exists",
    ),
    (
        "quitó la integración {name}",
        "removed the integration {name}",
    ),
    // mock/challenges.rs
    (
        "Cancelaste la verificación (simulada); no se envió nada.",
        "You canceled the verification (simulated); nothing was sent.",
    ),
    // mock/desktop.rs
    (
        "Aplicación {application} (simulada)",
        "App {application} (simulated)",
    ),
    ("Aplicación (simulada)", "App (simulated)"),
    // mock/identity.rs
    ("GIF simulados", "Simulated GIFs"),
    ("Dominio", "Domain"),
    (
        "ese sticker no existe (simulado)",
        "that sticker doesn't exist (simulated)",
    ),
    (
        "hace falta {upgrade} para usar stickers de otros servidores",
        "{upgrade} is needed to use stickers from other servers",
    ),
    // mock/interactions.rs
    (
        "La conversación no existe (simulado).",
        "The conversation doesn't exist (simulated).",
    ),
    (
        "El mensaje ya no está (simulado).",
        "The message is gone (simulated).",
    ),
    (
        "RuneBot no reconoce ese control (simulado).",
        "RuneBot doesn't recognize that control (simulated).",
    ),
    (
        "Ese formulario ya no está abierto (simulado).",
        "That form is no longer open (simulated).",
    ),
    (
        "Ese comando ya no existe (simulado).",
        "That command no longer exists (simulated).",
    ),
    (
        "RuneBot no pudo completar la interacción (simulado).",
        "RuneBot couldn't complete the interaction (simulated).",
    ),
    ("(simulado: no va a sonar)", "(simulated: it won't go off)"),
    (
        "nada que mostrar, es una simulación.",
        "nothing to show, it's a simulation.",
    ),
    (
        "Moderación simulada: no se hizo nada.",
        "Simulated moderation: nothing was done.",
    ),
    (
        "RuneBot no conoce ese comando (simulado).",
        "RuneBot doesn't know that command (simulated).",
    ),
    // mock/manage.rs
    ("no tenés permiso", "you don't have permission"),
    (
        "la simulación guarda los favoritos en este equipo",
        "the simulation keeps favorites on this device",
    ),
    (
        "la simulación no sabe hacer eso",
        "the simulation can't do that",
    ),
    // mock/invitations.rs
    (
        "Un club abierto para probar invitaciones.",
        "An open workshop to try invites in.",
    ),
    (
        "no sos miembro del servidor",
        "you're not a member of the server",
    ),
    ("el rol no existe", "the role doesn't exist"),
    (
        "ese rol está a la altura del tuyo o por encima",
        "that role is as high as yours or higher",
    ),
    (
        "solo quien es dueño puede borrar el servidor",
        "only the owner can delete the server",
    ),
    ("El servidor se borró", "The server was deleted"),
    (
        "sos dueño: borrá el servidor en vez de abandonarlo",
        "you're the owner: delete the server instead of leaving it",
    ),
    (
        "un servidor solo tiene canales de texto, de voz y categorías",
        "a server only has text channels, voice channels and categories",
    ),
    ("la categoría no existe", "the category doesn't exist"),
    (
        "solo se puede poner un canal dentro de una categoría",
        "a channel can only go inside a category",
    ),
    (
        "una conversación directa no se administra",
        "a direct message isn't managed",
    ),
    (
        "una categoría no puede ir dentro de otra",
        "a category can't go inside another",
    ),
    (
        "el modo lento llega hasta 6 horas",
        "slowmode goes up to 6 hours",
    ),
    (
        "@everyone solo cambia sus permisos",
        "@everyone only changes its permissions",
    ),
    (
        "no podés dar permisos que no tenés",
        "you can't give permissions you don't have",
    ),
    (
        "no podés subir un rol por encima del tuyo",
        "you can't move a role above yours",
    ),
    ("@everyone no se puede borrar", "@everyone can't be deleted"),
    ("ese rol no existe", "that role doesn't exist"),
    (
        "el apodo puede tener hasta 32 caracteres",
        "the nickname can be up to 32 characters",
    ),
    (
        "nadie puede sacar a quien es dueño",
        "nobody can remove the owner",
    ),
    ("esa persona no estaba vetada", "that person wasn't banned"),
    (
        "el nombre puede tener hasta 32 caracteres",
        "the name can be up to 32 characters",
    ),
    (
        "«Sobre mí» puede tener hasta 190 caracteres",
        "“About me” can be up to 190 characters",
    ),
    ("la cuenta no existe", "the account doesn't exist"),
    (
        "el estado puede tener hasta {max} caracteres",
        "the status can be up to {max} characters",
    ),
    ("el nombre no puede quedar vacío", "the name can't be empty"),
    (
        "el nombre puede tener hasta 100 caracteres",
        "the name can be up to 100 characters",
    ),
    // mock/message_actions.rs
    (
        "la simulación no conoce esa acción",
        "the simulation doesn't know that action",
    ),
    (
        "la conversación no existe",
        "the conversation doesn't exist",
    ),
    (
        "ahí no se escriben mensajes",
        "messages can't be written there",
    ),
    (
        "ese mensaje no se puede reenviar",
        "that message can't be forwarded",
    ),
    (
        "solo quien la creó termina una encuesta",
        "only whoever created a poll can end it",
    ),
    ("ese mensaje no tiene encuesta", "that message has no poll"),
    ("la encuesta ya terminó", "the poll already ended"),
    (
        "nadie reaccionó con ese emoji",
        "nobody reacted with that emoji",
    ),
    ("esa respuesta no existe", "that answer doesn't exist"),
    (
        "no tenés acceso a esa conversación",
        "you don't have access to that conversation",
    ),
    (
        "esa persona no está en un canal de voz del servidor",
        "that person isn't in a voice channel of the server",
    ),
    (
        "no se puede mover a ese canal",
        "can't move to that channel",
    ),
    // mock/profiles.rs
    ("esa persona no existe", "that person doesn't exist"),
    (
        "no sos miembro de ese servidor",
        "you're not a member of that server",
    ),
    (
        "tu cuenta está restringida en ese servidor",
        "your account is restricted in that server",
    ),
    // mock/read_state.rs
    (
        "Esta solicitud de lectura no corresponde a la cuenta simulada.",
        "This read request isn't for the simulated account.",
    ),
    (
        "No se puede confirmar la lectura de esta conversación.",
        "Can't confirm the read of this conversation.",
    ),
    (
        "El límite de lectura no corresponde a un mensaje emitido en esa conversación.",
        "The read boundary isn't a message sent in that conversation.",
    ),
    (
        "La versión de lectura no admite otro avance.",
        "The read version can't advance any further.",
    ),
    (
        "No se puede marcar como no leído ese mensaje.",
        "Can't mark that message as unread.",
    ),
    // mock/search.rs
    ("Esa página no existe.", "That page doesn't exist."),
    (
        "Esa conversación no es de ese servidor.",
        "That conversation isn't in that server.",
    ),
    (
        "No podés leer el historial de esa conversación.",
        "You can't read that conversation's history.",
    ),
    // mock/server_admin.rs
    (
        "hasta 5 rasgos, cada uno con un nombre corto",
        "up to 5 traits, each with a short name",
    ),
    (
        "ese tiempo de inactividad no está disponible",
        "that inactivity time isn't available",
    ),
    (
        "una comunidad necesita verificación y revisar el contenido de todos",
        "a community needs verification and checking everyone's content",
    ),
    (
        "este servidor no tiene íconos de rol",
        "this server has no role icons",
    ),
    (
        "este servidor no tiene degradés de rol",
        "this server has no role gradients",
    ),
    (
        "un degradé lleva uno o dos colores más",
        "a gradient takes one or two more colors",
    ),
    ("solo un foro tiene etiquetas", "only a forum has tags"),
    (
        "un foro tiene hasta 20 etiquetas",
        "a forum has up to 20 tags",
    ),
    (
        "cada etiqueta necesita un nombre corto",
        "each tag needs a short name",
    ),
    ("no hay nada que mover", "there's nothing to move"),
    (
        "cada canal va una vez, en un lugar válido",
        "each channel goes once, in a valid place",
    ),
    (
        "una categoría no va dentro de otra",
        "a category doesn't go inside another",
    ),
    (
        "un canal solo va dentro de una categoría",
        "a channel only goes inside a category",
    ),
    // mock/showcase.rs
    (
        "(simulada): un tono generado.",
        "(simulated): a generated tone.",
    ),
    (
        "(simulada): un patrón de prueba.",
        "(simulated): a test pattern.",
    ),
    // mock/social.rs
    (
        "no hay una solicitud de esa persona",
        "there's no request from that person",
    ),
    (
        "no hay ninguna relación con esa persona",
        "there's no relationship with that person",
    ),
    ("ya está en el grupo", "they're already in the group"),
    (
        "solo podés añadir a tus amigos",
        "you can only add your friends",
    ),
    (
        "un grupo tiene hasta 10 personas",
        "a group has up to 10 people",
    ),
    (
        "solo quien es dueño del grupo puede quitar personas",
        "only the group's owner can remove people",
    ),
    ("no está en el grupo", "they're not in the group"),
    (
        "solo quien es dueño del grupo puede pasar la propiedad",
        "only the group's owner can transfer ownership",
    ),
    (
        "esa persona no está en el grupo",
        "that person isn't in the group",
    ),
    (
        "solo se cierran conversaciones directas",
        "only direct messages can be closed",
    ),
    (
        "solo tus amigos pueden tener un apodo",
        "only your friends can have a nickname",
    ),
    ("el grupo no existe", "the group doesn't exist"),
    (
        "no encontramos a nadie con ese nombre de usuario",
        "we couldn't find anyone with that username",
    ),
    (
        "no podés enviarte una solicitud",
        "you can't send yourself a request",
    ),
    ("ya son amigos", "you're already friends"),
    (
        "ya le enviaste una solicitud",
        "you already sent them a request",
    ),
    (
        "bloqueaste a esa persona: desbloqueala primero",
        "you blocked that person: unblock them first",
    ),
    ("bloqueaste a esa persona", "you blocked that person"),
    ("elegí al menos a dos amigos", "choose at least two friends"),
    // mock/stage.rs
    ("no está en el escenario", "they're not on the stage"),
    ("el escenario no existe", "the stage doesn't exist"),
    ("no estás en el escenario", "you're not on the stage"),
    (
        "no tenés permiso para pedir la palabra",
        "you don't have permission to request to speak",
    ),
    ("ya podés hablar", "you can already speak"),
    (
        "esperá que la moderación te invite a hablar",
        "wait for the moderators to invite you to speak",
    ),
    (
        "solo la moderación invita a hablar",
        "only moderators invite people to speak",
    ),
    // mock/threads.rs
    (
        "no tenés permiso para escribir en este hilo",
        "you don't have permission to write in this thread",
    ),
    ("el hilo está bloqueado", "the thread is locked"),
    ("el hilo no existe", "the thread doesn't exist"),
    (
        "en este canal no se abren hilos",
        "threads can't be opened in this channel",
    ),
    (
        "un hilo privado se abre solo en un canal de texto, sin mensaje",
        "a private thread only opens in a text channel, without a message",
    ),
    (
        "no tenés permiso para abrir hilos acá",
        "you don't have permission to open threads here",
    ),
    (
        "ese mensaje ya tiene un hilo",
        "that message already has a thread",
    ),
    ("el foro no existe", "the forum doesn't exist"),
    (
        "no tenés permiso para publicar en este foro",
        "you don't have permission to post in this forum",
    ),
    (
        "esa etiqueta es solo para la moderación",
        "that tag is for moderators only",
    ),
    (
        "el primer mensaje tiene de 1 a 2000 caracteres",
        "the first message has 1 to 2000 characters",
    ),
    (
        "una publicación lleva hasta 10 archivos",
        "a post takes up to 10 files",
    ),
    (
        "solo la moderación ve todos los hilos privados",
        "only moderators see every private thread",
    ),
    ("cursor inválido", "invalid cursor"),
    (
        "reabrí el hilo antes de seguirlo o dejarlo",
        "reopen the thread before following or leaving it",
    ),
    (
        "solo quien abrió el hilo o la moderación lo cambia",
        "only whoever opened the thread or moderators can change it",
    ),
    (
        "solo la moderación bloquea hilos",
        "only moderators lock threads",
    ),
    (
        "el hilo está bloqueado: solo la moderación lo reabre",
        "the thread is locked: only moderators can reopen it",
    ),
    (
        "solo las publicaciones de un foro llevan etiquetas",
        "only a forum's posts take tags",
    ),
    (
        "solo la moderación borra hilos",
        "only moderators delete threads",
    ),
    (
        "el nombre tiene de 1 a {max} caracteres, en una línea",
        "the name has 1 to {max} characters, on one line",
    ),
    ("esa etiqueta no es del foro", "that tag isn't the forum's"),
    (
        "una publicación lleva hasta {max} etiquetas",
        "a post takes up to {max} tags",
    ),
    (
        "este foro pide al menos una etiqueta",
        "this forum asks for at least one tag",
    ),
    // A real account's first cards (2026-10-17).
    ("Armo fábricas con limn", "I build factories with limn"),
    ("(emoji simulados)", "(simulated emoji)"),
    // Kept in Spanish and translated where shown (2026-10-18): the plan,
    // and the packs' names, which are content but for their label.
    ("Plus (simulado)", "Plus (simulated)"),
    ("Saludos (simulado)", "Saludos (simulated)"),
    (
        "Cangrejos animados (simulado)",
        "Cangrejos animados (simulated)",
    ),
    // Reactions, pins and search (2026-10-18): rune-model/src/remote_search.rs.
    ("encuesta", "poll"),
    (
        "Escribí el sitio como ejemplo.com.",
        "Write the site as example.com.",
    ),
];
