//! A server's administration lists (bans, audit log, invitations, members,
//! webhooks, integrations) loaded a page at a time and kept current with
//! live events, and the changes to them shown before the provider answers.
//! Also the GIF favorites a provider keeps for the account.
//!
//! Paging never repeats an item: pages are merged by id, a page that does
//! not continue where the list ends is dropped instead of leaving a hole,
//! and an item reported live is placed only where the loaded pages reach.
//! A revocation or a deletion confirmed here disappears at once; a refusal
//! puts it back where it was, and a confirmation forgets how it was.

use rune_model::{
    AuditEntry, AuditQuery, Ban, CustomEmoji, ExpressionId, Integration, IntegrationId, Invite,
    JoinMethod, Member, MemberOrder, MemberQuery, RoleId, Server, ServerId, Sticker, Timestamp,
    UserId, Webhook, WebhookId,
};
use rune_provider::{Command, Event, Failure, FavoriteGif};

use crate::{Applied, Store};

/// Audit entries kept for one query at most: older ones stop being offered.
pub const AUDIT_MAX: usize = 500;
/// Filters of one server's log kept at once; the oldest goes first.
const AUDIT_QUERIES: usize = 4;

/// A list loaded a page at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct Paged<T> {
    pub items: Vec<T>,
    /// The provider said there is nothing more.
    pub reached_end: bool,
}

/// What is loaded of one server's administration.
#[derive(Debug, Default)]
pub(crate) struct AdminLists {
    bans: Option<Paged<Ban>>,
    audit: Vec<(AuditQuery, Paged<AuditEntry>)>,
    invites: Option<Vec<Invite>>,
    members: Option<MemberPages>,
    webhooks: Option<Vec<Webhook>>,
    integrations: Option<Vec<Integration>>,
    // Server administration, rest (P7).
    role_counts: Option<Vec<(RoleId, u32)>>,
}

/// How far a server's members were read a page at a time. The members
/// themselves join the store's, one per person, so a page and a live
/// change never show anyone twice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemberPages {
    /// What asks for the next page; `None` once there is no more.
    pub next: Option<String>,
    /// How many members the server has, when the provider says.
    pub total: Option<u32>,
    // Server administration, rest (P7).
    /// What the pages were asked for; a page of another query starts over.
    pub query: MemberQuery,
    /// Who the pages listed, in their order, once each.
    pub listed: Vec<UserId>,
    /// How each of them joined, when the provider said.
    pub joins: std::collections::HashMap<UserId, JoinMethod>,
    // Members with an order or a filter (2026-10-19).
    /// When their first page arrived: who joins after it is listed live.
    pub since: Timestamp,
}

/// The GIF favorites a provider keeps, and how they were before a change
/// shown ahead of its answer.
#[derive(Debug, Default)]
pub(crate) struct Favorites {
    gifs: Option<Vec<FavoriteGif>>,
    before: Option<Vec<FavoriteGif>>,
}

/// What a change shown ahead of the provider touches.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Target {
    Ban(UserId),
    Invite(String),
    Expression(ExpressionId),
    Webhook(WebhookId),
    Integration(IntegrationId),
}

/// A change shown ahead: what it replaced, where it was, and the new name
/// for a rename (`None`: it was removed).
#[derive(Clone, Debug)]
pub(crate) struct Ahead {
    undo: Undo,
    index: usize,
    rename: Option<String>,
}

#[derive(Clone, Debug)]
enum Undo {
    Ban(Ban),
    Invite(Invite),
    Emoji(CustomEmoji),
    Sticker(Sticker),
    Webhook(Webhook),
    Integration(Integration),
}

impl Store {
    /// The bans of `server` loaded so far, by ascending user id.
    pub fn bans(&self, server: ServerId) -> Option<&Paged<Ban>> {
        self.admin.get(&server)?.bans.as_ref()
    }

    /// The entries of `server`'s log matching `query` loaded so far, newest first.
    pub fn audit(&self, server: ServerId, query: &AuditQuery) -> Option<&Paged<AuditEntry>> {
        self.admin
            .get(&server)?
            .audit
            .iter()
            .find(|(of, _)| of == query)
            .map(|(_, page)| page)
    }

    /// The invitations of `server`, once loaded.
    pub fn invites(&self, server: ServerId) -> Option<&[Invite]> {
        self.admin.get(&server)?.invites.as_deref()
    }

    /// How far `server`'s members were read a page at a time, once begun.
    pub fn member_pages(&self, server: ServerId) -> Option<&MemberPages> {
        self.admin.get(&server)?.members.as_ref()
    }

    /// The webhooks of `server`, once loaded.
    pub fn webhooks(&self, server: ServerId) -> Option<&[Webhook]> {
        self.admin.get(&server)?.webhooks.as_deref()
    }

    /// The integrations of `server`, once loaded.
    pub fn integrations(&self, server: ServerId) -> Option<&[Integration]> {
        self.admin.get(&server)?.integrations.as_deref()
    }

    /// The GIF favorites the provider keeps, once loaded, the first added first.
    pub fn gif_favorites(&self) -> Option<&[FavoriteGif]> {
        self.favorites.gifs.as_deref()
    }

    /// How many members have `role` of `server`, as the provider last
    /// counted them; `None` before it did.
    pub fn role_count(&self, server: ServerId, role: RoleId) -> Option<u32> {
        let counts = self.admin.get(&server)?.role_counts.as_ref()?;
        Some(
            counts
                .iter()
                .find(|(of, _)| *of == role)
                .map_or(0, |(_, count)| *count),
        )
    }

