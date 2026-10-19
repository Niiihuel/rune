//! Serein's reading settings (imagenes-y-lectura): animations that play
//! only under the pointer, the links of pictures shown kept or hidden, the
//! speed of the wheel and the trackpad, and the budgets of the pictures
//! kept on disk between runs; and RUNE's own, the member list kept in
//! narrow windows.

use crate::{Intent, Prefs, Ui};

/// The wheel's and the trackpad's speed, in percent: Serein's range, by
/// fives; 100 is as the system moves.
pub const SCROLL_SPEEDS: (u16, u16) = (25, 300);
const SCROLL_STEP: u16 = 5;
/// The budgets of the two parts on disk, in MiB, and the days a picture
/// not shown again stays: Serein's presets.
pub const MEDIA_BUDGETS: [u32; 5] = [256, 512, 1024, 2048, 4096];
pub const ARTWORK_BUDGETS: [u32; 4] = [64, 128, 256, 512];
pub const KEPT_DAYS: [u16; 5] = [7, 30, 90, 180, 365];

/// A part of the pictures on disk, as the host's cache splits them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachePart {
    /// Previews of attachments and cards, banners and pictures at their size.
    Media,
    /// Avatars, emoji, stickers, icons and badges.
    Artwork,
}

/// What the pictures on disk take, as the host measured it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheUsage {
    pub media_bytes: u64,
    pub artwork_bytes: u64,
}

impl CacheUsage {
    pub fn bytes(&self, part: CachePart) -> u64 {
        match part {
            CachePart::Media => self.media_bytes,
            CachePart::Artwork => self.artwork_bytes,
        }
    }
}

impl Prefs {
    /// What a turn of the wheel or a swipe of the trackpad moves, for the host.
    pub fn scroll_factor(&self) -> f32 {
        f32::from(self.scroll_speed) / 100.0
    }

    /// The disk's budget of `part`, in bytes.
    pub fn cache_bytes(&self, part: CachePart) -> u64 {
        u64::from(match part {
            CachePart::Media => self.media_cache_mib,
            CachePart::Artwork => self.artwork_cache_mib,
        }) * 1024
            * 1024
    }
}

/// The speed a slider at `fraction` stands for.
pub(crate) fn speed_at(fraction: f32) -> u16 {
    let (low, high) = SCROLL_SPEEDS;
    let steps = f32::from((high - low) / SCROLL_STEP);
    low + (fraction.clamp(0.0, 1.0) * steps).round() as u16 * SCROLL_STEP
}

/// Where `speed` stands on its slider.
pub(crate) fn speed_fraction(speed: u16) -> f32 {
    let (low, high) = SCROLL_SPEEDS;
    f32::from(speed.clamp(low, high) - low) / f32::from(high - low)
}

/// A budget or a retention as the settings say it.
pub(crate) fn size_label(mib: u32) -> String {
    if mib >= 1024 && mib.is_multiple_of(1024) {
        format!("{} GB", mib / 1024)
    } else {
        format!("{mib} MB")
    }
}

pub(crate) fn kept(prefs: &Prefs) -> Vec<(&'static str, Option<String>)> {
    let flag = |on: bool| Some(on.to_string());
    vec![
        ("animate_gifs", flag(prefs.animate_gifs)),
        ("hide_media_links", flag(prefs.hide_media_links)),
        ("scroll_speed", Some(prefs.scroll_speed.to_string())),
        ("members_narrow", flag(prefs.members_narrow)),
        ("media_cache_mib", Some(prefs.media_cache_mib.to_string())),
        (
            "artwork_cache_mib",
            Some(prefs.artwork_cache_mib.to_string()),
        ),
        ("cache_days", Some(prefs.cache_days.to_string())),
    ]
}

/// Take back what [`kept`] kept; what is missing or not a preset stays as
/// it starts.
pub(crate) fn restore(prefs: &mut Prefs, setting: &impl Fn(&str) -> Option<String>) {
    let usual = Prefs::default();
    let flag = |key: &str, default: bool| match setting(key).as_deref() {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    };
    prefs.animate_gifs = flag("animate_gifs", usual.animate_gifs);
    prefs.hide_media_links = flag("hide_media_links", usual.hide_media_links);
    prefs.members_narrow = flag("members_narrow", usual.members_narrow);
    let (low, high) = SCROLL_SPEEDS;
    prefs.scroll_speed = setting("scroll_speed")
        .and_then(|value| value.parse().ok())
        .filter(|speed| (low..=high).contains(speed) && speed % SCROLL_STEP == 0)
        .unwrap_or(usual.scroll_speed);
    prefs.media_cache_mib = setting("media_cache_mib")
        .and_then(|value| value.parse().ok())
        .filter(|mib| MEDIA_BUDGETS.contains(mib))
        .unwrap_or(usual.media_cache_mib);
    prefs.artwork_cache_mib = setting("artwork_cache_mib")
        .and_then(|value| value.parse().ok())
        .filter(|mib| ARTWORK_BUDGETS.contains(mib))
        .unwrap_or(usual.artwork_cache_mib);
    prefs.cache_days = setting("cache_days")
        .and_then(|value| value.parse().ok())
        .filter(|days| KEPT_DAYS.contains(days))
        .unwrap_or(usual.cache_days);
}

