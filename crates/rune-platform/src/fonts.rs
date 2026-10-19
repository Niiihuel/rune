//! The system's font families, and the faces of one for interface text,
//! through fontconfig's own tools (`fc-list`, `fc-match`), which every
//! Linux desktop has. Nothing is copied or kept: RUNE remembers only the
//! family's name and loads its files again at the next start.
//!
//! Everything here runs programs and reads files: call it off the
//! interface's thread.
//!
//! Windows has no fontconfig: there DirectWrite lists the families and
//! says which files hold a family's faces ([`windows`]).

#[cfg(not(windows))]
use std::process::Command;

// Windows (windows-medios).
#[cfg(windows)]
mod windows;

/// Families listed at most, and the largest face file read.
const MAX_FAMILIES: usize = 4096;
const MAX_FACE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_NAME: usize = 128;

/// One face of a font file: its bytes and its index in a collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    pub bytes: Vec<u8>,
    pub index: u32,
}

/// The weights interface text uses; a weight the family lacks is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Faces {
    pub regular: Face,
    pub medium: Option<Face>,
    /// `None` too when fontconfig answers with the medium or bold face:
    /// limn then draws semibold with the nearest one itself.
    pub semibold: Option<Face>,
    pub bold: Option<Face>,
}

/// The installed families, sorted, without hidden ones (named with a dot).
pub fn families() -> Vec<String> {
    #[cfg(windows)]
    let families = self::windows::families();
    #[cfg(not(windows))]
    let families = Command::new("fc-list")
        .args([":", "family"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_families(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default();
    families
}

#[cfg_attr(windows, allow(dead_code))]
fn parse_families(listed: &str) -> Vec<String> {
    let mut families: Vec<String> = listed
        .lines()
        // A line names one face's families; its first is the usual name.
        .filter_map(|line| line.split(',').next())
        .map(str::trim)
        .filter(|family| valid(family) && !family.starts_with('.'))
        .map(str::to_owned)
        .collect();
    families.sort_by_key(|family| family.to_lowercase());
    families.dedup();
    families.truncate(MAX_FAMILIES);
    families
}

fn valid(family: &str) -> bool {
    !family.is_empty() && family.len() <= MAX_NAME && !family.chars().any(char::is_control)
}

/// The regular, medium, semibold and bold faces of `family`; `None` when it
/// is not installed (fontconfig would answer with another family).
pub fn load(family: &str) -> Option<Faces> {
    if !valid(family) {
        return None;
    }
    let [regular, medium, semibold, bold] = files(family)?;
    let regular = regular?;
    let other = |found: Option<(String, u32)>| found.filter(|found| *found != regular);
    let medium = other(medium);
    let bold = other(bold);
    // Not the same file twice: a family without semibold answers with bold.
    let semibold = other(semibold)
        .filter(|found| Some(found) != medium.as_ref() && Some(found) != bold.as_ref());
    Some(Faces {
        regular: read(&regular)?,
        medium: medium.as_ref().and_then(read),
        semibold: semibold.as_ref().and_then(read),
        bold: bold.as_ref().and_then(read),
    })
}

/// The files of `family`'s regular, medium, semibold and bold faces.
#[cfg(not(windows))]
fn files(family: &str) -> Option<[Option<(String, u32)>; 4]> {
    let regular = face(family, 80)?;
    Some([
        Some(regular),
        face(family, 100),
        face(family, 180),
        face(family, 200),
    ])
}

#[cfg(windows)]
use self::windows::faces as files;

/// The file and index fontconfig picks for `family` at `weight` (80
/// regular, 100 medium, 180 semibold, 200 bold), upright and of normal width.
#[cfg(not(windows))]
fn face(family: &str, weight: u32) -> Option<(String, u32)> {
    let pattern = format!("{}:weight={weight}:slant=0:width=100", escape(family));
    let output = Command::new("fc-match")
        .args(["-f", "%{file}\\n%{index}\\n%{family}\\n%{weight}", &pattern])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    matched(&String::from_utf8_lossy(&output.stdout), family, weight)
}

/// What `fc-match` printed, if it is `family` near `weight`.
#[cfg_attr(windows, allow(dead_code))]
fn matched(printed: &str, family: &str, weight: u32) -> Option<(String, u32)> {
    let mut lines = printed.lines();
    let file = lines.next()?.to_owned();
    let index = lines.next()?.trim().parse().ok()?;
    let families = lines.next()?;
    let found = lines.next()?.trim();
    let same = families
        .split(',')
        .any(|name| name.trim().eq_ignore_ascii_case(family));
    // A family without the weight answers with its nearest one; a variable
    // font, with the range it covers.
    let near = match found
        .strip_prefix('[')
        .and_then(|range| range.strip_suffix(']'))
    {
        Some(range) => {
            let mut ends = range
                .split_whitespace()
                .filter_map(|end| end.parse::<f32>().ok());
            let (low, high) = (ends.next()?, ends.next()?);
            (low..=high).contains(&(weight as f32))
        }
        None => weight.abs_diff(found.parse::<f32>().ok()?.round() as u32) <= 30,
    };
    (same && near && file.starts_with('/')).then_some((file, index))
}

/// Characters fontconfig's pattern syntax gives meaning to.
#[cfg_attr(windows, allow(dead_code))]
fn escape(family: &str) -> String {
    let mut escaped = String::with_capacity(family.len());
    for character in family.chars() {
        if matches!(character, '\\' | '-' | ':' | ',' | '=') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// The font files fontconfig would fall back to for `character`, best
/// first (`fc-match -s`): what a system font may draw where RUNE's own
/// lack a glyph, as a browser falls back (2026-10-17: names and tags with
/// such characters showed boxes). Only OpenType files, at most `limit`.
pub fn fallbacks(character: char, limit: usize) -> Vec<(String, u32)> {
    #[cfg(windows)]
    let found = self::windows::fallbacks(character, limit);
    #[cfg(not(windows))]
    let found = Command::new("fc-match")
        .args([
            "-s",
            "-f",
            "%{file}\\n%{index}\\n",
            &format!(":charset={:x}", u32::from(character)),
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| candidates(&String::from_utf8_lossy(&output.stdout), limit))
        .unwrap_or_default();
    found
}

/// The files and faces `fc-match -s` printed, two lines each. A variable
/// font's named instance (the high bits of its index) is its face.
#[cfg_attr(windows, allow(dead_code))]
fn candidates(printed: &str, limit: usize) -> Vec<(String, u32)> {
    let lines: Vec<&str> = printed.lines().collect();
    lines
        .chunks_exact(2)
        .filter_map(|pair| {
            let file = pair[0].trim();
            let index: u32 = pair[1].trim().parse().ok()?;
            (file.starts_with('/') && opentype(file)).then(|| (file.to_owned(), index & 0xFFFF))
        })
        .take(limit)
        .collect()
}

/// Whether `file` is an OpenType font or collection, by its name.
fn opentype(file: &str) -> bool {
    [".ttf", ".otf", ".ttc", ".otc"]
        .iter()
        .any(|end| file.to_ascii_lowercase().ends_with(end))
}

/// The face `index` of the font file `file`, if it is not too large.
pub fn read_face(file: &str, index: u32) -> Option<Face> {
    read(&(file.to_owned(), index))
}

fn read((file, index): &(String, u32)) -> Option<Face> {
    let size = std::fs::metadata(file).ok()?.len();
    if size > MAX_FACE_BYTES {
        return None;
    }
    Some(Face {
        bytes: std::fs::read(file).ok()?,
        index: *index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallbacks_are_opentype_files_in_order_with_their_face() {
        let printed = "/f/NotoSansCJK-VF.otf.ttc\n262144\n/f/unifont.pcf.gz\n0\n\
                       /f/DejaVuSans.ttf\n0\nrelative.ttf\n0\n/f/Extra.otf\n0\n";
        assert_eq!(
            candidates(printed, 2),
            [
                ("/f/NotoSansCJK-VF.otf.ttc".to_owned(), 0),
                ("/f/DejaVuSans.ttf".to_owned(), 0)
            ]
        );
    }

    #[test]
    fn families_are_first_names_sorted_without_hidden_ones() {
        let listed = "Noto Sans\nInter Display,Inter Display Thin\n.Hidden\ninter\nNoto Sans\n\n";
        assert_eq!(
            parse_families(listed),
            ["inter", "Inter Display", "Noto Sans"]
        );
    }

    #[test]
    fn a_match_must_be_the_family_asked_for_near_the_weight() {
        let printed = "/fonts/Inter.ttc\n3\nInter,Inter Medium\n100";
        assert_eq!(
            matched(printed, "inter", 100),
            Some(("/fonts/Inter.ttc".into(), 3))
        );
        assert_eq!(matched(printed, "Inter", 200), None, "not bold");
        let semibold = "/fonts/Inter-SemiBold.ttf\n0\nInter\n180";
        assert_eq!(
            matched(semibold, "Inter", 180),
            Some(("/fonts/Inter-SemiBold.ttf".into(), 0))
        );
        // Without one, fontconfig's bold is near enough: `load` keeps it once.
        let bold = "/fonts/Inter-Bold.ttf\n0\nInter\n200";
        assert!(matched(bold, "Inter", 180).is_some());
        let fallback = "/fonts/DejaVuSans.ttf\n0\nDejaVu Sans\n80";
        assert_eq!(matched(fallback, "Missing", 80), None);
        let variable = "/fonts/Noto.ttf\n0\nNoto Sans\n[100 900]";
        assert_eq!(
            matched(variable, "Noto Sans", 200),
            Some(("/fonts/Noto.ttf".into(), 0)),
            "a variable font covers the weight"
        );
        assert_eq!(escape("Font-Name: x"), r"Font\-Name\: x");
    }
}
