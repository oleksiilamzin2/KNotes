//! The settings' «Текст» tab: the font, the size and the line height of a note's text and of
//! the chat's, each its own, kept in the app's settings and taken by what is shown at once.

use std::cell::RefCell;

use knotes_session::{AppSettings, TextLook};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSColor, NSFont, NSFontManager, NSLayoutConstraint, NSMenuItem, NSPopUpButton, NSStepper, NSTextField, NSView};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSRect, NSString};

use crate::{strings, text_look};

/// A text's controls; a control's tag says whose it is (the note's 0, the chat's 1).
struct Row {
    font: Retained<NSPopUpButton>,
    size: Retained<NSTextField>,
    stepper: Retained<NSStepper>,
    line: Retained<NSPopUpButton>,
}

pub struct Ivars {
    rows: RefCell<Vec<Row>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesTextTab"]
    #[ivars = Ivars]
    pub struct TextTab;

    unsafe impl NSObjectProtocol for TextTab {}

    impl TextTab {
        /// A family chosen; the menu's first item is the system's font.
        #[unsafe(method(fontChosen:))]
        fn font_chosen(&self, sender: &NSPopUpButton) {
            let family = (sender.indexOfSelectedItem() > 0).then(|| sender.titleOfSelectedItem().map(|t| t.to_string())).flatten();
            self.write(sender.tag(), |look| look.font = family);
        }

        /// The size typed (Return, or the field left): taken when a number.
        #[unsafe(method(sizeTyped:))]
        fn size_typed(&self, sender: &NSTextField) {
            match sender.stringValue().to_string().trim().parse::<u32>() {
                Ok(size) => self.write(sender.tag(), |look| look.size = Some(size.clamp(*TextLook::SIZES.start(), *TextLook::SIZES.end()))),
                Err(_) => self.refresh(),
            }
        }

        /// The stepper's arrows.
        #[unsafe(method(sizeStepped:))]
        fn size_stepped(&self, sender: &NSStepper) {
            let size = sender.integerValue().max(1) as u32;
            self.write(sender.tag(), |look| look.size = Some(size));
        }

        /// A line height chosen: its item's tag is the percent.
        #[unsafe(method(lineChosen:))]
        fn line_chosen(&self, sender: &NSPopUpButton) {
            let percent = sender.selectedTag().max(100) as u32;
            self.write(sender.tag(), |look| look.line = Some(percent));
        }
    }
);

