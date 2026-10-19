//! A page rasterizer with no account, home, network or writable runtime mounts.

use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use base64::Engine as _;
use image::{ImageFormat, ImageReader};
use rune_model::{PdfMark, PdfWord};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use super::{DocumentError as Error, ImageData, MAX_DOCUMENT_BYTES};

const MAX_OUTPUT: u64 = 8 * 1024 * 1024;
const MAX_SIDE: u32 = 1200;
/// Page navigation is one page at a time; a larger count is not believed.
const MAX_PAGES: u32 = 9999;
/// What is read of the page counter's report before it is ignored.
const MAX_INFO: u64 = 64 * 1024;
/// What is read of its report on every page: two lines of some 40 bytes
/// for each of [`MAX_PAGES`].
const MAX_ROTATIONS: u64 = 1024 * 1024;
/// The most of the extractor's report read, for a search: each word comes
/// with where it is, about 100 bytes, so some 250 000 words. The rest is not
/// searched.
pub(super) const MAX_REPORT: u64 = 24 * 1024 * 1024;
/// What a highlighter leaves on paper, multiplied into the page: dark text
/// stays dark and white turns yellow.
const HIGHLIGHT: [u16; 3] = [255, 214, 64];
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub(super) struct Tools {
    renderer: &'static str,
    sandbox: &'static str,
    limiter: &'static str,
}

impl Tools {
    /// Only reproducible, absolute runtime paths, never the document or PATH.
    pub fn configured() -> Option<Self> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let tools = Self {
            renderer: option_env!("RUNE_PDF_RENDERER")?,
            sandbox: option_env!("RUNE_PDF_SANDBOX")?,
            limiter: option_env!("RUNE_PDF_LIMITER")?,
        };
        [tools.renderer, tools.sandbox, tools.limiter]
            .iter()
            .all(|path| path.starts_with("/nix/store/") && !path.contains("/../"))
            .then_some(tools)
    }

    /// Poppler's page counter, beside its rasterizer in the same package.
    fn counter(&self) -> String {
        self.beside("pdfinfo")
    }

    /// Poppler's text extractor, beside its rasterizer in the same package.
    fn extractor(&self) -> String {
        self.beside("pdftotext")
    }

    fn beside(&self, tool: &str) -> String {
        let directory = self.renderer.rsplit_once('/').map_or("", |(dir, _)| dir);
        format!("{directory}/{tool}")
    }
}

pub(super) struct Workspace(PathBuf);
impl Workspace {
    fn create() -> Result<Self, Error> {
        for _ in 0..32 {
            let path = std::env::temp_dir().join(format!(
                "rune-pdf-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            #[cfg_attr(not(unix), allow(unused_mut))]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(Error::Failed),
            }
        }
        Err(Error::Failed)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A private directory holding a copy of the document and an empty output.
async fn prepare(bytes: &[u8]) -> Result<(Workspace, PathBuf, PathBuf), Error> {
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(Error::TooLarge);
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(Error::Invalid);
    }
    let workspace = tokio::task::spawn_blocking(Workspace::create)
        .await
        .map_err(|_| Error::Failed)??;
    let input = workspace.0.join("input.pdf");
    let output = workspace.0.join("out");
    tokio::fs::create_dir(&output)
        .await
        .map_err(|_| Error::Failed)?;
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&input).await.map_err(|_| Error::Failed)?;
    file.write_all(bytes).await.map_err(|_| Error::Failed)?;
    file.flush().await.map_err(|_| Error::Failed)?;
    Ok((workspace, input, output))
}

/// `program` limited and isolated, seeing only the document and the output.
/// The caller adds the program's own arguments.
async fn sandboxed(tools: Tools, program: &str, input: &Path, output: &Path) -> Command {
    let mut command = Command::new(tools.limiter);
    command.args([
        "--as=536870912",
        "--cpu=5",
        "--fsize=8388608",
        "--nofile=64",
        "--core=0",
        "--",
        tools.sandbox,
    ]);
    command.args([
        "--die-with-parent",
        "--unshare-all",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--ro-bind",
        "/nix/store",
        "/nix/store",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--dir",
        "/home",
        "--dir",
        "/tmp",
        "--setenv",
        "HOME",
        "/home",
        "--setenv",
        "LANG",
        "C.UTF-8",
    ]);
    // Fontconfig configuration and immutable system font packages only.
    if tokio::fs::try_exists("/etc/fonts").await.unwrap_or(false) {
        command.args(["--ro-bind", "/etc/fonts", "/etc/fonts"]);
    }
    command
        .arg("--ro-bind")
        .arg(input)
        .arg("/input.pdf")
        .arg("--bind")
        .arg(output)
        .arg("/out")
        .args(["--chdir", "/out", "--", program])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}

/// How many pages the document has, by the sandboxed counter's own report;
/// `None` when it cannot tell. Only its `Pages:` line is read.
pub(super) async fn page_count(bytes: Arc<[u8]>, tools: Tools) -> Option<u32> {
    let (_workspace, input, output) = prepare(&bytes).await.ok()?;
    drop(bytes);
    parse_pages(&counted(tools, &input, &output, &[], MAX_INFO).await?)
}

/// The sandboxed counter's report on the document, with `args`, read up
/// to `limit`; `None` when it fails.
async fn counted(
    tools: Tools,
    input: &Path,
    output: &Path,
    args: &[&str],
    limit: u64,
) -> Option<String> {
    let mut command = sandboxed(tools, &tools.counter(), input, output).await;
    command.args(args).arg("/input.pdf").stdout(Stdio::piped());
    let mut child = command.spawn().ok()?;
    let mut report = Vec::new();
    let mut stdout = child.stdout.take()?.take(limit);
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        stdout.read_to_end(&mut report).await.ok()?;
        child.wait().await.ok()
    })
    .await
    .ok()??;
    status
        .success()
        .then(|| String::from_utf8_lossy(&report).into_owned())
}

