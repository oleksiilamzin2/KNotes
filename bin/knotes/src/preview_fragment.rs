//! What the live preview draws in place of hidden source, inside TextKit 2 layout fragments
//! (ADR 0017): typeset formulas, images (ADR 0020), list bullets, task circles, quote bars,
//! code backgrounds and the arrows of links to websites. The text keeps the source (hidden, its room
//! reserved by the live preview) and the paragraph's layout fragment draws at that place, so
//! the drawing moves with its text however TextKit lays out, scrolls or resizes (layers over
//! the view lagged a run loop behind and jumped while scrolling). Clicks reach the text.

use std::rc::Rc;
use std::sync::Arc;

use knotes_editor::FormulaImage;

use crate::image_loader::DeferredImage;
use objc2::rc::Retained;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBezierPath, NSColor, NSCompositingOperation, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSGraphicsContext, NSImage,
    NSImageSymbolConfiguration, NSStringDrawing, NSTextElement, NSTextElementProvider, NSTextLayoutFragment, NSTextLineFragment,
    NSTextLocation, NSTextRange, NSTextView,
};
use objc2_core_graphics::CGContext;
use objc2_foundation::{NSData, NSDictionary, NSObject, NSPoint, NSRect, NSSize, NSString, NSZeroRect};

/// Where a formula's or an image's picture goes in its paragraph.
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
    /// An image embedded alone on its line, `size` points large (`Line` or `Below`), decoded
    /// when first drawn; with its resize handle while the pointer is over it.
    Image { offset: usize, image: Rc<DeferredImage>, size: NSSize, anchor: Anchor, handle: bool },
    /// A list bullet in the room of a hidden `- `, its centre `centre` above the baseline.
    Bullet { offset: usize, centre: f64 },
    /// A task's circle in the room of a hidden `- [ ] `, ticked for `- [x] `.
    Task { offset: usize, centre: f64, checked: bool },
    /// A quote level's bar down the whole paragraph, `x` from the text column's start.
    QuoteBar { x: f64 },
    /// A code block's background across the column behind the paragraph, its corners rounded
    /// at the block's top and bottom.
    CodeBackground { top: bool, bottom: bool },
    /// A callout's tinted box across the column behind the paragraph, rounded at the callout's
    /// top and bottom.
    CalloutBox { colour: Retained<NSColor>, top: bool, bottom: bool },
    /// A callout's icon in the room of its hidden `[!kind]`, and its kind's name after it when
    /// no title is written.
    CalloutIcon { offset: usize, centre: f64, colour: Retained<NSColor>, symbol: &'static str, label: Option<String>, font: Retained<NSFont> },
    /// An arrow after a link to a website, `shift` points from the location of character
    /// `offset` (in the room kerning keeps there).
    External { offset: usize, centre: f64, shift: f64 },
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
const QUOTE_BAR: f64 = 3.0;
const CODE_CORNER: f64 = 5.0;
/// A callout's icon.
const CALLOUT_ICON: f64 = 14.0;
/// An image's resize handle: its width, tallest height and inset from the right edge.
const IMAGE_HANDLE_WIDTH: f64 = 5.0;
const IMAGE_HANDLE_HEIGHT: f64 = 36.0;
const IMAGE_HANDLE_INSET: f64 = 3.0;
/// Room after a link to a website for its arrow, and the arrow's size.
pub const EXTERNAL_ROOM: f64 = 13.0;
const EXTERNAL_ARROW: f64 = 7.0;

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
            let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(context, true);
            let previous = NSGraphicsContext::currentContext();
            NSGraphicsContext::setCurrentContext(Some(&graphics));
            // Backgrounds under the text.
            for (mark, _) in &self.ivars().marks {
                let Some(rect) = self.mark_rect(mark) else { continue };
                let rect = NSRect::new(NSPoint::new(point.x + rect.origin.x, point.y + rect.origin.y), rect.size);
                match mark {
                    Mark::CodeBackground { top, bottom } => {
                        NSColor::quaternarySystemFillColor().setFill();
                        rounded(rect, CODE_CORNER, *top, *bottom).fill();
                    }
                    Mark::CalloutBox { colour, top, bottom } => {
                        colour.colorWithAlphaComponent(0.1).setFill();
                        rounded(rect, CODE_CORNER, *top, *bottom).fill();
                    }
                    _ => {}
                }
            }
            NSGraphicsContext::setCurrentContext(previous.as_deref());
            // SAFETY: the superclass draws the text, as it would without this override.
            let _: () = unsafe { msg_send![super(self), drawAtPoint: point, inContext: context] };
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
                    (Mark::Image { image, handle, .. }, _) => {
                        image.drawn_at.set(Some(self.layoutFragmentFrame().origin.y + rect.origin.y - point.y));
                        if let Some(image) = image.get() {
                            // SAFETY: as above.
                            unsafe { image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(rect, NSZeroRect, NSCompositingOperation::SourceOver, 1.0, true, None) };
                        } else {
                            // Being decoded: its place, faintly.
                            NSColor::quaternarySystemFillColor().setFill();
                            rounded(rect, CODE_CORNER, true, true).fill();
                        }
                        if *handle {
                            draw_image_handle(image_handle(rect));
                        }
                    }
                    (Mark::Bullet { .. }, _) => {
                        NSColor::secondaryLabelColor().setFill();
                        NSBezierPath::bezierPathWithOvalInRect(rect).fill();
                    }
                    (Mark::Task { checked, .. }, _) => draw_task_box(rect, *checked),
                    (Mark::CodeBackground { .. } | Mark::CalloutBox { .. }, _) => {}
                    (Mark::CalloutIcon { colour, symbol, label, font, .. }, _) => draw_callout_icon(rect, colour, symbol, label.as_deref(), font),
                    (Mark::External { .. }, _) => draw_external_arrow(rect),
                    (Mark::QuoteBar { .. }, _) => {
                        NSColor::controlAccentColor().setFill();
                        // Square ends: the bars of consecutive lines join into one.
                        NSBezierPath::bezierPathWithRect(rect).fill();
                    }
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

                _ => Some((m, None)),
            })
            .collect();
        let this = Self::alloc().set_ivars(Ivars { marks });
        // SAFETY: NSTextLayoutFragment's designated initializer.
        unsafe { msg_send![super(this), initWithTextElement: element, range: range] }
    }

    /// Where `mark` is drawn, in this fragment's coordinates, once it is laid out.
    fn mark_rect(&self, mark: &Mark) -> Option<NSRect> {
        if let Mark::QuoteBar { x } = mark {
            let frame = self.layoutFragmentFrame();
            return Some(NSRect::new(NSPoint::new(x + 1.0 - frame.origin.x, 0.0), NSSize::new(QUOTE_BAR, frame.size.height)));
        }
        if let Mark::CodeBackground { .. } | Mark::CalloutBox { .. } = mark {
            let frame = self.layoutFragmentFrame();
            let column = self.textLayoutManager().and_then(|l| l.textContainer()).map_or(frame.size.width, |c| c.size().width);
            return Some(NSRect::new(NSPoint::new(-frame.origin.x, 0.0), NSSize::new(column, frame.size.height)));
        }
        let lines = self.textLineFragments();
        let (offset, below) = match mark {
            Mark::Formula { offset, anchor, .. } | Mark::Image { offset, anchor, .. } => (*offset, *anchor == Anchor::Below),
            Mark::Bullet { offset, .. } | Mark::Task { offset, .. } | Mark::External { offset, .. } | Mark::CalloutIcon { offset, .. } => (*offset, false),
            Mark::QuoteBar { .. } | Mark::CodeBackground { .. } | Mark::CalloutBox { .. } => return None,
        };
        let line: Retained<NSTextLineFragment> =
            if below { lines.iter().last()? } else { lines.iter().find(|l| contains(l, offset)).or_else(|| lines.iter().last())? };
        let bounds = line.typographicBounds();
        let x = bounds.origin.x + line.locationForCharacterAtIndex(offset as isize).x;
        let baseline = bounds.origin.y + line.glyphOrigin().y;
        let rect = match mark {
            Mark::Formula { .. } | Mark::Image { .. } => {
                let (size, anchor, above) = match mark {
                    Mark::Formula { image, anchor, .. } => (NSSize::new(image.width_pt, image.height_pt), anchor, image.baseline_pt),
                    Mark::Image { size, anchor, .. } => (*size, anchor, size.height),
                    _ => return None,
                };
                let origin = match anchor {
                    Anchor::Inline => NSPoint::new(x, baseline - above),
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
            Mark::External { centre, shift, .. } => {
                let half = EXTERNAL_ARROW / 2.0;
                NSRect::new(NSPoint::new(x + shift, baseline - centre - half), NSSize::new(EXTERNAL_ARROW, EXTERNAL_ARROW))
            }
            Mark::CalloutIcon { centre, label, font, .. } => {
                let width = CALLOUT_ICON + label.as_deref().map_or(0.0, |l| 4.0 + label_width(l, font));
                NSRect::new(NSPoint::new(x + 1.0, baseline - centre - CALLOUT_ICON / 2.0), NSSize::new(width, CALLOUT_ICON))
            }
            Mark::QuoteBar { .. } | Mark::CodeBackground { .. } | Mark::CalloutBox { .. } => return None,
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
                    Mark::Image { anchor, handle, .. } => {
                        let kind = if *anchor == Anchor::Below { "preview" } else { "shown" };
                        let mut line = format!("image: {x:.1} {y:.1} {w:.1} {h:.1} {kind}");
                        if *handle {
                            let r = image_handle(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)));
                            line += &format!("\ndecoration: image-handle {:.1} {:.1} {:.1} {:.1}", r.origin.x, r.origin.y, r.size.width, r.size.height);
                        }
                        line
                    }
                    Mark::Bullet { .. } => format!("decoration: bullet {x:.1} {y:.1} {w:.1} {h:.1}"),
                    Mark::Task { checked, .. } => {
                        let kind = if *checked { "task-done" } else { "task" };
                        format!("decoration: {kind} {x:.1} {y:.1} {w:.1} {h:.1}")
                    }
                    Mark::QuoteBar { .. } => format!("decoration: quote {x:.1} {y:.1} {w:.1} {h:.1}"),
                    Mark::CodeBackground { .. } => format!("decoration: code {x:.1} {y:.1} {w:.1} {h:.1}"),
                    Mark::CalloutBox { .. } => format!("decoration: callout-box {x:.1} {y:.1} {w:.1} {h:.1}"),
                    Mark::CalloutIcon { .. } => format!("decoration: callout-icon {x:.1} {y:.1} {w:.1} {h:.1}"),
                    Mark::External { .. } => format!("decoration: external {x:.1} {y:.1} {w:.1} {h:.1}"),
                })
            })
            .collect()
    }
}

