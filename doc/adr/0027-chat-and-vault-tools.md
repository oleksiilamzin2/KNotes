# 0027. The chat and its vault tools, in the app and through MCP

Status: accepted, 2026-11-14

## Context

ADR 0004 chose a chat on the owner's ChatGPT plan through Sign in with ChatGPT, with local
function tools over the index, shared with external agents through MCP, and left points to
check. The right panel's "ШІ" segment shows a stub (ADR 0023). The owner asked for the chat
and the tools behind it, and answered a question round (2026-11-14).

Checked in OpenAI's Sign in with ChatGPT documentation (developers.openai.com/siwc, ChatGPT
plan usage for open-source and local apps) on 2026-11-14:

- Authorization: `https://auth.openai.com/api/accounts/authorize` with `response_type=code`,
  PKCE (`S256`), `state`, `nonce`, a loopback `redirect_uri` on `127.0.0.1` (not
  `localhost`; only the port may change), scope `openid profile email offline_access
  resource.invoke chatgpt.tokens.use.direct`, `resource=https://api.openai.com/v1`. The first
  sign-in registers the app: `client_id=dynamic_agent_client`, `agent_name_hint` (the app's
  name), `ext_agent_host_id` (a stable `urn:uuid:` per host); the callback brings the issued
  `client_id` (`oaiapp_…`), saved and used from then on.
- Token endpoint: `https://auth.openai.com/api/accounts/oauth/token`, form-encoded; code
  exchange with the issued `client_id`, `code`, `code_verifier`, `redirect_uri`, `resource`;
  refresh with `grant_type=refresh_token`. Access tokens last an hour; refresh tokens 30
  days, renewed on each refresh (keep the latest, refresh one at a time). Plan usage needs the
  granted `chatgpt.tokens.use.direct` scope; the ID token's issuer is `https://auth.openai.com`
  and its audience the issued client.
- Inference: `POST https://api.openai.com/v1/responses` with the bearer token, `store:false`
  and `stream:true` on every request; the whole conversation goes in `input` each time
  (`previous_response_id` is not supported); no system message items (`instructions` or
  developer messages instead); no `temperature`, `max_output_tokens`, `metadata` and others.
  Models: `GET https://api.openai.com/v1/models`, entries with `visibility == "list"`.
- Tools: function tools work when grouped in a `namespace` (or given in `additional_tools`
  items); hosted tools (file search, code interpreter, image generation, hosted MCP,
  `tool_search`) do not.
- Errors: `subscription_sharing_usage_limit_exceeded` (429: pause, "Manage usage"),
  `…_usage_unavailable` and `…_user_unavailable` (503: retry with backoff), `…_invalid_user`
  (401), `…_user_not_eligible` (403); refresh failures (`invalid_grant`, `refresh_token_expired`,
  `refresh_token_reused`) mean signing in again. Credentials are not dropped for a network
  failure.
