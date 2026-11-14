# Memory, personalization, citations and trust in AI assistants for notes (state as of November 2026)

Scope: (a) memory and personalization designs; (b) citation, grounding and trust UX, applied to Knotes (native macOS, Obsidian-compatible Markdown + PDFs, OpenAI GPT-5.x via Sign in with ChatGPT, store:false, local conversations per vault, read-only vault tools, wiki-link citations with `#Heading` anchors and PDF pages).

Source-quality notes for the report writer:
- OpenAI's help center and openai.com pages returned HTTP 403 to the fetcher. Every OpenAI memory claim below comes from search-engine summaries of those pages, not from reading them. Treat the wording as unverified and the dates as likely correct.
- Anthropic (platform.claude.com, support.claude.com), Google (support.google.com, blog.google), Notion (notion.com/help) and Letta (docs.letta.com) pages were fetched directly unless marked.
- "[older]" marks information from before 2026. "[3rd-party]" marks non-primary sources.

---

## 1. Memory designs: how the major assistants implement memory, and what fits a local-first notes app

### Takeaway
Every major assistant now uses two layers. The first is a small, user-visible, editable store of durable facts (ChatGPT saved memories, Claude memory topics, Notion "About page + Memories database", Letta memory files), which is mostly kept in context. The second is retrieval over past conversations or files on demand (ChatGPT chat history, Claude chat search shown as a visible tool call, Claude Project RAG). The file-based designs (Basic Memory, Claude memory tool, Letta MemFS on git) show that "memory = Markdown files the user owns" is now a mainstream pattern. That pattern maps directly onto a vault "memory note" and keeps Knotes' local-first and store:false stance.

### Cited Findings

