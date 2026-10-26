//! Account-local reading choices and independently versioned provider cursors.

use rune_i18n::t;
use rune_model::{ChannelId, MessageId, ReadMark, ServerId, UserId};
use rune_provider::{AuthenticationState, Command, ConnectionState, Failure, RequestId};

use crate::{Applied, Store};

// UI requests start at 1<<48; Core starts at one. Keep this namespace across
// Ready and authentication boundaries so a late cancellation cannot collide.
const READ_REQUESTS: u64 = 1 << 63;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingRead {
    request: RequestId,
    target: MessageId,
    choice: ReadMark,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadState {
    /// Monotonic account-local progress, independent of a manual reminder.
    pub last_read: Option<MessageId>,
    pub last_message: Option<MessageId>,
    pub mentions: u32,
    pub confirmed_read: Option<MessageId>,
    pub confirmed_version: u64,
    pub unread_from: Option<MessageId>,
    /// A real provider-supplied first unread anchor, when it is known.
    pub first_unread: Option<MessageId>,
    pub(super) pending: Option<PendingRead>,
    pub(super) local_revision: u64,
    pub(super) saved_mark: ReadMark,
    pub(super) failed: bool,
}

impl ReadState {
    pub fn unread(&self) -> bool {
        if self.unread_from.is_some() {
            return true;
        }
        match (self.last_message, self.last_read) {
            (Some(last), Some(read)) => last > read,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    pub fn sync_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn sync_failed(&self) -> bool {
        self.failed
    }

    fn changed(&mut self) {
        // This clock never wraps or mixes with provider versions. At saturation
        // the complete choice still distinguishes pending writes and ACKs.
        self.local_revision = self.local_revision.saturating_add(1);
    }

    fn mark(&self) -> ReadMark {
        ReadMark {
            through: self.last_read,
            unread_from: self.unread_from,
            revision: self.local_revision,
        }
    }

    fn clear_remote(&mut self) {
        *self = Self {
            last_read: self.last_read,
            unread_from: self.unread_from,
            confirmed_read: self.confirmed_read,
            confirmed_version: self.confirmed_version,
            local_revision: self.local_revision,
            saved_mark: self.saved_mark,
            ..Default::default()
        };
    }
}

impl Store {
    fn readable(&self, channel: ChannelId) -> bool {
        self.me.is_some()
            && self
                .abilities(channel)
                .is_some_and(|a| a.read_history.allowed())
    }

    fn can_sync_read(&self, channel: ChannelId) -> bool {
        self.readable(channel)
            && self.read_sync()
            && !self.from_cache
            && self.connection == ConnectionState::Connected
            && self.provider.as_ref().is_some_and(|provider| {
                !provider.capabilities.authentication
                    || matches!(self.authentication, AuthenticationState::SignedIn { .. })
            })
    }

    /// Mark the newest known real message locally; retry remote confirmation
    /// only on an explicit action when there is no newer local progress.
    pub fn mark_read_command(&mut self, channel: ChannelId) -> Option<Command> {
        self.mark_read(channel);
        self.retry_read_command(channel)
    }

    /// Retry only progress already recorded locally, without reading newer
    /// arrivals or clearing a reminder made after the original request.
    pub fn retry_read_command(&mut self, channel: ChannelId) -> Option<Command> {
        if !self.can_sync_read(channel) {
            return None;
        }
        let read = self.reads.get(&channel)?;
        let target = read.last_read.filter(|message| message.0 != 0)?;
        if read.confirmed_read >= Some(target)
            || read.pending.is_some_and(|pending| pending.target == target)
        {
            return None;
        }
        let request = if self.read_requests < READ_REQUESTS {
            READ_REQUESTS
        } else {
            self.read_requests.checked_add(1)?
        };
        self.read_requests = request;
        let read = self.reads.get_mut(&channel)?;
        read.pending = Some(PendingRead {
            request,
            target,
            choice: read.mark(),
        });
        read.failed = false;
        self.revision += 1;
        Some(Command::MarkRead {
            request,
            account: self.me?,
            channel,
            message: target,
        })
    }

    /// Automatic reading only sends when genuinely advancing, never retries a
    /// failed confirmation on every layout or clears a manual reminder.
    pub fn mark_read_visible_command(&mut self, channel: ChannelId) -> Option<Command> {
        let read = self.read(channel);
        if read.unread_from.is_some() || read.last_message <= read.last_read {
            return None;
        }
        // One confirmation in flight per channel, as Serein paces them: the
        // progress made meanwhile goes once it answers (`read_follow_up`).
        if read.sync_pending() {
            self.mark_read(channel);
            return None;
        }
        self.mark_read_command(channel)
    }

    /// After a confirmation, the automatic progress it did not cover yet;
    /// never over a reminder or a failed attempt, which wait for the person.
    pub fn read_follow_up(&mut self, channel: ChannelId) -> Option<Command> {
        let read = self.reads.get(&channel)?;
        if read.unread_from.is_some()
            || read.pending.is_some()
            || read.failed
            || read.last_read <= read.confirmed_read
        {
            return None;
        }
        self.retry_read_command(channel)
    }

    /// A reminder names an existing exact anchor and never rewinds the local
    /// reading cursor. It stays in RUNE unless the provider takes reminders
    /// too ([`rune_provider::Capabilities::remote_unread`]): then it goes
    /// there, with the account's mentions it leaves unread among those loaded.
    pub fn mark_unread_command(
        &mut self,
        channel: ChannelId,
        message: MessageId,
    ) -> Option<Command> {
        self.mark_unread(channel, message);
        if !self.remote_unread() || !self.can_sync_read(channel) || message.0 == 0 {
            return None;
        }
        let me = self.me?;
        let mentions = self.timeline(channel).map_or(0, |timeline| {
            timeline
                .messages()
                .iter()
                .filter(|found| {
                    found.id >= message
                        && !found.delivery.is_local()
                        && self.counts_as_mention(found, me)
                })
                .count() as u32
        });
        Some(Command::MarkUnread {
            account: me,
            channel,
            message,
            mentions,
        })
    }

    /// Whether a reminder made here reaches the provider.
    pub fn remote_unread(&self) -> bool {
        self.read_sync()
            && self
                .provider
                .as_ref()
                .is_some_and(|provider| provider.capabilities.remote_unread)
    }

    /// Mark every channel of `server` read here; at the provider with one
    /// command when it takes a whole server, else channel by channel.
    pub fn mark_server_read_commands(&mut self, server: ServerId) -> Vec<Command> {
        let mut channels: Vec<ChannelId> = self
            .channels
            .values()
            .filter(|channel| channel.server == Some(server))
            .map(|channel| channel.id)
            .collect();
        channels.sort();
        let whole = self
            .provider
            .as_ref()
            .is_some_and(|provider| provider.capabilities.server_read)
            && channels.iter().any(|channel| self.can_sync_read(*channel));
        if !whole {
            return channels
                .into_iter()
                .filter_map(|channel| self.mark_read_command(channel))
                .collect();
        }
        let marked = channels
            .into_iter()
            .filter(|channel| self.mark_read(*channel).is_some())
            .count();
        match self.me {
            Some(account) if marked > 0 => vec![Command::MarkServerRead { account, server }],
            _ => Vec::new(),
        }
    }

    /// The provider says a conversation is held unread by hand, here or on
    /// another device. Its cursor goes back; the reminder falls on the exact
    /// first message after it: the one named, else the first loaded, else
    /// the newest known. Nothing after the cursor leaves nothing unread.
    pub(super) fn marked_unread(
        &mut self,
        account: UserId,
        channel: ChannelId,
        through: Option<MessageId>,
        anchor: Option<MessageId>,
        mentions: u32,
        version: u64,
    ) -> Applied {
        if self.me != Some(account)
            || !self.remote_unread()
            || !self.readable(channel)
            || version == 0
            || through.is_some_and(|message| message.0 == 0)
        {
            return Applied::Nothing;
        }
        let loaded = self.timeline(channel).and_then(|timeline| {
            timeline
                .messages()
                .iter()
                .find(|message| !message.delivery.is_local() && Some(message.id) > through)
                .map(|message| message.id)
        });
        let read = self.reads.entry(channel).or_default();
        if version <= read.confirmed_version {
            return Applied::Nothing;
        }
        read.confirmed_read = through;
        read.confirmed_version = version;
        let first = anchor
            .filter(|anchor| Some(*anchor) > through)
            .or(loaded)
            .or(read.last_message.filter(|last| Some(*last) > through));
        if let Some(first) = first {
            if read.unread_from != Some(first) {
                read.unread_from = Some(first);
                read.changed();
            }
            read.mentions = mentions;
        }
        self.revision += 1;
        Applied::ReadState(channel)
    }

    /// The newest message of channels the provider only summarizes: what is
    /// unread follows, without the messages.
    pub(super) fn latest_messages(&mut self, latest: Vec<(ChannelId, MessageId)>) -> Applied {
        let mut changed = false;
        for (channel, message) in latest {
            if message.0 == 0 || !self.channels.contains_key(&channel) {
                continue;
            }
            let read = self.reads.entry(channel).or_default();
            if Some(message) > read.last_message {
                read.last_message = Some(message);
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
            Applied::Activity
        } else {
            Applied::Nothing
        }
    }

    pub fn mark_read(&mut self, channel: ChannelId) -> Option<MessageId> {
        if !self.readable(channel) {
            return None;
        }
        let read = self.reads.get_mut(&channel)?;
        if !read.unread() && read.mentions == 0 {
            return None;
        }
        read.last_read = read.last_read.max(read.last_message);
        if read.first_unread <= read.last_read {
            read.first_unread = None;
        }
        read.unread_from = None;
        read.mentions = 0;
        read.changed();
        self.revision += 1;
        read.last_read
    }

    pub fn mark_unread(&mut self, channel: ChannelId, message: MessageId) -> MessageId {
        if message.0 == 0 || !self.readable(channel) {
            return message;
        }
        let read = self.reads.entry(channel).or_default();
        if read.unread_from != Some(message) {
            read.unread_from = Some(message);
            read.changed();
            self.revision += 1;
        }
        message
    }

    /// Unsaved local choices only; provider versions are stored independently.
    pub fn dirty_read_marks(&self) -> Vec<(ChannelId, ReadMark)> {
        if self.me.is_none() {
            return Vec::new();
        }
        let mut marks: Vec<_> = self
            .reads
            .iter()
            .filter(|(_, read)| read.local_revision != 0 && read.mark() != read.saved_mark)
            .map(|(channel, read)| (*channel, read.mark()))
            .collect();
        marks.sort_by_key(|(channel, _)| *channel);
        marks
    }

    pub fn read_mark_saved(&mut self, channel: ChannelId, mark: ReadMark) {
        if let Some(read) = self
            .reads
            .get_mut(&channel)
            .filter(|read| read.mark() == mark)
        {
            read.saved_mark = mark;
        }
    }

    pub fn discard_dirty_read_marks(&mut self) {
        for read in self.reads.values_mut() {
            read.saved_mark = read.mark();
        }
    }

    pub fn restore_read_marks(&mut self, account: UserId, marks: Vec<(ChannelId, ReadMark)>) {
        if self.me != Some(account) {
            return;
        }
        for (channel, mark) in marks {
            if channel.0 == 0
                || self
                    .channels
                    .get(&channel)
                    .is_some_and(|channel| !channel.kind.has_messages())
                || mark.through.is_some_and(|message| message.0 == 0)
                || mark.unread_from.is_some_and(|message| message.0 == 0)
            {
                continue;
            }
            let read = self.reads.entry(channel).or_default();
            if read.local_revision != 0 {
                continue;
            }
            read.last_read = read.last_read.max(mark.through);
            if read.first_unread <= read.last_read {
                read.first_unread = None;
            }
            read.unread_from = mark.unread_from;
            read.local_revision = mark.revision.max(1);
            read.saved_mark = read.mark();
            if read.last_read >= read.last_message {
                read.mentions = 0;
            }
            self.revision += 1;
        }
    }

    pub(super) fn confirmed_read(
        &mut self,
        account: UserId,
        request: Option<RequestId>,
        channel: ChannelId,
        message: MessageId,
        version: u64,
    ) -> Applied {
        if self.me != Some(account)
            || !self.read_sync()
            || !self.readable(channel)
            || message.0 == 0
        {
            return Applied::Nothing;
        }
        let read = self.reads.entry(channel).or_default();
        if request.is_some_and(|request| {
            !read
                .pending
                .is_some_and(|pending| pending.request == request && pending.target <= message)
        }) {
            return Applied::Nothing;
        }
        let consistent = version != 0
            && Some(message) >= read.confirmed_read
            && (version > read.confirmed_version
                || version == read.confirmed_version && Some(message) == read.confirmed_read);
        if !consistent {
            if let Some(request) = request {
                read.pending = None;
                read.failed = true;
                return Applied::Failed {
                    failure: Failure::ReadState { request, channel },
                    reason: t("La confirmación de lectura llegó con una versión incompatible. Puedes reintentar.").into(),
                    ambiguous: true,
                };
            }
            return Applied::Nothing;
        }
        if request.is_none() && version <= read.confirmed_version {
            return Applied::Nothing;
        }
        read.confirmed_read = Some(message);
        read.confirmed_version = version;
        let same_choice = read
            .pending
            .is_none_or(|pending| pending.choice == read.mark());
        if read.last_read < Some(message) {
            read.last_read = Some(message);
            read.changed();
        }
        if read.first_unread <= Some(message) {
            read.first_unread = None;
        }
        if read.last_message <= Some(message) && same_choice {
            read.mentions = 0;
        }
        if read
            .pending
            .is_some_and(|pending| pending.target <= message)
        {
            // Local revision may have changed to a reminder while in flight.
            // Confirmation never changes unread_from or the saved choice.
            read.pending = None;
            read.failed = false;
        }
        Applied::ReadState(channel)
    }

    pub(super) fn read_failed(&mut self, channel: ChannelId, request: RequestId) -> bool {
        let Some(read) = self.reads.get_mut(&channel).filter(|read| {
            read.pending
                .is_some_and(|pending| pending.request == request)
        }) else {
            return false;
        };
        read.pending = None;
        read.failed = true;
        true
    }

    pub(super) fn ready_reads(
        &mut self,
        same_account: bool,
        states: Vec<(ChannelId, Option<MessageId>, u32)>,
        versions: Vec<(ChannelId, u64)>,
        last_messages: Vec<(ChannelId, MessageId)>,
        first_unread: Vec<(ChannelId, MessageId)>,
    ) {
        if !same_account {
            self.reads.clear();
        }
        self.reads.retain(|channel, read| {
            self.channels.contains_key(channel)
                || read.local_revision != 0
                || read.confirmed_version != 0
        });
        for (channel, read) in &mut self.reads {
            if !self.channels.contains_key(channel) {
                read.clear_remote();
                continue;
            }
            read.first_unread = None;
            if read.pending.take().is_some() {
                read.failed = true;
            }
        }
        for (channel, last) in last_messages {
            if last.0 != 0 && self.channels.contains_key(&channel) {
                let read = self.reads.entry(channel).or_default();
                read.last_message = read.last_message.max(Some(last));
            }
        }
        let versions: std::collections::HashMap<_, _> = versions.into_iter().collect();
        for (channel, last_read, mentions) in states {
            if !self.channels.contains_key(&channel) || last_read.is_some_and(|id| id.0 == 0) {
                continue;
            }
            let read = self.reads.entry(channel).or_default();
            let version = versions.get(&channel).copied().unwrap_or(0);
            if version < read.confirmed_version
                || last_read < read.confirmed_read
                || version != 0
                    && version == read.confirmed_version
                    && last_read != read.confirmed_read
            {
                continue;
            }
            read.confirmed_read = last_read;
            read.confirmed_version = version;
            read.last_read = read.last_read.max(last_read);
            if read.local_revision == 0 {
                read.mentions = mentions;
            } else if read.last_read >= read.last_message {
                read.mentions = 0;
            } else if last_read >= read.last_read {
                read.mentions = mentions;
            }
            // When the provider cursor is behind local progress, its mention
            // count includes already-read messages. Keep only locally known
            // mentions rather than importing an incompatible aggregate.
        }
        for read in self.reads.values_mut() {
            if read.first_unread <= read.last_read {
                read.first_unread = None;
            }
        }
        for (channel, anchor) in first_unread {
            if anchor.0 == 0 || !self.channels.contains_key(&channel) {
                continue;
            }
            let read = self.reads.entry(channel).or_default();
            if versions.get(&channel).copied().unwrap_or(0) >= read.confirmed_version
                && Some(anchor) > read.last_read
                && Some(anchor) <= read.last_message
                && !self
                    .timelines
                    .get(&channel)
                    .is_some_and(|timeline| timeline.is_deleted(anchor))
            {
                read.first_unread = Some(anchor);
            }
        }
    }

    pub(super) fn forget_read_channel(&mut self, channel: ChannelId) {
        if let Some(read) = self
            .reads
            .get_mut(&channel)
            .filter(|read| read.local_revision != 0 || read.confirmed_version != 0)
        {
            read.clear_remote();
        } else {
            self.reads.remove(&channel);
        }
    }
}
