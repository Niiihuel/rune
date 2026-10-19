//! Explicit, account-local PDF previews. Only rasterized pixels enter this layer.

use std::sync::Arc;

use limn::{
    Component, ControlSize, Element, Icon, ImageViewer, Key, ListRow, Modal, RasterImage,
    SearchField, UiAction, UiEvent,
};
use rune_components::{
    message::{attachment_ids, attachment_is_pdf},
    Look,
};
use rune_i18n::{t, tr};
use rune_model::{Attachment, AttachmentId, ChannelId, MessageId, PdfMark, PdfWord, UserId};
use rune_state::Store;

use crate::{editor_id, Intent, Ui, ViewContext};

const CLOSE: &str = "document:close";
const IN: &str = "document:zoom-in";
const OUT: &str = "document:zoom-out";
const FIT: &str = "document:fit";
const PREVIOUS: &str = "document:previous";
const NEXT: &str = "document:next";
const SEARCH: &str = "document:search";
const SEARCH_CLOSE: &str = "document:search:close";
const QUERY: &str = "document:search:query";
/// A result's control is this and its page.
const RESULT: &str = "document:search:page:";
/// The most pages a search lists.
const MAX_RESULTS: usize = 50;
/// The most places highlighted on one page.
const MAX_MARKS: usize = 256;
/// Rendered pages kept while the viewer stays open, the most recent last.
pub const KEPT_PAGES: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentViewer {
    account: Option<UserId>,
    channel: ChannelId,
    message: MessageId,
    attachment: AttachmentId,
    request: u64,
    zoom: f32,
    /// The page shown, counting from 1.
    pub page: u32,
    /// 1 until the first page arrives with the document's count.
    pub pages: u32,
    rendered: Vec<(u32, RasterImage)>,
    /// A page rendered with the search's highlights, and which.
    marked: Option<(u32, Vec<PdfMark>, RasterImage)>,
    /// Why the page shown could not be rendered.
    error: Option<String>,
    /// A search through its text, once asked for.
    pub search: Option<DocumentSearch>,
}

/// A search through a document's text, read in the same sandbox as its pages.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentSearch {
    pub query: String,
    /// Each page's words once the host read them, or why it could not.
    pub pages: Option<Result<Arc<[Vec<PdfWord>]>, String>>,
    /// Its panel is open over the page.
    pub open: bool,
}