**ChatGPT (OpenAI): read through search summaries, because the pages return 403**
- Memory works in two ways: "saved memories" the user asked it to remember, and "chat history", which are insights ChatGPT gathers from past chats. — [Memory in ChatGPT (help)](https://help.openai.com/en/articles/8590148-memory-in-chatgpt); [Memory and new controls for ChatGPT](https://openai.com/index/memory-and-new-controls-for-chatgpt/) [original launch Feb 2024, updated since; older]
- Saved memories are kept until the user deletes them. Details taken from past chats "can change over time as ChatGPT updates what's more helpful to remember." — [Memory in ChatGPT (help)](https://help.openai.com/en/articles/8590148-memory-in-chatgpt)
- Deleting a chat does not delete a saved memory created from it ("Deleting a chat alone does not necessarily delete a separate saved memory created from that chat"). Deleting a memory does not remove mentions of it from past conversations, and deletions can take time to propagate. — [Memory in ChatGPT (help)](https://help.openai.com/en/articles/8590148-memory-in-chatgpt)
- June 2026: memories are now "updated automatically", with ChatGPT tracking the details it judges most important. Users can switch back to the legacy manual system under Settings > Memory > Saved memories. — [ChatGPT release notes](https://help.openai.com/en/articles/6825453-chatgpt-release-notes)
- "Dreaming" is a background process that reviews past chats and updates memory. Secondary sources say it began rolling out on June 4, 2026, to Plus/Pro users in the US first. — [OpenAI: Dreaming: Better memory for a more helpful ChatGPT](https://openai.com/index/chatgpt-memory-dreaming/) (title confirmed in search results, page body not readable); rollout date from [3rd-party] [mem0 article](https://x.com/mem0ai/article/2071990201531118063) and [ai-toolbox](https://www.ai-toolbox.co/chatgpt-management-and-productivity/how-to-manage-chatgpt-memory-2026)
- May 2026: a "Sources" icon under a response shows which saved memories, past chats or custom instructions informed it, so users can correct or delete outdated items. There is also a "memory summary" view of what ChatGPT has picked up. — [ChatGPT release notes](https://help.openai.com/en/articles/6825453-chatgpt-release-notes); [Memory in ChatGPT (help)](https://help.openai.com/en/articles/8590148-memory-in-chatgpt)
- Temporary Chats neither use nor create memories. An August 27, 2026 update added "personalized temporary chats" that use memories without creating new ones. — [ChatGPT release notes](https://help.openai.com/en/articles/6825453-chatgpt-release-notes)
- Toggles: "Reference saved memories" and "Reference chat history" are independent switches under Settings → Personalization → Memory. — [3rd-party] [whizi.io](https://whizi.io/resources/how-does-chatgpt-memory-work/); [gptprompts.ai](https://gptprompts.ai/chatgpt-memory-guide)
- Injection mechanism is disputed. One explainer says a summary is pre-loaded into context rather than searched. Another says the model searches past chats. OpenAI describes memory as selective, not a full searchable archive. — [3rd-party] [cognee.ai](https://www.cognee.ai/chatgpt-memory); [memx.app](https://memx.app/blog/does-chatgpt-remember-between-chats/); conflict unresolved

**Claude (Anthropic consumer apps)**
- New experience (2026): "Claude saves memory as a set of individual topics as you chat, rather than summarizing conversations after they end." Topics cover role, projects, communication preferences, technical preferences and ongoing work. Claude saves on its own or when asked. — [Claude help: chat search and memory](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Legacy experience (still used by some Team/Enterprise orgs): a synthesized summary that "is updated every 24 hours and provides context for every new standalone conversation". This is always-injected memory. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- View, edit and delete: Settings > Your Claude > "Manage" lists topics, each readable, editable and deletable. Users can also say "remember / change / forget" in chat. "Pause" stops new saves. "Reset" permanently deletes all memory. Deleting a conversation does not remove memory entries derived from it. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Scoping: each Project has its own memory. Inside a project Claude uses project memory plus account memory, never other projects' memory. "Use account memory" can be turned off per project for full isolation. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Sensitive data: health, race, ethnicity, religion, politics and gender identity are not stored by default (opt-in "Include sensitive topics in memory"). Government IDs, criminal history, financial account numbers and immigration status are never saved, even on request. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Per-chat controls: Incognito chats are not saved to memory or history and are never searched later. Memory can be turned off for one chat before the first message. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Chat search is retrieval over past chats, shown as tool calls. Project chats are searched only within their project. Citations link back to the original chats. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md); [search summary of Claude help](https://support.claude.com/en/articles/11817273-how-does-claude-s-memory-work)
- Memory import from other assistants and memory export exist (marked experimental). Memory is on by default for Free/Pro/Max/Ultra. Orgs created on or after Oct 31, 2026 start with memory on. Local Cowork sessions do not use memory. — [same](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Claude Projects: knowledge is loaded in context by default and switches automatically to "RAG mode" near the context limit, expanding capacity "by up to 10x". A visible "project knowledge search tool" appears when it retrieves. — [Claude help: RAG for Projects](https://support.claude.com/en/articles/11473015-retrieval-augmented-generation-rag-for-projects)

**Claude memory tool (API): file-based, client-side memory and the closest analogue to a vault memory note**
- "The memory tool operates client-side: Claude requests file operations, and your application runs them." The model reads and writes files under a `/memories` prefix that the app maps to real storage. Commands are view, create, str_replace, insert, delete and rename. — [Claude docs: Memory tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)
- It is "just-in-time context retrieval": "Rather than loading all relevant information up front, an agent records what it learns in memory files and reads them back on demand." The API injects a system instruction: "ALWAYS VIEW YOUR MEMORY DIRECTORY BEFORE DOING ANYTHING ELSE." — [same](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)
- Recommended safeguards: strip sensitive data before writing ("Claude usually refuses to write sensitive information... For stronger guarantees, add validation"), cap file sizes and `view` output, expire stale files, and enforce strict path-traversal protection. — [same](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)

**Gemini (Google)**
- "Personal context → Your past chats with Gemini" remembers "key details and preferences you've shared" and is "on by default". Past conversations are managed in Gemini Apps Activity (announced Feb 26, 2026, Canada rollout). — [Google blog (en-CA)](https://blog.google/intl/en-ca/feed/use-past-chats-to-get-more-personalized-responses/)
- Google is renaming "past chats" to "memory" and supports importing AI memories and chat history from other assistants. — [Google blog: Make the switch](https://blog.google/innovation-and-ai/products/gemini-app/switch-to-gemini-app/)
- "Personal Intelligence" (beta) is a separate layer drawing on Gmail, Photos, Search, YouTube and past chats. It is for personal accounts only and excluded from the EEA, Switzerland, the UK and Nigeria. Temporary Chats are excluded from history and personalization. — [Gemini Personal Intelligence overview](https://gemini.google/ca/overview/personal-intelligence/)
- Earlier "personal context" launch coverage [older, Aug 2025]: [9to5Google](https://9to5google.com/2025/08/13/gemini-personal-context/)

**Notion AI: memory as visible workspace objects**
- Notion Agent memories store role, team, frequent collaborators, active projects/workflows, often-used pages, and "preferences or corrections you share in chats". They live in a **Memories database** plus an **About page**, which is "the summary your agents read about you" (Settings → Notion AI → Memory). Users can edit or remove any entry. — [Notion help: View your Notion Agent Memories](https://www.notion.com/help/view-your-notion-agent-memories)
- Defaults: on for Notion Agent and off for Custom Agents. Memories are private per user and "don't grant access to new content". Workspace owners can disable them globally. — [same](https://www.notion.com/help/view-your-notion-agent-memories)
- MCP-connected tools (Claude Code, ChatGPT, Codex) can read and add to the same Memories. Memory becomes a shared, cross-tool store. — [same](https://www.notion.com/help/view-your-notion-agent-memories)
- The help page does not say whether agents always load memories or search them. The "About page" wording implies a summary that is always read. — [same](https://www.notion.com/help/view-your-notion-agent-memories)

**Mem (mem.ai)**
- Mem Chat can "create, edit, organize, and ask about your notes". "Tell Mem Chat things you want to remember, and it will intelligently create or update notes." In other words, memory writes become ordinary notes. — [Mem help: Chat](https://help.mem.ai/features/chat); [Mem: Chat feature page](https://get.mem.ai/features/chat)
- Chat has a context-window limit. — [Mem help: chat limits](https://help.mem.ai/features/chat-limits)

**Basic Memory: "the notes are the memory"**
- "A knowledge graph written in plain Markdown". It contrasts itself with built-in AI memory that is "small, vendor-owned, and usually invisible". For the local version, "Your files never leave your computer". Agents maintain notes with "every change reviewed by you". Integrates over MCP with Claude, ChatGPT, Codex, Gemini, Cursor, VS Code and Obsidian. — [Basic Memory docs](https://docs.basicmemory.com/)
- Format: observations are list items `- [category] content #tag (context)`. Relations are `- relation_type [[WikiLink]] (optional context)`. The parser reads them anywhere in the file, and Markdown checkboxes are not treated as categories. — [Basic Memory: knowledge format](https://development.docs.basicmemory.com/concepts/knowledge-format); [AI assistant guide](https://docs.basicmemory.com/guides/ai-assistant-guide)
- `build_context("memory://...", depth=N)` follows relations to gather connected notes, and wildcards like `memory://docs/*` are supported. — [3rd-party mirror] [mintlify Basic Memory knowledge graph](https://www.mintlify.com/basicmachines-co/basic-memory/concepts/knowledge-graph); [memory-notes skill](https://tessl.io/registry/skills/github/basicmachines-co/basic-memory/memory-notes)

**Letta (MemGPT lineage)**
- V1 agents have labelled `memory_blocks` (e.g., `human`, `persona`) plus "archival memory" (passages) and searchable message history. — [Letta docs: MemGPT concepts](https://docs.letta.com/concepts/memgpt)
- The newer **MemFS** is "a git-backed memory filesystem that they can inspect and edit". Memory is projected into Markdown files that the agent reads, edits and versions. Local-mode agents commit to a local git repo with no remote. It is on by default for new agents in Letta Code 0.15 and later. — [Letta Code: Memory & dreaming](https://docs.letta.com/letta-code/memory); [Letta MemFS](https://docs.letta.com/letta-code/memfs)
- **Dreaming**: background subagents "review recent conversations and update memory without interrupting your work". They trigger after N agent steps or on context compaction. An optional second pass reviews proposed updates, but it "does not ask you for approval". `/doctor` audits memory duplication and system-prompt token use. Repo limits are 5 MiB (Free) and 50 MiB (Pro). — [Letta Code: Memory & dreaming](https://docs.letta.com/letta-code/memory)

**Obsidian ecosystem (direct competitors in Knotes' file format)**
- Copilot for Obsidian has a Vault QA mode (local vector store, cited sources). Chat history is not kept by default ("Save as Note"). Long-term memory is described as a Plus-tier feature from v3.1.0. — [3rd-party fork/mirror] [obsidian-copilot fork](https://github.com/MartletH/obsidian-copilot); official repo not checked
- Other plugins: "The Last Brain" lets users inspect sources behind each answer and save memories only after review. "Yaaiop" stores opt-in memories in its plugin data file and re-sends them with every message. — [The Last Brain](https://community.obsidian.md/plugins/the-last-brain); [Yaaiop](https://www.obsidianstats.com/plugins/yaaiop)

### Inferences
- **Common design**: (1) a small, always-in-context "profile" (Claude's legacy summary, Notion's About page, ChatGPT saved memories); (2) larger memory retrieved on demand by a visible tool (Claude chat search, Claude Project knowledge search, the Claude memory tool's `view`); (3) a user-facing list where each entry can be edited or deleted; (4) per-chat opt-out and incognito; (5) scoping (Claude per-project memory ≈ Knotes per-vault).
- **Best fit for Knotes**: a plain Markdown memory note (or folder) inside the vault, for example `About me.md` plus `Memory/` notes. Three products validate the pattern: Basic Memory (notes are the memory), Letta MemFS (git-tracked Markdown) and the Claude memory tool (client-side files, model asks the app to edit). Benefits:
  - Nothing new leaves the machine except at request time (consistent with store:false).
  - The user edits memory in the normal editor.
  - Knotes' existing git integration gives history and undo for free.
  - It stays Obsidian-compatible.
- **Write path**: Basic Memory ("every change reviewed by you") and The Last Brain (save after review) use explicit approval. Letta and ChatGPT's 2026 automatic memory write in the background without approval. Knotes' vault tools are read-only today, so a proposed-edit diff that the user accepts fits Knotes' trust posture better than silent writes. This is an inference, not measured.
- **Gap competitors leave open**: both OpenAI and Anthropic say deleting a chat does not delete memories derived from it. Knotes could store a back-link from each memory entry to the conversation that produced it, for example `source: [[Chats/2026-11-12 ...]]`. Deletion and provenance would then be traceable, which is a differentiator.
- **Privacy**: anything always injected is sent to OpenAI on every request. store:false limits retention but not transmission. Applying Claude's sensitive-category defaults (no health/religion/IDs unless opted in) to automatic memory writes is a cheap guardrail.
- **Past-conversation retrieval** is cheap for Knotes because conversations are already stored locally per vault. A `search_conversations` read-only tool, shown as a visible step and citing the chat, mirrors Claude chat search.

### Gaps
- Exact injection mechanism of ChatGPT "reference chat history" (pre-loaded summary vs retrieval) is undocumented. Sources conflict.
- OpenAI help and blog pages could not be fetched (403). Wording of the 2026 release notes (Dreaming, Sources icon, personalized temporary chats) comes from search summaries only.
- No quantitative evaluations of memory quality or user satisfaction from OpenAI, Anthropic, Google or Notion were found.
- A per-entry size cap for ChatGPT saved memories is reported by third parties but was not verified, so it is omitted.
- Mem's docs do not say whether Chat answers cite specific notes.

---

## 2. Personalization of answers to the user's vault (profile, language/style, glossary/aliases, "about me")

### Takeaway
Shipping products personalize through three things: an editable profile or summary (Notion About page, Claude memory topics including "communication preferences", ChatGPT custom instructions and saved memories), per-workspace or per-project instructions (Claude Projects, NotebookLM "Configure chat" styles and roles), and the user's own notes as the knowledge source (Mem, Basic Memory). No product found offers an explicit glossary of people, places and aliases. That is an open niche that Obsidian's native `aliases` frontmatter could fill.

### Cited Findings
- Claude memory topics explicitly include "communication preferences" and "technical preferences" besides role and projects. — [Claude help](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md)
- Notion memory stores "the people you work with often", "pages you use often", and "preferences or corrections you share in chats". The About page is the summary agents read. — [Notion help](https://www.notion.com/help/view-your-notion-agent-memories)
- ChatGPT's response "Sources" shows custom instructions alongside saved memories and past chats as inputs that shaped an answer (May 2026). — [ChatGPT release notes](https://help.openai.com/en/articles/6825453-chatgpt-release-notes) (via search summary)
- NotebookLM (help page now titled "Gemini Notebook") offers "Configure Chat" with conversational styles (Default, Learning Guide, Custom role) and a response-length setting. — [Google help: Use chat](https://support.google.com/notebooklm/answer/16179559?hl=en)
- Mem: "Powered by your notes, Mem knows what you need to know"; facts told to chat become or update notes. — [Mem Chat feature page](https://get.mem.ai/features/chat); [Mem help: Chat](https://help.mem.ai/features/chat)
- Basic Memory's typed relations (`- works_at [[Company]]`) and observations (`- [preference] ...`) give a structured, Markdown-native way to store facts about people and places. — [Basic Memory knowledge format](https://development.docs.basicmemory.com/concepts/knowledge-format)
- Obsidian natively supports an `aliases` property in note frontmatter, so links and search resolve alternative names. — [Obsidian help: Aliases](https://help.obsidian.md/aliases) (from prior knowledge; URL not re-fetched this session)

### Inferences
- A Knotes "About me" note (preferred answer language, for example Ukrainian; style; date format; roles; key people) always injected, plus `aliases:` frontmatter on people and place notes resolved by the search tool, would cover profile, language and glossary without a new storage format. Effort is low: injection, plus making the search tool expand aliases.
- A user-editable profile beats an inferred one for trust. Notion and Claude both expose the summary the model reads, so Knotes should show the exact injected text, for example a "what the model knows about you" link in chat.
- Per-vault instructions (like Claude Projects or NotebookLM Configure chat) map onto a vault-level settings note.

### Gaps
- No primary source found on how any vendor resolves aliases or nicknames of people in personal notes, or on a vendor-built glossary feature.
- No measurements of the effect of profile injection on answer quality.

---

## 3. Citations: how they are produced, verified and displayed

### Takeaway
There are two production models. (1) **Structured, API-guaranteed citations**: Claude's citations API chunks documents into sentences and returns `cited_text` with char, page or block ranges, "guaranteed to contain valid pointers". (2) **Model-emitted markers mapped to retrieved results**: OpenAI file_search `file_citation` annotations (file_id + text index, no span), and Perplexity numbered markers mapped to `search_results` ids. NotebookLM leads on display: hover shows the full quoted passage, and a click opens the source at the quote in context. Knotes runs OpenAI with custom tools, so it has no native span citations and has to verify on the client side.

### Cited Findings

**Claude citations API**
- Documents (plain text, PDF, custom content) are "chunked" to define the minimum citation granularity. Plain text and PDF use sentence chunking. Custom content uses caller-defined blocks as-is. — [Claude docs: Citations](https://platform.claude.com/docs/en/build-with-claude/citations)
- Citation formats: `char_location` (0-indexed char ranges, plain text), `page_location` (1-indexed pages, PDF) and `content_block_location` (block indices, custom content), each with `cited_text`. — [same](https://platform.claude.com/docs/en/build-with-claude/citations)
- "Because the API parses citations ... and extracts `cited_text` directly, citations are guaranteed to contain valid pointers to the provided documents." `cited_text` does not count toward output tokens (or input tokens when passed back). — [same](https://platform.claude.com/docs/en/build-with-claude/citations)
- "In Anthropic's evaluations, the citations feature is significantly more likely to cite the most relevant quotes from documents than purely prompt-based approaches." — [same](https://platform.claude.com/docs/en/build-with-claude/citations)
- Limits: citations are incompatible with structured outputs (400 error), and image citations from PDFs and scanned PDFs without text are not citable. — [same](https://platform.claude.com/docs/en/build-with-claude/citations)
- Launch claims [older, 2025]: built-in citations increased "recall accuracy by up to 15%" vs custom implementations. Customer Endex "reduced source hallucinations and formatting issues from 10% to 0%" with "a 20% increase in references per response". — [Claude blog: Introducing Citations](https://claude.com/blog/introducing-citations-api) (vendor and customer claims, not independently verified)

**OpenAI Responses API file search**
- Citations arrive as `file_citation` annotations on output text with `file_id`, `filename` and `index` (a position in the text). Retrieved chunks are returned only with `include: ["file_search_call.results"]`. The docs do not describe span-level (char-range) source pointers. — [OpenAI docs: File search](https://developers.openai.com/api/docs/guides/tools-file-search)

**Perplexity**
- Agent API: the model writes numbered markers that map to the `id` of items in a separate `search_results` output. The docs advise using returned search results rather than model-written URLs, which can be hallucinated. — [Perplexity cookbook: streaming citations](https://docs.perplexity.ai/docs/cookbook/articles/streaming-citations)
- Community reports say `[n]` markers do not always align with citation list positions, so match by id. — [3rd-party/community] [Perplexity community thread](https://community.perplexity.ai/t/model-content-vs-search-results-citations-references/1165)

**NotebookLM / "Gemini Notebook"**
- Responses use "direct quotes, text, and images from your sources as citations". Hovering shows the full quoted text, and selecting a citation opens the source at the quote in context. Saved-as-note responses keep "clickable inline citations". Users choose which sources to include. — [Google help: Use chat](https://support.google.com/notebooklm/answer/16179559?hl=en)
- Inline citations appear as encircled numbers that jump to supporting passages [older, 2024]. — [TechRadar](https://techradar.com/computing/artificial-intelligence/googles-notebooklm-is-now-an-even-smarter-assistant-and-better-fact-checker)
- Citations point to passages in the user's own sources, not to bibliographic records. — [3rd-party] [CASRAI guide](https://www.casrai.org/guides/notebooklm-for-research-workflows)

**Claude consumer chat search** cites the original chats it drew on. — [Claude help](https://support.claude.com/en/articles/11817273-how-does-claude-s-memory-work) (via search summary)

### Inferences
- Knotes' wiki links are model-emitted pointers with no guarantee that the cited section supports the claim. Two options:
  - **Option A, client-side "Claude-style" verification**: require each citation to carry a short verbatim quote (e.g., `[[Note#Heading]]` plus `> "quote"` or a hidden attribute). Knotes then fuzzy-matches the quote against the note or PDF page locally and resolves it to a char range. Links or quotes that do not match are flagged, and a matched range can be highlighted when the note opens. Cost: extra output tokens for quotes, which OpenAI bills (Claude's `cited_text` is free).
  - **Option B, sentence chunk IDs**: vault tools return sentence- or paragraph-level chunks with stable IDs (like Claude's custom content blocks or Perplexity's search_result ids). The model cites IDs, which Knotes maps to exact spans. Pointers are then always valid by construction, and output is shorter than quoting.
- Perplexity's advice (do not trust model-written URLs, map to returned results) translates directly: Knotes should flag or demote any `[[link]]` to a note that was not returned by a tool in that turn.
- NotebookLM's hover preview plus open-at-quote is the UX benchmark. Knotes already has open-at-section, so the missing parts are the hover popover with the quoted text and the highlighted span.

### Gaps
- No primary source on how NotebookLM generates citations internally (model-emitted vs post-hoc alignment).
- No source found on how Notion AI displays or verifies sources in Q&A answers in 2026.
- Official Copilot for Obsidian docs on its sources list were not found (only forks).
- Publication date of Anthropic's Citations launch post is unclear: the fetcher extracted "June 23, 2025", and this was not cross-checked.

---

## 4. Grounding and hallucination mitigation for personal-knowledge QA

### Takeaway
The best-documented techniques are cheap prompt-level ones: permission to say "I don't know", quote-first-then-answer, and a post-hoc pass that finds a supporting quote for each claim and retracts the claim if none is found (Anthropic docs). NotebookLM, the strongest closed-domain product, refuses when the answer is not in sources. Contradiction handling is a known weak spot: benchmarks show models miss implicit conflicts, and explicitly prompting them to reason about conflicts "significantly improves" responses.

### Cited Findings
- Anthropic recommends:
  - (1) "Allow Claude to say 'I don't know'", which "can drastically reduce false information".
  - (2) For long documents (>20k tokens), "extract word-for-word quotes first before performing its task".
  - (3) "Verify with citations... have Claude verify each claim by finding a supporting quote after it generates a response. If it can't find a quote, it must retract the claim". The example prompt removes unsupported claims and marks the gap with `[]`.
  - (4) Best-of-N comparison (inconsistencies across samples flag hallucinations), iterative refinement, and "External knowledge restriction".
  - The techniques "don't eliminate them entirely".
  - Source: [Claude docs: Reduce hallucinations](https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/reduce-hallucinations)
- NotebookLM "is designed to answer questions based on the information provided in your uploaded sources. If the answer isn't in the source material, it won't provide a response." — [NotebookLM FAQ](https://support.google.com/notebooklm/answer/16269187?hl=en); [Learn about NotebookLM](https://support.google.com/notebooklm/answer/16164461)
- "Chat responses in Gemini Notebook only use data from your sources." Creative requests may get a "can't answer" response. — [Google help: Use chat](https://support.google.com/notebooklm/answer/16179559?hl=en)
- **WikiContradict** (NeurIPS 2024) [older]: 253 human-annotated instances of contradictory Wikipedia passages. "All models struggle to generate answers that accurately reflect the conflicting nature of the context, especially for implicit conflicts requiring reasoning." An automated judge (fine-tuned Llama-3-70B) reaches F≈0.8 against humans. — [arXiv 2406.13805](https://arxiv.org/html/2406.13805v1); [NeurIPS poster](https://neurips.cc/virtual/2024/poster/97844)
- **CONFLICTS / "DRAGged into Conflicts"** (Cattan et al., 2025) [older]: proposes a taxonomy of knowledge-conflict types with a desired behaviour for each. "LLMs often struggle to appropriately resolve conflicts between sources". Prompting LLMs "to explicitly reason about the potential conflict in the retrieved documents significantly improves the quality and appropriateness of their responses." — [arXiv 2506.08500](https://arxiv.org/pdf/2506.08500)
- **MAGIC** (2025) [older]: models remain imperfect at detecting inter-context conflicts, especially multi-hop ones. Even when they detect conflicts they often fail to localize the exact point. — [arXiv 2507.21544](https://arxiv.org/pdf/2507.21544v1.pdf) (attribution of this arXiv ID to MAGIC comes from a search summary and was not opened)
- Confidence display: no primary source was found for a shipping notes assistant showing calibrated confidence per claim. — (gap, see below)

### Inferences
- Knotes' observed failures map onto the documented fixes:
  - **"Estimates instead of quoting data"**: use quote-first for numeric or factual questions. The system prompt requires verbatim figures with a citation, and if no figure is found the model says so instead of estimating (Anthropic techniques 1–2).
  - **"Missed contradiction between two notes"**: add an explicit conflict check to the prompt (CONFLICTS finding). When retrieved notes give different values or dates for the same thing, report both with their citations and note dates (mtime or frontmatter), and do not pick one silently. A cheap step is a "Conflicts" line in the answer when it applies.
  - **"Not in your notes"**: make abstention a first-class answer, as in NotebookLM, but allow general knowledge when the user opts in, clearly labelled as not from the vault.
- **Verification pass**, in order of increasing effort and cost:
  - (a) Local check: string or fuzzy matching of quotes, plus checking that each cited link and anchor exists. No API cost, done in Rust.
  - (b) A second cheap model call ("for each sentence, which cited quote supports it?"), adding latency and tokens.
  - (c) A local NLI model (as in ALCE-style evaluation).
  - Option (a) gives most of the value for Knotes' scale.

### Gaps
- No vendor publishes how often its notes assistant abstains, or false-abstention rates.
- No primary evidence of per-claim confidence display in NotebookLM, Notion AI, Mem or ChatGPT.
- No benchmark specific to personal-notes QA (as opposed to Wikipedia or web) for contradiction detection was found.

---

## 5. Trust UX: tool steps, sources panel, open at passage, highlighting

### Takeaway
Shipping trust patterns are: visible tool calls for retrieval (Claude chat search, Claude Project knowledge search), a per-answer "Sources" disclosure that includes *memory* inputs (ChatGPT, May 2026), hover previews of the exact quote, a click that opens the source at the quote (NotebookLM), and citations that survive when the answer is saved as a note (NotebookLM). Knotes already has open-at-section and PDF pages. The parts it still lacks are the quote preview, the highlighted span, and showing which memory or profile entries shaped an answer.

### Cited Findings
- Claude chat search "appear[s] as tool calls". Project RAG shows a "project knowledge search tool" when retrieving, and a visual indicator marks RAG-enabled projects. — [Claude help: memory/chat search](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context.md); [Claude help: RAG for Projects](https://support.claude.com/en/articles/11473015-retrieval-augmented-generation-rag-for-projects)
- ChatGPT "Sources" icon under a response lists the saved memories, past chats and custom instructions that informed it, and lets the user correct or delete them. — [ChatGPT release notes](https://help.openai.com/en/articles/6825453-chatgpt-release-notes) (via search summary)
- NotebookLM: hover shows the full quoted text, selection opens the source at the quote in context, saved notes keep clickable citations, and a per-source checkbox controls which sources are used. — [Google help: Use chat](https://support.google.com/notebooklm/answer/16179559?hl=en)
- NotebookLM "Source guide": an auto-generated summary card per source. — [3rd-party] [Computerworld](https://www.computerworld.com/article/1611774)
- Basic Memory treats user review of every agent change as the trust mechanism. — [Basic Memory docs](https://docs.basicmemory.com/)
- Fluent answers create a "facade of trustworthiness" even when verifiability is poor [older, 2023]. — [Liu et al., arXiv 2304.09848](https://arxiv.org/abs/2304.09848)

### Inferences
- Candidate Knotes features, ranked by trust per unit of effort:
  1. **Hover popover on `[[Note#Heading]]`** showing the cited quote or section text. Low effort, since the text is local.
  2. **Highlight the cited span** when opening (requires quote → char range from §3 Option A/B). Medium effort.
  3. **"Unverified" badge** on links whose quote did not match, or that point to notes not retrieved this turn. Low–medium effort.
  4. **Collapsible tool-step trace** (search queries, notes read). Possibly partly present already. Low effort.
  5. **"Used memory/profile" chip** listing the memory entries injected, each linking to the memory note. Low effort once memory exists.
  6. **Keep citations when saving a chat answer as a note**: an Obsidian-native win, because the links are already wiki links.
- All of these are fully local, so there is no privacy cost.

### Gaps
- No user studies found that quantify how hover previews or highlighting change trust or verification behaviour in shipping products.

---

## 6. Measurements of citation accuracy and user trust

### Takeaway
Independent measurements are sobering. In 2023 research, generative search engines fully supported only ~51.5% of sentences with citations, and only ~74.5% of citations supported their sentence. In ALCE, the best models lacked complete citation support ~50% of the time on ELI5. The 2025 EBU/BBC study found ~45% of AI-assistant news answers had a significant issue, with ~31–33% showing sourcing problems. Vendor claims are much rosier (Endex 10%→0% source hallucinations with Claude Citations) and are unverified. Most data predate 2026. No 2026 independent citation-accuracy benchmark for notes assistants was found.

### Cited Findings
- **Liu, Zhang, Liang 2023, "Evaluating Verifiability in Generative Search Engines"** [older]: across Bing Chat, NeevaAI, Perplexity.ai and YouChat, "a mere 51.5% of generated sentences are fully supported by citations and only 74.5% of citations support their associated sentence". — [arXiv 2304.09848](https://arxiv.org/abs/2304.09848)
- **ALCE (Gao et al., 2023)** [older]: a benchmark with automatic metrics for fluency, correctness and citation quality that "strongly correlate" with human judgments. On ELI5, "even the best models lack complete citation support 50% of the time". Datasets are ASQA, QAMPARI and ELI5. ELI5 evaluation uses claim-level NLI (`--claims_nli`). — [arXiv 2305.14627](https://arxiv.org/abs/2305.14627); [ALCE GitHub](https://github.com/princeton-nlp/ALCE)
- **EBU/BBC News Integrity study (Oct 2025)** [older; reported via press, primary report not opened]: 14 languages, ~2,700–3,000 responses from ChatGPT, Copilot, Gemini and Perplexity. ~45% had at least one significant issue, ~31–33% had sourcing problems, and Gemini had sourcing problems in ~72% of answers vs <25% for others. Accuracy issues occurred in ~20%. — [Search Engine Journal](https://beta.searchenginejournal.com/ai-assistants-show-significant-issues-in-45-of-news-answers/558991/); [Tom's Guide](https://www.tomsguide.com/ai/45-percent-of-ai-generated-news-is-wrong-new-study-warns-heres-what-happened-when-i-tested-it-myself); [implicator.ai](https://www.implicator.ai/ai-assistants-misreport-news-in-45-of-cases-putting-trusted-brands-at-risk/) (figures vary slightly between outlets)
- **Tow Center (CJR)** [older; secondhand]: about 1,600 queries across eight chatbots, with Grok reportedly wrong on 94% of news-article queries and chatbots generally weak at citing sources. — [Techdirt](https://www.techdirt.com/tag/assistants/) (original report not checked)
- **WikiContradict** automated judge F≈0.8 vs humans for conflict-aware answers. — [arXiv 2406.13805](https://arxiv.org/html/2406.13805v1)
- **Vendor claims**: Claude Citations "up to 15%" recall accuracy gain, and Endex source hallucinations 10%→0% with +20% references per response. — [Claude blog](https://claude.com/blog/introducing-citations-api) (vendor, unverified)

### Inferences
- These figures come mostly from open-web QA, which is harder than closed-domain vault QA. Knotes should still assume a substantial share of citations may not fully support their claims unless they are verified. This justifies at least the cheap local verification pass.
- Knotes could build its own small citation eval: extend the existing search benchmark with expected supporting sections and score citation precision/recall (ALCE style) with string matching first and an NLI model later. This would also catch regressions like the missed contradiction.

### Gaps
- No 2026 independent measurement of citation accuracy for NotebookLM, Notion AI, Mem, ChatGPT memory-grounded answers, or Obsidian plugins.
- No published user-trust studies (surveys or behaviour) from these vendors on citations or memory transparency.
- The primary EBU/BBC report and the Tow Center report were not opened, so their figures are press-reported.