fn save(key: &str, value: impl ToString) -> Intent {
    Intent::SaveSetting {
        key: key.into(),
        value: Some(value.to_string()),
    }
}

/// A click of these settings, `rest` after `us:`; `None` for the others.
pub(crate) fn clicked(ui: &mut Ui, rest: &str) -> Option<Vec<Intent>> {
    let prefs = &mut ui.prefs;
    let flip = |value: &mut bool, key: &str| {
        *value = !*value;
        vec![save(key, *value)]
    };
    if let Some(name) = rest.strip_prefix("select:") {
        let name = ["cache-media", "cache-artwork", "cache-days"]
            .into_iter()
            .find(|known| *known == name)?;
        let state = &mut ui.settings_state;
        state.select = (state.select != Some(name)).then_some(name);
        return Some(Vec::new());
    }
    if let Some((what, value)) = rest.split_once('=') {
        let chosen = |presets: &[u32]| {
            value
                .parse::<u32>()
                .ok()
                .filter(|value| presets.contains(value))
        };
        let intent = match what {
            "cache-media" => {
                prefs.media_cache_mib = chosen(&MEDIA_BUDGETS)?;
                save("media_cache_mib", prefs.media_cache_mib)
            }
            "cache-artwork" => {
                prefs.artwork_cache_mib = chosen(&ARTWORK_BUDGETS)?;
                save("artwork_cache_mib", prefs.artwork_cache_mib)
            }
            "cache-days" => {
                prefs.cache_days = value.parse().ok().filter(|days| KEPT_DAYS.contains(days))?;
                save("cache_days", prefs.cache_days)
            }
            "cache-clear" => {
                // Nothing on disk to clear: the button is disabled then.
                ui.image_cache?;
                Intent::ClearImageCache(match value {
                    "media" => CachePart::Media,
                    "artwork" => CachePart::Artwork,
                    _ => return None,
                })
            }
            _ => return None,
        };
        ui.settings_state.select = None;
        return Some(vec![intent]);
    }
    Some(match rest {
        "animate-gifs" => flip(&mut prefs.animate_gifs, "animate_gifs"),
        "hide-media-links" => flip(&mut prefs.hide_media_links, "hide_media_links"),
        "members-narrow" => flip(&mut prefs.members_narrow, "members_narrow"),
        "scroll-speed-reset" => {
            prefs.scroll_speed = Prefs::default().scroll_speed;
            vec![save("scroll_speed", prefs.scroll_speed)]
        }
        _ => return None,
    })
}

/// The wheel's speed slider moved; `None` for the other sliders.
pub(crate) fn slid(ui: &mut Ui, id: &str, fraction: f32, committed: bool) -> Option<Vec<Intent>> {
    if id != "us:scroll-speed" {
        return None;
    }
    ui.prefs.scroll_speed = speed_at(fraction);
    Some(if committed {
        vec![save("scroll_speed", ui.prefs.scroll_speed)]
    } else {
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_go_by_fives_and_only_presets_come_back() {
        assert_eq!(speed_at(0.0), 25);
        assert_eq!(speed_at(1.0), 300);
        assert_eq!(speed_at(speed_fraction(100)), 100);
        assert_eq!(speed_at(2.0), 300);
        let mut prefs = Prefs::default();
        let kept: std::collections::HashMap<_, _> = [
            ("scroll_speed", "103"),
            ("media_cache_mib", "4096"),
            ("artwork_cache_mib", "100000"),
            ("cache_days", "7"),
            ("animate_gifs", "false"),
        ]
        .into_iter()
        .collect();
        restore(&mut prefs, &|key| {
            kept.get(key).map(|value| value.to_string())
        });
        assert_eq!(prefs.scroll_speed, 100, "not by fives");
        assert_eq!(prefs.media_cache_mib, 4096);
        assert_eq!(prefs.artwork_cache_mib, 128, "not a preset");
        assert_eq!(prefs.cache_days, 7);
        assert!(!prefs.animate_gifs && prefs.hide_media_links && !prefs.members_narrow);
        assert_eq!(size_label(1024), "1 GB");
        assert_eq!(size_label(256), "256 MB");
        assert_eq!(prefs.cache_bytes(CachePart::Media), 4096 * 1024 * 1024);
    }
}
