//! The picker of any color behind the custom swatch
//! ([`rune_components::server_settings::swatches`]). Its field and its hue
//! bar write the color as the hex field would, so each page answers them as
//! it answers that field. Where the thumbs stand is kept here while the
//! picker is used: a gray has no hue of its own and black no saturation, and
//! the picker keeps the ones it had.

use limn::{Color, ColorPicker, Hsv, UiAction};
use rune_model::Rgb;

use crate::{Intent, Ui};

/// A move of the field or the hue bar of the picker in use: the change of
/// its hex field it stands for, and the field's new text.
pub(crate) fn translate(ui: &mut Ui, action: &UiAction) -> Option<(UiAction, Intent)> {
    let picker = match action {
        UiAction::AreaSliderChanged { id, .. } => id.strip_suffix(":area")?,
        UiAction::SliderChanged { id, .. } => id.strip_suffix(":hue")?,
        _ => return None,
    };
    let current = picked(ui, picker)?;
    let next = ColorPicker::color_after(picker, current, action)?;
    ui.color_picker = Some((picker.to_owned(), next));
    let color = Color::from(next);
    let hex = rune_components::server_settings::hex(Rgb(u32::from(color.r) << 16
        | u32::from(color.g) << 8
        | u32::from(color.b)));
    let field = format!("{picker}:hex");
    Some((
        UiAction::InputChanged {
            id: field.clone(),
            value: hex.clone(),
        },
        Intent::SetInput {
            id: field,
            value: hex,
        },
    ))
}

/// Where the thumbs of the picker `id` stand, while it is the one in use.
pub(crate) fn picked(ui: &Ui, id: &str) -> Option<Hsv> {
    ui.color_picker
        .as_ref()
        .filter(|(open, _)| open == id)
        .map(|(_, hsv)| *hsv)
}

/// Follow what the hex field `id` is given by its page, as it does when the
/// picker opens, or typed in it.
pub(crate) fn note(ui: &mut Ui, id: &str, value: &str) {
    let Some(picker) = id.strip_suffix(":hex") else {
        return;
    };
    // The thumbs follow only what the pages take: limn reads `abc` too.
    if rune_components::server_settings::parse_hex(value).is_none() {
        return;
    }
    let typed = UiAction::InputChanged {
        id: id.to_owned(),
        value: value.to_owned(),
    };
    let next = match picked(ui, picker) {
        Some(current) => ColorPicker::color_after(picker, current, &typed),
        None => ColorPicker::parse_hex(value).map(Hsv::from),
    };
    if let Some(next) = next {
        ui.color_picker = Some((picker.to_owned(), next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_and_the_bar_write_hex_and_keep_the_hue_through_gray() {
        let mut ui = Ui::default();
        // Nothing is opened: the moves are not the pickers'.
        let hue = |fraction| UiAction::SliderChanged {
            id: "ss:color:hue".into(),
            fraction,
            committed: true,
        };
        assert!(translate(&mut ui, &hue(0.5)).is_none());
        // The page opens it on a blue.
        note(&mut ui, "ss:color:hex", "0000ff");
        let area = |x, y| UiAction::AreaSliderChanged {
            id: "ss:color:area".into(),
            x,
            y,
            committed: true,
        };
        // To the left edge: a gray, written as such.
        let (typed, shown) = translate(&mut ui, &area(0.0, 0.0)).unwrap();
        assert_eq!(
            typed,
            UiAction::InputChanged {
                id: "ss:color:hex".into(),
                value: "ffffff".into()
            }
        );
        assert_eq!(
            shown,
            Intent::SetInput {
                id: "ss:color:hex".into(),
                value: "ffffff".into()
            }
        );
        // Three digits are not a color the pages take: the thumbs stay.
        note(&mut ui, "ss:color:hex", "abc");
        let kept = picked(&ui, "ss:color").unwrap().hue;
        assert!((kept - 240.0).abs() < 0.5, "#aabbcc would turn it: {kept}");
        // The page answers the white it was given; the blue's hue stays.
        note(&mut ui, "ss:color:hex", "ffffff");
        let (typed, _) = translate(&mut ui, &area(1.0, 0.0)).unwrap();
        assert_eq!(
            typed,
            UiAction::InputChanged {
                id: "ss:color:hex".into(),
                value: "0000ff".into()
            }
        );
        // The bar turns the hue; another picker's moves are not this one's.
        let (typed, _) = translate(&mut ui, &hue(0.0)).unwrap();
        assert!(matches!(typed, UiAction::InputChanged { value, .. } if value == "ff0000"));
        let other = UiAction::SliderChanged {
            id: "us:accent:hue".into(),
            fraction: 0.5,
            committed: true,
        };
        assert!(translate(&mut ui, &other).is_none());
    }
}
