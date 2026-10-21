//! The video track of an MP4 file held in memory: where each of its samples
//! is, when it shows, and the H264 parameters to decode it. Sound is read
//! by Symphonia; this reads only what it skips.
//!
//! Only what a file uploaded whole needs: one `moov` with its sample tables,
//! no fragments. Every count is checked against the bytes that hold it
//! before anything is allocated, and the samples are bounded.

use std::ops::Range;

/// The most samples a track may have: two hours at 60 frames a second.
pub const MAX_SAMPLES: usize = 2 * 60 * 60 * 60;

/// Why a file cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mp4Error {
    /// Not an MP4 file, or a broken one.
    Invalid,
    /// An MP4 file this reader does not handle, such as a fragmented one.
    Unsupported,
}

/// How a track's pictures are coded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Codec {
    /// H264, with what its decoder needs before the first picture.
    Avc(AvcConfig),
    /// Anything else, by its four letters: `hvc1` (H265), `vp09`, `av01`…
    Other(String),
}

/// An `avcC` box: the parameter sets and how long each unit's length is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvcConfig {
    /// Bytes before each unit of a sample: 1, 2 or 4.
    pub length_size: usize,
    pub sps: Vec<Vec<u8>>,
    pub pps: Vec<Vec<u8>>,
}

/// One coded picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    /// Bytes of the file.
    pub offset: usize,
    pub size: usize,
    /// When it is decoded, in the track's time scale.
    pub decode: u64,
    /// When it is shown, in the track's time scale.
    pub show: i64,
    /// How long it lasts.
    pub duration: u32,
    /// A picture the decoder can start at.
    pub sync: bool,
}

/// A track of pictures.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoTrack {
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    /// Units of its times in a second.
    pub timescale: u32,
    /// In decoding order.
    pub samples: Vec<Sample>,
}

impl VideoTrack {
    /// Seconds from its first picture to the end of its last.
    pub fn seconds(&self) -> f64 {
        let end = self
            .samples
            .iter()
            .map(|sample| sample.show + i64::from(sample.duration))
            .max()
            .unwrap_or(0);
        let start = self
            .samples
            .iter()
            .map(|sample| sample.show)
            .min()
            .unwrap_or(0);
        (end - start).max(0) as f64 / f64::from(self.timescale.max(1))
    }

    /// The sample `index` of `file` as the decoder takes it: each unit after
    /// a start code instead of its length, the parameter sets first on a
    /// picture it can start at. `None` for a broken one or another codec.
    pub fn annex_b(&self, file: &[u8], index: usize) -> Option<Vec<u8>> {
        let Codec::Avc(config) = &self.codec else {
            return None;
        };
        let sample = self.samples.get(index)?;
        let mut data = file.get(sample.offset..sample.offset.checked_add(sample.size)?)?;
        let mut out = Vec::with_capacity(sample.size + 64);
        if sample.sync {
            for set in config.sps.iter().chain(&config.pps) {
                out.extend_from_slice(&[0, 0, 0, 1]);
                out.extend_from_slice(set);
            }
        }
        while !data.is_empty() {
            let (length, rest) = data.split_at_checked(config.length_size)?;
            let length = length
                .iter()
                .fold(0usize, |total, byte| total << 8 | usize::from(*byte));
            let (unit, rest) = rest.split_at_checked(length)?;
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(unit);
            data = rest;
        }
        Some(out)
    }
}

/// A box: its four letters and the bytes inside it.
struct Atom<'a> {
    kind: [u8; 4],
    body: &'a [u8],
    /// Where its body starts in the file.
    at: usize,
}

/// The boxes laid one after another in `bytes`, which start at `at` of the file.
fn atoms(bytes: &[u8], at: usize) -> Result<Vec<Atom<'_>>, Mp4Error> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let rest = &bytes[offset..];
        let header = rest.get(..8).ok_or(Mp4Error::Invalid)?;
        let size = u32::from_be_bytes(header[..4].try_into().expect("four bytes")) as u64;
        let kind: [u8; 4] = header[4..8].try_into().expect("four bytes");
        let (size, start) = match size {
            0 => (rest.len() as u64, 8),
            1 => (
                u64::from_be_bytes(
                    rest.get(8..16)
                        .ok_or(Mp4Error::Invalid)?
                        .try_into()
                        .expect("eight"),
                ),
                16,
            ),
            size => (size, 8),
        };
        let size = usize::try_from(size).map_err(|_| Mp4Error::Invalid)?;
        if size < start || size > rest.len() {
            return Err(Mp4Error::Invalid);
        }
        found.push(Atom {
            kind,
            body: &rest[start..size],
            at: at + offset + start,
        });
        offset += size;
    }
    Ok(found)
}