/// Which pages are shown a quarter turn from how they are stored (`/Rotate`
/// 90 or 270), in order, by the counter's `Page N rot: R` lines; it stops
/// at the first page out of order.
fn turned(report: &str) -> Vec<bool> {
    let mut turned = Vec::new();
    for line in report.lines() {
        let mut words = line.split_whitespace();
        let (Some("Page"), Some(page), Some("rot:"), Some(rotation)) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            continue;
        };
        let (Ok(page), Ok(rotation)) = (page.parse::<usize>(), rotation.parse::<u32>()) else {
            break;
        };
        if page != turned.len() + 1 {
            break;
        }
        turned.push(rotation % 180 == 90);
    }
    turned
}

fn parse_pages(report: &str) -> Option<u32> {
    report
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .and_then(|count| count.trim().parse::<u32>().ok())
        .filter(|count| (1..=MAX_PAGES).contains(count))
}

/// The words of each page, in order and with where each sits, by the
/// sandboxed extractor: what a search looks through and highlights. A
/// document whose report passes [`MAX_REPORT`] has its later pages cut; one
/// without text (scanned) has pages without words.
pub(super) async fn text(bytes: Arc<[u8]>, tools: Tools) -> Result<Vec<Vec<PdfWord>>, Error> {
    let (_workspace, input, output) = prepare(&bytes).await?;
    drop(bytes);
    // The extractor places words on the page as shown, turned, but says its
    // size as stored: each turned page's size is turned back.
    let last = MAX_PAGES.to_string();
    let rotations = counted(
        tools,
        &input,
        &output,
        &["-f", "1", "-l", &last],
        MAX_ROTATIONS,
    )
    .await
    .map_or_else(Vec::new, |report| turned(&report));
    let mut command = sandboxed(tools, &tools.extractor(), &input, &output).await;
    command
        .args(["-bbox", "-enc", "UTF-8", "-q", "/input.pdf", "-"])
        .stdout(Stdio::piped());
    let mut child = command.spawn().map_err(|_| Error::Unavailable)?;
    let mut said = Vec::new();
    let mut stdout = child.stdout.take().ok_or(Error::Failed)?.take(MAX_REPORT);
    let finished = tokio::time::timeout(Duration::from_secs(15), async {
        stdout.read_to_end(&mut said).await.ok()?;
        // Past the limit the rest is not waited for.
        if said.len() as u64 >= MAX_REPORT {
            let _ = child.start_kill();
        }
        child.wait().await.ok()
    })
    .await
    .map_err(|_| Error::Failed)?
    .ok_or(Error::Failed)?;
    if !finished.success() && (said.len() as u64) < MAX_REPORT {
        return Err(Error::Failed);
    }
    Ok(pages_of(&String::from_utf8_lossy(&said), &rotations))
}

