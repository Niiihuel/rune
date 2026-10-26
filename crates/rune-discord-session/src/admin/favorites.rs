//! The account's GIF favorites as Discord keeps them, in the
//! `FrecencyUserSettings` protobuf of `/users/@me/settings-proto/2` (the
//! route and field numbers Serein's `gif_favorites.rs` uses, read
//! 2026-10-17): field 1 holds the versions (the data version is its field 3),
//! field 2 the favorites, a map from each GIF's address to its picture
//! (`format` 1, `src` 2, `width` 3, `height` 4, `order` 5). A change rewrites
//! only field 2, keeping every entry and field it does not understand byte
//! for byte. The protobuf is read and written with the account settings'
//! own bounded reader ([`crate::settings_proto`]).

use base64::{engine::general_purpose::STANDARD, Engine};
use rune_i18n::es;
use rune_provider::FavoriteGif;
use serde_json::Value;

use crate::settings_proto::{self as proto, put_bytes, put_number, Field};
use crate::{wire, LoginError};

/// The answer's size, as Serein bounds it.
pub(crate) const MAX_RESPONSE: usize = 6 * 1024 * 1024;
/// The favorites' own field, at most.
const MAX_SUBTREE: usize = 512 * 1024;
/// Fields one message may hold, and favorites one account may keep.
const MAX_FIELDS: usize = 4096;
const MAX_ENTRIES: usize = 2048;
/// The longest address kept.
const MAX_URL: usize = 2048;

/// Every field of one message, as the shared reader reads it, bounded here
/// to what one favorites message may hold.
fn fields(bytes: &[u8]) -> Option<Vec<Field<'_>>> {
    proto::fields(bytes)
        .ok()
        .filter(|fields| fields.len() <= MAX_FIELDS)
}

/// One favorite as the map keeps it.
struct Entry<'a> {
    url: &'a str,
    src: &'a str,
    width: u64,
    height: u64,
    order: u64,
}

fn entry(bytes: &[u8]) -> Option<Entry<'_>> {
    let fields = fields(bytes)?;
    let one = |number| {
        let mut found = fields.iter().filter(|field| field.number == number);
        let first = found.next();
        found.next().is_none().then_some(first)
    };
    let url = std::str::from_utf8(one(1)??.bytes().ok()?).ok()?;
    let mut entry = Entry {
        url,
        src: "",
        width: 0,
        height: 0,
        order: 0,
    };
    if let Some(value) = one(2)? {
        for field in fields_once(value.bytes().ok()?)? {
            match field.number {
                2 => entry.src = std::str::from_utf8(field.bytes().ok()?).ok()?,
                3 => entry.width = field.varint().ok()?,
                4 => entry.height = field.varint().ok()?,
                5 => entry.order = field.varint().ok()?,
                _ => {}
            }
        }
    }
    (!url.is_empty()).then_some(entry)
}

/// The fields of a message whose known fields come at most once each.
fn fields_once(bytes: &[u8]) -> Option<Vec<Field<'_>>> {
    let fields = fields(bytes)?;
    let mut seen = 0u8;
    for field in fields
        .iter()
        .filter(|field| (1..=5).contains(&field.number))
    {
        let bit = 1 << field.number;
        if seen & bit != 0 {
            return None;
        }
        seen |= bit;
    }
    Some(fields)
}

/// The favorites as Discord answered them, kept whole for a change.
pub(crate) struct Favorites {
    /// The data version a change must name.
    pub version: u64,
    subtree: Vec<u8>,
}

impl Favorites {
    /// Read `{"settings": base64, "out_of_date"?}`. An answer saying the
    /// change came too late is a refusal: nothing was saved.
    pub(crate) fn decode(value: &Value) -> Result<Self, LoginError> {
        if value["out_of_date"].as_bool() == Some(true) {
            return Err(LoginError::new(es(
                "Tus favoritos cambiaron en otro dispositivo; vuelve a intentarlo.",
            )));
        }
        let encoded = value["settings"]
            .as_str()
            .filter(|text| text.len() <= MAX_RESPONSE)
            .ok_or_else(wire::invalid)?;
        let bytes = STANDARD.decode(encoded).map_err(|_| wire::invalid())?;
        let mut version = None;
        let mut subtree = None;
        for field in fields(&bytes).ok_or_else(wire::invalid)? {
            match field.number {
                1 if version.is_none() => {
                    let versions = fields(field.bytes().map_err(|_| wire::invalid())?)
                        .ok_or_else(wire::invalid)?;
                    let mut data = versions.iter().filter(|field| field.number == 3);
                    let found = data
                        .next()
                        .map(|field| field.varint().map_err(|_| wire::invalid()));
                    if data.next().is_some() {
                        return Err(wire::invalid());
                    }
                    version = Some(found.transpose()?.unwrap_or(0));
                }
                2 if subtree.is_none() => {
                    let value = field.bytes().map_err(|_| wire::invalid())?;
                    if value.len() > MAX_SUBTREE {
                        return Err(wire::invalid());
                    }
                    subtree = Some(value.to_vec());
                }
                1 | 2 => return Err(wire::invalid()),
                _ => {}
            }
        }
        let favorites = Self {
            version: version.unwrap_or(0),
            subtree: subtree.unwrap_or_default(),
        };
        // Every entry must read, and name a different address.
        let mut urls = std::collections::HashSet::new();
        for field in favorites.entries()? {
            if urls.len() >= MAX_ENTRIES || !urls.insert(field.url) {
                return Err(wire::invalid());
            }
        }
        Ok(favorites)
    }

