# Agent patterns and structured-query tools for notes AI (state as of 2026-11-14)

Scope: how leading notes/PKM AI harnesses run multi-step questions, when subagents pay off, tool-design guidance (Anthropic, OpenAI, MCP), structured queries over notes (Dataview, Bases, Tana, Capacities, MCP servers), time-aware retrieval, mitigations for early stopping / estimating / missed contradictions, and reasoning-effort controls. Knotes baseline for comparison: up to 16 model steps, parallel read-only tools (search, find_by_title, read_note, read_pdf, outline, links, related, list_notes, list_tasks, list_tags), vault overview in instructions, summarization at 90% of window; 28/30 correct, ~3 calls per question, overview questions take 9–28 calls and 30–55 s, model sometimes stops at first plausible note and estimates.

Source-dating legend: [2024], [2025] = older than 12 months, treat as background; [2026] = current. "via search summary" = fact came from a search-result summary, page not fetched in full.

## 1. How notes/PKM products handle multi-step questions (planning, step budgets, stopping, self-verification)

### Takeaway
Shipping notes assistants use a single tool loop with a hard iteration cap (Obsidian Copilot 4–8; Capacities a cost budget with partial answers), add a cheap router/classifier in front, and in 2026 increasingly add an explicit planning step plus an evaluator between phases for multi-item requests; separate "research/deep" modes (Khoj, NotebookLM) trade latency for large accuracy gains on multi-hop questions.