fn child<'a>(atoms: &'a [Atom<'a>], kind: &[u8; 4]) -> Option<&'a Atom<'a>> {
    atoms.iter().find(|atom| &atom.kind == kind)
}

/// Big-endian reads that fail instead of panicking.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Mp4Error> {
        let (taken, rest) = self.bytes.split_at_checked(n).ok_or(Mp4Error::Invalid)?;
        self.bytes = rest;
        Ok(taken)
    }

    fn u8(&mut self) -> Result<u8, Mp4Error> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, Mp4Error> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().expect("two")))
    }

    fn u32(&mut self) -> Result<u32, Mp4Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("four")))
    }

    fn u64(&mut self) -> Result<u64, Mp4Error> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("eight")))
    }

    /// A full box's version, after which its flags are skipped.
    fn version(&mut self) -> Result<u8, Mp4Error> {
        let version = self.u8()?;
        self.take(3)?;
        Ok(version)
    }

    /// A count of entries of `each` bytes that the rest can hold.
    fn count(&mut self, each: usize) -> Result<usize, Mp4Error> {
        let count = self.u32()? as usize;
        if count
            .checked_mul(each)
            .is_none_or(|bytes| bytes > self.bytes.len())
        {
            return Err(Mp4Error::Invalid);
        }
        Ok(count)
    }
}

/// The first track of pictures in `file`, or `None` when it has none, as a
/// file of sound only.
pub fn video_track(file: &[u8]) -> Result<Option<VideoTrack>, Mp4Error> {
    let top = atoms(file, 0)?;
    if child(&top, b"moof").is_some() && child(&top, b"moov").is_none() {
        return Err(Mp4Error::Unsupported);
    }
    let moov = child(&top, b"moov").ok_or(Mp4Error::Invalid)?;
    for trak in atoms(moov.body, moov.at)?
        .iter()
        .filter(|atom| &atom.kind == b"trak")
    {
        let trak = atoms(trak.body, trak.at)?;
        let mdia = child(&trak, b"mdia").ok_or(Mp4Error::Invalid)?;
        let mdia = atoms(mdia.body, mdia.at)?;
        let handler = child(&mdia, b"hdlr").ok_or(Mp4Error::Invalid)?;
        if handler.body.get(8..12) != Some(b"vide") {
            continue;
        }
        let mut header = Reader {
            bytes: child(&mdia, b"mdhd").ok_or(Mp4Error::Invalid)?.body,
        };
        let timescale = if header.version()? == 1 {
            header.take(16)?;
            header.u32()?
        } else {
            header.take(8)?;
            header.u32()?
        };
        if timescale == 0 {
            return Err(Mp4Error::Invalid);
        }
        let minf = child(&mdia, b"minf").ok_or(Mp4Error::Invalid)?;
        let minf = atoms(minf.body, minf.at)?;
        let stbl = child(&minf, b"stbl").ok_or(Mp4Error::Invalid)?;
        return table(&atoms(stbl.body, stbl.at)?, file.len(), timescale).map(Some);
    }
    Ok(None)
}

