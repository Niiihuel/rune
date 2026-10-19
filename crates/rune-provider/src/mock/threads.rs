//! The simulation's threads and its forum: a forum of posts with tags, and a
//! few threads under a text channel, all made up, some of them archived. They
//! use their own random stream and ids after every other channel, so the rest
//! of the world is the same with or without them.
//!
//! Like a service, the simulation starts sessions with the active threads
//! only: the archived ones are read a page at a time, after a page of the
//! open ones (which it already reported). It starts threads and
//! posts, follows, closes, locks, renames, tags and deletes them as the
//! account's permissions allow, and a reply reopens an archived thread that
//! is not locked.

use std::collections::HashMap;

use rune_i18n::{t, tr};
use rune_model::{
    Attachment, AttachmentId, Channel, ChannelId, ChannelKind, ForumDefaults, ForumTag, ImageKind,
    MessageId, Permissions, PostStarter, Reaction, RemoteImage, ServerId, TagId, Thread, Timestamp,
    UserId, MAX_APPLIED_TAGS, MAX_THREAD_NAME,
};

use super::{mix, ChannelSim, World, ME};
use crate::{Command, Created, Event, ThreadEdit, ThreadScope};

type Outcome = Result<Vec<Event>, String>;

/// Mixed into a thread's stream, apart from every other one of the world.
const SALT: u64 = 0x7448_5244;

/// The forum's own reaction, which its cards show.
const REACTION: &str = "👍";

/// Archived threads of one page. Small, so a few of them already ask for a
/// second page.
pub(super) const THREAD_PAGE: usize = 5;

/// The forum's tags: name, emoji, only for moderation.
const TAGS: [(&str, &str, bool); 5] = [
    ("Pregunta", "❓", false),
    ("Resuelto", "✅", true),
    ("Equipo", "📷", false),
    ("Edición", "🎨", false),
    ("Principiantes", "🌱", false),
];

/// A thread of the plan.
struct Plan {
    title: &'static str,
    messages: u64,
    /// The forum's tags it carries, by index.
    tags: &'static [usize],
    archived: bool,
    locked: bool,
    private: bool,
    joined: bool,
}

const fn plan(title: &'static str, messages: u64, tags: &'static [usize]) -> Plan {
    Plan {
        title,
        messages,
        tags,
        archived: false,
        locked: false,
        private: false,
        joined: false,
    }
}

/// The forum's posts, then `#galería`'s threads; the archived ones after
/// both, so the ids of the active ones stay as they were.
const POSTS: [Plan; 4] = [
    Plan {
        joined: true,
        ..plan(
            "¿Qué lente para retratos con poco presupuesto?",
            24,
            &[0, 2],
        )
    },
    plan("Revelado RAW: Darktable o RawTherapee", 12, &[3, 4]),
    Plan {
        locked: true,
        joined: true,
        ..plan("Fotos movidas con poca luz", 31, &[0, 1])
    },
    Plan {
        archived: true,
        ..plan("Propuesta: un tema por mes", 8, &[4])
    },
];

const THREADS: [Plan; 2] = [
    Plan {
        joined: true,
        ..plan("Salida fotográfica del sábado", 40, &[])
    },
    Plan {
        private: true,
        joined: true,
        ..plan("Jurado del concurso de octubre", 15, &[])
    },
];

const ARCHIVED_POSTS: [Plan; 7] = [
    Plan {
        archived: true,
        ..plan("¿Trípode de viaje o monopié?", 9, &[0, 2])
    },
    Plan {
        archived: true,
        joined: true,
        ..plan("Limpiar el sensor sin miedo", 14, &[0, 1])
    },
    Plan {
        archived: true,
        ..plan("Mejor hora para fotografiar el río", 6, &[4])
    },
    Plan {
        archived: true,
        ..plan("Compartan su primera foto", 22, &[4])
    },
    Plan {
        archived: true,
        locked: true,
        ..plan("Filtros ND: ¿valen la pena?", 11, &[0, 2])
    },
    Plan {
        archived: true,
        ..plan("Calibrar el monitor en Linux", 7, &[3])
    },
    Plan {
        archived: true,
        ..plan("Concurso de septiembre: resultados", 5, &[1])
    },
];

const ARCHIVED_THREADS: [Plan; 5] = [
    Plan {
        archived: true,
        ..plan("Salida al puerto de agosto", 18, &[])
    },
    Plan {
        archived: true,
        joined: true,
        ..plan("Retratos en el parque", 10, &[])
    },
    Plan {
        archived: true,
        ..plan("Revelado colectivo de julio", 6, &[])
    },
    Plan {
        archived: true,
        private: true,
        joined: true,
        ..plan("Organización del concurso de agosto", 8, &[])
    },
    // Private and without the account: only moderation would ever see it.
    Plan {
        archived: true,
        private: true,
        ..plan("Jurado de septiembre", 4, &[])
    },
];

