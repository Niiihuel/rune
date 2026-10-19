//! The account settings Discord keeps in `settings-proto/1`: a protobuf
//! (`PreloadedUserSettings`) sent as base64. The schema comes from the
//! official web client's public bundle (REA, `web.c5b338cc7e610833.js`:
//! `discord_protos.discord_users.v1.*`) and is the one Serein reads
//! (`discord-protocol/src/guild_folders.rs`, `activity_sharing.rs`,
//! `messaging_permissions.rs`), reimplemented here. Only what RUNE shows is
//! decoded: the status and custom status, the folders of servers and the
//! messaging privacy. A subtree RUNE writes keeps every other field byte for
//! byte, and a write replaces only that subtree, as Serein's do.
//!
//! A minimal decoder of its own instead of a protobuf crate: five subtrees
//! of known, fixed field numbers, bounded and checked here; a generated
//! schema would bring a build step and a dependency for that. The same
//! reader and writer serve `settings-proto/2`, the GIF favorites
//! ([`crate::admin::favorites`]).

use std::collections::{HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD, Engine};
use rune_model::{
    account::{FOLDERS_MAX, FOLDER_NAME_MAX, PRIVACY_SERVERS_MAX},
    ContentFilter, FriendSources, MessagingPrivacy, PrivacyChange, Rgb, ServerFolder, ServerId,
    SpamFilter, Status,
};
use serde_json::Value;

/// The largest answer read: Discord takes a 5 MiB base64 value, plus its JSON.
pub(crate) const MAX_RESPONSE: usize = 6 * 1024 * 1024;
/// The largest subtree decoded or written.
const MAX_SUBTREE: usize = 128 * 1024;
/// The longest custom status Discord takes.
pub(crate) const CUSTOM_MAX: usize = 128;

// Fields of `PreloadedUserSettings`.
const VERSIONS: u64 = 1;
const TEXT: u64 = 6;
const PRIVACY: u64 = 8;
const STATUS: u64 = 11;
const FOLDERS: u64 = 14;

/// Anything that is not the protobuf expected.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Malformed;

type Decoded<T> = Result<T, Malformed>;

/// The account settings as read: their data version and the subtrees RUNE
/// knows, raw.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Settings {
    /// Sent back as `required_data_version`, so a write lands only on what
    /// was read.
    pub version: u64,
    text: Vec<u8>,
    privacy: Vec<u8>,
    status: Vec<u8>,
    folders: Vec<u8>,
}

/// The account's own status and custom status, as kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Own {
    pub status: Status,
    pub custom: Option<Custom>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Custom {
    pub text: String,
    /// A Unicode emoji.
    pub emoji: Option<String>,
    /// A server's emoji in its place, by its id and name: drawn as its
    /// picture and kept when the custom status is written again.
    pub server_emoji: Option<(u64, String)>,
    /// Unix milliseconds; `None` keeps it.
    pub expires: Option<i64>,
}

impl Own {
    /// The line beside the status, as RUNE shows it: the emoji, a space and
    /// the text.
    pub fn line(&self) -> Option<String> {
        let custom = self.custom.as_ref()?;
        match (&custom.emoji, custom.text.is_empty()) {
            (Some(emoji), true) => Some(emoji.clone()),
            (Some(emoji), false) => Some(format!("{emoji} {}", custom.text)),
            (None, true) => None,
            (None, false) => Some(custom.text.clone()),
        }
    }

    /// The server's emoji of the custom status, as a picture. The settings
    /// do not say whether it moves; its still picture shows.
    pub fn picture(&self) -> Option<rune_model::TextPicture> {
        let (id, name) = self.custom.as_ref()?.server_emoji.clone()?;
        Some(rune_model::TextPicture {
            token: format!("<:{name}:{id}>"),
            image: crate::emoji::inline(id, false),
            name,
        })
    }
}

/// Its name on the wire; offline is not a status one sets, invisible is.
pub(crate) fn status_name(status: Status) -> &'static str {
    match status {
        Status::Online => "online",
        Status::Idle => "idle",
        Status::DoNotDisturb => "dnd",
        Status::Invisible | Status::Offline => "invisible",
    }
}

pub(crate) fn status_of(name: &str) -> Option<Status> {
    Some(match name {
        "online" => Status::Online,
        "idle" => Status::Idle,
        "dnd" => Status::DoNotDisturb,
        "invisible" => Status::Invisible,
        _ => return None,
    })
}

/// One field of a message: its number, wire type, value and whole bytes.
pub(crate) struct Field<'a> {
    pub number: u64,
    kind: u8,
    value: &'a [u8],
    /// The whole field as it came, to write it back unchanged.
    pub raw: &'a [u8],
}

