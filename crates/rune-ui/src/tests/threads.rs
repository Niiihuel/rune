//! Threads under their channel and a forum's posts, opened, started,
//! followed, closed and paged as a person does, over the simulation.

use super::*;
use rune_provider::ThreadScope;

fn thread_named(host: &Host, name: &str) -> ChannelId {
    host.store
        .read_channels()
        .find(|channel| channel.name == name && channel.is_thread())
        .map(|channel| channel.id)
        .unwrap()
}

impl Host {
    /// What the simulation answers to what was sent from `from` on.
    fn th_answer(&mut self, from: usize) {
        let commands: Vec<Command> = self.sent[from..].to_vec();
        for command in commands {
            for event in self.provider.manage(command) {
                self.event(event);
            }
        }
    }

    fn th_type(&mut self, id: &str, value: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::InputChanged {
                id: id.into(),
                value: value.into(),
            },
            self.now,
        );
        self.run(intents);
    }

    fn th_right_click(&mut self, id: &str) {
        let frame = self.frame();
        let rect = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"))
            .rect;
        let at = Point::new(rect.x + 12.0, rect.y + rect.height / 2.0);
        self.send(UiEvent::ContextMenu { position: at });
    }

    fn th_rect(&mut self, id: &str) -> limn::Rect {
        let frame = self.frame();
        frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"))
            .rect
    }

    fn th_shown(&mut self, id: &str) -> bool {
        self.frame().regions.iter().any(|region| region.id == id)
    }
}

#[test]
fn a_forum_lists_its_posts_and_a_post_opens_as_a_thread_with_its_parent() {
    let mut host = Host::new();
    let forum = host.open_channel("consultas");
    assert!(
        host.store.timeline(forum).is_none(),
        "a forum has no history of its own"
    );
    let painted = host.painted();
    for text in [
        "¿Qué lente para retratos con poco presupuesto?",
        "Fotos movidas con poca luz",
        "❓ Pregunta",
        "Bloqueada",
        "Nueva publicación",
    ] {
        assert!(painted.iter().any(|t| t == text), "{text}: {painted:?}");
    }
    let frame = host.frame();
    assert!(
        !frame.regions.iter().any(|r| r.id == editor_id(forum)),
        "no composer in a forum"
    );

    let post = thread_named(&host, "¿Qué lente para retratos con poco presupuesto?");
    host.click(&format!("post:{post}"));
    assert_eq!(host.ui.channel(), Some(post));
    assert!(host
        .store
        .timeline(post)
        .is_some_and(|timeline| timeline.latest && !timeline.is_empty()));
    let painted = host.painted();
    assert!(
        painted.iter().any(|t| t == "consultas"),
        "the header names the forum: {painted:?}"
    );
    let frame = host.frame();
    assert!(frame.regions.iter().any(|r| r.id == editor_id(post)));
}

#[test]
fn a_locked_post_says_why_it_takes_no_reply() {
    let mut host = Host::new();
    host.open_channel("consultas");
    let locked = thread_named(&host, "Fotos movidas con poca luz");
    host.click(&format!("post:{locked}"));
    assert_eq!(host.ui.channel(), Some(locked));
    let reason = "Este hilo está bloqueado: solo la moderación puede responder";
    let painted = host.painted();
    assert!(painted.iter().any(|t| t.contains(reason)), "{painted:?}");
}