impl World {
    /// Add the forum to the photography club's category and the threads to
    /// its `#galería`, with ids from `next_channel` on.
    pub(super) fn add_threads(&mut self, next_channel: &mut u64) {
        let server = ServerId(4);
        let find = |world: &World, kind: ChannelKind, name: &str| {
            world
                .channels
                .iter()
                .find(|c| c.server == Some(server) && c.kind == kind && c.name == name)
                .map(|c| c.id)
        };
        let (Some(category), Some(general)) = (
            find(self, ChannelKind::Category, "Fotos"),
            find(self, ChannelKind::Text, "galería"),
        ) else {
            return;
        };
        let forum = ChannelId(*next_channel);
        *next_channel += 1;
        self.channels.push(Channel {
            id: forum,
            server: Some(server),
            kind: ChannelKind::Forum,
            name: "consultas".into(),
            topic: Some("Una publicación por pregunta. Usá etiquetas.".into()),
            topic_pictures: Vec::new(),
            parent: Some(category),
            // After the category's two text channels.
            position: 2,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: TAGS
                .iter()
                .enumerate()
                .map(|(index, (name, emoji, moderated))| ForumTag {
                    id: TagId(forum.0 * 100 + index as u64),
                    name: (*name).into(),
                    emoji: Some((*emoji).into()),
                    moderated: *moderated,
                })
                .collect(),
            forum: ForumDefaults {
                reaction: Some(REACTION.into()),
                ..Default::default()
            },
        });
        let in_forum = |plans: &'static [Plan]| plans.iter().map(move |plan| (forum, plan));
        let in_gallery = |plans: &'static [Plan]| plans.iter().map(move |plan| (general, plan));
        let plans = in_forum(&POSTS)
            .chain(in_gallery(&THREADS))
            .chain(in_forum(&ARCHIVED_POSTS))
            .chain(in_gallery(&ARCHIVED_THREADS));
        for (index, (parent, plan)) in plans.enumerate() {
            let id = ChannelId(*next_channel);
            *next_channel += 1;
            let mut rng = mix(&[self.seed, parent.0, index as u64, SALT]);
            let people: Vec<UserId> = self
                .members
                .iter()
                .filter(|m| m.server == server && m.user != ME)
                .map(|m| m.user)
                .collect();
            let owner = people[rng.below(people.len() as u64) as usize];
            self.sims.insert(
                id,
                ChannelSim {
                    generated: plan.messages,
                    sitting_gap: 90 + rng.range(0, 600),
                    extra: Vec::new(),
                    overlay: HashMap::new(),
                },
            );
            self.channels.push(Channel {
                id,
                server: Some(server),
                kind: ChannelKind::Thread,
                name: plan.title.into(),
                topic: None,
                topic_pictures: Vec::new(),
                parent: Some(parent),
                position: 0,
                recipients: Vec::new(),
                owner: None,
                permissions: Permissions::NONE,
                // Its permissions are its parent's: see `refresh_permissions`.
                overwrites: Vec::new(),
                slowmode_secs: 0,
                nsfw: false,
                icon: None,
                thread: Some(Thread {
                    archived: plan.archived,
                    locked: plan.locked,
                    private: plan.private,
                    joined: plan.joined,
                    owner: Some(owner),
                    messages: plan.messages as u32,
                    members: rng.range(3, 14) as u32,
                    last_activity: Timestamp::default(),
                    tags: plan
                        .tags
                        .iter()
                        .map(|tag| TagId(forum.0 * 100 + *tag as u64))
                        .collect(),
                    ..Thread::default()
                }),
                tags: Vec::new(),
                forum: Default::default(),
            });
        }
        // Their last activity is their last made-up message.
        let threads: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|c| c.is_thread())
            .map(|c| c.id)
            .collect();
        for id in threads {
            let last = self.sims[&id].generated - 1;
            let when = self
                .message(id, super::message_id(id, last))
                .map(|m| m.timestamp);
            if let (Some(when), Some(thread)) = (when, self.thread_mut(id)) {
                thread.last_activity = when;
            }
        }
        self.add_starters(forum);
    }

    /// What the cards of `forum` show of each post's first message: every
    /// other one carries a picture, and each some of the forum's reaction
    /// (2026-10-18). The message itself carries them too.
    fn add_starters(&mut self, forum: ChannelId) {
        let posts: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|c| c.is_thread() && c.parent == Some(forum))
            .map(|c| c.id)
            .collect();
        for (index, id) in posts.into_iter().enumerate() {
            let first = super::message_id(id, 0);
            let Some(mut message) = self.message(id, first) else {
                continue;
            };
            if index % 2 == 0 {
                message.attachments.push(Attachment {
                    id: AttachmentId(first.0 + 1),
                    filename: "foto.jpg".into(),
                    size_bytes: 180_000,
                    content_type: Some("image/jpeg".into()),
                    dimensions: Some((1200, 800)),
                    preview: Some(RemoteImage {
                        url: format!("https://simulacion.invalid/foros/{index}.jpg"),
                        kind: ImageKind::AttachmentPreview,
                    }),
                    download: None,
                    spoiler: false,
                    voice: None,
                    placeholder: None,
                    explicit: false,
                });
            }
            message.reactions.push(Reaction {
                emoji: REACTION.into(),
                count: 2 + (index as u32 * 3) % 7,
                me: false,
                burst: 0,
                me_burst: false,
                burst_colors: Vec::new(),
            });
            let starter = Some(PostStarter::of(&message));
            if let Some(sim) = self.sims.get_mut(&id) {
                sim.overlay.insert(first, Some(message));
            }
            if let Some(thread) = self.thread_mut(id) {
                thread.starter = starter;
            }
        }
    }

    fn thread_mut(&mut self, id: ChannelId) -> Option<&mut Thread> {
        self.channels
            .iter_mut()
            .find(|c| c.id == id && c.is_thread())
            .and_then(|c| c.thread.as_mut())
    }

    /// Whether `channel` is an archived thread: sessions start without them.
    pub(super) fn archived_thread(channel: &Channel) -> bool {
        channel
            .thread
            .as_ref()
            .is_some_and(|thread| thread.archived)
    }

    /// Carry out a command about threads and posts.
    pub(super) fn thread_command(&mut self, command: Command) -> Outcome {
        match command {
            Command::CreateThread {
                request,
                channel,
                message,
                name,
                private,
            } => self.create_thread(request, channel, message, name, private),
            Command::CreatePost {
                request,
                forum,
                title,
                tags,
                content,
                files,
            } => self.create_post(request, forum, title, tags, content, &files),
            Command::LoadThreads {
                request,
                parent,
                scope,
                before,
            } => self.threads_page(request, parent, scope, before),
            Command::FollowThread { thread, follow } => self.follow(thread, follow),
            Command::EditThread { thread, edit } => self.edit_thread(thread, edit),
            Command::DeleteThread { thread } => self.delete_thread(thread),
            Command::MuteThread { thread, until } => self.mute_thread(thread, until),
            _ => Err(t("la simulación no sabe hacer eso").into()),
        }
    }

    /// A reply to `channel` goes through when it is not a locked thread
    /// (unless the account moderates threads); it reopens an archived one,
    /// reported before the message.
    pub(super) fn thread_reply(&mut self, channel: ChannelId) -> Result<Vec<Event>, String> {
        let Some(found) = self.channel(channel).filter(|c| c.is_thread()) else {
            return Ok(Vec::new());
        };
        let info = found.thread.clone().unwrap_or_default();
        let moderates = found.permissions.contains(Permissions::MANAGE_THREADS);
        if !found.permissions.contains(Permissions::SEND_IN_THREADS) {
            return Err(t("no tenés permiso para escribir en este hilo").into());
        }
        if info.locked && !moderates {
            return Err(t("el hilo está bloqueado").into());
        }
        if !info.archived {
            return Ok(Vec::new());
        }
        let thread = self.thread_mut(channel).ok_or(t("el hilo no existe"))?;
        thread.archived = false;
        Ok(self.reported(channel).into_iter().collect())
    }

    /// A message written in `channel` counts in its thread, if it is one.
    pub(super) fn count_reply(&mut self, channel: ChannelId, at: Timestamp) {
        if let Some(thread) = self.thread_mut(channel) {
            thread.messages = thread.messages.saturating_add(1);
            thread.last_activity = thread.last_activity.max(at);
        }
    }

    fn reported(&self, channel: ChannelId) -> Option<Event> {
        self.channel(channel).cloned().map(Event::ChannelChanged)
    }

    /// The thread `id` the account sees, with its permissions.
    fn seen_thread(&self, id: ChannelId) -> Result<&Channel, String> {
        let found = self
            .channel(id)
            .filter(|c| c.is_thread())
            .ok_or(t("el hilo no existe"))?;
        let hidden = found.thread.as_ref().is_some_and(|thread| {
            thread.private
                && !thread.joined
                && !found.permissions.contains(Permissions::MANAGE_THREADS)
        });
        if !found.permissions.contains(Permissions::VIEW) || hidden {
            return Err(t("el hilo no existe").into());
        }
        Ok(found)
    }

    /// Make a thread under `parent`, owned by the account, which is in it.
    fn add_thread(
        &mut self,
        parent: &Channel,
        name: String,
        private: bool,
        tags: Vec<TagId>,
    ) -> Result<ChannelId, String> {
        let server = parent.server.ok_or(t("el canal no existe"))?;
        let id = ChannelId(self.fresh_id());
        self.sims.insert(
            id,
            ChannelSim {
                generated: 0,
                sitting_gap: 60,
                extra: Vec::new(),
                overlay: HashMap::new(),
            },
        );
        self.channels.push(Channel {
            id,
            server: Some(server),
            kind: ChannelKind::Thread,
            name,
            topic: None,
            topic_pictures: Vec::new(),
            parent: Some(parent.id),
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: Some(Thread {
                private,
                joined: true,
                owner: Some(ME),
                members: 1,
                last_activity: self.now,
                tags,
                ..Thread::default()
            }),
            tags: Vec::new(),
            forum: Default::default(),
        });
        self.refresh_permissions(server);
        Ok(id)
    }

    fn create_thread(
        &mut self,
        request: crate::RequestId,
        channel: ChannelId,
        message: Option<MessageId>,
        name: String,
        private: bool,
    ) -> Outcome {
        let parent = self
            .channel(channel)
            .filter(|c| c.permissions.contains(Permissions::VIEW))
            .cloned()
            .ok_or(t("el canal no existe"))?;
        if !matches!(parent.kind, ChannelKind::Text | ChannelKind::Announcement) {
            return Err(t("en este canal no se abren hilos").into());
        }
        if private && (message.is_some() || parent.kind != ChannelKind::Text) {
            return Err(t("un hilo privado se abre solo en un canal de texto, sin mensaje").into());
        }
        let needed = if private {
            Permissions::CREATE_PRIVATE_THREADS
        } else {
            Permissions::CREATE_THREADS
        };
        if !parent.permissions.contains(needed) {
            return Err(t("no tenés permiso para abrir hilos acá").into());
        }
        let name = thread_name(&name)?;
        if let Some(message) = message {
            if self.message(channel, message).is_none() {
                return Err(t("el mensaje no existe").into());
            }
            if self.thread_starts.contains_key(&message) {
                return Err(t("ese mensaje ya tiene un hilo").into());
            }
        }
        let id = self.add_thread(&parent, name, private, Vec::new())?;
        if let Some(message) = message {
            self.thread_starts.insert(message, id);
        }
        Ok(self
            .reported(id)
            .into_iter()
            .chain([Event::Created {
                request,
                created: Created::Channel(id),
            }])
            .collect())
    }

    fn create_post(
        &mut self,
        request: crate::RequestId,
        forum: ChannelId,
        title: String,
        tags: Vec<TagId>,
        content: String,
        files: &[crate::PreparedUpload],
    ) -> Outcome {
        let parent = self
            .channel(forum)
            .filter(|c| c.kind == ChannelKind::Forum && c.permissions.contains(Permissions::VIEW))
            .cloned()
            .ok_or(t("el foro no existe"))?;
        if !parent.permissions.contains(Permissions::SEND) {
            return Err(t("no tenés permiso para publicar en este foro").into());
        }
        let title = thread_name(&title)?;
        let tags = post_tags(&parent, tags)?;
        if !parent.permissions.contains(Permissions::MANAGE_THREADS)
            && parent
                .tags
                .iter()
                .any(|known| known.moderated && tags.contains(&known.id))
        {
            return Err(t("esa etiqueta es solo para la moderación").into());
        }
        let content = content.trim().to_owned();
        if (content.is_empty() && files.is_empty()) || content.encode_utf16().count() > 2000 {
            return Err(t("el primer mensaje tiene de 1 a 2000 caracteres").into());
        }
        if files.len() > 10 {
            return Err(t("una publicación lleva hasta 10 archivos").into());
        }
        let id = self.add_thread(&parent, title, false, tags)?;
        let now = self.now;
        let mut first = self
            .post(id, ME, content, None, now)
            .ok_or(t("el foro no existe"))?;
        // Its files all went, as a service reports them before the post.
        let total = files.iter().map(|file| file.metadata().size_bytes).sum();
        let progress = (!files.is_empty()).then_some(Event::PostProgress {
            request,
            sent_bytes: total,
            total_bytes: total,
        });
        if !files.is_empty() {
            first.attachments = self.attachments(files);
            self.store(id, first.clone());
        }
        // Its card shows its first message at once (2026-10-18).
        if let Some(thread) = self.thread_mut(id) {
            thread.starter = Some(PostStarter::of(&first));
        }
        Ok(progress
            .into_iter()
            .chain(self.reported(id))
            .chain([Event::Created {
                request,
                created: Created::Channel(id),
            }])
            .collect())
    }

    /// A page of `scope`'s threads of `parent`, the most recently active
    /// first, after the cursor `before` (`{activity}:{id}`, the last of the
    /// page before): ties break by id, so nothing repeats or is lost. Open
    /// private ones only if the account is in them or moderates.
    fn threads_page(
        &mut self,
        request: crate::RequestId,
        parent: ChannelId,
        scope: ThreadScope,
        before: Option<String>,
    ) -> Outcome {
        let found = self
            .channel(parent)
            .filter(|c| c.permissions.contains(Permissions::VIEW))
            .ok_or(t("el canal no existe"))?;
        if scope == ThreadScope::Private && !found.permissions.contains(Permissions::MANAGE_THREADS)
        {
            return Err(t("solo la moderación ve todos los hilos privados").into());
        }
        let after = match before.as_deref() {
            None => None,
            Some(cursor) => Some(parse_cursor(cursor).ok_or(t("cursor inválido"))?),
        };
        let moderates = found.permissions.contains(Permissions::MANAGE_THREADS);
        let mut threads: Vec<&Channel> = self
            .channels
            .iter()
            .filter(|c| c.is_thread() && c.parent == Some(parent))
            .filter(|c| c.permissions.contains(Permissions::VIEW))
            .filter(|c| {
                let info = c.thread.clone().unwrap_or_default();
                match scope {
                    ThreadScope::Active => {
                        !info.archived && (!info.private || info.joined || moderates)
                    }
                    ThreadScope::Archived => info.archived && !info.private,
                    ThreadScope::Private => info.archived && info.private,
                    ThreadScope::Joined => info.archived && info.private && info.joined,
                }
            })
            .collect();
        threads.sort_by_key(|c| std::cmp::Reverse(order(c)));
        let page: Vec<Channel> = threads
            .into_iter()
            .filter(|c| after.is_none_or(|after| order(c) < after))
            .take(THREAD_PAGE + 1)
            .cloned()
            .collect();
        let more = page.len() > THREAD_PAGE;
        let page = &page[..page.len().min(THREAD_PAGE)];
        let next = more
            .then(|| page.last().map(|c| cursor(order(c))))
            .flatten();
        let ids = page.iter().map(|c| c.id).collect();
        Ok(page
            .iter()
            .cloned()
            .map(Event::ChannelChanged)
            .chain([Event::ThreadsLoaded {
                request,
                parent,
                scope,
                threads: ids,
                next,
            }])
            .collect())
    }

    fn follow(&mut self, id: ChannelId, follow: bool) -> Outcome {
        let found = self.seen_thread(id)?;
        let info = found.thread.clone().unwrap_or_default();
        if info.archived {
            return Err(t("reabrí el hilo antes de seguirlo o dejarlo").into());
        }
        let moderates = found.permissions.contains(Permissions::MANAGE_THREADS);
        if info.joined == follow {
            return Ok(self.reported(id).into_iter().collect());
        }
        let thread = self.thread_mut(id).ok_or(t("el hilo no existe"))?;
        thread.joined = follow;
        // Its silence is the account's membership's: leaving ends it.
        if !follow {
            thread.muted_until = None;
        }
        thread.members = if follow {
            thread.members.saturating_add(1)
        } else {
            thread.members.saturating_sub(1)
        };
        // Leaving a private thread hides it from anyone but moderation.
        if !follow && info.private && !moderates {
            return Ok(vec![Event::ChannelRemoved { channel: id }]);
        }
        Ok(self.reported(id).into_iter().collect())
    }

    /// Silence a thread the account follows until `until`, or, with
    /// `None` (or a time gone), hear it again.
    fn mute_thread(&mut self, id: ChannelId, until: Option<Timestamp>) -> Outcome {
        let found = self.seen_thread(id)?;
        if !found.thread.as_ref().is_some_and(|thread| thread.joined) {
            return Err(t("seguí el hilo para silenciarlo").into());
        }
        let now = self.now;
        let thread = self.thread_mut(id).ok_or(t("el hilo no existe"))?;
        thread.muted_until = until.filter(|until| *until > now);
        Ok(self.reported(id).into_iter().collect())
    }

    fn edit_thread(&mut self, id: ChannelId, edit: ThreadEdit) -> Outcome {
        let found = self.seen_thread(id)?.clone();
        let info = found.thread.clone().unwrap_or_default();
        let moderates = found.permissions.contains(Permissions::MANAGE_THREADS);
        let owns = info.owner == Some(ME);
        if !moderates && !owns {
            return Err(t("solo quien abrió el hilo o la moderación lo cambia").into());
        }
        if edit.locked.is_some() && !moderates {
            return Err(t("solo la moderación bloquea hilos").into());
        }
        if info.locked && !moderates && edit.archived == Some(false) {
            return Err(t("el hilo está bloqueado: solo la moderación lo reabre").into());
        }
        let name = edit.name.as_deref().map(thread_name).transpose()?;
        let tags = match edit.tags {
            Some(tags) => {
                let forum = found
                    .parent
                    .and_then(|parent| self.channel(parent))
                    .filter(|parent| parent.kind == ChannelKind::Forum)
                    .ok_or(t("solo las publicaciones de un foro llevan etiquetas"))?;
                // Moderated tags already on it stay; adding one needs moderation.
                let added: Vec<TagId> = tags
                    .iter()
                    .copied()
                    .filter(|tag| !info.tags.contains(tag))
                    .collect();
                if !moderates
                    && added.iter().any(|tag| {
                        forum
                            .tags
                            .iter()
                            .any(|known| known.id == *tag && known.moderated)
                    })
                {
                    return Err(t("esa etiqueta es solo para la moderación").into());
                }
                Some(post_tags(forum, tags)?)
            }
            None => None,
        };
        let thread = self.thread_mut(id).ok_or(t("el hilo no existe"))?;
        if let Some(archived) = edit.archived {
            thread.archived = archived;
        }
        if let Some(locked) = edit.locked {
            thread.locked = locked;
        }
        if let Some(tags) = tags {
            thread.tags = tags;
        }
        if let Some(name) = name {
            if let Some(channel) = self.channels.iter_mut().find(|c| c.id == id) {
                channel.name = name;
            }
        }
        Ok(self.reported(id).into_iter().collect())
    }

    fn delete_thread(&mut self, id: ChannelId) -> Outcome {
        let found = self.seen_thread(id)?;
        if !found.permissions.contains(Permissions::MANAGE_THREADS) {
            return Err(t("solo la moderación borra hilos").into());
        }
        self.channels.retain(|c| c.id != id);
        self.sims.remove(&id);
        self.thread_starts.retain(|_, thread| *thread != id);
        Ok(vec![Event::ChannelRemoved { channel: id }])
    }
}