impl<'a> Field<'a> {
    pub fn bytes(&self) -> Decoded<&'a [u8]> {
        (self.kind == 2).then_some(self.value).ok_or(Malformed)
    }

    pub fn varint(&self) -> Decoded<u64> {
        if self.kind != 0 {
            return Err(Malformed);
        }
        varint(&mut { self.value })
    }

    fn fixed64(&self) -> Decoded<u64> {
        let bytes: [u8; 8] = (self.kind == 1)
            .then_some(self.value)
            .and_then(|value| value.try_into().ok())
            .ok_or(Malformed)?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// The value of a wrapper message (`google.protobuf.*Value`): its field 1.
    fn wrapped(&self) -> Decoded<Option<Field<'a>>> {
        Ok(fields(self.bytes()?)?
            .into_iter()
            .rfind(|field| field.number == 1))
    }

    fn wrapped_varint(&self) -> Decoded<u64> {
        self.wrapped()?.map_or(Ok(0), |field| field.varint())
    }

    fn wrapped_text(&self) -> Decoded<String> {
        match self.wrapped()? {
            Some(field) => String::from_utf8(field.bytes()?.to_vec()).map_err(|_| Malformed),
            None => Ok(String::new()),
        }
    }

    /// A repeated fixed64, packed or not.
    fn ids(&self) -> Decoded<Vec<u64>> {
        let bytes = match self.kind {
            1 => self.value,
            _ => self.bytes()?,
        };
        if bytes.len() % 8 != 0 {
            return Err(Malformed);
        }
        Ok(bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().expect("eight bytes")))
            .collect())
    }
}

fn varint(input: &mut &[u8]) -> Decoded<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = input.split_first().ok_or(Malformed)?;
        *input = rest;
        if shift == 63 && byte > 1 {
            return Err(Malformed);
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Ok(value);
        }
    }
    Err(Malformed)
}

/// The fields of one message, bounded in number; groups and anything
/// malformed are refused.
pub(crate) fn fields(mut bytes: &[u8]) -> Decoded<Vec<Field<'_>>> {
    let mut found = Vec::new();
    while !bytes.is_empty() {
        if found.len() >= 8192 {
            return Err(Malformed);
        }
        let start = bytes;
        let tag = varint(&mut bytes)?;
        let (number, kind) = (tag >> 3, (tag & 7) as u8);
        if number == 0 || number > 0x1fff_ffff {
            return Err(Malformed);
        }
        let length = match kind {
            0 => {
                let before = bytes;
                varint(&mut bytes)?;
                let value = &before[..before.len() - bytes.len()];
                found.push(Field {
                    number,
                    kind,
                    value,
                    raw: &start[..start.len() - bytes.len()],
                });
                continue;
            }
            1 => 8,
            5 => 4,
            2 => usize::try_from(varint(&mut bytes)?).map_err(|_| Malformed)?,
            _ => return Err(Malformed),
        };
        let value = bytes.get(..length).ok_or(Malformed)?;
        bytes = &bytes[length..];
        found.push(Field {
            number,
            kind,
            value,
            raw: &start[..start.len() - bytes.len()],
        });
    }
    Ok(found)
}

pub(crate) fn put_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

pub(crate) fn put_bytes(number: u64, value: &[u8], out: &mut Vec<u8>) {
    put_varint(number << 3 | 2, out);
    put_varint(value.len() as u64, out);
    out.extend_from_slice(value);
}

pub(crate) fn put_number(number: u64, value: u64, out: &mut Vec<u8>) {
    put_varint(number << 3, out);
    put_varint(value, out);
}

fn put_fixed64(number: u64, value: u64, out: &mut Vec<u8>) {
    put_varint(number << 3 | 1, out);
    out.extend_from_slice(&value.to_le_bytes());
}

/// A wrapper message holding `value` as its field 1.
fn put_wrapped_number(number: u64, value: u64, out: &mut Vec<u8>) {
    let mut wrapper = Vec::new();
    put_number(1, value, &mut wrapper);
    put_bytes(number, &wrapper, out);
}

fn put_wrapped_text(number: u64, text: &str, out: &mut Vec<u8>) {
    let mut wrapper = Vec::new();
    put_bytes(1, text.as_bytes(), &mut wrapper);
    put_bytes(number, &wrapper, out);
}

fn put_ids(number: u64, ids: impl Iterator<Item = u64>, out: &mut Vec<u8>) {
    let packed: Vec<u8> = ids.flat_map(u64::to_le_bytes).collect();
    put_bytes(number, &packed, out);
}

/// `subtree` without the fields `numbers`.
fn without(subtree: &[u8], numbers: &[u64]) -> Decoded<Vec<u8>> {
    let mut kept = Vec::with_capacity(subtree.len());
    for field in fields(subtree)? {
        if !numbers.contains(&field.number) {
            kept.extend_from_slice(field.raw);
        }
    }
    Ok(kept)
}

