//! The rest of the simulation's administration (P7): the owned server's
//! profile traits and picture, its safety settings and paused invitations,
//! its channels reordered at once, forum settings, role pictures, emojis
//! and gradients, the count of members of each role, and member pages in
//! the order asked for, with how each one joined. Everything is checked as
//! a service would and is deterministic: when someone joined, and how, is
//! worked out from who they are. Since 2026-10-19 also its banner and a
//! moving icon (its boosts unlock both, not a moving banner), traits and a
//! forum's reaction with its own emojis, becoming a community or not, and a
//! forum's archive time and tag filter.

use rune_i18n::t;
use rune_model::{
    ChannelId, ChannelKind, ChannelMove, ForumTag, JoinMethod, JoinedBy, Member, MemberOrder,
    MemberQuery, Permissions, RoleId, RoleStyle, ServerId, ServerTrait, TagId, Timestamp, UserId,
    AFK_TIMEOUTS, ARCHIVE_AFTER, MAX_TAGS, MAX_TAG_NAME, MAX_TRAITS,
};

use super::{World, ME};
use crate::{ChannelEdit, Command, Event, RoleEdit, RoleIcon, ServerEdit};

type Outcome = Result<Vec<Event>, String>;

/// The server the account owns.
const OWNED: ServerId = ServerId(2);
/// The owned server's forum and its category, apart from every other
/// channel's id.
const FORUM: ChannelId = ChannelId(9_700);
const COMMUNITY: ChannelId = ChannelId(9_699);
/// How far back the simulated members joined, at most.
const JOINED_SPAN_DAYS: u64 = 60;
/// What a recent join is.
const RECENT_DAYS: i64 = 7;

impl World {
    /// The owned server's options: role pictures and gradients unlocked,
    /// a system channel, an idle channel and a few traits. The rest keep
    /// a service's defaults.
    pub(super) fn add_server_options(&mut self) {
        let general = self.channel_named(OWNED, "limn-core", ChannelKind::Text);
        let news = self.channel_named(OWNED, "componentes", ChannelKind::Text);
        let idle = self
            .channels
            .iter()
            .find(|channel| channel.server == Some(OWNED) && channel.kind == ChannelKind::Voice)
            .map(|channel| channel.id);
        let Some(server) = self.servers.iter_mut().find(|server| server.id == OWNED) else {
            return;
        };
        let options = &mut server.options;
        options.system_channel = general;
        options.afk_channel = idle;
        options.verification = rune_model::Verification::Low;
        options.media_scan = rune_model::MediaScan::WithoutRoles;
        options.role_icons = true;
        options.role_gradients = true;
        // Icon, banner and community (2026-10-19): boosts enough for a
        // moving icon and a still banner; its rules and news channels.
        options.animated_icon = true;
        options.banner_allowed = true;
        options.rules_channel = general;
        options.updates_channel = news;
        options.traits = Some(
            [("🦀", "Rust"), ("🎨", "Diseño"), ("🖥️", "Escritorio")]
                .into_iter()
                .map(|(emoji, label)| ServerTrait {
                    label: label.into(),
                    emoji: Some(emoji.into()),
                    emoji_id: None,
                    emoji_animated: false,
                })
                .collect(),
        );
        for server in self.servers.iter_mut().filter(|server| server.id != OWNED) {
            server.options.traits = Some(Vec::new());
        }
        self.add_owned_forum();
    }

    /// A forum of the owned server, in a category of its own after the
    /// others, so its settings can be changed here.
    fn add_owned_forum(&mut self) {
        let categories = self
            .channels
            .iter()
            .filter(|channel| {
                channel.server == Some(OWNED) && channel.kind == ChannelKind::Category
            })
            .count() as i32;
        let blank = |id: ChannelId, kind: ChannelKind, name: &str| rune_model::Channel {
            id,
            server: Some(OWNED),
            kind,
            name: name.into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        };
        self.channels.push(rune_model::Channel {
            position: categories,
            ..blank(COMMUNITY, ChannelKind::Category, "Comunidad")
        });
        let tags = [
            ("Idea", "💡", false),
            ("Aceptada", "✅", true),
            ("Diseño", "🎨", false),
        ];
        self.channels.push(rune_model::Channel {
            topic: Some("Una publicación por propuesta; la etiqueta dice en qué está.".into()),
            parent: Some(COMMUNITY),
            tags: tags
                .iter()
                .enumerate()
                .map(|(index, (name, emoji, moderated))| ForumTag {
                    id: TagId(FORUM.0 * 100 + index as u64),
                    name: (*name).into(),
                    emoji: Some((*emoji).into()),
                    moderated: *moderated,
                })
                .collect(),
            forum: rune_model::ForumDefaults {
                reaction: Some("👍".into()),
                ..Default::default()
            },
            ..blank(FORUM, ChannelKind::Forum, "propuestas")
        });
        self.refresh_permissions(OWNED);
    }

