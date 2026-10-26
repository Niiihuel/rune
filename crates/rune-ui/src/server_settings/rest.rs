//! The rest of the administration (P7): the server's picture and traits on
//! its profile, its safety and engagement settings, pausing invitations,
//! reordering its channels at once (by dragging, or from each row's menu),
//! a role's picture, emoji and gradient, how many have each role, and the
//! member pages' order and filter. The settings' rules hold: drafts keep
//! only what differs from the store, the bar of unsaved changes saves them,
//! and every permit is asked of `Store::server_abilities` again on save.
//! Since 2026-10-19 also the server's banner and moving icon (as its boosts
//! unlock them), its own emojis in the traits (`:name:`), and becoming a
//! community with its rules and updates channels, on the safety page.

use std::time::Instant;

use limn::RasterImage;
use rune_components::server_settings::parse_hex;
use rune_i18n::t;
use rune_model::{
    ChannelId, ChannelMove, CustomEmoji, MediaScan, MemberOrder, MemberQuery, Rgb, Role, RoleId,
    Server, ServerOptions, ServerTrait, Verification, AFK_TIMEOUTS, MAX_TRAITS,
};
use rune_provider::{
    Command, ExpressionImage, ExpressionKind, Failure, ProfileImage, ProfileImageKind, RoleEdit,
    RoleIcon, ServerEdit,
};
use rune_state::{Applied, Store};

use super::{refused, set_input, Page, ServerSettings};
use crate::{Intent, Ui};

/// The field where a role's emoji is written.
pub(crate) const ROLE_EMOJI: &str = "ss:role-emoji";
/// What a gradient starts with when it is turned on.
const GRADIENT: Rgb = Rgb(0x58_65_F2);

/// What a picture is being chosen for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Picking {
    ServerIcon,
    RoleIcon(RoleId),
    // Icon, banner and community (2026-10-19).
    ServerBanner,
}

/// A channel or a category where the page that reorders them puts it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Slot {
    pub channel: ChannelId,
    /// Its category; always `None` for a category.
    pub parent: Option<ChannelId>,
    pub category: bool,
}

/// What these pages keep while the settings are open.
#[derive(Debug, Default)]
pub struct Rest {
    /// A picture being chosen with this request, and for what.
    pub picking: Option<(u64, Picking)>,
    /// The server's picture chosen and not saved yet.
    pub icon_preview: Option<RasterImage>,
    /// A role's picture chosen and not saved yet.
    pub role_icon_preview: Option<(RoleId, RasterImage)>,
    /// The read of the profile's traits under way, and whether it failed.
    pub profile_request: Option<u64>,
    pub profile_failed: bool,
    /// The channels as moved here, in the page's order, not saved yet.
    pub layout: Option<Vec<Slot>>,
    /// The moves sent, until every channel is where they say.
    pub(super) saving_layout: Option<Vec<ChannelMove>>,
    /// A picture sent, the server's or a role's, that no answer confirmed
    /// yet: a picture cannot be compared with the one the store has.
    pub(super) pictures_sent: bool,
    pub dragging: Option<ChannelId>,
    pub drop_over: Option<ChannelId>,
    pub channel_menu: Option<ChannelId>,
    /// The members page's order and filter.
    pub query: MemberQuery,
    /// The dropdown open: `system`, `afk`, `afk-timeout`, `verification`,
    /// `media-scan`, `notify` or `member-order`.
    pub select: Option<&'static str>,
    /// The custom color field of a gradient's second (2) or third (3) color.
    pub gradient_hex: Option<(u8, String)>,
    // Icon, banner and community (2026-10-19).
    /// The server's banner chosen and not saved yet.
    pub banner_preview: Option<RasterImage>,
}

impl Rest {
    /// Close the innermost of these; false when none was open.
    pub(super) fn close(&mut self) -> bool {
        self.select.take().is_some()
            || self.channel_menu.take().is_some()
            || self.gradient_hex.take().is_some()
    }

    /// Whether a reorder is being saved.
    pub(super) fn saving(&self) -> bool {
        self.saving_layout.is_some()
    }
}

/// The P7 part of `edit` without what `server` already is; the rest empty.
pub(super) fn changes(edit: &ServerEdit, server: &Server) -> ServerEdit {
    let options = &server.options;
    let traits = edit
        .traits
        .as_ref()
        .map(|traits| tidy(traits))
        .filter(|traits| options.traits.as_ref() != Some(traits));
    ServerEdit {
        icon: edit
            .icon
            .clone()
            .filter(|icon| icon.is_some() || server.icon.is_some()),
        traits,
        system_channel: edit
            .system_channel
            .filter(|channel| *channel != options.system_channel),
        system_messages: edit
            .system_messages
            .filter(|messages| *messages != options.system_messages),
        mentions_only: edit
            .mentions_only
            .filter(|mentions| *mentions != options.mentions_only),
        afk_channel: edit
            .afk_channel
            .filter(|channel| *channel != options.afk_channel),
        afk_timeout: edit.afk_timeout.filter(|secs| *secs != options.afk_timeout),
        verification: edit
            .verification
            .filter(|level| *level != options.verification),
        media_scan: edit.media_scan.filter(|scan| *scan != options.media_scan),
        invites_paused: edit
            .invites_paused
            .filter(|paused| *paused != options.invites_paused),
        // Icon, banner and community (2026-10-19).
        banner: edit
            .banner
            .clone()
            .filter(|banner| banner.is_some() || options.banner.is_some()),
        community: edit
            .community
            .filter(|community| *community != options.community),
        rules_channel: edit
            .rules_channel
            .filter(|channel| *channel != options.rules_channel),
        updates_channel: edit
            .updates_channel
            .filter(|channel| *channel != options.updates_channel),
        ..ServerEdit::default()
    }
}

