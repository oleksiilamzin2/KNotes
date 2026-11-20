//! Restyling: after an edit (only what changed, ADR 0012), a region, the cursor's paragraph and the markup it reveals.

use super::*;

impl LivePreview {
    /// Restyles after a user edit: only the paragraphs whose styles changed (ADR 0012), plus
    /// the old and new cursor paragraphs, whose markup visibility changes.
    pub(super) fn restyle_after_edit(&self, view: &NSTextView) {
        // The found words' ranges no longer hold after an edit.
        self.clear_found(view);
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let total = storage.length();
        let mut edit = self.ivars().pending.take();
        if let Some(e) = edit {
            e.apply(&mut self.ivars().source.borrow_mut(), &self.ivars().inserted.borrow());
            self.ivars().source_units.set((self.ivars().source_units.get() + e.inserted).saturating_sub(e.removed));
        }
        if self.ivars().source_units.get() != total {
            edit = None;
        }
        if edit.is_none() {
            self.resync(&storage);
        }
        let source = self.ivars().source.borrow();
        let new = highlight(&source).to_utf16(&source);
        drop(source);
        let revealed = self.cursor_paragraph(&storage, view);
        self.ivars().selection.set(span(view.selectedRange()));
        let region = edit.map(|edit| {
            let (start, end) = changed_region(&self.ivars().styled.borrow(), &new, edit);
            let old = self.ivars().revealed.get();
            let old = edit.move_range(&(old.0..old.1));
            let start = start.min(old.start).min(revealed.0).min(total);
            let end = end.max(old.end).max(revealed.1).min(total);
            paragraph(&storage, NSRange::new(start, end - start))
        });
        *self.ivars().markup_fonts.borrow_mut() = markup_fonts(&new, &self.ivars().look);
        *self.ivars().styled.borrow_mut() = new;
        self.ivars().revealed.set(revealed);
        // An edit not announced through shouldChangeText restyles everything.
        self.restyle(&storage, region.unwrap_or((0, total)));
        // A word marked misspelled before it became a formula (`$Y` is a word until its second
        // `$` is typed) keeps its mark, which on the formula's hidden glyphs is a stray red
        // dot: the marks of what was restyled go from its formulas and image embeds.
        if let Some(region) = region {
            let styled = self.ivars().styled.borrow();
            let within = |r: &Range<usize>| r.start < region.1 && r.end > region.0;
            for range in styled.formulas.iter().map(|f| &f.range).chain(styled.images.iter().map(|i| &i.range)).filter(|r| within(r)) {
                view.setSpellingState_range(0, ns(range));
            }
        }
    }

    pub(super) fn resync(&self, storage: &NSTextStorage) {
        *self.ivars().source.borrow_mut() = storage.string().to_string();
        self.ivars().source_units.set(storage.length());
    }

