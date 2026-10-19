//! The system's fonts on Windows (windows-medios), through DirectWrite's
//! own collection of the installed fonts (the machine's and the user's):
//! the families by their English names (or their first), and a family's
//! faces by the files DirectWrite reads them from. Nothing is copied or
//! kept, as on Linux. Each call makes its own factory: none of this runs on
//! the interface's thread.

use windows::core::{Interface, PCWSTR};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFont, IDWriteFontCollection,
    IDWriteLocalFontFileLoader, IDWriteLocalizedStrings, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_SIMULATIONS_NONE, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT,
};

use super::{valid, MAX_FAMILIES};

/// The installed fonts, as DirectWrite has them now.
fn collection() -> Option<IDWriteFontCollection> {
    // SAFETY: a shared factory; the collection is written once, owned here.
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        let mut collection = None;
        factory
            .GetSystemFontCollection(&mut collection, false)
            .ok()?;
        collection
    }
}

/// The English name among `names`, or the first.
fn english(names: &IDWriteLocalizedStrings) -> Option<String> {
    let (mut index, mut exists) = (0, false.into());
    // SAFETY: valid out-pointers; the buffer holds the name and its NUL.
    unsafe {
        names
            .FindLocaleName(windows::core::w!("en-us"), &mut index, &mut exists)
            .ok()?;
        if !exists.as_bool() {
            index = 0;
        }
        if index >= names.GetCount() {
            return None;
        }
        let length = names.GetStringLength(index).ok()? as usize;
        let mut name = vec![0u16; length.checked_add(1)?];
        names.GetString(index, &mut name).ok()?;
        String::from_utf16(&name[..length]).ok()
    }
}

/// The installed families, sorted, the same way [`super::families`] lists
/// fontconfig's.
pub(super) fn families() -> Vec<String> {
    let Some(collection) = collection() else {
        return Vec::new();
    };
    // SAFETY: indices below the collection's own count.
    let names = (0..unsafe { collection.GetFontFamilyCount() })
        .take(MAX_FAMILIES)
        .filter_map(|at| {
            let family = unsafe { collection.GetFontFamily(at) }.ok()?;
            english(&unsafe { family.GetFamilyNames() }.ok()?)
        });
    tidy(names)
}

/// Valid names, sorted without case, each once, at most [`MAX_FAMILIES`].
fn tidy(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut families: Vec<String> = names
        .filter(|family| valid(family) && !family.starts_with('.'))
        .collect();
    families.sort_by_key(|family| family.to_lowercase());
    families.dedup();
    families.truncate(MAX_FAMILIES);
    families
}

/// Where the font `font` is read from: its file and its face in it. Only a
/// face DirectWrite draws as it is, not one it makes bolder or slanted.
fn file(font: &IDWriteFont) -> Option<(String, u32)> {
    // SAFETY: COM calls on owned objects; the file array and the path
    // buffer are sized by the calls that say how large they are; the key
    // is only read while the file that owns it lives.
    unsafe {
        if font.GetSimulations() != DWRITE_FONT_SIMULATIONS_NONE {
            return None;
        }
        let face = font.CreateFontFace().ok()?;
        let mut count = 0;
        face.GetFiles(&mut count, None).ok()?;
        if count != 1 {
            return None;
        }
        let mut files = [None];
        face.GetFiles(&mut count, Some(files.as_mut_ptr())).ok()?;
        let file = files[0].take()?;
        let (mut key, mut size) = (std::ptr::null_mut(), 0);
        file.GetReferenceKey(&mut key, &mut size).ok()?;
        // Only a file on disk: another loader's are not files RUNE can read.
        let loader: IDWriteLocalFontFileLoader = file.GetLoader().ok()?.cast().ok()?;
        let length = loader.GetFilePathLengthFromKey(key, size).ok()? as usize;
        let mut path = vec![0u16; length.checked_add(1)?];
        loader.GetFilePathFromKey(key, size, &mut path).ok()?;
        let path = String::from_utf16(&path[..length]).ok()?;
        Some((path, face.GetIndex()))
    }
}

/// The weights asked for, as CSS and DirectWrite number them, and how far
/// one found may be: as near as fontconfig's 30 of its own scale.
const REGULAR: i32 = 400;
const MEDIUM: i32 = 500;
const SEMIBOLD: i32 = 600;
const BOLD: i32 = 700;
const NEAR: i32 = 60;