    fn channel_named(&self, server: ServerId, name: &str, kind: ChannelKind) -> Option<ChannelId> {
        self.channels
            .iter()
            .find(|channel| {
                channel.server == Some(server) && channel.kind == kind && channel.name == name
            })
            .map(|channel| channel.id)
    }

    /// The commands of this part; `None` for any other.
    pub(super) fn rest_admin(&mut self, command: &Command) -> Option<Outcome> {
        Some(match command {
            Command::LoadServerProfile { server, .. } => self
                .server(*server)
                .map(|found| vec![Event::ServerChanged(found.clone())]),
            Command::ReorderChannels { server, moves } => self.reorder_channels(*server, moves),
            Command::LoadRoleCounts { server, .. } => self.role_counts(*server),
            _ => return None,
        })
    }

    /// Check what `edit` changes beyond the name and look of `server`, and
    /// change it. Call once the account may manage the server.
    pub(super) fn edit_options(
        &mut self,
        server: ServerId,
        edit: &ServerEdit,
    ) -> Result<(), String> {
        let channel_of = |channel: Option<ChannelId>, kind: ChannelKind| match channel {
            None => Ok(None),
            Some(id) => self
                .channel(id)
                .filter(|found| found.server == Some(server) && found.kind == kind)
                .map(|found| Some(found.id))
                .ok_or_else(|| t("ese canal no es de este servidor").to_owned()),
        };
        let system = edit
            .system_channel
            .map(|channel| channel_of(channel, ChannelKind::Text))
            .transpose()?;
        let afk = edit
            .afk_channel
            .map(|channel| channel_of(channel, ChannelKind::Voice))
            .transpose()?;
        if let Some(traits) = &edit.traits {
            if traits.len() > MAX_TRAITS || !traits.iter().all(ServerTrait::valid) {
                return Err(t("hasta 5 rasgos, cada uno con un nombre corto").into());
            }
        }
        // Icon, banner and community (2026-10-19).
        self.check_looks(server, edit)?;
        let rules = edit
            .rules_channel
            .map(|channel| channel_of(channel, ChannelKind::Text))
            .transpose()?;
        let updates = edit
            .updates_channel
            .map(|channel| channel_of(channel, ChannelKind::Text))
            .transpose()?;
        if edit
            .afk_timeout
            .is_some_and(|secs| !AFK_TIMEOUTS.contains(&secs))
        {
            return Err(t("ese tiempo de inactividad no está disponible").into());
        }
        let version = self.fresh_id();
        let found = self.server_mut(server)?;
        let community = edit.community.unwrap_or(found.options.community);
        let verification = edit.verification.unwrap_or(found.options.verification);
        let scan = edit.media_scan.unwrap_or(found.options.media_scan);
        if community
            && (verification == rune_model::Verification::None
                || scan != rune_model::MediaScan::Everyone)
        {
            return Err(
                t("una comunidad necesita verificación y revisar el contenido de todos").into(),
            );
        }
        if community
            && (rules.unwrap_or(found.options.rules_channel).is_none()
                || updates.unwrap_or(found.options.updates_channel).is_none())
        {
            return Err(t("una comunidad necesita un canal de reglas y uno de novedades").into());
        }
        if let Some(icon) = &edit.icon {
            found.icon = icon.as_ref().map(|image| rune_model::RemoteImage {
                url: format!(
                    "https://simulacion.invalid/servidores/{server}/icono-{version}-{}.png",
                    image.bytes().len()
                ),
                kind: rune_model::ImageKind::Artwork,
            });
        }
        if let Some(banner) = &edit.banner {
            found.options.banner = banner.as_ref().map(|image| rune_model::RemoteImage {
                url: format!(
                    "https://simulacion.invalid/servidores/{server}/banner-{version}-{}.png",
                    image.bytes().len()
                ),
                kind: rune_model::ImageKind::Banner,
            });
        }
        let options = &mut found.options;
        if let Some(traits) = &edit.traits {
            options.traits = Some(
                traits
                    .iter()
                    .map(|found| ServerTrait {
                        label: found.label.trim().to_owned(),
                        ..found.clone()
                    })
                    .collect(),
            );
        }
        if let Some(rules) = rules {
            options.rules_channel = rules;
        }
        if let Some(updates) = updates {
            options.updates_channel = updates;
        }
        if let Some(community) = edit.community {
            options.community = community;
            // Its rules and onboarding go with it, as at a service.
            if !community {
                options.rules_screening = false;
                options.onboarding = false;
            }
        }
        if let Some(system) = system {
            options.system_channel = system;
        }
        if let Some(messages) = edit.system_messages {
            options.system_messages = messages;
        }
        if let Some(mentions) = edit.mentions_only {
            options.mentions_only = mentions;
        }
        if let Some(afk) = afk {
            options.afk_channel = afk;
        }
        if let Some(secs) = edit.afk_timeout {
            options.afk_timeout = secs;
        }
        options.verification = verification;
        options.media_scan = scan;
        if let Some(paused) = edit.invites_paused {
            options.invites_paused = paused;
        }
        Ok(())
    }