    /// Restyles `region` (UTF-16), grown to whole paragraphs and to the whole of every formula
    /// meeting it. The styles are composed in memory, layer over layer, and set front to back,
    /// each run once (`text_paint`).
    pub(super) fn restyle(&self, storage: &NSTextStorage, region: (usize, usize)) {
        // A table is laid out whole (its columns are as wide as their widest cell), but its
        // rows outside the region are set again only when its columns changed: setting a row
        // lays it out again.
        let asked = self.whole(storage, region, false);
        let region = self.whole(storage, asked, true);
        if region.0 >= region.1 {
            return;
        }
        self.ivars().tables_changed.set(false);
        let look = &self.ivars().look;
        let styled = self.ivars().styled.borrow();
        let fonts = self.ivars().markup_fonts.borrow();
        let revealed = self.ivars().revealed.get();
        let mut palette = Palette::new();
        let plain = palette.look(&look.body, &look.text, "plain");
        let mut canvas = Canvas::new(region.0..region.1, Paint { look: plain, para: 0, kern: 0 });
        let meets = |r: &Range<usize>| r.start < region.1 && r.end > region.0;
        // A highlight keeps the font and colour of what it covers: its yellow comes below.
        for run in styled.runs.iter().filter(|r| meets(&r.range) && !matches!(r.style, Style::ListMarker { .. } | Style::QuoteMarker { .. } | Style::Fence | Style::Mark)) {
            let (font, color, name) = self.run_look(run, &styled);
            let id = palette.look(font, color, name);
            canvas.paint(run.range.clone(), |p| p.look = id);
        }
        // A paused card (ADR 0037): all of it muted, each part in its own font. Before the
        // markup, whose hidden signs stay hidden.
        let paused = self.ivars().paused_cards.borrow();
        for (card, _) in styled.card_ids.iter().filter(|(card, id)| meets(card) && paused.contains(id)) {
            let palette = std::cell::RefCell::new(&mut palette);
            canvas.paint(card.clone(), |p| p.look = palette.borrow_mut().muted(p.look, &look.muted, "card-paused"));
        }
        // After all runs (one inside would replace it), before the markup (which drops it).
        for run in styled.runs.iter().filter(|r| matches!(r.style, Style::Strike | Style::Done) && meets(&r.range)) {
            canvas.paint(run.range.clone(), |p| p.look |= STRUCK);
        }
        for run in styled.runs.iter().filter(|r| matches!(r.style, Style::Code | Style::CardDeck) && meets(&r.range)) {
            canvas.paint(run.range.clone(), |p| p.look |= SHADED);
        }
        // A thematic break away from the cursor: its signs keep their line's height but do not
        // show; the fragment draws the line (`Mark::Rule`).
        for run in styled.runs.iter().filter(|r| r.style == Style::Rule && meets(&r.range) && !overlaps(&r.range, revealed)) {
            let id = palette.look(&look.body, &look.clear, "rule-line");
            canvas.paint(run.range.clone(), |p| p.look = id);
        }
        for run in styled.runs.iter().filter(|r| r.style == Style::Mark && meets(&r.range)) {
            canvas.paint(run.range.clone(), |p| p.look |= MARKED);
        }
        for run in styled.runs.iter().filter(|r| r.style == Style::FootnoteRef && meets(&r.range)) {
            canvas.paint(run.range.clone(), |p| p.look |= RAISED);
        }
        // Markup shows in the cursor's paragraph; an inline run's signs (`**`, a link's
        // brackets) only with the cursor in that run, as Obsidian has it: the rest of the line
        // reads as it will.
        let selection = self.ivars().selection.get();
        for (m, font) in styled.markup.iter().zip(fonts.iter()).filter(|(m, _)| meets(m)) {
            let shown = overlaps(m, revealed) && styled.inline_owner(m, revealed.0).is_none_or(|run| knotes_editor::reaches(&run, selection));
            let id = if shown { palette.look(font, &look.muted, "markup") } else { palette.look(&look.hidden, &look.clear, "hidden") };
            canvas.paint(m.clone(), |p| p.look = id);
        }
        // A link to a website keeps room after its text for the arrow the fragment draws, while
        // its `](url)` is hidden.
        let arrow = palette.kern(EXTERNAL_ROOM);
        for end in external_link_ends(&styled, region, revealed, selection) {
            canvas.paint(end.kern_at..end.kern_at + 1, |p| p.kern = arrow);
        }
        // A done task's text, and a comment, are muted, bold or italic inside too (links keep
        // their colour).
        for run in styled.runs.iter().filter(|r| matches!(r.style, Style::Done | Style::Comment) && meets(&r.range)) {
            let mut looks: Vec<u16> = run.range.clone().filter_map(|i| canvas.get(i).map(|p| p.look)).collect();
            looks.sort_unstable();
            looks.dedup();
            let muted: Vec<(u16, u16)> = looks.into_iter().map(|id| (id, palette.recolor(id, &look.text, &look.muted))).collect();
            canvas.paint(run.range.clone(), |p| p.look = muted.iter().find(|m| m.0 == p.look).map_or(p.look, |m| m.1));
        }
        for run in styled.runs.iter().filter(|r| meets(&r.range)) {
            if let Style::QuoteMarker { depth } = run.style {
                // Shown with the cursor in its paragraph, as other markup; the paragraph indents
                // past the bars the fragment draws.
                if overlaps(&run.range, revealed) {
                    let id = palette.look(&look.body, &look.muted, "quote-marker");
                    canvas.paint(run.range.clone(), |p| p.look = id);
                } else {
                    self.paint_hidden(&mut canvas, &mut palette, storage, &run.range, "quote", 0.0);
                }
                let indent = f64::from(depth) * QUOTE_INDENT;
                // An empty quote line (`>` alone) keeps a line's height: TextKit 2 took it from
                // the hidden `>`.
                let para = palette.para(Para { first_indent: indent, head_indent: indent, min_line: self.line_height(), ..Para::default() });
                let paragraph = paragraph(storage, NSRange::new(run.range.start, 0));
                canvas.paint(paragraph.0..paragraph.1, |p| p.para = para);
            }
        }
        // A card stands as a block, as a quote does: its paragraphs indent past its bar.
        for card in styled.cards.iter().filter(|c| meets(c)) {
            let para = palette.para(Para { first_indent: QUOTE_INDENT, head_indent: QUOTE_INDENT, min_line: self.line_height(), ..Para::default() });
            let mut at = card.start;
            while at < card.end {
                let paragraph = paragraph(storage, NSRange::new(at, 0));
                canvas.paint(paragraph.0..paragraph.1, |p| p.para = para);
                at = paragraph.1.max(at + 1);
            }
        }
        for block in styled.code_blocks.iter().filter(|c| meets(c)) {
            self.paint_code_block(&mut canvas, &mut palette, storage, &styled, block, overlaps(block, revealed));
        }
        for callout in styled.callouts.iter().filter(|c| meets(&c.range)) {
            self.paint_callout(&mut canvas, &mut palette, storage, callout, overlaps(&callout.marker, revealed));
        }
        for run in styled.runs.iter().filter(|r| meets(&r.range)) {
            self.paint_list_marker(&mut canvas, &mut palette, storage, run);
        }
        for f in styled.formulas.iter().filter(|f| meets(&f.range)) {
            self.paint_formula(&mut canvas, &mut palette, storage, f, self.formula_shown(f));
        }
        for image in styled.images.iter().filter(|i| meets(&i.range)) {
            self.paint_image(&mut canvas, &mut palette, storage, image, overlaps(&image.range, revealed));
        }
        if !styled.embeds.is_empty() {
            let heights = self.ivars().embed_heights.borrow();
            for (key, _, range) in self.embeds(&storage.string()) {
                if let Some(&height) = heights.get(&key).filter(|_| range.0 < region.1 && range.1 > region.0) {
                    self.paint_embed(&mut canvas, &mut palette, storage, range, height, overlaps(&(range.0..range.1), revealed));
                }
            }
        }
        for (t, table) in styled.tables.iter().enumerate().filter(|(_, t)| meets(&t.range)) {
            self.paint_table(&mut canvas, &mut palette, storage, t, table);
        }
        self.paint_formula_lines(&mut canvas, &mut palette, storage, &styled, region);
        let applied = if self.ivars().tables_changed.get() { region } else { asked };
        // A table scrolling sideways sets only its rows in view; the rest catch up after.
        let applied = self.ivars().apply_within.get().map_or(applied, |w| (applied.0.max(w.0), applied.1.min(w.1).max(applied.0.max(w.0))));
        look.paint.apply_in(storage, &canvas, &palette, applied.0..applied.1);
    }

