//! The system's fonts for characters none of RUNE's own draws, found as a
//! browser finds them: fontconfig's best match for the character
//! (2026-10-17: a real account's names and server tags with such
//! characters showed boxes). A face is loaded only once something on
//! screen needs it, and only a few, each at most what
//! `rune_platform::fonts` reads; they go after every face RUNE has, so
//! nothing it draws changes.

use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};

use eframe::egui;
use skrifa::MetadataProvider;

/// Faces loaded at most, so that a page of every script does not load
/// every font of the system.
const MAX_FACES: usize = 6;
/// Characters looked up at most in a session, and in one lookup.
const MAX_LOOKUPS: usize = 512;
const BATCH: usize = 32;
/// Candidates fontconfig gives, tried in order, for one character.
const CANDIDATES: usize = 4;

/// What a lookup found: each character, and the file and face that draws it.
pub type Found = Vec<(char, Option<(String, Arc<egui::FontData>)>)>;

#[derive(Default)]
pub struct Fallbacks {
    /// Characters seen, and whether a face installed draws them.
    seen: HashMap<char, bool>,
    /// Characters no face draws, waiting for a lookup.
    wanted: Vec<char>,
    looked_up: HashSet<char>,
    /// Faces loaded: their file, and their name in the definitions.
    loaded: Vec<(String, String, Arc<egui::FontData>)>,
    lookup: Option<mpsc::Receiver<Found>>,
}

/// Whether a face of `fonts` has a glyph for `character`.
fn drawn<'a>(fonts: impl IntoIterator<Item = &'a egui::FontData>, character: char) -> bool {
    fonts.into_iter().any(|data| {
        skrifa::FontRef::from_index(&data.font, data.index)
            .is_ok_and(|font| font.charmap().map(character).is_some())
    })
}