- The interface must have a "Continue with ChatGPT" button, a first-use notice ("You're using
  your ChatGPT plan"), "Using ChatGPT plan" with a "Manage usage" link
  (`https://chatgpt.com/settings/usage`) near the composer.

## Decision

- The tools read only (owner's choice): search notes and PDFs (fragments with their place),
  read a note's blocks or a PDF's pages, a document's outline, its links and backlinks, notes
  of a folder or a tag, the tags. Writing is a later stage.
- One tool layer, `knotes-tools`, used two ways: by the in-app chat as namespaced function
  tools, run in the app; and by an MCP server, `knotes-mcp` (stdio), for external agents,
  built in the same stage (owner's choice).
- Conversations: several, listed, kept per vault in Knotes's data beside the index (owner's
  choice); the server keeps none.
- The document open is offered to the conversation (named to the model, readable through the
  tools), shown under the composer, and can be removed (owner's choice).
- Sign-in follows the documentation above. The ID token comes straight from the token
  endpoint over TLS, so its claims (issuer, audience, expiry, nonce) are checked and its
  signature left to TLS, as OpenID Connect allows for that case. Tokens go to the macOS
  Keychain; the host id and the issued client id to Knotes's data.
- The HTTP client is `ureq` (blocking, on threads of the chat's own); streamed events are read
  line by line.

## Consequences

- Fragments the tools find leave the machine with the conversation; the panel says so.
- Usage counts against the owner's ChatGPT plan; the panel links to its usage settings.
- Everything but the live sign-in and the model's answers is tested without the network: a
  transport trait stands in for OpenAI.

## Built (2026-11-14): the tools and the MCP server

- `knotes-core` reads a document's pages behind `DocumentPages` (page count, a page's text, its
  outline); `knotes-pdf` does it with PDFKit.
- `knotes-tools` gives eight tools, each with a JSON schema: `search` (notes' blocks and PDFs'
  pages, a fragment of 400 characters at most with its heading and place), `find_by_title`
  (titles and aliases, listed again when older than 10 s), `read_note` (blocks from a given
  one, 60 at most), `read_pdf` (pages from a given one, 10 at most), `outline`, `links`
  (backlinks by note, and the document's own links), `list_notes` (of a folder or a tag, by
  name or date) and `list_tags`. A read stops at 24 000 characters, at a block's or page's end,
  and says where to go on. Notes are read from their files, so as fresh as the disk; search,
  titles, links and tags from the index, as fresh as Knotes left it.
- `knotes-mcp [vault]` serves them over stdio (JSON-RPC 2.0, one message a line: `initialize`,
  `ping`, `tools/list`, `tools/call`; a tool's result as JSON text, its failure with `isError`).
  It opens the vault's index read-only beside a running Knotes, which keeps it up to date;
  without a vault named, the last one Knotes opened. A vault Knotes never indexed is refused
  with a message saying so. `scripts/bundle.sh` puts it in `Knotes.app/Contents/MacOS`. In the
  50k vault, starting and answering a search takes about half a second.

## Built (2026-11-14): the chat's requests

- `knotes-chat` sends a turn: the document open as a developer message, the user's text,
  the tools in a `vault` namespace, `include: ["reasoning.encrypted_content"]`, the
  conversation's id as `prompt_cache_key`, `store: false`, `stream: true`. It shows the reply's
  text as it streams, runs the tools the model calls and sends their output back, until a reply
  without calls (16 at most). A reply counts once `response.completed` comes; `response.failed`,
  `response.incomplete`, an `error` event or a stream cut short fail the turn, which leaves the
  conversation as it was; so does stopping it.
- The conversation keeps the items to send back: messages as their text, function calls, and
  reasoning sealed for sending back, all without the ids the server keeps nothing for.
- A 401 renews the token once and sends again. Errors are sorted by what to do: the usage limit,
  unavailable for now, sign in again, not eligible, unsupported (with the parameter), others
  with their code and message.
- The models are `GET /v1/models`' `models` with `visibility: "list"` (`slug`, `display_name`).
- HTTP is `ureq` over the system's TLS and trusted roots (Security framework, no TLS library
  compiled in); a test over the network, left out by default, checks it reaches OpenAI.

## Built (2026-11-14): signing in

- A sign-in listens on a free loopback port (`http://127.0.0.1:<port>/auth/callback`), opens
  the authorization URL with fresh `state`, `nonce` and PKCE verifier: the first time with
  `dynamic_agent_client`, `agent_name_hint=Knotes` and the host id, later with the issued client,
  the last ID token and the email as hints. A request with another `state` is turned away and
  the wait goes on; `access_denied` is "declined"; the code is exchanged with the issued client;
  the ID token's issuer, audience, expiry and nonce are checked, and on a sign-in again its
  account must be the one before; without `chatgpt.tokens.use.direct` granted, the plan is "not
  enabled". The browser gets a page saying how it went. OpenAI's authorize endpoint takes the
  URL (checked by hand: it goes on to its log-in page).
- The host id (a random `urn:uuid:`), the issued client, the account and whether the first-use
  notice was shown are in `chatgpt.json` in Knotes's data; the tokens in the login Keychain
  (one generic password, `app.knotes.Knotes.chatgpt`), in memory for tests.
- An access token with less than five minutes left is refreshed before use, one refresh at a
  time, the new refresh token kept; an unusable refresh token (`invalid_grant` and the others)
  forgets the tokens and asks to sign in again; a network or server failure keeps them. Signing
  out revokes the refresh token at the endpoint OpenAI's discovery document names, then forgets
  the tokens (the registration stays, for signing in again).
- SHA-256 comes from CommonCrypto and random bytes from the Security framework; base64url and
  form encoding are a few lines here: no crates for them.

## Built (2026-11-14): the panel

- Signed out, the "ШІ" segment says what the chat is (an OpenAI model on the owner's ChatGPT
  plan; the fragments it finds go to OpenAI with the conversation) above "Continue with
  ChatGPT"; while the browser is out it says so, with a button that calls the sign-in off.
  Declined, plan not allowed and other failures are said under the button. The first sign-in
  shows the notice OpenAI asks for once ("Ви використовуєте свій план ChatGPT").
- Signed in: the conversations as a list at the top (a new one, not saved yet, first), a
  button for a new conversation and a menu (delete it; the account; sign out); the conversation;
  the question field (Return sends, Shift-Return a new line; the button stops a turn under way);
  under it the document open as a chip (a click crosses it out, a second offers it again; another
  document is offered anew), the model (from the plan's list, the last chosen kept in the
  settings), and "Використовується план ChatGPT" with "Керувати" opening ChatGPT's usage settings.
- The conversation shows the questions, each on a tinted rounded card across the column (the
  owner asked to tell their words from the chat's; the card is drawn by the question paragraph's
  own TextKit 2 fragment, so the conversation stays one text, selectable across), a dim line for
  each tool the model called ("Пошук: radar", "Читаю Radar.md"), and the answers set from their
  Markdown (bold, italic, code,
  headings, lists), redrawn at most every 50 ms while they stream. A wiki link in an answer opens
  its note or PDF (at its heading or page) in the window; a link to nothing opens nothing; web
  links open in the browser.
- A turn runs on the chat's own thread with a reader of the index of its own; a finished turn
  saves the conversation (named by its first question). A failed one says why (the usage limit
  with a "Керувати використанням" link) and puts the question back in the field; "sign in again"
  shows the sign-in.
- Long conversations (the owner asked for compression): each request sends reasoning only from
  the turn under way, and tool results before the last two turns cut to 1 500 characters (the
  model can call the tool again). Past 90 % of the model's window, the window taken no larger
  than 272 000 tokens, the turn first has the model summarize what lies before the
  last two turns (with the summary before, if any; tool results cut) in a request of its own,
  without tools; from then on the summary goes as a developer message in place of the items it
  covers. The conversation keeps every item and shows them all, with a line where the model's
  memory turns from the summary to the messages ("вище розмову стиснуто"); "Стискаю розмову…"
  shows while it summarizes. A summary made for a turn that then fails is kept.
- The threshold, checked on 2026-11-14 (the owner asked to follow the newest models; first
  60 000 was guessed): OpenAI's GPT-5.6 page gives a 1 050 000-token window, but "prompts with
  >272K input tokens are priced at 2x input and 1.5x output for the full request"; Codex, on the
  same ChatGPT plans, gives GPT-5.6 a 272K window and summarizes at 90 % of it (its
  `auto_compact_token_limit`, read from its source by others). So: 90 % of the window the plan's
  model list gives (`context_window`, when it does), never past 272K, which comes to 244 800
  tokens. The conversation's size is OpenAI's count of the last request's input (its `usage`)
  with what came back, kept with the conversation; an estimate (the JSON's bytes over four)
  stands in before the first reply and after a summary. The "…" menu shows it ("Розмова: 45 тис.
  з 244 тис. токенів").
- A request OpenAI answers as unavailable (503), or that the network fails before a reply
  starts, is sent again after 1 s and then 4 s; then the turn says so. A usage limit is never
  tried again.
- While the model thinks before its first words, "Думає…".
- An answer's code blocks sit on a grey card (each line's fragment draws its part, rounded at the
  block's first and last lines; the text keeps its line breaks, so copying it gives the code).
- View → "Запитати ШІ" (Cmd+L) unfolds the right panel if folded, switches it to the chat and
  puts the cursor in the question field.
- The probe signs in with a script of OpenAI's answers (`KNOTES_PROBE_CHAT`) and tokens in
  memory; without it the probe is signed out and never reads the Keychain.