/// A page's words as one line of text, as the search reads it.
fn line(words: &[PdfWord]) -> String {
    words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` as a search compares it: each character lowercased on its own,
/// so a place in it is the same place in `text`, and spaces made plain.
fn folded(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| {
            if c.is_whitespace() {
                ' '
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect()
}

/// Each place `needle` starts in `text`, one after another.
fn found(text: &[char], needle: &[char]) -> Vec<usize> {
    let mut places = Vec::new();
    let mut at = 0;
    while !needle.is_empty() && at + needle.len() <= text.len() {
        if text[at..at + needle.len()] == *needle {
            places.push(at);
            at += needle.len();
        } else {
            at += 1;
        }
    }
    places
}

impl DocumentSearch {
    /// What is looked for, folded, its spaces one between words.
    fn needle(&self) -> Vec<char> {
        folded(&self.query.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    /// The pages that say `query`, ignoring case, each with a few words
    /// around the first place it does: at most [`MAX_RESULTS`].
    pub fn results(&self) -> Vec<(u32, String)> {
        let needle = self.needle();
        let Some(Ok(pages)) = &self.pages else {
            return Vec::new();
        };
        if needle.is_empty() {
            return Vec::new();
        }
        pages
            .iter()
            .enumerate()
            .filter_map(|(index, words)| {
                let flat = line(words);
                let before = *found(&folded(&flat), &needle).first()?;
                let start = before.saturating_sub(40);
                let snippet: String = flat.chars().skip(start).take(120).collect();
                let mut words: Vec<&str> = snippet.split_whitespace().collect();
                // A word cut at the start reads as a broken one.
                if start > 0 && words.len() > 1 {
                    words.remove(0);
                }
                let snippet = words.join(" ");
                Some((index as u32 + 1, format!("…{snippet}…")))
            })
            .take(MAX_RESULTS)
            .collect()
    }

    /// Where `query` is on page `page`, counting from 1: a mark over each
    /// word a place covers, narrowed to the letters it covers, at most
    /// [`MAX_MARKS`].
    pub fn marks(&self, page: u32) -> Vec<PdfMark> {
        let needle = self.needle();
        let Some(Ok(pages)) = &self.pages else {
            return Vec::new();
        };
        let Some(words) = (page as usize).checked_sub(1).and_then(|at| pages.get(at)) else {
            return Vec::new();
        };
        // Where each word starts in the line, and how many letters it has.
        let spans: Vec<(usize, usize)> = words
            .iter()
            .scan(0, |start, word| {
                let length = word.text.chars().count();
                let at = *start;
                *start += length + 1;
                Some((at, length))
            })
            .collect();
        let mut marks = Vec::new();
        // Places come in order: a word wholly before one is before the rest.
        let mut first = 0;
        for at in found(&folded(&line(words)), &needle) {
            let end = at + needle.len();
            while spans
                .get(first)
                .is_some_and(|(start, length)| start + length <= at)
            {
                first += 1;
            }
            let covered = words[first..]
                .iter()
                .zip(&spans[first..])
                .take_while(|(_, (start, _))| *start < end);
            for (word, &(start, length)) in covered {
                let (from, to) = (at.max(start), end.min(start + length));
                if from < to {
                    // Letters are taken as equally wide.
                    let [left, top, right, bottom] = word.bounds;
                    let x = |letters: usize| {
                        left + (right - left) * (letters - start) as f32 / length as f32
                    };
                    marks.push([x(from), top, x(to), bottom]);
                    if marks.len() == MAX_MARKS {
                        return marks;
                    }
                }
            }
        }
        marks
    }
}

impl DocumentViewer {
    /// The pages already rendered, the most recently shown last.
    pub fn rendered(&self) -> impl Iterator<Item = u32> + '_ {
        self.rendered.iter().map(|(page, _)| *page)
    }

    /// What the search highlights on the page shown.
    fn marks(&self) -> Vec<PdfMark> {
        self.search
            .as_ref()
            .map_or_else(Vec::new, |search| search.marks(self.page))
    }

    /// The page shown with its highlights once they are drawn; as it is
    /// meanwhile, or without any.
    fn shown(&self) -> Option<&RasterImage> {
        let marks = self.marks();
        let marked = self
            .marked
            .as_ref()
            .filter(|(page, kept, _)| !marks.is_empty() && *page == self.page && *kept == marks);
        marked.map(|(.., image)| image).or_else(|| {
            self.rendered
                .iter()
                .find(|(page, _)| *page == self.page)
                .map(|(_, image)| image)
        })
    }

    /// Ask for the page shown as it should look, unless it is at hand: with
    /// the search's highlights when it has any.
    fn ask(&mut self) -> Vec<Intent> {
        let marks = self.marks();
        if marks.is_empty() {
            if let Some(index) = self
                .rendered
                .iter()
                .position(|(kept, _)| *kept == self.page)
            {
                let kept = self.rendered.remove(index);
                self.rendered.push(kept);
                return Vec::new();
            }
        } else if self
            .marked
            .as_ref()
            .is_some_and(|(page, kept, _)| *page == self.page && *kept == marks)
        {
            return Vec::new();
        }
        vec![Intent::DocumentPage {
            request: self.request,
            page: self.page,
            marks,
        }]
    }
}

fn attachment(
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    id: AttachmentId,
) -> Option<&Attachment> {
    store
        .channel(channel)
        .filter(|channel| Store::visible(channel))?;
    store
        .timeline(channel)?
        .get(message)?
        .files()
        .find(|attachment| {
            attachment.id == id && attachment.download.is_some() && attachment_is_pdf(attachment)
        })
}

fn current<'a>(ui: &Ui, store: &'a Store) -> Option<&'a Attachment> {
    let viewer = ui.document_viewer.as_ref()?;
    (viewer.account == store.me() && Some(viewer.channel) == ui.channel()).then_some(())?;
    attachment(store, viewer.channel, viewer.message, viewer.attachment)
}

pub(crate) fn close(ui: &mut Ui) -> Vec<Intent> {
    let Some(viewer) = ui.document_viewer.take() else {
        return Vec::new();
    };
    let mut intents = vec![Intent::CancelDocument {
        request: viewer.request,
    }];
    if let Some(channel) = ui.channel() {
        intents.push(Intent::Focus(editor_id(channel)));
    }
    intents
}

pub(crate) fn refresh(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    if ui.document_viewer.is_some() && current(ui, store).is_none() {
        close(ui)
    } else {
        Vec::new()
    }
}

/// A page arrived, or failed, for `request`, with `marks` highlighted on it;
/// `pages` is the document's count.
pub fn document_result(
    ui: &mut Ui,
    store: &Store,
    request: u64,
    page: u32,
    pages: u32,
    marks: Vec<PdfMark>,
    result: Result<RasterImage, String>,
) -> Vec<Intent> {
    let intents = refresh(ui, store);
    let Some(viewer) = ui
        .document_viewer
        .as_mut()
        .filter(|viewer| viewer.request == request)
    else {
        return intents;
    };
    viewer.pages = pages.max(1);
    match result {
        Ok(image) if !marks.is_empty() => {
            viewer.marked = Some((page, marks, image));
            if page == viewer.page {
                viewer.error = None;
            }
        }
        Ok(image) => {
            viewer.rendered.retain(|(kept, _)| *kept != page);
            viewer.rendered.push((page, image));
            if viewer.rendered.len() > KEPT_PAGES {
                viewer.rendered.remove(0);
            }
            if page == viewer.page {
                viewer.error = None;
            }
        }
        Err(error) if page == viewer.page => viewer.error = Some(error),
        Err(_) => {}
    }
    intents
}

/// The text of the document open for `request` arrived, or could not be read.
pub fn document_text(ui: &mut Ui, request: u64, result: Result<Arc<[Vec<PdfWord>]>, String>) {
    if let Some(search) = ui
        .document_viewer
        .as_mut()
        .filter(|viewer| viewer.request == request)
        .and_then(|viewer| viewer.search.as_mut())
    {
        search.pages = Some(result);
    }
}

/// Show the page `step` away, asking for it unless it is already rendered.
fn turn(ui: &mut Ui, step: i64) -> Vec<Intent> {
    let Some(viewer) = ui.document_viewer.as_ref() else {
        return Vec::new();
    };
    go(ui, i64::from(viewer.page) + step)
}

/// Show page `page`, asking for it unless it is already rendered.
fn go(ui: &mut Ui, page: i64) -> Vec<Intent> {
    let Some(viewer) = ui.document_viewer.as_mut() else {
        return Vec::new();
    };
    if page < 1 || page > i64::from(viewer.pages) {
        return Vec::new();
    }
    viewer.page = page as u32;
    viewer.zoom = 1.0;
    viewer.error = None;
    viewer.ask()
}

/// Open the search over the page, reading the document's text the first time.
fn search(ui: &mut Ui) -> Vec<Intent> {
    let Some(viewer) = ui.document_viewer.as_mut() else {
        return Vec::new();
    };
    let mut intents = vec![Intent::Focus(QUERY.into())];
    match viewer.search.as_mut() {
        Some(search) => search.open = true,
        None => {
            viewer.search = Some(DocumentSearch {
                query: String::new(),
                pages: None,
                open: true,
            });
            intents.push(Intent::DocumentText {
                request: viewer.request,
            });
        }
    }
    intents
}

/// Close the search over the page, which shows what it found there.
fn close_search(ui: &mut Ui) -> Vec<Intent> {
    let Some(viewer) = ui.document_viewer.as_mut() else {
        return Vec::new();
    };
    if let Some(search) = viewer.search.as_mut() {
        search.open = false;
    }
    let mut intents = viewer.ask();
    intents.push(Intent::Focus(SEARCH.into()));
    intents
}

pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    if let UiAction::InputChanged { id, value } = action {
        let search = ui
            .document_viewer
            .as_mut()
            .and_then(|viewer| viewer.search.as_mut())
            .filter(|_| id == QUERY)?;
        search.query = value.chars().take(256).collect();
        return Some(Vec::new());
    }
    let UiAction::Clicked { id } = action else {
        return None;
    };
    if let Some(page) = id
        .strip_prefix(RESULT)
        .and_then(|page| page.parse::<i64>().ok())
    {
        // Closed first, so only the page it goes to is asked for.
        if let Some(search) = ui
            .document_viewer
            .as_mut()
            .and_then(|viewer| viewer.search.as_mut())
        {
            search.open = false;
        }
        let mut intents = go(ui, page);
        intents.push(Intent::Focus(SEARCH.into()));
        return Some(intents);
    }
    match id.as_str() {
        CLOSE => return Some(close(ui)),
        SEARCH => return Some(search(ui)),
        SEARCH_CLOSE => return Some(close_search(ui)),
        PREVIOUS => return Some(turn(ui, -1)),
        NEXT => return Some(turn(ui, 1)),
        IN | OUT | FIT => {
            if let Some(viewer) = ui.document_viewer.as_mut() {
                viewer.zoom = match id.as_str() {
                    IN => (viewer.zoom * 1.5).min(ImageViewer::MAX_ZOOM),
                    OUT => (viewer.zoom / 1.5).max(1.0),
                    _ => 1.0,
                };
            }
            return Some(Vec::new());
        }
        _ => {}
    }
    let (message, id, verb) = attachment_ids::parse(id)?;
    if verb != "pdf" {
        return None;
    }
    let Some(channel) = ui.channel() else {
        return Some(Vec::new());
    };
    let Some(found) = attachment(store, channel, message, id)
        .filter(|found| !crate::attachments::hidden(ui, found))
    else {
        return Some(Vec::new());
    };
    let source = found.download.clone().expect("validated above");
    let size = found.size_bytes;
    let mut intents = close(ui);
    let request = ui.fresh_request();
    ui.attachment_viewer = None;
    ui.document_viewer = Some(DocumentViewer {
        account: store.me(),
        channel,
        message,
        attachment: id,
        request,
        zoom: 1.0,
        page: 1,
        pages: 1,
        rendered: Vec::new(),
        marked: None,
        error: None,
        search: None,
    });
    intents.push(Intent::PreviewDocument {
        request,
        source,
        size,
    });
    intents.push(Intent::Focus(CLOSE.into()));
    Some(intents)
}

pub(crate) fn intercept(ui: &mut Ui, event: &UiEvent) -> Option<Vec<Intent>> {
    let viewer = ui.document_viewer.as_ref()?;
    let UiEvent::Key { key, ctrl, .. } = event else {
        return None;
    };
    // The search's field takes the keys; Esc closes it first.
    if viewer.search.as_ref().is_some_and(|search| search.open) {
        return (*key == Key::Escape).then(|| close_search(ui));
    }
    if *ctrl && *key == Key::F {
        return Some(search(ui));
    }
    match key {
        Key::Escape => return Some(close(ui)),
        Key::ArrowLeft => return Some(turn(ui, -1)),
        Key::ArrowRight => return Some(turn(ui, 1)),
        _ => {}
    }
    if matches!(key, Key::Tab | Key::Enter | Key::Space) {
        None
    } else {
        Some(Vec::new())
    }
}

pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(found) = current(ui, store) else {
        return root;
    };
    let viewer = ui.document_viewer.as_ref().expect("validated above");
    let mut image = ImageViewer::new(
        root,
        true,
        tr!(
            "{name} · Página {page} de {pages}",
            name = found.filename,
            page = viewer.page,
            pages = viewer.pages
        ),
        CLOSE,
        look.menu_palette(),
    )
    .viewport(context.window)
    .return_focus(attachment_ids::action(
        viewer.message,
        viewer.attachment,
        "pdf",
    ))
    .zoom(viewer.zoom)
    .zoom_actions(OUT, FIT, IN)
    .gallery(viewer.page as usize - 1, viewer.pages as usize)
    .previous_action(PREVIOUS)
    .next_action(NEXT)
    .navigation_labels(t("Página anterior"), t("Página siguiente"));
    // Searching needs the sandboxed tools that render the pages, too.
    image = image.action(SEARCH, Icon::Search, t("Buscar en el documento · Ctrl+F"));
    image = match (viewer.shown(), &viewer.error) {
        (Some(ready), _) => image.image(ready.clone()),
        (None, Some(error)) => image.error(error),
        (None, None) => image,
    };
    let shown = image.render();
    let Some(search) = viewer.search.as_ref().filter(|search| search.open) else {
        return shown;
    };
    let palette = look.menu_palette();
    let width = Modal::fit_width(context.window.width, 560.0);
    let mut body = vec![
        SearchField::new(QUERY, &search.query, t("Buscar en el texto"), palette)
            .size(ControlSize::Compact)
            .accessibility_label(t("Texto a buscar en el documento"))
            .render(),
    ];
    let status = |said: &str| rune_components::line(said, 13.0, look.muted);
    match &search.pages {
        None => body.push(status(t("Leyendo el texto del documento…"))),
        Some(Err(error)) => body.push(status(error)),
        Some(Ok(pages)) if pages.iter().all(Vec::is_empty) => body.push(status(t(
            "Este PDF no tiene texto para buscar: sus páginas son imágenes.",
        ))),
        Some(Ok(_)) if search.query.trim().is_empty() => body.push(status(t(
            "Escribí una palabra para ver en qué páginas está.",
        ))),
        Some(Ok(_)) => {
            let results = search.results();
            if results.is_empty() {
                body.push(status(t("Ninguna página tiene ese texto.")));
            } else {
                body.push(status(&if results.len() == 1 {
                    t("En 1 página").to_owned()
                } else {
                    tr!("En {count} páginas", count = results.len())
                }));
                for (page, snippet) in results {
                    body.push(
                        ListRow::new(
                            format!("{RESULT}{page}"),
                            tr!("Página {page}", page),
                            palette,
                        )
                        .subtitle(snippet)
                        .render(),
                    );
                }
            }
        }
    }
    Modal::new(
        shown,
        true,
        t("Buscar en el documento"),
        "",
        SEARCH_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(6.0))
    .initial_focus(QUERY)
    .return_focus(SEARCH)
    .backdrop_dismiss(true)
    .render()
}