/// Characters drawn as nothing, or by the emoji painted over the text,
/// which no fallback should be loaded for.
fn ignored(character: char) -> bool {
    character.is_whitespace()
        || character.is_control()
        || matches!(
            character,
            '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{1F3FB}'..='\u{1F3FF}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

impl Fallbacks {
    /// Look at the text `frame` paints: a character no face of `fonts`
    /// draws is wanted for a lookup.
    pub fn scan(&mut self, fonts: &egui::FontDefinitions, frame: &limn::Frame) {
        for command in &frame.commands {
            let limn::DrawCommand::Text { text, .. } = command else {
                continue;
            };
            // Plain ASCII, most of what is painted, is RUNE's own.
            for character in text.chars().filter(|character| !character.is_ascii()) {
                if ignored(character) || self.seen.contains_key(&character) {
                    continue;
                }
                let covered = drawn(
                    fonts
                        .font_data
                        .values()
                        .map(|data| &**data)
                        .chain(self.loaded.iter().map(|(_, _, data)| &**data)),
                    character,
                );
                self.seen.insert(character, covered);
                if !covered && self.looked_up.len() + self.wanted.len() < MAX_LOOKUPS {
                    self.wanted.push(character);
                }
            }
        }
    }

    /// Start a lookup of what is wanted, if none is running; take a
    /// finished one's faces. `Some` with `base` and the faces after it
    /// when the fonts changed.
    pub fn step(
        &mut self,
        base: &egui::FontDefinitions,
        start: impl FnOnce(Box<dyn FnOnce() -> Found + Send>) -> mpsc::Receiver<Found>,
    ) -> Option<egui::FontDefinitions> {
        let mut changed = false;
        if let Some(found) = self
            .lookup
            .as_ref()
            .and_then(|lookup| lookup.try_recv().ok())
        {
            self.lookup = None;
            for (character, face) in found {
                let Some((file, data)) = face else {
                    continue;
                };
                if !self.loaded.iter().any(|(known, ..)| *known == file)
                    && self.loaded.len() < MAX_FACES
                {
                    let name = format!("rune-system-fallback-{}", self.loaded.len());
                    self.loaded.push((file, name, data));
                    changed = true;
                }
                if drawn(self.loaded.iter().map(|(_, _, data)| &**data), character) {
                    self.seen.insert(character, true);
                }
            }
        }
        if self.lookup.is_none() && !self.wanted.is_empty() && self.loaded.len() < MAX_FACES {
            let batch: Vec<char> = self.wanted.drain(..self.wanted.len().min(BATCH)).collect();
            self.looked_up.extend(batch.iter().copied());
            let known: Vec<String> = self.loaded.iter().map(|(file, ..)| file.clone()).collect();
            self.lookup = Some(start(Box::new(move || look_up(&batch, &known))));
        }
        changed.then(|| self.with(base))
    }

    /// `base` with the faces loaded after every family's own.
    pub fn with(&self, base: &egui::FontDefinitions) -> egui::FontDefinitions {
        let mut fonts = base.clone();
        for (_, name, data) in &self.loaded {
            fonts.font_data.insert(name.clone(), data.clone());
            for family in fonts.families.values_mut() {
                family.push(name.clone());
            }
        }
        fonts
    }
}

/// The first face fontconfig offers for each character that really draws
/// it, skipping the files already loaded; a face found for one character
/// of the batch serves the others it draws.
fn look_up(characters: &[char], known: &[String]) -> Found {
    let mut found: Vec<(String, Arc<egui::FontData>)> = Vec::new();
    characters
        .iter()
        .map(|character| {
            if let Some((file, data)) = found.iter().find(|(_, data)| drawn([&**data], *character))
            {
                return (*character, Some((file.clone(), data.clone())));
            }
            for (file, index) in rune_platform::fonts::fallbacks(*character, CANDIDATES) {
                if known.contains(&file) || found.iter().any(|(taken, _)| *taken == file) {
                    continue;
                }
                let Some(face) = rune_platform::fonts::read_face(&file, index) else {
                    continue;
                };
                let data = Arc::new(egui::FontData {
                    index: face.index,
                    ..egui::FontData::from_owned(face.bytes)
                });
                if drawn([&*data], *character) {
                    found.push((file.clone(), data.clone()));
                    return (*character, Some((file, data)));
                }
            }
            (*character, None)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_characters_no_face_draws_are_wanted_once_each() {
        let fonts = limn_egui::font_definitions();
        let mut fallbacks = Fallbacks::default();
        let mut frame = limn::Frame::default();
        frame.commands.push(limn::DrawCommand::Text {
            text: "jugador ㅤ가 abc ñ ㅤ\u{200B}".into(),
            position: limn::Point::new(0.0, 0.0),
            color: limn::Color::WHITE,
            font_size: 14.0,
            font_family: limn::FontFamily::Sans,
        });
        fallbacks.scan(&fonts, &frame);
        fallbacks.scan(&fonts, &frame);
        assert_eq!(fallbacks.wanted, ['ㅤ', '가'], "Hangul: not in RUNE's fonts");
        assert_eq!(fallbacks.seen.get(&'ñ'), Some(&true), "RUNE's own");
        // A lookup that found nothing changes no font, and is not repeated.
        let started = std::cell::Cell::new(0);
        let changed = fallbacks.step(&fonts, |work| {
            started.set(started.get() + 1);
            let (sender, receiver) = mpsc::channel();
            drop(work);
            sender.send(vec![('ㅤ', None), ('가', None)]).unwrap();
            receiver
        });
        assert!(changed.is_none() && started.get() == 1);
        assert!(fallbacks.step(&fonts, |_| unreachable!()).is_none());
        assert!(fallbacks.wanted.is_empty() && fallbacks.lookup.is_none());
    }

    /// Asks this machine's fontconfig: run it by hand (`--ignored`).
    #[test]
    #[ignore = "reads the system's fonts"]
    fn the_system_has_a_face_for_hangul_and_its_filler() {
        let found = look_up(&['ㅤ', '가', '★'], &[]);
        for (character, face) in &found {
            eprintln!("{character:?}: {:?}", face.as_ref().map(|(file, _)| file));
        }
        assert!(found[0].1.is_some() && found[1].1.is_some(), "{found:?}");
    }
}