/// The settings in an answer of `GET` or `PATCH /users/@me/settings-proto/1`.
pub(crate) fn decode(answer: &Value) -> Decoded<Settings> {
    if answer["out_of_date"].as_bool() == Some(true) {
        return Err(Malformed);
    }
    let encoded = answer["settings"].as_str().ok_or(Malformed)?;
    let wire = STANDARD.decode(encoded).map_err(|_| Malformed)?;
    let mut settings = Settings::default();
    let mut versioned = false;
    for field in fields(&wire)? {
        let slot = match field.number {
            VERSIONS => {
                versioned = true;
                for inner in fields(field.bytes()?)? {
                    if inner.number == 3 {
                        settings.version = inner.varint()?;
                    }
                }
                continue;
            }
            TEXT => &mut settings.text,
            PRIVACY => &mut settings.privacy,
            STATUS => &mut settings.status,
            FOLDERS => &mut settings.folders,
            _ => continue,
        };
        let value = field.bytes()?;
        if value.len() > MAX_SUBTREE || !slot.is_empty() {
            return Err(Malformed);
        }
        // Each must parse, so a write never builds on a broken one.
        fields(value)?;
        *slot = value.to_vec();
    }
    if !versioned || settings.version > u64::from(u32::MAX) {
        return Err(Malformed);
    }
    Ok(settings)
}

/// The body of a `PATCH` writing `number`'s subtree as `subtree`.
pub(crate) fn patch(settings: &Settings, number: u64, subtree: Vec<u8>) -> Decoded<Value> {
    if subtree.len() > MAX_SUBTREE {
        return Err(Malformed);
    }
    let mut wire = Vec::new();
    put_bytes(number, &subtree, &mut wire);
    Ok(serde_json::json!({
        "settings": STANDARD.encode(wire),
        "required_data_version": settings.version,
    }))
}

// The status and custom status: `StatusSettings` (field 11).

