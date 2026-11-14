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
            description: "Search the text of the vault's notes and PDFs. Gives fragments (blocks) holding every word, best first (`match: block`); then, while too few, looser finds: documents holding every word across their blocks (`note`), documents whose title or aliases hold them (`title`), the same by the words' stems (`stem`), documents linked with one a word names (`linked`, with the document it was found `via`), and for three words or more, documents holding all but one (`partial`, with the word `missing`: maybe not what was meant). Question words (when, what, коли, що…) are left out. Read a loose find's document for the rest. Each comes with its document, section heading and block number to read on from. When more follow, `next_offset` says where: pass it as `offset` for the next page (300 deep at most; past that, narrow the query).",
            parameters: json!({"type": "object", "properties": {
                "query": {"type": "string", "description": "Words to find; each may be the start of a word."},
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
            description: "Read a note's text by blocks (paragraphs and sections), each with the heading it is under. Start at a block search gave, or at 0.",
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
            name: "list_notes",
            description: "List notes and PDFs, in a folder (with its subfolders), with a tag, or changed between two days, the most recently changed first or by name; each with when it last changed (UTC). Says how many there are in all (`total`) and, when more follow, where the next page starts (`next_offset`).",
            parameters: json!({"type": "object", "properties": {
                "folder": {"type": "string", "description": "A folder's path in the vault."},
                "tag": {"type": "string", "description": "A tag without #, e.g. \"work\" or \"area/sub\"."},
                "changed_after": {"type": "string", "description": "Only those changed on this day (YYYY-MM-DD, UTC) or later."},
                "changed_before": {"type": "string", "description": "Only those changed on this day (YYYY-MM-DD, UTC) or earlier."},
                "sort": {"type": "string", "enum": ["modified", "name"]},
                "limit": {"type": "integer", "description": "30 by default, 100 at most."},
                "offset": {"type": "integer", "description": "Where the page starts: a previous call's `next_offset`, 0 by default."}
            }, "additionalProperties": false}),
        },
        ToolSpec {
            name: "list_tags",
            description: "The vault's tags with how many notes have each, the most used first.",
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
    ]
}