/// A track's sample table: its codec and every sample, checked against the file.
fn table(stbl: &[Atom<'_>], file: usize, timescale: u32) -> Result<VideoTrack, Mp4Error> {
    let (codec, width, height) = description(child(stbl, b"stsd").ok_or(Mp4Error::Invalid)?)?;
    // Sizes.
    let sizes: Vec<usize> = if let Some(stsz) = child(stbl, b"stsz") {
        let mut read = Reader { bytes: stsz.body };
        read.version()?;
        let fixed = read.u32()? as usize;
        if fixed != 0 {
            let count = read.u32()? as usize;
            if count > MAX_SAMPLES {
                return Err(Mp4Error::Unsupported);
            }
            vec![fixed; count]
        } else {
            let count = read.count(4)?;
            (0..count)
                .map(|_| read.u32().map(|size| size as usize))
                .collect::<Result<_, _>>()?
        }
    } else {
        let stz2 = child(stbl, b"stz2").ok_or(Mp4Error::Invalid)?;
        let mut read = Reader { bytes: stz2.body };
        read.version()?;
        read.take(3)?;
        let bits = read.u8()?;
        let count = read.u32()? as usize;
        let bytes = match bits {
            4 => count.div_ceil(2),
            8 => count,
            16 => count.checked_mul(2).ok_or(Mp4Error::Invalid)?,
            _ => return Err(Mp4Error::Invalid),
        };
        let packed = read.take(bytes)?;
        (0..count)
            .map(|index| match bits {
                4 => usize::from(packed[index / 2] >> (4 * (1 - index % 2)) & 0xF),
                8 => usize::from(packed[index]),
                _ => usize::from(u16::from_be_bytes([
                    packed[2 * index],
                    packed[2 * index + 1],
                ])),
            })
            .collect()
    };
    if sizes.len() > MAX_SAMPLES {
        return Err(Mp4Error::Unsupported);
    }
    // Chunk offsets.
    let offsets: Vec<u64> = if let Some(stco) = child(stbl, b"stco") {
        let mut read = Reader { bytes: stco.body };
        read.version()?;
        let count = read.count(4)?;
        (0..count)
            .map(|_| read.u32().map(u64::from))
            .collect::<Result<_, _>>()?
    } else {
        let co64 = child(stbl, b"co64").ok_or(Mp4Error::Invalid)?;
        let mut read = Reader { bytes: co64.body };
        read.version()?;
        let count = read.count(8)?;
        (0..count).map(|_| read.u64()).collect::<Result<_, _>>()?
    };
    // Samples per chunk, as runs from a first chunk (counted from 1).
    let runs: Vec<(u32, u32)> = {
        let mut read = Reader {
            bytes: child(stbl, b"stsc").ok_or(Mp4Error::Invalid)?.body,
        };
        read.version()?;
        let count = read.count(12)?;
        (0..count)
            .map(|_| {
                let first = read.u32()?;
                let each = read.u32()?;
                read.u32()?;
                Ok((first, each))
            })
            .collect::<Result<_, Mp4Error>>()?
    };
    // Where each sample is.
    let mut places = Vec::with_capacity(sizes.len());
    for (index, run) in runs.iter().enumerate() {
        let last = runs.get(index + 1).map_or(offsets.len() as u64, |next| {
            u64::from(next.0).saturating_sub(1)
        });
        if run.0 == 0 || u64::from(run.0) > last + 1 {
            return Err(Mp4Error::Invalid);
        }
        for chunk in u64::from(run.0)..=last {
            let mut offset = *offsets.get(chunk as usize - 1).ok_or(Mp4Error::Invalid)?;
            for _ in 0..run.1 {
                let Some(size) = sizes.get(places.len()) else {
                    break;
                };
                let end = offset.checked_add(*size as u64).ok_or(Mp4Error::Invalid)?;
                if end > file as u64 {
                    return Err(Mp4Error::Invalid);
                }
                places.push((offset as usize, *size));
                offset = end;
            }
        }
    }
    if places.len() != sizes.len() || sizes.is_empty() {
        return Err(Mp4Error::Invalid);
    }
    // When each is decoded.
    let mut durations = Vec::with_capacity(sizes.len());
    {
        let mut read = Reader {
            bytes: child(stbl, b"stts").ok_or(Mp4Error::Invalid)?.body,
        };
        read.version()?;
        for _ in 0..read.count(8)? {
            let count = read.u32()? as usize;
            let delta = read.u32()?;
            if durations.len() + count > sizes.len() {
                return Err(Mp4Error::Invalid);
            }
            durations.extend(std::iter::repeat_n(delta, count));
        }
    }
    // Later samples without a duration last as long as the last one given.
    let last = durations.last().copied().unwrap_or(0);
    durations.resize(sizes.len(), last);
    // How much later each is shown than decoded.
    let mut shifts: Vec<i64> = Vec::new();
    if let Some(ctts) = child(stbl, b"ctts") {
        let mut read = Reader { bytes: ctts.body };
        let signed = read.version()? == 1;
        for _ in 0..read.count(8)? {
            let count = read.u32()? as usize;
            let raw = read.u32()?;
            let shift = if signed {
                i64::from(raw as i32)
            } else {
                i64::from(raw)
            };
            if shifts.len() + count > sizes.len() {
                return Err(Mp4Error::Invalid);
            }
            shifts.extend(std::iter::repeat_n(shift, count));
        }
    }
    shifts.resize(sizes.len(), 0);
    // Pictures it can start at; without the table, every one.
    let syncs: Option<Vec<u32>> = match child(stbl, b"stss") {
        Some(stss) => {
            let mut read = Reader { bytes: stss.body };
            read.version()?;
            let count = read.count(4)?;
            Some((0..count).map(|_| read.u32()).collect::<Result<_, _>>()?)
        }
        None => None,
    };
    let mut decode = 0u64;
    let samples = places
        .into_iter()
        .enumerate()
        .map(|(index, (offset, size))| {
            let sample = Sample {
                offset,
                size,
                decode,
                show: decode as i64 + shifts[index],
                duration: durations[index],
                sync: syncs
                    .as_ref()
                    .is_none_or(|syncs| syncs.binary_search(&(index as u32 + 1)).is_ok()),
            };
            decode += u64::from(durations[index]);
            sample
        })
        .collect();
    Ok(VideoTrack {
        codec,
        width,
        height,
        timescale,
        samples,
    })
}

/// The first sample description: its codec and picture size.
fn description(stsd: &Atom<'_>) -> Result<(Codec, u32, u32), Mp4Error> {
    let mut read = Reader { bytes: stsd.body };
    read.version()?;
    if read.u32()? == 0 {
        return Err(Mp4Error::Invalid);
    }
    let at = stsd.at + 8;
    let entry = atoms(read.bytes, at)?
        .into_iter()
        .next()
        .ok_or(Mp4Error::Invalid)?;
    let mut visual = Reader { bytes: entry.body };
    // Reserved and reference (8), predefined and reserved (16).
    visual.take(24)?;
    let width = u32::from(visual.u16()?);
    let height = u32::from(visual.u16()?);
    // Resolutions, reserved, frame count, compressor name, depth, predefined.
    visual.take(50)?;
    let kind = String::from_utf8_lossy(&entry.kind).into_owned();
    if !matches!(&entry.kind, b"avc1" | b"avc3") {
        return Ok((Codec::Other(kind), width, height));
    }
    let children = atoms(visual.bytes, entry.at + 78)?;
    let avcc = child(&children, b"avcC").ok_or(Mp4Error::Invalid)?;
    Ok((Codec::Avc(avc_config(avcc.body)?), width, height))
}

fn avc_config(body: &[u8]) -> Result<AvcConfig, Mp4Error> {
    let mut read = Reader { bytes: body };
    if read.u8()? != 1 {
        return Err(Mp4Error::Invalid);
    }
    read.take(3)?;
    let length_size = usize::from(read.u8()? & 0b11) + 1;
    if length_size == 3 {
        return Err(Mp4Error::Invalid);
    }
    let count = usize::from(read.u8()? & 0b1_1111);
    let sps = sets(&mut read, count)?;
    let count = usize::from(read.u8()?);
    let pps = sets(&mut read, count)?;
    Ok(AvcConfig {
        length_size,
        sps,
        pps,
    })
}

/// `count` parameter sets, each after its two-byte length.
fn sets(read: &mut Reader<'_>, count: usize) -> Result<Vec<Vec<u8>>, Mp4Error> {
    (0..count)
        .map(|_| {
            let length = usize::from(read.u16()?);
            read.take(length).map(<[u8]>::to_vec)
        })
        .collect()
}

/// The samples to decode to show the picture at `show` (in the track's
/// scale): from the last one it can start at before it, to it.
pub fn from_sync(track: &VideoTrack, show: i64) -> Range<usize> {
    let target = track
        .samples
        .iter()
        .rposition(|sample| sample.show <= show)
        .unwrap_or(0);
    let start = track.samples[..=target]
        .iter()
        .rposition(|sample| sample.sync)
        .unwrap_or(0);
    start..track.samples.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn full(kind: &[u8; 4], version: u8, body: &[u8]) -> Vec<u8> {
        let mut inner = vec![version, 0, 0, 0];
        inner.extend_from_slice(body);
        atom(kind, &inner)
    }

    fn words(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }

    /// A file with `samples` pictures of two bytes each, one unit apiece,
    /// two to a chunk, the first a key frame, and B-frame shifts.
    fn movie(samples: usize, codec: &[u8; 4]) -> Vec<u8> {
        let mdat_body: Vec<u8> = (0..samples)
            .flat_map(|index| [0, 0, 0, 2, 0x65, index as u8])
            .collect();
        let mdat_at = 8 + 8 + 8; // ftyp (8 body) + mdat header
        let ftyp = atom(b"ftyp", b"isom\0\0\0\0");
        let chunks = samples.div_ceil(2);
        let offsets: Vec<u32> = (0..chunks)
            .map(|chunk| (mdat_at + chunk * 12) as u32)
            .collect();
        let mut avcc = vec![1, 66, 0, 30, 0xFF, 0xE1];
        avcc.extend_from_slice(&[0, 3, 0x67, 1, 2]);
        avcc.extend_from_slice(&[1, 0, 2, 0x68, 3]);
        let mut entry = vec![0; 78];
        entry[24..26].copy_from_slice(&64u16.to_be_bytes());
        entry[26..28].copy_from_slice(&48u16.to_be_bytes());
        entry.extend(atom(b"avcC", &avcc));
        let mut stsd_body = words(&[1]);
        stsd_body.extend(atom(codec, &entry));
        let mut stbl = full(b"stsd", 0, &stsd_body);
        stbl.extend(full(b"stts", 0, &words(&[1, samples as u32, 100])));
        stbl.extend(full(
            b"ctts",
            0,
            &words(&[2, 1, 0, samples as u32 - 1, 200]),
        ));
        stbl.extend(full(b"stsc", 0, &words(&[1, 1, 2, 1])));
        let mut stsz = words(&[0, samples as u32]);
        stsz.extend(words(&vec![6; samples]));
        stbl.extend(full(b"stsz", 0, &stsz));
        let mut stco = words(&[chunks as u32]);
        stco.extend(words(&offsets));
        stbl.extend(full(b"stco", 0, &stco));
        stbl.extend(full(b"stss", 0, &words(&[1, 1])));
        let minf = atom(b"minf", &atom(b"stbl", &stbl));
        let mut mdia = full(b"mdhd", 0, &words(&[0, 0, 1000, 0, 0]));
        mdia.extend(full(b"hdlr", 0, b"\0\0\0\0vide\0\0\0\0\0\0\0\0\0\0\0\0\0"));
        mdia.extend(minf);
        let trak = atom(b"trak", &atom(b"mdia", &mdia));
        let mut file = ftyp;
        file.extend(atom(b"mdat", &mdat_body));
        file.extend(atom(b"moov", &trak));
        file
    }

    #[test]
    fn a_track_yields_its_samples_times_and_annex_b_units() {
        let file = movie(5, b"avc1");
        let track = video_track(&file).unwrap().unwrap();
        assert_eq!((track.width, track.height, track.timescale), (64, 48, 1000));
        assert_eq!(track.samples.len(), 5);
        let Codec::Avc(config) = &track.codec else {
            panic!("H264")
        };
        assert_eq!((config.sps.len(), config.pps.len()), (1, 1));
        assert_eq!(config.length_size, 4);
        assert!(track.samples[0].sync && !track.samples[1].sync);
        assert_eq!(track.samples[1].decode, 100);
        assert_eq!((track.samples[0].show, track.samples[1].show), (0, 300));
        // Two to a chunk: the third starts the second chunk.
        assert_eq!(track.samples[2].offset, track.samples[1].offset + 6);
        let first = track.annex_b(&file, 0).unwrap();
        assert_eq!(
            first,
            [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 0]
        );
        assert_eq!(track.annex_b(&file, 3).unwrap(), [0, 0, 0, 1, 0x65, 3]);
        assert_eq!(from_sync(&track, 650), 0..5);
        assert!((track.seconds() - 0.7).abs() < 1e-9, "{}", track.seconds());
    }

    #[test]
    fn other_codecs_fragments_and_broken_files_are_told_apart() {
        let track = video_track(&movie(2, b"hvc1")).unwrap().unwrap();
        assert_eq!(track.codec, Codec::Other("hvc1".into()));
        assert!(track.annex_b(&movie(2, b"hvc1"), 0).is_none());
        assert_eq!(video_track(b""), Err(Mp4Error::Invalid));
        let fragmented = [atom(b"ftyp", b"isom"), atom(b"moof", b"")].concat();
        assert_eq!(video_track(&fragmented), Err(Mp4Error::Unsupported));
        let file = movie(4, b"avc1");
        for cut in [10, file.len() / 2, file.len() - 3] {
            assert!(video_track(&file[..cut]).is_err(), "cut at {cut}");
        }
        // A count larger than its box is refused before any allocation.
        let mut lying = file.clone();
        let stsz = lying
            .windows(4)
            .position(|window| window == b"stsz")
            .unwrap();
        lying[stsz + 12..stsz + 16].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(video_track(&lying).is_err());
    }
}