    fn entries(&self) -> Result<Vec<Entry<'_>>, LoginError> {
        fields(&self.subtree)
            .ok_or_else(wire::invalid)?
            .into_iter()
            .filter(|field| field.number == 1)
            .map(|field| field.bytes().ok().and_then(entry).ok_or_else(wire::invalid))
            .collect()
    }

    /// The favorites RUNE can show, the first added first.
    pub(crate) fn list(&self) -> Vec<FavoriteGif> {
        let mut entries = self.entries().unwrap_or_default();
        entries.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.url.cmp(b.url)));
        entries
            .into_iter()
            .filter(|entry| address(entry.url))
            .map(|entry| FavoriteGif {
                url: entry.url.to_owned(),
                preview: address(entry.src).then(|| entry.src.to_owned()),
                width: u32::try_from(entry.width).unwrap_or(0).min(4096),
                height: u32::try_from(entry.height).unwrap_or(0).min(4096),
            })
            .collect()
    }

    pub(crate) fn contains(&self, url: &str) -> bool {
        self.entries()
            .is_ok_and(|entries| entries.iter().any(|entry| entry.url == url))
    }

    /// The base64 of field 2 with `gif` added (after every other one) or
    /// taken out; everything else in it stays as it came.
    pub(crate) fn patch(&self, gif: &FavoriteGif, favorite: bool) -> Result<String, LoginError> {
        let too_many = || LoginError::new(es("Ya tienes demasiados GIF favoritos."));
        let mut subtree = Vec::with_capacity(self.subtree.len() + 256);
        // The latest place among the entries, and how many are not `gif`.
        let mut last = None::<u64>;
        let mut others = 0;
        for field in fields(&self.subtree).ok_or_else(wire::invalid)? {
            let is_gif = field.number == 1 && {
                let entry = field
                    .bytes()
                    .ok()
                    .and_then(entry)
                    .ok_or_else(wire::invalid)?;
                last = last.max(Some(entry.order));
                entry.url == gif.url
            };
            if !is_gif {
                others += usize::from(field.number == 1);
                subtree.extend_from_slice(field.raw);
            }
        }
        if favorite {
            if others >= MAX_ENTRIES {
                return Err(too_many());
            }
            let order = match last {
                None => 0,
                Some(order) => order.checked_add(1).ok_or_else(wire::invalid)?,
            };
            put_bytes(1, &encoded(gif, order), &mut subtree);
        }
        if subtree.len() > MAX_SUBTREE {
            return Err(too_many());
        }
        let mut patch = Vec::with_capacity(subtree.len() + 8);
        put_bytes(2, &subtree, &mut patch);
        Ok(STANDARD.encode(patch))
    }
}

/// One entry as the settings keep it: the page's address, then its picture
/// (kind, address, size) and its place among the favorites.
fn encoded(gif: &FavoriteGif, order: u64) -> Vec<u8> {
    let preview = gif.preview.as_deref().unwrap_or(&gif.url);
    let mut picture = Vec::new();
    put_number(1, format(preview), &mut picture);
    put_bytes(2, preview.as_bytes(), &mut picture);
    put_number(3, u64::from(gif.width), &mut picture);
    put_number(4, u64::from(gif.height), &mut picture);
    put_number(5, order, &mut picture);
    let mut entry = Vec::new();
    put_bytes(1, gif.url.as_bytes(), &mut entry);
    put_bytes(2, &picture, &mut entry);
    entry
}

/// Discord's picture kind: a video clip (2) or an image (1).
fn format(source: &str) -> u64 {
    let path = source.split('?').next().unwrap_or(source);
    if [".mp4", ".webm", ".mov"]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        2
    } else {
        1
    }
}

