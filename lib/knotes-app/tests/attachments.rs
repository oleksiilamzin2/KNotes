use knotes_app::{AttachmentFilter, AttachmentItem, attachment_items};
use knotes_core::{Attachment, MissingAttachment};

#[test]
fn a_filter_lists_every_file_those_without_links_or_the_links_without_files() {
    let file = |path: &str, links| Attachment { path: path.to_owned(), size: 1, links };
    let files = [file("a.png", 2), file("b.pdf", 0)];
    let missing = [MissingAttachment { target: "gone.png".to_owned(), source: "n.md".to_owned() }];
    let list = |filter| attachment_items(&files, &missing, filter);
    assert_eq!(list(AttachmentFilter::All), [AttachmentItem::File(files[0].clone()), AttachmentItem::File(files[1].clone())]);
    assert_eq!(list(AttachmentFilter::Unlinked), [AttachmentItem::File(files[1].clone())]);
    assert_eq!(list(AttachmentFilter::Missing), [AttachmentItem::Missing(missing[0].clone())]);
}
