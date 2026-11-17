//! The path bar at the bottom of the note column, as the Finder's (owner's request, 2026-11-17):
//! where the open document is, from the vault down — the vault, its folders, the document —
//! each with a symbol in the accent colour, as the sidebar draws its own (owner's choice: the
//! Finder's coloured icons stood out). A click on the vault shows all notes, on a folder that
//! folder.

use std::cell::RefCell;
use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSControlSize, NSFont, NSImage, NSImageSymbolConfiguration, NSLayoutConstraint, NSPathControl, NSPathControlItem, NSPathStyle, NSView,
};
use objc2::Message;
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

/// `image` in the middle of a square, its proportions kept: the path control draws an item's
/// image in a square, and a folder (wider than tall) came out squeezed.
fn square(image: &NSImage) -> Retained<NSImage> {
    const SIDE: f64 = 16.0;
    let image = image.retain();
    let size = image.size();
    let handler = block2::RcBlock::new(move |rect: NSRect| -> objc2::runtime::Bool {
        let scale = (rect.size.width / size.width).min(rect.size.height / size.height);
        let (width, height) = (size.width * scale, size.height * scale);
        let at = NSPoint::new(rect.origin.x + (rect.size.width - width) / 2.0, rect.origin.y + (rect.size.height - height) / 2.0);
        image.drawInRect(NSRect::new(at, NSSize::new(width, height)));
        objc2::runtime::Bool::YES
    });
    NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(SIDE, SIDE), false, &handler)
}

/// The whole vault's symbol: a book (here and in the spaces' switcher).
pub const VAULT_SYMBOL: &str = "book.closed";

/// The bar's height.
pub const HEIGHT: f64 = 24.0;

/// Called with the folder a click chose (`""`: the vault).
type OnFolder = Box<dyn Fn(String)>;

#[derive(Default)]
pub struct TargetIvars {
    /// The folder of each item shown, in order; none for the document's.
    folders: RefCell<Vec<Option<String>>>,
    on_folder: RefCell<Option<OnFolder>>,
}

define_class!(
    /// The path control's target.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesPathBarTarget"]
    #[ivars = TargetIvars]
    struct PathTarget;

    unsafe impl NSObjectProtocol for PathTarget {}

    impl PathTarget {
        #[unsafe(method(pathClicked:))]
        fn path_clicked(&self, sender: &NSPathControl) {
            let Some(clicked) = sender.clickedPathItem() else { return };
            let index = sender.pathItems().iter().position(|item| std::ptr::eq(&*item, &*clicked));
            if let Some(index) = index {
                self.choose(index);
            }
        }
    }
);

impl PathTarget {
    fn choose(&self, index: usize) {
        let folder = self.ivars().folders.borrow().get(index).cloned().flatten();
        if let (Some(folder), Some(callback)) = (folder, self.ivars().on_folder.borrow().as_ref()) {
            callback(folder);
        }
    }
}

pub struct PathBar {
    /// The bar: a line above, the path control in it.
    pub view: Retained<NSView>,
    control: Retained<NSPathControl>,
    target: Retained<PathTarget>,
}

impl PathBar {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let plate = NSBox::new(mtm);
        plate.setBoxType(NSBoxType::Custom);
        plate.setBorderWidth(0.0);
        plate.setCornerRadius(0.0);
        plate.setFillColor(&NSColor::windowBackgroundColor());
        plate.setContentViewMargins(NSSize::new(0.0, 0.0));
        let view: Retained<NSView> = Retained::into_super(plate);
        view.setTranslatesAutoresizingMaskIntoConstraints(false);
        let line = NSBox::new(mtm);
        line.setBoxType(NSBoxType::Separator);
        line.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(&line);
        let control = NSPathControl::new(mtm);
        control.setPathStyle(NSPathStyle::Standard);
        control.setControlSize(NSControlSize::Small);
        control.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        control.setBackgroundColor(Some(&NSColor::clearColor()));
        control.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(&control);
        let target: Retained<PathTarget> = unsafe { msg_send![super(PathTarget::alloc(mtm).set_ivars(TargetIvars::default())), init] };
        let receiver: &AnyObject = target.as_ref();
        // SAFETY: the bar keeps its target as long as the control.
        unsafe {
            control.setTarget(Some(receiver));
            control.setAction(Some(sel!(pathClicked:)));
        }
        let constraints = [
            view.heightAnchor().constraintEqualToConstant(HEIGHT),
            line.topAnchor().constraintEqualToAnchor(&view.topAnchor()),
            line.leadingAnchor().constraintEqualToAnchor(&view.leadingAnchor()),
            line.trailingAnchor().constraintEqualToAnchor(&view.trailingAnchor()),
            control.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 8.0),
            control.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -8.0),
            control.centerYAnchor().constraintEqualToAnchor_constant(&view.centerYAnchor(), 0.5),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
        view.setHidden(true);
        Self { view, control, target }
    }

    /// Called with the folder a click chose (`""`: the vault).
    pub fn on_folder(&self, callback: impl Fn(String) + 'static) {
        *self.target.ivars().on_folder.borrow_mut() = Some(Box::new(callback));
    }

    /// Shows where `document` (a vault path) is in the vault at `vault`, named `name` (a note's
    /// title, a PDF's name); hidden with none.
    pub fn show(&self, vault: &Path, document: Option<(&str, &str)>) {
        let Some((path, name)) = document else {
            self.view.setHidden(true);
            return;
        };
        let accent = NSImageSymbolConfiguration::configurationWithPaletteColors(&NSArray::from_retained_slice(&[NSColor::controlAccentColor()]));
        let item = |title: &str, symbol: &str| {
            let item = NSPathControlItem::new();
            item.setTitle(&NSString::from_str(title));
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), Some(&NSString::from_str(title)));
            item.setImage(image.and_then(|i| i.imageWithSymbolConfiguration(&accent)).map(|i| square(&i)).as_deref());
            item
        };
        let vault_name = vault.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        // The vault as the whole vault is shown in the spaces' switcher: its symbol alone, no
        // name (owner's choice), the name told to VoiceOver.
        let whole = item(&vault_name, VAULT_SYMBOL);
        whole.setTitle(&NSString::new());
        let mut items = vec![whole];
        let mut folders = vec![Some(String::new())];
        let mut at = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            at = if at.is_empty() { (*part).to_owned() } else { format!("{at}/{part}") };
            items.push(item(part, "folder"));
            folders.push(Some(at.clone()));
        }
        let pdf = path.to_lowercase().ends_with(".pdf");
        items.push(item(name, if pdf { "doc.richtext" } else { "doc.text" }));
        folders.push(None);
        self.control.setPathItems(&NSArray::from_retained_slice(&items));
        *self.target.ivars().folders.borrow_mut() = folders;
        self.view.setHidden(false);
    }

    /// The path shown, «📕 › Folder › note», or `hidden` (probe mode).
    pub fn describe(&self) -> String {
        if self.view.isHidden() {
            return "hidden".to_owned();
        }
        // The vault's item has no title: "📕" for it.
        let titles: Vec<String> = self.control.pathItems().iter().enumerate().map(|(i, item)| if i == 0 { "📕".to_owned() } else { item.title().to_string() }).collect();
        titles.join(" › ")
    }

    /// A click on item `index` (probe mode).
    pub fn click(&self, index: usize) {
        self.target.choose(index);
    }
}
