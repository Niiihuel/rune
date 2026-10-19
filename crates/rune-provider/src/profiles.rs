//! What the commands that change the account's profile carry: pictures made
//! on this device, and the changes to its profile in one server.

use bytes::Bytes;

use crate::social::bounded_png;

/// Which picture of a profile: the round one, or the wide one behind it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProfileImageKind {
    Avatar,
    Banner,
    // Server banner (2026-10-18).
    /// A server's banner, over its channels: 16:9, as services take it.
    ServerBanner,
}

impl ProfileImageKind {
    /// The size it is made at, wide first: a square avatar, a 5:2 banner.
    pub fn sizes(self) -> &'static [(u32, u32)] {
        match self {
            Self::Avatar => &[(512, 512), (256, 256), (128, 128)],
            Self::Banner => &[(960, 384), (680, 272), (480, 192)],
            Self::ServerBanner => &[(960, 540), (640, 360), (480, 270)],
        }
    }
}

/// A new picture for a profile, made on this device: a PNG of at most
/// [`ProfileImage::MAX_BYTES`], or a moving GIF of at most
/// [`ProfileImage::MAX_ANIMATED_BYTES`], [`ProfileImage::MAX_EDGE`] pixels a
/// side. Its bytes never show in `Debug`.
#[derive(Clone, PartialEq)]
pub struct ProfileImage(Bytes);

impl ProfileImage {
    pub const MAX_BYTES: usize = 1024 * 1024;
    pub const MAX_EDGE: u32 = 1024;
    /// As large as a file the official client lets one choose for an avatar.
    pub const MAX_ANIMATED_BYTES: usize = 8 * 1024 * 1024;

    /// `None` unless `bytes` start as a PNG or a GIF within those bounds do.
    pub fn new(bytes: Bytes) -> Option<Self> {
        (bounded_png(&bytes, Self::MAX_BYTES, Self::MAX_EDGE) || bounded_gif(&bytes))
            .then_some(Self(bytes))
    }

    /// The file: a PNG or, for a moving picture, a GIF.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// A GIF: the services count it as a moving picture.
    pub fn animated(&self) -> bool {
        self.0.starts_with(b"GIF8")
    }

    /// Its media type, as a data URI names it.
    pub fn mime(&self) -> &'static str {
        if self.animated() {
            "image/gif"
        } else {
            "image/png"
        }
    }
}

/// Whether `gif` starts as a GIF of at most [`ProfileImage::MAX_EDGE`]
/// pixels a side and [`ProfileImage::MAX_ANIMATED_BYTES`] does.
fn bounded_gif(gif: &[u8]) -> bool {
    let edge = |at: usize| {
        gif.get(at..at + 2)
            .map(|bytes| u32::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            .is_some_and(|edge| (1..=ProfileImage::MAX_EDGE).contains(&edge))
    };
    gif.len() <= ProfileImage::MAX_ANIMATED_BYTES
        && (gif.starts_with(b"GIF87a") || gif.starts_with(b"GIF89a"))
        && edge(6)
        && edge(8)
}

/// Which part of a chosen picture becomes a profile's: where the frame's
/// middle falls, as shares of the picture's width and height, and how much
/// closer than the largest frame that fits it is, from 1 to
/// [`Crop::MAX_ZOOM`]. The default is the largest frame, in the middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            zoom: 1.0,
        }
    }
}

impl Crop {
    pub const MAX_ZOOM: f32 = 4.0;

    /// The frame in a `width` × `height` picture, as `kind` shapes it:
    /// left, top, width and height, in the picture's units. It never leaves
    /// the picture; shares or zoom that are not finite count as the default.
    pub fn frame(self, width: f32, height: f32, kind: ProfileImageKind) -> [f32; 4] {
        let (width, height) = (width.max(1.0), height.max(1.0));
        let (wide, high) = kind.sizes()[0];
        let aspect = wide as f32 / high as f32;
        let or = |value: f32, default: f32| if value.is_finite() { value } else { default };
        let zoom = or(self.zoom, 1.0).clamp(1.0, Self::MAX_ZOOM);
        let (fit_width, fit_height) = if width / height > aspect {
            (height * aspect, height)
        } else {
            (width, width / aspect)
        };
        let (frame_width, frame_height) = (fit_width / zoom, fit_height / zoom);
        let left = (or(self.x, 0.5) * width - frame_width / 2.0).clamp(0.0, width - frame_width);
        let top = (or(self.y, 0.5) * height - frame_height / 2.0).clamp(0.0, height - frame_height);
        [left, top, frame_width, frame_height]
    }
}

