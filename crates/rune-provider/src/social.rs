//! What the commands about people and conversations outside servers carry
//! that must be checked before it travels.

use bytes::Bytes;

/// A group's new picture: a PNG made on this device, at most
/// [`GroupIcon::MAX_BYTES`] long and [`GroupIcon::MAX_EDGE`] pixels a side.
/// Its bytes never show in `Debug`.
#[derive(Clone, PartialEq)]
pub struct GroupIcon(Bytes);

impl GroupIcon {
    pub const MAX_BYTES: usize = 256 * 1024;
    pub const MAX_EDGE: u32 = 1024;

    /// `None` unless `png` starts as a PNG of a sensible size does.
    pub fn new(png: Bytes) -> Option<Self> {
        bounded_png(&png, Self::MAX_BYTES, Self::MAX_EDGE).then_some(Self(png))
    }

    pub fn png(&self) -> &[u8] {
        &self.0
    }
}

/// Whether `png` starts as a PNG of at most `max_bytes` and `max_edge`
/// pixels a side does.
pub(crate) fn bounded_png(png: &[u8], max_bytes: usize, max_edge: u32) -> bool {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    let edge = |at: usize| {
        png.get(at..at + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_be_bytes)
            .filter(|edge| (1..=max_edge).contains(edge))
    };
    png.len() <= max_bytes && png.starts_with(SIGNATURE) && edge(16).is_some() && edge(20).is_some()
}

impl std::fmt::Debug for GroupIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GroupIcon({} bytes)", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn only_a_bounded_png_passes() {
        assert!(GroupIcon::new(Bytes::from(png(128, 128))).is_some());
        assert!(GroupIcon::new(Bytes::from(png(0, 128))).is_none());
        assert!(GroupIcon::new(Bytes::from(png(128, 4096))).is_none());
        assert!(GroupIcon::new(Bytes::from_static(b"GIF89a")).is_none());
        let mut huge = png(64, 64);
        huge.resize(GroupIcon::MAX_BYTES + 1, 0);
        assert!(GroupIcon::new(Bytes::from(huge)).is_none());
        let icon = GroupIcon::new(Bytes::from(png(64, 64))).unwrap();
        assert_eq!(format!("{icon:?}"), "GroupIcon(29 bytes)");
    }
}