fn custom_status(bytes: &[u8], now: i64) -> Decoded<Option<Custom>> {
    let mut custom = Custom::default();
    let mut custom_emoji = 0;
    for field in fields(bytes)? {
        match field.number {
            1 => {
                custom.text = String::from_utf8(field.bytes()?.to_vec()).map_err(|_| Malformed)?;
            }
            2 => custom_emoji = field.fixed64()?,
            3 => {
                let name = String::from_utf8(field.bytes()?.to_vec()).map_err(|_| Malformed)?;
                custom.emoji = (!name.is_empty()).then_some(name);
            }
            4 => {
                let at = i64::try_from(field.fixed64()?).map_err(|_| Malformed)?;
                custom.expires = (at != 0).then_some(at);
            }
            _ => {}
        }
    }
    // A server's emoji has an id; its name is not a Unicode emoji.
    if custom_emoji != 0 {
        custom.server_emoji = custom
            .emoji
            .take()
            .filter(|name| (2..=32).contains(&name.len()))
            .filter(|name| name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            .map(|name| (custom_emoji, name));
    }
    let expired = custom.expires.is_some_and(|at| at <= now);
    let empty = custom.text.is_empty() && custom.emoji.is_none() && custom.server_emoji.is_none();
    Ok((!expired && !empty).then_some(custom))
}

impl Settings {
    /// The account's status and custom status at `now` (Unix ms): an
    /// expired custom status is none.
    pub fn own(&self, now: i64) -> Decoded<Own> {
        let mut own = Own {
            status: Status::Online,
            custom: None,
        };
        for field in fields(&self.status)? {
            match field.number {
                1 => {
                    let name = field.wrapped_text()?;
                    own.status = if name.is_empty() {
                        Status::Online
                    } else {
                        status_of(&name).ok_or(Malformed)?
                    };
                }
                2 => own.custom = custom_status(field.bytes()?, now)?,
                _ => {}
            }
        }
        Ok(own)
    }

    /// The status subtree with `own` in it; game sharing and every unknown
    /// field stay, and so does the rest of the custom status if it is the same.
    pub fn with_own(&self, own: &Own) -> Decoded<Vec<u8>> {
        let mut previous_custom: Option<&[u8]> = None;
        let mut previous_status = String::new();
        let mut status_expiry: Option<Vec<u8>> = None;
        let mut subtree = Vec::new();
        for field in fields(&self.status)? {
            match field.number {
                1 => previous_status = field.wrapped_text()?,
                2 => previous_custom = Some(field.bytes()?),
                // How long the status lasts belongs to the status it was set with.
                4 => status_expiry = Some(field.raw.to_vec()),
                _ => subtree.extend_from_slice(field.raw),
            }
        }
        let status = status_name(own.status);
        put_wrapped_text(1, status, &mut subtree);
        if previous_status == status {
            subtree.extend(status_expiry.unwrap_or_default());
        }
        if let Some(custom) = &own.custom {
            let mut message = match previous_custom {
                Some(bytes) => without(bytes, &[1, 2, 3, 4])?,
                None => Vec::new(),
            };
            if !custom.text.is_empty() {
                put_bytes(1, custom.text.as_bytes(), &mut message);
            }
            if let Some((id, name)) = &custom.server_emoji {
                put_fixed64(2, *id, &mut message);
                put_bytes(3, name.as_bytes(), &mut message);
            } else if let Some(emoji) = &custom.emoji {
                put_bytes(3, emoji.as_bytes(), &mut message);
            }
            if let Some(expires) = custom.expires {
                put_fixed64(4, expires as u64, &mut message);
            }
            put_bytes(2, &message, &mut subtree);
        }
        Ok(subtree)
    }

    // The folders of servers: `GuildFolders` (field 14).

    /// The layout as kept, normalized as the official client shows it: a
    /// server named twice stays where it was first, a group without an id
    /// becomes servers on their own, empty entries go, and a folder id of
    /// zero or repeated gets a fresh one.
    pub fn folders(&self) -> Decoded<Vec<ServerFolder>> {
        Ok(self.folder_entries()?.0)
    }

    /// The layout, and the fields of each entry RUNE does not know, by the
    /// entry they belong to.
    fn folder_entries(&self) -> Decoded<Layout> {
        let mut entries = Vec::new();
        for field in fields(&self.folders)? {
            if field.number == 1 {
                if entries.len() >= FOLDERS_MAX {
                    return Err(Malformed);
                }
                entries.push(folder(field.bytes()?)?);
            }
        }
        Ok(normalize(entries))
    }

    /// The folders subtree laid out as `folders`: positions and unknown
    /// fields of the subtree and of each entry stay.
    pub fn with_folders(&self, folders: &[ServerFolder]) -> Decoded<Vec<u8>> {
        if !rune_model::account::valid_folders(folders) {
            return Err(Malformed);
        }
        let (_, unknown) = self.folder_entries()?;
        let mut subtree = without(&self.folders, &[1])?;
        for folder in folders {
            let mut entry = owner(folder)
                .and_then(|key| unknown.get(&key))
                .cloned()
                .unwrap_or_default();
            put_ids(1, folder.servers.iter().map(|server| server.0), &mut entry);
            if let Some(id) = folder.id {
                put_wrapped_number(2, id, &mut entry);
            }
            if let Some(name) = &folder.name {
                put_wrapped_text(3, name, &mut entry);
            }
            if let Some(color) = folder.color {
                put_wrapped_number(4, u64::from(color.0), &mut entry);
            }
            put_bytes(1, &entry, &mut subtree);
        }
        Ok(subtree)
    }

    // Messaging privacy: `PrivacySettings` (field 8) and the spam and
    // explicit content filters of `TextAndImagesSettings` (field 6).

    pub fn privacy(&self) -> Decoded<MessagingPrivacy> {
        let mut privacy = MessagingPrivacy::default();
        let mut restricted_v2 = None;
        for field in fields(&self.privacy)? {
            match field.number {
                3 | 16 => {
                    let list = if field.number == 3 {
                        &mut privacy.dms_off
                    } else {
                        &mut privacy.requests_off
                    };
                    let ids = field.ids()?;
                    if ids.contains(&0) || list.len() + ids.len() > PRIVACY_SERVERS_MAX {
                        return Err(Malformed);
                    }
                    list.extend(ids.into_iter().map(ServerId));
                }
                4 => privacy.server_dms = !boolean(field.varint()?)?,
                27 => restricted_v2 = Some(boolean(field.wrapped_varint()?)?),
                17 => privacy.request_filter = !boolean(field.wrapped_varint()?)?,
                11 => {
                    let flags = field.wrapped_varint()?;
                    privacy.friend_sources = FriendSources {
                        everyone: flags & 8 != 0,
                        friends_of_friends: flags & 2 != 0,
                        server_members: flags & 4 != 0,
                    };
                }
                _ => {}
            }
        }
        // The newer field says it when both are there, as Serein reads them.
        if let Some(restricted) = restricted_v2 {
            privacy.server_dms = !restricted;
        }
        for list in [&mut privacy.dms_off, &mut privacy.requests_off] {
            list.sort_unstable();
            list.dedup();
        }
        for field in fields(&self.text)? {
            match field.number {
                27 => privacy.spam = spam_of(field.varint()?),
                29 => {
                    for inner in fields(field.bytes()?)? {
                        let filter = content_of(inner.varint()?);
                        match inner.number {
                            1 => privacy.explicit_servers = filter,
                            2 => privacy.explicit_friends = filter,
                            3 => privacy.explicit_others = filter,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(privacy)
    }

    /// The subtree that carries `change`, and its number, written over what
    /// was read: only the fields that say it are replaced.
    pub fn with_privacy(&self, change: PrivacyChange) -> Decoded<(u64, Vec<u8>)> {
        let mut wanted = self.privacy()?;
        wanted.apply(change);
        if !wanted.valid() {
            return Err(Malformed);
        }
        let ids = |list: &[ServerId]| list.iter().map(|server| server.0).collect::<Vec<_>>();
        match change {
            PrivacyChange::Spam(filter) => {
                let mut subtree = without(&self.text, &[27])?;
                put_number(27, spam_number(filter), &mut subtree);
                Ok((TEXT, subtree))
            }
            PrivacyChange::ExplicitServers(_)
            | PrivacyChange::ExplicitFriends(_)
            | PrivacyChange::ExplicitOthers(_) => {
                let mut message = Vec::new();
                for field in fields(&self.text)?.into_iter().filter(|f| f.number == 29) {
                    message = without(field.bytes()?, &[1, 2, 3])?;
                }
                for (number, filter) in [
                    (1, wanted.explicit_servers),
                    (2, wanted.explicit_friends),
                    (3, wanted.explicit_others),
                ] {
                    // proto3 leaves defaults out.
                    if filter != ContentFilter::Default {
                        put_number(number, content_number(filter), &mut message);
                    }
                }
                let mut subtree = without(&self.text, &[29])?;
                put_bytes(29, &message, &mut subtree);
                Ok((TEXT, subtree))
            }
            PrivacyChange::ServerDms(_) => {
                // Both fields, as the official client and Serein write them.
                let mut subtree = without(&self.privacy, &[4, 27])?;
                put_number(4, u64::from(!wanted.server_dms), &mut subtree);
                put_wrapped_number(27, u64::from(!wanted.server_dms), &mut subtree);
                Ok((PRIVACY, subtree))
            }
            PrivacyChange::ServerDmsIn(..) => {
                let mut subtree = without(&self.privacy, &[3])?;
                put_ids(3, ids(&wanted.dms_off).into_iter(), &mut subtree);
                Ok((PRIVACY, subtree))
            }
            PrivacyChange::RequestFilter(_) => {
                let mut subtree = without(&self.privacy, &[17])?;
                put_wrapped_number(17, u64::from(!wanted.request_filter), &mut subtree);
                Ok((PRIVACY, subtree))
            }
            PrivacyChange::RequestFilterIn(..) => {
                let mut subtree = without(&self.privacy, &[16])?;
                put_ids(16, ids(&wanted.requests_off).into_iter(), &mut subtree);
                Ok((PRIVACY, subtree))
            }
            PrivacyChange::FriendSources(sources) => {
                let flags = if sources.everyone {
                    14
                } else {
                    u64::from(sources.friends_of_friends) * 2
                        + u64::from(sources.server_members) * 4
                };
                let mut subtree = without(&self.privacy, &[11])?;
                put_wrapped_number(11, flags, &mut subtree);
                Ok((PRIVACY, subtree))
            }
        }
    }
}

// Desktop session and games (escritorio-sesion-y-juegos): whether the account
// shows its activity, `StatusSettings.show_current_game` (field 3 of field
// 11, a `BoolValue`), as Serein reads and writes it.
const SHOW_GAME: u64 = 3;

impl Settings {
    /// Whether the account shows its activity. Left out, it does, as a new
    /// account; a wrapper without a value is proto3's false.
    pub fn shows_activity(&self) -> Decoded<bool> {
        let mut shown = None;
        for field in fields(&self.status)? {
            if field.number == SHOW_GAME {
                if shown.is_some() {
                    return Err(Malformed);
                }
                shown = Some(boolean(field.wrapped_varint()?)?);
            }
        }
        Ok(shown.unwrap_or(true))
    }

    /// The status subtree with activity sharing `on`: the status, the
    /// custom status and every unknown field stay.
    pub fn with_activity_shown(&self, on: bool) -> Decoded<Vec<u8>> {
        let mut subtree = without(&self.status, &[SHOW_GAME])?;
        put_wrapped_number(SHOW_GAME, u64::from(on), &mut subtree);
        Ok(subtree)
    }
}

/// A layout, and the bytes of each entry RUNE does not read, by owner.
type Layout = (Vec<ServerFolder>, HashMap<Owner, Vec<u8>>);

/// Who an entry's unread bytes go back to when the layout is written: a
/// folder by its id, a server on its own by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Owner {
    Folder(u64),
    Lone(ServerId),
}

fn owner(entry: &ServerFolder) -> Option<Owner> {
    entry
        .id
        .map(Owner::Folder)
        .or_else(|| entry.servers.first().map(|server| Owner::Lone(*server)))
}

fn folder(bytes: &[u8]) -> Decoded<(ServerFolder, Vec<u8>)> {
    let mut folder = ServerFolder::default();
    let mut unknown = Vec::new();
    for field in fields(bytes)? {
        match field.number {
            1 => {
                let ids = field.ids()?;
                if folder.servers.len() + ids.len() > FOLDERS_MAX {
                    return Err(Malformed);
                }
                folder.servers.extend(ids.into_iter().map(ServerId));
            }
            2 => folder.id = Some(field.wrapped_varint()?),
            3 => {
                // Bounded and printable, as the interface draws it.
                let name: String = field
                    .wrapped_text()?
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .take(FOLDER_NAME_MAX)
                    .collect();
                folder.name = Some(name);
            }
            4 => {
                folder.color = u32::try_from(field.wrapped_varint()?)
                    .ok()
                    .filter(|color| *color <= 0xFF_FF_FF)
                    .map(Rgb)
            }
            _ => unknown.extend_from_slice(field.raw),
        }
    }
    Ok((folder, unknown))
}

fn normalize(entries: Vec<(ServerFolder, Vec<u8>)>) -> Layout {
    // The first nonzero use of a folder id stands; any other folder takes
    // the lowest id nobody uses.
    let mut taken = HashSet::new();
    let stands: Vec<bool> = entries
        .iter()
        .map(|(entry, _)| matches!(entry.id, Some(id) if id != 0 && taken.insert(id)))
        .collect();
    let mut unused = (1..).filter(|id| !taken.contains(id));
    let mut placed = HashSet::new();
    let (mut layout, mut unread) = (Vec::new(), HashMap::new());
    for ((mut entry, extra), stands) in entries.into_iter().zip(stands) {
        if entry.id.is_some() && !stands {
            entry.id = unused.next();
        }
        // A server stays where it is first named.
        entry
            .servers
            .retain(|server| server.0 != 0 && placed.insert(*server));
        let parts = match entry.id {
            // Without an id a group is no folder: its servers stand alone,
            // and an empty one goes.
            None if entry.servers.len() != 1 => entry
                .servers
                .into_iter()
                .map(ServerFolder::single)
                .collect(),
            _ => vec![entry],
        };
        if let Some(first) = parts.first().and_then(owner).filter(|_| !extra.is_empty()) {
            unread.insert(first, extra);
        }
        layout.extend(parts);
    }
    (layout, unread)
}

fn boolean(value: u64) -> Decoded<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Malformed),
    }
}

// `DmSpamFilterV2` and `ExplicitContentRedaction`, as the bundle numbers them.

fn spam_of(value: u64) -> SpamFilter {
    match value {
        1 => SpamFilter::Off,
        2 => SpamFilter::NonFriends,
        3 => SpamFilter::Everyone,
        _ => SpamFilter::Default,
    }
}

fn spam_number(filter: SpamFilter) -> u64 {
    match filter {
        SpamFilter::Default => 0,
        SpamFilter::Off => 1,
        SpamFilter::NonFriends => 2,
        SpamFilter::Everyone => 3,
    }
}

fn content_of(value: u64) -> ContentFilter {
    match value {
        1 => ContentFilter::Show,
        2 => ContentFilter::Blur,
        3 => ContentFilter::Block,
        _ => ContentFilter::Default,
    }
}

fn content_number(filter: ContentFilter) -> u64 {
    match filter {
        ContentFilter::Default => 0,
        ContentFilter::Show => 1,
        ContentFilter::Blur => 2,
        ContentFilter::Block => 3,
    }
}

/// Fixtures: an answer carrying `subtrees` at data `version`.
#[cfg(test)]
pub(crate) fn answer(version: u64, subtrees: &[(u64, Vec<u8>)]) -> Value {
    let mut wire = Vec::new();
    let mut versions = Vec::new();
    put_number(3, version, &mut versions);
    put_bytes(VERSIONS, &versions, &mut wire);
    for (number, subtree) in subtrees {
        put_bytes(*number, subtree, &mut wire);
    }
    serde_json::json!({ "settings": STANDARD.encode(wire) })
}

/// Fixtures: what a `PATCH` body would leave, merged over `before` as
/// Discord keeps it: the subtree written replaces the one read.
#[cfg(test)]
pub(crate) fn applied(before: &Settings, body: &Value) -> Value {
    let wire = STANDARD.decode(body["settings"].as_str().unwrap()).unwrap();
    let mut subtrees = vec![
        (TEXT, before.text.clone()),
        (PRIVACY, before.privacy.clone()),
        (STATUS, before.status.clone()),
        (FOLDERS, before.folders.clone()),
    ];
    for field in fields(&wire).unwrap() {
        for (number, subtree) in &mut subtrees {
            if *number == field.number {
                *subtree = field.bytes().unwrap().to_vec();
            }
        }
    }
    answer(before.version + 1, &subtrees)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A status subtree: `status`, a custom status of `text` with `emoji`,
    /// game sharing on, and a field from the future.
    pub(crate) fn status(status: &str, text: &str, emoji: Option<&str>) -> (u64, Vec<u8>) {
        let mut subtree = Vec::new();
        put_wrapped_text(1, status, &mut subtree);
        let mut custom = Vec::new();
        put_bytes(1, text.as_bytes(), &mut custom);
        if let Some(emoji) = emoji {
            put_bytes(3, emoji.as_bytes(), &mut custom);
        }
        put_fixed64(5, 1_700_000_000_000, &mut custom);
        put_bytes(2, &custom, &mut subtree);
        put_wrapped_number(3, 1, &mut subtree);
        put_bytes(55, b"future", &mut subtree);
        (STATUS, subtree)
    }

    /// A folders subtree: `folders`, then positions that must stay.
    pub(crate) fn folders(folders: &[ServerFolder]) -> (u64, Vec<u8>) {
        let settings = Settings::default();
        let mut subtree = settings.with_folders(folders).unwrap();
        put_ids(2, [9u64, 8].into_iter(), &mut subtree);
        (FOLDERS, subtree)
    }

    /// A privacy subtree: direct messages off in `dms_off`, friend requests
    /// only from friends of friends, and a field from the future.
    pub(crate) fn privacy(dms_off: &[u64]) -> (u64, Vec<u8>) {
        let mut subtree = Vec::new();
        put_ids(3, dms_off.iter().copied(), &mut subtree);
        put_wrapped_number(11, 2, &mut subtree);
        put_bytes(99, b"future", &mut subtree);
        (PRIVACY, subtree)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn read(value: &Value) -> Settings {
        decode(value).unwrap()
    }

    #[test]
    fn the_status_and_custom_status_round_trip_keeping_what_is_unknown() {
        let before = read(&answer(7, &[status("dnd", "Ocupada", Some("🛠️"))]));
        assert_eq!(before.version, 7);
        let own = before.own(0).unwrap();
        assert_eq!(own.status, Status::DoNotDisturb);
        assert_eq!(own.line().as_deref(), Some("🛠️ Ocupada"));
        let wanted = Own {
            status: Status::Idle,
            custom: Some(Custom {
                text: "Afuera".into(),
                emoji: None,
                server_emoji: None,
                expires: Some(1_900_000_000_000),
            }),
        };
        let body = patch(&before, STATUS, before.with_own(&wanted).unwrap()).unwrap();
        assert_eq!(body["required_data_version"], 7);
        let after = read(&applied(&before, &body));
        assert_eq!(after.own(0).unwrap(), wanted);
        // Expired, it is none; game sharing, the creation time and the
        // future field stayed.
        assert_eq!(after.own(1_900_000_000_000).unwrap().custom, None);
        let raw = &after.status;
        assert!(raw.windows(6).any(|w| w == b"future"));
        assert!(fields(raw).unwrap().iter().any(|field| field.number == 3));
        let custom = fields(raw)
            .unwrap()
            .into_iter()
            .find(|f| f.number == 2)
            .unwrap();
        assert!(fields(custom.bytes().unwrap())
            .unwrap()
            .iter()
            .any(|f| f.number == 5));
        // Taken away, the custom status leaves no trace.
        let cleared = Own {
            status: Status::Invisible,
            custom: None,
        };
        let body = patch(&after, STATUS, after.with_own(&cleared).unwrap()).unwrap();
        assert_eq!(read(&applied(&after, &body)).own(0).unwrap(), cleared);
    }

    #[test]
    fn a_servers_emoji_in_the_custom_status_shows_and_survives_a_change_of_status() {
        let mut custom = Vec::new();
        put_bytes(1, b"programando", &mut custom);
        put_fixed64(2, 1234567890123456789, &mut custom);
        put_bytes(3, b"Rana_Saltarina", &mut custom);
        let mut subtree = Vec::new();
        put_wrapped_text(1, "online", &mut subtree);
        put_bytes(2, &custom, &mut subtree);
        let before = read(&answer(3, &[(STATUS, subtree)]));
        let own = before.own(0).unwrap();
        assert_eq!(
            own.line().as_deref(),
            Some("programando"),
            "its name is no Unicode emoji"
        );
        let picture = own.picture().unwrap();
        assert_eq!(picture.name, "Rana_Saltarina");
        assert_eq!(
            picture.image.url,
            "https://cdn.discordapp.com/emojis/1234567890123456789.webp?size=48"
        );
        // Away for a while: the custom status keeps its emoji.
        let wanted = Own {
            status: Status::Idle,
            ..own.clone()
        };
        let body = patch(&before, STATUS, before.with_own(&wanted).unwrap()).unwrap();
        assert_eq!(read(&applied(&before, &body)).own(0).unwrap(), wanted);
    }

    #[test]
    fn folders_round_trip_keeping_positions_and_messy_layouts_are_normalized() {
        let layout = vec![
            ServerFolder::single(ServerId(1)),
            ServerFolder {
                id: Some(5),
                name: Some("Juegos".into()),
                color: Some(Rgb(0x5865F2)),
                servers: vec![ServerId(2), ServerId(3)],
            },
        ];
        let before = read(&answer(1, &[folders(&layout)]));
        assert_eq!(before.folders().unwrap(), layout);
        let moved = vec![layout[1].clone(), layout[0].clone()];
        let body = patch(&before, FOLDERS, before.with_folders(&moved).unwrap()).unwrap();
        let after = read(&applied(&before, &body));
        assert_eq!(after.folders().unwrap(), moved);
        let positions = fields(&after.folders).unwrap();
        assert!(positions.iter().any(|field| field.number == 2));
        // Another client's leftovers: a server named twice, a group with
        // no id, a folder id repeated and an empty entry.
        let mut messy = Vec::new();
        for (id, servers) in [
            (Some(4u64), vec![1u64, 2]),
            (None, vec![2, 3, 4]),
            (Some(4), vec![5]),
            (None, vec![]),
        ] {
            let mut entry = Vec::new();
            put_ids(1, servers.into_iter(), &mut entry);
            if let Some(id) = id {
                put_wrapped_number(2, id, &mut entry);
            }
            put_bytes(1, &entry, &mut messy);
        }
        let shown = read(&answer(1, &[(FOLDERS, messy)])).folders().unwrap();
        let summary: Vec<(Option<u64>, Vec<u64>)> = shown
            .iter()
            .map(|f| (f.id, f.servers.iter().map(|s| s.0).collect()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Some(4), vec![1, 2]),
                (None, vec![3]),
                (None, vec![4]),
                (Some(1), vec![5]),
            ]
        );
        assert!(rune_model::account::valid_folders(&shown));
    }

    #[test]
    fn privacy_changes_rewrite_only_their_fields() {
        let before = read(&answer(3, &[privacy(&[30, 10])]));
        let read_privacy = before.privacy().unwrap();
        assert_eq!(read_privacy.dms_off, vec![ServerId(10), ServerId(30)]);
        assert!(read_privacy.server_dms && read_privacy.request_filter);
        assert_eq!(
            read_privacy.friend_sources,
            FriendSources {
                everyone: false,
                friends_of_friends: true,
                server_members: false
            }
        );
        let mut current = before.clone();
        for change in [
            PrivacyChange::ServerDmsIn(ServerId(20), false),
            PrivacyChange::ServerDms(false),
            PrivacyChange::RequestFilterIn(ServerId(10), false),
            PrivacyChange::Spam(SpamFilter::Everyone),
            PrivacyChange::ExplicitFriends(ContentFilter::Blur),
            PrivacyChange::FriendSources(FriendSources::default()),
        ] {
            let mut wanted = current.privacy().unwrap();
            wanted.apply(change);
            let (number, subtree) = current.with_privacy(change).unwrap();
            let body = patch(&current, number, subtree).unwrap();
            current = read(&applied(&current, &body));
            assert_eq!(current.privacy().unwrap(), wanted, "{change:?}");
        }
        assert!(current.privacy.windows(6).any(|w| w == b"future"));
        // A server listed twice or zero is not taken.
        let mut broken = Vec::new();
        put_ids(3, [0u64].into_iter(), &mut broken);
        assert!(read(&answer(1, &[(PRIVACY, broken)])).privacy().is_err());
    }

    #[test]
    fn broken_answers_are_refused() {
        assert!(decode(&serde_json::json!({"settings": "%%%"})).is_err());
        assert!(decode(&serde_json::json!({"settings": STANDARD.encode([0x08])})).is_err());
        let mut truncated = Vec::new();
        put_bytes(VERSIONS, &[], &mut truncated);
        put_varint(STATUS << 3 | 2, &mut truncated);
        put_varint(40, &mut truncated);
        assert!(decode(&serde_json::json!({"settings": STANDARD.encode(truncated)})).is_err());
        let mut stale = answer(1, &[]);
        stale["out_of_date"] = true.into();
        assert!(decode(&stale).is_err());
        // Without a version there is nothing to write over.
        assert!(decode(&serde_json::json!({"settings": ""})).is_err());
        assert!(varint(&mut &[0xff; 11][..]).is_err());
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    #[test]
    fn activity_sharing_is_on_unless_said_and_its_write_keeps_the_status() {
        assert!(read(&answer(1, &[])).shows_activity().unwrap());
        // The fixture's game sharing is on; turned off, the rest stays.
        let before = read(&answer(4, &[status("dnd", "Ocupada", None)]));
        assert!(before.shows_activity().unwrap());
        let body = patch(&before, STATUS, before.with_activity_shown(false).unwrap()).unwrap();
        assert_eq!(body["required_data_version"], 4);
        let after = read(&applied(&before, &body));
        assert!(!after.shows_activity().unwrap());
        assert_eq!(after.own(0).unwrap(), before.own(0).unwrap());
        assert!(after.status.windows(6).any(|w| w == b"future"));
        // A status written later keeps it off.
        let wanted = Own {
            status: Status::Idle,
            custom: None,
        };
        let body = patch(&after, STATUS, after.with_own(&wanted).unwrap()).unwrap();
        assert!(!read(&applied(&after, &body)).shows_activity().unwrap());
        // An empty wrapper is false; a value that is no boolean is refused.
        let mut empty = Vec::new();
        put_bytes(SHOW_GAME, &[], &mut empty);
        assert!(!read(&answer(1, &[(STATUS, empty)]))
            .shows_activity()
            .unwrap());
        let mut odd = Vec::new();
        put_wrapped_number(SHOW_GAME, 2, &mut odd);
        assert!(read(&answer(1, &[(STATUS, odd)])).shows_activity().is_err());
    }
}