// Icon, banner and community (2026-10-19).
/// Whether `edit` asks for something beyond the identity and safety
/// permits that `abilities` deny: a banner, a moving picture, the community
/// and its channels.
pub(super) fn denied_looks(edit: &ServerEdit, abilities: &rune_state::ServerAbilities) -> bool {
    let moving = |image: &Option<Option<ProfileImage>>| matches!(image, Some(Some(image)) if image.animated());
    (moving(&edit.icon) && !abilities.animated_icon.allowed())
        || (edit.banner.is_some() && !abilities.edit_banner.allowed())
        || (moving(&edit.banner) && !abilities.animated_banner.allowed())
        || (edit.community.is_some() && !abilities.toggle_community.allowed())
        || ((edit.rules_channel.is_some() || edit.updates_channel.is_some())
            && !abilities.edit_community.allowed())
}

/// Why the draft cannot be saved as a community, when it touches what one
/// needs and lacks some of it.
pub(super) fn community_problem(settings: &ServerSettings, store: &Store) -> Option<String> {
    let server = store.server(settings.server)?;
    let edit = changes(&settings.profile, server);
    let touched = edit.community.is_some()
        || edit.rules_channel.is_some()
        || edit.updates_channel.is_some()
        || edit.verification.is_some()
        || edit.media_scan.is_some();
    let options = shown_options(server, &settings.profile);
    (touched && options.community && !options.community_ready()).then(|| {
        if options.rules_channel.is_none() || options.updates_channel.is_none() {
            t("Elige el canal de reglas y el de novedades de la comunidad.")
        } else {
            t("Una comunidad necesita al menos verificación baja y revisar el contenido de todos.")
        }
        .to_owned()
    })
}

/// The server's own emoji `text` names as `:name:` (or bare), if any.
pub(crate) fn own_emoji<'a>(server: Option<&'a Server>, text: &str) -> Option<&'a CustomEmoji> {
    let name = text.trim().trim_matches(':');
    server?
        .emojis
        .iter()
        .find(|emoji| !name.is_empty() && emoji.name == name)
}

/// What a trait's emoji field shows: the emoji, or `:name:` for one of
/// the server's own.
pub(crate) fn trait_emoji(found: &ServerTrait) -> String {
    match (&found.emoji, found.emoji_id) {
        (Some(name), Some(_)) => format!(":{name}:"),
        (emoji, _) => emoji.clone().unwrap_or_default(),
    }
}

/// Traits as they would be saved: trimmed, without the empty ones.
fn tidy(traits: &[ServerTrait]) -> Vec<ServerTrait> {
    traits
        .iter()
        .map(|found| ServerTrait {
            label: found.label.trim().to_owned(),
            emoji: found
                .emoji
                .as_deref()
                .map(str::trim)
                .filter(|emoji| !emoji.is_empty())
                .map(str::to_owned),
            ..found.clone()
        })
        .filter(|found| !found.label.is_empty())
        .collect()
}

/// `server`'s options with what is changed here.
pub(crate) fn shown_options(server: &Server, edit: &ServerEdit) -> ServerOptions {
    let options = server.options.clone();
    ServerOptions {
        traits: edit.traits.clone().or(options.traits),
        system_channel: edit.system_channel.unwrap_or(options.system_channel),
        system_messages: edit.system_messages.unwrap_or(options.system_messages),
        mentions_only: edit.mentions_only.unwrap_or(options.mentions_only),
        afk_channel: edit.afk_channel.unwrap_or(options.afk_channel),
        afk_timeout: edit.afk_timeout.unwrap_or(options.afk_timeout),
        verification: edit.verification.unwrap_or(options.verification),
        media_scan: edit.media_scan.unwrap_or(options.media_scan),
        // Icon, banner and community (2026-10-19).
        community: edit.community.unwrap_or(options.community),
        rules_channel: edit.rules_channel.unwrap_or(options.rules_channel),
        updates_channel: edit.updates_channel.unwrap_or(options.updates_channel),
        ..options
    }
}

/// Whether the profile part of `edit` changes the picture or the traits.
pub(super) fn touches_identity(edit: &ServerEdit) -> bool {
    edit.icon.is_some() || edit.traits.is_some() || edit.banner.is_some()
}

/// Whether `edit` changes the safety settings.
pub(super) fn touches_safety(edit: &ServerEdit) -> bool {
    edit.system_channel.is_some()
        || edit.system_messages.is_some()
        || edit.mentions_only.is_some()
        || edit.afk_channel.is_some()
        || edit.afk_timeout.is_some()
        || edit.verification.is_some()
        || edit.media_scan.is_some()
}

/// The role's picture, emoji and gradient in `edit`, without what `role`
/// already shows.
pub(super) fn role_changes(
    edit: &RoleEdit,
    role: &Role,
) -> (Option<Option<RoleIcon>>, Option<Vec<Rgb>>) {
    let style = &role.style;
    let icon = edit.icon.clone().filter(|icon| match icon {
        Some(RoleIcon::Image(_)) => true,
        Some(RoleIcon::Emoji(emoji)) => style.emoji.as_ref() != Some(emoji) || style.icon.is_some(),
        None => style.has_mark(),
    });
    let gradient = edit
        .gradient
        .clone()
        .filter(|gradient| *gradient != style.gradient);
    (icon, gradient)
}

/// `role` as the editor shows it with its picture, emoji and gradient.
pub(super) fn shown_style(role: &mut Role, edit: &RoleEdit) {
    if let Some(icon) = &edit.icon {
        (role.style.icon, role.style.emoji) = match icon {
            Some(RoleIcon::Emoji(emoji)) => (None, Some(emoji.clone())),
            // The picture shows from the preview kept here.
            Some(RoleIcon::Image(_)) | None => (None, None),
        };
    }
    if let Some(gradient) = &edit.gradient {
        role.style.gradient = gradient.clone();
    }
}