/// Where a thread sits among the archived: its last activity, then its id.
fn order(channel: &Channel) -> (Timestamp, ChannelId) {
    let last = channel
        .thread
        .as_ref()
        .map_or(Timestamp::default(), |t| t.last_activity);
    (last, channel.id)
}

fn cursor((last, id): (Timestamp, ChannelId)) -> String {
    format!("{}:{}", last.0, id.0)
}

fn parse_cursor(cursor: &str) -> Option<(Timestamp, ChannelId)> {
    let (last, id) = cursor.split_once(':')?;
    Some((Timestamp(last.parse().ok()?), ChannelId(id.parse().ok()?)))
}

/// A thread's or post's name, trimmed: 1 to 100 characters, one line.
fn thread_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_THREAD_NAME || name.contains('\n') {
        return Err(tr!(
            "el nombre tiene de 1 a {max} caracteres, en una línea",
            max = MAX_THREAD_NAME
        ));
    }
    Ok(name.to_owned())
}

/// `tags` as a post of `forum` may carry them: the forum's own, distinct, at
/// most five, and one at least when the forum asks for it.
fn post_tags(forum: &Channel, tags: Vec<TagId>) -> Result<Vec<TagId>, String> {
    let mut kept = Vec::new();
    for tag in tags {
        if !forum.tags.iter().any(|known| known.id == tag) {
            return Err(t("esa etiqueta no es del foro").into());
        }
        if !kept.contains(&tag) {
            kept.push(tag);
        }
    }
    if kept.len() > MAX_APPLIED_TAGS {
        return Err(tr!(
            "una publicación lleva hasta {max} etiquetas",
            max = MAX_APPLIED_TAGS
        ));
    }
    if forum.forum.require_tag && kept.is_empty() {
        return Err(t("este foro pide al menos una etiqueta").into());
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use crate::mock::{MockConfig, MockProvider, ME};
    use crate::{Command, Created, Event, ThreadEdit, ThreadScope};
    use rune_model::{Channel, ChannelId, ChannelKind, Permissions, Timestamp};

    fn config() -> MockConfig {
        MockConfig {
            history_end: Timestamp(1_790_000_000_000),
            ..MockConfig::default()
        }
    }

    fn named<'a>(channels: &'a [Channel], name: &str) -> &'a Channel {
        channels.iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn the_forum_and_threads_are_the_same_for_the_same_seed_and_inherit_access() {
        let snapshot = MockProvider::new(config()).snapshot();
        let again = MockProvider::new(config()).snapshot();
        let threads: Vec<_> = snapshot.channels.iter().filter(|c| c.is_thread()).collect();
        // A session starts with the active threads only.
        assert_eq!(threads.len(), 5);
        assert!(threads.iter().all(|c| !c.thread.as_ref().unwrap().archived));
        assert_eq!(
            threads,
            again
                .channels
                .iter()
                .filter(|c| c.is_thread())
                .collect::<Vec<_>>()
        );
        let forum = snapshot
            .channels
            .iter()
            .find(|c| c.kind == ChannelKind::Forum)
            .unwrap();
        assert_eq!(forum.tags.len(), 5);
        let posts: Vec<_> = threads
            .iter()
            .filter(|c| c.parent == Some(forum.id))
            .collect();
        assert_eq!(posts.len(), 3);
        for post in &posts {
            let info = post.thread.as_ref().unwrap();
            assert!(info
                .tags
                .iter()
                .all(|tag| forum.tags.iter().any(|t| t.id == *tag)));
            assert!(info.last_activity.0 > 0 && info.messages > 0);
        }
        // Every thread can be read like its parent, and its messages exist.
        for thread in &threads {
            assert!(thread
                .permissions
                .contains(Permissions::VIEW | Permissions::SEND_IN_THREADS));
            assert!(snapshot.last_messages.iter().any(|(c, _)| *c == thread.id));
        }
        // Nothing of an archived thread leaks into the start.
        assert!(snapshot
            .last_messages
            .iter()
            .all(|(c, _)| snapshot.channels.iter().any(|known| known.id == *c)));
    }

    /// Every page of `scope`, as asked one after the other.
    fn pages(provider: &MockProvider, parent: ChannelId, scope: ThreadScope) -> Vec<Vec<u64>> {
        let mut before = None;
        let mut pages = Vec::new();
        for request in 1..10 {
            let events = provider.manage(Command::LoadThreads {
                request,
                parent,
                scope,
                before: before.clone(),
            });
            let Some(Event::ThreadsLoaded {
                threads,
                next,
                request: answered,
                ..
            }) = events.last().cloned()
            else {
                panic!("{events:?}");
            };
            assert_eq!(answered, request);
            // The page's threads arrive before it, each once.
            assert_eq!(events.len(), threads.len() + 1);
            pages.push(threads.iter().map(|c| c.0).collect());
            match next {
                Some(next) => before = Some(next),
                None => return pages,
            }
        }
        panic!("the pages never end");
    }

    #[test]
    fn archived_threads_come_a_page_at_a_time_without_repeats_or_gaps() {
        let provider = MockProvider::new(config());
        let snapshot = provider.snapshot();
        let forum = named(&snapshot.channels, "consultas").id;
        let gallery = named(&snapshot.channels, "galería").id;
        let public = pages(&provider, forum, ThreadScope::Archived);
        assert_eq!(
            public.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![5, 3],
            "eight archived posts, five a page"
        );
        let mut all: Vec<u64> = public.concat();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 8);
        // In the channel: public ones, then the private one the account is in;
        // the other private one never shows, and all of them needs moderation.
        assert_eq!(
            pages(&provider, gallery, ThreadScope::Archived)
                .concat()
                .len(),
            3
        );
        assert_eq!(
            pages(&provider, gallery, ThreadScope::Joined)
                .concat()
                .len(),
            1
        );
        assert!(matches!(
            provider
                .manage(Command::LoadThreads {
                    request: 9,
                    parent: gallery,
                    scope: ThreadScope::Private,
                    before: None,
                })
                .as_slice(),
            [Event::Failed { .. }]
        ));
    }

    #[test]
    fn threads_and_posts_are_made_only_as_permitted_and_reported_before_their_answer() {
        let provider = MockProvider::new(config());
        let snapshot = provider.snapshot();
        let gallery = named(&snapshot.channels, "galería").id;
        let (messages, _) = provider.history(gallery, None, 1);
        let events = provider.manage(Command::CreateThread {
            request: 7,
            channel: gallery,
            message: Some(messages[0].id),
            name: "  Más fotos  ".into(),
            private: false,
        });
        let [Event::ChannelChanged(thread), Event::Created {
            request: 7,
            created: Created::Channel(made),
        }] = events.as_slice()
        else {
            panic!("{events:?}");
        };
        assert_eq!((thread.id, thread.name.as_str()), (*made, "Más fotos"));
        let info = thread.thread.as_ref().unwrap();
        assert!(info.joined && info.owner == Some(ME) && !info.private);
        assert_eq!(thread.parent, Some(gallery));
        // One thread per message.
        assert!(matches!(
            provider
                .manage(Command::CreateThread {
                    request: 8,
                    channel: gallery,
                    message: Some(messages[0].id),
                    name: "Otra".into(),
                    private: false,
                })
                .as_slice(),
            [Event::Failed { .. }]
        ));

        let forum = named(&snapshot.channels, "consultas").clone();
        let tag = forum.tags[0].id;
        let events = provider.manage(Command::CreatePost {
            request: 9,
            forum: forum.id,
            title: "¿Flash o luz continua?".into(),
            tags: vec![tag],
            content: "Para retratos en casa.".into(),
            files: vec![crate::PreparedUpload::new(
                rune_model::PendingAttachment {
                    token: rune_model::UploadToken(1),
                    filename: "luz.png".into(),
                    size_bytes: 3,
                },
                bytes::Bytes::from_static(b"png"),
            )
            .unwrap()],
        });
        let [Event::PostProgress {
            request: 9,
            sent_bytes: 3,
            total_bytes: 3,
        }, Event::ChannelChanged(post), Event::Created { .. }] = events.as_slice()
        else {
            panic!("{events:?}");
        };
        assert_eq!(post.thread.as_ref().unwrap().tags, vec![tag]);
        let (first, _) = provider.history(post.id, None, 10);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].attachments[0].filename, "luz.png");
        // A moderated tag is not the account's to put in this club.
        let moderated = forum.tags.iter().find(|t| t.moderated).unwrap().id;
        assert!(matches!(
            provider
                .manage(Command::CreatePost {
                    request: 10,
                    forum: forum.id,
                    title: "x".into(),
                    tags: vec![tag, tag, tag, tag, tag, moderated],
                    content: "y".into(),
                    files: Vec::new(),
                })
                .as_slice(),
            [Event::Failed { .. }]
        ));
    }

    #[test]
    fn following_closing_and_a_reply_that_reopens_go_as_a_service_would() {
        let provider = MockProvider::new(config());
        let snapshot = provider.snapshot();
        let saturday = named(&snapshot.channels, "Salida fotográfica del sábado").id;
        let changed = |events: Vec<Event>| match events.as_slice() {
            [Event::ChannelChanged(channel)] => channel.clone(),
            other => panic!("{other:?}"),
        };
        let thread = changed(provider.manage(Command::FollowThread {
            thread: saturday,
            follow: false,
        }));
        assert!(!thread.thread.unwrap().joined);
        // The account neither started it nor moderates the club.
        assert!(matches!(
            provider
                .manage(Command::EditThread {
                    thread: saturday,
                    edit: ThreadEdit {
                        archived: Some(true),
                        ..Default::default()
                    },
                })
                .as_slice(),
            [Event::Failed { .. }]
        ));
        assert!(matches!(
            provider
                .manage(Command::DeleteThread { thread: saturday })
                .as_slice(),
            [Event::Failed { .. }]
        ));

        // Its own thread it closes, and a reply opens it again.
        let gallery = named(&snapshot.channels, "galería").id;
        let events = provider.manage(Command::CreateThread {
            request: 1,
            channel: gallery,
            message: None,
            name: "Propio".into(),
            private: false,
        });
        let Some(Event::ChannelChanged(own)) = events.first() else {
            panic!("{events:?}");
        };
        let closed = changed(provider.manage(Command::EditThread {
            thread: own.id,
            edit: ThreadEdit {
                archived: Some(true),
                name: Some("Propio, cerrado".into()),
                ..Default::default()
            },
        }));
        assert!(closed.thread.as_ref().unwrap().archived);
        assert_eq!(closed.name, "Propio, cerrado");
        let mut world = provider.world.lock().unwrap();
        let reopened = world.thread_reply(own.id).unwrap();
        let [Event::ChannelChanged(open)] = reopened.as_slice() else {
            panic!("{reopened:?}");
        };
        assert!(!open.thread.as_ref().unwrap().archived);
        let later = Timestamp(open.thread.as_ref().unwrap().last_activity.0 + 1_000);
        world.count_reply(own.id, later);
        let counted = world.channel(own.id).unwrap().thread.clone().unwrap();
        assert_eq!((counted.messages, counted.last_activity), (1, later));
        // A locked post takes no reply from someone who does not moderate.
        let locked = named(&snapshot.channels, "Fotos movidas con poca luz").id;
        assert!(world.thread_reply(locked).is_err());
    }

    #[test]
    fn a_later_change_to_the_parent_reaches_its_threads() {
        let provider = MockProvider::new(config());
        let snapshot = provider.snapshot();
        let gallery = named(&snapshot.channels, "galería").clone();
        let saturday = named(&snapshot.channels, "Salida fotográfica del sábado").id;
        assert!(snapshot
            .channels
            .iter()
            .all(|c| !c.is_thread() || c.overwrites.is_empty()));
        let mut world = provider.world.lock().unwrap();
        // The account writes in none of its threads any more.
        let found = world
            .channels
            .iter_mut()
            .find(|c| c.id == gallery.id)
            .unwrap();
        found.overwrites.push(rune_model::Overwrite {
            target: rune_model::OverwriteTarget::Member(ME),
            allow: Permissions::NONE,
            deny: Permissions::SEND_IN_THREADS,
        });
        let changed = world.refresh_permissions(gallery.server.unwrap());
        let thread = changed
            .iter()
            .find_map(|event| match event {
                Event::ChannelChanged(c) if c.id == saturday => Some(c),
                _ => None,
            })
            .expect("the thread is reported again");
        assert!(!thread.permissions.contains(Permissions::SEND_IN_THREADS));
        assert_eq!(
            thread.permissions,
            world.channel(gallery.id).unwrap().permissions
        );
        assert!(world.thread_reply(saturday).is_err());
    }
}
