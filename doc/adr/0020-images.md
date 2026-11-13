# 0020. Images in notes

Status: accepted, 2026-11-13

## Context

Notes embed images as Obsidian does: `![[picture.png]]`, `![[picture.png|300]]` or
`|300x200`, and `![alt](path/picture.png)`. The live preview showed them as links. The
owner answered a question round (2026-11-13).

## Decision

- An image embed alone in its paragraph shows the image, centred (owner's choice), at its
  natural size but no wider than the note column, or at the size written after `|`. Away from
  the cursor its markup is hidden; a click puts the cursor in it, as with display formulas
  (ADR 0017): the markup shows and the image stays below it.
- Dragging the image's right edge resizes it and writes the width into the embed (`|300`).
- A double click opens the image in Quick Look.
- An image pasted (Cmd+V) or dropped from Finder into a note is copied to
  `.knotes/attachments` at the vault root as `Pasted image 20261113114500.png` (Obsidian's
  naming) and embedded where the cursor is. Obsidian ignores folders starting with a dot, so
  it does not show these images (owner's choice: the vault stays clean).
- `.knotes/` at the vault root becomes Knotes's folder in the vault (as `.obsidian/` is
  Obsidian's). The per-folder rules file of ADR 0010 is renamed from `.knotes` to
  `.knotes.json`; an old `.knotes` file is still read and is replaced on the next write.
- The scan and the watcher take in `.knotes/attachments`; the sidebar shows no hidden folder.
- No images from the web (`![](https://…)` stays a link): no network requests from notes.

## Consequences

- Inline images inside a line of text stay links for now.
- Images pasted in Knotes are broken links in Obsidian.

## Built (2026-11-13)

- An image's line is a display formula's (ADR 0017): its source hidden, the line centred
  and as tall as the image plus gaps, the width kept by kerning on the first character; the
  paragraph's layout fragment draws the image. Images are kept a point narrower than the
  column: beside the kept width, the hidden source's tiny glyphs wrapped onto a second line
  as tall as the image.
- Sizes come from the file's header (ImageIO, 0.1 ms); the image is decoded for the size it
  shows at (ImageIO thumbnails, EXIF rotation applied; SVG through `NSImage`) the first time
  its fragment draws: TextKit makes fragments for paragraphs out of view too. A note with ten
  12-megapixel photos (M5 Pro, release): 173 MB with each photo decoded whole, 67 MB this way
  (54 MB without the photos); a decode takes 13 ms on the main thread as a photo comes into
  view.
- Paste reads image files first (copied in Finder or dropped), then image data alone (PNG,
  or TIFF converted to PNG); a pasteboard with text keeps the text view's paste. An image
  file already in the vault is embedded where it is, not copied.
- A double click opens Quick Look (the note's text view is the panel's controller); the first
  click moves the image below its source, so the second one previews the image the first was
  on.
- Not checked by tests (probe mode cannot drive them): the drag of the right edge, the Quick
  Look panel itself, a real drop from Finder.
