# 0030. Spaces: named, coloured sets of folders a window works in

Status: accepted, 2026-11-15.

## Context

A vault often holds several areas of a life (work, home, a project) that the user wants apart
while working in one, without giving up links between them as separate vaults would. The owner
asked for "spaces": a folder may belong to one; search and what the user does go on in the
current space first, then in the whole vault; each space has its colour, and files show the mark
of their space. Obsidian's "Workspaces" are saved window layouts, another thing; the app calls
these «Простори».

The owner chose (2026-11-15): a folder belongs to one space at most (its subfolders with it);
what search finds outside the space comes as a separate group, «Решта бази»; the mark shows on
files of other spaces only (on all in «Уся база»). Notes in no space were to show only in «Уся
база»; trying it, the owner asked for the rest of the vault below the space's, in the sidebar
and in the lists alike.

## Decision

- **Where they are kept: in the vault**, so they travel with it (git, sync):
  - The spaces, in the root `.knotes.json`: `"spaces": [{"id": "work", "name": "Робота",
    "color": "blue"}, …]`, in the order shown. The root file is watched (a file under `.knotes/`
    is not) and already holds a vault-wide setting (`newNotes.folder`). Colours are names of
    macOS's system colours (blue, purple, pink, red, orange, yellow, green, teal, gray), right in
    light and dark; an unknown name shows as gray. A colour of one's own (at the owner's word,
    2026-11-15) is `#rrggbb`, chosen in macOS's colour panel from the settings' «Свій колір…»;
    it stays as chosen in dark appearance.
  - A folder's space, in its own `.knotes.json`: `"space": "work"`. Subfolders take it, the
    nearest folder that says wins (as list settings and the `ai` rule do), so a folder moved or
    renamed keeps it; `"space": null` takes a subfolder out. The vault's root is in no space; an
    id no space has counts as none. `"space": "*"` puts a folder in every space (at the owner's
    word, 2026-11-15, for folders such as contacts or an inbox needed in each, in place of a note
    in several spaces): each shows it among its own folders, none marks it, the tools count it in
    each and name it apart (`list_spaces`'s `shared`).
- **One reader**, crate `knotes-spaces` (serde_json, NFC paths): from the rules files it gives
  the spaces and each document's space. The app builds it from each snapshot's rules, the AI
  tools from the files as they read the `ai` rule; both compare paths composed (macOS writes some
  names decomposed).
- **The current space is the window's**, kept beside the index (like the tabs, ADR 0024), not in
  the vault: two windows, or two Macs, may be in different spaces.
- **What a space changes** (first step): the sidebar lists its folders and its tags (counted in
  it), then a «Решта бази» row with the rest of the vault's below it, each folder in one part
  only (one of a part inside the other's hangs from its nearest ancestor in its own part, or at
  the top named by its path), the tags with no note in the space there; that row, and its
  folders and tags, list the notes outside the space. Each space's tags and untagged notes are
  counted with the snapshot, off the main thread (a snapshot follows every save), and only
  when the tags, the untagged notes or the spaces' folders changed. Every other list (all notes, a folder, a
  tag, untagged) gives the space's notes, then the rest of the list's under a «Решта бази»
  header (no date headers there), and list search its finds the same way; a new note goes to
  the space's first folder unless a folder is selected. The chat is told the current space each turn (a developer
  message, as the open document is: the cached start of the conversation stays) and its search
  gives the space's documents first, each result naming its space (one message with the document
  open: a turn starts at the developer message before the question); `search` and `list_notes`
  take a `space` argument (a name or an id), which narrows every way search finds, by meaning
  too; the vault's description lists the spaces with their folders. Trying it, the chat answered
  "what spaces are there" with the top folders: its conversation had begun before the spaces
  were made, and the description is fixed at a conversation's start. So the spaces are told in
  the message before each question too (with the window's, or that it shows the whole vault), a
  `list_spaces` tool lists them (folders, notes, the window's), and the instructions say what
  spaces are and the owner's other words for them (простори, профілі, воркспейси, сфери). In the list, search by
  meaning runs apart in the space's notes, then in the whole list, so the rest of the vault can
  neither take the space's places nor raise its bar. The MCP server knows no current space: it searches the whole vault,
  names each result's space and takes the same argument.
- **The mark**: a dot of the space's colour beside a file of another space (in «Уся база» beside
  every file in one): in the list, the tabs, and the folder tree beside a folder whose own
  rules set another space.
- **Managed** in Settings, a «Простори» tab (add, rename, colour, remove), and from a folder's
  menu in the sidebar («Простір ▸»); a pop-up at the foot of the sidebar switches (the spaces
  with their dots, «Уся база»; the owner's choice over a toolbar menu), hidden while the vault
  has no space.

## Consequences

- A space is folders, not tags: one note cannot be in two spaces; a note moved to another folder
  changes space with it.
- Spaces are listed in the root `.knotes.json`, which also passes its `list.*` settings on to
  every folder; the two never mix (`spaces` is read only from the root).
- Later, if the first step holds: a space's own inbox, tabs and recent notes; the graph by
  space; marks on links in the editor and in the links panel.
- The settings' «Простори» tab edits the front window's vault, writing its root's rules file
  (rename keeps the id; a space removed leaves its folders' keys, which then count as none).
