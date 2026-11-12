# 0016. Toolbar glass without the separator line

Status: accepted, 2026-11-12

## Context

On macOS 26 and later a scroll view under the toolbar gets scroll pockets: the glass that
fades content scrolling beneath the toolbar (and beneath the list's search row). The
pockets of the list and note columns also drew a line along the toolbar's bottom, which the
owner asked to remove.

- `titlebarSeparatorStyle = .none`, on the window or on the split view items, does not
  reach the pockets (checked on screen captures).
- A transparent titlebar removed the line, but also the glass: list text showed through
  the list title and the search row.
- The public scroll edge API (`NSScrollEdgeEffectStyle`, macOS 26.1) only styles accessory
  view controllers; there is no public switch for the pocket line.

## Decision

- Keep the titlebar opaque, so the glass works.
- The list and note scroll views are `QuietScrollView`, which sets the pockets' private
  `allowsSeparator` to NO when a pocket is added and on every layout, only if the pocket
  answers the selector.

## Consequences

- Glass under the toolbar and the search row, no line (screen captures: scrolled list with
  the search row open, plain window, collapsed sidebar).
- A private selector: if a future macOS drops it, the line comes back; nothing breaks. The
  probe reports the pockets' separators and a window test checks them.
