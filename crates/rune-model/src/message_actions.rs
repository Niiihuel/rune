//! What message actions add to the domain (P4): how a moment written in a
//! message is shown to each reader.

use serde::{Deserialize, Serialize};

/// How a moment written in a message reads, in the reader's own zone.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TimeStyle {
    /// «22:13».
    ShortTime,
    /// «22:13:20».
    LongTime,
    /// «14/11/2023».
    ShortDate,
    /// «14 de noviembre de 2023».
    LongDate,
    /// «14 de noviembre de 2023 22:13», when no style is written.
    #[default]
    ShortDateTime,
    /// «martes, 14 de noviembre de 2023 22:13».
    LongDateTime,
    /// «hace 3 minutos», «en 2 días»: it changes as time goes by.
    Relative,
}