#[test]
fn followed_threads_hang_under_their_channel_and_open_from_the_column() {
    let mut host = Host::new();
    let gallery = host.open_channel("galería");
    let frame = host.frame();
    let threads: Vec<&str> = frame
        .regions
        .iter()
        .filter(|r| r.id.starts_with("thread:"))
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(threads.len(), 2, "{threads:?}");
    let saturday = thread_named(&host, "Salida fotográfica del sábado");
    let row = |id: String| {
        frame
            .regions
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.rect)
            .unwrap()
    };
    let (parent, child) = (
        row(format!("channel:{gallery}")),
        row(format!("thread:{saturday}")),
    );
    assert!(
        child.y > parent.y && child.x > parent.x,
        "{parent:?} {child:?}"
    );
    // A forum's posts are not in the column.
    let post = thread_named(&host, "Revelado RAW: Darktable o RawTherapee");
    assert!(!frame
        .regions
        .iter()
        .any(|r| r.id == format!("thread:{post}")));

    host.click(&format!("thread:{saturday}"));
    assert_eq!(host.ui.channel(), Some(saturday));
    assert!(host.store.timeline(saturday).is_some());
    let painted = host.painted();
    assert!(painted.iter().any(|t| t == "galería"), "{painted:?}");
    assert!(host
        .store
        .abilities(saturday)
        .is_some_and(|abilities| abilities.write.allowed()));

    // Unfollowed from its menu, it leaves the column once the provider says so.
    host.th_right_click(&format!("thread:{saturday}"));
    let painted = host.painted();
    for label in [
        "Dejar de seguir",
        "Marcar como leído",
        "Copiar enlace",
        "Cerrar hilo",
        "Eliminar hilo",
    ] {
        assert!(painted.iter().any(|t| t == label), "{label}: {painted:?}");
    }
    // Someone else's thread, in a club the account does not moderate.
    assert!(painted.iter().any(|t| t.starts_with("Solo la moderación")));
    let sent = host.sent.len();
    host.click(&format!("th:{saturday}:unfollow"));
    assert_eq!(
        host.sent[sent..],
        [Command::FollowThread {
            thread: saturday,
            follow: false
        }]
    );
    assert!(
        host.th_shown(&format!("thread:{saturday}")),
        "open: still shown"
    );
    host.th_answer(sent);
    host.click(&format!("channel:{gallery}"));
    assert!(!host.th_shown(&format!("thread:{saturday}")));
}

#[test]
fn the_list_of_threads_pages_open_then_archived_ones_once_each() {
    let mut host = Host::new();
    let gallery = host.open_channel("galería");
    // Opening the list asks for the open threads the session was not told of.
    let sent = host.sent.len();
    host.click(&format!("th:list:{gallery}"));
    let [Command::LoadThreads {
        scope: ThreadScope::Active,
        before: None,
        ..
    }] = &host.sent[sent..]
    else {
        panic!("{:?}", &host.sent[sent..]);
    };
    host.th_answer(sent);
    let painted = host.painted();
    for text in [
        "Seguidos — 2",
        "Activos — 0",
        "Archivados — 0",
        "Ver archivados",
    ] {
        assert!(painted.iter().any(|t| t == text), "{text}: {painted:?}");
    }
    let mut scopes = Vec::new();
    for _ in 0..3 {
        let sent = host.sent.len();
        if !host.th_shown(&format!("th:more:{gallery}")) {
            break;
        }
        host.click(&format!("th:more:{gallery}"));
        let [Command::LoadThreads { scope, .. }] = &host.sent[sent..] else {
            panic!("{:?}", &host.sent[sent..]);
        };
        scopes.push(*scope);
        // A second press while it loads asks for nothing more.
        let pressed = host.sent.len();
        if host.th_shown(&format!("th:more:{gallery}")) {
            host.click(&format!("th:more:{gallery}"));
        }
        assert_eq!(host.sent.len(), pressed);
        host.th_answer(sent);
    }
    assert_eq!(scopes, vec![ThreadScope::Archived, ThreadScope::Joined]);
    // Opened again, the list asks for nothing more.
    host.click("th:close:close");
    let sent = host.sent.len();
    host.click(&format!("th:list:{gallery}"));
    assert_eq!(host.sent.len(), sent);
    let painted = host.painted();
    for text in [
        "Archivados — 4",
        "Salida al puerto de agosto",
        "Organización del concurso de agosto",
        "No hay más hilos archivados",
    ] {
        assert!(painted.iter().any(|t| t == text), "{text}: {painted:?}");
    }
    assert!(!painted.iter().any(|t| t == "Jurado de septiembre"));
    // An archived thread opens from the list, which closes.
    let old = thread_named(&host, "Salida al puerto de agosto");
    host.click(&format!("thread:{old}"));
    assert_eq!(host.ui.channel(), Some(old));
    assert!(!host.th_shown(&format!("th:more:{gallery}")));
    // A reply would reopen it; saying one types would not.
    let abilities = host.store.abilities(old).unwrap();
    assert!(abilities.write.allowed() && !abilities.typing.allowed());
}