    /// What `edit` asks of `server`'s boosts and emojis (2026-10-19): a
    /// banner where it is unlocked, a GIF where moving is, and emojis of
    /// its own in its traits.
    fn check_looks(&self, server: ServerId, edit: &ServerEdit) -> Result<(), String> {
        let found = self.server(server)?;
        let options = &found.options;
        let moving = |image: &Option<Option<crate::ProfileImage>>| matches!(image, Some(Some(image)) if image.animated());
        if moving(&edit.icon) && !options.animated_icon {
            return Err(t("este servidor no tiene el ícono animado").into());
        }
        if edit.banner.as_ref().is_some_and(Option::is_some) && !options.banner_allowed {
            return Err(t("este servidor no tiene banner").into());
        }
        if moving(&edit.banner) && !options.animated_banner {
            return Err(t("este servidor no tiene el banner animado").into());
        }
        let own = |id: rune_model::EmojiId| found.emojis.iter().any(|emoji| emoji.id == id);
        if edit
            .traits
            .iter()
            .flatten()
            .filter_map(|found| found.emoji_id)
            .any(|id| !own(id))
        {
            return Err(t("ese emoji no es de este servidor").into());
        }
        Ok(())
    }

    /// The style `edit` leaves `role` of `server` with, checked against
    /// what the server allows; `None` when it does not change it.
    pub(super) fn role_style(
        &self,
        server: ServerId,
        role: RoleId,
        edit: &RoleEdit,
    ) -> Result<Option<RoleStyle>, String> {
        if edit.icon.is_none() && edit.gradient.is_none() {
            return Ok(None);
        }
        let found = self.server(server)?;
        if role == RoleId(server.0) {
            return Err(t("@everyone solo cambia sus permisos").into());
        }
        let mut style = found
            .roles
            .iter()
            .find(|candidate| candidate.id == role)
            .ok_or(t("el rol no existe"))?
            .style
            .clone();
        if let Some(icon) = &edit.icon {
            if !found.options.role_icons {
                return Err(t("este servidor no tiene íconos de rol").into());
            }
            if let Some(problem) = icon.as_ref().and_then(RoleIcon::problem) {
                return Err(problem.to_lowercase());
            }
            (style.icon, style.emoji) = match icon {
                None => (None, None),
                Some(RoleIcon::Emoji(emoji)) => (None, Some(emoji.clone())),
                Some(RoleIcon::Image(image)) => (
                    Some(rune_model::RemoteImage {
                        url: format!(
                            "https://simulacion.invalid/roles/{role}/icono-{}.png",
                            image.bytes().len()
                        ),
                        kind: rune_model::ImageKind::Artwork,
                    }),
                    None,
                ),
            };
        }
        if let Some(gradient) = &edit.gradient {
            if !found.options.role_gradients {
                return Err(t("este servidor no tiene degradés de rol").into());
            }
            if gradient.len() > 2 || gradient.iter().any(|color| color.0 > 0xFF_FFFF) {
                return Err(t("un degradé lleva uno o dos colores más").into());
            }
            style.gradient = gradient.clone();
        }
        Ok(Some(style))
    }