    /// `region` grown to whole paragraphs and to the whole of every formula meeting it (a
    /// display formula styles all its paragraphs).
    pub(super) fn whole(&self, storage: &NSTextStorage, region: (usize, usize), tables: bool) -> (usize, usize) {
        let total = storage.length();
        let styled = self.ivars().styled.borrow();
        let mut r = (region.0.min(total), region.1.min(total));
        loop {
            let para = paragraph(storage, NSRange::new(r.0, r.1 - r.0));
            // A table's column widths come from all its rows.
            let grown = styled
                .formulas
                .iter()
                .map(|f| &f.range)
                .chain(styled.tables.iter().map(|t| &t.range).filter(|_| tables))
                .filter(|r| r.start < para.1 && r.end > para.0)
                .fold(para, |g, r| (g.0.min(r.start), g.1.max(r.end.min(total))));
            if grown == r {
                return r;
            }
            r = grown;
        }
    }

    /// Restyles what changes when the cursor moves: the markup of the paragraph it left and of
    /// the one it entered, and the list markers it now touches or left.
    pub(super) fn reveal_cursor_paragraph(&self, view: &NSTextView) {
        if self.ivars().pending.get().is_some() {
            // Mid-edit: the highlight still has the old offsets; the edit restyles this itself.
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let (was, now) = (self.ivars().selection.get(), span(view.selectedRange()));
        let mut regions = self.markers_touched_anew(now);
        let new = self.cursor_paragraph(&storage, view);
        let old = self.ivars().revealed.replace(new);
        if old != new {
            regions.extend([old, new].into_iter().filter(|&p| self.reveals_in_paragraph(p)));
            // Formulas of their own lines and code blocks show their source while the cursor is
            // anywhere in them.
            let styled = self.ivars().styled.borrow();
            let blocks = styled.formulas.iter().filter(|f| f.display).map(|f| &f.range).chain(&styled.code_blocks);
            regions.extend(blocks.filter(|b| overlaps(b, old) != overlaps(b, new)).map(|b| (b.start, b.end)));
        }
        if (old, was) != (new, now) {
            // The inline runs and formulas the cursor came into or left: their signs show or go.
            let styled = self.ivars().styled.borrow();
            let changed = |range: &Range<usize>| super::shown_inline(range, old, was) != super::shown_inline(range, new, now);
            for para in [old, new].into_iter().filter(|&para| para != super::NOWHERE) {
                regions.extend(styled.inline_runs(para.0, para.1).filter(|run| changed(run)).map(|run| (run.start, run.end)));
                let first = styled.formulas.partition_point(|f| f.range.end <= para.0);
                regions.extend(styled.formulas[first..].iter().take_while(|f| f.range.start < para.1).filter(|f| !f.display && changed(&f.range)).map(|f| (f.range.start, f.range.end)));
            }
        }
        regions.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for r in regions {
            match merged.last_mut() {
                Some(last) if r.0 <= last.1 => last.1 = last.1.max(r.1),
                _ => merged.push(r),
            }
        }
        for region in merged {
            self.restyle(&storage, region);
        }
    }

    /// Whether a paragraph shows differently with the cursor in it: it has markup or a quote's
    /// prefix.
    /// The cursor's paragraph shows its markup, or none (an embedded note given up the keys).
    pub fn refocus(&self, view: &NSTextView) {
        self.reveal_cursor_paragraph(view);
    }

    /// The paragraph whose markup shows: the cursor's; none in an embedded note without the keys
    /// (ADR 0034).
    pub(super) fn cursor_paragraph(&self, storage: &NSTextStorage, view: &NSTextView) -> (usize, usize) {
        let focused = view.window().and_then(|w| w.firstResponder()).is_some_and(|r| std::ptr::eq(&*r as *const _ as *const u8, view as *const _ as *const u8));
        if self.ivars().embedded.get() && !focused || self.ivars().shown_only.get() {
            return NOWHERE;
        }
        paragraph(storage, view.selectedRange())
    }

    pub(super) fn reveals_in_paragraph(&self, para: (usize, usize)) -> bool {
        let styled = self.ivars().styled.borrow();
        let first = styled.markup.partition_point(|m| m.end <= para.0);
        let runs = styled.runs.partition_point(|r| r.range.start < para.0);
        styled.markup.get(first).is_some_and(|m| m.start < para.1)
            || styled.runs[runs..].iter().take_while(|r| r.range.start < para.1).any(|r| matches!(r.style, Style::QuoteMarker { .. } | Style::Rule))
    }

    /// Records the new selection; the ranges of the list markers it now touches or left.
    pub(super) fn markers_touched_anew(&self, new: (usize, usize)) -> Vec<(usize, usize)> {
        let old = self.ivars().selection.replace(new);
        if old == new {
            return Vec::new();
        }
        let styled = self.ivars().styled.borrow();
        // Markers are short: those touching a selection start a little before it.
        let near = |sel: (usize, usize)| {
            let first = styled.runs.partition_point(|r| r.range.start + MARKER_REACH < sel.0);
            styled.runs[first..].iter().take_while(move |r| r.range.start <= sel.1)
        };
        near(old)
            .chain(near(new))
            .filter(|r| matches!(r.style, Style::ListMarker { .. }) && touches(&r.range, old) != touches(&r.range, new))
            .map(|r| (r.range.start, r.range.end))
            .collect()
    }

    /// Font, colour and test name of a run; links to missing notes are dimmed.
    pub(super) fn run_look<'a>(&'a self, run: &Run, styled: &Highlight) -> (&'a NSFont, &'a NSColor, &'static str) {
        let look = &self.ivars().look;
        if matches!(run.style, Style::Link | Style::WikiLink)
            && let Some(links) = self.ivars().links.borrow().as_ref()
            && let Some(link) = link_at(styled, run.range.start)
            && link.link.kind != LinkKind::External
            && links.resolve(link.link.kind, &link.link.target, &self.ivars().path.borrow()).is_none()
        {
            return (&look.body, &look.missing, "missing");
        }
        run_look(run.style, look)
    }

