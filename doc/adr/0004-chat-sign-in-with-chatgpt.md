# 0004. Chat through Sign in with ChatGPT and local function tools

Status: accepted, 2026-11-12 (details to verify before the chat stage)

## Context

The fourth column is a chat with an OpenAI model that answers questions about the
vault. The owner wants it to run on their ChatGPT Plus/Pro plan, without an API key.

"Sign in with ChatGPT" (OpenAI, autumn 2026) allows that for open-source and local
personal apps. Verified in the OpenAI cookbook "Sign in with ChatGPT" on 2026-11-12:

- OAuth authorization code flow with PKCE in the system browser, callback on a
  `127.0.0.1` loopback listener;
- dynamic client registration (`client_id=dynamic_agent_client`, no client secret);
- scope `chatgpt.tokens.use.direct` (plus `offline_access` for refresh), resource
  `https://api.openai.com/v1`;
- one host id (`urn:uuid:…`) per installation;
- requests go to the Responses API with the user's access token.

Not yet verified (must be checked in the OpenAI models-and-inference guide before the
chat stage): whether function tools are allowed on this route, `store=false`,
`previous_response_id`, embeddings, and the exact streaming behaviour.

## Decision

- The chat talks to the Responses API with the Sign in with ChatGPT token.
- The model gets narrow **function tools** (search titles, search content, read block
  range, list by tag/date, backlinks); the app executes them locally over the index and
  returns only their results. The model never sees the database file.
- The same tool layer is exposed to external agents by the MCP server, so in-app chat
  and external agents use one implementation.
- Tools return fragments with ids, not whole notes; reading is by block range.
- No raw SQL tool. If SQL is ever exposed, only through a read-only connection with an
  authorizer, a row limit and a timeout.
- Writes (create/update note) are proposals: the user sees a diff and confirms.
- Notes or tags can be marked "exclude from AI"; search tools filter them out.
- The full conversation, including tool calls and results, is resent on each step;
  old large tool results are trimmed.

## Consequences

- Usage is billed to the user's ChatGPT plan; usage limits surface as a
  "Manage usage" action.
- Found fragments leave the machine; the UI has to make that clear.
- If function tools turn out to be unavailable on this route, the decision is revisited
  (fallback: API key or OpenAI-compatible server).