/// `family`'s file at `weight`, upright and of normal width, if it has a
/// face near that weight.
fn face(collection: &IDWriteFontCollection, family: &str, weight: i32) -> Option<(String, u32)> {
    let name = crate::win::wide(family);
    let (mut index, mut exists) = (0, false.into());
    // SAFETY: the name ends in NUL; valid out-pointers.
    unsafe {
        collection
            .FindFamilyName(PCWSTR(name.as_ptr()), &mut index, &mut exists)
            .ok()?;
        if !exists.as_bool() {
            return None;
        }
        let font = collection
            .GetFontFamily(index)
            .ok()?
            .GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT(weight),
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )
            .ok()?;
        if (font.GetWeight().0 - weight).abs() > NEAR {
            return None;
        }
        file(&font)
    }
}

/// The files of `family`'s regular, medium, semibold and bold faces, as
/// [`super::load`] reads them.
pub(super) fn faces(family: &str) -> Option<[Option<(String, u32)>; 4]> {
    let collection = collection()?;
    Some([REGULAR, MEDIUM, SEMIBOLD, BOLD].map(|weight| face(&collection, family, weight)))
}

/// Families Windows itself falls back to, tried first: its symbols and
/// emoji, then its scripts' own.
const PREFERRED: &[&str] = &[
    "Segoe UI",
    "Segoe UI Symbol",
    "Segoe UI Emoji",
    "Segoe UI Historic",
    "Microsoft YaHei",
    "Microsoft JhengHei",
    "Yu Gothic",
    "Malgun Gothic",
    "Nirmala UI",
    "Leelawadee UI",
    "Ebrima",
    "Gadugi",
    "Myanmar Text",
    "Javanese Text",
    "Mongolian Baiti",
    "Microsoft Himalaya",
];

/// The files of the families that have `character`, best first: Windows'
/// own fallbacks, then any other, upright and regular; at most `limit`.
pub(super) fn fallbacks(character: char, limit: usize) -> Vec<(String, u32)> {
    let Some(collection) = collection() else {
        return Vec::new();
    };
    let has = |font: &IDWriteFont| {
        // SAFETY: a plain query on an owned font.
        unsafe { font.HasCharacter(u32::from(character)) }.is_ok_and(|has| has.as_bool())
    };
    let regular = |at: u32| {
        // SAFETY: an index below the collection's count.
        unsafe {
            collection.GetFontFamily(at).ok()?.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT(REGULAR),
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )
        }
        .ok()
    };
    let preferred = PREFERRED.iter().filter_map(|family| {
        let name = crate::win::wide(family);
        let (mut index, mut exists) = (0, false.into());
        // SAFETY: the name ends in NUL; valid out-pointers.
        unsafe { collection.FindFamilyName(PCWSTR(name.as_ptr()), &mut index, &mut exists) }
            .ok()?;
        exists.as_bool().then_some(index)
    });
    // SAFETY: a plain count.
    let count = unsafe { collection.GetFontFamilyCount() };
    let mut seen = Vec::new();
    preferred
        .chain(0..count)
        .filter(|at| {
            let fresh = !seen.contains(at);
            seen.push(*at);
            fresh
        })
        .filter_map(regular)
        .filter(has)
        .filter_map(|font| file(&font))
        .filter(|(file, _)| super::opentype(file))
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_valid_names_sorted_each_once() {
        let names = [
            "Segoe UI",
            "arial",
            ".Hidden",
            "Segoe UI",
            "",
            "Arial Black",
        ];
        assert_eq!(
            tidy(names.into_iter().map(String::from)),
            ["arial", "Arial Black", "Segoe UI"]
        );
    }

    // DirectWrite of the runner (CI) or of Wine, with whatever it has installed.
    #[test]
    #[ignore = "reads the system's fonts"]
    fn the_system_lists_families_and_reads_a_face() {
        let families = families();
        assert!(!families.is_empty());
        let readable = families
            .iter()
            .take(32)
            .find_map(|family| super::super::load(family));
        assert!(readable.is_some_and(|faces| !faces.regular.bytes.is_empty()));
        assert!(!fallbacks('a', 2).is_empty());
    }
}