    /// Check a forum's tags and defaults in `edit` and change them in
    /// `channel`; new tags get their ids here.
    pub(super) fn edit_forum(
        &mut self,
        channel: ChannelId,
        edit: &ChannelEdit,
    ) -> Result<(), String> {
        if edit.tags.is_none() && edit.forum.is_none() {
            return Ok(());
        }
        if self.channel(channel).map(|found| found.kind) != Some(ChannelKind::Forum) {
            return Err(t("solo un foro tiene etiquetas").into());
        }
        let mut tags = Vec::new();
        if let Some(wanted) = &edit.tags {
            if wanted.len() > MAX_TAGS {
                return Err(t("un foro tiene hasta 20 etiquetas").into());
            }
            let known: Vec<TagId> = self
                .channel(channel)
                .map(|found| found.tags.iter().map(|tag| tag.id).collect())
                .unwrap_or_default();
            for tag in wanted {
                let name = tag.name.trim();
                if !(1..=MAX_TAG_NAME).contains(&name.chars().count())
                    || (tag.id.0 != 0 && !known.contains(&tag.id))
                {
                    return Err(t("cada etiqueta necesita un nombre corto").into());
                }
                let id = if tag.id.0 == 0 {
                    TagId(self.fresh_id())
                } else {
                    tag.id
                };
                tags.push(ForumTag {
                    id,
                    name: name.to_owned(),
                    ..tag.clone()
                });
            }
        }
        let found = self
            .channels
            .iter_mut()
            .find(|candidate| candidate.id == channel)
            .ok_or(t("el canal no existe"))?;
        if edit.tags.is_some() {
            // A post keeps only the tags that still exist.
            let kept: Vec<TagId> = tags.iter().map(|tag| tag.id).collect();
            found.tags = tags;
            for post in self
                .channels
                .iter_mut()
                .filter(|post| post.parent == Some(channel))
            {
                if let Some(thread) = post.thread.as_mut() {
                    thread.tags.retain(|tag| kept.contains(tag));
                }
            }
        }
        if let Some(forum) = &edit.forum {
            // Server emoji, archive and tag filter (2026-10-19).
            let server = self.channel(channel).and_then(|found| found.server);
            let own = forum.reaction_emoji.is_none_or(|id| {
                self.servers
                    .iter()
                    .filter(|found| Some(found.id) == server)
                    .any(|found| found.emojis.iter().any(|emoji| emoji.id == id))
            });
            if !own
                || forum
                    .archive_after
                    .is_some_and(|after| !ARCHIVE_AFTER.contains(&after))
            {
                return Err(t("esa reacción o ese archivado no están disponibles").into());
            }
            let found = self
                .channels
                .iter_mut()
                .find(|candidate| candidate.id == channel)
                .ok_or(t("el canal no existe"))?;
            found.forum = forum.clone();
        }
        Ok(())
    }

    /// Put the channels where `moves` say, all or none.
    fn reorder_channels(&mut self, server: ServerId, moves: &[ChannelMove]) -> Outcome {
        self.require(server, Permissions::MANAGE_CHANNELS)?;
        if moves.is_empty() {
            return Err(t("no hay nada que mover").into());
        }
        let mut seen = std::collections::HashSet::new();
        for step in moves {
            let found = self
                .channel(step.channel)
                .filter(|found| found.server == Some(server) && !found.is_thread())
                .ok_or(t("ese canal no es de este servidor"))?;
            if !seen.insert(step.channel) || step.position < 0 {
                return Err(t("cada canal va una vez, en un lugar válido").into());
            }
            match step.parent {
                Some(_) if found.kind == ChannelKind::Category => {
                    return Err(t("una categoría no va dentro de otra").into());
                }
                Some(parent)
                    if !self.channel(parent).is_some_and(|category| {
                        category.kind == ChannelKind::Category && category.server == Some(server)
                    }) =>
                {
                    return Err(t("un canal solo va dentro de una categoría").into());
                }
                _ => {}
            }
            // Taking the category's permissions also changes who may do
            // what, there and in the category, as each one's own allow.
            if step.lock_permissions {
                let Some(parent) = step.parent else {
                    return Err(t("solo se sincroniza con una categoría").into());
                };
                self.managed_channel(step.channel, Permissions::MANAGE_ROLES)?;
                self.managed_channel(parent, Permissions::MANAGE_ROLES)?;
            }
        }
        let mut events = Vec::new();
        for step in moves {
            let synced = step
                .parent
                .filter(|_| step.lock_permissions)
                .and_then(|parent| self.channel(parent))
                .map(|category| category.overwrites.clone());
            let Some(found) = self
                .channels
                .iter_mut()
                .find(|candidate| candidate.id == step.channel)
            else {
                continue;
            };
            let overwrites = synced.filter(|synced| *synced != found.overwrites);
            if found.parent != step.parent
                || found.position != step.position
                || overwrites.is_some()
            {
                found.parent = step.parent;
                found.position = step.position;
                if let Some(overwrites) = overwrites {
                    found.overwrites = overwrites;
                }
                events.push(Event::ChannelChanged(found.clone()));
            }
        }
        // What the account may do there follows the overwrites it took.
        if moves.iter().any(|step| step.lock_permissions) {
            let refreshed = self.refresh_permissions(server);
            events.retain(|event| {
                !refreshed.iter().any(|newer| {
                    matches!((event, newer), (Event::ChannelChanged(a), Event::ChannelChanged(b)) if a.id == b.id)
                })
            });
            events.extend(refreshed);
        }
        Ok(events)
    }

