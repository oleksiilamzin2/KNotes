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
