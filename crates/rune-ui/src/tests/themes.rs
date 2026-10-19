//! Own themes, the main color and the window's transparency
//! (temas-propios), driven through Ajustes › Apariencia over the simulation.

use super::*;
use limn::Color;
use rune_components::Look;

use crate::themes;

fn fills(frame: &Frame, color: Color) -> usize {
    frame
        .commands
        .iter()
        .filter(|command| {
            matches!(command, limn::DrawCommand::FillRect { color: found, .. } if *found == color)
        })
        .count()
}

impl Host {
    fn appearance(&mut self) {
        self.click("me:settings");
        self.click("us:page=appearance");
    }

    fn theme_act(&mut self, action: UiAction) {
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
    }

    /// Bring the control `id` to the top of the settings, and click it.
    fn theme_click(&mut self, id: &str) {
        self.frame();
        self.tree.reveal_at_start(id, 0.0);
        self.click(id);
    }
}

#[test]
fn a_theme_is_made_with_its_preview_and_the_window_changes_on_saving() {
    let mut host = Host::new();
    host.appearance();
    host.click("us:theme=dark");
    host.click(themes::CREATE);
    let painted = host.painted();
    for text in ["Crear tema", "Base", "Así se ve tu tema."] {
        assert!(painted.iter().any(|shown| shown == text), "{text}");
    }
    // The conversation's color, from its picker's field.
    host.theme_click("us:theme-token:canvas");
    assert_eq!(
        host.ui.themes.editor.as_ref().unwrap().picking,
        Some("canvas")
    );
    host.theme_act(UiAction::InputChanged {
        id: "us:theme-token:canvas:hex".into(),
        value: "203040".into(),
    });
    let chosen = Color::rgb(0x20, 0x30, 0x40);
    host.theme_click("us:theme-token:canvas:dismiss");
    host.tree.reveal_at_start("us:theme-token:rail", 0.0);
    assert!(
        fills(&host.frame(), chosen) >= 1,
        "the preview and the swatch"
    );
    assert_eq!(host.ui.look(), Look::DARK, "the window waits for Guardar");
    host.theme_click(themes::SAVE);
    assert!(host.ui.themes.editor.is_none());
    assert_eq!(host.ui.look().canvas, chosen);
    assert_eq!(host.ui.look().sidebar, Look::DARK.sidebar);
    // It joins the looks to choose from, chosen; a built-in one leaves it.
    host.frame();
    host.tree.reveal_at_start("us:theme=light", 0.0);
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "us:theme-use=0"));
    host.click("us:theme=light");
    assert_eq!(host.ui.look(), Look::LIGHT);
    host.click("us:theme-use=0");
    assert_eq!(host.ui.look().canvas, chosen);
    let next = {
        let kept = host.ui.settings_to_keep();
        let mut next = Ui::default();
        next.restore(|key| {
            kept.iter()
                .find(|(kept, _)| *kept == key)
                .and_then(|(_, value)| value.clone())
        });
        next
    };
    assert_eq!(next.look(), host.ui.look(), "kept for the next run");
}

#[test]
fn the_main_color_tints_the_window_from_its_presets() {
    let mut host = Host::new();
    host.appearance();
    let blue = Color::rgb(0x34, 0x98, 0xdb);
    host.frame();
    host.tree.reveal_at_start("us:theme-accent:3498db", 0.0);
    // Only its swatch, before.
    let before = fills(&host.frame(), blue);
    host.click("us:theme-accent:3498db");
    assert_eq!(host.ui.look().accent, blue);
    assert!(fills(&host.frame(), blue) > before, "the page in it");
    assert_eq!(
        host.ui
            .settings_to_keep()
            .into_iter()
            .find(|(key, _)| *key == "accent_color"),
        Some(("accent_color", Some("#3498db".into())))
    );
}

#[test]
fn a_see_through_window_paints_its_ground_once_and_keeps_the_settings_opaque() {
    let mut host = Host::new();
    host.ui.theme = Theme::Dark;
    assert!(fills(&host.frame(), Look::DARK.sidebar) > 0);
    themes::window_known(&mut host.ui, 40, true);
    let glass = host.frame();
    assert_eq!(fills(&glass, Look::DARK.backdrop(40)), 1, "one ground");
    assert_eq!(
        fills(&glass, Look::DARK.sidebar),
        0,
        "the columns paint nothing"
    );
    host.appearance();
    assert!(
        fills(&host.frame(), Look::DARK.canvas) > 0,
        "the settings are opaque"
    );
    // The page says it applies now, and who blurs.
    let painted = host.painted().join(" ");
    assert!(painted.contains("KWin o Hyprland"), "{painted}");
    assert!(!painted.contains("Se aplica al volver a abrir RUNE."));
}
