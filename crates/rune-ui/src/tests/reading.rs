//! Serein's reading settings and the pictures on disk (imagenes-y-lectura),
//! driven as a person would: the links of the pictures shown, the member
//! list in a narrow window, and the sections of the settings.

use super::*;
use crate::reading::{CachePart, CacheUsage};
use crate::settings::SEARCH;
use rune_model::{Embed, EmbedImage, EmbedKind, ImageKind, RemoteImage};

impl Host {
    /// Do `action` as the settings get it, and what they asked.
    fn reading_act(&mut self, action: UiAction) -> Vec<Intent> {
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents.clone());
        intents
    }

    fn reading_press(&mut self, id: &str) -> Vec<Intent> {
        self.reading_act(UiAction::Clicked { id: id.into() })
    }

    /// Open the settings at what `query` finds, as the search box does.
    fn reading_find(&mut self, query: &str) {
        self.click("me:settings");
        self.frame();
        self.tree.set_input_value(SEARCH, query);
        self.reading_act(UiAction::InputChanged {
            id: SEARCH.into(),
            value: query.into(),
        });
        self.reading_act(UiAction::Submitted {
            id: SEARCH.into(),
            value: query.into(),
        });
    }

    fn reading_buttons(&mut self) -> Vec<String> {
        let frame = self.frame();
        frame
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
            .map(|region| region.id.clone())
            .collect()
    }
}

fn save(key: &str, value: &str) -> Intent {
    Intent::SaveSetting {
        key: key.into(),
        value: Some(value.into()),
    }
}

#[test]
fn a_picture_s_lone_link_hides_unless_the_person_keeps_the_links() {
    let link = "https://example.test/gato.gif";
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content = link.into();
    message.attachments.clear();
    message.reactions.clear();
    message.poll = None;
    message.components = Default::default();
    message.text_references.clear();
    message.embeds = vec![Embed {
        kind: EmbedKind::Gifv,
        url: Some(link.into()),
        image: Some(EmbedImage {
            source: RemoteImage {
                url: "https://media.example.test/gato.png".into(),
                kind: ImageKind::AttachmentPreview,
            },
            dimensions: (320, 240),
        }),
        ..Embed::default()
    }];
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    host.tree.scroll_to_end(&timeline_id(channel));
    let shows_link = |host: &mut Host| {
        host.painted()
            .iter()
            .any(|text| text.contains("example.test/gato.gif"))
    };
    assert!(!shows_link(&mut host), "Serein's default: the GIF alone");
    assert_eq!(
        host.reading_press("us:hide-media-links"),
        vec![save("hide_media_links", "false")]
    );
    assert!(shows_link(&mut host), "the link stays over its picture");
    host.reading_press("us:hide-media-links");
    assert!(!shows_link(&mut host));
}

#[test]
fn the_member_list_stays_in_a_narrow_window_when_the_person_keeps_it() {
    let mut host = Host::new();
    host.open_channel("general");
    host.ui.members_open = true;
    let laid = |host: &mut Host, width: f32| {
        let size = Size::new(width, 797.0);
        host.tree.set_root(view(
            &host.ui,
            &host.store,
            &ViewContext {
                window: size,
                ..host.context()
            },
        ));
        host.tree.layout(size)
    };
    let shown = |frame: &Frame| frame.regions.iter().any(|region| region.id == "members");
    assert!(!shown(&laid(&mut host, 1100.0)), "it gives its room away");
    assert_eq!(
        host.reading_press("us:members-narrow"),
        vec![save("members_narrow", "true")]
    );
    for width in [1100.0, 960.0] {
        let frame = laid(&mut host, width);
        assert!(shown(&frame), "kept at {width}");
        assert!(frame
            .regions
            .iter()
            .any(|region| region.id == "members:toggle"));
        let timeline = frame
            .regions
            .iter()
            .find(|region| region.id.starts_with("timeline:"))
            .unwrap()
            .rect;
        assert!(timeline.width >= 280.0, "{timeline:?} at {width}");
    }
    // Narrower than a line's room beside it, it goes all the same.
    assert!(!shown(&laid(&mut host, 900.0)));
    // And closing it by hand still closes it.
    host.ui.members_open = false;
    assert!(!shown(&laid(&mut host, 1100.0)));
}