### Cited Findings
**Obsidian Copilot (logancyang)**
- v3.0.0 "Autonomous Agent" (Plus feature) triggers tool calls automatically instead of explicit `@tool` commands; tools: local vault search, web search, Composer (edits), YouTube processing, each toggleable; "Max Iterations Control": reasoning depth adjustable from 4 to 8 iterations, higher for "more complex queries"; expandable tool-call banners show which tools ran and their results, plus a "Tool Execution Banner" during execution; works best with Gemini, then Claude and GPT. Release is ~14 months old at capture [2025] — [newreleases.io: obsidian-copilot 3.0.0](https://newreleases.io/project/github/logancyang/obsidian-copilot/release/3.0.0)
- Current README ("Copilot V4"): Agent mode now runs external coding agents inside Obsidian — OpenCode (recommended, downloaded and managed by Copilot), Claude Code, Codex (via `@agentclientprotocol/codex-acp`); multi-agent `@`-mention of several agents for one read-only research/review request requires Plus; built-in skills for Obsidian Markdown, **Bases**, Canvas and the Obsidian CLI; Projects carry their own instructions, context and history [2026] — [obsidian-copilot README](https://cdn.jsdelivr.net/gh/logancyang/obsidian-copilot@master/README.md)
- Tool source files in the repo include `SearchTools.ts`, `NoteTools.ts`, `FileTreeTools.ts`, `TagTools.ts`, `TimeTools.ts`, `ObsidianCliDailyTools.ts`, `ObsidianCliTools.ts`, `memoryTools.ts`, `ToolResultFormatter.ts` (functions inferred from filenames only) — [GitHub src/tools](https://github.com/logancyang/obsidian-copilot/tree/master/src/tools)
- Time tools: `getCurrentTime`, `getTimeRangeMs` ("Convert natural language time expressions to date ranges for use with localSearch"), `getTimeInfoByEpoch`, `convertTimeBetweenTimezones`; the model is told to call `getTimeRangeMs` first and pass the returned `{startTime,endTime}` (epoch ms) to `localSearch` — [TimeTools.ts](https://raw.githubusercontent.com/logancyang/obsidian-copilot/master/src/tools/TimeTools.ts)

**Capacities (most detailed public architecture write-up, 2026-08-11)**
- Single continuous tool loop orchestrator; a lightweight request classifier runs first — trivial inputs (greetings) skip the orchestrator, otherwise it **prunes the tool set** to relevant capabilities — [Capacities dev blog: AI systems at Capacities](https://capacities.io/blog/ai-systems-at-capacities)
- Read tools: `searchNotes`, `getNoteContent`, `getMediaContent`, `exploreConnections`, `generateQuery`, `getQueryResults` (+ optional web search); write tools pause the loop for user approval — [same](https://capacities.io/blog/ai-systems-at-capacities)
- `planAgentLoop` creates ordered execution plans for multi-item requests; orchestrator runs phases and checks completion with a **task evaluator** between write phases; `writingAssistantAgent` produces the final user-facing answer from accumulated context — [same](https://capacities.io/blog/ai-systems-at-capacities)
- Shared **budget accumulator** across main stream and sub-agents; when budget runs low the system returns a **partial response** rather than stopping abruptly; execution pauses after too many loops (no number given) — [same](https://capacities.io/blog/ai-systems-at-capacities)
- Lessons: collapsing 13+ client tools into 4 entry points kept routing accurate even on cheap models; splitting across agents adds latency; context rot hits cheaper models as prompts/tools grow; prompts must work on the cheapest major-provider model ("is simply not good enough" otherwise) — [same](https://capacities.io/blog/ai-systems-at-capacities)
- Assistant "can decide to read the full content of notes in the backlink section or from the search results"; object queries return content by object types + optional filters (via search summary) — [Capacities docs: AI assistant](https://docs.capacities.io/reference/ai-assistant); [Capacities product](https://capacities.io/product)

**Khoj**
- Research mode (released Nov 2024, beta, `/research`): "Iterative retrieval is permitted"; same tools as default mode but runs longer. With gemini-1.5-flash-002 on 200-question subsets: FRAMES 42.0% (default) → 63.5% (research); SimpleQA 84.0% → 86.0%; adding the code tool lifted FRAMES default from 35.68% to 42.00%; error margin ~6% [2024] — [Khoj blog: Evaluating Khoj](https://blog.khoj.dev/posts/evaluate-khoj-quality/)
- A third-party guide shows an API example with `max_iterations` = 8 (not confirmed in official docs) — [skills.sh khoj-research-guide](https://skills.sh/wentorai/research-plugins/khoj-research-guide)

**Notion**
- Notion 3.0 Agents (2025-09-18): "over 20 minutes of multi-step actions"; memory through an instructions page and Notion pages/databases; can create/update hundreds of database pages; per-agent model choice (Claude Sonnet 4, GPT-5) [2025] — [Notion 3.0 release](https://notion.com/releases/2025-09-18)
- Custom Agents on schedules/triggers dated Feb 2026 (Notion 3.3); Enterprise Search described as iterative multi-step search (third-party, vendor-adjacent) — [eesel.ai Notion AI review](https://eesel.ai/blog/notion-ai-review); [eesel Personal Agent](https://www.eesel.ai/blog/notion-personal-agent)

**NotebookLM**
- Deep Research arrived Nov 2025 with two modes: "Deep Research" (thorough) vs "Fast Research" (quick fact-checking); the report and its sources (cited and uncited) can be imported as notebook sources — [Unite.AI](https://www.unite.ai/google-notebooklm-introduces-deep-research-feature/)
- GA on 2026-05-06 with 100+ sites auto-browsed: single unverified blog claim — [bibigpt.co](https://bibigpt.co/de/blog/posts/notebooklm-deep-research-vs-bibigpt-workflow-2026/)

**Tana**
- AI chat agents = own system prompt + commands, built on AI command nodes; search nodes are live queries (AND by default, capped at 2,500 nodes); supertags give typed nodes with fields; MCP exposes `search_nodes`, read nodes, update fields (via search summary) — [Tana AI docs](https://tana.inc/docs/tana-ai); [Tana search nodes](https://outliner.tana.inc/docs/search-nodes); [Tana supertags](https://outliner.tana.inc/docs/supertags); [Tana releases](https://tana.inc/releases)

### Inferences
- Knotes' 16-step cap is already generous vs Copilot's 4–8; the 9–28-call overview questions point to missing aggregate tools, not a too-small budget (see §3–4).
- Capacities' pattern (classifier → pruned tools → plan → evaluator → partial answer on budget exhaustion) maps cleanly onto Knotes: a cheap pre-classification of "lookup vs overview/list" could pick reasoning effort, step budget and a prompt variant.
- Returning a partial, explicitly-labelled answer when the budget runs out is better than letting the model estimate silently.
- Tool banners (Copilot) are the norm; MCP spec also says clients SHOULD show visual indicators of tool calls (§3).

### Gaps
- Mem, Logseq AI and Apple Intelligence: not researched in this pass (no primary sources gathered); no reliable data on their multi-step behaviour.
- Obsidian Copilot V4's iteration limits/stopping rules for its agent mode are not documented in the README; Khoj's official default iteration count not confirmed.
- No vendor publishes stopping criteria beyond iteration caps/budgets.

## 2. Subagents / orchestrator-worker for overview questions: when it pays off, cost and latency

### Takeaway
Multi-agent fan-out wins big on breadth-first research (Anthropic: +90.2% over single agent, up to 90% less wall time) but costs ~15x chat tokens (~4x for single agents) and adds latency; vendors (Anthropic docs, Claude Code docs, Capacities) advise using subagents only for parallel, independent, verbose sub-searches and warn about overuse.

### Cited Findings
- Orchestrator-worker: lead agent plans, saves plan to memory (context >200k gets truncated), spawns parallel subagents, synthesizes; a CitationAgent attributes claims afterwards [2025] — [Anthropic: multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)
- Opus 4 lead + Sonnet 4 subagents beat single-agent Opus 4 by 90.2% on internal research eval; on BrowseComp token usage alone explained ~80% of variance (tool-call count and model choice most of the rest) — [same](https://www.anthropic.com/engineering/multi-agent-research-system)
- Agents use ~4x tokens of chat; multi-agent ~15x — [same](https://www.anthropic.com/engineering/multi-agent-research-system)
- Effort-scaling rules embedded in prompts: simple fact-finding = 1 agent, 3–10 tool calls; direct comparisons = 2–4 subagents, 10–15 calls each; complex research = 10+ subagents — [same](https://www.anthropic.com/engineering/multi-agent-research-system)
- Lead spins up 3–5 subagents in parallel; subagents use 3+ tools in parallel; together cut research time by up to 90% on complex queries — [same](https://www.anthropic.com/engineering/multi-agent-research-system)
- Poor fit: tasks needing shared context or many inter-agent dependencies; good fit: "valuable tasks that involve heavy parallelization"; subagents need clear objective, output format, tool guidance and boundaries to avoid duplicated work — [same](https://www.anthropic.com/engineering/multi-agent-research-system)
- Subagents return condensed summaries of ~1,000–2,000 tokens after using tens of thousands internally — [Anthropic: effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) [2025]
- Claude Code docs: use subagents when output is verbose and not needed in main context, when tool restrictions are wanted, when work is self-contained; use the main conversation when "Latency matters", for quick targeted changes, or when phases share context; "A subagent that isn't a fork starts fresh and may need time to gather context"; each subagent spends its own tokens; default 20 concurrent subagents; built-in Explore is read-only with thoroughness "quick / medium / very thorough" — [Claude Code docs: subagents](https://code.claude.com/docs/en/sub-agents) [2026]
- Anthropic prompting guide: latest Claude models delegate proactively; "Watch for overuse" (e.g. spawning subagents where a direct grep suffices); sample damping prompt: "Use subagents when tasks can run in parallel, require isolated context, or involve independent workstreams that don't need to share state. For simple tasks, sequential operations ... work directly rather than delegating." — [Claude prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices) [2026]
- Capacities: sub-agents return compact summary references with full payloads kept server-side; but production MCP use showed external models choose tools correctly, which supported keeping a **single chat agent**; agent splitting adds latency — [Capacities dev blog](https://capacities.io/blog/ai-systems-at-capacities) [2026]
- Building effective agents: orchestrator-workers fit when "Subtasks aren't pre-defined, but determined by the orchestrator"; parallelization variants "sectioning" (independent subtasks) and "voting" (same task repeated) [2024] — [Anthropic: building effective agents](https://www.anthropic.com/engineering/building-effective-agents)
- OpenAI GPT-5 guide: peak performance when separable tasks are split across agent turns, one per task [2025, archived recipe] — [OpenAI GPT-5 prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)
- OpenAI GPT-5.5-era prompt guidance mentions delegating to another agent "If at any point you can parallelize work" (coding-oriented) — [OpenAI prompt guidance](https://developers.openai.com/api/docs/guides/prompt-guidance.md)

### Inferences
- For Knotes overview questions ("what's left to do", "which documents expire soon") the data are structured (checkboxes, dates, properties) — a single `query_notes`/`list_tasks` call with date filters should replace most of the 9–28 calls; fan-out subagents are a second-line option (high effort, ~4–15x tokens, extra latency, counts against ChatGPT-plan limits).
- If subagents are added later: restrict to read-only, give each a disjoint slice (folder, time window, or candidate-note batch), cap at ~3–5 in parallel, demand ≤1–2k-token structured summaries with note citations; use a cheaper/lower-effort model for workers.
- Cheaper intermediate: "parallel batch" inside the single loop (already allowed) plus explicit effort-scaling rules in the prompt (1 call for lookups, broad-then-narrow for lists).

### Gaps
- No public latency/cost numbers for subagent use specifically in notes apps; Anthropic numbers are for web research with Claude 4-era models (2025).
- Unknown how ChatGPT-subscription rate limits (Sign in with ChatGPT) treat many parallel worker calls — not researched.

## 3. Tool design guidance (Anthropic, OpenAI, MCP): granularity, response size, pagination, errors, token efficiency, response_format

### Takeaway
All three sources converge: few non-overlapping, workflow-shaped tools (OpenAI: soft cap <20 at turn start), natural-language identifiers, filtering/pagination/truncation with defaults and "how to narrow" hints, a `response_format` (concise/detailed) knob (~3x token saving), actionable `isError` results the model can self-correct from (MCP 2025-11-25 formalised this), strict schemas, and eval-driven iteration measuring calls/latency/tokens.

### Cited Findings
**Anthropic — Writing effective tools for agents (2025-09-11)**
- Don't wrap endpoints 1:1; prefer `search_contacts` over `list_contacts`; consolidate workflows: `schedule_event` instead of `list_users`+`list_events`+`create_event`; `search_logs` returning only relevant lines + context instead of `read_logs`; `get_customer_context` instead of three lookups — [Anthropic: writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Namespacing by service/resource (`asana_search`, `asana_projects_search`); prefix vs suffix effects vary by model, test it — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Return high-signal fields (`name`, `file_type`) not `uuid`/`mime_type`; resolving UUIDs to semantic names or 0-indexed IDs improved retrieval precision by reducing hallucinations — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- `response_format` enum: DETAILED (206 tokens in example) vs CONCISE (72 tokens); concise ≈ one-third of tokens in the Slack example; detailed returns IDs needed for chained calls — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Pagination, range selection, filtering, truncation with sensible defaults; Claude Code caps tool responses at 25,000 tokens by default; when truncating, tell the agent how to narrow; encourage many small targeted searches over one broad one — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Replace opaque errors/tracebacks with specific actionable guidance, with examples of correct input — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Descriptions "as if onboarding a new hire"; unambiguous parameter names (`user_id` not `user`); strict data models; small description refinements yield big gains (Sonnet 3.5 SWE-bench SOTA after description tweaks) — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Eval method: realistic tasks needing many calls; verifiable outcomes; track runtime, number of calls, tokens, tool errors (redundant calls signal pagination/limit problems); held-out sets; Claude-optimized tools beat expert-written ones on held-out Slack/Asana tests — [same](https://www.anthropic.com/engineering/writing-tools-for-agents)
- A tool-testing agent rewriting flawed tool descriptions cut task completion time by 40% — [Anthropic: multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)
- Keep tools minimal and non-overlapping: if "a human engineer can't definitively say which tool should be used", the agent can't either; tool-result clearing is the lightest-touch compaction — [Anthropic: context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- ACI: "Poka-yoke your tools"; requiring absolute file paths eliminated a class of errors [2024] — [Anthropic: building effective agents](https://www.anthropic.com/engineering/building-effective-agents)

**OpenAI — function calling guide (current)**
- Best practices: clear detailed names/params/instructions, say in system prompt when to use each; enums/object structure to prevent invalid states; "intern test"; "Don't make the model fill arguments you already know"; combine functions always called in sequence; "Aim for fewer than 20 functions available at the start of a turn" ("just a soft suggestion") — [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling)
- Strict mode recommended always (`additionalProperties:false`, all properties required, optional as `null` type); Responses API tries to normalize to strict by default — [same](https://developers.openai.com/api/docs/guides/function-calling)
- Function definitions count as input tokens; reduce via fewer upfront functions, shorter descriptions, or **tool search** (`defer_loading: true`, namespaces; requires gpt-5.4+) — [same](https://developers.openai.com/api/docs/guides/function-calling)
- Parallel calls on GPT-5+; `parallel_tool_calls:false` forces zero or one call — [same](https://developers.openai.com/api/docs/guides/function-calling)

**MCP specification**
- 2025-11-25 tools spec: **input validation errors** (e.g. "date in wrong format, value out of range") are now Tool Execution Errors (`isError: true`), not protocol errors; "Tool Execution Errors contain actionable feedback that language models can use to self-correct and retry"; clients SHOULD pass them to the model. Example text: "Invalid departure date: must be in the future. Current date is 08/08/2025." — [MCP spec 2025-11-25: tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- Same spec: `outputSchema` + `structuredContent` (SHOULD also return serialized JSON in a text block); `resource_link` content; annotations incl. `lastModified`, `audience`, `priority`; tool-name rules (1–128 chars, `[A-Za-z0-9_.-]`); `execution.taskSupport` (forbidden/optional/required) for task-augmented (long-running) calls; no-param tools should use `{"type":"object","additionalProperties":false}`; clients SHOULD show visual indicators when tools are invoked — [same](https://modelcontextprotocol.io/specification/2025-11-25/server/tools); older 2025-06-18 version for comparison — [MCP spec 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
- A 2026-07-28 MCP protocol revision is referenced by obsidian-local-rest-api (contents not researched) — [obsidian-local-rest-api](https://github.com/coddingtonbear/obsidian-local-rest-api)

**Notes-specific MCP servers (concrete conventions)**
- Basic Memory 1.0 renamed tools to `bm_*` (12 tools): `bm_search` (`query, mode, filters, note_types, since, lines, max_matches=10, compact, valid_at, valid_overlaps, time_kind, page=1, page_size=10, output_format`), `bm_context` (`identifier, depth=1, since="7d", max_related=10, compact, page_size=10, output_format="json"`), `bm_read` (`section, start_line, end_line, max_tokens, include_frontmatter`; partial reads number lines and say where to continue), `bm_find` (recursive by glob or frontmatter `filters`, `fields` returns selected frontmatter values), `bm_list`, `bm_project` (`command="recent"`, `since="7d"`), `bm_schema` (validate/infer/diff), `bm_man` (built-in manual) — [Basic Memory MCP tools reference](https://docs.basicmemory.com/raw/reference/mcp-tools-reference.md)
- Basic Memory responses: JSON includes `results, current_page, page_size, total, has_more`; `compact` drops bodies; errors set `isError: true` with `error`, `message`, optional **`try_instead`**; codes `NOT_FOUND`, `AMBIGUOUS`, `ALREADY_EXISTS`, `REVISION_CONFLICT`, `INVALID_ARGUMENTS` — [same](https://docs.basicmemory.com/raw/reference/mcp-tools-reference.md)
- cyanheads obsidian-mcp-server v3.9.0 (14 tools): `obsidian_get_note` formats content / full / **document-map** / section and periodic notes (daily…yearly, optional date); `obsidian_list_notes` depth 1–20 (default 2), capped 1,000 entries; `obsidian_list_tags` with counts, `minCount`, limit default 200; `obsidian_search_notes` modes text (`contextLength` default 100, `pathPrefix`, `maxMatchesPerHit`, `nextCursor`), **jsonlogic** (tree over path, content, frontmatter, tags, stat; `glob`, `regexp`), omnisearch (BM25); patch/append by heading, block ref or frontmatter field — [cyanheads/obsidian-mcp-server](https://github.com/cyanheads/obsidian-mcp-server)
- obsidian-local-rest-api: simple search warns "a query containing a common word can match thousands of times"; `maxResults`, `maxMatchesPerFile`, `totalMatches` per result; its MCP `search_simple` returns ≤20 files and 10 matches/file by default; JsonLogic search over frontmatter/tags/path/content; PATCH targets heading/block/frontmatter — [obsidian-local-rest-api](https://github.com/coddingtonbear/obsidian-local-rest-api)

### Inferences (Knotes, impact/effort)
- **Actionable errors + empty-result hints** (High/Low): when search returns 0 or few hits, return `isError`-style guidance with `try_instead` (e.g. "try list_notes with folder=…", "try query_notes with property due<…"); include current date in date-related errors (MCP example).
- **Totals and truncation hints on every list/search** (High/Low): always return `total`, `has_more`, `next_cursor`, and a one-line "narrow with …" hint; models stop early partly because they can't see how much they haven't seen.
- **`response_format`/`fields` parameter** (Medium/Low): concise rows (title, path, date, 1-line snippet) by default for list-type tools, detailed on request; ~3x token savings per Anthropic.
- **Document-map style outline** already exists (`outline`); consider merging `outline` into `read_note(format=map)` to reduce overlap (cyanheads pattern) (Low/Low).
- **Tool count**: Knotes has 10 tools — within OpenAI's <20 soft cap; adding `query_notes` and `recent_activity` keeps it ≤12. Collapse overlapping ones (find_by_title vs search vs list_notes) only if evals show confusion (Capacities 13+→4 lesson).
- **Strict schemas** and enums for filters (Medium/Low); name params unambiguously (`note_path`, `since`, `until`).
- **Eval metrics**: add per-question tool calls, wall time, tokens, tool errors to the 30-question benchmark; keep a held-out set (Medium/Low).

### Gaps
- No official numeric guidance on optimal response size per tool call for GPT-5.x (only Claude Code's 25k cap and OpenAI's <20 tools).
- OpenAI's claim that strict mode conflicts with parallel calls appears only in third-party sources — not confirmed on the official page; official page only says fine-tuned models with multiple calls disable strict.

## 4. Structured queries over notes (Dataview DQL/TASK, Bases, properties, Notion, Tana, Capacities) and how MCP servers expose them; shape of a "query notes by properties/dates" tool

### Takeaway
The de-facto vocabulary for Obsidian-compatible structured queries is Bases (YAML filters with and/or/not, `file.*` properties, `today()/now()` with duration arithmetic, summaries) and Dataview (TASK with implicit task fields and emoji/inline date shorthands); MCP servers expose this as JSON filter trees (JSONLogic in cyanheads and local-rest-api, frontmatter `filters`+`fields` in Basic Memory) rather than raw query languages, and Capacities lets the AI generate a query and then execute it as two tools.

### Cited Findings
**Obsidian Bases**
- Bases shipped as early-access core plugin in Obsidian 1.9.0 (2025-05-21), with breaking syntax changes in 1.9.1/1.9.2, GA for everyone in 1.9.10 (Aug 2025); later added `file.backlinks`, `file()`, `file.hasProperty()` [2025] — [Obsidian 1.9.0 changelog](https://obsidian.md/changelog/2025-05-21-desktop-v1.9.0); [Obsidian 1.9.10 changelog](https://obsidian.md/changelog/2025-08-18-desktop-v1.9.10); [Neowin on 1.9.10](https://www.neowin.net/news/obsidian-1910-lands-with-a-new-core-plugin-bug-fixes-and-more/)
- `.base` YAML: `filters` (string or nested `and`/`or`/`not`), `formulas`, `properties` (display config), `summaries` (formulas over `values`), `views` (type, name, limit, groupBy, order, filters, summaries); global and view filters ANDed — [Obsidian help: Bases syntax](https://obsidian.md/help/bases/syntax)
- File properties: `file.name, path, folder, ext` (string), `file.mtime, ctime` (date), `file.size`, `file.tags, links, embeds` (lists), `file.backlinks` (performance-heavy; "prefer reversing the lookup with file.links"), `file.properties`; functions `file.hasTag()`, `file.hasLink()`, `file.inFolder()` — [same](https://obsidian.md/help/bases/syntax)
- Dates: `today()`, `now()`, `date("2024-12-01")`; durations `y/M/d/w/h/m/s` (`M` month vs `m` minute); date − date = duration; example `file.mtime > now() - "1 week"`; `note.release_date - today()`; default summaries include Earliest, Latest, Checked, Unchecked, Empty, Filled, Unique — [same](https://obsidian.md/help/bases/syntax)

**Dataview**
- `TASK` returns individual tasks; examples `TASK WHERE !completed AND contains(tags, "#shopping")`, `TASK WHERE !completed GROUP BY file.link`; due as inline field `[due:: 2022-08-13]`; child tasks follow matching parents; `CALENDAR due WHERE typeof(due) = "date"` — [Dataview query types](https://blacksmithgu.github.io/obsidian-dataview/queries/query-types/)
- Implicit task fields: `status, checked, completed, fullyCompleted, text, visual, line, lineCount, path, section, tags, outlinks, link, children, task, annotated, parent, blockId`; emoji shorthands: due `🗓️YYYY-MM-DD`, completion `✅`, created `➕`, start `🛫`, scheduled `⏳` — [Dataview task metadata](https://blacksmithgu.github.io/obsidian-dataview/annotation/metadata-tasks/)

**Other products**
- Tana: supertags = typed nodes with field templates forming searchable collections; search nodes are live queries built in a query builder (AND default, max 2,500 nodes); MCP `search_nodes` and field updates (via search summary) — [Tana search nodes](https://outliner.tana.inc/docs/search-nodes); [Tana supertags](https://outliner.tana.inc/docs/supertags); [Tana releases](https://tana.inc/releases)
- Capacities: object types (basic/custom) with properties; object queries by types + filters; agent tools `generateQuery` then `getQueryResults` (model builds a query, server executes) — [Capacities content types](https://docs.capacities.io/reference/content-types); [Capacities dev blog](https://capacities.io/blog/ai-systems-at-capacities)
- Notion 3.0 agents update/create database pages in bulk; no dedicated query feature described on the release page — [Notion 3.0](https://notion.com/releases/2025-09-18)

**MCP exposure**
- cyanheads: JSONLogic search over path/content/frontmatter/tags/stat with `glob` and `regexp` operators, paged by `nextCursor`; **no Dataview DQL** mentioned; modified-since/until filters not documented (maybe via `stat`) — [cyanheads/obsidian-mcp-server](https://github.com/cyanheads/obsidian-mcp-server)
- obsidian-local-rest-api: `POST /search/` with content type `application/vnd.olrapi.jsonlogic+json` evaluated per note's frontmatter, tags, path, content (DQL not covered in README as fetched) — [obsidian-local-rest-api](https://github.com/coddingtonbear/obsidian-local-rest-api)
- Basic Memory: `bm_find(filters=…, fields=[…])` returns selected frontmatter values per note; `bm_search(filters, note_types, since, valid_at, valid_overlaps, time_kind)`; `bm_schema infer` suggests fields from existing notes — [Basic Memory MCP reference](https://docs.basicmemory.com/raw/reference/mcp-tools-reference.md)
- Obsidian Copilot V4 ships a built-in "Bases" skill so agents can read/write `.base` files — [obsidian-copilot README](https://cdn.jsdelivr.net/gh/logancyang/obsidian-copilot@master/README.md)

### Inferences — proposed Knotes `query_notes` tool (High impact / Medium effort, SQLite already holds metadata)
- Input (strict JSON, Bases-vocabulary so users and model share terms): `where` = filter tree (`and`/`or`/`not` of comparisons) over `folder`, `tag`, `has_link`, `property.<name>` (frontmatter, typed: date/number/text/list/checkbox), `file.mtime`, `file.ctime`, `file.size`; date operands accept ISO dates and relative strings (`"today"`, `"today+30d"`, `"now-1w"`); `fields` (which properties to return), `sort`, `group_by` (folder/tag/property), `aggregate` (`count`, `min/max date`), `limit` (default 25) + `cursor`; `response_format` concise/detailed.
- Output: `total`, rows (title, path, requested fields, 1-line snippet), `has_more`, `next_cursor`, and a hint when 0 rows ("property 'expires' exists on 14 notes; did you mean 'expiry'?" — generated from a property catalog).
- Add `list_properties` (or extend `list_tags`/the vault overview) with property names, types, counts and example values — lets the model discover "expires/valid_until/термін" before querying (Basic Memory `bm_schema infer`, Tana field templates analogue) (High/Low).
- Extend `list_tasks` to parse Dataview inline fields `[due:: …]` and Dataview emoji dates (🗓️, ⏳, 🛫, ✅, ➕; the Tasks plugin also uses 📅 for due — not verified in this pass), with filters `status` (open/done/cancelled via `[-]`), `due_before/after`, `overdue`, `scheduled`, `folder`, `tag`, `group_by=file`, and `total` count — "what's left to do" becomes 1–2 calls (High/Low-Medium).
- Prefer a JSON filter over letting GPT write DQL: strict schema validation, actionable errors, no parser in the loop; optionally accept a Bases `filters` string later for power users.

### Gaps
- No primary source found for how Notion AI internally queries databases for Q&A (only release notes and third-party reviews).
- Whether obsidian-local-rest-api still accepts Dataview DQL search (historically documented) was not confirmed from the current README.
- No benchmark comparing JSON-filter tools vs query-language tools for LLM accuracy.

## 5. Time-aware retrieval: recent activity, date ranges, daily notes

### Takeaway
Leading notes MCP servers make time a first-class parameter on search/context tools (Basic Memory `since="7d"` default, natural-language timeframes; recent-activity tool), parse natural-language time server-side (Copilot `getTimeRangeMs`), and expose periodic/daily notes by date (cyanheads); Bases gives the canonical date-arithmetic syntax.

### Cited Findings
- Basic Memory: `bm_context(since="7d")`, `bm_search(since=…, valid_at, valid_overlaps, time_kind)`, `bm_project(command="recent", since="7d")` for recent activity, paged — [Basic Memory MCP reference](https://docs.basicmemory.com/raw/reference/mcp-tools-reference.md)
- Pre-1.0 Basic Memory `build_context(url="memory://…", depth=1, timeframe="7d")` and `recent_activity(type, depth, timeframe)`; timeframes accept "2 days ago", "last week", "yesterday", "today", "3 weeks ago" or "7d"; max timeframe 1 year; depth >3 may hurt performance (third-party mirrors) — [Glama: build_context](https://glama.ai/mcp/servers/basicmachines-co/basic-memory/tools/build_context); [Glama: recent_activity](https://glama.ai/mcp/servers/basicmachines-co/basic-memory/tools/recent_activity)
- Obsidian Copilot `getTimeRangeMs` handlers: "last/past N days|weeks|months|years" (end = start of today), "yesterday", "last/this/next week|month|year|quarter", "week of X", month names, quarters, 4-digit years (future dates shifted back a year), chrono-node fallback; returns epoch ms for `localSearch` — [TimeTools.ts](https://raw.githubusercontent.com/logancyang/obsidian-copilot/master/src/tools/TimeTools.ts)
- cyanheads `obsidian_get_note` reads periodic notes (daily through yearly, optional date) — [cyanheads/obsidian-mcp-server](https://github.com/cyanheads/obsidian-mcp-server)
- Copilot has dedicated daily-note tools (`ObsidianCliDailyTools.ts`, inferred from filename) — [GitHub src/tools](https://github.com/logancyang/obsidian-copilot/tree/master/src/tools)
- Bases: `file.mtime > now() - "1 week"`, `note.release_date - today()` — [Bases syntax](https://obsidian.md/help/bases/syntax)
- Timestamps, folder hierarchy and names are relevance signals for agents — [Anthropic: context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- MCP content annotations carry `lastModified` — [MCP spec 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)

### Inferences (Knotes)
- Add `since`/`until` (ISO or relative "7d", "today-30d") to `search`, `list_notes`, `list_tasks`, `query_notes`, resolved in Rust (no extra model step like Copilot's two-call pattern) (High/Low).
- Add `recent_activity(since="7d", group_by=day|folder)` returning changed/created notes with 1-line diffs or first lines; useful for "what did I work on last week" (Medium/Low). Knotes already reads "recent notes beside the task list" per the latest commit — formalise as a tool with paging.
- Treat daily notes specially: detect daily-note folder/format from Obsidian config (`.obsidian/daily-notes.json`) and allow `date`/range addressing; return date in results (Medium/Medium).
- Always put today's date (and weekday, timezone) in instructions and in date-related error messages.

### Gaps
- No primary source on how Mem, Notion or Apple Intelligence resolve relative dates.
- No evaluation data on time-filter accuracy for LLM agents.

## 6. Model-behaviour issues (stopping early, estimating instead of finding data, missing contradictions): known mitigations

### Takeaway
Current vendor guidance replaces generic "be thorough" with explicit contracts: retrieval budgets with stopping rules that never outrank correctness, an explicit trigger to keep retrieving for exhaustive/list questions, "absence of evidence is not a no", mandatory citations, defined "done" + verification, and investigate-before-answering; for contradictions, structured competing-hypothesis tracking, citation passes and per-source aggregation (MADAM-RAG) are the documented options.

### Cited Findings
- GPT-5.5 prompt guidance — retrieval budgets: "Retrieval budgets are stopping rules for search. They tell the model when enough evidence is enough."; "For ordinary Q&A, start with one broad search using short, discriminative keywords."; retrieve again when "The user asked for exhaustive coverage, a comparison, or a comprehensive list."; "Do not search again to improve phrasing, add examples, cite nonessential details" — [OpenAI prompt guidance (GPT-5.5)](https://developers.openai.com/api/docs/guides/prompt-guidance?model=gpt-5.5)
- Same: stopping conditions — "Resolve the user query in the fewest useful tool loops, but do not let loop minimization outrank correctness"; "After each result, ask: 'Can I answer the user's core request now with useful evidence and citations for the factual claims?'"; "Use the minimum evidence sufficient to answer correctly, cite it precisely, then stop." — [same](https://developers.openai.com/api/docs/guides/prompt-guidance?model=gpt-5.5)
- Same: "Absence of evidence shouldn't automatically become a factual 'no.'"; "For grounded answers, citation behavior should be part of the prompt. Define what needs support, what counts as enough evidence, and how the model should behave when evidence is missing." — [same](https://developers.openai.com/api/docs/guides/prompt-guidance?model=gpt-5.5)
- Same: preambles — "Before any tool calls for a multi-step task, send a short user-visible update", one or two sentences; use `phase: "commentary"` for intermediate updates and `phase: "final_answer"` for the answer — [same](https://developers.openai.com/api/docs/guides/prompt-guidance?model=gpt-5.5); [Simon Willison on GPT-5.5 guide](https://simonwillison.net/2026/Apr/25/gpt-5-5-prompting-guide/)
- OpenAI: start migrations to GPT-5.5 from "the smallest prompt that preserves the product contract", not carried-over prompt stacks — [Simon Willison quoting OpenAI](https://simonwillison.net/2026/Apr/25/gpt-5-5-prompting-guide/); [OpenAI: upgrading to GPT-5.5](https://developers.openai.com/api/docs/guides/upgrading-to-gpt-5p5)
- OpenAI reasoning guide: "For agentic or research work, define what 'done' means and how the model should verify its results"; avoid prescribing every intermediate step — [OpenAI reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
- GPT-5 guide [2025]: `<context_gathering>` early-stop criteria ("Top hits converge (~70%) on one area/path"; "If signals conflict or scope is fuzzy, run one refined parallel batch, then proceed"); persistence prompt ("keep going until the user's query is completely resolved", "Never stop or hand back to the user when you encounter uncertainty") — note the early-stop template is designed to *reduce* exploration, the opposite of what Knotes needs for list questions — [OpenAI GPT-5 prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)
- Anthropic: `<investigate_before_answering>` "Never speculate about code you have not opened… investigate and read relevant files BEFORE answering…"; "Before you finish, verify your answer against [test criteria]"; reflection prompt "After receiving tool results, carefully reflect on their quality and determine optimal next steps"; research prompt: "develop several competing hypotheses. Track your confidence levels… Regularly self-critique your approach"; "Encourage source verification: Ask Claude to verify information across multiple sources" — [Claude prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- Anthropic: some models (Opus 5) over-verify if told to self-check — remove such instructions there; i.e. verification prompts are model-specific — [same](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- Anthropic multi-agent: failure modes found by simulating agents step-by-step included continuing after sufficient results and preferring SEO content over authoritative sources → encode source-quality heuristics; separate CitationAgent; LLM-judge rubric (factual accuracy, citation accuracy, completeness, source quality, tool efficiency) on ~20 real queries — [Anthropic: multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)
- Building effective agents: agents should get "ground truth" from the environment at each step; evaluator-optimizer loop when criteria are clear [2024] — [Anthropic: building effective agents](https://www.anthropic.com/engineering/building-effective-agents)
- Capacities: task evaluator between plan phases; partial response on budget exhaustion — [Capacities dev blog](https://capacities.io/blog/ai-systems-at-capacities)
- Conflicting evidence: MADAM-RAG assigns each document to an agent and aggregates a debate; RAMDocs benchmark (ambiguity, misinformation, noise) top scores ~30–35% EM — conflicts remain hard (secondary summaries of arXiv 2504.13079) [2025] — [arXiv 2504.13079](https://arxiv.org/html/2504.13079v2); a follow-up reports a rule-only extractor scoring 1.0 on its bilingual variant, suggesting template regularity — [arXiv 2606.12903](https://arxiv.org/pdf/2606.12903)
- NotebookLM answers from sources even when sources don't fully address the question (review) — [Unite.AI](https://www.unite.ai/google-notebooklm-introduces-deep-research-feature/)

### Inferences (Knotes prompt/loop changes)
- **Completeness contract for list/overview questions** (High/Low): "If the user asks for all/what's left/which ones/how many, the answer must come from a tool that enumerates (list_tasks/query_notes) with total counts, not from one note; report the total and whether the list is complete; never estimate counts."
- **Primary-data rule** (High/Low): "Numbers, dates and statuses must be quoted from a note you read in this turn, with [[link]] citation; if not found, say so and what you checked" (combines GPT-5.5 citation rules + Anthropic investigate-before-answering + "absence of evidence ≠ no").
- **Stop rule per GPT-5.5** (Medium/Low): after each tool batch, ask "can I answer with cited evidence?"; for lookups allow 1–3 calls; for lists require an enumerating tool first.
- **Contradiction check** (Medium/Low-Medium): when ≥2 notes give a value for the same fact (date, amount, status), list both with dates and prefer the newer `mtime`/explicit date; implementable as a prompt rule plus returning `mtime` in search/read results so the model can see recency.
- **Final self-check step** (Medium/Low): optional, model-dependent (GPT-5.x benefits; avoid over-verification); can be a cheap separate low-effort call that checks the draft answer's claims against cited snippets (citation-agent pattern) — only for overview answers to bound latency.
- **Preambles/progress** (Low/Low): use `phase: "commentary"` text before long tool runs to make 30–55 s feel responsive.

### Gaps
- No quantified data on how much "completeness contract" or citation requirements reduce early-stopping in notes Q&A specifically; Knotes' own 30-question benchmark is the only way to measure.
- No primary source found on GPT-5.x specific tendency to "stop at the first plausible note"; mitigations above are general.

## 7. Reasoning-effort controls and their effect on agentic search quality and cost

### Takeaway
OpenAI now exposes `reasoning.effort` none/minimal/low/medium/high/xhigh/max (model-dependent; GPT-5.5 defaults to medium) and recommends `low` (not `none`) for tool use/search and `medium` for agentic research, warning that higher effort can cause over-searching; products surface this as fast/deep modes (NotebookLM Fast vs Deep Research, Khoj default vs research, Claude Code Explore quick/medium/very thorough, Copilot 4–8 iterations).

### Cited Findings
- Effort values: `none, minimal, low, medium, high, xhigh, max` (per model); `gpt-5.5` defaults to `medium`; GPT-5.6, GPT-6.1 Sol, GPT-6 Sol/Luna also default to medium; GPT-6 Astra rejects `none`, GPT-6.1 Sol rejects `none` and `minimal` — [OpenAI reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
- Guidance: `none` for latency-critical (voice, fast retrieval, classification; for gpt-5.5 try `low` first); `low` for "tool use, planning, search" and cost-sensitive work; `medium` default incl. agentic research; `high` for hard reasoning, compare with medium; `xhigh` for deep research/long agentic tasks only when evals show benefit — [same](https://developers.openai.com/api/docs/guides/reasoning)
- `configuration_update` item can change effort mid-conversation; `reasoning.mode` standard/pro (pro = more work, more cost) on GPT-5.6/GPT-6 — [same](https://developers.openai.com/api/docs/guides/reasoning)
- Reasoning tokens count toward `max_output_tokens` and are billed as output; reserve ≥25,000 tokens for reasoning+output initially; truncation yields `status: incomplete` possibly with no visible output — [same](https://developers.openai.com/api/docs/guides/reasoning)
- Keep reasoning across tool calls: pass reasoning items back with tool outputs; with `store:false`/ZDR, reasoning items carry `encrypted_content` by default (legacy `include: reasoning.encrypted_content` still accepted); `reasoning.context` auto/current_turn/all_turns (GPT-5.6 defaults to all_turns; earlier models current_turn); reusable only within same model family — [same](https://developers.openai.com/api/docs/guides/reasoning)
- GPT-5.5 guidance: "evaluate `low` before `none` when tool use, planning, search, or multi-step decision making still matters"; "higher effort can lead to overthinking, unnecessary searching, or output quality regressions" — [OpenAI prompt guidance (GPT-5.5)](https://developers.openai.com/api/docs/guides/prompt-guidance?model=gpt-5.5)
- GPT-5 guide [2025]: reusing reasoning via Responses API raised Tau-Bench Retail from 73.9% to 78.2% vs Chat Completions; `minimal` effort is more prompt-sensitive, needs explicit planning/persistence prompts — [OpenAI GPT-5 prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/gpt-5_prompting_guide)
- Anthropic: on BrowseComp, token usage explains ~80% of performance variance — more thinking/tool tokens = better research, at cost — [Anthropic: multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system); Claude effort parameter replaces `budget_tokens`; Opus 4.6 does more upfront exploration at higher effort, lower effort if too aggressive — [Claude prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices)
- Product modes: NotebookLM Fast Research vs Deep Research — [Unite.AI](https://www.unite.ai/google-notebooklm-introduces-deep-research-feature/); Khoj default vs research (FRAMES 42.0%→63.5%) — [Khoj blog](https://blog.khoj.dev/posts/evaluate-khoj-quality/); Claude Code Explore thoroughness levels — [Claude Code subagents](https://code.claude.com/docs/en/sub-agents); Copilot max iterations 4–8 — [newreleases.io](https://newreleases.io/project/github/logancyang/obsidian-copilot/release/3.0.0); GitHub added customizable reasoning level for Copilot cloud agent (changelog 2026-08-03; content not fetched) — [GitHub changelog](https://github.blog/changelog/2026-08-03-customize-the-reasoning-level-for-copilot-cloud-agent)

### Inferences (Knotes)
- **Route effort by question type** (Medium/Low): lookups → `low`; overview/list/compare → `medium` (maybe `high` behind a "deep" toggle); switch mid-conversation with `configuration_update` rather than restarting. Measure on the 30-question set: tool calls, latency, accuracy per effort.
- **Make sure reasoning items (encrypted_content) are replayed between steps** under `store:false`; check `reasoning.context` default for the model in use — loss of reasoning between tool calls makes the model re-plan and stop early (Tau-Bench +4.3 pts evidence).
- **"Deep" mode toggle** in UI (like NotebookLM/Khoj): raises effort and step budget and enables completeness contract/self-check; default stays fast.
- Reserve sufficient `max_output_tokens` (≥25k recommended starting point) to avoid empty `incomplete` responses on deep questions.

### Gaps
- No public numbers on GPT-5.5/5.6 search accuracy vs effort level for retrieval tasks; OpenAI only gives qualitative guidance.
- Exact effort/limits available through the ChatGPT-subscription ("Sign in with ChatGPT") path vs API keys not researched.
