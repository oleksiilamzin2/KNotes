//! What the live preview draws in place of hidden source, inside TextKit 2 layout fragments
//! (ADR 0017): typeset formulas, list bullets and task boxes. The text keeps the source (hidden, its room
//! reserved by the live preview) and the paragraph's layout fragment draws at that place, so
//! the drawing moves with its text however TextKit lays out, scrolls or resizes (layers over
//! the view lagged a run loop behind and jumped while scrolling). Clicks reach the text.

use std::sync::Arc;

use knotes_editor::FormulaImage;
use objc2::rc::Retained;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_app_kit::{
    NSBezierPath, NSColor, NSCompositingOperation, NSGraphicsContext, NSImage, NSTextElement, NSTextLayoutFragment, NSTextLineFragment,
    NSTextLocation, NSTextRange, NSTextView,
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

/// Something to draw in a paragraph, at UTF-16 `offset` from the paragraph's start.
pub enum Mark {
    Formula { offset: usize, image: Arc<FormulaImage>, anchor: Anchor },
    /// A list bullet in the room of a hidden `- `, its centre `centre` above the baseline.
    Bullet { offset: usize, centre: f64 },
    /// A task's circle in the room of a hidden `- [ ] `, ticked for `- [x] `.
    Task { offset: usize, centre: f64, checked: bool },
}

/// Space between a display formula's source and its preview.
pub const PREVIEW_GAP: f64 = 6.0;
/// Room a hidden bullet marker keeps, and where in it the dot is centred.
pub const BULLET_ROOM: f64 = 20.0;
const BULLET_X: f64 = 7.0;
const BULLET_SIZE: f64 = 5.0;
/// Room a hidden task marker keeps, and the circle drawn at its start.
pub const TASK_ROOM: f64 = 22.0;
const TASK_BOX: f64 = 14.0;

pub struct Ivars {
    /// Each mark with its image, for formulas.
    marks: Vec<(Mark, Option<Retained<NSImage>>)>,
}

define_class!(
    #[unsafe(super(NSTextLayoutFragment, NSObject))]
    #[name = "KnotesPreviewFragment"]
    #[ivars = Ivars]
    pub struct PreviewFragment;

    impl PreviewFragment {
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
                match (mark, image) {
                    // SAFETY: a valid image drawn into the current context.
                    (Mark::Formula { .. }, Some(image)) => unsafe {
                        image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(rect, NSZeroRect, NSCompositingOperation::SourceOver, 1.0, true, None)
                    },
                    (Mark::Bullet { .. }, _) => {
                        NSColor::secondaryLabelColor().setFill();
                        NSBezierPath::bezierPathWithOvalInRect(rect).fill();
                    }
                    (Mark::Task { checked, .. }, _) => draw_task_box(rect, *checked),
                    _ => {}
                }
            }
            NSGraphicsContext::setCurrentContext(previous.as_deref());
        }

        /// The drawing may reach past the text (a preview sits in the paragraph spacing).
        #[unsafe(method(renderingSurfaceBounds))]
        fn rendering_surface_bounds(&self) -> NSRect {
            // SAFETY: the superclass's bounds of the text.
            let text: NSRect = unsafe { msg_send![super(self), renderingSurfaceBounds] };
            self.ivars().marks.iter().filter_map(|(mark, _)| self.mark_rect(mark)).fold(text, union)
        }
    }
);

