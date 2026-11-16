//! Code blocks (ADR 0025): their background, gutter and highlighting, the copy button.

use super::*;

impl LivePreview {
    /// A code block's paragraphs inset in the background the fragments draw; its fences fold
    /// to padding unless the cursor is in the block, then they show.
    pub(super) fn paint_code_block<'a>(
        &'a self,
        canvas: &mut Canvas<Paint>,
        palette: &mut Palette<'a>,
        storage: &NSTextStorage,
        styled: &Highlight,
        block: &Range<usize>,
        revealed: bool,
    ) {
        let look = &self.ivars().look;
        let paragraphs = paragraph(storage, ns(block));
        let base = palette.para_of(canvas.get(paragraphs.0).map_or(0, |p| p.para));
        // Room for the line numbers left of the code (ADR 0025).
        let lines = self.code_lines(storage, styled, block);
        let inset = base.first_indent + CODE_INSET + self.code_gutter(lines.len());
        let code = palette.para(Para { first_indent: inset, head_indent: inset, ..base });
        canvas.paint(paragraphs.0..paragraphs.1, |p| p.para = code);
        let first = styled.runs.partition_point(|r| r.range.start < block.start);
        for fence in styled.runs[first..].iter().take_while(|r| r.range.start < block.end).filter(|r| r.style == Style::Fence) {
            if revealed {
                let id = palette.look(&look.mono, &look.muted, "code-fence");
                canvas.paint(fence.range.clone(), |p| p.look = id);
            } else {
                let id = palette.look(&look.hidden, &look.clear, "fence");
                canvas.paint(fence.range.clone(), |p| p.look = id);
                let folded = palette.para(Para { min_line: FENCE_PADDING, max_line: FENCE_PADDING, first_indent: inset, head_indent: inset, ..base });
                let line = paragraph(storage, NSRange::new(fence.range.start, 0));
                canvas.paint(line.0..line.1, |p| p.para = folded);
            }
        }
        // Its code coloured by kind when the fence names a language (ADR 0025).
        if let Some((nth, run)) = styled.code_langs.iter().enumerate().find(|(_, c)| c.block == *block) {
            for (range, token) in self.code_tokens(storage, nth, run).iter() {
                let (font, name) = match token {
                    CodeToken::Keyword => (&look.mono_bold, "code-keyword"),
                    other => (&look.mono, code_token_name(*other)),
                };
                let id = palette.look(font, &look.code_colors[code_look(*token)], name);
                canvas.paint(run.code.start + range.start..run.code.start + range.end, |p| p.look = id);
            }
        }
    }

    /// Where each code line of `block` starts (UTF-16): the lines between its fences.
    pub(super) fn code_lines(&self, storage: &NSTextStorage, styled: &Highlight, block: &Range<usize>) -> CodeLines {
        let key = (block.start, block.end);
        if let Some(lines) = self.ivars().code_lines.borrow().get(&key) {
            return Rc::clone(lines);
        }
        let first = styled.runs.partition_point(|r| r.range.start < block.start);
        let fences: Vec<&Range<usize>> = styled.runs[first..].iter().take_while(|r| r.range.start < block.end).filter(|r| r.style == Style::Fence).map(|r| &r.range).collect();
        let start = fences.iter().find(|f| f.start == block.start).map_or(block.start, |f| (f.end + 1).min(block.end));
        let end = fences.iter().find(|f| f.start > block.start && f.end == block.end).map_or(block.end, |f| f.start);
        let mut lines = Vec::new();
        if start < end {
            lines.push(start);
            let text = storage.string().substringWithRange(NSRange::new(start, end - start)).to_string();
            let mut at = start;
            for unit in text.encode_utf16() {
                at += 1;
                if unit == u16::from(b'\n') && at < end {
                    lines.push(at);
                }
            }
        }
        let lines = Rc::new(lines);
        let mut cache = self.ivars().code_lines.borrow_mut();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, Rc::clone(&lines));
        lines
    }

    /// The width of the line numbers' gutter for `lines` lines.
    pub(super) fn code_gutter(&self, lines: usize) -> f64 {
        let digits = lines.max(1).to_string().len() as f64;
        digits * self.mono_digit_width() + crate::preview_fragment::CODE_NUMBER_GAP
    }

    /// A digit's width in the code font, once measured.
    pub(super) fn mono_digit_width(&self) -> f64 {
        let look = &self.ivars().look;
        if let Some(width) = look.mono_digit.get() {
            return width;
        }
        // SAFETY: a font value of its attribute.
        let width = unsafe {
            let attrs = NSDictionary::from_retained_objects(&[objc2_app_kit::NSFontAttributeName], &[Retained::into_super(Retained::into_super(look.mono.clone()))]);
            objc2_app_kit::NSStringDrawing::sizeWithAttributes(&*NSString::from_str("0"), Some(&attrs)).width
        };
        look.mono_digit.set(Some(width));
        width
    }

    /// The tokens of the `nth` code block's code, from the cache or highlighted now (only its
    /// changed lines parsed again).
    pub(super) fn code_tokens(&self, storage: &NSTextStorage, nth: usize, run: &CodeRun) -> CodeTokens {
        let code = storage.string().substringWithRange(ns(&run.code)).to_string();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (&run.language, &code).hash(&mut hasher);
            hasher.finish()
        };
        if let Some(tokens) = self.ivars().code_tokens.borrow().get(&key) {
            return Rc::clone(tokens);
        }
        let tokens = {
            let mut highlighters = self.ivars().code_highlighters.borrow_mut();
            if !highlighters.get(&nth).is_some_and(|h| h.language() == run.language) {
                match CodeHighlighter::new(&run.language) {
                    Some(h) => {
                        highlighters.insert(nth, h);
                    }
                    None => {
                        highlighters.remove(&nth);
                    }
                }
            }
            highlighters.get_mut(&nth).map(|h| h.update(&code).0).unwrap_or_default()
        };
        let bytes: Vec<usize> = tokens.iter().flat_map(|(r, _)| [r.start, r.end]).collect();
        let units = utf16_offsets(&code, &bytes);
        let tokens: Vec<(Range<usize>, CodeToken)> = tokens.iter().enumerate().map(|(i, (_, token))| (units[2 * i]..units[2 * i + 1], *token)).collect();
        let tokens = Rc::new(tokens);
        let mut cache = self.ivars().code_tokens.borrow_mut();
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(key, Rc::clone(&tokens));
        tokens
    }

    /// The pointer moved over a code block or off it: the copy button shows on the block it is
    /// over.
    pub fn hover_code(&self, view: &NSTextView, point: Option<NSPoint>) {
        let block = point.and_then(|p| {
            let at = view.characterIndexForInsertionAtPoint(p);
            let styled = self.ivars().styled.borrow();
            styled.code_blocks.iter().find(|c| c.start <= at && at <= c.end).map(|c| c.start)
        });
        let old = self.ivars().hovered_code.replace(block);
        if old == block {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let firsts: Vec<usize> = {
            let styled = self.ivars().styled.borrow();
            [old, block].into_iter().flatten().filter_map(|start| styled.code_blocks.iter().find(|c| c.start == start).cloned()).filter_map(|c| self.code_lines(&storage, &styled, &c).first().copied()).collect()
        };
        for at in firsts.into_iter().filter(|&at| at < storage.length()) {
            self.restyle(&storage, paragraph(&storage, NSRange::new(at, 0)));
        }
    }

    /// A click on the copy button of the code block under the pointer: its code (between its
    /// fences) copied.
    pub(super) fn press_code_copy(&self, view: &NSTextView, point: NSPoint) -> bool {
        let Some(start) = self.ivars().hovered_code.get() else { return false };
        let Some(storage) = (unsafe { view.textStorage() }) else { return false };
        let lines = {
            let styled = self.ivars().styled.borrow();
            let Some(block) = styled.code_blocks.iter().find(|c| c.start == start).cloned() else { return false };
            let lines = self.code_lines(&storage, &styled, &block);
            let first = styled.runs.partition_point(|r| r.range.start < block.start);
            let end = styled.runs[first..].iter().take_while(|r| r.range.start < block.end).filter(|r| r.style == Style::Fence && r.range.start > block.start && r.range.end == block.end).map(|r| r.range.start).next().unwrap_or(block.end);
            lines.first().map(|&first| first..end)
        };
        let (Some(code), Some(layout)) = (lines, view.textLayoutManager()) else { return false };
        let Some(content) = layout.textContentManager() else { return false };
        let Some(fragment) = content.locationFromLocation_withOffset(&content.documentRange().location(), code.start as isize).and_then(|at| layout.textLayoutFragmentForLocation(&at)) else { return false };
        let Some(button) = fragment.downcast_ref::<crate::preview_fragment::PreviewFragment>().and_then(|f| f.code_copy_rect()) else { return false };
        let (frame, origin) = (fragment.layoutFragmentFrame(), view.textContainerOrigin());
        let (x, y) = (origin.x + frame.origin.x + button.origin.x, origin.y + frame.origin.y + button.origin.y);
        if point.x < x || point.x > x + button.size.width || point.y < y || point.y > y + button.size.height {
            return false;
        }
        let text = storage.string().substringWithRange(ns(&code)).to_string();
        let board = self.ivars().copy_board.borrow().clone().unwrap_or_else(objc2_app_kit::NSPasteboard::generalPasteboard);
        board.clearContents();
        // SAFETY: AppKit's pasteboard type constant lives for the whole process.
        board.setString_forType(&NSString::from_str(text.strip_suffix('\n').unwrap_or(&text)), unsafe { objc2_app_kit::NSPasteboardTypeString });
        true
    }

    /// `KNOTES_PROBE_CODE_HOVER=index`: the pointer over character `index`;
    /// `KNOTES_PROBE_CODE_COPY=index`: then a click on the copy button, to a pasteboard of the
    /// probe's own, the code copied printed.
    pub fn probe_code_pointer(&self, view: &NSTextView) {
        let (hover, copy) = (std::env::var("KNOTES_PROBE_CODE_HOVER").ok(), std::env::var("KNOTES_PROBE_CODE_COPY").ok());
        let Some(index) = hover.as_deref().or(copy.as_deref()).and_then(|i| i.parse::<usize>().ok()) else { return };
        let (Some(layout), Some(window)) = (view.textLayoutManager(), view.window()) else { return };
        window.displayIfNeeded();
        let Some(content) = layout.textContentManager() else { return };
        let Some(fragment) = content.locationFromLocation_withOffset(&content.documentRange().location(), index as isize).and_then(|at| layout.textLayoutFragmentForLocation(&at)) else { return };
        let (frame, origin) = (fragment.layoutFragmentFrame(), view.textContainerOrigin());
        self.hover_code(view, Some(NSPoint::new(origin.x + frame.origin.x + 40.0, origin.y + frame.origin.y + frame.size.height / 2.0)));
        window.displayIfNeeded();
        if copy.is_none() {
            return;
        }
        let board = objc2_app_kit::NSPasteboard::pasteboardWithUniqueName();
        *self.ivars().copy_board.borrow_mut() = Some(board.clone());
        let start = self.ivars().hovered_code.get();
        let first = start.and_then(|s| {
            let storage = unsafe { view.textStorage() }?;
            let styled = self.ivars().styled.borrow();
            let block = styled.code_blocks.iter().find(|c| c.start == s).cloned()?;
            self.code_lines(&storage, &styled, &block).first().copied()
        });
        let button = first
            .and_then(|at| content.locationFromLocation_withOffset(&content.documentRange().location(), at as isize))
            .and_then(|at| layout.textLayoutFragmentForLocation(&at))
            .and_then(|f| f.downcast_ref::<crate::preview_fragment::PreviewFragment>().and_then(|p| p.code_copy_rect()).map(|r| (r, f.layoutFragmentFrame())));
        if let Some((r, frame)) = button {
            let centre = NSPoint::new(origin.x + frame.origin.x + r.origin.x + r.size.width / 2.0, origin.y + frame.origin.y + r.origin.y + r.size.height / 2.0);
            self.press(view, centre);
        }
        // SAFETY: AppKit's pasteboard type constant lives for the whole process.
        let copied = board.stringForType(unsafe { objc2_app_kit::NSPasteboardTypeString }).map(|s| s.to_string()).unwrap_or_default();
        println!("code-copied: {}", copied.replace('\n', "⏎"));
    }
}

