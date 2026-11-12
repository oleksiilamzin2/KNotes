//! Typeset formulas drawn as layers over the note text view (ADR 0017): the text keeps the
//! formula's source (hidden, its room reserved by the live preview); a layer shows the image
//! at that place. Layers take no clicks, so a click there puts the cursor in the source.

use std::cell::RefCell;
use std::sync::Arc;

use knotes_editor::FormulaImage;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSImage, NSTextElementProvider, NSTextView};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};
use objc2_quartz_core::CALayer;

/// Where a formula's image goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Inline, from the formula's first character, its baseline on the text baseline.
    Inline,
    /// A display formula's own (centred) line, vertically centred in it.
    Line,
    /// Below the line of character `index` (a display formula being edited): a preview,
    /// centred in the text column.
    Below,
}

pub struct Placement {
    pub image: Arc<FormulaImage>,
    /// UTF-16 index of the anchor character.
    pub index: usize,
    pub anchor: Anchor,
}

/// A layer and the image it shows (its contents are not set again for the same one).
type ImageLayer = (Retained<CALayer>, Option<Arc<FormulaImage>>);

#[derive(Default)]
pub struct FormulaLayers {
    layers: RefCell<Vec<ImageLayer>>,
    /// Frame and kind of each placed layer, for probe mode.
    placed: RefCell<Vec<(NSRect, Anchor)>>,
}

/// Geometry of the line holding a character, in view coordinates (flipped: y grows down).
struct LineAt {
    /// x of the character.
    x: f64,
    baseline: f64,
    top: f64,
    height: f64,
    column_x: f64,
    column_width: f64,
}

/// UTF-16 range of the text TextKit 2 laid out for the visible part of the view (laid out
/// again now, so frames there are current after a restyle).
pub fn viewport(view: &NSTextView) -> Option<(usize, usize)> {
    let layout = view.textLayoutManager()?;
    let content = layout.textContentManager()?;
    let controller = layout.textViewportLayoutController();
    controller.layoutViewport();
    let range = controller.viewportRange()?;
    let start = content.documentRange().location();
    let from = content.offsetFromLocation_toLocation(&start, &range.location()).max(0) as usize;
    let to = content.offsetFromLocation_toLocation(&start, &range.endLocation()).max(0) as usize;
    Some((from, to))
}

impl FormulaLayers {
    /// Shows exactly `placements`, reusing layers.
    pub fn place(&self, view: &NSTextView, placements: &[Placement]) {
        view.setWantsLayer(true);
        let Some(host) = view.layer() else { return };
        let mut layers = self.layers.borrow_mut();
        let mut placed = self.placed.borrow_mut();
        placed.clear();
        let scale = view.window().map_or(2.0, |w| w.backingScaleFactor());
        let mut used = 0;
        for p in placements {
            let Some(line) = line_at(view, p.index) else { continue };
            let (w, h) = (p.image.width_pt, p.image.height_pt);
            let origin = match p.anchor {
                Anchor::Inline => NSPoint::new(line.x, line.baseline - p.image.baseline_pt),
                Anchor::Line => NSPoint::new(line.x, line.top + (line.height - h) / 2.0),
                Anchor::Below => NSPoint::new(line.column_x + (line.column_width - w) / 2.0, line.top + line.height + PREVIEW_GAP),
            };
            let frame = NSRect::new(origin, NSSize::new(w, h));
            if used == layers.len() {
                let layer = CALayer::new();
                host.addSublayer(&layer);
                layers.push((layer, None));
            }
            let (layer, shown) = &mut layers[used];
            if !shown.as_ref().is_some_and(|s| Arc::ptr_eq(s, &p.image)) {
                let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&p.image.png));
                let contents: Option<&AnyObject> = image.as_deref().map(AsRef::as_ref);
                // SAFETY: an NSImage is valid layer contents on macOS.
                unsafe { layer.setContents(contents) };
                layer.setContentsScale(scale);
                *shown = Some(Arc::clone(&p.image));
            }
            layer.setFrame(frame);
            layer.setHidden(false);
            placed.push((frame, p.anchor));
            used += 1;
        }
        for (layer, _) in &layers[used..] {
            layer.setHidden(true);
        }
    }

    /// `x y w h kind` of each placed layer.
    pub fn describe(&self) -> Vec<String> {
        self.placed
            .borrow()
            .iter()
            .map(|(r, anchor)| {
                let kind = if *anchor == Anchor::Below { "preview" } else { "typeset" };
                format!("{:.1} {:.1} {:.1} {:.1} {kind}", r.origin.x, r.origin.y, r.size.width, r.size.height)
            })
            .collect()
    }
}

/// Space between a display formula's source and its preview.
pub const PREVIEW_GAP: f64 = 6.0;

/// The line fragment holding character `index`, laid out by TextKit 2.
fn line_at(view: &NSTextView, index: usize) -> Option<LineAt> {
    let layout = view.textLayoutManager()?;
    let content = layout.textContentManager()?;
    let start = content.documentRange().location();
    let location = content.locationFromLocation_withOffset(&start, index as isize)?;
    let fragment = layout.textLayoutFragmentForLocation(&location)?;
    let paragraph_start = content.offsetFromLocation_toLocation(&start, &fragment.rangeInElement().location()).max(0) as usize;
    let within = index.saturating_sub(paragraph_start);
    let lines = fragment.textLineFragments();
    let line = lines
        .iter()
        .find(|l| {
            let r = l.characterRange();
            within >= r.location && within < r.location + r.length.max(1)
        })
        .or_else(|| lines.iter().last())?;
    let frame = fragment.layoutFragmentFrame();
    let bounds = line.typographicBounds();
    let glyphs = line.glyphOrigin();
    let at = line.locationForCharacterAtIndex(within.min(line.characterRange().location + line.characterRange().length) as isize);
    let origin = view.textContainerOrigin();
    let column_width = unsafe { view.textContainer() }.map_or(view.frame().size.width, |c| c.size().width);
    let top = origin.y + frame.origin.y + bounds.origin.y;
    Some(LineAt {
        x: origin.x + frame.origin.x + bounds.origin.x + at.x,
        baseline: top + glyphs.y,
        top,
        height: bounds.size.height,
        column_x: origin.x,
        column_width,
    })
}