    // Members with an order or a filter (2026-10-19).
    /// List `member`, new to the store, in its server's member pages when
    /// they joined after the first page arrived and the pages' filter takes
    /// them: where the pages' order puts them, or nowhere yet while a page
    /// still to come would bring them.
    pub(crate) fn list_newcomer(&mut self, member: &Member) {
        let recent = self.wall().plus_minutes(-7 * 24 * 60);
        let Some(pages) = self
            .admin
            .get_mut(&member.server)
            .and_then(|lists| lists.members.as_mut())
        else {
            return;
        };
        let Some(joined) = member.joined.filter(|joined| *joined >= pages.since) else {
            return;
        };
        if pages.listed.contains(&member.user) || (pages.query.recent && joined < recent) {
            return;
        }
        let user = member.user;
        let at = match pages.query.order {
            MemberOrder::NewestMembers => Some(0),
            MemberOrder::OldestMembers => None,
            // Ids grow with time: a lower id, an older account.
            MemberOrder::NewestAccounts => pages.listed.iter().position(|other| *other < user),
            MemberOrder::OldestAccounts => pages.listed.iter().position(|other| *other > user),
        };
        match at {
            Some(at) => pages.listed.insert(at, user),
            None if pages.next.is_none() => pages.listed.push(user),
            None => return,
        }
        pages.total = pages.total.map(|total| total.saturating_add(1));
    }

    /// Show the effect of an administration command before the provider
    /// answers: an unban, a revoked invitation, a renamed or deleted
    /// expression. Other commands change nothing here.
    pub fn ahead(&mut self, command: &Command) {
        if let Command::SetGifFavorite { gif, favorite } = command {
            self.favorite_ahead(gif, *favorite);
            return;
        }
        let (server, target, rename) = match command {
            Command::Unban { server, user } => (*server, Target::Ban(*user), None),
            Command::RevokeInvite { server, code } => (*server, Target::Invite(code.clone()), None),
            Command::DeleteExpression { server, expression } => {
                (*server, Target::Expression(*expression), None)
            }
            Command::RenameExpression {
                server,
                expression,
                name,
            } => (
                *server,
                Target::Expression(*expression),
                Some(name.trim().to_owned()),
            ),
            Command::DeleteWebhook { server, webhook } => {
                (*server, Target::Webhook(*webhook), None)
            }
            Command::DeleteIntegration {
                server,
                integration,
            } => (*server, Target::Integration(*integration), None),
            _ => return,
        };
        if let Some(ahead) = self.show_ahead(server, &target, rename) {
            self.ahead.insert((server, target), ahead);
            self.revision += 1;
        }
    }

    /// Make the change in what is shown, returning how it was.
    fn show_ahead(
        &mut self,
        server: ServerId,
        target: &Target,
        rename: Option<String>,
    ) -> Option<Ahead> {
        let (undo, index) = match target {
            Target::Ban(user) => {
                let bans = &mut self.admin.get_mut(&server)?.bans.as_mut()?.items;
                let index = bans.iter().position(|ban| ban.user == *user)?;
                (Undo::Ban(bans.remove(index)), index)
            }
            Target::Invite(code) => {
                let invites = self.admin.get_mut(&server)?.invites.as_mut()?;
                let index = invites.iter().position(|invite| invite.code == *code)?;
                (Undo::Invite(invites.remove(index)), index)
            }
            Target::Expression(expression) => {
                let found = self.servers.iter_mut().find(|known| known.id == server)?;
                change_expression(found, *expression, rename.as_deref())?
            }
            Target::Webhook(id) => {
                let webhooks = self.admin.get_mut(&server)?.webhooks.as_mut()?;
                let index = webhooks.iter().position(|known| known.id == *id)?;
                (Undo::Webhook(webhooks.remove(index)), index)
            }
            Target::Integration(id) => {
                let integrations = self.admin.get_mut(&server)?.integrations.as_mut()?;
                let index = integrations.iter().position(|known| known.id == *id)?;
                (Undo::Integration(integrations.remove(index)), index)
            }
        };
        Some(Ahead {
            undo,
            index,
            rename,
        })
    }

    /// Put back what a refused change took away or renamed.
    fn undo_ahead(&mut self, server: ServerId, target: &Target) {
        let Some(Ahead { undo, index, .. }) = self.ahead.remove(&(server, target.clone())) else {
            return;
        };
        self.revision += 1;
        match undo {
            Undo::Ban(ban) => {
                if let Some(bans) = self.admin.get_mut(&server).and_then(|l| l.bans.as_mut()) {
                    insert_ban(&mut bans.items, ban);
                }
            }
            Undo::Invite(invite) => {
                if let Some(invites) = self.admin.get_mut(&server).and_then(|l| l.invites.as_mut())
                {
                    if !invites.iter().any(|known| known.code == invite.code) {
                        invites.insert(index.min(invites.len()), invite);
                    }
                }
            }
            Undo::Emoji(emoji) => {
                if let Some(found) = self.servers.iter_mut().find(|known| known.id == server) {
                    put_back(&mut found.emojis, emoji, index, |e| e.id);
                }
            }
            Undo::Sticker(sticker) => {
                if let Some(found) = self.servers.iter_mut().find(|known| known.id == server) {
                    put_back(&mut found.stickers, sticker, index, |s| s.id);
                }
            }
            Undo::Webhook(webhook) => {
                if let Some(list) = self
                    .admin
                    .get_mut(&server)
                    .and_then(|l| l.webhooks.as_mut())
                {
                    put_back(list, webhook, index, |w| w.id);
                }
            }
            Undo::Integration(integration) => {
                if let Some(list) = self
                    .admin
                    .get_mut(&server)
                    .and_then(|l| l.integrations.as_mut())
                {
                    put_back(list, integration, index, |i| i.id);
                }
            }
        }
    }

    /// Show a favorite added or taken out before the provider answers.
    fn favorite_ahead(&mut self, gif: &FavoriteGif, favorite: bool) {
        let Some(gifs) = self.favorites.gifs.as_mut() else {
            return;
        };
        let before = gifs.clone();
        let had = gifs.iter().any(|known| known.url == gif.url);
        match (favorite, had) {
            (true, false) => gifs.push(gif.clone()),
            (false, true) => gifs.retain(|known| known.url != gif.url),
            _ => return,
        }
        self.favorites.before.get_or_insert(before);
        self.revision += 1;
    }