/// The channels of `server` as the page lists them: those without a
/// category first, then each category and its channels.
pub(crate) fn layout(store: &Store, server: rune_model::ServerId) -> Vec<Slot> {
    let mut slots = Vec::new();
    for (category, channels) in store.channel_tree(server) {
        if let Some(category) = category {
            slots.push(Slot {
                channel: category.id,
                parent: None,
                category: true,
            });
        }
        for channel in channels {
            slots.push(Slot {
                channel: channel.id,
                parent: category.map(|category| category.id),
                category: false,
            });
        }
    }
    slots
}

/// The layout as shown: as moved here, while it holds the same channels.
pub(crate) fn shown_layout(settings: &ServerSettings, store: &Store) -> Vec<Slot> {
    let current = layout(store, settings.server);
    settings
        .rest
        .layout
        .clone()
        .filter(|draft| same_channels(draft, &current))
        .unwrap_or(current)
}

fn same_channels(a: &[Slot], b: &[Slot]) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|slot| b.iter().any(|other| other.channel == slot.channel))
}

/// Whether the channels shown are not where the store has them.
pub(super) fn layout_changed(settings: &ServerSettings, store: &Store) -> bool {
    settings.rest.layout.is_some()
        && shown_layout(settings, store) != layout(store, settings.server)
}

/// The moves that put the channels where the page shows them: each one
/// whose category or place among its siblings differs.
pub(super) fn moves(settings: &ServerSettings, store: &Store) -> Vec<ChannelMove> {
    layout_moves(store, settings.server, &shown_layout(settings, store))
}

/// The moves that put `server`'s channels as `shown` lists the ones the
/// account sees: categories among themselves, and each category's channels
/// (those without one first). Only the groups whose order changed are
/// numbered again: one left as it was keeps its positions, gaps and all, so
/// nothing is written where the account did not move anything. The
/// channels it cannot see count too, and never move: each keeps its place
/// among its siblings, and the ones it sees are numbered around it, so it
/// shifts nothing where they land. Empty when there is no room to number
/// them.
pub(crate) fn layout_moves(
    store: &Store,
    server: rune_model::ServerId,
    shown: &[Slot],
) -> Vec<ChannelMove> {
    let current = layout(store, server);
    let all = store.channel_tree_with_hidden(server);
    let categories: Vec<&rune_model::Channel> =
        all.iter().filter_map(|(category, _)| *category).collect();
    // A channel shown without its category, hidden from the account, keeps
    // it unless it was moved here.
    let parent_of = |slot: &Slot| {
        let was = current.iter().find(|was| was.channel == slot.channel);
        match store.channel(slot.channel) {
            Some(found) if was.is_some_and(|was| was.parent == slot.parent) => found
                .parent
                .filter(|parent| categories.iter().any(|category| category.id == *parent)),
            _ => slot.parent,
        }
    };
    let group = |slots: &[Slot], category: bool, parent: Option<ChannelId>| -> Vec<ChannelId> {
        slots
            .iter()
            .filter(|slot| slot.category == category && (category || parent_of(slot) == parent))
            .map(|slot| slot.channel)
            .collect()
    };
    // As it was, but for those that left it: no channel came in, and no
    // two of those that stay swapped.
    let untouched = |wanted: &[ChannelId], category: bool, parent: Option<ChannelId>| {
        group(&current, category, parent)
            .into_iter()
            .filter(|channel| wanted.contains(channel))
            .eq(wanted.iter().copied())
    };
    let mut moves = Vec::new();
    let wanted = group(shown, true, None);
    if !untouched(&wanted, true, None)
        && place(store, &categories, &wanted, None, &mut moves).is_none()
    {
        return Vec::new();
    }
    let parents = std::iter::once(None).chain(categories.iter().map(|category| Some(category.id)));
    for parent in parents {
        let wanted = group(shown, false, parent);
        if untouched(&wanted, false, parent) {
            continue;
        }
        let siblings = all
            .iter()
            .find(|(category, _)| category.map(|category| category.id) == parent)
            .map(|(_, channels)| channels.clone())
            .unwrap_or_default();
        if place(store, &siblings, &wanted, parent, &mut moves).is_none() {
            return Vec::new();
        }
    }
    moves
}

/// Number `wanted`, the order the account wants for the siblings it sees
/// under `parent`, around `siblings`, all of them there now in order: each
/// one it sees gives its place to the next of `wanted`, and each one it does
/// not see keeps its place and its position. Positions only grow, so the
/// ones it sees land in order whatever the rest; one it does not see keeps
/// its place unless there is no room before it. `None` when a position would
/// pass `i32::MAX`, after one it does not see sits at the very end.
fn place(
    store: &Store,
    siblings: &[&rune_model::Channel],
    wanted: &[ChannelId],
    parent: Option<ChannelId>,
    moves: &mut Vec<ChannelMove>,
) -> Option<()> {
    let mut next = wanted.iter().copied();
    // `Err` is one the account sees, `Ok` the position of one it does not.
    let mut order: Vec<Result<i32, ChannelId>> = Vec::new();
    for sibling in siblings {
        if !Store::visible(sibling) {
            order.push(Ok(sibling.position));
        } else if let Some(channel) = next.next() {
            order.push(Err(channel));
        }
    }
    order.extend(next.map(Err));
    let mut floor = -1i32;
    for slot in order {
        match slot {
            Ok(fixed) => floor = floor.max(fixed),
            Err(channel) => {
                floor = floor.checked_add(1)?;
                if store
                    .channel(channel)
                    .is_some_and(|found| found.parent != parent || found.position != floor)
                {
                    moves.push(ChannelMove {
                        channel,
                        parent,
                        position: floor,
                        lock_permissions: false,
                    });
                }
            }
        }
    }
    Some(())
}

