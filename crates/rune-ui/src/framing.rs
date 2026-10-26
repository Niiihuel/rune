//! Framing a picture chosen for the profile (edicion-de-perfil): the whole
//! picture with the part that is kept, moved by dragging over it and
//! brought closer with a slider. The frame follows the pointer here; once a
//! move settles, the host frames the picture again
//! ([`Intent::FrameProfileImage`]) and the draft changes when it answers.
//! Nothing leaves until the profile is saved.

use limn::UiAction;
use rune_components::profile::{ids, EditorFraming};
use rune_i18n::{t, tr};
use rune_provider::{Crop, ProfileImageKind};
use rune_state::Store;

use crate::profile::Editor;
use crate::{Intent, Ui};

/// The whole of a picture just chosen, as the host gives it.
#[derive(Clone, Debug)]
pub struct PictureSource {
    pub image: limn::RasterImage,
    /// The size of `image`, for the frame's shape.
    pub width: u32,
    pub height: u32,
    /// Whether the picture moves.
    pub animated: bool,
}

/// The picture being framed, while the editor holds it.
#[derive(Clone, Debug)]
pub struct Framing {
    pub kind: ProfileImageKind,
    pub source: PictureSource,
    /// The frame drawn, which follows the pointer.
    pub crop: Crop,
    /// The frame of the picture in the draft: one that fails goes back to it.
    pub kept: Crop,
    /// Whether the picture in the draft moves.
    pub animated: bool,
    /// Asked to keep moving, it came back still: too many frames, too slow
    /// or too heavy. It is framed still from then on.
    pub heavy: bool,
    /// The frame asked for and not answered yet: its request and the frame.
    pub pending: Option<(u64, Crop)>,
}

impl Framing {
    pub(crate) fn new(
        kind: ProfileImageKind,
        source: PictureSource,
        animated: bool,
        heavy: bool,
    ) -> Self {
        Self {
            kind,
            source,
            crop: Crop::default(),
            kept: Crop::default(),
            animated,
            heavy,
            pending: None,
        }
    }
}

/// Whether a moving picture of `kind` may stay moving in the profile being
/// edited, as its abilities say.
pub(crate) fn may_move(
    store: &Store,
    server: Option<rune_model::ServerId>,
    kind: ProfileImageKind,
) -> bool {
    let abilities = store.profile_abilities(server);
    match kind {
        ProfileImageKind::Avatar => abilities.animated.allowed(),
        // A server's banner is not the profile's (2026-10-19); never here.
        ProfileImageKind::Banner | ProfileImageKind::ServerBanner => abilities.banner.allowed(),
    }
}

/// A move of the frame or of its zoom; `None` for any other action.
pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let committed = match action {
        UiAction::AreaSliderChanged {
            id,
            x,
            y,
            committed,
        } if id == ids::FRAME => {
            let framing = framing(ui)?;
            framing.crop.x = *x;
            framing.crop.y = *y;
            *committed
        }
        UiAction::SliderChanged {
            id,
            fraction,
            committed,
        } if id == ids::FRAME_ZOOM => {
            let framing = framing(ui)?;
            framing.crop.zoom = 1.0 + fraction.clamp(0.0, 1.0) * (Crop::MAX_ZOOM - 1.0);
            *committed
        }
        _ => return None,
    };
    if !committed {
        return Some(Vec::new());
    }
    let request = ui.fresh_request();
    let editor = ui.profiles.editor.as_mut()?;
    let server = editor.server;
    let framing = editor.framing.as_mut()?;
    framing.pending = Some((request, framing.crop));
    let (kind, crop) = (framing.kind, framing.crop);
    // As the account may now: its plan may have arrived since the choice.
    let animated = framing.source.animated && !framing.heavy && may_move(store, server, kind);
    // The answer comes as a choice's does, in its place; a newer frame
    // replaces an older one still on its way.
    editor.picking = Some((request, kind, animated));
    Some(vec![Intent::FrameProfileImage {
        request,
        kind,
        crop,
        animated,
    }])
}

/// The framing of the open editor, unless it is saving or a picture is
/// being chosen: only another frame may be on its way.
fn framing(ui: &mut Ui) -> Option<&mut Framing> {
    let editor = ui.profiles.editor.as_mut()?;
    let pending = editor
        .framing
        .as_ref()
        .and_then(|framing| framing.pending)
        .map(|(request, _)| request);
    let free = !editor.saving
        && editor
            .picking
            .is_none_or(|(request, ..)| pending == Some(request));
    editor.framing.as_mut().filter(|_| free)
}

/// What the editor shows of the picture being framed; `reason` says why the
/// account may not keep a moving one moving, as it stands now.
pub(crate) fn view(editor: &Editor, reason: Option<&str>) -> Option<EditorFraming> {
    let framing = editor.framing.as_ref()?;
    let source = &framing.source;
    let (width, height) = (source.width.max(1) as f32, source.height.max(1) as f32);
    let [left, top, wide, high] = framing.crop.frame(width, height, framing.kind);
    let first = |reason: &str| {
        tr!(
            "{reason}. Se usa sólo el primer cuadro.",
            reason = reason.trim_end_matches('.')
        )
    };
    let note = match (source.animated, framing.animated) {
        (false, _) => None,
        (true, true) => Some(match reason {
            // Chosen before the plan said no: framed again, it stays still.
            Some(reason) => tr!(
                "{reason}. Mueve el encuadre para usar sólo el primer cuadro.",
                reason = reason.trim_end_matches('.')
            ),
            None => {
                t("Se mueve: se sube como GIF, con el mismo encuadre en cada cuadro.").to_owned()
            }
        }),
        (true, false) if framing.heavy => Some(first(t(
            "La animación es demasiado larga o pesada para un perfil.",
        ))),
        // Without a reason, the plan allows it now: the next frame moves.
        (true, false) => reason.map(first),
    };
    Some(EditorFraming {
        image: source.image.clone(),
        aspect: width / height,
        frame: [left / width, top / height, wide / width, high / height],
        x: framing.crop.x,
        y: framing.crop.y,
        zoom: (framing.crop.zoom - 1.0) / (Crop::MAX_ZOOM - 1.0),
        round: framing.kind == ProfileImageKind::Avatar,
        note,
    })
}