impl TextTab {
    pub fn make(mtm: MainThreadMarker) -> (Retained<Self>, Retained<NSView>) {
        let this: Retained<Self> = {
            let this = Self::alloc(mtm).set_ivars(Ivars { rows: RefCell::new(Vec::new()) });
            unsafe { msg_send![super(this), init] }
        };
        let view = NSView::new(mtm);
        let add = |v: &NSView| {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            view.addSubview(v);
        };
        let heads: Vec<Retained<NSTextField>> = [strings::TEXT_FONT, strings::TEXT_SIZE, strings::TEXT_LINE]
            .iter()
            .map(|head| {
                let label = NSTextField::labelWithString(&NSString::from_str(head), mtm);
                label.setTextColor(Some(&NSColor::secondaryLabelColor()));
                label.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
                add(&label);
                label
            })
            .collect();
        let families = NSFontManager::sharedFontManager(mtm).availableFontFamilies();
        let mut constraints = vec![view.widthAnchor().constraintEqualToConstant(560.0), heads[0].topAnchor().constraintEqualToAnchor_constant(&view.topAnchor(), 24.0)];
        let mut rows: Vec<Row> = Vec::new();
        let mut above = heads[0].bottomAnchor();
        for (tag, name) in [strings::TEXT_NOTE, strings::TEXT_CHAT].iter().enumerate() {
            let tag = tag as isize;
            let name = NSTextField::labelWithString(&NSString::from_str(name), mtm);
            let font = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
            font.addItemWithTitle(&NSString::from_str(strings::TEXT_SYSTEM_FONT));
            if let Some(menu) = font.menu() {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            font.addItemsWithTitles(&families);
            let size = NSTextField::textFieldWithString(&NSString::new(), mtm);
            let stepper = NSStepper::new(mtm);
            stepper.setMinValue(f64::from(*TextLook::SIZES.start()));
            stepper.setMaxValue(f64::from(*TextLook::SIZES.end()));
            stepper.setIncrement(1.0);
            stepper.setValueWraps(false);
            let unit = NSTextField::labelWithString(&NSString::from_str(strings::TEXT_POINTS), mtm);
            let line = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
            for percent in TextLook::LINES.step_by(10) {
                line.addItemWithTitle(&NSString::from_str(&line_title(percent)));
                if let Some(item) = line.lastItem() {
                    item.setTag(percent as isize);
                }
            }
            // SAFETY: the tab lives as long as the settings window, kept with it.
            unsafe {
                font.setTarget(Some(&this));
                font.setAction(Some(sel!(fontChosen:)));
                size.setTarget(Some(&this));
                size.setAction(Some(sel!(sizeTyped:)));
                stepper.setTarget(Some(&this));
                stepper.setAction(Some(sel!(sizeStepped:)));
                line.setTarget(Some(&this));
                line.setAction(Some(sel!(lineChosen:)));
            }
            size.cell().expect("a text field has a cell").setSendsActionOnEndEditing(true);
            font.setTag(tag);
            size.setTag(tag);
            stepper.setTag(tag);
            line.setTag(tag);
            for v in [&*name as &NSView, &font, &size, &stepper, &unit, &line] {
                add(v);
            }
            constraints.extend([
                font.topAnchor().constraintEqualToAnchor_constant(&above, 8.0),
                name.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 24.0),
                name.widthAnchor().constraintEqualToConstant(70.0),
                name.centerYAnchor().constraintEqualToAnchor(&font.centerYAnchor()),
                font.leadingAnchor().constraintEqualToAnchor_constant(&name.trailingAnchor(), 8.0),
                font.widthAnchor().constraintEqualToConstant(170.0),
                size.leadingAnchor().constraintEqualToAnchor_constant(&font.trailingAnchor(), 16.0),
                size.widthAnchor().constraintEqualToConstant(40.0),
                size.centerYAnchor().constraintEqualToAnchor(&font.centerYAnchor()),
                stepper.leadingAnchor().constraintEqualToAnchor_constant(&size.trailingAnchor(), 4.0),
                stepper.centerYAnchor().constraintEqualToAnchor(&font.centerYAnchor()),
                unit.leadingAnchor().constraintEqualToAnchor_constant(&stepper.trailingAnchor(), 4.0),
                unit.centerYAnchor().constraintEqualToAnchor(&font.centerYAnchor()),
                line.leadingAnchor().constraintEqualToAnchor_constant(&unit.trailingAnchor(), 16.0),
                line.centerYAnchor().constraintEqualToAnchor(&font.centerYAnchor()),
                line.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            ]);
            if tag == 0 {
                // Each column's head over its first control.
                constraints.extend([
                    heads[0].leadingAnchor().constraintEqualToAnchor_constant(&font.leadingAnchor(), 2.0),
                    heads[1].leadingAnchor().constraintEqualToAnchor_constant(&size.leadingAnchor(), 2.0),
                    heads[1].centerYAnchor().constraintEqualToAnchor(&heads[0].centerYAnchor()),
                    heads[2].leadingAnchor().constraintEqualToAnchor_constant(&line.leadingAnchor(), 2.0),
                    heads[2].centerYAnchor().constraintEqualToAnchor(&heads[0].centerYAnchor()),
                ]);
            }
            above = font.bottomAnchor();
            rows.push(Row { font, size, stepper, line });
        }
        let note = NSTextField::wrappingLabelWithString(&NSString::from_str(strings::TEXT_NOTE_BELOW), mtm);
        note.setTextColor(Some(&NSColor::secondaryLabelColor()));
        note.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        add(&note);
        constraints.extend([
            note.topAnchor().constraintEqualToAnchor_constant(&above, 14.0),
            note.leadingAnchor().constraintEqualToAnchor(&rows[0].font.leadingAnchor()),
            note.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            // Held at the top; the window, as tall as its tallest tab, leaves the room below.
            view.bottomAnchor().constraintGreaterThanOrEqualToAnchor_constant(&note.bottomAnchor(), 24.0),
        ]);
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        *this.ivars().rows.borrow_mut() = rows;
        this.refresh();
        (this, view)
    }