/// Where a channel goes when it is moved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Before(ChannelId),
    After(ChannelId),
    /// The first channel of a category, or the first without one.
    Into(Option<ChannelId>),
}

/// Put `slots` in the page's order: the channels without a category, then
/// each category followed by its channels, each group in the order given.
pub(crate) fn normalize(slots: Vec<Slot>) -> Vec<Slot> {
    let mut out: Vec<Slot> = slots
        .iter()
        .filter(|slot| !slot.category && slot.parent.is_none())
        .copied()
        .collect();
    for category in slots.iter().filter(|slot| slot.category) {
        out.push(*category);
        out.extend(
            slots
                .iter()
                .filter(|slot| !slot.category && slot.parent == Some(category.channel)),
        );
    }
    // A channel whose category is not listed stays with those without one.
    for slot in &slots {
        if !out.iter().any(|kept| kept.channel == slot.channel) {
            out.insert(
                0,
                Slot {
                    parent: None,
                    ..*slot
                },
            );
        }
    }
    out
}

/// Move `channel` to `target` in the shown layout, when the account may.
fn move_channel(settings: &mut ServerSettings, store: &Store, channel: ChannelId, target: Target) {
    if !store
        .server_abilities(settings.server)
        .is_some_and(|abilities| abilities.reorder_channels.allowed())
    {
        return;
    }
    let mut slots = shown_layout(settings, store);
    let Some(from) = slots.iter().position(|slot| slot.channel == channel) else {
        return;
    };
    let moving = slots.remove(from);
    let at = |slots: &[Slot], id: ChannelId| slots.iter().position(|slot| slot.channel == id);
    if moving.category {
        // A category goes before or after another, with its channels.
        let category = |id: ChannelId| {
            slots.iter().find(|slot| slot.channel == id).map(|slot| {
                if slot.category {
                    slot.channel
                } else {
                    slot.parent.unwrap_or(id)
                }
            })
        };
        let index = match target {
            Target::Before(id) => category(id).and_then(|id| at(&slots, id)),
            Target::After(id) => category(id)
                .and_then(|id| at(&slots, id))
                .map(|index| index + 1),
            Target::Into(_) => None,
        };
        let Some(index) = index else {
            return;
        };
        slots.insert(index.min(slots.len()), moving);
    } else {
        let (index, parent) = match target {
            Target::Before(id) | Target::After(id) => {
                let Some(index) = at(&slots, id) else {
                    return;
                };
                let found = slots[index];
                if found.category {
                    // Onto a category: its first channel.
                    (index + 1, Some(found.channel))
                } else if matches!(target, Target::After(_)) {
                    (index + 1, found.parent)
                } else {
                    (index, found.parent)
                }
            }
            Target::Into(Some(category)) => match at(&slots, category) {
                Some(index) => (index + 1, Some(category)),
                None => return,
            },
            Target::Into(None) => (0, None),
        };
        slots.insert(index.min(slots.len()), Slot { parent, ..moving });
    }
    let slots = normalize(slots);
    settings.rest.layout = (slots != layout(store, settings.server)).then_some(slots);
}

/// A drag on the channels page: a row picked up, carried over another,
/// dropped before or after it (onto a category: its first channel), or let go.
pub(super) fn dragged(settings: &mut ServerSettings, store: &Store, action: &limn::UiAction) {
    let channel = |id: &str| {
        id.strip_prefix("ss:chan:")
            .and_then(|id| id.parse().ok())
            .map(ChannelId)
    };
    match action {
        limn::UiAction::DragStarted { id, .. } => {
            settings.rest.dragging = channel(id).filter(|_| {
                store
                    .server_abilities(settings.server)
                    .is_some_and(|abilities| abilities.reorder_channels.allowed())
            });
        }
        limn::UiAction::DragMoved { over, .. } => {
            settings.rest.drop_over = over.as_deref().and_then(channel);
        }
        limn::UiAction::Dropped { target, .. } => {
            if let (Some(dragged), Some(target)) =
                (settings.rest.dragging, target.as_deref().and_then(channel))
            {
                let slots = shown_layout(settings, store);
                let at = |id: ChannelId| slots.iter().position(|slot| slot.channel == id);
                if let (Some(from), Some(to)) = (at(dragged), at(target)) {
                    if from != to {
                        let target = if from > to {
                            Target::Before(target)
                        } else {
                            Target::After(target)
                        };
                        move_channel(settings, store, dragged, target);
                    }
                }
            }
            settings.rest.dragging = None;
            settings.rest.drop_over = None;
        }
        _ => {
            settings.rest.dragging = None;
            settings.rest.drop_over = None;
        }
    }
}

/// One step up or down the page, into the category before or after when
/// it leaves its own.
fn step(settings: &mut ServerSettings, store: &Store, channel: ChannelId, up: bool) {
    let slots = shown_layout(settings, store);
    let Some(index) = slots.iter().position(|slot| slot.channel == channel) else {
        return;
    };
    let slot = slots[index];
    if slot.category {
        let categories: Vec<ChannelId> = slots
            .iter()
            .filter(|other| other.category)
            .map(|other| other.channel)
            .collect();
        let at = categories.iter().position(|id| *id == channel).unwrap_or(0);
        let target = if up {
            at.checked_sub(1)
                .map(|before| Target::Before(categories[before]))
        } else {
            categories.get(at + 1).map(|after| Target::After(*after))
        };
        if let Some(target) = target {
            move_channel(settings, store, channel, target);
        }
        return;
    }
    let neighbor = if up {
        index.checked_sub(1).map(|before| slots[before])
    } else {
        slots.get(index + 1).copied()
    };
    let target = match neighbor {
        // Past the top: nothing to do.
        None => return,
        // Over its own category: to the end of the one before, or out.
        Some(other) if other.category && up => {
            let before = slots[..index.saturating_sub(1)]
                .iter()
                .rev()
                .find(|other| other.category)
                .map(|other| other.channel);
            let last = slots.iter().rev().find(|other| {
                !other.category && other.parent == before && other.channel != channel
            });
            match last {
                Some(last) => Target::After(last.channel),
                None => Target::Into(before),
            }
        }
        // Down onto the next category: its first channel.
        Some(other) if other.category => Target::Into(Some(other.channel)),
        Some(other) if up => Target::Before(other.channel),
        Some(other) => Target::After(other.channel),
    };
    move_channel(settings, store, channel, target);
}