/// The value of `name` among a tag's attributes, as a finite number.
fn attribute(attributes: &str, name: &str) -> Option<f32> {
    let start = attributes.find(&format!(" {name}=\""))? + name.len() + 3;
    let value = attributes.get(start..)?;
    value[..value.find('"')?]
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

/// A word as the report escaped it, without control characters.
fn unescape(word: &str) -> String {
    word.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

/// The extractor's `-bbox` report as each page's words, each placed as a
/// fraction of its page as shown, the pages `turned` a quarter turn with
/// their width and height swapped. A word without a sensible place is left out.
fn pages_of(report: &str, turned: &[bool]) -> Vec<Vec<PdfWord>> {
    let mut pages: Vec<Vec<PdfWord>> = Vec::new();
    // Its head holds the PDF's own title and metadata: only its body is read.
    let Some(start) = report.find("<doc>") else {
        return pages;
    };
    let mut rest = &report[start..];
    let mut size = None;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if tag.starts_with("page ") {
            if pages.len() >= MAX_PAGES as usize {
                break;
            }
            let turn = turned.get(pages.len()).copied().unwrap_or(false);
            size = attribute(tag, "width")
                .zip(attribute(tag, "height"))
                .filter(|(width, height)| *width > 0.0 && *height > 0.0)
                .map(|(width, height)| {
                    if turn {
                        (height, width)
                    } else {
                        (width, height)
                    }
                });
            pages.push(Vec::new());
        } else if tag.starts_with("word ") {
            let Some(close) = rest.find("</word>") else {
                break;
            };
            let text = unescape(&rest[..close]);
            rest = &rest[close + "</word>".len()..];
            let (Some(page), Some((width, height))) = (pages.last_mut(), size) else {
                continue;
            };
            let at = |name, side: f32| Some((attribute(tag, name)? / side).clamp(0.0, 1.0));
            let bounds = [
                at("xMin", width),
                at("yMin", height),
                at("xMax", width),
                at("yMax", height),
            ];
            if let [Some(left), Some(top), Some(right), Some(bottom)] = bounds {
                if !text.trim().is_empty() && left <= right && top <= bottom {
                    page.push(PdfWord {
                        text,
                        bounds: [left, top, right, bottom],
                    });
                }
            }
        }
    }
    pages
}

/// Page `page`, counting from 1, at most 1200 px on its longer side, with
/// `marks` highlighted on it.
pub(super) async fn page(
    bytes: Arc<[u8]>,
    page: u32,
    tools: Tools,
    marks: Vec<PdfMark>,
) -> Result<ImageData, Error> {
    if page == 0 || page > MAX_PAGES {
        return Err(Error::Invalid);
    }
    let (workspace, input, output) = prepare(&bytes).await?;
    drop(bytes);
    let page = page.to_string();
    let mut command = sandboxed(tools, tools.renderer, &input, &output).await;
    command.args([
        "-f",
        &page,
        "-l",
        &page,
        "-singlefile",
        "-scale-to",
        "1200",
        "-png",
        "-q",
        "/input.pdf",
        "/out/page",
    ]);
    let mut child = command.spawn().map_err(|_| Error::Unavailable)?;
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| Error::Failed)?
        .map_err(|_| Error::Failed)?;
    if !status.success() {
        return Err(Error::Failed);
    }
    let path = output.join("page.png");
    let size = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| Error::Failed)?;
    if !size.is_file() || size.len() > MAX_OUTPUT {
        return Err(Error::Failed);
    }
    let png = tokio::fs::read(&path).await.map_err(|_| Error::Failed)?;
    drop(workspace);
    if png.len() as u64 > MAX_OUTPUT {
        return Err(Error::Failed);
    }
    tokio::task::spawn_blocking(move || normalize(png, &marks))
        .await
        .map_err(|_| Error::Failed)?
}

/// `marks` over `page`, as a highlighter over paper.
fn highlight(mut page: image::RgbaImage, marks: &[PdfMark]) -> image::RgbaImage {
    let (width, height) = page.dimensions();
    let at = |fraction: f32, side: u32| (fraction.clamp(0.0, 1.0) * side as f32).round() as u32;
    for &[left, top, right, bottom] in marks {
        for y in at(top, height)..at(bottom, height).min(height) {
            for x in at(left, width)..at(right, width).min(width) {
                let pixel = page.get_pixel_mut(x, y);
                for (channel, tint) in pixel.0.iter_mut().zip(HIGHLIGHT) {
                    *channel = (u16::from(*channel) * tint / 255) as u8;
                }
            }
        }
    }
    page
}

