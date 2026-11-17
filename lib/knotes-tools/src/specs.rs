use serde_json::{Value, json};

/// A tool as the model (or an MCP client) sees it: its name, what it does and its arguments'
/// JSON schema.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

/// The vault's tools, all reading only.
pub fn specs() -> Vec<ToolSpec> {
    let path = json!({"type": "string", "description": "The document's path in the vault, as other tools give it (e.g. \"Projects/Radar.md\" or \"docs/paper.pdf\")."});
    vec![
        ToolSpec {
            name: "search",
            description: "Search the text of the vault's notes and PDFs. Gives fragments (blocks) holding every word, best first (`match: block`); then, while too few, looser finds: documents holding every word across their blocks (`note`), documents whose title or aliases hold them (`title`), the same by the words' stems (`stem`), documents linked with one a word names (`linked`, with the document it was found `via`), and for three words or more, documents holding all but one (`partial`, with the word `missing`: maybe not what was meant). Blocks near the query in meaning, whatever their words or language, come among them when the app can search by meaning (`meaning`: read one before relying on it), those merely near the subject left out; a block found by meaning says how far it stands out from all the vault's blocks (`closeness`, in standard deviations: 3 and above is mostly what was asked, below 2.5 often only near the subject). Question words (when, what, коли, що…) are left out. Read a loose find's document for the rest. Each comes with its document, when that last changed (`modified`, UTC), section heading and block number to read on from. When no document holds the words nor their stems, `words_found` is false (any finds are by meaning only). A find in one of the vault's spaces (named sets of folders) names it (`space`); `space` searches one space only. When more follow, `next_offset` says where: pass it as `offset` for the next page (300 deep at most; past that, narrow the query).",
            parameters: json!({"type": "object", "properties": {
                "query": {"type": "string", "description": "Words to find; each may be the start of a word."},
                "space": {"type": "string", "description": "A space's name: search only its documents."},
                "limit": {"type": "integer", "description": "How many fragments, 10 by default, 30 at most."},
                "offset": {"type": "integer", "description": "Where the page starts: a previous call's `next_offset`, 0 by default."}
            }, "required": ["query"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "find_by_title",
            description: "Find notes and PDFs by their title, alias or path, fuzzily (a few letters are enough). When more follow, `next_offset` says where the next page starts.",
            parameters: json!({"type": "object", "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "description": "10 by default, 30 at most."},
                "offset": {"type": "integer", "description": "Where the page starts: a previous call's `next_offset`, 0 by default."}
            }, "required": ["query"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "read_note",
            description: "Read a note's text by blocks (paragraphs and sections), each with the heading it is under, and when the note last changed (`modified`, UTC). Start at a block search gave, or at 0.",
            parameters: json!({"type": "object", "properties": {
                "path": path,
                "from_block": {"type": "integer", "description": "The first block, 0 by default."},
                "max_blocks": {"type": "integer", "description": "20 by default, 60 at most."}
            }, "required": ["path"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "read_pdf",
            description: "Read a PDF's text by pages, counted from 1.",
            parameters: json!({"type": "object", "properties": {
                "path": path,
                "from_page": {"type": "integer", "description": "The first page, 1 by default."},
                "pages": {"type": "integer", "description": "How many pages, 5 by default, 10 at most."}
            }, "required": ["path"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "outline",
            description: "A document's headings (a note's) or bookmarks (a PDF's, with their pages), in order, with their levels.",
            parameters: json!({"type": "object", "properties": {"path": path}, "required": ["path"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "links",
            description: "The notes linking to a document (backlinks, with how many links each) and the document's own links (with the files they lead to; none when the note is not there yet).",
            parameters: json!({"type": "object", "properties": {"path": path}, "required": ["path"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "related",
            description: "A document with the notes it links to (each with how it starts) and the notes linking to it (each with the line that links), in one call: what is connected with a person, a project, a thing. When the app searches by meaning, also the notes near it in meaning that no link shows (`near_in_meaning`, each with its nearest fragment and `closeness`, as search gives it): about the same thing though unlinked.",
            parameters: json!({"type": "object", "properties": {"path": path}, "required": ["path"], "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_notes",
            description: "List notes and PDFs, in a folder (with its subfolders), with a tag, or changed between two days, the most recently changed first or by name; each with when it last changed (UTC). Says how many there are in all (`total`) and, when more follow, where the next page starts (`next_offset`).",
            parameters: json!({"type": "object", "properties": {
                "folder": {"type": "string", "description": "A folder's path in the vault."},
                "tag": {"type": "string", "description": "A tag without #, e.g. \"work\" or \"area/sub\"."},
                "space": {"type": "string", "description": "A space's name (a named set of folders): only its documents."},
                "changed_after": {"type": "string", "description": "Only those changed on this day (YYYY-MM-DD, UTC) or later."},
                "changed_before": {"type": "string", "description": "Only those changed on this day (YYYY-MM-DD, UTC) or earlier."},
                "sort": {"type": "string", "enum": ["modified", "name"]},
                "limit": {"type": "integer", "description": "30 by default, 100 at most."},
                "offset": {"type": "integer", "description": "Where the page starts: a previous call's `next_offset`, 0 by default."}
            }, "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_tasks",
            description: "The vault's task items (`- [ ]` / `- [x]` lines), open ones by default, by note (the most recently changed first), each with its line, words and section heading; in a folder if given. For to-dos and what is left to do; only checkbox items: plans and appointments written as plain text are not here. Says how many notes have them (`total`) and where the next page starts (`next_offset`).",
            parameters: json!({"type": "object", "properties": {
                "state": {"type": "string", "enum": ["open", "done", "all"], "description": "Open by default."},
                "folder": {"type": "string", "description": "A folder's path in the vault."},
                "limit": {"type": "integer", "description": "Notes per page, 30 by default, 100 at most."},
                "offset": {"type": "integer", "description": "Where the page starts: a previous call's `next_offset`, 0 by default."}
            }, "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_tags",
            description: "The vault's tags with how many notes have each, the most used first.",
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_spaces",
            description: "The vault's spaces: named sets of its folders the user works in (they may call them spaces, простори, профілі, воркспейси or сфери), each with the folders whose rules put them in it and how many notes it has, the folders in every space (`shared`, shown in each), and the space the user's window is in (`current`, none for the whole vault). search and list_notes take a space's name as `space`.",
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_decks",
            description: "The decks of the user's flashcards (spaced repetition, FSRS; they may call them картки, колоди, повтори). A card is written in a note's text: `#@deck/sub question::answer`, the question and the answer apart on a `?` line, words to recall as `==words==`, or several paragraphs as `#@deck{ question ? answer }`; `^c-a1b2c3` at its end is its id. Decks nest by `/`. Each deck comes with its cards and those of the decks under it: how many (`cards`), how many reviewed before and due again now (`due`), how many never reviewed (`new`); then the same for the whole vault. The app asks the `due` ones, and of the `new` only as many a day as the deck is set to (20 unless the user set another).",
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_cards",
            description: "The user's flashcards, of a deck (with the decks under it), of a note, or all: each with its deck, its note (`path`), its `question` and `answer`, and where it stands (`state`): `new` (never reviewed), `due` (to review now) or `later`. A reviewed card says when it is due (`due`, UTC) and was last reviewed (`last_reviewed`), how many times it was reviewed (`reviews`) and how many of them forgotten (`forgotten`), and FSRS's memory of it: `stability_days` (after that many days the user recalls it 9 times of 10) and `difficulty` (1 easy to 10 hard). Says how many there are in all (`total`) and, when more follow, where the next page starts (`next_offset`). Reading only: cards are reviewed in the app.",
            parameters: json!({"type": "object", "properties": {
                "deck": {"type": "string", "description": "A deck's path as list_decks gives it, e.g. \"мови/англійська\": its cards and those of the decks under it."},
                "path": {"type": "string", "description": "A note's path in the vault: only its cards."},
                "state": {"type": "string", "enum": ["all", "to_review", "due", "new"], "description": "All by default; `to_review` is due and new together."},
                "sort": {"type": "string", "enum": ["review", "forgotten", "note"], "description": "`review` by default: the due (the longest due first), the new, then the rest as they come due; `forgotten`: the most often forgotten first; `note`: by note, in its text's order."},
                "limit": {"type": "integer", "description": "30 by default, 100 at most."},
                "offset": {"type": "integer", "description": "Where the page starts: a previous call's `next_offset`, 0 by default."}
            }, "additionalProperties": false}),
        },
    ]
}
