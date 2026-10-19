/// A place on a PDF page, as fractions of its width and height from its top
/// left corner: left, top, right, bottom, each from 0 to 1.
pub type PdfMark = [f32; 4];

/// A word of a PDF page, as its text extractor read it, and where it sits.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfWord {
    pub text: String,
    pub bounds: PdfMark,
}