pub(super) fn normalize(bytes: Vec<u8>, marks: &[PdfMark]) -> Result<ImageData, Error> {
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(Error::Failed);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let mut decoded = reader.decode().map_err(|_| Error::Failed)?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err(Error::Failed);
    }
    if !marks.is_empty() {
        decoded = image::DynamicImage::ImageRgba8(highlight(decoded.to_rgba8(), marks));
    }
    let mut png = Cursor::new(Vec::new());
    decoded
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| Error::Failed)?;
    let bytes = png.into_inner();
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(Error::Failed);
    }
    Ok(ImageData {
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        size_bytes: bytes.len() as u32,
        width: decoded.width(),
        height: decoded.height(),
        base64: Arc::from(base64::engine::general_purpose::STANDARD.encode(bytes)),
        animation: None,
        format: crate::images::Encoding::Png,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_word_is_placed_on_its_page_and_strangers_are_left_out() {
        // As Poppler's pdftotext -bbox reports it; the head is the PDF's own.
        let report = r#"<html><head><title>&lt;page width="9" height="9"&gt;</title></head>
<body><doc>
  <page width="200.000000" height="100.000000">
    <word xMin="20.000000" yMin="10.000000" xMax="60.000000" yMax="30.000000">Hola</word>
    <word xMin="70.000000" yMin="10.000000" xMax="90.000000" yMax="30.000000">&amp;&lt;b&gt;</word>
    <word xMin="nan" yMin="10.000000" xMax="90.000000" yMax="30.000000">roto</word>
    <word xMin="95.000000" yMin="10.000000" xMax="400.000000" yMax="30.000000">ancho</word>
  </page>
  <page width="0" height="100.000000">
    <word xMin="1.000000" yMin="1.000000" xMax="2.000000" yMax="2.000000">sin lugar</word>
  </page>
  <page width="200.000000" height="100.000000">
    <word xMin="1.000000" yMin="1.000000" xMax="2.000000" yMax="2.000000">cortad"#;
        let pages = pages_of(report, &[]);
        assert_eq!(pages.len(), 3);
        let words: Vec<&str> = pages[0].iter().map(|word| word.text.as_str()).collect();
        assert_eq!(words, ["Hola", "&<b>", "ancho"]);
        assert_eq!(pages[0][0].bounds, [0.1, 0.1, 0.3, 0.3]);
        assert_eq!(pages[0][2].bounds[2], 1.0, "kept within its page");
        assert!(pages[1].is_empty() && pages[2].is_empty());
        assert!(pages_of("", &[]).is_empty());
    }

    #[test]
    fn a_turned_page_places_its_words_on_the_page_as_shown() {
        // As Poppler says it of a 240×120 page with /Rotate 90: the size as
        // stored, the word where it shows on the page 120 wide, 240 tall.
        let report = r#"<doc>
  <page width="240.000000" height="120.000000">
    <word xMin="78.000000" yMin="20.000000" xMax="90.000000" yMax="44.000000">Hola</word>
  </page>
  <page width="240.000000" height="120.000000">
    <word xMin="24.000000" yMin="12.000000" xMax="48.000000" yMax="24.000000">Hola</word>
  </page>
</doc>"#;
        let pages = pages_of(report, &[true]);
        assert_eq!(pages[0][0].bounds, [0.65, 20.0 / 240.0, 0.75, 44.0 / 240.0]);
        assert_eq!(
            pages[1][0].bounds,
            [0.1, 0.1, 0.2, 0.2],
            "only the first turned"
        );
        let counted = "Title:          Page 7 rot: 90\nPages:          3\n\
            Page    1 size: 240 x 120 pts\nPage    1 rot:  90\n\
            Page    2 size: 240 x 120 pts\nPage    2 rot:  180\n\
            Page    3 rot:  270\nPage    9 rot:  90\n";
        assert_eq!(turned(counted), [true, false, true]);
        assert!(turned("Pages: 2\n").is_empty());
    }

    #[test]
    fn a_highlight_tints_only_its_place_as_a_marker_would() {
        let page = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        let marked = highlight(page, &[[0.2, 0.2, 0.5, 0.4], [0.9, 0.9, 2.0, 2.0]]);
        assert_eq!(marked.get_pixel(2, 2).0, [255, 214, 64, 255]);
        assert_eq!(marked.get_pixel(4, 3).0, [255, 214, 64, 255]);
        assert_eq!(marked.get_pixel(5, 3).0, [255; 4], "right edge excluded");
        assert_eq!(marked.get_pixel(2, 4).0, [255; 4]);
        assert_eq!(marked.get_pixel(9, 9).0, [255, 214, 64, 255]);
        let black = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]));
        assert_eq!(
            highlight(black, &[[0.0, 0.0, 1.0, 1.0]]).get_pixel(1, 1).0,
            [0, 0, 0, 255],
            "text stays dark"
        );
    }

    #[test]
    fn only_a_plausible_pages_line_counts() {
        assert_eq!(
            parse_pages("Title: x\nPages:          12\nEncrypted: no\n"),
            Some(12)
        );
        assert_eq!(parse_pages("Title: Pages: 3\n"), None);
        assert_eq!(parse_pages("Pages: 0\n"), None);
        assert_eq!(parse_pages("Pages: 99999\n"), None);
        assert_eq!(parse_pages("Pages: -1\n"), None);
        assert_eq!(parse_pages(""), None);
    }
}
