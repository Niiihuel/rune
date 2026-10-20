//! RUNE's icon set, drawn by `tools/rune-icons` and installed on each UI
//! thread with `limn_egui::set_icon_pack(rune_components::glyphs::pack)`.
//! A limn icon it lacks stays a Bootstrap Icon.

use limn::Icon;

macro_rules! glyph {
    ($name:literal) => {
        include_str!(concat!("../../../assets/glyphs/", $name, ".svg"))
    };
}
pub(crate) use glyph;

/// Kinds of channel limn has no icon for.
pub const MEGAPHONE: &str = glyph!("megaphone");
pub const FORUM: &str = glyph!("forum");
pub const THREAD: &str = glyph!("thread");
pub const STAGE: &str = glyph!("stage");

/// A private channel's glyph: its kind's, with a padlock cut into its top
/// right corner, as the official client draws them.
pub fn locked(glyph: Icon) -> Option<Icon> {
    Some(Icon::Custom(match glyph {
        Icon::Hash => glyph!("hash-lock"),
        Icon::Volume => glyph!("volume-lock"),
        Icon::Custom(MEGAPHONE) => glyph!("megaphone-lock"),
        Icon::Custom(FORUM) => glyph!("forum-lock"),
        Icon::Custom(THREAD) => glyph!("thread-lock"),
        Icon::Custom(STAGE) => glyph!("stage-lock"),
        _ => return None,
    }))
}