/// The image under `point` (text view coordinates), shown on its own line or below its source
/// (`below`): the character where its line starts, and its rect in the view.
pub fn image_at(view: &NSTextView, point: NSPoint) -> Option<(usize, NSRect, bool)> {
    let layout = view.textLayoutManager()?;
    let origin = view.textContainerOrigin();
    let fragment = layout.textLayoutFragmentForPosition(NSPoint::new(point.x - origin.x, point.y - origin.y))?;
    let preview = fragment.downcast_ref::<PreviewFragment>()?;
    let content = layout.textContentManager()?;
    let start = content.offsetFromLocation_toLocation(&content.documentRange().location(), &fragment.rangeInElement().location()).max(0) as usize;
    let frame = fragment.layoutFragmentFrame();
    // The image's paragraph is its line: the fragment starts where the line does.
    preview.ivars().marks.iter().find_map(|(mark, _)| {
        let Mark::Image { anchor, .. } = mark else { return None };
        let r = preview.mark_rect(mark)?;
        let rect = NSRect::new(NSPoint::new(origin.x + frame.origin.x + r.origin.x, origin.y + frame.origin.y + r.origin.y), r.size);
        Some((start, rect, *anchor == Anchor::Below))
    })
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

/// Where an image's resize handle is drawn: a pill inside its right edge, halfway down, in
/// the band where the pointer resizes it.
fn image_handle(image: NSRect) -> NSRect {
    let height = (image.size.height * 0.4).clamp(12.0, IMAGE_HANDLE_HEIGHT);
    let x = image.origin.x + image.size.width - IMAGE_HANDLE_INSET - IMAGE_HANDLE_WIDTH;
    NSRect::new(NSPoint::new(x, image.origin.y + (image.size.height - height) / 2.0), NSSize::new(IMAGE_HANDLE_WIDTH, height))
}

/// Light with a dark edge, to show on any picture.
fn draw_image_handle(rect: NSRect) {
    let pill = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, rect.size.width / 2.0, rect.size.width / 2.0);
    NSColor::whiteColor().colorWithAlphaComponent(0.95).setFill();
    pill.fill();
    NSColor::blackColor().colorWithAlphaComponent(0.45).setStroke();
    pill.setLineWidth(1.0);
    pill.stroke();
}

