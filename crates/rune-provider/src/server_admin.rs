//! What the rest of a server's administration (P7) carries that must be
//! checked before it travels: a role's new picture or emoji.

use rune_i18n::t;

use crate::ExpressionImage;

/// What a role shows beside names: a picture made on this device (as an
/// emoji's, never animated) or a Unicode emoji.
#[derive(Clone, Debug, PartialEq)]
pub enum RoleIcon {
    Image(ExpressionImage),
    Emoji(String),
}

impl RoleIcon {
    /// Why it cannot be sent as it is, in the interface's words.
    pub fn problem(&self) -> Option<&'static str> {
        match self {
            Self::Image(image) if image.is_gif() => Some(t(
                "El ícono de un rol va sin animación: elegí un PNG o un JPG.",
            )),
            Self::Emoji(emoji)
                if !(1..=8).contains(&emoji.chars().count())
                    || emoji.chars().any(|c| c.is_alphanumeric() || c.is_control()) =>
            {
                Some(t("Elegí un solo emoji."))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::ExpressionKind;

    #[test]
    fn a_role_icon_is_a_still_picture_or_one_emoji() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(64u32.to_be_bytes());
        png.extend(64u32.to_be_bytes());
        let still = ExpressionImage::png(ExpressionKind::Emoji, Bytes::from(png)).unwrap();
        assert_eq!(RoleIcon::Image(still).problem(), None);
        let mut gif = b"GIF89a".to_vec();
        gif.extend([64, 0, 64, 0]);
        let moving = ExpressionImage::gif(ExpressionKind::Emoji, Bytes::from(gif)).unwrap();
        assert!(RoleIcon::Image(moving).problem().is_some());
        assert_eq!(RoleIcon::Emoji("🦀".into()).problem(), None);
        assert_eq!(RoleIcon::Emoji("👩‍💻".into()).problem(), None);
        for wrong in ["", "rust", "🦀🦀🦀🦀🦀🦀🦀🦀🦀"] {
            assert!(RoleIcon::Emoji(wrong.into()).problem().is_some(), "{wrong}");
        }
    }
}