impl std::fmt::Debug for ProfileImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProfileImage({} bytes)", self.0.len())
    }
}

/// Changes to the account's profile in one server; `None` leaves a field as
/// it is, `Some(None)` takes it away so the account's own shows instead.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerProfileEdit {
    /// The name it goes by there.
    pub nick: Option<Option<String>>,
    pub about: Option<Option<String>>,
    pub avatar: Option<Option<ProfileImage>>,
    pub banner: Option<Option<ProfileImage>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes
    }

    #[test]
    fn a_profile_picture_is_a_bounded_png_whose_bytes_never_print() {
        let image = ProfileImage::new(Bytes::from(png(960, 384))).unwrap();
        assert_eq!(format!("{image:?}"), "ProfileImage(24 bytes)");
        assert!(!image.animated() && image.mime() == "image/png");
        assert!(ProfileImage::new(Bytes::from(png(2048, 512))).is_none());
        let mut huge = png(64, 64);
        huge.resize(ProfileImage::MAX_BYTES + 1, 0);
        assert!(ProfileImage::new(Bytes::from(huge)).is_none());
        assert!(ProfileImage::new(Bytes::from_static(b"\xff\xd8\xff")).is_none());
        for kind in [
            ProfileImageKind::Avatar,
            ProfileImageKind::Banner,
            ProfileImageKind::ServerBanner,
        ] {
            assert!(kind
                .sizes()
                .iter()
                .all(|(w, h)| *w <= ProfileImage::MAX_EDGE && *h <= ProfileImage::MAX_EDGE));
        }
    }

    #[test]
    fn a_moving_picture_is_a_bounded_gif() {
        let gif = |width: u16, height: u16| {
            let mut bytes = b"GIF89a".to_vec();
            bytes.extend(width.to_le_bytes());
            bytes.extend(height.to_le_bytes());
            bytes
        };
        let image = ProfileImage::new(Bytes::from(gif(512, 512))).unwrap();
        assert!(image.animated() && image.mime() == "image/gif");
        assert!(ProfileImage::new(Bytes::from(gif(2048, 512))).is_none());
        assert!(ProfileImage::new(Bytes::from(gif(0, 512))).is_none());
        let mut huge = gif(64, 64);
        huge.resize(ProfileImage::MAX_ANIMATED_BYTES + 1, 0);
        assert!(ProfileImage::new(Bytes::from(huge)).is_none());
        // A PNG keeps its own, smaller bound.
        let mut big_png = png(64, 64);
        big_png.resize(ProfileImage::MAX_BYTES + 1, 0);
        assert!(ProfileImage::new(Bytes::from(big_png)).is_none());
    }

    #[test]
    fn a_frame_keeps_its_shape_inside_the_picture() {
        let avatar = ProfileImageKind::Avatar;
        // The largest square, in the middle, by default.
        assert_eq!(
            Crop::default().frame(400.0, 200.0, avatar),
            [100.0, 0.0, 200.0, 200.0]
        );
        // Moved to a corner and twice as close, it stops at the edge.
        let near = Crop {
            x: 0.0,
            y: 1.0,
            zoom: 2.0,
        };
        assert_eq!(near.frame(400.0, 200.0, avatar), [0.0, 100.0, 100.0, 100.0]);
        // A banner is 5:2: in a tall picture, as wide as it.
        let banner = Crop::default().frame(100.0, 300.0, ProfileImageKind::Banner);
        assert_eq!(banner, [0.0, 130.0, 100.0, 40.0]);
        // A server's is 16:9.
        let server = Crop::default().frame(160.0, 300.0, ProfileImageKind::ServerBanner);
        assert_eq!(server, [0.0, 105.0, 160.0, 90.0]);
        // Nonsense counts as the default; zoom stays within its bounds.
        let odd = Crop {
            x: f32::NAN,
            y: f32::INFINITY,
            zoom: 50.0,
        };
        assert_eq!(
            odd.frame(400.0, 400.0, avatar),
            [150.0, 150.0, 100.0, 100.0]
        );
    }
}