#[test]
fn a_thread_starts_from_the_channel_and_opens_once_the_provider_names_it() {
    let mut host = Host::new();
    let gallery = host.open_channel("galería");
    host.click(&format!("th:list:{gallery}"));
    host.click(&format!("th:start:{gallery}"));
    let painted = host.painted();
    assert!(
        painted.iter().any(|t| t == "Escribe un nombre"),
        "{painted:?}"
    );
    host.th_type("th:new:name", "Paseo nocturno");
    host.click("th:new:private");
    let sent = host.sent.len();
    host.click("th:new:create");
    let [Command::CreateThread {
        channel,
        message: None,
        name,
        private: true,
        ..
    }] = &host.sent[sent..]
    else {
        panic!("{:?}", &host.sent[sent..]);
    };
    assert_eq!((*channel, name.as_str()), (gallery, "Paseo nocturno"));
    // Nothing is made up meanwhile: no thread, the dialog waits.
    assert!(host
        .store
        .read_channels()
        .all(|c| c.name != "Paseo nocturno"));
    assert!(host
        .painted()
        .iter()
        .any(|t| t == "Esperando al proveedor…"));
    host.th_answer(sent);
    let made = thread_named(&host, "Paseo nocturno");
    assert_eq!(host.ui.channel(), Some(made));
    assert!(!host.th_shown("th:new:create"));
    let info = host.store.channel(made).unwrap().thread.clone().unwrap();
    assert!(info.private && info.joined);

    // From a message: its menu offers it, and the thread starts from it.
    host.click(&format!("channel:{gallery}"));
    let message = host
        .store
        .timeline(gallery)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .id;
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{message}:thread"),
        },
        host.now,
    );
    host.run(intents);
    let painted = host.painted();
    assert!(
        painted
            .iter()
            .any(|t| t.starts_with("En #galería, a partir del mensaje de")),
        "{painted:?}"
    );
    assert!(
        !host.th_shown("th:new:private"),
        "a thread of a message is public"
    );
    host.th_type("th:new:name", "Sobre esta foto");
    let sent = host.sent.len();
    host.click("th:new:create");
    assert!(matches!(&host.sent[sent..],
        [Command::CreateThread { message: Some(from), private: false, .. }] if *from == message));
}