impl PreviewFragment {
    /// A fragment for `element` drawing `marks` (formulas whose image loads).
    pub fn new(element: &NSTextElement, range: Option<&NSTextRange>, marks: Vec<Mark>) -> Retained<Self> {
        let marks = marks
            .into_iter()
            .filter_map(|m| match &m {
                Mark::Formula { image, .. } => NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&image.png)).map(|i| (m, Some(i))),
                Mark::Bullet { .. } | Mark::Task { .. } => Some((m, None)),
            })
            .collect();
        let this = Self::alloc().set_ivars(Ivars { marks });
        // SAFETY: NSTextLayoutFragment's designated initializer.
        unsafe { msg_send![super(this), initWithTextElement: element, range: range] }
    }

    /// Where `mark` is drawn, in this fragment's coordinates, once it is laid out.
    fn mark_rect(&self, mark: &Mark) -> Option<NSRect> {
        let lines = self.textLineFragments();
        let (offset, below) = match mark {
            Mark::Formula { offset, anchor, .. } => (*offset, *anchor == Anchor::Below),
            Mark::Bullet { offset, .. } | Mark::Task { offset, .. } => (*offset, false),
        };
        let line: Retained<NSTextLineFragment> =
            if below { lines.iter().last()? } else { lines.iter().find(|l| contains(l, offset)).or_else(|| lines.iter().last())? };
        let bounds = line.typographicBounds();
        let x = bounds.origin.x + line.locationForCharacterAtIndex(offset as isize).x;
        let baseline = bounds.origin.y + line.glyphOrigin().y;
        let rect = match mark {
            Mark::Formula { image, anchor, .. } => {
                let size = NSSize::new(image.width_pt, image.height_pt);
                let origin = match anchor {
                    Anchor::Inline => NSPoint::new(x, baseline - image.baseline_pt),
                    Anchor::Line => NSPoint::new(x, bounds.origin.y + (bounds.size.height - size.height) / 2.0),
                    Anchor::Below => {
                        let column = self.textLayoutManager().and_then(|l| l.textContainer()).map_or(0.0, |c| c.size().width);
                        NSPoint::new((column - size.width) / 2.0 - self.layoutFragmentFrame().origin.x, bounds.origin.y + bounds.size.height + PREVIEW_GAP)
                    }
                };
                NSRect::new(origin, size)
            }
            Mark::Bullet { centre, .. } => {
                let half = BULLET_SIZE / 2.0;
                NSRect::new(NSPoint::new(x + BULLET_X - half, baseline - centre - half), NSSize::new(BULLET_SIZE, BULLET_SIZE))
            }
            Mark::Task { centre, .. } => {
                NSRect::new(NSPoint::new(x + 1.0, baseline - centre - TASK_BOX / 2.0), NSSize::new(TASK_BOX, TASK_BOX))
            }
        };
        Some(rect)
    }

    /// `formula-image: x y w h kind` or `decoration: kind x y w h` of each mark, in the text
    /// view's coordinates.
    fn describe(&self, view: &NSTextView) -> Vec<String> {
        let frame = self.layoutFragmentFrame();
        let origin = view.textContainerOrigin();
        self.ivars()
            .marks
            .iter()
            .filter_map(|(mark, _)| {
                let r = self.mark_rect(mark)?;
                let (x, y, w, h) = (origin.x + frame.origin.x + r.origin.x, origin.y + frame.origin.y + r.origin.y, r.size.width, r.size.height);
                Some(match mark {
                    Mark::Formula { anchor, .. } => {
                        let kind = if *anchor == Anchor::Below { "preview" } else { "typeset" };
                        format!("formula-image: {x:.1} {y:.1} {w:.1} {h:.1} {kind}")
                    }
                    Mark::Bullet { .. } => format!("decoration: bullet {x:.1} {y:.1} {w:.1} {h:.1}"),
                    Mark::Task { checked, .. } => {
                        let kind = if *checked { "task-done" } else { "task" };
                        format!("decoration: {kind} {x:.1} {y:.1} {w:.1} {h:.1}")
                    }
                })
            })
            .collect()
    }
}

/// What TextKit drew in place of hidden source in the visible part of `view` (probe lines).
pub fn describe_visible(view: &NSTextView) -> Vec<String> {
    let Some(layout) = view.textLayoutManager() else { return Vec::new() };
    let controller = layout.textViewportLayoutController();
    controller.layoutViewport();
    let Some(viewport) = controller.viewportRange() else { return Vec::new() };
    let mut lines = Vec::new();
    let mut at = viewport.location();
    while let Some(fragment) = layout.textLayoutFragmentForLocation(&at) {
        if let Some(preview) = fragment.downcast_ref::<PreviewFragment>() {
            lines.extend(preview.describe(view));
        }
        let next = fragment.rangeInElement().endLocation();
        if next.compare(&viewport.endLocation()) != objc2_foundation::NSComparisonResult::Ascending
            || next.compare(&at) != objc2_foundation::NSComparisonResult::Descending
        {
            break;
        }
        at = next;
    }
    lines
}

/// A circle, outlined, or filled in the accent colour with a tick for a done task (the
/// owner's choice over Obsidian's square boxes).
fn draw_task_box(rect: NSRect, checked: bool) {
    let inset = NSRect::new(NSPoint::new(rect.origin.x + 0.5, rect.origin.y + 0.5), NSSize::new(rect.size.width - 1.0, rect.size.height - 1.0));
    let shape = NSBezierPath::bezierPathWithOvalInRect(inset);
    if !checked {
        NSColor::secondaryLabelColor().setStroke();
        shape.setLineWidth(1.2);
        shape.stroke();
        return;
    }
    NSColor::controlAccentColor().setFill();
    shape.fill();
    let (x, y, s) = (rect.origin.x, rect.origin.y, rect.size.width);
    let tick = NSBezierPath::bezierPath();
    tick.moveToPoint(NSPoint::new(x + s * 0.26, y + s * 0.52));
    tick.lineToPoint(NSPoint::new(x + s * 0.43, y + s * 0.70));
    tick.lineToPoint(NSPoint::new(x + s * 0.75, y + s * 0.32));
    tick.setLineWidth(1.8);
    tick.setLineCapStyle(objc2_app_kit::NSLineCapStyle::Round);
    tick.setLineJoinStyle(objc2_app_kit::NSLineJoinStyle::Round);
    NSColor::whiteColor().setStroke();
    tick.stroke();
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