#[test]
fn the_reading_settings_switch_slide_and_are_kept() {
    let mut host = Host::new();
    host.reading_find("animar gif");
    assert_eq!(host.ui.settings, Some(SettingsPage::Appearance));
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "Animar GIF"),
        "{painted:?}"
    );
    assert!(host
        .reading_buttons()
        .contains(&"us:animate-gifs".to_owned()));
    assert_eq!(
        host.reading_press("us:animate-gifs"),
        vec![save("animate_gifs", "false")]
    );
    assert!(!host.ui.prefs.animate_gifs);
    // The wheel's speed: by fives, kept when let go, and back to 100 %.
    host.reading_find("velocidad de desplazamiento");
    let moved = host.reading_act(UiAction::SliderChanged {
        id: "us:scroll-speed".into(),
        fraction: crate::reading::speed_fraction(150) + 0.004,
        committed: false,
    });
    assert!(moved.is_empty());
    assert_eq!(host.ui.prefs.scroll_speed, 150);
    assert!(host.painted().iter().any(|text| text == "150 %"));
    let kept = host.reading_act(UiAction::SliderChanged {
        id: "us:scroll-speed".into(),
        fraction: crate::reading::speed_fraction(150),
        committed: true,
    });
    assert_eq!(kept, vec![save("scroll_speed", "150")]);
    assert!((host.ui.prefs.scroll_factor() - 1.5).abs() < f32::EPSILON);
    assert_eq!(
        host.reading_press("us:scroll-speed-reset"),
        vec![save("scroll_speed", "100")]
    );
    // All of them come back in the next run.
    let mut prefs = host.ui.prefs.clone();
    prefs.hide_media_links = false;
    prefs.members_narrow = true;
    let kept = prefs.kept();
    let mut back = Prefs::default();
    back.restore(&|key: &str| {
        kept.iter()
            .find(|(kept, _)| *kept == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(back, prefs);
}

#[test]
fn the_image_cache_says_what_it_takes_and_clears_each_part() {
    let mut host = Host::new();
    host.reading_find("caché de imágenes");
    assert_eq!(host.ui.settings, Some(SettingsPage::Privacy));
    // Kept in memory only: it says so, and there is nothing to clear.
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("quedan sólo en memoria")));
    assert!(host.reading_press("us:cache-clear=media").is_empty());
    host.ui.image_cache = Some(CacheUsage {
        media_bytes: 3 * 1024 * 1024 / 2,
        artwork_bytes: 2048,
    });
    let painted = host.painted();
    assert!(
        painted
            .iter()
            .any(|text| text.contains("Usa 1.5 MB de 1 GB.")),
        "{painted:?}"
    );
    assert!(painted
        .iter()
        .any(|text| text.contains("Usa 2.0 KB de 128 MB.")));
    assert_eq!(
        host.reading_press("us:cache-clear=media"),
        vec![Intent::ClearImageCache(CachePart::Media)]
    );
    assert_eq!(
        host.reading_press("us:cache-clear=artwork"),
        vec![Intent::ClearImageCache(CachePart::Artwork)]
    );
    // A budget and the retention, from their lists; nothing but presets.
    host.reading_press("us:select:cache-media");
    assert_eq!(host.ui.settings_state.select, Some("cache-media"));
    assert!(host
        .reading_buttons()
        .contains(&"us:cache-media=4096".to_owned()));
    assert_eq!(
        host.reading_press("us:cache-media=4096"),
        vec![save("media_cache_mib", "4096")]
    );
    assert_eq!(host.ui.settings_state.select, None);
    assert!(host.painted().iter().any(|text| text.contains("de 4 GB.")));
    assert!(host.reading_press("us:cache-media=3000").is_empty());
    assert_eq!(host.ui.prefs.media_cache_mib, 4096);
    assert_eq!(
        host.reading_press("us:cache-days=7"),
        vec![save("cache_days", "7")]
    );
    assert_eq!(host.ui.prefs.cache_days, 7);
}
