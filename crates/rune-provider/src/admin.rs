//! What the administration commands carry that must be checked before it
//! travels: a new emoji or sticker made on this device.

use bytes::Bytes;
use rune_i18n::t;

use crate::social::bounded_png;

/// Which kind of custom expression.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpressionKind {
    Emoji,
    Sticker,
}

impl ExpressionKind {
    /// The most bytes its picture may take, as chat services cap them.
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Emoji => 256 * 1024,
            Self::Sticker => 512 * 1024,
        }
    }

    /// The side of its square picture: at most that for an emoji, exactly
    /// that for a sticker.
    pub fn edge(self) -> u32 {
        match self {
            Self::Emoji => 128,
            Self::Sticker => 320,
        }
    }
}

/// The picture of a new expression: a square PNG made on this device, or a
/// GIF kept as it was chosen so it stays animated. Bounded by its kind's
/// [`ExpressionKind::max_bytes`]; its bytes never show in `Debug`.
#[derive(Clone, PartialEq)]
pub struct ExpressionImage {
    bytes: Bytes,
    gif: bool,
}

impl ExpressionImage {
    /// `None` unless `png` is a PNG of `kind`'s size and weight.
    pub fn png(kind: ExpressionKind, png: Bytes) -> Option<Self> {
        let sized = |at: usize| {
            png.get(at..at + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_be_bytes)
        };
        let fits = match kind {
            ExpressionKind::Emoji => true,
            ExpressionKind::Sticker => sized(16) == Some(320) && sized(20) == Some(320),
        };
        (fits && bounded_png(&png, kind.max_bytes(), kind.edge())).then_some(Self {
            bytes: png,
            gif: false,
        })
    }

    /// `None` unless `gif` is a GIF within `kind`'s weight and size.
    pub fn gif(kind: ExpressionKind, gif: Bytes) -> Option<Self> {
        let side = |at: usize| {
            gif.get(at..at + 2)
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as u32)
        };
        let (width, height) = (side(6)?, side(8)?);
        let fits = match kind {
            // Any size: the service shows it small.
            ExpressionKind::Emoji => width > 0 && height > 0,
            ExpressionKind::Sticker => width == kind.edge() && height == kind.edge(),
        };
        (fits
            && gif.len() <= kind.max_bytes()
            && (gif.starts_with(b"GIF87a") || gif.starts_with(b"GIF89a")))
        .then_some(Self {
            bytes: gif,
            gif: true,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// A GIF, possibly animated; else a PNG.
    pub fn is_gif(&self) -> bool {
        self.gif
    }
}

impl std::fmt::Debug for ExpressionImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format = if self.gif { "GIF" } else { "PNG" };
        write!(f, "ExpressionImage({format}, {} bytes)", self.bytes.len())
    }
}

/// A custom expression to add to a server.
#[derive(Clone, Debug, PartialEq)]
pub struct NewExpression {
    pub kind: ExpressionKind,
    /// As [`rune_model::valid_emoji_name`] or
    /// [`rune_model::valid_sticker_name`] require.
    pub name: String,
    pub image: ExpressionImage,
    /// A sticker's related emoji or words, as its search finds it: 1 to 200
    /// characters. Unused for an emoji.
    pub tags: String,
    /// What a sticker shows: empty, or 2 to 100 characters. Unused for an emoji.
    pub description: String,
}

impl NewExpression {
    /// Why it cannot be sent as it is, in the interface's words.
    pub fn problem(&self) -> Option<&'static str> {
        let name = self.name.trim();
        match self.kind {
            ExpressionKind::Emoji if !rune_model::valid_emoji_name(name) => Some(
                t("El nombre del emoji necesita de 2 a 32 letras sin acentos, números o guiones bajos."),
            ),
            ExpressionKind::Sticker if !rune_model::valid_sticker_name(name) => {
                Some(t("El nombre del sticker necesita de 2 a 30 caracteres."))
            }
            ExpressionKind::Sticker if !(1..=200).contains(&self.tags.trim().chars().count()) => {
                Some(t("Elige un emoji relacionado o una palabra para buscarlo."))
            }
            ExpressionKind::Sticker
                if !matches!(self.description.trim().chars().count(), 0 | 2..=100) =>
            {
                Some(t("La descripción va vacía o con 2 a 100 caracteres."))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Bytes {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        Bytes::from(bytes)
    }

    fn gif(width: u16, height: u16) -> Bytes {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        Bytes::from(bytes)
    }

    #[test]
    fn pictures_follow_each_kind_s_size_and_weight() {
        use ExpressionKind::{Emoji, Sticker};
        assert!(ExpressionImage::png(Emoji, png(128, 128)).is_some());
        assert!(ExpressionImage::png(Emoji, png(256, 256)).is_none());
        assert!(ExpressionImage::png(Sticker, png(320, 320)).is_some());
        assert!(ExpressionImage::png(Sticker, png(128, 128)).is_none());
        assert!(ExpressionImage::gif(Emoji, gif(48, 48)).unwrap().is_gif());
        assert!(ExpressionImage::gif(Sticker, gif(48, 48)).is_none());
        assert!(ExpressionImage::gif(Sticker, gif(320, 320)).is_some());
        assert!(ExpressionImage::gif(Emoji, png(48, 48)).is_none());
        let mut heavy = gif(48, 48).to_vec();
        heavy.resize(Emoji.max_bytes() + 1, 0);
        assert!(ExpressionImage::gif(Emoji, Bytes::from(heavy)).is_none());
        let image = ExpressionImage::png(Emoji, png(64, 64)).unwrap();
        assert_eq!(format!("{image:?}"), "ExpressionImage(PNG, 29 bytes)");
    }

    #[test]
    fn a_new_expression_says_what_it_lacks() {
        let image = ExpressionImage::png(ExpressionKind::Sticker, png(320, 320)).unwrap();
        let mut sticker = NewExpression {
            kind: ExpressionKind::Sticker,
            name: "Cangrejo".into(),
            image,
            tags: "🦀".into(),
            description: String::new(),
        };
        assert_eq!(sticker.problem(), None);
        sticker.description = "x".into();
        assert!(sticker.problem().is_some());
        sticker.description.clear();
        sticker.tags = " ".into();
        assert!(sticker.problem().is_some());
        let emoji = NewExpression {
            kind: ExpressionKind::Emoji,
            name: "con espacio".into(),
            ..sticker
        };
        assert!(emoji.problem().is_some());
    }
}