    /// A server came from the provider: changes to its expressions it
    /// already has are confirmed; the rest are shown again over it.
    pub(crate) fn reapply_expressions(&mut self, server: ServerId) {
        let pending: Vec<(Target, Option<String>)> = self
            .ahead
            .iter()
            .filter(|((of, target), _)| *of == server && matches!(target, Target::Expression(_)))
            .map(|((_, target), ahead)| (target.clone(), ahead.rename.clone()))
            .collect();
        for (target, rename) in pending {
            self.ahead.remove(&(server, target.clone()));
            let Target::Expression(expression) = target else {
                continue;
            };
            let Some(found) = self.servers.iter().find(|known| known.id == server) else {
                continue;
            };
            let current = match expression {
                ExpressionId::Emoji(id) => found
                    .emojis
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.name.as_str()),
                ExpressionId::Sticker(id) => found
                    .stickers
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.as_str()),
            };
            // Gone, or already as asked: nothing is left to show ahead.
            if current.is_none() || current == rename.as_deref() {
                continue;
            }
            if let Some(ahead) = self.show_ahead(server, &target, rename) {
                self.ahead.insert((server, target), ahead);
            }
        }
    }

    /// Undo what a refused administration command showed ahead.
    pub(crate) fn admin_failed(&mut self, failure: &Failure) {
        if *failure == Failure::GifFavorites {
            if let Some(before) = self.favorites.before.take() {
                self.favorites.gifs = Some(before);
                self.revision += 1;
            }
            return;
        }
        let (server, target) = match failure {
            Failure::Member { server, user } => (*server, Target::Ban(*user)),
            Failure::Invite { server, code } => (*server, Target::Invite(code.clone())),
            Failure::Expression { server, expression } => {
                (*server, Target::Expression(*expression))
            }
            Failure::Webhook {
                server,
                webhook: Some(webhook),
            } => (*server, Target::Webhook(*webhook)),
            Failure::Integration {
                server,
                integration,
            } => (*server, Target::Integration(*integration)),
            _ => return,
        };
        self.undo_ahead(server, &target);
    }

    pub(crate) fn forget_admin(&mut self, server: Option<ServerId>) {
        match server {
            Some(server) => {
                self.admin.remove(&server);
                self.ahead.retain(|(of, _), _| *of != server);
            }
            None => {
                self.admin.clear();
                self.ahead.clear();
                self.favorites = Favorites::default();
            }
        }
    }

    /// Apply a page or a live change of a server's administration lists.
    pub(crate) fn apply_admin(&mut self, event: Event) -> Applied {
        match event {
            Event::BansLoaded {
                server,
                after,
                bans,
                reached_end,
                ..
            } => {
                let ordered = bans.windows(2).all(|pair| pair[0].user < pair[1].user)
                    && bans
                        .first()
                        .is_none_or(|first| after.is_none_or(|after| first.user > after));
                if !ordered {
                    return Applied::Nothing;
                }
                let lists = self.admin.entry(server).or_default();
                match (after, lists.bans.as_mut()) {
                    (None, _) => {
                        lists.bans = Some(Paged {
                            items: bans,
                            reached_end,
                        })
                    }
                    (Some(after), Some(list)) => {
                        let end = list.items.last().map(|ban| ban.user);
                        // A page from past the end would leave a hole.
                        if end.is_some_and(|end| after > end) {
                            return Applied::Nothing;
                        }
                        if end.is_none_or(|end| after == end) {
                            list.reached_end = reached_end;
                        }
                        for ban in bans {
                            insert_ban(&mut list.items, ban);
                        }
                    }
                    (Some(_), None) => return Applied::Nothing,
                }
                self.hide_removed(server);
                Applied::Bans(server)
            }
            Event::BanAdded { server, ban } => {
                if let Some(list) = self.admin.get_mut(&server).and_then(|l| l.bans.as_mut()) {
                    // Past the loaded pages it comes with a later one.
                    if list.reached_end || list.items.last().is_some_and(|end| ban.user <= end.user)
                    {
                        insert_ban(&mut list.items, ban);
                    }
                }
                Applied::Bans(server)
            }
            Event::BanRemoved { server, user } => {
                self.ahead.remove(&(server, Target::Ban(user)));
                if let Some(list) = self.admin.get_mut(&server).and_then(|l| l.bans.as_mut()) {
                    list.items.retain(|ban| ban.user != user);
                }
                Applied::Bans(server)
            }
            Event::AuditLoaded {
                server,
                query,
                before,
                entries,
                reached_end,
                ..
            } => {
                let valid = entries.windows(2).all(|pair| pair[0].id > pair[1].id)
                    && entries.iter().all(|entry| {
                        query.matches(entry) && before.is_none_or(|before| entry.id < before)
                    });
                if !valid {
                    return Applied::Nothing;
                }
                let lists = self.admin.entry(server).or_default();
                let found = lists.audit.iter().position(|(of, _)| *of == query);
                match (before, found) {
                    (None, None) => {
                        if lists.audit.len() >= AUDIT_QUERIES {
                            lists.audit.remove(0);
                        }
                        lists.audit.push((
                            query,
                            Paged {
                                items: entries,
                                reached_end,
                            },
                        ));
                    }
                    (None, Some(index)) => {
                        let list = &mut lists.audit[index].1;
                        let newest = list.items.first().map(|entry| entry.id);
                        // More new entries than a page: what lies between is unknown.
                        let gap = !reached_end
                            && entries.last().is_some_and(|oldest| {
                                newest.is_none_or(|newest| oldest.id > newest)
                            });
                        if gap || newest.is_none() {
                            *list = Paged {
                                items: entries,
                                reached_end,
                            };
                        } else {
                            list.reached_end |= reached_end;
                            merge_entries(&mut list.items, entries);
                        }
                    }
                    (Some(before), Some(index)) => {
                        let list = &mut lists.audit[index].1;
                        let oldest = list.items.last().map(|entry| entry.id);
                        if oldest.is_some_and(|oldest| before < oldest) {
                            return Applied::Nothing;
                        }
                        if oldest.is_none_or(|oldest| before == oldest) {
                            list.reached_end = reached_end;
                        }
                        merge_entries(&mut list.items, entries);
                    }
                    (Some(_), None) => return Applied::Nothing,
                }
                Applied::Audit(server)
            }
            Event::InvitesLoaded {
                server, invites, ..
            } => {
                let mut unique: Vec<Invite> = Vec::with_capacity(invites.len());
                for invite in invites {
                    if !unique.iter().any(|known| known.code == invite.code) {
                        unique.push(invite);
                    }
                }
                self.admin.entry(server).or_default().invites = Some(unique);
                self.hide_removed(server);
                Applied::Invites(server)
            }
            Event::InviteRemoved { server, code } => {
                self.ahead.remove(&(server, Target::Invite(code.clone())));
                if let Some(invites) = self.admin.get_mut(&server).and_then(|l| l.invites.as_mut())
                {
                    invites.retain(|invite| invite.code != code);
                }
                Applied::Invites(server)
            }
            Event::InviteCreated {
                request,
                server,
                invite,
            } => {
                let code = invite.code.clone();
                if let Some(invites) = self.admin.get_mut(&server).and_then(|l| l.invites.as_mut())
                {
                    if !invites.iter().any(|known| known.code == code) {
                        invites.insert(0, invite);
                    }
                }
                Applied::InviteCreated {
                    request,
                    server,
                    code,
                }
            }
            Event::MembersLoaded {
                server,
                after,
                members,
                next,
                total,
                query,
                joins,
                ..
            } => {
                if self.server(server).is_none() || members.iter().any(|m| m.server != server) {
                    return Applied::Nothing;
                }
                let users: Vec<UserId> = members.iter().map(|member| member.user).collect();
                // One per person: a page joins whoever is known already.
                for member in members {
                    self.apply_management(Event::MemberChanged(member));
                }
                let since = self.wall();
                let lists = self.admin.entry(server).or_default();
                let pages = match (&after, lists.members.as_mut()) {
                    (None, _) => lists.members.insert(MemberPages {
                        next,
                        total,
                        query,
                        since,
                        ..MemberPages::default()
                    }),
                    // Only a page that continues the last one of the same
                    // query moves the cursor.
                    (Some(after), Some(pages))
                        if pages.next.as_ref() == Some(after) && pages.query == query =>
                    {
                        pages.next = next;
                        pages.total = total.or(pages.total);
                        pages
                    }
                    _ => return Applied::MemberPage(server),
                };
                for user in users {
                    if !pages.listed.contains(&user) {
                        pages.listed.push(user);
                    }
                }
                pages.joins.extend(joins);
                Applied::MemberPage(server)
            }
            // Server administration, rest (P7).
            Event::RoleCounts { server, counts } => {
                if self.server(server).is_none() {
                    return Applied::Nothing;
                }
                self.admin.entry(server).or_default().role_counts = Some(counts);
                Applied::RoleCounts(server)
            }
            Event::Pruned {
                request,
                server,
                days,
                count,
                removed,
            } => Applied::Pruned {
                request,
                server,
                days,
                count,
                removed,
            },
            Event::WebhooksLoaded {
                server, webhooks, ..
            } => {
                let mut unique: Vec<Webhook> = Vec::with_capacity(webhooks.len());
                for webhook in webhooks {
                    if !unique.iter().any(|known| known.id == webhook.id) {
                        unique.push(webhook);
                    }
                }
                self.admin.entry(server).or_default().webhooks = Some(unique);
                self.hide_removed(server);
                Applied::Webhooks(server)
            }
            Event::WebhookChanged { server, webhook } => {
                if let Some(list) = self
                    .admin
                    .get_mut(&server)
                    .and_then(|l| l.webhooks.as_mut())
                {
                    match list.iter_mut().find(|known| known.id == webhook.id) {
                        Some(known) => *known = webhook,
                        None => list.push(webhook),
                    }
                }
                self.hide_removed(server);
                Applied::Webhooks(server)
            }
            Event::WebhookRemoved { server, webhook } => {
                self.ahead.remove(&(server, Target::Webhook(webhook)));
                if let Some(list) = self
                    .admin
                    .get_mut(&server)
                    .and_then(|l| l.webhooks.as_mut())
                {
                    list.retain(|known| known.id != webhook);
                }
                Applied::Webhooks(server)
            }
            Event::IntegrationsLoaded {
                server,
                integrations,
                ..
            } => {
                let mut unique: Vec<Integration> = Vec::with_capacity(integrations.len());
                for integration in integrations {
                    if !unique.iter().any(|known| known.id == integration.id) {
                        unique.push(integration);
                    }
                }
                self.admin.entry(server).or_default().integrations = Some(unique);
                self.hide_removed(server);
                Applied::Integrations(server)
            }
            Event::IntegrationRemoved {
                server,
                integration,
            } => {
                self.ahead
                    .remove(&(server, Target::Integration(integration)));
                if let Some(list) = self
                    .admin
                    .get_mut(&server)
                    .and_then(|l| l.integrations.as_mut())
                {
                    list.retain(|known| known.id != integration);
                }
                Applied::Integrations(server)
            }
            Event::GifFavorites(mut gifs) => {
                let mut seen = std::collections::HashSet::new();
                gifs.retain(|gif| seen.insert(gif.url.clone()));
                self.favorites = Favorites {
                    gifs: Some(gifs),
                    before: None,
                };
                Applied::GifFavorites
            }
            _ => Applied::Nothing,
        }
    }

    /// A list came again from the provider: what is being removed here
    /// stays hidden until the provider answers.
    fn hide_removed(&mut self, server: ServerId) {
        let pending: Vec<Target> = self
            .ahead
            .keys()
            .filter(|(of, target)| *of == server && !matches!(target, Target::Expression(_)))
            .map(|(_, target)| target.clone())
            .collect();
        for target in pending {
            if let Some(ahead) = self.show_ahead(server, &target, None) {
                self.ahead.insert((server, target), ahead);
            }
        }
    }
}