/// What a page asks for when it opens: the profile's traits, the count of
/// each role, the member pages in their order.
pub(super) fn load(
    settings: &mut ServerSettings,
    page: Page,
    ui: &mut Ui,
    store: &Store,
) -> Vec<Command> {
    let server = settings.server;
    let Some(abilities) = store.server_abilities(server) else {
        return Vec::new();
    };
    let capabilities = store
        .provider
        .as_ref()
        .map(|info| info.capabilities)
        .unwrap_or_default();
    match page {
        // Once known, the traits stay current with the server's changes.
        Page::Profile
            if capabilities.server_identity
                && store
                    .server(server)
                    .is_some_and(|found| found.options.traits.is_none()) =>
        {
            let request = ui.fresh_request();
            settings.rest.profile_request = Some(request);
            settings.rest.profile_failed = false;
            vec![Command::LoadServerProfile { request, server }]
        }
        Page::Roles if abilities.role_counts.allowed() => vec![Command::LoadRoleCounts {
            request: ui.fresh_request(),
            server,
        }],
        _ => Vec::new(),
    }
}

/// What a field's new text changes; false when it is not one of these.
pub(super) fn typed(settings: &mut ServerSettings, store: &Store, id: &str, value: &str) -> bool {
    if let Some(rest) = id.strip_prefix("ss:trait:") {
        let Some((index, part)) = rest
            .split_once(':')
            .and_then(|(index, part)| Some((index.parse::<usize>().ok()?, part)))
        else {
            return true;
        };
        let mut traits = shown_traits(settings, store);
        if let Some(found) = traits.get_mut(index) {
            match part {
                "label" => found.label = value.to_owned(),
                "emoji" => {
                    // One of the server's own by `:name:` (2026-10-19).
                    let own = own_emoji(store.server(settings.server), value);
                    found.emoji_id = own.map(|emoji| emoji.id);
                    found.emoji_animated = own.is_some_and(|emoji| emoji.animated);
                    found.emoji = match own {
                        Some(emoji) => Some(emoji.name.clone()),
                        None => Some(value.trim().to_owned()).filter(|emoji| !emoji.is_empty()),
                    };
                }
                _ => return true,
            }
            settings.profile.traits = Some(traits);
        }
        return true;
    }
    match id {
        ROLE_EMOJI => {
            let emoji = value.trim();
            settings.role_edit.icon =
                Some((!emoji.is_empty()).then(|| RoleIcon::Emoji(emoji.to_owned())));
            settings.rest.role_icon_preview = None;
            true
        }
        "ss:gradient2:hex" | "ss:gradient3:hex" => {
            let which = if id.contains('2') { 2 } else { 3 };
            if let Some(color) = parse_hex(value) {
                set_stop(settings, store, which, Some(color));
            }
            settings.rest.gradient_hex = Some((which, value.to_owned()));
            true
        }
        _ => false,
    }
}

/// The traits as the profile page shows them.
pub(crate) fn shown_traits(settings: &ServerSettings, store: &Store) -> Vec<ServerTrait> {
    settings.profile.traits.clone().unwrap_or_else(|| {
        store
            .server(settings.server)
            .and_then(|server| server.options.traits.clone())
            .unwrap_or_default()
    })
}

/// The open role's gradient as shown.
fn shown_gradient(settings: &ServerSettings, store: &Store) -> Vec<Rgb> {
    settings.role_edit.gradient.clone().unwrap_or_else(|| {
        settings
            .role
            .and_then(|role| {
                store
                    .server(settings.server)?
                    .roles
                    .iter()
                    .find(|r| r.id == role)
                    .cloned()
            })
            .map(|role| role.style.gradient)
            .unwrap_or_default()
    })
}

/// Set the gradient's second (2) or third (3) color; `None` takes the
/// third away.
fn set_stop(settings: &mut ServerSettings, store: &Store, which: u8, color: Option<Rgb>) {
    let mut gradient = shown_gradient(settings, store);
    match (which, color) {
        (2, Some(color)) if gradient.is_empty() => gradient.push(color),
        (2, Some(color)) => gradient[0] = color,
        (3, Some(color)) if gradient.len() == 1 => gradient.push(color),
        (3, Some(color)) if gradient.len() == 2 => gradient[1] = color,
        (3, None) => gradient.truncate(1),
        _ => return,
    }
    settings.role_edit.gradient = Some(gradient);
}

