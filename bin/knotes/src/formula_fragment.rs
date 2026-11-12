//! Typeset formulas drawn inside TextKit 2 layout fragments (ADR 0017): the text keeps the
//! formula's source (hidden, its room reserved by the live preview) and the paragraph's
//! layout fragment draws the image at that place, so the image moves with its text however
//! TextKit lays out, scrolls or resizes (layers over the view lagged a run loop behind and
//! jumped while scrolling). A click on a formula reaches the text: the cursor goes into the
//! source.

use std::sync::Arc;

use knotes_editor::FormulaImage;
use objc2::rc::Retained;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_app_kit::{
    NSCompositingOperation, NSGraphicsContext, NSImage, NSTextElement, NSTextLayoutFragment, NSTextLineFragment, NSTextLocation,
    NSTextRange, NSTextView,
};
use objc2_core_graphics::CGContext;
use objc2_foundation::{NSData, NSObject, NSPoint, NSRect, NSSize, NSZeroRect};

/// Where a formula's image goes in its paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Inline, from the formula's first character, its baseline on the text baseline.
    Inline,
    /// A display formula's own (centred) line, vertically centred in it.
    Line,
    /// Below the paragraph's last line (a display formula being edited): a preview, centred in
    /// the text column.
    Below,
}

/// An image to draw in a paragraph, at UTF-16 `offset` from the paragraph's start.
pub struct Mark {
    pub offset: usize,
    pub image: Arc<FormulaImage>,
    pub anchor: Anchor,
}

/// Space between a display formula's source and its preview.
pub const PREVIEW_GAP: f64 = 6.0;

pub struct Ivars {
    marks: Vec<(Mark, Retained<NSImage>)>,
}

define_class!(
    #[unsafe(super(NSTextLayoutFragment, NSObject))]
    #[name = "KnotesFormulaFragment"]
    #[ivars = Ivars]
    pub struct FormulaFragment;

    impl FormulaFragment {
        #[unsafe(method(drawAtPoint:inContext:))]
        fn draw(&self, point: NSPoint, context: &CGContext) {
            // SAFETY: the superclass draws the text, as it would without this override.
            let _: () = unsafe { msg_send![super(self), drawAtPoint: point, inContext: context] };
            let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(context, true);
            let previous = NSGraphicsContext::currentContext();
            NSGraphicsContext::setCurrentContext(Some(&graphics));
            for (mark, image) in &self.ivars().marks {
                let Some(rect) = self.mark_rect(mark) else { continue };
                let rect = NSRect::new(NSPoint::new(point.x + rect.origin.x, point.y + rect.origin.y), rect.size);
                // SAFETY: a valid image drawn into the current context.
                unsafe {
                    image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(rect, NSZeroRect, NSCompositingOperation::SourceOver, 1.0, true, None)
                };
            }
            NSGraphicsContext::setCurrentContext(previous.as_deref());
        }

        /// The images may reach past the text (a preview sits in the paragraph spacing).
        #[unsafe(method(renderingSurfaceBounds))]
        fn rendering_surface_bounds(&self) -> NSRect {
            // SAFETY: the superclass's bounds of the text.
            let text: NSRect = unsafe { msg_send![super(self), renderingSurfaceBounds] };
            self.ivars().marks.iter().filter_map(|(mark, _)| self.mark_rect(mark)).fold(text, union)
        }
    }
);

impl FormulaFragment {
    /// A fragment for `element` drawing `marks` (those whose image loads).
    pub fn new(element: &NSTextElement, range: Option<&NSTextRange>, marks: Vec<Mark>) -> Retained<Self> {
        let marks = marks
            .into_iter()
            .filter_map(|m| NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&m.image.png)).map(|image| (m, image)))
            .collect();
        let this = Self::alloc().set_ivars(Ivars { marks });
        // SAFETY: NSTextLayoutFragment's designated initializer.
        unsafe { msg_send![super(this), initWithTextElement: element, range: range] }
    }

    /// Where `mark`'s image goes, in this fragment's coordinates, once it is laid out.
    fn mark_rect(&self, mark: &Mark) -> Option<NSRect> {
        let lines = self.textLineFragments();
        let size = NSSize::new(mark.image.width_pt, mark.image.height_pt);
        let line: Retained<NSTextLineFragment> = match mark.anchor {
            Anchor::Below => lines.iter().last()?,
            _ => lines.iter().find(|l| contains(l, mark.offset)).or_else(|| lines.iter().last())?,
        };
        let bounds = line.typographicBounds();
        let origin = match mark.anchor {
            Anchor::Inline => {
                let at = line.locationForCharacterAtIndex(mark.offset as isize);
                NSPoint::new(bounds.origin.x + at.x, bounds.origin.y + line.glyphOrigin().y - mark.image.baseline_pt)
            }
            Anchor::Line => {
                let at = line.locationForCharacterAtIndex(mark.offset as isize);
                NSPoint::new(bounds.origin.x + at.x, bounds.origin.y + (bounds.size.height - size.height) / 2.0)
            }
            Anchor::Below => {
                let column = self.textLayoutManager().and_then(|l| l.textContainer()).map_or(0.0, |c| c.size().width);
                NSPoint::new((column - size.width) / 2.0 - self.layoutFragmentFrame().origin.x, bounds.origin.y + bounds.size.height + PREVIEW_GAP)
            }
        };
        Some(NSRect::new(origin, size))
    }

    /// `x y w h kind` of each image, in the text view's coordinates.
    fn describe(&self, view: &NSTextView) -> Vec<String> {
        let frame = self.layoutFragmentFrame();
        let origin = view.textContainerOrigin();
        self.ivars()
            .marks
            .iter()
            .filter_map(|(mark, _)| {
                let r = self.mark_rect(mark)?;
                let kind = if mark.anchor == Anchor::Below { "preview" } else { "typeset" };
                let (x, y) = (origin.x + frame.origin.x + r.origin.x, origin.y + frame.origin.y + r.origin.y);
                Some(format!("{x:.1} {y:.1} {:.1} {:.1} {kind}", r.size.width, r.size.height))
            })
            .collect()
    }
}

/// `x y w h kind` of each formula image TextKit laid out for the visible part of `view`, for
/// probe mode.
pub fn describe_visible(view: &NSTextView) -> Vec<String> {
    let Some(layout) = view.textLayoutManager() else { return Vec::new() };
    let controller = layout.textViewportLayoutController();
    controller.layoutViewport();
    let Some(viewport) = controller.viewportRange() else { return Vec::new() };
    let mut images = Vec::new();
    let mut at = viewport.location();
    while let Some(fragment) = layout.textLayoutFragmentForLocation(&at) {
        if let Some(formulas) = fragment.downcast_ref::<FormulaFragment>() {
            images.extend(formulas.describe(view));
        }
        let next = fragment.rangeInElement().endLocation();
        if next.compare(&viewport.endLocation()) != objc2_foundation::NSComparisonResult::Ascending
            || next.compare(&at) != objc2_foundation::NSComparisonResult::Descending
        {
            break;
        }
        at = next;
    }
    images
}

fn contains(line: &NSTextLineFragment, offset: usize) -> bool {
    let r = line.characterRange();
    offset >= r.location && offset < r.location + r.length.max(1)
}

fn union(a: NSRect, b: NSRect) -> NSRect {
    let x0 = a.origin.x.min(b.origin.x);
    let y0 = a.origin.y.min(b.origin.y);
    let x1 = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
    NSRect::new(NSPoint::new(x0, y0), NSSize::new(x1 - x0, y1 - y0))
}
