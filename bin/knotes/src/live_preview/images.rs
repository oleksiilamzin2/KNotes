//! Images and PDF previews (ADR 0020, 0022): sized, decoded as they come into view, resized, opened.

use super::*;

impl LivePreview {
    /// The PDF whose preview is at `point`, opened as a click on a link to it does.
    pub(super) fn open_pdf_at(&self, view: &NSTextView, point: NSPoint) -> bool {
        let Some((line, rect, _)) = crate::preview_fragment::image_at(view, point) else { return false };
        let inside = point.x >= rect.origin.x && point.x <= rect.origin.x + rect.size.width && point.y >= rect.origin.y && point.y <= rect.origin.y + rect.size.height;
        let Some(storage) = (unsafe { view.textStorage() }) else { return false };
        let run = self.ivars().styled.borrow().images.iter().find(|i| i.pdf && paragraph(&storage, ns(&i.range)).0 == line).cloned();
        let (true, Some(run)) = (inside, run) else { return false };
        let link = Link { embed: false, display: None, ..run.link };
        if let Some(callback) = self.ivars().on_link.borrow().as_ref() {
            callback(&link);
        }
        true
    }

    /// The vault the shown notes are in: images are read from it.
    pub fn set_vault(&self, root: PathBuf) {
        *self.ivars().vault.borrow_mut() = Some(root);
        self.forget_images();
    }

    pub(super) fn forget_images(&self) {
        self.ivars().images.borrow_mut().clear();
        self.ivars().bitmaps.borrow_mut().clear();
        self.ivars().pdf_pages.borrow_mut().clear();
    }

    /// The size the image `run` embeds shows at: its own, or the one written after `|` (its
    /// proportions kept), no wider than the text column; read from the file's header, not
    /// decoded. With its vault path and file, and whether it is being resized. `None` when it
    /// does not load.
    pub(super) fn image_size(&self, run: &ImageRun) -> Option<(String, PathBuf, NSSize, bool)> {
        let path = self.ivars().links.borrow().as_ref()?.resolve(run.link.kind, &run.link.target, &self.ivars().path.borrow())?;
        let file = self.ivars().vault.borrow().as_ref()?.join(&path);
        let key = image_key(&path, run);
        let natural = (*self.ivars().images.borrow_mut().entry(key.clone()).or_insert_with(|| image_loader::natural_size(&file, run.page)))?;
        if natural.width <= 0.0 || natural.height <= 0.0 {
            return None;
        }
        let ratio = natural.height / natural.width;
        let resized = self.ivars().resizing.get().filter(|r| r.0 == run.range.start).map(|r| r.1);
        let width = resized.unwrap_or(match (run.width, run.height) {
            (Some(w), Some(h)) => f64::from(w).min(f64::from(h) / ratio),
            (Some(w), None) => f64::from(w),
            (None, Some(h)) => f64::from(h) / ratio,
            (None, None) => natural.width,
        });
        // A point narrower than the column: the hidden source's tiny glyphs beside the kept
        // width would otherwise wrap onto a second line as tall as the image.
        let width = width.min(self.ivars().column.get() - 1.0).max(1.0).round();
        Some((key, file, NSSize::new(width, (width * ratio).round()), resized.is_some()))
    }

    /// "сторінка 3 з 12" under a PDF's preview (ADR 0022); `None` for an image.
    pub(super) fn image_caption(&self, run: &ImageRun) -> Option<String> {
        if !run.pdf {
            return None;
        }
        let path = self.ivars().links.borrow().as_ref()?.resolve(run.link.kind, &run.link.target, &self.ivars().path.borrow())?;
        let file = self.ivars().vault.borrow().as_ref()?.join(&path);
        let pages = (*self.ivars().pdf_pages.borrow_mut().entry(path).or_insert_with(|| image_loader::pdf_pages(&file)))?;
        let page = (run.page.unwrap_or(1) as usize).clamp(1, pages.max(1));
        Some(strings::pdf_page(page, pages))
    }