    fn role_counts(&self, server: ServerId) -> Outcome {
        self.require(server, Permissions::MANAGE_ROLES)?;
        let counts = self
            .server(server)?
            .roles
            .iter()
            .filter(|role| role.id != RoleId(server.0))
            .map(|role| {
                let count = self
                    .members
                    .iter()
                    .filter(|member| member.server == server && member.roles.contains(&role.id))
                    .count();
                (role.id, count as u32)
            })
            .collect();
        Ok(vec![Event::RoleCounts { server, counts }])
    }

    /// When `user` joined `server`, as the simulation says: within the last
    /// [`JOINED_SPAN_DAYS`], the account and who owns it the first.
    fn joined(&self, server: ServerId, user: UserId) -> Timestamp {
        let owner = self.server(server).ok().and_then(|found| found.owner);
        let days = if user == ME || Some(user) == owner {
            JOINED_SPAN_DAYS
        } else {
            (user.0.wrapping_mul(0x9E37_79B9) ^ server.0) % JOINED_SPAN_DAYS
        };
        let minutes = (user.0 % 1440) as i64;
        self.history_end
            .plus_minutes(-(days as i64) * 24 * 60 - minutes)
    }

    /// How `user` joined `server`, as the simulation says.
    fn join_method(&self, server: ServerId, user: UserId) -> JoinMethod {
        if self.is_bot(user) {
            return JoinMethod::Bot;
        }
        // Who made it never joined: it was theirs.
        if self.server(server).ok().and_then(|found| found.owner) == Some(user) {
            return JoinMethod::Other;
        }
        let codes: Vec<&str> = self
            .invites
            .get(&server)
            .map(|invites| invites.iter().map(|invite| invite.code.as_str()).collect())
            .unwrap_or_default();
        match user.0 % 5 {
            0 | 1 if !codes.is_empty() => {
                JoinMethod::Invite(Some(codes[user.0 as usize % codes.len()].to_owned()))
            }
            0 | 1 => JoinMethod::Invite(None),
            2 => JoinMethod::VanityLink,
            3 => JoinMethod::Discovery,
            _ => JoinMethod::App,
        }
    }

