//! Where the tabs lie along their strip (ADR 0024), in points from the strip's left: each tab's
//! slot, its cross, the tab under a point, and the slots while one is dragged. The strip draws
//! and clicks by these, so a cross is clicked where it is drawn.

/// The gap between two tabs.
pub const TAB_GAP: f64 = 4.0;
/// A tab shows its name while it is at least this wide; narrower, its symbol alone.
pub const TAB_NAMED: f64 = 56.0;
/// A tab is never narrower than this (the owner's measure, from a tab as he wants the
/// narrowest): room for its symbol and a short name. More tabs than fit so are longer than the
/// strip, which shows a part of them (`tab_scroll`).
pub const TAB_MIN: f64 = 150.0;
/// The cross's size, and how near a point must be to count as on it.
const CROSS: f64 = 15.0;
const NEAR: f64 = 5.0;
/// A tab pressed is dragged once the pointer has moved this far: a click that slips is no drag.
const SLIP: f64 = 5.0;

/// A tab's place along the strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub x: f64,
    pub width: f64,
}

/// The slots of `count` tabs on a strip `strip` wide: all as wide, together filling it (as
/// Safari's do), none narrower than `TAB_MIN`: too many to fit so, they run on past the
/// strip's end.
pub fn tab_slots(count: usize, strip: f64) -> Vec<Slot> {
    if count == 0 {
        return Vec::new();
    }
    let width = ((strip - TAB_GAP * (count - 1) as f64) / count as f64).max(TAB_MIN);
    (0..count).map(|i| Slot { x: i as f64 * (width + TAB_GAP), width }).collect()
}

/// Where a tab in `slot` of a strip `height` tall shows its cross (its left and its top; it is
/// square, `cross_size` a side): at the tab's left, clear of the round end of the strip's first
/// tab; in its middle when it has no room for a name.
pub fn cross_of(slot: Slot, height: f64) -> (f64, f64) {
    let left = if slot.width < TAB_NAMED { slot.x + (slot.width - CROSS) / 2.0 } else { slot.x + 9.0 };
    (left, height / 2.0 - CROSS / 2.0)
}

pub fn cross_size() -> f64 {
    CROSS
}

/// The tab at the point (`x`, `y`) of a strip `height` tall, and whether the point is on its
/// cross (or within a few points of it).
pub fn tab_at(slots: &[Slot], height: f64, x: f64, y: f64) -> Option<(usize, bool)> {
    if !(0.0..=height).contains(&y) {
        return None;
    }
    let tab = slots.iter().position(|slot| x >= slot.x && x < slot.x + slot.width)?;
    let (left, top) = cross_of(slots[tab], height);
    let on_cross = x >= left - NEAR && x <= left + CROSS + NEAR && y >= top - NEAR && y <= top + CROSS + NEAR;
    Some((tab, on_cross))
}

/// The tab `tab`, pressed `grab` points into it, with the pointer now at `x` on a strip
/// `strip` wide: where its left edge is, and the place among the tabs it would take (the one
/// its middle is over). None while it has not moved far enough to be dragged (`dragging`: it
/// is dragged already, and stays so however near it comes back).
pub fn drag_tab(slots: &[Slot], tab: usize, grab: f64, x: f64, strip: f64, dragging: bool) -> Option<(f64, usize)> {
    let slot = slots.get(tab)?;
    let left = (x - grab).clamp(0.0, (strip - slot.width).max(0.0));
    if !dragging && (left - slot.x).abs() < SLIP {
        return None;
    }
    let over = (((left + slot.width / 2.0) / (slot.width + TAB_GAP)) as usize).min(slots.len() - 1);
    Some((left, over))
}

/// The slots while `tab` is dragged, its left edge at `left`, over the place `over`: the others
/// stand as if it were there already, and it is where the pointer has it.
pub fn slots_dragged(slots: &[Slot], tab: usize, left: f64, over: usize) -> Vec<Slot> {
    let mut placed = slots.to_vec();
    if tab >= slots.len() {
        return placed;
    }
    let mut order: Vec<usize> = (0..slots.len()).filter(|i| *i != tab).collect();
    order.insert(over.min(order.len()), tab);
    for (at, index) in order.into_iter().enumerate() {
        placed[index] = slots[at];
    }
    placed[tab].x = left;
    placed
}

/// How far the tabs can be scrolled along a strip `strip` wide: how much longer they are.
fn scroll_end(slots: &[Slot], strip: f64) -> f64 {
    (slots.last().map_or(0.0, |last| last.x + last.width) - strip).max(0.0)
}

/// How far the tabs are scrolled (the point of them at the strip's left), from `scroll` where
/// they were: never past either end, and with the tab `shown` wholly in view, moved no
/// further than that takes.
pub fn tab_scroll(slots: &[Slot], strip: f64, scroll: f64, shown: Option<usize>) -> f64 {
    let mut scroll = scroll;
    if let Some(slot) = shown.and_then(|shown| slots.get(shown)) {
        scroll = scroll.min(slot.x).max(slot.x + slot.width - strip);
    }
    scroll.clamp(0.0, scroll_end(slots, strip))
}