/// Rename `expression` of `server`, or remove it, returning how it was.
fn change_expression(
    server: &mut Server,
    expression: ExpressionId,
    rename: Option<&str>,
) -> Option<(Undo, usize)> {
    match expression {
        ExpressionId::Emoji(id) => {
            let index = server.emojis.iter().position(|e| e.id == id)?;
            let before = server.emojis[index].clone();
            match rename {
                Some(name) => server.emojis[index].name = name.to_owned(),
                None => {
                    server.emojis.remove(index);
                }
            }
            Some((Undo::Emoji(before), index))
        }
        ExpressionId::Sticker(id) => {
            let index = server.stickers.iter().position(|s| s.id == id)?;
            let before = server.stickers[index].clone();
            match rename {
                Some(name) => server.stickers[index].name = name.to_owned(),
                None => {
                    server.stickers.remove(index);
                }
            }
            Some((Undo::Sticker(before), index))
        }
    }
}

/// Put `item` back: over itself when it is still there, else where it was.
fn put_back<T, K: PartialEq>(items: &mut Vec<T>, item: T, index: usize, key: impl Fn(&T) -> K) {
    match items.iter().position(|known| key(known) == key(&item)) {
        Some(found) => items[found] = item,
        None => items.insert(index.min(items.len()), item),
    }
}

