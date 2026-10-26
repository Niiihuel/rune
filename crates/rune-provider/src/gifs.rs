//! Search results for an optional expression service, independent of chat
//! accounts, or for the chat service's own GIF search (P5).

use std::sync::Arc;

use rune_i18n::t;
use rune_model::RemoteImage;

#[derive(Clone, PartialEq, Eq)]
pub enum GifQuery {
    Trending,
    Search(String),
}

impl std::fmt::Debug for GifQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Trending => "Trending",
            Self::Search(_) => "Search([REDACTED])",
        })
    }
}

/// Pixels retained only for the currently displayed page, never persisted.
#[derive(Clone, PartialEq, Eq)]
pub struct GifThumbnail {
    pub sha256: String,
    pub size_bytes: u32,
    pub width: u32,
    pub height: u32,
    pub png_base64: Arc<str>,
}

impl std::fmt::Debug for GifThumbnail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GifThumbnail")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifItem {
    pub id: String,
    pub title: String,
    pub creator: Option<String>,
    /// Service-supplied share URL; insertion never implicitly sends it.
    pub url: String,
    /// Kept unmodified; not submitted to the reusable artwork cache.
    pub preview: RemoteImage,
    pub thumbnail: Option<GifThumbnail>,
}

/// A GIF among the account's favorites, wherever it was found.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FavoriteGif {
    /// The address a message carries; it names the favorite.
    pub url: String,
    /// The picture or clip that shows it, when known.
    #[serde(default)]
    pub preview: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifAttribution {
    pub label: String,
    pub url: String,
    /// Official service-supplied attribution artwork, when provided.
    pub image: Option<GifThumbnail>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifPage {
    pub items: Vec<GifItem>,
    pub next_offset: Option<u32>,
    pub attribution: GifAttribution,
    /// What is trending to search for, with a picture, on a trending page
    /// of a service that has them; searching one searches its name.
    pub categories: Vec<GifCategory>,
}

/// A trending search term with a picture of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifCategory {
    pub name: String,
    pub image: Option<RemoteImage>,
}

/// Categories kept on a page, at most, and suggestions.
pub const MAX_GIF_CATEGORIES: usize = 32;
pub const MAX_GIF_SUGGESTIONS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GifError {
    NotConfigured,
    InvalidQuery,
    Unavailable,
    InvalidResponse,
    AccessDenied,
    RateLimited { retry_after_secs: u64 },
}

impl GifError {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotConfigured => t("El buscador de GIF no está configurado."),
            Self::InvalidQuery => t("Busca con un texto de hasta 50 caracteres."),
            Self::Unavailable => t("No se pudo cargar la búsqueda. Vuelve a intentarlo."),
            Self::InvalidResponse => t("El buscador devolvió una respuesta no compatible."),
            Self::AccessDenied => t("La configuración del buscador no tiene acceso."),
            Self::RateLimited { .. } => t("Espera antes de volver a buscar."),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifResult {
    pub request: crate::RequestId,
    pub query: GifQuery,
    pub offset: u32,
    pub result: Result<GifPage, GifError>,
}
