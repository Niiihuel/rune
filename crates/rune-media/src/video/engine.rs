//! Who encodes and decodes video: OpenH264, always there, or one of the
//! GPU's engines ([`super::gpu`]), tried in their order.

/// Who encodes or decodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    /// OpenH264, on the CPU: always there.
    #[default]
    Software,
    /// VA-API, through Mesa.
    VaApi,
    /// NVENC to encode, NVDEC to decode.
    Nvidia,
}

/// The GPU's engines in the order they are tried, OpenH264 after them: the
/// integrated GPU's VA-API first, as its pictures need not cross the bus.
pub const HARDWARE: &[Engine] = &[Engine::VaApi, Engine::Nvidia];

/// The engines after `engine` in `engines`: what is left once it failed.
pub(crate) fn after(engines: &'static [Engine], engine: Engine) -> &'static [Engine] {
    engines
        .iter()
        .position(|next| *next == engine)
        .map_or(engines, |at| &engines[at + 1..])
}

impl Engine {
    /// Its name as an encoder, for the preview and the diagnostic line.
    pub fn encoder(self) -> &'static str {
        match self {
            Self::Software => "OpenH264",
            Self::VaApi => "VA-API",
            Self::Nvidia => "NVENC",
        }
    }

    /// Its name as a decoder.
    pub fn decoder(self) -> &'static str {
        match self {
            Self::Software => "OpenH264",
            Self::VaApi => "VA-API",
            Self::Nvidia => "NVDEC",
        }
    }
}
