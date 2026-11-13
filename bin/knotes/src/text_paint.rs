//! The live preview's attributes, composed on a `Canvas` before they reach the text storage
//! (ADR 0012): each UTF-16 unit gets a `Paint` (indices into a `Palette`), and the final runs
//! are set front to back, each once, with attribute sets made once and shared.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use knotes_editor::Canvas;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSKernAttributeName, NSMutableParagraphStyle, NSParagraphStyle,
    NSBackgroundColorAttributeName, NSParagraphStyleAttributeName, NSStrikethroughStyleAttributeName, NSTextAlignment, NSTextStorage, NSUnderlineStyle,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSRange, NSString};

/// One unit's attributes while a region is composed: a look (font, colour, test name; the top
/// bit for a line through it), a paragraph style and kerning, as palette indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    pub look: u16,
    pub para: u16,
    pub kern: u32,
}

/// The `look` bit for struck text.
pub const STRUCK: u16 = 1 << 15;
/// The `look` bit for text on a background (inline code).
pub const SHADED: u16 = 1 << 14;
/// The look itself, without the bits.
const LOOK: u16 = !(STRUCK | SHADED);

/// A paragraph style's settings; the default sets nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Para {
    pub first_indent: f64,
    pub head_indent: f64,
    /// Tabs advance next to nothing (a list item's hidden indentation).
    pub hidden_tabs: bool,
    pub centred: bool,
    pub min_line: f64,
    pub max_line: f64,
    pub spacing_after: f64,
}

type ParaKey = [u64; 7];

impl Para {
    fn key(&self) -> ParaKey {
        [
            self.first_indent.to_bits(),
            self.head_indent.to_bits(),
            u64::from(self.hidden_tabs),
            u64::from(self.centred),
            self.min_line.to_bits(),
            self.max_line.to_bits(),
            self.spacing_after.to_bits(),
        ]
    }

    fn style(&self) -> Retained<NSParagraphStyle> {
        let style = NSMutableParagraphStyle::new();
        style.setFirstLineHeadIndent(self.first_indent);
        style.setHeadIndent(self.head_indent);
        if self.hidden_tabs {
            style.setTabStops(Some(&NSArray::new()));
            style.setDefaultTabInterval(0.01);
        }
        if self.centred {
            style.setAlignment(NSTextAlignment::Center);
        }
        style.setMinimumLineHeight(self.min_line);
        style.setMaximumLineHeight(self.max_line);
        style.setParagraphSpacing(self.spacing_after);
        Retained::into_super(style)
    }
}