/// A callout's icon in its colour, with the kind's name after it in its colour (bold) when no
/// title is written.
fn draw_callout_icon(rect: NSRect, colour: &NSColor, symbol: &str, label: Option<&str>, font: &NSFont) {
    let icon = NSRect::new(rect.origin, NSSize::new(CALLOUT_ICON, CALLOUT_ICON));
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(12.0, 0.3)
        .configurationByApplyingConfiguration(&NSImageSymbolConfiguration::configurationWithHierarchicalColor(colour));
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None).and_then(|i| i.imageWithSymbolConfiguration(&config)) {
        let size = image.size();
        let scale = (CALLOUT_ICON / size.width.max(size.height)).min(1.0);
        let (w, h) = (size.width * scale, size.height * scale);
        let at = NSRect::new(NSPoint::new(icon.origin.x + (CALLOUT_ICON - w) / 2.0, icon.origin.y + (CALLOUT_ICON - h) / 2.0), NSSize::new(w, h));
        // SAFETY: a valid image drawn into the current context.
        unsafe { image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(at, NSZeroRect, NSCompositingOperation::SourceOver, 1.0, true, None) };
    }
    if let Some(label) = label {
        let attrs = label_attributes(font, colour);
        let top = rect.origin.y + (CALLOUT_ICON - (font.ascender() - font.descender())) / 2.0;
        // SAFETY: font and colour values of their attributes.
        unsafe { NSString::from_str(label).drawAtPoint_withAttributes(NSPoint::new(rect.origin.x + CALLOUT_ICON + 4.0, top), Some(&attrs)) };
    }
}