/// Where each of a code block's lines starts (UTF-16).
pub(super) type CodeLines = Rc<Vec<usize>>;

/// A code block's tokens: UTF-16 ranges from the code's start, and their kinds.
pub(super) type CodeTokens = Rc<Vec<(Range<usize>, CodeToken)>>;

/// Xcode's default colours (light, dark) for each kind of code, by `code_look`.
pub(super) const CODE_COLORS: [(u32, u32); 9] = [
    (0x5D6C79, 0x6C7986), // comment
    (0xC41A16, 0xFC6A5D), // string
    (0x1C00CF, 0xD0BF69), // number
    (0x9B2393, 0xFC5FA3), // keyword
    (0x3900A0, 0xD0A8FF), // type
    (0x6C36A9, 0xA167E6), // function
    (0x326D74, 0x67B7A4), // constant
    (0x643820, 0xFD8F3F), // preprocessor
    (0x815F03, 0xBF8555), // attribute
];

/// A token kind's index in `CODE_COLORS`.
pub(super) fn code_look(token: CodeToken) -> usize {
    match token {
        CodeToken::Comment => 0,
        CodeToken::String => 1,
        CodeToken::Number => 2,
        CodeToken::Keyword => 3,
        CodeToken::Type => 4,
        CodeToken::Function => 5,
        CodeToken::Constant => 6,
        CodeToken::Preprocessor => 7,
        CodeToken::Attribute => 8,
    }
}

/// A token kind's name in the style attribute (tests read it).
pub(super) fn code_token_name(token: CodeToken) -> &'static str {
    match token {
        CodeToken::Comment => "code-comment",
        CodeToken::String => "code-string",
        CodeToken::Number => "code-number",
        CodeToken::Keyword => "code-keyword",
        CodeToken::Type => "code-type",
        CodeToken::Function => "code-function",
        CodeToken::Constant => "code-constant",
        CodeToken::Preprocessor => "code-preprocessor",
        CodeToken::Attribute => "code-attribute",
    }
}
