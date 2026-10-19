//! What the store keeps of an account is sealed with that account's key:
//! XChaCha20-Poly1305 with a random nonce per value, and the value's place
//! (what it is, the account, the conversation, the row) as associated data,
//! so a value copied to another place fails to open instead of showing
//! there. The key lives in the system keyring, never in the file.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use rusqlite::types::ValueRef;
use sha2::Sha256;
use zeroize::Zeroizing;

/// The layout of a sealed value: this byte, the nonce, then the ciphertext.
const VERSION: u8 = 1;
const NONCE: usize = 24;
/// What sealing adds to a value: the version, the nonce and the tag.
pub const OVERHEAD: usize = 1 + NONCE + 16;

/// One account's key: 32 random bytes. Every copy is erased when dropped,
/// and none is ever printed.
#[derive(Clone)]
pub struct Key(Zeroizing<[u8; 32]>);

impl Key {
    /// A new random key, or `None` when the system gives no randomness.
    pub fn generate() -> Option<Self> {
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(&mut *bytes).ok()?;
        Some(Self(bytes))
    }

    /// As the keyring keeps it: 64 lowercase hexadecimal digits.
    pub fn to_text(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(String::with_capacity(64));
        for byte in self.0.iter() {
            for digit in [byte >> 4, byte & 15] {
                text.push(char::from_digit(digit.into(), 16).expect("a nibble"));
            }
        }
        text
    }

    /// The key [`Key::to_text`] wrote, or `None` for anything else.
    pub fn from_text(text: &str) -> Option<Self> {
        let text = text.as_bytes();
        if text.len() != 64 {
            return None;
        }
        let mut bytes = Zeroizing::new([0; 32]);
        for (byte, pair) in bytes.iter_mut().zip(text.chunks(2)) {
            let digit = |at: u8| {
                (at as char)
                    .to_digit(16)
                    .filter(|_| !at.is_ascii_uppercase())
            };
            *byte = (digit(pair[0])? << 4 | digit(pair[1])?) as u8;
        }
        Some(Self(bytes))
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&(*self.0).into())
    }

    /// Seal `plain` for `place`.
    pub(crate) fn seal(&self, place: &Place, plain: &[u8]) -> Option<Vec<u8>> {
        let mut nonce = [0; NONCE];
        getrandom::fill(&mut nonce).ok()?;
        let sealed = self
            .cipher()
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: &place.0,
                },
            )
            .ok()?;
        let mut value = Vec::with_capacity(1 + NONCE + sealed.len());
        value.push(VERSION);
        value.extend_from_slice(&nonce);
        value.extend_from_slice(&sealed);
        Some(value)
    }

    /// The text sealed for `place`; `None` for a value in plain text, sealed
    /// with another key or for another place, or damaged.
    pub(crate) fn open(&self, place: &Place, value: ValueRef<'_>) -> Option<Zeroizing<String>> {
        let ValueRef::Blob(value) = value else {
            return None;
        };
        let plain = Zeroizing::new(self.open_bytes(place, value)?);
        std::str::from_utf8(&plain)
            .ok()
            .map(|text| Zeroizing::new(text.to_owned()))
    }

    fn open_bytes(&self, place: &Place, value: &[u8]) -> Option<Vec<u8>> {
        let (&VERSION, rest) = value.split_first()? else {
            return None;
        };
        if rest.len() < NONCE {
            return None;
        }
        let (nonce, sealed) = rest.split_at(NONCE);
        self.cipher()
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: &place.0,
                },
            )
            .ok()
    }

    // Files outside the store (imagenes-y-lectura): downloaded pictures.
    /// Seal `plain`, the file `name` RUNE keeps in `part` for `account`
    /// outside the store; it opens at that place only.
    pub fn seal_file(
        &self,
        account: &str,
        part: &str,
        name: &str,
        plain: &[u8],
    ) -> Option<Vec<u8>> {
        self.seal(&Place::file(account, part, name), plain)
    }

    /// What [`Key::seal_file`] sealed at that place; `None` for a file
    /// sealed with another key or at another place, or damaged.
    pub fn open_file(
        &self,
        account: &str,
        part: &str,
        name: &str,
        sealed: &[u8],
    ) -> Option<Vec<u8>> {
        self.open_bytes(&Place::file(account, part, name), sealed)
    }

    /// The name of the file kept for `what` (a picture's URL): a digest
    /// keyed with a key of its own drawn from this one, so without it a
    /// name tells nothing of what the file holds.
    pub fn file_name(&self, what: &str) -> String {
        let keyed = |key: &[u8], text: &[u8]| -> [u8; 32] {
            <Hmac<Sha256> as Mac>::new_from_slice(key)
                .expect("HMAC takes a key of any length")
                .chain_update(text)
                .finalize()
                .into_bytes()
                .into()
        };
        let names = Zeroizing::new(keyed(&*self.0, b"rune file names"));
        keyed(&*names, what.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Key([REDACTED])")
    }
}

/// Where a sealed value belongs. A message has the same place in the
/// conversation's rows and among the ones a search found, so it can move
/// between them without being sealed again.
pub(crate) struct Place(Vec<u8>);