fn label_attributes(font: &NSFont, colour: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &[font.as_ref() as &AnyObject, colour.as_ref()])
}

fn label_width(label: &str, font: &NSFont) -> f64 {
    // SAFETY: font and colour values of their attributes.
    unsafe { NSString::from_str(label).sizeWithAttributes(Some(&label_attributes(font, &NSColor::labelColor()))) }.width
}

/// An arrow up and to the right in the link colour: the link leads out of the vault.
fn draw_external_arrow(rect: NSRect) {
    let (x0, y0, s) = (rect.origin.x, rect.origin.y, rect.size.width);
    let arrow = NSBezierPath::bezierPath();
    // Flipped: y0 is the top. The shaft from the bottom left, the head at the top right.
    arrow.moveToPoint(NSPoint::new(x0, y0 + s));
    arrow.lineToPoint(NSPoint::new(x0 + s, y0));
    arrow.moveToPoint(NSPoint::new(x0 + s * 0.35, y0));
    arrow.lineToPoint(NSPoint::new(x0 + s, y0));
    arrow.lineToPoint(NSPoint::new(x0 + s, y0 + s * 0.65));
    arrow.setLineWidth(1.3);
    arrow.setLineCapStyle(objc2_app_kit::NSLineCapStyle::Round);
    arrow.setLineJoinStyle(objc2_app_kit::NSLineJoinStyle::Round);
    NSColor::linkColor().setStroke();
    arrow.stroke();
}

/// `rect` with its top and/or bottom corners rounded.
fn rounded(rect: NSRect, radius: f64, top: bool, bottom: bool) -> Retained<NSBezierPath> {
    let (x0, y0) = (rect.origin.x, rect.origin.y);
    let (x1, y1) = (x0 + rect.size.width, y0 + rect.size.height);
    let (rt, rb) = (if top { radius } else { 0.0 }, if bottom { radius } else { 0.0 });
    let path = NSBezierPath::bezierPath();
    // Flipped: y0 is the top.
    path.moveToPoint(NSPoint::new(x0 + rt, y0));
    path.appendBezierPathWithArcFromPoint_toPoint_radius(NSPoint::new(x1, y0), NSPoint::new(x1, y1), rt);
    path.appendBezierPathWithArcFromPoint_toPoint_radius(NSPoint::new(x1, y1), NSPoint::new(x0, y1), rb);
    path.appendBezierPathWithArcFromPoint_toPoint_radius(NSPoint::new(x0, y1), NSPoint::new(x0, y0), rb);
    path.appendBezierPathWithArcFromPoint_toPoint_radius(NSPoint::new(x0, y0), NSPoint::new(x1, y0), rt);
    path.closePath();
    path
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
