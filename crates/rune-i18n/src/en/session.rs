//! The unofficial session, OAuth, the login helper and the platform (`rune-discord-session`, `rune-discord`, `rune-login`, `rune-platform`).

pub(super) const ENTRIES: super::Entries = &[
    // The session's account settings.
    (
        "El estado puede tener hasta 128 caracteres, sin saltos de línea.",
        "A status can have up to 128 characters, without line breaks.",
    ),
    ("Esas carpetas no son válidas.", "Those folders aren't valid."),
    ("Ese canal ya no está en tu lista.", "That channel is no longer in your list."),
    ("La nota puede tener hasta 256 caracteres.", "A note can have up to 256 characters."),
    // Server administration.
    ("@everyone no se puede eliminar.", "@everyone can't be deleted."),
    (
        "Carga a esa persona en la lista de miembros antes de cambiar sus roles.",
        "Load that person in the member list before changing their roles.",
    ),
    ("De @everyone solo cambian sus permisos.", "Only @everyone's permissions can change."),
    ("El aislamiento dura hasta 28 días.", "A timeout lasts up to 28 days."),
    (
        "El apodo puede tener hasta 32 caracteres, sin saltos de línea.",
        "A nickname can have up to 32 characters, without line breaks.",
    ),
    ("El cursor de la página no es válido.", "The page cursor isn't valid."),
    ("El modo lento llega hasta 6 horas.", "Slowmode goes up to 6 hours."),
    (
        "El motivo puede tener hasta 512 caracteres, sin saltos de línea.",
        "A reason can have up to 512 characters, without line breaks.",
    ),
    (
        "El nombre del canal necesita de 1 a 100 caracteres.",
        "A channel name needs 1 to 100 characters.",
    ),
    (
        "El nombre del emoji necesita de 2 a 32 letras sin acentos, números o guiones bajos.",
        "An emoji name needs 2 to 32 unaccented letters, numbers or underscores.",
    ),
    (
        "El nombre del rol necesita de 1 a 100 caracteres.",
        "A role name needs 1 to 100 characters.",
    ),
    (
        "El nombre del servidor necesita de 2 a 100 caracteres.",
        "A server name needs 2 to 100 characters.",
    ),
    (
        "El nombre del sticker necesita de 2 a 30 caracteres.",
        "A sticker name needs 2 to 30 characters.",
    ),
    (
        "El nombre necesita de 1 a 80 caracteres, sin «discord» ni «clyde».",
        "The name needs 1 to 80 characters, without “discord” or “clyde”.",
    ),
    ("El tema es demasiado largo.", "The topic is too long."),
    ("Esa expresión ya no existe.", "That expression no longer exists."),
    (
        "Esa persona ya no está en un canal de voz del servidor.",
        "That person is no longer in a voice channel of the server.",
    ),
    ("Ese canal ya no está en el servidor.", "That channel is no longer in the server."),
    ("Ese filtro no está disponible.", "That filter isn't available."),
    ("Ese permiso ya no está en el canal.", "That permission is no longer in the channel."),
    (
        "Ese rol está a la altura del tuyo o por encima.",
        "That role is as high as yours or higher.",
    ),
    ("Ese rol ya no existe.", "That role no longer exists."),
    (
        "Ese servidor o canal ya no está disponible.",
        "That server or channel is no longer available.",
    ),
    (
        "Ese tipo de canal no se crea en un servidor.",
        "That kind of channel isn't created in a server.",
    ),
    (
        "La descripción puede tener hasta 300 caracteres.",
        "A description can have up to 300 characters.",
    ),
    (
        "Nadie puede hacerle eso a quien es dueño del servidor.",
        "Nobody can do that to the server's owner.",
    ),
    ("No hay cambios para guardar.", "There are no changes to save."),
    ("No puedes dar permisos que no tienes.", "You can't give permissions you don't have."),
    ("No puedes hacerte esto a ti.", "You can't do this to yourself."),
    (
        "No puedes subir un rol a la altura del tuyo o más arriba.",
        "You can't move a role as high as yours or higher.",
    ),
    (
        "No se puede aislar a quien administra el servidor.",
        "A server administrator can't be timed out.",
    ),
    ("No se puede mover a ese canal.", "Can't move to that channel."),
    (
        "No tienes permiso para esta acción en el servidor.",
        "You don't have permission for this action in the server.",
    ),
    (
        "Se invita a un canal, no a una categoría.",
        "Invites are to a channel, not to a category.",
    ),
    (
        "Se poda por 1 a 30 días sin actividad.",
        "Pruning takes 1 to 30 days without activity.",
    ),
    (
        "Solo quien es dueño puede eliminar el servidor.",
        "Only the owner can delete the server.",
    ),
    (
        "Eres dueño del servidor: elimínalo en vez de salir.",
        "You own the server: delete it instead of leaving.",
    ),
    (
        "Su rol está a la altura del tuyo o por encima.",
        "Their role is as high as yours or higher.",
    ),
    (
        "Tu cuenta está restringida en este servidor: puedes leer, pero no administrar.",
        "Your account is restricted in this server: you can read, but not manage.",
    ),
    (
        "Tu membresía en este servidor todavía no cargó.",
        "Your membership in this server hasn't loaded yet.",
    ),
    (
        "Un canal solo va dentro de una categoría.",
        "A channel only goes inside a category.",
    ),
    (
        "Un permiso no puede estar permitido y denegado a la vez.",
        "A permission can't be allowed and denied at once.",
    ),
    (
        "Un webhook publica en un canal de texto, de anuncios o un foro.",
        "A webhook posts in a text channel, an announcement channel or a forum.",
    ),
    ("Una categoría no va dentro de otra.", "A category doesn't go inside another."),
    (
        "Una invitación dura hasta 7 días y sirve hasta 100 veces.",
        "An invite lasts up to 7 days and works up to 100 times.",
    ),
    // The audit log: what someone did, after their name.
    ("(sin nombre)", "(no name)"),
    ("abrió un escenario", "opened a stage"),
    ("activó una regla de AutoMod", "triggered an AutoMod rule"),
    ("actualizó a un miembro", "updated a member"),
    ("actualizó los permisos de los comandos", "updated the commands' permissions"),
    ("actualizó un escenario", "updated a stage"),
    ("actualizó un evento", "updated an event"),
    ("actualizó un hilo", "updated a thread"),
    ("actualizó un webhook", "updated a webhook"),
    ("actualizó una integración", "updated an integration"),
    ("actualizó una invitación", "updated an invite"),
    ("actualizó una regla de AutoMod", "updated an AutoMod rule"),
    ("agregó un bot", "added a bot"),
    ("agregó una integración", "added an integration"),
    ("cerró un escenario", "closed a stage"),
    ("creó un evento", "created an event"),
    ("creó un hilo", "created a thread"),
    ("creó un webhook", "created a webhook"),
    ("creó una regla de AutoMod", "created an AutoMod rule"),
    ("dejó de fijar un mensaje", "unpinned a message"),
    (
        "desconectó a alguien de un canal de voz",
        "disconnected someone from a voice channel",
    ),
    ("eliminó mensajes en bloque", "bulk deleted messages"),
    ("eliminó un evento", "deleted an event"),
    ("eliminó un hilo", "deleted a thread"),
    ("eliminó un mensaje", "deleted a message"),
    ("eliminó un webhook", "deleted a webhook"),
    ("eliminó una regla de AutoMod", "deleted an AutoMod rule"),
    ("fijó un mensaje", "pinned a message"),
    ("hizo un cambio en el servidor", "made a change in the server"),
    ("movió a alguien de canal de voz", "moved someone to another voice channel"),
    ("podó miembros inactivos", "pruned inactive members"),
    ("podó {count} miembros inactivos", "pruned {count} inactive members"),
    ("quitó una integración", "removed an integration"),
    ("Suscripciones del servidor", "Server subscriptions"),
    // Favorites and the rest of administration.
    ("Ese GIF no se puede guardar como favorito.", "That GIF can't be saved as a favorite."),
    (
        "Tus favoritos cambiaron en otro dispositivo; vuelve a intentarlo.",
        "Your favorites changed on another device; try again.",
    ),
    ("Ya tienes demasiados GIF favoritos.", "You already have too many favorite GIFs."),
    ("Ese canal no sirve para eso.", "That channel can't be used for that."),
    (
        "Este servidor no tiene desbloqueados los degradés de rol.",
        "This server hasn't unlocked role gradients.",
    ),
    (
        "Este servidor no tiene desbloqueados los íconos de rol.",
        "This server hasn't unlocked role icons.",
    ),
    (
        "Hasta 5 rasgos, cada uno de hasta 24 caracteres y un emoji.",
        "Up to 5 traits, each of up to 24 characters and one emoji.",
    ),
    ("La reacción es un solo emoji.", "A reaction is a single emoji."),
    ("Solo un foro tiene etiquetas.", "Only a forum has tags."),
    (
        "Un foro tiene hasta 20 etiquetas, cada una con un nombre corto.",
        "A forum has up to 20 tags, each with a short name.",
    ),
    (
        "Una comunidad necesita verificación y revisar el contenido de todos.",
        "A community needs verification and scanning everyone's content.",
    ),
    // What the session loads.
    ("Conversación", "Conversation"),
    (
        "No se pudo interpretar el estado de la cuenta.",
        "Couldn't read the account's state.",
    ),
    ("Rol", "Role"),
    ("Servidor no disponible", "Server unavailable"),
    // Threads and forums.
    (
        "Abre el hilo otra vez antes de seguirlo o dejarlo.",
        "Open the thread again before following or leaving it.",
    ),
    ("El canal ya no está disponible.", "The channel is no longer available."),
    ("El hilo ya no está disponible.", "The thread is no longer available."),
    (
        "El nombre del hilo tiene de 1 a 100 caracteres, en una línea.",
        "A thread name has 1 to 100 characters, on one line.",
    ),
    ("En este canal no se abre ese hilo.", "That thread can't be opened in this channel."),
    ("Esa etiqueta es solo para la moderación.", "That tag is for moderators only."),
    ("Esas etiquetas no son del foro.", "Those tags aren't from the forum."),
    ("Este foro pide al menos una etiqueta.", "This forum requires at least one tag."),
    (
        "La publicación supera los archivos o el tamaño permitidos.",
        "The post exceeds the allowed files or size.",
    ),
    (
        "Solo las publicaciones de un foro llevan etiquetas.",
        "Only a forum's posts have tags.",
    ),
    ("Solo un foro tiene publicaciones.", "Only a forum has posts."),
    // Verification (captcha) of an action.
    (
        "Cancelaste la verificación; no se envió nada.",
        "You canceled the verification; nothing was sent.",
    ),
    (
        "Discord pidió una verificación mientras otra esperaba; vuelve a intentarlo después.",
        "Discord asked for a verification while another was waiting; try again later.",
    ),
    (
        "Discord volvió a pedir una verificación; la acción no se repite otra vez.",
        "Discord asked for a verification again; the action won't be repeated again.",
    ),
    (
        "No se completó la verificación; no se envió nada.",
        "The verification wasn't completed; nothing was sent.",
    ),
    (
        "Se perdió la conexión durante la verificación; no se envió nada.",
        "The connection was lost during the verification; nothing was sent.",
    ),
    (
        "Venció el plazo de la verificación; no se envió nada.",
        "The verification expired; nothing was sent.",
    ),
    // The Gateway.
    (
        "Discord cerró la conexión Gateway. El intento se detuvo.",
        "Discord closed the Gateway connection. The attempt stopped.",
    ),
    (
        "Discord envió un evento Gateway incompatible.",
        "Discord sent an incompatible Gateway event.",
    ),
    (
        "Discord limitó la conexión Gateway. Espera antes de volver a conectar.",
        "Discord rate limited the Gateway connection. Wait before connecting again.",
    ),
    ("Discord rechazó la conexión Gateway.", "Discord rejected the Gateway connection."),
    (
        "Discord rechazó la sesión Gateway. Inicia sesión otra vez.",
        "Discord rejected the Gateway session. Sign in again.",
    ),
    (
        "Gateway solicitó demasiadas respuestas. La conexión se detuvo.",
        "The Gateway asked for too many replies. The connection stopped.",
    ),
    (
        "La cola de eventos de Discord se llenó. Vuelve a conectar.",
        "Discord's event queue filled up. Connect again.",
    ),
    ("No se pudo preparar Gateway.", "Couldn't prepare the Gateway."),
    (
        "No se pudo preparar el reintento de Gateway.",
        "Couldn't prepare the Gateway retry.",
    ),
    (
        "Un evento de Discord excedió el límite local.",
        "A Discord event exceeded the local limit.",
    ),
    // Requests to Discord.
    ("Discord no pudo confirmar esta acción.", "Discord couldn't confirm this action."),
    (
        "Discord pidió esperar antes de otra solicitud.",
        "Discord asked to wait before another request.",
    ),
    (
        "Discord pidió esperar. La acción no se reintentó.",
        "Discord asked to wait. The action wasn't retried.",
    ),
    (
        "Discord rechazó la sesión; inicia sesión otra vez.",
        "Discord rejected the session; sign in again.",
    ),
    ("Discord rechazó los datos de esta acción.", "Discord rejected this action's data."),
    (
        "El canal o mensaje ya no está disponible.",
        "The channel or message is no longer available.",
    ),
    ("El transporte no está disponible.", "The transport isn't available."),
    (
        "Esta integración no admite cuentas de bot.",
        "This integration doesn't support bot accounts.",
    ),
    (
        "Este canal tiene más mensajes fijados de los que RUNE puede leer.",
        "This channel has more pinned messages than RUNE can read.",
    ),
    (
        "La respuesta de Discord excede el límite local.",
        "Discord's response exceeds the local limit.",
    ),
    ("La respuesta de Discord quedó incompleta.", "Discord's response was incomplete."),
    ("La sesión local recibida no es válida.", "The local session received isn't valid."),
    ("La verificación recibida no es válida.", "The verification received isn't valid."),
    (
        "No se pudo confirmar la respuesta de Discord.",
        "Couldn't confirm Discord's response.",
    ),
    (
        "No se pudo conectar con Discord. Revisa la conexión a internet.",
        "Couldn't connect to Discord. Check your internet connection.",
    ),
    ("Discord no respondió a tiempo.", "Discord didn't respond in time."),
    ("No se pudo iniciar el cliente HTTPS.", "Couldn't start the HTTPS client."),
    (
        "No tienes permiso para completar esta acción.",
        "You don't have permission to complete this action.",
    ),
    (
        "Discord no confirmó el cambio; se volvió a leer cómo quedó.",
        "Discord didn't confirm the change; how it ended up was read again.",
    ),
    (
        "Discord respondió con ajustes de la cuenta que RUNE no entiende.",
        "Discord answered with account settings RUNE doesn't understand.",
    ),
    ("Ese cambio de privacidad no es válido.", "That privacy change isn't valid."),
    (
        "Ese cambio no entra en los ajustes de la cuenta.",
        "That change doesn't fit in the account settings.",
    ),
    (
        "Las carpetas cambiaron en otro dispositivo. Ya se ven como quedaron: vuelve a intentarlo.",
        "The folders changed on another device. They now show as they are: try again.",
    ),
    ("Esa invitación no es válida.", "That invite isn't valid."),
    ("Ese webhook ya no existe.", "That webhook no longer exists."),
    (
        "Muchas entradas seguidas no coinciden con el filtro; prueba con otro.",
        "Many entries in a row don't match the filter; try another one.",
    ),
    ("No se pudo preparar el sticker.", "Couldn't prepare the sticker."),
    (
        "Solo se cambian los webhooks creados en el servidor.",
        "Only webhooks created in the server can be changed.",
    ),
    (
        "No encontramos ese mensaje en este contexto.",
        "We couldn't find that message in this context.",
    ),
    (
        "Discord no reconoce la aplicación de la actividad.",
        "Discord doesn't recognize the activity's app.",
    ),
    (
        "La actividad es demasiado larga para publicarla.",
        "The activity is too long to publish.",
    ),
    ("La actividad no dice de qué aplicación es.", "The activity doesn't say which app it's from."),
    (
        "La lista de juegos de Discord no se pudo leer.",
        "Discord's game list couldn't be read.",
    ),
    ("No se pudo leer la lista de juegos de Discord.", "Couldn't read Discord's game list."),
    (
        "Discord devolvió paquetes de stickers que RUNE no lee.",
        "Discord returned sticker packs RUNE doesn't read.",
    ),
    (
        "El perfil tiene un texto demasiado largo o con caracteres no permitidos.",
        "The profile has text that's too long or has characters that aren't allowed.",
    ),
    (
        "Se guardó, pero no se pudo confirmar cómo quedó. Abre el perfil otra vez antes de volver a cambiarlo.",
        "It was saved, but how it ended up couldn't be confirmed. Open the profile again before changing it again.",
    ),
    ("Discord envió resultados incompatibles.", "Discord sent incompatible results."),
    // Refusals about people and conversations.
    (
        "Abriste demasiadas conversaciones nuevas; espera un rato.",
        "You opened too many new conversations; wait a while.",
    ),
    (
        "Discord limitó las solicitudes de amistad de esta cuenta.",
        "Discord limited this account's friend requests.",
    ),
    (
        "Discord requiere una verificación para completar esta acción.",
        "Discord requires a verification to complete this action.",
    ),
    (
        "Discord requiere verificar la cuenta en su aplicación oficial.",
        "Discord requires verifying the account in its official app.",
    ),
    ("El grupo ya tiene el máximo de personas.", "The group already has the most people allowed."),
    (
        "Esa persona no acepta solicitudes de amistad.",
        "That person doesn't accept friend requests.",
    ),
    (
        "Esa persona no recibe mensajes directos tuyos.",
        "That person doesn't receive direct messages from you.",
    ),
    ("Esa persona ya no existe.", "That person no longer exists."),
    (
        "Llegaste al máximo de amigos que permite Discord.",
        "You reached the most friends Discord allows.",
    ),
    (
        "No encontramos a nadie con ese nombre de usuario.",
        "We couldn't find anyone with that username.",
    ),
    ("No puedes enviarte una solicitud a ti.", "You can't send a request to yourself."),
    (
        "No se pueden enviar solicitudes de amistad a un bot.",
        "Friend requests can't be sent to a bot.",
    ),
    ("Ya son amigos.", "You're already friends."),
    (
        "El nombre puede tener hasta 100 caracteres, sin saltos de línea.",
        "A name can have up to 100 characters, without line breaks.",
    ),
    (
        "Escribe un nombre de usuario válido: de 2 a 32 letras minúsculas, números, «_» o «.».",
        "Enter a valid username: 2 to 32 lowercase letters, numbers, “_” or “.”.",
    ),
    (
        "Esta conexión no deja un grupo sin nombre: escribe uno.",
        "This connection doesn't allow a group without a name: enter one.",
    ),
    // Uploads.
    (
        "Discord no indicó dónde guardar el archivo; no se envió nada.",
        "Discord didn't say where to store the file; nothing was sent.",
    ),
    (
        "El almacenamiento rechazó el archivo; no se envió nada.",
        "The storage rejected the file; nothing was sent.",
    ),
    (
        "No se pudo subir el archivo; no se envió nada.",
        "Couldn't upload the file; nothing was sent.",
    ),
    ("GIF de Discord", "Discord GIF"),
    // Apps and interactions.
    ("Aplicación", "App"),
    ("Carga el mensaje otra vez antes de usarlo.", "Load the message again before using it."),
    ("El comando no tiene esa opción.", "The command doesn't have that option."),
    ("El formulario ya no está abierto.", "The form is no longer open."),
    ("Ese control ya no se puede usar.", "That control can no longer be used."),
    ("Ese subcomando ya no existe.", "That subcommand no longer exists."),
    ("Esta interacción falló.", "This interaction failed."),
    ("Este mensaje no pertenece a una aplicación.", "This message doesn't belong to an app."),
    ("La aplicación no respondió a tiempo.", "The app didn't respond in time."),
    ("La conversación no está disponible.", "The conversation isn't available."),
    (
        "La interacción es demasiado grande: no se envió.",
        "The interaction is too large: it wasn't sent.",
    ),
    (
        "No tienes permiso para usar aplicaciones en el canal.",
        "You don't have permission to use apps in the channel.",
    ),
    ("Vuelve a cargar los comandos: este cambió.", "Load the commands again: this one changed."),
    // Messages.
    (
        "Esta compilación no incluye la ventana de verificación.",
        "This build doesn't include the verification window.",
    ),
    ("La encuesta no está completa.", "The poll isn't complete."),
    (
        "No tienes permiso para esta acción en esa conversación.",
        "You don't have permission for this action in that conversation.",
    ),
    ("Carga la encuesta otra vez antes de votar.", "Load the poll again before voting."),
    (
        "Carga la encuesta otra vez: solo quien la creó la termina.",
        "Load the poll again: only whoever created it can end it.",
    ),
    (
        "El mensaje debe contener entre 1 y 2000 caracteres.",
        "A message must have between 1 and 2000 characters.",
    ),
    ("Esa reacción no está disponible.", "That reaction isn't available."),
    ("Esas respuestas no valen para la encuesta.", "Those answers aren't valid for the poll."),
    ("Esta acción todavía no está disponible.", "This action isn't available yet."),
    ("Este hilo está archivado.", "This thread is archived."),
    ("Este hilo está bloqueado.", "This thread is locked."),
    (
        "Este sticker no está disponible en el servidor.",
        "This sticker isn't available in the server.",
    ),
    ("La encuesta ya terminó.", "The poll already ended."),
    (
        "No tienes permiso para esta acción en el canal.",
        "You don't have permission for this action in the channel.",
    ),
    (
        "Solo puedes editar un mensaje propio cargado en RUNE.",
        "You can only edit your own message loaded in RUNE.",
    ),
    ("Solo puedes editar un mensaje propio.", "You can only edit your own message."),
    (
        "Tu cuenta está restringida en este servidor: puedes leer, pero no participar.",
        "Your account is restricted in this server: you can read, but not take part.",
    ),
    ("Ya no estás en ese servidor.", "You're no longer in that server."),
    (
        "Tu cuenta está restringida en este servidor: no puedes cambiar tu perfil ahí.",
        "Your account is restricted in this server: you can't change your profile there.",
    ),
    // The provider: session, keyring, connection.
    ("Busca con un texto de hasta 50 caracteres.", "Search with a text of up to 50 characters."),
    ("Cambiando de cuenta.", "Switching accounts."),
    (
        "El inicio de sesión venció. Puedes iniciar otro intento.",
        "The sign-in expired. You can start another attempt.",
    ),
    (
        "El llavero del sistema no respondió a tiempo.",
        "The system keyring didn't respond in time.",
    ),
    (
        "El mensaje o la sesión cambió antes de completar la acción.",
        "The message or the session changed before the action was completed.",
    ),
    (
        "Hay demasiadas acciones pendientes. Vuelve a intentarlo.",
        "There are too many pending actions. Try again.",
    ),
    ("La conexión con Discord terminó.", "The connection to Discord ended."),
    ("La conexión todavía no está disponible.", "The connection isn't available yet."),
    (
        "La sesión guardada ya no es válida. Inicia sesión otra vez.",
        "The saved session is no longer valid. Sign in again.",
    ),
    (
        "La sesión o la conexión ya no está disponible.",
        "The session or the connection is no longer available.",
    ),
    ("La sesión ya no está disponible.", "The session is no longer available."),
    (
        "No se encontró la sesión guardada de esa cuenta. Entra con «Usar otra cuenta» u olvidala.",
        "The saved session of that account wasn't found. Sign in with “Use another account” or forget it.",
    ),
    (
        "No se pudo borrar la sesión guardada: {error}",
        "Couldn't delete the saved session: {error}",
    ),
    ("No se pudo preparar una sesión segura.", "Couldn't prepare a secure session."),
    (
        "No se pudo recordar la sesión: {error} Sigue abierta hasta que cierres RUNE.",
        "Couldn't remember the session: {error} It stays open until you close RUNE.",
    ),
    ("Se agotó el tiempo de la verificación.", "The verification timed out."),
    ("Sesión cerrada en este equipo.", "Signed out on this computer."),
    (
        "Sesión personal, recordada en el llavero del sistema si hay uno. Conversaciones, mensajes, voz en canales de servidor y mirar transmisiones y cámaras, compartir la pantalla y enviar la cámara (sin probar todavía con una llamada real), llamadas en privados y grupos (sin probar todavía contra Discord), invitaciones con verificación, amigos, privados, grupos, hilos, foros, administración de servidores, lecturas sincronizadas y búsqueda en todo el historial (sin probar todavía con una cuenta real).",
        "Personal session, remembered in the system keyring if there is one. Conversations, messages, voice in server channels and watching streams and cameras, sharing your screen and sending your camera (not yet tried with a real call), calls in direct messages and groups (not yet tried against Discord), invites with verification, friends, direct messages, groups, threads, forums, server administration, synced read states and search across the whole history (not yet tried with a real account).",
    ),
    // Reading history.
    (
        "Esa conversación ya no está disponible para leer.",
        "That conversation is no longer available to read.",
    ),
    ("Esa conversación ya no está disponible.", "That conversation is no longer available."),
    ("Esa página de resultados no existe.", "That page of results doesn't exist."),
    ("No puedes leer uno de los canales elegidos.", "You can't read one of the chosen channels."),
    (
        "No tienes permiso para leer el historial de esa conversación.",
        "You don't have permission to read that conversation's history.",
    ),
    // Friends and groups.
    ("El grupo ya tiene 10 personas.", "The group already has 10 people."),
    ("Esa conversación ya no está en tu lista.", "That conversation is no longer in your list."),
    ("Esa persona no es válida para esta acción.", "That person isn't valid for this action."),
    ("Esa persona ya está en el grupo.", "That person is already in the group."),
    ("Esa persona ya no está en el grupo.", "That person is no longer in the group."),
    ("Ese grupo ya no está en tu lista.", "That group is no longer in your list."),
    (
        "Esta acción todavía no está disponible en esta conexión.",
        "This action isn't available yet on this connection.",
    ),
    ("No hay ninguna relación con esa persona.", "There's no relationship with that person."),
    ("No hay una solicitud de esa persona.", "There's no request from that person."),
    ("Solo puedes añadir a tus amigos.", "You can only add your friends."),
    ("Solo quien es dueño del grupo puede hacerlo.", "Only the group owner can do that."),
    (
        "Solo se crea un grupo con amigos, cada uno una vez.",
        "A group is only created with friends, each one once.",
    ),
    ("Solo tus amigos pueden tener un apodo.", "Only your friends can have a nickname."),
    ("Un grupo nuevo lleva de 2 a 9 amigos.", "A new group takes 2 to 9 friends."),
    // Stages.
    ("Entra al escenario primero.", "Join the stage first."),
    ("Esa persona ya no está en el escenario.", "That person is no longer on the stage."),
    ("Ese escenario no está disponible.", "That stage isn't available."),
    (
        "Espera que la moderación te invite a hablar.",
        "Wait for the moderators to invite you to speak.",
    ),
    (
        "No tienes permiso para pedir la palabra en este escenario.",
        "You don't have permission to request to speak on this stage.",
    ),
    (
        "Solo la moderación invita a hablar o pasa al público.",
        "Only moderators invite to speak or move to the audience.",
    ),
    ("Ya puedes hablar en este escenario.", "You can already speak on this stage."),
    // Voice and streams.
    ("Discord cerró la conexión de voz.", "Discord closed the voice connection."),
    (
        "Discord eligió otro códec de video para la transmisión.",
        "Discord chose another video codec for the stream.",
    ),
    ("Discord envió un servidor de voz inválido.", "Discord sent an invalid voice server."),
    (
        "Discord exige cifrado de extremo a extremo que no se pudo negociar.",
        "Discord requires end-to-end encryption that couldn't be negotiated.",
    ),
    (
        "Discord no acordó cifrado de extremo a extremo; la llamada no empezó.",
        "Discord didn't agree on end-to-end encryption; the call didn't start.",
    ),
    (
        "Discord no le dio lugar a la pantalla en la transmisión.",
        "Discord didn't make room for the screen in the stream.",
    ),
    ("Discord rechazó la sesión de voz.", "Discord rejected the voice session."),
    ("Discord te desconectó de la llamada.", "Discord disconnected you from the call."),
    ("El servidor de voz dejó de responder.", "The voice server stopped responding."),
    ("El servidor de voz eligió otro cifrado.", "The voice server chose another encryption."),
    ("El servidor de voz envió demasiados mensajes.", "The voice server sent too many messages."),
    ("El servidor de voz envió un mensaje inválido.", "The voice server sent an invalid message."),
    (
        "El servidor de voz indicó una dirección no pública.",
        "The voice server gave a non-public address.",
    ),
    (
        "El servidor de voz no aceptó el cifrado de RUNE.",
        "The voice server didn't accept RUNE's encryption.",
    ),
    (
        "El servidor de voz no ofrece el cifrado requerido.",
        "The voice server doesn't offer the required encryption.",
    ),
    (
        "El servidor de voz no respondió por UDP; revisa el cortafuegos.",
        "The voice server didn't respond over UDP; check the firewall.",
    ),
    ("El servidor de voz no respondió.", "The voice server didn't respond."),
    ("El servidor de voz no retomó la llamada.", "The voice server didn't resume the call."),
    ("El servidor de voz se reinició.", "The voice server restarted."),
    ("La conexión de voz no responde.", "The voice connection isn't responding."),
    ("La llamada agotó sus claves. Vuelve a entrar.", "The call ran out of keys. Join again."),
    ("La sesión de voz terminó. Vuelve a entrar.", "The voice session ended. Join again."),
    ("No se pudo abrir la conexión de audio.", "Couldn't open the audio connection."),
    ("No se pudo acordar el cifrado de la llamada.", "Couldn't agree on the call's encryption."),
    ("No se pudo conectar con el servidor de voz.", "Couldn't connect to the voice server."),
    ("No se pudo preparar la llamada.", "Couldn't prepare the call."),
    ("No se pudo recuperar la conexión de voz.", "Couldn't recover the voice connection."),
    ("Se cortó la conexión de voz.", "The voice connection dropped."),
    ("Se perdió la conexión de audio de la llamada.", "The call's audio connection was lost."),
    (
        "Discord pidió quitar el cifrado de la llamada; se cortó.",
        "Discord asked to remove the call's encryption; it was cut off.",
    ),
    ("Discord sacó a esta cuenta de la llamada.", "Discord removed this account from the call."),
    (
        "El cifrado de la llamada falló varias veces. Vuelve a entrar.",
        "The call's encryption failed several times. Join again.",
    ),
    (
        "El servidor de voz anunció un participante inválido.",
        "The voice server announced an invalid participant.",
    ),
    (
        "El servidor de voz envió un mensaje de cifrado inválido.",
        "The voice server sent an invalid encryption message.",
    ),
    (
        "El servidor de voz envió una clave de grupo inválida.",
        "The voice server sent an invalid group key.",
    ),
    (
        "El servidor de voz propuso un cambio de grupo inválido.",
        "The voice server proposed an invalid group change.",
    ),
    (
        "La llamada cambió de clave demasiadas veces. Vuelve a entrar.",
        "The call changed keys too many times. Join again.",
    ),
    ("La llamada superó el límite de participantes.", "The call exceeded the participant limit."),
    ("No se pudo preparar el cifrado de la llamada.", "Couldn't prepare the call's encryption."),
    ("No se pudo reiniciar el cifrado de la llamada.", "Couldn't restart the call's encryption."),
    ("Discord envió una transmisión inválida.", "Discord sent an invalid stream."),
    (
        "Discord limitó las transmisiones de este servidor.",
        "Discord limited this server's streams.",
    ),
    (
        "Discord no asignó un servidor de voz a tiempo.",
        "Discord didn't assign a voice server in time.",
    ),
    (
        "Discord no conectó la transmisión a tiempo.",
        "Discord didn't connect the stream in time.",
    ),
    (
        "Discord no te dejó transmitir en este canal.",
        "Discord didn't let you stream in this channel.",
    ),
    (
        "Entra a la llamada para compartir la pantalla.",
        "Join the call to share your screen.",
    ),
    ("Entra a la llamada para encender la cámara.", "Join the call to turn on your camera."),
    ("Entra a la llamada para ver la transmisión.", "Join the call to watch the stream."),
    ("Esa calidad de transmisión no existe.", "That stream quality doesn't exist."),
    (
        "Esa persona no está transmitiendo en tu llamada.",
        "That person isn't streaming in your call.",
    ),
    ("Ese canal de voz no está disponible.", "That voice channel isn't available."),
    ("Esta ventana no puede compartir la pantalla.", "This window can't share your screen."),
    ("Esta ventana no puede enviar la cámara.", "This window can't send your camera."),
    ("Esta ventana no puede mostrar transmisiones.", "This window can't show streams."),
    ("Esta ventana no puede reproducir llamadas.", "This window can't play calls."),
    ("La llamada terminó.", "The call ended."),
    ("La transmisión está llena.", "The stream is full."),
    ("La transmisión terminó.", "The stream ended."),
    (
        "No tienes permiso para ver esta transmisión.",
        "You don't have permission to watch this stream.",
    ),
    ("Te desconectaron de la llamada.", "You were disconnected from the call."),
    ("Te movieron a otro canal de voz.", "You were moved to another voice channel."),
    ("Tu llamada siguió en otro dispositivo.", "Your call continued on another device."),
    (
        "Discord devolvió datos incompatibles con esta versión de RUNE.",
        "Discord returned data incompatible with this version of RUNE.",
    ),
    // OAuth2.
    (
        "Demasiadas respuestas de autorización inválidas. Vuelve a intentarlo.",
        "Too many invalid authorization responses. Try again.",
    ),
    (
        "La autorización fue rechazada o cancelada en Discord.",
        "The authorization was denied or canceled on Discord.",
    ),
    ("No se pudo obtener aleatoriedad segura.", "Couldn't get secure randomness."),
    (
        "No se pudo recibir la respuesta del navegador.",
        "Couldn't receive the browser's response.",
    ),
    ("Respuesta local demasiado grande.", "Local response too large."),
    ("Respuesta local ilegible.", "Unreadable local response."),
    ("Respuesta local incompleta.", "Incomplete local response."),
    ("Respuesta local inválida.", "Invalid local response."),
    (
        "Respuesta recibida. Vuelve a RUNE para ver el resultado.",
        "Response received. Go back to RUNE to see the result.",
    ),
    (
        "Se agotó el tiempo para autorizar. Vuelve a iniciar sesión.",
        "The time to authorize ran out. Sign in again.",
    ),
    (
        "Discord devolvió demasiados servidores en una página.",
        "Discord returned too many servers in one page.",
    ),
    (
        "Discord devolvió un error. No se reintentó automáticamente.",
        "Discord returned an error. It wasn't retried automatically.",
    ),
    ("Discord devolvió un identificador inválido.", "Discord returned an invalid ID."),
    ("Discord devolvió una caducidad inválida.", "Discord returned an invalid expiration."),
    ("Discord devolvió una respuesta inesperada.", "Discord returned an unexpected response."),
    ("Discord devolvió una sesión inválida.", "Discord returned an invalid session."),
    ("Discord no autorizó esta operación.", "Discord didn't authorize this operation."),
    (
        "Discord no concedió exactamente los permisos de identidad y servidores solicitados.",
        "Discord didn't grant exactly the identity and server permissions requested.",
    ),
    ("Discord no devolvió la identidad autorizada.", "Discord didn't return the authorized identity."),
    (
        "Discord pidió esperar antes de volver a intentar.",
        "Discord asked to wait before trying again.",
    ),
    (
        "Discord rechazó la autorización. Revisa Public Client, el ID y el redirect registrado; el canje público directo es experimental.",
        "Discord rejected the authorization. Check Public Client, the ID and the registered redirect; the direct public exchange is experimental.",
    ),
    (
        "El listado de servidores superó el límite de esta integración.",
        "The server list exceeded this integration's limit.",
    ),
    (
        "Esta integración requiere una identidad de usuario autorizada por OAuth2.",
        "This integration requires a user identity authorized through OAuth2.",
    ),
    (
        "La autorización de Discord caducó durante la conexión.",
        "Discord's authorization expired during the connection.",
    ),
    ("La autorización de Discord ya caducó.", "Discord's authorization already expired."),
    (
        "La autorización pertenece a otra aplicación.",
        "The authorization belongs to another app.",
    ),
    ("La paginación de servidores no avanzó.", "The server pagination didn't advance."),
    (
        "La respuesta de Discord superó el límite permitido.",
        "Discord's response exceeded the allowed limit.",
    ),
    (
        "No se pudo completar la conexión HTTPS con Discord.",
        "Couldn't complete the HTTPS connection to Discord.",
    ),
    ("No se pudo leer la respuesta de Discord.", "Couldn't read Discord's response."),
    (
        "El ID público de la aplicación debe ser un entero decimal distinto de cero.",
        "The app's public ID must be a nonzero decimal integer.",
    ),
    (
        "El ID público de la aplicación no debe tener ceros iniciales.",
        "The app's public ID must not have leading zeros.",
    ),
    (
        "El puerto local de autorización está ocupado. Cierra el otro intento y vuelve a intentar.",
        "The local authorization port is busy. Close the other attempt and try again.",
    ),
    (
        "La autorización caducó. Vuelve a iniciar sesión.",
        "The authorization expired. Sign in again.",
    ),
    (
        "La integración OAuth2 solo permite identidad y un listado básico de servidores; esta función no está habilitada.",
        "The OAuth2 integration only allows identity and a basic server list; this feature isn't enabled.",
    ),
    (
        "La sesión local ya está cerrada. No se confirmó la revocación remota; puedes quitar RUNE en Discord → Ajustes → Aplicaciones autorizadas.",
        "The local session is already closed. The remote revocation wasn't confirmed; you can remove RUNE in Discord → Settings → Authorized Apps.",
    ),
    ("Discord · OAuth2 experimental", "Discord · Experimental OAuth2"),
    (
        "OAuth2 experimental: identidad y listado básico de servidores. Sin mensajes ni voz. Canje público pendiente de validación con tu aplicación.",
        "Experimental OAuth2: identity and a basic server list. No messages or voice. Public exchange pending validation with your app.",
    ),
    // The login and verification windows.
    (
        "Esta compilación no incluye la ventana de inicio de sesión.",
        "This build doesn't include the sign-in window.",
    ),
    (
        "La respuesta de inicio de sesión superó el límite permitido.",
        "The sign-in response exceeded the allowed limit.",
    ),
    ("La ventana de inicio de sesión no respondió.", "The sign-in window didn't respond."),
    (
        "La ventana de inicio de sesión se cerró inesperadamente.",
        "The sign-in window closed unexpectedly.",
    ),
    (
        "La ventana de verificación se cerró inesperadamente.",
        "The verification window closed unexpectedly.",
    ),
    (
        "No se pudo abrir la ventana de inicio de sesión.",
        "Couldn't open the sign-in window.",
    ),
    ("No se pudo abrir la ventana de verificación.", "Couldn't open the verification window."),
    ("No se pudo localizar el ejecutable de RUNE.", "Couldn't find RUNE's executable."),
    ("Se agotó el tiempo de inicio de sesión.", "The sign-in timed out."),
    (
        "El navegador de la ventana de acceso falló. Vuelve a intentarlo.",
        "The sign-in window's browser failed. Try again.",
    ),
    (
        "El navegador de la ventana de acceso terminó por una causa no identificada.",
        "The sign-in window's browser ended for an unknown reason.",
    ),
    (
        "El navegador de la ventana de verificación falló. No se envió nada.",
        "The verification window's browser failed. Nothing was sent.",
    ),
    (
        "La aplicación pidió terminar el navegador de la ventana de acceso.",
        "The app asked to end the sign-in window's browser.",
    ),
    (
        "La respuesta de la ventana de acceso no es válida.",
        "The sign-in window's response isn't valid.",
    ),
    (
        "La respuesta de la verificación no es válida.",
        "The verification's response isn't valid.",
    ),
    (
        "La verificación no pudo cargarse. Puedes aceptar la invitación desde el navegador.",
        "The verification couldn't load. You can accept the invite from the browser.",
    ),
    (
        "No se pudo completar el inicio de sesión en la ventana temporal.",
        "Couldn't complete the sign-in in the temporary window.",
    ),
    (
        "No se pudo completar la verificación. No se envió nada.",
        "Couldn't complete the verification. Nothing was sent.",
    ),
    (
        "No se pudo obtener aleatoriedad segura para iniciar sesión.",
        "Couldn't get secure randomness to sign in.",
    ),
    (
        "Se agotó el tiempo de la verificación. No se envió nada.",
        "The verification timed out. Nothing was sent.",
    ),
    ("Se canceló el inicio de sesión.", "The sign-in was canceled."),
    (
        "Se cerró la verificación sin completarla. No se envió nada.",
        "The verification was closed without completing it. Nothing was sent.",
    ),
    (
        "El navegador cerró la ventana de acceso al superar su límite de memoria.",
        "The browser closed the sign-in window after it exceeded its memory limit.",
    ),
    ("Cargando la verificación…", "Loading the verification…"),
    // windows-login: WebView2, the browser of those windows on Windows.
    (
        "Falta WebView2 Runtime de Microsoft, el navegador de la ventana de acceso en Windows. Instálalo desde https://go.microsoft.com/fwlink/p/?LinkId=2124703 y vuelve a intentarlo.",
        "Microsoft's WebView2 Runtime, the browser of the sign-in window on Windows, is missing. Install it from https://go.microsoft.com/fwlink/p/?LinkId=2124703 and try again.",
    ),
    (
        "No se pudo leer la identidad del navegador de la ventana de acceso.",
        "Couldn't read the identity of the sign-in window's browser.",
    ),
    (
        "El navegador de la ventana de acceso dio una identidad no válida.",
        "The sign-in window's browser gave an invalid identity.",
    ),
    (
        "Completa la verificación para entrar al servidor.",
        "Complete the verification to join the server.",
    ),
    ("Empezar la verificación", "Start the verification"),
    (
        "La verificación no pudo cargarse. No se envió nada.",
        "The verification couldn't load. Nothing was sent.",
    ),
    (
        "Página propia de RUNE. Solo se envía la respuesta a esta solicitud; cerrar la ventana no envía nada.",
        "RUNE's own page. Only the answer to this request is sent; closing the window sends nothing.",
    ),
    ("RUNE · Iniciar sesión en Discord", "RUNE · Sign in to Discord"),
    ("RUNE · Verificación", "RUNE · Verification"),
    ("Verificación completa. Volviendo a RUNE…", "Verification complete. Going back to RUNE…"),
    ("Verificación para entrar al servidor", "Verification to join the server"),
    // The platform: autostart, file chooser, notifications.
    ("Cliente de chat nativo", "Native chat client"),
    (
        "El archivo o la respuesta del selector supera el límite permitido.",
        "The file or the picker's response exceeds the allowed limit.",
    ),
    (
        "El selector de archivos no está disponible en este equipo.",
        "The file picker isn't available on this computer.",
    ),
    (
        "El selector no devolvió un archivo local válido.",
        "The picker didn't return a valid local file.",
    ),
    (
        "El selector tardó demasiado. Puedes volver a intentarlo.",
        "The picker took too long. You can try again.",
    ),
    (
        "La selección tiene más archivos de los permitidos.",
        "The selection has more files than allowed.",
    ),
    ("La solicitud del selector no es válida.", "The picker's request isn't valid."),
    (
        "No se pudo obtener permiso para abrir el archivo.",
        "Couldn't get permission to open the file.",
    ),
    ("No se pudo preparar el archivo seleccionado.", "Couldn't prepare the selected file."),
    ("Se canceló la selección del archivo.", "The file selection was canceled."),
    (
        "Selecciona un archivo normal, no una carpeta o un dispositivo.",
        "Select a regular file, not a folder or a device.",
    ),
    ("Abrir", "Open"),
    ("Guardar archivo como", "Save file as"),
    ("Seleccionar archivo", "Select file"),
    ("Seleccionar archivos", "Select files"),
    // Screen sharing and the camera.
    ("El portal del escritorio no ofrece la cámara.", "The desktop portal doesn't offer the camera."),
    (
        "El portal del escritorio no permite compartir la pantalla.",
        "The desktop portal doesn't allow sharing your screen.",
    ),
    (
        "El portal del escritorio no respondió a tiempo.",
        "The desktop portal didn't respond in time.",
    ),
    (
        "El portal del escritorio respondió algo inválido.",
        "The desktop portal responded with something invalid.",
    ),
    ("El sistema no dio permiso.", "The system didn't give permission."),
    ("El sistema terminó la sesión.", "The system ended the session."),
    ("No hay ninguna cámara conectada.", "There's no camera connected."),
    (
        "No hay un portal del escritorio (xdg-desktop-portal) en esta sesión.",
        "There's no desktop portal (xdg-desktop-portal) in this session.",
    ),
    ("Se canceló en el diálogo del sistema.", "It was canceled in the system dialog."),
    // The keyring and global shortcuts.
    ("El llavero del sistema no está disponible.", "The system keyring isn't available."),
    ("El llavero del sistema no respondió.", "The system keyring didn't respond."),
    ("No se desbloqueó el llavero del sistema.", "The system keyring wasn't unlocked."),
    (
        "RUNE: sesión guardada de {name} ({service})",
        "RUNE: saved session of {name} ({service})",
    ),
    (
        "RUNE: clave de los datos locales ({account})",
        "RUNE: key to the local data ({account})",
    ),
    ("El escritorio cerró los atajos globales.", "The desktop closed the global shortcuts."),
    ("El escritorio no autorizó el atajo global.", "The desktop didn't authorize the global shortcut."),
    ("El escritorio no ofrece atajos globales.", "The desktop doesn't offer global shortcuts."),
    ("El portal de atajos globales no respondió.", "The global shortcuts portal didn't respond."),
    ("No hay portal del escritorio en esta sesión.", "There's no desktop portal in this session."),
    // The system tray.
    ("1 conversación sin leer", "1 unread conversation"),
    ("1 mención", "1 mention"),
    ("Ausente", "Idle"),
    ("En línea", "Online"),
    ("Invisible", "Invisible"),
    ("No molestar", "Do not disturb"),
    ("Todo leído", "All read"),
    ("{many} conversaciones sin leer", "{many} unread conversations"),
    ("{many} menciones", "{many} mentions"),
    ("Abrir RUNE", "Open RUNE"),
    ("Ensordecer", "Deafen"),
    ("Estado", "Status"),
    ("Salir de RUNE", "Quit RUNE"),
    ("Silenciar micrófono", "Mute microphone"),
];