/// An HTTPS address fit to show and to send.
fn address(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() <= MAX_URL
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// A favorite RUNE may write: HTTPS addresses, sizes within reason.
pub(crate) fn valid(gif: &FavoriteGif) -> Result<(), LoginError> {
    if address(&gif.url)
        && gif.preview.as_deref().is_none_or(address)
        && gif.width <= 4096
        && gif.height <= 4096
    {
        Ok(())
    } else {
        Err(LoginError::new(es(
            "Ese GIF no se puede guardar como favorito.",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gif(index: u32) -> FavoriteGif {
        FavoriteGif {
            url: format!("https://tenor.com/view/synthetic-{index}"),
            preview: Some(format!("https://media.tenor.com/synthetic/{index}.mp4")),
            width: 300,
            height: 200,
        }
    }

    /// An answer holding `subtree` as field 2, version `version`, and an
    /// unknown top-level field.
    fn answer(subtree: &[u8], version: u64) -> Value {
        let mut versions = Vec::new();
        put_number(1, 7, &mut versions);
        put_number(3, version, &mut versions);
        let mut settings = Vec::new();
        put_bytes(1, &versions, &mut settings);
        put_bytes(2, subtree, &mut settings);
        put_bytes(9, b"future", &mut settings);
        json!({ "settings": STANDARD.encode(settings) })
    }

    /// The field 2 a patch carries.
    fn patched(patch: &str) -> Vec<u8> {
        let bytes = STANDARD.decode(patch).unwrap();
        let fields = fields(&bytes).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].number, 2);
        fields[0].bytes().unwrap().to_vec()
    }

    #[test]
    fn favorites_round_trip_in_order_keeping_what_is_not_understood() {
        let empty = Favorites::decode(&answer(&[], 3)).unwrap();
        assert_eq!(empty.version, 3);
        assert!(empty.list().is_empty());
        let first = patched(&empty.patch(&gif(1), true).unwrap());
        // An entry RUNE cannot show, and the tooltip flag, stay as they are.
        let mut subtree = first.clone();
        put_bytes(
            1,
            &{
                let mut item = Vec::new();
                put_bytes(1, b"ftp://elsewhere/x.gif", &mut item);
                item
            },
            &mut subtree,
        );
        put_number(2, 1, &mut subtree);
        let current = Favorites::decode(&answer(&subtree, 4)).unwrap();
        assert_eq!(current.list(), [gif(1)]);
        let second = patched(&current.patch(&gif(2), true).unwrap());
        assert!(
            second.starts_with(&subtree),
            "untouched fields stay first, byte for byte"
        );
        let saved = Favorites::decode(&answer(&second, 5)).unwrap();
        assert_eq!(saved.list(), [gif(1), gif(2)], "the first added first");
        assert_eq!(format("https://media.tenor.com/x.mp4"), 2);
        assert_eq!(format("https://media.tenor.com/x.gif?w=1"), 1);
        // Taking one out keeps the rest, the odd one and the flag.
        let removed = patched(&saved.patch(&gif(1), false).unwrap());
        let left = Favorites::decode(&answer(&removed, 6)).unwrap();
        assert_eq!(left.list(), [gif(2)]);
        assert!(left.contains("ftp://elsewhere/x.gif") && !left.contains(&gif(1).url));
        assert!(fields(&removed)
            .unwrap()
            .iter()
            .any(|field| field.number == 2 && field.varint() == Ok(1)));
    }

    #[test]
    fn out_of_date_duplicates_and_garbage_are_refused() {
        let mut stale = answer(&[], 1);
        stale["out_of_date"] = json!(true);
        assert!(Favorites::decode(&stale).is_err());
        let one = patched(
            &Favorites::decode(&answer(&[], 1))
                .unwrap()
                .patch(&gif(1), true)
                .unwrap(),
        );
        let twice = [one.clone(), one].concat();
        assert!(Favorites::decode(&answer(&twice, 1)).is_err());
        assert!(Favorites::decode(&json!({"settings": "%%%"})).is_err());
        assert!(
            Favorites::decode(&json!({"settings": STANDARD.encode([0x0b])})).is_err(),
            "a group"
        );
        assert!(
            Favorites::decode(&json!({"settings": STANDARD.encode([0x12, 0x05, 0x01])})).is_err(),
            "cut short"
        );
        assert!(valid(&gif(1)).is_ok());
        let mut plain = gif(1);
        plain.url = "http://tenor.com/x".into();
        assert!(valid(&plain).is_err());
    }
}