    pub fn restyle_all(&self, view: &NSTextView, source: &str) {
        let styled = highlight(source).to_utf16(source);
        *self.ivars().markup_fonts.borrow_mut() = markup_fonts(&styled, &self.ivars().look);
        *self.ivars().styled.borrow_mut() = styled;
        if let Some(storage) = unsafe { view.textStorage() } {
            self.restyle(&storage, (0, storage.length()));
        }
    }
}

pub(super) fn run_look(style: Style, look: &Look) -> (&NSFont, &NSColor, &'static str) {
    match style {
        Style::Heading(level) => {
            let i = usize::from(level.clamp(1, 6)) - 1;
            (&look.headings[i], &look.text, HEADING_NAMES[i])
        }
        Style::Strong => (&look.bold, &look.text, "strong"),
        Style::Emphasis => (&look.italic, &look.text, "emphasis"),
        Style::Strike => (&look.body, &look.text, "strike"),
        Style::Code => (&look.mono, &look.text, "code"),
        Style::CodeBlock => (&look.mono, &look.text, "codeblock"),
        Style::Link => (&look.body, &look.accent, "link"),
        Style::WikiLink => (&look.body, &look.accent, "wikilink"),
        Style::Tag => (&look.body, &look.accent, "tag"),
        Style::Math => (&look.body, &look.text, "math"),
        Style::ListMarker { .. } => (&look.body, &look.muted, "list-marker"),
        Style::Done => (&look.body, &look.muted, "done"),
        Style::QuoteMarker { .. } => (&look.body, &look.muted, "quote-marker"),
        Style::Fence => (&look.mono, &look.muted, "code-fence"),
        Style::Mark => (&look.body, &look.text, "mark"),
        Style::Comment => (&look.body, &look.muted, "comment"),
        Style::FootnoteRef => (&look.footnote, &look.accent, "footnote"),
        Style::FootnoteLabel => (&look.body, &look.muted, "footnote-label"),
        Style::CardId => (&look.footnote, &look.muted, "card-id"),
        Style::Rule => (&look.body, &look.muted, "rule"),
        Style::Frontmatter => (&look.footnote, &look.muted, "frontmatter"),
        Style::CardDeck => (&look.body, &look.card_deck, "card-deck"),
        Style::CardQuestion => (&look.bold, &look.text, "card-question"),
        Style::CardMark => (&look.body, &look.muted, "card-mark"),
    }
}