/// Keep bans ordered by user, one per user.
fn insert_ban(bans: &mut Vec<Ban>, ban: Ban) {
    match bans.binary_search_by_key(&ban.user, |known| known.user) {
        Ok(found) => bans[found] = ban,
        Err(at) => bans.insert(at, ban),
    }
}

/// Merge `page` into `items`, newest first, one entry per id, at most [`AUDIT_MAX`].
fn merge_entries(items: &mut Vec<AuditEntry>, page: Vec<AuditEntry>) {
    for entry in page {
        if let Err(at) = items.binary_search_by(|known| entry.id.cmp(&known.id)) {
            items.insert(at, entry);
        }
    }
    items.truncate(AUDIT_MAX);
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rune_model::{AuditAction, AuditActionKind, EmojiId, Timestamp};
    use rune_provider::mock::MockProvider;
    use rune_provider::Provider;

    use super::*;

    const LAB: ServerId = ServerId(2);

    fn store() -> Store {
        let provider = MockProvider::new(Default::default());
        let mut store = Store::new();
        store.provider = Some(provider.info());
        store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
        store
    }

    fn entry(id: u64, actor: u64) -> AuditEntry {
        AuditEntry {
            id,
            actor: UserId(actor),
            action: AuditAction::ServerUpdated,
            when: Timestamp(id as i64),
            reason: None,
        }
    }

    fn page(before: Option<u64>, ids: &[u64], reached_end: bool) -> Event {
        Event::AuditLoaded {
            request: 1,
            server: LAB,
            query: AuditQuery::default(),
            before,
            entries: ids.iter().map(|id| entry(*id, 7)).collect(),
            reached_end,
        }
    }

    fn ids(store: &Store) -> Vec<u64> {
        store
            .audit(LAB, &AuditQuery::default())
            .unwrap()
            .items
            .iter()
            .map(|entry| entry.id)
            .collect()
    }

    #[test]
    fn audit_pages_merge_without_duplicates_when_entries_arrive_meanwhile() {
        let mut store = store();
        let now = Instant::now();
        store.apply(page(None, &[10, 9, 8], false), now);
        // Two new entries were made; the head is asked for again.
        store.apply(page(None, &[12, 11, 10], false), now);
        assert_eq!(ids(&store), [12, 11, 10, 9, 8]);
        // The next page continues from the oldest loaded entry.
        store.apply(page(Some(8), &[7, 6], true), now);
        assert_eq!(ids(&store), [12, 11, 10, 9, 8, 7, 6]);
        assert!(
            store
                .audit(LAB, &AuditQuery::default())
                .unwrap()
                .reached_end
        );
        // The same page twice, or an overlapping one, repeats nothing.
        store.apply(page(Some(9), &[8, 7], false), now);
        assert_eq!(ids(&store), [12, 11, 10, 9, 8, 7, 6]);
        assert!(
            store
                .audit(LAB, &AuditQuery::default())
                .unwrap()
                .reached_end
        );
    }

    #[test]
    fn a_head_past_a_gap_replaces_and_a_page_past_the_end_is_dropped() {
        let mut store = store();
        let now = Instant::now();
        store.apply(page(None, &[10, 9], false), now);
        // More new entries than a page: what lies between is unknown.
        store.apply(page(None, &[30, 29], false), now);
        assert_eq!(ids(&store), [30, 29]);
        // A late page that continued the old list would leave a hole.
        store.apply(page(Some(9), &[8, 7], false), now);
        assert_eq!(ids(&store), [30, 29]);
        // A page out of order, or not older than its cursor, is refused.
        assert_eq!(
            store.apply(page(Some(29), &[27, 28], false), now),
            Applied::Nothing
        );
        assert_eq!(
            store.apply(page(Some(29), &[29], false), now),
            Applied::Nothing
        );
    }

    #[test]
    fn each_filter_keeps_its_own_pages_and_a_mismatching_page_is_refused() {
        let mut store = store();
        let now = Instant::now();
        let mine = AuditQuery {
            actor: Some(UserId(7)),
            action: Some(AuditActionKind::ServerUpdated),
        };
        store.apply(page(None, &[5, 4], true), now);
        store.apply(
            Event::AuditLoaded {
                request: 2,
                server: LAB,
                query: mine,
                before: None,
                entries: vec![entry(5, 7)],
                reached_end: true,
            },
            now,
        );
        assert_eq!(store.audit(LAB, &mine).unwrap().items.len(), 1);
        assert_eq!(ids(&store), [5, 4]);
        let other = AuditQuery {
            actor: Some(UserId(8)),
            action: None,
        };
        let refused = store.apply(
            Event::AuditLoaded {
                request: 3,
                server: LAB,
                query: other,
                before: None,
                entries: vec![entry(6, 7)],
                reached_end: true,
            },
            now,
        );
        assert_eq!(refused, Applied::Nothing);
        assert!(store.audit(LAB, &other).is_none());
    }

    fn ban(user: u64) -> Ban {
        Ban {
            user: UserId(user),
            reason: None,
        }
    }

    fn banned(store: &Store) -> Vec<u64> {
        store
            .bans(LAB)
            .unwrap()
            .items
            .iter()
            .map(|ban| ban.user.0)
            .collect()
    }

    fn bans_page(after: Option<u64>, users: &[u64], reached_end: bool) -> Event {
        Event::BansLoaded {
            request: 1,
            server: LAB,
            after: after.map(UserId),
            bans: users.iter().map(|user| ban(*user)).collect(),
            reached_end,
        }
    }

    #[test]
    fn live_bans_go_only_where_the_loaded_pages_reach() {
        let mut store = store();
        let now = Instant::now();
        store.apply(bans_page(None, &[10, 20], false), now);
        // Within the loaded range: shown now, once.
        store.apply(
            Event::BanAdded {
                server: LAB,
                ban: ban(15),
            },
            now,
        );
        // Past it: it comes with its page, not twice.
        store.apply(
            Event::BanAdded {
                server: LAB,
                ban: ban(25),
            },
            now,
        );
        assert_eq!(banned(&store), [10, 15, 20]);
        store.apply(bans_page(Some(20), &[25, 30], true), now);
        assert_eq!(banned(&store), [10, 15, 20, 25, 30]);
        store.apply(
            Event::BanAdded {
                server: LAB,
                ban: ban(40),
            },
            now,
        );
        store.apply(
            Event::BanRemoved {
                server: LAB,
                user: UserId(10),
            },
            now,
        );
        assert_eq!(banned(&store), [15, 20, 25, 30, 40]);
        // A page that starts past the end is dropped.
        store.apply(bans_page(Some(99), &[100], true), now);
        assert_eq!(banned(&store), [15, 20, 25, 30, 40]);
    }

    #[test]
    fn an_unban_shows_at_once_and_a_refusal_puts_it_back() {
        let mut store = store();
        let now = Instant::now();
        store.apply(bans_page(None, &[10, 20, 30], true), now);
        let unban = Command::Unban {
            server: LAB,
            user: UserId(20),
        };
        store.ahead(&unban);
        assert_eq!(banned(&store), [10, 30]);
        // The list comes again before the answer: still hidden.
        store.apply(bans_page(None, &[10, 20, 30], true), now);
        assert_eq!(banned(&store), [10, 30]);
        store.apply(
            Event::Failed {
                failure: Failure::of(&unban).unwrap(),
                reason: "no tienes permiso".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(banned(&store), [10, 20, 30]);
        // Confirmed: it stays gone, and a later refusal changes nothing.
        store.ahead(&unban);
        store.apply(
            Event::BanRemoved {
                server: LAB,
                user: UserId(20),
            },
            now,
        );
        store.apply(
            Event::Failed {
                failure: Failure::of(&unban).unwrap(),
                reason: "tarde".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(banned(&store), [10, 30]);
    }

    #[test]
    fn expressions_change_ahead_and_survive_an_unrelated_server_update() {
        let mut store = store();
        let now = Instant::now();
        let first = store.server(LAB).unwrap().emojis[0].clone();
        let expression = ExpressionId::Emoji(first.id);
        let rename = Command::RenameExpression {
            server: LAB,
            expression,
            name: " cangrejo ".into(),
        };
        store.ahead(&rename);
        assert_eq!(store.server(LAB).unwrap().emojis[0].name, "cangrejo");
        // The provider reports the server for something else meanwhile.
        let unrelated = store.server(LAB).unwrap().clone();
        let mut old = unrelated.clone();
        old.emojis[0] = first.clone();
        old.description = Some("otra cosa".into());
        store.apply(Event::ServerChanged(old), now);
        assert_eq!(store.server(LAB).unwrap().emojis[0].name, "cangrejo");
        store.apply(
            Event::Failed {
                failure: Failure::of(&rename).unwrap(),
                reason: "nombre inválido".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(store.server(LAB).unwrap().emojis[0], first);
        // A deletion: gone at once, back where it was when refused.
        let delete = Command::DeleteExpression {
            server: LAB,
            expression,
        };
        store.ahead(&delete);
        assert!(store
            .server(LAB)
            .unwrap()
            .emojis
            .iter()
            .all(|emoji| emoji.id != first.id));
        store.apply(
            Event::Failed {
                failure: Failure::of(&delete).unwrap(),
                reason: "no".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(store.server(LAB).unwrap().emojis[0].id, first.id);
        // Unknown targets change nothing.
        store.ahead(&Command::DeleteExpression {
            server: LAB,
            expression: ExpressionId::Emoji(EmojiId(1)),
        });
        assert!(store.ahead.is_empty());
    }

    #[test]
    fn a_revoked_invitation_is_hidden_until_answered() {
        let mut store = store();
        let now = Instant::now();
        let invite = |code: &str| Invite {
            code: code.into(),
            channel: None,
            inviter: None,
            uses: None,
            max_uses: None,
            expires: None,
            temporary: false,
        };
        store.apply(
            Event::InvitesLoaded {
                request: 1,
                server: LAB,
                invites: vec![invite("a"), invite("b"), invite("a")],
            },
            now,
        );
        assert_eq!(store.invites(LAB).unwrap().len(), 2, "one per code");
        let revoke = Command::RevokeInvite {
            server: LAB,
            code: "a".into(),
        };
        store.ahead(&revoke);
        assert_eq!(store.invites(LAB).unwrap(), [invite("b")]);
        store.apply(
            Event::Failed {
                failure: Failure::of(&revoke).unwrap(),
                reason: "no".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(store.invites(LAB).unwrap(), [invite("a"), invite("b")]);
        store.ahead(&revoke);
        store.apply(
            Event::InviteRemoved {
                server: LAB,
                code: "a".into(),
            },
            now,
        );
        assert_eq!(store.invites(LAB).unwrap(), [invite("b")]);
        assert!(store.ahead.is_empty());
        // A new one joins the list once, first.
        let made = store.apply(
            Event::InviteCreated {
                request: 9,
                server: LAB,
                invite: invite("c"),
            },
            now,
        );
        assert_eq!(
            made,
            Applied::InviteCreated {
                request: 9,
                server: LAB,
                code: "c".into()
            }
        );
        store.apply(
            Event::InviteCreated {
                request: 9,
                server: LAB,
                invite: invite("c"),
            },
            now,
        );
        assert_eq!(store.invites(LAB).unwrap(), [invite("c"), invite("b")]);
    }

    fn member_page(store: &Store, after: Option<&str>, users: &[u64], next: Option<&str>) -> Event {
        let template = store.members(LAB)[0].clone();
        Event::MembersLoaded {
            request: 1,
            server: LAB,
            after: after.map(str::to_owned),
            members: users
                .iter()
                .map(|user| rune_model::Member {
                    user: UserId(*user),
                    ..template.clone()
                })
                .collect(),
            next: next.map(str::to_owned),
            total: Some(900),
            query: Default::default(),
            joins: Vec::new(),
        }
    }

    #[test]
    fn member_pages_join_the_members_once_and_only_a_continuing_page_moves_on() {
        let mut store = store();
        let now = Instant::now();
        let known = store.members(LAB).len();
        let first = store.members(LAB)[0].user.0;
        // A page with someone already known, and two new people.
        store.apply(
            member_page(&store, None, &[first, 70_001, 70_002], Some("a")),
            now,
        );
        assert_eq!(store.members(LAB).len(), known + 2);
        assert_eq!(store.member_pages(LAB).unwrap().next.as_deref(), Some("a"));
        // Someone of the next page joins live first: still once.
        let mut joined = store.members(LAB)[0].clone();
        joined.user = UserId(70_003);
        store.apply(Event::MemberChanged(joined), now);
        // A late page that does not continue adds people but not the cursor.
        store.apply(member_page(&store, Some("x"), &[70_004], Some("y")), now);
        assert_eq!(store.member_pages(LAB).unwrap().next.as_deref(), Some("a"));
        store.apply(member_page(&store, Some("a"), &[70_003, 70_005], None), now);
        assert_eq!(store.members(LAB).len(), known + 5);
        let pages = store.member_pages(LAB).unwrap();
        assert_eq!((pages.next.as_deref(), pages.total), (None, Some(900)));
    }

    #[test]
    fn who_joins_after_the_first_page_is_listed_where_the_order_puts_them() {
        static NOW: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1_000_000);
        let mut store = store();
        store.clock = Some(|| Timestamp(NOW.load(std::sync::atomic::Ordering::Relaxed)));
        let now = Instant::now();
        let query = |order, recent| MemberQuery { order, recent };
        let with = |store: &Store, event: Event, query: MemberQuery| match event {
            Event::MembersLoaded {
                request,
                server,
                after,
                members,
                next,
                total,
                joins,
                ..
            } => Event::MembersLoaded {
                request,
                server,
                after,
                members,
                next,
                total,
                query,
                joins,
            },
            _ => unreachable!("{:?}", store.member_pages(LAB)),
        };
        let joining = |store: &Store, user: u64, joined: i64| {
            let mut member = store.members(LAB)[0].clone();
            member.user = UserId(user);
            member.joined = Some(Timestamp(joined));
            Event::MemberChanged(member)
        };
        // Newest accounts first, more pages to come.
        let page = member_page(&store, None, &[70_009, 70_005], Some("a"));
        store.apply(
            with(&store, page, query(MemberOrder::NewestAccounts, false)),
            now,
        );
        // Known from before the pages, or loaded later: not a newcomer.
        store.apply(joining(&store, 70_007, 999_999), now);
        // A newer account than one listed lands before it; an older one
        // than all waits for its page.
        store.apply(joining(&store, 70_006, 1_000_500), now);
        store.apply(joining(&store, 70_001, 1_000_600), now);
        let pages = store.member_pages(LAB).unwrap();
        assert_eq!(
            pages.listed,
            [UserId(70_009), UserId(70_006), UserId(70_005)]
        );
        assert_eq!(pages.total, Some(901));
        // The oldest members first, with no page left: at the end.
        let page = member_page(&store, None, &[70_005], None);
        store.apply(
            with(&store, page, query(MemberOrder::OldestMembers, true)),
            now,
        );
        NOW.store(1_002_000, std::sync::atomic::Ordering::Relaxed);
        store.apply(joining(&store, 70_020, 1_001_000), now);
        let pages = store.member_pages(LAB).unwrap();
        assert_eq!(pages.listed, [UserId(70_005), UserId(70_020)]);
        // Recent ones only: someone whose join is reported older is left out.
        NOW.store(
            1_001_000 + 8 * 24 * 60 * 60_000,
            std::sync::atomic::Ordering::Relaxed,
        );
        store.apply(joining(&store, 70_021, 1_001_500), now);
        assert_eq!(store.member_pages(LAB).unwrap().listed.len(), 2);
    }

    fn webhook(id: u64) -> Webhook {
        Webhook {
            id: WebhookId(id),
            channel: rune_model::ChannelId(1),
            name: format!("w{id}"),
            kind: rune_model::WebhookKind::Incoming,
            creator: None,
            avatar: None,
        }
    }

    #[test]
    fn a_deleted_webhook_hides_at_once_and_comes_back_if_refused() {
        let mut store = store();
        let now = Instant::now();
        store.apply(
            Event::WebhooksLoaded {
                request: 1,
                server: LAB,
                webhooks: vec![webhook(1), webhook(2), webhook(1)],
            },
            now,
        );
        assert_eq!(store.webhooks(LAB).unwrap(), [webhook(1), webhook(2)]);
        let delete = Command::DeleteWebhook {
            server: LAB,
            webhook: WebhookId(1),
        };
        store.ahead(&delete);
        assert_eq!(store.webhooks(LAB).unwrap(), [webhook(2)]);
        // A change of another one meanwhile keeps it hidden.
        let mut renamed = webhook(2);
        renamed.name = "otro".into();
        store.apply(
            Event::WebhookChanged {
                server: LAB,
                webhook: renamed.clone(),
            },
            now,
        );
        assert_eq!(store.webhooks(LAB).unwrap(), [renamed.clone()]);
        store.apply(
            Event::Failed {
                failure: Failure::of(&delete).unwrap(),
                reason: "no".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(store.webhooks(LAB).unwrap(), [webhook(1), renamed.clone()]);
        store.ahead(&delete);
        store.apply(
            Event::WebhookRemoved {
                server: LAB,
                webhook: WebhookId(1),
            },
            now,
        );
        assert_eq!(store.webhooks(LAB).unwrap(), [renamed]);
        assert!(store.ahead.is_empty());
    }

    #[test]
    fn a_favorite_shows_at_once_and_a_refusal_puts_the_list_back() {
        let mut store = store();
        let now = Instant::now();
        let gif = |url: &str| FavoriteGif {
            url: url.into(),
            preview: None,
            width: 0,
            height: 0,
        };
        // Nothing is shown ahead of a list never loaded.
        store.ahead(&Command::SetGifFavorite {
            gif: gif("a"),
            favorite: true,
        });
        assert!(store.gif_favorites().is_none());
        store.apply(Event::GifFavorites(vec![gif("a"), gif("a"), gif("b")]), now);
        assert_eq!(store.gif_favorites().unwrap(), [gif("a"), gif("b")]);
        store.ahead(&Command::SetGifFavorite {
            gif: gif("c"),
            favorite: true,
        });
        store.ahead(&Command::SetGifFavorite {
            gif: gif("a"),
            favorite: false,
        });
        assert_eq!(store.gif_favorites().unwrap(), [gif("b"), gif("c")]);
        store.apply(
            Event::Failed {
                failure: Failure::GifFavorites,
                reason: "no".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(store.gif_favorites().unwrap(), [gif("a"), gif("b")]);
    }

    // Server administration, rest (P7).

    #[test]
    fn a_query_s_pages_list_in_their_order_and_another_query_starts_over() {
        let mut store = store();
        let now = Instant::now();
        let first = store.members(LAB)[0].user.0;
        let oldest = rune_model::MemberQuery {
            order: rune_model::MemberOrder::OldestAccounts,
            recent: false,
        };
        let page = |store: &Store, after: Option<&str>, users: &[u64], query| {
            let Event::MembersLoaded {
                request,
                server,
                after,
                members,
                next,
                total,
                ..
            } = member_page(store, after, users, Some("n"))
            else {
                unreachable!()
            };
            Event::MembersLoaded {
                request,
                server,
                after,
                members,
                next,
                total,
                query,
                joins: vec![(UserId(70_002), rune_model::JoinMethod::VanityLink)],
            }
        };
        store.apply(page(&store, None, &[70_002, first], oldest), now);
        let pages = store.member_pages(LAB).unwrap();
        assert_eq!(pages.query, oldest);
        assert_eq!(pages.listed, [UserId(70_002), UserId(first)]);
        assert_eq!(
            pages.joins.get(&UserId(70_002)),
            Some(&rune_model::JoinMethod::VanityLink)
        );
        // A page of another query does not continue this one.
        store.apply(
            page(
                &store,
                Some("n"),
                &[70_009],
                rune_model::MemberQuery::default(),
            ),
            now,
        );
        assert_eq!(store.member_pages(LAB).unwrap().listed.len(), 2);
        // Asked again from the start, the new query replaces the list.
        store.apply(
            page(&store, None, &[first], rune_model::MemberQuery::default()),
            now,
        );
        let pages = store.member_pages(LAB).unwrap();
        assert_eq!(pages.listed, [UserId(first)]);
        assert_eq!(pages.query, rune_model::MemberQuery::default());
    }

    #[test]
    fn role_counts_say_none_for_a_role_left_out_and_unknown_before_any() {
        let mut store = store();
        let now = Instant::now();
        let role = rune_model::RoleId(77);
        assert_eq!(store.role_count(LAB, role), None);
        let applied = store.apply(
            Event::RoleCounts {
                server: LAB,
                counts: vec![(role, 4)],
            },
            now,
        );
        assert_eq!(applied, Applied::RoleCounts(LAB));
        assert_eq!(store.role_count(LAB, role), Some(4));
        assert_eq!(store.role_count(LAB, rune_model::RoleId(78)), Some(0));
    }
}