impl Place {
    pub(crate) fn message(account: &str, channel: i64, id: i64) -> Self {
        Self::new("message", account, channel, id)
    }

    pub(crate) fn draft(account: &str, channel: i64) -> Self {
        Self::new("draft", account, channel, 0)
    }

    pub(crate) fn snapshot(account: &str) -> Self {
        Self::new("snapshot", account, 0, 0)
    }

    pub(crate) fn read_mark(account: &str, channel: i64) -> Self {
        Self::new("read_mark", account, channel, 0)
    }

    pub(crate) fn draft_files(account: &str, channel: i64) -> Self {
        Self::new("draft_files", account, channel, 0)
    }

    /// A file kept outside the store: a kind of its own, so it never
    /// stands for one of the rows' places.
    fn file(account: &str, part: &str, name: &str) -> Self {
        Self(
            format!(
                "4:file:{}:{account}:{}:{part}:{name}",
                account.len(),
                part.len()
            )
            .into_bytes(),
        )
    }

    fn new(what: &str, account: &str, channel: i64, id: i64) -> Self {
        // Lengths first: no account text can pass for another place.
        Self(
            format!(
                "{}:{what}:{}:{account}:{channel}:{id}",
                what.len(),
                account.len()
            )
            .into_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_value_opens_only_with_its_key_and_at_its_place() {
        let key = Key::generate().unwrap();
        let here = Place::message("a", 5, 1);
        let sealed = key.seal(&here, "hola **mundo**".as_bytes()).unwrap();
        assert!(!sealed.windows(5).any(|window| window == b"mundo"));
        assert_eq!(
            key.open(&here, ValueRef::Blob(&sealed))
                .as_deref()
                .map(String::as_str),
            Some("hola **mundo**")
        );
        assert!(key
            .open(&Place::message("b", 5, 1), ValueRef::Blob(&sealed))
            .is_none());
        assert!(key
            .open(&Place::message("a", 5, 2), ValueRef::Blob(&sealed))
            .is_none());
        assert!(key
            .open(&Place::draft("a", 5), ValueRef::Blob(&sealed))
            .is_none());
        assert!(Key::generate()
            .unwrap()
            .open(&here, ValueRef::Blob(&sealed))
            .is_none());
        let mut damaged = sealed.clone();
        *damaged.last_mut().unwrap() ^= 1;
        assert!(key.open(&here, ValueRef::Blob(&damaged)).is_none());
        assert!(key.open(&here, ValueRef::Text(b"hola")).is_none());
        assert!(key.open(&here, ValueRef::Blob(&[VERSION])).is_none());
        // A nonce of its own each time: the same text never seals the same.
        assert_ne!(
            key.seal(&here, b"x").unwrap(),
            key.seal(&here, b"x").unwrap()
        );
    }

    #[test]
    fn a_file_sealed_outside_the_store_opens_only_at_its_place() {
        let key = Key::generate().unwrap();
        let picture = [0, 159, 146, 150, 255];
        let sealed = key.seal_file("a", "media", "x", &picture).unwrap();
        assert_eq!(sealed.len(), picture.len() + OVERHEAD);
        assert_eq!(
            key.open_file("a", "media", "x", &sealed).as_deref(),
            Some(&picture[..])
        );
        for (account, part, name) in [
            ("b", "media", "x"),
            ("a", "artwork", "x"),
            ("a", "media", "y"),
        ] {
            assert!(key.open_file(account, part, name, &sealed).is_none());
        }
        assert!(Key::generate()
            .unwrap()
            .open_file("a", "media", "x", &sealed)
            .is_none());
        // Text sealed at a file's place never opens as one of the rows'.
        let text = key.seal_file("a", "media", "x", b"hola").unwrap();
        assert!(key
            .open(&Place::draft("a", 0), ValueRef::Blob(&text))
            .is_none());
    }

    #[test]
    fn a_file_s_name_depends_on_the_key_and_says_nothing_of_its_url() {
        let key = Key::generate().unwrap();
        let url = "https://cdn.test/avatars/1/a.png";
        let name = key.file_name(url);
        assert_eq!(name, key.file_name(url));
        assert_eq!(name.len(), 64);
        assert!(name
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')));
        assert_ne!(name, key.file_name("https://cdn.test/avatars/1/b.png"));
        assert_ne!(name, Key::generate().unwrap().file_name(url));
        // Not the digest anyone could work out from the URL alone.
        use sha2::Digest;
        assert_ne!(name, format!("{:x}", Sha256::digest(url.as_bytes())));
    }

    #[test]
    fn a_key_reads_back_from_its_text_and_is_never_printed() {
        let key = Key::generate().unwrap();
        let text = key.to_text();
        assert_eq!(text.len(), 64);
        let again = Key::from_text(&text).unwrap();
        assert_eq!(*again.0, *key.0);
        for invalid in ["", "00", &"g".repeat(64), &"AB".repeat(32), &"0".repeat(65)] {
            assert!(Key::from_text(invalid).is_none(), "{invalid}");
        }
        assert_eq!(format!("{key:?}"), "Key([REDACTED])");
        assert!(!format!("{key:?}").contains(text.as_str()));
    }
}
