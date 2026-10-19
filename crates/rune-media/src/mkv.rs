//! The video track of a Matroska or WebM file held in memory, as its header
//! names it: how its pictures are coded, how large they are and how long
//! the file lasts.
//! GStreamer reads and decodes the pictures themselves; Symphonia, the sound.
//!
//! Only the header is walked, up to the first cluster: every size is checked
//! against the bytes that hold it, and nothing is allocated from one.

/// The four bytes every Matroska and WebM file starts with.
pub const MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];

const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
const DURATION: u32 = 0x4489;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_TYPE: u32 = 0x83;
const CODEC_ID: u32 = 0x86;
const VIDEO_SETTINGS: u32 = 0xE0;
const PIXEL_WIDTH: u32 = 0xB0;
const PIXEL_HEIGHT: u32 = 0xBA;
const CLUSTER: u32 = 0x1F43_B675;
/// A track of pictures, as `TrackType` says it.
const VIDEO: u64 = 1;

/// A track of pictures.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoTrack {
    /// Its Matroska codec name: `V_VP8`, `V_VP9`, `V_AV1`, `V_MPEG4/ISO/AVC`…
    pub codec: String,
    /// How long the file says it lasts, when it says.
    pub seconds: Option<f64>,
    /// How wide and tall its pictures are, when it says.
    pub size: Option<(u64, u64)>,
}

/// One element: its id, and where its body is.
struct Element {
    id: u32,
    body: std::ops::Range<usize>,
}

/// A variable-length number at `at`, its length marker kept (an id) or
/// taken off (a size): its value and how many bytes it took.
fn vint(file: &[u8], at: usize, id: bool) -> Option<(u64, usize)> {
    let first = *file.get(at)?;
    let length = first.leading_zeros() as usize + 1;
    if length > if id { 4 } else { 8 } {
        return None;
    }
    let mut value = if id {
        u64::from(first)
    } else {
        u64::from(first) & (0xFF_u64 >> length)
    };
    for byte in file.get(at + 1..at + length)? {
        value = value << 8 | u64::from(*byte);
    }
    Some((value, length))
}

/// The element at `at`, its body cut at `end`: a size the file does not
/// say (all ones: a live recording's) or one past `end` reaches `end`.
fn element(file: &[u8], at: usize, end: usize) -> Option<Element> {
    let (id, id_length) = vint(file, at, true)?;
    let (size, size_length) = vint(file, at + id_length, false)?;
    let start = at + id_length + size_length;
    let unknown = size == (1 << (7 * size_length)) - 1;
    let stop = match usize::try_from(size)
        .ok()
        .and_then(|size| start.checked_add(size))
    {
        Some(stop) if !unknown => stop.min(end),
        _ => end,
    };
    (start <= stop).then_some(Element {
        id: id as u32,
        body: start..stop,
    })
}

/// The elements of a body, in order, until one cannot be read.
fn children(file: &[u8], body: std::ops::Range<usize>) -> impl Iterator<Item = Element> + '_ {
    let end = body.end;
    let mut at = body.start;
    std::iter::from_fn(move || {
        let child = element(file, at, end)?;
        at = child.body.end;
        Some(child)
    })
}

fn unsigned(bytes: &[u8]) -> Option<u64> {
    (bytes.len() <= 8).then(|| {
        bytes
            .iter()
            .fold(0, |value, byte| value << 8 | u64::from(*byte))
    })
}

fn float(bytes: &[u8]) -> Option<f64> {
    match bytes.len() {
        4 => Some(f64::from(f32::from_be_bytes(bytes.try_into().ok()?))),
        8 => Some(f64::from_be_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

/// The first track of pictures of `file`; `None` when it has none, as a
/// WebM of sound alone. `Err` when it is not a Matroska file.
pub fn video_track(file: &[u8]) -> Result<Option<VideoTrack>, ()> {
    if !file.starts_with(&MAGIC) {
        return Err(());
    }
    let header = element(file, 0, file.len()).ok_or(())?;
    let segment = element(file, header.body.end, file.len())
        .filter(|segment| segment.id == SEGMENT)
        .ok_or(())?;
    let (mut scale, mut duration, mut track) = (1_000_000_u64, None, None);
    for child in children(file, segment.body) {
        let body = |element: &Element| &file[element.body.clone()];
        match child.id {
            INFO => {
                for field in children(file, child.body) {
                    match field.id {
                        TIMESTAMP_SCALE => scale = unsigned(body(&field)).ok_or(())?,
                        DURATION => duration = float(body(&field)),
                        _ => {}
                    }
                }
            }
            TRACKS if track.is_none() => {
                track = children(file, child.body)
                    .filter(|entry| entry.id == TRACK_ENTRY)
                    .find_map(|entry| {
                        let fields: Vec<Element> = children(file, entry.body).collect();
                        let field = |id| fields.iter().find(|field| field.id == id);
                        if field(TRACK_TYPE).map(body).and_then(unsigned) != Some(VIDEO) {
                            return None;
                        }
                        let codec = String::from_utf8_lossy(body(field(CODEC_ID)?))
                            .trim_end_matches('\0')
                            .to_owned();
                        let video: Vec<Element> = field(VIDEO_SETTINGS)
                            .map_or_else(Vec::new, |video| {
                                children(file, video.body.clone()).collect()
                            });
                        let pixels = |id| {
                            let field = video.iter().find(|field| field.id == id)?;
                            unsigned(body(field))
                        };
                        Some((codec, pixels(PIXEL_WIDTH).zip(pixels(PIXEL_HEIGHT))))
                    });
            }
            // The header is over.
            CLUSTER => break,
            _ => {}
        }
    }
    // Ticks of `scale` nanoseconds; a duration past any sense is not believed.
    let seconds = duration
        .map(|ticks| ticks * scale as f64 / 1e9)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0 && *seconds < 1e7);
    Ok(track.map(|(codec, size)| VideoTrack {
        codec,
        seconds,
        size,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).expect("fixture")
    }

    #[test]
    fn each_file_names_its_pictures_codec_and_length() {
        for (name, codec, seconds, size) in [
            ("clip-vp8.webm", "V_VP8", 2.0, (160, 90)),
            ("clip-vp9.webm", "V_VP9", 1.0, (160, 90)),
            ("clip-av1.mkv", "V_AV1", 1.0, (160, 90)),
            ("clip-h264.mkv", "V_MPEG4/ISO/AVC", 1.0, (160, 90)),
            ("hevc.mkv", "V_MPEGH/ISO/HEVC", 0.4, (64, 36)),
        ] {
            let track = video_track(&fixture(name)).unwrap().expect(name);
            assert_eq!(track.codec, codec, "{name}");
            assert_eq!(track.size, Some(size), "{name}");
            let said = track.seconds.expect(name);
            assert!((said - seconds).abs() < 0.1, "{name}: {said} s");
        }
    }

    #[test]
    fn sound_alone_has_no_pictures_and_strangers_are_refused() {
        assert_eq!(video_track(&fixture("tone.webm")), Ok(None));
        assert!(video_track(&fixture("clip.mp4")).is_err());
        assert!(video_track(b"").is_err());
        // Cut anywhere, a file is read as far as it goes, never past it.
        let file = fixture("clip-vp9.webm");
        for end in 0..file.len().min(600) {
            let _ = video_track(&file[..end]);
        }
        // A size that claims more than there is reaches the end, no further.
        let mut huge = file.clone();
        huge[4] = 0x01;
        let _ = video_track(&huge);
    }
}