    /// The image `run` embeds, to be decoded for the size it shows at when first drawn, and
    /// that size.
    pub(super) fn image(&self, run: &ImageRun) -> Option<(Rc<DeferredImage>, NSSize)> {
        let (path, file, size, resizing) = self.image_size(run)?;
        // Decoded again only to show larger; while it is being resized it is stretched.
        let mut bitmaps = self.ivars().bitmaps.borrow_mut();
        let image = match bitmaps.get(&path) {
            Some(image) if image.size.width >= size.width || resizing => Rc::clone(image),
            old => {
                let this = objc2::rc::Weak::new(self);
                let decoded = path.clone();
                // The smaller picture stands in, stretched, until the larger one is decoded:
                // a gap between them flickered as the image was widened.
                let stand_in = old.and_then(|old| old.ready());
                let image = Rc::new(
                    DeferredImage::new(file, size, run.page, move || {
                        if let Some(this) = this.load() {
                            this.image_decoded(&decoded);
                        }
                    })
                    .standing_in(stand_in),
                );
                bitmaps.insert(path, Rc::clone(&image));
                image
            }
        };
        Some((image, size))
    }

    /// Files changed on disk (vault paths): images among them (and PDFs, every page kept) are
    /// read again.
    pub fn files_changed(&self, view: &NSTextView, paths: &[String]) {
        let ours = |key: &String| paths.iter().any(|p| key == p || key.strip_prefix(p.as_str()).is_some_and(|rest| rest.starts_with('#')));
        let stale: Vec<String> = self.ivars().images.borrow().keys().filter(|k| ours(k)).cloned().collect();
        if stale.is_empty() {
            return;
        }
        for key in &stale {
            self.ivars().images.borrow_mut().remove(key);
            self.ivars().bitmaps.borrow_mut().remove(key);
        }
        for path in paths {
            self.ivars().pdf_pages.borrow_mut().remove(path);
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let links = self.ivars().links.borrow().clone();
        let source = self.ivars().path.borrow().clone();
        let lines: Vec<(usize, usize)> = self
            .ivars()
            .styled
            .borrow()
            .images
            .iter()
            .filter(|i| links.as_ref().and_then(|l| l.resolve(i.link.kind, &i.link.target, &source)).is_some_and(|p| paths.contains(&p)))
            .map(|i| paragraph(&storage, ns(&i.range)))
            .collect();
        for line in lines {
            self.restyle(&storage, line);
        }
    }

    /// What each of the note's images shows, by key (`doc.pdf#page=1 stand-in`), sorted
    /// (probe mode).
    pub fn images_shown(&self) -> Vec<String> {
        let mut shown: Vec<String> = self.ivars().bitmaps.borrow().iter().map(|(key, image)| format!("{key} {}", image.shows())).collect();
        shown.sort();
        shown
    }

    /// The note's images decoded (vault paths, sorted), and how many are kept (probe mode).
    pub fn decoded_images(&self) -> (Vec<String>, usize) {
        let bitmaps = self.ivars().bitmaps.borrow();
        let mut ready: Vec<String> = bitmaps.iter().filter(|(_, i)| i.is_decoded()).map(|(p, _)| p.clone()).collect();
        ready.sort();
        (ready, bitmaps.len())
    }

    /// The pointer moved over the image on the line starting at `line`, or off images: the
    /// handle shows on the image it is over.
    pub fn hover_image(&self, view: &NSTextView, line: Option<usize>) {
        let old = self.ivars().hovered_image.replace(line);
        if old == line {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let length = storage.length();
        for at in [old, line].into_iter().flatten().filter(|&at| at < length) {
            self.restyle(&storage, paragraph(&storage, NSRange::new(at, 0)));
        }
    }

    /// The image `key` names (a vault path, a PDF's with its page) is decoded: its lines are
    /// laid out again, to draw it.
    pub(super) fn image_decoded(&self, key: &str) {
        let Some(view) = self.ivars().view.borrow().load() else { return };
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let links = self.ivars().links.borrow().clone();
        let source = self.ivars().path.borrow().clone();
        let lines: Vec<(usize, usize)> = self
            .ivars()
            .styled
            .borrow()
            .images
            .iter()
            .filter(|i| links.as_ref().and_then(|l| l.resolve(i.link.kind, &i.link.target, &source)).is_some_and(|p| image_key(&p, i) == key))
            .map(|i| paragraph(&storage, ns(&i.range)))
            .collect();
        for line in lines {
            self.restyle(&storage, line);
        }
    }

    /// The note scrolled: images more than a screen above or below the view's visible part
    /// are let go (decoded again when they come back), checked every quarter screen.
    pub(super) fn let_far_images_go(&self, view: &NSTextView) {
        let visible = view.visibleRect();
        let top = visible.origin.y - view.textContainerOrigin().y;
        let screen = visible.size.height;
        if self.ivars().images_checked_at.get().is_some_and(|at| (at - top).abs() < screen / 4.0) {
            return;
        }
        self.ivars().images_checked_at.set(Some(top));
        for image in self.ivars().bitmaps.borrow().values() {
            let Some(at) = image.drawn_at.get() else { continue };
            if at + image.size.height < top - screen || at > top + 2.0 * screen {
                image.let_go();
            }
        }
    }

    /// An image away from the cursor: its line's source hidden, the line centred and as tall
    /// as the image, the image's width kept by kerning (ADR 0020). With the cursor on its line:
    /// the source, and room below it for the image.
    pub(super) fn paint_image<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, run: &ImageRun, revealed: bool) {
        let Some((_, _, size, _)) = self.image_size(run) else { return };
        let line = paragraph(storage, ns(&run.range));
        let caption = if run.pdf { PDF_CAPTION } else { 0.0 };
        let reserved = size.height + 2.0 * PREVIEW_GAP + caption;
        if revealed {
            let para = palette.para(Para { spacing_after: reserved, ..Para::default() });
            canvas.paint(line.0..line.1, |p| p.para = para);
            return;
        }
        let look = self.look();
        let text = storage.string();
        let end = if line.1 > line.0 && matches!(text.characterAtIndex(line.1 - 1), 0x0a | 0x0d) { line.1 - 1 } else { line.1 };
        let hidden = palette.look(&look.hidden, &look.clear, "image");
        let kern = palette.kern(size.width);
        canvas.paint(line.0..end, |p| {
            p.look = hidden;
            p.kern = 0;
        });
        canvas.paint(line.0..line.0 + 1, |p| p.kern = kern);
        let para = palette.para(Para { centred: true, min_line: reserved, max_line: reserved, ..Para::default() });
        canvas.paint(line.0..line.1, |p| p.para = para);
    }

    /// The image on the line starting at `line` dragged to `width` points: shown so at once;
    /// when `done`, the width is written into its embed (`|300`), one edit to undo, the cursor
    /// staying where it was.
    pub fn resize_image(&self, view: &NSTextView, line: usize, width: f64, done: bool) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let run = self.ivars().styled.borrow().images.iter().find(|i| paragraph(&storage, ns(&i.range)).0 == line).cloned();
        let Some(run) = run else { return };
        let width = width.round().clamp(MIN_IMAGE_WIDTH, (self.ivars().column.get() - 1.0).max(MIN_IMAGE_WIDTH));
        self.ivars().resizing.set(Some((run.range.start, width)));
        if !done {
            self.restyle(&storage, paragraph(&storage, ns(&run.range)));
            return;
        }
        self.ivars().resizing.set(None);
        let source = storage.string().substringWithRange(ns(&run.range)).to_string();
        let new = set_image_width(&source, width as u32);
        let selection = view.selectedRange();
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&new), ns(&run.range)) };
        let grown = new.encode_utf16().count() as isize - run.range.len() as isize;
        let keep = |at: usize| if at >= run.range.end { (at as isize + grown).max(0) as usize } else { at };
        view.setSelectedRange(NSRange::new(keep(selection.location), selection.length));
    }

    /// The file of the image on the line starting at `line`.
    pub fn image_file(&self, view: &NSTextView, line: usize) -> Option<PathBuf> {
        let storage = unsafe { view.textStorage() }?;
        let styled = self.ivars().styled.borrow();
        let run = styled.images.iter().find(|i| paragraph(&storage, ns(&i.range)).0 == line)?;
        let path = self.ivars().links.borrow().as_ref()?.resolve(run.link.kind, &run.link.target, &self.ivars().path.borrow())?;
        Some(self.ivars().vault.borrow().as_ref()?.join(path))
    }

    /// The text column changed width: images wider than it, or that were, fit again.
    pub(super) fn fit_images(&self, view: &NSTextView) {
        let column = text_column(view);
        if self.ivars().column.replace(column) == column {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let paragraphs: Vec<(usize, usize)> = self.ivars().styled.borrow().images.iter().map(|i| paragraph(&storage, ns(&i.range))).collect();
        for para in paragraphs {
            self.restyle(&storage, para);
        }
    }
}

/// What an image is kept by: its vault path, a PDF's with the page shown (`doc.pdf#page=3`).
pub(super) fn image_key(path: &str, run: &ImageRun) -> String {
    if run.pdf { format!("{path}#page={}", run.page.unwrap_or(1)) } else { path.to_owned() }
}
