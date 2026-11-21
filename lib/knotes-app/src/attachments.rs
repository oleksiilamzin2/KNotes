use knotes_core::{Attachment, MissingAttachment};

/// What the attachments' window lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttachmentFilter {
    /// Every attachment.
    #[default]
    All,
    /// Those no link points at.
    Unlinked,
    /// Links to images and PDFs that are not in the vault.
    Missing,
}

/// A row of the attachments' window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentItem {
    File(Attachment),
    Missing(MissingAttachment),
}

/// The rows `filter` lists of the vault's `files` and `missing` links, in their order.
pub fn attachment_items(files: &[Attachment], missing: &[MissingAttachment], filter: AttachmentFilter) -> Vec<AttachmentItem> {
    match filter {
        AttachmentFilter::All => files.iter().cloned().map(AttachmentItem::File).collect(),
        AttachmentFilter::Unlinked => files.iter().filter(|f| f.links == 0).cloned().map(AttachmentItem::File).collect(),
        AttachmentFilter::Missing => missing.iter().cloned().map(AttachmentItem::Missing).collect(),
    }
}