/// A click on one of these; false when it is not one of them.
pub(super) fn clicked(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    parts: &[&str],
    now: Instant,
    intents: &mut Vec<Intent>,
) -> bool {
    let server = settings.server;
    let Some(abilities) = store.server_abilities(server) else {
        return false;
    };
    let channel = |part: &str| part.parse().ok().map(ChannelId);
    match parts {
        ["pick", key] => {
            let key: &'static str = match *key {
                "system" => "system",
                "afk" => "afk",
                "afk-timeout" => "afk-timeout",
                "verification" => "verification",
                "media-scan" => "media-scan",
                "notify" => "notify",
                "member-order" => "member-order",
                // Icon, banner and community (2026-10-19).
                "rules" => "rules",
                "updates" => "updates",
                _ => return true,
            };
            settings.rest.select = if settings.rest.select == Some(key) {
                None
            } else {
                Some(key)
            };
        }
        ["pick-dismiss"] => settings.rest.select = None,
        ["set", "member-order", index] => {
            settings.rest.select = None;
            if let Some(order) = index
                .parse::<usize>()
                .ok()
                .and_then(|i| MemberOrder::ALL.get(i))
            {
                let query = MemberQuery {
                    order: *order,
                    ..settings.rest.query
                };
                requery(settings, ui, store, intents, query);
            }
        }
        ["members", "recent"] => {
            let query = MemberQuery {
                recent: !settings.rest.query.recent,
                ..settings.rest.query
            };
            requery(settings, ui, store, intents, query);
        }
        // Icon, banner and community (2026-10-19).
        ["set", key @ ("rules" | "updates"), value] => {
            settings.rest.select = None;
            if abilities.edit_community.allowed() {
                let chosen = Some(channel(value));
                match *key {
                    "rules" => settings.profile.rules_channel = chosen,
                    _ => settings.profile.updates_channel = chosen,
                }
            }
        }
        ["community"] if abilities.toggle_community.allowed() => {
            let Some(found) = store.server(server) else {
                return true;
            };
            let on = !shown_options(found, &settings.profile).community;
            let edit = &mut settings.profile;
            edit.community = Some(on);
            // What a community asks for, raised to its least.
            if on {
                if shown_options(found, edit).verification == Verification::None {
                    edit.verification = Some(Verification::Low);
                }
                edit.media_scan = Some(MediaScan::Everyone);
            }
        }
        ["banner", "pick"]
            if abilities.edit_banner.allowed() && settings.rest.picking.is_none() =>
        {
            let request = ui.fresh_request();
            settings.rest.picking = Some((request, Picking::ServerBanner));
            intents.push(Intent::PickProfileImage {
                request,
                kind: ProfileImageKind::ServerBanner,
                animated: abilities.animated_banner.allowed(),
            });
        }
        ["banner", "remove"] if abilities.edit_banner.allowed() => {
            settings.profile.banner = Some(None);
            settings.rest.banner_preview = None;
        }
        ["set", key, value] if abilities.edit_safety.allowed() => {
            settings.rest.select = None;
            let edit = &mut settings.profile;
            match *key {
                "system" => edit.system_channel = Some(channel(value)),
                "afk" => edit.afk_channel = Some(channel(value)),
                "afk-timeout" => {
                    if let Some(secs) = value
                        .parse()
                        .ok()
                        .filter(|secs| AFK_TIMEOUTS.contains(secs))
                    {
                        edit.afk_timeout = Some(secs);
                    }
                }
                "verification" => {
                    edit.verification = value
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| Verification::ALL.get(index).copied())
                        .or(edit.verification);
                }
                "media-scan" => {
                    edit.media_scan = value
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| MediaScan::ALL.get(index).copied())
                        .or(edit.media_scan);
                }
                "notify" => edit.mentions_only = Some(*value == "mentions"),
                _ => {}
            }
        }
        ["system-messages", which] if abilities.edit_safety.allowed() => {
            let Some(found) = store.server(server) else {
                return true;
            };
            let mut messages = shown_options(found, &settings.profile).system_messages;
            match *which {
                "joins" => messages.joins = !messages.joins,
                "join-replies" => messages.join_replies = !messages.join_replies,
                "boosts" => messages.boosts = !messages.boosts,
                "tips" => messages.tips = !messages.tips,
                _ => return true,
            }
            settings.profile.system_messages = Some(messages);
        }

        // The profile's picture and traits.
        ["icon", "pick"]
            if abilities.edit_identity.allowed() && settings.rest.picking.is_none() =>
        {
            let request = ui.fresh_request();
            settings.rest.picking = Some((request, Picking::ServerIcon));
            intents.push(Intent::PickProfileImage {
                request,
                kind: ProfileImageKind::Avatar,
                // A moving icon, where the server's boosts unlock it (2026-10-19).
                animated: abilities.animated_icon.allowed(),
            });
        }
        ["icon", "remove"] if abilities.edit_identity.allowed() => {
            settings.profile.icon = Some(None);
            settings.rest.icon_preview = None;
        }
        ["trait", "add"] if abilities.edit_identity.allowed() => {
            let mut traits = shown_traits(settings, store);
            if traits.len() < MAX_TRAITS {
                traits.push(ServerTrait {
                    label: String::new(),
                    emoji: None,
                    emoji_id: None,
                    emoji_animated: false,
                });
                let index = traits.len() - 1;
                settings.profile.traits = Some(traits);
                intents.push(set_input(&format!("ss:trait:{index}:label"), ""));
                intents.push(set_input(&format!("ss:trait:{index}:emoji"), ""));
                intents.push(Intent::Focus(format!("ss:trait:{index}:label")));
            }
        }
        ["trait", index, "remove"] if abilities.edit_identity.allowed() => {
            let mut traits = shown_traits(settings, store);
            if let Some(index) = index.parse::<usize>().ok().filter(|i| *i < traits.len()) {
                traits.remove(index);
                trait_inputs(&traits, intents);
                settings.profile.traits = Some(traits);
            }
        }
        ["profile", "retry"] => {
            for command in load(settings, Page::Profile, ui, store) {
                intents.push(Intent::Command(command));
            }
        }

        // Pausing invitations, at once.
        ["invites", which @ ("pause" | "resume")] => {
            let paused = *which == "pause";
            match abilities.pause_invites.reason() {
                None => intents.push(Intent::Command(Command::EditServer {
                    server,
                    edit: ServerEdit {
                        invites_paused: Some(paused),
                        ..ServerEdit::default()
                    },
                })),
                reason => refused(
                    ui,
                    if paused {
                        t("No se pausaron las invitaciones")
                    } else {
                        t("No se reanudaron las invitaciones")
                    },
                    reason.map(str::to_owned),
                    now,
                ),
            }
        }

        // The channels page.
        ["chan", id, "menu"] => {
            let id = channel(id);
            settings.rest.channel_menu = if settings.rest.channel_menu == id {
                None
            } else {
                id
            };
        }
        ["chan-menu", "dismiss"] => settings.rest.channel_menu = None,
        ["chan", id, step_to @ ("up" | "down")] => {
            settings.rest.channel_menu = None;
            if let Some(id) = channel(id) {
                step(settings, store, id, *step_to == "up");
            }
        }
        ["chan", id, "to", category] => {
            settings.rest.channel_menu = None;
            if let Some(id) = channel(id) {
                move_channel(settings, store, id, Target::Into(channel(category)));
            }
        }
        // A click on a row itself does nothing: it is there to be dragged.
        ["chan", _] => {}

        // A role's picture, emoji and gradient.
        ["role-icon", "pick"] if settings.rest.picking.is_none() => {
            if let Some(role) = settings.role.filter(|_| abilities.role_icons.allowed()) {
                let request = ui.fresh_request();
                settings.rest.picking = Some((request, Picking::RoleIcon(role)));
                intents.push(Intent::PickExpressionImage {
                    request,
                    kind: ExpressionKind::Emoji,
                });
            }
        }
        ["role-icon", "remove"] if abilities.role_icons.allowed() && settings.role.is_some() => {
            settings.role_edit.icon = Some(None);
            settings.rest.role_icon_preview = None;
            intents.push(set_input(ROLE_EMOJI, ""));
        }
        ["gradient"] if abilities.role_gradients.allowed() && settings.role.is_some() => {
            let on = shown_gradient(settings, store).is_empty();
            settings.role_edit.gradient = Some(if on { vec![GRADIENT] } else { Vec::new() });
            settings.rest.gradient_hex = None;
        }
        [stop @ ("gradient2" | "gradient3"), what] if abilities.role_gradients.allowed() => {
            let which = if *stop == "gradient2" { 2 } else { 3 };
            match *what {
                "custom" => {
                    settings.rest.gradient_hex = match settings.rest.gradient_hex {
                        Some(_) => None,
                        None => {
                            let gradient = shown_gradient(settings, store);
                            let shown = gradient
                                .get(which as usize - 2)
                                .copied()
                                .unwrap_or(GRADIENT);
                            let hex = rune_components::server_settings::hex(shown);
                            intents.push(set_input(&format!("ss:{stop}:hex"), &hex));
                            Some((which, hex))
                        }
                    }
                }
                "dismiss" => settings.rest.gradient_hex = None,
                "default" if which == 3 => set_stop(settings, store, 3, None),
                color => {
                    if let Some(color) = parse_hex(color) {
                        set_stop(settings, store, which, Some(color));
                    }
                }
            }
        }
        _ => return false,
    }
    true
}

