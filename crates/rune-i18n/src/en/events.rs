//! Scheduled events (eventos-programados): the model's checks, the entry
//! and the dialog (`rune-components` and `rune-ui` `events.rs`), the
//! invitation's card, and what the provider, the simulation and the
//! personal session say about them.

pub(super) const ENTRIES: super::Entries = &[
    // The model's checks.
    ("Poné un nombre al evento.", "Give the event a name."),
    (
        "El nombre del evento es demasiado largo.",
        "The event's name is too long.",
    ),
    (
        "La descripción es demasiado larga.",
        "The description is too long.",
    ),
    ("Escribí dónde es el evento.", "Write where the event is."),
    ("El lugar es demasiado largo.", "The place is too long."),
    (
        "Un evento en otro lugar necesita una hora de fin.",
        "An event somewhere else needs an end time.",
    ),
    (
        "El evento tiene que empezar más adelante.",
        "The event has to start later than now.",
    ),
    (
        "El evento tiene que terminar después de empezar.",
        "The event has to end after it starts.",
    ),
    // The entry and the dialog.
    ("Eventos", "Events"),
    ("Eventos ({count})", "Events ({count})"),
    ("Eventos de {server}", "{server}'s events"),
    ("Cargando eventos…", "Loading events…"),
    ("No hay eventos programados", "No scheduled events"),
    (
        "Cuando alguien planee uno, va a aparecer acá.",
        "When someone plans one, it will show up here.",
    ),
    ("En curso", "Happening now"),
    ("Terminó", "Ended"),
    ("Cancelado", "Canceled"),
    ("Le interesa a una persona", "One person is interested"),
    (
        "Les interesa a {count} personas",
        "{count} people are interested",
    ),
    ("Me interesa", "Interested"),
    ("Te interesa", "You're interested"),
    ("Crear evento", "Create event"),
    ("Editar evento", "Edit event"),
    ("Interesados", "Interested people"),
    (
        "Todavía no le interesa a nadie.",
        "Nobody is interested yet.",
    ),
    ("Lo creó {person}", "Created by {person}"),
    ("Iniciar", "Start"),
    ("Guardando…", "Saving…"),
    ("Borrar evento", "Delete event"),
    ("¿Borrar «{name}»?", "Delete “{name}”?"),
    (
        "Nadie va a poder verlo, y no se puede deshacer.",
        "Nobody will be able to see it, and it can't be undone.",
    ),
    ("¿Dónde es?", "Where is it?"),
    ("Escenario", "Stage"),
    ("Otro lugar", "Somewhere else"),
    ("Una dirección o un enlace", "An address or a link"),
    ("Dirección o enlace", "Address or link"),
    ("Nombre del evento", "Event name"),
    ("¿Qué van a hacer?", "What will you do?"),
    (
        "Contá de qué se trata (opcional)",
        "Say what it's about (optional)",
    ),
    ("Empieza", "Starts"),
    ("Termina", "Ends"),
    ("Con hora de fin", "With an end time"),
    (
        "Escribí la fecha y la hora de inicio enteras.",
        "Write the whole start date and time.",
    ),
    (
        "Escribí la fecha y la hora de fin enteras.",
        "Write the whole end date and time.",
    ),
    (
        "Ese evento ya no está disponible.",
        "That event is no longer available.",
    ),
    ("Un canal que ya no está", "A channel that is gone"),
    // The invitation's card.
    (
        "Te invitaron a un evento",
        "You've been invited to an event",
    ),
    ("Ver evento", "View event"),
    ("Un escenario del servidor", "A stage of the server"),
    (
        "Un canal de voz del servidor",
        "A voice channel of the server",
    ),
    // What the state says.
    (
        "Este proveedor no muestra eventos",
        "This provider doesn't show events",
    ),
    (
        "Este proveedor no permite crear eventos",
        "This provider doesn't allow creating events",
    ),
    (
        "Todavía no sos miembro de {server}",
        "You're not a member of {server} yet",
    ),
    (
        "No tenés permiso para crear eventos en {server}",
        "You don't have permission to create events in {server}",
    ),
    ("El evento ya terminó", "The event has ended"),
    (
        "Sólo quien lo creó o la moderación pueden cambiar este evento",
        "Only whoever created it or the moderators can change this event",
    ),
    // The permissions' catalog.
    ("Permisos de eventos", "Events permissions"),
    ("Crear eventos", "Create events"),
    (
        "Permite planear eventos y editar o borrar los que haya creado uno.",
        "Allows planning events, and editing or deleting the ones you created.",
    ),
    ("Gestionar eventos", "Manage events"),
    (
        "Permite editar, iniciar, terminar y borrar los eventos de cualquiera.",
        "Allows editing, starting, ending and deleting anyone's events.",
    ),
    // The provider's failures.
    ("cargar los eventos", "load the events"),
    ("cambiar el evento", "change the event"),
    (
        "marcar tu interés en el evento",
        "mark your interest in the event",
    ),
    // The simulation.
    ("el evento ya no existe", "the event no longer exists"),
    ("el evento ya terminó", "the event has ended"),
    (
        "el evento no puede pasar a ese estado",
        "the event can't go to that state",
    ),
    (
        "ese canal ya no está en el servidor",
        "that channel is no longer in the server",
    ),
    // The personal session.
    (
        "No tenés permiso para cambiar eventos en este servidor.",
        "You don't have permission to change events in this server.",
    ),
    (
        "El evento no está completo: revisalo.",
        "The event isn't complete: check it.",
    ),
    (
        "Un evento sólo se inicia o se termina.",
        "An event can only be started or ended.",
    ),
];
