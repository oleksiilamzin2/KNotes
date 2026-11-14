# Chat and editor UX, and AI-assisted writing/editing of notes with diffs, in leading AI note tools (state as of November 2026)

Scope note: primary sources are product docs, READMEs and source code (Obsidian Copilot v4 docs, Smart Composer source on GitHub, Notion Help, Apple Mac Help and Developer docs, Cursor docs, Claude Code docs, Craft support, Granola docs). Third-party claims are marked "(third-party)". Claims taken only from a search-result snippet, not a full page read, are marked "(snippet)". Version and maintenance status as checked on 2026-11-14:
- Obsidian Copilot: release 4.0.20 published 2026-11-13, with 4.0.21 canary on 2026-11-14. Actively developed. [GitHub releases](https://github.com/logancyang/obsidian-copilot/releases)
- Smart Connections: release 4.7.2 published 2026-08-06. [GitHub releases](https://github.com/brianpetro/obsidian-smart-connections/releases)
- Smart Composer: last commit 2026-01-27 (v1.2.9). The README says it is maintained by one developer and is "not under active development". [GitHub](https://github.com/glowingjade/obsidian-smart-composer), [commits](https://github.com/glowingjade/obsidian-smart-composer/commits/main)

Knotes baseline, for comparison: right-panel chat with a conversation list, streamed Markdown, tool-step lines, clickable note links, the open document as a removable chip, a model picker, Cmd+L focus and question cards. Tools are read-only today. Writing is planned after a git auto-commit stage, with diffs to confirm.

---

## 1. Context passing (selection, current note, multiple notes, folders/tags as scope)

### Takeaway
The leading pattern (Obsidian Copilot v4, Smart Composer, Cursor) has four parts:
- **Badges ("chips") above the composer** show exactly what will be sent with the next message.
- **The active note is auto-attached** and can be removed.
- **The selection becomes its own chip**, carrying file and line range.
- **An `@`/`[[` picker** adds notes, folders, URLs and files.

Folders and tags work as *scopes* for the agent to inspect. They are not bulk-pasted content. Knotes already has the doc chip. The cheapest high-value additions are a selection chip and an `@`/`[[` picker.

### Cited Findings

**Obsidian Copilot v4 (Agent Chat)**
- **"+" button:** opens "Add context", listing Active Note, Notes, Folders, Active Web Tab, other web tabs, and Files from your computer. — [Copilot docs: Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Typed syntax:** `[[` opens a picker for a note, PDF or canvas. `@` browses context categories; agent mentions sit under Agents and need Plus. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Folder context:** choosing a folder inserts `{Folder/path}`. "The agent inspects the folder but isn't given every file's contents." This is scope, not bulk paste. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Active note:** a new chat starts with an "Active Note" badge unless "Auto-attach active note" is off. Each message reports the active note *at send time*, so switching notes changes what the agent is pointed at. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Selection:** text selected in Reading or Edit view is added automatically "as you select it". Selections also come from PDFs, agent diff tabs and Excalidraw shapes. A command, "Add selection to chat context", can be bound to a hotkey. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Badges:** "Context badges above the composer show what will be sent with the next message." Each has an x to remove it. Selection badges show the start of the text, and hovering shows the full excerpt and its source. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Attachment lifetimes:** one-turn attachments clear after sending. Mentioned agents last for one multi-agent question. Project context applies to every new chat in that Project, and `AGENTS.md` to every Agent Chat in the vault or Project. Project context is "a focus aid, not a permission boundary." — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Note header icon:** a Copilot icon in each note's header opens Agent Chat with that note attached to the draft. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)
- **Drag and drop:** images and files can be dropped on the composer. Files from outside the vault are copied into the attachments folder, with one notice listing the copies. — [Context and Mentions](https://docs.obsidiancopilot.com/context-and-mentions/)

**Smart Composer (from source code)**
- **Context types:** `file`, `folder`, `vault`, `current-file`, `block` (selected content plus `startLine`/`endLine`), `url` and `image`. — [mentionable.ts](https://github.com/glowingjade/obsidian-smart-composer/blob/main/src/types/mentionable.ts)
- **Commands:** it registers "Open chat" and "Add selection to chat". — [main.ts](https://github.com/glowingjade/obsidian-smart-composer/blob/main/src/main.ts)
- **README features:** `@<fname>` mentions files and folders. Web links and YouTube transcripts are pulled in automatically. Images can be uploaded, dragged or pasted. `Cmd+Shift+Enter` runs a "Vault Search" (semantic retrieval) answer. — [Smart Composer README](https://github.com/glowingjade/obsidian-smart-composer)

**Cursor**
- **@-mention types:** Files and Folders (typing `/` after a folder goes deeper), Terminals, Chats (a previous conversation), Git diffs (`@Commit` for the working state, `@Branch` for the diff with main) and Browser. Images can be dragged in. — [Cursor docs: Mentions](https://cursor.com/docs/context/mentions)

**Notion AI**
- **Mentions:** @-mentions reference pages, people and dates in a query. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)
- **Shortcut:** `Shift+Cmd+J` opens Notion AI from anywhere, including outside Notion. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)
- **Other context:** files can be uploaded in chat. AI Connectors link Slack and Google Drive. AI blocks have a "Specified context" field. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)

**Reflect**
- **Search filters as scope:** "If you have any filters selected within the search, the chat function will call on those notes specifically." Tags, dates and backlinks can narrow it. (snippet; possibly from the Reflect Classic era) — [Reflect blog](https://reflect.app/blog/chat-with-your-notes)
- **Current product:** Reflect is now an open-source ("Reflect Open") Mac app on local Markdown files. AI is optional and uses your own API key. "Notes marked private are excluded from AI access." — [reflect.app](https://reflect.app)

**Others**
- **Google Docs Gemini:** selecting text "focuses Gemini's attention". Workspace Intelligence drafts from "your files, emails, chat, and the web" (April 22, 2026). — [Google Workspace Updates](https://workspaceupdates.googleblog.com/2026/04/new-gemini-capabilities-in-google-docs-help-you-go-from-blank-page-to-brilliance.html)
- **Apple Writing Tools Compose (ChatGPT):** asks you to choose "Include All Text or Selected Text" as scope. — [Apple Mac Help: Writing Tools](https://support.apple.com/guide/mac-help/mchldcd6c260/mac)
- **Granola recipes:** a recipe is either single-meeting or multiple-meeting. Multiple-meeting recipes appear on the home screen, in folders, in people and company views, or when several meetings are selected. — [Granola docs: Recipes](https://docs.granola.ai/help-center/getting-more-from-your-notes/recipes.md)
- **Craft Assistant:** works at document level and space level. — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)

### Inferences
- **Selection chip (high impact, low effort).** When the editor has a selection and the user presses Cmd+L (or a new "Ask about selection" command), add a chip with note path and line or heading range, the same shape as Smart Composer's `block`. Knotes' doc chip is half of this already.
- **Mention picker (high impact, medium effort).** Add `@`/`[[` to the question field for notes, folders and tags. Knotes' FTS5 search can back it. Folders and tags should become *scope hints for the read-only tools*, Copilot's `{Folder/path}` model. Pasting contents would break the 50k-note budget.
- **Freshness rule.** Re-read the active note at send time, as Copilot does, so the chip never sends a stale document.
- **Badge transparency** (hover shows the full excerpt and source) is cheap and builds trust before write tools arrive.

### Gaps
- I did not verify Cursor's Cmd+L / Cmd+Shift+L "add selection to chat" binding; the mentions page does not cover it.
- I found no primary docs for Mem's context passing, and none for Craft selection context.

---

## 2. Inline AI in the editor (slash commands, selection toolbars, continue writing, custom prompts, Apple Writing Tools, keyboard flows)

### Takeaway
Two layers dominate:
- **System-level Apple Writing Tools.** It is free on macOS, needs no model cost, and is automatic in `NSTextView`. Custom engines need `NSWritingToolsCoordinator`, for which objc2 has Rust bindings.
- **App-level "custom commands".** These are saved prompts with variables (`{selection}`, `{activeNote}`, folder, tag), run from `/`, the command palette or the right-click menu. Their results go to a small panel with Copy / Insert at cursor / Replace selection.

Obsidian Copilot stores these commands as Markdown files in the vault, which fits Knotes' Obsidian compatibility.

### Cited Findings

**Obsidian Copilot custom commands**
- **Triggers:** `/` in chat (in Agent Chat it inserts the prompt without sending, for review), the command palette ("Apply custom command", plus each command registered by name), and right-click → Copilot → command name (needs "Show in context menu"). — [Copilot docs: Custom commands](https://docs.obsidiancopilot.com/custom-commands/)
- **Template variables:** `{}` (the selection, or the active note if nothing is selected), `{activeNote}`, `{[[Note Title]]}`, `{Folder/Path}` and `{#tag1, #tag2}`. Tags must be in note properties. — [Custom commands](https://docs.obsidiancopilot.com/custom-commands/)
- **Result handling:** a result panel offers refine, copy, insert at cursor, or replace selection. Quick Ask offers Copy to clipboard, Insert at cursor and Replace selection, with follow-ups in the same panel. — [Custom commands](https://docs.obsidiancopilot.com/custom-commands/)
- **Replace safety:** "Replace selection" appears only when Copilot can safely identify the original text in the same note and pane. Quick Ask and Quick Command are unavailable in Source mode. — [Custom commands](https://docs.obsidiancopilot.com/custom-commands/)
- **Storage and models:** commands are Markdown files in `<Copilot folder>/copilot-custom-prompts/`. Each command can have its own model, and "Generate defaults" creates a starter set. — [Custom commands](https://docs.obsidiancopilot.com/custom-commands/)

**Other app-level inline AI**
- **Smart Composer:** prompt templates are created and reused by typing `/` in chat. A template can be created from any selected text in one click. — [Smart Composer README](https://github.com/glowingjade/obsidian-smart-composer)
- **Notion AI:**
  - Invoke by pressing space in a page, or by selecting text or blocks and choosing "Edit with AI". — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)
  - `/AI Block` inserts a regenerable AI block. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)
  - Inline results can be accepted, discarded or retried. Translate is under `•••` → More → Translate. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)
- **Reflect:**
  - AI covers "summarize text, and improve your writing", fixing grammar and spelling, and saved custom prompts. — [reflect.app](https://reflect.app)
  - The AI palette demo shows a prompt box with Re-run (R), Insert (I), Copy (C) and Replace (⌘↩) as shortcut hints. That these are single-key shortcuts is inferred from the demo. — [reflect.app](https://reflect.app)
- **Granola templates:** typing `/` in the note applies a meeting-type template. Switching templates afterward regenerates the enhanced notes. Granola's help pages conflict on whether a template can be picked during a meeting. (snippet) — [Granola blog](https://www.granola.ai/blog/meeting-recipes-repeatable-formats)
- **Text Generator plugin:** the README only says "Template Engine" and "Community Templates". I found no detail on triggers or insertion. — [Text Generator README](https://github.com/nhaouari/obsidian-textgenerator-plugin)

**Apple Writing Tools (user-facing, macOS 27 guide)**
- **Opening it:** select text, then Control-click → Writing Tools → Show Writing Tools. — [Apple Mac Help](https://support.apple.com/guide/mac-help/mchldcd6c260/mac)
- **Tools:** Describe a change, Rewrite (Friendly, Professional or Concise), Proofread, Summarize, Key Points, and Compose (through the ChatGPT extension). Writing Tools work "in most places you write, including third-party apps." — [Apple Mac Help](https://support.apple.com/guide/mac-help/mchldcd6c260/mac)
- **Proofread display:** changes are "underlined with a glowing line". You can view an explanation for each change, switch between versions, or revert. This is an inline per-change review pattern. (snippet of an earlier version of the same Apple page) — [Apple Mac Help](https://support.apple.com/guide/mac-help/mchldcd6c260/mac)
- **Siri AI (Beta):** the macOS 27 guide also lists Siri AI, which can compose, rewrite, proofread and give feedback. — [Apple Mac Help](https://support.apple.com/guide/mac-help/mchldcd6c260/mac)

**Apple Writing Tools (developer side)**
- **Custom text engines:** need `NSWritingToolsCoordinator`. You create it with a delegate and the view, supply `Context` objects holding copies of the text, and set `preferredResultOptions`. Its `state` reflects activity. Introduced in WWDC25 session 265, "Dive deeper into Writing Tools". The sample needs Xcode 16.4 and macOS 15.4. — [Apple sample: Enhancing your custom text engine with Writing Tools](https://developer.apple.com/documentation/appkit/enhancing-your-custom-text-engine-with-writing-tools.md), [WWDC25 265](https://developer.apple.com/videos/play/wwdc2025/265/)
- **Standard views:** "NSTextView, UITextView, and SwiftUI TextEditor … provide many features like spell checking and Writing Tools" automatically. — [Apple sample article](https://developer.apple.com/documentation/appkit/enhancing-your-custom-text-engine-with-writing-tools.md)
- **Rust bindings:** objc2-app-kit 0.3.2 includes `NSWritingToolsCoordinatorContext` and `NSWritingToolsCoordinatorState`. — [docs.rs objc2-app-kit](https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/struct.NSWritingToolsCoordinatorContext.html)
- **Known issue:** a developer forum thread reports a text-replacement issue reproducible in Apple's coordinator sample. — [Apple Developer Forums 821703](https://developer.apple.com/forums/thread/821703)

**Ulysses (native Markdown Mac editor) as a Writing Tools precedent**
- **Behavior:** it uses the system Writing Tools popover plus "Replace". Replacement keeps links, footnotes and images, but **code blocks and tables are ignored**. Writing Tools can be disabled via Edit > Writing Tools. — [Ulysses help: Writing Tools](https://help.ulysses.app/dive-into-editing/writing-tools)
- **Siri AI:** "Ask Siri" appears in context menus for sheets, groups and selected text, on macOS 27. Ulysses calls the integration limited "because Apple predefines the possible interactions". — [Ulysses help](https://help.ulysses.app/dive-into-editing/writing-tools)

### Inferences
- **Writing Tools first (high native-feel impact, low to medium effort).** If Knotes' editor is an `NSTextView` subclass, Writing Tools may already work. The work is then testing Markdown round-trips (code blocks, tables, wikilinks) and possibly `writingToolsBehavior`/result options to restrict to plain text. If the live-preview engine is custom, the objc2 coordinator bindings make it feasible. Either way it gives rewrite and proofread with no OpenAI cost.
- **Custom commands as Markdown files (medium effort).** Store them in the vault, run them via a slash menu in the chat field and via the editor's context menu, with three result actions (Copy / Insert / Replace selection). This is Knotes' first "write" path that needs no git or diff stage. It is a user-initiated, local, undoable insertion through the editor's normal undo stack.
- **Replace guard.** Adopt Copilot's rule: offer Replace only if the original selection text is still at the same place.

### Gaps
- I did not confirm Apple's exact `writingToolsBehavior` options; the developer page needs JavaScript.
- I found no primary source on "continue writing" or ghost-text autocomplete in the 2026 versions of these note apps.
- I found no source on how Writing Tools handle wikilinks or YAML front matter.

---

## 3. Acting on answers (copy, insert at cursor, create note, append to daily note, save conversation as note)

### Takeaway
The near-universal set of answer actions is Copy, Insert/Replace at cursor and Regenerate. Obsidian Copilot also auto-saves every chat as a Markdown note and offers an `obsidian://` link to it. Related-notes panels let the user drag a result into the editor as a link.

### Cited Findings
- **Obsidian Copilot Quick Chat:**
  - Response actions are Insert/Replace at cursor, Copy, Regenerate and Delete. User messages have Copy, Edit and Delete. — [Copilot docs: Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
  - Autosave is on by default. Each exchange is saved as Markdown under `<Copilot folder>/copilot-conversations/`. With autosave off, "Save chat as note" appears in the top bar. The note name template is configurable. — [Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
  - "Copy chat link" creates an `obsidian://` link to the chat's note. — [Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
- **Obsidian Copilot Agent Chat:** chats are saved as Markdown chat notes, with images under `copilot-conversations/attachments/`. — [Copilot docs: Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Quick Ask and Quick Command:** Copy, Insert at cursor, Replace selection. — [Custom commands](https://docs.obsidiancopilot.com/custom-commands/)
- **Reflect palette:** Re-run, Insert, Copy, Replace. — [reflect.app](https://reflect.app)
- **Smart Connections:** dragging a result into a note creates a link. Right-click copies all connection links. "Send to Smart Context" copies all connected content for use in an AI chat. — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
- **Copilot Relevant Notes:** dragging a title into the editor inserts a link ("a reference, not a copy"). "Add to chat" attaches a suggestion; suggestions are never auto-attached. — [Copilot docs: Relevant notes](https://docs.obsidiancopilot.com/relevant-notes/)
- **Granola:** recipes include a built-in follow-up email recipe; output is a chat answer. — [Granola docs: Recipes](https://docs.granola.ai/help-center/getting-more-from-your-notes/recipes.md)
- **Notion AI:** outputs such as spreadsheets, PDFs or slide decks can be downloaded and previewed. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)

### Inferences
- **Answer actions (high impact, low effort).** Add Copy (Markdown), Insert at cursor in the active tab, and "New note from answer". Each is a deterministic local action, so it needs no AI write tool or diff.
- **Save chat as Markdown (high value for Obsidian users, low effort).** Default it to a configurable folder. Decide whether that folder is excluded from search: Copilot-style autosave can pollute the vault and FTS results. Knotes' "clickable note links" already resolve, so saved chats keep working links.
- **Append to daily note (low effort).** No competitor documents it as a first-class button (see Gaps). It is a differentiator for daily-note users, built from Insert plus a daily-note path resolver.

### Gaps
- I found no primary source for an "append answer to daily note" action in any of the surveyed tools.
- I found no source for "create note from answer" as a dedicated button in Copilot or Smart Composer.

---

## 4. AI editing with diffs (apply flows, granularity, conflicts, undo, safety)

### Takeaway
Two families exist, plus a hybrid:
- **Propose → review → apply.** Examples: Smart Composer's Apply view with per-block Accept Incoming / Accept Current / Accept Both, Craft "Explore", Notion "Always ask", and Gemini in Docs, whose suggestions stay private until approved.
- **Apply → review → revoke.** Examples: Obsidian Copilot v4's "Files changed" card with per-stretch "Revoke", Craft "Execute" with one-step ⌘Z per response, and Cursor and Claude Code checkpoints.
- **Hybrid.** Mature tools also keep a durable history: Notion page history, Mem Version History, and git, which both Cursor and Claude Code say checkpoints do not replace.

User backlash at Cursor when per-change review was weakened suggests per-hunk review is valued. That evidence is forum titles only. Knotes' plan (git auto-commit plus diff confirmation) matches the hybrid. The open design choices are granularity (block or heading patches versus whole-file rewrite) and conflict handling.

### Cited Findings

**Smart Composer (propose → review → apply)**
- **The model's edit format:** a system prompt says the assistant's suggested edits arrive in `<smtcmp_block>` blocks using `<!-- ... existing content ... -->` elision markers. — [apply.ts](https://github.com/glowingjade/obsidian-smart-composer/blob/main/src/utils/chat/apply.ts)
- **Apply step:** a second LLM call then "rewrite[s] the entire markdown file with ONLY the changes from the specified markdown block applied." This is a whole-file rewrite by an apply model. — [apply.ts](https://github.com/glowingjade/obsidian-smart-composer/blob/main/src/utils/chat/apply.ts)
- **Speed-up:** it uses OpenAI Predicted Outputs for gpt-4o and gpt-4.1 family models. — [apply.ts](https://github.com/glowingjade/obsidian-smart-composer/blob/main/src/utils/chat/apply.ts)
- **Apply view:** a diff with prev/next navigation. Each changed block has "Accept Incoming", "Accept Current" and "Accept Both" (merge-conflict style). A top-level Accept writes the merged result via `app.vault.modify(file, newContent)`; Reject closes. — [ApplyViewRoot.tsx](https://github.com/glowingjade/obsidian-smart-composer/blob/main/src/components/apply-view/ApplyViewRoot.tsx)
- **README:** "suggests edits to your document … apply with a single click." — [Smart Composer README](https://github.com/glowingjade/obsidian-smart-composer)

**Obsidian Copilot v4 Agent (apply → review → revoke)**
- **Files changed card:** after a turn that edits the vault, the card lists each note with added and removed line counts and "new" or "deleted" badges. Clicking a row opens a tab with before and after: removed text struck through, added text highlighted. — [Copilot docs: Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **No per-change accept/reject.** Instead you hover a stretch and choose "Revoke", or "Revoke all" in the tab header. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Conflict rule:** "Revoke is unavailable once you or a later turn edits that stretch." The agent is not told about a revoke. Changes made via shell commands are not listed. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Navigation:** a change counter ("1 of 6") with arrows, a left-edge minimap, and an assignable "Go to next change in file diff" command. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Permissions:**
  - Modes are Safe, Plan and Auto, with support varying by agent backend. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
  - A "Permission required" card shows the proposed change or tool input, with temporary or persistent allow/deny. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
  - Saved rules are reviewable under "Edit approved rules". — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Older (v3.1.5, pre-v4, date not verified):** Composer had inline and side-by-side diff views with a toggle, plus a "composer auto-accept toggle". — [newreleases.io v3.1.5](https://newreleases.io/project/github/logancyang/obsidian-copilot/release/3.1.5)
- **Unmerged PR (not confirmed shipped):** PR #2394 proposed "collapsible per-line accept/reject … One click to accept the whole thing, one more click to pick lines." — [PR #2394 mirror](https://github-users.yhajoa.workers.dev/logancyang/obsidian-copilot/pull/2394)

**Craft Assistant (2026)**
- **Two modes:** "Explore" reads the document and proposes changes, and "nothing is modified until you approve". "Execute" applies changes directly, and edits appear in real time. — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)
- **Undo:** "⌘Z (Ctrl+Z) undoes the full last Assistant action as one step." — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)
- **Model limits:** only the Max and Fast models can edit. Core and On-device are read-only. — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)
- **Platforms:** Execute runs on macOS and iOS; Windows and Web are "coming later". — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)
- **Document Review:** gives feedback in four categories (Structure, Clarity, Completeness, Quality) and "does not apply changes directly". To act on it, you switch to Execute. — [Craft support: Document review](https://support.craft.do/en/ai-assistant/document-review.md)

**Notion AI**
- **Approval modes:** "Always ask" ("Nothing happens until you approve it"), "Review for me" (the default; it pauses only on flagged actions such as page or bulk deletions, and "won't always flag the same things you would"), and "Skip all approvals". The mode can't be changed while the agent runs. — [Notion Help: approvals](https://www.notion.com/help/choose-how-notion-agent-asks-for-your-approval)
- **Undo:** ask the agent to undo it, or open page history and go back to an earlier version. — [Notion Help: approvals](https://www.notion.com/help/choose-how-notion-agent-asks-for-your-approval)
- **Inline edits:** accept, discard, or try again. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)

**Google Docs Gemini (April 2026)**
- **Edits from a bottom bar or side panel.** "Gemini's suggested edits are only visible to you until you approve them." — [Google Workspace Updates, Apr 22 2026](https://workspaceupdates.googleblog.com/2026/04/new-gemini-capabilities-in-google-docs-help-you-go-from-blank-page-to-brilliance.html)
- **Shown as tracked changes** (third-party). — [TechRepublic](https://www.techrepublic.com/article/news-how-to-use-gemini-google-docs/)
- **July 2026:** Gemini generates suggested edits from reviewers' comments, "for you to review, approve, and seamlessly apply". — [Google Workspace Updates, Jul 2026](https://workspaceupdates.googleblog.com/2026/07/streamline-collaboration-in-google-docs-with-Gemini-powered-comment-workflows.html?linkId=63223433)

**Cursor**
- **Live diff:** "The diff view shows changes as they happen." Stop with Cmd+Shift+Backspace. "Agent Review → Find Issues" analyzes proposed edits line by line. — [Cursor docs: Review](https://cursor.com/docs/agent/review)
- **Checkpoints:** created automatically before significant changes. Clicking one previews the files, and restoring reverts files only, not messages. "Checkpoints are stored locally and separate from Git." — [Cursor docs: Checkpoints](https://cursor.com/docs/agent/chat/checkpoints)
- **User sentiment** (forum thread titles only; content not reviewed): "Bring back per-change Apply + inline diff review — you're throwing away your best UX advantage". — [Cursor forum](https://forum.cursor.com/t/bring-back-per-change-apply-inline-diff-review-you-re-throwing-away-your-best-ux-advantage/160856)
- **More such titles:** "[Regression] AI edits applying automatically without Diff/Approval UI" — [Cursor forum](https://forum.cursor.com/t/regression-ai-edits-applying-automatically-without-diff-approval-ui/154887); "Cursor keeps forcing auto-accept when in agent mode" — [Cursor forum](https://forum.cursor.com/t/cursor-keeps-forcing-auto-accept-when-in-agent-mode/157627/9)

**Claude Code**
- **Checkpoints:** one is captured before each prompt. Snapshots are kept for the 100 most recent checkpoints. — [Claude Code docs: Checkpointing](https://code.claude.com/docs/en/checkpointing)
- **Rewind:** `/rewind` or Esc Esc. Options are Restore code and conversation, Restore conversation, Restore code, Summarize from here, and Summarize up to here. After restoring the conversation, the original prompt goes back into the input "so you can re-send or edit it". — [Claude Code docs: Checkpointing](https://code.claude.com/docs/en/checkpointing)
- **Limitations:** Bash changes are not tracked, nor are external edits. It is "Not a replacement for version control". `/branch` forks a session. — [Claude Code docs: Checkpointing](https://code.claude.com/docs/en/checkpointing)

**Mem 2.0 (Oct 1, 2025; older)**
- "Chat can create, edit, and organize notes for you." Version History lets you revert AI changes. — [Mem blog](https://get.mem.ai/blog/mem-2-0)

**Third-party Obsidian diff plugins** (summary from search results only)
- **Drift:** a VSCode-style diff viewer with per-chunk accept/reject for external (AI) edits. — [Obsidian forum: Drift](https://forum.obsidian.md/t/new-plugin-drift-vscode-style-diff-viewer-for-ai-powered-obsidian-editing/113290)
- **In-place Diff View:** apply or revert edits through hover actions or bulk commands. — [Obsidian forum: In-place Diff View](https://forum.obsidian.md/t/in-place-diff-view/118103)

**Apple Proofread** is the system's inline per-change review: underline, explanation, and revert per change. — [Apple Mac Help](https://support.apple.com/guide/mac-help/mchldcd6c260/mac)

### Inferences

**Recommended model for Knotes.** Use propose → review → apply for chat-initiated edits, because Knotes users are deliberately careful (git stage first). Steps:
1. The model calls a write tool with **structured patches anchored to heading paths or block ranges**, with an expected-old-text check, not a whole-file rewrite. Smart Composer's whole-file rewrite needs a second model call and can touch unrelated text.
2. Knotes shows an **inline diff in the editor tab** (struck through and highlighted, as in Copilot and Apple Proofread) **plus a "Files changed" card in chat** with +/- counts.
3. Per-hunk Accept and Reject, plus Accept all and Reject all.
4. On accept, one git commit per AI action, with a message naming the conversation. **⌘Z undoes the whole AI action as one step**, as in Craft.

**Conflict handling (from Copilot and Smart Composer evidence).**
- Smart Composer computes the merged content from a snapshot and calls `vault.modify` with the whole file. From the code shown, nothing checks whether the file changed meanwhile, so the user's interim edits could be overwritten. This is an inference from code, not a documented bug.
- Copilot instead disables Revoke once a stretch was edited.
- For Knotes: re-verify the anchored old text at accept time. On mismatch, mark that hunk stale and offer to re-ask, never silently overwrite.

**Auto-accept.** Offer it later as an opt-in setting (as Copilot v3.1.5 and Notion's "Skip all approvals" do). The Cursor forum titles suggest forcing it is unpopular.

**Effort.** Patch tool, diff rendering and git commit form the large block. The diff review UI is the costliest part, roughly medium-high. Impact is high, since it unlocks all write features.

### Gaps
- I found no documented conflict handling for Notion, Craft or Gemini when the user edits during a pending suggestion.
- I found no primary detail on Notion's preview format for proposed edits.
- Smart Composer's README does not document per-block review; the findings come from source code only.

---

## 5. Meeting and voice capture (relevance for a notes app)

### Takeaway
Meeting capture is now table stakes in commercial AI note apps: Granola, Notion AI meeting notes, Mem Voice Mode, Reflect voice notes. The common design is a transcript, merged with the user's own sparse notes, through a template per meeting type, plus reusable "recipes" (saved prompts) in chat. On macOS 26+, Apple's on-device `SpeechAnalyzer` makes local long-form transcription feasible without a cloud service.

### Cited Findings
- **Granola recipes:** saved prompts invoked with `/` in a floating chat bar. They are grouped as Discover, My recipes and Workspace. Each is single-meeting or multiple-meeting. Recipes are static, with no variables, and can be shared privately, workspace-wide or by link. — [Granola docs: Recipes](https://docs.granola.ai/help-center/getting-more-from-your-notes/recipes.md)
- **Granola enhance and templates:** "Enhance Notes" combines pre-meeting context, live notes and the full transcript. Templates shape the enhanced notes per meeting type (1:1s, interviews, sales calls), with 29 built-in. (snippet) — [Granola blog](https://www.granola.ai/blog/meeting-recipes-repeatable-formats)
- **Notion AI:** transcribes meetings and summarizes notes, with settings for audio storage and transcript deletion. — [Notion AI FAQs](https://www.notion.com/help/notion-ai-faqs)
- **Mem Voice Mode:** "record, transcribe, and write meeting notes without a meeting bot". It turns spoken brain dumps into organized notes (Oct 2025). — [Mem blog](https://get.mem.ai/blog/mem-2-0)
- **Reflect:** voice notes are transcribed through a transcription provider you choose. AI lists key takeaways and action items from meeting notes. — [reflect.app](https://reflect.app)
- **Apple SpeechAnalyzer / SpeechTranscriber:** introduced at WWDC25 (session 277). Ships in macOS 26 and other 26 releases. On-device, with system-managed models. — [WWDC25 277](https://developer.apple.com/videos/play/wwdc2025/277/)
- **SpeechAnalyzer claims** (all third-party):
  - SFSpeechRecognizer had a roughly one-minute limit, which SpeechAnalyzer removes. — [issoh.co.jp](https://www.issoh.co.jp/tech/details/8929/)
  - The Yap CLI transcribed about 34 minutes of audio in about 45 seconds on macOS Tahoe. — [Gigazine](https://gigazine.net/gsc_news/en/20250619-apple-speech-analyzer)
  - Argmax reports a 2.12% word error rate on LibriSpeech test-clean. — [Argmax](https://www.argmaxinc.com/blog/apple-and-argmax)

### Inferences
- **Priority for Knotes: medium-low.** It is a personal Obsidian-compatible knowledge tool, not a meeting product.
- **The cheap, relevant parts:**
  - "Recipes", reusing the custom-commands mechanism from section 2.
  - A "voice note" command: record → `SpeechAnalyzer` on-device → transcript plus an AI summary into a new Markdown note, with the audio as an attachment.
- **Effort:** medium, since it needs Swift-only APIs bridged through objc2 or a small Swift shim.

### Gaps
- I did not confirm Granola's current template behavior during a live meeting; Granola's own help pages conflict.
- I did not verify whether `SpeechAnalyzer` is callable through objc2 (it may be Swift-only).

---

## 6. Discoverability (suggested questions, related notes, proactive surfacing)

### Takeaway
The proven pattern is a **live "related notes" panel** that follows the active note, or even the chat draft. Each entry shows a similarity score and offers drag-to-link and "add to chat". Smart Connections and Copilot Live Relevant Notes both rely on embeddings. Mem "Heads Up" pushes this into proactive surfacing while writing and before meetings. I found little primary documentation on suggested-question chips in note apps.

### Cited Findings
- **Smart Connections Connections view:**
  - Shows notes semantically related to the current note and "update[s] automatically when you change notes". Each result has a similarity score. — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
  - Controls: a pause button freezes results, results can be pinned or hidden, and dragging one into a note creates a link. — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
  - Embeddings are made by a built-in local model, "private and offline by default". — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
- **Connections Pro:** adds inline connections in the editor, plus a footer panel that updates *as you type*. — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
- **Copilot Live Relevant Notes:**
  - With Live on, suggestions follow your draft and the conversation; with it off, they stay tied to the editor note. A percentage shows similarity, "not a measure of factual accuracy". — [Copilot docs: Relevant notes](https://docs.obsidiancopilot.com/relevant-notes/)
  - Suggestions are never auto-attached. — [Copilot docs: Relevant notes](https://docs.obsidiancopilot.com/relevant-notes/)
  - Requires Copilot 4.0.8+ and the Miyo indexer. — [Copilot docs: Relevant notes](https://docs.obsidiancopilot.com/relevant-notes/)
- **Mem Heads Up:** notes resurface at relevant moments, "including while you write". Before a meeting it shows past history with that person (Oct 2025). — [Mem blog](https://get.mem.ai/blog/mem-2-0)
- **Smart Connections Lookup:** semantic search "based on meaning, not keywords", so an exact-match note may not appear. — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)

### Inferences
- **Lexical related notes first (medium impact, low-medium effort).** Knotes' decisions are SQLite + FTS5 + scan, with no embeddings. A cheap first step is a "Related" section beside Backlinks: BM25 over the active note's salient terms, plus shared tags and links. Embeddings can come later if quality demands it.
- **Add "add to chat" and drag-to-link** on Backlinks and Related entries. This connects existing panels to the chat at low cost.
- **Question cards.** Knotes' existing question cards are effectively suggested questions. Making them context-aware (from the active note's headings and open tasks) is a small step.

### Gaps
- I found no primary docs on suggested-question chips in Copilot, Smart Composer, Notion or Reflect.
- I found no benchmarks for related-notes latency on vaults of 10–50k notes.

---

## 7. Streaming and latency UX (reasoning summaries, tool progress, stop/regenerate, edit-and-resend, branching)

### Takeaway
State of the art:
- **Collapse tool calls and reasoning into one expandable activity row** with counts (Obsidian Copilot v4).
- **Stop replaces Send.** Queued messages return to the input on stop.
- **Queue or steer while the agent runs** (Cursor).
- **Edit a sent message**, keeping its original Markdown.
- **Branch "continue from here in a new chat"** instead of, or alongside, regenerate. Seen in Copilot, ChatGPT "Branch in new chat" and Claude Code `/branch`.

OpenAI exposes reasoning summaries only when requested, via `reasoning.summary`.

### Cited Findings
- **Obsidian Copilot v4 Agent Chat:**
  - Consecutive tool calls and reasoning collapse into "a compact activity row showing total commands, distinct files, and reasoning time"; open it for each step. Background tasks show as "2 tasks running", each with "Stop task". — [Copilot docs: Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
  - Send becomes "Stop" while a reply runs. Stopping returns unsent queued messages to the box. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
  - Sent messages have "Edit" (original Markdown kept). There is no regenerate in Agent Chat; "Continue from here in a new chat" branches instead. — [Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Obsidian Copilot Quick Chat:**
  - Has Regenerate. — [Copilot docs: Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
  - The view follows the growing answer. Scrolling up stops following, and a down-arrow button resumes. — [Copilot docs: Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
  - Enter sends by default; it can be changed to Shift+Enter. — [Copilot docs: Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
- **Cursor keyboard flows:** Enter queues a message while the agent works. Cmd+Enter sends immediately. Enter twice steers at the next tool call. Tab queues steering for after the turn. Cmd+K searches past conversations, and Cmd+F finds text in the open one. — [Cursor docs: Checkpoints](https://cursor.com/docs/agent/chat/checkpoints)
- **ChatGPT:**
  - Hover → "More actions" → "Branch in new chat" creates a new thread with context up to that message (reported around September 2025; third-party). — [Tom's Guide](https://www.tomsguide.com/ai/chatgpt-just-quietly-rolled-out-a-game-changing-upgrade-heres-why-im-already-obsessed-with-it)
  - Edited user messages keep versions behind a pager (community write-ups, third-party). — [nodea.ai](https://nodea.ai/blog/fork-chatgpt-conversation)
- **Claude Code:** rewinding puts the original prompt back in the input for edit-and-resend. `/branch` forks a session. — [Claude Code docs: Checkpointing](https://code.claude.com/docs/en/checkpointing)
- **OpenAI reasoning summaries** (relevant because Knotes uses OpenAI and Sign in with ChatGPT):
  - Request them with `reasoning.summary` = `auto`, `concise` or `detailed`. They are "omitted unless you explicitly request them" and returned in the `summary` array of the reasoning output item. — [OpenAI docs: Reasoning](https://developers.openai.com/api/docs/guides/reasoning)
  - Raw reasoning tokens are not exposed. Some latest models need organization verification. — [OpenAI docs: Reasoning](https://developers.openai.com/api/docs/guides/reasoning)
- **Craft:** a notification dot appears when a background session finishes. Sessions pause after 5 minutes of inactivity. — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)

### Inferences
- **Activity row (low effort, medium impact).** Knotes' tool-step lines can become one collapsible row ("Searched 3×, read 5 notes, 4 s"), Copilot-style, which cuts visual noise in long answers.
- **Reasoning summary (low-medium effort).** Request `reasoning.summary: "auto"` and stream it into a collapsed "Thinking" line. Check whether the ChatGPT-sign-in path supports it; see Gaps.
- **Edit-and-resend plus "Branch from here" (medium effort).** Branching suits notes well, because conversations become saved notes. Auto-follow scroll with a "jump to latest" button is a small polish item.

### Gaps
- I did not verify that reasoning summaries are available through the Sign in with ChatGPT / Codex-style auth path Knotes uses.
- I found no streaming event names for summaries on the fetched page.

---

## 8. Conversation management (search, pin, rename, export to note)

### Takeaway
Obsidian Copilot v4 sets the bar for note apps:
- Chats are Markdown notes in the vault.
- The sidebar groups chats as Pinned, Projects and Recents, with search, archive and bookmarks, multi-select, rename and delete.
- Each chat has an `obsidian://` deep link.

Cursor adds Cmd+K search across past chats and @-mentioning an old chat as context. Craft syncs sessions across devices and runs several in parallel.

### Cited Findings
- **Copilot Agent sidebar:** groups chats into Pinned, Projects and Recents, with search, archive and bookmarks. Shift-click and Cmd-click multi-select chats. "+" opens another chat tab, each with its own history, draft, attachments and queue. — [Copilot docs: Agent mode and tools](https://docs.obsidiancopilot.com/agent-mode-and-tools/)
- **Copilot Quick Chat "Chat history":** search saved conversations, reopen, rename, open the Markdown source, delete. — [Copilot docs: Chat interface](https://docs.obsidiancopilot.com/chat-interface/)
- **Copilot Projects:** give ongoing work "its own instructions, reusable context, and chat history". — [Copilot README](https://cdn.jsdelivr.net/gh/logancyang/obsidian-copilot@master/README.md)
- **Cursor:** Cmd+K searches past conversations. — [Cursor docs: Checkpoints](https://cursor.com/docs/agent/chat/checkpoints)
- **Cursor:** `@` Chats references a previous conversation. — [Cursor docs: Mentions](https://cursor.com/docs/context/mentions)
- **Craft:** sessions sync across devices, several can run in parallel, and history stays in the session list. — [Craft support: Editing](https://support.craft.do/en/ai-assistant/editing.md)
- **Granola:** recipe search covers titles and recipe text. — [Granola docs: Recipes](https://docs.granola.ai/help-center/getting-more-from-your-notes/recipes.md)

### Inferences
- **Index conversations in FTS5 (low effort, medium impact).** Knotes can index conversations in the same FTS5 store for search, and add pin and rename. Export is a single "Save as note" action, which also makes chats Obsidian-portable.
- **Deep links (low effort).** A `knotes://` deep link per chat, mirroring Copilot's "Copy chat link", lets notes link to the conversation that produced them. Useful provenance once AI writes are committed to git.

### Gaps
- I found no primary docs on conversation management for Notion AI chat, Smart Composer, Reflect or Mem in 2026.
- I did not check macOS system integration (Services menu, Shortcuts/App Intents) for any competitor except Ulysses' Siri AI context menu, which uses Apple-predefined intents on macOS 27 ([Ulysses help](https://help.ulysses.app/dive-into-editing/writing-tools)). The report writer should treat Services and Shortcuts exposure as unresearched.
