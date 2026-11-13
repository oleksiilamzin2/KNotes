use knotes_editor::task_box_typed;

#[test]
fn a_space_after_an_empty_pair_of_brackets_in_a_list_item_makes_a_task_box() {
    assert_eq!(task_box_typed("- [] "), Some(2));
    assert_eq!(task_box_typed("\t* [] "), Some(3));
    assert_eq!(task_box_typed("1. [] "), Some(3));
}

#[test]
fn brackets_elsewhere_stay_as_typed() {
    assert_eq!(task_box_typed("- [ ] "), None);
    assert_eq!(task_box_typed("[] "), None);
    assert_eq!(task_box_typed("- text [] "), None);
    assert_eq!(task_box_typed("- []x"), None);
    assert_eq!(task_box_typed("-[] "), None);
}