/// The looks, kerning values and paragraph styles one composition uses; index 0 of kerning
/// and paragraph styles means none.
pub struct Palette<'a> {
    looks: Vec<(&'a NSFont, &'a NSColor, &'static str)>,
    look_ids: HashMap<(*const NSFont, *const NSColor, &'static str), u16>,
    kerns: Vec<f64>,
    kern_ids: HashMap<u64, u32>,
    paras: Vec<Para>,
    para_ids: HashMap<ParaKey, u16>,
}

impl<'a> Palette<'a> {
    pub fn new() -> Self {
        Self {
            looks: Vec::new(),
            look_ids: HashMap::new(),
            kerns: vec![0.0],
            kern_ids: HashMap::new(),
            paras: vec![Para::default()],
            para_ids: HashMap::new(),
        }
    }

    pub fn look(&mut self, font: &'a NSFont, color: &'a NSColor, name: &'static str) -> u16 {
        let looks = &mut self.looks;
        *self.look_ids.entry((font, color, name)).or_insert_with(|| {
            looks.push((font, color, name));
            (looks.len() - 1) as u16
        })
    }

    /// Look `id` with colour `color` instead of `from` (others unchanged), keeping its line.
    pub fn recolor(&mut self, id: u16, from: &NSColor, color: &'a NSColor) -> u16 {
        let (font, old, name) = self.looks[usize::from(id & LOOK)];
        if !std::ptr::eq(old, from) {
            return id;
        }
        self.look(font, color, name) | (id & !LOOK)
    }

    pub fn kern(&mut self, points: f64) -> u32 {
        if points == 0.0 {
            return 0;
        }
        let kerns = &mut self.kerns;
        *self.kern_ids.entry(points.to_bits()).or_insert_with(|| {
            kerns.push(points);
            (kerns.len() - 1) as u32
        })
    }

    pub fn para(&mut self, para: Para) -> u16 {
        if para == Para::default() {
            return 0;
        }
        let paras = &mut self.paras;
        *self.para_ids.entry(para.key()).or_insert_with(|| {
            paras.push(para);
            (paras.len() - 1) as u16
        })
    }

    pub fn para_of(&self, id: u16) -> Para {
        self.paras[usize::from(id)]
    }
}

/// Attribute sets and paragraph styles made once and shared by every composition.
pub struct PaintCache {
    style_key: Retained<NSString>,
    /// Background of inline code.
    shade: Retained<NSColor>,
    styles: RefCell<HashMap<ParaKey, Retained<NSParagraphStyle>>>,
    attributes: RefCell<HashMap<AttributesKey, Retained<NSDictionary<NSString, AnyObject>>>>,
}

/// Font, colour, test name, kerning, paragraph style, line-through and background of an
/// attribute set.
type AttributesKey = (*const NSFont, *const NSColor, &'static str, u64, *const NSParagraphStyle, bool, bool);

impl PaintCache {
    /// `style_key` names the attribute carrying each range's test name.
    pub fn new(style_key: &str) -> Self {
        Self { style_key: NSString::from_str(style_key), shade: NSColor::quaternarySystemFillColor(), styles: RefCell::new(HashMap::new()), attributes: RefCell::new(HashMap::new()) }
    }

    fn attributes(&self, palette: &Palette, paint: Paint) -> Retained<NSDictionary<NSString, AnyObject>> {
        let (font, color, name) = palette.looks[usize::from(paint.look & LOOK)];
        let kern = palette.kerns[paint.kern as usize];
        let struck = paint.look & STRUCK != 0;
        let shaded = paint.look & SHADED != 0;
        let para = (paint.para != 0).then(|| {
            let para = palette.para_of(paint.para);
            self.styles.borrow_mut().entry(para.key()).or_insert_with(|| para.style()).clone()
        });
        let key: AttributesKey = (font, color, name, kern.to_bits(), para.as_deref().map_or(std::ptr::null(), |p| p as *const _), struck, shaded);
        self.attributes
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let style = NSString::from_str(name);
                let kern = NSNumber::numberWithDouble(kern);
                let line = NSNumber::numberWithInteger(NSUnderlineStyle::Single.0);
                // SAFETY: AppKit's attribute name constants live for the whole process.
                let mut keys = unsafe { vec![NSFontAttributeName, NSForegroundColorAttributeName, &*self.style_key] };
                let mut values: Vec<&AnyObject> = vec![font.as_ref(), color.as_ref(), style.as_ref()];
                if paint.kern != 0 {
                    // SAFETY: as above.
                    keys.push(unsafe { NSKernAttributeName });
                    values.push(kern.as_ref());
                }
                if let Some(para) = &para {
                    // SAFETY: as above.
                    keys.push(unsafe { NSParagraphStyleAttributeName });
                    values.push(para.as_ref());
                }
                if struck {
                    // SAFETY: as above.
                    keys.push(unsafe { NSStrikethroughStyleAttributeName });
                    values.push(line.as_ref());
                }
                if shaded {
                    // SAFETY: as above.
                    keys.push(unsafe { NSBackgroundColorAttributeName });
                    values.push(self.shade.as_ref());
                }
                NSDictionary::from_slices(&keys, &values)
            })
            .clone()
    }

    /// Sets the composed attributes, front to back, each run once.
    pub fn apply(&self, storage: &NSTextStorage, canvas: &Canvas<Paint>, palette: &Palette) {
        storage.beginEditing();
        for (range, paint) in canvas.runs() {
            let attrs = self.attributes(palette, paint);
            // SAFETY: values of their attributes' types.
            unsafe { storage.setAttributes_range(Some(&attrs), ns(&range)) };
        }
        storage.endEditing();
    }
}

fn ns(r: &Range<usize>) -> NSRange {
    NSRange::new(r.start, r.end - r.start)
}
