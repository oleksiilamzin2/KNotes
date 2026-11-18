use knotes_app::{Slot, TAB_GAP, cross_of, cross_size, drag_tab, slots_dragged, tab_at, tab_slots};

const HEIGHT: f64 = 36.0;

#[test]
fn the_tabs_fill_the_strip_all_as_wide_and_a_point_finds_its_tab() {
    assert!(tab_slots(0, 600.0).is_empty());
    assert_eq!(tab_slots(1, 600.0), [Slot { x: 0.0, width: 600.0 }]);
    let slots = tab_slots(3, 608.0);
    assert_eq!(slots, [Slot { x: 0.0, width: 200.0 }, Slot { x: 204.0, width: 200.0 }, Slot { x: 408.0, width: 200.0 }]);
    assert_eq!(slots[2].x + slots[2].width, 608.0, "they end with the strip");
    // More tabs than points: each keeps a point of width.
    assert!(tab_slots(400, 300.0).iter().all(|slot| slot.width == 1.0));
    // A point on a tab's body; in the gap between two, and above or below the strip, none.
    assert_eq!(tab_at(&slots, HEIGHT, 100.0, 18.0), Some((0, false)));
    assert_eq!(tab_at(&slots, HEIGHT, 204.0, 18.0), Some((1, false)));
    assert_eq!(tab_at(&slots, HEIGHT, 607.9, 36.0), Some((2, false)));
    assert_eq!(tab_at(&slots, HEIGHT, 202.0, 18.0), None, "the gap");
    assert_eq!((tab_at(&slots, HEIGHT, 100.0, -1.0), tab_at(&slots, HEIGHT, 100.0, 36.5), tab_at(&slots, HEIGHT, 700.0, 18.0)), (None, None, None));
}

#[test]
fn a_cross_is_clicked_where_it_is_drawn() {
    // A tab with room for its name: the cross at its left, in the strip's middle.
    let slots = tab_slots(3, 608.0);
    for (tab, slot) in slots.iter().enumerate() {
        let (left, top) = cross_of(*slot, HEIGHT);
        assert_eq!((left, top), (slot.x + 9.0, (HEIGHT - cross_size()) / 2.0));
        let middle = (left + cross_size() / 2.0, top + cross_size() / 2.0);
        assert_eq!(tab_at(&slots, HEIGHT, middle.0, middle.1), Some((tab, true)), "the middle of the cross drawn is on it");
        assert_eq!(tab_at(&slots, HEIGHT, left + cross_size() + 4.0, middle.1), Some((tab, true)), "a few points off still");
        assert_eq!(tab_at(&slots, HEIGHT, left + cross_size() + 6.0, middle.1), Some((tab, false)), "further, the tab's body");
    }
    // Too narrow for a name: the cross in the tab's middle, where its symbol was (it was drawn
    // there and clicked at the left).
    let narrow = tab_slots(12, 12.0 * 40.0 + 11.0 * TAB_GAP);
    let (left, _) = cross_of(narrow[5], HEIGHT);
    assert_eq!(left + cross_size() / 2.0, narrow[5].x + narrow[5].width / 2.0);
    assert_eq!(tab_at(&narrow, HEIGHT, narrow[5].x + 20.0, 18.0), Some((5, true)));
}

#[test]
fn a_tab_dragged_follows_the_pointer_and_the_others_make_room() {
    let slots = tab_slots(3, 608.0);
    // Pressed 50 points into the first tab: a slip of a few points is a click still.
    assert_eq!(drag_tab(&slots, 0, 50.0, 53.0, 608.0, false), None);
    assert_eq!(drag_tab(&slots, 0, 50.0, 56.0, 608.0, false), Some((6.0, 0)));
    assert_eq!(drag_tab(&slots, 0, 50.0, 53.0, 608.0, true), Some((3.0, 0)), "dragged already: it stays so near its place");
    // Its middle over the second tab's place, then the third's; never out of the strip.
    assert_eq!(drag_tab(&slots, 0, 50.0, 160.0, 608.0, true), Some((110.0, 1)));
    assert_eq!(drag_tab(&slots, 0, 50.0, 400.0, 608.0, true), Some((350.0, 2)));
    assert_eq!(drag_tab(&slots, 0, 50.0, 2_000.0, 608.0, true), Some((408.0, 2)));
    assert_eq!(drag_tab(&slots, 2, 50.0, -500.0, 608.0, true), Some((0.0, 0)));
    assert_eq!(drag_tab(&slots, 7, 50.0, 100.0, 608.0, true), None, "no such tab");
    // The first over the third's place: the second and third stand one place to the left.
    let placed = slots_dragged(&slots, 0, 350.0, 2);
    assert_eq!(placed, [Slot { x: 350.0, width: 200.0 }, slots[0], slots[1]]);
    // The third over the first's: the others one place to the right.
    assert_eq!(slots_dragged(&slots, 2, 10.0, 0), [slots[1], slots[2], Slot { x: 10.0, width: 200.0 }]);
    assert_eq!(slots_dragged(&slots, 1, 204.0, 1), slots, "over its own place: as they were");
    assert_eq!(slots_dragged(&slots, 9, 0.0, 0), slots);
}