/// The artwork for `icon`; the filled variant of a pair shares it unless
/// the outline is a toggle's off state (heart, play, pause, stop).
pub fn pack(icon: Icon) -> Option<&'static str> {
    Some(match icon {
        Icon::Add => glyph!("add"),
        Icon::Minus => glyph!("minus"),
        Icon::Close => glyph!("close"),
        Icon::CloseBold => glyph!("close-bold"),
        Icon::Check => glyph!("check"),
        Icon::ChevronDown => glyph!("chevron-down"),
        Icon::ChevronUp => glyph!("chevron-up"),
        Icon::ChevronRight => glyph!("chevron-right"),
        Icon::ArrowLeft => glyph!("arrow-left"),
        Icon::ArrowRight => glyph!("arrow-right"),
        Icon::ArrowUp => glyph!("arrow-up"),
        Icon::ArrowDown => glyph!("arrow-down"),
        Icon::Reply => glyph!("reply"),
        Icon::Forward => glyph!("forward"),
        Icon::ExternalLink => glyph!("external-link"),
        Icon::Refresh => glyph!("refresh"),
        Icon::More => glyph!("more"),
        Icon::Menu => glyph!("menu"),
        Icon::DragHandle => glyph!("drag-handle"),
        Icon::Fullscreen => glyph!("fullscreen"),
        Icon::Collapse => glyph!("collapse"),
        Icon::Hash => glyph!("hash"),
        Icon::At => glyph!("at"),
        Icon::Code => glyph!("code"),
        Icon::Sliders => glyph!("sliders"),
        Icon::Filter => glyph!("filter"),
        Icon::Send => glyph!("send"),
        Icon::Paperclip => glyph!("paperclip"),
        Icon::Search => glyph!("search"),
        Icon::InfoCircle => glyph!("info-circle"),
        Icon::CheckCircle => glyph!("check-circle"),
        Icon::CloseCircle => glyph!("close-circle"),
        Icon::ErrorCircle => glyph!("error-circle"),
        Icon::WarningTriangle => glyph!("warning-triangle"),
        Icon::Circle => glyph!("circle"),
        Icon::Clock | Icon::ClockFill => glyph!("clock"),
        Icon::History => glyph!("history"),
        Icon::Bell | Icon::BellFill => glyph!("bell"),
        Icon::BellSlash => glyph!("bell-slash"),
        Icon::Lock => glyph!("lock"),
        Icon::Trash => glyph!("trash"),
        Icon::Edit => glyph!("edit"),
        Icon::Copy => glyph!("copy"),
        Icon::Pin => glyph!("pin"),
        Icon::Heart => glyph!("heart"),
        Icon::HeartFill => glyph!("heart-fill"),
        Icon::Flag => glyph!("flag"),
        Icon::Target => glyph!("target"),
        Icon::Bolt | Icon::BoltFill => glyph!("bolt"),
        Icon::Globe => glyph!("globe"),
        Icon::Lightbulb | Icon::Brain => glyph!("lightbulb"),
        Icon::Tree => glyph!("tree"),
        Icon::FriedEgg => glyph!("fried-egg"),
        Icon::Car => glyph!("car"),
        Icon::Gamepad => glyph!("gamepad"),
        Icon::Emoji => glyph!("emoji"),
        Icon::Neutral => glyph!("neutral"),
        Icon::Unread => glyph!("unread"),
        Icon::Microphone => glyph!("microphone"),
        Icon::MicrophoneMuted => glyph!("microphone-muted"),
        Icon::Headphones => glyph!("headphones"),
        Icon::Volume => glyph!("volume"),
        Icon::VolumeMuted => glyph!("volume-muted"),
        Icon::Video => glyph!("video"),
        Icon::VideoOff => glyph!("video-off"),
        Icon::Screen => glyph!("screen"),
        Icon::Phone => glyph!("phone"),
        Icon::HangUp => glyph!("hang-up"),
        Icon::Play => glyph!("play"),
        Icon::PlayFill => glyph!("play-fill"),
        Icon::Pause => glyph!("pause"),
        Icon::PauseFill => glyph!("pause-fill"),
        Icon::Stop => glyph!("stop"),
        Icon::StopFill => glyph!("stop-fill"),
        Icon::MusicNote => glyph!("music-note"),
        Icon::Audio => glyph!("audio"),
        Icon::Image => glyph!("image"),
        Icon::File => glyph!("file"),
        Icon::Folder | Icon::FolderFill => glyph!("folder"),
        Icon::Keyboard | Icon::KeyboardFill => glyph!("keyboard"),
        Icon::Laptop | Icon::LaptopFill => glyph!("laptop"),
        Icon::Users | Icon::UsersFill => glyph!("users"),
        Icon::Invite => glyph!("invite"),
        Icon::Leave => glyph!("leave"),
        Icon::Settings | Icon::SettingsFill => glyph!("settings"),
        Icon::Server | Icon::ServerFill => glyph!("server"),
        Icon::Puzzle | Icon::PuzzleFill => glyph!("puzzle"),
        Icon::ChartBar | Icon::ChartBarFill => glyph!("chart-bar"),
        Icon::Palette | Icon::PaletteFill => glyph!("palette"),
        Icon::Chat | Icon::ChatFill => glyph!("chat"),
        Icon::Archive | Icon::ArchiveFill => glyph!("archive"),
        Icon::ArchiveUp => glyph!("archive-up"),
        Icon::Book | Icon::BookFill => glyph!("book"),
        Icon::Grid | Icon::GridFill => glyph!("grid"),
        Icon::Text => glyph!("text"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph is one evenodd path in `currentColor` on the 24 grid,
    /// with no strokes: the shape the generator writes.
    #[test]
    fn glyphs_are_single_filled_paths() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/glyphs");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let svg = std::fs::read_to_string(&path).unwrap();
            assert!(
                svg.contains(r#"viewBox="0 0 24 24" fill="currentColor""#),
                "{path:?}"
            );
            assert_eq!(svg.matches("<path").count(), 1, "{path:?}");
            assert!(!svg.contains("stroke"), "{path:?}");
            count += 1;
        }
        assert!(count > 100);
    }

    #[test]
    fn pairs_share_or_split_as_documented() {
        assert_eq!(pack(Icon::Bell), pack(Icon::BellFill));
        assert_ne!(pack(Icon::Heart), pack(Icon::HeartFill));
        assert!(pack(Icon::Github).is_none());
    }
}
