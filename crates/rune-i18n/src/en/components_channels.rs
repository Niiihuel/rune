//! Channel settings, the channel menu and folders (`rune-components`).

pub(super) const ENTRIES: super::Entries = &[
    // Slowmode.
    ("Apagado", "Off"),
    ("1 segundo", "1 second"),
    ("5 segundos", "5 seconds"),
    ("10 segundos", "10 seconds"),
    ("15 segundos", "15 seconds"),
    ("30 segundos", "30 seconds"),
    ("{seconds} segundos", "{seconds} seconds"),
    ("1 minuto", "1 minute"),
    ("2 minutos", "2 minutes"),
    ("5 minutos", "5 minutes"),
    ("10 minutos", "10 minutes"),
    ("15 minutos", "15 minutes"),
    ("30 minutos", "30 minutes"),
    ("1 hora", "1 hour"),
    ("2 horas", "2 hours"),
    ("6 horas", "6 hours"),
    ("{minutes} min {seconds} s", "{minutes} min {seconds} s"),
    ("Inmune al modo lento", "Immune to slowmode"),
    (
        "Modo lento: un mensaje cada {wait}",
        "Slowmode: one message every {wait}",
    ),
    (
        "El modo lento está activado. Los miembros pueden enviar un mensaje cada {wait}.",
        "Slowmode is on. Members can send one message every {wait}.",
    ),
    // Permission groups.
    (
        "Permisos generales de los canales",
        "General channel permissions",
    ),
    (
        "Permisos generales de las categorías",
        "General category permissions",
    ),
    ("Permisos de membresía", "Membership permissions"),
    (
        "Permisos de los canales de texto",
        "Text channel permissions",
    ),
    (
        "Permisos de los canales de voz",
        "Voice channel permissions",
    ),
    // Permissions.
    ("Ver el canal", "View channel"),
    ("Ver canales", "View channels"),
    ("Gestionar canal", "Manage channel"),
    ("Gestionar canales", "Manage channels"),
    ("Gestionar permisos", "Manage permissions"),
    ("Crear invitación", "Create invite"),
    ("Enviar mensajes", "Send messages"),
    ("Enviar mensajes en hilos", "Send messages in threads"),
    ("Crear hilos públicos", "Create public threads"),
    ("Crear hilos privados", "Create private threads"),
    ("Insertar enlaces", "Embed links"),
    ("Adjuntar archivos", "Attach files"),
    ("Crear encuestas", "Create polls"),
    ("Añadir reacciones", "Add reactions"),
    ("Usar emojis externos", "Use external emoji"),
    (
        "Mencionar a @everyone, @here y a todos los roles",
        "Mention @everyone, @here and all roles",
    ),
    ("Gestionar mensajes", "Manage messages"),
    ("Gestionar hilos", "Manage threads"),
    ("Leer el historial de mensajes", "Read message history"),
    ("Conectar", "Connect"),
    ("Hablar", "Speak"),
    ("Video", "Video"),
    ("Usar Actividad de voz", "Use voice activity"),
    ("Prioridad de palabra", "Priority speaker"),
    ("Silenciar miembros", "Mute members"),
    ("Ensordecer miembros", "Deafen members"),
    ("Mover miembros", "Move members"),
    ("Pedir la palabra", "Request to speak"),
    // What each permission lets do: in a channel.
    (
        "Deja que los miembros vean este canal.",
        "Lets members see this channel.",
    ),
    (
        "Deja cambiar el nombre, el tema y los ajustes de este canal, y también eliminarlo.",
        "Lets members change this channel's name, topic and settings, and also delete it.",
    ),
    (
        "Deja cambiar los permisos de este canal.",
        "Lets members change this channel's permissions.",
    ),
    (
        "Deja invitar gente nueva al servidor con un enlace que lleva a este canal.",
        "Lets members invite new people to the server with a link that leads to this channel.",
    ),
    (
        "Deja escribir en este canal.",
        "Lets members write in this channel.",
    ),
    (
        "Deja escribir en los hilos de este canal.",
        "Lets members write in this channel's threads.",
    ),
    (
        "Deja abrir hilos que todos en este canal pueden ver.",
        "Lets members start threads everyone in this channel can see.",
    ),
    (
        "Deja abrir hilos que solo ven quienes son invitados y la moderación.",
        "Lets members start threads only invited people and moderators can see.",
    ),
    (
        "Los enlaces que se compartan aquí muestran una vista previa.",
        "Links shared here show a preview.",
    ),
    (
        "Deja subir archivos e imágenes a este canal.",
        "Lets members upload files and images to this channel.",
    ),
    (
        "Deja empezar encuestas en este canal.",
        "Lets members start polls in this channel.",
    ),
    (
        "Deja poner reacciones nuevas a los mensajes de este canal. Sin este permiso se puede sumar a las que ya están.",
        "Lets members add new reactions to this channel's messages. Without it, they can still join reactions already there.",
    ),
    (
        "Deja usar en este canal emojis de otros servidores.",
        "Lets members use emoji from other servers in this channel.",
    ),
    (
        "Deja avisar a todo el canal o a quienes estén en línea, y mencionar cualquier rol aunque no sea mencionable.",
        "Lets members notify the whole channel or whoever is online, and mention any role even if it isn't mentionable.",
    ),
    (
        "Deja borrar mensajes de otros miembros en este canal.",
        "Lets members delete other members' messages in this channel.",
    ),
    (
        "Deja renombrar, cerrar y borrar los hilos de este canal, y ver los privados.",
        "Lets members rename, close and delete this channel's threads, and see private ones.",
    ),
    (
        "Deja leer lo que se escribió antes. Sin este permiso solo se ve lo que llega mientras se está en línea.",
        "Lets members read what was written before. Without it, they only see what arrives while they are online.",
    ),
    (
        "Deja que los miembros vean este canal de voz.",
        "Lets members see this voice channel.",
    ),
    (
        "Deja cambiar el nombre y los ajustes de este canal de voz, y también eliminarlo.",
        "Lets members change this voice channel's name and settings, and also delete it.",
    ),
    (
        "Deja invitar gente nueva al servidor con un enlace que lleva a este canal de voz.",
        "Lets members invite new people to the server with a link that leads to this voice channel.",
    ),
    (
        "Deja entrar a este canal de voz y escuchar.",
        "Lets members join this voice channel and listen.",
    ),
    (
        "Deja hablar en este canal. Sin este permiso se entra silenciado hasta que alguien con «Silenciar miembros» lo permita.",
        "Lets members speak in this channel. Without it, they join muted until someone with “Mute members” allows them.",
    ),
    (
        "Deja compartir la cámara o la pantalla en este canal.",
        "Lets members share their camera or screen in this channel.",
    ),
    (
        "Deja hablar sin pulsar una tecla. Sin este permiso hay que usar pulsar para hablar.",
        "Lets members speak without pressing a key. Without it, they have to use push to talk.",
    ),
    (
        "Mientras habla, baja el volumen de quienes no tienen este permiso.",
        "While they speak, lowers the volume of those who don't have this permission.",
    ),
    (
        "Deja silenciar a otros en este canal, para todos.",
        "Lets members mute others in this channel, for everyone.",
    ),
    (
        "Deja que otros no puedan hablar ni escuchar en este canal.",
        "Lets members stop others from speaking or listening in this channel.",
    ),
    (
        "Deja desconectar a otros de este canal o llevarlos a otro al que se tenga acceso.",
        "Lets members disconnect others from this channel or move them to another one they have access to.",
    ),
    (
        "Deja levantar la mano para hablar, si este canal es un escenario.",
        "Lets members raise their hand to speak, if this channel is a stage.",
    ),
    // What each permission lets do: in a category's channels.
    (
        "Deja que los miembros vean los canales de esta categoría.",
        "Lets members see this category's channels.",
    ),
    (
        "Deja cambiar el nombre y los ajustes de estos canales, y también eliminarlos.",
        "Lets members change these channels' names and settings, and also delete them.",
    ),
    (
        "Deja cambiar los permisos de estos canales.",
        "Lets members change these channels' permissions.",
    ),
    (
        "Deja invitar gente nueva al servidor con un enlace que lleva a estos canales.",
        "Lets members invite new people to the server with a link that leads to these channels.",
    ),
    (
        "Deja escribir en estos canales.",
        "Lets members write in these channels.",
    ),
    (
        "Deja escribir en los hilos de estos canales.",
        "Lets members write in these channels' threads.",
    ),
    (
        "Deja abrir hilos que todos en estos canales pueden ver.",
        "Lets members start threads everyone in these channels can see.",
    ),
    (
        "Los enlaces que se compartan en estos canales muestran una vista previa.",
        "Links shared in these channels show a preview.",
    ),
    (
        "Deja subir archivos e imágenes a estos canales.",
        "Lets members upload files and images to these channels.",
    ),
    (
        "Deja empezar encuestas en estos canales.",
        "Lets members start polls in these channels.",
    ),
    (
        "Deja poner reacciones nuevas a los mensajes de estos canales. Sin este permiso se puede sumar a las que ya están.",
        "Lets members add new reactions to these channels' messages. Without it, they can still join reactions already there.",
    ),
    (
        "Deja usar en estos canales emojis de otros servidores.",
        "Lets members use emoji from other servers in these channels.",
    ),
    (
        "Deja avisar a todos o a quienes estén en línea, y mencionar cualquier rol aunque no sea mencionable, en estos canales.",
        "Lets members notify everyone or whoever is online, and mention any role even if it isn't mentionable, in these channels.",
    ),
    (
        "Deja borrar mensajes de otros miembros en estos canales.",
        "Lets members delete other members' messages in these channels.",
    ),
    (
        "Deja renombrar, cerrar y borrar los hilos de estos canales, y ver los privados.",
        "Lets members rename, close and delete these channels' threads, and see private ones.",
    ),
    (
        "Deja leer lo que se escribió antes en estos canales. Sin este permiso solo se ve lo que llega mientras se está en línea.",
        "Lets members read what was written before in these channels. Without it, they only see what arrives while they are online.",
    ),
    (
        "Deja entrar a estos canales de voz y escuchar.",
        "Lets members join these voice channels and listen.",
    ),
    (
        "Deja hablar en estos canales. Sin este permiso se entra silenciado hasta que alguien con «Silenciar miembros» lo permita.",
        "Lets members speak in these channels. Without it, they join muted until someone with “Mute members” allows them.",
    ),
    (
        "Deja compartir la cámara o la pantalla en estos canales.",
        "Lets members share their camera or screen in these channels.",
    ),
    (
        "Deja silenciar a otros en estos canales, para todos.",
        "Lets members mute others in these channels, for everyone.",
    ),
    (
        "Deja que otros no puedan hablar ni escuchar en estos canales.",
        "Lets members stop others from speaking or listening in these channels.",
    ),
    (
        "Deja desconectar a otros de estos canales o llevarlos a otro al que se tenga acceso.",
        "Lets members disconnect others from these channels or move them to another one they have access to.",
    ),
    (
        "Deja levantar la mano para hablar en los escenarios.",
        "Lets members raise their hand to speak in stages.",
    ),
    // What «Ver el canal» adds for @everyone.
    (
        "Si se lo quitas a @everyone, el canal de voz pasa a ser privado.",
        "If you take it away from @everyone, the voice channel becomes private.",
    ),
    (
        "Si se lo quitas a @everyone, los canales de la categoría pasan a ser privados.",
        "If you take it away from @everyone, the category's channels become private.",
    ),
    (
        "Si se lo quitas a @everyone, el canal pasa a ser privado.",
        "If you take it away from @everyone, the channel becomes private.",
    ),
    // The control of three states.
    ("denegar", "deny"),
    ("heredar", "inherit"),
    ("permitir", "allow"),
    // The permissions page's cards.
    (
        "Permisos sincronizados con la categoría: ",
        "Permissions synced with the category: ",
    ),
    (
        "Los permisos no están sincronizados con la categoría: ",
        "Permissions are not synced with the category: ",
    ),
    ("Sincronizar ahora", "Sync now"),
    ("Categoría privada", "Private category"),
    (
        "Si la haces privada, solo los roles y miembros que elijas van a poder verla. Los canales sincronizados con ella toman los mismos permisos.",
        "If you make it private, only the roles and members you choose will be able to see it. Channels synced with it take the same permissions.",
    ),
    ("Canal privado", "Private channel"),
    (
        "Si lo haces privado, solo los roles y miembros que elijas van a poder verlo.",
        "If you make it private, only the roles and members you choose will be able to see it.",
    ),
    (
        "¿Quién puede acceder a esta categoría?",
        "Who can access this category?",
    ),
    (
        "¿Quién puede acceder a este canal?",
        "Who can access this channel?",
    ),
    ("Añadir miembros o roles", "Add members or roles"),
    ("Roles", "Roles"),
    ("No hay roles", "No roles"),
    ("Miembros", "Members"),
    ("Quitarle el acceso a {name}", "Remove access for {name}"),
    ("Quitar", "Remove"),
    // The advanced view.
    ("Permisos avanzados", "Advanced permissions"),
    ("ROLES/MIEMBROS", "ROLES/MEMBERS"),
    ("Añadir un rol o un miembro", "Add a role or member"),
    ("Añadir:", "Add:"),
    ("Rol/Miembro", "Role/Member"),
    ("Buscar un rol o un miembro", "Search for a role or member"),
    ("Nada para añadir.", "Nothing to add."),
    ("Rol", "Role"),
    // The header's bell and pins.
    ("Ajustes de notificaciones", "Notification settings"),
    ("Mensajes fijados", "Pinned messages"),
    (
        "No se pudieron cargar los mensajes fijados",
        "Couldn't load pinned messages",
    ),
    ("Volver a intentar", "Try again"),
    ("Todavía no hay mensajes fijados", "No pinned messages yet"),
    (
        "Los mensajes que se fijen en esta conversación van a aparecer aquí.",
        "Messages pinned in this conversation will show up here.",
    ),
    ("Ir", "Jump"),
    ("Desfijar mensaje", "Unpin message"),
    ("Tema del canal", "Channel topic"),
    // Duplicating and deleting a channel.
    ("canal-nuevo", "new-channel"),
    ("Nombre del canal", "Channel name"),
    ("Se va a llamar #{name}", "It will be called #{name}"),
    (
        "Se va a crear un canal nuevo con los mismos permisos, tema y modo lento que {original}.",
        "A new channel will be created with the same permissions, topic and slowmode as {original}.",
    ),
    ("Duplicar canal", "Duplicate channel"),
    ("en {category}", "in {category}"),
    ("Crear un canal", "Create channel"),
    ("Eliminar canal", "Delete channel"),
    ("Eliminar categoría", "Delete category"),
    (
        "¿Seguro que quieres eliminar #{name}? No se puede deshacer.",
        "Are you sure you want to delete #{name}? This can't be undone.",
    ),
    (
        "¿Seguro que quieres eliminar {name}? No se puede deshacer.",
        "Are you sure you want to delete {name}? This can't be undone.",
    ),
    (
        "¿Seguro que quieres eliminar la categoría {name}? Sus canales quedan sin categoría. No se puede deshacer.",
        "Are you sure you want to delete the category {name}? Its channels will be left without a category. This can't be undone.",
    ),
    // The column of channels.
    ("Editar canal", "Edit channel"),
    // Folders.
    ("Azul", "Blue"),
    ("Ajustes de carpeta", "Folder settings"),
    ("Nombre de la carpeta", "Folder name"),
    ("Color de la carpeta", "Folder color"),
    (
        "Tiene {count} caracteres; el máximo es {max}.",
        "It has {count} characters; the maximum is {max}.",
    ),
    (
        "Vacío, se llama como sus servidores. Se guarda en tu cuenta.",
        "Left empty, it's named after its servers. It's saved to your account.",
    ),
    ("Guardar", "Save"),
    // The member list.
    ("Cargando miembro", "Loading member"),
    ("En el teléfono", "On mobile"),
    ("Dueño del servidor", "Server owner"),
    // Reactions, pins and search (2026-10-18).
    ("Fijar mensajes", "Pin messages"),
    (
        "Deja fijar y desfijar mensajes en este canal.",
        "Lets members pin and unpin messages in this channel.",
    ),
    (
        "Deja fijar y desfijar mensajes en estos canales.",
        "Lets members pin and unpin messages in these channels.",
    ),
];