/// The font each markup range takes when shown: that of the innermost run containing it.
pub(super) fn markup_fonts(styled: &Highlight, look: &Look) -> Vec<Retained<NSFont>> {
    styled.markup_owners().into_iter().map(|owner| owner.map_or_else(|| look.body.clone(), |style| run_look(style, look).0.retain())).collect()
}

/// How far before a selection a list marker touching it may start.
pub(super) const MARKER_REACH: usize = 16;

/// Whether the selection touches a list marker: starts in it or ends in or after its start.
pub(super) fn touches(marker: &Range<usize>, selection: (usize, usize)) -> bool {
    selection.0 < marker.end && selection.1 >= marker.start
}

/// Where the room for a website link's arrow is kept: the first character of its hidden
/// `](url)`, or the last of a bare address (`bare`).
pub(super) struct ArrowRoom {
    pub(super) kern_at: usize,
    pub(super) bare: bool,
}

/// The arrow rooms of the links to websites meeting `region`, but of the one the cursor is in
/// (in the `revealed` paragraph, `selection` there): its source shows.
pub(super) fn external_link_ends(styled: &Highlight, region: (usize, usize), revealed: (usize, usize), selection: (usize, usize)) -> Vec<ArrowRoom> {
    let first = styled.links.partition_point(|l| l.range.end <= region.0);
    styled.links[first..]
        .iter()
        .take_while(|l| l.range.start < region.1)
        .filter(|l| l.link.kind == LinkKind::External && !super::shown_inline(&l.range, revealed, selection))
        .filter_map(|l| {
            let after = styled.markup.partition_point(|m| m.start <= l.range.start);
            let close = styled.markup[after..].iter().take_while(|m| m.start < l.range.end).find(|m| m.end == l.range.end);
            let opened = styled.markup[..after].last().is_some_and(|m| m.start == l.range.start);
            match close {
                Some(m) => Some(ArrowRoom { kern_at: m.start, bare: false }),
                // No markup at either end: a bare address.
                None if !opened => Some(ArrowRoom { kern_at: l.range.end - 1, bare: true }),
                None => None,
            }
        })
        .filter(|room| room.kern_at >= region.0 && room.kern_at < region.1)
        .collect()
}