#[test]
fn a_post_carries_its_tags_and_the_forum_orders_lays_out_and_filters_posts() {
    let mut host = Host::new();
    let forum = host.open_channel("consultas");
    let tags = host.store.channel(forum).unwrap().tags.clone();
    let open_tag = tags.iter().find(|t| !t.moderated).unwrap().id;
    let moderated = tags.iter().find(|t| t.moderated).unwrap().id;
    host.click(&format!("th:start:{forum}"));
    // Only moderation puts a moderated tag on, and that is not the account here.
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("th:new:tag:{}", moderated.0),
        },
        host.now,
    );
    host.run(intents);
    host.th_type("th:new:name", "¿Flash o luz continua?");
    host.click(&format!("th:new:tag:{}", open_tag.0));
    assert!(
        !host.th_shown("th:new:create"),
        "the first message is missing"
    );
    host.th_type("th:new:content", "Para retratos en casa.");
    let sent = host.sent.len();
    host.click("th:new:create");
    assert!(matches!(&host.sent[sent..],
        [Command::CreatePost { tags, title, .. }] if *tags == vec![open_tag] && title == "¿Flash o luz continua?"));
    host.th_answer(sent);
    let post = thread_named(&host, "¿Flash o luz continua?");
    assert_eq!(host.ui.channel(), Some(post));

    host.click(&format!("channel:{forum}"));
    // By creation, the newest post first.
    host.click(&format!("th:sort:{forum}:created"));
    let first = host.th_rect(&format!("post:{post}"));
    let older = thread_named(&host, "Revelado RAW: Darktable o RawTherapee");
    assert!(first.y < host.th_rect(&format!("post:{older}")).y);
    // Filtered by a tag, only its posts.
    let edition = tags.iter().find(|t| t.name == "Edición").unwrap().id;
    host.click(&format!("th:filter:{forum}:{}", edition.0));
    assert!(host.th_shown(&format!("post:{older}")));
    assert!(!host.th_shown(&format!("post:{post}")));
    // Two tags chosen: either of them, unless the forum asks for every
    // one (2026-10-19).
    host.click(&format!("th:filter:{forum}:{}", open_tag.0));
    assert!(host.th_shown(&format!("post:{post}")));
    let mut strict = host.store.channel(forum).unwrap().clone();
    strict.forum.match_all_tags = true;
    host.event(Event::ChannelChanged(strict));
    assert!(!host.th_shown(&format!("post:{post}")));
    host.click(&format!("th:filter:{forum}:clear"));
    // As a gallery, cards side by side: the two newest first.
    host.click(&format!("th:layout:{forum}:gallery"));
    let lens = thread_named(&host, "Fotos movidas con poca luz");
    let (a, b) = (
        host.th_rect(&format!("post:{post}")),
        host.th_rect(&format!("post:{lens}")),
    );
    assert!((a.y - b.y).abs() < 0.5 && a.x != b.x, "{a:?} {b:?}");
}