/// Ask the members again, from their first page, in `query`'s order.
fn requery(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    intents: &mut Vec<Intent>,
    query: MemberQuery,
) {
    let allowed = store
        .server_abilities(settings.server)
        .is_some_and(|abilities| abilities.member_queries.allowed());
    if !allowed || query == settings.rest.query {
        return;
    }
    settings.rest.query = query;
    intents.push(Intent::Command(Command::LoadMembers {
        request: ui.fresh_request(),
        server: settings.server,
        after: None,
        query,
    }));
}

/// The fields of `traits`, as limn keeps their text.
fn trait_inputs(traits: &[ServerTrait], intents: &mut Vec<Intent>) {
    for (index, found) in traits.iter().enumerate() {
        intents.push(set_input(&format!("ss:trait:{index}:label"), &found.label));
        intents.push(set_input(
            &format!("ss:trait:{index}:emoji"),
            &trait_emoji(found),
        ));
    }
}

/// Show the store's values in the fields of these pages nobody edited.
pub(super) fn refresh(settings: &mut ServerSettings, store: &Store, intents: &mut Vec<Intent>) {
    let Some(server) = store.server(settings.server) else {
        return;
    };
    if settings.profile.traits.is_none() {
        trait_inputs(
            server.options.traits.as_deref().unwrap_or_default(),
            intents,
        );
    }
    if settings.role_edit.icon.is_none() {
        let emoji = settings
            .role
            .and_then(|role| server.roles.iter().find(|found| found.id == role))
            .and_then(|role| role.style.emoji.clone())
            .unwrap_or_default();
        intents.push(set_input(ROLE_EMOJI, &emoji));
        settings.rest.role_icon_preview = None;
    }
    if settings.profile.icon.is_none() {
        settings.rest.icon_preview = None;
    }
    if settings.profile.banner.is_none() {
        settings.rest.banner_preview = None;
    }
    let current = layout(store, settings.server);
    if settings
        .rest
        .layout
        .as_ref()
        .is_some_and(|draft| *draft == current || !same_channels(draft, &current))
    {
        settings.rest.layout = None;
    }
}

/// Send the channels' new places, when they changed and the account may.
/// False when it may not.
pub(super) fn save_layout(
    settings: &mut ServerSettings,
    store: &Store,
    intents: &mut Vec<Intent>,
) -> bool {
    if !layout_changed(settings, store) {
        return true;
    }
    if !store
        .server_abilities(settings.server)
        .is_some_and(|abilities| abilities.reorder_channels.allowed())
    {
        return false;
    }
    let moves = moves(settings, store);
    if moves.is_empty() {
        settings.rest.layout = None;
        return true;
    }
    settings.rest.saving_layout = Some(moves.clone());
    intents.push(Intent::Command(Command::ReorderChannels {
        server: settings.server,
        moves,
    }));
    true
}

