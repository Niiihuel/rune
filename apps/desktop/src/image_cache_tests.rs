//! More pictures on screen than the GPU keeps (emoji-parpadeo, 2026-10-19):
//! the picker's server emoji beside the rail and the members. Each has to be
//! decoded once and stay; evicting what is drawn made them take turns to
//! appear and repainted without end.

use eframe::egui;

#[test]
fn a_screen_with_more_pictures_than_the_texture_limit_decodes_each_once() {
    limn_egui::set_image_cache_entries(rune_core::IMAGE_TEXTURE_ENTRIES);
    let pictures: Vec<_> = (0..rune_core::IMAGE_TEXTURE_ENTRIES + 26)
        .map(|index| {
            crate::capture_fixture::simulated(&rune_model::RemoteImage {
                url: format!("https://simulacion.invalid/emojis/{index}.png"),
                kind: rune_model::ImageKind::Artwork,
            })
            .unwrap()
        })
        .collect();
    let rows = pictures
        .chunks(10)
        .map(|row| {
            limn::Element::row(
                row.iter()
                    .map(|picture| {
                        limn::Element::raster_image(picture.clone()).size(Some(36.0), Some(36.0))
                    })
                    .collect(),
            )
        })
        .collect();
    let frame =
        limn::UiTree::new(limn::Element::column(rows)).layout(limn::Size::new(800.0, 800.0));
    let ctx = egui::Context::default();
    // The pictures' textures each pass uploads, not the font's.
    let pass = || {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            limn_egui::paint_at(ui, &frame, egui::Pos2::ZERO);
        });
        let uploaded = output
            .textures_delta
            .set
            .iter()
            .filter(|(id, _)| **id != egui::TextureId::default())
            .count();
        output.textures_delta.clear();
        uploaded
    };
    let (mut uploaded, mut quiet) = (0, 0);
    for _ in 0..600 {
        let now = pass();
        uploaded += now;
        quiet = if now == 0 { quiet + 1 } else { 0 };
        if uploaded >= pictures.len() && quiet > 30 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(uploaded, pictures.len(), "each picture is decoded once");
    assert!(quiet > 30, "a settled screen uploads nothing more");
}