#[test]
fn a_post_takes_files_from_its_dialog_and_lets_them_go_when_closed() {
    use rune_model::{PendingAttachment, UploadToken};
    let mut host = Host::new();
    let forum = host.open_channel("consultas");
    host.click(&format!("th:start:{forum}"));
    host.click("th:new:attach");
    let Some(Intent::PickAttachment { request, channel }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    assert_eq!(channel, forum);
    let me = host.store.me().unwrap();
    let file = PendingAttachment {
        token: UploadToken(77),
        filename: "lente.jpg".into(),
        size_bytes: 2048,
    };
    let intents = crate::upload_update(
        &mut host.ui,
        &host.store,
        rune_provider::UploadUpdate::Prepared {
            account: me,
            channel: forum,
            request,
            result: Ok(vec![file]),
        },
    );
    host.run(intents);
    // Kept while the forum is there, as a composer's files are.
    host.event(Event::Typing {
        channel: forum,
        user: rune_model::UserId(11),
    });
    assert!(host.th_shown(&format!("composer:{forum}:upload:77:remove")));
    host.th_type("th:new:name", "¿Este lente?");
    // With a file, the first message may stay empty.
    let sent = host.sent.len();
    host.click("th:new:create");
    assert_eq!(host.sent.len(), sent, "the files go through the services");
    let Some(Intent::CreatePost { request, files, .. }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    assert_eq!(files, [UploadToken(77)]);
    // Its bytes show as they go, from none (envios-y-subidas).
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Enviando adjuntos: 0 B")));
    host.event(Event::PostProgress {
        request,
        sent_bytes: 1024,
        total_bytes: 2048,
    });
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Enviando adjuntos: 1") && text.contains("de 2")));
    // Refused before it went: it can go again, with its file.
    crate::post_refused(&mut host.ui, request, rune_provider::UploadError::Busy);
    assert!(host.th_shown("th:new:create"));
    assert!(host.th_shown(&format!("composer:{forum}:upload:77:remove")));
    // Closed: the file is let go.
    host.click(rune_components::threads::ids::NEW_CLOSE);
    assert!(host.upload_intents.contains(&Intent::RemoveAttachment {
        channel: forum,
        token: UploadToken(77),
    }));
}

#[test]
fn an_own_thread_is_renamed_closed_and_reopened_from_its_menu() {
    let mut host = Host::new();
    let gallery = host.open_channel("galería");
    host.click(&format!("th:list:{gallery}"));
    host.click(&format!("th:start:{gallery}"));
    host.th_type("th:new:name", "Propio");
    let sent = host.sent.len();
    host.click("th:new:create");
    host.th_answer(sent);
    let own = thread_named(&host, "Propio");

    host.th_right_click(&format!("thread:{own}"));
    host.click(&format!("th:{own}:edit"));
    host.th_type("th:edit:name", "Propio, renombrado");
    let sent = host.sent.len();
    host.click("th:edit:save");
    assert!(matches!(&host.sent[sent..],
        [Command::EditThread { edit, .. }] if edit.name.as_deref() == Some("Propio, renombrado") && edit.archived.is_none()));
    // Until the provider says so, it keeps its name.
    assert_eq!(host.store.channel(own).unwrap().name, "Propio");
    host.th_answer(sent);
    assert_eq!(host.store.channel(own).unwrap().name, "Propio, renombrado");

    host.th_right_click(&format!("thread:{own}"));
    let sent = host.sent.len();
    host.click(&format!("th:{own}:close"));
    host.th_answer(sent);
    assert!(
        host.store
            .channel(own)
            .unwrap()
            .thread
            .as_ref()
            .unwrap()
            .archived
    );
    // Locking is for moderation, even of an own thread.
    assert!(!host.store.thread_abilities(own).unwrap().lock.allowed());
    // Deleting asks first; here only moderation could.
    assert_eq!(
        host.store.thread_abilities(own).unwrap().delete.reason(),
        Some("Solo la moderación puede hacerlo en este servidor")
    );
}

impl Host {
    /// The controls of a window too narrow for every tool of the header.
    fn th_narrow(&mut self) -> Vec<String> {
        let size = Size::new(720.0, 700.0);
        self.tree.set_root(view(
            &self.ui,
            &self.store,
            &ViewContext {
                window: size,
                ..self.context()
            },
        ));
        let frame = self.tree.layout(size);
        frame
            .regions
            .iter()
            .map(|region| region.id.clone())
            .collect()
    }

    fn th_choose(&mut self, id: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents);
    }
}

#[test]
fn a_narrow_header_folds_its_inbox_and_threads_into_its_more_menu() {
    let mut host = Host::new();
    let gallery = host.open_channel("galería");
    let list = format!("th:list:{gallery}");
    let shown = host.th_narrow();
    assert!(shown.contains(&"cl:more".to_owned()), "{shown:?}");
    assert!(!shown.contains(&list) && !shown.contains(&"inbox:open".to_owned()));
    // Wide, each is a tool of its own and there is no «más».
    let wide: Vec<String> = host.frame().regions.iter().map(|r| r.id.clone()).collect();
    assert!(wide.contains(&list) && !wide.contains(&"cl:more".to_owned()));
    // Its menu offers both; choosing one closes it and does what the tool does.
    host.th_choose("cl:more");
    let menu = host.th_narrow();
    assert!(menu.contains(&list) && menu.contains(&"inbox:open".to_owned()));
    let sent = host.sent.len();
    host.th_choose(&list);
    assert_eq!(host.ui.channel_list.more, None);
    assert!(crate::threads::covering(&host.ui));
    assert!(matches!(&host.sent[sent..], [Command::LoadThreads { .. }]));
    // Pressed twice, it closes; anything else closes it too.
    host.th_choose("th:close:close");
    host.th_choose("cl:more");
    host.th_choose("cl:more");
    assert_eq!(host.ui.channel_list.more, None);
    host.th_choose("cl:more");
    host.th_choose("inbox:open");
    assert_eq!(host.ui.channel_list.more, None);
    assert!(host.ui.inbox.is_some());
}

#[test]
fn a_followed_thread_is_silenced_from_its_menu_and_marks_only_its_mentions() {
    let mut host = Host::new();
    host.open_channel("galería");
    let saturday = thread_named(&host, "Salida fotográfica del sábado");
    let wall = rune_provider::mock::wall_clock();
    host.th_right_click(&format!("thread:{saturday}"));
    assert!(host.painted().iter().any(|t| t == "Silenciar hilo"));
    let sent = host.sent.len();
    host.th_choose(&format!("th:{saturday}:mute:1h"));
    let [Command::MuteThread {
        thread,
        until: Some(until),
    }] = &host.sent[sent..]
    else {
        panic!("{:?}", &host.sent[sent..]);
    };
    assert_eq!(*thread, saturday);
    assert!(until.0 > wall.0);
    // Silenced once the provider says so.
    let muted_now = |host: &Host| {
        host.store
            .channel(saturday)
            .unwrap()
            .thread
            .as_ref()
            .unwrap()
            .muted(wall)
    };
    assert!(!muted_now(&host));
    host.th_answer(sent);
    assert!(muted_now(&host));
    let channel = host.store.channel(saturday).unwrap().clone();
    let (unread, _, muted) = crate::channel_list::marks(&host.ui, &host.store, &channel, wall);
    assert!(muted && !unread);
    // Its mentions alert nobody, as a muted channel's.
    let me = host.store.me().unwrap();
    let mut mention = host.provider.history(saturday, None, 1).0.pop().unwrap();
    mention.content = "<@1> mira".into();
    mention.mentions = vec![me];
    mention.author = channel.thread.as_ref().unwrap().owner.unwrap();
    host.ui.prefs.message_sounds = true;
    assert_eq!(
        crate::message_alert(&host.ui, &host.store, &mention, false, wall),
        None
    );
    // Its menu turns it back on.
    host.th_right_click(&format!("thread:{saturday}"));
    assert!(host
        .painted()
        .iter()
        .any(|t| t == "Dejar de silenciar el hilo"));
    let sent = host.sent.len();
    host.th_choose(&format!("th:{saturday}:unmute"));
    assert!(matches!(
        &host.sent[sent..],
        [Command::MuteThread { until: None, .. }]
    ));
    host.th_answer(sent);
    assert!(!muted_now(&host));
    assert!(crate::message_alert(&host.ui, &host.store, &mention, false, wall).is_some());
    // Only a thread it follows.
    let sent = host.sent.len();
    host.th_choose(&format!("th:{saturday}:unfollow"));
    host.th_answer(sent);
    assert_eq!(
        host.store.thread_abilities(saturday).unwrap().mute.reason(),
        Some("Sigue el hilo para silenciarlo")
    );
    let sent = host.sent.len();
    host.th_choose(&format!("th:{saturday}:mute:15m"));
    assert_eq!(host.sent.len(), sent);
}

#[test]
fn a_forums_cards_show_their_first_words_reaction_and_picture() {
    let mut host = Host::new();
    host.open_channel("consultas");
    let lens = thread_named(&host, "¿Qué lente para retratos con poco presupuesto?");
    let starter = host
        .store
        .channel(lens)
        .unwrap()
        .thread
        .as_ref()
        .unwrap()
        .starter
        .clone()
        .expect("the simulation reads each card's first message");
    assert!(!starter.excerpt.is_empty() && starter.image.is_some());
    let painted = host.painted();
    assert!(
        painted.contains(&starter.excerpt) || {
            // Cut to two lines, it starts the same.
            let head: String = starter.excerpt.chars().take(20).collect();
            painted.iter().any(|t| t.starts_with(&head))
        },
        "{painted:?}"
    );
    assert!(painted.iter().any(|t| t == "👍"));
    // Its picture is asked for while its card is in view.
    let wanted = crate::wanted_images(&host.ui, &host.store, &host.tree);
    assert!(
        wanted.contains(starter.image.as_ref().unwrap()),
        "{wanted:?}"
    );
    // A blocked author's words and picture stay folded, as in the timeline.
    let owner = host
        .store
        .channel(lens)
        .unwrap()
        .thread
        .as_ref()
        .unwrap()
        .owner;
    host.event(Event::RelationshipChanged(rune_model::Relationship {
        user: owner.unwrap(),
        kind: rune_model::RelationshipKind::Blocked,
        since: None,
        nickname: None,
        spam: false,
        ignored: false,
    }));
    let head: String = starter.excerpt.chars().take(20).collect();
    assert!(!host.painted().iter().any(|t| t.starts_with(&head)));
    let wanted = crate::wanted_images(&host.ui, &host.store, &host.tree);
    assert!(!wanted.contains(starter.image.as_ref().unwrap()));
}

#[test]
fn a_silenced_threads_header_bell_hears_it_again() {
    let mut host = Host::new();
    host.open_channel("galería");
    let saturday = thread_named(&host, "Salida fotográfica del sábado");
    let sent = host.sent.len();
    host.th_choose(&format!("th:{saturday}:mute:1h"));
    host.th_answer(sent);
    host.click(&format!("thread:{saturday}"));
    assert_eq!(host.ui.channel(), Some(saturday));
    // The bell shows its silence, and ends it with its own menu's ids.
    host.click(rune_components::channel_menu::ids::BELL);
    let painted = host.painted();
    assert!(
        painted.iter().any(|t| t == "Dejar de silenciar el hilo"),
        "{painted:?}"
    );
    let sent = host.sent.len();
    host.click(&format!("th:{saturday}:unmute"));
    assert!(matches!(
        &host.sent[sent..],
        [Command::MuteThread { until: None, .. }]
    ));
    assert_eq!(
        host.ui.channel_list.bell, None,
        "choosing it closes the bell"
    );
    host.th_answer(sent);
    host.click(rune_components::channel_menu::ids::BELL);
    assert!(!host
        .painted()
        .iter()
        .any(|t| t == "Dejar de silenciar el hilo"));
}

#[test]
fn an_open_forum_asks_again_for_its_first_page_after_a_new_session() {
    let mut host = Host::new();
    let forum = host.open_channel("consultas");
    let sent = host.sent.len();
    let snapshot = host.provider.snapshot();
    host.event(Event::Ready(Box::new(snapshot)));
    assert!(
        host.sent[sent..].iter().any(|command| matches!(command,
            Command::LoadThreads { parent, scope: ThreadScope::Active, before: None, .. }
                if *parent == forum)),
        "{:?}",
        &host.sent[sent..]
    );
}

#[test]
fn a_forums_cards_mark_unread_as_the_column_does_at_their_level() {
    let mut host = Host::new();
    let forum = host.open_channel("consultas");
    let wall = rune_provider::mock::wall_clock();
    let lens = thread_named(&host, "¿Qué lente para retratos con poco presupuesto?");
    let mut news = host.provider.history(lens, None, 1).0.pop().unwrap();
    news.id = MessageId(news.id.0 + 1_000_000);
    news.author = rune_model::UserId(424_242);
    host.event(Event::MessageCreated {
        message: news,
        nonce: None,
    });
    let marked = |host: &Host| {
        let forum = host.store.channel(forum).unwrap();
        crate::threads::forum_view(&host.ui, &host.store, forum, wall)
            .cards
            .iter()
            .any(|card| card.thread == lens.0 && card.unread)
    };
    assert!(host.store.read(lens).unread() && marked(&host));
    // Only mentions: an unread post is not marked, as its column row is not.
    let sent = host.sent.len();
    host.th_choose(&format!("cl:notify:{forum}:mentions"));
    host.th_answer(sent);
    let post = host.store.channel(lens).unwrap().clone();
    assert!(host.store.read(lens).unread() && !marked(&host));
    assert!(!crate::channel_list::marks(&host.ui, &host.store, &post, wall).0);
}