/// What an event changes in these pages.
pub(super) fn on_applied(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    applied: &Applied,
    now: Instant,
) {
    let server = settings.server;
    // The answer to a save with pictures: they are the store's now.
    if matches!(applied, Applied::Server(of) if *of == server) && settings.rest.pictures_sent {
        settings.rest.pictures_sent = false;
        settings.profile.icon = None;
        settings.rest.icon_preview = None;
        settings.profile.banner = None;
        settings.rest.banner_preview = None;
        if matches!(settings.role_edit.icon, Some(Some(RoleIcon::Image(_)))) {
            settings.role_edit.icon = None;
            settings.rest.role_icon_preview = None;
        }
    }
    match applied {
        Applied::Ready => {
            settings.rest.saving_layout = None;
            settings.rest.profile_request = None;
            settings.rest.pictures_sent = false;
        }
        Applied::Server(of)
            if *of == server
                && store
                    .server(server)
                    .is_some_and(|found| found.options.traits.is_some()) =>
        {
            settings.rest.profile_request = None;
        }
        Applied::Channel(_) | Applied::ChannelRemoved(_) => {
            let current = layout(store, server);
            // Done once the store has every channel where the moves say (or
            // the channel is gone): the store is the truth from then on.
            let landed = |step: &ChannelMove| {
                store.channel(step.channel).is_none_or(|found| {
                    found.parent == step.parent && found.position == step.position
                })
            };
            if settings
                .rest
                .saving_layout
                .as_ref()
                .is_some_and(|sent| sent.iter().all(landed))
            {
                settings.rest.saving_layout = None;
                settings.rest.layout = None;
            }
            if settings
                .rest
                .layout
                .as_ref()
                .is_some_and(|draft| !same_channels(draft, &current))
            {
                settings.rest.layout = None;
            }
        }
        Applied::Failed { failure, .. } => match failure {
            Failure::ServerLoad { request, .. }
                if settings.rest.profile_request == Some(*request) =>
            {
                settings.rest.profile_request = None;
                settings.rest.profile_failed = true;
            }
            Failure::ChannelOrder { server: of } if *of == server => {
                settings.rest.saving_layout = None;
            }
            // Refused: the pictures stay drafted, to send again.
            Failure::Server { server: Some(of) } | Failure::Role { server: of }
                if *of == server =>
            {
                settings.rest.pictures_sent = false;
            }
            _ => {}
        },
        _ => {}
    }
    let _ = (ui, now);
}

/// What came of choosing the server's picture asked for with `request`;
/// `None` when it was not this.
pub fn server_icon_picked(
    ui: &mut Ui,
    request: u64,
    result: &Result<Option<(ProfileImage, RasterImage)>, String>,
    now: Instant,
) -> Option<Vec<Intent>> {
    let settings = ui.server_settings.as_mut()?;
    let (_, picking @ (Picking::ServerIcon | Picking::ServerBanner)) = settings
        .rest
        .picking
        .filter(|(waiting, _)| *waiting == request)?
    else {
        return None;
    };
    settings.rest.picking = None;
    match result {
        // The banner, its way (2026-10-19).
        Ok(Some((image, preview))) if picking == Picking::ServerBanner => {
            settings.profile.banner = Some(Some(image.clone()));
            settings.rest.banner_preview = Some(preview.clone());
        }
        Ok(Some((image, preview))) => {
            settings.profile.icon = Some(Some(image.clone()));
            settings.rest.icon_preview = Some(preview.clone());
        }
        Ok(None) => {}
        Err(reason) => ui.notify(
            t("No se pudo usar la imagen"),
            Some(reason.clone()),
            true,
            now,
        ),
    }
    Some(Vec::new())
}

/// What came of choosing a role's picture asked for with `request`;
/// `None` when it was not this. A moving picture is refused: a role's
/// icon is still.
pub(crate) fn role_icon_picked(
    ui: &mut Ui,
    request: u64,
    result: &Result<Option<(ExpressionImage, RasterImage)>, String>,
    now: Instant,
) -> Option<Vec<Intent>> {
    let settings = ui.server_settings.as_mut()?;
    let (_, Picking::RoleIcon(role)) = settings
        .rest
        .picking
        .filter(|(waiting, _)| *waiting == request)?
    else {
        return None;
    };
    settings.rest.picking = None;
    match result {
        Ok(Some((image, preview))) => {
            let icon = RoleIcon::Image(image.clone());
            match icon.problem() {
                Some(problem) => {
                    ui.notify(
                        t("No se pudo usar la imagen"),
                        Some(problem.into()),
                        true,
                        now,
                    );
                }
                None if settings.role == Some(role) => {
                    settings.role_edit.icon = Some(Some(icon));
                    settings.rest.role_icon_preview = Some((role, preview.clone()));
                    return Some(vec![set_input(ROLE_EMOJI, "")]);
                }
                None => {}
            }
        }
        Ok(None) => {}
        Err(reason) => ui.notify(
            t("No se pudo usar la imagen"),
            Some(reason.clone()),
            true,
            now,
        ),
    }
    Some(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(channel: u64, parent: Option<u64>, category: bool) -> Slot {
        Slot {
            channel: ChannelId(channel),
            parent: parent.map(ChannelId),
            category,
        }
    }

    #[test]
    fn a_layout_keeps_loose_channels_first_and_each_category_with_its_own() {
        let mixed = vec![
            slot(10, None, true),
            slot(1, Some(10), false),
            slot(2, None, false),
            slot(20, None, true),
            slot(3, Some(20), false),
            slot(4, Some(99), false),
        ];
        let ids: Vec<u64> = normalize(mixed).iter().map(|slot| slot.channel.0).collect();
        assert_eq!(ids, [4, 2, 10, 1, 20, 3]);
    }
}