    /// The controls as the settings are now.
    pub fn refresh(&self) {
        for (row, (look, own)) in self.ivars().rows.borrow().iter().zip(looks()) {
            match &look.font {
                // A family that is not on this Mac: the system's font is what shows.
                Some(family) if row.font.indexOfItemWithTitle(&NSString::from_str(family)) > 0 => row.font.selectItemWithTitle(&NSString::from_str(family)),
                _ => row.font.selectItemAtIndex(0),
            }
            let size = text_look::size(&look, own).round() as isize;
            row.size.setStringValue(&NSString::from_str(&size.to_string()));
            row.stepper.setIntegerValue(size);
            row.line.selectItemWithTag(look.line.unwrap_or(100).div_ceil(10) as isize * 10);
        }
    }

    /// Changes the text's look (`tag`: the note's 0, the chat's 1) in the app's settings; what
    /// is shown takes it at once.
    fn write(&self, tag: isize, change: impl FnOnce(&mut TextLook)) {
        let app_data = knotes_session::app_data_dir();
        let mut settings = AppSettings::load(&app_data);
        change(if tag == 0 { &mut settings.note_text } else { &mut settings.chat_text });
        if let Err(e) = settings.save(&app_data) {
            eprintln!("knotes: cannot save settings: {e}");
        }
        text_look::set(settings.note_text, settings.chat_text);
        if tag == 0 { crate::live_preview::text_look_changed() } else { crate::chat_panel::text_look_changed() }
        self.refresh();
    }

    /// The tab for the probe: each text's font, size and line height as the controls show them.
    pub fn probe_line(&self) -> String {
        let rows = self.ivars().rows.borrow();
        let row = |row: &Row| format!("{} {} {}", row.font.titleOfSelectedItem().map(|t| t.to_string()).unwrap_or_default(), row.size.stringValue(), row.line.titleOfSelectedItem().map(|t| t.to_string()).unwrap_or_default());
        rows.iter().map(row).collect::<Vec<_>>().join(" | ")
    }

    /// `note size 18`, `chat font Georgia` or `note line 150`: set through the controls, as the
    /// user sets it (probe mode).
    pub fn probe_set(&self, spec: &str) {
        let mut words = spec.splitn(3, ' ');
        let (Some(whose), Some(what), Some(value)) = (words.next(), words.next(), words.next()) else { return };
        let rows = self.ivars().rows.borrow();
        let Some(row) = rows.get(usize::from(whose == "chat")) else { return };
        let target: &AnyObject = self.as_ref();
        // SAFETY: each control's own action, sent to the tab as the control sends it.
        unsafe {
            match what {
                "font" => {
                    row.font.selectItemWithTitle(&NSString::from_str(value));
                    let _: () = msg_send![target, fontChosen: &*row.font];
                }
                "size" => {
                    row.size.setStringValue(&NSString::from_str(value));
                    let _: () = msg_send![target, sizeTyped: &*row.size];
                }
                _ => {
                    row.line.selectItemWithTag(value.parse().unwrap_or(100));
                    let _: () = msg_send![target, lineChosen: &*row.line];
                }
            }
        }
    }
}

/// The note's look and the chat's, each with the size it has when none is set.
fn looks() -> [(TextLook, f64); 2] {
    [(text_look::note(), text_look::NOTE_SIZE), (text_look::chat(), text_look::chat_size())]
}

/// A line height in the font's own heights, as its menu names it: `1,5` for 150 percent.
fn line_title(percent: u32) -> String {
    format!("{},{}", percent / 100, percent % 100 / 10)
}