    /// `server`'s members in `query`'s order and filter, with when and how
    /// each one joined.
    pub(super) fn queried_members(
        &self,
        server: ServerId,
        query: MemberQuery,
    ) -> Vec<(Member, JoinedBy)> {
        let recent = self.history_end.plus_minutes(-RECENT_DAYS * 24 * 60);
        let mut members: Vec<(Member, JoinedBy)> = self
            .members
            .iter()
            .filter(|member| member.server == server)
            .map(|member| {
                let joined = self.joined(server, member.user);
                let member = Member {
                    joined: Some(joined),
                    ..member.clone()
                };
                let method = (member.user, self.join_method(server, member.user));
                (member, method)
            })
            .filter(|(member, _)| !query.recent || member.joined >= Some(recent))
            .collect();
        // Ids grow with time here as at a service: a lower id, an older account.
        match query.order {
            MemberOrder::NewestMembers => {
                members.sort_by_key(|(member, _)| std::cmp::Reverse((member.joined, member.user)))
            }
            MemberOrder::OldestMembers => {
                members.sort_by_key(|(member, _)| (member.joined, member.user))
            }
            MemberOrder::NewestAccounts => {
                members.sort_by_key(|(member, _)| std::cmp::Reverse(member.user))
            }
            MemberOrder::OldestAccounts => members.sort_by_key(|(member, _)| member.user),
        }
        members
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConfig;
    use crate::Failure;

    fn world() -> World {
        World::build(&MockConfig::default())
    }

    fn run(world: &mut World, command: Command) -> Vec<Event> {
        world.manage(command, Timestamp(1_760_000_000_000))
    }

    fn refused(events: &[Event]) -> bool {
        matches!(events, [Event::Failed { .. }])
    }

    #[test]
    fn the_owned_server_saves_its_traits_safety_and_paused_invites() {
        let mut world = world();
        let general = world.channel_named(OWNED, "limn-core", ChannelKind::Text);
        assert!(world.server(OWNED).unwrap().options.traits.is_some());
        let edit = ServerEdit {
            traits: Some(vec![ServerTrait {
                label: "Gráficos".into(),
                emoji: Some("🖼️".into()),
                emoji_id: None,
                emoji_animated: false,
            }]),
            system_channel: Some(None),
            afk_timeout: Some(900),
            verification: Some(rune_model::Verification::High),
            invites_paused: Some(true),
            ..ServerEdit::default()
        };
        let events = run(
            &mut world,
            Command::EditServer {
                server: OWNED,
                edit,
            },
        );
        let [Event::ServerChanged(saved)] = events.as_slice() else {
            panic!("{events:?}")
        };
        let options = &saved.options;
        assert_eq!(options.traits.as_ref().unwrap()[0].label, "Gráficos");
        assert!(options.system_channel.is_none() && general.is_some());
        assert_eq!(options.afk_timeout, 900);
        assert!(options.invites_paused);
        // An idle time the service does not offer, or too many traits, is refused.
        for edit in [
            ServerEdit {
                afk_timeout: Some(42),
                ..ServerEdit::default()
            },
            ServerEdit {
                traits: Some(vec![
                    ServerTrait {
                        label: "x".into(),
                        emoji: None,
                        emoji_id: None,
                        emoji_animated: false,
                    };
                    MAX_TRAITS + 1
                ]),
                ..ServerEdit::default()
            },
        ] {
            assert!(refused(&run(
                &mut world,
                Command::EditServer {
                    server: OWNED,
                    edit
                }
            )));
        }
        // Where the account does not manage the server, nothing changes.
        let other = world
            .servers
            .iter()
            .find(|server| server.id != OWNED)
            .unwrap()
            .id;
        let events = run(
            &mut world,
            Command::EditServer {
                server: other,
                edit: ServerEdit {
                    invites_paused: Some(true),
                    ..ServerEdit::default()
                },
            },
        );
        assert!(matches!(events.as_slice(),
            [Event::Failed { failure: Failure::Server { server: Some(of) }, .. }] if *of == other));
    }

    #[test]
    fn channels_move_at_once_and_a_wrong_move_moves_nothing() {
        let mut world = world();
        let general = world
            .channel_named(OWNED, "limn-core", ChannelKind::Text)
            .unwrap();
        let category = world
            .channels
            .iter()
            .find(|channel| channel.server == Some(OWNED) && channel.kind == ChannelKind::Category)
            .unwrap()
            .id;
        let bad = run(
            &mut world,
            Command::ReorderChannels {
                server: OWNED,
                moves: vec![
                    ChannelMove {
                        channel: general,
                        parent: None,
                        position: 9,
                        lock_permissions: false,
                    },
                    ChannelMove {
                        channel: category,
                        parent: Some(category),
                        position: 0,
                        lock_permissions: false,
                    },
                ],
            },
        );
        assert!(matches!(bad.as_slice(),
            [Event::Failed { failure: Failure::ChannelOrder { server }, .. }] if *server == OWNED));
        assert_ne!(world.channel(general).unwrap().position, 9);
        let moved = run(
            &mut world,
            Command::ReorderChannels {
                server: OWNED,
                moves: vec![ChannelMove {
                    channel: general,
                    parent: None,
                    position: 9,
                    lock_permissions: false,
                }],
            },
        );
        assert!(matches!(moved.as_slice(),
            [Event::ChannelChanged(channel)] if channel.id == general && channel.parent.is_none() && channel.position == 9));
        // Into the category, taking its permissions in place of its own.
        let hidden = rune_model::Overwrite {
            target: rune_model::OverwriteTarget::Role(rune_model::RoleId(OWNED.0)),
            allow: Permissions::NONE,
            deny: Permissions::VIEW,
        };
        world
            .channels
            .iter_mut()
            .find(|channel| channel.id == category)
            .unwrap()
            .overwrites = vec![hidden];
        let lone = run(
            &mut world,
            Command::ReorderChannels {
                server: OWNED,
                moves: vec![ChannelMove {
                    channel: general,
                    parent: None,
                    position: 1,
                    lock_permissions: true,
                }],
            },
        );
        assert!(matches!(lone.as_slice(), [Event::Failed { .. }]));
        // Changing permissions is allowed or not in the category itself,
        // whatever the server allows.
        let lock = Command::ReorderChannels {
            server: OWNED,
            moves: vec![ChannelMove {
                channel: general,
                parent: Some(category),
                position: 0,
                lock_permissions: true,
            }],
        };
        let mine = world.channel(category).unwrap().permissions;
        let set = |world: &mut World, permissions| {
            world
                .channels
                .iter_mut()
                .find(|channel| channel.id == category)
                .unwrap()
                .permissions = permissions;
        };
        set(&mut world, mine.without(Permissions::MANAGE_ROLES));
        let refused = run(&mut world, lock.clone());
        assert!(matches!(refused.as_slice(), [Event::Failed { .. }]));
        assert_ne!(world.channel(general).unwrap().parent, Some(category));
        set(&mut world, mine);
        let synced = run(&mut world, lock);
        assert!(matches!(synced.as_slice(),
            [Event::ChannelChanged(channel)] if channel.id == general && channel.overwrites == [hidden]));
    }

    #[test]
    fn roles_take_icons_and_gradients_where_the_server_allows_and_are_counted() {
        let mut world = world();
        let role = world.server(OWNED).unwrap().assignable_roles()[0].id;
        let events = run(
            &mut world,
            Command::EditRole {
                server: OWNED,
                role,
                edit: RoleEdit {
                    icon: Some(Some(RoleIcon::Emoji("🛡️".into()))),
                    gradient: Some(vec![rune_model::Rgb(0x00FF00)]),
                    ..RoleEdit::default()
                },
            },
        );
        let Some(Event::ServerChanged(server)) = events.first() else {
            panic!("{events:?}")
        };
        let style = &server
            .roles
            .iter()
            .find(|found| found.id == role)
            .unwrap()
            .style;
        assert_eq!(style.emoji.as_deref(), Some("🛡️"));
        assert_eq!(style.gradient, [rune_model::Rgb(0x00FF00)]);
        let counts = run(
            &mut world,
            Command::LoadRoleCounts {
                request: 1,
                server: OWNED,
            },
        );
        let [Event::RoleCounts { counts, .. }] = counts.as_slice() else {
            panic!("{counts:?}")
        };
        let known = world
            .members
            .iter()
            .filter(|member| member.server == OWNED && member.roles.contains(&role))
            .count() as u32;
        assert!(counts.contains(&(role, known)));
        // A server without them refuses both.
        world.server_mut(OWNED).unwrap().options.role_gradients = false;
        assert!(refused(&run(
            &mut world,
            Command::EditRole {
                server: OWNED,
                role,
                edit: RoleEdit {
                    gradient: Some(Vec::new()),
                    ..RoleEdit::default()
                },
            },
        )));
    }

    #[test]
    fn member_pages_follow_their_order_and_say_how_each_one_joined() {
        let world = world();
        let newest = world.queried_members(OWNED, MemberQuery::default());
        let joined: Vec<_> = newest.iter().map(|(member, _)| member.joined).collect();
        assert!(joined.windows(2).all(|pair| pair[0] >= pair[1]));
        let recent = world.queried_members(
            OWNED,
            MemberQuery {
                recent: true,
                ..MemberQuery::default()
            },
        );
        assert!(!recent.is_empty() && recent.len() < newest.len());
        let oldest = world.queried_members(
            OWNED,
            MemberQuery {
                order: MemberOrder::OldestAccounts,
                recent: false,
            },
        );
        assert!(oldest
            .windows(2)
            .all(|pair| pair[0].0.user < pair[1].0.user));
        assert!(newest
            .iter()
            .any(|(_, (_, method))| matches!(method, JoinMethod::Invite(Some(_)))));
    }

    #[test]
    fn a_forum_keeps_its_tags_reaction_and_defaults() {
        let mut world = world();
        let forum = FORUM;
        assert_eq!(world.channel(forum).unwrap().tags.len(), 3);
        let events = run(
            &mut world,
            Command::EditChannel {
                channel: forum,
                edit: ChannelEdit {
                    tags: Some(vec![ForumTag {
                        id: TagId(0),
                        name: "Nueva".into(),
                        emoji: Some("✨".into()),
                        moderated: true,
                    }]),
                    forum: Some(rune_model::ForumDefaults {
                        gallery: true,
                        reaction: Some("👍".into()),
                        ..Default::default()
                    }),
                    ..ChannelEdit::default()
                },
            },
        );
        let Some(Event::ChannelChanged(saved)) = events.first() else {
            panic!("{events:?}")
        };
        assert_eq!(saved.tags.len(), 1);
        assert_ne!(saved.tags[0].id, TagId(0));
        assert!(saved.tags[0].moderated && saved.forum.gallery);
        assert_eq!(saved.forum.reaction.as_deref(), Some("👍"));
    }

    #[test]
    fn boosts_unlock_the_banner_and_a_moving_icon_and_a_community_needs_its_channels() {
        let mut world = world();
        let gif = |width: u16| {
            let mut bytes = b"GIF89a".to_vec();
            bytes.extend(width.to_le_bytes());
            bytes.extend(width.to_le_bytes());
            crate::ProfileImage::new(bytes::Bytes::from(bytes)).unwrap()
        };
        let edit = |world: &mut World, edit: ServerEdit| {
            run(
                world,
                Command::EditServer {
                    server: OWNED,
                    edit,
                },
            )
        };
        // A moving icon and a banner are unlocked; a moving banner is not.
        let saved = edit(
            &mut world,
            ServerEdit {
                icon: Some(Some(gif(128))),
                banner: Some(Some(gif(960))),
                ..ServerEdit::default()
            },
        );
        assert!(refused(&saved), "{saved:?}");
        world.server_mut(OWNED).unwrap().options.animated_banner = true;
        let saved = edit(
            &mut world,
            ServerEdit {
                icon: Some(Some(gif(128))),
                banner: Some(Some(gif(960))),
                ..ServerEdit::default()
            },
        );
        let [Event::ServerChanged(server)] = saved.as_slice() else {
            panic!("{saved:?}")
        };
        assert!(server.options.banner.is_some() && server.icon.is_some());
        world.server_mut(OWNED).unwrap().options.banner_allowed = false;
        assert!(refused(&edit(
            &mut world,
            ServerEdit {
                banner: Some(Some(gif(960))),
                ..ServerEdit::default()
            },
        )));
        // A trait takes one of the server's own emojis, not another's.
        let own = world.server(OWNED).unwrap().emojis[0].clone();
        let with = |id| ServerEdit {
            traits: Some(vec![ServerTrait {
                label: "Propio".into(),
                emoji: Some(own.name.clone()),
                emoji_id: Some(id),
                emoji_animated: false,
            }]),
            ..ServerEdit::default()
        };
        assert!(refused(&edit(&mut world, with(rune_model::EmojiId(1)))));
        let saved = edit(&mut world, with(own.id));
        let [Event::ServerChanged(server)] = saved.as_slice() else {
            panic!("{saved:?}")
        };
        assert_eq!(
            server.options.traits.as_ref().unwrap()[0].emoji_id,
            Some(own.id)
        );
        // It stops being a community, and to be one again it needs its
        // channels, verification and everyone scanned.
        let saved = edit(
            &mut world,
            ServerEdit {
                community: Some(false),
                rules_channel: Some(None),
                ..ServerEdit::default()
            },
        );
        let [Event::ServerChanged(server)] = saved.as_slice() else {
            panic!("{saved:?}")
        };
        assert!(!server.options.community && !server.options.onboarding);
        let back = ServerEdit {
            community: Some(true),
            ..ServerEdit::default()
        };
        assert!(refused(&edit(&mut world, back.clone())));
        let rules = world.channel_named(OWNED, "limn-core", ChannelKind::Text);
        let saved = edit(
            &mut world,
            ServerEdit {
                rules_channel: Some(rules),
                ..back
            },
        );
        let [Event::ServerChanged(server)] = saved.as_slice() else {
            panic!("{saved:?}")
        };
        assert!(server.options.community && server.options.community_ready());
    }

    #[test]
    fn a_forum_s_archive_time_and_reaction_are_the_service_s_and_the_server_s() {
        let mut world = world();
        let own = world.server(OWNED).unwrap().emojis[0].id;
        let save = |world: &mut World, forum: rune_model::ForumDefaults| {
            run(
                world,
                Command::EditChannel {
                    channel: FORUM,
                    edit: ChannelEdit {
                        forum: Some(forum),
                        ..ChannelEdit::default()
                    },
                },
            )
        };
        for wrong in [
            rune_model::ForumDefaults {
                archive_after: Some(42),
                ..Default::default()
            },
            rune_model::ForumDefaults {
                reaction_emoji: Some(rune_model::EmojiId(1)),
                ..Default::default()
            },
        ] {
            assert!(refused(&save(&mut world, wrong)));
        }
        let saved = save(
            &mut world,
            rune_model::ForumDefaults {
                reaction_emoji: Some(own),
                archive_after: Some(10080),
                match_all_tags: true,
                ..Default::default()
            },
        );
        let Some(Event::ChannelChanged(forum)) = saved.first() else {
            panic!("{saved:?}")
        };
        assert_eq!(forum.forum.reaction_emoji, Some(own));
        assert!(forum.forum.match_all_tags && forum.forum.archive_after == Some(10080));
    }
}
